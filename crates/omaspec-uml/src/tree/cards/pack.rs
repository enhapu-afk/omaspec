//! What a map's layout packs itself, without lines to lay it out by: the
//! cards of a block in rows and columns, and what no arrow joins beside
//! and below one another.

use super::Item;
use super::arrows::Arrow;
use crate::graph::Rect;

/// More cards than this that are alike stand in a block.
pub(super) const FAN: usize = 4;
/// Between the cards of a block, and around them inside its outline.
const BLOCK_GAP: f32 = 24.0;
const GROUP_PAD: f32 = 14.0;
/// Between what is packed beside and below one another.
pub(super) const GAP: f32 = 48.0;
/// How much wider than high a packing aims to be.
const ASPECT: f32 = 1.6;
/// What is placed, from its own top left.
#[derive(Debug, Clone, Default)]
pub(super) struct Scene {
    pub size: (f32, f32),
    /// The cards, by their item.
    pub cards: Vec<(usize, Rect)>,
    /// The frames, by their folder; an outer one before those in it.
    pub frames: Vec<(usize, Rect)>,
    pub groups: Vec<Rect>,
    pub arrows: Vec<Arrow>,
}

fn moved(r: Rect, dx: f32, dy: f32) -> Rect {
    Rect {
        x: r.x + dx,
        y: r.y + dy,
        ..r
    }
}

impl Scene {
    /// Lays `other` into this one, its top left at `(dx, dy)`.
    pub fn put(&mut self, other: &Scene, dx: f32, dy: f32) {
        self.cards
            .extend(other.cards.iter().map(|&(i, r)| (i, moved(r, dx, dy))));
        self.frames
            .extend(other.frames.iter().map(|&(g, r)| (g, moved(r, dx, dy))));
        self.groups
            .extend(other.groups.iter().map(|&r| moved(r, dx, dy)));
        for arrow in &other.arrows {
            let mut arrow = arrow.clone();
            for point in arrow.lines.iter_mut().flat_map(|(line, _)| line) {
                point.0 += dx;
                point.1 += dy;
            }
            arrow.chip = (arrow.chip.0 + dx, arrow.chip.1 + dy);
            self.arrows.push(arrow);
        }
    }
}

/// Cards in rows and columns, each column as wide as its widest card;
/// with an outline around more than one when `outlined`.
pub(super) fn block(items: &[Item], cards: &[usize], outlined: bool) -> Scene {
    let count = cards.len();
    let columns = if count <= FAN {
        count.max(1)
    } else {
        (count as f32 * ASPECT).sqrt().ceil() as usize
    };
    let pad = if outlined && count > 1 {
        GROUP_PAD
    } else {
        0.0
    };
    let mut widths = vec![0.0f32; columns];
    for (at, &card) in cards.iter().enumerate() {
        widths[at % columns] = widths[at % columns].max(items[card].size.0);
    }
    let mut scene = Scene::default();
    let mut y = pad;
    for row in cards.chunks(columns) {
        let mut x = pad;
        let mut height = 0.0f32;
        for (column, &card) in row.iter().enumerate() {
            let (w, h) = items[card].size;
            scene.cards.push((card, Rect { x, y, w, h }));
            x += widths[column] + BLOCK_GAP;
            height = height.max(h);
        }
        scene.size.0 = scene.size.0.max(x - BLOCK_GAP + pad);
        y += height + BLOCK_GAP;
    }
    scene.size.1 = y - BLOCK_GAP + pad;
    if pad > 0.0 {
        scene.groups.push(Rect {
            x: 0.0,
            y: 0.0,
            w: scene.size.0,
            h: scene.size.1,
        });
    }
    scene
}

/// `boxes` beside one another in rows, in their order: of the widths a
/// row may have, the one that brings all of it nearest to [`ASPECT`] as
/// wide as high.
pub(super) fn shelves(boxes: &[Scene]) -> Scene {
    // Where each box goes when no row is wider than `limit`, and how much
    // room all of them take.
    let rows = |limit: f32| -> (Vec<(f32, f32)>, (f32, f32)) {
        let mut at = Vec::with_capacity(boxes.len());
        let (mut x, mut y, mut row, mut width) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for b in boxes {
            if x > 0.0 && x + b.size.0 > limit + 0.5 {
                y += row + GAP;
                (x, row) = (0.0, 0.0);
            }
            at.push((x, y));
            x += b.size.0 + GAP;
            row = row.max(b.size.1);
            width = width.max(x - GAP);
        }
        (at, (width, y + row))
    };
    // A row of the first few boxes, of one more, and so on; and what the
    // room they take together suggests.
    const TRIED: usize = 64;
    let area: f32 = boxes
        .iter()
        .map(|b| (b.size.0 + GAP) * (b.size.1 + GAP))
        .sum();
    let widest = boxes.iter().map(|b| b.size.0).fold(0.0f32, f32::max);
    let mut limits = vec![widest.max((area * ASPECT).sqrt())];
    let mut run = -GAP;
    for b in boxes.iter().take(TRIED) {
        run += b.size.0 + GAP;
        limits.push(run.max(widest));
    }
    let off = |(w, h): (f32, f32)| {
        (w / (h * ASPECT).max(1.0))
            .max(f32::MIN_POSITIVE)
            .ln()
            .abs()
    };
    let best = limits
        .into_iter()
        .map(rows)
        .min_by(|a, b| off(a.1).total_cmp(&off(b.1)));
    let mut scene = Scene::default();
    if let Some((at, size)) = best {
        for (b, (x, y)) in boxes.iter().zip(at) {
            scene.put(b, x, y);
        }
        scene.size = size;
    }
    scene
}
