//! Lines without small steps, and without a step too many. Where a line
//! leaves one box and enters the next a few units further across, its
//! route steps aside by that much (a jog), and a run through the ranks
//! can sit a unit off the one before it. Such a step is no detour anyone
//! wants; it only looks unsteady. Each jog shorter than [`SNAP`] goes
//! ([`unjog`]): the run before it or the one after it moves onto the
//! other's line, an end sliding along the side of its box. A run moves
//! only where it keeps inside that side, off every other box and the
//! labels of other lines, and as far from every other line as before, or
//! [`CLEAR`] at least; its own label moves with it, onto no line or box.
//!
//! A line through several ranks steps aside in each gap, as far as the
//! point it passes through there was placed off the one before: three
//! small steps the same way where one would do. Where a line steps aside
//! twice the same way, the run between the two steps moves onto the far
//! side of the longer one, so the line steps once ([`merge`]); the shorter
//! step is absorbed, never an end moved. The run moves only where it keeps
//! off every box, the name of every frame and the labels of other lines,
//! enters no frame the line did not pass through before, crosses no line
//! it did not cross, and keeps [`ROOM`] from the lines beside it, or as
//! much as before.

use super::Rect;

type Point = (f32, f32);

/// A step shorter than this goes.
pub(super) const SNAP: f32 = 8.5;
/// A moved run keeps at least this far from other lines.
const CLEAR: f32 = 4.0;
/// An end keeps this far from the corners of its side, which may be round.
const CORNER: f32 = 8.0;
/// A run moved to take a step out keeps at least this far from the lines
/// beside it.
const ROOM: f32 = 10.0;
/// Its label keeps this far from other words and from boxes.
const MARGIN: f32 = 4.0;

/// One end of a line: the box it meets, and whether lines may meet that
/// box anywhere along a side.
#[derive(Debug, Clone, Copy)]
pub(super) struct End {
    pub rect: Rect,
    pub flat: bool,
}

/// Where the label of a line lies for the points it has, if it has one.
pub(super) type Label<'a> = &'a dyn Fn(usize, &[Point]) -> Option<Rect>;

