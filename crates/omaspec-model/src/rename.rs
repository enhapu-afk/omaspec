//! Renaming a diagram, a state or an event everywhere it is used
//! (`docs/adr/0019-where-a-name-is-used.md`): a plan of edits, line by
//! line, that the command line and the editor make alike. Each use is
//! spelled as the old one was, so an identifier stays one.

use std::ops::Range;

use crate::Specs;
use crate::across::key;
use crate::refs::{File, Piece, Project, Ref, RefKind, event_lines, refs, spans, states_named};

/// Which of the things an old name may be a rename renames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thing {
    Diagram,
    State,
    Event,
}

/// One replacement on one line of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub file: String,
    /// Zero-based.
    pub line: usize,
    /// The bytes of the line replaced.
    pub range: Range<usize>,
    pub with: String,
}

/// The edits a rename makes, and what it leaves alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// What is renamed, in words: `the state Open of door.puml`.
    pub what: String,
    /// In file and line order, each line's left to right.
    pub edits: Vec<Edit>,
    /// The traces that name the old name: records, never rewritten.
    pub traces: Vec<Ref>,
    /// What else the writer should know.
    pub notes: Vec<String>,
}

impl Plan {
    /// The files the plan changes, each once.
    pub fn files(&self) -> Vec<&str> {
        let mut files: Vec<&str> = self.edits.iter().map(|e| e.file.as_str()).collect();
        files.dedup();
        files
    }

    /// `text`, the file at `file`, with the plan's edits made.
    pub fn apply(&self, file: &str, text: &str) -> String {
        let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
        // Right to left, so the ranges before stay where they were.
        for edit in self.edits.iter().filter(|e| e.file == file).rev() {
            if let Some(line) = lines.get_mut(edit.line)
                && line.get(edit.range.clone()).is_some()
            {
                line.replace_range(edit.range.clone(), &edit.with);
            }
        }
        lines.join("\n")
    }
}

/// The plan that renames `old` to `new` in `files`. When `old` is several
/// things, `machine` and `thing` say which, else the rename is refused
/// with what they are; so is a `new` that is taken.
pub fn rename(
    old: &str,
    new: &str,
    machine: Option<&str>,
    thing: Option<Thing>,
    files: &[File],
    specs: &Specs,
) -> Result<Plan, String> {
    let (k, n) = (key(old), key(new));
    if k.is_empty() {
        return Err(format!("{old:?} has no letters or digits to compare"));
    }
    if n.is_empty() {
        return Err(format!("{new:?} has no letters or digits"));
    }
    if new.contains(['"', ':', '\n', '\r']) || new.trim() != new {
        return Err(format!(
            "{new:?} cannot be a name: no quotes, colons or line breaks, nor spaces around it"
        ));
    }
    if k == n {
        return Err(format!("{new} is {old} already, as names compare"));
    }
    let project = Project::read(files, specs);
    let only = match machine {
        Some(m) => {
            let found = project.machines_named(m);
            if found.is_empty() {
                return Err(format!("No state machine of the project is named {m}"));
            }
            Some(found)
        }
        None => None,
    };
    let ok = |x: usize| only.as_ref().is_none_or(|o| o.contains(&x));
    let pieces = &project.pieces;
    let diagrams: Vec<usize> = (0..pieces.len())
        .filter(|&x| ok(x) && pieces[x].names.iter().any(|name| key(name) == k))
        .collect();
    let machines = || (0..pieces.len()).filter(|&x| ok(x) && pieces[x].machine.is_some());
    let mut candidates: Vec<(Thing, usize)> = Vec::new();
    candidates.extend(diagrams.iter().map(|&d| (Thing::Diagram, d)));
    // A state named like a diagram is a link to it, renamed with it.
    candidates.extend(
        machines()
            .filter(|&x| diagrams.is_empty() || diagrams.contains(&x))
            .filter(|&x| {
                states_named(pieces[x].machine.as_ref().unwrap(), &k)
                    .next()
                    .is_some()
            })
            .map(|x| (Thing::State, x)),
    );
    candidates.extend(
        machines()
            .filter(|&x| !event_lines(pieces[x].machine.as_ref().unwrap(), &k).is_empty())
            .map(|x| (Thing::Event, x)),
    );
    candidates.retain(|(t, _)| thing.is_none_or(|wanted| wanted == *t));
    let (what, x) = match candidates.as_slice() {
        [] => return Err(format!("Nothing in the project is called {old}")),
        [one] => *one,
        many => {
            let listed: Vec<String> = many
                .iter()
                .map(|(t, x)| describe(*t, old, pieces[*x].file))
                .collect();
            return Err(format!(
                "{old} is {}: say which with --machine, and --diagram, --state or --event",
                listed.join(", ")
            ));
        }
    };
    let mut plan = Plan {
        what: describe(what, old, pieces[x].file),
        ..Plan::default()
    };
    match what {
        Thing::Diagram => diagram(&project, x, &k, &n, new, &mut plan)?,
        Thing::State => {
            if states_named(pieces[x].machine.as_ref().unwrap(), &n)
                .next()
                .is_some()
            {
                return Err(format!("{} has a state {new} already", pieces[x].file));
            }
            state(&pieces[x], &k, new, &mut plan);
        }
        Thing::Event => {
            if !event_lines(pieces[x].machine.as_ref().unwrap(), &n).is_empty() {
                return Err(format!("{} has an event {new} already", pieces[x].file));
            }
            event(&project, x, &k, new, &mut plan);
        }
    }
    let file = pieces[x].file;
    plan.traces = refs(old, machine, files, specs)
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.kind == RefKind::Trace && r.machine.as_deref() == Some(file))
        .collect();
    plan.edits
        .sort_by(|a, b| (&a.file, a.line, a.range.start).cmp(&(&b.file, b.line, b.range.start)));
    plan.edits.dedup();
    Ok(plan)
}

