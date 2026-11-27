//! Draws a structure diagram. A package, node or frame a line runs into
//! from outside is laid out with what is around it ([`open`]); any other
//! is laid out on its own and becomes a box around its children's layout.
//!
//! Notes keep the side that was asked for. "Right" stays on the right,
//! including when the group runs left to right.

use std::fmt::Write as _;

use super::skin::{ComponentStyle, Skin};
use super::{Crossing, Dash, Direction, Element, Group, Head, Link, Member, Shape, Side};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::{self, Color, Paint};
use crate::graph::across::{self, Drawn, Placement};
use crate::graph::by_hand::{self, Arrow, Dress, Laid, LineMut};
use crate::graph::{self, Rect};
use crate::hand::Key;
use crate::map::{self, HeadFill, Map};
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{self, Block, Family, Font, Style};

mod names;
mod open;
mod shapes;

use names::Names;
use open::layout_group;

const MARGIN: f32 = 10.0;
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 6.0;
const MIN_W: f32 = 54.0;
const MIN_H: f32 = 28.0;
const COMP_PAD: f32 = 12.0;
/// A component's icon in its top right corner: its width, how far it
/// sits from the box's edge, and the air its name keeps from it.
const ICON_W: f32 = 14.0;
const ICON_EDGE: f32 = 6.0;
const ICON_AIR: f32 = 6.0;
const COMP_GAP: f32 = 8.0;
const NOTE_GAP: f32 = 10.0;
const NOTE_PAD: f32 = 6.0;
/// A `--` between members, and the line under a class name.
const RULE: f32 = 10.0;
/// How far a package's body reaches past its tab at the least, so the
/// tab reads as one.
const SHOULDER: f32 = 16.0;
const HEAD_LENGTH: f32 = 10.0;
const HEAD_HALF: f32 = 4.0;
const MARK_LENGTH: f32 = 14.0;
const DEPTH: f32 = 10.0;
const STACK: f32 = 5.0;
/// Cylinder caps and the stick figure, shared by measuring and painting.
const CAP: f32 = 9.0;
/// How far a cloud's scalloped edge dips into its box, and how long a bump
/// of it is at most.
const CLOUD_DIP: f32 = 6.0;
const CLOUD_BUMP: f32 = 26.0;
const ACTOR_H: f32 = 50.0;
const ACTOR_W: f32 = 40.0;
/// How wide a lollipop's circle is, and the gap under it to its name.
const LOLLIPOP: f32 = 16.0;
const LOLLIPOP_GAP: f32 = 3.0;

pub fn draw(diagram: &super::Diagram) -> Result<String, Declined> {
    drawn(diagram, false).map(|(svg, _)| svg)
}

/// The SVG, and where its lines are in it: each as a wire the writer may
/// draw by hand.
pub fn draw_mapped(diagram: &super::Diagram) -> Result<(String, Map), Declined> {
    drawn(diagram, true)
}

/// The SVG, and its map when `mapped` asks for one; an empty map else.
fn drawn(diagram: &super::Diagram, mapped: bool) -> Result<(String, Map), Declined> {
    let skin = Skin::from_params(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    // Set first: what takes the page's fill takes its gradient. A
    // picture inside a note has no page of its own: the note's is it.
    canvas.page_gradient(skin.page_gradient.filter(|_| !crate::embedded::inside()));
    canvas.sketch(skin.sketch);
    canvas.shadows(skin.shadow);
    let family = canvas.family();
    let annotations = Annotations::new(
        &diagram.frame,
        [
            skin.title_font,
            skin.header_font,
            skin.footer_font,
            skin.caption_font,
        ],
        family,
    )?;
    let mut scene = layout_group(&diagram.root, &skin, family)?;
    link_crossings(&diagram.root, &mut scene, &skin, family)?;
    let hands = &diagram.frame.by_hand;
    let arrows = if mapped || !hands.is_empty() {
        arrows(&diagram.root)
    } else {
        Vec::new()
    };
    let misses = if hands.is_empty() {
        Vec::new()
    } else {
        by_hand::lay(&placements(&scene.nodes), &arrows, &mut scene, hands)
    };
    scene.offset(0.0, annotations.top());
    // Every part by its name from the source (docs/adr/0037).
    let names = Names::of(&diagram.root);
    paint(&mut canvas, &skin, &scene, &names);
    annotations.draw(&mut canvas);
    let map = if mapped {
        let top = placements(&scene.nodes);
        canvas.place(
            Map {
                places: places(&diagram.root, &top, &diagram.frame.held, &names),
                wires: by_hand::wires(
                    &top,
                    &arrows,
                    &scene,
                    hands,
                    |points| crate::curve::flatten(points, skin.curve),
                    |path, slot, drawn| dress(&scene, (path, slot), drawn, &skin, &names),
                ),
                misses: {
                    let mut misses = misses;
                    misses.extend(crate::hand::said(
                        &diagram.frame.held,
                        &diagram.frame.unplaced,
                        &scene.all_unheld(),
                    ));
                    misses.sort();
                    misses
                },
                inside: {
                    let mut inside = Vec::new();
                    notes_inside(&scene, &names, &mut inside);
                    inside
                },
                ..Map::default()
            },
            MARGIN,
        )
    } else {
        Map::default()
    };
    Ok((canvas.finish(MARGIN, skin.background), map))
}

/// Every line of the diagram, with what the block of lines by hand calls
/// it: the names lines use for its ends.
/// Puts each label whose side the writer chose by hand on that side of
/// its line ([`crate::hand::Label`]).
pub fn label_by_hand(root: &mut Group, labels: &[crate::hand::Label]) {
    if labels.is_empty() {
        return;
    }
    let keyed = arrows(root);
    for label in labels {
        let Some((arrow, _)) = keyed.iter().find(|(_, key)| *key == label.key) else {
            continue;
        };
        let mut group = &mut *root;
        for &index in &arrow.path {
            group = &mut group.nodes[index].children;
        }
        let link = match group.edges.get_mut(arrow.slot) {
            Some(edge) => edge,
            None => &mut group.crossings[arrow.slot - group.edges.len()].arrow,
        };
        link.label_before = Some(label.side.before());
    }
}

fn arrows(root: &Group) -> Vec<(Arrow, Key)> {
    fn collect(group: &Group, path: &mut Vec<usize>, out: &mut Vec<Arrow>) {
        let within = |end: &[usize]| [path.as_slice(), end].concat();
        // A hidden line is no line of the picture: not in its map, and
        // not one to shape by hand.
        for (slot, edge) in group.edges.iter().enumerate() {
            if edge.hidden {
                continue;
            }
            out.push(Arrow {
                path: path.clone(),
                slot,
                from: within(&[edge.from]),
                to: within(&[edge.to]),
                order: edge.line,
                line: Some(edge.line),
            });
        }
        for (index, crossing) in group.crossings.iter().enumerate() {
            if crossing.arrow.hidden {
                continue;
            }
            out.push(Arrow {
                path: path.clone(),
                slot: group.edges.len() + index,
                from: within(&crossing.from),
                to: within(&crossing.to),
                order: crossing.arrow.line,
                line: Some(crossing.arrow.line),
            });
        }
        for (index, node) in group.nodes.iter().enumerate() {
            path.push(index);
            collect(&node.children, path, out);
            path.pop();
        }
    }
    let mut found = Vec::new();
    collect(root, &mut Vec::new(), &mut found);
    by_hand::keyed(found, |path| key_at(root, path))
}

/// The key of the box at `path`.
fn key_at(root: &Group, path: &[usize]) -> String {
    let mut group = root;
    let mut key = String::new();
    for &index in path {
        let Some(node) = group.nodes.get(index) else {
            break;
        };
        key.clone_from(&node.key);
        group = &node.children;
    }
    key
}

/// Every box a writer may hold in place, for the map (docs/adr/0021).
fn places(
    root: &Group,
    top: &[Placement],
    held: &[(usize, crate::hand::Hold)],
    names: &Names,
) -> Vec<crate::map::Place> {
    let across = |within: &[usize]| {
        let mut group = root;
        for &index in within {
            match group.nodes.get(index) {
                Some(node) => group = &node.children,
                None => return false,
            }
        }
        group.direction == Direction::Right
    };
    by_hand::places(
        top,
        |path| key_at(root, path),
        across,
        held,
        |path| Some(names.node(path)),
    )
}

impl Laid for Scene {
    fn points(&self, path: &[usize], slot: usize) -> Option<&[(f32, f32)]> {
        let mut here = self;
        for &index in path {
            here = &here.nodes.get(index)?.children;
        }
        here.edges.get(slot).map(|edge| edge.points.as_slice())
    }

    fn line_mut(&mut self, path: &[usize], slot: usize) -> Option<LineMut<'_>> {
        let edge = self.scene_at(path)?.edges.get_mut(slot)?;
        Some(LineMut {
            label: (edge.label.width, edge.label.height),
            points: &mut edge.points,
            label_at: &mut edge.label_at,
        })
    }

    fn labels(&self, path: &[usize], slot: usize) -> Vec<Rect> {
        let mut here = self;
        for &index in path {
            let Some(node) = here.nodes.get(index) else {
                return Vec::new();
            };
            here = &node.children;
        }
        let Some(edge) = here.edges.get(slot) else {
            return Vec::new();
        };
        let containers = containers(self);
        edge.label_rect()
            .into_iter()
            .chain(card_rects(edge, &containers))
            .collect()
    }

    fn drawn(&self) -> Drawn {
        let mut drawn = Drawn::default();
        drawn_in(self, &mut drawn);
        drawn
    }
}

/// One group after layout. `width` and `height` are the graph, which is
/// what a parent sizes its box from. A note above the first row can sit
/// outside that, and the canvas still grows to hold it.
type Scene = graph::open::Scene<Item, Look>;
type Placed = graph::open::Placed<Item, Look>;
type PlacedEdge = graph::open::Wire<Look>;
type PlacedNote = graph::open::Note;
type Measured = graph::open::Measured<Item, Look>;

/// What a box keeps for painting.
struct Item {
    /// Its key: what the sketch look seeds its strokes with.
    name: String,
    shape: Shape,
    fill: Paint,
    /// The gradient its shape is filled with, where it is one.
    gradient: Option<color::Gradient>,
    border: Stroke,
    rows: Vec<Row>,
    /// First member row. Equal to `rows.len()` when the box has none.
    body: usize,
    /// How far the title band reaches down from the top of the box.
    /// Lines into a container stay below it. Zero for a leaf that is not a
    /// package; a package uses it as the tab's height.
    banner: f32,
    gap: f32,
    composite: bool,
    /// Followed on a click on the box and its words.
    link: Option<crate::text::Link>,
    /// A lollipop's name stands beside its circle, across a flow down the
    /// page; else under it. The box is the circle alone, so the lines end
    /// on it, and the name has the room across the flow
    /// ([`open::lollipop_rooms`]).
    beside: bool,
    /// `hide Name`: the box keeps its place and is not drawn.
    hidden: bool,
}

/// What a line keeps for painting, besides its label.
struct Look {
    from_card: Block,
    to_card: Block,
    from_head: Head,
    to_head: Head,
    stroke: Stroke,
    /// `-[hidden]-`: laid out, not painted.
    hidden: bool,
}

impl Look {
    /// The look of `arrow` with its measured multiplicities. A hidden line
    /// has no heads, so nothing of it is drawn or listed.
    fn of(arrow: &Link, blocks_from: Block, blocks_to: Block, stroke: Stroke) -> Look {
        let head = |head| if arrow.hidden { Head::None } else { head };
        Look {
            from_card: blocks_from,
            to_card: blocks_to,
            from_head: head(arrow.from_head),
            to_head: head(arrow.to_head),
            stroke,
            hidden: arrow.hidden,
        }
    }
}

enum Row {
    Center(Block),
    Left(Block),
    Rule(Dash),
}

impl Row {
    fn width(&self) -> f32 {
        match self {
            Row::Center(block) | Row::Left(block) => block.width,
            Row::Rule(_) => 0.0,
        }
    }

    fn height(&self) -> f32 {
        match self {
            Row::Center(block) | Row::Left(block) => block.height,
            Row::Rule(_) => RULE,
        }
    }

    fn block(&self) -> Option<&Block> {
        match self {
            Row::Center(block) | Row::Left(block) => Some(block),
            Row::Rule(_) => None,
        }
    }
}

struct Labeled {
    label: Block,
    from_card: Block,
    to_card: Block,
}

fn child_origin(rect: Rect, item: &Measured) -> (f32, f32) {
    let (x, w) = match item.item.shape {
        Shape::Node => (rect.x, rect.w - DEPTH),
        Shape::Stack => (rect.x, rect.w - STACK * 2.0),
        _ => (rect.x, rect.w),
    };
    let ox = x + (w - item.children.width) / 2.0;
    let oy = rect.y + item.item.banner + item.item.gap;
    (ox, oy)
}

