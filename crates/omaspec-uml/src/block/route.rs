//! Routing the edges between the laid-out blocks: the shortest right-angled
//! path past the blocks in the way, along a free lane, with its label.

use super::lay::{Laid, Rect, measure};
use super::{Diagram, End};
use crate::Declined;
use crate::text::Family;

pub(super) fn route(diagram: &Diagram, family: Family, laid: &mut Laid) -> Result<(), Declined> {
    let mut reserved = Vec::new();
    if let Some(rect) = laid.title {
        reserved.push(rect);
    }
    for (index, group) in laid.groups.iter().enumerate() {
        let band = laid.bands[index];
        if band > 0.0 {
            reserved.push(Rect {
                x: group.x,
                y: group.y,
                w: group.w,
                h: band,
            });
        }
    }
    for edge in &diagram.edges {
        let from = end_rect(edge.from, laid);
        let to = end_rect(edge.to, laid);
        let mut hard = Vec::new();
        if let End::Node(index) = edge.from {
            hard.push(laid.nodes[index]);
        }
        if let End::Node(index) = edge.to {
            hard.push(laid.nodes[index]);
        }
        let mut soft = Vec::new();
        for (index, node) in laid.nodes.iter().enumerate() {
            let own = matches!(edge.from, End::Node(n) if n == index)
                || matches!(edge.to, End::Node(n) if n == index);
            if !own {
                soft.push(node.inflate(6.0));
            }
        }
        for rect in reserved.iter() {
            soft.push(rect.inflate(2.0));
        }
        let path = choose_path(from, to, &soft, &hard);
        let label = match &edge.label {
            Some(text) => {
                let block = measure(text, diagram, family, false)?;
                let mut blocked = soft.clone();
                blocked.extend(hard.iter().copied());
                Some(place_label(&path, block.width, block.height, &blocked))
            }
            None => None,
        };
        for window in path.windows(2) {
            reserved.push(corridor(window[0], window[1]));
        }
        if let Some(rect) = label {
            reserved.push(rect);
        }
        laid.edges.push(path);
        laid.edge_labels.push(label);
    }
    Ok(())
}

pub(super) fn end_rect(end: End, laid: &Laid) -> Rect {
    match end {
        End::Node(index) => laid.nodes[index],
        End::Group(index) => laid.groups[index],
    }
}

pub(super) fn choose_path(from: Rect, to: Rect, soft: &[Rect], hard: &[Rect]) -> Vec<(f32, f32)> {
    let start = exit_point(from, to);
    let end = exit_point(to, from);
    let mut best: Option<(f32, Vec<(f32, f32)>)> = None;
    let mut offer = |points: Vec<(f32, f32)>| {
        let path = clean(points);
        if path.len() < 2 || !path_clear(&path, soft, hard) {
            return;
        }
        let len = path_len(&path);
        if best.as_ref().is_none_or(|(have, _)| len + 0.05 < *have) {
            best = Some((len, path));
        }
    };
    if (start.1 - end.1).abs() < 0.8 || (start.0 - end.0).abs() < 0.8 {
        offer(vec![start, end]);
    }
    for x in lanes(start.0, end.0, soft, true) {
        offer(vec![start, (x, start.1), (x, end.1), end]);
    }
    for y in lanes(start.1, end.1, soft, false) {
        offer(vec![start, (start.0, y), (end.0, y), end]);
    }
    // Leave each side at a right angle, then use a lane. A one-bend path
    // can only do that by travelling along the side it just left.
    let leave = jog(from, start);
    let arrive = jog(to, end);
    for y in lanes(leave.1, arrive.1, soft, false) {
        offer(vec![start, leave, (leave.0, y), (arrive.0, y), arrive, end]);
    }
    for x in lanes(leave.0, arrive.0, soft, true) {
        offer(vec![start, leave, (x, leave.1), (x, arrive.1), arrive, end]);
    }
    best.map(|(_, path)| path).unwrap_or_else(|| {
        clean(vec![
            start,
            ((start.0 + end.0) / 2.0, start.1),
            ((start.0 + end.0) / 2.0, end.1),
            end,
        ])
    })
}

