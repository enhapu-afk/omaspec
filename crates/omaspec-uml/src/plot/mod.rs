//! PlantUML's charts (`@startchart`, since PlantUML 1.2026.0): bars,
//! lines, areas and scatter points over two axes, as PlantUML reads them
//! (REQ-103). The Java PlantUML hangs on a number no chart can draw
//! (plantuml/plantuml#3030) and fails on an axis that runs nowhere
//! (#3031); here both are a mistake on their line.
//!
//! What is read: `h-axis`, `v-axis` and `v2-axis`, each with a title in
//! quotes, categories in brackets or a range (`0 --> 100`), and after them
//! `spacing`, `ticks [0:"Poor", …]`, `label-top`, `label-right` and
//! `grid`; `bar`, `line`, `area` and `scatter`, each with a name in
//! quotes, its values (`[45, 62]`, or points `[(1, 2), (3, 4)]` for a line
//! or a scatter on a range), a color (`#3498db`, `#red`), `v2` for the
//! second axis, `<<class>>` for a style and `labels`; `orientation
//! horizontal`, `stackMode grouped|stacked`, `legend
//! left|right|top|bottom` and `annotation "text" at (x, y) <<arrow>>`.
//! Areas stack in the order written, as PlantUML stacks them. `<style>`
//! reaches `chartDiagram` and its `bar`, `line`, `area`, `scatter`,
//! `axis`, `grid`, `legend` and `annotation`, and a series by its class.
//! A series without a color takes the next of the charts' palette, which
//! reads on a light page and on a dark one.

mod axis;
mod draw;
mod legend;
mod look;
mod read;

pub use draw::draw;
pub use read::parse;

use crate::color::Color;
use crate::common::Frame;

/// A parsed chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    /// The axis across the bottom (`h-axis`).
    pub h: Axis,
    /// The axis up the left side (`v-axis`).
    pub v: Axis,
    /// The second value axis, on the right (`v2-axis`).
    pub v2: Option<Axis>,
    /// In the order written, which is the order they are drawn in.
    pub series: Vec<Series>,
    /// `orientation horizontal`: the categories down the v-axis, the bars
    /// across.
    pub horizontal: bool,
    /// `stackMode stacked`: the bars of a category one on another.
    pub stacked: bool,
    /// `legend …`: where the legend stands; none without it.
    pub legend: Option<Side>,
    pub annotations: Vec<Annotation>,
    pub frame: Frame,
}

/// An axis as written. One of the two holds the categories, or a range a
/// line's points stand on; the other the values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Axis {
    pub title: Option<String>,
    /// `[Q1, Q2, …]`.
    pub categories: Vec<String>,
    /// `0 --> 100`; from the values where none is written.
    pub range: Option<(f64, f64)>,
    /// `spacing 25`: the step between two ticks.
    pub spacing: Option<f64>,
    /// `ticks [0:"Poor", 50:"Average"]`: the ticks, each with its words.
    pub ticks: Vec<(f64, String)>,
    /// `label-top`: the title across the axis's far end, not along it.
    pub label_top: bool,
    /// `label-right`: the title at the axis's right end, not under it.
    pub label_right: bool,
    /// `grid`: a line across the chart at each tick.
    pub grid: bool,
    /// Zero-based; the chart's first line where none is written.
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bar,
    Line,
    Area,
    Scatter,
}

impl Kind {
    pub fn keyword(self) -> &'static str {
        match self {
            Kind::Bar => "bar",
            Kind::Line => "line",
            Kind::Area => "area",
            Kind::Scatter => "scatter",
        }
    }
}

/// A series' values: one for each category in turn, or points on a range.
#[derive(Debug, Clone, PartialEq)]
pub enum Values {
    Each(Vec<f64>),
    Points(Vec<(f64, f64)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub kind: Kind,
    /// Its name in the legend.
    pub name: Option<String>,
    pub values: Values,
    pub color: Option<Color>,
    /// `v2`: on the second value axis.
    pub v2: bool,
    /// `<<name>>`: the style class it takes.
    pub class: Option<String>,
    /// `labels`: its values written at them.
    pub labels: bool,
    pub line: usize,
}

/// Where the legend stands beside the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// `annotation "text" at (x, y)`: words at a place of the chart, with an
/// arrow to it under `<<arrow>>`.
#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub text: String,
    /// A category, or a place on a range.
    pub at: Place,
    pub value: f64,
    pub arrow: bool,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    Category(usize),
    At(f64),
}
