//! The sketch look: PlantUML's `skinparam handwritten true` and Mermaid's
//! `look: handDrawn` (docs/adr/0023). Each shape is drawn by hand, as
//! rough.js draws one: every run twice, each a little bowed, its ends a
//! little off; curves and circles through points a little off their
//! place; a fill a closed wobble along its outline. Words are set in
//! Excalifont, still measured as the UI font.
//!
//! The hand is steady: what is off and by how much follows from a seed,
//! the element's name where the drawer gives one and the shape's own size
//! where it does not, never from where it stands, so the same source
//! always draws the same picture and a box keeps its strokes when the
//! layout moves it.

mod path;

use std::fmt::Write as _;

use crate::svg::n;

pub(crate) use path::outlines;

pub(crate) type Point = (f32, f32);

/// How far an end or a handle may stray, in the picture's units, at most.
/// Rough.js's `maxRandomnessOffset` at roughness 1 reads about this far
/// on a diagram.
const OFFSET: f32 = 2.4;
/// How much a run bows: rough.js's `bowing`, a little past 1 so a long
/// side reads as drawn rather than as a ruled line.
const BOWING: f32 = 1.35;

/// A part of an outline: a straight run, or a curve as points along it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Piece {
    Line(Point, Point),
    Curve(Vec<Point>),
}

/// What a shape's border runs along, one subpath of it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Outline {
    pub pieces: Vec<Piece>,
    pub closed: bool,
}

impl Outline {
    /// The runs between `points`, back to the first when `closed`.
    pub(crate) fn polyline(points: &[Point], closed: bool) -> Outline {
        let mut pieces: Vec<Piece> = points
            .windows(2)
            .map(|pair| Piece::Line(pair[0], pair[1]))
            .collect();
        if closed
            && points.len() > 2
            && let (Some(&last), Some(&first)) = (points.last(), points.first())
            && path::distance(last, first) > 0.01
        {
            pieces.push(Piece::Line(last, first));
        }
        Outline {
            pieces,
            closed: closed && points.len() > 2,
        }
    }

    /// A rectangle, its corners rounded by `r`.
    pub(crate) fn rect((x, y, w, h): (f32, f32, f32, f32), r: f32) -> Outline {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        if r < 0.5 {
            return Outline::polyline(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)], true);
        }
        let k = r * 0.552_284_8;
        let corner =
            |a: Point, b: Point, c: Point, d: Point| Piece::Curve(path::cubic_points(a, b, c, d));
        let (right, bottom) = (x + w, y + h);
        Outline {
            pieces: vec![
                Piece::Line((x + r, y), (right - r, y)),
                corner(
                    (right - r, y),
                    (right - r + k, y),
                    (right, y + r - k),
                    (right, y + r),
                ),
                Piece::Line((right, y + r), (right, bottom - r)),
                corner(
                    (right, bottom - r),
                    (right, bottom - r + k),
                    (right - r + k, bottom),
                    (right - r, bottom),
                ),
                Piece::Line((right - r, bottom), (x + r, bottom)),
                corner(
                    (x + r, bottom),
                    (x + r - k, bottom),
                    (x, bottom - r + k),
                    (x, bottom - r),
                ),
                Piece::Line((x, bottom - r), (x, y + r)),
                corner((x, y + r), (x, y + r - k), (x + r - k, y), (x + r, y)),
            ],
            closed: true,
        }
    }

    /// Every point the outline passes, in order, each once.
    fn points(&self) -> Vec<Point> {
        let mut out: Vec<Point> = Vec::new();
        for piece in &self.pieces {
            let points: &[Point] = match piece {
                Piece::Line(a, b) => &[*a, *b],
                Piece::Curve(points) => points,
            };
            for &p in points {
                if out
                    .last()
                    .is_none_or(|&last| path::distance(last, p) > 0.01)
                {
                    out.push(p);
                }
            }
        }
        if self.closed && out.len() > 1 && path::distance(out[0], out[out.len() - 1]) <= 0.01 {
            out.pop();
        }
        out
    }
}

