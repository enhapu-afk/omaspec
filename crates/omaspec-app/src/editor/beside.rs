//! The lockstep in the window: a scenario played with the state machines
//! of its participants beside it. F5 on a sequence diagram whose
//! participants have machines in its project opens each machine in a pane
//! beside the scenario's (docs/adr/0018 for how a name finds its machine).
//! As the scenario's play takes a message, the receiver's machine takes
//! its step when the message's light reaches it: a play is drawn from
//! the time alone, so the step is taken now for the moment the light
//! arrives. Going back and starting again take the machines along.
//!
//! The ways are the scenario's own check played out
//! ([`omaspec_model::lockstep`]), so a machine flashes where the check
//! reports, as in `omaspec play --lockstep` ([`super::lockstep`]).

use std::path::PathBuf;

use iced::Task;
use omaspec_core::Language;
use omaspec_model::Followed;

use super::{App, Message};
use crate::paper::{self, Ask, Offer, Paper};

/// The first render ticket's document key of a machine beside a
/// scenario; one for each, up to [`MOST`]. Below the compare pane's.
pub const FIRST: u64 = u64::MAX - 1 - MOST as u64;
/// The machines beside a scenario at most.
pub const MOST: usize = 16;

/// The machines beside the scenario that plays.
pub struct Beside {
    /// The scenario's tab.
    pub scenario: u64,
    pub panes: Vec<Pane>,
    /// For each message played, how many steps each machine took for it.
    taken: Vec<Vec<usize>>,
}

/// A participant's machine beside the scenario.
pub struct Pane {
    pub key: u64,
    /// The participant, as the scenario draws it.
    pub name: String,
    pub paper: Paper,
    text: String,
    language: Language,
    path: PathBuf,
    machine: omaspec_model::Machine,
    followed: Followed,
    /// Its steps taken, and whether its play has started: once its
    /// picture is in.
    played: usize,
    started: bool,
}

impl Pane {
    pub fn text(&self) -> String {
        self.text.clone()
    }

    pub fn language(&self) -> Language {
        self.language
    }

    /// Starts its play once its picture is in, and catches up with the
    /// steps the scenario has taken meanwhile.
    fn start(&mut self, scenario: &str) {
        if self.started {
            return;
        }
        let started = self.paper.start_followed(
            self.machine.clone(),
            self.followed.clone(),
            scenario,
            &self.name,
        );
        self.started = started.is_ok();
        let now = self.paper.play_now();
        for _ in 0..self.played {
            self.paper.play_ask_at(Ask::Next, now);
        }
    }
}

impl App {
    /// The machines of the participants of the scenario in tab `key`,
    /// beside its play: each read from its file in the project, or from
    /// its tab where one holds it, and drawn. None where no participant
    /// has a machine.
    pub(super) fn open_beside(&mut self, key: u64) -> Task<Message> {
        self.beside = None;
        let Some(doc) = self.docs.iter().find(|d| d.key == key) else {
            return Task::none();
        };
        let (text, language) = (doc.text(), doc.language);
        let Some(root) = self.project_of(doc) else {
            return Task::none();
        };
        let Some(step) = omaspec_model::lockstep(&text, language, &doc.specs) else {
            return Task::none();
        };
        let panes: Vec<Pane> = step
            .players
            .into_iter()
            .take(MOST)
            .enumerate()
            .filter_map(|(index, player)| {
                let path = root.join(&player.spec.file);
                let text = match self.docs.iter().find(|d| d.path.as_ref() == Some(&path)) {
                    Some(tab) => tab.text(),
                    None => std::fs::read_to_string(&path).ok()?,
                };
                let name = path.file_name()?.to_string_lossy().into_owned();
                let language = omaspec_core::detect(&text, Language::from_file_name(&name));
                Some(Pane {
                    key: FIRST + index as u64,
                    name: player.name,
                    paper: Paper::default(),
                    text,
                    language,
                    path,
                    machine: player.spec.machine,
                    followed: player.followed,
                    played: 0,
                    started: false,
                })
            })
            .collect();
        if panes.is_empty() {
            return Task::none();
        }
        let wrong: Vec<usize> = panes
            .iter()
            .flat_map(|p| &p.followed.steps)
            .filter(|s| s.finding.is_some())
            .map(|s| s.line)
            .collect();
        if let Some(doc) = self.find(key) {
            doc.paper.mark_wrong(wrong);
        }
        let asks: Vec<(u64, String, Language)> = panes
            .iter()
            .map(|p| (p.key, p.text.clone(), p.language))
            .collect();
        self.beside = Some(Beside {
            scenario: key,
            panes,
            taken: Vec::new(),
        });
        for (key, text, language) in asks {
            self.dispatch(key, &text, language);
        }
        Task::none()
    }

