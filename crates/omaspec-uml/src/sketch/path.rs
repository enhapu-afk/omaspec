//! An SVG path's `d` read into outlines a sketch can draw: straight runs
//! stay runs, curves and arcs become points close enough together to draw
//! again as a curve.

use super::{Outline, Piece, Point};

/// Points a curve is cut into per unit of its length, at least.
const SPACING: f32 = 4.0;

/// The outlines of `d`, one per subpath; `None` when `d` is not a path
/// this reader knows, so the shape is drawn as it is.
pub(crate) fn outlines(d: &str) -> Option<Vec<Outline>> {
    let mut tokens = Tokens::new(d);
    let mut out = Vec::new();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut at: Point = (0.0, 0.0);
    let mut start: Point = (0.0, 0.0);
    // The second handle of the last cubic or the handle of the last
    // quadratic, for `S` and `T`.
    let mut last_cubic: Option<Point> = None;
    let mut last_quad: Option<Point> = None;
    let mut command = None;
    let flush = |pieces: &mut Vec<Piece>, out: &mut Vec<Outline>, closed: bool| {
        if !pieces.is_empty() {
            out.push(Outline {
                pieces: std::mem::take(pieces),
                closed,
            });
        }
    };
    loop {
        tokens.skip_separators();
        if tokens.done() {
            break;
        }
        let c = match tokens.command() {
            Some(c) => c,
            None => match command {
                // A number after a command repeats it; after a move, as a line.
                Some('M') => 'L',
                Some('m') => 'l',
                Some(c) if !matches!(c, 'Z' | 'z') => c,
                _ => return None,
            },
        };
        command = Some(c);
        let relative = c.is_ascii_lowercase();
        let base = if relative { at } else { (0.0, 0.0) };
        let point = |tokens: &mut Tokens| -> Option<Point> {
            let x = tokens.number()?;
            let y = tokens.number()?;
            Some((base.0 + x, base.1 + y))
        };
        let (cubic, quad) = (last_cubic.take(), last_quad.take());
        match c.to_ascii_uppercase() {
            'M' => {
                flush(&mut pieces, &mut out, false);
                at = point(&mut tokens)?;
                start = at;
            }
            'L' => {
                let to = point(&mut tokens)?;
                pieces.push(Piece::Line(at, to));
                at = to;
            }
            'H' => {
                let x = tokens.number()? + if relative { at.0 } else { 0.0 };
                let to = (x, at.1);
                pieces.push(Piece::Line(at, to));
                at = to;
            }
            'V' => {
                let y = tokens.number()? + if relative { at.1 } else { 0.0 };
                let to = (at.0, y);
                pieces.push(Piece::Line(at, to));
                at = to;
            }
            'C' | 'S' => {
                let first = if c.eq_ignore_ascii_case(&'C') {
                    point(&mut tokens)?
                } else {
                    cubic.map_or(at, |h| (2.0 * at.0 - h.0, 2.0 * at.1 - h.1))
                };
                let second = point(&mut tokens)?;
                let to = point(&mut tokens)?;
                pieces.push(Piece::Curve(cubic_points(at, first, second, to)));
                last_cubic = Some(second);
                at = to;
            }
            'Q' | 'T' => {
                let handle = if c.eq_ignore_ascii_case(&'Q') {
                    point(&mut tokens)?
                } else {
                    quad.map_or(at, |h| (2.0 * at.0 - h.0, 2.0 * at.1 - h.1))
                };
                let to = point(&mut tokens)?;
                // A quadratic is the cubic with handles two thirds of the way.
                let first = lerp(at, handle, 2.0 / 3.0);
                let second = lerp(to, handle, 2.0 / 3.0);
                pieces.push(Piece::Curve(cubic_points(at, first, second, to)));
                last_quad = Some(handle);
                at = to;
            }
            'A' => {
                let rx = tokens.number()?;
                let ry = tokens.number()?;
                let rotation = tokens.number()?;
                let large = tokens.flag()?;
                let sweep = tokens.flag()?;
                let to = point(&mut tokens)?;
                let points = arc_points(at, (rx, ry), rotation, large, sweep, to);
                pieces.push(if points.len() > 2 {
                    Piece::Curve(points)
                } else {
                    Piece::Line(at, to)
                });
                at = to;
            }
            'Z' => {
                if distance(at, start) > 0.01 {
                    pieces.push(Piece::Line(at, start));
                }
                at = start;
                flush(&mut pieces, &mut out, true);
            }
            _ => return None,
        }
    }
    flush(&mut pieces, &mut out, false);
    Some(out)
}

