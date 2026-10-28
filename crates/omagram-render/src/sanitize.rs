//! Makes an engine's SVG safe to show and to share.
//!
//! The SVG is parsed and written out again with only known drawing
//! elements and attributes: no scripts, no event handlers, no
//! `foreignObject`, no animations (they can rewrite links), no comments
//! and processing instructions. Links may point inside the document
//! (`#id`); `<a>` may also open `http(s)` and `mailto` links, `<image>` may
//! hold an embedded raster image (`data:image/png;base64,…`). Everything
//! else, `javascript:` and `file:` above all, is dropped, as are CSS
//! `@import` and `url()`s that leave the document. Escapes and comments
//! are decoded before that check, and `image-set()` and `src()` count
//! as external references too.

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

#[derive(Debug)]
pub struct SanitizeError(String);

impl std::fmt::Display for SanitizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SanitizeError {}

/// The safe subset of `svg`. Fails if `svg` is not well-formed XML with an
/// `<svg>` root; a DTD is refused (no entity tricks).
pub fn sanitize(svg: &str) -> Result<String, SanitizeError> {
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
    let mut out = String::with_capacity(svg.len());
    write_element(root, true, &mut out);
    Ok(out)
}

fn is_svg(node: Node, name: &str) -> bool {
    node.tag_name().namespace() == Some(SVG_NS) && node.tag_name().name() == name
}

fn write_element(node: Node, root: bool, out: &mut String) {
    let name = node.tag_name().name();
    out.push('<');
    out.push_str(name);
    if root {
        let _ = write!(out, r#" xmlns="{SVG_NS}" xmlns:xlink="{XLINK_NS}""#);
    }
    for attr in node.attributes() {
        let local = attr.name();
        let prefix = match attr.namespace() {
            None => "",
            Some(XLINK_NS) => "xlink:",
            Some(XML_NS) => "xml:",
            Some(_) => continue,
        };
        if local.to_ascii_lowercase().starts_with("on") {
            continue;
        }
        let value = attr.value();
        let value = match local {
            "href" => match safe_href(name, value) {
                Some(v) => v.to_owned(),
                None => continue,
            },
            "style" => safe_css(value),
            _ => {
                if css_blocked(value) {
                    continue;
                }
                safe_css(value)
            }
        };
        let _ = write!(out, r#" {prefix}{local}="{}""#, escape(&value, true));
    }

    let children: Vec<Node> = node
        .children()
        .filter(|c| match c.node_type() {
            NodeType::Element => {
                c.tag_name().namespace() == Some(SVG_NS) && ELEMENTS.contains(&c.tag_name().name())
            }
            NodeType::Text => true,
            _ => false,
        })
        .collect();
    if children.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for child in children {
        match child.node_type() {
            NodeType::Element => write_element(child, false, out),
            _ => {
                let text = child.text().unwrap_or_default();
                if name == "style" {
                    out.push_str(&escape(&safe_css(text), false));
                } else {
                    out.push_str(&escape(text, false));
                }
            }
        }
    }
    let _ = write!(out, "</{name}>");
}

/// The link target if `element` may keep it.
fn safe_href<'a>(element: &str, value: &'a str) -> Option<&'a str> {
    let v = value.trim();
    let lower = v.to_ascii_lowercase();
    let ok = v.starts_with('#')
        || (element == "a"
            && ["http://", "https://", "mailto:"]
                .iter()
                .any(|p| lower.starts_with(p)))
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

/// CSS without `@import` and with external references replaced by `none`.
///
/// The original text is kept when decoding it changes nothing that has
/// to be removed, so a harmless backslash stays as it was written.
fn safe_css(css: &str) -> String {
    let normalized = normalize_css(css);
    let cleaned = neutralize_css(&normalized);
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
/// with `#` stays. `javascript:` and `expression(` clear the whole value.
fn neutralize_css(css: &str) -> String {
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
                if target.starts_with('#') {
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
    }

    #[test]
    fn filters_css() {
        let out = clean(
            r##"<style>@import url(http://x/a.css); rect { fill: url(https://x/p.svg#a); stroke: url(#ok) }</style><rect style="fill: url('file:///x')"/>"##,
        );
        assert!(!out.contains("@import"));
        assert!(!out.contains("://x/"));
        assert!(!out.contains("file:"));
        assert!(out.contains("url(#ok)"));
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
