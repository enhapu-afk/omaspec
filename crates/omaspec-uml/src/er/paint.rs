//! Painting a laid-out ER diagram: the entities as tables, the relations
//! with their crow's feet, their labels and their loops.

use super::lay::{Laid, Measured, Rect, lay};
use super::route::{outward, route, shorten, shorten_first};
use super::{Axis, Card, Diagram, Entity, LOOP_STEP, MARGIN, PAD, ROUND, Relation};
use crate::Declined;
use crate::color::{Color, Paint};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

/// The SVG. Labels sit in the gap the layout left for them.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    let mut canvas = Canvas::new(&diagram.look.font_name, false);
    canvas.sketch(diagram.look.sketch);
    let family = canvas.family();
    let laid = lay(diagram, family)?;
    paint_diagram(&mut canvas, diagram, &laid)?;
    Ok(canvas.finish(MARGIN, diagram.look.background))
}

pub(super) fn paint_diagram(
    canvas: &mut Canvas,
    diagram: &Diagram,
    laid: &Laid,
) -> Result<(), Declined> {
    for (index, relation) in diagram.relations.iter().enumerate() {
        paint_relation(canvas, diagram, laid, relation, index)?;
    }
    for (index, entity) in diagram.entities.iter().enumerate() {
        paint_entity(
            canvas,
            diagram,
            &laid.measured[index],
            laid.boxes[index],
            entity,
        );
    }
    if let Some(title) = &laid.title {
        let width = laid
            .boxes
            .iter()
            .map(|rect| rect.right())
            .fold(title.width, f32::max);
        canvas.text(Layer::Front, title, (width / 2.0, 0.0), Align::Center);
    }
    Ok(())
}

pub(super) fn paint_entity(
    canvas: &mut Canvas,
    diagram: &Diagram,
    measured: &Measured,
    rect: Rect,
    entity: &Entity,
) {
    let fill = entity.fill.unwrap_or(diagram.look.fill);
    let border = entity.border.unwrap_or(diagram.look.border);
    let mut stroke = Stroke::new(border, entity.thickness.unwrap_or(diagram.look.thickness));
    stroke.dash = entity.dash;
    canvas.rect(
        Layer::Front,
        (rect.x, rect.y, rect.w, rect.h),
        ROUND,
        Some(fill),
        Some(stroke),
    );
    let title_x = rect.x + rect.w / 2.0;
    let title_y = rect.y + PAD;
    canvas.text(
        Layer::Front,
        &measured.title,
        (title_x, title_y),
        Align::Center,
    );
    if measured.rows.is_empty() {
        return;
    }
    let line_y = title_y + measured.title.height + 4.0;
    canvas.line(
        Layer::Front,
        (rect.x + PAD, line_y),
        (rect.right() - PAD, line_y),
        Stroke::new(border, 1.0),
    );
    let mut y = line_y + 6.0;
    for row in &measured.rows {
        let mut x = rect.x + PAD;
        if let Some(type_name) = &row.type_name {
            canvas.text(Layer::Front, type_name, (x, y), Align::Left);
            x += type_name.width + 8.0;
        }
        canvas.text(Layer::Front, &row.name, (x, y), Align::Left);
        if let Some(keys) = &row.keys {
            canvas.text(Layer::Front, keys, (rect.right() - PAD, y), Align::Right);
        }
        y += row.name.height;
        if let Some(comment) = &row.comment {
            y += 2.0;
            canvas.text(Layer::Front, comment, (rect.x + PAD, y), Align::Left);
            y += comment.height;
        }
        y += 4.0;
    }
}

