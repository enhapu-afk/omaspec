//! What a diagram means, whether it is written in PlantUML or in Mermaid:
//! one behavior model for both, and the checks a specification writer
//! wants on it (see `docs/adr/0007-behavior-specifications.md`).
//!
//! So far the model holds state machines. PlantUML state diagrams come
//! from the native engine's parse, Mermaid ones from the reading the
//! Mermaid drawer draws from; a diagram either reader does not fully
//! understand is not checked, so a finding is never a guess.
//!
//! No I/O and no GUI, like `omagram-core` and `omagram-uml`.

mod across;
mod check;
mod convert;
mod diff;
mod label;
mod mermaid;
mod plantuml;
mod run;
mod trace;

use omagram_core::lint::{self, Finding};
use omagram_core::{Kind as DiagramKind, Language, classify};
use omagram_uml::map::{Map, Shape, Spot};

pub use across::Spec;
pub use convert::{Converted, Notice, Refused, convert};
pub use diff::{Change, ChangeKind, diff};
pub use label::Trigger;
pub use run::{Run, Taken};
pub use trace::{Followed, TraceStep};

/// A state machine: its states, where each one sits, and its transitions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Machine {
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// The name transitions use; empty for a start or an end.
    pub name: String,
    pub kind: Kind,
    /// The composite state it sits in; `None` at the top.
    pub parent: Option<usize>,
    /// Which concurrent region of the parent holds it; 0 without regions.
    pub region: usize,
    /// Zero-based line where it first appears.
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    State,
    /// `[*]` as a source: where its container starts.
    Start,
    /// `[*]` as a target: where its container ends.
    End,
    Choice,
    Fork,
    Join,
    /// `[H]`: back to the state last left inside the state around it.
    History,
    /// `[H*]`: back to everything last left inside the state around it.
    DeepHistory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub from: usize,
    pub to: usize,
    pub trigger: Trigger,
    /// Zero-based line.
    pub line: usize,
}

impl Machine {
    /// The state machine `code` describes, if it is a state diagram the
    /// readers fully understand.
    pub fn read(code: &str, lang: Language) -> Option<Machine> {
        if classify(code, lang).kind != DiagramKind::State {
            return None;
        }
        match lang {
            Language::PlantUml => plantuml::read(code),
            Language::Mermaid => mermaid::read(code),
        }
    }

    /// The states that hold `state`, innermost first.
    pub fn ancestors(&self, state: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.states[state].parent, |&p| self.states[p].parent)
    }

    /// Whether other states sit inside `state`.
    pub fn is_composite(&self, state: usize) -> bool {
        self.states.iter().any(|s| s.parent == Some(state))
    }
}

/// Where the states and transitions of a machine are drawn, as far as the
/// picture's map tells.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Placed {
    /// By state index: its box, or its mark.
    pub states: Vec<Option<Spot>>,
    /// By transition index: from the source to the tip of the head.
    pub routes: Vec<Option<Vec<(f32, f32)>>>,
}

impl Machine {
    /// Finds each state and transition on `map`: a state by its name, the
    /// state it sits in, its line and its shape, a transition by its line.
    /// A start or an end may be drawn as a bar, as the Mermaid drawer draws
    /// one with several transitions.
    pub fn place(&self, map: &Map) -> Placed {
        let fits = |shape: Shape, kind: Kind| match kind {
            Kind::State => shape == Shape::State,
            Kind::Start => matches!(shape, Shape::Start | Shape::Bar),
            Kind::End => matches!(shape, Shape::End | Shape::Bar),
            Kind::Choice => shape == Shape::Choice,
            Kind::Fork | Kind::Join => shape == Shape::Bar,
            Kind::History | Kind::DeepHistory => shape == Shape::History,
        };
        let mut taken = vec![false; map.states.len()];
        let states = self
            .states
            .iter()
            .map(|s| {
                let parent = s.parent.map(|p| self.states[p].name.as_str());
                let at = (0..map.states.len()).find(|&c| {
                    let spot = &map.states[c];
                    !taken[c]
                        && spot.line == s.line
                        && spot.name == s.name
                        && spot.parent.as_deref() == parent
                        && fits(spot.shape, s.kind)
                })?;
                taken[at] = true;
                Some(map.states[at].clone())
            })
            .collect();
        let mut used = vec![false; map.transitions.len()];
        let routes = self
            .transitions
            .iter()
            .map(|t| {
                let at = (0..map.transitions.len())
                    .find(|&r| !used[r] && map.transitions[r].line == t.line)?;
                used[at] = true;
                Some(map.transitions[at].points.clone())
            })
            .collect();
        Placed { states, routes }
    }
}

/// The findings of a specification check on `code`, in line order.
pub fn check(code: &str, lang: Language) -> Vec<Finding> {
    Machine::read(code, lang)
        .map(|m| check::machine(&m))
        .unwrap_or_default()
}

/// Every finding for `code`: the lint's and the specification check's, in
/// line order.
pub fn findings(code: &str, lang: Language) -> Vec<Finding> {
    findings_with(code, lang, &[])
}

/// The findings of a recorded trace (JSON Lines) against the state
/// machines of its project, in line order.
pub fn check_trace(trace: &str, specs: &[Spec]) -> Vec<Finding> {
    trace::check(trace, specs)
}

/// The objects of a recorded trace that go by `spec`'s machine, in the
/// order they first appear.
pub fn trace_objects(trace: &str, spec: &Spec) -> Vec<String> {
    trace::objects(trace, spec)
}

/// The lines of `object` in a recorded trace, followed through `spec`'s
/// machine line by line, for a viewer to play.
pub fn follow_trace(trace: &str, spec: &Spec, object: &str) -> Followed {
    trace::follow(trace, spec, object)
}

/// Every finding for `code`, also against the state machines of the
/// project it belongs to (`specs`), in line order.
pub fn findings_with(code: &str, lang: Language, specs: &[Spec]) -> Vec<Finding> {
    let mut all = lint::lint(code, lang);
    all.extend(check(code, lang));
    all.extend(across::check(code, lang, specs));
    all.sort_by_key(|f| f.line);
    all
}
