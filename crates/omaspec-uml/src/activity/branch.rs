//! The steps that set flows side by side: `if` with its branches, the
//! row of an `elseif`, `fork` and `split`, and where their flows come
//! together again.

use super::layout::{BAR, BRANCH_MARGIN, BRANCH_MIN, CLEAR, GAP, HALF, Layout};
use super::tile::{Edge, Pt, Shape, Tile, pt};
use super::{Branch, Flow, Parallel};
use crate::Declined;
use crate::svg::Align;
use crate::text::Block;

impl Layout<'_> {
    /// The flows of branches side by side, each in a column of its own
    /// with its entry in the middle; columns start at x = 0, at `top`.
    pub(super) fn columns(
        &self,
        flows: &[&Flow],
        lane: usize,
        top: f32,
    ) -> Result<Vec<Tile>, Declined> {
        let mut x = 0.0;
        let mut tiles = Vec::new();
        for flow in flows {
            let mut tile = self.flow(flow, lane)?;
            let inner = tile.width().max(BRANCH_MIN);
            tile.place(x + BRANCH_MARGIN + (inner - tile.width()) / 2.0, top);
            x += inner + 2.0 * BRANCH_MARGIN;
            tiles.push(tile);
        }
        Ok(tiles)
    }

    /// Where the branches of an `if` or a `fork … end merge` come
    /// together: a diamond below them centered at `x`, if more than one
    /// goes on. `ways` are the tiles with the start of the line into each
    /// (for an empty branch, the line goes on to the diamond). The exit.
    pub(super) fn merge(
        &self,
        shapes: &mut Vec<Shape>,
        edges: &mut Vec<Edge>,
        ways: &[(&Tile, Vec<Pt>)],
        x: f32,
        lane: usize,
    ) -> Result<Option<Pt>, Declined> {
        let mut ends = Vec::new();
        let mut lowest: f32 = 0.0;
        for (tile, start) in ways {
            let label = self.arrow_label(tile.arrow_out.as_ref())?;
            let room = if label.is_empty() {
                0.0
            } else {
                label.height + 4.0
            };
            if tile.empty {
                let last = *start.last().expect("a line starts somewhere");
                lowest = lowest.max(last.y + room);
                ends.push((start.clone(), tile.arrow_out.as_ref(), label));
            } else if let Some(exit) = tile.exit {
                lowest = lowest.max(tile.bounds.3 + room);
                ends.push((vec![exit], tile.arrow_out.as_ref(), label));
            } else {
                lowest = lowest.max(tile.bounds.3);
            }
        }
        match ends.len() {
            0 => Ok(None),
            1 => {
                let (points, arrow, label) = ends.pop().expect("one end");
                let last = *points.last().expect("a line starts somewhere");
                if points.len() > 1 {
                    edges.push(Edge::new(points, false, self.stroke(arrow)?));
                }
                shapes.extend(self.beside(label, last.x, last.y + 2.0, last.lane));
                Ok(Some(last))
            }
            // In one swimlane, the branches end on a line across them and
            // the flow goes on from its middle.
            _ if self.skin.merge_line
                && ends
                    .iter()
                    .all(|(points, ..)| points.last().is_some_and(|p| p.lane == lane)) =>
            {
                let y = lowest + GAP / 2.0 + 2.0;
                let mut span = (x, x);
                for (mut points, arrow, label) in ends {
                    let last = *points.last().expect("a line starts somewhere");
                    shapes.extend(self.beside(label, last.x, last.y + 2.0, last.lane));
                    span = (span.0.min(last.x), span.1.max(last.x));
                    points.push(pt(last.x, y, last.lane));
                    edges.push(Edge::new(points, false, self.stroke(arrow)?));
                }
                shapes.push(Shape::Bar {
                    y,
                    height: 0.0,
                    spans: vec![(span.0, span.1, lane)],
                });
                Ok(Some(pt(x, y, lane)))
            }
            _ => {
                let top = lowest + GAP / 2.0 + 2.0;
                let cy = top + HALF;
                shapes.push(Shape::Diamond {
                    rect: (x - HALF, top, 2.0 * HALF, 2.0 * HALF),
                    text: Block::empty(),
                    lane,
                });
                let left = pt(x - HALF, cy, lane);
                let right = pt(x + HALF, cy, lane);
                for (mut points, arrow, label) in ends {
                    let last = *points.last().expect("a line starts somewhere");
                    shapes.extend(self.beside(label, last.x, last.y + 2.0, last.lane));
                    if (last.x - x).abs() < 0.5 {
                        points.push(pt(x, top, lane));
                        edges.push(Edge::new(points, true, self.stroke(arrow)?));
                        continue;
                    }
                    let corner = if last.x < x { left } else { right };
                    points.push(pt(last.x, cy, last.lane));
                    points.push(corner);
                    let mut edge = Edge::new(points, true, self.stroke(arrow)?);
                    edge.sides[1] = Some((left, right));
                    edges.push(edge);
                }
                Ok(Some(pt(x, top + 2.0 * HALF, lane)))
            }
        }
    }

    /// `if` with one test: the diamond between the two branches, `then`
    /// on the left, `else` on the right, as PlantUML draws it; the other
    /// way round when the `else` goes to a swimlane further left.
    pub(super) fn if_else(
        &self,
        then: &Branch,
        otherwise: &Branch,
        lane: usize,
    ) -> Result<Tile, Declined> {
        let font = self.skin.arrow_font;
        let (dw, dh, test) = self.diamond(then.test.as_ref())?;
        let mut sides = [then, otherwise];
        if entry_lane(&otherwise.flow, lane) < entry_lane(&then.flow, lane) {
            sides.reverse();
        }
        let labels = [
            self.measure_opt(sides[0].label.as_ref(), font)?,
            self.measure_opt(sides[1].label.as_ref(), font)?,
        ];
        let cy = dh / 2.0;
        let top = (cy + GAP).max(dh + CLEAR) + self.labels_above(&[&then.flow, &otherwise.flow])?;
        let mut tiles = self.columns(&[&sides[0].flow, &sides[1].flow], lane, top)?;
        // The diamond's corners, with the labels beside them, between the
        // two entries.
        let room = |label: &Block| (label.width + 6.0).max(10.0) + dw / 2.0;
        let x1 = tiles[0].entry.x;
        let lo = x1 + room(&labels[0]);
        let hi = tiles[1].entry.x - room(&labels[1]);
        if lo > hi {
            tiles[1].translate(lo - hi, 0.0);
        }
        let x2 = tiles[1].entry.x;
        let xd = ((x1 + x2) / 2.0).clamp(lo, lo.max(x2 - room(&labels[1])));

        let mut shapes = vec![Shape::Diamond {
            rect: (xd - dw / 2.0, 0.0, dw, dh),
            text: test,
            lane,
        }];
        let corners = (pt(xd - dw / 2.0, cy, lane), pt(xd + dw / 2.0, cy, lane));
        let [left_label, right_label] = labels;
        if !left_label.is_empty() {
            shapes.push(Shape::Label {
                at: (corners.0.x - 2.0, cy - left_label.height - 1.0),
                text: left_label,
                align: Align::Right,
                lane,
            });
        }
        if !right_label.is_empty() {
            shapes.push(Shape::Label {
                at: (corners.1.x + 2.0, cy - right_label.height - 1.0),
                text: right_label,
                align: Align::Left,
                lane,
            });
        }
        let mut edges = Vec::new();
        let mut ways = Vec::new();
        for (tile, corner) in tiles.iter().zip([corners.0, corners.1]) {
            let start = vec![corner, pt(tile.entry.x, cy, tile.entry.lane)];
            if let Some(mut edge) = self.branch_line(&mut shapes, start.clone(), tile)? {
                edge.sides[0] = Some(corners);
                edges.push(edge);
            }
            ways.push((tile, start));
        }
        let exit = self.merge(&mut shapes, &mut edges, &ways, xd, lane)?;
        let mut tile = Tile::new(shapes, edges, pt(xd, 0.0, lane), exit);
        for t in tiles {
            tile.absorb(t);
        }
        Ok(tile)
    }

    /// `if` with `elseif`s: a row of diamonds, each above its branch, the
    /// `else` on the right.
    pub(super) fn if_row(
        &self,
        branches: &[Branch],
        otherwise: &Branch,
        lane: usize,
    ) -> Result<Tile, Declined> {
        let font = self.skin.arrow_font;
        let mut tests = Vec::new();
        let mut labels = Vec::new();
        // The labels on the lines from one diamond to the next, and from
        // the last to the `else`.
        let mut between = Vec::new();
        for (i, b) in branches.iter().enumerate() {
            tests.push(self.diamond(b.test.as_ref())?);
            labels.push(self.measure_opt(b.label.as_ref(), font)?);
            if i > 0 {
                between.push(self.measure_opt(b.incoming.as_ref(), font)?);
            }
        }
        between.push(self.measure_opt(otherwise.label.as_ref(), font)?);
        let sizes: Vec<(f32, f32)> = tests.iter().map(|(w, h, _)| (*w, *h)).collect();
        let row = sizes.iter().map(|s| s.1).fold(0.0, f32::max);
        let cy = row / 2.0;
        let below = labels.iter().map(|l| l.height).fold(0.0, f32::max);
        let flows: Vec<&Flow> = branches
            .iter()
            .map(|b| &b.flow)
            .chain([&otherwise.flow])
            .collect();
        let top = row + GAP.max(below + 8.0) + self.labels_above(&flows)?;
        let mut tiles = self.columns(&flows, lane, top)?;
        let n = branches.len();
        for i in 0..n {
            let next_half = sizes.get(i + 1).map_or(0.0, |s| s.0 / 2.0);
            let need = sizes[i].0 / 2.0 + next_half + (between[i].width + 12.0).max(20.0);
            let have = tiles[i + 1].entry.x - tiles[i].entry.x;
            if have < need {
                for t in &mut tiles[i + 1..] {
                    t.translate(need - have, 0.0);
                }
            }
        }

        let mut shapes = Vec::new();
        let mut edges = Vec::new();
        let mut ways = Vec::new();
        let plain = self.stroke(None)?;
        for (i, ((w, h, test), label)) in tests.into_iter().zip(labels).enumerate() {
            let x = tiles[i].entry.x;
            let y = cy - h / 2.0;
            shapes.push(Shape::Diamond {
                rect: (x - w / 2.0, y, w, h),
                text: test,
                lane,
            });
            shapes.extend(self.beside(label, x - 2.0, y + h + 1.0, lane));
            let right = pt(x + w / 2.0, cy, lane);
            let on_line = &between[i];
            if !on_line.is_empty() {
                shapes.push(Shape::Label {
                    at: (right.x + 4.0, cy - on_line.height - 1.0),
                    text: on_line.clone(),
                    align: Align::Left,
                    lane,
                });
            }
            if i + 1 < n {
                let next = pt(tiles[i + 1].entry.x - sizes[i + 1].0 / 2.0, cy, lane);
                edges.push(Edge::new(vec![right, next], true, plain));
            } else {
                let tile = &tiles[n];
                let start = vec![right, pt(tile.entry.x, cy, tile.entry.lane)];
                if let Some(edge) = self.branch_line(&mut shapes, start.clone(), tile)? {
                    edges.push(edge);
                }
                ways.push((n, start));
            }
            let start = vec![pt(x, y + h, lane)];
            if let Some(edge) = self.branch_line(&mut shapes, start.clone(), &tiles[i])? {
                edges.push(edge);
            }
            ways.push((i, start));
        }
        ways.sort_by_key(|(i, _)| *i);
        let ways: Vec<(&Tile, Vec<Pt>)> = ways.into_iter().map(|(i, s)| (&tiles[i], s)).collect();
        let xs: Vec<f32> = ways
            .iter()
            .filter_map(|(t, start)| match t.empty {
                true => start.last().map(|p| p.x),
                false => t.exit.map(|p| p.x),
            })
            .collect();
        let lo = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let middle = if lo <= hi {
            (lo + hi) / 2.0
        } else {
            tiles[0].entry.x
        };
        let exit = self.merge(&mut shapes, &mut edges, &ways, middle, lane)?;
        let entry = pt(tiles[0].entry.x, cy - sizes[0].1 / 2.0, lane);
        let mut tile = Tile::new(shapes, edges, entry, exit);
        for t in tiles {
            tile.absorb(t);
        }
        Ok(tile)
    }

    pub(super) fn parallel(
        &self,
        kind: Parallel,
        flows: &[Flow],
        lane: usize,
    ) -> Result<Tile, Declined> {
        let bar = if kind == Parallel::Split { 0.0 } else { BAR };
        // Flows in swimlanes further left go further left, so no lines cross.
        let mut refs: Vec<&Flow> = flows.iter().collect();
        refs.sort_by_key(|f| entry_lane(f, lane));
        let tiles = self.columns(&refs, lane, bar + GAP + self.labels_above(&refs)?)?;
        let first = &tiles[0];
        let last = &tiles[tiles.len() - 1];
        let (left, right) = if kind == Parallel::Split {
            (first.entry.x, last.entry.x)
        } else {
            (first.bounds.0, last.bounds.2)
        };
        let middle = (left + right) / 2.0;
        let spans = tiles
            .iter()
            .map(|t| match kind {
                Parallel::Split => (t.entry.x, t.entry.x, t.entry.lane),
                _ => (t.bounds.0, t.bounds.2, t.entry.lane),
            })
            .collect();
        let mut shapes = vec![Shape::Bar {
            y: 0.0,
            height: bar,
            spans,
        }];
        let mut edges = Vec::new();
        let mut ways = Vec::new();
        for tile in &tiles {
            let start = vec![pt(tile.entry.x, bar, tile.entry.lane)];
            if let Some(edge) = self.branch_line(&mut shapes, start.clone(), tile)? {
                edges.push(edge);
            }
            ways.push((tile, start));
        }
        let exit = if kind == Parallel::Merge {
            self.merge(&mut shapes, &mut edges, &ways, middle, lane)?
        } else {
            self.bar_below(&mut shapes, &mut edges, &ways, bar, middle, lane)?
        };
        let mut tile = Tile::new(shapes, edges, pt(middle, 0.0, lane), exit);
        for t in tiles {
            tile.absorb(t);
        }
        Ok(tile)
    }

    /// The bar (or the line of a split) the flows of a parallel end at.
    pub(super) fn bar_below(
        &self,
        shapes: &mut Vec<Shape>,
        edges: &mut Vec<Edge>,
        ways: &[(&Tile, Vec<Pt>)],
        bar: f32,
        middle: f32,
        lane: usize,
    ) -> Result<Option<Pt>, Declined> {
        let mut lowest: f32 = 0.0;
        let mut ends = Vec::new();
        for (tile, start) in ways {
            let label = self.arrow_label(tile.arrow_out.as_ref())?;
            let room = if label.is_empty() {
                0.0
            } else {
                label.height + 4.0
            };
            lowest = lowest.max(tile.bounds.3 + room);
            let end = if tile.empty {
                Some(start.clone())
            } else {
                tile.exit.map(|p| vec![p])
            };
            if let Some(points) = end {
                ends.push((points, tile, label));
            }
        }
        if ends.is_empty() {
            return Ok(None);
        }
        let y = lowest + GAP;
        let mut spans = Vec::new();
        for (mut points, tile, label) in ends {
            let last = *points.last().expect("a line starts somewhere");
            shapes.extend(self.beside(label, last.x, last.y + 2.0, last.lane));
            points.push(pt(last.x, y, last.lane));
            spans.push((
                if bar > 0.0 {
                    tile.bounds.0.min(last.x)
                } else {
                    last.x
                },
                if bar > 0.0 {
                    tile.bounds.2.max(last.x)
                } else {
                    last.x
                },
                last.lane,
            ));
            edges.push(Edge::new(
                points,
                bar > 0.0,
                self.stroke(tile.arrow_out.as_ref())?,
            ));
        }
        if bar == 0.0 {
            spans.push((middle, middle, lane));
        }
        shapes.push(Shape::Bar {
            y,
            height: bar,
            spans,
        });
        Ok(Some(pt(middle, y + bar, lane)))
    }
}

/// The swimlane a flow starts in.
fn entry_lane(flow: &Flow, lane: usize) -> usize {
    flow.steps.first().map_or(lane, |s| s.lane)
}
