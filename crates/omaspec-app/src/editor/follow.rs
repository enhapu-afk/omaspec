//! A trace followed as it is written, in the editor (*Follow a trace…*,
//! docs/adr/0036): the trace's file chosen in the desktop's dialog (in the
//! bottom line where there is none), watched beside the diagrams, and
//! played on the state machine of its tab as the preview's `--follow`
//! plays it ([`crate::paper::Paper::start_live`]). A change to the diagram
//! starts it again; stopping the play ends it.

use std::path::{Path, PathBuf};

use iced::Task;

use super::prompt::Asking;
use super::{App, Message, Pane};
use crate::file_dialog::{self, Answer};

impl App {
    /// Asks which trace to follow on the state machine of the active tab.
    pub(super) fn follow_trace_asking(&mut self) -> Task<Message> {
        let key = self.doc().key;
        if !self.dialogs {
            return self.ask(Asking::FollowTrace(key));
        }
        let folder = self
            .doc()
            .path
            .as_deref()
            .and_then(Path::parent)
            .map_or_else(|| self.start_folder(), Path::to_path_buf);
        let asked = file_dialog::Opening {
            title: "Follow a trace as it is written".into(),
            accept: "Follow".into(),
            folder,
            kinds: vec![("Traces (JSON Lines)".into(), vec!["*.jsonl".into()])],
            directory: false,
        };
        self.close_picker();
        Task::perform(file_dialog::open(asked), move |answer| {
            Message::FollowAnswered(key, answer)
        })
    }

    /// Follows what the dialog chose; the bottom line asks where no dialog
    /// came.
    pub(super) fn follow_answered(&mut self, key: u64, answer: Answer) -> Task<Message> {
        match answer {
            Answer::Cancelled => Task::none(),
            Answer::Unavailable(why) => {
                let task = self.ask(Asking::FollowTrace(key));
                self.notice = Some(format!("no open dialog ({why}): name the trace here"));
                task
            }
            Answer::Chosen { path, .. } => self.follow_path(key, path),
        }
    }

    /// Follows the trace the bottom line names on tab `key`, the one it
    /// was asked for.
    pub(super) fn follow_submitted(&mut self, key: u64) -> Task<Message> {
        let Some(path) = self
            .prompt
            .as_ref()
            .and_then(|p| super::prompt::expand(p.input.trim()))
        else {
            self.notice = Some("name the trace to follow".into());
            return Task::none();
        };
        self.prompt = None;
        self.follow_path(key, path)
    }

    /// Follows the trace in `path` on the state machine of tab `key`.
    pub(super) fn follow_path(&mut self, key: u64, path: PathBuf) -> Task<Message> {
        let path = std::path::absolute(&path).unwrap_or(path);
        let Some(doc) = self.docs.iter_mut().find(|doc| doc.key == key) else {
            return Task::none();
        };
        let name = doc.name();
        match doc.paper.start_live(path.clone(), &name) {
            Ok(()) => {
                self.traces.insert(key, path);
                self.notice = None;
                self.set_focus(Pane::Diagram);
                self.apply_keyboard_focus()
            }
            Err(why) => {
                self.notice = Some(format!("not following the trace: {why}"));
                Task::none()
            }
        }
    }

    /// Moves the plays that follow the file `path`; whether one does. A
    /// tab whose play ended since follows it no more.
    pub(super) fn follow_changed(&mut self, path: &Path) -> bool {
        let keys: Vec<u64> = self
            .traces
            .iter()
            .filter(|(_, trace)| same(trace, path))
            .map(|(&key, _)| key)
            .collect();
        let mut moved = false;
        for key in keys {
            match self.docs.iter_mut().find(|doc| doc.key == key) {
                Some(doc) if doc.paper.following().is_some() => {
                    doc.paper.follow_more();
                    moved = true;
                }
                _ => {
                    self.traces.remove(&key);
                }
            }
        }
        moved
    }

    /// Starts following again on tab `key` once it is drawn anew, where
    /// this drawing, a change to the machine, ended the play (`followed`:
    /// it followed before). A play that ended otherwise (stopped, a
    /// presentation, another play) ended following too.
    pub(super) fn follow_again(&mut self, key: u64, followed: bool) {
        let Some(path) = self.traces.get(&key).cloned() else {
            return;
        };
        let Some(doc) = self.docs.iter_mut().find(|doc| doc.key == key) else {
            self.traces.remove(&key);
            return;
        };
        if doc.paper.following().is_some() {
            return;
        }
        if !followed || doc.paper.playing() {
            self.traces.remove(&key);
            return;
        }
        let name = doc.name();
        if let Err(why) = doc.paper.start_live(path, &name) {
            self.notice = Some(format!("not following the trace: {why}"));
            self.traces.remove(&key);
        }
    }

    /// Ends following on tab `key`, where its play stops.
    pub(super) fn stop_following(&mut self, key: u64) {
        self.traces.remove(&key);
    }

    /// The trace files followed, to watch beside the diagrams.
    pub(super) fn followed_files(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.traces.values().cloned()
    }
}

/// Whether `a` and `b` name one file, as the watcher may name it otherwise.
fn same(a: &Path, b: &Path) -> bool {
    a == b
        || std::fs::canonicalize(a)
            .ok()
            .zip(std::fs::canonicalize(b).ok())
            .is_some_and(|(a, b)| a == b)
}
