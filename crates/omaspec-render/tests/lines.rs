//! How clean the lines of the graph diagrams are (ROADMAP: even cleaner
//! lines), measured over every state, structure and flowchart diagram of
//! the repository Omaspec draws: where lines cross each other, where a
//! line runs through a box that is not its end, where a line steps aside
//! by a few units only (a jog), where two lines come so near they look
//! tangled, where a line runs through words (a label, a multiplicity) or
//! words cover other words or a box, how often lines bend and how long
//! they are.
//!
//! `tests/lines.txt` holds the counts. The test fails when crossings,
//! lines through boxes, jogs or words in the way grow for a diagram; near
//! lines, bends and length may change. After an intended change, write
//! the new counts with `UPDATE_LINES=1 cargo test -p omaspec-render --test
//! lines` and review the diff. `OMASPEC_LINES=1` prints the table.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use omaspec_core::Language;
use omaspec_render::{Config, Renderer};
use omaspec_uml::graph::Rect;
use omaspec_uml::map::Wire;

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Count {
    lines: usize,
    crossings: usize,
    through: usize,
    jogs: usize,
    near: usize,
    labels: usize,
    bends: usize,
    length: f32,
}

type Point = (f32, f32);

/// Whether `a`–`b` and `c`–`d` cross at a point inside both, not where
/// one ends.
fn cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    let side = |p: Point, q: Point, r: Point| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    let eps = 0.5;
    ((d1 > eps && d2 < -eps) || (d1 < -eps && d2 > eps))
        && ((d3 > eps && d4 < -eps) || (d3 < -eps && d4 > eps))
}

/// Whether `a`–`b` runs through the inside of `rect`, past its rim.
fn through(a: Point, b: Point, rect: Rect) -> bool {
    let inset = 2.0;
    let (l, t, r, bo) = (
        rect.x + inset,
        rect.y + inset,
        rect.x + rect.w - inset,
        rect.y + rect.h - inset,
    );
    if l >= r || t >= bo {
        return false;
    }
    // Clip the segment to the rectangle (Liang–Barsky).
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    for (p, q) in [
        (-dx, a.0 - l),
        (dx, r - a.0),
        (-dy, a.1 - t),
        (dy, bo - a.1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let u = q / p;
            if p < 0.0 {
                t0 = t0.max(u);
            } else {
                t1 = t1.min(u);
            }
        }
    }
    t1 - t0 > 1e-3 && (t1 - t0) * dx.hypot(dy) > 1.0
}

