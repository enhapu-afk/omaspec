//! From the body's lines to a [`MindMap`] or a [`Wbs`], the way PlantUML's
//! commands read them: levels from the markers and their indentation,
//! `[#color]`, `_` for a box-less node, `:…;` for a label of several
//! lines. A line neither knows declines the diagram, naming the line.

use super::{Element, Idea, Map, MindMap, Node, NodeShape, Wbs};
use crate::Declined;
use crate::common::{Frame, Text, snippet};
use crate::source::{Line, starts_with_ci};

/// The deepest level drawn; the layout goes down the tree recursively.
const MAX_DEPTH: usize = 100;

/// Keywords every diagram type has that Omagram does not draw yet.
const NOT_YET: [&str; 8] = [
    "skin", "sprite", "hide", "show", "newpage", "rotate", "page", "<style>",
];

/// Parses a mind map from the lines [`crate::source::body`] gives.
pub fn parse_mindmap(lines: &[Line]) -> Result<MindMap, Declined> {
    let mut frame = Frame::default();
    let mut p = MindMapParser::default();
    let mut at = 0;
    while at < lines.len() {
        at = match frame.line(lines, at)? {
            Some(next) => next,
            None => p.line(lines, at)?,
        };
    }
    Ok(MindMap {
        maps: p.maps.into_iter().map(|m| m.map).collect(),
        vertical: p.vertical,
        frame,
    })
}

/// Parses a WBS diagram from the lines [`crate::source::body`] gives.
pub fn parse_wbs(lines: &[Line]) -> Result<Wbs, Declined> {
    let mut frame = Frame::default();
    let mut p = WbsParser::default();
    let mut at = 0;
    while at < lines.len() {
        at = match frame.line(lines, at)? {
            Some(next) => next,
            None => p.line(lines, at)?,
        };
    }
    let root = p.root.ok_or_else(|| Declined {
        line: None,
        reason: "a WBS diagram needs a node".into(),
    })?;
    Ok(Wbs { root, frame })
}

#[derive(Default)]
struct MindMapParser {
    maps: Vec<MapState>,
    vertical: bool,
    /// Where new ideas go when their line does not say: `left side` and
    /// `right side` switch it.
    left_by_default: bool,
    /// The markers of the first `*` line, indentation included: PlantUML
    /// measures the indentation of later lines against it.
    first: Option<String>,
}

struct MapState {
    map: Map,
    /// The level of the first idea after the root; every level is a
    /// multiple of it (two spaces of indentation are one level).
    multiplier: usize,
    /// The last idea on each side, as indexes from the root: a new idea
    /// hangs below it or below one of its ancestors.
    last: [Vec<usize>; 2],
}

impl MindMapParser {
    fn line(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let line = lines[at];
        let Line { no, text, .. } = line;
        let lower = text.to_ascii_lowercase();
        let squeezed: Vec<&str> = lower.split_whitespace().collect();
        match squeezed.as_slice() {
            ["top", "to", "bottom", "direction"] => {
                self.vertical = true;
                return Ok(at + 1);
            }
            ["left", "to", "right", "direction"] => {
                self.vertical = false;
                return Ok(at + 1);
            }
            _ => {}
        }
        if let Some((mut node, level, next)) = multiline(lines, at) {
            node.text.value = label_mindmap(&node.text.value);
            self.add(node, level, !self.left_by_default, no)?;
            return Ok(next);
        }
        if let Some(after) = text.strip_prefix(['*', '#']) {
            let count = 1 + after.len() - after.trim_start_matches(['*', '#']).len();
            let (markers, rest) = text.split_at(count);
            let (color, shape, rest) = mindmap_decoration(rest);
            let typed = format!("{}{markers}", " ".repeat(line.indent));
            let level = self.smart_level(&typed, no)?;
            let node = node(label_mindmap(rest.trim_start()), shape, color, no);
            self.add(node, level, !self.left_by_default, no)?;
            return Ok(at + 1);
        }
        if let Some(rest) = text.strip_prefix('0') {
            let node = node(label_mindmap(rest.trim_start()), NodeShape::Box, None, no);
            self.add(node, 0, true, no)?;
            return Ok(at + 1);
        }
        if text.starts_with(['+', '-']) {
            if line.indent > 0 {
                return Err(Declined::at(
                    no,
                    "a `+` or `-` line must start at the beginning of the line",
                ));
            }
            let count = text.len() - text.trim_start_matches(['+', '-']).len();
            let (markers, rest) = text.split_at(count);
            let (color, shape, rest) = mindmap_decoration(rest);
            let node = node(label_mindmap(rest.trim_start()), shape, color, no);
            self.add(node, count - 1, !markers.contains('-'), no)?;
            return Ok(at + 1);
        }
        if let Some(left) = side(&lower) {
            self.left_by_default = left;
            return Ok(at + 1);
        }
        Err(not_drawn(text, no, "a mind map"))
    }

