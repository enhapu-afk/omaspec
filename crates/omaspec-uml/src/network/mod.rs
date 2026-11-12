//! Network diagrams (nwdiag): networks drawn as bars across the picture,
//! one under another as written, the servers on them as boxes between
//! the bars, a line from each server to each network it is on with its
//! address beside it. `@startnwdiag`, or `nwdiag { … }` in an
//! `@startuml`, as PlantUML reads it.
//!
//! What is read: `network name { … }` with `address`, `color`,
//! `description` and `width = full`; servers with `[address = "…",
//! shape = database, description = "…", color = "…"]`, on a network or
//! on their own; `group { … }` with `color` and `description`, drawn as a
//! colored field behind its servers; and peer lines (`internet --
//! router`). PlantUML's styles reach it (`nwdiagDiagram { network { … }
//! server { … } arrow { … } group { … } }`), and so do the colors a pack
//! lays over it. Anything else declines and names the line.

mod draw;
mod parse;

pub use draw::draw;
pub use parse::{is_network, parse};

use crate::color::Color;
use crate::common::Frame;
use crate::structure::Shape;

/// A parsed network diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    /// Top to bottom, as written.
    pub networks: Vec<Network>,
    /// In the order they first appear.
    pub servers: Vec<Server>,
    pub groups: Vec<Group>,
    /// Lines between two servers (`a -- b`), with the line they are on.
    pub peers: Vec<(usize, usize, usize)>,
    pub frame: Frame,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    /// May be empty: `network { … }`.
    pub name: String,
    pub address: Option<String>,
    pub description: Option<String>,
    pub color: Option<Color>,
    /// `width = full`: across the whole picture.
    pub full: bool,
    /// The servers on it, with the address each has there.
    pub members: Vec<(usize, Option<String>)>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Server {
    pub name: String,
    /// Shown in place of the name.
    pub description: Option<String>,
    pub shape: Shape,
    pub color: Option<Color>,
    /// An address given outside a network: shown on its peer line.
    pub address: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub description: Option<String>,
    pub color: Option<Color>,
    pub members: Vec<usize>,
    pub line: usize,
}
