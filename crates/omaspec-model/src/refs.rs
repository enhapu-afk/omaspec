//! Where a name is used in a project
//! (`docs/adr/0019-where-a-name-is-used.md`): the places that name a
//! diagram, a state or an event, compared as links compare names.

use std::collections::BTreeSet;
use std::ops::Range;

use omaspec_core::{Language, markdown, project};
use omaspec_uml::sequence::{self, End, Item};

use crate::across::{event_of, key, read_sequence};
use crate::{Kind, Machine, Specs};

/// A file of a project, read.
#[derive(Debug, Clone, Copy)]
pub struct File<'a> {
    /// Its path in the project, `/` between folders.
    pub path: &'a str,
    pub text: &'a str,
    pub kind: FileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Diagram(Language),
    /// A Markdown document: each of its diagrams on its own lines.
    Document,
    /// A recorded trace, one JSON object per line.
    Trace,
}

/// A place that names a diagram, a state or an event.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ref {
    pub file: String,
    /// Zero-based, on the file's lines.
    pub line: usize,
    pub kind: RefKind,
    /// The state machine a state or an event belongs to, by its file.
    pub machine: Option<String>,
    /// The line, trimmed.
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefKind {
    /// Where a diagram says its name: `@start…`, its title, else its file.
    Diagram,
    /// A scenario's participant that plays the state machine.
    Participant,
    /// A state of another machine that opens the diagram of its name.
    Link,
    /// A line of a machine that declares the state or leads into or out of it.
    State,
    /// A transition of a machine that the event fires.
    Event,
    /// A scenario's message that sends the event to a participant playing
    /// its machine.
    Message,
    /// A line of a recorded trace.
    Trace,
}

impl RefKind {
    pub fn name(self) -> &'static str {
        match self {
            RefKind::Diagram => "diagram",
            RefKind::Participant => "participant",
            RefKind::Link => "link",
            RefKind::State => "state",
            RefKind::Event => "event",
            RefKind::Message => "message",
            RefKind::Trace => "trace",
        }
    }
}

