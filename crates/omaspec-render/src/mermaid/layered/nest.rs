//! Concurrent regions, and transitions between containers.
//!
//! The regions of a state are laid out one by one and set side by side
//! across the flow, a dashed line between two, as Mermaid does. A
//! transition into a container laid out on its own ([`super::open`]) is
//! ranked in the container that holds both ends, as if it joined the boxes
//! there, and routed around everything once the whole diagram is placed.

use std::collections::HashSet;

use mermaid_rs_renderer::{DiagramKind, Graph, NodeShape};
use omaspec_uml::graph::across::{self, Drawn, Link, Placement};
use omaspec_uml::graph::{self, Rect};
use omaspec_uml::{Block, Family, Font, Stroke};

use super::shape;
use super::{
    COMP_PAD, Kind, Member, Palette, Placed, PlacedEdge, Scene, Spot, Tree, caps, edge_look,
    layout_spot, measure_edge, parents,
};

/// Between a region's states and the dashed line.
const REGION_GAP: f32 = 14.0;
/// Around a subgraph's title, which lines keep off.
pub(super) const TITLE_AIR: f32 = 4.0;

pub(super) fn is_region(graph: &Graph, index: usize) -> bool {
    graph.subgraphs[index]
        .id
        .as_deref()
        .is_some_and(|id| id.starts_with("__region_"))
}

/// mmdr gives a nested state one start and one end, shared by its
/// concurrent regions, and draws a shared start with several transitions
/// as a fork bar. Mermaid gives every region its own. A copy of `graph`
/// where each region that uses one has its own; `None` when nothing
/// changes.
pub(super) fn own_markers(graph: &Graph) -> Option<Graph> {
    if graph.kind != DiagramKind::State {
        return None;
    }
    let regions: Vec<usize> = (0..graph.subgraphs.len())
        .filter(|&index| is_region(graph, index))
        .collect();
    if regions.is_empty() {
        return None;
    }
    let parent = parents(graph, false);
    let sets: Vec<HashSet<&str>> = graph
        .subgraphs
        .iter()
        .map(|sub| sub.nodes.iter().map(String::as_str).collect())
        .collect();
    let mut copy = graph.clone();
    let mut touched = Vec::new();
    for &region in &regions {
        let Some(scope) = parent[region].and_then(|p| graph.subgraphs[p].id.as_deref()) else {
            continue;
        };
        for marker in [format!("__start_{scope}__"), format!("__end_{scope}__")] {
            if !graph.nodes.contains_key(&marker) || sets[region].contains(marker.as_str()) {
                continue;
            }
            let own = format!("{marker}{region}");
            let mut used = false;
            for edge in &mut copy.edges {
                let end = if edge.from == marker && sets[region].contains(edge.to.as_str()) {
                    &mut edge.from
                } else if edge.to == marker && sets[region].contains(edge.from.as_str()) {
                    &mut edge.to
                } else {
                    continue;
                };
                end.clone_from(&own);
                used = true;
            }
            if !used {
                continue;
            }
            let mut node = graph.nodes[&marker].clone();
            node.id.clone_from(&own);
            copy.nodes.insert(own.clone(), node);
            let order = graph.node_order.get(&marker).copied().unwrap_or(usize::MAX);
            copy.node_order.insert(own.clone(), order);
            // Listed wherever the region is, so it still nests the same way.
            for (index, set) in sets.iter().enumerate() {
                if index == region || sets[region].is_subset(set) {
                    copy.subgraphs[index].nodes.push(own.clone());
                }
            }
            touched.push(marker);
            touched.push(own);
        }
    }
    if touched.is_empty() {
        return None;
    }
    // mmdr's rule, now per region: a start with several transitions out,
    // or an end with several in, is a bar.
    for id in touched {
        let start = id.starts_with("__start_");
        let count = copy
            .edges
            .iter()
            .filter(|edge| {
                if start {
                    edge.from == id
                } else {
                    edge.to == id
                }
            })
            .count();
        if let Some(node) = copy.nodes.get_mut(&id) {
            node.shape = match (count > 1, start) {
                (true, _) => NodeShape::ForkJoin,
                (false, true) => NodeShape::Circle,
                (false, false) => NodeShape::DoubleCircle,
            };
        }
    }
    Some(copy)
}

