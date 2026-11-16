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
    /// A picture too large at `scale` is drawn at the largest that fits.
    pub fit: bool,
    /// The least size of a PNG's or JPEG's words, in pixels: shrunk to fit
    /// no further, and cut into tiles where it then does not fit (REQ-070).
    pub min_font: Option<f32>,
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
    let txt = args
        .output
        .and_then(Path::extension)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"));
    // A map is a picture: as text, a project is its diagrams.
    let inputs = read_inputs(&with_folders(args.files, !(args.text || txt))?)?;
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
    if args.text || txt {
        anyhow::ensure!(
            args.min_font.is_none(),
            "--min-font sizes the words of a PNG or JPEG; text has no pixels"
        );
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
    if let Some(least) = args.min_font {
        anyhow::ensure!(
            matches!(format, Format::Png | Format::Jpeg),
            "--min-font sizes the words of a PNG or JPEG; an {} keeps them as they are, at any zoom",
            format.extension().to_uppercase()
        );
        anyhow::ensure!(
            least > 0.0 && least <= 200.0,
            "--min-font must be between 0 and 200 pixels"
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
                message: Some(message),
                ..Report::default()
            });
        } else {
            eprintln!("{}: error: {message}", input.name());
        }
    }
    for piece in &pieces {
        let input = piece.input;
        // Where the picture goes: a file, else the clipboard or standard
        // output.
        let dest = match (args.output, &input.path) {
            _ if args.copy => None,
            (Some(out), _) if out.as_os_str() != "-" => Some(out.to_owned()),
            (Some(_), _) | (None, None) => None,
            (None, Some(_)) => piece.image_path(format.extension()),
        };
        let exported = match export_piece(
            piece,
            &mut maps,
            &mut renderer,
            &mut skins,
            Draw {
                format,
                scale: args.scale,
                fit: args.fit,
                min_font: args.min_font,
                // Tiles are files: not on the clipboard or standard output.
                tiles: dest.is_some(),
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
                        engine,
                        line,
                        message: Some(message),
                        ..Report::default()
                    });
                } else {
                    let at = line.map(|l| format!(":{l}")).unwrap_or_default();
                    eprintln!("{}{at}: error: {message}", input.name());
                }
                continue;
            }
        };
        let took = format!(
            "{}, {:.1} ms",
            exported.engine,
            exported.elapsed.as_secs_f64() * 1000.0
        );
        let done = Report {
            file: input.name(),
            ok: true,
            engine: Some(exported.engine),
            elapsed_ms: Some(millis(exported.elapsed)),
            undrawable: exported.undrawable.iter().map(char::to_string).collect(),
            fitted_scale: exported.fitted,
            sized: exported.sized,
            ..Report::default()
        };
        // Where it went, and what it says besides, on standard error.
        let said = |to: String| {
            eprintln!("{} {to} ({took})", input.name());
            warn_undrawable(&input.name(), &exported.undrawable);
            say_fitted(&input.name(), exported.fitted);
            say_sized(&input.name(), exported.sized);
        };
        match (exported.picture, dest) {
            (Picture::Tiles(tiles), Some(dest)) => {
                let paths = write_tiles(&dest, &tiles, input.path.as_deref())?;
                match args.json {
                    true => reports.push(Report {
                        tiles: paths.iter().map(|p| p.display().to_string()).collect(),
                        ..done
                    }),
                    false => said(format!(
                        "→ {} … {}, {} tiles",
                        paths[0].display(),
                        paths[paths.len() - 1].display(),
                        paths.len()
                    )),
                }
            }
            (Picture::Tiles(_), None) => anyhow::bail!("tiles are files"),
            (Picture::One(bytes), _) if args.copy => {
                copy_to_clipboard(&bytes, format.mime())?;
                match args.json {
                    true => reports.push(Report {
                        copied: true,
                        ..done
                    }),
                    false => said(format!("copied as {}", format.extension().to_uppercase())),
                }
            }
            (Picture::One(bytes), Some(dest)) => {
                anyhow::ensure!(
                    input.path.as_deref() != Some(dest.as_path()),
                    "{} would overwrite the diagram itself",
                    dest.display()
                );
                write_atomic(&dest, &bytes)?;
                // Tiles an earlier drawing cut it into are gone now.
                if args.min_font.is_some() {
                    clear_tiles(&tiles_folder(&dest), &[])?;
                }
                match args.json {
                    true => reports.push(Report {
                        output: Some(dest.display().to_string()),
                        ..done
                    }),
                    false => said(format!("→ {}", dest.display())),
                }
            }
            (Picture::One(bytes), None) => stdout().write_all(&bytes)?,
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
                message,
                ..Report::default()
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

#[derive(Serialize, Default)]
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
    /// The characters no installed font draws: the picture shows an empty
    /// box for each.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    undrawable: Vec<String>,
    /// With `--fit`: the scale the picture was drawn at to fit.
    #[serde(skip_serializing_if = "Option::is_none")]
    fitted_scale: Option<f32>,
    /// With `--min-font`: the scale, the size of the smallest words and
    /// the grid of tiles.
    #[serde(skip_serializing_if = "Option::is_none", flatten)]
    sized: Option<Sizing>,
    /// With `--min-font`, where the picture is cut: the tiles, row by row.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tiles: Vec<String>,
}