/// Every place in `files` that uses `name`: a diagram it names, and a
/// state or an event of a machine; with `machine`, only that machine's own
/// name, states and events. `specs` are the project's machines, which say
/// who plays which. In file and line order.
pub fn refs(
    name: &str,
    machine: Option<&str>,
    files: &[File],
    specs: &Specs,
) -> Result<Vec<Ref>, String> {
    let wanted = key(name);
    if wanted.is_empty() {
        return Err(format!("{name:?} has no letters or digits to compare"));
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
    let mut out = BTreeSet::new();
    let out = &mut out;

    // The diagrams that go by the name.
    let named: Vec<usize> = project
        .pieces
        .iter()
        .enumerate()
        .filter(|(i, p)| p.names.iter().any(|n| key(n) == wanted) && only_ok(&only, *i))
        .map(|(i, _)| i)
        .collect();
    for &d in &named {
        let diagram = &project.pieces[d];
        let lines = diagram.name_lines(&wanted);
        if lines.is_empty() {
            push(out, diagram, 0, RefKind::Diagram, None);
        }
        for line in lines {
            push(out, diagram, line, RefKind::Diagram, None);
        }
        if diagram.machine.is_some() {
            for scenario in &project.pieces {
                for line in scenario.participants_playing(d) {
                    push(out, scenario, line, RefKind::Participant, None);
                }
            }
        }
        for (x, other) in project.pieces.iter().enumerate() {
            if x == d {
                continue;
            }
            if let Some(m) = &other.machine {
                for state in states_named(m, &wanted) {
                    for line in state_lines(m, state) {
                        push(out, other, line, RefKind::Link, Some(other.file));
                    }
                }
            }
        }
        for trace in &project.traces {
            for line in trace.lines_of(d, &project, |e| e.machine.as_deref().unwrap_or(&e.object)) {
                push_trace(out, trace, line, Some(diagram.file));
            }
        }
    }

    // The states and events of machines that go by the name.
    for (x, piece) in project.pieces.iter().enumerate() {
        let Some(m) = &piece.machine else {
            continue;
        };
        if !only_ok(&only, x) {
            continue;
        }
        // A state named like a diagram is a link to it, listed above.
        let links = named.iter().any(|&d| d != x);
        if !links {
            for state in states_named(m, &wanted) {
                for line in state_lines(m, state) {
                    push(out, piece, line, RefKind::State, Some(piece.file));
                }
            }
            for trace in &project.traces {
                for line in
                    trace.lines_where(x, |e| e.state.as_deref().is_some_and(|s| key(s) == wanted))
                {
                    push_trace(out, trace, line, Some(piece.file));
                }
            }
        }
        let fired = event_lines(m, &wanted);
        if fired.is_empty() {
            continue;
        }
        for line in fired {
            push(out, piece, line, RefKind::Event, Some(piece.file));
        }
        for scenario in &project.pieces {
            for line in scenario.messages_to(x, &wanted) {
                push(out, scenario, line, RefKind::Message, Some(piece.file));
            }
        }
        for trace in &project.traces {
            for line in trace.lines_where(x, |e| {
                e.event.as_deref().is_some_and(|ev| key(ev) == wanted)
            }) {
                push_trace(out, trace, line, Some(piece.file));
            }
        }
    }
    Ok(std::mem::take(out).into_iter().collect())
}

fn push(out: &mut BTreeSet<Ref>, piece: &Piece, line: usize, kind: RefKind, machine: Option<&str>) {
    let line = piece.doc_line(line);
    out.insert(Ref {
        file: piece.file.to_owned(),
        line,
        kind,
        machine: machine.map(str::to_owned),
        text: piece.line_text(line).trim().to_owned(),
    });
}

fn only_ok(only: &Option<Vec<usize>>, piece: usize) -> bool {
    only.as_ref().is_none_or(|o| o.contains(&piece))
}

fn push_trace(out: &mut BTreeSet<Ref>, trace: &Trace, line: usize, machine: Option<&str>) {
    out.insert(Ref {
        file: trace.file.to_owned(),
        line,
        kind: RefKind::Trace,
        machine: machine.map(str::to_owned),
        text: trace.text.lines().nth(line).unwrap_or("").trim().to_owned(),
    });
}

/// The diagrams and traces of a project, read once for its uses.
pub(crate) struct Project<'a> {
    pub(crate) pieces: Vec<Piece<'a>>,
    traces: Vec<Trace<'a>>,
    specs: &'a Specs,
    /// The piece that holds each of `specs`, by its place there.
    spec_piece: Vec<Option<usize>>,
}

/// A diagram: a file's own, or one of a Markdown document's.
pub(crate) struct Piece<'a> {
    pub(crate) file: &'a str,
    /// The whole file's text.
    file_text: &'a str,
    pub(crate) code: String,
    pub(crate) language: Language,
    /// A document's block, whose lines are the document's.
    pub(crate) block: Option<markdown::Block>,
    /// The names it goes by.
    pub(crate) names: Vec<String>,
    pub(crate) machine: Option<Machine>,
    pub(crate) sequence: Option<(sequence::Diagram, usize)>,
    /// A scenario's participants, each with the machine it plays.
    players: Vec<Option<usize>>,
}

struct Trace<'a> {
    file: &'a str,
    text: &'a str,
    /// Its lines that are read, each with the machine its object plays.
    entries: Vec<(usize, crate::trace::Entry, Option<usize>)>,
}

