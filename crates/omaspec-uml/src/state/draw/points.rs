//! Entry and exit points on the border of the state around them.
//!
//! Laid out among the states, the points are set on the border once all
//! is placed ([`onto_borders`]): on the top and bottom border where the
//! state runs down, on the left and right one where it runs left to
//! right, as PlantUML puts them. The lines inside the state reach on to
//! them there. The lines between boxes at a point are laid anew in two
//! passes of [`super::nest::link_crossings`]: those that stay inside the
//! point's state with the point stepped just inside, then those that cross
//! the border with the point stepped just outside ([`step_out`]), so each
//! leaves and meets it from its side; [`step_in`] then sets the points
//! back on the border, and those lines' ends with them.

use super::super::{Container, Crossing, Direction, Kind};
use super::{COMP_PAD, Placed, Scene, Side, stack_width};

type Point = (f32, f32);

/// Sets each point on its border, the lines inside its state and its
/// notes with it, takes the route from every transition between boxes that
/// ends at one, and steps it just inside its state for the first pass.
pub(super) fn onto_borders(root: &Container, scene: &mut Scene) {
    let mut moved = Vec::new();
    lift(root, scene, &mut Vec::new(), &mut moved);
    if !moved.is_empty() {
        unroute(root, scene, &mut Vec::new(), &moved);
        stand(root, scene, -1.0);
    }
}

/// Whether `crossing`, held by the container at `path`, ends at a point and
/// leads out of the state on whose border it stands: such a line is laid
/// with the point outside.
pub(super) fn across(root: &Container, path: &[usize], crossing: &Crossing) -> bool {
    let from = [path, &crossing.from].concat();
    let to = [path, &crossing.to].concat();
    let out = |point: &[usize], other: &[usize]| {
        point_at(root, point).is_some() && !other.starts_with(&point[..point.len() - 1])
    };
    out(&from, &to) || out(&to, &from)
}

/// Steps each point just outside its state, so the lines across its border
/// are laid to it from outside.
pub(super) fn step_out(root: &Container, scene: &mut Scene) {
    stand(root, scene, 1.0);
}

/// Sets each point back on its border, and the ends of the lines between
/// boxes laid to it with it.
pub(super) fn step_in(root: &Container, scene: &mut Scene) {
    let mut steps = Vec::new();
    steps_of(root, scene, &mut Vec::new(), &mut steps);
    if steps.is_empty() {
        return;
    }
    stand(root, scene, 0.0);
    follow(root, root, scene, &mut Vec::new(), &steps);
}

/// Each point of `container` and inside it, laid out as `scene` at `path`,
/// with where it stood for each pass: its step inside and its step outside.
fn steps_of(
    container: &Container,
    scene: &mut Scene,
    path: &mut Vec<usize>,
    steps: &mut Vec<(Vec<usize>, Point, Point)>,
) {
    for (index, (node, placed)) in container
        .nodes
        .iter()
        .zip(scene.nodes.iter_mut())
        .enumerate()
    {
        path.push(index);
        let inside = &node.children;
        let band = placed.item.band();
        for (at, (point, spot)) in inside.nodes.iter().zip(&placed.children.nodes).enumerate() {
            if !matches!(point.kind, Kind::Entry | Kind::Exit) {
                continue;
            }
            let size = (spot.rect.w, spot.rect.h);
            let within = offset(point.kind, inside.direction, size, band, -1.0);
            let out = offset(point.kind, inside.direction, size, band, 1.0);
            steps.push(([path.as_slice(), &[at]].concat(), within, out));
        }
        steps_of(inside, &mut placed.children, path, steps);
        path.pop();
    }
}

/// Whether a point stands at `path` in `root`.
fn point_at(root: &Container, path: &[usize]) -> Option<()> {
    let (last, holder) = path.split_last()?;
    let mut container = root;
    for &index in holder {
        container = &container.nodes.get(index)?.children;
    }
    let node = container.nodes.get(*last)?;
    matches!(node.kind, Kind::Entry | Kind::Exit).then_some(())
}

