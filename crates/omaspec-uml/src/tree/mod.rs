//! Mind maps and work breakdown structures: trees of boxes that PlantUML
//! lays out itself. Their syntax is close (lines of `*` give the levels),
//! their layouts are not: a mind map grows to both sides of its root, a
//! WBS hangs down from it.

mod mermaid;
mod mindmap;
mod parse;
mod skin;
mod wbs;

use crate::Declined;
use crate::annotations::Annotations;
use crate::color::{self, Paint};
pub use crate::common::{Frame, Skinparam, Text};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family};
pub use mermaid::{from_mermaid, mermaid_refusal};
pub use mindmap::draw as draw_mindmap;
pub use mindmap::draw_tiled as draw_mindmap_tiled;
pub use parse::{parse_mindmap, parse_wbs};
pub use wbs::draw as draw_wbs;
pub use wbs::draw_tiled as draw_wbs_tiled;

use crate::graph::Rect;
use crate::map::Tile;

/// Boxes of a mind map or a WBS drawn as tiles, each showing a diagram of
/// the project (`docs/adr/0018-diagrams-that-link.md`). The tile has room
/// for the picture and the box's words below it; the caller puts the
/// picture in the room that [`crate::map::Map::tiles`] gives.
pub struct Tiles<'a> {
    /// The key of the diagram a box's words name; `None` leaves the box a
    /// box.
    pub pick: &'a dyn Fn(&str) -> Option<String>,
    /// The room for a picture, in the drawing's units.
    pub picture: (f32, f32),
    /// Diagrams that are on no box, by name and key: tiles in rows below
    /// the map, under `heading`.
    pub loose: &'a [(String, String)],
    pub heading: &'a str,
    /// A dashed arrow from tile to tile, by key, for each diagram that
    /// links to another one the map does not hang below it.
    pub links: &'a [(String, String)],
}

/// Between a tile's picture and its words.
const TILE_GAP: f32 = 6.0;
/// Between the map and the tiles that are on no box, and between those.
const LOOSE_DROP: f32 = 36.0;
const LOOSE_GAP: f32 = 20.0;
/// At least this many loose tiles fit in a row.
const LOOSE_ROW: usize = 4;

/// A box of the tree, with what is written in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The label as written; a block's lines joined with newlines.
    pub text: Text,
    pub shape: NodeShape,
    /// `[#color]`: the box's fill.
    pub color: Option<String>,
    /// Mermaid: the ink on this box. `None` uses the skin's font.
    pub ink: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeShape {
    /// The default: the label in a rounded box.
    Box,
    /// `[text]` in a Mermaid mind map: square corners.
    Square,
    /// `((text))` in a Mermaid mind map.
    Circle,
    /// `{{text}}` in a Mermaid mind map.
    Hexagon,
    /// `)text(` in a Mermaid mind map.
    Cloud,
    /// `))text((` in a Mermaid mind map: a burst around the label.
    Bang,
    /// `_`: the label without a box.
    Plain,
    /// `_` without a label in a WBS: nothing is drawn, only the lines.
    Pseudo,
}

impl NodeShape {
    /// A drawn box, which keeps a margin around it. Plain text and a
    /// pseudo node do not.
    fn spreads(self) -> bool {
        !matches!(self, Self::Plain | Self::Pseudo)
    }
}

/// A parsed `@startmindmap` diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct MindMap {
    /// Every root starts a map of its own, drawn below the one before.
    pub maps: Vec<Map>,
    /// `top to bottom direction`: the maps grow downwards (and upwards)
    /// instead of to the right (and left).
    pub vertical: bool,
    pub frame: Frame,
}

/// One root and the ideas on both of its sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    pub root: Node,
    /// To the right of the root (below it when vertical).
    pub right: Vec<Idea>,
    /// To the left of the root (above it when vertical): `-` lines and
    /// those after `left side`.
    pub left: Vec<Idea>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Idea {
    pub node: Node,
    pub children: Vec<Idea>,
}

