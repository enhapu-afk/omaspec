//! What the editor offers while a diagram is written: the names it has so
//! far, the events its state machine takes, the events a participant's
//! state machine in the project takes, and the words and blocks of its
//! language ([`suggest`]); and what a name under the pointer is
//! ([`describe`]).
//!
//! A line half written rarely reads as a diagram, so the line the caret
//! is in is left out of what the readers get: the rest mostly reads, and
//! the word being typed is not offered back. Where even the rest does not
//! read, the words of the diagram are still offered.

mod marks;
mod words;

pub use marks::{Mark, Tone, marks};
pub use words::block_texts;

use std::collections::HashSet;

use omaspec_core::{Kind as DiagramKind, Language, classify};

use crate::across::{event_of, read_sequence};
use crate::{Kind, Machine, Specs};

/// What a suggestion is, for its mark in the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    /// A state, a participant, a box: a name of this diagram.
    Name,
    /// An event a state machine takes, or a message sent before.
    Event,
    /// A state machine of the project.
    Machine,
    /// A word of the language.
    Keyword,
    /// A block of the language, with places to fill in.
    Block,
}

/// One thing to write at the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// What the list shows, and what is written unless `snippet` says more.
    pub label: String,
    pub what: What,
    /// A few words beside it; may be empty.
    pub detail: String,
    /// A block with its places to fill in, as LSP writes them (`${1:name}`,
    /// `$0`).
    pub snippet: Option<String>,
    /// Lower comes first: what the place asks for before what always fits.
    pub tier: u8,
}

impl Suggestion {
    fn new(label: impl Into<String>, what: What, detail: impl Into<String>, tier: u8) -> Self {
        Suggestion {
            label: label.into(),
            what,
            detail: detail.into(),
            snippet: None,
            tier,
        }
    }
}

/// Where in its line the caret stands.
enum Place<'a> {
    /// At the line's first word.
    First,
    /// Where a label starts, after the `:` of the line `head`.
    Label { head: &'a str },
    /// After an arrow, where a name comes.
    Name,
    /// Anywhere else.
    Other,
}

/// What to offer in `code` on its zero-based `line`, whose text up to the
/// caret is `before`. `asked` is true when the writer asked for the list
/// (Ctrl+Space): then it also comes where nothing was typed yet.
pub fn suggest(
    code: &str,
    lang: Language,
    line: usize,
    before: &str,
    asked: bool,
    specs: &Specs,
) -> Vec<Suggestion> {
    let comment = match lang {
        Language::PlantUml => "'",
        Language::Mermaid => "%%",
    };
    if before.trim_start().starts_with(comment) {
        return Vec::new();
    }
    let typed = before
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '-')
        .last()
        .map_or(before.len(), |(at, _)| at);
    let (head, word) = (before[..typed].trim(), &before[typed..]);
    let rest = without_line(code, line);
    let kind = classify(&rest, lang);
    let labelled = matches!(kind.kind, DiagramKind::State | DiagramKind::Sequence);

    let place = if head.is_empty() {
        Place::First
    } else if labelled && head.ends_with(':') && head.contains(['-', '>', '<']) {
        Place::Label {
            head: head[..head.len() - 1].trim_end(),
        }
    } else if labelled && words::outside_quotes(head, ':') {
        // Further into a label: its guard, its action, its second word.
        return Vec::new();
    } else if head.ends_with(['>', '-', '|', '&']) {
        Place::Name
    } else {
        Place::Other
    };
    // A space alone opens the list only where a name or an event comes.
    if word.is_empty() && !asked && !matches!(place, Place::Label { .. } | Place::Name) {
        return Vec::new();
    }

    let mut out: Vec<Suggestion> = Vec::new();
    match place {
        Place::Label { head } => {
            match kind.kind {
                DiagramKind::State => events_of_machine(&rest, lang, &mut out),
                _ => messages(&rest, lang, head, specs, &mut out),
            }
            return unique(out);
        }
        Place::First => {
            names(&rest, lang, kind.kind, specs, &mut out);
            words::of_language(lang, &kind, &mut out);
        }
        Place::Name | Place::Other => names(&rest, lang, kind.kind, specs, &mut out),
    }
    let known: HashSet<String> = out.iter().map(|s| s.label.clone()).collect();
    out.extend(
        words::written(&rest, lang, &kind)
            .into_iter()
            .filter(|w| !known.contains(w))
            .map(|w| Suggestion::new(w, What::Name, "", 4)),
    );
    unique(out)
}

