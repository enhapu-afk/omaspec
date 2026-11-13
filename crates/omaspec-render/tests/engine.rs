//! A render names the engine that drew the diagram.

use omaspec_core::Language;
use omaspec_render::{Config, Engine, Renderer};

#[test]
fn a_mermaid_render_names_the_engine_that_drew_it() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    // A flowchart is Omaspec's to draw; ZenUML is left to mmdr.
    let flow = renderer
        .render("flowchart LR\n  A --> B\n", Language::Mermaid)
        .unwrap();
    assert_eq!(flow.engine, Engine::NativeMermaid);
    assert_eq!(flow.engine.name(), "Omaspec Mermaid");
    let zen = renderer
        .render("zenuml\n  A.method()\n", Language::Mermaid)
        .unwrap();
    assert_eq!(zen.engine, Engine::Mermaid);
    assert_eq!(zen.engine.name(), "mmdr");
}
