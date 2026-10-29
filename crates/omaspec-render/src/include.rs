//! The files `!include` reads: local ones inside the project folder.
//!
//! A diagram from elsewhere must not pull private files into a picture that
//! is shared later. So an included file has to lie inside the project
//! folder once links are followed, outside hidden folders such as `.ssh`,
//! be a regular file of at most 1 MB, and be text. URLs and PlantUML's
//! standard library (`<C4/C4_Container>`) are not read: Omaspec reads
//! nothing from the network and does not ship the library.

use std::fs;
use std::path::{Component, Path, PathBuf};

use omaspec_uml::{Included, Includes};

/// A file may hold at most this many bytes.
const MAX_FILE: u64 = 1 << 20;

/// Where a diagram lives: its own folder, which `!include` paths start
/// from, and the project folder, which no included file may leave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub dir: PathBuf,
    pub root: PathBuf,
}

impl Place {
    /// The place of the diagram file `path` in the project folder `root`
    /// (the editor's folder). Without one, or for a file outside it, the
    /// file's own folder is the project folder.
    pub fn of(path: &Path, root: Option<&Path>) -> Option<Place> {
        let path = std::path::absolute(path).ok()?;
        let dir = path.parent()?.to_path_buf();
        let root = root
            .and_then(|root| std::path::absolute(root).ok())
            .filter(|root| dir.starts_with(root))
            .unwrap_or_else(|| dir.clone());
        Some(Place { dir, root })
    }
}

/// Reads `!include`s for a diagram at a [`Place`].
pub(crate) struct Folder {
    place: Place,
    /// The project folder with its links followed; `None` if it is gone.
    root: Option<PathBuf>,
}

impl Folder {
    pub(crate) fn new(place: &Place) -> Self {
        Folder {
            place: place.clone(),
            root: fs::canonicalize(&place.root).ok(),
        }
    }
}

impl Includes for Folder {
    fn read(&self, path: &str, from: Option<&str>) -> Result<Included, String> {
        if path.starts_with('<') {
            return Err(format!(
                "`{path}` is in PlantUML's standard library, which Omaspec does not have"
            ));
        }
        if path.contains("://") {
            return Err(format!(
                "`{path}` is on the network, and Omaspec reads nothing from there"
            ));
        }
        let shown_root = self.place.root.display();
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| format!("the project folder `{shown_root}` is not there"))?;
        let base = match from.and_then(|file| Path::new(file).parent()) {
            Some(dir) => dir.to_path_buf(),
            None => self.place.dir.clone(),
        };
        let file =
            fs::canonicalize(base.join(path)).map_err(|_| format!("`{path}` is not there"))?;
        let inside = file
            .strip_prefix(root)
            .map_err(|_| format!("`{path}` is outside the project folder `{shown_root}`"))?;
        let hidden = inside.components().any(|part| {
            matches!(part, Component::Normal(name) if name.to_string_lossy().starts_with('.'))
        });
        if hidden {
            return Err(format!("`{path}` is hidden or in a hidden folder"));
        }
        let meta = fs::metadata(&file).map_err(|e| format!("`{path}` cannot be read: {e}"))?;
        if !meta.is_file() {
            return Err(format!("`{path}` is not a file"));
        }
        if meta.len() > MAX_FILE {
            return Err(format!("`{path}` is larger than 1 MB"));
        }
        let bytes = fs::read(&file).map_err(|e| format!("`{path}` cannot be read: {e}"))?;
        let text = String::from_utf8(bytes).map_err(|_| format!("`{path}` is not text"))?;
        Ok(Included {
            key: file.to_string_lossy().into_owned(),
            text,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("omaspec-include-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reads_inside_the_project_and_nothing_else() {
        let root = temp("rules");
        let project = root.join("project");
        fs::create_dir_all(project.join("docs/parts")).unwrap();
        fs::create_dir_all(project.join(".private")).unwrap();
        fs::write(project.join("common.puml"), "participant Api").unwrap();
        fs::write(
            project.join("docs/parts/more.puml"),
            "!include ../../common.puml",
        )
        .unwrap();
        fs::write(project.join(".private/key.puml"), "secret").unwrap();
        fs::write(
            project.join("big.puml"),
            vec![b'x'; (MAX_FILE + 1) as usize],
        )
        .unwrap();
        fs::write(project.join("binary.puml"), [0xff, 0xfe, 0x00]).unwrap();
        fs::write(root.join("outside.puml"), "secret").unwrap();
        std::os::unix::fs::symlink(root.join("outside.puml"), project.join("link.puml")).unwrap();

        let place = Place::of(&project.join("docs/main.puml"), Some(&project)).unwrap();
        assert_eq!(place.root, project);
        let folder = Folder::new(&place);
        // Relative to the diagram, then relative to the including file.
        let more = folder.read("parts/more.puml", None).unwrap();
        assert_eq!(more.text, "!include ../../common.puml");
        let common = folder.read("../../common.puml", Some(&more.key)).unwrap();
        assert_eq!(common.text, "participant Api");
        assert_eq!(folder.read("../common.puml", None).unwrap().key, common.key);
        for (path, part) in [
            ("../../outside.puml", "outside the project folder"),
            ("../link.puml", "outside the project folder"),
            ("../.private/key.puml", "hidden"),
            ("../big.puml", "larger than 1 MB"),
            ("../binary.puml", "not text"),
            ("parts", "not a file"),
            ("nope.puml", "not there"),
            ("<C4/C4_Container>", "standard library"),
            ("https://example.com/x.puml", "network"),
        ] {
            let err = folder.read(path, None).unwrap_err();
            assert!(err.contains(part), "{path}: {err}");
        }
        // A file outside the editor's folder is its own project.
        let alone = Place::of(&root.join("outside.puml"), Some(&project)).unwrap();
        assert_eq!(alone.root, root);
        let _ = fs::remove_dir_all(&root);
    }
}
