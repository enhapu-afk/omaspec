//! Story maps (`@startstorymap`). Omaspec's own syntax: PlantUML has no
//! such diagram, so nothing here is sent to the Java fallback.
//!
//! The backbone of a product across the top: what its users do
//! (`activity`), and the steps of each under it (`step`). Under each step
//! its stories (`* …`), each in the release it goes into (`* … : MVP`).
//! The releases (`release MVP : what it is for`) are bands across the
//! whole map, top down in the order they are written; a story given none
//! stands in a band after them.
//!
//! ```text
//! @startstorymap
//! release MVP : sell the first book
//! release Later
//! activity Find a book
//!   step Search
//!     * By title : MVP
//!     * By author : Later
//! activity Buy
//!   step Pay
//!     * By card : MVP
//! @endstorymap
//! ```

mod draw;
mod parse;
mod skin;

pub use draw::{draw, draw_mapped};
pub use parse::parse;

use crate::common::Frame;

/// A parsed story map.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    /// The bands, top down.
    pub releases: Vec<Release>,
    /// The backbone, left to right.
    pub activities: Vec<Activity>,
    pub frame: Frame,
}

/// A release: a band across the map, with what goes into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub name: String,
    /// What it is for, said under its name: the words after its colon.
    pub goal: Option<String>,
    /// Zero-based line.
    pub line: usize,
    /// The byte on its line where its name starts: the goal after it
    /// closes the line.
    pub col: usize,
}

/// What the users do, across the top: its steps under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub name: String,
    pub line: usize,
    pub steps: Vec<Step>,
}

/// A step of an activity: a column of the map, its stories under it. A
/// step without a name holds the stories written under the activity
/// before its first step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub name: Option<String>,
    pub line: usize,
    pub stories: Vec<Story>,
}

/// A story under a step, in the release it goes into: an index into
/// [`Diagram::releases`]; none for one not planned yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Story {
    pub text: String,
    pub release: Option<usize>,
    pub line: usize,
}

impl Diagram {
    /// Every story, by its activity and step.
    pub fn stories(&self) -> impl Iterator<Item = &Story> {
        self.activities
            .iter()
            .flat_map(|a| a.steps.iter().flat_map(|s| s.stories.iter()))
    }
}
