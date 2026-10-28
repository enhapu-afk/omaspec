//! From the body's lines to a [`Diagram`]. A line the parser does not
//! understand declines the whole diagram, naming that line.
//!
//! Names belong to the whole diagram, as in PlantUML: a name already used
//! inside another state is that state, and `state Name {` opens it where
//! it is. A new name is made in the state that is open. Transitions are
//! sorted into containers at the end, since a split into concurrent
//! regions moves the states it held one step down.
//!
//! `[H]` and `[H*]` are the history of the open state, `Name[H]` and
//! `Name[H*]` that of the state `Name`: one mark per state and kind, made
//! inside it at its first mention. `A.X` is the state `X` inside `A`.

use std::collections::{HashMap, HashSet};

use super::{
    Container, Crossing, Dash, Diagram, Direction, Kind, Node, Note, Side, Split, Text, Transition,
};
use crate::Declined;
use crate::common::{Frame, block, keyword, snippet, squeezed_eq, text_at};
use crate::graph::across::meeting;
use crate::source::Line;

/// Whether the lines of an `@startuml` diagram are a state diagram.
/// Notes are skipped, so a note that mentions `[*]` does not decide it.
/// An activity diagram is detected first and wins when both match.
pub fn is_state(lines: &[Line]) -> bool {
    lines
        .iter()
        .any(|line| keyword(line.text, "note").is_none() && marks_state(line.text))
}

fn marks_state(text: &str) -> bool {
    if text.contains("[*]") || keyword(text, "state").is_some() {
        return true;
    }
    if squeezed_eq(text, "hideemptydescription") {
        return true;
    }
    let lower = text.to_ascii_lowercase();
    [
        "<<choice>>",
        "<<fork>>",
        "<<join>>",
        "<<history>>",
        "<<history*>>",
        "<<deephistory>>",
    ]
    .iter()
    .any(|mark| lower.contains(mark))
}

/// Parses a state diagram from the lines [`crate::source::body`] gives.
pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut parser = Parser {
        frame: Frame::styled(super::skin::STYLE),
        ..Parser::default()
    };
    let mut at = 0;
    while at < lines.len() {
        at = match parser.frame.line(lines, at)? {
            Some(next) => next,
            None => parser.line(lines, at)?,
        };
    }
    parser.finish()
}

const MAX_DEPTH: usize = 100;

#[derive(Default)]
struct Parser {
    frame: Frame,
    root: Container,
    /// Open states: the path to the node being filled (a region once its
    /// state is split), and the line of the `{`.
    open: Vec<(Vec<usize>, usize)>,
    /// Every transition with the paths of its ends. They go into their
    /// containers at the end, after every split has moved its states.
    arrows: Vec<(Vec<usize>, Vec<usize>, Transition)>,
    /// Each named state's path, so a name is found without walking the
    /// whole diagram.
    names: HashMap<String, Vec<usize>>,
    /// States opened with `{` so far. A history mark may make a state a
    /// composite before its body is written.
    opened: HashSet<String>,
    /// States only a transition named so far. PlantUML moves one into the
    /// state where it is declared or described next.
    implicit: HashSet<String>,
}

struct Decl {
    key: String,
    label: String,
    /// A quoted name or an `as` alias, so it replaces a name already shown.
    labeled: bool,
    kind: Option<Kind>,
    stereotype: Option<String>,
    fill: Option<String>,
    line_color: Option<String>,
    border_dash: Dash,
    border_bold: bool,
    description: Option<String>,
    open: bool,
    line: usize,
}

struct RawArrow {
    from: String,
    to: String,
    label: Option<String>,
    color: Option<String>,
    dash: Dash,
    bold: bool,
}

impl Parser {
    fn finish(mut self) -> Result<Diagram, Declined> {
        if let Some((_, line)) = self.open.last() {
            return Err(Declined::at(*line, "a state has no `}`"));
        }
        empty_region(&self.root)?;
        for (from, to, mut arrow) in std::mem::take(&mut self.arrows) {
            let (home, rest) = from.split_at(from.len() - 1);
            if to.len() == from.len() && to.starts_with(home) {
                arrow.from = rest[0];
                arrow.to = to[to.len() - 1];
                self.container_mut(home).edges.push(arrow);
                continue;
            }
            // `arrow` checked that neither end holds the other.
            let (depth, a, b) = meeting(&from, &to).expect("ends apart");
            arrow.from = a;
            arrow.to = b;
            self.container_mut(&from[..depth]).crossings.push(Crossing {
                from: from[depth..].to_vec(),
                to: to[depth..].to_vec(),
                arrow,
            });
        }
        Ok(Diagram {
            root: self.root,
            frame: self.frame,
        })
    }

    /// The path of the node whose children are being filled; empty for
    /// the diagram itself.
    fn here(&self) -> Vec<usize> {
        self.open
            .last()
            .map(|(path, _)| path.clone())
            .unwrap_or_default()
    }

    /// The children of the node at `path`, or the diagram for an empty
    /// path.
    fn container(&self, path: &[usize]) -> &Container {
        let mut container = &self.root;
        for &index in path {
            container = &container.nodes[index].children;
        }
        container
    }

    fn container_mut(&mut self, path: &[usize]) -> &mut Container {
        let mut container = &mut self.root;
        for &index in path {
            container = &mut container.nodes[index].children;
        }
        container
    }

    fn current_mut(&mut self) -> &mut Container {
        let here = self.here();
        self.container_mut(&here)
    }

    fn node_mut(&mut self, path: &[usize]) -> &mut Node {
        let (last, home) = path.split_last().expect("a node has a path");
        &mut self.container_mut(home).nodes[*last]
    }

    fn line(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line { no, text, .. } = lines[at];
        if squeezed_eq(text, "lefttorightdirection") {
            self.current_mut().direction = Direction::Right;
            return Ok(at + 1);
        }
        if squeezed_eq(text, "toptobottomdirection") {
            self.current_mut().direction = Direction::Down;
            return Ok(at + 1);
        }
        if squeezed_eq(text, "hideemptydescription") {
            return Ok(at + 1);
        }
        if text == "}" {
            self.close(no)?;
            return Ok(at + 1);
        }
        if keyword(text, "state").is_some() {
            let decl = declaration(text, no)?;
            self.declare(decl)?;
            return Ok(at + 1);
        }
        if keyword(text, "note").is_some() {
            return self.note(lines, at);
        }
        if is_separator(text) {
            self.split(text, no)?;
            return Ok(at + 1);
        }
        if let Some(arrow) = transition(text, no) {
            self.arrow(arrow?, no)?;
            return Ok(at + 1);
        }
        if let Some(word) = refused(text) {
            return Err(Declined::at(no, format!("`{word}` is not supported yet")));
        }
        if let Some((name, body)) = description(text, no) {
            self.describe(&name?, &body, no)?;
            return Ok(at + 1);
        }
        Err(Declined::at(
            no,
            format!("`{}` is not supported yet", snippet(text)),
        ))
    }

    fn close(&mut self, line: usize) -> Result<(), Declined> {
        if self.open.pop().is_none() {
            return Err(Declined::at(line, "a `}` has no state to close"));
        }
        Ok(())
    }

    fn declare(&mut self, mut decl: Decl) -> Result<(), Declined> {
        if decl.key == "[*]" || decl.key.is_empty() {
            return Err(Declined::at(
                decl.line,
                "`[*]` is the start or the end, not a state name",
            ));
        }
        // A state written again, here or inside another, is the same state.
        let path = match self.find(&decl.key) {
            Some(path) => self.settle(&decl.key, path, decl.line)?,
            None if !decl.labeled && decl.key.contains('.') => {
                let path = self.dotted(&decl.key, decl.line)?;
                let last = decl.key.rsplit('.').next().unwrap_or_default().to_owned();
                decl.label.clone_from(&last);
                decl.key = last;
                path
            }
            None => self.push(plain_node(&decl.key, &decl.label, decl.line)),
        };
        apply(self.node_mut(&path), &decl)?;
        if !decl.open {
            return Ok(());
        }
        if self.node_mut(&path).kind != Kind::State {
            return Err(Declined::at(
                decl.line,
                "only a state can contain other states",
            ));
        }
        if self.opened.contains(&decl.key) {
            return Err(Declined::at(
                decl.line,
                format!("`{}` is already defined", decl.key),
            ));
        }
        if self.open.len() >= MAX_DEPTH || path.len() >= MAX_DEPTH {
            return Err(Declined::at(decl.line, "states nest more than 100 deep"));
        }
        self.node_mut(&path).composite = true;
        self.opened.insert(decl.key.clone());
        self.open.push((path, decl.line));
        Ok(())
    }

