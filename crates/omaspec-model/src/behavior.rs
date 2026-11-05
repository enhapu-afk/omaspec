//! A diagram's behavior as text, one fact a line, for git's text
//! conversion (`omaspec git textconv`): `git diff` then shows a state, a
//! transition or a message that came or went, not lines that moved. A
//! state machine's states and transitions are sorted, as their order is
//! layout; a scenario's messages keep theirs, as it is behavior. The
//! machine checks' findings follow, without the lines they name. The same
//! machine reads the same in PlantUML and in Mermaid.

use omaspec_core::lint::Severity;
use omaspec_core::{Kind as DiagramKind, Language, classify, project};
use omaspec_uml::sequence::{self, End, Item, Side};

use crate::diff::without_lines;
use crate::{Kind, Machine, Trigger, across, check};

/// The behavior of the state machine or sequence diagram `code`; `None`
/// for another kind of diagram or one Omaspec does not fully read.
pub fn behavior(code: &str, lang: Language) -> Option<String> {
    let name = project::names(code, lang, "").into_iter().next();
    let named = |kind: &str| match &name {
        Some(name) => format!("{kind} {name}"),
        None => kind.to_owned(),
    };
    let mut lines = match classify(code, lang).kind {
        DiagramKind::State => {
            let machine = Machine::read(code, lang)?;
            let mut lines = vec![named("State machine")];
            lines.extend(machine_lines(&machine));
            lines
        }
        DiagramKind::Sequence => {
            let (diagram, _) = across::read_sequence(code, lang)?;
            let mut lines = vec![named("Sequence diagram")];
            lines.extend(scenario_lines(&diagram));
            lines
        }
        _ => return None,
    };
    lines.push(String::new());
    Some(lines.join("\n"))
}

/// The states, the transitions and the findings, each sorted.
fn machine_lines(m: &Machine) -> Vec<String> {
    let mut states: Vec<String> = (0..m.states.len())
        .filter_map(|s| {
            let kind = match m.states[s].kind {
                Kind::State => "state",
                Kind::Choice => "choice",
                Kind::Fork => "fork",
                Kind::Join => "join",
                _ => return None,
            };
            Some(format!("  {kind} {}", path(m, s)))
        })
        .collect();
    states.sort();
    let mut transitions: Vec<String> = m
        .transitions
        .iter()
        .map(|t| {
            format!(
                "  {} -> {}{}",
                path(m, t.from),
                path(m, t.to),
                trigger(&t.trigger)
            )
        })
        .collect();
    transitions.sort();
    let mut findings: Vec<String> = check::machine(m)
        .into_iter()
        .map(|f| format!("  {}: {}", severity(f.severity), without_lines(&f.message)))
        .collect();
    findings.sort();
    states
        .into_iter()
        .chain(transitions)
        .chain(findings)
        .collect()
}

/// A state by the states around it: `On/region 2/Speaker` where `On`
/// has more than one region, a start or an end as `[*]`, a history as
/// `[H]` or `[H*]`.
fn path(m: &Machine, s: usize) -> String {
    let own = |s: usize| {
        let state = &m.states[s];
        match state.kind {
            Kind::Start | Kind::End => "[*]".to_owned(),
            Kind::History => "[H]".to_owned(),
            Kind::DeepHistory => "[H*]".to_owned(),
            _ => state.name.clone(),
        }
    };
    let mut parts = vec![own(s)];
    let mut at = s;
    loop {
        let parent = m.states[at].parent;
        let split = m
            .states
            .iter()
            .any(|other| other.parent == parent && other.region > 0);
        if split {
            parts.push(format!("region {}", m.states[at].region + 1));
        }
        match parent {
            Some(p) => {
                parts.push(own(p));
                at = p;
            }
            None => break,
        }
    }
    parts.reverse();
    parts.join("/")
}

/// ` on a, b [guard] / action`; nothing for a transition taken when its
/// state is done.
fn trigger(t: &Trigger) -> String {
    let mut events = t.events.clone();
    events.sort();
    let mut out = String::new();
    if !events.is_empty() {
        out.push_str(&format!(" on {}", events.join(", ")));
    }
    if let Some(guard) = &t.guard {
        out.push_str(&format!(" [{guard}]"));
    }
    if let Some(action) = &t.action {
        out.push_str(&format!(" / {action}"));
    }
    out
}

