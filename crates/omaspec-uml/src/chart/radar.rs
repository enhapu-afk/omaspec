//! A radar (`radar-beta`): an axis for each thing measured, from the
//! middle out, rings for the scale, and each curve a shape over the axes
//! in a color of its own, said in a legend.

use super::{Look, body, number, said, title, unquote, words};
use crate::Declined;
use crate::color::Color;
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::Font;

/// The radius of the outer ring.
const RADIUS: f32 = 140.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<String>,
    /// Each axis's label.
    pub axes: Vec<String>,
    /// Each curve's label and its value on every axis.
    pub curves: Vec<(String, Vec<f64>)>,
    pub min: f64,
    pub max: f64,
    pub ticks: usize,
    /// Rings as polygons rather than circles.
    pub polygon: bool,
    pub legend: bool,
}

/// The radar of `source`; `None` when it is none this drawer reads.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    let (title, lines) = body(source, "radar-beta").or_else(|| body(source, "radar"))?;
    let mut ids: Vec<String> = Vec::new();
    let mut axes: Vec<String> = Vec::new();
    let mut raw: Vec<(String, String)> = Vec::new();
    let (mut min, mut max, mut ticks, mut polygon, mut legend) = (None, None, 5, false, true);
    for (_, line) in lines {
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let rest = rest.trim();
        match word {
            "axis" => {
                for item in split(rest, ',') {
                    let (id, label) = labelled(item)?;
                    ids.push(id.to_owned());
                    axes.push(label.to_owned());
                }
            }
            "curve" => {
                let open = rest.find('{')?;
                let close = rest.rfind('}').filter(|close| *close > open)?;
                let (_, label) = labelled(rest[..open].trim())?;
                raw.push((label.to_owned(), rest[open + 1..close].to_owned()));
            }
            "min" => min = Some(number(rest)?),
            "max" => max = Some(number(rest)?),
            "ticks" => ticks = rest.parse().ok().filter(|t| (1..=20).contains(t))?,
            "graticule" => polygon = rest == "polygon",
            "showLegend" => legend = rest != "false",
            _ => return None,
        }
    }
    if axes.len() < 3 {
        return None;
    }
    // Values by place, or by the axes' ids (`{ m: 85, s: 90 }`).
    let mut curves = Vec::new();
    for (label, values) in raw {
        let mut row = vec![0.0; axes.len()];
        for (i, item) in split(&values, ',').into_iter().enumerate() {
            match item.split_once(':') {
                Some((id, value)) => {
                    let at = ids.iter().position(|a| a == id.trim())?;
                    row[at] = number(value)?;
                }
                None => *row.get_mut(i)? = number(item)?,
            }
        }
        curves.push((label, row));
    }
    let highest = curves
        .iter()
        .flat_map(|(_, v)| v.iter().copied())
        .fold(0.0, f64::max);
    let max = max.unwrap_or(highest.max(1.0));
    let min = min.unwrap_or(0.0);
    (max > min).then_some(Diagram {
        title,
        axes,
        curves,
        min,
        max,
        ticks,
        polygon,
        legend,
    })
}

