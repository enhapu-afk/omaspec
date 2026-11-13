//! Mermaid block diagrams (`block-beta`).
//!
//! Blocks sit on a column grid. A nested `block:` keeps its children inside
//! the frame, and the title stands in a band above them. An edge leaves the
//! border and takes the shortest orthogonal lane that misses every other
//! block and every title. A block arrow is a chevron.
//! On the stock theme a block is the familiar lilac tile. On any other
//! theme the tile is a mix of the accent onto the page, the stroke is the
//! accent, and the label follows the page when that still reads.

mod lay;
mod paint;
mod read;
mod route;
#[cfg(test)]
mod tests;
mod tokens;
pub use paint::draw;

use crate::color::Color;
use crate::common::Text;
use read::{Stop, read};

const CLASSIC_FILL: u32 = 0xECECFF;
const CLASSIC_BORDER: u32 = 0x7B88A8;
const NEST_LIMIT: usize = 100;

/// Which way a block arrow points. `x` and `y` have a head on both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Right,
    Left,
    Up,
    Down,
    BothAcross,
    BothDown,
}

/// The outline of one block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Rectangle,
    Round,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    DoubleCircle,
    Diamond,
    Hexagon,
    Parallelogram,
    ParallelogramAlt,
    Trapezoid,
    TrapezoidAlt,
    Asymmetric,
    Arrow(Direction),
}

/// Colors and type for one diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    /// Mermaid's `look: handDrawn`: shapes drawn by hand (docs/adr/0023).
    pub sketch: bool,
    pub font_size: f32,
    pub preferred: Color,
    pub background: Option<Color>,
    pub accent: Color,
    pub line: Color,
    pub classic: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            sketch: false,
            font_size: 16.0,
            preferred: Color::hex(0x333333),
            background: Some(Color::WHITE),
            accent: Color::hex(CLASSIC_BORDER),
            line: Color::hex(0x2F3B4D),
            classic: true,
        }
    }
}

/// One end of an edge: a block or a nested group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Node(usize),
    Group(usize),
}

/// One block diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    pub columns: Option<usize>,
    pub items: Vec<Item>,
    pub groups: Vec<Group>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub look: Look,
}

/// A column slot in a frame, in reading order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Node(usize),
    Group(usize),
    Space(usize),
}

/// A nested `block:` frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: String,
    pub label: String,
    pub span: usize,
    pub columns: Option<usize>,
    pub items: Vec<Item>,
    pub line: usize,
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub text: Option<Color>,
    pub stroke_width: Option<f32>,
}

/// One block.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: Shape,
    pub span: usize,
    pub line: usize,
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub text: Option<Color>,
    pub stroke_width: Option<f32>,
}

/// A line between two blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: End,
    pub to: End,
    pub label: Option<Text>,
    pub arrow_start: bool,
    pub arrow_end: bool,
    pub dotted: bool,
    pub thick: bool,
    pub line: usize,
}

/// The diagram `source` describes, or `None` when it is not one or it uses
/// a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a block diagram, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}
