//! What changed in the behavior between two versions of a diagram: the
//! states and transitions of a state machine and their triggers, the
//! participants and messages of a scenario and the blocks around them, and
//! what the specification checks find now or no longer find. The two
//! versions may be written in different languages.
//!
//! Layout is not behavior: lines moved, re-indented or reordered, colors,
//! notes, labels shown instead of names and the shapes of participants
//! change nothing here. States and participants go by their names; a
//! renamed one is one removed and one added.

use std::collections::HashMap;

use omaspec_core::diff::{Step, script};
use omaspec_core::lint::{Finding, Severity};
use omaspec_core::{Kind as DiagramKind, Language, classify};
use omaspec_uml::sequence::{self, End, Item, Side};

use crate::{Kind, Machine, Spec, Trigger, across, check};

/// One change from the old version to the new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub kind: ChangeKind,
    /// What changed, in the writer's words.
    pub message: String,
    /// Zero-based line in the old version, where it is there.
    pub old_line: Option<usize>,
    /// Zero-based line in the new version, where it is there.
    pub new_line: Option<usize>,
    /// How serious a finding is, for [`ChangeKind::Found`] and
    /// [`ChangeKind::Resolved`].
    pub severity: Option<Severity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
    /// A specification check finds this in the new version only.
    Found,
    /// A specification check found this in the old version only.
    Resolved,
}

impl ChangeKind {
    pub fn name(self) -> &'static str {
        match self {
            ChangeKind::Added => "added",
            ChangeKind::Removed => "removed",
            ChangeKind::Changed => "changed",
            ChangeKind::Found => "found",
            ChangeKind::Resolved => "resolved",
        }
    }
}

/// The changes in behavior from `old` to `new`: first those of the
/// states or participants, then of the transitions or messages, then of
/// the findings, each in line order. A sequence diagram's findings are
/// those against `specs`, the state machines of its project. `Err` says
/// why the two cannot be compared.
pub fn diff(
    old: &str,
    old_lang: Language,
    new: &str,
    new_lang: Language,
    specs: &[Spec],
) -> Result<Vec<Change>, String> {
    let (a, b) = (classify(old, old_lang), classify(new, new_lang));
    match (a.kind, b.kind) {
        (DiagramKind::State, DiagramKind::State) => {
            let a = Machine::read(old, old_lang).ok_or(UNREAD_OLD)?;
            let b = Machine::read(new, new_lang).ok_or(UNREAD_NEW)?;
            let mut changes = machines(&a, &b);
            changes.extend(findings(check::machine(&a), check::machine(&b)));
            Ok(changes)
        }
        (DiagramKind::Sequence, DiagramKind::Sequence) => {
            let a = across::read_sequence(old, old_lang).ok_or(UNREAD_OLD)?;
            let b = across::read_sequence(new, new_lang).ok_or(UNREAD_NEW)?;
            let mut changes = scenarios(&a.0, a.1, &b.0, b.1)?;
            changes.extend(findings(
                across::check(old, old_lang, specs),
                across::check(new, new_lang, specs),
            ));
            Ok(changes)
        }
        (x, y) if x != y && (a.name.is_empty() || b.name.is_empty()) => {
            Err("The versions are different kinds of diagram".into())
        }
        (x, y) if x != y => Err(format!(
            "The versions are different kinds of diagram: {} and {}",
            a_kind(&a.name),
            a_kind(&b.name)
        )),
        _ if a.name.is_empty() => Err(
            "Omaspec compares the behavior of state machines and sequence diagrams only; compare the lines instead".into(),
        ),
        _ => Err(format!(
            "Omaspec compares the behavior of state machines and sequence diagrams, not of {}; compare the lines instead",
            a_kind(&a.name)
        )),
    }
}