/// The regions of a split state, each laid out on its own and set across
/// the flow: side by side when it runs down, one under the other when it
/// runs across.
pub(super) fn layout_regions(
    tree: &Tree,
    members: &[Member],
    axis: graph::Direction,
    palette: &Palette,
    family: Family,
) -> Option<Scene> {
    let mut scenes = Vec::with_capacity(members.len());
    for member in members {
        let Member::Group(index) = member else {
            return None;
        };
        scenes.push(layout_spot(tree, Spot::In(*index), palette, family)?);
    }
    let beside = axis == graph::Direction::Down;
    let across = scenes
        .iter()
        .map(|scene| if beside { scene.height } else { scene.width })
        .fold(0.0, f32::max);
    let last = scenes.len().saturating_sub(1);
    let mut nodes = Vec::with_capacity(scenes.len());
    let mut cursor = 0.0;
    for (index, mut scene) in scenes.into_iter().enumerate() {
        let before = if index == 0 { 0.0 } else { REGION_GAP };
        let after = if index == last { 0.0 } else { REGION_GAP };
        let (along, rect) = if beside {
            let along = scene.width;
            scene.offset(cursor + before, (across - scene.height) / 2.0);
            let rect = Rect {
                x: cursor,
                y: 0.0,
                w: before + along + after,
                h: across,
            };
            (along, rect)
        } else {
            let along = scene.height;
            scene.offset((across - scene.width) / 2.0, cursor + before);
            let rect = Rect {
                x: 0.0,
                y: cursor,
                w: across,
                h: before + along + after,
            };
            (along, rect)
        };
        cursor += before + along + after;
        nodes.push(Placed {
            rect,
            kind: Kind::Region,
            fill: None,
            border: Stroke::new(palette.cluster_border, 1.0),
            mark: palette.cluster_border,
            title: Block::empty(),
            description: Vec::new(),
            header_h: 0.0,
            nested: true,
            opened: false,
            children: scene,
        });
    }
    let (width, height) = if beside {
        (cursor, across)
    } else {
        (across, cursor)
    };
    Some(Scene {
        nodes,
        edges: Vec::new(),
        notes: Vec::new(),
        width,
        height,
    })
}

/// Stretches the regions of a split state to its border and up to its
/// title, so each owns its whole part of the box.
pub(super) fn fill_regions(children: &mut Scene, rect: Rect, top: f32) {
    let regions = &mut children.nodes;
    if regions.first().is_none_or(|node| node.kind != Kind::Region) {
        return;
    }
    let stacked =
        regions.len() < 2 || regions[1].rect.y >= regions[0].rect.y + regions[0].rect.h - 0.5;
    let last = regions.len() - 1;
    for (index, region) in regions.iter_mut().enumerate() {
        let r = &mut region.rect;
        if stacked {
            r.x = rect.x;
            r.w = rect.w;
        } else {
            r.y = top;
            r.h = rect.y + rect.h - top;
        }
        let (mut start, mut end) = if stacked {
            (r.y, r.y + r.h)
        } else {
            (r.x, r.x + r.w)
        };
        if index == 0 {
            start = if stacked { top } else { rect.x };
        }
        if index == last {
            end = if stacked {
                rect.y + rect.h
            } else {
                rect.x + rect.w
            };
        }
        if stacked {
            r.y = start;
            r.h = end - start;
        } else {
            r.x = start;
            r.w = end - start;
        }
    }
}

/// The dashed lines between the concurrent regions of `node`, border to
/// border.
pub(super) fn separators(node: &Placed) -> Vec<((f32, f32), (f32, f32))> {
    node.children
        .nodes
        .windows(2)
        .filter(|pair| pair[1].kind == Kind::Region)
        .map(|pair| {
            let (a, b) = (pair[0].rect, pair[1].rect);
            if b.y >= a.y + a.h - 0.5 {
                ((node.rect.x, b.y), (node.rect.x + node.rect.w, b.y))
            } else {
                ((b.x, b.y), (b.x, b.y + b.h))
            }
        })
        .collect()
}

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
        let label = measure_edge(tree, &edge.label, Font::new(palette.size, color), family);
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
    if !tree.flowchart {
        return Some(());
    }
    for (index, meet) in tree.meets.iter().enumerate() {
        if !meet.inward || meet.hidden {
            continue;
        }
        let edge = &tree.graph.edges[index];
        let (stroke, color) = edge_look(tree.graph, index, palette)?;
        let label = measure_edge(tree, &edge.label, Font::new(palette.size, color), family);
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
            let frame = node.nested && node.kind != Kind::Region;
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
    for note in &scene.notes {
        drawn.notes.push(Rect {
            x: note.x,
            y: note.y,
            w: note.w,
            h: note.h,
        });
    }
    for node in &scene.nodes {
        for (a, b) in separators(node) {
            drawn.lines.push(vec![a, b]);
        }
        drawn_in(&node.children, drawn);
    }
}
