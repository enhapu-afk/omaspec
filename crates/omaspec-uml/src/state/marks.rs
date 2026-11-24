//! Marks laid over a state machine before it is drawn; the source stays
//! as it is. The coverage of recorded traces (`omaspec coverage`): a
//! transition or a state taken, and how often, or never taken. What
//! changed since another version (`omaspec diff -o`): what came, what
//! changed, and what went, drawn where its two states still are.
//!
//! A transition is found by the line it is written on, a state by its
//! name. A taken one is drawn bold in green with how often after its
//! label (`open ×12`), one never taken dashed in an orange red; one that
//! came bold in green, one that changed bold in amber, one that went
//! dashed in the orange red, its label struck through. The colors are
//! middle tones that read on a light page and on a dark one.

use std::collections::HashMap;

use super::{Container, Dash, Diagram, Kind, Transition};
use crate::common::text_at;

/// What was taken, or what changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    /// By the zero-based line of the transition.
    pub transitions: HashMap<usize, Mark>,
    /// By the state's name.
    pub states: HashMap<String, Mark>,
    /// Transitions that went, drawn between their states where both are
    /// still there.
    pub ghosts: Vec<Ghost>,
    /// Said under the diagram when it has no caption of its own.
    pub caption: Option<String>,
    /// A trace river: each way taken as wide as how often, against the way
    /// taken most; one never taken thin.
    pub river: bool,
    /// Where traces left the machine: the state's name, and how often.
    pub left: Vec<(String, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Taken this many times.
    Taken(u32),
    Missed,
    /// Came since the version compared with.
    Added,
    /// Changed since then.
    Changed,
}

/// A transition that went: from the state `from` to the state `to`, by
/// their names, with the label it had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ghost {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
}

/// A transition or a state taken, or one that came.
pub const TAKEN: &str = "#2F9E44";
/// One never taken, or one that went.
pub const MISSED: &str = "#E8590C";
/// One that changed.
pub const CHANGED: &str = "#C77700";

/// The words a mark line starts with, after its comment mark: the pane
/// hands marks to the engine as lines after the diagram, as it hands a
/// fold (`crate::fold`), and the file never holds them.
const MARK: &str = "mark (Omaspec)";

impl Marks {
    /// The lines that say these marks, in Mermaid's comments or
    /// PlantUML's: `' mark (Omaspec) line 4 taken 12`, `… state missed
    /// Idle`, `… caption 3 of 5 transitions taken`. Ghosts go by no line
    /// and are left out.
    pub fn lines(&self, mermaid: bool) -> String {
        let comment = if mermaid { "%%" } else { "'" };
        let said = |mark: &Mark| match mark {
            Mark::Taken(n) => format!("taken {n}"),
            Mark::Missed => "missed".to_owned(),
            Mark::Added => "added".to_owned(),
            Mark::Changed => "changed".to_owned(),
        };
        let mut out = String::new();
        let mut lines: Vec<_> = self.transitions.iter().collect();
        lines.sort_by_key(|(line, _)| **line);
        for (line, mark) in lines {
            out.push_str(&format!("{comment} {MARK} line {line} {}\n", said(mark)));
        }
        let mut states: Vec<_> = self.states.iter().collect();
        states.sort_by(|a, b| a.0.cmp(b.0));
        for (name, mark) in states {
            let name = name.replace(['\n', '\r'], " ");
            out.push_str(&format!("{comment} {MARK} state {} {name}\n", said(mark)));
        }
        if let Some(caption) = &self.caption {
            let caption = caption.replace(['\n', '\r'], " ");
            out.push_str(&format!("{comment} {MARK} caption {caption}\n"));
        }
        if self.river {
            out.push_str(&format!("{comment} {MARK} river\n"));
        }
        for (name, times) in &self.left {
            let name = name.replace(['\n', '\r'], " ");
            out.push_str(&format!("{comment} {MARK} left {times} {name}\n"));
        }
        out
    }

