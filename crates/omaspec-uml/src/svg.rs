//! A small SVG writer: shapes on layers, text from measured [`Block`]s,
//! the size taken from what was drawn.

use std::fmt::Write as _;

use crate::color::{Color, Paint};
use crate::common::Scale;
use crate::map::Fit;
use crate::sketch::{self, Hand, Outline};
use crate::text::{Block, Family, Icon, Line, Link, Picture, TABLE_PAD, TableRow};

/// Drawing order, back to front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Background,
    Lifelines,
    Frames,
    /// The lines of a graph diagram with the marks at their ends, in a
    /// group of their own ([`LINES`]): a pane that glides the boxes to a
    /// new layout draws the lines itself meanwhile, over a picture without
    /// them ([`without_lines`]).
    Lines,
    Bars,
    Front,
}

const LAYERS: usize = 6;

/// The id of the group that holds the [`Layer::Lines`].
pub const LINES: &str = "lines";

/// The id of the group that holds a chart's grid: lines that only guide
/// the eye, drawn faint on purpose, as PlantUML draws them. A check of
/// how well a picture reads leaves them out.
pub const GRID: &str = "grid";

/// The id of the group around an icon drawn in a line of text: it is
/// seen as the words beside it are, against what lies under it.
pub const ICON: &str = "icon";

/// The handwriting face a sketched diagram's words are set in. Measured
/// widths stay those of the canvas font; each run is locked to its width.
pub const HAND_FONT: &str = "Excalifont";

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
    /// The sketch look ([`crate::sketch`]), when the diagram asks for it.
    sketch: Option<Sketch>,
    /// The boxes cast a shadow (`skinparam shadowing true`).
    shadows: bool,
    /// The shadows of each layer's shapes, drawn under that layer.
    shades: [String; LAYERS],
}

/// How far a shadow lies to the right of its shape and below it.
const SHADOW: f32 = 3.0;
/// A shape smaller than this casts none: an arrowhead, a mark on a line.
const SHADOW_FROM: f32 = 14.0;

/// What the sketch look seeds the next shape with.
#[derive(Debug, Clone, Copy, Default)]
struct Sketch {
    /// The element being drawn, by its name.
    name: Option<u64>,
    /// Shapes of that element drawn so far.
    count: u64,
    /// Inside [`Canvas::plain`]: this shape stays as it is.
    plain: bool,
}

