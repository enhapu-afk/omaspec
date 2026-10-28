//! From the body's lines to a [`Diagram`]. A line the parser does not
//! understand declines the whole diagram, naming that line.
//!
//! Names belong to the whole diagram, as in PlantUML. A name a line uses
//! before it is declared is created where the line is; a later declaration
//! of that name fills the same element in.

use super::{
    Crossing, Dash, Diagram, Direction, Element, Group, Head, Link, Member, Note, Shape, Side,
    Slot, Text,
};
use crate::Declined;
use crate::common::{Frame, block, brace_delta, keyword, snippet, squeezed_eq, text_at};
use crate::graph::across::meeting;
use crate::source::{Line, starts_with_ci};

/// Whether the lines of an `@startuml` diagram are a class, component,
/// use case, deployment or object diagram.
///
/// A sequence keyword (`participant`, `alt`, …) wins, so a login with
/// `actor` and `database` stays a sequence diagram. An activity or a state
/// diagram is detected before this and wins when both match.
pub fn is_structure(lines: &[Line]) -> bool {
    let mut depth: usize = 0;
    let mut saw = false;
    for line in lines {
        let text = line.text;
        if is_note_header(text) {
            continue;
        }
        if depth > 0 {
            depth = depth.saturating_add_signed(brace_delta(text));
            continue;
        }
        if sequence_line(text) {
            return false;
        }
        if marks_structure(text) {
            saw = true;
        }
        depth = depth.saturating_add_signed(brace_delta(text));
    }
    saw
}

fn is_note_header(text: &str) -> bool {
    keyword(text, "note").is_some()
        || keyword(text, "rnote").is_some()
        || keyword(text, "hnote").is_some()
}

/// A line that only a sequence diagram writes. `actor` and `database` are
/// not here: a use case and a deployment diagram use them too.
fn sequence_line(text: &str) -> bool {
    if text.starts_with("==") || text.starts_with("|||") || text.starts_with("...") {
        return true;
    }
    if text.strip_prefix('[').is_some_and(inner_is_arrow) {
        return true;
    }
    [
        "participant",
        "autonumber",
        "activate",
        "deactivate",
        "destroy",
        "return",
        "alt",
        "opt",
        "loop",
        "par",
        "break",
        "critical",
        "newpage",
        "box",
        "create",
    ]
    .iter()
    .any(|word| keyword(text, word).is_some())
        || keyword(text, "ref")
            .is_some_and(|rest| keyword(rest, "over").is_some() || rest.is_empty())
}

fn marks_structure(text: &str) -> bool {
    let lead = before_label(text);
    // `actor` and `database` are sequence participants as well. They are
    // drawn here once the diagram is a structure diagram, but a line of
    // only those does not make it one. `map`, `json` and `port` do: the
    // parser then declines them, and the Java PlantUML draws them.
    if let Some((shape, _, rest)) = leading_shape(&lead)
        && !rest.is_empty()
        && !matches!(shape, Shape::Actor | Shape::Database)
    {
        return true;
    }
    if ["map", "json", "port", "portin", "portout"]
        .iter()
        .any(|word| keyword(&lead, word).is_some_and(|rest| !rest.is_empty()))
    {
        return true;
    }
    if let Some(inner) = starts_wrapped(&lead, '(', ')')
        && inner != "*"
    {
        return true;
    }
    if ends_wrapped(&lead, '(', ')') {
        return true;
    }
    if let Some(inner) = starts_wrapped(&lead, '[', ']')
        && !inner_is_arrow(inner)
    {
        return true;
    }
    ends_wrapped(&lead, '[', ']')
}

/// The part of a line before `: label`. `:User:` is a name, not a label.
fn before_label(text: &str) -> String {
    let mut quote = false;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    if let Some(end) = actor_span(text) {
        let mut out = String::from("A");
        out.push_str(&text[end..]);
        return before_label(&out);
    }
    for (i, (b, c)) in chars.iter().enumerate() {
        if *c == '"' {
            quote = !quote;
            continue;
        }
        if !quote && *c == ':' && chars.get(i + 1).is_none_or(|(_, n)| *n != ':') {
            return text[..*b].trim_end().to_owned();
        }
    }
    text.to_owned()
}

fn actor_span(text: &str) -> Option<usize> {
    let rest = text.strip_prefix(':')?;
    let end = rest.find(':')?;
    if end == 0 {
        return None;
    }
    Some(end + 2)
}

/// What `text` starts with between `open` and `close`: `Login` in
/// `(Login) -- User`.
fn starts_wrapped(text: &str, open: char, close: char) -> Option<&str> {
    let rest = text.trim().strip_prefix(open)?;
    let inner = &rest[..rest.find(close)?];
    (!inner.is_empty()).then_some(inner)
}

fn ends_wrapped(text: &str, open: char, close: char) -> bool {
    let text = text.trim_end();
    let Some(start) = text.rfind(open) else {
        return false;
    };
    if start == 0 {
        return false;
    }
    let inner = text[start + open.len_utf8()..].strip_suffix(close);
    let Some(inner) = inner else {
        return false;
    };
    let before = text.as_bytes()[start - 1];
    (before.is_ascii_whitespace() || matches!(before, b'>' | b'-' | b'.'))
        && !inner.is_empty()
        && inner != "*"
        && !inner_is_arrow(inner)
}

/// Whether what follows a `[` is an arrow, as in `[-> A` or `[o-> A`, not
/// a name, as in `[Order]`.
fn inner_is_arrow(inner: &str) -> bool {
    let inner = inner.trim_start();
    let inner = inner.strip_prefix(['o', 'O', 'x', 'X']).unwrap_or(inner);
    inner.starts_with(['-', '<'])
}

/// Parses a structure diagram from the lines [`crate::source::body`] gives.
pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut parser = Parser::default();
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Open {
    None,
    Members,
    Group,
}

struct RawName {
    key: String,
    label: String,
    shape: Option<Shape>,
}

struct Inherit {
    raw: RawName,
    dash: Dash,
}

struct Decl {
    key: String,
    label: String,
    shape: Shape,
    stereotypes: Vec<String>,
    fill: Option<String>,
    line_color: Option<String>,
    open: Open,
    parents: Vec<Inherit>,
    line: usize,
}

struct ArrowBits {
    from_head: Head,
    to_head: Head,
    dash: Dash,
    bold: bool,
    color: Option<String>,
}

struct RawRel {
    from: RawName,
    to: RawName,
    from_card: Option<String>,
    to_card: Option<String>,
    arrow: ArrowBits,
    label: Option<String>,
}

struct Pending {
    from: RawName,
    to: RawName,
    from_card: Option<String>,
    to_card: Option<String>,
    from_head: Head,
    to_head: Head,
    dash: Dash,
    bold: bool,
    color: Option<String>,
    label: Option<String>,
    line: usize,
    home: Vec<usize>,
}

struct RawNote {
    target: String,
    side: Side,
    text: String,
    line: usize,
}

struct Parser {
    frame: Frame,
    root: Group,
    open: Vec<(Vec<usize>, usize)>,
    members: Option<(Vec<usize>, usize)>,
    slot: Slot,
    last: Option<Vec<usize>>,
    pending: Vec<Pending>,
    raw_notes: Vec<RawNote>,
    notes_made: usize,
    /// The last `left to right direction` or `top to bottom direction`,
    /// which holds for the whole diagram, inside packages too, as in
    /// PlantUML.
    direction: Option<Direction>,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            frame: Frame::default(),
            root: Group::default(),
            open: Vec::new(),
            members: None,
            slot: Slot::Attribute,
            last: None,
            pending: Vec::new(),
            raw_notes: Vec::new(),
            notes_made: 0,
            direction: None,
        }
    }
}

impl Parser {
    fn finish(mut self) -> Result<Diagram, Declined> {
        if let Some((_, line)) = self.members {
            return Err(Declined::at(line, "a declaration has no `}`"));
        }
        if let Some((_, line)) = self.open.last() {
            return Err(Declined::at(*line, "a package has no `}`"));
        }
        let pending = std::mem::take(&mut self.pending);
        let mut resolved = Vec::with_capacity(pending.len());
        for item in pending {
            let from = self.ensure(&item.from, &item.home, item.line)?;
            let to = self.ensure(&item.to, &item.home, item.line)?;
            resolved.push((from, to, item));
        }
        for (from, to, item) in resolved {
            self.place_link(from, to, &item)?;
        }
        self.place_notes()?;
        if let Some(direction) = self.direction {
            run_all(&mut self.root, direction);
        }
        Ok(Diagram {
            root: self.root,
            frame: self.frame,
        })
    }

