//! From the body's lines to a [`Diagram`]. A line the parser does not
//! understand declines the whole diagram, naming that line.

use super::{
    Arrow, Diagram, End, GroupKind, Head, Item, Mark, Message, Note, NotePlace, NoteShape,
    Participant, ParticipantBox, Shape, Side, Spot, Text,
};
use crate::Declined;
use crate::common::{Frame, block, keyword, snippet, squeezed_eq, text_at};
use crate::source::{Line, starts_with_ci};

/// Parses a sequence diagram from the lines [`crate::source::body`] gives.
pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut parser = Parser::default();
    let mut at = 0;
    while at < lines.len() {
        at = parser.line(lines, at)?;
    }
    parser.finish()
}

const SHAPES: [(&str, Shape); 8] = [
    ("participant", Shape::Participant),
    ("actor", Shape::Actor),
    ("boundary", Shape::Boundary),
    ("control", Shape::Control),
    ("entity", Shape::Entity),
    ("database", Shape::Database),
    ("collections", Shape::Collections),
    ("queue", Shape::Queue),
];

const GROUPS: [(&str, GroupKind); 7] = [
    ("alt", GroupKind::Alt),
    ("opt", GroupKind::Opt),
    ("loop", GroupKind::Loop),
    ("par", GroupKind::Par),
    ("break", GroupKind::Break),
    ("critical", GroupKind::Critical),
    ("group", GroupKind::Group),
];

/// Keywords of PlantUML that Omagram does not draw yet, with what to say.
const NOT_YET: [&str; 5] = [
    "newpage",
    "autoactivate",
    "skin",
    "left to right direction",
    "top to bottom direction",
];

struct Parser {
    diagram: Diagram,
    frame: Frame,
    /// Whether a participant appeared in the diagram's items yet.
    used: Vec<bool>,
    /// Open groups: kind and line.
    groups: Vec<(GroupKind, usize)>,
    /// The open `box`, as an index into the diagram's boxes.
    open_box: Option<usize>,
    autonumber: Autonumber,
    /// Active participants, newest last, with the end whose message
    /// activated them: where `return` goes.
    activations: Vec<(usize, Option<End>)>,
    /// The ends of the last message.
    last_message: Option<(End, End)>,
    /// The participant a `create` made, and its line: like PlantUML, the
    /// next message must go to it, and its arrow draws the head.
    pending_create: Option<(usize, usize)>,
    /// A delay came after the last message: PlantUML takes no `activate`,
    /// `deactivate`, `create` or `destroy` until the next one.
    after_delay: bool,
    /// The participants from left to right: declaring one again moves it
    /// to the end, as in PlantUML.
    order: Vec<usize>,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            diagram: Diagram {
                participants: Vec::new(),
                items: Vec::new(),
                boxes: Vec::new(),
                footbox: true,
                hide_unlinked: false,
                frame: Frame::default(),
            },
            frame: Frame::styled(super::skin::STYLE),
            used: Vec::new(),
            groups: Vec::new(),
            open_box: None,
            autonumber: Autonumber::default(),
            activations: Vec::new(),
            last_message: None,
            pending_create: None,
            after_delay: false,
            order: Vec::new(),
        }
    }
}

struct Autonumber {
    on: bool,
    next: i64,
    step: i64,
    format: Option<String>,
}

impl Default for Autonumber {
    fn default() -> Self {
        Self {
            on: false,
            next: 1,
            step: 1,
            format: None,
        }
    }
}

/// What a message does to a life line, after its second end.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    /// `++`: the receiver.
    Activate,
    /// `--`: the sender.
    Deactivate,
    /// `**`: the receiver, before the message.
    Create,
    /// `!!`: the receiver.
    Destroy,
}

/// The six forms PlantUML takes after a message, longest first; the steps
/// run in the order they are written.
const LIFECYCLES: [(&str, &[Step]); 6] = [
    ("++--", &[Step::Activate, Step::Deactivate]),
    ("--++", &[Step::Deactivate, Step::Activate]),
    ("++", &[Step::Activate]),
    ("--", &[Step::Deactivate]),
    ("**", &[Step::Create]),
    ("!!", &[Step::Destroy]),
];

#[derive(Default)]
struct Lifecycle {
    steps: &'static [Step],
    /// For the activation; PlantUML ignores it without `++`.
    color: Option<String>,
}

impl Parser {
    /// Parses the line at `at` (and the lines a block takes); returns the
    /// index of the next line.
    fn line(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line { no, text, .. } = lines[at];
        let squeezed = text
            .split_whitespace()
            .collect::<String>()
            .to_ascii_lowercase();

        if let Some(next) = self.frame.line(lines, at)? {
            return Ok(next);
        }
        match squeezed.as_str() {
            "hidefootbox" => {
                self.diagram.footbox = false;
                return Ok(at + 1);
            }
            "showfootbox" => {
                self.diagram.footbox = true;
                return Ok(at + 1);
            }
            "hideunlinked" => {
                self.diagram.hide_unlinked = true;
                return Ok(at + 1);
            }
            "end" => {
                if self.groups.pop().is_none() {
                    return Err(Declined::at(no, "`end` without an open group"));
                }
                self.diagram.items.push(Item::GroupEnd { line: no });
                return Ok(at + 1);
            }
            "endbox" => {
                if self.open_box.take().is_none() {
                    return Err(Declined::at(no, "`end box` without a `box`"));
                }
                return Ok(at + 1);
            }
            _ => {}
        }
        if keyword(text, "hide").is_some() || keyword(text, "show").is_some() {
            return Err(Declined::at(
                no,
                format!("`{}` is not supported yet", snippet(text)),
            ));
        }
        if let Some(rest) = keyword(text, "autonumber") {
            self.autonumber(rest, no)?;
            return Ok(at + 1);
        }
        if let Some((shape, rest)) = shape_keyword(text) {
            self.declare(shape, rest, no)?;
            return Ok(at + 1);
        }
        if let Some(rest) = keyword(text, "create") {
            self.not_after_delay("create", no)?;
            return self.create(rest, no).map(|()| at + 1);
        }
        if let Some(rest) = keyword(text, "activate") {
            self.not_after_delay("activate", no)?;
            let (code, rest) =
                name(rest).ok_or_else(|| Declined::at(no, "`activate` needs a participant"))?;
            let color = color_only(rest.trim(), no)?;
            let participant = self.participant(&code, no);
            self.activate(participant, color, no);
            return Ok(at + 1);
        }
        if let Some(rest) = keyword(text, "deactivate") {
            self.not_after_delay("deactivate", no)?;
            let participant = match name(rest) {
                Some((code, tail)) if tail.trim().is_empty() => self.participant(&code, no),
                Some(_) => {
                    return Err(Declined::at(no, "`deactivate` takes one participant"));
                }
                None => match self.activations.last() {
                    Some(&(participant, _)) => participant,
                    None => return Err(Declined::at(no, "nothing to deactivate")),
                },
            };
            self.deactivate(participant, no);
            return Ok(at + 1);
        }
        if let Some(rest) = keyword(text, "destroy") {
            self.not_after_delay("destroy", no)?;
            let (code, tail) =
                name(rest).ok_or_else(|| Declined::at(no, "`destroy` needs a participant"))?;
            if !tail.trim().is_empty() {
                return Err(Declined::at(no, "`destroy` takes one participant"));
            }
            let participant = self.participant(&code, no);
            self.diagram.items.push(Item::Destroy {
                participant,
                line: no,
            });
            return Ok(at + 1);
        }
        // `return ok` is the reply. `return o<-k` is a message: PlantUML's
        // arrow matches before the `return` command, so `return` and `k`
        // are participants and the arrow points at `return`.
        if let Some(rest) = keyword(text, "return")
            && split_message(text).is_none()
        {
            self.give_back(rest, no)?;
            return Ok(at + 1);
        }
        if let Some(next) = self.note(lines, at)? {
            return Ok(next);
        }
        if let Some(rest) = keyword(text, "ref") {
            return self.reference(lines, at, rest);
        }
        if let Some(rest) = keyword(text, "box") {
            self.open_box(rest, no)?;
            return Ok(at + 1);
        }
        if self.group(text, no)? {
            return Ok(at + 1);
        }
        if text.starts_with("==") {
            if text.len() < 4 || !text.ends_with("==") {
                return Err(Declined::at(no, "a divider is written `== text ==`"));
            }
            let label = text.trim_matches('=').trim();
            self.diagram.items.push(Item::Divider {
                label: text_at(label, no),
            });
            return Ok(at + 1);
        }
        if text.starts_with("...") {
            let label = if text.chars().all(|c| c == '.') {
                None
            } else if text.len() > 6 && text.ends_with("...") {
                Some(text_at(text[3..text.len() - 3].trim(), no))
            } else {
                return Err(Declined::at(
                    no,
                    "a delay is written `...` or `... text ...`",
                ));
            };
            self.diagram.items.push(Item::Delay { label, line: no });
            self.after_delay = true;
            return Ok(at + 1);
        }
        if text.starts_with("||") {
            let height = if text.chars().all(|c| c == '|') {
                None
            } else {
                let inner = text
                    .strip_prefix("||")
                    .and_then(|t| t.strip_suffix("||"))
                    .and_then(|t| t.trim().parse::<f32>().ok())
                    .filter(|h| (0.0..=10_000.0).contains(h));
                match inner {
                    Some(h) => Some(h),
                    None => {
                        return Err(Declined::at(no, "a space is written `|||` or `||45||`"));
                    }
                }
            };
            self.diagram.items.push(Item::Space { height, line: no });
            return Ok(at + 1);
        }
        if let Some(word) = NOT_YET.iter().find(|w| {
            starts_with_ci(text, w)
                && text[w.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric())
        }) {
            return Err(Declined::at(no, format!("`{word}` is not supported yet")));
        }
        match split_message(text) {
            Some(Ok(raw)) => {
                self.message(raw, no)?;
                Ok(at + 1)
            }
            Some(Err(reason)) => Err(Declined::at(no, reason)),
            None => Err(Declined::at(
                no,
                format!(
                    "`{}` is not part of a sequence diagram the native engine can draw",
                    snippet(text)
                ),
            )),
        }
    }

