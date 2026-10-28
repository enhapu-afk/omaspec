//! The pieces a layout is made of: shapes, the lines between them and
//! tiles, which hold both and know where a flow enters and leaves them.
//! Every point keeps the swimlane it moves with.

use super::{ActionShape, Arrow};
use crate::color::Paint;
use crate::svg::{Align, Stroke};
use crate::text::Block;

/// A point and the swimlane it moves with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt {
    pub x: f32,
    pub y: f32,
    pub lane: usize,
}

pub fn pt(x: f32, y: f32, lane: usize) -> Pt {
    Pt { x, y, lane }
}

#[derive(Debug, Clone)]
pub enum Shape {
    /// An action: a rounded box with its text, or another shape.
    Box {
        rect: (f32, f32, f32, f32),
        text: Block,
        fill: Paint,
        shape: ActionShape,
        lane: usize,
    },
    /// A test with pointed ends, or an empty diamond when `text` is empty.
    Diamond {
        rect: (f32, f32, f32, f32),
        text: Block,
        lane: usize,
    },
    Start {
        center: (f32, f32),
        lane: usize,
    },
    Stop {
        center: (f32, f32),
        lane: usize,
    },
    End {
        center: (f32, f32),
        lane: usize,
    },
    /// A `fork` bar (`height` > 0) or the line of a `split`, across the
    /// spans of the flows it joins: each a left and a right end and the
    /// swimlane that flow is in.
    Bar {
        y: f32,
        height: f32,
        spans: Vec<(f32, f32, usize)>,
    },
    Note {
        rect: (f32, f32, f32, f32),
        text: Block,
        fill: Paint,
        lane: usize,
    },
    /// A partition: a frame around a flow, its name in a tab.
    Frame {
        rect: (f32, f32, f32, f32),
        title: Block,
        fill: Paint,
        lane: usize,
    },
    /// A label: its anchor, aligned by `align`, with its top at the anchor.
    Label {
        at: (f32, f32),
        text: Block,
        align: Align,
        lane: usize,
    },
}

impl Shape {
    /// Left, top, right and bottom.
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        match self {
            Shape::Box { rect, .. }
            | Shape::Diamond { rect, .. }
            | Shape::Note { rect, .. }
            | Shape::Frame { rect, .. } => (rect.0, rect.1, rect.0 + rect.2, rect.1 + rect.3),
            Shape::Start { center, .. } | Shape::End { center, .. } => (
                center.0 - 10.0,
                center.1 - 10.0,
                center.0 + 10.0,
                center.1 + 10.0,
            ),
            Shape::Stop { center, .. } => (
                center.0 - 11.0,
                center.1 - 11.0,
                center.0 + 11.0,
                center.1 + 11.0,
            ),
            Shape::Bar { y, height, spans } => {
                let l = spans.iter().map(|s| s.0).fold(f32::INFINITY, f32::min);
                let r = spans.iter().map(|s| s.1).fold(f32::NEG_INFINITY, f32::max);
                (l, *y, r, y + height)
            }
            Shape::Label {
                at, text, align, ..
            } => {
                let left = match align {
                    Align::Left => at.0,
                    Align::Center => at.0 - text.width / 2.0,
                    Align::Right => at.0 - text.width,
                };
                (left, at.1, left + text.width, at.1 + text.height)
            }
        }
    }

    pub fn lane(&self) -> Option<usize> {
        match self {
            Shape::Box { lane, .. }
            | Shape::Diamond { lane, .. }
            | Shape::Start { lane, .. }
            | Shape::Stop { lane, .. }
            | Shape::End { lane, .. }
            | Shape::Note { lane, .. }
            | Shape::Frame { lane, .. }
            | Shape::Label { lane, .. } => Some(*lane),
            Shape::Bar { .. } => None,
        }
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Shape::Box { rect, .. }
            | Shape::Diamond { rect, .. }
            | Shape::Note { rect, .. }
            | Shape::Frame { rect, .. } => {
                rect.0 += dx;
                rect.1 += dy;
            }
            Shape::Start { center, .. }
            | Shape::Stop { center, .. }
            | Shape::End { center, .. } => {
                center.0 += dx;
                center.1 += dy;
            }
            Shape::Bar { y, spans, .. } => {
                *y += dy;
                for s in spans {
                    s.0 += dx;
                    s.1 += dx;
                }
            }
            Shape::Label { at, .. } => {
                at.0 += dx;
                at.1 += dy;
            }
        }
    }
}

