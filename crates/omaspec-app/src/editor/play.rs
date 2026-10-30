//! Playing the diagram on screen: its state machine, its scenario, or a
//! recorded trace from the project folder.

use std::path::{Path, PathBuf};
use std::time::Instant;

use iced::Task;

use super::picker::{Choice, Item, Kind, Picker};
use super::{App, FLASH, Message, PICKER, Pane};
use crate::worker::{Alarm, blocking};
use crate::{paper, specs};

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
