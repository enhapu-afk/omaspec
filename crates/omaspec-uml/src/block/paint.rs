//! Painting a laid-out block diagram: each shape with its fill and border,
//! the groups as frames, the edges with their heads and labels.

use super::lay::{Laid, Rect, layout};
use super::{CLASSIC_BORDER, CLASSIC_FILL, Diagram, Direction, Edge, Look, Node, Shape};
use crate::Declined;
use crate::color::Color;
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{self, Family, Font};

/// The SVG. Nested blocks contain their children. An edge does not cross
/// another block.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    if !(4.0..=100.0).contains(&diagram.look.font_size) {
        return Err(Declined::at(0, "the font size is not one Omaspec draws"));
    }
    let mut canvas = Canvas::new(&diagram.look.font_name, false);
    canvas.sketch(diagram.look.sketch);
    let family = canvas.family();
    let laid = layout(diagram, family)?;
    paint(&mut canvas, diagram, &laid, family)?;
    Ok(canvas.finish(18.0, diagram.look.background))
}

pub(super) fn paint(
    canvas: &mut Canvas,
    diagram: &Diagram,
    laid: &Laid,
    family: Family,
) -> Result<(), Declined> {
    let look = &diagram.look;
    for (index, group) in diagram.groups.iter().enumerate() {
        let rect = laid.groups[index];
        if rect.w <= 0.0 || rect.h <= 0.0 {
            continue;
        }
        let fill = group.fill.unwrap_or_else(|| group_fill(look));
        let color = group.stroke.unwrap_or_else(|| border(look));
        let width = group.stroke_width.unwrap_or(1.4);
        canvas.rect(
            Layer::Frames,
            (rect.x, rect.y, rect.w, rect.h),
            8.0,
            Some(fill),
            Some(Stroke::new(color, width).dashed(7.0, 5.0)),
        );
    }
    for (index, node) in diagram.nodes.iter().enumerate() {
        draw_node(canvas, node, laid.nodes[index], look);
    }
    for (index, edge) in diagram.edges.iter().enumerate() {
        draw_edge(canvas, edge, &laid.edges[index], look);
    }
    for (index, group) in diagram.groups.iter().enumerate() {
        let caption = if group.label.is_empty() {
            &group.id
        } else {
            &group.label
        };
        let fill = group.fill.unwrap_or_else(|| group_fill(look));
        let color = group.text.unwrap_or_else(|| ink(fill, look));
        let block = text::block(caption, Font::new(look.font_size, color), family)
            .map_err(|_| Declined::at(group.line, "a label"))?;
        let rect = laid.groups[index];
        let band = laid.bands[index];
        let y = rect.y + (band - block.height).max(2.0) / 2.0;
        canvas.text(Layer::Front, &block, (rect.cx(), y), Align::Center);
    }
    for (index, node) in diagram.nodes.iter().enumerate() {
        let fill = node.fill.unwrap_or_else(|| tile(look));
        let color = node.text.unwrap_or_else(|| ink(fill, look));
        let block = text::block(&node.label, Font::new(look.font_size, color), family)
            .map_err(|_| Declined::at(node.line, "a label"))?;
        let (x, y) = anchor(node.shape, laid.nodes[index]);
        canvas.text(
            Layer::Front,
            &block,
            (x, y - block.height / 2.0),
            Align::Center,
        );
    }
    for (index, edge) in diagram.edges.iter().enumerate() {
        if let (Some(label), Some(rect)) = (&edge.label, laid.edge_labels[index]) {
            let block = text::block(
                &label.value,
                Font::new(look.font_size, look.preferred),
                family,
            )
            .map_err(|_| Declined::at(label.line, "a label"))?;
            canvas.text(Layer::Front, &block, (rect.cx(), rect.y), Align::Center);
        }
    }
    if let (Some(title), Some(rect)) = (&diagram.title, laid.title) {
        let block = text::block(
            &title.value,
            Font::new(look.font_size, look.preferred).bold(),
            family,
        )
        .map_err(|_| Declined::at(title.line, "a title"))?;
        canvas.text(Layer::Front, &block, (rect.cx(), rect.y), Align::Center);
    }
    Ok(())
}

