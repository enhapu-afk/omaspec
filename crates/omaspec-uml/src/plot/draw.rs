//! A chart drawn: the plot with its grid, its series and their values,
//! the axes with their ticks and titles around it, the annotations at
//! their places, and the legend at its side. One mapping takes a place
//! along the categories and a value to the page, so a horizontal chart is
//! the same chart turned: its bars lie across, its categories run down.

use super::axis::{Scale, said};
use super::legend::Legend;
use super::look::{Class, Look, Shape};
use super::{Axis, Diagram, Kind, Place, Series, Side, Values};
use crate::Declined;
use crate::annotations::Annotations;
use crate::chart::words;
use crate::color::Color;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{Block, Font};

/// The plot's least room, across and down.
const PLOT_W: f32 = 440.0;
const PLOT_H: f32 = 300.0;
/// The least room a category takes along its axis.
const SLOT: f32 = 48.0;
/// A tick's length, out from its axis.
const TICK: f32 = 5.0;
const GAP: f32 = 8.0;
const MARGIN: f32 = 10.0;
/// The room between the chart and its legend.
const LEGEND_GAP: f32 = 16.0;
/// A scatter's points across, and a line's thickness, unless styled.
pub(super) const MARKER: f32 = 8.0;
pub(super) const LINE: f32 = 2.0;
/// How far below its place an annotation with an arrow stands.
const ARROW: f32 = 30.0;

/// A series as it is drawn: its color, what its class and its part give
/// it, and for each value where it starts and ends along its axis.
pub(super) struct Drawn<'a> {
    pub series: &'a Series,
    pub color: Color,
    pub style: Class,
    pub spans: Vec<(f64, f64)>,
}

