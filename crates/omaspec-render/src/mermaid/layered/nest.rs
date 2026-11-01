//! Lines between containers. A line into a subgraph laid out on its own
//! ([`super::open`]) is ranked in the container that holds both ends, as if
//! it joined the boxes there, and routed around everything once the whole
//! diagram is placed.

use std::collections::HashSet;

use omaspec_uml::graph::Rect;
use omaspec_uml::graph::across::{self, Drawn, Link, Placement};
use omaspec_uml::{Block, Family, Font};

use super::shape;
use super::{
    COMP_PAD, Kind, Palette, Placed, PlacedEdge, Scene, Tree, caps, edge_look, measure_edge,
};

/// Around a subgraph's title, which lines keep off.
pub(super) const TITLE_AIR: f32 = 4.0;

/// Routes every transition between containers and adds it to the one
/// that holds both its ends. `None` when one finds no way, which leaves
/// the diagram to mmdr.
pub(super) fn link_crossings(
    tree: &Tree,
    scene: &mut Scene,
    palette: &Palette,
    family: Family,
) -> Option<()> {
    let mut links = Vec::new();
    let mut found = Vec::new();
    // The lines laid out with the containers around them are drawn
    // already.
    let mut drawn = HashSet::new();
    drawn_edges(scene, &mut drawn);
    for (index, meet) in tree.meets.iter().enumerate() {
        if !meet.crossing || meet.hidden || drawn.contains(&index) {
            continue;
        }
        let edge = &tree.graph.edges[index];
        let (stroke, color) = edge_look(tree.graph, index, palette)?;
        let label = measure_edge(&edge.label, Font::new(palette.size, color), family);
        let from = tree.paths.get(&edge.from)?;
        let to = tree.paths.get(&edge.to)?;
        let (depth, ..) = across::meeting(from, to)?;
        links.push(Link {
            from: from.clone(),
            to: to.clone(),
            label: (label.width, label.height),
            shape: None,
        });
        found.push((from[..depth].to_vec(), index, label, stroke));
    }
    if links.is_empty() {
        return Some(());
    }
    let top = placements(&scene.nodes);
    let mut drawn = Drawn::default();
    drawn_in(scene, &mut drawn);
    let routed = across::link(&top, &links, &drawn);
    for ((home, index, label, stroke), linked) in found.into_iter().zip(routed) {
        let linked = linked?;
        let (mut points, label_at) = (linked.points, linked.label_at);
        let edge = &tree.graph.edges[index];
        for (id, at_start) in [(&edge.from, true), (&edge.to, false)] {
            let end = placed_at(scene, tree.paths.get(id)?)?;
            if let Some(outline) = end.kind.outline(end.rect) {
                shape::clip(&mut points, &outline, at_start);
            }
        }
        let meet = &tree.meets[index];
        scene_at(scene, &home)?.edges.push(PlacedEdge {
            from: meet.from,
            to: meet.to,
            edge: index,
            points,
            label,
            label_at,
            stroke,
            caps: caps(edge),
        });
    }
    Some(())
}

fn drawn_edges(scene: &Scene, out: &mut HashSet<usize>) {
    out.extend(scene.edges.iter().map(|edge| edge.edge));
    for node in &scene.nodes {
        drawn_edges(&node.children, out);
    }
}

/// A flowchart line between a subgraph and a node inside it. The shared
/// router treats the frame as an obstacle and would walk around the
/// outside, so this stays in the frame: from the nearest side to the
/// node's facing side, with one elbow when the title is in the way.
pub(super) fn link_inward(
    tree: &Tree,
    scene: &mut Scene,
    palette: &Palette,
    family: Family,
) -> Option<()> {
    for (index, meet) in tree.meets.iter().enumerate() {
        if !meet.inward || meet.hidden {
            continue;
        }
        let edge = &tree.graph.edges[index];
        let (stroke, color) = edge_look(tree.graph, index, palette)?;
        let label = measure_edge(&edge.label, Font::new(palette.size, color), family);
        let from = tree.paths.get(&edge.from)?;
        let to = tree.paths.get(&edge.to)?;
        let (outer, inner, into_inner) = if to.len() > from.len() && to.starts_with(from) {
            (from, to, true)
        } else if from.len() > to.len() && from.starts_with(to) {
            (to, from, false)
        } else {
            return None;
        };
        let (frame_rect, title, node_rect, outline) = {
            let frame = placed_at(scene, outer)?;
            let node = placed_at(scene, inner)?;
            (
                frame.rect,
                cluster_title(frame),
                node.rect,
                node.kind.outline(node.rect),
            )
        };
        let mut points = inward_points(frame_rect, node_rect, title);
        if !into_inner {
            points.reverse();
        }
        if let Some(outline) = outline {
            shape::clip(&mut points, &outline, !into_inner);
        }
        if points.len() < 2 {
            return None;
        }
        let label_at = label_on(&points, &label);
        let member = *outer.last()?;
        scene_at(scene, &outer[..outer.len() - 1])?
            .edges
            .push(PlacedEdge {
                from: member,
                to: usize::MAX,
                edge: index,
                points,
                label,
                label_at,
                stroke,
                caps: caps(edge),
            });
    }
    Some(())
}

