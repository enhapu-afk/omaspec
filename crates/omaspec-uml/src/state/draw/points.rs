//! Entry and exit points on the border of the state around them.
//!
//! Laid out among the states, the points are set on the border once all
//! is placed ([`onto_borders`]): on the top and bottom border where the
//! state runs down, on the left and right one where it runs left to
//! right, as PlantUML puts them. The lines inside the state reach on to
//! them there. A line between boxes at a point is laid anew by
//! [`super::nest::link_crossings`] to where the point stands for it
//! ([`stance`]): just outside its state, which is closed to the line, when
//! the line leads out of the state; just inside (under its title) when it
//! stays inside. [`meet`] then leads the line's end on to the point.

use super::super::{Container, Direction, Kind};
use super::{COMP_PAD, Placed, Rect, Scene, Side, stack_width};

type Point = (f32, f32);

/// Sets each point on its border, the lines inside its state and its
/// notes with it, and takes the route from every transition between
/// boxes that ends at one.
pub(super) fn onto_borders(root: &Container, scene: &mut Scene) {
    let mut moved = Vec::new();
    lift(root, scene, &mut Vec::new(), &mut moved);
    if !moved.is_empty() {
        unroute(root, scene, &mut Vec::new(), &moved);
    }
}

/// Where the end at `end` of a line to `other` stands for it, when it is
/// a point of `root`, laid out as `scene`, and whether the point's state
/// is closed to the line: a step outside the state for a line that leads
/// out of it, a step inside (under the title of a state whose states run
/// down, for an entry) for one that stays inside. `None` for an end that
/// is no point.
pub(super) fn stance(
    root: &Container,
    scene: &Scene,
    end: &[usize],
    other: &[usize],
) -> Option<(Rect, bool)> {
    let (last, holder) = end.split_last()?;
    let mut container = root;
    let mut laid = scene;
    let mut state = None;
    for &index in holder {
        state = Some(laid.nodes.get(index)?);
        container = &container.nodes.get(index)?.children;
        laid = &laid.nodes.get(index)?.children;
    }
    let node = container.nodes.get(*last)?;
    if !matches!(node.kind, Kind::Entry | Kind::Exit) {
        return None;
    }
    let rect = laid.nodes.get(*last)?.rect;
    let band = state.map_or(0.0, |state| state.item.band());
    let out = !other.starts_with(holder);
    let (dx, dy) = step(node.kind, container.direction, (rect.w, rect.h), band, out);
    let stand = Rect {
        x: rect.x + dx,
        y: rect.y + dy,
        ..rect
    };
    Some((stand, out))
}

/// How far a point of `kind`, `size` large, in a state whose states run
/// `direction` and whose title band is `band` high, stands off its place
/// on the border for a line that meets it from outside (`out`), or from
/// inside: under the title band for an entry where the states run down.
/// Far enough off that a line met beside it can turn onto it with room.
fn step(kind: Kind, direction: Direction, (w, h): Point, band: f32, out: bool) -> Point {
    let sign = if kind == Kind::Entry { -1.0 } else { 1.0 };
    let outward = match direction {
        Direction::Down => (0.0, sign * (h / 2.0 + OFF)),
        Direction::Right => (sign * (w / 2.0 + OFF), 0.0),
    };
    if out {
        outward
    } else if direction == Direction::Down && kind == Kind::Entry {
        (0.0, band + h / 2.0 + OFF)
    } else {
        (-outward.0, -outward.1)
    }
}

/// How far past a point's half size it stands off its place for a line.
const OFF: f32 = 14.0;

/// The start (or the end) of the line `points`, laid to where its point
/// stood for it (`stand`), led on to the point at `rect`: along the line's
/// end run where that runs toward the point; else, its end run turned
/// onto the point, so it meets it square and not along the border.
pub(super) fn meet(points: &mut Vec<Point>, start: bool, stand: Rect, rect: Rect) {
    let n = points.len();
    if n < 2 {
        return;
    }
    let (end, next) = match start {
        true => (0, 1),
        false => (n - 1, n - 2),
    };
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let (sx, sy) = (stand.x + stand.w / 2.0, stand.y + stand.h / 2.0);
    // Off the point up or down (`true`), or to a side.
    let upright = (sy - cy).abs() >= (sx - cx).abs();
    // Where the line meets the point: the middle of its side toward
    // where it stood.
    let side = match upright {
        true => (cx, if sy < cy { rect.y } else { rect.y + rect.h }),
        false => (if sx < cx { rect.x } else { rect.x + rect.w }, cy),
    };
    let (p, q) = (points[end], points[next]);
    let toward = match upright {
        true => (p.0 - q.0).abs() < 0.5,
        false => (p.1 - q.1).abs() < 0.5,
    };
    if toward {
        points[end] = match upright {
            true => (p.0, side.1),
            false => (side.0, p.1),
        };
        return;
    }
    let turn = match upright {
        true => (side.0, p.1),
        false => (p.0, side.1),
    };
    points[end] = turn;
    match start {
        true => points.insert(0, side),
        false => points.push(side),
    }
}

