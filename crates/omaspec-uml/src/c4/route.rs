//! Routing the relationships between the laid-out shapes: straight where
//! they face each other, around a box in the way, over a free band above
//! the row where nothing else serves.

use super::lay::{Gaps, Lane, PlacedRel, Rect, Side, chip_lines, stack_height};
use super::paint::page_ink;
use super::{Diagram, RelKind};
use crate::Declined;
use crate::text::Family;
use std::collections::HashMap;

pub(super) fn route_all(
    diagram: &Diagram,
    family: Family,
    rects: &[Rect],
    gaps: &Gaps,
    headers: &[Rect],
    lane: &Lane<'_>,
) -> Result<Vec<PlacedRel>, Declined> {
    let sides: Vec<(Side, Side)> = diagram
        .rels
        .iter()
        .map(|rel| {
            if rel.from == rel.to {
                (Side::Right, Side::Right)
            } else {
                ends(rel.kind, rects[rel.from], rects[rel.to])
            }
        })
        .collect();
    let (exit_slot, entry_slot) = slot_maps(diagram, &sides);
    let mut obstacles = Vec::with_capacity(rects.len() + headers.len());
    obstacles.extend_from_slice(rects);
    obstacles.extend_from_slice(headers);
    let mut placed = Vec::new();
    for (index, rel) in diagram.rels.iter().enumerate() {
        let points = if let Some(slot) = lane.sky.iter().position(|sky| *sky == index) {
            skyway(
                rects[rel.from],
                rects[rel.to],
                slot,
                lane.y0,
                gaps,
                &obstacles,
            )
        } else if rel.from == rel.to {
            self_route(rects[rel.from], exit_slot[index].0)
        } else if faces(rel.kind, rects[rel.from], rects[rel.to], sides[index]) {
            short_route(
                rects[rel.from],
                rects[rel.to],
                sides[index],
                exit_slot[index],
                entry_slot[index],
            )
        } else {
            tour(
                rects[rel.from],
                rects[rel.to],
                sides[index],
                exit_slot[index].0,
                gaps.lane,
            )
        };
        let ink = rel.text_color.unwrap_or_else(|| page_ink(&diagram.look));
        let lines = chip_lines(diagram, rel, family, ink)?;
        let width = lines
            .iter()
            .fold(0.0_f32, |wide, line| wide.max(line.width))
            + 12.0;
        let height = stack_height(&lines) + 8.0;
        let anchor = label_anchor(&points);
        let mut chip = Rect {
            x: anchor.0 - width / 2.0 + rel.offset_x,
            y: anchor.1 - height / 2.0 + rel.offset_y,
            w: width,
            h: height,
        };
        chip = nudge(chip, &obstacles, &placed);
        placed.push(PlacedRel {
            points,
            lines,
            chip,
            color: rel.line_color.unwrap_or(diagram.look.line),
            dashed: rel.kind == RelKind::Back,
            bi: rel.kind == RelKind::Bi,
        });
    }
    Ok(placed)
}

pub(super) type Slot = (usize, usize);

pub(super) fn slot_maps(diagram: &Diagram, sides: &[(Side, Side)]) -> (Vec<Slot>, Vec<Slot>) {
    let mut exit_count: HashMap<(usize, Side), usize> = HashMap::new();
    let mut entry_count: HashMap<(usize, Side), usize> = HashMap::new();
    for (index, rel) in diagram.rels.iter().enumerate() {
        if rel.from == rel.to {
            *exit_count.entry((rel.from, Side::Right)).or_insert(0) += 1;
        } else {
            *exit_count.entry((rel.from, sides[index].0)).or_insert(0) += 1;
            *entry_count.entry((rel.to, sides[index].1)).or_insert(0) += 1;
        }
    }
    let mut exit_seen: HashMap<(usize, Side), usize> = HashMap::new();
    let mut entry_seen: HashMap<(usize, Side), usize> = HashMap::new();
    let mut exits = Vec::new();
    let mut entries = Vec::new();
    for (index, rel) in diagram.rels.iter().enumerate() {
        if rel.from == rel.to {
            let key = (rel.from, Side::Right);
            let seen = exit_seen.entry(key).or_insert(0);
            let slot = *seen;
            *seen += 1;
            let slots = exit_count[&key];
            exits.push((slot, slots));
            entries.push((0, 1));
            continue;
        }
        let exit_key = (rel.from, sides[index].0);
        let entry_key = (rel.to, sides[index].1);
        let exit = exit_seen.entry(exit_key).or_insert(0);
        let exit_slot = *exit;
        *exit += 1;
        let entry = entry_seen.entry(entry_key).or_insert(0);
        let entry_slot = *entry;
        *entry += 1;
        exits.push((exit_slot, exit_count[&exit_key]));
        entries.push((entry_slot, entry_count[&entry_key]));
    }
    (exits, entries)
}