    /// The marks the lines of `source` say ([`Self::lines`]); `None` when
    /// it has none.
    pub fn read(source: &str) -> Option<Marks> {
        let mut marks = Marks::default();
        let mut any = false;
        for line in source.lines() {
            let text = line.trim();
            let Some(rest) = text
                .strip_prefix('\'')
                .or_else(|| text.strip_prefix("%%"))
                .map(str::trim_start)
                .and_then(|rest| rest.strip_prefix(MARK))
                .map(str::trim_start)
            else {
                continue;
            };
            let mark = |words: &mut std::str::SplitWhitespace| match words.next()? {
                "taken" => Some(Mark::Taken(words.next()?.parse().ok()?)),
                "missed" => Some(Mark::Missed),
                "added" => Some(Mark::Added),
                "changed" => Some(Mark::Changed),
                _ => None,
            };
            if let Some(rest) = rest.strip_prefix("line ") {
                let mut words = rest.split_whitespace();
                let Some(at) = words.next().and_then(|n| n.parse().ok()) else {
                    continue;
                };
                if let Some(mark) = mark(&mut words) {
                    marks.transitions.insert(at, mark);
                    any = true;
                }
            } else if let Some(rest) = rest.strip_prefix("state ") {
                let mut words = rest.split_whitespace();
                let Some(mark) = mark(&mut words) else {
                    continue;
                };
                let name = words.collect::<Vec<_>>().join(" ");
                if !name.is_empty() {
                    marks.states.insert(name, mark);
                    any = true;
                }
            } else if let Some(caption) = rest.strip_prefix("caption ") {
                marks.caption = Some(caption.trim().to_owned());
                any = true;
            } else if rest.trim() == "river" {
                marks.river = true;
                any = true;
            } else if let Some(rest) = rest.strip_prefix("left ") {
                let mut words = rest.split_whitespace();
                let Some(times) = words.next().and_then(|n| n.parse().ok()) else {
                    continue;
                };
                let name = words.collect::<Vec<_>>().join(" ");
                if !name.is_empty() {
                    marks.left.push((name, times));
                    any = true;
                }
            }
        }
        any.then_some(marks)
    }
}

impl Diagram {
    /// Draws what `marks` say over the diagram.
    pub fn mark(&mut self, marks: &Marks) {
        mark_container(&mut self.root, marks);
        if marks.river {
            let most = marks
                .transitions
                .values()
                .filter_map(|mark| match mark {
                    Mark::Taken(times) => Some(*times),
                    _ => None,
                })
                .max()
                .unwrap_or(1)
                .max(1);
            weigh(&mut self.root, marks, most);
        }
        for (name, times) in &marks.left {
            self.left_here(name, *times);
        }
        for ghost in &marks.ghosts {
            self.ghost(ghost);
        }
        if self.frame.caption.is_none()
            && let Some(caption) = &marks.caption
        {
            self.frame.caption = Some(text_at(caption, 0));
        }
    }
}

fn mark_container(container: &mut Container, marks: &Marks) {
    for edge in &mut container.edges {
        mark_transition(edge, marks);
    }
    for crossing in &mut container.crossings {
        mark_transition(&mut crossing.arrow, marks);
    }
    for node in &mut container.nodes {
        let named = !matches!(
            node.kind,
            Kind::Start | Kind::End | Kind::Region | Kind::Note | Kind::Data
        );
        match marks.states.get(&node.key).filter(|_| named) {
            // Whatever border the source gives it: only the dash says
            // never taken.
            Some(Mark::Taken(_)) => {
                node.line_color = Some(TAKEN.to_owned());
                node.border_bold = true;
                node.border_dash = Dash::Solid;
            }
            Some(Mark::Missed) => {
                node.line_color = Some(MISSED.to_owned());
                node.border_dash = Dash::Dashed;
                node.border_bold = false;
            }
            Some(mark @ (Mark::Added | Mark::Changed)) => {
                let color = if *mark == Mark::Added { TAKEN } else { CHANGED };
                node.line_color = Some(color.to_owned());
                node.border_bold = true;
                node.border_dash = Dash::Solid;
            }
            None => {}
        }
        mark_container(&mut node.children, marks);
    }
}