    /// `--` or `||` inside a state: what follows is the next concurrent
    /// region. The first one splits the state; what it held so far becomes
    /// its first region.
    fn split(&mut self, text: &str, line: usize) -> Result<(), Declined> {
        let split = if text.trim().chars().all(|c| c == '-') {
            Split::Stacked
        } else if text.trim().chars().all(|c| c == '|') {
            Split::Beside
        } else {
            return Err(Declined::at(line, "a separator mixes `-` and `|`"));
        };
        let Some((path, _)) = self.open.last().cloned() else {
            return Err(Declined::at(
                line,
                "concurrent regions outside a state are not supported yet",
            ));
        };
        let in_region = path
            .split_last()
            .is_some_and(|(_, home)| self.container(home).split.is_some());
        let state = if in_region {
            path[..path.len() - 1].to_vec()
        } else {
            path
        };
        let container = self.container_mut(&state);
        match container.split {
            Some(already) if already != split => {
                return Err(Declined::at(line, "a state mixes `--` and `||` regions"));
            }
            Some(_) => {}
            None => {
                let held = std::mem::take(container);
                container.direction = held.direction;
                container.split = Some(split);
                let first = region_node(held, line);
                container.nodes.push(first);
                // Every state held so far moved one step down.
                let ends = self.arrows.iter_mut().flat_map(|(from, to, _)| [from, to]);
                for end in ends.chain(self.names.values_mut()) {
                    if end.len() > state.len() && end.starts_with(&state) {
                        end.insert(state.len(), 0);
                    }
                }
            }
        }
        let container = self.container_mut(&state);
        let next = Container {
            direction: container.direction,
            ..Container::default()
        };
        container.nodes.push(region_node(next, line));
        let mut region = state;
        region.push(self.container(&region).nodes.len() - 1);
        if let Some(top) = self.open.last_mut() {
            top.0 = region;
        }
        Ok(())
    }

    fn arrow(&mut self, raw: RawArrow, line: usize) -> Result<(), Declined> {
        let from = self.endpoint(&raw.from, true, line)?;
        let to = self.endpoint(&raw.to, false, line)?;
        let apart = from.len() != to.len() || from[..from.len() - 1] != to[..to.len() - 1];
        if apart && meeting(&from, &to).is_none() {
            return Err(Declined::at(
                line,
                "a transition between a state and a state inside it is not supported yet",
            ));
        }
        // PlantUML keeps a second or later concurrent region to itself.
        let region = self.late_region(&self.here());
        for (end, name) in [(&from, &raw.from), (&to, &raw.to)] {
            if self.late_region(end) != region {
                return Err(Declined::at(
                    line,
                    format!(
                        "PlantUML takes no transition into or out of a second concurrent region (`{name}`)"
                    ),
                ));
            }
        }
        let label = raw.label.map(|value| text_at(&value, line));
        self.arrows.push((
            from,
            to,
            Transition {
                from: 0,
                to: 0,
                label,
                color: raw.color,
                dash: raw.dash,
                bold: raw.bold,
                line,
            },
        ));
        Ok(())
    }

    /// The innermost second or later concurrent region `path` lies in, or
    /// nothing.
    fn late_region(&self, path: &[usize]) -> Vec<usize> {
        let mut found = Vec::new();
        for depth in 1..=path.len() {
            if path[depth - 1] > 0 && self.container(&path[..depth - 1]).split.is_some() {
                found = path[..depth].to_vec();
            }
        }
        found
    }

    fn note(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let no = lines[at].no;
        let head = note_head(lines[at].text, no)?;
        let (text, next) = if head.one_line {
            let body = head.label.unwrap_or_default();
            if body.is_empty() {
                return Err(Declined::at(no, "a note needs text"));
            }
            (text_at(&body, no), at + 1)
        } else {
            let (body, next) = block(lines, at, "note", |line| squeezed_eq(line, "endnote"))?;
            if body.trim().is_empty() {
                return Err(Declined::at(no, "a note needs text"));
            }
            let line = lines.get(at + 1).map_or(no, |line| line.no);
            (Text { value: body, line }, next)
        };
        if head.target == "[*]" {
            return Err(Declined::at(no, "a note on `[*]` is not supported yet"));
        }
        let path = self.resolve(&head.target, no);
        let (target, home) = path.split_last().expect("a node has a path");
        let target = *target;
        let home = home.to_vec();
        self.container_mut(&home).notes.push(Note {
            target,
            side: head.side,
            text,
            line: no,
        });
        Ok(next)
    }

    fn describe(&mut self, name: &str, body: &str, line: usize) -> Result<(), Declined> {
        if name == "[*]" {
            return Err(Declined::at(
                line,
                "a description on `[*]` is not supported yet",
            ));
        }
        if let Some(path) = self.find(name) {
            self.settle(name, path, line)?;
        }
        let path = self.resolve(name, line);
        self.node_mut(&path).description.push(text_at(body, line));
        Ok(())
    }

    /// Where the state `key`, found at `path`, is once it is declared or
    /// described in the open state. Like PlantUML, a state only a
    /// transition made so far moves here from the state it was made in.
    fn settle(&mut self, key: &str, path: Vec<usize>, line: usize) -> Result<Vec<usize>, Declined> {
        let here = self.here();
        if !self.implicit.remove(key) || path[..path.len() - 1] == here[..] {
            return Ok(path);
        }
        if !self.late_region(&path).is_empty() || !self.late_region(&here).is_empty() {
            return Err(Declined::at(
                line,
                format!(
                    "PlantUML moves `{key}` here, across a concurrent region; that is not supported yet"
                ),
            ));
        }
        let (&last, parent) = path.split_last().expect("a state has a path");
        let node = self.container_mut(parent).nodes.remove(last);
        // Paths past it in its state move up one; those to it or inside it
        // get its new place below.
        let fix = |p: &mut Vec<usize>| {
            let inside = p.starts_with(&path);
            if !inside && p.len() > parent.len() && p.starts_with(parent) && p[parent.len()] > last
            {
                p[parent.len()] -= 1;
            }
            inside
        };
        let mut names = Vec::new();
        for (name, p) in &mut self.names {
            if fix(p) {
                names.push(name.clone());
            }
        }
        let mut ends = Vec::new();
        for (index, (from, to, _)) in self.arrows.iter_mut().enumerate() {
            if fix(from) {
                ends.push((index, true));
            }
            if fix(to) {
                ends.push((index, false));
            }
        }
        for (p, _) in &mut self.open {
            fix(p);
        }
        let here = self.here();
        let new = self.push_at(here, node);
        let moved = |p: &mut Vec<usize>| {
            let rest = p.split_off(path.len());
            *p = new.iter().copied().chain(rest).collect();
        };
        for name in names {
            if let Some(p) = self.names.get_mut(&name) {
                moved(p);
            }
        }
        for (index, from) in ends {
            let (a, b, _) = &mut self.arrows[index];
            moved(if from { a } else { b });
        }
        Ok(new)
    }

    /// The path to the state named `key`, wherever it is.
    fn find(&self, key: &str) -> Option<Vec<usize>> {
        self.names.get(key).cloned()
    }

    /// The state named `name`, made in the open state if it is new.
    fn resolve(&mut self, name: &str, line: usize) -> Vec<usize> {
        match self.find(name) {
            Some(path) => path,
            None => self.push(plain_node(name, name, line)),
        }
    }

