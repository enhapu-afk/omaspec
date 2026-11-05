//! The state machines of a project folder (in diagram files and in
//! Markdown documents), which sequence diagrams are checked against
//! (`omaspec_model::Spec`), and all its diagram files, which
//! maps show and states link to (`omaspec_render::Atlas`). The folder is
//! scanned as the file picker scans it; a file larger than 1 MB is not
//! read, as for `!include`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use omaspec_core::Language;
use omaspec_model::{Spec, Specs};
use omaspec_render::anchors::Anchor;
use omaspec_render::{Atlas, Place, RenderError, Rendered, Renderer};

use crate::editor::folder::scan_for;

/// A recorded trace may hold at most this many bytes, as the command line
/// reads it.
const MAX_TRACE: u64 = 16 << 20;

/// The state machines in the project folder `root`, by their paths in it.
pub fn read(root: &Path) -> Specs {
    read_project(root).0
}

/// The state machines in the project folder `root`, and all its diagrams
/// with the main one its project file names; each file read once
/// ([`crate::index::Index`] keeps them up to date).
pub fn read_project(root: &Path) -> (Specs, Atlas) {
    crate::index::Index::read(root).into_parts()
}

/// The path of the file at `path` in the project at `root`, as maps show
/// it; `None` outside the project.
pub fn path_in(root: &Path, path: &Path) -> Option<String> {
    let path = std::path::absolute(path).ok()?;
    path.strip_prefix(root).ok().map(file_name)
}

/// The projects' diagrams that maps show (`docs/adr/0018`), each project
/// read once: `omaspec render` and the MCP server keep one for all their
/// diagrams.
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

    /// Maps that know the project at `root` already.
    pub fn with(root: PathBuf, atlas: Arc<Atlas>) -> Maps {
        Maps(HashMap::from([(root, atlas)]))
    }

    /// The project a diagram links into, with its own path there: a map
    /// always ([`Self::of`]); anything else only in a project, where a
    /// state that names a diagram links to it.
    fn linked(
        &mut self,
        text: &str,
        language: Language,
        path: Option<&Path>,
        place: Option<&Place>,
    ) -> Option<(Arc<Atlas>, Option<String>)> {
        if omaspec_render::is_map(text, language) {
            return self.of(text, language, path, place);
        }
        let place = place?;
        let root = omaspec_render::project_root(&place.dir).filter(|root| *root == place.root)?;
        let atlas = self
            .0
            .entry(root.clone())
            .or_insert_with(|| Arc::new(read_project(&root).1));
        let own = path.and_then(|p| path_in(&root, p));
        Some((Arc::clone(atlas), own))
    }

    /// [`Self::render`], with each tile's diagram as `paint` writes it:
    /// a map in a look shows its tiles in that look.
    pub fn render_painted(
        &mut self,
        renderer: &mut Renderer,
        text: &str,
        language: Language,
        path: Option<&Path>,
        place: Option<&Place>,
        paint: &dyn Fn(&str, Language) -> String,
    ) -> Result<Rendered, RenderError> {
        match self.linked(text, language, path, place) {
            Some((atlas, own)) => {
                let mut painted = Atlas::clone(&atlas);
                for entry in &mut painted.diagrams {
                    entry.source = paint(&entry.source, entry.language);
                }
                renderer.render_linked(text, language, place, own.as_deref(), &painted)
            }
            None => renderer.render_at(text, language, place),
        }
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
        match self.linked(text, language, path, place) {
            Some((atlas, own)) => {
                renderer.render_linked(text, language, place, own.as_deref(), &atlas)
            }
            None => renderer.render_at(text, language, place),
        }
    }
}

/// The SVG of the diagram at `file`, a path in the project, beside it:
/// `orders/orders.puml` is drawn into `orders/orders.svg`.
pub fn svg_of(file: &str) -> String {
    let (folder, name) = file.rsplit_once('/').map_or(("", file), |(f, n)| (f, n));
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    if folder.is_empty() {
        format!("{stem}.svg")
    } else {
        format!("{folder}/{stem}.svg")
    }
}

