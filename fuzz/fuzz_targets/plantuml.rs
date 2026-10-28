//! Omagram's own PlantUML engine on any text: no panic, a declined
//! diagram names a line inside the text, what it draws passes the sanitizer.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|code: &str| {
    let lines = code.split('\n').count();
    if let Err(declined) = omagram_uml::parse(code) {
        assert!(declined.line.is_none_or(|l| l < lines));
    }
    if let Ok(svg) = omagram_uml::render(code) {
        assert!(omagram_render::sanitize::sanitize(&svg).is_ok());
    }
});