/// How far a point of `kind`, `size` large, in a state whose states run
/// `direction` and whose title band is `band` high, stands off its place
/// on the border: a step outside (`toward` 1), a step inside (`-1`), below
/// the title band for an entry where the states run down, or none (`0`).
fn offset(kind: Kind, direction: Direction, (w, h): Point, band: f32, toward: f32) -> Point {
    let sign = if kind == Kind::Entry { -1.0 } else { 1.0 };
    let out = match direction {
        Direction::Down => (0.0, sign * (h / 2.0 + 1.0)),
        Direction::Right => (sign * (w / 2.0 + 1.0), 0.0),
    };
    if toward > 0.0 {
        out
    } else if toward == 0.0 {
        (0.0, 0.0)
    } else if direction == Direction::Down && kind == Kind::Entry {
        (0.0, band + h / 2.0 + 1.0)
    } else {
        (-out.0, -out.1)
    }
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
            match note.side {
                Side::Top => note.y -= over,
                Side::Bottom => note.y += under,
                Side::Left | Side::Right => {}
            }
        }
    }
}

/// Stands every point of `container` and inside it `toward` its outward
/// step ([`outward`]) off its place on the border: one step outside, one
/// inside (`-1`), or on it (`0`). A point keeps how far it stands off, so
/// its name is kept clear where it ends ([`super::point_name`]).
fn stand(container: &Container, scene: &mut Scene, toward: f32) {
    for (node, placed) in container.nodes.iter().zip(scene.nodes.iter_mut()) {
        let inside = &node.children;
        let band = placed.item.band();
        for (point, spot) in inside.nodes.iter().zip(placed.children.nodes.iter_mut()) {
            if !matches!(point.kind, Kind::Entry | Kind::Exit) {
                continue;
            }
            let size = (spot.rect.w, spot.rect.h);
            let now = offset(point.kind, inside.direction, size, band, toward);
            let (was_x, was_y) = spot.item.aside;
            spot.rect.x += now.0 - was_x;
            spot.rect.y += now.1 - was_y;
            spot.item.aside = now;
        }
        stand(inside, &mut placed.children, toward);
    }
}

/// The ends of the transitions between boxes at a point, in `container`
/// of the diagram `root`, laid out as `scene` at `path`, and inside it,
/// moved with the point from where they were laid to it (`steps`, by
/// [`stand`]): from outside for a line across the point's border, else
/// from inside.
fn follow(
    root: &Container,
    container: &Container,
    scene: &mut Scene,
    path: &mut Vec<usize>,
    steps: &[(Vec<usize>, Point, Point)],
) {
    let by = |end: &[usize], crossing: &Crossing| {
        let at = [path.as_slice(), end].concat();
        let (_, within, out) = steps.iter().find(|(point, _, _)| *point == at)?;
        let (dx, dy) = if across(root, path, crossing) {
            *out
        } else {
            *within
        };
        Some((-dx, -dy))
    };
    for (index, crossing) in container.crossings.iter().enumerate() {
        let (from, to) = (by(&crossing.from, crossing), by(&crossing.to, crossing));
        let Some(line) = scene.edges.get_mut(container.edges.len() + index) else {
            continue;
        };
        if let Some(by) = from {
            reach(&mut line.points, true, by);
        }
        if let Some(by) = to {
            reach(&mut line.points, false, by);
        }
    }
    for (index, (node, placed)) in container
        .nodes
        .iter()
        .zip(scene.nodes.iter_mut())
        .enumerate()
    {
        path.push(index);
        follow(root, &node.children, &mut placed.children, path, steps);
        path.pop();
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
fn tether(note: &super::PlacedNote, rect: super::Rect) -> (Point, Point) {
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
/// `path`: both passes of [`super::nest::link_crossings`] lay them anew.
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
