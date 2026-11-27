//! PlantUML's old activity syntax (`(*) --> "First Action"`), read into
//! the state diagram's tree and drawn by its drawer, as PlantUML lays it
//! out on a graph too: an action is a state, `(*)` the start where an
//! arrow leaves it and the end where one leads to it, an `if` a choice
//! with its test beside it, `===B1===` a bar, a partition a folder
//! around the actions first named in it, a note a note on the action
//! before it.
//!
//! An arrow without a start leaves the last thing named. In an `if` the
//! first arrow of each branch leaves its choice, the words in brackets
//! (`-->[true]`) on it; after `endif` the flow goes on from where the
//! last branch ended. One dash (`->`) lays its end beside its start, as
//! `-right->` does; two or more lay it below.

use std::collections::HashMap;

use super::{Container, Crossing, Dash, Diagram, Kind, Node, Note, Side, Toward, Transition};
use crate::Declined;
use crate::common::{Frame, Text, keyword, text_at};
use crate::graph::across::holding;
use crate::source::{Line, starts_with_ci};

/// Whether `lines` are written in the old syntax: a line that starts at
/// `(*)` or ends there.
pub fn is_legacy(lines: &[Line]) -> bool {
    lines.iter().any(|line| {
        let t = line.text.trim();
        t.starts_with("(*)") || t.starts_with("(*top)") || t.ends_with("(*)")
    })
}

/// Reads a diagram in the old activity syntax from the lines
/// [`crate::source::body`] gives.
pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut reader = Reader {
        frame: Frame::styled(super::skin::STYLE),
        ..Reader::default()
    };
    let mut at = 0;
    while at < lines.len() {
        at = match reader.frame.line(lines, at)? {
            Some(next) => next,
            None => reader.line(lines, at)?,
        };
    }
    reader.finish()
}

/// Something an arrow starts or ends at, as written.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    /// `(*)` or `(*top)`.
    Star,
    /// `"words"`, `"words" as A`.
    Quoted {
        words: String,
        alias: Option<String>,
    },
    /// `===B1===`.
    Bar(String),
    /// A name: an alias, or the words of an action without quotes.
    Name(String),
    /// `if "test" then`.
    If(String),
}

/// A thing of the diagram as read, before it is placed in the tree.
struct Raw {
    key: String,
    label: Text,
    kind: Kind,
    stereotype: Option<String>,
    /// The partition it was first named in.
    partition: Option<usize>,
    line: usize,
}

struct Partition {
    name: String,
    label: Text,
    fill: Option<String>,
    line: usize,
}

/// What comes first in the diagram's own box: a thing, or a partition.
enum Placed {
    Raw(usize),
    Partition(usize),
}

#[derive(Default)]
struct Reader {
    frame: Frame,
    raws: Vec<Raw>,
    /// Each thing by the names that reach it: its words, its alias.
    keys: HashMap<String, usize>,
    arrows: Vec<(usize, usize, Transition)>,
    notes: Vec<(usize, Side, Text, usize)>,
    partitions: Vec<Partition>,
    order: Vec<Placed>,
    /// The partition being filled.
    open: Option<usize>,
    /// What the next arrow without a start leaves.
    last: Option<usize>,
    /// The choices of the `if`s open, the innermost last.
    ifs: Vec<usize>,
    start: Option<usize>,
    end: Option<usize>,
    /// Every name as its line writes it ([`Diagram::names`]).
    names: Vec<crate::Mention>,
}

