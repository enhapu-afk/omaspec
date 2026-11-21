//! *Export as › Animation…*: a container of the diagram folding to its
//! box and unfolding again, as a picture that moves.
//!
//! The save dialog asks for the file and, beside its name, for the
//! container; the file's ending says what is written (`.webp`, `.png` an
//! animated PNG, `.gif`, or `.html` a page in vectors). Where no dialog
//! answers, the bottom line asks, Tab taking the next container. The
//! picture is drawn as `omaspec play --fold` draws it ([`super::moving`]),
//! on a thread of its own, from the text in the tab, saved or not.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use iced::Task;

use super::films::Film;
use super::prompt::Asking;
use super::{App, Message};
use crate::file_dialog::{self, Answer};

/// Held while an animation is drawn: the window's end waits for it
/// ([`finish`]), and a second one for the first.
static DRAWING: Mutex<()> = Mutex::new(());

/// Waits for an animation that is being drawn. The program's end would
/// take the GPU from under its thread, and the file would not be written.
pub(super) fn finish() {
    drop(DRAWING.lock().unwrap_or_else(PoisonError::into_inner));
}

/// The id of the save dialog's choice of the container.
pub(super) const FOLD: &str = "fold";
/// What an animation is written as when its name does not say.
const ENDING: &str = "webp";

impl App {
    /// The containers the active tab's diagram can fold, or why none.
    pub(super) fn foldable(&self) -> Result<Vec<String>, String> {
        let paper = &self.doc().paper;
        if paper.playing() {
            return Err("stop the play first (Esc): a fold is of the diagram at rest".into());
        }
        let containers = paper.containers();
        if containers.is_empty() {
            return Err("nothing folds here: the diagram has no container".into());
        }
        Ok(containers)
    }

    /// Asks where the animation goes and which container folds in it.
    pub(super) fn export_animation(&mut self) -> Task<Message> {
        let containers = match self.foldable() {
            Ok(containers) => containers,
            Err(why) => {
                self.notice = Some(why);
                return Task::none();
            }
        };
        if !self.dialogs {
            return self.ask(Asking::Animation);
        }
        let key = self.doc().key;
        let asked = self.animation_asked(&containers);
        self.close_picker();
        Task::perform(file_dialog::save(asked), move |answer| {
            Message::AnimationAnswered(key, answer)
        })
    }

    /// Where the animation of the active tab is suggested to go:
    /// `coffee-fold.webp` beside `coffee.puml`.
    pub(super) fn suggested_animation(&self) -> PathBuf {
        let doc = self.doc();
        let folder = self
            .folder
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let name = doc.name();
        let stem = name
            .rsplit_once('.')
            .map_or(name.as_str(), |(stem, _)| stem);
        let file = format!("{stem}-fold.{ENDING}");
        match &doc.path {
            Some(path) => path.with_file_name(file),
            None => folder.join(file),
        }
    }

    /// What the save dialog asks for the animation.
    pub(super) fn animation_asked(&self, containers: &[String]) -> file_dialog::Asked {
        let suggested = self.suggested_animation();
        file_dialog::Asked {
            title: "Export a fold as an animation".into(),
            accept: "Export".into(),
            folder: suggested
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            name: suggested
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            kind: (
                "Animation (WebP, animated PNG, GIF, or a page)".into(),
                String::new(),
                "*.webp *.png *.apng *.gif *.html".into(),
            ),
            choices: vec![file_dialog::Choice {
                id: FOLD.into(),
                label: "Fold".into(),
                options: containers
                    .iter()
                    .map(|name| (name.clone(), name.clone()))
                    .collect(),
                initial: containers.first().cloned().unwrap_or_default(),
            }],
        }
    }

    /// What the save dialog answered for the animation of tab `key`.
    pub(super) fn animation_answered(&mut self, key: u64, answer: Answer) -> Task<Message> {
        if !self.docs.iter().any(|doc| doc.key == key) {
            return Task::none();
        }
        match &answer {
            Answer::Cancelled => Task::none(),
            Answer::Unavailable(why) => {
                if self.doc().key != key {
                    self.notice = Some(format!("no save dialog: {why}"));
                    return Task::none();
                }
                let task = self.ask(Asking::Animation);
                self.notice = Some(format!("no save dialog ({why}): name the file here"));
                task
            }
            Answer::Chosen { path, .. } => {
                let fold = answer.choice(FOLD).map(str::to_owned);
                self.write_animation(key, path.clone(), fold)
            }
        }
    }

