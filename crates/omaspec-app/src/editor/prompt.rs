//! The prompt in the bottom line: what it asks for (a file to save to or
//! to open, where an export or a new project goes, a new name), its key,
//! and what Enter does with the answer.

use std::path::{Path, PathBuf};

use iced::keyboard::{Key, Modifiers, key::Named};
use iced::{Task, window};
use omaspec_core::Language;
use omaspec_render::export::Format;

use super::doc::Doc;
use super::files::export_in;
use super::{App, Command, Message, PROMPT};
use crate::save_dialog::Answer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Asking {
    SaveAs,
    Open,
    /// The folder of a new project, its first diagram in this language.
    NewProject {
        language: Language,
        git: bool,
    },
    /// Where to write this export. Enter writes it; an existing file asks
    /// once more.
    Export(Format),
    /// The folder to export the project into. A folder that is there asks
    /// once more.
    ExportProject,
    /// The new name of what the caret names ([`App::renaming`]).
    Rename,
}

/// A question in the bottom line.
pub(super) struct Prompt {
    pub(super) asking: Asking,
    pub(super) input: String,
    /// The file named exists; Enter again overwrites it.
    pub(super) overwrite: bool,
    /// The tab that opened the prompt. Enter saves that tab.
    pub(super) doc: u64,
    /// What an export is drawn in; Tab takes the next.
    pub(super) look: ExportLook,
}

/// What an export is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportLook {
    /// As the diagram pane shows it.
    Shown,
    /// In the file's own colors.
    File,
    /// In a look laid over the file's colors ([`crate::look`]).
    Look(crate::look::Look),
}

impl ExportLook {
    pub fn label(self) -> &'static str {
        match self {
            ExportLook::Shown => "as shown",
            ExportLook::File => "file's colors",
            ExportLook::Look(look) => look.name(),
        }
    }

    /// What the save dialog's choice of colors answers for it.
    pub fn key(self) -> &'static str {
        match self {
            ExportLook::Shown => "shown",
            ExportLook::File => "own",
            ExportLook::Look(look) => look.name(),
        }
    }

    /// As the save dialog lists it: `As shown`.
    pub fn title(self) -> String {
        let label = self.label();
        let mut chars = label.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
    }
}

/// The id of the save dialog's choice of colors.
pub(super) const LOOK: &str = "look";

/// A format as a menu and a dialog name it.
pub(super) fn format_name(format: Format) -> &'static str {
    match format {
        Format::Svg => "SVG",
        Format::Png => "PNG",
        Format::Jpeg => "JPEG",
        Format::Pdf => "PDF",
        Format::Markdown => "Markdown",
        Format::Zip => "ZIP",
    }
}

/// The prompt's key beside what its field takes: Tab is the next look
/// an export is drawn in.
pub(super) fn key(key: &Key, mods: Modifiers) -> Option<Command> {
    matches!(key.as_ref(), Key::Named(Named::Tab) if !mods.control()).then_some(Command::NextLook)
}

