//! Painting a ditaa drawing on its grid: a cell is 10 by 16 units, a box
//! runs from the middle of its corner cells to the middle of the others,
//! a line from the middle of a cell to the middle of the next.

use super::{Box, Diagram, Point, Shape};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::{self, Color, Paint};
use crate::common::{Skinparam, normalize_key};
use crate::graph::Rect;
use crate::structure::{Shape as Outline, paint_outline};
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{Font, Style, styled};

const CELL: (f32, f32) = (10.0, 16.0);
const MARGIN: f32 = 10.0;

pub fn draw(d: &Diagram) -> Result<String, Declined> {
    let look = Look::read(&d.frame.skinparams)?;
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let annotations = Annotations::new(
        &d.frame,
        [look.font.bold(), look.font, look.font, look.font],
        family,
    )?;
    let top = annotations.top();
    let at = |(r, c): (usize, usize)| {
        (
            c as f32 * CELL.0 + CELL.0 / 2.0,
            top + r as f32 * CELL.1 + CELL.1 / 2.0,
        )
    };
    let line = Stroke::new(look.ink, 1.2);
    // The boxes, the larger first, so one inside another stays over it;
    // each casts its shadow as a box of PlantUML's does.
    canvas.shadows(d.shadows);
    let mut boxes: Vec<&Box> = d.boxes.iter().collect();
    boxes.sort_by_key(|b| std::cmp::Reverse((b.bottom - b.top) * (b.right - b.left)));
    for b in boxes {
        let fill = b.color.unwrap_or(look.page.unwrap_or(Color::WHITE));
        let stroke = if b.dashed {
            line.dashed(5.0, 3.0)
        } else {
            line
        };
        paint_box(&mut canvas, b, rect_of(b, at), fill, stroke);
    }
    canvas.shadows(false);
    // The lines, then the heads over them; a head right before a box's side
    // reaches it.
    for (from, to, dashed) in &d.lines {
        let stroke = if *dashed { line.dashed(5.0, 3.0) } else { line };
        canvas.line(Layer::Lines, at(*from), at(*to), stroke);
    }
    for &((r, c), point) in &d.heads {
        let next = match point {
            Point::Right => Some((r, c + 1)),
            Point::Left => c.checked_sub(1).map(|c| (r, c)),
            Point::Up => r.checked_sub(1).map(|r| (r, c)),
            Point::Down => Some((r + 1, c)),
        };
        let (x, y) = match next.filter(|cell| d.boxes.iter().any(|b| on_side(b, *cell))) {
            Some(cell) => {
                let dashed = d
                    .lines
                    .iter()
                    .any(|(a, b, dashed)| *dashed && (*a == (r, c) || *b == (r, c)));
                let stroke = if dashed { line.dashed(5.0, 3.0) } else { line };
                canvas.line(Layer::Lines, at((r, c)), at(cell), stroke);
                at(cell)
            }
            None => {
                let (x, y) = at((r, c));
                match point {
                    Point::Right => (x + 5.0, y),
                    Point::Left => (x - 5.0, y),
                    Point::Up => (x, y - 5.0),
                    Point::Down => (x, y + 5.0),
                }
            }
        };
        let s = 5.0;
        let tip: [(f32, f32); 3] = match point {
            Point::Right => [
                (x, y),
                (x - 2.0 * s, y - s * 0.8),
                (x - 2.0 * s, y + s * 0.8),
            ],
            Point::Left => [
                (x, y),
                (x + 2.0 * s, y - s * 0.8),
                (x + 2.0 * s, y + s * 0.8),
            ],
            Point::Up => [
                (x, y),
                (x - s * 0.8, y + 2.0 * s),
                (x + s * 0.8, y + 2.0 * s),
            ],
            Point::Down => [
                (x, y),
                (x - s * 0.8, y - 2.0 * s),
                (x + s * 0.8, y - 2.0 * s),
            ],
        };
        canvas.polygon(Layer::Front, &tip, Some(look.ink), None);
    }
    // The words where they were written, read on what lies under them.
    for words in &d.texts {
        let (r, c) = words.at;
        let ground = words
            .inside
            .and_then(|i| d.boxes[i].color)
            .unwrap_or(look.page.unwrap_or(Color::WHITE));
        let ink = legible(look.ink, ground);
        let font = Font {
            color: ink,
            ..look.font
        };
        let block = styled(
            vec![vec![(words.text.clone(), Style::from(font))]],
            font,
            family,
        );
        let x = c as f32 * CELL.0;
        let y = top + r as f32 * CELL.1 + (CELL.1 - block.height) / 2.0;
        canvas.text(Layer::Front, &block, (x, y), Align::Left);
    }
    annotations.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, look.page))
}

