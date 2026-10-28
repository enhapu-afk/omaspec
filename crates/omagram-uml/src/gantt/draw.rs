//! Draws a Gantt chart on a day scale. Names stay on the left, bars on
//! the right. The axis shows day numbers, or the day of the month when
//! the chart has a real date; the origin date is the only full date drawn.
//! An excluded day keeps its column, shaded, so that date stays the real one.

use super::date;
use super::skin::Skin;
use super::{Bar, Diagram, Row};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::Color;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const MARGIN: f32 = 10.0;
const ROW: f32 = 22.0;
const BAR: f32 = 14.0;
const DAY: f32 = 16.0;
const GAP: f32 = 16.0;
const HEAD: f32 = 8.0;

pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    let skin = Skin::from_params(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    let family = canvas.family();
    let notes = Annotations::new(
        &diagram.frame,
        [skin.title, skin.header, skin.footer, skin.caption],
        family,
    )?;
    if !diagram.rows.is_empty() {
        paint(&mut canvas, diagram, &skin, family, notes.top())?;
    }
    notes.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, skin.background))
}

struct Geom {
    axis_x: f32,
    t0: i64,
    t1: i64,
    day: f32,
}

fn paint(
    canvas: &mut Canvas,
    diagram: &Diagram,
    skin: &Skin,
    family: Family,
    top: f32,
) -> Result<(), Declined> {
    let mut name_w: f32 = 0.0;
    for row in &diagram.rows {
        let (text, font, line) = match row {
            Row::Section { text, line } => (text.as_str(), skin.section, *line),
            Row::Bar(bar) => (bar.label.as_str(), skin.label, bar.line),
        };
        let label = block(text, font, family, line)?;
        name_w = name_w.max(label.width);
    }
    let (t0, t1) = bounds(diagram);
    let geom = Geom {
        axis_x: name_w + GAP,
        t0,
        t1,
        day: DAY * diagram.zoom,
    };
    let line = block("0", skin.header, family, 0)?.height;
    let label_h = block("0", skin.label, family, 0)?.height;
    let section_h = block("0", skin.section, family, 0)?.height;
    // The classic row is 22. A larger theme font grows it, or the names stack.
    let row_h = ROW.max(label_h.max(section_h) + 4.0);
    let rows_top = top + band(diagram.origin.is_some(), line);
    paint_axis(canvas, diagram, skin, family, &geom, top, line)?;
    let right = x_of(&geom, geom.t1);
    let bottom = rows_top + row_h * diagram.rows.len() as f32;
    let grid = Stroke::new(skin.grid, 1.0);
    for day in geom.t0..=geom.t1 {
        let x = x_of(&geom, day);
        canvas.line(Layer::Background, (x, rows_top), (x, bottom), grid);
    }
    canvas.line(
        Layer::Background,
        (geom.axis_x, rows_top),
        (right, rows_top),
        grid,
    );
    for (index, row) in diagram.rows.iter().enumerate() {
        let y = rows_top + index as f32 * row_h;
        match row {
            Row::Section { text, line } => {
                canvas.rect(
                    Layer::Background,
                    (0.0, y, right.max(name_w), row_h),
                    0.0,
                    Some(skin.section_fill),
                    None,
                );
                let label = block(text, skin.section, family, *line)?;
                canvas.text(
                    Layer::Front,
                    &label,
                    (0.0, y + (row_h - label.height) / 2.0),
                    Align::Left,
                );
            }
            Row::Bar(bar) => paint_bar(canvas, skin, family, &geom, bar, y, row_h)?,
        }
    }
    if let Some(origin) = diagram.origin {
        for day in geom.t0..geom.t1 {
            let absolute = origin.saturating_add(day);
            if diagram.closed.binary_search(&absolute).is_ok() {
                canvas.rect(
                    Layer::Background,
                    (x_of(&geom, day), rows_top, geom.day, bottom - rows_top),
                    0.0,
                    Some(skin.closed),
                    None,
                );
            }
        }
    }
    for (index, row) in diagram.rows.iter().enumerate() {
        let Row::Bar(bar) = row else { continue };
        let Some(pred) = bar.link_from.as_deref() else {
            continue;
        };
        let Some((from_index, from)) = find_bar(diagram, pred) else {
            continue;
        };
        let y0 = rows_top + from_index as f32 * row_h + row_h / 2.0;
        let y1 = rows_top + index as f32 * row_h + row_h / 2.0;
        link(
            canvas,
            skin,
            (x_of(&geom, from.end), y0),
            (x_of(&geom, bar.start), y1),
        );
    }
    Ok(())
}

fn paint_bar(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    geom: &Geom,
    bar: &Bar,
    y: f32,
    row_h: f32,
) -> Result<(), Declined> {
    let label = block(&bar.label, skin.label, family, bar.line)?;
    canvas.text(
        Layer::Front,
        &label,
        (0.0, y + (row_h - label.height) / 2.0),
        Align::Left,
    );
    let mid = y + row_h / 2.0;
    let fill = bar.color.unwrap_or(skin.bar);
    let border = Stroke::new(skin.bar_border, 1.0);
    if bar.milestone || bar.shown_end <= bar.start {
        let x = x_of(geom, bar.start);
        canvas.polygon(
            Layer::Bars,
            &[
                (x, mid - 8.0),
                (x + 8.0, mid),
                (x, mid + 8.0),
                (x - 8.0, mid),
            ],
            Some(fill),
            Some(border),
        );
        return Ok(());
    }
    let x = x_of(geom, bar.start);
    let width = (bar.shown_end - bar.start) as f32 * geom.day;
    let bar_y = y + (row_h - BAR) / 2.0;
    canvas.rect(
        Layer::Bars,
        (x, bar_y, width, BAR),
        skin.round,
        Some(fill),
        Some(border),
    );
    if let Some(done) = bar.done {
        let part = (width * done).clamp(0.0, width);
        if part > 2.0 {
            canvas.rect(
                Layer::Bars,
                (x + 1.0, bar_y + 1.0, part - 2.0, BAR - 2.0),
                skin.round,
                Some(darker(fill)),
                None,
            );
        }
    }
    Ok(())
}

