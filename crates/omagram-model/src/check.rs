//! The checks on a state machine, worded for the person who writes the
//! specification: each names the state or event and says what to change.
//!
//! - a state no path from the start reaches (warning);
//! - a state nothing leaves (warning, with the `[*]` that makes an end
//!   explicit);
//! - the same event leaving a state or a choice for two different states,
//!   with no guard or the same guard to choose (error: the specification
//!   contradicts itself); a choice or a state that leads on without an
//!   event to two states that way (warning: it may be left open, and a play
//!   lets the writer pick); a transition written twice (warning);
//! - a composite state entered at its edge with no start inside
//!   (warning, at the place the start goes);
//! - no start at all, so reachability cannot be checked (info).

use std::collections::HashMap;

use omagram_core::lint::{Finding, Severity};

use crate::{Kind, Machine};

/// The findings for `m`, in line order.
pub fn machine(m: &Machine) -> Vec<Finding> {
    let mut out = Vec::new();
    let reached = reachable(m);
    match &reached {
        Some(reached) => unreachable(m, reached, &mut out),
        None => {
            if let Some(first) = m.states.iter().find(|s| s.kind == Kind::State) {
                out.push(finding(
                    first.line,
                    Severity::Info,
                    "No [*] --> … says where the machine starts, so whether every \
                     state can be reached is not checked.",
                ));
            }
        }
    }
    dead_ends(m, reached.as_deref(), &mut out);
    ambiguous(m, &mut out);
    entered_without_start(m, &mut out);
    // By line, an error before a warning on the same line.
    out.sort_by(|a, b| a.line.cmp(&b.line).then(b.severity.cmp(&a.severity)));
    out
}

fn finding(line: usize, severity: Severity, message: impl Into<String>) -> Finding {
    Finding {
        line,
        severity,
        message: message.into(),
    }
}

/// Which states a path from the top start reaches; `None` without a start.
///
/// Being in a state means being in every state around it, and entering a
/// composite state enters the start of each of its regions; a region with
/// no start (reported on its own) counts as entered everywhere. A
/// transition from a composite state leaves any state inside it. This errs
/// towards reaching too much, so a state reported unreachable surely is.
fn reachable(m: &Machine) -> Option<Vec<bool>> {
    let mut todo: Vec<usize> = (0..m.states.len())
        .filter(|&i| m.states[i].kind == Kind::Start && m.states[i].parent.is_none())
        .collect();
    if todo.is_empty() {
        return None;
    }
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); m.states.len()];
    for t in &m.transitions {
        out[t.from].push(t.to);
    }
    let mut inside: Vec<Vec<usize>> = vec![Vec::new(); m.states.len()];
    for (i, s) in m.states.iter().enumerate() {
        if let Some(parent) = s.parent {
            inside[parent].push(i);
        }
    }
    let mut reached = vec![false; m.states.len()];
    while let Some(state) = todo.pop() {
        if std::mem::replace(&mut reached[state], true) {
            continue;
        }
        todo.extend(&out[state]);
        todo.extend(m.ancestors(state));
        let children = &inside[state];
        for region in regions(m, children) {
            let members = children.iter().filter(|&&c| m.states[c].region == region);
            let starts: Vec<usize> = members
                .clone()
                .filter(|&&c| m.states[c].kind == Kind::Start)
                .copied()
                .collect();
            if starts.is_empty() {
                todo.extend(members);
            } else {
                todo.extend(starts);
            }
        }
    }
    Some(reached)
}

/// The regions `children` of one state fill.
fn regions(m: &Machine, children: &[usize]) -> std::ops::Range<usize> {
    let last = children.iter().map(|&c| m.states[c].region).max();
    0..last.map_or(0, |r| r + 1)
}

/// The states the writer named, which findings are about.
fn named(m: &Machine, state: usize) -> bool {
    let s = &m.states[state];
    !s.name.is_empty()
        && !matches!(
            s.kind,
            Kind::Start | Kind::End | Kind::History | Kind::DeepHistory
        )
}

/// States no path reaches; inside one of them only that one is reported.
fn unreachable(m: &Machine, reached: &[bool], out: &mut Vec<Finding>) {
    for (i, s) in m.states.iter().enumerate() {
        if named(m, i) && !reached[i] && m.ancestors(i).all(|a| reached[a]) {
            out.push(finding(
                s.line,
                Severity::Warning,
                format!("State {} cannot be reached from the start.", s.name),
            ));
        }
    }
}

/// States nothing leaves, neither they nor a state around them. A state
/// that cannot be reached is reported for that alone.
fn dead_ends(m: &Machine, reached: Option<&[bool]>, out: &mut Vec<Finding>) {
    let mut leaves = vec![false; m.states.len()];
    for t in &m.transitions {
        leaves[t.from] = true;
    }
    for (i, s) in m.states.iter().enumerate() {
        if !named(m, i)
            || m.is_composite(i)
            || reached.is_some_and(|r| !r[i])
            || leaves[i]
            || m.ancestors(i).any(|a| leaves[a])
        {
            continue;
        }
        let message = match s.kind {
            Kind::State => format!(
                "Nothing leaves {0}. If the machine ends there, add {0} --> [*].",
                s.name
            ),
            _ => format!("Nothing leaves {}.", s.name),
        };
        out.push(finding(s.line, Severity::Warning, message));
    }
}

