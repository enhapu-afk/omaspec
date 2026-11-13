//! Routing the relations between the laid-out entities: from the facing
//! sides, each end pulled back for its crow's foot.

use super::lay::Rect;

pub(super) fn route(
    from: Rect,
    to: Rect,
    from_slot: usize,
    from_slots: usize,
    to_slot: usize,
    to_slots: usize,
) -> Vec<(f32, f32)> {
    let start = port(from, to, from_slot, from_slots);
    let end = port(to, from, to_slot, to_slots);
    if (from.cx() - to.cx()).abs() >= (from.cy() - to.cy()).abs() {
        if (start.1 - end.1).abs() < 0.5 {
            return vec![start, end];
        }
        // The long run stays on the source port. The jog sits near the
        // target, and each relationship jogs at its own x.
        let sign = if end.0 >= start.0 { 1.0 } else { -1.0 };
        let span = (end.0 - start.0).abs();
        let inset = (22.0 + to_slot as f32 * 10.0).min(span * 0.35).max(16.0);
        let jog = end.0 - sign * inset;
        vec![start, (jog, start.1), (jog, end.1), end]
    } else if (start.0 - end.0).abs() < 0.5 {
        vec![start, end]
    } else {
        let sign = if end.1 >= start.1 { 1.0 } else { -1.0 };
        let span = (end.1 - start.1).abs();
        let inset = (22.0 + to_slot as f32 * 10.0).min(span * 0.35).max(16.0);
        let jog = end.1 - sign * inset;
        vec![start, (start.0, jog), (end.0, jog), end]
    }
}

pub(super) fn port(from: Rect, to: Rect, slot: usize, slots: usize) -> (f32, f32) {
    // The middle of the side, or spread through it when several lines share it.
    let t = if slots <= 1 {
        0.5
    } else {
        0.18 + 0.64 * (slot as f32 / (slots as f32 - 1.0))
    };
    if (to.cx() - from.cx()).abs() >= (to.cy() - from.cy()).abs() {
        let x = if to.cx() >= from.cx() {
            from.right()
        } else {
            from.x
        };
        (x, from.y + t * from.h)
    } else {
        let y = if to.cy() >= from.cy() {
            from.bottom()
        } else {
            from.y
        };
        (from.x + t * from.w, y)
    }
}

pub(super) fn outward(rect: Rect, port: (f32, f32)) -> (f32, f32) {
    let left = (port.0 - rect.x).abs();
    let right = (port.0 - rect.right()).abs();
    let top = (port.1 - rect.y).abs();
    let bottom = (port.1 - rect.bottom()).abs();
    let nearest = left.min(right).min(top).min(bottom);
    if nearest == left {
        (-1.0, 0.0)
    } else if nearest == right {
        (1.0, 0.0)
    } else if nearest == top {
        (0.0, -1.0)
    } else {
        (0.0, 1.0)
    }
}

pub(super) fn shorten(points: &mut [(f32, f32)], len: f32) {
    let Some(last) = points.len().checked_sub(1) else {
        return;
    };
    if last == 0 || len <= 0.0 {
        return;
    }
    let toward = points[last - 1];
    pull_toward(&mut points[last], toward, len);
}

pub(super) fn shorten_first(points: &mut [(f32, f32)], len: f32) {
    if points.len() < 2 || len <= 0.0 {
        return;
    }
    let toward = points[1];
    pull_toward(&mut points[0], toward, len);
}

pub(super) fn pull_toward(point: &mut (f32, f32), toward: (f32, f32), len: f32) {
    let dx = toward.0 - point.0;
    let dy = toward.1 - point.1;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 0.01 {
        return;
    }
    let t = (len / dist).min(0.85);
    point.0 += dx * t;
    point.1 += dy * t;
}