pub fn draw(d: &Diagram) -> Result<String, Declined> {
    let look = Look::read(&d.frame.skinparams)?;
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let muted = Font::new(10.0, look.ink.mix(look.page.unwrap_or(Color::WHITE), 0.35));
    let annotations = Annotations::new(&d.frame, [look.title, muted, muted, look.font], family)?;
    if d.horizontal && (d.v2.is_some() || d.series.iter().any(|s| s.v2)) {
        return Err(Declined::at(
            d.v2.as_ref().map_or(d.series[0].line, |axis| axis.line),
            "a second axis on a horizontal chart is not supported yet",
        ));
    }
    let drawn = drawn(d, &look)?;
    let (cat_axis, val_axis) = if d.horizontal {
        (&d.v, &d.h)
    } else {
        (&d.h, &d.v)
    };
    // The scales: the values on each value axis, the points' places.
    let values = |second: bool| -> Vec<f64> {
        let mut all: Vec<f64> = drawn
            .iter()
            .filter(|s| s.series.v2 == second)
            .flat_map(|s| match &s.series.values {
                Values::Each(_) => s
                    .spans
                    .iter()
                    .flat_map(|&(a, b)| [a, b])
                    .collect::<Vec<_>>(),
                Values::Points(points) => points.iter().map(|p| p.1).collect(),
            })
            .collect();
        if !second {
            all.extend(d.annotations.iter().map(|a| a.value));
        }
        all
    };
    let primary = Scale::of(val_axis, &values(false));
    let second =
        d.v2.as_ref()
            .map(|axis| Scale::of(axis, &values(true)))
            .or_else(|| {
                d.series
                    .iter()
                    .any(|s| s.v2)
                    .then(|| Scale::of(&Axis::default(), &values(true)))
            });
    let points = d
        .series
        .iter()
        .any(|s| matches!(s.values, Values::Points(_)));
    let places = points.then(|| {
        let xs: Vec<f64> = d
            .series
            .iter()
            .flat_map(|s| match &s.values {
                Values::Points(points) => points.iter().map(|p| p.0).collect(),
                Values::Each(_) => Vec::new(),
            })
            .collect();
        Scale::of(cat_axis, &xs)
    });
    let count = d
        .series
        .iter()
        .map(|s| match &s.values {
            Values::Each(values) => values.len(),
            Values::Points(_) => 0,
        })
        .max()
        .unwrap_or(0)
        .max(cat_axis.categories.len())
        .max(1);
    let label = |text: &str, font: Font| words(text, font, family);
    // What each axis says along it: its place (0 to 1) and its words.
    let category_labels: Vec<(f32, Block)> = match &places {
        Some(scale) => scale
            .ticks
            .iter()
            .map(|(v, t)| (scale.at(*v), label(t, look.tick_font)))
            .collect(),
        None => (0..count)
            .map(|i| {
                let name = cat_axis.categories.get(i).map_or("", String::as_str);
                ((i as f32 + 0.5) / count as f32, label(name, look.tick_font))
            })
            .collect(),
    };
    let ticks_of = |scale: &Scale| -> Vec<(f32, Block)> {
        scale
            .ticks
            .iter()
            .map(|(v, t)| (scale.at(*v), label(t, look.tick_font)))
            .collect()
    };
    let value_labels = ticks_of(&primary);
    let second_labels = second.as_ref().map(ticks_of).unwrap_or_default();
    let (left_labels, bottom_labels) = if d.horizontal {
        (&category_labels, &value_labels)
    } else {
        (&value_labels, &category_labels)
    };
    let widest = |labels: &[(f32, Block)]| labels.iter().map(|l| l.1.width).fold(0.0, f32::max);
    let tallest = |labels: &[(f32, Block)]| labels.iter().map(|l| l.1.height).fold(0.0, f32::max);
    // The plot's room: each category room for its words.
    let (width, height) = if d.horizontal {
        let slot = (tallest(&category_labels) + 14.0).max(36.0);
        (PLOT_W, (slot * count as f32).max(PLOT_H))
    } else if places.is_none() {
        let slot = (widest(&category_labels) + 12.0).max(SLOT);
        ((slot * count as f32).max(PLOT_W), PLOT_H)
    } else {
        (PLOT_W, PLOT_H)
    };
    let title_of = |axis: &Axis| axis.title.as_deref().map(|t| label(t, look.font));
    let (left_title, bottom_title) = (title_of(&d.v), title_of(&d.h));
    let right_title = d.v2.as_ref().and_then(title_of);
    let legend = Legend::of(d, &drawn, &look, family);
    // The room around the plot.
    let along = |title: &Option<Block>, top: bool| match title {
        Some(t) if !top => t.height + GAP,
        _ => 0.0,
    };
    let mut left = widest(left_labels) + TICK + GAP + along(&left_title, d.v.label_top);
    let right_axis = if second.is_some() {
        widest(&second_labels)
            + TICK
            + GAP
            + along(&right_title, d.v2.as_ref().is_some_and(|a| a.label_top))
    } else {
        0.0
    };
    let beside = match (&bottom_title, d.h.label_right) {
        (Some(t), true) => t.width + GAP,
        _ => 0.0,
    };
    let above = [
        (&left_title, d.v.label_top),
        (&right_title, d.v2.as_ref().is_some_and(|a| a.label_top)),
    ]
    .into_iter()
    .filter_map(|(t, top)| t.as_ref().filter(|_| top))
    .map(|t| t.height + GAP)
    .fold(0.0, f32::max)
    .max(if drawn.iter().any(|s| s.series.labels) {
        16.0
    } else {
        6.0
    });
    let below = tallest(bottom_labels)
        + TICK
        + GAP / 2.0
        + match (&bottom_title, d.h.label_right) {
            (Some(t), false) => t.height + GAP,
            _ => 0.0,
        };
    let mut top = annotations.top() + above;
    match (&legend, d.legend) {
        (Some(l), Some(Side::Top)) => top += l.height + LEGEND_GAP,
        (Some(l), Some(Side::Left)) => left += l.width + LEGEND_GAP,
        _ => {}
    }
    let (x0, y0) = (left, top);
    let pos = |c: f32, v: f32| -> (f32, f32) {
        if d.horizontal {
            (x0 + v * width, y0 + c * height)
        } else {
            (x0 + c * width, y0 + height - v * height)
        }
    };
    let plot = Plot { count };

    // The grid, faint, behind everything.
    canvas.begin_group(Layer::Background, crate::svg::GRID);
    let grid = |canvas: &mut Canvas, at: f32, across_values: bool| {
        // A line across the values stands at a place along the categories.
        let (a, b) = if across_values {
            (pos(at, 0.0), pos(at, 1.0))
        } else {
            (pos(0.0, at), pos(1.0, at))
        };
        canvas.line(Layer::Background, a, b, look.grid);
    };
    if val_axis.grid {
        for (at, _) in &value_labels {
            grid(&mut canvas, *at, false);
        }
    }
    if cat_axis.grid {
        for (at, _) in &category_labels {
            grid(&mut canvas, *at, true);
        }
    }
    if let (Some(axis), Some(scale)) = (&d.v2, &second)
        && axis.grid
    {
        for (v, _) in &scale.ticks {
            grid(&mut canvas, scale.at(*v), false);
        }
    }
    canvas.end_group(Layer::Background);

    // The series: areas at the back, then bars, lines and points.
    let scale_of = |s: &Series| match (&second, s.v2) {
        (Some(second), true) => second,
        _ => &primary,
    };
    let place_of = |i: usize, x: Option<f64>| -> f32 {
        match (x, &places) {
            (Some(x), Some(scale)) => scale.at(x),
            _ => (i as f32 + 0.5) / count as f32,
        }
    };
    for s in drawn.iter().filter(|s| s.series.kind == Kind::Area) {
        let scale = scale_of(s.series);
        let tops: Vec<(f32, f32)> = s
            .spans
            .iter()
            .enumerate()
            .map(|(i, span)| pos(place_of(i, None), scale.at(span.1)))
            .collect();
        let mut outline = tops.clone();
        outline.extend(
            s.spans
                .iter()
                .enumerate()
                .rev()
                .map(|(i, span)| pos(place_of(i, None), scale.at(span.0))),
        );
        let stroke = s
            .style
            .line
            .map(|c| Stroke::new(c, s.style.thickness.unwrap_or(1.0)));
        canvas.polygon(Layer::Frames, &outline, Some(s.color), stroke);
    }
    let lanes = bar_lanes(d, &drawn);
    for (s, lane) in drawn.iter().zip(&lanes) {
        let Some((lane, of)) = lane else { continue };
        let scale = scale_of(s.series);
        for (i, span) in s.spans.iter().enumerate() {
            let from = if d.stacked { span.0 } else { scale.base() };
            let (c0, c1) = plot.lane(i, *lane, *of, look.bar_width);
            let a = pos(c0, scale.at(from).clamp(0.0, 1.0));
            let b = pos(c1, scale.at(span.1).clamp(0.0, 1.0));
            let rect = (
                a.0.min(b.0),
                a.1.min(b.1),
                (a.0 - b.0).abs(),
                (a.1 - b.1).abs(),
            );
            canvas.rect(Layer::Frames, rect, 0.0, Some(s.color), look.bar_border);
        }
    }
    for s in drawn.iter().filter(|s| s.series.kind == Kind::Line) {
        let scale = scale_of(s.series);
        let points = series_points(s, scale, &place_of, &pos);
        let width = s.style.thickness.unwrap_or(LINE);
        canvas.polyline(Layer::Lines, &points, Stroke::new(s.color, width));
    }
    for s in drawn.iter().filter(|s| s.series.kind == Kind::Scatter) {
        let scale = scale_of(s.series);
        let shape = s.style.shape.unwrap_or(Shape::Circle);
        let size = s.style.size.unwrap_or(MARKER);
        for point in series_points(s, scale, &place_of, &pos) {
            marker(&mut canvas, Layer::Lines, point, shape, size, s.color);
        }
    }
    // The values the series write at them.
    for s in drawn.iter().filter(|s| s.series.labels) {
        let scale = scale_of(s.series);
        let lanes = lanes_of(&lanes, &drawn, s);
        for (i, span) in s.spans.iter().enumerate() {
            let value = span.1
                - if s.series.kind == Kind::Area || d.stacked && lanes.is_some() {
                    span.0
                } else {
                    0.0
                };
            let c = match lanes {
                Some((lane, of)) => {
                    let (c0, c1) = plot.lane(i, lane, of, look.bar_width);
                    (c0 + c1) / 2.0
                }
                None => place_of(i, None),
            };
            let end = pos(c, scale.at(span.1).clamp(0.0, 1.0));
            let block = label(&said(value), look.label_font);
            let at = if d.horizontal {
                (end.0 + 4.0, end.1 - block.height / 2.0)
            } else if value < 0.0 && lanes.is_some() {
                (end.0, end.1 + 3.0)
            } else {
                (end.0, end.1 - block.height - 3.0)
            };
            let align = if d.horizontal {
                Align::Left
            } else {
                Align::Center
            };
            canvas.text(Layer::Front, &block, at, align);
        }
    }

    // The axes: their lines, where 0 lies, the ticks and their words.
    let (x1, y1) = (x0 + width, y0 + height);
    canvas.line(Layer::Frames, (x0, y0), (x0, y1), look.axis);
    canvas.line(Layer::Frames, (x0, y1), (x1, y1), look.axis);
    if second.is_some() {
        canvas.line(Layer::Frames, (x1, y0), (x1, y1), look.axis);
    }
    if primary.crosses_zero() {
        canvas.line(
            Layer::Frames,
            pos(0.0, primary.at(0.0)),
            pos(1.0, primary.at(0.0)),
            look.axis,
        );
    }
    if let Some(scale) = places.as_ref().filter(|s| s.crosses_zero()) {
        canvas.line(
            Layer::Frames,
            pos(scale.at(0.0), 0.0),
            pos(scale.at(0.0), 1.0),
            look.axis,
        );
    }
    for (at, block) in left_labels {
        let y = if d.horizontal {
            y0 + at * height
        } else {
            y1 - at * height
        };
        canvas.line(Layer::Frames, (x0 - TICK, y), (x0, y), look.axis);
        canvas.text(
            Layer::Front,
            block,
            (x0 - TICK - 3.0, y - block.height / 2.0),
            Align::Right,
        );
    }
    for (at, block) in bottom_labels {
        let x = x0 + at * width;
        canvas.line(Layer::Frames, (x, y1), (x, y1 + TICK), look.axis);
        canvas.text(Layer::Front, block, (x, y1 + TICK + 2.0), Align::Center);
    }
    for (at, block) in &second_labels {
        let y = y1 - at * height;
        canvas.line(Layer::Frames, (x1, y), (x1 + TICK, y), look.axis);
        canvas.text(
            Layer::Front,
            block,
            (x1 + TICK + 3.0, y - block.height / 2.0),
            Align::Left,
        );
    }
    // The titles: along their axes, or across the far end.
    if let Some(t) = &left_title {
        if d.v.label_top {
            canvas.text(
                Layer::Front,
                t,
                (x0, y0 - t.height - GAP / 2.0),
                Align::Center,
            );
        } else {
            let x = x0 - TICK - GAP - widest(left_labels) - t.height;
            canvas.text_turned(
                Layer::Front,
                t,
                (x, y0 + height / 2.0),
                Align::Center,
                -90.0,
            );
        }
    }
    if let Some(t) = &right_title {
        if d.v2.as_ref().is_some_and(|a| a.label_top) {
            canvas.text(
                Layer::Front,
                t,
                (x1, y0 - t.height - GAP / 2.0),
                Align::Center,
            );
        } else {
            let x = x1 + TICK + GAP + widest(&second_labels) + t.height;
            canvas.text_turned(Layer::Front, t, (x, y0 + height / 2.0), Align::Center, 90.0);
        }
    }
    if let Some(t) = &bottom_title {
        if d.h.label_right {
            canvas.text(
                Layer::Front,
                t,
                (x1 + GAP, y1 - t.height / 2.0),
                Align::Left,
            );
        } else {
            let y = y1 + TICK + GAP / 2.0 + tallest(bottom_labels) + GAP / 2.0;
            canvas.text(Layer::Front, t, (x0 + width / 2.0, y), Align::Center);
        }
    }

    // The annotations, at their places.
    for note in &d.annotations {
        let c = match (&note.at, &places) {
            (Place::Category(i), _) => place_of(*i, None),
            (Place::At(x), Some(scale)) => scale.at(*x),
            (Place::At(_), None) => {
                return Err(Declined::at(
                    note.line,
                    "on a chart of categories an annotation stands at a category",
                ));
            }
        };
        let point = pos(c, primary.at(note.value));
        let block = label(&note.text, look.note_font);
        // The words on the page's color, so they read over a bar.
        let backed = |canvas: &mut Canvas, (x, y): (f32, f32)| {
            let ground = look.page.unwrap_or(Color::WHITE);
            let room = (
                x - block.width / 2.0 - 3.0,
                y - 1.0,
                block.width + 6.0,
                block.height + 2.0,
            );
            canvas.rect(Layer::Front, room, 3.0, Some(ground), None);
            canvas.text(Layer::Front, &block, (x, y), Align::Center);
        };
        if note.arrow {
            let below = point.1 + ARROW;
            canvas.line(
                Layer::Front,
                (point.0, below),
                (point.0, point.1 + 7.0),
                Stroke::new(look.note_line, 1.0),
            );
            canvas.polygon(
                Layer::Front,
                &[
                    point,
                    (point.0 - 4.0, point.1 + 8.0),
                    (point.0 + 4.0, point.1 + 8.0),
                ],
                Some(look.note_line),
                None,
            );
            backed(&mut canvas, (point.0, below + 2.0));
        } else {
            backed(&mut canvas, (point.0, point.1 - block.height - 4.0));
        }
    }

    // The legend, at its side.
    if let (Some(legend), Some(side)) = (&legend, d.legend) {
        let chart_left = x0 - widest(left_labels) - TICK - GAP - along(&left_title, d.v.label_top);
        let at = match side {
            Side::Right => (x1 + right_axis + beside + LEGEND_GAP, y0),
            Side::Left => (chart_left - LEGEND_GAP - legend.width, y0),
            Side::Top => (
                x0 + (width - legend.width) / 2.0,
                y0 - above - legend.height - LEGEND_GAP,
            ),
            Side::Bottom => (
                x0 + (width - legend.width) / 2.0,
                y1 + below + LEGEND_GAP / 2.0,
            ),
        };
        legend.draw(&mut canvas, &look, at);
    }
    annotations.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, look.page))
}

