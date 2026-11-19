//! Lines that meet one box as a tree: one stem into the box, the lines
//! joining it partway, as a class diagram draws its subclasses under
//! `skinparam groupInheritance` with one triangle on the parent.
//!
//! The boxes stay where the layout put them, and so does each line up to
//! where it reaches the junction, halfway into the gap beside the box.
//! There it turns onto a run across the flow to the stem, and the stem
//! goes on into the box: every line of the tree ends on the same few
//! pixels, so the heads there are one. A tree that would run along
//! another line or through a box is not drawn, and its lines stay as
//! they were.

use std::collections::HashMap;

use super::{Direction, Edge, Rect};

type Point = (f32, f32);

/// The stem is at least this long, to hold a head and show it is one.
const STEM: f32 = 28.0;
/// The part of the stem by the box that stays straight: a head of up to
/// 14 and some of the stem past it. A line's last run is all its last
/// corner's to round, so a point stands there and the curve of the
/// junction ends before it.
const KEEP: f32 = 18.0;
/// The junction keeps this far from a box beyond it.
const CLEAR: f32 = 8.0;
/// How far the rank beside a tree's box stands from it at least: the
/// stem, the curves at the junction and the runs on to the boxes.
const ROOM: f32 = 60.0;
/// How far into that gap the junction lies: past its middle, so the stem
/// is the longer part.
const DEPTH: f32 = 0.6;
/// How far a stem keeps from where another line ends on the same side:
/// two heads 14 wide side by side, and a little between them.
const HEADS: f32 = 16.0;
/// How far a stem keeps from the corners of its side.
const INSET: f32 = 8.0;
/// Where the junction may move to, off the middle of the gap, when a line
/// or a box is in the way there.
const TRIES: [f32; 5] = [0.0, -6.0, 6.0, -12.0, 12.0];

/// A line of a tree ([`Edge::tree`]): the lines of one key that meet one
/// box at the end it names share their stem into it. The key tells the
/// trees into one box apart, a byte, so an edge stays as small as it was:
/// the layout copies its edges often.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tree {
    pub key: u8,
    /// The end the stem is at: the line's first (`from`) or its last.
    pub at_from: bool,
}

/// The lines of one tree into one box: its key, the box, and whether they
/// meet the side the flow leaves it by.
type TreeKey = (u8, usize, bool);

/// A point with its coordinate along the flow first.
fn turned(point: Point, direction: Direction) -> Point {
    match direction {
        Direction::Down => (point.1, point.0),
        Direction::Right => point,
    }
}

/// A box as (along start, along end, across start, across end).
fn spans(rect: Rect, direction: Direction) -> (f32, f32, f32, f32) {
    match direction {
        Direction::Down => (rect.y, rect.bottom(), rect.x, rect.right()),
        Direction::Right => (rect.x, rect.right(), rect.y, rect.bottom()),
    }
}

/// Widens each gap between the rank of a tree's box and the rank its lines
/// come from to [`ROOM`]. `chains` are the slots each line passes, `rows`
/// the slots of each rank; the first `boxes` slots are the boxes, by
/// their index, and an end past them (a frame's) is no tree's.
pub(super) fn room(
    gaps: &mut [f32],
    edges: &[Edge],
    chains: &[Vec<usize>],
    rows: &[Vec<usize>],
    boxes: usize,
) {
    if edges.iter().all(|edge| edge.tree.is_none()) {
        return;
    }
    let mut row_of = HashMap::new();
    for (row, slots) in rows.iter().enumerate() {
        for &slot in slots {
            row_of.insert(slot, row);
        }
    }
    for (index, edge) in edges.iter().enumerate() {
        let (Some(tree), Some(chain)) = (edge.tree, chains.get(index)) else {
            continue;
        };
        if edge.hidden || edge.level.is_some() || edge.from == edge.to || chain.len() < 2 {
            continue;
        }
        let at = if tree.at_from { edge.from } else { edge.to };
        if at >= boxes {
            continue;
        }
        let next = if chain[0] == at {
            chain[1]
        } else if chain[chain.len() - 1] == at {
            chain[chain.len() - 2]
        } else {
            continue;
        };
        if let (Some(&row), Some(&beside)) = (row_of.get(&at), row_of.get(&next))
            && row.abs_diff(beside) == 1
            && let Some(gap) = gaps.get_mut(row.min(beside))
        {
            *gap = gap.max(ROOM);
        }
    }
}