/// A parsed `@startwbs` diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Wbs {
    pub root: Element,
    pub frame: Frame,
}

/// A WBS box and the boxes below it. The root's children form a row
/// under it; below any other box its children hang in a column, on its
/// left (`<`, `-`) or on its right side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub node: Node,
    pub left: Vec<Element>,
    pub right: Vec<Element>,
}

/// Space around the drawing.
const MARGIN: f32 = 10.0;

/// A node's label measured, with its fill.
struct Label {
    block: Block,
    shape: NodeShape,
    fill: Paint,
    /// Drawn as a tile.
    tile: Option<TileLabel>,
}

/// What a tile shows.
struct TileLabel {
    key: String,
    name: String,
    line: Option<usize>,
    picture: (f32, f32),
}

/// A tile as drawn: its frame and the room for the picture.
type Drawn = (Rect, Rect);

impl Label {
    fn new(node: &Node, skin: &skin::Skin, family: Family) -> Result<Label, Declined> {
        let line = node.text.line;
        let mut font = skin.font;
        if let Some(value) = &node.ink {
            font.color = match color::parse(value).map_err(|e| Declined::at(line, e))? {
                Some(color) => color,
                None => {
                    return Err(Declined::at(line, "a text color must not be transparent"));
                }
            };
        }
        let block =
            text::block(&node.text.value, font, family).map_err(|e| Declined::at(line, e))?;
        let fill = match &node.color {
            Some(c) => color::parse(c).map_err(|e| Declined::at(line, e))?,
            None => skin.fill,
        };
        Ok(Label {
            block,
            shape: node.shape,
            fill,
            tile: None,
        })
    }

    /// The label as a tile, when `tiles` picks the diagram its words name.
    fn tiled(mut self, node: &Node, tiles: Option<&Tiles>) -> Label {
        self.tile = tiles.and_then(|tiles| {
            let key = (tiles.pick)(&node.text.value)?;
            Some(TileLabel {
                key,
                name: node.text.value.clone(),
                line: Some(node.text.line),
                picture: tiles.picture,
            })
        });
        self
    }

    /// The key of the diagram it shows as a tile.
    fn tile_key(&self) -> Option<&str> {
        self.tile.as_ref().map(|t| t.key.as_str())
    }

    /// The size of the box, or of the text with its margin when there is
    /// no box: `before` and `after` are the text's margins on the left and
    /// the right.
    fn size(&self, skin: &skin::Skin, before: f32, after: f32) -> (f32, f32) {
        let b = &self.block;
        let pad = skin.padding;
        if let Some(tile) = &self.tile {
            let (w, h) = tile.picture;
            return (
                w.max(b.width) + 2.0 * pad,
                h + TILE_GAP + b.height + 2.0 * pad,
            );
        }
        match self.shape {
            NodeShape::Box | NodeShape::Square => (b.width + 2.0 * pad, b.height + 2.0 * pad),
            // Wide enough that the text's rectangle sits inside the circle.
            NodeShape::Circle => {
                let d = b.width.hypot(b.height) + 2.0 * pad;
                (d, d)
            }
            // The points eat into a box the text would fill, so the box is wider.
            NodeShape::Hexagon => (b.width + 3.0 * pad, b.height + 2.0 * pad),
            // The bumps sit in a rim outside the text's padding.
            NodeShape::Cloud => {
                let rim = CLOUD_RIM;
                (
                    b.width + 2.0 * pad + 2.0 * rim,
                    b.height + 2.0 * pad + 2.0 * rim,
                )
            }
            // The points stick out of the circle that holds the text.
            NodeShape::Bang => {
                let d = (b.width.hypot(b.height) + 2.0 * pad) / BANG_INNER;
                (d, d)
            }
            NodeShape::Plain => (b.width + before + after, b.height + 2.0),
            NodeShape::Pseudo => (0.0, 0.0),
        }
    }

