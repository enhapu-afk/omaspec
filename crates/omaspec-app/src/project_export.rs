//! A project as files anyone can open (`omaspec export`, and the
//! palette's *Export the project*): every diagram as an SVG with its source,
//! beside one another as the diagrams are, the main diagram also as
//! `index.svg`, each tile and linked state a link to the SVG of the diagram
//! it opens, and all of it once more as one ZIP. A browser is enough to
//! follow the links; dropping any of the pictures on Omaspec opens its
//! source.
//!
//! Nothing is written outside the export's folder (`export` in the project
//! unless another is named), and an SVG left there from an earlier export
//! stays; the ZIP holds this export only.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use omaspec_render::Renderer;
use omaspec_render::export::{self, Format, Options, Source};
use serde::Serialize;

use crate::files::write_atomic;
use crate::look::Look;
use crate::specs::{Maps, export_links, path_in, read_project, svg_of};

/// The export's folder in the project unless another is named.
pub const FOLDER: &str = "export";

/// The main diagram once more, at the top of the folder, for a browser to
/// start at.
pub const INDEX: &str = "index.svg";

/// What an export wrote.
#[derive(Debug, Serialize)]
pub struct Exported {
    pub folder: PathBuf,
    pub zip: PathBuf,
    /// Each diagram written: its path in the project, and its SVG's in the
    /// folder.
    #[serde(serialize_with = "pairs")]
    pub written: Vec<(String, String)>,
    /// Whether the main diagram is `index.svg` too.
    pub index: bool,
    /// The diagrams that did not draw; a tile or a state that opens one
    /// links to nothing.
    pub failed: Vec<Failed>,
}

#[derive(Debug, Serialize)]
pub struct Failed {
    /// Its path in the project.
    pub file: String,
    /// One-based.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engine: Option<&'static str>,
    pub message: String,
}

impl Exported {
    /// One line for the editor's status bar.
    pub fn summary(&self) -> String {
        let count = self.written.len();
        let mut line = format!(
            "{count} {} in {}",
            if count == 1 { "diagram" } else { "diagrams" },
            self.folder.display()
        );
        if self.index {
            line.push_str(&format!(", start at {INDEX}"));
        }
        let zip = self.zip.file_name().unwrap_or_default().to_string_lossy();
        line.push_str(&format!(", all of it in {zip}"));
        if let Some(first) = self.failed.first() {
            let at = first.line.map(|l| format!(":{l}")).unwrap_or_default();
            line.push_str(&format!(
                "; {} did not draw, first {}{at}",
                self.failed.len(),
                first.file
            ));
        }
        line
    }
}

/// `omaspec export`: status lines on standard error, or the JSON report on
/// standard output. Fails when a diagram did not draw.
pub fn command(dir: &Path, out: Option<&Path>, look: Option<Look>, json: bool) -> Result<ExitCode> {
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let exported = export(dir, out, look, &mut renderer)?;
    if json {
        let mut stdout = std::io::stdout();
        serde_json::to_writer_pretty(&mut stdout, &exported)?;
        writeln!(stdout)?;
    } else {
        for failed in &exported.failed {
            let at = failed.line.map(|l| format!(":{l}")).unwrap_or_default();
            eprintln!(
                "{}{at}: error: {}",
                dir.join(&failed.file).display(),
                failed.message
            );
        }
        let count = exported.written.len();
        eprintln!(
            "{count} {} → {}{}",
            if count == 1 { "diagram" } else { "diagrams" },
            exported.folder.display(),
            if exported.index {
                format!(" (start at {INDEX})")
            } else {
                String::new()
            }
        );
        eprintln!("all of it in {}", exported.zip.display());
    }
    Ok(if exported.failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

/// The diagrams written, as objects in the JSON report.
fn pairs<S: serde::Serializer>(pairs: &[(String, String)], s: S) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Written<'a> {
        file: &'a str,
        output: &'a str,
    }
    s.collect_seq(pairs.iter().map(|(file, output)| Written { file, output }))
}

