//! The files `!include` reads: local ones inside the project folder; and
//! where that folder is.
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
    /// The place of the diagram file `path`. Its project folder is the
    /// project it is in (see [`project_root`]); else `folder` (the
    /// editor's folder) when the file is inside it; else the file's own
    /// folder.
    pub fn of(path: &Path, folder: Option<&Path>) -> Option<Place> {
        let path = std::path::absolute(path).ok()?;
        let dir = path.parent()?.to_path_buf();
        let root = project_root(&dir)
            .or_else(|| {
                folder
                    .and_then(|folder| std::path::absolute(folder).ok())
                    .filter(|folder| dir.starts_with(folder))
            })
            .unwrap_or_else(|| dir.clone());
        Some(Place { dir, root })
    }
}

/// The root of the project the folder `dir` is in: the nearest folder,
/// `dir` itself or one above it, that holds `omaspec-project.toml`. The
/// home folder and the file system's root do not count, so a project file
/// left there by mistake does not open every diagram's `!include` to all
/// of it.
pub fn project_root(dir: &Path) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    root_below(dir, home.as_deref())
}

fn root_below(dir: &Path, home: Option<&Path>) -> Option<PathBuf> {
    let dir = std::path::absolute(dir).ok()?;
    dir.ancestors()
        .take_while(|folder| folder.parent().is_some() && Some(*folder) != home)
        .find(|folder| folder.join(omaspec_core::project::FILE).is_file())
        .map(Path::to_path_buf)
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

    #[test]
    fn a_project_file_marks_the_root_above_the_editors_folder() {
        let root = temp("marker");
        let shop = root.join("shop");
        fs::create_dir_all(shop.join("orders/returns")).unwrap();
        fs::write(shop.join(omaspec_core::project::FILE), "name = \"Shop\"\n").unwrap();
        let file = shop.join("orders/returns/refund.puml");
        // From any folder below it, and over the editor's folder.
        assert_eq!(
            project_root(&shop.join("orders/returns")),
            Some(shop.clone())
        );
        assert_eq!(project_root(&shop), Some(shop.clone()));
        let place = Place::of(&file, Some(&shop.join("orders"))).unwrap();
        assert_eq!(place.root, shop);
        assert_eq!(place.dir, shop.join("orders/returns"));
        assert_eq!(Place::of(&file, None).unwrap().root, shop);
        // The nearest one wins.
        fs::write(shop.join("orders/omaspec-project.toml"), "").unwrap();
        assert_eq!(Place::of(&file, None).unwrap().root, shop.join("orders"));
        // A folder named like it is no project file.
        fs::create_dir_all(root.join("other/omaspec-project.toml")).unwrap();
        assert_eq!(project_root(&root.join("other")), None);
        // Not the home folder, nor anything above it.
        assert_eq!(
            root_below(&shop.join("orders"), Some(&shop)),
            Some(shop.join("orders"))
        );
        assert_eq!(root_below(&shop, Some(&shop)), None);
        assert_eq!(root_below(&shop.join("x"), Some(&shop)), None);
        let _ = fs::remove_dir_all(&root);
    }
}
