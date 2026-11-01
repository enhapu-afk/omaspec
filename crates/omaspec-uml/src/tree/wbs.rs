//! Lays out a WBS diagram the way PlantUML does and draws it.
//!
//! The root's children stand in a row below it, joined by a bar. Below
//! any other box its children hang in a column on a spine down from its
//! middle: those on the left side to the left of the spine, the others
//! to the right, each with a short line from the spine.

use super::skin::{Skin, TreeKind};
use super::{Annotations, Element, Label, MARGIN, NodeShape, Tiles, Wbs, draw_extras};
use crate::Declined;
use crate::map::{Map, Tile};
use crate::svg::{Canvas, Layer};
use crate::text::Family;

/// Between two children in the root's row.
const ROW_GAP: f32 = 20.0;
/// Between a spine and the column beside it.
const SPINE_GAP: f32 = 10.0;
/// From the root's box down to its children's row; the bar is halfway.
const ROW_DROP: f32 = 40.0;

pub fn draw(d: &Wbs) -> Result<String, Declined> {
    draw_tiled(d, None).map(|(svg, _)| svg)
}

/// Draws `d` with the boxes `tiles` picks as tiles, and says where they are.
pub fn draw_tiled(d: &Wbs, tiles: Option<&Tiles>) -> Result<(String, Map), Declined> {
    let skin = Skin::from_params(&d.frame.skinparams, TreeKind::Wbs)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    let family = canvas.family();
    let annotations = Annotations::new(
        &d.frame,
        [
            skin.title_font,
            skin.header_font,
            skin.footer_font,
            skin.caption_font,
        ],
        family,
    )?;
    // The root's own children are the row, not a column.
    let alone = Element {
        node: d.root.node.clone(),
        left: Vec::new(),
        right: Vec::new(),
    };
    let root = Branch::new(&alone, &skin, family, tiles)?;
    let row = d
        .root
        .right
        .iter()
        .map(|e| Branch::new(e, &skin, family, tiles))
        .collect::<Result<Vec<_>, _>>()?;
    let mut drawer = Drawer {
        skin: &skin,
        canvas: &mut canvas,
        placed: Vec::new(),
    };
    drawer.root(&root, &row, annotations.top());
    let mut placed = drawer.placed;
    if let Some(tiles) = tiles {
        draw_extras(&mut canvas, &skin, family, tiles, &mut placed)?;
    }
    annotations.draw(&mut canvas);
    let map = Map {
        tiles: placed,
        ..Map::default()
    }
    .placed(canvas.fit(MARGIN));
    Ok((canvas.finish(MARGIN, skin.background), map))
}

/// A box with its children, measured.
struct Branch {
    label: Label,
    /// The box (or the text without one).
    main: (f32, f32),
    left: Vec<Branch>,
    right: Vec<Branch>,
    /// Space above each child in a column.
    margin: f32,
    width: f32,
    height: f32,
    /// From the branch's left edge to its box's middle: where the spine
    /// runs and the line from above arrives.
    middle: f32,
}

impl Branch {
    fn new(
        e: &Element,
        skin: &Skin,
        family: Family,
        tiles: Option<&Tiles>,
    ) -> Result<Branch, Declined> {
        let label = Label::new(&e.node, skin, family)?.tiled(&e.node, tiles);
        let main = label.size(skin, 0.0, 3.0);
        let measure = |list: &[Element]| {
            list.iter()
                .map(|c| Branch::new(c, skin, family, tiles))
                .collect::<Result<Vec<_>, _>>()
        };
        let (left, right) = (measure(&e.left)?, measure(&e.right)?);
        let margin = if label.shape == NodeShape::Pseudo {
            0.0
        } else {
            skin.margin
        };
        let column_width = |list: &[Branch]| list.iter().map(|b| b.width).fold(0.0, f32::max);
        let column_height = |list: &[Branch]| list.iter().map(|b| margin + b.height).sum::<f32>();
        let (width, height, middle) = if left.is_empty() && right.is_empty() {
            (main.0, main.1, main.0 / 2.0)
        } else {
            let half = main.0 / 2.0;
            let middle = half.max(SPINE_GAP + column_width(&left));
            let width = middle + half.max(SPINE_GAP + column_width(&right));
            let height = main.1 + column_height(&left).max(column_height(&right));
            (width, height, middle)
        };
        Ok(Branch {
            label,
            main,
            left,
            right,
            margin,
            width,
            height,
            middle,
        })
    }

