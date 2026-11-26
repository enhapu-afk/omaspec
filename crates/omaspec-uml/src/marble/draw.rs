//! A marble diagram drawn as the reference draws it
//! (`marbleRenderer.ts`, `styles.ts`): each lane a line with its name at
//! its left, its values marbles on it, its spans pills, its end a bar
//! (`|`) or a cross (`#`, `!`), a break in time two strokes over a gap,
//! the axis's ticks with a dotted line down through every lane. A
//! labelled character takes a color of the palette; a fill that would
//! sink into a dark page is lifted toward its outline, and the words on
//! a fill read on it.
//!
//! What a lane's line writes is tied to it: its name, each marble's
//! character at its byte, its times; a label's words to the label's
//! line, the first time they are drawn, the others as echoes.

use super::layout::{self, Laid, LaidLane, Says};
use super::{Diagram, End, Item, Lane};
use crate::chart::words;
use crate::color::Color;
use crate::map::Map;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{Block, Font};

/// The SVG of `d` and where what its lines write is drawn.
pub fn draw(d: &Diagram) -> (String, Map) {
    let look = &d.look;
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let laid = layout::lay(d, canvas.family());
    let ground = look.ground();
    let radius = d.config.radius;
    let right = laid.right(d.columns);
    if let Some(top) = laid.axis {
        // Faint, as a chart's grid is: in its group, which how well a
        // picture reads leaves out.
        let grid = Stroke::new(look.grid.mix(ground, 0.5), 1.0).dashed(2.0, 5.0);
        canvas.begin_group(Layer::Background, crate::svg::GRID);
        for tick in &d.ticks {
            let x = laid.x(tick.col);
            canvas.line(
                Layer::Background,
                (x, laid.top - 6.0),
                (x, laid.bottom),
                grid,
            );
        }
        canvas.end_group(Layer::Background);
        for tick in &d.ticks {
            let x = laid.x(tick.col);
            let block = words(&tick.text, laid.fonts.tick, canvas.family())
                .written_at(tick.line, Some(tick.byte));
            canvas.text(Layer::Front, &block, (x, top), Align::Center);
        }
    }
    let mut said = vec![false; d.labels.len()];
    for (lane, at) in d.lanes.iter().zip(&laid.lanes) {
        draw_lane(&mut canvas, d, &laid, lane, at, &mut said);
    }
    if let Some(title) = &laid.title {
        let middle = (laid.x0 + right.max(laid.x0)) / 2.0;
        canvas.text(Layer::Front, title, (middle, 0.0), Align::Center);
    }
    // Room for a marble at either end of the time.
    canvas.include(laid.x0 - radius, laid.top);
    canvas.include(right + radius, laid.bottom);
    let map = canvas.place(Map::default(), d.config.padding);
    (canvas.finish(d.config.padding, look.page), map)
}

