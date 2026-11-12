//! Lines that arrive at one side of a box share a trunk
//! ([0022](../../../../docs/adr/0022-lines-bundled.md)), as Graphviz's
//! `concentrate` shares the span of edges that end together.
//!
//! The points of a line stay where the layout put them, so no box moves.
//! A line with words on it keeps its own arrival: those words sit on the
//! run beside the box ([0020](../../../../docs/adr/0020-room-for-labels.md)),
//! and several of them on one trunk would pile up. A mark at an end, a
//! multiplicity, sits on the stem and stays off the trunk for the same
//! reason. What is shared, for a line that carries neither, is the last
//! stem, the few pixels beside the box. Each such line keeps the run it
//! has through the ranks and turns onto the trunk only as it arrives. A
//! line that leaves a box stays on its own, so two boxes that overlap
//! across the flow are still joined by a straight line. Lines share a
//! trunk only when they arrive alike ([`Edge::arrival`]): the same head,
//! color, width and dash, so the heads that stack there are one.

use super::{Direction, Edge, Node, Rect, Slot};

type Point = (f32, f32);

/// A new run shorter than this would count as a jog.
const LEAST: f32 = 8.0;
/// How far the shared run stays from the box when a line had no bend.
const STEM: f32 = 12.0;
/// How far a trunk meets a flat side from its corners.
const INSET: f32 = 10.0;

/// Which side of a box a line arrives on: the one the flow reaches first
/// (`Before`: the top, or the left), or the one it leaves by.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Before,
    After,
}

/// The lines as the layout left them, and what it knows about each one.
pub(super) struct Drawn<'a> {
    pub(super) routes: &'a mut [Vec<Point>],
    pub(super) edges: &'a [Edge],
    pub(super) chains: &'a [Vec<usize>],
    pub(super) rows: &'a [Vec<usize>],
    pub(super) slots: &'a [Slot],
    pub(super) rects: &'a [Rect],
    pub(super) nodes: &'a [Node],
    pub(super) back: &'a [bool],
    pub(super) direction: Direction,
}

/// Gives the lines of `drawn` that arrive together one trunk. The lines
/// are in the order of the edges, already freed of small steps.
pub(super) fn share(drawn: &mut Drawn<'_>) {
    // Each arriving line, by where it arrives and how it looks there.
    let arrivals: Vec<(usize, (usize, Side, u64))> = (0..drawn.edges.len())
        .filter_map(|index| {
            if !arrives(index, drawn.edges, drawn.chains, drawn.nodes) {
                return None;
            }
            let edge = &drawn.edges[index];
            let side = arrival_side(&drawn.routes[index], drawn.rects[edge.to], drawn.direction)?;
            Some((index, (edge.to, side, edge.arrival)))
        })
        .collect();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut seen = vec![false; arrivals.len()];
    for (at, &(index, how)) in arrivals.iter().enumerate() {
        if seen[at] {
            continue;
        }
        let mut group = vec![index];
        seen[at] = true;
        for (other_at, &(other, other_how)) in arrivals.iter().enumerate().skip(at + 1) {
            if seen[other_at] || other_how != how {
                continue;
            }
            group.push(other);
            seen[other_at] = true;
        }
        if group.len() >= 2 {
            groups.push(group);
        }
    }
    for group in groups {
        share_group(&group, drawn);
    }
}

/// A drawn line into a flat box, with a point of its own beside that box.
fn arrives(index: usize, edges: &[Edge], chains: &[Vec<usize>], nodes: &[Node]) -> bool {
    let edge = &edges[index];
    edge.to < nodes.len()
        && edge.from != edge.to
        && !edge.hidden
        && !edge.apart
        && !edge.marked
        && edge.label_width == 0.0
        && edge.label_height == 0.0
        && nodes[edge.to].flat
        && chains[index].len() >= 3
}