/// The plot's categories.
struct Plot {
    count: usize,
}

impl Plot {
    /// The room of bar lane `lane` of `of` in category `i`, along the
    /// categories (0 to 1): `share` of the category's room, in its middle.
    fn lane(&self, i: usize, lane: usize, of: usize, share: f32) -> (f32, f32) {
        let slot = 1.0 / self.count as f32;
        let group = slot * share;
        let each = group / of.max(1) as f32;
        let start = i as f32 * slot + (slot - group) / 2.0 + each * lane as f32;
        (start, start + each)
    }
}

/// Each series' place among the bars of a category, and how many share
/// it: every bar its own lane side by side, or, stacked, one lane for
/// each value axis.
fn bar_lanes(d: &Diagram, drawn: &[Drawn]) -> Vec<Option<(usize, usize)>> {
    let bars: Vec<usize> = (0..drawn.len())
        .filter(|&i| drawn[i].series.kind == Kind::Bar)
        .collect();
    let axes = bars.iter().any(|&i| drawn[i].series.v2) as usize
        + bars.iter().any(|&i| !drawn[i].series.v2) as usize;
    (0..drawn.len())
        .map(|i| {
            let lane = bars.iter().position(|&b| b == i)?;
            Some(if d.stacked {
                (usize::from(drawn[i].series.v2 && axes == 2), axes)
            } else {
                (lane, bars.len())
            })
        })
        .collect()
}