pub(super) fn paint_relation(
    canvas: &mut Canvas,
    diagram: &Diagram,
    laid: &Laid,
    relation: &Relation,
    index: usize,
) -> Result<(), Declined> {
    let from = laid.boxes[relation.from];
    let to = laid.boxes[relation.to];
    let mut stroke = Stroke::new(diagram.look.line, 1.5);
    if relation.dashed {
        stroke.dash = Some((4.0, 3.0));
    }
    let label = relation_label(diagram, relation, canvas.family())?;
    if relation.from == relation.to {
        let slot = diagram.relations[..index]
            .iter()
            .filter(|other| other.from == relation.from && other.to == relation.to)
            .count();
        paint_loop(
            canvas,
            diagram,
            relation,
            from,
            stroke,
            slot,
            label.as_ref(),
        );
        return Ok(());
    }
    let (from_slot, from_slots) = side_slot(diagram, laid, relation.from, relation.to, index);
    let (to_slot, to_slots) = side_slot(diagram, laid, relation.to, relation.from, index);
    let mut points = route(from, to, from_slot, from_slots, to_slot, to_slots);
    let start = points[0];
    let end = *points.last().unwrap_or(&start);
    shorten(&mut points, foot_len(relation.to_card));
    shorten_first(&mut points, foot_len(relation.from_card));
    for pair in points.windows(2) {
        canvas.line(Layer::Lifelines, pair[0], pair[1], stroke);
    }
    paint_foot(canvas, end, outward(to, end), relation.to_card, stroke);
    paint_foot(
        canvas,
        start,
        outward(from, start),
        relation.from_card,
        stroke,
    );
    if let Some(block) = label {
        let along_x = (from.cx() - to.cx()).abs() >= (from.cy() - to.cy()).abs();
        let at = label_anchor(&points, along_x);
        paint_chip(canvas, diagram, &block, at);
    }
    Ok(())
}

