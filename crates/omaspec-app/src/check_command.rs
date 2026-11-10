//! `omaspec check`: a project folder for CI. Lint (syntax, the checks
//! against its state machines, links to diagrams it does not have) and
//! each SVG held against the one drawn now, the same drawing as
//! `render --check`.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use omaspec_core::lint::Severity;
use serde::Serialize;

use crate::commands::stdout;
use crate::files::read_input;
use crate::specs::Projects;

/// One finding, with a one-based line when it has one.
struct Item {
    file: String,
    line: Option<usize>,
    severity: Severity,
    message: String,
}

#[derive(Serialize)]
struct Picture {
    file: String,
    svg: String,
    same: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// What lint finds in the diagrams of the folder `dir`, as saved: the
/// file within it, the zero-based line, and the finding. The same as the
/// first half of [`check`], for the editor's *Check the project*.
pub fn findings(dir: &Path) -> Result<Vec<(String, omaspec_core::lint::Finding)>> {
    let mut projects = Projects::default();
    let mut out = Vec::new();
    for relative in &crate::editor::folder::scan(dir) {
        let input = read_input(Some(&dir.join(relative)))?;
        let file = crate::specs::file_name(relative);
        for finding in projects.findings(&input, None, Some(dir)) {
            out.push((file.clone(), finding));
        }
    }
    Ok(out)
}

pub fn check(dir: &Path, json: bool, strict: bool, github: bool) -> Result<ExitCode> {
    if !dir.exists() {
        anyhow::bail!("no folder `{}`", dir.display());
    }
    anyhow::ensure!(dir.is_dir(), "`{}` is a file, not a folder", dir.display());
    let found = crate::editor::folder::scan(dir);
    anyhow::ensure!(!found.is_empty(), "{}: no diagrams in it", dir.display());

    let mut projects = Projects::default();
    let mut items = Vec::new();
    for relative in &found {
        let input = read_input(Some(&dir.join(relative)))?;
        let file = crate::specs::file_name(relative);
        for finding in projects.findings(&input, None, Some(dir)) {
            items.push(Item {
                file: file.clone(),
                line: Some(finding.line + 1),
                severity: finding.severity,
                message: finding.message,
            });
        }
    }

    let mut pictures = Vec::new();
    for held in crate::render_command::hold_files(&[dir.to_path_buf()])? {
        if held.same.is_none() {
            if let Some(message) = held.message {
                items.push(Item {
                    file: within(dir, &held.file),
                    line: held.line,
                    severity: Severity::Error,
                    message,
                });
            }
            continue;
        }
        let Some(svg) = held.output else { continue };
        pictures.push(Picture {
            file: within(dir, &held.file),
            svg: within(dir, &svg),
            same: held.same == Some(true),
            message: held.message,
        });
    }

    let (errors, warnings, infos) = counts(&items);
    let differ = pictures.iter().filter(|p| !p.same).count();
    let failed = errors > 0 || (strict && warnings > 0) || differ > 0;
    let annotate = github || in_actions();

    if json {
        let mut out = stdout();
        serde_json::to_writer_pretty(&mut out, &report(failed, &items, &pictures))?;
        writeln!(out)?;
        if annotate {
            write_annotations(&mut std::io::stderr(), &items, &pictures)?;
        }
    } else if github {
        write_annotations(&mut stdout(), &items, &pictures)?;
        eprintln!(
            "{}",
            summary(errors, warnings, infos, pictures.len(), differ)
        );
    } else {
        let mut out = stdout();
        for item in &items {
            writeln!(out, "{}", item.line_text())?;
        }
        for picture in pictures.iter().filter(|p| !p.same) {
            writeln!(out, "{}: {}", picture.svg, picture.why())?;
        }
        if annotate {
            write_annotations(&mut out, &items, &pictures)?;
        }
        eprintln!(
            "{}",
            summary(errors, warnings, infos, pictures.len(), differ)
        );
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn report<'a>(ok_failed: bool, items: &'a [Item], pictures: &'a [Picture]) -> Report<'a> {
    Report {
        ok: !ok_failed,
        findings: items.iter().map(Item::found).collect(),
        pictures,
    }
}

#[derive(Serialize)]
struct Report<'a> {
    ok: bool,
    findings: Vec<Found<'a>>,
    pictures: &'a [Picture],
}

#[derive(Serialize)]
struct Found<'a> {
    file: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    severity: Severity,
    message: &'a str,
}

impl Item {
    fn found(&self) -> Found<'_> {
        Found {
            file: &self.file,
            line: self.line,
            severity: self.severity,
            message: &self.message,
        }
    }

