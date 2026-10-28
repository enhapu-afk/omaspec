//! Every skin pack draws, changes the picture and leaves lint quiet.
//! PlantUML packs draw in Omagram's own engine.

use omagram_core::skin::{self, Colors, Pack};
use omagram_core::{Language, lint};
use omagram_render::{Config, Engine, Renderer};

const COLORS: Colors = Colors {
    background: "#121212",
    surface: "#1e1e1e",
    text: "#bebebe",
    accent: "#e68e0d",
    muted: "#555555",
    note: "#2a2a2a",
};

#[test]
fn mermaid_packs_draw() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    for code in [
        "flowchart LR\n  A[Edit] --> B{Saved?}\n  B -->|yes| C[Render]\n",
        "sequenceDiagram\n  Alice->>Bob: hi\n  Note right of Bob: thinks\n  Bob-->>Alice: hello\n",
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : job\n  note right of Idle : wait\n",
    ] {
        let bare = renderer.render(code, Language::Mermaid).unwrap().svg;
        for pack in Pack::ALL {
            let (range, text) = skin::edit(code, Language::Mermaid, Some(pack), &COLORS).unwrap();
            let skinned = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
            assert!(
                lint::lint(&skinned, Language::Mermaid).is_empty(),
                "{pack:?}"
            );
            let svg = renderer.render(&skinned, Language::Mermaid).unwrap().svg;
            assert_ne!(svg, bare, "{pack:?} changes nothing");
            if pack == Pack::Omarchy {
                let svg = svg.to_ascii_lowercase();
                assert!(svg.contains("#121212") && svg.contains("#e68e0d"), "{svg}");
            }
        }
    }
}

#[test]
fn plantuml_packs_draw_natively() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let code = "@startuml\nactor User\ndatabase DB\nUser -> DB ++ : query\nalt found\n  DB --> User -- : rows\nend\nnote over DB : indexed\n@enduml\n";
    let bare = renderer.render(code, Language::PlantUml).unwrap();
    assert_eq!(bare.engine, Engine::NativePlantUml);
    for pack in Pack::ALL {
        let (range, text) = skin::edit(code, Language::PlantUml, Some(pack), &COLORS).unwrap();
        let skinned = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        assert!(
            lint::lint(&skinned, Language::PlantUml).is_empty(),
            "{pack:?}"
        );
        let svg = renderer.render(&skinned, Language::PlantUml).unwrap().svg;
        assert_ne!(svg, bare.svg, "{pack:?} changes nothing");
        if pack == Pack::Omarchy {
            // The light defaults are all replaced.
            for light in ["#E2E2F0", "#FEFFDD", "#EEEEEE", "#FFFFFF"] {
                assert!(!svg.contains(light), "{light} left in {svg}");
            }
            assert!(svg.contains("#121212") && svg.contains("#E68E0D"), "{svg}");
        }
    }
}

#[test]
fn plantuml_packs_draw_trees_natively() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    for code in [
        "@startmindmap\n* Release\n**[#Orange] Build\n*** Test\nleft side\n** Plan\n@endmindmap\n",
        "@startwbs\n* Release\n**[#Orange] Build\n*** Test\n**< Plan\n@endwbs\n",
    ] {
        let bare = renderer.render(code, Language::PlantUml).unwrap();
        assert_eq!(bare.engine, Engine::NativePlantUml);
        for pack in Pack::ALL {
            let (range, text) = skin::edit(code, Language::PlantUml, Some(pack), &COLORS).unwrap();
            let skinned = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
            assert!(
                lint::lint(&skinned, Language::PlantUml).is_empty(),
                "{pack:?}"
            );
            let svg = renderer.render(&skinned, Language::PlantUml).unwrap().svg;
            assert_ne!(svg, bare.svg, "{pack:?} changes nothing");
            if pack == Pack::Omarchy {
                // The light boxes and dark lines are replaced.
                for light in ["#F1F1F1", "#181818", "#000000"] {
                    assert!(!svg.contains(light), "{light} left in {svg}");
                }
                assert!(svg.contains("#121212") && svg.contains("#E68E0D"), "{svg}");
            }
        }
    }
}

#[test]
fn plantuml_packs_draw_activity_natively() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let code = "@startuml\n|Dev|\nstart\npartition Local {\n:Build;\nnote right: fast\n}\n|CI|\nif (green?) then (yes)\n  fork\n    :Publish;\n  fork again\n    :Tag;\n  end fork\nelse (no)\n  :Fix;\nendif\nstop\n@enduml\n";
    let bare = renderer.render(code, Language::PlantUml).unwrap();
    assert_eq!(bare.engine, Engine::NativePlantUml);
    for pack in Pack::ALL {
        let (range, text) = skin::edit(code, Language::PlantUml, Some(pack), &COLORS).unwrap();
        let skinned = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        assert!(
            lint::lint(&skinned, Language::PlantUml).is_empty(),
            "{pack:?}"
        );
        let svg = renderer.render(&skinned, Language::PlantUml).unwrap().svg;
        assert_ne!(svg, bare.svg, "{pack:?} changes nothing");
        if pack == Pack::Omarchy {
            // Light boxes and notes, dark lines and circles are replaced.
            for default in ["#F1F1F1", "#FEFFDD", "#181818", "#000000", "#222222"] {
                assert!(!svg.contains(default), "{default} left in {svg}");
            }
            assert!(svg.contains("#121212") && svg.contains("#E68E0D"), "{svg}");
        }
    }
}

