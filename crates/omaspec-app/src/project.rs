//! Starting a project (`docs/adr/0017-projects.md`): `omaspec new` and the
//! editor's *New project* make a folder with `omaspec-project.toml`, a map
//! of the project and a first state machine on it
//! (`docs/adr/0018-diagrams-that-link.md`), and on request a git
//! repository with a pre-commit hook that runs `omaspec check`, unless a
//! hook is there already.

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
    /// The first state machine, relative to `root`.
    pub machine: String,
    /// Whether it was written now; a file of that name stays as it was.
    pub wrote_machine: bool,
    pub git: Git,
    /// The pre-commit hook, when a repository was started now.
    pub hook: Option<Hook>,
}

/// What happened to the pre-commit hook of a repository started now.
#[derive(Debug, PartialEq, Eq)]
pub enum Hook {
    Written,
    /// A hook was there already, and was left as it was.
    Already,
    /// The hook could not be written: why.
    Failed(String),
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
        if self.wrote_machine {
            parts.push(self.machine.clone());
        } else {
            parts.push(format!("{} as it was", self.machine));
        }
        match &self.git {
            Git::No => {}
            Git::Initialized => match &self.hook {
                Some(Hook::Written) => {
                    parts.push("a git repository, a pre-commit hook".into());
                }
                Some(Hook::Already) => {
                    parts.push("a git repository; its pre-commit hook stays".into());
                }
                Some(Hook::Failed(why)) => {
                    parts.push(format!("a git repository; no pre-commit hook: {why}"));
                }
                None => parts.push("a git repository".into()),
            },
            Git::Inside(repo) => parts.push(format!("in the git repository {}", repo.display())),
            Git::Failed(why) => parts.push(format!("no git: {why}")),
        }
        parts.join(", ")
    }
}

/// Makes `dir` a project: creates the folder if it is not there, writes
/// the first state machine in `language` (unless a file of its name is
/// there), then `omaspec-project.toml`, and with `git` runs
/// `git init` unless the folder is inside a repository already. A
/// repository started now gets a pre-commit hook that runs `omaspec
/// check`, unless a hook is there already. A folder that is a project
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
    let machine = project::machine_file(language, &name);
    let wrote_machine = write_new(&root.join(&machine), &project::starter(language, &name))?;
    // No map is written: it is made from the project's diagrams
    // (docs/adr/0026).
    let file = Project {
        name: Some(name.clone()),
        look: None,
    };
    if !write_new(&root.join(FILE), &file.to_toml())? {
        bail!("`{}` is a project already", root.display());
    }
    let (git, hook) = if git {
        match init_git(&root) {
            Git::Initialized => (Git::Initialized, Some(install_hook(&root))),
            other => (other, None),
        }
    } else {
        (Git::No, None)
    };
    Ok(Created {
        root,
        name,
        machine,
        wrote_machine,
        git,
        hook,
    })
}

/// The hook `omaspec new --git` writes. It checks the project before a
/// commit; `git commit --no-verify` skips it.
const HOOK: &str = "\
#!/bin/sh
# Checks the project's diagrams before a commit (`omaspec render` draws
# the SVGs the check holds). `git commit --no-verify` skips it.
exec omaspec check .
";

/// Writes [`HOOK`] into `root`'s git hooks, unless a pre-commit hook is
/// there already.
fn install_hook(root: &Path) -> Hook {
    let path = root.join(".git/hooks/pre-commit");
    if path.exists() {
        return Hook::Already;
    }
    if let Some(dir) = path.parent()
        && let Err(err) = std::fs::create_dir_all(dir)
    {
        return Hook::Failed(err.to_string());
    }
    let mut file = match open_hook(&path) {
        Ok(file) => file,
        Err(err) if err.kind() == ErrorKind::AlreadyExists => return Hook::Already,
        Err(err) => return Hook::Failed(err.to_string()),
    };
    match file
        .write_all(HOOK.as_bytes())
        .and_then(|()| file.sync_all())
    {
        Ok(()) => Hook::Written,
        Err(err) => Hook::Failed(err.to_string()),
    }
}

/// A new hook file, executable.
#[cfg(unix)]
fn open_hook(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(path)
}

/// A new hook file. Windows has no execute bit; git runs the hook through
/// its own shell.
#[cfg(not(unix))]
fn open_hook(path: &Path) -> std::io::Result<std::fs::File> {
    OpenOptions::new().write(true).create_new(true).open(path)
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
        assert_eq!(made.name, "shop");
        assert_eq!(made.git, Git::No);
        let file = read(&shop).unwrap().unwrap();
        assert_eq!(file.name.as_deref(), Some("shop"));
        // The project's file and its first machine, and no map: that is
        // made from the diagrams.
        let mut files: Vec<String> = std::fs::read_dir(&shop)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        assert_eq!(files, ["omaspec-project.toml", "shop.mmd"]);
        assert!(made.wrote_machine);
        assert_eq!(made.machine, "shop.mmd");
        let machine = std::fs::read_to_string(shop.join("shop.mmd")).unwrap();
        assert!(machine.contains("title: shop"), "{machine}");
        assert_eq!(omaspec_render::project_root(&shop), Some(made.root.clone()));
        assert!(
            made.summary()
                .starts_with("new project shop: omaspec-project.toml, shop.mmd")
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
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let machine = format!("{name}.puml");
        std::fs::write(dir.join(&machine), "@startuml\nA -> B\n@enduml\n").unwrap();
        std::fs::write(dir.join("other.puml"), "@startuml\nC -> D\n@enduml\n").unwrap();
        let made = create(&dir, Language::PlantUml, false).unwrap();
        // A file of the first machine's name stays as it was.
        assert!(!made.wrote_machine);
        assert_eq!(
            std::fs::read_to_string(dir.join(&machine)).unwrap(),
            "@startuml\nA -> B\n@enduml\n"
        );
        assert!(made.summary().contains(&format!("{machine} as it was")));
        assert!(dir.join("other.puml").exists());
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
        use std::os::unix::fs::PermissionsExt;
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
        assert_eq!(made.hook, Some(Hook::Written));
        let hook = dir.join("shop/.git/hooks/pre-commit");
        let text = std::fs::read_to_string(&hook).unwrap();
        assert!(text.contains("exec omaspec check .\n"), "{text}");
        assert_eq!(
            std::fs::metadata(&hook).unwrap().permissions().mode() & 0o111,
            0o111
        );
        // A hook that is there already stays, including one of our own
        // written over.
        std::fs::write(&hook, "keep\n").unwrap();
        assert_eq!(install_hook(&made.root), Hook::Already);
        assert_eq!(std::fs::read_to_string(&hook).unwrap(), "keep\n");

        let inner = create(&dir.join("shop/parts"), Language::PlantUml, true).unwrap();
        assert_eq!(inner.git, Git::Inside(made.root.clone()));
        assert_eq!(inner.hook, None);
        assert!(!dir.join("shop/parts/.git").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
