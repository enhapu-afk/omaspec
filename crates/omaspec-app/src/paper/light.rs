//! The light that runs along an arrow while a diagram is played.
//!
//! One value says where the light is and how bright each part of it is.
//! The stroke it has already travelled rests at that brightness, and the
//! head is one gradient over the last stretch of it, so the join never
//! shows as a step. A scenario and a state machine share the curve; they
//! differ only in how far the line dims once the head has arrived.

use std::time::Duration;

use iced::widget::canvas::gradient::Linear;
use iced::widget::canvas::{Frame, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Point, Rectangle, Vector};

use super::canvas::fill_paper_band;
use super::play::with_alpha;

/// How long a travelled route stays lit after its light arrived.
pub(super) const LINGER: Duration = Duration::from_millis(520);
/// How long the veil takes to lift off the next message, and how long a
/// line takes to fade in or out with that step.
pub(super) const REVEAL: Duration = Duration::from_millis(460);
/// How long a light's bright head takes to settle into the line it travelled.
const DISSOLVE: Duration = Duration::from_millis(220);
/// How far behind its head a light's trail reaches, on screen.
const TRAIL: f32 = 150.0;
/// How bright the head is, over the resting line.
const HEAD: f32 = 0.7;

/// Where a running light is, and how bright the line under it is.
#[derive(Clone, Copy, Debug)]
pub(super) struct Light {
    /// 0 at the start of the arrow, 1 once the light has arrived.
    pub head: f32,
    /// Alpha of the stroke the light has already travelled.
    pub rest: f32,
    /// Alpha of the bright head over that stroke. 0 once the head is gone.
    pub extra: f32,
}

/// How a light rests. `rest` is the alpha at full presence, before the
/// linger. The linger ends at `floor` of that alpha: a scenario keeps
/// half, a machine route fades out. `head_with_linger` fades the head with
/// that same linger; a scenario lets the head dissolve on its own.
#[derive(Clone, Copy)]
pub(super) struct Curve {
    pub rest: f32,
    pub floor: f32,
    pub head_with_linger: bool,
}

/// A scenario message. It fades in over [`REVEAL`] and keeps half its light.
pub(super) const SCENARIO: Curve = Curve {
    rest: 0.9,
    floor: 0.5,
    head_with_linger: false,
};

/// A state machine's route. It is fully present and fades out after it arrives.
pub(super) const MACHINE: Curve = Curve {
    rest: 0.75,
    floor: 0.0,
    head_with_linger: true,
};

/// The light `since` after it set off, along a way that takes `took`.
/// `onward` runs the head; going back shows the line as it rests.
/// `presence` fades the whole light in or out.
pub(super) fn appearance(
    since: Duration,
    took: Duration,
    onward: bool,
    presence: f32,
    curve: Curve,
) -> Light {
    let arrived = onward && since >= took;
    // Dim only once the head has arrived, and from then, so the line does
    // not step down as the head gets there. Going back dims after the reveal.
    let dimming = if onward && !arrived {
        1.0
    } else {
        let from = if onward {
            since.saturating_sub(took)
        } else {
            since.saturating_sub(REVEAL)
        };
        let k = ease_out(fraction(from, LINGER));
        1.0 - (1.0 - curve.floor) * k
    };
    let head = if onward {
        ease_in_out(fraction(since, took)).min(1.0)
    } else {
        1.0
    };
    // Full until the head arrives, then it eases into the resting line.
    let dissolve = if !onward {
        0.0
    } else if since > took {
        1.0 - ease_out(fraction(since - took, DISSOLVE))
    } else {
        1.0
    };
    let extra = presence * dissolve * if curve.head_with_linger { dimming } else { 1.0 };
    Light {
        head,
        rest: curve.rest * presence * dimming,
        extra,
    }
}

pub(super) fn fraction(elapsed: Duration, of: Duration) -> f32 {
    if of.is_zero() {
        1.0
    } else {
        (elapsed.as_secs_f32() / of.as_secs_f32()).clamp(0.0, 1.0)
    }
}

pub(super) fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub(super) fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// The light along `points`. `light.head` is 0 to 1 along the whole line.
pub(super) fn draw_run(
    frame: &mut Frame,
    points: &[Point],
    accent: Color,
    tip: Color,
    light: Light,
) {
    let length = polyline_length(points);
    let head = length * light.head.clamp(0.0, 1.0);
    if points.len() < 2 || head <= 0.5 || (light.rest <= 0.01 && light.extra <= 0.01) {
        return;
    }
    if light.rest > 0.01 {
        let walked = stretch(points, 0.0, head);
        if walked.len() >= 2 {
            frame.stroke(&polyline(walked), soft(accent, 2.5, light.rest));
        }
    }
    if light.extra <= 0.01 {
        return;
    }
    let tail = (head - TRAIL).max(0.0);
    let stations = stations(points, tail, head);
    draw_ribbon(frame, &stations, accent, light.extra);
    let Some(&(at, _)) = stations.last() else {
        return;
    };
    for (radius, alpha) in [(18.0, 0.06), (12.0, 0.13), (7.5, 0.28), (4.6, 0.7)] {
        frame.fill(
            &Path::circle(at, radius),
            with_alpha(accent, alpha * light.extra),
        );
    }
    frame.fill(&Path::circle(at, 2.6), with_alpha(tip, light.extra));
}

