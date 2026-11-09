//! The ranks between ranks of boxes (docs/adr/0020). A box of rank *r*
//! takes rank *2r*, so every line passes a rank between the boxes it joins,
//! where it has a point of its own, and a labelled line's point there is a
//! box as big as its label. The ranks of boxes are ordered as before, and
//! each rank between follows them ([`order_ranks`]); each label takes the
//! side of its line away from the lines beside it ([`label_sides`]); once
//! the lines meet their boxes, the points move onto them ([`point_wants`]),
//! and the gaps make room for the lines that bend there ([`rank_gaps`]).

use super::hold::{self, Hold};
use super::{ALONG_PAD, Direction, Edge, PORT_CORNER, Ports, Slot, TRACK, center, nest};

/// Between a label and its line, across the flow, by direction: beside a
/// line running down, and above or below one running across.
const LABEL_GAP: [f32; 2] = [6.0, 4.0];

/// The room a label takes in its rank, with the gap to its line: across
/// the flow and along it.
pub(super) fn label_room(edge: &Edge, direction: Direction) -> (f32, f32) {
    match direction {
        Direction::Down => (edge.label_width + LABEL_GAP[0], edge.label_height),
        Direction::Right => (edge.label_height + LABEL_GAP[1], edge.label_width),
    }
}

/// Which side of its line each label lies on, and its room with it: right
/// of a line running down, above one running across. But where the label
/// would lie between its line and a neighbor that comes from the same slot
/// or goes on to the same one (lines side by side), it goes to the other
/// side, outside the pair (docs/adr/0012, rule 4); unless a neighbor there
/// shares a box as narrow or narrower, where the two lines run closer
/// still. Decided once the ranks are ordered, before anything is placed.
pub(super) fn label_sides(
    slots: &mut [Slot],
    rows: &[Vec<usize>],
    chains: &[Vec<usize>],
    labels: &[Option<usize>],
    direction: Direction,
) -> Vec<bool> {
    let mut flips = vec![false; chains.len()];
    // The slot before or after a point, a frame for a point on its border.
    let key = |slot: Option<&usize>| {
        slot.map(|&slot| match slots[slot].anchor {
            Some((frame, _)) => slots.len() + frame,
            None => slot,
        })
    };
    // How wide the narrowest slot is that `a` and `b` both come from or
    // both go on to; a frame's border counts as wide, and none as no end.
    let together = |a: usize, b: usize| {
        let (a, b) = (&slots[a], &slots[b]);
        [
            (a.preds.first(), b.preds.first()),
            (a.succs.first(), b.succs.first()),
        ]
        .into_iter()
        .filter_map(|(x, y)| (x.is_some() && key(x) == key(y)).then_some(x).flatten())
        .map(|&slot| match slots[slot].real {
            Some(_) => slots[slot].box_cross,
            None => f32::MAX,
        })
        .fold(f32::INFINITY, f32::min)
    };
    let mut label_of = vec![None; slots.len()];
    for (index, at) in labels.iter().enumerate() {
        if let Some(at) = *at {
            label_of[chains[index][at]] = Some(index);
        }
    }
    for row in rows.iter().skip(1).step_by(2) {
        for (at, &point) in row.iter().enumerate() {
            let Some(index) = label_of[point] else {
                continue;
            };
            let before = at.checked_sub(1).map(|b| row[b]);
            let after = row.get(at + 1).copied();
            // The neighbor on the side the label takes, and the other.
            let (usual, other) = match direction {
                Direction::Down => (after, before),
                Direction::Right => (before, after),
            };
            let paired = |n: Option<usize>| n.map_or(f32::INFINITY, |n| together(point, n));
            flips[index] = paired(usual) < paired(other);
        }
    }
    for (index, at) in labels.iter().enumerate() {
        if let Some(at) = *at {
            let slot = &mut slots[chains[index][at]];
            // The room lies before the line: left of it, or above it.
            let before = (direction == Direction::Down) == flips[index];
            slot.before = if before { slot.pack_cross } else { 0.0 };
        }
    }
    flips
}

