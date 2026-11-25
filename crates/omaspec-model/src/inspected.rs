//! What `omaspec inspect` tells of a diagram's behavior beside its outline
//! (REQ-107): a state machine's states and transitions, a sequence
//! diagram's participants, messages and notes, each on its line (one-based,
//! as the outline's), so an agent reads what the diagram does without
//! drawing it or reading its source.

use omaspec_core::{Kind as DiagramKind, Language, classify};
use omaspec_uml::sequence::{self, End, Item, NotePlace, Shape, Side};
use serde_json::{Map, Value, json};

use crate::behavior::{path, plain};
use crate::{Kind, Machine, across};

/// The behavior of the state machine or sequence diagram `code`, under the
/// key it goes by (`machine` or `scenario`); `None` for another kind, or
/// one Omaspec does not fully read.
pub fn inspected(code: &str, lang: Language) -> Option<(&'static str, Value)> {
    match classify(code, lang).kind {
        DiagramKind::State => Some(("machine", machine(&Machine::read(code, lang)?))),
        DiagramKind::Sequence => {
            let (diagram, offset) = across::read_sequence(code, lang)?;
            Some(("scenario", scenario(&diagram, offset)))
        }
        _ => None,
    }
}

/// The states, a start and an end aside, and the transitions, in the
/// order written. A state goes by its path (`On/Playing`, `[*]` for a
/// start or an end), as the transitions name it.
fn machine(m: &Machine) -> Value {
    let states: Vec<Value> = (0..m.states.len())
        .filter_map(|s| {
            let state = &m.states[s];
            let kind = match state.kind {
                Kind::State => "state",
                Kind::Choice => "choice",
                Kind::Fork => "fork",
                Kind::Join => "join",
                Kind::Entry => "entry point",
                Kind::Exit => "exit point",
                Kind::History => "history",
                Kind::DeepHistory => "deep history",
                Kind::Start | Kind::End => return None,
            };
            let mut out = Map::new();
            out.insert("name".into(), json!(path(m, s)));
            out.insert("kind".into(), json!(kind));
            if let Some(label) = &state.label {
                out.insert("label".into(), json!(label));
            }
            if state.decide {
                out.insert("decide".into(), json!(true));
            }
            out.insert("line".into(), json!(state.line + 1));
            Some(Value::Object(out))
        })
        .collect();
    let transitions: Vec<Value> = m
        .transitions
        .iter()
        .map(|t| {
            let mut out = Map::new();
            out.insert("from".into(), json!(path(m, t.from)));
            out.insert("to".into(), json!(path(m, t.to)));
            if !t.trigger.events.is_empty() {
                out.insert("events".into(), json!(t.trigger.events));
            }
            if let Some(guard) = &t.trigger.guard {
                out.insert("guard".into(), json!(guard));
            }
            if let Some(action) = &t.trigger.action {
                out.insert("action".into(), json!(action));
            }
            out.insert("line".into(), json!(t.line + 1));
            Value::Object(out)
        })
        .collect();
    json!({ "states": states, "transitions": transitions })
}

