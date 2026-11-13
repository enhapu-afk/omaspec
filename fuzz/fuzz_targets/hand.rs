//! The block of lines and boxes by hand (docs/adr/0016, 0021, 0028):
//! every entry that reads writes back as it was read, and a box held by
//! any name, written into a diagram, reads back as written.
#![no_main]

use libfuzzer_sys::fuzz_target;
use omaspec_core::Language;
use omaspec_uml::hand::{self, Hand, Hold, Word};

fuzz_target!(|code: &str| {
    let lines = code.split('\n').count();
    // Whatever reads, written again, reads the same.
    for entry in hand::read(code).map(|b| b.entries).unwrap_or_default() {
        assert!(entry.line < lines);
        let Ok(read) = entry.read else { continue };
        let written = format!("' lines by hand (Omaspec)\n' {read}\n");
        let again = hand::read(&written).expect("a block").entries;
        assert_eq!(again.len(), 1, "{written:?}");
        assert_eq!(again[0].read.as_ref(), Ok(&read), "{written:?}");
    }
    // A box of any name, held by any word, kept by the writer.
    let mut parts = code.splitn(3, '\n');
    let (Some(name), Some(beside)) = (parts.next(), parts.next()) else {
        return;
    };
    if name.is_empty() || beside.is_empty() || name.contains('"') || beside.contains('"') {
        return;
    }
    let word = Word::ALL[code.len() % Word::ALL.len()];
    let hold = Hold {
        name: name.to_owned(),
        beside: beside.to_owned(),
        word,
    };
    let diagram = "@startuml\nA --> B\n@enduml\n";
    if let Some((range, text)) = omaspec_core::hand::hold(diagram, Language::PlantUml, name, Some(&hold))
    {
        let held = format!("{}{text}{}", &diagram[..range.start], &diagram[range.end..]);
        let kept = hand::read(&held).expect("a block").entries;
        assert!(
            kept.iter().any(|e| e.read.as_ref() == Ok(&Hand::Hold(hold.clone()))),
            "{held:?}"
        );
    }
});
