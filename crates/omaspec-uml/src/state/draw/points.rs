//! Entry and exit points on the border of the state around them.

use super::super::{Container, Kind};
use super::{COMP_PAD, Placed, Scene, stack_width};

/// Moves each entry point of a state over the state's top border and each
/// exit point under its bottom one, once everything is laid out. The lines
/// inside the state to and from a point reach on to where its middle will
/// be, on the border; those that cross the border lose their route, which
/// [`super::nest::link_crossings`] lays anew to the point outside, so they
/// leave and meet it from outside. [`onto_border_lines`] then sets it on
/// the border.
pub(super) fn onto_borders(root: &Container, scene: &mut Scene) {
    let mut moved = Vec::new();
    lift(root, scene, &mut Vec::new(), &mut moved);
    if !moved.is_empty() {
        unroute(root, scene, &mut Vec::new(), &moved);
    }
}

/// Sets each point onto its border, its middle on it, once the lines
/// across the border are laid ([`onto_borders`]), and their ends with it.
pub(super) fn onto_border_lines(root: &Container, scene: &mut Scene) {
    let mut moved = Vec::new();
    settle(root, scene, &mut Vec::new(), &mut moved);
    if !moved.is_empty() {
        follow(root, scene, &mut Vec::new(), &moved);
    }
}

/// Each point of `container` and inside it, laid out as `scene` at `path`,
/// set onto its border; where each is and how far it moved, in `moved`.
fn settle(
    container: &Container,
    scene: &mut Scene,
    path: &mut Vec<usize>,
    moved: &mut Vec<(Vec<usize>, f32)>,
) {
    for (index, (node, placed)) in container
        .nodes
        .iter()
        .zip(scene.nodes.iter_mut())
        .enumerate()
    {
        path.push(index);
        let dy = match node.kind {
            Kind::Entry => placed.rect.h / 2.0 + 1.0,
            Kind::Exit => -(placed.rect.h / 2.0 + 1.0),
            _ => 0.0,
        };
        if dy != 0.0 {
            placed.rect.y += dy;
            moved.push((path.clone(), dy));
        }
        settle(&node.children, &mut placed.children, path, moved);
        path.pop();
    }
}

/// The ends of the transitions between boxes at a point in `moved` moved
/// with it, in `container`, laid out as `scene` at `path`, and inside it.
fn follow(
    container: &Container,
    scene: &mut Scene,
    path: &mut Vec<usize>,
    moved: &[(Vec<usize>, f32)],
) {
    let by = |end: &[usize]| {
        let at = [path.as_slice(), end].concat();
        moved
            .iter()
            .find(|(point, _)| *point == at)
            .map(|(_, dy)| *dy)
    };
    for (index, crossing) in container.crossings.iter().enumerate() {
        let Some(line) = scene.edges.get_mut(container.edges.len() + index) else {
            continue;
        };
        if let Some(dy) = by(&crossing.from) {
            reach(&mut line.points, true, dy);
        }
        if let Some(dy) = by(&crossing.to) {
            reach(&mut line.points, false, dy);
        }
    }
    for (index, (node, placed)) in container
        .nodes
        .iter()
        .zip(scene.nodes.iter_mut())
        .enumerate()
    {
        path.push(index);
        follow(&node.children, &mut placed.children, path, moved);
        path.pop();
    }
}

/// Moves the points of every state in `container`, laid out as `scene` at
/// `path` in the tree, and says where each one is in `moved`.
fn lift(
    container: &Container,
    scene: &mut Scene,
    path: &mut Vec<usize>,
    moved: &mut Vec<Vec<usize>>,
) {
    for (index, (node, placed)) in container
        .nodes
        .iter()
        .zip(scene.nodes.iter_mut())
        .enumerate()
    {
        path.push(index);
        let border = placed.rect;
        let (inside, laid) = (&node.children, &mut placed.children);
        for (at, (point, spot)) in inside.nodes.iter().zip(laid.nodes.iter_mut()).enumerate() {
            let y = match point.kind {
                Kind::Entry => border.y,
                Kind::Exit => border.y + border.h,
                _ => continue,
            };
            let dy = y - (spot.rect.y + spot.rect.h / 2.0);
            // Fully outside the state until the lines across its border
            // are laid, so they leave and meet it from outside
            // ([`onto_border_lines`]).
            let out = match point.kind {
                Kind::Entry => -(spot.rect.h / 2.0 + 1.0),
                _ => spot.rect.h / 2.0 + 1.0,
            };
            spot.rect.y += dy + out;
            for (edge, line) in inside.edges.iter().zip(laid.edges.iter_mut()) {
                if edge.from == at {
                    reach(&mut line.points, true, dy);
                }
                if edge.to == at {
                    reach(&mut line.points, false, dy);
                }
            }
            moved.push([path.as_slice(), &[at]].concat());
        }
        lift(inside, laid, path, moved);
        clear_title(placed);
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

/// The start (or the end) of the line `points` moved `dy` down with the
/// point it meets: along its first run where that runs up or down, else
/// by a run of its own.
fn reach(points: &mut Vec<(f32, f32)>, start: bool, dy: f32) {
    if points.len() < 2 || dy == 0.0 {
        return;
    }
    let (end, next) = match start {
        true => (0, 1),
        false => (points.len() - 1, points.len() - 2),
    };
    let there = (points[end].0, points[end].1 + dy);
    if (points[next].0 - points[end].0).abs() < 0.5 {
        points[end] = there;
    } else if start {
        points.insert(0, there);
    } else {
        points.push(there);
    }
}

/// Takes the route from every transition between boxes that ends at a
/// point in `moved`, at any depth of `container`, laid out as `scene` at
/// `path`.
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