/// Joins the lines of each tree in `lines`, the routes of `edges` over
/// boxes `rects`. Points are turned so the flow runs along the first
/// coordinate, joined, and turned back.
pub(super) fn trees(
    lines: &mut [Vec<Point>],
    edges: &[Edge],
    rects: &[Rect],
    direction: Direction,
) {
    if edges.iter().all(|edge| edge.tree.is_none()) {
        return;
    }
    let mut groups: HashMap<TreeKey, Vec<(usize, bool)>> = HashMap::new();
    let mut order = Vec::new();
    for (index, edge) in edges.iter().enumerate() {
        let Some(tree) = edge.tree else { continue };
        let at = if tree.at_from { edge.from } else { edge.to };
        let points = &lines[index];
        // A line within a rank keeps a route of its own.
        if edge.hidden
            || edge.level.is_some()
            || edge.from == edge.to
            || at >= rects.len()
            || points.len() < 2
        {
            continue;
        }
        let (start, end) = (points[0], points[points.len() - 1]);
        let on = |point: Point| inside(rects[at], point);
        // Which end of the route the box is at, whichever way it was laid.
        let reversed = match (on(start), on(end)) {
            (_, true) => false,
            (true, false) => true,
            _ => continue,
        };
        let tip = turned(if reversed { start } else { end }, direction);
        let (a0, a1, ..) = spans(rects[at], direction);
        let after = if (tip.0 - a1).abs() < 1.0 {
            true
        } else if (tip.0 - a0).abs() < 1.0 {
            false
        } else {
            continue;
        };
        let key = (tree.key, at, after);
        if !groups.contains_key(&key) {
            order.push(key);
        }
        groups.entry(key).or_default().push((index, reversed));
    }
    // Each tree joined, with its stem across the flow and its lines as
    // they were; and each that stays as it was.
    let mut joined_trees: Vec<(TreeKey, f32, Vec<Vec<Point>>)> = Vec::new();
    let mut unjoined = Vec::new();
    for (at_key, key) in order.iter().enumerate() {
        let group = &groups[key];
        let (_, at, after) = *key;
        // The lines of the trees into this side still to come end where
        // their own stem will be.
        let later: Vec<usize> = order[at_key + 1..]
            .iter()
            .filter(|other| other.1 == at && other.2 == after)
            .flat_map(|other| groups[other].iter().map(|&(index, _)| index))
            .collect();
        let tree = (group.len() >= 2)
            .then(|| join(lines, group, &later, rects, at, after, direction))
            .flatten();
        let Some((joined, stem)) = tree else {
            unjoined.push(*key);
            continue;
        };
        let before = group
            .iter()
            .map(|&(index, _)| lines[index].clone())
            .collect();
        for ((index, reversed), mut points) in group.iter().zip(joined) {
            if *reversed {
                points.reverse();
            }
            lines[*index] = points;
        }
        joined_trees.push((*key, stem, before));
    }
    // A tree that could not join keeps its heads where they were: a stem
    // laid next to one of them goes back to its own lines.
    for key in unjoined {
        let (_, at, after) = key;
        let ends: Vec<f32> = groups[&key]
            .iter()
            .filter_map(|&(index, reversed)| {
                let points = &lines[index];
                let end = if reversed {
                    points.first()
                } else {
                    points.last()
                };
                end.map(|&p| turned(p, direction).1)
            })
            .collect();
        for (joined, stem, before) in &mut joined_trees {
            let near = ends.iter().any(|end| (end - *stem).abs() < HEADS);
            if joined.1 == at && joined.2 == after && near && !before.is_empty() {
                for (&(index, _), points) in groups[joined].iter().zip(before.drain(..)) {
                    lines[index] = points;
                }
            }
        }
    }
}

/// Whether `point` lies on the border of `rect`, or in it.
fn inside(rect: Rect, point: Point) -> bool {
    point.0 >= rect.x - 1.0
        && point.0 <= rect.right() + 1.0
        && point.1 >= rect.y - 1.0
        && point.1 <= rect.bottom() + 1.0
}