/// Where a label lies in the room its line took in a rank: beside the run
/// of the line through that rank, on its side, and along the straight part
/// of that run. Drawn, the run's corners turn in curves of up to
/// [`crate::curve::RADIUS`], and a curve that turns toward the label's side
/// reaches into the rank: the label moves along the run until the curve
/// stays [`LABEL_GAP`] off it, as far as the run allows; where the run is
/// too short for that, the label sits in the run's middle and moves off
/// the line by what the curve still reaches.
pub(super) fn label_beside(
    points: &[(f32, f32)],
    slot: &Slot,
    edge: &Edge,
    flip: bool,
    direction: Direction,
) -> (f32, f32) {
    let along = |p: (f32, f32)| match direction {
        Direction::Down => p.1,
        Direction::Right => p.0,
    };
    let across = |p: (f32, f32)| match direction {
        Direction::Down => p.0,
        Direction::Right => p.1,
    };
    let (label_across, label_along) = match direction {
        Direction::Down => (edge.label_width, edge.label_height),
        Direction::Right => (edge.label_height, edge.label_width),
    };
    // The label's side of the line, across the flow.
    let side = match (direction, flip) {
        (Direction::Down, false) | (Direction::Right, true) => 1.0,
        (Direction::Down, true) | (Direction::Right, false) => -1.0,
    };
    let gap = match direction {
        Direction::Down => LABEL_GAP[0],
        Direction::Right => LABEL_GAP[1],
    };
    let middle = slot.primary + slot.box_along / 2.0;
    // The run through the rank, by its first point.
    let run = points.windows(2).position(|pair| {
        let (a, b) = (pair[0], pair[1]);
        (across(a) - across(b)).abs() < 0.01
            && along(a).min(along(b)) <= middle
            && middle <= along(a).max(along(b))
    });
    let Some(k) = run else {
        return put(
            center(slot),
            slot.primary,
            side,
            gap,
            label_across,
            direction,
        );
    };
    let line = across(points[k]);
    // The run's ends in the flow's order, and the point past each corner.
    let (near, far, past_near, past_far) = if along(points[k]) <= along(points[k + 1]) {
        (
            k,
            k + 1,
            k.checked_sub(1),
            (k + 2 < points.len()).then_some(k + 2),
        )
    } else {
        (
            k + 1,
            k,
            (k + 2 < points.len()).then_some(k + 2),
            k.checked_sub(1),
        )
    };
    // How far the label keeps from a corner so the curve there stays
    // `gap` off it: nothing where the line turns away from the label's
    // side, or where the run ends on a box.
    let keeps = |corner: usize, past: Option<usize>| -> (f32, f32) {
        let Some(past) = past else {
            return (0.0, 0.0);
        };
        let turn = across(points[past]) - line;
        if turn * side <= 0.01 {
            return (0.0, 0.0);
        }
        let r = crate::curve::radius_at(points, corner);
        let clear = (gap - 1.0).max(0.0);
        if r <= clear {
            return (0.0, r);
        }
        (r - (2.0 * r * clear - clear * clear).sqrt(), r)
    };
    let (keep_near, r_near) = keeps(near, past_near);
    let (keep_far, r_far) = keeps(far, past_far);
    if keep_near <= 0.0 && keep_far <= 0.0 {
        return put(line, slot.primary, side, gap, label_across, direction);
    }
    let (start, end) = (along(points[near]), along(points[far]));
    let (lo, hi) = (start + keep_near, end - keep_far - label_along);
    if lo <= hi {
        return put(
            line,
            slot.primary.clamp(lo, hi),
            side,
            gap,
            label_across,
            direction,
        );
    }
    // Too short a run: in its middle, off the line by what the curves
    // reach at the label's nearest rows.
    let at = ((start + end) / 2.0 - label_along / 2.0).max(start);
    let reach = |r: f32, d: f32| {
        if d >= r || r <= 0.0 {
            0.0
        } else {
            r - (r * r - (r - d) * (r - d)).sqrt()
        }
    };
    let into = reach(r_near, (at - start).max(0.0))
        .max(reach(r_far, (end - at - label_along).max(0.0)))
        .min(r_near.max(r_far));
    let off = (into - (gap - 1.0)).max(0.0);
    put(line, at, side, gap + off, label_across, direction)
}

