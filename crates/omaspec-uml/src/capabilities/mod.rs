//! Capability maps (`@startcapabilities`). Omaspec's own syntax: PlantUML
//! has no such diagram, so nothing here is sent to the Java fallback.
//!
//! What a business can do, as tiles inside tiles: an area (`* …`), the
//! capabilities in it (`** …`), and theirs (`*** …`). Each may be given a
//! value on one scale (`levels Maturity : none, basic, managed, leading`),
//! after its last colon (`** Online shop : managed`), and is colored by
//! it; the legend under the map names the colors.
//!
//! ```text
//! @startcapabilities
//! levels Maturity : none, basic, managed, leading
//! * Customers
//! ** Marketing : basic
//! ** Sales
//! *** Online shop : leading
//! *** Stores : managed
//! @endcapabilities
//! ```

mod draw;
mod parse;

pub use draw::{draw, draw_mapped};
pub use parse::parse;

use crate::color::Color;
use crate::common::Frame;

/// The deepest a capability stands: an area and five levels in it.
pub const DEEPEST: usize = 6;

/// A parsed capability map.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    /// The scale the values are on, where the map has one.
    pub levels: Option<Levels>,
    /// The areas, in the order they are written.
    pub areas: Vec<Capability>,
    pub frame: Frame,
}

/// The scale the capabilities' values are on, its steps from the first
/// written to the last; the legend's title is its name.
#[derive(Debug, Clone, PartialEq)]
pub struct Levels {
    pub name: String,
    pub steps: Vec<Level>,
    /// Zero-based line.
    pub line: usize,
    /// The byte on the line where its name starts.
    pub col: usize,
}

/// A step of the scale, in a color of its own where the file gives one.
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    pub name: String,
    pub color: Option<Color>,
    /// The byte on the scale's line where its name starts.
    pub col: usize,
}

/// What the business can do, and what it is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub name: String,
    /// Its step on the scale: an index into [`Levels::steps`].
    pub value: Option<usize>,
    pub line: usize,
    /// The byte on the line where its name starts: a value may follow it.
    pub col: usize,
    pub children: Vec<Capability>,
}
