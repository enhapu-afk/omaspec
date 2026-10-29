//! Lint, outline, formatting and the rest of the text functions on any
//! text: no panic, lines inside the text, formatting settles.
#![no_main]

use libfuzzer_sys::fuzz_target;
use omaspec_core::{Language, classify, detect, format, lint, locate, outline};

fuzz_target!(|code: &str| {
    let lines = code.split('\n').count();
    let detected = detect(code, None);
    for language in [Language::PlantUml, Language::Mermaid, detected] {
        let _ = classify(code, language);
        for finding in lint::lint(code, language) {
            assert!(finding.line < lines);
        }
        for entry in outline::outline(code, language) {
            assert!(entry.line < lines);
        }
        let once = format::format(code, language);
        assert_eq!(format::format(&once, language), once);
    }
    if let Some(word) = code.split_whitespace().next()
        && let Some(line) = locate::line_of(code, word, 0)
    {
        assert!(line < lines);
    }
});
