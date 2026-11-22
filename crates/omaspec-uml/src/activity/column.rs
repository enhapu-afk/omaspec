//! `!pragma useVerticalIf on`: the tests of an `if` with `elseif`s one
//! below the other, each branch right of its test, the `else` below the
//! last, the branches coming together on a rail on the right, as PlantUML
//! draws them.

use super::Branch;
use super::branch::lanes_of;
use super::layout::{BRANCH_MARGIN, CLEAR, GAP, HALF, Layout, RAIL};
use super::tile::{Edge, Pt, Shape, Tile, pt};
use crate::Declined;
use crate::svg::Align;
use crate::text::Block;

impl Layout<'_> {
    /// `if` with `elseif`s stacked. Branches in swimlanes of their own go
    /// in a row ([`Self::if_row`]), as they do without the pragma.
    pub(super) fn if_column(
        &self,
        branches: &[Branch],
        otherwise: &Branch,
        lane: usize,
    ) -> Result<Tile, Declined> {
        let font = self.skin.arrow_font;
        let mut tiles = Vec::new();
        for branch in branches {
            tiles.push(self.flow(&branch.flow, lane)?);
        }
        let mut rest = self.flow(&otherwise.flow, lane)?;
        if tiles
            .iter()
            .chain([&rest])
            .any(|tile| lanes_of(tile).iter().any(|&l| l != lane))
        {
            return self.if_row(branches, otherwise, lane);
        }
        let mut tests = Vec::new();
        let mut labels = Vec::new();
        let mut incoming = Vec::new();
        for branch in branches {
            tests.push(self.diamond(branch.test.as_ref())?);
            labels.push(self.measure_opt(branch.label.as_ref(), font)?);
            incoming.push(self.measure_opt(branch.incoming.as_ref(), font)?);
        }
        let out = self.measure_opt(otherwise.label.as_ref(), font)?;
        // The column of the branches: right of every test and the words
        // beside its corner, and of the words beside the line down from
        // one test to the next, or to the `else`.
        let left = tiles
            .iter()
            .filter(|tile| !tile.empty)
            .map(|tile| tile.entry.x - tile.bounds.0)
            .fold(0.0, f32::max);
        let corners = tests
            .iter()
            .zip(&labels)
            .map(|((w, _, _), label)| match label.is_empty() {
                true => w / 2.0 + CLEAR,
                false => w / 2.0 + 2.0 + label.width + 6.0,
            });
        let downs = incoming
            .iter()
            .skip(1)
            .chain([&out])
            .filter(|words| !words.is_empty())
            .map(|words| 6.0 + words.width + BRANCH_MARGIN + left);
        let widest = tests.iter().map(|(w, _, _)| w / 2.0).fold(0.0, f32::max);
        let x = corners
            .chain(downs)
            .fold(widest + BRANCH_MARGIN + left, f32::max);
        let plain = self.stroke(None)?;
        let mut shapes = Vec::new();
        let mut edges = Vec::new();
        // Each branch's way to the rail, from where it leaves its flow,
        // with the arrow it leaves by.
        let mut to_rail: Vec<(Vec<Pt>, usize)> = Vec::new();
        // How far right words stand that the rail passes.
        let mut words_right = x;
        let (mut y, mut above, mut foot) = (0.0, 0.0, 0.0);
        for (i, ((dw, dh, text), label)) in tests.into_iter().zip(labels).enumerate() {
            if i > 0 {
                // Down from the test before, below its branch.
                let room = match incoming[i].is_empty() {
                    true => 0.0,
                    false => incoming[i].height + 4.0,
                };
                y = (foot + GAP + room).max(above + GAP);
                shapes.extend(self.beside(incoming[i].clone(), 0.0, foot + 2.0, lane));
                edges.push(Edge::new(
                    vec![pt(0.0, foot, lane), pt(0.0, y, lane)],
                    true,
                    plain,
                ));
            }
            let cy = y + dh / 2.0;
            shapes.push(Shape::Diamond {
                rect: (-dw / 2.0, y, dw, dh),
                text,
                lane,
            });
            if !label.is_empty() {
                shapes.push(Shape::Label {
                    at: (dw / 2.0 + 2.0, cy - label.height - 1.0),
                    text: label,
                    align: Align::Left,
                    lane,
                });
            }
            let corner = pt(dw / 2.0, cy, lane);
            let tile = &mut tiles[i];
            let words = self.arrow_label(tile.arrow_out.as_deref())?;
            if tile.empty {
                // Straight on to the rail, the words of its arrow above.
                if !words.is_empty() {
                    words_right = words_right.max(x + 6.0 + words.width);
                    shapes.push(Shape::Label {
                        at: (x + 6.0, cy - words.height - 1.0),
                        text: words,
                        align: Align::Left,
                        lane,
                    });
                }
                to_rail.push((vec![corner, pt(x, cy, lane)], i));
                above = cy;
            } else {
                let top = y
                    + (dh / 2.0 + GAP).max(dh + CLEAR)
                    + self.labels_above(&[&branches[i].flow])?;
                tile.translate(x - tile.entry.x, top - tile.bounds.1);
                let first = self.arrow_label(tile.arrow_in.as_deref())?;
                if !first.is_empty() {
                    words_right = words_right.max(x + 6.0 + first.width);
                }
                let start = vec![corner, pt(x, cy, lane)];
                if let Some(edge) = self.branch_line(&mut shapes, start, tile)? {
                    edges.push(edge);
                }
                if let Some(exit) = tile.exit {
                    // A way out that comes in from the right (a `while`'s)
                    // goes on from there, not back over itself.
                    let from = from_right(&mut tile.edges, exit).unwrap_or(exit);
                    let turn = tile.bounds.3 + GAP / 2.0 + words.height;
                    if !words.is_empty() {
                        words_right = words_right.max(from.x + 6.0 + words.width);
                    }
                    shapes.extend(self.beside(words, from.x, from.y + 2.0, lane));
                    to_rail.push((vec![from, pt(from.x, turn, lane)], i));
                    above = turn;
                } else {
                    above = tile.bounds.3;
                }
            }
            foot = y + dh;
        }
        // The `else` below the last test and its branch.
        let room = match out.is_empty() {
            true => 0.0,
            false => out.height + 4.0,
        };
        shapes.extend(self.beside(out, 0.0, foot + 2.0, lane));
        let words = self.arrow_label(rest.arrow_out.as_deref())?;
        let gone = match words.is_empty() {
            true => 0.0,
            false => words.height + 4.0,
        };
        let (else_end, else_low) = if rest.empty {
            shapes.extend(self.beside(words, 0.0, foot + 2.0 + room, lane));
            (Some(vec![pt(0.0, foot, lane)]), foot + room + gone)
        } else {
            let top =
                (foot + GAP + room).max(above + GAP) + self.labels_above(&[&otherwise.flow])?;
            rest.translate(-rest.entry.x, top - rest.bounds.1);
            if let Some(edge) = self.branch_line(&mut shapes, vec![pt(0.0, foot, lane)], &rest)? {
                edges.push(edge);
            }
            match rest.exit {
                Some(exit) => {
                    shapes.extend(self.beside(words, exit.x, exit.y + 2.0, lane));
                    (Some(vec![exit]), rest.bounds.3 + gone)
                }
                None => (None, rest.bounds.3),
            }
        };
        // The rail right of all of it; where the ways meet, below it all.
        let right = tiles
            .iter()
            .filter(|tile| !tile.empty)
            .map(|tile| tile.bounds.2)
            .chain([words_right + 4.0])
            .chain((!rest.empty).then_some(rest.bounds.2))
            .fold(f32::NEG_INFINITY, f32::max);
        let rail = right + RAIL;
        let lowest = tiles
            .iter()
            .filter(|tile| !tile.empty)
            .map(|tile| tile.bounds.3)
            .chain(to_rail.iter().map(|(points, _)| points[points.len() - 1].y))
            .chain([foot, else_low])
            .fold(0.0, f32::max);
        let exit = match (to_rail.is_empty(), else_end) {
            (true, None) => None,
            (true, Some(points)) => points.last().copied(),
            (false, else_end) => {
                let top = lowest + GAP / 2.0 + 2.0;
                let cy = top + HALF;
                shapes.push(Shape::Diamond {
                    rect: (-HALF, top, 2.0 * HALF, 2.0 * HALF),
                    text: Block::empty(),
                    lane,
                });
                // Each way ends on the rail; the rail runs down from the
                // highest into the diamond, a piece from each to the next,
                // so a line is followed from its branch to the diamond.
                // A hidden way joins nothing.
                let mut joins = Vec::new();
                for (mut points, i) in to_rail {
                    let last = *points.last().expect("a line starts somewhere");
                    points.push(pt(rail, last.y, lane));
                    let arrow = tiles[i].arrow_out.as_deref();
                    let stroke = self.stroke(arrow)?;
                    if stroke.width > 0.0 {
                        joins.push(last.y);
                    }
                    edges.push(Edge::new(points, false, stroke).by(arrow));
                }
                joins.sort_by(f32::total_cmp);
                joins.dedup_by(|a, b| (*a - *b).abs() < 0.5);
                for pair in joins.windows(2) {
                    edges.push(Edge::new(
                        vec![pt(rail, pair[0], lane), pt(rail, pair[1], lane)],
                        false,
                        plain,
                    ));
                }
                if let Some(&last) = joins.last() {
                    edges.push(Edge::new(
                        vec![pt(rail, last, lane), pt(rail, cy, lane), pt(HALF, cy, lane)],
                        true,
                        plain,
                    ));
                }
                if let Some(mut points) = else_end {
                    points.push(pt(0.0, top, lane));
                    let arrow = rest.arrow_out.as_deref();
                    edges.push(Edge::new(points, true, self.stroke(arrow)?).by(arrow));
                }
                Some(pt(0.0, top + 2.0 * HALF, lane))
            }
        };
        let mut tile = Tile::new(shapes, edges, pt(0.0, 0.0, lane), exit);
        for t in tiles {
            tile.absorb(t);
        }
        tile.absorb(rest);
        Ok(tile)
    }
}

/// Where the line that ends at `exit` comes in from the right along it, as
/// a `while`'s way out does: that line ends where it turns to come in, and
/// the point it ends at now.
fn from_right(edges: &mut [Edge], exit: Pt) -> Option<Pt> {
    let edge = edges
        .iter_mut()
        .find(|edge| edge.points.last() == Some(&exit) && edge.points.len() >= 3)?;
    let before = edge.points[edge.points.len() - 2];
    let flat = before.lane == exit.lane && (before.y - exit.y).abs() < 0.5;
    (flat && before.x > exit.x + 0.5).then(|| {
        edge.points.pop();
        before
    })
}
