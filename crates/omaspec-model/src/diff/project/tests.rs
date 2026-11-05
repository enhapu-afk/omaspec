use omaspec_core::{Language, detect, markdown};

use super::*;

/// A project version from its files, each by its path and its text.
fn files<'a>(list: &[(&'a str, &'a str)]) -> Vec<File<'a>> {
    list.iter()
        .map(|&(path, text)| File {
            path,
            text,
            kind: if markdown::is_markdown(path) {
                FileKind::Document
            } else {
                FileKind::Diagram(detect(text, Language::from_file_name(path)))
            },
        })
        .collect()
}

/// The changes from `old` to `new`, each as its files, the kind, the
/// message and its line in the new version (else the old one), one-based.
fn told(old: &[(&str, &str)], new: &[(&str, &str)]) -> Vec<(String, &'static str, String, usize)> {
    diff_project(&files(old), &files(new))
        .into_iter()
        .flat_map(|d| {
            let at = match (&d.old_file, &d.new_file) {
                (Some(a), Some(b)) if a != b => format!("{a} > {b}"),
                (_, Some(b)) => b.clone(),
                (Some(a), None) => a.clone(),
                (None, None) => unreachable!(),
            };
            d.changes.into_iter().map(move |c| {
                let line = c.new_line.or(c.old_line).unwrap() + 1;
                (at.clone(), c.kind.name(), c.message, line)
            })
        })
        .collect()
}

const DOOR: &str = "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
const LAMP: &str = "stateDiagram-v2\n[*] --> Off\nOff --> On : switch\nOn --> Off : switch\n";
const VISIT: &str =
    "@startuml\nUser -> Door : open\nUser -> Door : close\nUser -> Door : lock\n@enduml\n";

#[test]
fn only_what_changed_is_told_by_its_file() {
    let jammed = DOOR.replace("@enduml", "Open --> Jammed : kick\n@enduml");
    let got = told(
        &[
            ("door.puml", DOOR),
            ("lamp.mmd", LAMP),
            ("visit.puml", VISIT),
        ],
        &[
            ("door.puml", &jammed),
            ("lamp.mmd", LAMP),
            ("visit.puml", VISIT),
        ],
    );
    assert_eq!(
        got,
        [
            ("door.puml".into(), "added", "New state Jammed.".into(), 7),
            (
                "door.puml".into(),
                "added",
                "New transition: Open to Jammed on kick.".into(),
                7
            ),
            (
                "door.puml".into(),
                "found",
                "Nothing leaves Jammed. If the machine ends there, add Jammed --> [*].".into(),
                7
            ),
        ]
    );
    // Nothing changed: nothing told.
    let same = [("door.puml", DOOR), ("visit.puml", VISIT)];
    assert!(told(&same, &same).is_empty());
}

#[test]
fn a_scenario_that_did_not_change_tells_what_its_machine_no_longer_takes() {
    let unlockable = DOOR
        .replace("Closed --> Locked : lock\n", "")
        .replace("Locked --> Closed : unlock\n", "");
    let got = told(
        &[("door.puml", DOOR), ("visit.puml", VISIT)],
        &[("door.puml", &unlockable), ("visit.puml", VISIT)],
    );
    let visit: Vec<_> = got.iter().filter(|c| c.0 == "visit.puml").collect();
    assert_eq!(visit.len(), 1, "{got:?}");
    assert_eq!(visit[0].1, "found");
    assert!(
        visit[0].2.starts_with("Door has no transition for lock"),
        "{}",
        visit[0].2
    );
    assert_eq!(visit[0].3, 4);
}

