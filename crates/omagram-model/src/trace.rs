//! A recorded trace held against the state machines of a project: what a
//! running system did, one JSON object per line.
//!
//! ```text
//! {"time":"10:30:01.123","object":"Door","event":"unlock"}
//! {"time":"10:30:02.400","object":"Door","event":"open","state":"Opening"}
//! ```
//!
//! Each line names an `object` and the `event` it took, and may give the
//! `state` it was in afterwards and the `time`; other fields are left
//! alone. An object goes by the state machine named like it, as a
//! participant of a sequence diagram does, or by the one its `machine`
//! names, so that many objects can share one machine. It begins where its
//! machine starts, or, when its first line gives a state and no event, in
//! that state.
//!
//! Played in order, as the player plays a machine, an event that no state
//! the object can be in by then takes is a deviation, and so is a state it
//! cannot be in. After a wrong state the object goes on from the state the
//! trace gives; after a wrong event, from the next line that gives one.
//! Every region with a way for an event takes it at once. Guards are not
//! evaluated, and what the writer picks in a play (the way out of a choice,
//! one of several ways without an event) the system may take without
//! logging it, so the object may be wherever any of them leads. All of this
//! errs towards a machine that can do more, so a finding is sure.

use std::collections::HashMap;

use omagram_core::lint::{Finding, Severity};
use serde_json::Value;

use crate::across::{
    Spec, event_of, events_list, key, no_transition, orthogonal, states_list, written,
};
use crate::{Kind, Machine, Run, Taken};

/// Ways an object can be at once that are followed; past that it is not
/// checked, rather than checked wrongly.
const MAX_WAYS: usize = 1000;

/// The findings of `trace` against the project's state machines, in line
/// order.
pub fn check(trace: &str, specs: &[Spec]) -> Vec<Finding> {
    let mut out = Vec::new();
    // By object; `None` for one without a machine, reported once.
    let mut objects: HashMap<String, Option<Object>> = HashMap::new();
    for (line, text) in trace.lines().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        let entry = match entry(line, text) {
            Ok(entry) => entry,
            Err(why) => {
                out.push(finding(line, Severity::Error, why));
                continue;
            }
        };
        let object = objects.entry(entry.object.clone()).or_insert_with(|| {
            let name = entry.machine.as_deref().unwrap_or(&entry.object);
            match machine_named(name, specs) {
                Ok(spec) => Some(Object::new(spec)),
                Err(why) => {
                    out.push(finding(
                        line,
                        Severity::Info,
                        format!("{why}, so the events of {} are not checked.", entry.object),
                    ));
                    None
                }
            }
        });
        if let Some(object) = object {
            out.extend(object.follow(&entry).findings);
        }
    }
    out
}

/// One line of an object's trace, as a viewer plays it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceStep {
    /// Zero-based, in the trace.
    pub line: usize,
    pub time: Option<String>,
    pub event: Option<String>,
    pub state: Option<String>,
    /// The transitions its event took, on every way it could go.
    pub taken: Vec<Taken>,
    /// The innermost states it can be in afterwards; where it stood when
    /// its event deviates, none while it is not followed.
    pub at: Vec<usize>,
    /// What deviates here, or why it is not followed from here.
    pub finding: Option<Finding>,
}

/// One object's trace through a machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Followed {
    /// The transitions out of the machine's start; none when its first
    /// line says where it is.
    pub start: Vec<Taken>,
    /// The states it begins in there.
    pub begins: Vec<usize>,
    pub steps: Vec<TraceStep>,
}

/// The objects of `trace` that go by `spec`'s machine, in the order they
/// first appear.
pub fn objects(trace: &str, spec: &Spec) -> Vec<String> {
    let names: Vec<String> = spec.names.iter().map(|n| key(n)).collect();
    let mut out: Vec<String> = Vec::new();
    for (line, text) in trace.lines().enumerate() {
        let Ok(entry) = entry(line, text) else {
            continue;
        };
        let name = key(entry.machine.as_deref().unwrap_or(&entry.object));
        if names.contains(&name) && !out.contains(&entry.object) {
            out.push(entry.object);
        }
    }
    out
}

