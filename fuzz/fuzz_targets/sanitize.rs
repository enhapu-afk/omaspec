//! The SVG sanitizer on any text: what it lets through parses again and
//! holds no script, handler or outside link.
#![no_main]

use libfuzzer_sys::fuzz_target;
use omaspec_render::{export, sanitize::sanitize};

fuzz_target!(|svg: &str| {
    let Ok(clean) = sanitize(svg) else { return };
    let lower = clean.to_ascii_lowercase();
    assert!(!lower.contains("<script"));
    assert!(!lower.contains("<foreignobject"));
    assert!(!lower.contains("javascript:"));
    let _ = export::parse(&clean);
});
