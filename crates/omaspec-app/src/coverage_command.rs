//! `omaspec coverage`: how much of a state machine recorded traces take,
//! each transition and each state, as a report and as the diagram with
//! what was taken and what never was drawn over it. The source stays as
//! it is.

use std::collections::HashMap;
use std::fs;
use std::io::{Write, stdout};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use omaspec_model::{Coverage, Kind, Spec, Transition, coverage};
use omaspec_render::export::{self, Format};
use omaspec_uml::state::{Mark, Marks};
use serde::Serialize;

use crate::commands::language;
use crate::files::read_input;

pub struct Args<'a> {
    pub file: &'a Path,
    pub traces: &'a [PathBuf],
    pub output: Option<&'a Path>,
    pub json: bool,
    /// The share of transitions in percent below which it fails.
    pub min: Option<f32>,
}

/// What `coverage` finds, as `--json` prints it and the MCP tool answers.
#[derive(Serialize)]
pub(crate) struct Report {
    file: String,
    traces: Vec<String>,
    objects: Vec<String>,
    deviations: usize,
    covered_transitions: usize,
    total_transitions: usize,
    covered_states: usize,
    total_states: usize,
    transitions: Vec<TransitionReport>,
    states: Vec<StateReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    /// Whether as many transitions were taken as `min` asks for.
    pub(crate) passed: bool,
}

#[derive(Serialize)]
struct TransitionReport {
    /// One-based, as `lint` reports.
    line: usize,
    from: String,
    to: String,
    trigger: String,
    hits: u32,
}

#[derive(Serialize)]
struct StateReport {
    name: String,
    line: usize,
    hits: u32,
}

