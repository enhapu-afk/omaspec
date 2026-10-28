//! Where the parts of a drawn diagram are, in the units of the finished
//! picture: the boxes of states, the routes of transitions, the arrows of
//! messages. A viewer lights them up while a specification runs.

use crate::graph::Rect;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Map {
    pub states: Vec<Spot>,
    pub transitions: Vec<Route>,
    pub messages: Vec<Step>,
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

impl Fit {
    pub fn point(self, (x, y): (f32, f32)) -> (f32, f32) {
        ((x + self.dx) * self.sx, (y + self.dy) * self.sy)
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
        self
    }
}
