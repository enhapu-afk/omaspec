//! Mermaid `requirementDiagram`, read into the structure engine's tree.
//!
//! A requirement and an element are boxes: what they are over their
//! name, then a row for each thing said about them (`ID`, `Text`, `Risk`
//! and `Verification`; `Type` and `Doc Ref`). A relationship is a dashed
//! arrow that names its kind (`«satisfies»`); `contains` is a solid line
//! with a filled diamond at the requirement that holds the other.
//!
//! `direction TB` and `LR`, `style`, `classDef`, `class` and `:::` are
//! read. `direction BT` and `RL`, a relationship of a name nothing
//! declares, and a risk or a verification method Mermaid does not know
//! are refused by name.

use std::collections::HashMap;

use super::mermaid::{PaintStyle, Reason, breaks, cut_comment, lay, paint_style, unquote};
use super::{Dash, Diagram, Direction, Element, Group, Head, Link, Member, Shape, Slot};
use crate::common::{Frame, Text, text_at};
use crate::source::starts_with_ci;

const HEADER: &str = "requirementDiagram";
/// A row's words are broken after about this many letters.
const ROW: usize = 44;

/// What a box is, as written and as shown over its name.
const KINDS: [(&str, &str); 7] = [
    ("requirement", "Requirement"),
    ("functionalRequirement", "Functional Requirement"),
    ("interfaceRequirement", "Interface Requirement"),
    ("performanceRequirement", "Performance Requirement"),
    ("physicalRequirement", "Physical Requirement"),
    ("designConstraint", "Design Constraint"),
    ("element", "Element"),
];
const RISKS: [&str; 3] = ["Low", "Medium", "High"];
const METHODS: [&str; 4] = ["Analysis", "Inspection", "Test", "Demonstration"];
const RELATIONS: [&str; 7] = [
    "contains",
    "copies",
    "derives",
    "satisfies",
    "verifies",
    "refines",
    "traces",
];

/// The diagram `source` describes, or `None` when it is not a
/// `requirementDiagram` or it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a requirement diagram, and one it accepts, are
/// `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

enum Stop {
    /// Not a requirement diagram.
    Foreign,
    At(Reason),
}

fn stop(line: usize, what: &'static str) -> Stop {
    Stop::At(Reason { line, what })
}

