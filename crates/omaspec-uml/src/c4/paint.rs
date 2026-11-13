//! Painting a laid-out C4 diagram: the shapes in the colors of their kind,
//! the boundaries dashed, the relationships with their heads and labels.

use super::lay::{Laid, Mark, Rect, lay, stack_height};
use super::{BOUND_ROUND, Boundary, Diagram, Kind, Look, MARGIN, ROUND, Shape};
use crate::Declined;
use crate::color::{Color, Paint};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::Block;

/// The SVG. Relationship labels sit in the gap the layout left for them.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    if !(4.0..=100.0).contains(&diagram.look.font_size) {
        return Err(Declined::at(0, "the font size is not one Omaspec draws"));
    }
    let mut canvas = Canvas::new(&diagram.look.font_name, false);
    canvas.sketch(diagram.look.sketch);
    let family = canvas.family();
    let laid = lay(diagram, family)?;
    paint(&mut canvas, diagram, &laid);
    Ok(canvas.finish(MARGIN, diagram.look.background))
}

pub(super) struct ShapePaint {
    pub(super) fill: Color,
    pub(super) border: Color,
    pub(super) ink: Color,
    pub(super) dashed: bool,
    pub(super) mark: Mark,
}

pub(super) fn shape_paint(shape: &Shape, look: &Look) -> ShapePaint {
    let (fill, border) = shape
        .fill
        .zip(shape.border)
        .unwrap_or_else(|| family_colors(shape.kind, look));
    let fill = shape.fill.unwrap_or(fill);
    let border = shape.border.unwrap_or(border);
    let mark = shape.kind.mark();
    let ink = shape.ink.unwrap_or_else(|| {
        if mark == Mark::Figure {
            page_ink(look)
        } else {
            readable(fill, look.preferred)
        }
    });
    ShapePaint {
        fill,
        border,
        ink,
        dashed: shape.kind.external(),
        mark,
    }
}

pub(super) fn family_colors(kind: Kind, look: &Look) -> (Color, Color) {
    if look.classic {
        classic_colors(kind)
    } else {
        let page = look.background.unwrap_or(Color::WHITE);
        let share = match kind {
            Kind::Person => 0.0,
            Kind::System | Kind::SystemDb | Kind::SystemQueue => 0.28,
            Kind::Container | Kind::ContainerDb | Kind::ContainerQueue => 0.48,
            Kind::Component | Kind::ComponentDb | Kind::ComponentQueue => 0.68,
            Kind::PersonExt => 0.82,
            Kind::SystemExt | Kind::SystemDbExt | Kind::SystemQueueExt => 0.62,
            Kind::ContainerExt | Kind::ContainerDbExt | Kind::ContainerQueueExt => 0.75,
            Kind::ComponentExt | Kind::ComponentDbExt | Kind::ComponentQueueExt => 0.86,
        };
        let fill = look.accent.mix(page, share);
        let border = if kind.external() {
            look.accent
        } else {
            look.accent.mix(Color::BLACK, 0.18)
        };
        (fill, border)
    }
}

pub(super) fn classic_colors(kind: Kind) -> (Color, Color) {
    let (fill, border) = match kind {
        Kind::Person => (0x08427B, 0x073B6F),
        Kind::PersonExt => (0x686868, 0x8A8A8A),
        Kind::System | Kind::SystemDb | Kind::SystemQueue => (0x1168BD, 0x3C7FC0),
        Kind::SystemExt | Kind::SystemDbExt | Kind::SystemQueueExt => (0x999999, 0x8A8A8A),
        Kind::Container | Kind::ContainerDb | Kind::ContainerQueue => (0x438DD5, 0x3C7FC0),
        Kind::ContainerExt | Kind::ContainerDbExt | Kind::ContainerQueueExt => (0xB3B3B3, 0xA6A6A6),
        Kind::Component | Kind::ComponentDb | Kind::ComponentQueue => (0x85BBF0, 0x78A8D8),
        Kind::ComponentExt | Kind::ComponentDbExt | Kind::ComponentQueueExt => (0xCCCCCC, 0xBFBFBF),
    };
    (Color::hex(fill), Color::hex(border))
}

pub(super) fn boundary_paint(boundary: &Boundary, look: &Look) -> (Paint, Color, Color) {
    let border = boundary
        .border
        .unwrap_or(if look.classic { look.line } else { look.accent });
    let ink = boundary.ink.unwrap_or_else(|| {
        boundary
            .fill
            .map(|fill| readable(fill, look.preferred))
            .unwrap_or_else(|| page_ink(look))
    });
    (boundary.fill, border, ink)
}

