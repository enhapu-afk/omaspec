//! Diagrams checked against each other: what a sequence diagram sends a
//! participant, held against that participant's state machine in another
//! diagram of the project.
//!
//! A participant has a state machine when a state diagram is named like it
//! (by the name after `@startuml`, its title, or its file name). Each
//! message the participant receives is an event for its machine:
//!
//! - an event the machine has no transition for is reported (a reply,
//!   drawn dashed, may carry a value instead and is left alone);
//! - played in order, a message that no state the participant can be in by
//!   then takes is reported too. The scenario may start anywhere: the
//!   first message narrows where the machine can be. `alt`, `opt`, `loop`
//!   and `break` are followed branch by branch; `par` and `ref` leave the
//!   participant's state open. A message the machine ignores leaves it
//!   where it was, as in UML.
//!
//! Guards are not evaluated: a guarded transition may always fire. All of
//! this errs towards a machine that can take more, so a finding is sure.

use std::collections::HashMap;

use omaspec_core::Language;
use omaspec_core::lang::strip_mermaid_fence;
use omaspec_core::lint::{Finding, Severity};
use omaspec_uml::Diagram;
use omaspec_uml::sequence::{self, End, GroupKind, Item, Mark, Message};

use crate::{Kind, Machine};

/// A state machine of the project, with the names a participant may call
/// it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    /// The name after `@startuml`, the title, the file name without its
    /// extension: whichever there are.
    pub names: Vec<String>,
    /// The file, as messages show it: its path in the project.
    pub file: String,
    pub machine: Machine,
}

impl Spec {
    /// The state machine in `code`, from the file shown as `file`; `None`
    /// if `code` is no state diagram the readers understand.
    pub fn read(code: &str, lang: Language, file: &str) -> Option<Spec> {
        let machine = Machine::read(code, lang)?;
        let mut names = Vec::new();
        names.extend(diagram_name(code, lang));
        names.extend(title(code, lang));
        let stem = file.rsplit('/').next().unwrap_or(file);
        let stem = stem.split('.').next().unwrap_or(stem);
        names.push(stem.to_owned());
        names.retain(|n| !key(n).is_empty());
        Some(Spec {
            names,
            file: file.to_owned(),
            machine,
        })
    }
}

/// `@startuml Name`.
pub(crate) fn diagram_name(code: &str, lang: Language) -> Option<String> {
    if lang != Language::PlantUml {
        return None;
    }
    let line = code
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("@startuml"))?;
    let name = line["@startuml".len()..].trim();
    (!name.is_empty() && !name.starts_with('(')).then(|| name.trim_matches('"').to_owned())
}

/// `title Name` in PlantUML, `title: Name` in a Mermaid front matter.
pub(crate) fn title(code: &str, lang: Language) -> Option<String> {
    let mut front = false;
    for line in code.lines().map(str::trim) {
        match lang {
            Language::PlantUml => {
                let lower = line.to_ascii_lowercase();
                if let Some(rest) = lower.strip_prefix("title ") {
                    return Some(line[line.len() - rest.len()..].trim().to_owned());
                }
            }
            Language::Mermaid => {
                if line == "---" {
                    if front {
                        return None;
                    }
                    front = true;
                } else if front && let Some(rest) = line.strip_prefix("title:") {
                    return Some(rest.trim().trim_matches('"').to_owned());
                }
            }
        }
    }
    None
}

/// How names and events compare: letters and digits only, in lower case,
/// so that `Door control`, `door_control` and `DoorControl` are one name.
pub(crate) fn key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The event a message label or a trigger names: what comes before its
/// parameters, as written.
pub(crate) fn event_of(label: &str) -> &str {
    let label = label.split("\\n").next().unwrap_or(label);
    label.split('(').next().unwrap_or(label).trim()
}

/// The findings of `code` against the project's state machines.
pub fn check(code: &str, lang: Language, specs: &[Spec]) -> Vec<Finding> {
    if specs.is_empty() {
        return Vec::new();
    }
    let Some((diagram, offset)) = read_sequence(code, lang) else {
        return Vec::new();
    };
    let mut players: HashMap<usize, Player> = HashMap::new();
    for (index, participant) in diagram.participants.iter().enumerate() {
        let wanted = [key(&participant.code), key(&participant.label)];
        let mut found = specs
            .iter()
            .filter(|s| s.names.iter().any(|n| wanted.contains(&key(n))));
        // Two machines with the participant's name: which is meant is
        // unclear, so neither is used.
        if let (Some(spec), None) = (found.next(), found.next()) {
            players.insert(index, Player::new(spec, &participant.label));
        }
    }
    if players.is_empty() {
        return Vec::new();
    }
    let mut play = Play {
        players,
        frames: Vec::new(),
        out: Vec::new(),
        offset,
    };
    for item in &diagram.items {
        play.item(item);
    }
    play.out
}