#[test]
fn a_moved_diagram_is_found_by_its_text_then_by_its_name() {
    let titled = LAMP.replace("stateDiagram-v2", "---\ntitle: Lamp\n---\nstateDiagram-v2");
    let dimmed = titled.replace(
        "On --> Off : switch",
        "On --> Dim : switch\nDim --> Off : switch",
    );
    let got = told(
        &[("door.puml", DOOR), ("lamp.mmd", &titled)],
        &[("rooms/door.puml", DOOR), ("rooms/light.mmd", &dimmed)],
    );
    assert_eq!(got[0].0, "door.puml > rooms/door.puml");
    assert_eq!(got[0].1, "changed");
    assert_eq!(got[0].2, "Moved from door.puml.");
    assert_eq!(got[1].0, "lamp.mmd > rooms/light.mmd");
    assert_eq!(got[1].2, "Moved from lamp.mmd.");
    assert!(got[2..].iter().any(|c| c.2 == "New state Dim."), "{got:?}");
    assert!(
        got.iter()
            .all(|c| c.1 != "removed" || c.2.starts_with("Transition"))
    );

    // A folder renamed, the diagram's text and title changed with it.
    let renamed = titled
        .replace("Lamp", "Light")
        .replace("Off --> On", "Off --> Dim");
    let got = told(
        &[("house/lamp.mmd", &titled)],
        &[("home/lamp.mmd", &renamed)],
    );
    assert_eq!(got[0].0, "house/lamp.mmd > home/lamp.mmd");
    assert_eq!(got[0].2, "Moved from house/lamp.mmd.");
}

#[test]
fn a_finding_that_names_a_moved_machine_is_no_change() {
    let kick = "@startuml\nUser -> Door : kick\n@enduml\n";
    let got = told(
        &[("door.puml", DOOR), ("visit.puml", kick)],
        &[("rooms/door.puml", DOOR), ("visit.puml", kick)],
    );
    assert_eq!(
        got,
        [(
            "door.puml > rooms/door.puml".into(),
            "changed",
            "Moved from door.puml.".into(),
            1
        )]
    );
}

#[test]
fn new_and_removed_diagrams_with_what_the_checks_find() {
    let gate = "@startuml\n[*] --> Shut\nShut --> Wide : open\nstate Stuck\n@enduml\n";
    let got = told(
        &[("door.puml", DOOR), ("visit.puml", VISIT)],
        &[("door.puml", DOOR), ("gate.puml", gate)],
    );
    assert_eq!(got[0].0, "gate.puml");
    assert_eq!(got[0].1, "added");
    assert_eq!(got[0].2, "New state machine gate.");
    assert_eq!(got[0].3, 1);
    assert!(
        got.iter()
            .any(|c| c.1 == "found" && c.2 == "State Stuck cannot be reached from the start."),
        "{got:?}"
    );
    let removed: Vec<_> = got.iter().filter(|c| c.0 == "visit.puml").collect();
    assert_eq!(removed[0].1, "removed");
    assert_eq!(removed[0].2, "Sequence diagram visit removed.");
}

#[test]
fn a_document_s_diagrams_are_paired_by_name_on_its_lines() {
    let old = format!(
        "# House\n\n```plantuml\n{}```\n\n```mermaid\n{LAMP}```\n",
        DOOR.replace("@startuml\n", "@startuml\ntitle Door\n")
    );
    // A paragraph more, the blocks swapped, the door with a state more.
    let new = format!(
        "# House\n\nHow it opens.\n\n```mermaid\n{LAMP}```\n\n```plantuml\n{}```\n",
        DOOR.replace("@startuml\n", "@startuml\ntitle Door\n")
            .replace(
                "@enduml",
                "Open --> Ajar : wind\nAjar --> Open : push\n@enduml"
            )
    );
    let got = told(&[("README.md", &old)], &[("README.md", &new)]);
    assert_eq!(
        got[0],
        ("README.md".into(), "added", "New state Ajar.".into(), 20)
    );
    assert!(
        got.iter().all(|c| c.0 == "README.md" && c.1 == "added"),
        "{got:?}"
    );
}

#[test]
fn other_diagrams_tell_only_that_their_lines_changed() {
    let flow = "@startuml\nstart\n:fill;\nstop\n@enduml\n";
    let indented = "@startuml\n\n  start\n  :fill;\n  stop\n@enduml\n";
    assert!(told(&[("flow.puml", flow)], &[("flow.puml", indented)]).is_empty());
    let got = told(
        &[("flow.puml", flow)],
        &[("flow.puml", &flow.replace("fill", "pour"))],
    );
    assert_eq!(
        got,
        [(
            "flow.puml".into(),
            "changed",
            "Lines changed; Omaspec compares the behavior of state machines and sequence diagrams, not of an activity diagram.".into(),
            1
        )]
    );
}
