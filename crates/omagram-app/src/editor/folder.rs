//! The folder the editor works in: the diagrams in it, and fuzzy matching
//! for the file picker.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::Task;
use omagram_core::{DiagramKind, Language};
use omagram_model::Spec;

use super::doc::Doc;
use super::{App, Message};
use crate::specs;
use crate::worker::blocking;

/// The scan stops after this many diagrams.
pub const MAX_FILES: usize = 5000;
/// … and does not go deeper than this.
const MAX_DEPTH: usize = 8;
/// Folders that hold build output or dependencies, not diagrams.
const SKIPPED: &[&str] = &["node_modules", "target", "__pycache__", "venv"];
/// The sidebar shows at most this many children of one folder.
pub const MAX_LISTED: usize = 1000;
/// It tells the kind of this many diagrams of a folder, from the start of
/// each file.
const MAX_KINDS: usize = 200;
const KIND_BYTES: u64 = 4096;

/// One child of a folder, for the sidebar tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    pub name: String,
    pub dir: bool,
    /// A diagram's kind, read from the start of the file.
    pub kind: Option<DiagramKind>,
}

/// The children of one folder the sidebar shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    pub children: Vec<Child>,
    /// How many children were left out after [`MAX_LISTED`].
    pub more: usize,
}

/// The children of `dir`, one level: directories first, then files.
/// Hidden names and dependency folders are left out. Links are not followed.
/// A crowded folder drops files that are not diagrams first.
pub fn list(dir: &Path) -> Listing {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Listing::default();
    };
    let mut children = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || SKIPPED.contains(&name.as_ref()) {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            children.push(Child {
                name: name.into_owned(),
                dir: true,
                kind: None,
            });
        } else if kind.is_file() {
            children.push(Child {
                name: name.into_owned(),
                dir: false,
                kind: None,
            });
        }
    }
    let mut more = 0;
    if children.len() > MAX_LISTED {
        children.sort_by_key(|child| !child.dir && Language::from_file_name(&child.name).is_none());
        more = children.len() - MAX_LISTED;
        children.truncate(MAX_LISTED);
    }
    children.sort_by_cached_key(|child| (!child.dir, child.name.to_lowercase()));
    for child in children.iter_mut().filter(|c| !c.dir).take(MAX_KINDS) {
        child.kind = kind_of(&dir.join(&child.name), &child.name);
    }
    Listing { children, more }
}

/// The kind of the diagram in `path`, from its first few kilobytes.
fn kind_of(path: &Path, name: &str) -> Option<DiagramKind> {
    let language = Language::from_file_name(name)?;
    let mut head = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(KIND_BYTES)
        .read_to_end(&mut head)
        .ok()?;
    Some(omagram_core::classify(
        &String::from_utf8_lossy(&head),
        language,
    ))
}

/// The diagram files under `dir` (by extension), relative to it and
/// sorted. Hidden entries and dependency folders are skipped. The file
/// picker uses this. The sidebar lists [`list`] instead, one level at a time.
pub fn scan(dir: &Path) -> Vec<PathBuf> {
    scan_for(dir, |name| Language::from_file_name(name).is_some())
}

/// The files under `dir` whose names `wanted` keeps, as [`scan`] finds
/// diagrams.
pub fn scan_for(dir: &Path, wanted: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![(dir.to_path_buf(), 0)];
    while let Some((current, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            // Not following links keeps the scan inside the folder.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                if depth < MAX_DEPTH && !SKIPPED.contains(&name.as_ref()) {
                    pending.push((path, depth + 1));
                }
            } else if kind.is_file() && wanted(&name) {
                if let Ok(relative) = path.strip_prefix(dir) {
                    found.push(relative.to_path_buf());
                }
                if found.len() >= MAX_FILES {
                    found.sort();
                    return found;
                }
            }
        }
    }
    found.sort();
    found
}

