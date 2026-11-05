//! Where a name is used in a project, for the command line and the MCP
//! server (`docs/adr/0019-where-a-name-is-used.md`): the project's files
//! read, `omaspec_model::refs` asked, the uses printed.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use omaspec_core::{Language, detect, markdown};
use omaspec_model::{File, FileKind, Ref, Spec, Specs};
use serde_json::{Value, json};

use crate::editor::folder::scan_for;
use crate::specs::file_name;

/// A diagram or a document may hold at most this many bytes, as for
/// `!include`; a trace as many as the command line reads.
const MAX_FILE: u64 = 1 << 20;
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
    let wanted = |name: &str| {
        Language::from_file_name(name).is_some()
            || markdown::is_markdown(name)
            || name.ends_with(".jsonl")
    };
    scan_for(root, wanted)
        .into_iter()
        .filter_map(|relative| {
            let path = root.join(&relative);
            let name = file_name(&relative);
            let trace = name.ends_with(".jsonl");
            let most = if trace { MAX_TRACE } else { MAX_FILE };
            let small = std::fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= most);
            let text = small
                .then(|| std::fs::read_to_string(&path).ok())
                .flatten()?;
            let kind = if trace {
                FileKind::Trace
            } else if markdown::is_markdown(&name) {
                FileKind::Document
            } else {
                FileKind::Diagram(detect(&text, Language::from_file_name(&name)))
            };
            Some(Owned {
                path: name,
                text,
                kind,
            })
        })
        .collect()
}

/// The state machines of `files`, as the project's index reads them.
pub fn specs_of(files: &[Owned]) -> Specs {
    files
        .iter()
        .flat_map(|f| match f.kind {
            FileKind::Diagram(language) => Spec::read(&f.text, language, &f.path)
                .into_iter()
                .collect::<Vec<_>>(),
            FileKind::Document => markdown::blocks(&f.text)
                .iter()
                .filter_map(|b| Spec::read_in_document(&b.code, b.language, &f.path))
                .collect(),
            FileKind::Trace => Vec::new(),
        })
        .collect()
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