/// The distance from `p` to the segment `a`–`b`.
fn distance(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

fn inside(p: Point, rect: Rect, slack: f32) -> bool {
    p.0 >= rect.x - slack
        && p.0 <= rect.x + rect.w + slack
        && p.1 >= rect.y - slack
        && p.1 <= rect.y + rect.h + slack
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x - 0.5
        && inner.y >= outer.y - 0.5
        && inner.x + inner.w <= outer.x + outer.w + 0.5
        && inner.y + inner.h <= outer.y + outer.h + 0.5
}

fn measure(wires: &[Wire]) -> Count {
    let paths: Vec<&[Point]> = wires.iter().map(|w| w.path.as_slice()).collect();
    let mut count = Count {
        lines: wires.len(),
        ..Count::default()
    };
    for path in &paths {
        count.bends += path.len().saturating_sub(2);
        // An inner run that only steps aside.
        if path.len() > 3 {
            count.jogs += path[1..path.len() - 1]
                .windows(2)
                .filter(|s| (s[1].0 - s[0].0).hypot(s[1].1 - s[0].1) < 8.0)
                .count();
        }
        count.length += path
            .windows(2)
            .map(|p| (p[1].0 - p[0].0).hypot(p[1].1 - p[0].1))
            .sum::<f32>();
    }
    for i in 0..paths.len() {
        for j in i + 1..paths.len() {
            for a in paths[i].windows(2) {
                for b in paths[j].windows(2) {
                    if cross(a[0], a[1], b[0], b[1]) {
                        count.crossings += 1;
                    }
                }
            }
        }
    }
    // Two lines nearer than a few units, at a corner of one, away from the
    // boxes they share: they look tangled.
    for i in 0..wires.len() {
        for j in 0..wires.len() {
            if i == j {
                continue;
            }
            let shared = [wires[i].from.rect, wires[i].to.rect];
            let corners = wires[i].path.len().saturating_sub(1);
            for &corner in wires[i].path.iter().take(corners).skip(1) {
                if shared.iter().any(|&r| inside(corner, r, 12.0))
                    || [wires[j].from.rect, wires[j].to.rect]
                        .iter()
                        .any(|&r| inside(corner, r, 12.0))
                {
                    continue;
                }
                if wires[j]
                    .path
                    .windows(2)
                    .any(|s| distance(corner, s[0], s[1]) < 6.0)
                {
                    count.near += 1;
                }
            }
        }
    }
    // Words a line runs through, its own too: no drawer measured here sets
    // them on a background. Or words on other words or on a box the lines
    // join, but for one around an end of their line (a nested state, a
    // package) that holds them whole.
    let mut ends: Vec<Rect> = Vec::new();
    for wire in wires {
        for rect in [wire.from.rect, wire.to.rect] {
            if !ends.contains(&rect) {
                ends.push(rect);
            }
        }
    }
    let words: Vec<(&Wire, Rect)> = wires
        .iter()
        .flat_map(|wire| wire.labels.iter().map(move |&rect| (wire, rect)))
        .collect();
    for (k, &(wire, label)) in words.iter().enumerate() {
        let shrunk = Rect {
            x: label.x + 1.0,
            y: label.y + 1.0,
            w: (label.w - 2.0).max(0.0),
            h: (label.h - 2.0).max(0.0),
        };
        let cut = wires
            .iter()
            .any(|other| other.path.windows(2).any(|s| through(s[0], s[1], shrunk)));
        let around = |r: Rect| {
            contains(r, label)
                && [wire.from.rect, wire.to.rect]
                    .iter()
                    .any(|&end| end != r && contains(r, end))
        };
        let covers = words[k + 1..]
            .iter()
            .map(|&(_, other)| other)
            .chain(ends.iter().copied().filter(|&r| !around(r)))
            .any(|other| overlap(shrunk, other));
        if cut || covers {
            count.labels += 1;
        }
    }
    // The boxes the lines join; a box around one of a line's ends (a
    // nested state, a package) is one it may run through.
    let mut boxes: Vec<Rect> = Vec::new();
    for wire in wires {
        for rect in [wire.from.rect, wire.to.rect] {
            if !boxes.contains(&rect) {
                boxes.push(rect);
            }
        }
    }
    for wire in wires {
        for &rect in &boxes {
            if contains(rect, wire.from.rect) || contains(rect, wire.to.rect) {
                continue;
            }
            if wire.path.windows(2).any(|s| through(s[0], s[1], rect)) {
                count.through += 1;
            }
        }
    }
    count
}

fn corpus(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let dirs = [
        "../omaspec-uml/tests/state",
        "../omaspec-uml/tests/structure",
        "tests/layered",
        "../../examples",
        "../../examples/gallery",
        "../../examples/door",
        "../../examples/drone",
    ];
    for dir in dirs {
        let Ok(entries) = fs::read_dir(root.join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if Language::from_file_name(&path.to_string_lossy()).is_some() {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn name(root: &Path, file: &Path) -> String {
    let full = fs::canonicalize(file).unwrap();
    let repo = fs::canonicalize(root.join("../..")).unwrap();
    full.strip_prefix(&repo).unwrap().display().to_string()
}

fn table(counts: &BTreeMap<String, Count>) -> String {
    let mut out = String::from("# file lines crossings through jogs near labels bends length\n");
    let mut total = Count::default();
    for (file, c) in counts {
        let _ = writeln!(
            out,
            "{file} {} {} {} {} {} {} {} {:.0}",
            c.lines, c.crossings, c.through, c.jogs, c.near, c.labels, c.bends, c.length
        );
        total.lines += c.lines;
        total.crossings += c.crossings;
        total.through += c.through;
        total.jogs += c.jogs;
        total.near += c.near;
        total.labels += c.labels;
        total.bends += c.bends;
        total.length += c.length;
    }
    let _ = writeln!(
        out,
        "total {} {} {} {} {} {} {} {:.0}",
        total.lines,
        total.crossings,
        total.through,
        total.jogs,
        total.near,
        total.labels,
        total.bends,
        total.length
    );
    out
}

/// Each diagram's crossings, lines through boxes, jogs and words in the
/// way.
fn read_table(text: &str) -> BTreeMap<String, [usize; 4]> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let parts: Vec<&str> = l.split_whitespace().collect();
            let at = |i: usize| parts.get(i)?.parse().ok();
            Some((parts.first()?.to_string(), [at(2)?, at(3)?, at(4)?, at(6)?]))
        })
        .collect()
}

#[test]
fn lines_get_no_worse() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let mut counts = BTreeMap::new();
    for file in corpus(root) {
        let code = fs::read_to_string(&file).unwrap();
        let language = Language::from_file_name(&file.to_string_lossy()).unwrap();
        let Ok(drawn) = renderer.render(&code, language) else {
            continue;
        };
        if drawn.map.wires.is_empty() {
            continue;
        }
        counts.insert(name(root, &file), measure(&drawn.map.wires));
    }
    assert!(
        counts.len() >= 30,
        "only {} diagrams measured",
        counts.len()
    );
    let now = table(&counts);
    if std::env::var_os("OMASPEC_LINES").is_some() {
        println!("{now}");
    }
    let stored = root.join("tests/lines.txt");
    if std::env::var_os("UPDATE_LINES").is_some() {
        fs::write(&stored, &now).unwrap();
        return;
    }
    let before = read_table(&fs::read_to_string(&stored).unwrap_or_default());
    let after = read_table(&now);
    let mut worse = Vec::new();
    for (file, now) in &after {
        if let Some(was) = before.get(file)
            && now.iter().zip(was).any(|(n, w)| n > w)
        {
            worse.push(format!(
                "{file}: crossings, through boxes, jogs, words {was:?} → {now:?}"
            ));
        }
    }
    assert!(
        worse.is_empty(),
        "lines got worse:\n{}\n(after an intended change: UPDATE_LINES=1)",
        worse.join("\n")
    );
}

/// Lists every jog that is left, by its size:
/// `cargo test -p omaspec-render --test lines jogs -- --ignored --nocapture`.
#[test]
#[ignore]
fn jogs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    for file in corpus(root) {
        let code = fs::read_to_string(&file).unwrap();
        let language = Language::from_file_name(&file.to_string_lossy()).unwrap();
        let Ok(drawn) = renderer.render(&code, language) else {
            continue;
        };
        for w in &drawn.map.wires {
            let p = &w.path;
            if p.len() <= 3 {
                continue;
            }
            for (k, s) in p[1..p.len() - 1].windows(2).enumerate() {
                let len = (s[1].0 - s[0].0).hypot(s[1].1 - s[0].1);
                if len < 8.0 {
                    println!(
                        "{:.1} {} {} -> {} at {} of {}",
                        len,
                        name(root, &file),
                        w.key.from,
                        w.key.to,
                        k + 1,
                        p.len() - 1
                    );
                }
            }
        }
    }
}