fn lerp(a: Point, b: Point, t: f32) -> Point {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

pub(crate) fn distance(a: Point, b: Point) -> f32 {
    ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt()
}

/// Points along a cubic from `a` to `d`, `a` and `d` among them.
pub(crate) fn cubic_points(a: Point, b: Point, c: Point, d: Point) -> Vec<Point> {
    let length = distance(a, b) + distance(b, c) + distance(c, d);
    let steps = ((length / SPACING).ceil() as usize).clamp(2, 64);
    (0..=steps)
        .map(|step| {
            let t = step as f32 / steps as f32;
            let s = 1.0 - t;
            let (w0, w1, w2, w3) = (s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t);
            (
                w0 * a.0 + w1 * b.0 + w2 * c.0 + w3 * d.0,
                w0 * a.1 + w1 * b.1 + w2 * c.1 + w3 * d.1,
            )
        })
        .collect()
}

/// Points along an arc as SVG writes it, from its ends (the SVG
/// specification's conversion to a center, F.6.5); just the ends when the
/// arc is a line.
fn arc_points(
    from: Point,
    (rx, ry): (f32, f32),
    rotation: f32,
    large: bool,
    sweep: bool,
    to: Point,
) -> Vec<Point> {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-3 || ry < 1e-3 || distance(from, to) < 1e-3 {
        return vec![from, to];
    }
    let phi = rotation.to_radians();
    let (sin, cos) = phi.sin_cos();
    let (dx, dy) = ((from.0 - to.0) / 2.0, (from.1 - to.1) / 2.0);
    let x1 = cos * dx + sin * dy;
    let y1 = -sin * dx + cos * dy;
    // Radii too small to reach are scaled up, as SVG does.
    let grow = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if grow > 1.0 {
        rx *= grow.sqrt();
        ry *= grow.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut factor = (num / den).max(0.0).sqrt();
    if large == sweep {
        factor = -factor;
    }
    let cx1 = factor * rx * y1 / ry;
    let cy1 = -factor * ry * x1 / rx;
    let cx = cos * cx1 - sin * cy1 + (from.0 + to.0) / 2.0;
    let cy = sin * cx1 + cos * cy1 + (from.1 + to.1) / 2.0;
    let angle = |ux: f32, uy: f32| uy.atan2(ux);
    let start = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
    let end = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry);
    let mut delta = end - start;
    let tau = std::f32::consts::TAU;
    if sweep && delta < 0.0 {
        delta += tau;
    } else if !sweep && delta > 0.0 {
        delta -= tau;
    }
    let length = delta.abs() * rx.max(ry);
    let steps = ((length / SPACING).ceil() as usize).clamp(2, 128);
    let mut points: Vec<Point> = (0..=steps)
        .map(|step| {
            let t = start + delta * step as f32 / steps as f32;
            let (s, c) = t.sin_cos();
            let (x, y) = (rx * c, ry * s);
            (cos * x - sin * y + cx, sin * x + cos * y + cy)
        })
        .collect();
    // The ends exactly as written.
    points[0] = from;
    *points.last_mut().expect("two points at least") = to;
    points
}

/// The numbers, flags and commands of a `d`.
struct Tokens<'a> {
    text: &'a [u8],
    at: usize,
}

impl<'a> Tokens<'a> {
    fn new(d: &'a str) -> Self {
        Tokens {
            text: d.as_bytes(),
            at: 0,
        }
    }