impl Reader {
    fn line(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line { no, text, .. } = lines[at];
        let t = text.trim();
        // Where words of `t` start on the line of the source.
        let col = |words: &str| lines[at].at + crate::source::offset_in(text, words);
        if t.is_empty() {
            return Ok(at + 1);
        }
        if let Some(rest) = keyword(t, "partition") {
            self.partition(rest, no)?;
            return Ok(at + 1);
        }
        if t == "}" || squeezed(t, "endpartition") {
            if self.open.take().is_none() {
                return Err(Declined::at(no, "`}` closes no partition"));
            }
            return Ok(at + 1);
        }
        if t.eq_ignore_ascii_case("else") {
            let choice = *self
                .ifs
                .last()
                .ok_or_else(|| Declined::at(no, "`else` belongs to no `if`"))?;
            self.last = Some(choice);
            return Ok(at + 1);
        }
        if squeezed(t, "endif") {
            if self.ifs.pop().is_none() {
                return Err(Declined::at(no, "`endif` closes no `if`"));
            }
            return Ok(at + 1);
        }
        if keyword(t, "note").is_some() {
            return self.note(lines, at);
        }
        if starts_with_ci(t, "if") && t.get(2..3).is_some_and(|c| c == " " || c == "\"") {
            let Some(Token::If(test)) = token(t) else {
                return Err(Declined::at(no, "an `if` is written `if \"test\" then`"));
            };
            let from = self
                .last
                .ok_or_else(|| Declined::at(no, "an `if` needs something before it"))?;
            let test_at = t.find('"').map(|q| col(&t[q + 1..]));
            let choice = self.thing(Token::If(test), false, None, no)?;
            self.raws[choice].label.col = test_at;
            self.arrows
                .push((from, choice, transition(&Shape::default(), None, no)));
            self.last = Some(choice);
            return Ok(at + 1);
        }
        if let Some((start, end, shape)) = find_arrow(t) {
            let right = &t[end..];
            // Words in brackets stand amid the line, where they start.
            let label_at = right.trim_start().strip_prefix('[').map(col);
            return self.arrow(lines, at, &t[..start], shape, (right, label_at));
        }
        Err(Declined::at(
            no,
            format!(
                "`{}` is not part of an activity diagram in the old syntax the native engine can draw",
                crate::common::snippet(t)
            ),
        ))
    }

    /// An arrow from `left` (the last thing when empty) to what follows
    /// it on the line, and on the lines after while its quotes are open.
    fn arrow(
        &mut self,
        lines: &[Line],
        at: usize,
        left: &str,
        shape: Shape,
        (right, label_at): (&str, Option<usize>),
    ) -> Result<usize, Declined> {
        let no = lines[at].no;
        let (label, rest) = bracketed(right.trim_start());
        // Words in quotes over several lines, with what follows them.
        let mut rest = rest.trim().to_owned();
        let mut next = at + 1;
        while rest.starts_with('"') && rest[1..].find('"').is_none() {
            let Some(line) = lines.get(next) else {
                return Err(Declined::at(no, "the quotes opened here are not closed"));
            };
            rest.push('\n');
            rest.push_str(line.text);
            next += 1;
        }
        let (target, stereotype) = split_stereotype(&rest);
        let to = token(target).ok_or_else(|| {
            Declined::at(
                no,
                format!(
                    "`{}` is nothing an arrow leads to",
                    crate::common::snippet(target)
                ),
            )
        })?;
        let from = match left.trim() {
            "" => self
                .last
                .ok_or_else(|| Declined::at(no, "an arrow needs something before it"))?,
            left => {
                let (left, stereotype) = split_stereotype(left);
                let from = token(left).ok_or_else(|| {
                    Declined::at(
                        no,
                        format!(
                            "`{}` is nothing an arrow leaves",
                            crate::common::snippet(left)
                        ),
                    )
                })?;
                self.thing(from, false, stereotype, no)?
            }
        };
        // The test of an `if` an arrow leads to stands amid the line too;
        // `right` is a piece of it, which the words were copied from.
        let test_at = match &to {
            Token::If(_) => right.find('"').map(|q| {
                let line = &lines[at];
                line.at + crate::source::offset_in(line.text, &right[q + 1..])
            }),
            _ => None,
        };
        let to = self.thing(to, true, stereotype, no)?;
        if test_at.is_some() && next == at + 1 {
            self.raws[to].label.col = test_at;
        }
        let mut arrow = transition(&shape, label, no);
        if let Some(label) = &mut arrow.label {
            label.col = label_at;
        }
        self.arrows.push((from, to, arrow));
        self.last = Some(to);
        Ok(next)
    }

