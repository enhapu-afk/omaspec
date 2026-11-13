//! Draws a Gantt chart on a day scale. Names stay on the left, bars on
//! the right. The axis shows day numbers, or the day of the month when
//! the chart has a real date; the origin date is the only full date drawn.
//! An excluded day keeps its column, shaded, so that date stays the real one.
//!
//! On a coarser scale (`printscale weekly`, `monthly`, `quarterly`,
//! `yearly`) a column is a week from Monday, a month, a quarter or a year:
//! the axis names each, with the months or years they fall in above, and
//! runs from the first column's start to the last one's end.

use super::date;
use super::skin::Skin;
use super::{Bar, Diagram, Row, Scale};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::Color;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const MARGIN: f32 = 10.0;
const ROW: f32 = 22.0;
const BAR: f32 = 14.0;
const GAP: f32 = 16.0;
const HEAD: f32 = 8.0;

pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    let skin = Skin::from_params(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    canvas.sketch(skin.sketch);
    canvas.shadows(skin.shadow);
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

/// A column of a coarse scale: its first day and the day after its last,
/// from day 0, and what the axis calls it.
struct Column {
    start: i64,
    end: i64,
    name: String,
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
    let columns = columns(diagram.scale, diagram.origin, t0, t1);
    let (t0, t1) = match (columns.first(), columns.last()) {
        (Some(first), Some(last)) => (first.start, last.end),
        _ => (t0, t1),
    };
    let geom = Geom {
        axis_x: name_w + GAP,
        t0,
        t1,
        day: diagram.scale.day_width() * diagram.zoom,
    };
    let line = block("0", skin.header, family, 0)?.height;
    let label_h = block("0", skin.label, family, 0)?.height;
    let section_h = block("0", skin.section, family, 0)?.height;
    // The classic row is 22. A larger theme font grows it, or the names stack.
    let row_h = ROW.max(label_h.max(section_h) + 4.0);
    let groups = groups(diagram.scale, diagram.origin, &columns);
    let two_lines = if columns.is_empty() {
        diagram.origin.is_some()
    } else {
        !groups.is_empty()
    };
    let rows_top = top + band(two_lines, line);
    if columns.is_empty() {
        paint_axis(canvas, diagram, skin, family, &geom, top, line)?;
    } else {
        paint_columns(canvas, skin, family, &geom, &columns, &groups, top, line)?;
    }
    let right = x_of(&geom, geom.t1);
    let bottom = rows_top + row_h * diagram.rows.len() as f32;
    let grid = Stroke::new(skin.grid, 1.0);
    canvas.begin_group(Layer::Background, crate::GRID);
    if columns.is_empty() {
        for day in geom.t0..=geom.t1 {
            let x = x_of(&geom, day);
            canvas.line(Layer::Background, (x, rows_top), (x, bottom), grid);
        }
    } else {
        for day in columns.iter().map(|c| c.start).chain([geom.t1]) {
            let x = x_of(&geom, day);
            canvas.line(Layer::Background, (x, rows_top), (x, bottom), grid);
        }
    }
    canvas.line(
        Layer::Background,
        (geom.axis_x, rows_top),
        (right, rows_top),
        grid,
    );
    canvas.end_group(Layer::Background);
    for (index, row) in diagram.rows.iter().enumerate() {
        let y = rows_top + index as f32 * row_h;
        match row {
            Row::Section { text, line } => {
                canvas.plain(|canvas| {
                    canvas.rect(
                        Layer::Background,
                        (0.0, y, right.max(name_w), row_h),
                        0.0,
                        Some(skin.section_fill),
                        None,
                    );
                });
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
                canvas.plain(|canvas| {
                    canvas.rect(
                        Layer::Background,
                        (x_of(&geom, day), rows_top, geom.day, bottom - rows_top),
                        0.0,
                        Some(skin.closed),
                        None,
                    );
                });
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

/// A task's name and its bar; a linked task's are links.
fn paint_bar(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    geom: &Geom,
    bar: &Bar,
    y: f32,
    row_h: f32,
) -> Result<(), Declined> {
    canvas.linked(bar.link.as_ref(), &[Layer::Front, Layer::Bars], |canvas| {
        paint_task(canvas, skin, family, geom, bar, y, row_h)
    })
}

fn paint_task(
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
    // A day of a year's column is a tenth of a pixel: a bar stays in sight.
    let width = ((bar.shown_end - bar.start) as f32 * geom.day).max(2.0);
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
            // The done part sits a hair inside the bar, so it keeps its form.
            canvas.plain(|canvas| {
                canvas.rect(
                    Layer::Bars,
                    (x + 1.0, bar_y + 1.0, part - 2.0, BAR - 2.0),
                    skin.round,
                    Some(darker(fill)),
                    None,
                );
            });
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

/// The columns of the axis from `t0` to `t1` on a coarse scale, the first
/// starting at or before `t0`, the last ending at or after `t1`; none on a
/// daily scale, which draws its own.
fn columns(scale: Scale, origin: Option<i64>, t0: i64, t1: i64) -> Vec<Column> {
    if scale == Scale::Daily {
        return Vec::new();
    }
    let base = origin.unwrap_or(0);
    let mut out = Vec::new();
    let mut start = column_start(scale, origin, t0);
    loop {
        let end = column_end(scale, base, start);
        out.push(Column {
            start,
            end,
            name: column_name(scale, origin, start),
        });
        if end >= t1 {
            return out;
        }
        start = end;
    }
}

/// The first day of the column `day` falls in, from day 0.
fn column_start(scale: Scale, origin: Option<i64>, day: i64) -> i64 {
    let Some(base) = origin else {
        // No calendar: weeks of seven days from day 0.
        return day.div_euclid(7) * 7;
    };
    let at = base + day;
    let (year, month, _) = date::civil_from_days(at);
    let first = match scale {
        Scale::Daily => at,
        Scale::Weekly { .. } => at - i64::from(date::iso_weekday(at)) + 1,
        Scale::Monthly => date::days_from_civil(year, month, 1),
        Scale::Quarterly => date::days_from_civil(year, (month - 1) / 3 * 3 + 1, 1),
        Scale::Yearly => date::days_from_civil(year, 1, 1),
    };
    first - base
}

/// The day after the last of the column that starts on `start`.
fn column_end(scale: Scale, base: i64, start: i64) -> i64 {
    let (year, month, _) = date::civil_from_days(base + start);
    let months = |n: u32| {
        let index = month - 1 + n;
        date::days_from_civil(year + (index / 12) as i32, index % 12 + 1, 1) - base
    };
    match scale {
        Scale::Daily => start + 1,
        Scale::Weekly { .. } => start + 7,
        Scale::Monthly => months(1),
        Scale::Quarterly => months(3),
        Scale::Yearly => months(12),
    }
}

/// What the axis calls the column that starts on `start`: a week's ISO
/// number (or the day its Monday falls on), a month, a quarter, a year.
fn column_name(scale: Scale, origin: Option<i64>, start: i64) -> String {
    let Some(base) = origin else {
        return (start.div_euclid(7) + 1).to_string();
    };
    let (year, month, day) = date::civil_from_days(base + start);
    match scale {
        Scale::Daily => day.to_string(),
        Scale::Weekly { dates: true } => day.to_string(),
        Scale::Weekly { dates: false } => date::iso_week(base + start).to_string(),
        Scale::Monthly => MONTHS[month as usize - 1].to_owned(),
        Scale::Quarterly => format!("Q{}", (month - 1) / 3 + 1),
        Scale::Yearly => year.to_string(),
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The spans above the columns: the months a week's columns fall in, the
/// years of the months' and quarters'. None for years, nor without dates.
fn groups(scale: Scale, origin: Option<i64>, columns: &[Column]) -> Vec<Column> {
    let Some(base) = origin else {
        return Vec::new();
    };
    let of = |column: &Column| {
        let (year, month, _) = date::civil_from_days(base + column.start);
        match scale {
            Scale::Weekly { .. } => Some(format!("{} {year}", MONTHS[month as usize - 1])),
            Scale::Monthly | Scale::Quarterly => Some(year.to_string()),
            Scale::Daily | Scale::Yearly => None,
        }
    };
    let mut out: Vec<Column> = Vec::new();
    for column in columns {
        let Some(name) = of(column) else {
            return Vec::new();
        };
        match out.last_mut() {
            Some(last) if last.name == name => last.end = column.end,
            _ => out.push(Column {
                start: column.start,
                end: column.end,
                name,
            }),
        }
    }
    out
}

/// The axis of a coarse scale: the groups on the first line, the columns
/// on the second, each name in the middle of its span. A name wider than
/// its span is left out; a column's, every so many so they do not touch.
#[allow(clippy::too_many_arguments)]
fn paint_columns(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    geom: &Geom,
    columns: &[Column],
    groups: &[Column],
    top: f32,
    line: f32,
) -> Result<(), Declined> {
    let middle = |c: &Column| (x_of(geom, c.start) + x_of(geom, c.end)) / 2.0;
    let wide = |c: &Column| x_of(geom, c.end) - x_of(geom, c.start);
    for group in groups {
        let label = block(&group.name, skin.header, family, 0)?;
        if label.width + 4.0 <= wide(group) {
            canvas.text(Layer::Front, &label, (middle(group), top), Align::Center);
        }
    }
    let y = if groups.is_empty() {
        top
    } else {
        top + (line + 2.0).max(14.0)
    };
    let labels = columns
        .iter()
        .map(|c| block(&c.name, skin.header, family, 0))
        .collect::<Result<Vec<_>, _>>()?;
    let widest = labels.iter().map(|l| l.width).fold(0.0, f32::max);
    let narrowest = columns.iter().map(wide).fold(f32::INFINITY, f32::min);
    let step = ((widest + 4.0) / narrowest.max(0.1)).ceil().max(1.0) as usize;
    for (column, label) in columns.iter().zip(&labels).step_by(step) {
        canvas.text(Layer::Front, label, (middle(column), y), Align::Center);
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