/// The sequence diagram `code` holds, and how many lines come before what
/// was read (a Markdown fence around Mermaid).
pub(crate) fn read_sequence(code: &str, lang: Language) -> Option<(sequence::Diagram, usize)> {
    match lang {
        Language::PlantUml => match omaspec_uml::parse(code) {
            Ok(Diagram::Sequence(d)) => Some((d, 0)),
            _ => None,
        },
        Language::Mermaid => {
            let body = strip_mermaid_fence(code);
            let offset = code
                .get(..(body.as_ptr() as usize).saturating_sub(code.as_ptr() as usize))
                .map_or(0, |before| before.matches('\n').count());
            sequence::from_mermaid(body).map(|d| (d, offset))
        }
    }
}

/// A participant with a state machine, played through the scenario.
struct Player<'a> {
    spec: &'a Spec,
    name: String,
    /// The states it can be in by now; `None` before anything is known.
    can_be: Option<Vec<bool>>,
    /// Event key → the transitions it fires.
    events: HashMap<String, Vec<usize>>,
    /// Event key → the event as the machine writes it.
    written: HashMap<String, String>,
}

impl<'a> Player<'a> {
    fn new(spec: &'a Spec, name: &str) -> Self {
        let mut events: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, t) in spec.machine.transitions.iter().enumerate() {
            for event in &t.trigger.events {
                let k = key(event_of(event));
                if !k.is_empty() {
                    events.entry(k).or_default().push(index);
                }
            }
        }
        Player {
            spec,
            name: name.to_owned(),
            can_be: None,
            events,
            written: written(&spec.machine),
        }
    }
}

/// The state of every player before a group, and how each branch left it.
struct Frame {
    kind: GroupKind,
    before: HashMap<usize, Option<Vec<bool>>>,
    branches: Vec<HashMap<usize, Option<Vec<bool>>>>,
}

struct Play<'a> {
    players: HashMap<usize, Player<'a>>,
    frames: Vec<Frame>,
    out: Vec<Finding>,
    offset: usize,
}

impl Play<'_> {
    fn snapshot(&self) -> HashMap<usize, Option<Vec<bool>>> {
        self.players
            .iter()
            .map(|(&p, player)| (p, player.can_be.clone()))
            .collect()
    }

    fn restore(&mut self, snapshot: &HashMap<usize, Option<Vec<bool>>>) {
        for (p, can_be) in snapshot {
            if let Some(player) = self.players.get_mut(p) {
                player.can_be = can_be.clone();
            }
        }
    }

    /// Whether messages are played in order here: not inside a `par`,
    /// whose branches interleave.
    fn ordered(&self) -> bool {
        !self.frames.iter().any(|f| f.kind == GroupKind::Par)
    }

    fn item(&mut self, item: &Item) {
        match item {
            Item::Message(m) => self.message(m),
            Item::GroupStart { kind, .. } => {
                let before = self.snapshot();
                self.frames.push(Frame {
                    kind: *kind,
                    before,
                    branches: Vec::new(),
                });
            }
            Item::GroupElse { .. } => {
                let now = self.snapshot();
                if let Some(frame) = self.frames.last_mut() {
                    frame.branches.push(now);
                    let before = frame.before.clone();
                    self.restore(&before);
                }
            }
            Item::GroupEnd { .. } => {
                let now = self.snapshot();
                if let Some(mut frame) = self.frames.pop() {
                    frame.branches.push(now);
                    self.join(&frame);
                }
            }
            Item::Ref { over, .. } => {
                for p in over {
                    if let Some(player) = self.players.get_mut(p) {
                        player.can_be = None;
                    }
                }
            }
            _ => {}
        }
    }

    /// Where every player can be after a group.
    fn join(&mut self, frame: &Frame) {
        for (&p, player) in &mut self.players {
            let before = frame.before.get(&p).cloned().flatten();
            let ends: Vec<Option<Vec<bool>>> = frame
                .branches
                .iter()
                .map(|b| b.get(&p).cloned().flatten())
                .collect();
            player.can_be = match frame.kind {
                GroupKind::Alt | GroupKind::Critical | GroupKind::Group => union(ends),
                GroupKind::Opt | GroupKind::Break => {
                    union(ends.into_iter().chain([before]).collect())
                }
                // Once round leads back to where it began, or else no telling.
                GroupKind::Loop => {
                    let round = union(ends);
                    match (&before, &round) {
                        (Some(b), Some(r)) if r.iter().zip(b).all(|(r, b)| !r || *b) => {
                            Some(b.clone())
                        }
                        _ => None,
                    }
                }
                // Interleaved branches: open unless no branch moved it.
                GroupKind::Par => {
                    if ends.iter().all(|e| *e == before) {
                        before
                    } else {
                        None
                    }
                }
            };
        }
    }

    fn message(&mut self, m: &Message) {
        let End::Participant(to) = m.to else {
            return;
        };
        if m.arrow.end_mark == Mark::Cross {
            return;
        }
        let ordered = self.ordered();
        let line = m.line + self.offset;
        if let Some(player) = self.players.get_mut(&to)
            && let Some(message) = receive(player, m, ordered)
        {
            self.out.push(Finding {
                line,
                severity: Severity::Warning,
                message,
            });
        }
    }
}

