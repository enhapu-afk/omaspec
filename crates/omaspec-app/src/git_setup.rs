//! `omaspec git setup` and `omaspec git textconv` (ROADMAP, 0.8): git
//! shows a diagram by its behavior. The project's `.gitattributes` names
//! the diagram files `diff=omaspec`, to be committed and shared; the
//! command git runs for that is set in the repository's own
//! configuration, as git takes no command from a repository, so each
//! clone runs `setup` once. `git diff`, `git log -p` and `git show` then
//! compare the behavior of both versions ([`omaspec_model::behavior`])
//! line by line instead of their sources; `--no-textconv` shows the
//! sources.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use omaspec_core::{Language, detect};

use crate::commands::stdout;

/// The diagram files git hands to Omaspec, as `Language::from_file_name`
/// knows them.
const PATTERNS: [&str; 6] = [
    "*.puml",
    "*.plantuml",
    "*.wsd",
    "*.puml.txt",
    "*.mmd",
    "*.mermaid",
];

/// The git configuration key of the text conversion.
const KEY: &str = "diff.omaspec.textconv";

/// A diagram larger than this is handed back as it is.
const MAX_FILE: usize = 1 << 20;

/// `omaspec git setup`: the project at `dir` (by default the one the
/// current folder is in) tells git to show its diagrams by their behavior.
pub fn setup(dir: Option<&Path>) -> Result<ExitCode> {
    let root = crate::names::root(dir)?;
    anyhow::ensure!(root.is_dir(), "no folder `{}`", root.display());
    crate::git::prefix(&root)
        .with_context(|| format!("{} is in no git repository", root.display()))?;

    let attributes = root.join(".gitattributes");
    let added = add_attributes(&attributes)?;
    let command = format!("{} git textconv", program());
    let before = crate::git::local_config(&root, KEY);
    if before.as_deref() != Some(command.as_str()) {
        crate::git::set_local_config(&root, KEY, &command)?;
    }

    let mut out = stdout();
    match added.len() {
        0 => writeln!(
            out,
            "{}: diagrams go to omaspec already",
            attributes.display()
        )?,
        _ => writeln!(
            out,
            "{}: {} diff=omaspec",
            attributes.display(),
            added.join(", ")
        )?,
    }
    match before {
        Some(was) if was == command => writeln!(out, "git config {KEY}: {command}, as it was")?,
        Some(was) => writeln!(out, "git config {KEY}: {command} (was {was})")?,
        None => writeln!(out, "git config {KEY}: {command}")?,
    }
    eprintln!(
        "git diff, git log -p and git show now tell diagrams by their behavior (--no-textconv: by their lines); commit .gitattributes, and in another clone run omaspec git setup once"
    );
    Ok(ExitCode::SUCCESS)
}

/// Adds the patterns `diff=omaspec` does not follow yet to the file at
/// `path`, made if it is not there; the patterns added.
fn add_attributes(path: &Path) -> Result<Vec<&'static str>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err).with_context(|| path.display().to_string()),
    };
    let missing: Vec<&str> = PATTERNS
        .iter()
        .copied()
        .filter(|pattern| !follows(&text, pattern))
        .collect();
    if missing.is_empty() {
        return Ok(missing);
    }
    let mut add = String::new();
    if !text.is_empty() && !text.ends_with('\n') {
        add.push('\n');
    }
    add.push_str("# git diff tells diagrams by their behavior (omaspec git setup)\n");
    for pattern in &missing {
        add.push_str(&format!("{pattern} diff=omaspec\n"));
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| file.write_all(add.as_bytes()))
        .with_context(|| path.display().to_string())?;
    Ok(missing)
}

/// Whether a line of `text` gives `pattern` the attribute `diff=omaspec`.
fn follows(text: &str, pattern: &str) -> bool {
    text.lines().any(|line| {
        let mut parts = line.split_whitespace();
        parts.next() == Some(pattern) && parts.any(|attribute| attribute == "diff=omaspec")
    })
}

/// How git starts this Omaspec: `omaspec` when that is the one on the
/// path, else its whole path, quoted for the shell git runs it through.
fn program() -> String {
    let own = std::env::current_exe().ok();
    let on_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<PathBuf>>())
        .map(|dir| dir.join("omaspec"))
        .find(|candidate| candidate.is_file());
    let same = |a: &Path, b: &Path| match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    match (own, on_path) {
        (Some(own), Some(found)) if same(&own, &found) => "omaspec".to_owned(),
        (Some(own), _) => quoted(&own.display().to_string()),
        (None, _) => "omaspec".to_owned(),
    }
}

/// `text` in single quotes for a POSIX shell.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `omaspec git textconv`: the diagram at `file` as git compares it, its
/// behavior where Omaspec reads one, else the file as it is.
pub fn textconv(file: &Path) -> Result<ExitCode> {
    let bytes = fs::read(file).with_context(|| file.display().to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned());
    let language = detect(&text, name.as_deref().and_then(Language::from_file_name));
    let behavior = (bytes.len() <= MAX_FILE)
        .then(|| omaspec_model::behavior(&text, language))
        .flatten();
    let mut out = stdout();
    match behavior {
        Some(behavior) => out.write_all(behavior.as_bytes())?,
        None => out.write_all(&bytes)?,
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pattern_follows_omaspec_once() {
        let text = "*.puml diff=omaspec\n*.mmd   text  diff=omaspec\n*.wsd diff=other\n";
        assert!(follows(text, "*.puml") && follows(text, "*.mmd"));
        assert!(!follows(text, "*.wsd") && !follows(text, "*.mermaid"));
    }

    #[test]
    fn a_path_is_quoted_for_the_shell() {
        assert_eq!(quoted("/opt/my tools/omaspec"), "'/opt/my tools/omaspec'");
        assert_eq!(quoted("/it's/omaspec"), r"'/it'\''s/omaspec'");
    }
}