    fn finish(mut self) -> Result<Diagram, Declined> {
        if let Some(&(kind, line)) = self.groups.last() {
            return Err(Declined::at(
                line,
                format!("`{}` has no `end`", kind.keyword()),
            ));
        }
        if self.open_box.is_some() {
            let line = self.diagram.boxes.last().map_or(0, |b| b.line);
            return Err(Declined::at(line, "`box` has no `end box`"));
        }
        if let Some((participant, line)) = self.pending_create {
            let code = &self.diagram.participants[participant].code;
            return Err(Declined::at(
                line,
                format!("`create {code}` needs a message to `{code}` after it"),
            ));
        }
        self.diagram.frame = self.frame;
        Ok(reordered(self.diagram, &self.order))
    }

    /// The participant called `code`, created if new. Like PlantUML, only
    /// the code counts: `"Web app"` is not the participant `"Web app" as
    /// Web`.
    fn participant(&mut self, code: &str, line: usize) -> usize {
        self.participant_as(code, None, line)
    }

    /// Like [`Self::participant`]; a new participant shows `label` if one
    /// is given, one that exists keeps its name.
    fn participant_as(&mut self, code: &str, label: Option<&str>, line: usize) -> usize {
        let index = self.find_or_add(code, label, line);
        self.used[index] = true;
        index
    }

    /// Like [`Self::participant_as`], without counting it as used.
    fn find_or_add(&mut self, code: &str, label: Option<&str>, line: usize) -> usize {
        let found = self
            .diagram
            .participants
            .iter()
            .position(|p| p.code == code);
        if let Some(index) = found {
            return index;
        }
        self.diagram.participants.push(Participant {
            code: code.to_owned(),
            label: label.unwrap_or(code).to_owned(),
            shape: Shape::Participant,
            stereotype: None,
            spot: None,
            color: None,
            line,
        });
        self.used.push(false);
        let index = self.diagram.participants.len() - 1;
        self.order.push(index);
        // Named first inside a box, it stands in the box, as in PlantUML.
        if let Some(open) = self.open_box {
            self.diagram.boxes[open].participants.push(index);
        }
        index
    }

    fn declare(&mut self, shape: Shape, rest: &str, no: usize) -> Result<usize, Declined> {
        let (first, quoted, mut rest) =
            declared_name(rest).ok_or_else(|| Declined::at(no, "a participant needs a name"))?;
        let mut second = None;
        if let Some(after) = keyword(rest.trim_start(), "as") {
            let (name, second_quoted, after) = declared_name(after)
                .ok_or_else(|| Declined::at(no, "`as` needs a name after it"))?;
            second = Some((name, second_quoted));
            rest = after;
        }
        // Unquoted, a name holds what a message may name.
        for (name, quoted) in [(&first, quoted)]
            .into_iter()
            .chain(second.iter().map(|(n, q)| (n, *q)))
        {
            if !quoted && !name.chars().all(is_name_char) {
                return Err(Declined::at(
                    no,
                    format!(
                        "`{}` is no participant name for PlantUML, which takes letters, digits, `_`, `.` and `@`",
                        snippet(name)
                    ),
                ));
            }
        }
        let (code, label) = match second {
            // "Long name" as L, L as "Long name", Name as N
            Some((alias, _)) if quoted => (alias, first),
            Some((label, true)) => (first, label),
            Some((alias, false)) => (alias, first),
            None => (first.clone(), first),
        };
        let mut stereotype = None;
        let mut spot = None;
        let mut color = None;
        let mut rest = rest.trim_start();
        while !rest.is_empty() {
            if let Some(after) = rest.strip_prefix("<<") {
                let end = after
                    .find(">>")
                    .ok_or_else(|| Declined::at(no, "a stereotype needs `>>`"))?;
                if end == 0 {
                    return Err(Declined::at(no, "a stereotype needs a text"));
                }
                (spot, stereotype) = spotted(&after[..end]);
                rest = after[end + 2..].trim_start();
            } else if rest.starts_with('#') {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                color = Some(rest[..end].to_owned());
                rest = rest[end..].trim_start();
            } else {
                return Err(Declined::at(
                    no,
                    format!(
                        "`{}` in a participant declaration is not supported yet",
                        snippet(rest)
                    ),
                ));
            }
        }

        let existing = self
            .diagram
            .participants
            .iter()
            .position(|p| p.code == code);
        let index = match existing {
            // PlantUML keeps how it first appeared and moves it to the
            // end, out of its box (into the open one, below).
            Some(index) => {
                for b in &mut self.diagram.boxes {
                    b.participants.retain(|&i| i != index);
                }
                let emptied = self
                    .diagram
                    .boxes
                    .iter()
                    .enumerate()
                    .find(|(at, b)| b.participants.is_empty() && Some(*at) != self.open_box);
                if let Some((_, b)) = emptied {
                    return Err(Declined::at(
                        no,
                        format!(
                            "declaring `{code}` again empties the box of line {}, which is not supported yet",
                            b.line + 1
                        ),
                    ));
                }
                self.order.retain(|&i| i != index);
                self.order.push(index);
                index
            }
            None => {
                self.diagram.participants.push(Participant {
                    code,
                    label,
                    shape,
                    stereotype,
                    spot,
                    color,
                    line: no,
                });
                self.used.push(false);
                self.order.push(self.diagram.participants.len() - 1);
                self.diagram.participants.len() - 1
            }
        };
        if let Some(open) = self.open_box {
            let members = &mut self.diagram.boxes[open].participants;
            if !members.contains(&index) {
                members.push(index);
            }
        }
        Ok(index)
    }

    /// `create [shape] Name`.
    fn create(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        let participant = match shape_keyword(rest) {
            Some((shape, rest)) => self.declare(shape, rest, no)?,
            None => {
                let (code, tail) =
                    name(rest).ok_or_else(|| Declined::at(no, "`create` needs a participant"))?;
                if !tail.trim().is_empty() {
                    return Err(Declined::at(no, "`create` takes one participant"));
                }
                self.find_or_add(&code, None, no)
            }
        };
        self.created(participant, no)
    }

    /// `create` and `**`: the next message must go to `participant`, and
    /// its arrow draws the head. Nothing may use it before.
    fn created(&mut self, participant: usize, no: usize) -> Result<(), Declined> {
        let code = &self.diagram.participants[participant].code;
        if self.used[participant] {
            return Err(Declined::at(
                no,
                format!("`{code}` is created after it was used"),
            ));
        }
        if let Some((waiting, _)) = self.pending_create
            && waiting != participant
        {
            return Err(self.not_created(waiting, no));
        }
        self.pending_create = Some((participant, no));
        Ok(())
    }

    /// Whether the message from `from` to `to` creates the participant a
    /// `create` made; PlantUML takes no other message after `create`.
    fn creates(&mut self, from: End, to: End, no: usize) -> Result<bool, Declined> {
        let Some((created, _)) = self.pending_create.take() else {
            return Ok(false);
        };
        let it = End::Participant(created);
        if to == it && from != it {
            return Ok(true);
        }
        if from == it && !matches!(to, End::Participant(_)) {
            let code = &self.diagram.participants[created].code;
            return Err(Declined::at(
                no,
                format!("`{code}` sending the message that creates it is not supported yet"),
            ));
        }
        Err(self.not_created(created, no))
    }

    fn not_after_delay(&self, what: &str, no: usize) -> Result<(), Declined> {
        if self.after_delay {
            return Err(Declined::at(
                no,
                format!("after a delay, PlantUML takes no `{what}` before the next message"),
            ));
        }
        Ok(())
    }