/// The top left of a label `gap` off a line at `line` across the flow, on
/// `side` of it, at `at` along the flow.
fn put(
    line: f32,
    at: f32,
    side: f32,
    gap: f32,
    label_across: f32,
    direction: Direction,
) -> (f32, f32) {
    let across = if side > 0.0 {
        line + gap
    } else {
        line - gap - label_across
    };
    match direction {
        Direction::Down => (across, at),
        Direction::Right => (at, across),
    }
}

/// Orders the ranks as if they were not doubled: the ranks of boxes among
/// themselves, each line's point between them skipped, by the sweeps
/// below; then each rank between them by where its points' lines go on
/// to, and of two that go on to the same slot by where they come from. So
/// lines cross only in the gap after the rank they leave, where they bend,
/// and as often as before the ranks were doubled. A held box moves beside
/// its neighbor once the ranks of boxes are ordered ([`hold::order`]). `framing` holds the
/// frames around each slot and how many there are, if any.
pub(super) fn order_ranks(
    rows: &mut [Vec<usize>],
    slots: &mut [Slot],
    framing: Option<(&[Vec<usize>], usize)>,
    holds: &[Hold],
) {
    // Every line passes a point in each rank between two ranks of boxes,
    // which has one slot before it and one after.
    let skip = |slots: &[Slot], links: &[usize], before: bool| -> Vec<usize> {
        links
            .iter()
            .filter_map(|&point| {
                let s = &slots[point];
                if before {
                    s.preds.first()
                } else {
                    s.succs.first()
                }
                .copied()
            })
            .collect()
    };
    let mut boxes: Vec<Vec<usize>> = rows.iter().step_by(2).cloned().collect();
    let kept: Vec<(usize, Vec<usize>, Vec<usize>)> = boxes
        .iter()
        .flatten()
        .map(|&slot| {
            let s = &slots[slot];
            (
                slot,
                skip(slots, &s.preds, true),
                skip(slots, &s.succs, false),
            )
        })
        .collect();
    let mut was = Vec::with_capacity(kept.len());
    for (slot, preds, succs) in kept {
        let s = &mut slots[slot];
        let preds = std::mem::replace(&mut s.preds, preds);
        let succs = std::mem::replace(&mut s.succs, succs);
        was.push((slot, preds, succs));
    }
    match framing {
        Some((frames, count)) => super::sweep_framed(&mut boxes, slots, frames, count),
        None => super::sweep(&mut boxes, slots),
    }
    for (slot, preds, succs) in was {
        slots[slot].preds = preds;
        slots[slot].succs = succs;
    }
    // A held box stands beside its neighbor (docs/adr/0021).
    hold::order(&mut boxes, holds);
    for (rank, row) in boxes.into_iter().enumerate() {
        rows[2 * rank] = row;
    }
    for rank in (1..rows.len()).step_by(2) {
        for by_preds in [true, false] {
            match framing {
                Some((frames, count)) => {
                    let pos = nest::positions(rows, slots);
                    let even: Vec<Vec<usize>> = rows.iter().step_by(2).cloned().collect();
                    let keys = nest::keys(&even, &pos, frames, count);
                    let by = if by_preds {
                        nest::By::Preds
                    } else {
                        nest::By::Succs
                    };
                    nest::sort_row(&mut rows[rank], slots, &pos, by, frames, &keys);
                }
                None => {
                    let pos = super::positions(rows, slots.len());
                    super::sort_row(&mut rows[rank], slots, &pos, by_preds);
                }
            }
        }
    }
}