    /// The bottom line's container is `name`, or the next one.
    pub(super) fn next_fold(&mut self, name: Option<String>) {
        let Ok(containers) = self.foldable() else {
            return;
        };
        if let Some(prompt) = &mut self.prompt {
            prompt.fold = name.filter(|n| containers.contains(n)).unwrap_or_else(|| {
                let at = containers
                    .iter()
                    .position(|c| *c == prompt.fold)
                    .unwrap_or(0);
                containers[(at + 1) % containers.len()].clone()
            });
        }
    }

    /// Writes the animation the bottom line names. A file that is there
    /// asks once.
    pub(super) fn animation_submitted(&mut self) -> Task<Message> {
        let Some(prompt) = &self.prompt else {
            return Task::none();
        };
        let (key, input, overwrite, fold) = (
            prompt.doc,
            prompt.input.clone(),
            prompt.overwrite,
            prompt.fold.clone(),
        );
        let Some(typed) = super::prompt::expand(input.trim()) else {
            self.notice = Some("name a file to export to".into());
            return Task::none();
        };
        if typed.is_dir() {
            self.notice = Some(format!("{} is a folder", typed.display()));
            return Task::none();
        }
        let path = film_path(typed);
        let Some(index) = self.docs.iter().position(|doc| doc.key == key) else {
            self.prompt = None;
            return Task::none();
        };
        // Asked again, the line still open, as an export's is.
        if let Err(why) = self.export_target(index, &path) {
            self.notice = Some(why);
            return Task::none();
        }
        if path.exists() && !overwrite {
            if let Some(prompt) = &mut self.prompt {
                prompt.overwrite = true;
                prompt.input = path.display().to_string();
            }
            self.notice = Some(format!(
                "{} exists: Enter again overwrites it",
                path.display()
            ));
            return Task::none();
        }
        self.prompt = None;
        self.write_animation(key, path, Some(fold))
    }

    /// Draws the container `fold` of tab `key` folding and unfolding (the
    /// first where none is named) and writes it to `path`, in the
    /// background; [`Message::Exported`] says how it went.
    fn write_animation(&mut self, key: u64, path: PathBuf, fold: Option<String>) -> Task<Message> {
        let Some(index) = self.docs.iter().position(|doc| doc.key == key) else {
            return Task::none();
        };
        let path = film_path(path);
        if let Err(why) = self.export_target(index, &path) {
            self.notice = Some(why);
            return Task::none();
        }
        let doc = &self.docs[index];
        let containers = doc.paper.containers();
        let Some(fold) = fold
            .filter(|name| containers.contains(name))
            .or_else(|| containers.first().cloned())
        else {
            self.notice = Some("nothing folds here: the diagram has no container".into());
            return Task::none();
        };
        let moving = super::moving::Moving {
            file: doc
                .path
                .clone()
                .unwrap_or_else(|| PathBuf::from(doc.name())),
            text: Some(doc.text()),
            output: path.clone(),
            events: Vec::new(),
            steps: 0,
            width: None,
            fps: None,
            folds: vec![fold.clone()],
            dives: Vec::new(),
            builds: false,
            lockstep: false,
            history: false,
            travel: None,
            traffic: Vec::new(),
            themes: None,
            ink: false,
            look: None,
            timing: Default::default(),
            whole: false,
            theme: None,
            file_colors: !self.follow_theme,
            root: self.place_of(key).map(|place| place.root),
            forced: Some(doc.language),
            renderer: None,
            quality: super::gifs::Quality::default(),
        };
        self.notice = Some(format!("drawing {fold} folding and unfolding…"));
        // The thread starts when the task runs, not when it is made.
        Task::perform(
            async move {
                crate::worker::blocking(move || {
                    let _drawing = DRAWING.lock().unwrap_or_else(PoisonError::into_inner);
                    super::moving::play(&moving)
                        .map(|_| path.display().to_string())
                        .map_err(|err| format!("{err:#}"))
                })
                .await
            },
            |result| Message::Exported(result.unwrap_or_else(|| Err("the export failed".into()))),
        )
    }
}

/// `typed` as the file of a picture that moves: with `.webp` where its
/// ending names none.
pub(super) fn film_path(typed: PathBuf) -> PathBuf {
    if Film::of(&typed).is_some() {
        return typed;
    }
    let mut name = typed.into_os_string();
    name.push(".");
    name.push(ENDING);
    PathBuf::from(name)
}
