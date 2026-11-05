//! `omaspec diff DIR`: what changed in the behavior of a project since a
//! commit, or from another folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn omaspec(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args(args)
        .env("OMASPEC_PLANTUML", "none")
        .output()
        .expect("omaspec runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("omaspec-history-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, file: &str, text: &str) {
    let path = dir.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// Git in `dir`, committing as nobody in particular, unsigned.
fn git(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .is_ok_and(|o| o.status.success())
}

const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
const VISIT: &str = "@startuml\nUser -> Door : lock\n@enduml\n";
const FLOW: &str = "@startuml\nstart\n:fill;\nstop\n@enduml\n";
const NOTES: &str =
    "# Lamp\n\n```mermaid\nstateDiagram-v2\n[*] --> Off\nOff --> On : switch\n```\n";

#[test]
fn a_project_since_its_last_commit() {
    let dir = scratch("git");
    let specs = dir.join("specs");
    write(&specs, "door.puml", DOOR);
    write(&specs, "visit.puml", VISIT);
    write(&specs, "flow.puml", FLOW);
    write(&specs, "notes.md", NOTES);
    write(&dir, "elsewhere.puml", DOOR);
    if !(git(&dir, &["init", "-q"])
        && git(&dir, &["add", "."])
        && git(&dir, &["commit", "-qm", "one"]))
    {
        eprintln!("skipped: no git");
        return;
    }
    let folder = specs.to_str().unwrap();
    let (code, out, err) = omaspec(&["diff", folder]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(out.is_empty());
    assert!(
        err.contains("no diagram changed in its behavior since HEAD"),
        "{err}"
    );

    // The door no longer locks; the flow moved; a gate is new; the notes
    // are only indented; a file outside the folder does not count.
    write(
        &specs,
        "door.puml",
        &DOOR
            .replace("Closed --> Locked : lock\n", "")
            .replace("Locked --> Closed : unlock\n", ""),
    );
    fs::remove_file(specs.join("flow.puml")).unwrap();
    write(&specs, "sub/flow.puml", FLOW);
    write(&specs, "gate.puml", "@startuml\n[*] --> Shut\n@enduml\n");
    write(&specs, "notes.md", &NOTES.replace("\nOff", "\n  Off"));
    fs::remove_file(dir.join("elsewhere.puml")).unwrap();

    let (code, out, err) = omaspec(&["diff", folder]);
    assert_eq!(code, Some(1), "{out}{err}");
    assert!(
        out.contains("door.puml:6: removed: Transition removed: Closed to Locked on lock."),
        "{out}"
    );
    assert!(
        out.lines()
            .any(|l| l.starts_with("visit.puml:2: found") && l.contains("no transition for lock")),
        "{out}"
    );
    assert!(
        out.contains("gate.puml:1: added: New state machine gate."),
        "{out}"
    );
    assert!(
        out.contains("sub/flow.puml:1: changed: Moved from flow.puml."),
        "{out}"
    );
    assert!(
        !out.contains("notes.md") && !out.contains("elsewhere"),
        "{out}"
    );
    assert!(
        err.contains("4 diagrams changed since HEAD: 1 new"),
        "{err}"
    );

    let (code, out, _) = omaspec(&["diff", "--json", folder]);
    assert_eq!(code, Some(1));
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["same"], false);
    let moved = json["diagrams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["old"] == "flow.puml")
        .expect("the flow moved");
    assert_eq!(moved["new"], "sub/flow.puml");
    assert_eq!(moved["changes"][0]["change"], "changed");
    assert_eq!(moved["changes"][0]["new"]["line"], 1);

    // An older commit, by its name; one that does not look like one.
    assert!(git(&dir, &["tag", "first"]));
    let (code, _, err) = omaspec(&["diff", "--rev", "first", folder]);
    assert_eq!(code, Some(1));
    assert!(err.contains("since first"), "{err}");
    let (code, _, err) = omaspec(&["diff", "--rev", "--output=x", folder]);
    assert_eq!(code, Some(2), "{err}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn two_folders_and_what_a_folder_does_not_take() {
    let dir = scratch("folders");
    let (old, new) = (dir.join("old"), dir.join("new"));
    write(&old, "door.puml", DOOR);
    write(&new, "door.puml", &DOOR.replace("lock\n", "lock, bolt\n"));
    let (old_s, new_s) = (old.to_str().unwrap(), new.to_str().unwrap());
    let (code, out, err) = omaspec(&["diff", old_s, new_s]);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        out.starts_with("door.puml:6: changed: Closed to Locked now happens on bolt, lock"),
        "{out}"
    );
    assert!(
        err.contains(&format!("1 diagram changed from {old_s} to {new_s}")),
        "{err}"
    );
    let (code, _, _) = omaspec(&["diff", old_s, old_s]);
    assert_eq!(code, Some(0));

    let file = new.join("door.puml");
    for (args, says) in [
        (
            vec!["diff", old_s, file.to_str().unwrap()],
            "two files or two folders",
        ),
        (
            vec!["diff", "--project", old_s, new_s],
            "a folder is its own project",
        ),
        (
            vec!["diff", "--lang", "mermaid", new_s],
            "--lang is for a diagram file",
        ),
        (
            vec!["diff", "--rev", "HEAD", old_s, new_s],
            "--rev compares one folder with a commit",
        ),
    ] {
        let (code, _, err) = omaspec(&args);
        assert_eq!(code, Some(2), "{args:?}");
        assert!(err.contains(says), "{args:?}: {err}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_diagram_s_log_commit_by_commit() {
    let dir = scratch("log");
    let commit =
        |message: &str| git(&dir, &["add", "-A"]) && git(&dir, &["commit", "-qm", message]);
    write(&dir, "door.puml", DOOR);
    write(&dir, "visit.puml", VISIT);
    if !(git(&dir, &["init", "-q"]) && commit("one")) {
        eprintln!("skipped: no git");
        return;
    }
    // Only indented: the same behavior.
    write(&dir, "door.puml", &DOOR.replace("\n[*]", "\n  [*]"));
    assert!(commit("indent"));
    fs::create_dir_all(dir.join("rooms")).unwrap();
    assert!(git(&dir, &["mv", "door.puml", "rooms/door.puml"]) && commit("move"));
    // The door no longer locks, and the visit knocks first: the visit's
    // log checks it against the door of that commit.
    write(
        &dir,
        "rooms/door.puml",
        &DOOR
            .replace("Closed --> Locked : lock\n", "")
            .replace("Locked --> Closed : unlock\n", ""),
    );
    write(
        &dir,
        "visit.puml",
        &VISIT.replace(
            "User -> Door : lock",
            "User -> Door : knock\nUser -> Door : lock",
        ),
    );
    assert!(commit("no lock"));

    let door = dir.join("rooms/door.puml");
    let (code, out, err) = omaspec(&["log", door.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{err}");
    let blocks: Vec<&str> = out.split("\n").filter(|l| !l.starts_with("    ")).collect();
    assert!(blocks[0].ends_with(" no lock"), "{out}");
    assert!(blocks[1].ends_with(" move"), "{out}");
    assert!(blocks[2].ends_with(" indent"), "{out}");
    assert!(blocks[3].ends_with(" one"), "{out}");
    assert!(
        out.contains("^:6: removed: Transition removed: Closed to Locked on lock."),
        "{out}"
    );
    assert!(out.contains(":1: changed: Moved from door.puml."), "{out}");
    assert!(
        out.contains(" indent\n    no change in behavior\n"),
        "{out}"
    );
    assert!(out.contains("door.puml@"), "{out}");
    assert!(out.contains(":1: added: New state machine Door."), "{out}");
    assert!(err.contains("4 commits changed"), "{err}");
    assert!(err.contains("1 of them kept its behavior"), "{err}");

    let visit = dir.join("visit.puml");
    let (code, out, err) = omaspec(&["log", "-n", "1", "--json", visit.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{err}");
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    let entries = json.as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["subject"], "no lock");
    let changes = entries[0]["diagrams"][0]["changes"].as_array().unwrap();
    let said = |kind: &str, text: &str| {
        changes
            .iter()
            .any(|c| c["change"] == kind && c["message"].as_str().unwrap().contains(text))
    };
    assert!(said("added", "knock"), "{out}");
    assert!(said("found", "no transition for lock"), "{out}");

    // Outside a repository, git says so.
    let (code, _, err) = omaspec(&["log", scratch("nogit").join("a.puml").to_str().unwrap()]);
    assert_eq!(code, Some(2), "{err}");
    let _ = fs::remove_dir_all(&dir);
}
