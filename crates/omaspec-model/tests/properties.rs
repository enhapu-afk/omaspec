//! Properties that hold for every state machine and every scenario, checked
//! on generated ones: converted to the other language and back, formatted,
//! renamed and renamed back, the behavior stays the same. A failure is
//! shrunk to the smallest machine that shows it.
//!
//! `PROPTEST_CASES` sets how many to try (default 256 each).

use omaspec_core::{Language, format};
use omaspec_model::{File, FileKind, Specs, Thing, convert, diff, rename};
use proptest::prelude::*;

const STATES: &[&str] = &["Idle", "Busy", "Open", "Closed", "Locked", "Done", "Wait"];
const EVENTS: &[&str] = &["go", "stop", "open", "close", "tick", "retry"];
const GUARDS: &[&str] = &["ok", "free", "n > 1"];
const ACTIONS: &[&str] = &["start", "log", "reset"];

/// One transition: from a state or the start, to a state or the end, with
/// an event, a guard and an action, each perhaps.
#[derive(Debug, Clone)]
struct Line {
    from: Option<usize>,
    to: Option<usize>,
    event: Option<usize>,
    guard: Option<usize>,
    action: Option<usize>,
}

/// A machine: its states, its lines, and perhaps one state that holds a
/// small machine of its own.
#[derive(Debug, Clone)]
struct Machine {
    states: usize,
    lines: Vec<Line>,
    nested: Option<usize>,
}

fn machine() -> impl Strategy<Value = Machine> {
    (2..=STATES.len()).prop_flat_map(|states| {
        let line = (
            prop::option::weighted(0.9, 0..states),
            prop::option::weighted(0.9, 0..states),
            prop::option::weighted(0.8, 0..EVENTS.len()),
            prop::option::weighted(0.3, 0..GUARDS.len()),
            prop::option::weighted(0.3, 0..ACTIONS.len()),
        )
            .prop_map(|(from, to, event, guard, action)| Line {
                from,
                to,
                event,
                guard,
                action,
            });
        (
            Just(states),
            prop::collection::vec(line, 1..10),
            prop::option::weighted(0.3, 0..states),
        )
            .prop_map(|(states, lines, nested)| Machine {
                states,
                lines,
                nested,
            })
    })
}

/// `event [guard] / action`, as both languages write a transition's label.
fn label(line: &Line) -> String {
    let mut out = String::new();
    if let Some(e) = line.event {
        out.push_str(EVENTS[e]);
    }
    if let Some(g) = line.guard {
        out.push_str(&format!(" [{}]", GUARDS[g]));
    }
    if let Some(a) = line.action {
        out.push_str(&format!(" / {}", ACTIONS[a]));
    }
    out.trim().to_owned()
}

/// The machine in PlantUML or in Mermaid.
fn write(m: &Machine, language: Language) -> String {
    let (head, indent, end) = match language {
        Language::PlantUml => ("@startuml\n", "", "@enduml\n"),
        Language::Mermaid => ("stateDiagram-v2\n", "  ", ""),
    };
    let mut out = format!("{head}{indent}[*] --> {}\n", STATES[0]);
    for line in &m.lines {
        let from = line.from.map_or("[*]", |s| STATES[s]);
        let to = line.to.map_or("[*]", |s| STATES[s]);
        // A start has no event, and nothing goes back into it.
        if line.from.is_none() && line.to.is_none() {
            continue;
        }
        let text = if line.from.is_none() {
            String::new()
        } else {
            label(line)
        };
        if text.is_empty() {
            out.push_str(&format!("{indent}{from} --> {to}\n"));
        } else {
            out.push_str(&format!("{indent}{from} --> {to} : {text}\n"));
        }
    }
    if let Some(s) = m.nested {
        out.push_str(&format!(
            "{indent}state {} {{\n{indent}  [*] --> Inner\n{indent}  Inner --> Deep : tick\n{indent}}}\n",
            STATES[s]
        ));
    }
    out.push_str(end);
    out
}

