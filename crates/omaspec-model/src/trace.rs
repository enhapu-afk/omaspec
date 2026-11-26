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
//! a state's way without an event) the system may take without logging
//! it, so the object may be wherever any of them leads. All of this
//! errs towards a machine that can do more, so a finding is sure.

use std::collections::HashMap;

use omaspec_core::lint::{Finding, Happened, Severity, Task};
use serde_json::Value;

use crate::across::{
    Spec, event_of, events_list, key, no_transition, orthogonal, states_list, written,
};
use crate::task::task;
use crate::{Kind, Machine, Run, Specs, Taken};

/// Ways an object can be at once that are followed; past that it is not
/// checked, rather than checked wrongly.
const MAX_WAYS: usize = 1000;

/// The findings of `trace` against the project's state machines, in line
/// order.
pub fn check(trace: &str, specs: &Specs) -> Vec<Finding> {
    let mut checking = Checking::new(specs);
    trace.lines().flat_map(|text| checking.line(text)).collect()
}

/// A trace held against the project's state machines one line at a time,
/// as it is written: what [`check`] finds, line by line (`omaspec trace
/// --follow`).
pub struct Checking<'a> {
    specs: &'a Specs,
    /// By object; `None` for one without a machine, reported once.
    objects: HashMap<String, Option<(&'a Spec, Object)>>,
    /// The zero-based line the next one is.
    line: usize,
}

impl<'a> Checking<'a> {
    pub fn new(specs: &'a Specs) -> Self {
        Checking {
            specs,
            objects: HashMap::new(),
            line: 0,
        }
    }

    /// The findings of the trace's next line, `text`, without its line end.
    pub fn line(&mut self, text: &str) -> Vec<Finding> {
        let line = self.line;
        self.line += 1;
        let mut out = Vec::new();
        if text.trim().is_empty() {
            return out;
        }
        let entry = match entry(line, text) {
            Ok(entry) => entry,
            Err(why) => {
                out.push(finding("trace-line", line, Severity::Error, why));
                return out;
            }
        };
        let specs = self.specs;
        let object = self.objects.entry(entry.object.clone()).or_insert_with(|| {
            let name = entry.machine.as_deref().unwrap_or(&entry.object);
            match machine_named(name, specs) {
                Ok(spec) => Some((spec, Object::new(spec))),
                Err(why) => {
                    out.push(finding(
                        "trace-unchecked",
                        line,
                        Severity::Info,
                        format!("{why}, so the events of {} are not checked.", entry.object),
                    ));
                    None
                }
            }
        });
        if let Some((spec, object)) = object {
            out.extend(object.follow(spec, &entry).findings);
        }
        out
    }
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
    let mut one = Following::new(spec);
    for (line, text) in trace.lines().enumerate() {
        let Ok(entry) = entry(line, text) else {
            continue;
        };
        if entry.object == object {
            one.take(spec, entry);
        }
    }
    one.followed
}

/// Every object of `trace` that goes by `spec`'s machine, in the order
/// they first appear, each followed as [`follow`] follows it: the trace
/// read once, however many objects it has.
pub fn follow_all(trace: &str, spec: &Spec) -> Vec<(String, Followed)> {
    let names: Vec<String> = spec.names.iter().map(|n| key(n)).collect();
    let mut order: Vec<String> = Vec::new();
    let mut each: HashMap<String, Following> = HashMap::new();
    for (line, text) in trace.lines().enumerate() {
        let Ok(entry) = entry(line, text) else {
            continue;
        };
        if !names.contains(&key(entry.machine.as_deref().unwrap_or(&entry.object))) {
            continue;
        }
        let one = each.entry(entry.object.clone()).or_insert_with(|| {
            order.push(entry.object.clone());
            Following::new(spec)
        });
        one.take(spec, entry);
    }
    order
        .into_iter()
        .map(|object| {
            let followed = each
                .remove(&object)
                .map(|one| one.followed)
                .unwrap_or_default();
            (object, followed)
        })
        .collect()
}