    /// Draws the box (or the text alone) with its top left corner at
    /// `(x, y)`; `before` as for [`Label::size`]. A tile says where it is.
    fn draw(
        &self,
        canvas: &mut Canvas,
        skin: &skin::Skin,
        (x, y): (f32, f32),
        before: f32,
    ) -> Option<Drawn> {
        let (w, h) = self.size(skin, before, 0.0);
        let stroke = Stroke::new(skin.border, skin.thickness);
        if let Some(tile) = &self.tile {
            canvas.rect(
                Layer::Front,
                (x, y, w, h),
                skin.round,
                self.fill,
                Some(stroke),
            );
            let (pw, ph) = tile.picture;
            let picture = Rect {
                x: x + (w - pw) / 2.0,
                y: y + skin.padding,
                w: pw,
                h: ph,
            };
            let paper = skin.background.or(Some(color::Color::WHITE));
            canvas.rect(
                Layer::Front,
                (picture.x, picture.y, pw, ph),
                0.0,
                paper,
                Some(Stroke::new(skin.border, 0.5)),
            );
            let top = picture.y + ph + TILE_GAP;
            canvas.text(Layer::Front, &self.block, (x + w / 2.0, top), Align::Center);
            return Some((Rect { x, y, w, h }, picture));
        }
        let inner = match self.shape {
            NodeShape::Box | NodeShape::Square => {
                let round = if self.shape == NodeShape::Square {
                    0.0
                } else {
                    skin.round
                };
                canvas.rect(Layer::Front, (x, y, w, h), round, self.fill, Some(stroke));
                (x + skin.padding, y + skin.padding, w - 2.0 * skin.padding)
            }
            NodeShape::Circle => {
                canvas.ellipse(
                    Layer::Front,
                    (x + w / 2.0, y + h / 2.0),
                    (w / 2.0, h / 2.0),
                    self.fill,
                    Some(stroke),
                );
                let left = x + (w - self.block.width) / 2.0;
                let top = y + (h - self.block.height) / 2.0;
                (left, top, self.block.width)
            }
            NodeShape::Hexagon => {
                let points = [
                    (x + w * 0.25, y),
                    (x + w * 0.75, y),
                    (x + w, y + h / 2.0),
                    (x + w * 0.75, y + h),
                    (x + w * 0.25, y + h),
                    (x, y + h / 2.0),
                ];
                canvas.polygon(Layer::Front, &points, self.fill, Some(stroke));
                let left = x + (w - self.block.width) / 2.0;
                (left, y + skin.padding, self.block.width)
            }
            NodeShape::Cloud => {
                canvas.path(
                    Layer::Front,
                    &cloud_path(x, y, w, h),
                    (x, y, x + w, y + h),
                    self.fill,
                    Some(stroke),
                );
                let left = x + (w - self.block.width) / 2.0;
                let top = y + (h - self.block.height) / 2.0;
                (left, top, self.block.width)
            }
            NodeShape::Bang => {
                canvas.polygon(
                    Layer::Front,
                    &bang_points(x, y, w, h),
                    self.fill,
                    Some(stroke),
                );
                let left = x + (w - self.block.width) / 2.0;
                let top = y + (h - self.block.height) / 2.0;
                (left, top, self.block.width)
            }
            NodeShape::Plain => (x + before, y + 1.0, self.block.width),
            NodeShape::Pseudo => return None,
        };
        let (left, top, width) = inner;
        let anchor = match skin.align {
            Align::Left => left,
            Align::Center => left + width / 2.0,
            Align::Right => left + width,
        };
        canvas.text(Layer::Front, &self.block, (anchor, top), skin.align);
        None
    }

