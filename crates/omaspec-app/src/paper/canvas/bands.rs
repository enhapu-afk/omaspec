//! The headings of a long diagram kept in sight (REQ-073). A band of the
//! picture its drawer names ([`omaspec_uml::map::Map::bands`]) that scrolls
//! up past the pane's top, or out past its left edge, stays there while
//! what it heads is in sight, and goes with that: a sequence diagram's
//! participants over its lifelines, a swimlane's name over its lane, a
//! Gantt chart's dates over its rows and its tasks beside their bars.
//!
//! Only the pane holds them: they are pixels of the picture on screen,
//! drawn again at the edge, and the picture and every export are as drawn.
//! Like a sheet's frozen rows they lie over all that marks the picture, a
//! play's light and veil too, and a point in one is the heading's own: a
//! click there picks the participant, not the message under it.

use iced::mouse::Cursor;
use iced::widget::canvas::{self, Frame, Geometry};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector};
use omaspec_uml::map::{Band, Edge, Ground, Heading};

use super::super::Message;
use super::super::raster::Raster;
use super::super::sprite;
use super::super::view::{Glide, View};

/// How far the paper under a band held at the top goes on, thinning out,
/// or beside one held at the left, in pixels: what scrolls under the band
/// fades into the paper there, where a hard edge cut it off.
const FADE: f32 = 30.0;
/// The most of the pane a held band may take, across it: a heading larger
/// than that, zoomed in, scrolls away with what it heads.
const MOST: f32 = 1.0 / 3.0;

/// From the pane's top to the pills a sequence diagram's participants
/// turn into as they scroll past it.
const PILL_TOP: f32 = 6.0;
/// The most a line of a pill's name is high, in pixels.
const PILL_LINE: f32 = 17.0;
/// A pill's room beside its name and over it, to the height of a line.
const PILL_PAD: (f32, f32) = (0.6, 0.22);
/// How far the paper round a pill thins out, in pixels: what scrolls by
/// goes under it softly, on every side.
const HALO: f32 = 7.0;
/// The layers that halo is drawn in, each as thin as the others.
const HALO_LAYERS: u8 = 7;

/// A band held at its edge of the pane.
#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub edge: Edge,
    /// The band, in the diagram's units.
    pub src: Rectangle,
    /// Where it is drawn, in the pane.
    pub dest: Rectangle,
    /// The paper behind it, as wide or as high as the paper is in sight.
    pub strip: Rectangle,
    /// From a point drawn in the band to where that point of the picture
    /// lies, scrolled out of sight.
    pub back: Vector,
    /// The band's heads that have scrolled past the pane's top, where they
    /// alone are held ([`omaspec_uml::map::Map::heads`]): the band is not
    /// drawn whole then, and what scrolls by is seen between them.
    pub heads: Vec<Pill>,
    /// Whether the heads alone are held, also while none has gone yet.
    pub apart: bool,
}

/// A participant on its way from its place in the picture to a pill at
/// the pane's top, as far as it has scrolled past it: the way hangs on
/// the scrolling alone, so it glides as the diagram does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pill {
    pub head: Heading,
    /// How far it is: from 0, its own shape at the top, to 1, the pill.
    pub gone: f32,
    /// Where it is drawn, in the pane.
    pub shape: Rectangle,
    /// Where its name is drawn.
    pub name: Rectangle,
    /// Where it lies in the picture, scrolled out of sight, and its name
    /// there: a click on the pill is a click on that name.
    pub on: Rectangle,
    pub name_on: Rectangle,
}