/// Every object of a trace that goes by a machine, followed line by line
/// while the trace is written: what [`follow_all`] gives, a line at a
/// time, each object keeping only its last step, so a run of hours stays
/// as small as its objects (`omaspec preview --follow`).
pub struct Followers {
    spec: Spec,
    names: Vec<String>,
    /// The zero-based line the next one is.
    line: usize,
    order: Vec<String>,
    each: HashMap<String, Following>,
}

impl Followers {
    pub fn new(spec: Spec) -> Self {
        let names = spec.names.iter().map(|n| key(n)).collect();
        Followers {
            spec,
            names,
            line: 0,
            order: Vec::new(),
            each: HashMap::new(),
        }
    }

    pub fn machine(&self) -> &Machine {
        &self.spec.machine
    }

    /// The step the trace's next line `text` takes, and its object; `None`
    /// for a line that is no event or one of another machine's object.
    pub fn line(&mut self, text: &str) -> Option<(String, TraceStep)> {
        let line = self.line;
        self.line += 1;
        let entry = entry(line, text).ok()?;
        if !self
            .names
            .contains(&key(entry.machine.as_deref().unwrap_or(&entry.object)))
        {
            return None;
        }
        let object = entry.object.clone();
        let (each, order, spec) = (&mut self.each, &mut self.order, &self.spec);
        let one = each.entry(object.clone()).or_insert_with(|| {
            order.push(object.clone());
            Following::new(spec)
        });
        one.take(spec, entry);
        let steps = &mut one.followed.steps;
        if steps.len() > 1 {
            steps.drain(..steps.len() - 1);
        }
        Some((object, steps.last()?.clone()))
    }

    /// Each object in the order it first appears, as followed so far:
    /// where it began, and its last step.
    pub fn objects(&self) -> impl Iterator<Item = (&str, &Followed)> {
        self.order.iter().filter_map(|object| {
            self.each
                .get(object)
                .map(|one| (object.as_str(), &one.followed))
        })
    }
}

/// One object on its way through a machine, line by line.
struct Following {
    tracked: Object,
    before: Vec<usize>,
    followed: Followed,
}

impl Following {
    fn new(spec: &Spec) -> Self {
        Following {
            tracked: Object::new(spec),
            before: Vec::new(),
            followed: Followed::default(),
        }
    }

    /// Follows the object through `entry`, its next line.
    fn take(&mut self, spec: &Spec, entry: Entry) {
        let outcome = self.tracked.follow(spec, &entry);
        if self.followed.steps.is_empty() {
            self.followed.start = self.tracked.started.clone();
            self.followed.begins = self.tracked.begins.clone();
            self.before = self.followed.begins.clone();
        }
        let mut at = self.tracked.at();
        if at.is_empty() && outcome.deviated && entry.state.is_none() {
            at = self.before.clone();
        }
        self.before = at.clone();
        self.followed.steps.push(TraceStep {
            line: entry.line,
            time: entry.time,
            event: entry.event,
            state: entry.state,
            taken: outcome.taken,
            at,
            finding: outcome.findings.into_iter().next(),
        });
    }
}

/// One line of a trace.
pub(crate) struct Entry {
    /// Zero-based.
    line: usize,
    pub(crate) object: String,
    pub(crate) machine: Option<String>,
    pub(crate) event: Option<String>,
    pub(crate) state: Option<String>,
    time: Option<String>,
}