/// The routes of `group`, each ending at box `at` on its side `after`
/// the flow (or before it), joined into one stem, and where the stem
/// stands across the flow; `None` when the tree would run along another
/// line or through a box, or end by another line's head. The lines of
/// `later` are other trees' and move to stems of their own.
fn join(
    lines: &[Vec<Point>],
    group: &[(usize, bool)],
    later: &[usize],
    rects: &[Rect],
    at: usize,
    after: bool,
    direction: Direction,
) -> Option<(Vec<Vec<Point>>, f32)> {
    let sign = if after { 1.0 } else { -1.0 };
    let (a0, a1, c0, c1) = spans(rects[at], direction);
    let edge = if after { a1 } else { a0 };
    // Each route turned, from its far end to the box.
    let routes: Vec<Vec<Point>> = group
        .iter()
        .map(|&(index, reversed)| {
            let mut points: Vec<Point> =
                lines[index].iter().map(|&p| turned(p, direction)).collect();
            if reversed {
                points.reverse();
            }
            points
        })
        .collect();
    let members: Vec<usize> = group.iter().map(|&(index, _)| index).collect();
    // The others, turned: what the tree must not run along.
    let others: Vec<Vec<Point>> = lines
        .iter()
        .enumerate()
        .filter(|(index, _)| !members.contains(index))
        .map(|(_, points)| points.iter().map(|&p| turned(p, direction)).collect())
        .collect();
    // Where the others end on this side of the box, across the flow: a
    // stem there would stack its head on theirs.
    let heads: Vec<f32> = lines
        .iter()
        .enumerate()
        .filter(|(index, _)| !members.contains(index) && !later.contains(index))
        .flat_map(|(_, points)| [points.first(), points.last()])
        .flatten()
        .map(|&p| turned(p, direction))
        .filter(|p| (p.0 - edge).abs() < 1.0 && p.1 > c0 - 1.0 && p.1 < c1 + 1.0)
        .map(|p| p.1)
        .collect();
    let boxes: Vec<(f32, f32, f32, f32)> = rects
        .iter()
        .enumerate()
        .filter(|&(index, _)| index != at)
        .map(|(_, &rect)| spans(rect, direction))
        .collect();
    // The gap beside the box: up to the nearest box beyond it, across
    // from what the lines take.
    let lo = routes.iter().flatten().map(|p| p.1).fold(c0, f32::min);
    let hi = routes.iter().flatten().map(|p| p.1).fold(c1, f32::max);
    let gap = boxes
        .iter()
        .filter(|b| b.3 > lo && b.2 < hi)
        .map(|b| if after { b.0 - edge } else { edge - b.1 })
        .filter(|&d| d > 0.5)
        .fold(f32::INFINITY, f32::min);
    let reach = routes
        .iter()
        .map(|route| sign * (route[0].0 - edge))
        .fold(f32::INFINITY, f32::min);
    let gap = gap.min(reach);
    if !gap.is_finite() || gap < STEM + CLEAR {
        return None;
    }
    let middle = (gap * DEPTH).max(STEM).min(gap - CLEAR);
    // The stem: on a line that runs straight into the box, nearest its
    // middle; else at the middle of the side; else where one of its lines
    // ends, nearest the middle first; else a step further out each time.
    let center = (c0 + c1) / 2.0;
    let nearest = |a: &f32, b: &f32| (a - center).abs().total_cmp(&(b - center).abs());
    let straight = routes
        .iter()
        .filter(|route| route.iter().all(|p| (p.1 - route[0].1).abs() < 0.5))
        .map(|route| route[0].1)
        .filter(|&c| c > c0 && c < c1)
        .min_by(nearest);
    let mut own: Vec<f32> = routes
        .iter()
        .map(|route| route[route.len() - 1].1)
        .collect();
    own.sort_by(nearest);
    let steps = (1..)
        .map(|step| step as f32 * HEADS)
        .take_while(|&off| off < (c1 - c0) / 2.0);
    let stems: Vec<f32> = straight
        .into_iter()
        .chain([center])
        .chain(own)
        .chain(steps.flat_map(|off| [center - off, center + off]))
        .filter(|&c| c >= c0 + INSET && c <= c1 - INSET)
        .filter(|&c| heads.iter().all(|head| (head - c).abs() >= HEADS))
        .collect();
    for stem in stems {
        for shift in TRIES {
            let d = middle + shift;
            if d < STEM || d > gap - CLEAR {
                continue;
            }
            let junction = edge + sign * d;
            let Some(joined) = routes
                .iter()
                .map(|route| onto(route, junction, stem, edge, sign))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            if clear(&joined, &others, &boxes, junction, stem, edge) {
                let back = joined
                    .into_iter()
                    .map(|route| route.into_iter().map(|p| turned(p, direction)).collect())
                    .collect();
                return Some((back, stem));
            }
        }
    }
    None
}