    /// The thing `token` names, made the first time: `(*)` is the start
    /// where an arrow leaves it (`target` false) and the end where one
    /// leads to it; an `if` is a new choice, opened.
    fn thing(
        &mut self,
        token: Token,
        target: bool,
        stereotype: Option<String>,
        no: usize,
    ) -> Result<usize, Declined> {
        let at = match token {
            Token::Star => {
                let (slot, kind) = match target {
                    true => (self.end, Kind::End),
                    false => (self.start, Kind::Start),
                };
                match slot {
                    Some(at) => at,
                    None => {
                        let at = self.make(String::new(), text_at("", no), kind, no);
                        match target {
                            true => self.end = Some(at),
                            false => self.start = Some(at),
                        }
                        at
                    }
                }
            }
            // A new alias is a new action, whatever its words, as PlantUML
            // makes one; words alone reach the first with those words.
            Token::Quoted { words, alias } => {
                let found = match &alias {
                    Some(alias) => self.keys.get(alias),
                    None => self.keys.get(&words),
                }
                .copied();
                let at = match found {
                    Some(at) => {
                        self.mention(at, false, no);
                        at
                    }
                    None => {
                        let key = alias.clone().unwrap_or_else(|| words.clone());
                        let at = self.make(key, text_at(&words, no), Kind::State, no);
                        self.mention(at, true, no);
                        at
                    }
                };
                self.keys.entry(words).or_insert(at);
                if let Some(alias) = alias {
                    self.keys.entry(alias).or_insert(at);
                }
                at
            }
            Token::Name(name) => match self.keys.get(&name) {
                Some(&at) => {
                    self.mention(at, false, no);
                    at
                }
                None => {
                    let at = self.make(name.clone(), text_at(&name, no), Kind::State, no);
                    self.keys.insert(name, at);
                    self.mention(at, true, no);
                    at
                }
            },
            Token::Bar(name) => {
                let key = format!("==={name}===");
                match self.keys.get(&key) {
                    Some(&at) => at,
                    None => {
                        let at = self.make(key.clone(), text_at("", no), Kind::Fork, no);
                        self.keys.insert(key, at);
                        at
                    }
                }
            }
            Token::If(test) => {
                let key = format!("if {}", self.raws.len());
                let at = self.make(key, text_at(&test, no), Kind::Choice, no);
                self.ifs.push(at);
                at
            }
        };
        if stereotype.is_some() {
            self.raws[at].stereotype = stereotype;
        }
        Ok(at)
    }

    /// The name of the thing `at` as the line `no` writes it: declaring
    /// it, or using it.
    fn mention(&mut self, at: usize, declares: bool, no: usize) {
        let raw = &self.raws[at];
        let shown = match raw.label.value != raw.key {
            true => raw.label.value.clone(),
            false => String::new(),
        };
        let place = self
            .open
            .map(|p| vec![(self.partitions[p].name.clone(), 0)])
            .unwrap_or_default();
        self.names.push(crate::Mention {
            name: raw.key.clone(),
            shown,
            line: no,
            declares,
            place,
        });
    }

    fn make(&mut self, key: String, label: Text, kind: Kind, line: usize) -> usize {
        let at = self.raws.len();
        self.raws.push(Raw {
            key,
            label,
            kind,
            stereotype: None,
            partition: self.open,
            line,
        });
        if self.open.is_none() {
            self.order.push(Placed::Raw(at));
        }
        at
    }

    /// `partition Name {`, `partition "Long name" #color {`: the same name
    /// again opens the same partition.
    fn partition(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        let Some(rest) = rest.trim().strip_suffix('{') else {
            return Err(Declined::at(
                no,
                "a partition is written `partition Name {`",
            ));
        };
        let rest = rest.trim();
        // A quoted name keeps a `#` in it; the color stands after it.
        let (name, after) = match rest.strip_prefix('"') {
            Some(quoted) => match quoted.find('"') {
                Some(close) => (&quoted[..close], quoted[close + 1..].trim()),
                None => return Err(Declined::at(no, "the partition's name has an open quote")),
            },
            None => match rest.find(" #") {
                Some(at) => (&rest[..at], rest[at..].trim()),
                None => (rest, ""),
            },
        };
        let fill = match after {
            "" => None,
            color if color.starts_with('#') => Some(color.to_owned()),
            _ => {
                return Err(Declined::at(
                    no,
                    "a partition is written `partition Name #color {`",
                ));
            }
        };
        let name = name.trim();
        if name.is_empty() {
            return Err(Declined::at(no, "a partition needs a name"));
        }
        if self.open.is_some() {
            return Err(Declined::at(
                no,
                "a partition inside a partition is not supported yet",
            ));
        }
        let at = match self.partitions.iter().position(|p| p.name == name) {
            Some(at) => at,
            None => {
                self.partitions.push(Partition {
                    name: name.to_owned(),
                    label: text_at(name, no),
                    fill: None,
                    line: no,
                });
                self.order
                    .push(Placed::Partition(self.partitions.len() - 1));
                self.partitions.len() - 1
            }
        };
        if fill.is_some() {
            self.partitions[at].fill = fill;
        }
        self.open = Some(at);
        Ok(())
    }

