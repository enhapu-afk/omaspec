//! Makes an engine's SVG safe to show and to share.
//!
//! The SVG is parsed and written out again with only known drawing
//! elements and attributes: no scripts, no event handlers, no
//! `foreignObject`, no animations (they can rewrite links), no comments
//! and processing instructions. Links may point inside the document
//! (`#id`); `<a>` may also open `http(s)` and `mailto` links, and a
//! relative path that stays in the picture's folder or below it (no `..`:
//! the engine does not know where the picture goes), or, for the links an
//! export lays between a project's diagrams, that goes up no further than
//! the project ([`Links`]); `<image>` may
//! hold an embedded raster image (`data:image/png;base64,…`). Everything
//! else, `javascript:` and `file:` above all, is dropped, as are CSS
//! `@import` and `url()`s that leave the document. Escapes and comments
//! are decoded before that check, and `image-set()` and `src()` count
//! as external references too. `xml:base` goes: it would move every
//! relative link the allowlist let through somewhere else.
//!
//! What is let through, usvg reads one call deeper for each level and for
//! each reference it follows (a `<use>`, a clip path, a mask, a marker, a
//! pattern's fill): elements nest no deeper than [`MAX_DEPTH`], and a
//! reference leads to an element that leads on to another no more than
//! [`MAX_CHAIN`] times in a row. A stylesheet's `url(#…)` could reach any
//! element, so it goes; a drawing element's own stays.

use std::collections::HashMap;
use std::fmt::Write as _;

use roxmltree::{Document, Node, NodeType, ParsingOptions};

const SVG_NS: &str = "http://www.w3.org/2000/svg";
const XLINK_NS: &str = "http://www.w3.org/1999/xlink";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

const ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "desc",
    "title",
    "metadata",
    "symbol",
    "use",
    "switch",
    "a",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "text",
    "tspan",
    "textPath",
    "image",
    "style",
    "linearGradient",
    "radialGradient",
    "stop",
    "pattern",
    "clipPath",
    "mask",
    "marker",
    "filter",
    "feBlend",
    "feColorMatrix",
    "feComponentTransfer",
    "feComposite",
    "feConvolveMatrix",
    "feDiffuseLighting",
    "feDisplacementMap",
    "feDistantLight",
    "feDropShadow",
    "feFlood",
    "feFuncA",
    "feFuncB",
    "feFuncG",
    "feFuncR",
    "feGaussianBlur",
    "feImage",
    "feMerge",
    "feMergeNode",
    "feMorphology",
    "feOffset",
    "fePointLight",
    "feSpecularLighting",
    "feSpotLight",
    "feTile",
    "feTurbulence",
];

/// How deep elements may nest. Omaspec's own pictures nest about ten
/// deep, nested states and a project's map included.
pub const MAX_DEPTH: usize = 64;

/// How many references may follow one another, each in an element the
/// one before it points to. Omaspec's pictures and PlantUML's have none;
/// a gradient that takes its stops from another takes two. With
/// [`MAX_DEPTH`] it keeps what usvg reads within a thread's 2 MiB of
/// stack in a debug build, where some 300 `<use>` in a row did not fit.
pub const MAX_CHAIN: usize = 8;

#[derive(Debug)]
pub struct SanitizeError(String);

impl std::fmt::Display for SanitizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SanitizeError {}

/// The safe subset of `svg`. Fails if `svg` is not well-formed XML with an
/// `<svg>` root, nests deeper than [`MAX_DEPTH`] or chains more than
/// [`MAX_CHAIN`] references; a DTD is refused (no entity tricks).
pub fn sanitize(svg: &str) -> Result<String, SanitizeError> {
    sanitize_with(svg, Links::default())
}

/// How far up from the picture's folder a relative link of an `<a>` may
/// lead: not at all for an engine's SVG, which does not know where the
/// picture goes; for an export's links between the diagrams of a project,
/// as many folders as the picture lies below the project's own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Links {
    pub up: usize,
}