fn arrival_side(points: &[Point], rect: Rect, direction: Direction) -> Option<Side> {
    let point = *points.last()?;
    let (across, along, start, end, lo, hi) = match direction {
        Direction::Down => (
            point.0,
            point.1,
            rect.y,
            rect.bottom(),
            rect.x,
            rect.right(),
        ),
        Direction::Right => (
            point.1,
            point.0,
            rect.x,
            rect.right(),
            rect.y,
            rect.bottom(),
        ),
    };
    if across < lo - 1.0 || across > hi + 1.0 {
        return None;
    }
    if (along - start).abs() < 1.0 {
        Some(Side::Before)
    } else if (along - end).abs() < 1.0 {
        Some(Side::After)
    } else {
        None
    }
}

/// The point of `edge` in the rank beside the box it arrives at.
fn neighbor(edge: usize, chains: &[Vec<usize>], back: &[bool]) -> usize {
    let chain = &chains[edge];
    if back[edge] {
        chain[1]
    } else {
        chain[chain.len() - 2]
    }
}

fn share_group(group: &[usize], drawn: &mut Drawn<'_>) {
    let target = drawn.edges[group[0]].to;
    let rect = drawn.rects[target];
    let Some(side) = arrival_side(&drawn.routes[group[0]], rect, drawn.direction) else {
        return;
    };
    // Already one arrival: nothing to share.
    let heads: Vec<f32> = group
        .iter()
        .map(|&index| {
            across(
                *drawn.routes[index].last().unwrap_or(&(0.0, 0.0)),
                drawn.direction,
            )
        })
        .collect();
    let (lo, hi) = span(&heads);
    if hi - lo < 0.5 {
        return;
    }
    let mut placed: Vec<(usize, usize)> = Vec::new();
    for &index in group {
        let slot = neighbor(index, drawn.chains, drawn.back);
        let Some(row) = drawn.rows.iter().position(|row| row.contains(&slot)) else {
            return;
        };
        let Some(pos) = drawn.rows[row].iter().position(|&at| at == slot) else {
            return;
        };
        placed.push((pos, index));
    }
    // One row beside the box. Anything between two points keeps them apart.
    if placed.iter().any(|&(_, index)| {
        let slot = neighbor(index, drawn.chains, drawn.back);
        drawn.rows.iter().position(|row| row.contains(&slot))
            != drawn
                .rows
                .iter()
                .position(|row| row.contains(&neighbor(group[0], drawn.chains, drawn.back)))
    }) {
        return;
    }
    placed.sort_by_key(|&(pos, _)| pos);
    let mut run = Vec::new();
    let mut runs = Vec::new();
    for (at, &(pos, index)) in placed.iter().enumerate() {
        if at > 0 && pos > placed[at - 1].0 + 1 {
            runs.push(std::mem::take(&mut run));
        }
        run.push(index);
    }
    runs.push(run);
    for run in runs {
        // Two lines between the same boxes stay side by side, each with
        // its own arrow and its label.
        let mut count = std::collections::HashMap::<usize, usize>::new();
        for &index in &run {
            *count.entry(drawn.edges[index].from).or_default() += 1;
        }
        let run: Vec<usize> = run
            .into_iter()
            .filter(|&index| count[&drawn.edges[index].from] == 1)
            .collect();
        let mut chunks = Vec::new();
        let mut chunk = Vec::new();
        for (at, &index) in run.iter().enumerate() {
            if at > 0 {
                let prev = neighbor(run[at - 1], drawn.chains, drawn.back);
                let slot = neighbor(index, drawn.chains, drawn.back);
                let row = drawn.rows.iter().find(|row| row.contains(&slot));
                let gap = row
                    .and_then(|row| {
                        let a = row.iter().position(|&at| at == prev)?;
                        let b = row.iter().position(|&at| at == slot)?;
                        Some(b - a)
                    })
                    .unwrap_or(2);
                if gap != 1 {
                    chunks.push(std::mem::take(&mut chunk));
                }
            }
            chunk.push(index);
        }
        if !chunk.is_empty() {
            chunks.push(chunk);
        }
        for chunk in chunks.into_iter().filter(|chunk| chunk.len() >= 2) {
            lay_trunk(&chunk, rect, side, drawn);
        }
    }
}