/// A kind of diagram in a sentence: `a state diagram`, `an activity
/// diagram`, `a WBS`.
pub(crate) fn a_kind(name: &str) -> String {
    let acronym = name.chars().nth(1).is_some_and(char::is_uppercase);
    let name = match acronym {
        true => name.to_owned(),
        false => name.to_lowercase(),
    };
    let article = match name.starts_with(['a', 'e', 'i', 'o', 'u']) {
        true => "an",
        false => "a",
    };
    format!("{article} {name}")
}

const UNREAD_OLD: &str = "Omaspec does not fully read the old version, so it cannot compare it";
const UNREAD_NEW: &str = "Omaspec does not fully read the new version, so it cannot compare it";

/// The changes from machine `a` to machine `b`, states first.
pub(crate) fn machines(a: &Machine, b: &Machine) -> Vec<Change> {
    let keys_a: Vec<String> = (0..a.states.len()).map(|s| key(a, s)).collect();
    let keys_b: Vec<String> = (0..b.states.len()).map(|s| key(b, s)).collect();
    let index_b: HashMap<&str, usize> = keys_b
        .iter()
        .enumerate()
        .map(|(i, k)| (k.as_str(), i))
        .collect();
    let index_a: HashMap<&str, usize> = keys_a
        .iter()
        .enumerate()
        .map(|(i, k)| (k.as_str(), i))
        .collect();
    let mut changes = Vec::new();
    for (i, s) in a.states.iter().enumerate() {
        if is_marker(s.kind) {
            continue;
        }
        let Some(&j) = index_b.get(keys_a[i].as_str()) else {
            changes.push(change(
                ChangeKind::Removed,
                format!("{} {} removed.", capital(kind_name(s.kind)), s.name),
                Some(s.line),
                None,
            ));
            continue;
        };
        let t = &b.states[j];
        if s.kind != t.kind {
            changes.push(change(
                ChangeKind::Changed,
                format!(
                    "{} is now a {}; it was a {}.",
                    s.name,
                    kind_name(t.kind),
                    kind_name(s.kind)
                ),
                Some(s.line),
                Some(t.line),
            ));
        }
        let (was, is) = (container(a, i), container(b, j));
        if was != is {
            changes.push(change(
                ChangeKind::Changed,
                format!("{} now sits {is}; it sat {was}.", s.name),
                Some(s.line),
                Some(t.line),
            ));
        }
    }
    for (j, t) in b.states.iter().enumerate() {
        if !is_marker(t.kind) && !index_a.contains_key(keys_b[j].as_str()) {
            let place = match t.parent {
                Some(p) => format!(" in {}", b.states[p].name),
                None => String::new(),
            };
            changes.push(change(
                ChangeKind::Added,
                format!("New {} {}{place}.", kind_name(t.kind), t.name),
                None,
                Some(t.line),
            ));
        }
    }
    sort(&mut changes);
    let mut transitions = transitions(a, &keys_a, b, &keys_b);
    sort(&mut transitions);
    changes.extend(transitions);
    changes
}

/// A transition as the diff matches it.
struct Arrow<'a> {
    from: &'a str,
    to: &'a str,
    /// The events, sorted: `a, b` and `b, a` are the same.
    events: Vec<&'a str>,
    trigger: &'a Trigger,
    line: usize,
}

fn arrows<'a>(m: &'a Machine, keys: &'a [String]) -> Vec<Arrow<'a>> {
    m.transitions
        .iter()
        .map(|t| {
            let mut events: Vec<&str> = t.trigger.events.iter().map(String::as_str).collect();
            events.sort_unstable();
            Arrow {
                from: &keys[t.from],
                to: &keys[t.to],
                events,
                trigger: &t.trigger,
                line: t.line,
            }
        })
        .collect()
}

