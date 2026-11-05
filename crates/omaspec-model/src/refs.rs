//! Where a name is used in a project
//! (`docs/adr/0019-where-a-name-is-used.md`): the places that name a
//! diagram, a state or an event, compared as links compare names.

use std::collections::BTreeSet;
use std::ops::Range;

use omaspec_core::lang::strip_mermaid_fence;
use omaspec_core::{Language, markdown, project};
use omaspec_uml::sequence::{self, End, Item};
use omaspec_uml::tree;

use crate::across::{event_of, key, read_sequence};
use crate::{Kind, Machine, Spec, Specs};

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
    /// A map's box that opens the diagram.
    Box,
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
            RefKind::Box => "box",
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
        for map in &project.pieces {
            for (_, line) in map.boxes.iter().filter(|(text, _)| key(text) == wanted) {
                push(out, map, *line, RefKind::Box, None);
            }
        }
        if diagram.machine.is_some() {
            for scenario in &project.pieces {
                for line in scenario.participants_playing(d, &project) {
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
                for line in trace.lines_where(x, &project, |e| {
                    e.state.as_deref().is_some_and(|s| key(s) == wanted)
                }) {
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
            for line in scenario.messages_to(x, &wanted, &project) {
                push(out, scenario, line, RefKind::Message, Some(piece.file));
            }
        }
        for trace in &project.traces {
            for line in trace.lines_where(x, &project, |e| {
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
}

/// A diagram: a file's own, or one of a Markdown document's.
pub(crate) struct Piece<'a> {
    pub(crate) file: &'a str,
    /// The whole file's text.
    file_text: &'a str,
    pub(crate) code: String,
    pub(crate) language: Language,
    /// A document's block, whose lines are the document's.
    block: Option<markdown::Block>,
    /// The names it goes by.
    pub(crate) names: Vec<String>,
    pub(crate) machine: Option<Machine>,
    sequence: Option<(sequence::Diagram, usize)>,
    /// A map's boxes: their words and lines.
    pub(crate) boxes: Vec<(String, usize)>,
}

struct Trace<'a> {
    file: &'a str,
    text: &'a str,
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
                }),
            }
        }
        Project {
            pieces,
            traces,
            specs,
        }
    }

    /// The machines that go by `name`.
    pub(crate) fn machines_named(&self, name: &str) -> Vec<usize> {
        self.specs
            .named(&[name])
            .into_iter()
            .filter_map(|spec| self.piece_of(spec))
            .collect()
    }

    /// The piece that holds `spec`'s machine.
    fn piece_of(&self, spec: &Spec) -> Option<usize> {
        self.pieces
            .iter()
            .position(|p| p.file == spec.file && p.machine.as_ref() == Some(&spec.machine))
    }

    /// The machine the participant or object `names` plays: one, or none.
    fn played_by(&self, names: &[&str]) -> Option<usize> {
        match self.specs.named(names).as_slice() {
            [spec] => self.piece_of(spec),
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
        let boxes = boxes(&code, language);
        Piece {
            file: file.path,
            file_text: file.text,
            code,
            language,
            block,
            names,
            machine,
            sequence,
            boxes,
        }
    }

    /// The file's line for `line` of the code.
    pub(crate) fn doc_line(&self, line: usize) -> usize {
        self.block.as_ref().map_or(line, |b| b.doc_line(line))
    }

    fn line_text(&self, line: usize) -> &str {
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
    fn participants_playing(&self, d: usize, project: &Project) -> Vec<usize> {
        let Some((diagram, offset)) = &self.sequence else {
            return Vec::new();
        };
        diagram
            .participants
            .iter()
            .filter(|p| project.played_by(&[&p.code, &p.label]) == Some(d))
            .map(|p| p.line + offset)
            .collect()
    }

    /// The lines of the messages that send the event of key `wanted` to a
    /// participant playing the machine of piece `x`.
    fn messages_to(&self, x: usize, wanted: &str, project: &Project) -> Vec<usize> {
        let Some((diagram, offset)) = &self.sequence else {
            return Vec::new();
        };
        let plays: Vec<bool> = diagram
            .participants
            .iter()
            .map(|p| project.played_by(&[&p.code, &p.label]) == Some(x))
            .collect();
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
        let names = &project.pieces[d].names;
        self.entries()
            .filter(|(_, e)| {
                let k = key(name(e));
                names.iter().any(|n| key(n) == k)
            })
            .map(|(line, _)| line)
            .collect()
    }

    /// The lines of objects that play the machine of piece `x` and for
    /// which `keep` holds.
    fn lines_where(
        &self,
        x: usize,
        project: &Project,
        keep: impl Fn(&crate::trace::Entry) -> bool,
    ) -> Vec<usize> {
        self.entries()
            .filter(|(_, e)| keep(e))
            .filter(|(_, e)| {
                project.played_by(&[e.machine.as_deref().unwrap_or(&e.object)]) == Some(x)
            })
            .map(|(line, _)| line)
            .collect()
    }

    fn entries(&self) -> impl Iterator<Item = (usize, crate::trace::Entry)> + '_ {
        self.text
            .split('\n')
            .enumerate()
            .filter(|(_, text)| !text.trim().is_empty())
            .filter_map(|(line, text)| crate::trace::entry(line, text).ok().map(|e| (line, e)))
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

/// The words and lines of a mind map's or a WBS's boxes.
fn boxes(code: &str, language: Language) -> Vec<(String, usize)> {
    fn ideas(list: &[tree::Idea], out: &mut Vec<String>) {
        for idea in list {
            out.push(idea.node.text.value.clone());
            ideas(&idea.children, out);
        }
    }
    fn elements(e: &tree::Element, out: &mut Vec<String>) {
        out.push(e.node.text.value.clone());
        for child in e.left.iter().chain(&e.right) {
            elements(child, out);
        }
    }
    let mind = |m: &tree::MindMap, out: &mut Vec<String>| {
        for map in &m.maps {
            out.push(map.root.text.value.clone());
            ideas(&map.right, out);
            ideas(&map.left, out);
        }
    };
    let mut words = Vec::new();
    match language {
        Language::Mermaid => {
            let body = strip_mermaid_fence(code);
            let header = body
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with("%%") && !l.starts_with("---"));
            if header.is_some_and(|h| h.starts_with("mindmap"))
                && let Some(m) = tree::from_mermaid(body)
            {
                mind(&m, &mut words);
            }
        }
        Language::PlantUml => {
            let lower = code.trim_start().to_ascii_lowercase();
            if lower.starts_with("@startmindmap") || lower.starts_with("@startwbs") {
                match omaspec_uml::parse(code) {
                    Ok(omaspec_uml::Diagram::MindMap(m)) => mind(&m, &mut words),
                    Ok(omaspec_uml::Diagram::Wbs(w)) => elements(&w.root, &mut words),
                    _ => {}
                }
            }
        }
    }
    let mut seen: Vec<&str> = Vec::new();
    words
        .iter()
        .filter_map(|text| {
            let nth = seen.iter().filter(|s| **s == text.as_str()).count();
            seen.push(text);
            omaspec_core::locate::line_of(code, text, nth).map(|line| (text.clone(), line))
        })
        .collect()
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

#[cfg(test)]
mod tests;
