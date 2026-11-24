//! A trace's deviation from its state machine as a task (docs/trace.md,
//! docs/adr/0036): what the line says happened, the states the object can
//! be in by then and the events they take, each with the line of the
//! diagram it is written on, so an agent can fix the code that wrote the
//! trace, or propose the transition the diagram lacks, without reading
//! the message.

use omaspec_core::lint::{Expected, Happened, Offer, Place, Task};

use crate::{Kind, Machine, Run};

/// The task of a deviation of `object` (by the machine `m` of `file`)
/// where `happened`, while it could be in any of `ways`.
pub(crate) fn task(
    m: &Machine,
    file: &str,
    object: &str,
    happened: Happened,
    ways: &[Run],
) -> Task {
    Task {
        object: object.to_owned(),
        machine: file.to_owned(),
        happened,
        expected: expected(m, ways),
    }
}

/// The states `ways` are in and the events they take, each once, in the
/// order the ways give them.
fn expected(m: &Machine, ways: &[Run]) -> Expected {
    let mut out = Expected::default();
    for run in ways {
        for &s in run.active() {
            if !matches!(m.states[s].kind, Kind::State | Kind::End) {
                continue;
            }
            let place = Place {
                name: named(m, s),
                line: m.states[s].line,
            };
            if !out.states.contains(&place) {
                out.states.push(place);
            }
        }
        for t in run.options(m) {
            let tr = &m.transitions[t];
            for event in &tr.trigger.events {
                let offer = Offer {
                    event: event.clone(),
                    to: named(m, tr.to),
                    line: tr.line,
                };
                if !out.events.contains(&offer) {
                    out.events.push(offer);
                }
            }
        }
    }
    out
}

/// A state as a trace names it (`state` takes its name, not its long
/// name), or what a mark is: `end`, `end of Brewing`.
fn named(m: &Machine, state: usize) -> String {
    match m.states[state].kind {
        Kind::State => m.states[state].name.clone(),
        _ => m.shown_name(state),
    }
}

#[cfg(test)]
mod tests {
    use omaspec_core::Language;
    use omaspec_core::lint::Severity;

    use crate::{Spec, Specs};

    /// Every deviation names both lines: the trace's, and the diagram's
    /// of each state and event it expected, within the diagram.
    #[test]
    fn every_deviation_is_a_task_with_the_lines_of_both() {
        let door = "@startuml Door\n[*] --> Closed\nClosed --> Open : open [unlocked]\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
        let specs = Specs::new(vec![
            Spec::read(door, Language::PlantUml, "door.puml").unwrap(),
        ]);
        let lines = door.lines().count();
        for trace in [
            // An event it cannot take where it is.
            "{\"object\":\"Door\",\"event\":\"lock\"}\n{\"object\":\"Door\",\"event\":\"open\"}",
            // A state it cannot be in.
            "{\"object\":\"Door\",\"event\":\"lock\",\"state\":\"Open\"}",
            // A state the machine does not have.
            "{\"object\":\"Door\",\"event\":\"open\"}\n{\"object\":\"Door\",\"state\":\"Ajar\"}",
        ] {
            let found = crate::check_trace(trace, &specs);
            let deviations: Vec<_> = found
                .iter()
                .filter(|f| f.code == "trace-deviation")
                .collect();
            assert!(!deviations.is_empty(), "{trace}: {found:?}");
            for finding in deviations {
                assert_eq!(finding.severity, Severity::Error);
                assert!(finding.line < trace.lines().count(), "{finding:?}");
                let task = finding.task.as_deref().expect("a task");
                assert_eq!(
                    (task.object.as_str(), task.machine.as_str()),
                    ("Door", "door.puml")
                );
                assert!(!task.expected.states.is_empty(), "{task:?}");
                for place in &task.expected.states {
                    assert!(place.line < lines, "{place:?}");
                }
                for offer in &task.expected.events {
                    assert!(offer.line < lines, "{offer:?}");
                    assert!(door.lines().nth(offer.line).unwrap().contains(&offer.event));
                }
            }
        }
    }
}
