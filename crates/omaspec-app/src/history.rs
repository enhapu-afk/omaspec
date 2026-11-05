//! A project's versions, read for their behavior (ROADMAP, 0.8):
//! `omaspec diff DIR` tells what changed in the behavior of the project in
//! DIR since a commit, or from another folder
//! ([`omaspec_model::diff_project`]): a changelog of the specification.
//! Both versions are read alike, as the project's index reads the folder.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use omaspec_model::{DiagramChanges, File};
use serde::Serialize;

use crate::commands::{ReportedChange, change_line, reported_changes, stdout};
use crate::names::{Owned, diagram_files, diagram_files_at};

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
