//! What the words of a state machine and of a scenario are, for the
//! editor to color: the states and participants a line names, and in a
//! transition's label its event, its guard and its action. A message is an
//! event where its receiver's state machine in the project takes it.
//!
//! Read line by line and without the readers, so a diagram half written
//! keeps its colors; what a line is not sure to be gets none.

use std::collections::HashMap;
use std::ops::Range;

use omaspec_core::{Kind as DiagramKind, Language, classify};

use super::words::{outside_quotes, starts_with_word};
use crate::Specs;
use crate::across::event_of;

/// What a marked run of a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tone {
    /// A state or a participant.
    Name,
    /// What fires a transition, or a message a state machine takes.
    Event,
    /// The condition in `[` and `]`.
    Guard,
    /// What happens, after `/`.
    Action,
}

/// A run of a line and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    /// Zero-based line.
    pub line: usize,
    /// Bytes within the line.
    pub range: Range<usize>,
    pub tone: Tone,
}

/// Words a line of a scenario starts with when it is no message.
pub(crate) const NO_MESSAGE: &[&str] = &[
    "alt",
    "else",
    "opt",
    "loop",
    "par",
    "and",
    "group",
    "critical",
    "option",
    "break",
    "box",
    "rect",
    "end",
    "ref",
    "return",
    "activate",
    "deactivate",
    "destroy",
    "create",
    "title",
    "skinparam",
    "hide",
    "autonumber",
    "legend",
    "caption",
    "header",
    "footer",
    "state",
];

/// Words that declare a participant.
const DECLARES: &[&str] = &[
    "participant",
    "actor",
    "boundary",
    "control",
    "entity",
    "database",
    "collections",
    "queue",
];

/// The marks of `code`, in the order of its lines; none for a diagram
/// that is neither a state machine nor a scenario.
pub fn marks(code: &str, lang: Language, specs: &Specs) -> Vec<Mark> {
    let kind = classify(code, lang).kind;
    if !matches!(kind, DiagramKind::State | DiagramKind::Sequence) {
        return Vec::new();
    }
    let comment = match lang {
        Language::PlantUml => "'",
        Language::Mermaid => "%%",
    };
    let lines: Vec<(usize, usize, &str)> = code
        .split('\n')
        .enumerate()
        .map(|(no, raw)| (no, raw.len() - raw.trim_start().len(), raw.trim()))
        .filter(|(_, _, text)| !text.is_empty() && !text.starts_with(comment))
        .filter(|(_, _, text)| !text.starts_with(['@', '!']))
        .collect();
    let mut out = Vec::new();
    let mut mark = |line: usize, lead: usize, range: Range<usize>, tone: Tone| {
        if !range.is_empty() {
            out.push(Mark {
                line,
                range: lead + range.start..lead + range.end,
                tone,
            });
        }
    };

    // Who a name stands for, to find a participant's state machine.
    let mut shown: HashMap<&str, &str> = HashMap::new();
    if kind == DiagramKind::Sequence {
        for (_, _, text) in &lines {
            if let Some((name, label)) = declared(text, lang) {
                shown.insert(&text[name], label);
            }
        }
    }

    let mut note = false;
    for (no, lead, text) in lines {
        // A note's lines are prose.
        if note {
            note = !text.to_ascii_lowercase().starts_with("end note") && text != "end";
            continue;
        }
        if starts_with_word(&text.to_ascii_lowercase(), "note") {
            note = !outside_quotes(text, ':');
            continue;
        }
        let (head, label) = match colon(text) {
            Some(at) => (&text[..at], Some(at + 1)),
            None => (text, None),
        };
        if kind == DiagramKind::Sequence {
            if let Some((name, _)) = declared(text, lang) {
                mark(no, lead, name, Tone::Name);
                continue;
            }
            if NO_MESSAGE.iter().any(|word| starts_with_word(head, word)) {
                continue;
            }
            let Some((from, arrow, to)) = ends(head.trim_end(), false) else {
                continue;
            };
            let back = arrow.starts_with('<') && !arrow.ends_with('>');
            let receiver = unquoted(&head[if back { from.clone() } else { to.clone() }]);
            mark(no, lead, from, Tone::Name);
            mark(no, lead, to, Tone::Name);
            let Some(start) = label else { continue };
            let said = event_of(&text[start..]);
            let label = shown.get(receiver).copied().unwrap_or(receiver);
            if let [spec] = specs.named(&[receiver, label]).as_slice()
                && takes(spec, said)
            {
                let at = start + (text[start..].len() - text[start..].trim_start().len());
                mark(no, lead, at..at + said.len(), Tone::Event);
            }
        } else if let Some(name) = state_declared(head) {
            mark(no, lead, name, Tone::Name);
        } else if let Some((from, _, to)) = ends(head.trim_end(), true) {
            for name in [from, to] {
                if !head[name.clone()].starts_with('[') {
                    mark(no, lead, name, Tone::Name);
                }
            }
            if let Some(start) = label {
                for (range, tone) in trigger(&text[start..]) {
                    mark(no, lead, start + range.start..start + range.end, tone);
                }
            }
        }
    }
    out
}

