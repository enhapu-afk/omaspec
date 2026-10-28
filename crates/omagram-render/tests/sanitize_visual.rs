//! Sanitizing must not change how a diagram looks: mmdr's raw SVG and the
//! sanitized one rasterize to the same pixels. Runs over the Mermaid
//! fixtures of omagram-core, plus every `.mmd` in `$OMAGRAM_EXTRA_MERMAID`
//! if set (for local corpora).

use std::fs;
use std::path::{Path, PathBuf};

use mermaid_rs_renderer::{RenderOptions, render_with_options};
use omagram_render::export::{parse, rasterize};
use omagram_render::sanitize::sanitize;

fn mermaid_files() -> Vec<PathBuf> {
    let mut dirs =
        vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../omagram-core/tests/fixtures")];
    if let Some(extra) = std::env::var_os("OMAGRAM_EXTRA_MERMAID") {
        dirs.push(extra.into());
    }
    let mut files: Vec<PathBuf> = dirs
        .iter()
        .flat_map(|d| fs::read_dir(d).unwrap().map(|e| e.unwrap().path()))
        .filter(|p| p.extension().is_some_and(|e| e == "mmd"))
        .collect();
    files.sort();
    files
}

#[test]
fn sanitized_svg_looks_the_same() {
    let mut compared = 0;
    for file in mermaid_files() {
        let source = fs::read_to_string(&file).unwrap();
        let Ok(raw) = render_with_options(&source, RenderOptions::default()) else {
            continue;
        };
        let clean = sanitize(&raw).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        let a = rasterize(&parse(&raw).unwrap(), 1.0).unwrap();
        let b = rasterize(&parse(&clean).unwrap(), 1.0).unwrap();
        assert_eq!(
            (a.width(), a.height()),
            (b.width(), b.height()),
            "{}",
            file.display()
        );
        assert!(
            a.data() == b.data(),
            "{} renders differently after sanitizing",
            file.display()
        );
        compared += 1;
    }
    assert!(compared >= 8, "only {compared} diagrams compared");
}