/// [`sanitize`], with relative links that may go `links.up` folders up.
pub fn sanitize_with(svg: &str, links: Links) -> Result<String, SanitizeError> {
    let options = ParsingOptions {
        allow_dtd: false,
        ..Default::default()
    };
    let doc =
        Document::parse_with_options(svg, options).map_err(|e| SanitizeError(e.to_string()))?;
    let root = doc.root_element();
    if !is_svg(root, "svg") {
        return Err(SanitizeError("the root element is not <svg>".into()));
    }
    let mut writer = Writer {
        links,
        out: String::with_capacity(svg.len()),
        targets: HashMap::new(),
        refs: vec![Vec::new()],
        open: vec![0],
    };
    writer.element(root, 0)?;
    writer.chain()?;
    Ok(writer.out)
}

fn is_svg(node: Node, name: &str) -> bool {
    node.tag_name().namespace() == Some(SVG_NS) && node.tag_name().name() == name
}

/// Writes what is let through, and keeps where references lead.
struct Writer {
    links: Links,
    out: String,
    /// Each id the elements written carry, and those elements (by their
    /// place in [`Self::refs`]).
    targets: HashMap<String, Vec<usize>>,
    /// The ids each element with an id points to, it or what it holds;
    /// the first entry is the document's, every reference counts there.
    refs: Vec<Vec<String>>,
    /// The elements with an id being written, around the one at hand.
    open: Vec<usize>,
}