/// The rows of a box as they are written, each kept once: a key written
/// again replaces the first.
#[derive(Default)]
struct Rows {
    said: Vec<(&'static str, String, usize)>,
}

impl Rows {
    fn set(&mut self, name: &'static str, value: String, line: usize) {
        match self.said.iter_mut().find(|(have, ..)| *have == name) {
            Some(row) => *row = (name, value, line),
            None => self.said.push((name, value, line)),
        }
    }
}

struct Reader {
    nodes: Vec<Element>,
    /// The box whose `{` is open, and whether it is an element.
    open: Option<(usize, bool, Rows)>,
    edges: Vec<Link>,
    direction: Direction,
    /// `classDef`: a name and its style.
    defs: HashMap<String, PaintStyle>,
    /// The classes a box was given, in the order they were written.
    classes: Vec<(String, String, usize)>,
    /// `style`: a box and its style.
    styles: Vec<(String, PaintStyle, usize)>,
}

fn read(source: &str) -> Result<Diagram, Stop> {
    let (title, lines) = scan(source);
    let Some(((_, header), rest)) = lines.split_first() else {
        return Err(Stop::Foreign);
    };
    if !starts_with_ci(header, HEADER) || header.len() != HEADER.len() {
        return Err(Stop::Foreign);
    }
    let mut reader = Reader {
        nodes: Vec::new(),
        open: None,
        edges: Vec::new(),
        direction: Direction::Down,
        defs: HashMap::new(),
        classes: Vec::new(),
        styles: Vec::new(),
    };
    for (line, text) in rest {
        reader.line(*line, text)?;
    }
    reader.finish(title)
}

/// The title of the front matter, and the lines that say something
/// without their comments.
fn scan(source: &str) -> (Option<Text>, Vec<(usize, String)>) {
    let mut front = false;
    let mut title = None;
    let mut lines = Vec::new();
    for (no, raw) in source.lines().enumerate() {
        let text = raw.trim();
        if text == "---" && (front || lines.is_empty()) {
            front = !front;
            continue;
        }
        if front {
            if title.is_none()
                && !raw.starts_with([' ', '\t'])
                && let Some(rest) = text.strip_prefix("title:")
            {
                let value = unquote(rest.trim());
                if !value.is_empty() {
                    title = Some(text_at(&value, no));
                }
            }
            continue;
        }
        let text = cut_comment(text);
        if !text.is_empty() {
            lines.push((no, text));
        }
    }
    (title, lines)
}

impl Reader {
    fn line(&mut self, line: usize, text: &str) -> Result<(), Stop> {
        if self.open.is_some() {
            return self.row(line, text);
        }
        let (first, rest) = match text.split_once(char::is_whitespace) {
            Some((first, rest)) => (first, rest.trim()),
            None => (text, ""),
        };
        if let Some((_, shown)) = KINDS
            .iter()
            .find(|(word, _)| first.eq_ignore_ascii_case(word))
            && rest.contains('{')
        {
            return self.declare(line, shown, rest);
        }
        match first.to_ascii_lowercase().as_str() {
            "direction" => {
                self.direction = match rest.to_ascii_lowercase().as_str() {
                    "tb" | "td" => Direction::Down,
                    "lr" => Direction::Right,
                    "bt" => return Err(stop(line, "direction BT")),
                    "rl" => return Err(stop(line, "direction RL")),
                    _ => return Err(stop(line, "an unknown line")),
                };
                return Ok(());
            }
            "style" => {
                let (names, style) = rest
                    .split_once(char::is_whitespace)
                    .ok_or(stop(line, "a style"))?;
                let style = paint_style(style, line).map_err(Stop::At)?;
                for name in names.split(',').filter(|name| !name.is_empty()) {
                    self.styles.push((name.to_owned(), style.clone(), line));
                }
                return Ok(());
            }
            "classdef" => {
                let (names, style) = rest
                    .split_once(char::is_whitespace)
                    .ok_or(stop(line, "a style"))?;
                let style = paint_style(style, line).map_err(Stop::At)?;
                for name in names.split(',').filter(|name| !name.is_empty()) {
                    lay(self.defs.entry(name.to_owned()).or_default(), &style);
                }
                return Ok(());
            }
            "class" => {
                let (names, classes) = rest
                    .split_once(char::is_whitespace)
                    .ok_or(stop(line, "an unknown line"))?;
                for name in names.split(',').filter(|name| !name.is_empty()) {
                    self.give(name, classes.trim(), line);
                }
                return Ok(());
            }
            "acctitle" | "acctitle:" | "accdescr" | "accdescr:" => return Ok(()),
            _ => {}
        }
        // `name:::class`, with no box of its own.
        if let Some((name, classes)) = text.split_once(":::")
            && !classes.contains(['-', '<', '{'])
        {
            self.give(&unquote(name.trim()), classes.trim(), line);
            return Ok(());
        }
        self.relate(line, text)
    }

    /// `kind name {`, with the classes of a `:::` after the name.
    fn declare(&mut self, line: usize, shown: &str, rest: &str) -> Result<(), Stop> {
        let (name, after) = rest.split_once('{').unwrap_or((rest, ""));
        let (name, classes) = match name.split_once(":::") {
            Some((name, classes)) => (name, Some(classes.trim())),
            None => (name, None),
        };
        let name = unquote(name.trim());
        if name.is_empty() {
            return Err(stop(line, "a box without a name"));
        }
        if self.nodes.iter().any(|node| node.key == name) {
            return Err(stop(line, "a name declared twice"));
        }
        if let Some(classes) = classes {
            self.give(&name, classes, line);
        }
        let element = shown == "Element";
        self.nodes.push(Element {
            label: text_at(&name, line),
            key: name,
            stereotypes: vec![text_at(shown, line)],
            members: Vec::new(),
            shape: Shape::Class,
            fill: None,
            line_color: None,
            thickness: None,
            text_color: None,
            border_dash: None,
            explicit: true,
            composite: false,
            children: Group::default(),
            link: None,
            line,
        });
        self.open = Some((self.nodes.len() - 1, element, Rows::default()));
        // `requirement a { id: 1 }` on one line.
        let after = after.trim();
        if let Some(inner) = after.strip_suffix('}') {
            if !inner.trim().is_empty() {
                self.row(line, inner.trim())?;
            }
            return self.row(line, "}");
        }
        if after.is_empty() {
            Ok(())
        } else {
            self.row(line, after)
        }
    }

