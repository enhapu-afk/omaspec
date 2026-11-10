//! A project's versions, read for their behavior (ROADMAP, 0.8):
//! `omaspec diff DIR` tells what changed in the behavior of the project in
//! DIR since a commit, or from another folder
//! ([`omaspec_model::diff_project`]): a changelog of the specification.
//! Both versions are read alike, as the project's index reads the folder.
//! `omaspec log FILE` tells it for one diagram, commit by commit.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use omaspec_core::{Kind, Language, classify, markdown};
use omaspec_model::{DiagramChanges, File, FileKind};
use serde::Serialize;
use serde_json::json;

use crate::commands::{ReportedChange, change_line, reported_changes, stdout};
use crate::git::FileCommit;
use crate::names::{MAX_FILE, Owned, diagram_files, diagram_files_at, owned};

/// `omaspec diff` for folders: one compared with its version in git's
/// `rev` (`HEAD` unless given), or two with each other.
pub fn diff(folders: &[PathBuf], rev: Option<&str>, json: bool) -> Result<ExitCode> {
    let (old, new, since) = match folders {
        [dir] => {
            let rev = rev.unwrap_or("HEAD");
            (
                diagram_files_at(dir, rev)?,
                diagram_files(dir),
                format!("since {rev}"),
            )
        }
        [old, new] => {
            anyhow::ensure!(rev.is_none(), "--rev compares one folder with a commit");
            (
                diagram_files(old),
                diagram_files(new),
                format!("from {} to {}", old.display(), new.display()),
            )
        }
        _ => anyhow::bail!("diff compares two versions"),
    };
    let diagrams = omaspec_model::diff_project(&files(&old), &files(&new));
    let mut out = stdout();
    if json {
        serde_json::to_writer_pretty(&mut out, &report(&diagrams))?;
        writeln!(out)?;
    } else {
        for d in &diagrams {
            let (old, new) = names(d);
            for c in &d.changes {
                writeln!(out, "{}", change_line(old, new, c))?;
            }
        }
        eprintln!("{}", summary(&diagrams, &since));
    }
    Ok(if diagrams.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// The files as the model reads them.
pub(crate) fn files(owned: &[Owned]) -> Vec<File<'_>> {
    owned.iter().map(Owned::file).collect()
}

/// The diagram's file in the old version and in the new one, each the
/// other's where it is not there.
fn names(d: &DiagramChanges) -> (&str, &str) {
    let old = d
        .old_file
        .as_deref()
        .or(d.new_file.as_deref())
        .unwrap_or("");
    let new = d.new_file.as_deref().unwrap_or(old);
    (old, new)
}

#[derive(Serialize)]
struct Report<'a> {
    same: bool,
    diagrams: Vec<Diagram<'a>>,
}

#[derive(Serialize)]
struct Diagram<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    old: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new: Option<&'a str>,
    changes: Vec<ReportedChange<'a>>,
}

fn report(diagrams: &[DiagramChanges]) -> Report<'_> {
    Report {
        same: diagrams.is_empty(),
        diagrams: diagrams
            .iter()
            .map(|d| {
                let (old, new) = names(d);
                Diagram {
                    old: d.old_file.as_deref(),
                    new: d.new_file.as_deref(),
                    changes: reported_changes(old, new, &d.changes),
                }
            })
            .collect(),
    }
}

/// `3 diagrams changed since HEAD: 1 new, 1 removed`.
fn summary(diagrams: &[DiagramChanges], since: &str) -> String {
    if diagrams.is_empty() {
        return format!("no diagram changed in its behavior {since}");
    }
    let changed = match diagrams.len() {
        1 => "1 diagram changed".to_owned(),
        n => format!("{n} diagrams changed"),
    };
    let added = diagrams.iter().filter(|d| d.old_file.is_none()).count();
    let removed = diagrams.iter().filter(|d| d.new_file.is_none()).count();
    let mut parts = Vec::new();
    if added > 0 {
        parts.push(format!("{added} new"));
    }
    if removed > 0 {
        parts.push(format!("{removed} removed"));
    }
    match parts.is_empty() {
        true => format!("{changed} {since}"),
        false => format!("{changed} {since}: {}", parts.join(", ")),
    }
}

