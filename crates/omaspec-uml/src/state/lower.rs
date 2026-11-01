//! A Mermaid state diagram, as [`super::mermaid::read`] reads it, lowered
//! into the engine's state diagram: the tree PlantUML's state diagrams
//! are parsed into. One drawer then draws both, as one draws sequence and
//! class diagrams of either language.
//!
//! Mermaid lists every state with the state it sits in and its region;
//! here each composite holds its own container, split into regions side
//! by side where `--` divides it (PlantUML's `||`), and a transition between containers is a crossing of
//! the container that holds both ends. Mermaid lays the whole diagram out
//! in one direction; every container takes it. Classes and `style` lines
//! become the state's colors, and what the tree has no field for (the
//! width of a border, the color of the words) a stereotype of the state's
//! own with its skinparams.

use std::collections::HashMap;

use super::mermaid::{self, Chart};
use super::{
    Container, Crossing, Dash, Diagram, Direction, Kind, Node, Note, Side, Split, Transition,
};
use crate::common::{Frame, Skinparam, Text};
use crate::graph::across::meeting;

/// A Mermaid state diagram as the engine's state diagram; `None` when
/// [`mermaid::read`] does not read it, or a transition joins a state with
/// one it holds.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    let chart = mermaid::read(source)?;
    let (direction, mirrored) = match chart.direction {
        None | Some(mermaid::Direction::Down) => (Direction::Down, false),
        Some(mermaid::Direction::Up) => (Direction::Down, true),
        Some(mermaid::Direction::Right) => (Direction::Right, false),
        Some(mermaid::Direction::Left) => (Direction::Right, true),
    };
    let mut lowering = Lowering {
        chart: &chart,
        direction,
        mirrored,
        paths: vec![Vec::new(); chart.states.len()],
        frame: Frame {
            title: front_title(source),
            ..Frame::default()
        },
        looks: looks(&chart),
    };
    let mut root = lowering.container(None, &mut Vec::new());
    for t in &chart.transitions {
        let (from, to) = (&lowering.paths[t.from], &lowering.paths[t.to]);
        let mut arrow = Transition {
            from: 0,
            to: 0,
            label: (!t.label.is_empty()).then(|| text(&t.label, t.line)),
            color: None,
            dash: Dash::Solid,
            bold: false,
            line: t.line,
        };
        let (home, rest) = from.split_at(from.len().checked_sub(1)?);
        if to.len() == from.len() && to.starts_with(home) {
            arrow.from = rest[0];
            arrow.to = *to.last()?;
            container_mut(&mut root, home)?.edges.push(arrow);
            continue;
        }
        // A state and one inside it: PlantUML's reader refuses that too.
        let (depth, a, b) = meeting(from, to)?;
        arrow.from = a;
        arrow.to = b;
        let crossing = Crossing {
            from: from[depth..].to_vec(),
            to: to[depth..].to_vec(),
            arrow,
        };
        container_mut(&mut root, &from[..depth])?
            .crossings
            .push(crossing);
    }
    for note in &chart.notes {
        let path = &lowering.paths[note.target];
        let (home, at) = path.split_at(path.len().checked_sub(1)?);
        container_mut(&mut root, home)?.notes.push(Note {
            target: at[0],
            side: match note.side {
                mermaid::Side::Left => Side::Left,
                mermaid::Side::Right => Side::Right,
            },
            text: text(&note.text, note.line),
            line: note.line,
        });
    }
    Some(Diagram {
        root,
        frame: lowering.frame,
    })
}

struct Lowering<'a> {
    chart: &'a Chart,
    direction: Direction,
    mirrored: bool,
    /// Each state's path from the top: its index in each container on the
    /// way, a region counting as a node.
    paths: Vec<Vec<usize>>,
    frame: Frame,
    /// Each state's own look, by its index.
    looks: HashMap<usize, Look>,
}