/// `code` with its line `line` empty.
fn without_line(code: &str, line: usize) -> String {
    code.split('\n')
        .enumerate()
        .map(|(at, text)| if at == line { "" } else { text })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Each label once, the first of its tier's order.
fn unique(mut out: Vec<Suggestion>) -> Vec<Suggestion> {
    out.sort_by_key(|s| s.tier);
    let mut seen = HashSet::new();
    out.retain(|s| !s.label.is_empty() && seen.insert(s.label.clone()));
    out
}

/// The names of the diagram: its states, or its participants and the
/// project's state machines that could be one.
fn names(code: &str, lang: Language, kind: DiagramKind, specs: &Specs, out: &mut Vec<Suggestion>) {
    match kind {
        DiagramKind::State => {
            if let Some(machine) = Machine::read(code, lang) {
                for state in machine.states.iter().filter(|s| named(s.kind)) {
                    out.push(Suggestion::new(&state.name, What::Name, "state", 1));
                }
            }
        }
        DiagramKind::Sequence => {
            if let Some((diagram, _)) = read_sequence(code, lang) {
                for p in &diagram.participants {
                    let detail = if p.label == p.code {
                        "participant"
                    } else {
                        p.label.as_str()
                    };
                    out.push(Suggestion::new(&p.code, What::Name, detail, 1));
                }
            }
            for spec in specs.iter() {
                if let Some(name) = spec.names.iter().find(|n| words::is_word(n)) {
                    let detail = format!("state machine · {}", spec.file);
                    out.push(Suggestion::new(name, What::Machine, detail, 3));
                }
            }
        }
        _ => {}
    }
}

/// Whether a state of this kind has a name transitions write.
fn named(kind: Kind) -> bool {
    matches!(kind, Kind::State | Kind::Choice | Kind::Fork | Kind::Join)
}

/// The events the diagram's own machine takes so far.
fn events_of_machine(code: &str, lang: Language, out: &mut Vec<Suggestion>) {
    let Some(machine) = Machine::read(code, lang) else {
        return;
    };
    for event in machine.transitions.iter().flat_map(|t| &t.trigger.events) {
        out.push(Suggestion::new(event, What::Event, "event", 1));
    }
}

/// What a message of the line `head` (`A -> B`) may say: the events the
/// receiver's state machine takes, then what the scenario sent before.
fn messages(code: &str, lang: Language, head: &str, specs: &Specs, out: &mut Vec<Suggestion>) {
    let diagram = read_sequence(code, lang).map(|(diagram, _)| diagram);
    if let Some(receiver) = receiver(head) {
        let label = diagram
            .as_ref()
            .and_then(|d| d.participants.iter().find(|p| p.code == receiver))
            .map_or(receiver, |p| p.label.as_str());
        if let [spec] = specs.named(&[receiver, label]).as_slice() {
            let name = spec
                .names
                .first()
                .map_or(spec.file.as_str(), String::as_str);
            for event in spec
                .machine
                .transitions
                .iter()
                .flat_map(|t| &t.trigger.events)
            {
                let detail = format!("{name} takes it");
                out.push(Suggestion::new(event, What::Event, detail, 0));
            }
        }
    }
    for item in diagram.iter().flat_map(|d| &d.items) {
        if let omaspec_uml::sequence::Item::Message(m) = item
            && let Some(text) = &m.text
        {
            let said = event_of(&text.value);
            out.push(Suggestion::new(said, What::Event, "sent before", 2));
        }
    }
}

/// Who receives the message `head` starts (`A -> B`, `A <- B`,
/// `A->>+B`), as written.
fn receiver(head: &str) -> Option<&str> {
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '.';
    let first = match head.strip_prefix('"') {
        Some(rest) => rest.find('"')? + 2,
        None => head.find(|c| !word(c)).unwrap_or(head.len()),
    };
    let rest = &head[first..];
    let last = match rest.strip_suffix('"') {
        Some(open) => open.rfind('"')?,
        None => rest
            .char_indices()
            .rev()
            .take_while(|(_, c)| word(*c))
            .last()
            .map_or(rest.len(), |(at, _)| at),
    };
    let arrow = rest[..last].trim();
    if first == 0 || last == rest.len() || !arrow.contains(['-', '<', '>', ')']) {
        return None;
    }
    let to = if arrow.starts_with('<') && !arrow.ends_with('>') {
        &head[..first]
    } else {
        &rest[last..]
    };
    Some(to.trim_matches('"'))
}

/// What the name `word` is in `code`, as a few lines of Markdown; `None`
/// for a word the diagram and the project do not know.
pub fn describe(code: &str, lang: Language, word: &str, specs: &Specs) -> Option<String> {
    if word.is_empty() {
        return None;
    }
    let mut said: Vec<String> = Vec::new();
    match classify(code, lang).kind {
        DiagramKind::State => {
            if let Some(machine) = Machine::read(code, lang) {
                said.extend(state(&machine, word));
                said.extend(event(&machine, word));
            }
        }
        DiagramKind::Sequence => {
            if let Some((diagram, _)) = read_sequence(code, lang) {
                said.extend(participant(&diagram, word, specs));
            }
        }
        _ => {}
    }
    if said.is_empty()
        && let [spec] = specs.named(&[word]).as_slice()
    {
        said.push(machine_line(spec));
    }
    (!said.is_empty()).then(|| said.join("\n\n"))
}

/// A list of at most six names in code marks, with how many more.
fn listed<'a>(names: impl IntoIterator<Item = &'a str>) -> String {
    let mut seen = HashSet::new();
    let names: Vec<&str> = names.into_iter().filter(|n| seen.insert(*n)).collect();
    let mut out: Vec<String> = names.iter().take(6).map(|n| format!("`{n}`")).collect();
    if names.len() > 6 {
        out.push(format!("{} more", names.len() - 6));
    }
    out.join(", ")
}