    fn here(&self) -> Vec<usize> {
        self.open
            .last()
            .map(|(path, _)| path.clone())
            .unwrap_or_default()
    }

    fn container(&self, path: &[usize]) -> &Group {
        let mut group = &self.root;
        for &index in path {
            group = &group.nodes[index].children;
        }
        group
    }

    fn container_mut(&mut self, path: &[usize]) -> &mut Group {
        let mut group = &mut self.root;
        for &index in path {
            group = &mut group.nodes[index].children;
        }
        group
    }

    fn node_mut(&mut self, path: &[usize]) -> &mut Element {
        let (last, home) = path.split_last().expect("an element has a path");
        &mut self.container_mut(home).nodes[*last]
    }

    fn node(&self, path: &[usize]) -> &Element {
        let (last, home) = path.split_last().expect("an element has a path");
        &self.container(home).nodes[*last]
    }

    fn line(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line { no, text, .. } = lines[at];
        if self.members.is_some() {
            return self.member_line(text, no, at);
        }
        if squeezed_eq(text, "lefttorightdirection") {
            self.direction = Some(Direction::Right);
            return Ok(at + 1);
        }
        if squeezed_eq(text, "toptobottomdirection") {
            self.direction = Some(Direction::Down);
            return Ok(at + 1);
        }
        if squeezed_eq(text, "allowmixing") || squeezed_eq(text, "allow_mixing") {
            return Ok(at + 1);
        }
        if text == "}" {
            self.close(no)?;
            return Ok(at + 1);
        }
        if text == "{" {
            self.open_last(no)?;
            return Ok(at + 1);
        }
        if squeezed_eq(text, "endnote") || squeezed_eq(text, "endrnote") {
            return Err(Declined::at(no, "there is no note for this end line"));
        }
        if let Some(word) = rejected(text) {
            let why = match word {
                "map" | "json" => format!("`{word}` diagrams are drawn by the Java PlantUML only"),
                "together" => "`together` is not supported yet".to_owned(),
                _ => format!("`{word}` is not supported yet"),
            };
            return Err(Declined::at(no, why));
        }
        if text.contains("<$") {
            return Err(Declined::at(no, "sprites are not supported yet"));
        }
        if starts_with_ci(text, "<style") {
            return Err(Declined::at(no, "`<style>` is not supported yet"));
        }
        if is_note_header(text) {
            return self.note(lines, at);
        }
        if let Some(decl) = declaration(text, no)? {
            self.declare(decl)?;
            return Ok(at + 1);
        }
        if find_arrow(text).is_some() {
            self.add_relation(relation(text, no)?, no)?;
            return Ok(at + 1);
        }
        if let Some((name, member)) = field_line(text, no)? {
            self.add_field(name, member, no)?;
            return Ok(at + 1);
        }
        if let Some(decl) = bare(text, no)? {
            self.declare(decl)?;
            return Ok(at + 1);
        }
        Err(Declined::at(
            no,
            format!("`{}` is not supported yet", snippet(text)),
        ))
    }

    fn member_line(&mut self, text: &str, no: usize, at: usize) -> Result<usize, Declined> {
        if text == "}" {
            self.members = None;
            return Ok(at + 1);
        }
        if opens_declaration(text) {
            return Err(Declined::at(
                no,
                "a nested declaration is not supported yet",
            ));
        }
        let path = self.members.as_ref().expect("members").0.clone();
        if let Some(member) = member_from(text, no, &mut self.slot)? {
            self.node_mut(&path).members.push(member);
        }
        Ok(at + 1)
    }

    fn declare(&mut self, decl: Decl) -> Result<(), Declined> {
        let path = if let Some(path) = self.find(&decl.key) {
            self.merge(&path, &decl)?;
            path
        } else {
            self.insert(&decl)?
        };
        match decl.open {
            Open::Members => {
                if self.open.len() + 1 > MAX_DEPTH {
                    return Err(Declined::at(decl.line, "declarations nest too deep"));
                }
                self.members = Some((path.clone(), decl.line));
                self.slot = Slot::Attribute;
            }
            Open::Group => {
                if self.open.len() >= MAX_DEPTH {
                    return Err(Declined::at(decl.line, "packages nest too deep"));
                }
                self.node_mut(&path).composite = true;
                self.open.push((path.clone(), decl.line));
            }
            Open::None => {}
        }
        let home = self.here();
        for parent in &decl.parents {
            self.pending.push(Pending {
                from: parent.raw.clone_name(),
                to: RawName {
                    key: decl.key.clone(),
                    label: decl.label.clone(),
                    shape: None,
                },
                from_card: None,
                to_card: None,
                from_head: Head::Triangle,
                to_head: Head::None,
                dash: parent.dash,
                bold: false,
                color: None,
                label: None,
                line: decl.line,
                home: home.clone(),
            });
        }
        self.last = Some(path);
        Ok(())
    }

    fn merge(&mut self, path: &[usize], decl: &Decl) -> Result<(), Declined> {
        let node = self.node_mut(path);
        if node.explicit && node.shape != decl.shape && !compatible(node.shape, decl.shape) {
            return Err(Declined::at(
                decl.line,
                format!("`{}` is already a {}", decl.key, shape_word(node.shape)),
            ));
        }
        if !node.explicit || decl.shape == Shape::Abstract {
            node.shape = decl.shape;
        }
        node.explicit = true;
        if decl.label != decl.key || node.label.value == node.key {
            node.label = text_at(&decl.label, decl.line);
        }
        for stereotype in &decl.stereotypes {
            node.stereotypes.push(text_at(stereotype, decl.line));
        }
        if node.stereotypes.is_empty()
            && let Some(word) = default_stereo(node.shape)
        {
            node.stereotypes.push(text_at(word, decl.line));
        }
        if let Some(fill) = &decl.fill {
            node.fill = Some(fill.clone());
        }
        if let Some(color) = &decl.line_color {
            node.line_color = Some(color.clone());
        }
        Ok(())
    }

    fn insert(&mut self, decl: &Decl) -> Result<Vec<usize>, Declined> {
        let here = self.here();
        let mut stereotypes: Vec<Text> = decl
            .stereotypes
            .iter()
            .map(|s| text_at(s, decl.line))
            .collect();
        if stereotypes.is_empty()
            && let Some(word) = default_stereo(decl.shape)
        {
            stereotypes.push(text_at(word, decl.line));
        }
        let index = self.container(&here).nodes.len();
        self.container_mut(&here).nodes.push(Element {
            key: decl.key.clone(),
            label: text_at(&decl.label, decl.line),
            stereotypes,
            members: Vec::new(),
            shape: decl.shape,
            fill: decl.fill.clone(),
            line_color: decl.line_color.clone(),
            thickness: None,
            text_color: None,
            border_dash: None,
            explicit: true,
            composite: matches!(decl.open, Open::Group),
            children: Group::default(),
            line: decl.line,
        });
        let mut path = here;
        path.push(index);
        Ok(path)
    }

    fn add_relation(&mut self, rel: RawRel, line: usize) -> Result<(), Declined> {
        self.pending.push(Pending {
            from: rel.from,
            to: rel.to,
            from_card: rel.from_card,
            to_card: rel.to_card,
            from_head: rel.arrow.from_head,
            to_head: rel.arrow.to_head,
            dash: rel.arrow.dash,
            bold: rel.arrow.bold,
            color: rel.arrow.color,
            label: rel.label,
            line,
            home: self.here(),
        });
        Ok(())
    }

    fn add_field(&mut self, name: RawName, member: Member, line: usize) -> Result<(), Declined> {
        let home = self.here();
        let path = self.ensure(&name, &home, line)?;
        let node = self.node_mut(&path);
        if node.composite {
            return Err(Declined::at(
                line,
                "a member on a package is not supported yet",
            ));
        }
        node.members.push(member);
        self.last = Some(path);
        Ok(())
    }

