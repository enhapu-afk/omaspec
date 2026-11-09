//! `omaspec render`: diagrams to SVG, PNG, JPEG, PDF, Markdown or ZIP,
//! each beside its diagram unless `--output` says otherwise; a Markdown
//! document gives one image per diagram, and a folder its diagrams. With
//! `--check`, each SVG is held against the one beside the diagram instead
//! of written ([`hold`], [`crate::check`]). `omaspec check` holds them
//! the same way. As text (`.txt`), a state machine or a sequence diagram
//! is its behavior, one fact a line ([`as_text`]), for an agent or a
//! terminal that shows no pictures.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use omaspec_core::Language;
use omaspec_render::export::{self, Format};
use omaspec_render::{RenderError, Renderer};
use serde::Serialize;

use crate::commands::{failure, millis, render_config, stdout};
use crate::files::{copy_to_clipboard, read_inputs, write_atomic};

pub struct RenderArgs<'a> {
    pub files: &'a [PathBuf],
    pub output: Option<&'a Path>,
    pub format: Option<Format>,
    /// Text instead of a picture ([`as_text`]); so is a `.txt` output.
    pub text: bool,
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
    let txt = args
        .output
        .and_then(Path::extension)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"));
    if args.text || txt {
        anyhow::ensure!(
            !args.check,
            "--check holds each diagram's SVG; text is not drawn"
        );
        return render_text(args, &pieces, &empty);
    }
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

    if args.check {
        let held = hold(&pieces, empty, args.look, args.scale, args.source)?;
        return report_held(args.json, &held);
    }

    let mut renderer = Renderer::new(&render_config());
    let mut failed = false;
    let mut reports = Vec::new();
    // A map shows its project's diagrams in tiles (docs/adr/0018).
    let mut maps = crate::specs::Maps::default();
    let mut skins = HashMap::new();
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
        let exported = match export_piece(
            piece,
            &mut maps,
            &mut renderer,
            &mut skins,
            Draw {
                format,
                scale: args.scale,
                source: args.source,
                look: args.look,
            },
        )? {
            Ok(exported) => exported,
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
        let bytes = exported.bytes;
        let took = format!(
            "{}, {:.1} ms",
            exported.engine,
            exported.elapsed.as_secs_f64() * 1000.0
        );

        if args.copy {
            copy_to_clipboard(&bytes, format.mime())?;
            if args.json {
                reports.push(Report {
                    file: input.name(),
                    ok: true,
                    output: None,
                    copied: true,
                    engine: Some(exported.engine),
                    elapsed_ms: Some(millis(exported.elapsed)),
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
                        engine: Some(exported.engine),
                        elapsed_ms: Some(millis(exported.elapsed)),
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
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// The behavior of the state machine or sequence diagram `code`, one fact
/// a line, as `git diff` compares it after `omaspec git setup`; `Err` says
/// why a diagram has none.
pub(crate) fn as_text(code: &str, language: Language) -> Result<String, String> {
    if let Some(text) = omaspec_model::behavior(code, language) {
        return Ok(text);
    }
    let kind = omaspec_core::classify(code, language).kind;
    Err(match kind {
        omaspec_core::Kind::State | omaspec_core::Kind::Sequence => {
            "Omaspec does not fully read this diagram, so it cannot write it as text; `omaspec lint` says why".to_owned()
        }
        _ => "Omaspec writes state machines and sequence diagrams as text; `omaspec outline` gives the structure of any diagram".to_owned(),
    })
}

/// `render` as text: each diagram's [`as_text`] beside it (`door.txt`), to
/// `--output`, to standard output or to the clipboard.
fn render_text(
    args: &RenderArgs,
    pieces: &[crate::document::Piece],
    empty: &[&crate::files::Input],
) -> Result<ExitCode> {
    let mut failed = !empty.is_empty();
    let mut reports = Vec::new();
    let mut report =
        |file: String, output: Option<String>, copied: bool, message: Option<String>| {
            reports.push(Report {
                file,
                ok: message.is_none(),
                output,
                copied,
                engine: None,
                elapsed_ms: None,
                line: None,
                message,
                same: None,
            });
        };
    for input in empty {
        let message = "no PlantUML or Mermaid diagram in it";
        match args.json {
            true => report(input.name(), None, false, Some(message.to_owned())),
            false => eprintln!("{}: error: {message}", input.name()),
        }
    }
    for piece in pieces {
        let name = piece.input.name();
        let text = match as_text(&piece.code, piece.language) {
            Ok(text) => text,
            Err(message) => {
                failed = true;
                match args.json {
                    true => report(name, None, false, Some(message)),
                    false => eprintln!("{name}: error: {message}"),
                }
                continue;
            }
        };
        if args.copy {
            copy_to_clipboard(text.as_bytes(), "text/plain")?;
            match args.json {
                true => report(name, None, true, None),
                false => eprintln!("{name} copied as text"),
            }
            continue;
        }
        let dest = match (args.output, &piece.input.path) {
            (Some(out), _) if out.as_os_str() != "-" => Some(out.to_owned()),
            (Some(_), _) | (None, None) => None,
            (None, Some(_)) => piece.image_path("txt"),
        };
        match dest {
            Some(dest) => {
                anyhow::ensure!(
                    piece.input.path.as_deref() != Some(dest.as_path()),
                    "{} would overwrite the diagram itself",
                    dest.display()
                );
                write_atomic(&dest, text.as_bytes())?;
                match args.json {
                    true => report(name, Some(dest.display().to_string()), false, None),
                    false => eprintln!("{name} → {}", dest.display()),
                }
            }
            None => stdout().write_all(text.as_bytes())?,
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
    /// With `--check`: whether the SVG beside the diagram is the one drawn
    /// now.
    #[serde(skip_serializing_if = "Option::is_none")]
    same: Option<bool>,
}

fn skip_false(copied: &bool) -> bool {
    !*copied
}

/// One diagram drawn as `render` draws it, or why it was not.
struct Exported {
    bytes: Vec<u8>,
    engine: &'static str,
    elapsed: std::time::Duration,
}

/// How [`export_piece`] draws.
#[derive(Clone, Copy)]
struct Draw {
    format: Format,
    scale: f32,
    source: bool,
    look: Option<crate::look::Look>,
}

/// What [`hold`] says about one diagram: drawn and held against the SVG
/// beside it, or not drawn.
pub(crate) struct Held {
    pub file: String,
    pub ok: bool,
    pub output: Option<String>,
    pub engine: Option<&'static str>,
    pub elapsed_ms: Option<f64>,
    /// One-based, when the error names a line.
    pub line: Option<usize>,
    pub message: Option<String>,
    /// Whether the SVG beside the diagram is the one drawn now. `None`
    /// when nothing was held against.
    pub same: Option<bool>,
}

/// Draws the diagrams of `files` as `render --check` does and holds each
/// SVG against the one beside its diagram. Writes nothing.
pub(crate) fn hold_files(files: &[PathBuf]) -> Result<Vec<Held>> {
    let inputs = read_inputs(&with_folders(files)?)?;
    let read: Vec<(&crate::files::Input, Vec<crate::document::Piece>)> = inputs
        .iter()
        .map(|input| (input, crate::document::pieces(input, None)))
        .collect();
    let empty: Vec<&crate::files::Input> = read
        .iter()
        .filter(|(_, pieces)| pieces.is_empty())
        .map(|(input, _)| *input)
        .collect();
    let pieces: Vec<crate::document::Piece> =
        read.into_iter().flat_map(|(_, pieces)| pieces).collect();
    hold(&pieces, empty, None, 2.0, true)
}

/// Each diagram of `pieces` drawn as `render --check` draws it, at
/// `scale` and with the source stored when `source`, held against the SVG
/// beside it. `empty` are documents with no diagram.
fn hold(
    pieces: &[crate::document::Piece],
    empty: Vec<&crate::files::Input>,
    look: Option<crate::look::Look>,
    scale: f32,
    source: bool,
) -> Result<Vec<Held>> {
    let mut renderer = Renderer::new(&render_config());
    let mut maps = crate::specs::Maps::default();
    let mut skins = HashMap::new();
    let draw = Draw {
        format: Format::Svg,
        scale,
        source,
        look,
    };
    let mut held = Vec::new();
    for input in empty {
        held.push(Held {
            file: input.name(),
            ok: false,
            output: None,
            engine: None,
            elapsed_ms: None,
            line: None,
            message: Some("no PlantUML or Mermaid diagram in it".to_owned()),
            same: None,
        });
    }
    for piece in pieces {
        match export_piece(piece, &mut maps, &mut renderer, &mut skins, draw)? {
            Err(err) => {
                let (line, engine, message) = failure(&err);
                held.push(Held {
                    file: piece.input.name(),
                    ok: false,
                    output: None,
                    engine,
                    elapsed_ms: None,
                    line: line.map(|l| piece.line(l - 1) + 1),
                    message: Some(message),
                    same: None,
                });
            }
            Ok(exported) => {
                let dest = piece.image_path("svg").expect("a file's diagram");
                let verdict = match std::fs::read_to_string(&dest) {
                    Ok(old) => crate::check::compare(
                        &old,
                        &String::from_utf8_lossy(&exported.bytes),
                        &piece.code,
                    ),
                    Err(_) => Some("not there; `omaspec render` draws it".to_owned()),
                };
                held.push(Held {
                    file: piece.input.name(),
                    ok: true,
                    output: Some(dest.display().to_string()),
                    engine: Some(exported.engine),
                    elapsed_ms: Some(millis(exported.elapsed)),
                    line: None,
                    same: Some(verdict.is_none()),
                    message: verdict,
                });
            }
        }
    }
    Ok(held)
}

/// Prints what [`hold`] found, as `render --check` does.
fn report_held(json: bool, held: &[Held]) -> Result<ExitCode> {
    let mut checked = 0;
    let mut differ = 0;
    let mut failed = false;
    let mut reports = Vec::new();
    for item in held {
        if item.same.is_none() {
            failed |= item.message.is_some();
            if json {
                reports.push(report_of(item));
            } else if let Some(message) = &item.message {
                let at = item.line.map(|l| format!(":{l}")).unwrap_or_default();
                eprintln!("{}{at}: error: {message}", item.file);
            }
            continue;
        }
        checked += 1;
        if item.same == Some(false) {
            failed = true;
            differ += 1;
        }
        if json {
            reports.push(report_of(item));
        } else if let Some(why) = &item.message {
            writeln!(
                stdout(),
                "{}: {why}",
                item.output.as_deref().unwrap_or(&item.file)
            )?;
        }
    }
    if json {
        let mut out = stdout();
        serde_json::to_writer_pretty(&mut out, &reports)?;
        writeln!(out)?;
    } else if checked > 0 {
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

fn report_of(item: &Held) -> Report {
    Report {
        file: item.file.clone(),
        ok: item.ok,
        output: item.output.clone(),
        copied: false,
        engine: item.engine,
        elapsed_ms: item.elapsed_ms,
        line: item.line,
        message: item.message.clone(),
        same: item.same,
    }
}

/// `piece` drawn and exported as `draw` says. The outer error stops the
/// command; the inner one is this diagram.
fn export_piece(
    piece: &crate::document::Piece,
    maps: &mut crate::specs::Maps,
    renderer: &mut Renderer,
    skins: &mut HashMap<Option<PathBuf>, Option<crate::look::Skin>>,
    draw: Draw,
) -> Result<Result<Exported, RenderError>> {
    let input = piece.input;
    let place = input
        .path
        .as_deref()
        .and_then(|path| omaspec_render::Place::of(path, None));
    // `--look`, else the look its project pins; a picture without color
    // schemes takes auto's light one.
    let root = place.as_ref().map(|p| p.root.clone());
    let skin = match skins.get(&root) {
        Some(skin) => skin.clone(),
        None => {
            let skin = crate::look::skin_for(draw.look, root.as_deref())?.map(|skin| {
                if draw.format == Format::Svg {
                    skin
                } else {
                    skin.without_schemes()
                }
            });
            skins.insert(root, skin.clone());
            skin
        }
    };
    // A project's file is drawn as the project's map, which is made from
    // its diagrams (docs/adr/0026).
    let project = input
        .path
        .as_deref()
        .filter(|path| {
            path.file_name()
                .is_some_and(|n| n == omaspec_core::project::FILE)
        })
        .and_then(|path| std::path::absolute(path).ok())
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let drawn = match &project {
        Some(root) => crate::look::render_map(maps, renderer, root, "", skin.as_ref()),
        None => crate::look::render(
            maps,
            renderer,
            &piece.code,
            piece.language,
            input.path.as_deref(),
            place.as_ref(),
            skin.as_ref(),
        ),
    };
    let rendered = match drawn {
        Ok(rendered) => rendered,
        Err(err) => return Ok(Err(err)),
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
    let bytes = export::export(
        &rendered.svg,
        draw.format,
        &export::Options {
            scale: draw.scale,
            // A map has no source of its own: it is made from the project.
            source: (draw.source && project.is_none()).then(|| export::Source {
                code: &piece.code,
                language: piece.language,
                file: source_name.as_deref(),
            }),
            links: &links,
            up,
        },
    )?;
    Ok(Ok(Exported {
        bytes,
        engine: rendered.engine.name(),
        elapsed: rendered.elapsed,
    }))
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