/// Where the points of the ranks between two ranks of boxes want to lie,
/// by rank (none for a rank of boxes), as [`super::fit_to`] and
/// [`nest::Placer::fit`] take them: each where its line enters the slot
/// after it, so the line bends once, a little past the rank before, as it
/// did before the ranks were doubled; and a label's point, where the two
/// boxes of its line overlap across the flow as far as their sides reach,
/// inside that, so the line runs straight. Points keep their order and room, so the room
/// of a label beside may hold a line off; it then bends once more, before
/// the box it goes into.
pub(super) fn point_wants(
    slots: &[Slot],
    rows: &[Vec<usize>],
    chains: &[Vec<usize>],
    ports: &[Ports],
    flat: &[bool],
) -> Vec<Vec<(usize, f32, f32)>> {
    // How far across a line may meet a slot: along a flat side, or only in
    // the middle; anywhere on a frame's border, where it follows the line.
    let reach = |slot: usize| {
        let s = &slots[slot];
        if s.anchor.is_some() {
            return (f32::NEG_INFINITY, f32::INFINITY);
        }
        let half = if flat[slot] {
            (s.box_cross / 2.0 - PORT_CORNER.min(s.box_cross / 4.0)).max(0.0)
        } else {
            0.0
        };
        (center(s) - half, center(s) + half)
    };
    // For every point, the slot after it and where across its line enters
    // that.
    let mut ahead: Vec<Option<f32>> = vec![None; slots.len()];
    for (index, chain) in chains.iter().enumerate() {
        let last = chain.len().saturating_sub(1);
        for step in 1..last {
            let next = chain[step + 1];
            let port = if step + 1 == last {
                ports[index].end
            } else {
                0.0
            };
            let mut at = center(&slots[next]) + port;
            // A label's point: its ports head for it, so inside the
            // overlap the line runs straight. A line without one meets its
            // boxes where its ports say, and its point follows.
            if last == 2 && slots[chain[step]].pack_cross > 0.0 {
                let ((a, b), (c, d)) = (reach(chain[0]), reach(next));
                let (lo, hi) = (a.max(c), b.min(d));
                if lo <= hi {
                    at = at.clamp(lo, hi);
                }
            }
            ahead[chain[step]] = Some(at);
        }
    }
    rows.iter()
        .enumerate()
        .map(|(rank, row)| {
            if rank % 2 == 0 {
                return Vec::new();
            }
            row.iter()
                .map(|&slot| {
                    let s = &slots[slot];
                    match ahead[slot] {
                        Some(at) => (slot, at - s.before - s.box_cross / 2.0, 1.0),
                        None => (slot, s.cross, 0.05),
                    }
                })
                .collect()
        })
        .collect()
}

/// Where the points before a line's pass beside a frame's name want to
/// lie, by rank: where the line comes down beside the name ([`nest::jogs`]),
/// so it bends once on its way there, not once more after the point.
pub(super) fn pass_wants(
    slots: &[Slot],
    rows: &[Vec<usize>],
    chains: &[Vec<usize>],
    jogs: &nest::Jogs,
) -> Vec<Vec<(usize, f32, f32)>> {
    let mut beside: Vec<Option<f32>> = vec![None; slots.len()];
    for (&(index, step), way) in jogs {
        if let (Some(&point), Some(jog)) = (chains[index].get(step), way.first())
            && step > 0
        {
            beside[point] = Some(jog.above);
        }
    }
    rows.iter()
        .enumerate()
        .map(|(rank, row)| {
            if rank % 2 == 0 {
                return Vec::new();
            }
            row.iter()
                .filter_map(|&slot| {
                    let s = &slots[slot];
                    beside[slot].map(|at| (slot, at - s.before - s.box_cross / 2.0, 1.0))
                })
                .collect()
        })
        .collect()
}