    /// The mark of `kind` inside the node at `path` (the diagram for an
    /// empty path), made there at its first mention.
    fn marker(&mut self, mut path: Vec<usize>, kind: Kind, line: usize) -> Vec<usize> {
        let nodes = &mut self.container_mut(&path).nodes;
        match nodes.iter().position(|node| node.kind == kind) {
            Some(index) => path.push(index),
            None => {
                path.push(nodes.len());
                nodes.push(marker_node(kind, line));
            }
        }
        path
    }

    fn endpoint(&mut self, name: &str, source: bool, line: usize) -> Result<Vec<usize>, Declined> {
        if name == "[*]" {
            let kind = if source { Kind::Start } else { Kind::End };
            return Ok(self.marker(self.here(), kind, line));
        }
        if let Some((owner, kind)) = history(name) {
            return self.history(owner, kind, line);
        }
        if self.find(name).is_none() && name.contains('.') {
            return self.dotted(name, line);
        }
        if self.find(name).is_none() {
            self.implicit.insert(name.to_owned());
        }
        Ok(self.resolve(name, line))
    }

    /// `A.X`: the state `X` inside `A`, each made where it is missing (`A`
    /// in the open state, `X` inside `A`).
    fn dotted(&mut self, name: &str, line: usize) -> Result<Vec<usize>, Declined> {
        if name.split('.').any(str::is_empty) {
            return Err(Declined::at(line, format!("`{name}` has an empty part")));
        }
        let mut parts = name.split('.');
        let mut path = self.resolve(parts.next().unwrap_or_default(), line);
        for part in parts {
            let node = self.node_mut(&path);
            if node.kind != Kind::State {
                let key = node.key.clone();
                return Err(Declined::at(
                    line,
                    format!("`{key}` is not a state, so nothing is inside it"),
                ));
            }
            if node.children.split.is_some() {
                return Err(Declined::at(
                    line,
                    "a dotted name into concurrent regions is not supported yet",
                ));
            }
            node.composite = true;
            path = match self.find(part) {
                Some(found) if found.len() == path.len() + 1 && found.starts_with(&path) => found,
                Some(_) => {
                    return Err(Declined::at(
                        line,
                        format!("`{part}` is a state elsewhere already"),
                    ));
                }
                None => self.push_at(path, plain_node(part, part, line)),
            };
        }
        Ok(path)
    }

    /// The history mark of the state `owner`, or of the open state when
    /// `owner` is empty.
    fn history(&mut self, owner: &str, kind: Kind, line: usize) -> Result<Vec<usize>, Declined> {
        let state = if owner.is_empty() {
            let here = self.here();
            if here.is_empty() {
                return Err(Declined::at(
                    line,
                    "a history needs a state around it, or `Name[H]`",
                ));
            }
            here
        } else {
            if let Some(path) = self.find(owner)
                && self.here().starts_with(&path)
            {
                return Err(Declined::at(
                    line,
                    format!(
                        "inside `{owner}`, PlantUML makes `{owner}[H]` another `{owner}`; write `[H]`"
                    ),
                ));
            }
            let path = self.resolve(owner, line);
            let node = self.node_mut(&path);
            if node.kind != Kind::State {
                return Err(Declined::at(
                    line,
                    format!("`{owner}` is not a state, so it has no history"),
                ));
            }
            if node.children.split.is_some() {
                return Err(Declined::at(
                    line,
                    "the history of a state with concurrent regions is not supported yet",
                ));
            }
            node.composite = true;
            path
        };
        Ok(self.marker(state, kind, line))
    }

    /// Adds `node` to the open state; its path.
    fn push(&mut self, node: Node) -> Vec<usize> {
        let here = self.here();
        self.push_at(here, node)
    }

    /// Adds `node` inside the node at `path` (the diagram for an empty
    /// path); its path.
    fn push_at(&mut self, mut path: Vec<usize>, node: Node) -> Vec<usize> {
        let key = node.key.clone();
        let nodes = &mut self.container_mut(&path).nodes;
        path.push(nodes.len());
        nodes.push(node);
        if !key.is_empty() {
            self.names.entry(key).or_insert_with(|| path.clone());
        }
        path
    }
}

/// A region with no state in it declines, at its separator.
fn empty_region(container: &Container) -> Result<(), Declined> {
    for node in &container.nodes {
        if node.kind == Kind::Region && node.children.nodes.is_empty() {
            return Err(Declined::at(node.line, "a concurrent region needs a state"));
        }
        empty_region(&node.children)?;
    }
    Ok(())
}

/// `[H]`, `[H*]`, `Name[H]` or `Name[H*]`: whose history (empty for the
/// open state) and which one.
fn history(name: &str) -> Option<(&str, Kind)> {
    let (owner, kind) = match name.strip_suffix("[H*]") {
        Some(owner) => (owner, Kind::DeepHistory),
        None => (name.strip_suffix("[H]")?, Kind::History),
    };
    (!owner.contains(['[', ']'])).then_some((owner, kind))
}

fn region_node(children: Container, line: usize) -> Node {
    Node {
        kind: Kind::Region,
        composite: true,
        children,
        ..marker_node(Kind::Region, line)
    }
}

fn plain_node(key: &str, label: &str, line: usize) -> Node {
    Node {
        key: key.to_owned(),
        label: text_at(label, line),
        description: Vec::new(),
        kind: Kind::State,
        fill: None,
        line_color: None,
        border_dash: Dash::Solid,
        border_bold: false,
        stereotype: None,
        composite: false,
        children: Container::default(),
        line,
    }
}

fn marker_node(kind: Kind, line: usize) -> Node {
    Node {
        key: String::new(),
        label: text_at("", line),
        description: Vec::new(),
        kind,
        fill: None,
        line_color: None,
        border_dash: Dash::Solid,
        border_bold: false,
        stereotype: None,
        composite: false,
        children: Container::default(),
        line,
    }
}

fn apply(node: &mut Node, decl: &Decl) -> Result<(), Declined> {
    if let Some(kind) = decl.kind {
        if node.composite || matches!(node.kind, Kind::Start | Kind::End) {
            return Err(Declined::at(
                decl.line,
                "a state that contains states cannot change its kind",
            ));
        }
        if node.kind != Kind::State && node.kind != kind {
            return Err(Declined::at(
                decl.line,
                format!("`{}` is already another kind of state", node.key),
            ));
        }
        node.kind = kind;
    }
    if let Some(stereotype) = &decl.stereotype {
        node.stereotype = Some(stereotype.clone());
    }
    if decl.labeled {
        node.label = text_at(&decl.label, decl.line);
    }
    if let Some(fill) = &decl.fill {
        node.fill = Some(fill.clone());
    }
    if let Some(color) = &decl.line_color {
        node.line_color = Some(color.clone());
        node.border_dash = decl.border_dash;
        node.border_bold = decl.border_bold;
    }
    if let Some(text) = &decl.description {
        node.description.push(text_at(text, decl.line));
    }
    Ok(())
}