/// The row highlight, fading out at its top and bottom.
pub(super) fn soft_band(
    frame: &mut Frame,
    paper: Rectangle,
    scale: f32,
    top: f32,
    bottom: f32,
    color: Color,
) {
    let height = bottom - top;
    if height < 1.0 || color.a <= 0.004 {
        return;
    }
    let edge = (14.0 / height).clamp(0.08, 0.45);
    let clear = with_alpha(color, 0.0);
    let fade = Linear::new(Point::new(paper.x, top), Point::new(paper.x, bottom))
        .add_stop(0.0, clear)
        .add_stop(edge, color)
        .add_stop(1.0 - edge, color)
        .add_stop(1.0, clear);
    fill_paper_band(frame, paper, scale, top, bottom, fade);
}

pub(super) fn polyline_length(points: &[Point]) -> f32 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// The part of `points` from `from` to `to` along it (screen lengths).
fn stretch(points: &[Point], from: f32, to: f32) -> Vec<Point> {
    let mut out = Vec::new();
    let mut walked = 0.0;
    for w in points.windows(2) {
        let len = w[0].distance(w[1]);
        let (a, b) = (walked, walked + len);
        walked = b;
        if b < from || a > to || len == 0.0 {
            continue;
        }
        let at = |d: f32| {
            let k = ((d - a) / len).clamp(0.0, 1.0);
            Point::new(
                w[0].x + (w[1].x - w[0].x) * k,
                w[0].y + (w[1].y - w[0].y) * k,
            )
        };
        if out.is_empty() {
            out.push(at(from.max(a)));
        }
        out.push(at(to.min(b)));
    }
    out
}

fn polyline(points: impl IntoIterator<Item = Point>) -> Path {
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

fn soft(color: Color, width: f32, alpha: f32) -> Stroke<'static> {
    Stroke::default()
        .with_color(with_alpha(color, alpha))
        .with_width(width)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round)
}

/// How much brighter than the resting line the head is, 0 at the tail of
/// the trail and [`HEAD`] at the head. One curve, so a run has no steps.
fn trail_alpha(k: f32) -> f32 {
    HEAD * k * k
}

/// The trail's width. It starts as wide as the resting line.
fn trail_width(k: f32) -> f32 {
    2.5 + 3.5 * k
}

/// Points of `points` from `from` to `to`, with the distance past `from`.
fn stations(points: &[Point], from: f32, to: f32) -> Vec<(Point, f32)> {
    let mut out = Vec::new();
    let mut walked = 0.0;
    for pair in points.windows(2) {
        let len = pair[0].distance(pair[1]);
        let (a, b) = (walked, walked + len);
        walked = b;
        if b < from || a > to || len < 1e-3 {
            continue;
        }
        let at = |d: f32| {
            let k = ((d - a) / len).clamp(0.0, 1.0);
            Point::new(
                pair[0].x + (pair[1].x - pair[0].x) * k,
                pair[0].y + (pair[1].y - pair[0].y) * k,
            )
        };
        let start = from.max(a);
        push_station(&mut out, at(start), start - from);
        let end = to.min(b);
        if end > start + 0.05 {
            push_station(&mut out, at(end), end - from);
        }
    }
    out
}

fn push_station(out: &mut Vec<(Point, f32)>, at: Point, distance: f32) {
    if out
        .last()
        .is_none_or(|(_, previous)| distance - previous > 0.05)
    {
        out.push((at, distance));
    }
}