fn transitions(a: &Machine, keys_a: &[String], b: &Machine, keys_b: &[String]) -> Vec<Change> {
    let (old, new) = (arrows(a, keys_a), arrows(b, keys_b));
    let mut paired_old = vec![false; old.len()];
    let mut paired_new = vec![false; new.len()];
    let mut changes = Vec::new();
    // Unchanged first, then what changed least: the target, the guard or
    // the action, the target and the guard, the events.
    let rounds: [fn(&Arrow, &Arrow) -> bool; 5] = [
        |x, y| x.from == y.from && x.to == y.to && x.events == y.events && same_rest(x, y),
        |x, y| x.from == y.from && x.events == y.events && x.trigger.guard == y.trigger.guard,
        |x, y| x.from == y.from && x.to == y.to && x.events == y.events,
        |x, y| x.from == y.from && x.events == y.events && !x.events.is_empty(),
        |x, y| x.from == y.from && x.to == y.to,
    ];
    for (round, matches) in rounds.iter().enumerate() {
        for (i, x) in old.iter().enumerate() {
            if paired_old[i] {
                continue;
            }
            let Some(j) = (0..new.len()).find(|&j| !paired_new[j] && matches(x, &new[j])) else {
                continue;
            };
            paired_old[i] = true;
            paired_new[j] = true;
            if round > 0 {
                changes.extend(changed(a, b, x, &new[j]));
            }
        }
    }
    for (i, x) in old.iter().enumerate() {
        if !paired_old[i] {
            changes.push(change(
                ChangeKind::Removed,
                format!("Transition removed: {}.", describe(a, x)),
                Some(x.line),
                None,
            ));
        }
    }
    for (j, y) in new.iter().enumerate() {
        if !paired_new[j] {
            changes.push(change(
                ChangeKind::Added,
                format!("New transition: {}.", describe(b, y)),
                None,
                Some(y.line),
            ));
        }
    }
    changes
}

fn same_rest(x: &Arrow, y: &Arrow) -> bool {
    x.trigger.guard == y.trigger.guard && x.trigger.action == y.trigger.action
}

/// What changed in a transition that kept its source.
fn changed(a: &Machine, b: &Machine, x: &Arrow, y: &Arrow) -> Vec<Change> {
    let mut out = Vec::new();
    let mut say = |message: String| {
        out.push(change(
            ChangeKind::Changed,
            message,
            Some(x.line),
            Some(y.line),
        ))
    };
    if x.to != y.to {
        let (was, is) = (name(a, x.to), name(b, y.to));
        let from = name(b, y.from);
        say(match (starts(y.from), when(y)) {
            (Some(machine), _) => format!("{machine} now starts in {is} instead of {was}."),
            (None, Some(when)) => format!("{when}, {from} now goes to {is} instead of {was}."),
            (None, None) => format!("{} now goes on to {is} instead of {was}.", capital(&from)),
        });
    } else if x.events != y.events {
        let now = match y.events.is_empty() {
            true => "now needs no event".to_owned(),
            false => format!("now happens on {}", y.events.join(", ")),
        };
        let was = match x.events.is_empty() {
            true => "it needed none".to_owned(),
            false => format!("it happened on {}", x.events.join(", ")),
        };
        say(format!("{} {now}; {was}.", capital(&path(b, y))));
    }
    if x.trigger.guard != y.trigger.guard {
        let what = capital(&with_events(b, y));
        say(match (&x.trigger.guard, &y.trigger.guard) {
            (Some(was), Some(is)) => format!("{what} now has the guard [{is}] instead of [{was}]."),
            (None, Some(is)) => format!("{what} now has the guard [{is}]."),
            (Some(was), None) => format!("{what} no longer has the guard [{was}]."),
            (None, None) => unreachable!(),
        });
    }
    if x.trigger.action != y.trigger.action {
        let what = capital(&with_events(b, y));
        say(match (&x.trigger.action, &y.trigger.action) {
            (Some(was), Some(is)) => format!("{what} now does {is} instead of {was}."),
            (None, Some(is)) => format!("{what} now does {is}."),
            (Some(was), None) => format!("{what} no longer does {was}."),
            (None, None) => unreachable!(),
        });
    }
    out
}

