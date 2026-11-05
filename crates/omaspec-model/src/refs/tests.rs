use super::*;

#[test]
fn a_name_is_found_however_it_is_spelled() {
    fn found<'a>(line: &'a str, name: &str) -> Vec<&'a str> {
        spans(line, &key(name))
            .into_iter()
            .map(|r| &line[r])
            .collect()
    }
    assert_eq!(
        found("participant DoorControl", "Door control"),
        ["DoorControl"]
    );
    assert_eq!(
        found("title Door control", "door_control"),
        ["Door control"]
    );
    assert_eq!(
        found("X -> door_control : open", "DoorControl"),
        ["door_control"]
    );
    assert_eq!(found("Closed --> Door : open", "door"), ["Door"]);
    assert_eq!(found("Closed-->Door", "door"), ["Door"]);
    assert_eq!(found("Door --> Door", "door"), ["Door", "Door"]);
    // Joined words are another name; so is a longer word.
    assert!(found("front_door --> Doors", "door").is_empty());
    assert!(found("door-bell --> x", "door").is_empty());
    assert_eq!(found("Tür --> Tor", "TÜR"), ["Tür"]);
}

const MAP: &str = "@startwbs\n* House\n** Door\n** Lamp\n@endwbs\n";
const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
const LAMP: &str =
    "@startuml\ntitle Lamp\n[*] --> Idle\nIdle --> Door : lead\nDoor --> Idle : open\n@enduml\n";
const RUN: &str = "@startuml\ntitle Run\nparticipant User\nparticipant Door\nUser -> Door : open\nUser -> Door : close()\nUser -> Lamp : open\n@enduml\n";
const NOTES: &str = "# Notes\n\n```plantuml\nUser -> Door : open\n```\n";
const TRACE: &str = "{\"object\":\"door\",\"event\":\"open\",\"state\":\"Open\"}\n{\"object\":\"hall lamp\",\"machine\":\"Lamp\",\"event\":\"open\"}\n";

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
            path: "notes.md",
            text: NOTES,
            kind: FileKind::Document,
        },
        File {
            path: "runs/today.jsonl",
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

/// `(file:line, kind, machine)`, lines one-based.
fn listed(name: &str, machine: Option<&str>) -> Vec<(String, &'static str, Option<String>)> {
    let files = files();
    let specs = specs(&files);
    refs(name, machine, &files, &specs)
        .unwrap()
        .into_iter()
        .map(|r| {
            (
                format!("{}:{}", r.file, r.line + 1),
                r.kind.name(),
                r.machine,
            )
        })
        .collect()
}

fn at(
    place: &str,
    kind: &'static str,
    machine: Option<&str>,
) -> (String, &'static str, Option<String>) {
    (place.to_owned(), kind, machine.map(str::to_owned))
}

#[test]
fn a_diagram_is_used_by_boxes_participants_links_and_traces() {
    assert_eq!(
        listed("door", None),
        [
            at("door.puml:2", "diagram", None),
            at("house.puml:3", "box", None),
            at("lamp.puml:4", "link", Some("lamp.puml")),
            at("lamp.puml:5", "link", Some("lamp.puml")),
            at("notes.md:4", "participant", None),
            at("run.puml:4", "participant", None),
            at("runs/today.jsonl:1", "trace", Some("door.puml")),
        ]
    );
}

#[test]
fn a_state_and_an_event_of_one_spelling_are_both_listed() {
    // Open is a state of the door and an event of the door and the lamp.
    assert_eq!(
        listed("Open", None),
        [
            at("door.puml:4", "state", Some("door.puml")),
            at("door.puml:4", "event", Some("door.puml")),
            at("door.puml:5", "state", Some("door.puml")),
            at("lamp.puml:5", "event", Some("lamp.puml")),
            at("notes.md:4", "message", Some("door.puml")),
            at("run.puml:5", "message", Some("door.puml")),
            at("run.puml:7", "message", Some("lamp.puml")),
            at("runs/today.jsonl:1", "trace", Some("door.puml")),
            at("runs/today.jsonl:2", "trace", Some("lamp.puml")),
        ]
    );
    // One machine's only.
    assert_eq!(
        listed("open", Some("Lamp")),
        [
            at("lamp.puml:5", "event", Some("lamp.puml")),
            at("run.puml:7", "message", Some("lamp.puml")),
            at("runs/today.jsonl:2", "trace", Some("lamp.puml")),
        ]
    );
    // `close()` is the event close.
    assert!(listed("close", None).contains(&at("run.puml:6", "message", Some("door.puml"))));
}

#[test]
fn a_name_nothing_uses_is_none_and_no_name_is_an_error() {
    assert!(listed("Window", None).is_empty());
    let files = files();
    let specs = specs(&files);
    assert!(refs("--", None, &files, &specs).is_err());
    assert!(refs("Idle", Some("Window"), &files, &specs).is_err());
}