    /// `file:line: severity: message`, as `lint` prints it.
    fn line_text(&self) -> String {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        match self.line {
            Some(line) => format!("{}:{line}: {severity}: {}", self.file, self.message),
            None => format!("{}: {severity}: {}", self.file, self.message),
        }
    }
}

impl Picture {
    fn why(&self) -> &str {
        self.message.as_deref().unwrap_or("drawn differently")
    }
}

fn counts(items: &[Item]) -> (usize, usize, usize) {
    let (mut errors, mut warnings, mut infos) = (0, 0, 0);
    for item in items {
        match item.severity {
            Severity::Error => errors += 1,
            Severity::Warning => warnings += 1,
            Severity::Info => infos += 1,
        }
    }
    (errors, warnings, infos)
}

fn summary(errors: usize, warnings: usize, infos: usize, checked: usize, differ: usize) -> String {
    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(count(errors, "error", "errors"));
    }
    if warnings > 0 {
        parts.push(count(warnings, "warning", "warnings"));
    }
    if infos > 0 {
        parts.push(count(infos, "info", "infos"));
    }
    let findings = if parts.is_empty() {
        "no findings".to_owned()
    } else {
        parts.join(", ")
    };
    let pictures = if checked == 0 {
        "no SVG to hold".to_owned()
    } else if differ == 0 {
        format!("{checked} SVGs drawn as they are")
    } else {
        format!("{differ} of {checked} SVGs differ; `omaspec render` draws them again")
    };
    format!("{findings}; {pictures}")
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// A GitHub Actions workflow command. The file and the message are escaped
/// the way the runner reads them.
fn annotation(level: &str, file: &str, line: Option<usize>, message: &str) -> String {
    let file = escape_property(file);
    let message = escape_data(message);
    match line {
        Some(line) => format!("::{level} file={file},line={line}::{message}"),
        None => format!("::{level} file={file}::{message}"),
    }
}

fn escape_data(text: &str) -> String {
    text.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

fn escape_property(text: &str) -> String {
    escape_data(text).replace(':', "%3A").replace(',', "%2C")
}

fn write_annotations(mut out: impl Write, items: &[Item], pictures: &[Picture]) -> Result<()> {
    for item in items {
        let level = match item.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "notice",
        };
        writeln!(
            out,
            "{}",
            annotation(level, &item.file, item.line, &item.message)
        )?;
    }
    for picture in pictures.iter().filter(|p| !p.same) {
        writeln!(
            out,
            "{}",
            annotation("error", &picture.svg, None, picture.why())
        )?;
    }
    Ok(())
}

fn in_actions() -> bool {
    std::env::var("GITHUB_ACTIONS").ok().as_deref() == Some("true")
}

/// `path` under `dir`, with `/` between its parts; `path` itself otherwise.
fn within(dir: &Path, path: &str) -> String {
    match Path::new(path).strip_prefix(dir) {
        Ok(rel) => crate::specs::file_name(rel),
        Err(_) => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_annotation_escapes_what_would_break_the_line() {
        assert_eq!(
            annotation("error", "a:b,c.puml", Some(2), "100%\nno"),
            "::error file=a%3Ab%2Cc.puml,line=2::100%25%0Ano"
        );
        assert_eq!(
            annotation("warning", "door.svg", None, "not there"),
            "::warning file=door.svg::not there"
        );
    }

    #[test]
    fn the_summary_names_findings_and_pictures() {
        assert_eq!(
            summary(0, 0, 0, 2, 0),
            "no findings; 2 SVGs drawn as they are"
        );
        assert_eq!(
            summary(1, 2, 0, 3, 1),
            "1 error, 2 warnings; 1 of 3 SVGs differ; `omaspec render` draws them again"
        );
    }
}
