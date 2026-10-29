//! Ctrl+click on every label of the Mermaid fixtures and of the PlantUML
//! sequence diagrams finds the line that holds it. Text the engine
//! computes (dates on a time axis, percentages, message numbers) is in no
//! line and is left out.

use omaspec_core::{Language, locate};
use omaspec_render::export::{self, usvg};
use omaspec_render::{Config, Renderer, pick};

/// Every label with its middle, in drawing order.
fn labels(group: &usvg::Group, out: &mut Vec<(String, f32, f32)>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => labels(g, out),
            usvg::Node::Text(t) => {
                let parts: Vec<&str> = t.chunks().iter().map(|c| c.text()).collect();
                let text = parts
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let rect = t.abs_bounding_box();
                if !text.is_empty() {
                    out.push((
                        text,
                        rect.left() + rect.width() / 2.0,
                        rect.top() + rect.height() / 2.0,
                    ));
                }
            }
            _ => {}
        }
    }
}

fn computed(text: &str) -> bool {
    text.chars()
        .all(|c| c.is_ascii_digit() || "%-.:/ ".contains(c))
}

#[test]
fn every_written_label_leads_to_its_line() {
    let dir = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../omaspec-core/tests/fixtures"
    );
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "mmd"))
        .collect();
    paths.sort();
    let mut sources: Vec<(String, String)> = paths
        .iter()
        .map(|p| (p.display().to_string(), std::fs::read_to_string(p).unwrap()))
        .collect();
    // The sequence fixture has a typo on purpose; this one draws.
    sources.push((
        "sequence".into(),
        "sequenceDiagram\n  participant W as Web app\n  participant A as Auth\n  W->>A: log in\n  Note right of A: checks the password\n  loop three times\n    A-->>W: try again\n  end\n  alt locked\n    A-->>W: locked out\n  else fine\n    A-->>W: welcome\n  end\n".into(),
    ));
    let mut checked = 0;
    for (path, code) in sources {
        // Some fixtures are broken on purpose.
        let Ok(out) = renderer.render(&code, Language::Mermaid) else {
            continue;
        };
        let tree = export::parse(&out.svg).unwrap();
        let mut all = Vec::new();
        labels(tree.root(), &mut all);
        let lines: Vec<&str> = code.split('\n').collect();
        for (text, x, y) in all.iter().filter(|(text, ..)| !computed(text)) {
            let label = pick::label_at(&tree, *x, *y).expect(text);
            let line = locate::line_of(&code, &label.text, label.nth);
            let first_word = text
                .split_whitespace()
                .next()
                .unwrap()
                .trim_matches(['[', ']']);
            assert!(
                line.is_some_and(|l| lines[l].contains(first_word)),
                "{path}: {text:?} led to {:?}",
                line.map(|l| lines[l])
            );
            checked += 1;
        }
    }
    assert!(checked > 50, "only {checked} labels checked");
}

#[test]
fn every_native_plantuml_label_leads_to_its_line() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let mut checked = 0;
    for path in [
        "examples/login.puml",
        "examples/ideas.puml",
        "examples/launch.puml",
        "examples/release.puml",
        "examples/player.puml",
        "crates/omaspec-uml/tests/state/states.puml",
        "crates/omaspec-uml/tests/state/sideways.puml",
        "crates/omaspec-core/tests/fixtures/state-player.puml",
        "crates/omaspec-uml/tests/sequence/every-construct.puml",
        "crates/omaspec-uml/tests/activity/every-step.puml",
        "crates/omaspec-uml/tests/activity/partitions.puml",
        "crates/omaspec-uml/tests/activity/backward.puml",
        "crates/omaspec-uml/tests/mindmap/mindmap.puml",
        "crates/omaspec-uml/tests/wbs/wbs.puml",
        "crates/omaspec-uml/tests/structure/class.puml",
        "crates/omaspec-uml/tests/structure/component.puml",
        "crates/omaspec-uml/tests/structure/usecase.puml",
        "crates/omaspec-uml/tests/structure/deploy.puml",
        "crates/omaspec-uml/tests/structure/object.puml",
        "crates/omaspec-uml/tests/timing/browser.puml",
        "crates/omaspec-uml/tests/timing/signals.puml",
        "crates/omaspec-uml/tests/gantt/build.puml",
    ] {
        let code = std::fs::read_to_string(format!("{root}/{path}")).unwrap();
        let out = renderer.render(&code, Language::PlantUml).unwrap();
        let tree = export::parse(&out.svg).unwrap();
        let mut all = Vec::new();
        labels(tree.root(), &mut all);
        let lines: Vec<&str> = code.split('\n').collect();
        for (text, x, y) in all.iter().filter(|(text, ..)| !computed(text)) {
            let label = pick::label_at(&tree, *x, *y).expect(text);
            let line = locate::line_of(&code, &label.text, label.nth);
            let first_word = text
                .split_whitespace()
                .next()
                .unwrap()
                .trim_matches(['[', ']', '«', '»']);
            assert!(
                line.is_some_and(|l| lines[l].contains(first_word)),
                "{path}: {text:?} led to {:?}",
                line.map(|l| lines[l])
            );
            checked += 1;
        }
    }
    assert!(checked > 60, "only {checked} labels checked");
}
