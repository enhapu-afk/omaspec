//! Where a name is used in a project, for the command line and the MCP
//! server (`docs/adr/0019-where-a-name-is-used.md`): the project's files
//! read, `omaspec_model::refs` asked, the uses printed.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use omaspec_core::{Language, detect, markdown};
use omaspec_model::{File, FileKind, Ref, Specs};
use serde_json::{Value, json};

use crate::editor::folder::{MAX_FILES, scan_for, walked};
use crate::specs::file_name;

/// A diagram or a document may hold at most this many bytes, as for
/// `!include`; a trace as many as the command line reads.
pub const MAX_FILE: u64 = 1 << 20;
const MAX_TRACE: u64 = 16 << 20;

/// A file of a project, read.
pub struct Owned {
    pub path: String,
    pub text: String,
    pub kind: FileKind,
}

impl Owned {
    pub fn file(&self) -> File<'_> {
        File {
            path: &self.path,
            text: &self.text,
            kind: self.kind,
        }
    }
}

/// The diagrams, documents and traces of the project at `root`.
pub fn project_files(root: &Path) -> Vec<Owned> {
    read_files(root, true)
}

/// The diagrams and documents of the project at `root`, as its index
/// reads them.
pub fn diagram_files(root: &Path) -> Vec<Owned> {
    read_files(root, false)
}

/// [`diagram_files`] as of the git revision `rev`: those the folder held
/// then, under the same rules.
pub fn diagram_files_at(root: &Path, rev: &str) -> Result<Vec<Owned>> {
    let keep = |path: &str, size: u64| {
        let name = path.rsplit('/').next().unwrap_or(path);
        size <= MAX_FILE && walked(path) && is_diagram(name)
    };
    let mut files = crate::git::files_at(root, rev, keep)?;
    files.sort_by(|a, b| Path::new(&a.0).cmp(Path::new(&b.0)));
    files.truncate(MAX_FILES);
    Ok(files
        .into_iter()
        .map(|(path, text)| owned(path, text))
        .collect())
}

fn is_diagram(name: &str) -> bool {
    Language::from_file_name(name).is_some() || markdown::is_markdown(name)
}

fn read_files(root: &Path, traces: bool) -> Vec<Owned> {
    let wanted = |name: &str| is_diagram(name) || (traces && name.ends_with(".jsonl"));
    scan_for(root, wanted)
        .into_iter()
        .filter_map(|relative| {
            let path = root.join(&relative);
            let name = file_name(&relative);
            let most = if name.ends_with(".jsonl") {
                MAX_TRACE
            } else {
                MAX_FILE
            };
            let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= most);
            let text = small
                .then(|| std::fs::read_to_string(&path).ok())
                .flatten()?;
            Some(owned(name, text))
        })
        .collect()
}

/// The file at `path` in a project, its kind told by its name and its text.
pub fn owned(path: String, text: String) -> Owned {
    let kind = if path.ends_with(".jsonl") {
        FileKind::Trace
    } else if markdown::is_markdown(&path) {
        FileKind::Document
    } else {
        FileKind::Diagram(detect(&text, Language::from_file_name(&path)))
    };
    Owned { path, text, kind }
}

/// The state machines of `files`, as the project's index reads them.
pub fn specs_of(files: &[Owned]) -> Specs {
    Specs::of_files(&files.iter().map(Owned::file).collect::<Vec<_>>())
}

/// The uses of `name` in the project at `root`; with `machine`, only that
/// machine's.
pub fn refs_in(root: &Path, name: &str, machine: Option<&str>) -> Result<Vec<Ref>> {
    let files = project_files(root);
    let specs = specs_of(&files);
    let files: Vec<File> = files.iter().map(Owned::file).collect();
    omaspec_model::refs(name, machine, &files, &specs).map_err(|why| anyhow::anyhow!(why))
}

/// A use as JSON, its line one-based.
pub fn ref_json(r: &Ref) -> Value {
    json!({
        "file": r.file,
        "line": r.line + 1,
        "kind": r.kind.name(),
        "machine": r.machine,
        "text": r.text,
    })
}

/// The project folder a command works in: `project`, else the project the
/// current folder is in, else the current folder.
pub fn root(project: Option<&Path>) -> Result<PathBuf> {
    if let Some(project) = project {
        return std::path::absolute(project).context("no such folder");
    }
    let here = std::env::current_dir().context("no current folder")?;
    Ok(omaspec_render::project_root(&here).unwrap_or(here))
}

