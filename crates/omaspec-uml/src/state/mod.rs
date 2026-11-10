//! State diagrams: states, nested states, concurrent regions, choices,
//! forks and history, laid out by [`crate::graph`].
//!
//! Names are shared by the whole diagram, as in PlantUML: a transition may
//! join states in different containers. The container that holds both ends
//! keeps it as a [`Crossing`]; the nested states it runs into are laid out
//! with that container, and it runs through their ranks like any other.
//!
//! Mermaid state diagrams have a reader of their own ([`mermaid`]), and
//! [`from_mermaid`] lowers what it reads into this tree, so this drawer
//! draws them too.

mod draw;
mod lower;
pub mod mermaid;
mod parse;
mod skin;

pub use crate::common::{Frame, Skinparam, Text};
pub use draw::{draw, draw_mapped, label_by_hand};
pub use lower::from_mermaid;
pub use parse::{is_state, parse};

/// A parsed state diagram. The root is the diagram itself: it has no box.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub root: Container,
    pub frame: Frame,
}

/// The states and transitions written directly inside one state (or the
/// diagram). Nested states keep their own container.
#[derive(Debug, Clone, PartialEq)]
pub struct Container {
    pub nodes: Vec<Node>,
    pub edges: Vec<Transition>,
    /// Transitions between states held by different boxes of this
    /// container.
    pub crossings: Vec<Crossing>,
    pub notes: Vec<Note>,
    pub direction: Direction,
    /// Laid out against its direction: up for `Down`, leftwards for
    /// `Right` (Mermaid's `BT` and `RL`).
    pub mirrored: bool,
    /// Set when the state is split into concurrent regions: then every
    /// node here is a [`Kind::Region`].
    pub split: Option<Split>,
    /// Its states the writer held beside others ([`Container::hold`]).
    pub held: Vec<crate::graph::hold::Held>,
    /// When false, no line here bundles
    /// ([0022](../../../docs/adr/0022-lines-bundled.md)).
    pub bundled: bool,
    /// Members whose lines stay out of a bundle.
    pub apart: Vec<usize>,
}

impl Container {
    /// Gives each container the states of `held` it holds both of
    /// ([0021](../../../docs/adr/0021-boxes-held-in-place.md)), by their
    /// keys; returns the entries that name no state, or two in different
    /// containers. A start, an end or a history has no key to hold it by.
    pub fn hold(&mut self, held: &[(usize, crate::hand::Hold)]) -> Vec<(usize, String)> {
        crate::hand::boxes::hold(self, held)
    }

    /// Keeps the lines of `entries` apart
    /// ([0022](../../../docs/adr/0022-lines-bundled.md)). An entry that
    /// names no state is returned, to be said. A start, an end or a
    /// history has no key to keep apart.
    pub fn part(&mut self, entries: &[(usize, crate::hand::Apart)]) -> Vec<(usize, String)> {
        crate::hand::boxes::part(self, entries)
    }
}

impl Diagram {
    /// Folds the composite states named in `names` to their box
    /// (docs/adr/0024); the lines into them end on its border.
    pub fn fold(&mut self, names: &[String]) {
        crate::fold::fold(&mut self.root, &mut self.frame, names);
    }
}

impl Container {
    /// How many states and how many transitions it holds, at any depth.
    fn counts(&self) -> (usize, usize) {
        let mut states = 0;
        let mut transitions = self.edges.len() + self.crossings.len();
        for node in &self.nodes {
            if node.kind == Kind::State {
                states += 1;
            }
            let (s, t) = node.children.counts();
            states += s;
            transitions += t;
        }
        (states, transitions)
    }
}

impl crate::fold::Folds for Container {
    fn count(&self) -> usize {
        self.nodes.len()
    }

    fn key(&self, index: usize) -> &str {
        &self.nodes[index].key
    }

    fn composite(&self, index: usize) -> bool {
        self.nodes[index].composite
    }

    fn inner(&self, index: usize) -> &Container {
        &self.nodes[index].children
    }

    fn inner_mut(&mut self, index: usize) -> &mut Container {
        &mut self.nodes[index].children
    }

    fn close(&mut self, index: usize) {
        let node = &mut self.nodes[index];
        let (states, transitions) = node.children.counts();
        node.composite = false;
        node.children = Container::default();
        // What the box holds, as a line under its name.
        node.description.push(crate::common::text_at(
            &format!(
                "{} · {}",
                crate::fold::counted(states, "state"),
                crate::fold::counted(transitions, "transition")
            ),
            node.line,
        ));
    }

