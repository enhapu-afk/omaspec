//! A project's map (`docs/adr/0018-diagrams-that-link.md`) in a look of
//! its own. Each box that names a diagram of the project is a card: the
//! diagram's picture on its own paper, shaped like the diagram, and below
//! it the box's words, the diagram's kind and a few facts. The root's card
//! stands out; curves hang the cards below one another; the diagrams on no
//! box hang from the root, dashed; arrows show where one diagram plays
//! against, opens or includes another, around the cards between them. A
//! box that names nothing is a label between the cards.
//!
//! The tree comes from a WBS or a mind map, of either language, and is
//! laid out from the top down whatever way the source grows. The colors
//! come from its skin, so the map takes the Omarchy colors with the
//! diagrams it shows.

mod arrows;
mod paint;

use super::skin::{Skin, TreeKind};
use super::{Element, Idea, MindMap, NodeShape, Tiles, Wbs};
use crate::Declined;
use crate::common::Text;
use crate::graph::Rect;
use crate::graph::route::Point;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    /// "System map · 3 diagrams · 1 on the map".
    pub summary: String,
    /// The project's main map: its root's card says so.
    pub main: bool,
    /// The line that puts the first diagram on no box on the map.
    pub place: Option<String>,
}

/// The room for a picture, the cards' sizes and the space between them.
const AREA: f32 = 240.0 * 150.0;
const ROOT_AREA: f32 = AREA * 1.6;
/// Between a card's edge and its picture.
const PAD: f32 = 12.0;
/// Below the picture: the words, the kind and the facts.
const CAPTION: f32 = 64.0;
/// Between the text of a caption and the card's edge.
const TEXT_X: f32 = 18.0;
/// A caption's words grow the card up to this width, then are shortened.
const WIDEST: f32 = 360.0;
const SIBLING_GAP: f32 = 40.0;
const LEVEL_GAP: f32 = 72.0;
const LOOSE_GAP: f32 = 32.0;
const LOOSE_ROW: usize = 4;
/// The heading's height, and the space around everything.
const HEADING: f32 = 84.0;
const MARGIN: f32 = 40.0;
/// A label's height.
const LABEL_H: f32 = 32.0;
/// How far left of the cards an arrow may run around them; half of it
/// above them.
const ARROW_ROOM: f32 = 24.0;

/// A picture grows to fill its room up to this much.
const MOST_GROWTH: f32 = 1.25;

/// The rounding of a card's picture, which the picture put there keeps.
pub const PICTURE_ROUND: f32 = 10.0;

/// The fonts of a caption and a label, at the skin's sizes.
const TITLE: f32 = 15.0;
const SMALL: f32 = 11.5;
const LABEL: f32 = 13.0;

/// A WBS as a map of cards.
pub fn draw_wbs(d: &Wbs, tiles: &Tiles) -> Result<(String, map::Map), Declined> {
    let skin = Skin::from_params(&d.frame.skinparams, TreeKind::Wbs)?;
    let roots = vec![element(&d.root, tiles)];
    draw(roots, &skin, d.frame.title.as_ref(), tiles)
}

/// A mind map as a map of cards; each root starts a tree of its own.
pub fn draw_mindmap(d: &MindMap, tiles: &Tiles) -> Result<(String, map::Map), Declined> {
    let skin = Skin::from_params(&d.frame.skinparams, TreeKind::MindMap)?;
    let roots = d
        .maps
        .iter()
        .map(|m| {
            let mut ideas: Vec<&Idea> = m.right.iter().chain(&m.left).collect();
            ideas.sort_by_key(|i| i.node.text.line);
            let children = ideas.into_iter().flat_map(|i| idea(i, tiles)).collect();
            item(&m.root.text, children, tiles)
        })
        .collect();
    draw(roots, &skin, d.frame.title.as_ref(), tiles)
}

/// A box of a map that is made, not written, as the map of a project's
/// folder is (`docs/adr/0026-the-map-is-made.md`): its words, the diagram
/// it shows by its key (`None` for a label, as a folder's name over its
/// diagrams), and the boxes below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub words: String,
    pub key: Option<String>,
    pub below: Vec<Branch>,
}

