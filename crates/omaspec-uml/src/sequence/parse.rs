//! From the body's lines to a [`Diagram`]. A line the parser does not
//! understand declines the whole diagram, naming that line.

use super::{
    Arrow, Diagram, End, GroupKind, Head, Item, Mark, Message, Note, NotePlace, NoteShape,
    Participant, ParticipantBox, Shape, Side, Spot, Text,
};
use crate::Declined;
use crate::common::{Frame, block_text, keyword, snippet, squeezed_eq, take_link, text_at};
use crate::source::{Line, starts_with_ci};

/// Parses a sequence diagram from the lines [`crate::source::body`] gives.
pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut lines = lines.to_vec();
    let mut parser = Parser::default();
    let mut at = 0;
    while at < lines.len() {
        // `& A -> B` and `/ note over B`: the line is read without its
        // sign. Only a line that begins something has one: a `&` in the
        // text of a note over several lines is its text.
        let together = beside(lines[at].text).map(|text| lines[at].text = text);
        let before = parser.diagram.items.len();
        let next = parser.line(&lines, at)?;
        if together.is_some() {
            parser.together(before, lines[at].no)?;
        }
        at = next;
    }
    parser.finish()
}

/// The rest of a line that puts what it holds at the height of the line
/// before: `&` before anything (PlantUML's teoz), `/` before a note.
fn beside(text: &str) -> Option<&str> {
    if let Some(rest) = text.strip_prefix('&') {
        let rest = rest.trim_start();
        return (!rest.is_empty()).then_some(rest);
    }
    let rest = text.strip_prefix('/')?.trim_start();
    ["note", "hnote", "rnote"]
        .iter()
        .any(|word| keyword(rest, word).is_some())
        .then_some(rest)
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

/// The commands that are a diagram of their own, a page of what PlantUML
/// knows, with what that page shows.
const PAGES: [(&str, &str); 6] = [
    ("colors", "PlantUML's named colors"),
    ("skinparameters", "every skinparam"),
    ("listopeniconic", "the OpenIconic icons"),
    ("listsprite", "the sprites"),
    ("listsprites", "the sprites"),
    ("stdlib", "PlantUML's standard library"),
];

/// Keywords of PlantUML that Omaspec does not draw yet, with what to say.
const NOT_YET: [&str; 3] = ["skin", "left to right direction", "top to bottom direction"];

struct Parser {
    diagram: Diagram,
    frame: Frame,
    /// Whether a participant appeared in the diagram's items yet.
    used: Vec<bool>,
    /// Open groups: kind and line.
    groups: Vec<(GroupKind, usize)>,
    /// The open `box`, as an index into the diagram's boxes.
    open_boxes: Vec<usize>,
    autonumber: Autonumber,
    /// `autoactivate on`: a message opens a bar on its receiver, a reply
    /// closes its sender's.
    autoactivate: bool,
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
    /// `participant A order 10`: where it stands among the others, sorted
    /// in a stable way; those without an order count as 0.
    places: std::collections::HashMap<usize, i64>,
    /// Teoz's anchors, `{start} A -> B`, as the messages have named them.
    anchors: std::collections::HashSet<String>,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            diagram: Diagram {
                participants: Vec::new(),
                items: Vec::new(),
                boxes: Vec::new(),
                footbox: true,
                numbers_circled: false,
                hide_unlinked: false,
                frame: Frame::default(),
                names: Vec::new(),
            },
            frame: Frame::styled(super::skin::STYLE),
            used: Vec::new(),
            groups: Vec::new(),
            open_boxes: Vec::new(),
            autonumber: Autonumber::default(),
            autoactivate: false,
            activations: Vec::new(),
            last_message: None,
            pending_create: None,
            after_delay: false,
            order: Vec::new(),
            places: std::collections::HashMap::new(),
            anchors: std::collections::HashSet::new(),
        }
    }
}

struct Autonumber {
    on: bool,
    /// The levels before the one that counts (`autonumber 1.1.1` has two),
    /// which `autonumber inc A`, `inc B`, … raise.
    levels: Vec<i64>,
    next: i64,
    step: i64,
    format: Option<String>,
}

