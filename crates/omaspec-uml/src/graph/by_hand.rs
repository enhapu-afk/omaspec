//! Lines by hand ([0016](../../../../docs/adr/0016-lines-by-hand.md)) for
//! every drawer of graph diagrams: once everything is placed, each line
//! the writer gave bends is laid again through them, around everything
//! else ([`across::link`]), and each line becomes a [`Wire`] of the map.
//!
//! A drawer names its lines ([`Arrow`], [`keyed`]) and lets them be read
//! and changed ([`Laid`]); the rest is the same for all of them.

use super::across::{self, Drawn, Link, Placement, center};
use super::route::Point;
use crate::hand::{self, Key};
use crate::map::Wire;

/// A line as a drawer laid it: the container that holds it (the path of
/// child indices from the top), its slot among that container's lines,
/// the paths from the top down to its two ends, where it comes in the
/// source, and its line there if the drawer knows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrow {
    pub path: Vec<usize>,
    pub slot: usize,
    pub from: Vec<usize>,
    pub to: Vec<usize>,
    pub order: usize,
    pub line: Option<usize>,
}

/// One laid line, to change.
pub struct LineMut<'a> {
    pub points: &'a mut Vec<Point>,
    /// The top left of its label.
    pub label_at: &'a mut Point,
    /// The label's width and height; zero for none.
    pub label: (f32, f32),
}

/// The lines a drawer laid.
pub trait Laid {
    /// The points of the line at `slot` of the container at `path`.
    fn points(&self, path: &[usize], slot: usize) -> Option<&[Point]>;
    /// The same line, to lay again.
    fn line_mut(&mut self, path: &[usize], slot: usize) -> Option<LineMut<'_>>;
    /// Everything drawn: lines, labels, notes. A line with fewer than two
    /// points is not drawn.
    fn drawn(&self) -> Drawn;
    /// Finishes a line just laid again through its bends the way the
    /// drawer finishes its lines: cut to a round box, say.
    fn relaid(&mut self, _path: &[usize], _slot: usize) {}
}

/// `arrows` with what the block of lines by hand calls each: the names
/// `name` gives the two ends, and a count among lines between the same two.
pub fn keyed(arrows: Vec<Arrow>, name: impl Fn(&[usize]) -> String) -> Vec<(Arrow, Key)> {
    let ends: Vec<(String, String, usize)> = arrows
        .iter()
        .map(|a| (name(&a.from), name(&a.to), a.order))
        .collect();
    arrows.into_iter().zip(hand::keys(&ends)).collect()
}

/// How far past the drawing a bend may lie; one further out is surely a
/// slip, and would shrink the diagram to a dot.
const REACH: f32 = 1000.0;

/// Lays each line of `hands` again through its bends; `top` is the placed
/// boxes. Returns the entries that did not hold, by line: with why.
pub fn lay(
    top: &[Placement],
    arrows: &[(Arrow, Key)],
    laid: &mut impl Laid,
    hands: &[(usize, hand::Line)],
) -> Vec<(usize, String)> {
    let mut misses = Vec::new();
    let mut links = Vec::new();
    let mut which = Vec::new();
    let Some(extent) = across::extent(top) else {
        return misses;
    };
    let near = |(x, y): Point| {
        x >= extent.x - REACH
            && x <= extent.x + extent.w + REACH
            && y >= extent.y - REACH
            && y <= extent.y + extent.h + REACH
    };
    for (entry, line) in hands {
        let Some(index) = arrows.iter().position(|(_, key)| *key == line.key) else {
            misses.push((*entry, hand::no_line(&line.key)));
            continue;
        };
        let arrow = &arrows[index].0;
        let ends = across::find(top, &arrow.from).zip(across::find(top, &arrow.to));
        let Some((from, to)) = ends else {
            continue;
        };
        let Some(laid) = laid.line_mut(&arrow.path, arrow.slot) else {
            continue;
        };
        if laid.points.len() < 2 {
            continue;
        }
        let (a, b) = (center(from.rect), center(to.rect));
        let (vias, far): (Vec<Point>, Vec<Point>) =
            line.vias.iter().map(|v| v.at(a, b)).partition(|&p| near(p));
        if !far.is_empty() {
            misses.push((*entry, hand::far_out(&line.key, far.len())));
        }
        links.push(Link {
            from: arrow.from.clone(),
            to: arrow.to.clone(),
            label: laid.label,
            vias: vias.clone(),
        });
        // Out of what the others keep clear of; back if it finds no way.
        which.push((index, *entry, std::mem::take(laid.points), vias));
    }
    let drawn = laid.drawn();
    let routed = across::link(top, &links, &drawn);
    for ((index, entry, before, vias), linked) in which.into_iter().zip(routed) {
        let (arrow, key) = &arrows[index];
        let Some(line) = laid.line_mut(&arrow.path, arrow.slot) else {
            continue;
        };
        match linked {
            Some(linked) => {
                *line.points = tight(linked.points, &vias);
                *line.label_at = linked.label_at;
                laid.relaid(&arrow.path, arrow.slot);
                if linked.left_out > 0 {
                    misses.push((entry, hand::left_out(key, linked.left_out)));
                }
            }
            None => {
                *line.points = before;
                misses.push((entry, hand::no_way(key)));
            }
        }
    }
    misses.sort_by_key(|&(line, _)| line);
    misses
}