/// The rectangle a box spans, from the middle of its corner cells.
fn rect_of(b: &Box, at: impl Fn((usize, usize)) -> (f32, f32)) -> Rect {
    let (x0, y0) = at((b.top, b.left));
    let (x1, y1) = at((b.bottom, b.right));
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// Whether `cell` lies on one of the sides of `b`.
fn on_side(b: &Box, (r, c): (usize, usize)) -> bool {
    let across = (r == b.top || r == b.bottom) && (b.left..=b.right).contains(&c);
    let down = (c == b.left || c == b.right) && (b.top..=b.bottom).contains(&r);
    across || down
}

fn paint_box(canvas: &mut Canvas, b: &Box, rect: Rect, fill: Color, stroke: Stroke) {
    let (x0, y0, x1, y1) = (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h);
    match b.shape {
        Shape::Rect => canvas.rect(
            Layer::Frames,
            (x0, y0, rect.w, rect.h),
            if b.round { 8.0 } else { 0.0 },
            Some(fill),
            Some(stroke),
        ),
        Shape::Storage => paint_outline(canvas, Outline::Database, rect, 0.0, Some(fill), stroke),
        Shape::Ellipse => paint_outline(canvas, Outline::UseCase, rect, 0.0, Some(fill), stroke),
        Shape::Document => {
            // A wave along the bottom.
            let wave = rect.h.min(16.0) * 0.35;
            let d = format!(
                "M{} {}H{}V{}C{} {} {} {} {} {}C{} {} {} {} {} {}Z",
                n(x0),
                n(y0),
                n(x1),
                n(y1 - wave),
                n(x1 - rect.w * 0.25),
                n(y1 - wave * 2.0),
                n(x1 - rect.w * 0.25),
                n(y1 + wave * 0.5),
                n((x0 + x1) / 2.0),
                n(y1 - wave * 0.5),
                n(x0 + rect.w * 0.25),
                n(y1 - wave * 1.5),
                n(x0 + rect.w * 0.25),
                n(y1 + wave * 0.5),
                n(x0),
                n(y1 - wave),
            );
            canvas.path(
                Layer::Frames,
                &d,
                (x0, y0, x1, y1),
                Some(fill),
                Some(stroke),
            );
        }
        Shape::Io => {
            let slant = rect.w.min(rect.h) * 0.3;
            canvas.polygon(
                Layer::Frames,
                &[(x0 + slant, y0), (x1, y0), (x1 - slant, y1), (x0, y1)],
                Some(fill),
                Some(stroke),
            );
        }
        Shape::Choice => {
            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            canvas.polygon(
                Layer::Frames,
                &[(cx, y0), (x1, cy), (cx, y1), (x0, cy)],
                Some(fill),
                Some(stroke),
            );
        }
        Shape::Trapezoid => {
            let slant = rect.w.min(rect.h) * 0.3;
            canvas.polygon(
                Layer::Frames,
                &[(x0, y0), (x1, y0), (x1 - slant, y1), (x0 + slant, y1)],
                Some(fill),
                Some(stroke),
            );
        }
    }
}

/// `ink`, or black or white where it does not read on `ground`.
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

/// How a ditaa drawing looks: black on white, or the page and the ink a
/// pack gives.
struct Look {
    font_name: String,
    sketch: bool,
    page: Paint,
    ink: Color,
    font: Font,
}

impl Look {
    fn read(params: &[Skinparam]) -> Result<Look, Declined> {
        let mut page: Paint = None;
        let mut ink = Color::BLACK;
        let mut size = 13.0;
        let mut font_name = "sans-serif".to_owned();
        let mut sketch = false;
        for p in params {
            match normalize_key(&p.key).as_str() {
                "backgroundcolor" => page = color::parse(&p.value).map_err(|e| p.fail(e))?,
                "defaultfontcolor" => ink = p.solid()?,
                "defaultfontsize" => size = p.number(4.0, 40.0)?,
                "defaultfontname" => font_name = p.value.trim().to_owned(),
                "handwritten" => sketch = p.value.trim().eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
        Ok(Look {
            font_name,
            sketch,
            page,
            ink,
            font: Font::new(size, ink),
        })
    }
}
