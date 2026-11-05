//! Opening, saving, exporting and closing documents, the prompt that asks
//! for a name, and changes on disk.

use std::path::{Path, PathBuf};

use anyhow::Context;
use iced::{Task, window};
use omaspec_core::embed;
use omaspec_render::export::Format;

use super::compare::Target;
use super::doc::Doc;
use super::{
    App, Asking, ExportLook, MAX_IMAGE, Message, PROMPT, Prompt, expand, export_path, free_name,
    is_image, plural, suggested_export,
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
            // Beside the project the sidebar is in, not inside it.
            Asking::NewProject { .. } => {
                let base = self
                    .folder
                    .as_deref()
                    .and_then(omaspec_render::project_root)
                    .and_then(|root| root.parent().map(Path::to_path_buf))
                    .unwrap_or(folder);
                format!("{}/", base.display())
            }
            Asking::Export(format) => suggested_export(doc, &folder, format),
            Asking::ExportProject => self
                .project_to_export()
                .unwrap_or(folder)
                .join(crate::project_export::FOLDER)
                .display()
                .to_string(),
            Asking::Rename => self
                .renaming
                .as_ref()
                .map(|r| r.named.name.clone())
                .unwrap_or_default(),
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
            look: ExportLook::Shown,
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

    /// The looks an export of `format` can take: as the pane shows it, the
    /// other of the file's and the theme's colors, and Omaspec's looks;
    /// `auto` for an SVG only. A Markdown export is the source alone.
    pub(super) fn export_looks(&self, format: Format) -> Vec<ExportLook> {
        use crate::look::Look;
        if format == Format::Markdown {
            return vec![ExportLook::Shown];
        }
        let mut looks = vec![ExportLook::Shown];
        looks.push(if self.follow_theme {
            ExportLook::File
        } else {
            ExportLook::Look(Look::Omarchy)
        });
        looks.extend([Look::Plain, Look::Light, Look::Dark].map(ExportLook::Look));
        if format == Format::Svg {
            looks.push(ExportLook::Look(Look::Auto));
        }
        looks
    }

    /// The looks the prompt `asking` offers: an export's, a project's as an
    /// SVG's; none for the other questions.
    pub(super) fn prompt_looks(&self, asking: Asking) -> Vec<ExportLook> {
        match asking {
            Asking::Export(format) => self.export_looks(format),
            Asking::ExportProject => self.export_looks(Format::Svg),
            _ => Vec::new(),
        }
    }

    /// The export prompt draws in `look`, or in the next one.
    pub(super) fn next_export_look(&mut self, look: Option<ExportLook>) {
        let Some(asking) = self.prompt.as_ref().map(|p| p.asking) else {
            return;
        };
        let looks = self.prompt_looks(asking);
        if looks.is_empty() {
            return;
        }
        if let Some(prompt) = &mut self.prompt {
            prompt.look = look.filter(|l| looks.contains(l)).unwrap_or_else(|| {
                let at = looks.iter().position(|l| *l == prompt.look).unwrap_or(0);
                looks[(at + 1) % looks.len()]
            });
        }
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
        let (key, input, overwrite, look) = (
            prompt.doc,
            prompt.input.clone(),
            prompt.overwrite,
            prompt.look,
        );
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
        let place = self.place_of(key);
        let doc = &self.docs[index];
        let own = place
            .as_ref()
            .zip(doc.path.as_deref())
            .and_then(|(place, path)| crate::specs::path_in(&place.root, path));
        let shown_name = doc
            .shown_diagram()
            .map_or_else(|| doc.name(), |(_, _, name)| name);
        let task = match look {
            ExportLook::Shown => doc
                .paper
                .export_to(format, &shown_name, own, path)
                .map(Message::Exported),
            ExportLook::File => export_in(None, doc, place, format, path),
            ExportLook::Look(look) => export_in(Some(look), doc, place, format, path),
        };
        self.prompt = None;
        task
    }

    /// The project the palette's *Export the project* writes: the active
    /// tab's, else the one the sidebar is in, else the sidebar's folder.
    pub(super) fn project_to_export(&self) -> Option<PathBuf> {
        self.place_of(self.doc().key)
            .map(|place| place.root)
            .or_else(|| {
                let folder = self.folder.as_deref()?;
                omaspec_render::project_root(folder).or_else(|| Some(folder.to_path_buf()))
            })
    }

    /// Asks where to export the project to.
    pub(super) fn export_project(&mut self) -> Task<Message> {
        if self.project_to_export().is_none() {
            self.notice = Some("open a project or a folder to export".into());
            return Task::none();
        }
        self.ask(Asking::ExportProject)
    }

    /// Exports the project into the folder the prompt names, in the
    /// background. A folder that is there asks once.
    pub(super) fn export_project_submitted(&mut self) -> Task<Message> {
        let Some(prompt) = &self.prompt else {
            return Task::none();
        };
        let (input, overwrite, look) = (prompt.input.clone(), prompt.overwrite, prompt.look);
        let Some(root) = self.project_to_export() else {
            self.prompt = None;
            return Task::none();
        };
        let Some(out) = expand(input.trim()) else {
            self.notice = Some("name a folder to export to".into());
            return Task::none();
        };
        if out.is_file() {
            self.notice = Some(format!("{} is a file", out.display()));
            return Task::none();
        }
        if out.is_dir() && !overwrite {
            if let Some(prompt) = &mut self.prompt {
                prompt.overwrite = true;
            }
            self.notice = Some(format!(
                "{} is there: Enter again writes into it",
                out.display()
            ));
            return Task::none();
        }
        let look = match look {
            // As the pane shows it: in the look the project pins, else in
            // the theme's colors when it follows them.
            ExportLook::Shown if self.follow_theme && self.pinned_look(Some(&root)).is_none() => {
                Some(crate::look::Look::Omarchy)
            }
            ExportLook::Shown => None,
            ExportLook::File => Some(crate::look::Look::File),
            ExportLook::Look(look) => Some(look),
        };
        self.prompt = None;
        self.notice = Some(format!("exporting {} …", root.display()));
        Task::perform(
            crate::worker::blocking(move || -> Result<String, String> {
                let mut renderer = omaspec_render::Renderer::new(&crate::commands::render_config());
                crate::project_export::export(&root, Some(&out), look, &mut renderer)
                    .map(|exported| exported.summary())
                    .map_err(|e| format!("{e:#}"))
            }),
            |result| {
                Message::ProjectExported(result.unwrap_or_else(|| Err("the export failed".into())))
            },
        )
    }

    /// Asks how a new project starts: in PlantUML or Mermaid, with git or
    /// without. The prompt for its folder comes next.
    pub(super) fn ask_project_kind(&mut self) -> Task<Message> {
        use super::picker::{Choice, Item, Kind, Picker};
        use omaspec_core::Language;
        let items = [
            (Language::PlantUml, false),
            (Language::PlantUml, true),
            (Language::Mermaid, false),
            (Language::Mermaid, true),
        ]
        .into_iter()
        .map(|(language, git)| {
            let main = omaspec_core::project::main_file(language);
            Item {
                label: match git {
                    false => language.name().to_owned(),
                    true => format!("{}, with git", language.name()),
                },
                hint: match git {
                    false => main,
                    true => format!("{main} · git init, pre-commit hook"),
                },
                choice: Choice::NewProject { language, git },
            }
        })
        .collect();
        self.place_picker(Picker::new(Kind::NewProject, "start a project in", items));
        iced::widget::operation::focus(super::PICKER)
    }

    /// Makes the folder in the prompt a project, shows it in the sidebar
    /// and opens its first diagram. What went wrong keeps the prompt open.
    pub(super) fn new_project_submitted(
        &mut self,
        language: omaspec_core::Language,
        git: bool,
    ) -> Task<Message> {
        let Some(path) = self.prompt.as_ref().and_then(|p| expand(p.input.trim())) else {
            self.notice = Some("name a folder for the project".into());
            return Task::none();
        };
        let made = match crate::project::create(&path, language, git) {
            Ok(made) => made,
            Err(err) => {
                self.notice = Some(format!("{err:#}"));
                return Task::none();
            }
        };
        self.prompt = None;
        let mut notice = made.summary();
        // Its state machines are read with the folder's.
        let scan = self.set_folder(Some(made.root.clone()));
        let shown = match self.open_doc(&made.root.join(&made.main)) {
            Ok(index) => self.select(index),
            Err(err) => {
                notice = format!("{notice}; {} did not open: {err:#}", made.main);
                Task::none()
            }
        };
        self.notice = Some(notice);
        self.keep_session();
        Task::batch([scan, shown])
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
fn export_in(
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
        let rendered = crate::look::render(
            &mut crate::specs::Maps::default(),
            &mut renderer,
            self.text,
            self.language,
            self.path,
            self.place,
            skin.as_ref(),
        )
        .map_err(|e| e.to_string())?;
        let links = self.own.map_or_else(Vec::new, |own| {
            crate::specs::export_links(&rendered.links, own)
        });
        let options = omaspec_render::export::Options {
            scale: 2.0,
            source: Some(omaspec_render::export::Source {
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
