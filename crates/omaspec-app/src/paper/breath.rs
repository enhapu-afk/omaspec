//! The light on the name under the caret.
//!
//! It comes in by spreading from a point. On the way to another place it
//! shrinks back to that point, travels, and spreads over the new one. A
//! box takes the light over its shape. Words, a message or a label on a
//! line, take a quiet wash.

use std::hash::{Hash, Hasher};
use std::time::Duration;

use iced::time::Instant;
use iced::{Point, Rectangle, Size};
use omaspec_core::locate;
use omaspec_render::pick;

use super::{AimMemo, Paper};

/// How long a light takes to shrink, travel and spread.
pub(super) const MOVE: Duration = Duration::from_millis(420);
/// How long a light takes to spread where it appears.
pub(super) const ARRIVE: Duration = Duration::from_millis(220);
/// How long a light takes to shrink away.
pub(super) const LEAVE: Duration = Duration::from_millis(160);

/// Share of [`MOVE`] spent shrinking to a point, then travelling.
const COLLAPSE_END: f32 = 110.0 / 420.0;
const TRAVEL_END: f32 = 280.0 / 420.0;

/// What the light spreads over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mark {
    /// A state, an activity, a participant, or any other box.
    Shape,
    /// A message, or the words on a line.
    Words,
}

/// Where the light is going.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Aim {
    pub rect: Rectangle,
    pub mark: Mark,
    /// The letters' color. A shape leaves this unused.
    pub ink: [f32; 3],
    /// The fill under the letters, when the picture paints one.
    pub ground: Option<[f32; 3]>,
}

impl Aim {
    pub(super) fn shape(rect: Rectangle) -> Self {
        Self {
            rect,
            mark: Mark::Shape,
            ink: [0.0, 0.0, 0.0],
            ground: None,
        }
    }

    pub(super) fn words(rect: Rectangle, ink: [f32; 3], ground: Option<[f32; 3]>) -> Self {
        Self {
            rect,
            mark: Mark::Words,
            ink,
            ground,
        }
    }
}

/// The light as one frame draws it. `spread` is 0 at the travelling point
/// and 1 when the mark has its full size.
#[derive(Clone, Copy, Debug)]
pub(super) struct Shown {
    pub rect: Rectangle,
    pub alpha: f32,
    pub spread: f32,
    pub mark: Mark,
    pub ink: [f32; 3],
    pub ground: Option<[f32; 3]>,
}

/// The light on its way from one place and strength to another.
#[derive(Clone, Copy, Debug)]
pub(super) struct Breath {
    pub from: Rectangle,
    pub to: Rectangle,
    from_mark: Mark,
    to_mark: Mark,
    from_ink: [f32; 3],
    to_ink: [f32; 3],
    from_ground: Option<[f32; 3]>,
    to_ground: Option<[f32; 3]>,
    from_spread: f32,
    pub start: Instant,
    pub from_a: f32,
    to_a: f32,
    /// Shrinks to a point and travels. A light that only comes or goes
    /// spreads or shrinks where it is.
    pub travels: bool,
}

impl Breath {
    /// Moves the light to `target`, or fades it out. The same place does
    /// not restart the glide. From nothing, or from a light that has not
    /// appeared yet, it spreads where it is going. From a light already
    /// showing it shrinks, travels and spreads.
    pub(super) fn go(current: Option<Self>, target: Option<Aim>, now: Instant) -> Option<Self> {
        match target {
            Some(to) => {
                let Some(breath) = current else {
                    return Some(Self::arrive(to, now));
                };
                if breath.to_a == 1.0 && breath.to_mark == to.mark && same_place(breath.to, to.rect)
                {
                    // The same place in a new theme keeps its motion and
                    // takes the new letters.
                    return Some(Self {
                        from_ink: to.ink,
                        to_ink: to.ink,
                        from_ground: to.ground,
                        to_ground: to.ground,
                        ..breath
                    });
                }
                let shown = breath.at(now);
                if shown.alpha > 0.05 {
                    Some(Self::travel(shown, to, now))
                } else {
                    Some(Self::arrive(to, now))
                }
            }
            None => {
                let breath = current?;
                if breath.to_a == 0.0 {
                    return (!breath.settled(now)).then_some(breath);
                }
                let shown = breath.at(now);
                (shown.alpha > 0.01).then(|| Self::leave(shown, now))
            }
        }
    }

