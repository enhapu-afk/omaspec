//! Lists of places in a project, each a line of a file to go to: what
//! *Check the project* finds, and where the name at the caret is used
//! (`docs/adr/0019-where-a-name-is-used.md`). The command line has both
//! as `omaspec check` and `omaspec refs`.

use iced::Task;
use omaspec_core::lint::Severity;
use omaspec_model::File;

use super::picker::{Choice, Item, Kind, Picker};
use super::{App, Message, PICKER};
use crate::names::{self, Owned};

impl App {
    /// The project the tab is in, else the sidebar's.
    fn places_root(&mut self) -> Option<std::path::PathBuf> {
        let root = self
            .project_of(self.doc())
            .or_else(|| self.folder_project());
        if root.is_none() {
            self.notice = Some("this works in a project: open its folder first".into());
        }
        root
    }

    /// Lints every diagram of the project, as saved, and lists what it
    /// finds; Enter goes to the line.
    pub(super) fn check_project(&mut self) -> Task<Message> {
        let Some(root) = self.places_root() else {
            return Task::none();
        };
        let found = match crate::check_command::findings(&root) {
            Ok(found) => found,
            Err(err) => {
                self.notice = Some(format!("not checked: {err:#}"));
                return Task::none();
            }
        };
        if found.is_empty() {
            self.notice = Some("the project checks clean: no finding in its saved files".into());
            return Task::none();
        }
        let rank = |severity: Severity| match severity {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Info => 2,
        };
        let mut found = found;
        found.sort_by(|a, b| {
            rank(a.1.severity)
                .cmp(&rank(b.1.severity))
                .then_with(|| a.0.cmp(&b.0))
                .then(a.1.line.cmp(&b.1.line))
        });
        let items = found
            .into_iter()
            .map(|(file, finding)| Item {
                label: format!("{file}:{}: {}", finding.line + 1, finding.message),
                hint: match finding.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Info => "info",
                }
                .to_owned(),
                choice: Choice::ShowIn {
                    path: root.join(&file),
                    line: finding.line,
                    cols: 0..0,
                },
            })
            .collect();
        self.place_picker(Picker::new(Kind::Places, "findings in the project", items));
        iced::widget::operation::focus(PICKER)
    }

    /// Lists every use of the diagram, state or event the caret names;
    /// Enter goes to the line.
    pub(super) fn show_uses(&mut self) -> Task<Message> {
        let doc = self.doc();
        let (Some(root), Some(path)) = (self.project_of(doc), doc.path.clone()) else {
            self.notice = Some("where a name is used is asked in a saved file of a project".into());
            return Task::none();
        };
        let Some(file) = crate::specs::path_in(&root, &path) else {
            return Task::none();
        };
        let caret = doc.editor.cursor();
        let files = self.project_files(&root);
        let specs = names::specs_of(&files);
        let read: Vec<File> = files.iter().map(Owned::file).collect();
        let Some(named) =
            omaspec_model::named_at(&file, caret.row as usize, caret.col as usize, &read, &specs)
        else {
            self.notice = Some("no diagram, state or event is named at the caret".into());
            return Task::none();
        };
        let uses = match omaspec_model::refs(&named.name, named.machine.as_deref(), &read, &specs) {
            Ok(uses) => uses,
            Err(why) => {
                self.notice = Some(why);
                return Task::none();
            }
        };
        if uses.is_empty() {
            self.notice = Some(format!("nothing in the project uses {}", named.name));
            return Task::none();
        }
        self.notice = Some(format!("{} is used in these places", named.name));
        let items = uses
            .into_iter()
            .map(|used| Item {
                label: format!("{}:{}: {}", used.file, used.line + 1, used.text),
                hint: used.kind.name().to_owned(),
                choice: Choice::ShowIn {
                    path: root.join(&used.file),
                    line: used.line,
                    cols: 0..0,
                },
            })
            .collect();
        self.place_picker(Picker::new(Kind::Places, "where it is used", items));
        iced::widget::operation::focus(PICKER)
    }

    /// The text commands of the Edit menu, which the editor has on its
    /// keys: they act on the source of the tab.
    pub(super) fn edit(&mut self, action: scrive_iced::Action) -> Task<Message> {
        let key = self.doc().key;
        let done = self.update(Message::Editor(key, scrive_iced::Event::Editor(action)));
        Task::batch([done, self.focus_editor()])
    }

    /// Copies the selection, or the caret's line without one, and with
    /// `cut` takes it out.
    pub(super) fn copy(&mut self, cut: bool) -> Task<Message> {
        let (text, _) = self.doc().editor.document().clipboard_payload();
        if text.is_empty() {
            return self.focus_editor();
        }
        let copied = iced::clipboard::write::<Message>(text);
        if cut {
            Task::batch([copied, self.edit(scrive_iced::Action::Cut)])
        } else {
            Task::batch([copied, self.focus_editor()])
        }
    }

    /// Asks the clipboard for its text, which [`Message::Pasted`] puts at
    /// the caret.
    pub(super) fn paste(&mut self) -> Task<Message> {
        iced::clipboard::read().map(Message::Pasted)
    }

    /// Asks git for the commits that changed the tab's file and what each
    /// changed in its behavior; [`Message::Past`] lists them.
    pub(super) fn history(&mut self) -> Task<Message> {
        let doc = self.doc();
        let key = doc.key;
        let Some(path) = doc.path.clone().filter(|_| doc.on_disk()) else {
            self.notice = Some("a file has a history once it is saved".into());
            return Task::none();
        };
        self.notice = Some("reading the history…".into());
        Task::perform(
            crate::worker::blocking(move || {
                crate::history::past(&path, HISTORY).map_err(|err| format!("{err:#}"))
            }),
            move |found| Message::Past(key, found.unwrap_or_else(|| Err("no answer".into()))),
        )
    }

    /// Lists the commits of the tab `key`; Enter compares the tab with
    /// the file as of that commit.
    pub(super) fn past(
        &mut self,
        key: u64,
        found: Result<Vec<crate::history::Past>, String>,
    ) -> Task<Message> {
        if self.doc().key != key {
            return Task::none();
        }
        let found = match found {
            Ok(found) if !found.is_empty() => found,
            Ok(_) => {
                self.notice = Some("no commit of a git repository changed this file".into());
                return Task::none();
            }
            Err(why) => {
                self.notice = Some(format!("no history: {why}"));
                return Task::none();
            }
        };
        self.notice = None;
        let items = found
            .into_iter()
            .map(|past| Item {
                label: past.label,
                hint: past.changed,
                choice: Choice::Compare(super::compare::Target::Commit {
                    label: format!("git {}", past.rev.chars().take(7).collect::<String>()),
                    rev: past.rev,
                }),
            })
            .collect();
        self.place_picker(Picker::new(Kind::Places, "history of the diagram", items));
        iced::widget::operation::focus(PICKER)
    }

    /// Notes `path` as the file opened last.
    pub(super) fn opened(&mut self, path: &std::path::Path) {
        self.recent_files.retain(|have| have != path);
        self.recent_files.insert(0, path.to_owned());
        self.recent_files.truncate(RECENT);
    }

    /// Lists the files opened last, the newest first, that are still
    /// there; Enter opens one.
    pub(super) fn open_recent(&mut self) -> Task<Message> {
        let items: Vec<Item> = self
            .recent_files
            .iter()
            .filter(|path| path.is_file())
            .map(|path| Item {
                label: path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                hint: path
                    .parent()
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_default(),
                choice: Choice::Open(path.clone()),
            })
            .collect();
        if items.is_empty() {
            self.notice = Some("no file opened lately is still there".into());
            return Task::none();
        }
        self.place_picker(Picker::new(Kind::Places, "open recent", items));
        iced::widget::operation::focus(PICKER)
    }
}

/// How many commits the history lists.
const HISTORY: usize = 50;
/// How many files opened last are kept.
const RECENT: usize = 20;