fn lanes_of(
    lanes: &[Option<(usize, usize)>],
    drawn: &[Drawn],
    s: &Drawn,
) -> Option<(usize, usize)> {
    let at = drawn.iter().position(|other| std::ptr::eq(other, s))?;
    lanes[at]
}

/// The places of a line's or a scatter's values on the page.
fn series_points(
    s: &Drawn,
    scale: &Scale,
    place_of: &dyn Fn(usize, Option<f64>) -> f32,
    pos: &dyn Fn(f32, f32) -> (f32, f32),
) -> Vec<(f32, f32)> {
    match &s.series.values {
        Values::Points(points) => points
            .iter()
            .map(|&(x, y)| pos(place_of(0, Some(x)), scale.at(y)))
            .collect(),
        Values::Each(_) => s
            .spans
            .iter()
            .enumerate()
            .map(|(i, span)| pos(place_of(i, None), scale.at(span.1)))
            .collect(),
    }
}

pub(super) fn marker(
    canvas: &mut Canvas,
    layer: Layer,
    (x, y): (f32, f32),
    shape: Shape,
    size: f32,
    color: Color,
) {
    let r = size / 2.0;
    match shape {
        Shape::Circle => canvas.ellipse(layer, (x, y), (r, r), Some(color), None),
        Shape::Square => canvas.rect(layer, (x - r, y - r, size, size), 0.0, Some(color), None),
        Shape::Triangle => canvas.polygon(
            layer,
            &[(x, y - r), (x + r, y + r * 0.8), (x - r, y + r * 0.8)],
            Some(color),
            None,
        ),
    }
}

