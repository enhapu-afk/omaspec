//! Where the parts of a drawn diagram are, in the units of the finished
//! picture: the boxes of states, the routes of transitions, the arrows of
//! messages. A viewer lights them up while a specification runs. It also
//! names every line a writer can shape by hand ([`Wire`]), and says which
//! entries of the block of lines by hand did not hold.

use crate::graph::Rect;
use crate::hand::{self, Corner, End, Key};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Map {
    pub states: Vec<Spot>,
    pub transitions: Vec<Route>,
    pub messages: Vec<Step>,
    pub wires: Vec<Wire>,
    /// The boxes of a map drawn as tiles that show other diagrams.
    pub tiles: Vec<Tile>,
    /// Entries of the block of lines by hand that did not hold: the
    /// zero-based line of each, and why.
    pub misses: Vec<(usize, String)>,
    /// How the drawing's units became the picture's.
    pub fit: Fit,
}

/// A line of a graph diagram, which the writer may shape by hand
/// ([`crate::hand`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Wire {
    /// What the block of lines by hand calls it.
    pub key: Key,
    /// Zero-based line of what draws it, where the drawer knows it.
    pub line: Option<usize>,
    /// As drawn, from the box it leaves to the tip of its head.
    pub points: Vec<(f32, f32)>,
    /// Its straight runs as laid, before their corners were rounded: from
    /// the box it leaves to the one it enters, a point at each corner.
    pub path: Vec<(f32, f32)>,
    /// The boxes it leaves and enters.
    pub from: End,
    pub to: End,
    /// The zero-based line of the entry that shapes it; none when the
    /// layout laid it.
    pub entry: Option<usize>,
}

impl Wire {
    /// Its corners: where its runs meet.
    pub fn corners(&self) -> &[(f32, f32)] {
        match self.path.len() {
            0..=2 => &[],
            n => &self.path[1..n - 1],
        }
    }

    /// The line through `corners`, points of the picture that `fit` made
    /// of the drawing, as it will be drawn ([`hand::shape`]): what a drag
    /// shows.
    pub fn shaped(&self, corners: &[(f32, f32)], fit: Fit) -> Vec<(f32, f32)> {
        let back = |end: End| End {
            rect: fit.back_rect(end.rect),
            ..end
        };
        let corners: Vec<(f32, f32)> = corners.iter().map(|&p| fit.back(p)).collect();
        hand::shape(back(self.from), back(self.to), &corners)
            .into_iter()
            .map(|p| fit.point(p))
            .collect()
    }

    /// The fewest of `corners` that shape the same line: one that only
    /// lies on a straight run between two others goes, unless the line
    /// needs it to keep where it runs.
    pub fn simplest(&self, corners: &[(f32, f32)], fit: Fit) -> Vec<(f32, f32)> {
        let close = |a: &[(f32, f32)], b: &[(f32, f32)]| {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(p, q)| (p.0 - q.0).abs() < 0.01 && (p.1 - q.1).abs() < 0.01)
        };
        let full = self.shaped(corners, fit);
        let mut kept = corners.to_vec();
        let mut i = 0;
        while i < kept.len() {
            let mut fewer = kept.clone();
            fewer.remove(i);
            if !fewer.is_empty() && close(&self.shaped(&fewer, fit), &full) {
                kept = fewer;
            } else {
                i += 1;
            }
        }
        kept
    }

    /// The entry for `corners`, points of the picture: each as its share
    /// of the span of the two boxes, which `fit` leaves as it is.
    pub fn entry_for(&self, corners: &[(f32, f32)]) -> hand::Line {
        let span = hand::span(self.from.rect, self.to.rect);
        hand::Line {
            key: self.key.clone(),
            corners: corners.iter().map(|&p| Corner::of(p, span)).collect(),
        }
    }
}

/// A state's box, or the mark of a pseudo state.
#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    /// The name transitions use; empty for a start or an end.
    pub name: String,
    pub shape: Shape,
    /// Zero-based line where the state first appears.
    pub line: usize,
    /// The name of the state it sits in; `None` at the top.
    pub parent: Option<String>,
    pub rect: Rect,
    /// How round its corners are: a circle's radius for a start or an end.
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    State,
    Start,
    End,
    Choice,
    /// A fork or a join.
    Bar,
    History,
}