/// The commits that changed `file`, newest first and at most `count`,
/// each with what it changed in the behavior of its diagrams.
fn entries(
    file: &Path,
    count: Option<usize>,
    project: Option<&Path>,
) -> Result<Vec<(crate::git::FileCommit, Vec<omaspec_model::DiagramChanges>)>> {
    let commits = crate::git::file_log(file, count, MAX_FILE)?;
    let mut projects = Projects::new(crate::specs::root_of(Some(file), project));
    let mut entries = Vec::new();
    for commit in commits {
        let old = version(commit.before.as_ref());
        let new = version(commit.after.as_ref());
        if old.is_none() && new.is_none() {
            continue;
        }
        let diagrams = match plays(old.as_ref()) || plays(new.as_ref()) {
            true => projects.diff(&commit, old, new)?,
            false => omaspec_model::diff_project(&files(old.as_slice()), &files(new.as_slice())),
        };
        entries.push((commit, diagrams));
    }
    Ok(entries)
}

/// One commit of a file's history, for the editor's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Past {
    /// The revision to compare with.
    pub rev: String,
    /// `1a2b3c4 2026-11-05 the subject`.
    pub label: String,
    /// `3 changes in behavior`, `no change in behavior`.
    pub changed: String,
}

/// The history of `file` for the editor: its last `count` commits, each
/// with how much it changed in the behavior, as `omaspec log` tells it.
pub fn past(file: &Path, count: usize) -> Result<Vec<Past>> {
    Ok(entries(file, Some(count), None)?
        .into_iter()
        .map(|(commit, diagrams)| {
            let changes: usize = diagrams.iter().map(|d| d.changes.len()).sum();
            Past {
                rev: commit.hash,
                label: format!("{} {} {}", commit.short, commit.date, commit.subject),
                changed: match changes {
                    0 => "no change in behavior".to_owned(),
                    1 => "1 change in behavior".to_owned(),
                    n => format!("{n} changes in behavior"),
                },
            }
        })
        .collect())
}