/// The series with their colors and their spans: a bar from the value
/// axis's base, or, stacked, from the bars of its category before it; an
/// area always on the areas before it, as PlantUML stacks them.
fn drawn<'a>(d: &'a Diagram, look: &Look) -> Result<Vec<Drawn<'a>>, Declined> {
    use std::collections::HashMap;
    let mut up: HashMap<(bool, usize), f64> = HashMap::new();
    let mut down: HashMap<(bool, usize), f64> = HashMap::new();
    let mut areas: HashMap<(bool, usize), f64> = HashMap::new();
    let mut out = Vec::new();
    for (index, series) in d.series.iter().enumerate() {
        let class = series
            .class
            .as_ref()
            .and_then(|class| look.classes.get(class));
        let part = match series.kind {
            Kind::Bar => &look.bar,
            Kind::Line => &look.line,
            Kind::Area => &look.area,
            Kind::Scatter => &look.scatter,
        };
        let style = merged(class, part);
        let main = |c: &Class| match series.kind {
            Kind::Bar | Kind::Area => c.fill.or(c.line),
            Kind::Line => c.line.or(c.fill),
            Kind::Scatter => c.marker.or(c.fill),
        };
        // A color the chart names stays as written; the palette's, on a
        // line or a point, reads on the page.
        let color = series
            .color
            .or_else(|| class.and_then(main))
            .or_else(|| main(part))
            .unwrap_or_else(|| match series.kind {
                Kind::Line | Kind::Scatter => {
                    look.color(index).legible_on(look.ground(), look.ink, 3.0)
                }
                Kind::Bar | Kind::Area => look.color(index),
            });
        let spans: Vec<(f64, f64)> = match &series.values {
            Values::Points(points) => points.iter().map(|p| (p.1, p.1)).collect(),
            Values::Each(values) => values
                .iter()
                .enumerate()
                .map(|(i, &v)| {
                    let key = (series.v2, i);
                    let on = |stack: &mut HashMap<(bool, usize), f64>| {
                        let from = stack.get(&key).copied().unwrap_or(0.0);
                        stack.insert(key, from + v);
                        (from, from + v)
                    };
                    match series.kind {
                        Kind::Bar if d.stacked && v >= 0.0 => on(&mut up),
                        Kind::Bar if d.stacked => on(&mut down),
                        Kind::Bar => (0.0, v),
                        Kind::Area => on(&mut areas),
                        Kind::Line | Kind::Scatter => (v, v),
                    }
                })
                .collect(),
        };
        if spans.iter().any(|(a, b)| !(a.is_finite() && b.is_finite())) {
            return Err(Declined::refused(
                series.line,
                "these values add up to more than a chart can draw",
            ));
        }
        out.push(Drawn {
            series,
            color,
            style,
            spans,
        });
    }
    Ok(out)
}

