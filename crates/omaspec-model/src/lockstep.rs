//! A scenario played beside the state machines of its participants: what
//! each message does to the machine of the participant that receives it,
//! as a way through that machine a pane plays like a recorded trace
//! ([`Followed`]).
//!
//! It is the scenario's own check ([`crate::across`]) played out: a
//! participant may start anywhere, the first message it receives narrows
//! where it can be, a message its machine ignores leaves it where it was,
//! and the branches of a group each start where the group began. So what
//! the play shows deviating is what the check reports, on the same line,
//! in the same words. A deviation is shown as an error, as a trace's is:
//! the machine cannot follow the scenario there.

use omaspec_core::Language;
use omaspec_core::lint::{Finding, Severity};

use crate::across::{self, Moved, Spec};
use crate::{Followed, Kind, Machine, Run, Specs, Taken, TraceStep};

/// A scenario and the machines of its participants, played together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lockstep {
    /// The participants with a state machine of their own, in the order
    /// the scenario draws them.
    pub players: Vec<Playing>,
}

/// A participant with a state machine, and its way through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playing {
    /// Its place among the scenario's participants, and its name as drawn.
    pub participant: usize,
    pub name: String,
    pub spec: Spec,
    /// A step on the line of each message it receives, and one on the line
    /// of a group that sends it back to where the group began (an `else`)
    /// or on to where its branches lead (its `end`). Where it could be
    /// before it receives anything is where the first message can be
    /// taken: the scenario may start anywhere.
    pub followed: Followed,
}

/// The participants of the scenario in `code` with a state machine in
/// `specs`, and their ways through them; `None` when `code` is no
/// scenario or none of its participants has a machine.
pub fn lockstep(code: &str, lang: Language, specs: &Specs) -> Option<Lockstep> {
    let (who, moves) = across::moves(code, lang, specs)?;
    let players = who
        .into_iter()
        .map(
            |across::Cast {
                 participant,
                 name,
                 spec,
             }| {
                let mine: Vec<&Moved> = moves
                    .iter()
                    .filter(|m| m.participant == participant)
                    .collect();
                Playing {
                    participant,
                    name,
                    followed: followed(&spec.machine, &mine),
                    spec: spec.clone(),
                }
            },
        )
        .collect();
    Some(Lockstep { players })
}

/// The way through `m` that `moves` make.
fn followed(m: &Machine, moves: &[&Moved]) -> Followed {
    let mut out = Followed::default();
    for (index, moved) in moves.iter().enumerate() {
        if moved.creates {
            let (taken, at) = match Run::start(m) {
                Some((run, taken)) => (taken, run.active().to_vec()),
                None => (Vec::new(), resting(m, moved.after.as_deref())),
            };
            out.steps.push(step(moved, taken, at));
            continue;
        }
        if moved.event.is_none() {
            out.steps
                .push(step(moved, Vec::new(), resting(m, moved.after.as_deref())));
            continue;
        }
        // Where it could be is open: the states that take the event are
        // where the scenario says it is.
        let from = match &moved.before {
            Some(before) => resting(m, Some(before)),
            None => {
                let takers = takers(m, &moved.fired, None);
                if index == 0 {
                    out.begins = takers.clone();
                } else if !takers.is_empty() {
                    out.steps.push(TraceStep {
                        line: moved.line,
                        time: None,
                        event: None,
                        state: None,
                        taken: Vec::new(),
                        at: takers.clone(),
                        finding: None,
                    });
                }
                takers
            }
        };
        // A deviation takes nothing; a message the machine ignores finds
        // nothing to leave.
        let taken = match moved.finding {
            Some(_) => Vec::new(),
            None => fired(m, &moved.fired, &from),
        };
        let at = match &moved.after {
            Some(after) => resting(m, Some(after)),
            None => from,
        };
        out.steps.push(step(moved, taken, at));
    }
    out
}

fn step(moved: &Moved, taken: Vec<Taken>, at: Vec<usize>) -> TraceStep {
    TraceStep {
        line: moved.line,
        time: None,
        event: moved.event.clone(),
        state: None,
        taken,
        at,
        finding: moved.finding.clone().map(|f| Finding {
            severity: Severity::Error,
            ..f
        }),
    }
}

/// The states `can_be` marks that a pane lights: states and ends, not
/// the starts, choices and bars passed through; none while it is open.
fn resting(m: &Machine, can_be: Option<&[bool]>) -> Vec<usize> {
    let Some(can_be) = can_be else {
        return Vec::new();
    };
    (0..m.states.len())
        .filter(|&s| can_be[s] && matches!(m.states[s].kind, Kind::State | Kind::End))
        .collect()
}

/// Of the states in `among` (every state where that is `None`), those
/// that one of the transitions `fired` leaves, itself or from a state
/// around it.
fn takers(m: &Machine, fired: &[usize], among: Option<&[usize]>) -> Vec<usize> {
    let all: Vec<usize> = (0..m.states.len())
        .filter(|&s| matches!(m.states[s].kind, Kind::State) && !has_children(m, s))
        .collect();
    among
        .unwrap_or(&all)
        .iter()
        .copied()
        .filter(|&s| fired.iter().any(|&t| leaves(m, t, s)))
        .collect()
}

