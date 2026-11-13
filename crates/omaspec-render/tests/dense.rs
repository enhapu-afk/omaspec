//! A dense flowchart drawn while one types: 150 boxes and 476 lines
//! between random pairs, as in Mermaid's issue 8777, where Mermaid takes
//! half a minute. The layout orders and straightens its lines in well
//! under a second in a release build; CI runs this test that way too
//! (`cargo test --release -p omaspec-render --test dense`).

use std::collections::HashSet;
use std::time::{Duration, Instant};

use omaspec_core::Language;
use omaspec_render::{Config, Engine, Renderer};

/// `n` boxes and `m` lines between random pairs of them, the same on every
/// run, back and forth and round in circles as they come.
fn dense(n: usize, m: usize) -> String {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % bound as u64) as usize
    };
    let mut seen = HashSet::new();
    let mut out = String::from("flowchart TD\n");
    while seen.len() < m {
        let (a, b) = (next(n), next(n));
        if a != b && seen.insert((a, b)) {
            out.push_str(&format!("  N{a} --> N{b}\n"));
        }
    }
    out
}

#[test]
fn a_dense_flowchart_is_drawn_in_under_a_second() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let mut took = Duration::MAX;
    // The smaller ones show how the time grows; a debug build takes only
    // the largest.
    let sizes: &[(usize, usize)] = if cfg!(debug_assertions) {
        &[(150, 476)]
    } else {
        &[(50, 158), (100, 317), (150, 476)]
    };
    // The best of a few runs: a shared runner busy for a moment slows one.
    let runs = if cfg!(debug_assertions) { 1 } else { 3 };
    for &(n, m) in sizes {
        let code = dense(n, m);
        took = Duration::MAX;
        for _ in 0..runs {
            let start = Instant::now();
            let out = renderer.render(&code, Language::Mermaid).unwrap();
            took = took.min(start.elapsed());
            assert_eq!(out.engine, Engine::NativeMermaid, "drawn by Omaspec");
            assert_eq!(out.map.wires.len(), m, "every line is drawn");
        }
        eprintln!("{n} boxes, {m} lines: {took:?}");
    }
    // A debug build is many times slower; it only must not run away.
    let limit = if cfg!(debug_assertions) {
        Duration::from_secs(30)
    } else {
        Duration::from_secs(1)
    };
    assert!(took < limit, "150 boxes and 476 lines took {took:?}");
}
