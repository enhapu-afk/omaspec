//! Lays out a flow as tiles, PlantUML's way: every step is a tile with an
//! entry at its top and an exit at its bottom; a flow stacks its tiles and
//! joins them with arrows; `if`, loops and parallels set flows side by
//! side and draw the lines between them (the branching ones in
//! [`super::branch`]). Everything is placed as if there
//! were one swimlane, every point keeping the swimlane it belongs to;
//! [`super::draw`] then moves each swimlane's part into its column.

use super::skin::Skin;
use super::tile::{Edge, Pt, Shape, Tile, pt};
use super::{ActionShape, Arrow, Backward, Flow, LineStyle, Node, Note, Step, Text};
use crate::Declined;
use crate::color;
use crate::svg::{Align, Stroke};
use crate::text::{self, Block, Family, Font};

/// The arrow between two steps.
pub(super) const GAP: f32 = 20.0;
/// Half the size of an empty diamond, and the width of a test's pointed ends.
pub(super) const HALF: f32 = 12.0;
/// The narrowest a branch of an `if` or a parallel is, and the room beside it.
pub(super) const BRANCH_MIN: f32 = 30.0;
pub(super) const BRANCH_MARGIN: f32 = 10.0;
/// Between a loop and the line that leads back.
const RAIL: f32 = 15.0;
/// Between an action and its note.
const NOTE_GAP: f32 = 15.0;
/// Height of a `fork` bar.
pub(super) const BAR: f32 = 6.0;
/// The least room between the bottom of a tile and the top of the next.
pub(super) const CLEAR: f32 = 10.0;

pub struct Layout<'a> {
    pub skin: &'a Skin,
    pub family: Family,
}