/// `On job` or `On job [ready]`: when a transition fires; `None` when it
/// needs no event.
fn when(x: &Arrow) -> Option<String> {
    if x.events.is_empty() {
        return None;
    }
    let guard = x
        .trigger
        .guard
        .as_ref()
        .map(|g| format!(" [{g}]"))
        .unwrap_or_default();
    Some(format!("On {}{guard}", x.events.join(", ")))
}

/// `Idle to Busy`.
fn path(m: &Machine, x: &Arrow) -> String {
    format!("{} to {}", name(m, x.from), name(m, x.to))
}

/// `Idle to Busy on job`.
fn with_events(m: &Machine, x: &Arrow) -> String {
    match x.events.is_empty() {
        true => path(m, x),
        false => format!("{} on {}", path(m, x), x.events.join(", ")),
    }
}

/// `Idle to Busy on job [ready] / start`.
fn describe(m: &Machine, x: &Arrow) -> String {
    let mut text = with_events(m, x);
    if let Some(guard) = &x.trigger.guard {
        text.push_str(&format!(" [{guard}]"));
    }
    if let Some(action) = &x.trigger.action {
        text.push_str(&format!(" / {action}"));
    }
    text
}

/// How the diff knows a state across versions: a named one by its name, a
/// start, an end or a history by its kind, the state it sits in and its
/// region.
fn key(m: &Machine, s: usize) -> String {
    let state = &m.states[s];
    if !state.name.is_empty() {
        return state.name.clone();
    }
    let parent = state.parent.map_or("", |p| m.states[p].name.as_str());
    format!("\0{:?}\0{parent}\0{}", state.kind, state.region)
}

fn is_marker(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Start | Kind::End | Kind::History | Kind::DeepHistory
    )
}

/// What a start key starts: `The machine`, or the state it sits in.
fn starts(key: &str) -> Option<String> {
    let parent = key.strip_prefix("\0Start\0")?.split('\0').next()?;
    Some(match parent.is_empty() {
        true => "The machine".into(),
        false => parent.to_owned(),
    })
}

/// A state's name in a message, by its key: `Idle`, `the start`, `the end
/// of Busy`.
fn name(m: &Machine, key: &str) -> String {
    let Some(rest) = key.strip_prefix('\0') else {
        return key.to_owned();
    };
    let mut parts = rest.split('\0');
    let (kind, parent, region) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    let what = match kind {
        "Start" => "the start",
        "End" => "the end",
        "DeepHistory" => "the deep history",
        _ => "the history",
    };
    if parent.is_empty() {
        return what.to_owned();
    }
    let regions = m
        .states
        .iter()
        .filter(|s| {
            m.states
                .get(s.parent.unwrap_or(usize::MAX))
                .is_some_and(|p| p.name == parent)
        })
        .map(|s| s.region)
        .max()
        .unwrap_or(0);
    match regions {
        0 => format!("{what} of {parent}"),
        _ => format!(
            "{what} of {parent} (region {})",
            region.parse::<usize>().unwrap_or(0) + 1
        ),
    }
}