    /// Whether `key` is a machine's pane beside the scenario.
    pub(super) fn is_beside(key: u64) -> bool {
        (FIRST..FIRST + MOST as u64).contains(&key)
    }

    /// The pane of the machine `key`, if it is beside the scenario.
    pub(super) fn beside_pane(&mut self, key: u64) -> Option<&mut Pane> {
        self.beside
            .as_mut()?
            .panes
            .iter_mut()
            .find(|p| p.key == key)
    }

    /// Where the machine `key` lives, for its `!include`s.
    pub(super) fn beside_place(&self, key: u64) -> Option<omaspec_render::Place> {
        let pane = self.beside.as_ref()?.panes.iter().find(|p| p.key == key)?;
        omaspec_render::Place::of(&pane.path, self.folder.as_deref())
    }

    /// The machine `key`'s picture is in: its play starts.
    pub(super) fn beside_rendered(
        &mut self,
        key: u64,
        answer: crate::worker::Answer,
    ) -> Task<Message> {
        let scenario = self.beside_scenario();
        let Some(pane) = self.beside_pane(key) else {
            return Task::none();
        };
        let task = pane
            .paper
            .rendered(answer)
            .map(move |m| Message::Paper(key, m));
        pane.start(&scenario);
        task
    }

    /// A message to the pane of the machine `key` beside the scenario.
    pub(super) fn beside_paper(&mut self, key: u64, message: paper::Message) -> Task<Message> {
        // The machines step with the scenario, not by themselves.
        if matches!(message, paper::Message::Play(_) | paper::Message::StopPlay) {
            return Task::none();
        }
        let scenario = self.beside_scenario();
        match self.beside_pane(key) {
            Some(pane) => {
                let task = pane
                    .paper
                    .update(message)
                    .map(move |m| Message::Paper(key, m));
                // Its play starts as soon as its picture is in.
                pane.start(&scenario);
                task
            }
            None => Task::none(),
        }
    }

    /// The scenario's name, as a machine's play says what it follows.
    fn beside_scenario(&self) -> String {
        self.beside
            .as_ref()
            .and_then(|b| self.docs.iter().find(|d| d.key == b.scenario))
            .map(|d| d.name())
            .unwrap_or_default()
    }

    /// The scenario in tab `key` took `ask`: each machine beside it takes
    /// the steps of the messages that moved it, the receiver's as the
    /// light arrives.
    pub(super) fn beside_follow(&mut self, key: u64, ask: Ask) {
        let Some(doc) = self.docs.iter().find(|d| d.key == key) else {
            return;
        };
        let Some(beside) = self.beside.as_mut().filter(|b| b.scenario == key) else {
            return;
        };
        let scenario = &doc.paper;
        let now = scenario.play_now();
        match ask {
            Ask::Next => {
                let Some(Offer::Scenario { at, .. }) = scenario.offer() else {
                    return;
                };
                let lines = scenario.message_lines();
                let Some(&line) = at.checked_sub(1).and_then(|i| lines.get(i)) else {
                    return;
                };
                if beside.taken.len() >= at {
                    return;
                }
                let arrives = scenario.play_arrives().unwrap_or(now);
                let mut counts = Vec::new();
                for pane in &mut beside.panes {
                    let mut count = 0;
                    while let Some(next) = pane.followed.steps.get(pane.played)
                        && next.line <= line
                    {
                        // A group's step before the message now; the
                        // message's own as its light arrives.
                        let at = if next.line < line { now } else { arrives };
                        pane.played += 1;
                        count += 1;
                        if pane.started {
                            pane.paper.play_ask_at(Ask::Next, at);
                        }
                    }
                    counts.push(count);
                }
                beside.taken.push(counts);
            }
            Ask::Back => {
                let Some(counts) = beside.taken.pop() else {
                    return;
                };
                for (pane, count) in beside.panes.iter_mut().zip(counts) {
                    for _ in 0..count {
                        pane.played = pane.played.saturating_sub(1);
                        if pane.started {
                            pane.paper.play_ask_at(Ask::Back, now);
                        }
                    }
                }
            }
            Ask::Restart => {
                beside.taken.clear();
                for pane in &mut beside.panes {
                    pane.played = 0;
                    if pane.started {
                        pane.paper.play_ask_at(Ask::Restart, now);
                    }
                }
            }
            Ask::Take(_) | Ask::Hover(_) => {}
        }
    }
}