/// A map of cards made from `roots`, in the look `skinparams` give. The
/// keys are the boxes' own: [`Tiles::pick`] is not asked.
pub fn draw_branches(
    roots: &[Branch],
    skinparams: &[crate::common::Skinparam],
    tiles: &Tiles,
) -> Result<(String, map::Map), Declined> {
    fn item(branch: &Branch) -> Item {
        Item {
            text: branch.words.clone(),
            line: None,
            key: branch.key.clone(),
            loose: false,
            children: branch.below.iter().map(item).collect(),
        }
    }
    let skin = Skin::from_params(skinparams, TreeKind::Wbs)?;
    draw(roots.iter().map(item).collect(), &skin, None, tiles)
}

/// A box of the tree: a card when its words name a diagram, else a label.
#[derive(Debug)]
struct Item {
    text: String,
    line: Option<usize>,
    key: Option<String>,
    /// On no box: it hangs from the root, dashed.
    loose: bool,
    children: Vec<Item>,
}

fn item(text: &Text, children: Vec<Item>, tiles: &Tiles) -> Item {
    Item {
        key: (tiles.pick)(&text.value),
        text: text.value.clone(),
        line: Some(text.line),
        loose: false,
        children,
    }
}

/// A WBS box and those below it, in the order they are written.
fn element(e: &Element, tiles: &Tiles) -> Item {
    let mut below: Vec<&Element> = e.left.iter().chain(&e.right).collect();
    below.sort_by_key(|c| c.node.text.line);
    let children = below
        .into_iter()
        .flat_map(|c| {
            let child = element(c, tiles);
            // A box without a box hands its children up.
            if c.node.shape == NodeShape::Pseudo {
                child.children
            } else {
                vec![child]
            }
        })
        .collect();
    item(&e.node.text, children, tiles)
}

fn idea(i: &Idea, tiles: &Tiles) -> Vec<Item> {
    let children = i.children.iter().flat_map(|c| idea(c, tiles)).collect();
    let it = item(&i.node.text, children, tiles);
    if i.node.shape == NodeShape::Pseudo {
        it.children
    } else {
        vec![it]
    }
}

/// A card or a label, measured and placed.
#[derive(Debug, Clone)]
struct Placed {
    text: String,
    line: Option<usize>,
    key: Option<String>,
    loose: bool,
    root: bool,
    card: Option<Card>,
    /// The whole card or label.
    rect: Rect,
    /// A card's room for its picture.
    picture: Option<Rect>,
    /// The card or label it hangs below.
    parent: Option<usize>,
    /// The room the diagrams on no box take beside the root's children;
    /// not painted.
    block: bool,
}

/// The tree laid out, ready to paint.
struct Layout {
    nodes: Vec<Placed>,
    /// The width of the drawing and its height.
    size: (f32, f32),
    heading: Option<Heading>,
    /// Where the label over the diagrams on no box goes.
    loose_label: Option<(f32, f32)>,
    /// The arrows between the cards.
    arrows: Vec<arrows::Arrow>,
}

/// The words over the diagrams on no box.
const LOOSE_WORDS: &str = "not on the map yet";

/// The curve a box hangs by, as the four points of a cubic: from the
/// middle below `from` to the middle above `to`.
fn hang(from: Rect, to: Rect) -> [Point; 4] {
    let (x0, y0) = (from.x + from.w / 2.0, from.y + from.h);
    let (x1, y1) = (to.x + to.w / 2.0, to.y);
    let k = (y1 - y0).abs().max(40.0) * 0.5;
    [(x0, y0), (x0, y0 + k), (x1, y1 - k), (x1, y1)]
}