/// A line from point to point, with a head at its end if `head`.
#[derive(Debug, Clone)]
pub struct Edge {
    pub points: Vec<Pt>,
    pub head: bool,
    pub stroke: Stroke,
    /// The left and right corner of a diamond the line starts or ends
    /// at: when swimlanes move the other end past the diamond, the line
    /// takes the corner on that side.
    pub sides: [Option<(Pt, Pt)>; 2],
}

impl Edge {
    pub fn new(points: Vec<Pt>, head: bool, stroke: Stroke) -> Edge {
        Edge {
            points,
            head,
            stroke,
            sides: [None, None],
        }
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        let points = self
            .points
            .iter_mut()
            .chain(self.sides.iter_mut().flatten().flat_map(|(a, b)| [a, b]));
        for p in points {
            p.x += dx;
            p.y += dy;
        }
    }
}

/// A step or a flow, laid out.
pub struct Tile {
    pub shapes: Vec<Shape>,
    pub edges: Vec<Edge>,
    /// Where the arrow into it ends.
    pub entry: Pt,
    /// Where the arrow out of it starts; `None` when the flow ends in it.
    pub exit: Option<Pt>,
    /// The arrow leading to it and the one leaving it, for the flow
    /// around it to draw.
    pub arrow_in: Option<Arrow>,
    pub arrow_out: Option<Arrow>,
    /// A flow without steps: the line goes straight through.
    pub empty: bool,
    /// Left, top, right, bottom.
    pub bounds: (f32, f32, f32, f32),
}

impl Tile {
    pub fn new(shapes: Vec<Shape>, edges: Vec<Edge>, entry: Pt, exit: Option<Pt>) -> Tile {
        let mut tile = Tile {
            shapes,
            edges,
            entry,
            exit,
            arrow_in: None,
            arrow_out: None,
            empty: false,
            bounds: (entry.x, entry.y, entry.x, entry.y),
        };
        tile.measure();
        tile
    }

    pub fn measure(&mut self) {
        let mut b = (self.entry.x, self.entry.y, self.entry.x, self.entry.y);
        let mut grow = |(l, t, r, bo): (f32, f32, f32, f32)| {
            b = (b.0.min(l), b.1.min(t), b.2.max(r), b.3.max(bo));
        };
        for s in &self.shapes {
            grow(s.bounds());
        }
        for e in &self.edges {
            for p in &e.points {
                grow((p.x, p.y, p.x, p.y));
            }
        }
        if let Some(p) = self.exit {
            grow((p.x, p.y, p.x, p.y));
        }
        self.bounds = b;
    }

    pub fn translate(&mut self, dx: f32, dy: f32) {
        for s in &mut self.shapes {
            s.translate(dx, dy);
        }
        for e in &mut self.edges {
            e.translate(dx, dy);
        }
        self.entry.x += dx;
        self.entry.y += dy;
        if let Some(p) = &mut self.exit {
            p.x += dx;
            p.y += dy;
        }
        let b = &mut self.bounds;
        *b = (b.0 + dx, b.1 + dy, b.2 + dx, b.3 + dy);
    }

    /// Moves it so its left edge is at `x` and its top at `y`.
    pub fn place(&mut self, x: f32, y: f32) {
        self.translate(x - self.bounds.0, y - self.bounds.1);
    }

    pub fn width(&self) -> f32 {
        self.bounds.2 - self.bounds.0
    }

    pub fn absorb(&mut self, other: Tile) {
        let (l, t, r, b) = other.bounds;
        self.shapes.extend(other.shapes);
        self.edges.extend(other.edges);
        let s = &mut self.bounds;
        *s = (s.0.min(l), s.1.min(t), s.2.max(r), s.3.max(b));
    }
}