fn draw_lane(
    canvas: &mut Canvas,
    d: &Diagram,
    laid: &Laid,
    lane: &Lane,
    at: &LaidLane,
    said: &mut [bool],
) {
    let look = &d.look;
    let radius = d.config.radius;
    let y = at.y;
    let family = canvas.family();
    let name = words(&lane.name, laid.fonts.lane, family).written_at(lane.line, Some(lane.byte));
    canvas.text(
        Layer::Front,
        &name,
        (laid.names, y - name.height / 2.0),
        Align::Right,
    );
    let line = Stroke::new(look.lane, 1.5);
    for &(from, to) in &lane.segments {
        if to > from {
            canvas.line(Layer::Lifelines, (laid.x(from), y), (laid.x(to), y), line);
        }
    }
    for &col in &d.breaks {
        if lane
            .segments
            .iter()
            .any(|&(from, to)| from <= col && col <= to)
        {
            cut(canvas, d, laid.x(col), y);
        }
    }
    let height = radius * 1.6;
    for (index, item) in lane.items.iter().enumerate() {
        if let Item::Span {
            from,
            to,
            end,
            subscription,
            ..
        } = item
        {
            let label = layout::label_of(laid, item);
            let (fill, stroke, ink) = span_colors(d, label);
            let (x1, x2) = (laid.x(*from), laid.x(*to));
            let rect = (x1, y - height / 2.0, (x2 - x1).max(0.0), height);
            canvas.rect(
                Layer::Frames,
                rect,
                rect.2.min(rect.3) / 2.0,
                Some(fill),
                Some(Stroke::new(stroke, 1.0)),
            );
            canvas.shape_of(lane.line, rect);
            if *subscription {
                subscribed(canvas, look.complete, x1, y, height / 2.0);
            }
            if *end == End::Open {
                let tip = x2 - 4.0;
                let d = format!(
                    "M{} {} l4 4 l-4 4 M{} {} l4 4 l-4 4",
                    tip - 10.0,
                    y - 4.0,
                    tip - 5.0,
                    y - 4.0
                );
                canvas.path(
                    Layer::Frames,
                    &d,
                    (tip - 10.0, y - 4.0, tip - 1.0, y + 4.0),
                    None,
                    Some(Stroke::new(ink, 1.5)),
                );
            }
            if let Some(label) = label.filter(|_| at.inside.binary_search(&index).is_ok()) {
                let font = Font {
                    color: ink,
                    ..laid.fonts.label
                };
                let block = words(&d.labels[label].text, font, family);
                let shift = if *end == End::Open { 6.0 } else { 0.0 };
                said_once(
                    canvas,
                    said,
                    label,
                    d,
                    block,
                    ((x1 + x2) / 2.0 - shift, y),
                    true,
                );
            }
        }
    }
    for item in &lane.items {
        if let Item::Time { col, .. } = item {
            cut(canvas, d, laid.x(*col), y);
        }
    }
    for item in &lane.items {
        let (col, shifted) = match item {
            Item::Complete { col, grouped } | Item::Error { col, grouped } => (*col, *grouped),
            Item::Cancel { col } => (*col, false),
            _ => continue,
        };
        let cx = laid.x(col)
            + if shifted {
                layout::grouped_shift(radius)
            } else {
                0.0
            };
        let mark = match item {
            Item::Complete { .. } => {
                canvas.line(
                    Layer::Bars,
                    (cx, y - radius),
                    (cx, y + radius),
                    Stroke::new(look.complete, 2.5),
                );
                (cx - 1.5, y - radius, 3.0, 2.0 * radius)
            }
            _ => {
                let error = matches!(item, Item::Error { .. });
                let size = radius * if error { 0.7 } else { 0.55 };
                let stroke = if error {
                    Stroke::new(look.error, 3.0)
                } else {
                    Stroke::new(look.cancel, 2.0)
                };
                cross(canvas, (cx, y), size, stroke);
                (cx - size, y - size, 2.0 * size, 2.0 * size)
            }
        };
        canvas.shape_of(lane.line, mark);
    }
    for item in &lane.items {
        match item {
            Item::Subscribe { col } => subscribed(canvas, look.complete, laid.x(*col), y, radius),
            Item::Group { col, size, .. } => {
                let half = if *size > 1 {
                    (*size - 1) as f32 * layout::pitch(radius) / 2.0 + radius
                } else {
                    radius
                };
                let cx = laid.x(*col);
                canvas.rect(
                    Layer::Bars,
                    (
                        cx - radius - 3.0,
                        y - half - 3.0,
                        2.0 * radius + 6.0,
                        2.0 * half + 6.0,
                    ),
                    radius + 3.0,
                    None,
                    Some(Stroke::new(look.lane, 1.0)),
                );
            }
            _ => {}
        }
    }
    for item in &lane.items {
        let Item::Event {
            symbol,
            col,
            stack,
            byte,
        } = item
        else {
            continue;
        };
        let label = layout::label_of(laid, item);
        let (fill, stroke, ink) = event_colors(d, label);
        let cx = laid.x(*col);
        let cy = y + stack.map_or(0.0, |(index, size)| layout::stacked(index, size, radius));
        canvas.ellipse(
            Layer::Front,
            (cx, cy),
            (radius, radius),
            Some(fill),
            Some(Stroke::new(stroke, 1.5)),
        );
        let block = words(
            &symbol.to_string(),
            Font {
                color: ink,
                ..laid.fonts.symbol
            },
            family,
        )
        .written_at(lane.line, *byte);
        canvas.text(
            Layer::Front,
            &block,
            (cx, cy - block.height / 2.0),
            Align::Center,
        );
    }
    let leader = Stroke::new(look.grid, 1.0).dashed(2.0, 3.0);
    for placed in &at.labels {
        let top = y + laid.row_top(placed.row, at.extent, placed.block.height);
        // On the page's color, so the axis's dotted lines pass behind.
        if let Some(page) = look.page {
            let half = placed.block.width / 2.0 + 2.0;
            canvas.plain(|canvas| {
                canvas.rect(
                    Layer::Background,
                    (placed.x - half, top, 2.0 * half, placed.block.height),
                    2.0,
                    Some(page),
                    None,
                );
            });
        }
        if placed.row > 1 {
            canvas.line(
                Layer::Background,
                (placed.x, y + at.extent + 2.0),
                (placed.x, top - 2.0),
                leader,
            );
        }
        match placed.says {
            Says::Time(_) => {
                canvas.text(Layer::Front, &placed.block, (placed.x, top), Align::Center);
            }
            Says::Label(index) => {
                said_once(
                    canvas,
                    said,
                    index,
                    d,
                    placed.block.clone(),
                    (placed.x, top),
                    false,
                );
            }
        }
    }
}

