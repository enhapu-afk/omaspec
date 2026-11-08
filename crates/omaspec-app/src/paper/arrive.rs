//! A new picture comes in. The one on screen stays where it was, and
//! moves with the view, until the new one is rasterized. Then the new
//! one fades in over it, or its boxes glide to their new places
//! ([`super::morph`]). A fold keeps the box it folded where it was.

use std::time::Duration;

use iced::animation::Easing;
use iced::time::Instant;
use iced::{Animation, Point, Size, Task};

use super::raster::{Plan, Raster};
use super::view::View;
use super::{Change, Diagram, Fade, Message, Paper, Previous, Status, cards, frames, morph};
use crate::worker::Answer;

/// How long a new picture takes to fade in over the one before.
const DIAGRAM_FADE: Duration = Duration::from_millis(180);
/// A fold fades longer, and eases both ways: the picture changes shape.
const FOLD_FADE: Duration = Duration::from_millis(320);
/// The same boxes glide to where the new picture put them.
const MORPH: Duration = Duration::from_millis(400);

impl Paper {
    /// Takes a render answer: shows the diagram, or keeps the last one and
    /// shows the error. Answers older than the one on screen are dropped.
    pub fn rendered(&mut self, answer: Answer) -> Task<Message> {
        let id = answer.ticket.id;
        if id <= self.shown {
            return Task::none();
        }
        self.shown = id;
        let latest = id == self.requested;
        match answer.result {
            Ok(drawn) => {
                // The lines are drawn anew: the pointer finds them again,
                // but a drag goes on, and bends on their way wait for the
                // picture that has them.
                let theirs = id > self.hand_sent_at;
                self.hand = match (self.hand.take(), &self.diagram) {
                    (Some(hand), Some(old)) => hand.redrawn(&old.drawn.map, &drawn.map, theirs),
                    _ => None,
                };
                self.grab = match (self.grab.take(), &self.diagram) {
                    (Some(grab), Some(old)) => grab.redrawn(&old.drawn.map, &drawn.map, theirs),
                    _ => None,
                };
                // A changed diagram ends its play; the same one drawn again
                // (in a new theme or font) keeps it, in the new places.
                if let Some(play) = &mut self.play {
                    let same = self.diagram.as_ref().is_some_and(|d| {
                        d.source == answer.source && d.language == answer.language
                    });
                    if same {
                        play.remap(&drawn.map);
                    } else {
                        self.play = None;
                    }
                }
                let now = Instant::now();
                let running = self.glide.filter(|glide| !glide.done(now));
                let on_screen = running.map_or(self.view, |glide| glide.at(now));
                let old_size = self.diagram.as_ref().map(|d| d.size);
                let anchor = self.take_anchor(id);
                let old_center = anchor.as_deref().and_then(|name| self.box_center(name));
                // Where the picture on screen sits now. It stays there until
                // the new one is rasterized: fitting at once would jump it.
                let held_at = match &self.previous {
                    Some(previous) if self.fresh => previous.view_at(on_screen),
                    _ => on_screen,
                };
                if !self.fresh {
                    self.previous = Some(Previous {
                        view: on_screen,
                        then: on_screen,
                        size: old_size.unwrap_or(Size::new(1.0, 1.0)),
                        pictures: [self.overview.shown.clone(), self.detail.shown.clone()]
                            .into_iter()
                            .flatten()
                            .collect(),
                        boxes: self.diagram.as_ref().map(morph::sides).unwrap_or_default(),
                        links: self.followable().to_vec(),
                        spots: self.fold_spots(),
                    });
                    self.fresh = true;
                }
                let size = drawn.tree.size();
                self.diagram = Some(Diagram {
                    size: Size::new(size.width(), size.height()),
                    pictures: cards::Pictures::of(&drawn),
                    drawn,
                    source: answer.source,
                    language: answer.language,
                });
                if latest {
                    self.status = Status::InSync;
                }
                // A zoom into a link, or out of it, owns the view.
                if self.passage.is_none() {
                    // Where the view was going. A zoom still gliding goes on
                    // there, to the same place in the new picture.
                    let mut view = self.view;
                    if view.fit {
                        view = View::fitted(self.diagram_size(), self.viewport);
                    } else if let Some(old_center) = old_center
                        && let Some(name) = anchor.as_deref()
                        && let Some(new_center) = self.box_center(name)
                    {
                        // The box stays where it was on screen.
                        view.center += new_center - old_center;
                    }
                    self.glide = match running {
                        Some(glide) if view != self.view => {
                            Some(glide.toward(view, self.viewport, now))
                        }
                        other => other,
                    };
                    self.view = view;
                }
                if let Some(previous) = self.previous.as_mut().filter(|_| self.fresh) {
                    previous.view = held_at;
                    previous.then = self.glide.map_or(self.view, |glide| glide.at(now));
                }
                // Same plans, new pixels. The overview now on its way is
                // not the one a glide in flight is travelling to.
                self.overview.forget();
                self.detail.forget();
                self.landed = false;
                let pixels = self.rasterize(Duration::ZERO);
                if self.passage.is_some() {
                    return pixels;
                }
                // A container unfolded out of sight: the view glides to it.
                match self.view_for_unfold() {
                    Some(to) => {
                        let middle =
                            Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0);
                        Task::batch([pixels, self.glide_to(to, middle)])
                    }
                    None => pixels,
                }
            }
            Err(err) => {
                // The bends sent did not draw; the old picture stays.
                if id > self.hand_sent_at && self.hand.as_ref().is_some_and(|h| h.sent) {
                    self.hand = None;
                }
                if id > self.hand_sent_at && self.grab.as_ref().is_some_and(|g| g.sent) {
                    self.grab = None;
                }
                if latest {
                    self.status = Status::Failed(err);
                }
                Task::none()
            }
        }
    }

    /// The box a fold should keep on screen, once, for the answer to
    /// request `id`. A render asked for before the fold does not draw it,
    /// and leaves the anchor to the one that does. The fold itself stays
    /// until [`Self::start_fade`] reads it.
    fn take_anchor(&mut self, id: u64) -> Option<String> {
        match &mut self.change {
            Change::Fold { anchor, since } if id > *since => anchor.take(),
            _ => None,
        }
    }

    /// Fades from the pictures before to the new diagram's overview. A
    /// detail of the diagram before goes with them; the new one comes sharp
    /// once it is rasterized. When a box moved, the boxes glide for
    /// [`MORPH`]. Otherwise a fold eases longer than an edit.
    pub(super) fn start_fade(&mut self, now: Instant) {
        // A picture asked for before the fold leaves it to the one after.
        let fold = match self.change {
            Change::Fold { since, .. } if self.shown <= since => None,
            Change::Fold { .. } => Some(true),
            Change::Edit => Some(false),
        };
        if fold.is_some() {
            self.change = Change::Edit;
        }
        let fold = fold.unwrap_or(false);
        let previous = self.previous.take();
        let seen = self.glide.map_or(self.view, |glide| glide.at(now));
        let (old, old_view, old_size, old_boxes) = match previous {
            Some(previous) => {
                let view = previous.view_at(seen);
                (previous.pictures, view, previous.size, previous.boxes)
            }
            None => (Vec::new(), self.view, self.diagram_size(), Vec::new()),
        };
        if self.detail.shown.as_ref().is_some_and(|detail| {
            old.iter()
                .any(|picture| picture.handle.id() == detail.handle.id())
        }) {
            self.detail.shown = None;
        }
        let morph = self.morph_of(&old, &old_boxes, old_size);
        let (duration, easing) = if morph.is_some() {
            (MORPH, Easing::EaseInOut)
        } else if fold {
            (FOLD_FADE, Easing::EaseInOut)
        } else {
            (DIAGRAM_FADE, Easing::EaseOut)
        };
        self.fade = Some(Fade {
            old,
            old_view,
            old_size,
            morph,
            next: self.overview.shown.clone(),
            new_view: self.view,
            new_size: self.diagram_size(),
            shown: Animation::new(false)
                .duration(duration)
                .easing(easing)
                .go(true, now),
        });
    }

    /// A pan or a zoom while a glide runs follows the pointer. A stepped
    /// zoom still on its way is sampled from that glide, so it is left
    /// alone until it arrives.
    pub(super) fn note_view(&mut self) {
        if self.glide.is_some() {
            return;
        }
        self.pin_morph(self.view);
    }

    fn pin_morph(&mut self, view: View) {
        if let Some(fade) = self.fade.as_mut().filter(|fade| fade.morph.is_some()) {
            fade.new_view = view;
        }
    }

    /// The glide when the new picture moved a box that was already there.
    /// A passage, a box held by the pointer, or a picture with nothing on
    /// screen before it keeps the short fade.
    fn morph_of(
        &self,
        old: &[Raster],
        old_boxes: &[morph::Side],
        old_size: Size,
    ) -> Option<morph::Morph> {
        if old.is_empty() || self.passage.is_some() || self.grab.is_some() {
            return None;
        }
        let diagram = self.diagram.as_ref()?;
        morph::plan(old_boxes, &morph::sides(diagram), old_size, diagram.size)
    }

    /// Whether pixels that come now wait: the view glides, and a picture
    /// is on screen meanwhile.
    pub(super) fn holds(&self) -> bool {
        self.overview.shown.is_some() && self.glide.is_some_and(|g| !g.done(Instant::now()))
    }

    /// Shows the pixels that came while the view glided.
    pub(super) fn show_held(&mut self) -> Task<Message> {
        let overview = self
            .held_overview
            .take()
            .map(|(seq, raster)| self.show_overview(seq, raster));
        if let Some((seq, raster)) = self.held_detail.take() {
            self.show_detail(seq, raster);
        }
        overview.unwrap_or_else(Task::none)
    }

    pub(super) fn show_overview(&mut self, seq: u64, raster: Option<Raster>) -> Task<Message> {
        let failed = raster.is_none() && self.overview.is_latest(seq);
        let shown = self.overview.receive(seq, raster);
        if shown {
            frames::note(|| pixels("overview", self.overview.asked));
        }
        if failed {
            return self.no_overview();
        }
        if shown && self.fresh {
            self.landed = true;
            // A glide keeps its own picture until it ends. A short fade
            // still gives way at once, as it always has.
            let morphing = self.fade.as_ref().is_some_and(|fade| fade.morph.is_some());
            if !morphing {
                self.fresh = false;
                self.landed = false;
                self.start_fade(Instant::now());
            }
        }
        self.resume_outward()
    }

    /// The overview asked for last could not be drawn. What waited for it
    /// waits no longer: the picture still on screen gives way to the new
    /// diagram's view, and a zoom out of a link lifts its veil where it was
    /// going.
    fn no_overview(&mut self) -> Task<Message> {
        if self.fresh {
            self.fresh = false;
            self.landed = false;
            self.previous = None;
        }
        self.stop_waiting();
        Task::none()
    }

    pub(super) fn show_detail(&mut self, seq: u64, raster: Option<Raster>) {
        if self.detail.receive(seq, raster) {
            frames::note(|| pixels("detail", self.detail.asked));
        }
    }

    /// The middle of the box `name`, in the diagram's units.
    pub(super) fn box_center(&self, name: &str) -> Option<Point> {
        let place = self
            .diagram
            .as_ref()?
            .drawn
            .map
            .places
            .iter()
            .find(|place| place.name == name)?;
        Some(Point::new(
            place.rect.x + place.rect.w / 2.0,
            place.rect.y + place.rect.h / 2.0,
        ))
    }

    /// The view the picture on screen is still drawn at, while a new
    /// diagram waits for its first raster. Fitting has already moved
    /// [`Self::view`]; drawing with that would jump the old picture.
    pub(super) fn held_picture(&self) -> Option<&Previous> {
        if !self.fresh || self.fade.is_some() || self.passage.is_some() {
            return None;
        }
        self.previous
            .as_ref()
            .filter(|previous| !previous.pictures.is_empty())
    }
}

/// New pixels of a layer on screen, for [`frames::note`].
fn pixels(layer: &str, plan: Option<Plan>) -> String {
    match plan {
        Some(plan) => format!(
            "{layer} {}×{} px shown",
            plan.part.width(),
            plan.part.height()
        ),
        None => format!("{layer} shown"),
    }
}