    /// PlantUML's level of an org-mode line from its markers with their
    /// indentation, measured against the first such line; `**` and more
    /// ignore the indentation.
    fn smart_level(&mut self, typed: &str, no: usize) -> Result<usize, Declined> {
        let first = self.first.get_or_insert_with(|| typed.to_owned());
        let typed = if typed.ends_with("**") {
            typed.trim()
        } else {
            typed
        };
        smart_level(typed, first, no)
    }

    fn add(&mut self, node: Node, level: usize, right: bool, no: usize) -> Result<(), Declined> {
        let Some(state) = self.maps.last_mut().filter(|_| level > 0) else {
            // The first idea and every one of level 0 start a map.
            self.maps.push(MapState {
                map: Map {
                    root: node,
                    right: Vec::new(),
                    left: Vec::new(),
                },
                multiplier: 0,
                last: [Vec::new(), Vec::new()],
            });
            return Ok(());
        };
        if state.multiplier == 0 {
            state.multiplier = level;
        }
        if !level.is_multiple_of(state.multiplier) {
            return Err(Declined::at(
                no,
                "the indentation is not a multiple of the first level's",
            ));
        }
        let level = level / state.multiplier;
        if level > MAX_DEPTH {
            return Err(Declined::at(no, too_deep()));
        }
        let side = usize::from(!right);
        let path = &mut state.last[side];
        if level > path.len() + 1 {
            return Err(Declined::at(no, "this idea skips a level"));
        }
        path.truncate(level - 1);
        let mut list = if right {
            &mut state.map.right
        } else {
            &mut state.map.left
        };
        for &i in path.iter() {
            list = &mut list[i].children;
        }
        list.push(Idea {
            node,
            children: Vec::new(),
        });
        path.push(list.len() - 1);
        Ok(())
    }
}

#[derive(Default)]
struct WbsParser {
    root: Option<Element>,
    /// The last element, as sides and indexes from the root.
    last: Vec<(bool, usize)>,
    /// The markers of the root's line, indentation included.
    first: Option<String>,
}

impl WbsParser {
    fn line(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let line = lines[at];
        let Line { no, text, .. } = line;
        if text.starts_with(['*', '+', '-']) {
            let count = text.len() - text.trim_start_matches(['*', '+', '-']).len();
            let (markers, rest) = text.split_at(count);
            if let Some(d) = wbs_decoration(rest)
                && (d.rest.is_empty() || d.rest.starts_with(char::is_whitespace))
            {
                let typed = format!("{}{markers}", " ".repeat(line.indent));
                let label = d.rest.trim_start();
                let label = quoted_as_code(label).unwrap_or(label);
                self.item(&typed, d, Some(label).filter(|l| !l.is_empty()), no)?;
                return Ok(at + 1);
            }
        }
        if let Some((node, typed, d, next)) = wbs_multiline(lines, at) {
            self.add_node(&typed, d.left, node, no)?;
            return Ok(next);
        }
        if is_link(text) {
            return Err(Declined::at(
                no,
                "arrows between WBS nodes are not supported yet",
            ));
        }
        Err(not_drawn(text, no, "a WBS diagram"))
    }

