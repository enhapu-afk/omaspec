//! Golden tests: every diagram in `tests/fixtures` against its report in
//! `tests/golden`. After an intended change, regenerate the reports with
//! `UPDATE_GOLDEN=1 cargo test -p omaspec-core --test golden` and review the diff.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use omaspec_core::{Language, classify, detect, format, lint, outline};

fn report(name: &str, code: &str) -> String {
    let lang = detect(code, Language::from_file_name(name));
    let kind = classify(code, lang);
    let mut out = String::new();
    writeln!(
        out,
        "== {name}: {} / {:?} ({})",
        lang.name(),
        kind.kind,
        kind.name
    )
    .unwrap();
    writeln!(out, "== lint").unwrap();
    for f in lint::lint(code, lang) {
        writeln!(out, "{:>4} {:?}: {}", f.line + 1, f.severity, f.message).unwrap();
    }
    writeln!(out, "== outline").unwrap();
    for e in outline::outline(code, lang) {
        writeln!(
            out,
            "{:>4} {}{} | {}",
            e.line + 1,
            "  ".repeat(e.depth),
            e.tag,
            e.label
        )
        .unwrap();
    }
    writeln!(out, "== fmt").unwrap();
    out.push_str(&format::format(code, lang));
    out
}

#[test]
fn fixtures_match_golden_reports() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut names: Vec<_> = fs::read_dir(root.join("fixtures"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    assert!(!names.is_empty());

    let mut mismatches = Vec::new();
    for name in &names {
        let code = fs::read_to_string(root.join("fixtures").join(name)).unwrap();
        let actual = report(name, &code);
        let golden = root.join("golden").join(format!("{name}.txt"));
        if update {
            fs::write(&golden, &actual).unwrap();
        } else if fs::read_to_string(&golden).ok().as_deref() != Some(actual.as_str()) {
            mismatches.push(name.clone());
        }
    }
    assert!(
        mismatches.is_empty(),
        "reports differ for {mismatches:?}; if intended, run with UPDATE_GOLDEN=1 and review"
    );
}

#[test]
fn formatting_twice_changes_nothing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        let code = fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap();
        let lang = detect(&code, Language::from_file_name(name));
        let once = format::format(&code, lang);
        assert_eq!(format::format(&once, lang), once, "{name}");
    }
}
