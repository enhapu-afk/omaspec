//! Mermaid `erDiagram`.
//!
//! Entities are tables: the name, then each attribute on its own row.
//! A relationship is a line with a crow's foot at each end that has a
//! cardinality. `direction LR` grows to the right and `TB` downward;
//! `RL` and `BT` are the same picture mirrored. A style may set `fill`,
//! `stroke`, `stroke-width`, `stroke-dasharray` and `color`. Anything
//! else in a style, a cardinality this reader does not know, and a line
//! it cannot place are refused by name.

mod lay;
mod paint;
mod read;
mod route;
#[cfg(test)]
mod tests;
pub use paint::draw;

use crate::color::{Color, Paint};
use crate::common::Text;
use read::{Stop, read};

const PAD: f32 = 10.0;
const ROUND: f32 = 6.0;
const MARGIN: f32 = 12.0;
/// Room between two ranks, before a label asks for more.
const RANK: f32 = 72.0;
/// Room between two entities of one rank, before a label asks for more.
const STACK: f32 = 48.0;
/// Added to a label so the chip, the jog and both crow's feet stay out of the boxes.
const LABEL_PAD: f32 = 100.0;
/// A self-loop's first run past its box, its marks included; and how
/// much further each loop around it runs.
const LOOP: f32 = 36.0;
const LOOP_STEP: f32 = 10.0;

/// An ER diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    /// The way the ranks grow. `flip` mirrors that axis.
    pub axis: Axis,
    pub flip: bool,
    pub entities: Vec<Entity>,
    pub relations: Vec<Relation>,
    pub look: Look,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Right,
    Down,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub key: String,
    pub label: String,
    pub attributes: Vec<Attribute>,
    pub line: usize,
    pub fill: Option<Color>,
    pub border: Option<Color>,
    pub ink: Option<Color>,
    pub thickness: Option<f32>,
    pub dash: Option<(f32, f32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub type_name: String,
    pub name: String,
    pub keys: Vec<Key>,
    pub comment: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Pk,
    Fk,
    Uk,
}

impl Key {
    fn as_str(self) -> &'static str {
        match self {
            Key::Pk => "PK",
            Key::Fk => "FK",
            Key::Uk => "UK",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
    pub from_card: Option<Card>,
    pub to_card: Option<Card>,
    pub dashed: bool,
    pub line: usize,
}

/// Crow's foot. `None` on a relation is a plain line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    One,
    ZeroOne,
    Many,
    ZeroMany,
}

/// Colors the theme fills in. The reader leaves the Mermaid defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    /// Mermaid's `look: handDrawn`: shapes drawn by hand (docs/adr/0023).
    pub sketch: bool,
    pub font_size: f32,
    /// Theme text, before it is checked against a fill.
    pub preferred: Color,
    pub fill: Color,
    pub border: Color,
    pub line: Color,
    pub background: Paint,
    pub thickness: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            sketch: false,
            font_size: 14.0,
            preferred: Color::BLACK,
            fill: Color::hex(0xECECFF),
            border: Color::hex(0x7B88A8),
            line: Color::hex(0x2F3B4D),
            background: Some(Color::WHITE),
            thickness: 1.5,
        }
    }
}

/// The ER diagram `source` describes, or `None` when it is not an
/// `erDiagram` or it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not an ER diagram, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}
