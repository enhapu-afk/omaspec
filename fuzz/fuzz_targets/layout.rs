//! The layout of graph diagrams on any graph the bytes describe: boxes,
//! lines, some of them trees, frames around some boxes, and boxes held
//! against others. No panic, every place finite, a route for every line,
//! every box inside its frame and no two boxes on top of each other.
#![no_main]

use libfuzzer_sys::fuzz_target;
use omaspec_uml::graph::hold::{Hold, Word};
use omaspec_uml::graph::{Direction, Edge, Frame, Node, Tree, framed};

fuzz_target!(|bytes: &[u8]| {
    let mut at = 0;
    let mut next = || {
        let b = bytes.get(at).copied().unwrap_or(0);
        at += 1;
        b
    };
    let n = usize::from(next() % 24) + 1;
    let nodes: Vec<Node> = (0..n)
        .map(|_| Node {
            width: 20.0 + f32::from(next() % 120),
            height: 16.0 + f32::from(next() % 60),
            before: 0.0,
            loop_extra: 0.0,
            flat: next() % 2 == 0,
            round: next() % 3 == 0,
        })
        .collect();
    // Up to three frames, nested or side by side, around some boxes.
    let count = usize::from(next() % 4);
    let frames: Vec<Frame> = (0..count)
        .map(|f| Frame {
            parent: (f > 0 && next() % 2 == 0).then(|| usize::from(next()) % f),
            top: 24.0,
            bottom: 12.0,
            left: 12.0,
            right: 12.0,
            min_width: f32::from(next() % 100),
            ..Frame::default()
        })
        .collect();
    let within: Vec<Option<usize>> = if count == 0 {
        Vec::new()
    } else {
        (0..n)
            .map(|_| {
                let pick = usize::from(next()) % (count + 1);
                (pick < count).then_some(pick)
            })
            .collect()
    };
    let lines = usize::from(next() % 40);
    let edges: Vec<Edge> = (0..lines)
        .map(|_| {
            let kind = next();
            Edge {
                // Now and then an end at a frame (past the boxes).
                from: usize::from(next()) % (n + count),
                to: usize::from(next()) % (n + count),
                label_width: f32::from(next() % 60),
                label_height: if kind % 3 == 0 { 14.0 } else { 0.0 },
                hidden: kind % 11 == 0,
                apart: kind % 7 == 0,
                marked: kind % 5 == 0,
                arrival: u32::from(kind % 3),
                // Now and then a head where it starts, as `Base <|-- Kind`.
                departure: u32::from(kind / 3 % 3),
                label_before: None,
                level: (kind % 13 == 0).then_some(kind % 2 == 0),
                // Now and then a line of a tree, as subclasses under one
                // triangle.
                tree: (kind % 4 == 0).then(|| Tree {
                    key: kind % 3,
                    at_from: kind % 8 == 0,
                }),
            }
        })
        .collect();
    let holds: Vec<Hold> = (0..usize::from(next() % 5))
        .map(|_| Hold {
            node: usize::from(next()) % (n + 1),
            beside: usize::from(next()) % (n + 1),
            word: Word::ALL[usize::from(next()) % Word::ALL.len()],
        })
        .collect();
    let direction = if next() % 2 == 0 {
        Direction::Down
    } else {
        Direction::Right
    };
    // Now and then each frame's boxes in a chain, as the states of a
    // container follow one another: frames then share ranks.
    let mut edges = edges;
    if next() % 2 == 1 {
        for f in 0..count {
            let members: Vec<usize> = (0..n).filter(|&i| within[i] == Some(f)).collect();
            edges.extend(members.windows(2).map(|pair| Edge {
                from: pair[0],
                to: pair[1],
                label_width: 0.0,
                label_height: 0.0,
                hidden: false,
                apart: false,
                marked: false,
                arrival: 0,
                departure: 0,
                label_before: None,
                level: None,
                tree: None,
            }));
        }
    }
    let laid = framed(&nodes, &frames, &within, &edges, &holds, direction);
    assert_eq!(laid.nodes.len(), n);
    assert_eq!(laid.routes.len(), edges.len());
    assert!(laid.unheld.iter().all(|(index, _)| *index < holds.len()));
    for rect in &laid.nodes {
        assert!(rect.x.is_finite() && rect.y.is_finite(), "{rect:?}");
    }
    for route in &laid.routes {
        assert!(route.points.iter().all(|p| p.0.is_finite() && p.1.is_finite()));
    }
    // Every box inside its frame.
    for (index, f) in within.iter().enumerate() {
        if let Some(f) = *f {
            let (inner, outer) = (laid.nodes[index], laid.frames[f]);
            let holds = inner.x >= outer.x - 0.5
                && inner.y >= outer.y - 0.5
                && inner.x + inner.w <= outer.x + outer.w + 0.5
                && inner.y + inner.h <= outer.y + outer.h + 0.5;
            assert!(
                holds,
                "{index} {inner:?} outside frame {f} {outer:?}: {within:?} {edges:?}"
            );
        }
    }
    // No two boxes on top of each other.
    for (i, a) in laid.nodes.iter().enumerate() {
        for b in &laid.nodes[i + 1..] {
            let apart = a.x + a.w <= b.x + 0.5
                || b.x + b.w <= a.x + 0.5
                || a.y + a.h <= b.y + 0.5
                || b.y + b.h <= a.y + 0.5;
            assert!(apart, "{a:?} and {b:?} overlap: {nodes:?} {edges:?} {holds:?}");
        }
    }
});
