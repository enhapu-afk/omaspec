//! Sequence diagrams between the languages. Both read into the sequence
//! engine's tree, and each is written from there: participants first, in
//! their columns and boxes, then the messages, activations, notes and
//! blocks in order.

use std::collections::{HashMap, HashSet};

use omaspec_core::Language;
use omaspec_uml::sequence::{
    Arrow, Diagram, End, GroupKind, Head, Item, Mark, NotePlace, NoteShape, Shape,
};

use super::{
    Converted, Notice, Refused, is_plain, not_the_same, plain_id, refused, to_mermaid_text,
    to_plantuml_text,
};
use crate::{across, diff};

/// What a `&` or a `/` before a message or a note loses.
const TOGETHER: &str = "Mermaid draws each message and note at a height of its own";

/// What a group's colors become in Mermaid.
const GROUP_COLORS: &str = "Mermaid has no colors on a group's tab or parts; they are left out";
const UNREAD: &str = "Omaspec does not fully read this sequence diagram, so it cannot convert it";

pub(super) fn convert(code: &str, from: Language, to: Language) -> Result<Converted, Refused> {
    let (diagram, offset) =
        across::read_sequence(code, from).ok_or_else(|| refused(None, UNREAD))?;
    let mut w = Writer {
        offset,
        ..Writer::default()
    };
    w.name_all(&diagram);
    match to {
        Language::Mermaid => w.mermaid(&diagram)?,
        Language::PlantUml => w.plantuml(&diagram)?,
    }
    let mut text = w.out.join("\n");
    text.push('\n');
    let mut original = diagram;
    for p in &mut original.participants {
        p.code.clone_from(&w.ids[&p.code]);
    }
    let (back, back_offset) =
        across::read_sequence(&text, to).ok_or_else(|| not_the_same(to, None))?;
    let changes = diff::scenarios(&original, offset, &back, back_offset)
        .map_err(|reason| refused(None, reason))?;
    if let Some(change) = changes.first() {
        return Err(not_the_same(to, Some(change)));
    }
    Ok(Converted {
        code: text,
        notices: w.notices,
    })
}

#[derive(Default)]
struct Writer {
    out: Vec<String>,
    notices: Vec<Notice>,
    /// Lines of the original start this far into its text.
    offset: usize,
    /// Each participant's code, by its code in the original.
    ids: HashMap<String, String>,
}

impl Writer {
    fn notice(&mut self, line: Option<usize>, message: impl Into<String>) {
        self.notices.push(Notice {
            line: line.map(|l| l + self.offset),
            message: message.into(),
        });
    }

    fn refuse(&self, line: usize, reason: &str) -> Refused {
        refused(Some(line + self.offset), reason)
    }

    /// Gives every participant a code both languages take.
    fn name_all(&mut self, d: &Diagram) {
        let mut taken = HashSet::new();
        for p in &d.participants {
            let id = match is_plain(&p.code) && !taken.contains(&p.code) {
                true => p.code.clone(),
                false => plain_id(&p.code, &taken),
            };
            if id != p.code {
                self.notice(
                    Some(p.line),
                    format!(
                        "{} is called {id}; its head still shows {}",
                        p.code, p.label
                    ),
                );
            }
            taken.insert(id.clone());
            self.ids.insert(p.code.clone(), id);
        }
    }

    fn id(&self, d: &Diagram, participant: usize) -> String {
        self.ids[&d.participants[participant].code].clone()
    }

