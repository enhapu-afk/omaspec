//! Spike: can mmdr render the Mermaid diagram types Omagram wants to offer,
//! how fast, and how well?
//!
//! Usage: `spike-mermaid [--out DIR] FILE_OR_DIR...`
//!
//! For every `.mmd` / `.mermaid` file it prints the first-render time, the
//! median of repeated renders, the rasterization time, and writes the SVG and
//! a 2x PNG to the output directory (default `spikes/mermaid/out`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mermaid_rs_renderer::{RenderOptions, render_with_options};
use resvg::{tiny_skia, usvg};

const WARM_RUNS: usize = 15;

fn main() -> Result<()> {
    let mut out = PathBuf::from("spikes/mermaid/out");
    let mut inputs = Vec::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--out" {
            out = args.next().context("--out needs a directory")?.into();
        } else {
            inputs.push(PathBuf::from(arg));
        }
    }
    anyhow::ensure!(
        !inputs.is_empty(),
        "usage: spike-mermaid [--out DIR] FILE_OR_DIR..."
    );

    let mut files = Vec::new();
    for input in &inputs {
        collect(input, &mut files)?;
    }
    files.sort();
    fs::create_dir_all(&out)?;

    let mut svg_options = usvg::Options::default();
    let fonts = Instant::now();
    svg_options.fontdb_mut().load_system_fonts();
    println!(
        "system fonts for rasterizing: {:.1} ms\n",
        ms(fonts.elapsed())
    );

    println!(
        "{:<34} {:<16} {:>8} {:>8} {:>9}  result",
        "file", "type", "first", "median", "raster"
    );
    let (mut ok, mut failed) = (0, 0);
    for file in &files {
        let source = fs::read_to_string(file)?;
        let name = file.file_stem().unwrap_or_default().to_string_lossy();
        let kind = diagram_kind(&source);

        let start = Instant::now();
        let first = render_with_options(&source, RenderOptions::default());
        let first_time = start.elapsed();

        let svg = match first {
            Ok(svg) => svg,
            Err(err) => {
                failed += 1;
                let reason = err.to_string();
                let reason = reason.lines().next().unwrap_or_default();
                println!(
                    "{name:<34} {kind:<16} {:>8.2} {:>8} {:>9}  ERROR {reason}",
                    ms(first_time),
                    "-",
                    "-"
                );
                continue;
            }
        };

        let mut runs: Vec<Duration> = (0..WARM_RUNS)
            .map(|_| {
                let start = Instant::now();
                let _ = render_with_options(&source, RenderOptions::default());
                start.elapsed()
            })
            .collect();
        runs.sort();
        let median = runs[WARM_RUNS / 2];

        let start = Instant::now();
        let raster = rasterize(&svg, &svg_options, 2.0);
        let raster_time = start.elapsed();

        fs::write(out.join(format!("{name}.svg")), &svg)?;
        let result = match raster {
            Ok(pixmap) => {
                pixmap.save_png(out.join(format!("{name}.png")))?;
                ok += 1;
                format!(
                    "ok {}x{} px, svg {} KB",
                    pixmap.width(),
                    pixmap.height(),
                    svg.len() / 1024
                )
            }
            Err(err) => {
                failed += 1;
                format!("RASTER ERROR {err}")
            }
        };
        println!(
            "{name:<34} {kind:<16} {:>8.2} {:>8.2} {:>9.2}  {result}",
            ms(first_time),
            ms(median),
            ms(raster_time)
        );
    }
    println!(
        "\n{ok} rendered, {failed} failed; output in {}",
        out.display()
    );
    Ok(())
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect(&entry?.path(), files)?;
        }
    } else if matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("mmd" | "mermaid")
    ) {
        files.push(path.to_owned());
    }
    Ok(())
}

/// The diagram header: the first line that is not a comment or front matter.
fn diagram_kind(source: &str) -> String {
    let mut in_front_matter = false;
    for line in source.lines().map(str::trim) {
        if line == "---" {
            in_front_matter = !in_front_matter;
            continue;
        }
        if in_front_matter || line.is_empty() || line.starts_with("%%") {
            continue;
        }
        return line
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
    }
    String::new()
}

fn rasterize(svg: &str, options: &usvg::Options, scale: f32) -> Result<tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_str(svg, options)?;
    let size = tree
        .size()
        .to_int_size()
        .scale_by(scale)
        .context("empty SVG")?;
    let mut pixmap =
        tiny_skia::Pixmap::new(size.width(), size.height()).context("SVG too large")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