impl<'a> Project<'a> {
    pub(crate) fn read(files: &[File<'a>], specs: &'a Specs) -> Project<'a> {
        let mut pieces = Vec::new();
        let mut traces = Vec::new();
        for file in files {
            match file.kind {
                FileKind::Diagram(language) => pieces.push(Piece::read(
                    file,
                    file.text.to_owned(),
                    language,
                    None,
                    project::names(file.text, language, file.path),
                )),
                FileKind::Document => {
                    for block in markdown::blocks(file.text) {
                        let names = project::names(&block.code, block.language, "");
                        let piece = Piece::read(
                            file,
                            block.code.clone(),
                            block.language,
                            Some(block),
                            names,
                        );
                        // A document's diagram goes by a name as a machine only.
                        let names = if piece.machine.is_some() {
                            piece.names.clone()
                        } else {
                            Vec::new()
                        };
                        pieces.push(Piece { names, ..piece });
                    }
                }
                FileKind::Trace => traces.push(Trace {
                    file: file.path,
                    text: file.text,
                    entries: Vec::new(),
                }),
            }
        }
        // Each machine of `specs` in its piece, found by its file.
        let mut by_file: std::collections::HashMap<&str, Vec<usize>> =
            std::collections::HashMap::new();
        for (x, piece) in pieces.iter().enumerate() {
            if piece.machine.is_some() {
                by_file.entry(piece.file).or_default().push(x);
            }
        }
        let spec_piece = specs
            .iter()
            .map(|spec| {
                by_file
                    .get(spec.file.as_str())?
                    .iter()
                    .copied()
                    .find(|&x| pieces[x].machine.as_ref() == Some(&spec.machine))
            })
            .collect();
        let mut project = Project {
            pieces,
            traces,
            specs,
            spec_piece,
        };
        // Who plays which machine, once.
        let players: Vec<Vec<Option<usize>>> = project
            .pieces
            .iter()
            .map(|piece| match &piece.sequence {
                Some((diagram, _)) => diagram
                    .participants
                    .iter()
                    .map(|p| project.played_by(&[&p.code, &p.label]))
                    .collect(),
                None => Vec::new(),
            })
            .collect();
        for (piece, players) in project.pieces.iter_mut().zip(players) {
            piece.players = players;
        }
        let entries: Vec<_> = project
            .traces
            .iter()
            .map(|trace| {
                trace
                    .text
                    .split('\n')
                    .enumerate()
                    .filter(|(_, text)| !text.trim().is_empty())
                    .filter_map(|(line, text)| {
                        crate::trace::entry(line, text).ok().map(|e| (line, e))
                    })
                    .map(|(line, e)| {
                        let machine =
                            project.played_by(&[e.machine.as_deref().unwrap_or(&e.object)]);
                        (line, e, machine)
                    })
                    .collect()
            })
            .collect();
        for (trace, entries) in project.traces.iter_mut().zip(entries) {
            trace.entries = entries;
        }
        project
    }

    /// The machines that go by `name`, or the one in the file at `name`.
    pub(crate) fn machines_named(&self, name: &str) -> Vec<usize> {
        let in_file: Vec<usize> = (0..self.pieces.len())
            .filter(|&x| self.pieces[x].file == name && self.pieces[x].machine.is_some())
            .collect();
        if !in_file.is_empty() {
            return in_file;
        }
        self.specs
            .indices(&[name])
            .into_iter()
            .filter_map(|spec| self.spec_piece[spec])
            .collect()
    }

    /// The machine the participant or object `names` plays: one, or none.
    pub(crate) fn played_by(&self, names: &[&str]) -> Option<usize> {
        match self.specs.indices(names).as_slice() {
            [spec] => self.spec_piece[*spec],
            _ => None,
        }
    }
}

