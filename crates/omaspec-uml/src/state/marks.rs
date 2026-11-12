//! Marks laid over a state machine before it is drawn: a transition or a
//! state taken, and how often, or never taken. The coverage of recorded
//! traces is drawn this way (`omaspec coverage`); the source stays as it
//! is.
//!
//! A transition is found by the line it is written on, a state by its
//! name. A taken one is drawn bold in green with how often after its
//! label (`open ×12`), one never taken dashed in an orange red. Both
//! colors are middle tones that read on a light page and on a dark one.

use std::collections::HashMap;

use super::{Container, Dash, Diagram, Kind, Transition};
use crate::common::text_at;

/// What was taken.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    /// By the zero-based line of the transition.
    pub transitions: HashMap<usize, Mark>,
    /// By the state's name.
    pub states: HashMap<String, Mark>,
    /// Said under the diagram when it has no caption of its own.
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Taken this many times.
    Taken(u32),
    Missed,
}

/// A transition or a state taken.
pub const TAKEN: &str = "#2F9E44";
/// One never taken.
pub const MISSED: &str = "#E8590C";

impl Diagram {
    /// Draws what `marks` say over the diagram.
    pub fn mark(&mut self, marks: &Marks) {
        mark_container(&mut self.root, marks);
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
        let named = !matches!(node.kind, Kind::Start | Kind::End | Kind::Region);
        match marks.states.get(&node.key).filter(|_| named) {
            Some(Mark::Taken(_)) => {
                node.line_color = Some(TAKEN.to_owned());
                node.border_bold = true;
            }
            Some(Mark::Missed) => {
                node.line_color = Some(MISSED.to_owned());
                node.border_dash = Dash::Dashed;
            }
            None => {}
        }
        mark_container(&mut node.children, marks);
    }
}

fn mark_transition(edge: &mut Transition, marks: &Marks) {
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
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Color, parse};

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
            caption: Some("3 of 5 transitions taken".to_owned()),
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