impl Lowering<'_> {
    /// The container of the state `parent` (the diagram for `None`), at
    /// `path`: its states, or its regions when `--` divides it.
    fn container(&mut self, parent: Option<usize>, path: &mut Vec<usize>) -> Container {
        let members: Vec<usize> = (0..self.chart.states.len())
            .filter(|&i| self.chart.states[i].parent == parent)
            .collect();
        let mut regions: Vec<usize> = members
            .iter()
            .map(|&i| self.chart.states[i].region)
            .collect();
        regions.sort_unstable();
        regions.dedup();
        let mut container = Container {
            direction: self.direction,
            mirrored: self.mirrored,
            ..Container::default()
        };
        if regions.len() < 2 {
            self.fill(&mut container, &members, path);
            return container;
        }
        // Mermaid draws the regions of `--` side by side.
        container.split = Some(Split::Beside);
        for (at, region) in regions.into_iter().enumerate() {
            let held: Vec<usize> = members
                .iter()
                .copied()
                .filter(|&i| self.chart.states[i].region == region)
                .collect();
            let line = held.first().map_or(0, |&i| self.chart.states[i].line);
            let mut inner = Container {
                direction: self.direction,
                mirrored: self.mirrored,
                ..Container::default()
            };
            path.push(at);
            self.fill(&mut inner, &held, path);
            path.pop();
            container.nodes.push(Node {
                kind: Kind::Region,
                composite: true,
                children: inner,
                ..marker(Kind::Region, line)
            });
        }
        container
    }

    fn fill(&mut self, container: &mut Container, members: &[usize], path: &mut Vec<usize>) {
        for (at, &i) in members.iter().enumerate() {
            path.push(at);
            self.paths[i] = path.clone();
            let children = if self.chart.states[i].composite {
                self.container(Some(i), path)
            } else {
                Container::default()
            };
            container.nodes.push(self.node(i, children));
            path.pop();
        }
    }

    fn node(&mut self, i: usize, children: Container) -> Node {
        let s = &self.chart.states[i];
        let kind = match s.kind {
            mermaid::Kind::State => Kind::State,
            mermaid::Kind::Start => Kind::Start,
            mermaid::Kind::End => Kind::End,
            mermaid::Kind::Choice => Kind::Choice,
            mermaid::Kind::Fork => Kind::Fork,
            mermaid::Kind::Join => Kind::Join,
        };
        let mut node = match kind {
            Kind::Start | Kind::End => marker(kind, s.line),
            Kind::Fork | Kind::Join => Node {
                key: s.id.clone(),
                ..marker(kind, s.line)
            },
            _ => Node {
                key: s.id.clone(),
                label: text(s.label.as_deref().unwrap_or(&s.id), s.line),
                description: s.descriptions.iter().map(|d| text(d, s.line)).collect(),
                ..marker(kind, s.line)
            },
        };
        node.composite = s.composite;
        node.children = children;
        if let Some(look) = self.looks.get(&i) {
            node.fill.clone_from(&look.fill);
            node.line_color.clone_from(&look.stroke);
            if look.dashed {
                node.border_dash = Dash::Dashed;
            }
            if look.width.is_some() || look.color.is_some() {
                let name = format!("mermaid-style-{i}");
                let mut add = |key: &str, value: &str| {
                    self.frame.skinparams.push(Skinparam {
                        key: format!("{key}<<{name}>>"),
                        value: value.to_owned(),
                        line: s.line,
                        declared: None,
                    });
                };
                if let Some(width) = &look.width {
                    add("StateBorderThickness", width);
                }
                if let Some(color) = &look.color {
                    add("StateFontColor", color);
                }
                node.stereotype = Some(name);
            }
        }
        node
    }
}

fn marker(kind: Kind, line: usize) -> Node {
    Node {
        key: String::new(),
        label: Text {
            value: String::new(),
            line,
        },
        description: Vec::new(),
        kind,
        fill: None,
        line_color: None,
        border_dash: Dash::Solid,
        border_bold: false,
        stereotype: None,
        composite: false,
        children: Container::default(),
        line,
    }
}

fn container_mut<'a>(root: &'a mut Container, path: &[usize]) -> Option<&'a mut Container> {
    let mut here = root;
    for &at in path {
        here = &mut here.nodes.get_mut(at)?.children;
    }
    Some(here)
}

/// A state's colors from its classes and its `style` lines, in the order
/// Mermaid applies them: the classes as given, then the styles.
#[derive(Default)]
struct Look {
    fill: Option<String>,
    stroke: Option<String>,
    width: Option<String>,
    color: Option<String>,
    dashed: bool,
}