/// The bright head: one filled run per straight part, a gradient along it.
/// Neighbours share an edge and the color on it, so the corner does not
/// show. A stack of separate strokes used to: each cap sat on the next.
fn draw_ribbon(frame: &mut Frame, stations: &[(Point, f32)], accent: Color, presence: f32) {
    let Some(&(_, span)) = stations.last() else {
        return;
    };
    if stations.len() < 2 || span < 0.5 {
        return;
    }
    let ends: Vec<(Point, Point)> = stations
        .iter()
        .enumerate()
        .map(|(i, &(at, distance))| {
            let half = trail_width(distance / span) * 0.5;
            let prev = i.checked_sub(1).map(|j| stations[j].0);
            let next = stations.get(i + 1).map(|station| station.0);
            offset_at(prev, at, next, half)
        })
        .collect();
    for i in 0..stations.len() - 1 {
        let k0 = stations[i].1 / span;
        let k1 = stations[i + 1].1 / span;
        let fill = Linear::new(stations[i].0, stations[i + 1].0)
            .add_stop(0.0, with_alpha(accent, trail_alpha(k0) * presence))
            .add_stop(1.0, with_alpha(accent, trail_alpha(k1) * presence));
        let (a, d) = ends[i];
        let (b, c) = ends[i + 1];
        frame.fill(&quad(a, b, c, d), fill);
    }
}

/// Left and right of `at`, mitered when the line turns. The miter stays
/// short, so a sharp corner does not spike.
fn offset_at(prev: Option<Point>, at: Point, next: Option<Point>, half: f32) -> (Point, Point) {
    let n = match (
        prev.and_then(|p| unit(at - p)),
        next.and_then(|p| unit(p - at)),
    ) {
        (Some(a), Some(b)) => {
            let na = left_normal(a);
            let nb = left_normal(b);
            let sum = na + nb;
            let len = (sum.x * sum.x + sum.y * sum.y).sqrt();
            if len < 0.25 {
                na
            } else {
                let miter = Vector::new(sum.x / len, sum.y / len);
                let denom = miter.x * na.x + miter.y * na.y;
                if denom < 0.35 {
                    na
                } else {
                    miter * (1.0 / denom).min(2.0)
                }
            }
        }
        (Some(a), None) | (None, Some(a)) => left_normal(a),
        (None, None) => Vector::new(0.0, 1.0),
    };
    (at + n * half, at - n * half)
}

fn unit(v: Vector) -> Option<Vector> {
    let len = (v.x * v.x + v.y * v.y).sqrt();
    (len > 1e-3).then_some(Vector::new(v.x / len, v.y / len))
}

fn left_normal(d: Vector) -> Vector {
    Vector::new(-d.y, d.x)
}

fn quad(a: Point, b: Point, c: Point, d: Point) -> Path {
    Path::new(|builder| {
        builder.move_to(a);
        builder.line_to(b);
        builder.line_to(c);
        builder.line_to(d);
        builder.close();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settled_line_keeps_half_its_light() {
        let light = appearance(
            Duration::from_secs(2),
            Duration::from_millis(400),
            true,
            1.0,
            SCENARIO,
        );
        assert!((light.head - 1.0).abs() < 0.001);
        assert!(
            (light.rest - 0.45).abs() < 0.001,
            "the line rests at half, not at {}",
            light.rest
        );
        assert!(
            light.extra < 0.001,
            "the head has dissolved, {}",
            light.extra
        );
    }

    #[test]
    fn a_stretch_follows_the_route() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ];
        let part = stretch(&points, 5.0, 15.0);
        assert_eq!(
            part,
            [
                Point::new(5.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 5.0)
            ]
        );
        assert_eq!(
            stretch(&points, 20.0, 20.0).last(),
            Some(&Point::new(10.0, 10.0))
        );
    }

    #[test]
    fn a_trail_is_one_gradient_along_a_straight_arrow() {
        let points = [Point::new(0.0, 0.0), Point::new(400.0, 0.0)];
        let stations = stations(&points, 250.0, 400.0);
        assert_eq!(stations.len(), 2, "one run, not a stack of strokes");
        assert!((stations[0].1).abs() < 0.01);
        assert!((stations[1].1 - 150.0).abs() < 0.01);
        assert!(trail_alpha(0.0) < 0.001, "the tail meets the resting line");
        assert!((trail_alpha(0.5) - HEAD * 0.25).abs() < 0.001);
        assert!((trail_alpha(1.0) - HEAD).abs() < 0.001);
    }

    #[test]
    fn a_self_message_corner_is_one_miter() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(80.0, 0.0),
            Point::new(80.0, 36.0),
            Point::new(8.0, 36.0),
        ];
        let len = polyline_length(&points);
        let stations = stations(&points, 0.0, len);
        assert_eq!(stations.len(), 4);
        let half = trail_width(stations[1].1 / stations.last().unwrap().1) * 0.5;
        let (left, right) = offset_at(
            Some(stations[0].0),
            stations[1].0,
            Some(stations[2].0),
            half,
        );
        let reach = stations[1].0.distance(left);
        assert!(
            reach < half * 2.1 && reach > half * 1.2,
            "miter {reach} for half {half}"
        );
        assert!(left.distance(right) > half, "the two sides stay apart");
    }
}
