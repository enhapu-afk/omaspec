//! Golden tests for the Mermaid diagrams Omaspec lays out itself: every
//! diagram in `tests/layered` (state diagrams and flowcharts, drawn by the
//! layered drawer) and every Mermaid example of the repository Omaspec
//! draws, against its SVG in `tests/golden`, one element per line. A
//! diagram mmdr draws instead is no golden: mmdr is not ours to pin.
//!
//! After an intended change, regenerate them with `UPDATE_GOLDEN=1 cargo
//! test -p omaspec-render --test golden` and review the diff.

use std::fs;
use std::path::{Path, PathBuf};

use omaspec_core::Language;
use omaspec_render::{Config, Engine, Renderer};

/// The SVG with a line break between elements, for readable diffs.
fn lines(svg: &str) -> String {
    let mut out = svg.replace("><", ">\n<");
    out.push('\n');
    out
}

fn mermaid_in(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "mmd"))
        .collect()
}

#[test]
fn mermaid_diagrams_match_golden_svgs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let corpus = mermaid_in(&root.join("tests/layered"));
    let mut examples = mermaid_in(&root.join("../../examples"));
    examples.extend(mermaid_in(&root.join("../../examples/gallery")));
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });

    let mut files: Vec<(PathBuf, bool)> = corpus.into_iter().map(|p| (p, true)).collect();
    files.extend(examples.into_iter().map(|p| (p, false)));
    files.sort();
    let mut mismatches = Vec::new();
    let mut pinned = 0;
    for (file, ours) in &files {
        let name = file.file_name().unwrap().to_str().unwrap();
        let code = fs::read_to_string(file).unwrap();
        let drawn = renderer
            .render(&code, Language::Mermaid)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        if drawn.engine != Engine::NativeMermaid {
            assert!(!ours, "{name}: drawn by mmdr, not by the layered drawer");
            continue;
        }
        pinned += 1;
        let actual = lines(&drawn.svg);
        let golden = root.join("tests/golden").join(format!("{name}.svg"));
        if update {
            fs::create_dir_all(golden.parent().unwrap()).unwrap();
            fs::write(&golden, &actual).unwrap();
        } else if fs::read_to_string(&golden).ok().as_deref() != Some(actual.as_str()) {
            mismatches.push(name.to_owned());
        }
    }
    assert!(pinned >= 20, "only {pinned} diagrams pinned");
    assert!(
        mismatches.is_empty(),
        "changed: {mismatches:?}; run with UPDATE_GOLDEN=1 and review the diff"
    );
}