/// A label's words at `(x, y)`, their top there or, `centered`, their
/// middle: tied to the label's line the first time, an echo after.
fn said_once(
    canvas: &mut Canvas,
    said: &mut [bool],
    index: usize,
    d: &Diagram,
    block: Block,
    (x, y): (f32, f32),
    centered: bool,
) {
    let label = &d.labels[index];
    let block = block.written_at(label.line, Some(label.byte));
    let top = if centered { y - block.height / 2.0 } else { y };
    let again = std::mem::replace(&mut said[index], true);
    let before = canvas.echo(again);
    canvas.text(Layer::Front, &block, (x, top), Align::Center);
    canvas.echo(before);
}

/// `^`: an upright stroke across the lane, a caret under it.
fn subscribed(canvas: &mut Canvas, color: Color, x: f32, y: f32, extent: f32) {
    let bottom = y + extent + 2.0;
    let d = format!(
        "M{x} {} L{x} {bottom} M{} {} L{x} {bottom} L{} {}",
        y - extent - 2.0,
        x - 4.0,
        bottom + 5.0,
        x + 4.0,
        bottom + 5.0
    );
    canvas.path(
        Layer::Bars,
        &d,
        (x - 4.0, y - extent - 2.0, x + 4.0, bottom + 5.0),
        None,
        Some(Stroke::new(color, 2.0)),
    );
}

fn cross(canvas: &mut Canvas, (x, y): (f32, f32), size: f32, stroke: Stroke) {
    canvas.line(
        Layer::Bars,
        (x - size, y - size),
        (x + size, y + size),
        stroke,
    );
    canvas.line(
        Layer::Bars,
        (x + size, y - size),
        (x - size, y + size),
        stroke,
    );
}

/// A break in time: two strokes over a gap in the lane.
fn cut(canvas: &mut Canvas, d: &Diagram, x: f32, y: f32) {
    let look = &d.look;
    canvas.plain(|canvas| {
        canvas.rect(
            Layer::Lifelines,
            (x - 4.0, y - 3.0, 8.0, 6.0),
            0.0,
            Some(look.break_ground),
            None,
        );
    });
    let stroke = Stroke::new(look.break_mark, 1.5);
    canvas.line(
        Layer::Lifelines,
        (x - 7.0, y + 7.0),
        (x - 1.0, y - 7.0),
        stroke,
    );
    canvas.line(
        Layer::Lifelines,
        (x + 1.0, y + 7.0),
        (x + 7.0, y - 7.0),
        stroke,
    );
}

