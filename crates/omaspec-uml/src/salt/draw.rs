//! Laying out and painting Salt: each widget measured, then painted in
//! the room its grid gives it, in thin lines in the ink on the page.

use super::{Diagram, Lines, Row, Rule, Widget};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::{self, Color, Paint};
use crate::common::{Skinparam, normalize_key};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const MARGIN: f32 = 10.0;
/// Between two columns of a grid, and between two rows.
const GAP: (f32, f32) = (8.0, 3.0);
/// Inside a grid's lines.
const PAD: f32 = 4.0;
/// A check box's side, a radio button's width.
const MARK: f32 = 11.0;
/// How far a tree's level steps in.
const INDENT: f32 = 14.0;

pub fn draw(d: &Diagram) -> Result<String, Declined> {
    let look = Look::read(&d.frame.skinparams)?;
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let annotations = Annotations::new(
        &d.frame,
        [
            look.font.bold(),
            Font::new(10.0, look.muted),
            Font::new(10.0, look.muted),
            look.font,
        ],
        family,
    )?;
    let painter = Painter {
        look: &look,
        family,
    };
    let size = painter.size(&d.root)?;
    painter.paint(&mut canvas, &d.root, (0.0, annotations.top()), size)?;
    annotations.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, look.page))
}

struct Painter<'a> {
    look: &'a Look,
    family: Family,
}