    fn item(
        &mut self,
        typed: &str,
        d: WbsDecoration,
        label: Option<&str>,
        no: usize,
    ) -> Result<(), Declined> {
        let shape = match (label, d.shape) {
            (None, NodeShape::Plain) => NodeShape::Pseudo,
            (None, _) => return Err(Declined::at(no, "a WBS node needs a label")),
            (Some(_), shape) => shape,
        };
        let label = label.map_or(String::new(), |l| label_wbs(l).to_owned());
        let node = node(label, shape, d.color.map(str::to_owned), no);
        self.add_node(typed, d.left, node, no)
    }

    fn add_node(
        &mut self,
        typed: &str,
        left: Option<bool>,
        node: Node,
        no: usize,
    ) -> Result<(), Declined> {
        let left = left.unwrap_or(typed.contains('-'));
        let Some(root) = self.root.as_mut() else {
            self.first = Some(typed.to_owned());
            self.root = Some(Element {
                node,
                left: Vec::new(),
                right: Vec::new(),
            });
            return Ok(());
        };
        let level = smart_level(typed, self.first.as_deref().unwrap_or_default(), no)?;
        if level == 0 {
            return Err(Declined::at(no, "a WBS diagram has only one root"));
        }
        if level > MAX_DEPTH {
            return Err(Declined::at(no, too_deep()));
        }
        if level > self.last.len() + 1 {
            return Err(Declined::at(no, "this node skips a level"));
        }
        self.last.truncate(level - 1);
        let mut parent = root;
        for &(on_left, i) in &self.last {
            parent = if on_left {
                &mut parent.left[i]
            } else {
                &mut parent.right[i]
            };
        }
        let element = Element {
            node,
            left: Vec::new(),
            right: Vec::new(),
        };
        // Below the root everything is one row; a left one goes first.
        let place = if level == 1 && left {
            parent.right.insert(0, element);
            (false, 0)
        } else if left {
            parent.left.push(element);
            (true, parent.left.len() - 1)
        } else {
            parent.right.push(element);
            (false, parent.right.len() - 1)
        };
        self.last.push(place);
        Ok(())
    }
}

fn node(label: String, shape: NodeShape, color: Option<String>, no: usize) -> Node {
    Node {
        text: Text {
            value: label,
            line: no,
        },
        shape,
        color,
        ink: None,
    }
}

/// PlantUML's `getSmartLevel`: the level of a line from its markers and
/// their indentation, against the markers of the first line.
fn smart_level(typed: &str, first: &str, no: usize) -> Result<usize, Declined> {
    let len = typed.len();
    if !typed.contains(' ') {
        Ok(len - 1)
    } else if typed.ends_with(first) {
        Ok(len - first.len())
    } else if typed.trim().len() == 1 {
        Ok(len - 1)
    } else if typed.starts_with(first) {
        Ok(len - first.len())
    } else {
        Err(Declined::at(
            no,
            "this indentation does not match the first line's",
        ))
    }
}

/// `[#color]` and `_` after a mind map line's markers, as PlantUML takes
/// them: the color first. With the rest of the line.
fn mindmap_decoration(text: &str) -> (Option<String>, NodeShape, &str) {
    let (color, rest) = match bracket_color(text) {
        Some((color, rest)) => (Some(color.to_owned()), rest),
        None => (None, text),
    };
    match rest.strip_prefix('_') {
        Some(rest) => (color, NodeShape::Plain, rest),
        None => (color, NodeShape::Box, rest),
    }
}

/// `[#word]` at the start of `text`: the color and the rest.
fn bracket_color(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_prefix("[#")?;
    let end = inner.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    if end == 0 || !inner[end..].starts_with(']') {
        return None;
    }
    Some((&text[1..end + 2], &inner[end + 1..]))
}

