//! Mermaid sequence diagrams read into the engine's sequence tree, every
//! item on its line, so that Mermaid's sequences are drawn and checked the
//! way PlantUML's are. Mermaid's arrow heads, activations and participant
//! shapes map onto PlantUML's.

use std::collections::HashMap;

use super::{
    Arrow, Diagram, End, Frame, GroupKind, Head, Item, Mark, Message, Note, NotePlace, NoteShape,
    ParticipantBox, Shape, Text,
};

#[derive(Clone, Copy)]
struct ArrowKind {
    dashed: bool,
    head: Head,
    tail: Head,
    end_mark: Mark,
    /// `A<-B` is `B->A`.
    reverse: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Life {
    None,
    /// `+`, on the arrow or the receiver's name, turns the receiver on.
    ActivateReceiver,
    /// `-` on the arrow turns the sender off (`B-->>-A`).
    DeactivateSender,
    /// `-` on the receiver's name turns that receiver off (`B-->>A-`).
    DeactivateReceiver,
}

enum Open {
    Frame(GroupKind),
    Box(Option<String>, Option<String>, Vec<usize>),
    /// `rect`, closed by `end`. No label.
    Rect,
}

/// `create` or `destroy` waits for the next message. Mermaid rejects any
/// other statement in between, and rejects a message that does not involve
/// that participant.
#[derive(Clone, Copy)]
enum Wait {
    /// The next message must arrive at this participant, and not leave it.
    Create(usize),
    /// The next message must send or receive as this participant. The
    /// cross is stored after that message, on the line of the `destroy`.
    Destroy(usize, usize),
}

enum Read {
    Handled,
    Waiting(Wait),
    /// Not a create, destroy, rect or declaration.
    Unhandled,
}

/// The sequence diagram a Mermaid `sequenceDiagram` describes, every item
/// on its line. `None` when the engine does not draw it: an `@{ }` that is
/// not a participant's `type` or `alias`, a color it cannot read, or a
/// line it does not know.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    let mut diagram = Diagram {
        participants: Vec::new(),
        items: Vec::new(),
        boxes: Vec::new(),
        footbox: false,
        hide_unlinked: false,
        frame: Frame::default(),
    };
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut open: Vec<Open> = Vec::new();
    let mut numbers: Option<(usize, usize)> = None;
    let mut saw_header = false;
    let mut pending: Option<Wait> = None;

    for (line, text) in statements(source) {
        let lower = text.to_ascii_lowercase();
        if lower == "sequencediagram" {
            saw_header = true;
            continue;
        }
        if !saw_header {
            return None;
        }
        if let Some(wait) = pending.take() {
            commit_wait(&mut diagram, &mut index, &mut numbers, line, &text, wait)?;
            continue;
        }
        match leading(&mut diagram, &mut index, &mut open, &text, line)? {
            Read::Handled => continue,
            Read::Waiting(wait) => {
                pending = Some(wait);
                continue;
            }
            Read::Unhandled => {}
        }
        if let Some(rest) = starts(&text, "title") {
            diagram.frame.title = Some(words(line, rest));
            continue;
        }
        if lower.starts_with("acctitle") || lower.starts_with("accdescr") {
            continue;
        }
        if starts(&lower, "link").is_some() || starts(&lower, "links").is_some() {
            continue;
        }
        if let Some(rest) = starts(&text, "box") {
            if open.iter().any(|item| matches!(item, Open::Box(_, _, _))) {
                return None;
            }
            let (color, label) = box_title(rest)?;
            open.push(Open::Box(color, label, Vec::new()));
            continue;
        }
        if let Some((kind, rest)) = frame(&text) {
            open.push(Open::Frame(kind));
            diagram.items.push(Item::GroupStart {
                kind,
                label: some_words(line, rest),
                secondary: None,
                line,
            });
            continue;
        }
        if let Some(rest) = starts(&text, "else") {
            diagram.items.push(Item::GroupElse {
                label: some_words(line, rest),
                line,
            });
            continue;
        }
        if let Some(rest) = starts(&text, "and") {
            if !open
                .iter()
                .any(|item| matches!(item, Open::Frame(GroupKind::Par)))
            {
                return None;
            }
            diagram.items.push(Item::GroupElse {
                label: some_words(line, rest),
                line,
            });
            continue;
        }
        if let Some(rest) = starts(&text, "option") {
            if !open
                .iter()
                .any(|item| matches!(item, Open::Frame(GroupKind::Critical)))
            {
                return None;
            }
            diagram.items.push(Item::GroupElse {
                label: some_words(line, rest),
                line,
            });
            continue;
        }
        if lower == "end" {
            match open.pop() {
                Some(Open::Frame(_)) => diagram.items.push(Item::GroupEnd { line }),
                Some(Open::Box(color, label, participants)) => {
                    diagram.boxes.push(ParticipantBox {
                        title: label.map(|value| Text { value, line }),
                        color,
                        line,
                        participants,
                    });
                }
                Some(Open::Rect) => diagram.items.push(Item::RectEnd { line }),
                None => return None,
            }
            continue;
        }
        if let Some(note) = note(line, &text, &mut diagram, &mut index) {
            diagram.items.push(Item::Note(note?));
            continue;
        }
        if lower.starts_with("note ") {
            return None;
        }
        if let Some(rest) = starts(&text, "activate") {
            let code = unquote(rest);
            let at = ensure(
                &mut diagram,
                &mut index,
                &code,
                &code,
                Shape::Participant,
                line,
            );
            diagram.items.push(Item::Activate {
                participant: at,
                color: None,
                line,
            });
            continue;
        }
        if let Some(rest) = starts(&text, "deactivate") {
            let code = unquote(rest);
            let at = ensure(
                &mut diagram,
                &mut index,
                &code,
                &code,
                Shape::Participant,
                line,
            );
            diagram.items.push(Item::Deactivate {
                participant: at,
                line,
            });
            continue;
        }
        if let Some(rest) = starts(&text, "autonumber") {
            numbers = autonumber(rest)?;
            continue;
        }
        if let Some(message) = message(line, &text, &mut diagram, &mut index, &mut numbers) {
            let (message, life) = message?;
            push_message(&mut diagram, line, message, life, None);
            continue;
        }
        return None;
    }
    if pending.is_some() || !open.is_empty() {
        return None;
    }
    Some(diagram)
}