fn paint_axis(
    canvas: &mut Canvas,
    diagram: &Diagram,
    skin: &Skin,
    family: Family,
    geom: &Geom,
    top: f32,
    line: f32,
) -> Result<(), Declined> {
    if let Some(origin) = diagram.origin {
        let label = block(&date::format(origin), skin.header, family, 0)?;
        canvas.text(Layer::Front, &label, (geom.axis_x, top), Align::Left);
    }
    // 14 keeps the day numbers where the default 10px header puts them.
    let y = top
        + if diagram.origin.is_some() {
            (line + 2.0).max(14.0)
        } else {
            0.0
        };
    let span = geom.t1 - geom.t0;
    let mut step = if geom.day >= 14.0 && span <= 40 { 1 } else { 5 };
    // A theme font wider than a column would pile the day numbers up.
    if skin.header.size > 10.5 && geom.day > 0.0 {
        let sample = block("30", skin.header, family, 0)?;
        let fit = ((sample.width + 4.0) / geom.day).ceil() as i64;
        if fit > step {
            step = fit;
        }
    }
    let mut day = geom.t0;
    while day <= geom.t1 {
        let label = block(&tick_text(day, diagram.origin), skin.header, family, 0)?;
        canvas.text(Layer::Front, &label, (x_of(geom, day), y), Align::Center);
        day += step;
    }
    Ok(())
}

fn bounds(diagram: &Diagram) -> (i64, i64) {
    let mut t0 = 0;
    let mut t1 = 1;
    let mut any = false;
    for row in &diagram.rows {
        let Row::Bar(bar) = row else { continue };
        let end = bar.end.max(bar.start.saturating_add(1));
        if !any {
            t0 = bar.start;
            t1 = end;
            any = true;
        } else {
            t0 = t0.min(bar.start);
            t1 = t1.max(end);
        }
    }
    if t1 <= t0 {
        t1 = t0.saturating_add(1);
    }
    (t0, t1)
}

fn find_bar<'a>(diagram: &'a Diagram, key: &str) -> Option<(usize, &'a Bar)> {
    diagram
        .rows
        .iter()
        .enumerate()
        .find_map(|(index, row)| match row {
            Row::Bar(bar) if bar.key == key || bar.label == key => Some((index, bar)),
            _ => None,
        })
}

fn band(dated: bool, line: f32) -> f32 {
    let needed = if dated { line * 2.0 + 4.0 } else { line + 4.0 };
    // The classic band, so a 10px header does not move an existing chart.
    needed.max(if dated { 28.0 } else { 16.0 })
}

fn x_of(geom: &Geom, day: i64) -> f32 {
    geom.axis_x + (day - geom.t0) as f32 * geom.day
}

fn tick_text(day: i64, origin: Option<i64>) -> String {
    match origin {
        Some(origin) => {
            let (_, _, d) = date::civil_from_days(origin.saturating_add(day));
            d.to_string()
        }
        None => day.to_string(),
    }
}

fn link(canvas: &mut Canvas, skin: &Skin, from: (f32, f32), to: (f32, f32)) {
    if (from.0 - to.0).abs() < 1.0 || (from.1 - to.1).abs() < 1.0 {
        arrow(canvas, skin, from, to);
        return;
    }
    let bend = (from.0, to.1);
    canvas.line(Layer::Front, from, bend, skin.arrow);
    arrow(canvas, skin, bend, to);
}

fn arrow(canvas: &mut Canvas, skin: &Skin, from: (f32, f32), to: (f32, f32)) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let back = (to.0 - ux * HEAD, to.1 - uy * HEAD);
    let start = (from.0 + ux * HEAD, from.1 + uy * HEAD);
    canvas.line(Layer::Front, start, back, skin.arrow);
    let (px, py) = (-uy * 3.5, ux * 3.5);
    let left = (back.0 + px, back.1 + py);
    let right = (back.0 - px, back.1 - py);
    if skin.strict {
        canvas.line(Layer::Front, left, to, skin.arrow);
        canvas.line(Layer::Front, right, to, skin.arrow);
    } else {
        canvas.polygon(
            Layer::Bars,
            &[to, left, right],
            Some(skin.arrow.color),
            Some(skin.arrow),
        );
    }
}

fn darker(color: Color) -> Color {
    let Color(r, g, b) = color;
    let scale = |channel: u8| (f32::from(channel) * 0.65).round() as u8;
    Color(scale(r), scale(g), scale(b))
}

fn block(text: &str, font: Font, family: Family, line: usize) -> Result<Block, Declined> {
    text::block(text, font, family).map_err(|reason| Declined::at(line, reason))
}