    /// Where it shows at `now`.
    pub(super) fn at(self, now: Instant) -> Shown {
        let t = self.fraction(now);
        if t >= 1.0 {
            return if self.to_a == 0.0 {
                self.show(self.from, 0.0, 0.0, self.from_mark, true)
            } else {
                self.show(self.to, self.to_a, 1.0, self.to_mark, false)
            };
        }
        if self.travels {
            self.on_the_way(t)
        } else if self.to_a == 0.0 {
            let k = ease_in_out(t);
            self.show(
                self.from,
                self.from_a * (1.0 - k),
                self.from_spread * (1.0 - k),
                self.from_mark,
                true,
            )
        } else {
            // Quick at first, so the point is visible at once and then opens.
            let k = ease_out(t);
            self.show(
                self.to,
                self.from_a + (self.to_a - self.from_a) * k,
                k,
                self.to_mark,
                false,
            )
        }
    }

    fn show(self, rect: Rectangle, alpha: f32, spread: f32, mark: Mark, from_side: bool) -> Shown {
        let (ink, ground) = if from_side {
            (self.from_ink, self.from_ground)
        } else {
            (self.to_ink, self.to_ground)
        };
        Shown {
            rect,
            alpha,
            spread,
            mark,
            ink,
            ground,
        }
    }

    pub(super) fn settled(self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= self.length()
    }

    /// The rectangle it is spreading toward, while it is coming in or holding.
    #[cfg(test)]
    pub(super) fn target(self) -> Option<Rectangle> {
        (self.to_a > 0.0).then_some(self.to)
    }

    fn arrive(to: Aim, now: Instant) -> Self {
        Self {
            from: to.rect,
            to: to.rect,
            from_mark: to.mark,
            to_mark: to.mark,
            from_ink: to.ink,
            to_ink: to.ink,
            from_ground: to.ground,
            to_ground: to.ground,
            from_spread: 0.0,
            start: now,
            from_a: 0.0,
            to_a: 1.0,
            travels: false,
        }
    }

    fn travel(shown: Shown, to: Aim, now: Instant) -> Self {
        Self {
            from: shown.rect,
            to: to.rect,
            from_mark: shown.mark,
            to_mark: to.mark,
            from_ink: shown.ink,
            to_ink: to.ink,
            from_ground: shown.ground,
            to_ground: to.ground,
            from_spread: shown.spread,
            start: now,
            from_a: shown.alpha,
            to_a: 1.0,
            travels: true,
        }
    }

    fn leave(shown: Shown, now: Instant) -> Self {
        Self {
            from: shown.rect,
            to: shown.rect,
            from_mark: shown.mark,
            to_mark: shown.mark,
            from_ink: shown.ink,
            to_ink: shown.ink,
            from_ground: shown.ground,
            to_ground: shown.ground,
            from_spread: shown.spread,
            start: now,
            from_a: shown.alpha,
            to_a: 0.0,
            travels: false,
        }
    }

    fn on_the_way(self, t: f32) -> Shown {
        if t < COLLAPSE_END {
            let k = ease_in_out(t / COLLAPSE_END);
            self.show(
                self.from,
                self.from_a,
                self.from_spread * (1.0 - k),
                self.from_mark,
                true,
            )
        } else if t < TRAVEL_END {
            let k = ease_in_out((t - COLLAPSE_END) / (TRAVEL_END - COLLAPSE_END));
            let center = mix_point(self.from.center(), self.to.center(), k);
            self.show(
                Rectangle::new(center, iced::Size::ZERO),
                1.0,
                0.0,
                self.to_mark,
                false,
            )
        } else {
            let k = ease_in_out((t - TRAVEL_END) / (1.0 - TRAVEL_END));
            self.show(self.to, 1.0, k, self.to_mark, false)
        }
    }

    fn fraction(self, now: Instant) -> f32 {
        now.saturating_duration_since(self.start).as_secs_f32() / self.length().as_secs_f32()
    }

    fn length(self) -> Duration {
        if self.travels {
            MOVE
        } else if self.to_a == 0.0 {
            LEAVE
        } else {
            ARRIVE
        }
    }
}

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn mix_point(a: Point, b: Point, t: f32) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

/// The same place, within half a unit: not worth a new glide.
fn same_place(a: Rectangle, b: Rectangle) -> bool {
    let close = |p: f32, q: f32| (p - q).abs() < 0.5;
    close(a.x, b.x) && close(a.y, b.y) && close(a.width, b.width) && close(a.height, b.height)
}