/// How `--min-font` drew a picture.
#[derive(Debug, Clone, Copy, Default, Serialize)]
struct Sizing {
    scale: f32,
    /// The font size of the smallest words, in pixels; none without words.
    #[serde(skip_serializing_if = "Option::is_none")]
    smallest_text_px: Option<f32>,
    rows: usize,
    columns: usize,
}

/// The folder the tiles of a picture meant for `dest` go into, beside it:
/// `odyssey.png`'s are `odyssey-tiles/1.png` and on. A folder of their own
/// cannot take the name of another diagram's picture, and holds what an
/// earlier drawing left, to take away.
fn tiles_folder(dest: &Path) -> PathBuf {
    let stem = dest.file_stem().unwrap_or_default().to_string_lossy();
    dest.with_file_name(format!("{stem}-tiles"))
}

/// `tiles` written into the folder for `dest` ([`tiles_folder`]), each
/// named by its number, with as many digits as the last has, so they sort;
/// the tiles an earlier drawing left there past these taken away. Their
/// paths. Not over the `diagram` itself.
fn write_tiles(dest: &Path, tiles: &[Vec<u8>], diagram: Option<&Path>) -> Result<Vec<PathBuf>> {
    let folder = tiles_folder(dest);
    std::fs::create_dir_all(&folder)?;
    let ext = dest
        .extension()
        .map_or_else(|| "png".into(), |e| e.to_string_lossy());
    let digits = tiles.len().to_string().len();
    let paths: Vec<PathBuf> = (1..=tiles.len())
        .map(|n| folder.join(format!("{n:0digits$}.{ext}")))
        .collect();
    for (path, bytes) in paths.iter().zip(tiles) {
        anyhow::ensure!(
            diagram != Some(path.as_path()),
            "{} would overwrite the diagram itself",
            path.display()
        );
        write_atomic(path, bytes)?;
    }
    clear_tiles(&folder, &paths)?;
    Ok(paths)
}

/// Takes the tiles in `folder` that are not among `keep` away: files named
/// by a number alone, PNG or JPEG, as only a drawing of tiles names them
/// there; the folder too, once nothing else is in it.
fn clear_tiles(folder: &Path, keep: &[PathBuf]) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let tile =
            path.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|stem| !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit()))
                && path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg")
                });
        if tile && !keep.contains(&path) {
            std::fs::remove_file(&path)?;
        }
    }
    if keep.is_empty() {
        // Left where something else is in it.
        let _ = std::fs::remove_dir(folder);
    }
    Ok(())
}

