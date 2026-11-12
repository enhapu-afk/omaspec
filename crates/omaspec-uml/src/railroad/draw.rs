//! Laying out and painting a railroad: each node measured as a width and
//! the room above and below its track, then painted along it.

use super::{Diagram, Node};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::{self, Color, Paint};
use crate::common::{Skinparam, normalize_key};
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{self, Block, Family, Font, Style};

const MARGIN: f32 = 10.0;
/// Between two parts of a sequence.
const GAP: f32 = 12.0;
/// How round a turn of the track is.
const R: f32 = 7.0;
/// Room at each side of a branch for its turns.
const SIDE: f32 = 2.0 * R + 6.0;
/// Between two branches, or a loop and what it repeats.
const VGAP: f32 = 9.0;
/// Inside a box, beside its words and above them.
const PAD: (f32, f32) = (8.0, 4.0);
/// The track before the first part and after the last.
const LEAD: f32 = 14.0;

pub fn draw(d: &Diagram) -> Result<String, Declined> {
    let look = Look::read(&d.frame.skinparams)?;
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let annotations = Annotations::new(
        &d.frame,
        [
            look.font.bold(),
            Font::new(10.0, look.ink),
            Font::new(10.0, look.ink),
            look.font,
        ],
        family,
    )?;
    let rail = Rail {
        look: &look,
        family,
    };
    let mut top = annotations.top();
    for (name, node) in &d.rules {
        let laid = rail.measure(node)?;
        if let Some(name) = name {
            let block = rail.plain(name, look.font.bold());
            canvas.text(Layer::Front, &block, (0.0, top), Align::Left);
            top += block.height + 6.0;
        }
        let y = top + laid.up;
        let line = Stroke::new(look.line, 1.2);
        // The start, an open circle; the end, a dot.
        canvas.ellipse(
            Layer::Frames,
            (4.0, y),
            (4.0, 4.0),
            Some(look.page_or_white()),
            Some(line),
        );
        canvas.line(Layer::Lifelines, (8.0, y), (8.0 + LEAD, y), line);
        let x = 8.0 + LEAD;
        rail.paint(&mut canvas, node, (x, y))?;
        let end = x + laid.w + LEAD;
        canvas.line(Layer::Lifelines, (x + laid.w, y), (end, y), line);
        arrow(&mut canvas, (end - 4.0, y), true, look.line);
        canvas.ellipse(
            Layer::Frames,
            (end + 4.0, y),
            (4.0, 4.0),
            Some(look.line),
            None,
        );
        top = y + laid.down + 22.0;
    }
    annotations.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, look.page))
}

/// What a node takes: its width, the room above its track and below.
#[derive(Debug, Clone, Copy)]
struct Laid {
    w: f32,
    up: f32,
    down: f32,
}

struct Rail<'a> {
    look: &'a Look,
    family: Family,
}

