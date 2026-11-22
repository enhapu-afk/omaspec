//! The steps that set flows side by side: `if` with its branches, the
//! row of an `elseif`, `fork` and `split`, and where their flows come
//! together again.

use super::layout::{BAR, BRANCH_MARGIN, BRANCH_MIN, CLEAR, GAP, HALF, Layout};
use super::tile::{Edge, Pt, Shape, Tile, pt};
use super::{Branch, Flow, LineStyle, Parallel};
use crate::Declined;
use crate::svg::Align;
use crate::text::Block;

/// Between two labels below a row of diamonds, or a label and the next
/// way down.
const LABEL_GAP: f32 = 6.0;

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
            let label = self.arrow_label(tile.arrow_out.as_deref())?;
            let room = if label.is_empty() {
                0.0
            } else {
                label.height + 4.0
            };
            if tile.empty {
                let last = *start.last().expect("a line starts somewhere");
                lowest = lowest.max(last.y + room);
                ends.push((start.clone(), tile.arrow_out.as_deref(), label));
            } else if let Some(exit) = tile.exit {
                lowest = lowest.max(tile.bounds.3 + room);
                ends.push((vec![exit], tile.arrow_out.as_deref(), label));
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

    /// `switch`: one diamond above its cases, side by side, as PlantUML
    /// draws it: a line from its left corner to each case left of its
    /// middle, from its right corner to each right of it, from its foot to
    /// the one in the middle; each case's words beside its line; a diamond
    /// below where the cases meet.
    pub(super) fn switch(
        &self,
        test: &super::Text,
        cases: &[Branch],
        lane: usize,
    ) -> Result<Tile, Declined> {
        let font = self.skin.arrow_font;
        let (dw, dh, text) = self.diamond(Some(test))?;
        // Cases in swimlanes further left go further left.
        let mut cases: Vec<&Branch> = cases.iter().collect();
        cases.sort_by_key(|case| entry_lane(&case.flow, lane));
        let labels = cases
            .iter()
            .map(|case| self.measure_opt(case.label.as_ref(), font))
            .collect::<Result<Vec<_>, _>>()?;
        let room = labels
            .iter()
            .filter(|label| !label.is_empty())
            .map(|label| label.height + 4.0)
            .fold(0.0, f32::max);
        let cy = dh / 2.0;
        let flows: Vec<&Flow> = cases.iter().map(|case| &case.flow).collect();
        let top = (cy + GAP).max(dh + CLEAR) + room + self.labels_above(&flows)?;
        let mut tiles = self.columns(&flows, lane, top)?;
        // Each case's words keep clear of the line to the next case.
        for i in 1..tiles.len() {
            let want = tiles[i - 1].entry.x + 6.0 + labels[i - 1].width + BRANCH_MARGIN;
            if tiles[i].entry.lane == tiles[i - 1].entry.lane && tiles[i].entry.x < want {
                let dx = want - tiles[i].entry.x;
                for tile in &mut tiles[i..] {
                    tile.translate(dx, 0.0);
                }
            }
        }
        let n = tiles.len();
        let (half, foot) = (n / 2, n % 2 == 1);
        let mine: Vec<f32> = tiles
            .iter()
            .filter(|tile| tile.entry.lane == lane)
            .map(|tile| tile.entry.x)
            .collect();
        let xs = if mine.is_empty() {
            vec![tiles[0].entry.x, tiles[n - 1].entry.x]
        } else {
            mine
        };
        let xd = (xs.iter().copied().fold(f32::INFINITY, f32::min)
            + xs.iter().copied().fold(f32::NEG_INFINITY, f32::max))
            / 2.0;
        // The cases on either side clear the diamond, the words of those
        // on its left too.
        let left = tiles[..half]
            .iter()
            .zip(&labels)
            .filter(|(tile, _)| tile.entry.lane == lane)
            .map(|(tile, label)| xd - dw / 2.0 - CLEAR - 6.0 - label.width - tile.entry.x)
            .fold(0.0, f32::min);
        for tile in tiles[..half]
            .iter_mut()
            .filter(|tile| tile.entry.lane == lane)
        {
            tile.translate(left, 0.0);
        }
        let from = if foot { half + 1 } else { half };
        let right = tiles[from..]
            .iter()
            .filter(|tile| tile.entry.lane == lane)
            .map(|tile| xd + dw / 2.0 + CLEAR - tile.entry.x)
            .fold(0.0, f32::max);
        for tile in tiles[from..]
            .iter_mut()
            .filter(|tile| tile.entry.lane == lane)
        {
            tile.translate(right, 0.0);
        }
        let mut shapes = vec![Shape::Diamond {
            rect: (xd - dw / 2.0, 0.0, dw, dh),
            text,
            lane,
        }];
        let corners = (pt(xd - dw / 2.0, cy, lane), pt(xd + dw / 2.0, cy, lane));
        let mut edges = Vec::new();
        let mut ways = Vec::new();
        for (i, (tile, label)) in tiles.iter().zip(labels).enumerate() {
            let at = tile.entry;
            let (start, side) = if foot && i == half {
                shapes.extend(self.beside(label, xd, dh + 2.0, lane));
                let mut start = vec![pt(xd, dh, lane)];
                if at.lane != lane || (at.x - xd).abs() > 0.5 {
                    let jog = dh + room + 2.0;
                    start.extend([pt(xd, jog, lane), pt(at.x, jog, at.lane)]);
                }
                (start, false)
            } else {
                let corner = if i < half { corners.0 } else { corners.1 };
                shapes.extend(self.beside(label, at.x, cy + 2.0, at.lane));
                (vec![corner, pt(at.x, cy, at.lane)], true)
            };
            if let Some(mut edge) = self.branch_line(&mut shapes, start.clone(), tile)? {
                if side {
                    edge.sides[0] = Some(corners);
                }
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
        // Each label stands beside its way down, on the side the line does
        // not turn to below the diamond: left of it where the line turns
        // right, toward a swimlane further right; else right of it.
        let left_of: Vec<bool> = (0..n).map(|i| tiles[i].entry.lane > lane).collect();
        let room = |i: usize, left: bool| match labels.get(i) {
            Some(label) if i < n && left_of[i] == left && !label.is_empty() => label.width + 4.0,
            _ => 0.0,
        };
        for i in 0..n {
            let next_half = sizes.get(i + 1).map_or(0.0, |s| s.0 / 2.0);
            let mut need = sizes[i].0 / 2.0 + next_half + (between[i].width + 12.0).max(20.0);
            // The labels below between this way down and the next clear of
            // each other and of the next way down.
            let below = room(i, false) + room(i + 1, true);
            if below > 0.0 {
                need = need.max(below + LABEL_GAP);
            }
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
            // Its label beside the way down, left of it where the line
            // turns right below the diamond.
            if left_of[i] && !label.is_empty() {
                shapes.push(Shape::Label {
                    at: (x - 4.0, y + h + 1.0),
                    text: label,
                    align: Align::Right,
                    lane,
                });
            } else {
                shapes.extend(self.beside(label, x - 2.0, y + h + 1.0, lane));
            }
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
        label: Option<&super::Text>,
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
        // Where every flow of a split starts with a hidden arrow, nothing
        // joins them at the top, as PlantUML draws it; a fork keeps its bar.
        let hidden = |flow: &Flow| {
            flow.steps
                .first()
                .and_then(|step| step.arrow.as_ref())
                .is_some_and(|arrow| arrow.style == LineStyle::Hidden)
        };
        let mut shapes = Vec::new();
        if kind != Parallel::Split || !flows.iter().all(hidden) {
            shapes.push(Shape::Bar {
                y: 0.0,
                height: bar,
                spans,
            });
        }
        let mut edges = Vec::new();
        let mut ways = Vec::new();
        for tile in &tiles {
            let start = vec![pt(tile.entry.x, bar, tile.entry.lane)];
            if let Some(edge) = self.branch_line(&mut shapes, start.clone(), tile)? {
                edges.push(edge);
            }
            ways.push((tile, start));
        }
        let above = shapes.len();
        let exit = if kind == Parallel::Merge {
            self.merge(&mut shapes, &mut edges, &ways, middle, lane)?
        } else {
            self.bar_below(&mut shapes, &mut edges, &ways, bar, middle, lane)?
        };
        // `end fork {and}`: its words right of the bar that joins the flows,
        // in the swimlane of its rightmost span.
        if let Some(label) = label
            && let Some((&(_, right, beside), y)) =
                shapes[above..].iter().find_map(|shape| match shape {
                    Shape::Bar { y, spans, .. } => spans
                        .iter()
                        .max_by(|a, b| a.2.cmp(&b.2).then(a.1.total_cmp(&b.1)))
                        .map(|span| (span, *y)),
                    _ => None,
                })
        {
            let text = self.measure(label, self.skin.arrow_font)?;
            let at = (right + 4.0, y + bar / 2.0 - text.height / 2.0);
            shapes.push(Shape::Label {
                at,
                text,
                align: crate::svg::Align::Left,
                lane: beside,
            });
        }
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
            let label = self.arrow_label(tile.arrow_out.as_deref())?;
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
        // A fork whose flows all end still has the bar that joins them,
        // and what follows goes on from it, as PlantUML draws it; a split
        // whose flows all end has no line.
        if ends.is_empty() && bar == 0.0 {
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
                self.stroke(tile.arrow_out.as_deref())?,
            ));
        }
        if bar == 0.0 {
            spans.push((middle, middle, lane));
        } else {
            // A fork's bar spans the flows that end before it too, as
            // PlantUML draws it, so the line on from its middle starts on it.
            for (tile, _) in ways
                .iter()
                .filter(|(tile, _)| !tile.empty && tile.exit.is_none())
            {
                spans.push((tile.bounds.0, tile.bounds.2, tile.entry.lane));
            }
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
