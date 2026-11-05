//! `omaspec check` and the pre-commit hook of `omaspec new --git`.

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
    let dir = std::env::temp_dir().join(format!("omaspec-check-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn check_runs_lint_the_links_and_the_pictures() {
    let dir = scratch("project");
    fs::write(dir.join("omaspec-project.toml"), "name = \"shop\"\n").unwrap();
    fs::write(
        dir.join("door.puml"),
        "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> [*]\n@enduml\n",
    )
    .unwrap();
    fs::write(
        dir.join("visit.puml"),
        "@startuml\nstate Open [[door.svg]]\nstate Gone [[gone.svg]]\n[*] --> Open\nOpen --> Gone\nGone --> [*]\n@enduml\n",
    )
    .unwrap();
    let root = dir.to_str().unwrap();

    // No SVG beside the diagrams yet, and a link to a diagram that is not there.
    let (code, out, err) = omaspec(&["check", root]);
    assert_eq!(code, Some(1), "{err}\n{out}");
    assert!(
        out.contains("visit.puml:3: warning: The link to gone.svg leads to no diagram"),
        "{out}"
    );
    assert!(out.contains("door.svg: not there"), "{out}");
    assert!(out.contains("visit.svg: not there"), "{out}");
    assert!(
        err.contains("1 warning") && err.contains("2 of 2 SVGs differ"),
        "{err}"
    );

    // Warnings stay warnings until --strict; the missing SVGs are what fails.
    assert_eq!(omaspec(&["check", "--strict", root]).0, Some(1));

    assert_eq!(omaspec(&["render", root]).0, Some(0), "draw the SVGs");
    let (code, out, err) = omaspec(&["check", root]);
    assert_eq!(code, Some(0), "{err}\n{out}");
    assert!(out.contains("visit.puml:3: warning:"), "{out}");
    assert!(!out.contains("not there"), "{out}");
    assert!(err.contains("1 warning; 2 SVGs drawn as they are"), "{err}");
    assert_eq!(omaspec(&["check", "--strict", root]).0, Some(1));

    // The same bytes `render --check` holds against.
    let (render_code, render_out, render_err) = omaspec(&["render", "--check", root]);
    assert_eq!(render_code, Some(0), "{render_err}");
    assert_eq!(render_out, "");

    fs::write(
        dir.join("door.puml"),
        "@startuml\ntitle Door\n[*] --> Closed\n@enduml\n",
    )
    .unwrap();
    let (code, out, _) = omaspec(&["check", "--json", root]);
    assert_eq!(code, Some(1));
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["ok"], false);
    let pictures = json["pictures"].as_array().unwrap();
    let door = pictures.iter().find(|p| p["file"] == "door.puml").unwrap();
    assert_eq!(door["svg"], "door.svg");
    assert_eq!(door["same"], false);
    assert!(
        door["message"].as_str().unwrap().contains("changed since"),
        "{door}"
    );

    let (code, out, err) = omaspec(&["check", "--github", root]);
    assert_eq!(code, Some(1), "{err}");
    assert!(
        out.contains("::warning file=visit.puml,line=3::")
            && out.contains("::error file=door.svg::")
            && out.contains("changed since"),
        "{out}"
    );
    assert!(
        !out.contains("visit.puml:3:"),
        "annotations, not the report"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn new_git_writes_a_pre_commit_hook_that_runs_check() {
    let dir = scratch("new-git");
    let shop = dir.join("shop");
    let (code, out, err) = omaspec(&["new", "--git", "--json", shop.to_str().unwrap()]);
    if code == Some(1) {
        eprintln!("skipped: {err}");
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    assert_eq!(code, Some(0), "{err}\n{out}");
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["git"]["initialized"], true);
    assert_eq!(json["git"]["hook"], "written");
    let hook = shop.join(".git/hooks/pre-commit");
    let text = fs::read_to_string(&hook).unwrap();
    assert!(text.starts_with("#!/bin/sh\n"), "{text}");
    assert!(text.contains("exec omaspec check .\n"), "{text}");

    // The starter has no SVGs yet, so the check the hook runs fails on them.
    let (code, out, _) = omaspec(&["check", shop.to_str().unwrap()]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.contains("system.svg: not there") && out.contains("shop.svg: not there"),
        "{out}"
    );

    let _ = fs::remove_dir_all(Path::new(&dir));
}
