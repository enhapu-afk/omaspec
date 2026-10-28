//! Draws a timing diagram on its own axis. One time unit has one width, so
//! a constraint stays as long as the times say.

use super::skin::Skin;
use super::{Constraint, Diagram, Mark, Row, RowKind};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::Color;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const MARGIN: f32 = 10.0;
const ROW_GAP: f32 = 8.0;
const LEVEL: f32 = 20.0;
const NAME_GAP: f32 = 14.0;
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
    let placed = place(diagram, &skin, family, notes.top())?;
    paint(&mut canvas, diagram, &skin, family, &placed)?;
    notes.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, skin.background))
}

struct Lane {
    body: f32,
    height: f32,
    /// Extra room under the waveform for constraints on this row.
    constraints: f32,
}

struct Place {
    lanes: Vec<Lane>,
    name_w: f32,
    axis_x: f32,
    t0: f64,
    t1: f64,
    px: f64,
    axis_y: f32,
}

fn place(diagram: &Diagram, skin: &Skin, family: Family, top: f32) -> Result<Place, Declined> {
    let (t0, t1, px) = axis_of(diagram);
    let mut name_w: f32 = 0.0;
    let mut preamble: f32 = 0.0;
    for row in &diagram.rows {
        let label = block(&row.label, skin.label, family, row.line)?;
        name_w = name_w.max(label.width);
        if let Some(mark) = &row.initial {
            let text = block(&mark.name, skin.state, family, mark.line)?;
            preamble = preamble.max(text.width + 16.0);
        }
    }
    name_w += 4.0;
    let axis_x = name_w + NAME_GAP + if preamble > 0.0 { preamble + 8.0 } else { 0.0 };
    let mut y = top;
    let mut lanes = Vec::new();
    for row in &diagram.rows {
        let above = note_height(diagram, &row.key, true, skin, family)?;
        if above > 0.0 {
            y += above + 4.0;
        }
        let constraints = diagram
            .constraints
            .iter()
            .filter(|constraint| constraint.row.as_deref() == Some(row.key.as_str()))
            .count() as f32
            * 16.0;
        let height = body_height(row) + constraints;
        lanes.push(Lane {
            body: y,
            height,
            constraints,
        });
        y += height + ROW_GAP;
        let below = note_height(diagram, &row.key, false, skin, family)?;
        if below > 0.0 {
            y += below + 4.0;
        }
    }
    Ok(Place {
        lanes,
        name_w,
        axis_x,
        t0,
        t1,
        px,
        axis_y: y + 4.0,
    })
}

fn body_height(row: &Row) -> f32 {
    match row.kind {
        RowKind::Robust => row.order.len().max(1) as f32 * LEVEL + 8.0,
        RowKind::Analog => 52.0,
        RowKind::Concise | RowKind::Rectangle => 36.0,
        RowKind::Binary | RowKind::Clock => 30.0,
    }
}

fn note_height(
    diagram: &Diagram,
    key: &str,
    top: bool,
    skin: &Skin,
    family: Family,
) -> Result<f32, Declined> {
    let mut height: f32 = 0.0;
    for note in &diagram.notes {
        if note.row == key && note.top == top {
            let text = block(&note.text, skin.note, family, note.line)?;
            height = height.max(text.height + 8.0);
        }
    }
    Ok(height)
}

