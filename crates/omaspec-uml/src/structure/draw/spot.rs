//! The circle before a class's name, as PlantUML draws one: its letter in
//! a circle left of the words of the header (the stereotypes and the
//! name), the two centered in the box together, the circle in the middle
//! of the header's height.

use super::Row;
use crate::Declined;
use crate::color::{self, Color, Paint};
use crate::graph::Rect;
use crate::structure::Element;
use crate::structure::skin::Skin;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

/// Between the circle and the words.
const GAP: f32 = 4.0;
/// Above and below the circle, inside the header.
const AIR: f32 = 2.0;

/// A circle measured for its box.
#[derive(Debug, Clone)]
pub(super) struct Spot {
    fill: Paint,
    border: Color,
    letter: Block,
    radius: f32,
    /// The line that declares the class, whose keyword the circle shows
    /// (`interface`): the caret there lights it.
    declared: Option<usize>,
}

impl Spot {
    /// What it takes beside the words.
    fn room(&self) -> f32 {
        self.radius * 2.0 + GAP
    }
}

/// The circle of `el`, unless it has none or the skin draws none.
pub(super) fn measure(el: &Element, skin: &Skin, family: Family) -> Result<Option<Spot>, Declined> {
    let Some(spot) = &el.spot else {
        return Ok(None);
    };
    if skin.strict {
        return Ok(None);
    }
    let spots = &skin.spots;
    let fill = match &spot.color {
        Some(value) => color::parse(value).map_err(|err| Declined::at(el.line, err))?,
        None => Some(spots.fill[spot.kind.index()]),
    };
    let font = Font::new(spots.size, spots.letter).bold();
    let letter = text::block(&spot.letter.to_string(), font, family)
        .map_err(|err| Declined::at(el.line, err))?;
    Ok(Some(Spot {
        fill,
        border: spots.border[spot.kind.index()],
        letter,
        radius: spots.radius,
        declared: el.explicit.then_some(el.line),
    }))
}

/// The header (`rows[..body]`) with the circle's room before its words,
/// and high enough to hold the circle: the new start of the members.
pub(super) fn beside(rows: &mut Vec<Row>, body: usize, spot: &Spot) -> usize {
    let room = spot.room();
    for row in &mut rows[..body] {
        *row = match std::mem::replace(row, Row::Space(0.0)) {
            Row::Center(block) => Row::Spotted {
                block,
                left: false,
                room,
            },
            Row::Left(block) => Row::Spotted {
                block,
                left: true,
                room,
            },
            other => other,
        };
    }
    let high: f32 = rows[..body].iter().map(Row::height).sum();
    let need = spot.radius * 2.0 + AIR * 2.0;
    if high >= need {
        return body;
    }
    let pad = (need - high) / 2.0;
    rows.insert(body, Row::Space(pad));
    rows.insert(0, Row::Space(pad));
    body + 2
}

/// The circle of the box called `name` beside its header `rows`, which
/// stand from `top` to `bottom` in `area`, in a group of its own
/// ([`crate::svg::SPOT`]).
pub(super) fn paint(
    canvas: &mut Canvas,
    (spot, name): (&Spot, &str),
    rows: &[Row],
    area: Rect,
    pad_x: f32,
    (top, bottom): (f32, f32),
) {
    let words = rows
        .iter()
        .filter_map(|row| match row {
            Row::Spotted { block, .. } => Some(block.width),
            _ => None,
        })
        .fold(0.0, f32::max);
    let left = rows
        .iter()
        .any(|row| matches!(row, Row::Spotted { left: true, .. }));
    let r = spot.radius;
    let cx = if left {
        area.x + pad_x + r
    } else {
        area.x + (area.w - words - spot.room()) / 2.0 + r
    };
    let cy = (top + bottom) / 2.0;
    let id = format!("{}{}", crate::svg::SPOT, crate::svg::part_id(name));
    canvas.begin_group(Layer::Front, &id);
    canvas.ellipse(
        Layer::Front,
        (cx, cy),
        (r, r),
        spot.fill,
        Some(Stroke::new(spot.border, 1.0)),
    );
    canvas.text(
        Layer::Front,
        &spot.letter,
        (cx, cy - spot.letter.height / 2.0),
        Align::Center,
    );
    canvas.end_group(Layer::Front);
    if let Some(line) = spot.declared {
        canvas.shape_of(line, (cx - r, cy - r, r * 2.0, r * 2.0));
    }
}

/// Where a header row's words start (`Align::Left`) or are centered.
pub(super) fn words_at(area: Rect, pad_x: f32, left: bool, room: f32) -> (f32, Align) {
    if left {
        (area.x + pad_x + room, Align::Left)
    } else {
        (area.x + area.w / 2.0 + room / 2.0, Align::Center)
    }
}