fn looks(chart: &Chart) -> HashMap<usize, Look> {
    let by_id: HashMap<&str, usize> = chart
        .states
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.id.is_empty())
        .map(|(i, s)| (s.id.as_str(), i))
        .collect();
    let mut styles: HashMap<usize, Vec<&str>> = HashMap::new();
    for (id, class) in &chart.classes {
        let def = chart
            .class_defs
            .iter()
            .rev()
            .find(|(name, _)| name == class);
        if let (Some(&i), Some((_, style))) = (by_id.get(id.as_str()), def) {
            styles.entry(i).or_default().push(style);
        }
    }
    for (id, style) in &chart.styles {
        if let Some(&i) = by_id.get(id.as_str()) {
            styles.entry(i).or_default().push(style);
        }
    }
    styles
        .into_iter()
        .map(|(i, list)| {
            let mut look = Look::default();
            for style in list {
                for part in style.split(',') {
                    let Some((key, value)) = part.split_once(':') else {
                        continue;
                    };
                    let (key, value) = (key.trim(), value.trim());
                    if value.is_empty() {
                        continue;
                    }
                    let color = || {
                        if value.eq_ignore_ascii_case("none") {
                            "transparent".to_owned()
                        } else {
                            value.to_owned()
                        }
                    };
                    match key {
                        "fill" => look.fill = Some(color()),
                        "stroke" => look.stroke = Some(color()),
                        "stroke-width" => {
                            look.width = Some(value.trim_end_matches("px").trim().to_owned());
                        }
                        "color" => look.color = Some(color()),
                        "stroke-dasharray" => {
                            look.dashed = !value.eq_ignore_ascii_case("none") && value != "0";
                        }
                        _ => {}
                    }
                }
            }
            (i, look)
        })
        .collect()
}

/// `title:` in a front matter at the top.
fn front_title(source: &str) -> Option<Text> {
    let mut lines = source.split('\n').enumerate();
    let mut open = false;
    for (no, line) in lines.by_ref() {
        let line = line.trim();
        if line.is_empty() && !open {
            continue;
        }
        if line == "---" {
            if open {
                return None;
            }
            open = true;
            continue;
        }
        if !open {
            return None;
        }
        if let Some(title) = line.strip_prefix("title:") {
            let title = title.trim().trim_matches('"').trim();
            return (!title.is_empty()).then(|| text(title, no));
        }
    }
    None
}

/// Words as Mermaid writes them, in the engine's markup: `<br>` breaks
/// the line, `<b>` and `<i>` (`<strong>`, `<em>`) are bold and italic,
/// other tags go, entity codes (`#quot;`, `&amp;`, `#9829;`) are their
/// characters, and what is markup to PlantUML (`**`, `//`, `--`, `""`,
/// `__`, `<color>`, `[[…]]`, `\n`) is drawn as it stands.
pub(crate) fn text(raw: &str, line: usize) -> Text {
    Text {
        value: markup(raw),
        line,
    }
}