impl Rail<'_> {
    /// Words as written: a grammar's terminal is no Creole.
    fn plain(&self, text: &str, font: Font) -> Block {
        text::styled(
            vec![vec![(text.to_owned(), Style::from(font))]],
            font,
            self.family,
        )
    }

    fn boxed(&self, text: &str) -> (Block, f32, f32) {
        let b = self.plain(text, self.look.font);
        let (w, h) = (b.width + 2.0 * PAD.0, b.height + 2.0 * PAD.1);
        (b, w.max(18.0), h)
    }

    fn measure(&self, node: &Node) -> Result<Laid, Declined> {
        Ok(match node {
            Node::Empty => Laid {
                w: 0.0,
                up: 0.0,
                down: 0.0,
            },
            Node::Terminal(t) | Node::NonTerminal(t) | Node::Special(t) => {
                let (_, w, h) = self.boxed(t);
                Laid {
                    w,
                    up: h / 2.0,
                    down: h / 2.0,
                }
            }
            Node::Class { items, negated } => {
                let (w, h) = self.class_size(items, *negated);
                Laid {
                    w,
                    up: h / 2.0,
                    down: h / 2.0,
                }
            }
            Node::Sequence(items) => {
                let mut laid = Laid {
                    w: 0.0,
                    up: 0.0,
                    down: 0.0,
                };
                for (i, item) in items.iter().enumerate() {
                    let l = self.measure(item)?;
                    laid.w += l.w + if i > 0 { GAP } else { 0.0 };
                    laid.up = laid.up.max(l.up);
                    laid.down = laid.down.max(l.down);
                }
                laid
            }
            Node::Choice(ways) => {
                let laid: Vec<Laid> = ways
                    .iter()
                    .map(|w| self.measure(w))
                    .collect::<Result<_, _>>()?;
                let widest = laid.iter().map(|l| l.w).fold(0.0, f32::max);
                let first = laid.first().copied().unwrap_or(Laid {
                    w: 0.0,
                    up: 0.0,
                    down: 0.0,
                });
                let below: f32 = laid[1..].iter().map(|l| VGAP + l.up + l.down).sum();
                Laid {
                    w: widest + 2.0 * SIDE,
                    up: first.up,
                    down: first.down + below,
                }
            }
            Node::Optional(inner) => {
                let l = self.measure(inner)?;
                Laid {
                    w: l.w + 2.0 * SIDE,
                    up: 2.0,
                    down: VGAP + l.up + l.down,
                }
            }
            Node::Repeat { node, label } => {
                let l = self.measure(node)?;
                let said = label
                    .as_ref()
                    .map_or(0.0, |t| self.plain(t, self.small()).height + 2.0);
                Laid {
                    w: l.w + 2.0 * SIDE,
                    up: l.up + VGAP + said,
                    down: l.down,
                }
            }
        })
    }

    fn small(&self) -> Font {
        Font {
            size: (self.look.font.size - 2.0).max(8.0),
            ..self.look.font
        }
    }

    fn class_size(&self, items: &[String], negated: bool) -> (f32, f32) {
        let mut w: f32 = 0.0;
        let mut h = 2.0 * PAD.1;
        for item in items {
            let b = self.plain(item, self.look.font);
            w = w.max(b.width);
            h += b.height;
        }
        if negated {
            let b = self.plain("not", self.small());
            w = w.max(b.width);
            h += b.height;
        }
        (w + 2.0 * PAD.0, h)
    }

    /// Paints `node` with its track entering at `(x, y)`.
    fn paint(&self, canvas: &mut Canvas, node: &Node, (x, y): (f32, f32)) -> Result<(), Declined> {
        let look = self.look;
        let line = Stroke::new(look.line, 1.2);
        match node {
            Node::Empty => {}
            Node::Terminal(t) | Node::NonTerminal(t) | Node::Special(t) => {
                let (b, w, h) = self.boxed(t);
                let rect = (x, y - h / 2.0, w, h);
                match node {
                    Node::NonTerminal(_) => canvas.rect(
                        Layer::Frames,
                        rect,
                        6.0,
                        Some(look.fill),
                        Some(Stroke::new(look.border, 1.0)),
                    ),
                    Node::Special(_) => canvas.rect(
                        Layer::Frames,
                        rect,
                        0.0,
                        Some(look.page_or_white()),
                        Some(Stroke::new(look.border, 1.0).dashed(3.0, 2.0)),
                    ),
                    _ => canvas.rect(
                        Layer::Frames,
                        rect,
                        0.0,
                        Some(look.page_or_white()),
                        Some(Stroke::new(look.border, 1.0)),
                    ),
                }
                canvas.text(
                    Layer::Front,
                    &b,
                    (x + w / 2.0, y - b.height / 2.0),
                    Align::Center,
                );
            }
            Node::Class { items, negated } => {
                let (w, h) = self.class_size(items, *negated);
                let top = y - h / 2.0;
                canvas.rect(
                    Layer::Frames,
                    (x, top, w, h),
                    0.0,
                    Some(look.page_or_white()),
                    Some(Stroke::new(look.border, 1.0).dashed(3.0, 2.0)),
                );
                let mut at = top + PAD.1;
                if *negated {
                    let b = self.plain("not", self.small());
                    canvas.text(Layer::Front, &b, (x + w / 2.0, at), Align::Center);
                    at += b.height;
                }
                for (i, item) in items.iter().enumerate() {
                    let b = self.plain(item, look.font);
                    if i > 0 || *negated {
                        canvas.line(
                            Layer::Frames,
                            (x + 3.0, at),
                            (x + w - 3.0, at),
                            Stroke::new(look.border, 0.5),
                        );
                    }
                    canvas.text(Layer::Front, &b, (x + w / 2.0, at), Align::Center);
                    at += b.height;
                }
            }
            Node::Sequence(items) => {
                let mut at = x;
                for (i, item) in items.iter().enumerate() {
                    let l = self.measure(item)?;
                    if i > 0 {
                        canvas.line(Layer::Lifelines, (at, y), (at + GAP, y), line);
                        at += GAP;
                    }
                    self.paint(canvas, item, (at, y))?;
                    at += l.w;
                }
            }
            Node::Choice(ways) => {
                let laid = self.measure(node)?;
                let (left, right) = (x, x + laid.w);
                let mut below = y;
                for (i, way) in ways.iter().enumerate() {
                    let l = self.measure(way)?;
                    let track = if i == 0 { y } else { below + VGAP + l.up };
                    let start = left + SIDE;
                    if i == 0 {
                        canvas.line(Layer::Lifelines, (left, y), (start, y), line);
                        canvas.line(Layer::Lifelines, (start + l.w, y), (right, y), line);
                    } else {
                        branch(canvas, (left, y), (start, track), line);
                        branch_back(canvas, (start + l.w, track), (right, y), line);
                    }
                    self.paint(canvas, way, (start, track))?;
                    below = if i == 0 { y + l.down } else { track + l.down };
                }
            }
            Node::Optional(inner) => {
                let l = self.measure(inner)?;
                let (left, right) = (x, x + l.w + 2.0 * SIDE);
                canvas.line(Layer::Lifelines, (left, y), (right, y), line);
                arrow(canvas, ((left + right) / 2.0, y), true, look.line);
                let track = y + VGAP + l.up;
                let start = left + SIDE;
                branch(canvas, (left, y), (start, track), line);
                branch_back(canvas, (start + l.w, track), (right, y), line);
                self.paint(canvas, inner, (start, track))?;
            }
            Node::Repeat { node: inner, label } => {
                let l = self.measure(inner)?;
                let (left, right) = (x, x + l.w + 2.0 * SIDE);
                let (start, end) = (left + SIDE, left + SIDE + l.w);
                canvas.line(Layer::Lifelines, (left, y), (start, y), line);
                canvas.line(Layer::Lifelines, (end, y), (right, y), line);
                self.paint(canvas, inner, (start, y))?;
                // The loop back over it, to the left.
                let top = y - l.up - VGAP;
                let (a, b) = (start - R - 2.0, end + R + 2.0);
                let d = format!(
                    "M{} {}Q{} {} {} {}L{} {}Q{} {} {} {}L{} {}Q{} {} {} {}L{} {}Q{} {} {} {}",
                    n(end),
                    n(y),
                    n(b),
                    n(y),
                    n(b),
                    n(y - R),
                    n(b),
                    n(top + R),
                    n(b),
                    n(top),
                    n(b - R),
                    n(top),
                    n(a + R),
                    n(top),
                    n(a),
                    n(top),
                    n(a),
                    n(top + R),
                    n(a),
                    n(y - R),
                    n(a),
                    n(y),
                    n(start),
                    n(y)
                );
                canvas.path(Layer::Lifelines, &d, (a, top, b, y), None, Some(line));
                arrow(canvas, ((a + b) / 2.0, top), false, look.line);
                if let Some(label) = label {
                    let t = self.plain(label, self.small());
                    canvas.text(
                        Layer::Front,
                        &t,
                        ((a + b) / 2.0, top - t.height - 2.0),
                        Align::Center,
                    );
                }
            }
        }
        Ok(())
    }
}