fn mark_transition(edge: &mut Transition, marks: &Marks) {
    // A hidden arrow is no transition, and stays hidden whatever its line
    // shares with one.
    if edge.dash == Dash::Hidden {
        return;
    }
    match marks.transitions.get(&edge.line) {
        Some(Mark::Taken(times)) => {
            edge.color = Some(TAKEN.to_owned());
            edge.bold = true;
            edge.dash = Dash::Solid;
            let count = format!("×{times}");
            edge.label = Some(match edge.label.take() {
                Some(label) => text_at(&format!("{} {count}", label.value), label.line),
                None => text_at(&count, edge.line),
            });
        }
        Some(Mark::Missed) => {
            edge.color = Some(MISSED.to_owned());
            edge.dash = Dash::Dashed;
            edge.bold = false;
        }
        Some(mark @ (Mark::Added | Mark::Changed)) => {
            let color = if *mark == Mark::Added { TAKEN } else { CHANGED };
            edge.color = Some(color.to_owned());
            edge.bold = true;
            edge.dash = Dash::Solid;
        }
        None => {}
    }
}

impl Diagram {
    /// Draws `ghost` between its two states, dashed in red, its label
    /// struck through; nothing where one of them is no state of the
    /// diagram any more. It goes by no line of the source.
    fn ghost(&mut self, ghost: &Ghost) {
        let (Some(from), Some(to)) = (
            path_of(&self.root, &ghost.from),
            path_of(&self.root, &ghost.to),
        ) else {
            return;
        };
        let arrow = |from: usize, to: usize| Transition {
            from,
            to,
            label: ghost
                .label
                .as_ref()
                .filter(|label| !label.is_empty())
                .map(|label| text_at(&format!("<s>{label}</s>"), crate::NO_LINE)),
            color: Some(MISSED.to_owned()),
            dash: Dash::Dashed,
            bold: false,
            line: crate::NO_LINE,
            label_before: None,
            toward: None,
            weight: None,
            note: None,
        };
        let home = &from[..from.len() - 1];
        if to.len() == from.len() && to.starts_with(home) {
            let (a, b) = (from[from.len() - 1], to[to.len() - 1]);
            container_at(&mut self.root, home).edges.push(arrow(a, b));
            return;
        }
        // One between a state and a state inside it too (REQ-072).
        if let Some((depth, a, b)) = crate::graph::across::holding(&from, &to) {
            container_at(&mut self.root, &from[..depth])
                .crossings
                .push(super::Crossing {
                    from: from[depth..].to_vec(),
                    to: to[depth..].to_vec(),
                    arrow: arrow(a, b),
                });
        }
    }
}

/// A trace river's ways: each taken one as wide as how often against the
/// one taken most (`most`), up to five times an arrow; one never taken
/// thin.
fn weigh(container: &mut Container, marks: &Marks, most: u32) {
    let weight = |edge: &Transition| match marks.transitions.get(&edge.line) {
        Some(Mark::Taken(times)) => Some(1.0 + 4.0 * (*times as f32 / most as f32).sqrt()),
        Some(Mark::Missed) => Some(0.6),
        _ => None,
    };
    for edge in &mut container.edges {
        edge.weight = weight(edge);
    }
    for crossing in &mut container.crossings {
        crossing.arrow.weight = weight(&crossing.arrow);
    }
    for node in &mut container.nodes {
        weigh(&mut node.children, marks, most);
    }
}

