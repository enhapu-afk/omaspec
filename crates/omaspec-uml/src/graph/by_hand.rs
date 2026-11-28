//! Lines by hand ([0016](../../../../docs/adr/0016-lines-by-hand.md)) for
//! every drawer of graph diagrams: once everything is placed, each line
//! the writer shaped is drawn as they shaped it ([`hand::shape`]), its
//! label placed along it ([`across::link`]), and each line becomes a
//! [`Wire`] of the map.
//!
//! A drawer names its lines ([`Arrow`], [`keyed`]) and lets them be read
//! and changed ([`Laid`]); the rest is the same for all of them.

use super::Rect;
use super::across::{self, Drawn, Link, Placement};
use super::route::Point;
use crate::hand::{self, Key};
use crate::map::{Head, Wire};
use crate::svg::Stroke;

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
    /// The ends it meets at a row of their own, not anywhere on the box
    /// (a class's member's): a line shaped by hand leaves and enters
    /// there too.
    pub lead: [Option<Lead>; 2],
}

/// Where a line meets its box at a row of its own: level, out of the
/// side that faces where it goes, a stem's length before it turns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lead {
    pub y: f32,
    pub stem: f32,
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
    /// Where the line's words are: its label, the multiplicities at its
    /// ends.
    fn labels(&self, _path: &[usize], _slot: usize) -> Vec<Rect> {
        Vec::new()
    }
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

/// Draws each line of `hands` in its shape; `top` is the placed boxes.
/// Returns the entries that did not hold, by line: with why.
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
        // A line from a state to the state it lies in ends at its title,
        // and corners inside it are the line's own (REQ-072).
        let (from, to) = (
            across::end_at(from, &arrow.from, &arrow.to).0,
            across::end_at(to, &arrow.to, &arrow.from).0,
        );
        let Some(laid) = laid.line_mut(&arrow.path, arrow.slot) else {
            continue;
        };
        if laid.points.len() < 2 {
            continue;
        }
        let span = hand::span(from.rect, to.rect);
        let (corners, far): (Vec<Point>, Vec<Point>) = line
            .corners
            .iter()
            .map(|c| c.at(span))
            .partition(|&p| near(p));
        if !far.is_empty() {
            misses.push((*entry, hand::far_out(&line.key, far.len())));
        }
        let (corners, inside): (Vec<Point>, Vec<Point>) = corners
            .into_iter()
            .partition(|&p| !hand::inside(from.rect, p) && !hand::inside(to.rect, p));
        if !inside.is_empty() {
            misses.push((*entry, hand::left_out(&line.key, inside.len())));
        }
        if corners.is_empty() {
            continue;
        }
        // Where an end meets its box at a row of its own, it leaves the
        // side that faces the corner next to it, level, along a stem.
        let lead = |lead: Option<Lead>, rect: Rect, corner: Point| {
            let lead = lead?;
            let right = corner.0 > rect.x + rect.w / 2.0;
            let side = if right { rect.x + rect.w } else { rect.x };
            let tip = if right {
                side + lead.stem
            } else {
                side - lead.stem
            };
            let dot = hand::End {
                rect: Rect {
                    x: tip,
                    y: lead.y,
                    w: 0.0,
                    h: 0.0,
                },
                flat: false,
            };
            Some((dot, (side, lead.y)))
        };
        let leave = lead(arrow.lead[0], from.rect, corners[0]);
        let enter = lead(arrow.lead[1], to.rect, corners[corners.len() - 1]);
        let mut shape = hand::shape(
            leave.map_or(end(from), |(dot, _)| dot),
            enter.map_or(end(to), |(dot, _)| dot),
            &corners,
        );
        if let Some((_, at)) = leave {
            shape.insert(0, at);
        }
        shape.extend(enter.map(|(_, at)| at));
        links.push(Link {
            from: arrow.from.clone(),
            to: arrow.to.clone(),
            label: laid.label,
            shape: Some(shape),
            stand: [None; 2],
            sealed: [false; 2],
        });
        // Out of what the others keep clear of; back if it has no ends.
        which.push((index, *entry, std::mem::take(laid.points)));
    }
    let drawn = laid.drawn();
    let shaped = across::link(top, &links, &drawn);
    for ((index, entry, before), linked) in which.into_iter().zip(shaped) {
        let (arrow, key) = &arrows[index];
        let Some(line) = laid.line_mut(&arrow.path, arrow.slot) else {
            continue;
        };
        let Some(linked) = linked else {
            *line.points = before;
            continue;
        };
        // Drawn as any line: its corners curve like the layout's (0011), so
        // a line dragged out of the layout keeps the look of the others.
        *line.points = linked.points;
        *line.label_at = linked.label_at;
        laid.relaid(&arrow.path, arrow.slot);
        if linked.through {
            misses.push((entry, hand::through(key)));
        }
    }
    misses.sort_by_key(|&(line, _)| line);
    misses
}