/// Where each line meets a frame's border wants to lie, by rank: across
/// from the point its line passes before or after, so it runs straight to
/// the border.
pub(super) fn anchor_wants(slots: &[Slot], rows: &[Vec<usize>]) -> Vec<Vec<(usize, f32, f32)>> {
    rows.iter()
        .map(|row| {
            row.iter()
                .filter(|&&slot| slots[slot].anchor.is_some())
                .filter_map(|&slot| {
                    let s = &slots[slot];
                    let other = s.preds.first().or(s.succs.first())?;
                    Some((
                        slot,
                        center(&slots[*other]) - s.before - s.box_cross / 2.0,
                        1.0,
                    ))
                })
                .collect()
        })
        .collect()
}

/// The room after each rank, by how many lines bend there (`counts`): half
/// of [`ALONG_PAD`], and a track more for each line past the first. Where
/// one of them bends on its way into a box (`into_box`), after a rank
/// between two ranks of boxes, the box keeps the other half from the last
/// track too, for its arrow.
pub(super) fn rank_gaps(counts: &[usize], into_box: &[bool]) -> Vec<f32> {
    counts
        .iter()
        .zip(into_box)
        .enumerate()
        .map(|(gap, (&count, &into_box))| {
            let tracks = count.saturating_sub(1) as f32 * TRACK;
            if gap % 2 == 1 && into_box {
                ALONG_PAD + tracks
            } else {
                ALONG_PAD / 2.0 + tracks
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(primary: f32, box_along: f32) -> Slot {
        Slot {
            real: None,
            box_cross: 0.0,
            box_along,
            before: 0.0,
            pack_cross: 0.0,
            preds: Vec::new(),
            succs: Vec::new(),
            cross: 100.0,
            primary,
            anchor: None,
            meets: None,
        }
    }

    fn edge(label_height: f32) -> Edge {
        Edge {
            from: 0,
            to: 1,
            label_width: 40.0,
            label_height,
            hidden: false,
            apart: false,
            marked: false,
            arrival: 0,
        }
    }

    /// Down from a box, left to the run through the rank, down into the
    /// next box: the curve of the corner above the run lies on its right,
    /// between the run and where the line came from.
    const LINE: [(f32, f32); 4] = [(160.0, 0.0), (160.0, 88.0), (100.0, 88.0), (100.0, 200.0)];

    #[test]
    fn a_label_moves_along_its_run_out_of_the_curve() {
        // The label's rank starts 12 under the corner, where the curve
        // still bulges to the right; on the left the line turns away.
        let rank = slot(100.0, 14.0);
        let left = label_beside(&LINE, &rank, &edge(14.0), true, Direction::Down);
        assert_eq!(left, (100.0 - LABEL_GAP[0] - 40.0, 100.0));
        let right = label_beside(&LINE, &rank, &edge(14.0), false, Direction::Down);
        assert_eq!(right.0, 100.0 + LABEL_GAP[0]);
        assert!(right.1 > 100.0 && right.1 < 140.0, "{right:?}");
        // Down there the curve is `LABEL_GAP` off the label's edge.
        let r = crate::curve::radius_at(&LINE, 2);
        let d = right.1 - 88.0;
        let reach = r - (r * r - (r - d) * (r - d)).sqrt();
        assert!((reach - (LABEL_GAP[0] - 1.0)).abs() < 0.01, "{reach}");
    }

    #[test]
    fn a_short_run_puts_the_label_at_its_start_off_the_line() {
        // A run of 24 between two corners whose curves both lie on the
        // right, and a label taller than the run.
        let line = [
            (160.0, 0.0),
            (160.0, 60.0),
            (100.0, 60.0),
            (100.0, 84.0),
            (160.0, 84.0),
            (160.0, 200.0),
        ];
        let rank = slot(62.0, 30.0);
        let right = label_beside(&line, &rank, &edge(30.0), false, Direction::Down);
        assert!((right.1 - 60.0).abs() < 0.01, "{right:?}");
        assert!(
            right.0 > 100.0 + LABEL_GAP[0] + 5.0,
            "off the line by what the curve reaches: {right:?}"
        );
        // Turning away on the left: where it was.
        let left = label_beside(&line, &rank, &edge(30.0), true, Direction::Down);
        assert_eq!(left, (100.0 - LABEL_GAP[0] - 40.0, 62.0));
    }
}
