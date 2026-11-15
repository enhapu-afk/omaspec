//! Saving a tab under a name, opening a file or a folder, and the folder of
//! a new project, in the desktop's own dialogs ([`crate::file_dialog`]), as
//! an export asks: a question in the bottom line is easily missed. Where no
//! dialog comes, the bottom line asks as before.

use std::path::{Path, PathBuf};

use iced::Task;
use omaspec_core::Language;

use super::{App, Asking, Message};
use crate::file_dialog::{self, Answer};

/// The kinds of file Omaspec opens, as the dialog offers them.
fn openable() -> Vec<(String, Vec<String>)> {
    let patterns = |patterns: &[&str]| patterns.iter().map(|p| (*p).to_owned()).collect();
    vec![
        (
            "Diagrams and documents".into(),
            patterns(&[
                "*.puml",
                "*.plantuml",
                "*.wsd",
                "*.mmd",
                "*.mermaid",
                "*.md",
                "*.markdown",
            ]),
        ),
        (
            "Pictures that carry their diagram".into(),
            patterns(&["*.svg", "*.png", "*.jpg", "*.jpeg"]),
        ),
    ]
}

impl App {
    /// Where a dialog opens when nothing else says: the sidebar's folder,
    /// else the one Omaspec was started in.
    fn start_folder(&self) -> PathBuf {
        self.folder
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default()
    }

    /// Asks under which name to save the tab: in the desktop's dialog,
    /// else in the bottom line.
    pub(super) fn save_as(&mut self) -> Task<Message> {
        if !self.dialogs {
            return self.ask(Asking::SaveAs);
        }
        let key = self.doc().key;
        let asked = self.save_as_asked();
        self.close_picker();
        Task::perform(file_dialog::save(asked), move |answer| {
            Message::SaveAsAnswered(key, answer)
        })
    }

    /// What the save dialog asks for the active tab: its name and folder,
    /// or `untitled` in the sidebar's folder, and its kind of file.
    pub(super) fn save_as_asked(&self) -> file_dialog::Asked {
        let doc = self.doc();
        let (folder, name) = match &doc.path {
            Some(path) => (
                path.parent()
                    .map_or_else(|| self.start_folder(), Path::to_path_buf),
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ),
            None => (
                self.start_folder(),
                format!("untitled.{}", doc.language.extension()),
            ),
        };
        let kind = if doc.is_document() {
            ("Markdown document", "text/markdown", "*.md")
        } else {
            match doc.language {
                Language::PlantUml => ("PlantUML diagram", "", "*.puml"),
                Language::Mermaid => ("Mermaid diagram", "", "*.mmd"),
            }
        };
        file_dialog::Asked {
            title: "Save the diagram as".into(),
            accept: "Save".into(),
            folder,
            name,
            kind: (kind.0.into(), kind.1.into(), kind.2.into()),
            choices: Vec::new(),
        }
    }

    /// Saves the tab `key` where the dialog says; the bottom line asks
    /// where no dialog came.
    pub(super) fn save_as_answered(&mut self, key: u64, answer: Answer) -> Task<Message> {
        match answer {
            Answer::Cancelled => Task::none(),
            Answer::Unavailable(why) => {
                if self.doc().key != key {
                    self.notice = Some(format!("no save dialog: {why}"));
                    return Task::none();
                }
                let task = self.ask(Asking::SaveAs);
                self.notice = Some(format!("no save dialog ({why}): name the file here"));
                task
            }
            Answer::Chosen { path, .. } => {
                if !self.docs.iter().any(|doc| doc.key == key) {
                    return Task::none();
                }
                if path.is_dir() {
                    self.notice = Some(format!("{} is a folder", path.display()));
                    return Task::none();
                }
                // The dialog asked before it let a file be replaced.
                self.save_doc(key, &path);
                let saved = self.docs.iter().any(|doc| doc.key == key && !doc.modified);
                if saved {
                    Task::batch([self.focus_editor(), self.rescan()])
                } else {
                    Task::none()
                }
            }
        }
    }

    /// Asks which file to open (`folder`: which folder for the sidebar):
    /// in the desktop's dialog, else in the bottom line, which takes both.
    pub(super) fn open_asking(&mut self, folder: bool) -> Task<Message> {
        if !self.dialogs {
            return self.ask(Asking::Open);
        }
        let asked = self.open_asked(folder);
        self.close_picker();
        Task::perform(file_dialog::open(asked), move |answer| {
            Message::OpenAnswered(folder, answer)
        })
    }

    /// What the dialog to open asks: a diagram, a document or a picture
    /// that carries its diagram, or a folder.
    pub(super) fn open_asked(&self, folder: bool) -> file_dialog::Opening {
        file_dialog::Opening {
            title: if folder {
                "Open a folder".into()
            } else {
                "Open a diagram".into()
            },
            accept: "Open".into(),
            folder: self.start_folder(),
            kinds: if folder { Vec::new() } else { openable() },
            directory: folder,
        }
    }

    /// Opens what the dialog chose; the bottom line asks where no dialog
    /// came.
    pub(super) fn open_answered(&mut self, answer: Answer) -> Task<Message> {
        match answer {
            Answer::Cancelled => Task::none(),
            Answer::Unavailable(why) => {
                let task = self.ask(Asking::Open);
                self.notice = Some(format!("no open dialog ({why}): name it here"));
                task
            }
            Answer::Chosen { path, .. } => self.open_path(path),
        }
    }

