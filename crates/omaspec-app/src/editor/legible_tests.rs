//! The diagrams in the colors of every bundled Omarchy theme, as the pane
//! draws them while it follows the theme, and in the light and the dark
//! look of an export: each text reads at 4.5:1 against what lies under it,
//! each line at 3:1 against the paper. And each one in both looks makes
//! one SVG for both color schemes (`render --look auto`).

use omaspec_core::skin::{Colors, Pack};
use omaspec_render::contrast::{self, Kind};
use omaspec_render::{Config, Renderer};
use omaspec_theme::Palette;

use crate::gallery::EXAMPLES;
use crate::look::{self, Hex, Look};

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

fn renderer() -> Renderer {
    Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    })
}

/// The pairs of every gallery example in `colors` that read too faintly,
/// named after `name`.
fn faults(renderer: &mut Renderer, name: &str, colors: &Colors) -> Vec<String> {
    let mut faults = Vec::new();
    for example in EXAMPLES {
        let language = example.language();
        let source = match look::overlay(example.text, language, Pack::Omarchy, colors) {
            Some((shown, _)) => shown,
            None => example.text.to_owned(),
        };
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
    faults
}

fn assert_none(faults: &[String]) {
    if std::env::var_os("OMASPEC_CONTRAST").is_some() {
        for fault in faults {
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

#[test]
fn the_gallery_reads_in_every_bundled_theme() {
    let mut renderer = renderer();
    let mut all = Vec::new();
    for (name, palette) in themes() {
        all.extend(faults(&mut renderer, &name, &Hex::of(&palette).colors()));
    }
    assert_none(&all);
}

#[test]
fn the_gallery_reads_in_the_light_and_the_dark_look() {
    let mut renderer = renderer();
    let mut all = faults(&mut renderer, "light", &look::LIGHT);
    all.extend(faults(&mut renderer, "dark", &look::DARK));
    assert_none(&all);
}

#[test]
fn every_example_makes_one_svg_for_both_color_schemes() {
    let mut renderer = renderer();
    let mut failed = Vec::new();
    for example in EXAMPLES {
        let language = example.language();
        let mut draw = |look: Look| {
            let (text, _) = look::apply(example.text, language, look);
            renderer.render(&text, language).unwrap().svg
        };
        let (light, dark) = (draw(Look::Light), draw(Look::Dark));
        match omaspec_render::schemes::both(&light, &dark) {
            Ok(both) => assert!(
                both.contains("prefers-color-scheme: dark"),
                "{}: no dark colors",
                example.file
            ),
            Err(err) => failed.push(format!("{}: {err}", example.file)),
        }
    }
    assert!(failed.is_empty(), "{failed:#?}");
}