impl<'a> Piece<'a> {
    fn read(
        file: &File<'a>,
        code: String,
        language: Language,
        block: Option<markdown::Block>,
        names: Vec<String>,
    ) -> Piece<'a> {
        let machine = Machine::read(&code, language);
        let sequence = machine
            .is_none()
            .then(|| read_sequence(&code, language))
            .flatten();
        Piece {
            file: file.path,
            file_text: file.text,
            code,
            language,
            block,
            names,
            machine,
            sequence,
            players: Vec::new(),
        }
    }

    /// The file's line for `line` of the code.
    pub(crate) fn doc_line(&self, line: usize) -> usize {
        self.block.as_ref().map_or(line, |b| b.doc_line(line))
    }

    pub(crate) fn line_text(&self, line: usize) -> &'a str {
        self.file_text.split('\n').nth(line).unwrap_or("")
    }

    /// The lines of the code that write a name of key `wanted`: the
    /// `@start…` line and the title's.
    pub(crate) fn name_lines(&self, wanted: &str) -> Vec<usize> {
        let mut front = false;
        let mut out = Vec::new();
        for (i, line) in self.code.split('\n').enumerate() {
            let trimmed = line.trim();
            let names_it = match self.language {
                Language::PlantUml => {
                    let lower = trimmed.to_ascii_lowercase();
                    lower.starts_with("@start") || lower.starts_with("title ")
                }
                Language::Mermaid => {
                    if trimmed == "---" {
                        front = !front;
                        false
                    } else {
                        front && trimmed.starts_with("title:")
                    }
                }
            };
            if names_it && !spans(line, wanted).is_empty() {
                out.push(i);
            }
        }
        out
    }

    /// The lines of the participants that play the machine of piece `d`.
    pub(crate) fn participants_playing(&self, d: usize) -> Vec<usize> {
        let Some((diagram, offset)) = &self.sequence else {
            return Vec::new();
        };
        diagram
            .participants
            .iter()
            .zip(&self.players)
            .filter(|(_, plays)| **plays == Some(d))
            .map(|(p, _)| p.line + offset)
            .collect()
    }

    /// The lines of the messages that send the event of key `wanted` to a
    /// participant playing the machine of piece `x`.
    pub(crate) fn messages_to(&self, x: usize, wanted: &str) -> Vec<usize> {
        let Some((diagram, offset)) = &self.sequence else {
            return Vec::new();
        };
        if !self.players.contains(&Some(x)) {
            return Vec::new();
        }
        let plays: Vec<bool> = self.players.iter().map(|p| *p == Some(x)).collect();
        diagram
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Message(m) => Some(m),
                _ => None,
            })
            .filter(|m| matches!(m.to, End::Participant(to) if plays[to]))
            .filter(|m| {
                m.text
                    .as_ref()
                    .is_some_and(|t| key(event_of(&t.value)) == wanted)
            })
            .map(|m| m.line + offset)
            .collect()
    }
}

impl Trace<'_> {
    /// The lines whose object (by `name`) goes by the machine of piece `d`
    /// itself.
    fn lines_of(
        &self,
        d: usize,
        project: &Project,
        name: impl Fn(&crate::trace::Entry) -> &str,
    ) -> Vec<usize> {
        let names: Vec<String> = project.pieces[d].names.iter().map(|n| key(n)).collect();
        self.entries
            .iter()
            .filter(|(_, e, _)| names.contains(&key(name(e))))
            .map(|(line, _, _)| *line)
            .collect()
    }

    /// The lines of objects that play the machine of piece `x` and for
    /// which `keep` holds.
    fn lines_where(&self, x: usize, keep: impl Fn(&crate::trace::Entry) -> bool) -> Vec<usize> {
        self.entries
            .iter()
            .filter(|(_, e, machine)| *machine == Some(x) && keep(e))
            .map(|(line, _, _)| *line)
            .collect()
    }
}

/// The states of `m` named with key `wanted`.
pub(crate) fn states_named<'m>(
    m: &'m Machine,
    wanted: &'m str,
) -> impl Iterator<Item = usize> + 'm {
    m.states
        .iter()
        .enumerate()
        .filter(move |(_, s)| s.kind == Kind::State && key(&s.name) == wanted)
        .map(|(i, _)| i)
}