fn markup(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(close) = rest.find('>')
        {
            let tag = rest[1..close].trim().to_ascii_lowercase();
            let name = tag
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or_default();
            let known = name.chars().all(|c| c.is_ascii_alphanumeric()) && !name.is_empty();
            if known {
                match name {
                    "br" => out.push('\n'),
                    "b" | "strong" => out.push_str("**"),
                    "i" | "em" => out.push_str("//"),
                    _ => {}
                }
                rest = &rest[close + 1..];
                continue;
            }
        }
        if let Some((decoded, len)) = entity(rest) {
            escape(decoded, &mut out);
            rest = &rest[len..];
            continue;
        }
        escape(c, &mut out);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// `c` as written: a character the engine reads as markup gets its `~`.
fn escape(c: char, out: &mut String) {
    if matches!(c, '*' | '/' | '"' | '-' | '_' | '<' | '[' | '\\') {
        out.push('~');
    }
    out.push(c);
}

/// The character an entity code at the start of `rest` stands for, and
/// its length: Mermaid's `#name;` and `#123;`, and HTML's `&name;` and
/// `&#123;`.
fn entity(rest: &str) -> Option<(char, usize)> {
    let lead = rest.chars().next()?;
    if lead != '#' && lead != '&' {
        return None;
    }
    let end = rest[..rest.len().min(12)].find(';')?;
    let body = rest[1..end].trim_start_matches('#');
    let c = match body {
        "quot" => '"',
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        digits if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) => {
            char::from_u32(digits.parse().ok()?)?
        }
        _ => return None,
    };
    Some((c, end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lowered(source: &str) -> Diagram {
        from_mermaid(source).unwrap_or_else(|| panic!("not read: {source}"))
    }

    fn keys(container: &Container) -> Vec<&str> {
        container.nodes.iter().map(|n| n.key.as_str()).collect()
    }

    #[test]
    fn states_nest_and_regions_split_their_state() {
        let d = lowered(
            "stateDiagram-v2\n[*] --> A\nstate A {\n  [*] --> B\n  --\n  [*] --> C\n}\nA --> D\n",
        );
        assert_eq!(keys(&d.root), ["", "A", "D"]);
        let a = &d.root.nodes[1];
        assert!(a.composite);
        assert_eq!(a.children.split, Some(Split::Beside));
        assert!(a.children.nodes.iter().all(|n| n.kind == Kind::Region));
        assert_eq!(keys(&a.children.nodes[0].children), ["", "B"]);
        assert_eq!(keys(&a.children.nodes[1].children), ["", "C"]);
        let edges: Vec<(usize, usize)> = d.root.edges.iter().map(|e| (e.from, e.to)).collect();
        assert_eq!(edges, [(0, 1), (1, 2)]);
        assert_eq!(a.children.nodes[0].children.edges.len(), 1);
    }

    #[test]
    fn a_transition_between_containers_is_a_crossing() {
        let d = lowered(
            "stateDiagram-v2\nstate On {\n  Idle --> Busy\n}\nOff --> Idle : wake\nBusy --> Off\n",
        );
        assert_eq!(keys(&d.root), ["On", "Off"]);
        let crossings: Vec<(&[usize], &[usize])> = d
            .root
            .crossings
            .iter()
            .map(|c| (c.from.as_slice(), c.to.as_slice()))
            .collect();
        assert_eq!(
            crossings,
            [(&[1][..], &[0, 0][..]), (&[0, 1][..], &[1][..])]
        );
        assert_eq!(
            d.root.crossings[0].arrow.label.as_ref().unwrap().value,
            "wake"
        );
        // A state and one it holds: refused, as PlantUML's reader does.
        assert!(from_mermaid("stateDiagram\nstate S {\n  A --> B\n}\nS --> A\n").is_none());
    }

    #[test]
    fn notes_directions_titles_and_styles_come_along() {
        let d = lowered(
            "---\ntitle: Heater\n---\nstateDiagram-v2\ndirection RL\nclassDef hot fill:#f96,stroke:#333,stroke-width:3px,color:#fff\nCold --> Hot\nHot:::hot\nstyle Cold stroke-dasharray:4 2\nnote left of Cold : brr\n",
        );
        assert_eq!(d.frame.title.as_ref().unwrap().value, "Heater");
        assert_eq!(d.root.direction, Direction::Right);
        assert!(d.root.mirrored);
        let (cold, hot) = (&d.root.nodes[0], &d.root.nodes[1]);
        assert_eq!(cold.border_dash, Dash::Dashed);
        assert_eq!(hot.fill.as_deref(), Some("#f96"));
        assert_eq!(hot.line_color.as_deref(), Some("#333"));
        let stereotype = hot.stereotype.clone().unwrap();
        let params: Vec<(&str, &str)> = d
            .frame
            .skinparams
            .iter()
            .map(|p| (p.key.as_str(), p.value.as_str()))
            .collect();
        assert_eq!(
            params,
            [
                (
                    format!("StateBorderThickness<<{stereotype}>>").as_str(),
                    "3"
                ),
                (format!("StateFontColor<<{stereotype}>>").as_str(), "#fff"),
            ]
        );
        assert_eq!(d.root.notes[0].target, 0);
        assert_eq!(d.root.notes[0].side, Side::Left);
    }

    #[test]
    fn words_keep_what_mermaid_shows() {
        assert_eq!(markup("one<br/>two"), "one\ntwo");
        assert_eq!(markup("<b>bold</b> and <i>it</i>"), "**bold** and //it//");
        assert_eq!(markup("a -- b ** c // d"), "a ~-~- b ~*~* c ~/~/ d");
        assert_eq!(markup("#quot;hi#quot; &amp; #9829;"), "~\"hi~\" & ♥");
        assert_eq!(markup("<span class=\"x\">kept</span>"), "kept");
        assert_eq!(markup("a < b"), "a ~< b");
        let block = crate::text::block(
            &markup("**a** -- //b//"),
            crate::text::Font::new(12.0, crate::Color::BLACK),
            crate::text::Family::Sans,
        )
        .unwrap();
        let shown: String = block.lines[0]
            .runs
            .iter()
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(shown, "**a** -- //b//");
    }
}
