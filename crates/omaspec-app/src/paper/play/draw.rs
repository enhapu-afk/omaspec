//! How a play draws: its lights, their glow and ripple, and the strokes
//! and shapes they are made of.

use std::time::Duration;

use iced::border::Radius;
use iced::time::Instant;
use iced::widget::canvas::{Frame, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Point, Rectangle, Size, Vector};
use omaspec_uml::map::{Shape, Spot};

use super::super::focus::{Outline, Segment, trace};
use super::super::light::{ease_out, fraction};

use super::*;

/// Notes that a state's light goes on or off at `at`; older notes go.
pub(super) fn light(events: &mut Vec<(Instant, bool)>, at: Instant, on: bool) {
    events.push((at, on));
    events.sort_by_key(|(t, _)| *t);
    if events.len() > 6 {
        events.remove(0);
    }
}

/// Whether the light is on its way to being on at `now`.
pub(super) fn aimed(events: &[(Instant, bool)], now: Instant) -> bool {
    events
        .iter()
        .rev()
        .find(|(t, _)| *t <= now)
        .is_some_and(|(_, on)| *on)
}

/// How bright a light is at `now`, from when it went on and off.
pub(super) fn level(events: &[(Instant, bool)], now: Instant) -> f32 {
    let mut since: Option<(Instant, f32, bool)> = None;
    let value = |since: Option<(Instant, f32, bool)>, at: Instant| match since {
        None => 0.0,
        Some((t, from, on)) => {
            let k = ease_out(fraction(at.saturating_duration_since(t), LIGHT_FADE));
            if on {
                from + (1.0 - from) * k
            } else {
                from * (1.0 - k)
            }
        }
    };
    for &(t, on) in events {
        if t > now {
            break;
        }
        since = Some((t, value(since, t), on));
    }
    value(since, now)
}

/// How long a light takes along `points` (diagram units), as a play's
/// model says ([`omaspec_model::play::travel_millis`]).
pub(super) fn travel_time(points: &[(f32, f32)]) -> Duration {
    Duration::from_millis(omaspec_model::play::travel_millis(points))
}

pub(super) fn ramp(now: Instant, since: Instant, over: Duration) -> f32 {
    ease_out(fraction(now.saturating_duration_since(since), over))
}

pub(in crate::paper) fn with_alpha(c: Color, a: f32) -> Color {
    Color {
        a: (c.a * a).clamp(0.0, 1.0),
        ..c
    }
}

pub(super) fn soft(color: Color, width: f32, alpha: f32) -> Stroke<'static> {
    Stroke::default()
        .with_color(with_alpha(color, alpha))
        .with_width(width)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round)
}

pub(super) fn grow(r: Rectangle, by: f32) -> Rectangle {
    Rectangle::new(
        r.position() - Vector::new(by, by),
        Size::new(r.width + 2.0 * by, r.height + 2.0 * by),
    )
}

pub(super) fn rounded(r: Rectangle, radius: f32) -> Path {
    let radius = radius.min(r.width / 2.0).min(r.height / 2.0).max(0.0);
    Path::rounded_rectangle(r.position(), r.size(), Radius::from(radius))
}

pub(super) fn polygon(points: &[Point]) -> Path {
    Path::new(|b| {
        if let Some((&first, rest)) = points.split_first() {
            b.move_to(first);
            for &p in rest {
                b.line_to(p);
            }
            b.close();
        }
    })
}

pub(super) fn polyline(points: impl IntoIterator<Item = Point>) -> Path {
    Path::new(|b| {
        let mut first = true;
        for p in points {
            if first {
                b.move_to(p);
                first = false;
            } else {
                b.line_to(p);
            }
        }
    })
}

pub(super) fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

/// How much bigger the diagram is on screen than in its own units.
pub(super) fn zoom(spot: &Spot, view: &Viewer) -> f32 {
    let screen = (view.place)(spot.rect);
    if spot.rect.w > 0.0 {
        screen.width / spot.rect.w
    } else {
        1.0
    }
}