fn declaration(text: &str, line: usize) -> Result<Decl, Declined> {
    let Some(rest) = keyword(text, "state") else {
        return Err(Declined::at(line, "`state` needs a name"));
    };
    let rest = rest.trim();
    let (rest, open) = match rest.rfind('{') {
        Some(at) => {
            if !rest[at + 1..].trim().is_empty() || rest[..at].contains('{') {
                return Err(Declined::at(
                    line,
                    "a state body has to start on the next line",
                ));
            }
            (rest[..at].trim(), true)
        }
        None => (rest, false),
    };
    let (spec, description) = match unquoted_colon(rest) {
        Some(at) => {
            let body = rest[at + 1..].trim();
            if body.is_empty() {
                return Err(Declined::at(line, "a description needs text"));
            }
            (rest[..at].trim(), Some(body.to_owned()))
        }
        None => (rest, None),
    };
    let mut decl = Decl {
        key: String::new(),
        label: String::new(),
        labeled: false,
        kind: None,
        stereotype: None,
        fill: None,
        line_color: None,
        border_dash: Dash::Solid,
        border_bold: false,
        description,
        open,
        line,
    };
    let mut label: Option<(String, bool)> = None;
    let mut alias: Option<(String, bool)> = None;
    let mut expect_alias = false;
    for token in tokenize(spec, line)? {
        if expect_alias {
            alias = Some((token.text, token.quoted));
            expect_alias = false;
            continue;
        }
        match token.kind {
            Tok::Name => {
                if label.is_some() {
                    return Err(Declined::at(
                        line,
                        "this state declaration is not supported yet",
                    ));
                }
                label = Some((token.text, token.quoted));
            }
            Tok::As if decl.kind.is_some() || decl.stereotype.is_some() => {
                return Err(Declined::at(
                    line,
                    "PlantUML takes the stereotype after the alias: `state \"Long name\" as Name <<choice>>`",
                ));
            }
            Tok::As => expect_alias = true,
            Tok::Stereo => {
                if decl.kind.is_some() || decl.stereotype.is_some() {
                    return Err(Declined::at(line, "a state has more than one stereotype"));
                }
                match kind_of(&token.text, line)? {
                    Some(kind) => decl.kind = Some(kind),
                    None => decl.stereotype = Some(token.text),
                }
            }
            Tok::Color => take_color(&token.text, line, &mut decl)?,
        }
    }
    if expect_alias {
        return Err(Declined::at(line, "`as` needs a name"));
    }
    let Some((text, quoted)) = label else {
        return Err(Declined::at(line, "a state needs a name"));
    };
    decl.labeled = quoted || alias.is_some();
    let key_quoted;
    (decl.key, key_quoted, decl.label) = match alias {
        // `state code as "Long name"`: what is quoted is shown.
        Some((alias, true)) if !quoted => (text, false, alias),
        Some((alias, alias_quoted)) => (alias, alias_quoted, text),
        None => (text.clone(), quoted, text),
    };
    if decl.key.is_empty() {
        return Err(Declined::at(line, "a state needs a name"));
    }
    if !key_quoted && !plain_name(&decl.key) {
        return Err(not_a_name(&decl.key, line));
    }
    Ok(decl)
}

struct Token {
    kind: Tok,
    text: String,
    quoted: bool,
}

enum Tok {
    Name,
    As,
    Stereo,
    Color,
}

fn tokenize(spec: &str, line: usize) -> Result<Vec<Token>, Declined> {
    let bytes = spec.as_bytes();
    let mut out = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        if bytes[index] == b'<' && bytes.get(index + 1) == Some(&b'<') {
            let start = index + 2;
            let Some(end) = spec[start..].find(">>") else {
                return Err(Declined::at(line, "a stereotype has no `>>`"));
            };
            out.push(Token {
                kind: Tok::Stereo,
                text: spec[start..start + end].trim().to_owned(),
                quoted: false,
            });
            index = start + end + 2;
            continue;
        }
        if bytes[index] == b'"' {
            let start = index + 1;
            let Some(end) = spec[start..].find('"') else {
                return Err(Declined::at(line, "a name has no closing `\"`"));
            };
            out.push(Token {
                kind: Tok::Name,
                text: spec[start..start + end].to_owned(),
                quoted: true,
            });
            index = start + end + 1;
            continue;
        }
        if bytes[index] == b'#' {
            let start = index;
            while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            out.push(Token {
                kind: Tok::Color,
                text: spec[start..index].to_owned(),
                quoted: false,
            });
            continue;
        }
        let start = index;
        while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let word = spec[start..index].to_owned();
        let kind = if word.eq_ignore_ascii_case("as") {
            Tok::As
        } else {
            Tok::Name
        };
        out.push(Token {
            kind,
            text: word,
            quoted: false,
        });
    }
    Ok(out)
}

/// The kind a stereotype makes a state; `None` for one of the writer's own,
/// which only picks colors.
fn kind_of(stereo: &str, line: usize) -> Result<Option<Kind>, Declined> {
    Ok(Some(match stereo.to_ascii_lowercase().as_str() {
        "choice" => Kind::Choice,
        "fork" => Kind::Fork,
        "join" => Kind::Join,
        "history" => Kind::History,
        "history*" | "deephistory" => Kind::DeepHistory,
        // PlantUML draws these as shapes of their own.
        "start" | "end" | "entrypoint" | "exitpoint" | "inputpin" | "outputpin"
        | "expansioninput" | "expansionoutput" | "sdlreceive" => {
            return Err(Declined::at(
                line,
                format!("the stereotype `<<{stereo}>>` is not supported yet"),
            ));
        }
        "" => return Err(Declined::at(line, "a stereotype needs a text")),
        _ => return Ok(None),
    }))
}

fn take_color(raw: &str, line: usize, decl: &mut Decl) -> Result<(), Declined> {
    if raw.contains(';') {
        return Err(Declined::at(line, "this state color is not supported yet"));
    }
    if let Some(rest) = raw.strip_prefix("##") {
        let (style, color) = if let Some(rest) = rest.strip_prefix('[') {
            let Some((style, color)) = rest.split_once(']') else {
                return Err(Declined::at(line, "a border color has no `]`"));
            };
            (style, color)
        } else {
            ("", rest)
        };
        if color.is_empty() {
            return Err(Declined::at(line, "a border color is missing"));
        }
        match style.to_ascii_lowercase().as_str() {
            "" | "solid" => {}
            "dashed" => decl.border_dash = Dash::Dashed,
            "dotted" => decl.border_dash = Dash::Dotted,
            "bold" => decl.border_bold = true,
            _ => {
                return Err(Declined::at(
                    line,
                    format!("the border style `{style}` is not supported yet"),
                ));
            }
        }
        decl.line_color = Some(color.to_owned());
    } else {
        let color = raw.strip_prefix('#').unwrap_or(raw);
        if color.is_empty() {
            return Err(Declined::at(line, "a color is missing"));
        }
        decl.fill = Some(raw.to_owned());
    }
    Ok(())
}

fn transition(text: &str, line: usize) -> Option<Result<RawArrow, Declined>> {
    if let Some(parsed) = bracket_arrow(text, line) {
        return Some(parsed);
    }
    let (start, end) = arrow_run(text)?;
    let run = &text[start..end];
    let left = text[..start].trim();
    let right = text[end..].trim();
    // A label comes after the arrow: `A : text --> B` describes `A`.
    if label_colon(left).is_some() {
        return None;
    }
    if head_marker(text, start, end) {
        return Some(Err(Declined::at(line, "this arrow is not supported yet")));
    }
    let forward = run.contains('>');
    let backward = run.contains('<');
    if forward && backward {
        return Some(Err(Declined::at(
            line,
            "bidirectional arrows are not supported yet",
        )));
    }
    if !forward && !backward {
        return None;
    }
    // `A -left-> B`, `A <-up- B`.
    let word = match forward {
        true => left.rsplit_once('-').map(|(_, word)| word),
        false => right.split_once('-').map(|(word, _)| word),
    };
    if word.is_some_and(|word| {
        let word = word.to_ascii_lowercase();
        !word.is_empty()
            && ["left", "right", "up", "down"]
                .iter()
                .any(|d| d.starts_with(&word))
    }) {
        return Some(Err(Declined::at(
            line,
            "arrows with a direction are not supported yet",
        )));
    }
    let dashes = |run: &str| !run.is_empty() && run.chars().all(|c| c == '-');
    let shaped = match forward {
        true => run.strip_suffix('>').is_some_and(dashes),
        false => run.strip_prefix('<').is_some_and(dashes),
    };
    if !shaped {
        return Some(Err(Declined::at(
            line,
            format!("`{run}` is no arrow for PlantUML, which takes `->`, `-->` or `<--`"),
        )));
    }
    Some(raw_arrow(
        left,
        right,
        backward,
        None,
        Dash::Solid,
        false,
        line,
    ))
}