pub(super) fn ends(kind: RelKind, from: Rect, to: Rect) -> (Side, Side) {
    match kind {
        RelKind::Up => (Side::Top, Side::Bottom),
        RelKind::Down => (Side::Bottom, Side::Top),
        RelKind::Left => (Side::Left, Side::Right),
        RelKind::Right => (Side::Right, Side::Left),
        RelKind::Rel | RelKind::Bi | RelKind::Back => facing(from, to),
    }
}

pub(super) fn facing(from: Rect, to: Rect) -> (Side, Side) {
    let dx = to.cx() - from.cx();
    let dy = to.cy() - from.cy();
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            (Side::Right, Side::Left)
        } else {
            (Side::Left, Side::Right)
        }
    } else if dy >= 0.0 {
        (Side::Bottom, Side::Top)
    } else {
        (Side::Top, Side::Bottom)
    }
}

pub(super) fn faces(kind: RelKind, from: Rect, to: Rect, sides: (Side, Side)) -> bool {
    if !matches!(kind, RelKind::Rel | RelKind::Bi | RelKind::Back) {
        let toward = match sides.0 {
            Side::Top => to.cy() < from.cy(),
            Side::Bottom => to.cy() > from.cy(),
            Side::Left => to.cx() < from.cx(),
            Side::Right => to.cx() > from.cx(),
        };
        if !toward {
            return false;
        }
    }
    match sides {
        (Side::Right, Side::Left) => to.cx() >= from.cx(),
        (Side::Left, Side::Right) => to.cx() <= from.cx(),
        (Side::Bottom, Side::Top) => to.cy() >= from.cy(),
        (Side::Top, Side::Bottom) => to.cy() <= from.cy(),
        _ => false,
    }
}

pub(super) fn short_route(
    from: Rect,
    to: Rect,
    sides: (Side, Side),
    exit: (usize, usize),
    entry: (usize, usize),
) -> Vec<(f32, f32)> {
    let from_port = port(from, sides.0, exit.0, exit.1);
    let to_port = port(to, sides.1, entry.0, entry.1);
    let jog = match sides.0 {
        Side::Left | Side::Right => {
            let mid = (from_port.0 + to_port.0) / 2.0;
            let spread = spread(exit.0, exit.1);
            (
                clamp_between(from_port.0, to_port.0, mid + spread),
                from_port.1,
            )
        }
        Side::Top | Side::Bottom => {
            let mid = (from_port.1 + to_port.1) / 2.0;
            let spread = spread(exit.0, exit.1);
            (
                from_port.0,
                clamp_between(from_port.1, to_port.1, mid + spread),
            )
        }
    };
    let mut points = vec![from_port, step(from_port, sides.0, 12.0)];
    match sides.0 {
        Side::Left | Side::Right => {
            points.push((jog.0, points[1].1));
            points.push((jog.0, to_port.1));
        }
        Side::Top | Side::Bottom => {
            points.push((points[1].0, jog.1));
            points.push((to_port.0, jog.1));
        }
    }
    points.push(step(to_port, sides.1, 12.0));
    points.push(to_port);
    dedup(points)
}

pub(super) fn tour(
    from: Rect,
    to: Rect,
    sides: (Side, Side),
    slot: usize,
    lane: f32,
) -> Vec<(f32, f32)> {
    let out = (14.0 + slot as f32 * 8.0).min(lane);
    let from_port = port(from, sides.0, slot, slot + 1);
    let to_port = port(to, sides.1, 0, 1);
    let left = from.x.min(to.x) - out;
    let right = from.right().max(to.right()) + out;
    let top = from.y.min(to.y) - out;
    let bottom = from.bottom().max(to.bottom()) + out;
    let a = step(from_port, sides.0, out);
    let b = step(to_port, sides.1, out);
    let mut points = vec![from_port, a];
    points.push(lane_point(a, sides.0, left, right, top, bottom));
    points.push(lane_point(b, sides.1, left, right, top, bottom));
    points.push(b);
    points.push(to_port);
    dedup(points)
}

pub(super) fn lane_point(
    point: (f32, f32),
    side: Side,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
) -> (f32, f32) {
    match side {
        Side::Top => (point.0, top),
        Side::Bottom => (point.0, bottom),
        Side::Left => (left, point.1),
        Side::Right => (right, point.1),
    }
}

pub(super) fn self_route(rect: Rect, slot: usize) -> Vec<(f32, f32)> {
    let y1 = rect.y + rect.h * 0.32;
    let y2 = rect.y + rect.h * 0.68;
    let x = rect.right();
    let out = 18.0 + slot as f32 * 14.0;
    vec![(x, y1), (x + out, y1), (x + out, y2), (x, y2)]
}