    /// The tile it drew at `drawn`, for the map, below the tile `parent`.
    fn placed(&self, drawn: Option<Drawn>, parent: Option<&str>) -> Option<Tile> {
        let (tile, (frame, picture)) = self.tile.as_ref().zip(drawn)?;
        Some(Tile {
            key: tile.key.clone(),
            name: tile.name.clone(),
            line: tile.line,
            parent: parent.map(str::to_owned),
            frame,
            picture,
        })
    }
}

/// Below the map: the tiles of the diagrams on no box, under a heading,
/// in rows as wide as the map (at least [`LOOSE_ROW`] tiles). Then the
/// dashed arrows between tiles. Both go into `placed`.
fn draw_extras(
    canvas: &mut Canvas,
    skin: &skin::Skin,
    family: Family,
    tiles: &Tiles,
    placed: &mut Vec<Tile>,
) -> Result<(), Declined> {
    if !tiles.loose.is_empty() {
        let (left, right) = canvas.width_span();
        let top = canvas.bottom() + LOOSE_DROP;
        let heading = text::block(tiles.heading, skin.font, family).map_err(|e| Declined {
            line: None,
            reason: e,
        })?;
        canvas.text(Layer::Front, &heading, (left, top), Align::Left);
        let mut y = top + heading.height + skin.margin;
        let labels = tiles
            .loose
            .iter()
            .map(|(name, key)| {
                let block = text::block(name, skin.font, family).map_err(|e| Declined {
                    line: None,
                    reason: e,
                })?;
                Ok(Label {
                    block,
                    shape: NodeShape::Box,
                    fill: skin.fill,
                    tile: Some(TileLabel {
                        key: key.clone(),
                        name: name.clone(),
                        line: None,
                        picture: tiles.picture,
                    }),
                })
            })
            .collect::<Result<Vec<_>, Declined>>()?;
        let widest = labels
            .iter()
            .map(|l| l.size(skin, 0.0, 0.0).0)
            .fold(0.0, f32::max);
        let width = (right - left).max(LOOSE_ROW as f32 * (widest + LOOSE_GAP) - LOOSE_GAP);
        let (mut x, mut row) = (left, 0.0f32);
        for label in &labels {
            let (w, h) = label.size(skin, 0.0, 0.0);
            if x > left && x + w > left + width {
                (x, y) = (left, y + row + LOOSE_GAP);
                row = 0.0;
            }
            let drawn = label.draw(canvas, skin, (x, y), 0.0);
            placed.extend(label.placed(drawn, None));
            x += w + LOOSE_GAP;
            row = row.max(h);
        }
    }
    let stroke = skin.link.dashed(6.0, 4.0);
    for (from, to) in tiles.links {
        let find = |key: &String| placed.iter().find(|t| &t.key == key).map(|t| t.frame);
        if let (Some(a), Some(b)) = (find(from), find(to))
            && from != to
        {
            dashed_arrow(canvas, a, b, stroke);
        }
    }
    Ok(())
}

