//! A Mermaid `mindmap` read into the mind-map tree.
//!
//! Indentation sets the parent, the way Mermaid does: the first node's
//! indent is the root, and a later node hangs from the nearest node above
//! it that is indented less. The root's children alternate right and left,
//! because the layout grows to both sides. `::icon()` and `:::class` mark
//! the previous node and are not drawn. A cloud (`)text(`) and a bang
//! (`))text((`) are their own shapes. `(-text-)` is still refused: it is
//! not that cloud.

use super::{Idea, Map, MindMap, Node, NodeShape};
use crate::common::{Frame, text_at};

const MAX_DEPTH: usize = 100;

/// The mind map `source` describes, or `None` when it is not a `mindmap`
/// or it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<MindMap> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a mind map, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

struct Reason {
    line: usize,
    what: &'static str,
}

enum Stop {
    Foreign,
    At(Reason),
}

fn read(source: &str) -> Result<MindMap, Stop> {
    let src = scan(source);
    let Some((header, rest)) = src.lines.split_first() else {
        return Err(Stop::Foreign);
    };
    if !header.1.eq_ignore_ascii_case("mindmap") {
        return Err(Stop::Foreign);
    }
    let mut tree = Tree::new(src.title);
    for (line, text) in rest {
        tree.line(*line, text).map_err(Stop::At)?;
    }
    Ok(tree.finish())
}

struct Scanned {
    title: Option<crate::common::Text>,
    lines: Vec<(usize, String)>,
}

/// Front matter only at the top. A `---` later is a node's text.
fn scan(source: &str) -> Scanned {
    let mut title = None;
    let mut lines = Vec::new();
    let mut iter = source.lines().enumerate().peekable();
    if iter.peek().is_some_and(|(_, raw)| raw.trim() == "---") {
        iter.next();
        for (no, raw) in iter.by_ref() {
            let text = raw.trim();
            if text == "---" {
                break;
            }
            if title.is_none()
                && !raw.starts_with([' ', '\t'])
                && let Some(rest) = text.strip_prefix("title:")
            {
                let value = unquote(rest.trim());
                if !value.is_empty() {
                    title = Some(text_at(&value, no));
                }
            }
        }
    }
    for (no, raw) in iter {
        let text = raw.trim();
        if text.is_empty() || text.starts_with("%%") {
            continue;
        }
        let indent = raw.chars().take_while(|c| c.is_whitespace()).count();
        let mut kept = " ".repeat(indent);
        kept.push_str(text);
        lines.push((no, kept));
    }
    Scanned { title, lines }
}

struct Tree {
    title: Option<crate::common::Text>,
    root: Option<Node>,
    right: Vec<Idea>,
    left: Vec<Idea>,
    base: Option<usize>,
    branches: usize,
    stack: Vec<Open>,
}

struct Open {
    level: usize,
    right: bool,
    path: Vec<usize>,
}

impl Tree {
    fn new(title: Option<crate::common::Text>) -> Self {
        Self {
            title,
            root: None,
            right: Vec::new(),
            left: Vec::new(),
            base: None,
            branches: 0,
            stack: Vec::new(),
        }
    }

    fn line(&mut self, line: usize, raw: &str) -> Result<(), Reason> {
        let indent = raw.chars().take_while(|c| c.is_whitespace()).count();
        let text = raw.trim();
        if text.starts_with("::icon(") {
            if text.ends_with(')') {
                return Ok(());
            }
            return Err(Reason {
                line,
                what: "an icon",
            });
        }
        if text.starts_with(":::") {
            return Ok(());
        }
        let node = parse_node(text, line)?;
        self.place(node, indent, line)
    }

    fn place(&mut self, node: Node, indent: usize, line: usize) -> Result<(), Reason> {
        let level = match self.base {
            None => {
                self.base = Some(indent);
                0
            }
            Some(base) if indent < base => {
                return Err(Reason {
                    line,
                    what: "a second root",
                });
            }
            Some(base) => indent - base,
        };
        if self.root.is_none() {
            self.root = Some(node);
            return Ok(());
        }
        if level == 0 {
            return Err(Reason {
                line,
                what: "a second root",
            });
        }
        self.stack.retain(|open| open.level < level);
        let (right, parent) = match self.stack.last() {
            Some(open) if open.path.len() >= MAX_DEPTH => {
                return Err(Reason {
                    line,
                    what: "a level past 100",
                });
            }
            Some(open) => (open.right, Some(open.path.clone())),
            None => {
                let right = self.branches.is_multiple_of(2);
                self.branches += 1;
                (right, None)
            }
        };
        let list = if right {
            &mut self.right
        } else {
            &mut self.left
        };
        let path = append(list, parent.as_deref(), node);
        self.stack.push(Open { level, right, path });
        Ok(())
    }

