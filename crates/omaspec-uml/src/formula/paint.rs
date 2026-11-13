//! Painting the set formula: its glyphs as text, its bars and radicals as
//! strokes, its tall brackets as paths.

use super::layout::{Curve, Face, Pack, layout, run_style};
use super::{DISPLAY, Diagram, MARGIN, MUTED, TITLE};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::Color;
use crate::svg::{self, Align, Canvas, Layer, Stroke};
use crate::text::{self, Family, Font};

/// The SVG. The formula is black on a white page; skin parameters do not
/// recolor it.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    let mut canvas = Canvas::new("serif", false);
    let family = canvas.family();
    let notes = Annotations::new(
        &diagram.frame,
        [
            Font::new(TITLE, Color::BLACK).bold(),
            Font::new(10.0, MUTED),
            Font::new(10.0, MUTED),
            Font::new(TITLE, Color::BLACK),
        ],
        family,
    )?;
    let face = Face {
        size: DISPLAY,
        family,
        in_script: false,
    };
    let pack = layout(&diagram.root, face);
    let baseline = notes.top() + pack.height;
    paint(&mut canvas, &pack, 0.0, baseline, family);
    // An italic leans past the box the roman width reserved.
    canvas.include(pack.width + face.size * 0.08, baseline);
    notes.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, Some(Color::WHITE)))
}

pub(super) fn paint(canvas: &mut Canvas, pack: &Pack, left: f32, baseline: f32, family: Family) {
    for curve in &pack.paths {
        let (d, bounds, width) = curve.stroke(left, baseline);
        canvas.path(
            Layer::Frames,
            &d,
            bounds,
            None,
            Some(Stroke::new(Color::BLACK, width)),
        );
    }
    for rule in &pack.rules {
        canvas.line(
            Layer::Frames,
            (left + rule.x1, baseline + rule.y),
            (left + rule.x2, baseline + rule.y),
            Stroke::new(Color::BLACK, rule.width),
        );
    }
    for g in &pack.glyphs {
        let style = run_style(g.size, g.italic);
        let font = Font::new(g.size, Color::BLACK);
        let block = text::styled(vec![vec![(g.text.clone(), style)]], font, family);
        canvas.text(
            Layer::Front,
            &block,
            (left + g.x, baseline + g.y),
            Align::Left,
        );
    }
}

impl Curve {
    pub(super) fn stroke(&self, left: f32, baseline: f32) -> (String, (f32, f32, f32, f32), f32) {
        let ox = left;
        let oy = baseline;
        match *self {
            Curve::Radical {
                x0,
                y0,
                x1,
                y1,
                x2,
                y2,
                x3,
                width,
            } => {
                let d = format!(
                    "M {} {} L {} {} L {} {} L {} {}",
                    svg::n(ox + x0),
                    svg::n(oy + y0),
                    svg::n(ox + x1),
                    svg::n(oy + y1),
                    svg::n(ox + x2),
                    svg::n(oy + y2),
                    svg::n(ox + x3),
                    svg::n(oy + y2),
                );
                let l = ox + x0.min(x1).min(x2).min(x3);
                let r = ox + x0.max(x1).max(x2).max(x3);
                let t = oy + y0.min(y1).min(y2);
                let b = oy + y0.max(y1).max(y2);
                (d, pad(l, t, r, b, width), width)
            }
            Curve::Paren {
                left: side,
                x,
                y0,
                y1,
                w,
                width,
            } => {
                let (d, l, r) = paren_path(side, ox + x, oy + y0, oy + y1, w);
                (d, pad(l, oy + y0, r, oy + y1, width), width)
            }
            Curve::Bracket {
                left: side,
                x,
                y0,
                y1,
                w,
                width,
            } => {
                let (d, l, r) = bracket_path(side, ox + x, oy + y0, oy + y1, w);
                (d, pad(l, oy + y0, r, oy + y1, width), width)
            }
            Curve::Brace {
                left: side,
                x,
                y0,
                y1,
                w,
                width,
            } => {
                let (d, l, r) = brace_path(side, ox + x, oy + y0, oy + y1, w);
                (d, pad(l, oy + y0, r, oy + y1, width), width)
            }
            Curve::Bar {
                x,
                y0,
                y1,
                double,
                gap,
                width,
            } => {
                let x = ox + x;
                let y0 = oy + y0;
                let y1 = oy + y1;
                let d = if double {
                    format!(
                        "M {} {} L {} {} M {} {} L {} {}",
                        svg::n(x),
                        svg::n(y0),
                        svg::n(x),
                        svg::n(y1),
                        svg::n(x + gap),
                        svg::n(y0),
                        svg::n(x + gap),
                        svg::n(y1),
                    )
                } else {
                    format!(
                        "M {} {} L {} {}",
                        svg::n(x),
                        svg::n(y0),
                        svg::n(x),
                        svg::n(y1)
                    )
                };
                let r = if double { x + gap } else { x };
                (d, pad(x, y0, r, y1, width), width)
            }
        }
    }
}