/// `create`, `destroy`, `rect` and a participant declaration. An `@{ }`
/// that is not a closed `type`/`alias` on one of those declines the diagram.
fn leading(
    diagram: &mut Diagram,
    index: &mut HashMap<String, usize>,
    open: &mut Vec<Open>,
    text: &str,
    line: usize,
) -> Option<Read> {
    if let Some(rest) = starts(text, "create") {
        return Some(Read::Waiting(read_create(
            diagram, index, open, rest, line,
        )?));
    }
    if let Some(rest) = starts(text, "destroy") {
        return Some(Read::Waiting(read_destroy(
            diagram, index, open, rest, line,
        )?));
    }
    if let Some(rest) = starts(text, "rect") {
        read_rect(diagram, open, rest, line)?;
        return Some(Read::Handled);
    }
    if let Some((shape, rest)) = declaration(text) {
        read_participant(diagram, index, open, shape, rest, line)?;
        return Some(Read::Handled);
    }
    if text.contains("@{") {
        return None;
    }
    Some(Read::Unhandled)
}

fn read_create(
    diagram: &mut Diagram,
    index: &mut HashMap<String, usize>,
    open: &mut [Open],
    rest: &str,
    line: usize,
) -> Option<Wait> {
    // Mermaid's `create` takes `participant` or `actor` only. A bare
    // `create B` is not a statement.
    let (shape, rest) = declaration(rest)?;
    if !matches!(shape, Shape::Participant | Shape::Actor) {
        return None;
    }
    let declared = declared(shape, rest)?;
    if index.contains_key(&declared.code) {
        return None;
    }
    let at = place(diagram, index, open, &declared, line);
    Some(Wait::Create(at))
}

