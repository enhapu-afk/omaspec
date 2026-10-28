//! Lines that bend in curves. The routers lay a line out as straight runs
//! that keep clear of the boxes; drawn, each corner turns in a curve, so
//! a jog becomes an S and a detour an arc, as PlantUML (through Graphviz)
//! and Mermaid draw their lines. A curve starts no further from its corner
//! than half of either run, the first and the last run whole, so it never
//! leaves the corridor the router found, and the ends keep their direction:
//! an arrowhead still sits on the line.

use crate::Declined;
use crate::common::Skinparam;
use crate::svg::n;

type Point = (f32, f32);

/// How far a corner's curve reaches along each run, at most.
pub const RADIUS: f32 = 48.0;

/// The corner radius `skinparam linetype` asks for: PlantUML's `ortho` and
/// `polyline` keep the corners; without the skinparam the lines curve.
pub(crate) fn linetype(p: &Skinparam) -> Result<f32, Declined> {
    match p.value.trim().to_ascii_lowercase().as_str() {
        "ortho" | "polyline" => Ok(0.0),
        _ => Err(p.unsupported()),
    }
}

/// How far a curve reaches along a cubic's handles: a quarter circle for a
/// right angle.
const ROUND: f32 = 0.552_284_8;

/// Points the curve of one corner is drawn with, for [`flatten`].
const STEPS: usize = 6;

/// One rounded corner: where its curve leaves the run before, the two
/// handles, and where it joins the run after.
struct Bend {
    from: Point,
    first: Point,
    second: Point,
    to: Point,
}

/// The corners of `points` rounded by up to `radius`. Straight joins and
/// corners too short to round are left out.
fn bends(points: &[Point], radius: f32) -> Vec<Option<Bend>> {
    let last = points.len().saturating_sub(1);
    (1..last)
        .map(|i| {
            let (a, p, b) = (points[i - 1], points[i], points[i + 1]);
            let (before, after) = (dist(a, p), dist(p, b));
            if before < 0.01 || after < 0.01 {
                return None;
            }
            let (u, v) = (unit(a, p), unit(p, b));
            // Nearly straight on: no corner to round.
            if (u.0 * v.0 + u.1 * v.1) > 0.999 {
                return None;
            }
            // A run between two corners is shared; the first and the last
            // belong to one corner each.
            let room_before = if i == 1 { before } else { before / 2.0 };
            let room_after = if i + 1 == last { after } else { after / 2.0 };
            let r = radius.min(room_before).min(room_after);
            if r < 0.5 {
                return None;
            }
            let from = (p.0 - u.0 * r, p.1 - u.1 * r);
            let to = (p.0 + v.0 * r, p.1 + v.1 * r);
            Some(Bend {
                from,
                first: (from.0 + u.0 * r * ROUND, from.1 + u.1 * r * ROUND),
                second: (to.0 - v.0 * r * ROUND, to.1 - v.1 * r * ROUND),
                to,
            })
        })
        .collect()
}

/// The SVG path of `points` with every corner rounded by up to `radius`.
pub fn path(points: &[Point], radius: f32) -> String {
    let Some(&start) = points.first() else {
        return String::new();
    };
    let mut d = format!("M{},{}", n(start.0), n(start.1));
    for (i, bend) in bends(points, radius).into_iter().enumerate() {
        match bend {
            Some(b) => d.push_str(&format!(
                " L{},{} C{},{} {},{} {},{}",
                n(b.from.0),
                n(b.from.1),
                n(b.first.0),
                n(b.first.1),
                n(b.second.0),
                n(b.second.1),
                n(b.to.0),
                n(b.to.1)
            )),
            None => {
                let p = points[i + 1];
                d.push_str(&format!(" L{},{}", n(p.0), n(p.1)));
            }
        }
    }
    if points.len() > 1 {
        let end = points[points.len() - 1];
        d.push_str(&format!(" L{},{}", n(end.0), n(end.1)));
    }
    d
}

/// The same line as [`path`] draws, as points close enough together for a
/// light to run along it.
pub fn flatten(points: &[Point], radius: f32) -> Vec<Point> {
    let Some(&start) = points.first() else {
        return Vec::new();
    };
    let mut out = vec![start];
    for (i, bend) in bends(points, radius).into_iter().enumerate() {
        match bend {
            Some(b) => {
                out.push(b.from);
                for step in 1..=STEPS {
                    let t = step as f32 / STEPS as f32;
                    out.push(cubic(b.from, b.first, b.second, b.to, t));
                }
            }
            None => out.push(points[i + 1]),
        }
    }
    if points.len() > 1 {
        out.push(points[points.len() - 1]);
    }
    out.dedup_by(|a, b| dist(*a, *b) < 0.01);
    out
}

fn cubic(a: Point, b: Point, c: Point, d: Point, t: f32) -> Point {
    let s = 1.0 - t;
    let (w0, w1, w2, w3) = (s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t);
    (
        w0 * a.0 + w1 * b.0 + w2 * c.0 + w3 * d.0,
        w0 * a.1 + w1 * b.1 + w2 * c.1 + w3 * d.1,
    )
}

fn dist(a: Point, b: Point) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

fn unit(a: Point, b: Point) -> Point {
    let len = dist(a, b).max(f32::EPSILON);
    ((b.0 - a.0) / len, (b.1 - a.1) / len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_corner_turns_in_a_quarter_circle_and_the_ends_stay() {
        let points = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0)];
        let d = path(&points, 20.0);
        assert_eq!(d, "M0,0 L80,0 C91.05,0 100,8.95 100,20 L100,100");
        let flat = flatten(&points, 20.0);
        assert_eq!(flat.first(), Some(&(0.0, 0.0)));
        assert_eq!(flat.last(), Some(&(100.0, 100.0)));
        // The middle of the curve lies on the quarter circle around (80, 20).
        let mid = cubic((80.0, 0.0), (91.05, 0.0), (100.0, 8.95), (100.0, 20.0), 0.5);
        let r = dist(mid, (80.0, 20.0));
        assert!((r - 20.0).abs() < 0.1, "{r}");
    }

    #[test]
    fn a_short_jog_becomes_an_s_and_straight_lines_stay_straight() {
        // Two corners 10 apart share the run between them: 5 each.
        let jog = [(0.0, 0.0), (50.0, 0.0), (50.0, 10.0), (100.0, 10.0)];
        let d = path(&jog, 20.0);
        assert!(d.starts_with("M0,0 L45,0 C"), "{d}");
        assert!(d.contains(" 50,5 L50,5 C"), "{d}");
        assert!(d.ends_with(" 55,10 L100,10"), "{d}");
        assert_eq!(path(&[(0.0, 0.0), (10.0, 0.0)], 20.0), "M0,0 L10,0");
        assert_eq!(
            path(&[(0.0, 0.0), (10.0, 0.0), (30.0, 0.0)], 20.0),
            "M0,0 L10,0 L30,0"
        );
    }

    #[test]
    fn the_first_and_last_runs_may_be_used_whole() {
        let points = [(0.0, 0.0), (8.0, 0.0), (8.0, 100.0)];
        assert!(path(&points, 20.0).starts_with("M0,0 L0,0 C"));
    }
}