pub(super) fn jog(rect: Rect, port: (f32, f32)) -> (f32, f32) {
    const STEP: f32 = 12.0;
    if (port.0 - rect.x).abs() <= 0.2 || (port.0 - rect.right()).abs() <= 0.2 {
        let sign = if port.0 >= rect.cx() { 1.0 } else { -1.0 };
        (port.0 + sign * STEP, port.1)
    } else {
        let sign = if port.1 >= rect.cy() { 1.0 } else { -1.0 };
        (port.0, port.1 + sign * STEP)
    }
}

pub(super) fn exit_point(rect: Rect, other: Rect) -> (f32, f32) {
    let dx = other.cx() - rect.cx();
    let dy = other.cy() - rect.cy();
    let margin = 8.0;
    if dx.abs() >= dy.abs() {
        let x = if dx >= 0.0 { rect.right() } else { rect.x };
        let y = rect.cy().clamp(
            rect.y + margin,
            (rect.bottom() - margin).max(rect.y + margin),
        );
        (x, y)
    } else {
        let y = if dy >= 0.0 { rect.bottom() } else { rect.y };
        let x = rect.cx().clamp(
            rect.x + margin,
            (rect.right() - margin).max(rect.x + margin),
        );
        (x, y)
    }
}

pub(super) fn lanes(a: f32, b: f32, soft: &[Rect], vertical: bool) -> Vec<f32> {
    let mid = (a + b) / 2.0;
    let mut values = vec![mid];
    for step in [12.0, 24.0, 36.0, 52.0, 72.0] {
        values.push(mid - step);
        values.push(mid + step);
    }
    for rect in soft {
        if vertical {
            values.push(rect.x - 8.0);
            values.push(rect.right() + 8.0);
        } else {
            values.push(rect.y - 8.0);
            values.push(rect.bottom() + 8.0);
        }
    }
    values.sort_by(|left, right| (left - mid).abs().total_cmp(&(right - mid).abs()));
    let mut kept = Vec::new();
    for value in values {
        if kept.iter().any(|have: &f32| (have - value).abs() < 0.8) {
            continue;
        }
        kept.push(value);
        if kept.len() == 16 {
            break;
        }
    }
    kept
}

pub(super) fn path_clear(path: &[(f32, f32)], soft: &[Rect], hard: &[Rect]) -> bool {
    path.windows(2).all(|window| {
        let misses_soft = soft
            .iter()
            .all(|rect| !segment_hits(window[0], window[1], *rect));
        let misses_hard = hard.iter().all(|rect| {
            !segment_hits(window[0], window[1], *rect)
                && !segment_rides(window[0], window[1], *rect)
        });
        misses_soft && misses_hard
    })
}

pub(super) fn segment_hits(a: (f32, f32), b: (f32, f32), rect: Rect) -> bool {
    if (a.1 - b.1).abs() <= 0.05 {
        let (x1, x2) = if a.0 < b.0 { (a.0, b.0) } else { (b.0, a.0) };
        a.1 > rect.y && a.1 < rect.bottom() && x2 > rect.x && x1 < rect.right()
    } else if (a.0 - b.0).abs() <= 0.05 {
        let (y1, y2) = if a.1 < b.1 { (a.1, b.1) } else { (b.1, a.1) };
        a.0 > rect.x && a.0 < rect.right() && y2 > rect.y && y1 < rect.bottom()
    } else {
        true
    }
}