pub(super) fn tile(look: &Look) -> Color {
    if look.classic {
        Color::hex(CLASSIC_FILL)
    } else {
        let page = look.background.unwrap_or(Color::WHITE);
        page.mix(look.accent, if page.is_dark() { 0.34 } else { 0.16 })
    }
}

pub(super) fn border(look: &Look) -> Color {
    if look.classic {
        Color::hex(CLASSIC_BORDER)
    } else {
        look.accent
    }
}

pub(super) fn group_fill(look: &Look) -> Color {
    let page = look.background.unwrap_or(Color::WHITE);
    if look.classic {
        page.mix(Color::hex(CLASSIC_FILL), 0.45)
    } else {
        page.mix(look.accent, if page.is_dark() { 0.14 } else { 0.07 })
    }
}

pub(super) fn ink(fill: Color, look: &Look) -> Color {
    if fill.is_dark() != look.preferred.is_dark() {
        look.preferred
    } else if fill.is_dark() {
        Color::WHITE
    } else {
        Color::hex(0x1A1A1A)
    }
}

pub(super) fn anchor(shape: Shape, rect: Rect) -> (f32, f32) {
    match shape {
        Shape::Arrow(Direction::Right | Direction::BothAcross) => {
            (rect.cx() - rect.w * 0.06, rect.cy())
        }
        Shape::Arrow(Direction::Left) => (rect.cx() + rect.w * 0.06, rect.cy()),
        Shape::Arrow(Direction::Down | Direction::BothDown) => {
            (rect.cx(), rect.cy() - rect.h * 0.06)
        }
        Shape::Arrow(Direction::Up) => (rect.cx(), rect.cy() + rect.h * 0.06),
        Shape::Asymmetric => (rect.cx() - rect.w * 0.08, rect.cy()),
        _ => (rect.cx(), rect.cy()),
    }
}

pub(super) fn draw_node(canvas: &mut Canvas, node: &Node, rect: Rect, look: &Look) {
    let fill = node.fill.unwrap_or_else(|| tile(look));
    let color = node.stroke.unwrap_or_else(|| border(look));
    let stroke = Stroke::new(color, node.stroke_width.unwrap_or(1.6));
    match node.shape {
        Shape::Rectangle => canvas.rect(Layer::Bars, box_of(rect), 4.0, Some(fill), Some(stroke)),
        Shape::Round => {
            canvas.rect(
                Layer::Bars,
                box_of(rect),
                (rect.h * 0.28).min(16.0),
                Some(fill),
                Some(stroke),
            );
        }
        Shape::Stadium => canvas.rect(
            Layer::Bars,
            box_of(rect),
            rect.h / 2.0,
            Some(fill),
            Some(stroke),
        ),
        Shape::Subroutine => {
            canvas.rect(Layer::Bars, box_of(rect), 0.0, Some(fill), Some(stroke));
            let inset = (rect.w * 0.08).clamp(6.0, 14.0);
            canvas.line(
                Layer::Bars,
                (rect.x + inset, rect.y),
                (rect.x + inset, rect.bottom()),
                stroke,
            );
            canvas.line(
                Layer::Bars,
                (rect.right() - inset, rect.y),
                (rect.right() - inset, rect.bottom()),
                stroke,
            );
        }
        Shape::Circle => {
            canvas.ellipse(
                Layer::Bars,
                (rect.cx(), rect.cy()),
                (rect.w / 2.0, rect.h / 2.0),
                Some(fill),
                Some(stroke),
            );
        }
        Shape::DoubleCircle => {
            canvas.ellipse(
                Layer::Bars,
                (rect.cx(), rect.cy()),
                (rect.w / 2.0, rect.h / 2.0),
                Some(fill),
                Some(stroke),
            );
            let inset = (rect.w * 0.08).clamp(4.0, 8.0);
            canvas.ellipse(
                Layer::Bars,
                (rect.cx(), rect.cy()),
                (
                    (rect.w / 2.0 - inset).max(1.0),
                    (rect.h / 2.0 - inset).max(1.0),
                ),
                None,
                Some(stroke),
            );
        }
        Shape::Cylinder => draw_cylinder(canvas, rect, fill, stroke),
        Shape::Arrow(direction) => {
            canvas.polygon(
                Layer::Bars,
                &chevron(rect, direction),
                Some(fill),
                Some(stroke),
            );
        }
        shape => canvas.polygon(Layer::Bars, &outline(shape, rect), Some(fill), Some(stroke)),
    }
}

