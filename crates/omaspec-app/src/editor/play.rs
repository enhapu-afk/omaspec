//! Playing the diagram on screen: its state machine, its scenario, or a
//! recorded trace from the project folder.

use std::path::{Path, PathBuf};
use std::time::Instant;

use iced::Task;
use iced::keyboard::{Key, Modifiers, key::Named};

use super::picker::{Choice, Item, Kind, Picker};
use super::{App, Command, FLASH, Message, PICKER, Pane};
use crate::worker::{Alarm, blocking};
use crate::{paper, specs};

/// The play's keys: a digit takes that option, Space, Right or Enter the
/// next step, Left or Backspace the one before, Home starts again, and
/// Escape stops it. Without Ctrl, Alt or Super, but for Escape.
pub(super) fn key(key: &Key, mods: Modifiers) -> Option<Command> {
    let plain = !mods.control() && !mods.alt() && !mods.logo();
    let ask = |ask: paper::Ask| Some(Command::PlayAsk(ask));
    match key.as_ref() {
        Key::Named(Named::Escape) => Some(Command::StopPlay),
        _ if !plain => None,
        Key::Character(c) if matches!(c, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9") => {
            ask(paper::Ask::Take(c.parse::<usize>().unwrap_or(1) - 1))
        }
        Key::Named(Named::Space | Named::ArrowRight | Named::Enter) => ask(paper::Ask::Next),
        Key::Named(Named::ArrowLeft | Named::Backspace) => ask(paper::Ask::Back),
        Key::Named(Named::Home) => ask(paper::Ask::Restart),
        _ => None,
    }
}

/// The keys of the play that runs, a recorded trace or not, for the status
/// bar.
pub(super) fn keys(tracing: bool) -> &'static str {
    if tracing {
        "playing a trace: Space next line, ← back, Esc stops"
    } else {
        "playing: 1–9 a step, Space next, ← back, Esc stops"
    }
}

impl App {
    /// Plays the active diagram, its state machine or its scenario, with
    /// the keyboard on the diagram; or stops the play.
    pub(super) fn toggle_play(&mut self) -> Task<Message> {
        let key = self.doc().key;
        if self.doc().paper.playing() {
            return self
                .doc_mut()
                .paper
                .update(paper::Message::StopPlay)
                .map(move |m| Message::Paper(key, m));
        }
        // A play shows the whole machine: the folds open first, and it
        // starts once that picture is in.
        if self.doc_mut().paper.unfold_all() {
            self.doc_mut().paper.take_refold();
            self.play_when_drawn = Some(key);
            self.notice = Some("unfolding to play the whole machine".into());
            return self.render(key, true);
        }
        match self.doc_mut().paper.start_play() {
            Ok(()) => {
                self.set_focus(Pane::Diagram);
                // The status bar shows the play's keys while it runs.
                self.notice = None;
                self.apply_keyboard_focus()
            }
            Err(why) => {
                self.notice = Some(why);
                Task::none()
            }
        }
    }

    /// Looks for recorded traces of the state machine on screen in its
    /// project folder, in the background.
    pub(super) fn open_traces(&mut self) -> Task<Message> {
        let doc = self.doc();
        let key = doc.key;
        let Some(spec) = doc.paper.spec(&doc.name()) else {
            self.notice = Some("a trace plays on a state machine: show one first".into());
            return Task::none();
        };
        let Some(root) = self.project_of(doc) else {
            self.notice = Some("traces are found in the diagram's folder: save it first".into());
            return Task::none();
        };
        Task::perform(
            blocking(move || specs::traces(&root, &spec)),
            move |found| Message::Traces(key, found.unwrap_or_default()),
        )
    }