/// What changed in the behavior from `old` to `new`, in words.
fn changes(old: &str, old_lang: Language, new: &str, new_lang: Language) -> Vec<String> {
    match diff(old, old_lang, new, new_lang, &Specs::default()) {
        Ok(changes) => changes.into_iter().map(|c| c.message).collect(),
        Err(why) => vec![format!("no diff: {why}")],
    }
}

fn other(language: Language) -> Language {
    match language {
        Language::PlantUml => Language::Mermaid,
        Language::Mermaid => Language::PlantUml,
    }
}

proptest! {
    /// Into the other language and back: the same behavior, each way.
    #[test]
    fn a_machine_keeps_its_behavior_in_the_other_language(m in machine(), mermaid in any::<bool>()) {
        let from = if mermaid { Language::Mermaid } else { Language::PlantUml };
        let to = other(from);
        let text = write(&m, from);
        let there = convert(&text, from, to).map_err(|r| TestCaseError::fail(format!("{r:?}\n{text}")))?;
        // Only where nothing was left out on the way.
        prop_assume!(there.notices.is_empty());
        let lost = changes(&text, from, &there.code, to);
        prop_assert!(lost.is_empty(), "{lost:?}\n{text}\n{}", there.code);
        let back = convert(&there.code, to, from).map_err(|r| TestCaseError::fail(format!("{r:?}\n{}", there.code)))?;
        let lost = changes(&text, from, &back.code, from);
        prop_assert!(lost.is_empty(), "{lost:?}\n{text}\n{}", back.code);
    }

    /// Formatting changes how a machine is written, never what it does,
    /// and a second pass changes nothing.
    #[test]
    fn formatting_keeps_a_machines_behavior(m in machine(), mermaid in any::<bool>()) {
        let language = if mermaid { Language::Mermaid } else { Language::PlantUml };
        let text = write(&m, language);
        let once = format::format(&text, language);
        let lost = changes(&text, language, &once, language);
        prop_assert!(lost.is_empty(), "{lost:?}\n{text}\n{once}");
        prop_assert_eq!(format::format(&once, language), once);
    }

    /// A state renamed and renamed back: the text as it was.
    #[test]
    fn a_state_renamed_and_back_is_the_same_text(m in machine(), pick in any::<prop::sample::Index>(), mermaid in any::<bool>()) {
        let language = if mermaid { Language::Mermaid } else { Language::PlantUml };
        let text = write(&m, language);
        let path = if mermaid { "door.mmd" } else { "door.puml" };
        let state = STATES[pick.index(m.states)];
        prop_assume!(text.contains(state));
        let files = |text: &'static str| vec![File { path, text, kind: FileKind::Diagram(language) }];
        let first: &'static str = Box::leak(text.clone().into_boxed_str());
        let plan = rename(state, "Renamed", None, Some(Thing::State), &files(first), &Specs::default())
            .map_err(|why| TestCaseError::fail(format!("{why}\n{text}")))?;
        let renamed = plan.apply(path, &text);
        prop_assert!(!renamed.contains(&format!(" {state} ")) || renamed.contains("Renamed"), "{renamed}");
        let second: &'static str = Box::leak(renamed.clone().into_boxed_str());
        let back = rename("Renamed", state, None, Some(Thing::State), &files(second), &Specs::default())
            .map_err(|why| TestCaseError::fail(format!("{why}\n{renamed}")))?;
        prop_assert_eq!(back.apply(path, &renamed), text);
    }
}

const PEOPLE: &[&str] = &["User", "Shop", "Bank", "Mail", "Stock"];
const WORDS: &[&str] = &["order", "pay", "paid", "ship", "done", "retry"];

/// One part of a scenario: a message, or a block of messages.
#[derive(Debug, Clone)]
enum Part {
    Message {
        from: usize,
        to: usize,
        word: usize,
        reply: bool,
    },
    Block {
        kind: usize,
        first: Vec<Part>,
        second: Vec<Part>,
    },
}