fn read_destroy(
    diagram: &mut Diagram,
    index: &mut HashMap<String, usize>,
    open: &mut [Open],
    rest: &str,
    line: usize,
) -> Option<Wait> {
    if rest.contains("@{") || rest.split_whitespace().count() != 1 {
        return None;
    }
    let code = unquote(rest);
    if code.is_empty() {
        return None;
    }
    let fresh = !index.contains_key(&code);
    let at = ensure(diagram, index, &code, &code, Shape::Participant, line);
    if fresh && let Some(Open::Box(_, _, members)) = open.last_mut() {
        members.push(at);
    }
    Some(Wait::Destroy(at, line))
}

fn read_rect(diagram: &mut Diagram, open: &mut Vec<Open>, rest: &str, line: usize) -> Option<()> {
    let color = rest.trim();
    if color.is_empty() || crate::color::parse(color).is_err() {
        return None;
    }
    open.push(Open::Rect);
    diagram.items.push(Item::RectStart {
        color: color.to_owned(),
        line,
    });
    Some(())
}

fn read_participant(
    diagram: &mut Diagram,
    index: &mut HashMap<String, usize>,
    open: &mut [Open],
    shape: Shape,
    rest: &str,
    line: usize,
) -> Option<()> {
    let declared = declared(shape, rest)?;
    place(diagram, index, open, &declared, line);
    Some(())
}

struct Declared {
    code: String,
    label: String,
    shape: Shape,
}

/// A participant's name, its `as` label, and a `@{ type, alias }` in front
/// of `as`. `None` when the braces are there but are not that annotation.
fn declared(shape: Shape, rest: &str) -> Option<Declared> {
    let (rest, anno) = split_config(rest)?;
    let (code, mut label) = name_and_label(&rest)?;
    if code.is_empty() {
        return None;
    }
    let shape = apply_anno(shape, &mut label, &code, anno);
    Some(Declared { code, label, shape })
}

fn place(
    diagram: &mut Diagram,
    index: &mut HashMap<String, usize>,
    open: &mut [Open],
    declared: &Declared,
    line: usize,
) -> usize {
    let at = ensure(
        diagram,
        index,
        &declared.code,
        &declared.label,
        declared.shape,
        line,
    );
    if let Some(Open::Box(_, _, members)) = open.last_mut() {
        members.push(at);
    }
    at
}

struct Anno {
    shape: Option<Shape>,
    alias: Option<String>,
}

/// Splits `@{ ... }` out of a declaration. The brace may sit before `as`.
fn split_config(rest: &str) -> Option<(String, Anno)> {
    let rest = rest.trim();
    let Some(at) = find_marker(rest, "@{") else {
        return Some((
            rest.to_owned(),
            Anno {
                shape: None,
                alias: None,
            },
        ));
    };
    let before = rest[..at].trim();
    let after = &rest[at + 2..];
    let end = after.find('}')?;
    let body = &after[..end];
    let tail = after[end + 1..].trim();
    if before.contains("@{") || tail.contains("@{") {
        return None;
    }
    let anno = anno(body)?;
    let joined = match (before.is_empty(), tail.is_empty()) {
        (true, true) => String::new(),
        (false, true) => before.to_owned(),
        (true, false) => tail.to_owned(),
        (false, false) => format!("{before} {tail}"),
    };
    Some((joined, anno))
}

