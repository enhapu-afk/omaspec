//! A project's index (ROADMAP, 0.8): its diagrams as maps show them and
//! states link to them (`omaspec_render::Atlas`), and its state machines,
//! found by name (`omaspec_model::Specs`), in diagram files and Markdown
//! documents. A refresh scans the folder as the file picker does and reads
//! again only the files whose size or time of change differ, so a project
//! of 2,000 diagrams is not read whole each time one of them is saved or
//! the window gets the focus. A file larger than 1 MB is not read, as for
//! `!include`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use omaspec_core::{Language, detect, markdown};
use omaspec_model::{Spec, Specs};
use omaspec_render::{Atlas, Entry, Place};

use crate::editor::folder::walk;
use crate::specs::file_name;

/// A file may hold at most this many bytes.
const MAX_FILE: u64 = 1 << 20;

/// What tells a changed file: its size and its time of change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

impl Stamp {
    fn of(path: &Path) -> Option<Stamp> {
        let meta = std::fs::metadata(path).ok()?;
        meta.is_file().then(|| Stamp {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

/// A file of the project as it was read.
struct Indexed {
    stamp: Stamp,
    /// Its state machines: a diagram's one, a document's any number.
    specs: Vec<Spec>,
    /// As a map shows it; `None` for a document, whose diagrams are no
    /// tiles, and for a file that could not be read.
    entry: Option<Entry>,
    /// A document's text, which a search goes through.
    document: Option<String>,
}

/// What an index holds at one moment, for a window to keep.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub specs: Arc<Specs>,
    pub atlas: Arc<Atlas>,
    /// The folders the scan looked into, the root first.
    pub dirs: Arc<Vec<PathBuf>>,
    /// The Markdown documents, by their paths in the project, with their
    /// texts; the diagrams' texts are the atlas's.
    pub documents: Arc<Vec<(String, String)>>,
}

/// A project folder's diagrams and state machines, kept up to date by
/// [`Index::refresh`].
pub struct Index {
    root: PathBuf,
    /// By their paths in the project, `/` between folders.
    files: BTreeMap<String, Indexed>,
    specs: Arc<Specs>,
    atlas: Arc<Atlas>,
    documents: Arc<Vec<(String, String)>>,
    /// The folders the last refresh looked into, the root first.
    dirs: Arc<Vec<PathBuf>>,
    /// Whether it was refreshed at all.
    read: bool,
}

impl Index {
    /// The index of the project at `root`, not read yet.
    pub fn new(root: &Path) -> Index {
        Index {
            root: root.to_path_buf(),
            files: BTreeMap::new(),
            specs: Arc::default(),
            atlas: Arc::default(),
            documents: Arc::default(),
            dirs: Arc::default(),
            read: false,
        }
    }

    /// The project at `root`, read whole.
    pub fn read(root: &Path) -> Index {
        let mut index = Index::new(root);
        index.refresh();
        index
    }

    /// Brings the index up to date with the folder; the files read again,
    /// new or gone, by their paths in the project.
    pub fn refresh(&mut self) -> Vec<String> {
        let (found, dirs) = walk(&self.root, |name| {
            Language::from_file_name(name).is_some() || markdown::is_markdown(name)
        });
        if *self.dirs != dirs {
            self.dirs = Arc::new(dirs);
        }
        let mut changed = Vec::new();
        let mut files = BTreeMap::new();
        for relative in found {
            let path = self.root.join(&relative);
            let name = file_name(&relative);
            let Some(stamp) = Stamp::of(&path).filter(|s| s.len <= MAX_FILE) else {
                continue;
            };
            let indexed = match self.files.remove(&name) {
                Some(kept) if kept.stamp == stamp => kept,
                _ => {
                    changed.push(name.clone());
                    read(&self.root, &path, &name, stamp)
                }
            };
            files.insert(name, indexed);
        }
        // What is left was not found again.
        changed.extend(std::mem::take(&mut self.files).into_keys());
        changed.sort();
        self.files = files;

        let project = crate::project::read(&self.root).and_then(Result::ok);
        let main = project
            .as_ref()
            .and_then(|p| p.main.as_deref())
            .map(|m| m.replace('\\', "/").trim_start_matches("./").to_owned());
        let name = project.and_then(|p| p.name);
        let heads = main != self.atlas.main || name != self.atlas.name;
        if changed.is_empty() && !heads && self.read {
            return changed;
        }
        self.read = true;
        if !changed.is_empty() || self.specs.is_empty() {
            self.specs = Arc::new(
                self.files
                    .values()
                    .flat_map(|f| f.specs.iter().cloned())
                    .collect(),
            );
        }
        self.atlas = Arc::new(Atlas {
            diagrams: self
                .files
                .values()
                .filter_map(|f| f.entry.clone())
                .collect(),
            main,
            name,
        });
        let documents: Vec<(String, String)> = self
            .files
            .iter()
            .filter_map(|(path, f)| Some((path.clone(), f.document.clone()?)))
            .collect();
        if *self.documents != documents {
            self.documents = Arc::new(documents);
        }
        changed
    }

    /// The machines, diagrams, folders and documents as they are now.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            specs: self.specs(),
            atlas: self.atlas(),
            dirs: self.dirs(),
            documents: Arc::clone(&self.documents),
        }
    }

    /// The project's folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The project's state machines, the same until one changes.
    pub fn specs(&self) -> Arc<Specs> {
        Arc::clone(&self.specs)
    }

    /// The project's diagrams, the same until one changes.
    pub fn atlas(&self) -> Arc<Atlas> {
        Arc::clone(&self.atlas)
    }

    /// The folders of the project that can hold its diagrams, the root
    /// first: those a watch looks into.
    pub fn dirs(&self) -> Arc<Vec<PathBuf>> {
        Arc::clone(&self.dirs)
    }

    /// Its state machines and diagrams, for one reading.
    pub fn into_parts(self) -> (Specs, Atlas) {
        (
            Arc::unwrap_or_clone(self.specs),
            Arc::unwrap_or_clone(self.atlas),
        )
    }
}

/// The file at `path`, shown as `name` in the project at `root`.
fn read(root: &Path, path: &Path, name: &str, stamp: Stamp) -> Indexed {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Indexed {
            stamp,
            specs: Vec::new(),
            entry: None,
            document: None,
        };
    };
    // A document's state machines count; its diagrams are no tiles.
    if markdown::is_markdown(name) {
        return Indexed {
            stamp,
            specs: markdown::blocks(&text)
                .iter()
                .filter_map(|b| Spec::read_in_document(&b.code, b.language, name))
                .collect(),
            entry: None,
            document: Some(text),
        };
    }
    let language = detect(&text, Language::from_file_name(name));
    Indexed {
        stamp,
        specs: Spec::read(&text, language, name).into_iter().collect(),
        entry: Some(Entry::new(
            name,
            text,
            language,
            Place::of(path, Some(root)),
        )),
        document: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omaspec-index-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(dir).unwrap()
    }