/// A dashed curve with a head from the side of `a` that faces `b` to the
/// side of `b` that faces `a`, under the boxes.
fn dashed_arrow(canvas: &mut Canvas, a: Rect, b: Rect, stroke: Stroke) {
    let center = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let ((ax, ay), (bx, by)) = (center(a), center(b));
    let (dx, dy) = (bx - ax, by - ay);
    // Out of each box along the axis they lie apart on most, measured
    // against the boxes' size.
    let across = dx.abs() / (a.w + b.w).max(1.0) > dy.abs() / (a.h + b.h).max(1.0);
    let (p1, n1, p2, n2) = if across {
        let s = dx.signum();
        (
            (ax + s * a.w / 2.0, ay),
            (s, 0.0),
            (bx - s * b.w / 2.0, by),
            (-s, 0.0),
        )
    } else {
        let s = dy.signum();
        (
            (ax, ay + s * a.h / 2.0),
            (0.0, s),
            (bx, by - s * b.h / 2.0),
            (0.0, -s),
        )
    };
    let reach = ((p2.0 - p1.0).hypot(p2.1 - p1.1) * 0.4).max(30.0);
    let c1 = (p1.0 + n1.0 * reach, p1.1 + n1.1 * reach);
    let c2 = (p2.0 + n2.0 * reach, p2.1 + n2.1 * reach);
    // The head: its tip on the box, along the curve's last direction.
    let head = 9.0;
    let tail = (p2.0 + n2.0 * head, p2.1 + n2.1 * head);
    let pt = |p: (f32, f32)| format!("{},{}", crate::svg::n(p.0), crate::svg::n(p.1));
    let d = format!("M{} C{} {} {}", pt(p1), pt(c1), pt(c2), pt(tail));
    let xs = [p1.0, c1.0, c2.0, p2.0];
    let ys = [p1.1, c1.1, c2.1, p2.1];
    let bounds = (
        xs.iter().copied().fold(f32::INFINITY, f32::min),
        ys.iter().copied().fold(f32::INFINITY, f32::min),
        xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
    );
    canvas.path(Layer::Lifelines, &d, bounds, None, Some(stroke));
    let side = (-n2.1 * head * 0.45, n2.0 * head * 0.45);
    canvas.polygon(
        Layer::Lifelines,
        &[
            p2,
            (tail.0 + side.0, tail.1 + side.1),
            (tail.0 - side.0, tail.1 - side.1),
        ],
        Some(stroke.color),
        None,
    );
}

/// How far a cloud's bumps sit outside the text's own padding.
const CLOUD_RIM: f32 = 12.0;

/// The bang's inner radius, as a share of the outer one. The label sits
/// inside that circle, and the points stick out to the box.
const BANG_INNER: f32 = 0.68;

/// A cloud whose left and right middles are the box edges, so a link
/// meets the outline. The bumps stay in the outer rim.
fn cloud_path(x: f32, y: f32, w: f32, h: f32) -> String {
    let left = x;
    let right = x + w;
    let top = y;
    let bottom = y + h;
    let mid = y + h / 2.0;
    let rim = CLOUD_RIM.min(h / 2.0).min(w / 4.0);
    let p = |px: f32, py: f32| format!("{},{}", crate::svg::n(px), crate::svg::n(py));
    format!(
        "M{} Q{} {} Q{} {} Q{} {} Q{} {} Q{} {} Q{} {} Q{} {} Z",
        p(left, mid),
        p(left, bottom - rim),
        p(left + w * 0.22, bottom),
        p(x + w * 0.50, bottom - rim * 0.35),
        p(left + w * 0.78, bottom),
        p(right, bottom - rim),
        p(right, mid),
        p(right, top + rim * 0.15),
        p(x + w * 0.72, top),
        p(x + w * 0.50, top + rim * 0.7),
        p(x + w * 0.34, top),
        p(x + w * 0.16, top + rim * 0.2),
        p(left, top + rim),
        p(left, top + h * 0.34),
        p(left, mid),
    )
}