    /// Where the line from the spine meets the box on its left and on its
    /// right side, from the branch's top left corner.
    fn sides(&self) -> (f32, f32, f32) {
        (
            self.middle - self.main.0 / 2.0,
            self.middle + self.main.0 / 2.0,
            self.main.1 / 2.0,
        )
    }
}

struct Drawer<'a> {
    skin: &'a Skin,
    canvas: &'a mut Canvas,
    /// The tiles drawn so far.
    placed: Vec<Tile>,
}

impl Drawer<'_> {
    fn line(&mut self, from: (f32, f32), to: (f32, f32)) {
        self.canvas.line(Layer::Lifelines, from, to, self.skin.link);
    }

    /// Draws the box of `b`, a tile below the tile `parent`.
    fn label(&mut self, b: &Branch, (x, y): (f32, f32), parent: Option<&str>) {
        let drawn = b.label.draw(self.canvas, self.skin, (x, y), 0.0);
        self.placed.extend(b.label.placed(drawn, parent));
    }

    /// The root at `top`, its children in a row below it.
    fn root(&mut self, root: &Branch, row: &[Branch], top: f32) {
        let parent = root.label.tile_key();
        let (main_w, main_h) = root.main;
        let y0 = top + main_h;
        let y1 = y0 + ROW_DROP / 2.0;
        let y2 = y0 + ROW_DROP;
        if row.is_empty() {
            self.label(root, (0.0, top), None);
            self.line((main_w / 2.0, y0), (main_w / 2.0, y1));
            return;
        }
        let first = row[0].middle;
        let mut last = first;
        let mut x = 0.0;
        for b in row {
            last = x + b.middle;
            self.line((last, y1), (last, y2));
            self.branch(b, (x, y2), parent);
            x += b.width + ROW_GAP;
        }
        let at = if last > first {
            self.line((first, y1), (last, y1));
            first + (last - first - main_w) / 2.0
        } else {
            let at = (row[0].width.max(main_w) - main_w) / 2.0;
            self.line((first, y1), (at + main_w / 2.0, y1));
            at
        };
        self.label(root, (at, top), None);
        self.line((at + main_w / 2.0, y0), (at + main_w / 2.0, y1));
    }

    /// `b` with its top left corner at `(x, y)`, below the tile `parent`.
    fn branch(&mut self, b: &Branch, (x, y): (f32, f32), parent: Option<&str>) {
        self.label(b, (x + b.middle - b.main.0 / 2.0, y), parent);
        let parent = b.label.tile_key().or(parent);
        if b.left.is_empty() && b.right.is_empty() {
            return;
        }
        let spine = x + b.middle;
        let below = y + b.main.1;
        let mut bottom = below;
        let mut cy = below;
        for c in &b.left {
            cy += b.margin;
            let (_, right, mid) = c.sides();
            let cx = spine - c.width - SPINE_GAP;
            self.line((cx + right, cy + mid), (spine, cy + mid));
            bottom = bottom.max(cy + mid);
            self.branch(c, (cx, cy), parent);
            cy += c.height;
        }
        let mut cy = below;
        for c in &b.right {
            cy += b.margin;
            let (left, _, mid) = c.sides();
            self.line((spine, cy + mid), (spine + SPINE_GAP + left, cy + mid));
            bottom = bottom.max(cy + mid);
            self.branch(c, (spine + SPINE_GAP, cy), parent);
            cy += c.height;
        }
        self.line((spine, below), (spine, bottom));
    }
}