fn draw(
    mut roots: Vec<Item>,
    skin: &Skin,
    title: Option<&Text>,
    tiles: &Tiles,
) -> Result<(String, map::Map), Declined> {
    let family = Family::of(&skin.font_name);
    let loose: Vec<Item> = tiles
        .loose
        .iter()
        .map(|(name, key)| Item {
            text: name.clone(),
            line: None,
            key: Some(key.clone()),
            loose: true,
            children: Vec::new(),
        })
        .collect();
    let heading = tiles.heading.cloned().map(|mut h| {
        if let Some(title) = title {
            h.title = title.value.clone();
        }
        h
    });
    if roots.is_empty() {
        return Err(Declined::at(0, "a map needs a root"));
    }
    let layout = lay_out(&mut roots, loose, heading, tiles, family);
    let svg = paint::svg(&layout, skin, family);
    let placed = layout
        .nodes
        .iter()
        .filter_map(|n| {
            let key = n.key.clone()?;
            Some(Tile {
                key,
                name: n.text.clone(),
                line: n.line,
                parent: card_above(&layout.nodes, n.parent),
                frame: n.rect,
                picture: n.picture?,
            })
        })
        .collect();
    Ok((
        svg,
        map::Map {
            tiles: placed,
            ..map::Map::default()
        },
    ))
}

/// The key of the nearest card at or above `at`.
fn card_above(nodes: &[Placed], mut at: Option<usize>) -> Option<String> {
    while let Some(i) = at {
        if let Some(key) = &nodes[i].key {
            return Some(key.clone());
        }
        at = nodes[i].parent;
    }
    None
}

/// What a card says, its size, and the size of its picture; a label's
/// size alone.
type Measured = (Option<Card>, (f32, f32), Option<(f32, f32)>);