/// The transitions `fired` takes from where it is (`from`): each from
/// the states it leaves to those it enters, all at once.
fn fired(m: &Machine, fired: &[usize], from: &[usize]) -> Vec<Taken> {
    let mut out: Vec<Taken> = Vec::new();
    for &t in fired {
        let left: Vec<usize> = from.iter().copied().filter(|&s| leaves(m, t, s)).collect();
        if left.is_empty() {
            continue;
        }
        let mut at = vec![false; m.states.len()];
        let mut seen = vec![false; m.states.len()];
        across::enter(m, m.transitions[t].to, &mut at, &mut seen);
        out.push(Taken {
            transition: t,
            left,
            entered: resting(m, Some(&at)),
            beat: 0,
        });
    }
    out
}

/// Whether transition `t` leaves state `s`: from it, or from a state
/// around it.
fn leaves(m: &Machine, t: usize, s: usize) -> bool {
    let from = m.transitions[t].from;
    from == s || m.ancestors(s).any(|a| a == from)
}

fn has_children(m: &Machine, s: usize) -> bool {
    m.states.iter().any(|c| c.parent == Some(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs() -> Specs {
        Specs::new(vec![
            Spec::read(
                "@startuml Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n",
                Language::PlantUml,
                "door.puml",
            )
            .unwrap(),
        ])
    }

    fn names(m: &Machine, states: &[usize]) -> Vec<String> {
        states.iter().map(|&s| m.states[s].name.clone()).collect()
    }

    #[test]
    fn each_message_moves_the_machine_of_its_receiver() {
        let code = "@startuml\nGuest -> Door : open\nGuest -> Door : close\nGuest -> Door : lock\n@enduml\n";
        let step = lockstep(code, Language::PlantUml, &specs()).unwrap();
        let [door] = step.players.as_slice() else {
            panic!("{step:?}");
        };
        let m = &door.spec.machine;
        assert_eq!(door.name, "Door");
        // Before the first message it is where `open` can be taken.
        assert_eq!(names(m, &door.followed.begins), ["Closed"]);
        let ways: Vec<(usize, Vec<String>, usize)> = door
            .followed
            .steps
            .iter()
            .map(|s| (s.line, names(m, &s.at), s.taken.len()))
            .collect();
        assert_eq!(
            ways,
            [
                (1, vec!["Open".to_owned()], 1),
                (2, vec!["Closed".to_owned()], 1),
                (3, vec!["Locked".to_owned()], 1),
            ]
        );
        assert!(door.followed.steps.iter().all(|s| s.finding.is_none()));
    }

    #[test]
    fn a_message_the_machine_cannot_take_there_deviates_as_the_check_says() {
        let code = "@startuml\nGuest -> Door : open\nGuest -> Door : lock\n@enduml\n";
        let step = lockstep(code, Language::PlantUml, &specs()).unwrap();
        let door = &step.players[0];
        let wrong = &door.followed.steps[1];
        assert!(wrong.taken.is_empty());
        assert_eq!(names(&door.spec.machine, &wrong.at), ["Open"]);
        let finding = wrong.finding.as_ref().unwrap();
        assert_eq!(finding.severity, Severity::Error);
        // The check's own words, on its line.
        let checked = across::check(code, Language::PlantUml, &specs());
        assert_eq!(checked.len(), 1);
        assert_eq!(checked[0].line, finding.line);
        assert_eq!(checked[0].message, finding.message);
    }

    #[test]
    fn a_branch_starts_where_its_group_began() {
        let code = "@startuml\nGuest -> Door : open\nalt in a hurry\nGuest -> Door : close\nelse\nGuest -> Door : close\nend\n@enduml\n";
        let step = lockstep(code, Language::PlantUml, &specs()).unwrap();
        let door = &step.players[0];
        let m = &door.spec.machine;
        let ways: Vec<(usize, Vec<String>)> = door
            .followed
            .steps
            .iter()
            .map(|s| (s.line, names(m, &s.at)))
            .collect();
        assert_eq!(
            ways,
            [
                (1, vec!["Open".to_owned()]),
                (3, vec!["Closed".to_owned()]),
                // `else`: back to where `alt` began.
                (4, vec!["Open".to_owned()]),
                (5, vec!["Closed".to_owned()]),
            ]
        );
    }

    #[test]
    fn a_scenario_without_a_machine_plays_nothing_beside() {
        assert!(
            lockstep(
                "@startuml\nA -> B : hi\n@enduml\n",
                Language::PlantUml,
                &specs()
            )
            .is_none()
        );
        assert!(
            lockstep(
                "@startuml\n[*] --> A\n@enduml\n",
                Language::PlantUml,
                &specs()
            )
            .is_none()
        );
    }
}