    fn not_created(&self, participant: usize, no: usize) -> Declined {
        let code = &self.diagram.participants[participant].code;
        Declined::at(
            no,
            format!("after `create {code}`, the next message must go to `{code}`"),
        )
    }

    fn activate(&mut self, participant: usize, color: Option<String>, no: usize) {
        let caller = self
            .last_message
            .filter(|(_, to)| *to == End::Participant(participant))
            .map(|(from, _)| from);
        self.activations.push((participant, caller));
        self.diagram.items.push(Item::Activate {
            participant,
            color,
            line: no,
        });
    }

    fn deactivate(&mut self, participant: usize, no: usize) {
        if let Some(at) = self
            .activations
            .iter()
            .rposition(|(p, _)| *p == participant)
        {
            self.activations.remove(at);
        }
        self.diagram.items.push(Item::Deactivate {
            participant,
            line: no,
        });
    }

    /// `return label`: a dashed message from the newest activation back to
    /// whoever started it, which ends the activation.
    fn give_back(&mut self, label: &str, no: usize) -> Result<(), Declined> {
        let Some(&(participant, caller)) = self.activations.last() else {
            return Err(Declined::at(no, "`return` without an active participant"));
        };
        let Some(caller) = caller else {
            return Err(Declined::at(
                no,
                "`return`: no message started the activation it ends",
            ));
        };
        let creates = self.creates(End::Participant(participant), caller, no)?;
        let number = self.next_number();
        self.diagram.items.push(Item::Message(Message {
            from: End::Participant(participant),
            to: caller,
            arrow: Arrow {
                dashed: true,
                ..plain_arrow()
            },
            text: (!label.is_empty()).then(|| text_at(label, no)),
            number,
            creates,
            line: no,
        }));
        self.last_message = Some((End::Participant(participant), caller));
        self.after_delay = false;
        self.deactivate(participant, no);
        Ok(())
    }

    fn next_number(&mut self) -> Option<String> {
        let auto = &mut self.autonumber;
        if !auto.on {
            return None;
        }
        let n = auto.next;
        auto.next = auto.next.saturating_add(auto.step);
        Some(match &auto.format {
            None => format!("<b>{n}</b>"),
            Some(format) => number_with(format, n),
        })
    }

    fn autonumber(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        if let Some(after) = keyword(rest, "stop") {
            if !after.is_empty() {
                return Err(Declined::at(no, "`autonumber stop` takes nothing after it"));
            }
            self.autonumber.on = false;
            return Ok(());
        }
        let (resume, mut rest) = match keyword(rest, "resume") {
            Some(after) => (true, after),
            None => (false, rest),
        };
        let mut numbers = Vec::new();
        let mut format = None;
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            if format.is_some() {
                return Err(Declined::at(no, "nothing may follow the autonumber format"));
            }
            if let Some(quoted) = rest.strip_prefix('"') {
                let end = quoted.find('"').ok_or_else(|| {
                    Declined::at(no, "the autonumber format needs a closing `\"`")
                })?;
                format = Some(quoted[..end].to_owned());
                rest = &quoted[end + 1..];
                continue;
            }
            let token = rest.split_whitespace().next().unwrap_or(rest);
            match token.parse::<i64>() {
                Ok(n) if numbers.len() < 2 => numbers.push(n),
                _ => {
                    return Err(Declined::at(
                        no,
                        format!("`autonumber {}` is not supported yet", snippet(rest)),
                    ));
                }
            }
            rest = &rest[token.len()..];
        }
        let auto = &mut self.autonumber;
        if resume {
            if numbers.len() > 1 {
                return Err(Declined::at(no, "`autonumber resume` takes one step"));
            }
            if let Some(&step) = numbers.first() {
                auto.step = step;
            }
            if format.is_some() {
                auto.format = format;
            }
        } else {
            auto.next = numbers.first().copied().unwrap_or(1);
            auto.step = numbers.get(1).copied().unwrap_or(1);
            auto.format = format;
        }
        auto.on = true;
        Ok(())
    }

    fn message(&mut self, raw: RawMessage, no: usize) -> Result<(), Declined> {
        let (life, text) = lifecycle(raw.rest).map_err(|reason| Declined::at(no, reason))?;
        let starred = life.steps.contains(&Step::Create);
        let parts = raw.arrow;
        // Ends in the order they are written: participants appear in that order.
        let left = self.end(raw.left, Side::Left, no);
        let right = match raw.right {
            RawEnd::Name { code, label } if starred => {
                let index = self.find_or_add(&code, label.as_deref(), no);
                self.created(index, no)?;
                self.used[index] = true;
                End::Participant(index)
            }
            other => self.end(other, Side::Right, no),
        };
        let (from, to, head, tail, end_mark, start_mark) = match (parts.left_head, parts.right_head)
        {
            (Head::None, head) => (
                left,
                right,
                head,
                Head::None,
                parts.right_mark,
                parts.left_mark,
            ),
            (head, Head::None) => (
                right,
                left,
                head,
                Head::None,
                parts.left_mark,
                parts.right_mark,
            ),
            (tail, head) => (left, right, head, tail, parts.right_mark, parts.left_mark),
        };
        if !matches!(from, End::Participant(_)) && !matches!(to, End::Participant(_)) {
            return Err(Declined::at(no, "a message needs a participant at one end"));
        }
        if starred && (to != right || !matches!(to, End::Participant(_))) {
            return Err(Declined::at(
                no,
                "`**` needs an arrow pointing right, to the participant it creates",
            ));
        }
        let creates = self.creates(from, to, no)?;
        let number = self.next_number();
        self.diagram.items.push(Item::Message(Message {
            from,
            to,
            arrow: Arrow {
                dashed: parts.dashed,
                head,
                tail,
                end_mark,
                start_mark,
                color: parts.color,
                bold: parts.bold,
            },
            text: text.filter(|t| !t.is_empty()).map(|t| text_at(t, no)),
            number,
            creates,
            line: no,
        }));
        self.last_message = Some((from, to));
        self.after_delay = false;
        for &step in life.steps {
            let (end, what) = match step {
                Step::Create => continue,
                Step::Deactivate => (from, "`--` needs a participant sending the message"),
                Step::Activate => (to, "`++` needs a participant receiving the message"),
                Step::Destroy => (to, "`!!` needs a participant receiving the message"),
            };
            let End::Participant(participant) = end else {
                return Err(Declined::at(no, what));
            };
            match step {
                Step::Activate => self.activate(participant, life.color.clone(), no),
                Step::Deactivate => self.deactivate(participant, no),
                _ => self.diagram.items.push(Item::Destroy {
                    participant,
                    line: no,
                }),
            }
        }
        Ok(())
    }

    fn end(&mut self, raw: RawEnd, side: Side, no: usize) -> End {
        match raw {
            RawEnd::Name { code, label } => {
                End::Participant(self.participant_as(&code, label.as_deref(), no))
            }
            RawEnd::Edge => End::Edge(side),
            RawEnd::Short => End::Short(side),
        }
    }

    /// `note`, `hnote`, `rnote` in all their places; `None` if `lines[at]`
    /// is not a note.
    fn note(&mut self, lines: &[Line], at: usize) -> Result<Option<usize>, Declined> {
        let Line { no, text, .. } = lines[at];
        let Some((shape, rest)) = [
            ("note", NoteShape::Note),
            ("hnote", NoteShape::Hexagon),
            ("rnote", NoteShape::Rectangle),
        ]
        .into_iter()
        .find_map(|(word, shape)| keyword(text, word).map(|rest| (shape, rest))) else {
            return Ok(None);
        };
        let side = word_then(rest, "left")
            .map(|rest| (Side::Left, rest))
            .or_else(|| word_then(rest, "right").map(|rest| (Side::Right, rest)));
        let (place, rest) = if let Some((side, rest)) = side {
            match keyword(rest, "of") {
                Some(rest) => {
                    let (code, rest) = name(rest)
                        .ok_or_else(|| Declined::at(no, "`note … of` needs a participant"))?;
                    let p = self.participant(&code, no);
                    let place = match side {
                        Side::Left => NotePlace::LeftOf(p),
                        Side::Right => NotePlace::RightOf(p),
                    };
                    (place, rest)
                }
                None => {
                    if !matches!(self.diagram.items.last(), Some(Item::Message(_))) {
                        return Err(Declined::at(
                            no,
                            "a note without `of` must follow a message",
                        ));
                    }
                    (NotePlace::Message(side), rest)
                }
            }
        } else if let Some(rest) = keyword(rest, "over") {
            let (over, rest) = self.names(rest, no)?;
            (NotePlace::Over(over[0], over[over.len() - 1]), rest)
        } else if let Some(rest) = word_then(rest, "across") {
            (NotePlace::Across, rest)
        } else {
            return Err(Declined::at(
                no,
                format!("`{}` is not supported yet", snippet(text)),
            ));
        };
        let (color, rest) =
            leading_color(rest.trim_start()).map_err(|reason| Declined::at(no, reason))?;
        let (value, next) = if let Some(single) = rest.trim_start().strip_prefix(':') {
            (single.trim().to_owned(), at + 1)
        } else if rest.trim().is_empty() {
            block(lines, at, "note", |l| {
                matches!(
                    l.split_whitespace()
                        .collect::<String>()
                        .to_ascii_lowercase()
                        .as_str(),
                    "endnote" | "endhnote" | "endrnote"
                )
            })?
        } else {
            return Err(Declined::at(
                no,
                format!("`{}` in a note is not supported yet", snippet(rest.trim())),
            ));
        };
        self.diagram.items.push(Item::Note(Note {
            shape,
            place,
            text: Text { value, line: no },
            color,
            line: no,
        }));
        Ok(Some(next))
    }

    /// `ref over A, B : text`, or the text on the lines up to `end ref`.
    fn reference(&mut self, lines: &[Line], at: usize, rest: &str) -> Result<usize, Declined> {
        let no = lines[at].no;
        let rest = keyword(rest, "over")
            .ok_or_else(|| Declined::at(no, "a reference is written `ref over A, B : text`"))?;
        let (over, rest) = self.names(rest, no)?;
        let (color, rest) =
            leading_color(rest.trim_start()).map_err(|reason| Declined::at(no, reason))?;
        let (value, next) = if let Some(single) = rest.trim_start().strip_prefix(':') {
            (single.trim().to_owned(), at + 1)
        } else if rest.trim().is_empty() {
            block(lines, at, "ref", |l| squeezed_eq(l, "endref"))?
        } else {
            return Err(Declined::at(
                no,
                format!(
                    "`{}` in a reference is not supported yet",
                    snippet(rest.trim())
                ),
            ));
        };
        self.diagram.items.push(Item::Ref {
            over,
            text: Text { value, line: no },
            color,
        });
        Ok(next)
    }

    /// One or more participants separated by commas.
    fn names<'a>(&mut self, rest: &'a str, no: usize) -> Result<(Vec<usize>, &'a str), Declined> {
        let mut out = Vec::new();
        let mut rest = rest;
        loop {
            let (code, after) = name(rest.trim_start())
                .ok_or_else(|| Declined::at(no, "a participant is missing"))?;
            out.push(self.participant(&code, no));
            match after.trim_start().strip_prefix(',') {
                Some(more) => rest = more,
                None => return Ok((out, after)),
            }
        }
    }

    fn open_box(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        if self.open_box.is_some() {
            return Err(Declined::at(no, "a box inside a box is not supported"));
        }
        let rest = rest.trim();
        let (title, rest) = if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted
                .find('"')
                .ok_or_else(|| Declined::at(no, "the box title needs a closing `\"`"))?;
            (Some(quoted[..end].to_owned()), quoted[end + 1..].trim())
        } else {
            let end = rest.find('#').unwrap_or(rest.len());
            let title = rest[..end].trim();
            ((!title.is_empty()).then(|| title.to_owned()), &rest[end..])
        };
        let color = color_only(rest, no)?;
        self.diagram.boxes.push(ParticipantBox {
            title: title.map(|t| Text { value: t, line: no }),
            color,
            line: no,
            participants: Vec::new(),
        });
        self.open_box = Some(self.diagram.boxes.len() - 1);
        Ok(())
    }

    /// Group starts and `else`; `false` if the line is neither.
    fn group(&mut self, text: &str, no: usize) -> Result<bool, Declined> {
        for (word, kind) in GROUPS {
            if starts_with_ci(text, word) && text[word.len()..].starts_with('#') {
                return Err(Declined::at(
                    no,
                    format!("group colors (`{word}#…`) are not supported yet"),
                ));
            }
            let Some(rest) = keyword(text, word) else {
                continue;
            };
            let (label, secondary) = match rest.strip_suffix(']').and_then(|r| r.rsplit_once('[')) {
                Some((label, secondary)) if kind == GroupKind::Group => {
                    (label.trim(), Some(secondary.trim()))
                }
                _ => (rest, None),
            };
            self.diagram.items.push(Item::GroupStart {
                kind,
                label: (!label.is_empty()).then(|| text_at(label, no)),
                secondary: secondary.filter(|s| !s.is_empty()).map(|s| text_at(s, no)),
                line: no,
            });
            self.groups.push((kind, no));
            return Ok(true);
        }
        if let Some(rest) = keyword(text, "else") {
            if self.groups.is_empty() {
                return Err(Declined::at(no, "`else` outside a group"));
            }
            self.diagram.items.push(Item::GroupElse {
                label: (!rest.is_empty()).then(|| text_at(rest, no)),
                line: no,
            });
            return Ok(true);
        }
        Ok(false)
    }
}

