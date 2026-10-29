//! Signal-flow diagrams (`@startflow`). Omaspec's own syntax: PlantUML
//! has no such diagram, so nothing here is sent to the Java fallback.
//!
//! A block receives and sends named signals. The graph engine only ranks
//! the blocks left to right. The wires are drawn to the ports, because a
//! graph edge ends at the center of a box.

mod draw;
mod parse;
mod skin;

pub use draw::draw;
pub use parse::parse;

use crate::common::Frame;

/// A parsed signal flow.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub blocks: Vec<Block>,
    pub wires: Vec<Wire>,
    pub frame: Frame,
}

/// A named signal on the diagram's edge or on a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    pub line: usize,
}

/// One block. `label` is what is drawn; it starts as the name.
///
/// A block with inner `blocks` or `wires` is a subsystem: those wires run
/// among the inner blocks and this block's own ports, and the ports are
/// the signals that cross its border.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub label: String,
    pub line: usize,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub blocks: Vec<Block>,
    pub wires: Vec<Wire>,
}

/// A signal from an output to an input. The label is only the text after
/// `:`, when the line has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wire {
    pub from: End,
    pub to: End,
    pub label: Option<String>,
    pub line: usize,
}

/// One end of a wire: a boundary signal, or one port of a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    Boundary(String),
    Port { block: String, port: String },
}