fn bracket_arrow(text: &str, line: usize) -> Option<Result<RawArrow, Declined>> {
    if let Some(at) = find_unquoted(text, "<-[") {
        let after = at + 3;
        if text[after..].contains("]->") {
            return Some(Err(Declined::at(
                line,
                "bidirectional arrows are not supported yet",
            )));
        }
        let end = text[after..].find("]-")?;
        let inside = &text[after..after + end];
        let left = text[..at].trim();
        let right = text[after + end + 2..].trim();
        if label_colon(left).is_some() {
            return None;
        }
        if head_marker(text, at, after + end + 2) {
            return Some(Err(Declined::at(line, "this arrow is not supported yet")));
        }
        return Some(styled(left, right, true, inside, line));
    }
    if let Some(at) = find_unquoted(text, "-[") {
        let after = at + 2;
        let Some(end) = text[after..].find("]->") else {
            return Some(Err(Declined::at(line, "an arrow has no end")));
        };
        let inside = &text[after..after + end];
        let left = text[..at].trim();
        let right = text[after + end + 3..].trim();
        if label_colon(left).is_some() {
            return None;
        }
        if head_marker(text, at, after + end + 3) {
            return Some(Err(Declined::at(line, "this arrow is not supported yet")));
        }
        return Some(styled(left, right, false, inside, line));
    }
    None
}

fn styled(
    left: &str,
    right: &str,
    backward: bool,
    inside: &str,
    line: usize,
) -> Result<RawArrow, Declined> {
    let (color, dash, bold) = arrow_style(inside, line)?;
    raw_arrow(left, right, backward, color, dash, bold, line)
}

fn arrow_style(inside: &str, line: usize) -> Result<(Option<String>, Dash, bool), Declined> {
    let mut color = None;
    let mut dash = Dash::Solid;
    let mut bold = false;
    for part in inside.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "dashed" => dash = Dash::Dashed,
            "dotted" => dash = Dash::Dotted,
            "bold" => bold = true,
            "solid" => {}
            "hidden" => {
                return Err(Declined::at(line, "hidden arrows are not supported yet"));
            }
            _ => {
                if color.is_some()
                    || part.contains('=')
                    || part.contains(';')
                    || !part.starts_with('#')
                {
                    return Err(Declined::at(
                        line,
                        format!("the arrow style `{part}` is not supported yet"),
                    ));
                }
                let bare = part.strip_prefix('#').unwrap_or(part);
                if bare.is_empty() {
                    return Err(Declined::at(line, "an arrow color is missing"));
                }
                color = Some(part.to_owned());
            }
        }
    }
    Ok((color, dash, bold))
}

/// The transition between the states written `left` and `right` of the
/// arrow; `backward` when it points left. The label comes after the
/// second state as written.
fn raw_arrow(
    left: &str,
    right: &str,
    backward: bool,
    color: Option<String>,
    dash: Dash,
    bold: bool,
    line: usize,
) -> Result<RawArrow, Declined> {
    let left = end_name(left, line)?;
    let (right, label) = second_end(right, line)?;
    if left.is_empty() || right.is_empty() {
        return Err(Declined::at(line, "a transition needs a state"));
    }
    let (from, to) = match backward {
        true => (right, left),
        false => (left, right),
    };
    Ok(RawArrow {
        from,
        to,
        label,
        color,
        dash,
        bold,
    })
}

/// The state after the arrow and the label: a colon with a space beside it
/// starts the label (`B:x` is a name), and a stereotype after the name
/// (`B <<fork>>`) changes nothing, as PlantUML draws nothing of it.
fn second_end(raw: &str, line: usize) -> Result<(String, Option<String>), Declined> {
    let raw = raw.trim();
    let (name, label) = match label_colon(raw) {
        Some(at) => {
            let label = transition_label(raw[at + 1..].trim());
            (
                raw[..at].trim(),
                (!label.is_empty()).then(|| label.to_owned()),
            )
        }
        None => (raw, None),
    };
    let name = match name.find("<<") {
        Some(at) if name.ends_with(">>") => name[..at].trim(),
        _ => name,
    };
    Ok((end_name(name, line)?, label))
}

/// A state at a transition's end: `[*]`, a history (`[H]`, `Name[H*]`) or
/// a plain name; PlantUML takes no quotes there.
fn end_name(raw: &str, line: usize) -> Result<String, Declined> {
    let raw = raw.trim();
    if raw.starts_with('"') {
        return Err(Declined::at(
            line,
            "PlantUML takes no quoted name in a transition; declare it: `state \"Long name\" as Name`",
        ));
    }
    let owner = match history(raw) {
        Some((owner, _)) => owner,
        None if raw == "[*]" => "",
        None => raw,
    };
    if !owner.is_empty() && !plain_name(owner) {
        return Err(not_a_name(raw, line));
    }
    Ok(raw.to_owned())
}

/// A state named in a description: quoted, or plain.
fn name_of(raw: &str, line: usize) -> Result<String, Declined> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(String::new());
    }
    if let Some(inner) = raw.strip_prefix('"') {
        let Some(inner) = inner.strip_suffix('"') else {
            return Err(Declined::at(line, "a name has no closing `\"`"));
        };
        if inner.is_empty() || inner.contains('"') {
            return Err(Declined::at(line, "this name is not supported yet"));
        }
        return Ok(inner.to_owned());
    }
    if raw != "[*]" && !plain_name(raw) {
        return Err(not_a_name(raw, line));
    }
    Ok(raw.to_owned())
}

/// Whether PlantUML takes `name` unquoted: letters, digits, `_`, `.` (a
/// state inside another) and `:`.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | ':'))
}

fn not_a_name(raw: &str, line: usize) -> Declined {
    Declined::at(
        line,
        format!(
            "`{}` is no state name for PlantUML, which takes letters, digits, `_`, `.` and `:`",
            snippet(raw)
        ),
    )
}

fn description(text: &str, line: usize) -> Option<(Result<String, Declined>, String)> {
    let at = unquoted_colon(text)?;
    let body = text[at + 1..].trim();
    if body.is_empty() {
        return None;
    }
    Some((name_of(text[..at].trim(), line), body.to_owned()))
}

struct NoteHead {
    side: Side,
    target: String,
    label: Option<String>,
    one_line: bool,
}

fn note_head(text: &str, line: usize) -> Result<NoteHead, Declined> {
    let Some(rest) = keyword(text, "note") else {
        return Err(Declined::at(line, "`note` is not supported yet"));
    };
    let mut words = rest.split_whitespace();
    let side_word = words.next().unwrap_or("");
    let side = match side_word.to_ascii_lowercase().as_str() {
        "left" => Side::Left,
        "right" => Side::Right,
        "top" => Side::Top,
        "bottom" => Side::Bottom,
        "over" => {
            return Err(Declined::at(
                line,
                "PlantUML takes `note left of`, `right of`, `top of` or `bottom of` in a state diagram, not `note over`",
            ));
        }
        _ => {
            return Err(Declined::at(
                line,
                format!("`note {}` is not supported yet", snippet(rest)),
            ));
        }
    };
    let Some(rest) = keyword(rest, side_word) else {
        return Err(Declined::at(line, "`note` is not supported yet"));
    };
    let Some(rest) = keyword(rest, "of") else {
        return Err(Declined::at(
            line,
            format!("`note {}` is not supported yet", snippet(rest)),
        ));
    };
    let colon = unquoted_colon(rest);
    let one_line = colon.is_some();
    let (target, label) = match colon {
        Some(at) => {
            let label = rest[at + 1..].trim();
            (&rest[..at], (!label.is_empty()).then(|| label.to_owned()))
        }
        None => (rest, None),
    };
    let target = name_of(target, line)?;
    if target.is_empty() {
        return Err(Declined::at(line, "a note needs a state"));
    }
    Ok(NoteHead {
        side,
        target,
        label,
        one_line,
    })
}

