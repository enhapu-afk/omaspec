//! A treemap (`treemap-beta`): each leaf a box as large as its value,
//! each section a box around its own with its name over them, laid out
//! as squares as near as they come (the squarified layout). A section of
//! the top takes a color of the palette, what it holds a lighter one.

use super::{Look, body, number, said, title, unquote, words};
use crate::Declined;
use crate::graph::Rect;
use crate::svg::{Align, Canvas, Layer};
use crate::text::Font;

const WIDTH: f32 = 640.0;
const HEIGHT: f32 = 420.0;
/// A section's band with its name, and the room around what it holds.
const BAND: f32 = 22.0;
const PAD: f32 = 4.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<String>,
    pub items: Vec<Item>,
}

/// A section, which holds others, or a leaf with its value.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub name: String,
    pub value: Option<f64>,
    pub children: Vec<Item>,
}

impl Item {
    /// Its value: a leaf's own, a section's the sum of what it holds.
    pub fn total(&self) -> f64 {
        match self.value {
            Some(v) => v.max(0.0),
            None => self.children.iter().map(Item::total).sum(),
        }
    }
}

/// The treemap of `source`; `None` when it is none this drawer reads.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    let (title, lines) = body(source, "treemap-beta").or_else(|| body(source, "treemap"))?;
    // Each item with how far it stands in, read again by its indent.
    let raw_lines: Vec<&str> = source.lines().collect();
    let mut flat: Vec<(usize, Item)> = Vec::new();
    for (no, line) in lines {
        let indent = raw_lines[no].len() - raw_lines[no].trim_start().len();
        let line = line.split(":::").next().unwrap_or(line).trim();
        let item = match line.rsplit_once(':') {
            Some((name, value)) if number(value).is_some() => Item {
                name: unquote(name).to_owned(),
                value: number(value),
                children: Vec::new(),
            },
            _ if line.starts_with('"') => Item {
                name: unquote(line).to_owned(),
                value: None,
                children: Vec::new(),
            },
            _ => return None,
        };
        flat.push((indent, item));
    }
    let mut at = 0;
    let items = nest(&flat, &mut at, None);
    (!items.is_empty() && at == flat.len()).then_some(Diagram { title, items })
}

/// The items of `flat` from `*at` on that stand deeper than `parent`.
fn nest(flat: &[(usize, Item)], at: &mut usize, parent: Option<usize>) -> Vec<Item> {
    let mut out = Vec::new();
    while let Some((indent, item)) = flat.get(*at) {
        if parent.is_some_and(|p| *indent <= p) {
            break;
        }
        *at += 1;
        let mut item = item.clone();
        if item.value.is_none() {
            item.children = nest(flat, at, Some(*indent));
        }
        out.push(item);
    }
    out
}

pub fn draw(d: &Diagram, look: &Look) -> Result<String, Declined> {
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let top = title(&mut canvas, d.title.as_deref(), look, WIDTH / 2.0);
    let area = Rect {
        x: 0.0,
        y: top,
        w: WIDTH,
        h: HEIGHT,
    };
    let values: Vec<f64> = d.items.iter().map(Item::total).collect();
    for (k, (item, rect)) in d.items.iter().zip(squarify(&values, area)).enumerate() {
        paint(&mut canvas, look, item, rect, look.color(k), 0);
    }
    Ok(canvas.finish(10.0, look.page))
}

fn paint(
    canvas: &mut Canvas,
    look: &Look,
    item: &Item,
    rect: Rect,
    color: crate::color::Color,
    depth: usize,
) {
    let family = canvas.family();
    // A section of the top in its color, what it holds lighter, deeper on.
    let fill = color.mix(look.ground(), (0.15 + depth as f32 * 0.25).min(0.75));
    let ink = look.ink_on(fill);
    // Apart from its neighbors by a gap of the page, not by a line.
    canvas.rect(
        Layer::Frames,
        (
            rect.x + 1.0,
            rect.y + 1.0,
            (rect.w - 2.0).max(0.0),
            (rect.h - 2.0).max(0.0),
        ),
        3.0,
        Some(fill),
        // Drawn by hand, a box has its line, in its own color.
        look.sketch
            .then(|| crate::svg::Stroke::new(fill.mix(look.ink, 0.3), 1.0)),
    );
    let font = Font::new(look.size - 1.0, ink);
    if item.value.is_some() {
        // A leaf: its name and its value, where they fit.
        let name = words(&item.name, font, family);
        let value = words(&said(item.total()), Font::new(look.size - 3.0, ink), family);
        if name.width + 8.0 <= rect.w && name.height + value.height + 6.0 <= rect.h {
            let y = rect.y + (rect.h - name.height - value.height) / 2.0;
            canvas.text(
                Layer::Front,
                &name,
                (rect.x + rect.w / 2.0, y),
                Align::Center,
            );
            canvas.text(
                Layer::Front,
                &value,
                (rect.x + rect.w / 2.0, y + name.height),
                Align::Center,
            );
        }
        return;
    }
    let name = words(&item.name, font.bold(), family);
    if name.width + 8.0 <= rect.w && BAND <= rect.h {
        canvas.text(
            Layer::Front,
            &name,
            (rect.x + 6.0, rect.y + 3.0),
            Align::Left,
        );
    }
    let inner = Rect {
        x: rect.x + PAD,
        y: rect.y + BAND,
        w: (rect.w - 2.0 * PAD).max(0.0),
        h: (rect.h - BAND - PAD).max(0.0),
    };
    let values: Vec<f64> = item.children.iter().map(Item::total).collect();
    for (child, place) in item.children.iter().zip(squarify(&values, inner)) {
        paint(canvas, look, child, place, color, depth + 1);
    }
}

