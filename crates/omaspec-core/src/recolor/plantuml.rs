//! PlantUML's boxes with a name: the color goes onto the line that
//! declares the state, the class, the component or the participant
//! (`state Busy #f7768e`, `[Order] #f7768e`, `participant Bob #f7768e`),
//! after its stereotype and its link, where PlantUML reads it. A box that
//! lines only name gets a declaration before the first of them.

use std::ops::Range;

use omaspec_uml::Diagram;
use omaspec_uml::structure::{Group, Shape};
use omaspec_uml::timing::RowKind;

use super::{Edit, Target, changed, covering, indent_of, split_newline};
use crate::project::key;
use crate::text::line_ranges;

const STATE: &[&str] = &["state"];

/// The keywords a structure diagram declares a box with, the longer first.
const STRUCTURE: &[&str] = &[
    "abstract class",
    "annotation",
    "interface",
    "dataclass",
    "exception",
    "metaclass",
    "component",
    "namespace",
    "rectangle",
    "abstract",
    "protocol",
    "artifact",
    "database",
    "usecase",
    "storage",
    "hexagon",
    "package",
    "folder",
    "object",
    "struct",
    "record",
    "person",
    "entity",
    "class",
    "cloud",
    "frame",
    "actor",
    "agent",
    "stack",
    "node",
    "enum",
    "card",
    "file",
];

const SEQUENCE: &[&str] = &[
    "participant",
    "collections",
    "boundary",
    "database",
    "control",
    "entity",
    "actor",
    "queue",
];

/// A line that declares a box.
struct Decl<'a> {
    /// What it is called: its name, and the other one `as` gives it.
    names: Vec<&'a str>,
    /// Where the names end in the line.
    head: usize,
    /// Written without a keyword, as `[Order]` or `(Login)`.
    bare: bool,
}

/// A name at the start of `text`, and how many bytes it takes: quoted, in
/// the brackets of a component, a use case or an actor, or a word.
fn name_token(text: &str, brackets: bool) -> Option<(&str, usize)> {
    if let Some(inner) = text.strip_prefix('"') {
        let close = inner.find('"')?;
        return Some((&inner[..close], close + 2));
    }
    if brackets {
        for (open, close) in [('[', ']'), ('(', ')'), (':', ':')] {
            if let Some(inner) = text.strip_prefix(open)
                && !inner.starts_with(open)
            {
                let end = inner.find(close)?;
                return (end > 0).then(|| (&inner[..end], end + 2));
            }
        }
    }
    let end = text
        .find(|c: char| c.is_whitespace() || matches!(c, '{' | '<' | '#' | ':' | '[' | '(' | '"'))
        .unwrap_or(text.len());
    (end > 0).then(|| (&text[..end], end))
}

/// How many bytes of space `text` starts with.
fn gap(text: &str) -> usize {
    text.len() - text.trim_start().len()
}

/// What `line` declares with one of `words`: `state Idle`, `state "Long
/// name" as Idle`, `participant Bob as "Robert"`, and with `brackets`
/// `[Order] as O` without a keyword.
fn declared<'a>(line: &'a str, words: &[&str], brackets: bool) -> Option<Decl<'a>> {
    let start = gap(line);
    let trimmed = &line[start..];
    let lower = trimmed.to_ascii_lowercase();
    let word = words.iter().find_map(|word| {
        let rest = lower.strip_prefix(word)?;
        rest.starts_with(char::is_whitespace).then_some(word.len())
    });
    let bare = word.is_none();
    let mut at = match word {
        Some(len) => start + len,
        None if brackets && trimmed.starts_with(['[', '(', ':']) => start,
        None => return None,
    };
    at += gap(&line[at..]);
    let (first, taken) = name_token(&line[at..], brackets)?;
    at += taken;
    let mut names = vec![first];
    let rest = &line[at..];
    let space = gap(rest);
    if space > 0
        && rest[space..]
            .get(..2)
            .is_some_and(|w| w.eq_ignore_ascii_case("as"))
    {
        let after = &rest[space + 2..];
        let more = gap(after);
        if more > 0
            && let Some((second, taken)) = name_token(&after[more..], brackets)
        {
            names.push(second);
            at += space + 2 + more + taken;
        }
    }
    Some(Decl {
        names,
        head: at,
        bare,
    })
}