    fn mermaid(&mut self, d: &Diagram) -> Result<(), Refused> {
        // `hide footbox` is Mermaid's setting against drawing the
        // participants again below.
        if !d.footbox {
            self.out
                .push("%%{init: {\"sequence\": {\"mirrorActors\": false}}}%%".into());
        }
        self.out.push("sequenceDiagram".into());
        if let Some(title) = &d.frame.title {
            self.out
                .push(format!("    title {}", to_mermaid_text(&title.value)));
        }
        self.frame_notices(d);
        if d.items
            .iter()
            .any(|i| matches!(i, Item::Message(m) if m.number.is_some()))
        {
            self.out.push("    autonumber".into());
        }
        self.participants(d, Language::Mermaid);
        let mut open: Vec<GroupKind> = Vec::new();
        let mut last_to: Option<usize> = None;
        // A `destroy` stored after its message is written before it:
        // Mermaid requires that order, and the reader puts the cross back
        // after the message.
        let mut consumed = vec![false; d.items.len()];
        for (i, item) in d.items.iter().enumerate() {
            if consumed[i] {
                continue;
            }
            match item {
                Item::Message(m) => {
                    let (Some(from), Some(to)) = (self.end(d, m.from), self.end(d, m.to)) else {
                        return Err(self.refuse(
                            m.line,
                            "Mermaid has no message from or to the edge of the diagram",
                        ));
                    };
                    if m.together {
                        self.notice(Some(m.line), TOGETHER);
                    }
                    if m.slant != 0.0 {
                        self.notice(Some(m.line), "Mermaid has no slanted arrows; it is level");
                    }
                    if let Some((at, participant)) = destroy_after(d, i, m) {
                        let id = self.id(d, participant);
                        self.out.push(format!("    destroy {id}"));
                        consumed[at] = true;
                    }
                    if m.creates
                        && let End::Participant(created) = m.to
                    {
                        self.out
                            .push(format!("    {}", self.created_line(d, created)));
                    }
                    self.arrow_look(&m.arrow, m.line);
                    let text = m.text.as_ref().map(|t| t.value.as_str()).unwrap_or("");
                    let arrow = mermaid_arrow(&m.arrow);
                    self.out
                        .push(format!("    {from}{arrow}{to}: {}", to_mermaid_text(text)));
                    if let End::Participant(to) = m.to {
                        last_to = Some(to);
                    }
                }
                Item::NewPage { line, .. } => self.notice(
                    Some(*line),
                    "Mermaid has no pages; what follows `newpage` comes right after",
                ),
                Item::Duration { line, .. } => self.notice(
                    Some(*line),
                    "Mermaid has no durations between messages; it is left out",
                ),
                Item::Note(note) => {
                    if note.together {
                        self.notice(Some(note.line), TOGETHER);
                    }
                    if note.shape != NoteShape::Note || note.color.is_some() {
                        self.notice(
                            Some(note.line),
                            "Mermaid has one kind of note, without color",
                        );
                    }
                    let place = match note.place {
                        NotePlace::LeftOf(p) => format!("left of {}", self.id(d, p)),
                        NotePlace::RightOf(p) => format!("right of {}", self.id(d, p)),
                        NotePlace::Over(a, b) if a == b => format!("over {}", self.id(d, a)),
                        NotePlace::Over(a, b) => {
                            format!("over {},{}", self.id(d, a), self.id(d, b))
                        }
                        NotePlace::Across => {
                            // Across nobody: the diagram draws nothing.
                            let Some(last) = d.participants.len().checked_sub(1) else {
                                return Err(self.refuse(
                                    note.line,
                                    "a note across the diagram needs a participant",
                                ));
                            };
                            format!("over {},{}", self.id(d, 0), self.id(d, last))
                        }
                        NotePlace::Message(_) => {
                            self.notice(
                                Some(note.line),
                                "Mermaid puts no note beside an arrow; it goes right of the receiver",
                            );
                            match last_to {
                                Some(to) => format!("right of {}", self.id(d, to)),
                                None => continue,
                            }
                        }
                    };
                    self.out.push(format!(
                        "    Note {place}: {}",
                        to_mermaid_text(&note.text.value)
                    ));
                }
                Item::GroupStart {
                    kind,
                    label,
                    tab,
                    fill,
                    line,
                    ..
                } => {
                    if *kind == GroupKind::Group {
                        return Err(self.refuse(*line, "Mermaid has no group block"));
                    }
                    if tab.is_some() || fill.is_some() {
                        self.notice(Some(*line), GROUP_COLORS);
                    }
                    open.push(*kind);
                    let label = label
                        .as_ref()
                        .map(|t| format!(" {}", to_mermaid_text(&t.value)))
                        .unwrap_or_default();
                    self.out.push(format!("    {}{label}", kind.keyword()));
                }
                Item::GroupElse { label, fill, line } => {
                    if fill.is_some() {
                        self.notice(Some(*line), GROUP_COLORS);
                    }
                    let word = match open.last() {
                        Some(GroupKind::Par) => "and",
                        Some(GroupKind::Critical) => "option",
                        _ => "else",
                    };
                    let label = label
                        .as_ref()
                        .map(|t| format!(" {}", to_mermaid_text(&t.value)))
                        .unwrap_or_default();
                    self.out.push(format!("    {word}{label}"));
                }
                Item::GroupEnd { .. } => {
                    open.pop();
                    self.out.push("    end".into());
                }
                Item::RectStart { color, .. } => {
                    self.out.push(format!("    rect {color}"));
                }
                Item::RectEnd { .. } => self.out.push("    end".into()),
                Item::Activate {
                    participant,
                    color,
                    line,
                } => {
                    if color.is_some() {
                        self.notice(Some(*line), "Colors and line styles are not carried over");
                    }
                    let id = self.id(d, *participant);
                    self.out.push(format!("    activate {id}"));
                }
                Item::Deactivate { participant, .. } => {
                    let id = self.id(d, *participant);
                    self.out.push(format!("    deactivate {id}"));
                }
                Item::Divider { label } => {
                    self.notice(Some(label.line), "Mermaid has no divider; it is left out");
                }
                Item::Delay { line, .. } => {
                    self.notice(Some(*line), "Mermaid has no delay; it is left out");
                }
                Item::Space { line, .. } => {
                    self.notice(Some(*line), "Mermaid has no space step; it is left out");
                }
                Item::Ref { text, .. } => {
                    return Err(self.refuse(text.line, "Mermaid has no ref"));
                }
                // One after no message of its own (after a group, say):
                // Mermaid destroys a participant only at a message.
                Item::Destroy { line, .. } => {
                    return Err(self.refuse(
                        *line,
                        "Mermaid destroys a participant only at a message of its own, and this destroy follows none",
                    ));
                }
            }
        }
        Ok(())
    }