pub fn run(args: &Args) -> Result<ExitCode> {
    let report = measure(args)?;
    let mut out = stdout();
    if args.json {
        serde_json::to_writer_pretty(&mut out, &report)?;
        writeln!(out)?;
    } else {
        let file = &report.file;
        let objects = report.objects.len();
        writeln!(
            out,
            "{file}: {} of {} transitions taken ({:.0} %), {} of {} states, by {objects} {}{}",
            report.covered_transitions,
            report.total_transitions,
            percent(report.covered_transitions, report.total_transitions),
            report.covered_states,
            report.total_states,
            if objects == 1 { "object" } else { "objects" },
            match report.deviations {
                0 => String::new(),
                1 => "; 1 line the machine cannot take (omaspec trace says which)".to_owned(),
                n => format!("; {n} lines the machine cannot take (omaspec trace says which)"),
            }
        )?;
        for t in report.transitions.iter().filter(|t| t.hits == 0) {
            let arrow = format!("{} --> {}", t.from, t.to);
            let said = if t.trigger.is_empty() {
                arrow
            } else {
                format!("{arrow} : {}", t.trigger)
            };
            writeln!(out, "{file}:{}: never taken: {said}", t.line)?;
        }
        if let Some(min) = args.min.filter(|_| !report.passed) {
            writeln!(out, "{file}: below the {min} % asked for")?;
        }
    }
    Ok(if report.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// Follows the traces through the machine in `args.file` and, with an
/// output, draws the coverage over it.
pub(crate) fn measure(args: &Args) -> Result<Report> {
    let input = read_input(Some(args.file))?;
    let lang = language(&input, None);
    let name = args
        .file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let Some(spec) = Spec::read(&input.text, lang, &name) else {
        bail!(
            "{} is no state machine Omaspec reads, so nothing is covered",
            args.file.display()
        );
    };
    if args.traces.is_empty() {
        bail!("name the traces with --traces");
    }
    let texts: Vec<String> = args
        .traces
        .iter()
        .map(|path| fs::read_to_string(path).with_context(|| format!("reading {}", path.display())))
        .collect::<Result<_>>()?;
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let cov = coverage(&spec, &refs);
    let (covered, total) = cov.transitions_covered();
    let (covered_states, total_states) = cov.states_covered(&spec);
    let passed = args.min.is_none_or(|min| percent(covered, total) >= min);
    let image = match args.output {
        Some(output) => {
            draw(&input.text, lang, args.file, &spec, &cov, output)?;
            Some(output.display().to_string())
        }
        None => None,
    };
    Ok(Report {
        file: args.file.display().to_string(),
        traces: args
            .traces
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        objects: cov.objects.clone(),
        deviations: cov.deviations,
        covered_transitions: covered,
        total_transitions: total,
        covered_states,
        total_states,
        transitions: spec
            .machine
            .transitions
            .iter()
            .zip(&cov.transitions)
            .map(|(t, &hits)| TransitionReport {
                line: t.line + 1,
                from: state_name(&spec, t.from),
                to: state_name(&spec, t.to),
                trigger: trigger(t),
                hits,
            })
            .collect(),
        states: spec
            .machine
            .states
            .iter()
            .zip(&cov.states)
            .filter(|(s, _)| s.kind == Kind::State)
            .map(|(s, &hits)| StateReport {
                name: s.name.clone(),
                line: s.line + 1,
                hits,
            })
            .collect(),
        image,
        passed,
    })
}

/// The diagram with what was taken and what never was drawn over it,
/// written to `output` in the format its extension names.
fn draw(
    code: &str,
    lang: omaspec_core::Language,
    file: &Path,
    spec: &Spec,
    cov: &Coverage,
    output: &Path,
) -> Result<()> {
    let format = output
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(Format::from_extension)
        .filter(|format| matches!(format, Format::Svg | Format::Png | Format::Pdf))
        .with_context(|| format!("{} is no .svg, .png or .pdf", output.display()))?;
    let mut transitions = HashMap::new();
    for (t, &hits) in spec.machine.transitions.iter().zip(&cov.transitions) {
        let mark = if hits > 0 {
            Mark::Taken(hits)
        } else {
            Mark::Missed
        };
        // Several transitions written on one line share it: taken when
        // one of them is.
        transitions
            .entry(t.line)
            .and_modify(|had: &mut Mark| {
                if let (Mark::Taken(a), Mark::Taken(b)) = (*had, mark) {
                    *had = Mark::Taken(a + b);
                } else if matches!(mark, Mark::Taken(_)) {
                    *had = mark;
                }
            })
            .or_insert(mark);
    }
    let states = spec
        .machine
        .states
        .iter()
        .zip(&cov.states)
        .filter(|(s, _)| s.kind == Kind::State)
        .map(|(s, &hits)| {
            let mark = if hits > 0 {
                Mark::Taken(hits)
            } else {
                Mark::Missed
            };
            (s.name.clone(), mark)
        })
        .collect();
    let (covered, total) = cov.transitions_covered();
    let marks = Marks {
        transitions,
        states,
        ghosts: Vec::new(),
        caption: Some(format!(
            "{covered} of {total} transitions taken ({:.0} %)",
            percent(covered, total)
        )),
    };
    let place = omaspec_render::Place::of(file, None);
    let rendered = omaspec_render::render_marked(code, lang, place.as_ref(), &marks)
        .map_err(|err| anyhow::anyhow!("{}: {err}", file.display()))?;
    let bytes = export::export(&rendered.svg, format, &export::Options::default())?;
    fs::write(output, bytes).with_context(|| format!("writing {}", output.display()))?;
    Ok(())
}

fn percent(part: usize, whole: usize) -> f32 {
    if whole == 0 {
        100.0
    } else {
        part as f32 * 100.0 / whole as f32
    }
}

/// A state's name, `[*]` for a start or an end.
fn state_name(spec: &Spec, index: usize) -> String {
    match spec.machine.states.get(index) {
        Some(state) if !state.name.is_empty() => state.name.clone(),
        _ => "[*]".to_owned(),
    }
}

fn trigger(t: &Transition) -> String {
    let mut text = t.trigger.events.join(", ");
    if let Some(guard) = &t.trigger.guard {
        text = format!("{text} [{guard}]").trim().to_owned();
    }
    if let Some(action) = &t.trigger.action {
        text = format!("{text} / {action}").trim().to_owned();
    }
    text
}
