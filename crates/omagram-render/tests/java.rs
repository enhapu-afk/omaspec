//! The Java PlantUML worker against a stand-in that speaks `-pipe`.

use std::path::PathBuf;
use std::time::Duration;

use omagram_core::Language;
use omagram_render::{Config, Engine, JavaPlantUml, RenderError, Renderer};

fn config(timeout_ms: u64) -> Config {
    Config {
        plantuml: Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fake-plantuml")),
        java_fallback: true,
        java_timeout: Duration::from_millis(timeout_ms),
        java_start_timeout: Duration::from_millis(timeout_ms),
        sandbox: None,
    }
}

fn text_of(svg: &str) -> &str {
    let start = svg.find("<text>").unwrap() + 6;
    &svg[start..svg.find("</text>").unwrap()]
}

/// A salt diagram: the native engine does not draw that kind, so the
/// router asks Java. A class diagram is drawn natively.
const SALT: &str = "@startsalt\n{+\n}\n@endsalt\n";

#[test]
fn one_process_serves_many_diagrams() {
    let mut renderer = Renderer::new(&config(5000));
    assert!(renderer.has_java_fallback());
    for n in 1..=3 {
        let out = renderer.render(SALT, Language::PlantUml).unwrap();
        assert_eq!(out.engine, Engine::JavaPlantUml);
        assert_eq!(text_of(&out.svg), format!("diagram {n}"));
    }
    // What the own engine draws never reaches the Java process.
    let out = renderer
        .render("@startuml\nA -> B\n@enduml\n", Language::PlantUml)
        .unwrap();
    assert_eq!(out.engine, Engine::NativePlantUml);
    let out = renderer.render(SALT, Language::PlantUml).unwrap();
    assert_eq!(text_of(&out.svg), "diagram 4");
}

#[test]
fn json_is_drawn_here_and_a_refusal_keeps_its_line() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let out = renderer
        .render("@startjson\n{\"a\": 1}\n@endjson\n", Language::PlantUml)
        .unwrap();
    assert_eq!(out.engine, Engine::NativePlantUml);
    assert!(out.svg.contains(">1</tspan>"));

    let err = renderer
        .render(
            "@startjson\n#highlight \"a\" <<h1>>\n{\"a\": 1}\n@endjson\n",
            Language::PlantUml,
        )
        .unwrap_err();
    let RenderError::Unavailable {
        line: Some(1),
        reason,
    } = err
    else {
        panic!("{err}");
    };
    assert!(reason.contains("stereotype"), "{reason}");
    assert!(!reason.contains("JSONs are not drawn"), "{reason}");

    let mut renderer = Renderer::new(&config(5000));
    let out = renderer
        .render(
            "@startjson\n#highlight \"a\" <<h1>>\n{\"a\": 1}\n@endjson\n",
            Language::PlantUml,
        )
        .unwrap();
    assert_eq!(out.engine, Engine::JavaPlantUml);
}

#[test]
fn formula_is_drawn_here_and_a_refusal_keeps_its_line() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let out = renderer
        .render("@startlatex\nf(t) = t\n@endlatex\n", Language::PlantUml)
        .unwrap();
    assert_eq!(out.engine, Engine::NativePlantUml);
    assert!(out.svg.contains(">t</tspan>"));
    let out = renderer
        .render("@startmath\nalpha\n@endmath\n", Language::PlantUml)
        .unwrap();
    assert_eq!(out.engine, Engine::NativePlantUml);
    assert!(out.svg.contains("α"));

    let err = renderer
        .render(
            "@startlatex\n<style>\n</style>\nx\n@endlatex\n",
            Language::PlantUml,
        )
        .unwrap_err();
    let RenderError::Unavailable {
        line: Some(1),
        reason,
    } = err
    else {
        panic!("{err}");
    };
    assert!(reason.contains("<style>"), "{reason}");
    assert!(!reason.contains("Formulas are not drawn"), "{reason}");

    let mut renderer = Renderer::new(&config(5000));
    let out = renderer
        .render(
            "@startlatex\n<style>\n</style>\nx\n@endlatex\n",
            Language::PlantUml,
        )
        .unwrap();
    assert_eq!(out.engine, Engine::JavaPlantUml);
}

#[test]
fn a_signal_flow_is_not_sent_to_java() {
    let mut renderer = Renderer::new(&config(5000));
    let err = renderer
        .render("@startflow\nnope\n@endflow\n", Language::PlantUml)
        .unwrap_err();
    let RenderError::Unavailable {
        line: Some(1),
        reason,
    } = err
    else {
        panic!("{err:?}");
    };
    assert!(reason.contains("not supported"), "{reason}");
    assert!(!reason.contains("plantuml"), "{reason}");

    let out = renderer
        .render(
            "@startflow\nin a\nout b\na -> b\n@endflow\n",
            Language::PlantUml,
        )
        .unwrap();
    assert_eq!(out.engine, Engine::NativePlantUml);
}