/// Machines beside a scenario that plays, to draw.
pub(super) fn shown(beside: Option<&Beside>, key: u64) -> Option<&Beside> {
    beside.filter(|b| b.scenario == key && !b.panes.is_empty())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::Command;
    use super::*;
    use crate::paper::testing::{answer, drawn};

    fn run(app: &mut App, task: Task<Message>) {
        crate::at_once::here(task, |message| app.update(message));
    }

    /// Hands `text` to the pane `key` as its drawing.
    fn deliver(app: &mut App, key: u64, text: &str) {
        let id = match app.beside_pane(key) {
            Some(pane) => pane.paper.pretend_next(),
            None => app.find(key).unwrap().paper.pretend_next(),
        };
        let mut reply = answer(key, id, Ok(drawn(text, Language::PlantUml)));
        reply.language = Language::PlantUml;
        reply.source = text.into();
        let task = app.update(Message::Worker(crate::worker::Event::Rendered(Box::new(
            reply,
        ))));
        run(app, task);
    }

    fn press(app: &mut App, command: Command) {
        let task = app.update(Message::Command(command));
        run(app, task);
    }

    /// F5 on a scenario opens the machine of its participant beside it;
    /// each message steps it, back and again take it along, and the play's
    /// end closes it.
    #[test]
    fn a_scenario_plays_with_its_machines_beside_it() {
        let dir = std::env::temp_dir().join(format!("omaspec-beside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("omaspec-project.toml"), "name = \"home\"\n").unwrap();
        let door = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
        std::fs::write(dir.join("door.puml"), door).unwrap();
        let keys = "@startuml\nactor Guest\nGuest -> Door : lock\nGuest -> Door : open\n@enduml\n";
        let file = dir.join("keys.puml");
        std::fs::write(&file, keys).unwrap();
        let start = super::super::Start {
            files: vec![file],
            ..super::super::Start::default()
        };
        let mut app = App::new(start, None).0;
        let (specs, atlas) = crate::specs::read_project(&dir);
        let task = app.update(Message::Specs(
            dir.clone(),
            Arc::new(specs),
            Arc::new(atlas),
        ));
        run(&mut app, task);
        let key = app.doc().key;
        deliver(&mut app, key, keys);
        press(&mut app, Command::Play);
        let panes = app.beside.as_ref().map_or(0, |b| b.panes.len());
        assert_eq!(panes, 1, "{:?}", app.notice);
        deliver(&mut app, FIRST, door);
        let trace = |app: &mut App| match app.beside_pane(FIRST).and_then(|p| p.paper.offer()) {
            Some(Offer::Trace {
                played, finding, ..
            }) => (played, finding.map(|(s, _)| s)),
            other => panic!("{other:?}"),
        };
        assert_eq!(trace(&mut app), (0, None));
        press(&mut app, Command::PlayAsk(Ask::Next));
        assert_eq!(trace(&mut app), (1, None));
        // The door is locked: it cannot be opened.
        press(&mut app, Command::PlayAsk(Ask::Next));
        assert_eq!(
            trace(&mut app),
            (2, Some(omaspec_core::lint::Severity::Error))
        );
        press(&mut app, Command::PlayAsk(Ask::Back));
        assert_eq!(trace(&mut app), (1, None));
        press(&mut app, Command::PlayAsk(Ask::Restart));
        assert_eq!(trace(&mut app), (0, None));
        press(&mut app, Command::Play);
        assert!(app.beside.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
