//! Starting a project (`docs/adr/0017-projects.md`): `omaspec new` and the
//! editor's *New project* make a folder with `omaspec-project.toml` and a
//! first diagram, and on request a git repository.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use omaspec_core::Language;
use omaspec_core::project::{self, FILE, Project};

/// A project file larger than this is not read.
const MAX_FILE: u64 = 64 << 10;

/// What [`create`] made.
#[derive(Debug)]
pub struct Created {
    /// The project's folder, absolute.
    pub root: PathBuf,
    pub name: String,
    /// The first diagram, relative to `root`.
    pub main: String,
    /// Whether `main` was written now; a file of that name stays as it was.
    pub wrote_main: bool,
    pub git: Git,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Git {
    /// Not asked for.
    No,
    Initialized,
    /// The folder is in this repository already; no second one inside it.
    Inside(PathBuf),
    /// `git init` did not run or failed: why.
    Failed(String),
}

impl Created {
    /// One line for the editor's status bar.
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("new project {}: {FILE}", self.name)];
        if self.wrote_main {
            parts.push(self.main.clone());
        } else {
            parts.push(format!("{} as it was", self.main));
        }
        match &self.git {
            Git::No => {}
            Git::Initialized => parts.push("a git repository".into()),
            Git::Inside(repo) => parts.push(format!("in the git repository {}", repo.display())),
            Git::Failed(why) => parts.push(format!("no git: {why}")),
        }
        parts.join(", ")
    }
}

/// Makes `dir` a project: creates the folder if it is not there, writes
/// the first diagram in `language` unless a file of its name is there,
/// then `omaspec-project.toml`, and with `git` runs `git init` unless the
/// folder is inside a repository already. A folder that is a project
/// already, the home folder and the file system's root are refused.
pub fn create(dir: &Path, language: Language, git: bool) -> Result<Created> {
    let root =
        std::path::absolute(dir).with_context(|| format!("no folder `{}`", dir.display()))?;
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if root.parent().is_none() || home.as_deref() == Some(root.as_path()) {
        bail!(
            "`{}` is no place for a project; give it a folder of its own",
            root.display()
        );
    }
    if root.exists() && !root.is_dir() {
        bail!("`{}` is a file, not a folder", root.display());
    }
    if root.join(FILE).exists() {
        bail!("`{}` is a project already", root.display());
    }
    std::fs::create_dir_all(&root)
        .with_context(|| format!("cannot create `{}`", root.display()))?;
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".into());
    let main = project::main_file(language);
    let wrote_main = write_new(&root.join(&main), &project::starter(language, &name))?;
    let file = Project {
        name: Some(name.clone()),
        main: Some(main.clone()),
    };
    if !write_new(&root.join(FILE), &file.to_toml())? {
        bail!("`{}` is a project already", root.display());
    }
    let git = if git { init_git(&root) } else { Git::No };
    Ok(Created {
        root,
        name,
        main,
        wrote_main,
        git,
    })
}

/// What `omaspec-project.toml` in `root` says; `None` when there is none.
pub fn read(root: &Path) -> Option<Result<Project, String>> {
    let path = root.join(FILE);
    let meta = std::fs::metadata(&path).ok().filter(|m| m.is_file())?;
    if meta.len() > MAX_FILE {
        return Some(Err(format!("{FILE} is larger than 64 KB")));
    }
    Some(match std::fs::read_to_string(&path) {
        Ok(text) => Project::parse(&text).map_err(|e| e.to_string()),
        Err(err) => Err(format!("{FILE}: {err}")),
    })
}

/// Writes `text` to a file that is not there yet; `false` when it is.
fn write_new(path: &Path, text: &str) -> Result<bool> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == ErrorKind::AlreadyExists => return Ok(false),
        Err(err) => return Err(err).with_context(|| format!("cannot write `{}`", path.display())),
    };
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_all())
        .with_context(|| format!("cannot write `{}`", path.display()))?;
    Ok(true)
}

/// `git init` in `root`, unless it or a folder above it is a repository.
fn init_git(root: &Path) -> Git {
    if let Some(repo) = root.ancestors().find(|dir| dir.join(".git").exists()) {
        return Git::Inside(repo.to_path_buf());
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "core.hooksPath=/dev/null", "init", "--quiet"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .output();
    match out {
        Ok(out) if out.status.success() => Git::Initialized,
        Ok(out) => Git::Failed(
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .next()
                .unwrap_or("git init failed")
                .to_owned(),
        ),
        Err(err) if err.kind() == ErrorKind::NotFound => Git::Failed("git is not installed".into()),
        Err(err) => Git::Failed(err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("omaspec-project-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn makes_a_folder_a_project_with_a_first_diagram() {
        let dir = temp("new");
        let shop = dir.join("shop");
        let made = create(&shop, Language::Mermaid, false).unwrap();
        assert_eq!(made.root, std::fs::canonicalize(&shop).unwrap());
        assert_eq!(
            (made.name.as_str(), made.main.as_str()),
            ("shop", "system.mmd")
        );
        assert!(made.wrote_main);
        assert_eq!(made.git, Git::No);
        let file = read(&shop).unwrap().unwrap();
        assert_eq!(file.name.as_deref(), Some("shop"));
        assert_eq!(file.main.as_deref(), Some("system.mmd"));
        let main = std::fs::read_to_string(shop.join("system.mmd")).unwrap();
        assert!(main.contains("title: shop"), "{main}");
        assert_eq!(omaspec_render::project_root(&shop), Some(made.root.clone()));
        assert!(
            made.summary()
                .starts_with("new project shop: omaspec-project.toml, system.mmd")
        );

        // Twice is refused, and nothing is overwritten.
        let err = create(&shop, Language::PlantUml, false).unwrap_err();
        assert!(err.to_string().contains("a project already"), "{err}");
        assert!(!shop.join("system.puml").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_with_diagrams_keeps_them() {
        let dir = temp("existing");
        std::fs::write(dir.join("system.puml"), "@startuml\nA -> B\n@enduml\n").unwrap();
        let made = create(&dir, Language::PlantUml, false).unwrap();
        assert!(!made.wrote_main);
        assert_eq!(
            std::fs::read_to_string(dir.join("system.puml")).unwrap(),
            "@startuml\nA -> B\n@enduml\n"
        );
        assert!(made.summary().contains("system.puml as it was"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_a_file_and_the_root() {
        let dir = temp("refused");
        std::fs::write(dir.join("file"), "").unwrap();
        let err = create(&dir.join("file"), Language::PlantUml, false).unwrap_err();
        assert!(err.to_string().contains("is a file"), "{err}");
        let err = create(Path::new("/"), Language::PlantUml, false).unwrap_err();
        assert!(err.to_string().contains("no place for a project"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn git_starts_a_repository_unless_one_is_around_it() {
        let dir = temp("git");
        let made = create(&dir.join("shop"), Language::PlantUml, true).unwrap();
        match made.git {
            Git::Failed(why) => {
                eprintln!("skipped: {why}");
                return;
            }
            git => assert_eq!(git, Git::Initialized),
        }
        assert!(dir.join("shop/.git").is_dir());
        let inner = create(&dir.join("shop/parts"), Language::PlantUml, true).unwrap();
        assert_eq!(inner.git, Git::Inside(made.root.clone()));
        assert!(!dir.join("shop/parts/.git").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