fn machine_line(spec: &crate::Spec) -> String {
    let states = spec.machine.states.iter().filter(|s| named(s.kind)).count();
    let events = listed(
        spec.machine
            .transitions
            .iter()
            .flat_map(|t| &t.trigger.events)
            .map(String::as_str),
    );
    let takes = if events.is_empty() {
        String::new()
    } else {
        format!(", takes {events}")
    };
    format!(
        "**state machine** in `{}`: {states} states{takes}",
        spec.file
    )
}

/// The state `word` of `machine`: where it is and what leaves it.
fn state(machine: &Machine, word: &str) -> Option<String> {
    let at = machine
        .states
        .iter()
        .position(|s| s.name == word && named(s.kind))?;
    let ins = machine.transitions.iter().filter(|t| t.to == at).count();
    let outs: Vec<_> = machine
        .transitions
        .iter()
        .filter(|t| t.from == at)
        .collect();
    let mut said = format!(
        "**state** `{word}`, line {}: {ins} in, {} out",
        machine.states[at].line + 1,
        outs.len()
    );
    if let Some(parent) = machine.states[at].parent {
        said.push_str(&format!(", in `{}`", machine.states[parent].name));
    }
    let events = listed(
        outs.iter()
            .flat_map(|t| &t.trigger.events)
            .map(String::as_str),
    );
    if !events.is_empty() {
        said.push_str(&format!("\n\ntakes {events}"));
    }
    Some(said)
}

/// The event `word` of `machine`: the transitions it fires.
fn event(machine: &Machine, word: &str) -> Option<String> {
    let fired: Vec<String> = machine
        .transitions
        .iter()
        .filter(|t| t.trigger.events.iter().any(|e| e == word))
        .map(|t| {
            let name = |at: usize| match machine.states[at].name.as_str() {
                "" => "[*]",
                name => name,
            };
            format!("`{}` → `{}`", name(t.from), name(t.to))
        })
        .collect();
    if fired.is_empty() {
        return None;
    }
    let more = match fired.len() {
        0..=5 => String::new(),
        n => format!(", {} more", n - 5),
    };
    Some(format!(
        "**event** `{word}`: {}{more}",
        fired[..fired.len().min(5)].join(", ")
    ))
}