/// A segment that travels along a side, rather than meeting it.
pub(super) fn segment_rides(a: (f32, f32), b: (f32, f32), rect: Rect) -> bool {
    if (a.1 - b.1).abs() <= 0.05 {
        let on = (a.1 - rect.y).abs() <= 0.2 || (a.1 - rect.bottom()).abs() <= 0.2;
        if on {
            let (x1, x2) = if a.0 < b.0 { (a.0, b.0) } else { (b.0, a.0) };
            return x2.min(rect.right()) - x1.max(rect.x) > 1.0;
        }
    } else if (a.0 - b.0).abs() <= 0.05 {
        let on = (a.0 - rect.x).abs() <= 0.2 || (a.0 - rect.right()).abs() <= 0.2;
        if on {
            let (y1, y2) = if a.1 < b.1 { (a.1, b.1) } else { (b.1, a.1) };
            return y2.min(rect.bottom()) - y1.max(rect.y) > 1.0;
        }
    }
    false
}

pub(super) fn path_len(path: &[(f32, f32)]) -> f32 {
    path.windows(2)
        .map(|window| (window[0].0 - window[1].0).hypot(window[0].1 - window[1].1))
        .sum()
}

pub(super) fn clean(path: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut points = Vec::new();
    for point in path {
        if points.last().is_some_and(|have: &(f32, f32)| {
            (have.0 - point.0).abs() < 0.05 && (have.1 - point.1).abs() < 0.05
        }) {
            continue;
        }
        points.push(point);
    }
    if points.len() < 3 {
        return points;
    }
    let mut simple = vec![points[0]];
    for index in 1..points.len() - 1 {
        let a = simple[simple.len() - 1];
        let b = points[index];
        let c = points[index + 1];
        let same_x = (a.0 - b.0).abs() < 0.05 && (b.0 - c.0).abs() < 0.05;
        let same_y = (a.1 - b.1).abs() < 0.05 && (b.1 - c.1).abs() < 0.05;
        if !(same_x || same_y) {
            simple.push(b);
        }
    }
    simple.push(points[points.len() - 1]);
    simple
}

pub(super) fn corridor(a: (f32, f32), b: (f32, f32)) -> Rect {
    if (a.1 - b.1).abs() <= (a.0 - b.0).abs() {
        Rect {
            x: a.0.min(b.0),
            y: a.1 - 5.0,
            w: (a.0 - b.0).abs().max(1.0),
            h: 10.0,
        }
    } else {
        Rect {
            x: a.0 - 5.0,
            y: a.1.min(b.1),
            w: 10.0,
            h: (a.1 - b.1).abs().max(1.0),
        }
    }
}

pub(super) fn place_label(path: &[(f32, f32)], w: f32, h: f32, blocked: &[Rect]) -> Rect {
    let Some((a, b)) = longest(path) else {
        return Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
        };
    };
    let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let horizontal = (a.1 - b.1).abs() <= (a.0 - b.0).abs();
    let span = if horizontal { b.0 - a.0 } else { b.1 - a.1 };
    let mut fallback = Rect {
        x: mid.0 - w / 2.0,
        y: mid.1 - h - 4.0,
        w,
        h,
    };
    for sign in [1.0_f32, -1.0] {
        for slide in [0.0, 0.28, -0.28, 0.5, -0.5] {
            let along = slide * span;
            let rect = if horizontal {
                Rect {
                    x: mid.0 + along - w / 2.0,
                    y: if sign > 0.0 {
                        mid.1 - 4.0 - h
                    } else {
                        mid.1 + 4.0
                    },
                    w,
                    h,
                }
            } else {
                let left = if sign > 0.0 {
                    mid.0 + 6.0
                } else {
                    mid.0 - 6.0 - w
                };
                Rect {
                    x: left,
                    y: mid.1 + along - h / 2.0,
                    w,
                    h,
                }
            };
            fallback = rect;
            if blocked.iter().all(|have| !rect.intersects(*have)) {
                return rect;
            }
        }
    }
    fallback
}

pub(super) fn longest(path: &[(f32, f32)]) -> Option<((f32, f32), (f32, f32))> {
    path.windows(2)
        .max_by(|left, right| {
            let len = |pair: &[(f32, f32)]| (pair[0].0 - pair[1].0).hypot(pair[0].1 - pair[1].1);
            len(left).total_cmp(&len(right))
        })
        .map(|window| (window[0], window[1]))
}