pub(super) fn box_of(rect: Rect) -> (f32, f32, f32, f32) {
    (rect.x, rect.y, rect.w, rect.h)
}

pub(super) fn draw_cylinder(canvas: &mut Canvas, rect: Rect, fill: Color, stroke: Stroke) {
    let ry = (rect.h * 0.14).clamp(6.0, 14.0);
    let d = format!(
        "M {x} {top} A {rx} {ry} 0 0 1 {right} {top} L {right} {bot} A {rx} {ry} 0 0 1 {x} {bot} Z",
        x = n(rect.x),
        right = n(rect.right()),
        top = n(rect.y + ry),
        bot = n(rect.bottom() - ry),
        rx = n(rect.w / 2.0),
        ry = n(ry),
    );
    canvas.path(
        Layer::Bars,
        &d,
        (rect.x, rect.y, rect.right(), rect.bottom()),
        Some(fill),
        Some(stroke),
    );
    canvas.ellipse(
        Layer::Bars,
        (rect.cx(), rect.y + ry),
        (rect.w / 2.0, ry),
        Some(fill),
        Some(stroke),
    );
}

pub(super) fn outline(shape: Shape, rect: Rect) -> Vec<(f32, f32)> {
    let Rect { x, y, w, h } = rect;
    let (cx, cy) = (rect.cx(), rect.cy());
    match shape {
        Shape::Diamond => vec![(cx, y), (x + w, cy), (cx, y + h), (x, cy)],
        Shape::Hexagon => {
            let inset = w * 0.2;
            vec![
                (x + inset, y),
                (x + w - inset, y),
                (x + w, cy),
                (x + w - inset, y + h),
                (x + inset, y + h),
                (x, cy),
            ]
        }
        Shape::Parallelogram => {
            let skew = w * 0.16;
            vec![(x + skew, y), (x + w, y), (x + w - skew, y + h), (x, y + h)]
        }
        Shape::ParallelogramAlt => {
            let skew = w * 0.16;
            vec![(x, y), (x + w - skew, y), (x + w, y + h), (x + skew, y + h)]
        }
        Shape::Trapezoid => {
            let skew = w * 0.14;
            vec![(x + skew, y), (x + w - skew, y), (x + w, y + h), (x, y + h)]
        }
        Shape::TrapezoidAlt => {
            let skew = w * 0.14;
            vec![(x, y), (x + w, y), (x + w - skew, y + h), (x + skew, y + h)]
        }
        Shape::Asymmetric => {
            let skew = w * 0.22;
            vec![
                (x, y),
                (x + w - skew, y),
                (x + w, cy),
                (x + w - skew, y + h),
                (x, y + h),
            ]
        }
        _ => vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)],
    }
}