/// The kinds of shape, for a seed from a shape's own form.
mod form {
    pub const RECT: u8 = 1;
    pub const LINE: u8 = 2;
    pub const POLYGON: u8 = 3;
    pub const POLYLINE: u8 = 4;
    pub const PATH: u8 = 5;
    pub const ELLIPSE: u8 = 6;
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
            sketch: None,
            shadows: false,
            shades: Default::default(),
        }
    }

    /// Lets every box drawn from here on cast a shadow when `on`, as
    /// PlantUML's `skinparam shadowing true` does: a filled shape with a
    /// border, down and to the right, under the shapes of its layer. The
    /// sketch look has none.
    pub fn shadows(&mut self, on: bool) {
        self.shadows = on;
    }

    /// The shadow of `shape`, an element without its paint and its end,
    /// `w` wide and `h` high.
    fn shade(&mut self, layer: Layer, shape: &str, (w, h): (f32, f32), fill: Paint, stroke: bool) {
        let boxed = fill.is_some() && stroke && w >= SHADOW_FROM && h >= SHADOW_FROM;
        if self.shadows && boxed && !matches!(layer, Layer::Background | Layer::Lines) {
            let _ = write!(self.shades[layer as usize], r##"{shape} fill="#000000"/>"##);
        }
    }

    /// Draws every shape from here on by hand when `on`
    /// ([`crate::sketch`]), and sets the words in [`HAND_FONT`].
    /// They stay measured as the font the canvas was made with, so a box
    /// keeps its size; [`Self::text`] locks each run to that width.
    pub fn sketch(&mut self, on: bool) {
        self.sketch = on.then(Sketch::default);
        if on {
            self.font = HAND_FONT.to_owned();
        }
    }

    /// Whether shapes are drawn by hand.
    pub fn sketched(&self) -> bool {
        self.sketch.is_some()
    }

    /// Whether the next shape goes through the hand. A [`Self::plain`]
    /// block draws as it is, even while the diagram is sketched.
    fn handed(&self) -> bool {
        self.sketch.is_some_and(|sketch| !sketch.plain)
    }

    /// Draws what `paint` draws as it is. A label's ground and a highlight
    /// stay straight, so they still cover the line or the band under them.
    pub fn plain<T>(&mut self, paint: impl FnOnce(&mut Canvas) -> T) -> T {
        let Some(before) = self.sketch else {
            return paint(self);
        };
        self.sketch = Some(Sketch {
            plain: true,
            ..before
        });
        let painted = paint(self);
        self.sketch = Some(before);
        painted
    }

    /// Draws with `paint` the shapes of the element called `name`: drawn
    /// by hand, they take their strokes from the name, so a box keeps them
    /// wherever the layout puts it.
    pub fn named<T>(&mut self, name: &str, paint: impl FnOnce(&mut Canvas) -> T) -> T {
        let Some(before) = self.sketch else {
            return paint(self);
        };
        self.sketch = Some(Sketch {
            name: Some(sketch::seed(name.as_bytes(), 0)),
            count: 0,
            plain: before.plain,
        });
        let painted = paint(self);
        self.sketch = Some(before);
        painted
    }

    /// A filled box whose edge is the whole shape. Drawn by hand it gains
    /// a rim, so a card that had no border still reads as drawn.
    pub fn card(&mut self, layer: Layer, rect: (f32, f32, f32, f32), rx: f32, fill: Color) {
        let stroke = self.handed().then(|| Stroke::new(rim(fill), 1.25));
        self.rect(layer, rect, rx, Some(fill), stroke);
    }

    /// The hand for the next shape: seeded by the element's name and how
    /// many of its shapes came before, or by the shape's own form.
    fn hand(&mut self, kind: u8, points: &[(f32, f32)]) -> Hand {
        let sketch = self.sketch.as_mut().expect("drawn by hand");
        let seed = match sketch.name {
            Some(name) => {
                sketch.count += 1;
                name ^ sketch::seed(&[kind], sketch.count)
            }
            None => sketch::shape_seed(kind, points),
        };
        Hand::new(seed)
    }

    /// `outlines` drawn by hand: the fill a little inside, the border over it.
    fn by_hand(
        &mut self,
        layer: Layer,
        hand: &mut Hand,
        outlines: &[Outline],
        fill: Paint,
        stroke: Stroke,
    ) {
        if fill.is_some() {
            let d = hand.fill(outlines);
            let paint = self.paint(fill, None);
            let _ = write!(self.layers[layer as usize], r#"<path d="{d}"{paint}/>"#);
        }
        let d = hand.stroke(outlines, stroke.dash.is_none());
        self.stroke_by_hand(layer, &d, stroke);
    }

    fn stroke_by_hand(&mut self, layer: Layer, d: &str, stroke: Stroke) {
        let paint = self.paint(None, Some(stroke));
        let _ = write!(
            self.layers[layer as usize],
            r#"<path d="{d}"{paint} stroke-linecap="round" stroke-linejoin="round"/>"#
        );
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

    /// The top edge of everything drawn so far.
    pub fn top(&self) -> f32 {
        if self.top > self.bottom {
            0.0
        } else {
            self.top
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
        if self.handed() && (stroke.is_some() || fill.is_some()) {
            let mut hand = self.hand(form::RECT, &[(0.0, 0.0), (w, h), (rx, 0.0)]);
            let outline = Outline::rect((x, y, w.max(0.0), h.max(0.0)), rx);
            if let Some(stroke) = stroke {
                self.by_hand(layer, &mut hand, &[outline], fill, stroke);
            } else {
                let d = hand.fill(&[outline]);
                let paint = self.paint(fill, None);
                let _ = write!(self.layers[layer as usize], r#"<path d="{d}"{paint}/>"#);
            }
            return;
        }
        let paint = self.paint(fill, stroke);
        let round = if rx > 0.0 {
            format!(r#" rx="{0}" ry="{0}""#, n(rx))
        } else {
            String::new()
        };
        let shape = format!(
            r#"<rect x="{}" y="{}" width="{}" height="{}"{round}"#,
            n(x),
            n(y),
            n(w.max(0.0)),
            n(h.max(0.0))
        );
        self.shade(layer, &shape, (w, h), fill, stroke.is_some());
        let _ = write!(self.layers[layer as usize], "{shape}{paint}/>");
    }

    /// What is drawn into `layer` until [`Self::end_group`] lies in a group
    /// with the id `id`, as the lines of a chart's [`GRID`].
    pub fn begin_group(&mut self, layer: Layer, id: &str) {
        let _ = write!(self.layers[layer as usize], r#"<g id="{id}">"#);
    }

    pub fn end_group(&mut self, layer: Layer) {
        self.layers[layer as usize].push_str("</g>");
    }

    /// Draws with `paint`; with a `link`, what it draws into each of
    /// `layers` is a link a click follows, one on each layer an element is
    /// drawn on.
    pub fn linked<T>(
        &mut self,
        link: Option<&Link>,
        layers: &[Layer],
        paint: impl FnOnce(&mut Canvas) -> T,
    ) -> T {
        let Some(link) = link else {
            return paint(self);
        };
        for &layer in layers {
            open_link(&mut self.layers[layer as usize], link);
        }
        let painted = paint(self);
        for &layer in layers {
            self.layers[layer as usize].push_str("</a>");
        }
        painted
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
        if self.handed() {
            let points = [(x1, y1), (x2, y2)];
            let mut hand = self.hand(form::LINE, &points);
            let d = hand.stroke(&[Outline::polyline(&points, false)], stroke.dash.is_none());
            self.stroke_by_hand(layer, &d, stroke);
            return;
        }
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
        if self.handed() && points.len() > 2 && (stroke.is_some() || fill.is_some()) {
            let mut hand = self.hand(form::POLYGON, points);
            let outline = Outline::polyline(points, true);
            // A mark with no border of its own, an arrowhead among them,
            // takes a stroke in its own ink so the edge is drawn too.
            let stroke = stroke.or_else(|| fill.map(|color| Stroke::new(color, 1.0)));
            if let Some(stroke) = stroke {
                self.by_hand(layer, &mut hand, &[outline], fill, stroke);
                return;
            }
        }
        let paint = self.paint(fill, stroke);
        let span = |of: fn(&(f32, f32)) -> f32| {
            let all = points.iter().map(of);
            all.clone().fold(f32::NEG_INFINITY, f32::max) - all.fold(f32::INFINITY, f32::min)
        };
        let size = (span(|p| p.0), span(|p| p.1));
        let points: Vec<String> = points
            .iter()
            .map(|&(x, y)| format!("{},{}", n(x), n(y)))
            .collect();
        let shape = format!(r#"<polygon points="{}""#, points.join(" "));
        self.shade(layer, &shape, size, fill, stroke.is_some());
        let _ = write!(self.layers[layer as usize], "{shape}{paint}/>");
    }

    pub fn polyline(&mut self, layer: Layer, points: &[(f32, f32)], stroke: Stroke) {
        for &(x, y) in points {
            self.grow(x, y);
        }
        if self.handed() && points.len() > 1 {
            let mut hand = self.hand(form::POLYLINE, points);
            let d = hand.stroke(&[Outline::polyline(points, false)], stroke.dash.is_none());
            self.stroke_by_hand(layer, &d, stroke);
            return;
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
        let d = crate::curve::path(points, radius);
        if self.handed()
            && let Some(outlines) = sketch::outlines(&d)
        {
            let mut hand = self.hand(form::POLYLINE, points);
            let d = hand.stroke(&outlines, stroke.dash.is_none());
            self.stroke_by_hand(layer, &d, stroke);
            return;
        }
        let paint = self.paint(None, Some(stroke));
        let _ = write!(self.layers[layer as usize], r#"<path d="{d}"{paint}/>"#);
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
        if self.handed()
            && (stroke.is_some() || fill.is_some())
            && let Some(outlines) = sketch::outlines(d)
        {
            let size = [(0.0, 0.0), (bounds.2 - bounds.0, bounds.3 - bounds.1)];
            let mut hand = self.hand(form::PATH, &size);
            let stroke = stroke.or_else(|| fill.map(|color| Stroke::new(color, 1.0)));
            if let Some(stroke) = stroke {
                self.by_hand(layer, &mut hand, &outlines, fill, stroke);
                return;
            }
        }
        let paint = self.paint(fill, stroke);
        let shape = format!(r#"<path d="{d}""#);
        let size = (bounds.2 - bounds.0, bounds.3 - bounds.1);
        self.shade(layer, &shape, size, fill, stroke.is_some());
        let _ = write!(self.layers[layer as usize], "{shape}{paint}/>");
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
        if self.handed() && rx > 0.0 && ry > 0.0 && (stroke.is_some() || fill.is_some()) {
            let mut hand = self.hand(form::ELLIPSE, &[(0.0, 0.0), (rx, ry)]);
            if fill.is_some() {
                let d = hand.ellipse_fill((cx, cy), (rx, ry));
                let paint = self.paint(fill, None);
                let _ = write!(self.layers[layer as usize], r#"<path d="{d}"{paint}/>"#);
            }
            if let Some(stroke) = stroke {
                let d = hand.ellipse_stroke((cx, cy), (rx, ry));
                self.stroke_by_hand(layer, &d, stroke);
            }
            return;
        }
        let paint = self.paint(fill, stroke);
        let shape = format!(
            r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}""#,
            n(cx),
            n(cy),
            n(rx),
            n(ry)
        );
        self.shade(layer, &shape, (2.0 * rx, 2.0 * ry), fill, stroke.is_some());
        let _ = write!(self.layers[layer as usize], "{shape}{paint}/>");
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
        // Icons stand beside the words, drawn after the text element.
        let mut icons: Vec<(f32, f32, Icon, Color)> = Vec::new();
        for line in &block.lines {
            let left = match align {
                Align::Left => x,
                // An item of a list starts at the block's left edge.
                Align::Center if line.flush => x - block.width / 2.0,
                Align::Right if line.flush => x - block.width,
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
                            &mut icons,
                        );
                        cell_top += inner.height;
                    }
                }
            } else {
                self.write_line(&mut out, line, left, top + line.ascent, &mut icons);
            }
            top += line.height;
        }
        out.push_str("</text>");
        for (x, baseline, icon, color) in icons {
            let _ = write!(out, r#"<g id="{ICON}">"#);
            self.icon(&mut out, &icon, (x, baseline), color);
            out.push_str("</g>");
        }
        self.layers[layer as usize].push_str(&out);
    }

    /// `icon` with its left edge at `x`, standing on `baseline`, in
    /// `color` unless it has its own.
    fn icon(&self, out: &mut String, icon: &Icon, (x, baseline): (f32, f32), color: Color) {
        let fill = self.color(icon.color.unwrap_or(color));
        match &icon.picture {
            Picture::Arrow(arrow) => {
                let d = arrow_path(*arrow, x, baseline, icon.width, icon.height);
                let width = (icon.height * 0.075).max(0.8);
                let _ = write!(
                    out,
                    r#"<path d="{d}" fill="none" stroke="{fill}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                    n(width)
                );
            }
            Picture::Line { d, filled } => {
                let scale = icon.height / 16.0;
                let top = baseline - icon.height;
                let paint = if *filled {
                    format!(r#"fill="{fill}""#)
                } else {
                    format!(
                        r#"fill="none" stroke="{fill}" stroke-width="1.2" stroke-linejoin="round" stroke-linecap="round""#
                    )
                };
                let _ = write!(
                    out,
                    r#"<path d="{d}" transform="translate({} {}) scale({})" {paint}/>"#,
                    n(x),
                    n(top),
                    scale
                );
            }
            Picture::Pixels(sprite) => {
                // A path for each level, its pixels run by run.
                let unit = icon.height / sprite.height.max(1) as f32;
                let top = baseline - icon.height;
                let mut paths: [String; 16] = Default::default();
                for y in 0..sprite.height {
                    let row = &sprite.levels[y * sprite.width..(y + 1) * sprite.width];
                    let mut x0 = 0;
                    while x0 < row.len() {
                        let level = row[x0];
                        let mut x1 = x0 + 1;
                        while x1 < row.len() && row[x1] == level {
                            x1 += 1;
                        }
                        if level > 0 {
                            let _ = write!(
                                paths[usize::from(level.min(15))],
                                "M{} {}h{}v{}h-{}z",
                                n(x + x0 as f32 * unit),
                                n(top + y as f32 * unit),
                                n((x1 - x0) as f32 * unit),
                                n(unit),
                                n((x1 - x0) as f32 * unit)
                            );
                        }
                        x0 = x1;
                    }
                }
                for (level, d) in paths.iter().enumerate().filter(|(_, d)| !d.is_empty()) {
                    let opacity = level as f32 / 15.0;
                    if level == 15 {
                        let _ = write!(out, r#"<path d="{d}" fill="{fill}"/>"#);
                    } else {
                        let _ = write!(
                            out,
                            r#"<path d="{d}" fill="{fill}" fill-opacity="{}"/>"#,
                            n(opacity)
                        );
                    }
                }
            }
            Picture::Path { d, dx, dy } => {
                let scale = icon.height / crate::icons::SIDE;
                let top = baseline - icon.height;
                let _ = write!(
                    out,
                    r#"<path d="{d}" transform="translate({} {}) scale({}) translate({} {})" fill="{fill}"/>"#,
                    n(x),
                    n(top),
                    scale,
                    n(*dx),
                    n(*dy)
                );
            }
        }
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

    /// One line of `out`'s `<text>`, its first run anchored at `x` and
    /// `baseline`; its icons go to `icons`, with where they stand, and the
    /// words after one start where it ends.
    fn write_line(
        &self,
        out: &mut String,
        line: &Line,
        x: f32,
        baseline: f32,
        icons: &mut Vec<(f32, f32, Icon, Color)>,
    ) {
        let mut at = x;
        let mut placed = false;
        // The handwriting face has no arrows: drawn in its hand instead.
        let runs = if self.font == HAND_FONT {
            hand_arrows(&line.runs, self.family)
        } else {
            std::borrow::Cow::Borrowed(line.runs.as_slice())
        };
        for run in runs.iter() {
            let s = &run.style;
            if let Some(icon) = &run.icon {
                icons.push((at, baseline, icon.clone(), s.color));
                at += run.width;
                placed = false;
                continue;
            }
            if let Some(link) = &run.link {
                open_link(out, link);
            }
            out.push_str("<tspan");
            if !placed {
                let _ = write!(out, r#" x="{}" y="{}""#, n(at), n(baseline));
                placed = true;
            }
            at += run.width;
            let _ = write!(
                out,
                r#" font-size="{}" fill="{}""#,
                n(s.size),
                self.color(s.color)
            );
            // The handwriting face has a regular cut only. A bold or an
            // italic ask would send usvg to a plain face for that run.
            let hand = self.font == HAND_FONT && !s.mono;
            if s.bold && !hand {
                out.push_str(r#" font-weight="bold""#);
            }
            if s.italic && !hand {
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
            if run.link.is_some() {
                out.push_str("</a>");
            }
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
        for (index, layer) in self.layers.iter().enumerate() {
            // One group, so shadows that overlap are no darker there.
            let shade = &self.shades[index];
            if !shade.is_empty() {
                let _ = write!(
                    out,
                    r#"<g transform="translate({0},{0})" opacity="0.22">{shade}</g>"#,
                    n(SHADOW)
                );
            }
            if !layer.is_empty() {
                if index == Layer::Lines as usize {
                    let _ = write!(out, r#"<g id="{LINES}">"#);
                } else {
                    out.push_str("<g>");
                }
                out.push_str(layer);
                out.push_str("</g>");
            }
        }
        out.push_str("</g></svg>");
        out
    }
}

/// The arrows the handwriting face lacks.
const ARROWS: [char; 6] = ['→', '←', '↑', '↓', '↔', '⇒'];

/// `runs` with each arrow of [`ARROWS`] a picture of its own, as wide as
/// the face would set it, drawn by hand: the words around it keep their
/// widths in the same proportion.
fn hand_arrows(
    runs: &[crate::text::Run],
    family: Family,
) -> std::borrow::Cow<'_, [crate::text::Run]> {
    use crate::text::{Run, measure};
    if !runs
        .iter()
        .any(|run| run.icon.is_none() && !run.style.mono && run.text.contains(ARROWS))
    {
        return std::borrow::Cow::Borrowed(runs);
    }
    let mut out: Vec<Run> = Vec::with_capacity(runs.len() + 2);
    for run in runs {
        if run.icon.is_some() || run.style.mono || !run.text.contains(ARROWS) {
            out.push(run.clone());
            continue;
        }
        let whole = measure(&run.text, &run.style, family);
        let share = if whole > 0.0 { run.width / whole } else { 1.0 };
        let mut words = String::new();
        let flush = |words: &mut String, out: &mut Vec<Run>| {
            if !words.is_empty() {
                let text = std::mem::take(words);
                out.push(Run {
                    width: measure(&text, &run.style, family) * share,
                    text,
                    ..run.clone()
                });
            }
        };
        for c in run.text.chars() {
            if ARROWS.contains(&c) {
                flush(&mut words, &mut out);
                out.push(Run {
                    text: String::new(),
                    width: measure(&c.to_string(), &run.style, family) * share,
                    icon: Some(Icon {
                        picture: Picture::Arrow(c),
                        width: measure(&c.to_string(), &run.style, family) * share,
                        height: run.style.size,
                        color: None,
                    }),
                    ..run.clone()
                });
            } else {
                words.push(c);
            }
        }
        flush(&mut words, &mut out);
    }
    std::borrow::Cow::Owned(out)
}

/// An arrow drawn as a hand draws it, in a box `width` wide on `baseline`
/// for text of `size`: a shaft at the height of a dash with a head.
fn arrow_path(arrow: char, x: f32, baseline: f32, width: f32, size: f32) -> String {
    let middle = baseline - size * 0.3;
    let (left, right) = (x + width * 0.12, x + width * 0.88);
    let head = (size * 0.22).min(width * 0.4);
    let horizontal = |from: f32, to: f32| {
        // A shaft with a slight bow, and the head at `to`.
        let back = if to > from { -head } else { head };
        format!(
            "M{} {}Q{} {} {} {}M{} {}L{} {}L{} {}",
            n(from),
            n(middle + 0.4),
            n((from + to) / 2.0),
            n(middle - 0.6),
            n(to),
            n(middle),
            n(to + back),
            n(middle - head * 0.75),
            n(to),
            n(middle),
            n(to + back),
            n(middle + head * 0.75)
        )
    };
    match arrow {
        '←' => horizontal(right, left),
        '↔' => {
            let mut d = horizontal(left, right);
            d.push_str(&format!(
                "M{} {}L{} {}L{} {}",
                n(left + head),
                n(middle - head * 0.75),
                n(left),
                n(middle),
                n(left + head),
                n(middle + head * 0.75)
            ));
            d
        }
        '⇒' => {
            let gap = size * 0.08;
            format!(
                "M{} {}L{} {}M{} {}L{} {}M{} {}L{} {}L{} {}",
                n(left),
                n(middle - gap),
                n(right - head * 0.4),
                n(middle - gap),
                n(left),
                n(middle + gap),
                n(right - head * 0.4),
                n(middle + gap),
                n(right - head),
                n(middle - head * 0.85),
                n(right),
                n(middle),
                n(right - head),
                n(middle + head * 0.85)
            )
        }
        '↑' | '↓' => {
            let centre = x + width / 2.0;
            let (top, bottom) = (baseline - size * 0.72, baseline - size * 0.02);
            let (from, to, back) = if arrow == '↑' {
                (bottom, top, head)
            } else {
                (top, bottom, -head)
            };
            format!(
                "M{} {}Q{} {} {} {}M{} {}L{} {}L{} {}",
                n(centre + 0.3),
                n(from),
                n(centre - 0.5),
                n((from + to) / 2.0),
                n(centre),
                n(to),
                n(centre - head * 0.75),
                n(to + back),
                n(centre),
                n(to),
                n(centre + head * 0.75),
                n(to + back)
            )
        }
        _ => horizontal(left, right),
    }
}

/// `svg` without its [`Layer::Lines`] group; `None` when it has none.
pub fn without_lines(svg: &str) -> Option<String> {
    let open = format!(r#"<g id="{LINES}">"#);
    let start = svg.find(&open)?;
    let mut depth = 0usize;
    let mut at = start;
    let rest = &svg[start..];
    // Groups open and close in pairs; the one that brings the depth back
    // to zero closes the layer.
    let mut index = 0;
    while index < rest.len() {
        let here = &rest[index..];
        if here.starts_with("</g>") {
            depth -= 1;
            index += "</g>".len();
            if depth == 0 {
                at = start + index;
                break;
            }
        } else if here.starts_with("<g>") || here.starts_with("<g ") {
            depth += 1;
            index += 2;
        } else {
            index += here.chars().next().map_or(1, char::len_utf8);
        }
    }
    (at > start).then(|| format!("{}{}", &svg[..start], &svg[at..]))
}

/// A number with at most two decimals, without trailing zeros.
pub fn n(v: f32) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{:.2}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// `<a>` to `link`, with its tooltip as the title.
fn open_link(out: &mut String, link: &Link) {
    let _ = write!(out, r#"<a href="{}">"#, escape(&link.url));
    if let Some(tooltip) = &link.tooltip {
        let _ = write!(out, "<title>{}</title>", escape(tooltip));
    }
}

pub(crate) fn escape(s: &str) -> String {
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

/// A rim that reads on `fill`: darker on a light card, lighter on a dark one.
fn rim(fill: Color) -> Color {
    if fill.is_dark() {
        fill.mix(Color::WHITE, 0.5)
    } else {
        fill.mix(Color::BLACK, 0.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{Font, block};

    #[test]
    fn the_lines_layer_is_a_group_of_its_own_that_can_be_left_out() {
        let mut c = Canvas::new("sans-serif", false);
        c.rect(
            Layer::Frames,
            (0.0, 0.0, 20.0, 10.0),
            0.0,
            Some(Color::WHITE),
            None,
        );
        c.begin_group(Layer::Lines, "wire");
        c.polyline(
            Layer::Lines,
            &[(0.0, 5.0), (30.0, 5.0)],
            Stroke::new(Color::BLACK, 1.0),
        );
        c.end_group(Layer::Lines);
        c.polygon(
            Layer::Lines,
            &[(30.0, 5.0), (26.0, 2.0), (26.0, 8.0)],
            Some(Color::BLACK),
            None,
        );
        c.rect(
            Layer::Front,
            (40.0, 0.0, 5.0, 5.0),
            0.0,
            Some(Color::BLACK),
            None,
        );
        let svg = c.finish(0.0, None);
        assert!(
            svg.contains(r#"<g id="lines"><g id="wire"><polyline"#),
            "{svg}"
        );
        let bare = without_lines(&svg).expect("has lines");
        assert!(
            !bare.contains("<polyline") && !bare.contains("<polygon"),
            "{bare}"
        );
        assert!(
            bare.contains("<rect x=\"0\"") && bare.contains("<rect x=\"40\""),
            "{bare}"
        );
        assert_eq!(bare.matches("<g").count(), bare.matches("</g>").count());
        assert!(without_lines("<svg><g><rect/></g></svg>").is_none());
    }

    #[test]
    fn numbers_are_short() {
        assert_eq!(n(1.0), "1");
        assert_eq!(n(1.256), "1.26");
        assert_eq!(n(-0.001), "0");
        assert_eq!(n(f32::NAN), "0");
    }

    #[test]
    fn a_box_casts_a_shadow_under_its_layer_and_a_mark_none() {
        let ink = Color(0, 0, 0);
        let white = Some(Color(255, 255, 255));
        let draw = |shadows: bool| {
            let mut canvas = Canvas::new("sans-serif", false);
            canvas.shadows(shadows);
            let stroke = Some(Stroke::new(ink, 1.0));
            canvas.rect(Layer::Front, (0.0, 0.0, 80.0, 30.0), 4.0, white, stroke);
            canvas.ellipse(Layer::Front, (120.0, 15.0), (20.0, 15.0), white, stroke);
            // An arrowhead, a line and a box without a border cast none.
            canvas.polygon(
                Layer::Front,
                &[(0.0, 50.0), (8.0, 54.0), (0.0, 58.0)],
                white,
                stroke,
            );
            canvas.rect(Layer::Front, (0.0, 70.0, 80.0, 30.0), 0.0, white, None);
            canvas.finish(10.0, None)
        };
        let plain = draw(false);
        assert!(!plain.contains("opacity"), "{plain}");
        let cast = draw(true);
        let shade = cast.find(r#"opacity="0.22""#).expect("a shadow");
        assert!(shade < cast.find(r##"fill="#FFFFFF""##).unwrap(), "{cast}");
        assert_eq!(cast.matches(r##"fill="#000000"/>"##).count(), 2, "{cast}");
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
    fn a_sketch_sets_the_handwriting_face_and_not_a_bold_one() {
        let mut c = Canvas::new("sans-serif", false);
        c.sketch(true);
        let word = block(
            "Book",
            Font::new(12.0, Color::BLACK).bold().italic(),
            Family::Sans,
        )
        .unwrap();
        c.text(Layer::Front, &word, (4.0, 4.0), Align::Left);
        let svg = c.finish(4.0, None);
        assert!(
            svg.contains(&format!("font-family=\"{HAND_FONT}\"")),
            "{svg}"
        );
        assert!(!svg.contains("font-weight"), "{svg}");
        assert!(!svg.contains("font-style"), "{svg}");
        // A monospaced run keeps its face.
        let code = block("\"\"id\"\"", Font::new(12.0, Color::BLACK), Family::Sans).unwrap();
        let mut c = Canvas::new("sans-serif", false);
        c.sketch(true);
        c.text(Layer::Front, &code, (4.0, 4.0), Align::Left);
        let svg = c.finish(4.0, None);
        assert!(svg.contains(r#"font-family="monospace""#), "{svg}");
    }

    #[test]
    fn a_card_with_no_border_is_drawn_by_hand() {
        let mut c = Canvas::new("sans-serif", false);
        c.sketch(true);
        c.card(
            Layer::Bars,
            (0.0, 0.0, 80.0, 40.0),
            6.0,
            Color::hex(0x88AADD),
        );
        let svg = c.finish(0.0, None);
        assert!(!svg.contains("<rect"), "{svg}");
        assert!(svg.contains(r#"stroke-linecap="round""#), "{svg}");
    }

    #[test]
    fn a_fill_with_no_border_is_drawn_by_hand() {
        let mut c = Canvas::new("sans-serif", false);
        c.sketch(true);
        c.rect(
            Layer::Bars,
            (0.0, 0.0, 40.0, 20.0),
            4.0,
            Some(Color::hex(0xDDEEFF)),
            None,
        );
        let svg = c.finish(0.0, None);
        assert!(!svg.contains("<rect"), "{svg}");
        assert!(svg.contains("<path"), "{svg}");
    }

    #[test]
    fn a_label_ground_stays_straight() {
        let mut c = Canvas::new("sans-serif", false);
        c.sketch(true);
        c.plain(|c| {
            c.rect(
                Layer::Frames,
                (0.0, 0.0, 40.0, 16.0),
                3.0,
                Some(Color::WHITE),
                None,
            );
        });
        let svg = c.finish(0.0, None);
        assert!(svg.contains("<rect"), "{svg}");
        assert!(!svg.contains(r#"stroke-linecap="round""#), "{svg}");
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