    fn ensure(
        &mut self,
        raw: &RawName,
        home: &[usize],
        line: usize,
    ) -> Result<Vec<usize>, Declined> {
        if let Some(path) = self.find(&raw.key) {
            self.upgrade(&path, raw, line)?;
            return Ok(path);
        }
        let shape = raw.shape.unwrap_or(Shape::Class);
        let mut stereotypes = Vec::new();
        if let Some(word) = default_stereo(shape) {
            stereotypes.push(text_at(word, line));
        }
        let index = self.container(home).nodes.len();
        self.container_mut(home).nodes.push(Element {
            key: raw.key.clone(),
            label: text_at(&raw.label, line),
            stereotypes,
            members: Vec::new(),
            shape,
            fill: None,
            line_color: None,
            thickness: None,
            text_color: None,
            border_dash: None,
            explicit: false,
            composite: false,
            children: Group::default(),
            line,
        });
        let mut path = home.to_vec();
        path.push(index);
        Ok(path)
    }

    fn upgrade(&mut self, path: &[usize], raw: &RawName, line: usize) -> Result<(), Declined> {
        let Some(shape) = raw.shape else {
            return Ok(());
        };
        let node = self.node_mut(path);
        if node.shape == shape {
            return Ok(());
        }
        if node.explicit {
            return Err(Declined::at(
                line,
                format!("`{}` is already a {}", raw.key, shape_word(node.shape)),
            ));
        }
        node.shape = shape;
        node.label = text_at(&raw.label, line);
        if node.stereotypes.is_empty()
            && let Some(word) = default_stereo(shape)
        {
            node.stereotypes.push(text_at(word, line));
        }
        Ok(())
    }

    fn find(&self, name: &str) -> Option<Vec<usize>> {
        self.find_in(&self.root, &[], name, true)
            .or_else(|| self.find_in(&self.root, &[], name, false))
    }

    fn find_in(
        &self,
        group: &Group,
        path: &[usize],
        name: &str,
        by_key: bool,
    ) -> Option<Vec<usize>> {
        for (index, node) in group.nodes.iter().enumerate() {
            let hit = if by_key {
                node.key == name
            } else {
                node.label.value == name && node.key != name
            };
            let mut next = path.to_vec();
            next.push(index);
            if hit {
                return Some(next);
            }
            if let Some(found) = self.find_in(&node.children, &next, name, by_key) {
                return Some(found);
            }
        }
        None
    }

    fn place_link(
        &mut self,
        from: Vec<usize>,
        to: Vec<usize>,
        item: &Pending,
    ) -> Result<(), Declined> {
        if from.is_empty() || to.is_empty() {
            return Err(Declined::at(item.line, "a line needs two elements"));
        }
        let link = |from: usize, to: usize| Link {
            from,
            to,
            label: item.label.as_ref().map(|text| text_at(text, item.line)),
            from_card: item.from_card.as_ref().map(|text| text_at(text, item.line)),
            to_card: item.to_card.as_ref().map(|text| text_at(text, item.line)),
            from_head: item.from_head,
            to_head: item.to_head,
            color: item.color.clone(),
            dash: item.dash,
            bold: item.bold,
            line: item.line,
        };
        if from == to {
            let index = *from.last().expect("a path");
            let home = from[..from.len() - 1].to_vec();
            self.container_mut(&home).edges.push(link(index, index));
            return Ok(());
        }
        let home = &from[..from.len() - 1];
        if to.len() == from.len() && to.starts_with(home) {
            let from_i = *from.last().expect("a path");
            let to_i = *to.last().expect("a path");
            let home = home.to_vec();
            self.container_mut(&home).edges.push(link(from_i, to_i));
            return Ok(());
        }
        let Some((depth, a, b)) = meeting(&from, &to) else {
            return Err(Declined::at(
                item.line,
                "a line between a box and something inside it is not supported yet",
            ));
        };
        self.container_mut(&from[..depth]).crossings.push(Crossing {
            from: from[depth..].to_vec(),
            to: to[depth..].to_vec(),
            arrow: link(a, b),
        });
        Ok(())
    }

    fn place_notes(&mut self) -> Result<(), Declined> {
        for note in std::mem::take(&mut self.raw_notes) {
            let Some(path) = self.find(&note.target) else {
                return Err(Declined::at(
                    note.line,
                    format!("`{}` is not an element", note.target),
                ));
            };
            let index = *path.last().expect("a path");
            let home = path[..path.len() - 1].to_vec();
            self.container_mut(&home).notes.push(Note {
                target: index,
                side: note.side,
                text: text_at(&note.text, note.line),
                line: note.line,
            });
        }
        Ok(())
    }

    fn open_last(&mut self, line: usize) -> Result<(), Declined> {
        let Some(path) = self.last.clone() else {
            return Err(Declined::at(line, "there is no declaration for this `{`"));
        };
        let shape = self.node(&path).shape;
        match open_of(shape) {
            Open::None => Err(Declined::at(
                line,
                format!("a {} has no body", shape_word(shape)),
            )),
            Open::Members => {
                self.members = Some((path, line));
                self.slot = Slot::Attribute;
                Ok(())
            }
            Open::Group => {
                if self.open.len() >= MAX_DEPTH {
                    return Err(Declined::at(line, "packages nest too deep"));
                }
                self.node_mut(&path).composite = true;
                self.open.push((path, line));
                Ok(())
            }
        }
    }

    fn close(&mut self, line: usize) -> Result<(), Declined> {
        if self.open.pop().is_none() {
            return Err(Declined::at(line, "there is no `{` for this `}`"));
        }
        Ok(())
    }

    fn note(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line { no, text, .. } = lines[at];
        if keyword(text, "hnote").is_some() {
            return Err(Declined::at(no, "`hnote` is not supported yet"));
        }
        let rest = keyword(text, "rnote")
            .or_else(|| keyword(text, "note"))
            .unwrap_or(text);
        if rest.to_ascii_lowercase().contains("on link") {
            return Err(Declined::at(no, "a note on a line is not supported yet"));
        }
        if keyword(rest, "over").is_some() {
            return Err(Declined::at(
                no,
                "a note over several elements is not supported yet",
            ));
        }
        if rest.starts_with('"') {
            let (body, after) = take_quoted(rest, no)?;
            let key = note_alias(after, no, &mut self.notes_made)?;
            self.insert_note(&key, &body, no)?;
            return Ok(at + 1);
        }
        if let Some(alias) = keyword(rest, "as") {
            let key = alias.trim();
            if key.is_empty() {
                return Err(Declined::at(no, "`as` needs a name"));
            }
            let key = key.to_owned();
            let (body, next) = block(lines, at, "note", end_note)?;
            self.insert_note(&key, &body, no)?;
            return Ok(next);
        }
        if let Some((side, after)) = side_of(rest) {
            let Some(after) = keyword(after, "of") else {
                return Err(Declined::at(no, "a note needs `of` and an element"));
            };
            let (name, inline) = split_label(after);
            let name = name.trim().trim_matches('"');
            if name.is_empty() {
                return Err(Declined::at(no, "a note needs an element"));
            }
            let (body, next) = if let Some(text) = inline {
                (text.to_owned(), at + 1)
            } else {
                block(lines, at, "note", end_note)?
            };
            self.raw_notes.push(RawNote {
                target: name.to_owned(),
                side,
                text: body,
                line: no,
            });
            return Ok(next);
        }
        Err(Declined::at(
            no,
            format!("`{}` is not supported yet", snippet(text)),
        ))
    }

    fn insert_note(&mut self, key: &str, body: &str, line: usize) -> Result<(), Declined> {
        let here = self.here();
        let index = self.container(&here).nodes.len();
        self.container_mut(&here).nodes.push(Element {
            key: key.to_owned(),
            label: text_at(body, line),
            stereotypes: Vec::new(),
            members: Vec::new(),
            shape: Shape::Note,
            fill: None,
            line_color: None,
            thickness: None,
            text_color: None,
            border_dash: None,
            explicit: true,
            composite: false,
            children: Group::default(),
            line,
        });
        let mut path = here;
        path.push(index);
        self.last = Some(path);
        Ok(())
    }
}

