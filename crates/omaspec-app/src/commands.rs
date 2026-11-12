//! The CLI subcommands.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use omaspec_core::lint::{Finding, Severity};
use omaspec_core::outline::Entry;
use omaspec_core::{DiagramKind, Language, classify, detect, embed, outline};
use omaspec_render::{Config, RenderError, mermaid_coverage};
use omaspec_theme::{Palette, Rgb, Source, Theme};
use serde::Serialize;

use crate::files::{Input, read_input, read_inputs, write_atomic};
use crate::specs::Projects;

/// Standard output for the commands. When its reader stops early
/// (`omaspec example | head`), a write fails with [`ReaderGone`], which
/// `main` takes for the end of the command, not for an error.
pub(crate) fn stdout() -> Stdout {
    Stdout(io::stdout().lock())
}

pub(crate) struct Stdout(io::StdoutLock<'static>);

impl Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf).map_err(gone)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush().map_err(gone)
    }
}

/// The reader of standard output went away. Only standard output says so:
/// a broken pipe to `wl-copy` or to PlantUML stays an error.
#[derive(Debug)]
struct ReaderGone;

impl std::fmt::Display for ReaderGone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the reader of standard output went away")
    }
}

impl std::error::Error for ReaderGone {}

fn gone(err: io::Error) -> io::Error {
    if err.kind() == io::ErrorKind::BrokenPipe {
        io::Error::new(err.kind(), ReaderGone)
    } else {
        err
    }
}

/// Whether `err` is only that the reader of standard output went away.
pub(crate) fn reader_gone(err: &anyhow::Error) -> bool {
    err.chain().any(|e| {
        e.downcast_ref::<io::Error>()
            .and_then(io::Error::get_ref)
            .is_some_and(|inner| inner.is::<ReaderGone>())
    })
}

pub(crate) fn language(input: &Input, forced: Option<Language>) -> Language {
    forced.unwrap_or_else(|| {
        let hint = input.file_name().and_then(Language::from_file_name);
        detect(&input.text, hint)
    })
}

/// A finding as the CLI reports it: one-based line.
#[derive(Serialize)]
pub(crate) struct Reported<'a> {
    file: String,
    line: usize,
    severity: Severity,
    message: &'a str,
    /// How to put it right, its line counted from 1 like the finding's.
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<omaspec_core::lint::Fix>,
}

/// The findings of the diagram `file` as the CLI reports them.
pub(crate) fn reported<'a>(file: &str, findings: &'a [Finding]) -> Vec<Reported<'a>> {
    findings
        .iter()
        .map(|f| Reported {
            file: file.to_owned(),
            line: f.line + 1,
            severity: f.severity,
            message: &f.message,
            fix: f.fix.clone().map(|mut fix| {
                fix.line += 1;
                fix
            }),
        })
        .collect()
}

pub fn lint(
    files: &[PathBuf],
    forced: Option<Language>,
    json: bool,
    strict: bool,
    fix: bool,
    project: Option<&Path>,
) -> Result<ExitCode> {
    let mut all: Vec<(String, Vec<Finding>)> = Vec::new();
    let mut projects = Projects::default();
    for mut input in read_inputs(files)? {
        let mut findings = projects.findings(&input, forced, project);
        if fix {
            let path = input
                .path
                .clone()
                .context("--fix needs files, not standard input")?;
            let (fixed, done) = omaspec_core::lint::fix_all(&input.text, &findings);
            if done > 0 {
                write_atomic(&path, fixed.as_bytes())?;
                // Beside the findings on standard output, also as JSON.
                eprintln!("fixed {done} in {}", path.display());
                input.text = fixed;
                findings = projects.findings(&input, forced, project);
            }
        }
        all.push((input.name(), findings));
    }
    report(&all, json, strict)
}