pub(super) fn page_ink(look: &Look) -> Color {
    readable(look.background.unwrap_or(Color::WHITE), look.preferred)
}

pub(super) fn readable(fill: Color, preferred: Color) -> Color {
    if fill.is_dark() != preferred.is_dark() {
        preferred
    } else if fill.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

pub(super) fn paint(canvas: &mut Canvas, diagram: &Diagram, laid: &Laid) {
    for (index, placed) in laid.boundaries.iter().enumerate() {
        let (fill, border, _) = boundary_paint(&diagram.boundaries[index], &diagram.look);
        canvas.rect(
            Layer::Frames,
            (placed.rect.x, placed.rect.y, placed.rect.w, placed.rect.h),
            BOUND_ROUND,
            fill,
            Some(Stroke::new(border, 1.25).dashed(7.0, 5.0)),
        );
        paint_lines(canvas, placed.rect.cx(), placed.rect.y + 8.0, &placed.lines);
    }
    for rel in &laid.rels {
        let mut stroke = Stroke::new(rel.color, diagram.look.thickness);
        if rel.dashed {
            stroke = stroke.dashed(7.0, 4.0);
        }
        let mut points = rel.points.clone();
        shorten(&mut points, 9.0, true);
        if rel.bi {
            shorten(&mut points, 9.0, false);
        }
        if points.len() >= 2 {
            canvas.polyline(Layer::Lifelines, &points, stroke);
        }
    }
    for (index, placed) in laid.shapes.iter().enumerate() {
        let style = shape_paint(&diagram.shapes[index], &diagram.look);
        paint_mark(canvas, placed.rect, &style, diagram.look.thickness);
        let top = match style.mark {
            Mark::Figure => placed.rect.y + 56.0,
            Mark::Cylinder => placed.rect.y + 20.0,
            _ => placed.rect.y + 10.0,
        };
        let text_h = stack_height(&placed.lines);
        let room = placed.rect.bottom() - top - 8.0;
        let y = top + (room - text_h).max(0.0) / 2.0;
        paint_lines(canvas, placed.rect.cx(), y, &placed.lines);
    }
    for rel in &laid.rels {
        if let (Some(first), Some(last)) = (rel.points.first(), rel.points.last()) {
            arrow(canvas, *last, prev_point(&rel.points, false), rel.color);
            if rel.bi {
                arrow(canvas, *first, prev_point(&rel.points, true), rel.color);
            }
        }
        let page = diagram.look.background.unwrap_or(Color::WHITE);
        canvas.rect(
            Layer::Front,
            (rel.chip.x, rel.chip.y, rel.chip.w, rel.chip.h),
            4.0,
            Some(page),
            Some(Stroke::new(rel.color.mix(page, 0.45), 1.0)),
        );
        paint_lines(
            canvas,
            rel.chip.x + rel.chip.w / 2.0,
            rel.chip.y + 4.0,
            &rel.lines,
        );
    }
    if let Some(title) = &laid.title {
        let right = laid
            .shapes
            .iter()
            .map(|shape| shape.rect.right())
            .chain(laid.boundaries.iter().map(|boundary| boundary.rect.right()))
            .fold(title.width, f32::max);
        let left = laid
            .shapes
            .iter()
            .map(|shape| shape.rect.x)
            .chain(laid.boundaries.iter().map(|boundary| boundary.rect.x))
            .fold(0.0_f32, f32::min);
        canvas.text(
            Layer::Front,
            title,
            ((left + right) / 2.0, 0.0),
            Align::Center,
        );
    }
}

pub(super) fn paint_lines(canvas: &mut Canvas, cx: f32, mut y: f32, lines: &[Block]) {
    for line in lines {
        canvas.text(Layer::Front, line, (cx, y), Align::Center);
        y += line.height + 2.0;
    }
}

pub(super) fn paint_mark(canvas: &mut Canvas, rect: Rect, style: &ShapePaint, width: f32) {
    let mut stroke = Stroke::new(style.border, width);
    if style.dashed {
        stroke = stroke.dashed(6.0, 4.0);
    }
    match style.mark {
        Mark::Figure => paint_figure(canvas, rect, style.fill, stroke),
        Mark::Cylinder => paint_cylinder(canvas, rect, style.fill, stroke),
        Mark::Queue => paint_queue(canvas, rect, style.fill, stroke),
        Mark::Box => canvas.rect(
            Layer::Bars,
            (rect.x, rect.y, rect.w, rect.h),
            ROUND,
            Some(style.fill),
            Some(stroke),
        ),
    }
}

pub(super) fn paint_figure(canvas: &mut Canvas, rect: Rect, fill: Color, stroke: Stroke) {
    let cx = rect.cx();
    let top = rect.y + 4.0;
    let radius = 12.0;
    canvas.ellipse(
        Layer::Bars,
        (cx, top + radius),
        (radius, radius),
        Some(fill),
        Some(stroke),
    );
    let neck = top + radius * 2.0 + 1.0;
    let d = format!(
        "M{},{} c-16,2 -20,16 -18,26 h36 c2,-10 -2,-24 -18,-26 z",
        crate::svg::n(cx),
        crate::svg::n(neck)
    );
    canvas.path(
        Layer::Bars,
        &d,
        (cx - 22.0, top, cx + 22.0, neck + 28.0),
        Some(fill),
        Some(stroke),
    );
}

pub(super) fn paint_cylinder(canvas: &mut Canvas, rect: Rect, fill: Color, stroke: Stroke) {
    let ry = 9.0;
    let x = rect.x;
    let y = rect.y + ry;
    let w = rect.w;
    let h = rect.h - ry * 2.0;
    let d = format!(
        "M{},{} A{},{} 0 0 1 {},{} L{},{} A{},{} 0 0 1 {},{} Z",
        crate::svg::n(x),
        crate::svg::n(y),
        crate::svg::n(w / 2.0),
        crate::svg::n(ry),
        crate::svg::n(x + w),
        crate::svg::n(y),
        crate::svg::n(x + w),
        crate::svg::n(y + h),
        crate::svg::n(w / 2.0),
        crate::svg::n(ry),
        crate::svg::n(x),
        crate::svg::n(y + h)
    );
    canvas.path(
        Layer::Bars,
        &d,
        (rect.x, rect.y, rect.right(), rect.bottom()),
        Some(fill),
        Some(stroke),
    );
    let lip = format!(
        "M{},{} A{},{} 0 0 0 {},{}",
        crate::svg::n(x),
        crate::svg::n(y),
        crate::svg::n(w / 2.0),
        crate::svg::n(ry),
        crate::svg::n(x + w),
        crate::svg::n(y)
    );
    canvas.path(
        Layer::Bars,
        &lip,
        (rect.x, rect.y, rect.right(), y + ry),
        None,
        Some(stroke),
    );
}

pub(super) fn paint_queue(canvas: &mut Canvas, rect: Rect, fill: Color, stroke: Stroke) {
    let bulge = 16.0;
    let x2 = rect.right() - bulge;
    let mid = rect.cy();
    let d = format!(
        "M{},{} L{},{} Q{},{} {},{} L{},{} Z",
        crate::svg::n(rect.x),
        crate::svg::n(rect.y),
        crate::svg::n(x2),
        crate::svg::n(rect.y),
        crate::svg::n(rect.right()),
        crate::svg::n(mid),
        crate::svg::n(x2),
        crate::svg::n(rect.bottom()),
        crate::svg::n(rect.x),
        crate::svg::n(rect.bottom())
    );
    canvas.path(
        Layer::Bars,
        &d,
        (rect.x, rect.y, rect.right(), rect.bottom()),
        Some(fill),
        Some(stroke),
    );
}

pub(super) fn prev_point(points: &[(f32, f32)], at_start: bool) -> (f32, f32) {
    if at_start {
        points.iter().skip(1).copied().next().unwrap_or(points[0])
    } else {
        points
            .iter()
            .rev()
            .nth(1)
            .copied()
            .unwrap_or(points[points.len() - 1])
    }
}

pub(super) fn arrow(canvas: &mut Canvas, tip: (f32, f32), from: (f32, f32), color: Color) {
    let dx = tip.0 - from.0;
    let dy = tip.1 - from.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.1 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let base = (tip.0 - ux * 9.0, tip.1 - uy * 9.0);
    let px = -uy * 3.8;
    let py = ux * 3.8;
    canvas.polygon(
        Layer::Front,
        &[tip, (base.0 + px, base.1 + py), (base.0 - px, base.1 - py)],
        Some(color),
        None,
    );
}

pub(super) fn shorten(points: &mut [(f32, f32)], len: f32, at_end: bool) {
    if points.len() < 2 {
        return;
    }
    if at_end {
        let last = points.len() - 1;
        let from = points[last - 1];
        pull(&mut points[last], from, len);
    } else {
        let toward = points[1];
        pull(&mut points[0], toward, len);
    }
}

pub(super) fn pull(point: &mut (f32, f32), toward: (f32, f32), len: f32) {
    let dx = toward.0 - point.0;
    let dy = toward.1 - point.1;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 0.1 {
        return;
    }
    let step = len.min(dist);
    point.0 += dx / dist * step;
    point.1 += dy / dist * step;
}
