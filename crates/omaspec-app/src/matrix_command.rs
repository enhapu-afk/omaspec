//! `omaspec matrix`: a state machine as a table, a row for each state, a
//! column for each event (`omaspec_model::matrix`); Markdown on standard
//! output or into a file, or JSON.

use std::fs;
use std::io::{Write, stdout};
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use omaspec_model::{Machine, Matrix, matrix};
use serde_json::{Value, json};

use crate::commands::language;
use crate::files::read_input;

pub fn run(file: &Path, output: Option<&Path>, json: bool) -> Result<ExitCode> {
    let input = read_input(Some(file))?;
    let lang = language(&input, None);
    let Some(m) = Machine::read(&input.text, lang) else {
        bail!(
            "{} is no state machine Omaspec reads, so it has no table",
            file.display()
        );
    };
    let table = matrix(&m);
    if let Some(output) = output {
        let markdown = output
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if !markdown {
            bail!(
                "{} is no .md file; the table is written as Markdown",
                output.display()
            );
        }
        fs::write(output, table.markdown(&m))
            .with_context(|| format!("writing {}", output.display()))?;
    }
    let mut out = stdout();
    if json {
        let mut report = report(&m, &table);
        report["file"] = json!(file.display().to_string());
        serde_json::to_writer_pretty(&mut out, &report)?;
        writeln!(out)?;
    } else if output.is_none() {
        write!(out, "{}", table.markdown(&m))?;
    }
    Ok(ExitCode::SUCCESS)
}

/// The table as the JSON report and the MCP tool give it: the states, the
/// events, and every cell with what it fires, one-based lines.
pub(crate) fn report(m: &Machine, table: &Matrix) -> Value {
    let name = |s: usize| match m.states[s].name.as_str() {
        "" => "[*]".to_owned(),
        name => name.to_owned(),
    };
    let mut cells = Vec::new();
    for (row, &state) in table.states.iter().enumerate() {
        for (column, event) in table.events.iter().enumerate() {
            let cell = &table.cells[row][column];
            let ways: Vec<Value> = cell
                .ways
                .iter()
                .map(|&t| {
                    let t = &m.transitions[t];
                    json!({
                        "to": name(t.to),
                        "guard": t.trigger.guard,
                        "action": t.trigger.action,
                        "line": t.line + 1,
                    })
                })
                .collect();
            let mut said = json!({
                "state": name(state),
                "event": event,
                "kind": cell.kind.name(),
                "transitions": ways,
            });
            if let Some(from) = cell.from {
                said["from"] = json!(name(from));
            }
            cells.push(said);
        }
    }
    json!({
        "states": table.states.iter().map(|&s| name(s)).collect::<Vec<_>>(),
        "events": table.events,
        "cells": cells,
    })
}