/// Whether `spec`'s machine has a transition `event` fires.
fn takes(spec: &crate::Spec, event: &str) -> bool {
    !event.is_empty()
        && spec
            .machine
            .transitions
            .iter()
            .any(|t| t.trigger.events.iter().any(|e| e == event))
}

/// `name` without quotes around it.
fn unquoted(name: &str) -> &str {
    name.trim_matches('"')
}

/// Where the first `:` outside quotes stands.
pub(crate) fn colon(text: &str) -> Option<usize> {
    let mut quoted = false;
    text.char_indices().find_map(|(at, c)| {
        quoted ^= c == '"';
        (!quoted && c == ':').then_some(at)
    })
}

/// How long the name is that `text` starts with: a word, a name in
/// quotes, or with `pseudo` a state such as `[*]`.
fn name_at_start(text: &str, pseudo: bool) -> usize {
    if let Some(rest) = text.strip_prefix('"') {
        return rest.find('"').map_or(0, |close| close + 2);
    }
    if pseudo && text.starts_with('[') {
        return ["[*]", "[H*]", "[H]"]
            .iter()
            .find(|p| text.starts_with(**p))
            .map_or(0, |p| p.len());
    }
    text.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(text.len())
}

/// Where the name starts that `text` ends with.
fn name_at_end(text: &str, pseudo: bool) -> usize {
    if let Some(rest) = text.strip_suffix('"') {
        return rest.rfind('"').unwrap_or(text.len());
    }
    if pseudo && text.ends_with(']') {
        return ["[*]", "[H*]", "[H]"]
            .iter()
            .find(|p| text.ends_with(**p))
            .map_or(text.len(), |p| text.len() - p.len());
    }
    text.char_indices()
        .rev()
        .take_while(|(_, c)| c.is_alphanumeric() || *c == '_' || *c == '.')
        .last()
        .map_or(text.len(), |(at, _)| at)
}

/// The two ends of the line `head` (`A --> B`) and the arrow between
/// them; `None` when it is no line from one name to another.
pub(crate) fn ends(head: &str, pseudo: bool) -> Option<(Range<usize>, &str, Range<usize>)> {
    let first = name_at_start(head, pseudo);
    let last = first + name_at_end(&head[first..], pseudo);
    let arrow = head[first..last].trim();
    let reads = !arrow.is_empty()
        && arrow.contains('-')
        && arrow.contains(['>', '<', ')'])
        && !arrow.contains(char::is_whitespace)
        && !arrow.contains('"');
    (first > 0 && last < head.len() && reads).then_some((0..first, arrow, last..head.len()))
}

/// The name a line declares as a participant, and what its head shows.
pub(crate) fn declared(text: &str, lang: Language) -> Option<(Range<usize>, &str)> {
    let word = DECLARES.iter().find(|word| starts_with_word(text, word))?;
    let rest = text[word.len()..].trim_start();
    let at = text.len() - rest.len();
    let first = name_at_start(rest, false);
    if first == 0 {
        return None;
    }
    let after = rest[first..].trim_start();
    let Some(second) = after.strip_prefix("as ").map(str::trim_start) else {
        return Some((at..at + first, unquoted(&rest[..first])));
    };
    let from = text.len() - second.len();
    let len = name_at_start(second, false);
    // Mermaid: `participant A as Alice`, the name first and what is shown
    // to the end of the line. PlantUML: the one without quotes is the name.
    if lang == Language::Mermaid {
        return Some((at..at + first, second.trim()));
    }
    if len == 0 {
        return None;
    }
    if second.starts_with('"') {
        Some((at..at + first, unquoted(&second[..len])))
    } else {
        Some((from..from + len, unquoted(&rest[..first])))
    }
}

/// The name a line declares as a state: `state Name`, `state "Long" as
/// Name`.
pub(crate) fn state_declared(head: &str) -> Option<Range<usize>> {
    if !starts_with_word(head, "state") {
        return None;
    }
    let rest = head["state".len()..].trim_start();
    let at = head.len() - rest.len();
    let first = name_at_start(rest, false);
    if first == 0 {
        return None;
    }
    match rest[first..].trim_start().strip_prefix("as ") {
        Some(name) => {
            let name = name.trim_start();
            let from = head.len() - name.len();
            let len = name_at_start(name, false);
            (len > 0).then_some(from..from + len)
        }
        None if rest.starts_with('"') => None,
        None => Some(at..at + first),
    }
}

