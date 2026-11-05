//! `omaspec render` as text: a state machine or a sequence diagram as its
//! behavior, one fact a line.

use std::fs;
use std::path::PathBuf;
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
    let dir = std::env::temp_dir().join(format!("omaspec-text-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
const TEXT: &str = "State machine Door\n  state Closed\n  state Open\n  Closed -> Open on open\n  Open -> Closed on close\n  [*] -> Closed\n";

#[test]
fn a_machine_renders_as_its_behavior() {
    let dir = scratch("machine");
    let door = dir.join("door.puml");
    fs::write(&door, DOOR).unwrap();
    let door = door.to_str().unwrap();

    // To a .txt file, beside the diagram, or to standard output.
    let out = dir.join("out.txt");
    let (code, _, err) = omaspec(&["render", door, "-o", out.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(fs::read_to_string(&out).unwrap(), TEXT);
    let (code, _, _) = omaspec(&["render", "--format", "text", door]);
    assert_eq!(code, Some(0));
    assert_eq!(fs::read_to_string(dir.join("door.txt")).unwrap(), TEXT);
    let (code, stdout, _) = omaspec(&["render", "--format", "text", "-o", "-", door]);
    assert_eq!(code, Some(0));
    assert_eq!(stdout, TEXT);
    let (_, json, _) = omaspec(&["render", "--json", "--format", "text", door]);
    let report: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(report[0]["ok"], true, "{report}");

    // A document's diagrams each in their own file.
    let readme = dir.join("README.md");
    fs::write(
        &readme,
        format!(
            "# Door\n\n```plantuml\n{DOOR}```\n\n```mermaid\nsequenceDiagram\n  A->>B: hi\n```\n"
        ),
    )
    .unwrap();
    let (code, _, err) = omaspec(&["render", "--format", "text", readme.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(fs::read_to_string(dir.join("README-1.txt")).unwrap(), TEXT);
    assert!(
        fs::read_to_string(dir.join("README-2.txt"))
            .unwrap()
            .contains("A -> B : hi")
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_diagram_without_behavior_says_where_to_look() {
    let dir = scratch("flow");
    let flow = dir.join("flow.puml");
    fs::write(&flow, "@startuml\nstart\n:fill;\nstop\n@enduml\n").unwrap();
    let (code, _, err) = omaspec(&[
        "render",
        "--format",
        "text",
        "-o",
        "-",
        flow.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(1));
    assert!(err.contains("omaspec outline"), "{err}");
    let (code, _, err) = omaspec(&[
        "render",
        "--check",
        "--format",
        "text",
        flow.to_str().unwrap(),
    ]);
    assert_eq!(code, Some(2), "{err}");
    let _ = fs::remove_dir_all(&dir);
}