impl Paper {
    /// Lights the place on the sheet that the label at `row`/`col` draws,
    /// or lets the light go out. `row` is a zero-based line and `col` a
    /// byte on that line. Nothing lights while a play runs.
    pub fn follow_source(&mut self, source: &str, row: u32, col: u32) {
        if self.playing() {
            self.set_breath(None);
            return;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        let source_hash = hasher.finish();
        if let Some(memo) = &self.aim
            && memo.source == source_hash
            && memo.shown == self.shown
            && memo.row == row
            && memo.col == col
        {
            if !self.found {
                self.set_breath(memo.place);
            }
            return;
        }
        self.found = false;
        let place = self.aim_at(
            source,
            usize::try_from(row).unwrap_or(usize::MAX),
            usize::try_from(col).unwrap_or(usize::MAX),
        );
        self.aim = Some(AimMemo {
            source: source_hash,
            shown: self.shown,
            row,
            col,
            place,
        });
        self.set_breath(place);
    }

    /// Whether the light on the caret's name is still on its way.
    pub fn breath_moving(&self, now: Instant) -> bool {
        self.breath.is_some_and(|breath| !breath.settled(now))
    }

    /// The rectangle the light is spreading toward, in diagram units, while
    /// it is coming in or holding.
    #[cfg(test)]
    pub fn breath_target(&self) -> Option<Rectangle> {
        self.breath.and_then(|breath| breath.target())
    }

    /// The box of the label at `row`/`col`: the smallest shape around that
    /// occurrence. A comment line draws nothing. When the sheet draws fewer
    /// copies than the source mentions (a lifeline's head and its foot), the
    /// later mention keeps the last copy.
    #[cfg(test)]
    pub(super) fn place_of(&self, source: &str, row: usize, col: usize) -> Option<Rectangle> {
        self.written_place_of(source, row, col)
            .or_else(|| self.unwritten(source, row, col))
            .or_else(|| self.shape_of_row(source, row, col))
    }

    #[cfg(test)]
    fn written_place_of(&self, source: &str, row: usize, col: usize) -> Option<Rectangle> {
        let tree = self.shown_tree()?;
        let texts = pick::texts(tree);
        let (text, nth) = locate::aimed(source, row, col, texts.iter().map(String::as_str))?;
        let copies = texts.iter().filter(|drawn| drawn.as_str() == text).count();
        if copies == 0 {
            return None;
        }
        let nth = self.copy_of(source, &text, nth, (row, col)).min(copies - 1);
        let (x, y, w, h) = pick::spot(tree, &text, nth)?;
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        Some(Rectangle::new(Point::new(x, y), Size::new(w, h)))
    }

    /// The drawn label at `row`/`col`, and where it sits: on the box it
    /// names, or as words of their own.
    pub(super) fn seat_at(
        &self,
        source: &str,
        row: usize,
        col: usize,
    ) -> Option<(String, pick::Seat)> {
        let tree = self.shown_tree()?;
        let texts = pick::texts(tree);
        let (text, nth) = locate::aimed(source, row, col, texts.iter().map(String::as_str))?;
        let copies = texts.iter().filter(|drawn| drawn.as_str() == text).count();
        if copies == 0 {
            return None;
        }
        let nth = self.copy_of(source, &text, nth, (row, col)).min(copies - 1);
        let seat = pick::seat(tree, &text, nth)?;
        let (_, _, w, h) = seat.bounds();
        (w > 0.0 && h > 0.0).then_some((text, seat))
    }

    /// The light for the label at `row`/`col`: the box around it, or the
    /// words themselves when they sit on a line.
    pub(super) fn aim_at(&self, source: &str, row: usize, col: usize) -> Option<Aim> {
        let Some((_, seat)) = self.seat_at(source, row, col) else {
            return self
                .unwritten(source, row, col)
                .or_else(|| self.shape_of_row(source, row, col))
                .map(Aim::shape);
        };
        let (x, y, w, h) = seat.bounds();
        let rect = Rectangle::new(Point::new(x, y), Size::new(w, h));
        Some(match seat {
            pick::Seat::Shape { .. } => Aim::shape(rect),
            pick::Seat::Words { ink, ground, .. } => Aim::words(rect, ink, ground),
        })
    }

    /// The shape without words the zero-based line `row` draws, for the
    /// caret on the line's first word where no words under it are found
    /// ([`omaspec_uml::map::Map::shapes`]): the circle before a class's
    /// name for its `interface`. Only on the picture the map was laid for.
    fn shape_of_row(&self, source: &str, row: usize, col: usize) -> Option<Rectangle> {
        let line = source.split('\n').nth(row)?;
        let start = line.len() - line.trim_start().len();
        let end = line[start..]
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .map_or(line.len(), |at| start + at);
        if !(start..=end).contains(&col) || start == end {
            return None;
        }
        let diagram = self.diagram.as_ref()?;
        let tree = self.shown_tree()?;
        if !std::sync::Arc::ptr_eq(tree, &diagram.drawn.tree) {
            return None;
        }
        let shape = diagram
            .drawn
            .map
            .shapes
            .iter()
            .find(|shape| shape.line == row)?;
        let r = shape.rect;
        Some(Rectangle::new(Point::new(r.x, r.y), Size::new(r.w, r.h)))
    }

    /// A shape of the map whose name the picture does not write (a
    /// choice's diamond, [`omaspec_uml::map::Map::quiet`]), named by the
    /// word under the caret, or in a `<<mark>>` by the name before it. A
    /// comment line names none. Only on the picture the map was laid for.
    fn unwritten(&self, source: &str, row: usize, col: usize) -> Option<Rectangle> {
        let diagram = self.diagram.as_ref()?;
        let tree = self.shown_tree()?;
        if !std::sync::Arc::ptr_eq(tree, &diagram.drawn.tree) || diagram.drawn.map.quiet.is_empty()
        {
            return None;
        }
        let line = source.split('\n').nth(row)?.trim_end_matches('\r');
        let lead = line.trim_start();
        if lead.starts_with('\'') || lead.starts_with("%%") {
            return None;
        }
        let word_at = |at: usize| {
            let start = line[..at]
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(0, |i| i + 1);
            let end = line[at..]
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map_or(line.len(), |i| at + i);
            (start, &line[start..end])
        };
        let col = col.min(line.len());
        if !line.is_char_boundary(col) {
            return None;
        }
        let (start, mut word) = word_at(col);
        if let Some(head) = line[..start].trim_end().strip_suffix("<<") {
            let head = head.trim_end();
            word = word_at(head.len()).1;
        }
        if word.is_empty() || !diagram.drawn.map.quiet.iter().any(|q| q == word) {
            return None;
        }
        let place = diagram.drawn.map.places.iter().find(|p| p.name == word)?;
        let r = place.rect;
        Some(Rectangle::new(Point::new(r.x, r.y), Size::new(r.w, r.h)))
    }

    /// Moves the light to `target`, or lets it go out.
    pub(super) fn set_breath(&mut self, target: Option<Aim>) {
        self.breath = Breath::go(self.breath, target, crate::clock::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }

    #[test]
    fn a_light_spreads_from_a_point_and_travels_as_one() {
        let here = rect(10.0, 20.0, 40.0, 30.0);
        let there = rect(200.0, 80.0, 90.0, 50.0);
        let start = Instant::now();
        let breath = Breath::go(None, Some(Aim::shape(here)), start).unwrap();
        assert!(!breath.travels);
        let born = breath.at(start);
        assert!(born.spread < 0.01, "{}", born.spread);
        assert_eq!(born.rect, here);
        assert_eq!(born.mark, Mark::Shape);

        let settled = Breath {
            start: start - (ARRIVE + Duration::from_millis(20)),
            ..breath
        };
        let held = settled.at(start);
        assert!((held.spread - 1.0).abs() < 0.01 && held.alpha > 0.99);

        let moving = Breath::go(
            Some(settled),
            Some(Aim::words(there, [0.1, 0.1, 0.1], None)),
            start,
        )
        .unwrap();
        assert!(moving.travels);
        let shrunk = moving.at(start + Duration::from_millis(110));
        assert!(shrunk.spread < 0.05, "shrunk to a point, {}", shrunk.spread);
        let mid = moving.at(start + Duration::from_millis(110 + 85));
        let between = mix_point(here.center(), there.center(), 0.5);
        assert!(mid.spread < 0.05, "still a point, {}", mid.spread);
        assert!(
            (mid.rect.x - between.x).abs() < 1.0 && (mid.rect.y - between.y).abs() < 1.0,
            "travels between the two, {:?}",
            mid.rect
        );
        let arrived = moving.at(start + MOVE);
        assert_eq!(arrived.rect, there);
        assert_eq!(arrived.mark, Mark::Words);
        assert!((arrived.spread - 1.0).abs() < 0.01, "{}", arrived.spread);
    }
}