/// The outline a state's light follows, on screen: a rounded box (a bar
/// is one), a ring, a diamond.
pub(in crate::paper) enum Glow {
    Box(Rectangle, f32),
    Ring(Point, f32),
    Diamond(Rectangle),
    /// The state's own outline as its picture draws it (docs/adr/0029),
    /// on screen: a state drawn by hand keeps its bowed sides. With the
    /// box around it and how far its rim reaches past it.
    Own(Vec<Segment>, Rectangle, f32),
}

impl Glow {
    /// [`Self::of`], in the state's own shape where the picture says it:
    /// a state or a choice whose part has an outline.
    pub(in crate::paper) fn shaped(spot: &Spot, outline: Option<&Outline>, view: &Viewer) -> Glow {
        match (outline, spot.shape) {
            (Some(outline), Shape::State | Shape::Choice) => {
                let (segments, bounds, reach) = outline.on_screen(view.place);
                Glow::Own(segments, bounds, reach)
            }
            _ => Glow::of(spot, view),
        }
    }

    /// Its own outline `out` pixels larger on every side, stretched from
    /// its middle, as one more shape of `builder`.
    fn trace_own(
        segments: &[Segment],
        bounds: Rectangle,
        builder: &mut iced::widget::canvas::path::Builder,
        out: f32,
    ) {
        let c = bounds.center();
        let (kx, ky) = (
            (bounds.width + 2.0 * out) / bounds.width.max(1e-3),
            (bounds.height + 2.0 * out) / bounds.height.max(1e-3),
        );
        trace(segments, builder, &|(x, y)| {
            Point::new(c.x + (x - c.x) * kx, c.y + (y - c.y) * ky)
        });
    }

    pub(in crate::paper) fn of(spot: &Spot, view: &Viewer) -> Glow {
        let r = (view.place)(spot.rect);
        let radius = spot.radius * zoom(spot, view);
        match spot.shape {
            Shape::Start | Shape::End | Shape::History => Glow::Ring(r.center(), radius),
            Shape::Choice => Glow::Diamond(r),
            // A point's corner is its circle's, or a pin's square one.
            Shape::State | Shape::Bar | Shape::Point => Glow::Box(r, radius),
        }
    }

    /// The box the outline fits in.
    pub(in crate::paper) fn bounds(&self) -> Rectangle {
        match *self {
            Glow::Box(r, _) | Glow::Diamond(r) => r,
            // With its rim, which is the state's too.
            Glow::Own(_, r, reach) => grow(r, reach),
            Glow::Ring(c, radius) => Rectangle::new(
                c - Vector::new(radius, radius),
                Size::new(2.0 * radius, 2.0 * radius),
            ),
        }
    }

    pub(super) fn path(&self, out: f32) -> Path {
        match *self {
            Glow::Own(ref segments, bounds, reach) => {
                Path::new(|b| Self::trace_own(segments, bounds, b, reach + out))
            }
            Glow::Box(r, radius) => rounded(grow(r, out), radius + out),
            Glow::Ring(c, radius) => Path::circle(c, radius + out),
            Glow::Diamond(r) => {
                let r = grow(r, out);
                let c = r.center();
                polygon(&[
                    Point::new(c.x, r.y),
                    Point::new(r.x + r.width, c.y),
                    Point::new(c.x, r.y + r.height),
                    Point::new(r.x, c.y),
                ])
            }
        }
    }

    /// The outline `out` pixels larger as one more shape of `builder`.
    pub(in crate::paper) fn trace(
        &self,
        builder: &mut iced::widget::canvas::path::Builder,
        out: f32,
    ) {
        match *self {
            Glow::Own(ref segments, bounds, reach) => {
                Self::trace_own(segments, bounds, builder, reach + out);
            }
            Glow::Box(r, radius) => {
                let r = grow(r, out);
                let radius = (radius + out)
                    .min(r.width / 2.0)
                    .min(r.height / 2.0)
                    .max(0.0);
                builder.rounded_rectangle(r.position(), r.size(), radius.into());
            }
            Glow::Ring(c, radius) => builder.circle(c, radius + out),
            Glow::Diamond(r) => {
                let r = grow(r, out);
                let c = r.center();
                builder.move_to(Point::new(c.x, r.y));
                builder.line_to(Point::new(r.x + r.width, c.y));
                builder.line_to(Point::new(c.x, r.y + r.height));
                builder.line_to(Point::new(r.x, c.y));
                builder.close();
            }
        }
    }

