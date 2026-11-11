//! Motion in the editor: the caret glides to where it goes, and the
//! completion list arrives instead of standing there at once. Both are
//! short, both go by the frame times iced hands the widget, and both are
//! off when the host says so.
//!
//! Omaspec addition to scrive 0.3.0.

use std::time::{Duration, Instant};

/// How long the caret takes to where it goes.
pub const CARET: Duration = Duration::from_millis(90);
/// How long the completion list takes to arrive.
pub const ARRIVE: Duration = Duration::from_millis(110);
/// How far below its place the list starts, in pixels.
pub const RISE: f32 = 6.0;

/// Fast at first, slow into its place.
#[must_use]
pub fn ease(t: f32) -> f32 {
    let left = 1.0 - t.clamp(0.0, 1.0);
    1.0 - left * left * left
}

/// How far a motion that began at `start` and takes `long` has come at
/// `now`, from 0 to 1.
fn share(start: Instant, long: Duration, now: Instant) -> f32 {
    (now.saturating_duration_since(start).as_secs_f32() / long.as_secs_f32()).min(1.0)
}

/// The caret on its way from one place to another.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Glide {
    from: (f32, f32),
    to: (f32, f32),
    /// `None` until the caret was drawn once: it starts in its place.
    start: Option<Instant>,
    /// How far the text was scrolled when the caret was last drawn: a caret
    /// the scrolling moved does not glide after its text.
    scrolled: (f64, f32),
}

impl Glide {
    /// Where to draw the caret at `now`, which belongs at `target` with
    /// the text scrolled by `scrolled`. The caret glides there from where
    /// it was drawn last; it is there at once the first time, after a
    /// scroll, and with `on` false.
    pub fn at(&mut self, target: (f32, f32), scrolled: (f64, f32), now: Instant, on: bool) -> (f32, f32) {
        let Some(start) = self.start else {
            return self.stand(target, scrolled, now);
        };
        if !on || scrolled != self.scrolled {
            return self.stand(target, scrolled, now);
        }
        if target != self.to {
            self.from = self.place(start, now);
            self.to = target;
            self.start = Some(now);
            return self.from;
        }
        self.place(start, now)
    }

    /// Whether the caret is still on its way at `now`.
    #[must_use]
    pub fn running(&self, now: Instant) -> bool {
        self.start.is_some_and(|start| self.from != self.to && share(start, CARET, now) < 1.0)
    }

    fn stand(&mut self, target: (f32, f32), scrolled: (f64, f32), now: Instant) -> (f32, f32) {
        *self = Glide { from: target, to: target, start: Some(now), scrolled };
        target
    }

    fn place(&self, start: Instant, now: Instant) -> (f32, f32) {
        let t = ease(share(start, CARET, now));
        (
            self.from.0 + (self.to.0 - self.from.0) * t,
            self.from.1 + (self.to.1 - self.from.1) * t,
        )
    }
}

/// The completion list arriving.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Arrive {
    /// When the list opened; `None` while it is closed.
    since: Option<Instant>,
}

impl Arrive {
    /// How much of the list is there at `now`, from 0 to 1: it opens with
    /// the first call after [`Self::closed`]. With `on` false it is there
    /// at once.
    pub fn shown(&mut self, now: Instant, on: bool) -> f32 {
        let since = *self.since.get_or_insert(now);
        if on {
            ease(share(since, ARRIVE, now))
        } else {
            1.0
        }
    }

    /// The list is closed: the next one arrives anew.
    pub fn closed(&mut self) {
        self.since = None;
    }

    /// Whether an open list has arrived at `now`; a closed one has not.
    #[must_use]
    pub fn done(&self, now: Instant) -> bool {
        self.since.is_some_and(|since| share(since, ARRIVE, now) >= 1.0)
    }
}