impl RawName {
    fn clone_name(&self) -> Self {
        Self {
            key: self.key.clone(),
            label: self.label.clone(),
            shape: self.shape,
        }
    }
}

fn end_note(text: &str) -> bool {
    squeezed_eq(text, "endnote") || squeezed_eq(text, "endrnote")
}

fn note_alias(after: &str, line: usize, made: &mut usize) -> Result<String, Declined> {
    let after = after.trim();
    if after.is_empty() {
        *made += 1;
        return Ok(format!("\u{1}note{made}"));
    }
    let Some(alias) = keyword(after, "as") else {
        return Err(Declined::at(
            line,
            format!("`{}` is not supported yet", snippet(after)),
        ));
    };
    if alias.is_empty() {
        return Err(Declined::at(line, "`as` needs a name"));
    }
    Ok(alias.to_owned())
}

fn side_of(text: &str) -> Option<(Side, &str)> {
    for (word, side) in [
        ("left", Side::Left),
        ("right", Side::Right),
        ("top", Side::Top),
        ("bottom", Side::Bottom),
    ] {
        if let Some(rest) = keyword(text, word) {
            return Some((side, rest));
        }
    }
    None
}

fn rejected(text: &str) -> Option<&'static str> {
    [
        "together",
        "hide",
        "show",
        "remove",
        "sprite",
        "skin",
        "style",
        "url",
        "link",
        "map",
        "json",
        "port",
        "portin",
        "portout",
        "circle",
        "diamond",
        "queue",
        "boundary",
        "control",
        "entity",
        "collections",
    ]
    .into_iter()
    .find(|word| keyword(text, word).is_some())
}

fn leading_shape(text: &str) -> Option<(Shape, Open, &str)> {
    const PHRASES: &[(&str, Shape, Open)] = &[
        ("abstract class", Shape::Abstract, Open::Members),
        ("abstract", Shape::Abstract, Open::Members),
        ("annotation", Shape::Annotation, Open::Members),
        ("interface", Shape::Interface, Open::Members),
        ("dataclass", Shape::Class, Open::Members),
        ("exception", Shape::Class, Open::Members),
        ("metaclass", Shape::Class, Open::Members),
        ("protocol", Shape::Interface, Open::Members),
        ("component", Shape::Component, Open::Group),
        ("namespace", Shape::Package, Open::Group),
        ("rectangle", Shape::Rectangle, Open::Group),
        ("artifact", Shape::Artifact, Open::Group),
        ("database", Shape::Database, Open::Group),
        ("usecase", Shape::UseCase, Open::None),
        ("storage", Shape::Storage, Open::Group),
        ("hexagon", Shape::Hexagon, Open::Group),
        ("package", Shape::Package, Open::Group),
        ("folder", Shape::Folder, Open::Group),
        ("object", Shape::Object, Open::Members),
        ("struct", Shape::Class, Open::Members),
        ("record", Shape::Class, Open::Members),
        ("class", Shape::Class, Open::Members),
        ("cloud", Shape::Cloud, Open::Group),
        ("frame", Shape::Frame, Open::Group),
        ("person", Shape::Actor, Open::None),
        ("actor", Shape::Actor, Open::None),
        ("agent", Shape::Agent, Open::Group),
        ("stack", Shape::Stack, Open::Group),
        ("node", Shape::Node, Open::Group),
        ("enum", Shape::Enum, Open::Members),
        ("card", Shape::Card, Open::Group),
        ("file", Shape::File, Open::Group),
    ];
    for &(word, shape, open) in PHRASES {
        if let Some(rest) = phrase(text, word) {
            return Some((shape, open, rest));
        }
    }
    None
}

/// Whether a line inside a body declares something of its own, most
/// likely after a missing `}`: `class Inner {` or `note left of A`.
/// Members such as `FILE`, `node : Node` or `abstract void draw()` do not.
fn opens_declaration(text: &str) -> bool {
    if let Some(rest) = keyword(text, "note") {
        return side_of(rest).is_some_and(|(_, after)| keyword(after, "of").is_some());
    }
    text.ends_with('{')
        && leading_shape(text)
            .is_some_and(|(_, _, rest)| rest.starts_with(|c: char| c.is_alphanumeric() || c == '"'))
}

fn phrase<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    if !starts_with_ci(text, word) {
        return None;
    }
    let rest = &text[word.len()..];
    match rest.chars().next() {
        None => Some(rest),
        Some(c)
            if c.is_whitespace() || matches!(c, '{' | '<' | '"' | '(' | '[' | ':' | '#' | '.') =>
        {
            Some(rest.trim())
        }
        _ => None,
    }
}

fn open_of(shape: Shape) -> Open {
    match shape {
        Shape::Class
        | Shape::Abstract
        | Shape::Interface
        | Shape::Enum
        | Shape::Annotation
        | Shape::Object => Open::Members,
        Shape::UseCase | Shape::Actor | Shape::Note => Open::None,
        _ => Open::Group,
    }
}

fn default_stereo(shape: Shape) -> Option<&'static str> {
    match shape {
        Shape::Interface => Some("interface"),
        Shape::Enum => Some("enum"),
        Shape::Annotation => Some("annotation"),
        _ => None,
    }
}

fn shape_word(shape: Shape) -> &'static str {
    match shape {
        Shape::Class | Shape::Abstract => "class",
        Shape::Interface => "interface",
        Shape::Enum => "enum",
        Shape::Annotation => "annotation",
        Shape::Component => "component",
        Shape::UseCase => "use case",
        Shape::Actor => "actor",
        Shape::Node => "node",
        Shape::Artifact => "artifact",
        Shape::Cloud => "cloud",
        Shape::Database => "database",
        Shape::Folder => "folder",
        Shape::Frame => "frame",
        Shape::Rectangle => "rectangle",
        Shape::Object => "object",
        Shape::Package => "package",
        Shape::File => "file",
        Shape::Card => "card",
        Shape::Storage => "storage",
        Shape::Stack => "stack",
        Shape::Hexagon => "hexagon",
        Shape::Agent => "agent",
        Shape::Note => "note",
    }
}

fn compatible(a: Shape, b: Shape) -> bool {
    matches!(
        (a, b),
        (Shape::Class, Shape::Abstract) | (Shape::Abstract, Shape::Class)
    )
}

fn declaration(text: &str, line: usize) -> Result<Option<Decl>, Declined> {
    let Some((shape, open, rest)) = leading_shape(text) else {
        return Ok(None);
    };
    Ok(Some(parse_decl(shape, open, rest, line)?))
}

fn parse_decl(shape: Shape, open: Open, rest: &str, line: usize) -> Result<Decl, Declined> {
    let mut rest = rest.trim();
    let mut opened = false;
    if let Some(body) = rest.strip_suffix('{') {
        opened = true;
        rest = body.trim();
    }
    if rest.is_empty() {
        return Err(Declined::at(line, "a declaration needs a name"));
    }
    let (rest, stereotypes) = take_stereotypes(rest, line)?;
    let (rest, fill, line_color) = take_colors(&rest, line)?;
    let (name, parents) = take_inherits(rest.trim(), line)?;
    if name.is_empty() {
        return Err(Declined::at(line, "a declaration needs a name"));
    }
    let (key, label, _) = split_alias(&name, line)?;
    if key.is_empty() {
        return Err(Declined::at(line, "a declaration needs a name"));
    }
    if opened && matches!(open, Open::None) {
        return Err(Declined::at(
            line,
            format!("a {} has no body", shape_word(shape)),
        ));
    }
    Ok(Decl {
        key,
        label,
        shape,
        stereotypes,
        fill,
        line_color,
        open: if opened { open } else { Open::None },
        parents,
        line,
    })
}