/// `route` up to where it last reaches `junction` along the flow, then
/// across to `stem` and along it to the box's side at `edge`.
fn onto(route: &[Point], junction: f32, stem: f32, edge: f32, sign: f32) -> Option<Vec<Point>> {
    let beyond = |p: Point| sign * (p.0 - junction) > 0.0;
    let crossing = (0..route.len() - 1)
        .rev()
        .find(|&k| beyond(route[k]) && !beyond(route[k + 1]))?;
    let (p, q) = (route[crossing], route[crossing + 1]);
    let t = if (q.0 - p.0).abs() < f32::EPSILON {
        0.0
    } else {
        (junction - p.0) / (q.0 - p.0)
    };
    let meet = (junction, p.1 + (q.1 - p.1) * t);
    let mut out: Vec<Point> = route[..=crossing].to_vec();
    if (meet.1 - stem).abs() > 0.5 {
        out.push(meet);
    }
    out.push((junction, stem));
    if (junction - edge).abs() > KEEP + 0.5 {
        out.push((edge + sign * KEEP, stem));
    }
    out.push((edge, stem));
    out.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01);
    Some(out)
}

/// Whether the runs the tree adds keep off `others`' runs and out of
/// `boxes`: the run across at `junction` and the stem along `stem`.
fn clear(
    joined: &[Vec<Point>],
    others: &[Vec<Point>],
    boxes: &[(f32, f32, f32, f32)],
    junction: f32,
    stem: f32,
    edge: f32,
) -> bool {
    let lo = joined
        .iter()
        .flatten()
        .filter(|p| (p.0 - junction).abs() < 0.01)
        .map(|p| p.1)
        .fold(stem, f32::min);
    let hi = joined
        .iter()
        .flatten()
        .filter(|p| (p.0 - junction).abs() < 0.01)
        .map(|p| p.1)
        .fold(stem, f32::max);
    let (s0, s1) = (junction.min(edge), junction.max(edge));
    // The run across passes no box, nor does the stem.
    let boxed = boxes.iter().any(|&(a0, a1, c0, c1)| {
        (junction > a0 - 2.0 && junction < a1 + 2.0 && hi > c0 && lo < c1)
            || (stem > c0 - 2.0 && stem < c1 + 2.0 && s1 > a0 && s0 < a1)
    });
    if boxed {
        return false;
    }
    // No other line runs along either of them.
    !others.iter().any(|points| {
        points.windows(2).any(|pair| {
            let (p, q) = (pair[0], pair[1]);
            let across = (p.0 - junction).abs() < 3.0
                && (q.0 - junction).abs() < 3.0
                && p.1.max(q.1) > lo + 1.0
                && p.1.min(q.1) < hi - 1.0;
            let along = (p.1 - stem).abs() < 3.0
                && (q.1 - stem).abs() < 3.0
                && p.0.max(q.0) > s0 + 1.0
                && p.0.min(q.0) < s1 - 1.0;
            across || along
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn a_tree_makes_no_edge_larger() {
        // The bytes a line of a tree takes fit in what an edge had spare.
        assert!(
            std::mem::size_of::<Edge>() <= 40,
            "{}",
            std::mem::size_of::<Edge>()
        );
    }

    #[test]
    fn three_lines_into_one_box_meet_in_one_stem() {
        // A parent over three children, the lines laid from each child up
        // to the parent's bottom, each on a track of its own.
        let rects = [
            rect(100.0, 0.0, 80.0, 30.0),
            rect(0.0, 100.0, 60.0, 30.0),
            rect(110.0, 100.0, 60.0, 30.0),
            rect(220.0, 100.0, 60.0, 30.0),
        ];
        let mut lines = vec![
            vec![(30.0, 100.0), (30.0, 60.0), (120.0, 60.0), (120.0, 30.0)],
            vec![(140.0, 100.0), (140.0, 30.0)],
            vec![(250.0, 100.0), (250.0, 70.0), (160.0, 70.0), (160.0, 30.0)],
        ];
        let tree = Some(Tree {
            key: 7,
            at_from: false,
        });
        let edges: Vec<Edge> = (1..=3)
            .map(|child| Edge {
                from: child,
                to: 0,
                tree,
                ..crate::graph::tests::edge(child, 0)
            })
            .collect();
        trees(&mut lines, &edges, &rects, Direction::Down);
        // Every line ends on the stem of the straight one, at x 140.
        for line in &lines {
            let last = line[line.len() - 1];
            assert_eq!(last, (140.0, 30.0), "{line:?}");
            // The stem stays straight by the box, past the head.
            assert_eq!(line[line.len() - 2], (140.0, 30.0 + KEEP), "{line:?}");
        }
        // They join past the middle of the gap, at one height.
        let junction = lines[0][lines[0].len() - 3].1;
        assert!(
            (junction - (30.0 + 70.0 * DEPTH)).abs() < 0.01,
            "{junction}"
        );
        assert_eq!(lines[2][lines[2].len() - 3], (140.0, junction));
        // The first keeps its run up from the child to the junction.
        assert_eq!(lines[0][0], (30.0, 100.0));
        assert_eq!(lines[0][1], (30.0, junction));
    }

    #[test]
    fn a_tree_that_would_run_along_another_line_is_not_drawn() {
        let rects = [
            rect(100.0, 0.0, 80.0, 30.0),
            rect(0.0, 100.0, 60.0, 30.0),
            rect(220.0, 100.0, 60.0, 30.0),
        ];
        let before = vec![
            vec![(30.0, 100.0), (30.0, 60.0), (130.0, 60.0), (130.0, 30.0)],
            vec![(250.0, 100.0), (250.0, 70.0), (150.0, 70.0), (150.0, 30.0)],
            // Another line across the whole gap, wherever the junction
            // could go.
            vec![(0.0, 41.0), (300.0, 41.0), (300.0, 89.0), (0.0, 89.0)],
        ];
        let mut edges: Vec<Edge> = [1, 2]
            .into_iter()
            .map(|child| Edge {
                tree: Some(Tree {
                    key: 1,
                    at_from: false,
                }),
                ..crate::graph::tests::edge(child, 0)
            })
            .collect();
        edges.push(crate::graph::tests::edge(1, 2));
        let mut lines = before.clone();
        // The other line runs across at 41 and 89 only; move it onto
        // every height a junction may take.
        lines[2] = (0..=20)
            .flat_map(|step| {
                let y = 40.0 + step as f32 * 2.5;
                [(0.0, y), (300.0, y)]
            })
            .collect();
        let others = lines[2].clone();
        trees(&mut lines, &edges, &rects, Direction::Down);
        assert_eq!(lines[0], before[0]);
        assert_eq!(lines[1], before[1]);
        assert_eq!(lines[2], others);
    }

    /// The triangles of a picture, each as its points.
    fn triangles(svg: &str) -> Vec<&str> {
        svg.split("<polygon points=\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .collect()
    }

    #[test]
    fn under_group_inheritance_the_subclasses_share_one_triangle() {
        let body = "class Base\nBase <|-- A\nBase <|-- B\nBase <|-- C\n";
        for flow in ["", "left to right direction\n"] {
            let alone = crate::render(&format!("@startuml\n{flow}{body}@enduml\n")).unwrap();
            let grouped = crate::render(&format!(
                "@startuml\nskinparam groupInheritance 2\n{flow}{body}@enduml\n"
            ))
            .unwrap();
            let mut distinct = triangles(&grouped);
            distinct.dedup();
            assert_eq!(distinct.len(), 1, "{flow}{grouped}");
            assert_eq!(triangles(&alone).len(), 3);
            assert_ne!(triangles(&alone)[0], triangles(&alone)[2], "{flow}{alone}");
        }
        // Fewer lines than it asks for, and a line with words, keep their
        // own triangles.
        let three = crate::render(&format!(
            "@startuml\nskinparam groupInheritance 4\n{body}@enduml\n"
        ))
        .unwrap();
        let mut distinct = triangles(&three);
        distinct.dedup();
        assert_eq!(distinct.len(), 3, "{three}");
        let worded = crate::render(
            "@startuml\nskinparam groupInheritance 2\nclass Base\nBase <|-- A\nBase <|-- B\nBase <|-- C : odd\n@enduml\n",
        )
        .unwrap();
        let mut distinct = triangles(&worded);
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 2, "{worded}");
    }

    #[test]
    fn two_trees_into_one_box_stand_apart() {
        // The solid lines and the dashed ones each under a triangle of
        // their own, the two side by side, not on top of each other.
        let svg = crate::render(
            "@startuml\nskinparam groupInheritance 2\nclass Base\nBase <|-- A\nBase <|-- B\n\
             Base <|.. C\nBase <|.. D\n@enduml\n",
        )
        .unwrap();
        let mut tips: Vec<f32> = triangles(&svg)
            .into_iter()
            .map(|points| {
                let tip = points.split(' ').nth(1).unwrap();
                tip.split(',').next().unwrap().parse().unwrap()
            })
            .collect();
        tips.sort_by(f32::total_cmp);
        tips.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        assert_eq!(tips.len(), 2, "{svg}");
        assert!(tips[1] - tips[0] >= HEADS, "{tips:?}");
    }
}
