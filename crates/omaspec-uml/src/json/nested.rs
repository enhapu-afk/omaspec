//! A JSON value drawn nested (`skinparam jsonLayout nested`): an object is
//! a table of keys and values, an array a column, numbered where it holds
//! an object or an array. A nested object or array sits in the value
//! cell, so the key stays next to what it holds; a folded one says what
//! it holds there instead.

use super::cells::{Fonts, Scalar, path_name, words};
use super::{Kind, Look, Value};
use crate::map::Place;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{Block, Family};

const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 5.0;
/// Space between a cell's rule and a nested card.
const INSET: f32 = 6.0;
const RX: f32 = 5.0;
const STROKE: f32 = 1.5;
const RULE: f32 = 1.0;

enum Node {
    Scalar(Scalar),
    Card(Card),
    /// A folded object or array, by its path.
    Folded(Scalar, Vec<String>),
}

struct Card {
    path: Vec<String>,
    rows: Vec<Row>,
    key_w: f32,
    width: f32,
    height: f32,
    /// `{}` or `[]` when the card has no rows.
    label: Option<Block>,
}

struct Row {
    key: Option<Block>,
    value: Node,
    highlight: bool,
    height: f32,
}

/// Draws `value` with its top left at `(0, top)`; the places of its
/// nested cards and folded values, by their paths, for the pane to fold.
pub(super) fn paint(
    canvas: &mut Canvas,
    look: &Look,
    value: &Value,
    fonts: (&Fonts, &Fonts),
    top: f32,
    family: Family,
) -> Vec<Place> {
    let node = lay(value, Vec::new(), fonts, family);
    let mut places = Vec::new();
    match &node {
        Node::Card(card) => paint_card(canvas, look, card, (0.0, top), fonts, &mut places),
        Node::Scalar(scalar) | Node::Folded(scalar, _) => {
            scalar.paint(canvas, (0.0, top), fonts.0.value.color);
        }
    }
    places
}

fn lay(value: &Value, path: Vec<String>, fonts: (&Fonts, &Fonts), family: Family) -> Node {
    let font = if value.highlight { fonts.1 } else { fonts.0 };
    if value.composite() && value.folded && !path.is_empty() {
        return Node::Folded(Scalar::of(value, font, family), path);
    }
    match &value.kind {
        Kind::Object(rows) if rows.is_empty() => Node::Card(empty_card("{}", path, font, family)),
        Kind::Array(items) if items.is_empty() => Node::Card(empty_card("[]", path, font, family)),
        Kind::Object(rows) => {
            let laid = rows
                .iter()
                .map(|(key, child)| {
                    let key_font = if child.highlight { fonts.1 } else { fonts.0 }.key;
                    let mut at = path.clone();
                    at.push(key.clone());
                    let key = words(key, key_font, family);
                    row(Some(key), lay(child, at, fonts, family), child.highlight)
                })
                .collect();
            Node::Card(card(path, laid))
        }
        Kind::Array(items) => {
            let indexed = items.iter().any(Value::composite);
            let laid = items
                .iter()
                .enumerate()
                .map(|(index, child)| {
                    let key = indexed.then(|| words(&index.to_string(), font.muted, family));
                    let mut at = path.clone();
                    at.push(index.to_string());
                    row(key, lay(child, at, fonts, family), child.highlight)
                })
                .collect();
            Node::Card(card(path, laid))
        }
        _ => Node::Scalar(Scalar::of(value, font, family)),
    }
}

fn empty_card(label: &str, path: Vec<String>, fonts: &Fonts, family: Family) -> Card {
    let label = words(label, fonts.muted, family);
    Card {
        path,
        width: PAD_X + label.width + PAD_X,
        height: PAD_Y + label.height + PAD_Y,
        label: Some(label),
        rows: Vec::new(),
        key_w: 0.0,
    }
}

fn row(key: Option<Block>, value: Node, highlight: bool) -> Row {
    let key_h = key.as_ref().map(|b| b.height).unwrap_or(0.0);
    let (value_pad, value_h) = match &value {
        Node::Scalar(s) | Node::Folded(s, _) => (PAD_Y, s.height()),
        Node::Card(card) => (INSET, card.height),
    };
    let height = (PAD_Y + key_h + PAD_Y).max(value_pad + value_h + value_pad);
    Row {
        key,
        value,
        highlight,
        height,
    }
}

fn card(path: Vec<String>, rows: Vec<Row>) -> Card {
    let has_keys = rows.iter().any(|row| row.key.is_some());
    let key_text = rows
        .iter()
        .filter_map(|row| row.key.as_ref())
        .map(|block| block.width)
        .fold(0.0, f32::max);
    let key_w = if has_keys {
        PAD_X + key_text + PAD_X
    } else {
        0.0
    };
    let value_w = rows
        .iter()
        .map(|row| match &row.value {
            Node::Scalar(s) | Node::Folded(s, _) => PAD_X + s.width() + PAD_X,
            Node::Card(inner) => INSET + inner.width + INSET,
        })
        .fold(0.0, f32::max);
    let height = rows.iter().map(|row| row.height).sum();
    Card {
        path,
        rows,
        key_w,
        width: key_w + value_w,
        height,
        label: None,
    }
}

fn paint_card(
    canvas: &mut Canvas,
    look: &Look,
    card: &Card,
    (x, y): (f32, f32),
    fonts: (&Fonts, &Fonts),
    places: &mut Vec<Place>,
) {
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
    if let Some(label) = &card.label {
        let tx = x + (card.width - label.width) / 2.0;
        let ty = y + (card.height - label.height) / 2.0;
        canvas.text(Layer::Front, label, (tx, ty), Align::Left);
        canvas.rect(
            Layer::Frames,
            (x, y, card.width, card.height),
            RX,
            None,
            Some(border),
        );
        return;
    }
    let rule = Stroke::new(look.line, RULE);
    let mut row_y = y;
    for (index, row) in card.rows.iter().enumerate() {
        if row.highlight {
            let inset = 2.0;
            canvas.plain(|canvas| {
                canvas.rect(
                    Layer::Background,
                    (
                        x + inset,
                        row_y + inset,
                        card.width - 2.0 * inset,
                        row.height - 2.0 * inset,
                    ),
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
        match &row.value {
            Node::Scalar(scalar) => scalar.paint(canvas, (cell_x + PAD_X, row_y + PAD_Y), ink),
            Node::Folded(scalar, path) => {
                scalar.paint(canvas, (cell_x + PAD_X, row_y + PAD_Y), ink);
                let cell = (cell_x, row_y, card.width - card.key_w, row.height);
                places.push(place_of(path, cell, false));
            }
            Node::Card(inner) => {
                paint_card(
                    canvas,
                    look,
                    inner,
                    (cell_x + INSET, row_y + INSET),
                    fonts,
                    places,
                );
            }
        }
        row_y += row.height;
        if index + 1 != card.rows.len() {
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