fn severity(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

/// The participants, sorted, then the messages and the blocks around them
/// in their order.
fn scenario_lines(d: &sequence::Diagram) -> Vec<String> {
    let code = |i: usize| d.participants[i].code.clone();
    let end = |e: End| match e {
        End::Participant(i) => code(i),
        End::Edge(Side::Left) | End::Short(Side::Left) => "[".into(),
        End::Edge(Side::Right) | End::Short(Side::Right) => "]".into(),
    };
    let text = |t: Option<&sequence::Text>| t.map(|t| plain(&t.value)).unwrap_or_default();
    let mut participants: Vec<String> = (0..d.participants.len())
        .map(|i| format!("  participant {}", code(i)))
        .collect();
    participants.sort();
    let mut lines = participants;
    let mut depth = 1;
    let pad = |depth: usize| "  ".repeat(depth);
    for item in &d.items {
        match item {
            Item::Message(m) => {
                let arrow = if m.arrow.dashed { "-->" } else { "->" };
                let said = text(m.text.as_ref());
                let said = if said.is_empty() {
                    String::new()
                } else {
                    format!(" : {said}")
                };
                lines.push(format!(
                    "{}{} {arrow} {}{said}",
                    pad(depth),
                    end(m.from),
                    end(m.to)
                ));
            }
            Item::GroupStart { kind, label, .. } => {
                lines.push(
                    format!("{}{} {}", pad(depth), kind.keyword(), text(label.as_ref()))
                        .trim_end()
                        .to_owned(),
                );
                depth += 1;
            }
            Item::GroupElse { label, .. } => {
                let at = pad(depth.saturating_sub(1).max(1));
                lines.push(
                    format!("{at}else {}", text(label.as_ref()))
                        .trim_end()
                        .to_owned(),
                );
            }
            Item::GroupEnd { .. } => {
                depth = depth.saturating_sub(1).max(1);
                lines.push(format!("{}end", pad(depth)));
            }
            Item::Ref {
                over, text: said, ..
            } => {
                let over: Vec<String> = over.iter().map(|&i| code(i)).collect();
                lines.push(format!(
                    "{}ref over {} : {}",
                    pad(depth),
                    over.join(", "),
                    plain(&said.value)
                ));
            }
            Item::Destroy { participant, .. } => {
                lines.push(format!("{}destroy {}", pad(depth), code(*participant)));
            }
            _ => {}
        }
    }
    lines
}

/// A label as text: line breaks as spaces, runs of spaces as one.
fn plain(text: &str) -> String {
    text.replace("\\n", " ")
        .replace("<br/>", " ")
        .replace("<br>", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_reads_as_its_sorted_facts() {
        let door = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock, bolt [key] / click\nstate Locked\n@enduml\n";
        assert_eq!(
            behavior(door, Language::PlantUml).unwrap(),
            "State machine Door\n  state Closed\n  state Locked\n  state Open\n  Closed -> Locked on bolt, lock [key] / click\n  Closed -> Open on open\n  Open -> Closed on close\n  [*] -> Closed\n  warning: Nothing leaves Locked. If the machine ends there, add Locked --> [*].\n"
        );
        // Moved lines change nothing; the same machine in Mermaid reads the same.
        let moved = "@startuml\ntitle Door\nstate Locked\nOpen --> Closed : close\n[*] --> Closed\nClosed --> Locked : lock, bolt [key] / click\nClosed --> Open : open\n@enduml\n";
        assert_eq!(
            behavior(moved, Language::PlantUml),
            behavior(door, Language::PlantUml)
        );
        let mermaid = "---\ntitle: Door\n---\nstateDiagram-v2\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock, bolt [key] / click\n";
        assert_eq!(
            behavior(mermaid, Language::Mermaid),
            behavior(door, Language::PlantUml)
        );
    }

    #[test]
    fn nested_states_and_regions_are_named_by_the_states_around_them() {
        let on = "@startuml\n[*] --> On\nstate On {\n  [*] --> Stopped\n  Stopped --> Playing : play\n  --\n  [*] --> Speaker\n}\nOn --> [*] : off\n@enduml\n";
        let text = behavior(on, Language::PlantUml).unwrap();
        assert!(text.contains("\n  state On/region 1/Stopped\n"), "{text}");
        assert!(text.contains("\n  state On/region 2/Speaker\n"), "{text}");
        assert!(
            text.contains("\n  On/region 1/[*] -> On/region 1/Stopped\n"),
            "{text}"
        );
        assert!(text.contains("\n  On -> [*] on off\n"), "{text}");
    }

    #[test]
    fn a_scenario_keeps_its_order_and_its_blocks() {
        let visit = "@startuml\ntitle Visit\nparticipant User\nparticipant Door\nUser -> Door : knock\nalt open\n  Door --> User : come in\nelse locked\n  User -> Door : unlock\nend\n@enduml\n";
        assert_eq!(
            behavior(visit, Language::PlantUml).unwrap(),
            "Sequence diagram Visit\n  participant Door\n  participant User\n  User -> Door : knock\n  alt open\n    Door --> User : come in\n  else locked\n    User -> Door : unlock\n  end\n"
        );
    }

    #[test]
    fn other_diagrams_have_no_behavior_to_tell() {
        let flow = "@startuml\nstart\n:fill;\nstop\n@enduml\n";
        assert_eq!(behavior(flow, Language::PlantUml), None);
    }
}