/// A card's or a label's size, and a card's picture size.
fn measure(it: &Item, root: bool, tiles: &Tiles, family: Family) -> Measured {
    let Some(key) = &it.key else {
        let w = width(&it.text, Font::new(LABEL, crate::Color::BLACK), family) + 32.0;
        return (None, (w, LABEL_H), None);
    };
    let card = (tiles.card)(key);
    let own = card
        .as_ref()
        .map(|c| c.size)
        .filter(|(w, h)| *w > 0.0 && *h > 0.0);
    let aspect = own.map_or(1.6, |(w, h)| w / h).clamp(0.4, 3.0);
    let (area, widest, highest) = if root {
        (ROOT_AREA, 400.0, 320.0)
    } else {
        (AREA, 320.0, 260.0)
    };
    let mut pw = (area * aspect).sqrt().clamp(160.0, widest);
    let mut ph = (area / aspect).sqrt().clamp(100.0, highest);
    // A small diagram grows a little at most, so its words keep their size.
    if let Some((w, h)) = own {
        let grow = (pw / w).min(ph / h);
        if grow > MOST_GROWTH {
            pw = (w * MOST_GROWTH).max(PAD * 4.0);
            ph = (h * MOST_GROWTH).max(PAD * 4.0);
        }
    }
    let ink = crate::Color::BLACK;
    let words = width(&it.text, Font::new(TITLE, ink).bold(), family) + 2.0 * TEXT_X;
    let meta = card.as_ref().map_or(0.0, |c| {
        width(&c.kind, Font::new(SMALL, ink), family)
            + width(&c.facts, Font::new(SMALL, ink), family)
            + 2.0 * TEXT_X
            + 40.0
    });
    let w = (pw + 2.0 * PAD).max(words.max(meta).min(WIDEST));
    (card, (w, PAD + ph + CAPTION), Some((pw, ph)))
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

/// The tree from the top down, each level below the tallest box of the
/// one above; the diagrams on no box in rows below everything.
fn lay_out(
    roots: &mut [Item],
    loose: Vec<Item>,
    heading: Option<Heading>,
    tiles: &Tiles,
    family: Family,
) -> Layout {
    // Flat, with each box's children and depth.
    fn flatten(
        it: &Item,
        parent: Option<usize>,
        level: usize,
        tiles: &Tiles,
        family: Family,
        out: &mut (Vec<Placed>, Vec<Vec<usize>>, Vec<usize>),
    ) -> usize {
        let root = parent.is_none();
        let (card, (w, h), picture) = measure(it, root, tiles, family);
        let at = out.0.len();
        out.0.push(Placed {
            text: it.text.clone(),
            line: it.line,
            key: it.key.clone(),
            loose: it.loose,
            root,
            card,
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            },
            picture: picture.map(|(pw, ph)| Rect {
                x: 0.0,
                y: 0.0,
                w: pw,
                h: ph,
            }),
            parent,
            block: false,
        });
        out.1.push(Vec::new());
        out.2.push(level);
        for c in &it.children {
            let child = flatten(c, Some(at), level + 1, tiles, family, out);
            out.1[at].push(child);
        }
        at
    }
    let mut flat = (Vec::new(), Vec::new(), Vec::new());
    let tops: Vec<usize> = roots
        .iter()
        .map(|r| flatten(r, None, 0, tiles, family, &mut flat))
        .collect();
    let (mut nodes, mut children, mut depth) = flat;

    // The diagrams on no box: a block of rows beside the first root's
    // children, which they hang from, dashed.
    let measured: Vec<(&Item, Measured)> = loose
        .iter()
        .map(|it| (it, measure(it, false, tiles, family)))
        .collect();
    let per_row = LOOSE_ROW.max((measured.len() as f32).sqrt().ceil() as usize);
    let rows: Vec<&[(&Item, Measured)]> = measured.chunks(per_row).collect();
    let row_width = |row: &[(&Item, Measured)]| {
        row.iter().map(|(_, (_, (w, _), _))| w).sum::<f32>()
            + LOOSE_GAP * row.len().saturating_sub(1) as f32
    };
    let row_height = |row: &[(&Item, Measured)]| {
        row.iter()
            .map(|(_, (_, (_, h), _))| *h)
            .fold(0.0f32, f32::max)
    };
    let block = (!rows.is_empty()).then(|| {
        let w = rows.iter().map(|r| row_width(r)).fold(0.0f32, f32::max);
        let h =
            rows.iter().map(|r| row_height(r)).sum::<f32>() + LOOSE_GAP * (rows.len() - 1) as f32;
        let at = nodes.len();
        nodes.push(Placed {
            text: String::new(),
            line: None,
            key: None,
            loose: true,
            root: false,
            card: None,
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            },
            picture: None,
            parent: Some(tops[0]),
            block: true,
        });
        children.push(Vec::new());
        depth.push(1);
        children[tops[0]].push(at);
        at
    });

    // How wide each subtree is.
    let mut span = vec![0.0f32; nodes.len()];
    for at in (0..nodes.len()).rev() {
        let below: f32 = children[at].iter().map(|&c| span[c]).sum::<f32>()
            + SIBLING_GAP * children[at].len().saturating_sub(1) as f32;
        span[at] = nodes[at].rect.w.max(below);
    }
    // Where each box's middle goes.
    fn place(at: usize, middle: f32, span: &[f32], children: &[Vec<usize>], x: &mut [f32]) {
        x[at] = middle;
        let total: f32 = children[at].iter().map(|&c| span[c]).sum::<f32>()
            + SIBLING_GAP * children[at].len().saturating_sub(1) as f32;
        let mut left = middle - total / 2.0;
        for &c in &children[at] {
            place(c, left + span[c] / 2.0, span, children, x);
            left += span[c] + SIBLING_GAP;
        }
    }
    let mut x = vec![0.0f32; nodes.len()];
    let mut left = 0.0;
    for &top in &tops {
        place(top, left + span[top] / 2.0, &span, &children, &mut x);
        left += span[top] + SIBLING_GAP * 2.0;
    }
    for (at, n) in nodes.iter_mut().enumerate() {
        n.rect.x = x[at] - n.rect.w / 2.0;
    }
    // The arrows between cards of one row run on lanes below it.
    let plan = arrows::plan(&nodes, &depth, tiles.relations);
    // Each level below the tallest box of the one above and its lanes,
    // which run below the cards: the room of the diagrams on no box
    // stands beside those, unless a lane leads under it.
    let levels = depth.iter().copied().max().unwrap_or(0) + 1;
    let mut tallest = vec![0.0f32; levels];
    let mut over_lanes = vec![0.0f32; levels];
    for (at, n) in nodes.iter().enumerate() {
        let d = depth[at];
        tallest[d] = tallest[d].max(n.rect.h);
        if !n.block || plan.under(d, n.rect) {
            over_lanes[d] = over_lanes[d].max(n.rect.h);
        }
    }
    let top = MARGIN + if heading.is_some() { HEADING } else { 0.0 };
    let mut level_top = vec![top; levels];
    for d in 1..levels {
        let above = tallest[d - 1].max(over_lanes[d - 1] + plan.room(d - 1));
        level_top[d] = level_top[d - 1] + above + LEVEL_GAP;
    }
    let lane_base: Vec<f32> = (0..levels).map(|d| level_top[d] + over_lanes[d]).collect();
    for (at, n) in nodes.iter_mut().enumerate() {
        // A label sits in the middle of its level.
        n.rect.y = level_top[depth[at]]
            + if n.key.is_none() && !n.block {
                (tallest[depth[at]] - n.rect.h) / 2.0
            } else {
                0.0
            };
    }

    // The diagrams on no box, in their block.
    let mut loose_label = None;
    if let Some(b) = block {
        let area = nodes[b].rect;
        let anchor = tops[0];
        let root = nodes[anchor].rect;
        loose_label = Some((area.x + area.w / 2.0, (root.y + root.h + area.y) / 2.0));
        let mut y = area.y;
        for row in &rows {
            let mut left = area.x + (area.w - row_width(row)) / 2.0;
            for (it, (card, (w, h), picture)) in row.iter() {
                nodes.push(Placed {
                    text: it.text.clone(),
                    line: None,
                    key: it.key.clone(),
                    loose: true,
                    root: false,
                    card: card.clone(),
                    rect: Rect {
                        x: left,
                        y,
                        w: *w,
                        h: *h,
                    },
                    picture: picture.map(|(pw, ph)| Rect {
                        x: 0.0,
                        y: 0.0,
                        w: pw,
                        h: ph,
                    }),
                    parent: Some(anchor),
                    block: false,
                });
                left += w + LOOSE_GAP;
            }
            y += row_height(row) + LOOSE_GAP;
        }
    }

    // The arrows between the cards: they may run left of, right of and
    // below the cards, and the map grows by that.
    let leftmost = nodes.iter().map(|n| n.rect.x).fold(f32::MAX, f32::min);
    let pills: Vec<Rect> = loose_label
        .map(|at| arrows::chip_rect(LOOSE_WORDS, at))
        .into_iter()
        .collect();
    let origin = (leftmost - ARROW_ROOM, top - ARROW_ROOM / 2.0);
    let mut arrows = arrows::lay(&nodes, &plan, &lane_base, origin, &pills);
    let reach = arrows::extent(&arrows);

    // Everything moved right of the margin; the pictures into their cards.
    let leftmost = reach.map_or(leftmost, |r| leftmost.min(r.x));
    let shift = MARGIN - leftmost;
    for arrow in &mut arrows {
        for p in arrow.lines.iter_mut().flatten() {
            p.0 += shift;
        }
        arrow.chip.0 += shift;
    }
    for n in &mut nodes {
        n.rect.x += shift;
        if let Some(p) = &mut n.picture {
            p.x = n.rect.x + (n.rect.w - p.w) / 2.0;
            p.y = n.rect.y + PAD;
        }
    }
    let loose_label = loose_label.map(|(lx, ly)| (lx + shift, ly));
    let right = nodes
        .iter()
        .map(|n| n.rect.x + n.rect.w)
        .fold(reach.map_or(0.0, |r| r.x + r.w + shift), f32::max);
    let bottom = nodes
        .iter()
        .map(|n| n.rect.y + n.rect.h)
        .fold(reach.map_or(0.0, |r| r.y + r.h), f32::max);
    let heading_width = heading.as_ref().map_or(0.0, |h| {
        let ink = crate::Color::BLACK;
        MARGIN
            + width(&h.title, Font::new(22.0, ink).bold(), family).max(width(
                &h.summary,
                Font::new(12.5, ink),
                family,
            ))
    });
    let hint = if loose_label.is_some() && heading.as_ref().is_some_and(|h| h.place.is_some()) {
        48.0
    } else {
        0.0
    };
    Layout {
        nodes,
        size: (
            (right + MARGIN).max(heading_width + MARGIN),
            bottom + hint + MARGIN,
        ),
        heading,
        loose_label,
        arrows,
    }
}

#[cfg(test)]
mod tests;