/// The line `text`, or why it is not read.
pub(crate) fn entry(line: usize, text: &str) -> Result<Entry, String> {
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
fn machine_named<'a>(name: &str, specs: &'a Specs) -> Result<&'a Spec, String> {
    match specs.named(&[name]).as_slice() {
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
struct Object {
    track: Track,
    /// Event key → the event as the machine writes it.
    written: HashMap<String, String>,
    /// The transitions out of the start, once it began there.
    started: Vec<Taken>,
    /// The states it began in there.
    begins: Vec<usize>,
    /// For each way it can be in now, the transitions without an event it
    /// took to get there since the last line; one the next line confirms
    /// was passed (a choice's branch, a guard's way).
    pending: Vec<Vec<Taken>>,
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
    /// The transitions without an event taken since the line before, on
    /// the ways this line leaves possible.
    passed: Vec<Taken>,
    /// Whether the line deviates from the machine.
    deviated: bool,
}

impl Outcome {
    fn deviation(&mut self, line: usize, message: String, task: Task) {
        self.findings
            .push(finding("trace-deviation", line, Severity::Error, message).asking(task));
        self.deviated = true;
    }
}

impl Object {
    fn new(spec: &Spec) -> Self {
        Object {
            track: Track::Fresh,
            written: written(&spec.machine),
            started: Vec::new(),
            begins: Vec::new(),
            pending: Vec::new(),
        }
    }

    /// Takes `paths`, the ways it can be in with how it got there.
    fn set_ways(&mut self, paths: Vec<(Run, Vec<Taken>)>) -> Vec<Run> {
        let (ways, pending): (Vec<Run>, Vec<Vec<Taken>>) = paths.into_iter().unzip();
        self.pending = pending;
        ways
    }

    /// Followed from `state`, where the trace says it is.
    fn anchor(&mut self, m: &Machine, state: usize) {
        self.track = match Run::at(m, state) {
            Some(run) => Track::Ways(self.set_ways(close_paths(m, vec![(run, Vec::new())]))),
            None => Track::Lost,
        };
    }

    /// The transitions without an event of the ways it can be in now,
    /// each once: what the trace ends on.
    fn pending(&self) -> Vec<&Taken> {
        let mut out: Vec<&Taken> = Vec::new();
        for step in self.pending.iter().flatten() {
            if !out.iter().any(|t| t.transition == step.transition) {
                out.push(step);
            }
        }
        out
    }

    /// Where `entry` leaves the object by its machine, for the task of a
    /// line that names a state the machine has not: the ways it can be
    /// in, past the line's event where that takes; from the start before
    /// its first line; any state while it is not followed.
    fn ways_after(&self, m: &Machine, entry: &Entry) -> Vec<Run> {
        let ways: Vec<Run> = match &self.track {
            Track::Ways(ways) => ways.clone(),
            Track::Fresh => Run::start(m).map(|(run, _)| vec![run]).unwrap_or_default(),
            Track::Lost => Vec::new(),
        };
        let ways = match &entry.event {
            Some(event) => {
                let k = key(event_of(event));
                let mut next = Vec::new();
                for run in &ways {
                    react(m, run, &k, &[], &[], &mut next);
                }
                match next.is_empty() {
                    true => ways,
                    false => next.into_iter().map(|(run, _)| run).collect(),
                }
            }
            None => ways,
        };
        match ways.is_empty() {
            true => (0..m.states.len()).filter_map(|s| Run::at(m, s)).collect(),
            false => ways,
        }
    }

    /// Follows the object through `entry`, its next line.
    fn follow(&mut self, spec: &Spec, entry: &Entry) -> Outcome {
        let mut outcome = Outcome::default();
        let m = &spec.machine;
        let file = &spec.file;
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
        // What the line says happened, for a deviation's task.
        let happened = || Happened {
            event: entry.event.clone(),
            state: entry.state.clone(),
            time: entry.time.clone(),
        };
        let ask = |ways: &[Run]| task(spec, &entry.object, happened(), ways);
        let state = match &entry.state {
            Some(name) => match state_named(m, name) {
                Some(state) => Some(state),
                None => {
                    let ways = self.ways_after(m, entry);
                    outcome.deviation(
                        entry.line,
                        trace_has(format!(
                            "{} in {name}, but its state machine has no such state ({file}).",
                            entry.object
                        )),
                        ask(&ways),
                    );
                    self.track = Track::Lost;
                    self.pending.clear();
                    return outcome;
                }
            },
            None => None,
        };
        let mut pending = std::mem::take(&mut self.pending);
        let fresh = matches!(self.track, Track::Fresh);
        let ways = match std::mem::replace(&mut self.track, Track::Lost) {
            Track::Fresh => match (entry.event.is_some(), state) {
                // Where it begins.
                (false, Some(state)) => {
                    self.anchor(m, state);
                    return outcome;
                }
                _ => match Run::start(m) {
                    Some((run, steps)) => {
                        let paths = close_paths(m, vec![(run, Vec::new())]);
                        let (ways, paths): (Vec<Run>, Vec<Vec<Taken>>) = paths.into_iter().unzip();
                        pending = paths;
                        self.started = steps;
                        self.begins = active(&ways);
                        ways
                    }
                    None => {
                        outcome.findings.push(finding(
                            "trace-no-start",
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
                    self.anchor(m, state);
                }
                return outcome;
            }
        };
        // The first line's event may be the label of the start's way
        // (`[*] --> Idle : power`), which the machine takes as it starts,
        // where it takes the event no further: the line says it started,
        // as a play takes it (REQ-141).
        let as_start = fresh
            && entry.event.as_deref().is_some_and(|event| {
                let k = key(event_of(event));
                starts_on(m, &k) && !reacts(m, &ways, &k)
            });
        let ways = match &entry.event {
            Some(_) if as_start => ways,
            Some(event) => {
                let event = event_of(event);
                let k = key(event);
                let mut next = Vec::new();
                for (index, run) in ways.iter().enumerate() {
                    let before = next.len();
                    react(m, run, &k, &[], &[], &mut next);
                    if next.len() > before {
                        // A way the event leaves: how it got there holds.
                        outcome.pass(pending.get(index).map_or(&[][..], Vec::as_slice));
                    }
                }
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
                    outcome.deviation(entry.line, say(body), ask(&ways));
                    outcome.passed.clear();
                    if let Some(state) = state {
                        self.anchor(m, state);
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
                let paths = close_paths(
                    m,
                    next.into_iter().map(|(run, _)| (run, Vec::new())).collect(),
                );
                let (ways, paths): (Vec<Run>, Vec<Vec<Taken>>) = paths.into_iter().unzip();
                pending = paths;
                ways
            }
            None => ways,
        };
        let ways = match state {
            Some(state) => {
                let keep: Vec<bool> = ways.iter().map(|w| w.is_in(m, state)).collect();
                let kept: Vec<Run> = ways
                    .iter()
                    .zip(&keep)
                    .filter(|(_, k)| **k)
                    .map(|(w, _)| w.clone())
                    .collect();
                if kept.is_empty() {
                    outcome.deviation(
                        entry.line,
                        trace_has(format!(
                            "{} in {}, but by its state machine it can only be in {} ({file}).",
                            entry.object,
                            m.states[state].name,
                            states_list(m, &can_be(m, &ways))
                        )),
                        ask(&ways),
                    );
                    outcome.passed.clear();
                    self.anchor(m, state);
                    return outcome;
                }
                // The ways the state leaves: without an event since the
                // line before, how they got there holds when this line
                // took no event.
                if entry.event.is_none() || as_start {
                    for (path, _) in pending.iter().zip(&keep).filter(|(_, k)| **k) {
                        outcome.pass(path);
                    }
                }
                pending = pending
                    .into_iter()
                    .zip(&keep)
                    .filter(|(_, k)| **k)
                    .map(|(path, _)| path)
                    .collect();
                kept
            }
            None => ways,
        };
        if ways.len() >= MAX_WAYS {
            outcome.findings.push(finding(
                "trace-too-many-ways",
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
        self.pending = pending;
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

impl Outcome {
    /// `path` was passed: its transitions, each once.
    fn pass(&mut self, path: &[Taken]) {
        for step in path {
            if !self.passed.iter().any(|t| t.transition == step.transition) {
                self.passed.push(step.clone());
            }
        }
    }
}

/// `ways` and everywhere they lead without an event, each with the
/// transitions it took to get there: what the writer picks in a play, the
/// system may take without logging it.
fn close_paths(m: &Machine, mut ways: Vec<(Run, Vec<Taken>)>) -> Vec<(Run, Vec<Taken>)> {
    let mut i = 0;
    while i < ways.len() && ways.len() < MAX_WAYS {
        let (run, path) = ways[i].clone();
        i += 1;
        for t in run.options(m) {
            if m.transitions[t].trigger.events.is_empty() {
                let mut next = run.clone();
                let steps = next.take(m, t);
                if !ways.iter().any(|(r, _)| *r == next) {
                    ways.push((next, [path.as_slice(), &steps].concat()));
                }
            }
        }
    }
    ways
}

/// What `trace` takes of `spec`'s machine, added to `cov`, in one pass:
/// each object of the machine followed as [`check`] follows it.
pub(crate) fn cover(trace: &str, spec: &Spec, cov: &mut crate::Coverage) {
    let m = &spec.machine;
    let names: Vec<String> = spec.names.iter().map(|n| key(n)).collect();
    let mut objects: HashMap<String, Object> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let count = |cov: &mut crate::Coverage, step: &Taken| {
        cov.transitions[step.transition] += 1;
        for &state in &step.entered {
            cov.states[state] += 1;
        }
    };
    for (line, text) in trace.lines().enumerate() {
        let Ok(entry) = entry(line, text) else {
            continue;
        };
        if !names.contains(&key(entry.machine.as_deref().unwrap_or(&entry.object))) {
            continue;
        }
        let object = objects.entry(entry.object.clone()).or_insert_with(|| {
            order.push(entry.object.clone());
            Object::new(spec)
        });
        let fresh = object.started.is_empty();
        // Where it is before the line: where it leaves the machine, should
        // the line deviate.
        let before = object.at();
        let outcome = object.follow(spec, &entry);
        if fresh {
            for step in &object.started {
                count(cov, step);
            }
        }
        for step in outcome.taken.iter().chain(&outcome.passed) {
            count(cov, step);
        }
        // A line that says where it is, with no event: it is there.
        if entry.event.is_none() && !outcome.deviated {
            let mut seen = Vec::new();
            for mut state in object.at() {
                loop {
                    if !seen.contains(&state) {
                        seen.push(state);
                        cov.states[state] += 1;
                    }
                    match m.states[state].parent {
                        Some(parent) => state = parent,
                        None => break,
                    }
                }
            }
        }
        if outcome.deviated {
            cov.deviations += 1;
            // Its first line: where it began. One not followed since an
            // earlier deviation left the machine there, not here.
            let before = if before.is_empty() && fresh {
                object.begins.clone()
            } else {
                before
            };
            for &state in &before {
                if let Some(left) = cov.left.get_mut(state) {
                    *left += 1;
                }
            }
        }
    }
    // Where the trace ends, every way it may have gone counts.
    for name in &order {
        for step in objects[name].pending() {
            count(cov, step);
        }
        if !cov.objects.contains(name) {
            cov.objects.push(name.clone());
        }
    }
}

/// Where `run` can go when the event with key `k` comes: every region
/// with a way for it takes one, so after the ways `fired` so far (which
/// took `taken`) only regions apart from theirs still do.
/// Whether `k`, an event's key, names the one way out of the machine's
/// top start, which the machine takes by itself as it starts.
fn starts_on(m: &Machine, k: &str) -> bool {
    let mut out = m.transitions.iter().filter(|t| {
        let from = &m.states[t.from];
        from.kind == Kind::Start && from.parent.is_none()
    });
    match (out.next(), out.next()) {
        (Some(way), None) => way.trigger.events.iter().any(|e| key(event_of(e)) == k),
        _ => false,
    }
}

/// Whether any of `ways` takes the event `k`.
fn reacts(m: &Machine, ways: &[Run], k: &str) -> bool {
    let mut next = Vec::new();
    ways.iter().any(|run| {
        react(m, run, k, &[], &[], &mut next);
        !next.is_empty()
    })
}

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

fn finding(
    code: &'static str,
    line: usize,
    severity: Severity,
    message: impl Into<String>,
) -> Finding {
    Finding::new(code, line, severity, message)
}

#[cfg(test)]
mod tests {
    use omaspec_core::Language;

    use super::*;

    /// Every object followed in one reading, as each is followed alone.
    #[test]
    fn all_objects_are_followed_as_each_alone() {
        let spec = Spec::read(
            "@startuml Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
            omaspec_core::Language::PlantUml,
            "door.puml",
        )
        .unwrap();
        let trace = [
            r#"{"object":"Door","event":"open"}"#,
            r#"{"object":"back","machine":"door","event":"open"}"#,
            r#"{"object":"Car","event":"park"}"#,
            r#"{"object":"Door","event":"open"}"#,
            r#"{"object":"back","machine":"door","event":"close"}"#,
        ]
        .join("\n");
        let all = follow_all(&trace, &spec);
        let names: Vec<&str> = all.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, objects(&trace, &spec));
        for (object, followed) in &all {
            assert_eq!(*followed, follow(&trace, &spec, object), "{object}");
        }
    }

    const DOOR: &str = "@startuml\n[*] --> Closed\nClosed --> Open : open [unlocked]\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";

    fn specs() -> Specs {
        Specs::new(vec![
            Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap(),
        ])
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
        let specs = Specs::new(vec![
            Spec::read(
                "@startuml\n[*] --> Idle\nIdle --> Check : job\nstate Check <<choice>>\nCheck --> Busy\nCheck --> Failed\nBusy --> Idle : done\nFailed --> Idle : reset\n@enduml\n",
                Language::PlantUml,
                "worker.puml",
            )
            .unwrap(),
        ]);
        let found = check(
            "{\"object\":\"Worker\",\"event\":\"job\"}\n{\"object\":\"Worker\",\"event\":\"reset\"}",
            &specs,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn every_region_with_a_way_takes_the_event() {
        let specs = Specs::new(vec![
            Spec::read(
                "@startuml\n[*] --> Box\nstate Box {\n  [*] --> A1\n  A1 --> A2 : go\n  --\n  [*] --> B1\n  B1 --> B2 : go\n}\n@enduml\n",
                Language::PlantUml,
                "box.puml",
            )
            .unwrap(),
        ]);
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

#[cfg(test)]
mod start_tests {
    use omaspec_core::Language;

    use super::*;

    fn findings(machine: &str, lines: &[&str]) -> Vec<String> {
        let specs = Specs::new(vec![
            Spec::read(machine, Language::PlantUml, "start.puml").unwrap(),
        ]);
        check(&lines.join("\n"), &specs)
            .into_iter()
            .map(|f| format!("{}: {}", f.line + 1, f.message))
            .collect()
    }

    /// REQ-141: a trace's first event may be the label of the start's
    /// way, which the machine takes as it starts, as a play takes it;
    /// later, or where the first state takes it too, it is an event as
    /// any other.
    #[test]
    fn a_trace_may_begin_with_its_start_s_label() {
        let machine = "@startuml Start\n[*] --> Idle : power\nIdle --> Busy : work\nBusy --> Idle : done\n@enduml\n";
        let line = |event: &str, state: &str| {
            format!(r#"{{"object":"m","machine":"start","event":"{event}","state":"{state}"}}"#)
        };
        let (power, work) = (line("power", "Idle"), line("work", "Busy"));
        assert_eq!(findings(machine, &[&power, &work]), Vec::<String>::new());
        assert_eq!(findings(machine, &[&work]), Vec::<String>::new());
        let late = findings(machine, &[&work, &line("power", "Idle")]);
        assert!(
            late.iter().any(|f| f.contains("does not take power")),
            "{late:?}"
        );
        // The state it starts in takes it: its own transition, as before.
        let looped = "@startuml Start\n[*] --> Idle : power\nIdle --> Busy : power\n@enduml\n";
        let taken = findings(looped, &[&line("power", "Busy")]);
        assert_eq!(taken, Vec::<String>::new());
        let stays = findings(looped, &[&line("power", "Idle")]);
        assert!(
            stays.iter().any(|f| f.contains("can only be in Busy")),
            "{stays:?}"
        );
    }
}
