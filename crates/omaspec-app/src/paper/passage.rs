//! The way into a linked diagram and back out
//! (`docs/adr/0018-diagrams-that-link.md`). As a zoom into a link
//! arrives, the diagram it leads to grows out of the link's box and the
//! picture around it sinks under the pane's background; the zoom ends
//! where that diagram shows in its own tab, so the switch to the tab does
//! not show. On the way back the diagram shrinks into its box as the
//! picture around it comes back.

use std::time::Duration;

use iced::mouse::Cursor;
use iced::time::Instant;
use iced::widget::canvas::{self, Frame, Geometry};
use iced::{Color, Point, Rectangle, Renderer, Size, Task, Theme, Vector};
use omaspec_render::Link;

use super::canvas::{Grid, RADIUS, rounded, shadow};
use super::raster::Raster;
use super::view::{Glide, View};
use super::{Message, Paper, link_under};

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
    /// Where an outward zoom runs to, once this diagram has a picture.
    /// Set means the veil stays up until then: an empty pane under a
    /// falling veil is a black frame.
    pub goal: Option<View>,
}

impl Passage {
    /// How much of the linked diagram, and of the veil around it, shows at
    /// `progress` of the way (0 to 1): the last half of the way in, the
    /// first half of the way out.
    pub fn shown(&self, progress: f32) -> f32 {
        if self.goal.is_some() {
            return 1.0;
        }
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

impl Paper {
    /// The tiles and states of the picture that open other diagrams.
    pub fn links(&self) -> &[Link] {
        self.diagram.as_ref().map_or(&[], |d| &d.drawn.links)
    }

    /// The link at `at`, a point of the pane: the smallest one there.
    pub fn link_at(&self, at: Point) -> Option<&Link> {
        let p = self.view.to_diagram(at, self.viewport);
        link_under(self.followable(), p)
    }

    /// The line of the map's box whose tile is at `at`, a point of the
    /// pane.
    pub fn tile_line_at(&self, at: Point) -> Option<usize> {
        let map = &self.diagram.as_ref()?.drawn.map;
        let p = self.view.to_diagram(at, self.viewport);
        map.tiles
            .iter()
            .find(|t| {
                let r = t.frame;
                p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h
            })
            .and_then(|t| t.line)
    }

    /// The links a click follows: none while a play runs.
    pub(super) fn followable(&self) -> &[Link] {
        if self.play.is_some() {
            &[]
        } else {
            self.links()
        }
    }

    /// Zooms into `area` of the diagram (in diagram units), on the way into
    /// the diagram it links to; how long that takes. With `landing`, the
    /// zoom ends where the area holds that diagram as it will show;
    /// without, with the area filling most of the pane until
    /// [`Self::arriving`] says where.
    pub fn zoom_into(
        &mut self,
        area: Rectangle,
        landing: Option<Showing>,
    ) -> (Task<Message>, Duration) {
        if self.viewport == Size::ZERO {
            return (Task::none(), Duration::ZERO);
        }
        let to = match &landing {
            Some(linked) => view_holding(area, linked.size, linked.place, self.viewport),
            None => View::showing(area, self.viewport),
        };
        self.passage = Some(Passage {
            area,
            linked: landing,
            inward: true,
            goal: None,
        });
        let task = self.dive(to);
        (task, self.glide.map_or(Duration::ZERO, |g| g.duration()))
    }

    /// The diagram the zoom into a link leads to, drawn: the zoom now ends
    /// where that diagram will show, in the time it had left, and its
    /// picture grows out of the link's box on the way.
    pub fn arriving(&mut self, linked: Showing) {
        let Some(passage) = self.passage.as_mut().filter(|p| p.inward) else {
            return;
        };
        let moved = passage
            .linked
            .as_ref()
            .is_none_or(|old| old.size != linked.size || old.place != linked.place);
        let area = passage.area;
        let to = view_holding(area, linked.size, linked.place, self.viewport);
        passage.linked = Some(linked);
        if !moved {
            return;
        }
        let now = Instant::now();
        match self.glide.filter(|g| !g.done(now)) {
            Some(glide) => {
                let left = glide.duration().saturating_sub(Duration::from_secs_f32(
                    glide.progress(now) * glide.duration().as_secs_f32(),
                ));
                self.glide = Some(Glide::dive(glide.at(now), to, self.viewport, now).taking(left));
            }
            None => self.glide = None,
        }
        self.view = to;
        self.note_view();
    }

    /// Ends a zoom into a link at `view`: the diagram it leads to is shown
    /// in its own tab now, and this one keeps `view` for a click back.
    pub fn land(&mut self, view: View) {
        self.glide = None;
        self.passage = None;
        self.view = view;
        self.note_view();
        let _ = self.show_held();
    }

    /// The diagram as the pane shows it, for a zoom into a link to it or
    /// out of it.
    pub fn showing(&self) -> Option<Showing> {
        let diagram = self.diagram.as_ref()?;
        if self.viewport == Size::ZERO {
            return None;
        }
        Some(Showing {
            size: diagram.size,
            place: self
                .view
                .diagram_rect(diagram.size, Rectangle::with_size(self.viewport)),
            picture: self.overview.shown.clone(),
        })
    }

    /// Starts in `area` of the diagram and zooms out to `to`: back from
    /// the diagram it links to, which shrinks into the area as `from`
    /// showed it. A fitted `to` fits the diagram.
    pub fn zoom_out_of(
        &mut self,
        area: Rectangle,
        to: View,
        from: Option<Showing>,
    ) -> Task<Message> {
        if self.viewport == Size::ZERO || self.diagram.is_none() {
            return Task::none();
        }
        let to = if to.fit {
            View::fitted(self.diagram_size(), self.viewport)
        } else {
            to
        };
        self.settle();
        let start = match &from {
            Some(linked) => view_holding(area, linked.size, linked.place, self.viewport),
            None => View::showing(area, self.viewport),
        };
        // No picture yet: the veil stays up, and the zoom starts when it
        // arrives. An empty pane under a falling veil is a black frame.
        // `goal` set means that wait.
        let goal = (from.is_some() && self.overview.shown.is_none()).then_some(to);
        self.view = start;
        self.note_view();
        self.passage = from.map(|linked| Passage {
            area,
            linked: Some(linked),
            inward: false,
            goal,
        });
        if goal.is_some() {
            self.glide = None;
            return self.rasterize(Duration::ZERO);
        }
        self.dive(to)
    }

    fn dive(&mut self, to: View) -> Task<Message> {
        let now = Instant::now();
        let from = self.glide.map_or(self.view, |g| g.at(now));
        self.view = to;
        self.glide = Some(Glide::dive(from, to, self.viewport, now));
        // Into a link, the end lies under the veil: no sharp pixels for
        // it, which would come in mid-way and cost a frame. Out of one,
        // the picture already on screen stays until the glide ends
        // ([`Self::holds`]), and the end's pixels are asked for at once.
        if self.inward() {
            return Task::none();
        }
        self.rasterize(Duration::ZERO)
    }

    /// Whether a zoom into a link runs, or has arrived.
    pub(super) fn inward(&self) -> bool {
        self.passage.as_ref().is_some_and(|p| p.inward)
    }

    /// A zoom out of a link that waits for this diagram's picture gives
    /// up: it ends where it was going, and the veil lifts. A pan or a zoom
    /// asks for that too; under the veil it would move nothing in sight.
    pub(super) fn stop_waiting(&mut self) {
        let waiting = self
            .passage
            .as_ref()
            .is_some_and(|passage| !passage.inward && passage.goal.is_some());
        if !waiting {
            return;
        }
        if let Some(goal) = self.passage.take().and_then(|passage| passage.goal) {
            self.view = goal;
            self.glide = None;
        }
    }

    /// A zoom out that waited for this diagram's picture can leave now.
    pub(super) fn resume_outward(&mut self) -> Task<Message> {
        let waiting = self
            .passage
            .as_ref()
            .is_some_and(|passage| passage.goal.is_some() && !passage.inward);
        if !waiting || self.overview.shown.is_none() {
            return Task::none();
        }
        let Some(to) = self
            .passage
            .as_mut()
            .and_then(|passage| passage.goal.take())
        else {
            return Task::none();
        };
        self.dive(to)
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
            goal: None,
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
