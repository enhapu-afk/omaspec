//! mmdr (and Omagram's repairs) on any Mermaid text: errors are fine,
//! a panic reaching the caller is not.
#![no_main]

use libfuzzer_sys::fuzz_target;
use omagram_core::Language;
use omagram_render::{Config, Renderer};

fuzz_target!(|code: &str| {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let _ = renderer.render(code, Language::Mermaid);
});
