//! An XY chart (`xychart-beta`): bars and lines over categories or a range
//! on the x-axis, a y-axis with its range or the data's, a grid faint as
//! a chart's is. Bars of several series stand side by side in their
//! category; a line has a dot at each value. A horizontal chart is left
//! to mmdr.

use super::{Look, body, number, said, title, unquote, words};
use crate::Declined;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::Font;

const WIDTH: f32 = 560.0;
const HEIGHT: f32 = 320.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<String>,
    pub x_title: Option<String>,
    /// The categories, or none for a range.
    pub categories: Vec<String>,
    pub x_range: Option<(f64, f64)>,
    pub y_title: Option<String>,
    pub y_range: Option<(f64, f64)>,
    pub series: Vec<Series>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub bar: bool,
    pub name: Option<String>,
    pub values: Vec<f64>,
}

/// The chart of `source`; `None` when it is none this drawer reads.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    let (title, lines) = body(source, "xychart-beta").or_else(|| body(source, "xychart"))?;
    // `xychart-beta horizontal` is left to mmdr.
    let head = source
        .lines()
        .map(str::trim)
        .find(|l| l.to_ascii_lowercase().starts_with("xychart"))?;
    if head.to_ascii_lowercase().contains("horizontal") {
        return None;
    }
    let mut d = Diagram {
        title,
        x_title: None,
        categories: Vec::new(),
        x_range: None,
        y_title: None,
        y_range: None,
        series: Vec::new(),
    };
    for (_, line) in lines {
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        match word {
            "x-axis" => {
                let (axis_title, rest) = titled(rest);
                d.x_title = axis_title;
                if let Some(list) = rest.strip_prefix('[') {
                    d.categories = list
                        .strip_suffix(']')?
                        .split(',')
                        .map(|c| unquote(c.trim()).to_owned())
                        .filter(|c| !c.is_empty())
                        .collect();
                } else if !rest.is_empty() {
                    d.x_range = Some(range(rest)?);
                }
            }
            "y-axis" => {
                let (axis_title, rest) = titled(rest);
                d.y_title = axis_title;
                if !rest.is_empty() {
                    d.y_range = Some(range(rest)?);
                }
            }
            "bar" | "line" => {
                let (name, rest) = titled(rest);
                let values = rest
                    .strip_prefix('[')?
                    .strip_suffix(']')?
                    .split(',')
                    .map(number)
                    .collect::<Option<Vec<f64>>>()?;
                d.series.push(Series {
                    bar: word == "bar",
                    name,
                    values,
                });
            }
            _ => return None,
        }
    }
    (!d.series.is_empty()).then_some(d)
}

/// A title in quotes or one word before the rest, if one is.
fn titled(text: &str) -> (Option<String>, &str) {
    let text = text.trim();
    if let Some(after) = text.strip_prefix('"')
        && let Some(end) = after.find('"')
    {
        return (Some(after[..end].to_owned()), after[end + 1..].trim());
    }
    if text.starts_with('[') || text.contains("-->") || text.is_empty() {
        return (None, text);
    }
    match text.split_once(char::is_whitespace) {
        Some((word, rest)) if rest.trim().starts_with('[') || rest.contains("-->") => {
            (Some(word.to_owned()), rest.trim())
        }
        None if number(text).is_none() => (Some(text.to_owned()), ""),
        _ => (None, text),
    }
}

/// `4000 --> 11000`.
fn range(text: &str) -> Option<(f64, f64)> {
    let (low, high) = text.split_once("-->")?;
    let (low, high) = (number(low)?, number(high)?);
    (high > low).then_some((low, high))
}

/// Ticks a reader takes in: steps of 1, 2 or 5 times a power of ten.
fn ticks(low: f64, high: f64) -> Vec<f64> {
    let rough = (high - low) / 5.0;
    let power = 10f64.powf(rough.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * power)
        .find(|s| *s >= rough)
        .unwrap_or(power * 10.0);
    let mut out = Vec::new();
    let mut v = (low / step).ceil() * step;
    while v <= high + step * 1e-9 {
        out.push(v);
        v += step;
    }
    out
}