    /// `note left: words`, or `note right` over lines up to `end note`: a
    /// note on the last thing named.
    fn note(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line { no, text, .. } = lines[at];
        let rest = keyword(text.trim(), "note").unwrap_or_default();
        let (side, rest) = match rest.split_once(|c: char| c == ':' || c.is_whitespace()) {
            Some((side, rest)) => (side.trim(), Some(rest)),
            None => (rest.trim(), None),
        };
        let side = match side.to_ascii_lowercase().as_str() {
            "left" => Side::Left,
            "right" => Side::Right,
            "top" => Side::Top,
            "bottom" => Side::Bottom,
            _ => {
                return Err(Declined::at(
                    no,
                    "a note is written `note left: words` or `note right` … `end note`",
                ));
            }
        };
        let target = self
            .last
            .ok_or_else(|| Declined::at(no, "a note needs something before it"))?;
        let inline = text.trim()[4..].contains(':');
        let (words, next) = if inline {
            let words = rest.unwrap_or_default().trim_start_matches(':').trim();
            (words.to_owned(), at + 1)
        } else {
            let mut body = Vec::new();
            let mut next = at + 1;
            loop {
                let Some(line) = lines.get(next) else {
                    return Err(Declined::at(no, "the note has no `end note`"));
                };
                next += 1;
                if squeezed(line.text, "endnote") {
                    break;
                }
                body.push(line.text);
            }
            (body.join("\n"), next)
        };
        self.notes.push((target, side, text_at(&words, no), no));
        Ok(next)
    }

    /// The tree: the diagram's own things and partitions in the order
    /// they were first named, each partition holding its own; every
    /// arrow in the box that holds both its ends.
    fn finish(self) -> Result<Diagram, Declined> {
        let Reader {
            frame,
            raws,
            arrows,
            notes,
            partitions,
            order,
            names,
            ..
        } = self;
        let mut paths: Vec<Vec<usize>> = vec![Vec::new(); raws.len()];
        let mut root = Container::default();
        for placed in &order {
            match *placed {
                Placed::Raw(at) => {
                    paths[at] = vec![root.nodes.len()];
                    root.nodes.push(node(&raws[at]));
                }
                Placed::Partition(p) => {
                    let partition = &partitions[p];
                    let mut children = Container::default();
                    let here = root.nodes.len();
                    for (at, raw) in raws.iter().enumerate() {
                        if raw.partition == Some(p) {
                            paths[at] = vec![here, children.nodes.len()];
                            children.nodes.push(node(raw));
                        }
                    }
                    root.nodes.push(Node {
                        key: partition.name.clone(),
                        label: partition.label.clone(),
                        fill: partition.fill.clone(),
                        composite: true,
                        folder: true,
                        children,
                        ..node_of(
                            String::new(),
                            text_at("", partition.line),
                            Kind::State,
                            partition.line,
                        )
                    });
                }
            }
        }
        for (from, to, mut arrow) in arrows {
            let (from, to) = (&paths[from], &paths[to]);
            let (home, rest) = from.split_at(from.len() - 1);
            if to.len() == from.len() && to.starts_with(home) {
                arrow.from = rest[0];
                arrow.to = to[to.len() - 1];
                container_mut(&mut root, home).edges.push(arrow);
                continue;
            }
            let (depth, a, b) = holding(from, to).expect("an arrow has two ends");
            arrow.from = a;
            arrow.to = b;
            container_mut(&mut root, &from[..depth])
                .crossings
                .push(Crossing {
                    from: from[depth..].to_vec(),
                    to: to[depth..].to_vec(),
                    arrow,
                });
        }
        for (target, side, text, line) in notes {
            let path = &paths[target];
            let (home, at) = path.split_at(path.len() - 1);
            container_mut(&mut root, home).notes.push(Note {
                target: at[0],
                side,
                text,
                line,
            });
        }
        Ok(Diagram {
            root,
            frame,
            names,
            activity: true,
        })
    }
}

fn container_mut<'a>(root: &'a mut Container, path: &[usize]) -> &'a mut Container {
    let mut container = root;
    for &at in path {
        container = &mut container.nodes[at].children;
    }
    container
}

fn node(raw: &Raw) -> Node {
    Node {
        stereotype: raw.stereotype.clone(),
        ..node_of(raw.key.clone(), raw.label.clone(), raw.kind, raw.line)
    }
}

fn node_of(key: String, label: Text, kind: Kind, line: usize) -> Node {
    Node {
        key,
        label,
        description: Vec::new(),
        kind,
        fill: None,
        line_color: None,
        border_dash: Dash::Solid,
        border_bold: false,
        text_color: None,
        stereotype: None,
        composite: false,
        folder: false,
        children: Container::default(),
        link: None,
        line,
    }
}

/// How an arrow is drawn, as its dashes and brackets say.
#[derive(Debug, Clone, Default, PartialEq)]
struct Shape {
    dash: Dash,
    bold: bool,
    color: Option<String>,
    toward: Option<Toward>,
}

