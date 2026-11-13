//! Ditaa (`@startditaa`): a drawing in characters turned into a picture.
//! Boxes are found where `+`, `-` and `|` close a rectangle (`/` and `\`
//! for round corners, `=` and `:` for a dashed side); every other line of
//! those characters is a line between them, with a head where it meets
//! `<`, `>`, `^` or `v`; the rest is text where it was written. A box
//! takes a color (`cBLU`, `cRED`, `c1AB`) and a shape (`{d}` a document,
//! `{s}` storage, `{io}` input and output, `{c}` a choice, `{o}` an
//! ellipse, `{tr}` a trapezoid) from the code written inside it, which is
//! not shown. `ditaa(--no-shadows)` on the start line takes the shadows
//! away; other options are read and left as they are.

mod draw;
mod grid;

pub use draw::draw;
pub use grid::parse;

use crate::color::Color;
use crate::common::Frame;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub boxes: Vec<Box>,
    /// Lines between grid cells: from and to, dashed.
    pub lines: Vec<(Cell, Cell, bool)>,
    /// Arrow heads: the cell and the way they point.
    pub heads: Vec<(Cell, Point)>,
    /// Text by the cell it starts at.
    pub texts: Vec<(Cell, String)>,
    pub shadows: bool,
    pub frame: Frame,
}

/// A cell of the grid: its row and its column.
pub type Cell = (usize, usize);

/// A box by its corner cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Box {
    pub top: usize,
    pub left: usize,
    pub bottom: usize,
    pub right: usize,
    pub round: bool,
    pub dashed: bool,
    pub color: Option<Color>,
    pub shape: Shape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Rect,
    Document,
    Storage,
    Io,
    Choice,
    Ellipse,
    Trapezoid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Point {
    Left,
    Right,
    Up,
    Down,
}
