//! The sketch look (docs/adr/0023): every diagram of the repository that
//! Omaspec draws itself draws by hand too when it asks, PlantUML's with
//! `skinparam handwritten true` and Mermaid's with `look: handDrawn`, in
//! the same engine, always the same picture, and its words as before,
//! set in the handwriting face.

use std::fs;
use std::path::{Path, PathBuf};

use omaspec_core::Language;
use omaspec_render::{Config, Engine, Renderer};

fn files(dirs: &[&str], extension: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out: Vec<PathBuf> = dirs
        .iter()
        .flat_map(|dir| fs::read_dir(root.join(dir)).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == extension))
        .collect();
    out.sort();
    out
}

/// `skinparam handwritten true` after each `@start…` line.
fn handwritten(code: &str) -> String {
    let mut out = String::new();
    for line in code.split_inclusive('\n') {
        out.push_str(line);
        if line.trim_start().to_ascii_lowercase().starts_with("@start") {
            out.push_str("skinparam handwritten true\n");
        }
    }
    out
}

/// `look: handDrawn` in the front matter's `config`, or a front matter
/// that says so before the diagram.
fn hand_drawn(code: &str) -> String {
    match code.strip_prefix("---\n") {
        Some(rest) => format!("---\nconfig:\n  look: handDrawn\n{rest}"),
        None => format!("---\nconfig:\n  look: handDrawn\n---\n{code}"),
    }
}

/// A ruled shape inside the picture. The page rect behind it does not count.
fn geometric(svg: &str) -> bool {
    let body = svg.split_once("<g").map(|(_, rest)| rest).unwrap_or(svg);
    body.contains("<rect") || body.contains("<ellipse") || body.contains("<polygon")
}

/// The words of a picture: what lies between `>` and `<`.
fn words(svg: &str) -> Vec<&str> {
    svg.split('>')
        .filter_map(|part| part.split('<').next())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .collect()
}

/// The PlantUML and Mermaid kinds each file is, for a count.
fn check(language: Language, paths: &[PathBuf], sketch: fn(&str) -> String) -> usize {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let native = match language {
        Language::PlantUml => Engine::NativePlantUml,
        _ => Engine::NativeMermaid,
    };
    let mut drawn = 0;
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy();
        let code = fs::read_to_string(path).unwrap();
        let Ok(plain) = renderer.render(&code, language) else {
            continue;
        };
        if plain.engine != native || plain.svg.contains(r#"stroke-linecap="round""#) {
            continue;
        }
        let sketched = sketch(&code);
        let by_hand = renderer
            .render(&sketched, language)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(by_hand.engine, native, "{name}: left to another engine");
        assert!(
            by_hand.svg.contains(r#"stroke-linecap="round""#),
            "{name}: not drawn by hand"
        );
        assert_eq!(
            words(&by_hand.svg),
            words(&plain.svg),
            "{name}: the text changed"
        );
        let again = renderer.render(&sketched, language).unwrap();
        assert_eq!(again.svg, by_hand.svg, "{name}: a hand that is not steady");
        drawn += 1;
    }
    drawn
}

#[test]
fn every_plantuml_diagram_draws_by_hand() {
    let paths = files(
        &[
            "examples",
            "examples/gallery",
            "crates/omaspec-uml/tests/activity",
            "crates/omaspec-uml/tests/flow",
            "crates/omaspec-uml/tests/gantt",
            "crates/omaspec-uml/tests/mindmap",
            "crates/omaspec-uml/tests/sequence",
            "crates/omaspec-uml/tests/state",
            "crates/omaspec-uml/tests/structure",
            "crates/omaspec-uml/tests/timing",
            "crates/omaspec-uml/tests/wbs",
        ],
        "puml",
    );
    let drawn = check(Language::PlantUml, &paths, handwritten);
    assert!(drawn >= 50, "only {drawn} drawn");
}

#[test]
fn every_mermaid_diagram_draws_by_hand() {
    let paths = files(
        &[
            "examples",
            "examples/gallery",
            "crates/omaspec-render/tests/layered",
        ],
        "mmd",
    );
    let drawn = check(Language::Mermaid, &paths, hand_drawn);
    assert!(drawn >= 25, "only {drawn} drawn");
}

/// One of each kind Omaspec draws itself, the ones the repository has no
/// example of among them.
const KINDS: &[&str] = &[
    "sequenceDiagram\n  Alice->>Bob: hi\n  Bob-->>Alice: hello\n",
    "classDiagram\n  Animal <|-- Duck\n  class Duck {\n    +swim()\n  }\n",
    "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : job\n",
    "mindmap\n  root((Plan))\n    Goals\n    Risks\n",
    "gantt\n  dateFormat YYYY-MM-DD\n  Design :a, 2026-01-01, 3d\n  Build :after a, 5d\n",
    "pie title Pets\n  \"Dogs\" : 3\n  \"Cats\" : 2\n",
    "journey\n  title A day\n  section Morning\n    Coffee: 5: Me\n",
    "timeline\n  title History\n  2002 : LinkedIn\n  2004 : Facebook\n",
    "erDiagram\n  USER ||--o{ ORDER : places\n",
    "C4Context\n  Person(user, \"User\")\n  System(app, \"App\")\n  Rel(user, app, \"Uses\")\n",
    "architecture-beta\n  service db(database)[Database]\n  service api(server)[API]\n  db:L -- R:api\n",
    "block-beta\n  columns 2\n  a b\n",
    "flowchart LR\n  A[Edit] --> B{Saved?}\n  B -->|yes| C((Render))\n",
];

