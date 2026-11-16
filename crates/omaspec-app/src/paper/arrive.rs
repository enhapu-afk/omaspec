//! A new picture comes in. The one on screen stays where it was, and
//! moves with the view, until the new one is rasterized. Then the new
//! one fades in over it, or its boxes glide to their new places
//! ([`super::morph`]). A fold keeps the box it folded where it was.

use std::sync::Arc;
use std::time::Duration;

use iced::animation::Easing;
use iced::time::Instant;
use iced::{Animation, Point, Size, Task};

use super::arrival::{Arrival, Event, Next};
use super::raster::{Plan, Raster};
use super::sheet::Sheet;
use super::view::View;
use super::{Change, Diagram, Fade, Message, Paper, Previous, Status, cards, frames, morph, wires};
use crate::worker::Answer;

/// How long a new picture takes to fade in over the one before.
const DIAGRAM_FADE: Duration = Duration::from_millis(180);
/// A fold fades longer, and eases both ways: the picture changes shape.
const FOLD_FADE: Duration = Duration::from_millis(320);
/// The same boxes glide to where the new picture put them.
const MORPH: Duration = Duration::from_millis(400);
/// A diagram of more boxes than this fades from one layout to the next
/// instead of gliding box by box. A frame's work grows faster than the
/// boxes do, and has to stay well under the 16.7 ms of a 60 Hz screen.
/// Counted in the places of its map, the same for the picture before and
/// the one after ([`too_many`]).
pub(super) const MOST_GLIDING: usize = 300;