impl Pill {
    /// The pill of `head`, now `on` screen with its name at `name`, held
    /// under `top`; none while it has not reached it.
    fn of(head: Heading, on: Rectangle, name: Rectangle, zoom: f32, top: f32) -> Option<Pill> {
        if on.y >= top || on.height <= 0.0 {
            return None;
        }
        let k = ((top - on.y) / on.height).clamp(0.0, 1.0);
        let gone = k * k * (3.0 - 2.0 * k);
        // The name no larger than it is drawn, and no line of it higher
        // than a pill's.
        let line = (head.line * zoom).max(1e-3);
        let small = (PILL_LINE / line).min(1.0);
        let line = line * small;
        let size = Size::new(name.width * small, name.height * small);
        let pill = Rectangle::new(
            Point::new(on.center_x() - size.width / 2.0 - line * PILL_PAD.0, top),
            size + Size::new(line * PILL_PAD.0 * 2.0, line * PILL_PAD.1 * 2.0),
        );
        let lerp = |a: f32, b: f32| a + (b - a) * gone;
        let between = |a: Rectangle, b: Rectangle| {
            Rectangle::new(
                Point::new(lerp(a.x, b.x), lerp(a.y, b.y)),
                Size::new(lerp(a.width, b.width), lerp(a.height, b.height)),
            )
        };
        // Its own shape, kept at the top, and its name in it.
        let down = Vector::new(0.0, top - on.y);
        let own = Rectangle::new(on.position() + down, on.size());
        let name_own = Rectangle::new(name.position() + down, name.size());
        let name_pill = Rectangle::new(
            Point::new(
                pill.center_x() - size.width / 2.0,
                pill.center_y() - size.height / 2.0,
            ),
            size,
        );
        Some(Pill {
            head,
            gone,
            shape: between(own, pill),
            name: between(name_own, name_pill),
            on,
            name_on: name,
        })
    }
}

impl Held {
    /// Where the point `at` of the pane lies in the picture, if it is the
    /// band's own: in the band, or in a pill where the heads alone are
    /// held, which is the middle of its participant's name.
    fn own(&self, at: Point) -> Option<Point> {
        if self.apart {
            self.heads
                .iter()
                .find(|pill| pill.shape.contains(at))
                .map(|pill| pill.name_on.center())
        } else {
            self.strip.contains(at).then(|| at + self.back)
        }
    }
}

/// The bands of a `diagram` this large, seen in `view` in the pane
/// `local`, that are held at their edge: across the top first, then down
/// the left over them, as a sheet holds its headings.
///
/// `heads` are the parts of the first band across the top that alone are
/// held, if it is not held whole.
pub fn held(
    bands: &[Band],
    heads: &[Heading],
    diagram: Size,
    view: View,
    local: Rectangle,
) -> Vec<Held> {
    let paper = view.diagram_rect(diagram, local);
    let mut out = Vec::new();
    for edge in [Edge::Top, Edge::Left] {
        for (nth, band) in bands.iter().filter(|band| band.edge == edge).enumerate() {
            let src = rectangle(band.rect);
            let on = view.on_screen(src, local);
            let over = view.on_screen(rectangle(band.over), local);
            let thick = match edge {
                Edge::Top => on.height / local.height.max(1.0),
                Edge::Left => on.width / local.width.max(1.0),
            };
            // Pills are small however far the view is zoomed in.
            let apart = edge == Edge::Top && nth == 0 && !heads.is_empty();
            if thick > MOST && !apart {
                continue;
            }
            // A pill forms as its head reaches the line under the top it
            // will stand on, not the pane's edge some pixels on.
            let reach = if apart { PILL_TOP } else { 0.0 };
            let reaching = Rectangle::new(on.position() - Vector::new(0.0, reach), on.size());
            let Some(dest) = held_at(reaching, edge, over, local) else {
                continue;
            };
            let back = on.position() - dest.position();
            // Pushed up by the end of what the band heads, as it is.
            let top = local.y + PILL_TOP + (dest.y - local.y).min(0.0);
            let heads = if apart {
                heads
                    .iter()
                    .filter_map(|head| {
                        let on = view.on_screen(rectangle(head.rect), local);
                        let name = view.on_screen(rectangle(head.label), local);
                        Pill::of(*head, on, name, view.zoom, top)
                    })
                    .collect()
            } else {
                Vec::new()
            };
            out.push(Held {
                edge,
                src,
                dest,
                strip: strip(dest, edge, paper, local),
                back,
                heads,
                apart,
            });
        }
    }
    out
}

/// Where the point `at` of the pane lies in the picture: in a held band,
/// the band's own point, scrolled out of sight; elsewhere, `at`. A band
/// down the left lies over one across the top.
pub fn through(held: &[Held], at: Point) -> Point {
    held.iter()
        .rev()
        .find_map(|band| band.own(at))
        .unwrap_or(at)
}

/// The headings of the picture on screen, and how it is seen: what
/// [`KeptBands`] draws and the picture's clicks go through.
#[derive(Debug, Clone, Copy)]
pub struct Headings<'a> {
    pub bands: &'a [Band],
    /// The heads of the first band across the top that alone are held.
    pub heads: &'a [Heading],
    /// The areas the picture fills with a color of their own under what
    /// lies on them: what scrolls under a band there fades into that
    /// color, not the paper's.
    pub grounds: &'a [Ground],
    /// The diagram's size, in its units.
    pub diagram: Size,
    pub view: View,
    pub glide: Option<Glide>,
}

