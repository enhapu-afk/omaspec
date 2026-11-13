//! Laying a block diagram out on its grid of columns: each block measured,
//! each group a frame of its own grid, rows filled as Mermaid fills them.

use super::read::{Metrics, metrics};
use super::route::route;
use super::{Diagram, Direction, End, Group, Item, Node, Shape};
use crate::Declined;
use crate::common::{Text, text_at};
use crate::text::{self, Block, Family, Font};

#[derive(Clone, Copy)]
pub(super) struct Size {
    pub(super) w: f32,
    pub(super) h: f32,
}

pub(super) struct GroupSize {
    pub(super) outer: Size,
    pub(super) band: f32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Rect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl Rect {
    pub(super) fn cx(self) -> f32 {
        self.x + self.w / 2.0
    }

    pub(super) fn cy(self) -> f32 {
        self.y + self.h / 2.0
    }

    pub(super) fn right(self) -> f32 {
        self.x + self.w
    }

    pub(super) fn bottom(self) -> f32 {
        self.y + self.h
    }

    pub(super) fn inflate(self, by: f32) -> Rect {
        Rect {
            x: self.x - by,
            y: self.y - by,
            w: self.w + 2.0 * by,
            h: self.h + 2.0 * by,
        }
    }

    pub(super) fn intersects(self, other: Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

pub(super) struct Cell {
    pub(super) item: Item,
    pub(super) span: usize,
    pub(super) col: usize,
    pub(super) size: Size,
}

pub(super) struct Laid {
    pub(super) nodes: Vec<Rect>,
    pub(super) groups: Vec<Rect>,
    pub(super) bands: Vec<f32>,
    pub(super) title: Option<Rect>,
    pub(super) edges: Vec<Vec<(f32, f32)>>,
    pub(super) edge_labels: Vec<Option<Rect>>,
}

pub(super) fn layout(diagram: &Diagram, family: Family) -> Result<Laid, Declined> {
    let metrics = metrics(diagram.look.font_size);
    let node_size = measure_nodes(diagram, family, metrics)?;
    let group_size = measure_groups(diagram, family, metrics, &node_size)?;
    let mut nodes = vec![
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0
        };
        diagram.nodes.len()
    ];
    let mut groups = vec![
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0
        };
        diagram.groups.len()
    ];
    let mut bands = vec![0.0; diagram.groups.len()];
    let items = diagram.items.clone();
    let (grid_w, grid_h) = frame_size(
        diagram,
        &items,
        diagram.columns,
        &node_size,
        &group_size,
        metrics,
    );
    let title = diagram
        .title
        .as_ref()
        .map(|title| measure(title, diagram, family, true))
        .transpose()?;
    let title_h = title.as_ref().map(|block| block.height).unwrap_or(0.0);
    let title_w = title.as_ref().map(|block| block.width).unwrap_or(0.0);
    let picture_w = grid_w.max(title_w);
    let top = if title.is_some() {
        title_h + metrics.font * 0.75
    } else {
        0.0
    };
    let area = Rect {
        x: (picture_w - grid_w) / 2.0,
        y: top,
        w: grid_w,
        h: grid_h,
    };
    let title_rect = title.as_ref().map(|block| Rect {
        x: (picture_w - block.width) / 2.0,
        y: 0.0,
        w: block.width,
        h: block.height,
    });
    {
        let mut place = Place {
            diagram,
            node_size: &node_size,
            group_size: &group_size,
            metrics,
            nodes: &mut nodes,
            groups: &mut groups,
            bands: &mut bands,
        };
        place_frame(&mut place, &items, diagram.columns, area);
    }
    let mut laid = Laid {
        nodes,
        groups,
        bands,
        title: title_rect,
        edges: Vec::new(),
        edge_labels: Vec::new(),
    };
    route(diagram, family, &mut laid)?;
    Ok(laid)
}

pub(super) fn measure_nodes(
    diagram: &Diagram,
    family: Family,
    metrics: Metrics,
) -> Result<Vec<Size>, Declined> {
    diagram
        .nodes
        .iter()
        .map(|node| {
            let block = measure(&text_at(&node.label, node.line), diagram, family, false)?;
            Ok(box_for(node.shape, block.width, block.height, metrics))
        })
        .collect()
}

pub(super) fn measure_groups(
    diagram: &Diagram,
    family: Family,
    metrics: Metrics,
    node_size: &[Size],
) -> Result<Vec<GroupSize>, Declined> {
    let mut group_size = Vec::with_capacity(diagram.groups.len());
    for _ in &diagram.groups {
        group_size.push(GroupSize {
            outer: Size { w: 0.0, h: 0.0 },
            band: 0.0,
        });
    }
    for index in (0..diagram.groups.len()).rev() {
        let items = diagram.groups[index].items.clone();
        let columns = diagram.groups[index].columns;
        let line = diagram.groups[index].line;
        let caption = if diagram.groups[index].label.is_empty() {
            diagram.groups[index].id.clone()
        } else {
            diagram.groups[index].label.clone()
        };
        let (mut inner_w, mut inner_h) =
            frame_size(diagram, &items, columns, node_size, &group_size, metrics);
        if items.is_empty() {
            inner_w = metrics.min_w * 0.8;
            inner_h = metrics.min_h * 0.7;
        }
        let title = measure(&text_at(&caption, line), diagram, family, false)?;
        let band = (title.height + 8.0).max(metrics.font + 8.0);
        group_size[index] = GroupSize {
            outer: Size {
                w: inner_w.max(title.width) + 2.0 * metrics.pad,
                h: band + inner_h + metrics.pad,
            },
            band,
        };
    }
    Ok(group_size)
}

pub(super) fn measure(
    text: &Text,
    diagram: &Diagram,
    family: Family,
    bold: bool,
) -> Result<Block, Declined> {
    let mut font = Font::new(diagram.look.font_size, diagram.look.preferred);
    if bold {
        font = font.bold();
    }
    text::block(&text.value, font, family).map_err(|_| Declined::at(text.line, "a label"))
}

pub(super) fn box_for(shape: Shape, text_w: f32, text_h: f32, metrics: Metrics) -> Size {
    let w = text_w + metrics.font * 1.8;
    let h = text_h + metrics.font * 1.0;
    let (w, h) = match shape {
        Shape::Circle => {
            let d = text_w.hypot(text_h) + metrics.font;
            (d, d)
        }
        Shape::DoubleCircle => {
            let d = text_w.hypot(text_h) + metrics.font * 1.7;
            (d, d)
        }
        Shape::Diamond => (text_w * 2.0 + metrics.font, text_h * 2.0 + metrics.font),
        Shape::Hexagon => (w + metrics.font * 1.3, h + metrics.font * 0.4),
        Shape::Stadium => (w.max(h * 1.7), h),
        Shape::Arrow(Direction::Up | Direction::Down | Direction::BothDown) => (
            w.max(metrics.min_w * 0.8),
            (h + metrics.font).max(metrics.min_h * 1.6),
        ),
        Shape::Arrow(_) => ((w + metrics.font * 1.4).max(metrics.min_w * 1.3), h),
        Shape::Parallelogram
        | Shape::ParallelogramAlt
        | Shape::Trapezoid
        | Shape::TrapezoidAlt
        | Shape::Asymmetric => (w + metrics.font * 1.2, h),
        Shape::Subroutine => (w + metrics.font, h),
        Shape::Cylinder => (w, h + metrics.font * 0.8),
        _ => (w, h),
    };
    Size {
        w: w.max(metrics.min_w),
        h: h.max(metrics.min_h),
    }
}

pub(super) fn columns_of(set: Option<usize>, items: usize) -> usize {
    if items == 0 {
        1
    } else {
        set.unwrap_or(items).max(1)
    }
}

pub(super) fn frame_size(
    diagram: &Diagram,
    items: &[Item],
    columns: Option<usize>,
    node_size: &[Size],
    group_size: &[GroupSize],
    metrics: Metrics,
) -> (f32, f32) {
    let rows = rows_for(diagram, items, columns, node_size, group_size, metrics);
    let widths = column_widths(&rows, columns_of(columns, items.len()), metrics);
    grid_size(&rows, &widths, metrics)
}

pub(super) fn item_span(item: Item, diagram_nodes: &[Node], diagram_groups: &[Group]) -> usize {
    match item {
        Item::Node(index) => diagram_nodes[index].span,
        Item::Group(index) => diagram_groups[index].span,
        Item::Space(span) => span,
    }
}

pub(super) fn item_size(
    item: Item,
    node_size: &[Size],
    group_size: &[GroupSize],
    metrics: Metrics,
) -> Size {
    match item {
        Item::Node(index) => node_size[index],
        Item::Group(index) => group_size[index].outer,
        Item::Space(_) => Size {
            w: metrics.min_w * 0.55,
            h: metrics.min_h,
        },
    }
}

pub(super) fn rows_for(
    diagram: &Diagram,
    items: &[Item],
    columns: Option<usize>,
    node_size: &[Size],
    group_size: &[GroupSize],
    metrics: Metrics,
) -> Vec<Vec<Cell>> {
    let columns = columns_of(columns, items.len());
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut col = 0usize;
    for &item in items {
        let span = item_span(item, &diagram.nodes, &diagram.groups).clamp(1, columns);
        if col > 0 && col + span > columns {
            rows.push(std::mem::take(&mut row));
            col = 0;
        }
        let mut size = item_size(item, node_size, group_size, metrics);
        if matches!(item, Item::Space(_)) {
            size.w *= span as f32;
        }
        row.push(Cell {
            item,
            span,
            col,
            size,
        });
        col += span;
        if col >= columns {
            rows.push(std::mem::take(&mut row));
            col = 0;
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

pub(super) fn column_widths(rows: &[Vec<Cell>], columns: usize, metrics: Metrics) -> Vec<f32> {
    let mut width = vec![0.0_f32; columns];
    for row in rows {
        for cell in row {
            if cell.span == 1 {
                width[cell.col] = width[cell.col].max(cell.size.w);
            }
        }
    }
    for row in rows {
        for cell in row {
            if cell.span <= 1 {
                continue;
            }
            let have = width[cell.col..cell.col + cell.span].iter().sum::<f32>()
                + metrics.gap * (cell.span - 1) as f32;
            if have + 0.01 < cell.size.w {
                let extra = (cell.size.w - have) / cell.span as f32;
                for slot in &mut width[cell.col..cell.col + cell.span] {
                    *slot += extra;
                }
            }
        }
    }
    width
}

pub(super) fn grid_size(rows: &[Vec<Cell>], widths: &[f32], metrics: Metrics) -> (f32, f32) {
    let used = rows
        .iter()
        .flat_map(|row| row.iter())
        .map(|cell| cell.col + cell.span)
        .max()
        .unwrap_or(0);
    if used == 0 {
        return (0.0, 0.0);
    }
    let width = widths[..used].iter().sum::<f32>() + metrics.gap * (used - 1) as f32;
    let mut height = 0.0;
    for (index, row) in rows.iter().enumerate() {
        height += row.iter().map(|cell| cell.size.h).fold(0.0_f32, f32::max);
        if index + 1 != rows.len() {
            height += metrics.gap;
        }
    }
    (width, height)
}

pub(super) struct Place<'a> {
    pub(super) diagram: &'a Diagram,
    pub(super) node_size: &'a [Size],
    pub(super) group_size: &'a [GroupSize],
    pub(super) metrics: Metrics,
    pub(super) nodes: &'a mut [Rect],
    pub(super) groups: &'a mut [Rect],
    pub(super) bands: &'a mut [f32],
}

pub(super) fn place_frame(
    place: &mut Place<'_>,
    items: &[Item],
    columns: Option<usize>,
    area: Rect,
) {
    let rows = rows_for(
        place.diagram,
        items,
        columns,
        place.node_size,
        place.group_size,
        place.metrics,
    );
    let widths = column_widths(&rows, columns_of(columns, items.len()), place.metrics);
    let (grid_w, grid_h) = grid_size(&rows, &widths, place.metrics);
    let origin_x = area.x + (area.w - grid_w).max(0.0) / 2.0;
    let mut y = area.y + (area.h - grid_h).max(0.0) / 2.0;
    for row in &rows {
        let row_h = row.iter().map(|cell| cell.size.h).fold(0.0_f32, f32::max);
        let cells: Vec<(Item, usize, usize)> = row
            .iter()
            .map(|cell| (cell.item, cell.col, cell.span))
            .collect();
        for (item, col, span) in cells {
            let x = origin_x + widths[..col].iter().sum::<f32>() + place.metrics.gap * col as f32;
            let cell_w = widths[col..col + span].iter().sum::<f32>()
                + place.metrics.gap * span.saturating_sub(1) as f32;
            match item {
                Item::Node(index) => {
                    let size = place.node_size[index];
                    place.nodes[index] = Rect {
                        x: x + (cell_w - size.w) / 2.0,
                        y: y + (row_h - size.h) / 2.0,
                        w: size.w,
                        h: size.h,
                    };
                }
                Item::Space(_) => {}
                Item::Group(index) => {
                    place.groups[index] = Rect {
                        x,
                        y,
                        w: cell_w,
                        h: row_h,
                    };
                    let band = place.group_size[index].band;
                    place.bands[index] = band;
                    let pad = place.metrics.pad;
                    let frame = place.groups[index];
                    let inner = Rect {
                        x: frame.x + pad,
                        y: frame.y + band,
                        w: (frame.w - 2.0 * pad).max(0.0),
                        h: (frame.h - band - pad).max(0.0),
                    };
                    let children = place.diagram.groups[index].items.clone();
                    let nested = place.diagram.groups[index].columns;
                    place_frame(place, &children, nested, inner);
                }
            }
        }
        y += row_h + place.metrics.gap;
    }
}

pub(super) fn contains_end(diagram: &Diagram, from: End, to: End) -> bool {
    match from {
        End::Group(index) if holds(diagram, index, to) => true,
        _ => match to {
            End::Group(index) => holds(diagram, index, from),
            End::Node(_) => false,
        },
    }
}

pub(super) fn holds(diagram: &Diagram, group: usize, end: End) -> bool {
    for item in &diagram.groups[group].items {
        match *item {
            Item::Node(index) if End::Node(index) == end => return true,
            Item::Group(index) if End::Group(index) == end || holds(diagram, index, end) => {
                return true;
            }
            _ => {}
        }
    }
    false
}