/// Whether the picture of `map` has more boxes than glide in time.
pub(super) fn too_many(map: &omaspec_uml::map::Map) -> bool {
    map.places.len() > MOST_GLIDING
}

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
                let now = crate::clock::now();
                let running = self.glide.filter(|glide| !glide.done(now));
                let on_screen = running.map_or(self.view, |glide| glide.at(now));
                let old_size = self.diagram.as_ref().map(|d| d.size);
                let anchor = self.take_anchor(id);
                let old_center = anchor.as_deref().and_then(|name| self.box_center(name));
                // Where the picture on screen sits now. It stays there until
                // the new one is rasterized: fitting at once would jump it.
                let held_at = self
                    .arrival
                    .previous()
                    .map_or(on_screen, |previous| previous.view_at(on_screen));
                // The picture on screen, unless one before it is still to
                // be faded from: a short fade on screen gives way at once,
                // its new picture held; a glide goes on to its end.
                let previous = (!self.arrival.coming()).then(|| Previous {
                    view: on_screen,
                    then: on_screen,
                    size: old_size.unwrap_or(Size::new(1.0, 1.0)),
                    pictures: [self.overview.shown.clone(), self.detail.shown.clone()]
                        .into_iter()
                        .flatten()
                        .collect(),
                    // Nothing of a picture that will not glide is
                    // read for a glide.
                    boxes: self
                        .diagram
                        .as_ref()
                        .filter(|diagram| {
                            Sheet::of(&diagram.drawn).is_some() && !too_many(&diagram.drawn.map)
                        })
                        .map(morph::sides)
                        .unwrap_or_default(),
                    links: self.followable().to_vec(),
                    spots: self.fold_spots(),
                    sheet: self.diagram.as_ref().and_then(|d| Sheet::of(&d.drawn)),
                    plan: self.overview.asked,
                    map: self
                        .diagram
                        .as_ref()
                        .map(|d| Arc::clone(&d.drawn.map))
                        .unwrap_or_default(),
                });
                let next = match self.take_arrival() {
                    Arrival::Still | Arrival::Fading(_) => Arrival::Coming(previous),
                    Arrival::Coming(before) => Arrival::Coming(before),
                    Arrival::Sheets(before) => Arrival::Coming(Some(before)),
                    Arrival::Gliding(fade, Next::Alone) => {
                        Arrival::Gliding(fade, Next::Following(previous))
                    }
                    Arrival::Gliding(fade, Next::Following(before) | Next::Waiting(before)) => {
                        Arrival::Gliding(fade, Next::Following(before))
                    }
                };
                self.arrive(Event::Drawn, next);
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
                if let Some(previous) = self.arrival.previous_mut() {
                    previous.view = held_at;
                    previous.then = self.glide.map_or(self.view, |glide| glide.at(now));
                }
                // Same plans, new pixels. The overview now on its way is
                // not the one a glide in flight is travelling to.
                self.overview.forget();
                self.detail.forget();
                // Sheets of parts belong to one pair of diagrams.
                self.bare_old.clear();
                self.bare_new.clear();
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
    /// until [`Self::start`] reads it.
    fn take_anchor(&mut self, id: u64) -> Option<String> {
        match &mut self.change {
            Change::Fold { anchor, since } if id > *since => anchor.take(),
            _ => None,
        }
    }

    /// Fades from `previous`, the pictures before, to the new diagram's
    /// overview, on `event`. A detail of the diagram before goes with them;
    /// the new one comes sharp once it is rasterized. When a box moved and
    /// both pictures mark their parts, the boxes glide for [`MORPH`], each
    /// from a picture of its own, and the lines with them, once the sheets
    /// of the two pictures are drawn: until then the old picture stays, and
    /// this is called again when they come (docs/adr/0029). Otherwise, or
    /// when a sheet could not be drawn (`failed`), the pictures fade, a fold
    /// longer than an edit.
    fn start(
        &mut self,
        event: Event,
        previous: Option<Previous>,
        failed: bool,
        now: Instant,
    ) -> Task<Message> {
        let parts = self
            .diagram
            .as_ref()
            .is_some_and(|d| Sheet::of(&d.drawn).is_some());
        let mut morph = match &previous {
            Some(previous) if parts && previous.sheet.is_some() => self
                .morph_of(
                    &previous.pictures,
                    &previous.boxes,
                    previous.size,
                    &previous.map,
                )
                .map(|mut morph| {
                    if let Some(diagram) = &self.diagram {
                        morph::gather(&mut morph, &previous.map, &diagram.drawn.map);
                    }
                    morph
                }),
            _ => None,
        };
        let sheets_missing = self.bare_old.shown.is_none() || self.bare_new.shown.is_none();
        let asked = match &previous {
            Some(before) if morph.is_some() && !failed && sheets_missing => self.ask_bare(before),
            _ => None,
        };
        if let Some(asked) = asked
            && let Some(previous) = previous
        {
            self.arrive(event, Arrival::Sheets(previous));
            return asked;
        }
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
        let seen = self.glide.map_or(self.view, |glide| glide.at(now));
        let (old, old_view, old_size, old_map) = match previous {
            Some(previous) => {
                let view = previous.view_at(seen);
                (previous.pictures, view, previous.size, Some(previous.map))
            }
            None => (Vec::new(), self.view, self.diagram_size(), None),
        };
        if self.detail.shown.as_ref().is_some_and(|detail| {
            old.iter()
                .any(|picture| picture.handle.id() == detail.handle.id())
        }) {
            self.detail.shown = None;
        }
        // The two sheets of parts the boxes glide from; without them,
        // the pictures fade.
        let bare = match (
            &morph,
            self.bare_old.shown.take(),
            self.bare_new.shown.take(),
        ) {
            (Some(_), Some(old), Some(new)) => Some((old, new)),
            _ => None,
        };
        if bare.is_none() {
            morph = None;
        }
        // A fold that fades, of too many boxes or of a picture without
        // parts or a sheet: its fade stands in for the glide.
        let stands_in = fold && morph.is_none();
        let (duration, easing) = if morph.is_some() {
            (MORPH, Easing::EaseInOut)
        } else if fold {
            (FOLD_FADE, Easing::EaseInOut)
        } else {
            (DIAGRAM_FADE, Easing::EaseOut)
        };
        let lines = match (&morph, &bare, &old_map, &self.diagram) {
            (Some(morph), Some(_), Some(old_map), Some(diagram)) => {
                Some(wires::plan(old_map, &diagram.drawn.map, &morph.moves))
            }
            _ => None,
        };
        self.bare_old.clear();
        self.bare_new.clear();
        if let (Some(morph), Some((old, new)), Some(old_map), Some(diagram)) =
            (morph.as_mut(), &bare, &old_map, &self.diagram)
        {
            let maps = [&**old_map, &*diagram.drawn.map];
            morph::prepare(morph, (old, new), maps, lines.as_ref());
        }
        let fade = Fade {
            old,
            old_view,
            old_size,
            morph,
            new_view: self.view,
            new_size: self.diagram_size(),
            lines,
            bare,
            stands_in,
            shown: Animation::new(false)
                .duration(duration)
                .easing(easing)
                .go(true, now),
        };
        let to = if fade.morph.is_some() {
            Arrival::Gliding(fade, Next::Alone)
        } else {
            Arrival::Fading(fade)
        };
        self.arrive(event, to);
        Task::none()
    }

    /// Asks for the sheets of the old picture and of the new one, at the
    /// plans their overviews were drawn to: each picture's ground, and
    /// each of its parts on its own. `None` when one of them has no
    /// sheet, so they fade.
    fn ask_bare(&mut self, previous: &Previous) -> Option<Task<Message>> {
        let (old_sheet, old_plan) = (previous.sheet.clone()?, previous.plan?);
        let new_sheet = Sheet::of(&self.diagram.as_ref()?.drawn)?;
        let new_plan = self.overview.asked?;
        Some(Task::batch([
            self.bare_old
                .ask_sheet(Some(old_plan), old_sheet, Duration::ZERO, Message::BareOld),
            self.bare_new
                .ask_sheet(Some(new_plan), new_sheet, Duration::ZERO, Message::BareNew),
        ]))
    }

    /// A sheet came, or could not be drawn: with both in, or one failed,
    /// the fade that waited for them starts.
    pub(super) fn bare_came(
        &mut self,
        seq: u64,
        raster: Option<Raster>,
        old: bool,
    ) -> Task<Message> {
        let layer = if old {
            &mut self.bare_old
        } else {
            &mut self.bare_new
        };
        let failed = raster.is_none() && layer.is_latest(seq);
        let came = layer.receive(seq, raster);
        if !(came || failed) {
            return Task::none();
        }
        match self.take_arrival() {
            Arrival::Sheets(previous) => {
                let both = self.bare_old.shown.is_some() && self.bare_new.shown.is_some();
                let event = if failed { Event::NoSheet } else { Event::Sheet };
                if both || failed {
                    return self.start(event, Some(previous), failed, crate::clock::now());
                }
                self.arrive(event, Arrival::Sheets(previous));
            }
            other => self.arrival = other,
        }
        Task::none()
    }

    /// The fade or the glide on screen has ended. A diagram that came
    /// meanwhile comes now: its overview, once that is in.
    pub(super) fn faded(&mut self, now: Instant) -> Task<Message> {
        match self.take_arrival() {
            Arrival::Fading(_) | Arrival::Gliding(_, Next::Alone) => {
                self.arrive(Event::Faded, Arrival::Still);
            }
            Arrival::Gliding(_, Next::Following(previous)) => {
                self.arrive(Event::Faded, Arrival::Coming(previous));
            }
            Arrival::Gliding(_, Next::Waiting(previous)) => {
                return self.start(Event::Faded, previous, false, now);
            }
            other => self.arrival = other,
        }
        Task::none()
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
        if let Arrival::Gliding(fade, _) = &mut self.arrival {
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
        old_map: &omaspec_uml::map::Map,
    ) -> Option<morph::Morph> {
        if old.is_empty() || self.passage.is_some() || self.grab.is_some() {
            return None;
        }
        let diagram = self.diagram.as_ref()?;
        // Each box of a glide is a picture of its own in every frame,
        // drawn from its part of the two sheets it travels between. A
        // tree of 2,800 boxes took 90 ms a frame that way, nine frames a
        // second; one picture fading into the other stays at sixty. So
        // does the way from an older drawing a comparison shows
        // ([`Self::glide_from`]).
        if too_many(old_map) || too_many(&diagram.drawn.map) {
            return None;
        }
        // A fold of a tree glides whatever else moved: its boxes go into
        // the box above them.
        let folding = matches!(self.change, Change::Fold { .. })
            && diagram.drawn.map.places.iter().any(|place| place.fixed);
        morph::plan_with(
            old_boxes,
            &morph::sides(diagram),
            old_size,
            diagram.size,
            folding,
        )
    }

    /// Whether pixels that come now wait: the view glides, and a picture
    /// is on screen meanwhile.
    pub(super) fn holds(&self) -> bool {
        self.overview.shown.is_some() && self.glide.is_some_and(|g| !g.done(crate::clock::now()))
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
            self.back = None;
            return self.no_overview();
        }
        if !shown || !self.arrival.coming() {
            return self.resume_outward();
        }
        let now = crate::clock::now();
        let fade = match self.take_arrival() {
            Arrival::Coming(previous) => self.start(Event::Overview, previous, false, now),
            Arrival::Sheets(previous) => self.start(Event::Overview, Some(previous), false, now),
            // A glide keeps its own picture until it ends.
            Arrival::Gliding(fade, Next::Following(previous)) => {
                self.arrive(
                    Event::Overview,
                    Arrival::Gliding(fade, Next::Waiting(previous)),
                );
                return self.resume_outward();
            }
            other => {
                self.arrival = other;
                return self.resume_outward();
            }
        };
        // The older drawing is on screen: now the way to the one it was
        // shown for.
        let back = match self.back.take() {
            Some(mut back) => {
                self.requested += 1;
                back.ticket.id = self.requested;
                self.rendered(back)
            }
            None => Task::none(),
        };
        Task::batch([fade, self.resume_outward(), back])
    }

    /// Shows the way from `old`, an older drawing of this diagram (as B of
    /// a comparison), to the diagram on screen: the old one stands as it
    /// is, then its boxes and lines glide to their new places, what came
    /// fades in and what went fades out, as a changed layout glides after
    /// an edit. A diagram of too many boxes to glide ([`MOST_GLIDING`])
    /// fades from the old drawing to the new one, as it does after an
    /// edit.
    pub fn glide_from(
        &mut self,
        old: crate::worker::Drawn,
        source: Arc<str>,
        language: omaspec_core::Language,
    ) -> Task<Message> {
        let Some(now) = &self.diagram else {
            return Task::none();
        };
        let ticket = |id: u64| crate::worker::Ticket { doc: 0, id };
        self.back = Some(Answer {
            ticket: ticket(0),
            source: Arc::clone(&now.source),
            language: now.language,
            result: Ok(now.drawn.clone()),
        });
        self.requested += 1;
        let task = self.rendered(Answer {
            ticket: ticket(self.requested),
            source,
            language,
            result: Ok(old),
        });
        // Nothing glides to the old one: it is where the way starts.
        self.arrival.forget_previous();
        task
    }

    /// The diagram on screen as it was drawn, with its source, for
    /// [`Self::glide_from`] on another pane.
    pub fn drawn_now(&self) -> Option<(crate::worker::Drawn, Arc<str>, omaspec_core::Language)> {
        let diagram = self.diagram.as_ref()?;
        Some((
            diagram.drawn.clone(),
            Arc::clone(&diagram.source),
            diagram.language,
        ))
    }

    /// The overview asked for last could not be drawn. What waited for it
    /// waits no longer: the picture still on screen gives way to the new
    /// diagram's view, and a zoom out of a link lifts its veil where it was
    /// going.
    fn no_overview(&mut self) -> Task<Message> {
        match self.take_arrival() {
            Arrival::Coming(_) | Arrival::Sheets(_) => {
                self.arrive(Event::NoOverview, Arrival::Still);
            }
            Arrival::Gliding(fade, Next::Following(_) | Next::Waiting(_)) => {
                self.arrive(Event::NoOverview, Arrival::Gliding(fade, Next::Alone));
            }
            other => self.arrival = other,
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
        if self.passage.is_some() {
            return None;
        }
        self.arrival
            .held()
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