/// `(code)` at the start of `text`: the code and the rest.
fn paren_code(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_prefix('(')?;
    let end = inner.find(|c: char| !(c.is_alphanumeric() || c == '_'))?;
    if end == 0 || !inner[end..].starts_with(')') {
        return None;
    }
    Some((&inner[..end], &inner[end + 1..]))
}

/// What may follow a WBS line's markers: `_` and `<` or `>` in either
/// order, `[#color]` and `(code)` in either order, the two groups in
/// either order too.
struct WbsDecoration<'a> {
    shape: NodeShape,
    /// `<` is `Some(true)`, `>` is `Some(false)`.
    left: Option<bool>,
    color: Option<&'a str>,
    rest: &'a str,
}

fn wbs_decoration(text: &str) -> Option<WbsDecoration<'_>> {
    let mut d = WbsDecoration {
        shape: NodeShape::Box,
        left: None,
        color: None,
        rest: text,
    };
    // Which group each token belongs to, in order: 0 for shape and
    // direction, 1 for color and code.
    let mut groups: Vec<u8> = Vec::new();
    let mut code = false;
    loop {
        let rest = d.rest;
        if let Some(after) = rest.strip_prefix('_')
            && d.shape == NodeShape::Box
        {
            d.shape = NodeShape::Plain;
            d.rest = after;
            groups.push(0);
        } else if let Some(after) = rest.strip_prefix(['<', '>'])
            && d.left.is_none()
        {
            d.left = Some(rest.starts_with('<'));
            d.rest = after;
            groups.push(0);
        } else if let Some((color, after)) = bracket_color(rest)
            && d.color.is_none()
        {
            d.color = Some(color);
            d.rest = after;
            groups.push(1);
        } else if let Some((_, after)) = paren_code(rest)
            && !code
        {
            code = true;
            d.rest = after;
            groups.push(1);
        } else {
            break;
        }
    }
    let switches = groups.windows(2).filter(|w| w[0] != w[1]).count();
    (switches <= 1).then_some(d)
}

/// `"label" as CODE`: the label without quotes.
fn quoted_as_code(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('"')?;
    let close = inner.rfind('"')?;
    let after = inner[close + 1..].trim_start();
    if after.len() == inner[close + 1..].len() {
        return None;
    }
    let word = after.get(..2)?;
    let code = after[2..].strip_prefix(char::is_whitespace)?.trim_start();
    (word.eq_ignore_ascii_case("as")
        && !code.is_empty()
        && code.chars().all(|c| c.is_alphanumeric() || c == '_'))
    .then(|| &inner[..close])
}