pub(super) fn pad(l: f32, t: f32, r: f32, b: f32, width: f32) -> (f32, f32, f32, f32) {
    let p = width / 2.0;
    (l - p, t - p, r + p, b + p)
}

pub(super) fn paren_path(left: bool, x: f32, y0: f32, y1: f32, w: f32) -> (String, f32, f32) {
    let h = y1 - y0;
    let c1 = y0 + h * 0.28;
    let c2 = y1 - h * 0.28;
    if left {
        let d = format!(
            "M {} {} C {} {} {} {} {} {}",
            svg::n(x + w),
            svg::n(y0),
            svg::n(x),
            svg::n(c1),
            svg::n(x),
            svg::n(c2),
            svg::n(x + w),
            svg::n(y1),
        );
        (d, x, x + w)
    } else {
        let d = format!(
            "M {} {} C {} {} {} {} {} {}",
            svg::n(x),
            svg::n(y0),
            svg::n(x + w),
            svg::n(c1),
            svg::n(x + w),
            svg::n(c2),
            svg::n(x),
            svg::n(y1),
        );
        (d, x, x + w)
    }
}

pub(super) fn bracket_path(left: bool, x: f32, y0: f32, y1: f32, w: f32) -> (String, f32, f32) {
    let d = if left {
        format!(
            "M {} {} H {} V {} H {}",
            svg::n(x + w),
            svg::n(y0),
            svg::n(x),
            svg::n(y1),
            svg::n(x + w),
        )
    } else {
        format!(
            "M {} {} H {} V {} H {}",
            svg::n(x),
            svg::n(y0),
            svg::n(x + w),
            svg::n(y1),
            svg::n(x),
        )
    };
    (d, x, x + w)
}

pub(super) fn brace_path(left: bool, x: f32, y0: f32, y1: f32, w: f32) -> (String, f32, f32) {
    let mid = (y0 + y1) / 2.0;
    let h = y1 - y0;
    let (tip, neck) = if left { (x, x + w) } else { (x + w, x) };
    let a = y0 + h * 0.18;
    let b = mid - h * 0.06;
    let c = mid + h * 0.06;
    let d = y1 - h * 0.18;
    let path = format!(
        "M {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {}",
        svg::n(neck),
        svg::n(y0),
        svg::n(tip),
        svg::n(y0),
        svg::n(tip),
        svg::n(a),
        svg::n(tip),
        svg::n(b),
        svg::n(tip),
        svg::n(mid),
        svg::n(neck),
        svg::n(mid),
        svg::n(neck),
        svg::n(mid),
        svg::n(neck),
        svg::n(mid),
        svg::n(tip),
        svg::n(mid),
        svg::n(tip),
        svg::n(c),
        svg::n(tip),
        svg::n(y1),
        svg::n(tip),
        svg::n(d),
        svg::n(neck),
        svg::n(y1),
    );
    (path, x, x + w)
}
