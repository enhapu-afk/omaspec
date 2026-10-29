//! The session: open tabs with their unsaved text, the folder, the active
//! tab. Kept in `$XDG_STATE_HOME/omaspec/session.json` (readable by the
//! user only, it may hold unsaved diagrams), so closing the window loses
//! nothing and the next start picks up where this one stopped.
//!
//! One window owns the session at a time; a second one works without.

use std::fs::{self, File, TryLockError};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use iced::Task;
use omaspec_core::Language;
use serde::{Deserialize, Serialize};

use super::doc::Doc;
use super::{App, Message, SESSION_PAUSE};
use crate::worker::Alarm;

const FILE: &str = "session.json";
const LOCK: &str = "session.lock";
/// A session file larger than this is not read.
const MAX_SIZE: u64 = 64 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    #[serde(default)]
    pub folder: Option<PathBuf>,
    #[serde(default)]
    pub active: usize,
    #[serde(default = "shown")]
    pub sidebar: bool,
    /// The diagram takes the active Omarchy colors when drawn.
    #[serde(default)]
    pub follow_theme: bool,
    /// The file picker has been used once.
    #[serde(default)]
    pub picked: bool,
    #[serde(default)]
    pub docs: Vec<SavedDoc>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            folder: None,
            active: 0,
            sidebar: true,
            follow_theme: false,
            picked: false,
            docs: Vec::new(),
        }
    }
}

fn shown() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedDoc {
    /// The file; `None` for a tab never saved.
    pub path: Option<PathBuf>,
    /// The text, when it differs from the file (or there is none).
    pub text: Option<String>,
    /// The language set by hand (`plantuml`, `mermaid`).
    #[serde(default)]
    pub language: Option<String>,
    /// The caret, zero-based.
    #[serde(default)]
    pub line: usize,
    #[serde(default)]
    pub column: usize,
}

/// Where the session lives, held by this window.
pub struct Store {
    dir: PathBuf,
    /// Locked while this window runs; closing the file unlocks it.
    _lock: File,
}

impl Store {
    /// The user's session store, or `None` if another window holds it.
    pub fn open() -> Result<Option<Self>> {
        let dir = state_dir().context("cannot locate the state directory (HOME is not set)")?;
        Self::at(&dir)
    }

    /// The store in `dir`, or `None` if another window holds it.
    pub fn at(dir: &Path) -> Result<Option<Self>> {
        fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
        // Only the user may look inside, even while a new file is written.
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
            .with_context(|| dir.display().to_string())?;
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(LOCK))
            .with_context(|| dir.join(LOCK).display().to_string())?;
        match lock.try_lock() {
            Ok(()) => Ok(Some(Self {
                dir: dir.to_owned(),
                _lock: lock,
            })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(err)) => {
                Err(err).with_context(|| format!("locking {}", dir.join(LOCK).display()))
            }
        }
    }

    /// The saved session; `None` if there is none or it cannot be read.
    pub fn load(&self) -> Option<Session> {
        let path = self.dir.join(FILE);
        if fs::metadata(&path).ok()?.len() > MAX_SIZE {
            return None;
        }
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    }

    pub fn save(&self, session: &Session) -> Result<()> {
        let path = self.dir.join(FILE);
        let json = serde_json::to_string_pretty(session)?;
        crate::files::write_atomic(&path, json.as_bytes())?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .with_context(|| path.display().to_string())
    }
}

/// `$XDG_STATE_HOME/omaspec`, by default `~/.local/state/omaspec`.
fn state_dir() -> Option<PathBuf> {
    let state = match std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".local/state"),
    };
    Some(state.join("omaspec"))
}