    /// Draws what the recorded traces in the project folder take of the
    /// state machine on screen, or stops drawing it.
    pub(super) fn toggle_coverage(&mut self) -> Task<Message> {
        let key = self.doc().key;
        if self.doc().paper.covered.is_some() {
            self.doc_mut().paper.covered = None;
            self.notice = Some("coverage hidden".into());
            return self.render(key, true);
        }
        let doc = self.doc();
        let Some(spec) = doc.paper.spec(&doc.name()) else {
            self.notice = Some("coverage is drawn over a state machine: show one first".into());
            return Task::none();
        };
        let Some(root) = self.project_of(doc) else {
            self.notice = Some("traces are found in the diagram's folder: save it first".into());
            return Task::none();
        };
        Task::perform(
            blocking(move || {
                let mut paths: Vec<PathBuf> = specs::traces(&root, &spec)
                    .into_iter()
                    .map(|(_, path, _)| path)
                    .collect();
                paths.dedup();
                paths
                    .iter()
                    .filter_map(|path| std::fs::read_to_string(path).ok())
                    .collect::<Vec<String>>()
            }),
            move |traces| Message::Covered(key, traces.unwrap_or_default()),
        )
    }

    /// Draws the coverage of `traces` over the state machine of tab `key`.
    pub(super) fn covered(&mut self, key: u64, traces: Vec<String>) -> Task<Message> {
        let Some(doc) = self.docs.iter_mut().find(|doc| doc.key == key) else {
            return Task::none();
        };
        if traces.is_empty() {
            self.notice = Some(
                "no trace (JSON Lines, *.jsonl) in the diagram's folder has an object of its state machine"
                    .into(),
            );
            return Task::none();
        }
        let name = doc.name();
        let said = doc.paper.spec(&name).map(|spec| {
            let texts: Vec<&str> = traces.iter().map(String::as_str).collect();
            let (taken, total) = omaspec_model::coverage(&spec, &texts).transitions_covered();
            format!(
                "{taken} of {total} transitions taken ({:.0} %) by {} {}; green how often, dashed red never",
                omaspec_model::coverage_share(taken, total),
                traces.len(),
                if traces.len() == 1 { "trace" } else { "traces" }
            )
        });
        doc.paper.covered = Some(std::sync::Arc::new(crate::worker::Covered { name, traces }));
        self.notice = said;
        self.render(key, true)
    }

    /// Plays the one trace found, or offers them.
    pub(super) fn found_traces(
        &mut self,
        key: u64,
        mut found: Vec<(String, PathBuf, String)>,
    ) -> Task<Message> {
        if key != self.doc().key {
            return Task::none();
        }
        match found.len() {
            0 => {
                self.notice = Some(
                    "no trace (JSON Lines, *.jsonl) in the diagram's folder has an object of its state machine"
                        .into(),
                );
                Task::none()
            }
            1 => {
                let (_, path, object) = found.remove(0);
                self.play_trace(&path, &object)
            }
            _ => {
                let items = found
                    .into_iter()
                    .map(|(label, path, object)| Item {
                        label,
                        hint: String::new(),
                        choice: Choice::Trace { path, object },
                    })
                    .collect();
                self.place_picker(Picker::new(Kind::Traces, "play a trace", items));
                iced::widget::operation::focus(PICKER)
            }
        }
    }

    /// Plays the trace of `object` in `path` on the state machine on screen.
    pub(super) fn play_trace(&mut self, path: &Path, object: &str) -> Task<Message> {
        let trace = match crate::files::read_input(Some(path)) {
            Ok(input) => input.text,
            Err(err) => {
                self.notice = Some(format!("{err:#}"));
                return Task::none();
            }
        };
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let file = self.doc().name();
        match self
            .doc_mut()
            .paper
            .start_trace(&trace, &name, object, &file)
        {
            Ok(()) => {
                self.set_focus(Pane::Diagram);
                self.notice = None;
                self.apply_keyboard_focus()
            }
            Err(why) => {
                self.notice = Some(why);
                Task::none()
            }
        }
    }

    /// Lights the source line of the step whose light set off last,
    /// without taking the keyboard off the diagram. The caret stays where
    /// the writer is typing, and in a tab out of sight: the line is let go.
    pub(super) fn light_played_line(&mut self, key: u64) {
        let typing = self.focus == Pane::Source && self.detached.is_none();
        let shown = key == self.doc().key && !typing;
        let Some(doc) = self.find(key) else {
            return;
        };
        if let Some(line) = doc.paper.take_jump(Instant::now())
            && shown
        {
            doc.jump_to_line(line);
            self.after(Alarm::Flash(key), FLASH);
        }
    }
}
