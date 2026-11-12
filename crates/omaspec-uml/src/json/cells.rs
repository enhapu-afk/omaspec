//! What a cell of a JSON card shows, in both layouts: a string with its
//! Creole, a number as written, `true` and `false` behind a box ticked or
//! not, `null` as `␀`, and a folded object or array as what it holds.

use super::{Kind, MUTED, Value};
use crate::color::Color;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font, Style};

/// Between the box of a truth value and its word.
const BOX_GAP: f32 = 4.0;

/// The fonts of a card's words.
#[derive(Clone, Copy)]
pub(super) struct Fonts {
    pub(super) key: Font,
    pub(super) value: Font,
    /// Indices and what a folded value holds, in gray.
    pub(super) muted: Font,
}

impl Fonts {
    /// Keys and values in `ink`; the gray darkened or lightened towards
    /// `ink` until it reads on `ground`.
    pub(super) fn new(ink: Color, ground: Color, size: f32, legible: f32) -> Fonts {
        let value = Font::new(size, ink);
        Fonts {
            key: value.bold(),
            value,
            muted: Font::new(size, MUTED.legible_on(ground, ink, legible)),
        }
    }
}

/// A value that is no card: words, or a truth value with its box.
pub(super) enum Scalar {
    Words(Block),
    Truth(bool, Block),
}

impl Scalar {
    /// The cell of `value`, a scalar or a folded object or array.
    pub(super) fn of(value: &Value, fonts: &Fonts, family: Family) -> Scalar {
        match &value.kind {
            Kind::String(text) => Scalar::Words(creole(text, fonts.value, family)),
            Kind::Number(text) => Scalar::Words(words(text, fonts.value, family)),
            Kind::Bool(truth) => {
                let word = if *truth { "true" } else { "false" };
                Scalar::Truth(*truth, words(word, fonts.value, family))
            }
            Kind::Null => Scalar::Words(words("\u{2400}", fonts.value, family)),
            Kind::Object(_) | Kind::Array(_) => Scalar::Words(folded(value, fonts, family)),
        }
    }

    pub(super) fn width(&self) -> f32 {
        match self {
            Scalar::Words(block) => block.width,
            Scalar::Truth(_, block) => side(block) + BOX_GAP + block.width,
        }
    }

    pub(super) fn height(&self) -> f32 {
        match self {
            Scalar::Words(block) | Scalar::Truth(_, block) => block.height,
        }
    }

    /// Drawn with its top left at `at`; the box of a truth value in `ink`.
    pub(super) fn paint(&self, canvas: &mut Canvas, (x, y): (f32, f32), ink: Color) {
        match self {
            Scalar::Words(block) => canvas.text(Layer::Front, block, (x, y), Align::Left),
            Scalar::Truth(ticked, block) => {
                let side = side(block);
                let line = block.lines.first().map_or(block.height, |l| l.height);
                let top = y + (line - side) / 2.0;
                let stroke = Stroke::new(ink, 1.2);
                canvas.rect(Layer::Front, (x, top, side, side), 1.5, None, Some(stroke));
                if *ticked {
                    let at = |u: f32, v: f32| (x + u * side, top + v * side);
                    let tick = [at(0.2, 0.52), at(0.42, 0.74), at(0.8, 0.26)];
                    canvas.polyline(Layer::Front, &tick, Stroke::new(ink, 1.6));
                }
                canvas.text(Layer::Front, block, (x + side + BOX_GAP, y), Align::Left);
            }
        }
    }
}

/// The side of the box of a truth value: as tall as a capital.
fn side(block: &Block) -> f32 {
    let size = block
        .lines
        .first()
        .and_then(|line| line.runs.first())
        .map_or(14.0, |run| run.style.size);
    (size * 0.72).round()
}

/// What a folded object or array holds: `{…} 4 keys`, `[…] 1 item`.
pub(super) fn folded(value: &Value, fonts: &Fonts, family: Family) -> Block {
    let said = match &value.kind {
        Kind::Object(rows) => format!("{{…}} {}", counted(rows.len(), "key")),
        Kind::Array(items) => format!("[…] {}", counted(items.len(), "item")),
        _ => String::new(),
    };
    words(&said, fonts.muted, family)
}

fn counted(n: usize, what: &str) -> String {
    if n == 1 {
        format!("1 {what}")
    } else {
        format!("{n} {what}s")
    }
}

/// A string with its Creole, as PlantUML reads it; one whose markup the
/// engine cannot draw is drawn as written, rather than not at all. A
/// carriage return is dropped, a tab is four spaces, a line break breaks.
pub(super) fn creole(text: &str, font: Font, family: Family) -> Block {
    let plain = text.replace('\r', "").replace('\t', "    ");
    text::block(&plain, font, family).unwrap_or_else(|_| words(&plain, font, family))
}

/// Words as written, a line each.
pub(super) fn words(text: &str, font: Font, family: Family) -> Block {
    let style = Style::from(font);
    let lines = text
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                Vec::new()
            } else {
                vec![(line.to_owned(), style)]
            }
        })
        .collect();
    text::styled(lines, font, family)
}

/// The name a value goes by in a fold, as `#highlight` writes a path:
/// `JSON_Array/4`, a key with a slash, a quote or a space in quotes.
pub fn path_name(path: &[String]) -> String {
    path.iter()
        .map(|part| {
            let plain = !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '+'));
            if plain {
                part.clone()
            } else {
                // The escapes `#highlight` reads: a fold name stays on its line.
                let escaped = part
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\t', "\\t");
                format!("\"{escaped}\"")
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}
