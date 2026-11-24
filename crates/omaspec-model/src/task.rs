//! A trace's deviation from its state machine as a task (docs/trace.md,
//! docs/adr/0036): what the line says happened, the states the object can
//! be in by then and the events they take, each with the line of the
//! diagram it is written on, so an agent can fix the code that wrote the
//! trace, or propose the transition the diagram lacks, without reading
//! the message.

use omaspec_core::lint::{Expected, Happened, Offer, Place, Task};

use crate::{Kind, Machine, Run, Spec};

/// The task of a deviation of `object`, by the machine of `spec`, where
/// `happened`, while it could be in any of `ways`. Its lines are the
/// file's, a Markdown document's too.
pub(crate) fn task(spec: &Spec, object: &str, happened: Happened, ways: &[Run]) -> Task {
    Task {
        object: object.to_owned(),
        machine: spec.file.clone(),
        happened,
        expected: expected(spec, ways),
    }
}

/// The states `ways` are in and the events they take, each once, in the
/// order the ways give them.
fn expected(spec: &Spec, ways: &[Run]) -> Expected {
    let m = &spec.machine;
    let mut out = Expected::default();
    for run in ways {
        for &s in run.active() {
            if !matches!(m.states[s].kind, Kind::State | Kind::End) {
                continue;
            }
            let place = Place {
                name: named(m, s),
                line: spec.line_in_file(m.states[s].line),
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
                    line: spec.line_in_file(tr.line),
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
            // So on the first line, where it begins.
            "{\"object\":\"Door\",\"state\":\"Ajar\"}",
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

        // Past the line's event: where `close` leads, not where it came
        // from.
        let found = crate::check_trace(
            "{\"object\":\"Door\",\"event\":\"open\"}\n{\"object\":\"Door\",\"event\":\"close\",\"state\":\"Ajar\"}",
            &specs,
        );
        let task = found[0].task.as_deref().unwrap();
        let states: Vec<&str> = task
            .expected
            .states
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(states, ["Closed"]);
    }

    /// A machine in a Markdown document tells the document's lines.
    #[test]
    fn a_document_s_machine_tells_the_document_s_lines() {
        let doc = "# Door\n\nIt locks.\n\n```plantuml\n@startuml Door\n[*] --> Closed\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n```\n";
        let blocks = omaspec_core::markdown::blocks(doc);
        let spec = Spec::read_in_document(&blocks[0], "door.md").unwrap();
        let specs = Specs::new(vec![spec]);
        let mut trace = String::from("{\"object\":\"x\",\"machine\":\"door\",\"event\":\"lock\"}");
        trace.push_str("\n{\"object\":\"x\",\"machine\":\"door\",\"event\":\"lock\"}");
        let found = crate::check_trace(&trace, &specs);
        let task = found
            .iter()
            .find_map(|f| f.task.as_deref())
            .expect("a task");
        let offer = &task.expected.events[0];
        assert_eq!(offer.event, "unlock");
        assert!(
            doc.lines()
                .nth(offer.line)
                .unwrap()
                .contains("Locked --> Closed : unlock")
        );
        let place = &task.expected.states[0];
        assert!(
            doc.lines().nth(place.line).unwrap().contains("Locked"),
            "{place:?}"
        );
    }
}