/// Like [`keyword`], but the word may also be followed by `:` (`note
/// left: text`), which stays in the rest.
fn word_then<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    keyword(text, word).or_else(|| {
        let rest = text.get(word.len()..)?;
        (starts_with_ci(text, word) && rest.starts_with(':')).then_some(rest)
    })
}

fn shape_keyword(text: &str) -> Option<(Shape, &str)> {
    SHAPES
        .iter()
        .find_map(|&(word, shape)| keyword(text, word).map(|rest| (shape, rest)))
}

/// `diagram` with its participants in `order`, every index following.
fn reordered(mut diagram: Diagram, order: &[usize]) -> Diagram {
    if order.iter().enumerate().all(|(at, &i)| at == i) {
        return diagram;
    }
    let mut new = vec![0; order.len()];
    for (at, &old) in order.iter().enumerate() {
        new[old] = at;
    }
    let mut old = std::mem::take(&mut diagram.participants)
        .into_iter()
        .map(Some)
        .collect::<Vec<_>>();
    diagram.participants = order.iter().filter_map(|&i| old[i].take()).collect();
    let end = |e: &mut End| {
        if let End::Participant(i) = e {
            *i = new[*i];
        }
    };
    for item in &mut diagram.items {
        match item {
            Item::Message(m) => {
                end(&mut m.from);
                end(&mut m.to);
            }
            Item::Note(note) => match &mut note.place {
                NotePlace::LeftOf(i) | NotePlace::RightOf(i) => *i = new[*i],
                NotePlace::Over(a, b) => {
                    let (x, y) = (new[*a], new[*b]);
                    (*a, *b) = (x.min(y), x.max(y));
                }
                NotePlace::Across | NotePlace::Message(_) => {}
            },
            Item::Ref { over, .. } => {
                for i in over.iter_mut() {
                    *i = new[*i];
                }
                over.sort_unstable();
            }
            Item::Activate { participant, .. }
            | Item::Deactivate { participant, .. }
            | Item::Destroy { participant, .. } => *participant = new[*participant],
            _ => {}
        }
    }
    for b in &mut diagram.boxes {
        for i in b.participants.iter_mut() {
            *i = new[*i];
        }
    }
    diagram
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '@')
}

/// A participant name at the start of `text` (after spaces): in quotes, or
/// a run of letters, digits, `_`, `.` and `@`. With the rest of the text.
fn name(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    if let Some(quoted) = text.strip_prefix('"') {
        let end = quoted.find('"')?;
        let name = quoted[..end].trim();
        return (!name.is_empty()).then(|| (name.to_owned(), &quoted[end + 1..]));
    }
    let end = text.find(|c| !is_name_char(c)).unwrap_or(text.len());
    (end > 0).then(|| (text[..end].to_owned(), &text[end..]))
}

/// A name in a declaration: quoted, or anything up to a space, `#` or `<`.
/// With whether it was quoted, and the rest.
fn declared_name(text: &str) -> Option<(String, bool, &str)> {
    let text = text.trim_start();
    if text.starts_with('"') {
        let (name, rest) = name(text)?;
        return Some((name, true, rest));
    }
    let end = text
        .find(|c: char| c.is_whitespace() || matches!(c, '#' | '<' | '"'))
        .unwrap_or(text.len());
    (end > 0).then(|| (text[..end].to_owned(), false, &text[end..]))
}

/// A `#color` at the start of `text`, and the rest; `#` alone is none.
fn leading_color(text: &str) -> Result<(Option<String>, &str), &'static str> {
    if !text.starts_with('#') {
        return Ok((None, text));
    }
    let end = text
        .find(|c: char| c.is_whitespace() || c == ':')
        .unwrap_or(text.len());
    if end == 1 {
        return Err("a color is missing after `#`");
    }
    Ok((Some(text[..end].to_owned()), &text[end..]))
}

