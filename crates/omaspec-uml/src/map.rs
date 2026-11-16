//! Where the parts of a drawn diagram are, in the units of the finished
//! picture: the boxes of states, the routes of transitions, the arrows of
//! messages. A viewer lights them up while a specification runs. It also
//! names every line a writer can shape by hand ([`Wire`]), and says which
//! entries of the block of lines by hand did not hold.

use crate::graph::Rect;
use crate::hand::{self, Corner, End, Key};
use crate::svg::Stroke;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Map {
    pub states: Vec<Spot>,
    pub transitions: Vec<Route>,
    pub messages: Vec<Step>,
    pub wires: Vec<Wire>,
    /// The boxes a writer may hold in place ([`Place`]).
    pub places: Vec<Place>,
    /// The boxes of a map drawn as tiles that show other diagrams.
    pub tiles: Vec<Tile>,
    /// The lines of a table's cells that say a line of the source
    /// ([`crate::table`]).
    pub cells: Vec<Cell>,
    /// Entries of the block of lines by hand that did not hold: the
    /// zero-based line of each, and why.
    pub misses: Vec<(usize, String)>,
    /// Areas filled with a color of their own under what is drawn on
    /// them: a sequence diagram's boxes.
    pub grounds: Vec<Ground>,
    /// Bands a pane keeps in sight as it scrolls past them (REQ-073): a
    /// sequence diagram's participants across its top, a Gantt chart's
    /// tasks down its left. What is drawn is the picture's; the pane only
    /// holds them at its edge.
    pub bands: Vec<Band>,
    /// Parts of their own that lie inside a box, as a note inside a state:
    /// each part's name and that of the box's part. A glide draws such a
    /// part over its box, not under it.
    pub inside: Vec<(String, String)>,
    /// How the drawing's units became the picture's.
    pub fit: Fit,
}

/// A band of the picture a pane keeps in sight ([`Map::bands`]): where it
/// lies, and the edge of the pane it stays at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub rect: Rect,
    pub edge: Edge,
}

/// An edge of the pane a [`Band`] stays at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    /// It scrolls up out of sight: the names over what lies under them.
    Top,
    /// It scrolls out to the left: the names beside what lies right of them.
    Left,
}

/// An area a diagram fills with one color under what lies on it, as a
/// sequence diagram's box under its participants and messages. A play's
/// veil dims what lies on it toward that color rather than the paper's, so
/// the area stays as even as the paper.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ground {
    pub rect: Rect,
    pub fill: crate::color::Color,
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
    /// Where its words are: its label, and in a class diagram the
    /// multiplicities at its ends.
    pub labels: Vec<Rect>,
    /// How it is drawn: its color, width and dash.
    pub stroke: Stroke,
    /// The marks at its ends, as drawn.
    pub heads: Vec<Head>,
    /// The layout alone lays it: a link of a mind map or a WBS, which a
    /// pane glides with its boxes and no writer shapes by hand.
    pub fixed: bool,
    /// The parts its words are drawn in (docs/adr/0029), where its drawer
    /// marks them: [`WORDS`] in its middle, [`FROM`] beside its start and
    /// [`TO`] beside its end, a multiplicity's.
    pub words: [Option<String>; 3],
}

/// Which of a line's [`Wire::words`] are in its middle.
pub const WORDS: usize = 0;
/// Which are beside its start.
pub const FROM: usize = 1;
/// Which are beside its end.
pub const TO: usize = 2;

/// A mark at the end of a line: an arrowhead, a triangle, a diamond, a
/// dot, drawn as a polygon of the picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    pub points: Vec<(f32, f32)>,
    /// Whether the last point joins the first.
    pub closed: bool,
    pub fill: HeadFill,
}

/// What a mark is filled with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadFill {
    /// The color of its line.
    Ink,
    /// The paper's color: an empty diamond hides the line under it.
    Paper,
    /// Nothing: an outline.
    None,
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