fn span(values: &[f32]) -> (f32, f32) {
    values
        .iter()
        .copied()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        })
}

/// One trunk for `chunk`, at the point of the line in the middle of the
/// row, clamped to the side of the box.
fn lay_trunk(chunk: &[usize], rect: Rect, side: Side, drawn: &mut Drawn<'_>) {
    let mut order: Vec<usize> = chunk.to_vec();
    order.sort_by(|&a, &b| {
        super::center(&drawn.slots[neighbor(a, drawn.chains, drawn.back)]).total_cmp(
            &super::center(&drawn.slots[neighbor(b, drawn.chains, drawn.back)]),
        )
    });
    let mid = order[order.len() / 2];
    let trunk = clamp(
        super::center(&drawn.slots[neighbor(mid, drawn.chains, drawn.back)]),
        rect,
        drawn.direction,
    );
    // A line too close to move would be crossed by the others on their way
    // to the trunk. It stays, and it splits the row.
    let mut blocked = false;
    let mut sub = Vec::new();
    let mut subs = Vec::new();
    for &index in &order {
        let shift = (across(
            *drawn.routes[index].last().unwrap_or(&(0.0, 0.0)),
            drawn.direction,
        ) - trunk)
            .abs();
        if (0.5..LEAST).contains(&shift) {
            blocked = true;
            if sub.len() >= 2 {
                subs.push(std::mem::take(&mut sub));
            } else {
                sub.clear();
            }
            continue;
        }
        sub.push(index);
    }
    if sub.len() >= 2 {
        subs.push(sub);
    }
    if blocked {
        for sub in subs {
            lay_trunk(&sub, rect, side, drawn);
        }
        return;
    }
    // The trunk is drawn only when it crosses nothing that is staying. A
    // line that would cut another keeps the arrival it had.
    let before = involved(drawn.routes, &order);
    let mut draft = drawn.routes.to_vec();
    for &index in &order {
        let join = join_at(rect, side, drawn.direction);
        retarget(&mut draft[index], trunk, join, drawn.direction);
    }
    if involved(&draft, &order) > before {
        return;
    }
    for &index in &order {
        drawn.routes[index].clone_from(&draft[index]);
    }
}

/// Crossings of the lines in `changed` with each other and with the rest.
fn involved(routes: &[Vec<Point>], changed: &[usize]) -> usize {
    let mut count = 0;
    for (n, &index) in changed.iter().enumerate() {
        for &other in &changed[n + 1..] {
            count += cuts(&routes[index], &routes[other]);
        }
        for (other, route) in routes.iter().enumerate() {
            if !changed.contains(&other) {
                count += cuts(&routes[index], route);
            }
        }
    }
    count
}

fn cuts(one: &[Point], other: &[Point]) -> usize {
    let mut count = 0;
    for a in one.windows(2) {
        for b in other.windows(2) {
            if crossed(a[0], a[1], b[0], b[1]) {
                count += 1;
            }
        }
    }
    count
}

