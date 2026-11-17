//! The focus on one box (docs/adr/0024): a veil over the diagram, and
//! through it the box, the boxes its lines lead to or come from, those
//! lines and their words. The rest is still there, only quieter. Shift
//! and a click on a box focuses it, on paper ends it; in the editor the
//! focus follows the caret onto another box.
//!
//! The veil has the color of the picture's own paper, so the rest pales
//! into it, on a white card as on a dark one. A box comes through in its
//! own shape, which the picture says (the outline of its part,
//! docs/adr/0029), a diamond as a diamond and a box drawn by hand with
//! its bowed sides; where a picture marks no parts, in the shape the map
//! knows. Its rim and the lines are drawn again over the veil, so nothing
//! is cut out as a rectangle.

use iced::mouse::Cursor;
use iced::widget::canvas::{self, Frame, Geometry, LineDash, Path, Stroke, path};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use omaspec_render::export::usvg;
use omaspec_uml::graph::Rect;
use omaspec_uml::map::{Head, HeadFill, Map, Shape, Spot};

use super::canvas::{Grid, fill_paper_holed};
use super::play::{Glow, Viewer, with_alpha};
use super::raster::Raster;
use super::view::{Glide, View};
use super::{Message, Paper};

/// How much of the paper's color lies over what is not in focus.
const VEIL: f32 = 0.76;
/// How far around a line's words the veil stays away, in screen pixels.
const WORDS_AIR: f32 = 2.0;

/// A line in focus, as the picture draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct LitLine {
    /// Its route as the picture draws it, curves and all, where the
    /// picture has it ([`route`]); else it is drawn through `points`.
    route: Option<Vec<Segment>>,
    pub points: Vec<(f32, f32)>,
    pub color: Color,
    pub width: f32,
    pub dash: Option<(f32, f32)>,
    pub heads: Vec<Head>,
}

/// The outline of a box as its picture draws it, in the diagram's units,
/// and the rim along it.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    segments: Vec<Segment>,
    /// The rim's color and width.
    rim: Option<(Color, f32)>,
    /// The box around it.
    bounds: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Segment {
    Move((f32, f32)),
    Line((f32, f32)),
    Quad((f32, f32), (f32, f32)),
    Cubic((f32, f32), (f32, f32), (f32, f32)),
    Close,
}

impl Outline {
    /// The outline of the part `part` in `tree`: the largest shape it
    /// fills, where the picture marks its parts.
    pub fn of(tree: &usvg::Tree, part: &str) -> Option<Outline> {
        let prefix = format!("{}-", omaspec_uml::part_id(part));
        let mut best: Option<(f32, &usvg::Path)> = None;
        each_path(tree.root(), &prefix, false, &mut |path| {
            let b = path.abs_bounding_box();
            let area = b.width() * b.height();
            if path.fill().is_some() && best.is_none_or(|(most, _)| area > most) {
                best = Some((area, path));
            }
        });
        let (_, path) = best?;
        let segments = segments_of(path);
        let rim = path.stroke().and_then(|stroke| match stroke.paint() {
            usvg::Paint::Color(c) => Some((
                Color {
                    a: stroke.opacity().get(),
                    ..Color::from_rgb8(c.red, c.green, c.blue)
                },
                stroke.width().get(),
            )),
            _ => None,
        });
        let b = path.abs_bounding_box();
        Some(Outline {
            segments,
            rim,
            bounds: Rect {
                x: b.x(),
                y: b.y(),
                w: b.width(),
                h: b.height(),
            },
        })
    }

    /// The outline on screen, as one more shape of `builder`.
    fn trace(&self, builder: &mut path::Builder, place: &dyn Fn((f32, f32)) -> Point) {
        trace(&self.segments, builder, place);
    }
}