/// A steady hand: the same seed draws the same strokes.
pub(crate) struct Hand {
    state: u64,
    /// Share of [`OFFSET`] this stroke uses. A fill stays nearer the
    /// outline than the border that is drawn over it.
    amount: f32,
}

impl Hand {
    pub(crate) fn new(seed: u64) -> Hand {
        Hand {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
            amount: 1.0,
        }
    }

    /// A number in `[0, 1)` (splitmix64).
    fn next(&mut self) -> f32 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 40) as f32 / (1u64 << 24) as f32
    }

    /// A number in `[-x, x]`, less on a long run (`gain`).
    fn off(&mut self, x: f32, gain: f32) -> f32 {
        gain * (self.next() * 2.0 * x - x)
    }

    fn jitter(&mut self, p: Point, x: f32) -> Point {
        (p.0 + self.off(x, 1.0), p.1 + self.off(x, 1.0))
    }

    /// The border of `outlines`, drawn twice (once when not `twice`, as for
    /// a dashed line, whose dashes would not meet), as one path's `d`. An
    /// open outline keeps its ends where they are, so a line still meets
    /// its box and its arrowhead.
    pub(crate) fn stroke(&mut self, outlines: &[Outline], twice: bool) -> String {
        self.amount = 1.0;
        let mut d = String::new();
        for outline in outlines {
            let curved = outline
                .pieces
                .iter()
                .any(|piece| matches!(piece, Piece::Curve(_)));
            for overlay in [false, true].into_iter().take(if twice { 2 } else { 1 }) {
                if !outline.closed {
                    self.through(&mut d, outline, overlay, true);
                } else if curved {
                    self.through(&mut d, outline, overlay, false);
                } else {
                    self.each_run(&mut d, outline, overlay);
                }
            }
        }
        d
    }

    /// A closed outline of straight runs: each run on its own, its ends a
    /// little past the corner, as a hand draws a box.
    fn each_run(&mut self, d: &mut String, outline: &Outline, overlay: bool) {
        for piece in &outline.pieces {
            match piece {
                Piece::Line(a, b) => self.run(d, *a, *b, overlay, true, (false, false)),
                Piece::Curve(points) => self.curve(d, points, overlay, true, (false, false)),
            };
        }
    }

    /// One stroke through every piece, each going on from where the one
    /// before ended as drawn, so a rounded corner stays round. `ends`
    /// keeps the first and the last point where they are, as an open line
    /// needs; a closed one ends a little off where it began, as a hand
    /// closes a shape.
    fn through(&mut self, d: &mut String, outline: &Outline, overlay: bool, ends: bool) {
        let last = outline.pieces.len().saturating_sub(1);
        let mut at: Option<Point> = None;
        for (i, piece) in outline.pieces.iter().enumerate() {
            let keep = (ends, ends && i == last);
            let moves = at.is_none();
            at = Some(match piece {
                Piece::Line(a, b) => self.run(d, at.unwrap_or(*a), *b, overlay, moves, keep),
                Piece::Curve(points) => {
                    let mut points = points.clone();
                    if let Some(from) = at {
                        points[0] = from;
                    }
                    self.curve(d, &points, overlay, moves, keep)
                }
            });
        }
    }

    /// One run from `a` to `b` as a bowed cubic (rough.js's `_line`); an
    /// end in `keep` stays. `moves` starts a new stroke at `a`. Returns
    /// where the run ends, as drawn.
    fn run(
        &mut self,
        d: &mut String,
        a: Point,
        b: Point,
        overlay: bool,
        moves: bool,
        keep: (bool, bool),
    ) -> Point {
        let length = path::distance(a, b);
        let gain = if length < 200.0 {
            1.0
        } else if length > 500.0 {
            0.4
        } else {
            -0.001_666_8 * length + 1.233_334
        };
        let mut offset = OFFSET * self.amount;
        if offset * offset * 100.0 > length * length {
            offset = length / 10.0;
        }
        let half = offset / 2.0;
        let spread = if overlay { half } else { offset };
        let diverge = 0.2 + self.next() * 0.2;
        let bow_x = self.off(BOWING * offset * (b.1 - a.1) / 200.0, gain);
        let bow_y = self.off(BOWING * offset * (a.0 - b.0) / 200.0, gain);
        // A run that goes on from the one before starts where that ended.
        let start = if moves && !keep.0 {
            (a.0 + self.off(spread, gain), a.1 + self.off(spread, gain))
        } else {
            a
        };
        if moves {
            let _ = write!(d, "M{},{}", n(start.0), n(start.1));
        }
        let first = (
            bow_x + a.0 + (b.0 - a.0) * diverge + self.off(spread, gain),
            bow_y + a.1 + (b.1 - a.1) * diverge + self.off(spread, gain),
        );
        let second = (
            bow_x + a.0 + 2.0 * (b.0 - a.0) * diverge + self.off(spread, gain),
            bow_y + a.1 + 2.0 * (b.1 - a.1) * diverge + self.off(spread, gain),
        );
        let end = if keep.1 {
            b
        } else {
            (b.0 + self.off(spread, gain), b.1 + self.off(spread, gain))
        };
        let _ = write!(
            d,
            "C{},{} {},{} {},{}",
            n(first.0),
            n(first.1),
            n(second.0),
            n(second.1),
            n(end.0),
            n(end.1)
        );
        end
    }

    /// A curve through `points`, each a little off, drawn smooth
    /// (Catmull-Rom). Returns where it ends, as drawn.
    fn curve(
        &mut self,
        d: &mut String,
        points: &[Point],
        overlay: bool,
        moves: bool,
        keep: (bool, bool),
    ) -> Point {
        let length: f32 = points.windows(2).map(|w| path::distance(w[0], w[1])).sum();
        let points = thin(points, length);
        if points.len() < 2 {
            return points.first().copied().unwrap_or_default();
        }
        // A small curve, as a rounded corner, strays less, or it turns into
        // a bevel. A long one strays enough to read as drawn.
        let offset = (OFFSET * self.amount * if overlay { 0.55 } else { 0.9 }).min(length / 14.0);
        let last = points.len() - 1;
        // The first point stays when the curve goes on from the run
        // before, or starts an open line.
        let shaken: Vec<Point> = points
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                if (i == 0 && (!moves || keep.0)) || (i == last && keep.1) {
                    p
                } else {
                    self.jitter(p, offset)
                }
            })
            .collect();
        if moves {
            let _ = write!(d, "M{},{}", n(shaken[0].0), n(shaken[0].1));
        }
        catmull_rom(d, &shaken);
        shaken[last]
    }

    /// The fill of `outlines`: one closed wobble along each, nearer the
    /// outline than the border, so it sits inside the stroke.
    pub(crate) fn fill(&mut self, outlines: &[Outline]) -> String {
        self.amount = 0.62;
        let mut d = String::new();
        for outline in outlines {
            if outline.points().len() < 3 {
                continue;
            }
            self.closed(&mut d, outline);
            d.push('Z');
        }
        d
    }

    /// One stroke around `outline`, back to where it began, as drawn.
    fn closed(&mut self, d: &mut String, outline: &Outline) {
        let last = outline.pieces.len().saturating_sub(1);
        let mut at: Option<Point> = None;
        let mut start = Point::default();
        for (i, piece) in outline.pieces.iter().enumerate() {
            let home = i == last;
            let from = if let Some(at) = at {
                at
            } else {
                let from = match piece {
                    Piece::Line(a, _) => *a,
                    Piece::Curve(points) => points.first().copied().unwrap_or_default(),
                };
                let spread = OFFSET * self.amount * 0.5;
                let begun = self.jitter(from, spread);
                let _ = write!(d, "M{},{}", n(begun.0), n(begun.1));
                start = begun;
                begun
            };
            at = Some(match piece {
                Piece::Line(_, b) => self.run(
                    d,
                    from,
                    if home { start } else { *b },
                    false,
                    false,
                    (true, home),
                ),
                Piece::Curve(points) => {
                    let mut points = points.clone();
                    if let Some(first) = points.first_mut() {
                        *first = from;
                    }
                    if home && let Some(tail) = points.last_mut() {
                        *tail = start;
                    }
                    self.curve(d, &points, false, false, (true, home))
                }
            });
        }
    }

    /// The border of an ellipse drawn twice, each time round a little
    /// further than where it started, as a hand closes a circle.
    pub(crate) fn ellipse_stroke(&mut self, (cx, cy): Point, (rx, ry): Point) -> String {
        self.amount = 1.0;
        let mut d = String::new();
        for overlay in [false, true] {
            let points =
                self.ellipse_points((cx, cy), (rx, ry), if overlay { 0.5 } else { 1.0 }, true);
            let _ = write!(d, "M{},{}", n(points[0].0), n(points[0].1));
            catmull_rom(&mut d, &points);
        }
        d
    }

    /// The fill of an ellipse, a little off its border.
    pub(crate) fn ellipse_fill(&mut self, (cx, cy): Point, (rx, ry): Point) -> String {
        self.amount = 0.7;
        let mut points = self.ellipse_points((cx, cy), (rx, ry), 0.7, false);
        let mut d = String::new();
        points.push(points[0]);
        let _ = write!(d, "M{},{}", n(points[0].0), n(points[0].1));
        catmull_rom(&mut d, &points);
        d.push('Z');
        d
    }

    /// Points round an ellipse, its radii a little off, from an angle of
    /// its own; past the start by a step when `overlap`.
    fn ellipse_points(
        &mut self,
        (cx, cy): Point,
        (rx, ry): Point,
        amount: f32,
        overlap: bool,
    ) -> Vec<Point> {
        let around = std::f32::consts::TAU * ((rx * rx + ry * ry) / 2.0).sqrt();
        let steps = ((around / 14.0).ceil() as usize).clamp(8, 48);
        let step = std::f32::consts::TAU / steps as f32;
        let start = self.next() * std::f32::consts::TAU;
        let spread = (OFFSET * self.amount * 0.5 * amount).min(rx.min(ry) / 4.0);
        let grow = 1.0 + self.off(0.04 * amount, 1.0);
        let count = if overlap { steps + 2 } else { steps };
        (0..count)
            .map(|i| {
                let angle = start + step * i as f32;
                let (s, c) = angle.sin_cos();
                (
                    cx + rx * grow * c + self.off(spread, 1.0),
                    cy + ry * grow * s + self.off(spread, 1.0),
                )
            })
            .collect()
    }
}