/// Where a state sits, for a message: `at the top`, `in On`, `in region 2
/// of On`.
fn container(m: &Machine, s: usize) -> String {
    let state = &m.states[s];
    let Some(parent) = state.parent else {
        return "at the top".into();
    };
    let regions = m
        .states
        .iter()
        .any(|other| other.parent == Some(parent) && other.region > 0);
    match regions {
        true => format!(
            "in region {} of {}",
            state.region + 1,
            m.states[parent].name
        ),
        false => format!("in {}", m.states[parent].name),
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::State => "state",
        Kind::Start => "start",
        Kind::End => "end",
        Kind::Choice => "choice",
        Kind::Fork => "fork",
        Kind::Join => "join",
        Kind::History => "history",
        Kind::DeepHistory => "deep history",
    }
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn change(
    kind: ChangeKind,
    message: String,
    old_line: Option<usize>,
    new_line: Option<usize>,
) -> Change {
    Change {
        kind,
        message,
        old_line,
        new_line,
        severity: None,
    }
}

/// In line order: by the new line, else by the old one.
fn sort(changes: &mut [Change]) {
    changes.sort_by_key(|c| (c.new_line.or(c.old_line), c.old_line));
}

/// What the checks find in only one of the versions, the new findings
/// first. A finding that names a line (`first on line 12`) is the same
/// finding on another line.
fn findings(old: Vec<Finding>, new: Vec<Finding>) -> Vec<Change> {
    let mut left: Vec<Option<&Finding>> = old.iter().map(Some).collect();
    let mut out = Vec::new();
    for f in &new {
        let message = without_lines(&f.message);
        let seen = left.iter_mut().find(|o| {
            o.is_some_and(|o| o.severity == f.severity && without_lines(&o.message) == message)
        });
        match seen {
            Some(slot) => *slot = None,
            None => out.push(Change {
                kind: ChangeKind::Found,
                message: f.message.clone(),
                old_line: None,
                new_line: Some(f.line),
                severity: Some(f.severity),
            }),
        }
    }
    for f in left.into_iter().flatten() {
        out.push(Change {
            kind: ChangeKind::Resolved,
            message: f.message.clone(),
            old_line: Some(f.line),
            new_line: None,
            severity: Some(f.severity),
        });
    }
    out
}

/// `message` with the number after each `line ` left out.
fn without_lines(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut rest = message;
    while let Some(at) = rest.find("line ") {
        let (head, tail) = rest.split_at(at + "line ".len());
        out.push_str(head);
        rest = tail.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

/// One step of a scenario as the diff matches it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Beat {
    Message {
        from: String,
        to: String,
        text: String,
        reply: bool,
    },
    Open {
        keyword: &'static str,
        label: String,
    },
    Else {
        label: String,
    },
    Close,
    Ref {
        over: Vec<String>,
        text: String,
    },
    Destroy(String),
}

/// The changes from scenario `a` to scenario `b`, whose lines start
/// `a_off` and `b_off` lines into their texts.
pub(crate) fn scenarios(
    a: &sequence::Diagram,
    a_off: usize,
    b: &sequence::Diagram,
    b_off: usize,
) -> Result<Vec<Change>, String> {
    let mut changes = Vec::new();
    let has = |d: &sequence::Diagram, code: &str| d.participants.iter().any(|p| p.code == code);
    for p in &a.participants {
        if !has(b, &p.code) {
            changes.push(change(
                ChangeKind::Removed,
                format!("Participant {} removed.", p.code),
                Some(p.line + a_off),
                None,
            ));
        }
    }
    for p in &b.participants {
        if !has(a, &p.code) {
            changes.push(change(
                ChangeKind::Added,
                format!("New participant {}.", p.code),
                None,
                Some(p.line + b_off),
            ));
        }
    }
    sort(&mut changes);
    let (old, new) = (beats(a, a_off), beats(b, b_off));
    let keys_old: Vec<&Beat> = old.iter().map(|(beat, _)| beat).collect();
    let keys_new: Vec<&Beat> = new.iter().map(|(beat, _)| beat).collect();
    let steps = script(&keys_old, &keys_new)
        .ok_or("The scenarios differ in too many places to list them one by one")?;
    // Each run of removals and additions between two unchanged beats: a
    // message that kept its ends and moved nowhere changed.
    let mut removed: Vec<usize> = Vec::new();
    let mut added: Vec<usize> = Vec::new();
    let flush = |removed: &mut Vec<usize>, added: &mut Vec<usize>, out: &mut Vec<Change>| {
        let mut taken = vec![false; added.len()];
        for &i in removed.iter() {
            let (beat, line) = &old[i];
            let partner = added
                .iter()
                .enumerate()
                .position(|(k, &j)| !taken[k] && same_ends(beat, &new[j].0));
            match partner {
                Some(k) => {
                    taken[k] = true;
                    let (other, new_line) = &new[added[k]];
                    out.push(change(
                        ChangeKind::Changed,
                        reworded(beat, other),
                        Some(*line),
                        Some(*new_line),
                    ));
                }
                None => {
                    if let Some(message) = told(beat, false) {
                        out.push(change(ChangeKind::Removed, message, Some(*line), None));
                    }
                }
            }
        }
        for (k, &j) in added.iter().enumerate() {
            let (beat, line) = &new[j];
            if !taken[k]
                && let Some(message) = told(beat, true)
            {
                out.push(change(ChangeKind::Added, message, None, Some(*line)));
            }
        }
        removed.clear();
        added.clear();
    };
    let mut messages = Vec::new();
    for step in steps {
        match step {
            Step::Same(..) => flush(&mut removed, &mut added, &mut messages),
            Step::Removed(i) => removed.push(i),
            Step::Added(j) => added.push(j),
        }
    }
    flush(&mut removed, &mut added, &mut messages);
    changes.extend(messages);
    Ok(changes)
}

/// The beats of a scenario, each with its line.
fn beats(d: &sequence::Diagram, off: usize) -> Vec<(Beat, usize)> {
    let code = |i: usize| d.participants[i].code.clone();
    let end = |e: End| match e {
        End::Participant(i) => code(i),
        End::Edge(Side::Left) | End::Short(Side::Left) => "the left edge".into(),
        End::Edge(Side::Right) | End::Short(Side::Right) => "the right edge".into(),
    };
    let mut out = Vec::new();
    for item in &d.items {
        let beat = match item {
            Item::Message(m) => (
                Beat::Message {
                    from: end(m.from),
                    to: end(m.to),
                    text: m.text.as_ref().map(|t| plain(&t.value)).unwrap_or_default(),
                    reply: m.arrow.dashed,
                },
                m.line,
            ),
            Item::GroupStart {
                kind, label, line, ..
            } => (
                Beat::Open {
                    keyword: kind.keyword(),
                    label: label.as_ref().map(|t| plain(&t.value)).unwrap_or_default(),
                },
                *line,
            ),
            Item::GroupElse { label, line } => (
                Beat::Else {
                    label: label.as_ref().map(|t| plain(&t.value)).unwrap_or_default(),
                },
                *line,
            ),
            Item::GroupEnd { line } => (Beat::Close, *line),
            Item::Ref { over, text, .. } => (
                Beat::Ref {
                    over: over.iter().map(|&i| code(i)).collect(),
                    text: plain(&text.value),
                },
                text.line,
            ),
            Item::Destroy { participant, line } => (Beat::Destroy(code(*participant)), *line),
            _ => continue,
        };
        out.push((beat.0, beat.1 + off));
    }
    out
}

/// A label as text: line breaks (`\n`, `<br>`) as spaces, runs of spaces as
/// one.
fn plain(text: &str) -> String {
    text.replace("\\n", " ")
        .replace("<br/>", " ")
        .replace("<br>", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn same_ends(a: &Beat, b: &Beat) -> bool {
    match (a, b) {
        (Beat::Message { from, to, .. }, Beat::Message { from: f, to: t, .. }) => {
            from == f && to == t
        }
        (Beat::Open { keyword, .. }, Beat::Open { keyword: k, .. }) => keyword == k,
        (Beat::Else { .. }, Beat::Else { .. }) => true,
        _ => false,
    }
}

/// What changed between two beats with the same ends.
fn reworded(old: &Beat, new: &Beat) -> String {
    match (old, new) {
        (
            Beat::Message {
                from,
                to,
                text: was,
                reply: was_reply,
            },
            Beat::Message {
                text: is,
                reply: is_reply,
                ..
            },
        ) => {
            let mut parts = Vec::new();
            if was != is {
                parts.push(format!("now says {} instead of {}", said(is), said(was)));
            }
            if was_reply != is_reply {
                parts.push(match is_reply {
                    true => "is now a reply".to_owned(),
                    false => "is no longer a reply".to_owned(),
                });
            }
            format!("The message from {from} to {to} {}.", parts.join(" and "))
        }
        (
            Beat::Open {
                keyword,
                label: was,
            },
            Beat::Open { label: is, .. },
        ) => {
            format!("The {keyword} {} is now {}.", said(was), said(is))
        }
        (Beat::Else { label: was }, Beat::Else { label: is }) => {
            format!("The else {} is now {}.", said(was), said(is))
        }
        _ => String::new(),
    }
}

fn said(text: &str) -> String {
    match text.is_empty() {
        true => "nothing".into(),
        false => text.to_owned(),
    }
}

/// A beat that is new (`added`) or gone, as a message; `None` for the end
/// of a block, which its start already tells.
fn told(beat: &Beat, added: bool) -> Option<String> {
    let tell = |what: String| match added {
        true => format!("New {what}."),
        false => format!("{} removed.", capital(&what)),
    };
    let labelled = |keyword: &str, label: &str| match label.is_empty() {
        true => keyword.to_owned(),
        false => format!("{keyword} {label}"),
    };
    Some(match beat {
        Beat::Message {
            from,
            to,
            text,
            reply,
        } => {
            let what = if *reply { "reply" } else { "message" };
            let text = match text.is_empty() {
                true => String::new(),
                false => format!(": {text}"),
            };
            tell(format!("{what} from {from} to {to}{text}"))
        }
        Beat::Open { keyword, label } => tell(labelled(keyword, label)),
        Beat::Else { label } => tell(labelled("else", label)),
        Beat::Close => return None,
        Beat::Ref { over, text } => tell(format!("ref over {}: {text}", over.join(", "))),
        Beat::Destroy(who) => match added {
            true => format!("{who} is now destroyed here."),
            false => format!("{who} is no longer destroyed here."),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn puml(body: &str) -> String {
        format!("@startuml\n{body}@enduml\n")
    }

    fn messages(old: &str, new: &str) -> Vec<(ChangeKind, String)> {
        diff(
            &puml(old),
            Language::PlantUml,
            &puml(new),
            Language::PlantUml,
            &[],
        )
        .unwrap()
        .into_iter()
        .map(|c| (c.kind, c.message))
        .collect()
    }

    const DOOR: &str = "[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n";

    #[test]
    fn layout_is_no_change() {
        let moved = "Closed --> Locked : lock\n[*] --> Closed\n  Open --> Closed : close\nLocked --> Closed : unlock\nClosed --> Open : open\nstate Closed #pink\nnote right of Open : wide\n";
        assert!(messages(DOOR, moved).is_empty());
        // The same machine in Mermaid.
        let mermaid = "stateDiagram-v2\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n";
        let changes = diff(
            &puml(DOOR),
            Language::PlantUml,
            mermaid,
            Language::Mermaid,
            &[],
        );
        assert_eq!(changes, Ok(Vec::new()));
    }

    #[test]
    fn tells_states_and_transitions_that_changed() {
        let new = "[*] --> Open\nClosed --> Open : open [free] / beep\nOpen --> Jammed : close\nClosed --> Locked : lock, bolt\nstate Jammed\n";
        let got = messages(DOOR, new);
        let want = [
            (ChangeKind::Added, "New state Jammed."),
            (
                ChangeKind::Changed,
                "The machine now starts in Open instead of Closed.",
            ),
            (
                ChangeKind::Changed,
                "Closed to Open on open now has the guard [free].",
            ),
            (ChangeKind::Changed, "Closed to Open on open now does beep."),
            (
                ChangeKind::Changed,
                "On close, Open now goes to Jammed instead of Closed.",
            ),
            (
                ChangeKind::Changed,
                "Closed to Locked now happens on bolt, lock; it happened on lock.",
            ),
            (
                ChangeKind::Removed,
                "Transition removed: Locked to Closed on unlock.",
            ),
            (
                ChangeKind::Found,
                "Nothing leaves Jammed. If the machine ends there, add Jammed --> [*].",
            ),
            (
                ChangeKind::Found,
                "State Closed cannot be reached from the start.",
            ),
        ];
        for (kind, message) in want {
            assert!(
                got.iter().any(|(k, m)| *k == kind && m == message),
                "{kind:?} {message:?} not in {got:#?}"
            );
        }
    }

    #[test]
    fn a_new_target_with_a_new_guard_is_one_change() {
        let got = messages(
            DOOR,
            &DOOR.replace("Open --> Closed : close", "Open --> Jammed : close [stuck]"),
        );
        for want in [
            "On close [stuck], Open now goes to Jammed instead of Closed.",
            "Open to Jammed on close now has the guard [stuck].",
        ] {
            assert!(
                got.iter().any(|(_, m)| m == want),
                "{want:?} not in {got:#?}"
            );
        }
        assert!(
            !got.iter().any(|(k, _)| *k == ChangeKind::Removed),
            "{got:#?}"
        );
    }

    #[test]
    fn tells_where_a_state_moved() {
        let old =
            "[*] --> Off\nOff --> Idle : on\nIdle --> Off : off\nstate On {\n[*] --> Busy\n}\n";
        let new =
            "[*] --> Off\nstate On {\n[*] --> Idle\n}\nOff --> Idle : on\nIdle --> Off : off\n";
        let got = messages(old, new);
        for want in [
            "State Busy removed.",
            "Idle now sits in On; it sat at the top.",
            "On now starts in Idle instead of Busy.",
        ] {
            assert!(
                got.iter().any(|(_, m)| m == want),
                "{want:?} not in {got:#?}"
            );
        }
    }

    #[test]
    fn tells_messages_and_blocks_of_a_scenario() {
        let old = "Alice -> Shop : order(cart)\nShop --> Alice : total\nalt paid\nShop -> Store : ship\nelse\nShop -> Alice : remind\nend\n";
        let new = "participant Bank\nAlice -> Shop : order(cart, coupon)\nShop -> Alice : total\nAlice -> Bank : pay\nloop daily\nShop -> Store : ship\nend\n";
        let got = messages(old, new);
        for want in [
            "New participant Bank.",
            "The message from Alice to Shop now says order(cart, coupon) instead of order(cart).",
            "The message from Shop to Alice is no longer a reply.",
            "New message from Alice to Bank: pay.",
            "New loop daily.",
            "Alt paid removed.",
            "Message from Shop to Alice: remind removed.",
        ] {
            assert!(
                got.iter().any(|(_, m)| m == want),
                "{want:?} not in {got:#?}"
            );
        }
    }

    #[test]
    fn says_why_it_does_not_compare() {
        let state = puml("[*] --> A\n");
        let sequence = puml("A -> B : x\n");
        let classes = puml("class A\nclass B\nA --> B\n");
        let err = diff(
            &state,
            Language::PlantUml,
            &sequence,
            Language::PlantUml,
            &[],
        )
        .unwrap_err();
        assert!(err.contains("different kinds"), "{err}");
        let err = diff(
            &classes,
            Language::PlantUml,
            &classes,
            Language::PlantUml,
            &[],
        )
        .unwrap_err();
        assert!(err.contains("not of a class diagram"), "{err}");
        assert_eq!(a_kind("Activity diagram"), "an activity diagram");
        assert_eq!(a_kind("WBS"), "a WBS");
    }
}