pub fn trace(
    files: &[PathBuf],
    json: bool,
    strict: bool,
    project: Option<&Path>,
) -> Result<ExitCode> {
    let mut all: Vec<(String, Vec<Finding>)> = Vec::new();
    let mut projects = Projects::default();
    for input in read_inputs(files)? {
        let specs = projects.of(input.path.as_deref(), project);
        all.push((input.name(), omaspec_model::check_trace(&input.text, specs)));
    }
    report(&all, json, strict)
}

/// Prints the findings of each file as `lint` does; fails on an error, or
/// with `strict` on a warning.
fn report(all: &[(String, Vec<Finding>)], json: bool, strict: bool) -> Result<ExitCode> {
    let failed = all
        .iter()
        .flat_map(|(_, findings)| findings)
        .any(|f| f.severity == Severity::Error || (strict && f.severity == Severity::Warning));
    let mut out = stdout();
    if json {
        let reported: Vec<Reported> = all
            .iter()
            .flat_map(|(file, findings)| reported(file, findings))
            .collect();
        serde_json::to_writer_pretty(&mut out, &reported)?;
        writeln!(out)?;
    } else {
        for (file, findings) in all {
            for f in findings {
                let severity = match f.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Info => "info",
                };
                writeln!(out, "{file}:{}: {severity}: {}", f.line + 1, f.message)?;
            }
        }
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// The two versions `diff` compares: the old one, how to name it, the new
/// one. With one file, the old one is its version in git's `rev`.
pub(crate) fn versions(
    old: Option<&Path>,
    new: &Path,
    rev: Option<&str>,
) -> Result<(Input, String, Input)> {
    let new_input = read_input(Some(new))?;
    match old {
        Some(old) => {
            anyhow::ensure!(rev.is_none(), "--rev compares one file with a commit");
            Ok((read_input(Some(old))?, old.display().to_string(), new_input))
        }
        None => {
            let rev = rev.unwrap_or("HEAD");
            let text = crate::git::show(new, rev)?;
            let old = Input {
                // Named like the new file, so its language is found the same.
                path: Some(new.to_owned()),
                text,
            };
            Ok((old, format!("{}@{rev}", new.display()), new_input))
        }
    }
}

/// A change as the CLI reports it: one-based lines, in the file each is in.
#[derive(Serialize)]
pub(crate) struct ReportedChange<'a> {
    change: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<Severity>,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    old: Option<At>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new: Option<At>,
}

#[derive(Serialize)]
pub(crate) struct At {
    file: String,
    line: usize,
}

pub(crate) fn reported_changes<'a>(
    old: &str,
    new: &str,
    changes: &'a [omaspec_model::Change],
) -> Vec<ReportedChange<'a>> {
    changes
        .iter()
        .map(|c| ReportedChange {
            change: c.kind.name(),
            severity: c.severity,
            message: &c.message,
            old: c.old_line.map(|l| At {
                file: old.to_owned(),
                line: l + 1,
            }),
            new: c.new_line.map(|l| At {
                file: new.to_owned(),
                line: l + 1,
            }),
        })
        .collect()
}