/// `(t0, t1, pixels per time unit)`. The axis is about 480 pixels when no
/// `scale` is set. The last state keeps a visible width: 15% of the span,
/// and at least 50 pixels, or 100 time units when every event is at one time.
fn axis_of(diagram: &Diagram) -> (f64, f64, f64) {
    let mut times = Vec::new();
    for row in &diagram.rows {
        times.extend(row.states.iter().map(|state| state.time));
    }
    for message in &diagram.messages {
        times.push(message.time);
        times.push(message.to_time);
    }
    for constraint in &diagram.constraints {
        times.push(constraint.start);
        times.push(constraint.end);
    }
    for highlight in &diagram.highlights {
        times.push(highlight.start);
        times.push(highlight.end);
    }
    if times.is_empty() {
        times.push(0.0);
    }
    let t0 = times.iter().copied().fold(f64::INFINITY, f64::min);
    let t_max = times.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (t_max - t0).max(0.0);
    let one = distinct(&times) <= 1;
    if let Some((across, pixels)) = diagram.scale {
        let px = pixels / across;
        let extra = if one {
            100.0
        } else {
            (span * 0.15).max(50.0 / px)
        };
        (t0, t_max + extra, px)
    } else if one {
        (t0, t0 + 100.0, 4.8)
    } else {
        let provisional = 480.0 / span.max(1e-6);
        let extra = (span * 0.15).max(50.0 / provisional);
        let full = span + extra;
        (t0, t0 + full, 480.0 / full)
    }
}

fn distinct(times: &[f64]) -> usize {
    let mut sorted = times.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mut count = 0;
    let mut last = f64::NAN;
    for time in sorted {
        if count == 0 || (time - last).abs() > 1e-9 {
            count += 1;
            last = time;
        }
    }
    count
}

struct Axis<'a> {
    x: &'a dyn Fn(f64) -> f32,
    right: f32,
    t0: f64,
    t1: f64,
    name_w: f32,
}

fn at(axis: &Axis<'_>, time: f64) -> f32 {
    (axis.x)(time)
}

fn paint(
    canvas: &mut Canvas,
    diagram: &Diagram,
    skin: &Skin,
    family: Family,
    place: &Place,
) -> Result<(), Declined> {
    let x_of = |time: f64| place.axis_x + ((time - place.t0) * place.px) as f32;
    let axis = Axis {
        x: &x_of,
        right: x_of(place.t1),
        t0: place.t0,
        t1: place.t1,
        name_w: place.name_w,
    };
    if let (Some(first), Some(last)) = (place.lanes.first(), place.lanes.last()) {
        for highlight in &diagram.highlights {
            let left = x_of(highlight.start.min(highlight.end));
            let width = x_of(highlight.start.max(highlight.end)) - left;
            canvas.rect(
                Layer::Background,
                (
                    left,
                    first.body,
                    width.max(1.0),
                    last.body + last.height - first.body,
                ),
                0.0,
                Some(highlight.color.unwrap_or(skin.highlight)),
                None,
            );
            if !highlight.caption.is_empty() {
                let caption = block(&highlight.caption, skin.state, family, highlight.line)?;
                canvas.text(
                    Layer::Front,
                    &caption,
                    (left + width / 2.0, first.body + 2.0),
                    Align::Center,
                );
            }
        }
    }
    for (index, row) in diagram.rows.iter().enumerate() {
        let lane = &place.lanes[index];
        paint_row(canvas, skin, family, lane, &axis, row)?;
        paint_notes(canvas, diagram, skin, family, row, lane)?;
    }
    if !diagram.hide_axis {
        paint_axis(canvas, diagram, skin, family, place, &axis)?;
    }
    let mut loose = 0.0;
    for constraint in &diagram.constraints {
        let y = if let Some(name) = &constraint.row {
            let index = diagram
                .rows
                .iter()
                .position(|row| row.key == *name)
                .unwrap_or(0);
            let lane = &place.lanes[index];
            lane.body + lane.height - lane.constraints + loose_index(diagram, constraint) * 16.0
        } else {
            let y = place.axis_y + 22.0 + loose;
            loose += 18.0;
            y
        };
        paint_constraint(canvas, skin, family, constraint, &axis, y)?;
    }
    for message in &diagram.messages {
        let from = row_center(diagram, place, &message.from);
        let to = row_center(diagram, place, &message.to);
        arrow(
            canvas,
            skin,
            (x_of(message.time), from),
            (x_of(message.to_time), to),
        );
        if !message.label.is_empty() {
            let label = block(&message.label, skin.arrow_font, family, message.line)?;
            let mid_x = (x_of(message.time) + x_of(message.to_time)) / 2.0;
            let mid_y = (from + to) / 2.0 - label.height - 1.0;
            canvas.text(Layer::Front, &label, (mid_x, mid_y), Align::Center);
        }
    }
    Ok(())
}