pub(super) fn cluster_title(node: &Placed) -> Option<Rect> {
    if node.kind != Kind::Cluster || node.header_h <= 0.0 {
        return None;
    }
    Some(Rect {
        x: node.rect.x + (node.rect.w - node.title.width) / 2.0 - TITLE_AIR,
        y: node.rect.y + COMP_PAD - TITLE_AIR,
        w: node.title.width + 2.0 * TITLE_AIR,
        h: node.header_h + 2.0 * TITLE_AIR,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

fn inward_points(frame: Rect, node: Rect, title: Option<Rect>) -> Vec<(f32, f32)> {
    let center = (node.x + node.w / 2.0, node.y + node.h / 2.0);
    let side = closest_side(frame, center);
    let (mut from, to) = inward_ends(frame, node, side);
    if let Some(title) = title
        && cuts(from, to, title)
    {
        from = shift_off(frame, to, title, side);
    }
    elbow(from, to, side)
}

fn closest_side(frame: Rect, at: (f32, f32)) -> Side {
    let choices = [
        (Side::Left, at.0 - frame.x),
        (Side::Right, frame.x + frame.w - at.0),
        (Side::Top, at.1 - frame.y),
        (Side::Bottom, frame.y + frame.h - at.1),
    ];
    choices
        .into_iter()
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(side, _)| side)
        .unwrap_or(Side::Left)
}

fn inward_ends(frame: Rect, node: Rect, side: Side) -> ((f32, f32), (f32, f32)) {
    let (cx, cy) = (node.x + node.w / 2.0, node.y + node.h / 2.0);
    let margin = 6.0;
    match side {
        Side::Left => (
            (frame.x, clamp_in(cy, frame.y, frame.h, margin)),
            (node.x, cy),
        ),
        Side::Right => (
            (frame.x + frame.w, clamp_in(cy, frame.y, frame.h, margin)),
            (node.x + node.w, cy),
        ),
        Side::Top => (
            (clamp_in(cx, frame.x, frame.w, margin), frame.y),
            (cx, node.y),
        ),
        Side::Bottom => (
            (clamp_in(cx, frame.x, frame.w, margin), frame.y + frame.h),
            (cx, node.y + node.h),
        ),
    }
}

fn clamp_in(value: f32, start: f32, span: f32, margin: f32) -> f32 {
    let (low, high) = (start + margin, start + span - margin);
    if low > high {
        start + span / 2.0
    } else {
        value.clamp(low, high)
    }
}

/// Moves the frame end along its side until the line misses `title`.
fn shift_off(frame: Rect, node_at: (f32, f32), title: Rect, side: Side) -> (f32, f32) {
    let margin = 6.0;
    match side {
        Side::Left | Side::Right => {
            let x = if side == Side::Left {
                frame.x
            } else {
                frame.x + frame.w
            };
            let y = if node_at.1 >= title.y + title.h / 2.0 {
                title.y + title.h + 2.0
            } else {
                title.y - 2.0
            };
            (x, clamp_in(y, frame.y, frame.h, margin))
        }
        Side::Top | Side::Bottom => {
            let y = if side == Side::Top {
                frame.y
            } else {
                frame.y + frame.h
            };
            let x = if node_at.0 >= title.x + title.w / 2.0 {
                title.x + title.w + 2.0
            } else {
                title.x - 2.0
            };
            (clamp_in(x, frame.x, frame.w, margin), y)
        }
    }
}

fn elbow(from: (f32, f32), to: (f32, f32), side: Side) -> Vec<(f32, f32)> {
    let corner = match side {
        Side::Left | Side::Right => (to.0, from.1),
        Side::Top | Side::Bottom => (from.0, to.1),
    };
    if (from.0 - corner.0).hypot(from.1 - corner.1) < 0.5
        || (corner.0 - to.0).hypot(corner.1 - to.1) < 0.5
    {
        vec![from, to]
    } else {
        vec![from, corner, to]
    }
}

fn cuts(a: (f32, f32), b: (f32, f32), rect: Rect) -> bool {
    let inside = |p: (f32, f32)| {
        p.0 > rect.x && p.0 < rect.x + rect.w && p.1 > rect.y && p.1 < rect.y + rect.h
    };
    if inside(a) || inside(b) {
        return true;
    }
    let corner = [
        (rect.x, rect.y),
        (rect.x + rect.w, rect.y),
        (rect.x + rect.w, rect.y + rect.h),
        (rect.x, rect.y + rect.h),
    ];
    (0..4).any(|index| cross(a, b, corner[index], corner[(index + 1) % 4]))
}

fn cross(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
    let side = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    };
    side(a, b, c) * side(a, b, d) < 0.0 && side(c, d, a) * side(c, d, b) < 0.0
}

fn label_on(points: &[(f32, f32)], label: &Block) -> (f32, f32) {
    let mut best = (points[0], points[0]);
    let mut best_len = -1.0;
    for pair in points.windows(2) {
        let len = (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
        if len > best_len {
            best_len = len;
            best = (pair[0], pair[1]);
        }
    }
    (
        (best.0.0 + best.1.0) / 2.0 - label.width / 2.0,
        (best.0.1 + best.1.1) / 2.0 - label.height / 2.0,
    )
}

pub(super) fn placed_at<'a>(scene: &'a Scene, path: &[usize]) -> Option<&'a Placed> {
    let (&last, above) = path.split_last()?;
    let mut scene = scene;
    for &index in above {
        scene = &scene.nodes.get(index)?.children;
    }
    scene.nodes.get(last)
}