/// A series' class over its part's style.
fn merged(class: Option<&Class>, part: &Class) -> Class {
    let class = class.cloned().unwrap_or_default();
    Class {
        fill: class.fill.or(part.fill),
        line: class.line.or(part.line),
        thickness: class.thickness.or(part.thickness),
        shape: class.shape.or(part.shape),
        size: class.size.or(part.size),
        marker: class.marker.or(part.marker),
    }
}

#[cfg(test)]
mod tests {
    /// The chart `text` drawn.
    fn svg(text: &str) -> String {
        crate::render(&format!("@startchart\n{text}\n@endchart\n")).unwrap()
    }

    /// The rectangles of `color`: x, y, width, height.
    fn rects(svg: &str, color: &str) -> Vec<[f32; 4]> {
        svg.split("<rect")
            .skip(1)
            .filter(|rect| rect.contains(&format!("fill=\"{color}\"")))
            .map(|rect| {
                let number = |name: &str| -> f32 {
                    let at = rect.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
                    rect[at..].split('"').next().unwrap().parse().unwrap()
                };
                [number("x"), number("y"), number("width"), number("height")]
            })
            .collect()
    }

    /// Grouped, two bars stand side by side in their category; stacked,
    /// the second stands on the first; turned, they lie across.
    #[test]
    fn bars_stand_side_by_side_on_one_another_or_across() {
        let series = "h-axis [A, B]\nv-axis 0 --> 100\nbar \"x\" [20, 40] #FF0000\nbar \"y\" [30, 10] #0000FF";
        let grouped = svg(series);
        let (red, blue) = (rects(&grouped, "#FF0000"), rects(&grouped, "#0000FF"));
        assert_eq!((red.len(), blue.len()), (2, 2));
        assert!(blue[0][0] > red[0][0] + red[0][2] - 0.5, "{red:?} {blue:?}");
        assert!((red[0][1] + red[0][3] - (blue[0][1] + blue[0][3])).abs() < 0.5);
        let stacked = svg(&format!("stackMode stacked\n{series}"));
        let (red, blue) = (rects(&stacked, "#FF0000"), rects(&stacked, "#0000FF"));
        assert!((red[0][0] - blue[0][0]).abs() < 0.5, "{red:?} {blue:?}");
        assert!(
            (blue[0][1] + blue[0][3] - red[0][1]).abs() < 0.5,
            "{red:?} {blue:?}"
        );
        // Thirty on twenty is half the axis: as high as twenty and thirty.
        assert!((red[0][3] / 20.0 - blue[0][3] / 30.0).abs() < 0.01);
        let turned =
            svg("orientation horizontal\nv-axis [A, B]\nh-axis 0 --> 100\nbar [80, 40] #FF0000");
        let bars = rects(&turned, "#FF0000");
        assert!(
            bars[0][2] > bars[0][3] && bars[0][2] > 2.0 * bars[1][2] - 0.5,
            "{bars:?}"
        );
    }