/// Transitions out of one state that the same event fires with no guard,
/// or the same guard, to choose between them.
fn ambiguous(m: &Machine, out: &mut Vec<Finding>) {
    // (state, event, guard) → the first transition, by index.
    let mut seen: HashMap<(usize, Option<&str>, Option<&str>), usize> = HashMap::new();
    for (index, t) in m.transitions.iter().enumerate() {
        let from = &m.states[t.from];
        if !matches!(from.kind, Kind::State | Kind::Choice) {
            continue;
        }
        let guard = t.trigger.guard.as_deref();
        let events: Vec<Option<&str>> = if t.trigger.events.is_empty() {
            vec![None]
        } else {
            t.trigger.events.iter().map(|e| Some(e.as_str())).collect()
        };
        for event in events {
            let Some(&first) = seen.get(&(t.from, event, guard)) else {
                seen.insert((t.from, event, guard), index);
                continue;
            };
            let other = &m.transitions[first];
            let target = |i: usize| target_name(m, i);
            let line = other.line + 1;
            let (severity, message) = if other.to == t.to {
                (
                    Severity::Warning,
                    format!(
                        "The transition from {} to {} is written twice (first on line {line}).",
                        from.name,
                        target(t.to)
                    ),
                )
            } else {
                let what = match (from.kind, event) {
                    (_, Some(event)) => format!("Event {event} leads from {}", from.name),
                    (Kind::Choice, None) => format!("Choice {} leads", from.name),
                    (_, None) => format!("{} leads on without an event", from.name),
                };
                let choose = match guard {
                    Some(g) => format!("both under the guard [{g}]"),
                    None => "and no guard [...] chooses between them".to_owned(),
                };
                // One event to two states contradicts itself, also out of
                // a choice; ways that need no event may be left open on
                // purpose.
                let severity = if event.is_some() {
                    Severity::Error
                } else {
                    Severity::Warning
                };
                (
                    severity,
                    format!(
                        "{what} to {} (line {line}) and to {}, {choose}.",
                        target(other.to),
                        target(t.to)
                    ),
                )
            };
            out.push(finding(t.line, severity, message));
        }
    }
}

/// How a message names the target of a transition.
fn target_name(m: &Machine, state: usize) -> String {
    let s = &m.states[state];
    match s.kind {
        Kind::End => "the end [*]".to_owned(),
        Kind::History => "the history [H]".to_owned(),
        Kind::DeepHistory => "the deep history [H*]".to_owned(),
        _ => s.name.clone(),
    }
}