    fn finish(self) -> MindMap {
        let maps = self
            .root
            .map(|root| Map {
                root,
                right: self.right,
                left: self.left,
            })
            .into_iter()
            .collect();
        MindMap {
            maps,
            vertical: false,
            frame: Frame {
                title: self.title,
                ..Frame::default()
            },
        }
    }
}

fn append(list: &mut Vec<Idea>, parent: Option<&[usize]>, node: Node) -> Vec<usize> {
    let Some(parent) = parent else {
        list.push(leaf(node));
        return vec![list.len() - 1];
    };
    let idea = idea_mut(list, parent);
    idea.children.push(leaf(node));
    let mut path = parent.to_vec();
    path.push(idea.children.len() - 1);
    path
}

fn leaf(node: Node) -> Idea {
    Idea {
        node,
        children: Vec::new(),
    }
}

fn idea_mut<'a>(list: &'a mut [Idea], path: &[usize]) -> &'a mut Idea {
    let mut idea = &mut list[path[0]];
    for &index in &path[1..] {
        idea = &mut idea.children[index];
    }
    idea
}

#[derive(Clone, Copy)]
enum Mark {
    Shape(NodeShape),
    Refuse(&'static str),
}

/// Shape openers, longest first, and the closer Mermaid pairs with each.
const OPENERS: &[(&str, &str, Mark)] = &[
    ("((", "))", Mark::Shape(NodeShape::Circle)),
    ("))", "((", Mark::Shape(NodeShape::Bang)),
    ("{{", "}}", Mark::Shape(NodeShape::Hexagon)),
    ("(-", "-)", Mark::Refuse("a cloud")),
    ("-)", "(-", Mark::Refuse("a bang")),
    ("(", ")", Mark::Shape(NodeShape::Box)),
    (")", "(", Mark::Shape(NodeShape::Cloud)),
    ("[", "]", Mark::Shape(NodeShape::Square)),
];

fn parse_node(text: &str, line: usize) -> Result<Node, Reason> {
    let Some((at, open, close, mark)) = find_opener(text) else {
        return Ok(make_node(text, NodeShape::Box, line));
    };
    let inside = text[at + open.len()..].trim_start();
    let Some(inside) = inside.strip_suffix(close) else {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    };
    let shape = match mark {
        Mark::Shape(shape) => shape,
        Mark::Refuse(what) => return Err(Reason { line, what }),
    };
    Ok(make_node(inside.trim(), shape, line))
}

fn find_opener(text: &str) -> Option<(usize, &'static str, &'static str, Mark)> {
    let mut best: Option<(usize, &'static str, &'static str, Mark)> = None;
    for (index, _) in text.char_indices() {
        for (open, close, mark) in OPENERS {
            if text[index..].starts_with(open) {
                let longer = best.is_none_or(|(at, previous, _, _)| {
                    index < at || (index == at && open.len() > previous.len())
                });
                if longer {
                    best = Some((index, open, close, *mark));
                }
            }
        }
    }
    best
}

fn make_node(label: &str, shape: NodeShape, line: usize) -> Node {
    Node {
        text: text_at(&breaks(&unwrap_label(label)), line),
        shape,
        color: None,
        ink: None,
    }
}

fn unwrap_label(text: &str) -> String {
    let text = text.trim();
    let text = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text)
        .trim();
    text.strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
        .unwrap_or(text)
        .to_owned()
}

