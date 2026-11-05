//! `omaspec git setup`: `git diff` tells diagrams by their behavior.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn omaspec(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args(args)
        .output()
        .expect("omaspec runs");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("omaspec-gitsetup-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Git in `dir`, as nobody in particular, unsigned, without a pager; its
/// output, or `None` when it failed.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false", "-c", "core.pager=cat"])
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";

#[test]
fn git_diff_tells_a_diagram_by_its_behavior() {
    let dir = scratch("diff");
    fs::write(dir.join("door.puml"), DOOR).unwrap();
    fs::write(dir.join("notes.txt"), "one\n").unwrap();
    if git(&dir, &["init", "-q"]).is_none() {
        eprintln!("skipped: no git");
        return;
    }
    assert!(git(&dir, &["add", "."]).is_some() && git(&dir, &["commit", "-qm", "one"]).is_some());

    let (code, out, err) = omaspec(&["git", "setup", dir.to_str().unwrap()]);
    assert_eq!(code, Some(0), "{err}");
    assert!(out.contains("*.puml, *.plantuml"), "{out}");
    let attributes = fs::read_to_string(dir.join(".gitattributes")).unwrap();
    assert!(
        attributes.contains("*.puml diff=omaspec\n*.plantuml diff=omaspec"),
        "{attributes}"
    );
    let command = git(&dir, &["config", "--local", "diff.omaspec.textconv"]).unwrap();
    assert!(command.trim_end().ends_with(" git textconv"), "{command}");
    // Once is enough: a second run adds nothing.
    let (code, out, _) = omaspec(&["git", "setup", dir.to_str().unwrap()]);
    assert_eq!(code, Some(0));
    assert!(
        out.contains("already") && out.contains("as it was"),
        "{out}"
    );
    assert_eq!(
        fs::read_to_string(dir.join(".gitattributes")).unwrap(),
        attributes
    );

    // A line moved and a transition added: the diff tells the transition.
    fs::write(
        dir.join("door.puml"),
        "@startuml\ntitle Door\nOpen --> Closed : close\n[*] --> Closed\nClosed --> Open : open\nClosed --> Locked : lock\n@enduml\n",
    )
    .unwrap();
    fs::write(dir.join("notes.txt"), "two\n").unwrap();
    let diff = git(&dir, &["diff", "--no-color"]).unwrap();
    assert!(diff.contains("\n+  Closed -> Locked on lock\n"), "{diff}");
    assert!(diff.contains("\n+  state Locked\n"), "{diff}");
    assert!(!diff.contains("Open --> Closed"), "{diff}");
    // Any other file as it is, and the lines on request.
    assert!(diff.contains("\n-one\n+two\n"), "{diff}");
    let lines = git(&dir, &["diff", "--no-color", "--no-textconv", "door.puml"]).unwrap();
    assert!(lines.contains("+Open --> Closed : close"), "{lines}");

    // git log -p tells the commits the same way.
    assert!(git(&dir, &["commit", "-qam", "lock"]).is_some());
    let log = git(&dir, &["log", "-p", "-n1", "--no-color", "--", "door.puml"]).unwrap();
    assert!(log.contains("+  Closed -> Locked on lock"), "{log}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn textconv_hands_back_what_it_does_not_read() {
    let dir = scratch("textconv");
    let flow = dir.join("flow.puml");
    fs::write(&flow, "@startuml\nstart\n:fill;\nstop\n@enduml\n").unwrap();
    let (code, out, _) = omaspec(&["git", "textconv", flow.to_str().unwrap()]);
    assert_eq!(code, Some(0));
    assert_eq!(out, "@startuml\nstart\n:fill;\nstop\n@enduml\n");
    let door = dir.join("door.mmd");
    fs::write(&door, "stateDiagram-v2\n[*] --> Closed\n").unwrap();
    let (_, out, _) = omaspec(&["git", "textconv", door.to_str().unwrap()]);
    // No title: a temporary file's name is no name.
    assert!(out.starts_with("State machine\n  state Closed\n"), "{out}");
    assert!(out.contains("  [*] -> Closed\n"), "{out}");
    let (code, _, _) = omaspec(&["git", "setup", dir.to_str().unwrap()]);
    assert_eq!(code, Some(2), "a folder in no repository");
    let _ = fs::remove_dir_all(&dir);
}