/// `omaspec log FILE`: for each commit that changed the diagram file or
/// Markdown document `file`, newest first, what changed in its behavior;
/// at most `count` commits. A sequence diagram is checked against the
/// state machines of its project (`project`, else the one it is in) as
/// each commit had them.
pub fn log(
    file: &Path,
    count: Option<usize>,
    project: Option<&Path>,
    json: bool,
) -> Result<ExitCode> {
    let entries = entries(file, count, project)?;
    let mut out = stdout();
    if json {
        let report: Vec<serde_json::Value> = entries
            .iter()
            .map(|(c, diagrams)| {
                let mut entry = json!({
                    "commit": c.hash,
                    "short": c.short,
                    "date": c.date,
                    "subject": c.subject,
                });
                if let Ok(serde_json::Value::Object(rest)) = serde_json::to_value(report(diagrams))
                {
                    entry
                        .as_object_mut()
                        .into_iter()
                        .for_each(|e| e.extend(rest.clone()));
                }
                entry
            })
            .collect();
        serde_json::to_writer_pretty(&mut out, &report)?;
        writeln!(out)?;
    } else {
        for (c, diagrams) in &entries {
            writeln!(out, "{} {} {}", c.short, c.date, c.subject)?;
            if diagrams.is_empty() {
                writeln!(out, "    no change in behavior")?;
            }
            for d in diagrams {
                let (old, new) = names(d);
                let (old, new) = (format!("{old}@{}^", c.short), format!("{new}@{}", c.short));
                for change in &d.changes {
                    writeln!(out, "    {}", change_line(&old, &new, change))?;
                }
            }
        }
        let kept = entries.iter().filter(|(_, d)| d.is_empty()).count();
        eprintln!(
            "{}",
            log_summary(entries.len(), kept, &file.display().to_string())
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// `3 commits changed door.puml; 1 of them kept its behavior`: a commit
/// that changed only what Omaspec does not compare kept nothing it knows of.
fn log_summary(commits: usize, kept: usize, file: &str) -> String {
    let changed = match commits {
        0 => return format!("no commit changed {file}"),
        1 => format!("1 commit changed {file}"),
        n => format!("{n} commits changed {file}"),
    };
    match (kept, commits) {
        (0, _) => changed,
        (1, 1) => format!("{changed}; it kept its behavior"),
        (k, n) if k == n => format!("{changed}; each of them kept its behavior"),
        (k, _) => format!("{changed}; {k} of them kept its behavior"),
    }
}

/// A version of the file as the project diff reads it; `None` where it
/// was not there or too large.
fn version(v: Option<&crate::git::Version>) -> Option<Owned> {
    let v = v?;
    Some(owned(v.path.clone(), v.text.clone()?))
}

/// Whether `file` holds a sequence diagram, which plays state machines.
fn plays(file: Option<&Owned>) -> bool {
    let Some(file) = file else {
        return false;
    };
    let sequence = |code: &str, language: Language| classify(code, language).kind == Kind::Sequence;
    match file.kind {
        FileKind::Diagram(language) => sequence(&file.text, language),
        FileKind::Document => markdown::blocks(&file.text)
            .iter()
            .any(|b| sequence(&b.code, b.language)),
        FileKind::Trace => false,
    }
}

/// The project a sequence diagram's machines come from, read as each
/// commit had it.
struct Projects {
    root: Option<PathBuf>,
    /// The project's folder from the repository's root.
    prefix: Option<String>,
    /// By commit.
    read: HashMap<String, Vec<Owned>>,
}

impl Projects {
    fn new(root: Option<PathBuf>) -> Projects {
        let prefix = root.as_deref().and_then(|r| crate::git::prefix(r).ok());
        Projects {
            root,
            prefix,
            read: HashMap::new(),
        }
    }

    /// The changes of the file from `old` to `new` in `commit`, the rest
    /// of the project as the commit and its parent had it.
    fn diff(
        &mut self,
        commit: &FileCommit,
        old: Option<Owned>,
        new: Option<Owned>,
    ) -> Result<Vec<DiagramChanges>> {
        let (old_path, new_path) = (path(old.as_ref()), path(new.as_ref()));
        let before = self.with(commit.parent.as_deref(), old)?;
        let after = self.with(Some(&commit.hash), new)?;
        Ok(omaspec_model::diff_project(&files(&before), &files(&after))
            .into_iter()
            .filter(|d| {
                (d.old_file.is_some() && d.old_file == old_path)
                    || (d.new_file.is_some() && d.new_file == new_path)
            })
            .collect())
    }

    /// The project as `rev` had it, by paths from the repository's root,
    /// with `file` as given; nothing but `file` without a commit or a
    /// project.
    fn with(&mut self, rev: Option<&str>, file: Option<Owned>) -> Result<Vec<Owned>> {
        let mut files = match (rev, &self.root, &self.prefix) {
            (Some(rev), Some(root), Some(prefix)) => {
                if !self.read.contains_key(rev) {
                    let read = diagram_files_at(root, rev)?
                        .into_iter()
                        .map(|f| owned(format!("{prefix}{}", f.path), f.text))
                        .collect();
                    self.read.insert(rev.to_owned(), read);
                }
                self.read[rev]
                    .iter()
                    .map(|f| owned(f.path.clone(), f.text.clone()))
                    .collect()
            }
            _ => Vec::new(),
        };
        if let Some(file) = file {
            files.retain(|f| f.path != file.path);
            files.push(file);
        }
        Ok(files)
    }
}

fn path(file: Option<&Owned>) -> Option<String> {
    file.map(|f| f.path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagram(old: Option<&str>, new: Option<&str>) -> DiagramChanges {
        DiagramChanges {
            old_file: old.map(str::to_owned),
            new_file: new.map(str::to_owned),
            changes: Vec::new(),
        }
    }

    #[test]
    fn the_log_says_how_many_commits_kept_the_behavior() {
        assert_eq!(log_summary(0, 0, "a.puml"), "no commit changed a.puml");
        assert_eq!(log_summary(1, 0, "a.puml"), "1 commit changed a.puml");
        assert_eq!(
            log_summary(1, 1, "a.puml"),
            "1 commit changed a.puml; it kept its behavior"
        );
        assert_eq!(
            log_summary(3, 1, "a.puml"),
            "3 commits changed a.puml; 1 of them kept its behavior"
        );
        assert_eq!(
            log_summary(2, 2, "a.puml"),
            "2 commits changed a.puml; each of them kept its behavior"
        );
    }

    #[test]
    fn the_summary_counts_new_and_removed_diagrams() {
        assert_eq!(
            summary(&[], "since HEAD"),
            "no diagram changed in its behavior since HEAD"
        );
        let one = [diagram(Some("a.puml"), Some("a.puml"))];
        assert_eq!(summary(&one, "since v1"), "1 diagram changed since v1");
        let three = [
            diagram(Some("a.puml"), Some("b.puml")),
            diagram(None, Some("c.puml")),
            diagram(Some("d.puml"), None),
        ];
        assert_eq!(
            summary(&three, "since HEAD"),
            "3 diagrams changed since HEAD: 1 new, 1 removed"
        );
        assert_eq!(names(&three[1]), ("c.puml", "c.puml"));
        assert_eq!(names(&three[2]), ("d.puml", "d.puml"));
    }
}