/// A track that turns down from `from` and runs on at `to`.
fn branch(canvas: &mut Canvas, (x0, y0): (f32, f32), (x1, y1): (f32, f32), line: Stroke) {
    let d = format!(
        "M{} {}Q{} {} {} {}L{} {}Q{} {} {} {}L{} {}",
        n(x0),
        n(y0),
        n(x0 + R),
        n(y0),
        n(x0 + R),
        n(y0 + R),
        n(x0 + R),
        n(y1 - R),
        n(x0 + R),
        n(y1),
        n(x0 + 2.0 * R),
        n(y1),
        n(x1),
        n(y1)
    );
    canvas.path(Layer::Lifelines, &d, (x0, y0, x1, y1), None, Some(line));
}

/// A track that runs from `from` and turns up to `to`.
fn branch_back(canvas: &mut Canvas, (x0, y0): (f32, f32), (x1, y1): (f32, f32), line: Stroke) {
    let d = format!(
        "M{} {}L{} {}Q{} {} {} {}L{} {}Q{} {} {} {}",
        n(x0),
        n(y0),
        n(x1 - 2.0 * R),
        n(y0),
        n(x1 - R),
        n(y0),
        n(x1 - R),
        n(y0 - R),
        n(x1 - R),
        n(y1 + R),
        n(x1 - R),
        n(y1),
        n(x1),
        n(y1)
    );
    canvas.path(Layer::Lifelines, &d, (x0, y1, x1, y0), None, Some(line));
}