    /// Asks for the folder of a new project: in the desktop's dialog,
    /// beside the project the sidebar is in, else in the bottom line.
    pub(super) fn new_project(&mut self, language: Language, git: bool) -> Task<Message> {
        if !self.dialogs {
            return self.ask(Asking::NewProject { language, git });
        }
        let asked = file_dialog::Opening {
            title: "The folder for the new project".into(),
            accept: "Make the project".into(),
            folder: self.project_base(),
            kinds: Vec::new(),
            directory: true,
        };
        self.close_picker();
        Task::perform(file_dialog::open(asked), move |answer| {
            Message::NewProjectAnswered(language, git, answer)
        })
    }

    /// Where a new project goes: beside the project the sidebar is in, not
    /// inside it.
    pub(super) fn project_base(&self) -> PathBuf {
        self.folder
            .as_deref()
            .and_then(omaspec_render::project_root)
            .and_then(|root| root.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| self.start_folder())
    }

    /// Makes the project in the folder the dialog chose; the bottom line
    /// asks where no dialog came.
    pub(super) fn new_project_answered(
        &mut self,
        language: Language,
        git: bool,
        answer: Answer,
    ) -> Task<Message> {
        match answer {
            Answer::Cancelled => Task::none(),
            Answer::Unavailable(why) => {
                let task = self.ask(Asking::NewProject { language, git });
                self.notice = Some(format!("no folder dialog ({why}): name the folder here"));
                task
            }
            Answer::Chosen { path, .. } => self.make_project(&path, language, git),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::{App, Message, Start};
    use crate::file_dialog::Answer;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("omaspec-dialogs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn open(files: &[PathBuf]) -> App {
        let start = Start {
            files: files.to_vec(),
            ..Start::default()
        };
        App::new(start, None).0
    }

    fn chosen(path: PathBuf) -> Answer {
        Answer::Chosen {
            path,
            choices: Vec::new(),
        }
    }

    /// Save As asks in the desktop's dialog under the tab's own name, for
    /// its kind of file; the file chosen is written, a folder refused, and
    /// the bottom line asks where no dialog came.
    #[test]
    fn save_as_asks_in_the_desktop_s_dialog() {
        let dir = temp_dir("save-as");
        let src = dir.join("door.puml");
        std::fs::write(&src, "@startuml\n[*] --> Closed\n@enduml\n").unwrap();
        let mut app = open(std::slice::from_ref(&src));
        let key = app.doc().key;
        let asked = app.save_as_asked();
        assert_eq!(
            (asked.folder.as_path(), asked.name.as_str()),
            (dir.as_path(), "door.puml")
        );
        assert_eq!(asked.kind.2, "*.puml");

        let copy = dir.join("copy.puml");
        let _ = app.update(Message::SaveAsAnswered(key, chosen(copy.clone())));
        assert!(copy.exists(), "{:?}", app.notice);
        assert_eq!(app.doc().path.as_deref(), Some(copy.as_path()));

        let _ = app.update(Message::SaveAsAnswered(key, chosen(dir.clone())));
        assert!(
            app.notice
                .as_deref()
                .is_some_and(|n| n.contains("is a folder"))
        );

        let _ = app.update(Message::SaveAsAnswered(
            key,
            Answer::Unavailable("no portal".into()),
        ));
        assert!(app.prompt.is_some());
        assert!(
            app.notice
                .as_deref()
                .is_some_and(|n| n.contains("no save dialog"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Open asks for a diagram, a document or a picture, or for a folder;
    /// what is chosen opens, a folder in the sidebar.
    #[test]
    fn open_asks_for_a_file_or_a_folder() {
        let dir = temp_dir("open");
        let src = dir.join("flow.mmd");
        std::fs::write(&src, "flowchart LR\n  A --> B\n").unwrap();
        let mut app = open(&[]);
        let file = app.open_asked(false);
        assert!(!file.directory);
        assert!(file.kinds[0].1.contains(&"*.mmd".to_owned()));
        let folder = app.open_asked(true);
        assert!(folder.directory && folder.kinds.is_empty());

        let _ = app.update(Message::OpenAnswered(false, chosen(src.clone())));
        assert_eq!(app.doc().path.as_deref(), Some(src.as_path()));
        let _ = app.update(Message::OpenAnswered(true, chosen(dir.clone())));
        assert_eq!(app.folder.as_deref(), Some(dir.as_path()));
        let _ = app.update(Message::OpenAnswered(
            false,
            Answer::Unavailable("no portal".into()),
        ));
        assert!(app.prompt.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A new project goes into the folder the dialog chose.
    #[test]
    fn a_new_project_goes_where_the_dialog_says() {
        let dir = temp_dir("new-project");
        let mut app = open(&[]);
        let place = dir.join("shop");
        let _ = app.update(Message::NewProjectAnswered(
            omaspec_core::Language::PlantUml,
            false,
            chosen(place.clone()),
        ));
        assert!(
            place.join(omaspec_core::project::FILE).exists(),
            "{:?}",
            app.notice
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
