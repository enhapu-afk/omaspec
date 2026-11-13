//! Laying an ER diagram out: each entity measured from its attributes,
//! the entities in ranks along the direction, room kept for labels and
//! for relations that loop back.

use super::paint::readable;
use super::{Axis, Diagram, Entity, LABEL_PAD, LOOP, LOOP_STEP, PAD, RANK, Relation, STACK};
use crate::Declined;
use crate::color::Color;
use crate::text::{self, Block, Family, Font};

#[derive(Clone, Copy, Debug)]
pub(super) struct Rect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl Rect {
    pub(super) fn right(self) -> f32 {
        self.x + self.w
    }
    pub(super) fn bottom(self) -> f32 {
        self.y + self.h
    }
    pub(super) fn cx(self) -> f32 {
        self.x + self.w / 2.0
    }
    pub(super) fn cy(self) -> f32 {
        self.y + self.h / 2.0
    }
}

pub(super) struct Row {
    pub(super) type_name: Option<Block>,
    pub(super) name: Block,
    pub(super) keys: Option<Block>,
    pub(super) comment: Option<Block>,
    pub(super) height: f32,
}

pub(super) struct Measured {
    pub(super) title: Block,
    pub(super) rows: Vec<Row>,
    pub(super) width: f32,
    pub(super) height: f32,
}

pub(super) struct Laid {
    pub(super) boxes: Vec<Rect>,
    pub(super) measured: Vec<Measured>,
    pub(super) title: Option<Block>,
}

pub(super) fn lay(diagram: &Diagram, family: Family) -> Result<Laid, Declined> {
    let paper = diagram.look.background.unwrap_or(Color::WHITE);
    let page_ink = readable(paper, diagram.look.preferred);
    let font = Font::new(diagram.look.font_size, page_ink);
    let title_font = Font::new(diagram.look.font_size + 2.0, page_ink).bold();
    let measured = diagram
        .entities
        .iter()
        .map(|entity| measure_entity(entity, diagram, family))
        .collect::<Result<Vec<_>, _>>()?;
    let title = diagram
        .title
        .as_ref()
        .map(|text| {
            text::block(&text.value, title_font, family).map_err(|err| Declined::at(text.line, err))
        })
        .transpose()?;
    let label_room = label_room(diagram, font, family)?;
    let port_sep = port_sep(diagram, font, family)?;
    let loops = loop_extras(diagram, font, family)?;
    let rank_gap = RANK.max(label_room + LABEL_PAD);
    let stack_gap = STACK.max(label_room + LABEL_PAD);
    let mut measured = measured;
    spread_sides(&mut measured, diagram, port_sep);
    let mut boxes = place(diagram, &measured, rank_gap, stack_gap, &loops);
    let title_h = title
        .as_ref()
        .map(|block| block.height + 14.0)
        .unwrap_or(0.0);
    for rect in &mut boxes {
        rect.y += title_h;
    }
    Ok(Laid {
        boxes,
        measured,
        title,
    })
}

pub(super) fn label_room(diagram: &Diagram, font: Font, family: Family) -> Result<f32, Declined> {
    let mut room = 0.0_f32;
    for relation in &diagram.relations {
        let Some(label) = &relation.label else {
            continue;
        };
        if relation.from == relation.to {
            continue;
        }
        let block =
            text::block(label, font, family).map_err(|err| Declined::at(relation.line, err))?;
        room = room.max(block.width.max(block.height));
    }
    Ok(room)
}

/// Room past the box for each self-loop, including its label.
/// How far apart two relationship lines must sit so their labels do not meet.
pub(super) fn port_sep(diagram: &Diagram, font: Font, family: Family) -> Result<f32, Declined> {
    let mut sep = 28.0_f32;
    for relation in &diagram.relations {
        if relation.from == relation.to {
            continue;
        }
        let Some(label) = &relation.label else {
            continue;
        };
        let block =
            text::block(label, font, family).map_err(|err| Declined::at(relation.line, err))?;
        let across = match diagram.axis {
            Axis::Right => block.height,
            Axis::Down => block.width,
        };
        sep = sep.max(across + 10.0);
    }
    Ok(sep)
}