    /// Covers in `color` what lies in its bounds but outside the outline:
    /// the corners a rectangular clip lets through.
    pub(in crate::paper) fn mask(&self, frame: &mut Frame, color: Color) {
        let r = self.bounds();
        if let Glow::Own(segments, bounds, reach) = self {
            // Between the box let through and the outline with its rim.
            let around = Path::new(|b| {
                b.rectangle(r.position(), r.size());
                Self::trace_own(segments, *bounds, b, *reach);
            });
            frame.fill(
                &around,
                iced::widget::canvas::Fill {
                    style: iced::widget::canvas::Style::Solid(color),
                    rule: iced::widget::canvas::fill::Rule::EvenOdd,
                },
            );
            return;
        }
        let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.width, r.y + r.height);
        match *self {
            Glow::Box(_, radius) | Glow::Ring(_, radius) => {
                let radius = radius.min(r.width / 2.0).min(r.height / 2.0);
                if radius <= 0.5 {
                    return;
                }
                // Each corner: from the corner along one side, round the
                // arc, back to the corner.
                for (corner, along, then) in [
                    (
                        Point::new(x0, y0),
                        Vector::new(radius, 0.0),
                        Vector::new(0.0, radius),
                    ),
                    (
                        Point::new(x1, y0),
                        Vector::new(0.0, radius),
                        Vector::new(-radius, 0.0),
                    ),
                    (
                        Point::new(x1, y1),
                        Vector::new(-radius, 0.0),
                        Vector::new(0.0, -radius),
                    ),
                    (
                        Point::new(x0, y1),
                        Vector::new(0.0, -radius),
                        Vector::new(radius, 0.0),
                    ),
                ] {
                    let path = Path::new(|b| {
                        b.move_to(corner);
                        b.line_to(corner + along);
                        b.arc_to(corner, corner + then, radius);
                        b.close();
                    });
                    frame.fill(&path, color);
                }
            }
            // Drawn above.
            Glow::Own(..) => {}
            Glow::Diamond(_) => {
                let c = r.center();
                for corner in [
                    [Point::new(x0, y0), Point::new(c.x, y0), Point::new(x0, c.y)],
                    [Point::new(x1, y0), Point::new(x1, c.y), Point::new(c.x, y0)],
                    [Point::new(x1, y1), Point::new(c.x, y1), Point::new(x1, c.y)],
                    [Point::new(x0, y1), Point::new(x0, c.y), Point::new(c.x, y1)],
                ] {
                    frame.fill(&polygon(&corner), color);
                }
            }
        }
    }

    /// A soft bloom around the outline and a crisp line on it.
    pub(in crate::paper) fn draw(&self, frame: &mut Frame, accent: Color, level: f32) {
        pub(super) const LAYERS: usize = 7;
        for layer in 0..LAYERS {
            let k = 1.0 - layer as f32 / LAYERS as f32;
            frame.stroke(
                &self.path(1.5 + layer as f32 * 1.6),
                soft(accent, 2.0 + layer as f32 * 1.4, 0.26 * level * k.powf(1.7)),
            );
        }
        frame.stroke(&self.path(0.5), soft(accent, 2.2, level));
    }

    /// A ring spreading out from the outline as `k` goes from 0 to 1.
    pub(super) fn ripple(&self, frame: &mut Frame, accent: Color, k: f32) {
        frame.stroke(
            &self.path(2.0 + 20.0 * k),
            soft(accent, 2.0, 0.6 * (1.0 - k)),
        );
    }
}