/// `type` and `alias` only. Anything else, including an unclosed brace the
/// caller already rejected, keeps the diagram on mmdr.
fn anno(body: &str) -> Option<Anno> {
    let mut anno = Anno {
        shape: None,
        alias: None,
    };
    let body = body.trim();
    if body.is_empty() {
        return Some(anno);
    }
    for part in comma_parts(body) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) = part.split_once(':')?;
        let key = unquote(key.trim());
        let value = unquote(value.trim());
        if value.is_empty() {
            return None;
        }
        match key.to_ascii_lowercase().as_str() {
            "type" => anno.shape = Some(shape_named(&value)?),
            "alias" => anno.alias = Some(value),
            _ => return None,
        }
    }
    Some(anno)
}

fn apply_anno(shape: Shape, label: &mut String, code: &str, anno: Anno) -> Shape {
    let shape = anno.shape.unwrap_or(shape);
    // Mermaid uses `alias` only when the line has no `as` label.
    if let Some(alias) = anno.alias
        && label == code
    {
        *label = alias;
    }
    shape
}

fn shape_named(name: &str) -> Option<Shape> {
    match name.to_ascii_lowercase().as_str() {
        "actor" => Some(Shape::Actor),
        "participant" => Some(Shape::Participant),
        "boundary" => Some(Shape::Boundary),
        "control" => Some(Shape::Control),
        "entity" => Some(Shape::Entity),
        "database" => Some(Shape::Database),
        "collections" => Some(Shape::Collections),
        "queue" => Some(Shape::Queue),
        _ => None,
    }
}

fn comma_parts(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quote = None;
    let mut start = 0;
    for (at, ch) in body.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == ',' {
            out.push(&body[start..at]);
            start = at + 1;
        }
    }
    out.push(&body[start..]);
    out
}

fn find_marker(hay: &str, needle: &str) -> Option<usize> {
    let mut quote = None;
    for (at, ch) in hay.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        let Some(slice) = hay.get(at..at + needle.len()) else {
            continue;
        };
        if slice == needle {
            return Some(at);
        }
    }
    None
}

/// The message a `create` or `destroy` was waiting for. The cross is
/// pushed after the message (and after its `+` / `-`) so the drawer, which
/// marks the last arrow, puts it on that message. Mermaid writes `destroy`
/// before the message; the item order is the drawing order.
fn commit_wait(
    diagram: &mut Diagram,
    index: &mut HashMap<String, usize>,
    numbers: &mut Option<(usize, usize)>,
    line: usize,
    text: &str,
    wait: Wait,
) -> Option<()> {
    if text.contains("@{") {
        return None;
    }
    let (mut message, life) = message(line, text, diagram, index, numbers)??;
    let destroy = match wait {
        Wait::Create(who) => {
            if message.to != End::Participant(who) || message.from == End::Participant(who) {
                return None;
            }
            message.creates = true;
            None
        }
        Wait::Destroy(who, destroy_line) => {
            let end = End::Participant(who);
            if message.from != end && message.to != end {
                return None;
            }
            Some((who, destroy_line))
        }
    };
    push_message(diagram, line, message, life, destroy);
    Some(())
}

fn push_message(
    diagram: &mut Diagram,
    line: usize,
    message: Message,
    life: Life,
    destroy: Option<(usize, usize)>,
) {
    let sender = message.from;
    let receiver = message.to;
    diagram.items.push(Item::Message(message));
    let who = match life {
        Life::None => None,
        Life::ActivateReceiver => Some((true, receiver)),
        Life::DeactivateSender => Some((false, sender)),
        Life::DeactivateReceiver => Some((false, receiver)),
    };
    if let Some((on, End::Participant(participant))) = who {
        if on {
            diagram.items.push(Item::Activate {
                participant,
                color: None,
                line,
            });
        } else {
            diagram.items.push(Item::Deactivate { participant, line });
        }
    }
    if let Some((participant, line)) = destroy {
        diagram.items.push(Item::Destroy { participant, line });
    }
}