pub(super) fn port(rect: Rect, side: Side, slot: usize, slots: usize) -> (f32, f32) {
    let t = if slots <= 1 {
        0.5
    } else {
        0.22 + 0.56 * (slot as f32) / (slots as f32 - 1.0)
    };
    match side {
        Side::Top => (rect.x + rect.w * t, rect.y),
        Side::Bottom => (rect.x + rect.w * t, rect.bottom()),
        Side::Left => (rect.x, rect.y + rect.h * t),
        Side::Right => (rect.right(), rect.y + rect.h * t),
    }
}

pub(super) fn step(point: (f32, f32), side: Side, dist: f32) -> (f32, f32) {
    match side {
        Side::Top => (point.0, point.1 - dist),
        Side::Bottom => (point.0, point.1 + dist),
        Side::Left => (point.0 - dist, point.1),
        Side::Right => (point.0 + dist, point.1),
    }
}

pub(super) fn spread(slot: usize, slots: usize) -> f32 {
    if slots <= 1 {
        0.0
    } else {
        (slot as f32 - (slots as f32 - 1.0) / 2.0) * 12.0
    }
}

pub(super) fn clamp_between(a: f32, b: f32, value: f32) -> f32 {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    value.clamp(lo, hi)
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

pub(super) fn skyway(
    from: Rect,
    to: Rect,
    slot: usize,
    band_y0: f32,
    gaps: &Gaps,
    obstacles: &[Rect],
) -> Vec<(f32, f32)> {
    let y = band_y0 + gaps.chip_h * 0.5 + 4.0 + slot as f32 * gaps.pitch;
    let x1 = drop_x(from, obstacles, y);
    let x2 = drop_x(to, obstacles, y);
    dedup(vec![(x1, from.y), (x1, y), (x2, y), (x2, to.y)])
}

/// An x on the top of `rect` whose drop from the overhead band misses every
/// other box and every boundary title.
pub(super) fn drop_x(rect: Rect, obstacles: &[Rect], band_y: f32) -> f32 {
    for share in [0.5_f32, 0.3, 0.7, 0.16, 0.84] {
        let x = rect.x + rect.w * share;
        let clear = obstacles
            .iter()
            .all(|obstacle| !segment_hits((x, band_y), (x, rect.y), *obstacle));
        if clear {
            return x;
        }
    }
    rect.cx()
}

pub(super) fn route_hits(
    points: &[(f32, f32)],
    ends: (usize, usize),
    rects: &[Rect],
    headers: &[Rect],
) -> bool {
    for pair in points.windows(2) {
        for (index, rect) in rects.iter().enumerate() {
            if index != ends.0 && index != ends.1 && segment_hits(pair[0], pair[1], *rect) {
                return true;
            }
        }
        if headers
            .iter()
            .any(|rect| segment_hits(pair[0], pair[1], *rect))
        {
            return true;
        }
    }
    false
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
        } else {
            if t < t0 {
                return false;
            }
            if t < t1 {
                t1 = t;
            }
        }
    }
    true
}

pub(super) fn label_anchor(points: &[(f32, f32)]) -> (f32, f32) {
    let mut best = 0.0_f32;
    let mut at = points.first().copied().unwrap_or((0.0, 0.0));
    for pair in points.windows(2) {
        let len = ((pair[1].0 - pair[0].0).powi(2) + (pair[1].1 - pair[0].1).powi(2)).sqrt();
        if len > best {
            best = len;
            at = ((pair[0].0 + pair[1].0) / 2.0, (pair[0].1 + pair[1].1) / 2.0);
        }
    }
    at
}

pub(super) fn nudge(chip: Rect, obstacles: &[Rect], placed: &[PlacedRel]) -> Rect {
    let fits = |chip: Rect| {
        obstacles
            .iter()
            .all(|obstacle| !chip.inflate(2.0).intersects(*obstacle))
            && placed
                .iter()
                .all(|rel| !chip.inflate(2.0).intersects(rel.chip))
    };
    if fits(chip) {
        return chip;
    }
    for ring in 1..18 {
        let distance = ring as f32 * 6.0;
        let shifts = [
            (0.0, -distance),
            (0.0, distance),
            (distance, 0.0),
            (-distance, 0.0),
            (distance, -distance),
            (-distance, -distance),
            (distance, distance),
            (-distance, distance),
        ];
        for (dx, dy) in shifts {
            let moved = Rect {
                x: chip.x + dx,
                y: chip.y + dy,
                w: chip.w,
                h: chip.h,
            };
            if fits(moved) {
                return moved;
            }
        }
    }
    chip
}
