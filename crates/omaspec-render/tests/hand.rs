//! A block of lines by hand (docs/adr/0016) is comments to every engine:
//! entries for lines that are not there change no picture and no lint,
//! and are named as not holding. An entry for a line lays it through its
//! bend, in every drawer of graph diagrams.

use std::path::{Path, PathBuf};

use omaspec_core::hand;
use omaspec_core::{Language, lint};
use omaspec_render::{Config, Renderer};
use omaspec_uml::hand::{Key, Line, Via};

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
        vias: vec![Via {
            along: 0.5,
            aside: -60.0,
        }],
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
        assert!(with.contains("Nowhere -> Nobody #2 via 0.5 -60"), "{with}");

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

/// The distance from `p` to the nearest run of `points`.
fn off(points: &[(f32, f32)], p: (f32, f32)) -> f32 {
    points
        .windows(2)
        .map(|w| {
            let ((ax, ay), (bx, by)) = (w[0], w[1]);
            let (dx, dy) = (bx - ax, by - ay);
            let t = (((p.0 - ax) * dx + (p.1 - ay) * dy) / (dx * dx + dy * dy).max(1e-6))
                .clamp(0.0, 1.0);
            (ax + t * dx - p.0).hypot(ay + t * dy - p.1)
        })
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn a_line_by_hand_runs_by_its_bend_in_every_graph_drawer() {
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
        // Well off to the side of the way between its ends.
        let line = Line {
            key: wire.key.clone(),
            vias: vec![Via {
                along: 0.5,
                aside: 120.0,
            }],
        };
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
        if drawn.map.misses.is_empty() {
            let bend = laid.bends(drawn.map.fit)[0];
            // The corner at the bend curves, inside its corridor.
            let off = off(&laid.points, bend);
            assert!(off < 21.0, "{name}: {} is {off} off its bend", wire.key);
            assert_ne!(drawn.svg, bare.svg, "{name}");
            kinds.insert(format!("{language:?} {}", name.split('.').next().unwrap()));
        } else {
            // Named as not holding, and drawn as before.
            assert!(
                drawn.map.misses[0].1.contains(&wire.key.to_string()),
                "{name}"
            );
        }
    }
    assert!(kinds.len() >= 8, "only {kinds:?}");
}