fn message(
    line: usize,
    text: &str,
    diagram: &mut Diagram,
    index: &mut HashMap<String, usize>,
    numbers: &mut Option<(usize, usize)>,
) -> Option<Option<(Message, Life)>> {
    let (from, to, label, kind, life) = arrow(text)?;
    if from.is_empty() || to.is_empty() {
        return Some(None);
    }
    let from = ensure(diagram, index, &from, &from, Shape::Participant, line);
    let to = ensure(diagram, index, &to, &to, Shape::Participant, line);
    let number = numbers.as_mut().map(|(next, step)| {
        let shown = next.to_string();
        *next = next.saturating_add(*step);
        shown
    });
    Some(Some((
        Message {
            from: End::Participant(from),
            to: End::Participant(to),
            arrow: Arrow {
                dashed: kind.dashed,
                head: kind.head,
                tail: kind.tail,
                end_mark: kind.end_mark,
                start_mark: Mark::None,
                color: None,
                bold: false,
            },
            text: label.map(|value| words(line, &value)),
            number,
            creates: false,
            line,
        },
        life,
    )))
}

fn arrow(text: &str) -> Option<(String, String, Option<String>, ArrowKind, Life)> {
    let mut quote = None;
    for (at, ch) in text.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        let Some((used, kind, life)) = arrow_token(&text[at..]) else {
            continue;
        };
        let from = unquote(text[..at].trim());
        let rest = text[at + used..].trim();
        let (to, label) = split_label(rest);
        let mut to = unquote(to.trim());
        let mut from = from;
        if kind.reverse {
            std::mem::swap(&mut from, &mut to);
        }
        let (to, life) = name_life(to, life);
        if from.is_empty() || to.is_empty() {
            continue;
        }
        return Some((from, to, label, kind, life));
    }
    None
}

/// `Bob+` and `Alice-` are the same `+` / `-` written after the name.
fn name_life(to: String, life: Life) -> (String, Life) {
    if life != Life::None {
        return (to, life);
    }
    if let Some(name) = to.strip_suffix('+').map(str::trim)
        && !name.is_empty()
    {
        return (name.to_owned(), Life::ActivateReceiver);
    }
    if let Some(name) = to.strip_suffix('-').map(str::trim)
        && !name.is_empty()
    {
        return (name.to_owned(), Life::DeactivateReceiver);
    }
    (to, life)
}

fn arrow_token(rest: &str) -> Option<(usize, ArrowKind, Life)> {
    const TABLE: &[(&str, ArrowKind)] = &[
        (
            "<<-->>",
            ArrowKind {
                dashed: true,
                head: Head::Filled,
                tail: Head::Filled,
                end_mark: Mark::None,
                reverse: false,
            },
        ),
        (
            "<<->>",
            ArrowKind {
                dashed: false,
                head: Head::Filled,
                tail: Head::Filled,
                end_mark: Mark::None,
                reverse: false,
            },
        ),
        (
            "-->>",
            ArrowKind {
                dashed: true,
                head: Head::Filled,
                tail: Head::None,
                end_mark: Mark::None,
                reverse: false,
            },
        ),
        (
            "-->",
            ArrowKind {
                dashed: true,
                head: Head::None,
                tail: Head::None,
                end_mark: Mark::None,
                reverse: false,
            },
        ),
        (
            "--)",
            ArrowKind {
                dashed: true,
                head: Head::Open,
                tail: Head::None,
                end_mark: Mark::None,
                reverse: false,
            },
        ),
        (
            "--x",
            ArrowKind {
                dashed: true,
                head: Head::None,
                tail: Head::None,
                end_mark: Mark::Cross,
                reverse: false,
            },
        ),
        (
            "->>",
            ArrowKind {
                dashed: false,
                head: Head::Filled,
                tail: Head::None,
                end_mark: Mark::None,
                reverse: false,
            },
        ),
        (
            "->",
            ArrowKind {
                dashed: false,
                head: Head::None,
                tail: Head::None,
                end_mark: Mark::None,
                reverse: false,
            },
        ),
        (
            "-)",
            ArrowKind {
                dashed: false,
                head: Head::Open,
                tail: Head::None,
                end_mark: Mark::None,
                reverse: false,
            },
        ),
        (
            "-x",
            ArrowKind {
                dashed: false,
                head: Head::None,
                tail: Head::None,
                end_mark: Mark::Cross,
                reverse: false,
            },
        ),
        (
            "<--",
            ArrowKind {
                dashed: true,
                head: Head::None,
                tail: Head::None,
                end_mark: Mark::None,
                reverse: true,
            },
        ),
        (
            "<-",
            ArrowKind {
                dashed: false,
                head: Head::None,
                tail: Head::None,
                end_mark: Mark::None,
                reverse: true,
            },
        ),
    ];
    for (token, kind) in TABLE {
        let Some(after) = rest.strip_prefix(token) else {
            continue;
        };
        let (life, extra) = if after.starts_with('+') {
            (Life::ActivateReceiver, 1)
        } else if after.starts_with('-') {
            (Life::DeactivateSender, 1)
        } else {
            (Life::None, 0)
        };
        return Some((token.len() + extra, *kind, life));
    }
    None
}