/// What follows the names of a declaration.
struct Tail {
    /// Its fill, `#color`.
    fill: Option<Range<usize>>,
    /// Where a fill goes when it has none: after the stereotypes and the
    /// link, before the border's `##color`.
    insert: usize,
    /// Where what this reads ends.
    end: usize,
}

fn tail(line: &str, head: usize) -> Tail {
    let mut at = head;
    // `class Box<T>`: the generic belongs to the name.
    if line[at..].starts_with('<')
        && !line[at..].starts_with("<<")
        && let Some(end) = line[at..].find('>')
    {
        at += end + 1;
    }
    let mut insert = at;
    let mut colored = false;
    let mut fill = None;
    loop {
        let from = at + gap(&line[at..]);
        let text = &line[from..];
        let token = text
            .find(|c: char| c.is_whitespace() || matches!(c, '{' | '<'))
            .unwrap_or(text.len());
        let closed = |open: &str, close: &str| {
            text.strip_prefix(open)
                .and_then(|inner| inner.find(close))
                .map(|end| open.len() + end + close.len())
        };
        let order = || {
            let rest = text.get(..5).filter(|w| w.eq_ignore_ascii_case("order"))?;
            let after = &text[rest.len()..];
            let digits = after.trim_start();
            let end = digits
                .find(|c: char| !(c.is_ascii_digit() || c == '-'))
                .unwrap_or(digits.len());
            (gap(after) > 0 && end > 0).then(|| 5 + gap(after) + end)
        };
        if let Some(len) = closed("<<", ">>")
            .or_else(|| closed("[[", "]]"))
            .or_else(order)
        {
            at = from + len;
            if !colored {
                insert = at;
            }
        } else if text.starts_with("##") {
            colored = true;
            at = from + token;
        } else if text.starts_with('#') && token > 1 {
            colored = true;
            if fill.is_none() {
                fill = Some(from..from + token);
            }
            at = from + token;
        } else {
            break;
        }
    }
    Tail {
        fill,
        insert,
        end: at,
    }
}

/// `line` with the fill `color` after its names, which end at `head`, or
/// without a fill; `None` when it has none to take away.
fn filled(line: &str, head: usize, color: Option<&str>) -> Option<String> {
    let tail = tail(line, head);
    Some(match (tail.fill, color) {
        (Some(at), Some(color)) => format!("{}{color}{}", &line[..at.start], &line[at.end..]),
        (Some(at), None) => format!("{}{}", line[..at.start].trim_end(), &line[at.end..]),
        (None, Some(color)) => {
            format!("{} {color}{}", &line[..tail.insert], &line[tail.insert..])
        }
        (None, None) => return None,
    })
}

/// The line that declares the box of key `wanted`, as its range in `code`
/// and where its names end.
fn declaration(
    code: &str,
    words: &[&str],
    brackets: bool,
    wanted: &str,
) -> Option<(Range<usize>, usize)> {
    line_ranges(code).into_iter().find_map(|range| {
        let (line, _) = split_newline(&code[range.clone()]);
        let decl = declared(line, words, brackets)?;
        if !decl.names.iter().any(|name| key(name) == wanted) {
            return None;
        }
        // `[Order] --> [Billing]` names two boxes and declares none.
        let rest = line[tail(line, decl.head).end..].trim();
        (!decl.bare || rest.is_empty() || rest == "{").then_some((range, decl.head))
    })
}

/// The color on the declaration of `name`, if a line declares it.
fn on_declaration(
    code: &str,
    words: &[&str],
    brackets: bool,
    name: &str,
    color: Option<&str>,
) -> Option<Result<Edit, String>> {
    let (range, head) = declaration(code, words, brackets, &key(name))?;
    let (line, newline) = split_newline(&code[range.clone()]);
    Some(changed(
        name,
        range,
        line,
        filled(line, head, color),
        newline,
    ))
}

/// Whether `line` names the box of key `wanted`: as a word of its own, or,
/// for a name of several words (`phrase`), somewhere in it. A short name
/// inside another word (`On` in `Connecting`) is not named.
fn mentions(line: &str, wanted: &str, phrase: bool) -> bool {
    line.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|word| !word.is_empty() && key(word) == *wanted)
        || (phrase && key(line).contains(wanted))
}