/// Takes the small steps out of `lines`, whose ends are `ends`; `boxes`
/// are every box the lines must keep out of. The label of each line
/// (`label`) follows it: the other lines keep out of it, and a run moves
/// only where its label, moved with it, covers no other line or box.
pub(super) fn unjog(
    lines: &mut [Vec<Point>],
    ends: &[(Option<End>, Option<End>)],
    boxes: &[Rect],
    label: Label,
) {
    let mut labels: Vec<Option<Rect>> = (0..lines.len()).map(|i| label(i, &lines[i])).collect();
    for index in 0..lines.len() {
        let mut k = 1;
        while k + 2 < lines[index].len() {
            let solids: Vec<Rect> = labels
                .iter()
                .enumerate()
                .filter(|&(other, _)| other != index)
                .filter_map(|(_, label)| *label)
                .chain(boxes.iter().copied())
                .collect();
            let own = |points: &[Point]| label(index, points);
            if let Some(moved) = snap(lines, index, k, ends[index], &solids, &own) {
                labels[index] = label(index, &moved);
                lines[index] = moved;
                // The jog is gone and its runs merged, which may have made
                // a step before it small: look again from the start. Each
                // pass takes points out, so this ends.
                k = 1;
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
    label: &dyn Fn(&[Point]) -> Option<Rect>,
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
        // The label follows its run, onto nothing else.
        if let Some(rect) = label(&moved) {
            let lined = lines
                .iter()
                .enumerate()
                .filter(|&(other, _)| other != index)
                .any(|(_, line)| line.windows(2).any(|s| cuts(s[0], s[1], rect)));
            if lined || boxes.iter().any(|&solid| overlap(solid, rect)) {
                continue;
            }
        }
        return Some(merged(moved));
    }
    None
}

/// Takes the steps too many out of `lines`: wherever a line steps aside
/// twice the same way, it steps once, where the longer step was. `boxes`
/// are every box and frame name a run must keep off, `frames` the frames
/// a run must not enter anew, `label` where the label of a line lies;
/// `sideways` tells whether a step runs along x (the flow runs down) or
/// along y.
pub(super) fn merge(
    lines: &mut [Vec<Point>],
    boxes: &[Rect],
    frames: &[Rect],
    label: Label,
    sideways: bool,
) {
    let mut labels: Vec<Option<Rect>> = (0..lines.len()).map(|i| label(i, &lines[i])).collect();
    for index in 0..lines.len() {
        // The shortest step first, into a longer one beside it; each step
        // taken out leaves fewer, so this ends.
        'again: loop {
            let solids: Vec<Rect> = labels
                .iter()
                .enumerate()
                .filter(|&(other, _)| other != index)
                .filter_map(|(_, label)| *label)
                .chain(boxes.iter().copied())
                .collect();
            let own = |points: &[Point]| label(index, points);
            let points = &lines[index];
            let mut steps: Vec<(f32, usize)> = (1..points.len().saturating_sub(2))
                .map(|k| {
                    let (a, b) = (points[k], points[k + 1]);
                    ((a.0 - b.0).abs() + (a.1 - b.1).abs(), k)
                })
                .collect();
            steps.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (_, k) in steps {
                if let Some(moved) = absorb(lines, index, k, (&solids, frames), &own, sideways) {
                    labels[index] = label(index, &moved);
                    lines[index] = moved;
                    continue 'again;
                }
            }
            break;
        }
    }
}

/// `lines[index]` with its step from point `k` to `k + 1` absorbed into
/// the step before or after it, the longer one first, if one goes the
/// same way and the run between them may move.
fn absorb(
    lines: &[Vec<Point>],
    index: usize,
    k: usize,
    (solids, frames): (&[Rect], &[Rect]),
    label: &dyn Fn(&[Point]) -> Option<Rect>,
    sideways: bool,
) -> Option<Vec<Point>> {
    let points = &lines[index];
    let (a, b) = (points[k], points[k + 1]);
    let across = (b.0 - a.0, b.1 - a.1);
    let flat_x = across.1.abs() < 0.01;
    let flat_y = across.0.abs() < 0.01;
    if across.0.abs() + across.1.abs() < 0.01 || !(flat_x || flat_y) || flat_x != sideways {
        return None;
    }
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
    // How far a step goes, signed along the axis of this one.
    let step = |p: Point, q: Point| if flat_x { q.0 - p.0 } else { q.1 - p.1 };
    let this = step(a, b);
    // A step the other way takes this one in too, as long as what is left
    // of it is no jog: a straight run, or a step as large as the layout's.
    let takes = |other: f32| {
        let left = (other + this).abs();
        other.abs() > 0.01 && (other * this > 0.0 || left < 0.01 || left >= SNAP)
    };
    // (length of the step absorbing this one, the run that moves, by how much)
    let mut ways: Vec<(f32, [usize; 2], Point)> = Vec::new();
    if k + 3 < points.len() {
        let next = step(points[k + 2], points[k + 3]);
        if takes(next) {
            ways.push((next.abs(), [k + 1, k + 2], (-across.0, -across.1)));
        }
    }
    if k >= 2 {
        let prev = step(points[k - 2], points[k - 1]);
        if takes(prev) {
            ways.push((prev.abs(), [k - 1, k], across));
        }
    }
    ways.sort_by(|a, b| b.0.total_cmp(&a.0));
    let others: Vec<&Vec<Point>> = lines
        .iter()
        .enumerate()
        .filter(|&(other, _)| other != index)
        .map(|(_, line)| line)
        .collect();
    let crossings = |line: &[Point]| -> usize {
        others
            .iter()
            .map(|other| {
                line.windows(2)
                    .map(|s| {
                        other
                            .windows(2)
                            .filter(|t| super::bundle::crossed(s[0], s[1], t[0], t[1]))
                            .count()
                    })
                    .sum::<usize>()
            })
            .sum()
    };
    for (_, moving, shift) in ways {
        let mut moved = points.clone();
        for &i in &moving {
            moved[i].0 += shift.0;
            moved[i].1 += shift.1;
        }
        let run = (moved[moving[0]], moved[moving[1]]);
        let was = (points[moving[0]], points[moving[1]]);
        // Through nothing the line kept out of: the run moved and the step
        // grown may both have come onto a box, a name or into a frame.
        let through = |line: &[Point], rect: Rect| line.windows(2).any(|s| cuts(s[0], s[1], rect));
        if solids
            .iter()
            .chain(frames)
            .any(|&rect| through(&moved, rect) && !through(points, rect))
        {
            continue;
        }
        let clear = others.iter().all(|line| {
            line.windows(2).all(|s| {
                let now = gap(run, (s[0], s[1]));
                now >= ROOM || now >= gap(was, (s[0], s[1])) - 0.01
            })
        });
        if !clear {
            continue;
        }
        let moved = merged(moved);
        if crossings(&moved) > crossings(points) {
            continue;
        }
        // The label follows its run, onto nothing else and not up against
        // other words or a box.
        if let Some(rect) = label(&moved) {
            let lined = others
                .iter()
                .any(|line| line.windows(2).any(|s| cuts(s[0], s[1], rect)));
            let roomy = Rect {
                x: rect.x - MARGIN,
                y: rect.y - MARGIN,
                w: rect.w + 2.0 * MARGIN,
                h: rect.h + 2.0 * MARGIN,
            };
            if lined || solids.iter().any(|&solid| overlap(solid, roomy)) {
                continue;
            }
        }
        return Some(moved);
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

/// Whether two boxes overlap by more than a touch.
fn overlap(a: Rect, b: Rect) -> bool {
    a.x + 0.5 < b.x + b.w && b.x + 0.5 < a.x + a.w && a.y + 0.5 < b.y + b.h && b.y + 0.5 < a.y + a.h
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
        unjog(&mut lines, &ends, &[a, b], &|_, _| None);
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
        unjog(&mut lines, &ends, &[a, b], &|_, _| None);
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
        unjog(&mut lines, &ends, &[a, b], &|_, _| None);
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
        unjog(&mut lines, &ends, &[a, b], &|_, _| None);
        assert_eq!(lines[0], wide);
    }

    #[test]
    fn two_steps_the_same_way_become_one_where_the_longer_was() {
        let (a, b) = (rect(0.0, 0.0, 80.0, 30.0), rect(200.0, 300.0, 80.0, 30.0));
        // Right by 100 in the first gap, by 20 in the second, by 80 in
        // the third: the small step goes into the long one before it, then
        // the last one goes there too, since its run may move.
        let mut lines = vec![vec![
            (40.0, 30.0),
            (40.0, 60.0),
            (140.0, 60.0),
            (140.0, 150.0),
            (160.0, 150.0),
            (160.0, 250.0),
            (240.0, 250.0),
            (240.0, 300.0),
        ]];
        merge(&mut lines, &[a, b], &[], &|_, _| None, true);
        assert_eq!(
            lines[0],
            [(40.0, 30.0), (40.0, 60.0), (240.0, 60.0), (240.0, 300.0)]
        );
        // A box beside the second gap: no run may come to x 240 there, so
        // the steps end up in the last gap, below it.
        let mut lines = vec![vec![
            (40.0, 30.0),
            (40.0, 60.0),
            (140.0, 60.0),
            (140.0, 150.0),
            (160.0, 150.0),
            (160.0, 250.0),
            (240.0, 250.0),
            (240.0, 300.0),
        ]];
        let wall = rect(200.0, 100.0, 80.0, 30.0);
        merge(&mut lines, &[a, b, wall], &[], &|_, _| None, true);
        assert_eq!(
            lines[0],
            [(40.0, 30.0), (40.0, 250.0), (240.0, 250.0), (240.0, 300.0)]
        );
    }

    #[test]
    fn a_step_and_one_back_cancel_out_but_leave_no_jog() {
        let (a, b) = (rect(0.0, 0.0, 80.0, 30.0), rect(0.0, 300.0, 80.0, 30.0));
        let mut lines = vec![vec![
            (40.0, 30.0),
            (40.0, 60.0),
            (75.0, 60.0),
            (75.0, 250.0),
            (40.0, 250.0),
            (40.0, 300.0),
        ]];
        merge(&mut lines, &[a, b], &[], &|_, _| None, true);
        assert_eq!(lines[0], [(40.0, 30.0), (40.0, 300.0)]);
        // Right by 35, back by 30: a step of 5 would be left, so it stays.
        let zigzag = vec![
            (40.0, 30.0),
            (40.0, 60.0),
            (75.0, 60.0),
            (75.0, 250.0),
            (45.0, 250.0),
            (45.0, 300.0),
        ];
        let mut lines = vec![zigzag.clone()];
        merge(&mut lines, &[a, b], &[], &|_, _| None, true);
        assert_eq!(lines[0], zigzag);
    }

    #[test]
    fn a_run_stays_off_other_lines_names_and_frames() {
        let (a, b) = (rect(0.0, 0.0, 80.0, 30.0), rect(200.0, 300.0, 80.0, 30.0));
        let stepped = vec![
            (40.0, 30.0),
            (40.0, 60.0),
            (140.0, 60.0),
            (140.0, 150.0),
            (240.0, 150.0),
            (240.0, 300.0),
        ];
        // The run between the steps could move to x 40 or to x 240. Other
        // lines run 6 units beside both.
        let mut lines = vec![
            stepped.clone(),
            vec![(34.0, 30.0), (34.0, 300.0)],
            vec![(246.0, 30.0), (246.0, 300.0)],
        ];
        merge(&mut lines, &[a, b], &[], &|_, _| None, true);
        assert_eq!(lines[0], stepped);
        // Lines the run would cross either way, which it does not now.
        let mut lines = vec![
            stepped.clone(),
            vec![(0.0, 100.0), (100.0, 100.0)],
            vec![(190.0, 100.0), (300.0, 100.0)],
        ];
        merge(&mut lines, &[a, b], &[], &|_, _| None, true);
        assert_eq!(lines[0], stepped);
        // Frames the line kept out of, on both sides.
        let mut lines = vec![stepped.clone()];
        merge(
            &mut lines,
            &[a, b],
            &[rect(0.0, 80.0, 100.0, 50.0), rect(190.0, 80.0, 200.0, 50.0)],
            &|_, _| None,
            true,
        );
        assert_eq!(lines[0], stepped);
        // Only the far side blocked: the run comes onto the near one.
        let mut lines = vec![stepped.clone(), vec![(246.0, 30.0), (246.0, 300.0)]];
        merge(&mut lines, &[a, b], &[], &|_, _| None, true);
        assert_eq!(
            lines[0],
            [(40.0, 30.0), (40.0, 150.0), (240.0, 150.0), (240.0, 300.0)]
        );
        // With the flow running right, the steps run along y.
        let swap = |r: Rect| rect(r.y, r.x, r.h, r.w);
        let mut lines = vec![stepped.iter().map(|&(x, y)| (y, x)).collect::<Vec<_>>()];
        merge(&mut lines, &[swap(a), swap(b)], &[], &|_, _| None, false);
        assert_eq!(
            lines[0],
            [(30.0, 40.0), (150.0, 40.0), (150.0, 240.0), (300.0, 240.0)]
        );
        // Nothing in the way: one step, in the later gap of two alike.
        let mut lines = vec![stepped];
        merge(&mut lines, &[a, b], &[], &|_, _| None, true);
        assert_eq!(
            lines[0],
            [(40.0, 30.0), (40.0, 150.0), (240.0, 150.0), (240.0, 300.0)]
        );
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
        unjog(&mut lines, &ends, &[a, b], &|_, _| None);
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