/// Where this relationship sits among the ones that leave `entity` toward
/// the same side. `index` is the relationship's place in the list.
pub(super) fn side_slot(
    diagram: &Diagram,
    laid: &Laid,
    entity: usize,
    other: usize,
    index: usize,
) -> (usize, usize) {
    let Some(side) = facing(&laid.boxes[entity], &laid.boxes[other]) else {
        return (0, 1);
    };
    let mut slot = 0usize;
    let mut slots = 0usize;
    for (at, relation) in diagram.relations.iter().enumerate() {
        let (here, there) = if relation.from == entity {
            (relation.from, relation.to)
        } else if relation.to == entity {
            (relation.to, relation.from)
        } else {
            continue;
        };
        if here == there {
            continue;
        }
        if facing(&laid.boxes[here], &laid.boxes[there]) != Some(side) {
            continue;
        }
        if at < index {
            slot += 1;
        }
        slots += 1;
    }
    (slot, slots.max(1))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

pub(super) fn facing(from: &Rect, to: &Rect) -> Option<Side> {
    if from.x == to.x && from.y == to.y && from.w == to.w && from.h == to.h {
        return None;
    }
    if (to.cx() - from.cx()).abs() >= (to.cy() - from.cy()).abs() {
        if to.cx() >= from.cx() {
            Some(Side::Right)
        } else {
            Some(Side::Left)
        }
    } else if to.cy() >= from.cy() {
        Some(Side::Bottom)
    } else {
        Some(Side::Top)
    }
}

pub(super) fn relation_label(
    diagram: &Diagram,
    relation: &Relation,
    family: Family,
) -> Result<Option<Block>, Declined> {
    let Some(label) = &relation.label else {
        return Ok(None);
    };
    let paper = diagram.look.background.unwrap_or(Color::WHITE);
    let font = Font::new(
        diagram.look.font_size,
        readable(paper, diagram.look.preferred),
    );
    text::block(label, font, family)
        .map(Some)
        .map_err(|err| Declined::at(relation.line, err))
}

pub(super) fn paint_chip(canvas: &mut Canvas, diagram: &Diagram, block: &Block, at: (f32, f32)) {
    let chip = Rect {
        x: at.0 - block.width / 2.0 - 4.0,
        y: at.1 - block.height / 2.0 - 2.0,
        w: block.width + 8.0,
        h: block.height + 4.0,
    };
    canvas.plain(|canvas| {
        canvas.rect(
            Layer::Frames,
            (chip.x, chip.y, chip.w, chip.h),
            3.0,
            diagram.look.background.or(Some(Color::WHITE)),
            None,
        );
    });
    canvas.text(
        Layer::Front,
        block,
        (chip.x + 4.0, chip.y + 2.0),
        Align::Left,
    );
}

pub(super) fn foot_len(card: Option<Card>) -> f32 {
    match card {
        None => 0.0,
        Some(Card::One) => 10.0,
        Some(Card::ZeroOne) => 14.0,
        Some(Card::Many) => 14.0,
        Some(Card::ZeroMany) => 18.0,
    }
}

pub(super) fn paint_foot(
    canvas: &mut Canvas,
    port: (f32, f32),
    away: (f32, f32),
    card: Option<Card>,
    stroke: Stroke,
) {
    let Some(card) = card else {
        return;
    };
    let at = |distance: f32| (port.0 + away.0 * distance, port.1 + away.1 * distance);
    let perp = (-away.1, away.0);
    let shift =
        |point: (f32, f32), amount: f32| (point.0 + perp.0 * amount, point.1 + perp.1 * amount);
    let mut bar = |distance: f32| {
        let center = at(distance);
        canvas.line(
            Layer::Front,
            shift(center, -6.0),
            shift(center, 6.0),
            stroke,
        );
    };
    match card {
        Card::One => {
            bar(2.0);
            bar(7.0);
        }
        Card::ZeroOne => {
            bar(2.0);
            canvas.ellipse(
                Layer::Front,
                at(11.0),
                (3.5, 3.5),
                diagram_hole(canvas),
                Some(stroke),
            );
        }
        Card::Many => {
            let origin = at(14.0);
            canvas.line(Layer::Front, origin, port, stroke);
            canvas.line(Layer::Front, origin, shift(port, 7.0), stroke);
            canvas.line(Layer::Front, origin, shift(port, -7.0), stroke);
        }
        Card::ZeroMany => {
            canvas.ellipse(
                Layer::Front,
                at(16.0),
                (3.5, 3.5),
                diagram_hole(canvas),
                Some(stroke),
            );
            let origin = at(11.0);
            canvas.line(Layer::Front, origin, port, stroke);
            canvas.line(Layer::Front, origin, shift(port, 7.0), stroke);
            canvas.line(Layer::Front, origin, shift(port, -7.0), stroke);
        }
    }
}

/// The circle of a zero is a hole, so the page shows through it.
pub(super) fn diagram_hole(_canvas: &Canvas) -> Paint {
    None
}

pub(super) fn paint_loop(
    canvas: &mut Canvas,
    diagram: &Diagram,
    relation: &Relation,
    rect: Rect,
    stroke: Stroke,
    slot: usize,
    label: Option<&Block>,
) {
    let away = loop_outward(diagram.axis, diagram.flip);
    let out = LOOP_OUT + slot as f32 * LOOP_STEP;
    let (start_port, end_port) = loop_ports(rect, away, slot);
    let outer_start = (start_port.0 + away.0 * out, start_port.1 + away.1 * out);
    let outer_end = (end_port.0 + away.0 * out, end_port.1 + away.1 * out);
    canvas.line(Layer::Lifelines, outer_start, outer_end, stroke);
    let mut start = [start_port, outer_start];
    let mut end = [outer_end, end_port];
    shorten_first(&mut start, foot_len(relation.from_card));
    shorten(&mut end, foot_len(relation.to_card));
    canvas.line(Layer::Lifelines, start[0], start[1], stroke);
    canvas.line(Layer::Lifelines, end[0], end[1], stroke);
    paint_foot(canvas, start_port, away, relation.from_card, stroke);
    paint_foot(canvas, end_port, away, relation.to_card, stroke);
    if let Some(block) = label {
        // The words of every loop of the box stand past the outermost
        // one, beside one another across the flow, in the loops' order.
        let loops = diagram
            .relations
            .iter()
            .filter(|other| other.from == relation.from && other.to == relation.to)
            .count();
        let outermost = LOOP_OUT + (loops as f32 - 1.0) * LOOP_STEP;
        let across = (-away.1, away.0);
        let (along, wide) = if away.0.abs() >= away.1.abs() {
            (block.width, block.height)
        } else {
            (block.height, block.width)
        };
        let mid = (
            (outer_start.0 + outer_end.0) / 2.0,
            (outer_start.1 + outer_end.1) / 2.0,
        );
        let shift = (slot as f32 - (loops as f32 - 1.0) / 2.0) * (wide + 6.0);
        let at = (
            mid.0 + away.0 * (outermost - out + along / 2.0 + 6.0) + across.0 * shift,
            mid.1 + away.1 * (outermost - out + along / 2.0 + 6.0) + across.1 * shift,
        );
        paint_chip(canvas, diagram, block, at);
    }
}

/// How far a self-loop's first run lies from its box.
pub(super) const LOOP_OUT: f32 = 14.0;

/// The side of a box where its self-loop was given room: toward the next
/// rank, which is the low side after `RL` or `BT` mirrors the picture.
pub(super) fn loop_outward(axis: Axis, flip: bool) -> (f32, f32) {
    match (axis, flip) {
        (Axis::Right, false) => (1.0, 0.0),
        (Axis::Right, true) => (-1.0, 0.0),
        (Axis::Down, false) => (0.0, 1.0),
        (Axis::Down, true) => (0.0, -1.0),
    }
}

/// Where the self-loop in `slot` meets its box: on the side toward the
/// next rank, off the middle where the lines to other boxes leave, each
/// loop around the one before it.
pub(super) fn loop_ports(rect: Rect, away: (f32, f32), slot: usize) -> ((f32, f32), (f32, f32)) {
    let across = (-away.1, away.0);
    let origin = (
        rect.cx() + away.0 * rect.w / 2.0,
        rect.cy() + away.1 * rect.h / 2.0,
    );
    let span = if away.0.abs() >= away.1.abs() {
        rect.h
    } else {
        rect.w
    };
    let middle = span * 0.3;
    let half = (span * 0.06 + slot as f32 * LOOP_STEP).min(span / 2.0 - middle - 2.0);
    let (near, far) = (middle - half, middle + half);
    (
        (origin.0 + across.0 * near, origin.1 + across.1 * near),
        (origin.0 + across.0 * far, origin.1 + across.1 * far),
    )
}

/// How far a self-loop's mark reaches past the box, not counting its label.
#[cfg(test)]
pub(super) fn loop_reach(slot: usize, from: Option<Card>, to: Option<Card>) -> f32 {
    let out = LOOP_OUT + slot as f32 * LOOP_STEP;
    let foot = |card: Option<Card>| match card {
        None => 0.0,
        Some(Card::One) => 7.0,
        Some(Card::ZeroOne) => 14.5,
        Some(Card::Many) => 14.0,
        Some(Card::ZeroMany) => 19.5,
    };
    out.max(foot(from)).max(foot(to))
}

/// The middle of the longest run along the ranks, so the label sits in the
/// gap and not on a short jog.
pub(super) fn label_anchor(points: &[(f32, f32)], along_x: bool) -> (f32, f32) {
    let mut best_along = -1.0_f32;
    let mut at_along = points.first().copied().unwrap_or((0.0, 0.0));
    let mut best = -1.0_f32;
    let mut at = at_along;
    for pair in points.windows(2) {
        let dx = pair[1].0 - pair[0].0;
        let dy = pair[1].1 - pair[0].1;
        let len = dx * dx + dy * dy;
        let mid = ((pair[0].0 + pair[1].0) / 2.0, (pair[0].1 + pair[1].1) / 2.0);
        if len > best {
            best = len;
            at = mid;
        }
        let along = if along_x {
            dx.abs() >= dy.abs()
        } else {
            dy.abs() >= dx.abs()
        };
        if along && len > best_along {
            best_along = len;
            at_along = mid;
        }
    }
    if best_along > 0.0 { at_along } else { at }
}

/// The theme text when it contrasts with the fill, otherwise black or white.
pub(super) fn readable(fill: Color, preferred: Color) -> Color {
    if fill.is_dark() != preferred.is_dark() {
        preferred
    } else if fill.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}