/// Points of a curve no closer than a few units, so a curve drawn through
/// them, each a little off, wobbles rather than shivers; a short curve
/// keeps four or so, so it stays a curve. The ends stay.
fn thin(points: &[Point], length: f32) -> Vec<Point> {
    let apart = (length / 4.0).min(10.0);
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = vec![points[0]];
    for &p in &points[1..points.len() - 1] {
        if path::distance(*out.last().expect("one point"), p) >= apart {
            out.push(p);
        }
    }
    let end = points[points.len() - 1];
    if out.len() > 1 && path::distance(*out.last().expect("one point"), end) < apart / 2.0 {
        out.pop();
    }
    out.push(end);
    out
}

/// A smooth curve through `points` from the first, which the path is at.
fn catmull_rom(d: &mut String, points: &[Point]) {
    let count = points.len();
    for i in 0..count.saturating_sub(1) {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(count - 1)];
        let first = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let second = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        let _ = write!(
            d,
            "C{},{} {},{} {},{}",
            n(first.0),
            n(first.1),
            n(second.0),
            n(second.1),
            n(p2.0),
            n(p2.1)
        );
    }
}

/// A seed from `bytes` (FNV-1a), mixed with `salt`.
pub(crate) fn seed(bytes: &[u8], salt: u64) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// A seed from a shape's own size and form, wherever it stands: its
/// points less the first, to a unit.
pub(crate) fn shape_seed(kind: u8, points: &[Point]) -> u64 {
    let mut bytes = vec![kind];
    if let Some(&(x0, y0)) = points.first() {
        for &(x, y) in points {
            bytes.extend_from_slice(&((x - x0).round() as i32).to_le_bytes());
            bytes.extend_from_slice(&((y - y0).round() as i32).to_le_bytes());
        }
    }
    seed(&bytes, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(d: &str) -> Vec<f32> {
        d.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter(|s| !s.is_empty())
            .map(|s| s.parse().unwrap())
            .collect()
    }

    #[test]
    fn the_same_seed_draws_the_same_strokes() {
        let outline = Outline::rect((0.0, 0.0, 80.0, 30.0), 6.0);
        let a = Hand::new(7).stroke(std::slice::from_ref(&outline), true);
        let b = Hand::new(7).stroke(std::slice::from_ref(&outline), true);
        let c = Hand::new(8).stroke(std::slice::from_ref(&outline), true);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn strokes_stay_near_their_outline() {
        let outline = Outline::rect((10.0, 10.0, 120.0, 40.0), 0.0);
        let d = Hand::new(3).stroke(&[outline], true);
        // Every rectangle side is drawn twice.
        assert_eq!(d.matches('M').count(), 8, "{d}");
        for pair in numbers(&d).chunks(2) {
            let (x, y) = (pair[0], pair[1]);
            assert!(
                (2.0..=138.0).contains(&x) && (2.0..=58.0).contains(&y),
                "{x},{y} in {d}"
            );
        }
    }

    #[test]
    fn an_open_line_keeps_its_ends() {
        let outline = Outline::polyline(&[(0.0, 0.0), (100.0, 0.0), (100.0, 80.0)], false);
        let d = Hand::new(11).stroke(&[outline], true);
        // Two strokes, each from the first point to the last.
        assert_eq!(d.matches('M').count(), 2, "{d}");
        assert!(d.starts_with("M0,0C"), "{d}");
        assert_eq!(d.matches(" 100,80").count(), 2, "{d}");
    }

    #[test]
    fn a_fill_closes_along_its_outline() {
        let outline = Outline::rect((10.0, 20.0, 90.0, 40.0), 0.0);
        let d = Hand::new(4).fill(&[outline]);
        assert!(d.ends_with('Z'), "{d}");
        assert!(d.contains('C'), "{d}");
        assert!(!d.contains('L'), "{d}");
        for pair in numbers(&d).chunks(2) {
            let (x, y) = (pair[0], pair[1]);
            assert!(
                (4.0..=106.0).contains(&x) && (14.0..=66.0).contains(&y),
                "{x},{y} in {d}"
            );
        }
    }

    #[test]
    fn a_short_run_strays_little() {
        let outline = Outline::polyline(&[(0.0, 0.0), (6.0, 0.0), (3.0, 5.0)], true);
        let d = Hand::new(5).stroke(&[outline], true);
        for pair in numbers(&d).chunks(2) {
            assert!(
                (-1.5..=7.5).contains(&pair[0]) && (-1.5..=6.5).contains(&pair[1]),
                "{d}"
            );
        }
    }

    #[test]
    fn an_ellipse_goes_round_and_its_fill_closes() {
        let mut hand = Hand::new(1);
        let d = hand.ellipse_stroke((50.0, 50.0), (20.0, 10.0));
        for pair in numbers(&d).chunks(2) {
            let (dx, dy) = ((pair[0] - 50.0) / 20.0, (pair[1] - 50.0) / 10.0);
            let r = (dx * dx + dy * dy).sqrt();
            assert!((0.6..1.4).contains(&r), "{r} in {d}");
        }
        assert!(hand.ellipse_fill((50.0, 50.0), (20.0, 10.0)).ends_with('Z'));
    }

    #[test]
    fn a_shape_seed_is_where_it_stands_aside() {
        let at = |x: f32| shape_seed(1, &[(x, 0.0), (x + 50.0, 0.0), (x + 50.0, 20.0)]);
        assert_eq!(at(0.0), at(120.0));
        assert_ne!(
            at(0.0),
            shape_seed(1, &[(0.0, 0.0), (60.0, 0.0), (60.0, 20.0)])
        );
        assert_ne!(seed(b"Idle", 0), seed(b"Idle", 1));
    }
}