impl Default for Autonumber {
    fn default() -> Self {
        Self {
            on: false,
            levels: Vec::new(),
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
                    return Err(Declined::refused(no, "`end` without an open group"));
                }
                self.diagram.items.push(Item::GroupEnd { line: no });
                return Ok(at + 1);
            }
            "endbox" => {
                if self.open_boxes.pop().is_none() {
                    return Err(Declined::refused(no, "`end box` without a `box`"));
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
        if let Some(rest) = keyword(text, "newpage").filter(|r| !r.starts_with(['-', '<'])) {
            if let Some(&(kind, _)) = self.groups.last() {
                return Err(Declined::at(
                    no,
                    format!(
                        "a new page inside `{}` is not supported yet: end the group first",
                        kind.keyword()
                    ),
                ));
            }
            let title = rest.trim();
            self.diagram.items.push(Item::NewPage {
                title: (!title.is_empty()).then(|| text_at(title, no)),
                line: no,
            });
            return Ok(at + 1);
        }
        if let Some(rest) = keyword(text, "autoactivate") {
            self.autoactivate = match rest.trim().to_ascii_lowercase().as_str() {
                "on" => true,
                "off" => false,
                _ => return Err(Declined::at(no, "`autoactivate` takes `on` or `off`")),
            };
            return Ok(at + 1);
        }
        if let Some((shape, rest)) = shape_keyword(text) {
            // `participant P [`, its words on the lines up to the `]` that
            // closes it; words before that `]` are left out, as PlantUML
            // leaves them.
            if crate::common::description_opens(text) {
                let head = rest.trim_end().trim_end_matches('[').trim();
                if head.is_empty() || !head.chars().all(is_name_char) {
                    return Err(Declined::at(
                        no,
                        "a participant whose words are in brackets takes a plain name, as PlantUML does",
                    ));
                }
                let end = lines[at + 1..]
                    .iter()
                    .position(|l| crate::common::closes_description(l.text))
                    .map(|i| at + 1 + i)
                    .ok_or_else(|| {
                        Declined::at(no, "the participant's `[` has no `]` to end it")
                    })?;
                let body: Vec<&str> = lines[at + 1..end].iter().map(|l| l.text.trim()).collect();
                if body.iter().all(|l| l.is_empty()) {
                    return Err(Declined::at(
                        no,
                        "nothing stands between the participant's `[` and `]`",
                    ));
                }
                let before = self.diagram.participants.len();
                let index = self.declare(shape, head, no)?;
                // Declared again, it keeps the words it had.
                if self.diagram.participants.len() > before {
                    self.diagram.participants[index].label = body.join("\n");
                }
                return Ok(end + 1);
            }
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
        // `Box -> Item` is a message from a participant named Box.
        if let Some(rest) = keyword(text, "box")
            && !rest.starts_with(['-', '<', '.', '=', '\\', '/', '?'])
        {
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
        // Teoz: `{start} A -> B` names a message's height, and
        // `{start} <-> {end} : words` draws how long it takes between two.
        if let Some((anchor, rest)) = anchor_of(text) {
            return self.anchored(anchor, rest, no).map(|()| at + 1);
        }
        match split_message(text) {
            Some(Ok(raw)) => {
                self.message(raw, no)?;
                Ok(at + 1)
            }
            Some(Err(reason)) => Err(Declined::at(no, reason)),
            // Before any participant or message, the diagram is a
            // sequence diagram only because no other type claims it.
            // A command that draws a page of PlantUML's own.
            None if PAGES
                .iter()
                .any(|(word, _)| text.eq_ignore_ascii_case(word)) =>
            {
                let (word, page) = PAGES
                    .iter()
                    .find(|(word, _)| text.eq_ignore_ascii_case(word))
                    .copied()
                    .unwrap_or_default();
                Err(Declined::at(
                    no,
                    format!("`{word}` draws a page of {page}, which Omaspec does not draw yet"),
                ))
            }
            None if self.diagram.participants.is_empty() && self.diagram.items.is_empty() => {
                Err(Declined::at(
                    no,
                    format!(
                        "`{}` starts no diagram the native engine knows",
                        snippet(text)
                    ),
                ))
            }
            None => Err(Declined::at(
                no,
                format!(
                    "`{}` is not part of a sequence diagram the native engine can draw",
                    snippet(text)
                ),
            )),
        }
    }

    /// The line at `no` began with `&` or `/` and gave the items from
    /// `from` on: its message or note stands at the height of the one
    /// before.
    fn together(&mut self, from: usize, no: usize) -> Result<(), Declined> {
        let (earlier, new) = self.diagram.items.split_at_mut(from);
        // Without a message or a note before it there is no height to
        // share: the line is drawn as if it had no sign.
        let follows = earlier
            .iter()
            .any(|item| matches!(item, Item::Message(_) | Item::Note(_)));
        match new.first_mut() {
            Some(Item::Message(message)) => message.together = follows,
            Some(Item::Note(note)) => note.together = follows,
            _ => {
                return Err(Declined::at(
                    no,
                    "`&` and `/` go before a message or a note",
                ));
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Diagram, Declined> {
        // A group or a box left open ends with the diagram, as PlantUML
        // ends it; lint says which one has no `end`.
        while let Some((_, line)) = self.groups.pop() {
            self.diagram.items.push(Item::GroupEnd { line });
        }
        self.open_boxes.clear();
        if let Some((participant, line)) = self.pending_create {
            let code = &self.diagram.participants[participant].code;
            return Err(Declined::at(
                line,
                format!("`create {code}` needs a message to `{code}` after it"),
            ));
        }
        self.frame.pages = 1 + self
            .diagram
            .items
            .iter()
            .filter(|i| matches!(i, Item::NewPage { .. }))
            .count();
        self.diagram.frame = self.frame;
        let places = &self.places;
        self.order
            .sort_by_key(|index| places.get(index).copied().unwrap_or(0));
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
        self.diagram.names.push(crate::Mention::used(code, line));
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
            stereotype_spaced: false,
            spot: None,
            color: None,
            link: None,
            line,
        });
        self.used.push(false);
        let index = self.diagram.participants.len() - 1;
        self.order.push(index);
        // Named first inside a box, it stands in the box, as in PlantUML,
        // and in those around it.
        for &open in &self.open_boxes {
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
        let mut stereotype_spaced = false;
        let mut spot = None;
        let mut color = None;
        let mut link = None;
        let mut place = None;
        let mut rest = rest.trim_start();
        while !rest.is_empty() {
            if let Some(after) = keyword(rest, "order") {
                let token = after.split_whitespace().next().unwrap_or("");
                place = Some(i64::from(token.parse::<i32>().map_err(|_| {
                    Declined::at(no, "`order` takes a whole number, as `order 10`")
                })?));
                rest = after.trim_start()[token.len()..].trim_start();
                continue;
            }
            if let Some(after) = rest.strip_prefix("<<") {
                let end = after
                    .find(">>")
                    .ok_or_else(|| Declined::at(no, "a stereotype needs `>>`"))?;
                if end == 0 {
                    return Err(Declined::at(no, "a stereotype needs a text"));
                }
                (spot, stereotype) = spotted(&after[..end]);
                stereotype_spaced = after[..end].starts_with(' ') && after[..end].ends_with(' ');
                rest = after[end + 2..].trim_start();
            } else if rest.starts_with('#') {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                color = Some(rest[..end].to_owned());
                rest = rest[end..].trim_start();
            } else if rest.starts_with("[[")
                && let Some(end) = rest.find("]]")
            {
                let (_, taken) = take_link(&rest[..end + 2], no)?;
                link = taken;
                rest = rest[end + 2..].trim_start();
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

        self.diagram
            .names
            .push(crate::Mention::declared(&code, &label, no));
        let existing = self
            .diagram
            .participants
            .iter()
            .position(|p| p.code == code);
        let index = match existing {
            // PlantUML keeps how it first appeared and moves it to the
            // end, out of its box (into the open one, below).
            Some(index) => {
                // Only a box it leaves can be emptied by it: one that was
                // empty from the start stays as PlantUML draws it.
                let mut emptied = None;
                for (at, b) in self.diagram.boxes.iter_mut().enumerate() {
                    let held = b.participants.contains(&index);
                    b.participants.retain(|&i| i != index);
                    if held && b.participants.is_empty() && !self.open_boxes.contains(&at) {
                        emptied = Some(b.line);
                    }
                }
                if let Some(line) = emptied {
                    return Err(Declined::at(
                        no,
                        format!(
                            "declaring `{code}` again empties its box, which begins {}; that is not supported yet",
                            crate::common::lines_away(no, line)
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
                    stereotype_spaced,
                    spot,
                    color,
                    link,
                    line: no,
                });
                self.used.push(false);
                self.order.push(self.diagram.participants.len() - 1);
                self.diagram.participants.len() - 1
            }
        };
        for &open in &self.open_boxes {
            let members = &mut self.diagram.boxes[open].participants;
            if !members.contains(&index) {
                members.push(index);
            }
        }
        // Declared again, it keeps the place it was given first.
        if let Some(place) = place
            && existing.is_none()
        {
            self.places.insert(index, place);
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
                // The line that creates a participant declares it, unless
                // one above did.
                let known = self.diagram.participants.iter().any(|p| p.code == code);
                self.diagram.names.push(match known {
                    true => crate::Mention::used(&code, no),
                    false => crate::Mention::declared(&code, &code, no),
                });
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
            return Err(Declined::refused(
                no,
                format!("after a delay, PlantUML takes no `{what}` before the next message"),
            ));
        }
        Ok(())
    }

    fn not_created(&self, participant: usize, no: usize) -> Declined {
        let code = &self.diagram.participants[participant].code;
        Declined::refused(
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
            together: false,
            slant: 0.0,
            anchor: None,
            written_back: false,
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
        let mut shown: Vec<String> = auto.levels.iter().map(i64::to_string).collect();
        shown.push(n.to_string());
        let shown = shown.join(".");
        // PlantUML keeps a number in levels bold, whatever the format.
        Some(match &auto.format {
            Some(format) if auto.levels.is_empty() => number_with(format, &shown),
            _ => format!("<b>{shown}</b>"),
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
        if let Some(after) = keyword(rest, "inc") {
            return self.raise_level(after.trim(), no);
        }
        let (resume, mut rest) = match keyword(rest, "resume") {
            Some(after) => (true, after),
            None => (false, rest),
        };
        let mut levels = None;
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
            // `1.1.1`: the start, its levels before the one that counts.
            let leveled = token.contains('.') && numbers.is_empty() && levels.is_none() && !resume;
            if leveled {
                let parts: Option<Vec<i64>> = token.split('.').map(|p| p.parse().ok()).collect();
                let Some(mut parts) = parts.filter(|p| p.len() <= 26) else {
                    return Err(Declined::at(
                        no,
                        format!("`autonumber {}` is not supported yet", snippet(rest)),
                    ));
                };
                numbers.push(parts.pop().unwrap_or(1));
                levels = Some(parts);
                rest = &rest[token.len()..];
                continue;
            }
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
            auto.levels = levels.unwrap_or_default();
        }
        auto.on = true;
        Ok(())
    }

    /// `autonumber inc A`: the first level up by one, those after it and
    /// the count back to 1; `B` the second, and so on.
    fn raise_level(&mut self, letter: &str, no: usize) -> Result<(), Declined> {
        let auto = &mut self.autonumber;
        let level = match letter.as_bytes() {
            // Without a letter, the level before the count.
            [] => auto.levels.len().checked_sub(1),
            [b] if b.is_ascii_alphabetic() => Some(usize::from(b.to_ascii_uppercase() - b'A')),
            _ => return Err(Declined::at(no, "`autonumber inc` takes a letter: A, B, …")),
        };
        match level {
            Some(level) if level < auto.levels.len() => {
                auto.levels[level] = auto.levels[level].saturating_add(1);
                for lower in &mut auto.levels[level + 1..] {
                    *lower = 1;
                }
                auto.next = 1;
            }
            // A number without levels skips one, as PlantUML counts; on
            // the count's own level it goes on as it goes.
            _ if auto.levels.is_empty() => auto.next = auto.next.saturating_add(1),
            _ => {}
        }
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
                self.diagram.names.push(crate::Mention::used(&code, no));
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
            together: false,
            slant: raw.slant,
            anchor: None,
            written_back: parts.left_head != Head::None && parts.right_head == Head::None,
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
        // `autoactivate on`, where the message says nothing of bars itself
        // and is not lost (`->x`): a call opens one on its receiver, in the
        // color after it, a reply closes its sender's. Between a participant
        // and the edge, the participant is the one, whichever way it points.
        if self.autoactivate && life.steps.is_empty() && end_mark != Mark::Cross {
            let lone = match (from, to) {
                (End::Participant(_), End::Participant(_)) => None,
                (End::Participant(p), _) | (_, End::Participant(p)) => Some(p),
                _ => None,
            };
            let (opens, closes) = match (lone, from, to) {
                (Some(p), ..) => (Some(p), Some(p)),
                (None, End::Participant(sender), End::Participant(receiver)) => {
                    (Some(receiver), Some(sender))
                }
                _ => (None, None),
            };
            match (parts.dashed, opens, closes) {
                (false, Some(p), _) => self.activate(p, life.color.clone(), no),
                (true, _, Some(p)) if self.activations.iter().any(|(q, _)| *q == p) => {
                    self.deactivate(p, no);
                }
                _ => {}
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
        // Where a note beside a message goes among the items.
        let mut beside_at = None;
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
            // `note left S1` is `note left of S1`, where S1 is there.
            let beside = keyword(rest, "of").or_else(|| {
                let (code, _) = name(rest.trim_start())?;
                self.diagram
                    .participants
                    .iter()
                    .any(|p| p.code == code)
                    .then_some(rest)
            });
            match beside {
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
                    // Right after the message, before the bars it opens
                    // or closes: the drawer takes those after the note.
                    let Some(message) = self.last_message_index() else {
                        return Err(Declined::at(
                            no,
                            "a note without `of` must follow a message",
                        ));
                    };
                    beside_at = Some(message + 1);
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
        let (text, next) = if let Some(single) = rest.trim_start().strip_prefix(':') {
            (text_at(single.trim(), no), at + 1)
        } else if rest.trim().is_empty() {
            block_text(lines, at, "note", |l| {
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
        let note = Item::Note(Note {
            shape,
            place,
            text,
            color,
            together: false,
            line: no,
        });
        match beside_at {
            Some(at) => self.diagram.items.insert(at, note),
            None => self.diagram.items.push(note),
        }
        Ok(Some(next))
    }

    /// A line that starts with an anchor (`{start}`): the message it names,
    /// or a duration from it to another.
    fn anchored(&mut self, anchor: &str, rest: &str, no: usize) -> Result<(), Declined> {
        if let Some(after) = rest.strip_prefix("<->") {
            let Some((to, rest)) = anchor_of(after.trim_start()) else {
                return Err(Declined::at(
                    no,
                    "a duration goes between two anchors, as `{start} <-> {end} : words`",
                ));
            };
            if anchor == to {
                return Err(Declined::at(
                    no,
                    format!("a duration goes between two anchors, not `{{{to}}}` twice"),
                ));
            }
            for name in [anchor, to] {
                if !self.anchors.contains(name) {
                    return Err(Declined::at(
                        no,
                        format!("`{{{name}}}` names no message before this line"),
                    ));
                }
            }
            let label = match rest.trim() {
                "" => None,
                words => match words.strip_prefix(':') {
                    Some(words) => (!words.trim().is_empty()).then(|| text_at(words.trim(), no)),
                    None => {
                        return Err(Declined::at(
                            no,
                            format!("unexpected `{}` after the duration", snippet(words)),
                        ));
                    }
                },
            };
            self.diagram.items.push(Item::Duration {
                from: anchor.to_owned(),
                to: to.to_owned(),
                label,
                line: no,
            });
            return Ok(());
        }
        match split_message(rest) {
            Some(Ok(raw)) => {
                if !self.anchors.insert(anchor.to_owned()) {
                    return Err(Declined::at(
                        no,
                        format!("`{{{anchor}}}` names a message already"),
                    ));
                }
                self.message(raw, no)?;
                let at = self
                    .diagram
                    .items
                    .iter()
                    .rposition(|item| matches!(item, Item::Message(_)));
                if let Some(Item::Message(m)) = at.map(|at| &mut self.diagram.items[at]) {
                    m.anchor = Some(anchor.to_owned());
                }
                Ok(())
            }
            Some(Err(reason)) => Err(Declined::at(no, reason)),
            None => Err(Declined::at(
                no,
                format!("an anchor (`{{{anchor}}}`) goes before a message"),
            )),
        }
    }

    /// The last item if it is a message, or the message the bars after
    /// it belong to (`++`, `--`, `!!`, `autoactivate`).
    fn last_message_index(&self) -> Option<usize> {
        let items = &self.diagram.items;
        let at = items.iter().rposition(|item| {
            !matches!(
                item,
                Item::Activate { .. } | Item::Deactivate { .. } | Item::Destroy { .. }
            )
        })?;
        matches!(items[at], Item::Message(_)).then_some(at)
    }

    /// `ref over A, B : text`, or the text on the lines up to `end ref`.
    fn reference(&mut self, lines: &[Line], at: usize, rest: &str) -> Result<usize, Declined> {
        let no = lines[at].no;
        let rest = keyword(rest, "over")
            .ok_or_else(|| Declined::at(no, "a reference is written `ref over A, B : text`"))?;
        let (over, rest) = self.names(rest, no)?;
        let (color, rest) =
            leading_color(rest.trim_start()).map_err(|reason| Declined::at(no, reason))?;
        let (text, next) = if let Some(single) = rest.trim_start().strip_prefix(':') {
            (text_at(single.trim(), no), at + 1)
        } else if rest.trim().is_empty() {
            block_text(lines, at, "ref", |l| squeezed_eq(l, "endref"))?
        } else {
            return Err(Declined::at(
                no,
                format!(
                    "`{}` in a reference is not supported yet",
                    snippet(rest.trim())
                ),
            ));
        };
        self.diagram.items.push(Item::Ref { over, text, color });
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
            title: title.map(|t| Text {
                value: t,
                line: no,
                col: None,
            }),
            color,
            line: no,
            participants: Vec::new(),
            depth: self.open_boxes.len(),
        });
        self.open_boxes.push(self.diagram.boxes.len() - 1);
        Ok(())
    }

    /// Group starts and `else`; `false` if the line is neither.
    fn group(&mut self, text: &str, no: usize) -> Result<bool, Declined> {
        for (word, kind) in GROUPS {
            // `alt#Gold`: the tab's color, right after the word.
            let tabbed = starts_with_ci(text, word)
                .then(|| group_color(&text[word.len()..]))
                .flatten();
            let (tab, rest) = if let Some((tab, rest)) = tabbed {
                (Some(tab), rest.trim_start())
            } else if let Some(rest) = keyword(text, word) {
                // `Loop -> Bob` is a message from a participant named Loop.
                if rest.starts_with(['-', '<']) {
                    continue;
                }
                (None, rest)
            } else {
                continue;
            };
            // `alt #LightBlue`: the color behind its first part.
            let (fill, rest) = match group_color(rest) {
                Some((fill, rest)) => (Some(fill), rest.trim_start()),
                None => (None, rest),
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
                tab,
                fill,
                line: no,
            });
            self.groups.push((kind, no));
            return Ok(true);
        }
        if let Some(rest) = keyword(text, "else") {
            if self.groups.is_empty() {
                return Err(Declined::refused(no, "`else` outside a group"));
            }
            let (fill, rest) = match group_color(rest) {
                Some((fill, rest)) => (Some(fill), rest.trim()),
                None => (None, rest.trim()),
            };
            self.diagram.items.push(Item::GroupElse {
                label: (!rest.is_empty()).then(|| text_at(rest, no)),
                fill,
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
        // `Participant -> Bob` is a message from a participant so named.
        .filter(|(_, rest)| !rest.starts_with(['-', '<']))
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

/// A group's color at the start of `text`, as PlantUML takes it: `#`
/// and word characters (`#Gold`, `#FF0000`); `# of retries` is words.
fn group_color(text: &str) -> Option<(String, &str)> {
    let after = text.strip_prefix('#')?;
    let end = after
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(after.len());
    let rest = &after[end..];
    (end > 0 && (rest.is_empty() || rest.starts_with(char::is_whitespace)))
        .then(|| (text[..end + 1].to_owned(), rest))
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
fn number_with(format: &str, n: &str) -> String {
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
    let (sign, digits) = n.strip_prefix('-').map_or(("", n), |digits| ("-", digits));
    let zeros = "0".repeat(width.saturating_sub(digits.len()));
    format!(
        "{}{sign}{zeros}{digits}{}",
        &format[..start],
        &format[end..]
    )
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
    /// `(10)` before or after the arrow.
    slant: f32,
    /// After the second end: `++` and the like, `: text`.
    rest: &'a str,
}

/// Whether a sequence diagram takes `text` for a message, one it draws
/// or one it declines. PlantUML tries the sequence diagram first, so a
/// line between two names that is no message here makes another type.
pub(crate) fn reads_as_message(text: &str) -> bool {
    let text = anchor_of(text).map_or(text, |(_, rest)| rest);
    split_message(text).is_some()
}

/// `{name}` at the start of `text`, Teoz's anchor, and the rest.
fn anchor_of(text: &str) -> Option<(&str, &str)> {
    let inner = text.strip_prefix('{')?;
    let close = inner.find('}')?;
    let name = &inner[..close];
    (!name.is_empty() && name.chars().all(is_name_char))
        .then(|| (name, inner[close + 1..].trim_start()))
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
    let (before, arrow_text) = match slant(arrow_text) {
        Some((fall, rest)) => (Some(fall), rest),
        None => (None, arrow_text),
    };
    let (arrow, rest) = match arrow(arrow_text, may_mark || before.is_some())? {
        Ok(found) => found,
        Err(reason) => return Some(Err(reason)),
    };
    let (after, rest) = match slant(rest) {
        Some((fall, rest)) => (Some(fall), rest),
        None => (None, rest),
    };
    // As PlantUML takes it: right after a head on the right, or right
    // before one on the left.
    let headed =
        |head: Head, mark: Mark| matches!(head, Head::Filled | Head::Open) && mark == Mark::None;
    let placed = match (&before, &after) {
        (Some(_), Some(_)) => {
            return Some(Err(
                "a slant goes before the arrow or after it, not both".into()
            ));
        }
        (Some(_), None) => headed(arrow.left_head, arrow.left_mark),
        (None, Some(_)) => headed(arrow.right_head, arrow.right_mark),
        (None, None) => true,
    };
    let slant = match (before, after) {
        (Some(Err(reason)), _) | (_, Some(Err(reason))) => return Some(Err(reason)),
        (Some(Ok(fall)), _) | (_, Some(Ok(fall))) => fall,
        (None, None) => 0.0,
    };
    if !placed {
        return Some(Err(SLANT_PLACE.into()));
    }
    // PlantUML draws a slant level between two open heads.
    let slant = if arrow.left_head == Head::Open && arrow.right_head == Head::Open {
        0.0
    } else {
        slant
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
    if slant != 0.0 && !(named(&left) && named(&right)) {
        return Some(Err(SLANT_PLACE.into()));
    }
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
        slant,
        rest,
    }))
}

/// The most a slanted arrow may fall.
const MOST_SLANT: f32 = 1000.0;

/// Where a slant may stand, as PlantUML takes it.
const SLANT_PLACE: &str = "a slant goes right after a head on the right (`->(10)`) or right before one on the left (`(10)<-`), between two participants";

/// `(10)` at the start of `text`, beside an arrow: how many pixels it
/// falls, and the rest.
fn slant(text: &str) -> Option<(Result<f32, String>, &str)> {
    let inner = text.strip_prefix('(')?;
    let close = inner.find(')')?;
    let digits = &inner[..close];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let fall = digits
        .parse::<f32>()
        .ok()
        .filter(|fall| *fall <= MOST_SLANT)
        .ok_or_else(|| format!("a slant falls {MOST_SLANT} pixels at most"));
    Some((fall, &inner[close + 1..]))
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
    fn a_sign_puts_a_line_at_the_height_of_the_one_before() {
        let d = diagram(
            "@startuml\nA -> B : one\n& B -> C ++ : two\nnote over A : a\n/ hnote over B\nb\nend note\n&note over C : c\nA -> B : alone\n@enduml",
        );
        let together: Vec<bool> = d
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Message(m) => Some(m.together),
                Item::Note(note) => Some(note.together),
                _ => None,
            })
            .collect();
        assert_eq!(together, [false, true, false, true, true, false]);
        // The line is read as it is without its sign.
        assert_eq!(messages(&d)[1].text.as_ref().unwrap().value, "two");
        assert!(matches!(d.items[2], Item::Activate { .. }));

        // Nothing before it: no height to share.
        let d = diagram("@startuml\n& A -> B\n@enduml");
        assert!(!messages(&d)[0].together);

        for source in [
            "A -> B\n& alt x\nend",
            "A -> B\n& activate B",
            "A -> B\n& ...",
        ] {
            let e = declined(&format!("@startuml\n{source}\n@enduml"));
            assert_eq!(e.line, Some(2), "{source}");
            assert!(e.reason.contains("before a message or a note"), "{e:?}");
        }
        // `/` alone is no sign: only before a note.
        let e = declined("@startuml\nA -> B\n/ A -> B\n@enduml");
        assert_eq!(e.line, Some(2));
    }

    /// A `&` or a `/` that begins a line of a note's text is its text.
    #[test]
    fn a_sign_in_a_note_over_lines_is_its_text() {
        let d = diagram("@startuml\nA -> B\nnote over A\n& more\n/ note this\nend note\n@enduml");
        let notes: Vec<&Note> = d
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Note(note) => Some(note),
                _ => None,
            })
            .collect();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].text.value, "& more\n/ note this");
        assert!(!notes[0].together);
    }

    /// REQ-080: `title` and a no-break space is a title that shows
    /// nothing, and a character nobody sees is written out in a message.
    #[test]
    fn spaces_and_marks_nobody_sees() {
        let d = diagram("@startuml\ntitle \u{a0}\nA -> B\n@enduml");
        assert_eq!(d.frame.title.as_ref().unwrap().value, "\u{a0}");
        // At the end of a word it is no text: the name is `A`.
        let d = diagram("@startuml\nparticipant A\u{a0}\nA -> B\n@enduml");
        assert_eq!(codes(&d), ["A", "B"]);
        // Nor after a space at the end of any other line.
        let d = diagram("@startuml\nparticipant A \u{a0}\nA -> B : hi \u{a0}\n@enduml");
        assert_eq!(codes(&d), ["A", "B"]);
        assert_eq!(messages(&d)[0].text.as_ref().unwrap().value, "hi");
        let d = diagram("@startuml\nHeader \u{a0}\nA -> B\n@enduml");
        assert_eq!(d.frame.header.as_ref().unwrap().value, "\u{a0}");
        let e = declined("@startuml\ntitle\nA -> B\n@enduml");
        assert!(e.reason.contains("has no end line"), "{e:?}");

        let e = declined("@startuml\nparticipant Al\u{200b}ice\n@enduml");
        assert!(
            e.reason
                .starts_with("`Al<U+200B>ice` is no participant name"),
            "{e:?}"
        );
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
            ("A -> B\n\"A\"o-> B", "not part of"),
            ("A -> B\nA o- B", "not part of"),
            // Before any message, no type claims the line.
            ("A o- B", "starts no diagram"),
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
        // A box empty from the start is not one it empties.
        let other = parse(
            &body("box X\nend box\nparticipant A\nparticipant A")
                .unwrap()
                .1,
        );
        assert!(
            other
                .as_ref()
                .err()
                .is_none_or(|err| !err.reason.contains("empties its box")),
            "{other:?}"
        );
        assert!(
            err.reason
                .contains("empties its box, which begins 3 lines above"),
            "{}",
            err.reason
        );
        // Names as PlantUML takes them.
        for (source, part) in [
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
        // A group left open ends with the diagram, as PlantUML ends it.
        let ends = |source: &str| {
            let d = diagram(source);
            let count = |end: bool| {
                d.items
                    .iter()
                    .filter(|item| match item {
                        Item::GroupEnd { .. } => end,
                        Item::GroupStart { .. } => !end,
                        _ => false,
                    })
                    .count()
            };
            (count(false), count(true))
        };
        assert_eq!(ends("alt x\nA -> B\nloop y\nB -> A"), (2, 2));
        assert_eq!(ends("alt x\nA -> B\nend"), (1, 1));
        assert!(declined("end").reason.contains("without an open group"));
        assert!(declined("else").reason.contains("outside a group"));
        let d =
            diagram("alt#Gold #LightBlue yes\nA -> B\nelse #Pink no\nA -> B\nend\nalt plain\nend");
        let colors: Vec<(Option<&str>, Option<&str>)> = d
            .items
            .iter()
            .filter_map(|i| match i {
                Item::GroupStart { tab, fill, .. } => Some((tab.as_deref(), fill.as_deref())),
                Item::GroupElse { fill, .. } => Some((None, fill.as_deref())),
                _ => None,
            })
            .collect();
        assert_eq!(
            colors,
            [
                (Some("#Gold"), Some("#LightBlue")),
                (None, Some("#Pink")),
                (None, None)
            ]
        );
    }

    #[test]
    fn an_order_places_a_participant_and_brackets_hold_its_words() {
        let d = diagram(concat!(
            "participant A\nparticipant B order 5\nparticipant C order -1\n",
            "participant D\nparticipant E order 0\nA -> D",
        ));
        assert_eq!(codes(&d), ["C", "A", "D", "E", "B"]);
        let d = diagram("participant P [\n  first\n  second\n]\nparticipant Q\nP -> Q");
        assert_eq!(codes(&d), ["P", "Q"]);
        assert_eq!(d.participants[0].label, "first\nsecond");
    }

    #[test]
    fn a_participant_named_as_a_keyword_sends_messages() {
        let d =
            diagram("participant Participant\nparticipant Bob\nParticipant -> Bob\nActor <- Bob");
        assert_eq!(codes(&d), ["Participant", "Bob", "Actor"]);
        assert_eq!(messages(&d).len(), 2);
        let d = diagram("participant ->Api");
        assert_eq!(codes(&d), ["participant", "Api"]);
    }

    #[test]
    fn a_box_inside_a_box_holds_its_participants_in_both() {
        let d = diagram(concat!(
            "box \"Service\" #LightBlue\nparticipant Bob\nbox \"Team\"\nparticipant Alice\n",
            "participant John\nend box\nend box\nparticipant Other\nBob -> Alice\n",
        ));
        let boxes: Vec<(usize, Vec<&str>)> = d
            .boxes
            .iter()
            .map(|b| {
                let codes = b
                    .participants
                    .iter()
                    .map(|&p| d.participants[p].code.as_str())
                    .collect();
                (b.depth, codes)
            })
            .collect();
        assert_eq!(
            boxes,
            [
                (0, vec!["Bob", "Alice", "John"]),
                (1, vec!["Alice", "John"])
            ]
        );
    }

    #[test]
    fn what_the_review_of_the_skinparams_and_boxes_found() {
        // Only `#` and word characters are a group's color.
        let d =
            diagram("alt # of retries > 3\nA -> B\nelse # second\nA -> B\nend\nalt#Gold ok\nend");
        let labels: Vec<(Option<&str>, Option<&str>)> = d
            .items
            .iter()
            .filter_map(|i| match i {
                Item::GroupStart { label, fill, .. } => {
                    Some((label.as_ref().map(|l| l.value.as_str()), fill.as_deref()))
                }
                Item::GroupElse { label, fill, .. } => {
                    Some((label.as_ref().map(|l| l.value.as_str()), fill.as_deref()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                (Some("# of retries > 3"), None),
                (Some("# second"), None),
                (Some("ok"), None)
            ]
        );
        // Written with its head on the left, whichever way it points.
        let d = diagram("participant A\nparticipant B\nB <- A\nB --> A\nA <-> B");
        let back: Vec<bool> = messages(&d).iter().map(|m| m.written_back).collect();
        assert_eq!(back, [true, false, false]);
        // A stereotype keeps the spaces it was written with.
        let d = diagram("participant A << spaced >>\nparticipant B <<tight>>");
        let spaced: Vec<bool> = d.participants.iter().map(|p| p.stereotype_spaced).collect();
        assert_eq!(spaced, [true, false]);
        assert!(declined("{a} A -> B\n{a} <-> {a}").reason.contains("twice"));
    }

    #[test]
    fn anchors_name_messages_and_a_duration_spans_two() {
        let d = diagram(concat!(
            "{start} A -> B : go\nB -> C\n{end} B -> A : back\n",
            "{start} <-> {end} : some time\n",
        ));
        let anchors: Vec<Option<&str>> = messages(&d).iter().map(|m| m.anchor.as_deref()).collect();
        assert_eq!(anchors, [Some("start"), None, Some("end")]);
        let Some(Item::Duration {
            from, to, label, ..
        }) = d.items.last()
        else {
            panic!("{:?}", d.items.last());
        };
        assert_eq!((from.as_str(), to.as_str()), ("start", "end"));
        assert_eq!(label.as_ref().unwrap().value, "some time");
        for (source, reason) in [
            ("{a} A -> B\n{a} <-> {b}", "names no message"),
            ("{a} A -> B\n{a} B -> A", "already"),
            ("{a} note left: x", "before a message"),
            ("{a} A -> B\n{a} <-> x", "between two anchors"),
        ] {
            let err = declined(source);
            assert!(err.reason.contains(reason), "{source}: {}", err.reason);
        }
    }

    #[test]
    fn what_the_review_of_pages_and_participants_found() {
        // A note beside a message whose bars autoactivate opened.
        let d =
            diagram("autoactivate on\nA -> B : hi\nnote right: one\nB --> A : ok\nnote left: two");
        let notes: Vec<(usize, NotePlace)> = d
            .items
            .iter()
            .enumerate()
            .filter_map(|(at, i)| match i {
                Item::Note(n) => Some((at, n.place)),
                _ => None,
            })
            .collect();
        assert_eq!(notes.len(), 2);
        for (at, place) in notes {
            assert!(matches!(d.items[at - 1], Item::Message(_)), "{at}");
            assert!(matches!(place, NotePlace::Message(_)));
        }
        // Lost messages open no bar; at the edge the participant is the one.
        let bars = |source: &str| {
            diagram(source)
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Activate { participant, .. } => Some(format!("on {participant}")),
                    Item::Deactivate { participant, .. } => Some(format!("off {participant}")),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(bars("autoactivate on\nA ->x B").is_empty());
        assert_eq!(bars("autoactivate on\nA -> B\nB -->x A"), ["on 1"]);
        assert_eq!(bars("autoactivate on\nA ->]"), ["on 0"]);
        assert_eq!(bars("autoactivate on\nA -> B\n[--> B"), ["on 1", "off 1"]);
        // Declared again, a participant keeps its first place and words.
        let d = diagram("participant A\nparticipant B\nparticipant A order -5\nA -> B");
        assert_eq!(codes(&d), ["B", "A"]);
        let d = diagram("participant P\nparticipant Q\nparticipant P [\nX\n]\nP -> Q");
        assert_eq!(
            d.participants.iter().find(|p| p.code == "P").unwrap().label,
            "P"
        );
        // A bracket closes where a `]` is not opened on its line.
        let d = diagram("participant P [\nalpha [x]\nbeta]\nP -> R : msg");
        assert_eq!(d.participants[0].label, "alpha [x]");
        assert_eq!(messages(&d).len(), 1);
        // A participant named `newpage`.
        let d = diagram("A -> B : one\nnewpage -> A : hi");
        assert!(!d.items.iter().any(|i| matches!(i, Item::NewPage { .. })));
        // A number in levels stays bold whatever the format; a sign pads.
        let d = diagram("autonumber 1.1.1 \"<b>[000]\"\nA -> B\nautonumber -5 \"000\"\nA -> B");
        let numbers: Vec<&str> = messages(&d)
            .iter()
            .map(|m| m.number.as_deref().unwrap())
            .collect();
        assert_eq!(numbers, ["<b>1.1.1</b>", "-005"]);
    }

    #[test]
    fn numbers_in_levels_raise_and_reset() {
        let d = diagram(concat!(
            "autonumber 1.1.1\nA -> B\nB --> A\n",
            "autonumber inc A\nA -> B\n",
            "autonumber inc B\nA -> B\nB --> A\n",
            "autonumber inc A\nA -> B\n",
            "autonumber 10\nA -> B\n",
        ));
        let numbers: Vec<&str> = messages(&d)
            .iter()
            .map(|m| m.number.as_deref().unwrap())
            .collect();
        assert_eq!(
            numbers,
            [
                "<b>1.1.1</b>",
                "<b>1.1.2</b>",
                "<b>2.1.1</b>",
                "<b>2.2.1</b>",
                "<b>2.2.2</b>",
                "<b>3.1.1</b>",
                "<b>10</b>"
            ]
        );
        let d = diagram("autonumber 3 \"<b>[000]\"\nA -> B");
        assert_eq!(messages(&d)[0].number.as_deref(), Some("<b>[003]"));
    }

    #[test]
    fn autoactivate_opens_a_bar_at_each_call() {
        let d = diagram(concat!(
            "autoactivate on\n",
            "alice -> bob : hello\n",
            "bob -> bob : self\n",
            "bill -> bob #005500 : thread\n",
            "bob -> george ** : create\n",
            "return done\n",
            "return rc\n",
            "bob --> alice : reply\n",
            "autoactivate off\n",
            "alice -> bob : plain\n",
        ));
        let name = |end: &End| match end {
            End::Participant(p) => d.participants[*p].code.clone(),
            other => format!("{other:?}"),
        };
        let seen: Vec<String> = d
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Message(m) => Some(format!("msg {}>{}", name(&m.from), name(&m.to))),
                Item::Activate {
                    participant, color, ..
                } => Some(match color {
                    Some(color) => format!("on {} {color}", d.participants[*participant].code),
                    None => format!("on {}", d.participants[*participant].code),
                }),
                Item::Deactivate { participant, .. } => {
                    Some(format!("off {}", d.participants[*participant].code))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            seen,
            [
                "msg alice>bob",
                "on bob",
                "msg bob>bob",
                "on bob",
                "msg bill>bob",
                "on bob #005500",
                "msg bob>george",
                "msg bob>bill",
                "off bob",
                "msg bob>bob",
                "off bob",
                "msg bob>alice",
                "off bob",
                "msg alice>bob",
            ]
        );
    }

    #[test]
    fn a_note_beside_a_participant_may_leave_out_of() {
        let d = diagram("A -> B\nnote left: one\nnote left A: two\nnote right B : three");
        let places: Vec<NotePlace> = d
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Note(n) => Some(n.place),
                _ => None,
            })
            .collect();
        assert_eq!(
            places,
            [
                NotePlace::Message(Side::Left),
                NotePlace::LeftOf(0),
                NotePlace::RightOf(1)
            ]
        );
    }

    #[test]
    fn a_slant_falls_from_sender_to_receiver() {
        let d = diagram("A ->(10) B : down\nA (20)<- B\nA -> B\nA -->(5) B");
        let falls: Vec<f32> = messages(&d).iter().map(|m| m.slant).collect();
        assert_eq!(falls, [10.0, 20.0, 0.0, 5.0]);
        assert_eq!(messages(&d)[1].from, End::Participant(1));
        for (source, reason) in [
            ("A (10)->(20) B", "not both"),
            ("A ->(1001) B", "at most"),
            ("A (10)-> B", "right before one on the left"),
            ("A <-(10) B", "right after a head on the right"),
            ("A (10)x<- B", "right before"),
            ("A -\\(10) B", "right after"),
            ("[->(10) A", "between two participants"),
            ("A ->(10)]", "between two participants"),
        ] {
            let err = declined(source);
            assert!(err.reason.contains(reason), "{source}: {}", err.reason);
        }
        // Words in parentheses are no slant.
        let d = diagram("A -> B : (10) seconds");
        assert_eq!(messages(&d)[0].slant, 0.0);
        // Between two open heads PlantUML keeps it level.
        let d = diagram("A <->(10) B\nA <<->>(10) B\nA o->(10) B");
        let falls: Vec<f32> = messages(&d).iter().map(|m| m.slant).collect();
        assert_eq!(falls, [10.0, 0.0, 10.0]);
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
            (
                "@startuml\nalt yes\nA -> B\nnewpage\nend\n@enduml",
                3,
                "inside `alt`",
            ),
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
            (
                "@startuml\nparticipant A order ten\n@enduml",
                1,
                "whole number",
            ),
            ("@startuml\nparticipant A [\nx\n@enduml", 1, "no `]`"),
            ("@startuml\nA -> B foo\n@enduml", 1, "foo"),
            ("@startuml\nhide stereotype\n@enduml", 1, "hide stereotype"),
            (
                "@startuml\nautonumber\nautonumber inc AB\n@enduml",
                2,
                "takes a letter",
            ),
            (
                "@startuml\nparticipant P as Q [\nx\n]\n@enduml",
                1,
                "plain name",
            ),
            (
                "@startuml\nparticipant P [\n]\n@enduml",
                1,
                "nothing stands",
            ),
            (
                "@startuml\nparticipant A order 99999999999\n@enduml",
                1,
                "whole number",
            ),
            ("@startuml\nautoactivate maybe\n@enduml", 1, "`on` or `off`"),
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