impl Writer {
    /// Writes `node`, `depth` elements below the root, and what it holds.
    fn element(&mut self, node: Node, depth: usize) -> Result<(), SanitizeError> {
        if depth >= MAX_DEPTH {
            return Err(SanitizeError(format!(
                "elements nest deeper than {MAX_DEPTH}"
            )));
        }
        let name = node.tag_name().name();
        let out = &mut self.out;
        out.push('<');
        out.push_str(name);
        if depth == 0 {
            let _ = write!(out, r#" xmlns="{SVG_NS}" xmlns:xlink="{XLINK_NS}""#);
        }
        let mut pointed = Vec::new();
        let mut id = None;
        for attr in node.attributes() {
            let local = attr.name();
            let prefix = match attr.namespace() {
                None => "",
                Some(XLINK_NS) => "xlink:",
                // How words are read. `xml:base` is not kept: a browser
                // reads each relative link against it.
                Some(XML_NS) if matches!(local, "space" | "lang") => "xml:",
                Some(_) => continue,
            };
            if local.to_ascii_lowercase().starts_with("on") {
                continue;
            }
            let value = attr.value();
            let value = match local {
                "href" => match safe_href(name, value, self.links) {
                    Some(v) => v.to_owned(),
                    None => continue,
                },
                "style" => safe_css(value, false),
                _ => {
                    if css_blocked(value) {
                        continue;
                    }
                    safe_css(value, false)
                }
            };
            match local {
                // A link is followed by whoever clicks it, not by the
                // drawing.
                "href" if name != "a" => {
                    pointed.extend(value.strip_prefix('#').map(str::to_owned));
                }
                "href" => {}
                "id" if prefix.is_empty() => id = Some(value.clone()),
                _ => pointed.extend(url_targets(&value)),
            }
            let _ = write!(out, r#" {prefix}{local}="{}""#, escape(&value, true));
        }
        let opened = id.map(|id| {
            let at = self.refs.len();
            self.refs.push(Vec::new());
            self.targets.entry(id).or_default().push(at);
            self.open.push(at);
        });
        for target in pointed {
            for &at in &self.open {
                self.refs[at].push(target.clone());
            }
        }
        self.children(node, name, depth)?;
        if opened.is_some() {
            self.open.pop();
        }
        Ok(())
    }

    /// Writes what `node`, the element `name`, holds, and closes it.
    fn children(&mut self, node: Node, name: &str, depth: usize) -> Result<(), SanitizeError> {
        let children: Vec<Node> = node
            .children()
            .filter(|c| match c.node_type() {
                NodeType::Element => {
                    c.tag_name().namespace() == Some(SVG_NS)
                        && ELEMENTS.contains(&c.tag_name().name())
                }
                NodeType::Text => true,
                _ => false,
            })
            .collect();
        if children.is_empty() {
            self.out.push_str("/>");
            return Ok(());
        }
        self.out.push('>');
        for child in children {
            match child.node_type() {
                NodeType::Element => self.element(child, depth + 1)?,
                _ => {
                    let text = child.text().unwrap_or_default();
                    if name == "style" {
                        self.out.push_str(&escape(&safe_css(text, true), false));
                    } else {
                        self.out.push_str(&escape(text, false));
                    }
                }
            }
        }
        let _ = write!(self.out, "</{name}>");
        Ok(())
    }

    /// Fails when references lead on from element to element more than
    /// [`MAX_CHAIN`] times in a row, or round in a circle. Each round
    /// lengthens every chain by one where it can, so the rounds end by
    /// the chain's length.
    fn chain(&self) -> Result<(), SanitizeError> {
        let mut hops = vec![0; self.refs.len()];
        loop {
            let mut longer = false;
            for (at, pointed) in self.refs.iter().enumerate() {
                let reach = pointed
                    .iter()
                    .filter_map(|id| self.targets.get(id))
                    .flatten()
                    .map(|&to| hops[to] + 1)
                    .max()
                    .unwrap_or(0);
                if reach > hops[at] {
                    if reach > MAX_CHAIN {
                        return Err(SanitizeError(format!(
                            "references lead on more than {MAX_CHAIN} times in a row"
                        )));
                    }
                    hops[at] = reach;
                    longer = true;
                }
            }
            if !longer {
                return Ok(());
            }
        }
    }
}

/// The ids `value` names in `url(#…)`, its escapes and comments decoded.
fn url_targets(value: &str) -> Vec<String> {
    let css = normalize_css(value);
    let lower = css.to_ascii_lowercase();
    let mut targets = Vec::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find("url(").map(|i| from + i) {
        let open = at + "url(".len();
        let close = css[open..].find(')').map_or(css.len(), |i| open + i);
        let target = css[open..close].trim().trim_matches(['"', '\'']).trim();
        if let Some(id) = target.strip_prefix('#') {
            targets.push(id.to_owned());
        }
        from = close;
    }
    targets
}

/// Whether `href` is a relative path: no scheme, not from the root, no
/// backslash, no query.
pub(crate) fn relative(href: &str) -> bool {
    let path = href.split('#').next().unwrap_or_default();
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', '?'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .next()
            .is_some_and(|first| !first.contains(':'))
}

/// A relative path that goes no more than `up` folders above the one it is
/// read from, its `..` written or escaped.
fn inside(href: &str, up: usize) -> bool {
    let path = href.split('#').next().unwrap_or_default();
    let decoded = path
        .to_ascii_lowercase()
        .replace("%2e", ".")
        .replace("%2f", "/");
    let mut depth: isize = 0;
    let floor = -isize::try_from(up).unwrap_or(isize::MAX);
    relative(href)
        && decoded.split('/').all(|part| {
            match part {
                "" | "." => {}
                ".." => depth -= 1,
                _ => depth += 1,
            }
            depth >= floor
        })
}

/// The link target if `element` may keep it.
fn safe_href<'a>(element: &str, value: &'a str, links: Links) -> Option<&'a str> {
    let v = value.trim();
    let lower = v.to_ascii_lowercase();
    let ok = v.starts_with('#')
        || (element == "a"
            && ["http://", "https://", "mailto:"]
                .iter()
                .any(|p| lower.starts_with(p)))
        || (element == "a" && inside(v, links.up))
        || (matches!(element, "image" | "feImage")
            && [
                "data:image/png",
                "data:image/jpeg",
                "data:image/jpg",
                "data:image/gif",
                "data:image/webp",
            ]
            .iter()
            .any(|p| lower.starts_with(p)));
    ok.then_some(v)
}

