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
const HEAD: f32 = 0.85;
/// How far the head's color goes from the accent towards the light, so
/// the run reads on a line of the accent's own color.
const WHITEN: f32 = 0.6;

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
    draw_run_over(frame, points, accent, tip, light, None);
}

/// [`draw_run`], with the stretch the light has not reached yet shaded in
/// `ahead` (the page's color), so the line lights up as the head passes,
/// where the line is already drawn under it.
pub(super) fn draw_run_over(
    frame: &mut Frame,
    points: &[Point],
    accent: Color,
    tip: Color,
    light: Light,
    ahead: Option<Color>,
) {
    let length = polyline_length(points);
    let head = length * light.head.clamp(0.0, 1.0);
    if points.len() < 2 || head <= 0.5 || (light.rest <= 0.01 && light.extra <= 0.01) {
        return;
    }
    if let Some(shade) = ahead.filter(|_| light.extra > 0.01 && head < length - 0.5) {
        let left = stretch(points, head, length);
        if left.len() >= 2 {
            frame.stroke(&polyline(left), soft(shade, 4.0, light.extra));
        }
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
    draw_ribbon(frame, &stations, accent, tip, light.extra);
    let Some(&(at, _)) = stations.last() else {
        return;
    };
    let warm = mix(accent, tip, WHITEN);
    for (radius, alpha, color) in [
        (18.0, 0.07, accent),
        (12.0, 0.15, accent),
        (7.5, 0.32, warm),
        (4.6, 0.75, warm),
    ] {
        frame.fill(
            &Path::circle(at, radius),
            with_alpha(color, alpha * light.extra),
        );
    }
    frame.fill(&Path::circle(at, 2.6), with_alpha(tip, light.extra));
}

/// A highlight over `area` (screen units) that fades out on all four
/// sides, smoothly: drawn row by row on whole device pixels, each row one
/// gradient across, as bright as the fade at its height. A band with only
/// its top and bottom faded ended hard where the paper's shadow begins.
pub(super) fn soft_spot(
    frame: &mut Frame,
    paper: Rectangle,
    scale: f32,
    area: Rectangle,
    color: Color,
) {
    let (left, right) = (
        area.x.max(paper.x),
        (area.x + area.width).min(paper.x + paper.width),
    );
    let (top, bottom) = (area.y, area.y + area.height);
    if right - left < 1.0 || bottom - top < 1.0 || color.a <= 0.004 {
        return;
    }
    let scale = if scale > 0.0 { scale } else { 1.0 };
    // Across: a ramp in at the left and out at the right, four stops each,
    // in order (iced keeps eight, sorted).
    let width = right - left;
    let edge_x = (64.0_f32).min(width * 0.35) / width;
    // `with_alpha` scales the color's own alpha by `share`.
    let across = |share: f32| {
        let mut row = Linear::new(Point::new(left, top), Point::new(right, top));
        for i in 0..4 {
            let t = i as f32 / 3.0;
            row = row.add_stop(edge_x * t, with_alpha(color, share * smooth(t)));
        }
        for i in 0..4 {
            let t = i as f32 / 3.0;
            row = row.add_stop(
                1.0 - edge_x * (1.0 - t),
                with_alpha(color, share * smooth(1.0 - t)),
            );
        }
        row
    };
    // Down: rows of whole device pixels, at most 48 of them, each as bright
    // as the fade at its height.
    let height = bottom - top;
    let edge_y = (20.0_f32).min(height * 0.4);
    let (first, last) = ((top * scale).floor(), (bottom * scale).ceil());
    let step = ((last - first) / 48.0).ceil().max(1.0);
    let mut y = first;
    while y < last {
        let (y0, y1) = (y / scale, (y + step).min(last) / scale);
        let middle = (y0 + y1) / 2.0;
        let fade = smooth((middle - top) / edge_y).min(smooth((bottom - middle) / edge_y));
        if fade > 0.004 {
            fill_paper_band(frame, paper, scale, y0, y1, across(fade));
        }
        y += step;
    }
}

/// `t` eased in and out, from 0 to 1.
pub(super) fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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

/// The trail's color at `k` of the way from its tail to its head: the
/// accent, lighter towards the head, as bright as [`trail_alpha`].
fn trail_color(accent: Color, tip: Color, k: f32, presence: f32) -> Color {
    with_alpha(mix(accent, tip, WHITEN * k * k), trail_alpha(k) * presence)
}

/// `a` moved `t` of the way to `b`, alpha too.
fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
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
fn draw_ribbon(
    frame: &mut Frame,
    stations: &[(Point, f32)],
    accent: Color,
    tip: Color,
    presence: f32,
) {
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
            .add_stop(0.0, trail_color(accent, tip, k0, presence))
            .add_stop(1.0, trail_color(accent, tip, k1, presence));
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
