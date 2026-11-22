//! Choices as PlantUML draws them: a small diamond with no words in it.
//! The layout gives each the room of a larger one ([`super::CHOICE_ROOM`]),
//! which keeps the lines around it as clear as they were; once every line
//! is laid, the ends of those that meet the larger diamond go on, the way
//! they come, to the drawn one ([`super::CHOICE`]).

use super::super::{Container, Kind};
use super::{CHOICE, Scene};

/// Lengthens the ends of the lines of `scene` that meet a choice of `root`
/// onto the diamond drawn.
pub(super) fn reach(root: &Container, scene: &mut Scene) {
    let mut centers = Vec::new();
    gather(root, scene, &mut centers);
    if !centers.is_empty() {
        lengthen(scene, &centers);
    }
}

/// The middles of every choice of `container`, and of those inside its
/// states.
fn gather(container: &Container, scene: &Scene, out: &mut Vec<(f32, f32)>) {
    for (node, placed) in container.nodes.iter().zip(&scene.nodes) {
        if node.kind == Kind::Choice {
            let r = placed.rect;
            out.push((r.x + r.w / 2.0, r.y + r.h / 2.0));
        }
        gather(&node.children, &placed.children, out);
    }
}

fn lengthen(scene: &mut Scene, centers: &[(f32, f32)]) {
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
fn onto(from: (f32, f32), end: (f32, f32), centers: &[(f32, f32)]) -> Option<(f32, f32)> {
    let half = CHOICE / 2.0;
    let room = super::CHOICE_ROOM / 2.0;
    // How far out of a diamond of half `half` about `c` a point lies: 1 on
    // its edge.
    let out = |p: (f32, f32), c: (f32, f32)| ((p.0 - c.0).abs() + (p.1 - c.1).abs()) / half;
    let &c = centers
        .iter()
        .find(|&&c| (out(end, c) - room / half).abs() < 0.2)?;
    let (dx, dy) = (end.0 - from.0, end.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length < 0.01 {
        return None;
    }
    let (ux, uy) = (dx / length, dy / length);
    // Toward the middle, in small steps, until on the drawn diamond.
    let mut t = 0.0;
    while t <= 2.0 * room {
        let p = (end.0 + ux * t, end.1 + uy * t);
        if out(p, c) <= 1.0 {
            return Some(p);
        }
        t += 0.25;
    }
    None
}
