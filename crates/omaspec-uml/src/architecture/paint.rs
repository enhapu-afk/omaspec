//! Painting a laid-out architecture diagram: each service as its icon on a
//! tile, the groups as frames, the edges with their heads.

use super::lay::{Laid, Rect, lay};
use super::{CLASSIC_TILE, Diagram, Icon, Look, MARGIN, TILE};
use crate::Declined;
use crate::color::Color;
use crate::geom::shorten;
use crate::svg::{Align, Canvas, Layer, Stroke, n};

/// The SVG. Labels sit under the tiles, and a line leaves below a label
/// when the port is the bottom.
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

pub(super) fn page_ink(look: &Look) -> Color {
    let page = look.background.unwrap_or(Color::WHITE);
    if page.is_dark() != look.preferred.is_dark() {
        look.preferred
    } else if page.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

pub(super) fn tile_color(look: &Look) -> Color {
    if look.classic {
        Color::hex(CLASSIC_TILE)
    } else {
        look.accent
    }
}

pub(super) fn glyph_ink(tile: Color) -> Color {
    if tile.is_dark() {
        Color::WHITE
    } else {
        Color::hex(0x1A1A1A)
    }
}

pub(super) fn paint(canvas: &mut Canvas, diagram: &Diagram, laid: &Laid) {
    let look = &diagram.look;
    let tile = tile_color(look);
    let glyph = glyph_ink(tile);
    for group in &laid.groups {
        canvas.rect(
            Layer::Frames,
            (group.rect.x, group.rect.y, group.rect.w, group.rect.h),
            10.0,
            None,
            Some(Stroke::new(look.line, 1.25).dashed(7.0, 5.0)),
        );
        let icon = Rect {
            x: group.rect.x + 8.0,
            y: group.rect.y + 8.0,
            w: 22.0,
            h: 22.0,
        };
        paint_tile(canvas, icon, tile, group.icon, glyph);
        canvas.text(
            Layer::Front,
            &group.label,
            (
                icon.right() + 6.0,
                icon.y + (icon.h - group.label.height) / 2.0,
            ),
            Align::Left,
        );
    }
    for edge in &laid.edges {
        let mut points = edge.points.clone();
        if edge.arrow_end {
            shorten(&mut points, 8.0, true);
        }
        if edge.arrow_start {
            shorten(&mut points, 8.0, false);
        }
        if points.len() >= 2 {
            canvas.polyline(Layer::Bars, &points, Stroke::new(look.line, 1.75));
        }
        if edge.arrow_end
            && let (Some(tip), Some(from)) = (edge.points.last(), edge.points.iter().rev().nth(1))
        {
            arrow(canvas, *tip, *from, look.line);
        }
        if edge.arrow_start
            && let (Some(tip), Some(from)) = (edge.points.first(), edge.points.get(1))
        {
            arrow(canvas, *tip, *from, look.line);
        }
    }
    for (index, node) in laid.nodes.iter().enumerate() {
        if diagram.nodes[index].junction {
            canvas.ellipse(
                Layer::Bars,
                (node.tile.cx(), node.tile.cy()),
                (node.tile.w / 2.0, node.tile.h / 2.0),
                Some(look.line),
                None,
            );
            continue;
        }
        paint_tile(canvas, node.tile, tile, diagram.nodes[index].icon, glyph);
        if let Some(label) = &node.label {
            canvas.text(
                Layer::Front,
                label,
                (node.tile.cx(), node.tile.bottom() + 4.0),
                Align::Center,
            );
        }
    }
    if let Some(title) = &laid.title {
        let right = laid
            .nodes
            .iter()
            .map(|node| node.tile.right())
            .chain(laid.groups.iter().map(|group| group.rect.right()))
            .fold(title.width, f32::max);
        let left = laid
            .nodes
            .iter()
            .map(|node| node.tile.x)
            .chain(laid.groups.iter().map(|group| group.rect.x))
            .fold(0.0_f32, f32::min);
        canvas.text(
            Layer::Front,
            title,
            ((left + right) / 2.0, 0.0),
            Align::Center,
        );
    }
}

pub(super) fn paint_tile(canvas: &mut Canvas, tile: Rect, fill: Color, icon: Icon, glyph: Color) {
    canvas.card(Layer::Bars, (tile.x, tile.y, tile.w, tile.h), 8.0, fill);
    match icon {
        Icon::Cloud => paint_cloud(canvas, tile, glyph),
        Icon::Database => paint_database(canvas, tile, glyph),
        Icon::Disk => paint_disk(canvas, tile, glyph),
        Icon::Internet => paint_internet(canvas, tile, glyph),
        Icon::Server => paint_server(canvas, tile, glyph),
        Icon::Plain => paint_plain(canvas, tile, glyph),
    }
}

pub(super) fn paint_cloud(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let cx = tile.cx();
    let cy = tile.cy();
    let xy = |x: f32, y: f32| format!("{},{}", n(cx + x * scale), n(cy + y * scale));
    // A flat base and three humps. Filled circles at this size merge into a blob.
    let path = format!(
        "M{} C{} {} {} C{} {} {} C{} {} {} C{} {} {} Z",
        xy(-14.0, 6.0),
        xy(-22.0, 6.0),
        xy(-22.0, -3.0),
        xy(-12.0, -4.0),
        xy(-14.0, -14.0),
        xy(-2.0, -14.0),
        xy(-1.0, -5.0),
        xy(1.0, -16.0),
        xy(14.0, -14.0),
        xy(13.0, -4.0),
        xy(22.0, -6.0),
        xy(22.0, 6.0),
        xy(12.0, 6.0),
    );
    canvas.path(
        Layer::Front,
        &path,
        (
            cx - 22.0 * scale,
            cy - 16.0 * scale,
            cx + 22.0 * scale,
            cy + 8.0 * scale,
        ),
        Some(glyph),
        None,
    );
}

pub(super) fn paint_database(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let rx = 14.0 * scale;
    let ry = 5.5 * scale;
    let cx = tile.cx();
    let top = tile.cy() - 12.0 * scale;
    let bot = tile.cy() + 12.0 * scale;
    let x = cx - rx;
    let stroke = Stroke::new(glyph, 1.6);
    let body = format!(
        "M{},{} A{},{} 0 0 1 {},{} L{},{} A{},{} 0 0 1 {},{} Z",
        n(x),
        n(top),
        n(rx),
        n(ry),
        n(x + rx * 2.0),
        n(top),
        n(x + rx * 2.0),
        n(bot),
        n(rx),
        n(ry),
        n(x),
        n(bot)
    );
    canvas.path(
        Layer::Front,
        &body,
        (x, top - ry, x + rx * 2.0, bot + ry),
        None,
        Some(stroke),
    );
    let lip = format!(
        "M{},{} A{},{} 0 0 0 {},{}",
        n(x),
        n(top),
        n(rx),
        n(ry),
        n(x + rx * 2.0),
        n(top)
    );
    canvas.path(
        Layer::Front,
        &lip,
        (x, top - ry, x + rx * 2.0, top + ry),
        None,
        Some(stroke),
    );
}

pub(super) fn paint_disk(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let stroke = Stroke::new(glyph, 1.6);
    canvas.ellipse(
        Layer::Front,
        (tile.cx(), tile.cy()),
        (14.0 * scale, 14.0 * scale),
        None,
        Some(stroke),
    );
    canvas.ellipse(
        Layer::Front,
        (tile.cx(), tile.cy()),
        (4.0 * scale, 4.0 * scale),
        Some(glyph),
        None,
    );
}

pub(super) fn paint_internet(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let stroke = Stroke::new(glyph, 1.6);
    let radius = 13.0 * scale;
    canvas.ellipse(
        Layer::Front,
        (tile.cx(), tile.cy()),
        (radius, radius),
        None,
        Some(stroke),
    );
    canvas.ellipse(
        Layer::Front,
        (tile.cx(), tile.cy()),
        (radius * 0.45, radius),
        None,
        Some(stroke),
    );
    canvas.line(
        Layer::Front,
        (tile.cx() - radius, tile.cy()),
        (tile.cx() + radius, tile.cy()),
        stroke,
    );
}

pub(super) fn paint_server(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let width = 28.0 * scale;
    let height = 7.0 * scale;
    let x = tile.cx() - width / 2.0;
    let y = tile.cy() - height * 2.1;
    let stroke = Stroke::new(glyph, 1.4);
    for row in 0..3 {
        let top = y + row as f32 * (height + 3.0 * scale);
        canvas.rect(
            Layer::Front,
            (x, top, width, height),
            2.0,
            None,
            Some(stroke),
        );
    }
}

pub(super) fn paint_plain(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let side = 18.0 * scale;
    let x = tile.cx() - side / 2.0;
    let y = tile.cy() - side / 2.0;
    canvas.rect(
        Layer::Front,
        (x, y, side, side),
        3.0,
        None,
        Some(Stroke::new(glyph, 1.6)),
    );
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
    let px = -uy;
    let py = ux;
    let base = (tip.0 - ux * 9.0, tip.1 - uy * 9.0);
    canvas.polygon(
        Layer::Front,
        &[
            tip,
            (base.0 + px * 4.2, base.1 + py * 4.2),
            (base.0 - px * 4.2, base.1 - py * 4.2),
        ],
        Some(color),
        None,
    );
}