fn split_label(rest: &str) -> (String, Option<String>) {
    let mut quote = None;
    for (at, ch) in rest.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == ':' {
            let label = rest[at + 1..].trim();
            let label = if label.is_empty() {
                None
            } else {
                Some(breaks(label))
            };
            return (rest[..at].to_owned(), label);
        }
    }
    (rest.to_owned(), None)
}

fn note(
    line: usize,
    text: &str,
    diagram: &mut Diagram,
    index: &mut HashMap<String, usize>,
) -> Option<Option<Note>> {
    let rest = starts(text, "note")?;
    let lower = rest.to_ascii_lowercase();
    let (place, targets) = if let Some(targets) = starts(rest, "left of") {
        (SideNote::Left, targets)
    } else if let Some(targets) = starts(rest, "right of") {
        (SideNote::Right, targets)
    } else if let Some(targets) = starts(rest, "over") {
        (SideNote::Over, targets)
    } else if lower.starts_with("left of")
        || lower.starts_with("right of")
        || lower.starts_with("over")
    {
        return Some(None);
    } else {
        return None;
    };
    let (targets, label) = split_label(targets);
    let label = label?;
    let names: Vec<String> = targets
        .split(',')
        .map(|part| unquote(part.trim()))
        .filter(|part| !part.is_empty())
        .collect();
    if names.is_empty() {
        return Some(None);
    }
    let mut ids = Vec::new();
    for name in &names {
        ids.push(ensure(diagram, index, name, name, Shape::Participant, line));
    }
    let place = match place {
        SideNote::Left => NotePlace::LeftOf(ids[0]),
        SideNote::Right => NotePlace::RightOf(ids[0]),
        SideNote::Over => {
            let (lo, hi) = ids
                .iter()
                .fold((usize::MAX, 0), |(lo, hi), id| (lo.min(*id), hi.max(*id)));
            NotePlace::Over(lo, hi)
        }
    };
    Some(Some(Note {
        shape: NoteShape::Rectangle,
        place,
        text: words(line, &label),
        color: None,
        line,
    }))
}

enum SideNote {
    Left,
    Right,
    Over,
}

fn declaration(text: &str) -> Option<(Shape, &str)> {
    const WORDS: &[(&str, Shape)] = &[
        ("participant", Shape::Participant),
        ("database", Shape::Database),
        ("boundary", Shape::Boundary),
        ("control", Shape::Control),
        ("entity", Shape::Entity),
        ("actor", Shape::Actor),
    ];
    for (word, shape) in WORDS {
        if let Some(rest) = starts(text, word) {
            return Some((*shape, rest));
        }
    }
    None
}