fn refused(text: &str) -> Option<&str> {
    let word = text.split_whitespace().next()?;
    let lower = word.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "skin"
            | "sprite"
            | "newpage"
            | "allowmixing"
            | "hide"
            | "show"
            | "style"
            | "url"
            | "namespace"
            | "package"
            | "class"
            | "interface"
            | "enum"
            | "component"
            | "node"
            | "usecase"
            | "actor"
            | "rectangle"
            | "card"
            | "object"
    )
    .then_some(word)
}

fn is_separator(text: &str) -> bool {
    let text = text.trim();
    text.len() >= 2 && text.chars().all(|c| c == '-' || c == '|')
}

/// Whether the arrow at `start..end` of `text` carries a head or a tail
/// (`x->`, `o->`, `->o`): a mark written against the arrow, alone.
/// `X --> Y` is the state `X`.
fn head_marker(text: &str, start: usize, end: usize) -> bool {
    const MARKS: [char; 6] = ['x', 'o', 'X', 'O', '/', '\\'];
    let alone = |c: Option<char>| c.is_none_or(char::is_whitespace);
    let mut before = text[..start].chars().rev();
    let mut after = text[end..].chars();
    let on = |c: Option<char>| c.is_some_and(|c| MARKS.contains(&c));
    (on(before.next()) && alone(before.next())) || (on(after.next()) && alone(after.next()))
}

fn arrow_run(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut quote = false;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            quote = !quote;
            index += 1;
            continue;
        }
        if !quote && matches!(bytes[index], b'-' | b'<' | b'>') {
            let start = index;
            while index < bytes.len() && matches!(bytes[index], b'-' | b'<' | b'>') {
                index += 1;
            }
            let run = &text[start..index];
            if run.contains('-') && (run.contains('<') || run.contains('>')) {
                return Some((start, index));
            }
            continue;
        }
        index += 1;
    }
    None
}

fn find_unquoted(text: &str, needle: &str) -> Option<usize> {
    let mut quote = false;
    let mut index = 0;
    for ch in text.chars() {
        if ch == '"' {
            quote = !quote;
        } else if !quote && text[index..].starts_with(needle) {
            return Some(index);
        }
        index += ch.len_utf8();
    }
    None
}

/// A transition label as PlantUML keeps it. One leading `-` that is not
/// the start of `--` is dropped, and the spaces after it: `-go` and
/// `- left->done` are `go` and `left->done`. `--go` and a lone `-` stay.
fn transition_label(label: &str) -> &str {
    match label.strip_prefix('-') {
        Some(rest) if !rest.starts_with('-') && !rest.trim_start().is_empty() => rest.trim_start(),
        _ => label,
    }
}

/// Where a transition's label starts: the first colon with a space beside
/// it. One between two other characters is part of a name (`B:x`), and
/// so is one at the end (`B:`).
fn label_colon(text: &str) -> Option<usize> {
    let mut quote = false;
    for (index, ch) in text.char_indices() {
        if ch == '"' {
            quote = !quote;
        } else if !quote && ch == ':' {
            let before = text[..index].chars().next_back();
            let after = text[index + 1..].chars().next();
            if before.is_none_or(char::is_whitespace) || after.is_some_and(char::is_whitespace) {
                return Some(index);
            }
        }
    }
    None
}

