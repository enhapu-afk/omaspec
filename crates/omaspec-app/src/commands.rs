//! The CLI subcommands.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use omaspec_core::lint::{Finding, Severity};
use omaspec_core::outline::Entry;
use omaspec_core::{DiagramKind, Language, classify, detect, embed, format, outline};
use omaspec_render::export::{self, Format};
use omaspec_render::{Config, RenderError, Renderer, mermaid_coverage};
use omaspec_theme::{Palette, Rgb, Source, Theme};
use serde::Serialize;

use crate::files::{Input, copy_to_clipboard, read_input, read_inputs, write_atomic};
use crate::specs::Projects;

/// Standard output for the commands. When its reader stops early
/// (`omaspec example | head`), a write fails with [`ReaderGone`], which
/// `main` takes for the end of the command, not for an error.
fn stdout() -> Stdout {
    Stdout(io::stdout().lock())
}

struct Stdout(io::StdoutLock<'static>);

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
        })
        .collect()
}

pub fn lint(
    files: &[PathBuf],
    forced: Option<Language>,
    json: bool,
    strict: bool,
    project: Option<&Path>,
) -> Result<ExitCode> {
    let mut all: Vec<(String, Vec<Finding>)> = Vec::new();
    let mut projects = Projects::default();
    for input in read_inputs(files)? {
        let specs = projects.of(input.path.as_deref(), project);
        let findings = omaspec_model::findings_with(&input.text, language(&input, forced), specs);
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
            let text = crate::editor::compare::show(new, rev)?;
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
    json: bool,
    project: Option<&Path>,
) -> Result<ExitCode> {
    let (old, new) = match files {
        [new] => (None, new.as_path()),
        [old, new] => (Some(old.as_path()), new.as_path()),
        _ => anyhow::bail!("diff compares two versions"),
    };
    let (old, old_name, new) = versions(old, new, rev)?;
    let specs = Projects::default()
        .of(new.path.as_deref(), project)
        .to_vec();
    let changes = omaspec_model::diff(
        &old.text,
        language(&old, forced),
        &new.text,
        language(&new, forced),
        &specs,
    )
    .map_err(|reason| anyhow::anyhow!(reason))?;
    let new_name = new.name();
    let mut out = stdout();
    if json {
        serde_json::to_writer_pretty(&mut out, &reported_changes(&old_name, &new_name, &changes))?;
        writeln!(out)?;
    } else {
        for c in &changes {
            let at = match (c.new_line, c.old_line) {
                (Some(line), _) => format!("{new_name}:{}", line + 1),
                (None, Some(line)) => format!("{old_name}:{}", line + 1),
                (None, None) => new_name.clone(),
            };
            let what = match c.severity {
                Some(severity) => format!("{} {}", c.kind.name(), severity_name(severity)),
                None => c.kind.name().to_owned(),
            };
            writeln!(out, "{at}: {what}: {}", c.message)?;
        }
    }
    Ok(if changes.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
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

pub fn fmt(
    files: &[PathBuf],
    forced: Option<Language>,
    write: bool,
    check: bool,
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
        let formatted = format::format(&input.text, language(input, forced));
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
    let entries = outline::outline(&input.text, language(&input, forced));
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
    Ok(ExitCode::SUCCESS)
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
    outline: Vec<Entry>,
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
    let lang = language(&input, forced);
    let specs = Projects::default()
        .of(input.path.as_deref(), project)
        .to_vec();
    let findings = omaspec_model::findings_with(&input.text, lang, &specs);
    let inspection = inspection(&input.name(), &input.text, lang, &findings);
    let mut out = stdout();
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
    Inspection {
        file: file.to_owned(),
        language: lang,
        kind: classify(code, lang),
        findings: reported(file, findings),
        outline: one_based(&outline::outline(code, lang)),
        native: Some(native(code, lang)),
    }
}

pub struct RenderArgs<'a> {
    pub files: &'a [PathBuf],
    pub output: Option<&'a Path>,
    pub format: Option<Format>,
    pub scale: f32,
    /// Store the diagram source in SVG and PNG images.
    pub source: bool,
    pub copy: bool,
    pub lang: Option<Language>,
    /// A JSON report on stdout instead of status lines. The images are
    /// still written; stdout is not an image in this mode.
    pub json: bool,
}

/// Renderer settings; `$OMASPEC_PLANTUML` names the `plantuml` command,
/// `none` turns the Java fallback off. The Java PlantUML runs confined
/// through `omaspec __sandbox` unless `$OMASPEC_SANDBOX` is `off`.
pub(crate) fn render_config() -> Config {
    let mut config = Config::default();
    if std::env::var_os("OMASPEC_SANDBOX").is_none_or(|v| v != "off") {
        config.sandbox = std::env::current_exe().ok();
    }
    match std::env::var_os("OMASPEC_PLANTUML") {
        Some(v) if v.is_empty() || v == "none" => config.java_fallback = false,
        Some(v) => config.plantuml = Some(v.into()),
        None => {}
    }
    config
}

pub fn render(args: &RenderArgs) -> Result<ExitCode> {
    let inputs = read_inputs(args.files)?;
    anyhow::ensure!(
        args.output.is_none() || inputs.len() == 1,
        "--output works with one diagram only"
    );
    anyhow::ensure!(
        args.scale > 0.0 && args.scale <= 16.0,
        "--scale must be between 0 and 16"
    );
    let format = args
        .format
        .or_else(|| {
            let ext = args.output?.extension()?.to_str()?;
            Format::from_extension(ext)
        })
        .unwrap_or(Format::Svg);

    if args.json && !args.copy {
        let image_on_stdout = match args.output {
            Some(path) => path.as_os_str() == "-",
            None => inputs.iter().any(|input| input.path.is_none()),
        };
        anyhow::ensure!(
            !image_on_stdout,
            "--json writes the report to stdout; choose an output file"
        );
    }

    let mut renderer = Renderer::new(&render_config());
    let mut failed = false;
    let mut reports = Vec::new();
    for input in &inputs {
        let lang = language(input, args.lang);
        let place = input
            .path
            .as_deref()
            .and_then(|path| omaspec_render::Place::of(path, None));
        let rendered = match renderer.render_at(&input.text, lang, place.as_ref()) {
            Ok(r) => r,
            Err(err) => {
                failed = true;
                let (line, engine, message) = failure(&err);
                if args.json {
                    reports.push(Report {
                        file: input.name(),
                        ok: false,
                        output: None,
                        copied: false,
                        engine,
                        elapsed_ms: None,
                        line,
                        message: Some(message),
                    });
                } else {
                    let at = line.map(|l| format!(":{l}")).unwrap_or_default();
                    eprintln!("{}{at}: error: {message}", input.name());
                }
                continue;
            }
        };
        let options = export::Options {
            scale: args.scale,
            source: args.source.then(|| export::Source {
                code: &input.text,
                language: lang,
                file: input.file_name(),
            }),
        };
        let bytes = export::export(&rendered.svg, format, &options)?;
        let took = format!(
            "{}, {:.1} ms",
            rendered.engine.name(),
            rendered.elapsed.as_secs_f64() * 1000.0
        );

        if args.copy {
            copy_to_clipboard(&bytes, format.mime())?;
            if args.json {
                reports.push(Report {
                    file: input.name(),
                    ok: true,
                    output: None,
                    copied: true,
                    engine: Some(rendered.engine.name()),
                    elapsed_ms: Some(millis(rendered.elapsed)),
                    line: None,
                    message: None,
                });
            } else {
                eprintln!(
                    "{} copied as {} ({took})",
                    input.name(),
                    format.extension().to_uppercase()
                );
            }
            continue;
        }
        let dest = match (args.output, &input.path) {
            (Some(out), _) if out.as_os_str() != "-" => Some(out.to_owned()),
            (Some(_), _) | (None, None) => None,
            (None, Some(path)) => Some(path.with_extension(format.extension())),
        };
        match dest {
            Some(dest) => {
                anyhow::ensure!(
                    input.path.as_deref() != Some(dest.as_path()),
                    "{} would overwrite the diagram itself",
                    dest.display()
                );
                write_atomic(&dest, &bytes)?;
                if args.json {
                    reports.push(Report {
                        file: input.name(),
                        ok: true,
                        output: Some(dest.display().to_string()),
                        copied: false,
                        engine: Some(rendered.engine.name()),
                        elapsed_ms: Some(millis(rendered.elapsed)),
                        line: None,
                        message: None,
                    });
                } else {
                    eprintln!("{} → {} ({took})", input.name(), dest.display());
                }
            }
            None => stdout().write_all(&bytes)?,
        }
    }
    if args.json {
        let mut out = stdout();
        serde_json::to_writer_pretty(&mut out, &reports)?;
        writeln!(out)?;
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

#[derive(Serialize)]
struct Report {
    file: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(skip_serializing_if = "skip_false")]
    copied: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    engine: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_ms: Option<f64>,
    /// One-based, when the error names a line.
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

fn skip_false(copied: &bool) -> bool {
    !*copied
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
    use crate::project::{Git, create};
    let made = create(dir, language, git)?;
    let mut out = stdout();
    if json {
        let mut value = serde_json::json!({
            "root": made.root,
            "name": made.name,
            "file": omaspec_core::project::FILE,
            "main": made.main,
            "wrote_main": made.wrote_main,
        });
        value["git"] = match &made.git {
            Git::No => serde_json::json!(null),
            Git::Initialized => serde_json::json!({ "initialized": true }),
            Git::Inside(repo) => serde_json::json!({ "initialized": false, "inside": repo }),
            Git::Failed(why) => serde_json::json!({ "initialized": false, "error": why }),
        };
        writeln!(out, "{}", serde_json::to_string_pretty(&value)?)?;
    } else {
        let main = dir.join(&made.main);
        if made.wrote_main {
            writeln!(out, "created {}", main.display())?;
        } else {
            writeln!(out, "kept {}, the main diagram", main.display())?;
        }
        let file = dir.join(omaspec_core::project::FILE);
        writeln!(out, "created {}", file.display())?;
        match &made.git {
            Git::No | Git::Failed(_) => {}
            Git::Initialized => writeln!(out, "started a git repository in {}", dir.display())?,
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