/// The participants, then the messages and the notes in their order, each
/// message with the groups around it (`alt ok`, `alt else fail`, `loop
/// each`); `offset` the lines before a Mermaid body.
fn scenario(d: &sequence::Diagram, offset: usize) -> Value {
    let line = |line: usize| line + offset + 1;
    let name = |i: usize| d.participants[i].code.clone();
    let end = |e: End| match e {
        End::Participant(i) => name(i),
        End::Edge(Side::Left) | End::Short(Side::Left) => "[".into(),
        End::Edge(Side::Right) | End::Short(Side::Right) => "]".into(),
    };
    let participants: Vec<Value> = d
        .participants
        .iter()
        .map(|p| {
            let mut out = Map::new();
            out.insert("name".into(), json!(p.code));
            if plain(&p.label) != p.code {
                out.insert("label".into(), json!(plain(&p.label)));
            }
            out.insert("kind".into(), json!(shape(p.shape)));
            out.insert("line".into(), json!(line(p.line)));
            Value::Object(out)
        })
        .collect();
    let mut messages = Vec::new();
    let mut notes = Vec::new();
    // The groups the items stand in, outermost first.
    let mut groups: Vec<(String, String)> = Vec::new();
    let named = |kind: &str, label: Option<&sequence::Text>| match label {
        Some(label) if !plain(&label.value).is_empty() => {
            format!("{kind} {}", plain(&label.value))
        }
        _ => kind.to_owned(),
    };
    for item in &d.items {
        match item {
            Item::Message(m) => {
                let mut out = Map::new();
                out.insert("from".into(), json!(end(m.from)));
                out.insert("to".into(), json!(end(m.to)));
                if let Some(text) = m.text.as_ref().map(|t| plain(&t.value))
                    && !text.is_empty()
                {
                    out.insert("text".into(), json!(text));
                }
                if m.arrow.dashed {
                    out.insert("reply".into(), json!(true));
                }
                if !groups.is_empty() {
                    let within: Vec<&str> = groups.iter().map(|(_, at)| at.as_str()).collect();
                    out.insert("in".into(), json!(within));
                }
                out.insert("line".into(), json!(line(m.line)));
                messages.push(Value::Object(out));
            }
            Item::Note(note) => {
                let (place, over): (&str, Vec<String>) = match note.place {
                    NotePlace::LeftOf(i) => ("left of", vec![name(i)]),
                    NotePlace::RightOf(i) => ("right of", vec![name(i)]),
                    NotePlace::Over(a, b) if a == b => ("over", vec![name(a)]),
                    NotePlace::Over(a, b) => ("over", vec![name(a), name(b)]),
                    NotePlace::Across => ("across", Vec::new()),
                    NotePlace::Message(_) => ("on the message before", Vec::new()),
                };
                let mut out = Map::new();
                out.insert("text".into(), json!(plain(&note.text.value)));
                out.insert("place".into(), json!(place));
                if !over.is_empty() {
                    out.insert("over".into(), json!(over));
                }
                out.insert("line".into(), json!(line(note.line)));
                notes.push(Value::Object(out));
            }
            Item::GroupStart { kind, label, .. } => {
                let kind = kind.keyword().to_owned();
                let at = named(&kind, label.as_ref());
                groups.push((kind, at));
            }
            Item::GroupElse { label, .. } => {
                if let Some((kind, at)) = groups.last_mut() {
                    *at = named(&format!("{kind} else"), label.as_ref());
                }
            }
            Item::GroupEnd { .. } => {
                groups.pop();
            }
            _ => {}
        }
    }
    json!({ "participants": participants, "messages": messages, "notes": notes })
}

fn shape(shape: Shape) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_tells_its_states_and_transitions_on_their_lines() {
        let door = "@startuml\n[*] --> Closed\nClosed --> Open : open\nClosed --> Locked : lock [key] / click\nstate Open <<decide>>\n@enduml\n";
        let (key, machine) = inspected(door, Language::PlantUml).unwrap();
        assert_eq!(key, "machine");
        assert_eq!(
            machine["transitions"],
            json!([
                {"from": "[*]", "to": "Closed", "line": 2},
                {"from": "Closed", "to": "Open", "events": ["open"], "line": 3},
                {"from": "Closed", "to": "Locked", "events": ["lock"], "guard": "key", "action": "click", "line": 4},
            ])
        );
        let states = machine["states"].as_array().unwrap();
        assert_eq!(states.len(), 3, "{machine}");
        assert_eq!(
            states[0],
            json!({"name": "Closed", "kind": "state", "line": 2})
        );
        assert!(
            states
                .iter()
                .any(|s| s["name"] == "Open" && s["decide"] == true),
            "{machine}"
        );
        // The same machine in Mermaid, its lines its own.
        let mermaid =
            "---\ntitle: Door\n---\nstateDiagram-v2\n  [*] --> Closed\n  Closed --> Open : open\n";
        let (_, machine) = inspected(mermaid, Language::Mermaid).unwrap();
        assert_eq!(
            machine["transitions"][1],
            json!({"from": "Closed", "to": "Open", "events": ["open"], "line": 6})
        );
    }

    #[test]
    fn a_scenario_tells_its_messages_in_their_groups_and_its_notes() {
        let checkout = "@startuml\nactor Customer\nparticipant \"The shop\" as Shop\nCustomer -> Shop : order\nalt paid\n  Shop --> Customer : receipt\nelse declined\n  Shop --> Customer : sorry\nend\nnote over Customer, Shop : done\n@enduml\n";
        let (key, scenario) = inspected(checkout, Language::PlantUml).unwrap();
        assert_eq!(key, "scenario");
        assert_eq!(
            scenario["participants"],
            json!([
                {"name": "Customer", "kind": "actor", "line": 2},
                {"name": "Shop", "label": "The shop", "kind": "participant", "line": 3},
            ])
        );
        assert_eq!(
            scenario["messages"],
            json!([
                {"from": "Customer", "to": "Shop", "text": "order", "line": 4},
                {"from": "Shop", "to": "Customer", "text": "receipt", "reply": true, "in": ["alt paid"], "line": 6},
                {"from": "Shop", "to": "Customer", "text": "sorry", "reply": true, "in": ["alt else declined"], "line": 8},
            ])
        );
        assert_eq!(
            scenario["notes"],
            json!([{"text": "done", "place": "over", "over": ["Customer", "Shop"], "line": 10}])
        );
        // Another kind of diagram has none.
        assert!(inspected("@startuml\nclass A\n@enduml\n", Language::PlantUml).is_none());
    }
}