/// The tiles and states of a drawn diagram as links between exports: each
/// that opens one diagram of the project links to that diagram's SVG
/// beside its file, relative to `own`, this diagram's path in the project
/// (as `omaspec render` writes them, and `omaspec export`).
pub fn export_links(links: &[omaspec_render::Link], own: &str) -> Vec<Anchor> {
    let from: Vec<&str> = own.split('/').collect();
    let dir = &from[..from.len().saturating_sub(1)];
    links
        .iter()
        .filter_map(|link| {
            let [target] = link.files.as_slice() else {
                return None;
            };
            let to: Vec<&str> = target.split('/').collect();
            let (folders, file) = to.split_at(to.len().checked_sub(1)?);
            let shared = dir.iter().zip(folders).take_while(|(a, b)| a == b).count();
            let mut parts: Vec<String> = vec!["..".to_owned(); dir.len() - shared];
            parts.extend(folders[shared..].iter().map(|f| (*f).to_owned()));
            parts.push(svg_of(file[0]));
            Some(Anchor {
                rect: link.rect,
                href: parts.join("/"),
                title: Some(link.name.replace('\n', " ")),
            })
        })
        .collect()
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
pub struct Projects {
    read: HashMap<PathBuf, Specs>,
    none: Specs,
}

impl Projects {
    /// The state machines `file` is checked against; none for standard
    /// input without a project.
    pub fn of(&mut self, file: Option<&Path>, project: Option<&Path>) -> &Specs {
        match root_of(file, project) {
            Some(root) => self.read.entry(root.clone()).or_insert_with(|| read(&root)),
            None => &self.none,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_between_exports_are_relative_to_the_diagram_s_folder() {
        let link = |name: &str, files: &[&str]| omaspec_render::Link {
            rect: omaspec_uml::graph::Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
            },
            name: name.to_owned(),
            files: files.iter().map(|f| (*f).to_owned()).collect(),
            tile: true,
        };
        let links = [
            link("Orders", &["orders/orders.puml"]),
            link("Shop", &["shop.mmd"]),
            link("Pay", &["orders/pay/pay.puml"]),
            // Two diagrams by one name: no link to choose.
            link("Twice", &["a.puml", "b.puml"]),
        ];
        let hrefs = |own: &str| -> Vec<String> {
            export_links(&links, own)
                .into_iter()
                .map(|a| a.href)
                .collect()
        };
        assert_eq!(
            hrefs("system.puml"),
            ["orders/orders.svg", "shop.svg", "orders/pay/pay.svg"]
        );
        assert_eq!(
            hrefs("orders/checkout.puml"),
            ["orders.svg", "../shop.svg", "pay/pay.svg"]
        );
        assert_eq!(hrefs("maps/deep/map.puml")[1], "../../shop.svg");
        assert_eq!(
            export_links(&links, "system.puml")[0].title.as_deref(),
            Some("Orders")
        );
    }

    #[test]
    fn a_document_s_state_machines_belong_to_its_project() {
        let root = std::env::temp_dir().join(format!("omaspec-specs-md-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(
            root.join("docs/README.md"),
            "# Door\n\n```plantuml\n@startuml\ntitle Door\n[*] --> Closed\n@enduml\n```\n\n\
             ```plantuml\n[*] --> Untitled\n```\n",
        )
        .unwrap();
        std::fs::write(
            root.join("visit.puml"),
            "@startuml\nA -> Door : open\n@enduml\n",
        )
        .unwrap();
        let (specs, atlas) = read_project(&root);
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].names, ["Door"]);
        assert_eq!(specs[0].file, "docs/README.md");
        // Without a title or a name it is no one's machine.
        assert!(specs[1].names.is_empty());
        // A document is no tile of a map.
        assert_eq!(atlas.diagrams.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

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
