//! Mind maps and work breakdown structures: trees of boxes that PlantUML
//! lays out itself. Their syntax is close (lines of `*` give the levels),
//! their layouts are not: a mind map grows to both sides of its root, a
//! WBS hangs down from it.

mod cards;
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
pub use cards::{
    Branch, Card, Heading, Icon, PICTURE_ROUND, Relation, RelationKind, draw_branches,
};
pub use mermaid::{from_mermaid, mermaid_refusal};
pub use mindmap::draw as draw_mindmap;
pub use mindmap::draw_mapped as draw_mindmap_mapped;
pub use parse::{parse_mindmap, parse_wbs};
pub use wbs::draw as draw_wbs;
pub use wbs::draw_mapped as draw_wbs_mapped;

/// What the map of a project says beside its boxes
/// (`docs/adr/0026-the-map-is-made.md`): each box with a key is a card
/// with room for its diagram's picture, which the caller puts in the room
/// [`crate::map::Map::tiles`] gives.
pub struct Tiles<'a> {
    /// What the card of a diagram says, by its key; `None` before the
    /// caller knows.
    pub card: &'a dyn Fn(&str) -> Option<Card>,
    /// Arrows from card to card.
    pub relations: &'a [Relation],
    pub heading: Option<&'a Heading>,
}

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
    /// `<<name>>` after its label: what a style's `.name` reaches.
    pub class: Option<String>,
    /// What the pane calls the box when it folds it
    /// ([0024](../../../docs/adr/0024-focus-and-collapse.md)): its label
    /// on one line, numbered (`Tests (2)`) when a box before it has the
    /// same. Empty for a box without a label.
    pub key: String,
    /// How many boxes were folded away below it; 0 when it is not folded.
    /// The box is drawn as it was, so it glides as it is; the pane marks
    /// it.
    pub folded: usize,
}

/// Gives every box of a tree its [`Node::key`], in the order they are
/// handed to it.
#[derive(Default)]
pub(crate) struct Names(std::collections::HashMap<String, usize>);

impl Names {
    pub(crate) fn name(&mut self, node: &mut Node) {
        let label = node
            .text
            .value
            .replace("\\n", " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if label.is_empty() {
            return;
        }
        let nth = self.0.entry(label.clone()).or_insert(0);
        *nth += 1;
        node.key = match *nth {
            1 => label,
            nth => format!("{label} ({nth})"),
        };
    }

    fn ideas(&mut self, ideas: &mut [Idea]) {
        for idea in ideas {
            self.name(&mut idea.node);
            self.ideas(&mut idea.children);
        }
    }

    fn element(&mut self, element: &mut Element) {
        self.name(&mut element.node);
        for child in element.left.iter_mut().chain(&mut element.right) {
            self.element(child);
        }
    }
}

impl Idea {
    /// Whether it and every box below it has a name.
    fn named(&self) -> bool {
        !self.node.key.is_empty() && self.children.iter().all(Idea::named)
    }

    /// How many boxes hang below it.
    fn below(&self) -> usize {
        self.children.iter().map(|c| 1 + c.below()).sum()
    }

    fn fold(&mut self, names: &[String]) {
        if !self.children.is_empty() && names.contains(&self.node.key) {
            self.node.folded = self.below();
            self.children.clear();
        }
        for child in &mut self.children {
            child.fold(names);
        }
    }
}

impl MindMap {
    /// Names every box ([`Node::key`]): once, after the map is read.
    pub(crate) fn name(&mut self) {
        let mut names = Names::default();
        for map in &mut self.maps {
            names.name(&mut map.root);
            names.ideas(&mut map.right);
            names.ideas(&mut map.left);
        }
    }

    /// Folds the boxes named in `names` to themselves, without what hangs
    /// below them (docs/adr/0024). A name of a box that holds nothing
    /// folds nothing.
    pub fn fold(&mut self, names: &[String]) {
        for map in &mut self.maps {
            let below: usize = map
                .right
                .iter()
                .chain(&map.left)
                .map(|i| 1 + i.below())
                .sum();
            if below > 0 && names.contains(&map.root.key) {
                map.root.folded = below;
                map.right.clear();
                map.left.clear();
            }
            for idea in map.right.iter_mut().chain(&mut map.left) {
                idea.fold(names);
            }
        }
    }
}

impl Element {
    /// Whether it and every box below it has a name: then each link is a
    /// line of the map, which a pane glides.
    fn named(&self) -> bool {
        !self.node.key.is_empty() && self.left.iter().chain(&self.right).all(Element::named)
    }

    fn below(&self) -> usize {
        self.left
            .iter()
            .chain(&self.right)
            .map(|c| 1 + c.below())
            .sum()
    }

    fn fold(&mut self, names: &[String]) {
        let below = self.below();
        if below > 0 && names.contains(&self.node.key) {
            self.node.folded = below;
            self.left.clear();
            self.right.clear();
        }
        for child in self.left.iter_mut().chain(&mut self.right) {
            child.fold(names);
        }
    }
}

impl Wbs {
    /// Names every box ([`Node::key`]): once, after the diagram is read.
    pub(crate) fn name(&mut self) {
        Names::default().element(&mut self.root);
    }

    /// Folds the boxes named in `names`, as [`MindMap::fold`] does.
    pub fn fold(&mut self, names: &[String]) {
        self.root.fold(names);
    }
}