impl Diagram {
    /// Where traces left the machine at the state `name`, `times` of
    /// them: a note beside it in the color of what was missed.
    fn left_here(&mut self, name: &str, times: u32) {
        let Some(path) = path_of(&self.root, name) else {
            return;
        };
        let Some((&index, around)) = path.split_last() else {
            return;
        };
        let container = container_at(&mut self.root, around);
        // No line writes it: the marks lay it over the diagram.
        let line = crate::NO_LINE;
        let said = match times {
            1 => "✕ 1 trace left the machine here".to_owned(),
            n => format!("✕ {n} traces left the machine here"),
        };
        container.notes.push(super::Note {
            target: index,
            side: super::Side::Right,
            text: text_at(&format!("<color:{MISSED}>{said}</color>"), line),
            line,
        });
    }
}

/// The path to the state called `name`, by the indices of its containers.
fn path_of(container: &Container, name: &str) -> Option<Vec<usize>> {
    for (index, node) in container.nodes.iter().enumerate() {
        if node.key == name
            && !matches!(
                node.kind,
                Kind::Start | Kind::End | Kind::Region | Kind::Note | Kind::Data
            )
        {
            return Some(vec![index]);
        }
        if let Some(mut inner) = path_of(&node.children, name) {
            inner.insert(0, index);
            return Some(inner);
        }
    }
    None
}

