//! A Markdown document with fenced blocks of any kind: the diagrams found
//! lie in fences, and drawing them all ends without a panic.
#![no_main]

use libfuzzer_sys::fuzz_target;
use omaspec_core::markdown;
use omaspec_render::{Config, Renderer};

fuzz_target!(|text: &str| {
    let rows = text.split('\n').count();
    for block in markdown::blocks(text) {
        assert!(block.lines.start < block.lines.end && block.lines.end <= rows);
        for row in block.lines.clone() {
            assert!(markdown::in_fence(text, row), "row {row} of {block:?}");
        }
    }
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let _ = renderer.render_document(text, None);
});