/// Plays message `m` to `player`; what to report, if anything. `ordered`:
/// the messages before it are known to come before it.
fn receive(player: &mut Player, m: &Message, ordered: bool) -> Option<String> {
    let machine = &player.spec.machine;
    if m.creates {
        player.can_be = Some(started(machine));
        return None;
    }
    let label = m.text.as_ref()?.value.as_str();
    let event = event_of(label);
    let k = key(event);
    if k.is_empty() {
        return None;
    }
    let own = m.from == m.to;
    let Some(fired) = player.events.get(&k) else {
        // A reply may carry a value, a message to oneself an action.
        return (!m.arrow.dashed && !own).then(|| {
            format!(
                "{} has no transition for {event} in its state machine ({}). {}",
                player.name,
                player.spec.file,
                events_list(&player.written)
            )
        });
    };
    let can_be = player
        .can_be
        .clone()
        .unwrap_or_else(|| vec![true; machine.states.len()]);
    let takers: Vec<usize> = (0..machine.states.len())
        .filter(|&s| can_be[s] && takes(machine, s, fired))
        .collect();
    if takers.is_empty() {
        // Ignored, as UML has it: the machine stays where it was.
        return (ordered && !own && player.can_be.is_some()).then(|| {
            format!(
                "{} does not take {event} here: by now it is in {}, and {} ({}).",
                player.name,
                states_list(machine, &can_be),
                no_transition(machine, &can_be, event),
                player.spec.file
            )
        });
    }
    let mut next = vec![false; machine.states.len()];
    for &s in &takers {
        for &t in fired {
            let tr = &machine.transitions[t];
            if tr.from == s || machine.ancestors(s).any(|a| a == tr.from) {
                let mut seen = vec![false; machine.states.len()];
                enter(machine, tr.to, &mut next, &mut seen);
            }
        }
    }
    // States in other regions than the one the event moved stay.
    for s in 0..machine.states.len() {
        if can_be[s] && takers.iter().any(|&t| orthogonal(machine, s, t)) {
            next[s] = true;
        }
    }
    player.can_be = Some(next);
    None
}

/// The union of where each branch left a player; open if one left it open.
fn union(ends: Vec<Option<Vec<bool>>>) -> Option<Vec<bool>> {
    let mut out: Option<Vec<bool>> = None;
    for end in ends {
        let end = end?;
        out = Some(match out {
            Some(acc) => acc.iter().zip(&end).map(|(a, b)| *a || *b).collect(),
            None => end,
        });
    }
    out
}

/// Whether state `s` (or a state around it) has one of the transitions
/// `fired`.
fn takes(m: &Machine, s: usize, fired: &[usize]) -> bool {
    fired.iter().any(|&t| {
        let from = m.transitions[t].from;
        from == s || m.ancestors(s).any(|a| a == from)
    })
}

/// Where the machine is once started: the top start entered.
fn started(m: &Machine) -> Vec<bool> {
    let mut at = vec![false; m.states.len()];
    let mut seen = vec![false; m.states.len()];
    for (i, s) in m.states.iter().enumerate() {
        if s.kind == Kind::Start && s.parent.is_none() {
            enter(m, i, &mut at, &mut seen);
        }
    }
    at
}