/// A container the layout around it opens holds no layout of its own; it
/// is measured as if it held something.
fn measure_element(
    el: &Element,
    skin: &Skin,
    family: Family,
    open: bool,
    direction: Direction,
) -> Result<Measured, Declined> {
    let mut look = skin.look(el.shape, el.stereotypes.iter().map(|s| s.value.as_str()));
    if let Some(value) = &el.text_color {
        let color = match color::parse(value).map_err(|err| Declined::at(el.line, err))? {
            Some(color) => color,
            None => {
                return Err(Declined::at(
                    el.line,
                    "a text color must not be transparent",
                ));
            }
        };
        look.font.color = color;
        look.member.color = color;
        look.stereo.color = color;
    }
    let (fill, gradient, border) = paints(el, &look)?;
    // `skinparam packageStyle`: a package drawn as another shape, in its
    // own look.
    let shape = match el.shape {
        Shape::Package => skin.package_style.unwrap_or(Shape::Package),
        other => other,
    };
    // A fill the writer gave a box may be anything under the skin's text:
    // light under the light text of a dark look. The words are moved
    // towards black or white, whichever reads better there, until they
    // read. An actor's name stands under the figure, on the page. Over a
    // gradient they read on both its ends.
    if el.text_color.is_none()
        && el.fill.is_some()
        && !matches!(el.shape, Shape::Actor | Shape::Lollipop)
    {
        let grounds = color::grounds(fill, gradient);
        for font in [&mut look.font, &mut look.member, &mut look.stereo] {
            legible_on(font, &grounds);
        }
    }
    // An actor's and a use case's words belong to their figure.
    let left = skin.left && !matches!(el.shape, Shape::Actor | Shape::UseCase | Shape::Lollipop);
    let (mut rows, mut body) = if el.shape == Shape::Point {
        (Vec::new(), 0)
    } else {
        rows_of(el, &look, family, left)?
    };
    // A package holding nothing has its name in the tab and its
    // stereotypes, or what a fold says it holds, in the body, as
    // PlantUML draws one.
    let tabbed = matches!(shape, Shape::Package | Shape::Folder) && !el.composite;
    if tabbed && body > el.stereotypes.len() {
        rows[..body].rotate_left(el.stereotypes.len());
        body -= el.stereotypes.len();
    }
    let children = if el.composite && !open {
        layout_group(&el.children, skin, family)?
    } else {
        Scene::empty()
    };
    let header_w = rows[..body].iter().map(Row::width).fold(0.0, f32::max);
    let header_h = rows[..body].iter().map(Row::height).sum::<f32>();
    let all_w = rows.iter().map(Row::width).fold(0.0, f32::max);
    let all_h = rows.iter().map(Row::height).sum::<f32>();
    let min_w = min_width(shape, skin);
    let divider = if !el.composite && (body < rows.len() || shape == Shape::Card) {
        RULE
    } else {
        0.0
    };
    let empty = !open && children.width == 0.0 && children.height == 0.0;
    let gap = if empty { 0.0 } else { COMP_GAP };

    let (width, height, banner, gap) = if el.composite {
        match shape {
            Shape::Node => {
                let banner = DEPTH + COMP_PAD + header_h;
                let width = (header_w.max(children.width) + COMP_PAD * 2.0).max(min_w) + DEPTH;
                let height = banner + gap + children.height + COMP_PAD;
                (width, height, banner, gap)
            }
            Shape::Stack => {
                let extra = STACK * 2.0;
                let banner = extra + COMP_PAD + header_h;
                let width = (header_w.max(children.width) + COMP_PAD * 2.0).max(min_w) + extra;
                let height = banner + gap + children.height + COMP_PAD;
                (width, height, banner, gap)
            }
            Shape::Package | Shape::Folder => {
                let banner = header_h + 8.0;
                let below = if empty { COMP_PAD } else { gap };
                let width = (children.width + COMP_PAD * 2.0)
                    .max(header_w + PAD_X * 2.0 + SHOULDER)
                    .max(min_w);
                let height = banner + below + children.height + COMP_PAD;
                (width, height, banner, below)
            }
            Shape::Database | Shape::Storage => {
                let banner = CAP + COMP_PAD + header_h;
                let width = (header_w.max(children.width) + COMP_PAD * 2.0).max(min_w);
                let height = banner + gap + children.height + COMP_PAD + CAP;
                (width, height, banner, gap)
            }
            _ => {
                let banner = COMP_PAD + header_h;
                let width = (header_w.max(children.width) + COMP_PAD * 2.0).max(min_w);
                let height = banner + gap + children.height + COMP_PAD;
                (width, height, banner, gap)
            }
        }
    } else {
        match shape {
            Shape::Actor => {
                let width = all_w.max(ACTOR_W).max(min_w);
                let height = ACTOR_H + all_h + 2.0;
                (width, height, 0.0, 0.0)
            }
            // The circle alone: its name has the room beside it.
            Shape::Lollipop => (LOLLIPOP, LOLLIPOP, 0.0, 0.0),
            // A point on a line, no box: the lines meet there.
            Shape::Point => (0.0, 0.0, 0.0, 0.0),
            Shape::UseCase => {
                let mut width = (all_w + PAD_X * 3.0).max(min_w.max(84.0));
                let height = (all_h + PAD_Y * 3.0).max(40.0);
                if width < height * 1.7 {
                    width = height * 1.7;
                }
                (width, height, 0.0, 0.0)
            }
            Shape::Cloud | Shape::Hexagon => {
                let width = (all_w + PAD_X * 3.0).max(min_w.max(76.0));
                let height = (all_h + PAD_Y * 3.0).max(44.0);
                (width, height, 0.0, 0.0)
            }
            Shape::Database | Shape::Storage => {
                let width = (all_w + PAD_X * 2.0).max(min_w.max(70.0));
                let height = all_h + PAD_Y * 2.0 + CAP * 2.0;
                (width, height, 0.0, 0.0)
            }
            Shape::Node => {
                let width = (all_w + PAD_X * 2.0).max(min_w) + DEPTH;
                let height = (all_h + PAD_Y * 2.0).max(MIN_H) + DEPTH;
                (width, height, 0.0, 0.0)
            }
            Shape::Stack => {
                let extra = STACK * 2.0;
                let width = (all_w + PAD_X * 2.0).max(min_w) + extra;
                let height = (all_h + PAD_Y * 2.0).max(MIN_H) + extra;
                (width, height, 0.0, 0.0)
            }
            Shape::Agent => {
                let width = (all_w + PAD_X * 2.0).max(min_w);
                let height = (all_h + PAD_Y * 2.0).max(MIN_H) + 12.0;
                (width, height, 0.0, 0.0)
            }
            Shape::Person => {
                let width = (all_w + PAD_X * 3.0).max(min_w.max(shapes::HEAD * 2.5));
                let height = (all_h + PAD_Y * 3.0).max(MIN_H) + shapes::head_room();
                (width, height, 0.0, 0.0)
            }
            Shape::Queue => {
                let width = (all_w + PAD_X * 2.0).max(min_w) + 3.0 * shapes::BOW;
                let height = (all_h + PAD_Y * 2.0).max(MIN_H);
                (width, height, 0.0, 0.0)
            }
            Shape::Package | Shape::Folder => {
                let banner = header_h + 8.0;
                let under_w = rows[body..].iter().map(Row::width).fold(0.0, f32::max);
                let under_h = rows[body..].iter().map(Row::height).sum::<f32>();
                let width = (header_w + PAD_X * 2.0 + SHOULDER)
                    .max(under_w + PAD_X * 2.0)
                    .max(min_w);
                let height = banner + (under_h + PAD_Y * 2.0).max(20.0);
                (width, height, banner, 0.0)
            }
            Shape::Note => {
                let width = (all_w + NOTE_PAD * 2.0 + 6.0).max(24.0);
                let height = (all_h + NOTE_PAD * 2.0).max(20.0);
                (width, height, 0.0, 0.0)
            }
            _ => {
                // The name is centered in the box, so the room the icon
                // takes on the right is kept on the left as well.
                let icon = if shape == Shape::Component
                    && skin.component_style != ComponentStyle::Rectangle
                {
                    (2.0 * (ICON_W + ICON_EDGE + ICON_AIR) - 2.0 * PAD_X).max(0.0)
                } else {
                    0.0
                };
                let width = (all_w + icon + PAD_X * 2.0).max(min_w);
                let height = (all_h + divider + PAD_Y * 2.0).max(MIN_H);
                (width, height, 0.0, 0.0)
            }
        }
    };

    Ok(Measured {
        width,
        height,
        flat: is_flat(shape),
        children,
        item: Item {
            name: el.key.clone(),
            shape,
            fill,
            gradient,
            border,
            rows,
            body,
            banner,
            gap,
            composite: el.composite,
            link: el.link.clone(),
            beside: direction == Direction::Down,
            hidden: el.hidden,
        },
    })
}

/// The room an opened container keeps around what it holds, as
/// [`measure_element`] sizes one around a layout of its own.
fn frame_of(item: &Item, skin: &Skin) -> graph::Frame {
    let header_w = item.rows[..item.body]
        .iter()
        .map(Row::width)
        .fold(0.0, f32::max);
    let min_w = min_width(item.shape, skin);
    let top = item.banner + item.gap;
    let (bottom, right, min_width) = match item.shape {
        Shape::Node => (
            COMP_PAD,
            COMP_PAD + DEPTH,
            (header_w + COMP_PAD * 2.0).max(min_w) + DEPTH,
        ),
        Shape::Stack => (
            COMP_PAD,
            COMP_PAD + STACK * 2.0,
            (header_w + COMP_PAD * 2.0).max(min_w) + STACK * 2.0,
        ),
        Shape::Package | Shape::Folder => (
            COMP_PAD,
            COMP_PAD,
            (header_w + PAD_X * 2.0 + SHOULDER).max(min_w),
        ),
        Shape::Database | Shape::Storage => (
            COMP_PAD + CAP,
            COMP_PAD,
            (header_w + COMP_PAD * 2.0).max(min_w),
        ),
        _ => (COMP_PAD, COMP_PAD, (header_w + COMP_PAD * 2.0).max(min_w)),
    };
    // Lines from above pass beside the name: a package's tab, or the
    // middle of the band of the others.
    let name = match item.shape {
        Shape::Package | Shape::Folder => graph::Name {
            share: 0.0,
            offset: 0.0,
            width: (header_w + PAD_X * 2.0).max(28.0),
            height: item.banner,
        },
        _ => {
            let width = header_w + 8.0;
            graph::Name {
                share: 0.5,
                offset: -(right - COMP_PAD + width) / 2.0,
                width,
                height: item.banner,
            }
        }
    };
    graph::Frame {
        parent: None,
        top,
        bottom,
        left: COMP_PAD,
        right,
        min_width,
        name,
    }
}

fn min_width(shape: Shape, skin: &Skin) -> f32 {
    let class = matches!(
        shape,
        Shape::Class
            | Shape::Abstract
            | Shape::Interface
            | Shape::Enum
            | Shape::Entity
            | Shape::Annotation
            | Shape::Object
    );
    if class {
        MIN_W.max(skin.min_class)
    } else {
        MIN_W
    }
}

/// The rows of a box: what it is, its name, its members. `left` starts
/// the first two at the box's left edge instead of its middle.
fn rows_of(
    el: &Element,
    look: &super::skin::Look,
    family: Family,
    left: bool,
) -> Result<(Vec<Row>, usize), Declined> {
    let head = |block| {
        if left {
            Row::Left(block)
        } else {
            Row::Center(block)
        }
    };
    let mut rows = Vec::new();
    // What one line alone writes is tied to it: a stereotype, a member,
    // a heading, the words of a note. The name, which lines mention
    // again and again, is not.
    for stereotype in &el.stereotypes {
        let shown = format!("«{}»", stereotype.value);
        let block = text::block(&shown, look.stereo, family)
            .map_err(|err| Declined::at(stereotype.line, err))?;
        rows.push(head(block.written_on(stereotype.line)));
    }
    let name_font = if el.shape == Shape::Abstract {
        look.font.italic()
    } else {
        look.font
    };
    let mut name = if el.shape == Shape::Note {
        el.label.measure_written(name_font, family)?
    } else {
        el.label.measure(name_font, family)?
    };
    if el.shape == Shape::Object {
        name = restyle(name, name_font, family, false, true);
    }
    if !name.is_empty() {
        rows.push(head(name));
    }
    let body = rows.len();
    for member in &el.members {
        match member {
            Member::Text {
                text,
                italic,
                underline,
                ..
            } => {
                let block = text.measure_written(look.member, family)?;
                let block = restyle(block, look.member, family, *italic, *underline);
                if !block.is_empty() {
                    rows.push(Row::Left(block));
                }
            }
            Member::Rule(dash) => rows.push(Row::Rule(*dash)),
            Member::Heading(text) => {
                let block = text.measure_written(look.member, family)?;
                if !block.is_empty() {
                    rows.push(Row::Center(block));
                }
            }
        }
    }
    Ok((rows, body))
}

/// `block` in italics or underlined; the line that writes it stays its
/// line.
fn restyle(block: Block, font: Font, family: Family, italic: bool, underline: bool) -> Block {
    if block.is_empty() || (!italic && !underline) {
        return block;
    }
    let written = block.written;
    let lines = block
        .lines
        .into_iter()
        .map(|line| {
            line.runs
                .into_iter()
                .map(|run| {
                    let mut style = run.style;
                    if italic {
                        style.italic = true;
                    }
                    if underline {
                        style.underline = true;
                    }
                    (run.text, style)
                })
                .collect::<Vec<(String, Style)>>()
        })
        .collect();
    Block {
        written,
        ..text::styled(lines, font, family)
    }
}

/// An element's fill, the gradient it runs as where it is one (its plain
/// fill is then the gradient's first color), and its border.
fn paints(
    el: &Element,
    look: &super::skin::Look,
) -> Result<(Paint, Option<color::Gradient>, Stroke), Declined> {
    let (fill, gradient) = match &el.fill {
        Some(value) => fill_of(value, el.line)?,
        None => (look.fill, look.gradient),
    };
    let border = match &el.line_color {
        Some(value) => border_of(value, el.line)?,
        None => look.border,
    };
    let mut stroke = Stroke::new(border, el.thickness.unwrap_or(look.thickness));
    stroke.dash = el.border_dash;
    Ok((fill, gradient, stroke))
}

/// A fill written on `line`, and the gradient it runs as where it is one
/// (`#red-green`): its plain fill is then the gradient's first color.
fn fill_of(value: &str, line: usize) -> Result<(Paint, Option<color::Gradient>), Declined> {
    color::parse_fill(value).map_err(|err| Declined::at(line, err))
}

/// A border's color written on `line`.
fn border_of(value: &str, line: usize) -> Result<Color, Declined> {
    color::parse(value)
        .map_err(|err| Declined::at(line, err))?
        .ok_or_else(|| Declined::at(line, "a border must not be transparent"))
}