    fn plantuml(&mut self, d: &Diagram) -> Result<(), Refused> {
        self.out.push("@startuml".into());
        if let Some(title) = &d.frame.title {
            self.out
                .push(format!("title {}", to_plantuml_text(&title.value)));
        }
        if !d.footbox {
            self.out.push("hide footbox".into());
        }
        if d.items
            .iter()
            .any(|i| matches!(i, Item::Message(m) if m.number.is_some()))
        {
            self.out.push("autonumber".into());
        }
        self.participants(d, Language::PlantUml);
        for item in &d.items {
            match item {
                Item::Message(m) => {
                    let (Some(from), Some(to)) = (self.end(d, m.from), self.end(d, m.to)) else {
                        return Err(self.refuse(m.line, "a message from or to the edge"));
                    };
                    if m.arrow.head == Head::None && m.arrow.end_mark == Mark::None {
                        self.notice(
                            Some(m.line),
                            "PlantUML draws every message with a head; this one gets one",
                        );
                    }
                    if m.creates {
                        self.out.push(format!("create {to}"));
                    }
                    let text = match &m.text {
                        Some(t) if !t.value.is_empty() => {
                            format!(" : {}", to_plantuml_text(&t.value))
                        }
                        _ => String::new(),
                    };
                    let arrow = plantuml_arrow(&m.arrow);
                    let anchor = m
                        .anchor
                        .as_ref()
                        .map(|a| format!("{{{a}}} "))
                        .unwrap_or_default();
                    self.out.push(format!("{anchor}{from} {arrow} {to}{text}"));
                }
                Item::Duration {
                    from, to, label, ..
                } => self.out.push(match label {
                    Some(label) => format!(
                        "{{{from}}} <-> {{{to}}} : {}",
                        to_plantuml_text(&label.value)
                    ),
                    None => format!("{{{from}}} <-> {{{to}}}"),
                }),
                Item::NewPage { title, .. } => self.out.push(match title {
                    Some(title) => format!("newpage {}", to_plantuml_text(&title.value)),
                    None => "newpage".into(),
                }),
                Item::Note(note) => {
                    let place = match note.place {
                        NotePlace::LeftOf(p) => format!("left of {}", self.id(d, p)),
                        NotePlace::RightOf(p) => format!("right of {}", self.id(d, p)),
                        NotePlace::Over(a, b) if a == b => format!("over {}", self.id(d, a)),
                        NotePlace::Over(a, b) => {
                            format!("over {}, {}", self.id(d, a), self.id(d, b))
                        }
                        NotePlace::Across => "across".into(),
                        NotePlace::Message(side) => match side {
                            omaspec_uml::sequence::Side::Left => "left".into(),
                            omaspec_uml::sequence::Side::Right => "right".into(),
                        },
                    };
                    self.out.push(format!(
                        "note {place} : {}",
                        to_plantuml_text(&note.text.value)
                    ));
                }
                Item::GroupStart { kind, label, .. } => {
                    let label = label
                        .as_ref()
                        .map(|t| format!(" {}", to_plantuml_text(&t.value)))
                        .unwrap_or_default();
                    self.out.push(format!("{}{label}", kind.keyword()));
                }
                Item::GroupElse { label, .. } => {
                    let label = label
                        .as_ref()
                        .map(|t| format!(" {}", to_plantuml_text(&t.value)))
                        .unwrap_or_default();
                    self.out.push(format!("else{label}"));
                }
                Item::GroupEnd { .. } => self.out.push("end".into()),
                Item::Activate { participant, .. } => {
                    let id = self.id(d, *participant);
                    self.out.push(format!("activate {id}"));
                }
                Item::Deactivate { participant, .. } => {
                    let id = self.id(d, *participant);
                    self.out.push(format!("deactivate {id}"));
                }
                Item::Destroy { participant, .. } => {
                    let id = self.id(d, *participant);
                    self.out.push(format!("destroy {id}"));
                }
                Item::RectStart { line, .. } => {
                    self.notice(
                        Some(*line),
                        "PlantUML has no rect highlight; it is left out",
                    );
                }
                Item::RectEnd { .. } => {}
                // Mermaid writes none of these.
                Item::Divider { .. }
                | Item::Delay { .. }
                | Item::Space { .. }
                | Item::Ref { .. } => {}
            }
        }
        self.out.push("@enduml".into());
        Ok(())
    }

