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

/// Diagrams that write colors of their own, for a white page: a box of a
/// sequence, a swimlane, a note, a state, Mermaid's `rect` and `style`;
/// and the circle of every kind of class, whose colors the pack sets.
const OWN_COLORS: &[(&str, omaspec_core::Language, &str)] = &[
    (
        "every circle",
        omaspec_core::Language::PlantUml,
        "@startuml\nclass C\ninterface I\nabstract A\nenum E\nannotation N\nentity T {\n  id\n}\nprotocol P\nstruct S\nexception X\nmetaclass M\nrecord R\ndataclass D\n@enduml\n",
    ),
    (
        "own box",
        omaspec_core::Language::PlantUml,
        "@startuml\nbox \"Backend\" #LightBlue\nparticipant Orders\ndatabase Stock\nend box\nOrders -> Stock : still reserved?\nnote over Orders #FFFDE7 : kept a week\n... two days later ...\nOrders -> Orders : reserve\n@enduml\n",
    ),
    (
        "own lane",
        omaspec_core::Language::PlantUml,
        "@startuml\n|Customer|\nstart\n:Write ticket;\n|#AntiqueWhite|Support|\n:Read ticket;\nif (Known?) then (yes)\n  :Send article;\nelse (no)\n  :Ask;\nendif\nstop\n@enduml\n",
    ),
    (
        "own state",
        omaspec_core::Language::PlantUml,
        "@startuml\nstate Idle #Pink\nstate Busy #LightGreen\n[*] --> Idle\nIdle --> Busy : go\n@enduml\n",
    ),
    (
        "own rect",
        omaspec_core::Language::Mermaid,
        "sequenceDiagram\n  participant A\n  participant S\n  rect rgb(240, 248, 255)\n    loop poll\n      A->>S: ping\n    end\n  end\n",
    ),
    (
        "own style",
        omaspec_core::Language::Mermaid,
        "flowchart LR\n  A --> B\n  style A fill:#FFF3B0\n  classDef soft fill:#E0F2FE\n  class B soft\n",
    ),
];

/// The pairs of every gallery example in `colors` that read too faintly,
/// named after `name`, and of the diagrams with colors of their own.
fn faults(renderer: &mut Renderer, name: &str, colors: &Colors) -> Vec<String> {
    let mut faults = Vec::new();
    let examples = EXAMPLES
        .iter()
        .map(|example| (example.file, example.language(), example.text))
        .chain(OWN_COLORS.iter().copied());
    for (file, language, text) in examples {
        let source = match look::overlay(text, language, Pack::Omarchy, colors, None) {
            Some((shown, _)) => shown,
            None => text.to_owned(),
        };
        let svg = match renderer.render(&source, language) {
            Ok(out) => out.svg,
            Err(err) => {
                faults.push(format!("{name} {file}: does not draw: {err}"));
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
                    "{name} {file}: {what} {:?} on {:?} at {:?}: {ratio:.2}",
                    pair.color, pair.on, pair.at
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
            let (text, _) = look::apply(example.text, language, &look.skin().unwrap());
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