fn unquoted_colon(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut quote = false;
    for (index, &byte) in bytes.iter().enumerate() {
        if byte == b'"' {
            quote = !quote;
            continue;
        }
        if !quote && byte == b':' {
            return Some(index);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagram(source: &str) -> Diagram {
        let (_, lines) = crate::source::body(source).unwrap();
        parse(&lines).unwrap_or_else(|err| panic!("{err:?}"))
    }

    fn declined(source: &str) -> Declined {
        let (_, lines) = crate::source::body(source).unwrap();
        parse(&lines).unwrap_err()
    }

    fn node<'a>(container: &'a Container, key: &str) -> &'a Node {
        container
            .nodes
            .iter()
            .find(|node| node.key == key)
            .unwrap_or_else(|| panic!("no {key}"))
    }

    /// Each edge of the diagram's top as `from -> to : label`, by keys.
    fn edges(diagram: &Diagram) -> Vec<String> {
        let root = &diagram.root;
        root.edges
            .iter()
            .map(|e| {
                let label = e.label.as_ref().map_or("", |l| l.value.as_str());
                format!(
                    "{} -> {} : {label}",
                    root.nodes[e.from].key, root.nodes[e.to].key
                )
            })
            .collect()
    }

    #[test]
    fn names_and_labels_as_plantuml_reads_them() {
        // A colon with a space beside it starts the label; one inside a name
        // or at its end is part of it.
        let d = diagram(
            "@startuml\nA --> B:x\nA --> C: go\nA --> D :go\nA --> E:x : y\nA --> F:\n@enduml\n",
        );
        assert_eq!(
            edges(&d),
            [
                "A -> B:x : ",
                "A -> C : go",
                "A -> D : go",
                "A -> E:x : y",
                "A -> F: : "
            ]
        );
        // A stereotype after the second state draws nothing and changes nothing.
        let d = diagram("@startuml\nA --> B<<fork>>\nA --> B <<x>> : y\n@enduml\n");
        assert_eq!(edges(&d), ["A -> B : ", "A -> B : y"]);
        assert_eq!(node(&d.root, "B").kind, Kind::State);
        // One leading `-` is not part of the label. `--` and a lone `-` are.
        let d = diagram(
            "@startuml\nA --> B : -go\nA --> C : - left->done\nA --> D : --go\nA --> E : -\n@enduml\n",
        );
        assert_eq!(
            edges(&d),
            [
                "A -> B : go",
                "A -> C : left->done",
                "A -> D : --go",
                "A -> E : -"
            ]
        );
        // The label comes after the arrow: before it, the line describes a state.
        let d = diagram("@startuml\nA --> B\nA : waits --> B\nB <-- A : back\n@enduml\n");
        assert_eq!(edges(&d), ["A -> B : ", "A -> B : back"]);
        assert_eq!(node(&d.root, "A").description[0].value, "waits --> B");
        // Letters of any script, digits, `_`, `.` and `:` only, and no quotes in a transition.
        diagram("@startuml\nÄrger_1 --> Ruhe2\n@enduml\n");
        for (source, line, part) in [
            ("A --> B$", 1, "`B$` is no state name"),
            ("A-B --> C", 1, "`A-B` is no state name"),
            ("A --> B[H", 1, "`B[H` is no state name"),
            ("state A$", 1, "`A$` is no state name"),
            ("state \"Long\" as L-1", 1, "`L-1` is no state name"),
            ("A --> \"B x\"", 1, "no quoted name"),
            ("A$ : text", 1, "`A$` is no state name"),
            ("state \"x\" <<join>> as P", 1, "stereotype after the alias"),
        ] {
            let err = declined(&format!("@startuml\n{source}\n@enduml\n"));
            assert_eq!(err.line, Some(line), "{source}: {err:?}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
        // Quoted names stay where PlantUML takes them.
        let d = diagram(
            "@startuml\nstate \"B x\" as B <<join>>\nstate \"C y\"\n\"C y\" : text\nA --> B\n@enduml\n",
        );
        assert_eq!(node(&d.root, "C y").description[0].value, "text");
    }

    #[test]
    fn a_stereotype_of_the_writers_own_keeps_a_state_a_state() {
        let d = diagram(
            "@startuml\nstate A <<sub>>\nA : does things\nstate \"Init\" as I <<Phase>> {\n  [*] --> X\n}\n[*] --> I\nI --> A\n@enduml\n",
        );
        let a = node(&d.root, "A");
        assert_eq!(
            (a.kind, a.stereotype.as_deref()),
            (Kind::State, Some("sub"))
        );
        assert_eq!(a.description[0].value, "does things");
        let init = node(&d.root, "I");
        assert!(init.composite);
        assert_eq!(init.stereotype.as_deref(), Some("Phase"));
        assert_eq!(node(&init.children, "X").stereotype, None);
        // PlantUML draws these as shapes of their own, not drawn here yet.
        for (source, part) in [
            ("state S <<start>>", "`<<start>>` is not supported yet"),
            (
                "state E <<entryPoint>>",
                "`<<entryPoint>>` is not supported yet",
            ),
            ("state A <<a>> <<b>>", "more than one stereotype"),
            ("state \"x\" <<sub>> as P", "stereotype after the alias"),
            ("[*] --> A\nnote over A : n", "not `note over`"),
        ] {
            let err = declined(&format!("@startuml\n{source}\n@enduml\n"));
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
    }

    #[test]
    fn arrows_and_regions_as_plantuml_takes_them() {
        diagram("@startuml\nA -> B\nB ---> C\nA <-- C\nC -[#red,dashed]-> A\n@enduml\n");
        for (source, part) in [
            ("A ->> B", "`->>` is no arrow"),
            ("A -->--> B", "`-->-->` is no arrow"),
            ("A <<- B", "`<<-` is no arrow"),
            ("A -left-> B", "with a direction"),
            ("A -Up-> B", "with a direction"),
            ("A <-l- B", "with a direction"),
            ("A -[red]-> B", "arrow style `red`"),
            ("A -[norank]-> B", "arrow style `norank`"),
        ] {
            let err = declined(&format!("@startuml\n{source}\n@enduml\n"));
            assert_eq!(err.line, Some(1), "{source}: {err:?}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
        // A second or later region keeps its states to itself; the first is
        // part of the state around it.
        let regions = |inside: &str, after: &str| {
            format!(
                "@startuml\n[*] --> S\nstate A {{\n  [*] --> X\n  X --> S\n  --\n  [*] --> Y\n  {inside}\n}}\n{after}\n@enduml\n"
            )
        };
        diagram(&regions("Y --> Z", "X --> S"));
        for (inside, after, line) in [
            ("Y --> S", "", 7),
            ("Y --> X", "", 7),
            ("[*] --> S", "", 7),
            ("Y --> Z", "Z --> S", 9),
        ] {
            let err = declined(&regions(inside, after));
            assert_eq!(err.line, Some(line), "{inside} / {after}: {err:?}");
            assert!(
                err.reason.contains("second concurrent region"),
                "{}",
                err.reason
            );
        }
    }

    #[test]
    fn a_state_names_its_own_history_without_its_name() {
        // Inside `On`, PlantUML makes `On[H]` the history of another `On` inside it.
        let err =
            declined("@startuml\nstate On {\n  [*] --> A\n  A --> On[H] : back\n}\n@enduml\n");
        assert_eq!(err.line, Some(3), "{err:?}");
        assert!(err.reason.contains("write `[H]`"), "{}", err.reason);
        diagram(
            "@startuml\nstate On {\n  [*] --> A\n  A --> [H] : back\n}\nB --> On[H*]\n@enduml\n",
        );
    }

    #[test]
    fn the_player_is_one_flat_container() {
        let diagram = diagram(include_str!("../../../../examples/player.puml"));
        assert_eq!(diagram.frame.title.unwrap().value, "A media player");
        assert_eq!(diagram.root.nodes.len(), 5);
        assert_eq!(diagram.root.edges.len(), 8);
        assert!(
            diagram
                .root
                .nodes
                .iter()
                .any(|node| node.kind == Kind::Start)
        );
        assert!(diagram.root.nodes.iter().any(|node| node.kind == Kind::End));
        assert!(
            diagram
                .root
                .edges
                .iter()
                .any(|edge| edge.from == edge.to
                    && edge.label.as_ref().unwrap().value == "next track")
        );
        let close = diagram
            .root
            .edges
            .iter()
            .find(|edge| edge.label.as_ref().is_some_and(|t| t.value == "close"))
            .unwrap();
        assert_eq!(diagram.root.nodes[close.to].kind, Kind::End);
    }

    #[test]
    fn a_transition_into_its_own_state_declines() {
        let err =
            declined("@startuml\nstate Outer {\n  state Inner\n}\nInner --> Outer\n@enduml\n");
        assert_eq!(err.line, Some(4));
        assert!(err.reason.contains("inside it"), "{}", err.reason);
    }

    #[test]
    fn a_name_inside_another_state_is_that_state() {
        let diagram =
            diagram("@startuml\nstate Outer {\n  state Inner\n}\nIdle --> Inner : go\n@enduml\n");
        let root = &diagram.root;
        assert_eq!(root.nodes.len(), 2, "no second Inner at the top");
        assert!(root.edges.is_empty());
        assert_eq!(root.crossings.len(), 1);
        let crossing = &root.crossings[0];
        let outer = root.nodes.iter().position(|n| n.key == "Outer").unwrap();
        let idle = root.nodes.iter().position(|n| n.key == "Idle").unwrap();
        assert_eq!(crossing.from, vec![idle]);
        assert_eq!(crossing.to, vec![outer, 0]);
        assert_eq!((crossing.arrow.from, crossing.arrow.to), (idle, outer));
        assert_eq!(crossing.arrow.label.as_ref().unwrap().value, "go");
    }

    #[test]
    fn a_transition_between_two_inner_states_stays_in_their_state() {
        let diagram =
            diagram("@startuml\nstate Outer {\n  state A\n  state B\n}\nA --> B\n@enduml\n");
        let outer = node(&diagram.root, "Outer");
        assert_eq!(outer.children.edges.len(), 1);
        assert!(diagram.root.crossings.is_empty());
    }

    /// PlantUML's own example: `Configuring` is made inside `NotShooting`,
    /// and `state Configuring {` later opens it there.
    #[test]
    fn a_state_only_a_transition_made_moves_where_it_is_declared() {
        // PlantUML 1.2026.8 draws `Configuring` beside `NotShooting`: a
        // transition inside made it, the body outside places it.
        let d = diagram(
            "@startuml\n[*] --> NotShooting\nstate NotShooting {\n  [*] --> Idle\n  Idle --> Configuring : EvConfig\n}\nstate Configuring {\n  [*] --> NewValueSelection\n}\n@enduml\n",
        );
        let shooting = node(&d.root, "NotShooting");
        assert!(
            shooting
                .children
                .nodes
                .iter()
                .all(|n| n.key != "Configuring")
        );
        let configuring = node(&d.root, "Configuring");
        assert!(configuring.composite);
        node(&configuring.children, "NewValueSelection");
        // A description moves it too, into another state as well; a
        // transition does not, nor a state declared where it is.
        let moved = |source: &str, outer: &str, key: &str| {
            let d = diagram(&format!("@startuml\n{source}\n@enduml\n"));
            let holder = if outer.is_empty() {
                &d.root
            } else {
                &node(&d.root, outer).children
            };
            assert!(holder.nodes.iter().any(|n| n.key == key), "{source}");
        };
        moved(
            "state On {\n  [*] --> Idle\n  Idle --> Playing : play\n}\nPlaying : fault",
            "",
            "Playing",
        );
        moved(
            "state On {\n  [*] --> Idle\n  Idle --> Playing : play\n}\nstate Off {\n  Playing : fault\n}",
            "Off",
            "Playing",
        );
        moved(
            "state On {\n  [*] --> Idle\n  Idle --> Playing : play\n}\nPlaying --> Off : stop",
            "On",
            "Playing",
        );
        moved(
            "state On {\n  state Playing\n}\nPlaying : fault",
            "On",
            "Playing",
        );
        let err = declined(
            "@startuml\nstate On {\n  [*] --> A\n  --\n  [*] --> B\n  B --> C\n}\nC : here\n@enduml\n",
        );
        assert!(
            err.reason.contains("across a concurrent region"),
            "{}",
            err.reason
        );
    }

    /// PlantUML's own example: a quoted name is shown, on either side of
    /// `as`.
    #[test]
    fn an_alias_on_either_side_of_as() {
        let diagram = diagram(
            "@startuml\nstate alias1\nstate \"alias2\"\nstate \"long name\" as alias3\nstate alias4 as \"long name\"\nalias1 -> alias2\nalias2 -> alias3\nalias3 -> alias4\n@enduml\n",
        );
        let shown: Vec<(&str, &str)> = diagram
            .root
            .nodes
            .iter()
            .map(|n| (n.key.as_str(), n.label.value.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("alias1", "alias1"),
                ("alias2", "alias2"),
                ("alias3", "long name"),
                ("alias4", "long name"),
            ]
        );
        assert_eq!(diagram.root.edges.len(), 3);
    }

    /// PlantUML's own example: `A.X` is `X` inside `A`.
    #[test]
    fn a_dotted_name_is_a_state_inside_another() {
        let diagram = diagram(
            "@startuml\nstate A.X\nstate A.Y\nstate B.Z\nX --> Z\nZ --> Y\nB.Z --> A.Y : back\n@enduml\n",
        );
        let keys: Vec<&str> = diagram.root.nodes.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(keys, ["A", "B"]);
        let a = node(&diagram.root, "A");
        assert!(a.composite);
        let inside: Vec<&str> = a
            .children
            .nodes
            .iter()
            .map(|n| n.label.value.as_str())
            .collect();
        assert_eq!(inside, ["X", "Y"]);
        node(&node(&diagram.root, "B").children, "Z");
        assert_eq!(diagram.root.crossings.len(), 3);
        let err = declined("@startuml\nstate A.\n@enduml\n");
        assert!(err.reason.contains("empty part"), "{}", err.reason);
    }

    #[test]
    fn states_named_x_and_o_are_states_and_heads_still_decline() {
        let diagram = diagram("@startuml\nX --> O\nO -> x : back\n@enduml\n");
        let keys: Vec<&str> = diagram.root.nodes.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(keys, ["X", "O", "x"]);
        assert_eq!(diagram.root.edges.len(), 2);
        for arrow in [
            "a x-> b",
            "a ->o b",
            "a o-> b",
            "a -[#red]->o b",
            "a o-[#red]-> b",
        ] {
            let err = declined(&format!("@startuml\n{arrow}\n@enduml\n"));
            assert!(err.reason.contains("arrow"), "{arrow}: {}", err.reason);
        }
    }

    #[test]
    fn a_history_sits_in_its_state() {
        let diagram = diagram(
            "@startuml\nstate Idle\nstate Busy {\n  [*] --> Work\n  Idle --> [H] : resume\n}\nIdle --> Busy[H*] : deep\nIdle --> Busy[H] : again\n@enduml\n",
        );
        let busy = node(&diagram.root, "Busy");
        let kinds: Vec<Kind> = busy.children.nodes.iter().map(|n| n.kind).collect();
        assert_eq!(
            kinds,
            [Kind::Start, Kind::State, Kind::History, Kind::DeepHistory],
            "`[H]` and `Busy[H]` are one mark"
        );
        let into: Vec<usize> = diagram
            .root
            .crossings
            .iter()
            .map(|c| c.to[c.to.len() - 1])
            .collect();
        assert_eq!(into, [2, 3, 2]);
    }

    #[test]
    fn a_history_may_come_before_its_state() {
        let diagram = diagram(
            "@startuml\n[*] --> Idle\nIdle --> Later[H] : back\nstate Later {\n  [*] --> Work\n}\n@enduml\n",
        );
        let later = node(&diagram.root, "Later");
        assert!(later.composite);
        assert_eq!(later.children.nodes[0].kind, Kind::History);
        node(&later.children, "Work");
    }

    #[test]
    fn a_history_without_its_state_declines() {
        let err = declined("@startuml\n[*] --> A\nA --> [H]\n@enduml\n");
        assert_eq!(err.line, Some(2));
        let err = declined(
            "@startuml\nstate A {\n  [*] --> B\n  --\n  [*] --> C\n}\nAway --> A[H]\n@enduml\n",
        );
        assert!(err.reason.contains("concurrent regions"), "{}", err.reason);
    }

    #[test]
    fn regions_split_a_state_and_move_what_it_held() {
        let diagram = diagram(
            "@startuml\nstate Away\nstate A {\n  state B\n  Away --> B\n  --\n  [*] --> C\n}\n@enduml\n",
        );
        let a = node(&diagram.root, "A");
        assert_eq!(a.children.split, Some(Split::Stacked));
        assert_eq!(a.children.nodes.len(), 2);
        assert!(a.children.nodes.iter().all(|n| n.kind == Kind::Region));
        node(&a.children.nodes[0].children, "B");
        node(&a.children.nodes[1].children, "C");
        let crossing = &diagram.root.crossings[0];
        assert_eq!(crossing.to, vec![1, 0, 0], "B moved into the first region");
    }

    #[test]
    fn regions_side_by_side() {
        let diagram = diagram("@startuml\nstate A {\n  state B\n  ||\n  state C\n}\n@enduml\n");
        assert_eq!(node(&diagram.root, "A").children.split, Some(Split::Beside));
    }

    #[test]
    fn concurrent_regions_need_a_state_around_them() {
        let err = declined("@startuml\n[*] --> A\n--\n[*] --> B\n@enduml\n");
        assert_eq!(err.line, Some(2));
        assert!(err.reason.contains("concurrent"), "{}", err.reason);
    }

    #[test]
    fn mixed_and_empty_regions_decline() {
        let err = declined(
            "@startuml\nstate A {\n  state B\n  --\n  state C\n  ||\n  state D\n}\n@enduml\n",
        );
        assert_eq!(err.line, Some(5));
        assert!(err.reason.contains("mixes"), "{}", err.reason);
        let err = declined("@startuml\nstate A {\n  state B\n  --\n}\n@enduml\n");
        assert_eq!(err.line, Some(3));
        assert!(err.reason.contains("needs a state"), "{}", err.reason);
    }

    #[test]
    fn aliases_descriptions_notes_and_stereotypes() {
        let diagram = diagram(
            "@startuml\nstate \"Playing track\" as Playing #pink\nPlaying : waits\nstate Pick <<choice>>\nnote right of Playing : here\n[*] --> Playing\n@enduml\n",
        );
        let playing = node(&diagram.root, "Playing");
        assert_eq!(playing.label.value, "Playing track");
        assert_eq!(playing.fill.as_deref(), Some("#pink"));
        assert_eq!(playing.description[0].value, "waits");
        assert_eq!(node(&diagram.root, "Pick").kind, Kind::Choice);
        assert_eq!(diagram.root.notes.len(), 1);
        assert_eq!(diagram.root.notes[0].side, Side::Right);
    }

    #[test]
    fn a_multibyte_character_beside_an_arrow_does_not_panic() {
        let source = "@startuml\nA \u{feff}-[#red]-> B\n@enduml\n";
        let (_, lines) = crate::source::body(source).unwrap();
        let _ = parse(&lines);
    }

    #[test]
    fn an_unknown_stereotype_declines() {
        let err = declined("@startuml\nstate X <<entrypoint>>\n@enduml\n");
        assert_eq!(err.line, Some(1));
        assert!(err.reason.contains("entrypoint"), "{}", err.reason);
    }

    #[test]
    fn activity_keeps_a_diagram_that_also_has_a_state_marker() {
        let err = crate::parse("@startuml\nstart\n[*] --> A\nstop\n@enduml").unwrap_err();
        assert!(err.line.is_some(), "{err:?}");
    }

    #[test]
    fn a_sequence_stays_a_sequence_and_a_state_is_a_state() {
        assert!(matches!(
            crate::parse("@startuml\nA -> B\n@enduml"),
            Ok(crate::Diagram::Sequence(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\n[*] --> Idle\n@enduml"),
            Ok(crate::Diagram::State(_))
        ));
    }

    #[test]
    fn left_to_right_is_recorded_on_the_container() {
        let diagram =
            diagram("@startuml\nstate Box {\n  left to right direction\n  [*] --> A\n}\n@enduml\n");
        assert_eq!(diagram.root.direction, Direction::Down);
        assert_eq!(
            node(&diagram.root, "Box").children.direction,
            Direction::Right
        );
    }
}
