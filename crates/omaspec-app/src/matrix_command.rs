//! `omaspec matrix`: a state machine as a table, a row for each state, a
//! column for each event (`omaspec_model::matrix`); Markdown on standard
//! output or into a file, a picture in the look of the machine
//! (`omaspec_render::render_table`), or JSON.

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
        let ext = output
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "md" {
            fs::write(output, table.markdown(&m))
                .with_context(|| format!("writing {}", output.display()))?;
        } else {
            draw(&input.text, lang, input.path.as_deref(), output)?;
        }
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

/// The state machine `source` (at `file`) as a table, drawn in the look of
/// its states into `output`, a .svg, .png or .pdf.
pub(crate) fn draw(
    source: &str,
    lang: omaspec_core::Language,
    file: Option<&Path>,
    output: &Path,
) -> Result<()> {
    use omaspec_render::export::{self, Format};
    let format = output
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(Format::from_extension)
        .filter(|format| matches!(format, Format::Svg | Format::Png | Format::Pdf))
        .with_context(|| format!("{} is no .md, .svg, .png or .pdf", output.display()))?;
    let place = file.and_then(|file| omaspec_render::Place::of(file, None));
    let rendered = omaspec_render::render_table(source, lang, place.as_ref())
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let bytes = export::export(&rendered.svg, format, &export::Options::default())?;
    fs::write(output, bytes).with_context(|| format!("writing {}", output.display()))?;
    Ok(())
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