/// Composite states a transition enters at their edge while a region of
/// theirs has no start to say where to begin; reported at the region's
/// first state, where the start goes.
fn entered_without_start(m: &Machine, out: &mut Vec<Finding>) {
    for (i, s) in m.states.iter().enumerate() {
        if !m.is_composite(i) || !m.transitions.iter().any(|t| t.to == i) {
            continue;
        }
        let children: Vec<usize> = (0..m.states.len())
            .filter(|&c| m.states[c].parent == Some(i))
            .collect();
        let regions = regions(m, &children);
        let several = regions.len() > 1;
        for region in regions {
            let members: Vec<usize> = children
                .iter()
                .copied()
                .filter(|&c| m.states[c].region == region)
                .collect();
            if members.iter().any(|&c| m.states[c].kind == Kind::Start) {
                continue;
            }
            let Some(line) = members.iter().map(|&c| m.states[c].line).min() else {
                continue;
            };
            let place = if several {
                format!("its region {}", region + 1)
            } else {
                "it".to_owned()
            };
            out.push(finding(
                line,
                Severity::Warning,
                format!(
                    "{} is entered, but no [*] --> … inside {place} says where to begin.",
                    s.name
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use omagram_core::Language;

    use super::*;

    /// The findings as `(line, severity, message)`, one-based lines.
    fn check(code: &str, lang: Language) -> Vec<(usize, Severity, String)> {
        crate::check(code, lang)
            .into_iter()
            .map(|f| (f.line + 1, f.severity, f.message))
            .collect()
    }

    fn puml(body: &str) -> Vec<(usize, Severity, String)> {
        check(&format!("@startuml\n{body}\n@enduml\n"), Language::PlantUml)
    }

    fn mmd(body: &str) -> Vec<(usize, Severity, String)> {
        check(&format!("stateDiagram-v2\n{body}\n"), Language::Mermaid)
    }

    #[test]
    fn a_contradiction_a_lost_state_and_a_dead_end() {
        let expected = [
            (
                4,
                Severity::Error,
                "Event job leads from Idle to Busy (line 3) and to Error, and no guard [...] chooses between them.",
            ),
            (
                4,
                Severity::Warning,
                "Nothing leaves Error. If the machine ends there, add Error --> [*].",
            ),
            (
                6,
                Severity::Warning,
                "State Locked cannot be reached from the start.",
            ),
        ];
        let body = "[*] --> Idle\nIdle --> Busy : job\nIdle --> Error : job\nBusy --> Idle : done\nstate Locked\nLocked --> Idle : unlock";
        for found in [puml(body), mmd(body)] {
            let found: Vec<(usize, Severity, &str)> =
                found.iter().map(|(l, s, m)| (*l, *s, m.as_str())).collect();
            assert_eq!(found, expected);
        }
    }

    #[test]
    fn guards_events_and_ends_make_it_clear() {
        for body in [
            // Guards choose; different events; an explicit end.
            "[*] --> Idle\nIdle --> Busy : job [free]\nIdle --> Wait : job [busy]\nWait --> Busy : freed\nBusy --> [*]",
            // A transition from a composite state leaves every state in it.
            "[*] --> Run\nstate Run {\n  [*] --> Fast\n  Fast --> Slow : brake\n}\nRun --> [*] : stop",
            // Two events on one transition, each once.
            "[*] --> A\nA --> B : x, y\nB --> [*]",
        ] {
            assert_eq!(puml(body), [], "{body}");
            assert_eq!(mmd(body), [], "{body}");
        }
    }

    #[test]
    fn inside_a_lost_state_only_that_one_is_reported() {
        let found = mmd("[*] --> A\nA --> [*]\nstate Away {\n  [*] --> In\n  In --> Out\n}");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].2, "State Away cannot be reached from the start.");
    }

    #[test]
    fn written_twice_and_choices_without_guards() {
        let found = puml(
            "[*] --> A\nA --> B : go\nA --> B : go\nstate c <<choice>>\nB --> c\nc --> A\nc --> [*]",
        );
        assert_eq!(
            found,
            [
                (
                    4,
                    Severity::Warning,
                    "The transition from A to B is written twice (first on line 3).".to_owned()
                ),
                (
                    8,
                    Severity::Warning,
                    "Choice c leads to A (line 7) and to the end [*], and no guard [...] chooses between them."
                        .to_owned()
                ),
            ]
        );
    }

    /// Ways that need no event may be left open on purpose; one event to
    /// two states contradicts itself.
    #[test]
    fn ways_without_an_event_are_open_and_one_event_to_two_states_is_wrong() {
        let found = puml("[*] --> A\nA --> [*]\nA -> B\nB --> C : go\nB --> A : go\nC --> [*]");
        let severities: Vec<(usize, Severity)> = found.iter().map(|f| (f.0, f.1)).collect();
        assert_eq!(
            severities,
            [(4, Severity::Warning), (6, Severity::Error)],
            "{found:?}"
        );
    }

    /// Out of a choice as well.
    #[test]
    fn one_event_from_a_choice_to_two_states_is_wrong() {
        let found =
            puml("state c <<choice>>\n[*] --> c\nc --> A : go\nc --> B : go\nA --> [*]\nB --> [*]");
        assert_eq!(
            found,
            [(
                5,
                Severity::Error,
                "Event go leads from c to A (line 4) and to B, and no guard [...] chooses between them."
                    .to_owned()
            )]
        );
    }

    #[test]
    fn a_composite_state_needs_a_start_when_entered() {
        let found = puml(
            "[*] --> Session\nstate Session {\n  Login --> Main : ok\n}\nSession --> [*] : logout",
        );
        assert_eq!(
            found,
            [(
                4,
                Severity::Warning,
                "Session is entered, but no [*] --> … inside it says where to begin.".to_owned()
            )]
        );
        let regions = puml(
            "[*] --> Both\nstate Both {\n  [*] --> L\n  --\n  R --> R : tick\n}\nBoth --> [*]",
        );
        assert_eq!(regions.len(), 1, "{regions:?}");
        assert_eq!(regions[0].0, 6);
        assert!(regions[0].2.contains("inside its region 2"), "{regions:?}");
    }

    #[test]
    fn without_a_start_reachability_is_not_checked() {
        let found = mmd("Idle --> Busy : job\nBusy --> Idle : done\nLost --> Idle");
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].0, found[0].1), (2, Severity::Info));
        // Drawn as a state diagram by its first line alone, so checked as one.
        let found = puml(
            "hide empty description\nIdle --> Busy : job\nBusy --> Idle : done\nLost --> Idle",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].0, found[0].1), (3, Severity::Info));
    }

    #[test]
    fn findings_join_the_lint_in_line_order() {
        let code =
            "@startuml\n[*] --> A\nA --> B : x\nA --> C : x\nB --> [*]\nC --> [*]\n@enduml\n";
        let all = crate::findings(code, Language::PlantUml);
        assert!(all.windows(2).all(|w| w[0].line <= w[1].line));
        assert!(
            all.iter()
                .any(|f| f.severity == Severity::Error && f.line == 3)
        );
        // Other diagram types are not checked.
        assert_eq!(
            crate::check("@startuml\nA -> B\n@enduml", Language::PlantUml),
            []
        );
    }
}