fn part(people: usize) -> impl Strategy<Value = Part> {
    let message = (0..people, 0..people, 0..WORDS.len(), any::<bool>()).prop_map(
        |(from, to, word, reply)| Part::Message {
            from,
            to,
            word,
            reply,
        },
    );
    message.prop_recursive(2, 12, 4, move |inner| {
        (
            0..3usize,
            prop::collection::vec(inner.clone(), 1..4),
            prop::collection::vec(inner, 0..3),
        )
            .prop_map(|(kind, first, second)| Part::Block {
                kind,
                first,
                second,
            })
    })
}

fn scenario() -> impl Strategy<Value = (usize, Vec<Part>)> {
    (2..=PEOPLE.len())
        .prop_flat_map(|people| (Just(people), prop::collection::vec(part(people), 1..8)))
}

/// The scenario in PlantUML: `alt`/`else`, `loop` and `opt` blocks.
fn write_scenario(people: usize, parts: &[Part]) -> String {
    fn parts_into(out: &mut String, parts: &[Part], depth: usize) {
        let pad = "  ".repeat(depth);
        for part in parts {
            match part {
                Part::Message {
                    from,
                    to,
                    word,
                    reply,
                } => {
                    let arrow = if *reply { "-->" } else { "->" };
                    out.push_str(&format!(
                        "{pad}{} {arrow} {} : {}\n",
                        PEOPLE[*from], PEOPLE[*to], WORDS[*word]
                    ));
                }
                Part::Block {
                    kind,
                    first,
                    second,
                } => {
                    let head = ["alt ok", "loop again", "opt maybe"][*kind];
                    out.push_str(&format!("{pad}{head}\n"));
                    parts_into(out, first, depth + 1);
                    if *kind == 0 && !second.is_empty() {
                        out.push_str(&format!("{pad}else no\n"));
                        parts_into(out, second, depth + 1);
                    }
                    out.push_str(&format!("{pad}end\n"));
                }
            }
        }
    }
    let mut out = String::from("@startuml\n");
    for p in &PEOPLE[..people] {
        out.push_str(&format!("participant {p}\n"));
    }
    parts_into(&mut out, parts, 0);
    out.push_str("@enduml\n");
    out
}

proptest! {
    /// A scenario into Mermaid and back: the same messages in the same
    /// blocks, each way.
    #[test]
    fn a_scenario_keeps_its_behavior_in_the_other_language((people, parts) in scenario()) {
        let text = write_scenario(people, &parts);
        let there = convert(&text, Language::PlantUml, Language::Mermaid)
            .map_err(|r| TestCaseError::fail(format!("{r:?}\n{text}")))?;
        prop_assume!(there.notices.is_empty());
        let lost = changes(&text, Language::PlantUml, &there.code, Language::Mermaid);
        prop_assert!(lost.is_empty(), "{lost:?}\n{text}\n{}", there.code);
        let back = convert(&there.code, Language::Mermaid, Language::PlantUml)
            .map_err(|r| TestCaseError::fail(format!("{r:?}\n{}", there.code)))?;
        let lost = changes(&text, Language::PlantUml, &back.code, Language::PlantUml);
        prop_assert!(lost.is_empty(), "{lost:?}\n{text}\n{}", back.code);
    }

    /// A comment anywhere in a machine changes nothing of its picture.
    #[test]
    fn a_comment_changes_nothing_drawn(m in machine(), at in any::<prop::sample::Index>()) {
        let text = write(&m, Language::PlantUml);
        let mut lines: Vec<&str> = text.lines().collect();
        // After the start line, before the end.
        let spot = 1 + at.index(lines.len() - 1);
        lines.insert(spot, "' a remark");
        let commented = lines.join("\n") + "\n";
        prop_assert_eq!(
            omaspec_uml::render(&text).ok(),
            omaspec_uml::render(&commented).ok(),
            "{}",
            commented
        );
    }
}