    /// A line between the braces of a box: `key: value`, or the `}`.
    fn row(&mut self, line: usize, text: &str) -> Result<(), Stop> {
        let Some((index, element, rows)) = &mut self.open else {
            return Ok(());
        };
        if text == "}" {
            let node = &mut self.nodes[*index];
            for (name, value, at) in std::mem::take(&mut rows.said) {
                node.members.push(Member::Text {
                    text: text_at(&broken(&format!("{name}: {value}")), at),
                    italic: false,
                    underline: false,
                    slot: Slot::Attribute,
                });
            }
            self.open = None;
            return Ok(());
        }
        let (key, value) = text.split_once(':').ok_or(stop(line, "an unknown line"))?;
        let value = breaks(&unquote(value.trim()));
        let known = |names: &[&'static str], what| {
            names
                .iter()
                .find(|name| name.eq_ignore_ascii_case(&value))
                .map(|name| (*name).to_owned())
                .ok_or(stop(line, what))
        };
        let (name, value) = match (key.trim().to_ascii_lowercase().as_str(), *element) {
            ("id", false) => ("ID", value),
            ("text", false) => ("Text", value),
            ("risk", false) => ("Risk", known(&RISKS, "a risk")?),
            ("verifymethod", false) => ("Verification", known(&METHODS, "a verification method")?),
            ("type", true) => ("Type", value),
            ("docref", true) => ("Doc Ref", value),
            _ => return Err(stop(line, "an unknown line")),
        };
        if !value.is_empty() {
            rows.set(name, value, line);
        }
        Ok(())
    }

    /// `a - kind -> b` or `b <- kind - a`: from `a` to `b`.
    fn relate(&mut self, line: usize, text: &str) -> Result<(), Stop> {
        let unknown = || stop(line, "an unknown line");
        let (first, rest) = name_at(text).ok_or_else(unknown)?;
        let (back, rest) = match rest.strip_prefix("<-") {
            Some(rest) => (true, rest),
            None => (false, rest.strip_prefix('-').ok_or_else(unknown)?),
        };
        let mark = if back { "-" } else { "->" };
        let (kind, rest) = rest.split_once(mark).ok_or_else(unknown)?;
        let kind = RELATIONS
            .iter()
            .find(|name| name.eq_ignore_ascii_case(kind.trim()))
            .ok_or(stop(line, "a kind of relationship"))?;
        let (second, rest) = name_at(rest.trim_start()).ok_or_else(unknown)?;
        if !rest.trim().is_empty() {
            return Err(unknown());
        }
        let (from, to) = if back {
            (second, first)
        } else {
            (first, second)
        };
        let find = |name: &str| {
            self.nodes
                .iter()
                .position(|node| node.key == name)
                .ok_or(stop(line, "a relationship of a name nothing declares"))
        };
        let (from, to) = (find(&from)?, find(&to)?);
        let contains = *kind == "contains";
        self.edges.push(Link {
            from,
            to,
            label: Some(text_at(&format!("«{kind}»"), line)),
            from_card: None,
            to_card: None,
            from_head: if contains {
                Head::DiamondFilled
            } else {
                Head::None
            },
            to_head: if contains { Head::None } else { Head::Arrow },
            color: None,
            dash: if contains { Dash::Solid } else { Dash::Dashed },
            bold: false,
            line,
            label_before: None,
        });
        Ok(())
    }

    /// The classes of `classes` (`a,b`) for the box `name`.
    fn give(&mut self, name: &str, classes: &str, line: usize) {
        for class in classes.split(',').map(str::trim) {
            if !class.is_empty() {
                self.classes
                    .push((name.trim().to_owned(), class.to_owned(), line));
            }
        }
    }