/// CSS without `@import` and with external references replaced by `none`;
/// in a stylesheet (`sheet`), the references into the document too.
///
/// The original text is kept when decoding it changes nothing that has
/// to be removed, so a harmless backslash stays as it was written.
fn safe_css(css: &str, sheet: bool) -> String {
    let normalized = normalize_css(css);
    let cleaned = neutralize_css(&normalized, sheet);
    if cleaned == normalized {
        css.to_owned()
    } else {
        cleaned
    }
}

/// `javascript:` or `expression(` after CSS decoding.
fn css_blocked(css: &str) -> bool {
    let normalized = normalize_css(css);
    contains_ci(&normalized, "javascript:") || contains_ci(&normalized, "expression(")
}

/// Decodes CSS escapes and drops comments. Text inside quotes stays
/// quoted, so a comment marker or an escape there is part of the string.
fn normalize_css(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                while let Some(inner) = chars.next() {
                    if inner == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                }
            }
            '"' | '\'' => {
                out.push(ch);
                while let Some(inner) = chars.next() {
                    if inner == '\\' {
                        push_escape(&mut chars, &mut out);
                        continue;
                    }
                    out.push(inner);
                    if inner == ch || inner == '\n' || inner == '\r' {
                        break;
                    }
                }
            }
            '\\' => push_escape(&mut chars, &mut out),
            _ => out.push(ch),
        }
    }
    out
}

/// The character produced by a CSS escape. The backslash is already
/// consumed. A trailing backslash stays; a backslash before a newline
/// produces nothing.
fn push_escape(chars: &mut std::iter::Peekable<std::str::Chars<'_>>, out: &mut String) {
    let Some(next) = chars.next() else {
        out.push('\\');
        return;
    };
    if matches!(next, '\n' | '\r' | '\u{000c}') {
        if next == '\r' && chars.peek() == Some(&'\n') {
            chars.next();
        }
        return;
    }
    if next.is_ascii_hexdigit() {
        let mut hex = String::new();
        hex.push(next);
        for _ in 0..5 {
            match chars.peek() {
                Some(digit) if digit.is_ascii_hexdigit() => hex.push(chars.next().unwrap()),
                _ => break,
            }
        }
        if matches!(
            chars.peek().copied(),
            Some(' ' | '\t' | '\n' | '\r' | '\u{000c}')
        ) {
            chars.next();
        }
        let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
        out.push(css_code_point(code));
        return;
    }
    out.push(next);
}

fn css_code_point(code: u32) -> char {
    match char::from_u32(code) {
        Some(ch) if code != 0 => ch,
        _ => '\u{FFFD}',
    }
}