/// `text` is empty or one `#color`.
fn color_only(text: &str, no: usize) -> Result<Option<String>, Declined> {
    let (color, rest) = leading_color(text).map_err(|reason| Declined::at(no, reason))?;
    if rest.trim().is_empty() {
        Ok(color)
    } else {
        Err(Declined::at(
            no,
            format!("unexpected `{}`", snippet(rest.trim())),
        ))
    }
}

/// `format` (PlantUML's autonumber format) with `n` in place of its first
/// run of `0` and `#`; `0`s pad with zeros. Markup in `<…>` is left alone.
fn number_with(format: &str, n: i64) -> String {
    let mut in_tag = false;
    let mut run: Option<(usize, usize)> = None;
    for (i, b) in format.bytes().enumerate() {
        if in_tag {
            in_tag = b != b'>';
            continue;
        }
        match b {
            b'0' | b'#' => run = Some((run.map_or(i, |(start, _)| start), i + 1)),
            _ if run.is_some() => break,
            b'<' => in_tag = true,
            _ => {}
        }
    }
    let Some((start, end)) = run else {
        return format!("{format}{n}");
    };
    let width = format[start..end].matches('0').count();
    format!("{}{n:0width$}{}", &format[..start], &format[end..])
}

enum RawEnd {
    /// A participant, with the name it shows if it is new.
    Name {
        code: String,
        label: Option<String>,
    },
    Edge,
    Short,
}

#[derive(Default)]
struct ArrowParts {
    left_mark: Mark,
    left_head: Head,
    right_head: Head,
    right_mark: Mark,
    dashed: bool,
    color: Option<String>,
    bold: bool,
}

fn plain_arrow() -> Arrow {
    Arrow {
        dashed: false,
        head: Head::Filled,
        tail: Head::None,
        end_mark: Mark::None,
        start_mark: Mark::None,
        color: None,
        bold: false,
    }
}

struct RawMessage<'a> {
    left: RawEnd,
    arrow: ArrowParts,
    right: RawEnd,
    /// After the second end: `++` and the like, `: text`.
    rest: &'a str,
}

/// A message line taken apart: `None` if `text` does not look like a
/// message, an error if it does but uses something not supported.
fn split_message(text: &str) -> Option<Result<RawMessage<'_>, String>> {
    // `[` and `?` touch the arrow; after a participant, a mark needs a
    // space before it.
    let (left, arrow_text, may_mark) = if let Some(rest) = text.strip_prefix('[') {
        (RawEnd::Edge, rest, true)
    } else if let Some(rest) = text.strip_prefix('?') {
        (RawEnd::Short, rest, true)
    } else {
        let (code, label, rest) = message_end(text)?;
        let spaced = rest.starts_with(char::is_whitespace);
        (RawEnd::Name { code, label }, rest.trim_start(), spaced)
    };
    if !matches!(left, RawEnd::Name { .. }) && arrow_text.starts_with(char::is_whitespace) {
        return arrow(arrow_text.trim_start(), true)
            .map(|_| Err("`[` and `?` go right before the arrow, without a space".into()));
    }
    let (arrow, rest) = match arrow(arrow_text, may_mark)? {
        Ok(found) => found,
        Err(reason) => return Some(Err(reason)),
    };
    let (right, rest) = if let Some(rest) = rest.strip_prefix(']') {
        (RawEnd::Edge, rest)
    } else if let Some(rest) = rest.strip_prefix('?') {
        (RawEnd::Short, rest)
    } else if rest.trim_start().starts_with([']', '?']) {
        return Some(Err(
            "`]` and `?` go right after the arrow, without a space".into()
        ));
    } else {
        match message_end(rest) {
            Some((code, label, rest)) => (RawEnd::Name { code, label }, rest),
            None => return Some(Err("a message needs something at both ends".into())),
        }
    };
    // PlantUML takes `"Long name" as L` only between two participants.
    let named = |end: &RawEnd| matches!(end, RawEnd::Name { .. });
    let labelled = |end: &RawEnd| matches!(end, RawEnd::Name { label: Some(_), .. });
    if (labelled(&left) || labelled(&right)) && !(named(&left) && named(&right)) {
        return Some(Err(
            "`as` in a message works only between two participants".into()
        ));
    }
    Some(Ok(RawMessage {
        left,
        arrow,
        right,
        rest,
    }))
}

/// A participant at one end of a message: `Name` or `"Long name"`, and for
/// one that is new also `"Long name" as L` or `L as "Long name"`. With its
/// code, the name it shows and the rest.
fn message_end(text: &str) -> Option<(String, Option<String>, &str)> {
    let text = text.trim_start();
    let (first, rest) = name(text)?;
    if text.starts_with('"') {
        // `"Long name" as L`: a space after `as`, before it optional.
        if let Some(after) = after_as(rest)
            && after.starts_with(char::is_whitespace)
            && !after.trim_start().starts_with('"')
            && let Some((code, rest)) = name(after)
        {
            return Some((code, Some(first), rest));
        }
    } else if rest.starts_with(char::is_whitespace)
        // `L as "Long name"`: a space before `as`, after it optional.
        && let Some(after) = after_as(rest)
        && after.trim_start().starts_with('"')
        && let Some((label, rest)) = name(after)
    {
        return Some((first, Some(label), rest));
    }
    Some((first, None, rest))
}

/// `text` after spaces and the word `as`, in any case.
fn after_as(text: &str) -> Option<&str> {
    let text = text.trim_start();
    let word = text.get(..2)?;
    word.eq_ignore_ascii_case("as").then(|| &text[2..])
}

/// An arrow at the start of `text`: marks, heads, a body of dashes with an
/// optional `[style]`. `may_mark` says whether an `o` or `x` may come
/// first: after `[` and `?`, or after a space.
fn arrow(text: &str, may_mark: bool) -> Option<Result<(ArrowParts, &str), String>> {
    let bytes = text.as_bytes();
    let mut parts = ArrowParts::default();
    let mut i = 0;
    let mark = |b: u8| if b == b'x' { Mark::Cross } else { Mark::Circle };
    if may_mark
        && matches!(bytes.first(), Some(b'x' | b'o'))
        && matches!(bytes.get(1), Some(b'<' | b'\\' | b'/' | b'-'))
    {
        parts.left_mark = mark(bytes[0]);
        i = 1;
    }
    for (token, head) in [
        ("<<", Head::Open),
        ("<", Head::Filled),
        ("//", Head::UpperOpen),
        ("/", Head::UpperFilled),
        ("\\\\", Head::LowerOpen),
        ("\\", Head::LowerFilled),
    ] {
        if text[i..].starts_with(token) && text[i + token.len()..].starts_with('-') {
            parts.left_head = head;
            i += token.len();
            break;
        }
    }
    if !text[i..].starts_with('-') {
        return None;
    }
    let mut dashes = 0;
    let mut styled = false;
    loop {
        if text[i..].starts_with('-') {
            dashes += 1;
            i += 1;
        } else if text[i..].starts_with('[') && !styled {
            let close = text[i..].find(']')?;
            if let Err(reason) = style(&mut parts, &text[i + 1..i + close]) {
                return Some(Err(reason));
            }
            styled = true;
            i += close + 1;
        } else {
            break;
        }
    }
    parts.dashed |= dashes >= 2;
    for (token, head) in [
        (">>", Head::Open),
        (">", Head::Filled),
        ("\\\\", Head::UpperOpen),
        ("\\", Head::UpperFilled),
        ("//", Head::LowerOpen),
        ("/", Head::LowerFilled),
    ] {
        if text[i..].starts_with(token) {
            parts.right_head = head;
            i += token.len();
            break;
        }
    }
    // A mark touches the head, then a space and a participant follow, or
    // `]` or `?`. `->x: hi` goes to a participant called `x`.
    if parts.right_head != Head::None
        && matches!(bytes.get(i), Some(b'x' | b'o'))
        && bytes
            .get(i + 1)
            .is_some_and(|b| b.is_ascii_whitespace() || matches!(b, b']' | b'?'))
    {
        parts.right_mark = mark(bytes[i]);
        i += 1;
    }
    if parts.left_head == Head::None && parts.right_head == Head::None {
        return None;
    }
    Some(Ok((parts, &text[i..])))
}

/// The inside of `-[…]->`: colors and `dashed`, `dotted`, `bold`, `plain`.
fn style(parts: &mut ArrowParts, inside: &str) -> Result<(), String> {
    for item in inside.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        match item.to_ascii_lowercase().as_str() {
            "dashed" | "dotted" => parts.dashed = true,
            "bold" => parts.bold = true,
            "plain" => {}
            _ if item.starts_with('#') => parts.color = Some(item.to_owned()),
            _ => return Err(format!("the arrow style `{item}` is not supported yet")),
        }
    }
    Ok(())
}

