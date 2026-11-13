//! Mermaid architecture diagrams (`architecture-beta`).
//!
//! Services sit on a grid the ports decide: `db:L -- R:server` puts the
//! server on the left of the database. A group is a dashed frame around
//! its members, and its title stands in a band above them so the frame
//! does not cover a sibling. A junction is a dot. The five built-in icons
//! are drawn as marks. An icon pack
//! (`logos:…`) is refused by name, because a made-up glyph would be a
//! worse picture than the one mmdr draws for that pack.
//! On the stock theme a service is the familiar architecture blue. On any
//! other theme the tile is the accent and the label follows the page.

mod lay;
mod paint;
mod read;
mod route;
#[cfg(test)]
mod tests;
pub use paint::draw;

use crate::color::Color;
use crate::common::Text;
use read::{Stop, read};

const TILE: f32 = 56.0;
const JUNCTION: f32 = 14.0;
const MARGIN: f32 = 16.0;
const CLASSIC_TILE: u32 = 0x087EBF;

/// How a service or a group is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Cloud,
    Database,
    Disk,
    Internet,
    Server,
    Plain,
}

/// Which side of a service an edge leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
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
            accent: Color::hex(0x7B88A8),
            line: Color::hex(0x2F3B4D),
            classic: true,
        }
    }
}

/// One architecture diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    pub groups: Vec<Group>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub look: Look,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: String,
    pub label: String,
    pub icon: Icon,
    pub parent: Option<usize>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub icon: Icon,
    pub junction: bool,
    pub parent: Option<usize>,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub from_port: Side,
    pub to_port: Side,
    pub from_group: bool,
    pub to_group: bool,
    pub arrow_start: bool,
    pub arrow_end: bool,
    pub line: usize,
}

/// The diagram `source` describes, or `None` when it is not one or it uses
/// a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not an architecture diagram, and one it accepts, are
/// `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}