/// How well `query` matches `candidate`, or `None` if its letters do not
/// all appear in order (ignoring case). Matches at word starts, in a row
/// and in the file name count more; shorter candidates win ties.
pub fn score(query: &str, candidate: &str) -> Option<i32> {
    let found = matched(query, candidate)?;
    let chars: Vec<char> = candidate.chars().collect();
    let name_start = candidate
        .rfind('/')
        .map_or(0, |i| candidate[..=i].chars().count());
    let mut total = 0;
    let mut previous: Option<usize> = None;
    for at in found {
        let mut points = 10;
        if at == 0 || matches!(chars[at - 1], '/' | '-' | '_' | '.' | ' ') {
            points += 15;
        }
        if previous == Some(at.wrapping_sub(1)) {
            points += 10;
        }
        if at >= name_start {
            points += 20;
        }
        total += points;
        previous = Some(at);
    }
    Some(total - chars.len() as i32 / 4)
}

/// Where the letters of `query` are in `candidate` (character indices),
/// each the first after the one before, ignoring case; `None` if not all
/// are. The picker lights them up.
pub fn matched(query: &str, candidate: &str) -> Option<Vec<usize>> {
    let chars: Vec<char> = candidate.chars().collect();
    let mut found = Vec::new();
    let mut next = 0;
    for wanted in query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
    {
        let at = (next..chars.len()).find(|&i| chars[i].to_lowercase().eq([wanted]))?;
        found.push(at);
        next = at + 1;
    }
    Some(found)
}

impl App {
    /// Makes `folder` the folder, lists its first level, and looks for
    /// diagrams in it for the file picker.
    pub(super) fn set_folder(&mut self, folder: Option<PathBuf>) -> Task<Message> {
        self.help = None;
        if self.folder != folder {
            self.files.clear();
            self.open_dirs.clear();
            self.tree.clear();
        }
        self.folder = folder;
        self.fill_tree();
        Task::batch([self.rescan(), self.load_specs()])
    }

    /// Lists the project root and every folder the tree has open.
    pub(super) fn fill_tree(&mut self) {
        let Some(root) = self.folder.clone() else {
            self.tree.clear();
            self.open_dirs.clear();
            return;
        };
        self.open_dirs.retain(|rel| root.join(rel).is_dir());
        let mut tree = HashMap::new();
        tree.insert(PathBuf::new(), list(&root));
        for rel in &self.open_dirs {
            tree.insert(rel.clone(), list(&root.join(rel)));
        }
        self.tree = tree;
    }

    /// Opens `dir` (relative to the project) or closes it and everything
    /// under it.
    pub(super) fn toggle_dir(&mut self, dir: PathBuf) {
        if dir.as_os_str().is_empty() {
            return;
        }
        if self.open_dirs.remove(&dir) {
            self.open_dirs.retain(|open| !open.starts_with(&dir));
        } else {
            self.open_dirs.insert(dir);
        }
        self.fill_tree();
    }

    /// Looks for diagrams in the folder, in the background.
    /// The project folder of an open document: as for `!include`, the
    /// sidebar's folder when the file is in it, else the file's own folder.
    /// An unsaved document belongs to the sidebar's folder.
    pub(super) fn project_of(&self, doc: &Doc) -> Option<PathBuf> {
        match &doc.path {
            Some(path) => omagram_render::Place::of(path, self.folder.as_deref()).map(|p| p.root),
            None => self.folder.clone(),
        }
    }

    /// Reads the state machines of every project folder an open document
    /// is in, in the background.
    pub(super) fn load_specs(&self) -> Task<Message> {
        let mut roots: Vec<PathBuf> = self
            .docs
            .iter()
            .filter_map(|d| self.project_of(d))
            .collect();
        roots.extend(self.folder.clone());
        roots.sort();
        roots.dedup();
        Task::batch(roots.into_iter().map(|root| {
            Task::perform(
                blocking(move || {
                    let specs = specs::read(&root);
                    (root, specs)
                }),
                |found| match found {
                    Some((root, specs)) => Message::Specs(root, Arc::new(specs)),
                    None => Message::Specs(PathBuf::new(), Arc::default()),
                },
            )
        }))
    }