/// The line of the picture `tree` that runs along `points`, as it is
/// drawn there: the stroke among its lines that starts where they start
/// and ends nearest to where they end, within a mark's length.
fn route(tree: &usvg::Tree, points: &[(f32, f32)]) -> Option<Vec<Segment>> {
    /// How far before the tip of its mark a line may end.
    const MARK: f32 = 16.0;
    let (first, last) = (*points.first()?, *points.last()?);
    let far = |a: (f32, f32), b: (f32, f32)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let mut best: Option<(f32, Vec<Segment>)> = None;
    each_path(tree.root(), omaspec_uml::LINES, false, &mut |path| {
        if path.fill().is_some() || path.stroke().is_none() {
            return;
        }
        let segments = segments_of(path);
        let ends = |segment: &Segment| match *segment {
            Segment::Move(p) | Segment::Line(p) | Segment::Quad(_, p) | Segment::Cubic(_, _, p) => {
                Some(p)
            }
            Segment::Close => None,
        };
        let (Some(start), Some(end)) = (
            segments.first().and_then(ends),
            segments.iter().rev().find_map(ends),
        ) else {
            return;
        };
        let off = far(end, last);
        if far(start, first) <= 1.0
            && off <= MARK
            && best.as_ref().is_none_or(|(least, _)| off < *least)
        {
            best = Some((off, segments));
        }
    });
    best.map(|(_, segments)| segments)
}

/// The segments of `path` in the diagram's units.
fn segments_of(path: &usvg::Path) -> Vec<Segment> {
    let t = path.abs_transform();
    let at = |p: usvg::tiny_skia_path::Point| {
        (
            t.sx * p.x + t.kx * p.y + t.tx,
            t.ky * p.x + t.sy * p.y + t.ty,
        )
    };
    use usvg::tiny_skia_path::PathSegment as S;
    path.data()
        .segments()
        .map(|segment| match segment {
            S::MoveTo(p) => Segment::Move(at(p)),
            S::LineTo(p) => Segment::Line(at(p)),
            S::QuadTo(c, p) => Segment::Quad(at(c), at(p)),
            S::CubicTo(a, b, p) => Segment::Cubic(at(a), at(b), at(p)),
            S::Close => Segment::Close,
        })
        .collect()
}

/// Traces `segments` on screen as one more shape of `builder`.
fn trace(segments: &[Segment], builder: &mut path::Builder, place: &dyn Fn((f32, f32)) -> Point) {
    for segment in segments {
        match *segment {
            Segment::Move(p) => builder.move_to(place(p)),
            Segment::Line(p) => builder.line_to(place(p)),
            Segment::Quad(c, p) => builder.quadratic_curve_to(place(c), place(p)),
            Segment::Cubic(a, b, p) => builder.bezier_curve_to(place(a), place(b), place(p)),
            Segment::Close => builder.close(),
        }
    }
}

/// Hands each path of the groups of `node` whose id starts with `prefix`
/// to `take`, words left out; `inside` once such a group is entered.
fn each_path<'a>(
    node: &'a usvg::Group,
    prefix: &str,
    inside: bool,
    take: &mut dyn FnMut(&'a usvg::Path),
) {
    for child in node.children() {
        match child {
            usvg::Node::Group(group) => {
                each_path(
                    group,
                    prefix,
                    inside || group.id().starts_with(prefix),
                    take,
                );
            }
            usvg::Node::Path(path) if inside => take(path),
            _ => {}
        }
    }
}

/// What comes through the veil, in the diagram's units.
#[derive(Debug, Clone, PartialEq)]
pub struct Lit {
    /// The box in focus.
    pub focus: Spot,
    /// The boxes its lines lead to or come from.
    pub neighbors: Vec<Spot>,
    /// Those lines, as drawn.
    pub lines: Vec<LitLine>,
    /// Their words.
    pub words: Vec<Rect>,
    /// The parts the box in focus and each of the others is drawn in
    /// (docs/adr/0029), in that order.
    pub parts: Vec<String>,
    /// Their outlines as the picture draws them, in that order, where it
    /// marks its parts ([`Paper::lit`] fills them in).
    pub outlines: Vec<Option<Outline>>,
}

/// The box `name` and what is around it in `map`; `None` when no box of
/// the picture has that name.
pub fn lit(map: &Map, name: &str) -> Option<Lit> {
    let place = map.places.iter().find(|p| p.name == name)?;
    let focus = spot_at(map, place.rect, name);
    let part_at = |rect: Rect, name: &str| {
        map.places
            .iter()
            .find(|p| same(p.rect, rect))
            .map_or_else(|| name.to_owned(), |p| super::morph::map_part(map, p))
    };
    let mut parts = vec![part_at(place.rect, name)];
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
            parts.push(part_at(other.1, other.0));
        }
        let omaspec_uml::Color(r, g, b) = wire.stroke.color;
        lines.push(LitLine {
            route: None,
            points: wire.points.clone(),
            color: Color::from_rgb8(r, g, b),
            width: wire.stroke.width,
            dash: wire.stroke.dash,
            heads: wire.heads.clone(),
        });
        words.extend(wire.labels.iter().copied());
    }
    Some(Lit {
        focus,
        neighbors,
        lines,
        words,
        parts,
        outlines: Vec::new(),
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
            part: name.to_owned(),
        })
}

