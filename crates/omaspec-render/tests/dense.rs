//! A dense flowchart drawn while one types: 150 boxes and 476 lines
//! between random pairs, as in Mermaid's issue 8777, where Mermaid takes
//! half a minute, and that issue's own flowchart. The layout orders and straightens its lines in well
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

/// The flowchart of Mermaid's issue 8777 itself, as its page builds it:
/// 150 boxes named, a random tree joining them and 330 lines more with
/// words, by JavaScript's numbers (a double each, so the same seed gives
/// the same diagram). Mermaid's ELK takes 36 seconds on it, dagre 2.4.
fn issue_8777() -> String {
    let mut seed = 1.0_f64;
    let mut random = || {
        seed = (seed * 1_103_515_245.0 + 12_345.0) % 2_147_483_648.0;
        seed / 2_147_483_648.0
    };
    let mut out = String::from("flowchart TD\n");
    for i in 0..150 {
        out.push_str(&format!("  N{i}[Component {i}]\n"));
    }
    for i in 1..150 {
        let from = (random() * i as f64).floor() as usize;
        out.push_str(&format!("  N{from} --> N{i}\n"));
    }
    for k in 0..330 {
        let a = (random() * 150.0).floor() as usize;
        let b = (random() * 150.0).floor() as usize;
        if a != b {
            out.push_str(&format!("  N{a} -->|e{k}| N{b}\n"));
        }
    }
    out
}

/// Every line and every word of it drawn, in about a second, and no
/// taller than its lines need: lines whose runs across a gap lie apart
/// share a track there (it was twice as tall before they did).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "a debug build takes 15 s; CI runs it in a release build"
)]
fn the_flowchart_of_mermaid_issue_8777_is_drawn_in_time_and_compact() {
    let code = issue_8777();
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let start = Instant::now();
    let out = renderer.render(&code, Language::Mermaid).unwrap();
    let took = start.elapsed();
    assert_eq!(out.engine, Engine::NativeMermaid, "drawn by Omaspec");
    assert_eq!(out.map.wires.len(), 476, "every line is drawn");
    let words = (0..330)
        .filter(|k| out.svg.contains(&format!(">e{k}<")))
        .count();
    assert_eq!(
        words, 327,
        "every line's words, but those of a line to itself"
    );
    let height: f32 = out
        .svg
        .split("height=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .and_then(|h| h.parse().ok())
        .expect("the picture's height");
    eprintln!("Mermaid's issue 8777: {took:?}, {height} units tall");
    assert!(height < 40_000.0, "{height} units tall");
    assert!(took < Duration::from_secs(3), "took {took:?}");
}