/// Marks where the machine can rest after entering `state`: a state
/// itself, the starts of a composite state's regions, every branch of a
/// choice or fork, and on through transitions without an event.
fn enter(m: &Machine, state: usize, at: &mut [bool], seen: &mut [bool]) {
    if std::mem::replace(&mut seen[state], true) {
        return;
    }
    let follow = |from: usize, at: &mut [bool], seen: &mut [bool]| {
        for t in &m.transitions {
            if t.from == from && t.trigger.events.is_empty() {
                enter(m, t.to, at, seen);
            }
        }
    };
    let s = &m.states[state];
    match s.kind {
        Kind::Start | Kind::Choice | Kind::Fork | Kind::Join => follow(state, at, seen),
        // Back to wherever inside the state around it: one of its states,
        // or for a deep history any state at any depth.
        Kind::History | Kind::DeepHistory => {
            if let Some(parent) = s.parent {
                let deep = s.kind == Kind::DeepHistory;
                for (i, c) in m.states.iter().enumerate() {
                    if c.parent == Some(parent) || (deep && m.ancestors(i).any(|a| a == parent)) {
                        enter(m, i, at, seen);
                    }
                }
            }
        }
        // Done here; the state around it may go on without an event.
        Kind::End => {
            at[state] = true;
            if let Some(parent) = s.parent {
                follow(parent, at, seen);
            }
        }
        Kind::State => {
            let children: Vec<usize> = (0..m.states.len())
                .filter(|&c| m.states[c].parent == Some(state))
                .collect();
            if children.is_empty() {
                at[state] = true;
                follow(state, at, seen);
                return;
            }
            let regions = children
                .iter()
                .map(|&c| m.states[c].region)
                .max()
                .unwrap_or(0);
            for region in 0..=regions {
                let members: Vec<usize> = children
                    .iter()
                    .copied()
                    .filter(|&c| m.states[c].region == region)
                    .collect();
                let starts: Vec<usize> = members
                    .iter()
                    .copied()
                    .filter(|&c| m.states[c].kind == Kind::Start)
                    .collect();
                for c in if starts.is_empty() { members } else { starts } {
                    enter(m, c, at, seen);
                }
            }
        }
    }
}

/// Whether `a` and `b` lie in different regions of one composite state,
/// so both can be active at once.
pub(crate) fn orthogonal(m: &Machine, a: usize, b: usize) -> bool {
    let regions = |x: usize| {
        let mut out = Vec::new();
        let mut cur = x;
        while let Some(p) = m.states[cur].parent {
            out.push((p, m.states[cur].region));
            cur = p;
        }
        out
    };
    let ra = regions(a);
    regions(b)
        .iter()
        .any(|(p, r)| ra.iter().any(|(q, s)| p == q && r != s))
}

/// The events `m` takes: event key → the event as the machine writes it.
pub(crate) fn written(m: &Machine) -> HashMap<String, String> {
    let mut written = HashMap::new();
    for t in &m.transitions {
        for event in &t.trigger.events {
            let event = event_of(event);
            let k = key(event);
            if !k.is_empty() {
                written.entry(k).or_insert_with(|| event.to_owned());
            }
        }
    }
    written
}

/// The machine's events (from [`written`]), for a message that names none
/// of them.
pub(crate) fn events_list(written: &HashMap<String, String>) -> String {
    let mut names: Vec<&str> = written.values().map(String::as_str).collect();
    names.sort_by_key(|n| n.to_lowercase());
    if names.is_empty() {
        return "Its transitions name no events.".to_owned();
    }
    let shown = names.len().min(8);
    let mut list = names[..shown].join(", ");
    if names.len() > shown {
        list.push_str(&format!(" and {} more", names.len() - shown));
    }
    format!("It takes {list}.")
}

/// The states a machine can be in, by name.
pub(crate) fn states_list(m: &Machine, can_be: &[bool]) -> String {
    let mut names: Vec<String> = (0..m.states.len())
        .filter(|&s| can_be[s])
        .map(|s| match m.states[s].kind {
            Kind::End => match m.states[s].parent {
                Some(p) => format!("the end of {}", m.states[p].name),
                None => "its end".to_owned(),
            },
            _ => m.states[s].name.clone(),
        })
        .filter(|n| !n.is_empty())
        .collect();
    names.dedup();
    match names.len() {
        0 => "none of its states".to_owned(),
        1 => names.remove(0),
        n if n <= 4 => {
            let last = names.pop().unwrap_or_default();
            format!("{} or {last}", names.join(", "))
        }
        n => format!("{} or {} other states", names[..3].join(", "), n - 3),
    }
}