#[test]
fn plantuml_packs_draw_state_natively() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let code = "@startuml\nstate Playing #AliceBlue\nPlaying : the track\n[*] --> Playing\nPlaying --> Playing : loop\nnote right of Playing : here\nPlaying --> [*]\n@enduml\n";
    let bare = renderer.render(code, Language::PlantUml).unwrap();
    assert_eq!(bare.engine, Engine::NativePlantUml);
    for pack in Pack::ALL {
        let (range, text) = skin::edit(code, Language::PlantUml, Some(pack), &COLORS).unwrap();
        let skinned = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        assert!(
            lint::lint(&skinned, Language::PlantUml).is_empty(),
            "{pack:?}"
        );
        let svg = renderer.render(&skinned, Language::PlantUml).unwrap().svg;
        assert_ne!(svg, bare.svg, "{pack:?} changes nothing");
        if pack == Pack::Omarchy {
            for light in ["#F1F1F1", "#FEFFDD", "#181818", "#000000"] {
                assert!(!svg.contains(light), "{light} left in {svg}");
            }
            assert!(svg.contains("#121212") && svg.contains("#E68E0D"), "{svg}");
        }
    }
}

#[test]
fn plantuml_packs_draw_structure_natively() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let code = "@startuml\npackage Shop {\n  class Product\n  interface Sellable\n  object order\n}\ncomponent Web\ncloud Net\nfolder Files\nframe Box\nartifact App\nrectangle Area\nnote right of Product : stocked\nProduct --> Web : uses\nProduct o-- Sellable\n@enduml\n";
    let bare = renderer.render(code, Language::PlantUml).unwrap();
    assert_eq!(bare.engine, Engine::NativePlantUml);
    for pack in Pack::ALL {
        let (range, text) = skin::edit(code, Language::PlantUml, Some(pack), &COLORS).unwrap();
        let skinned = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        assert!(
            lint::lint(&skinned, Language::PlantUml).is_empty(),
            "{pack:?}"
        );
        let svg = renderer.render(&skinned, Language::PlantUml).unwrap().svg;
        assert_ne!(svg, bare.svg, "{pack:?} changes nothing");
        if pack == Pack::Omarchy {
            // Every box and the empty diamond follow the theme.
            for light in [
                "#FEFECE", "#FEFFDD", "#EEEEEE", "#FFFFFF", "#181818", "#000000",
            ] {
                assert!(!svg.contains(light), "{light} left in {svg}");
            }
            assert!(svg.contains("#121212") && svg.contains("#E68E0D"), "{svg}");
        }
    }
}

#[test]
fn plantuml_packs_draw_timing_natively() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let code = "@startuml\nrobust \"Browser\" as B\nconcise \"Web\" as W\nconcise \"User\" as U\nbinary \"Enable\" as E\nclock \"Clock\" as C with period 50\nanalog \"Load\" as L\nrectangle \"Job\" as J\nB is Idle\nW is Ready\n@0\nW is Idle #pink\nU is Idle\nB is Busy\nE is high\nL is 3\nJ is Run\n@100\nU is Waiting\nE is low\nL is 5\nU -> W : URL\nhighlight 20 to 60 : busy\n@enduml\n";
    let bare = renderer.render(code, Language::PlantUml).unwrap();
    assert_eq!(bare.engine, Engine::NativePlantUml);
    for pack in Pack::ALL {
        let (range, text) = skin::edit(code, Language::PlantUml, Some(pack), &COLORS).unwrap();
        let skinned = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        assert!(
            lint::lint(&skinned, Language::PlantUml).is_empty(),
            "{pack:?}"
        );
        let svg = renderer.render(&skinned, Language::PlantUml).unwrap().svg;
        assert_ne!(svg, bare.svg, "{pack:?} changes nothing");
        if pack == Pack::Omarchy {
            // Boxes, lines and the highlight follow the dark background.
            for light in ["#F1F1F1", "#FFF3C4", "#FEFFDD", "#181818", "#000000"] {
                assert!(!svg.contains(light), "{light} left in {svg}");
            }
            assert!(svg.contains("#121212") && svg.contains("#E68E0D"), "{svg}");
        }
    }
}

#[test]
fn plantuml_packs_draw_gantt_natively() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let code = "@startgantt\n-- Build --\n[Design] requires 5 days\n[Test] requires 3 days\n[Design] -> [Test]\n[Design] is colored in Fuchsia\n[Done] happens at [Test]'s end\n@endgantt\n";
    let bare = renderer.render(code, Language::PlantUml).unwrap();
    assert_eq!(bare.engine, Engine::NativePlantUml);
    for pack in Pack::ALL {
        let (range, text) = skin::edit(code, Language::PlantUml, Some(pack), &COLORS).unwrap();
        let skinned = format!("{}{text}{}", &code[..range.start], &code[range.end..]);
        assert!(
            lint::lint(&skinned, Language::PlantUml).is_empty(),
            "{pack:?}"
        );
        let svg = renderer.render(&skinned, Language::PlantUml).unwrap().svg;
        assert_ne!(svg, bare.svg, "{pack:?} changes nothing");
        if pack == Pack::Omarchy {
            // Sections, the grid and the bar borders follow the background.
            for light in ["#EEEEEE", "#E6E6E6", "#181818", "#000000"] {
                assert!(!svg.contains(light), "{light} left in {svg}");
            }
            assert!(svg.contains("#121212") && svg.contains("#E68E0D"), "{svg}");
        }
    }
}
