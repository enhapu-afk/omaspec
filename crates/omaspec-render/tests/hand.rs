//! A block of lines by hand (docs/adr/0016) is comments to every engine:
//! entries for lines that are not there change no picture and no lint,
//! and are named as not holding. An entry for a line draws it in its
//! shape, the one the pane shows while dragging, in every drawer of graph
//! diagrams.

use std::path::{Path, PathBuf};

use omaspec_core::hand;
use omaspec_core::{Language, lint};
use omaspec_render::{Config, Renderer};
use omaspec_uml::hand::{Corner, Key, Line};

#[test]
fn entries_for_no_line_change_no_gallery_picture() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let nowhere = Line {
        key: Key {
            from: "Nowhere".into(),
            to: "Nobody".into(),
            nth: 2,
        },
        corners: vec![Corner { x: 0.5, y: -0.4 }],
    };
    let gallery = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery");
    let mut seen = 0;
    for entry in std::fs::read_dir(&gallery).unwrap() {
        let path = entry.unwrap().path();
        let language = match path.extension().and_then(|e| e.to_str()) {
            Some("puml") => Language::PlantUml,
            Some("mmd") => Language::Mermaid,
            _ => continue,
        };
        let code = std::fs::read_to_string(&path).unwrap();
        let (range, text) = hand::edit(&code, language, &nowhere).unwrap();
        let with = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        assert!(with.contains("Nowhere -> Nobody #2 at 0.5 -0.4"), "{with}");

        let name = path.display();
        let bare = renderer.render(&code, language).unwrap();
        let drawn = renderer.render(&with, language).unwrap();
        assert_eq!(drawn.engine, bare.engine, "{name}");
        assert!(
            drawn.svg == bare.svg,
            "{name}: the block changed the picture"
        );
        assert_eq!(
            lint::lint(&with, language),
            lint::lint(&code, language),
            "{name}"
        );
        // A drawer that knows its lines says the entry names none of them.
        if !bare.map.wires.is_empty() {
            assert!(
                drawn
                    .map
                    .misses
                    .iter()
                    .any(|(_, why)| why.contains("Nowhere -> Nobody #2")),
                "{name}: {:?}",
                drawn.map.misses
            );
        }
        seen += 1;
    }
    assert!(seen >= 20, "only {seen} examples");
}

fn gallery() -> Vec<(PathBuf, Language)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery");
    let mut out: Vec<(PathBuf, Language)> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| {
            let path = entry.unwrap().path();
            let language = match path.extension()?.to_str()? {
                "puml" => Language::PlantUml,
                "mmd" => Language::Mermaid,
                _ => return None,
            };
            Some((path, language))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[test]
fn a_line_by_hand_is_drawn_as_the_pane_shows_it_in_every_graph_drawer() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let mut kinds = std::collections::BTreeSet::new();
    for (path, language) in gallery() {
        let code = std::fs::read_to_string(&path).unwrap();
        let bare = renderer.render(&code, language).unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let Some(wire) = bare.map.wires.first() else {
            continue;
        };
        // Around the outside of its two boxes: left of them when they lie
        // above each other, above them when they lie side by side.
        let (a, b) = (wire.from.rect, wire.to.rect);
        let (ca, cb) = (
            (a.x + a.w / 2.0, a.y + a.h / 2.0),
            (b.x + b.w / 2.0, b.y + b.h / 2.0),
        );
        let corners = if (ca.1 - cb.1).abs() >= (ca.0 - cb.0).abs() {
            let x = a.x.min(b.x) - 60.0;
            [(x, ca.1), (x, cb.1)]
        } else {
            let y = a.y.min(b.y) - 60.0;
            [(ca.0, y), (cb.0, y)]
        };
        let line = wire.entry_for(&corners);
        let (range, text) = hand::edit(&code, language, &line).unwrap();
        let with = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        let drawn = renderer.render(&with, language).unwrap();
        assert_eq!(drawn.engine, bare.engine, "{name}");
        let laid = drawn
            .map
            .wires
            .iter()
            .find(|w| w.key == wire.key)
            .unwrap_or_else(|| panic!("{name}: {} is gone", wire.key));
        assert!(laid.entry.is_some(), "{name}");
        assert_ne!(drawn.svg, bare.svg, "{name}");
        // Its corners where the pane put them, in the drawing's units.
        let back = |fit: omaspec_uml::map::Fit, points: &[(f32, f32)]| -> Vec<(f32, f32)> {
            points.iter().map(|&p| fit.back(p)).collect()
        };
        let shown = wire.shaped(&corners, bare.map.fit);
        let expected = back(bare.map.fit, &shown[1..shown.len() - 1]);
        let got = back(drawn.map.fit, laid.corners());
        assert_eq!(got.len(), expected.len(), "{name}: {got:?} {expected:?}");
        for (g, e) in got.iter().zip(&expected) {
            assert!(
                (g.0 - e.0).abs() < 0.5 && (g.1 - e.1).abs() < 0.5,
                "{name}: {got:?} {expected:?}"
            );
        }
        // Anything it runs through is said.
        for (_, why) in &drawn.map.misses {
            assert!(why.contains(&wire.key.to_string()), "{name}: {why}");
        }
        kinds.insert(format!("{language:?} {}", name.split('.').next().unwrap()));
    }
    assert!(kinds.len() >= 8, "only {kinds:?}");
}
