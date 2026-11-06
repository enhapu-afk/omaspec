//! Containers folded to their box (docs/adr/0024), in every drawer that
//! draws containers: the folded one is a box of its own, what it held is
//! gone, its lines end on it, and lines that came out alike are one.

use omaspec_core::Language;
use omaspec_render::{Config, Engine, Map, Renderer};

fn draw(code: &str, language: Language, folds: &[&str]) -> Map {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let mut text = code.to_owned();
    for name in folds {
        text.push_str(&omaspec_uml::fold::line(
            name,
            language == Language::Mermaid,
        ));
    }
    let drawn = renderer.render(&text, language).unwrap();
    assert!(
        matches!(drawn.engine, Engine::NativePlantUml | Engine::NativeMermaid),
        "{code}"
    );
    drawn.map
}

fn names(map: &Map) -> Vec<&str> {
    map.places.iter().map(|p| p.name.as_str()).collect()
}

fn lines(map: &Map) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = map
        .wires
        .iter()
        .map(|w| (w.key.from.clone(), w.key.to.clone()))
        .collect();
    out.sort();
    out
}

fn pair(from: &str, to: &str) -> (String, String) {
    (from.to_owned(), to.to_owned())
}

/// Checks a container `name` holding `inside`, between `outside` boxes.
fn folds_to_a_box(map: &Map, folded: &Map, name: &str, inside: &str) {
    let open = map.places.iter().find(|p| p.name == name).unwrap();
    assert!(open.frame, "{name} holds others");
    assert!(names(map).contains(&inside));
    let closed = folded.places.iter().find(|p| p.name == name).unwrap();
    assert!(!closed.frame, "{name} is a box of its own");
    assert!(!names(folded).contains(&inside), "{:?}", names(folded));
    assert!(closed.rect.w < open.rect.w || closed.rect.h < open.rect.h);
}

#[test]
fn a_plantuml_state_folds_and_its_lines_end_on_it() {
    let code = "@startuml\n[*] --> Ready\nstate Brewing {\n  [*] --> Grinding\n  Grinding --> Heating\n  Heating --> Pouring\n}\nReady --> Grinding : start\nReady --> Heating\nReady --> Pouring\nPouring --> Ready : cup full\nBrewing --> Ready : cancel\n@enduml\n";
    let map = draw(code, Language::PlantUml, &[]);
    let folded = draw(code, Language::PlantUml, &["Brewing"]);
    folds_to_a_box(&map, &folded, "Brewing", "Grinding");
    let found = lines(&folded);
    // `start` and `cup full` keep their words; the two plain lines from
    // Ready to Heating and Pouring are one now.
    assert_eq!(
        found
            .iter()
            .filter(|l| **l == pair("Ready", "Brewing"))
            .count(),
        2,
        "{found:?}"
    );
    assert_eq!(
        found
            .iter()
            .filter(|l| **l == pair("Brewing", "Ready"))
            .count(),
        2,
        "{found:?}"
    );
    // A name that is no container, or none at all, folds nothing.
    assert_eq!(draw(code, Language::PlantUml, &["Ready", "Nowhere"]), map);
}

#[test]
fn a_package_folds_with_what_it_holds() {
    let code = "@startuml\npackage Lending {\n  class Loan\n  class Copy\n  Loan --> Copy\n}\nclass Member\nMember --> Loan : borrows\nMember --> Copy\n@enduml\n";
    let map = draw(code, Language::PlantUml, &[]);
    let folded = draw(code, Language::PlantUml, &["Lending"]);
    folds_to_a_box(&map, &folded, "Lending", "Loan");
    assert_eq!(
        lines(&folded),
        [pair("Member", "Lending"), pair("Member", "Lending")]
    );
}

#[test]
fn a_mermaid_state_folds() {
    let code = "stateDiagram-v2\n  [*] --> Idle\n  state Busy {\n    [*] --> Working\n    Working --> Resting : tired\n  }\n  Idle --> Working : job\n  Resting --> Idle : done\n";
    let map = draw(code, Language::Mermaid, &[]);
    let folded = draw(code, Language::Mermaid, &["Busy"]);
    folds_to_a_box(&map, &folded, "Busy", "Working");
    assert!(lines(&folded).contains(&pair("Idle", "Busy")));
    assert!(lines(&folded).contains(&pair("Busy", "Idle")));
}

#[test]
fn a_flowchart_subgraph_folds_and_alike_lines_are_one() {
    let code = "flowchart LR\n  A[Push] --> L\n  subgraph Build\n    L[Lint] --> T[Test]\n    L --> D[Docs]\n  end\n  T --> P[Publish]\n  D --> P\n";
    let map = draw(code, Language::Mermaid, &[]);
    let folded = draw(code, Language::Mermaid, &["Build"]);
    folds_to_a_box(&map, &folded, "Build", "L");
    assert_eq!(lines(&folded), [pair("A", "Build"), pair("Build", "P")]);
}