fn same(a: Rect, b: Rect) -> bool {
    let close = |p: f32, q: f32| (p - q).abs() < 0.5;
    close(a.x, b.x) && close(a.y, b.y) && close(a.w, b.w) && close(a.h, b.h)
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
    pub(crate) fn lit(&self) -> Option<Lit> {
        if self.play.is_some() {
            return None;
        }
        let diagram = self.diagram.as_ref()?;
        let mut lit = lit(&diagram.drawn.map, self.focus.as_deref()?)?;
        lit.outlines = lit
            .parts
            .iter()
            .map(|part| Outline::of(&diagram.drawn.tree, part))
            .collect();
        for line in &mut lit.lines {
            line.route = route(&diagram.drawn.tree, &line.points);
        }
        Some(lit)
    }
}

/// The focus over the diagram, in two layers: the veil with its holes,
/// the rims and the lines (`over: false`), then the light on the box in
/// focus (`over: true`).
pub struct FocusLayer<'a> {
    pub lit: Lit,
    pub diagram: Size,
    pub overview: &'a Raster,
    pub view: View,
    pub glide: Option<Glide>,
    pub accent: Color,
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
        let now = crate::clock::now();
        let view = self.glide.map_or(self.view, |g| g.at(now));
        let place = |r: Rect| {
            view.on_screen(
                Rectangle::new(Point::new(r.x, r.y), Size::new(r.w, r.h)),
                local,
            )
        };
        let grid = Grid::of(bounds, self.scale);
        let paper = grid.snap(view.diagram_rect(self.diagram, local));
        let point = |(x, y): (f32, f32)| {
            place(Rect {
                x,
                y,
                w: 0.0,
                h: 0.0,
            })
            .position()
        };
        // How many pixels a unit of the diagram is.
        let unit = place(Rect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 0.0,
        })
        .width;
        // The shape each box comes through in: its picture's outline, else
        // the one the map knows.
        let viewer = Viewer {
            paper,
            scale: grid.scale,
            place: &place,
            paint: &|_, _, _| {},
        };
        let boxes: Vec<(&Spot, Option<&Outline>)> = std::iter::once(&self.lit.focus)
            .chain(&self.lit.neighbors)
            .enumerate()
            .map(|(at, spot)| (spot, self.lit.outlines.get(at).and_then(Option::as_ref)))
            .collect();
        let shape =
            |builder: &mut path::Builder, spot: &Spot, outline: Option<&Outline>| match outline {
                Some(outline) => outline.trace(builder, &point),
                None => Glow::of(spot, &viewer).trace(builder),
            };
        if self.over {
            // The light on the box in focus, along its own shape.
            let (spot, outline) = boxes[0];
            let around = Path::new(|builder| shape(builder, spot, outline));
            const LAYERS: usize = 6;
            for layer in 0..LAYERS {
                let k = 1.0 - layer as f32 / LAYERS as f32;
                frame.stroke(
                    &around,
                    Stroke::default()
                        .with_color(with_alpha(self.accent, 0.2 * k.powf(1.6)))
                        .with_width(4.0 + layer as f32 * 3.2),
                );
            }
            frame.stroke(
                &around,
                Stroke::default()
                    .with_color(with_alpha(self.accent, 0.95))
                    .with_width(2.2),
            );
        } else {
            let ground = Color {
                a: 1.0,
                ..self.overview.paper
            };
            // A box inside another that comes through is in its hole
            // already: a hole in a hole would be veiled again.
            let bounds_of =
                |spot: &Spot, outline: Option<&Outline>| outline.map_or(spot.rect, |o| o.bounds);
            let within = |inner: Rect, outer: Rect| {
                inner.x >= outer.x - 0.5
                    && inner.y >= outer.y - 0.5
                    && inner.x + inner.w <= outer.x + outer.w + 0.5
                    && inner.y + inner.h <= outer.y + outer.h + 0.5
            };
            let holes: Vec<(&Spot, Option<&Outline>)> = boxes
                .iter()
                .enumerate()
                .filter(|(at, (spot, outline))| {
                    let own = bounds_of(spot, *outline);
                    !boxes.iter().enumerate().any(|(other, (s, o))| {
                        let around = bounds_of(s, *o);
                        other != *at
                            && within(own, around)
                            && (around.w * around.h > own.w * own.h || other < *at)
                    })
                })
                .map(|(_, held)| *held)
                .collect();
            let words: Vec<Rectangle> = self
                .lit
                .words
                .iter()
                .filter(|word| !holes.iter().any(|(s, o)| within(**word, bounds_of(s, *o))))
                .map(|&word| {
                    let at = place(word);
                    Rectangle::new(
                        Point::new(at.x - WORDS_AIR, at.y - WORDS_AIR),
                        Size::new(at.width + 2.0 * WORDS_AIR, at.height + 2.0 * WORDS_AIR),
                    )
                })
                .collect();
            fill_paper_holed(
                &mut frame,
                paper,
                grid.scale,
                with_alpha(ground, VEIL),
                |builder| {
                    for (spot, outline) in &holes {
                        shape(builder, spot, *outline);
                    }
                    for word in &words {
                        builder.rectangle(word.position(), word.size());
                    }
                },
            );
            // The rims, half of which the veil took, and the lines.
            for (_, outline) in &holes {
                if let Some(outline) = outline
                    && let Some((color, width)) = outline.rim
                {
                    frame.stroke(
                        &Path::new(|builder| outline.trace(builder, &point)),
                        Stroke::default()
                            .with_color(color)
                            .with_width((width * unit).max(0.75)),
                    );
                }
            }
            for line in &self.lit.lines {
                let width = (line.width * unit).max(0.75);
                let segments;
                let mut stroke = Stroke::default().with_color(line.color).with_width(width);
                if let Some((on, off)) = line.dash {
                    segments = [on * unit, off * unit];
                    stroke = Stroke {
                        line_dash: LineDash {
                            segments: &segments,
                            offset: 0,
                        },
                        ..stroke
                    };
                }
                let route = Path::new(|builder| match &line.route {
                    Some(segments) => trace(segments, builder, &point),
                    None => {
                        for (at, &p) in line.points.iter().enumerate() {
                            if at == 0 {
                                builder.move_to(point(p));
                            } else {
                                builder.line_to(point(p));
                            }
                        }
                    }
                });
                frame.stroke(&route, stroke);
                for head in &line.heads {
                    let mark = Path::new(|builder| {
                        for (at, &p) in head.points.iter().enumerate() {
                            if at == 0 {
                                builder.move_to(point(p));
                            } else {
                                builder.line_to(point(p));
                            }
                        }
                        if head.closed {
                            builder.close();
                        }
                    });
                    match head.fill {
                        HeadFill::Ink => frame.fill(&mark, line.color),
                        HeadFill::Paper => frame.fill(&mark, ground),
                        HeadFill::None => {}
                    }
                    frame.stroke(
                        &mark,
                        Stroke::default().with_color(line.color).with_width(width),
                    );
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A box comes through the veil in the shape its picture draws: a
    /// diamond with its four corners, a line along its own curve. A
    /// picture that marks no parts gives none, and the map's shape holds.
    #[test]
    fn a_box_comes_through_in_its_own_shape() {
        use crate::paper::testing::{drawn, drawn_without_parts};
        use omaspec_core::Language;
        let source = "flowchart TD\n  Ask{Ask?} -->|yes| Do[Do it]\n";
        let picture = drawn(source, Language::Mermaid);
        let lit = lit(&picture.map, "Ask").unwrap();
        assert_eq!(lit.parts.len(), 2);
        let diamond = Outline::of(&picture.tree, &lit.parts[0]).expect("the diamond's part");
        let corners = diamond
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::Move(_) | Segment::Line(_)))
            .count();
        assert_eq!(corners, 4, "{:?}", diamond.segments);
        assert!(diamond.rim.is_some());
        let spot = lit.focus.rect;
        assert!(
            (diamond.bounds.w - spot.w).abs() < 2.0 && (diamond.bounds.h - spot.h).abs() < 2.0,
            "{:?} {spot:?}",
            diamond.bounds
        );
        assert!(Outline::of(&picture.tree, &lit.parts[1]).is_some());
        assert!(route(&picture.tree, &lit.lines[0].points).is_some());
        let bare = drawn_without_parts(source, Language::Mermaid);
        assert!(Outline::of(&bare.tree, &lit.parts[0]).is_none());
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