/// What a name at a place of a file is, for a rename there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    /// As it is written there.
    pub name: String,
    /// The file of the state machine whose state or event it is, or of
    /// the diagram a participant plays.
    pub machine: Option<String>,
    pub thing: Thing,
}

/// The name at byte `col` of the file's `line` (zero-based): the diagram
/// a title, a box or a participant names, or a state or an event of a
/// machine; `None` where no such name is.
pub fn named_at(
    file: &str,
    line: usize,
    col: usize,
    files: &[File],
    specs: &Specs,
) -> Option<Named> {
    let project = Project::read(files, specs);
    let pieces = &project.pieces;
    let piece = pieces
        .iter()
        .find(|p| p.file == file && p.block.as_ref().is_none_or(|b| b.code_line(line).is_some()))?;
    let code_line = piece
        .block
        .as_ref()
        .map_or(Some(line), |b| b.code_line(line))?;
    let text = piece.line_text(line);
    let at = |k: &str, part: Range<usize>| {
        spans(text, k)
            .into_iter()
            .find(|s| s.start <= col && col <= s.end && s.start >= part.start && s.end <= part.end)
    };
    let found = |span: Range<usize>, machine: Option<&str>, thing| Named {
        name: text[span].to_owned(),
        machine: machine.map(str::to_owned),
        thing,
    };
    let whole = 0..text.len();
    let label = (head(text) + 1).min(text.len())..text.len();
    let left = 0..head(text);
    // Where the diagram says its name.
    for name in &piece.names {
        if piece.name_lines(&key(name)).contains(&code_line)
            && let Some(span) = at(&key(name), whole.clone())
        {
            return Some(found(span, None, Thing::Diagram));
        }
    }
    // A map's box.
    for (words, _) in piece.boxes.iter().filter(|(_, l)| *l == code_line) {
        if let Some(span) = at(&key(words), whole.clone()) {
            return Some(found(span, None, Thing::Diagram));
        }
    }
    // A state left of a label, an event right of it.
    if let Some(m) = &piece.machine {
        for state in &m.states {
            if let Some(span) = at(&key(&state.name), left.clone()) {
                return Some(found(span, Some(piece.file), Thing::State));
            }
        }
        for t in m.transitions.iter().filter(|t| t.line == code_line) {
            for event in &t.trigger.events {
                if let Some(span) = at(&key(crate::across::event_of(event)), label.clone()) {
                    return Some(found(span, Some(piece.file), Thing::Event));
                }
            }
        }
    }
    // A participant, or the event of a message sent to one.
    if let Some((diagram, offset)) = &piece.sequence {
        let plays =
            |p: &omaspec_uml::sequence::Participant| project.played_by(&[&p.code, &p.label]);
        for p in &diagram.participants {
            for name in [&p.code, &p.label] {
                if let Some(span) = at(&key(name), left.clone())
                    && let Some(m) = plays(p)
                {
                    // An alias is no name of the machine's: its own is.
                    let machine = &pieces[m];
                    let written = key(&text[span.clone()]);
                    let mut named = found(span, Some(machine.file), Thing::Diagram);
                    if !machine.names.iter().any(|n| key(n) == written) {
                        named.name = machine.names.first()?.clone();
                    }
                    return Some(named);
                }
            }
        }
        for item in &diagram.items {
            if let omaspec_uml::sequence::Item::Message(m) = item
                && m.line + offset == code_line
                && let omaspec_uml::sequence::End::Participant(to) = m.to
                && let Some(machine) = plays(&diagram.participants[to])
                && let Some(text) = &m.text
                && let Some(span) = at(&key(crate::across::event_of(&text.value)), label.clone())
            {
                return Some(found(span, Some(pieces[machine].file), Thing::Event));
            }
        }
    }
    None
}