pub fn draw(d: &Diagram, look: &Look) -> Result<String, Declined> {
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let small = Font::new((look.size - 2.0).max(8.0), look.ink);
    let all: Vec<f64> = d
        .series
        .iter()
        .flat_map(|s| s.values.iter().copied())
        .collect();
    let (low, high) = d.y_range.unwrap_or_else(|| {
        let lo = all.iter().copied().fold(f64::MAX, f64::min).min(0.0);
        let hi = all.iter().copied().fold(f64::MIN, f64::max);
        (lo, if hi > lo { hi } else { lo + 1.0 })
    });
    let marks = ticks(low, high);
    let reach = marks
        .iter()
        .map(|v| words(&said(*v), small, family).width)
        .fold(0.0, f32::max);
    let y_title = d.y_title.as_deref().map(|t| words(t, small, family));
    let left = reach + 12.0 + y_title.as_ref().map_or(0.0, |b| b.width + 10.0);
    let top = title(&mut canvas, d.title.as_deref(), look, left + WIDTH / 2.0) + 6.0;
    let y_of = |v: f64| top + HEIGHT - ((v - low) / (high - low)) as f32 * HEIGHT;
    // The grid, faint, then the axes.
    canvas.begin_group(Layer::Background, crate::svg::GRID);
    let grid = Stroke::new(look.line.mix(look.ground(), 0.75), 1.0);
    for v in &marks {
        canvas.line(
            Layer::Background,
            (left, y_of(*v)),
            (left + WIDTH, y_of(*v)),
            grid,
        );
    }
    canvas.end_group(Layer::Background);
    let axis = Stroke::new(look.line, 1.0);
    canvas.line(Layer::Frames, (left, top), (left, top + HEIGHT), axis);
    canvas.line(
        Layer::Frames,
        (left, top + HEIGHT),
        (left + WIDTH, top + HEIGHT),
        axis,
    );
    for v in &marks {
        let label = words(&said(*v), small, family);
        canvas.text(
            Layer::Front,
            &label,
            (left - 6.0, y_of(*v) - label.height / 2.0),
            Align::Right,
        );
    }
    if let Some(block) = &y_title {
        canvas.text(
            Layer::Front,
            block,
            (0.0, top + HEIGHT / 2.0 - block.height / 2.0),
            Align::Left,
        );
    }
    // The categories, or the range's steps, along the x-axis.
    let count = d
        .series
        .iter()
        .map(|s| s.values.len())
        .max()
        .unwrap_or(0)
        .max(d.categories.len())
        .max(1);
    let slot = WIDTH / count as f32;
    let names: Vec<String> = match (d.categories.is_empty(), d.x_range) {
        (false, _) => d.categories.clone(),
        (true, Some((a, b))) => (0..count)
            .map(|i| said(a + (b - a) * i as f64 / (count.max(2) - 1) as f64))
            .collect(),
        _ => (1..=count).map(|i| i.to_string()).collect(),
    };
    for (i, name) in names.iter().enumerate().take(count) {
        let label = words(name, small, family);
        canvas.text(
            Layer::Front,
            &label,
            (left + slot * (i as f32 + 0.5), top + HEIGHT + 6.0),
            Align::Center,
        );
    }
    if let Some(t) = &d.x_title {
        let block = words(t, small, family);
        canvas.text(
            Layer::Front,
            &block,
            (left + WIDTH / 2.0, top + HEIGHT + 26.0),
            Align::Center,
        );
    }
    // Bars side by side, then lines over them.
    let bars: Vec<usize> = (0..d.series.len()).filter(|&k| d.series[k].bar).collect();
    let bar_w = slot * 0.7 / bars.len().max(1) as f32;
    for (b, &k) in bars.iter().enumerate() {
        let color = look.color(k);
        for (i, v) in d.series[k].values.iter().enumerate() {
            let x = left + slot * i as f32 + slot * 0.15 + bar_w * b as f32;
            let (y0, y1) = (y_of(low.max(0.0).min(high)), y_of(*v));
            canvas.rect(
                Layer::Frames,
                (x, y0.min(y1), bar_w * 0.92, (y0 - y1).abs()),
                0.0,
                Some(color),
                None,
            );
        }
    }
    for (k, series) in d.series.iter().enumerate().filter(|(_, s)| !s.bar) {
        let color = look.color(k).legible_on(look.ground(), look.ink, 3.0);
        let points: Vec<(f32, f32)> = series
            .values
            .iter()
            .enumerate()
            .map(|(i, v)| (left + slot * (i as f32 + 0.5), y_of(*v)))
            .collect();
        canvas.polyline(Layer::Lines, &points, Stroke::new(color, 2.0));
        for p in &points {
            canvas.ellipse(Layer::Lines, *p, (3.0, 3.0), Some(color), None);
        }
    }
    Ok(canvas.finish(10.0, look.page))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_and_lines_are_read_with_their_axes() {
        let d = from_mermaid(
            "xychart-beta\n    title \"Sales\"\n    x-axis [jan, feb, mar]\n    y-axis \"Revenue\" 4000 --> 11000\n    bar [5000, 6000, 7500]\n    line [5000, 6000, 7500]\n",
        )
        .unwrap();
        assert_eq!(d.title.as_deref(), Some("Sales"));
        assert_eq!(d.categories, ["jan", "feb", "mar"]);
        assert_eq!(
            (d.y_title.as_deref(), d.y_range),
            (Some("Revenue"), Some((4000.0, 11000.0)))
        );
        assert!(d.series[0].bar && !d.series[1].bar);
        assert_eq!(ticks(4000.0, 11000.0), [4000.0, 6000.0, 8000.0, 10000.0]);
        let svg = draw(&d, &Look::default()).unwrap();
        assert!(svg.contains(">feb<") && svg.contains(">10000<"), "{svg}");
        assert!(from_mermaid("xychart-beta horizontal\nbar [1, 2]\n").is_none());
        assert!(from_mermaid("xychart-beta\nbar [1, x]\n").is_none());
    }
}
