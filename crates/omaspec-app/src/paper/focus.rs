//! The focus on one box (docs/adr/0024): a veil over the diagram, as a
//! play lays one, and through it the box, the boxes its lines lead to or
//! come from, those lines and their words. The rest is still there, only
//! quieter. Shift and a click on a box focuses it, on paper ends it; in
//! the editor the focus follows the caret onto another box.

use iced::mouse::Cursor;
use iced::widget::canvas::{self, Frame, Geometry};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use omaspec_uml::graph::Rect;
use omaspec_uml::map::{Map, Shape, Spot};

use super::canvas::{Grid, fill_paper_band};
use super::play::{Glow, VEIL, Viewer, with_alpha};
use super::raster::Raster;
use super::view::{Glide, View};
use super::{Message, Paper};

/// How far a lit line's light reaches to each side, in screen pixels.
const LINE_LIGHT: f32 = 6.0;

/// What comes through the veil, in the diagram's units.
#[derive(Debug, Clone, PartialEq)]
pub struct Lit {
    /// The box in focus.
    pub focus: Spot,
    /// The boxes its lines lead to or come from.
    pub neighbors: Vec<Spot>,
    /// Those lines, as drawn.
    pub lines: Vec<Vec<(f32, f32)>>,
    /// Their words.
    pub words: Vec<Rect>,
}

/// The box `name` and what is around it in `map`; `None` when no box of
/// the picture has that name.
pub fn lit(map: &Map, name: &str) -> Option<Lit> {
    let place = map.places.iter().find(|p| p.name == name)?;
    let focus = spot_at(map, place.rect, name);
    let mut neighbors: Vec<Spot> = Vec::new();
    let mut lines = Vec::new();
    let mut words = Vec::new();
    for wire in &map.wires {
        let other = if wire.key.from == name {
            (&wire.key.to, wire.to.rect)
        } else if wire.key.to == name {
            (&wire.key.from, wire.from.rect)
        } else {
            continue;
        };
        if other.0 != name && !neighbors.iter().any(|n| same(n.rect, other.1)) {
            neighbors.push(spot_at(map, other.1, other.0));
        }
        lines.push(wire.points.clone());
        words.extend(wire.labels.iter().copied());
    }
    Some(Lit {
        focus,
        neighbors,
        lines,
        words,
    })
}

/// The state drawn at `rect`, with its shape and corners; a plain box when
/// the picture is of another kind.
fn spot_at(map: &Map, rect: Rect, name: &str) -> Spot {
    map.states
        .iter()
        .find(|s| same(s.rect, rect))
        .cloned()
        .unwrap_or_else(|| Spot {
            name: name.to_owned(),
            shape: Shape::State,
            line: 0,
            parent: None,
            rect,
            radius: 0.0,
        })
}

fn same(a: Rect, b: Rect) -> bool {
    let close = |p: f32, q: f32| (p - q).abs() < 0.5;
    close(a.x, b.x) && close(a.y, b.y) && close(a.w, b.w) && close(a.h, b.h)
}

/// The rectangles a line's light comes through on screen: runs of its
/// points that stay within a narrow band, each grown by [`LINE_LIGHT`].
pub fn strips(points: &[Point]) -> Vec<Rectangle> {
    const BAND: f32 = 14.0;
    let mut out = Vec::new();
    let mut start = 0;
    while start + 1 < points.len() {
        let mut end = start + 1;
        let mut bounds = span(&points[start..=end]);
        while end + 1 < points.len() {
            let wider = span(&points[start..=end + 1]);
            if wider.width.min(wider.height) > BAND {
                break;
            }
            end += 1;
            bounds = wider;
        }
        out.push(Rectangle::new(
            Point::new(bounds.x - LINE_LIGHT, bounds.y - LINE_LIGHT),
            Size::new(
                bounds.width + 2.0 * LINE_LIGHT,
                bounds.height + 2.0 * LINE_LIGHT,
            ),
        ));
        start = end;
    }
    out
}

fn span(points: &[Point]) -> Rectangle {
    let (mut x0, mut y0) = (f32::INFINITY, f32::INFINITY);
    let (mut x1, mut y1) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in points {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Rectangle::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0))
}

impl Paper {
    /// The box in focus, by name.
    pub fn focused(&self) -> Option<&str> {
        self.focus.as_deref()
    }

    /// Focuses the box `name`, or ends the focus.
    pub fn set_focus(&mut self, name: Option<String>) {
        self.focus = name;
    }