pub fn diff(
    files: &[PathBuf],
    rev: Option<&str>,
    forced: Option<Language>,
    (json, output): (bool, Option<&Path>),
    project: Option<&Path>,
) -> Result<ExitCode> {
    if files.iter().any(|f| f.is_dir()) {
        anyhow::ensure!(
            output.is_none(),
            "-o draws one state machine's changes; a folder's are listed"
        );
        anyhow::ensure!(
            files.iter().all(|f| f.is_dir()),
            "diff compares two files or two folders"
        );
        anyhow::ensure!(
            forced.is_none(),
            "--lang is for a diagram file; a folder's files say their languages"
        );
        anyhow::ensure!(
            project.is_none(),
            "a folder is its own project; --project is for a diagram file"
        );
        return crate::history::diff(files, rev, json);
    }
    let (old, new) = match files {
        [new] => (None, new.as_path()),
        [old, new] => (Some(old.as_path()), new.as_path()),
        _ => anyhow::bail!("diff compares two versions"),
    };
    let (old, old_name, new) = versions(old, new, rev)?;
    crate::document::one_diagram(&new, "diff")?;
    let specs = Projects::default().of(new.path.as_deref(), project).clone();
    let changes = omaspec_model::diff(
        &old.text,
        language(&old, forced),
        &new.text,
        language(&new, forced),
        &specs,
    )
    .map_err(|reason| anyhow::anyhow!(reason))?;
    let new_name = new.name();
    if let Some(output) = output {
        crate::diff_picture::draw(
            (&old.text, language(&old, forced)),
            (&new.text, language(&new, forced)),
            new.path.as_deref(),
            &changes,
            output,
        )?;
    }
    let mut out = stdout();
    if json {
        let reported = reported_changes(&old_name, &new_name, &changes);
        match output {
            Some(output) => serde_json::to_writer_pretty(
                &mut out,
                &serde_json::json!({ "changes": reported, "image": output.display().to_string() }),
            )?,
            None => serde_json::to_writer_pretty(&mut out, &reported)?,
        }
        writeln!(out)?;
    } else {
        for c in &changes {
            writeln!(out, "{}", change_line(&old_name, &new_name, c))?;
        }
    }
    Ok(if changes.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// A change as `diff` prints it, `file:line: kind: message`, on its line
/// in the new version (`new`), else in the old one (`old`).
pub(crate) fn change_line(old: &str, new: &str, c: &omaspec_model::Change) -> String {
    let at = match (c.new_line, c.old_line) {
        (Some(line), _) => format!("{new}:{}", line + 1),
        (None, Some(line)) => format!("{old}:{}", line + 1),
        (None, None) => new.to_owned(),
    };
    let what = match c.severity {
        Some(severity) => format!("{} {}", c.kind.name(), severity_name(severity)),
        None => c.kind.name().to_owned(),
    };
    format!("{at}: {what}: {}", c.message)
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

/// The other language, the one a diagram in `lang` converts to.
pub(crate) fn other(lang: Language) -> Language {
    match lang {
        Language::PlantUml => Language::Mermaid,
        Language::Mermaid => Language::PlantUml,
    }
}

pub fn convert(
    file: Option<PathBuf>,
    output: Option<&Path>,
    forced: Option<Language>,
    json: bool,
) -> Result<ExitCode> {
    let input = read_input(file.as_deref().filter(|f| f.as_os_str() != "-"))?;
    crate::document::one_diagram(&input, "convert")?;
    let from = language(&input, forced);
    let to = other(from);
    let result = omaspec_model::convert(&input.text, from, to);
    let name = input.name();
    if let (Ok(converted), Some(path)) = (&result, output) {
        if let Some(source) = &input.path {
            anyhow::ensure!(
                std::path::absolute(source)? != std::path::absolute(path)?,
                "{} would overwrite the diagram itself",
                path.display()
            );
        }
        write_atomic(path, converted.code.as_bytes())?;
    }
    let mut out = stdout();
    if json {
        let report = match &result {
            Ok(converted) => serde_json::json!({
                "ok": true,
                "language": to,
                "output": output.map(|p| p.display().to_string()),
                "code": output.is_none().then_some(&converted.code),
                "notices": converted.notices.iter().map(|n| serde_json::json!({
                    "line": n.line.map(|l| l + 1),
                    "message": n.message,
                })).collect::<Vec<_>>(),
            }),
            Err(refused) => serde_json::json!({
                "ok": false,
                "line": refused.line.map(|l| l + 1),
                "reason": refused.reason,
            }),
        };
        serde_json::to_writer_pretty(&mut out, &report)?;
        writeln!(out)?;
    }
    match result {
        Ok(converted) => {
            if !json {
                for n in &converted.notices {
                    let at = n.line.map(|l| format!(":{}", l + 1)).unwrap_or_default();
                    eprintln!("{name}{at}: note: {}", n.message);
                }
                match output {
                    Some(path) => eprintln!("{name} → {}", path.display()),
                    None => out.write_all(converted.code.as_bytes())?,
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(refused) => {
            if !json {
                let at = refused
                    .line
                    .map(|l| format!(":{}", l + 1))
                    .unwrap_or_default();
                eprintln!("{name}{at}: error: {}", refused.reason);
            }
            Ok(ExitCode::FAILURE)
        }
    }
}

/// What `fmt` does beside indenting.
#[derive(Debug, Clone, Copy, Default)]
pub struct Also {
    /// A Mermaid flowchart the way mermaid.js reads it too.
    pub portable: bool,
    /// A state machine or a scenario written evenly.
    pub pretty: bool,
}

pub fn fmt(
    files: &[PathBuf],
    forced: Option<Language>,
    write: bool,
    check: bool,
    also: Also,
) -> Result<ExitCode> {
    let inputs = read_inputs(files)?;
    anyhow::ensure!(
        !write || inputs.iter().all(|i| i.path.is_some()),
        "--write needs files, not standard input"
    );
    anyhow::ensure!(
        write || check || inputs.len() == 1,
        "formatting several files needs --write or --check"
    );
    let mut unformatted = false;
    let mut out = stdout();
    for input in &inputs {
        let formatted = crate::document::formatted(input, forced, also);
        let changed = formatted != input.text;
        if check {
            if changed {
                unformatted = true;
                writeln!(out, "{}", input.name())?;
            }
        } else if write {
            if let (true, Some(path)) = (changed, &input.path) {
                write_atomic(path, formatted.as_bytes())?;
                writeln!(out, "formatted {}", path.display())?;
            }
        } else {
            out.write_all(formatted.as_bytes())?;
        }
    }
    Ok(if unformatted {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

pub fn outline(file: Option<PathBuf>, forced: Option<Language>, json: bool) -> Result<ExitCode> {
    let input = read_input(file.as_deref().filter(|f| f.as_os_str() != "-"))?;
    let entries = crate::document::outline(&input, forced);
    let mut out = stdout();
    if json {
        serde_json::to_writer_pretty(&mut out, &one_based(&entries))?;
        writeln!(out)?;
    } else {
        for e in &entries {
            let text = match (e.tag.as_str(), e.label.as_str()) {
                ("", label) => label.to_owned(),
                (tag, "") => tag.to_owned(),
                (tag, label) => format!("{tag} {label}"),
            };
            writeln!(out, "{:>5}  {}{text}", e.line + 1, "  ".repeat(e.depth))?;
        }
    }
    if outline_cut(&entries) {
        eprintln!(
            "the outline lists only its first {} entries",
            outline::MAX_LISTED
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Whether `entries` are only an outline's first ones.
pub(crate) fn outline_cut(entries: &[Entry]) -> bool {
    entries.len() >= outline::MAX_LISTED
}

pub(crate) fn one_based(entries: &[Entry]) -> Vec<Entry> {
    entries
        .iter()
        .map(|e| Entry {
            line: e.line + 1,
            ..e.clone()
        })
        .collect()
}

#[derive(Serialize)]
pub(crate) struct Inspection<'a> {
    file: String,
    language: Language,
    kind: DiagramKind,
    findings: Vec<Reported<'a>>,
    pub(crate) outline: Vec<Entry>,
    /// Whether the outline holds only its first entries
    /// ([`outline::MAX_LISTED`]).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(crate) outline_cut: bool,
    /// Whether Omaspec's own engine draws the diagram. When it does not,
    /// Mermaid still draws through mmdr and PlantUML through Java, if it
    /// is installed; the reason says which.
    #[serde(skip_serializing_if = "Option::is_none")]
    native: Option<Native>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Native {
    Understood,
    Declined {
        /// One-based. Absent when no single line is to blame.
        #[serde(skip_serializing_if = "Option::is_none")]
        line: Option<usize>,
        reason: String,
    },
}

fn native(code: &str, lang: Language) -> Native {
    match lang {
        Language::PlantUml => match omaspec_uml::render(code) {
            Ok(_) => Native::Understood,
            Err(declined) => Native::Declined {
                line: declined.line.map(|l| l + 1),
                reason: declined.reason,
            },
        },
        Language::Mermaid => match mermaid_coverage(code) {
            Ok(None) => Native::Understood,
            Ok(Some(declined)) => Native::Declined {
                line: declined.line.map(|l| l + 1),
                reason: declined.reason,
            },
            Err(err) => {
                let (line, _, reason) = failure(&err);
                Native::Declined { line, reason }
            }
        },
    }
}

/// The line (one-based), the engine name, and the message `render` prints
/// after `error:`.
pub(crate) fn failure(err: &RenderError) -> (Option<usize>, Option<&'static str>, String) {
    match err {
        RenderError::Diagram {
            engine,
            line,
            message,
        } => (
            line.map(|l| l + 1),
            Some(engine.name()),
            format!("{message} ({})", engine.name()),
        ),
        RenderError::Unavailable { line, reason } => (line.map(|l| l + 1), None, reason.clone()),
        RenderError::Engine { engine, message } => (
            None,
            Some(engine.name()),
            format!("{}: {message}", engine.name()),
        ),
    }
}

pub fn inspect(
    file: Option<PathBuf>,
    forced: Option<Language>,
    project: Option<&Path>,
) -> Result<ExitCode> {
    let input = read_input(file.as_deref().filter(|f| f.as_os_str() != "-"))?;
    let specs = Projects::default()
        .of(input.path.as_deref(), project)
        .clone();
    let mut out = stdout();
    if crate::document::is_document(&input) {
        // Each diagram of the document, on its lines.
        let pieces = crate::document::pieces(&input, None);
        let findings: Vec<Vec<Finding>> = pieces.iter().map(|p| p.findings(&specs)).collect();
        let document = crate::document::inspect_document(&input, &pieces, &findings);
        serde_json::to_writer_pretty(&mut out, &document)?;
        writeln!(out)?;
        return Ok(ExitCode::SUCCESS);
    }
    let lang = language(&input, forced);
    let findings = crate::document::findings_of(&input.text, lang, &specs);
    let inspection = inspection(&input.name(), &input.text, lang, &findings);
    serde_json::to_writer_pretty(&mut out, &inspection)?;
    writeln!(out)?;
    Ok(ExitCode::SUCCESS)
}

/// What `inspect` reports on the diagram `file`, given its findings.
pub(crate) fn inspection<'a>(
    file: &str,
    code: &str,
    lang: Language,
    findings: &'a [Finding],
) -> Inspection<'a> {
    let entries = outline::outline(code, lang);
    Inspection {
        file: file.to_owned(),
        language: lang,
        kind: classify(code, lang),
        findings: reported(file, findings),
        outline_cut: outline_cut(&entries),
        outline: one_based(&entries),
        native: Some(native(code, lang)),
    }
}

/// Renderer settings; `$OMASPEC_PLANTUML` names the `plantuml` command,
/// `none` turns the Java fallback off. On Linux the Java PlantUML runs
/// confined through `omaspec __sandbox` unless `$OMASPEC_SANDBOX` is
/// `off`; elsewhere there is no Landlock, and PlantUML's own SANDBOX
/// profile is the only fence.
pub(crate) fn render_config() -> Config {
    let mut config = Config::default();
    if cfg!(target_os = "linux") && std::env::var_os("OMASPEC_SANDBOX").is_none_or(|v| v != "off") {
        config.sandbox = std::env::current_exe().ok();
    }
    match std::env::var_os("OMASPEC_PLANTUML") {
        Some(v) if v.is_empty() || v == "none" => config.java_fallback = false,
        Some(v) => config.plantuml = Some(v.into()),
        None => {}
    }
    config
}

/// Milliseconds with one decimal, the same rounding as the status line.
pub(crate) fn millis(elapsed: std::time::Duration) -> f64 {
    (elapsed.as_secs_f64() * 1000.0 * 10.0).round() / 10.0
}

pub fn extract(image: &Path, output: Option<&Path>, force: bool) -> Result<ExitCode> {
    let bytes = fs::read(image).with_context(|| image.display().to_string())?;
    let source = if bytes.starts_with(b"\x89PNG") {
        embed::source_from_png(&bytes)
    } else {
        std::str::from_utf8(&bytes)
            .ok()
            .and_then(embed::source_from_svg)
    };
    let Some(source) = source else {
        eprintln!("omaspec: no diagram source in {}", image.display());
        return Ok(ExitCode::FAILURE);
    };
    match output {
        Some(path) => {
            anyhow::ensure!(
                force || !path.exists(),
                "{} exists; use --force to overwrite it",
                path.display()
            );
            write_atomic(path, source.code.as_bytes())?;
            if let Some(name) = &source.file {
                eprintln!("source of {name} written to {}", path.display());
            }
        }
        None => stdout().write_all(source.code.as_bytes())?,
    }
    Ok(ExitCode::SUCCESS)
}

/// `omaspec new`: makes `dir` a project and says what it made.
pub fn new_project(dir: &Path, language: Language, git: bool, json: bool) -> Result<ExitCode> {
    use crate::project::{Git, Hook, create};
    let made = create(dir, language, git)?;
    let mut out = stdout();
    if json {
        let mut value = serde_json::json!({
            "root": made.root,
            "name": made.name,
            "file": omaspec_core::project::FILE,
            "machine": made.machine,
            "wrote_machine": made.wrote_machine,
        });
        value["git"] = match &made.git {
            Git::No => serde_json::json!(null),
            Git::Initialized => serde_json::json!({
                "initialized": true,
                "hook": match &made.hook {
                    Some(Hook::Written) => "written",
                    Some(Hook::Already) => "already",
                    Some(Hook::Failed(_)) => "failed",
                    None => "none",
                },
            }),
            Git::Inside(repo) => serde_json::json!({ "initialized": false, "inside": repo }),
            Git::Failed(why) => serde_json::json!({ "initialized": false, "error": why }),
        };
        writeln!(out, "{}", serde_json::to_string_pretty(&value)?)?;
    } else {
        let path = dir.join(&made.machine);
        if made.wrote_machine {
            writeln!(out, "created {}, the first state machine", path.display())?;
        } else {
            writeln!(out, "kept {}", path.display())?;
        }
        let file = dir.join(omaspec_core::project::FILE);
        writeln!(
            out,
            "created {}; beside it stands the project's map, made from its diagrams",
            file.display()
        )?;
        match &made.git {
            Git::No | Git::Failed(_) => {}
            Git::Initialized => {
                writeln!(out, "started a git repository in {}", dir.display())?;
                let hook = dir.join(".git/hooks/pre-commit");
                match &made.hook {
                    Some(Hook::Written) => {
                        writeln!(out, "wrote {}, a pre-commit hook", hook.display())?;
                    }
                    Some(Hook::Already) => {
                        writeln!(out, "kept {}, a pre-commit hook", hook.display())?;
                    }
                    Some(Hook::Failed(why)) => {
                        eprintln!(
                            "omaspec: the repository is there, but no pre-commit hook: {why}"
                        );
                    }
                    None => {}
                }
            }
            Git::Inside(repo) => writeln!(
                out,
                "no git repository started: {} is in {} already",
                dir.display(),
                repo.display()
            )?,
        }
    }
    if let Git::Failed(why) = &made.git {
        eprintln!("omaspec: the project is there, but no git repository: {why}");
        return Ok(ExitCode::FAILURE);
    }
    Ok(ExitCode::SUCCESS)
}

/// The examples, or the source of the one named `name`.
pub fn example(name: Option<&str>, json: bool) -> Result<ExitCode> {
    use crate::gallery::{EXAMPLES, Example, find};
    let described = |e: &Example| {
        serde_json::json!({
            "file": e.file,
            "title": e.title,
            "group": e.group.name(),
            "complex": e.complex,
            "language": e.language().name(),
            "about": e.about,
        })
    };
    let mut out = stdout();
    let Some(name) = name else {
        if json {
            let all: Vec<_> = EXAMPLES.iter().map(described).collect();
            writeln!(out, "{}", serde_json::to_string_pretty(&all)?)?;
        } else {
            let wide = EXAMPLES.iter().map(|e| e.file.len()).max().unwrap_or(0);
            for e in EXAMPLES {
                let complex = if e.complex { "complex" } else { "" };
                writeln!(
                    out,
                    "{:wide$}  {:9}  {:7}  {} — {}",
                    e.file,
                    e.group.name(),
                    complex,
                    e.title,
                    e.about
                )?;
            }
        }
        return Ok(ExitCode::SUCCESS);
    };
    let example = find(name)
        .with_context(|| format!("no example is named {name}; `omaspec example` lists them"))?;
    if json {
        let mut value = described(example);
        value["text"] = example.text.into();
        writeln!(out, "{}", serde_json::to_string_pretty(&value)?)?;
    } else {
        write!(out, "{}", example.text)?;
    }
    Ok(ExitCode::SUCCESS)
}

pub fn theme(file: Option<PathBuf>) -> Result<ExitCode> {
    let mut tweaks = omaspec_theme::Tweaks::default();
    let (title, palette) = match file {
        Some(path) => (path.display().to_string(), Palette::load(&path)?),
        None => {
            let theme = Theme::load_current();
            tweaks = theme.tweaks;
            let title = match &theme.source {
                Source::Omarchy { colors_file } => format!(
                    "{} ({})",
                    theme.name.as_deref().unwrap_or("unnamed"),
                    colors_file.display()
                ),
                Source::Fallback { reason } => format!("built-in fallback: {reason}"),
            };
            (title, theme.palette)
        }
    };
    let mut out = stdout();
    writeln!(out, "{title}, {:?}", palette.mode)?;
    for (name, color) in palette_rows(&palette) {
        writeln!(out, "{} {color}  {name}", swatch(color))?;
    }
    if let Some(file) = &tweaks.file {
        writeln!(out, "\nadjusted by {}", file.display())?;
        for (section, colors) in [("syntax", &tweaks.syntax), ("marks", &tweaks.marks)] {
            let mut names: Vec<_> = colors.iter().collect();
            names.sort_by_key(|(name, _)| name.as_str());
            for (name, color) in names {
                writeln!(out, "{} {color}  {section}.{name}", swatch(*color))?;
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// A two-cell truecolor block.
fn swatch(c: Rgb) -> String {
    format!("\x1b[48;2;{};{};{}m  \x1b[0m", c.r, c.g, c.b)
}

fn palette_rows(p: &Palette) -> [(&'static str, Rgb); 26] {
    [
        ("accent", p.accent),
        ("selection", p.selection),
        ("selection_foreground", p.selection_foreground),
        ("muted", p.muted),
        ("background", p.background),
        ("dark_background", p.dark_background),
        ("darker_background", p.darker_background),
        ("lighter_background", p.lighter_background),
        ("foreground", p.foreground),
        ("dark_foreground", p.dark_foreground),
        ("light_foreground", p.light_foreground),
        ("bright_foreground", p.bright_foreground),
        ("red", p.red),
        ("yellow", p.yellow),
        ("orange", p.orange),
        ("green", p.green),
        ("cyan", p.cyan),
        ("blue", p.blue),
        ("magenta", p.magenta),
        ("brown", p.brown),
        ("bright_red", p.bright_red),
        ("bright_yellow", p.bright_yellow),
        ("bright_green", p.bright_green),
        ("bright_cyan", p.bright_cyan),
        ("bright_blue", p.bright_blue),
        ("bright_magenta", p.bright_magenta),
    ]
}
