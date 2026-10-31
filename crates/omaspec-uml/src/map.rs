//! Where the parts of a drawn diagram are, in the units of the finished
//! picture: the boxes of states, the routes of transitions, the arrows of
//! messages. A viewer lights them up while a specification runs. It also
//! names every line a writer can draw by hand ([`Wire`]), and says which
//! entries of the block of lines by hand did not hold.

use crate::graph::Rect;
use crate::hand::{Key, Via};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Map {
    pub states: Vec<Spot>,
    pub transitions: Vec<Route>,
    pub messages: Vec<Step>,
    pub wires: Vec<Wire>,
    /// Entries of the block of lines by hand that did not hold: the
    /// zero-based line of each, and why.
    pub misses: Vec<(usize, String)>,
    /// How the drawing's units became the picture's.
    pub fit: Fit,
}

/// A line of a graph diagram, which the writer may draw by hand
/// ([`crate::hand`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Wire {
    /// What the block of lines by hand calls it.
    pub key: Key,
    /// Zero-based line of what draws it, where the drawer knows it.
    pub line: Option<usize>,
    /// As drawn, from the box it leaves to the tip of its head.
    pub points: Vec<(f32, f32)>,
    /// The middles of the boxes it leaves and enters, which its bends lie
    /// relative to.
    pub from: (f32, f32),
    pub to: (f32, f32),
    /// Its bends as its entry writes them, and the entry's zero-based
    /// line; none when the layout drew it.
    pub vias: Vec<Via>,
    pub entry: Option<usize>,
}

impl Wire {
    /// Where its bends lie in the picture, which `fit` made of the drawing.
    pub fn bends(&self, fit: Fit) -> Vec<(f32, f32)> {
        let (from, to) = (fit.back(self.from), fit.back(self.to));
        self.vias
            .iter()
            .map(|v| fit.point(v.at(from, to)))
            .collect()
    }

    /// The bend that lies at `point` of the picture, which `fit` made of
    /// the drawing.
    pub fn via_at(&self, point: (f32, f32), fit: Fit) -> Via {
        Via::of(fit.back(point), fit.back(self.from), fit.back(self.to))
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
            for point in &mut wire.points {
                *point = fit.point(*point);
            }
            wire.from = fit.point(wire.from);
            wire.to = fit.point(wire.to);
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
        self
    }
}