/// The line `points` without a short step between two runs that lie side
/// by side (a jog, which the line takes to meet a point where it stood
/// for it): one of the two runs moves onto the other's line, where the
/// run beyond it lets it, so no run turns slant and the ends stay.
pub(super) fn straighten(points: &mut Vec<Point>) {
    const JOG: f32 = 8.0;
    let flat = |a: Point, b: Point| (a.1 - b.1).abs() < 0.5;
    let upright = |a: Point, b: Point| (a.0 - b.0).abs() < 0.5;
    let parallel = |a: Point, b: Point, c: Point, d: Point| {
        (flat(a, b) && flat(c, d)) || (upright(a, b) && upright(c, d))
    };
    let mut i = 1;
    while i + 2 < points.len() {
        let (a, b, c, d) = (points[i - 1], points[i], points[i + 1], points[i + 2]);
        let step = (c.0 - b.0, c.1 - b.1);
        let short = step.0.abs().max(step.1.abs());
        if short < 0.5 || short >= JOG || !parallel(a, b, c, d) {
            i += 1;
            continue;
        }
        let along = |p: Point, q: Point| match flat(b, c) {
            true => flat(p, q),
            false => upright(p, q),
        };
        // The run after the step moves back onto the one before it, where
        // the run after it lies along the step; else the run before moves
        // on, where the run before it does. Never an end.
        if i + 3 < points.len() && along(d, points[i + 3]) {
            points[i + 1] = (c.0 - step.0, c.1 - step.1);
            points[i + 2] = (d.0 - step.0, d.1 - step.1);
        } else if i >= 2 && along(points[i - 2], a) {
            points[i - 1] = (a.0 + step.0, a.1 + step.1);
            points[i] = (b.0 + step.0, b.1 + step.1);
        } else {
            i += 1;
            continue;
        }
        // The step is gone: one point of the two it joined is left over,
        // and one between two runs in a line.
        points.dedup_by(|p, q| (p.0 - q.0).abs() < 0.5 && (p.1 - q.1).abs() < 0.5);
        let mut k = 1;
        while k + 1 < points.len() {
            let (p, q, r) = (points[k - 1], points[k], points[k + 1]);
            if (flat(p, q) && flat(q, r)) || (upright(p, q) && upright(q, r)) {
                points.remove(k);
            } else {
                k += 1;
            }
        }
        i = 1;
    }
}

/// The straight line between two points, each at its place and where it
/// stood for the line, that face each other on one line across the gap
/// between their states: from the side of the one to the side of the
/// other; `None` for any others.
pub(super) fn facing((ra, sa): (Rect, Rect), (rb, sb): (Rect, Rect)) -> Option<Vec<Point>> {
    let middle = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let ((ax, ay), (bx, by)) = (middle(ra), middle(rb));
    let (out_a, out_b) = (
        (middle(sa).0 - ax, middle(sa).1 - ay),
        (middle(sb).0 - bx, middle(sb).1 - by),
    );
    // Side by side, each stepping out toward the other.
    if (ay - by).abs() < 0.5
        && out_a.1 == 0.0
        && out_b.1 == 0.0
        && (bx - ax) * out_a.0 > 0.0
        && (ax - bx) * out_b.0 > 0.0
    {
        let (a, b) = if ax < bx {
            (ra.x + ra.w, rb.x)
        } else {
            (ra.x, rb.x + rb.w)
        };
        return Some(vec![(a, ay), (b, by)]);
    }
    if (ax - bx).abs() < 0.5
        && out_a.0 == 0.0
        && out_b.0 == 0.0
        && (by - ay) * out_a.1 > 0.0
        && (ay - by) * out_b.1 > 0.0
    {
        let (a, b) = if ay < by {
            (ra.y + ra.h, rb.y)
        } else {
            (ra.y, rb.y + rb.h)
        };
        return Some(vec![(ax, a), (bx, b)]);
    }
    None
}