fn bare(text: &str, line: usize) -> Result<Option<Decl>, Declined> {
    if find_arrow(text).is_some() {
        return Ok(None);
    }
    let mut rest = text.trim();
    let mut opened = false;
    if let Some(body) = rest.strip_suffix('{') {
        opened = true;
        rest = body.trim();
    }
    let (rest, stereotypes) = take_stereotypes(rest, line)?;
    let (rest, fill, line_color) = take_colors(&rest, line)?;
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(None);
    }
    let shaped = rest.starts_with(['(', '[', ':', '"']);
    if !shaped && split_as(rest).is_none() && stereotypes.is_empty() && fill.is_none() {
        return Ok(None);
    }
    let (key, label, shape) = split_alias(rest, line)?;
    let shape = shape.unwrap_or(Shape::Class);
    let open = open_of(shape);
    if opened && matches!(open, Open::None) {
        return Err(Declined::at(
            line,
            format!("a {} has no body", shape_word(shape)),
        ));
    }
    Ok(Some(Decl {
        key,
        label,
        shape,
        stereotypes,
        fill,
        line_color,
        open: if opened { open } else { Open::None },
        parents: Vec::new(),
        line,
    }))
}

fn take_stereotypes(input: &str, line: usize) -> Result<(String, Vec<String>), Declined> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut stereos = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' && chars.get(i + 1) == Some(&'<') {
            let start = i + 2;
            let mut j = start;
            let mut end = None;
            while j + 1 < chars.len() {
                if chars[j] == '>' && chars[j + 1] == '>' {
                    end = Some(j);
                    break;
                }
                j += 1;
            }
            let Some(end) = end else {
                return Err(Declined::at(line, "a stereotype has no `>>`"));
            };
            let text: String = chars[start..end].iter().collect();
            let text = text.trim();
            if text.is_empty() || text.contains(['(', '#', ',']) {
                return Err(Declined::at(line, "this stereotype is not supported yet"));
            }
            stereos.push(text.to_owned());
            i = end + 2;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    Ok((out, stereos))
}

fn take_colors(
    input: &str,
    line: usize,
) -> Result<(String, Option<String>, Option<String>), Declined> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut fill = None;
    let mut line_color = None;
    let mut i = 0;
    let mut quote = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            quote = !quote;
            out.push(c);
            i += 1;
            continue;
        }
        if !quote && c == '#' {
            let start = i;
            i += 1;
            while i < chars.len()
                && !chars[i].is_whitespace()
                && !matches!(chars[i], '{' | '<' | '"' | ',')
            {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            let (next_fill, next_line) = read_color(&token, line)?;
            if next_fill.is_some() {
                fill = next_fill;
            }
            if next_line.is_some() {
                line_color = next_line;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    Ok((out, fill, line_color))
}

fn read_color(token: &str, line: usize) -> Result<(Option<String>, Option<String>), Declined> {
    if token.contains([';', '/', '|']) {
        return Err(Declined::at(
            line,
            format!("`{token}` is not supported yet"),
        ));
    }
    if let Some(rest) = token.strip_prefix("##") {
        crate::color::parse(rest).map_err(|err| Declined::at(line, err))?;
        return Ok((None, Some(rest.to_owned())));
    }
    let rest = token.strip_prefix('#').unwrap_or(token);
    crate::color::parse(rest).map_err(|err| Declined::at(line, err))?;
    Ok((Some(rest.to_owned()), None))
}

fn take_inherits(input: &str, line: usize) -> Result<(String, Vec<Inherit>), Declined> {
    let Some(cut) = inherit_at(input) else {
        return Ok((input.to_owned(), Vec::new()));
    };
    let name = input[..cut].trim().to_owned();
    let mut parents = Vec::new();
    parse_inherit_tail(&input[cut..], line, &mut parents)?;
    Ok((name, parents))
}

fn inherit_at(input: &str) -> Option<usize> {
    let mut quote = false;
    let mut depth = 0i32;
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (b, c) = chars[i];
        if c == '"' {
            quote = !quote;
            i += 1;
            continue;
        }
        if !quote && matches!(c, '<' | '(' | '[') {
            depth += 1;
        }
        if !quote && matches!(c, '>' | ')' | ']') {
            depth -= 1;
        }
        if !quote && depth == 0 && word_at(&chars, i, "extends") {
            return Some(b);
        }
        if !quote && depth == 0 && word_at(&chars, i, "implements") {
            return Some(b);
        }
        i += 1;
    }
    None
}

/// Whether `word` (any case) starts at `chars[i]` as a whole word. Compares
/// in place: copying the rest of the line at every position made long
/// lines quadratic.
fn word_at(chars: &[(usize, char)], i: usize, word: &str) -> bool {
    if i > 0 && !chars[i - 1].1.is_whitespace() {
        return false;
    }
    let mut at = i;
    for w in word.chars() {
        match chars.get(at) {
            Some((_, c)) if c.eq_ignore_ascii_case(&w) => at += 1,
            _ => return false,
        }
    }
    chars
        .get(at)
        .is_none_or(|(_, c)| c.is_whitespace() || *c == ',')
}

fn parse_inherit_tail(mut rest: &str, line: usize, out: &mut Vec<Inherit>) -> Result<(), Declined> {
    while !rest.trim().is_empty() {
        let trimmed = rest.trim();
        let (dash, shape, word) = if starts_with_ci(trimmed, "extends")
            && phrase(trimmed, "extends").is_some()
        {
            (Dash::Solid, Shape::Class, "extends")
        } else if starts_with_ci(trimmed, "implements") && phrase(trimmed, "implements").is_some() {
            (Dash::Dashed, Shape::Interface, "implements")
        } else {
            return Err(Declined::at(
                line,
                format!("`{}` is not supported yet", snippet(trimmed)),
            ));
        };
        rest = phrase(trimmed, word).unwrap_or("").trim();
        let cut = inherit_at(rest).unwrap_or(rest.len());
        let list = &rest[..cut];
        for part in list.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let mut raw = raw_name(part, line)?;
            if raw.shape.is_none() {
                raw.shape = Some(shape);
            }
            out.push(Inherit { raw, dash });
        }
        rest = &rest[cut..];
    }
    Ok(())
}

fn split_alias(text: &str, line: usize) -> Result<(String, String, Option<Shape>), Declined> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Declined::at(line, "a declaration needs a name"));
    }
    if let Some((left, right)) = split_as(text) {
        let left_name = raw_name(left, line)?;
        let right_name = raw_name(right, line)?;
        let shape = left_name.shape.or(right_name.shape);
        let (key, label) = if quoted(left) && !quoted(right) {
            (right_name.key, left_name.label)
        } else if !quoted(left) && quoted(right) {
            (left_name.key, right_name.label)
        } else {
            (right_name.key, left_name.label)
        };
        return Ok((key, label, shape));
    }
    let raw = raw_name(text, line)?;
    Ok((raw.key, raw.label, raw.shape))
}

fn quoted(text: &str) -> bool {
    text.trim().starts_with('"')
}

fn split_as(text: &str) -> Option<(&str, &str)> {
    let mut quote = false;
    let mut depth = 0i32;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (b, c) = chars[i];
        if c == '"' {
            quote = !quote;
            i += 1;
            continue;
        }
        if !quote && matches!(c, '<' | '(' | '[') {
            depth += 1;
        }
        if !quote && matches!(c, '>' | ')' | ']') {
            depth -= 1;
        }
        if !quote && depth == 0 && c.is_whitespace() && word_at(&chars, next_word(&chars, i), "as")
        {
            let mut k = next_word(&chars, i);
            // step past "as"
            k += 2;
            while k < chars.len() && chars[k].1.is_whitespace() {
                k += 1;
            }
            if k < chars.len() {
                return Some((text[..b].trim(), text[chars[k].0..].trim()));
            }
        }
        i += 1;
    }
    None
}

fn next_word(chars: &[(usize, char)], i: usize) -> usize {
    let mut j = i;
    while j < chars.len() && chars[j].1.is_whitespace() {
        j += 1;
    }
    j
}