/// A box of a graph diagram, which the writer may hold beside another
/// ([0021](../../../docs/adr/0021-boxes-held-in-place.md)).
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    /// What the block of lines by hand calls it.
    pub name: String,
    pub rect: Rect,
    /// The container it keeps to, by the indices of its members from the
    /// top: only boxes of the same one stand in a rank with it.
    pub within: Vec<usize>,
    /// The ranks of that container run across the page, left to right,
    /// rather than down it.
    pub across: bool,
    /// The zero-based line of the entry that holds it; none when the
    /// layout places it.
    pub entry: Option<usize>,
    /// It holds others: a composite state, a package, a subgraph, which
    /// the pane may fold to its box
    /// ([0024](../../../docs/adr/0024-focus-and-collapse.md)).
    pub frame: bool,
    /// The layout alone says where it stands: a box of a mind map or a
    /// WBS, which folds and takes the focus and is held beside no other.
    pub fixed: bool,
    /// The part it is drawn in (docs/adr/0029), where its drawer marks its
    /// parts and the name is not enough: two boxes may share a name.
    pub part: Option<String>,
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
    /// The part it is drawn in (docs/adr/0029): a state's key, a mark's
    /// place in the diagram's tree.
    pub part: String,
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
    /// The key of what it shows: a diagram's file or a folder, as the
    /// map's maker gave it ([`crate::tree::Branch`]).
    pub key: String,
    /// What the tile is called.
    pub name: String,
    /// The whole tile.
    pub frame: Rect,
    /// The room in it for the picture.
    pub picture: Rect,
}

/// A line of a table's cell, and the line of the source it says.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub rect: Rect,
    /// Zero-based.
    pub line: usize,
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
        for cell in &mut self.cells {
            cell.rect = fit.rect(cell.rect);
        }
        for ground in &mut self.grounds {
            ground.rect = fit.rect(ground.rect);
        }
        for band in &mut self.bands {
            band.rect = fit.rect(band.rect);
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
            for rect in &mut wire.labels {
                *rect = fit.rect(*rect);
            }
            for point in wire.heads.iter_mut().flat_map(|head| &mut head.points) {
                *point = fit.point(*point);
            }
            wire.stroke.width *= fit.sx;
            wire.stroke.dash = wire
                .stroke
                .dash
                .map(|(on, off)| (on * fit.sx, off * fit.sx));
        }
        for place in &mut self.places {
            place.rect = fit.rect(place.rect);
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
            spot.line = spot.line.saturating_add(lines);
        }
        for route in &mut self.transitions {
            route.line = route.line.saturating_add(lines);
        }
        for step in &mut self.messages {
            step.line = step.line.saturating_add(lines);
        }
        for cell in &mut self.cells {
            cell.line = cell.line.saturating_add(lines);
        }
        for wire in &mut self.wires {
            wire.line = wire.line.map(|l| l.saturating_add(lines));
            wire.entry = wire.entry.map(|e| e.saturating_add(lines));
        }
        for place in &mut self.places {
            place.entry = place.entry.map(|e| e.saturating_add(lines));
        }
        for (line, _) in &mut self.misses {
            *line = line.saturating_add(lines);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_no_source_has_stays_without_one_when_shifted() {
        // A transition drawn only to show it went (a diff) has no line.
        let map = Map {
            transitions: vec![
                Route {
                    line: usize::MAX,
                    points: Vec::new(),
                },
                Route {
                    line: 3,
                    points: Vec::new(),
                },
            ],
            ..Map::default()
        }
        .shifted(5);
        assert_eq!(map.transitions[0].line, usize::MAX);
        assert_eq!(map.transitions[1].line, 8);
    }

    /// REQ-073: a sequence diagram's participants, a swimlane's names and a
    /// Gantt chart's axis and tasks are bands a pane keeps in sight; the
    /// picture is the same with them or without.
    #[test]
    fn long_diagrams_name_the_bands_a_pane_keeps_in_sight() {
        let bands = |source: &str| crate::render_mapped(source, None).unwrap().1.bands;
        let sequence = bands("@startuml\nAlice -> Bob : hello\nBob -> Carol : hi\n@enduml\n");
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].edge, Edge::Top);
        assert!(
            sequence[0].rect.h > 10.0 && sequence[0].rect.w > 100.0,
            "{sequence:?}"
        );
        let lanes = bands("@startuml\n|Shop|\nstart\n:order;\n|Store|\n:pack;\nstop\n@enduml\n");
        assert_eq!(
            lanes.iter().map(|b| b.edge).collect::<Vec<_>>(),
            [Edge::Top]
        );
        let gantt = bands("@startgantt\n[Design] lasts 3 days\n[Build] lasts 5 days\n@endgantt\n");
        assert_eq!(
            gantt.iter().map(|b| b.edge).collect::<Vec<_>>(),
            [Edge::Top, Edge::Left]
        );
        // The names' band lies left of the axis', under it.
        assert!(
            gantt[1].rect.x + gantt[1].rect.w <= gantt[0].rect.x + 1.0,
            "{gantt:?}"
        );
        assert!(
            gantt[1].rect.y >= gantt[0].rect.y + gantt[0].rect.h - 1.0,
            "{gantt:?}"
        );
        // Without lanes, an activity diagram has none.
        assert!(bands("@startuml\nstart\n:a;\nstop\n@enduml\n").is_empty());
    }
}