/// A label without the `<<stereotype>>` at the end of its last line,
/// which only styles pick up.
fn label_mindmap(label: &str) -> String {
    let start = [
        label.rfind("\\n").map(|i| i + 2),
        label.rfind('\n').map(|i| i + 1),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(0);
    match stereotype_at(&label[start..]) {
        Some(at) => label[..start + at].to_owned(),
        None => label.to_owned(),
    }
}

fn label_wbs(label: &str) -> &str {
    match stereotype_at(label) {
        Some(at) => label[..at].trim(),
        None => label,
    }
}

/// Where a `<<…>>` that ends `text` starts.
fn stereotype_at(text: &str) -> Option<usize> {
    let at = text.find("<<")?;
    (text.ends_with(">>") && text.len() - at >= 4).then_some(at)
}

/// A mind map node over several lines: `**:first` up to a line that ends
/// with `;` (and maybe a stereotype). `None` if `lines[at]` does not start
/// one or nothing ends it. With the node, its level from the markers and
/// the index of the next line. PlantUML trims the first line and counts
/// only the markers, so the indentation does not count: `  *:x` is a root.
fn multiline(lines: &[Line], at: usize) -> Option<(Node, usize, usize)> {
    let Line { no, text, .. } = lines[at];
    let count = text.len() - text.trim_start_matches(['*', '#']).len();
    if count == 0 {
        return None;
    }
    let (color, shape, rest) = mindmap_decoration(&text[count..]);
    let first = rest.strip_prefix(':')?;
    let (value, next) = multiline_text(lines, at, first)?;
    Some((node(value, shape, color, no), count - 1, next))
}

/// A WBS node over several lines: markers, decoration, `:`, then as for
/// a mind map. With the markers, without the indentation that PlantUML
/// trims, and the decoration.
fn wbs_multiline<'a>(
    lines: &[Line<'a>],
    at: usize,
) -> Option<(Node, String, WbsDecoration<'a>, usize)> {
    let Line { no, text, .. } = lines[at];
    let count = text.len() - text.trim_start_matches(['*', '+', '-']).len();
    if count == 0 {
        return None;
    }
    let d = wbs_decoration(&text[count..])?;
    let first = d.rest.strip_prefix(':')?;
    let (value, next) = multiline_text(lines, at, first)?;
    let node = node(value, d.shape, d.color.map(str::to_owned), no);
    Some((node, text[..count].to_owned(), d, next))
}

/// The text of a multi-line node whose first line holds `first`, and the
/// index after its last line; blank lines in it stay.
fn multiline_text(lines: &[Line], at: usize, first: &str) -> Option<(String, usize)> {
    let mut body = vec![first];
    for (offset, line) in lines[at + 1..].iter().enumerate() {
        body.extend(std::iter::repeat_n("", line.blank_before));
        match block_end(line.text) {
            Some(last) => {
                body.push(last);
                return Some((body.join("\n"), at + offset + 2));
            }
            None => body.push(line.text),
        }
    }
    None
}

/// The text before the `;` that ends a multi-line node, which a
/// `<<stereotype>>` may follow.
fn block_end(text: &str) -> Option<&str> {
    if let Some(before) = text.strip_suffix(';') {
        return Some(before);
    }
    text.rmatch_indices(';').find_map(|(semi, _)| {
        let after = text[semi + 1..].trim_start();
        (after.len() >= 5 && after.starts_with("<<") && after.ends_with(">>"))
            .then(|| &text[..semi])
    })
}

/// `left side`, `right side` and the like: whether new ideas go left.
fn side(lower: &str) -> Option<bool> {
    if lower.contains(['*', '#']) {
        return None;
    }
    // The last direction word that a `side` or `direction` follows.
    let words = ["left", "right", "top", "bottom"];
    let bytes = lower.as_bytes();
    let boundary = |i: usize| {
        i == 0
            || i >= bytes.len()
            || !(bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_')
            || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_')
    };
    let mut found = None;
    for word in words {
        for (i, _) in lower.match_indices(word) {
            let end = i + word.len();
            if boundary(i)
                && boundary(end)
                && (lower[end..].contains("side") || lower[end..].contains("direction"))
                && found.is_none_or(|(at, _)| i > at)
            {
                found = Some((i, word));
            }
        }
    }
    found.map(|(_, word)| matches!(word, "left" | "top"))
}

/// `A -> B`, `A ..> B`: an arrow between two codes.
fn is_link(text: &str) -> bool {
    let code = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    if code == 0 {
        return false;
    }
    let rest = text[code..].trim_start();
    let arrow = rest.len() - rest.trim_start_matches(['.', '-']).len();
    arrow > 0 && rest[arrow..].starts_with('>')
}

fn too_deep() -> String {
    format!("more than {MAX_DEPTH} levels are not supported")
}

fn not_drawn(text: &str, no: usize, what: &str) -> Declined {
    if let Some(word) = NOT_YET.iter().find(|w| {
        starts_with_ci(text, w)
            && text[w.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric())
    }) {
        return Declined::at(no, format!("`{word}` is not supported yet"));
    }
    Declined::at(
        no,
        format!(
            "`{}` is not part of {what} the native engine can draw",
            snippet(text)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::body;

    fn mindmap(source: &str) -> MindMap {
        parse_mindmap(&body(source).unwrap().1).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn wbs(source: &str) -> Wbs {
        parse_wbs(&body(source).unwrap().1).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn declined(source: &str) -> Declined {
        let (kind, lines) = body(source).unwrap();
        match kind {
            crate::source::Kind::Wbs => parse_wbs(&lines).expect_err("should decline"),
            _ => parse_mindmap(&lines).expect_err("should decline"),
        }
    }

    /// The tree as `label(children…)`, the left side after a `|`.
    fn outline(map: &Map) -> String {
        fn ideas(list: &[Idea]) -> String {
            list.iter()
                .map(|i| {
                    if i.children.is_empty() {
                        i.node.text.value.clone()
                    } else {
                        format!("{}({})", i.node.text.value, ideas(&i.children))
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
        format!(
            "{}({} | {})",
            map.root.text.value,
            ideas(&map.right),
            ideas(&map.left)
        )
    }

    fn wbs_outline(e: &Element) -> String {
        let list = |l: &[Element]| l.iter().map(wbs_outline).collect::<Vec<_>>().join(" ");
        if e.left.is_empty() && e.right.is_empty() {
            e.node.text.value.clone()
        } else {
            format!(
                "{}({} | {})",
                e.node.text.value,
                list(&e.left),
                list(&e.right)
            )
        }
    }

    #[test]
    fn mindmap_levels_sides_and_roots() {
        let d = mindmap(
            "@startmindmap\ntitle Plan\n* root\n** a\n*** a1\n** b\nleft side\n** c\n*** c1\nright side\n*** b1\n* second\n** x\n@endmindmap",
        );
        assert_eq!(d.frame.title.as_ref().unwrap().value, "Plan");
        let maps: Vec<String> = d.maps.iter().map(outline).collect();
        // After `right side`, `***` goes below the last idea on the right.
        assert_eq!(maps, ["root(a(a1) b(b1) | c(c1))", "second(x | )"]);
        assert_eq!(d.maps[0].right[1].node.text.line, 5);
        assert!(!d.vertical);
    }

    #[test]
    fn indentation_counts_against_the_first_line() {
        // Two spaces are one level; `**` ignores the indentation.
        let d = mindmap("* root\n  * a\n    * a1\n  * b");
        assert_eq!(outline(&d.maps[0]), "root(a(a1) b | )");
        let d = mindmap("* root\n    ** a\n ** b");
        assert_eq!(outline(&d.maps[0]), "root(a b | )");
        // The first level sets the step: `***` right below the root is
        // one level down, as `**` would be.
        let d = mindmap("* root\n*** a\n***** a1");
        assert_eq!(outline(&d.maps[0]), "root(a(a1) | )");
        let d = mindmap("* root\n\t* a\n\t\t* a1");
        assert_eq!(outline(&d.maps[0]), "root(a(a1) | )");
        // A multi-line node counts its markers only, as in PlantUML.
        let d = mindmap("* root\n  *:child\nmore;\n  ** below");
        let maps: Vec<String> = d.maps.iter().map(outline).collect();
        assert_eq!(maps, ["root( | )", "child\nmore(below | )"]);
    }

    #[test]
    fn plus_minus_root_and_vertical() {
        let d = mindmap(
            "top to bottom direction\n0 OS\n++ Linux\n+++ Debian\n-- Windows\n--- 95\n+- also left",
        );
        assert!(d.vertical);
        assert_eq!(
            outline(&d.maps[0]),
            "OS(Linux(Debian) | Windows(95) also left)"
        );
    }

    #[test]
    fn decorations_and_labels() {
        let d = mindmap(
            "* root <<rootstyle>>\n**[#Orange] colored\n**_ plain\n**[#red]_ both\n** [#blue] text\n**no space\n**:first\nsecond\n\nfourth;\n** after\n**:one line;\n** stereo <<x>>",
        );
        let ideas = &d.maps[0].right;
        let shown: Vec<(&str, NodeShape, Option<&str>)> = ideas
            .iter()
            .map(|i| {
                (
                    i.node.text.value.as_str(),
                    i.node.shape,
                    i.node.color.as_deref(),
                )
            })
            .collect();
        assert_eq!(d.maps[0].root.text.value, "root ");
        assert_eq!(
            shown,
            [
                ("colored", NodeShape::Box, Some("#Orange")),
                ("plain", NodeShape::Plain, None),
                ("both", NodeShape::Plain, Some("#red")),
                ("[#blue] text", NodeShape::Box, None),
                ("no space", NodeShape::Box, None),
                ("first\nsecond\n\nfourth", NodeShape::Box, None),
                ("after", NodeShape::Box, None),
                // Without a later `;` line it is one line, as written.
                (":one line;", NodeShape::Box, None),
                ("stereo ", NodeShape::Box, None),
            ]
        );
        assert_eq!(ideas[5].node.text.line, 6);
    }

    #[test]
    fn mindmap_declines_what_it_does_not_draw() {
        let deep: Vec<String> = (1..=102).map(|k| format!("{} x", "*".repeat(k))).collect();
        let deep = deep.join("\n");
        for (source, line, part) in [
            ("* a\n** b\n**** c", 2, "skips a level"),
            ("* a\n  * b\n   * c", 2, "multiple"),
            ("* a\n  ++ b", 1, "beginning of the line"),
            ("* a\nnewpage", 1, "newpage"),
            ("* a\n<style>\n</style>", 1, "<style>"),
            (deep.as_str(), 101, "levels"),
            ("* a\nfoo bar", 1, "foo bar"),
        ] {
            let err = declined(&format!("@startmindmap\n{source}\n@endmindmap"));
            assert_eq!(err.line, Some(line + 1), "{source}: {err:?}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
    }

    #[test]
    fn wbs_rows_columns_and_sides() {
        let d = wbs(
            "@startwbs\n* Project\n** Plan\n*** Scope\n***< Budget\n****> Detail\n** Build\n**< First\n--- left by minus\n+++ right by plus\n@endwbs",
        );
        // A left child of the root goes first in the row.
        assert_eq!(
            wbs_outline(&d.root),
            "Project( | First(left by minus | right by plus) Plan(Budget( | Detail) | Scope) Build)"
        );
    }

    #[test]
    fn wbs_decorations_codes_and_pseudo_nodes() {
        let d = wbs(
            "* root\n**_<[#Pink](a) both orders\n**[#Pink](b)_> old order\n** \"Quoted\" as Q\n**_\n**: two\nlines;\n** tag <<s>>",
        );
        let row: Vec<(&str, NodeShape, Option<&str>)> = d
            .root
            .right
            .iter()
            .map(|e| {
                (
                    e.node.text.value.as_str(),
                    e.node.shape,
                    e.node.color.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            row,
            [
                ("both orders", NodeShape::Plain, Some("#Pink")),
                ("old order", NodeShape::Plain, Some("#Pink")),
                ("Quoted", NodeShape::Box, None),
                ("", NodeShape::Pseudo, None),
                (" two\nlines", NodeShape::Box, None),
                ("tag", NodeShape::Box, None),
            ]
        );
    }

    #[test]
    fn wbs_declines_what_it_does_not_draw() {
        for (source, line, part) in [
            ("* a\n* b", 1, "only one root"),
            ("* a\n  *:b\nc;", 1, "only one root"),
            ("* a\n**", 1, "needs a label"),
            ("* a\n**text", 1, "**text"),
            ("* a\n*** b", 1, "skips a level"),
            ("* a\n** (x) b\n** (y) c\nx -> y", 3, "arrows"),
            ("* a\n**_[#red]< mixed", 1, "mixed"),
            ("* a\nleft to right direction", 1, "left to right"),
        ] {
            let err = declined(&format!("@startwbs\n{source}\n@endwbs"));
            assert_eq!(err.line, Some(line + 1), "{source}: {err:?}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
        assert!(declined("@startwbs\ntitle x\n@endwbs").line.is_none());
    }
}