/// The link from the box `from` to the box `to` below it, drawn through
/// `points`, for the map: a pane glides it with its boxes (docs/adr/0024).
/// None when one of the two has no name.
fn wire(
    (from, from_rect): (&str, crate::graph::Rect),
    (to, to_rect): (&str, crate::graph::Rect),
    points: Vec<(f32, f32)>,
    stroke: Stroke,
) -> Option<crate::map::Wire> {
    let end = |rect| crate::hand::End { rect, flat: false };
    (!from.is_empty() && !to.is_empty()).then(|| crate::map::Wire {
        key: crate::hand::Key {
            from: from.to_owned(),
            to: to.to_owned(),
            nth: 0,
        },
        line: None,
        path: points.clone(),
        points,
        from: end(from_rect),
        to: end(to_rect),
        entry: None,
        labels: Vec::new(),
        stroke,
        heads: Vec::new(),
        fixed: true,
    })
}

/// The box of the node called `key` at `rect`, for the map; none for a
/// box without a name.
fn place(key: &str, rect: crate::graph::Rect, frame: bool) -> Option<crate::map::Place> {
    (!key.is_empty()).then(|| crate::map::Place {
        name: key.to_owned(),
        rect,
        within: Vec::new(),
        across: false,
        entry: None,
        frame,
        fixed: true,
    })
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
    /// How this box looks: the skin's boxes, and what the styles set for
    /// its place, its stereotype and its depth.
    look: skin::Look,
    block: Block,
    shape: NodeShape,
    fill: Paint,
}

impl Label {
    /// The label of `node`, `depth` boxes below the root, a `leaf` when
    /// none hangs below it.
    fn new(
        node: &Node,
        skin: &skin::Skin,
        family: Family,
        (depth, leaf): (usize, bool),
    ) -> Result<Label, Declined> {
        let line = node.text.line;
        let look = skin.look(skin::Spot {
            depth,
            leaf,
            boxless: matches!(node.shape, NodeShape::Plain | NodeShape::Pseudo),
            class: node.class.as_deref(),
        });
        let mut font = look.font;
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
            None => look.fill,
        };
        Ok(Label {
            look,
            block,
            shape: node.shape,
            fill,
        })
    }

    /// The width and the height of what is written in the box.
    fn content(&self) -> (f32, f32) {
        (self.block.width, self.block.height)
    }

    /// The size of the box, or of the text with its margin when there is
    /// no box: `before` and `after` are the text's margins on the left and
    /// the right.
    fn size(&self, skin: &skin::Skin, before: f32, after: f32) -> (f32, f32) {
        let b = self.content();
        let pad = skin.padding;
        match self.shape {
            NodeShape::Box | NodeShape::Square => (b.0 + 2.0 * pad, b.1 + 2.0 * pad),
            // Wide enough that the text's rectangle sits inside the circle.
            NodeShape::Circle => {
                let d = b.0.hypot(b.1) + 2.0 * pad;
                (d, d)
            }
            // The points eat into a box the text would fill, so the box is wider.
            NodeShape::Hexagon => (b.0 + 3.0 * pad, b.1 + 2.0 * pad),
            // The bumps sit in a rim outside the text's padding.
            NodeShape::Cloud => {
                let rim = CLOUD_RIM;
                (b.0 + 2.0 * pad + 2.0 * rim, b.1 + 2.0 * pad + 2.0 * rim)
            }
            // The points stick out of the circle that holds the text.
            NodeShape::Bang => {
                let d = (b.0.hypot(b.1) + 2.0 * pad) / BANG_INNER;
                (d, d)
            }
            NodeShape::Plain => (b.0 + before + after, b.1 + 2.0),
            NodeShape::Pseudo => (0.0, 0.0),
        }
    }

    /// Draws the box (or the text alone) with its top left corner at
    /// `(x, y)`; `before` as for [`Label::size`].
    fn draw(&self, canvas: &mut Canvas, skin: &skin::Skin, (x, y): (f32, f32), before: f32) {
        let (w, h) = self.size(skin, before, 0.0);
        let content = self.content();
        let stroke = Stroke::new(self.look.border, self.look.thickness);
        let inner = match self.shape {
            NodeShape::Box | NodeShape::Square => {
                let round = if self.shape == NodeShape::Square {
                    0.0
                } else {
                    self.look.round
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
                let left = x + (w - content.0) / 2.0;
                let top = y + (h - content.1) / 2.0;
                (left, top, content.0)
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
                let left = x + (w - content.0) / 2.0;
                (left, y + skin.padding, content.0)
            }
            NodeShape::Cloud => {
                canvas.path(
                    Layer::Front,
                    &cloud_path(x, y, w, h),
                    (x, y, x + w, y + h),
                    self.fill,
                    Some(stroke),
                );
                let left = x + (w - content.0) / 2.0;
                let top = y + (h - content.1) / 2.0;
                (left, top, content.0)
            }
            NodeShape::Bang => {
                canvas.polygon(
                    Layer::Front,
                    &bang_points(x, y, w, h),
                    self.fill,
                    Some(stroke),
                );
                let left = x + (w - content.0) / 2.0;
                let top = y + (h - content.1) / 2.0;
                (left, top, content.0)
            }
            NodeShape::Plain => (x + before, y + 1.0, content.0),
            NodeShape::Pseudo => return,
        };
        let (left, top, width) = inner;
        let anchor = match skin.align {
            Align::Left => left,
            Align::Center => left + width / 2.0,
            Align::Right => left + width,
        };
        canvas.text(Layer::Front, &self.block, (anchor, top), skin.align);
    }
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
