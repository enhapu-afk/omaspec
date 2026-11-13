//! Mermaid C4 diagrams: context, container, component, dynamic and deployment.
//!
//! A person is a head and shoulders, a database is a cylinder and a queue
//! is a box with a bowed side. Everything else is a rounded box. Boundaries
//! are dashed and hold their children. On the stock Mermaid theme the boxes
//! keep the classic C4 blues, and a light fill gets dark text. On any other
//! theme the fills are steps of the accent toward the page, so a person, a
//! system, a container, a component and an external element stay apart.
//! A sprite is refused by name: the picture would otherwise drop the icon
//! and keep the rest. Links and tags are not drawn. A relationship that
//! would cross another box is lifted into a free band above the row.

mod args;
mod lay;
mod paint;
mod read;
mod route;
#[cfg(test)]
mod tests;
pub(crate) use args::{Args, get, norm_name, parse_args, split_args};
pub use paint::draw;

use crate::color::{Color, Paint};
use crate::common::Text;
use read::{Stop, read};

const INNER: f32 = 14.0;
const MARGIN: f32 = 16.0;
const ROUND: f32 = 8.0;
const BOUND_ROUND: f32 = 12.0;
const HEADS: [&str; 5] = [
    "C4Context",
    "C4Container",
    "C4Component",
    "C4Dynamic",
    "C4Deployment",
];

/// A C4 diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    pub shapes: Vec<Shape>,
    pub boundaries: Vec<Boundary>,
    pub rels: Vec<Rel>,
    pub shapes_per_row: usize,
    pub boundaries_per_row: usize,
    pub look: Look,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub id: String,
    pub label: String,
    pub kind: Kind,
    pub type_label: Option<String>,
    pub techn: Option<String>,
    pub descr: Option<String>,
    pub parent: Option<usize>,
    pub fill: Option<Color>,
    pub border: Option<Color>,
    pub ink: Option<Color>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Boundary {
    pub id: String,
    pub label: String,
    pub kind_name: String,
    pub descr: Option<String>,
    pub parent: Option<usize>,
    pub fill: Option<Color>,
    pub border: Option<Color>,
    pub ink: Option<Color>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rel {
    pub kind: RelKind,
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub techn: Option<String>,
    pub descr: Option<String>,
    pub offset_x: f32,
    pub offset_y: f32,
    pub line_color: Option<Color>,
    pub text_color: Option<Color>,
    pub line: usize,
}

/// What a box stands for. The external forms are dashed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Person,
    PersonExt,
    System,
    SystemDb,
    SystemQueue,
    SystemExt,
    SystemDbExt,
    SystemQueueExt,
    Container,
    ContainerDb,
    ContainerQueue,
    ContainerExt,
    ContainerDbExt,
    ContainerQueueExt,
    Component,
    ComponentDb,
    ComponentQueue,
    ComponentExt,
    ComponentDbExt,
    ComponentQueueExt,
}

/// How a relationship is drawn. The four directions leave that side of the
/// source; a back relationship is dashed; a bi-relationship has two heads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelKind {
    Rel,
    Bi,
    Up,
    Down,
    Left,
    Right,
    Back,
}

/// Colors the theme fills in. `classic` keeps the C4 blues.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    /// Mermaid's `look: handDrawn`: shapes drawn by hand (docs/adr/0023).
    pub sketch: bool,
    pub font_size: f32,
    pub preferred: Color,
    pub background: Paint,
    pub accent: Color,
    pub line: Color,
    pub classic: bool,
    pub thickness: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            sketch: false,
            font_size: 14.0,
            preferred: Color::hex(0x333333),
            background: Some(Color::WHITE),
            accent: Color::hex(0x7B88A8),
            line: Color::hex(0x2F3B4D),
            classic: true,
            thickness: 1.5,
        }
    }
}

/// The C4 diagram `source` describes, or `None` when it is not one or it
/// uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a C4 diagram, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}
