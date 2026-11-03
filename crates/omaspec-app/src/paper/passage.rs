//! The way into a linked diagram and back out
//! (`docs/adr/0018-diagrams-that-link.md`). As a zoom into a link
//! arrives, the diagram it leads to grows out of the link's box and the
//! picture around it sinks under the pane's background; the zoom ends
//! where that diagram shows in its own tab, so the switch to the tab does
//! not show. On the way back the diagram shrinks into its box as the
//! picture around it comes back.

use iced::mouse::Cursor;
use iced::widget::canvas::{self, Frame, Geometry};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, Vector};

use super::Message;
use super::canvas::{Grid, RADIUS, rounded, shadow};
use super::raster::Raster;
use super::view::{Glide, View};

/// A diagram as its pane shows it: what a zoom into a link to it ends on,
/// or a zoom out of it starts from.
#[derive(Debug, Clone)]
pub struct Showing {
    /// Its size, in its own units.
    pub size: Size,
    /// Where it lies in the pane.
    pub place: Rectangle,
    /// Its whole picture, once rasterized.
    pub picture: Option<Raster>,
}

/// A zoom into a link, or out of it, under way.
#[derive(Debug, Clone)]
pub struct Passage {
    /// The link's box, in this diagram's units.
    pub area: Rectangle,
    /// The diagram it leads to, once that is drawn.
    pub linked: Option<Showing>,
    /// In: the linked diagram comes as the zoom arrives. Out: it goes as
    /// the zoom leaves.
    pub inward: bool,
}

impl Passage {
    /// How much of the linked diagram, and of the veil around it, shows at
    /// `progress` of the way (0 to 1): the last half of the way in, the
    /// first half of the way out.
    pub fn shown(&self, progress: f32) -> f32 {
        let smooth = |t: f32| {
            let t = t.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        if self.inward {
            smooth((progress - 0.5) / 0.5)
        } else {
            1.0 - smooth(progress / 0.5)
        }
    }
}

/// A diagram of `size` in `room` on screen: as large as fits, centered.
pub fn fitted_in(size: Size, room: Rectangle) -> Rectangle {
    let scale = (room.width / size.width.max(f32::EPSILON))
        .min(room.height / size.height.max(f32::EPSILON));
    let shown = size * scale;
    Rectangle::new(
        room.center() - Vector::new(shown.width / 2.0, shown.height / 2.0),
        shown,
    )
}

/// The view of this diagram on which its `area`, holding a diagram of
/// `size` as [`fitted_in`] lays it, shows that diagram at `place`: where a
/// zoom into a link ends, and a zoom out of it starts.
pub fn view_holding(area: Rectangle, size: Size, place: Rectangle, viewport: Size) -> View {
    let size = Size::new(size.width.max(f32::EPSILON), size.height.max(f32::EPSILON));
    // Screen pixels for a unit of the linked diagram, and units of the
    // linked diagram for a unit of this one inside the area.
    let pixels = place.width / size.width;
    let inside = (area.width / size.width).min(area.height / size.height);
    let zoom = pixels / inside.max(f32::EPSILON);
    let middle = Point::new(viewport.width / 2.0, viewport.height / 2.0);
    View {
        zoom,
        center: area.center() - (place.center() - middle) * (1.0 / zoom),
        fit: false,
    }
}

/// The passage in a layer of its own over the diagram: in the diagram's
/// layer, its picture would cover the veil.
pub struct PassageLayer<'a> {
    pub passage: &'a Passage,
    pub view: View,
    pub glide: Option<Glide>,
    /// The pane's background, which the veil is made of.
    pub veil: Color,
    /// The window's scale.
    pub scale: f32,
}

impl canvas::Program<Message> for PassageLayer<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let now = iced::time::Instant::now();
        let (view, progress) = self
            .glide
            .map_or((self.view, 1.0), |g| (g.at(now), g.progress(now)));
        let shown = self.passage.shown(progress);
        if shown <= 0.0 {
            return vec![frame.into_geometry()];
        }
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            Color {
                a: shown,
                ..self.veil
            },
        );
        if let Some(linked) = &self.passage.linked {
            let grid = Grid::of(bounds, self.scale);
            let room = view.on_screen(self.passage.area, local);
            let paper = grid.snap(fitted_in(linked.size, room));
            shadow(&mut frame, paper, self.veil, self.scale, shown);
            if let Some(picture) = &linked.picture {
                frame.fill(
                    &rounded(paper, RADIUS),
                    Color {
                        a: shown,
                        ..picture.paper
                    },
                );
                // The picture's region, in the linked diagram's units, on
                // its paper.
                let k = paper.width / linked.size.width.max(f32::EPSILON);
                let r = picture.region;
                let at = Rectangle::new(
                    paper.position() + Vector::new(r.x * k, r.y * k),
                    r.size() * k,
                );
                grid.draw(&mut frame, paper, RADIUS, [(at, picture, shown)]);
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_linked_diagram_lands_where_its_tab_shows_it() {
        let viewport = Size::new(800.0, 600.0);
        let area = Rectangle::new(Point::new(300.0, 120.0), Size::new(90.0, 30.0));
        // A diagram taller than wide, fitted in its tab.
        let size = Size::new(400.0, 500.0);
        let place = View::fitted(size, viewport).diagram_rect(size, Rectangle::with_size(viewport));
        let view = view_holding(area, size, place, viewport);
        let landed = fitted_in(size, view.on_screen(area, Rectangle::with_size(viewport)));
        for (a, b) in [
            (landed.x, place.x),
            (landed.y, place.y),
            (landed.width, place.width),
            (landed.height, place.height),
        ] {
            assert!((a - b).abs() < 0.01, "{landed:?} {place:?}");
        }
    }

    #[test]
    fn it_comes_late_on_the_way_in_and_goes_early_on_the_way_out() {
        let passage = |inward| Passage {
            area: Rectangle::new(Point::ORIGIN, Size::new(10.0, 10.0)),
            linked: None,
            inward,
        };
        let (into, out) = (passage(true), passage(false));
        assert_eq!(into.shown(0.0), 0.0);
        assert_eq!(into.shown(0.5), 0.0);
        assert!(into.shown(0.75) > 0.0 && into.shown(0.75) < 1.0);
        assert_eq!(into.shown(1.0), 1.0);
        assert_eq!(out.shown(0.0), 1.0);
        assert_eq!(out.shown(0.5), 0.0);
        assert_eq!(out.shown(1.0), 0.0);
    }
}