    /// Takes the state machines read for `root`, and checks each document
    /// in it again whose machines changed.
    pub(super) fn found_specs(&mut self, root: PathBuf, specs: Arc<Vec<Spec>>) {
        if root.as_os_str().is_empty() {
            return;
        }
        for index in 0..self.docs.len() {
            if self.project_of(&self.docs[index]).as_ref() == Some(&root)
                && *self.docs[index].specs != *specs
            {
                let doc = &mut self.docs[index];
                doc.specs = specs.clone();
                doc.analyze();
            }
        }
        self.specs.insert(root, specs);
    }

    pub(super) fn rescan(&self) -> Task<Message> {
        let Some(folder) = self.folder.clone() else {
            return Task::none();
        };
        Task::perform(
            blocking(move || {
                let files = scan(&folder);
                (folder, files)
            }),
            |found| match found {
                Some((folder, files)) => Message::Scanned(folder, files),
                None => Message::Scanned(PathBuf::new(), Vec::new()),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_diagrams_but_not_hidden_or_dependency_folders() {
        let dir = std::env::temp_dir().join(format!("omagram-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for file in [
            "a.puml",
            "docs/flow.mmd",
            "docs/deep/state.wsd",
            "notes.md",
            ".git/x.puml",
            ".hidden.puml",
            "node_modules/pkg/b.mmd",
            "target/c.puml",
        ] {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        let found = scan(&dir);
        let found: Vec<&str> = found.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(found, ["a.puml", "docs/deep/state.wsd", "docs/flow.mmd"]);
        let listed = list(&dir);
        let names: Vec<(&str, bool)> = listed
            .children
            .iter()
            .map(|c| (c.name.as_str(), c.dir))
            .collect();
        assert_eq!(
            names,
            [("docs", true), ("a.puml", false), ("notes.md", false)]
        );
        let nested_dir = list(&dir.join("docs"));
        let nested: Vec<&str> = nested_dir
            .children
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(nested, ["deep", "flow.mmd"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_crowded_folder_lists_its_folders_and_diagrams() {
        let dir = std::env::temp_dir().join(format!("omagram-crowded-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for i in 0..MAX_LISTED + 10 {
            std::fs::write(dir.join(format!("{i:05}.png")), "").unwrap();
        }
        std::fs::write(dir.join("zz.puml"), "").unwrap();
        let listed = list(&dir);
        assert_eq!(listed.children.len(), MAX_LISTED);
        assert_eq!(listed.more, 12);
        assert_eq!(listed.children[0].name, "sub");
        assert_eq!(listed.children.last().unwrap().name, "zz.puml");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn letters_in_order_match() {
        assert!(score("flw", "docs/flow.mmd").is_some());
        assert!(score("FLOW", "docs/flow.mmd").is_some());
        assert!(score("wolf", "docs/flow.mmd").is_none());
        assert!(score("", "anything").is_some());
    }

    fn rank<'a>(query: &str, candidates: &[&'a str]) -> Vec<&'a str> {
        let mut scored: Vec<(i32, &str)> = candidates
            .iter()
            .filter_map(|c| Some((score(query, c)?, *c)))
            .collect();
        scored.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
        scored.into_iter().map(|(_, c)| c).collect()
    }

    #[test]
    fn file_names_and_word_starts_rank_first() {
        assert_eq!(
            rank("login", &["logs/in.puml", "auth/login.puml"]),
            ["auth/login.puml", "logs/in.puml"]
        );
        assert_eq!(
            rank("seq", &["release/queue.mmd", "sequence.puml"]),
            ["sequence.puml", "release/queue.mmd"]
        );
        assert_eq!(
            rank("st", &["a/b/c/state.puml", "state.puml"])[0],
            "state.puml"
        );
    }
}
