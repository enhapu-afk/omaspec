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
pub use draw::{draw, draw_mapped};
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