fn transition(shape: &Shape, label: Option<&str>, line: usize) -> Transition {
    Transition {
        from: 0,
        to: 0,
        label: label
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(|label| text_at(label, line)),
        color: shape.color.clone(),
        dash: shape.dash,
        bold: shape.bold,
        line,
        label_before: None,
        toward: shape.toward,
        weight: None,
        note: None,
    }
}

/// The first arrow in `text` outside quotes: where it starts and ends,
/// and its shape. An arrow is dashes (or dots), with a way (`-up->`) or a
/// style (`-[#red,dashed]->`) between them, and `>`.
fn find_arrow(text: &str) -> Option<(usize, usize, Shape)> {
    let bytes = text.as_bytes();
    let mut quoted = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => quoted = !quoted,
            b'-' | b'.' if !quoted => {
                if let Some((len, shape)) = arrow_at(&text[i..]) {
                    return Some((i, i + len, shape));
                }
            }
            _ => {}
        }
    }
    None
}

/// The arrow `text` starts with: its length and its shape.
fn arrow_at(text: &str) -> Option<(usize, Shape)> {
    let b = text.as_bytes();
    let marks = |from: usize| {
        b[from..]
            .iter()
            .take_while(|c| matches!(c, b'-' | b'.'))
            .count()
    };
    let mut shape = Shape::default();
    let first = marks(0);
    if first == 0 {
        return None;
    }
    if b[..first].contains(&b'.') {
        shape.dash = Dash::Dotted;
    }
    let mut j = first;
    let mut between = false;
    if b.get(j) == Some(&b'[') {
        let close = text[j..].find(']')? + j;
        // Hidden whatever else the brackets say, as PlantUML reads them.
        let mut hidden = false;
        for part in text[j + 1..close].split(',') {
            let part = part.trim();
            match part.to_ascii_lowercase().as_str() {
                "dashed" => shape.dash = Dash::Dashed,
                "dotted" => shape.dash = Dash::Dotted,
                "bold" => shape.bold = true,
                "hidden" => hidden = true,
                "plain" | "" => {}
                _ if part.starts_with('#') => shape.color = Some(part.to_owned()),
                _ => return None,
            }
        }
        if hidden {
            shape.dash = Dash::Hidden;
        }
        j = close + 1;
        between = true;
    } else if b.get(j).is_some_and(u8::is_ascii_alphabetic) {
        let len = b[j..]
            .iter()
            .take_while(|c| c.is_ascii_alphabetic())
            .count();
        shape.toward = Some(Toward::of(&text[j..j + len])?);
        j += len;
        between = true;
    }
    if between {
        let more = marks(j);
        if more == 0 {
            return None;
        }
        j += more;
    }
    if b.get(j) != Some(&b'>') {
        return None;
    }
    // One dash lays the end beside the start.
    if !between && first == 1 {
        shape.toward = Some(Toward::Right);
    }
    Some((j + 1, shape))
}

/// Words in brackets at the start of `text` (`[true] "Some Action"`), and
/// what follows them.
fn bracketed(text: &str) -> (Option<&str>, &str) {
    match text.strip_prefix('[').and_then(|rest| rest.split_once(']')) {
        Some((label, rest)) => (Some(label), rest),
        None => (None, text),
    }
}

/// `text` without a stereotype at its end (`"Action" << Begin >>`), and
/// the stereotype's name.
fn split_stereotype(text: &str) -> (&str, Option<String>) {
    let text = text.trim();
    if let Some(rest) = text.strip_suffix(">>")
        && let Some(open) = rest.rfind("<<")
    {
        let name = rest[open + 2..].trim();
        if !name.is_empty() {
            return (rest[..open].trim_end(), Some(name.to_owned()));
        }
    }
    (text, None)
}

/// What `text` names, an arrow's end.
fn token(text: &str) -> Option<Token> {
    let t = text.trim();
    if t == "(*)" || t == "(*top)" {
        return Some(Token::Star);
    }
    if starts_with_ci(t, "if") {
        let rest = t[2..].trim_start();
        if let Some(quoted) = rest.strip_prefix('"')
            && let Some(close) = quoted.find('"')
        {
            let after = quoted[close + 1..].trim();
            if after.is_empty() || after.eq_ignore_ascii_case("then") {
                return Some(Token::If(quoted[..close].to_owned()));
            }
        }
    }
    if let Some(quoted) = t.strip_prefix('"') {
        let close = quoted.find('"')?;
        let words = quoted[..close].to_owned();
        let after = quoted[close + 1..].trim();
        let alias = match keyword(after, "as") {
            Some(alias) if !alias.is_empty() && !alias.contains(char::is_whitespace) => {
                Some(alias.to_owned())
            }
            Some(_) => return None,
            None if after.is_empty() => None,
            None => return None,
        };
        return Some(Token::Quoted { words, alias });
    }
    if t.starts_with('=') {
        let name = t.trim_matches('=').trim();
        return (!name.is_empty() && !name.contains('=')).then(|| Token::Bar(name.to_owned()));
    }
    (!t.is_empty() && !t.contains(['"', '(', ')', '[', ']'])).then(|| Token::Name(t.to_owned()))
}