/// The colors `note` was given itself, over its skin's `look`; none
/// where it was given none.
fn note_colors(
    note: &super::Note,
    look: &super::skin::Look,
) -> Result<Option<graph::open::NoteColors>, Declined> {
    if note.fill.is_none() && note.line_color.is_none() {
        return Ok(None);
    }
    let (fill, gradient) = match &note.fill {
        Some(value) => fill_of(value, note.line)?,
        None => (look.fill, look.gradient),
    };
    let border = match &note.line_color {
        Some(value) => border_of(value, note.line)?,
        None => look.border,
    };
    Ok(Some(graph::open::NoteColors {
        fill,
        gradient,
        border,
    }))
}

/// `font` moved towards black or white, whichever reads better on all of
/// `grounds` (a gradient's two ends, where the words may lie on either),
/// until it reads on each: words on a fill the writer chose.
fn legible_on(font: &mut Font, grounds: &[Color]) {
    let worst = |color: Color| {
        grounds
            .iter()
            .map(|ground| color.contrast(*ground))
            .fold(f32::INFINITY, f32::min)
    };
    let far = if worst(Color::WHITE) > worst(Color::BLACK) {
        Color::WHITE
    } else {
        Color::BLACK
    };
    for ground in grounds {
        font.color = font.color.legible_on(*ground, far, 4.5);
    }
    // A step for one end may leave another short: then all the way.
    if grounds
        .iter()
        .any(|ground| font.color.contrast(*ground) < 4.5)
    {
        font.color = far;
    }
}

/// A line's label and multiplicities, each the words its line alone
/// writes: tied to it, so a `"1"` of one line is not taken for another's.
fn link_blocks(link: &Link, skin: &Skin, family: Family) -> Result<Labeled, Declined> {
    // A hidden line says nothing: no label keeps room beside it.
    let block = |text: &Option<crate::common::Text>| match text {
        Some(text) if !link.hidden => text.measure_written(skin.arrow_font, family),
        _ => Ok(Block::empty()),
    };
    Ok(Labeled {
        label: block(&link.label)?,
        from_card: block(&link.from_card)?,
        to_card: block(&link.to_card)?,
    })
}

fn arrow_stroke(skin: &Skin, edge: &Link) -> Result<Stroke, Declined> {
    let color = match &edge.color {
        Some(value) => match color::parse(value).map_err(|err| Declined::at(edge.line, err))? {
            Some(color) => color,
            None => {
                return Err(Declined::at(
                    edge.line,
                    "an arrow color must not be transparent",
                ));
            }
        },
        None => skin.arrow.color,
    };
    let width = if edge.bold {
        skin.arrow.width * 2.0
    } else {
        skin.arrow.width
    };
    let mut stroke = Stroke::new(color, width);
    stroke.dash = match edge.dash {
        Dash::Dashed => Some((7.0, 7.0)),
        Dash::Dotted => Some((1.5, 3.0)),
        Dash::Solid => skin.arrow.dash,
    };
    Ok(stroke)
}

struct Draft {
    target: usize,
    side: Side,
    w: f32,
    h: f32,
    text: Block,
    line: usize,
    colors: Option<graph::open::NoteColors>,
}

fn note_size(text: &Block) -> (f32, f32) {
    (
        (text.width + NOTE_PAD * 2.0 + 4.0).max(18.0),
        (text.height + NOTE_PAD * 2.0).max(18.0),
    )
}

fn place_notes(drafts: &mut [Draft], rects: &[Rect], edges: &[PlacedEdge]) -> Vec<PlacedNote> {
    let mut groups: Vec<(usize, Side, Vec<usize>)> = Vec::new();
    for (index, note) in drafts.iter().enumerate() {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.0 == note.target && group.1 == note.side)
        {
            group.2.push(index);
        } else {
            groups.push((note.target, note.side, vec![index]));
        }
    }
    let mut placed = Vec::with_capacity(drafts.len());
    for (target, side, indices) in groups {
        let Some(rect) = rects.get(target) else {
            continue;
        };
        let mut cursor = match side {
            Side::Top => rect.y - NOTE_GAP,
            Side::Bottom => rect.y + rect.h + NOTE_GAP,
            Side::Left | Side::Right => rect.y,
        };
        for (nth, index) in indices.into_iter().enumerate() {
            let note = &mut drafts[index];
            let (x, y) = match side {
                Side::Right => (rect.x + rect.w + NOTE_GAP, cursor),
                Side::Left => (rect.x - NOTE_GAP - note.w, cursor),
                Side::Bottom => (rect.x + (rect.w - note.w) / 2.0, cursor),
                Side::Top => {
                    cursor -= note.h;
                    (rect.x + (rect.w - note.w) / 2.0, cursor)
                }
            };
            if side == Side::Top {
                cursor -= 6.0;
            } else {
                cursor += note.h + 6.0;
            }
            placed.push(PlacedNote {
                x,
                y,
                w: note.w,
                h: note.h,
                target,
                side,
                nth,
                text: std::mem::replace(&mut note.text, Block::empty()),
                tether: None,
                on: None,
                line: note.line,
                colors: note.colors,
            });
            nudge(placed.last_mut().expect("just pushed"), side, edges);
        }
    }
    placed
}

/// Steps a note off an edge label, away from its element.
fn nudge(note: &mut PlacedNote, side: Side, edges: &[PlacedEdge]) {
    for _ in 0..4 {
        let text = (
            note.x + NOTE_PAD,
            note.y + NOTE_PAD,
            note.text.width,
            note.text.height,
        );
        let hit = edges
            .iter()
            .any(|edge| !edge.label.is_empty() && overlap(text, label_box(edge)));
        if !hit {
            break;
        }
        match side {
            Side::Left => note.x -= 8.0,
            Side::Right => note.x += 8.0,
            Side::Top => note.y -= 8.0,
            Side::Bottom => note.y += 8.0,
        }
    }
}

fn overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    let (x, y, w, h) = a;
    let (ox, oy, ow, oh) = b;
    x < ox + ow && ox < x + w && y < oy + oh && oy < y + h
}

fn label_box(edge: &PlacedEdge) -> (f32, f32, f32, f32) {
    (
        edge.label_at.0,
        edge.label_at.1,
        edge.label.width,
        edge.label.height,
    )
}

fn graph_direction(direction: Direction) -> graph::Direction {
    match direction {
        Direction::Down => graph::Direction::Down,
        Direction::Right => graph::Direction::Right,
    }
}

fn link_crossings(
    root: &Group,
    scene: &mut Scene,
    skin: &Skin,
    family: Family,
) -> Result<(), Declined> {
    let mut found = Vec::new();
    gather(root, &mut Vec::new(), &mut found);
    // The lines laid out with the groups around them are drawn already.
    found.retain(|(path, slot, _)| {
        scene
            .scene_at(path)
            .is_some_and(|host| host.edges.get(*slot).is_some_and(|e| e.points.len() < 2))
    });
    if found.is_empty() {
        return Ok(());
    }
    let mut labels = Vec::with_capacity(found.len());
    let mut links = Vec::with_capacity(found.len());
    for (path, _, crossing) in &found {
        let blocks = link_blocks(&crossing.arrow, skin, family)?;
        links.push(across::Link {
            from: [path.as_slice(), crossing.from.as_slice()].concat(),
            to: [path.as_slice(), crossing.to.as_slice()].concat(),
            label: (blocks.label.width, blocks.label.height),
            shape: None,
            stand: [None; 2],
            sealed: [false; 2],
        });
        labels.push(blocks);
    }
    let top = placements(&scene.nodes);
    let mut drawn = Drawn::default();
    drawn_in(scene, &mut drawn);
    let routed = across::link(&top, &links, &drawn);
    for (((path, slot, crossing), blocks), linked) in found.into_iter().zip(labels).zip(routed) {
        let Some(linked) = linked else {
            return Err(Declined::at(
                crossing.arrow.line,
                "no way was found for this line between the elements",
            ));
        };
        let Some(host) = scene.scene_at(&path) else {
            return Err(Declined::at(
                crossing.arrow.line,
                "no way was found for this line between the elements",
            ));
        };
        let Some(edge) = host.edges.get_mut(slot) else {
            continue;
        };
        *edge = PlacedEdge {
            points: linked.points,
            label: blocks.label,
            label_at: linked.label_at,
            look: Look::of(
                &crossing.arrow,
                blocks.from_card,
                blocks.to_card,
                arrow_stroke(skin, &crossing.arrow)?,
            ),
        };
    }
    Ok(())
}

/// Every crossing, with the path to its group and its place among the
/// group's edges, which come first.
fn gather<'a>(
    group: &'a Group,
    path: &mut Vec<usize>,
    out: &mut Vec<(Vec<usize>, usize, &'a Crossing)>,
) {
    for (index, crossing) in group.crossings.iter().enumerate() {
        out.push((path.clone(), group.edges.len() + index, crossing));
    }
    for (index, node) in group.nodes.iter().enumerate() {
        path.push(index);
        gather(&node.children, path, out);
        path.pop();
    }
}

fn placements(nodes: &[Placed]) -> Vec<Placement> {
    nodes
        .iter()
        .map(|node| {
            let frame = node.item.composite;
            let title = (frame && node.item.banner > 0.0).then(|| name_band(node));
            Placement {
                rect: node.rect,
                flat: is_flat(node.item.shape),
                frame,
                title,
                children: if node.item.composite {
                    placements(&node.children.nodes)
                } else {
                    Vec::new()
                },
            }
        })
        .collect()
}

/// Where a container's name is, which lines keep off: a package's tab,
/// or the middle of the band over the others. The rest of the band is
/// open, so a line comes in from above where its end is, as in PlantUML.
fn name_band(node: &Placed) -> Rect {
    let rect = node.rect;
    let name = header_width(node);
    match node.item.shape {
        Shape::Package | Shape::Folder => Rect {
            w: tab_width(rect.w, name),
            h: node.item.banner,
            ..rect
        },
        _ => {
            let (area, _) = text_area(node);
            let w = (name + PAD_X * 2.0).min(area.w);
            Rect {
                x: area.x + (area.w - w) / 2.0,
                y: rect.y,
                w,
                h: node.item.banner,
            }
        }
    }
}

fn is_flat(shape: Shape) -> bool {
    !matches!(
        shape,
        Shape::UseCase
            | Shape::Actor
            | Shape::Database
            | Shape::Storage
            | Shape::Hexagon
            | Shape::Lollipop
            | Shape::Point
    )
}

fn drawn_in(scene: &Scene, drawn: &mut Drawn) {
    for edge in &scene.edges {
        if edge.points.len() < 2 {
            continue;
        }
        drawn.lines.push(edge.points.clone());
        if !edge.label.is_empty() {
            drawn.labels.push(Rect {
                x: edge.label_at.0,
                y: edge.label_at.1,
                w: edge.label.width,
                h: edge.label.height,
            });
        }
    }
    for note in &scene.notes {
        drawn.notes.push(Rect {
            x: note.x,
            y: note.y,
            w: note.w,
            h: note.h,
        });
    }
    for node in &scene.nodes {
        drawn_in(&node.children, drawn);
    }
}

fn paint(canvas: &mut Canvas, skin: &Skin, scene: &Scene, names: &Names) {
    paint_shapes(canvas, skin, scene, (&mut Vec::new(), names));
    paint_edges(canvas, scene, skin.curve, (&mut Vec::new(), names));
    paint_notes(canvas, skin, scene, (&mut Vec::new(), names));
    paint_labels(canvas, scene, &containers(scene), (&mut Vec::new(), names));
    paint_heads(canvas, skin, scene, (&mut Vec::new(), names));
}

/// Each element in its part (docs/adr/0029): its shape and its words;
/// `path` is where `scene` is in the tree.
fn paint_shapes(
    canvas: &mut Canvas,
    skin: &Skin,
    scene: &Scene,
    (path, names): (&mut Vec<usize>, &Names),
) {
    for (index, node) in scene.nodes.iter().enumerate() {
        path.push(index);
        // An association class's point is drawn as nothing: the lines
        // that meet there are.
        if !node.item.hidden && node.item.shape != Shape::Point {
            canvas.part(&names.node(path), |canvas| {
                paint_shape(canvas, skin, node);
            });
        }
        if node.item.composite {
            paint_shapes(canvas, skin, &node.children, (path, names));
        }
        path.pop();
    }
}

fn paint_shape(canvas: &mut Canvas, skin: &Skin, node: &Placed) {
    canvas.named(&node.item.name, |canvas| {
        canvas.linked(
            node.item.link.as_ref(),
            &[Layer::Frames, Layer::Front],
            |canvas| paint_box(canvas, skin, node),
        );
    });
}