impl Layout<'_> {
    pub(super) fn measure(&self, text: &Text, font: Font) -> Result<Block, Declined> {
        text::block(&text.value, font, self.family).map_err(|e| Declined::at(text.line, e))
    }

    pub(super) fn measure_opt(&self, text: Option<&Text>, font: Font) -> Result<Block, Declined> {
        text.map_or(Ok(Block::empty()), |t| self.measure(t, font))
    }

    /// The line an arrow is drawn with.
    pub(super) fn stroke(&self, arrow: Option<&Arrow>) -> Result<Stroke, Declined> {
        let mut stroke = self.skin.arrow;
        let Some(arrow) = arrow else {
            return Ok(stroke);
        };
        if let Some(c) = &arrow.color {
            stroke.color = match color::parse(c) {
                Ok(Some(c)) => c,
                Ok(None) => return Err(Declined::at(arrow.line, "an arrow cannot be transparent")),
                Err(e) => return Err(Declined::at(arrow.line, e)),
            };
        }
        match arrow.style {
            LineStyle::Plain => {}
            LineStyle::Dashed => stroke = stroke.dashed(7.0, 7.0),
            LineStyle::Dotted => stroke = stroke.dashed(1.0, 3.0),
            LineStyle::Bold => stroke.width *= 2.0,
        }
        Ok(stroke)
    }

    pub(super) fn arrow_label(&self, arrow: Option<&Arrow>) -> Result<Block, Declined> {
        self.measure_opt(arrow.and_then(|a| a.label.as_ref()), self.skin.arrow_font)
    }

    /// The room the labels on the arrows into `flows` take above them.
    pub(super) fn labels_above(&self, flows: &[&Flow]) -> Result<f32, Declined> {
        let mut room: f32 = 0.0;
        for flow in flows {
            let arrow = flow.steps.first().and_then(|s| s.arrow.as_ref());
            let label = self.arrow_label(arrow)?;
            if !label.is_empty() {
                room = room.max(label.height + 4.0);
            }
        }
        Ok(room)
    }

    /// A label beside a line going down from `(x, y)`.
    pub(super) fn beside(&self, text: Block, x: f32, y: f32, lane: usize) -> Option<Shape> {
        (!text.is_empty()).then_some(Shape::Label {
            at: (x + 6.0, y),
            text,
            align: Align::Left,
            lane,
        })
    }

    pub fn flow(&self, flow: &Flow, lane: usize) -> Result<Tile, Declined> {
        let mut tile: Option<Tile> = None;
        for step in &flow.steps {
            let mut next = self.step(step)?;
            tile = Some(match tile {
                None => {
                    next.arrow_in = step.arrow.clone().or(next.arrow_in.take());
                    next
                }
                Some(prev) => {
                    let arrow = step.arrow.clone().or(next.arrow_in.take());
                    self.join(prev, next, arrow.as_ref())?
                }
            });
        }
        let mut tile = tile.unwrap_or_else(|| {
            let mut empty = Tile::new(
                Vec::new(),
                Vec::new(),
                pt(0.0, 0.0, lane),
                Some(pt(0.0, 0.0, lane)),
            );
            empty.empty = true;
            empty
        });
        tile.arrow_out = flow.out.clone();
        Ok(tile)
    }

    /// `next` below `prev`, joined by an arrow if `prev` goes on.
    fn join(
        &self,
        mut prev: Tile,
        mut next: Tile,
        arrow: Option<&Arrow>,
    ) -> Result<Tile, Declined> {
        let label = self.arrow_label(arrow)?;
        let (top, bottom) = (next.bounds.1, prev.bounds.3);
        match prev.exit {
            Some(out) => {
                let turns = out.lane != next.entry.lane;
                let room = if label.is_empty() {
                    0.0
                } else {
                    label.height + 4.0
                };
                let need = GAP + room + if turns { CLEAR } else { 0.0 };
                let dy = (bottom + CLEAR - top).max(out.y + need - next.entry.y);
                next.translate(out.x - next.entry.x, dy);
                let entry = pt(out.x, next.entry.y, next.entry.lane);
                let mut points = vec![out];
                let mut y = out.y + (entry.y - out.y - label.height) / 2.0;
                if turns {
                    // A line to another swimlane turns across between the
                    // two tiles, below its label.
                    let gap_top = bottom.max(out.y + room);
                    let turn = (gap_top + next.bounds.1.min(entry.y - GAP / 2.0)) / 2.0;
                    points.push(pt(out.x, turn, out.lane));
                    points.push(pt(out.x, turn, entry.lane));
                    y = out.y + 2.0;
                }
                points.push(entry);
                prev.edges
                    .push(Edge::new(points, true, self.stroke(arrow)?));
                prev.shapes.extend(self.beside(label, out.x, y, out.lane));
            }
            None => next.translate(prev.entry.x - next.entry.x, bottom + GAP - top),
        }
        let exit = next.exit;
        prev.absorb(next);
        prev.exit = exit;
        prev.measure();
        Ok(prev)
    }

    fn step(&self, step: &Step) -> Result<Tile, Declined> {
        let lane = step.lane;
        let circle = |shape: Shape, d: f32, goes_on: bool| {
            let exit = goes_on.then_some(pt(d / 2.0, d, lane));
            Tile::new(vec![shape], Vec::new(), pt(d / 2.0, 0.0, lane), exit)
        };
        Ok(match &step.node {
            Node::Start => circle(
                Shape::Start {
                    center: (10.0, 10.0),
                    lane,
                },
                20.0,
                true,
            ),
            Node::Stop => circle(
                Shape::Stop {
                    center: (11.0, 11.0),
                    lane,
                },
                22.0,
                false,
            ),
            Node::End => circle(
                Shape::End {
                    center: (10.0, 10.0),
                    lane,
                },
                20.0,
                false,
            ),
            Node::Detach => Tile::new(Vec::new(), Vec::new(), pt(0.0, 0.0, lane), None),
            Node::Action {
                text,
                color,
                shape,
                note,
            } => self.action(text, color.as_deref(), *shape, note.as_ref(), lane)?,
            Node::If {
                branches,
                otherwise,
            } if branches.len() == 1 => self.if_else(&branches[0], otherwise, lane)?,
            Node::If {
                branches,
                otherwise,
            } => self.if_row(branches, otherwise, lane)?,
            Node::While {
                test,
                yes,
                body,
                out,
            } => self.while_loop(test, yes.as_ref(), body, out.as_ref(), lane)?,
            Node::Repeat {
                body,
                backward,
                test,
                yes,
                out,
            } => {
                let labels = [test.as_ref(), yes.as_ref(), out.as_ref()];
                self.repeat(body, backward.as_ref(), labels, lane)?
            }
            Node::Parallel { kind, flows } => self.parallel(*kind, flows, lane)?,
            Node::Group { title, color, flow } => {
                self.group(title, color.as_deref(), flow, lane)?
            }
        })
    }

    fn action(
        &self,
        text: &Text,
        color: Option<&str>,
        shape: ActionShape,
        note: Option<&Note>,
        lane: usize,
    ) -> Result<Tile, Declined> {
        let skin = self.skin;
        let block = self.measure(text, skin.activity.font)?;
        let fill = match color {
            Some(c) => color::parse(c).map_err(|e| Declined::at(text.line, e))?,
            None => skin.activity.fill,
        };
        let (w, h) = (
            block.width + 2.0 * skin.padding + room(shape).0,
            block.height + 2.0 * skin.padding,
        );
        let mut shapes = Vec::new();
        let mut top = 0.0;
        if let Some(note) = note {
            let body = self.measure(&note.text, skin.note.font)?;
            let (nw, nh) = (body.width + 22.0, body.height + 10.0);
            let fill = match &note.color {
                Some(c) => color::parse(c).map_err(|e| Declined::at(note.text.line, e))?,
                None => skin.note.fill,
            };
            let nx = if note.right {
                w + NOTE_GAP
            } else {
                -NOTE_GAP - nw
            };
            top = ((nh - h) / 2.0).max(0.0);
            shapes.push(Shape::Note {
                rect: (nx, ((h - nh) / 2.0).max(0.0), nw, nh),
                text: body,
                fill,
                lane,
            });
        }
        shapes.push(Shape::Box {
            rect: (0.0, top, w, h),
            text: block,
            fill,
            shape,
            lane,
        });
        Ok(Tile::new(
            shapes,
            Vec::new(),
            pt(w / 2.0, top, lane),
            Some(pt(w / 2.0, top + h, lane)),
        ))
    }

    /// A test's diamond: its size and measured text; an empty diamond
    /// without a test.
    pub(super) fn diamond(&self, test: Option<&Text>) -> Result<(f32, f32, Block), Declined> {
        let text = self.measure_opt(test, self.skin.diamond.font)?;
        if text.is_empty() || text.width == 0.0 {
            return Ok((2.0 * HALF, 2.0 * HALF, Block::empty()));
        }
        if self.skin.test_in_diamond {
            // Twice the text each way holds it inside the diamond's sides.
            return Ok((2.0 * text.width + 16.0, 2.0 * text.height + 8.0, text));
        }
        let w = text.width.max(2.0 * HALF) + 2.0 * HALF;
        let h = text.height.max(2.0 * HALF);
        Ok((w, h, text))
    }

    /// The line into a branch from `start` (a corner of a diamond or a
    /// point on a bar), with the label of its first arrow; for an empty
    /// branch only the start of the line, which [`Self::merge`] goes on.
    pub(super) fn branch_line(
        &self,
        shapes: &mut Vec<Shape>,
        start: Vec<Pt>,
        tile: &Tile,
    ) -> Result<Option<Edge>, Declined> {
        if tile.empty {
            return Ok(None);
        }
        let arrow = tile.arrow_in.as_ref();
        let last = *start.last().expect("a line starts somewhere");
        let mut points = start;
        points.push(tile.entry);
        // Just above the entry, below whatever the line starts at.
        let label = self.arrow_label(arrow)?;
        let y = tile.entry.y - label.height - 2.0;
        shapes.extend(self.beside(label, last.x, y, tile.entry.lane));
        Ok(Some(Edge::new(points, true, self.stroke(arrow)?)))
    }

    fn while_loop(
        &self,
        test: &Text,
        yes: Option<&Text>,
        body: &Flow,
        out: Option<&Text>,
        lane: usize,
    ) -> Result<Tile, Declined> {
        let font = self.skin.arrow_font;
        let (dw, dh, text) = self.diamond(Some(test))?;
        let yes = self.measure_opt(yes, font)?;
        let out = self.measure_opt(out, font)?;
        let cy = dh / 2.0;
        let top = dh + GAP.max(yes.height + 8.0) + self.labels_above(&[body])?;
        let mut body = self.flow(body, lane)?;
        body.translate(-body.entry.x, top - body.bounds.1);
        let mut shapes = vec![Shape::Diamond {
            rect: (-dw / 2.0, 0.0, dw, dh),
            text,
            lane,
        }];
        shapes.extend(self.beside(yes, -2.0, dh + 1.0, lane));
        let mut edges = Vec::new();
        let empty = body.empty;
        let start = vec![pt(0.0, dh, lane)];
        match self.branch_line(&mut shapes, start, &body)? {
            Some(edge) => edges.push(edge),
            None => edges.push(Edge::new(
                vec![pt(0.0, dh, lane), pt(0.0, top, lane)],
                false,
                self.stroke(None)?,
            )),
        }
        let bottom = if empty { top } else { body.bounds.3 };
        let back_y = bottom + CLEAR;
        if let Some(exit) = if empty {
            Some(pt(0.0, top, lane))
        } else {
            body.exit
        } {
            let rail = body.bounds.0.min(-dw / 2.0) - RAIL;
            edges.push(Edge::new(
                vec![
                    exit,
                    pt(exit.x, back_y, lane),
                    pt(rail, back_y, lane),
                    pt(rail, cy, lane),
                    pt(-dw / 2.0, cy, lane),
                ],
                true,
                self.stroke(body.arrow_out.as_ref())?,
            ));
        }
        if !out.is_empty() {
            shapes.push(Shape::Label {
                at: (dw / 2.0 + 2.0, cy - out.height - 1.0),
                text: out.clone(),
                align: Align::Left,
                lane,
            });
        }
        let rail = body.bounds.2.max(dw / 2.0 + out.width + 8.0) + RAIL;
        let exit_y = back_y + RAIL;
        edges.push(Edge::new(
            vec![
                pt(dw / 2.0, cy, lane),
                pt(rail, cy, lane),
                pt(rail, exit_y, lane),
                pt(0.0, exit_y, lane),
            ],
            false,
            self.stroke(None)?,
        ));
        let mut tile = Tile::new(
            shapes,
            edges,
            pt(0.0, 0.0, lane),
            Some(pt(0.0, exit_y, lane)),
        );
        tile.absorb(body);
        Ok(tile)
    }

    /// `repeat`: a diamond the way back comes into, the flow, the test.
    /// The way back runs on the left, or on the right through the
    /// `backward` action, where PlantUML puts it.
    fn repeat(
        &self,
        body: &Flow,
        backward: Option<&Backward>,
        [test, yes, out]: [Option<&Text>; 3],
        lane: usize,
    ) -> Result<Tile, Declined> {
        let font = self.skin.arrow_font;
        let yes = self.measure_opt(yes, font)?;
        let out = self.measure_opt(out, font)?;
        let (dw, dh, text) = self.diamond(test)?;
        let back = match backward {
            Some(b) => Some((
                self.measure(&b.text, self.skin.activity.font)?,
                self.measure_opt(b.incoming.as_ref(), font)?,
                self.measure_opt(b.outgoing.as_ref(), font)?,
            )),
            None => None,
        };
        let mut shapes = vec![Shape::Diamond {
            rect: (-HALF, 0.0, 2.0 * HALF, 2.0 * HALF),
            text: Block::empty(),
            lane,
        }];
        let mut edges = Vec::new();
        let top = 2.0 * HALF + GAP + self.labels_above(&[body])?;
        let mut body = self.flow(body, lane)?;
        body.translate(-body.entry.x, top - body.bounds.1);
        let empty = body.empty;
        let mut test_top = if empty { top } else { body.bounds.3 + GAP };
        if let Some((text, incoming, outgoing)) = &back {
            // Room on the right for the action and the lines to it.
            let height = text.height + 2.0 * self.skin.padding;
            let room = height + 2.0 * GAP + incoming.height + outgoing.height;
            test_top = test_top.max(HALF + room - dh / 2.0);
        }
        let start = vec![pt(0.0, 2.0 * HALF, lane)];
        match self.branch_line(&mut shapes, start, &body)? {
            Some(edge) => edges.push(edge),
            None => edges.push(Edge::new(
                vec![pt(0.0, 2.0 * HALF, lane), pt(0.0, test_top, lane)],
                true,
                self.stroke(None)?,
            )),
        }
        if !empty && let Some(exit) = body.exit {
            let mut points = vec![exit];
            if exit.x.abs() > 0.5 {
                let mid = (exit.y + test_top) / 2.0;
                points.extend([pt(exit.x, mid, lane), pt(0.0, mid, lane)]);
            }
            points.push(pt(0.0, test_top, lane));
            edges.push(Edge::new(
                points,
                true,
                self.stroke(body.arrow_out.as_ref())?,
            ));
        }
        shapes.push(Shape::Diamond {
            rect: (-dw / 2.0, test_top, dw, dh),
            text,
            lane,
        });
        let cy = test_top + dh / 2.0;
        let plain = self.stroke(None)?;
        match back {
            None => {
                let rail = body.bounds.0.min(-dw / 2.0 - yes.width - 8.0).min(-HALF) - RAIL;
                if !yes.is_empty() {
                    shapes.push(Shape::Label {
                        at: (-dw / 2.0 - 2.0, cy - yes.height - 1.0),
                        text: yes,
                        align: Align::Right,
                        lane,
                    });
                }
                edges.push(Edge::new(
                    vec![
                        pt(-dw / 2.0, cy, lane),
                        pt(rail, cy, lane),
                        pt(rail, HALF, lane),
                        pt(-HALF, HALF, lane),
                    ],
                    true,
                    plain,
                ));
            }
            Some((text, incoming, outgoing)) => {
                let pad = self.skin.padding;
                let (w, h) = (text.width + 2.0 * pad, text.height + 2.0 * pad);
                let right = if empty { 0.0 } else { body.bounds.2 };
                let right = right.max(dw / 2.0 + yes.width + 8.0).max(HALF);
                let x = right + RAIL + w / 2.0;
                let y = (HALF + cy) / 2.0 - h / 2.0;
                shapes.push(Shape::Box {
                    rect: (x - w / 2.0, y, w, h),
                    text,
                    fill: self.skin.activity.fill,
                    shape: ActionShape::Plain,
                    lane,
                });
                if !yes.is_empty() {
                    shapes.push(Shape::Label {
                        at: (dw / 2.0 + 2.0, cy - yes.height - 1.0),
                        text: yes,
                        align: Align::Left,
                        lane,
                    });
                }
                shapes.extend(self.beside(incoming, x, y + h + 2.0, lane));
                shapes.extend(self.beside(outgoing, x, HALF + 2.0, lane));
                edges.push(Edge::new(
                    vec![pt(dw / 2.0, cy, lane), pt(x, cy, lane), pt(x, y + h, lane)],
                    true,
                    plain,
                ));
                edges.push(Edge::new(
                    vec![pt(x, y, lane), pt(x, HALF, lane), pt(HALF, HALF, lane)],
                    true,
                    plain,
                ));
            }
        }
        let bottom = test_top + dh;
        shapes.extend(self.beside(out, -2.0, bottom + 1.0, lane));
        let mut tile = Tile::new(
            shapes,
            edges,
            pt(0.0, 0.0, lane),
            Some(pt(0.0, bottom, lane)),
        );
        tile.absorb(body);
        Ok(tile)
    }

    /// A partition: a frame around its flow, as wide as the flow and its
    /// name need, the name in a tab at the top left.
    fn group(
        &self,
        title: &Text,
        color: Option<&str>,
        flow: &Flow,
        lane: usize,
    ) -> Result<Tile, Declined> {
        let look = &self.skin.partition;
        let name = self.measure(title, look.font)?;
        let fill = match color {
            Some(c) => color::parse(c).map_err(|e| Declined::at(title.line, e))?,
            None => look.fill,
        };
        let mut inner = self.flow(flow, lane)?;
        let head = (name.height + 20.0).max(25.0);
        let mut width = (inner.width() + 2.0 * BRANCH_MARGIN).max(name.width + 20.0);
        inner.place((width - inner.width()) / 2.0, head);
        // The line into the flow passes right of the tab, not through it.
        let tab = if name.width > 0.0 {
            name.width + 10.0
        } else {
            0.0
        };
        let short = tab + 8.0 - inner.entry.x;
        if tab > 0.0 && short > 0.0 {
            inner.translate(short, 0.0);
            width += short;
        }
        let height = inner.bounds.3 + 20.0;
        let mut edges = Vec::new();
        let (entry, exit) = if inner.empty {
            // Nothing inside: the line goes straight through.
            let x = inner.entry.x;
            edges.push(Edge::new(
                vec![pt(x, 0.0, lane), pt(x, height, lane)],
                false,
                self.stroke(None)?,
            ));
            (pt(x, 0.0, lane), Some(pt(x, height, lane)))
        } else {
            (inner.entry, inner.exit)
        };
        let mut tile = Tile::new(
            vec![Shape::Frame {
                rect: (0.0, 0.0, width, height),
                title: name,
                fill,
                lane,
            }],
            edges,
            entry,
            exit,
        );
        tile.arrow_in = inner.arrow_in.take();
        tile.absorb(inner);
        Ok(tile)
    }
}

/// What an action's shape adds across its text, and where the text
/// starts in that room: the point, notch, slant or inner lines.
pub(super) fn room(shape: ActionShape) -> (f32, f32) {
    match shape {
        ActionShape::Plain | ActionShape::Task | ActionShape::Object => (0.0, 0.0),
        ActionShape::Input | ActionShape::Output | ActionShape::SendSignal => (SDL_POINT, 0.0),
        ActionShape::AcceptEvent => (SDL_POINT, SDL_POINT),
        ActionShape::Save | ActionShape::Load => (SDL_POINT, SDL_POINT / 2.0),
        ActionShape::Procedure => (2.0 * SDL_LINE, SDL_LINE),
        ActionShape::Continuous => (2.0 * SDL_BRACE, SDL_BRACE),
    }
}

/// How far an SDL point, notch or slant reaches.
pub(super) const SDL_POINT: f32 = 10.0;
/// Where a procedure's inner lines stand in.
pub(super) const SDL_LINE: f32 = 6.0;
/// How far a continuous signal's braces reach.
pub(super) const SDL_BRACE: f32 = 8.0;
