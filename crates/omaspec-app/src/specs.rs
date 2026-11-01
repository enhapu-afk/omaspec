//! The state machines of a project folder, which sequence diagrams are
//! checked against (`omaspec_model::Spec`), and all its diagrams, which
//! maps show and states link to (`omaspec_render::Atlas`). The folder is
//! scanned as the file picker scans it; a file larger than 1 MB is not
//! read, as for `!include`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use omaspec_core::{Language, detect};
use omaspec_model::Spec;
use omaspec_render::{Atlas, Entry, Place};

use crate::editor::folder::{scan, scan_for};

/// A file may hold at most this many bytes.
const MAX_FILE: u64 = 1 << 20;
/// A recorded trace may hold at most this many bytes, as the command line
/// reads it.
const MAX_TRACE: u64 = 16 << 20;

/// The state machines in the project folder `root`, by their paths in it.
pub fn read(root: &Path) -> Vec<Spec> {
    read_project(root).0
}

/// The state machines in the project folder `root`, and all its diagrams
/// with the main one its project file names; each file read once.
pub fn read_project(root: &Path) -> (Vec<Spec>, Atlas) {
    let mut specs = Vec::new();
    let mut atlas = Atlas {
        main: crate::project::read(root)
            .and_then(Result::ok)
            .and_then(|p| p.main)
            .map(|m| m.replace('\\', "/").trim_start_matches("./").to_owned()),
        ..Atlas::default()
    };
    for relative in scan(root) {
        let path = root.join(&relative);
        let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_FILE);
        let Some(text) = small.then(|| std::fs::read_to_string(&path).ok()).flatten() else {
            continue;
        };
        let name = file_name(&relative);
        let language = detect(&text, Language::from_file_name(&name));
        specs.extend(Spec::read(&text, language, &name));
        atlas.diagrams.push(Entry {
            names: omaspec_core::project::names(&text, language, &name),
            place: Place::of(&path, Some(root)),
            file: name,
            source: text,
            language,
        });
    }
    atlas.diagrams.sort_by(|a, b| a.file.cmp(&b.file));
    (specs, atlas)
}

/// A path in the project as maps and messages show it: `/` between
/// folders.
pub fn file_name(relative: &Path) -> String {
    relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The recorded traces in the project folder `root` (JSON Lines, `.jsonl`)
/// with objects that go by `spec`'s machine: for each such object, a
/// label, the trace and the object.
pub fn traces(root: &Path, spec: &Spec) -> Vec<(String, PathBuf, String)> {
    let mut out = Vec::new();
    for relative in scan_for(root, |name| name.ends_with(".jsonl")) {
        let path = root.join(&relative);
        let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_TRACE);
        let Some(text) = small.then(|| std::fs::read_to_string(&path).ok()).flatten() else {
            continue;
        };
        for object in omaspec_model::trace_objects(&text, spec) {
            let label = format!("{object} in {}", relative.display());
            out.push((label, path.clone(), object));
        }
    }
    out
}

/// The project folder of a diagram file: `project` if one is given, else
/// the project the file is in (`omaspec-project.toml` above it), else the
/// file's own folder.
pub fn root_of(file: Option<&Path>, project: Option<&Path>) -> Option<PathBuf> {
    if let Some(project) = project {
        return std::path::absolute(project).ok();
    }
    omaspec_render::Place::of(file?, None).map(|place| place.root)
}

/// The state machines of every project folder one command needs, each
/// folder read once.
#[derive(Default)]
pub struct Projects(HashMap<PathBuf, Vec<Spec>>);

impl Projects {
    /// The state machines `file` is checked against; none for standard
    /// input without a project.
    pub fn of(&mut self, file: Option<&Path>, project: Option<&Path>) -> &[Spec] {
        match root_of(file, project) {
            Some(root) => self.0.entry(root.clone()).or_insert_with(|| read(&root)),
            None => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_state_machines_of_a_folder() {
        let root = std::env::temp_dir().join(format!("omaspec-specs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("states")).unwrap();
        std::fs::write(
            root.join("states/door.puml"),
            "@startuml\n[*] --> Closed\nClosed --> Open : open\n@enduml\n",
        )
        .unwrap();
        std::fs::write(root.join("login.puml"), "@startuml\nA -> B : hi\n@enduml\n").unwrap();
        std::fs::write(root.join("lamp.mmd"), "stateDiagram-v2\n[*] --> Off\n").unwrap();
        let specs = read(&root);
        let mut files: Vec<&str> = specs.iter().map(|s| s.file.as_str()).collect();
        files.sort();
        assert_eq!(files, ["lamp.mmd", "states/door.puml"]);

        let mut projects = Projects::default();
        assert_eq!(projects.of(Some(&root.join("login.puml")), None).len(), 2);
        assert!(projects.of(None, None).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
