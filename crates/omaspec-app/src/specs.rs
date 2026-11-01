//! The state machines of a project folder, which sequence diagrams are
//! checked against (`omaspec_model::Spec`), and all its diagrams, which
//! maps show and states link to (`omaspec_render::Atlas`). The folder is
//! scanned as the file picker scans it; a file larger than 1 MB is not
//! read, as for `!include`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use omaspec_core::{Language, detect};
use omaspec_model::Spec;
use omaspec_render::{Atlas, Entry, Place, RenderError, Rendered, Renderer};

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

/// The path of the file at `path` in the project at `root`, as maps show
/// it; `None` outside the project.
pub fn path_in(root: &Path, path: &Path) -> Option<String> {
    let path = std::path::absolute(path).ok()?;
    path.strip_prefix(root).ok().map(file_name)
}

/// The projects' diagrams that maps show (`docs/adr/0018`), each project
/// read once: `omaspec render` and the MCP server keep one for all their
/// diagrams, the preview window reads afresh for every drawing.
#[derive(Default)]
pub struct Maps(HashMap<PathBuf, Arc<Atlas>>);

impl Maps {
    /// The project a diagram at `place` (its file at `path`) is the map of,
    /// with its own path there: `None` unless it is a mind map or a WBS
    /// with a place.
    pub fn of(
        &mut self,
        text: &str,
        language: Language,
        path: Option<&Path>,
        place: Option<&Place>,
    ) -> Option<(Arc<Atlas>, Option<String>)> {
        let place = place.filter(|_| omaspec_render::is_map(text, language))?;
        let atlas = self
            .0
            .entry(place.root.clone())
            .or_insert_with(|| Arc::new(read_project(&place.root).1));
        let own = path.and_then(|p| path_in(&place.root, p));
        Some((Arc::clone(atlas), own))
    }

    /// Draws a diagram at `place` as its project shows it: a map with its
    /// tiles, anything else as it is.
    pub fn render(
        &mut self,
        renderer: &mut Renderer,
        text: &str,
        language: Language,
        path: Option<&Path>,
        place: Option<&Place>,
    ) -> Result<Rendered, RenderError> {
        match self.of(text, language, path, place) {
            Some((atlas, own)) => {
                renderer.render_linked(text, language, place, own.as_deref(), &atlas)
            }
            None => renderer.render_at(text, language, place),
        }
    }
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

        // Every diagram is in the atlas, by its path in the project; a map
        // finds it there, anything else draws as it is.
        let atlas = read_project(&root).1;
        let files: Vec<&str> = atlas.diagrams.iter().map(|e| e.file.as_str()).collect();
        assert_eq!(files, ["lamp.mmd", "login.puml", "states/door.puml"]);
        let map_file = root.join("states/map.puml");
        let map = "@startwbs\n* door\n@endwbs\n";
        let place = Place::of(&map_file, Some(&root));
        let mut maps = Maps::default();
        let (found, own) = maps
            .of(map, Language::PlantUml, Some(&map_file), place.as_ref())
            .unwrap();
        assert_eq!(found.diagrams.len(), 3);
        assert_eq!(own.as_deref(), Some("states/map.puml"));
        let flow = "@startuml\nA -> B\n@enduml\n";
        assert!(
            maps.of(flow, Language::PlantUml, Some(&map_file), place.as_ref())
                .is_none()
        );
        assert!(maps.of(map, Language::PlantUml, None, None).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