impl Painter<'_> {
    fn words(&self, text: &str, font: Font) -> Result<Block, Declined> {
        text::block(text, font, self.family).map_err(|e| Declined::at(0, e))
    }

    /// What a text field holds, as written: `****` is a password, not
    /// Creole.
    fn plain(&self, text: &str, font: Font) -> Block {
        text::styled(
            vec![vec![(text.to_owned(), text::Style::from(font))]],
            font,
            self.family,
        )
    }

    fn line(&self) -> Stroke {
        Stroke::new(self.look.ink, 1.0)
    }

    /// The room `widget` takes.
    fn size(&self, widget: &Widget) -> Result<(f32, f32), Declined> {
        let font = self.look.font;
        Ok(match widget {
            Widget::Empty => (0.0, font.size),
            Widget::Text(text) => {
                let b = self.words(text, font)?;
                (b.width, b.height)
            }
            Widget::Button(text) => {
                let b = self.words(text, font)?;
                ((b.width + 20.0).max(44.0), b.height + 6.0)
            }
            Widget::Field(text) => {
                let b = self.plain(text, font);
                (
                    (b.width + 12.0).max(40.0),
                    b.height.max(font.size * 1.164) + 4.0,
                )
            }
            Widget::Check { label, .. } | Widget::Radio { label, .. } => {
                let b = self.words(label, font)?;
                let gap = if label.is_empty() { 0.0 } else { 6.0 };
                (MARK + gap + b.width, b.height.max(MARK + 2.0))
            }
            Widget::Drop(text) => {
                let b = self.words(text, font)?;
                (b.width + 12.0 + 16.0, b.height + 4.0)
            }
            Widget::Group { title, body } => {
                let (w, h) = self.size(body)?;
                let t = self.words(title, font)?;
                (
                    (w + 2.0 * PAD + 4.0).max(t.width + 20.0),
                    h + 2.0 * PAD + t.height,
                )
            }
            Widget::Tabs(tabs) => {
                let mut w = 0.0;
                let mut h: f32 = 0.0;
                for tab in tabs {
                    let b = self.words(tab, font)?;
                    w += b.width + 12.0;
                    h = h.max(b.height + 4.0);
                }
                (w, h + 2.0)
            }
            Widget::Menu(items) => {
                let mut w = 6.0;
                let mut h: f32 = 0.0;
                for item in items {
                    let b = self.words(item, font)?;
                    w += b.width + 12.0;
                    h = h.max(b.height + 2.0);
                }
                (w, h)
            }
            Widget::Tree(items) => {
                let mut w: f32 = 0.0;
                let mut h = 0.0;
                for (depth, text) in items {
                    let b = self.words(text, font)?;
                    w = w.max(INDENT * *depth as f32 + 2.0 + b.width);
                    h += b.height;
                }
                (w, h)
            }
            Widget::Grid { lines, rows } => {
                let (widths, heights) = self.grid(*lines, rows)?;
                let inner = framed(*lines);
                (
                    widths.iter().sum::<f32>()
                        + GAP.0 * widths.len().saturating_sub(1) as f32
                        + 2.0 * inner,
                    heights.iter().sum::<f32>()
                        + GAP.1 * heights.len().saturating_sub(1) as f32
                        + 2.0 * inner,
                )
            }
        })
    }

    /// The widths of a grid's columns and the heights of its rows.
    fn grid(&self, lines: Lines, rows: &[Row]) -> Result<(Vec<f32>, Vec<f32>), Declined> {
        let columns = rows
            .iter()
            .map(|row| match row {
                Row::Cells(cells) => cells.len(),
                Row::Rule(_) => 0,
            })
            .max()
            .unwrap_or(0)
            .max(1);
        let pad = if matches!(lines, Lines::All | Lines::Columns) {
            2.0 * PAD
        } else {
            0.0
        };
        let mut widths = vec![0.0f32; columns];
        let mut heights = Vec::with_capacity(rows.len());
        for row in rows {
            match row {
                Row::Rule(_) => heights.push(7.0),
                Row::Cells(cells) => {
                    let mut tall: f32 = 0.0;
                    for (c, cell) in cells.iter().enumerate() {
                        let (w, h) = self.size(cell)?;
                        widths[c] = widths[c].max(w + pad);
                        tall = tall.max(h);
                    }
                    let pad = if matches!(lines, Lines::All | Lines::Rows) {
                        2.0
                    } else {
                        0.0
                    };
                    heights.push(tall + pad);
                }
            }
        }
        Ok((widths, heights))
    }

    /// Paints `widget` with its top left at `(x, y)` in a room `(w, h)`.
    fn paint(
        &self,
        canvas: &mut Canvas,
        widget: &Widget,
        (x, y): (f32, f32),
        (w, h): (f32, f32),
    ) -> Result<(), Declined> {
        let look = self.look;
        let font = look.font;
        let line = self.line();
        match widget {
            Widget::Empty => {}
            Widget::Text(text) => {
                let b = self.words(text, font)?;
                canvas.text(Layer::Front, &b, (x, y + (h - b.height) / 2.0), Align::Left);
            }
            Widget::Button(text) => {
                let b = self.words(text, font)?;
                let (bw, bh) = ((b.width + 20.0).max(44.0), b.height + 6.0);
                let top = y + (h - bh) / 2.0;
                canvas.rect(
                    Layer::Frames,
                    (x, top, bw, bh),
                    6.0,
                    look.button,
                    Some(Stroke::new(look.ink, 1.6)),
                );
                canvas.text(Layer::Front, &b, (x + bw / 2.0, top + 3.0), Align::Center);
            }
            Widget::Field(text) => {
                let b = self.plain(text, font);
                let bottom = y + (h + b.height) / 2.0 + 1.0;
                canvas.text(
                    Layer::Front,
                    &b,
                    (x + 6.0, bottom - b.height - 1.0),
                    Align::Left,
                );
                canvas.line(Layer::Frames, (x, bottom), (x + w, bottom), line);
                canvas.line(Layer::Frames, (x, bottom - 4.0), (x, bottom), line);
            }
            Widget::Check { on, label } | Widget::Radio { on, label } => {
                let middle = y + h / 2.0;
                let radio = matches!(widget, Widget::Radio { .. });
                if radio {
                    canvas.ellipse(
                        Layer::Frames,
                        (x + MARK / 2.0, middle),
                        (MARK / 2.0, MARK / 2.0),
                        look.button,
                        Some(line),
                    );
                    if *on {
                        canvas.ellipse(
                            Layer::Frames,
                            (x + MARK / 2.0, middle),
                            (2.5, 2.5),
                            Some(look.ink),
                            None,
                        );
                    }
                } else {
                    let top = middle - MARK / 2.0;
                    canvas.rect(
                        Layer::Frames,
                        (x, top, MARK, MARK),
                        0.0,
                        look.button,
                        Some(line),
                    );
                    if *on {
                        canvas.polyline(
                            Layer::Frames,
                            &[
                                (x + 2.0, top + 5.5),
                                (x + 4.5, top + 8.5),
                                (x + 9.0, top + 2.5),
                            ],
                            Stroke::new(look.ink, 1.8),
                        );
                    }
                }
                if !label.is_empty() {
                    let b = self.words(label, font)?;
                    canvas.text(
                        Layer::Front,
                        &b,
                        (x + MARK + 6.0, middle - b.height / 2.0),
                        Align::Left,
                    );
                }
            }
            Widget::Drop(text) => {
                let b = self.words(text, font)?;
                let (bw, bh) = (b.width + 12.0 + 16.0, b.height + 4.0);
                let top = y + (h - bh) / 2.0;
                canvas.rect(
                    Layer::Frames,
                    (x, top, bw, bh),
                    0.0,
                    look.button,
                    Some(line),
                );
                let split = x + bw - 16.0;
                canvas.line(Layer::Frames, (split, top), (split, top + bh), line);
                let (cx, cy) = (split + 8.0, top + bh / 2.0);
                canvas.polygon(
                    Layer::Frames,
                    &[(cx - 4.0, cy - 2.5), (cx + 4.0, cy - 2.5), (cx, cy + 3.0)],
                    Some(look.ink),
                    None,
                );
                canvas.text(Layer::Front, &b, (x + 6.0, top + 2.0), Align::Left);
            }
            Widget::Group { title, body } => {
                let t = self.words(title, font)?;
                let top = y + t.height / 2.0;
                canvas.rect(
                    Layer::Frames,
                    (x, top, w, h - t.height / 2.0),
                    0.0,
                    None,
                    Some(line),
                );
                // The title cuts its line.
                if !title.is_empty() {
                    canvas.rect(
                        Layer::Frames,
                        (x + 6.0, y, t.width + 6.0, t.height),
                        0.0,
                        Some(look.page.unwrap_or(Color::WHITE)),
                        None,
                    );
                    canvas.text(Layer::Front, &t, (x + 9.0, y), Align::Left);
                }
                let inner = self.size(body)?;
                self.paint(canvas, body, (x + PAD + 2.0, y + t.height + PAD), inner)?;
            }
            Widget::Tabs(tabs) => {
                let mut at = x;
                let mut bottom = y;
                for (i, tab) in tabs.iter().enumerate() {
                    let b = self.words(tab, font)?;
                    let (tw, th) = (b.width + 12.0, b.height + 4.0);
                    let stroke = if i == 0 {
                        Stroke::new(look.ink, 1.6)
                    } else {
                        line
                    };
                    canvas.rect(Layer::Frames, (at, y, tw, th), 0.0, None, Some(stroke));
                    canvas.text(Layer::Front, &b, (at + 6.0, y + 2.0), Align::Left);
                    at += tw;
                    bottom = y + th;
                }
                canvas.line(
                    Layer::Frames,
                    (x, bottom + 1.5),
                    (x + w, bottom + 1.5),
                    line,
                );
            }
            Widget::Menu(items) => {
                canvas.rect(Layer::Frames, (x, y, w, h), 0.0, Some(look.bar), Some(line));
                let mut at = x + 6.0;
                for item in items {
                    let b = self.words(item, font)?;
                    canvas.text(Layer::Front, &b, (at, y + 1.0), Align::Left);
                    at += b.width + 12.0;
                }
            }
            Widget::Tree(items) => {
                let mut top = y;
                // Where the last item of each level stood, for the lines.
                let mut last: Vec<f32> = Vec::new();
                for (depth, text) in items {
                    let b = self.words(text, font)?;
                    let middle = top + b.height / 2.0;
                    let at = x + INDENT * *depth as f32;
                    // Under the start of the parent's words.
                    let joint = at - INDENT + 4.0;
                    if *depth > 1 {
                        let from = last.get(depth - 2).copied().unwrap_or(top);
                        canvas.line(Layer::Frames, (joint, from), (joint, middle), line);
                        canvas.line(Layer::Frames, (joint, middle), (at - 2.0, middle), line);
                    } else {
                        canvas.ellipse(
                            Layer::Frames,
                            (at - 6.0, middle),
                            (2.0, 2.0),
                            None,
                            Some(line),
                        );
                    }
                    last.truncate(*depth - 1);
                    last.push(top + b.height - 1.0);
                    canvas.text(Layer::Front, &b, (at, top), Align::Left);
                    top += b.height;
                }
            }
            Widget::Grid { lines, rows } => {
                let (widths, heights) = self.grid(*lines, rows)?;
                let inner = framed(*lines);
                if matches!(lines, Lines::Frame | Lines::All) {
                    canvas.rect(Layer::Frames, (x, y, w, h), 0.0, None, Some(line));
                }
                let pad_x = if matches!(lines, Lines::All | Lines::Columns) {
                    PAD
                } else {
                    0.0
                };
                let mut top = y + inner;
                for (r, row) in rows.iter().enumerate() {
                    let rh = heights[r];
                    match row {
                        Row::Rule(rule) => {
                            let (x0, x1, mid) = (x + inner, x + w - inner, top + rh / 2.0);
                            match rule {
                                Rule::Line => {
                                    canvas.line(Layer::Frames, (x0, mid), (x1, mid), line)
                                }
                                Rule::Double => {
                                    canvas.line(
                                        Layer::Frames,
                                        (x0, mid - 1.5),
                                        (x1, mid - 1.5),
                                        line,
                                    );
                                    canvas.line(
                                        Layer::Frames,
                                        (x0, mid + 1.5),
                                        (x1, mid + 1.5),
                                        line,
                                    );
                                }
                                Rule::Dotted => canvas.line(
                                    Layer::Frames,
                                    (x0, mid),
                                    (x1, mid),
                                    line.dashed(1.0, 2.0),
                                ),
                                Rule::Dashed => canvas.line(
                                    Layer::Frames,
                                    (x0, mid),
                                    (x1, mid),
                                    line.dashed(5.0, 3.0),
                                ),
                            }
                        }
                        Row::Cells(cells) => {
                            let mut left = x + inner;
                            for (c, cell) in cells.iter().enumerate() {
                                let cw = widths[c];
                                self.paint(
                                    canvas,
                                    cell,
                                    (left + pad_x, top),
                                    (cw - 2.0 * pad_x, rh),
                                )?;
                                left += cw + GAP.0;
                            }
                        }
                    }
                    top += rh;
                    // The line under a row, but for the last.
                    if r + 1 < rows.len() {
                        if matches!(lines, Lines::All | Lines::Rows) {
                            let at = top + GAP.1 / 2.0;
                            canvas.line(Layer::Frames, (x, at), (x + w, at), line);
                        }
                        top += GAP.1;
                    }
                }
                if matches!(lines, Lines::All | Lines::Columns) {
                    let mut at = x + inner;
                    for cw in &widths[..widths.len().saturating_sub(1)] {
                        at += cw + GAP.0 / 2.0;
                        canvas.line(Layer::Frames, (at, y), (at, y + h), line);
                        at += GAP.0 / 2.0;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Room inside a grid's frame.
fn framed(lines: Lines) -> f32 {
    if matches!(lines, Lines::Frame | Lines::All) {
        PAD
    } else {
        0.0
    }
}

/// How Salt looks: black on the page, as PlantUML draws it, or the colors
/// of a pack.
struct Look {
    font_name: String,
    sketch: bool,
    page: Paint,
    ink: Color,
    muted: Color,
    font: Font,
    /// What a button, a field's box and a check box are filled with.
    button: Paint,
    /// A menu bar's fill.
    bar: Color,
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
                "defaultfontsize" => size = p.number(4.0, 100.0)?,
                "defaultfontname" => font_name = p.value.trim().to_owned(),
                "handwritten" => sketch = p.value.trim().eq_ignore_ascii_case("true"),
                _ => {}
            }
        }
        let ground = page.unwrap_or(Color::WHITE);
        Ok(Look {
            font_name,
            sketch,
            page,
            ink,
            muted: ink.mix(ground, 0.45),
            font: Font::new(size, ink),
            button: Some(ground),
            bar: ground.mix(ink, 0.14),
        })
    }
}