/// The two segments meet properly, each with its ends on opposite sides of
/// the other. A corner that only touches, and a shared run, do not.
pub(super) fn crossed(a: Point, b: Point, c: Point, d: Point) -> bool {
    let side = |p: Point, q: Point, r: Point| (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    ((d1 > 0.5 && d2 < -0.5) || (d1 < -0.5 && d2 > 0.5))
        && ((d3 > 0.5 && d4 < -0.5) || (d3 < -0.5 && d4 > 0.5))
}

/// Where every line of a trunk turns onto it: a short stem off the box, so
/// the run through the ranks, and a label on it, stays where it was.
fn join_at(rect: Rect, side: Side, direction: Direction) -> f32 {
    match (direction, side) {
        (Direction::Down, Side::Before) => rect.y - STEM,
        (Direction::Down, Side::After) => rect.bottom() + STEM,
        (Direction::Right, Side::Before) => rect.x - STEM,
        (Direction::Right, Side::After) => rect.right() + STEM,
    }
}

/// Where a line may meet `rect` across the flow.
fn clamp(at: f32, rect: Rect, direction: Direction) -> f32 {
    let (start, len) = match direction {
        Direction::Down => (rect.x, rect.w),
        Direction::Right => (rect.y, rect.h),
    };
    let inset = INSET.min(len / 4.0);
    if len <= inset * 2.0 {
        start + len / 2.0
    } else {
        at.clamp(start + inset, start + len - inset)
    }
}

fn across(point: Point, direction: Direction) -> f32 {
    match direction {
        Direction::Down => point.0,
        Direction::Right => point.1,
    }
}

fn along(point: Point, direction: Direction) -> f32 {
    match direction {
        Direction::Down => point.1,
        Direction::Right => point.0,
    }
}

fn put(across_at: f32, along_at: f32, direction: Direction) -> Point {
    match direction {
        Direction::Down => (across_at, along_at),
        Direction::Right => (along_at, across_at),
    }
}

/// The first point of the straight run into the box.
fn stem_at(points: &[Point], direction: Direction) -> Option<usize> {
    let last = points.len().checked_sub(1)?;
    let head = across(points[last], direction);
    let mut stem = last;
    while stem > 0 && (across(points[stem - 1], direction) - head).abs() < 0.05 {
        stem -= 1;
    }
    Some(stem)
}

/// `points` arrive at `(trunk, the box)` by way of `join`. The run down to
/// `join` stays on its own line, so a label there stays with it.
fn retarget(points: &mut Vec<Point>, trunk: f32, join: f32, direction: Direction) {
    if points.len() < 2 {
        return;
    }
    let last = points.len() - 1;
    let head_along = along(points[last], direction);
    let approach = across(points[last], direction);
    if (approach - trunk).abs() < 0.5 {
        return;
    }
    let Some(stem) = stem_at(points, direction) else {
        return;
    };
    // A bend already inside the stem turns there; the one further out
    // keeps its height and the line comes down to the stem on its own.
    let corner = along(points[stem], direction);
    let turn = if (corner - head_along).abs() < (join - head_along).abs() {
        corner
    } else {
        join
    };
    let mut kept = points[..=stem].to_vec();
    kept.push(put(approach, turn, direction));
    kept.push(put(trunk, turn, direction));
    kept.push(put(trunk, head_along, direction));
    *points = super::simplify(kept);
}

#[cfg(test)]
mod tests {
    use super::super::{Direction, Edge, Node, layout};

    fn node(width: f32, height: f32) -> Node {
        Node {
            width,
            height,
            before: 0.0,
            loop_extra: 0.0,
            flat: true,
        }
    }

    fn edge(from: usize, to: usize) -> Edge {
        Edge {
            from,
            to,
            label_width: 0.0,
            label_height: 0.0,
            hidden: false,
            apart: false,
            marked: false,
            arrival: 0,
            label_before: None,
            level: None,
        }
    }

    fn heads(edges: &[Edge]) -> Vec<f32> {
        layout(&[node(40.0, 20.0); 4], edges, Direction::Down)
            .routes
            .iter()
            .map(|route| route.points.last().unwrap().0)
            .collect()
    }

    #[test]
    fn lines_into_one_box_arrive_as_one() {
        let edges = [edge(0, 3), edge(1, 3), edge(2, 3)];
        let at = heads(&edges);
        let (lo, hi) = (
            at.iter().copied().fold(f32::MAX, f32::min),
            at.iter().copied().fold(0.0, f32::max),
        );
        assert!(hi - lo < 0.5, "{at:?}");
        let laid = layout(&[node(40.0, 20.0); 4], &edges, Direction::Down);
        let head = at[0];
        for route in &laid.routes {
            let points = &route.points;
            let (a, b) = (points[points.len() - 2], points[points.len() - 1]);
            assert!(
                (a.0 - b.0).abs() < 0.5 && (b.0 - head).abs() < 0.5,
                "the last run is the shared trunk: {points:?}"
            );
        }
    }

    #[test]
    fn a_labelled_line_keeps_its_own_arrival() {
        let mut edges = [edge(0, 3), edge(1, 3), edge(2, 3)];
        for edge in &mut edges {
            edge.label_width = 36.0;
            edge.label_height = 12.0;
        }
        let at = heads(&edges);
        let span = at.iter().copied().fold(f32::MIN, f32::max)
            - at.iter().copied().fold(f32::MAX, f32::min);
        assert!(span >= super::super::PORT_STEP - 0.1, "{at:?}");
    }

    #[test]
    fn a_mark_at_an_end_keeps_its_own_arrival() {
        let mut edges = [edge(0, 3), edge(1, 3), edge(2, 3)];
        for edge in &mut edges {
            edge.marked = true;
        }
        let at = heads(&edges);
        let span = at.iter().copied().fold(f32::MIN, f32::max)
            - at.iter().copied().fold(f32::MAX, f32::min);
        assert!(span >= super::super::PORT_STEP - 0.1, "{at:?}");
    }

    #[test]
    fn lines_that_arrive_differently_keep_their_own_arrivals() {
        let mut edges = [edge(0, 3), edge(1, 3), edge(2, 3)];
        for (key, edge) in edges.iter_mut().enumerate() {
            edge.arrival = key as u64;
        }
        let at = heads(&edges);
        let span = at.iter().copied().fold(f32::MIN, f32::max)
            - at.iter().copied().fold(f32::MAX, f32::min);
        assert!(span >= super::super::PORT_STEP - 0.1, "{at:?}");
        // Two that arrive alike still share their trunk; the third keeps
        // its own arrival.
        edges[1].arrival = edges[0].arrival;
        let at = heads(&edges);
        assert!((at[0] - at[1]).abs() < 0.5, "{at:?}");
        assert!((at[2] - at[0]).abs() >= 4.0, "{at:?}");
    }

    #[test]
    fn a_line_kept_apart_keeps_its_own_arrival() {
        let edges = [edge(0, 3), edge(1, 3), edge(2, 3)];
        let mut apart = edges;
        for edge in &mut apart {
            edge.apart = true;
        }
        let at = heads(&apart);
        let span = at.iter().copied().fold(f32::MIN, f32::max)
            - at.iter().copied().fold(f32::MAX, f32::min);
        assert!(span >= super::super::PORT_STEP - 0.1, "{at:?}");
    }

    #[test]
    fn two_lines_between_the_same_boxes_stay_apart() {
        let nodes = [node(80.0, 24.0), node(80.0, 24.0)];
        let edges = [edge(0, 1), edge(0, 1)];
        let laid = layout(&nodes, &edges, Direction::Down);
        let ends: Vec<f32> = laid
            .routes
            .iter()
            .map(|route| route.points.last().unwrap().0)
            .collect();
        assert!(
            (ends[0] - ends[1]).abs() >= super::super::PORT_STEP - 0.1,
            "{ends:?}"
        );
    }

    #[test]
    fn lines_that_leave_a_box_stay_spread() {
        let nodes = [node(120.0, 20.0), node(40.0, 20.0), node(40.0, 20.0)];
        let laid = layout(&nodes, &[edge(0, 1), edge(0, 2)], Direction::Down);
        let starts: Vec<f32> = laid.routes.iter().map(|route| route.points[0].0).collect();
        assert!(
            (starts[0] - starts[1]).abs() >= super::super::PORT_STEP - 0.1,
            "{starts:?}"
        );
    }
}