/// The lines of `object` in `trace`, followed through `spec`'s machine.
pub fn follow(trace: &str, spec: &Spec, object: &str) -> Followed {
    let mut followed = Followed::default();
    let mut tracked = Object::new(spec);
    let mut before: Vec<usize> = Vec::new();
    for (line, text) in trace.lines().enumerate() {
        let Ok(entry) = entry(line, text) else {
            continue;
        };
        if entry.object != object {
            continue;
        }
        let outcome = tracked.follow(&entry);
        if followed.steps.is_empty() {
            followed.start = tracked.started.clone();
            followed.begins = tracked.begins.clone();
            before = followed.begins.clone();
        }
        let mut at = tracked.at();
        if at.is_empty() && outcome.deviated && entry.state.is_none() {
            at = before;
        }
        before = at.clone();
        followed.steps.push(TraceStep {
            line,
            time: entry.time,
            event: entry.event,
            state: entry.state,
            taken: outcome.taken,
            at,
            finding: outcome.findings.into_iter().next(),
        });
    }
    followed
}

/// One line of a trace.
struct Entry {
    /// Zero-based.
    line: usize,
    object: String,
    machine: Option<String>,
    event: Option<String>,
    state: Option<String>,
    time: Option<String>,
}

/// The line `text`, or why it is not read.
fn entry(line: usize, text: &str) -> Result<Entry, String> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| format!("This line is not JSON ({e}), so it is not checked."))?;
    let Value::Object(fields) = value else {
        return Err("This line is not a JSON object, so it is not checked.".to_owned());
    };
    let field = |name: &str| match fields.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.trim().to_owned()).filter(|s| !s.is_empty())),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(_) => Err(format!(
            "The {name} of this line is neither text nor a number, so it is not checked."
        )),
    };
    let object = field("object")?
        .ok_or_else(|| "This line names no object, so it is not checked.".to_owned())?;
    let event = field("event")?;
    let state = field("state")?;
    if event.is_none() && state.is_none() {
        return Err(format!(
            "This line gives {object} neither an event nor a state, so it is not checked."
        ));
    }
    Ok(Entry {
        line,
        object,
        machine: field("machine")?,
        event,
        state,
        time: field("time")?,
    })
}

/// The one state machine named `name`, or why there is none.
fn machine_named<'a>(name: &str, specs: &'a [Spec]) -> Result<&'a Spec, String> {
    let wanted = key(name);
    let found: Vec<&Spec> = specs
        .iter()
        .filter(|s| s.names.iter().any(|n| key(n) == wanted))
        .collect();
    match found.as_slice() {
        [spec] => Ok(spec),
        [] => Err(format!("No state machine of the project is named {name}")),
        several => {
            let files: Vec<&str> = several.iter().map(|s| s.file.as_str()).collect();
            Err(format!(
                "{} state machines are named {name} ({})",
                several.len(),
                files.join(", ")
            ))
        }
    }
}

/// An object of the trace, followed through its machine.
struct Object<'a> {
    spec: &'a Spec,
    track: Track,
    /// Event key → the event as the machine writes it.
    written: HashMap<String, String>,
    /// The transitions out of the start, once it began there.
    started: Vec<Taken>,
    /// The states it began in there.
    begins: Vec<usize>,
}

enum Track {
    /// Nothing read yet.
    Fresh,
    /// Every way it can be by now.
    Ways(Vec<Run>),
    /// Not followed until a line gives its state.
    Lost,
}

/// What following one line found.
#[derive(Default)]
struct Outcome {
    findings: Vec<Finding>,
    /// The transitions its event took, on every way.
    taken: Vec<Taken>,
    /// Whether the line deviates from the machine.
    deviated: bool,
}

impl Outcome {
    fn deviation(&mut self, line: usize, message: String) {
        self.findings.push(finding(line, Severity::Error, message));
        self.deviated = true;
    }
}

