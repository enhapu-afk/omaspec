//! A line back against the flow keeps off the corner another line takes
//! (F8 of the review of the three diagrams). A box whose outline reaches
//! the middle of each side ([`Node::round`]: a choice's diamond, a circle,
//! a stadium) meets its lines at the middle of a side: a line arriving
//! from above and a line leaving back up from it met at its top corner,
//! and the two read as one line that splits. Where a line back against the
//! flow starts or ends at the point another line meets, it takes the side
//! the way it heads for instead, where no line meets it yet: out of the
//! side corner across, then along the flow, as a choice's branches leave a
//! diamond. Its run along the flow moves out past the corner where it ran
//! nearer, as long as it still ends on the side of a flat box; the new way
//! keeps off the boxes, the frames' names and the other lines' labels, and
//! out of frames the line was not in.

use super::route::{self, Point};
use super::{Direction, Edge, Node, Rect};

/// Two ends this close meet at one point.
const SAME: f32 = 0.5;
/// How far past the box's side a line runs along the flow to leave from
/// that side, and how far a moved end stays from the corners of the box
/// it ends at.
const OUT: f32 = 12.0;

/// What the lines of one layout are moved among.
pub(super) struct Around<'a> {
    pub edges: &'a [Edge],
    /// Which lines run back against the flow.
    pub back: &'a [bool],
    pub nodes: &'a [Node],
    pub rects: &'a [Rect],
    pub frames: &'a [Rect],
    /// The frames' names.
    pub names: &'a [Rect],
    /// Where the lines within a rank, routed apart, meet their boxes.
    pub taken: &'a [Point],
    /// Where the label of line `index` stands along `points`.
    pub label: &'a dyn Fn(usize, &[Point]) -> Option<Rect>,
    pub direction: Direction,
}

/// `lines` with each line back against the flow moved off a point of a
/// round box which another line meets.
pub(super) fn apart(lines: &mut [Vec<Point>], around: &Around) {
    // Laid out down the page; a flow to the right is turned to it and back.
    let turned = around.direction == Direction::Right;
    let turn = |p: Point| if turned { (p.1, p.0) } else { p };
    let turn_rect = |r: &Rect| {
        if turned {
            Rect {
                x: r.y,
                y: r.x,
                w: r.h,
                h: r.w,
            }
        } else {
            *r
        }
    };
    let mut work: Vec<Vec<Point>> = lines
        .iter()
        .map(|l| l.iter().map(|&p| turn(p)).collect())
        .collect();
    let boxes: Vec<Rect> = around.rects.iter().map(turn_rect).collect();
    let frames: Vec<Rect> = around.frames.iter().map(turn_rect).collect();
    let names: Vec<Rect> = around.names.iter().map(turn_rect).collect();
    let taken: Vec<Point> = around.taken.iter().map(|&p| turn(p)).collect();
    let label = |index: usize, points: &[Point]| {
        let points: Vec<Point> = points.iter().map(|&p| turn(p)).collect();
        (around.label)(index, &points).map(|r| turn_rect(&r))
    };
    let meets = |work: &[Vec<Point>], index: usize, at: Point| {
        let near = |p: &Point| (p.0 - at.0).abs() < SAME && (p.1 - at.1).abs() < SAME;
        taken.iter().any(near)
            || work.iter().enumerate().any(|(other, line)| {
                other != index && [line.first(), line.last()].into_iter().flatten().any(near)
            })
    };
    for index in 0..work.len() {
        if !around.back.get(index).copied().unwrap_or(false) {
            continue;
        }
        let Some(edge) = around.edges.get(index) else {
            continue;
        };
        for at_start in [true, false] {
            let node = if at_start { edge.from } else { edge.to };
            let Some(shape) = around.nodes.get(node) else {
                continue;
            };
            if !shape.round || edge.from == edge.to {
                continue;
            }
            let points = &work[index];
            let Some(&end) = (if at_start {
                points.first()
            } else {
                points.last()
            }) else {
                continue;
            };
            if !meets(&work, index, end) {
                continue;
            }
            // Everything the new way keeps off: the other boxes (a flat one
            // may take its end), the names, the other lines' labels.
            let others: Vec<(Rect, bool)> = boxes
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != node)
                .map(|(i, r)| (*r, around.nodes.get(i).is_some_and(|n| n.flat)))
                .collect();
            let mut solids: Vec<Rect> = names.clone();
            solids.extend(
                work.iter()
                    .enumerate()
                    .filter(|&(other, _)| other != index)
                    .filter_map(|(other, line)| label(other, line)),
            );
            let Some(moved) = sideways(points, boxes[node], at_start, &others) else {
                continue;
            };
            let start = if at_start {
                moved[0]
            } else {
                moved[moved.len() - 1]
            };
            let cut = |line: &[Point], r: Rect| {
                line.windows(2).any(|pair| route::cuts(r, pair[0], pair[1]))
            };
            let clear = !meets(&work, index, start)
                && !solids.iter().any(|&r| cut(&moved, r))
                && !frames.iter().any(|&r| cut(&moved, r) && !cut(points, r))
                && label(index, &moved).is_none_or(|own| {
                    !others.iter().any(|(r, _)| overlap(*r, own))
                        && !solids.iter().any(|&r| overlap(r, own))
                });
            if clear {
                work[index] = moved;
            }
        }
    }
    for (line, moved) in lines.iter_mut().zip(work) {
        *line = moved.into_iter().map(turn).collect();
    }
}