/// The squarified layout: `values` as rectangles in `area`, as near to
/// squares as a row at a time allows, in the order given.
pub fn squarify(values: &[f64], area: Rect) -> Vec<Rect> {
    let total: f64 = values.iter().map(|v| v.max(0.0)).sum();
    let mut out = vec![
        Rect {
            x: area.x,
            y: area.y,
            w: 0.0,
            h: 0.0
        };
        values.len()
    ];
    if total <= 0.0 || area.w <= 0.0 || area.h <= 0.0 {
        return out;
    }
    let scale = (area.w * area.h) as f64 / total;
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|&a, &b| values[b].total_cmp(&values[a]));
    let mut free = area;
    let mut row: Vec<usize> = Vec::new();
    let worst = |row: &[usize], side: f32| -> f32 {
        let areas: Vec<f32> = row
            .iter()
            .map(|&i| (values[i].max(0.0) * scale) as f32)
            .collect();
        let sum: f32 = areas.iter().sum();
        if sum <= 0.0 {
            return f32::MAX;
        }
        let (max, min) = areas
            .iter()
            .fold((0.0f32, f32::MAX), |(hi, lo), &a| (hi.max(a), lo.min(a)));
        let s2 = side * side;
        (s2 * max / (sum * sum)).max(sum * sum / (s2 * min.max(1e-6)))
    };
    let mut i = 0;
    while i < order.len() {
        let side = free.w.min(free.h);
        let mut with = row.clone();
        with.push(order[i]);
        if row.is_empty() || worst(&with, side) <= worst(&row, side) {
            row = with;
            i += 1;
        } else {
            free = lay_row(&row, values, scale, free, &mut out);
            row.clear();
        }
    }
    if !row.is_empty() {
        lay_row(&row, values, scale, free, &mut out);
    }
    out
}

/// Lays `row` along the shorter side of `free`; what is left of it.
fn lay_row(row: &[usize], values: &[f64], scale: f64, free: Rect, out: &mut [Rect]) -> Rect {
    let sum: f32 = row
        .iter()
        .map(|&i| (values[i].max(0.0) * scale) as f32)
        .sum();
    if free.w >= free.h {
        // A column at the left.
        let w = if free.h > 0.0 { sum / free.h } else { 0.0 };
        let mut y = free.y;
        for &i in row {
            let h = if w > 0.0 {
                (values[i].max(0.0) * scale) as f32 / w
            } else {
                0.0
            };
            out[i] = Rect { x: free.x, y, w, h };
            y += h;
        }
        Rect {
            x: free.x + w,
            w: (free.w - w).max(0.0),
            ..free
        }
    } else {
        let h = if free.w > 0.0 { sum / free.w } else { 0.0 };
        let mut x = free.x;
        for &i in row {
            let w = if h > 0.0 {
                (values[i].max(0.0) * scale) as f32 / h
            } else {
                0.0
            };
            out[i] = Rect { x, y: free.y, w, h };
            x += w;
        }
        Rect {
            y: free.y + h,
            h: (free.h - h).max(0.0),
            ..free
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_hold_their_leaves_and_each_takes_its_share() {
        let d = from_mermaid(
            "treemap-beta\n\"Products\"\n    \"Laptops\": 40\n    \"Phones\": 30\n\"Services\"\n    \"Support\": 20\n    \"Cloud\": 10\n",
        )
        .unwrap();
        assert_eq!(d.items.len(), 2);
        assert_eq!(d.items[0].children.len(), 2);
        assert_eq!(d.items[1].total(), 30.0);
        // Each rectangle as large as its share of the area.
        let area = Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        let rects = squarify(&[40.0, 30.0, 20.0, 10.0], area);
        for (r, share) in rects.iter().zip([0.4, 0.3, 0.2, 0.1]) {
            assert!((r.w * r.h - share * 10_000.0).abs() < 1.0, "{r:?}");
        }
        let svg = draw(&d, &Look::default()).unwrap();
        assert!(
            svg.contains(">Laptops<") && svg.contains(">Services<"),
            "{svg}"
        );
        assert!(from_mermaid("treemap-beta\nLaptops 40\n").is_none());
    }
}
