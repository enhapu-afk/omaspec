//! Opening, saving, exporting and closing documents, the prompt that asks
//! for a name, and changes on disk.

use std::path::{Path, PathBuf};

use anyhow::Context;
use iced::Task;
use omaspec_core::embed;
use omaspec_render::export::Format;

use super::compare::Target;
use super::doc::Doc;
use super::{App, Asking, Message, free_name, plural};

/// Images larger than this are not searched for a diagram.
const MAX_IMAGE: u64 = 64 << 20;

fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "png" | "svg" | "jpg" | "jpeg"
        )
    })
}

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
            self.opened(&path);
            return Ok(index);
        }
        let key = self.take_key();
        let doc = if is_image(&path) {
            self.doc_from_image(key, &path)?
        } else {
            Doc::open(key, &path, self.forced, &self.look)?
        };
        self.opened(&path);
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
        // Its unsaved changes go with it: the maps show its file again.
        let closed = &self.docs[index];
        let unsaved = (closed.modified && !closed.is_document())
            .then(|| self.project_of(closed))
            .flatten();
        self.docs.remove(index);
        if self.docs.is_empty() {
            let key = self.take_key();
            self.docs.push(Doc::untitled(key, self.forced, &self.look));
        }
        let maps = match unsaved {
            Some(root) => self.untile(&root),
            None => Task::none(),
        };
        // The tab that took the closed one's place, or the new last one.
        let next = if index < self.active || self.active >= self.docs.len() {
            self.active.saturating_sub(1)
        } else {
            self.active
        };
        self.active = next.min(self.docs.len() - 1);
        self.keep_session();
        Task::batch([maps, self.select(self.active)])
    }

    /// Saves the tab under its name, or asks for one: also when the name,
    /// not written yet, has been taken since.
    pub(super) fn save_tab(&mut self) -> Task<Message> {
        let doc = self.doc();
        match doc.path.clone() {
            Some(path) if !doc.on_disk() && path.exists() => {
                let task = self.ask(Asking::SaveAs);
                self.notice = Some(format!(
                    "{} exists now: Enter overwrites it",
                    path.display()
                ));
                if let Some(prompt) = &mut self.prompt {
                    prompt.overwrite = true;
                }
                task
            }
            Some(path) => {
                self.save(&path);
                Task::none()
            }
            None => self.ask(Asking::SaveAs),
        }
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

    /// Takes changes on disk into documents without unsaved changes; the
    /// others get a notice. Our own saves change nothing.
    pub(super) fn files_changed(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        let mut notices = Vec::new();
        let changed = std::mem::take(&mut self.changed);
        // A project file: the look it pins may be another now.
        if changed.iter().any(|p| {
            p.file_name()
                .is_some_and(|n| n == omaspec_core::project::FILE)
        }) {
            self.forget_themed_atlases();
            self.rerender_all();
        }
        for path in changed {
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

/// `doc` drawn anew in `look` (the file's colors without one) and written
/// to `out` as `format`, in the background; the image keeps the file's
/// own source.
pub(super) fn export_in(
    look: Option<crate::look::Look>,
    doc: &Doc,
    place: Option<omaspec_render::Place>,
    format: Format,
    out: PathBuf,
) -> Task<Message> {
    // A document's diagram as itself: README-2.puml.
    let (text, language, file) = doc
        .shown_diagram()
        .unwrap_or_else(|| (doc.text(), doc.language, doc.name()));
    let path = doc.path.clone();
    // A project's file: the map of the folder it shows.
    let map = doc.is_project().then(|| doc.folder.clone());
    Task::perform(
        crate::worker::blocking(move || -> Result<String, String> {
            let own = place
                .as_ref()
                .zip(path.as_deref())
                .and_then(|(place, path)| crate::specs::path_in(&place.root, path));
            let drawn = Drawn {
                text: &text,
                language,
                path: path.as_deref(),
                place: place.as_ref(),
                file: &file,
                own: own.as_deref(),
                map: map.as_deref(),
            };
            let bytes = drawn.export(look, format)?;
            crate::files::write_atomic(&out, &bytes).map_err(|e| format!("{e:#}"))?;
            Ok(out.display().to_string())
        }),
        |result| Message::Exported(result.unwrap_or_else(|| Err("the export failed".into()))),
    )
}

/// A diagram to export in a look.
pub(super) struct Drawn<'a> {
    pub text: &'a str,
    pub language: omaspec_core::Language,
    pub path: Option<&'a Path>,
    pub place: Option<&'a omaspec_render::Place>,
    /// The file name stored with the source.
    pub file: &'a str,
    /// Its path in its project: its tiles and states link to the exports
    /// of the diagrams they open.
    pub own: Option<&'a str>,
    /// A project's file: the folder whose map is exported, which is made
    /// from the project and has no source of its own (docs/adr/0026).
    pub map: Option<&'a str>,
}

impl Drawn<'_> {
    /// The diagram in `look` (the file's colors without one) as `format`,
    /// with the file's own source in it.
    pub(super) fn export(
        &self,
        look: Option<crate::look::Look>,
        format: Format,
    ) -> Result<Vec<u8>, String> {
        let mut renderer = omaspec_render::Renderer::new(&crate::commands::render_config());
        let skin = look.and_then(crate::look::Look::skin);
        let mut maps = crate::specs::Maps::default();
        let rendered = match (self.map, self.place) {
            (Some(folder), Some(place)) => crate::look::render_map(
                &mut maps,
                &mut renderer,
                &place.root,
                folder,
                skin.as_ref(),
            ),
            _ => crate::look::render(
                &mut maps,
                &mut renderer,
                self.text,
                self.language,
                self.path,
                self.place,
                skin.as_ref(),
            ),
        }
        .map_err(|e| e.to_string())?;
        let links = self.own.map_or_else(Vec::new, |own| {
            crate::specs::export_links(&rendered.links, own, false)
        });
        let options = omaspec_render::export::Options {
            scale: 2.0,
            source: self
                .map
                .is_none()
                .then_some(omaspec_render::export::Source {
                    code: self.text,
                    language: self.language,
                    file: Some(self.file),
                }),
            links: &links,
            up: self.own.map_or(0, crate::specs::depth),
        };
        omaspec_render::export::export(&rendered.svg, format, &options).map_err(|e| e.to_string())
    }
}
