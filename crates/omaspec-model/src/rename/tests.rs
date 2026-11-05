use omaspec_core::Language;

use super::*;
use crate::refs::FileKind;
use crate::{Spec, Specs};

#[test]
fn a_new_name_is_spelled_as_the_old_one_was() {
    assert_eq!(spell("Door", "front gate", false), "FrontGate");
    assert_eq!(spell("doorControl", "Front gate", false), "frontGate");
    assert_eq!(spell("door", "Front gate", false), "front_gate");
    assert_eq!(spell("door_control", "Front gate", false), "front_gate");
    assert_eq!(spell("Door_Control", "front gate", false), "Front_Gate");
    assert_eq!(spell("door-bell", "Front gate", false), "front-gate");
    assert_eq!(spell("DOOR", "Front gate", false), "FRONT_GATE");
    assert_eq!(spell("Door control", "Front gate", false), "Front gate");
    assert_eq!(spell("Door", "Front gate", true), "Front gate");
    assert_eq!(spell("open", "Swing", false), "swing");
}

const MAP: &str = "@startwbs\n* House\n** Door\n** Lamp\n@endwbs\n";
const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nnote left of Open\n  Open stays open.\nend note\n@enduml\n";
const LAMP: &str =
    "@startuml\ntitle Lamp\n[*] --> Idle\nIdle --> Door : lead\nDoor --> Idle : open\n@enduml\n";
const RUN: &str = "@startuml\ntitle Run\nparticipant User\nparticipant Door\nUser -> Door : open\nUser -> Door : close()\nUser -> Lamp : open\nnote over Door : the Door opens\n@enduml\n";
const NOTES: &str = "# Notes\n\n```plantuml\nUser -> Door : open\n```\n";
const TRACE: &str = "{\"object\":\"door\",\"event\":\"open\",\"state\":\"Open\"}\n";
const UNTITLED: &str = "stateDiagram-v2\n[*] --> Off\n";

fn files() -> Vec<File<'static>> {
    let diagram = |path, text| File {
        path,
        text,
        kind: FileKind::Diagram(Language::PlantUml),
    };
    vec![
        diagram("house.puml", MAP),
        diagram("door.puml", DOOR),
        diagram("lamp.puml", LAMP),
        diagram("run.puml", RUN),
        File {
            path: "fan.mmd",
            text: UNTITLED,
            kind: FileKind::Diagram(Language::Mermaid),
        },
        File {
            path: "notes.md",
            text: NOTES,
            kind: FileKind::Document,
        },
        File {
            path: "today.jsonl",
            text: TRACE,
            kind: FileKind::Trace,
        },
    ]
}

fn specs(files: &[File]) -> Specs {
    files
        .iter()
        .filter_map(|f| match f.kind {
            FileKind::Diagram(language) => Spec::read(f.text, language, f.path),
            _ => None,
        })
        .collect()
}

fn plan(old: &str, new: &str, machine: Option<&str>, thing: Option<Thing>) -> Result<Plan, String> {
    let files = files();
    let specs = specs(&files);
    rename(old, new, machine, thing, &files, &specs)
}

/// Each file the plan changes, as it reads afterwards.
fn renamed(plan: &Plan) -> Vec<(String, String)> {
    let files = files();
    plan.files()
        .into_iter()
        .map(|file| {
            let text = files.iter().find(|f| f.path == file).unwrap().text;
            (file.to_owned(), plan.apply(file, text))
        })
        .collect()
}

#[test]
fn a_diagram_is_renamed_where_it_is_named_shown_played_and_opened() {
    let plan = plan("Door", "Front gate", None, None).unwrap();
    assert_eq!(plan.what, "the diagram Door (door.puml)");
    let after = renamed(&plan);
    let changed: Vec<&str> = after.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(
        changed,
        [
            "door.puml",
            "house.puml",
            "lamp.puml",
            "notes.md",
            "run.puml"
        ]
    );
    let text = |file: &str| after.iter().find(|(f, _)| f == file).unwrap().1.clone();
    assert!(text("door.puml").contains("title Front gate\n"));
    assert!(text("house.puml").contains("** Front gate\n"));
    assert!(text("lamp.puml").contains("Idle --> FrontGate : lead\nFrontGate --> Idle : open\n"));
    assert_eq!(
        text("run.puml"),
        "@startuml\ntitle Run\nparticipant User\nparticipant FrontGate\nUser -> FrontGate : open\nUser -> FrontGate : close()\nUser -> Lamp : open\nnote over FrontGate : the Door opens\n@enduml\n"
    );
    assert!(text("notes.md").contains("User -> FrontGate : open\n"));
    // The trace is a record; the file name stays, and the rename says so.
    assert_eq!(plan.traces.len(), 1);
    assert_eq!(plan.traces[0].file, "today.jsonl");
    assert_eq!(
        plan.notes,
        [
            "door.puml still goes by door, its file name; `git mv door.puml front_gate.puml` drops it"
        ]
    );

    // What uses the new name now is what used the old one.
    let renamed_files: Vec<(String, String)> = files()
        .iter()
        .map(|f| {
            let text = after
                .iter()
                .find(|(file, _)| file == f.path)
                .map_or(f.text.to_owned(), |(_, t)| t.clone());
            (f.path.to_owned(), text)
        })
        .collect();
    let new_files: Vec<File> = files()
        .iter()
        .zip(&renamed_files)
        .map(|(f, (_, text))| File { text, ..*f })
        .collect();
    let new_specs = specs(&new_files);
    let uses: Vec<(String, RefKind)> = refs("Front gate", None, &new_files, &new_specs)
        .unwrap()
        .into_iter()
        .map(|r| (format!("{}:{}", r.file, r.line + 1), r.kind))
        .collect();
    assert_eq!(
        uses,
        [
            ("door.puml:2".to_owned(), RefKind::Diagram),
            ("house.puml:3".to_owned(), RefKind::Box),
            ("lamp.puml:4".to_owned(), RefKind::Link),
            ("lamp.puml:5".to_owned(), RefKind::Link),
            ("notes.md:4".to_owned(), RefKind::Participant),
            ("run.puml:4".to_owned(), RefKind::Participant),
            // The machine still goes by door, its file name, as the trace
            // calls it.
            ("today.jsonl:1".to_owned(), RefKind::Trace),
        ]
    );
}