/// Lists every crossing, by the two lines and where:
/// `cargo test -p omaspec-render --test lines crossings -- --ignored --nocapture`.
#[test]
#[ignore]
fn crossings() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    for file in corpus(root) {
        let code = fs::read_to_string(&file).unwrap();
        let language = Language::from_file_name(&file.to_string_lossy()).unwrap();
        let Ok(drawn) = renderer.render(&code, language) else {
            continue;
        };
        let wires = &drawn.map.wires;
        for i in 0..wires.len() {
            for j in i + 1..wires.len() {
                for a in wires[i].path.windows(2) {
                    for b in wires[j].path.windows(2) {
                        if cross(a[0], a[1], b[0], b[1]) {
                            println!(
                                "{}: {} -> {} × {} -> {} near ({:.0}, {:.0})",
                                name(root, &file),
                                wires[i].key.from,
                                wires[i].key.to,
                                wires[j].key.from,
                                wires[j].key.to,
                                a[0].0,
                                a[0].1
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Prints the lines of one diagram, with their words:
/// `SHOW=examples/door/door.puml cargo test -p omaspec-render --test lines show -- --ignored --nocapture`.
#[test]
#[ignore]
fn show() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let file = std::env::var("SHOW").unwrap();
    let code = fs::read_to_string(root.join("../..").join(&file)).unwrap();
    let language = Language::from_file_name(&file).unwrap();
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let drawn = renderer.render(&code, language).unwrap();
    for w in &drawn.map.wires {
        let pts: Vec<String> = w
            .path
            .iter()
            .map(|p| format!("({:.0},{:.0})", p.0, p.1))
            .collect();
        println!(
            "{} -> {} words {:?}: {}",
            w.key.from,
            w.key.to,
            w.labels
                .iter()
                .map(|r| (r.x as i32, r.y as i32, r.w as i32, r.h as i32))
                .collect::<Vec<_>>(),
            pts.join(" ")
        );
    }
}