/// Exports the project at `root` into `out`, or into its `export` folder,
/// drawn in `look`, else in the look the project pins, else in the files'
/// own colors. A diagram that does
/// not draw is reported and left out; two diagrams that would be one SVG
/// (`a.puml` and `a.mmd`), or one that would be the main diagram's
/// `index.svg`, stop the export before anything is written.
pub fn export(
    root: &Path,
    out: Option<&Path>,
    look: Option<Look>,
    renderer: &mut Renderer,
) -> Result<Exported> {
    let root =
        std::path::absolute(root).with_context(|| format!("no folder `{}`", root.display()))?;
    anyhow::ensure!(root.is_dir(), "{} is not a folder", root.display());
    let folder = match out {
        Some(out) => {
            std::path::absolute(out).with_context(|| format!("no folder `{}`", out.display()))?
        }
        None => root.join(FOLDER),
    };
    anyhow::ensure!(!folder.is_file(), "{} is a file", folder.display());
    let (_, atlas) = read_project(&root);
    anyhow::ensure!(
        !atlas.diagrams.is_empty(),
        "{} holds no diagrams",
        root.display()
    );
    let main = atlas
        .main
        .clone()
        .filter(|main| atlas.diagrams.iter().any(|entry| entry.file == *main));
    let files: Vec<&str> = atlas.diagrams.iter().map(|e| e.file.as_str()).collect();
    let targets = targets(&files, main.as_deref())?;
    let name = zip_name(atlas.name.as_deref(), &root);
    // `look`, else the one the project pins.
    let skin = crate::look::skin_for(look, Some(&root))?;
    let atlas = Arc::new(atlas);
    let mut maps = Maps::with(root.clone(), Arc::clone(&atlas));

    let mut pictures: Vec<(String, Vec<u8>)> = Vec::new();
    let mut exported = Exported {
        zip: folder.join(format!("{name}.zip")),
        folder,
        written: Vec::new(),
        index: false,
        failed: Vec::new(),
    };
    for (entry, target) in atlas.diagrams.iter().zip(targets) {
        let path = root.join(&entry.file);
        let drawn = crate::look::render(
            &mut maps,
            renderer,
            &entry.source,
            entry.language,
            Some(&path),
            entry.place.as_ref(),
            skin.as_ref(),
        );
        let rendered = match drawn {
            Ok(rendered) => rendered,
            Err(err) => {
                let (line, engine, message) = crate::commands::failure(&err);
                exported.failed.push(Failed {
                    file: entry.file.clone(),
                    line,
                    engine,
                    message,
                });
                continue;
            }
        };
        // Its path in the project it links into: a project inside this
        // one links among its own diagrams, which lie below it here too.
        let own = entry
            .place
            .as_ref()
            .and_then(|place| path_in(&place.root, &path))
            .unwrap_or_else(|| entry.file.clone());
        let file = entry.file.rsplit('/').next().unwrap_or(&entry.file);
        let svg = |own: &str| -> Result<Vec<u8>> {
            let links = export_links(&rendered.links, own);
            let options = Options {
                scale: 1.0,
                source: Some(Source {
                    code: &entry.source,
                    language: entry.language,
                    file: Some(file),
                }),
                links: &links,
                up: crate::specs::depth(own),
            };
            Ok(export::export(&rendered.svg, Format::Svg, &options)?)
        };
        let is_main = main.as_deref() == Some(entry.file.as_str());
        if is_main && target != INDEX {
            pictures.push((INDEX.to_owned(), svg(INDEX)?));
        }
        exported.index |= is_main;
        pictures.push((target.clone(), svg(&own)?));
        exported.written.push((entry.file.clone(), target));
    }
    anyhow::ensure!(
        !exported.written.is_empty(),
        "no diagram of {} drew",
        root.display()
    );

    for (target, bytes) in &pictures {
        let dest = exported.folder.join(target);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot make {}", parent.display()))?;
        }
        write_atomic(&dest, bytes)?;
    }
    // Unpacked, the ZIP is a folder named after the project.
    let named: Vec<(String, &[u8])> = pictures
        .iter()
        .map(|(target, bytes)| (format!("{name}/{target}"), bytes.as_slice()))
        .collect();
    let zip = export::zip(named.iter().map(|(n, b)| (n.as_str(), *b)))?;
    write_atomic(&exported.zip, &zip)?;
    Ok(exported)
}

/// Each diagram's SVG in the folder, as [`svg_of`] names it. Two diagrams
/// that would be one SVG, or one that would be `index.svg` while another
/// is the main diagram, are refused.
fn targets(files: &[&str], main: Option<&str>) -> Result<Vec<String>> {
    let mut taken: HashMap<String, &str> = HashMap::new();
    if let Some(main) = main {
        taken.insert(INDEX.to_owned(), main);
    }
    let mut targets = Vec::with_capacity(files.len());
    for &file in files {
        let target = svg_of(file);
        if let Some(other) = taken.get(&target).filter(|other| **other != file) {
            if target == INDEX {
                anyhow::bail!(
                    "{file} would be {INDEX}, which is the main diagram {other}; rename one of them"
                );
            }
            anyhow::bail!("{other} and {file} would both be {target}; rename one of them");
        }
        taken.insert(target.clone(), file);
        targets.push(target);
    }
    Ok(targets)
}

/// The ZIP's name: the project's, else its folder's, with nothing in it
/// that a file name cannot hold.
fn zip_name(name: Option<&str>, root: &Path) -> String {
    let folder = root.file_name().map(|n| n.to_string_lossy().into_owned());
    let name = name.map(str::to_owned).or(folder).unwrap_or_default();
    let clean: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '-'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim_start_matches('.').trim().to_owned();
    if clean.is_empty() {
        "project".to_owned()
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_diagram_has_an_svg_of_its_own() {
        assert_eq!(
            targets(
                &["system.puml", "orders/orders.puml", "a.puml.txt"],
                Some("system.puml")
            )
            .unwrap(),
            ["system.svg", "orders/orders.svg", "a.puml.svg"]
        );
        let twice = targets(&["orders/a.puml", "orders/a.mmd"], None).unwrap_err();
        assert!(
            twice
                .to_string()
                .contains("orders/a.puml and orders/a.mmd would both be orders/a.svg"),
            "{twice}"
        );
        // The main diagram may be called index; another may not.
        assert!(targets(&["index.puml", "b.mmd"], Some("index.puml")).is_ok());
        let index = targets(&["index.mmd", "map.puml"], Some("map.puml")).unwrap_err();
        assert!(
            index.to_string().contains("index.mmd would be index.svg"),
            "{index}"
        );
    }

    #[test]
    fn the_zip_is_named_after_the_project() {
        let root = Path::new("/work/shop-specs");
        assert_eq!(zip_name(Some("Shop: orders/pay"), root), "Shop- orders-pay");
        assert_eq!(zip_name(None, root), "shop-specs");
        assert_eq!(zip_name(Some(" .. "), root), "project");
    }
}
