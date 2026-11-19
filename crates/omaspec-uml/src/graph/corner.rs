//! A line back against the flow keeps off the corner another line takes
//! (F8 of the review of the three diagrams). A box that is not flat (a
//! choice's diamond, a circle) meets its lines at one point of each side:
//! a line arriving from above and a line leaving back up from it met at
//! its top corner, and the two read as one line that splits. Where a line
//! back against the flow starts or ends at the point another line meets,
//! it takes the side the way it heads for instead: out of the side corner
//! across, then along the flow, as a choice's branches leave a diamond.

use super::route::{self, Point};
use super::{Direction, Edge, Node, Rect};

/// Two ends this close meet at one point.
const SAME: f32 = 0.5;
/// How far past the box's side a line must run along the flow to leave
/// from that side.
const OUT: f32 = 12.0;

/// `lines` with each line back against the flow (`back`) moved off a
/// point of a box that is not flat which another line meets.
pub(super) fn apart(
    lines: &mut [Vec<Point>],
    edges: &[Edge],
    back: &[bool],
    nodes: &[Node],
    rects: &[Rect],
    direction: Direction,
) {
    // Laid out down the page; a flow to the right is turned to it and back.
    let turned = direction == Direction::Right;
    let turn = |p: Point| (p.1, p.0);
    let mut work: Vec<Vec<Point>> = if turned {
        lines
            .iter()
            .map(|l| l.iter().map(|&p| turn(p)).collect())
            .collect()
    } else {
        lines.to_vec()
    };
    let boxes: Vec<Rect> = rects
        .iter()
        .map(|r| {
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
        })
        .collect();
    for index in 0..work.len() {
        if !back.get(index).copied().unwrap_or(false) {
            continue;
        }
        let Some(edge) = edges.get(index) else {
            continue;
        };
        for at_start in [true, false] {
            let node = if at_start { edge.from } else { edge.to };
            if node >= nodes.len() || nodes[node].flat || edge.from == edge.to {
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
            let shared = work.iter().enumerate().any(|(other, line)| {
                other != index
                    && [line.first(), line.last()]
                        .into_iter()
                        .flatten()
                        .any(|p| (p.0 - end.0).abs() < SAME && (p.1 - end.1).abs() < SAME)
            });
            if !shared {
                continue;
            }
            let others: Vec<Rect> = boxes
                .iter()
                .enumerate()
                .filter(|&(i, _)| i != node)
                .map(|(_, r)| *r)
                .collect();
            if let Some(moved) = sideways(points, boxes[node], at_start, &others) {
                work[index] = moved;
            }
        }
    }
    for (line, moved) in lines.iter_mut().zip(work) {
        *line = if turned {
            moved.into_iter().map(turn).collect()
        } else {
            moved
        };
    }
}

/// `points` leaving `rect` (or arriving at it, when not `at_start`) from
/// the side the line heads for, at the middle of that side, then along
/// the flow where it ran before, or [`OUT`] past the side where it ran
/// nearer, the run moved out with it as long as it still ends on the side
/// of the box it ends at; `None` where the line runs straight on from the
/// point, or the new way would cut one of `others`.
fn sideways(points: &[Point], rect: Rect, at_start: bool, others: &[Rect]) -> Option<Vec<Point>> {
    let mut line: Vec<Point> = points.to_vec();
    if !at_start {
        line.reverse();
    }
    let start = *line.first()?;
    // The first run along the flow away from the point ends in a step
    // across; the run after it goes on along the flow.
    let step = line.iter().position(|p| (p.0 - start.0).abs() > SAME)?;
    let track = line[step].0;
    let middle = rect.y + rect.h / 2.0;
    let rightward = track > start.0;
    let (side, out) = if rightward {
        let side = rect.x + rect.w;
        (side, (side + OUT).max(track))
    } else {
        (rect.x, (rect.x - OUT).min(track))
    };
    let mut moved = vec![(side, middle), (out, middle)];
    if (out - track).abs() < SAME {
        moved.extend(line[step + 1..].iter().copied());
    } else {
        let next = *line.get(step + 1)?;
        moved.push((out, next.1));
        if step + 2 < line.len() {
            moved.extend(line[step + 2..].iter().copied());
        } else {
            // The run ends on a box: on its side still, off its corners.
            let on = others.iter().any(|r| {
                let border = (r.y - next.1).abs() < SAME || (r.y + r.h - next.1).abs() < SAME;
                border && out > r.x + OUT && out < r.x + r.w - OUT
            });
            if !on {
                return None;
            }
        }
    }
    let blocked = moved
        .windows(2)
        .any(|pair| others.iter().any(|&r| route::cuts(r, pair[0], pair[1])));
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
        // past the corner, onto the box it ends at.
        let near = vec![
            (120.0, 200.0),
            (120.0, 188.0),
            (130.0, 188.0),
            (130.0, 40.0),
        ];
        let above = rect(100.0, 10.0, 120.0, 30.0);
        let moved = sideways(&near, diamond, true, &[above]).unwrap();
        assert_eq!(moved, [(140.0, 215.0), (152.0, 215.0), (152.0, 40.0)]);
        // Where that would leave the box it ends at, it stays.
        assert!(sideways(&near, diamond, true, &[]).is_none());
        // A box in the new way: it stays.
        let wall = rect(160.0, 205.0, 40.0, 20.0);
        assert!(sideways(&up, diamond, true, &[wall]).is_none());
    }

    #[test]
    fn only_a_line_back_that_shares_its_point_moves() {
        let nodes = [
            Node {
                width: 40.0,
                height: 30.0,
                before: 0.0,
                loop_extra: 0.0,
                flat: false,
            },
            Node {
                width: 80.0,
                height: 30.0,
                before: 0.0,
                loop_extra: 0.0,
                flat: true,
            },
        ];
        let rects = [rect(100.0, 200.0, 40.0, 30.0), rect(140.0, 0.0, 80.0, 30.0)];
        let edges = [
            crate::graph::tests::edge(1, 0),
            crate::graph::tests::edge(0, 1),
        ];
        let into = vec![(120.0, 100.0), (120.0, 200.0)];
        let back = vec![
            (120.0, 200.0),
            (120.0, 188.0),
            (180.0, 188.0),
            (180.0, 30.0),
        ];
        let mut lines = vec![into.clone(), back.clone()];
        apart(
            &mut lines,
            &edges,
            &[false, true],
            &nodes,
            &rects,
            Direction::Down,
        );
        assert_eq!(lines[0], into);
        assert_eq!(lines[1], [(140.0, 215.0), (180.0, 215.0), (180.0, 30.0)]);
        // Alone at the corner, it keeps it.
        let mut lines = vec![vec![(120.0, 100.0), (110.0, 200.0)], back.clone()];
        apart(
            &mut lines,
            &edges,
            &[false, true],
            &nodes,
            &rects,
            Direction::Down,
        );
        assert_eq!(lines[1], back);
    }
}