impl App {
    /// A tab from the session. A file that exists and cannot be read
    /// still comes back from its unsaved text, and the error is shown.
    /// `None` when the file is gone and the tab held nothing unsaved, or
    /// when the file cannot be read and no text was stored.
    pub(super) fn restore(&mut self, saved: &SavedDoc) -> Option<Doc> {
        let forced = saved
            .language
            .as_deref()
            .and_then(Language::from_id)
            .or(self.forced);
        let key = self.take_key();
        let mut doc = match (&saved.path, &saved.text) {
            (Some(path), text) if path.exists() || text.is_some() => {
                match Doc::open(key, path, forced, &self.look) {
                    Ok(doc) => doc,
                    Err(err) if text.is_some() => {
                        self.notice = Some(format!("cannot read {}: {err:#}", path.display()));
                        let mut doc = Doc::unsaved_as(key, path.clone(), "", &self.look);
                        if forced.is_some() {
                            doc.set_forced(forced);
                        }
                        doc
                    }
                    Err(_) => return None,
                }
            }
            (None, Some(_)) => Doc::untitled(key, forced, &self.look),
            _ => return None,
        };
        if let Some(text) = &saved.text {
            doc.restore_text(text);
        }
        doc.place_caret(saved.line, saved.column);
        Some(doc)
    }

    /// Ends the program. The session keeps unsaved changes; without one
    /// (another window has it) they are dropped only on the second ask.
    pub(super) fn quit(&mut self) -> Task<Message> {
        if self.store.is_some() {
            match self.save_session() {
                Ok(()) => return iced::exit(),
                Err(err) => self.notice = Some(format!("session not saved: {err:#}")),
            }
        }
        let unsaved: Vec<String> = self
            .docs
            .iter()
            .filter(|d| d.modified)
            .map(Doc::name)
            .collect();
        if unsaved.is_empty() || self.leaving {
            return iced::exit();
        }
        self.leaving = true;
        self.notice = Some(format!(
            "unsaved changes in {}: Ctrl+S saves, closing again discards them",
            unsaved.join(", ")
        ));
        Task::none()
    }

    /// The session as it stands.
    pub(super) fn session(&self) -> Session {
        Session {
            // Help Omaspec is a visit; the project stays the folder.
            folder: match &self.help {
                Some(help) => help.back.clone(),
                None => self.folder.clone(),
            },
            active: self.active,
            sidebar: self.sidebar_open(),
            follow_theme: self.follow_theme,
            picked: self.picked,
            docs: self
                .docs
                .iter()
                .map(|doc| {
                    let caret = doc.editor.cursor();
                    SavedDoc {
                        path: doc.path.clone(),
                        text: doc.unsaved_text(),
                        language: doc.forced.map(|l| l.id().to_owned()),
                        line: caret.row as usize,
                        column: caret.col as usize,
                    }
                })
                .collect(),
        }
    }

    pub(super) fn save_session(&self) -> anyhow::Result<()> {
        match &self.store {
            Some(store) => store.save(&self.session()),
            None => Ok(()),
        }
    }

    /// Writes the session after a pause.
    pub(super) fn keep_session(&self) {
        if self.store.is_some() {
            self.after(Alarm::Session, SESSION_PAUSE);
        }
    }

    pub(super) fn write_session(&mut self) {
        if let Err(err) = self.save_session() {
            self.notice = Some(format!("session not saved: {err:#}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("omaspec-session-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn one_window_holds_the_session() {
        let dir = temp_dir("lock");
        let first = Store::at(&dir).unwrap().expect("free");
        assert!(Store::at(&dir).unwrap().is_none());
        drop(first);
        assert!(Store::at(&dir).unwrap().is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_and_loads_privately() {
        let dir = temp_dir("save");
        let store = Store::at(&dir).unwrap().unwrap();
        assert_eq!(store.load(), None);
        let session = Session {
            folder: Some("/work".into()),
            active: 1,
            sidebar: false,
            follow_theme: true,
            picked: true,
            docs: vec![SavedDoc {
                path: None,
                text: Some("@startuml\n@enduml\n".into()),
                language: Some("plantuml".into()),
                line: 1,
                column: 0,
            }],
        };
        store.save(&session).unwrap();
        assert_eq!(store.load(), Some(session));
        let mode = fs::metadata(dir.join(FILE)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);

        // A broken file counts as no session; missing fields take defaults.
        fs::write(dir.join(FILE), "{ not json").unwrap();
        assert_eq!(store.load(), None);
        fs::write(dir.join(FILE), "{}").unwrap();
        let loaded = store.load().unwrap();
        assert!(loaded.sidebar);
        assert!(!loaded.follow_theme);
        let _ = fs::remove_dir_all(&dir);
    }
}
