//! Opening, saving, exporting and closing documents, the prompt that asks
//! for a name, and changes on disk.

use std::path::Path;

use anyhow::Context;
use iced::{Task, window};
use omaspec_core::embed;
use omaspec_render::export::Format;

use super::compare::Target;
use super::doc::Doc;
use super::{
    App, Asking, MAX_IMAGE, Message, PROMPT, Prompt, expand, export_path, free_name, is_image,
    plural, suggested_export,
};

impl App {
    /// Back to the saved file; undo brings the changes back.
    pub(super) fn revert(&mut self) -> Task<Message> {
        let doc = self.doc();
        let Some(path) = doc.path.clone().filter(|_| doc.on_disk()) else {
            self.notice = Some("nothing saved to go back to".into());
            return Task::none();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let doc = self.doc_mut();
                doc.reload(&text);
                doc.analyze();
                let key = doc.key;
                self.notice = Some(format!("back to {} (Ctrl+Z undoes)", path.display()));
                self.keep_session();
                self.render(key, false)
            }
            Err(err) => {
                self.notice = Some(format!("{}: {err}", path.display()));
                Task::none()
            }
        }
    }

    /// Asks where to write the diagram on screen. SVG and PNG carry the
    /// source; the ZIP holds the SVG, the source and a PDF. The source file
    /// itself is never the destination.
    pub(super) fn export(&mut self, format: Format) -> Task<Message> {
        self.ask(Asking::Export(format))
    }

    /// Closes every tab but this one, except those with unsaved changes.
    pub(super) fn close_others(&mut self) -> Task<Message> {
        let key = self.doc().key;
        let before = self.docs.len();
        self.docs.retain(|d| d.key == key || d.modified);
        let kept = self.docs.len() - 1;
        self.active = self.docs.iter().position(|d| d.key == key).unwrap_or(0);
        self.notice = Some(match (before - self.docs.len(), kept) {
            (closed, 0) => format!("closed {closed} {}", plural(closed, "tab")),
            (closed, kept) => format!(
                "closed {closed} {}; {kept} with unsaved changes stay",
                plural(closed, "tab")
            ),
        });
        self.keep_session();
        self.select(self.active)
    }

    /// Opens `path` in a new tab, or finds the tab it is in; an untouched
    /// new tab gives way. Returns the tab's index.
    pub(super) fn open_doc(&mut self, path: &Path) -> anyhow::Result<usize> {
        let path = std::path::absolute(path)?;
        if let Some(index) = self
            .docs
            .iter()
            .position(|d| d.path.as_deref() == Some(&path))
        {
            return Ok(index);
        }
        let key = self.take_key();
        let doc = if is_image(&path) {
            self.doc_from_image(key, &path)?
        } else {
            Doc::open(key, &path, self.forced, &self.look)?
        };
        match self.docs.get(self.active) {
            Some(current) if current.is_blank() => {
                self.docs[self.active] = doc;
                Ok(self.active)
            }
            _ => {
                self.docs.push(doc);
                Ok(self.docs.len() - 1)
            }
        }
    }

    /// A new tab with the diagram source stored in the image at `path`,
    /// under a name in its folder that no file or tab has yet.
    pub(super) fn doc_from_image(&self, key: u64, path: &Path) -> anyhow::Result<Doc> {
        let size = std::fs::metadata(path)?.len();
        anyhow::ensure!(size <= MAX_IMAGE, "larger than 64 MB");
        let bytes = std::fs::read(path)?;
        let source = if bytes.starts_with(b"\x89PNG") {
            embed::source_from_png(&bytes)
        } else {
            std::str::from_utf8(&bytes)
                .ok()
                .and_then(embed::source_from_svg)
        }
        .context("no diagram source in this image")?;
        let language = source
            .language
            .unwrap_or_else(|| omaspec_core::detect(&source.code, None));
        let stem = source
            .file
            .as_deref()
            .and_then(|f| Path::new(f).file_stem())
            .or_else(|| path.file_stem())
            .map_or_else(|| "diagram".into(), |s| s.to_string_lossy().into_owned());
        let dir = path.parent().unwrap_or(Path::new("."));
        let taken: Vec<&Path> = self.docs.iter().filter_map(|d| d.path.as_deref()).collect();
        let name = free_name(dir, &stem, language.extension(), &taken);
        Ok(Doc::unsaved_as(key, name, &source.code, &self.look))
    }

    pub(super) fn open(&mut self, path: &Path) -> Task<Message> {
        match self.open_doc(path) {
            Ok(index) => {
                self.keep_session();
                Task::batch([self.select(index), self.load_specs()])
            }
            Err(err) => {
                self.notice = Some(format!("{}: {err:#}", path.display()));
                Task::none()
            }
        }
    }

    /// Closes a tab; one with unsaved changes only on the second ask. The
    /// last tab gives way to an empty one.
    pub(super) fn close(&mut self, index: usize) -> Task<Message> {
        let Some(doc) = self.docs.get(index) else {
            return Task::none();
        };
        if doc.modified && self.closing != Some(doc.key) {
            self.closing = Some(doc.key);
            self.notice = Some(format!(
                "{} has unsaved changes: Ctrl+S saves, Ctrl+W again closes it without saving",
                doc.name()
            ));
            return Task::none();
        }
        self.closing = None;
        self.notice = None;
        let gone = self.docs[index].key;
        if self
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.doc == gone)
        {
            self.prompt = None;
        }
        if self
            .compare
            .as_ref()
            .is_some_and(|c| c.a == gone || c.target == Target::Tab(gone))
        {
            self.stop_compare();
        }
        self.docs.remove(index);
        if self.docs.is_empty() {
            let key = self.take_key();
            self.docs.push(Doc::untitled(key, self.forced, &self.look));
        }
        // The tab that took the closed one's place, or the new last one.
        let next = if index < self.active || self.active >= self.docs.len() {
            self.active.saturating_sub(1)
        } else {
            self.active
        };
        self.active = next.min(self.docs.len() - 1);
        self.keep_session();
        self.select(self.active)
    }

    pub(super) fn save(&mut self, path: &Path) {
        let key = self.doc().key;
        self.save_doc(key, path);
    }

    pub(super) fn save_doc(&mut self, key: u64, path: &Path) {
        let Some(doc) = self.find(key) else {
            return;
        };
        let name = path.display().to_string();
        self.notice = Some(match doc.save(path) {
            Ok(()) => format!("saved {name}"),
            Err(err) => format!("not saved: {err:#}"),
        });
        self.keep_session();
    }

    /// Opens the prompt in the bottom line.
    pub(super) fn ask(&mut self, asking: Asking) -> Task<Message> {
        let folder = self
            .folder
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let doc = self.doc();
        let input = match asking {
            Asking::SaveAs => match &doc.path {
                Some(path) => path.display().to_string(),
                None => {
                    let name = format!("untitled.{}", doc.language.extension());
                    folder.join(name).display().to_string()
                }
            },
            Asking::Open => format!("{}/", folder.display()),
            Asking::Export(format) => suggested_export(doc, &folder, format),
        };
        let key = doc.key;
        // The question is drawn in the editor window. A shortcut from the
        // diagram window has to hand the keyboard over, or the answer
        // cannot be typed.
        let from_diagram = self
            .command_window
            .is_some_and(|id| self.detached == Some(id));
        self.close_picker();
        self.gallery = None;
        self.prompt = Some(Prompt {
            asking,
            input,
            overwrite: false,
            doc: key,
        });
        let focus = iced::widget::operation::focus(PROMPT);
        if from_diagram {
            Task::batch([window::gain_focus(self.main), focus])
        } else {
            focus
        }
    }

    pub(super) fn save_as_submitted(&mut self) -> Task<Message> {
        let Some(prompt) = &self.prompt else {
            return Task::none();
        };
        let (key, input, overwrite) = (prompt.doc, prompt.input.clone(), prompt.overwrite);
        let Some(path) = expand(input.trim()) else {
            self.notice = Some("name a file to save to".into());
            return Task::none();
        };
        let Some(index) = self.docs.iter().position(|doc| doc.key == key) else {
            self.prompt = None;
            return Task::none();
        };
        let own = self.docs[index].path.as_deref() == Some(path.as_path());
        if path.exists() && !own && !overwrite {
            if let Some(prompt) = &mut self.prompt {
                prompt.overwrite = true;
            }
            self.notice = Some(format!(
                "{} exists: Enter again overwrites it",
                path.display()
            ));
            return Task::none();
        }
        if path.is_dir() {
            self.notice = Some(format!("{} is a folder", path.display()));
            return Task::none();
        }
        self.save_doc(key, &path);
        let saved = self.docs.iter().any(|doc| doc.key == key && !doc.modified);
        if saved {
            self.prompt = None;
            return Task::batch([self.focus_editor(), self.rescan()]);
        }
        Task::none()
    }

    /// Writes the export the prompt names. An existing file asks once; the
    /// diagram's own source file is refused.
    pub(super) fn export_submitted(&mut self) -> Task<Message> {
        let Some(prompt) = &self.prompt else {
            return Task::none();
        };
        let Asking::Export(format) = prompt.asking else {
            return Task::none();
        };
        let (key, input, overwrite) = (prompt.doc, prompt.input.clone(), prompt.overwrite);
        let Some(typed) = expand(input.trim()) else {
            self.notice = Some("name a file to export to".into());
            return Task::none();
        };
        if typed.is_dir() {
            self.notice = Some(format!("{} is a folder", typed.display()));
            return Task::none();
        }
        let path = export_path(typed, format);
        let Some(index) = self.docs.iter().position(|doc| doc.key == key) else {
            self.prompt = None;
            return Task::none();
        };
        if self.docs[index].path.as_deref() == Some(path.as_path()) {
            self.notice = Some("that file is the diagram source; choose another name".into());
            return Task::none();
        }
        if let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty())
            && !parent.is_dir()
        {
            self.notice = Some(format!("{} is not a folder", parent.display()));
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
        let doc = &self.docs[index];
        let task = doc
            .paper
            .export_to(format, &doc.name(), path)
            .map(Message::Exported);
        self.prompt = None;
        task
    }

    /// Opens what the prompt names: a folder becomes the folder, a file
    /// opens in a tab (a new name is created on the first save).
    pub(super) fn open_submitted(&mut self) -> Task<Message> {
        let Some(path) = self.prompt.as_ref().and_then(|p| expand(p.input.trim())) else {
            self.notice = Some("name a file or folder to open".into());
            return Task::none();
        };
        self.prompt = None;
        if path.is_dir() {
            self.notice = Some(format!("folder {}", path.display()));
            let scan = self.set_folder(Some(path));
            self.keep_session();
            return Task::batch([scan, self.focus_editor()]);
        }
        self.open(&path)
    }

    /// Takes changes on disk into documents without unsaved changes; the
    /// others get a notice. Our own saves change nothing.
    pub(super) fn files_changed(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut notices = Vec::new();
        for path in std::mem::take(&mut self.changed) {
            let Some(doc) = self
                .docs
                .iter_mut()
                .find(|d| d.path.as_deref() == Some(&path))
            else {
                continue;
            };
            // Mid-save the file can be briefly missing; the next event retries.
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if doc.is_saved_text(&text) {
                continue;
            }
            if doc.modified {
                notices.push(format!(
                    "{} changed on disk; saving overwrites that",
                    doc.name()
                ));
                continue;
            }
            doc.reload(&text);
            doc.analyze();
            notices.push(format!("{} reloaded from disk", doc.name()));
            let key = doc.key;
            tasks.push(self.render(key, false));
        }
        if !notices.is_empty() {
            self.notice = Some(notices.join("; "));
        }
        // A state machine saved or changed on disk is checked against again.
        tasks.push(self.load_specs());
        Task::batch(tasks)
    }
}