/// A marble's fill, outline and words: the theme's, or its label's color.
fn event_colors(d: &Diagram, label: Option<usize>) -> (Color, Color, Color) {
    let look = &d.look;
    let ground = look.ground();
    let (fill, stroke) = match label {
        None => (look.event_fill, look.event_stroke),
        Some(index) => {
            let c = look.color(index);
            (c, c.mix(look.label, 0.35))
        }
    };
    let (fill, stroke) = lifted(fill, stroke, ground);
    let ink = match label {
        None => look.event_text,
        Some(_) => Color::WHITE,
    };
    (fill, stroke, legible(ink, fill))
}

/// A span's fill, outline and words: the theme's, or its label's color
/// toward the page.
fn span_colors(d: &Diagram, label: Option<usize>) -> (Color, Color, Color) {
    let look = &d.look;
    let ground = look.ground();
    let (fill, stroke) = match label {
        None => (look.span_fill, look.span_stroke),
        Some(index) => {
            let c = look.color(index);
            // Its outline reads on the page, a step toward the words
            // where the palette's color alone would not.
            (c.mix(ground, 0.55), c.legible_on(ground, look.label, 3.0))
        }
    };
    let (fill, stroke) = lifted(fill, stroke, ground);
    (fill, stroke, legible(look.span_text, fill))
}

/// On a dark page, a fill near black (a luminance under 0.05, which is
/// a contrast of 2 to black), mixed toward its outline from a quarter of
/// the way on until it no longer sinks into the page.
fn lifted(fill: Color, outline: Color, ground: Color) -> (Color, Color) {
    if !ground.is_dark() {
        return (fill, outline);
    }
    let mut lifted = fill;
    let mut share = 0.25;
    while lifted.contrast(Color::BLACK) < 2.0 && share <= 0.5 {
        lifted = fill.mix(outline, share);
        share += 0.05;
    }
    (lifted, outline)
}

/// `ink` where it reads on `ground`, else black or white, whichever
/// reads more.
fn legible(ink: Color, ground: Color) -> Color {
    if ink.contrast(ground) >= 4.5 {
        return ink;
    }
    let far = if Color::WHITE.contrast(ground) > Color::BLACK.contrast(ground) {
        Color::WHITE
    } else {
        Color::BLACK
    };
    ink.legible_on(ground, far, 4.5)
}

#[cfg(test)]
mod tests {
    use super::super::from_mermaid;
    use super::*;

    #[test]
    fn labels_that_would_meet_take_rows_of_their_own() {
        let mut d = from_mermaid(
            "marble-beta\n  clicks : -abc-|\n  label a \"the first\"\n  label b \"the second\"\n  label c \"the third\"\n",
        )
        .unwrap();
        d.look.page = Some(Color::WHITE);
        let laid = layout::lay(&d, Canvas::new(crate::FACE, false).family());
        let rows: Vec<usize> = laid.lanes[0].labels.iter().map(|l| l.row).collect();
        assert_eq!(rows, [0, 1, 2]);
        // The third stands under the first, with a line up to its marble.
        assert!(laid.row_top(2, 10.0, 14.0) > laid.row_top(0, 10.0, 14.0));
        let (svg, map) = draw(&d);
        assert!(svg.contains("the third"), "{svg}");
        // The lane's name, its three marbles and three labels are tied.
        assert_eq!(map.tied.len(), 7, "{:?}", map.tied);
    }

    #[test]
    fn a_label_drawn_again_is_an_echo() {
        let d = from_mermaid("marble-beta\n  a : -x-x-x|\n  label x \"tick\"\n").unwrap();
        let (_, map) = draw(&d);
        assert_eq!(map.echoes.len(), 2);
        let label_lines = map.tied.iter().filter(|t| t.line == 2).count();
        assert_eq!(label_lines, 1);
    }

    #[test]
    fn a_dark_fill_is_lifted_and_its_words_read() {
        let ground = Color::hex(0x121212);
        let (fill, _) = lifted(Color::hex(0x101010), Color::hex(0xe0e0e0), ground);
        assert!(fill.contrast(Color::BLACK) >= 2.0);
        assert!(legible(Color::hex(0x202020), fill).contrast(fill) >= 4.5);
        // A light page keeps the colors as they are.
        let light = Color::hex(0xECECFF);
        assert_eq!(lifted(light, Color::BLACK, Color::WHITE).0, light);
    }
}
