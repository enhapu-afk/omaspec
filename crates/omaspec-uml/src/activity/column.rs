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
        // The column of the branches, right of every test and the words
        // beside its corner.
        let reach = tests
            .iter()
            .zip(&labels)
            .map(|((w, _, _), label)| match label.is_empty() {
                true => w / 2.0 + CLEAR,
                false => w / 2.0 + 2.0 + label.width + 6.0,
            })
            .fold(0.0, f32::max);
        let widest = tests.iter().map(|(w, _, _)| w / 2.0).fold(0.0, f32::max);
        let left = tiles
            .iter()
            .filter(|tile| !tile.empty)
            .map(|tile| tile.entry.x - tile.bounds.0)
            .fold(0.0, f32::max);
        let x = reach.max(widest + BRANCH_MARGIN + left);
        let plain = self.stroke(None)?;
        let mut shapes = Vec::new();
        let mut edges = Vec::new();
        // Each branch's way on to the rail, from where it leaves its flow.
        let mut to_rail: Vec<(Vec<Pt>, usize)> = Vec::new();
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
            if tile.empty {
                // Straight on to the rail.
                to_rail.push((vec![corner, pt(x, cy, lane)], i));
                above = cy;
            } else {
                let top = y
                    + (dh / 2.0 + GAP).max(dh + CLEAR)
                    + self.labels_above(&[&branches[i].flow])?;
                tile.translate(x - tile.entry.x, top - tile.bounds.1);
                let start = vec![corner, pt(x, cy, lane)];
                if let Some(edge) = self.branch_line(&mut shapes, start, tile)? {
                    edges.push(edge);
                }
                if let Some(exit) = tile.exit {
                    let label = self.arrow_label(tile.arrow_out.as_deref())?;
                    let turn = tile.bounds.3 + GAP / 2.0 + label.height;
                    shapes.extend(self.beside(label, exit.x, exit.y + 2.0, lane));
                    to_rail.push((vec![exit, pt(exit.x, turn, lane)], i));
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
        let else_end = if rest.empty {
            Some(vec![pt(0.0, foot, lane)])
        } else {
            let top =
                (foot + GAP + room).max(above + GAP) + self.labels_above(&[&otherwise.flow])?;
            rest.translate(-rest.entry.x, top - rest.bounds.1);
            if let Some(edge) = self.branch_line(&mut shapes, vec![pt(0.0, foot, lane)], &rest)? {
                edges.push(edge);
            }
            rest.exit.map(|exit| vec![exit])
        };
        // The rail right of all of it; where the ways meet, below it all.
        let right = tiles
            .iter()
            .filter(|tile| !tile.empty)
            .map(|tile| tile.bounds.2)
            .chain([x, if rest.empty { 0.0 } else { rest.bounds.2 }])
            .fold(f32::NEG_INFINITY, f32::max);
        let rail = right + RAIL;
        let lowest = tiles
            .iter()
            .filter(|tile| !tile.empty)
            .map(|tile| tile.bounds.3)
            .chain(to_rail.iter().map(|(points, _)| points[points.len() - 1].y))
            .chain([foot, if rest.empty { foot } else { rest.bounds.3 }])
            .fold(0.0, f32::max);
        let exit = match (to_rail.is_empty(), else_end) {
            (true, None) => None,
            (true, Some(points)) => {
                let last = *points.last().expect("a line starts somewhere");
                Some(last)
            }
            (false, else_end) => {
                let top = lowest + GAP / 2.0 + 2.0;
                let cy = top + HALF;
                shapes.push(Shape::Diamond {
                    rect: (-HALF, top, 2.0 * HALF, 2.0 * HALF),
                    text: Block::empty(),
                    lane,
                });
                // The highest way runs down the rail into the diamond; the
                // others end on the rail.
                for (at, (mut points, i)) in to_rail.into_iter().enumerate() {
                    let last = *points.last().expect("a line starts somewhere");
                    points.push(pt(rail, last.y, lane));
                    if at == 0 {
                        points.extend([pt(rail, cy, lane), pt(HALF, cy, lane)]);
                    }
                    let stroke = self.stroke(tiles[i].arrow_out.as_deref())?;
                    edges.push(Edge::new(points, at == 0, stroke));
                }
                if let Some(mut points) = else_end {
                    points.push(pt(0.0, top, lane));
                    let stroke = self.stroke(rest.arrow_out.as_deref())?;
                    edges.push(Edge::new(points, true, stroke));
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