/// An element's shape and its words.
fn paint_box(canvas: &mut Canvas, skin: &Skin, node: &Placed) {
    let rect = node.rect;
    let round = skin.round.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0);
    let stroke = Some(node.item.border);
    // The gradient fills the shape alone, over its box as a whole, so a
    // shape of several parts (a node's faces) is filled as one; its
    // words and its border keep their colors.
    let span = (rect.x, rect.y, rect.w, rect.h);
    canvas.gradient_over(node.item.gradient, span, |canvas| match node.item.shape {
        Shape::UseCase => {
            canvas.ellipse(
                Layer::Frames,
                (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0),
                (rect.w / 2.0, rect.h / 2.0),
                node.item.fill,
                stroke,
            );
        }
        Shape::Actor => paint_actor(canvas, rect, node.item.fill, node.item.border),
        Shape::Lollipop => paint_lollipop(canvas, rect, node.item.fill, node.item.border),
        Shape::Point => {}
        Shape::Node => paint_node(canvas, rect, node.item.fill, node.item.border),
        Shape::Database | Shape::Storage => {
            paint_cylinder(canvas, rect, node.item.fill, node.item.border)
        }
        Shape::Cloud => {
            canvas.path(
                Layer::Frames,
                &cloud_path(rect),
                (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h),
                node.item.fill,
                stroke,
            );
        }
        Shape::Hexagon => {
            canvas.polygon(Layer::Frames, &hexagon(rect), node.item.fill, stroke);
        }
        Shape::Package | Shape::Folder => {
            paint_tab(
                canvas,
                rect,
                node.item.banner,
                header_width(node),
                round,
                node.item.fill,
                node.item.border,
            );
        }
        Shape::Stack => paint_stack(canvas, rect, round, node.item.fill, node.item.border),
        Shape::Agent => paint_agent(canvas, rect, round, node.item.fill, node.item.border),
        Shape::Person => shapes::paint_person(canvas, rect, node.item.fill, node.item.border),
        Shape::Queue => shapes::paint_queue(canvas, rect, node.item.fill, node.item.border),
        Shape::Note | Shape::Artifact | Shape::File => {
            dog_ear(canvas, rect, node.item.fill, node.item.border);
        }
        Shape::Class
        | Shape::Abstract
        | Shape::Interface
        | Shape::Enum
        | Shape::Entity
        | Shape::Annotation
        | Shape::Object
        | Shape::Component
        | Shape::Rectangle
        | Shape::Frame
        | Shape::Card => {
            canvas.rect(
                Layer::Frames,
                (rect.x, rect.y, rect.w, rect.h),
                round,
                node.item.fill,
                stroke,
            );
        }
    });
    if node.item.shape == Shape::Component {
        match skin.component_style {
            ComponentStyle::Uml2 => component_icon(canvas, rect, node.item.border),
            // The marks are filled as the box is, where it is.
            ComponentStyle::Uml1 => canvas.gradient_over(node.item.gradient, span, |canvas| {
                uml1_marks(canvas, rect, node.item.fill, node.item.border)
            }),
            ComponentStyle::Rectangle => {}
        }
    }
    paint_rows(canvas, node);
}

/// The outline of `shape` in `rect`, as a box of a structure diagram is
/// drawn, without its words: what a network diagram's servers take. A
/// package's tab is half its width.
pub(crate) fn paint_outline(
    canvas: &mut Canvas,
    shape: Shape,
    rect: Rect,
    round: f32,
    fill: Paint,
    stroke: Stroke,
) {
    let round = round.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0);
    match shape {
        Shape::UseCase => canvas.ellipse(
            Layer::Frames,
            (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0),
            (rect.w / 2.0, rect.h / 2.0),
            fill,
            Some(stroke),
        ),
        Shape::Actor => paint_actor(canvas, rect, fill, stroke),
        Shape::Lollipop => paint_lollipop(canvas, rect, fill, stroke),
        Shape::Node => paint_node(canvas, rect, fill, stroke),
        Shape::Database | Shape::Storage => paint_cylinder(canvas, rect, fill, stroke),
        Shape::Cloud => canvas.path(
            Layer::Frames,
            &cloud_path(rect),
            (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h),
            fill,
            Some(stroke),
        ),
        Shape::Hexagon => canvas.polygon(Layer::Frames, &hexagon(rect), fill, Some(stroke)),
        Shape::Package | Shape::Folder => {
            paint_tab(canvas, rect, 0.0, rect.w / 2.0, round, fill, stroke);
        }
        Shape::Stack => paint_stack(canvas, rect, round, fill, stroke),
        Shape::Agent => paint_agent(canvas, rect, round, fill, stroke),
        Shape::Person => shapes::paint_person(canvas, rect, fill, stroke),
        Shape::Queue => shapes::paint_queue(canvas, rect, fill, stroke),
        Shape::Note | Shape::Artifact | Shape::File => dog_ear(canvas, rect, fill, stroke),
        _ => {
            canvas.rect(
                Layer::Frames,
                (rect.x, rect.y, rect.w, rect.h),
                round,
                fill,
                Some(stroke),
            );
            if shape == Shape::Component {
                component_icon(canvas, rect, stroke);
            }
        }
    }
}

/// Where the words of `shape` stand in `rect`: below an actor's figure,
/// in a person's body, between a cylinder's caps.
pub(crate) fn words_area(shape: Shape, rect: Rect) -> Rect {
    match shape {
        Shape::Actor => Rect {
            y: rect.y + ACTOR_FIGURE,
            h: (rect.h - ACTOR_FIGURE).max(1.0),
            ..rect
        },
        Shape::Lollipop => Rect {
            y: rect.y + LOLLIPOP + LOLLIPOP_GAP,
            h: (rect.h - LOLLIPOP - LOLLIPOP_GAP).max(1.0),
            ..rect
        },
        Shape::Person => shapes::body_of(rect),
        Shape::Queue => shapes::queue_area(rect),
        Shape::Database | Shape::Storage => {
            let ry = CAP.min(rect.h * 0.22).max(1.0);
            Rect {
                y: rect.y + 2.0 * ry,
                h: (rect.h - 3.0 * ry).max(1.0),
                ..rect
            }
        }
        _ => rect,
    }
}

/// The height of an actor's stick figure above its words.
pub(crate) const ACTOR_FIGURE: f32 = 46.0;

fn paint_actor(canvas: &mut Canvas, rect: Rect, fill: Paint, stroke: Stroke) {
    let cx = rect.x + rect.w / 2.0;
    let head_r = 7.0;
    let head_y = rect.y + head_r + 1.0;
    canvas.ellipse(
        Layer::Frames,
        (cx, head_y),
        (head_r, head_r),
        fill,
        Some(stroke),
    );
    let neck = head_y + head_r;
    let hip = neck + 16.0;
    canvas.line(Layer::Frames, (cx, neck), (cx, hip), stroke);
    canvas.line(
        Layer::Frames,
        (cx - 11.0, neck + 6.0),
        (cx + 11.0, neck + 6.0),
        stroke,
    );
    canvas.line(Layer::Frames, (cx, hip), (cx - 10.0, hip + 14.0), stroke);
    canvas.line(Layer::Frames, (cx, hip), (cx + 10.0, hip + 14.0), stroke);
}

/// A lollipop's circle at the top of `rect`, its name under it.
fn paint_lollipop(canvas: &mut Canvas, rect: Rect, fill: Paint, stroke: Stroke) {
    let r = rect.w.min(rect.h) / 2.0;
    canvas.ellipse(
        Layer::Frames,
        (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0),
        (r, r),
        fill,
        Some(stroke),
    );
}

/// Where a lollipop's name stands: beside its circle, or under it.
fn lollipop_name(node: &Placed) -> Rect {
    let rect = node.rect;
    let rows = &node.item.rows;
    let w = rows.iter().map(Row::width).fold(0.0, f32::max);
    let h: f32 = rows.iter().map(Row::height).sum();
    if node.item.beside {
        Rect {
            x: rect.x + rect.w + LOLLIPOP_GAP,
            y: rect.y + (rect.h - h) / 2.0,
            w,
            h,
        }
    } else {
        Rect {
            x: rect.x + (rect.w - w) / 2.0,
            y: rect.y + rect.h + LOLLIPOP_GAP,
            w,
            h,
        }
    }
}

/// The room a lollipop's name takes across the flow `direction`: beside
/// the circle, `rows` wide, or under it, as high.
pub(super) fn lollipop_room(rows: (f32, f32), direction: Direction) -> f32 {
    LOLLIPOP_GAP
        + match direction {
            Direction::Down => rows.0,
            Direction::Right => rows.1,
        }
}

fn paint_node(canvas: &mut Canvas, rect: Rect, fill: Paint, stroke: Stroke) {
    let d = DEPTH.min(rect.w * 0.35).min(rect.h * 0.35).max(0.0);
    let front = Rect {
        x: rect.x,
        y: rect.y + d,
        w: (rect.w - d).max(1.0),
        h: (rect.h - d).max(1.0),
    };
    let top = [
        (front.x, front.y),
        (front.x + d, rect.y),
        (front.x + front.w + d, rect.y),
        (front.x + front.w, front.y),
    ];
    let side = [
        (front.x + front.w, front.y),
        (front.x + front.w + d, rect.y),
        (front.x + front.w + d, rect.y + front.h),
        (front.x + front.w, front.y + front.h),
    ];
    canvas.polygon(Layer::Frames, &top, fill, Some(stroke));
    canvas.polygon(Layer::Frames, &side, fill, Some(stroke));
    canvas.rect(
        Layer::Frames,
        (front.x, front.y, front.w, front.h),
        0.0,
        fill,
        Some(stroke),
    );
}

fn paint_cylinder(canvas: &mut Canvas, rect: Rect, fill: Paint, stroke: Stroke) {
    let ry = CAP.min(rect.h * 0.22).max(1.0);
    let rx = rect.w / 2.0;
    let cx = rect.x + rx;
    let top = rect.y + ry;
    let bot = rect.y + rect.h - ry;
    canvas.ellipse(Layer::Frames, (cx, bot), (rx, ry), fill, Some(stroke));
    if bot > top {
        // The wall between the caps keeps its form, so the lower rim stays visible.
        canvas.plain(|canvas| {
            canvas.rect(
                Layer::Frames,
                (rect.x, top, rect.w, bot - top),
                0.0,
                fill,
                None,
            );
        });
        canvas.line(Layer::Frames, (rect.x, top), (rect.x, bot), stroke);
        canvas.line(
            Layer::Frames,
            (rect.x + rect.w, top),
            (rect.x + rect.w, bot),
            stroke,
        );
    }
    canvas.ellipse(Layer::Frames, (cx, top), (rx, ry), fill, Some(stroke));
}

/// A package or a folder as one outline. Two rectangles left the tab a
/// pixel over the body, and a sketch drew the shared edge twice.
fn paint_tab(
    canvas: &mut Canvas,
    rect: Rect,
    banner: f32,
    label_w: f32,
    round: f32,
    fill: Paint,
    stroke: Stroke,
) {
    let banner = if banner > 1.0 {
        banner.min(rect.h - 1.0).max(1.0)
    } else {
        (rect.h * 0.4).max(1.0)
    };
    let tab_w = tab_width(rect.w, label_w);
    if tab_w >= rect.w - 0.5 {
        canvas.rect(
            Layer::Frames,
            (rect.x, rect.y, rect.w, rect.h),
            round,
            fill,
            Some(stroke),
        );
        return;
    }
    let body = (rect.h - banner).max(1.0);
    let r = round
        .min(banner / 2.0)
        .min(body / 2.0)
        .min(tab_w / 2.0)
        .min((rect.w - tab_w) / 2.0)
        .max(0.0);
    if r < 0.5 {
        let top = rect.y + banner;
        canvas.polygon(
            Layer::Frames,
            &[
                (rect.x + tab_w, top),
                (rect.x + tab_w, rect.y),
                (rect.x, rect.y),
                (rect.x, rect.y + rect.h),
                (rect.x + rect.w, rect.y + rect.h),
                (rect.x + rect.w, top),
            ],
            fill,
            Some(stroke),
        );
        return;
    }
    canvas.path(
        Layer::Frames,
        &tab_path(rect, tab_w, banner, r),
        (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h),
        fill,
        Some(stroke),
    );
}

/// The tab and the body in one rounded outline. The step where the tab
/// meets the body stays sharp; only the outer corners turn.
fn tab_path(rect: Rect, tab_w: f32, banner: f32, r: f32) -> String {
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    let top = y + banner;
    let right = x + w;
    let bottom = y + h;
    let tab = x + tab_w;
    let mut d = format!("M{},{}", n(x + r), n(y));
    let corner = |d: &mut String, ex: f32, ey: f32| {
        let _ = write!(d, "A{},{} 0 0 1 {},{}", n(r), n(r), n(ex), n(ey));
    };
    let _ = write!(d, "L{},{}", n(tab - r), n(y));
    corner(&mut d, tab, y + r);
    let _ = write!(d, "L{},{}L{},{}", n(tab), n(top), n(right - r), n(top));
    corner(&mut d, right, top + r);
    let _ = write!(d, "L{},{}", n(right), n(bottom - r));
    corner(&mut d, right - r, bottom);
    let _ = write!(d, "L{},{}", n(x + r), n(bottom));
    corner(&mut d, x, bottom - r);
    let _ = write!(d, "L{},{}", n(x), n(y + r));
    corner(&mut d, x + r, y);
    d.push('Z');
    d
}

/// The name band of a package or folder. Members, when a class has them,
/// start after it.
fn header_width(node: &Placed) -> f32 {
    let n = if node.item.body == 0 {
        node.item.rows.len()
    } else {
        node.item.body
    };
    node.item
        .rows
        .iter()
        .take(n)
        .map(Row::width)
        .fold(0.0, f32::max)
}

/// Wide enough for the name, and never wider than the box.
fn tab_width(box_w: f32, label_w: f32) -> f32 {
    let wanted = label_w + PAD_X * 2.0;
    if box_w <= wanted {
        return box_w.max(1.0);
    }
    wanted.max(28.0)
}

fn paint_stack(canvas: &mut Canvas, rect: Rect, round: f32, fill: Paint, stroke: Stroke) {
    let step = STACK;
    let front = Rect {
        x: rect.x,
        y: rect.y + step * 2.0,
        w: (rect.w - step * 2.0).max(1.0),
        h: (rect.h - step * 2.0).max(1.0),
    };
    for i in (1..=2).rev() {
        let o = i as f32 * step;
        canvas.rect(
            Layer::Frames,
            (front.x + o, front.y - o, front.w, front.h),
            round,
            fill,
            Some(stroke),
        );
    }
    canvas.rect(
        Layer::Frames,
        (front.x, front.y, front.w, front.h),
        round,
        fill,
        Some(stroke),
    );
}

fn paint_agent(canvas: &mut Canvas, rect: Rect, round: f32, fill: Paint, stroke: Stroke) {
    let head = 8.0;
    let body_h = (rect.h - head).max(1.0);
    canvas.rect(
        Layer::Frames,
        (rect.x, rect.y + head, rect.w, body_h),
        round,
        fill,
        Some(stroke),
    );
    canvas.ellipse(
        Layer::Frames,
        (rect.x + rect.w / 2.0, rect.y + head),
        (head, head),
        fill,
        Some(stroke),
    );
}