    /// Shift and a click at `at`, a point of the pane: the box there, the
    /// smallest around it, comes into focus; the one in focus, or paper,
    /// ends it.
    pub(super) fn focus_at(&mut self, at: Point) {
        let Some(diagram) = &self.diagram else {
            return;
        };
        let p = self.view.to_diagram(at, self.viewport);
        let hit = diagram
            .drawn
            .map
            .places
            .iter()
            .filter(|place| {
                let r = place.rect;
                r.x <= p.x && p.x <= r.x + r.w && r.y <= p.y && p.y <= r.y + r.h
            })
            .min_by(|a, b| (a.rect.w * a.rect.h).total_cmp(&(b.rect.w * b.rect.h)))
            .map(|place| place.name.clone());
        self.focus = match hit {
            Some(name) if self.focus.as_deref() != Some(name.as_str()) => Some(name),
            _ => None,
        };
    }

    /// What the focus lets through, while it is on a box of the picture
    /// and no play runs.
    pub(super) fn lit(&self) -> Option<Lit> {
        if self.play.is_some() {
            return None;
        }
        lit(&self.diagram.as_ref()?.drawn.map, self.focus.as_deref()?)
    }
}

/// The focus over the diagram, in two layers as a play's: the veil and
/// what comes through it (`over: false`), then the corners a clip lets
/// through veiled again and the light on the box in focus (`over: true`).
pub struct FocusLayer<'a> {
    pub lit: Lit,
    pub diagram: Size,
    pub overview: &'a Raster,
    pub detail: Option<&'a Raster>,
    pub view: View,
    pub glide: Option<Glide>,
    pub accent: Color,
    pub veil: Color,
    pub over: bool,
    pub scale: f32,
}

impl canvas::Program<Message> for FocusLayer<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &canvas::Event,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> Option<canvas::Action<Message>> {
        let canvas::Event::Window(iced::window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        self.glide
            .filter(|g| !g.done(*now))
            .map(|_| canvas::Action::request_redraw())
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let now = iced::time::Instant::now();
        let view = self.glide.map_or(self.view, |g| g.at(now));
        let place = |r: Rect| {
            view.on_screen(
                Rectangle::new(Point::new(r.x, r.y), Size::new(r.w, r.h)),
                local,
            )
        };
        let grid = Grid::of(bounds, self.scale);
        let paint = |frame: &mut Frame, clip: Rectangle, opacity: f32| {
            let pictures = [Some(self.overview), self.detail]
                .into_iter()
                .flatten()
                .map(|raster| (view.on_screen(raster.region, local), raster, opacity));
            grid.draw(frame, clip, 0.0, pictures);
        };
        let viewer = Viewer {
            paper: grid.snap(view.diagram_rect(self.diagram, local)),
            scale: grid.scale,
            place: &place,
            paint: &paint,
        };
        let boxes = std::iter::once(&self.lit.focus).chain(&self.lit.neighbors);
        if self.over {
            let veil = with_alpha(self.veil, VEIL);
            for spot in boxes {
                Glow::of(spot, &viewer).mask(&mut frame, veil);
            }
            Glow::of(&self.lit.focus, &viewer).draw(&mut frame, self.accent, 0.8);
        } else {
            let paper = viewer.paper;
            fill_paper_band(
                &mut frame,
                paper,
                viewer.scale,
                paper.y,
                paper.y + paper.height,
                with_alpha(self.veil, VEIL),
            );
            let through = boxes
                .map(|spot| Glow::of(spot, &viewer).bounds())
                .chain(self.lit.words.iter().map(|&w| place(w)))
                .chain(self.lit.lines.iter().flat_map(|line| {
                    let points: Vec<Point> = line
                        .iter()
                        .map(|&(x, y)| {
                            place(Rect {
                                x,
                                y,
                                w: 0.0,
                                h: 0.0,
                            })
                            .position()
                        })
                        .collect();
                    strips(&points)
                }));
            for clip in through {
                if let Some(clip) = clip.intersection(&paper) {
                    paint(&mut frame, clip, 1.0);
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_run_is_one_strip_and_a_corner_starts_another() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(0.0, 50.0),
            Point::new(0.0, 100.0),
            Point::new(80.0, 100.0),
        ];
        let found = strips(&points);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(
            found[0],
            Rectangle::new(Point::new(-6.0, -6.0), Size::new(12.0, 112.0))
        );
        assert_eq!(
            found[1],
            Rectangle::new(Point::new(-6.0, 94.0), Size::new(92.0, 12.0))
        );
    }

    #[test]
    fn the_focus_lights_a_state_its_neighbors_and_their_lines() {
        let source = "@startuml\nstate Busy\n[*] --> Idle\nIdle --> Busy : job\nBusy --> Idle : done\nBusy --> Off\nIdle --> Off\n@enduml\n";
        let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
        let lit = lit(&map, "Busy").unwrap();
        assert_eq!(lit.focus.name, "Busy");
        let mut names: Vec<&str> = lit.neighbors.iter().map(|s| s.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["Idle", "Off"]);
        assert_eq!(lit.lines.len(), 3);
        assert_eq!(lit.words.len(), 2);
        assert!(super::lit(&map, "Nowhere").is_none());
    }
}