    /// The participants in their columns, in their boxes.
    fn participants(&mut self, d: &Diagram, to: Language) {
        let pad = match to {
            Language::Mermaid => "    ",
            Language::PlantUml => "",
        };
        // A created participant is declared on the `create` line. Listing
        // it here too makes Mermaid reject the repeated id.
        let created = created_participants(d);
        // Mermaid's boxes do not nest: each participant goes into the
        // outermost box around it.
        if to == Language::Mermaid
            && let Some(inner) = d.boxes.iter().find(|b| b.depth > 0)
        {
            self.notice(
                Some(inner.line),
                "Mermaid has no box inside a box; its participants go into the box around it",
            );
        }
        let mut in_box: Option<usize> = None;
        for (i, p) in d.participants.iter().enumerate() {
            if to == Language::Mermaid && created.contains(&i) {
                continue;
            }
            let home = d.boxes.iter().position(|b| b.participants.contains(&i));
            if home != in_box {
                if in_box.is_some() {
                    self.out.push(match to {
                        Language::Mermaid => "    end".into(),
                        Language::PlantUml => "end box".into(),
                    });
                }
                if let Some(b) = home {
                    let b = &d.boxes[b];
                    let title = b.title.as_ref().map(|t| t.value.as_str()).unwrap_or("");
                    let head = match to {
                        Language::Mermaid => {
                            // `#LightBlue` is a name; Mermaid writes names bare.
                            let color = b
                                .color
                                .as_deref()
                                .map(|c| match c.strip_prefix('#') {
                                    Some(name) if !name.chars().all(|c| c.is_ascii_hexdigit()) => {
                                        format!(" {name}")
                                    }
                                    _ => format!(" {c}"),
                                })
                                .unwrap_or_default();
                            format!("    box{color} {title}").trim_end().to_owned()
                        }
                        Language::PlantUml => {
                            let color = match b.color.as_deref() {
                                Some(c) if c.starts_with('#') => format!(" {c}"),
                                Some(c) if c.chars().all(char::is_alphanumeric) => format!(" #{c}"),
                                Some(_) => {
                                    self.notice(Some(b.line), "This box color is not carried over");
                                    String::new()
                                }
                                None => String::new(),
                            };
                            format!("box \"{title}\"{color}")
                        }
                    };
                    self.out.push(head);
                }
                in_box = home;
            }
            let id = self.ids[&p.code].clone();
            let (keyword, anno) = match (to, p.shape) {
                (_, Shape::Actor) => ("actor", String::new()),
                (Language::Mermaid, Shape::Participant) => ("participant", String::new()),
                (Language::Mermaid, shape) => ("participant", type_annotation(shape)),
                (Language::PlantUml, Shape::Participant) => ("participant", String::new()),
                (Language::PlantUml, Shape::Boundary) => ("boundary", String::new()),
                (Language::PlantUml, Shape::Control) => ("control", String::new()),
                (Language::PlantUml, Shape::Entity) => ("entity", String::new()),
                (Language::PlantUml, Shape::Database) => ("database", String::new()),
                (Language::PlantUml, Shape::Collections) => ("collections", String::new()),
                (Language::PlantUml, Shape::Queue) => ("queue", String::new()),
            };
            if p.stereotype.is_some() || p.color.is_some() {
                self.notice(
                    Some(p.line),
                    "Stereotypes and colors of participants are not carried over",
                );
            }
            let line = match (to, p.label == id) {
                (Language::Mermaid, true) => format!("{pad}{keyword} {id}{anno}"),
                (Language::Mermaid, false) => {
                    format!("{pad}{keyword} {id}{anno} as {}", to_mermaid_text(&p.label))
                }
                (Language::PlantUml, true) => format!("{pad}{keyword} {id}"),
                (Language::PlantUml, false) => {
                    format!("{pad}{keyword} \"{}\" as {id}", to_plantuml_text(&p.label))
                }
            };
            self.out.push(line);
        }
        if in_box.is_some() {
            self.out.push(match to {
                Language::Mermaid => "    end".into(),
                Language::PlantUml => "end box".into(),
            });
        }
    }