/// Whether two boxes overlap by more than a touch.
fn overlap(a: Rect, b: Rect) -> bool {
    a.x + 0.5 < b.x + b.w && b.x + 0.5 < a.x + a.w && a.y + 0.5 < b.y + b.h && b.y + 0.5 < a.y + a.h
}

/// `points` leaving `rect` (or arriving at it, when not `at_start`) from
/// the side the line heads for, at the middle of that side, then along
/// the flow where it ran before, or [`OUT`] past the side where it ran
/// nearer, the run moved out with it as long as it still ends on the side
/// of a flat box of `others`; `None` where the line runs straight on from
/// the point, steps across only at its far end, or the new way would cut
/// one of `others`.
fn sideways(
    points: &[Point],
    rect: Rect,
    at_start: bool,
    others: &[(Rect, bool)],
) -> Option<Vec<Point>> {
    let mut line: Vec<Point> = points.to_vec();
    if !at_start {
        line.reverse();
    }
    let start = *line.first()?;
    // The first run along the flow away from the point ends in a step
    // across; the run after it goes on along the flow.
    let step = line.iter().position(|p| (p.0 - start.0).abs() > SAME)?;
    let next = *line.get(step + 1)?;
    let track = line[step].0;
    let middle = rect.y + rect.h / 2.0;
    let (side, out) = if track > start.0 {
        let side = rect.x + rect.w;
        (side, (side + OUT).max(track))
    } else {
        (rect.x, (rect.x - OUT).min(track))
    };
    let mut moved = vec![(side, middle), (out, middle)];
    if (out - track).abs() < SAME {
        moved.extend(line[step + 1..].iter().copied());
    } else {
        moved.push((out, next.1));
        if step + 2 < line.len() {
            moved.extend(line[step + 2..].iter().copied());
        } else {
            // The run ends on a box: a flat one, on its side still, off
            // its corners.
            let on = others.iter().any(|&(r, flat)| {
                let border = (r.y - next.1).abs() < SAME || (r.y + r.h - next.1).abs() < SAME;
                flat && border && out > r.x + OUT && out < r.x + r.w - OUT
            });
            if !on {
                return None;
            }
        }
    }
    let blocked = moved.windows(2).any(|pair| {
        others
            .iter()
            .any(|&(r, _)| route::cuts(r, pair[0], pair[1]))
    });
    if blocked {
        return None;
    }
    if !at_start {
        moved.reverse();
    }
    Some(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn a_line_back_up_leaves_from_the_side_it_heads_for() {
        // A diamond from 100 to 140 across, 200 to 230 along; the line back
        // up from its top corner bends right to 180 and runs up.
        let diamond = rect(100.0, 200.0, 40.0, 30.0);
        let up = vec![
            (120.0, 200.0),
            (120.0, 188.0),
            (180.0, 188.0),
            (180.0, 40.0),
        ];
        let moved = sideways(&up, diamond, true, &[]).unwrap();
        assert_eq!(moved, [(140.0, 215.0), (180.0, 215.0), (180.0, 40.0)]);
        // Heading for a place within the box's width, its run moves out
        // past the corner, onto the flat box it ends at.
        let near = vec![
            (120.0, 200.0),
            (120.0, 188.0),
            (130.0, 188.0),
            (130.0, 40.0),
        ];
        let above = rect(100.0, 10.0, 120.0, 30.0);
        let moved = sideways(&near, diamond, true, &[(above, true)]).unwrap();
        assert_eq!(moved, [(140.0, 215.0), (152.0, 215.0), (152.0, 40.0)]);
        // Not where that leaves the box it ends at, or the box is round.
        assert!(sideways(&near, diamond, true, &[]).is_none());
        assert!(sideways(&near, diamond, true, &[(above, false)]).is_none());
        // A box in the new way: it stays.
        let wall = rect(160.0, 205.0, 40.0, 20.0);
        assert!(sideways(&up, diamond, true, &[(wall, true)]).is_none());
        // A step across at the far end alone: it stays.
        let short = vec![(120.0, 200.0), (120.0, 188.0), (180.0, 188.0)];
        assert!(sideways(&short, diamond, true, &[]).is_none());
    }

    fn node(round: bool) -> Node {
        Node {
            width: 40.0,
            height: 30.0,
            before: 0.0,
            loop_extra: 0.0,
            flat: !round,
            round,
        }
    }

    fn moved(lines: &mut [Vec<Point>], nodes: &[Node], taken: &[Point]) {
        let rects = [rect(100.0, 200.0, 40.0, 30.0), rect(140.0, 0.0, 80.0, 30.0)];
        let edges = [
            crate::graph::tests::edge(1, 0),
            crate::graph::tests::edge(0, 1),
            crate::graph::tests::edge(0, 1),
        ];
        apart(
            lines,
            &Around {
                edges: &edges,
                back: &[false, true, true],
                nodes,
                rects: &rects,
                frames: &[],
                names: &[],
                taken,
                label: &|_, _| None,
                direction: Direction::Down,
            },
        );
    }

    #[test]
    fn only_a_line_back_that_shares_its_point_moves_and_onto_a_free_one() {
        let into = vec![(120.0, 100.0), (120.0, 200.0)];
        let back = vec![
            (120.0, 200.0),
            (120.0, 188.0),
            (180.0, 188.0),
            (180.0, 30.0),
        ];
        let mut lines = vec![into.clone(), back.clone()];
        moved(&mut lines, &[node(true), node(false)], &[]);
        assert_eq!(lines[0], into);
        assert_eq!(lines[1], [(140.0, 215.0), (180.0, 215.0), (180.0, 30.0)]);
        // Alone at the corner, it keeps it.
        let mut lines = vec![vec![(120.0, 100.0), (110.0, 200.0)], back.clone()];
        moved(&mut lines, &[node(true), node(false)], &[]);
        assert_eq!(lines[1], back);
        // A box that is not round (an actor, as wide as its name) keeps it.
        let mut lines = vec![into.clone(), back.clone()];
        moved(&mut lines, &[node(false), node(false)], &[]);
        assert_eq!(lines[1], back);
        // The side point taken by a line within the rank: it stays.
        let mut lines = vec![into.clone(), back.clone()];
        moved(&mut lines, &[node(true), node(false)], &[(140.0, 215.0)]);
        assert_eq!(lines[1], back);
        // A second line back the same way finds the side taken by the
        // first, and stays.
        let mut lines = vec![into.clone(), back.clone(), back.clone()];
        moved(&mut lines, &[node(true), node(false)], &[]);
        assert_eq!(lines[1], [(140.0, 215.0), (180.0, 215.0), (180.0, 30.0)]);
        assert_eq!(lines[2], back);
    }
}