impl<'a> Object<'a> {
    fn new(spec: &'a Spec) -> Self {
        Object {
            spec,
            track: Track::Fresh,
            written: written(&spec.machine),
            started: Vec::new(),
            begins: Vec::new(),
        }
    }

    /// Follows the object through `entry`, its next line.
    fn follow(&mut self, entry: &Entry) -> Outcome {
        let mut outcome = Outcome::default();
        let m = &self.spec.machine;
        let file = &self.spec.file;
        // A sentence about the object starts with its name, as the trace
        // writes it.
        let say = |body: String| match &entry.time {
            Some(time) => format!("At {time}, {body}"),
            None => body,
        };
        let trace_has = |rest: String| match &entry.time {
            Some(time) => format!("At {time}, the trace has {rest}"),
            None => format!("The trace has {rest}"),
        };
        let state = match &entry.state {
            Some(name) => match state_named(m, name) {
                Some(state) => Some(state),
                None => {
                    outcome.deviation(
                        entry.line,
                        trace_has(format!(
                            "{} in {name}, but its state machine has no such state ({file}).",
                            entry.object
                        )),
                    );
                    self.track = Track::Lost;
                    return outcome;
                }
            },
            None => None,
        };
        let ways = match std::mem::replace(&mut self.track, Track::Lost) {
            Track::Fresh => match (entry.event.is_some(), state) {
                // Where it begins.
                (false, Some(state)) => {
                    self.track = anchored(m, state);
                    return outcome;
                }
                _ => match Run::start(m) {
                    Some((run, steps)) => {
                        let ways = close(m, vec![run]);
                        self.started = steps;
                        self.begins = active(&ways);
                        ways
                    }
                    None => {
                        outcome.findings.push(finding(
                            entry.line,
                            Severity::Info,
                            format!(
                                "The state machine of {} has no start ({file}), so it is followed only from a line that gives its state.",
                                entry.object
                            ),
                        ));
                        return outcome;
                    }
                },
            },
            Track::Ways(ways) => ways,
            Track::Lost => {
                if let Some(state) = state {
                    self.track = anchored(m, state);
                }
                return outcome;
            }
        };
        let ways = match &entry.event {
            Some(event) => {
                let event = event_of(event);
                let k = key(event);
                let next = react_all(m, &ways, &k);
                if next.is_empty() {
                    let body = if self.written.contains_key(&k) {
                        let can_be = can_be(m, &ways);
                        format!(
                            "{} does not take {event} here: by now it is in {}, and {} ({file}).",
                            entry.object,
                            states_list(m, &can_be),
                            no_transition(m, &can_be, event)
                        )
                    } else {
                        format!(
                            "{} has no transition for {event} in its state machine ({file}). {}",
                            entry.object,
                            events_list(&self.written)
                        )
                    };
                    let body = match state {
                        Some(_) => body,
                        None => format!(
                            "{body} Its later events are checked again from a line that gives its state."
                        ),
                    };
                    outcome.deviation(entry.line, say(body));
                    if let Some(state) = state {
                        self.track = anchored(m, state);
                    }
                    return outcome;
                }
                for (_, taken) in &next {
                    for step in taken {
                        if !outcome
                            .taken
                            .iter()
                            .any(|t| t.transition == step.transition)
                        {
                            outcome.taken.push(step.clone());
                        }
                    }
                }
                close(m, next.into_iter().map(|(run, _)| run).collect())
            }
            None => ways,
        };
        let ways = match state {
            Some(state) => {
                let kept: Vec<Run> = ways.iter().filter(|w| w.is_in(m, state)).cloned().collect();
                if kept.is_empty() {
                    outcome.deviation(
                        entry.line,
                        trace_has(format!(
                            "{} in {}, but by its state machine it can only be in {} ({file}).",
                            entry.object,
                            m.states[state].name,
                            states_list(m, &can_be(m, &ways))
                        )),
                    );
                    self.track = anchored(m, state);
                    return outcome;
                }
                kept
            }
            None => ways,
        };
        if ways.len() >= MAX_WAYS {
            outcome.findings.push(finding(
                entry.line,
                Severity::Info,
                format!(
                    "{} can be in too many ways at once here, so it is followed again only from a line that gives its state.",
                    entry.object
                ),
            ));
            return outcome;
        }
        self.track = Track::Ways(ways);
        outcome
    }