pub fn draw(d: &Diagram, look: &Look) -> Result<String, Declined> {
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let label_font = Font::new(look.size - 1.0, look.ink);
    let reach = d
        .axes
        .iter()
        .map(|a| words(a, label_font, family).width)
        .fold(0.0, f32::max);
    let cx = reach + RADIUS + 16.0;
    let top = title(&mut canvas, d.title.as_deref(), look, cx);
    let cy = top + RADIUS + 22.0;
    let count = d.axes.len();
    let angle =
        |i: usize| -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / count as f32;
    let at = |i: usize, r: f32| (cx + r * angle(i).cos(), cy + r * angle(i).sin());
    let grid = Stroke::new(look.line.mix(look.ground(), 0.55), 1.0);
    // The rings, faint as a chart's grid is, then the axes.
    canvas.begin_group(Layer::Background, crate::svg::GRID);
    for t in 1..=d.ticks {
        let r = RADIUS * t as f32 / d.ticks as f32;
        if d.polygon {
            let points: Vec<(f32, f32)> = (0..count).map(|i| at(i, r)).collect();
            canvas.polygon(Layer::Background, &points, None, Some(grid));
        } else {
            canvas.ellipse(Layer::Background, (cx, cy), (r, r), None, Some(grid));
        }
    }
    canvas.end_group(Layer::Background);
    for (i, axis) in d.axes.iter().enumerate() {
        let end = at(i, RADIUS);
        canvas.line(
            Layer::Background,
            (cx, cy),
            end,
            Stroke::new(look.line, 1.0),
        );
        let label = words(axis, label_font, family);
        let (lx, ly) = at(i, RADIUS + 10.0);
        let align = if (lx - cx).abs() < 4.0 {
            Align::Center
        } else if lx > cx {
            Align::Left
        } else {
            Align::Right
        };
        let y = if ly < cy - 4.0 {
            ly - label.height
        } else if ly > cy + 4.0 {
            ly
        } else {
            ly - label.height / 2.0
        };
        canvas.text(Layer::Front, &label, (lx, y), align);
    }
    // Each curve a shape over the axes.
    let span = (d.max - d.min) as f32;
    for (k, (_, values)) in d.curves.iter().enumerate() {
        let color = look.color(k);
        let stroke_color = color.legible_on(look.ground(), look.ink, 3.0);
        let points: Vec<(f32, f32)> = values
            .iter()
            .enumerate()
            .map(|(i, v)| at(i, RADIUS * (((*v - d.min) as f32) / span).clamp(0.0, 1.0)))
            .collect();
        let mut path = String::new();
        for (j, (x, y)) in points.iter().enumerate() {
            path.push_str(&format!(
                "{}{} {}",
                if j == 0 { "M" } else { "L" },
                n(*x),
                n(*y)
            ));
        }
        path.push('Z');
        canvas.glaze(
            Layer::Frames,
            &path,
            (cx - RADIUS, cy - RADIUS, cx + RADIUS, cy + RADIUS),
            color,
            0.25,
        );
        canvas.polygon(
            Layer::Lines,
            &points,
            None,
            Some(Stroke::new(stroke_color, 2.0)),
        );
    }
    // The scale along the first axis.
    let small = Font::new((look.size - 4.0).max(8.0), look.ink);
    for t in 1..=d.ticks {
        let value = d.min + (d.max - d.min) * t as f64 / d.ticks as f64;
        let (x, y) = at(0, RADIUS * t as f32 / d.ticks as f32);
        let label = words(&said(value), small, family);
        // On the page's color, so it reads over the curves under it.
        canvas.rect(
            Layer::Bars,
            (x + 2.0, y, label.width + 4.0, label.height),
            2.0,
            Some(look.ground()),
            None,
        );
        canvas.text(Layer::Front, &label, (x + 4.0, y), Align::Left);
    }
    if d.legend && !d.curves.is_empty() {
        let mut y = top + 8.0;
        let x = cx + RADIUS + reach + 28.0;
        for (k, (label, _)) in d.curves.iter().enumerate() {
            let block = words(label, Font::new(look.size, look.ink), family);
            let color: Color = look.color(k);
            canvas.rect(
                Layer::Frames,
                (x, y + (block.height - 12.0) / 2.0, 12.0, 12.0),
                2.0,
                Some(color),
                Some(Stroke::new(
                    color.legible_on(look.ground(), look.ink, 3.0),
                    1.0,
                )),
            );
            canvas.text(Layer::Front, &block, (x + 18.0, y), Align::Left);
            y += block.height + 4.0;
        }
    }
    Ok(canvas.finish(10.0, look.page))
}

/// `id["Label"]` as its id and its label; a bare id is its own label.
fn labelled(text: &str) -> Option<(&str, &str)> {
    let text = text.trim();
    match text.split_once('[') {
        Some((id, rest)) => Some((id.trim(), unquote(rest.strip_suffix(']')?))),
        None => (!text.is_empty()).then_some((text, text)),
    }
}

/// `text` apart at `at`, outside quotes and brackets.
fn split(text: &str, at: char) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut quoted, mut depth, mut start) = (false, 0usize, 0);
    for (i, c) in text.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '[' | '{' if !quoted => depth += 1,
            ']' | '}' if !quoted => depth = depth.saturating_sub(1),
            c if c == at && !quoted && depth == 0 => {
                out.push(text[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(text[start..].trim());
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_and_curves_are_read_by_place_and_by_name() {
        let d = from_mermaid(
            "radar-beta\n  title Grades\n  axis m[\"Math\"], s[\"Science\"], e[\"English\"]\n  curve a[\"Alice\"]{85, 90, 80}\n  curve b[\"Bob\"]{ e: 85, m: 70, s: 75 }\n  max 100\n  min 0\n",
        )
        .unwrap();
        assert_eq!(d.axes, ["Math", "Science", "English"]);
        assert_eq!(d.curves[1], ("Bob".into(), vec![70.0, 75.0, 85.0]));
        assert_eq!((d.min, d.max), (0.0, 100.0));
        let svg = draw(&d, &Look::default()).unwrap();
        assert!(svg.contains(">Math<") && svg.contains(">Bob<"), "{svg}");
        assert!(!svg.contains("m[\""), "the label, not the syntax: {svg}");
        assert!(
            from_mermaid("radar-beta\naxis a, b\n").is_none(),
            "three axes at least"
        );
        // Braces the wrong way round are no curve, not a crash.
        assert!(from_mermaid("radar-beta\naxis a, b, c\ncurve x}{1,2,3\n").is_none());
    }
}