    const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\n@enduml\n";

    #[test]
    fn a_refresh_reads_only_what_changed() {
        let root = temp_dir("refresh");
        let write = |file: &str, text: &str| {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "omaspec-project.toml",
            "name = \"house\"\nmain = \"house.puml\"\n",
        );
        write("house.puml", "@startwbs\n* House\n** Door\n@endwbs\n");
        write("rooms/door.puml", DOOR);
        write(
            "notes.md",
            &format!(
                "# Lamp\n\n```plantuml\n{}```\n",
                DOOR.replace("Door", "Lamp")
            ),
        );
        let mut index = Index::new(&root);
        assert_eq!(
            index.refresh(),
            ["house.puml", "notes.md", "rooms/door.puml"]
        );
        let (specs, atlas) = (index.specs(), index.atlas());
        let names: Vec<&str> = specs.iter().map(|s| s.file.as_str()).collect();
        assert_eq!(names, ["notes.md", "rooms/door.puml"]);
        let files: Vec<&str> = atlas.diagrams.iter().map(|e| e.file.as_str()).collect();
        assert_eq!(files, ["house.puml", "rooms/door.puml"]);
        assert_eq!(atlas.main.as_deref(), Some("house.puml"));
        assert_eq!(*index.dirs(), [root.clone(), root.join("rooms")]);
        assert_eq!(atlas.name.as_deref(), Some("house"));

        // Nothing changed: nothing is read, and the same machines and
        // diagrams come back.
        assert!(index.refresh().is_empty());
        assert!(Arc::ptr_eq(&specs, &index.specs()));
        assert!(Arc::ptr_eq(&atlas, &index.atlas()));

        // One file changed, one new, one gone.
        write(
            "rooms/door.puml",
            &DOOR.replace("open\n", "open\nOpen --> Closed : close\n"),
        );
        write("rooms/window.puml", &DOOR.replace("Door", "Window"));
        std::fs::remove_file(root.join("notes.md")).unwrap();
        assert_eq!(
            index.refresh(),
            ["notes.md", "rooms/door.puml", "rooms/window.puml"]
        );
        let specs = index.specs();
        assert_eq!(specs.named(&["Door"])[0].machine.transitions.len(), 3);
        assert_eq!(index.specs().len(), 2);
        assert_eq!(index.atlas().diagrams.len(), 3);

        // The project file names another main diagram: the same machines.
        let specs = index.specs();
        write(
            "omaspec-project.toml",
            "name = \"home\"\nmain = \"house.puml\"\n",
        );
        assert!(index.refresh().is_empty());
        assert_eq!(index.atlas().name.as_deref(), Some("home"));
        assert!(Arc::ptr_eq(&specs, &index.specs()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