    /// The states it can be in by now; none while it is not followed.
    fn at(&self) -> Vec<usize> {
        match &self.track {
            Track::Ways(ways) => active(ways),
            Track::Fresh | Track::Lost => Vec::new(),
        }
    }
}

/// The state of `m` called `name`, as names compare.
fn state_named(m: &Machine, name: &str) -> Option<usize> {
    let wanted = key(name);
    (0..m.states.len())
        .find(|&s| m.states[s].kind == Kind::State && key(&m.states[s].name) == wanted)
}

/// Followed from `state`, where the trace says it is.
fn anchored(m: &Machine, state: usize) -> Track {
    match Run::at(m, state) {
        Some(run) => Track::Ways(close(m, vec![run])),
        None => Track::Lost,
    }
}

/// `ways` and everywhere they lead without an event: what the writer picks
/// in a play, the system may take without logging it.
fn close(m: &Machine, mut ways: Vec<Run>) -> Vec<Run> {
    let mut i = 0;
    while i < ways.len() && ways.len() < MAX_WAYS {
        let run = ways[i].clone();
        i += 1;
        for t in run.options(m) {
            if m.transitions[t].trigger.events.is_empty() {
                let mut next = run.clone();
                next.take(m, t);
                if !ways.contains(&next) {
                    ways.push(next);
                }
            }
        }
    }
    ways
}

/// Where `ways` can go when the event with key `k` comes, each with the
/// transitions it took.
fn react_all(m: &Machine, ways: &[Run], k: &str) -> Vec<(Run, Vec<Taken>)> {
    let mut out = Vec::new();
    for run in ways {
        react(m, run, k, &[], &[], &mut out);
    }
    out
}

/// Where `run` can go when the event with key `k` comes: every region
/// with a way for it takes one, so after the ways `fired` so far (which
/// took `taken`) only regions apart from theirs still do.
fn react(
    m: &Machine,
    run: &Run,
    k: &str,
    fired: &[usize],
    taken: &[Taken],
    out: &mut Vec<(Run, Vec<Taken>)>,
) {
    let names = |t: usize| {
        m.transitions[t]
            .trigger
            .events
            .iter()
            .any(|e| key(event_of(e)) == k)
    };
    let apart = |t: usize| {
        fired
            .iter()
            .all(|&f| orthogonal(m, m.transitions[f].from, m.transitions[t].from))
    };
    let ways: Vec<usize> = run
        .options(m)
        .into_iter()
        .filter(|&t| names(t) && apart(t))
        .collect();
    if ways.is_empty() {
        if !fired.is_empty() && !out.iter().any(|(r, _)| r == run) && out.len() < MAX_WAYS {
            out.push((run.clone(), taken.to_vec()));
        }
        return;
    }
    for t in ways {
        let mut next = run.clone();
        let steps = next.take(m, t);
        react(
            m,
            &next,
            k,
            &[fired, &[t]].concat(),
            &[taken, &steps].concat(),
            out,
        );
    }
}