fn frame(text: &str) -> Option<(GroupKind, &str)> {
    const WORDS: &[(&str, GroupKind)] = &[
        ("critical", GroupKind::Critical),
        ("break", GroupKind::Break),
        ("loop", GroupKind::Loop),
        ("alt", GroupKind::Alt),
        ("opt", GroupKind::Opt),
        ("par", GroupKind::Par),
    ];
    for (word, kind) in WORDS {
        if let Some(rest) = starts(text, word) {
            return Some((*kind, rest));
        }
    }
    None
}

fn box_title(rest: &str) -> Option<(Option<String>, Option<String>)> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Some((None, None));
    }
    // `rgba(0, 0, 255, .1) Title` is one color through the closing `)`,
    // then the title. A split on the first space would keep `rgba(0,`.
    let (first, label) = split_leading(rest);
    if first.eq_ignore_ascii_case("transparent") {
        return Some((None, (!label.is_empty()).then(|| label.to_owned())));
    }
    if label.is_empty() {
        if looks_colored(first) && crate::color::parse(first).is_err() {
            return None;
        }
        if crate::color::parse(first).is_ok() && looks_colored(first) {
            return Some((Some(first.to_owned()), None));
        }
        return Some((None, Some(rest.to_owned())));
    }
    if crate::color::parse(first).is_ok() {
        return Some((Some(first.to_owned()), Some(label.to_owned())));
    }
    if looks_colored(first) {
        return None;
    }
    Some((None, Some(rest.to_owned())))
}

/// The leading color and the text after it. A function color includes its
/// arguments, spaces and all, through the closing `)`.
fn split_leading(rest: &str) -> (&str, &str) {
    let lower = rest.to_ascii_lowercase();
    for prefix in ["rgba(", "hsla(", "rgb(", "hsl("] {
        if lower.starts_with(prefix)
            && let Some(end) = rest.find(')')
        {
            return (&rest[..=end], rest[end + 1..].trim());
        }
    }
    match rest.split_once(char::is_whitespace) {
        Some((first, label)) => (first, label.trim()),
        None => (rest, ""),
    }
}

fn looks_colored(token: &str) -> bool {
    let lower = token.trim().to_ascii_lowercase();
    lower.starts_with('#')
        || lower.starts_with("rgb(")
        || lower.starts_with("rgba(")
        || lower.starts_with("hsl(")
        || lower.starts_with("hsla(")
}

fn autonumber(rest: &str) -> Option<Option<(usize, usize)>> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Some(Some((1, 1)));
    }
    let mut parts = rest.split_whitespace();
    let first = parts.next()?;
    if first.eq_ignore_ascii_case("off") && parts.next().is_none() {
        return Some(None);
    }
    let start: usize = first.parse().ok()?;
    let step = match parts.next() {
        None => 1,
        Some(step) => step.parse().ok()?,
    };
    if parts.next().is_some() || step == 0 {
        return None;
    }
    Some(Some((start, step)))
}

fn ensure(
    diagram: &mut Diagram,
    index: &mut HashMap<String, usize>,
    code: &str,
    label: &str,
    shape: Shape,
    line: usize,
) -> usize {
    if let Some(at) = index.get(code).copied() {
        if diagram.participants[at].label == diagram.participants[at].code && label != code {
            diagram.participants[at].label = label.to_owned();
        }
        if shape != Shape::Participant {
            diagram.participants[at].shape = shape;
        }
        return at;
    }
    let at = diagram.participants.len();
    index.insert(code.to_owned(), at);
    diagram.participants.push(super::Participant {
        code: code.to_owned(),
        label: label.to_owned(),
        shape,
        stereotype: None,
        spot: None,
        color: None,
        line,
    });
    at
}