    /// `create participant Late` or `create actor D as Donald`, with a
    /// `@{ "type" }` when the shape is neither.
    fn created_line(&self, d: &Diagram, participant: usize) -> String {
        let p = &d.participants[participant];
        let id = self.id(d, participant);
        let (keyword, anno) = match p.shape {
            Shape::Actor => ("actor", String::new()),
            Shape::Participant => ("participant", String::new()),
            shape => ("participant", type_annotation(shape)),
        };
        if p.label == id {
            format!("create {keyword} {id}{anno}")
        } else {
            format!(
                "create {keyword} {id}{anno} as {}",
                to_mermaid_text(&p.label)
            )
        }
    }

    /// A message's end as a code; `None` for the edge of the diagram.
    fn end(&self, d: &Diagram, end: End) -> Option<String> {
        match end {
            End::Participant(p) => Some(self.id(d, p)),
            End::Edge(_) | End::Short(_) => None,
        }
    }

    fn frame_notices(&mut self, d: &Diagram) {
        let frame = &d.frame;
        for (what, text) in [
            ("header", &frame.header),
            ("footer", &frame.footer),
            ("caption", &frame.caption),
            ("mainframe", &frame.mainframe),
        ] {
            if let Some(text) = text {
                self.notice(
                    Some(text.line),
                    format!("Mermaid has no {what}; it is left out"),
                );
            }
        }
        if let Some(legend) = &frame.legend {
            self.notice(
                Some(legend.text.line),
                "Mermaid has no legend; it is left out",
            );
        }
        if let Some(param) = frame.skinparams.iter().find(|p| p.line > 0) {
            self.notice(
                Some(param.line),
                "Skin parameters and styles are not carried over",
            );
        }
    }

