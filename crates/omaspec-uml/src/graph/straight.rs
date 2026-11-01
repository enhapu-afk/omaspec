//! Lines without small steps. Where a line leaves one box and enters the
//! next a few units further across, its route steps aside by that much
//! (a jog), and a run through the ranks can sit a unit off the one before
//! it. Such a step is no detour anyone wants; it only looks unsteady. Each
//! jog shorter than [`SNAP`] goes: the run before it or the one after it
//! moves onto the other's line, an end sliding along the side of its box.
//! A run moves only where it keeps inside that side, off every other box,
//! and as far from every other line as before, or [`CLEAR`] at least.

use super::Rect;

type Point = (f32, f32);

/// A step shorter than this goes.
pub(super) const SNAP: f32 = 8.5;
/// A moved run keeps at least this far from other lines.
const CLEAR: f32 = 4.0;
/// An end keeps this far from the corners of its side, which may be round.
const CORNER: f32 = 8.0;

/// One end of a line: the box it meets, and whether lines may meet that
/// box anywhere along a side.
#[derive(Debug, Clone, Copy)]
pub(super) struct End {
    pub rect: Rect,
    pub flat: bool,
}

/// Takes the small steps out of `lines`, whose ends are `ends`; `boxes`
/// are every box the lines must keep out of.
pub(super) fn unjog(lines: &mut [Vec<Point>], ends: &[(Option<End>, Option<End>)], boxes: &[Rect]) {
    for index in 0..lines.len() {
        let mut k = 1;
        while k + 2 < lines[index].len() {
            if let Some(moved) = snap(lines, index, k, ends[index], boxes) {
                lines[index] = moved;
                // The jog is gone and its runs merged; look here again.
                continue;
            }
            k += 1;
        }
    }
}

/// `lines[index]` with the jog from point `k` to `k + 1` taken out, if it
/// is one and a run may move.
fn snap(
    lines: &[Vec<Point>],
    index: usize,
    k: usize,
    (start, end): (Option<End>, Option<End>),
    boxes: &[Rect],
) -> Option<Vec<Point>> {
    let points = &lines[index];
    let (a, b) = (points[k], points[k + 1]);
    let across = (b.0 - a.0, b.1 - a.1);
    let length = across.0.abs() + across.1.abs();
    let flat_x = across.1.abs() < 0.01;
    let flat_y = across.0.abs() < 0.01;
    if length < 0.01 || length >= SNAP || !(flat_x || flat_y) {
        return None;
    }
    // The runs on both sides run along the flow, at right angles to it.
    let along = |p: Point, q: Point| {
        if flat_x {
            (p.0 - q.0).abs() < 0.01
        } else {
            (p.1 - q.1).abs() < 0.01
        }
    };
    if !along(points[k - 1], a) || !along(b, points[k + 2]) {
        return None;
    }
    let last = points.len() - 1;
    // Either the run before the jog moves by `across`, or the one after it
    // moves back by it.
    let before: Vec<usize> = (0..=k)
        .rev()
        .take_while(|&i| i == k || i + 1 == k)
        .collect();
    let after: Vec<usize> = vec![k + 1, k + 2];
    let tries = [(after, (-across.0, -across.1)), (before, across)];
    for (moving, shift) in tries {
        let mut moved = points.clone();
        for &i in &moving {
            moved[i].0 += shift.0;
            moved[i].1 += shift.1;
        }
        // An end that moves stays on its side, away from its corners.
        let ends_ok = moving.iter().all(|&i| {
            let end = if i == 0 {
                start
            } else if i == last {
                end
            } else {
                return true;
            };
            match end {
                Some(end) => end.flat && on_side(moved[i], end.rect, flat_x),
                None => false,
            }
        });
        if !ends_ok {
            continue;
        }
        let run = (
            moved[*moving.iter().min().unwrap()],
            moved[*moving.iter().max().unwrap()],
        );
        let own: Vec<Rect> = [start, end].iter().flatten().map(|e| e.rect).collect();
        if boxes
            .iter()
            .filter(|rect| !own.contains(rect))
            .any(|&rect| cuts(run.0, run.1, rect))
        {
            continue;
        }
        let was = (
            points[*moving.iter().min().unwrap()],
            points[*moving.iter().max().unwrap()],
        );
        let clear = lines
            .iter()
            .enumerate()
            .filter(|&(other, _)| other != index)
            .all(|(_, line)| {
                line.windows(2).all(|s| {
                    let now = gap(run, (s[0], s[1]));
                    now >= CLEAR || now >= gap(was, (s[0], s[1])) - 0.01
                })
            });
        if !clear {
            continue;
        }
        return Some(merged(moved));
    }
    None
}

/// Whether `p`, an end moved across a side that runs along x (`flat_x`:
/// the side is horizontal) or along y, still lies on that side, off its
/// corners.
fn on_side(p: Point, rect: Rect, flat_x: bool) -> bool {
    let (lo, hi, at) = if flat_x {
        (rect.x, rect.x + rect.w, p.0)
    } else {
        (rect.y, rect.y + rect.h, p.1)
    };
    let corner = CORNER.min((hi - lo) / 4.0);
    at >= lo + corner && at <= hi - corner
}