/// Says that none of those states takes the event.
pub(crate) fn no_transition(m: &Machine, can_be: &[bool], event: &str) -> String {
    if can_be.iter().filter(|b| **b).count() == 1 {
        let only = (0..m.states.len()).find(|&s| can_be[s]);
        if only.is_some_and(|s| m.states[s].kind == Kind::End) {
            return format!("nothing takes {event} after the end");
        }
        format!("it has no transition for {event}")
    } else {
        format!("none of them has a transition for {event}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOOR: &str = "@startuml\n[*] --> Closed\nClosed --> Open : open [unlocked]\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";

    fn door() -> Vec<Spec> {
        vec![Spec::read(DOOR, Language::PlantUml, "door.puml").unwrap()]
    }

    fn check_puml(body: &str) -> Vec<(usize, String)> {
        let code = format!("@startuml\n{body}\n@enduml\n");
        check(&code, Language::PlantUml, &door())
            .into_iter()
            .map(|f| (f.line + 1, f.message))
            .collect()
    }

    #[test]
    fn a_spec_is_named_by_start_title_and_file() {
        let spec = Spec::read(
            "@startuml DoorMachine\ntitle Door control\n[*] --> A\n@enduml\n",
            Language::PlantUml,
            "x/door.puml",
        )
        .unwrap();
        assert_eq!(spec.names, ["DoorMachine", "Door control", "door"]);
        assert_eq!(spec.file, "x/door.puml");
        let mmd = Spec::read(
            "---\ntitle: Door control\n---\nstateDiagram-v2\n[*] --> A\n",
            Language::Mermaid,
            "door.mmd",
        )
        .unwrap();
        assert_eq!(mmd.names, ["Door control", "door"]);
        assert!(Spec::read("@startuml\nA -> B\n@enduml\n", Language::PlantUml, "s.puml").is_none());
    }

    #[test]
    fn an_unknown_event_is_reported_with_the_known_ones() {
        let found = check_puml("User -> Door : open()\nUser -> Door : kick\nDoor --> User : ok");
        assert_eq!(
            found,
            [(
                3,
                "Door has no transition for kick in its state machine (door.puml). It takes close, lock, open and unlock."
                    .replace(" and unlock", ", unlock")
            )]
        );
    }

    #[test]
    fn messages_play_in_order() {
        // From Locked, open is not taken; after unlock it is.
        let found = check_puml(
            "participant Door\nUser -> Door : lock\nUser -> Door : open\nUser -> Door : unlock\nUser -> Door : open",
        );
        assert_eq!(
            found,
            [(
                4,
                "Door does not take open here: by now it is in Locked, and it has no transition for open (door.puml)."
                    .to_owned()
            )]
        );
        // The names match without case, spaces or underscores; the first
        // message may come in any state.
        assert_eq!(
            check_puml("participant DOOR\nX -> DOOR : unlock\nX -> DOOR : open"),
            []
        );
    }

    #[test]
    fn branches_join() {
        // Either branch may have locked the door: open after the alt is
        // fine in one of them, so nothing is sure.
        assert_eq!(
            check_puml(
                "X -> Door : close\nalt a\n  X -> Door : lock\nelse b\n  X -> Door : open\nend\nX -> Door : close"
            ),
            []
        );
        // In both branches it ends locked.
        let found = check_puml(
            "X -> Door : close\nalt a\n  X -> Door : lock\nelse b\n  X -> Door : lock\nend\nX -> Door : close",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, 8);
        // An opt may be skipped; a par leaves the door open to anything.
        assert_eq!(
            check_puml("X -> Door : lock\nopt a\n  X -> Door : unlock\nend\nX -> Door : unlock"),
            []
        );
        assert_eq!(
            check_puml("X -> Door : lock\npar\n  X -> Door : unlock\nend\nX -> Door : unlock"),
            []
        );
    }

    #[test]
    fn mermaid_sequences_are_checked_too() {
        let code = "```mermaid\nsequenceDiagram\n  participant Door\n  User->>Door: lock\n  User->>Door: open\n```\n";
        let found = check(code, Language::Mermaid, &door());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line, 4);
    }

    #[test]
    fn regions_move_on_their_own() {
        let spec = Spec::read(
            "@startuml\n[*] --> Both\nstate Both {\n  [*] --> A1\n  A1 --> A2 : a\n  --\n  [*] --> B1\n  B1 --> B2 : b\n}\n@enduml\n",
            Language::PlantUml,
            "both.puml",
        )
        .unwrap();
        let code = "@startuml\nX -> Both : a\nX -> Both : b\n@enduml\n";
        assert_eq!(check(code, Language::PlantUml, &[spec]), []);
    }
}