fn raw_name(token: &str, line: usize) -> Result<RawName, Declined> {
    let token = token.trim();
    if token.is_empty() {
        return Err(Declined::at(line, "a name is missing"));
    }
    if let Some(inner) = strip_wrap(token, '(', ')') {
        if inner.is_empty() || inner == "*" {
            return Err(Declined::at(line, "`(*)` is not a use case"));
        }
        return Ok(RawName {
            key: inner.to_owned(),
            label: inner.to_owned(),
            shape: Some(Shape::UseCase),
        });
    }
    if let Some(inner) = strip_wrap(token, '[', ']') {
        if inner.is_empty() {
            return Err(Declined::at(line, "a component needs a name"));
        }
        return Ok(RawName {
            key: inner.to_owned(),
            label: inner.to_owned(),
            shape: Some(Shape::Component),
        });
    }
    if let Some(inner) = strip_wrap(token, ':', ':') {
        if inner.is_empty() {
            return Err(Declined::at(line, "an actor needs a name"));
        }
        return Ok(RawName {
            key: inner.to_owned(),
            label: inner.to_owned(),
            shape: Some(Shape::Actor),
        });
    }
    if let Some(inner) = strip_wrap(token, '"', '"') {
        if inner.is_empty() {
            return Err(Declined::at(line, "a name is missing"));
        }
        return Ok(RawName {
            key: inner.to_owned(),
            label: inner.to_owned(),
            shape: None,
        });
    }
    if token
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '{' | '}'))
    {
        return Err(Declined::at(
            line,
            format!("`{token}` is not a name this diagram draws"),
        ));
    }
    Ok(RawName {
        key: token.to_owned(),
        label: token.to_owned(),
        shape: None,
    })
}

fn strip_wrap(token: &str, open: char, close: char) -> Option<&str> {
    token.strip_prefix(open)?.strip_suffix(close)
}

fn relation(text: &str, line: usize) -> Result<RawRel, Declined> {
    let (lead, label) = split_label(text);
    let (start, end) = find_arrow(lead)
        .ok_or_else(|| Declined::at(line, format!("`{}` is not supported yet", snippet(text))))?;
    let left = lead[..start].trim();
    let arrow = &lead[start..end];
    let right = lead[end..].trim();
    if left.is_empty() || right.is_empty() {
        return Err(Declined::at(line, "a line needs two elements"));
    }
    let (from, from_card) = side(left, true, line)?;
    let (to, to_card) = side(right, false, line)?;
    Ok(RawRel {
        from,
        to,
        from_card,
        to_card,
        arrow: parse_arrow(arrow, line)?,
        label: label.map(str::to_owned),
    })
}

fn side(text: &str, card_last: bool, line: usize) -> Result<(RawName, Option<String>), Declined> {
    let tokens = tokens_of(text, line)?;
    match tokens.as_slice() {
        [name] => Ok((raw_name(name, line)?, None)),
        [name, card] if card_last => Ok((raw_name(name, line)?, Some(cardinality(card, line)?))),
        [card, name] if !card_last => Ok((raw_name(name, line)?, Some(cardinality(card, line)?))),
        _ => Err(Declined::at(
            line,
            format!("`{}` is not supported yet", snippet(text)),
        )),
    }
}

fn cardinality(token: &str, line: usize) -> Result<String, Declined> {
    token
        .trim()
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Declined::at(line, "a cardinality needs quotes"))
}

fn tokens_of(text: &str, line: usize) -> Result<Vec<String>, Declined> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (start, c) = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '"' || c == '(' || c == '[' || c == ':' {
            let close = match c {
                '"' => '"',
                '(' => ')',
                '[' => ']',
                _ => ':',
            };
            i += 1;
            let mut end = None;
            while i < chars.len() {
                if chars[i].1 == close {
                    end = Some(chars[i].0 + chars[i].1.len_utf8());
                    i += 1;
                    break;
                }
                i += 1;
            }
            let Some(end) = end else {
                return Err(Declined::at(line, "a name is not closed"));
            };
            out.push(text[start..end].to_owned());
            continue;
        }
        i += 1;
        while i < chars.len() {
            let ch = chars[i].1;
            if ch.is_whitespace() {
                break;
            }
            if ch == '<' {
                i += 1;
                while i < chars.len() && chars[i].1 != '>' {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
                continue;
            }
            i += 1;
        }
        let end = if i < chars.len() {
            chars[i].0
        } else {
            text.len()
        };
        out.push(text[start..end].to_owned());
    }
    Ok(out)
}

fn split_label(text: &str) -> (&str, Option<&str>) {
    let mut quote = false;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (b, c) = chars[i];
        if c == '"' {
            quote = !quote;
            i += 1;
            continue;
        }
        // `:User:` where a name starts is an actor, not the label's colon.
        if !quote
            && c == ':'
            && name_starts_after(&text[..b])
            && let Some(end) = chars[i + 1..].iter().position(|(_, ch)| *ch == ':')
            && end > 0
        {
            i += end + 2;
            continue;
        }
        if !quote && c == ':' && chars.get(i + 1).is_none_or(|(_, n)| *n != ':') {
            let label = text[b + 1..].trim();
            return (text[..b].trim(), Some(label));
        }
        i += 1;
    }
    (text.trim(), None)
}

/// Whether a name may start after `before`: at the start of the line or
/// right after an arrow. Elsewhere a colon starts the label, so
/// `A --> B : at 10:30` keeps its time.
fn name_starts_after(before: &str) -> bool {
    let mut chars = before.trim_end().chars().rev();
    match chars.next() {
        None | Some('-' | '.' | '>') => true,
        Some('|' | '*' | 'o' | 'O' | 'x' | 'X' | '#' | '+' | '^' | '}' | '{' | '\\' | '/') => {
            matches!(chars.next(), Some('-' | '.'))
        }
        _ => false,
    }
}

fn find_arrow(text: &str) -> Option<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    let mut quote = false;
    while i < chars.len() {
        let (b, c) = chars[i];
        if quote {
            if c == '"' {
                quote = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            quote = true;
            i += 1;
            continue;
        }
        if c == '(' || (c == '[' && !bracket_is_arrow(&chars, i)) || c == ':' {
            let close = match c {
                '(' => ')',
                '[' => ']',
                _ => ':',
            };
            if let Some(end) = chars[i + 1..].iter().position(|(_, ch)| *ch == close) {
                i += end + 2;
                continue;
            }
        }
        if arrow_at(&chars, i) {
            return Some((b, consume_arrow(text, &chars, i)));
        }
        i += 1;
    }
    None
}

fn bracket_is_arrow(chars: &[(usize, char)], i: usize) -> bool {
    matches!(
        chars.get(i + 1).map(|(_, c)| *c),
        Some('#' | '-' | '.' | '<' | '>' | 'o' | 'O' | 'x' | 'X')
    )
}

fn arrow_at(chars: &[(usize, char)], i: usize) -> bool {
    let c = chars[i].1;
    let next = chars.get(i + 1).map(|(_, ch)| *ch);
    match c {
        '-' => true,
        '.' => matches!(next, Some('.' | '-' | '>')),
        '<' => matches!(next, Some('-' | '|' | '.' | '>')),
        '*' | 'o' | 'O' | 'x' | 'X' => matches!(next, Some('-' | '.')),
        _ => false,
    }
}

fn consume_arrow(text: &str, chars: &[(usize, char)], mut i: usize) -> usize {
    while i < chars.len() {
        let c = chars[i].1;
        if c == '[' {
            let Some(end) = chars[i + 1..].iter().position(|(_, ch)| *ch == ']') else {
                break;
            };
            i += end + 2;
            continue;
        }
        if matches!(c, '-' | '.' | '<' | '>' | '*' | 'o' | 'O' | 'x' | 'X' | '|') {
            i += 1;
            continue;
        }
        if let Some(n) = direction_chars(chars, i) {
            i += n;
            continue;
        }
        break;
    }
    if i >= chars.len() {
        text.len()
    } else {
        chars[i].0
    }
}

fn parse_arrow(token: &str, line: usize) -> Result<ArrowBits, Declined> {
    if token.contains('+') {
        return Err(Declined::at(line, "nesting with `+` is not supported yet"));
    }
    if token.to_ascii_lowercase().contains("hidden") {
        return Err(Declined::at(line, "`hidden` arrows are not supported yet"));
    }
    if let Some(word) = direction_word(token) {
        return Err(Declined::at(
            line,
            format!("an arrow with a direction (`{word}`) is not supported yet"),
        ));
    }
    let mut rest = token.to_owned();
    let from_head = take_prefix(&mut rest, line)?;
    let to_head = take_suffix(&mut rest, line)?;
    let mut dash = if rest.contains('.') {
        Dash::Dashed
    } else {
        Dash::Solid
    };
    let mut bold = false;
    let mut color = None;
    while let Some(start) = rest.find('[') {
        let Some(end) = rest.find(']') else {
            return Err(Declined::at(line, "an arrow bracket has no `]`"));
        };
        let inside = rest[start + 1..end].to_owned();
        apply_bracket(&inside, &mut dash, &mut bold, &mut color, line)?;
        rest.replace_range(start..end + 1, "");
    }
    if rest.chars().any(|c| c != '-' && c != '.')
        || (rest.is_empty() && from_head == Head::None && to_head == Head::None)
    {
        return Err(Declined::at(
            line,
            format!("`{token}` is not a line this diagram draws"),
        ));
    }
    Ok(ArrowBits {
        from_head,
        to_head,
        dash,
        bold,
        color,
    })
}

