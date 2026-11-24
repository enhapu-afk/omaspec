//! `omaspec trace run.jsonl --follow`: a trace held against the state
//! machines of its project while it is written (docs/trace.md,
//! docs/adr/0036). Each finding is said the moment its line is complete,
//! so an orchestrator, a person at a terminal or an agent reading the
//! output sees a deviation as it happens; with `--json` one object a line.
//!
//! A file is read from where the last reading ended, every [`POLL`], as
//! bytes cut at the ends of lines, so a letter written across two reads
//! stays whole. One that is not there yet is waited for; one written anew,
//! shorter or another file by the same name, is followed from its start.
//! It runs until it is stopped. Standard input, and what is no file (a
//! pipe), is read as it streams, until it ends. Nothing but the file is
//! read: no network, no watcher of the folder.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write, stdin, stdout};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, Result};
use omaspec_core::lint::{Finding, Severity};
use omaspec_model::TraceChecking;

use crate::commands::{fails, reported};
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
    let streamed = |said: &mut Said, checking: &mut TraceChecking, input: &mut dyn BufRead| {
        let mut line = Vec::new();
        loop {
            line.clear();
            if input.read_until(b'\n', &mut line)? == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&line);
            said.findings(&checking.line(text.trim_end_matches(['\n', '\r'])))?;
        }
        Ok::<_, anyhow::Error>(if said.failed {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        })
    };
    let Some(path) = file else {
        return streamed(&mut said, &mut checking, &mut stdin().lock());
    };
    let mut read = 0u64;
    let mut which = None;
    let mut rest: Vec<u8> = Vec::new();
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
        let meta = opened
            .metadata()
            .with_context(|| format!("cannot read {}", path.display()))?;
        // A pipe streams; it is read as standard input is.
        if !meta.is_file() {
            return streamed(&mut said, &mut checking, &mut BufReader::new(opened));
        }
        let len = meta.len();
        let now = identity(&meta);
        if len < read || which.is_some_and(|was| was != now) {
            eprintln!(
                "{} was written anew: following it from its start",
                path.display()
            );
            read = 0;
            rest.clear();
            checking = TraceChecking::new(specs);
        }
        which = Some(now);
        if len > read {
            opened.seek(SeekFrom::Start(read))?;
            let before = rest.len();
            opened.take(len - read).read_to_end(&mut rest)?;
            read += (rest.len() - before) as u64;
            // Only lines with their end: the last may be half written.
            while let Some(end) = rest.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = rest.drain(..=end).collect();
                let text = String::from_utf8_lossy(&line);
                said.findings(&checking.line(text.trim_end_matches(['\n', '\r'])))?;
            }
        }
        std::thread::sleep(POLL);
    }
}

/// Which file `meta` is, so one put in its place by name is told apart:
/// its inode where the system has them.
#[cfg(unix)]
fn identity(meta: &std::fs::Metadata) -> u64 {
    std::os::unix::fs::MetadataExt::ino(meta)
}

#[cfg(not(unix))]
fn identity(_: &std::fs::Metadata) -> u64 {
    0
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
            self.failed |= fails(f, self.strict);
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