fn loose_index(diagram: &Diagram, constraint: &Constraint) -> f32 {
    diagram
        .constraints
        .iter()
        .filter(|other| other.row == constraint.row)
        .position(|other| std::ptr::eq(other, constraint))
        .unwrap_or(0) as f32
}

fn row_center(diagram: &Diagram, place: &Place, name: &str) -> f32 {
    let index = diagram
        .rows
        .iter()
        .position(|row| row.key == name || row.label == name)
        .unwrap_or(0);
    let lane = &place.lanes[index];
    lane.body + (lane.height - lane.constraints) / 2.0
}

fn paint_row(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    lane: &Lane,
    axis: &Axis<'_>,
    row: &Row,
) -> Result<(), Declined> {
    let body = lane.height - lane.constraints;
    let label = block(&row.label, skin.label, family, row.line)?;
    canvas.text(
        Layer::Front,
        &label,
        (0.0, lane.body + (body - label.height) / 2.0),
        Align::Left,
    );
    if let Some(mark) = &row.initial {
        let text = block(
            &mark.name,
            colored(skin.state, mark.color),
            family,
            mark.line,
        )?;
        let x = axis.name_w + NAME_GAP;
        let y = lane.body + (body - text.height) / 2.0;
        canvas.rect(
            Layer::Frames,
            (x - 4.0, y - 3.0, text.width + 8.0, text.height + 6.0),
            3.0,
            Some(mark.color.unwrap_or(skin.box_fill)),
            Some(Stroke::new(skin.box_border, 1.0)),
        );
        canvas.text(Layer::Front, &text, (x, y), Align::Left);
    }
    match row.kind {
        RowKind::Robust => paint_robust(canvas, skin, family, row, lane, axis)?,
        RowKind::Concise | RowKind::Rectangle => {
            paint_boxes(canvas, skin, family, row, lane, axis)?;
        }
        RowKind::Binary => paint_binary(canvas, skin, family, row, lane, axis)?,
        RowKind::Clock => paint_clock(canvas, skin, row, lane, axis),
        RowKind::Analog => paint_analog(canvas, skin, family, row, lane, axis)?,
    }
    Ok(())
}

fn paint_robust(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    row: &Row,
    lane: &Lane,
    axis: &Axis<'_>,
) -> Result<(), Declined> {
    let stroke = Stroke::new(row.fill.unwrap_or(skin.line), 1.5);
    let y_of = |name: &str| {
        let index = row.order.iter().position(|got| got == name).unwrap_or(0);
        lane.body + 4.0 + (index as f32 + 0.5) * LEVEL
    };
    if row.states.is_empty() {
        let y = lane.body + body_only(lane) / 2.0;
        canvas.line(
            Layer::Lifelines,
            (at(axis, axis.t0), y),
            (axis.right, y),
            stroke,
        );
        return Ok(());
    }
    let mut points = Vec::new();
    for (index, state) in row.states.iter().enumerate() {
        let x = at(axis, state.time);
        let y = y_of(&state.mark.name);
        if index == 0 {
            points.push((x, y));
        } else {
            let previous = points.last().copied().unwrap_or((x, y)).1;
            points.push((x, previous));
            points.push((x, y));
        }
    }
    let last_y = points.last().map(|point| point.1).unwrap_or(lane.body);
    points.push((axis.right, last_y));
    canvas.polyline(Layer::Lifelines, &points, stroke);
    for (index, state) in row.states.iter().enumerate() {
        let end = row
            .states
            .get(index + 1)
            .map(|next| at(axis, next.time))
            .unwrap_or(axis.right);
        let start = at(axis, state.time);
        paint_mark(
            canvas,
            skin,
            family,
            &state.mark,
            (start + end) / 2.0,
            y_of(&state.mark.name) - skin.state.size,
            true,
        )?;
    }
    Ok(())
}