fn component_icon(canvas: &mut Canvas, rect: Rect, stroke: Stroke) {
    let w = ICON_W;
    let h = 4.5;
    let x = rect.x + rect.w - w - ICON_EDGE;
    let y = rect.y + 5.0;
    canvas.rect(Layer::Frames, (x, y, w, h), 0.0, None, Some(stroke));
    canvas.rect(
        Layer::Frames,
        (x, y + h + 2.0, w, h),
        0.0,
        None,
        Some(stroke),
    );
}

/// A UML 1 component's two small boxes across its left edge.
fn uml1_marks(canvas: &mut Canvas, rect: Rect, fill: Paint, stroke: Stroke) {
    let (w, h) = (ICON_W, 5.0);
    let x = rect.x - w / 2.0;
    for y in [rect.y + 8.0, rect.y + 8.0 + h + 4.0] {
        canvas.rect(Layer::Frames, (x, y, w, h), 0.0, fill, Some(stroke));
    }
}

fn dog_ear(canvas: &mut Canvas, rect: Rect, fill: Paint, stroke: Stroke) {
    let fold = 12.0_f32.min(rect.w / 3.0).min(rect.h / 3.0).max(2.0);
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    let body = format!(
        "M{},{} L{},{} L{},{} L{},{} L{},{} Z",
        n(x),
        n(y),
        n(x),
        n(y + h),
        n(x + w),
        n(y + h),
        n(x + w),
        n(y + fold),
        n(x + w - fold),
        n(y)
    );
    canvas.path(
        Layer::Frames,
        &body,
        (x, y, x + w, y + h),
        fill,
        Some(stroke),
    );
    let corner = format!(
        "M{},{} L{},{} L{},{}",
        n(x + w - fold),
        n(y),
        n(x + w - fold),
        n(y + fold),
        n(x + w),
        n(y + fold)
    );
    canvas.path(
        Layer::Frames,
        &corner,
        (x, y, x + w, y + h),
        fill,
        Some(stroke),
    );
}

/// A cloud: the box with a scalloped edge, bumps all round it whose
/// tops touch the box and whose dips stand [`CLOUD_DIP`] inside, as
/// PlantUML draws one, so what it holds stays clear of its edge. A cloud
/// stretched from one outline left its words and boxes poking out of the
/// dips.
fn cloud_path(rect: Rect) -> String {
    let d = CLOUD_DIP.min(rect.w / 4.0).min(rect.h / 4.0);
    let (x0, y0, x1, y1) = (
        rect.x + d,
        rect.y + d,
        rect.x + rect.w - d,
        rect.y + rect.h - d,
    );
    // A cubic bump reaches 3/4 of its handles' offset.
    let reach = d / 0.75;
    let bumps = |len: f32| ((len / CLOUD_BUMP).round() as usize).max(2);
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    // Outward along each side: up, right, down, left.
    let outward = [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)];
    let mut d_path = format!("M{},{}", n(x0), n(y0));
    for side in 0..4 {
        let (from, to) = (corners[side], corners[(side + 1) % 4]);
        let (ox, oy) = (outward[side].0 * reach, outward[side].1 * reach);
        let count = bumps((to.0 - from.0).abs() + (to.1 - from.1).abs());
        for k in 0..count {
            let at = |t: f32| (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
            let (a, b) = (
                at(k as f32 / count as f32),
                at((k + 1) as f32 / count as f32),
            );
            let _ = write!(
                d_path,
                " C{},{} {},{} {},{}",
                n(a.0 + ox),
                n(a.1 + oy),
                n(b.0 + ox),
                n(b.1 + oy),
                n(b.0),
                n(b.1)
            );
        }
    }
    d_path.push('Z');
    d_path
}

fn hexagon(rect: Rect) -> [(f32, f32); 6] {
    let inset = (rect.w * 0.22).min(18.0);
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    [
        (x + inset, y),
        (x + w - inset, y),
        (x + w, y + h / 2.0),
        (x + w - inset, y + h),
        (x + inset, y + h),
        (x, y + h / 2.0),
    ]
}

enum Place {
    Top,
    Center,
}

fn paint_rows(canvas: &mut Canvas, node: &Placed) {
    if matches!(node.item.shape, Shape::Package | Shape::Folder) && !node.item.composite {
        paint_tabbed(canvas, node);
        return;
    }
    let (area, place) = text_area(node);
    let pad_y = if node.item.shape == Shape::Note {
        NOTE_PAD
    } else {
        PAD_Y
    };
    let pad_x = if node.item.shape == Shape::Note {
        NOTE_PAD
    } else {
        PAD_X
    };
    let extra = if node.item.body < node.item.rows.len()
        || (node.item.shape == Shape::Card && !node.item.rows.is_empty())
    {
        RULE
    } else {
        0.0
    };
    let content = node.item.rows.iter().map(Row::height).sum::<f32>() + extra;
    let mut y = match place {
        Place::Top => area.y + pad_y,
        Place::Center => area.y + (area.h - content) / 2.0,
    };
    let header = node.item.body.min(node.item.rows.len());
    for row in &node.item.rows[..header] {
        paint_row(canvas, area, pad_x, y, row, node.item.shape);
        y += row.height();
    }
    if extra > 0.0 {
        let line_y = y + extra / 2.0;
        canvas.line(
            Layer::Frames,
            (area.x, line_y),
            (area.x + area.w, line_y),
            Stroke::new(node.item.border.color, node.item.border.width.max(0.5)),
        );
        y += extra;
    }
    for row in &node.item.rows[header..] {
        match row {
            Row::Rule(dash) => {
                let line_y = y + RULE / 2.0;
                let mut stroke =
                    Stroke::new(node.item.border.color, node.item.border.width.max(0.5));
                if *dash != Dash::Solid {
                    stroke.dash = Some(if *dash == Dash::Dotted {
                        (1.5, 3.0)
                    } else {
                        (5.0, 4.0)
                    });
                }
                canvas.line(
                    Layer::Frames,
                    (area.x, line_y),
                    (area.x + area.w, line_y),
                    stroke,
                );
                y += RULE;
            }
            other => {
                paint_row(canvas, area, pad_x, y, other, node.item.shape);
                y += other.height();
            }
        }
    }
}

/// A package holding nothing: its name in the middle of the tab, the rest
/// (its stereotypes, what a fold says it holds) in the middle of the body.
fn paint_tabbed(canvas: &mut Canvas, node: &Placed) {
    let (tab, _) = text_area(node);
    let rect = node.rect;
    let header = node.item.body.min(node.item.rows.len());
    let (name, rest) = node.item.rows.split_at(header);
    let body = Rect {
        x: rect.x,
        y: tab.y + tab.h,
        w: rect.w,
        h: (rect.y + rect.h - tab.y - tab.h).max(1.0),
    };
    for (area, rows) in [(tab, name), (body, rest)] {
        let content = rows.iter().map(Row::height).sum::<f32>();
        let mut y = area.y + (area.h - content) / 2.0;
        for row in rows {
            paint_row(canvas, area, PAD_X, y, row, node.item.shape);
            y += row.height();
        }
    }
}

fn paint_row(canvas: &mut Canvas, area: Rect, pad_x: f32, y: f32, row: &Row, shape: Shape) {
    let Some(block) = row.block() else {
        return;
    };
    if shape == Shape::Note {
        canvas.text(Layer::Front, block, (area.x + pad_x, y), Align::Left);
        return;
    }
    match row {
        Row::Left(_) => canvas.text(Layer::Front, block, (area.x + pad_x, y), Align::Left),
        Row::Center(_) => {
            canvas.text(
                Layer::Front,
                block,
                (area.x + area.w / 2.0, y),
                Align::Center,
            );
        }
        Row::Rule(_) => {}
    }
}

fn text_area(node: &Placed) -> (Rect, Place) {
    let rect = node.rect;
    let place = if node.item.composite {
        Place::Top
    } else {
        match node.item.shape {
            Shape::UseCase
            | Shape::Cloud
            | Shape::Hexagon
            | Shape::Database
            | Shape::Storage
            | Shape::Note
            | Shape::Person
            | Shape::Queue
            | Shape::Actor
            | Shape::Lollipop => Place::Center,
            _ => Place::Top,
        }
    };
    let area = match node.item.shape {
        Shape::Node => Rect {
            x: rect.x,
            y: rect.y + DEPTH,
            w: (rect.w - DEPTH).max(1.0),
            h: (rect.h - DEPTH).max(1.0),
        },
        Shape::Stack => {
            let extra = STACK * 2.0;
            Rect {
                x: rect.x,
                y: rect.y + extra,
                w: (rect.w - extra).max(1.0),
                h: (rect.h - extra).max(1.0),
            }
        }
        Shape::Actor => Rect {
            x: rect.x,
            y: rect.y + ACTOR_H,
            w: rect.w,
            h: (rect.h - ACTOR_H).max(1.0),
        },
        Shape::Lollipop => lollipop_name(node),
        Shape::Person => shapes::body_of(rect),
        Shape::Queue => shapes::queue_area(rect),
        Shape::Agent if !node.item.composite => Rect {
            x: rect.x,
            y: rect.y + 12.0,
            w: rect.w,
            h: (rect.h - 12.0).max(1.0),
        },
        // The name is the tab, not the middle of the whole box.
        Shape::Package | Shape::Folder => {
            let h = if node.item.banner > 1.0 {
                node.item.banner.min(rect.h).max(1.0)
            } else {
                rect.h
            };
            return (
                Rect {
                    x: rect.x,
                    y: rect.y,
                    w: tab_width(rect.w, header_width(node)),
                    h,
                },
                Place::Center,
            );
        }
        _ => rect,
    };
    (area, place)
}

/// Each line in its part (docs/adr/0037), which its words and its heads
/// are drawn in too; `path` is where `scene` is in the tree.
fn paint_edges(
    canvas: &mut Canvas,
    scene: &Scene,
    curve: f32,
    (path, names): (&mut Vec<usize>, &Names),
) {
    for (slot, edge) in scene.edges.iter().enumerate() {
        if edge.points.len() < 2 || edge.look.hidden {
            continue;
        }
        canvas.part(&names.edge(path, slot), |canvas| {
            canvas.curve(
                Layer::Lines,
                &shortened(&edge.points, edge.look.from_head, edge.look.to_head),
                curve,
                edge.look.stroke,
            );
        });
    }
    for (index, node) in scene.nodes.iter().enumerate() {
        path.push(index);
        paint_edges(canvas, &node.children, curve, (path, names));
        path.pop();
    }
}

/// The part of each of `scene`'s notes, at `path` in the tree: a part of
/// its own by its element's ([`crate::svg::note_part`]).
fn note_parts(scene: &Scene, path: &[usize], names: &Names) -> Vec<String> {
    scene.note_parts(path, |_, at| names.node(at), |slot| names.edge(path, slot))
}

/// Each note's part with the part of the element it lies inside, into
/// `out`.
fn notes_inside(scene: &Scene, names: &Names, out: &mut Vec<(String, String)>) {
    let part = |_: &Placed, at: &[usize]| Some(names.node(at));
    let edge = |at: &[usize], slot: usize| names.edge(at, slot);
    scene.notes_inside((&mut Vec::new(), None), &part, &edge, out);
}

/// Each note, in a part of its own; `path` is where `scene` is in the
/// tree.
fn paint_notes(
    canvas: &mut Canvas,
    skin: &Skin,
    scene: &Scene,
    (path, names): (&mut Vec<usize>, &Names),
) {
    for (note, part) in scene.notes.iter().zip(note_parts(scene, path, names)) {
        if on_hidden(scene, note) {
            continue;
        }
        let (fill, gradient, border) = match note.colors {
            Some(own) => (own.fill, own.gradient, own.border),
            None => (skin.note.fill, skin.note.gradient, skin.note.border),
        };
        canvas.part(&part, |canvas| {
            let span = (note.x, note.y, note.w, note.h);
            canvas.gradient_over(gradient, span, |canvas| {
                dog_ear(
                    canvas,
                    Rect {
                        x: note.x,
                        y: note.y,
                        w: note.w,
                        h: note.h,
                    },
                    fill,
                    Stroke::new(border, skin.note.thickness),
                );
            });
        });
    }
    for (index, node) in scene.nodes.iter().enumerate() {
        path.push(index);
        paint_notes(canvas, skin, &node.children, (path, names));
        path.pop();
    }
}

/// Whether `note` is on a box hidden with `hide`: it keeps its place, as
/// the box does, and is not drawn.
fn on_hidden(scene: &Scene, note: &PlacedNote) -> bool {
    scene
        .nodes
        .get(note.target)
        .is_some_and(|node| node.item.hidden)
}

/// Each line's words and each note's in a part of their own.
/// `containers` are those of the whole diagram, whose borders the
/// multiplicities keep off; `path` is where `scene` is in the tree.
fn paint_labels(
    canvas: &mut Canvas,
    scene: &Scene,
    containers: &[Rect],
    (path, names): (&mut Vec<usize>, &Names),
) {
    for (slot, edge) in scene.edges.iter().enumerate() {
        let words = names.edge(path, slot);
        canvas.part(&words, |canvas| {
            canvas.text(Layer::Front, &edge.label, edge.label_at, Align::Left);
        });
        // Each multiplicity a part of its own, by its end: it travels
        // with that end, not with the words in the middle.
        for (card, at, at_start) in ends_cards(edge, containers) {
            canvas.part(&names.card(path, slot, at_start), |canvas| {
                canvas.text(Layer::Front, card, at, Align::Left);
            });
        }
    }
    for (note, part) in scene.notes.iter().zip(note_parts(scene, path, names)) {
        if on_hidden(scene, note) {
            continue;
        }
        canvas.part(&part, |canvas| {
            canvas.text(
                Layer::Front,
                &note.text,
                (note.x + NOTE_PAD, note.y + NOTE_PAD),
                Align::Left,
            );
        });
    }
    for (index, node) in scene.nodes.iter().enumerate() {
        path.push(index);
        paint_labels(canvas, &node.children, containers, (path, names));
        path.pop();
    }
}

