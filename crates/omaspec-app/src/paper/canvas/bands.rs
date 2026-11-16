//! The headings of a long diagram kept in sight (REQ-073). A band of the
//! picture its drawer names ([`omaspec_uml::map::Map::bands`]) that scrolls
//! up past the pane's top, or out past its left edge, stays there while
//! what it heads is in sight, and goes with the diagram's end: a sequence
//! diagram's participants over its messages, a swimlane's name over its
//! lane, a Gantt chart's axis over its rows and its tasks beside their bars.
//!
//! Only the pane holds them: they are pixels of the picture on screen,
//! drawn again at the edge, and the picture and every export are as drawn.

use iced::mouse::Cursor;
use iced::widget::canvas::{self, Frame, Geometry};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use omaspec_uml::map::{Band, Edge};

use super::super::Message;
use super::super::raster::Raster;
use super::super::sprite;
use super::super::view::{Glide, View};

/// How far the shade under a band held at the top reaches, or beside one
/// held at the left, in pixels; and how dark it starts.
const SHADE: f32 = 6.0;
const SHADE_ALPHA: f32 = 0.16;

/// The bands of the diagram on screen, each held at its edge where it
/// scrolled past it.
pub struct KeptBands<'a> {
    /// The diagram's size, in its units.
    pub diagram: Size,
    pub bands: &'a [Band],
    /// The whole diagram's pixels, which the bands are drawn from.
    pub overview: &'a Raster,
    pub view: View,
    pub glide: Option<Glide>,
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
        let view = self
            .glide
            .map_or(self.view, |glide| glide.at(crate::clock::now()));
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let paper = view.diagram_rect(self.diagram, local);
        let mut frame = Frame::new(renderer, bounds.size());
        let mut strips: Vec<(Edge, Rectangle)> = Vec::new();
        // Across the top first, then down the left over them, as a sheet
        // holds its headings.
        for edge in [Edge::Top, Edge::Left] {
            for band in self.bands.iter().filter(|band| band.edge == edge) {
                let src = Rectangle::new(
                    Point::new(band.rect.x, band.rect.y),
                    Size::new(band.rect.w, band.rect.h),
                );
                let Some(dest) = held_at(view.on_screen(src, local), edge, paper, local) else {
                    continue;
                };
                let strip = strip(dest, edge, paper, local);
                frame.fill_rectangle(strip.position(), strip.size(), self.overview.paper);
                let blit = sprite::Blit {
                    raster: self.overview,
                    src,
                    dest,
                    limit: strip,
                    alpha: 1.0,
                };
                sprite::blit(&mut frame, local, &blit);
                shade(&mut frame, strip, edge);
                strips.push((edge, strip));
            }
        }
        // Where headings across the top and down the left meet, the corner
        // is the paper's, as a sheet's is.
        for (_, top) in strips.iter().filter(|(edge, _)| *edge == Edge::Top) {
            for (_, left) in strips.iter().filter(|(edge, _)| *edge == Edge::Left) {
                if let Some(corner) = top.intersection(left) {
                    frame.fill_rectangle(corner.position(), corner.size(), self.overview.paper);
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

/// Where a band now `on` screen is held at its `edge` of the pane
/// `local`, if it scrolled past it and what it heads, the rest of the
/// `paper`, is still in sight: at the edge, or pushed past it by the
/// paper's end. `None` where it is in sight, or gone with the paper.
fn held_at(on: Rectangle, edge: Edge, paper: Rectangle, local: Rectangle) -> Option<Rectangle> {
    match edge {
        Edge::Top => {
            if on.y >= local.y {
                return None;
            }
            let y = local.y.min(paper.y + paper.height - on.height);
            (y + on.height > local.y).then(|| Rectangle::new(Point::new(on.x, y), on.size()))
        }
        Edge::Left => {
            if on.x >= local.x {
                return None;
            }
            let x = local.x.min(paper.x + paper.width - on.width);
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

/// A soft shade under a band held at the top, or beside one at the left,
/// so what scrolls under it reads as under it.
fn shade(frame: &mut Frame, strip: Rectangle, edge: Edge) {
    let steps = 4;
    for step in 0..steps {
        let alpha = SHADE_ALPHA * (1.0 - step as f32 / steps as f32);
        let reach = SHADE / steps as f32;
        let at = reach * step as f32;
        let (position, size) = match edge {
            Edge::Top => (
                Point::new(strip.x, strip.y + strip.height + at),
                Size::new(strip.width, reach),
            ),
            Edge::Left => (
                Point::new(strip.x + strip.width + at, strip.y),
                Size::new(reach, strip.height),
            ),
        };
        frame.fill_rectangle(position, size, Color::from_rgba(0.0, 0.0, 0.0, alpha));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }

    /// A band in sight stays where it is; one scrolled past the top is held
    /// there while the paper goes on below it, pushed up by the paper's end,
    /// and gone with it; one at the left alike.
    #[test]
    fn a_band_is_held_at_its_edge_while_what_it_heads_is_in_sight() {
        let local = rect(0.0, 0.0, 800.0, 600.0);
        let paper = rect(0.0, -1000.0, 800.0, 3000.0);
        assert_eq!(
            held_at(rect(0.0, 20.0, 800.0, 40.0), Edge::Top, paper, local),
            None
        );
        assert_eq!(
            held_at(rect(10.0, -400.0, 700.0, 40.0), Edge::Top, paper, local),
            Some(rect(10.0, 0.0, 700.0, 40.0))
        );
        // The paper ends 20 pixels down the pane: the band goes up with it.
        let ending = rect(0.0, -2980.0, 800.0, 3000.0);
        assert_eq!(
            held_at(rect(10.0, -2980.0, 700.0, 40.0), Edge::Top, ending, local),
            Some(rect(10.0, -20.0, 700.0, 40.0))
        );
        let gone = rect(0.0, -3000.0, 800.0, 3000.0);
        assert_eq!(
            held_at(rect(10.0, -3000.0, 700.0, 40.0), Edge::Top, gone, local),
            None
        );
        let wide = rect(-900.0, 0.0, 2000.0, 600.0);
        assert_eq!(
            held_at(rect(-900.0, 80.0, 150.0, 500.0), Edge::Left, wide, local),
            Some(rect(0.0, 80.0, 150.0, 500.0))
        );
        assert_eq!(
            strip(rect(0.0, 80.0, 150.0, 500.0), Edge::Left, wide, local),
            rect(0.0, 0.0, 150.0, 600.0)
        );
    }
}