/// Says the scale `--min-font` drew a picture at, and how large its
/// smallest words are.
fn say_sized(name: &str, sized: Option<Sizing>) {
    let Some(sized) = sized else { return };
    let scale = (sized.scale * 100.0).round() / 100.0;
    let words = match sized.smallest_text_px {
        Some(px) => format!(", its smallest words {:.1} px high", px),
        None => ", without words".to_owned(),
    };
    let plural = |n: usize, one: &str| format!("{n} {}", crate::editor::plural(n, one));
    let grid = if sized.rows * sized.columns > 1 {
        format!(
            ", cut into {} × {}",
            plural(sized.rows, "row"),
            plural(sized.columns, "column")
        )
    } else {
        String::new()
    };
    eprintln!("{name}: drawn at scale {scale}{words}{grid} (--min-font)");
}

/// Says the scale a picture was drawn at to fit, when `--fit` lowered it.
fn say_fitted(name: &str, fitted: Option<f32>) {
    if let Some(scale) = fitted {
        eprintln!("{name}: drawn at scale {scale} to fit in 100 million pixels (--fit)");
    }
}

/// Names the characters no installed font draws, and the font to install
/// for emoji, the usual ones.
fn warn_undrawable(name: &str, chars: &[char]) {
    if chars.is_empty() {
        return;
    }
    let listed: Vec<String> = chars.iter().map(char::to_string).collect();
    eprintln!(
        "{name}: warning: no installed font draws {}; they show as empty boxes. Install a font that has them (emoji: noto-fonts-emoji)",
        listed.join(" ")
    );
}

fn skip_false(copied: &bool) -> bool {
    !*copied
}

/// One diagram drawn as `render` draws it, or why it was not.
struct Exported {
    picture: Picture,
    engine: &'static str,
    elapsed: std::time::Duration,
    /// The characters no installed font draws, in a PNG, JPEG or PDF.
    undrawable: Vec<char>,
    /// The scale a picture was drawn at to fit, when `--fit` lowered it.
    fitted: Option<f32>,
    /// How `--min-font` drew it.
    sized: Option<Sizing>,
}

/// What a diagram was drawn as: one file, or the tiles `--min-font` cut
/// it into, in their order.
enum Picture {
    One(Vec<u8>),
    Tiles(Vec<Vec<u8>>),
}

