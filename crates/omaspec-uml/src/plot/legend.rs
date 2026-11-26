//! A chart's legend: each named series' sign (a square for bars and
//! areas, a stroke for a line, a scatter's marker) and its name, in a
//! column beside the chart or a row above or below it.

use super::draw::{Drawn, LINE, MARKER, marker};
use super::look::{Look, Shape};
use super::{Diagram, Kind, Side};
use crate::chart::words;
use crate::color::Color;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{Block, Family};

/// A legend's sign.
const SWATCH: f32 = 12.0;

/// The legend: each named series' sign and its name.
pub(super) struct Legend {
    /// Each entry with the series it is of, among those drawn.
    entries: Vec<(usize, Kind, Color, Shape, f32, Block)>,
    row: bool,
    pub width: f32,
    pub height: f32,
    padding: f32,
}

impl Legend {
    pub fn of(d: &Diagram, drawn: &[Drawn], look: &Look, family: Family) -> Option<Legend> {
        let side = d.legend?;
        let entries: Vec<(usize, Kind, Color, Shape, f32, Block)> = drawn
            .iter()
            .enumerate()
            .filter_map(|(at, s)| {
                let name = s.series.name.as_deref()?;
                Some((
                    at,
                    s.series.kind,
                    s.color,
                    s.style.shape.unwrap_or(Shape::Circle),
                    s.style.size.unwrap_or(MARKER),
                    words(name, look.legend_font, family).written_on(s.series.line),
                ))
            })
            .collect();
        if entries.is_empty() {
            return None;
        }
        let row = matches!(side, Side::Top | Side::Bottom);
        let padding = look.legend_padding;
        let entry = |b: &Block| (SWATCH + 6.0 + b.width, b.height.max(SWATCH));
        let (width, height) = if row {
            let w: f32 = entries.iter().map(|e| entry(&e.5).0).sum::<f32>()
                + 16.0 * (entries.len() - 1) as f32;
            let h = entries.iter().map(|e| entry(&e.5).1).fold(0.0, f32::max);
            (w, h)
        } else {
            let w = entries.iter().map(|e| entry(&e.5).0).fold(0.0, f32::max);
            let h: f32 = entries.iter().map(|e| entry(&e.5).1).sum::<f32>()
                + 8.0 * (entries.len() - 1) as f32;
            (w, h)
        };
        Some(Legend {
            entries,
            row,
            width: width + 2.0 * padding + look.legend_margin,
            height: height + 2.0 * padding,
            padding,
        })
    }

    /// Draws the legend at `(x, y)`, each entry in the part of its series
    /// (`names`, as the series are drawn).
    pub fn draw(&self, canvas: &mut Canvas, (look, names): (&Look, &[String]), (x, y): (f32, f32)) {
        let x = x + look.legend_margin;
        if look.legend_fill.is_some() || look.legend_border.is_some() {
            let border = look.legend_border.map(|c| Stroke::new(c, 1.0));
            let box_ = (x, y, self.width - look.legend_margin, self.height);
            canvas.rect(Layer::Background, box_, 3.0, look.legend_fill, border);
        }
        let (mut cx, mut cy) = (x + self.padding, y + self.padding);
        for (at, kind, color, shape, size, name) in &self.entries {
            let high = name.height.max(SWATCH);
            let middle = cy + high / 2.0;
            let part = names.get(*at).map_or("", String::as_str);
            canvas.part(part, |canvas| {
                match kind {
                    Kind::Bar | Kind::Area => {
                        canvas.rect(
                            Layer::Front,
                            (cx, middle - SWATCH / 2.0, SWATCH, SWATCH),
                            0.0,
                            Some(*color),
                            None,
                        );
                    }
                    Kind::Line => {
                        canvas.line(
                            Layer::Front,
                            (cx, middle),
                            (cx + SWATCH, middle),
                            Stroke::new(*color, LINE),
                        );
                    }
                    Kind::Scatter => {
                        marker(
                            canvas,
                            Layer::Front,
                            (cx + SWATCH / 2.0, middle),
                            *shape,
                            size.min(SWATCH),
                            *color,
                        );
                    }
                }
                canvas.text(
                    Layer::Front,
                    name,
                    (cx + SWATCH + 6.0, middle - name.height / 2.0),
                    Align::Left,
                );
            });
            if self.row {
                cx += SWATCH + 6.0 + name.width + 16.0;
            } else {
                cy += high + 8.0;
            }
        }
    }
}