/// A side with several relationships grows until those lines clear each other.
pub(super) fn spread_sides(measured: &mut [Measured], diagram: &Diagram, port_sep: f32) {
    let mut load = vec![0usize; measured.len()];
    for relation in &diagram.relations {
        if relation.from == relation.to {
            continue;
        }
        if let Some(count) = load.get_mut(relation.from) {
            *count += 1;
        }
        if let Some(count) = load.get_mut(relation.to) {
            *count += 1;
        }
    }
    for (item, count) in measured.iter_mut().zip(load) {
        if count <= 1 {
            continue;
        }
        let need = (count as f32 - 1.0) * port_sep / 0.64;
        match diagram.axis {
            Axis::Right => item.height = item.height.max(need),
            Axis::Down => item.width = item.width.max(need),
        }
    }
}

pub(super) fn loop_extras(
    diagram: &Diagram,
    font: Font,
    family: Family,
) -> Result<Vec<f32>, Declined> {
    (0..diagram.entities.len())
        .map(|index| loop_extra(diagram, index, font, family))
        .collect()
}

pub(super) fn loop_extra(
    diagram: &Diagram,
    index: usize,
    font: Font,
    family: Family,
) -> Result<f32, Declined> {
    let mut count = 0.0_f32;
    let mut label = 0.0_f32;
    for relation in &diagram.relations {
        if relation.from != index || relation.to != index {
            continue;
        }
        count += 1.0;
        if let Some(text) = &relation.label {
            let block =
                text::block(text, font, family).map_err(|err| Declined::at(relation.line, err))?;
            let along = match diagram.axis {
                Axis::Right => block.width,
                Axis::Down => block.height,
            };
            label = label.max(along + 8.0);
        }
    }
    if count == 0.0 {
        Ok(0.0)
    } else {
        Ok(LOOP + count * LOOP_STEP + label)
    }
}