/// A small head on a track at `(x, y)`, to the right or to the left.
fn arrow(canvas: &mut Canvas, (x, y): (f32, f32), right: bool, color: Color) {
    let s = if right { 1.0 } else { -1.0 };
    canvas.polygon(
        Layer::Front,
        &[
            (x + 3.5 * s, y),
            (x - 3.0 * s, y - 3.0),
            (x - 3.0 * s, y + 3.0),
        ],
        Some(color),
        None,
    );
}

/// How a railroad looks: PlantUML's, or the colors of a pack.
struct Look {
    font_name: String,
    sketch: bool,
    page: Paint,
    ink: Color,
    font: Font,
    line: Color,
    fill: Color,
    border: Color,
}

impl Look {
    fn read(params: &[Skinparam]) -> Result<Look, Declined> {
        let mut page: Paint = None;
        let mut ink = Color::BLACK;
        let mut size = 12.0;
        let mut font_name = "sans-serif".to_owned();
        let mut sketch = false;
        let (mut line, mut fill, mut border) = (None, None, None);
        for p in params {
            match normalize_key(&p.key).as_str() {
                "backgroundcolor" => page = color::parse(&p.value).map_err(|e| p.fail(e))?,
                "defaultfontcolor" => ink = p.solid()?,
                "defaultfontsize" => size = p.number(4.0, 100.0)?,
                "defaultfontname" => font_name = p.value.trim().to_owned(),
                "handwritten" => sketch = p.value.trim().eq_ignore_ascii_case("true"),
                "arrowcolor" => line = Some(p.solid()?),
                "rectanglebackgroundcolor" => {
                    fill = color::parse(&p.value).map_err(|e| p.fail(e))?
                }
                "rectanglebordercolor" => border = Some(p.solid()?),
                _ => {}
            }
        }
        Ok(Look {
            font_name,
            sketch,
            page,
            ink,
            font: Font::new(size, ink),
            line: line.unwrap_or(ink),
            fill: fill.unwrap_or(Color::hex(0xF1F1F1)),
            border: border.unwrap_or(ink),
        })
    }

    fn page_or_white(&self) -> Color {
        self.page.unwrap_or(Color::WHITE)
    }
}