/// Drops `@import` and replaces an external `url()`, `src()` or
/// `image-set()` with `none`. A `url()` or `src()` whose target starts
/// with `#` stays, but in a stylesheet (`sheet`), whose rules may lay it
/// on any element; so does an embedded `data:font/woff2` or
/// `data:font/ttf`. `javascript:` and `expression(` clear the whole value.
fn neutralize_css(css: &str, sheet: bool) -> String {
    if contains_ci(css, "javascript:") || contains_ci(css, "expression(") {
        return String::new();
    }
    let lower = css.to_ascii_lowercase();
    let mut out = String::with_capacity(css.len());
    let mut i = 0;
    while i < css.len() {
        let ch = css[i..].chars().next().unwrap();
        if ch == '"' || ch == '\'' {
            let end = end_of_string(css, i);
            out.push_str(&css[i..end]);
            i = end;
            continue;
        }
        if lower[i..].starts_with("@import") && !ident_continues(css, i + "@import".len()) {
            i = skip_statement(css, i + "@import".len());
            continue;
        }
        if let Some(name) = css_function(&lower, i) {
            let open = i + name.len() - 1;
            let Some(close) = matching_paren(css, open) else {
                break;
            };
            if name == "image-set(" {
                out.push_str("none");
            } else {
                let target = css[open + 1..close].trim().trim_matches(['"', '\'']);
                let embedded = target.to_ascii_lowercase();
                // A handwriting face embedded in an export. A data URL of
                // any other kind can carry a script, and goes.
                if (target.starts_with('#') && !sheet)
                    || embedded.starts_with("data:font/woff2")
                    || embedded.starts_with("data:font/ttf")
                {
                    out.push_str(&css[i..close + 1]);
                } else {
                    out.push_str("none");
                }
            }
            i = close + 1;
            continue;
        }
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn ident_continues(css: &str, index: usize) -> bool {
    css[index..]
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || !ch.is_ascii())
}

fn css_function(lower: &str, index: usize) -> Option<&'static str> {
    if index > 0
        && lower[..index].chars().next_back().is_some_and(|ch| {
            ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || !ch.is_ascii()
        })
    {
        return None;
    }
    ["image-set(", "url(", "src("]
        .into_iter()
        .find(|name| lower[index..].starts_with(name))
}

fn end_of_string(css: &str, start: usize) -> usize {
    let quote = css[start..].chars().next().unwrap();
    let mut index = start + quote.len_utf8();
    while index < css.len() {
        let ch = css[index..].chars().next().unwrap();
        index += ch.len_utf8();
        if ch == '\\' {
            if let Some(next) = css[index..].chars().next() {
                index += next.len_utf8();
            }
            continue;
        }
        if ch == quote || ch == '\n' || ch == '\r' {
            break;
        }
    }
    index
}

fn skip_statement(css: &str, mut index: usize) -> usize {
    while index < css.len() {
        let ch = css[index..].chars().next().unwrap();
        if ch == '"' || ch == '\'' {
            index = end_of_string(css, index);
            continue;
        }
        index += ch.len_utf8();
        if ch == ';' {
            break;
        }
    }
    index
}

/// The `)` that closes the `(` at `open`, skipping quotes and nested calls.
fn matching_paren(css: &str, open: usize) -> Option<usize> {
    let mut depth = 0;
    let mut index = open;
    while index < css.len() {
        let ch = css[index..].chars().next().unwrap();
        match ch {
            '"' | '\'' => index = end_of_string(css, index),
            '(' => {
                depth += 1;
                index += 1;
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
                index += 1;
            }
            _ => index += ch.len_utf8(),
        }
    }
    None
}

fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(needle)
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    find_ci(haystack, needle).is_some()
}

