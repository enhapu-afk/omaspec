//! Routing the edges between the laid-out services: from the side each
//! names to the side of the other, around the tiles in the way.

use super::lay::{PlacedEdge, PlacedGroup, PlacedNode, Rect, side_horizontal};
use super::{Diagram, Side};
use std::collections::HashMap;

pub(super) fn route_all(
    diagram: &Diagram,
    nodes: &[PlacedNode],
    groups: &[PlacedGroup],
) -> Vec<PlacedEdge> {
    let mut used: HashMap<(usize, Side, bool), usize> = HashMap::new();
    for edge in &diagram.edges {
        *used
            .entry((edge.from, edge.from_port, edge.from_group))
            .or_insert(0) += 1;
        *used
            .entry((edge.to, edge.to_port, edge.to_group))
            .or_insert(0) += 1;
    }
    let mut seen: HashMap<(usize, Side, bool), usize> = HashMap::new();
    let obstacles = obstacles(nodes, groups);
    let mut placed = Vec::new();
    for edge in &diagram.edges {
        let from_key = (edge.from, edge.from_port, edge.from_group);
        let to_key = (edge.to, edge.to_port, edge.to_group);
        let from_slot = *seen.entry(from_key).or_insert(0);
        seen.insert(from_key, from_slot + 1);
        let to_slot = *seen.entry(to_key).or_insert(0);
        seen.insert(to_key, to_slot + 1);
        let start = anchor(
            diagram,
            nodes,
            groups,
            edge.from,
            edge.from_port,
            edge.from_group,
            (from_slot, used[&from_key]),
        );
        let end = anchor(
            diagram,
            nodes,
            groups,
            edge.to,
            edge.to_port,
            edge.to_group,
            (to_slot, used[&to_key]),
        );
        placed.push(PlacedEdge {
            points: route(start, edge.from_port, end, edge.to_port, &obstacles),
            arrow_start: edge.arrow_start,
            arrow_end: edge.arrow_end,
        });
    }
    placed
}

pub(super) fn obstacles(nodes: &[PlacedNode], groups: &[PlacedGroup]) -> Vec<Rect> {
    let mut rects = Vec::new();
    for node in nodes {
        rects.push(node.tile);
        if let Some(label) = node.label_rect {
            rects.push(label);
        }
    }
    for group in groups {
        rects.push(group.label_rect);
        rects.push(Rect {
            x: group.rect.x + 8.0,
            y: group.rect.y + 8.0,
            w: 22.0,
            h: 22.0,
        });
    }
    rects
}

pub(super) fn anchor(
    diagram: &Diagram,
    nodes: &[PlacedNode],
    groups: &[PlacedGroup],
    node: usize,
    side: Side,
    on_group: bool,
    spread: (usize, usize),
) -> (f32, f32) {
    if on_group && let Some(parent) = diagram.nodes[node].parent {
        return group_anchor(groups[parent].rect, nodes[node].tile, side);
    }
    port_on(&nodes[node], side, spread.0, spread.1)
}

pub(super) fn group_anchor(group: Rect, service: Rect, side: Side) -> (f32, f32) {
    let y = service.cy().clamp(group.y + 28.0, group.bottom() - 10.0);
    let x = service.cx().clamp(group.x + 28.0, group.right() - 10.0);
    match side {
        Side::Left => (group.x, y),
        Side::Right => (group.right(), y),
        Side::Top => (x, group.y),
        Side::Bottom => (x, group.bottom()),
    }
}

pub(super) fn port_on(node: &PlacedNode, side: Side, slot: usize, slots: usize) -> (f32, f32) {
    let share = if slots <= 1 {
        0.5
    } else {
        0.32 + 0.36 * (slot as f32) / (slots as f32 - 1.0)
    };
    let tile = node.tile;
    match side {
        Side::Top => (tile.x + tile.w * share, tile.y),
        Side::Left => (tile.x, tile.y + tile.h * share),
        Side::Right => (tile.right(), tile.y + tile.h * share),
        Side::Bottom => {
            let bottom = node
                .label_rect
                .map(|label| label.bottom())
                .unwrap_or_else(|| tile.bottom());
            (tile.x + tile.w * share, bottom)
        }
    }
}

