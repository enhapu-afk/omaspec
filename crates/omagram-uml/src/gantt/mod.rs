//! Gantt charts. The days are laid out on one scale, so this module does
//! not use [`crate::graph`].
//!
//! A task's `ends` date is its last day: the bar runs up to the next day,
//! which is where the following task starts. Anything outside the everyday
//! subset declines and names the line. Mermaid `gantt` is read by
//! [`from_mermaid`] into the same chart.

mod date;
mod draw;
mod mermaid;
mod parse;
mod skin;

pub use crate::common::Skinparam;
pub use draw::draw;
pub use mermaid::{from_mermaid, mermaid_refusal};
pub use parse::parse;

use crate::color::Color;
use crate::common::Frame;

/// A parsed Gantt chart, with every task's dates worked out.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub rows: Vec<Row>,
    /// Rata die of day 0 when the chart has a real date. `None` when every
    /// time is `D+n` or a bare duration.
    pub origin: Option<i64>,
    /// Absolute days (`origin` plus a bar offset) that a Mermaid duration
    /// skips. The column stays, shaded, so the date on the axis is the real
    /// one. Empty when every column is a working day, which is every
    /// PlantUML chart: closed days there still decline.
    pub closed: Vec<i64>,
    /// `zoom`, default 1. One day is `16 * zoom` pixels.
    pub zoom: f32,
    pub frame: Frame,
}

/// One row, top to bottom as the tasks and separators were first written.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Section { text: String, line: usize },
    Bar(Bar),
}

/// One task or milestone. `end` is exclusive; a milestone has `start == end`.
#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub key: String,
    pub label: String,
    pub line: usize,
    pub start: i64,
    /// Where the next task starts. Exclusive.
    pub end: i64,
    /// Where the painted bar stops. Exclusive, and equal to [`Self::end`]
    /// unless a Mermaid duration runs into excluded days: the bar does not
    /// cover those trailing days, and the next task still waits for them.
    pub shown_end: i64,
    pub milestone: bool,
    pub color: Option<Color>,
    /// `is 40% complete`, from 0 to 1. Mermaid's `done` tag is a [`Marks`]
    /// flag, not this share.
    pub done: Option<f32>,
    /// Mermaid `done`, `active` and `crit`. PlantUML leaves them unset.
    /// The drawer ignores them; the Mermaid paint step turns them into
    /// [`Self::color`].
    pub marks: Marks,
    /// The task this one follows with `->`. `then` does not set it, and
    /// neither does Mermaid `after`: Mermaid moves the bar and draws no arrow.
    pub link_from: Option<String>,
}

/// Mermaid status tags on one task. They combine: `crit, done` sets both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Marks {
    pub done: bool,
    pub active: bool,
    pub crit: bool,
}
