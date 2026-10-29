//! Class, component, use case, deployment and object diagrams, laid out
//! by [`crate::graph`]. A Mermaid `classDiagram` is read into the same
//! tree.
//!
//! Names belong to the whole diagram, as in PlantUML: a line may join two
//! elements that sit in different packages. The package that holds both
//! keeps that line as a [`Crossing`]; the packages it runs into are laid
//! out with that one, and it runs through their ranks like any other.

mod draw;
mod mermaid;
mod parse;
mod skin;

pub use crate::common::{Frame, Skinparam, Text};
pub use draw::draw;
pub use mermaid::{from_mermaid, mermaid_refusal};
pub use parse::{is_structure, parse};

/// A parsed structure diagram. The root is the page: it has no box.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub root: Group,
    pub frame: Frame,
}

/// The elements and lines written directly inside one package, node or
/// frame (or on the page). A nested package keeps its own group.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub nodes: Vec<Element>,
    pub edges: Vec<Link>,
    /// Lines whose ends lie in different boxes of this group.
    pub crossings: Vec<Crossing>,
    pub notes: Vec<Note>,
    pub direction: Direction,
}

impl Default for Group {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            crossings: Vec::new(),
            notes: Vec::new(),
            direction: Direction::Down,
        }
    }
}

/// Which way this group's elements run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Down,
    Right,
}

/// One element: a class, a use case, a node, a package, …
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// The name lines use. An alias, when one was written.
    pub key: String,
    /// What the box shows.
    pub label: Text,
    pub stereotypes: Vec<Text>,
    pub members: Vec<Member>,
    pub shape: Shape,
    /// `#color` on the element, as written.
    pub fill: Option<String>,
    /// `##color` on the element, as written.
    pub line_color: Option<String>,
    /// Mermaid `stroke-width`. `None` uses the skin's thickness.
    pub thickness: Option<f32>,
    /// Mermaid `color`: the text on this box. `None` uses the skin.
    pub text_color: Option<String>,
    /// Mermaid `stroke-dasharray`, the on and off lengths of the border.
    pub border_dash: Option<(f32, f32)>,
    /// Set when the element was named with a keyword or a bracket form.
    /// A name a line invented can still take a real declaration.
    pub explicit: bool,
    /// `package Name { … }` or `node Name { … }`. An empty body is still
    /// a container. A class body is members, not a container.
    pub composite: bool,
    pub children: Group,
    pub line: usize,
}

/// A field, a method, or a line between the two compartments.
#[derive(Debug, Clone, PartialEq)]
pub enum Member {
    Text {
        text: Text,
        italic: bool,
        underline: bool,
        slot: Slot,
    },
    /// `--` or `..` across the box.
    Rule(Dash),
    /// `== title ==`, centered between the fields.
    Heading(Text),
}

/// Which compartment a member belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Attribute,
    Method,
}

/// The picture of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Class,
    Abstract,
    Interface,
    Enum,
    Annotation,
    Component,
    UseCase,
    Actor,
    Node,
    Artifact,
    Cloud,
    Database,
    Folder,
    Frame,
    Rectangle,
    Object,
    Package,
    File,
    Card,
    Storage,
    Stack,
    Hexagon,
    Agent,
    /// A note with no element of its own (`note "…" as N`).
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// What sits at one end of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Head {
    #[default]
    None,
    /// `>` or `<`.
    Arrow,
    /// `<|` or `|>`, the inheritance triangle.
    Triangle,
    /// `o`, an empty diamond.
    Diamond,
    /// `*`, a filled diamond.
    DiamondFilled,
}

/// `A "1" --> "*" B : label`.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub label: Option<Text>,
    pub from_card: Option<Text>,
    pub to_card: Option<Text>,
    pub from_head: Head,
    pub to_head: Head,
    pub color: Option<String>,
    pub dash: Dash,
    pub bold: bool,
    pub line: usize,
}

/// A line whose ends lie in different boxes of one group. The paths lead
/// from this group down to each end; `arrow.from` and `arrow.to` are
/// their first steps, the boxes here that hold the ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Crossing {
    pub from: Vec<usize>,
    pub to: Vec<usize>,
    pub arrow: Link,
}

/// Where a note sits relative to its element. "Right" stays on the right
/// of the drawn box, including when the group runs left to right.
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