/// `up` inside `-up->`, not a name that merely follows an arrow.
/// Sets the direction of `group` and of every group inside it.
fn run_all(group: &mut Group, direction: Direction) {
    group.direction = direction;
    for node in &mut group.nodes {
        run_all(&mut node.children, direction);
    }
}

fn direction_chars(chars: &[(usize, char)], i: usize) -> Option<usize> {
    for word in ["down", "left", "right", "up"] {
        let letters: Vec<char> = word.chars().collect();
        if chars.len() < i + letters.len() {
            continue;
        }
        let matches = letters
            .iter()
            .enumerate()
            .all(|(k, letter)| chars[i + k].1.eq_ignore_ascii_case(letter));
        if !matches {
            continue;
        }
        let after = chars.get(i + letters.len()).map(|(_, c)| *c);
        if after.is_none_or(|c| {
            matches!(
                c,
                '-' | '.' | '<' | '>' | '[' | '|' | '*' | 'o' | 'O' | 'x' | 'X'
            )
        }) {
            return Some(letters.len());
        }
    }
    None
}

fn direction_word(arrow: &str) -> Option<&'static str> {
    let lower = arrow.to_ascii_lowercase();
    for word in ["up", "down", "left", "right"] {
        if lower.contains(&format!("-{word}"))
            || lower.contains(&format!("{word}-"))
            || lower.contains(&format!("[{word}"))
        {
            return Some(word);
        }
    }
    None
}

fn take_prefix(rest: &mut String, line: usize) -> Result<Head, Declined> {
    if rest.starts_with('x') || rest.starts_with('X') {
        return Err(Declined::at(line, "an `x` arrow head is not supported yet"));
    }
    let head = if let Some(tail) = rest.strip_prefix("<|") {
        *rest = tail.to_owned();
        Head::Triangle
    } else if let Some(tail) = rest.strip_prefix('<') {
        *rest = tail.to_owned();
        Head::Arrow
    } else if let Some(tail) = rest.strip_prefix('*') {
        *rest = tail.to_owned();
        Head::DiamondFilled
    } else if rest.starts_with('o') || rest.starts_with('O') {
        *rest = rest[1..].to_owned();
        Head::Diamond
    } else {
        Head::None
    };
    Ok(head)
}

fn take_suffix(rest: &mut String, line: usize) -> Result<Head, Declined> {
    if rest.ends_with('x') || rest.ends_with('X') {
        return Err(Declined::at(line, "an `x` arrow head is not supported yet"));
    }
    let head = if let Some(tail) = rest.strip_suffix("|>") {
        *rest = tail.to_owned();
        Head::Triangle
    } else if let Some(tail) = rest.strip_suffix('>') {
        *rest = tail.to_owned();
        Head::Arrow
    } else if let Some(tail) = rest.strip_suffix('*') {
        *rest = tail.to_owned();
        Head::DiamondFilled
    } else if rest.ends_with('o') || rest.ends_with('O') {
        rest.pop();
        Head::Diamond
    } else if let Some(tail) = rest.strip_suffix("<|") {
        *rest = tail.to_owned();
        Head::Triangle
    } else if let Some(tail) = rest.strip_suffix('<') {
        *rest = tail.to_owned();
        Head::Arrow
    } else {
        Head::None
    };
    Ok(head)
}

fn apply_bracket(
    inside: &str,
    dash: &mut Dash,
    bold: &mut bool,
    color: &mut Option<String>,
    line: usize,
) -> Result<(), Declined> {
    if inside.trim().is_empty() {
        return Err(Declined::at(
            line,
            "an empty arrow bracket is not supported",
        ));
    }
    for part in inside.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        match part.to_ascii_lowercase().as_str() {
            "dashed" => *dash = Dash::Dashed,
            "dotted" => *dash = Dash::Dotted,
            "bold" => *bold = true,
            "plain" => *dash = Dash::Solid,
            "hidden" => {
                return Err(Declined::at(line, "`hidden` arrows are not supported yet"));
            }
            _ if crate::color::parse(part).is_ok() => *color = Some(part.to_owned()),
            _ => {
                return Err(Declined::at(
                    line,
                    format!("`[{part}]` on an arrow is not supported yet"),
                ));
            }
        }
    }
    Ok(())
}

fn field_line(text: &str, line: usize) -> Result<Option<(RawName, Member)>, Declined> {
    if find_arrow(text).is_some() {
        return Ok(None);
    }
    let (left, label) = split_label(text);
    let Some(body) = label else {
        return Ok(None);
    };
    if body.is_empty() {
        return Err(Declined::at(line, "a member needs a text"));
    }
    if left.split_whitespace().count() != 1 {
        return Ok(None);
    }
    let mut slot = Slot::Attribute;
    let Some(member) = member_from(body, line, &mut slot)? else {
        return Err(Declined::at(line, "a member needs a text"));
    };
    Ok(Some((raw_name(left, line)?, member)))
}

fn member_from(text: &str, line: usize, slot: &mut Slot) -> Result<Option<Member>, Declined> {
    let (italic, underline, forced, rest) = classifiers(text, line)?;
    if let Some(forced) = forced {
        *slot = forced;
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(None);
    }
    if rest.contains(['{', '}']) {
        return Err(Declined::at(
            line,
            format!("`{}` is not supported yet", snippet(text)),
        ));
    }
    if let Some(dash) = rule_of(rest) {
        return Ok(Some(Member::Rule(dash)));
    }
    if let Some(title) = heading_of(rest) {
        return Ok(Some(Member::Heading(text_at(title, line))));
    }
    let use_slot = if rest.contains('(') {
        Slot::Method
    } else {
        *slot
    };
    Ok(Some(Member::Text {
        text: text_at(rest, line),
        italic,
        underline,
        slot: use_slot,
    }))
}

fn classifiers(text: &str, line: usize) -> Result<(bool, bool, Option<Slot>, &str), Declined> {
    let mut italic = false;
    let mut underline = false;
    let mut forced = None;
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start();
        let Some(body) = trimmed.strip_prefix('{') else {
            return Ok((italic, underline, forced, trimmed));
        };
        let Some(end) = body.find('}') else {
            return Err(Declined::at(line, "a member mark has no `}`"));
        };
        let word = body[..end].trim().to_ascii_lowercase();
        rest = body[end + 1..].trim_start();
        match word.as_str() {
            "static" => underline = true,
            "abstract" => italic = true,
            "field" => forced = Some(Slot::Attribute),
            "method" => forced = Some(Slot::Method),
            "" => {}
            _ => {
                return Err(Declined::at(
                    line,
                    format!("`{{{word}}}` on a member is not supported yet"),
                ));
            }
        }
    }
}

fn rule_of(text: &str) -> Option<Dash> {
    let text = text.trim();
    if text.chars().count() < 2 {
        return None;
    }
    if text.chars().all(|c| c == '-' || c == '=') {
        Some(Dash::Solid)
    } else if text.chars().all(|c| c == '.') {
        Some(Dash::Dashed)
    } else if text.chars().all(|c| c == '_') {
        Some(Dash::Solid)
    } else {
        None
    }
}

fn heading_of(text: &str) -> Option<&str> {
    let inner = text.trim().strip_prefix("==")?.strip_suffix("==")?.trim();
    (!inner.is_empty()).then_some(inner)
}