impl Headings<'_> {
    /// The bands held now in the pane `local`.
    pub fn held(&self, local: Rectangle) -> Vec<Held> {
        let view = self
            .glide
            .map_or(self.view, |glide| glide.at(crate::clock::now()));
        held(self.bands, self.heads, self.diagram, view, local)
    }

    /// [`through`] the bands held now.
    pub fn through(&self, at: Point, local: Rectangle) -> Point {
        through(&self.held(local), at)
    }
}

fn rectangle(rect: omaspec_uml::graph::Rect) -> Rectangle {
    Rectangle::new(Point::new(rect.x, rect.y), Size::new(rect.w, rect.h))
}

/// The bands of the diagram on screen, each held at its edge where it
/// scrolled past it.
pub struct KeptBands<'a> {
    pub headings: Headings<'a>,
    /// The whole diagram's pixels, which the bands are drawn from.
    pub overview: &'a Raster,
    /// Sharper pixels of the band across the top and of the one down the
    /// left, where the overview is not sharp at this zoom.
    pub sharp: [Option<&'a Raster>; 2],
    /// How many pixels of the screen a unit of the pane is.
    pub scale: f32,
}

impl canvas::Program<Message> for KeptBands<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let mut frame = Frame::new(renderer, bounds.size());
        let held = self.headings.held(local);
        let view = self
            .headings
            .glide
            .map_or(self.headings.view, |glide| glide.at(crate::clock::now()));
        let grounds: Vec<(Rectangle, Color)> = self
            .headings
            .grounds
            .iter()
            .map(|ground| {
                let omaspec_uml::Color(r, g, b) = ground.fill;
                (
                    view.on_screen(rectangle(ground.rect), local),
                    Color::from_rgb8(r, g, b),
                )
            })
            .collect();
        for band in &held {
            let paper = self.overview.paper;
            if band.apart {
                self.pills(&mut frame, local, band, &grounds);
                continue;
            }
            frame.fill_rectangle(band.strip.position(), band.strip.size(), paper);
            let sharp = self.sharp[match band.edge {
                Edge::Top => 0,
                Edge::Left => 1,
            }]
            .filter(|raster| raster.region.intersects(&band.src));
            for raster in std::iter::once(self.overview).chain(sharp) {
                let blit = sprite::Blit {
                    raster,
                    src: band.src,
                    dest: band.dest,
                    limit: band.strip,
                    alpha: 1.0,
                };
                sprite::blit(&mut frame, local, &blit);
            }
            fade(
                &mut frame, band.strip, band.edge, paper, &grounds, self.scale,
            );
        }
        // Where headings across the top and down the left meet, the corner
        // is the paper's, as a sheet's is.
        for top in held.iter().filter(|band| band.edge == Edge::Top) {
            for left in held.iter().filter(|band| band.edge == Edge::Left) {
                if let Some(corner) = top.strip.intersection(&left.strip) {
                    frame.fill_rectangle(corner.position(), corner.size(), self.overview.paper);
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

impl KeptBands<'_> {
    /// The pills of `band`: each participant that has scrolled past the
    /// top on its way from its own shape to a pill with its name, the
    /// paper round it thinning out in the color it stands on.
    fn pills(
        &self,
        frame: &mut Frame,
        local: Rectangle,
        band: &Held,
        grounds: &[(Rectangle, Color)],
    ) {
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        let snap = |at: f32| (at * scale).round() / scale;
        let rgb = |omaspec_uml::Color(r, g, b)| Color::from_rgb8(r, g, b);
        let sharp = self.sharp[0];
        // What each stands on in the picture: a box's fill, or the paper.
        let ground = |pill: &Pill| {
            grounds
                .iter()
                .filter(|(ground, _)| ground.contains(pill.on.center()))
                .min_by(|a, b| (a.0.width * a.0.height).total_cmp(&(b.0.width * b.0.height)))
                .map_or(self.overview.paper, |(_, color)| *color)
        };
        let own = |pill: &Pill| {
            let (left, top) = (snap(pill.shape.x), snap(pill.shape.y));
            let (right, bottom) = (
                snap(pill.shape.x + pill.shape.width),
                snap(pill.shape.y + pill.shape.height),
            );
            Rectangle::new(Point::new(left, top), Size::new(right - left, bottom - top))
        };
        // From its own corners to a pill's round ends.
        let round =
            |pill: &Pill, own: Rectangle| 3.0 + (own.height / 2.0 - 3.0).max(0.0) * pill.gone;
        // All that lies under the pills first, then the pills: the halo of
        // one does not wash over its neighbor.
        for pill in &band.heads {
            let ground = ground(pill);
            // What is still in sight of the participant itself, on its way
            // out under the top, is covered: it is the pill now. Its
            // shadow too, which lies right of it and below.
            let zoom = pill.on.height / pill.head.rect.h.max(1e-3);
            let (rim, shadow) = (1.5, 4.0 * zoom);
            let left_over = Rectangle::new(
                pill.on.position() - Vector::new(rim, rim),
                pill.on.size() + Size::new(2.0 * rim + shadow, 2.0 * rim + shadow),
            );
            if let Some(cover) = left_over.intersection(&local) {
                frame.fill_rectangle(cover.position(), cover.size(), ground);
            }
        }
        for pill in &band.heads {
            let (ground, own) = (ground(pill), own(pill));
            // Layers that grow by whole pixels, each as thin: many lie
            // near it, so the color is whole there and gone at the edge.
            let alpha = 1.0 - 0.04_f32.powf(1.0 / f32::from(HALO_LAYERS));
            for layer in 0..HALO_LAYERS {
                let grow = snap(HALO * (1.0 - f32::from(layer) / f32::from(HALO_LAYERS)).powi(2));
                let path = canvas::Path::rounded_rectangle(
                    own.position() - Vector::new(grow, grow),
                    own.size() + Size::new(grow * 2.0, grow * 2.0),
                    (round(pill, own) + grow).into(),
                );
                frame.fill(&path, Color { a: alpha, ..ground });
            }
        }
        for pill in &band.heads {
            let (ground, own) = (ground(pill), own(pill));
            let fill = pill.head.fill.map_or(ground, rgb);
            let body = canvas::Path::rounded_rectangle(
                own.position(),
                own.size(),
                round(pill, own).into(),
            );
            frame.fill(&body, fill);
            frame.stroke(
                &body,
                canvas::Stroke::default().with_width(1.0).with_color(Color {
                    a: pill.gone,
                    ..rgb(pill.head.border)
                }),
            );
            // Its own shape from the picture, going, and its name, coming.
            let pictures = [
                (rectangle(pill.head.rect), pill.shape, 1.0 - pill.gone),
                (rectangle(pill.head.label), pill.name, pill.gone),
            ];
            for (src, dest, alpha) in pictures {
                // One picture at that strength: the sharp one where it
                // holds all of it, or the overview. Both would show
                // stronger than asked, the blurred under the sharp.
                let covers = |raster: &&Raster| {
                    let r = raster.region;
                    r.x <= src.x
                        && r.y <= src.y
                        && r.x + r.width >= src.x + src.width
                        && r.y + r.height >= src.y + src.height
                };
                let raster = sharp.filter(covers).unwrap_or(self.overview);
                let blit = sprite::Blit {
                    raster,
                    src,
                    dest,
                    limit: own,
                    alpha,
                };
                sprite::blit(frame, local, &blit);
            }
        }
    }
}

/// Where a band now `on` screen is held at its `edge` of the pane
/// `local`, if it scrolled past it and what it heads, `over`, is still in
/// sight: at the edge, or pushed past it by the end of what it heads.
/// `None` where it is in sight, or gone with what it heads.
fn held_at(on: Rectangle, edge: Edge, over: Rectangle, local: Rectangle) -> Option<Rectangle> {
    match edge {
        Edge::Top => {
            if on.y >= local.y {
                return None;
            }
            let y = local.y.min(over.y + over.height - on.height);
            (y + on.height > local.y).then(|| Rectangle::new(Point::new(on.x, y), on.size()))
        }
        Edge::Left => {
            if on.x >= local.x {
                return None;
            }
            let x = local.x.min(over.x + over.width - on.width);
            (x + on.width > local.x).then(|| Rectangle::new(Point::new(x, on.y), on.size()))
        }
    }
}

/// The paper behind a band held at `dest`: as wide as the paper is in
/// sight for one at the top, as high for one at the left.
fn strip(dest: Rectangle, edge: Edge, paper: Rectangle, local: Rectangle) -> Rectangle {
    match edge {
        Edge::Top => {
            let left = paper.x.max(local.x);
            let right = (paper.x + paper.width).min(local.x + local.width);
            Rectangle::new(
                Point::new(left, dest.y),
                Size::new((right - left).max(0.0), dest.height),
            )
        }
        Edge::Left => {
            let top = paper.y.max(local.y);
            let bottom = (paper.y + paper.height).min(local.y + local.height);
            Rectangle::new(
                Point::new(dest.x, top),
                Size::new(dest.width, (bottom - top).max(0.0)),
            )
        }
    }
}

/// The paper going on past a band held at the top, or beside one at the
/// left, and thinning out: what scrolls toward the band fades into the
/// paper before it goes under it; over one of the `grounds` (an area with
/// a fill of its own, on screen), into that fill.
fn fade(
    frame: &mut Frame,
    strip: Rectangle,
    edge: Edge,
    paper: Color,
    grounds: &[(Rectangle, Color)],
    scale: f32,
) {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let zone = match edge {
        Edge::Top => Rectangle::new(
            Point::new(strip.x, strip.y + strip.height),
            Size::new(strip.width, FADE),
        ),
        Edge::Left => Rectangle::new(
            Point::new(strip.x + strip.width, strip.y),
            Size::new(FADE, strip.height),
        ),
    };
    // Along the band: the pieces between the sides of the grounds the
    // zone lies in, each in the color under it.
    let along = |r: Rectangle| match edge {
        Edge::Top => (r.x, r.x + r.width),
        Edge::Left => (r.y, r.y + r.height),
    };
    let across = |r: Rectangle| match edge {
        Edge::Top => (r.y, r.y + r.height),
        Edge::Left => (r.x, r.x + r.width),
    };
    let (from, until) = along(zone);
    let (near, far) = across(zone);
    let under: Vec<(f32, f32, Color)> = grounds
        .iter()
        .filter(|(rect, _)| {
            let (a, b) = across(*rect);
            a <= near + 0.5 && b >= far - 0.5
        })
        .map(|(rect, color)| {
            let (a, b) = along(*rect);
            (a.max(from).round(), b.min(until).round(), *color)
        })
        .filter(|(a, b, _)| b > a)
        .collect();
    let mut cuts: Vec<f32> = under.iter().flat_map(|(a, b, _)| [*a, *b]).collect();
    cuts.extend([from, until]);
    cuts.sort_by(f32::total_cmp);
    cuts.dedup();
    for pair in cuts.windows(2) {
        let middle = (pair[0] + pair[1]) / 2.0;
        // The smallest ground there: a box in a box shows its own fill.
        let color = under
            .iter()
            .filter(|(a, b, _)| *a <= middle && middle <= *b)
            .min_by(|x, y| (x.1 - x.0).total_cmp(&(y.1 - y.0)))
            .map_or(paper, |(_, _, color)| *color);
        let (position, size) = match edge {
            Edge::Top => (
                Point::new(pair[0], zone.y),
                Size::new(pair[1] - pair[0], zone.height),
            ),
            Edge::Left => (
                Point::new(zone.x, pair[0]),
                Size::new(zone.width, pair[1] - pair[0]),
            ),
        };
        // In steps of one color each, on whole pixels of the screen: a
        // gradient showed a line at each of its stops in some windows.
        let snap = |at: f32| (at * scale).round() / scale;
        let steps = (FADE * scale).round().max(1.0) as usize;
        for step in 0..steps {
            let k = 1.0 - (step as f32 + 0.5) / steps as f32;
            let shade = Color {
                a: k * k * (3.0 - 2.0 * k),
                ..color
            };
            let (from, to) = (
                snap(step as f32 / steps as f32 * FADE),
                snap((step + 1) as f32 / steps as f32 * FADE),
            );
            let (at, size) = match edge {
                Edge::Top => (
                    Point::new(position.x, snap(position.y) + from),
                    Size::new(size.width, to - from),
                ),
                Edge::Left => (
                    Point::new(snap(position.x) + from, position.y),
                    Size::new(to - from, size.height),
                ),
            };
            frame.fill_rectangle(at, size, shade);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }

    /// A band in sight stays where it is; one scrolled past the top is held
    /// there while what it heads goes on below it, pushed up by its end,
    /// and gone with it; one at the left alike.
    #[test]
    fn a_band_is_held_at_its_edge_while_what_it_heads_is_in_sight() {
        let local = rect(0.0, 0.0, 800.0, 600.0);
        let over = rect(0.0, -960.0, 800.0, 2900.0);
        assert_eq!(
            held_at(rect(0.0, 20.0, 800.0, 40.0), Edge::Top, over, local),
            None
        );
        assert_eq!(
            held_at(rect(10.0, -400.0, 700.0, 40.0), Edge::Top, over, local),
            Some(rect(10.0, 0.0, 700.0, 40.0))
        );
        // What it heads ends 20 pixels down the pane, though the paper
        // goes on under it (a footer, the feet): the band goes up with it.
        let ending = rect(0.0, -2940.0, 800.0, 2960.0);
        assert_eq!(
            held_at(rect(10.0, -2980.0, 700.0, 40.0), Edge::Top, ending, local),
            Some(rect(10.0, -20.0, 700.0, 40.0))
        );
        let gone = rect(0.0, -2960.0, 800.0, 2960.0);
        assert_eq!(
            held_at(rect(10.0, -3000.0, 700.0, 40.0), Edge::Top, gone, local),
            None
        );
        let wide = rect(-750.0, 0.0, 1850.0, 600.0);
        assert_eq!(
            held_at(rect(-900.0, 80.0, 150.0, 500.0), Edge::Left, wide, local),
            Some(rect(0.0, 80.0, 150.0, 500.0))
        );
        assert_eq!(
            strip(rect(0.0, 80.0, 150.0, 500.0), Edge::Left, wide, local),
            rect(0.0, 0.0, 150.0, 600.0)
        );
    }

    /// A heading zoomed in past a third of the pane scrolls away rather
    /// than cover what it heads; a point in a held one is the heading's.
    #[test]
    fn a_heading_too_large_scrolls_away_and_a_point_in_one_is_its_own() {
        let local = rect(0.0, 0.0, 800.0, 600.0);
        let diagram = Size::new(400.0, 3000.0);
        let band = Band {
            rect: omaspec_uml::graph::Rect {
                x: 0.0,
                y: 0.0,
                w: 400.0,
                h: 40.0,
            },
            edge: Edge::Top,
            over: omaspec_uml::graph::Rect {
                x: 0.0,
                y: 40.0,
                w: 400.0,
                h: 2900.0,
            },
        };
        let view = |zoom: f32| View {
            zoom,
            center: Point::new(200.0, 1500.0),
            fit: false,
        };
        let kept = held(&[band], &[], diagram, view(1.0), local);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].dest.y, 0.0);
        // A click on the held heading picks the heading, 1200 pixels up.
        let at = Point::new(400.0, 20.0);
        assert_eq!(through(&kept, at), Point::new(400.0, 20.0 - 1200.0 + 0.0));
        assert_eq!(
            through(&kept, Point::new(400.0, 300.0)),
            Point::new(400.0, 300.0)
        );
        // Zoomed in six times, the heading is 240 pixels high: too high.
        assert!(held(&[band], &[], diagram, view(6.0), local).is_empty());

        // Its heads alone held: one that has scrolled past the top is a
        // pill there, and a click on it picks its participant.
        let head = |x: f32| Heading {
            rect: omaspec_uml::graph::Rect {
                x,
                y: 10.0,
                w: 80.0,
                h: 30.0,
            },
            label: omaspec_uml::graph::Rect {
                x: x + 20.0,
                y: 18.0,
                w: 40.0,
                h: 14.0,
            },
            line: 14.0,
            fill: None,
            border: omaspec_uml::Color(0, 0, 0),
        };
        let kept = held(
            &[band],
            &[head(20.0), head(300.0)],
            diagram,
            view(1.0),
            local,
        );
        let pills = &kept[0].heads;
        assert_eq!(pills.len(), 2);
        assert_eq!(pills[0].gone, 1.0);
        // Its name as large as drawn, with room round it, under the top
        // and in the middle of its participant, 260 in the pane.
        let shape = pills[0].shape;
        assert_eq!((shape.y, shape.center_x()), (PILL_TOP, 260.0));
        assert!(shape.width > 40.0 && shape.width < 80.0 && shape.height < 30.0);
        assert_eq!(pills[0].name.size(), Size::new(40.0, 14.0));
        let at = shape.center();
        assert_eq!(through(&kept, at), Point::new(260.0, 25.0 - 1200.0));
        let between = Point::new(400.0, 12.0);
        assert_eq!(through(&kept, between), between);
        // Zoomed in, the name is as small as a pill's.
        let kept = held(&[band], &[head(20.0)], diagram, view(6.0), local);
        assert!((kept[0].heads[0].name.height - PILL_LINE).abs() < 0.01);
    }
}