impl App {
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
        self.write_export(index, format, path, look)
    }

    /// Why the export of the tab at `index` cannot go to `path`: it is the
    /// diagram's own source, or its folder is not there.
    fn export_target(&self, index: usize, path: &Path) -> Result<(), String> {
        if self.docs[index].path.as_deref() == Some(path) {
            return Err("that file is the diagram source; choose another name".into());
        }
        if let Some(parent) = path.parent().filter(|dir| !dir.as_os_str().is_empty())
            && !parent.is_dir()
        {
            return Err(format!("{} is not a folder", parent.display()));
        }
        Ok(())
    }

    /// Writes the export of the tab at `index` to `path`, in `look`, in the
    /// background; [`Message::Exported`] says how it went.
    fn write_export(
        &mut self,
        index: usize,
        format: Format,
        path: PathBuf,
        look: ExportLook,
    ) -> Task<Message> {
        let key = self.docs[index].key;
        let place = self.place_of(key);
        let doc = &self.docs[index];
        let own = place
            .as_ref()
            .zip(doc.path.as_deref())
            .and_then(|(place, path)| crate::specs::path_in(&place.root, path));
        let shown_name = doc
            .shown_diagram()
            .map_or_else(|| doc.name(), |(_, _, name)| name);
        match look {
            ExportLook::Shown => doc
                .paper
                .export_to(format, &shown_name, own, path)
                .map(Message::Exported),
            ExportLook::File => export_in(None, doc, place, format, path),
            ExportLook::Look(look) => export_in(Some(look), doc, place, format, path),
        }
    }

    /// What the desktop's save dialog answered for the export of tab `key`:
    /// a file, which is written in the colors chosen (the dialog asked
    /// before it replaces one); nothing; or no dialog, and the bottom line
    /// asks instead.
    pub(super) fn export_answered(
        &mut self,
        key: u64,
        format: Format,
        answer: Answer,
    ) -> Task<Message> {
        let Some(index) = self.docs.iter().position(|doc| doc.key == key) else {
            return Task::none();
        };
        match &answer {
            Answer::Cancelled => Task::none(),
            Answer::Unavailable(why) => {
                if self.doc().key != key {
                    self.notice = Some(format!("no save dialog: {why}"));
                    return Task::none();
                }
                let task = self.ask(Asking::Export(format));
                self.notice = Some(format!("no save dialog ({why}): name the file here"));
                task
            }
            Answer::Chosen { path, .. } => {
                let path = export_path(path.clone(), format);
                if let Err(why) = self.export_target(index, &path) {
                    self.notice = Some(why);
                    return Task::none();
                }
                let look = answer
                    .choice(LOOK)
                    .and_then(|chosen| {
                        self.export_looks(format)
                            .into_iter()
                            .find(|look| look.key() == chosen)
                    })
                    .unwrap_or(ExportLook::Shown);
                self.write_export(index, format, path, look)
            }
        }
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
            let first = format!("a first state machine, .{}", language.extension());
            Item {
                label: match git {
                    false => language.name().to_owned(),
                    true => format!("{}, with git", language.name()),
                },
                hint: match git {
                    false => first,
                    true => format!("{first} · git init, pre-commit hook"),
                },
                choice: Choice::NewProject { language, git },
            }
        })
        .collect();
        self.place_picker(Picker::new(Kind::NewProject, "start a project in", items));
        iced::widget::operation::focus(super::PICKER)
    }

    /// Makes the folder in the prompt a project, shows it in the sidebar
    /// and opens its file, beside which its map stands. What went wrong
    /// keeps the prompt open.
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
        let file = omaspec_core::project::FILE;
        let shown = match self.open_doc(&made.root.join(file)) {
            Ok(index) => self.select(index),
            Err(err) => {
                notice = format!("{notice}; {file} did not open: {err:#}");
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
}

/// Where an export starts: beside a saved diagram, or `untitled` in the
/// folder. Never the source file itself.
pub(super) fn suggested_export(doc: &Doc, folder: &Path, kind: Format) -> String {
    let path = match &doc.path {
        Some(path) => {
            let out = if doc.is_document() {
                // A document's diagram by its number: README-2.svg.
                let name = doc
                    .shown_diagram()
                    .map_or_else(|| doc.name(), |(_, _, name)| name);
                let stem = name
                    .rsplit_once('.')
                    .map_or(name.as_str(), |(stem, _)| stem);
                path.with_file_name(format!("{stem}.{}", kind.extension()))
            } else {
                path.with_extension(kind.extension())
            };
            if doc.path.as_deref() == Some(out.as_path()) {
                let stem = path
                    .file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "diagram".into());
                path.with_file_name(format!("{stem}-export.{}", kind.extension()))
            } else {
                out
            }
        }
        None => folder.join(format!("untitled.{}", kind.extension())),
    };
    path.display().to_string()
}

/// A path typed into the prompt: `~` is the home folder, a relative path
/// starts in the working directory.
pub(super) fn expand(input: &str) -> Option<PathBuf> {
    if input.is_empty() {
        return None;
    }
    let path = match input.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME")?).join(rest),
        None => PathBuf::from(input),
    };
    std::path::absolute(path).ok()
}

/// `typed` with the extension of `format`: kept when it names that format,
/// swapped when it names another export format, added otherwise, so a dot
/// in the name stays (`flow-1.2` → `flow-1.2.svg`).
pub(super) fn export_path(typed: PathBuf, format: Format) -> PathBuf {
    let named = typed
        .extension()
        .and_then(|ext| Format::from_extension(&ext.to_string_lossy()));
    match named {
        Some(named) if named == format => typed,
        Some(_) => typed.with_extension(format.extension()),
        None => {
            let mut name = typed.into_os_string();
            name.push(".");
            name.push(format.extension());
            PathBuf::from(name)
        }
    }
}
