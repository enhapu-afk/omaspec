//! The steps that set flows side by side: `if` with its branches, the
//! row of an `elseif`, `fork` and `split`, and where their flows come
//! together again.

use std::cmp::Ordering;

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
        for (i, flow) in flows.iter().enumerate() {
            let mut tile = self.flow(flow, lane)?;
            let inner = tile.width().max(BRANCH_MIN);
            tile.place(x + BRANCH_MARGIN + (inner - tile.width()) / 2.0, top);
            // The words on the arrow into the flow, right of its line,
            // keep the next column off, where it starts in the same lane.
            let words = self.arrow_label(tile.arrow_in.as_deref())?;
            let beside = flows[i + 1..]
                .iter()
                .any(|next| entry_lane(next, lane) == tile.entry.lane);
            let reach = match words.is_empty() || !beside {
                true => 0.0,
                false => tile.entry.x + 6.0 + words.width + BRANCH_MARGIN,
            };
            x = (x + inner + 2.0 * BRANCH_MARGIN).max(reach);
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
                // Below the test, where the flow would have started.
                let last = *start.last().expect("a line starts somewhere");
                lowest = lowest.max(last.y + room).max(tile.entry.y - CLEAR);
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
                    edges.push(Edge::new(points, false, self.stroke(arrow)?).by(arrow));
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
                    edges.push(Edge::new(points, false, self.stroke(arrow)?).by(arrow));
                }
                shapes.push(Shape::Bar {
                    y,
                    height: 0.0,
                    spans: vec![(span.0, span.1, lane)],
                });
                Ok(Some(pt(x, y, lane)))
            }
            _ => {
                // A branch that comes down within the diamond's width turns
                // to its top just above it, with room for the arrowhead.
                let near = |points: &Vec<Pt>| {
                    let last = points.last().expect("a line starts somewhere");
                    last.lane == lane && (0.5..HALF).contains(&(last.x - x).abs())
                };
                let top = match ends.iter().any(|(points, ..)| near(points)) {
                    true => lowest + 4.0 + JOG,
                    false => lowest + GAP / 2.0 + 2.0,
                };
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
                    // One above the diamond goes into its top; one that would
                    // come down through it turns to its middle just above.
                    if last.lane == lane && (last.x - x).abs() < HALF {
                        if (last.x - x).abs() >= 0.5 {
                            let jog = top - JOG;
                            points.push(pt(last.x, jog, lane));
                            points.push(pt(x, jog, lane));
                        }
                        points.push(pt(x, top, lane));
                        edges.push(Edge::new(points, true, self.stroke(arrow)?).by(arrow));
                        continue;
                    }
                    let corner = if last.x < x { left } else { right };
                    points.push(pt(last.x, cy, last.lane));
                    points.push(corner);
                    let mut edge = Edge::new(points, true, self.stroke(arrow)?).by(arrow);
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
        let (dw, dh, test, beside) = self.test(then.test.as_ref())?;
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
        // `conditionStyle diamond`: the test above, right of the line in.
        shapes.extend(self.test_words(beside, xd, lane));
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
        // A `switch` keeps its hexagon under `conditionStyle diamond`, as
        // a row of `elseif`s does, as PlantUML draws it.
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
        // A case in a swimlane left of the switch's takes the left corner,
        // one right of it the right corner; those in its own lane split
        // about the middle, the one in the middle (of an odd count) taking
        // the foot.
        let n = tiles.len();
        let mine: Vec<usize> = (0..n).filter(|&i| tiles[i].entry.lane == lane).collect();
        let k = mine.len();
        let sides: Vec<Side> = (0..n)
            .map(|i| match tiles[i].entry.lane.cmp(&lane) {
                Ordering::Less => Side::Left,
                Ordering::Greater => Side::Right,
                Ordering::Equal => {
                    let j = mine.iter().position(|&m| m == i).expect("in its lane");
                    match (j < k / 2, k % 2 == 1 && j == k / 2) {
                        (true, _) => Side::Left,
                        (false, true) => Side::Foot,
                        (false, false) => Side::Right,
                    }
                }
            })
            .collect();
        let xd = match (mine.first(), mine.last()) {
            (Some(&a), Some(&b)) => (tiles[a].entry.x + tiles[b].entry.x) / 2.0,
            _ => 0.0,
        };
        // The words on the arrow into each case, right of its line.
        let arrows = tiles
            .iter()
            .map(|tile| {
                self.arrow_label(tile.arrow_in.as_deref()).map(|words| {
                    if words.is_empty() {
                        0.0
                    } else {
                        6.0 + words.width
                    }
                })
            })
            .collect::<Result<Vec<f32>, _>>()?;
        // The one at the foot goes straight down from it.
        let foot = (0..n).find(|&i| sides[i] == Side::Foot);
        if let Some(i) = foot {
            let dx = xd - tiles[i].entry.x;
            tiles[i].translate(dx, 0.0);
        }
        // The cases in the switch's lane clear the diamond and the case at
        // its foot: those on its left with their words, those on its right
        // the words of the one at its foot too.
        let (foot_left, foot_right) = foot.map_or((f32::INFINITY, f32::NEG_INFINITY), |i| {
            let words = match labels[i].is_empty() {
                true => 0.0,
                false => 6.0 + labels[i].width,
            };
            let reach = (tiles[i].bounds.2)
                .max(xd + words)
                .max(tiles[i].entry.x + arrows[i]);
            (tiles[i].bounds.0, reach)
        });
        // A side moves the cases on it that share a swimlane with the
        // switch, with the case at its foot or with another that moves, so
        // a case that runs on into another lane keeps clear of its
        // neighbours there and one alone in its lane stays: far enough for
        // each to clear the case at the foot, and those in the switch's
        // lane the diamond (on the left with their words).
        let lanes: Vec<Vec<usize>> = tiles.iter().map(lanes_of).collect();
        let foot_lanes = foot.map_or(Vec::new(), |i| lanes[i].clone());
        let moves = |side: Side| {
            let mut near: Vec<usize> = foot_lanes.iter().copied().chain([lane]).collect();
            let mut moving = vec![false; n];
            loop {
                let more: Vec<usize> = (0..n)
                    .filter(|&i| sides[i] == side && !moving[i])
                    .filter(|&i| lanes[i].iter().any(|l| near.contains(l)))
                    .collect();
                if more.is_empty() {
                    return moving;
                }
                for i in more {
                    moving[i] = true;
                    near.extend(&lanes[i]);
                }
            }
        };
        let (on_left, on_right) = (moves(Side::Left), moves(Side::Right));
        let here: Vec<bool> = tiles.iter().map(|tile| tile.entry.lane == lane).collect();
        let left = (0..n)
            .filter(|&i| on_left[i])
            .map(|i| {
                let reach = tiles[i].bounds.2.max(tiles[i].entry.x + arrows[i]);
                let clear = foot_left - 2.0 * BRANCH_MARGIN - reach;
                match here[i] {
                    true => {
                        clear.min(xd - dw / 2.0 - CLEAR - 6.0 - labels[i].width - tiles[i].entry.x)
                    }
                    false => clear,
                }
            })
            .fold(0.0, f32::min);
        let right = (0..n)
            .filter(|&i| on_right[i])
            .map(|i| {
                let clear = foot_right + 2.0 * BRANCH_MARGIN - tiles[i].bounds.0;
                match here[i] {
                    true => clear.max(xd + dw / 2.0 + CLEAR - tiles[i].entry.x),
                    false => clear,
                }
            })
            .fold(0.0, f32::max);
        for (i, tile) in tiles.iter_mut().enumerate() {
            if on_left[i] {
                tile.translate(left, 0.0);
            } else if on_right[i] {
                tile.translate(right, 0.0);
            }
        }
        let mut shapes = vec![Shape::Diamond {
            rect: (xd - dw / 2.0, 0.0, dw, dh),
            text,
            lane,
        }];
        let corners = (pt(xd - dw / 2.0, cy, lane), pt(xd + dw / 2.0, cy, lane));
        let mut edges = Vec::new();
        let mut ways = Vec::new();
        for ((tile, label), side) in tiles.iter().zip(labels).zip(sides) {
            let at = tile.entry;
            let (start, side) = if side == Side::Foot {
                shapes.extend(self.beside(label, xd, dh + 2.0, lane));
                let mut start = vec![pt(xd, dh, lane)];
                if at.lane != lane || (at.x - xd).abs() > 0.5 {
                    let jog = dh + room + 2.0;
                    start.extend([pt(xd, jog, lane), pt(at.x, jog, at.lane)]);
                }
                (start, false)
            } else {
                let corner = match side {
                    Side::Left => corners.0,
                    _ => corners.1,
                };
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
            edges.push(
                Edge::new(points, bar > 0.0, self.stroke(tile.arrow_out.as_deref())?)
                    .by(tile.arrow_out.as_deref()),
            );
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

/// The swimlanes `tile` draws in.
pub(super) fn lanes_of(tile: &Tile) -> Vec<usize> {
    let mut lanes: Vec<usize> = tile.shapes.iter().filter_map(Shape::lane).collect();
    for shape in &tile.shapes {
        if let Shape::Bar { spans, .. } = shape {
            lanes.extend(spans.iter().map(|span| span.2));
        }
    }
    lanes.extend(
        tile.edges
            .iter()
            .flat_map(|edge| edge.points.iter().map(|p| p.lane)),
    );
    lanes.push(tile.entry.lane);
    lanes.sort_unstable();
    lanes.dedup();
    lanes
}

/// From where a line turns to the top of the diamond below it to that
/// top: an arrowhead's length and a little.
const JOG: f32 = 14.0;

/// Which way a case of a `switch` leaves its diamond.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Foot,
    Right,
}

/// The swimlane a flow starts in.
fn entry_lane(flow: &Flow, lane: usize) -> usize {
    flow.steps.first().map_or(lane, |s| s.lane)
}
