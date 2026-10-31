//! A block of lines by hand (docs/adr/0016) is comments to every engine:
//! entries for lines that are not there change no picture and no lint.

use std::path::Path;

use omaspec_core::hand;
use omaspec_core::{Language, lint};
use omaspec_render::{Config, Renderer};
use omaspec_uml::hand::{Key, Line, Via};

#[test]
fn entries_for_no_line_change_no_gallery_picture() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let nowhere = Line {
        key: Key {
            from: "Nowhere".into(),
            to: "Nobody".into(),
            nth: 2,
        },
        vias: vec![Via {
            along: 0.5,
            aside: -60.0,
        }],
    };
    let gallery = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery");
    let mut seen = 0;
    for entry in std::fs::read_dir(&gallery).unwrap() {
        let path = entry.unwrap().path();
        let language = match path.extension().and_then(|e| e.to_str()) {
            Some("puml") => Language::PlantUml,
            Some("mmd") => Language::Mermaid,
            _ => continue,
        };
        let code = std::fs::read_to_string(&path).unwrap();
        let (range, text) = hand::edit(&code, language, &nowhere).unwrap();
        let with = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        assert!(with.contains("Nowhere -> Nobody #2 via 0.5 -60"), "{with}");

        let name = path.display();
        let bare = renderer.render(&code, language).unwrap();
        let drawn = renderer.render(&with, language).unwrap();
        assert_eq!(drawn.engine, bare.engine, "{name}");
        assert!(
            drawn.svg == bare.svg,
            "{name}: the block changed the picture"
        );
        assert_eq!(
            lint::lint(&with, language),
            lint::lint(&code, language),
            "{name}"
        );
        seen += 1;
    }
    assert!(seen >= 20, "only {seen} examples");
}