    fn done(&self) -> bool {
        self.at >= self.text.len()
    }

    fn skip_separators(&mut self) {
        while self
            .text
            .get(self.at)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b',')
        {
            self.at += 1;
        }
    }

    fn command(&mut self) -> Option<char> {
        self.skip_separators();
        let b = *self.text.get(self.at)?;
        if b"MmLlHhVvCcSsQqTtAaZz".contains(&b) {
            self.at += 1;
            Some(b as char)
        } else {
            None
        }
    }

    /// An arc's flag: `0` or `1`, which may run into the next number.
    fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        let b = *self.text.get(self.at)?;
        self.at += 1;
        match b {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }

    fn number(&mut self) -> Option<f32> {
        self.skip_separators();
        let start = self.at;
        let text = self.text;
        let mut i = self.at;
        if matches!(text.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let mut digits = false;
        while text.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
            digits = true;
        }
        if text.get(i) == Some(&b'.') {
            i += 1;
            while text.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
                digits = true;
            }
        }
        if !digits {
            return None;
        }
        if matches!(text.get(i), Some(b'e' | b'E')) {
            let mut j = i + 1;
            if matches!(text.get(j), Some(b'+' | b'-')) {
                j += 1;
            }
            if text.get(j).is_some_and(u8::is_ascii_digit) {
                while text.get(j).is_some_and(u8::is_ascii_digit) {
                    j += 1;
                }
                i = j;
            }
        }
        self.at = i;
        let value: f32 = std::str::from_utf8(&text[start..i]).ok()?.parse().ok()?;
        value.is_finite().then_some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ends(outline: &Outline) -> Vec<(Point, Point)> {
        outline
            .pieces
            .iter()
            .map(|piece| match piece {
                Piece::Line(a, b) => (*a, *b),
                Piece::Curve(points) => (points[0], *points.last().unwrap()),
            })
            .collect()
    }

    #[test]
    fn reads_lines_relative_and_absolute_and_closes() {
        let found = outlines("M10 10h20v10H10z M0,0 l5-5").unwrap();
        assert_eq!(found.len(), 2);
        assert!(found[0].closed && !found[1].closed);
        assert_eq!(
            ends(&found[0]),
            [
                ((10.0, 10.0), (30.0, 10.0)),
                ((30.0, 10.0), (30.0, 20.0)),
                ((30.0, 20.0), (10.0, 20.0)),
                ((10.0, 20.0), (10.0, 10.0)),
            ]
        );
        assert_eq!(ends(&found[1]), [((0.0, 0.0), (5.0, -5.0))]);
    }

    #[test]
    fn curves_and_arcs_keep_their_ends() {
        let found =
            outlines("M0,0 C10,0 20,10 20,20 Q20,30 10,30 A 10 10 0 0 1 0 20 a5,5 0 1,0 -5,5")
                .unwrap();
        let ends = ends(&found[0]);
        assert_eq!(ends[0], ((0.0, 0.0), (20.0, 20.0)));
        assert_eq!(ends[1], ((20.0, 20.0), (10.0, 30.0)));
        assert_eq!(ends[2], ((10.0, 30.0), (0.0, 20.0)));
        assert_eq!(ends[3], ((0.0, 20.0), (-5.0, 25.0)));
        // Sweeping clockwise (y down), the quarter circle of radius 10 runs
        // round (10, 20).
        let Piece::Curve(points) = &found[0].pieces[2] else {
            panic!("an arc is a curve");
        };
        let middle = points[points.len() / 2];
        let r = distance(middle, (10.0, 20.0));
        assert!((r - 10.0).abs() < 0.2, "{r}");
    }

    #[test]
    fn numbers_run_together_as_svg_allows() {
        let found = outlines("M.5.5L-1-1e1").unwrap();
        assert_eq!(ends(&found[0]), [((0.5, 0.5), (-1.0, -10.0))]);
        assert!(outlines("M0 0 X 1 1").is_none());
        assert!(outlines("M0").is_none());
    }
}
