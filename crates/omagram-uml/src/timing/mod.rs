//! Timing diagrams. PlantUML lays these out itself, without Graphviz, so
//! this module does not use [`crate::graph`].
//!
//! Time is proportional: a constraint from `@0` to `@50` is as wide as the
//! gap says. The picture is the everyday subset of the language. Anything
//! else declines and names the line.

mod draw;
mod parse;
mod skin;

pub use draw::draw;
pub use parse::{is_timing, parse};

use crate::color::Color;
use crate::common::Frame;

/// A parsed timing diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub rows: Vec<Row>,
    pub messages: Vec<Message>,
    pub constraints: Vec<Constraint>,
    pub highlights: Vec<Highlight>,
    pub notes: Vec<Note>,
    pub hide_axis: bool,
    /// `scale X as Y pixels`, so one time unit is `Y/X` pixels.
    pub scale: Option<(f64, f64)>,
    pub frame: Frame,
}

/// One participant, top to bottom as written.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub key: String,
    pub label: String,
    pub kind: RowKind,
    /// Timed states, earliest first.
    pub states: Vec<State>,
    /// A state written before any `@`, drawn in a column before the axis.
    pub initial: Option<Mark>,
    /// Robust levels, top to bottom. From `has`, then first use.
    pub order: Vec<String>,
    pub clock: Option<Clock>,
    /// `between A and B` for an analog row. Absent means 0 to the largest value.
    pub analog: Option<(f64, f64)>,
    /// `#color` on the participant line.
    pub fill: Option<Color>,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Robust,
    Concise,
    Binary,
    Clock,
    Analog,
    Rectangle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clock {
    pub period: f64,
    pub pulse: f64,
    pub offset: f64,
}

/// Text of one state, without its time.
#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    pub name: String,
    pub color: Option<Color>,
    pub note: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub time: f64,
    pub mark: Mark,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub from: String,
    pub to: String,
    pub time: f64,
    pub to_time: f64,
    pub label: String,
    pub line: usize,
}

/// A `<->` constraint. `row` is set when the left time was `Name@…`.
#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub row: Option<String>,
    pub start: f64,
    pub end: f64,
    pub label: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Highlight {
    pub start: f64,
    pub end: f64,
    pub color: Option<Color>,
    pub caption: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub row: String,
    /// `note top of`, otherwise bottom.
    pub top: bool,
    pub text: String,
    pub line: usize,
}