fn container_at<'a>(container: &'a mut Container, path: &[usize]) -> &'a mut Container {
    match path.split_first() {
        Some((&index, rest)) => container_at(&mut container.nodes[index].children, rest),
        None => container,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Color, parse};

    /// A river's marks are written as lines and read back: each way as
    /// wide as how often against the one taken most, where traces left as
    /// a note in the color of what was missed.
    #[test]
    fn a_river_is_written_read_and_drawn() {
        let mut marks = Marks::default();
        marks.transitions.insert(2, Mark::Taken(9));
        marks.transitions.insert(3, Mark::Taken(1));
        marks.transitions.insert(4, Mark::Missed);
        marks.river = true;
        marks.left = vec![("Open".to_owned(), 2)];
        let lines = marks.lines(false);
        assert!(lines.contains("' mark (Omaspec) river\n"), "{lines}");
        assert!(lines.contains("' mark (Omaspec) left 2 Open\n"), "{lines}");
        assert_eq!(Marks::read(&lines), Some(marks.clone()));
        let source = format!(
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\n{lines}@enduml\n"
        );
        let crate::Diagram::State(mut d) = crate::parse(&source).unwrap() else {
            panic!("a state diagram");
        };
        d.mark(&marks);
        let weight = |line: usize| {
            d.root
                .edges
                .iter()
                .find(|e| e.line == line)
                .and_then(|e| e.weight)
        };
        assert_eq!(weight(2), Some(5.0));
        assert!(weight(3).unwrap() > 1.0 && weight(3).unwrap() < 5.0);
        assert_eq!(weight(4), Some(0.6));
        assert!(
            d.root
                .notes
                .iter()
                .any(|n| n.text.value.contains("2 traces left the machine here"))
        );
        // The note is laid over the diagram: no line writes it, so it is
        // tied to none, not to the line that first names Open, whose only
        // words are the transition's label.
        let (svg, map) = crate::render_mapped(&source, None).unwrap();
        assert!(svg.contains("2 traces left the machine here"), "{svg}");
        assert!(map.tied.iter().all(|t| t.line != crate::NO_LINE));
        assert_eq!(
            map.tied.iter().filter(|t| t.line == 2).count(),
            1,
            "{:?}",
            map.tied
        );
    }

    const DOOR: &str = "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";

    #[test]
    fn taken_and_missed_transitions_and_states_are_drawn_apart() {
        let marks = Marks {
            transitions: HashMap::from([
                (1, Mark::Taken(1)),
                (2, Mark::Taken(3)),
                (3, Mark::Taken(2)),
                (4, Mark::Missed),
                (5, Mark::Missed),
            ]),
            states: HashMap::from([
                ("Closed".to_owned(), Mark::Taken(3)),
                ("Open".to_owned(), Mark::Taken(3)),
                ("Locked".to_owned(), Mark::Missed),
            ]),
            ghosts: Vec::new(),
            caption: Some("3 of 5 transitions taken".to_owned()),
            river: false,
            left: Vec::new(),
        };
        let (svg, _) = crate::render_marked(DOOR, None, &marks).unwrap();
        for words in [
            "open ×3",
            "close ×2",
            "×1",
            "lock",
            "3 of 5 transitions taken",
        ] {
            assert!(svg.contains(words), "{words}: {svg}");
        }
        assert!(!svg.contains("lock ×"), "{svg}");
        assert!(svg.contains(&TAKEN.to_ascii_uppercase()), "{svg}");
        assert!(svg.contains(&MISSED.to_ascii_uppercase()), "{svg}");
        assert!(svg.contains("stroke-dasharray"), "{svg}");
        // Not a state machine: nothing to mark.
        let err = crate::render_marked("@startuml\nA -> B\n@enduml\n", None, &marks).unwrap_err();
        assert!(err.reason.contains("state"), "{}", err.reason);
        // Handed over as lines after the diagram, as the pane does: the
        // same picture.
        let lines = marks.lines(false);
        assert!(
            lines.contains("' mark (Omaspec) line 2 taken 3\n"),
            "{lines}"
        );
        assert_eq!(Marks::read(&lines), Some(marks.clone()));
        let asked = DOOR.replace("@enduml\n", &format!("{lines}@enduml\n"));
        assert_eq!(crate::render(&asked).unwrap(), svg);
        assert_eq!(Marks::read(DOOR), None);
    }

    #[test]
    fn a_mark_says_taken_or_missed_whatever_border_the_source_gives() {
        let source = "@startuml\n[*] --> Cold\nstate Cold ##[dashed]blue\nstate Hot ##[bold]red\nCold --> Hot : heat\n@enduml\n";
        let crate::Diagram::State(mut d) = crate::parse(source).unwrap() else {
            panic!("a state diagram");
        };
        d.mark(&Marks {
            states: HashMap::from([
                ("Cold".to_owned(), Mark::Taken(1)),
                ("Hot".to_owned(), Mark::Missed),
            ]),
            ..Marks::default()
        });
        let node = |key: &str| d.root.nodes.iter().find(|n| n.key == key).unwrap();
        assert_eq!(
            (node("Cold").border_dash, node("Cold").border_bold),
            (Dash::Solid, true)
        );
        assert_eq!(
            (node("Hot").border_dash, node("Hot").border_bold),
            (Dash::Dashed, false)
        );
    }

    /// REQ-072: a transition that went between a state and the state it
    /// lies in is drawn as the others that went, inside the outer state.
    #[test]
    fn a_transition_that_went_into_its_own_state_is_drawn_too() {
        let source = "@startuml\n[*] --> Outer\nstate Outer {\n  [*] --> Inner\n}\n@enduml\n";
        let crate::Diagram::State(mut d) = crate::parse(source).unwrap() else {
            panic!("a state diagram");
        };
        let ghost = |from: &str, to: &str| Ghost {
            from: from.to_owned(),
            to: to.to_owned(),
            label: Some("gone".to_owned()),
        };
        d.mark(&Marks {
            ghosts: vec![ghost("Inner", "Outer"), ghost("Outer", "Inner")],
            ..Marks::default()
        });
        let nested: Vec<(&[usize], &[usize])> = d
            .root
            .crossings
            .iter()
            .map(|c| (c.from.as_slice(), c.to.as_slice()))
            .collect();
        assert_eq!(nested, [(&[1, 1][..], &[1][..]), (&[1][..], &[1, 1][..])]);
    }

    #[test]
    fn both_colors_read_on_a_light_page_and_a_dark_one() {
        let pages = [0xFFFFFF, 0xEFF1F5, 0x16181D, 0x2D353B].map(Color::hex);
        for mark in [TAKEN, MISSED] {
            let color = parse(mark).unwrap().unwrap();
            for page in pages {
                assert!(color.contrast(page) >= 3.0, "{mark} on {page}");
            }
        }
    }
}
