//! Draws a structure diagram. A package, node or frame a line runs into
//! from outside is laid out with what is around it ([`open`]); any other
//! is laid out on its own and becomes a box around its children's layout.
//!
//! Notes keep the side that was asked for. "Right" stays on the right,
//! including when the group runs left to right.

use std::fmt::Write as _;

use super::skin::Skin;
use super::{Crossing, Dash, Direction, Element, Group, Head, Link, Member, Shape, Side};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::{self, Color, Paint};
use crate::graph::across::{self, Drawn, Placement};
use crate::graph::{self, Rect};
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{self, Block, Family, Font, Style};

mod open;

use open::layout_group;

const MARGIN: f32 = 10.0;
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 6.0;
const MIN_W: f32 = 54.0;
const MIN_H: f32 = 28.0;
const COMP_PAD: f32 = 12.0;
const COMP_GAP: f32 = 8.0;
const NOTE_GAP: f32 = 10.0;
const NOTE_PAD: f32 = 6.0;
/// A `--` between members, and the line under a class name.
const RULE: f32 = 10.0;
const HEAD_LENGTH: f32 = 10.0;
const HEAD_HALF: f32 = 4.0;
const MARK_LENGTH: f32 = 14.0;
const DEPTH: f32 = 10.0;
const STACK: f32 = 5.0;
/// Cylinder caps and the stick figure, shared by measuring and painting.
const CAP: f32 = 9.0;
const ACTOR_H: f32 = 50.0;
const ACTOR_W: f32 = 40.0;
const LOOP_REACH: f32 = 16.0;
const LOOP_STEP: f32 = 12.0;

pub fn draw(diagram: &super::Diagram) -> Result<String, Declined> {
    let skin = Skin::from_params(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
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
    scene.offset(0.0, annotations.top());
    paint(&mut canvas, &skin, &scene);
    annotations.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, skin.background))
}

/// One group after layout. `width` and `height` are the graph, which is
/// what a parent sizes its box from. A note above the first row can sit
/// outside that, and the canvas still grows to hold it.
struct Scene {
    nodes: Vec<Placed>,
    edges: Vec<PlacedEdge>,
    notes: Vec<PlacedNote>,
    width: f32,
    height: f32,
}

impl Scene {
    fn empty() -> Scene {
        Scene {
            nodes: Vec::new(),
            edges: Vec::new(),
            notes: Vec::new(),
            width: 0.0,
            height: 0.0,
        }
    }

    fn offset(&mut self, dx: f32, dy: f32) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        for node in &mut self.nodes {
            node.rect.x += dx;
            node.rect.y += dy;
            node.children.offset(dx, dy);
        }
        for edge in &mut self.edges {
            for point in &mut edge.points {
                point.0 += dx;
                point.1 += dy;
            }
            edge.label_at.0 += dx;
            edge.label_at.1 += dy;
        }
        for note in &mut self.notes {
            note.x += dx;
            note.y += dy;
        }
    }
}

struct Placed {
    rect: Rect,
    shape: Shape,
    fill: Paint,
    border: Stroke,
    rows: Vec<Row>,
    /// First member row. Equal to `rows.len()` when the box has none.
    body: usize,
    /// How far the title band reaches down from the top of the box.
    /// Lines into a container stay below it. Zero for a leaf that is not a
    /// package; a package uses it as the tab's height.
    banner: f32,
    composite: bool,
    /// Laid out with the group around it, so its elements and lines are
    /// that group's too.
    opened: bool,
    children: Scene,
}

impl Placed {
    fn of(item: Measured, rect: Rect) -> Placed {
        Placed {
            rect,
            shape: item.shape,
            fill: item.fill,
            border: item.border,
            rows: item.rows,
            body: item.body,
            banner: item.banner,
            composite: item.composite,
            opened: false,
            children: item.children,
        }
    }
}

struct PlacedEdge {
    points: Vec<(f32, f32)>,
    label: Block,
    label_at: (f32, f32),
    from_card: Block,
    to_card: Block,
    from_head: Head,
    to_head: Head,
    stroke: Stroke,
}