fn describe(thing: Thing, name: &str, file: &str) -> String {
    match thing {
        Thing::Diagram => format!("the diagram {name} ({file})"),
        Thing::State => format!("the state {name} of {file}"),
        Thing::Event => format!("the event {name} of {file}"),
    }
}

/// The diagram of piece `d`: where it says its name, the boxes, the
/// participants that play it, the states that open it.
fn diagram(
    project: &Project,
    d: usize,
    k: &str,
    n: &str,
    new: &str,
    plan: &mut Plan,
) -> Result<(), String> {
    let pieces = &project.pieces;
    let piece = &pieces[d];
    if let Some(other) = pieces
        .iter()
        .enumerate()
        .find(|(x, p)| *x != d && p.names.iter().any(|name| key(name) == n))
    {
        return Err(format!("{new} is already the name of {}", other.1.file));
    }
    let lines = piece.name_lines(k);
    if lines.is_empty() {
        return Err(format!(
            "{} goes by that name as its file only: give it a title first (title {new})",
            piece.file
        ));
    }
    for line in lines {
        replace(
            plan,
            piece,
            piece.doc_line(line),
            k,
            new,
            |_| Some(0..usize::MAX),
            true,
        );
    }
    let (dir, name) = piece.file.rsplit_once('/').unwrap_or(("", piece.file));
    let (stem, extension) = name.split_once('.').unwrap_or((name, ""));
    if piece.block.is_none() && key(stem) == k {
        // Spelled as the file's name was.
        let moved = format!("{}.{extension}", spell(stem, new, false));
        let moved = if dir.is_empty() {
            moved
        } else {
            format!("{dir}/{moved}")
        };
        plan.notes.push(format!(
            "{} still goes by {stem}, its file name; `git mv {} {moved}` drops it",
            piece.file, piece.file
        ));
    }
    for map in pieces {
        for (_, line) in map.boxes.iter().filter(|(text, _)| key(text) == k) {
            replace(
                plan,
                map,
                map.doc_line(*line),
                k,
                new,
                |_| Some(0..usize::MAX),
                true,
            );
        }
    }
    if piece.machine.is_some() {
        for scenario in pieces {
            if scenario.participants_playing(d).is_empty() {
                continue;
            }
            for line in scenario.lines() {
                let text = scenario.line_text(line);
                if names_participants(text) {
                    replace(plan, scenario, line, k, new, |t| Some(0..head(t)), false);
                }
            }
        }
    }
    for (x, other) in pieces.iter().enumerate() {
        if x != d
            && let Some(m) = &other.machine
            && states_named(m, k).next().is_some()
        {
            state(other, k, new, plan);
        }
    }
    Ok(())
}

/// The state of key `k` of the machine in `piece`: its places on every line
/// of the machine left of a label, notes' text and comments aside.
fn state(piece: &Piece, k: &str, new: &str, plan: &mut Plan) {
    let mut in_note = false;
    for line in piece.lines() {
        let text = piece.line_text(line);
        let trimmed = text.trim().to_ascii_lowercase();
        if in_note {
            in_note = !(trimmed.starts_with("end note") || trimmed.starts_with("endnote"));
            continue;
        }
        if trimmed.starts_with('\'')
            || trimmed.starts_with("%%")
            || trimmed.starts_with('@')
            || trimmed.starts_with("title")
            || trimmed.starts_with("skinparam")
            || trimmed.starts_with("```")
        {
            continue;
        }
        if trimmed.starts_with("note") && head(text) == text.len() {
            // A note of several lines, unless it is `note "…" as N`: its
            // first line names the state, the rest is its text.
            in_note = !trimmed.starts_with("note \"");
        }
        replace(plan, piece, line, k, new, |t| Some(0..head(t)), false);
    }
}

