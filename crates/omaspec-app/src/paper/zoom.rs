//! Zoom, pan and fit. The view moves at once or glides, and the sharp
//! pixels for where it ends are asked for ([`super::raster`]).

use std::time::Duration;

use iced::time::Instant;
use iced::{Point, Size, Task, Vector};

use super::raster::Plan;
use super::view::{Glide, View};
use super::{Change, Message, Paper, RASTER_DELAY, frames};

impl Paper {
    pub fn set_scale_factor(&mut self, factor: f32) -> Task<Message> {
        self.scale_factor = factor;
        self.rasterize(Duration::ZERO)
    }

    /// Zooms by `factor` around the middle of the pane.
    pub fn zoom(&mut self, factor: f32) -> Task<Message> {
        let center = Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0);
        self.update(Message::ZoomStep(factor, center))
    }

    pub fn pan(&mut self, dx: f32, dy: f32) -> Task<Message> {
        self.update(Message::Pan(Vector::new(dx, dy)))
    }

    /// The diagram at 100 %, gliding there.
    pub fn actual_size(&mut self) -> Task<Message> {
        let center = Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0);
        self.glide_to(self.view.actual_size(), center)
    }

    /// Glides from the view on screen now to `to`, around `at`. The sharp
    /// pixels for `to` are asked for at once, so they are in about when the
    /// glide arrives; a few wheel notches in a row ask once.
    pub(super) fn glide_to(&mut self, to: View, at: Point) -> Task<Message> {
        let now = Instant::now();
        let from = self.glide.map_or(self.view, |g| g.at(now));
        self.view = to;
        self.glide = Some(Glide::new(from, to, at, self.viewport, now));
        self.rasterize(RASTER_DELAY)
    }

    /// Stops a glide where it is: a pan or a pinch goes on from there, and
    /// a zoom out of a link leaves the diagram it led to behind.
    pub(super) fn settle(&mut self) {
        if let Some(glide) = self.glide.take() {
            self.view = glide.at(Instant::now());
            if self.passage.as_ref().is_some_and(|p| !p.inward) {
                self.passage = None;
            }
        }
        let _ = self.show_held();
    }

    /// Takes `view` from another pane (Compare A|B zooms both together): a
    /// fitted one fits this diagram.
    pub fn follow(&mut self, view: View) -> Task<Message> {
        self.settle();
        if view.fit {
            return self.refit();
        }
        self.view = view;
        self.note_view();
        self.rasterize(RASTER_DELAY)
    }

    /// Whether `message` moves or zooms the view (and another pane should
    /// follow).
    pub fn moves_view(message: &Message) -> bool {
        matches!(
            message,
            Message::ZoomAt(..)
                | Message::ZoomStep(..)
                | Message::Pan(_)
                | Message::Pinch { .. }
                | Message::Fit
        )
    }

    /// The whole diagram in the pane, following its size from now on;
    /// it glides there.
    pub fn fit(&mut self) -> Task<Message> {
        if self.diagram.is_none() || self.viewport == Size::ZERO {
            self.view.fit = true;
            return Task::none();
        }
        let center = Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0);
        self.glide_to(View::fitted(self.diagram_size(), self.viewport), center)
    }

    /// Fits again at once, after the pane or the diagram changed.
    pub(super) fn refit(&mut self) -> Task<Message> {
        if self.diagram.is_none() || self.viewport == Size::ZERO {
            self.view.fit = true;
            return Task::none();
        }
        self.view = View::fitted(self.diagram_size(), self.viewport);
        self.note_view();
        self.rasterize(Duration::ZERO)
    }

    /// Drops the pixels while the pane is hidden; a viewport report brings
    /// them back.
    pub fn release(&mut self) {
        // What moved here has ended, though its last frame never came.
        frames::end();
        self.held_overview = None;
        self.held_detail = None;
        self.overview.clear();
        self.detail.clear();
        self.fresh = false;
        self.landed = false;
        self.previous = None;
        self.fade = None;
        self.change = Change::Edit;
    }

    pub(super) fn diagram_size(&self) -> Size {
        self.diagram
            .as_ref()
            .map_or(Size::new(1.0, 1.0), |d| d.size)
    }

    /// Rasterizes what the view needs sharp, after `delay` (see
    /// [`super::raster`]); asks nothing when the rasters in hand will do. A map
    /// zoomed in until its tiles can be read has its pictures parsed first.
    pub(super) fn rasterize(&mut self, delay: Duration) -> Task<Message> {
        if self.viewport == Size::ZERO {
            return Task::none();
        }
        let (parse, tree) = self.picture_tree();
        let (Some(tree), Some(diagram)) = (tree, &self.diagram) else {
            return parse;
        };
        let scale = self.view.zoom * self.scale_factor;
        let visible = self.view.visible(self.viewport);
        let overview = Plan::overview(diagram.size, scale);
        let detail = Plan::detail(diagram.size, scale, visible, self.detail.asked);
        Task::batch([
            parse,
            self.overview.ask(overview, &tree, delay, Message::Overview),
            self.detail.ask(detail, &tree, delay, Message::Detail),
        ])
    }
}
