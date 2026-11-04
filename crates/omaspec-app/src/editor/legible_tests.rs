//! The diagrams in the colors of every bundled Omarchy theme, as the pane
//! draws them while it follows the theme: each text reads at 4.5:1 against
//! what lies under it, each line at 3:1 against the paper.

use omaspec_core::Language;
use omaspec_core::skin::{self, Pack};
use omaspec_render::contrast::{self, Kind};
use omaspec_render::{Config, Renderer};
use omaspec_theme::Palette;

use super::Hex;
use crate::gallery::EXAMPLES;

/// Least contrast of a text with what lies under it (WCAG AA).
const TEXT: f32 = 4.5;
/// Least contrast of a line with the paper (WCAG's for graphics).
const LINE: f32 = 3.0;

/// The bundled themes' palettes, as Omaspec reads them.
fn themes() -> Vec<(String, Palette)> {
    let Ok(dirs) = std::fs::read_dir("/usr/share/omarchy/themes") else {
        return Vec::new();
    };
    let mut themes: Vec<(String, Palette)> = dirs
        .flatten()
        .filter_map(|dir| {
            let palette = Palette::load(&dir.path().join("colors.toml")).ok()?;
            Some((
                dir.file_name().to_string_lossy().into_owned(),
                palette.legible(),
            ))
        })
        .collect();
    themes.sort_by(|a, b| a.0.cmp(&b.0));
    themes
}

/// `text` with the Omarchy pack of `palette` laid over it.
fn themed(text: &str, language: Language, palette: &Palette) -> String {
    let hex = Hex::of(palette);
    match skin::edit(text, language, Some(Pack::Omarchy), &hex.colors()) {
        Some((range, block)) => format!("{}{block}{}", &text[..range.start], &text[range.end..]),
        None => text.to_owned(),
    }
}

#[test]
fn the_gallery_reads_in_every_bundled_theme() {
    let themes = themes();
    if themes.is_empty() {
        return;
    }
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let verbose = std::env::var_os("OMASPEC_CONTRAST").is_some();
    let mut faults = Vec::new();
    for (name, palette) in &themes {
        for example in EXAMPLES {
            let language = example.language();
            let source = themed(example.text, language, palette);
            let svg = match renderer.render(&source, language) {
                Ok(out) => out.svg,
                Err(err) => {
                    faults.push(format!("{name} {}: does not draw: {err}", example.file));
                    continue;
                }
            };
            for pair in contrast::pairs(&svg).unwrap() {
                let (least, what) = match &pair.kind {
                    Kind::Text(words) => (TEXT, format!("text {words:?}")),
                    Kind::Line => (LINE, "line".to_owned()),
                };
                let ratio = pair.ratio();
                if ratio < least {
                    faults.push(format!(
                        "{name} {}: {what} {:?} on {:?} at {:?}: {ratio:.2}",
                        example.file, pair.color, pair.on, pair.at
                    ));
                }
            }
        }
    }
    if verbose {
        for fault in &faults {
            eprintln!("{fault}");
        }
    }
    assert!(
        faults.is_empty(),
        "{} pairs too faint, the first: {:#?}",
        faults.len(),
        &faults[..faults.len().min(20)]
    );
}