/// The first line of `code` that names `name`, or the line `known`.
fn first_line(code: &str, name: &str, known: Option<usize>) -> Result<Range<usize>, String> {
    let ranges = line_ranges(code);
    let wanted = key(name);
    let phrase = name.chars().any(|c| !(c.is_alphanumeric() || c == '_'));
    known
        .and_then(|line| ranges.get(line).cloned())
        .or_else(|| {
            ranges
                .iter()
                .find(|range| {
                    let line = &code[(*range).clone()];
                    !line.trim_start().starts_with(['@', '\'']) && mentions(line, &wanted, phrase)
                })
                .cloned()
        })
        .ok_or_else(|| format!("{name} is not in the source"))
}

/// `lines` written before the line at `before`, as deep as it.
fn before(code: &str, before: &Range<usize>, lines: &[String]) -> Edit {
    let indent = indent_of(&code[before.clone()]);
    let text: String = lines
        .iter()
        .map(|line| format!("{indent}{line}\n"))
        .collect();
    (before.start..before.start, text)
}

fn no_color(name: &str) -> String {
    format!("{name} has no color of its own")
}

/// A name as a declaration writes it: in quotes unless it is one word.
fn written(name: &str) -> String {
    if name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '@'))
    {
        name.to_owned()
    } else {
        format!("\"{name}\"")
    }
}

pub(super) fn state(code: &str, name: &str, color: Option<&str>) -> Result<Edit, String> {
    if let Some(done) = on_declaration(code, STATE, false, name, color) {
        return done;
    }
    let color = color.ok_or_else(|| no_color(name))?;
    let first = first_line(code, name, None)?;
    Ok(before(code, &first, &[format!("state {name} {color}")]))
}

/// The element of key `wanted` in `group`, at any depth: its name, its
/// shape and the line it is first on.
fn element<'a>(group: &'a Group, wanted: &str) -> Option<(&'a str, Shape, usize)> {
    group.nodes.iter().find_map(|node| {
        if key(&node.key) == wanted || key(&node.label.value) == wanted {
            return Some((node.key.as_str(), node.shape, node.line));
        }
        element(&node.children, wanted)
    })
}

/// The line that declares an element of `shape` called `name`.
fn element_line(shape: Shape, name: &str) -> Option<String> {
    let word = match shape {
        Shape::Component => return Some(format!("[{name}]")),
        Shape::UseCase => return Some(format!("({name})")),
        Shape::Note => return None,
        Shape::Class | Shape::Abstract => "class",
        Shape::Interface => "interface",
        Shape::Enum => "enum",
        Shape::Entity => "entity",
        Shape::Annotation => "annotation",
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
    };
    Some(format!("{word} {}", written(name)))
}

pub(super) fn structure(code: &str, name: &str, color: Option<&str>) -> Result<Edit, String> {
    if let Some(done) = on_declaration(code, STRUCTURE, true, name, color) {
        return done;
    }
    let color = color.ok_or_else(|| no_color(name))?;
    let parsed = match omaspec_uml::parse(code) {
        Ok(Diagram::Structure(diagram)) => Some(diagram),
        _ => None,
    };
    let found = parsed
        .as_ref()
        .and_then(|diagram| element(&diagram.root, &key(name)));
    let (line, at) = match found {
        Some((name, shape, at)) => (
            element_line(shape, name).ok_or("a note takes its color on its own line")?,
            Some(at),
        ),
        None => (format!("class {}", written(name)), None),
    };
    let first = first_line(code, name, at)?;
    Ok(before(code, &first, &[format!("{line} {color}")]))
}

