//! A JSON value as PlantUML draws it: a card for each object and each
//! array, the cards of what they hold in the next column to the right,
//! each reached by a dotted arrow from a dot in its row. An array has no
//! keys; an empty object or array is a small empty card. A folded object
//! or array says what it holds in its row, and its cards are not drawn.
//!
//! Columns stand side by side, each as wide as its widest card. Down a
//! column the cards keep the order of their rows: each as near as it can
//! be to the row that reaches it, below the card above it.

use super::cells::{Fonts, Scalar, path_name, words};
use super::{Kind, Look, Value};
use crate::color::Color;
use crate::map::Place;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{Block, Family};

const PAD_X: f32 = 6.0;
const PAD_Y: f32 = 2.5;
/// The cell of a row that reaches a card: room for its dot.
const DOT_CELL: f32 = 22.0;
/// The dot, from the card's right edge.
const DOT_IN: f32 = 11.0;
const DOT_R: f32 = 3.0;
/// Between two columns, and between two cards of one.
const COLUMN_GAP: f32 = 56.0;
const CARD_GAP: f32 = 12.0;
/// An empty object or array.
const EMPTY_W: f32 = 30.0;
const EMPTY_H: f32 = 16.0;
const RX: f32 = 5.0;
const STROKE: f32 = 1.5;
const RULE: f32 = 1.0;
/// The arrowhead.
const HEAD: f32 = 8.0;

struct Card {
    /// Where it is in the value, from the top; empty for the top.
    path: Vec<String>,
    column: usize,
    rows: Vec<Row>,
    key_w: f32,
    width: f32,
    height: f32,
    x: f32,
    y: f32,
}

struct Row {
    key: Option<Block>,
    cell: Cell,
    highlight: bool,
    height: f32,
}

enum Cell {
    Scalar(Scalar),
    /// The card it reaches, by its place among the cards.
    Child(usize),
    /// A folded object or array, by its path.
    Folded(Scalar, Vec<String>),
}

/// Draws `value` with its top left at `(0, top)`; the places of the
/// cards and of the folded values, by their paths, for the pane to fold.
pub(super) fn paint(
    canvas: &mut Canvas,
    look: &Look,
    value: &Value,
    fonts: (&Fonts, &Fonts),
    top: f32,
    family: Family,
) -> Vec<Place> {
    let mut cards = Vec::new();
    build(value, Vec::new(), 0, fonts, family, &mut cards);
    place(&mut cards, top);
    let mut places = Vec::new();
    for index in 0..cards.len() {
        paint_card(canvas, look, &cards, index, fonts, &mut places);
    }
    places
}

/// The card of `value`, and of everything it holds, after the others in
/// `cards`; its index there.
fn build(
    value: &Value,
    path: Vec<String>,
    column: usize,
    fonts: (&Fonts, &Fonts),
    family: Family,
    cards: &mut Vec<Card>,
) -> usize {
    let index = cards.len();
    cards.push(Card {
        path: path.clone(),
        column,
        rows: Vec::new(),
        key_w: 0.0,
        width: EMPTY_W,
        height: EMPTY_H,
        x: 0.0,
        y: 0.0,
    });
    let entries: Vec<(Option<String>, &Value)> = match &value.kind {
        Kind::Object(rows) => rows.iter().map(|(k, v)| (Some(k.clone()), v)).collect(),
        Kind::Array(items) => items.iter().map(|v| (None, v)).collect(),
        // A scalar at the top: one row, no key.
        _ => vec![(None, value)],
    };
    let mut rows = Vec::with_capacity(entries.len());
    for (at, (key, child)) in entries.into_iter().enumerate() {
        let font = if child.highlight { fonts.1 } else { fonts.0 };
        let key_block = key.as_ref().map(|k| words(k, font.key, family));
        let mut child_path = path.clone();
        child_path.push(key.unwrap_or_else(|| at.to_string()));
        let cell = if !child.composite() || std::ptr::eq(child, value) {
            Cell::Scalar(Scalar::of(child, font, family))
        } else if child.folded {
            Cell::Folded(Scalar::of(child, font, family), child_path)
        } else {
            Cell::Child(build(child, child_path, column + 1, fonts, family, cards))
        };
        let cell_h = match &cell {
            Cell::Scalar(s) | Cell::Folded(s, _) => s.height(),
            Cell::Child(_) => font.value.size * 1.164,
        };
        let key_h = key_block.as_ref().map_or(0.0, |b| b.height);
        rows.push(Row {
            key: key_block,
            cell,
            highlight: child.highlight,
            height: key_h.max(cell_h) + 2.0 * PAD_Y,
        });
    }
    let key_w = rows
        .iter()
        .filter_map(|row| row.key.as_ref())
        .map(|b| b.width + 2.0 * PAD_X)
        .fold(0.0, f32::max);
    let cell_w = rows
        .iter()
        .map(|row| match &row.cell {
            Cell::Scalar(s) | Cell::Folded(s, _) => s.width() + 2.0 * PAD_X,
            Cell::Child(_) => DOT_CELL,
        })
        .fold(0.0, f32::max);
    let card = &mut cards[index];
    if !rows.is_empty() {
        card.key_w = key_w;
        card.width = key_w + cell_w;
        card.height = rows.iter().map(|row| row.height).sum();
    }
    card.rows = rows;
    index
}