/// Every box under `top` a writer may hold in place
/// ([0021](../../../../docs/adr/0021-boxes-held-in-place.md)): each with a
/// name (`name`, by its path; a pseudo state's `[*]` or `[H]` is none), the
/// container it keeps to and which way its ranks run (`across`, by the
/// container's path), the entry of `held` that holds it, and the part it
/// is drawn in, where its name is not (`part`, by its path).
pub fn places(
    top: &[Placement],
    name: impl Fn(&[usize]) -> String,
    across: impl Fn(&[usize]) -> bool,
    held: &[(usize, hand::Hold)],
    part: impl Fn(&[usize]) -> Option<String>,
) -> Vec<crate::map::Place> {
    fn walk(
        level: &[Placement],
        path: &mut Vec<usize>,
        visit: &mut impl FnMut(&[usize], &Placement),
    ) {
        for (index, place) in level.iter().enumerate() {
            path.push(index);
            visit(path, place);
            walk(&place.children, path, visit);
            path.pop();
        }
    }
    let mut out = Vec::new();
    walk(top, &mut Vec::new(), &mut |path, place| {
        let name = name(path);
        if name.is_empty() || name.starts_with('[') {
            return;
        }
        let within = path[..path.len() - 1].to_vec();
        out.push(crate::map::Place {
            entry: held
                .iter()
                .find(|(_, h)| h.name == name)
                .map(|&(line, _)| line),
            across: across(&within),
            name,
            rect: place.rect,
            within,
            frame: place.frame,
            fixed: false,
            part: part(path),
        });
    });
    out
}

/// The end of a line as the hand tool shapes it ([`across::end_at`]).
fn end(at: super::route::End) -> hand::End {
    hand::End {
        rect: at.rect,
        flat: at.flat,
    }
}

/// How a line is drawn: its stroke, and the marks at its ends.
pub struct Dress {
    pub stroke: Stroke,
    pub heads: Vec<Head>,
    /// The parts its words are drawn in, where the drawer marks them
    /// ([`crate::map::Wire::words`]).
    pub words: [Option<String>; 3],
}

/// Every line drawn, as a [`Wire`] for the map; `drawn` turns a line's
/// points into the way it is drawn (its curves), `dress` tells how the
/// line at a slot of a container is drawn, given those points.
pub fn wires(
    top: &[Placement],
    arrows: &[(Arrow, Key)],
    laid: &impl Laid,
    hands: &[(usize, hand::Line)],
    drawn: impl Fn(&[Point]) -> Vec<Point>,
    dress: impl Fn(&[usize], usize, &[Point]) -> Dress,
) -> Vec<Wire> {
    arrows
        .iter()
        .filter_map(|(arrow, key)| {
            let points = laid.points(&arrow.path, arrow.slot)?;
            let from = across::end_at(across::find(top, &arrow.from)?, &arrow.from, &arrow.to).0;
            let to = across::end_at(across::find(top, &arrow.to)?, &arrow.to, &arrow.from).0;
            if points.len() < 2 {
                return None;
            }
            let hand = hands.iter().find(|(_, line)| line.key == *key);
            let drawn = drawn(points);
            let Dress {
                stroke,
                heads,
                words,
            } = dress(&arrow.path, arrow.slot, &drawn);
            Some(Wire {
                key: key.clone(),
                line: arrow.line,
                points: drawn,
                // Without the short runs beside its corners.
                path: hand::straighten(points.to_vec()),
                from: end(from),
                to: end(to),
                entry: hand.map(|&(entry, _)| entry),
                labels: laid.labels(&arrow.path, arrow.slot),
                stroke,
                heads,
                fixed: false,
                words,
            })
        })
        .collect()
}