fn escape(s: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\n' if attribute => out.push_str("&#10;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(body: &str) -> String {
        let svg = format!(
            r#"<svg xmlns="{SVG_NS}" xmlns:xlink="{XLINK_NS}" width="10" height="10">{body}</svg>"#
        );
        sanitize(&svg).unwrap()
    }

    #[test]
    fn keeps_drawing() {
        let out = clean(
            r##"<g fill="url(#grad)"><rect x="1" y="2" width="3" height="4"/><text font-size="12">a &lt; b</text></g>"##,
        );
        assert!(out.contains(r##"<g fill="url(#grad)"><rect x="1" y="2" width="3" height="4"/>"##));
        assert!(out.contains("a &lt; b"));
    }

    #[test]
    fn drops_scripts_handlers_and_foreign_content() {
        let out = clean(
            r#"<script>alert(1)</script><rect onclick="alert(1)" onload="x"/><foreignObject><div/></foreignObject><animate attributeName="href" to="javascript:alert(1)"/>"#,
        );
        assert!(!out.contains("script"));
        assert!(!out.contains("onclick"));
        assert!(!out.contains("onload"));
        assert!(!out.contains("foreignObject"));
        assert!(!out.contains("animate"));
    }

    #[test]
    fn filters_links() {
        let out = clean(concat!(
            r#"<a xlink:href="javascript:alert(1)"><text>x</text></a>"#,
            r#"<a href="https://omarchy.org"><text>y</text></a>"#,
            r##"<use xlink:href="#shape"/><use href="file:///etc/passwd"/>"##,
            r#"<image href="/home/user/secret.png"/><image href="data:image/png;base64,AAAA"/>"#,
        ));
        assert!(!out.contains("javascript"));
        assert!(out.contains(r#"href="https://omarchy.org""#));
        assert!(out.contains(r##"xlink:href="#shape""##));
        assert!(!out.contains("passwd"));
        assert!(!out.contains("secret"));
        assert!(out.contains("data:image/png;base64,AAAA"));

        // A relative link stays in the picture's folder or below it.
        let out = clean(concat!(
            r#"<a href="docs/guide.html"><text>a</text></a>"#,
            r#"<a href="../outside.svg"><text>b</text></a>"#,
            r#"<a href="docs/%2E%2E/%2e%2e/up.svg"><text>c</text></a>"#,
            r#"<use href="docs/shape.svg"/>"#,
        ));
        assert!(out.contains(r#"href="docs/guide.html""#), "{out}");
        assert!(!out.contains("outside") && !out.contains("up.svg"), "{out}");
        assert!(!out.contains("shape.svg"), "only `<a>` leaves the document");

        // An export's links may go up to its project, no further.
        let svg = concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg">"#,
            r#"<a href="../shop.svg"><text>a</text></a>"#,
            r#"<a href="../../above.svg"><text>b</text></a>"#,
            r#"<a href="x/../../%2e%2e/escaped.svg"><text>c</text></a>"#,
            r#"<a href="../orders/checkout.svg"><text>d</text></a>"#,
            "</svg>",
        );
        let out = sanitize_with(svg, Links { up: 1 }).unwrap();
        assert!(out.contains(r#"href="../shop.svg""#), "{out}");
        assert!(out.contains(r#"href="../orders/checkout.svg""#), "{out}");
        assert!(!out.contains("above") && !out.contains("escaped"), "{out}");
    }

    #[test]
    fn a_base_does_not_move_the_links() {
        let svg = concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xml:base="https://example.invalid/" xml:lang="en">"#,
            r#"<a href="orders.svg" xml:base="https://example.invalid/b/"><text xml:space="preserve">a</text></a>"#,
            "</svg>",
        );
        let out = sanitize(svg).unwrap();
        assert!(!out.contains("base") && !out.contains("example"), "{out}");
        assert!(out.contains(r#"href="orders.svg""#), "{out}");
        assert!(out.contains(r#"xml:lang="en""#), "{out}");
        assert!(out.contains(r#"xml:space="preserve""#), "{out}");
    }

    /// `n` levels of `<g>` around a rect.
    fn nested(n: usize) -> String {
        format!(
            "{}<rect width=\"1\" height=\"1\"/>{}",
            "<g>".repeat(n),
            "</g>".repeat(n)
        )
    }

    #[test]
    fn nests_only_so_deep() {
        let svg = |n| format!(r#"<svg xmlns="{SVG_NS}">{}</svg>"#, nested(n));
        // The root and the rect are two levels of their own.
        assert!(sanitize(&svg(MAX_DEPTH - 2)).is_ok());
        let err = sanitize(&svg(MAX_DEPTH - 1)).unwrap_err();
        assert!(err.to_string().contains("nest"), "{err}");
        // Far deeper than a thread's stack would hold, in the writer.
        assert!(sanitize(&svg(100_000)).is_err());
    }

    /// `n` references in a row of `kind`, each element with `inner` in it.
    fn chain(kind: &str, n: usize, inner: &str) -> String {
        let link = |i: usize| -> String {
            let to = i - 1;
            match kind {
                "use" => format!(r##"<g id="c{i}"><use href="#c{to}"/>{inner}</g>"##),
                "clip" => {
                    format!(r#"<clipPath id="c{i}" clip-path="url(#c{to})">{inner}</clipPath>"#)
                }
                "mask" => format!(r#"<mask id="c{i}" mask="url(#c{to})">{inner}</mask>"#),
                "marker" => format!(
                    r#"<marker id="c{i}" markerWidth="2" markerHeight="2"><path d="M0 0 L1 1" marker-start="url(#c{to})"/>{inner}</marker>"#
                ),
                "pattern" => format!(
                    r#"<pattern id="c{i}" width="2" height="2" patternUnits="userSpaceOnUse"><rect width="1" height="1" fill="url(#c{to})"/>{inner}</pattern>"#
                ),
                _ => unreachable!(),
            }
        };
        let first = match kind {
            "use" => r#"<rect id="c0" width="1" height="1"/>"#.to_owned(),
            "clip" => r#"<clipPath id="c0"><rect width="5" height="5"/></clipPath>"#.to_owned(),
            "mask" => r#"<mask id="c0"><rect width="5" height="5" fill="white"/></mask>"#.to_owned(),
            "marker" => r#"<marker id="c0"><rect width="1" height="1"/></marker>"#.to_owned(),
            _ => r#"<pattern id="c0" width="2" height="2" patternUnits="userSpaceOnUse"><rect width="1" height="1"/></pattern>"#.to_owned(),
        };
        let links: String = (1..=n).map(link).collect();
        let user = match kind {
            "use" => format!(r##"<use href="#c{n}"/>"##),
            "clip" => format!(r#"<rect width="9" height="9" clip-path="url(#c{n})"/>"#),
            "mask" => format!(r#"<rect width="9" height="9" mask="url(#c{n})"/>"#),
            "marker" => format!(r#"<path d="M0 0 L5 5" marker-start="url(#c{n})"/>"#),
            _ => format!(r#"<rect width="9" height="9" fill="url(#c{n})"/>"#),
        };
        format!(
            r#"<svg xmlns="{SVG_NS}" width="10" height="10"><defs>{first}{links}</defs>{user}</svg>"#
        )
    }

    const CHAINS: [&str; 5] = ["use", "clip", "mask", "marker", "pattern"];

    #[test]
    fn references_lead_on_only_so_often() {
        for kind in CHAINS {
            // The first link does not lead on; the user leads to the last.
            assert!(sanitize(&chain(kind, MAX_CHAIN - 1, "")).is_ok(), "{kind}");
            let err = sanitize(&chain(kind, MAX_CHAIN, "")).unwrap_err();
            assert!(err.to_string().contains("in a row"), "{kind}: {err}");
            assert!(sanitize(&chain(kind, 10_000, "")).is_err(), "{kind}");
        }
        // Round in a circle.
        let circle = format!(
            r##"<svg xmlns="{SVG_NS}"><defs><clipPath id="a" clip-path="url(#b)"/><clipPath id="b" clip-path="url(#a)"/></defs><rect clip-path="url(#a)"/></svg>"##
        );
        assert!(sanitize(&circle).is_err());
        // Many references to one element are no chain, and a link is none.
        let many = format!(
            r##"<svg xmlns="{SVG_NS}"><defs><linearGradient id="g"/></defs>{}<a href="#g"><text>x</text></a></svg>"##,
            r#"<rect fill="url(#g)"/>"#.repeat(1000)
        );
        assert!(sanitize(&many).is_ok());
    }

    #[test]
    fn the_worst_it_lets_through_is_drawn_on_a_small_stack() {
        // Each link as deep as it may be: the root, the defs, the link and
        // the rect take four levels.
        let inner = nested(MAX_DEPTH - 5);
        let pictures: Vec<String> = CHAINS
            .iter()
            .map(|kind| {
                let svg = chain(kind, MAX_CHAIN - 1, &inner);
                sanitize(&svg).unwrap_or_else(|e| panic!("{kind}: {e}"))
            })
            .collect();
        // A thread of its own, so the stack is 2 MiB however the tests run.
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(move || {
                for svg in pictures {
                    let options = crate::export::Options::default();
                    crate::export::export(&svg, crate::export::Format::Png, &options).unwrap();
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn only_relative_paths_are_links() {
        for ok in [
            "orders.svg",
            "shop/orders.svg",
            "../orders.svg",
            "orders.svg#top",
        ] {
            assert!(relative(ok), "{ok}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "//host/x.svg",
            "https://example.org",
            "javascript:alert(1)",
            "file:///x",
            "a\\b.svg",
            "x.svg?y",
            "c:/x.svg",
        ] {
            assert!(!relative(bad), "{bad}");
        }
    }

    #[test]
    fn filters_css() {
        let out = clean(
            r##"<style>@import url(http://x/a.css); rect { fill: url(https://x/p.svg#a); stroke: url(#sheet) }</style><rect style="fill: url('file:///x'); stroke: url(#ok)"/>"##,
        );
        assert!(!out.contains("@import"));
        assert!(!out.contains("://x/"));
        assert!(!out.contains("file:"));
        assert!(out.contains("url(#ok)"));
        // A stylesheet's rule may lay a reference on any element.
        assert!(!out.contains("url(#sheet)"), "{out}");
    }

    #[test]
    fn a_handwriting_font_stays_and_other_data_urls_go() {
        let out = clean(
            r#"<style>@font-face{src:url("data:font/woff2;base64,QQ==")} text{fill:url("data:text/html,hi")}</style>"#,
        );
        assert!(out.contains("data:font/woff2;base64,QQ=="), "{out}");
        assert!(!out.contains("data:text/html"), "{out}");
        assert!(out.contains("none"), "{out}");
    }

    #[test]
    fn filters_css_escapes_comments_and_image_set() {
        let out = clean(concat!(
            r#"<style>@\69mport "https://evil.example/x.css"; "#,
            r#"rect { fill: \75rl(https://evil.example/a.png) }</style>"#,
            r#"<style>@im/**/port url(https://evil.example/b.css);</style>"#,
            r#"<style>a { content: "/*"; } @import url(https://evil.example/c.css); "#,
            r#"a { content: "*/"; }</style>"#,
            r#"<rect style="fill:image-set(url(https://evil.example/a.png) 1x, url(https://evil.example/b.png) 2x)" width="1" height="1"/>"#,
            r#"<rect style="stroke:src(https://evil.example/a.png)" width="1" height="1"/>"#,
            r##"<rect fill="url(#ok)" style="font-family: foo\bar"/>"##,
        ));
        let lower = out.to_ascii_lowercase();
        assert!(!lower.contains("@import"), "{out}");
        assert!(!lower.contains("\\69"), "{out}");
        assert!(!lower.contains("\\75"), "{out}");
        assert!(!lower.contains("evil.example"), "{out}");
        assert!(!lower.contains("image-set"), "{out}");
        assert!(!lower.contains("https:"), "{out}");
        assert!(out.contains("url(#ok)"), "{out}");
        assert!(out.contains(r"foo\bar"), "{out}");
    }

    #[test]
    fn refuses_what_is_not_svg() {
        assert!(sanitize("<html/>").is_err());
        assert!(sanitize("not xml").is_err());
        let dtd =
            r#"<!DOCTYPE svg [<!ENTITY x "y">]><svg xmlns="http://www.w3.org/2000/svg">&x;</svg>"#;
        assert!(sanitize(dtd).is_err());
    }

    #[test]
    fn output_is_well_formed_and_stable() {
        let once = clean(r#"<text x="1">"quoted" &amp; 'single'</text>"#);
        assert_eq!(sanitize(&once).unwrap(), once);
    }
}