/// How [`export_piece`] draws.
#[derive(Clone, Copy)]
struct Draw {
    format: Format,
    scale: f32,
    /// A picture too large at `scale` is drawn at the largest that fits.
    fit: bool,
    /// The least size of a picture's words, in pixels.
    min_font: Option<f32>,
    /// A picture may be cut into tiles: they go to files.
    tiles: bool,
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
    let inputs = read_inputs(&with_folders(files, true)?)?;
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
        fit: false,
        min_font: None,
        tiles: false,
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
                // An SVG is one file: no --min-font here.
                let Picture::One(drawn) = &exported.picture else {
                    anyhow::bail!("an SVG is one picture");
                };
                let verdict = match std::fs::read_to_string(&dest) {
                    Ok(old) => {
                        crate::check::compare(&old, &String::from_utf8_lossy(drawn), &piece.code)
                    }
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
        engine: item.engine,
        elapsed_ms: item.elapsed_ms,
        line: item.line,
        message: item.message.clone(),
        same: item.same,
        ..Report::default()
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
    let project = input.path.as_deref().and_then(crate::look::map_root);
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
        crate::specs::export_links(&rendered.links, &own, false)
    });
    let source_name = piece.source_name();
    // The text as drawn, whose lines the map's are: a presentation's play
    // is read from it.
    let drawn = (draw.format == Format::Html && project.is_none())
        .then(|| crate::look::drawn_text(&piece.code, piece.language, skin.as_ref()));
    let options_at = |scale: f32| export::Options {
        scale,
        // A map has no source of its own: it is made from the project.
        source: (draw.source && project.is_none()).then(|| export::Source {
            code: &piece.code,
            language: piece.language,
            file: source_name.as_deref(),
        }),
        links: &links,
        up,
        play: (draw.format == Format::Html).then(|| {
            let (paper, accent) = crate::look::stage(skin.as_ref());
            export::Play {
                map: &rendered.map,
                code: drawn.as_deref().map(|drawn| (drawn, piece.language)),
                paper,
                accent,
            }
        }),
    };
    let export_at = |scale: f32| export::export(&rendered.svg, draw.format, &options_at(scale));
    if let Some(least) = draw.min_font {
        // Parsed once: the words measured, the tiles cut and drawn from it.
        let tree = export::parse(&rendered.svg)?;
        let plan = export::plan(&tree, draw.format, draw.scale, least);
        let units = plan.smallest.map(|px| px / plan.scale);
        anyhow::ensure!(
            plan.parts.len() <= export::MOST_TILES,
            "{}: words {least} px high would take {} tiles of up to 100 million pixels, and at most {} are written{}; take a smaller --min-font, or an SVG or a PDF, which keep the words at any size",
            input.name(),
            plan.parts.len(),
            export::MOST_TILES,
            units.map_or_else(String::new, |units| format!(
                " (its smallest words are {units:.1} units high)"
            ))
        );
        anyhow::ensure!(
            draw.tiles || !plan.tiled(),
            "{}: --min-font cuts it into {} tiles, which are files; leave out --copy, or name one with --output",
            input.name(),
            plan.parts.len()
        );
        let mut files =
            export::export_planned(&tree, draw.format, options_at(plan.scale).source, &plan)?;
        let picture = match plan.tiled() {
            true => Picture::Tiles(files),
            false => Picture::One(files.pop().expect("a plan of one part")),
        };
        return Ok(Ok(Exported {
            picture,
            engine: rendered.engine.name(),
            elapsed: rendered.elapsed,
            undrawable: export::undrawable_in(&tree),
            fitted: None,
            sized: Some(Sizing {
                scale: plan.scale,
                smallest_text_px: plan.smallest,
                rows: plan.rows,
                columns: plan.columns,
            }),
        }));
    }
    let (bytes, fitted) = match export_at(draw.scale) {
        Err(export::ExportError::TooLarge {
            fits: Some(fits), ..
        }) if draw.fit && fits > 0.0 => (export_at(fits)?, Some(fits)),
        Err(too_large @ export::ExportError::TooLarge { fits: Some(_), .. }) => {
            anyhow::bail!("{too_large}; `--fit` draws it at that scale")
        }
        drawn => (drawn?, None),
    };
    // A picture's words are set here, in the faces installed; an SVG's
    // are set where it is shown.
    let undrawable = match draw.format {
        Format::Png | Format::Jpeg | Format::Pdf => {
            export::undrawable(&rendered.svg).unwrap_or_default()
        }
        _ => Vec::new(),
    };
    Ok(Ok(Exported {
        picture: Picture::One(bytes),
        engine: rendered.engine.name(),
        elapsed: rendered.elapsed,
        undrawable,
        fitted,
        sized: None,
    }))
}

/// `files`, each folder among them as the diagrams and Markdown documents
/// in it and, with `maps`, the file of every project in it, which is
/// drawn as that project's map.
fn with_folders(files: &[PathBuf], maps: bool) -> Result<Vec<PathBuf>> {
    let mut out = Vec::with_capacity(files.len());
    for file in files {
        if !file.is_dir() {
            out.push(file.clone());
            continue;
        }
        let found = crate::editor::folder::scan(file);
        anyhow::ensure!(!found.is_empty(), "{}: no diagrams in it", file.display());
        out.extend(found.into_iter().map(|relative| file.join(relative)));
        if maps {
            let projects =
                crate::editor::folder::scan_for(file, |name| name == omaspec_core::project::FILE);
            out.extend(projects.into_iter().map(|relative| file.join(relative)));
        }
    }
    Ok(out)
}