/// Where the point at `path` of `root` stands in `scene`; `None` where no
/// point is.
pub(super) fn rect_at(root: &Container, scene: &Scene, path: &[usize]) -> Option<Rect> {
    let (last, holder) = path.split_last()?;
    let mut container = root;
    let mut laid = scene;
    for &index in holder {
        container = &container.nodes.get(index)?.children;
        laid = &laid.nodes.get(index)?.children;
    }
    let node = container.nodes.get(*last)?;
    if !matches!(node.kind, Kind::Entry | Kind::Exit) {
        return None;
    }
    laid.nodes.get(*last).map(|placed| placed.rect)
}

/// Moves the points of every state in `container`, laid out as `scene` at
/// `path` in the tree, onto its border; where each is, in `moved`.
fn lift(
    container: &Container,
    scene: &mut Scene,
    path: &mut Vec<usize>,
    moved: &mut Vec<Vec<usize>>,
) {
    // Room a state's own notes over and under it give its points' names.
    let mut room: Vec<(usize, f32, f32)> = Vec::new();
    for (index, (node, placed)) in container
        .nodes
        .iter()
        .zip(scene.nodes.iter_mut())
        .enumerate()
    {
        path.push(index);
        let border = placed.rect;
        let (inside, laid) = (&node.children, &mut placed.children);
        let (mut over, mut under) = (0.0_f32, 0.0_f32);
        for (at, (point, spot)) in inside.nodes.iter().zip(laid.nodes.iter_mut()).enumerate() {
            if !matches!(point.kind, Kind::Entry | Kind::Exit) {
                continue;
            }
            let (cx, cy) = (
                spot.rect.x + spot.rect.w / 2.0,
                spot.rect.y + spot.rect.h / 2.0,
            );
            let by = match (inside.direction, point.kind) {
                (Direction::Down, Kind::Entry) => (0.0, border.y - cy),
                (Direction::Down, _) => (0.0, border.y + border.h - cy),
                (Direction::Right, Kind::Entry) => (border.x - cx, 0.0),
                (Direction::Right, _) => (border.x + border.w - cx, 0.0),
            };
            spot.rect.x += by.0;
            spot.rect.y += by.1;
            let name = spot.item.title.height + 4.0;
            match point.kind {
                Kind::Entry => over = over.max(name),
                _ => under = under.max(name),
            }
            for (edge, line) in inside.edges.iter().zip(laid.edges.iter_mut()) {
                if edge.from == at && edge.to == at {
                    // A loop on the point moves with it as a whole.
                    for p in &mut line.points {
                        *p = (p.0 + by.0, p.1 + by.1);
                    }
                    line.label_at = (line.label_at.0 + by.0, line.label_at.1 + by.1);
                    continue;
                }
                if edge.from == at {
                    reach(&mut line.points, true, by);
                }
                if edge.to == at {
                    reach(&mut line.points, false, by);
                }
            }
            // Its notes keep the room the layout gave them, tied to it by a
            // dashed line, as a note that stands off its state is.
            let rect = spot.rect;
            for note in laid
                .notes
                .iter_mut()
                .filter(|n| n.target == at && n.on.is_none())
            {
                note.tether = Some(tether(note, rect));
            }
            moved.push([path.as_slice(), &[at]].concat());
        }
        lift(inside, laid, path, moved);
        if inside.direction == Direction::Down {
            clear_title(placed);
            room.push((index, over, under));
        }
        path.pop();
    }
    for (index, over, under) in room {
        for note in scene
            .notes
            .iter_mut()
            .filter(|n| n.target == index && n.on.is_none())
        {
            let dy = match note.side {
                Side::Top => -over,
                Side::Bottom => under,
                Side::Left | Side::Right => 0.0,
            };
            note.y += dy;
            // The note's end of its dashed line goes with it.
            if let Some((from, _)) = &mut note.tether {
                from.1 += dy;
            }
        }
    }
}

