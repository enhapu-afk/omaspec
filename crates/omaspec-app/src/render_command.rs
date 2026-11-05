//! `omaspec render`: diagrams to SVG, PNG, JPEG, PDF, Markdown or ZIP,
//! each beside its diagram unless `--output` says otherwise; a Markdown
//! document gives one image per diagram, and a folder its diagrams. With
//! `--check`, each SVG is held against the one beside the diagram instead
//! of written ([`crate::check`]).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use omaspec_core::Language;
use omaspec_render::Renderer;
use omaspec_render::export::{self, Format};
use serde::Serialize;

use crate::commands::{failure, millis, render_config, stdout};
use crate::files::{copy_to_clipboard, read_inputs, write_atomic};

pub struct RenderArgs<'a> {
    pub files: &'a [PathBuf],
    pub output: Option<&'a Path>,
    pub format: Option<Format>,
    pub scale: f32,
    /// Store the diagram source in SVG and PNG images.
    pub source: bool,
    pub copy: bool,
    /// A look laid over the file's colors.
    pub look: Option<crate::look::Look>,
    pub lang: Option<Language>,
    /// A JSON report on stdout instead of status lines. The images are
    /// still written; stdout is not an image in this mode.
    pub json: bool,
    /// Hold each SVG against the one beside its diagram instead of
    /// writing it, and say what changed.
    pub check: bool,
}

pub fn render(args: &RenderArgs) -> Result<ExitCode> {
    let inputs = read_inputs(&with_folders(args.files)?)?;
    // A Markdown document gives one image per diagram; one without any
    // is said so.
    let read: Vec<(&crate::files::Input, Vec<crate::document::Piece>)> = inputs
        .iter()
        .map(|input| (input, crate::document::pieces(input, args.lang)))
        .collect();
    let empty: Vec<&crate::files::Input> = read
        .iter()
        .filter(|(_, pieces)| pieces.is_empty())
        .map(|(input, _)| *input)
        .collect();
    let pieces: Vec<crate::document::Piece> =
        read.into_iter().flat_map(|(_, pieces)| pieces).collect();
    anyhow::ensure!(
        args.output.is_none() || pieces.len() == 1,
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
    if args.check {
        anyhow::ensure!(
            format == Format::Svg && args.output.is_none() && !args.copy,
            "--check holds each diagram's SVG against the one beside it; it takes no --output, --copy or other format"
        );
        anyhow::ensure!(
            inputs.iter().all(|input| input.path.is_some()),
            "--check holds a file's SVG against the one beside it; standard input has none"
        );
    }
    anyhow::ensure!(
        args.look != Some(crate::look::Look::Auto) || format == Format::Svg,
        "--look auto makes an SVG, which a viewer turns dark with its own scheme; for {} take light or dark",
        format.extension().to_uppercase()
    );

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
    // With `--check`: how many SVGs were held against, how many differ.
    let (mut checked, mut differ) = (0, 0);
    let mut failed = false;
    let mut reports = Vec::new();
    // A map shows its project's diagrams in tiles (docs/adr/0018).
    let mut maps = crate::specs::Maps::default();
    for input in empty {
        failed = true;
        let message = "no PlantUML or Mermaid diagram in it".to_owned();
        if args.json {
            reports.push(Report {
                file: input.name(),
                ok: false,
                output: None,
                copied: false,
                engine: None,
                elapsed_ms: None,
                line: None,
                message: Some(message),
                same: None,
            });
        } else {
            eprintln!("{}: error: {message}", input.name());
        }
    }
    for piece in &pieces {
        let input = piece.input;
        let lang = piece.language;
        let place = input
            .path
            .as_deref()
            .and_then(|path| omaspec_render::Place::of(path, None));
        // `--look`, else the look its project pins; a picture without
        // color schemes takes auto's light one.
        let skin = crate::look::skin_for(args.look, place.as_ref().map(|p| p.root.as_path()))?.map(
            |skin| {
                if format == Format::Svg {
                    skin
                } else {
                    skin.without_schemes()
                }
            },
        );
        let drawn = crate::look::render(
            &mut maps,
            &mut renderer,
            &piece.code,
            lang,
            input.path.as_deref(),
            place.as_ref(),
            skin.as_ref(),
        );
        let rendered = match drawn {
            Ok(r) => r,
            Err(err) => {
                failed = true;
                let (line, engine, message) = failure(&err);
                let line = line.map(|l| piece.line(l - 1) + 1);
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
                        same: None,
                    });
                } else {
                    let at = line.map(|l| format!(":{l}")).unwrap_or_default();
                    eprintln!("{}{at}: error: {message}", input.name());
                }
                continue;
            }
        };
        // Each tile or state that opens a diagram links to its export.
        let own = place
            .as_ref()
            .zip(input.path.as_deref())
            .and_then(|(place, path)| crate::specs::path_in(&place.root, path));
        let up = own.as_deref().map_or(0, crate::specs::depth);
        let links = own.map_or_else(Vec::new, |own| {
            crate::specs::export_links(&rendered.links, &own)
        });
        let source_name = piece.source_name();
        let options = export::Options {
            scale: args.scale,
            source: args.source.then(|| export::Source {
                code: &piece.code,
                language: lang,
                file: source_name.as_deref(),
            }),
            links: &links,
            up,
        };
        let bytes = export::export(&rendered.svg, format, &options)?;
        let took = format!(
            "{}, {:.1} ms",
            rendered.engine.name(),
            rendered.elapsed.as_secs_f64() * 1000.0
        );
        if args.check {
            let dest = piece.image_path("svg").expect("a file's diagram");
            let verdict = match std::fs::read_to_string(&dest) {
                Ok(old) => {
                    crate::check::compare(&old, &String::from_utf8_lossy(&bytes), &piece.code)
                }
                Err(_) => Some("not there; `omaspec render` draws it".to_owned()),
            };
            checked += 1;
            if verdict.is_some() {
                failed = true;
                differ += 1;
            }
            if args.json {
                reports.push(Report {
                    file: input.name(),
                    ok: true,
                    output: Some(dest.display().to_string()),
                    copied: false,
                    engine: Some(rendered.engine.name()),
                    elapsed_ms: Some(millis(rendered.elapsed)),
                    line: None,
                    same: Some(verdict.is_none()),
                    message: verdict,
                });
            } else if let Some(why) = verdict {
                writeln!(stdout(), "{}: {why}", dest.display())?;
            }
            continue;
        }

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
                    same: None,
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
            (None, Some(_)) => piece.image_path(format.extension()),
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
                        same: None,
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
    } else if args.check && checked > 0 {
        eprintln!(
            "{}",
            match differ {
                0 => format!("{checked} SVGs drawn as they are"),
                n => format!("{n} of {checked} SVGs differ; `omaspec render` draws them again"),
            }
        );
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
    /// With `--check`: whether the SVG beside the diagram is the one drawn
    /// now.
    #[serde(skip_serializing_if = "Option::is_none")]
    same: Option<bool>,
}

fn skip_false(copied: &bool) -> bool {
    !*copied
}

/// `files`, each folder among them as the diagrams and Markdown documents
/// in it.
fn with_folders(files: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::with_capacity(files.len());
    for file in files {
        if !file.is_dir() {
            out.push(file.clone());
            continue;
        }
        let found = crate::editor::folder::scan(file);
        anyhow::ensure!(!found.is_empty(), "{}: no diagrams in it", file.display());
        out.extend(found.into_iter().map(|relative| file.join(relative)));
    }
    Ok(out)
}