fn breaks(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::new();
    let mut at = 0;
    while at < text.len() {
        if text[at..].starts_with("\\n") {
            out.push('\n');
            at += 2;
            continue;
        }
        let size = if lower[at..].starts_with("<br/>") {
            5
        } else if lower[at..].starts_with("<br />") {
            6
        } else if lower[at..].starts_with("<br>") {
            4
        } else {
            0
        };
        if size > 0 {
            out.push('\n');
            at += size;
            continue;
        }
        let ch = text[at..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        at += ch.len_utf8();
    }
    out
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::super::{NodeShape, draw_mindmap};
    use super::{from_mermaid, mermaid_refusal};

    fn map(source: &str) -> super::super::MindMap {
        from_mermaid(source).unwrap_or_else(|| panic!("refused {source}"))
    }

    #[test]
    fn reads_the_omarchy_fixture() {
        let diagram = map(include_str!(
            "../../../omagram-core/tests/fixtures/mindmap.mmd"
        ));
        let root = &diagram.maps[0];
        assert_eq!(root.root.text.value, "Omarchy");
        assert_eq!(root.root.shape, NodeShape::Circle);
        assert_eq!(root.right[0].node.text.value, "Hyprland");
        assert_eq!(root.right[0].children[0].node.text.value, "Keybindings");
        assert_eq!(root.right[0].children[1].node.text.value, "Window rules");
        assert_eq!(root.left[0].node.text.value, "Apps");
        let terminal = &root.left[0].children[0];
        assert_eq!(terminal.node.text.value, "Terminal");
        assert_eq!(terminal.children[0].node.text.value, "Ghostty");
        let editor = &root.left[0].children[1];
        assert_eq!(editor.node.text.value, "Editor");
        assert_eq!(editor.node.shape, NodeShape::Square);
        let svg = draw_mindmap(&diagram).unwrap();
        assert!(
            svg.contains("Omarchy") && svg.contains("Ghostty") && svg.contains("<ellipse"),
            "{svg}"
        );
    }

    #[test]
    fn shapes_a_line_break_and_a_title() {
        let source = "\
---
title: Plan
---
mindmap
  ((Root))
    (Round)
    {{Hex}}
    [Square]
    Two<br>lines
    :::quiet
";
        let diagram = map(source);
        assert_eq!(
            diagram.frame.title.as_ref().map(|text| text.value.as_str()),
            Some("Plan")
        );
        let root = &diagram.maps[0];
        assert_eq!(root.root.shape, NodeShape::Circle);
        assert_eq!(root.right[0].node.shape, NodeShape::Box);
        assert_eq!(root.left[0].node.shape, NodeShape::Hexagon);
        assert_eq!(root.right[1].node.text.value, "Square");
        assert_eq!(root.right[1].node.shape, NodeShape::Square);
        assert_eq!(root.left[1].node.text.value, "Two\nlines");
        let svg = draw_mindmap(&diagram).unwrap();
        assert!(svg.contains("Plan") && svg.contains("<polygon"), "{svg}");
    }

    #[test]
    fn a_cloud_and_a_bang_keep_their_shape() {
        let diagram = map("mindmap\n  ))Topic((\n    )Note(\n");
        let root = &diagram.maps[0];
        assert_eq!(root.root.shape, NodeShape::Bang);
        assert_eq!(root.root.text.value, "Topic");
        assert_eq!(root.right[0].node.shape, NodeShape::Cloud);
        assert_eq!(root.right[0].node.text.value, "Note");
        let svg = draw_mindmap(&diagram).unwrap();
        assert!(
            svg.contains("Topic")
                && svg.contains("Note")
                && svg.contains("<polygon")
                && svg.contains("<path"),
            "{svg}"
        );
    }

    #[test]
    fn a_second_root_and_the_other_cloud_are_refused() {
        assert_eq!(mermaid_refusal("mindmap\n  (-no-)\n"), Some((1, "a cloud")));
        assert_eq!(
            mermaid_refusal("mindmap\n  A\n  B\n"),
            Some((2, "a second root"))
        );
        assert_eq!(
            mermaid_refusal("mindmap\n  [open\n"),
            Some((1, "an unknown line"))
        );
        assert!(from_mermaid("sequenceDiagram\nA->>B: hi\n").is_none());
        assert_eq!(mermaid_refusal("sequenceDiagram\nA->>B: hi\n"), None);
    }

    #[test]
    fn an_icon_before_a_node_is_refused_when_it_is_unclosed() {
        assert_eq!(
            mermaid_refusal("mindmap\n  ::icon(fa fa-book\n"),
            Some((1, "an icon"))
        );
    }
}