/// The event of key `k` of the machine of piece `x`: its transitions'
/// triggers and the messages sent to the machine's players.
fn event(project: &Project, x: usize, k: &str, new: &str, plan: &mut Plan) {
    let piece = &project.pieces[x];
    let machine = piece.machine.as_ref().unwrap();
    // A trigger ends where its guard or its action starts.
    let trigger = |t: &str| {
        let start = head(t) + 1;
        let rest = t.get(start..)?;
        let end = rest.find(['[', '/']).unwrap_or(rest.len());
        Some(start..start + end)
    };
    for line in event_lines(machine, k) {
        replace(plan, piece, piece.doc_line(line), k, new, trigger, false);
    }
    // A message's event ends where its arguments start.
    let message = |t: &str| {
        let start = head(t) + 1;
        let rest = t.get(start..)?;
        let end = rest.find(['(', '\\']).unwrap_or(rest.len());
        Some(start..start + end)
    };
    for scenario in &project.pieces {
        for line in scenario.messages_to(x, k) {
            replace(
                plan,
                scenario,
                scenario.doc_line(line),
                k,
                new,
                message,
                false,
            );
        }
    }
}

/// Replaces each spelling of key `k` on the file's `line` of `piece`
/// within the part `part` picks, with `new` spelled as it was: as written
/// in `prose` or between quotes, else as one word in its style.
fn replace(
    plan: &mut Plan,
    piece: &Piece,
    line: usize,
    k: &str,
    new: &str,
    part: impl Fn(&str) -> Option<Range<usize>>,
    prose: bool,
) {
    let text = piece.line_text(line);
    let Some(part) = part(text) else {
        return;
    };
    let part = part.start.min(text.len())..part.end.min(text.len());
    for span in spans(text, k) {
        if span.start < part.start || span.end > part.end {
            continue;
        }
        let quoted = text[..span.start].matches('"').count() % 2 == 1;
        let with = spell(&text[span.clone()], new, prose || quoted);
        plan.edits.push(Edit {
            file: piece.file.to_owned(),
            line,
            range: span,
            with,
        });
    }
}

/// Where a line's head ends: at its first colon outside quotes, where a
/// transition's or a message's label starts.
fn head(text: &str) -> usize {
    let mut quoted = false;
    for (at, c) in text.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => return at,
            _ => {}
        }
    }
    text.len()
}

/// Whether a scenario's line names participants left of its label: a
/// declaration, a message, a note or an activation.
fn names_participants(text: &str) -> bool {
    let line = text.trim().to_ascii_lowercase();
    let head = &line[..head(&line)];
    const STARTS: [&str; 14] = [
        "participant",
        "actor",
        "boundary",
        "control",
        "entity",
        "database",
        "collections",
        "queue",
        "create",
        "activate",
        "deactivate",
        "destroy",
        "note",
        "ref",
    ];
    STARTS.iter().any(|s| line.starts_with(s))
        || head.contains("->")
        || head.contains("<-")
        || head.contains("-)")
        || head.contains("-x")
}

/// `new` spelled as `old` was: as written in `prose` or where `old` had
/// spaces, else as one word in `old`'s style (`FrontGate`, `frontGate`,
/// `front_gate`, `front-gate`, `FRONT_GATE`).
fn spell(old: &str, new: &str, prose: bool) -> String {
    if prose || old.contains(' ') {
        return new.to_owned();
    }
    let words: Vec<&str> = new
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let letters = || old.chars().filter(|c| c.is_alphabetic());
    let upper = letters().count() > 1 && letters().all(char::is_uppercase);
    let lower = letters().all(char::is_lowercase);
    let capital = |w: &str| {
        let mut chars = w.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
    };
    let joined = |sep: &str, word: &dyn Fn(&str) -> String| {
        words.iter().map(|w| word(w)).collect::<Vec<_>>().join(sep)
    };
    let sep = if old.contains('-') { "-" } else { "_" };
    if upper {
        joined(sep, &|w: &str| w.to_uppercase())
    } else if lower {
        joined(sep, &|w: &str| w.to_lowercase())
    } else if old.contains(['_', '-']) {
        joined(sep, &|w: &str| capital(w))
    } else if old.starts_with(char::is_lowercase) {
        let mut out = words.first().map(|w| w.to_lowercase()).unwrap_or_default();
        out.extend(words.iter().skip(1).map(|w| capital(w)));
        out
    } else {
        joined("", &|w: &str| capital(w))
    }
}

impl Piece<'_> {
    /// The file's lines the piece's code is on.
    fn lines(&self) -> impl Iterator<Item = usize> + '_ {
        let count = self.code.split('\n').count();
        let mut lines: Vec<usize> = (0..count).map(|line| self.doc_line(line)).collect();
        lines.dedup();
        lines.into_iter()
    }
}

#[cfg(test)]
mod tests;