/// How far a line by hand runs straight on each side of a bend it turns
/// at, at most. A curve rounds a corner by at most half the runs beside it
/// ([`crate::curve`]), so the line turns close by the bend, not far past it.
const TIGHT: f32 = 16.0;

/// `points` with a short straight run on each side of every corner that
/// lies on one of `vias`.
fn tight(points: Vec<Point>, vias: &[Point]) -> Vec<Point> {
    let on_via = |p: Point| {
        vias.iter()
            .any(|v| (v.0 - p.0).abs() < 0.01 && (v.1 - p.1).abs() < 0.01)
    };
    let toward = |p: Point, q: Point, d: f32| {
        let length = (q.0 - p.0).hypot(q.1 - p.1);
        (
            p.0 + (q.0 - p.0) * d / length,
            p.1 + (q.1 - p.1) * d / length,
        )
    };
    let mut out = Vec::with_capacity(points.len() + 2 * vias.len());
    for (i, &p) in points.iter().enumerate() {
        let inner = i > 0 && i + 1 < points.len();
        if !inner || !on_via(p) {
            out.push(p);
            continue;
        }
        let (a, b) = (points[i - 1], points[i + 1]);
        let room = TIGHT
            .min((p.0 - a.0).hypot(p.1 - a.1) / 2.0)
            .min((b.0 - p.0).hypot(b.1 - p.1) / 2.0);
        if room < 1.0 {
            out.push(p);
            continue;
        }
        out.extend([toward(p, a, room), p, toward(p, b, room)]);
    }
    out
}

/// Every line drawn, as a [`Wire`] for the map; `drawn` turns a line's
/// points into the way it is drawn (its curves).
pub fn wires(
    top: &[Placement],
    arrows: &[(Arrow, Key)],
    laid: &impl Laid,
    hands: &[(usize, hand::Line)],
    drawn: impl Fn(&[Point]) -> Vec<Point>,
) -> Vec<Wire> {
    arrows
        .iter()
        .filter_map(|(arrow, key)| {
            let points = laid.points(&arrow.path, arrow.slot)?;
            let from = across::find(top, &arrow.from)?;
            let to = across::find(top, &arrow.to)?;
            if points.len() < 2 {
                return None;
            }
            let hand = hands.iter().find(|(_, line)| line.key == *key);
            Some(Wire {
                key: key.clone(),
                line: arrow.line,
                points: drawn(points),
                from: center(from.rect),
                to: center(to.rect),
                vias: hand.map(|(_, line)| line.vias.clone()).unwrap_or_default(),
                entry: hand.map(|&(entry, _)| entry),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_corner_on_a_bend_gets_short_runs_beside_it() {
        let points = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (200.0, 100.0)];
        assert_eq!(
            tight(points.clone(), &[(100.0, 100.0)]),
            [
                (0.0, 0.0),
                (100.0, 0.0),
                (100.0, 84.0),
                (100.0, 100.0),
                (116.0, 100.0),
                (200.0, 100.0)
            ]
        );
        // Not on a bend, or an end: as it was.
        assert_eq!(tight(points.clone(), &[(50.0, 0.0), (0.0, 0.0)]), points);
        // A short run beside it: half of it.
        let short = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 50.0)];
        assert_eq!(
            tight(short, &[(10.0, 0.0)]),
            [
                (0.0, 0.0),
                (5.0, 0.0),
                (10.0, 0.0),
                (10.0, 5.0),
                (10.0, 50.0)
            ]
        );
        // The curve through the corner on the bend passes close by it.
        let drawn = crate::curve::flatten(
            &tight(points.clone(), &[(100.0, 100.0)]),
            crate::curve::RADIUS,
        );
        let nearest = drawn
            .iter()
            .map(|p| (p.0 - 100.0).hypot(p.1 - 100.0))
            .fold(f32::INFINITY, f32::min);
        assert!(nearest < 4.0, "{nearest}");
    }
}