fn paint_boxes(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    row: &Row,
    lane: &Lane,
    axis: &Axis<'_>,
) -> Result<(), Declined> {
    let radius = if row.kind == RowKind::Rectangle {
        skin.round
    } else {
        skin.round.max(4.0)
    };
    let (fill, border) = if row.kind == RowKind::Rectangle {
        (
            skin.rectangle_fill.unwrap_or(skin.box_fill),
            skin.rectangle_border.unwrap_or(skin.box_border),
        )
    } else {
        (skin.box_fill, skin.box_border)
    };
    let y = lane.body + 4.0;
    let height = body_only(lane) - 8.0;
    for (index, state) in row.states.iter().enumerate() {
        let start = at(axis, state.time);
        let end = row
            .states
            .get(index + 1)
            .map(|next| at(axis, next.time))
            .unwrap_or(axis.right);
        let fill = state.mark.color.or(row.fill).unwrap_or(fill);
        canvas.rect(
            Layer::Frames,
            (start, y, (end - start).max(1.0), height.max(1.0)),
            radius,
            Some(fill),
            Some(Stroke::new(border, 1.0)),
        );
        paint_mark(
            canvas,
            skin,
            family,
            &state.mark,
            (start + end) / 2.0,
            y + (height - skin.state.size) / 2.0,
            true,
        )?;
    }
    Ok(())
}

fn paint_binary(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    row: &Row,
    lane: &Lane,
    axis: &Axis<'_>,
) -> Result<(), Declined> {
    let top = lane.body + 4.0;
    let bottom = lane.body + body_only(lane) - 6.0;
    let stroke = Stroke::new(row.fill.unwrap_or(skin.line), 1.5);
    let high = |mark: &Mark| {
        matches!(
            mark.name.to_ascii_lowercase().as_str(),
            "high" | "1" | "true"
        )
    };
    if row.states.is_empty() {
        canvas.line(
            Layer::Lifelines,
            (at(axis, axis.t0), bottom),
            (axis.right, bottom),
            stroke,
        );
        return Ok(());
    }
    let mut points = Vec::new();
    for (index, state) in row.states.iter().enumerate() {
        let x = at(axis, state.time);
        let y = if high(&state.mark) { top } else { bottom };
        if index == 0 {
            points.push((x, y));
        } else {
            let previous = points.last().copied().unwrap_or((x, y)).1;
            points.push((x, previous));
            points.push((x, y));
        }
    }
    let last_y = points.last().map(|point| point.1).unwrap_or(bottom);
    points.push((axis.right, last_y));
    canvas.polyline(Layer::Lifelines, &points, stroke);
    let mid = lane.body + (body_only(lane) - skin.state.size) / 2.0;
    for (index, state) in row.states.iter().enumerate() {
        if state.mark.note.is_none() {
            continue;
        }
        let start = at(axis, state.time);
        let end = row
            .states
            .get(index + 1)
            .map(|next| at(axis, next.time))
            .unwrap_or(axis.right);
        paint_mark(
            canvas,
            skin,
            family,
            &state.mark,
            (start + end) / 2.0,
            mid,
            false,
        )?;
    }
    Ok(())
}

fn paint_clock(canvas: &mut Canvas, skin: &Skin, row: &Row, lane: &Lane, axis: &Axis<'_>) {
    let Some(clock) = row.clock else {
        return;
    };
    let top = lane.body + 4.0;
    let bottom = lane.body + body_only(lane) - 6.0;
    let edges = clock_edges(axis.t0, axis.t1, clock.period, clock.pulse, clock.offset);
    let mut points: Vec<(f32, f32)> = Vec::new();
    for pair in edges.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        let y = if clock_high((start + end) / 2.0, clock) {
            top
        } else {
            bottom
        };
        if let Some((_, previous)) = points.last().copied() {
            if (previous - y).abs() > 0.1 {
                points.push((at(axis, start), previous));
                points.push((at(axis, start), y));
            }
        } else {
            points.push((at(axis, start), y));
        }
        points.push((at(axis, end), y));
    }
    if points.len() >= 2 {
        canvas.polyline(
            Layer::Lifelines,
            &points,
            Stroke::new(row.fill.unwrap_or(skin.line), 1.5),
        );
    }
}

