//! How much of a state machine recorded traces take: each transition and
//! each state, how often (`omaspec coverage`).
//!
//! The traces are followed as `omaspec trace` follows them, object by
//! object, through the machine their objects go by. Where a trace leaves
//! open which way the object went (an event several states it may be in
//! take, a choice it did not log), every way it could have gone counts,
//! as the player lights them all: the coverage errs towards more, as the
//! check errs towards a machine that can do more.

use crate::trace::{follow, objects};
use crate::{Kind, Spec};

/// The coverage of one machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Coverage {
    /// How often each transition was taken, by its place in the machine.
    pub transitions: Vec<u32>,
    /// How often each state was entered, or begun in, by its place.
    pub states: Vec<u32>,
    /// The objects of the traces that go by the machine, in the order
    /// they first appear.
    pub objects: Vec<String>,
    /// Lines the machine cannot take as written: where the check finds a
    /// deviation.
    pub deviations: usize,
}

impl Coverage {
    /// The transitions taken at least once, and all of them.
    pub fn transitions_covered(&self) -> (usize, usize) {
        let taken = self.transitions.iter().filter(|&&n| n > 0).count();
        (taken, self.transitions.len())
    }

    /// The same for the states that have a name (not a start, an end, a
    /// choice or a fork), by `spec`'s machine.
    pub fn states_covered(&self, spec: &Spec) -> (usize, usize) {
        let named: Vec<usize> = (0..spec.machine.states.len())
            .filter(|&s| spec.machine.states[s].kind == Kind::State)
            .collect();
        let taken = named.iter().filter(|&&s| self.states[s] > 0).count();
        (taken, named.len())
    }
}

/// How much of `spec`'s machine the `traces` take.
pub fn coverage(spec: &Spec, traces: &[&str]) -> Coverage {
    let mut out = Coverage {
        transitions: vec![0; spec.machine.transitions.len()],
        states: vec![0; spec.machine.states.len()],
        ..Coverage::default()
    };
    for trace in traces {
        for object in objects(trace, spec) {
            let followed = follow(trace, spec, &object);
            // Begun where the first line says, not by the start: those
            // states are entered by no transition.
            if followed.start.is_empty() {
                for &state in &followed.begins {
                    out.states[state] += 1;
                }
            }
            let taken = followed
                .start
                .iter()
                .chain(followed.steps.iter().flat_map(|step| &step.taken));
            for step in taken {
                out.transitions[step.transition] += 1;
                for &state in &step.entered {
                    out.states[state] += 1;
                }
            }
            out.deviations += followed
                .steps
                .iter()
                .filter(|step| step.finding.is_some())
                .count();
            if !out.objects.contains(&object) {
                out.objects.push(object);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use omaspec_core::Language;

    use super::*;

    const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";

    #[test]
    fn each_transition_and_state_counts_how_often_the_traces_take_it() {
        let spec = Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap();
        let night = "{\"object\":\"Door\",\"event\":\"open\"}\n{\"object\":\"Door\",\"event\":\"close\"}\n{\"object\":\"Gate\",\"machine\":\"Door\",\"event\":\"open\"}\n{\"object\":\"Window\",\"event\":\"open\"}\n";
        let day = "{\"object\":\"Door\",\"event\":\"open\"}\n{\"object\":\"Door\",\"event\":\"unlock\"}\n";
        let cov = coverage(&spec, &[night, day]);
        let by_line = |line: usize| {
            let t = spec
                .machine
                .transitions
                .iter()
                .position(|t| t.line == line)
                .unwrap();
            cov.transitions[t]
        };
        // Three objects start: Door twice (one in each trace) and Gate.
        assert_eq!(
            (by_line(2), by_line(3), by_line(4), by_line(5), by_line(6)),
            (3, 3, 1, 0, 0)
        );
        assert_eq!(cov.objects, ["Door", "Gate"]);
        assert_eq!(cov.deviations, 1, "unlock where the door is open");
        assert_eq!(cov.transitions_covered(), (3, 5));
        assert_eq!(cov.states_covered(&spec), (2, 3));
        let state = |name: &str| {
            cov.states[spec
                .machine
                .states
                .iter()
                .position(|s| s.name == name)
                .unwrap()]
        };
        assert_eq!((state("Closed"), state("Open"), state("Locked")), (4, 3, 0));
    }
}