/// Each card's place: the columns side by side, the cards of one in
/// order, each centered on the row that reaches it where the card above
/// leaves room.
fn place(cards: &mut [Card], top: f32) {
    let columns = cards.iter().map(|c| c.column + 1).max().unwrap_or(0);
    let mut widths = vec![0.0f32; columns];
    for card in cards.iter() {
        widths[card.column] = widths[card.column].max(card.width);
    }
    let mut x = 0.0;
    let mut lefts = Vec::with_capacity(columns);
    for width in &widths {
        lefts.push(x);
        x += width + COLUMN_GAP;
    }
    let mut floors = vec![f32::NEG_INFINITY; columns];
    // The cards were built top down, a card before what it holds: each
    // parent is placed before its children.
    let mut wanted = vec![top; cards.len()];
    for index in 0..cards.len() {
        let column = cards[index].column;
        let y = wanted[index].max(floors[column] + CARD_GAP).max(top);
        cards[index].x = lefts[column];
        cards[index].y = y;
        floors[column] = y + cards[index].height;
        let mut row_y = y;
        for row in &cards[index].rows {
            if let Cell::Child(child) = row.cell {
                wanted[child] = row_y + row.height / 2.0 - cards[child].height / 2.0;
            }
            row_y += row.height;
        }
    }
}

fn paint_card(
    canvas: &mut Canvas,
    look: &Look,
    cards: &[Card],
    index: usize,
    fonts: (&Fonts, &Fonts),
    places: &mut Vec<Place>,
) {
    let card = &cards[index];
    let (x, y) = (card.x, card.y);
    let border = Stroke::new(look.line, STROKE);
    canvas.rect(
        Layer::Background,
        (x, y, card.width, card.height),
        RX,
        look.fill,
        None,
    );
    if !card.path.is_empty() {
        places.push(place_of(&card.path, (x, y, card.width, card.height), true));
    }
    let rule = Stroke::new(look.line, RULE);
    let mut row_y = y;
    for (at, row) in card.rows.iter().enumerate() {
        if row.highlight {
            canvas.plain(|canvas| {
                canvas.rect(
                    Layer::Background,
                    (x + 2.0, row_y + 2.0, card.width - 4.0, row.height - 4.0),
                    2.0,
                    look.lit_fill,
                    None,
                );
            });
        }
        let ink = if row.highlight { fonts.1 } else { fonts.0 }.value.color;
        if let Some(key) = &row.key {
            canvas.text(Layer::Front, key, (x + PAD_X, row_y + PAD_Y), Align::Left);
        }
        let cell_x = x + card.key_w;
        match &row.cell {
            Cell::Scalar(scalar) => scalar.paint(canvas, (cell_x + PAD_X, row_y + PAD_Y), ink),
            Cell::Folded(scalar, path) => {
                scalar.paint(canvas, (cell_x + PAD_X, row_y + PAD_Y), ink);
                let cell = (cell_x, row_y, card.width - card.key_w, row.height);
                places.push(place_of(path, cell, false));
            }
            Cell::Child(child) => {
                let from = (x + card.width - DOT_IN, row_y + row.height / 2.0);
                arrow(canvas, look.line, from, x + card.width, &cards[*child]);
            }
        }
        row_y += row.height;
        if at + 1 != card.rows.len() {
            canvas.line(Layer::Frames, (x, row_y), (x + card.width, row_y), rule);
        }
    }
    if card.key_w > 0.0 {
        canvas.line(
            Layer::Frames,
            (x + card.key_w, y),
            (x + card.key_w, y + card.height),
            rule,
        );
    }
    canvas.rect(
        Layer::Frames,
        (x, y, card.width, card.height),
        RX,
        None,
        Some(border),
    );
}

/// The dot at `from` and the dotted arrow from it, out of the card whose
/// right edge is at `right`, to the middle of `to`'s left edge.
fn arrow(canvas: &mut Canvas, ink: Color, from: (f32, f32), right: f32, to: &Card) {
    canvas.ellipse(Layer::Front, from, (DOT_R, DOT_R), Some(ink), None);
    let end = (to.x, to.y + to.height / 2.0);
    let out = right + 2.0;
    let tip = (end.0 - HEAD, end.1);
    let bend = (tip.0 - out) / 2.0;
    let d = format!(
        "M{} {} L{} {} C{} {} {} {} {} {}",
        n(from.0),
        n(from.1),
        n(out),
        n(from.1),
        n(out + bend),
        n(from.1),
        n(tip.0 - bend),
        n(tip.1),
        n(tip.0),
        n(tip.1)
    );
    let mut dotted = Stroke::new(ink, 1.0);
    dotted.dash = Some((3.0, 3.0));
    let top = from.1.min(end.1);
    let bottom = from.1.max(end.1);
    canvas.path(
        Layer::Lines,
        &d,
        (from.0, top, end.0, bottom),
        None,
        Some(dotted),
    );
    let head = [
        end,
        (end.0 - HEAD, end.1 - HEAD * 0.45),
        (end.0 - HEAD * 0.7, end.1),
        (end.0 - HEAD, end.1 + HEAD * 0.45),
    ];
    canvas.polygon(Layer::Lines, &head, Some(ink), None);
}

fn place_of(path: &[String], (x, y, w, h): (f32, f32, f32, f32), frame: bool) -> Place {
    Place {
        name: path_name(path),
        rect: crate::graph::Rect { x, y, w, h },
        within: Vec::new(),
        across: false,
        entry: None,
        frame,
        fixed: true,
    }
}

fn n(value: f32) -> String {
    format!("{}", (value * 100.0).round() / 100.0)
}
