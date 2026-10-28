//! The behavior model on any text, in both languages: the lint with the
//! specification checks, a play through the machine, the conversion and
//! the diff. No panic, and every finding names a line inside the text.
#![no_main]

use libfuzzer_sys::fuzz_target;
use omagram_core::Language;
use omagram_model::{Machine, Run, convert, diff, findings};

fuzz_target!(|code: &str| {
    let lines = code.split('\n').count();
    for (lang, other) in [
        (Language::PlantUml, Language::Mermaid),
        (Language::Mermaid, Language::PlantUml),
    ] {
        for finding in findings(code, lang) {
            assert!(finding.line < lines);
        }
        if let Some(machine) = Machine::read(code, lang)
            && let Some((mut run, _)) = Run::start(&machine)
        {
            for step in 0..32 {
                let options = run.options(&machine);
                let Some(&next) = options.get(step % options.len().max(1)) else {
                    break;
                };
                run.take(&machine, next);
            }
        }
        let _ = convert(code, lang, other);
        let _ = diff(code, lang, code, lang, &[]);
    }
});
