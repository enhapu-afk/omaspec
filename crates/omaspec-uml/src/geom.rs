//! Small geometry shared by drawers with ways of their own (C4,
//! architecture): an end of a line pulled back along it, for its head.

/// `points` with its last end (`at_end`) or its first pulled `len` back
/// along its last or first step, as far as that step goes.
pub(crate) fn shorten(points: &mut [(f32, f32)], len: f32, at_end: bool) {
    if points.len() < 2 {
        return;
    }
    if at_end {
        let last = points.len() - 1;
        let from = points[last - 1];
        pull(&mut points[last], from, len);
    } else {
        let toward = points[1];
        pull(&mut points[0], toward, len);
    }
}

/// `point` moved `len` towards `toward`, at most onto it.
pub(crate) fn pull(point: &mut (f32, f32), toward: (f32, f32), len: f32) {
    let dx = toward.0 - point.0;
    let dy = toward.1 - point.1;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 0.1 {
        return;
    }
    let step = len.min(dist);
    point.0 += dx / dist * step;
    point.1 += dy / dist * step;
}