/// The parts of a transition's label, `event [guard] / action`, as far
/// as it has them.
fn trigger(label: &str) -> Vec<(Range<usize>, Tone)> {
    let (mut depth, mut open, mut close, mut slash) = (0usize, None, None, None);
    for (at, c) in label.char_indices() {
        match c {
            '[' | '(' => {
                if depth == 0 && c == '[' && open.is_none() {
                    open = Some(at);
                }
                depth += 1;
            }
            ']' | ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 && c == ']' && open.is_some() && close.is_none() {
                    close = Some(at + 1);
                }
            }
            '/' if depth == 0 => {
                slash = Some(at);
                break;
            }
            _ => {}
        }
    }
    let end = slash.unwrap_or(label.len());
    let trimmed = |range: Range<usize>| {
        let text = &label[range.clone()];
        let start = range.start + (text.len() - text.trim_start().len());
        start..start + text.trim().len()
    };
    let mut out = Vec::new();
    out.push((trimmed(0..open.unwrap_or(end)), Tone::Event));
    if let (Some(open), Some(close)) = (open, close) {
        out.push((open..close, Tone::Guard));
    }
    if let Some(slash) = slash {
        out.push((trimmed(slash + 1..label.len()), Tone::Action));
    }
    out.retain(|(range, _)| !range.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Spec;

    /// Each mark as its line, its text and its tone.
    fn read(code: &str, lang: Language, specs: &Specs) -> Vec<(usize, String, Tone)> {
        let lines: Vec<&str> = code.split('\n').collect();
        marks(code, lang, specs)
            .into_iter()
            .map(|m| (m.line, lines[m.line][m.range].to_owned(), m.tone))
            .collect()
    }

    fn of(line: usize, text: &str, tone: Tone) -> (usize, String, Tone) {
        (line, text.to_owned(), tone)
    }

    #[test]
    fn a_transition_has_its_states_its_event_its_guard_and_its_action() {
        let code = "@startuml\n[*] --> Closed\n  Closed --> Open : open [unlocked (twice)] / beep / log\nstate \"Wide open\" as Wide\nstate Open {\n' Open --> Gone : no\nOpen -up-> Closed : close\nOpen : the door is open\n";
        use Tone::*;
        assert_eq!(
            read(code, Language::PlantUml, &Specs::default()),
            [
                of(1, "Closed", Name),
                of(2, "Closed", Name),
                of(2, "Open", Name),
                of(2, "open", Event),
                of(2, "[unlocked (twice)]", Guard),
                of(2, "beep / log", Action),
                of(3, "Wide", Name),
                of(4, "Open", Name),
                of(6, "Open", Name),
                of(6, "Closed", Name),
                of(6, "close", Event),
            ]
        );
        let mermaid = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy: start [ready]\n  note right of Idle\n    Idle --> Busy: no\n  end note\n";
        assert_eq!(
            read(mermaid, Language::Mermaid, &Specs::default()),
            [
                of(1, "Idle", Name),
                of(2, "Idle", Name),
                of(2, "Busy", Name),
                of(2, "start", Event),
                of(2, "[ready]", Guard),
            ]
        );
    }

    #[test]
    fn a_message_is_an_event_where_its_receiver_s_machine_takes_it() {
        let door = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\n@enduml\n";
        let specs = Specs::new(vec![
            Spec::read(door, Language::PlantUml, "door.puml").unwrap(),
        ]);
        let code = "@startuml\nactor User\nparticipant \"Front door\" as Door\nUser -> Door : open(now)\nUser -> Door : knock\nUser <- Door : open\nalt a-b\nDoor --> User : open\nend\n";
        use Tone::*;
        assert_eq!(
            read(code, Language::PlantUml, &specs),
            [
                of(1, "User", Name),
                of(2, "Door", Name),
                of(3, "User", Name),
                of(3, "Door", Name),
                of(3, "open", Event),
                of(4, "User", Name),
                of(4, "Door", Name),
                of(5, "User", Name),
                of(5, "Door", Name),
                of(7, "Door", Name),
                of(7, "User", Name),
            ]
        );
        let mermaid = "sequenceDiagram\n  participant D as Door\n  User->>+D: open\n";
        assert_eq!(
            read(mermaid, Language::Mermaid, &specs),
            [
                of(1, "D", Name),
                of(2, "User", Name),
                of(2, "D", Name),
                of(2, "open", Event),
            ]
        );
    }

    #[test]
    fn other_diagrams_and_lines_half_written_get_no_wrong_color() {
        let none = Specs::default();
        assert_eq!(
            read("flowchart TD\n  A --> B\n", Language::Mermaid, &none),
            []
        );
        let half = "@startuml\n[*] --> Idle\nIdle -->\nIdle --> \"Not clo\nIdle x y\nstate\n";
        assert_eq!(
            read(half, Language::PlantUml, &none),
            [of(1, "Idle", Tone::Name)]
        );
        // Letters of more than one byte keep their places.
        let wide = "@startuml\n[*] --> Tür\nTür --> Öffnet : öffnen [frei]\n";
        assert_eq!(
            read(wide, Language::PlantUml, &none)[3..],
            [of(2, "öffnen", Tone::Event), of(2, "[frei]", Tone::Guard)]
        );
    }
}