struct PlacedNote {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: Block,
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

struct Measured {
    width: f32,
    height: f32,
    shape: Shape,
    fill: Paint,
    border: Stroke,
    rows: Vec<Row>,
    body: usize,
    banner: f32,
    gap: f32,
    composite: bool,
    children: Scene,
}

struct Labeled {
    label: Block,
    from_card: Block,
    to_card: Block,
}

fn child_origin(rect: Rect, item: &Measured) -> (f32, f32) {
    let (x, w) = match item.shape {
        Shape::Node => (rect.x, rect.w - DEPTH),
        Shape::Stack => (rect.x, rect.w - STACK * 2.0),
        _ => (rect.x, rect.w),
    };
    let ox = x + (w - item.children.width) / 2.0;
    let oy = rect.y + item.banner + item.gap;
    (ox, oy)
}

/// A container the layout around it opens holds no layout of its own; it
/// is measured as if it held something.
fn measure_element(
    el: &Element,
    skin: &Skin,
    family: Family,
    open: bool,
) -> Result<Measured, Declined> {
    let mut look = *skin.of(el.shape);
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
    let (fill, border) = paints(el, &look)?;
    let (rows, body) = rows_of(el, &look, family)?;
    let children = if el.composite && !open {
        layout_group(&el.children, skin, family)?
    } else {
        Scene::empty()
    };
    let header_w = rows[..body].iter().map(Row::width).fold(0.0, f32::max);
    let header_h = rows[..body].iter().map(Row::height).sum::<f32>();
    let all_w = rows.iter().map(Row::width).fold(0.0, f32::max);
    let all_h = rows.iter().map(Row::height).sum::<f32>();
    let min_w = min_width(el.shape, skin);
    let divider = if !el.composite && (body < rows.len() || el.shape == Shape::Card) {
        RULE
    } else {
        0.0
    };
    let empty = !open && children.width == 0.0 && children.height == 0.0;
    let gap = if empty { 0.0 } else { COMP_GAP };

    let (width, height, banner, gap) = if el.composite {
        match el.shape {
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
                    .max(header_w + 28.0)
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
        match el.shape {
            Shape::Actor => {
                let width = all_w.max(ACTOR_W).max(min_w);
                let height = ACTOR_H + all_h + 2.0;
                (width, height, 0.0, 0.0)
            }
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
            Shape::Package | Shape::Folder => {
                let banner = all_h + 8.0;
                let width = (all_w + 28.0).max(min_w);
                let height = banner + 20.0;
                (width, height, banner, 0.0)
            }
            Shape::Note => {
                let width = (all_w + NOTE_PAD * 2.0 + 6.0).max(24.0);
                let height = (all_h + NOTE_PAD * 2.0).max(20.0);
                (width, height, 0.0, 0.0)
            }
            _ => {
                let icon = if el.shape == Shape::Component {
                    16.0
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
        shape: el.shape,
        fill,
        border,
        rows,
        body,
        banner,
        gap,
        composite: el.composite,
        children,
    })
}

/// The room an opened container keeps around what it holds, as
/// [`measure_element`] sizes one around a layout of its own.
fn frame_of(item: &Measured, skin: &Skin) -> graph::Frame {
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
        Shape::Package | Shape::Folder => (COMP_PAD, COMP_PAD, (header_w + 28.0).max(min_w)),
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
            | Shape::Annotation
            | Shape::Object
    );
    if class {
        MIN_W.max(skin.min_class)
    } else {
        MIN_W
    }
}

fn rows_of(
    el: &Element,
    look: &super::skin::Look,
    family: Family,
) -> Result<(Vec<Row>, usize), Declined> {
    let mut rows = Vec::new();
    for stereotype in &el.stereotypes {
        let shown = format!("«{}»", stereotype.value);
        let block = text::block(&shown, look.stereo, family)
            .map_err(|err| Declined::at(stereotype.line, err))?;
        rows.push(Row::Center(block));
    }
    let name_font = if el.shape == Shape::Abstract {
        look.font.italic()
    } else {
        look.font
    };
    let mut name = text_block(&el.label, name_font, family)?;
    if el.shape == Shape::Object {
        name = restyle(name, name_font, family, false, true);
    }
    if !name.is_empty() {
        rows.push(Row::Center(name));
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
                let block = text_block(text, look.member, family)?;
                let block = restyle(block, look.member, family, *italic, *underline);
                if !block.is_empty() {
                    rows.push(Row::Left(block));
                }
            }
            Member::Rule(dash) => rows.push(Row::Rule(*dash)),
            Member::Heading(text) => {
                let block = text_block(text, look.member, family)?;
                if !block.is_empty() {
                    rows.push(Row::Center(block));
                }
            }
        }
    }
    Ok((rows, body))
}

fn restyle(block: Block, font: Font, family: Family, italic: bool, underline: bool) -> Block {
    if block.is_empty() || (!italic && !underline) {
        return block;
    }
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
    text::styled(lines, font, family)
}

fn paints(el: &Element, look: &super::skin::Look) -> Result<(Paint, Stroke), Declined> {
    let fill = match &el.fill {
        Some(value) => color::parse(value).map_err(|err| Declined::at(el.line, err))?,
        None => look.fill,
    };
    let border = match &el.line_color {
        Some(value) => match color::parse(value).map_err(|err| Declined::at(el.line, err))? {
            Some(color) => color,
            None => {
                return Err(Declined::at(el.line, "a border must not be transparent"));
            }
        },
        None => look.border,
    };
    let mut stroke = Stroke::new(border, el.thickness.unwrap_or(look.thickness));
    stroke.dash = el.border_dash;
    Ok((fill, stroke))
}

fn link_blocks(link: &Link, skin: &Skin, family: Family) -> Result<Labeled, Declined> {
    let block = |text: &Option<crate::common::Text>| match text {
        Some(text) => text_block(text, skin.arrow_font, family),
        None => Ok(Block::empty()),
    };
    Ok(Labeled {
        label: block(&link.label)?,
        from_card: block(&link.from_card)?,
        to_card: block(&link.to_card)?,
    })
}

fn text_block(text: &crate::common::Text, font: Font, family: Family) -> Result<Block, Declined> {
    if text.value.is_empty() {
        return Ok(Block::empty());
    }
    text::block(&text.value, font, family).map_err(|err| Declined::at(text.line, err))
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
        for index in indices {
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
                text: std::mem::replace(&mut note.text, Block::empty()),
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
        scene_at(scene, path)
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
        let Some(host) = scene_at(scene, &path) else {
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
            from_card: blocks.from_card,
            to_card: blocks.to_card,
            from_head: crossing.arrow.from_head,
            to_head: crossing.arrow.to_head,
            stroke: arrow_stroke(skin, &crossing.arrow)?,
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

fn scene_at<'a>(scene: &'a mut Scene, path: &[usize]) -> Option<&'a mut Scene> {
    let mut here = scene;
    for &index in path {
        here = &mut here.nodes.get_mut(index)?.children;
    }
    Some(here)
}

fn placements(nodes: &[Placed]) -> Vec<Placement> {
    nodes
        .iter()
        .map(|node| {
            let frame = node.composite;
            let title = (frame && node.banner > 0.0).then(|| name_band(node));
            Placement {
                rect: node.rect,
                flat: is_flat(node.shape),
                frame,
                title,
                children: if node.composite {
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
    match node.shape {
        Shape::Package | Shape::Folder => Rect {
            w: tab_width(rect.w, name),
            h: node.banner,
            ..rect
        },
        _ => {
            let (area, _) = text_area(node);
            let w = (name + PAD_X * 2.0).min(area.w);
            Rect {
                x: area.x + (area.w - w) / 2.0,
                y: rect.y,
                w,
                h: node.banner,
            }
        }
    }
}

fn is_flat(shape: Shape) -> bool {
    !matches!(
        shape,
        Shape::UseCase
            | Shape::Actor
            | Shape::Cloud
            | Shape::Database
            | Shape::Storage
            | Shape::Hexagon
    )
}

fn drawn_in(scene: &Scene, drawn: &mut Drawn) {
    for edge in &scene.edges {
        if edge.points.len() >= 2 {
            drawn.lines.push(edge.points.clone());
        }
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

fn paint(canvas: &mut Canvas, skin: &Skin, scene: &Scene) {
    paint_shapes(canvas, skin, scene);
    paint_edges(canvas, scene, skin.curve);
    paint_notes(canvas, skin, scene);
    paint_labels(canvas, scene);
    paint_heads(canvas, skin, scene);
}

fn paint_shapes(canvas: &mut Canvas, skin: &Skin, scene: &Scene) {
    for node in &scene.nodes {
        paint_shape(canvas, skin, node);
        if node.composite {
            paint_shapes(canvas, skin, &node.children);
        }
    }
}

fn paint_shape(canvas: &mut Canvas, skin: &Skin, node: &Placed) {
    let rect = node.rect;
    let round = skin.round.min(rect.w / 2.0).min(rect.h / 2.0).max(0.0);
    let stroke = Some(node.border);
    match node.shape {
        Shape::UseCase => {
            canvas.ellipse(
                Layer::Frames,
                (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0),
                (rect.w / 2.0, rect.h / 2.0),
                node.fill,
                stroke,
            );
        }
        Shape::Actor => paint_actor(canvas, rect, node.fill, node.border),
        Shape::Node => paint_node(canvas, rect, node.fill, node.border),
        Shape::Database | Shape::Storage => paint_cylinder(canvas, rect, node.fill, node.border),
        Shape::Cloud => {
            canvas.path(
                Layer::Frames,
                &cloud_path(rect),
                (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h),
                node.fill,
                stroke,
            );
        }
        Shape::Hexagon => {
            canvas.polygon(Layer::Frames, &hexagon(rect), node.fill, stroke);
        }
        Shape::Package | Shape::Folder => {
            paint_tab(
                canvas,
                rect,
                node.banner,
                header_width(node),
                round,
                node.fill,
                node.border,
            );
        }
        Shape::Stack => paint_stack(canvas, rect, round, node.fill, node.border),
        Shape::Agent => paint_agent(canvas, rect, round, node.fill, node.border),
        Shape::Note | Shape::Artifact | Shape::File => {
            dog_ear(canvas, rect, node.fill, node.border);
        }
        Shape::Class
        | Shape::Abstract
        | Shape::Interface
        | Shape::Enum
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
                node.fill,
                stroke,
            );
        }
    }
    if node.shape == Shape::Component {
        component_icon(canvas, rect, node.border);
    }
    paint_rows(canvas, node);
}

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
        canvas.rect(
            Layer::Frames,
            (rect.x, top, rect.w, bot - top),
            0.0,
            fill,
            None,
        );
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
    let body_y = rect.y + banner;
    let body_h = (rect.h - banner).max(1.0);
    canvas.rect(
        Layer::Frames,
        (rect.x, body_y, rect.w, body_h),
        round,
        fill,
        Some(stroke),
    );
    canvas.rect(
        Layer::Frames,
        (rect.x, rect.y, tab_width(rect.w, label_w), banner + 1.0),
        0.0,
        fill,
        Some(stroke),
    );
}

/// The name band of a package or folder. Members, when a class has them,
/// start after it.
fn header_width(node: &Placed) -> f32 {
    let n = if node.body == 0 {
        node.rows.len()
    } else {
        node.body
    };
    node.rows.iter().take(n).map(Row::width).fold(0.0, f32::max)
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
    let w = 14.0;
    let h = 4.5;
    let x = rect.x + rect.w - w - 6.0;
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

fn cloud_path(rect: Rect) -> String {
    let unit = [
        (0.20, 0.60),
        (0.05, 0.60),
        (0.00, 0.40),
        (0.16, 0.32),
        (0.12, 0.12),
        (0.32, 0.02),
        (0.46, 0.16),
        (0.55, 0.02),
        (0.78, 0.04),
        (0.84, 0.24),
        (1.00, 0.28),
        (0.98, 0.55),
        (0.80, 0.60),
        (0.86, 0.82),
        (0.62, 0.98),
        (0.46, 0.80),
        (0.34, 0.98),
        (0.08, 0.86),
        (0.18, 0.64),
    ];
    let at = |i: usize| {
        format!(
            "{},{}",
            n(rect.x + unit[i].0 * rect.w),
            n(rect.y + unit[i].1 * rect.h)
        )
    };
    let mut d = format!("M{}", at(0));
    let mut i = 1;
    while i + 2 < unit.len() {
        let _ = write!(d, " C{} {} {}", at(i), at(i + 1), at(i + 2));
        i += 3;
    }
    d.push('Z');
    d
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
    let (area, place) = text_area(node);
    let pad_y = if node.shape == Shape::Note {
        NOTE_PAD
    } else {
        PAD_Y
    };
    let pad_x = if node.shape == Shape::Note {
        NOTE_PAD
    } else {
        PAD_X
    };
    let extra =
        if node.body < node.rows.len() || (node.shape == Shape::Card && !node.rows.is_empty()) {
            RULE
        } else {
            0.0
        };
    let content = node.rows.iter().map(Row::height).sum::<f32>() + extra;
    let mut y = match place {
        Place::Top => area.y + pad_y,
        Place::Center => area.y + (area.h - content) / 2.0,
    };
    let header = node.body.min(node.rows.len());
    for row in &node.rows[..header] {
        paint_row(canvas, area, pad_x, y, row, node.shape);
        y += row.height();
    }
    if extra > 0.0 {
        let line_y = y + extra / 2.0;
        canvas.line(
            Layer::Frames,
            (area.x, line_y),
            (area.x + area.w, line_y),
            Stroke::new(node.border.color, node.border.width.max(0.5)),
        );
        y += extra;
    }
    for row in &node.rows[header..] {
        match row {
            Row::Rule(dash) => {
                let line_y = y + RULE / 2.0;
                let mut stroke = Stroke::new(node.border.color, node.border.width.max(0.5));
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
                paint_row(canvas, area, pad_x, y, other, node.shape);
                y += other.height();
            }
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
    let place = if node.composite {
        Place::Top
    } else {
        match node.shape {
            Shape::UseCase
            | Shape::Cloud
            | Shape::Hexagon
            | Shape::Database
            | Shape::Storage
            | Shape::Note
            | Shape::Actor => Place::Center,
            _ => Place::Top,
        }
    };
    let area = match node.shape {
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
        Shape::Agent if !node.composite => Rect {
            x: rect.x,
            y: rect.y + 12.0,
            w: rect.w,
            h: (rect.h - 12.0).max(1.0),
        },
        // The name is the tab, not the middle of the whole box.
        Shape::Package | Shape::Folder => {
            let h = if node.banner > 1.0 {
                node.banner.min(rect.h).max(1.0)
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

fn paint_edges(canvas: &mut Canvas, scene: &Scene, curve: f32) {
    for edge in &scene.edges {
        if edge.points.len() < 2 {
            continue;
        }
        canvas.curve(
            Layer::Frames,
            &shortened(&edge.points, edge.from_head, edge.to_head),
            curve,
            edge.stroke,
        );
    }
    for node in &scene.nodes {
        paint_edges(canvas, &node.children, curve);
    }
}

fn paint_notes(canvas: &mut Canvas, skin: &Skin, scene: &Scene) {
    for note in &scene.notes {
        dog_ear(
            canvas,
            Rect {
                x: note.x,
                y: note.y,
                w: note.w,
                h: note.h,
            },
            skin.note.fill,
            Stroke::new(skin.note.border, skin.note.thickness),
        );
    }
    for node in &scene.nodes {
        paint_notes(canvas, skin, &node.children);
    }
}

fn paint_labels(canvas: &mut Canvas, scene: &Scene) {
    for edge in &scene.edges {
        canvas.text(Layer::Front, &edge.label, edge.label_at, Align::Left);
        if edge.points.len() < 2 {
            continue;
        }
        if !edge.from_card.is_empty() {
            let at = card_at(
                &edge.points,
                true,
                &edge.from_card,
                head_span(edge.from_head),
            );
            canvas.text(Layer::Front, &edge.from_card, at, Align::Left);
        }
        if !edge.to_card.is_empty() {
            let at = card_at(&edge.points, false, &edge.to_card, head_span(edge.to_head));
            canvas.text(Layer::Front, &edge.to_card, at, Align::Left);
        }
    }
    for note in &scene.notes {
        canvas.text(
            Layer::Front,
            &note.text,
            (note.x + NOTE_PAD, note.y + NOTE_PAD),
            Align::Left,
        );
    }
    for node in &scene.nodes {
        paint_labels(canvas, &node.children);
    }
}

fn paint_heads(canvas: &mut Canvas, skin: &Skin, scene: &Scene) {
    for edge in &scene.edges {
        if edge.points.len() < 2 {
            continue;
        }
        let last = edge.points.len() - 1;
        draw_head(
            canvas,
            skin,
            edge.points[0],
            edge.points[1],
            edge.from_head,
            edge.stroke,
        );
        draw_head(
            canvas,
            skin,
            edge.points[last],
            edge.points[last - 1],
            edge.to_head,
            edge.stroke,
        );
    }
    for node in &scene.nodes {
        paint_heads(canvas, skin, &node.children);
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
    if head == Head::None {
        return;
    }
    let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (vx * vx + vy * vy).sqrt();
    if len < 0.01 {
        return;
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
    let solid = Stroke {
        dash: None,
        width: stroke.width.min(1.5),
        ..stroke
    };
    match head {
        Head::None => {}
        Head::Arrow if skin.strict => {
            canvas.line(Layer::Front, left, tip, solid);
            canvas.line(Layer::Front, right, tip, solid);
        }
        Head::Arrow => {
            let inset = (tip.0 - ux * length * 0.55, tip.1 - uy * length * 0.55);
            canvas.polygon(
                Layer::Front,
                &[left, tip, right, inset],
                Some(stroke.color),
                Some(solid),
            );
        }
        Head::Triangle => {
            canvas.polygon(Layer::Front, &[left, tip, right], None, Some(solid));
        }
        Head::Diamond | Head::DiamondFilled => {
            // An empty diamond hides the line under it in the page's color.
            let fill = if head == Head::DiamondFilled {
                stroke.color
            } else {
                skin.background.unwrap_or(Color::WHITE)
            };
            canvas.polygon(
                Layer::Front,
                &[tip, left, base, right],
                Some(fill),
                Some(solid),
            );
        }
    }
}

fn card_at(points: &[(f32, f32)], at_start: bool, block: &Block, along: f32) -> (f32, f32) {
    let (tip, inner) = if at_start {
        (points[0], points[1])
    } else {
        let last = points.len() - 1;
        (points[last], points[last - 1])
    };
    let (vx, vy) = (inner.0 - tip.0, inner.1 - tip.1);
    let len = (vx * vx + vy * vy).sqrt().max(0.01);
    let travel = (along + 8.0).min(len * 0.65).max(6.0);
    let (ux, uy) = (vx / len, vy / len);
    let (px, py) = (-uy, ux);
    let off = block.height * 0.15 + 3.0;
    let cx = tip.0 + ux * travel + px * off;
    let cy = tip.1 + uy * travel + py * off;
    (cx - block.width / 2.0, cy - block.height / 2.0)
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;

    fn scene_of(source: &str) -> Scene {
        let (_, lines) = crate::source::body(source).unwrap();
        let diagram = parse(&lines).unwrap_or_else(|err| panic!("{err:?}"));
        let skin = Skin::default();
        let canvas = Canvas::new(&skin.font_name, false);
        layout_group(&diagram.root, &skin, canvas.family()).unwrap()
    }

    #[test]
    fn a_class_stays_inside_its_package() {
        let scene = scene_of("@startuml\npackage shop {\n  class Book\n}\n@enduml\n");
        let shop = &scene.nodes[0];
        assert!(shop.composite);
        let book = &shop.children.nodes[0];
        assert!(book.rect.x + 0.1 >= shop.rect.x);
        assert!(book.rect.y + 0.1 >= shop.rect.y);
        assert!(book.rect.x + book.rect.w <= shop.rect.x + shop.rect.w + 0.5);
        assert!(book.rect.y + book.rect.h <= shop.rect.y + shop.rect.h + 0.5);
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
        let tab = rects(&svg)
            .into_iter()
            .find(|rect| rect.x <= x + 0.6 && rect.x + rect.w + 0.6 >= x + w && rect.h < 40.0)
            .expect("a tab around the name");
        assert!(
            tab.w + 0.6 < rects(&svg).iter().map(|rect| rect.w).fold(0.0, f32::max),
            "the tab is the name, not the whole package"
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
        assert!(xs.windows(2).all(|pair| pair[1] - pair[0] > 4.0), "{xs:?}");
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

    fn rects(svg: &str) -> Vec<Rect> {
        svg.match_indices("<rect ")
            .map(|(at, _)| {
                let tag = &svg[at..svg[at..].find('>').map(|end| at + end).unwrap()];
                Rect {
                    x: attr(tag, "x"),
                    y: attr(tag, "y"),
                    w: attr(tag, "width"),
                    h: attr(tag, "height"),
                }
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
}