pub(super) fn chevron(rect: Rect, direction: Direction) -> Vec<(f32, f32)> {
    let Rect { x, y, w, h } = rect;
    let inset = (w.min(h) * 0.28).clamp(10.0, 22.0);
    let (cx, cy) = (rect.cx(), rect.cy());
    match direction {
        Direction::Right => vec![
            (x, y),
            (x + w - inset, y),
            (x + w, cy),
            (x + w - inset, y + h),
            (x, y + h),
            (x + inset * 0.75, cy),
        ],
        Direction::Left => vec![
            (x + w, y),
            (x + inset, y),
            (x, cy),
            (x + inset, y + h),
            (x + w, y + h),
            (x + w - inset * 0.75, cy),
        ],
        Direction::Down => vec![
            (x, y),
            (x, y + h - inset),
            (cx, y + h),
            (x + w, y + h - inset),
            (x + w, y),
            (cx, y + inset * 0.75),
        ],
        Direction::Up => vec![
            (x, y + h),
            (x, y + inset),
            (cx, y),
            (x + w, y + inset),
            (x + w, y + h),
            (cx, y + h - inset * 0.75),
        ],
        Direction::BothAcross => vec![
            (x + inset, y),
            (x + w - inset, y),
            (x + w, cy),
            (x + w - inset, y + h),
            (x + inset, y + h),
            (x, cy),
        ],
        Direction::BothDown => vec![
            (x, y + inset),
            (cx, y),
            (x + w, y + inset),
            (x + w, y + h - inset),
            (cx, y + h),
            (x, y + h - inset),
        ],
    }
}

pub(super) fn draw_edge(canvas: &mut Canvas, edge: &Edge, points: &[(f32, f32)], look: &Look) {
    if points.len() < 2 {
        return;
    }
    let mut stroke = Stroke::new(look.line, if edge.thick { 2.8 } else { 1.6 });
    if edge.dotted {
        stroke = stroke.dashed(5.0, 4.0);
    }
    let head = (look.font_size * 0.55).clamp(8.0, 14.0);
    let mut shaft = points.to_vec();
    if edge.arrow_end {
        pull_end(&mut shaft, head * 0.85);
    }
    if edge.arrow_start {
        pull_start(&mut shaft, head * 0.85);
    }
    canvas.polyline(Layer::Front, &shaft, stroke);
    if edge.arrow_end
        && let Some(dir) = unit(points[points.len() - 2], points[points.len() - 1])
    {
        canvas.polygon(
            Layer::Front,
            &arrow_head(points[points.len() - 1], dir, head),
            Some(look.line),
            None,
        );
    }
    if edge.arrow_start
        && let Some(dir) = unit(points[1], points[0])
    {
        canvas.polygon(
            Layer::Front,
            &arrow_head(points[0], dir, head),
            Some(look.line),
            None,
        );
    }
}

pub(super) fn pull_end(points: &mut [(f32, f32)], amount: f32) {
    let last = points.len() - 1;
    if let Some((dx, dy)) = unit(points[last - 1], points[last]) {
        let len = (points[last].0 - points[last - 1].0).hypot(points[last].1 - points[last - 1].1);
        let cut = amount.min(len * 0.6);
        points[last] = (points[last].0 - dx * cut, points[last].1 - dy * cut);
    }
}

pub(super) fn pull_start(points: &mut [(f32, f32)], amount: f32) {
    if let Some((dx, dy)) = unit(points[1], points[0]) {
        let len = (points[0].0 - points[1].0).hypot(points[0].1 - points[1].1);
        let cut = amount.min(len * 0.6);
        points[0] = (points[0].0 - dx * cut, points[0].1 - dy * cut);
    }
}

pub(super) fn unit(a: (f32, f32), b: (f32, f32)) -> Option<(f32, f32)> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = dx.hypot(dy);
    (len >= 0.5).then_some((dx / len, dy / len))
}

pub(super) fn arrow_head(tip: (f32, f32), dir: (f32, f32), size: f32) -> Vec<(f32, f32)> {
    let base = (tip.0 - dir.0 * size, tip.1 - dir.1 * size);
    let px = -dir.1 * size * 0.42;
    let py = dir.0 * size * 0.42;
    vec![tip, (base.0 + px, base.1 + py), (base.0 - px, base.1 - py)]
}
