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
        if bare.map.wires.iter().any(|wire| !wire.fixed) {
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
        // A link of a tree is the layout's alone.
        let Some(wire) = bare.map.wires.iter().find(|wire| !wire.fixed) else {
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

/// docs/adr/0021: a box held before its neighbor stands left of it, in
/// every drawer of graph diagrams; an entry for no box is said.
#[test]
fn a_box_held_before_its_neighbor_stands_left_of_it_in_every_drawer() {
    use omaspec_uml::hand::{Hold, Word};
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let diagrams = [
        (
            "@startuml\nstate A\nstate B\nstate C\nA --> B\nA --> C\n@enduml\n",
            Language::PlantUml,
        ),
        (
            "@startuml\nclass A\nclass B\nclass C\nA --> B\nA --> C\n@enduml\n",
            Language::PlantUml,
        ),
        ("flowchart TD\n  A --> B\n  A --> C\n", Language::Mermaid),
        ("stateDiagram-v2\n  A --> B\n  A --> C\n", Language::Mermaid),
        ("classDiagram\n  A --> B\n  A --> C\n", Language::Mermaid),
    ];
    let hold = |name: &str, beside: &str| Hold {
        name: name.into(),
        beside: beside.into(),
        word: Word::Before,
    };
    for (code, language) in diagrams {
        let ends = |map: &omaspec_uml::map::Map| {
            let to = |name: &str| {
                map.wires
                    .iter()
                    .find(|w| w.key.from == "A" && w.key.to == name)
                    .unwrap_or_else(|| panic!("{code}: no line A -> {name}"))
                    .to
                    .rect
            };
            (to("B"), to("C"))
        };
        let bare = renderer.render(code, language).unwrap();
        let (b, c) = ends(&bare.map);
        assert!(b.x < c.x, "{code}: B starts left of C");
        let (range, text) = hand::hold(code, language, "C", Some(&hold("C", "B"))).unwrap();
        let held = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        let drawn = renderer.render(&held, language).unwrap();
        let (b, c) = ends(&drawn.map);
        assert!(c.x < b.x && (c.y - b.y).abs() < 0.5, "{held}");
        assert!(
            drawn.map.misses.is_empty(),
            "{held}: {:?}",
            drawn.map.misses
        );
        // The map knows the box's entry, also for a diagram in a longer
        // text.
        let entry = held.lines().position(|l| l.contains("C before B"));
        let place = drawn.map.places.iter().find(|p| p.name == "C").unwrap();
        assert_eq!(place.entry, entry, "{held}");
        let shifted = drawn.map.clone().shifted(5);
        let place = shifted.places.iter().find(|p| p.name == "C").unwrap();
        assert_eq!(place.entry, entry.map(|e| e + 5));
        assert_eq!(
            lint::lint(&held, language),
            lint::lint(code, language),
            "{held}"
        );
        let (range, text) = hand::hold(code, language, "C", Some(&hold("C", "Nobody"))).unwrap();
        let nowhere = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        let missed = renderer.render(&nowhere, language).unwrap();
        assert_eq!(missed.svg, bare.svg, "{nowhere}");
        assert_eq!(
            missed
                .map
                .misses
                .iter()
                .map(|(_, why)| why.as_str())
                .collect::<Vec<_>>(),
            ["No box is named Nobody; C before Nobody is left out"],
            "{nowhere}"
        );
    }
}

/// docs/adr/0028: a box held below its neighbor stands in a rank under it,
/// in every drawer of graph diagrams, and no line is drawn for it; level
/// with it, it keeps its rank.
#[test]
fn a_box_held_below_its_neighbor_stands_under_it_in_every_drawer() {
    use omaspec_uml::hand::{Hold, Word};
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let diagrams = [
        (
            "@startuml\nstate A\nstate B\nstate C\nA --> B\nA --> C\n@enduml\n",
            Language::PlantUml,
        ),
        (
            "@startuml\nclass A\nclass B\nclass C\nA --> B\nA --> C\n@enduml\n",
            Language::PlantUml,
        ),
        ("flowchart TD\n  A --> B\n  A --> C\n", Language::Mermaid),
        ("stateDiagram-v2\n  A --> B\n  A --> C\n", Language::Mermaid),
        ("classDiagram\n  A --> B\n  A --> C\n", Language::Mermaid),
    ];
    for (code, language) in diagrams {
        let place = |map: &omaspec_uml::map::Map, name: &str| {
            map.places
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("{code}: no place {name}"))
                .rect
        };
        let bare = renderer.render(code, language).unwrap();
        for word in [Word::Below, Word::Level] {
            let hold = Hold {
                name: "C".into(),
                beside: "B".into(),
                word,
            };
            let (range, text) = hand::hold(code, language, "C", Some(&hold)).unwrap();
            let held = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
            let drawn = renderer.render(&held, language).unwrap();
            assert!(
                drawn.map.misses.is_empty(),
                "{held}: {:?}",
                drawn.map.misses
            );
            let (b, c) = (place(&drawn.map, "B"), place(&drawn.map, "C"));
            match word {
                Word::Below => assert!(c.y > b.y + b.h, "{held}"),
                _ => assert!((c.y + c.h / 2.0 - b.y - b.h / 2.0).abs() < 0.5, "{held}"),
            }
            assert_eq!(
                drawn.map.wires.len(),
                bare.map.wires.len(),
                "{held}: no line more"
            );
            assert_eq!(
                lint::lint(&held, language),
                lint::lint(code, language),
                "{held}"
            );
        }
    }
}

/// A flow that runs up or to the left is laid down or to the right and
/// mirrored after: a box held above its neighbor still stands above it on
/// the page, one held left of it left of it.
#[test]
fn a_box_held_by_the_page_keeps_its_side_in_a_mirrored_flow() {
    use omaspec_uml::hand::{Hold, Word};
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let cases = [
        (
            "stateDiagram-v2\n  direction BT\n  A --> B\n  A --> C\n",
            Word::Above,
        ),
        ("flowchart BT\n  A --> B\n  A --> C\n", Word::Above),
        ("flowchart RL\n  A --> B\n  A --> C\n", Word::Left),
    ];
    for (code, word) in cases {
        let hold = Hold {
            name: "C".into(),
            beside: "B".into(),
            word,
        };
        let (range, text) = hand::hold(code, Language::Mermaid, "C", Some(&hold)).unwrap();
        let held = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        let drawn = renderer.render(&held, Language::Mermaid).unwrap();
        assert!(
            drawn.map.misses.is_empty(),
            "{held}: {:?}",
            drawn.map.misses
        );
        let place = |name: &str| {
            drawn
                .map
                .places
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("{held}: no place {name}"))
                .rect
        };
        let (b, c) = (place("B"), place("C"));
        match word {
            Word::Above => assert!(c.y + c.h < b.y, "{held}: {c:?} not above {b:?}"),
            _ => assert!(c.x + c.w < b.x, "{held}: {c:?} not left of {b:?}"),
        }
    }
}