/// Whether `text` is `word` with any spaces in it, in any case.
fn squeezed(text: &str, word: &str) -> bool {
    crate::common::squeezed_eq(text, word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::body;

    fn read(source: &str) -> Diagram {
        let lines = body(source).unwrap().1;
        assert!(is_legacy(&lines), "{source}");
        parse(&lines).unwrap_or_else(|e| panic!("{e:?}"))
    }

    #[test]
    fn arrows_and_their_shapes() {
        let shape = |text: &str| arrow_at(text);
        assert_eq!(shape("-->").map(|s| s.0), Some(3));
        assert_eq!(shape("->").unwrap().1.toward, Some(Toward::Right));
        assert_eq!(shape("-->").unwrap().1.toward, None);
        assert_eq!(shape("-up->").unwrap().1.toward, Some(Toward::Up));
        assert_eq!(shape("-left->").unwrap().1.toward, Some(Toward::Left));
        let styled = shape("-[#red,dashed]->").unwrap().1;
        assert_eq!(
            (styled.color.as_deref(), styled.dash),
            (Some("#red"), Dash::Dashed)
        );
        assert_eq!(shape("-sideways->"), None);
        assert_eq!(shape("--"), None);
        assert_eq!(find_arrow("\"a -> b\" --> c").map(|a| a.0), Some(9));
    }

    #[test]
    fn tokens() {
        assert_eq!(token("(*)"), Some(Token::Star));
        assert_eq!(
            token("if \"Some Test\" then"),
            Some(Token::If("Some Test".into()))
        );
        assert_eq!(token("if \"\" then"), Some(Token::If(String::new())));
        assert_eq!(
            token("\"action 3\" as a3"),
            Some(Token::Quoted {
                words: "action 3".into(),
                alias: Some("a3".into())
            })
        );
        assert_eq!(token("===B1==="), Some(Token::Bar("B1".into())));
        assert_eq!(token("=== S1 ==="), Some(Token::Bar("S1".into())));
        assert_eq!(
            token("==BEFORE_DESTROY==="),
            Some(Token::Bar("BEFORE_DESTROY".into()))
        );
        assert_eq!(token("Bows"), Some(Token::Name("Bows".into())));
        assert_eq!(
            split_stereotype("\"Climbs\" << Begin >>"),
            ("\"Climbs\"", Some("Begin".to_owned()))
        );
    }

    #[test]
    fn an_if_branches_from_its_choice_and_goes_on_from_its_last_branch() {
        let d = read(
            "@startuml\n(*) --> \"check input\"\nIf \"input is verbose\" then\n--> [Yes] \"turn on verbosity\"\n--> \"run command\"\nelse\n--> \"run command\"\nEndif\n-->(*)\n@enduml\n",
        );
        let keys: Vec<&str> = d.root.nodes.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "",
                "check input",
                "if 2",
                "turn on verbosity",
                "run command",
                ""
            ]
        );
        assert_eq!(d.root.nodes[2].kind, Kind::Choice);
        assert_eq!(d.root.nodes[2].label.value, "input is verbose");
        assert_eq!(d.root.nodes[5].kind, Kind::End);
        let edges: Vec<(usize, usize, Option<&str>)> = d
            .root
            .edges
            .iter()
            .map(|e| (e.from, e.to, e.label.as_ref().map(|l| l.value.as_str())))
            .collect();
        assert_eq!(
            edges,
            [
                (0, 1, None),
                (1, 2, None),
                (2, 3, Some("Yes")),
                (3, 4, None),
                (2, 4, None),
                (4, 5, None),
            ]
        );
    }

    #[test]
    fn partitions_hold_what_is_first_named_in_them() {
        let d = read(
            "@startuml\npartition Conductor {\n  (*) --> \"Climbs on Platform\"\n  --> === S1 ===\n  --> Bows\n}\npartition Audience #LightSkyBlue {\n  === S1 === --> Applauds\n}\npartition Conductor {\n  Bows --> === S2 ===\n}\n@enduml\n",
        );
        let parts: Vec<(&str, usize)> = d
            .root
            .nodes
            .iter()
            .map(|n| (n.key.as_str(), n.children.nodes.len()))
            .collect();
        assert_eq!(parts, [("Conductor", 5), ("Audience", 1)]);
        assert_eq!(d.root.nodes[1].fill.as_deref(), Some("#LightSkyBlue"));
        // `S1` in the conductor leads to `Applauds` in the audience.
        assert_eq!(d.root.crossings.len(), 1);
        assert_eq!(d.root.crossings[0].from, [0, 2]);
        assert_eq!(d.root.crossings[0].to, [1, 0]);
        // A partition is a folder; what it holds is not.
        assert!(d.root.nodes.iter().all(|n| n.folder));
        assert!(d.root.nodes[0].children.nodes.iter().all(|n| !n.folder));
    }

    /// A partition is drawn as PlantUML draws it: a folder, its name bold
    /// in a tab on its top left, a line under the tab and none across the
    /// box; a state that holds others keeps its band.
    #[test]
    fn a_partition_is_a_folder() {
        let svg = crate::render("@startuml\npartition Conductor {\n  (*) --> Bows\n}\n@enduml\n")
            .unwrap();
        let folder = svg
            .split("<polygon points=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .expect("a folder");
        let points: Vec<(f32, f32)> = folder
            .split(' ')
            .map(|p| {
                let (x, y) = p.split_once(',').unwrap();
                (x.parse().unwrap(), y.parse().unwrap())
            })
            .collect();
        assert_eq!(points.len(), 6, "{folder}");
        let [(x, y), (tab, top), (slant, band), (right, under), _, _] = points[..] else {
            unreachable!()
        };
        assert!(top == y && band == under && band > y, "{folder}");
        assert!(x < tab && tab < slant && slant < right, "{folder}");
        let name = svg.find(">Conductor<").expect("the name");
        let text = &svg[svg[..name].rfind("<text").unwrap()..name];
        assert!(
            text.contains(r#"font-weight="bold""#) || text.contains("700"),
            "{text}"
        );
        assert!(!text.contains("text-anchor=\"middle\""), "{text}");
        // Its outline is the one polygon of six corners; a nested state's
        // is a box, and the polygons are the heads.
        let six = |svg: &str| {
            svg.split("<polygon points=\"")
                .skip(1)
                .filter(|p| p.split('"').next().unwrap().split(' ').count() == 6)
                .count()
        };
        assert_eq!(six(&svg), 1);
        let nested =
            crate::render("@startuml\nstate Outer {\n  [*] --> Inner\n}\n@enduml\n").unwrap();
        assert_eq!(six(&nested), 0, "{nested}");
    }

    #[test]
    fn words_over_lines_notes_and_aliases() {
        let d = read(
            "@startuml\n(*) -left-> \"this action\nis long\" as A1\nnote right: on one line\n-up-> \"Another\"\nA1 --> \"Short\"\nnote left\n on\n lines\nend note\n@enduml\n",
        );
        let labels: Vec<&str> = d
            .root
            .nodes
            .iter()
            .map(|n| n.label.value.as_str())
            .collect();
        assert_eq!(labels, ["", "this action\nis long", "Another", "Short"]);
        assert_eq!(d.root.edges[0].toward, Some(Toward::Left));
        assert_eq!(d.root.edges[2].from, 1);
        let notes: Vec<(usize, &str)> = d
            .root
            .notes
            .iter()
            .map(|n| (n.target, n.text.value.as_str()))
            .collect();
        assert_eq!(notes, [(1, "on one line"), (3, "on\nlines")]);
    }

    /// Found in review (2026-11-24): a new alias is a new action, hidden
    /// stays hidden, names are kept as written, a quoted partition keeps a
    /// `#`, and words amid a line say where they stand.
    #[test]
    fn aliases_hidden_arrows_names_and_places() {
        let d = read(
            "@startuml\n(*) --> \"same\" as a1\n(*) --> \"same\" as a2\na1 -[hidden,dashed]-> a2\npartition \"Step #1\" #pink {\n  a2 --> if \"ok?\" then\n  -->[yes] \"B\"\n  endif\n}\n@enduml\n",
        );
        let labels: Vec<&str> = d
            .root
            .nodes
            .iter()
            .map(|n| n.label.value.as_str())
            .collect();
        assert_eq!(labels, ["", "same", "same", "Step #1"]);
        assert_eq!(d.root.nodes[3].fill.as_deref(), Some("#pink"));
        assert_eq!(d.root.edges[2].dash, Dash::Hidden);
        // `if "ok?"` on line 5: its test starts after the quote.
        let choice = &d.root.nodes[3].children.nodes[0];
        assert_eq!((choice.label.line, choice.label.col), (5, Some(13)));
        let yes = d.root.nodes[3]
            .children
            .edges
            .iter()
            .find_map(|e| e.label.as_ref())
            .unwrap();
        assert_eq!((yes.value.as_str(), yes.col), ("yes", Some(6)));
        let names: Vec<(&str, usize, bool)> = d
            .names
            .iter()
            .map(|m| (m.name.as_str(), m.line, m.declares))
            .collect();
        assert_eq!(
            names,
            [
                ("a1", 1, true),
                ("a2", 2, true),
                ("a1", 3, false),
                ("a2", 3, false),
                ("a2", 5, false),
                ("B", 6, true)
            ]
        );
        assert_eq!(d.names[5].place, [("Step #1".to_owned(), 0)]);
    }

    /// A new-syntax diagram whose note or title ends at `(*)` stays one;
    /// one that starts at `(*top)` is an old one (found in review).
    #[test]
    fn what_decides_the_old_syntax() {
        crate::render(
            "@startuml\nstart\n:a;\nnote right\n  the old start was (*)\nend note\nstop\n@enduml\n",
        )
        .unwrap();
        crate::render("@startuml\ntitle Flow (*)\nstart\n:a;\nstop\n@enduml\n").unwrap();
        let svg = crate::render("@startuml\n(*top) --> \"A\"\n\"A\" --> (*)\n@enduml\n").unwrap();
        assert!(svg.contains(">A</tspan>"), "{svg}");
        // A stereotype of more than one byte in a key does not panic.
        crate::render(
            "@startuml\nskinparam activityBackgroundColor<<\u{e4}>> red\n(*) --> \"A\" <<\u{e4}>>\n\"A\" --> (*)\n@enduml\n",
        )
        .unwrap();
    }

    #[test]
    fn declines_what_it_does_not_read() {
        for (source, line) in [
            ("(*) --> \"a\"\nelse", 1),
            ("(*) --> \"a\"\nendif", 1),
            ("(*) --> \"a\"\nwhat is this", 1),
            ("(*) --> \"a\nnever closed", 0),
            ("(*) --> \"a\"\n}", 1),
        ] {
            let source = format!("@startuml\n{source}\n@enduml\n");
            let lines = body(&source).unwrap().1;
            let err = parse(&lines).unwrap_err();
            assert_eq!(err.line, Some(line + 1), "{source}: {err:?}");
        }
    }
}

#[cfg(test)]
mod drawn {
    /// The old syntax as PlantUML draws it, in an activity's look.
    #[test]
    fn an_old_activity_is_drawn_in_an_activity_s_look() {
        let svg = crate::render(
            "@startuml\nskinparam activity {\n  BarColor SaddleBrown\n  BackgroundColor<< Begin >> Olive\n}\n(*) --> \"Climbs\" << Begin >>\n--> ===B1===\n--> if \"Some Test\" then\n  -->[true] \"Some Action\"\n  -right-> (*)\nelse\n  ->[false] \"Other\"\nendif\n@enduml\n",
        )
        .unwrap();
        // Plain names of 12 points, the test beside its diamond, the bar
        // and the stereotype in their skinparams' colors.
        assert!(
            svg.contains(">Climbs</tspan>") && svg.contains(">Some Test</tspan>"),
            "{svg}"
        );
        assert!(!svg.contains("font-weight=\"bold\""), "{svg}");
        assert!(svg.contains("font-size=\"12\""), "{svg}");
        assert!(svg.contains("#8B4513") && svg.contains("#808000"), "{svg}");
        assert!(
            svg.contains(">true</tspan>") && svg.contains(">false</tspan>"),
            "{svg}"
        );
        // `activityShape octagon` cuts the corners of an action's box.
        let svg = crate::render(
            "@startuml\nskinparam activityShape octagon\n(*) --> \"First Action\"\n\"First Action\" --> (*)\n@enduml\n",
        )
        .unwrap();
        assert!(svg.contains("<polygon"), "{svg}");
        // A new-syntax activity still declines a skinparam for a stereotype.
        let err = crate::render(
            "@startuml\nskinparam activityBackgroundColor<<Begin>> Olive\nstart\n:a;\nstop\n@enduml\n",
        )
        .unwrap_err();
        assert_eq!(err.line, Some(1));
    }
}