/// `omaspec refs`: every use of `name`, one a line as `lint` writes its
/// findings, or as JSON; exit 1 when nothing uses it.
pub fn refs_command(
    name: &str,
    machine: Option<&str>,
    project: Option<&Path>,
    json: bool,
) -> Result<ExitCode> {
    let root = root(project)?;
    let found = refs_in(&root, name, machine)?;
    let mut out = std::io::stdout().lock();
    if json {
        let all: Vec<Value> = found.iter().map(ref_json).collect();
        serde_json::to_writer_pretty(&mut out, &all)?;
        writeln!(out)?;
    } else {
        for r in &found {
            let of = r
                .machine
                .as_deref()
                .map(|m| format!(" of {m}"))
                .unwrap_or_default();
            writeln!(
                out,
                "{}:{}: {}{of}: {}",
                r.file,
                r.line + 1,
                r.kind.name(),
                r.text
            )?;
        }
        if found.is_empty() {
            eprintln!("nothing in {} uses {name}", root.display());
        }
    }
    Ok(if found.is_empty() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// What `omaspec rename` is told to rename, when the old name is several
/// things.
pub struct Which<'a> {
    pub machine: Option<&'a str>,
    pub thing: Option<omaspec_model::Thing>,
}

/// `omaspec rename`: `old` renamed to `new` at every use in the project,
/// each changed file written as a save writes it, unless `dry_run`; the
/// lines as they read afterwards, and what was left alone. Exit 1 when the
/// rename is refused.
pub fn rename_command(
    old: &str,
    new: &str,
    which: &Which,
    project: Option<&Path>,
    dry_run: bool,
    json: bool,
) -> Result<ExitCode> {
    let root = root(project)?;
    let owned = project_files(&root);
    let specs = specs_of(&owned);
    let files: Vec<File> = owned.iter().map(Owned::file).collect();
    let plan = match omaspec_model::rename(old, new, which.machine, which.thing, &files, &specs) {
        Ok(plan) => plan,
        Err(why) => {
            if json {
                println!("{}", json!({ "renamed": false, "error": why }));
            } else {
                eprintln!("not renamed: {why}");
            }
            return Ok(ExitCode::FAILURE);
        }
    };
    let mut lines: Vec<(String, usize, String)> = Vec::new();
    for file in plan.files() {
        let text = &owned
            .iter()
            .find(|f| f.path == file)
            .context("a file of the plan")?
            .text;
        let after = plan.apply(file, text);
        let changed: Vec<usize> = plan
            .edits
            .iter()
            .filter(|e| e.file == file)
            .map(|e| e.line)
            .collect();
        for (line, text) in after.split('\n').enumerate() {
            if changed.contains(&line) {
                lines.push((file.to_owned(), line, text.trim().to_owned()));
            }
        }
        if !dry_run {
            crate::files::write_atomic(&root.join(file), after.as_bytes())
                .with_context(|| format!("{file} not written"))?;
        }
    }
    let mut out = std::io::stdout().lock();
    if json {
        let value = json!({
            "renamed": !dry_run,
            "what": plan.what,
            "to": new,
            "files": plan.files(),
            "lines": lines
                .iter()
                .map(|(file, line, text)| json!({ "file": file, "line": line + 1, "text": text }))
                .collect::<Vec<_>>(),
            "traces": plan.traces.iter().map(ref_json).collect::<Vec<_>>(),
            "notes": plan.notes,
        });
        serde_json::to_writer_pretty(&mut out, &value)?;
        writeln!(out)?;
        return Ok(ExitCode::SUCCESS);
    }
    let count = plan.files().len();
    let files = if count == 1 { "file" } else { "files" };
    let verb = if dry_run { "would rename" } else { "renamed" };
    writeln!(out, "{verb} {} to {new} in {count} {files}:", plan.what)?;
    for (file, line, text) in &lines {
        writeln!(out, "{file}:{}: {text}", line + 1)?;
    }
    for r in &plan.traces {
        writeln!(
            out,
            "{}:{}: left alone, a recorded trace: {}",
            r.file,
            r.line + 1,
            r.text
        )?;
    }
    for note in &plan.notes {
        writeln!(out, "{note}")?;
    }
    Ok(ExitCode::SUCCESS)
}