#[test]
fn syntax_errors_point_at_the_source_line() {
    let mut java = JavaPlantUml::find(&config(5000)).unwrap();
    // PlantUML reports line 2 of the block it got, counting `@startuml` as
    // 0; the block starts at source line 2.
    let source = "' comment\n\n@startuml\nA -> B\nSYNTAXERROR\n@enduml\n";
    let err = java.render(source).unwrap_err();
    assert_eq!(
        err,
        RenderError::Diagram {
            engine: Engine::JavaPlantUml,
            line: Some(4),
            message: "Syntax Error?".into()
        }
    );
    // Without `@startuml` the block gets one in front of the source.
    let err = java.render("A -> B\nSYNTAXERROR\n").unwrap_err();
    assert!(
        matches!(err, RenderError::Diagram { line: Some(1), .. }),
        "{err:?}"
    );
}

#[test]
fn restarts_after_a_crash() {
    let mut java = JavaPlantUml::find(&config(5000)).unwrap();
    assert!(java.render("@startuml\nA -> B\n@enduml").is_ok());
    assert!(matches!(
        java.render("@startuml\nCRASH\n@enduml"),
        Err(RenderError::Engine { .. })
    ));
    // A new process: counting starts again.
    assert_eq!(
        text_of(&java.render("@startuml\nA -> B\n@enduml").unwrap()),
        "diagram 1"
    );
}

#[test]
fn stops_a_hanging_process() {
    let mut java = JavaPlantUml::find(&config(300)).unwrap();
    let err = java.render("@startuml\nHANG\n@enduml").unwrap_err();
    assert!(err.to_string().contains("took longer"), "{err}");
    assert_eq!(
        text_of(&java.render("@startuml\nA -> B\n@enduml").unwrap()),
        "diagram 1"
    );
}

#[test]
fn without_java_plantuml_is_unavailable() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    // A kind the engine does not draw at all: the kind, not a line.
    let err = renderer.render(SALT, Language::PlantUml).unwrap_err();
    assert!(matches!(err, RenderError::Unavailable { line: None, .. }));
    assert!(
        err.to_string().starts_with("Salt UIs are not drawn"),
        "{err}"
    );
    // A timing or Gantt line the engine declines keeps that line.
    let timing = "@startuml\nbinary \"E\" as E\n@0\nE is maybe\n@enduml\n";
    let err = renderer.render(timing, Language::PlantUml).unwrap_err();
    assert!(
        matches!(err, RenderError::Unavailable { line: Some(3), .. }),
        "{err}"
    );
    assert!(err.to_string().contains("binary"), "{err}");
    let gantt = "@startgantt\nsaturday are closed\n@endgantt\n";
    let err = renderer.render(gantt, Language::PlantUml).unwrap_err();
    assert!(
        matches!(err, RenderError::Unavailable { line: Some(1), .. }),
        "{err}"
    );
    assert!(err.to_string().contains("closed"), "{err}");
    // An unknown block is not one of the whole kinds, so the engine's line stays.
    let other = "@startfoo\n@endfoo\n";
    let err = renderer.render(other, Language::PlantUml).unwrap_err();
    assert!(
        matches!(err, RenderError::Unavailable { line: Some(0), .. }),
        "{err}"
    );
    assert!(err.to_string().contains("@startfoo"), "{err}");
    // A state diagram the engine knows, but one line of it declines: that line.
    let concurrent = "@startuml\n[*] --> A\n--\n@enduml\n";
    let err = renderer.render(concurrent, Language::PlantUml).unwrap_err();
    assert!(
        matches!(err, RenderError::Unavailable { line: Some(2), .. }),
        "{err}"
    );
    assert!(err.to_string().contains("concurrent"), "{err}");
    assert!(err.to_string().contains("plantuml"));
    let component = "@startuml\nskinparam component {\n  BackgroundColor<<A>> Pink\n}\npackage P {\n  [Engine]\n}\n@enduml\n";
    let err = renderer.render(component, Language::PlantUml).unwrap_err();
    assert!(
        matches!(err, RenderError::Unavailable { line: Some(2), .. }),
        "{err}"
    );
    assert!(err.to_string().contains("skinparam"), "{err}");
    // A kind it draws, with a line it does not: that line.
    let sequence = "@startuml\nA -> B\nnewpage\n@enduml\n";
    let err = renderer.render(sequence, Language::PlantUml).unwrap_err();
    assert!(
        matches!(err, RenderError::Unavailable { line: Some(2), .. }),
        "{err}"
    );
}