    /// The axes' words and titles, the legend's names, the values a
    /// series writes, an annotation and a style all reach the picture; a
    /// sum too large to draw is a mistake on its line.
    #[test]
    fn a_chart_draws_what_it_says() {
        let drawn = svg(
            "<style>\nchartDiagram {\n  scatter {\n    MarkerShape square\n  }\n}\n.hot {\n  MarkerColor #00FF00\n}\n</style>\nh-axis \"Quarter\" [Q1, Q2]\nv-axis \"Revenue\" 0 --> 100 spacing 50\nv2-axis \"Share\" 0 --> 10\nbar \"Sales\" [45, 62] labels\nline \"Share\" [3, 4] v2\nscatter <<hot>> \"Checks\" [40, 50]\narea \"Base\" [10, 10]\nannotation \"Peak\" at (Q2, 62) <<arrow>>\nlegend bottom",
        );
        for words in [
            ">Q1<",
            ">Revenue<",
            ">Share<",
            ">Sales<",
            ">Checks<",
            ">62<",
            ">Peak<",
            ">50<",
            ">10<",
        ] {
            assert!(drawn.contains(words), "{words}: {drawn}");
        }
        // The scatter's points, squares in the class's color.
        assert_eq!(
            rects(&drawn, "#00FF00").len(),
            2 + 1,
            "two points and the legend's"
        );
        let source = "@startchart\nstackMode stacked\nbar [1.7e308]\nbar [1.7e308]\n@endchart\n";
        let why = crate::render(source).unwrap_err();
        assert!(why.refused && why.line == Some(3), "{why:?}");
    }
}