fn scene_at<'a>(scene: &'a mut Scene, path: &[usize]) -> Option<&'a mut Scene> {
    match path.split_first() {
        Some((&first, rest)) => scene_at(&mut scene.nodes.get_mut(first)?.children, rest),
        None => Some(scene),
    }
}

pub(super) fn placements(nodes: &[Placed]) -> Vec<Placement> {
    nodes
        .iter()
        .map(|node| {
            let frame = node.nested;
            let title = (frame && node.header_h > 0.0).then(|| match node.kind {
                // A subgraph's title stands alone: lines may come in past
                // it, as long as they keep off the text.
                Kind::Cluster => Rect {
                    x: node.rect.x + (node.rect.w - node.title.width) / 2.0 - TITLE_AIR,
                    y: node.rect.y + COMP_PAD - TITLE_AIR,
                    w: node.title.width + 2.0 * TITLE_AIR,
                    h: node.header_h + 2.0 * TITLE_AIR,
                },
                // A state's title sits in a band of its own, ruled off
                // below: lines come in under it.
                _ => Rect {
                    h: COMP_PAD + node.header_h,
                    ..node.rect
                },
            });
            Placement {
                rect: node.rect,
                flat: node.kind.flat(),
                frame,
                title,
                children: if node.nested {
                    placements(&node.children.nodes)
                } else {
                    Vec::new()
                },
            }
        })
        .collect()
}

/// The lines, labels, notes and region separators already drawn.
pub(super) fn drawn_in(scene: &Scene, drawn: &mut Drawn) {
    for edge in &scene.edges {
        if edge.points.len() < 2 {
            continue;
        }
        drawn.lines.push(edge.points.clone());
        if !edge.label.is_empty() {
            drawn.labels.push(Rect {
                x: edge.label_at.0,
                y: edge.label_at.1,
                w: edge.label.width,
                h: edge.label.height,
            });
        }
    }
    for node in &scene.nodes {
        drawn_in(&node.children, drawn);
    }
}