    fn arrow_look(&mut self, arrow: &Arrow, line: usize) {
        let half = matches!(
            arrow.head,
            Head::UpperFilled | Head::LowerFilled | Head::UpperOpen | Head::LowerOpen
        );
        if half || arrow.color.is_some() || arrow.bold || arrow.end_mark == Mark::Circle {
            self.notice(
                Some(line),
                "Mermaid draws this arrow plainer: its head, color or weight is left out",
            );
        }
        if arrow.start_mark != Mark::None {
            self.notice(Some(line), "Mermaid has no mark at the sending end");
        }
    }
}

/// The `destroy` after this message, skipping the activations the message
/// itself opened and the notes after it, as PlantUML puts the cross on the
/// message across them. Only when that participant sends or receives it.
fn destroy_after(
    d: &Diagram,
    from: usize,
    m: &omaspec_uml::sequence::Message,
) -> Option<(usize, usize)> {
    let mut j = from + 1;
    while let Some(item) = d.items.get(j) {
        match item {
            Item::Activate { .. } | Item::Deactivate { .. } | Item::Note(_) => j += 1,
            Item::Destroy { participant, .. }
                if m.from == End::Participant(*participant)
                    || m.to == End::Participant(*participant) =>
            {
                return Some((j, *participant));
            }
            _ => return None,
        }
    }
    None
}