fn name_and_label(rest: &str) -> Option<(String, String)> {
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    if let Some(at) = find_ci(rest, " as ") {
        let code = unquote(rest[..at].trim());
        let label = unquote(rest[at + 4..].trim());
        if code.is_empty() || label.is_empty() {
            return None;
        }
        return Some((code, label));
    }
    let code = unquote(rest);
    if code.is_empty() {
        return None;
    }
    Some((code.clone(), code))
}

fn starts<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.get(word.len()..)?;
    if !text[..word.len()].eq_ignore_ascii_case(word) {
        return None;
    }
    if rest.is_empty() {
        return Some("");
    }
    let mut chars = rest.chars();
    if chars.next().is_some_and(char::is_whitespace) {
        return Some(rest.trim());
    }
    None
}

fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let mut quote = None;
    for (at, ch) in hay.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        let Some(slice) = hay.get(at..at + needle.len()) else {
            continue;
        };
        if slice.eq_ignore_ascii_case(needle) {
            return Some(at);
        }
    }
    None
}

fn unquote(text: &str) -> String {
    let text = text.trim();
    let bytes = text.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        return text[1..text.len() - 1].to_owned();
    }
    text.to_owned()
}

fn words(line: usize, text: &str) -> Text {
    Text {
        value: breaks(text),
        line,
    }
}

fn some_words(line: usize, text: &str) -> Option<Text> {
    let text = text.trim();
    (!text.is_empty()).then(|| words(line, text))
}

/// `<br>` becomes a new line. The sequence engine draws `\n` and a real break.
fn breaks(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &lower[at..];
        let size = if rest.starts_with("<br/>") {
            5
        } else if rest.starts_with("<br />") {
            6
        } else if rest.starts_with("<br>") {
            4
        } else {
            0
        };
        if size > 0 {
            out.push('\n');
            at += size;
            continue;
        }
        let ch = text[at..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        at += ch.len_utf8();
    }
    out
}

/// The first construct [`from_mermaid`] refuses by name, and its line:
/// an `@{ }` that is not a participant's `type` or `alias`, or a color it
/// cannot read. Other refusals stay unnamed. A diagram [`from_mermaid`]
/// accepts is not named here.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    if from_mermaid(source).is_some() {
        return None;
    }
    let mut saw_header = false;
    for (line, text) in statements(source) {
        let lower = text.to_ascii_lowercase();
        if lower == "sequencediagram" {
            saw_header = true;
            continue;
        }
        if !saw_header {
            continue;
        }
        if text.contains("@{") && !closed_participant_config(&text) {
            return Some((line, "a @{ } annotation"));
        }
        if let Some(rest) = starts(&lower, "rect")
            && crate::color::parse(rest).is_err()
        {
            return Some((line, "a color"));
        }
        if let Some(rest) = starts(&text, "box")
            && box_title(rest).is_none()
        {
            return Some((line, "a color"));
        }
    }
    None
}

/// Whether `@{ }` on this line is a closed participant annotation.
fn closed_participant_config(text: &str) -> bool {
    let rest = if let Some(rest) = starts(text, "create") {
        let Some((_, rest)) = declaration(rest) else {
            return false;
        };
        rest
    } else if let Some((_, rest)) = declaration(text) {
        rest
    } else {
        return false;
    };
    split_config(rest).is_some()
}

fn statements(source: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut front = false;
    for (line, raw) in source.lines().enumerate() {
        let text = raw.trim();
        if text.is_empty() {
            continue;
        }
        if text == "---" {
            front = !front;
            continue;
        }
        if front || text.starts_with("%%") {
            continue;
        }
        let text = cut_comment(text);
        if !text.is_empty() {
            out.push((line, text));
        }
    }
    out
}

fn cut_comment(text: &str) -> String {
    let mut quote = None;
    let mut chars = text.chars().peekable();
    let mut out = String::new();
    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            out.push(ch);
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            out.push(ch);
            continue;
        }
        if ch == '%' && chars.peek() == Some(&'%') {
            break;
        }
        out.push(ch);
    }
    out.trim().to_owned()
}
