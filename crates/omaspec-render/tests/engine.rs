//! A render names the engine that drew the diagram.

use omaspec_core::Language;
use omaspec_render::{Config, Engine, Renderer};

#[test]
fn a_mermaid_render_names_the_engine_that_drew_it() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    // A flowchart is Omaspec's to draw; a quadrant chart is left to mmdr.
    let flow = renderer
        .render("flowchart LR\n  A --> B\n", Language::Mermaid)
        .unwrap();
    assert_eq!(flow.engine, Engine::NativeMermaid);
    assert_eq!(flow.engine.name(), "Omaspec Mermaid");
    let quadrant = "quadrantChart\n  title Reach\n  x-axis Low --> High\n  y-axis Low --> High\n  A: [0.3, 0.6]\n";
    let quadrant = renderer.render(quadrant, Language::Mermaid).unwrap();
    assert_eq!(quadrant.engine, Engine::Mermaid);
    assert_eq!(quadrant.engine.name(), "mmdr");
}