pub(super) fn participant(code: &str, name: &str, color: Option<&str>) -> Result<Edit, String> {
    if let Some(done) = on_declaration(code, SEQUENCE, false, name, color) {
        return done;
    }
    let color = color.ok_or_else(|| no_color(name))?;
    let Ok(Diagram::Sequence(diagram)) = omaspec_uml::parse(code) else {
        let first = first_line(code, name, None)?;
        return Ok(before(
            code,
            &first,
            &[format!("participant {} {color}", written(name))],
        ));
    };
    let wanted = key(name);
    let index = diagram
        .participants
        .iter()
        .position(|p| key(&p.code) == wanted || key(&p.label) == wanted)
        .ok_or_else(|| format!("{name} is no participant"))?;
    let me = &diagram.participants[index];
    // Participants stand in the order they are first written: those the
    // same line names before this one are declared before it too.
    let mut lines: Vec<String> = diagram.participants[..index]
        .iter()
        .filter(|p| {
            p.line == me.line && declaration(code, SEQUENCE, false, &key(&p.code)).is_none()
        })
        .map(|p| format!("participant {}", written(&p.code)))
        .collect();
    lines.push(format!("participant {} {color}", written(&me.code)));
    let first = first_line(code, name, Some(me.line))?;
    Ok(before(code, &first, &lines))
}

/// The participant the caret is on: its name anywhere, or the line that
/// declares it.
pub(super) fn participant_at(code: &str, row: usize, col: usize) -> Option<Target> {
    let line = code.split('\n').nth(row)?;
    let on_line = || {
        declared(line, SEQUENCE, false)
            .and_then(|decl| decl.names.last().map(|name| (*name).to_owned()))
    };
    let Ok(Diagram::Sequence(diagram)) = omaspec_uml::parse(code) else {
        return on_line().map(Target::Named);
    };
    let names = diagram
        .participants
        .iter()
        .flat_map(|p| [p.code.as_str(), p.label.as_str()]);
    let name = covering(line, col, names)
        .map(str::to_owned)
        .or_else(on_line)?;
    diagram
        .participants
        .iter()
        .find(|p| p.code == name || p.label == name)
        .map(|p| Target::Named(p.code.clone()))
}

/// The participant of a timing diagram the caret is on.
pub(super) fn row_at(code: &str, row: usize, col: usize) -> Option<Target> {
    let line = code.split('\n').nth(row)?;
    let Ok(Diagram::Timing(diagram)) = omaspec_uml::parse(code) else {
        return None;
    };
    let names = diagram
        .rows
        .iter()
        .flat_map(|r| [r.key.as_str(), r.label.as_str()]);
    let found = match covering(line, col, names) {
        Some(name) => diagram
            .rows
            .iter()
            .find(|r| r.key == name || r.label == name),
        None => diagram.rows.iter().find(|r| r.line == row),
    };
    found.map(|r| Target::Named(r.key.clone()))
}

/// A `#color` of its own in `line`, outside quotes.
fn color_token(line: &str) -> Option<Range<usize>> {
    let mut quoted = false;
    let mut before = ' ';
    for (at, c) in line.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == '#' && !quoted && before.is_whitespace() {
            let end = line[at..]
                .find(char::is_whitespace)
                .map_or(line.len(), |end| at + end);
            return Some(at..end);
        }
        before = c;
    }
    None
}

/// A timing diagram's participant takes its color at the end of the line
/// that declares it: `concise "Web" as W #f7768e`.
pub(super) fn row(code: &str, name: &str, color: Option<&str>) -> Result<Edit, String> {
    let Ok(Diagram::Timing(diagram)) = omaspec_uml::parse(code) else {
        return Err("this timing diagram is not read yet".into());
    };
    let wanted = key(name);
    let row = diagram
        .rows
        .iter()
        .find(|r| key(&r.key) == wanted || key(&r.label) == wanted)
        .ok_or_else(|| format!("{name} is no participant"))?;
    // PlantUML reads a color there on these alone.
    if color.is_some()
        && !matches!(
            row.kind,
            RowKind::Robust | RowKind::Concise | RowKind::Rectangle
        )
    {
        return Err(
            "PlantUML colors a robust or a concise participant, no clock, binary or analog one"
                .into(),
        );
    }
    let range = line_ranges(code)
        .get(row.line)
        .cloned()
        .ok_or_else(|| format!("{name} is not in the source"))?;
    let (line, newline) = split_newline(&code[range.clone()]);
    let new = match (color_token(line), color) {
        (Some(at), Some(color)) => Some(format!("{}{color}{}", &line[..at.start], &line[at.end..])),
        (Some(at), None) => Some(format!(
            "{}{}",
            line[..at.start].trim_end(),
            &line[at.end..]
        )),
        (None, Some(color)) => Some(format!("{} {color}", line.trim_end())),
        (None, None) => None,
    };
    changed(name, range, line, new, newline)
}