    fn finish(mut self, title: Option<Text>) -> Result<Diagram, Stop> {
        if let Some((index, ..)) = self.open {
            return Err(stop(self.nodes[index].line, "an unclosed `{`"));
        }
        // A box takes the default class, then its classes, then its own
        // style. A class or a style of a name nothing declares, and a
        // class nothing defines, paint nothing, as in Mermaid.
        let default = self.defs.get("default").cloned();
        for node in &mut self.nodes {
            let mut style = default.clone().unwrap_or_default();
            for (_, class, _) in self.classes.iter().filter(|(name, ..)| *name == node.key) {
                if let Some(def) = self.defs.get(class) {
                    lay(&mut style, def);
                }
            }
            for (_, own, _) in self.styles.iter().filter(|(name, ..)| *name == node.key) {
                lay(&mut style, own);
            }
            node.fill = style.fill;
            node.line_color = style.stroke;
            node.thickness = style.thickness;
            node.text_color = style.text;
            node.border_dash = style.dash;
        }
        Ok(Diagram {
            root: Group {
                nodes: self.nodes,
                edges: self.edges,
                direction: self.direction,
                ..Group::default()
            },
            frame: Frame {
                title,
                ..Frame::default()
            },
        })
    }
}

/// The name `text` starts with, in quotes or up to the mark of a
/// relationship, and what follows it.
fn name_at(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    if let Some(rest) = text.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some((rest[..end].to_owned(), rest[end + 1..].trim_start()));
    }
    let end = text
        .find(['-', '<', '>', ':', ',', '{', '='])
        .unwrap_or(text.len());
    let name = text[..end].trim();
    (!name.is_empty()).then(|| (name.to_owned(), &text[end..]))
}

