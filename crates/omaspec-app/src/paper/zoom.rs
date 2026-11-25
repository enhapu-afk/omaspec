//! Zoom, pan and fit. The view moves at once or glides, and the sharp
//! pixels for where it ends are asked for ([`super::raster`]).

use std::time::Duration;

use iced::time::Instant;
use iced::{Point, Rectangle, Size, Task, Vector};

use super::raster::Plan;
use super::view::{Glide, View};
use super::{Change, Message, Paper, RASTER_DELAY, frames};

/// An overview twice as sharp as the view is drawn up to this many pixels
/// (4 megapixels: about 30 ms in the background, 16 MB on the GPU).
const SHARPER_PIXELS: f32 = 4_000_000.0;

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
        let now = crate::clock::now();
        let from = self.glide.map_or(self.view, |g| g.at(now));
        self.view = to;
        self.glide = Some(Glide::new(from, to, at, self.viewport, now));
        self.rasterize(RASTER_DELAY)
    }

    /// Stops a glide where it is: a pan or a pinch goes on from there, and
    /// a zoom out of a link leaves the diagram it led to behind. The
    /// pixels held while it glided are shown: what they start (a glide's
    /// sheets, the way back to a drawing) is the task.
    pub(super) fn settle(&mut self) -> Task<Message> {
        if let Some(glide) = self.glide.take() {
            self.view = glide.at(crate::clock::now());
            if self.passage.as_ref().is_some_and(|p| !p.inward) {
                self.passage = None;
            }
        }
        self.show_held()
    }

    /// Glides a following play's camera ([`Paper::follow_play`]) to `area`
    /// of the diagram, or out to the whole, from `now` on the play's clock:
    /// on the shortest way, as a dive goes, so a step far from the last
    /// one zooms out a little on the way. What it looks at stands above the
    /// play's bar.
    pub(super) fn camera(&mut self, area: Option<Rectangle>, now: Instant) -> Task<Message> {
        if self.diagram.is_none() || self.viewport == Size::ZERO {
            return Task::none();
        }
        let whole = View::fitted(self.diagram_size(), self.viewport);
        let bar = self
            .play
            .as_ref()
            .zip(self.play_bar.tools())
            .map_or(0.0, |(play, tools)| {
                super::bar::height(&play.offer(), self.viewport.width, tools)
            });
        let to = match area {
            Some(area) => View::following(
                self.under_headings(area),
                self.diagram_size(),
                self.viewport,
                bar,
            ),
            None => whole,
        };
        let from = self.glide.map_or(self.view, |glide| glide.at(now));
        if from == to {
            return Task::none();
        }
        self.view = to;
        self.glide = Some(Glide::dive(from, to, self.viewport, now));
        self.rasterize(RASTER_DELAY)
    }

    /// Holds a long diagram's headings at the pane's edge, or lets them
    /// scroll away (REQ-073), and asks their sharp pixels once held.
    pub fn hold_headings(&mut self, on: bool) -> Task<Message> {
        if self.keep_bands == on {
            return Task::none();
        }
        self.keep_bands = on;
        self.band_top.clear();
        self.band_left.clear();
        self.rasterize(Duration::ZERO)
    }

    /// `area` with room for the headings the pane holds over it (REQ-073):
    /// as high again as the band across the top where the area lies under
    /// it, as wide again as the one down the left where it lies right of
    /// it, so the camera keeps a step out from under them. Under a
    /// sequence diagram's pills it is as high as they are.
    fn under_headings(&self, area: Rectangle) -> Rectangle {
        let Some(diagram) = self.diagram.as_ref().filter(|_| self.keep_bands) else {
            return area;
        };
        let mut room = area;
        // Where a band's heads alone are held, as pills, they take the
        // height of a name with its room, not the band's.
        let pills = diagram
            .drawn
            .map
            .heads
            .iter()
            .map(|head| head.line * 2.0)
            .fold(0.0, f32::max);
        let mut tops = 0;
        for band in &diagram.drawn.map.bands {
            let rect = band.rect;
            match band.edge {
                omaspec_uml::map::Edge::Top if area.y >= rect.y + rect.h => {
                    tops += 1;
                    let high = if tops == 1 && pills > 0.0 {
                        pills.min(rect.h)
                    } else {
                        rect.h
                    };
                    room.y -= high;
                    room.height += high;
                }
                omaspec_uml::map::Edge::Left if area.x >= rect.x + rect.w => {
                    room.x -= rect.w;
                    room.width += rect.w;
                }
                _ => {}
            }
        }
        room
    }

    /// Where the box called `name` is in the picture, a state or a
    /// container, in diagram units.
    pub(super) fn box_area(&self, name: &str) -> Option<Rectangle> {
        let map = &self.diagram.as_ref()?.drawn.map;
        let rect = map
            .states
            .iter()
            .find(|spot| spot.name == name)
            .map(|spot| spot.rect)
            .or_else(|| {
                map.places
                    .iter()
                    .find(|place| place.name == name)
                    .map(|place| place.rect)
            })?;
        Some(Rectangle::new(
            Point::new(rect.x, rect.y),
            Size::new(rect.w, rect.h),
        ))
    }

    /// Takes `view` from another pane (Compare A|B zooms both together): a
    /// fitted one fits this diagram.
    pub fn follow(&mut self, view: View) -> Task<Message> {
        let held = self.settle();
        if view.fit {
            return Task::batch([held, self.refit()]);
        }
        self.view = view;
        self.note_view();
        Task::batch([held, self.rasterize(RASTER_DELAY)])
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
        self.band_top.clear();
        self.band_left.clear();
        if !matches!(self.arrival, super::arrival::Arrival::Still) {
            self.arrive(
                super::arrival::Event::Released,
                super::arrival::Arrival::Still,
            );
        }
        self.change = Change::Edit;
    }

    pub fn diagram_size(&self) -> Size {
        self.diagram
            .as_ref()
            .map_or(Size::new(1.0, 1.0), |d| d.size)
    }

    /// How much wider than high the diagram is; none while there is none.
    pub fn shape(&self) -> Option<f32> {
        let size = self.diagram.as_ref()?.size;
        (size.height > 0.0).then(|| size.width / size.height)
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
        // Twice as sharp as the view while that is cheap: a zoom into a
        // link magnifies what lies around it up to five times before the
        // veil covers it, and a glide shows it sharp before the detail
        // comes.
        let sharper = scale * 2.0;
        let pixels = diagram.size.width * diagram.size.height * sharper * sharper;
        let overview = Plan::overview(
            diagram.size,
            if pixels <= SHARPER_PIXELS {
                sharper
            } else {
                scale
            },
        );
        let detail = Plan::detail(diagram.size, scale, visible, self.detail.asked);
        // The headings held at the edges, sharp too: the first band across
        // the top and the first down the left.
        let band = |edge: omaspec_uml::map::Edge, current: Option<Plan>| {
            let band = diagram
                .drawn
                .map
                .bands
                .iter()
                .find(|band| band.edge == edge && self.keep_bands)?;
            let rect = Rectangle::new(
                Point::new(band.rect.x, band.rect.y),
                Size::new(band.rect.w, band.rect.h),
            );
            let top = edge == omaspec_uml::map::Edge::Top;
            Plan::band(diagram.size, scale, rect, top, visible, current)
        };
        let top = band(omaspec_uml::map::Edge::Top, self.band_top.asked);
        let left = band(omaspec_uml::map::Edge::Left, self.band_left.asked);
        Task::batch([
            parse,
            self.overview.ask(overview, &tree, delay, Message::Overview),
            self.detail.ask(detail, &tree, delay, Message::Detail),
            self.band_top.ask(top, &tree, delay, Message::BandTop),
            self.band_left.ask(left, &tree, delay, Message::BandLeft),
        ])
    }
}