/// Each line's heads, in its part; `path` is where `scene` is in the tree.
fn paint_heads(
    canvas: &mut Canvas,
    skin: &Skin,
    scene: &Scene,
    (path, names): (&mut Vec<usize>, &Names),
) {
    for (slot, edge) in scene.edges.iter().enumerate() {
        if edge.points.len() < 2 {
            continue;
        }
        let last = edge.points.len() - 1;
        canvas.part(&names.edge(path, slot), |canvas| {
            draw_head(
                canvas,
                skin,
                edge.points[0],
                edge.points[1],
                edge.look.from_head,
                edge.look.stroke,
            );
            draw_head(
                canvas,
                skin,
                edge.points[last],
                edge.points[last - 1],
                edge.look.to_head,
                edge.look.stroke,
            );
        });
    }
    for (index, node) in scene.nodes.iter().enumerate() {
        path.push(index);
        paint_heads(canvas, skin, &node.children, (path, names));
        path.pop();
    }
}

fn shortened(points: &[(f32, f32)], from: Head, to: Head) -> Vec<(f32, f32)> {
    let mut line = points.to_vec();
    pull(&mut line, true, head_span(from));
    pull(&mut line, false, head_span(to));
    line
}

fn pull(points: &mut [(f32, f32)], at_start: bool, amount: f32) {
    if amount <= 0.0 || points.len() < 2 {
        return;
    }
    let (tip_i, from_i) = if at_start {
        (0, 1)
    } else {
        (points.len() - 1, points.len() - 2)
    };
    let tip = points[tip_i];
    let from = points[from_i];
    let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (vx * vx + vy * vy).sqrt();
    if len < 0.5 {
        return;
    }
    let scale = (amount / len).min(0.8);
    points[tip_i] = (tip.0 - vx * scale, tip.1 - vy * scale);
}

fn head_span(head: Head) -> f32 {
    match head {
        Head::None => 0.0,
        Head::Arrow => HEAD_LENGTH,
        Head::Triangle | Head::Diamond | Head::DiamondFilled => MARK_LENGTH,
    }
}

fn draw_head(
    canvas: &mut Canvas,
    skin: &Skin,
    tip: (f32, f32),
    from: (f32, f32),
    head: Head,
    stroke: Stroke,
) {
    let Some(shape) = head_shape(tip, from, head, skin.strict) else {
        return;
    };
    // A line in the diagram's arrow color takes the heads' own, where one
    // is set; one of its own color keeps that in its head.
    let stroke = match head_color(skin, stroke) {
        Some(color) => Stroke { color, ..stroke },
        None => stroke,
    };
    let solid = Stroke {
        dash: None,
        width: stroke.width.min(1.5),
        ..stroke
    };
    if !shape.closed {
        canvas.polyline(Layer::Lines, &shape.points, solid);
        return;
    }
    let fill = match shape.fill {
        HeadFill::Ink => Some(stroke.color),
        // An empty diamond hides the line under it in the page's color,
        // a gradient's as it runs there.
        HeadFill::Paper => {
            let paper = Some(skin.background.unwrap_or(Color::WHITE));
            canvas.paper(|canvas| {
                canvas.polygon(Layer::Lines, &shape.points, paper, Some(solid));
            });
            return;
        }
        HeadFill::None => None,
    };
    canvas.polygon(Layer::Lines, &shape.points, fill, Some(solid));
}

/// The mark `head` at `tip`, on a line coming from `from`; `strict` draws
/// an arrowhead as two strokes.
fn head_shape(tip: (f32, f32), from: (f32, f32), head: Head, strict: bool) -> Option<map::Head> {
    if head == Head::None {
        return None;
    }
    let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (vx * vx + vy * vy).sqrt();
    if len < 0.01 {
        return None;
    }
    let (ux, uy) = (vx / len, vy / len);
    let length = head_span(head);
    let half = match head {
        Head::Triangle => 7.0,
        Head::Diamond | Head::DiamondFilled => 6.0,
        _ => HEAD_HALF,
    };
    let side = (-uy * half, ux * half);
    let base = (tip.0 - ux * length, tip.1 - uy * length);
    let mid = (tip.0 - ux * length / 2.0, tip.1 - uy * length / 2.0);
    let (left, right) = if matches!(head, Head::Diamond | Head::DiamondFilled) {
        (
            (mid.0 + side.0, mid.1 + side.1),
            (mid.0 - side.0, mid.1 - side.1),
        )
    } else {
        (
            (base.0 + side.0, base.1 + side.1),
            (base.0 - side.0, base.1 - side.1),
        )
    };
    let (points, closed, fill) = match head {
        Head::None => return None,
        Head::Arrow if strict => (vec![left, tip, right], false, HeadFill::None),
        Head::Arrow => {
            let inset = (tip.0 - ux * length * 0.55, tip.1 - uy * length * 0.55);
            (vec![left, tip, right, inset], true, HeadFill::Ink)
        }
        Head::Triangle => (vec![left, tip, right], true, HeadFill::None),
        Head::Diamond => (vec![tip, left, base, right], true, HeadFill::Paper),
        Head::DiamondFilled => (vec![tip, left, base, right], true, HeadFill::Ink),
    };
    Some(map::Head {
        points,
        closed,
        fill,
        color: None,
    })
}

/// The color of a head on a line drawn with `stroke`: the heads' own on a
/// line in the diagram's arrow color, where one is set; else the line's.
fn head_color(skin: &Skin, stroke: Stroke) -> Option<Color> {
    skin.arrow_head.filter(|_| stroke.color == skin.arrow.color)
}

/// How the line at `slot` of the container at `path` is drawn, with the
/// marks at both ends of `drawn`, its points as drawn.
fn dress(
    scene: &Scene,
    (path, slot): (&[usize], usize),
    drawn: &[(f32, f32)],
    skin: &Skin,
    names: &Names,
) -> Dress {
    let mut here = scene;
    for &index in path {
        match here.nodes.get(index) {
            Some(node) => here = &node.children,
            None => break,
        }
    }
    let Some(edge) = here.edges.get(slot).filter(|_| drawn.len() >= 2) else {
        return Dress {
            words: [Some(names.edge(path, slot)), None, None],
            stroke: Stroke::new(Color::BLACK, 1.0),
            heads: Vec::new(),
        };
    };
    let last = drawn.len() - 1;
    let color = head_color(skin, edge.look.stroke);
    let heads = [
        head_shape(drawn[0], drawn[1], edge.look.from_head, skin.strict),
        head_shape(drawn[last], drawn[last - 1], edge.look.to_head, skin.strict),
    ]
    .map(|head| head.map(|head| map::Head { color, ..head }));
    let words = names.edge(path, slot);
    let card =
        |card: &Block, at_start| (!card.is_empty()).then(|| names.card(path, slot, at_start));
    Dress {
        words: [
            Some(words.clone()),
            card(&edge.look.from_card, true),
            card(&edge.look.to_card, false),
        ],
        stroke: edge.look.stroke,
        heads: heads.into_iter().flatten().collect(),
    }
}

/// The boxes of the containers in `scene`, nested ones too.
fn containers(scene: &Scene) -> Vec<Rect> {
    fn walk(scene: &Scene, out: &mut Vec<Rect>) {
        for node in &scene.nodes {
            if node.item.composite {
                out.push(node.rect);
            }
            walk(&node.children, out);
        }
    }
    let mut out = Vec::new();
    walk(scene, &mut out);
    out
}

