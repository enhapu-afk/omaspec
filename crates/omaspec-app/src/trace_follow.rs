//! `omaspec trace run.jsonl --follow`: a trace held against the state
//! machines of its project while it is written (docs/trace.md,
//! docs/adr/0036). Each finding is said the moment its line is complete,
//! so an orchestrator, a person at a terminal or an agent reading the
//! output sees a deviation as it happens; with `--json` one object a line.
//!
//! A file is read from where the last reading ended, every
//! [`POLL`]; one that is not there yet is waited for, one that grows
//! shorter (written anew) is followed from its start again. It runs until
//! it is stopped. Standard input is followed until it ends. Nothing but
//! the file is read: no network, no watcher of the folder.

use std::fs::File;
use std::io::{BufRead, Read, Seek, SeekFrom, Write, stdin, stdout};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use omaspec_core::lint::{Finding, Severity};
use omaspec_model::TraceChecking;

use crate::commands::reported;
use crate::specs::Projects;

/// How often a followed file is looked at.
const POLL: Duration = Duration::from_millis(100);

/// Follows the trace `file` (standard input for `None` or `-`), against
/// the machines of its project or of `project`.
pub fn trace(
    file: Option<&Path>,
    json: bool,
    strict: bool,
    project: Option<&Path>,
) -> Result<ExitCode> {
    let file = file.filter(|path| path.as_os_str() != "-");
    let mut projects = Projects::default();
    let specs = projects.of(file, project);
    let name = file.map_or_else(|| "<stdin>".to_owned(), |path| path.display().to_string());
    let mut said = Said {
        name,
        json,
        strict,
        failed: false,
    };
    let mut checking = TraceChecking::new(specs);
    match file {
        None => {
            for line in stdin().lock().lines() {
                let line = line.context("cannot read standard input")?;
                said.findings(&checking.line(&line))?;
            }
            Ok(if said.failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }
        Some(path) => {
            let mut read = 0u64;
            let mut rest = String::new();
            let mut waiting = false;
            loop {
                let Ok(mut opened) = File::open(path) else {
                    if !waiting {
                        eprintln!("waiting for {}", path.display());
                        waiting = true;
                    }
                    std::thread::sleep(POLL);
                    continue;
                };
                waiting = false;
                let len = opened.metadata().map(|m| m.len()).unwrap_or(read);
                // Written anew: followed from its start again.
                if len < read {
                    eprintln!(
                        "{} was written anew: following it from its start",
                        path.display()
                    );
                    read = 0;
                    rest.clear();
                    checking = TraceChecking::new(specs);
                }
                if len > read {
                    opened.seek(SeekFrom::Start(read))?;
                    let mut bytes = Vec::new();
                    opened.take(len - read).read_to_end(&mut bytes)?;
                    read += bytes.len() as u64;
                    rest.push_str(&String::from_utf8_lossy(&bytes));
                    // Only lines with their end: the last may be half
                    // written.
                    while let Some(end) = rest.find('\n') {
                        let line: String = rest.drain(..=end).collect();
                        let line = line.trim_end_matches(['\n', '\r']);
                        said.findings(&checking.line(line))?;
                    }
                }
                std::thread::sleep(POLL);
            }
        }
    }
}

/// Where the findings go, and whether one fails the trace.
struct Said {
    name: String,
    json: bool,
    strict: bool,
    failed: bool,
}

impl Said {
    /// Says `findings`, each on a line of its own, at once.
    fn findings(&mut self, findings: &[Finding]) -> Result<()> {
        if findings.is_empty() {
            return Ok(());
        }
        let mut out = stdout().lock();
        for f in findings {
            self.failed |=
                f.severity == Severity::Error || (self.strict && f.severity == Severity::Warning);
            if self.json {
                for one in reported(&self.name, std::slice::from_ref(f)) {
                    serde_json::to_writer(&mut out, &one)?;
                    writeln!(out)?;
                }
            } else {
                let severity = match f.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Info => "info",
                };
                writeln!(
                    out,
                    "{}:{}: {severity}: {}",
                    self.name,
                    f.line + 1,
                    f.message
                )?;
            }
        }
        out.flush()?;
        Ok(())
    }
}