/// The lines of `m` that declare `state` or lead into or out of it.
pub(crate) fn state_lines(m: &Machine, state: usize) -> Vec<usize> {
    let mut lines: Vec<usize> = std::iter::once(m.states[state].line)
        .chain(
            m.transitions
                .iter()
                .filter(|t| t.from == state || t.to == state)
                .map(|t| t.line),
        )
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// The lines of `m`'s transitions that the event of key `wanted` fires.
pub(crate) fn event_lines(m: &Machine, wanted: &str) -> Vec<usize> {
    let mut lines: Vec<usize> = m
        .transitions
        .iter()
        .filter(|t| t.trigger.events.iter().any(|e| key(event_of(e)) == wanted))
        .map(|t| t.line)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// The byte ranges of `line` that spell a name of key `wanted`: its words
/// with the spaces, underscores or dashes between them, case aside
/// (`Door control`, `door_control`, `DoorControl`), from a name's edge to
/// another. An underscore or a dash between letters joins words into one
/// name, so `front_door` holds no `door`.
pub(crate) fn spans(line: &str, wanted: &str) -> Vec<Range<usize>> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let joins = |c: char| c == '_' || c == '-';
    let alnum = |i: usize| chars.get(i).is_some_and(|(_, c)| c.is_alphanumeric());
    let mut out = Vec::new();
    if wanted.is_empty() {
        return out;
    }
    let mut i = 0;
    while i < chars.len() {
        let starts = alnum(i)
            && (i == 0 || (!alnum(i - 1) && !(joins(chars[i - 1].1) && i >= 2 && alnum(i - 2))));
        if !starts {
            i += 1;
            continue;
        }
        let mut got = String::new();
        let mut end = None;
        let mut j = i;
        while j < chars.len() {
            let c = chars[j].1;
            if c.is_alphanumeric() {
                got.extend(c.to_lowercase());
                if !wanted.starts_with(got.as_str()) {
                    break;
                }
                if got.len() == wanted.len() {
                    let ends = !alnum(j + 1)
                        && !(chars.get(j + 1).is_some_and(|(_, c)| joins(*c)) && alnum(j + 2));
                    if ends {
                        end = Some(j + 1);
                    }
                    break;
                }
            } else if !(c == ' ' || joins(c)) || !alnum(j + 1) && c != ' ' {
                break;
            }
            j += 1;
        }
        match end {
            Some(end) => {
                let to = chars.get(end).map_or(line.len(), |(at, _)| *at);
                out.push(chars[i].0..to);
                i = end;
            }
            None => i += 1,
        }
    }
    out
}

/// The links written into `code`, the file at `own` in its project, that
/// lead to a diagram of the project (its file, or its picture beside it)
/// where `has` finds none: `has` is asked for a path in the project
/// without its extension. PlantUML's `[[url]]` and Mermaid's `click …
/// href` count; an address elsewhere, or one that climbs out of the
/// project, is not this check's, nor is a Markdown document's numbered
/// picture (`README-2.svg`).
pub fn gone_links(
    code: &str,
    language: Language,
    own: &str,
    has: &dyn Fn(&str) -> bool,
) -> Vec<omaspec_core::lint::Finding> {
    let mut out = Vec::new();
    for (line, text) in code.split('\n').enumerate() {
        for url in written_links(text, language) {
            let Some(path) = in_project(own, &url) else {
                continue;
            };
            let Some((stem, extension)) = path.rsplit_once('.') else {
                continue;
            };
            let extension = extension.to_ascii_lowercase();
            let picture = matches!(extension.as_str(), "svg" | "png" | "pdf" | "jpg" | "jpeg");
            let diagram = Language::from_file_name(&path).is_some();
            let numbered = stem
                .rsplit_once('-')
                .is_some_and(|(_, n)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            if !(picture || diagram) || (picture && numbered) || has(stem) {
                continue;
            }
            out.push(omaspec_core::lint::Finding {
                line,
                severity: omaspec_core::lint::Severity::Warning,
                message: format!(
                    "The link to {url} leads to no diagram of the project: there is none at {stem}."
                ),
            });
        }
    }
    out
}

/// The addresses of the links on one line of a diagram.
fn written_links(text: &str, language: Language) -> Vec<String> {
    match language {
        Language::Mermaid => omaspec_uml::click::click(text)
            .and_then(|c| c.link)
            .map(|link| link.url)
            .into_iter()
            .collect(),
        Language::PlantUml => {
            let mut out = Vec::new();
            let mut rest = text;
            while let Some(open) = rest.find("[[") {
                let after = &rest[open + 2..];
                let Some(close) = after.find("]]") else {
                    break;
                };
                if let Ok((link, _)) = omaspec_uml::creole_link(&after[..close]) {
                    out.push(link.url);
                }
                rest = &after[close + 2..];
            }
            out
        }
    }
}

/// The path in the project that `url`, written in the file at `own`,
/// leads to; `None` for an address elsewhere or above the project.
fn in_project(own: &str, url: &str) -> Option<String> {
    let url = url.split(['#', '?']).next()?.trim();
    if url.is_empty() || url.contains(':') || url.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = own.split('/').collect();
    parts.pop();
    for part in url.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests;
