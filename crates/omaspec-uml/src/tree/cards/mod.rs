//! A project's map (`docs/adr/0026-the-map-is-made.md`), made from the
//! project ([`Branch`]). Each box with a key is a card: its diagram's
//! picture on its own paper, shaped like the diagram, and below it the
//! box's words, the diagram's kind and a few facts. A box without a key is
//! a folder: a frame around its cards.
//!
//! The map is laid out by its arrows ([`layout`]): a diagram stands above
//! the one it plays against, opens or includes, on the layout every
//! diagram of boxes and lines has ([`crate::graph::open`]). The diagrams
//! no arrow reaches stand in a block, and what is not joined is packed
//! beside one another, so a hundred diagrams are a block and no row.
//!
//! The colors come from a skin, so the map takes the Omarchy colors with
//! the diagrams it shows.

mod arrows;
mod layout;
mod pack;
mod paint;

use super::Tiles;
use super::skin::{Skin, TreeKind};
use crate::Declined;
use crate::graph::Rect;
use crate::map::{self, Tile};
use crate::text::{self, Family, Font};

/// What a card says about the diagram it shows, beside its picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    /// The diagram's kind and language: "State machine · PlantUML".
    pub kind: String,
    pub icon: Icon,
    /// A few facts, "7 states", or nothing.
    pub facts: String,
    /// The picture's own width and height.
    pub size: (f32, f32),
}

/// The little sign before a card's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    State,
    Scenario,
    Activity,
    Structure,
    Map,
    Other,
}

/// An arrow from one card to another, by the keys of their diagrams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub from: String,
    pub to: String,
    pub kind: RelationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RelationKind {
    /// A scenario is checked against the state machine of a participant.
    Plays,
    /// A state opens the diagram of its name.
    Opens,
    /// A diagram includes another one's file.
    Includes,
}

impl RelationKind {
    fn word(self) -> &'static str {
        match self {
            RelationKind::Plays => "plays",
            RelationKind::Opens => "opens",
            RelationKind::Includes => "includes",
        }
    }
}

/// What stands above the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub title: String,
    /// "System map · 3 diagrams".
    pub summary: String,
}

/// The room for a picture and the cards' sizes.
const AREA: f32 = 240.0 * 150.0;
/// Between a card's edge and its picture.
const PAD: f32 = 12.0;
/// Below the picture: the words, the kind and the facts.
const CAPTION: f32 = 64.0;
/// Between the text of a caption and the card's edge.
const TEXT_X: f32 = 18.0;
/// A caption's words grow the card up to this width, then are shortened.
const WIDEST: f32 = 360.0;
/// The heading's height, and the space around everything.
const HEADING: f32 = 84.0;
const MARGIN: f32 = 40.0;

/// A picture grows to fill its room up to this much.
const MOST_GROWTH: f32 = 1.25;

/// The rounding of a card's picture, which the picture put there keeps.
pub const PICTURE_ROUND: f32 = 10.0;

/// The fonts of a caption and a folder's name, at the skin's sizes.
const TITLE: f32 = 15.0;
const SMALL: f32 = 11.5;
const LABEL: f32 = 13.0;

/// A box of a map, which is made, not written
/// (`docs/adr/0026-the-map-is-made.md`): its words, the diagram it shows
/// by its key, and the boxes in it. A box without a key is a folder: a
/// frame with its words around the boxes in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub words: String,
    pub key: Option<String>,
    pub below: Vec<Branch>,
}

/// A map of cards made from `boxes`, in the look `skinparams` give.
pub fn draw_branches(
    boxes: &[Branch],
    skinparams: &[crate::common::Skinparam],
    tiles: &Tiles,
) -> Result<(String, map::Map), Declined> {
    let skin = Skin::from_params(skinparams, TreeKind::Wbs)?;
    let family = Family::of(&skin.font_name);
    let layout = layout::lay_out(boxes, tiles, family);
    let svg = paint::svg(&layout, &skin, family);
    let tiles = layout
        .cards
        .iter()
        .map(|card| Tile {
            key: card.key.clone(),
            name: card.text.clone(),
            frame: card.rect,
            picture: card.picture,
        })
        .collect();
    Ok((
        svg,
        map::Map {
            tiles,
            ..map::Map::default()
        },
    ))
}

/// A card measured: what it says, its size and the size of its picture.
#[derive(Debug, Clone)]
struct Item {
    text: String,
    key: String,
    card: Option<Card>,
    size: (f32, f32),
    picture: (f32, f32),
}

/// A card at its place.
#[derive(Debug, Clone)]
struct Placed {
    text: String,
    key: String,
    card: Option<Card>,
    rect: Rect,
    /// The room for its picture.
    picture: Rect,
}

/// The map laid out, ready to paint.
struct Layout {
    cards: Vec<Placed>,
    /// The folders: their names and frames, the outer ones first.
    frames: Vec<(String, Rect)>,
    /// The outlines around cards that lead to the same diagrams.
    groups: Vec<Rect>,
    arrows: Vec<arrows::Arrow>,
    /// The width of the drawing and its height.
    size: (f32, f32),
    heading: Option<Heading>,
}

/// The card of the diagram `key` shows, called `text`.
fn measure(text: &str, key: &str, tiles: &Tiles, family: Family) -> Item {
    let card = (tiles.card)(key);
    let own = card
        .as_ref()
        .map(|c| c.size)
        .filter(|(w, h)| *w > 0.0 && *h > 0.0);
    let aspect = own.map_or(1.6, |(w, h)| w / h).clamp(0.4, 3.0);
    let mut pw = (AREA * aspect).sqrt().clamp(160.0, 320.0);
    let mut ph = (AREA / aspect).sqrt().clamp(100.0, 260.0);
    // A small diagram grows a little at most, so its words keep their size.
    if let Some((w, h)) = own {
        let grow = (pw / w).min(ph / h);
        if grow > MOST_GROWTH {
            pw = (w * MOST_GROWTH).max(PAD * 4.0);
            ph = (h * MOST_GROWTH).max(PAD * 4.0);
        }
    }
    let ink = crate::Color::BLACK;
    let words = width(text, Font::new(TITLE, ink).bold(), family) + 2.0 * TEXT_X;
    let meta = card.as_ref().map_or(0.0, |c| {
        width(&c.kind, Font::new(SMALL, ink), family)
            + width(&c.facts, Font::new(SMALL, ink), family)
            + 2.0 * TEXT_X
            + 40.0
    });
    let w = (pw + 2.0 * PAD).max(words.max(meta).min(WIDEST));
    Item {
        text: text.to_owned(),
        key: key.to_owned(),
        card,
        size: (w, PAD + ph + CAPTION),
        picture: (pw, ph),
    }
}

/// How wide `words` are in `font`.
fn width(words: &str, font: Font, family: Family) -> f32 {
    text::block(&words.replace('\n', " "), font, family).map_or(0.0, |b| b.width)
}

/// `words` shortened with an ellipsis to fit `room`.
fn fitted(words: &str, font: Font, family: Family, room: f32) -> String {
    let words = words.replace('\n', " ");
    if width(&words, font, family) <= room {
        return words;
    }
    let chars: Vec<char> = words.chars().collect();
    for keep in (1..chars.len()).rev() {
        let short: String = chars[..keep]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_owned()
            + "…";
        if width(&short, font, family) <= room {
            return short;
        }
    }
    "…".into()
}

#[cfg(test)]
mod tests;