/// Whether the run `a`–`b` (along an axis) passes through the inside of
/// `rect`.
fn cuts(a: Point, b: Point, rect: Rect) -> bool {
    let (x0, x1) = (a.0.min(b.0), a.0.max(b.0));
    let (y0, y1) = (a.1.min(b.1), a.1.max(b.1));
    x1 > rect.x + 0.5
        && x0 < rect.x + rect.w - 0.5
        && y1 > rect.y + 0.5
        && y0 < rect.y + rect.h - 0.5
}

/// The distance between two axis-aligned runs that lie side by side; far
/// when they do not.
fn gap((a, b): (Point, Point), (c, d): (Point, Point)) -> f32 {
    let vertical = |p: Point, q: Point| (p.0 - q.0).abs() < 0.01;
    let horizontal = |p: Point, q: Point| (p.1 - q.1).abs() < 0.01;
    let overlap = |p0: f32, p1: f32, q0: f32, q1: f32| {
        p0.min(p1).max(q0.min(q1)) < p0.max(p1).min(q0.max(q1))
    };
    if vertical(a, b) && vertical(c, d) && overlap(a.1, b.1, c.1, d.1) {
        (a.0 - c.0).abs()
    } else if horizontal(a, b) && horizontal(c, d) && overlap(a.0, b.0, c.0, d.0) {
        (a.1 - c.1).abs()
    } else {
        f32::INFINITY
    }
}

/// The points without repeats, and without a point on a straight run.
fn merged(points: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if out
            .last()
            .is_some_and(|q| (q.0 - p.0).abs() < 0.01 && (q.1 - p.1).abs() < 0.01)
        {
            continue;
        }
        while out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let straight = ((a.0 - b.0).abs() < 0.01 && (b.0 - p.0).abs() < 0.01)
                || ((a.1 - b.1).abs() < 0.01 && (b.1 - p.1).abs() < 0.01);
            if straight {
                out.pop();
            } else {
                break;
            }
        }
        out.push(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn a_small_step_between_two_boxes_goes() {
        let (a, b) = (rect(0.0, 0.0, 80.0, 30.0), rect(0.0, 100.0, 80.0, 30.0));
        let mut lines = vec![vec![
            (40.0, 30.0),
            (40.0, 60.0),
            (42.0, 60.0),
            (42.0, 100.0),
        ]];
        let ends = [(
            Some(End {
                rect: a,
                flat: true,
            }),
            Some(End {
                rect: b,
                flat: true,
            }),
        )];
        unjog(&mut lines, &ends, &[a, b]);
        assert_eq!(lines[0], [(40.0, 30.0), (40.0, 100.0)]);
    }

    #[test]
    fn a_step_stays_where_moving_would_crowd_a_line_or_a_round_box() {
        let (a, b) = (rect(0.0, 0.0, 80.0, 30.0), rect(0.0, 100.0, 80.0, 30.0));
        let jogged = vec![(40.0, 30.0), (40.0, 60.0), (42.0, 60.0), (42.0, 100.0)];
        // Another line runs 2 units beside where either run would go.
        let mut lines = vec![
            jogged.clone(),
            vec![(38.5, 30.0), (38.5, 100.0)],
            vec![(43.5, 30.0), (43.5, 100.0)],
        ];
        let ends = [
            (
                Some(End {
                    rect: a,
                    flat: true,
                }),
                Some(End {
                    rect: b,
                    flat: true,
                }),
            ),
            (None, None),
            (None, None),
        ];
        unjog(&mut lines, &ends, &[a, b]);
        assert_eq!(lines[0], jogged);
        // A round box takes its line in the middle of a side only.
        let mut lines = vec![jogged.clone()];
        let ends = [(
            Some(End {
                rect: a,
                flat: false,
            }),
            Some(End {
                rect: b,
                flat: false,
            }),
        )];
        unjog(&mut lines, &ends, &[a, b]);
        assert_eq!(lines[0], jogged);
        // A large step is a choice of the layout, not a jog.
        let wide = vec![(40.0, 30.0), (40.0, 60.0), (50.0, 60.0), (50.0, 100.0)];
        let mut lines = vec![wide.clone()];
        let ends = [(
            Some(End {
                rect: a,
                flat: true,
            }),
            Some(End {
                rect: b,
                flat: true,
            }),
        )];
        unjog(&mut lines, &ends, &[a, b]);
        assert_eq!(lines[0], wide);
    }

    #[test]
    fn a_run_through_the_ranks_moves_onto_the_one_before_it() {
        let (a, b) = (rect(0.0, 0.0, 80.0, 30.0), rect(200.0, 300.0, 80.0, 30.0));
        let mut lines = vec![vec![
            (40.0, 30.0),
            (40.0, 60.0),
            (120.0, 60.0),
            (120.0, 150.0),
            (121.0, 150.0),
            (121.0, 250.0),
            (240.0, 250.0),
            (240.0, 300.0),
        ]];
        let ends = [(
            Some(End {
                rect: a,
                flat: true,
            }),
            Some(End {
                rect: b,
                flat: true,
            }),
        )];
        unjog(&mut lines, &ends, &[a, b]);
        assert_eq!(
            lines[0],
            [
                (40.0, 30.0),
                (40.0, 60.0),
                (120.0, 60.0),
                (120.0, 250.0),
                (240.0, 250.0),
                (240.0, 300.0)
            ]
        );
    }
}