/// One of [`LIFECYCLES`] and an activation color after a message's second
/// end, then `: text`.
fn lifecycle(rest: &str) -> Result<(Lifecycle, Option<&str>), String> {
    let mut life = Lifecycle::default();
    let mut rest = rest.trim_start();
    if let Some(&(spec, steps)) = LIFECYCLES.iter().find(|(spec, _)| rest.starts_with(spec)) {
        life.steps = steps;
        rest = rest[spec.len()..].trim_start();
    }
    let (color, after) = leading_color(rest)?;
    life.color = color;
    let rest = after.trim_start();
    if rest.is_empty() {
        return Ok((life, None));
    }
    match rest.strip_prefix(':') {
        Some(text) => Ok((life, Some(text.trim()))),
        None => Err(format!("unexpected `{}` after the message", snippet(rest))),
    }
}

/// `(C,#ADD1B2) Testable`: the spot (one letter, and its color) and what
/// is left of the stereotype, each if there is one.
fn spotted(stereotype: &str) -> (Option<Spot>, Option<String>) {
    let text = stereotype.trim();
    let some = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    let Some((inner, rest)) = text.strip_prefix('(').and_then(|t| t.split_once(')')) else {
        return (None, some(text));
    };
    let (letter, color) = match inner.split_once(',') {
        Some((letter, color)) => (letter.trim(), some(color.trim())),
        None => (inner.trim(), None),
    };
    let mut chars = letter.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), None) => (Some(Spot { letter, color }), some(rest.trim())),
        _ => (None, some(text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::body;

    fn diagram(source: &str) -> Diagram {
        parse(&body(source).unwrap().1).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn declined(source: &str) -> Declined {
        match body(source) {
            Ok((_, lines)) => parse(&lines).expect_err("should decline"),
            Err(e) => e,
        }
    }

    fn messages(d: &Diagram) -> Vec<&Message> {
        d.items
            .iter()
            .filter_map(|i| match i {
                Item::Message(m) => Some(m),
                _ => None,
            })
            .collect()
    }

    fn codes(d: &Diagram) -> Vec<&str> {
        d.participants.iter().map(|p| p.code.as_str()).collect()
    }

    #[test]
    fn declarations_name_alias_and_shape() {
        let d = diagram(
            "@startuml\nactor User\nparticipant \"Web App\" as Web #lightblue\nparticipant Auth as \"Auth service\" <<backend>>\ndatabase DB as D\nqueue Q\n@enduml",
        );
        assert_eq!(codes(&d), ["User", "Web", "Auth", "D", "Q"]);
        let p = &d.participants;
        assert_eq!((p[0].shape, p[0].label.as_str()), (Shape::Actor, "User"));
        assert_eq!(p[1].label, "Web App");
        assert_eq!(p[1].color.as_deref(), Some("#lightblue"));
        assert_eq!(p[2].label, "Auth service");
        assert_eq!(p[2].stereotype.as_deref(), Some("backend"));
        assert_eq!((p[3].label.as_str(), p[3].shape), ("DB", Shape::Database));
        assert_eq!(p[4].line, 5);
    }

    #[test]
    fn a_stereotype_may_carry_a_spot() {
        let d = diagram(
            "@startuml\nparticipant A << (C,#ADD1B2) Testable >>\nparticipant B << (S) >>\nparticipant C << ( x , orchid ) >>\nparticipant D << (long) no spot >>\n@enduml",
        );
        let p = &d.participants;
        let spot = |letter, color: Option<&str>| {
            Some(Spot {
                letter,
                color: color.map(str::to_owned),
            })
        };
        assert_eq!(p[0].spot, spot('C', Some("#ADD1B2")));
        assert_eq!(p[0].stereotype.as_deref(), Some("Testable"));
        assert_eq!(
            (p[1].spot.clone(), p[1].stereotype.clone()),
            (spot('S', None), None)
        );
        assert_eq!(p[2].spot, spot('x', Some("orchid")));
        assert_eq!(p[3].spot, None);
        assert_eq!(p[3].stereotype.as_deref(), Some("(long) no spot"));
    }

    #[test]
    fn a_return_that_is_an_arrow_is_a_message() {
        let back = diagram("Alice -> Bob ++ : hi\nreturn ok");
        let m = messages(&back);
        assert_eq!(codes(&back), ["Alice", "Bob"]);
        assert_eq!(
            (
                m[1].from,
                m[1].to,
                m[1].text.as_ref().map(|t| t.value.as_str())
            ),
            (End::Participant(1), End::Participant(0), Some("ok"))
        );

        let sent = diagram("Alice -> Bob ++ : hi\nreturn o<-k");
        assert_eq!(codes(&sent), ["Alice", "Bob", "return", "k"]);
        let m = messages(&sent);
        assert_eq!(m.len(), 2);
        assert_eq!(
            (m[1].from, m[1].to, m[1].text.is_none()),
            (End::Participant(3), End::Participant(2), true)
        );
    }

    #[test]
    fn messages_create_participants_in_order() {
        let d = diagram("Alice -> Bob : hi\nBob <-- Carol\nAlice -> Alice : self");
        assert_eq!(codes(&d), ["Alice", "Bob", "Carol"]);
        let m = messages(&d);
        assert_eq!(m.len(), 3);
        assert_eq!(
            (m[0].from, m[0].to),
            (End::Participant(0), End::Participant(1))
        );
        assert_eq!(m[0].text.as_ref().unwrap().value, "hi");
        // Bob <-- Carol: from Carol to Bob, dashed.
        assert_eq!(
            (m[1].from, m[1].to),
            (End::Participant(2), End::Participant(1))
        );
        assert!(m[1].arrow.dashed && m[1].text.is_none());
        assert_eq!(m[2].from, m[2].to);
        assert_eq!(m[2].line, 2);
    }

    #[test]
    fn arrow_kinds() {
        let d = diagram(
            "A ->> B\nA -\\ B\nA //-- B\nA ->x B\nA <-> B\nA o\\\\-- B\nA -[#red]> B\nA -[#0000ff,dashed]-> B\nA ->o B : x",
        );
        let m = messages(&d);
        let arrow = |i: usize| &m[i].arrow;
        assert_eq!(arrow(0).head, Head::Open);
        assert_eq!(arrow(1).head, Head::UpperFilled);
        assert_eq!(
            (m[2].from, arrow(2).head, arrow(2).dashed),
            (End::Participant(1), Head::UpperOpen, true)
        );
        assert_eq!(arrow(3).end_mark, Mark::Cross);
        assert_eq!((arrow(4).head, arrow(4).tail), (Head::Filled, Head::Filled));
        assert_eq!(
            (m[5].to, arrow(5).head, arrow(5).end_mark),
            (End::Participant(0), Head::LowerOpen, Mark::Circle)
        );
        assert_eq!(arrow(6).color.as_deref(), Some("#red"));
        assert!(!arrow(6).dashed);
        assert!(arrow(7).dashed && arrow(7).color.as_deref() == Some("#0000ff"));
        assert_eq!(arrow(8).end_mark, Mark::Circle);
        assert_eq!(m[8].text.as_ref().unwrap().value, "x");
    }

    #[test]
    fn edges_and_short_arrows() {
        let d = diagram("[-> A : in\nA ->] : out\n?-> A\nA ->?\n[<- A");
        let m = messages(&d);
        assert_eq!(
            (m[0].from, m[0].to),
            (End::Edge(Side::Left), End::Participant(0))
        );
        assert_eq!(
            (m[1].from, m[1].to),
            (End::Participant(0), End::Edge(Side::Right))
        );
        assert_eq!(m[2].from, End::Short(Side::Left));
        assert_eq!(m[3].to, End::Short(Side::Right));
        assert_eq!(
            (m[4].from, m[4].to),
            (End::Participant(0), End::Edge(Side::Left))
        );
        assert_eq!(codes(&d), ["A"]);
    }

    /// Messages and what happens to the life lines, one line each.
    fn events(d: &Diagram) -> Vec<String> {
        d.items
            .iter()
            .map(|i| match i {
                Item::Message(m) => format!("msg {:?}->{:?} {}", m.from, m.to, m.arrow.dashed),
                Item::Activate {
                    participant, color, ..
                } => format!("on {participant} {color:?}"),
                Item::Deactivate { participant, .. } => format!("off {participant}"),
                Item::Destroy { participant, .. } => format!("x {participant}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn activations_and_return() {
        let d = diagram(
            "A -> B ++ : call\nB -> C ++ #gold\nreturn done\nreturn\nactivate A\nA -> B --\ndestroy B",
        );
        assert_eq!(
            events(&d),
            [
                "msg Participant(0)->Participant(1) false",
                "on 1 None",
                "msg Participant(1)->Participant(2) false",
                "on 2 Some(\"#gold\")",
                "msg Participant(2)->Participant(1) true",
                "off 2",
                "msg Participant(1)->Participant(0) true",
                "off 1",
                "on 0 None",
                "msg Participant(0)->Participant(1) false",
                "off 0",
                "x 1",
            ]
        );
        assert!(declined("return").reason.contains("return"));
        assert!(declined("activate A\nreturn").reason.contains("no message"));
    }

    #[test]
    fn lifecycle_steps_run_in_the_written_order() {
        // `++--` activates first, `--++` deactivates first; a color without
        // `++` is ignored, as in PlantUML.
        let d = diagram("A -> A ++-- : once\nA -> B ++\nB -> C --++ #gold\nA -> D #red");
        assert_eq!(
            events(&d),
            [
                "msg Participant(0)->Participant(0) false",
                "on 0 None",
                "off 0",
                "msg Participant(0)->Participant(1) false",
                "on 1 None",
                "msg Participant(1)->Participant(2) false",
                "off 1",
                "on 2 Some(\"#gold\")",
                "msg Participant(0)->Participant(3) false",
            ]
        );
        // PlantUML knows six forms and no others.
        for (source, part) in [
            ("A -> B ++++", "`++`"),
            ("A -> B ++ --", "`--`"),
            ("A -> B !!**", "`**`"),
            ("A -> B +", "`+`"),
        ] {
            let err = declined(source);
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
    }

    #[test]
    fn participants_are_found_by_code_only() {
        // A display name in a message is a participant of its own.
        let d = diagram(
            "participant \"Bob\" as B\nA -> B\ncreate Bob\nA -> Bob\nparticipant \"Web app\" as Web\n\"Web app\" -> Auth",
        );
        assert_eq!(codes(&d), ["B", "A", "Bob", "Web", "Web app", "Auth"]);
        let creates: Vec<bool> = messages(&d).iter().map(|m| m.creates).collect();
        assert_eq!(creates, [false, true, false]);
        assert_eq!(messages(&d)[1].to, End::Participant(2));
        assert_eq!(messages(&d)[2].from, End::Participant(4));
    }

    #[test]
    fn marks_at_both_ends() {
        let d = diagram(
            "A o-> B\nA x-> B\nA o->o B\nA x->o B\n[o-> A\n[x-> A\nA o->]\nA ->o]\n[->o A\nA o<- B\nA ->x: to x",
        );
        let ends: Vec<(End, End, Mark, Mark)> = messages(&d)
            .iter()
            .map(|m| (m.from, m.to, m.arrow.start_mark, m.arrow.end_mark))
            .collect();
        let (a, b, x) = (
            End::Participant(0),
            End::Participant(1),
            End::Participant(2),
        );
        let (left, right) = (End::Edge(Side::Left), End::Edge(Side::Right));
        assert_eq!(
            ends,
            [
                (a, b, Mark::Circle, Mark::None),
                (a, b, Mark::Cross, Mark::None),
                (a, b, Mark::Circle, Mark::Circle),
                (a, b, Mark::Cross, Mark::Circle),
                (left, a, Mark::Circle, Mark::None),
                (left, a, Mark::Cross, Mark::None),
                (a, right, Mark::Circle, Mark::None),
                (a, right, Mark::None, Mark::Circle),
                (left, a, Mark::None, Mark::Circle),
                (b, a, Mark::None, Mark::Circle),
                (a, x, Mark::None, Mark::None),
            ]
        );
        assert_eq!(codes(&d), ["A", "B", "x"]);
        // A mark needs a space before it after a participant; `]`, `[` and
        // `?` touch the arrow.
        for (source, part) in [
            ("A ->o ]", "without a space"),
            ("A -> ]", "without a space"),
            ("[ -> A", "without a space"),
            ("\"A\"o-> B", "not part of"),
            ("A o- B", "not part of"),
        ] {
            let err = declined(source);
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
    }

    #[test]
    fn display_names_at_messages() {
        let d = diagram(
            "participant X as \"Shown\"\n\"Long name\" as L -> B : hi\nA -> C as \"Other\" ++\nL -> \"Long name\"\nA -> X as \"Ignored\"\nA -> \"New one\" as N ** : make\nA->\"Tight\"as T",
        );
        assert_eq!(codes(&d), ["X", "L", "B", "A", "C", "Long name", "N", "T"]);
        let labels: Vec<&str> = d.participants.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Shown",
                "Long name",
                "B",
                "A",
                "Other",
                "Long name",
                "New one",
                "Tight"
            ]
        );
        assert!(messages(&d)[4].creates);
        assert_eq!(messages(&d)[4].to, End::Participant(6));
        for (source, part) in [
            ("\"Long\" as L ->]", "between two participants"),
            ("[-> A as \"x\"", "between two participants"),
            ("A -> B as C", "`as C`"),
        ] {
            let err = declined(source);
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
    }

    #[test]
    fn creating_participants() {
        // The message after `create` creates; a note may come between.
        let d = diagram(
            "A -> B : hi\ncreate C\nA -> C : new\nA -> D ** : make\ncreate actor E\nnote over A : soon\n[-> E\ncreate F\nA -> F ** : both",
        );
        let creates: Vec<(End, bool)> = messages(&d).iter().map(|m| (m.to, m.creates)).collect();
        assert_eq!(codes(&d), ["A", "B", "C", "D", "E", "F"]);
        assert_eq!(
            creates,
            [
                (End::Participant(1), false),
                (End::Participant(2), true),
                (End::Participant(3), true),
                (End::Participant(4), true),
                (End::Participant(5), true),
            ]
        );
        assert_eq!(d.participants[4].shape, Shape::Actor);
        // PlantUML: "After create command, you have to send a message to".
        for (source, line, part) in [
            ("A -> B\ncreate B", 1, "after it was used"),
            ("A -> B\nA -> B **", 1, "after it was used"),
            ("create B\nB -> A : hello", 1, "next message must go to `B`"),
            (
                "A -> A : one\ncreate B\nA -> A : think\nA -> B : hi",
                2,
                "next message must go to `B`",
            ),
            (
                "create B\ncreate C\nA -> C",
                1,
                "next message must go to `B`",
            ),
            ("create B\nA -> C ** : x", 1, "next message must go to `B`"),
            ("create B\nB -> B : self", 1, "next message must go to `B`"),
            (
                "A -> B ++\ncreate C\nreturn",
                2,
                "next message must go to `C`",
            ),
            ("create B\nB ->] : out", 1, "not supported yet"),
            (
                "A -> B\ncreate C\nnote over A : x",
                1,
                "needs a message to `C`",
            ),
        ] {
            let err = declined(source);
            assert_eq!(err.line, Some(line), "{source}: {err:?}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
    }

    #[test]
    fn declaring_again_moves_to_the_end() {
        // PlantUML keeps how a participant first appeared and moves it to the end.
        let d = diagram("A -> B\nparticipant C\nactor \"Long\" as A #red\nA -> C");
        assert_eq!(codes(&d), ["B", "C", "A"]);
        let a = &d.participants[2];
        assert_eq!(
            (a.shape, a.label.as_str(), a.color.as_deref()),
            (Shape::Participant, "A", None)
        );
        let ends: Vec<(End, End)> = messages(&d).iter().map(|m| (m.from, m.to)).collect();
        assert_eq!(
            ends,
            [
                (End::Participant(2), End::Participant(0)),
                (End::Participant(2), End::Participant(1)),
            ]
        );
        // Out of its box and into the open one; named first in a box, it stands there.
        let d = diagram(
            "box X\nparticipant A\nparticipant B\nend box\nbox Y\nparticipant C\nC -> D\nparticipant A\nend box",
        );
        assert_eq!(codes(&d), ["B", "C", "D", "A"]);
        assert_eq!(d.boxes[0].participants, [0]);
        assert_eq!(d.boxes[1].participants, [1, 2, 3]);
        let err = declined("box X\nparticipant A\nend box\nparticipant A");
        assert_eq!(err.line, Some(3), "{err:?}");
        assert!(err.reason.contains("empties the box"), "{}", err.reason);
        // Names as PlantUML takes them.
        for (source, part) in [
            ("participant ->Api", "`->Api` is no participant name"),
            ("actor Us[x]er", "`Us[x]er` is no participant name"),
            ("participant \"W\" as W-1", "`W-1` is no participant name"),
            ("actor V <<>>", "a stereotype needs a text"),
            ("A -> B ++ #: x", "a color is missing"),
            ("note over A #: x", "a color is missing"),
        ] {
            let err = declined(source);
            assert_eq!(err.line, Some(0), "{source}: {err:?}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
        diagram("participant B@x.y_1\nactor Ärger << >>\nB@x.y_1 -> Ärger");
    }

    #[test]
    fn nothing_starts_or_ends_right_after_a_delay() {
        // PlantUML: "You cannot Activate/Deactivate just after a ...", until
        // the next message, whatever comes between.
        for (source, line, what) in [
            ("A -> B\n...\ncreate C\nA -> C", 2, "create"),
            ("A -> B\n... later ...\nactivate A", 2, "activate"),
            (
                "A -> B\nactivate B\n...\nnote over A : n\ndeactivate B",
                4,
                "deactivate",
            ),
            (
                "A -> B\n...\n== d ==\n|||\nref over A : r\ndestroy B",
                5,
                "destroy",
            ),
            ("A -> B\n...\nalt x\nactivate A\nend", 3, "activate"),
        ] {
            let err = declined(source);
            assert_eq!(err.line, Some(line), "{source}: {err:?}");
            assert!(
                err.reason.contains(&format!("`{what}`")),
                "{source}: {}",
                err.reason
            );
        }
        // Any message ends the delay, and one with `++` or `**` is a message first.
        for source in [
            "A -> B\n...\nA ->] : out\nactivate A",
            "A -> B\n...\nalt x\nA -> A : self\nend\nactivate A",
            "A -> B ++\n...\nreturn r\ndestroy B",
            "A -> B\n...\nA -> C ++ : x\nA -> D ** : new",
        ] {
            diagram(source);
        }
    }

    #[test]
    fn notes_everywhere() {
        let d = diagram(
            "A -> B\nnote left : on the message\nnote right of A : short\nnote over A, B #aqua\n  two\n  lines\nend note\nhnote over B : hex\nrnote across : all\nnote over \"B\": quoted",
        );
        let notes: Vec<&Note> = d
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Note(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(notes.len(), 6);
        assert_eq!(notes[0].place, NotePlace::Message(Side::Left));
        assert_eq!(notes[1].place, NotePlace::RightOf(0));
        assert_eq!(notes[2].place, NotePlace::Over(0, 1));
        assert_eq!(notes[2].text.value, "two\nlines");
        assert_eq!(notes[2].color.as_deref(), Some("#aqua"));
        assert_eq!(notes[3].shape, NoteShape::Hexagon);
        assert_eq!(
            (notes[4].shape, notes[4].place),
            (NoteShape::Rectangle, NotePlace::Across)
        );
        assert_eq!(notes[5].place, NotePlace::Over(1, 1));
        assert!(
            declined("note left : nothing before")
                .reason
                .contains("follow a message")
        );
        assert!(declined("note over A\nno end").reason.contains("no end"));
    }

    #[test]
    fn groups_dividers_delays_and_refs() {
        let d = diagram(
            "alt ok\n  A -> B\nelse failed\n  loop 3 times\n    A -> B\n  end\nend\ngroup audit [log]\nend\n== Phase 2 ==\n...\n... 5 minutes later ...\n|||\n||45||\nref over A, B : refresh\nref over B\n  two\nend ref",
        );
        let summary: Vec<String> = d
            .items
            .iter()
            .filter(|i| !matches!(i, Item::Message(_)))
            .map(|i| match i {
                Item::GroupStart {
                    kind,
                    label,
                    secondary,
                    ..
                } => format!(
                    "{} {:?} {:?}",
                    kind.keyword(),
                    label.as_ref().map(|t| t.value.as_str()),
                    secondary.as_ref().map(|t| t.value.as_str())
                ),
                Item::GroupElse { label, .. } => {
                    format!("else {:?}", label.as_ref().map(|t| t.value.as_str()))
                }
                Item::GroupEnd { .. } => "end".into(),
                Item::Divider { label } => format!("== {}", label.value),
                Item::Delay { label, .. } => {
                    format!("... {:?}", label.as_ref().map(|t| t.value.as_str()))
                }
                Item::Space { height, .. } => format!("||| {height:?}"),
                Item::Ref { over, text, .. } => format!("ref {over:?} {}", text.value),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(
            summary,
            [
                "alt Some(\"ok\") None",
                "else Some(\"failed\")",
                "loop Some(\"3 times\") None",
                "end",
                "end",
                "group Some(\"audit\") Some(\"log\")",
                "end",
                "== Phase 2",
                "... None",
                "... Some(\"5 minutes later\")",
                "||| None",
                "||| Some(45.0)",
                "ref [0, 1] refresh",
                "ref [1] two",
            ]
        );
        let unclosed = declined("@startuml\nalt x\nA -> B\n@enduml");
        assert_eq!(unclosed.line, Some(1));
        assert!(unclosed.reason.contains("`alt` has no `end`"));
        assert!(declined("end").reason.contains("without an open group"));
        assert!(declined("else").reason.contains("outside a group"));
        assert!(declined("alt#Gold x\nend").reason.contains("group colors"));
    }

    #[test]
    fn autonumber_formats() {
        let d = diagram(
            "autonumber\nA -> B\nA -> B\nautonumber 10 5 \"<b>[000]\"\nA -> B\nA -> B\nautonumber stop\nA -> B\nautonumber resume\nA -> B\nautonumber \"<color:#ff0000>##.\"\nA -> B",
        );
        let numbers: Vec<Option<&str>> = messages(&d).iter().map(|m| m.number.as_deref()).collect();
        assert_eq!(
            numbers,
            [
                Some("<b>1</b>"),
                Some("<b>2</b>"),
                Some("<b>[010]"),
                Some("<b>[015]"),
                None,
                Some("<b>[020]"),
                Some("<color:#ff0000>1."),
            ]
        );
    }

    #[test]
    fn legend_and_scale_can_be_participants() {
        let d =
            diagram("@startuml\nlegend -> scale : hi\nlegend\n  key\nendlegend\nscale 2\n@enduml");
        let names: Vec<&str> = d.participants.iter().map(|p| p.code.as_str()).collect();
        assert_eq!(names, ["legend", "scale"]);
        assert_eq!(d.frame.legend.unwrap().text.value, "key");
        assert_eq!(d.frame.scale, Some(crate::common::Scale::Factor(2.0)));
    }

    #[test]
    fn title_boxes_skinparams_and_footbox() {
        let d = diagram(
            "title\n  Login\n  flow\nend title\nheader draft\ncaption figure 1\nhide footbox\nbox \"Backend\" #LightBlue\n  participant API\n  database DB\nend box\nskinparam ArrowColor #ff0000\nskinparam participant {\n  BorderColor red\n  FontSize 12\n}\nAPI -> DB",
        );
        assert_eq!(d.frame.title.as_ref().unwrap().value, "Login\nflow");
        assert_eq!(d.frame.header.as_ref().unwrap().value, "draft");
        assert_eq!(d.frame.caption.as_ref().unwrap().value, "figure 1");
        assert!(!d.footbox);
        assert_eq!(d.boxes.len(), 1);
        assert_eq!(d.boxes[0].title.as_ref().unwrap().value, "Backend");
        assert_eq!(d.boxes[0].color.as_deref(), Some("#LightBlue"));
        assert_eq!(d.boxes[0].participants, [0, 1]);
        let params: Vec<(&str, &str)> = d
            .frame
            .skinparams
            .iter()
            .map(|s| (s.key.as_str(), s.value.as_str()))
            .collect();
        assert_eq!(
            params,
            [
                ("ArrowColor", "#ff0000"),
                ("participantBorderColor", "red"),
                ("participantFontSize", "12"),
            ]
        );
    }

    #[test]
    fn declines_what_it_does_not_know() {
        for (source, line, part) in [
            ("@startuml\nclass User {\n}\n@enduml", 1, "class User"),
            ("@startuml\n[*] --> A\n@enduml", 1, "[*]"),
            ("@startuml\n:act;\n@enduml", 1, ":act;"),
            ("@startuml\nnewpage\n@enduml", 1, "newpage"),
            (
                "@startuml\nlegend right top\nx\nendlegend\n@enduml",
                1,
                "`top` or `bottom`",
            ),
            ("@startuml\nlegend\nx\n@enduml", 1, "no end line"),
            ("@startuml\nmainframe\n@enduml", 1, "needs a text"),
            ("@startuml\nscale 0\n@enduml", 1, "factor"),
            ("@startuml\nscale 2 inches\n@enduml", 1, "not understood"),
            ("@startuml\nA -[thickness=3]-> B\n@enduml", 1, "thickness"),
            ("@startuml\nparticipant A order 10\n@enduml", 1, "order 10"),
            ("@startuml\nA -> B foo\n@enduml", 1, "foo"),
            ("@startuml\nhide stereotype\n@enduml", 1, "hide stereotype"),
            ("@startuml\nbox A\nbox B\n@enduml", 2, "inside a box"),
            ("@startuml\nautonumber 1.1.1\n@enduml", 1, "1.1.1"),
        ] {
            let err = declined(source);
            assert_eq!(err.line, Some(line), "{source}: {err:?}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
    }

    #[test]
    fn the_examples_parse() {
        let d = diagram(include_str!("../../../../examples/login.puml"));
        assert_eq!(codes(&d), ["User", "Web", "Auth", "Sessions"]);
        assert_eq!(messages(&d).len(), 10);
    }
}
