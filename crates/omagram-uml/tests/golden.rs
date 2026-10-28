//! Golden tests: every diagram in `tests/sequence`, `tests/activity`,
//! `tests/state`, `tests/structure`, `tests/timing`, `tests/gantt`,
//! `tests/mindmap`, `tests/wbs` and `tests/flow` (and the PlantUML examples
//! the engine draws) against its SVG in `tests/golden`, one element per
//! line. After
//! an intended change, regenerate them with `UPDATE_GOLDEN=1 cargo test
//! -p omagram-uml --test golden` and review the diff.

use std::fs;
use std::path::{Path, PathBuf};

/// The SVG with a line break between elements, for readable diffs.
fn lines(svg: &str) -> String {
    let mut out = svg.replace("><", ">\n<");
    out.push('\n');
    out
}

#[test]
fn diagrams_match_golden_svgs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut files: Vec<PathBuf> = [
        "sequence",
        "activity",
        "state",
        "structure",
        "timing",
        "gantt",
        "mindmap",
        "wbs",
        "flow",
    ]
    .iter()
    .flat_map(|dir| fs::read_dir(root.join("tests").join(dir)).unwrap())
    .map(|e| e.unwrap().path())
    .collect();
    for example in ["login", "release", "ideas", "launch", "player"] {
        files.push(root.join(format!("../../examples/{example}.puml")));
    }
    files.sort();

    let mut mismatches = Vec::new();
    for file in &files {
        let name = file.file_name().unwrap().to_str().unwrap();
        let code = fs::read_to_string(file).unwrap();
        let svg = omagram_uml::render(&code).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let actual = lines(&svg);
        let golden = root.join("tests/golden").join(format!("{name}.svg"));
        if update {
            fs::write(&golden, &actual).unwrap();
        } else if fs::read_to_string(&golden).ok().as_deref() != Some(actual.as_str()) {
            mismatches.push(name.to_owned());
        }
    }
    assert!(
        mismatches.is_empty(),
        "changed: {mismatches:?}; run with UPDATE_GOLDEN=1 and review the diff"
    );
}
