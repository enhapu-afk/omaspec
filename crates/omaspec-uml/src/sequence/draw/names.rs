//! The elements of a sequence diagram's picture by name, from the source
//! (docs/adr/0037): a participant by its kind and its code
//! (`participant-Alice`, `actor-User`), a message by its ends and its
//! words (`message-Alice-Bob-hello`, an edge `left` or `right`), a note
//! by what it stands by, its side and its number there
//! (`note-participant-Alice-right-1`, one beside a message after the
//! message's), a fragment by its keyword and its words (`alt-ok`), a
//! divider, a delay and a reference by their words, a box of
//! participants by its title. A name given twice takes `-2`.

use std::collections::HashMap;

use super::super::{Diagram, End, GroupKind, Item, NotePlace, Shape, Side};
use crate::svg::{Given, element, note_part, push_written};

#[derive(Debug, Clone, Default)]
pub(super) struct Names {
    participants: Vec<String>,
    items: Vec<String>,
    boxes: Vec<String>,
}

impl Names {
    pub(super) fn of(d: &Diagram) -> Names {
        let mut given = Given::default();
        let participants: Vec<String> = d
            .participants
            .iter()
            .map(|p| given.unique(element(kind_word(p.shape), &p.code)))
            .collect();
        let end = |end: &End| match end {
            End::Participant(p) => written(&d.participants[*p].code),
            End::Edge(side) | End::Short(side) => side_word(*side).to_owned(),
        };
        // How many notes each element has on each side so far.
        let mut notes: HashMap<(String, &str), usize> = HashMap::new();
        let mut items = Vec::with_capacity(d.items.len());
        for (k, item) in d.items.iter().enumerate() {
            let name = match item {
                Item::Message(m) => {
                    let mut name = format!("message-{}-{}", end(&m.from), end(&m.to));
                    let words = first_line(m.text.as_ref().map(|t| t.value.as_str()));
                    if !words.is_empty() {
                        name.push('-');
                        push_written(&mut name, words);
                    }
                    name
                }
                Item::Note(note) => {
                    let (of, side) = match note.place {
                        NotePlace::LeftOf(p) => (participants[p].clone(), "left"),
                        NotePlace::RightOf(p) => (participants[p].clone(), "right"),
                        NotePlace::Over(p, _) => (participants[p].clone(), "over"),
                        NotePlace::Across => (String::new(), "across"),
                        NotePlace::Message(side) => {
                            let message = k.checked_sub(1).and_then(|at| items.get(at)).cloned();
                            (message.unwrap_or_default(), side_word(side))
                        }
                    };
                    let seen = notes.entry((of.clone(), side)).or_default();
                    let nth = *seen;
                    *seen += 1;
                    match of.is_empty() {
                        true => format!("note-{side}-{}", nth + 1),
                        false => note_part(&of, side, nth),
                    }
                }
                Item::GroupStart { kind, label, .. } => {
                    let words = first_line(label.as_ref().map(|t| t.value.as_str()));
                    element(group_word(*kind), words)
                }
                Item::Divider { label } => element("divider", first_line(Some(&label.value))),
                Item::Delay { label, .. } => element(
                    "delay",
                    first_line(label.as_ref().map(|t| t.value.as_str())),
                ),
                Item::Ref { text, .. } => element("ref", first_line(Some(&text.value))),
                _ => String::new(),
            };
            items.push(if name.is_empty() {
                name
            } else {
                given.unique(name)
            });
        }
        let boxes = d
            .boxes
            .iter()
            .map(|b| {
                let words = first_line(b.title.as_ref().map(|t| t.value.as_str()));
                given.unique(element("box", words))
            })
            .collect();
        Names {
            participants,
            items,
            boxes,
        }
    }

    /// The element of participant `p`: its heads, its lifeline, its bars.
    pub(super) fn participant(&self, p: usize) -> &str {
        self.participants.get(p).map_or("", String::as_str)
    }

    /// The element of item `k`: a message, a note, a fragment, a divider,
    /// a delay or a reference; empty for one that draws nothing of its own.
    pub(super) fn item(&self, k: usize) -> &str {
        self.items.get(k).map_or("", String::as_str)
    }

    /// The element of the box of participants `b`.
    pub(super) fn group_box(&self, b: usize) -> &str {
        self.boxes.get(b).map_or("", String::as_str)
    }
}

/// `name` as an id holds it ([`element`]).
fn written(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    push_written(&mut out, name);
    out
}

/// The first line of `text`, trimmed; empty without one.
fn first_line(text: Option<&str>) -> &str {
    let text = text.unwrap_or_default();
    let first = text.split("\\n").next().unwrap_or_default();
    first.split('\n').next().unwrap_or_default().trim()
}

fn side_word(side: Side) -> &'static str {
    match side {
        Side::Left => "left",
        Side::Right => "right",
    }
}

fn kind_word(shape: Shape) -> &'static str {
    match shape {
        Shape::Participant => "participant",
        Shape::Actor => "actor",
        Shape::Boundary => "boundary",
        Shape::Control => "control",
        Shape::Entity => "entity",
        Shape::Database => "database",
        Shape::Collections => "collections",
        Shape::Queue => "queue",
    }
}

fn group_word(kind: GroupKind) -> &'static str {
    kind.keyword()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_are_named_from_the_source() {
        let crate::Diagram::Sequence(d) = crate::parse(
            "@startuml\nactor User\nparticipant \"Web app\" as Web\nUser -> Web : log in\nnote right : first\nalt ok\n  Web --> User : welcome\nelse\n  Web --> User : again\nend\nUser -> Web : log in\n== Done ==\nnote over User : bye\n@enduml\n",
        )
        .unwrap() else {
            panic!("a sequence");
        };
        let n = Names::of(&d);
        assert_eq!(n.participant(0), "actor-User");
        assert_eq!(n.participant(1), "participant-Web");
        let items: Vec<&str> = (0..d.items.len())
            .map(|k| n.item(k))
            .filter(|n| !n.is_empty())
            .collect();
        assert_eq!(
            items,
            [
                "message-User-Web-log_20_in",
                "note-message-User-Web-log_20_in-right-1",
                "alt-ok",
                "message-Web-User-welcome",
                "message-Web-User-again",
                "message-User-Web-log_20_in-2",
                "divider-Done",
                "note-actor-User-over-1"
            ]
        );
    }
}