fn paint_analog(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    row: &Row,
    lane: &Lane,
    axis: &Axis<'_>,
) -> Result<(), Declined> {
    let top = lane.body + 4.0;
    let bottom = lane.body + body_only(lane) - 6.0;
    let values: Vec<f64> = row
        .states
        .iter()
        .filter_map(|state| state.mark.name.parse().ok())
        .collect();
    let (low, high) = row.analog.unwrap_or_else(|| {
        let max = values.iter().copied().fold(0.0, f64::max);
        (0.0, max.max(1.0))
    });
    let (low, high) = (low.min(high), low.max(high));
    let span = if (high - low).abs() < 1e-9 {
        1.0
    } else {
        high - low
    };
    let y_of = |value: f64| {
        let unit = ((value - low) / span).clamp(0.0, 1.0);
        bottom - (unit as f32) * (bottom - top)
    };
    let stroke = Stroke::new(row.fill.unwrap_or(skin.line), 1.5);
    if row.states.is_empty() {
        canvas.line(
            Layer::Lifelines,
            (at(axis, axis.t0), bottom),
            (axis.right, bottom),
            stroke,
        );
        return Ok(());
    }
    let mut points = Vec::new();
    for state in &row.states {
        let value = state.mark.name.parse().unwrap_or(low);
        points.push((at(axis, state.time), y_of(value)));
    }
    if let Some((_, y)) = points.last().copied() {
        points.push((axis.right, y));
    }
    canvas.polyline(Layer::Lifelines, &points, stroke);
    for state in &row.states {
        let value = state.mark.name.parse().unwrap_or(low);
        paint_mark(
            canvas,
            skin,
            family,
            &state.mark,
            at(axis, state.time) + 4.0,
            y_of(value) - skin.state.size - 1.0,
            true,
        )?;
    }
    Ok(())
}

fn paint_mark(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    mark: &Mark,
    x: f32,
    y: f32,
    show_name: bool,
) -> Result<(), Declined> {
    let mut y = y;
    if show_name {
        let text = block(
            &mark.name,
            colored(skin.state, mark.color),
            family,
            mark.line,
        )?;
        canvas.text(Layer::Front, &text, (x, y), Align::Center);
        y += text.height;
    }
    if let Some(note) = &mark.note {
        let font = Font::new((skin.state.size - 2.0).max(8.0), skin.state.color).italic();
        let note = block(note, font, family, mark.line)?;
        canvas.text(Layer::Front, &note, (x, y), Align::Center);
    }
    Ok(())
}

fn paint_notes(
    canvas: &mut Canvas,
    diagram: &Diagram,
    skin: &Skin,
    family: Family,
    row: &Row,
    lane: &Lane,
) -> Result<(), Declined> {
    for note in &diagram.notes {
        if note.row != row.key {
            continue;
        }
        let text = block(&note.text, skin.note, family, note.line)?;
        let y = if note.top {
            lane.body - text.height - 10.0
        } else {
            lane.body + lane.height + 4.0
        };
        canvas.rect(
            Layer::Frames,
            (0.0, y - 3.0, text.width + 12.0, text.height + 6.0),
            2.0,
            Some(skin.note_fill),
            Some(Stroke::new(skin.note_border, 1.0)),
        );
        canvas.text(Layer::Front, &text, (6.0, y), Align::Left);
    }
    Ok(())
}

fn paint_axis(
    canvas: &mut Canvas,
    diagram: &Diagram,
    skin: &Skin,
    family: Family,
    place: &Place,
    axis: &Axis<'_>,
) -> Result<(), Declined> {
    let y = place.axis_y;
    let stroke = Stroke::new(skin.header.color, 1.0);
    canvas.line(Layer::Lifelines, (place.axis_x, y), (axis.right, y), stroke);
    let mut ticks = event_times(diagram);
    if ticks.is_empty() {
        ticks.push(place.t0);
    }
    ticks.sort_by(|a, b| a.total_cmp(b));
    ticks.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    for time in ticks {
        let x = at(axis, time);
        canvas.line(Layer::Lifelines, (x, y), (x, y + 4.0), stroke);
        let label = block(&format_time(time), skin.header, family, 0)?;
        canvas.text(Layer::Front, &label, (x, y + 5.0), Align::Center);
    }
    Ok(())
}