fn take_quoted(text: &str, line: usize) -> Result<(String, &str), Declined> {
    let rest = text.trim_start();
    let body = rest
        .strip_prefix('"')
        .ok_or_else(|| Declined::at(line, "a note needs a text"))?;
    let Some(end) = body.find('"') else {
        return Err(Declined::at(line, "a note has no closing quote"));
    };
    Ok((body[..end].to_owned(), body[end + 1..].trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines_of(source: &str) -> Vec<crate::source::Line<'_>> {
        crate::source::body(source).unwrap().1
    }

    fn diagram(source: &str) -> Diagram {
        parse(&lines_of(source)).unwrap_or_else(|err| panic!("{err:?}"))
    }

    fn declined(source: &str) -> crate::Declined {
        parse(&lines_of(source)).unwrap_err()
    }

    fn element<'a>(group: &'a Group, key: &str) -> &'a Element {
        group
            .nodes
            .iter()
            .find(|node| node.key == key)
            .unwrap_or_else(|| panic!("no {key}"))
    }

    #[test]
    fn detection_leaves_the_other_diagrams_alone() {
        assert!(matches!(
            crate::parse("@startuml\nactor User\nparticipant Web\nUser -> Web : hi\n@enduml"),
            Ok(crate::Diagram::Sequence(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\ndatabase Sessions\nUser -> Sessions : id\n@enduml"),
            Ok(crate::Diagram::Sequence(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\n[*] --> Idle\n@enduml"),
            Ok(crate::Diagram::State(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\nstart\n:go;\nstop\n@enduml"),
            Ok(crate::Diagram::Activity(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\nclass Foo\n@enduml"),
            Ok(crate::Diagram::Structure(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\n:User: --> (Login)\n@enduml"),
            Ok(crate::Diagram::Structure(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\n[Web] --> [Api]\n@enduml"),
            Ok(crate::Diagram::Structure(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\nnode Server\n@enduml"),
            Ok(crate::Diagram::Structure(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\nobject order\n@enduml"),
            Ok(crate::Diagram::Structure(_))
        ));
    }

    #[test]
    fn a_colon_after_a_name_starts_the_label() {
        let diagram = diagram(
            "@startuml\nclass A\nclass B\nA --> B : GET http://host\nA --> B : at 10:30\nclass User\nUser : password : String\n:Admin: --> (Login) : uses\n@enduml\n",
        );
        let labels: Vec<_> = diagram
            .root
            .edges
            .iter()
            .filter_map(|edge| edge.label.as_ref().map(|l| l.value.as_str()))
            .collect();
        assert_eq!(labels, ["GET http://host", "at 10:30", "uses"]);
        let user = element(&diagram.root, "User");
        assert!(matches!(
            &user.members[0],
            Member::Text { text, .. } if text.value == "password : String"
        ));
        element(&diagram.root, "Admin");
    }

    #[test]
    fn members_may_start_with_a_shape_word() {
        let diagram = diagram(
            "@startuml\nenum NodeType {\n  FILE\n  FOLDER\n}\nclass Tree {\n  file : File\n  node : Node\n  abstract void draw()\n}\n@enduml\n",
        );
        assert_eq!(element(&diagram.root, "NodeType").members.len(), 2);
        assert_eq!(element(&diagram.root, "Tree").members.len(), 3);
        let err = declined("@startuml\nclass A {\n  x : int\nclass B {\n}\n@enduml\n");
        assert_eq!(err.line, Some(3));
        let err = declined("@startuml\nclass A {\nnote left of A\n}\n@enduml\n");
        assert_eq!(err.line, Some(2));
    }

    #[test]
    fn classes_members_and_lines() {
        let diagram = diagram(
            "@startuml\nclass Product {\n  - id : int\n  {static} + count : int\n  --\n  + name() : String\n}\ninterface Sellable\nProduct <|-- Book\nSellable <|.. Book\nclass Shelf\nShelf *-- \"1\" Product : holds\n@enduml\n",
        );
        let product = element(&diagram.root, "Product");
        assert!(matches!(
            product.members[0],
            Member::Text {
                underline: false,
                slot: Slot::Attribute,
                ..
            }
        ));
        assert!(matches!(
            product.members[1],
            Member::Text {
                underline: true,
                ..
            }
        ));
        assert!(matches!(product.members[2], Member::Rule(_)));
        assert!(matches!(
            product.members[3],
            Member::Text {
                slot: Slot::Method,
                ..
            }
        ));
        assert_eq!(element(&diagram.root, "Sellable").shape, Shape::Interface);
        let extends = diagram
            .root
            .edges
            .iter()
            .find(|edge| edge.from_head == Head::Triangle && edge.dash == Dash::Solid)
            .unwrap();
        assert_eq!(diagram.root.nodes[extends.from].key, "Product");
        assert_eq!(diagram.root.nodes[extends.to].key, "Book");
        let owns = diagram
            .root
            .edges
            .iter()
            .find(|edge| edge.from_head == Head::DiamondFilled)
            .unwrap();
        assert_eq!(
            owns.to_card.as_ref().map(|text| text.value.as_str()),
            Some("1")
        );
        assert_eq!(
            owns.label.as_ref().map(|text| text.value.as_str()),
            Some("holds")
        );
    }

    #[test]
    fn a_package_keeps_its_classes_and_a_line_can_leave_it() {
        let diagram = diagram(
            "@startuml\npackage shop {\n  class Book\n}\nclass Shelf\nShelf --> Book\n@enduml\n",
        );
        let package = element(&diagram.root, "shop");
        assert!(package.composite);
        assert_eq!(package.children.nodes[0].key, "Book");
        assert_eq!(diagram.root.crossings.len(), 1);
        assert_eq!(diagram.root.crossings[0].to, vec![0, 0]);
    }

    #[test]
    fn aliases_colors_and_a_note() {
        let diagram = diagram(
            "@startuml\nclass \"Long name\" as L #pink\nusecase (Login) as UC\nactor :Clerk: as C\n[Web] as W\nnote right of L : here\n@enduml\n",
        );
        let long = element(&diagram.root, "L");
        assert_eq!(long.label.value, "Long name");
        assert_eq!(long.fill.as_deref(), Some("pink"));
        assert_eq!(element(&diagram.root, "UC").shape, Shape::UseCase);
        assert_eq!(element(&diagram.root, "UC").label.value, "Login");
        assert_eq!(element(&diagram.root, "C").shape, Shape::Actor);
        assert_eq!(element(&diagram.root, "W").shape, Shape::Component);
        assert_eq!(diagram.root.notes[0].side, Side::Right);
        assert_eq!(diagram.root.notes[0].target, 0);
    }

    #[test]
    fn unknown_lines_name_themselves() {
        let err = declined("@startuml\nclass Foo\ntogether {\n}\n@enduml\n");
        assert_eq!(err.line, Some(2));
        assert!(err.reason.contains("together"), "{}", err.reason);
        let err = declined("@startuml\nclass A\nA -up-> B\n@enduml\n");
        assert_eq!(err.line, Some(2));
        assert!(err.reason.contains("direction"), "{}", err.reason);
        let err = declined("@startuml\nclass A\nA -[hidden]-> B\n@enduml\n");
        assert!(err.reason.contains("hidden"), "{}", err.reason);
        let err = declined("@startuml\ncomponent C {\n  port p\n}\n@enduml\n");
        assert!(err.reason.contains("port"), "{}", err.reason);
        let err = declined("@startuml\nmap Data {\n}\n@enduml\n");
        assert!(err.reason.contains("Java"), "{}", err.reason);
    }

    #[test]
    fn a_direction_inside_a_package_holds_for_the_whole_diagram() {
        let diagram = diagram(
            "@startuml\nA --> B\npackage P {\n  left to right direction\n  C --> D\n}\n@enduml\n",
        );
        assert_eq!(diagram.root.direction, Direction::Right);
        assert_eq!(
            element(&diagram.root, "P").children.direction,
            Direction::Right
        );
    }

    #[test]
    fn extends_and_a_direction() {
        let diagram = diagram(
            "@startuml\nleft to right direction\nabstract class Child extends Parent implements Side\n@enduml\n",
        );
        assert_eq!(diagram.root.direction, Direction::Right);
        assert_eq!(element(&diagram.root, "Child").shape, Shape::Abstract);
        assert_eq!(diagram.root.edges.len(), 2);
        assert!(
            diagram
                .root
                .edges
                .iter()
                .any(|edge| edge.dash == Dash::Dashed)
        );
        assert_eq!(element(&diagram.root, "Side").shape, Shape::Interface);
    }
}