/// Sixteen points, outer on the even steps, so the left and right tips
/// are the box edges where a link arrives.
fn bang_points(x: f32, y: f32, w: f32, h: f32) -> [(f32, f32); 16] {
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    let rx = w / 2.0;
    let ry = h / 2.0;
    let mut points = [(0.0, 0.0); 16];
    for (i, point) in points.iter_mut().enumerate() {
        let angle = i as f32 * std::f32::consts::FRAC_PI_8;
        let scale = if i.is_multiple_of(2) { 1.0 } else { BANG_INNER };
        *point = (cx + rx * scale * angle.cos(), cy + ry * scale * angle.sin());
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> String {
        text.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    }

    fn within(inner: Rect, outer: Rect) -> bool {
        inner.x >= outer.x - 0.01
            && inner.y >= outer.y - 0.01
            && inner.x + inner.w <= outer.x + outer.w + 0.01
            && inner.y + inner.h <= outer.y + outer.h + 0.01
    }

    fn size(svg: &str) -> Rect {
        let attr = |name: &str| -> f32 {
            let at = svg.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
            svg[at..].split('"').next().unwrap().parse().unwrap()
        };
        Rect {
            x: 0.0,
            y: 0.0,
            w: attr("width"),
            h: attr("height"),
        }
    }

    fn known(text: &str) -> Option<String> {
        let k = key(text);
        ["orders", "checkout", "payment", "shop"]
            .contains(&k.as_str())
            .then_some(k)
    }

    #[test]
    fn a_wbs_draws_the_boxes_that_name_diagrams_as_tiles() {
        let source =
            "@startwbs\n* Map\n** Orders\n*** Checkout\n*** Notes\n** Payment\n** Ideas\n@endwbs\n";
        let wbs = crate::render_mapped(source, None).unwrap();
        assert!(wbs.1.tiles.is_empty());
        let loose = [("Refund".to_owned(), "refund".to_owned())];
        let links = [("payment".to_owned(), "checkout".to_owned())];
        let tiles = Tiles {
            pick: &known,
            picture: (160.0, 100.0),
            loose: &loose,
            heading: "Not on the map",
            links: &links,
        };
        let (svg, map) = crate::render_tiled(source, None, Some(&tiles)).unwrap();
        let keys: Vec<(&str, Option<usize>, Option<&str>)> = map
            .tiles
            .iter()
            .map(|t| (t.key.as_str(), t.line, t.parent.as_deref()))
            .collect();
        assert_eq!(
            keys,
            [
                ("orders", Some(2), None),
                ("checkout", Some(3), Some("orders")),
                ("payment", Some(5), None),
                ("refund", None, None),
            ]
        );
        let page = size(&svg);
        for tile in &map.tiles {
            assert!(within(tile.picture, tile.frame), "{tile:?}");
            assert!(within(tile.frame, page), "{tile:?} {page:?}");
            assert!((tile.picture.w - 160.0).abs() < 0.01, "{tile:?}");
        }
        let refund = &map.tiles[3];
        assert!(
            map.tiles[..3]
                .iter()
                .all(|t| t.frame.y + t.frame.h < refund.frame.y)
        );
        assert!(svg.contains("Not on the map"), "{svg}");
        assert_eq!(svg.matches("stroke-dasharray").count(), 1, "{svg}");
        // The boxes that name nothing stay boxes, and the plain drawing is
        // the drawing without tiles.
        assert!(svg.contains(">Notes<") && svg.contains(">Ideas<"));
        assert_eq!(crate::render(source).unwrap(), wbs.0);
    }

    #[test]
    fn a_mind_map_draws_tiles_on_both_sides() {
        let source = "@startmindmap\n* Shop\n** Orders\n*** Checkout\n-- Payment\n@endmindmap\n";
        let tiles = Tiles {
            pick: &known,
            picture: (120.0, 80.0),
            loose: &[],
            heading: "",
            links: &[],
        };
        let (svg, map) = crate::render_tiled(source, None, Some(&tiles)).unwrap();
        let keys: Vec<(&str, Option<&str>)> = map
            .tiles
            .iter()
            .map(|t| (t.key.as_str(), t.parent.as_deref()))
            .collect();
        assert_eq!(
            keys,
            [
                ("shop", None),
                ("orders", Some("shop")),
                ("checkout", Some("orders")),
                ("payment", Some("shop")),
            ]
        );
        let page = size(&svg);
        for tile in &map.tiles {
            assert!(
                within(tile.picture, tile.frame) && within(tile.frame, page),
                "{tile:?}"
            );
        }
        let (shop, payment) = (&map.tiles[0].frame, &map.tiles[3].frame);
        assert!(
            payment.x + payment.w < shop.x,
            "{payment:?} left of {shop:?}"
        );
        assert!(!svg.contains("stroke-dasharray"));
    }
}