    fn crossings(&mut self) -> Vec<(&mut Vec<usize>, &mut Vec<usize>)> {
        self.crossings
            .iter_mut()
            .map(|c| (&mut c.from, &mut c.to))
            .collect()
    }

    fn settle(&mut self) {
        // Alike but for the line they were written on.
        let alike = |a: &Transition, b: &Transition| {
            Transition {
                line: 0,
                ..a.clone()
            } == Transition {
                line: 0,
                ..b.clone()
            }
        };
        let (direct, crossing): (Vec<Crossing>, Vec<Crossing>) =
            std::mem::take(&mut self.crossings)
                .into_iter()
                .partition(|c| c.from.len() == 1 && c.to.len() == 1);
        for c in crossing {
            if !self
                .crossings
                .iter()
                .any(|k| k.from == c.from && k.to == c.to && alike(&k.arrow, &c.arrow))
            {
                self.crossings.push(c);
            }
        }
        for c in direct {
            let edge = Transition {
                from: c.from[0],
                to: c.to[0],
                ..c.arrow
            };
            if !self.edges.iter().any(|e| alike(e, &edge)) {
                self.edges.push(edge);
            }
        }
    }
}

impl crate::hand::boxes::Members for Container {
    fn count(&self) -> usize {
        self.nodes.len()
    }

    fn key(&self, index: usize) -> &str {
        &self.nodes[index].key
    }

    fn inner(&self, index: usize) -> &Container {
        &self.nodes[index].children
    }

    fn inner_mut(&mut self, index: usize) -> &mut Container {
        &mut self.nodes[index].children
    }

    fn kept(&mut self) -> crate::hand::boxes::Kept<'_> {
        crate::hand::boxes::Kept {
            held: &mut self.held,
            bundled: &mut self.bundled,
            apart: &mut self.apart,
        }
    }
}

impl Default for Container {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            crossings: Vec::new(),
            notes: Vec::new(),
            direction: Direction::Down,
            mirrored: false,
            split: None,
            held: Vec::new(),
            bundled: true,
            apart: Vec::new(),
        }
    }
}

/// How concurrent regions sit: one under the other (`--`) or side by side
/// (`||`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Stacked,
    Beside,
}

/// Which way this container's states run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Down,
    Right,
}

/// One state, or a pseudo state (`[*]`, a choice, a bar, history).
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The name transitions use. Empty for `[*]`.
    pub key: String,
    /// What the box shows. A quoted name or an alias's long name.
    pub label: Text,
    pub description: Vec<Text>,
    pub kind: Kind,
    /// `#color` on the state, as written.
    pub fill: Option<String>,
    /// `##color` on the state, as written.
    pub line_color: Option<String>,
    pub border_dash: Dash,
    pub border_bold: bool,
    /// A stereotype of the writer's own (`<<phase>>`), as written. PlantUML
    /// does not show it; skinparams for it color the state.
    pub stereotype: Option<String>,
    /// `state Name { … }`. An empty body is still a composite.
    pub composite: bool,
    pub children: Container,
    /// `[[url]]` on its declaration: a click on the state follows it.
    pub link: Option<crate::text::Link>,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    State,
    Choice,
    Fork,
    Join,
    History,
    DeepHistory,
    Start,
    End,
    /// One concurrent region of a state: no box and no name, only what it
    /// holds.
    Region,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// `A --> B : label`.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub from: usize,
    pub to: usize,
    pub label: Option<Text>,
    pub color: Option<String>,
    pub dash: Dash,
    pub bold: bool,
    pub line: usize,
    /// The side of the line its label takes, where the writer chose one
    /// by hand ([`crate::hand::Label`]): before the line across the flow,
    /// or after it.
    pub label_before: Option<bool>,
}

/// A transition whose ends lie in different boxes of one container. The
/// paths lead from this container down to each end; `arrow.from` and
/// `arrow.to` are their first steps, the boxes here that hold the ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Crossing {
    pub from: Vec<usize>,
    pub to: Vec<usize>,
    pub arrow: Transition,
}

/// Where a note sits relative to its state. "Right" stays on the right
/// of the drawn box, including when the container runs left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub target: usize,
    pub side: Side,
    pub text: Text,
    pub line: usize,
}
