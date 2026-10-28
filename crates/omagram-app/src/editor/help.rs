//! Diagrams that describe Omagram. The copies in `examples/Help Omagram`
//! are the source. The running program writes the same text to
//! `$XDG_DATA_HOME/omagram/Help Omagram` (otherwise
//! `~/.local/share/omagram/Help Omagram`) when that copy differs, so
//! the installed binary can open them without the repository.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use iced::Task;

use super::{App, Help, Message};

pub const DIR_NAME: &str = "Help Omagram";

struct Page {
    name: &'static str,
    text: &'static str,
}

const PAGES: &[Page] = &[
    Page {
        name: "menu.puml",
        text: include_str!("../../../../examples/Help Omagram/menu.puml"),
    },
    Page {
        name: "editing.puml",
        text: include_str!("../../../../examples/Help Omagram/editing.puml"),
    },
    Page {
        name: "windows.puml",
        text: include_str!("../../../../examples/Help Omagram/windows.puml"),
    },
    Page {
        name: "export.puml",
        text: include_str!("../../../../examples/Help Omagram/export.puml"),
    },
];

/// The help folder for this user, creating it and refreshing pages whose
/// text differs from the copy in the program.
pub fn materialize() -> Result<PathBuf> {
    let dir = data_dir()
        .context("cannot locate the data directory (HOME is not set)")?
        .join("omagram")
        .join(DIR_NAME);
    write_pages(&dir)?;
    Ok(dir)
}

/// Whether `dir` is a help folder that [`materialize`] wrote: a session
/// from before leaving Help Omagram was possible may still name it.
pub fn is_help_dir(dir: &Path) -> bool {
    dir.ends_with(Path::new("omagram").join(DIR_NAME))
}

/// The diagram the help folder opens on.
pub fn menu_file(dir: &Path) -> PathBuf {
    dir.join("menu.puml")
}

/// Writes every page into `dir` when the file is missing or its text differs.
pub fn write_pages(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
    for page in PAGES {
        let path = dir.join(page.name);
        if fs::read_to_string(&path).ok().as_deref() == Some(page.text) {
            continue;
        }
        crate::files::write_atomic(&path, page.text.as_bytes())?;
    }
    Ok(())
}

/// `$XDG_DATA_HOME`, or `~/.local/share`.
fn data_dir() -> Option<PathBuf> {
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => Some(PathBuf::from(std::env::var_os("HOME")?).join(".local/share")),
    }
}

impl App {
    /// Opens the Help Omagram folder and the diagram of the command menu.
    pub(super) fn open_help(&mut self) -> Task<Message> {
        match materialize() {
            Ok(dir) => self.show_help(dir),
            Err(err) => {
                self.notice = Some(format!("cannot open Help Omagram: {err:#}"));
                Task::none()
            }
        }
    }

    /// Shows the help folder `dir` in the sidebar and its menu in a tab.
    /// The project folder comes back with [`leave_help`](Self::leave_help).
    pub(super) fn show_help(&mut self, dir: PathBuf) -> Task<Message> {
        let back = match self.help.take() {
            Some(help) => help.back,
            None => self.folder.clone(),
        };
        let menu = menu_file(&dir);
        let scan = self.set_folder(Some(dir.clone()));
        self.help = Some(Help { dir, back });
        self.keep_session();
        Task::batch([scan, self.open(&menu)])
    }

    /// Back to the folder before Help Omagram. Help pages without
    /// changes close.
    pub(super) fn leave_help(&mut self) -> Task<Message> {
        let Some(help) = self.help.take() else {
            return Task::none();
        };
        let mut tasks = Vec::new();
        while let Some(index) = self.docs.iter().position(|doc| {
            !doc.modified
                && doc
                    .path
                    .as_deref()
                    .is_some_and(|p| p.starts_with(&help.dir))
        }) {
            tasks.push(self.close(index));
        }
        tasks.push(self.set_folder(help.back));
        self.keep_session();
        Task::batch(tasks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_land_in_the_folder_and_a_second_write_leaves_them() {
        let dir = std::env::temp_dir().join(format!("omagram-help-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_pages(&dir).unwrap();
        for page in PAGES {
            assert_eq!(
                fs::read_to_string(dir.join(page.name)).unwrap(),
                page.text,
                "{}",
                page.name
            );
        }
        let menu = dir.join("menu.puml");
        let before = fs::metadata(&menu).unwrap().modified().unwrap();
        write_pages(&dir).unwrap();
        assert_eq!(fs::metadata(&menu).unwrap().modified().unwrap(), before);
        let _ = fs::remove_dir_all(&dir);
    }
}
