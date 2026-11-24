//! What a finding asks of whoever puts it right, beside its words: for a
//! trace that deviates from its state machine, what happened, what the
//! machine expected there, and where each is written (docs/trace.md,
//! docs/adr/0036). An agent reads these fields, not the message, to fix
//! the code that wrote the trace or to propose a change to the diagram.

use serde::Serialize;

/// A deviation of a trace from its machine, as a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Task {
    /// The object of the trace whose line deviates.
    pub object: String,
    /// The file of the state machine it goes by.
    pub machine: String,
    pub happened: Happened,
    pub expected: Expected,
}

/// What the trace's line says happened.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Happened {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
}

/// What the machine expected there: the states the object can be in by
/// then, and the events those states take.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Expected {
    pub states: Vec<Place>,
    pub events: Vec<Offer>,
}

/// A state of the machine, with the line of the diagram it is first
/// written on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Place {
    pub name: String,
    /// Zero-based; one-based in a report.
    pub line: usize,
}

/// An event a state takes: where it leads, and the line of the diagram
/// its transition is written on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Offer {
    pub event: String,
    pub to: String,
    /// Zero-based; one-based in a report.
    pub line: usize,
}

impl Task {
    /// The task with its lines counted from one, as a report writes them.
    #[must_use]
    pub fn one_based(mut self) -> Self {
        for place in &mut self.expected.states {
            place.line += 1;
        }
        for offer in &mut self.expected.events {
            offer.line += 1;
        }
        self
    }
}