/// Participants whose head is drawn by a `create`, not at the top.
fn created_participants(d: &Diagram) -> HashSet<usize> {
    d.items
        .iter()
        .filter_map(|item| match item {
            Item::Message(m) if m.creates => match m.to {
                End::Participant(p) => Some(p),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// `@{ "type": "database" }` for the shapes Mermaid has no keyword for.
fn type_annotation(shape: Shape) -> String {
    let name = match shape {
        Shape::Participant | Shape::Actor => return String::new(),
        Shape::Boundary => "boundary",
        Shape::Control => "control",
        Shape::Entity => "entity",
        Shape::Database => "database",
        Shape::Collections => "collections",
        Shape::Queue => "queue",
    };
    format!(r#"@{{ "type": "{name}" }}"#)
}

/// The Mermaid arrow closest to `arrow`.
fn mermaid_arrow(arrow: &Arrow) -> &'static str {
    let dashed = arrow.dashed;
    if arrow.tail != Head::None {
        return if dashed { "<<-->>" } else { "<<->>" };
    }
    match (arrow.end_mark, arrow.head) {
        (Mark::Cross, _) => {
            if dashed {
                "--x"
            } else {
                "-x"
            }
        }
        (_, Head::Open) => {
            if dashed {
                "--)"
            } else {
                "-)"
            }
        }
        (_, Head::None) => {
            if dashed {
                "-->"
            } else {
                "->"
            }
        }
        _ => {
            if dashed {
                "-->>"
            } else {
                "->>"
            }
        }
    }
}

/// The PlantUML arrow closest to `arrow`.
fn plantuml_arrow(arrow: &Arrow) -> String {
    let line = if arrow.dashed { "--" } else { "-" };
    let head = match (arrow.end_mark, arrow.head) {
        (Mark::Cross, _) => ">x",
        (_, Head::Open) => ">>",
        _ => ">",
    };
    let tail = if arrow.tail != Head::None { "<" } else { "" };
    format!("{tail}{line}{head}")
}

#[cfg(test)]
mod tests {
    use super::super::convert as both;
    use super::*;

    const SHOP: &str = "@startuml\ntitle Checkout\nactor Alice\nbox \"Back end\" #LightBlue\nparticipant Shop\ndatabase Store\nend box\nAlice -> Shop : order(cart)\nactivate Shop\nShop -> Store : reserve\nStore --> Shop : ok\nalt paid\nShop -> Alice : receipt\nelse declined\nShop ->x Alice : retry\nend\nloop every day\nShop ->> Store : sync\nend\nnote over Shop, Store : nightly\ndeactivate Shop\n== later ==\n@enduml\n";

    /// Found by fuzzing (2026-11-18): a note across a diagram without a
    /// participant has nobody to stand over; it panicked.
    #[test]
    fn a_note_across_nobody_is_refused() {
        let code = "@startuml\ntitle Events\nrnote across : a\n@enduml\n";
        let err = both(code, Language::PlantUml, Language::Mermaid).unwrap_err();
        assert_eq!(err.line, Some(2), "{err:?}");
        assert!(err.reason.contains("needs a participant"), "{err:?}");
    }

    /// Found in review (2026-11-22): the words of `participant P [ … ]`
    /// stand on lines of their own, which Mermaid writes as `<br>`.
    #[test]
    fn a_participant_s_words_over_several_lines_convert() {
        let code =
            "@startuml\nparticipant P [\nfirst\nsecond\n]\nparticipant Q\nP -> Q : hi\n@enduml\n";
        let mermaid = both(code, Language::PlantUml, Language::Mermaid).unwrap();
        assert!(mermaid.code.contains("first<br>second"), "{}", mermaid.code);
    }

    #[test]
    fn a_plantuml_scenario_reads_the_same_in_mermaid_and_back() {
        let mermaid = both(SHOP, Language::PlantUml, Language::Mermaid).unwrap();
        for line in [
            "    title Checkout",
            "    actor Alice",
            "    box LightBlue Back end",
            "    participant Store@{ \"type\": \"database\" }",
            "    Alice->>Shop: order(cart)",
            "    Store-->>Shop: ok",
            "    Shop-xAlice: retry",
            "    Shop-)Store: sync",
            "    Note over Shop,Store: nightly",
        ] {
            assert!(
                mermaid.code.contains(&format!("{line}\n")),
                "{line:?} not in {}",
                mermaid.code
            );
        }
        let said: Vec<&str> = mermaid.notices.iter().map(|n| n.message.as_str()).collect();
        assert!(said.iter().any(|m| m.contains("divider")), "{said:?}");
        let back = both(&mermaid.code, Language::Mermaid, Language::PlantUml).unwrap();
        assert!(
            back.code.contains("Shop ->x Alice : retry\n"),
            "{}",
            back.code
        );
        assert!(back.code.contains("database Store\n"), "{}", back.code);
    }

    #[test]
    fn create_destroy_and_a_highlight_survive_the_round_trip() {
        let code = "\
sequenceDiagram
    create participant Late
    Alice->>Late: born
    destroy Late
    Late->>Alice: gone
    rect rgb(200, 200, 200)
    Alice->>Late: inside
    end
";
        let plant = both(code, Language::Mermaid, Language::PlantUml).unwrap();
        assert!(plant.code.contains("create Late\n"), "{}", plant.code);
        assert!(plant.code.contains("destroy Late\n"), "{}", plant.code);
        assert!(
            plant.notices.iter().any(|n| n.message.contains("rect")),
            "{:?}",
            plant.notices
        );
        let back = both(&plant.code, Language::PlantUml, Language::Mermaid).unwrap();
        assert!(
            back.code.contains("create participant Late\n"),
            "{}",
            back.code
        );
        assert!(back.code.contains("destroy Late\n"), "{}", back.code);
        assert!(!back.code.contains("rect "), "{}", back.code);
    }

    /// A `destroy` after notes on its message is that message's, as
    /// PlantUML draws it, and comes back; one after a group follows no
    /// message of its own, which Mermaid cannot write: refused, saying so.
    #[test]
    fn a_destroy_after_a_note_is_its_message_s() {
        let code = "@startuml\nA -> B : q\nnote over B : x\ndestroy B\n@enduml\n";
        let mermaid = both(code, Language::PlantUml, Language::Mermaid).unwrap();
        assert!(
            mermaid
                .code
                .contains("destroy B\n    A->>B: q\n    Note over B: x\n"),
            "{}",
            mermaid.code
        );
        let code = "@startuml\nA -> B : q\nalt ok\n  A -> B : r\nend\ndestroy B\n@enduml\n";
        let err = both(code, Language::PlantUml, Language::Mermaid).unwrap_err();
        assert!(err.reason.contains("only at a message"), "{err:?}");
        // The line of the `destroy`, counted from 0.
        assert_eq!(err.line, Some(5), "{err:?}");
    }

    #[test]
    fn mermaid_blocks_come_back_as_plantuml_blocks() {
        let code = "sequenceDiagram\n    participant A as Alice\n    A->>+B: hello\n    par together\n        B-)C: one\n    and\n        B-)D: two\n    end\n    critical lock\n        B->>A: done\n    option timeout\n        B--xA: gone\n    end\n";
        let plantuml = both(code, Language::Mermaid, Language::PlantUml).unwrap();
        for line in [
            "participant \"Alice\" as A",
            "A -> B : hello",
            "activate B",
            "par together",
            "B ->> C : one",
            "critical lock",
            "B -->x A : gone",
        ] {
            assert!(
                plantuml.code.contains(&format!("{line}\n")),
                "{line:?} not in {}",
                plantuml.code
            );
        }
    }

    #[test]
    fn participants_below_or_not_carry_over_both_ways() {
        let hidden = "@startuml\nhide footbox\nAlice -> Bob : hi\n@enduml\n";
        let mermaid = both(hidden, Language::PlantUml, Language::Mermaid).unwrap();
        assert!(
            mermaid.code.starts_with(
                "%%{init: {\"sequence\": {\"mirrorActors\": false}}}%%\nsequenceDiagram\n"
            ),
            "{}",
            mermaid.code
        );
        let back = both(&mermaid.code, Language::Mermaid, Language::PlantUml).unwrap();
        assert!(back.code.contains("\nhide footbox\n"), "{}", back.code);
        let front = "---\nconfig:\n  sequence:\n    mirrorActors: false\n---\nsequenceDiagram\n    Alice->>Bob: hi\n";
        let plantuml = both(front, Language::Mermaid, Language::PlantUml).unwrap();
        assert!(
            plantuml.code.contains("\nhide footbox\n"),
            "{}",
            plantuml.code
        );
        // Below, as both draw them unless told: nothing to say.
        let shown = "@startuml\nAlice -> Bob : hi\n@enduml\n";
        let mermaid = both(shown, Language::PlantUml, Language::Mermaid).unwrap();
        assert!(!mermaid.code.contains("mirrorActors"), "{}", mermaid.code);
        let back = both(&mermaid.code, Language::Mermaid, Language::PlantUml).unwrap();
        assert!(!back.code.contains("footbox"), "{}", back.code);
    }

    #[test]
    fn what_mermaid_cannot_say_refuses() {
        for (body, says) in [
            ("A -> B : x\nref over A, B : login\n", "no ref"),
            ("group setup\nA -> B : x\nend\n", "no group"),
            ("[-> A : start\n", "edge"),
        ] {
            let code = format!("@startuml\n{body}@enduml\n");
            let err = both(&code, Language::PlantUml, Language::Mermaid).unwrap_err();
            assert!(err.reason.contains(says), "{body}: {err:?}");
        }
    }
}