/// The participant `word` of a scenario: what it sends and receives, and
/// its state machine in the project.
fn participant(
    diagram: &omaspec_uml::sequence::Diagram,
    word: &str,
    specs: &Specs,
) -> Option<String> {
    use omaspec_uml::sequence::{End, Item};
    let at = diagram
        .participants
        .iter()
        .position(|p| p.code == word || p.label == word)?;
    let p = &diagram.participants[at];
    let (mut sends, mut receives) = (0, 0);
    for item in &diagram.items {
        if let Item::Message(m) = item {
            sends += usize::from(m.from == End::Participant(at));
            receives += usize::from(m.to == End::Participant(at));
        }
    }
    let shown = if p.label == p.code {
        String::new()
    } else {
        format!(" ({})", p.label)
    };
    let mut said = format!(
        "**participant** `{}`{shown}: sends {sends}, receives {receives}",
        p.code
    );
    if let [spec] = specs.named(&[&p.code, &p.label]).as_slice() {
        said.push_str("\n\n");
        said.push_str(&machine_line(spec));
    }
    Some(said)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Spec;

    const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close [clear] / beep\nstate Open {\n  state Wide\n}\n@enduml\n";

    fn labels(found: &[Suggestion]) -> Vec<&str> {
        found.iter().map(|s| s.label.as_str()).collect()
    }

    /// What comes with the caret at the end of `code`.
    fn at_end(code: &str, lang: Language, specs: &Specs) -> Vec<Suggestion> {
        let line = code.split('\n').count() - 1;
        let before = code.rsplit('\n').next().unwrap();
        suggest(code, lang, line, before, false, specs)
    }

    fn door() -> Specs {
        Specs::new(vec![
            Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap(),
        ])
    }

    #[test]
    fn a_state_machine_offers_its_states_and_after_the_colon_its_events() {
        let none = Specs::default();
        let code = "@startuml\n[*] --> Idle\nIdle --> Running : start\nRunning --> Idle : stop\nRunning --> I";
        let found = at_end(code, Language::PlantUml, &none);
        assert_eq!(&labels(&found)[..2], ["Idle", "Running"]);
        assert!(found.iter().all(|s| s.what == What::Name), "{found:?}");
        assert_eq!(found[0].detail, "state");

        // After an arrow a space is enough; after a name it is not.
        let after_arrow = format!("{}--> ", &code[..code.len() - 5]);
        assert_eq!(
            &labels(&at_end(&after_arrow, Language::PlantUml, &none))[..2],
            ["Idle", "Running"]
        );
        let after_name = format!("{} ", &code[..code.len() - 6]);
        assert_eq!(at_end(&after_name, Language::PlantUml, &none), []);

        let label = format!("{}dle : ", code);
        let found = at_end(&label, Language::PlantUml, &none);
        assert_eq!(labels(&found), ["start", "stop"]);
        assert!(found.iter().all(|s| s.what == What::Event));
        // Its guard and its action are the writer's.
        let guard = format!("{label}start [");
        assert_eq!(at_end(&guard, Language::PlantUml, &none), []);
    }

    #[test]
    fn the_word_being_typed_is_not_offered_back() {
        let none = Specs::default();
        let code = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Runn\n  Idle --> Done\n";
        let found = suggest(code, Language::Mermaid, 2, "  Idle --> Runn", false, &none);
        assert_eq!(labels(&found), ["Idle", "Done"]);
    }

    #[test]
    fn a_message_offers_the_events_its_receiver_s_machine_takes() {
        let specs = door();
        let code = "@startuml\nactor User\nparticipant \"Front door\" as Door\nUser -> Door : knock\nUser -> Door : ";
        let found = at_end(code, Language::PlantUml, &specs);
        assert_eq!(labels(&found), ["open", "close", "knock"]);
        assert_eq!(found[0].detail, "Door takes it");
        assert_eq!(found[2].detail, "sent before");
        // To someone without a machine: what was sent before.
        let back = "@startuml\nactor User\nUser -> Door : knock\nDoor --> User : ";
        assert_eq!(labels(&at_end(back, Language::PlantUml, &specs)), ["knock"]);
        // The arrow may point back, and Mermaid writes it its way.
        assert_eq!(receiver("User <- Door"), Some("User"));
        assert_eq!(receiver("User->>+Door"), Some("Door"));
        assert_eq!(receiver("\"A b\" --> Door"), Some("Door"));
        let mermaid = "sequenceDiagram\n  participant User\n  User->>Door: ";
        assert_eq!(
            labels(&at_end(mermaid, Language::Mermaid, &specs)),
            ["open", "close"]
        );
    }

    #[test]
    fn a_scenario_offers_its_participants_and_the_project_s_machines() {
        let specs = door();
        let code = "@startuml\nactor User\nparticipant \"Web app\" as web\nUser -> ";
        let found = at_end(code, Language::PlantUml, &specs);
        assert_eq!(&labels(&found)[..3], ["User", "web", "Door"]);
        assert_eq!(found[1].detail, "Web app");
        assert_eq!(found[2].what, What::Machine);
        assert_eq!(found[2].detail, "state machine · door.puml");
    }

    #[test]
    fn a_line_s_first_word_offers_the_language_too() {
        let none = Specs::default();
        let code = "@startuml\nactor User\nUser -> App : hi\na";
        let found = at_end(code, Language::PlantUml, &none);
        let alt = found.iter().find(|s| s.label == "alt").expect("alt");
        assert_eq!(alt.what, What::Block);
        assert!(alt.snippet.as_deref().unwrap().contains("else"));
        assert!(labels(&found).contains(&"activate"));
        assert!(labels(&found).contains(&"App"), "a word of the diagram");
        // Mermaid's words for a flowchart, and none of PlantUML's.
        let flow = "flowchart TD\n  A[Start here] --> B\n  s";
        let found = at_end(flow, Language::Mermaid, &none);
        assert!(labels(&found).contains(&"subgraph"));
        assert!(!labels(&found).contains(&"skinparam"));
        assert!(!labels(&found).contains(&"Start"), "a label is no name");
        // Nothing in a comment, and nothing on an empty line unless asked.
        assert_eq!(at_end("@startuml\n' al", Language::PlantUml, &none), []);
        assert_eq!(at_end("@startuml\n", Language::PlantUml, &none), []);
        let asked = suggest("@startuml\n", Language::PlantUml, 1, "", true, &none);
        assert!(labels(&asked).contains(&"participant"), "{asked:?}");
    }

    #[test]
    fn a_name_under_the_pointer_says_what_it_is() {
        let specs = door();
        let said = describe(DOOR, Language::PlantUml, "Open", &Specs::default()).unwrap();
        assert_eq!(
            said,
            "**state** `Open`, line 4: 1 in, 1 out\n\ntakes `close`"
        );
        let said = describe(DOOR, Language::PlantUml, "Wide", &Specs::default()).unwrap();
        assert!(said.ends_with("0 in, 0 out, in `Open`"), "{said}");
        assert_eq!(
            describe(DOOR, Language::PlantUml, "open", &Specs::default()).unwrap(),
            "**event** `open`: `Closed` → `Open`"
        );
        let scenario = "@startuml\nactor User\nparticipant \"Front door\" as Door\nUser -> Door : open\n@enduml\n";
        assert_eq!(
            describe(scenario, Language::PlantUml, "Door", &specs).unwrap(),
            "**participant** `Door` (Front door): sends 0, receives 1\n\n\
             **state machine** in `door.puml`: 3 states, takes `open`, `close`"
        );
        assert_eq!(
            describe(scenario, Language::PlantUml, "knock", &specs),
            None
        );
        assert_eq!(describe(DOOR, Language::PlantUml, "", &specs), None);
    }
}