/// The innermost states any of `ways` is in, in order.
fn active(ways: &[Run]) -> Vec<usize> {
    let mut out: Vec<usize> = ways
        .iter()
        .flat_map(|w| w.active().iter().copied())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The states any of `ways` is in.
fn can_be(m: &Machine, ways: &[Run]) -> Vec<bool> {
    let mut can_be = vec![false; m.states.len()];
    for run in ways {
        for &s in run.active() {
            can_be[s] = true;
        }
    }
    can_be
}

fn finding(line: usize, severity: Severity, message: impl Into<String>) -> Finding {
    Finding {
        line,
        severity,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use omagram_core::Language;

    use super::*;

    const DOOR: &str = "@startuml\n[*] --> Closed\nClosed --> Open : open [unlocked]\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";

    fn specs() -> Vec<Spec> {
        vec![Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap()]
    }

    /// The findings as `(line, severity, message)`, one-based lines.
    fn trace(lines: &[&str]) -> Vec<(usize, Severity, String)> {
        check(&lines.join("\n"), &specs())
            .into_iter()
            .map(|f| (f.line + 1, f.severity, f.message))
            .collect()
    }

    #[test]
    fn a_trace_the_machine_allows_is_clean() {
        let found = trace(&[
            r#"{"time":"10:30:01","object":"Door","event":"lock","state":"Locked","pid":17}"#,
            "",
            r#"{"object":"Door","event":"Unlock"}"#,
            r#"{"object":"Door","event":"open","state":"Open"}"#,
            r#"{"object":"Door","event":"close(force)"}"#,
        ]);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn an_event_the_object_cannot_take_there_is_an_error() {
        let found = trace(&[
            r#"{"object":"Door","event":"lock"}"#,
            r#"{"time":"10:30:02","object":"Door","event":"open"}"#,
            r#"{"object":"Door","event":"open"}"#,
        ]);
        assert_eq!(
            found,
            [(
                2,
                Severity::Error,
                "At 10:30:02, Door does not take open here: by now it is in Locked, and it has no transition for open (door.puml). Its later events are checked again from a line that gives its state.".to_owned()
            )],
            "line 3 is not checked again"
        );
    }

    #[test]
    fn an_object_keeps_its_name_as_the_trace_writes_it() {
        let found = trace(&[
            r#"{"object":"door-7","machine":"Door","event":"lock"}"#,
            r#"{"object":"door-7","event":"open"}"#,
        ]);
        assert!(
            found[0].2.starts_with("door-7 does not take open"),
            "{found:?}"
        );
    }

    #[test]
    fn an_unknown_event_names_the_ones_there_are() {
        let found = trace(&[r#"{"object":"Door","event":"honk"}"#]);
        assert_eq!(found.len(), 1);
        assert!(
            found[0].2.starts_with(
                "Door has no transition for honk in its state machine (door.puml). It takes close, lock, open, unlock."
            ),
            "{}",
            found[0].2
        );
    }

    #[test]
    fn a_wrong_state_is_an_error_and_the_trace_goes_on_from_it() {
        let found = trace(&[
            r#"{"object":"Door","event":"lock","state":"Open"}"#,
            r#"{"object":"Door","event":"close"}"#,
            r#"{"object":"Door","event":"open"}"#,
        ]);
        assert_eq!(
            found,
            [(
                1,
                Severity::Error,
                "The trace has Door in Open, but by its state machine it can only be in Locked (door.puml).".to_owned()
            )],
            "from Open, close and open are fine"
        );
    }

    #[test]
    fn a_state_on_the_first_line_says_where_the_object_begins() {
        let found = trace(&[
            r#"{"object":"Door","state":"Locked"}"#,
            r#"{"object":"Door","event":"unlock"}"#,
        ]);
        assert!(found.is_empty(), "{found:?}");
        let found = trace(&[r#"{"object":"Door","state":"Ajar"}"#]);
        assert_eq!(found.len(), 1);
        assert!(found[0].2.contains("has no such state"), "{}", found[0].2);
    }

    /// Names of machines and events compare loosely, objects exactly: a
    /// log tells its objects apart by their ids.
    #[test]
    fn objects_go_by_their_machine_and_each_has_its_own_state() {
        let found = trace(&[
            r#"{"object":"door-1","machine":"Door","event":"lock"}"#,
            r#"{"object":"door-2","machine":"Door","event":"open"}"#,
            r#"{"object":"door-1","event":"unlock"}"#,
            r#"{"object":"Car","event":"park"}"#,
            r#"{"object":"Car","event":"leave"}"#,
        ]);
        assert_eq!(
            found,
            [(
                4,
                Severity::Info,
                "No state machine of the project is named Car, so the events of Car are not checked.".to_owned()
            )]
        );
    }

    #[test]
    fn lines_that_are_not_events_are_errors() {
        let found = trace(&[
            "not json",
            "[1, 2]",
            r#"{"event":"open"}"#,
            r#"{"object":"Door"}"#,
            r#"{"object":"Door","event":["open"]}"#,
        ]);
        let lines: Vec<usize> = found.iter().map(|f| f.0).collect();
        assert_eq!(lines, [1, 2, 3, 4, 5]);
        assert!(found.iter().all(|f| f.1 == Severity::Error));
    }

    #[test]
    fn what_needs_no_event_may_happen_unlogged() {
        let specs = vec![
            Spec::read(
                "@startuml\n[*] --> Idle\nIdle --> Check : job\nstate Check <<choice>>\nCheck --> Busy\nCheck --> Failed\nBusy --> Idle : done\nFailed --> Idle : reset\n@enduml\n",
                Language::PlantUml,
                "worker.puml",
            )
            .unwrap(),
        ];
        let found = check(
            "{\"object\":\"Worker\",\"event\":\"job\"}\n{\"object\":\"Worker\",\"event\":\"reset\"}",
            &specs,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn every_region_with_a_way_takes_the_event() {
        let specs = vec![
            Spec::read(
                "@startuml\n[*] --> Box\nstate Box {\n  [*] --> A1\n  A1 --> A2 : go\n  --\n  [*] --> B1\n  B1 --> B2 : go\n}\n@enduml\n",
                Language::PlantUml,
                "box.puml",
            )
            .unwrap(),
        ];
        let found = check(
            "{\"object\":\"Box\",\"event\":\"go\",\"state\":\"A2\"}\n{\"object\":\"Box\",\"state\":\"B2\"}",
            &specs,
        );
        assert!(found.is_empty(), "{found:?}");
        let found = check(
            "{\"object\":\"Box\",\"event\":\"go\"}\n{\"object\":\"Box\",\"state\":\"B1\"}",
            &specs,
        );
        assert_eq!(found.len(), 1, "B moved with A: {found:?}");
    }

    #[test]
    fn an_object_is_followed_line_by_line() {
        let specs = specs();
        let spec = &specs[0];
        let m = &spec.machine;
        let trace = [
            r#"{"object":"Door","event":"lock"}"#,
            r#"{"object":"Car","event":"park"}"#,
            r#"{"object":"Door","event":"open"}"#,
            r#"{"object":"Door","event":"close"}"#,
            r#"{"object":"Door","state":"Locked"}"#,
            r#"{"object":"door-2","machine":"Door","event":"lock"}"#,
        ]
        .join("\n");
        assert_eq!(objects(&trace, spec), ["Door", "door-2"]);
        let names = |states: &[usize]| -> Vec<String> {
            states.iter().map(|&s| m.states[s].name.clone()).collect()
        };
        let followed = follow(&trace, spec, "Door");
        assert_eq!(followed.start.len(), 1, "out of the start");
        assert_eq!(names(&followed.begins), ["Closed"]);
        let lines: Vec<usize> = followed.steps.iter().map(|s| s.line).collect();
        assert_eq!(lines, [0, 2, 3, 4]);
        let [lock, open, close, locked] = &followed.steps[..] else {
            panic!("four steps");
        };
        assert_eq!(names(&lock.at), ["Locked"]);
        assert_eq!(lock.taken.len(), 1);
        assert!(lock.finding.is_none());
        // Where it stood when it deviated, then not followed.
        assert!(open.finding.is_some());
        assert_eq!(names(&open.at), ["Locked"]);
        assert!(close.at.is_empty() && close.finding.is_none());
        // A line with a state picks it up again.
        assert_eq!(names(&locked.at), ["Locked"]);
    }
}