/// Moves the title of a state with entry points along its band to the
/// free stretch nearest its middle, so the lines from the points keep off
/// its words; where none is wide enough, it stays in the middle.
fn clear_title(placed: &mut Placed) {
    let rect = placed.rect;
    let blocked: Vec<(f32, f32)> = placed
        .children
        .nodes
        .iter()
        .filter(|node| node.item.kind == Kind::Entry)
        .map(|node| (node.rect.x - GAP, node.rect.x + node.rect.w + GAP))
        .collect();
    if blocked.is_empty() {
        return;
    }
    let half = stack_width(&placed.item.title, &placed.item.description) / 2.0;
    let middle = rect.x + rect.w / 2.0;
    let (low, high) = (rect.x + COMP_PAD + half, rect.x + rect.w - COMP_PAD - half);
    let free = |c: f32| blocked.iter().all(|&(a, b)| c + half <= a || c - half >= b);
    if free(middle) || low > high {
        return;
    }
    let best = blocked
        .iter()
        .flat_map(|&(a, b)| [b + half, a - half])
        .filter(|&c| (low..=high).contains(&c) && free(c))
        .min_by(|a, b| (a - middle).abs().total_cmp(&(b - middle).abs()));
    if let Some(c) = best {
        placed.item.title_shift = c - middle;
    }
}

/// Between a title and the line from an entry point beside it.
const GAP: f32 = 4.0;

/// The dashed line from `note` to the point `rect` it is about: from the
/// middle of the note's side toward the point to the point's nearest side.
fn tether(note: &super::PlacedNote, rect: Rect) -> (Point, Point) {
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let (nx, ny) = (note.x + note.w / 2.0, note.y + note.h / 2.0);
    if (nx - cx).abs() * rect.h >= (ny - cy).abs() * rect.w {
        let (from_x, to_x) = match nx > cx {
            true => (note.x, rect.x + rect.w),
            false => (note.x + note.w, rect.x),
        };
        ((from_x, ny), (to_x, cy))
    } else {
        let (from_y, to_y) = match ny > cy {
            true => (note.y, rect.y + rect.h),
            false => (note.y + note.h, rect.y),
        };
        ((nx, from_y), (cx, to_y))
    }
}

/// The start (or the end) of the line `points` moved `by` with the point
/// it meets: along its end run where that runs the way the point moved;
/// across, the run slides with the point where the run before it lies
/// along the move; else by a run of its own.
fn reach(points: &mut Vec<Point>, start: bool, (dx, dy): Point) {
    let n = points.len();
    if n < 2 || (dx == 0.0 && dy == 0.0) {
        return;
    }
    let (end, next, before) = match start {
        true => (0, 1, 2),
        false => (n - 1, n - 2, n.wrapping_sub(3)),
    };
    // Whether `a` to `b` runs the way the point moved.
    let along = |a: Point, b: Point| match dy != 0.0 {
        true => (a.0 - b.0).abs() < 0.5,
        false => (a.1 - b.1).abs() < 0.5,
    };
    let moved = |p: Point| (p.0 + dx, p.1 + dy);
    let (p, q) = (points[end], points[next]);
    if along(p, q) {
        points[end] = moved(p);
    } else if let Some(&r) = points.get(before)
        && along(r, q)
    {
        points[end] = moved(p);
        points[next] = moved(q);
    } else if start {
        points.insert(0, moved(p));
    } else {
        points.push(moved(p));
    }
}

/// Takes the route from every transition between boxes that ends at a
/// point in `moved`, at any depth of `container`, laid out as `scene` at
/// `path`: [`super::nest::link_crossings`] lays them anew.
fn unroute(container: &Container, scene: &mut Scene, path: &mut Vec<usize>, moved: &[Vec<usize>]) {
    let ends = |end: &[usize]| {
        moved
            .iter()
            .any(|point| point.as_slice() == [path.as_slice(), end].concat())
    };
    let gone: Vec<usize> = container
        .crossings
        .iter()
        .enumerate()
        .filter(|(_, crossing)| ends(&crossing.from) || ends(&crossing.to))
        .map(|(index, _)| container.edges.len() + index)
        .collect();
    for slot in gone {
        if let Some(line) = scene.edges.get_mut(slot) {
            line.points.clear();
        }
    }
    for (index, (node, placed)) in container
        .nodes
        .iter()
        .zip(scene.nodes.iter_mut())
        .enumerate()
    {
        path.push(index);
        unroute(&node.children, &mut placed.children, path, moved);
        path.pop();
    }
}