/// The multiplicities at the ends of `edge`, where each goes, off the
/// borders of `containers`, and whether it stands at the line's start.
fn ends_cards<'a>(
    edge: &'a PlacedEdge,
    containers: &'a [Rect],
) -> impl Iterator<Item = (&'a Block, (f32, f32), bool)> + 'a {
    [
        (&edge.look.from_card, true, edge.look.from_head),
        (&edge.look.to_card, false, edge.look.to_head),
    ]
    .into_iter()
    .filter(|(card, ..)| !card.is_empty() && edge.points.len() >= 2)
    .map(move |(card, at_start, head)| {
        let along = head_span(head);
        (
            card,
            card_at(&edge.points, at_start, card, along, containers),
            at_start,
        )
    })
}

/// Where the multiplicities at the ends of `edge` are.
fn card_rects<'a>(edge: &'a PlacedEdge, containers: &'a [Rect]) -> impl Iterator<Item = Rect> + 'a {
    ends_cards(edge, containers).map(|(block, (x, y), _)| Rect {
        x,
        y,
        w: block.width,
        h: block.height,
    })
}

/// The top left of a multiplicity beside the end of `points`, `along`
/// past the head there. Beside a level run it sits close below or above
/// it; beside an upright one it stands clear of it, or the line would
/// strike it through. Across the border of one of `containers` the border
/// would: it takes the other side then, or moves on along the line, up to
/// where it would stop anyway.
fn card_at(
    points: &[(f32, f32)],
    at_start: bool,
    block: &Block,
    along: f32,
    containers: &[Rect],
) -> (f32, f32) {
    let (tip, inner) = if at_start {
        (points[0], points[1])
    } else {
        let last = points.len() - 1;
        (points[last], points[last - 1])
    };
    let (vx, vy) = (inner.0 - tip.0, inner.1 - tip.1);
    let len = (vx * vx + vy * vy).sqrt().max(0.01);
    let (ux, uy) = (vx / len, vy / len);
    let (px, py) = (-uy, ux);
    // Beside a level run it lies along the line: past the head by half
    // its own width too, or the head would stand in its words.
    let first = (along + 8.0 + py.abs() * block.width / 2.0)
        .min(len * 0.65)
        .max(6.0);
    let off = py.abs() * (block.height * 0.15 + 3.0) + px.abs() * (block.width / 2.0 + 3.0);
    let at = |travel: f32, side: f32| {
        let cx = tip.0 + ux * travel + side * px * off;
        let cy = tip.1 + uy * travel + side * py * off;
        (cx - block.width / 2.0, cy - block.height / 2.0)
    };
    let across = |(x, y): (f32, f32)| {
        let card = Rect {
            x,
            y,
            w: block.width,
            h: block.height,
        };
        containers.iter().any(|&frame| on_border(frame, card))
    };
    // A loop's other end lies beside its first run, not along it: the
    // multiplicity stands outside the loop, where the loop's words are
    // not, over the run that leaves and under the one that arrives.
    let other = if at_start {
        points[points.len() - 1]
    } else {
        points[0]
    };
    let (ox, oy) = (other.0 - tip.0, other.1 - tip.1);
    let (ahead, beside) = (ox * ux + oy * uy, ox * px + oy * py);
    let side = if ahead.abs() < 1.0 && beside.abs() > 1.0 {
        -beside.signum()
    } else {
        1.0
    };
    let last = first.max(len * 0.65);
    (0..)
        .map(|step| first + step as f32 * 4.0)
        .take_while(|&travel| travel <= last)
        .flat_map(|travel| [at(travel, side), at(travel, -side)])
        .find(|&spot| !across(spot))
        .unwrap_or_else(|| at(first, side))
}

/// Whether `rect` lies across the border of `frame`: partly in it.
fn on_border(frame: Rect, rect: Rect) -> bool {
    let meets = rect.x < frame.x + frame.w
        && frame.x < rect.x + rect.w
        && rect.y < frame.y + frame.h
        && frame.y < rect.y + rect.h;
    let inside = rect.x >= frame.x - 0.5
        && rect.y >= frame.y - 0.5
        && rect.x + rect.w <= frame.x + frame.w + 0.5
        && rect.y + rect.h <= frame.y + frame.h + 0.5;
    meets && !inside
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;

    /// A lollipop's box is its circle, so its lines end on it, whichever
    /// way the flow runs; its name stands beside it or under it.
    #[test]
    fn a_lollipop_s_lines_end_on_its_circle() {
        for source in [
            "@startuml\n() \"Clipboard service\" --> [Editor]\n@enduml\n",
            "@startuml\nleft to right direction\n() \"Clipboard service\" --> [Editor]\n@enduml\n",
        ] {
            let (svg, map) = crate::render_mapped(source, None).unwrap();
            let circle = map
                .places
                .iter()
                .find(|place| place.name.contains("Clipboard"))
                .expect("the circle's place");
            assert!((circle.rect.w - LOLLIPOP).abs() < 0.5, "{:?}", circle.rect);
            let start = map.wires[0].points[0];
            let (cx, cy) = (
                circle.rect.x + circle.rect.w / 2.0,
                circle.rect.y + circle.rect.h / 2.0,
            );
            let off = ((start.0 - cx).powi(2) + (start.1 - cy).powi(2)).sqrt();
            assert!(
                off <= LOLLIPOP / 2.0 + 1.0,
                "{source}: {start:?} from ({cx}, {cy})"
            );
            assert!(svg.contains(">Clipboard service<"));
        }
    }

    /// `packageStyle` changes a package's outline, not its look.
    #[test]
    fn a_package_drawn_as_a_rectangle_keeps_its_colors() {
        let svg = crate::render(
            "@startuml\nskinparam packageStyle rectangle\nskinparam packageBackgroundColor #FF0000\npackage Shop {\n  [Web]\n}\n@enduml\n",
        )
        .unwrap();
        assert!(svg.contains("fill=\"#FF0000\""), "{svg}");
    }

    /// A box's own fill may run from one color into another, as PlantUML
    /// writes it: the shape alone is filled with it, over its box as a
    /// whole, each box's gradient defined once. A border does not take one,
    /// nor a color PlantUML does not know.
    #[test]
    fn a_gradient_fills_a_box_s_shape() {
        let svg = crate::render(
            "@startuml\nclass Foo #red-green\nclass Bar #red-green\npackage P #white/lightblue {\n  component C #Lavender|LightBlue\n}\nusecase U #yellow\\orange\nnode N #red-green;line:blue\n@enduml\n",
        )
        .unwrap();
        // Each box has its own, over its own box: the node's faces share one.
        for (name, boxes, filled) in [
            ("omaspec-gradient-ff0000-008000-down-", 3, 2 + 3),
            ("omaspec-gradient-ffffff-add8e6-falling-", 1, 1),
            ("omaspec-gradient-e6e6fa-add8e6-across-", 1, 1),
            ("omaspec-gradient-ffff00-ffa500-rising-", 1, 1),
        ] {
            assert_eq!(
                svg.matches(&format!("id=\"{name}")).count(),
                boxes,
                "{name}: {svg}"
            );
            let fill = format!("fill=\"url(#{name}");
            assert_eq!(svg.matches(&fill).count(), filled, "{name}: {svg}");
        }
        assert_eq!(
            svg.matches(r#"gradientUnits="userSpaceOnUse""#).count(),
            6,
            "{svg}"
        );
        // A gradient in an inline style, beside its border.
        assert!(svg.contains(r##"stroke="#0000FF""##), "{svg}");
        for (source, reason) in [
            ("class A ##red-green", "a gradient as a border"),
            ("class A #red-nocolor", "`nocolor` in the gradient"),
        ] {
            let err = crate::render(&format!("@startuml\n{source}\n@enduml\n")).unwrap_err();
            assert!(err.reason.contains(reason), "{source}: {}", err.reason);
            assert_eq!(err.line, Some(1), "{source}");
        }
    }

    /// A skinparam's fill and the page's may run as gradients too, and a
    /// note takes colors of its own, a gradient among them, as the guide's
    /// example of them writes them; its words are kept legible on them.
    #[test]
    fn gradients_of_the_skin_the_page_and_a_note() {
        let svg = crate::render(
            "@startuml\nskinparam backgroundcolor AntiqueWhite/Gold\nskinparam classBackgroundColor Wheat|CornflowerBlue\nclass Foo\nnote left of Foo #blue\\9932CC\n  dark\nend note\nnote right of Foo #pink ##red : light\n@enduml\n",
        )
        .unwrap();
        assert!(
            svg.contains(r#"fill="url(#omaspec-gradient-faebd7-ffd700-falling)"/>"#),
            "the page: {svg}"
        );
        for name in [
            "omaspec-gradient-f5deb3-6495ed-across-",
            "omaspec-gradient-0000ff-9932cc-rising-",
        ] {
            assert!(
                svg.contains(&format!("fill=\"url(#{name}")),
                "{name}: {svg}"
            );
        }
        assert!(
            svg.contains(r##"fill="#FFC0CB" stroke="#FF0000""##),
            "{svg}"
        );
        // The dark note's words are light; the pink one's stay dark.
        let words = |word: &str| {
            let at = svg.find(&format!(">{word}<")).expect(word);
            let open = svg[..at].rfind("<text").expect("its text");
            svg[open..at].to_owned()
        };
        assert!(
            !words("dark").contains("fill=\"#000000\""),
            "{}",
            words("dark")
        );
        assert!(
            !words("light").contains("fill=\"#FFFFFF\""),
            "{}",
            words("light")
        );
        let err = crate::render(
            "@startuml\nclass Foo\nnote left of Foo #pink;line.dashed : x\n@enduml\n",
        )
        .unwrap_err();
        assert_eq!(err.line, Some(2));
        assert!(err.reason.contains("a style on a note"), "{}", err.reason);
        // A `;` after the words' `:` is the words'.
        let svg =
            crate::render("@startuml\nclass Foo\nnote left of Foo #red:wait;retry\n@enduml\n")
                .unwrap();
        assert!(svg.contains(">wait;retry<"), "{svg}");
        // A note's gradient in an inline style; a `#` in a name is the name's.
        let svg = crate::render(
            "@startuml\nclass \"C#\" as C\nnote left of C #red-green;line:blue : x\n@enduml\n",
        )
        .unwrap();
        assert!(
            svg.contains("url(#omaspec-gradient-ff0000-008000-down-"),
            "{svg}"
        );
        assert!(svg.contains(r##"stroke="#0000FF""##), "{svg}");
    }

    /// An empty diamond hides the line under it in the page's color: on a
    /// page that runs as a gradient, in the gradient as it runs there,
    /// laid over the page as the drawing's coordinates place it.
    #[test]
    fn an_empty_diamond_takes_the_page_s_gradient() {
        let svg = crate::render(
            "@startuml\nskinparam backgroundcolor AntiqueWhite/Gold\nclass A\nclass B\nA o-- B\n@enduml\n",
        )
        .unwrap();
        let paper = "omaspec-gradient-faebd7-ffd700-falling-page";
        assert!(
            svg.contains(&format!(
                r#"<linearGradient id="{paper}" gradientUnits="userSpaceOnUse""#
            )),
            "{svg}"
        );
        let diamond = svg
            .split("<polygon")
            .find(|shape| shape.contains(&format!("url(#{paper})")))
            .expect("the diamond");
        assert_eq!(
            diamond
                .split_whitespace()
                .next()
                .map(|p| p.matches(',').count()),
            Some(1)
        );
        // A picture inside a note has no page of its own: its diamond takes
        // the first color, and the page's gradient is defined once.
        let svg = crate::render(
            "@startuml\nskinparam backgroundcolor AntiqueWhite/Gold\nclass A\nclass B\nA o-- B\nnote right of A\n{{\nskinparam backgroundcolor AntiqueWhite/Gold\nclass X\nclass Y\nX o-- Y\n}}\nend note\n@enduml\n",
        )
        .unwrap();
        assert_eq!(svg.matches(&format!("id=\"{paper}\"")).count(), 1, "{svg}");
        assert_eq!(svg.matches(&format!("url(#{paper})")).count(), 1, "{svg}");
        assert!(svg.contains(r##"fill="#FAEBD7" stroke"##), "{svg}");
    }

    /// Words on a fill read on every end of it, or are black or white.
    #[test]
    fn words_on_a_gradient_read_on_both_its_ends() {
        let ends = [
            (Color::hex(0x000080), Color::hex(0xFFFFE0)),
            (Color::hex(0x0000FF), Color::hex(0xADD8E6)),
            (Color::hex(0xFF0000), Color::hex(0xFFFF00)),
            (Color::hex(0x333333), Color::hex(0x999999)),
        ];
        for (from, to) in ends {
            let mut font = Font::new(14.0, Color::hex(0x808080));
            legible_on(&mut font, &[from, to]);
            let reads = [from, to].iter().all(|g| font.color.contrast(*g) >= 4.5);
            assert!(
                reads || font.color == Color::WHITE || font.color == Color::BLACK,
                "{from} to {to}: {}",
                font.color
            );
        }
    }

    use crate::part_groups;

    /// Asked for its parts, a structure draws each element and each line's
    /// words (its label and multiplicities) in groups of their own, named
    /// as its map names them; a package's groups hold nothing of what it
    /// holds, a note inside it goes with it.
    #[test]
    fn a_structure_s_parts_are_named_as_its_map_names_them() {
        let source = "@startuml\npackage Lending {\n  class Loan\n  class Copy\n  Loan \"1\" --> \"*\" Copy : lends\n  note right of Copy : on the shelf\n}\nclass Member\nMember --> Loan : borrows\n@enduml\n";
        let (svg, map) = crate::render_mapped(source, None).unwrap();
        let groups = part_groups(&svg);
        let named = |name: &str| groups.iter().any(|(n, _)| n == name);
        for place in &map.places {
            let part = place.part.as_deref().unwrap();
            assert!(named(part), "{} ({part}) not drawn: {groups:?}", place.name);
        }
        let lends = map.wires.iter().find(|w| w.key.from == "Loan").unwrap();
        for words in lends.words.iter().flatten() {
            assert!(named(words), "{words} not drawn");
        }
        assert!(lends.words.iter().all(Option::is_some), "{:?}", lends.words);
        let lending = map.places.iter().find(|p| p.name == "Lending").unwrap();
        let lending = lending.part.as_deref().unwrap();
        for (_, group) in groups.iter().filter(|(n, _)| n == lending) {
            assert!(
                !group.contains(">Loan<") && !group.contains(">Copy<"),
                "{group}"
            );
        }
        // The note inside Lending is drawn in a part of its own, by
        // Copy's, and Lending's holds nothing of it.
        assert!(
            groups
                .iter()
                .any(|(n, g)| n == "note-class-Copy-right-1" && g.contains("shelf")),
            "the note in a part of its own: {groups:?}"
        );
        assert!(
            !groups
                .iter()
                .any(|(n, g)| n == lending && g.contains("shelf"))
        );
        let ground = crate::without_parts(&svg).unwrap();
        assert!(!ground.contains("shelf"), "{ground}");
    }

    /// A box's shadow is drawn in its part: it goes where the box goes,
    /// and stays off the ground.
    #[test]
    fn a_shadow_goes_with_its_box() {
        let source =
            "@startuml\nskinparam shadowing true\nclass Loan\nclass Copy\nLoan --> Copy\n@enduml\n";
        let (svg, _) = crate::render_mapped(source, None).unwrap();
        assert!(svg.contains(r##"fill="#000000""##), "shadows drawn: {svg}");
        let ground = crate::without_parts(&svg).unwrap();
        assert!(!ground.contains(r##"fill="#000000""##), "{ground}");
    }

    /// A namespace and a class of one name are two parts, the namespace's
    /// holding nothing of the classes in it.
    #[test]
    fn a_namespace_and_its_class_of_one_name_are_two_parts() {
        let diagram = super::super::mermaid::from_mermaid(
            "classDiagram\nnamespace Order {\n  class Order\n  class Line\n}\n",
        )
        .unwrap();
        let (svg, map) = draw_mapped(&diagram).unwrap();
        let orders: Vec<&str> = map
            .places
            .iter()
            .filter(|p| p.name == "Order")
            .filter_map(|p| p.part.as_deref())
            .collect();
        assert_eq!(orders.len(), 2, "{:?}", map.places);
        assert_ne!(orders[0], orders[1]);
        let groups = part_groups(&svg);
        let space = map
            .places
            .iter()
            .find(|p| p.name == "Order" && p.frame)
            .and_then(|p| p.part.as_deref())
            .unwrap();
        for (_, group) in groups.iter().filter(|(n, _)| n == space) {
            assert!(!group.contains(">Line<"), "{group}");
            assert_eq!(group.matches("<rect").count(), 0, "{group}");
        }
    }

    #[test]
    fn a_cloud_keeps_to_the_edge_of_its_box() {
        let rect = Rect {
            x: 10.0,
            y: 20.0,
            w: 240.0,
            h: 150.0,
        };
        let path = cloud_path(rect);
        let numbers: Vec<f32> = path
            .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter_map(|part| part.parse().ok())
            .collect();
        assert!(numbers.len() > 40, "{path}");
        for pair in numbers.chunks(2) {
            let (x, y) = (pair[0], pair[1]);
            // Handles reach past the box by a third of the dip; the curve
            // itself, three quarters of the way, stays on the box's edge.
            let out = CLOUD_DIP / 0.75 - CLOUD_DIP;
            assert!(
                x >= rect.x - out - 0.01 && x <= rect.x + rect.w + out + 0.01,
                "{x}"
            );
            assert!(
                y >= rect.y - out - 0.01 && y <= rect.y + rect.h + out + 0.01,
                "{y}"
            );
        }
        // Where two bumps meet, each curve's end, the edge dips CLOUD_DIP
        // in from the nearest side, no deeper.
        for end in numbers.chunks(2).skip(3).step_by(3) {
            let (x, y) = (end[0], end[1]);
            let nearest = (x - rect.x)
                .min(rect.x + rect.w - x)
                .min(y - rect.y)
                .min(rect.y + rect.h - y);
            assert!((nearest - CLOUD_DIP).abs() < 0.01, "({x}, {y}): {nearest}");
        }
    }

    fn scene_of(source: &str) -> Scene {
        let (_, lines) = crate::source::body(source).unwrap();
        let diagram = parse(&lines).unwrap_or_else(|err| panic!("{err:?}"));
        let skin = Skin::default();
        let canvas = Canvas::new(&skin.font_name, false);
        layout_group(&diagram.root, &skin, canvas.family()).unwrap()
    }

    #[test]
    fn a_loop_s_multiplicities_stand_outside_it() {
        let svg = crate::render(
            "@startuml\nclass Usuario\nUsuario \"1\" -- \"0..*\" Usuario : supervisa >\n@enduml\n",
        )
        .unwrap();
        // The baseline of each word, and the middle of the box's side.
        let baseline = |word: &str| -> f32 {
            let at = svg.find(&format!(">{word}</tspan>")).expect("the word");
            let tag = &svg[svg[..at].rfind("<tspan").unwrap()..at];
            let y = tag.split("y=\"").nth(1).unwrap().split('"').next().unwrap();
            y.parse().unwrap()
        };
        let rect = &svg[svg.find("<rect").unwrap()..];
        let number = |name: &str| -> f32 {
            let value = rect.split(&format!(" {name}=\"")).nth(1).unwrap();
            value.split('"').next().unwrap().parse().unwrap()
        };
        let middle = number("y") + number("height") / 2.0;
        // Over the run that leaves, under the one that arrives (its
        // figures, 12 high, rise 8 over their baseline): neither between
        // the two.
        assert!(baseline("1") < middle - 9.0, "{svg}");
        assert!(baseline("0..*") - 8.0 > middle + 9.0, "{svg}");
    }

    #[test]
    fn a_hidden_line_is_laid_out_and_not_drawn() {
        let shown = crate::render("rectangle a\nrectangle b\na --> b : seen\n").unwrap();
        let hidden = crate::render("rectangle a\nrectangle b\na -[hidden]-> b : unseen\n").unwrap();
        assert!(shown.contains("seen") && !hidden.contains("unseen"));
        // The line and its head are gone: only the two boxes are left.
        let shapes = |svg: &str| svg.matches("<p").count() + svg.matches("<line").count();
        assert!(shapes(&shown) >= shapes(&hidden) + 2, "{hidden}");
        // The two boxes stand as with the line: one under the other.
        let scene = scene_of("rectangle a\nrectangle b\na -[hidden]-> b\n");
        assert!(scene.nodes[1].rect.y > scene.nodes[0].rect.y + scene.nodes[0].rect.h);
        // Held to the right of the first, and beside it: no line joins it
        // to anything, and it does not drift from its neighbor.
        let scene = scene_of(
            "rectangle a\nrectangle b\nrectangle c\nrectangle wide_one_below\na --> b\nb --> wide_one_below\na -[hidden]right- c\n",
        );
        let (a, c) = (scene.nodes[0].rect, scene.nodes[2].rect);
        assert!((a.y - c.y).abs() < 0.5, "one rank");
        assert!(c.x > a.x + a.w && c.x - (a.x + a.w) < 80.0, "{a:?} {c:?}");
    }

    #[test]
    fn a_title_and_its_list_start_at_the_left_when_the_skin_says_so() {
        let source = "skinparam defaultTextAlignment left\nrectangle \"Title\\n* a long item of the list\" as r\n";
        let svg = crate::render(source).unwrap();
        let xs: Vec<&str> = svg
            .split("<tspan x=\"")
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap())
            .collect();
        assert_eq!(xs.len(), 2, "{svg}");
        assert_eq!(xs[0], xs[1], "both lines start at one edge: {svg}");
    }

    #[test]
    fn a_class_stays_inside_its_package() {
        let scene = scene_of("@startuml\npackage shop {\n  class Book\n}\n@enduml\n");
        let shop = &scene.nodes[0];
        assert!(shop.item.composite);
        let book = &shop.children.nodes[0];
        assert!(book.rect.x + 0.1 >= shop.rect.x);
        assert!(book.rect.y + 0.1 >= shop.rect.y);
        assert!(book.rect.x + book.rect.w <= shop.rect.x + shop.rect.w + 0.5);
        assert!(book.rect.y + book.rect.h <= shop.rect.y + shop.rect.h + 0.5);
    }

    #[test]
    fn a_multiplicity_keeps_off_a_package_border() {
        // Shelf below the package: beside the line where it leaves the
        // package, `many` sat on the package's border.
        let scene = scene_of(
            "@startuml\npackage catalog {\n  class Book\n}\nclass Shelf\nShelf o-- \"many\" Book\nBook --> Shelf\n@enduml\n",
        );
        let around = containers(&scene);
        assert_eq!(around.len(), 1);
        let edge = scene
            .edges
            .iter()
            .find(|edge| !edge.look.to_card.is_empty())
            .unwrap();
        let on = |cards: Vec<Rect>| cards.iter().any(|&card| on_border(around[0], card));
        assert!(on(card_rects(edge, &[]).collect()), "no longer a case");
        let cards: Vec<Rect> = card_rects(edge, &around).collect();
        assert_eq!(cards.len(), 1);
        assert!(!on(cards));
    }

    #[test]
    fn a_line_out_of_a_package_is_drawn() {
        let (_, lines) = crate::source::body(
            "@startuml\npackage shop {\n  class Book\n}\nclass Shelf\nShelf --> Book\n@enduml\n",
        )
        .unwrap();
        let diagram = parse(&lines).unwrap_or_else(|err| panic!("{err:?}"));
        let svg = draw(&diagram).unwrap_or_else(|err| panic!("{err:?}"));
        assert!(svg.contains("Book"), "{svg}");
        assert!(svg.contains("Shelf"), "{svg}");
        assert!(svg.contains("shop"), "{svg}");
        // The line, straight or turning in curves.
        assert!(svg.contains("<polyline") || svg.contains(" C"), "{svg}");
        let (x, w) = tspan(&svg, "shop");
        let outline = polygons(&svg)
            .into_iter()
            .find(|pts| tab_holds(pts, x, w))
            .expect("one outline around the name");
        let top = outline.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let tab_right = outline
            .iter()
            .filter(|p| (p.1 - top).abs() < 0.6)
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let tab_left = outline
            .iter()
            .filter(|p| (p.1 - top).abs() < 0.6)
            .map(|p| p.0)
            .fold(f32::INFINITY, f32::min);
        let full = outline
            .iter()
            .map(|p| p.0)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), x| {
                (lo.min(x), hi.max(x))
            });
        assert!(
            tab_right - tab_left + 0.6 < full.1 - full.0,
            "the tab is the name, not the whole package"
        );
        // The tab's right side ends where the body starts, on one y.
        let step = outline
            .iter()
            .filter(|p| (p.0 - tab_right).abs() < 0.6 && p.1 > top + 0.6)
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min);
        let body = outline
            .iter()
            .filter(|p| p.0 > tab_right + 0.6)
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min);
        assert!(
            (step - body).abs() < 0.6,
            "the tab sits on the body: {step} vs {body}"
        );
    }

    /// The smart home of the gallery: lines from the rooms come into the
    /// hub from above, as the flow runs, past its name, and end on top of
    /// the box they go to.
    #[test]
    fn lines_into_a_container_come_in_from_above_past_its_name() {
        let source = include_str!("../../../../examples/gallery/smart-home.puml");
        let (_, lines) = crate::source::body(source).unwrap();
        let diagram = parse(&lines).unwrap();
        let scene = scene_of(source);
        let at = diagram.root.nodes.iter().position(|node| node.key == "Hub");
        let hub = &scene.nodes[at.expect("the hub")];
        assert!(hub.opened, "laid out with the rooms");
        let rules = hub.children.nodes[0].rect;
        let name = name_band(hub);
        let ends: Vec<&PlacedEdge> = scene
            .edges
            .iter()
            .filter(|edge| {
                edge.points
                    .last()
                    .is_some_and(|end| (end.1 - rules.y).abs() < 0.1)
            })
            .collect();
        assert_eq!(ends.len(), 4, "temperature, opened, rang, phone app");
        for edge in &scene.edges {
            for pair in edge.points.windows(2) {
                assert!(
                    !graph::route::cuts(name, pair[0], pair[1]),
                    "{:?} cuts the name {name:?}",
                    edge.points
                );
            }
        }
        let mut xs: Vec<f32> = ends
            .iter()
            .map(|edge| edge.points.last().unwrap().0)
            .collect();
        xs.sort_by(f32::total_cmp);
        // A line with a label keeps its own arrival.
        assert!(xs.windows(2).all(|pair| pair[1] - pair[0] > 4.0), "{xs:?}");
    }

    /// Two multiplicities into one box stay on their own arrivals. On one
    /// trunk the marks would cover each other: the placer knows only its
    /// own line.
    #[test]
    fn multiplicities_into_one_box_stay_apart() {
        let scene = scene_of(
            "@startuml\nclass A\nclass B\nclass C\nA --> \"1\" C\nB --> \"*\" C\n@enduml\n",
        );
        let ends: Vec<f32> = scene
            .edges
            .iter()
            .map(|edge| edge.points.last().unwrap().0)
            .collect();
        assert!(
            (ends[0] - ends[1]).abs() > 4.0,
            "each multiplicity keeps its arrival: {ends:?}"
        );
        let marks: Vec<Rect> = scene
            .edges
            .iter()
            .flat_map(|edge| card_rects(edge, &[]))
            .collect();
        assert_eq!(marks.len(), 2, "{marks:?}");
        let (a, b) = (marks[0], marks[1]);
        let overlaps = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
        assert!(!overlaps, "{marks:?}");
    }

    /// An arrow, a triangle and a dependency into one box each keep their
    /// arrival: on one trunk the heads would cover each other. Lines that
    /// arrive alike still share one.
    #[test]
    fn lines_that_arrive_differently_stay_apart() {
        let ends = |source: &str| -> Vec<f32> {
            scene_of(source)
                .edges
                .iter()
                .map(|edge| edge.points.last().unwrap().0)
                .collect()
        };
        let mixed = ends(
            "@startuml\nclass A\nclass B\nclass D\nclass C\nA --> C\nB --|> C\nD ..> C\n@enduml\n",
        );
        let mut sorted = mixed.clone();
        sorted.sort_by(f32::total_cmp);
        assert!(
            sorted.windows(2).all(|pair| pair[1] - pair[0] > 4.0),
            "each head keeps its arrival: {mixed:?}"
        );
        let alike = ends(
            "@startuml\nclass A\nclass B\nclass D\nclass C\nA --> C\nB --> C\nD --> C\n@enduml\n",
        );
        assert!(
            alike.windows(2).all(|pair| (pair[1] - pair[0]).abs() < 0.5),
            "alike, they share a trunk: {alike:?}"
        );
    }

    /// Subclasses written parent first (`Shape <|-- Circle`) arrive at the
    /// parent as those written the other way round do: under a narrow box
    /// their five triangles lay on one another, and now share one trunk,
    /// across the flow too. A line with no head where it starts stays on
    /// its own.
    #[test]
    fn subclasses_written_parent_first_share_one_triangle() {
        let starts = |source: &str| -> Vec<(f32, f32)> {
            scene_of(source)
                .edges
                .iter()
                .map(|edge| edge.points[0])
                .collect()
        };
        let kinds = ["Circle", "Square", "Triangle", "Hexagon", "Line"];
        let body: String = kinds.iter().map(|k| format!("Shape <|-- {k}\n")).collect();
        for flow in ["", "left to right direction\n"] {
            let at = starts(&format!("@startuml\n{flow}class Shape\n{body}@enduml\n"));
            assert!(
                at.windows(2).all(|pair| (pair[1].0 - pair[0].0).abs()
                    + (pair[1].1 - pair[0].1).abs()
                    < 0.5),
                "{flow}one trunk: {at:?}"
            );
        }
        let plain: String = kinds.iter().map(|k| format!("Shape -- {k}\n")).collect();
        let at = starts(&format!("@startuml\nclass Shape\n{plain}@enduml\n"));
        let mut across: Vec<f32> = at.iter().map(|point| point.0).collect();
        across.sort_by(f32::total_cmp);
        across.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        assert_eq!(across.len(), kinds.len(), "each plain line its own: {at:?}");
    }

    /// As Graphviz ranks clusters: a package moves down only as far as its
    /// lines ask, so two can share ranks side by side.
    #[test]
    fn packages_share_the_ranks_of_the_page() {
        let scene = scene_of(
            "@startuml\npackage Members {\n  class Member\n  class Level\n  Member --> Level\n}\npackage Lending {\n  class Loan\n}\nMember --> Loan\n@enduml\n",
        );
        let (members, lending) = (&scene.nodes[0], &scene.nodes[1]);
        let (level, loan) = (
            members.children.nodes[1].rect,
            lending.children.nodes[0].rect,
        );
        assert!((level.y - loan.y).abs() < 0.5, "{level:?} {loan:?}");
        let (a, b) = (members.rect, lending.rect);
        assert!(
            a.x + a.w <= b.x + 0.5 || b.x + b.w <= a.x + 0.5,
            "{a:?} {b:?}"
        );
    }

    #[test]
    fn an_interface_keeps_the_word_from_its_line() {
        let svg = draw(
            &parse(
                &crate::source::body("@startuml\ninterface Sellable\n@enduml\n")
                    .unwrap()
                    .1,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(svg.contains("interface"), "{svg}");
        assert!(svg.contains("Sellable"), "{svg}");
    }

    fn tspan(svg: &str, word: &str) -> (f32, f32) {
        let at = svg.find(&format!(">{word}</tspan>")).expect(word);
        let start = svg[..at].rfind("<tspan ").expect("tspan");
        let tag = &svg[start..at];
        (attr(tag, "x"), attr(tag, "textLength"))
    }

    /// The top edge of `pts` covers the run from `x` over `w`.
    fn tab_holds(pts: &[(f32, f32)], x: f32, w: f32) -> bool {
        let top = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let mut left = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        for p in pts.iter().filter(|p| (p.1 - top).abs() < 0.6) {
            left = left.min(p.0);
            right = right.max(p.0);
        }
        left <= x + 0.6 && right + 0.6 >= x + w
    }

    fn polygons(svg: &str) -> Vec<Vec<(f32, f32)>> {
        svg.match_indices("<polygon ")
            .map(|(at, _)| {
                let tag = &svg[at..svg[at..].find('>').map(|end| at + end).unwrap()];
                let key = "points=\"";
                let start = tag.find(key).expect("points") + key.len();
                tag[start..]
                    .split('"')
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .map(|pair| {
                        let (x, y) = pair.split_once(',').unwrap();
                        (x.parse().unwrap(), y.parse().unwrap())
                    })
                    .collect()
            })
            .collect()
    }

    fn attr(tag: &str, name: &str) -> f32 {
        let key = format!("{name}=\"");
        let at = tag.find(&key).unwrap_or_else(|| panic!("{name} in {tag}"));
        tag[at + key.len()..]
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }

    /// A line's path carries its id, in the lines' group; its heads and
    /// its words carry its class (docs/adr/0037).
    #[test]
    fn a_line_carries_its_id() {
        let svg = crate::render("@startuml\nclass A\nclass B\nA --> B : uses\nA -- B\n@enduml\n")
            .unwrap();
        let lines_at = svg.find(r#"<g id="lines">"#).unwrap();
        let lines = &svg[lines_at..lines_at + crate::svg::group_length(&svg[lines_at..]).unwrap()];
        for name in ["link-A-B-uses", "link-A-B"] {
            let own = format!(r#"<g class="{name}" id="{name}">"#);
            let at = lines
                .find(&own)
                .unwrap_or_else(|| panic!("{name}: {lines}"));
            let drawn = &lines[at + own.len()..];
            assert!(
                drawn.starts_with("<path") || drawn.starts_with("<polyline"),
                "{name}: {lines}"
            );
            assert_eq!(svg.matches(&format!(r#"id="{name}""#)).count(), 1, "{name}");
        }
        // Its head and its words: the class alone.
        assert!(
            svg.matches(r#"<g class="link-A-B-uses">"#).count() >= 2,
            "{svg}"
        );
    }
}