fn event_times(diagram: &Diagram) -> Vec<f64> {
    let mut ticks = Vec::new();
    for row in &diagram.rows {
        ticks.extend(row.states.iter().map(|state| state.time));
    }
    for message in &diagram.messages {
        ticks.push(message.time);
        ticks.push(message.to_time);
    }
    for constraint in &diagram.constraints {
        ticks.push(constraint.start);
        ticks.push(constraint.end);
    }
    for highlight in &diagram.highlights {
        ticks.push(highlight.start);
        ticks.push(highlight.end);
    }
    ticks
}

fn paint_constraint(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    constraint: &Constraint,
    axis: &Axis<'_>,
    y: f32,
) -> Result<(), Declined> {
    let left = at(axis, constraint.start.min(constraint.end));
    let right = at(axis, constraint.start.max(constraint.end));
    arrow(canvas, skin, (left, y), (right, y));
    arrow(canvas, skin, (right, y), (left, y));
    if !constraint.label.is_empty() {
        let label = block(&constraint.label, skin.arrow_font, family, constraint.line)?;
        canvas.text(
            Layer::Front,
            &label,
            ((left + right) / 2.0, y - label.height - 1.0),
            Align::Center,
        );
    }
    Ok(())
}

fn arrow(canvas: &mut Canvas, skin: &Skin, from: (f32, f32), to: (f32, f32)) {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let tip = (to.0, to.1);
    let start = (from.0 + ux * HEAD, from.1 + uy * HEAD);
    let back = (tip.0 - ux * HEAD, tip.1 - uy * HEAD);
    canvas.line(Layer::Front, start, back, skin.arrow);
    let (px, py) = (-uy * 3.5, ux * 3.5);
    let left = (back.0 + px, back.1 + py);
    let right = (back.0 - px, back.1 - py);
    if skin.strict {
        canvas.line(Layer::Front, left, tip, skin.arrow);
        canvas.line(Layer::Front, right, tip, skin.arrow);
    } else {
        canvas.polygon(
            Layer::Front,
            &[tip, left, right],
            Some(skin.arrow.color),
            Some(skin.arrow),
        );
    }
}

fn body_only(lane: &Lane) -> f32 {
    lane.height - lane.constraints
}

fn colored(font: Font, color: Option<Color>) -> Font {
    match color {
        Some(color) => Font { color, ..font },
        None => font,
    }
}

fn format_time(time: f64) -> String {
    if (time - time.round()).abs() < 1e-6 && time.abs() < 1e15 {
        format!("{}", time.round() as i64)
    } else {
        format!("{time:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

fn clock_edges(t0: f64, t1: f64, period: f64, pulse: f64, offset: f64) -> Vec<f64> {
    let mut edges = vec![t0];
    if period > 0.0 && t1 > t0 {
        let mut n = ((t0 - offset) / period).floor() as i64 - 1;
        for _ in 0..10_000 {
            let rise = offset + n as f64 * period;
            if rise > t1 + period {
                break;
            }
            for edge in [rise, rise + pulse] {
                if edge > t0 && edge < t1 {
                    edges.push(edge);
                }
            }
            n = n.saturating_add(1);
        }
    }
    edges.push(t1);
    edges.sort_by(|a, b| a.total_cmp(b));
    edges.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    edges
}

fn clock_high(time: f64, clock: super::Clock) -> bool {
    if clock.period <= 0.0 {
        return false;
    }
    (time - clock.offset).rem_euclid(clock.period) < clock.pulse
}

fn block(text: &str, font: Font, family: Family, line: usize) -> Result<Block, Declined> {
    text::block(text, font, family).map_err(|reason| Declined::at(line, reason))
}