#[test]
fn a_state_is_renamed_left_of_labels_and_an_event_only_in_them() {
    // Open is a state and an event of the door, and an event of the lamp.
    let refused = plan("Open", "Ajar", None, None).unwrap_err();
    assert!(refused.contains("the state Open of door.puml"), "{refused}");
    assert!(refused.contains("the event Open of lamp.puml"), "{refused}");

    let state = plan("Open", "Ajar", Some("Door"), Some(Thing::State)).unwrap();
    assert_eq!(
        renamed(&state),
        [(
            "door.puml".to_owned(),
            "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Ajar : open\nAjar --> Closed : close\nnote left of Ajar\n  Open stays open.\nend note\n@enduml\n".to_owned()
        )]
    );

    let event = plan("open", "Switch on", Some("Lamp"), Some(Thing::Event)).unwrap();
    let after = renamed(&event);
    assert_eq!(after.len(), 2);
    assert!(after[0].1.contains("Door --> Idle : switch_on\n"));
    assert!(
        after[1]
            .1
            .contains("User -> Door : open\nUser -> Door : close()\nUser -> Lamp : switch_on\n")
    );
}

#[test]
fn a_rename_that_would_tie_two_things_or_has_no_written_name_is_refused() {
    let refused = |old, new, machine, thing| plan(old, new, machine, thing).unwrap_err();
    assert_eq!(
        refused("Door", "lamp", None, None),
        "lamp is already the name of lamp.puml"
    );
    assert_eq!(
        refused("Open", "Closed", Some("Door"), Some(Thing::State)),
        "door.puml has a state Closed already"
    );
    assert_eq!(
        refused("open", "close", Some("Door"), Some(Thing::Event)),
        "door.puml has an event close already"
    );
    assert!(refused("fan", "Ceiling fan", None, None).contains("give it a title first"));
    assert!(refused("Door", "a: b", None, None).contains("cannot be a name"));
    assert!(refused("Door", "DOOR", None, None).contains("already"));
    assert_eq!(
        refused("Window", "Pane", None, None),
        "Nothing in the project is called Window"
    );
}

#[test]
fn mermaid_states_events_and_titles_are_renamed_alike() {
    let door = "---\ntitle: Door\n---\nstateDiagram-v2\n[*] --> Closed\nClosed --> Open: open\nOpen: wide open\nOpen --> Closed: close\n";
    let visit = "sequenceDiagram\nparticipant User\nparticipant Door\nUser->>Door: open\nDoor-->>User: open\n";
    let files = [
        File {
            path: "door.mmd",
            text: door,
            kind: FileKind::Diagram(Language::Mermaid),
        },
        File {
            path: "visit.mmd",
            text: visit,
            kind: FileKind::Diagram(Language::Mermaid),
        },
    ];
    let specs = specs(&files);
    let state = rename("Open", "Ajar", None, Some(Thing::State), &files, &specs).unwrap();
    assert_eq!(
        state.apply("door.mmd", door),
        "---\ntitle: Door\n---\nstateDiagram-v2\n[*] --> Closed\nClosed --> Ajar: open\nAjar: wide open\nAjar --> Closed: close\n"
    );
    let event = rename("open", "swing", None, Some(Thing::Event), &files, &specs).unwrap();
    assert_eq!(event.files(), ["door.mmd", "visit.mmd"]);
    assert!(
        event
            .apply("door.mmd", door)
            .contains("Closed --> Open: swing\n")
    );
    // A reply to the user, who has no machine, is no event of the door's.
    assert_eq!(
        event.apply("visit.mmd", visit),
        "sequenceDiagram\nparticipant User\nparticipant Door\nUser->>Door: swing\nDoor-->>User: open\n"
    );
    let diagram = rename("Door", "Gate", None, Some(Thing::Diagram), &files, &specs).unwrap();
    assert!(
        diagram
            .apply("door.mmd", door)
            .starts_with("---\ntitle: Gate\n---\n")
    );
    assert_eq!(
        diagram.apply("visit.mmd", visit),
        "sequenceDiagram\nparticipant User\nparticipant Gate\nUser->>Gate: open\nGate-->>User: open\n"
    );
}