/// A box of a mind map or a WBS drawn as a tile, which shows a diagram of
/// the project (`docs/adr/0018-diagrams-that-link.md`).
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    /// The key of the name it shows ([`crate::tree::Tiles::pick`]).
    pub key: String,
    /// What the box says.
    pub name: String,
    /// Zero-based line of its box; `None` for a diagram that is on no box.
    pub line: Option<usize>,
    /// The key of the nearest tile above it in the tree.
    pub parent: Option<String>,
    /// The whole tile.
    pub frame: Rect,
    /// The room in it for the picture.
    pub picture: Rect,
}

/// The line a transition takes, from its source to the tip of its head.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    /// Zero-based line of the transition.
    pub line: usize,
    pub points: Vec<(f32, f32)>,
}

/// A message's arrow, and the band of the diagram it takes up.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// Zero-based line of the message.
    pub line: usize,
    /// From the sender to the tip of the head.
    pub points: Vec<(f32, f32)>,
    pub top: f32,
    pub bottom: f32,
}

/// How a drawing's units become the finished picture's: moved past the
/// margin, then scaled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    pub dx: f32,
    pub dy: f32,
    pub sx: f32,
    pub sy: f32,
}

impl Default for Fit {
    fn default() -> Self {
        Self {
            dx: 0.0,
            dy: 0.0,
            sx: 1.0,
            sy: 1.0,
        }
    }
}

impl Fit {
    pub fn point(self, (x, y): (f32, f32)) -> (f32, f32) {
        ((x + self.dx) * self.sx, (y + self.dy) * self.sy)
    }

    /// The drawing's point that [`Fit::point`] put at `(x, y)`.
    pub fn back(self, (x, y): (f32, f32)) -> (f32, f32) {
        (x / self.sx - self.dx, y / self.sy - self.dy)
    }

    fn rect(self, r: Rect) -> Rect {
        let (x, y) = self.point((r.x, r.y));
        Rect {
            x,
            y,
            w: r.w * self.sx,
            h: r.h * self.sy,
        }
    }

    /// The drawing's rect that [`Fit::rect`] put at `r`.
    pub fn back_rect(self, r: Rect) -> Rect {
        let (x, y) = self.back((r.x, r.y));
        Rect {
            x,
            y,
            w: r.w / self.sx,
            h: r.h / self.sy,
        }
    }
}

impl Map {
    /// The map in the finished picture's units.
    pub fn placed(mut self, fit: Fit) -> Map {
        for spot in &mut self.states {
            spot.rect = fit.rect(spot.rect);
            spot.radius *= fit.sx;
        }
        for route in &mut self.transitions {
            for point in &mut route.points {
                *point = fit.point(*point);
            }
        }
        for step in &mut self.messages {
            for point in &mut step.points {
                *point = fit.point(*point);
            }
            step.top = fit.point((0.0, step.top)).1;
            step.bottom = fit.point((0.0, step.bottom)).1;
        }
        for wire in &mut self.wires {
            for point in wire.points.iter_mut().chain(&mut wire.path) {
                *point = fit.point(*point);
            }
            wire.from.rect = fit.rect(wire.from.rect);
            wire.to.rect = fit.rect(wire.to.rect);
        }
        for tile in &mut self.tiles {
            tile.frame = fit.rect(tile.frame);
            tile.picture = fit.rect(tile.picture);
        }
        self.fit = fit;
        self
    }

    /// Every line moved down by `lines`: for a diagram read from inside a
    /// longer text.
    pub fn shifted(mut self, lines: usize) -> Map {
        for spot in &mut self.states {
            spot.line += lines;
        }
        for route in &mut self.transitions {
            route.line += lines;
        }
        for step in &mut self.messages {
            step.line += lines;
        }
        for wire in &mut self.wires {
            wire.line = wire.line.map(|l| l + lines);
            wire.entry = wire.entry.map(|e| e + lines);
        }
        for (line, _) in &mut self.misses {
            *line += lines;
        }
        for tile in &mut self.tiles {
            tile.line = tile.line.map(|l| l + lines);
        }
        self
    }
}
