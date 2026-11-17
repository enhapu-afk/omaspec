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
use iced::widget::canvas::gradient::Linear;
use iced::widget::canvas::{self, Frame, Geometry};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector};
use omaspec_uml::map::{Band, Edge, Ground};

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

/// A band held at its edge of the pane.
#[derive(Debug, Clone, Copy, PartialEq)]
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
}

/// The bands of a `diagram` this large, seen in `view` in the pane
/// `local`, that are held at their edge: across the top first, then down
/// the left over them, as a sheet holds its headings.
pub fn held(bands: &[Band], diagram: Size, view: View, local: Rectangle) -> Vec<Held> {
    let paper = view.diagram_rect(diagram, local);
    let mut out = Vec::new();
    for edge in [Edge::Top, Edge::Left] {
        for band in bands.iter().filter(|band| band.edge == edge) {
            let src = rectangle(band.rect);
            let on = view.on_screen(src, local);
            let over = view.on_screen(rectangle(band.over), local);
            let thick = match edge {
                Edge::Top => on.height / local.height.max(1.0),
                Edge::Left => on.width / local.width.max(1.0),
            };
            if thick > MOST {
                continue;
            }
            let Some(dest) = held_at(on, edge, over, local) else {
                continue;
            };
            out.push(Held {
                edge,
                src,
                dest,
                strip: strip(dest, edge, paper, local),
                back: on.position() - dest.position(),
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
        .find(|band| band.strip.contains(at))
        .map_or(at, |band| at + band.back)
}

/// The headings of the picture on screen, and how it is seen: what
/// [`KeptBands`] draws and the picture's clicks go through.
#[derive(Debug, Clone, Copy)]
pub struct Headings<'a> {
    pub bands: &'a [Band],
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
        held(self.bands, self.diagram, view, local)
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
            fade(&mut frame, band.strip, band.edge, paper, &grounds);
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
) {
    let (zone, to) = match edge {
        Edge::Top => (
            Rectangle::new(
                Point::new(strip.x, strip.y + strip.height),
                Size::new(strip.width, FADE),
            ),
            Vector::new(0.0, FADE),
        ),
        Edge::Left => (
            Rectangle::new(
                Point::new(strip.x + strip.width, strip.y),
                Size::new(FADE, strip.height),
            ),
            Vector::new(FADE, 0.0),
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
        // Eight stops, the most iced keeps: dense at first, then a tail.
        let mut thinning = Linear::new(position, position + to);
        for stop in 0..8 {
            let t = stop as f32 / 7.0;
            let k = 1.0 - t;
            thinning = thinning.add_stop(
                t,
                Color {
                    a: k * k * (3.0 - 2.0 * k),
                    ..color
                },
            );
        }
        frame.fill_rectangle(position, size, thinning);
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
        let kept = held(&[band], diagram, view(1.0), local);
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
        assert!(held(&[band], diagram, view(6.0), local).is_empty());
    }
}