pub(super) fn measure_entity(
    entity: &Entity,
    diagram: &Diagram,
    family: Family,
) -> Result<Measured, Declined> {
    let fill = entity.fill.unwrap_or(diagram.look.fill);
    let ink = entity
        .ink
        .unwrap_or_else(|| readable(fill, diagram.look.preferred));
    let font = Font::new(diagram.look.font_size, ink);
    let title = text::block(&entity.label, font.bold(), family)
        .map_err(|err| Declined::at(entity.line, err))?;
    let mut rows = Vec::new();
    let mut width = title.width;
    for attribute in &entity.attributes {
        let type_name = if attribute.type_name.is_empty() {
            None
        } else {
            Some(
                text::block(&attribute.type_name, font, family)
                    .map_err(|err| Declined::at(attribute.line, err))?,
            )
        };
        let name = text::block(&attribute.name, font, family)
            .map_err(|err| Declined::at(attribute.line, err))?;
        let keys = if attribute.keys.is_empty() {
            None
        } else {
            let written = attribute
                .keys
                .iter()
                .map(|key| key.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let key_ink = readable(fill, entity.border.unwrap_or(diagram.look.border));
            let key_font = Font::new(diagram.look.font_size, key_ink);
            Some(
                text::block(&written, key_font, family)
                    .map_err(|err| Declined::at(attribute.line, err))?,
            )
        };
        let comment = attribute
            .comment
            .as_ref()
            .map(|comment| {
                text::block(comment, font, family).map_err(|err| Declined::at(attribute.line, err))
            })
            .transpose()?;
        let type_w = type_name
            .as_ref()
            .map(|block| block.width + 8.0)
            .unwrap_or(0.0);
        let key_w = keys.as_ref().map(|block| block.width + 12.0).unwrap_or(0.0);
        let comment_w = comment.as_ref().map(|block| block.width).unwrap_or(0.0);
        width = width.max(type_w + name.width + key_w).max(comment_w);
        let height = name
            .height
            .max(type_name.as_ref().map(|block| block.height).unwrap_or(0.0))
            .max(keys.as_ref().map(|block| block.height).unwrap_or(0.0))
            + comment
                .as_ref()
                .map(|block| block.height + 2.0)
                .unwrap_or(0.0);
        rows.push(Row {
            type_name,
            name,
            keys,
            comment,
            height,
        });
    }
    width += 2.0 * PAD;
    let mut height = title.height + 2.0 * PAD;
    if !rows.is_empty() {
        height += 6.0;
        for row in &rows {
            height += row.height + 4.0;
        }
        height += 4.0;
    }
    Ok(Measured {
        title,
        rows,
        width,
        height,
    })
}

pub(super) fn place(
    diagram: &Diagram,
    measured: &[Measured],
    rank_gap: f32,
    stack_gap: f32,
    loops: &[f32],
) -> Vec<Rect> {
    let n = measured.len();
    let ranks = ranks(n, &diagram.relations);
    let max_rank = ranks.iter().copied().max().unwrap_or(0);
    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (index, rank) in ranks.into_iter().enumerate() {
        columns[rank].push(index);
    }
    let mut along = 0.0_f32;
    let mut across_at = vec![0.0_f32; n];
    let mut along_at = vec![0.0_f32; n];
    let mut column_across = Vec::new();
    for column in &columns {
        let span = column_span(column, measured, stack_gap, diagram.axis);
        column_across.push(span);
    }
    let max_across = column_across.iter().copied().fold(0.0_f32, f32::max);
    for (column, span) in columns.iter().zip(column_across) {
        let mut cursor = (max_across - span) / 2.0;
        let mut col_along = 0.0_f32;
        for &index in column {
            let (box_along, box_across) = extents(&measured[index], diagram.axis);
            along_at[index] = along;
            across_at[index] = cursor;
            cursor += box_across + stack_gap;
            col_along = col_along.max(box_along + loops[index]);
        }
        along += col_along + rank_gap;
    }
    if !columns.is_empty() {
        along -= rank_gap;
    }
    let mut rects = Vec::with_capacity(n);
    for index in 0..n {
        let (box_along, box_across) = extents(&measured[index], diagram.axis);
        let (mut x, mut y, w, h) = match diagram.axis {
            Axis::Right => (along_at[index], across_at[index], box_along, box_across),
            Axis::Down => (across_at[index], along_at[index], box_across, box_along),
        };
        if diagram.flip {
            match diagram.axis {
                Axis::Right => x = along - (x + w),
                Axis::Down => y = along - (y + h),
            }
        }
        rects.push(Rect { x, y, w, h });
    }
    rects
}

pub(super) fn column_span(
    column: &[usize],
    measured: &[Measured],
    stack_gap: f32,
    axis: Axis,
) -> f32 {
    if column.is_empty() {
        return 0.0;
    }
    let mut span = 0.0;
    for (nth, &index) in column.iter().enumerate() {
        let (_, across) = extents(&measured[index], axis);
        span += across;
        if nth + 1 != column.len() {
            span += stack_gap;
        }
    }
    span
}

pub(super) fn extents(measured: &Measured, axis: Axis) -> (f32, f32) {
    match axis {
        Axis::Right => (measured.width, measured.height),
        Axis::Down => (measured.height, measured.width),
    }
}

pub(super) fn ranks(n: usize, relations: &[Relation]) -> Vec<usize> {
    let mut rank = vec![0usize; n];
    for _ in 0..n {
        for relation in relations {
            if relation.from == relation.to || relation.from >= n || relation.to >= n {
                continue;
            }
            let next = rank[relation.from] + 1;
            if next < n && rank[relation.to] < next {
                rank[relation.to] = next;
            }
        }
    }
    rank
}