#[test]
fn every_mermaid_kind_draws_by_hand_from_the_init_too() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    for code in KINDS {
        let plain = renderer.render(code, Language::Mermaid).unwrap();
        assert_eq!(plain.engine, Engine::NativeMermaid, "{code}");
        for sketched in [
            hand_drawn(code),
            format!("%%{{init: {{\"look\": \"handDrawn\"}}}}%%\n{code}"),
        ] {
            let by_hand = renderer.render(&sketched, Language::Mermaid).unwrap();
            assert_eq!(by_hand.engine, Engine::NativeMermaid, "{sketched}");
            assert!(
                by_hand.svg.contains(r#"stroke-linecap="round""#),
                "not drawn by hand: {sketched}"
            );
            // Cards, tiles and a pie used to stay geometric: a fill with no
            // border never reached the hand.
            if code.starts_with("timeline")
                || code.starts_with("journey")
                || code.starts_with("architecture-beta")
                || code.starts_with("pie")
            {
                assert!(
                    !geometric(&by_hand.svg),
                    "still a ruled shape: {sketched}\n{}",
                    by_hand.svg
                );
            }
        }
        // The plain look, asked for, draws as before.
        let classic = format!("---\nconfig:\n  look: classic\n---\n{code}");
        let classic = renderer.render(&classic, Language::Mermaid).unwrap();
        assert_eq!(classic.engine, Engine::NativeMermaid, "{code}");
        assert!(!classic.svg.contains(r#"stroke-linecap="round""#), "{code}");
    }
}

/// A pie of one slice and a JSON card are fills with no border of their own.
#[test]
fn a_fill_with_no_border_is_drawn_by_hand() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let samples = [
        (
            Language::Mermaid,
            "%%{init: {\"look\": \"handDrawn\"}}%%\npie title Pets\n  \"Dogs\" : 1\n",
        ),
        (
            Language::PlantUml,
            "@startjson\nskinparam handwritten true\n{\"a\": 1}\n@endjson\n",
        ),
    ];
    for (language, code) in samples {
        let svg = renderer.render(code, language).unwrap().svg;
        assert!(!geometric(&svg), "{code}\n{svg}");
    }
}

/// The words are set in the bundled handwriting face, and usvg uses it.
#[test]
fn a_sketch_writes_in_the_handwriting_face() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let svg = renderer
        .render(
            "@startuml\nskinparam handwritten true\nclass Book\n@enduml\n",
            Language::PlantUml,
        )
        .unwrap()
        .svg;
    assert!(svg.contains("font-family=\"Excalifont\""), "{svg}");
    let face = omaspec_render::export::parse(&svg).unwrap();
    let plain = omaspec_render::export::parse(&svg.replace("Excalifont", "sans-serif")).unwrap();
    let drawn = omaspec_render::export::rasterize(&face, 2.0).unwrap();
    let other = omaspec_render::export::rasterize(&plain, 2.0).unwrap();
    assert_eq!(
        (drawn.width(), drawn.height()),
        (other.width(), other.height())
    );
    assert_ne!(drawn.data(), other.data());
}

/// The strokes of a picture drawn by hand, each less where it starts, so
/// one drawn elsewhere compares equal; the long ones only, which are boxes.
fn box_strokes(svg: &str) -> Vec<Vec<(f32, f32)>> {
    svg.split("<path d=\"")
        .skip(1)
        .filter(|part| part.contains(r#"fill="none""#) && part.contains("stroke-linecap"))
        .map(|part| {
            let d = &part[..part.find('"').unwrap()];
            let numbers: Vec<f32> = d
                .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .filter(|s| !s.is_empty())
                .map(|s| s.parse().unwrap())
                .collect();
            let (x0, y0) = (numbers[0], numbers[1]);
            numbers.chunks(2).map(|p| (p[0] - x0, p[1] - y0)).collect()
        })
        .filter(|points: &Vec<(f32, f32)>| points.len() > 40)
        .collect()
}

fn same(a: &[(f32, f32)], b: &[(f32, f32)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| (p.0 - q.0).abs() < 0.05 && (p.1 - q.1).abs() < 0.05)
}

#[test]
fn a_box_keeps_its_strokes_when_the_layout_moves_it() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let mut draw = |code: &str| renderer.render(code, Language::PlantUml).unwrap().svg;
    // `Busy` and `Done` move down a rank when `Waking` comes first.
    let before =
        draw("@startuml\nskinparam handwritten true\n[*] --> Busy\nBusy --> Done\n@enduml\n");
    let after = draw(
        "@startuml\nskinparam handwritten true\n[*] --> Waking\nWaking --> Busy\nBusy --> Done\n@enduml\n",
    );
    let (before, after) = (box_strokes(&before), box_strokes(&after));
    // The two states and the start; Waking comes in.
    assert_eq!((before.len(), after.len()), (3, 4));
    for stroke in &before {
        assert!(after.iter().any(|other| same(stroke, other)), "{stroke:?}");
    }
    // Two boxes of one size, seeded by their names, are drawn apart.
    let pair = draw("@startuml\nskinparam handwritten true\nstate A\nstate B\nA --> B\n@enduml\n");
    let pair = box_strokes(&pair);
    assert_eq!(pair.len(), 2);
    assert!(!same(&pair[0], &pair[1]));
}