/// `words` on lines of about [`ROW`] letters, broken at its spaces. A
/// last word of up to three letters stays on its line, not alone on the
/// next.
fn broken(words: &str) -> String {
    let mut out = String::new();
    for (index, paragraph) in words.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        let mut length = 0;
        let mut words = paragraph.split_whitespace().peekable();
        while let Some(word) = words.next() {
            let letters = word.chars().count();
            let tail = letters <= 3 && words.peek().is_none();
            if length > 0 && length + 1 + letters > ROW && !tail {
                out.push('\n');
                length = 0;
            } else if length > 0 {
                out.push(' ');
                length += 1;
            }
            out.push_str(word);
            length += letters;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRAKES: &str = "---\ntitle: Brakes\n---\nrequirementDiagram\n  direction LR\n  %% what the car has to do\n  requirement stop {\n    id: 1\n    text: The car stops within 40 m.\n    risk: high\n    verifymethod: test\n  }\n  performanceRequirement \"wet road\" {\n    id: 1.1\n    text: \"On a wet road it stops within 55 m.\"\n    risk: Medium\n    verifymethod: Demonstration\n  }\n  element track_test {\n    type: test suite\n    docref: tests/brakes.md\n  }\n  stop - contains -> \"wet road\"\n  stop <- verifies - track_test\n";

    #[test]
    fn requirements_and_elements_are_boxes_with_their_rows() {
        let diagram = from_mermaid(BRAKES).unwrap();
        assert_eq!(diagram.frame.title.as_ref().unwrap().value, "Brakes");
        assert_eq!(diagram.root.direction, Direction::Right);
        let told: Vec<(&str, &str, Vec<&str>)> = diagram
            .root
            .nodes
            .iter()
            .map(|node| {
                let rows = node
                    .members
                    .iter()
                    .map(|member| match member {
                        Member::Text { text, .. } => text.value.as_str(),
                        _ => "",
                    })
                    .collect();
                (node.key.as_str(), node.stereotypes[0].value.as_str(), rows)
            })
            .collect();
        assert_eq!(
            told,
            [
                (
                    "stop",
                    "Requirement",
                    vec![
                        "ID: 1",
                        "Text: The car stops within 40 m.",
                        "Risk: High",
                        "Verification: Test"
                    ]
                ),
                (
                    "wet road",
                    "Performance Requirement",
                    vec![
                        "ID: 1.1",
                        "Text: On a wet road it stops within 55 m.",
                        "Risk: Medium",
                        "Verification: Demonstration"
                    ]
                ),
                (
                    "track_test",
                    "Element",
                    vec!["Type: test suite", "Doc Ref: tests/brakes.md"]
                ),
            ]
        );
        assert_eq!(diagram.root.nodes[1].line, 12);
    }

    #[test]
    fn a_relationship_names_its_kind_and_runs_from_its_source() {
        let diagram = from_mermaid(BRAKES).unwrap();
        let [holds, verifies] = &diagram.root.edges[..] else {
            panic!("two relationships");
        };
        assert_eq!((holds.from, holds.to), (0, 1));
        assert_eq!(holds.label.as_ref().unwrap().value, "«contains»");
        assert_eq!(
            (holds.from_head, holds.to_head, holds.dash),
            (Head::DiamondFilled, Head::None, Dash::Solid)
        );
        // Written from its end: `stop <- verifies - track_test`.
        assert_eq!((verifies.from, verifies.to), (2, 0));
        assert_eq!(
            (verifies.from_head, verifies.to_head, verifies.dash),
            (Head::None, Head::Arrow, Dash::Dashed)
        );
        assert_eq!(verifies.line, 23);
    }

    #[test]
    fn long_words_are_broken_and_a_box_on_one_line_is_read() {
        let long = "requirementDiagram\n  requirement a { text: one two three four five six seven eight nine ten eleven twelve }\n  element b {\n  }\n";
        let diagram = from_mermaid(long).unwrap();
        let Member::Text { text, .. } = &diagram.root.nodes[0].members[0] else {
            panic!("a row");
        };
        assert_eq!(
            text.value,
            "Text: one two three four five six seven\neight nine ten eleven twelve"
        );
        assert_eq!(
            broken("Text: The car stops from 100 km/h within 40 m."),
            "Text: The car stops from 100 km/h within 40 m."
        );
        assert!(diagram.root.nodes[1].members.is_empty());
    }

    #[test]
    fn styles_and_classes_paint_a_box() {
        let source = "requirementDiagram\n  requirement a:::hot {\n    id: 1\n  }\n  element b {\n  }\n  classDef default stroke:#111111\n  classDef hot fill:#ffdddd\n  classDef cold fill:#ddddff,color:#000080\n  class b cold\n  style b stroke:#008000,stroke-width:3px\n  style nobody fill:#fff\n";
        let diagram = from_mermaid(source).unwrap();
        let [a, b] = &diagram.root.nodes[..] else {
            panic!("two boxes");
        };
        assert_eq!(a.fill.as_deref(), Some("#ffdddd"));
        assert_eq!(a.line_color.as_deref(), Some("#111111"));
        assert_eq!(b.fill.as_deref(), Some("#ddddff"));
        assert_eq!(b.text_color.as_deref(), Some("#000080"));
        assert_eq!(b.line_color.as_deref(), Some("#008000"));
        assert_eq!(b.thickness, Some(3.0));
    }

    #[test]
    fn the_diagram_is_drawn() {
        let svg = super::super::draw(&from_mermaid(BRAKES).unwrap()).unwrap();
        for words in [
            "Brakes",
            "wet road",
            "track_test",
            "Risk: High",
            "«verifies»",
        ] {
            assert!(svg.contains(words), "{words}: {svg}");
        }
    }

    #[test]
    fn what_it_does_not_draw_is_refused_by_name() {
        let boxes = "requirementDiagram\n  requirement a {\n  }\n  element b {\n  }\n";
        for (rest, line, what) in [
            ("  direction RL\n", 5, "direction RL"),
            ("  direction BT\n", 5, "direction BT"),
            ("  a - loves -> b\n", 5, "a kind of relationship"),
            (
                "  a - traces -> c\n",
                5,
                "a relationship of a name nothing declares",
            ),
            ("  requirement a {\n  }\n", 5, "a name declared twice"),
            ("  requirement c {\n    risk: huge\n  }\n", 6, "a risk"),
            (
                "  requirement c {\n    verifymethod: hope\n  }\n",
                6,
                "a verification method",
            ),
            ("  element c {\n    risk: high\n  }\n", 6, "an unknown line"),
            ("  requirement c {\n    id: 1\n", 5, "an unclosed `{`"),
            ("  style a opacity:0.5\n", 5, "a style"),
            ("  a -> b\n", 5, "an unknown line"),
        ] {
            let source = format!("{boxes}{rest}");
            assert_eq!(mermaid_refusal(&source), Some((line, what)), "{source}");
            assert!(from_mermaid(&source).is_none());
        }
        assert_eq!(mermaid_refusal("classDiagram\n  class A\n"), None);
        assert_eq!(mermaid_refusal(BRAKES), None);
    }
}
