//! A small SVG writer: shapes on layers, text from measured [`Block`]s,
//! the size taken from what was drawn.

use std::fmt::Write as _;

use crate::color::{Color, Paint};
use crate::common::Scale;
use crate::map::Fit;
use crate::text::{Block, Family, Line, TABLE_PAD, TableRow};

/// Drawing order, back to front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Background,
    Lifelines,
    Frames,
    Bars,
    Front,
}

const LAYERS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    pub dash: Option<(f32, f32)>,
}

impl Stroke {
    pub fn new(color: Color, width: f32) -> Stroke {
        Stroke {
            color,
            width,
            dash: None,
        }
    }

    pub fn dashed(self, on: f32, off: f32) -> Stroke {
        Stroke {
            dash: Some((on, off)),
            ..self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

pub struct Canvas {
    layers: [String; LAYERS],
    /// The font family as written in the SVG.
    font: String,
    family: Family,
    monochrome: bool,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    scale: Option<Scale>,
}

impl Canvas {
    pub fn new(font: &str, monochrome: bool) -> Canvas {
        Canvas {
            layers: Default::default(),
            font: font.to_owned(),
            family: Family::of(font),
            monochrome,
            left: f32::INFINITY,
            top: f32::INFINITY,
            right: f32::NEG_INFINITY,
            bottom: f32::NEG_INFINITY,
            scale: None,
        }
    }

    /// Sizes the finished picture by `scale`; what is drawn stays as it is.
    pub fn scale(&mut self, scale: Scale) {
        self.scale = Some(scale);
    }

    pub fn family(&self) -> Family {
        self.family
    }

    fn grow(&mut self, x: f32, y: f32) {
        self.left = self.left.min(x);
        self.top = self.top.min(y);
        self.right = self.right.max(x);
        self.bottom = self.bottom.max(y);
    }

    /// Counts the point `(x, y)` as drawn.
    pub fn include(&mut self, x: f32, y: f32) {
        self.grow(x, y);
    }

    /// The left and right edge of everything drawn so far.
    pub fn width_span(&self) -> (f32, f32) {
        if self.left > self.right {
            (0.0, 0.0)
        } else {
            (self.left, self.right)
        }
    }

    /// The bottom edge of everything drawn so far.
    pub fn bottom(&self) -> f32 {
        if self.top > self.bottom {
            0.0
        } else {
            self.bottom
        }
    }

    fn color(&self, c: Color) -> Color {
        if self.monochrome { c.gray() } else { c }
    }

    fn paint(&self, fill: Paint, stroke: Option<Stroke>) -> String {
        let mut out = String::new();
        match fill {
            Some(c) => {
                let _ = write!(out, r#" fill="{}""#, self.color(c));
            }
            None => out.push_str(r#" fill="none""#),
        }
        if let Some(s) = stroke {
            let _ = write!(
                out,
                r#" stroke="{}" stroke-width="{}""#,
                self.color(s.color),
                n(s.width)
            );
            if let Some((on, off)) = s.dash {
                let _ = write!(out, r#" stroke-dasharray="{},{}""#, n(on), n(off));
            }
        }
        out
    }

    pub fn rect(
        &mut self,
        layer: Layer,
        (x, y, w, h): (f32, f32, f32, f32),
        rx: f32,
        fill: Paint,
        stroke: Option<Stroke>,
    ) {
        self.grow(x, y);
        self.grow(x + w, y + h);
        let paint = self.paint(fill, stroke);
        let round = if rx > 0.0 {
            format!(r#" rx="{0}" ry="{0}""#, n(rx))
        } else {
            String::new()
        };
        let _ = write!(
            self.layers[layer as usize],
            r#"<rect x="{}" y="{}" width="{}" height="{}"{round}{paint}/>"#,
            n(x),
            n(y),
            n(w.max(0.0)),
            n(h.max(0.0))
        );
    }

    pub fn line(
        &mut self,
        layer: Layer,
        (x1, y1): (f32, f32),
        (x2, y2): (f32, f32),
        stroke: Stroke,
    ) {
        self.grow(x1, y1);
        self.grow(x2, y2);
        let paint = self.paint(None, Some(stroke));
        let _ = write!(
            self.layers[layer as usize],
            r#"<line x1="{}" y1="{}" x2="{}" y2="{}"{paint}/>"#,
            n(x1),
            n(y1),
            n(x2),
            n(y2)
        );
    }

    pub fn polygon(
        &mut self,
        layer: Layer,
        points: &[(f32, f32)],
        fill: Paint,
        stroke: Option<Stroke>,
    ) {
        for &(x, y) in points {
            self.grow(x, y);
        }
        let paint = self.paint(fill, stroke);
        let points: Vec<String> = points
            .iter()
            .map(|&(x, y)| format!("{},{}", n(x), n(y)))
            .collect();
        let _ = write!(
            self.layers[layer as usize],
            r#"<polygon points="{}"{paint}/>"#,
            points.join(" ")
        );
    }

    pub fn polyline(&mut self, layer: Layer, points: &[(f32, f32)], stroke: Stroke) {
        for &(x, y) in points {
            self.grow(x, y);
        }
        let paint = self.paint(None, Some(stroke));
        let points: Vec<String> = points
            .iter()
            .map(|&(x, y)| format!("{},{}", n(x), n(y)))
            .collect();
        let _ = write!(
            self.layers[layer as usize],
            r#"<polyline points="{}"{paint}/>"#,
            points.join(" ")
        );
    }

    /// A line through `points` whose corners turn in curves of up to
    /// `radius` ([`crate::curve`]); with no radius, a polyline.
    pub fn curve(&mut self, layer: Layer, points: &[(f32, f32)], radius: f32, stroke: Stroke) {
        if radius <= 0.0 || points.len() < 3 {
            self.polyline(layer, points, stroke);
            return;
        }
        for &(x, y) in points {
            self.grow(x, y);
        }
        let paint = self.paint(None, Some(stroke));
        let _ = write!(
            self.layers[layer as usize],
            r#"<path d="{}"{paint}/>"#,
            crate::curve::path(points, radius)
        );
    }

    /// A path; `bounds` are its left, top, right and bottom edge, for the
    /// canvas size.
    pub fn path(
        &mut self,
        layer: Layer,
        d: &str,
        bounds: (f32, f32, f32, f32),
        fill: Paint,
        stroke: Option<Stroke>,
    ) {
        self.grow(bounds.0, bounds.1);
        self.grow(bounds.2, bounds.3);
        let paint = self.paint(fill, stroke);
        let _ = write!(self.layers[layer as usize], r#"<path d="{d}"{paint}/>"#);
    }

    pub fn ellipse(
        &mut self,
        layer: Layer,
        (cx, cy): (f32, f32),
        (rx, ry): (f32, f32),
        fill: Paint,
        stroke: Option<Stroke>,
    ) {
        self.grow(cx - rx, cy - ry);
        self.grow(cx + rx, cy + ry);
        let paint = self.paint(fill, stroke);
        let _ = write!(
            self.layers[layer as usize],
            r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}"{paint}/>"#,
            n(cx),
            n(cy),
            n(rx),
            n(ry)
        );
    }

    /// `block` with its top at `y`; `x` is its left edge, middle or right
    /// edge by `align`, and every line is aligned the same way.
    pub fn text(&mut self, layer: Layer, block: &Block, (x, y): (f32, f32), align: Align) {
        if block.is_empty() {
            return;
        }
        let mut out = String::new();
        let _ = write!(
            out,
            r#"<text font-family="{}" xml:space="preserve">"#,
            escape(&self.font)
        );
        let mut top = y;
        for line in &block.lines {
            let left = match align {
                Align::Left => x,
                Align::Center => x - line.width / 2.0,
                Align::Right => x - line.width,
            };
            self.grow(left, top);
            self.grow(left + line.width, top + line.height);
            if let Some(row) = &line.table {
                self.table_grid(layer, line, row, left, top);
                for cell in &row.cells {
                    let mut cell_top = top;
                    for inner in &cell.lines {
                        self.write_line(
                            &mut out,
                            inner,
                            left + cell.left + TABLE_PAD,
                            cell_top + inner.ascent,
                        );
                        cell_top += inner.height;
                    }
                }
            } else {
                self.write_line(&mut out, line, left, top + line.ascent);
            }
            top += line.height;
        }
        out.push_str("</text>");
        self.layers[layer as usize].push_str(&out);
    }

    /// The grid of one table row. The first row draws the top edge; every
    /// row draws its bottom and the verticals, so a shared edge is one line.
    fn table_grid(&mut self, layer: Layer, line: &Line, row: &TableRow, left: f32, top: f32) {
        let stroke = Stroke::new(row.color, 1.0);
        let bottom = top + line.height;
        let right = left + line.width;
        if row.top {
            self.line(layer, (left, top), (right, top), stroke);
        }
        self.line(layer, (left, bottom), (right, bottom), stroke);
        for cell in &row.cells {
            let x = left + cell.left;
            self.line(layer, (x, top), (x, bottom), stroke);
        }
        self.line(layer, (right, top), (right, bottom), stroke);
    }

    /// One line of `out`'s `<text>`, its first run anchored at `x` and `baseline`.
    fn write_line(&self, out: &mut String, line: &Line, x: f32, baseline: f32) {
        for (i, run) in line.runs.iter().enumerate() {
            let s = &run.style;
            out.push_str("<tspan");
            if i == 0 {
                let _ = write!(out, r#" x="{}" y="{}""#, n(x), n(baseline));
            }
            let _ = write!(
                out,
                r#" font-size="{}" fill="{}""#,
                n(s.size),
                self.color(s.color)
            );
            if s.bold {
                out.push_str(r#" font-weight="bold""#);
            }
            if s.italic {
                out.push_str(r#" font-style="italic""#);
            }
            if s.mono {
                out.push_str(r#" font-family="monospace""#);
            }
            match (s.underline, s.strike) {
                (true, true) => out.push_str(r#" text-decoration="underline line-through""#),
                (true, false) => out.push_str(r#" text-decoration="underline""#),
                (false, true) => out.push_str(r#" text-decoration="line-through""#),
                _ => {}
            }
            if run.width > 0.0 {
                let _ = write!(
                    out,
                    r#" textLength="{}" lengthAdjust="spacing""#,
                    n(run.width)
                );
            }
            let _ = write!(out, ">{}</tspan>", escape(&run.text));
        }
    }

    /// The size of the finished picture with `margin` around what was
    /// drawn, before and after `scale`, and how far what was drawn moves.
    fn frame(&self, margin: f32) -> (f32, f32, f32, f32, f32, f32) {
        let (left, top, right, bottom) = if self.left > self.right {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (self.left, self.top, self.right, self.bottom)
        };
        let width = (right - left + 2.0 * margin).ceil();
        let height = (bottom - top + 2.0 * margin).ceil();
        let (shown_width, shown_height) = match self.scale {
            Some(scale) => {
                let f = scale.factor(width, height);
                ((width * f).ceil().max(1.0), (height * f).ceil().max(1.0))
            }
            None => (width, height),
        };
        (
            width,
            height,
            shown_width,
            shown_height,
            margin - left,
            margin - top,
        )
    }

    /// How what was drawn so far lands in the picture [`Self::finish`]
    /// makes with `margin`.
    pub fn fit(&self, margin: f32) -> Fit {
        let (width, height, shown_width, shown_height, dx, dy) = self.frame(margin);
        Fit {
            dx,
            dy,
            sx: shown_width / width.max(1.0),
            sy: shown_height / height.max(1.0),
        }
    }

    /// The finished SVG with `margin` around what was drawn, on a
    /// `background` if it is set.
    pub fn finish(self, margin: f32, background: Paint) -> String {
        let (width, height, shown_width, shown_height, dx, dy) = self.frame(margin);
        let mut out =
            String::with_capacity(self.layers.iter().map(String::len).sum::<usize>() + 256);
        let _ = write!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"#,
            n(shown_width),
            n(shown_height),
            n(width),
            n(height)
        );
        if let Some(bg) = background {
            let _ = write!(
                out,
                r#"<rect x="0" y="0" width="{}" height="{}" fill="{}"/>"#,
                n(width),
                n(height),
                self.color(bg)
            );
        }
        let _ = write!(out, r#"<g transform="translate({},{})">"#, n(dx), n(dy));
        for layer in &self.layers {
            if !layer.is_empty() {
                out.push_str("<g>");
                out.push_str(layer);
                out.push_str("</g>");
            }
        }
        out.push_str("</g></svg>");
        out
    }
}

/// A number with at most two decimals, without trailing zeros.
pub fn n(v: f32) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Not allowed in XML 1.0.
            c if c.is_control() && !matches!(c, '\t' | '\n' | '\r') => out.push('\u{fffd}'),
            '\u{fffe}' | '\u{ffff}' => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{Font, block};

    #[test]
    fn numbers_are_short() {
        assert_eq!(n(1.0), "1");
        assert_eq!(n(1.256), "1.26");
        assert_eq!(n(-0.001), "0");
        assert_eq!(n(f32::NAN), "0");
    }

    #[test]
    fn draws_and_escapes() {
        let mut c = Canvas::new("sans-serif", false);
        c.rect(
            Layer::Front,
            (5.0, 5.0, 20.0, 10.0),
            2.5,
            Some(Color::WHITE),
            Some(Stroke::new(Color::BLACK, 0.5)),
        );
        let b = block(
            "a < b & \"c\"\u{1}",
            Font::new(10.0, Color::BLACK),
            Family::Sans,
        )
        .unwrap();
        c.text(Layer::Front, &b, (5.0, 20.0), Align::Left);
        let svg = c.finish(5.0, None);
        assert!(svg.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" width="#));
        assert!(svg.contains(r##"<rect x="5" y="5" width="20" height="10" rx="2.5" ry="2.5" fill="#FFFFFF" stroke="#000000" stroke-width="0.5"/>"##));
        assert!(svg.contains(r#"<g transform="translate(0,0)">"#), "{svg}");
        assert!(svg.contains("a &lt; b &amp; &quot;c&quot;\u{fffd}"));
    }

    #[test]
    fn monochrome_grays_everything() {
        let mut c = Canvas::new("sans-serif", true);
        c.rect(
            Layer::Front,
            (0.0, 0.0, 1.0, 1.0),
            0.0,
            Some(Color(255, 0, 0)),
            None,
        );
        assert!(c.finish(0.0, None).contains("#4D4D4D"));
    }
}