pub(super) fn route(
    start: (f32, f32),
    start_side: Side,
    end: (f32, f32),
    end_side: Side,
    obstacles: &[Rect],
) -> Vec<(f32, f32)> {
    let direct = elbow(start, end, start_side, end_side);
    if !hits(&direct, obstacles) {
        return direct;
    }
    let mut best: Option<(f32, Vec<(f32, f32)>)> = None;
    let mut keep = |points: Vec<(f32, f32)>| {
        if hits(&points, obstacles) {
            return;
        }
        let len = path_len(&points);
        if best.as_ref().is_none_or(|(current, _)| len < *current) {
            best = Some((len, points));
        }
    };
    if !obstacles.is_empty() {
        let mut left = f32::MAX;
        let mut right = f32::MIN;
        let mut top = f32::MAX;
        let mut bottom = f32::MIN;
        for rect in obstacles {
            left = left.min(rect.x);
            right = right.max(rect.right());
            top = top.min(rect.y);
            bottom = bottom.max(rect.bottom());
        }
        keep(lane(start, start_side, end, end_side, top - 18.0, true));
        keep(lane(start, start_side, end, end_side, bottom + 18.0, true));
        keep(lane(start, start_side, end, end_side, left - 18.0, false));
        keep(lane(start, start_side, end, end_side, right + 18.0, false));
    }
    // Every obstacle, not only the one the straight elbow hits: the free
    // gap is often just past a label the elbow never touched.
    for rect in obstacles {
        keep(lane(start, start_side, end, end_side, rect.y - 14.0, true));
        keep(lane(
            start,
            start_side,
            end,
            end_side,
            rect.bottom() + 14.0,
            true,
        ));
        keep(lane(start, start_side, end, end_side, rect.x - 14.0, false));
        keep(lane(
            start,
            start_side,
            end,
            end_side,
            rect.right() + 14.0,
            false,
        ));
    }
    best.map(|(_, points)| points).unwrap_or(direct)
}

pub(super) fn path_len(points: &[(f32, f32)]) -> f32 {
    points
        .windows(2)
        .map(|pair| {
            let dx = pair[1].0 - pair[0].0;
            let dy = pair[1].1 - pair[0].1;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

pub(super) fn lane(
    start: (f32, f32),
    start_side: Side,
    end: (f32, f32),
    end_side: Side,
    at: f32,
    horizontal: bool,
) -> Vec<(f32, f32)> {
    let start_out = depart(start, start_side, 14.0);
    let end_out = depart(end, end_side, 14.0);
    let bend = if horizontal {
        vec![(start_out.0, at), (end_out.0, at)]
    } else {
        vec![(at, start_out.1), (at, end_out.1)]
    };
    let mut points = vec![start, start_out];
    points.extend(bend);
    points.push(end_out);
    points.push(end);
    dedup(points)
}

pub(super) fn elbow(
    start: (f32, f32),
    end: (f32, f32),
    start_side: Side,
    end_side: Side,
) -> Vec<(f32, f32)> {
    let mut points = vec![start];
    match (side_horizontal(start_side), side_horizontal(end_side)) {
        (true, true) if (start.1 - end.1).abs() > 0.5 => {
            let mid = (start.0 + end.0) / 2.0;
            points.push((mid, start.1));
            points.push((mid, end.1));
        }
        (false, false) if (start.0 - end.0).abs() > 0.5 => {
            let mid = (start.1 + end.1) / 2.0;
            points.push((start.0, mid));
            points.push((end.0, mid));
        }
        (true, false) => points.push((end.0, start.1)),
        (false, true) => points.push((start.0, end.1)),
        _ => {}
    }
    points.push(end);
    dedup(points)
}

pub(super) fn depart(point: (f32, f32), side: Side, dist: f32) -> (f32, f32) {
    match side {
        Side::Left => (point.0 - dist, point.1),
        Side::Right => (point.0 + dist, point.1),
        Side::Top => (point.0, point.1 - dist),
        Side::Bottom => (point.0, point.1 + dist),
    }
}

pub(super) fn hits(points: &[(f32, f32)], obstacles: &[Rect]) -> bool {
    points.windows(2).any(|pair| {
        obstacles
            .iter()
            .any(|rect| segment_hits(pair[0], pair[1], *rect))
    })
}

pub(super) fn segment_hits(start: (f32, f32), end: (f32, f32), rect: Rect) -> bool {
    let left = rect.x + 1.5;
    let right = rect.right() - 1.5;
    let top = rect.y + 1.5;
    let bottom = rect.bottom() - 1.5;
    if right <= left || bottom <= top {
        return false;
    }
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    let p = [-dx, dx, -dy, dy];
    let q = [
        start.0 - left,
        right - start.0,
        start.1 - top,
        bottom - start.1,
    ];
    for index in 0..4 {
        if p[index].abs() < 1.0e-4 {
            if q[index] < 0.0 {
                return false;
            }
            continue;
        }
        let t = q[index] / p[index];
        if p[index] < 0.0 {
            if t > t1 {
                return false;
            }
            if t > t0 {
                t0 = t;
            }
        } else if t < t0 {
            return false;
        } else if t < t1 {
            t1 = t;
        }
    }
    true
}

pub(super) fn dedup(points: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for point in points {
        if out.last().is_none_or(|prev: &(f32, f32)| {
            (prev.0 - point.0).abs() > 0.4 || (prev.1 - point.1).abs() > 0.4
        }) {
            out.push(point);
        }
    }
    out
}
