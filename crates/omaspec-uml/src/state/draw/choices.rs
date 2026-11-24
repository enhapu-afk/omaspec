//! Choices as PlantUML draws them: a small diamond with no words in it.
//! The layout gives each the room of a larger one ([`super::CHOICE_ROOM`];
//! wider and higher for an activity's test beside it,
//! [`super::actions::test_room`]), which keeps the lines around it as
//! clear as they were; once every line is laid, the ends of those that
//! meet the larger diamond go on, the way they come, to the drawn one
//! ([`super::CHOICE`]).

use super::super::{Container, Kind};
use super::{CHOICE, Scene};

/// A choice's middle, and half the width and the height of its room.
type Room = ((f32, f32), (f32, f32));

/// Lengthens the ends of the lines of `scene` that meet a choice of `root`
/// onto the diamond drawn.
pub(super) fn reach(root: &Container, scene: &mut Scene) {
    let mut centers = Vec::new();
    gather(root, scene, &mut centers);
    if !centers.is_empty() {
        lengthen(scene, &centers);
    }
}

/// The names of every choice of `container` and of those inside its
/// states, which their diamonds do not write.
pub(super) fn names(container: &Container) -> Vec<String> {
    let mut out = Vec::new();
    for node in &container.nodes {
        if node.kind == Kind::Choice {
            out.push(node.key.clone());
        }
        out.extend(names(&node.children));
    }
    out
}

/// The middles of every choice of `container`, and of those inside its
/// states.
fn gather(container: &Container, scene: &Scene, out: &mut Vec<Room>) {
    for (node, placed) in container.nodes.iter().zip(&scene.nodes) {
        if node.kind == Kind::Choice {
            let r = placed.rect;
            out.push(((r.x + r.w / 2.0, r.y + r.h / 2.0), (r.w / 2.0, r.h / 2.0)));
        }
        gather(&node.children, &placed.children, out);
    }
}

fn lengthen(scene: &mut Scene, centers: &[Room]) {
    // A note's dashed line to a choice ends at its box: on to the diamond.
    for note in &mut scene.notes {
        if let Some((from, to)) = &mut note.tether
            && let Some(&room) = centers.iter().find(|(c, (hw, hh))| {
                (to.0 - c.0).abs() <= hw + 0.5 && (to.1 - c.1).abs() <= hh + 0.5
            })
            && let Some(end) = toward(*from, *to, room)
        {
            *to = end;
        }
    }
    for edge in &mut scene.edges {
        let n = edge.points.len();
        if n < 2 {
            continue;
        }
        if let Some(end) = onto(edge.points[n - 2], edge.points[n - 1], centers) {
            edge.points[n - 1] = end;
        }
        if let Some(start) = onto(edge.points[1], edge.points[0], centers) {
            edge.points[0] = start;
        }
    }
    for node in &mut scene.nodes {
        lengthen(&mut node.children, centers);
    }
}

/// Where the line from `from` to `end` meets the drawn diamond of the
/// choice whose larger one `end` is on, going on the way it comes; `None`
/// when `end` is at no choice, or the line would miss it.
fn onto(from: (f32, f32), end: (f32, f32), centers: &[Room]) -> Option<(f32, f32)> {
    // How far out of the larger diamond a point lies: 1 on its edge; the
    // play as wide as a fifth of the drawn one's half, at the narrower.
    let near = |&&(c, (hw, hh)): &&Room| {
        let out = (end.0 - c.0).abs() / hw + (end.1 - c.1).abs() / hh;
        (out - 1.0).abs() < 0.2 * (CHOICE / 2.0) / hw.min(hh)
    };
    let &room = centers.iter().find(near)?;
    toward(from, end, room)
}

/// Where the line from `from` to `end` meets the drawn diamond in `room`,
/// going on the way it comes, or else toward its middle.
fn toward(from: (f32, f32), end: (f32, f32), (c, (hw, hh)): Room) -> Option<(f32, f32)> {
    let half = CHOICE / 2.0;
    let room = hw.max(hh);
    let out = |p: (f32, f32)| ((p.0 - c.0).abs() + (p.1 - c.1).abs()) / half;
    let (dx, dy) = (end.0 - from.0, end.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length < 0.01 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    // On, in small steps, until on the drawn diamond; where the line would
    // pass beside it (it met the larger one off a corner), toward its
    // middle instead.
    let step = |(ux, uy): (f32, f32)| {
        let mut t = 0.0;
        while t <= 2.0 * room {
            let p = (end.0 + ux * t, end.1 + uy * t);
            if out(p) <= 1.0 {
                return Some(p);
            }
            t += 0.25;
        }
        None
    };
    let (mx, my) = (c.0 - end.0, c.1 - end.1);
    let middle = (mx * mx + my * my).sqrt().max(0.01);
    step((ux, uy)).or_else(|| step((mx / middle, my / middle)))
}
