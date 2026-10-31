//! Draws a state diagram, top to bottom or left to right. A nested state a
//! transition runs into from outside is laid out with the container
//! around it ([`open`]); any other is laid out on its own and becomes a
//! box around its children's layout. Notes are placed afterwards, in
//! screen space, so "right of" stays on the right, and they do not change
//! the box the parent measured.
//!
//! Concurrent regions are laid out one by one and stacked, a dashed line
//! between two. A transition into a state laid out on its own is ranked
//! in the container that holds both ends, as if it joined the boxes there,
//! and routed around everything once the whole diagram is placed.

use super::skin::Skin;
use super::{Container, Dash, Diagram, Direction, Kind, Node, Side, Transition};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::{self, Color, Paint};
use crate::graph::{self, Rect};
use crate::map::{Map, Route, Shape, Spot};
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{self, Block, Family, Font};

mod nest;
mod open;

use nest::{arrows, fill_regions, lay_by_hand, layout_regions, link_crossings, separators, wires};
use open::layout_open;

/// Space around the drawing.
const MARGIN: f32 = 10.0;
/// Inside a state's box, left and right of the name.
const PAD_X: f32 = 14.0;
/// Inside a state's box, above and below the name.
const PAD_Y: f32 = 8.0;
const MIN_W: f32 = 40.0;
const MIN_H: f32 = 28.0;
/// Inside a nested state's frame.
const COMP_PAD: f32 = 12.0;
/// Between a nested state's title and its children.
const COMP_GAP: f32 = 8.0;
const START: f32 = 22.0;
const END: f32 = 26.0;
/// A fork or join, long side then short side. The long side crosses the flow.
const BAR_LONG: f32 = 80.0;
const BAR_SHORT: f32 = 8.0;
const CHOICE_MIN: f32 = 34.0;
/// Between a note and its state.
const NOTE_GAP: f32 = 10.0;
const NOTE_PAD: f32 = 6.0;
/// Arrow heads, the same size as on an activity diagram.
const HEAD_LENGTH: f32 = 10.0;
const HEAD_HALF: f32 = 4.0;
/// How far a self-loop stands off its box. The same numbers as
/// [`crate::graph`]'s loop, so the reserved room fits the line.
const LOOP_REACH: f32 = 16.0;
const LOOP_STEP: f32 = 12.0;

pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    draw_mapped(diagram).map(|(svg, _)| svg)
}

/// The SVG, and where its states and transitions are in it.
pub fn draw_mapped(diagram: &Diagram) -> Result<(String, Map), Declined> {
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
    let mut scene = layout_container(&diagram.root, &skin, family)?;
    link_crossings(&diagram.root, &mut scene, &skin, family)?;
    let arrows = arrows(&diagram.root);
    let misses = lay_by_hand(&arrows, &mut scene, &diagram.frame.by_hand);
    scene.offset(0.0, annotations.top());
    paint(&mut canvas, &skin, &scene);
    annotations.draw(&mut canvas);
    let mut map = Map::default();
    chart(
        &diagram.root,
        &scene,
        None,
        (skin.round, skin.curve),
        &mut map,
    );
    map.wires = wires(&arrows, &scene, &diagram.frame.by_hand, skin.curve);
    map.misses = misses;
    let map = map.placed(canvas.fit(MARGIN));
    Ok((canvas.finish(MARGIN, skin.background), map))
}

/// Puts the states and transitions of `container`, laid out as `scene`
/// and held by the state named `parent`, on `map`. The scene holds the
/// container's nodes in their order, and its edges, then its crossings.
/// `round` is how round the states are and how far the lines curve.
fn chart(
    container: &Container,
    scene: &Scene,
    parent: Option<&str>,
    (round, curve): (f32, f32),
    map: &mut Map,
) {
    for (node, placed) in container.nodes.iter().zip(&scene.nodes) {
        let shape = match node.kind {
            Kind::State => Shape::State,
            Kind::Start => Shape::Start,
            Kind::End => Shape::End,
            Kind::Choice => Shape::Choice,
            Kind::Fork | Kind::Join => Shape::Bar,
            Kind::History | Kind::DeepHistory => Shape::History,
            Kind::Region => {
                chart(
                    &node.children,
                    &placed.children,
                    parent,
                    (round, curve),
                    map,
                );
                continue;
            }
        };
        let rect = placed.rect;
        let radius = match shape {
            Shape::State => round.min(rect.w / 2.0).min(rect.h / 2.0),
            Shape::Start | Shape::End | Shape::History => rect.w.min(rect.h) / 2.0,
            Shape::Bar => 2.0,
            Shape::Choice => 0.0,
        };
        map.states.push(Spot {
            name: node.key.clone(),
            shape,
            line: node.line,
            parent: parent.map(str::to_owned),
            rect,
            radius,
        });
        chart(
            &node.children,
            &placed.children,
            Some(&node.key),
            (round, curve),
            map,
        );
    }
    let arrows = container
        .edges
        .iter()
        .chain(container.crossings.iter().map(|c| &c.arrow));
    for (arrow, edge) in arrows.zip(&scene.edges) {
        map.transitions.push(Route {
            line: arrow.line,
            points: drawn_route(&edge.points, curve),
        });
    }
}

/// One container after layout. `width` and `height` are the graph, notes
/// excluded, which is what a parent sizes its box from.
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
    kind: Kind,
    fill: Paint,
    border: Stroke,
    /// Start, end and bars. A color of their own, else the state border,
    /// so a dark theme's accent stays visible.
    mark: Color,
    title: Block,
    description: Vec<Block>,
    /// The title band of a nested state; 0 for a leaf.
    header_h: f32,
    composite: bool,
    /// Laid out with the container around it, so its states and
    /// transitions are that container's too.
    opened: bool,
    children: Scene,
}

impl Placed {
    fn of(item: Measured, rect: Rect) -> Placed {
        Placed {
            rect,
            kind: item.kind,
            fill: item.fill,
            border: item.border,
            mark: item.mark,
            title: item.title,
            description: item.description,
            header_h: item.header_h,
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
    stroke: Stroke,
}

struct PlacedNote {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: Block,
}

struct Measured {
    width: f32,
    height: f32,
    kind: Kind,
    fill: Paint,
    border: Stroke,
    mark: Color,
    title: Block,
    description: Vec<Block>,
    header_h: f32,
    gap: f32,
    composite: bool,
    children: Scene,
}

fn layout_container(container: &Container, skin: &Skin, family: Family) -> Result<Scene, Declined> {
    if container.nodes.is_empty() {
        return Ok(Scene::empty());
    }
    if let Some(split) = container.split {
        return layout_regions(container, split, skin, family);
    }
    layout_open(container, skin, family)
}

/// A nested state the layout around it opens holds no layout of its own;
/// it is measured as if it held something.
fn measure_node(
    node: &Node,
    direction: Direction,
    every: &Skin,
    family: Family,
    open: bool,
) -> Result<Measured, Declined> {
    // A state of a stereotype has its own look; the states inside keep theirs.
    let skin = &*every.stereotyped(node.stereotype.as_deref());
    let (fill, border, mark) = paints(node, skin)?;
    if node.composite {
        let children = if open {
            Scene::empty()
        } else {
            layout_container(&node.children, every, family)?
        };
        let title = text_block(&node.label, name_font(skin), family)?;
        let description = text_blocks(&node.description, desc_font(skin), family)?;
        let header_h = stack_height(&title, &description);
        let header_w = stack_width(&title, &description);
        let gap = if !open && children.width == 0.0 && children.height == 0.0 {
            0.0
        } else {
            COMP_GAP
        };
        return Ok(Measured {
            width: header_w.max(children.width) + COMP_PAD * 2.0,
            height: COMP_PAD + header_h + gap + children.height + COMP_PAD,
            kind: node.kind,
            fill,
            border,
            mark,
            title,
            description,
            header_h,
            gap,
            composite: true,
            children,
        });
    }

    let (width, height, title, description) = match node.kind {
        Kind::State => {
            let title = text_block(&node.label, name_font(skin), family)?;
            let description = text_blocks(&node.description, desc_font(skin), family)?;
            let width = (stack_width(&title, &description) + PAD_X * 2.0).max(MIN_W);
            let height = (stack_height(&title, &description) + PAD_Y * 2.0).max(MIN_H);
            (width, height, title, description)
        }
        Kind::Choice => {
            let title = text_block(&node.label, skin.state.font, family)?;
            let width = (title.width * 2.0 + 16.0).max(CHOICE_MIN);
            let height = (title.height * 2.0 + 16.0).max(CHOICE_MIN);
            (width, height, title, Vec::new())
        }
        Kind::History | Kind::DeepHistory => {
            let word = if node.kind == Kind::DeepHistory {
                "H*"
            } else {
                "H"
            };
            let title = text::block(word, name_font(skin), family)
                .map_err(|err| Declined::at(node.line, err))?;
            let diameter = title.width.max(title.height).max(12.0) + 16.0;
            (diameter, diameter, title, Vec::new())
        }
        Kind::Fork | Kind::Join => {
            let (width, height) = match direction {
                Direction::Down => (BAR_LONG, BAR_SHORT),
                Direction::Right => (BAR_SHORT, BAR_LONG),
            };
            (width, height, Block::empty(), Vec::new())
        }
        Kind::Start => (START, START, Block::empty(), Vec::new()),
        Kind::End => (END, END, Block::empty(), Vec::new()),
        // A region is laid out by its state, never as a node of its own.
        Kind::Region => (0.0, 0.0, Block::empty(), Vec::new()),
    };
    Ok(Measured {
        width,
        height,
        kind: node.kind,
        fill,
        border,
        mark,
        title,
        description,
        header_h: 0.0,
        gap: 0.0,
        composite: false,
        children: Scene::empty(),
    })
}

fn paints(node: &Node, skin: &Skin) -> Result<(Paint, Stroke, Color), Declined> {
    // A color written on the state. Start, end and bars use it when it is
    // there; otherwise they use the border, not the pale state fill, so
    // they stay visible on the background.
    let own = match &node.fill {
        Some(value) => color::parse(value).map_err(|err| Declined::at(node.line, err))?,
        None => None,
    };
    let fill = if node.fill.is_some() {
        own
    } else {
        skin.state.fill
    };
    let border_color = match &node.line_color {
        Some(value) => match color::parse(value).map_err(|err| Declined::at(node.line, err))? {
            Some(color) => color,
            None => {
                return Err(Declined::at(node.line, "a border must not be transparent"));
            }
        },
        None => skin.state.border,
    };
    let width = if node.border_bold {
        skin.state.thickness * 2.0
    } else {
        skin.state.thickness
    };
    let mut border = Stroke::new(border_color, width);
    border.dash = match node.border_dash {
        Dash::Solid => None,
        Dash::Dashed => Some((7.0, 5.0)),
        Dash::Dotted => Some((1.5, 3.0)),
    };
    let mark = own.unwrap_or(border_color);
    Ok((fill, border, mark))
}

fn edge_label(edge: &Transition, skin: &Skin, family: Family) -> Result<Block, Declined> {
    match &edge.label {
        Some(text) => text_block(text, skin.arrow_font, family),
        None => Ok(Block::empty()),
    }
}

fn arrow_stroke(skin: &Skin, edge: &Transition) -> Result<Stroke, Declined> {
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
        Dash::Dotted => Some((1.0, 3.0)),
        Dash::Solid => skin.arrow.dash,
    };
    Ok(stroke)
}

fn place_notes(
    container: &Container,
    rects: &[Rect],
    edges: &[PlacedEdge],
    skin: &Skin,
    family: Family,
) -> Result<Vec<PlacedNote>, Declined> {
    struct Draft {
        target: usize,
        side: Side,
        w: f32,
        h: f32,
        text: Block,
    }
    let mut sized = Vec::with_capacity(container.notes.len());
    for note in &container.notes {
        if note.target >= rects.len() {
            return Err(Declined::at(note.line, "a note needs a state"));
        }
        let text = text_block(&note.text, skin.note.font, family)?;
        let (w, h) = note_size(&text);
        sized.push(Draft {
            target: note.target,
            side: note.side,
            w,
            h,
            text,
        });
    }
    let mut groups: Vec<(usize, Side, Vec<usize>)> = Vec::new();
    for (index, note) in sized.iter().enumerate() {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.0 == note.target && group.1 == note.side)
        {
            group.2.push(index);
        } else {
            groups.push((note.target, note.side, vec![index]));
        }
    }
    let mut placed = Vec::with_capacity(sized.len());
    for (target, side, indices) in groups {
        // A self-loop sits on the free side of its state. The note keeps
        // off that line, and still on the side that was asked for.
        let rect = note_anchor(rects[target], edges, &container.edges, target);
        let mut cursor = match side {
            Side::Top => rect.y - NOTE_GAP,
            Side::Bottom => rect.y + rect.h + NOTE_GAP,
            Side::Left | Side::Right => rect.y,
        };
        for index in indices {
            let note = &mut sized[index];
            let (x, y) = match side {
                Side::Right => (rect.x + rect.w + NOTE_GAP, cursor),
                Side::Left => (rect.x - NOTE_GAP - note.w, cursor),
                Side::Bottom => (rect.x + (rect.w - note.w) / 2.0, cursor),
                Side::Top => {
                    cursor -= note.h;
                    (rect.x + (rect.w - note.w) / 2.0, cursor)
                }
            };
            if side != Side::Top {
                cursor += note.h + 6.0;
            } else {
                cursor -= 6.0;
            }
            placed.push(PlacedNote {
                x,
                y,
                w: note.w,
                h: note.h,
                text: std::mem::replace(&mut note.text, Block::empty()),
            });
            let last = placed.len() - 1;
            // An edge label can sit in the same gap. Step the note off it,
            // onto free paper rather than onto another state.
            clear_note(&mut placed[last], side, edges, rects);
        }
    }
    Ok(placed)
}

/// Moves a note until its words no longer cover an edge label. Of the
/// steps that stay on the asked-for side, the one that covers the least
/// of another state wins, and a shorter step breaks a tie.
fn clear_note(note: &mut PlacedNote, side: Side, edges: &[PlacedEdge], rects: &[Rect]) {
    for _ in 0..6 {
        let text = note_text_box(note);
        let Some(label) = edges.iter().find_map(|edge| {
            if edge.label.is_empty() {
                return None;
            }
            let label = label_box(edge);
            boxes_overlap(text, label).then_some(label)
        }) else {
            break;
        };
        let (dx, dy) = note_dodge(side, note, text, label, rects);
        if dx == 0.0 && dy == 0.0 {
            break;
        }
        note.x += dx;
        note.y += dy;
    }
}

fn note_text_box(note: &PlacedNote) -> (f32, f32, f32, f32) {
    (
        note.x + NOTE_PAD,
        note.y + NOTE_PAD,
        note.text.width,
        note.text.height,
    )
}

fn boxes_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    let (x, y, w, h) = a;
    let (ox, oy, ow, oh) = b;
    x < ox + ow && ox < x + w && y < oy + oh && oy < y + h
}

/// `(dx, dy)` that puts the text box clear of the label box.
fn note_dodge(
    side: Side,
    note: &PlacedNote,
    text: (f32, f32, f32, f32),
    label: (f32, f32, f32, f32),
    rects: &[Rect],
) -> (f32, f32) {
    const GAP: f32 = 4.0;
    let (x, y, w, h) = text;
    let (lx, ly, lw, lh) = label;
    let up = (0.0, -(y + h - ly + GAP));
    let down = (0.0, ly + lh - y + GAP);
    let left = (-(x + w - lx + GAP), 0.0);
    let right = (lx + lw - x + GAP, 0.0);
    let steps = match side {
        Side::Right => [up, down, right],
        Side::Left => [up, down, left],
        Side::Bottom => [left, right, down],
        Side::Top => [left, right, up],
    };
    let mut best = steps[0];
    let mut best_area = f32::MAX;
    let mut best_len = f32::MAX;
    for (dx, dy) in steps {
        let area = rects
            .iter()
            .map(|rect| {
                overlap_area(
                    (note.x + dx, note.y + dy, note.w, note.h),
                    (rect.x, rect.y, rect.w, rect.h),
                )
            })
            .sum::<f32>();
        let len = dx.abs() + dy.abs();
        if area < best_area - 0.5 || ((area - best_area).abs() <= 0.5 && len < best_len) {
            best = (dx, dy);
            best_area = area;
            best_len = len;
        }
    }
    best
}

fn overlap_area(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let (x, y, w, h) = a;
    let (ox, oy, ow, oh) = b;
    let width = (x + w).min(ox + ow) - x.max(ox);
    let height = (y + h).min(oy + oh) - y.max(oy);
    if width > 0.0 && height > 0.0 {
        width * height
    } else {
        0.0
    }
}

/// The box a note stands off: the state, plus a self-loop and its label
/// on the free side, so the note does not cover that line.
/// A note's sheet around its text.
fn note_size(text: &Block) -> (f32, f32) {
    (
        (text.width + NOTE_PAD * 2.0 + 4.0).max(18.0),
        (text.height + NOTE_PAD * 2.0).max(18.0),
    )
}

fn note_anchor(
    rect: Rect,
    edges: &[PlacedEdge],
    transitions: &[Transition],
    target: usize,
) -> Rect {
    let mut x0 = rect.x;
    let mut y0 = rect.y;
    let mut x1 = rect.x + rect.w;
    let mut y1 = rect.y + rect.h;
    for (edge, transition) in edges.iter().zip(transitions) {
        if transition.from != target || transition.to != target {
            continue;
        }
        for &(x, y) in &edge.points {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        if edge.label.width > 0.0 || edge.label.height > 0.0 {
            x0 = x0.min(edge.label_at.0);
            y0 = y0.min(edge.label_at.1);
            x1 = x1.max(edge.label_at.0 + edge.label.width);
            y1 = y1.max(edge.label_at.1 + edge.label.height);
        }
    }
    Rect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0.0),
        h: (y1 - y0).max(0.0),
    }
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

fn name_font(skin: &Skin) -> Font {
    skin.state.font.bold()
}

fn desc_font(skin: &Skin) -> Font {
    Font {
        size: (skin.state.font.size - 2.0).max(4.0),
        ..skin.state.font
    }
}

fn text_block(text: &super::Text, font: Font, family: Family) -> Result<Block, Declined> {
    if text.value.is_empty() {
        return Ok(Block::empty());
    }
    text::block(&text.value, font, family).map_err(|err| Declined::at(text.line, err))
}

fn text_blocks(items: &[super::Text], font: Font, family: Family) -> Result<Vec<Block>, Declined> {
    items
        .iter()
        .map(|text| text_block(text, font, family))
        .collect()
}

fn stack_width(title: &Block, description: &[Block]) -> f32 {
    description
        .iter()
        .fold(title.width, |width, block| width.max(block.width))
}

fn stack_height(title: &Block, description: &[Block]) -> f32 {
    description
        .iter()
        .fold(title.height, |height, block| height + block.height)
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
            paint_divider(canvas, node);
            for (a, b) in separators(node) {
                let mut stroke = Stroke::new(node.border.color, node.border.width.max(0.5));
                stroke.dash = Some((5.0, 4.0));
                canvas.line(Layer::Frames, a, b, stroke);
            }
            paint_shapes(canvas, skin, &node.children);
        }
    }
}

fn paint_shape(canvas: &mut Canvas, skin: &Skin, node: &Placed) {
    let rect = node.rect;
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    match node.kind {
        Kind::State => {
            let radius = skin.round.min(rect.w / 2.0).min(rect.h / 2.0);
            canvas.rect(
                Layer::Frames,
                (rect.x, rect.y, rect.w, rect.h),
                radius,
                node.fill,
                Some(node.border),
            );
        }
        Kind::Choice => {
            let points = [
                (cx, rect.y),
                (rect.x + rect.w, cy),
                (cx, rect.y + rect.h),
                (rect.x, cy),
            ];
            canvas.polygon(Layer::Frames, &points, node.fill, Some(node.border));
        }
        Kind::Fork | Kind::Join => {
            canvas.rect(
                Layer::Frames,
                (rect.x, rect.y, rect.w, rect.h),
                2.0,
                Some(node.mark),
                None,
            );
        }
        Kind::History | Kind::DeepHistory => {
            canvas.ellipse(
                Layer::Frames,
                (cx, cy),
                (rect.w / 2.0, rect.h / 2.0),
                node.fill,
                Some(node.border),
            );
        }
        Kind::Start => {
            let stroke = Stroke::new(node.mark, 1.0);
            canvas.ellipse(
                Layer::Frames,
                (cx, cy),
                (rect.w / 2.0, rect.h / 2.0),
                Some(node.mark),
                Some(stroke),
            );
        }
        Kind::End => {
            let stroke = Stroke::new(node.mark, 1.5);
            canvas.ellipse(
                Layer::Frames,
                (cx, cy),
                (rect.w / 2.0, rect.h / 2.0),
                None,
                Some(stroke),
            );
            canvas.ellipse(
                Layer::Frames,
                (cx, cy),
                (rect.w * 0.21, rect.h * 0.21),
                Some(node.mark),
                Some(Stroke::new(node.mark, 1.0)),
            );
        }
        Kind::Region => {}
    }
}

fn paint_divider(canvas: &mut Canvas, node: &Placed) {
    if node.header_h <= 0.0 {
        return;
    }
    let y = node.rect.y + COMP_PAD + node.header_h;
    canvas.line(
        Layer::Frames,
        (node.rect.x, y),
        (node.rect.x + node.rect.w, y),
        Stroke::new(node.border.color, node.border.width.max(0.5)),
    );
}

fn paint_edges(canvas: &mut Canvas, scene: &Scene, curve: f32) {
    for edge in &scene.edges {
        if edge.points.len() < 2 {
            continue;
        }
        canvas.curve(Layer::Frames, &shortened(&edge.points), curve, edge.stroke);
    }
    for node in &scene.nodes {
        paint_edges(canvas, &node.children, curve);
    }
}

fn paint_notes(canvas: &mut Canvas, skin: &Skin, scene: &Scene) {
    for note in &scene.notes {
        note_box(canvas, skin, note);
    }
    for node in &scene.nodes {
        paint_notes(canvas, skin, &node.children);
    }
}

fn note_box(canvas: &mut Canvas, skin: &Skin, note: &PlacedNote) {
    let (x, y, w, h) = (note.x, note.y, note.w, note.h);
    let stroke = Some(Stroke::new(skin.note.border, skin.note.thickness));
    let fold = 10.0_f32.min(w / 2.0).min(h / 2.0);
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
        Layer::Front,
        &body,
        (x, y, x + w, y + h),
        skin.note.fill,
        stroke,
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
        Layer::Front,
        &corner,
        (x, y, x + w, y + h),
        skin.note.fill,
        stroke,
    );
}

fn paint_labels(canvas: &mut Canvas, scene: &Scene) {
    for node in &scene.nodes {
        match node.kind {
            Kind::State if node.composite => {
                paint_stack(
                    canvas,
                    &node.title,
                    &node.description,
                    node.rect.x + node.rect.w / 2.0,
                    node.rect.y + COMP_PAD,
                );
            }
            Kind::State => {
                let height = stack_height(&node.title, &node.description);
                paint_stack(
                    canvas,
                    &node.title,
                    &node.description,
                    node.rect.x + node.rect.w / 2.0,
                    node.rect.y + (node.rect.h - height) / 2.0,
                );
            }
            Kind::Choice | Kind::History | Kind::DeepHistory => {
                let y = node.rect.y + (node.rect.h - node.title.height) / 2.0;
                canvas.text(
                    Layer::Front,
                    &node.title,
                    (node.rect.x + node.rect.w / 2.0, y),
                    Align::Center,
                );
            }
            Kind::Fork | Kind::Join | Kind::Start | Kind::End | Kind::Region => {}
        }
        paint_labels(canvas, &node.children);
    }
    for edge in &scene.edges {
        canvas.text(Layer::Front, &edge.label, edge.label_at, Align::Left);
    }
    for note in &scene.notes {
        canvas.text(
            Layer::Front,
            &note.text,
            (note.x + NOTE_PAD, note.y + NOTE_PAD),
            Align::Left,
        );
    }
}

fn paint_stack(canvas: &mut Canvas, title: &Block, description: &[Block], x: f32, mut y: f32) {
    if !title.is_empty() {
        canvas.text(Layer::Front, title, (x, y), Align::Center);
        y += title.height;
    }
    for block in description {
        canvas.text(Layer::Front, block, (x, y), Align::Center);
        y += block.height;
    }
}

fn paint_heads(canvas: &mut Canvas, skin: &Skin, scene: &Scene) {
    for edge in &scene.edges {
        arrow_head(canvas, skin, &edge.points, edge.stroke);
    }
    for node in &scene.nodes {
        paint_heads(canvas, skin, &node.children);
    }
}

/// The line stopped short of the border, so the head's tip sits on it.
/// The way a transition is drawn: its curve up to the head, then on to
/// the tip, for a light to run along.
fn drawn_route(points: &[(f32, f32)], curve: f32) -> Vec<(f32, f32)> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let mut route = crate::curve::flatten(&shortened(points), curve);
    route.push(points[points.len() - 1]);
    route.dedup();
    route
}

fn shortened(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut line = points.to_vec();
    let tip = line[line.len() - 1];
    let from = line[line.len() - 2];
    let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (vx * vx + vy * vy).sqrt();
    if len > HEAD_LENGTH + 0.5 {
        let scale = HEAD_LENGTH / len;
        let last = line.len() - 1;
        line[last] = (tip.0 - vx * scale, tip.1 - vy * scale);
    }
    line
}

fn arrow_head(canvas: &mut Canvas, skin: &Skin, points: &[(f32, f32)], stroke: Stroke) {
    if points.len() < 2 {
        return;
    }
    let tip = points[points.len() - 1];
    let from = points[points.len() - 2];
    let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (vx * vx + vy * vy).sqrt();
    if len < 0.01 {
        return;
    }
    let (ux, uy) = (vx / len, vy / len);
    let back = (tip.0 - ux * HEAD_LENGTH, tip.1 - uy * HEAD_LENGTH);
    let side = (-uy * HEAD_HALF, ux * HEAD_HALF);
    let solid = Stroke {
        dash: None,
        width: stroke.width.min(1.5),
        ..stroke
    };
    let mut head = vec![
        (back.0 + side.0, back.1 + side.1),
        tip,
        (back.0 - side.0, back.1 - side.1),
    ];
    if !skin.strict {
        head.push((tip.0 - ux * 6.0, tip.1 - uy * 6.0));
    }
    canvas.polygon(Layer::Front, &head, Some(stroke.color), Some(solid));
}

#[cfg(test)]
mod tests {
    use super::super::Split;
    use super::super::parse::parse;
    use super::*;

    fn laid(source: &str) -> (Container, Scene) {
        let (_, lines) = crate::source::body(source).unwrap();
        let diagram = parse(&lines).unwrap_or_else(|err| panic!("{err:?}"));
        let skin = Skin::default();
        let canvas = Canvas::new(&skin.font_name, false);
        let scene = layout_container(&diagram.root, &skin, canvas.family()).unwrap();
        (diagram.root, scene)
    }

    /// The model node and the box laid out for it. Same index in both.
    fn at<'a>(nodes: &'a [Node], scene: &'a Scene, key: &str) -> (&'a Node, &'a Placed) {
        let index = nodes
            .iter()
            .position(|node| node.key == key)
            .unwrap_or_else(|| panic!("no {key}"));
        (&nodes[index], &scene.nodes[index])
    }

    fn by_kind(scene: &Scene, kind: Kind) -> &Placed {
        scene
            .nodes
            .iter()
            .find(|node| node.kind == kind)
            .unwrap_or_else(|| panic!("no {kind:?}"))
    }

    fn bottom(rect: Rect) -> f32 {
        rect.y + rect.h
    }

    fn right(rect: Rect) -> f32 {
        rect.x + rect.w
    }

    /// A child sitting inside its parent is the nested state, not a clash.
    fn overlaps(a: Rect, b: Rect) -> bool {
        let crosses = a.x < right(b) - 0.5
            && b.x < right(a) - 0.5
            && a.y < bottom(b) - 0.5
            && b.y < bottom(a) - 0.5;
        if !crosses {
            return false;
        }
        let holds = |outer: Rect, inner: Rect| {
            inner.x >= outer.x - 0.5
                && right(inner) <= right(outer) + 0.5
                && inner.y >= outer.y - 0.5
                && bottom(inner) <= bottom(outer) + 0.5
        };
        !(holds(a, b) || holds(b, a))
    }

    fn collect(scene: &Scene, out: &mut Vec<Rect>) {
        for node in &scene.nodes {
            out.push(node.rect);
            collect(&node.children, out);
        }
    }

    /// Lines turn in curves, as PlantUML draws them, and a play's light
    /// runs along the curve; `skinparam linetype ortho` keeps the corners.
    #[test]
    fn transitions_curve_unless_the_linetype_is_ortho() {
        let source = "@startuml\n[*] --> A\nA --> B : go\nB --> A : back\nB --> C\nC --> A : again\n@enduml\n";
        let (curved, map) = crate::render_mapped(source, None).unwrap();
        let curves = |svg: &str| {
            svg.split("<path d=\"")
                .skip(1)
                .any(|rest| rest.split('"').next().is_some_and(|d| d.contains(" C")))
        };
        assert!(curves(&curved), "{curved}");
        // The light's way has the curve's points, more than the corners.
        let (ortho_svg, ortho_map) = crate::render_mapped(
            &source.replace("@startuml\n", "@startuml\nskinparam linetype ortho\n"),
            None,
        )
        .unwrap();
        assert!(!curves(&ortho_svg), "{ortho_svg}");
        let bent =
            |m: &crate::map::Map| m.transitions.iter().map(|r| r.points.len()).sum::<usize>();
        assert!(bent(&map) > bent(&ortho_map));
        for (curve, corner) in map.transitions.iter().zip(&ortho_map.transitions) {
            assert_eq!(curve.points.first(), corner.points.first());
            assert_eq!(curve.points.last(), corner.points.last());
        }
        let err = crate::render(
            &source.replace("@startuml\n", "@startuml\nskinparam linetype splines\n"),
        )
        .unwrap_err();
        assert!(err.reason.contains("linetype"), "{}", err.reason);
    }

    /// Whether the run of `points` passes within half a unit of `p`.
    fn passes(points: &[(f32, f32)], p: (f32, f32)) -> bool {
        points.windows(2).any(|w| {
            let ((ax, ay), (bx, by)) = (w[0], w[1]);
            let (dx, dy) = (bx - ax, by - ay);
            let t = (((p.0 - ax) * dx + (p.1 - ay) * dy) / (dx * dx + dy * dy).max(1e-6))
                .clamp(0.0, 1.0);
            (ax + t * dx - p.0).hypot(ay + t * dy - p.1) < 0.5
        })
    }

    const BY_HAND: &str = "@startuml\nskinparam linetype ortho\n[*] --> Idle\nIdle --> Running : start\nRunning --> Idle : stop\nRunning --> Idle : fail\nstate Busy {\n  [*] --> Work\n  Work --> [H]\n}\nIdle --> Busy\n\n' lines by hand (Omaspec)\n' Running -> Idle via 0.5 -140\n' Running -> Idle #2 via 0 0\n' Nowhere -> Idle via 0.5 10\n' Idle -> Busy via 0.5 -100\n' Work -> Busy[H] via 0.5 5000\n@enduml\n";

    #[test]
    fn a_transition_by_hand_runs_through_its_bends() {
        let (_, map) = crate::render_mapped(BY_HAND, None).unwrap();
        let keys: Vec<String> = map.wires.iter().map(|w| w.key.to_string()).collect();
        for key in [
            "[*] -> Idle",
            "Idle -> Running",
            "Running -> Idle",
            "Running -> Idle #2",
            "Idle -> Busy",
            "[*] -> Work",
            "Work -> Busy[H]",
        ] {
            assert!(keys.iter().any(|k| k == key), "{key} not in {keys:?}");
        }
        let wire = |key: &str| map.wires.iter().find(|w| w.key.to_string() == key).unwrap();
        let stop = wire("Running -> Idle");
        assert_eq!((stop.line, stop.entry), (Some(4), Some(13)));
        let bend = stop.bends(map.fit)[0];
        assert!(passes(&stop.points, bend), "{bend:?} {:?}", stop.points);
        // Its bend lies left of the way up from Running to Idle.
        assert!(bend.0 < stop.from.0.min(stop.to.0) - 100.0, "{bend:?}");
        // A bend inside Running's own box, a line that is not there.
        assert_eq!(
            map.misses,
            [
                (
                    14,
                    "A bend of Running -> Idle #2 lies in a box and is left out".to_owned()
                ),
                (
                    15,
                    "No line runs as Nowhere -> Idle; its bends are left out".to_owned()
                ),
                (
                    17,
                    "A bend of Work -> Busy[H] lies far off the diagram and is left out".to_owned()
                ),
            ]
        );
        // Without the block, the same lines run their own ways.
        let plain = BY_HAND.split("\n\n'").next().unwrap().to_owned() + "\n@enduml\n";
        let (_, before) = crate::render_mapped(&plain, None).unwrap();
        assert!(before.misses.is_empty() && before.wires.iter().all(|w| w.entry.is_none()));
        let stop_before = before.wires.iter().find(|w| w.key == stop.key).unwrap();
        assert_ne!(stop_before.points, stop.points);
        // The others stay as the layout drew them, where the picture grew
        // for the bend.
        let start = |m: &crate::map::Map| {
            let w = m
                .wires
                .iter()
                .find(|w| w.key.to_string() == "[*] -> Idle")
                .unwrap();
            let to = w.to;
            w.points
                .iter()
                .map(|p| ((p.0 - to.0).round(), (p.1 - to.1).round()))
                .collect::<Vec<_>>()
        };
        assert_eq!(start(&before), start(&map));
    }

    #[test]
    fn a_bend_moved_back_where_it_was_drawn_finds_it_again() {
        let (_, map) = crate::render_mapped(BY_HAND, None).unwrap();
        for wire in map.wires.iter().filter(|w| w.entry.is_some()) {
            for (via, at) in wire.vias.iter().zip(wire.bends(map.fit)) {
                let back = wire.via_at(at, map.fit);
                assert!((back.along - via.along).abs() < 1e-3, "{back:?} {via:?}");
                assert!((back.aside - via.aside).abs() < 1e-2, "{back:?} {via:?}");
            }
        }
    }

    #[test]
    fn the_player_steps_down_and_loops_beside_playing() {
        let (root, scene) = laid(include_str!("../../../../examples/player.puml"));
        assert_eq!(scene.nodes.len(), 5);
        assert_eq!(scene.edges.len(), 8);
        let start = by_kind(&scene, Kind::Start);
        let (_, stopped) = at(&root.nodes, &scene, "Stopped");
        let (_, playing) = at(&root.nodes, &scene, "Playing");
        let (_, paused) = at(&root.nodes, &scene, "Paused");
        let end = by_kind(&scene, Kind::End);
        assert!(stopped.rect.y + 0.1 >= bottom(start.rect));
        assert!(playing.rect.y + 0.1 >= bottom(stopped.rect));
        assert!(paused.rect.y + 0.1 >= bottom(playing.rect));
        assert!(end.rect.y + 0.5 >= stopped.rect.y);
        let playing_at = root
            .nodes
            .iter()
            .position(|node| node.key == "Playing")
            .unwrap();
        let looped = root
            .edges
            .iter()
            .zip(&scene.edges)
            .find(|(edge, _)| edge.from == edge.to && edge.from == playing_at)
            .unwrap()
            .1;
        let far = looped
            .points
            .iter()
            .map(|point| point.0)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(far > right(playing.rect));
        let mut rects = Vec::new();
        collect(&scene, &mut rects);
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                assert!(!overlaps(*a, *b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn a_nested_state_holds_its_own_states() {
        let (root, scene) = laid(include_str!(
            "../../../../crates/omaspec-core/tests/fixtures/state-player.puml"
        ));
        let (buffering, placed) = at(&root.nodes, &scene, "Buffering");
        let (_, waiting) = at(&buffering.children.nodes, &placed.children, "Waiting");
        assert!(waiting.rect.x >= placed.rect.x - 0.5);
        assert!(right(waiting.rect) <= right(placed.rect) + 0.5);
        assert!(waiting.rect.y >= placed.rect.y - 0.5);
        assert!(bottom(waiting.rect) <= bottom(placed.rect) + 0.5);
        let svg = crate::render(include_str!(
            "../../../../crates/omaspec-core/tests/fixtures/state-player.puml"
        ))
        .unwrap();
        assert!(svg.contains("Waiting"));
        assert!(svg.contains("Playing track"));
        assert!(svg.contains("waits for input"));
        assert!(svg.contains("Stoped"));
    }

    #[test]
    fn a_note_stays_on_the_right_when_the_flow_runs_across() {
        let (root, scene) =
            laid("@startuml\nleft to right direction\nstate A\nnote right of A : here\n@enduml\n");
        let (_, node) = at(&root.nodes, &scene, "A");
        assert_eq!(scene.notes.len(), 1);
        assert!(scene.notes[0].x > right(node.rect));
    }

    #[test]
    fn a_note_clears_the_self_loop_beside_its_state() {
        let (root, scene) =
            laid("@startuml\nstate A\nA --> A : loop\nnote right of A : here\n@enduml\n");
        let index = root.nodes.iter().position(|node| node.key == "A").unwrap();
        let looped = root
            .edges
            .iter()
            .zip(&scene.edges)
            .find(|(edge, _)| edge.from == edge.to && edge.from == index)
            .unwrap()
            .1;
        let note = &scene.notes[0];
        let label_right = looped.label_at.0 + looped.label.width;
        assert!(note.x + 0.1 >= label_right);
        let text = note_text_box(note);
        assert!(!boxes_overlap(
            text,
            (
                looped.label_at.0,
                looped.label_at.1,
                looped.label.width,
                looped.label.height,
            ),
        ));
    }

    #[test]
    fn a_note_keeps_off_the_label_on_the_edge_beside_it() {
        let (root, scene) = laid(include_str!("../../tests/state/sideways.puml"));
        let (_, busy) = at(&root.nodes, &scene, "Busy");
        let (_, done) = at(&root.nodes, &scene, "Done");
        assert_eq!(scene.notes.len(), 1);
        let note = &scene.notes[0];
        assert!(note.x + 0.1 >= right(busy.rect));
        // A hairline at the top stroke can remain. The sheet must not sit on Done.
        let overlap_h = (note.y + note.h).min(bottom(done.rect)) - note.y.max(done.rect.y);
        assert!(overlap_h < 1.0, "note covers Done by {overlap_h}");
        let text = note_text_box(note);
        for edge in &scene.edges {
            if edge.label.is_empty() {
                continue;
            }
            assert!(
                !boxes_overlap(text, label_box(edge)),
                "note covers {:?}",
                label_box(edge)
            );
        }
    }

    #[test]
    fn a_nested_state_keeps_its_own_direction() {
        let (root, scene) = laid(
            "@startuml\n[*] --> Box\nstate Box {\n  left to right direction\n  [*] --> A\n  A --> B\n}\n@enduml\n",
        );
        let start = by_kind(&scene, Kind::Start);
        let (boxed, placed) = at(&root.nodes, &scene, "Box");
        assert!(placed.rect.y + 0.5 >= bottom(start.rect));
        let (_, a) = at(&boxed.children.nodes, &placed.children, "A");
        let (_, b) = at(&boxed.children.nodes, &placed.children, "B");
        assert!(b.rect.x > right(a.rect) - 0.1);
    }

    #[test]
    fn regions_stack_inside_their_state_with_a_line_between() {
        let (root, scene) = laid(include_str!("../../tests/state/regions.puml"));
        let (active, placed) = at(&root.nodes, &scene, "Active");
        assert_eq!(active.children.split, Some(Split::Stacked));
        let regions = &placed.children.nodes;
        assert_eq!(regions.len(), 2);
        assert_eq!(separators(placed).len(), 1);
        let holds = |outer: Rect, inner: Rect| {
            inner.x >= outer.x - 0.5
                && right(inner) <= right(outer) + 0.5
                && inner.y >= outer.y - 0.5
                && bottom(inner) <= bottom(outer) + 0.5
        };
        for region in regions {
            assert!(holds(placed.rect, region.rect), "{:?}", region.rect);
            for state in &region.children.nodes {
                assert!(holds(region.rect, state.rect), "{:?}", state.rect);
            }
        }
        assert!(regions[1].rect.y >= bottom(regions[0].rect) - 0.5);
        let mut rects = Vec::new();
        collect(&scene, &mut rects);
        let boxes: Vec<Rect> = rects.into_iter().filter(|r| r.w > 0.0).collect();
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                assert!(!overlaps(*a, *b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn regions_side_by_side_split_across() {
        let (root, scene) =
            laid("@startuml\nstate A {\n  [*] --> B\n  ||\n  [*] --> C\n}\n@enduml\n");
        let (_, placed) = at(&root.nodes, &scene, "A");
        let regions = &placed.children.nodes;
        assert!(regions[1].rect.x >= right(regions[0].rect) - 0.5);
        let (a, b) = separators(placed)[0];
        assert_eq!(a.0, b.0, "an upright line");
    }

    fn linked(source: &str) -> (Container, Scene) {
        let (_, lines) = crate::source::body(source).unwrap();
        let diagram = parse(&lines).unwrap_or_else(|err| panic!("{err:?}"));
        let skin = Skin::default();
        let canvas = Canvas::new(&skin.font_name, false);
        let mut scene = layout_container(&diagram.root, &skin, canvas.family()).unwrap();
        link_crossings(&diagram.root, &mut scene, &skin, canvas.family()).unwrap();
        (diagram.root, scene)
    }

    fn touches(rect: Rect, (x, y): (f32, f32)) -> bool {
        let near = |a: f32, b: f32| (a - b).abs() < 0.1;
        let within_x = x >= rect.x - 0.1 && x <= right(rect) + 0.1;
        let within_y = y >= rect.y - 0.1 && y <= bottom(rect) + 0.1;
        (within_x && (near(y, rect.y) || near(y, bottom(rect))))
            || (within_y && (near(x, rect.x) || near(x, right(rect))))
    }

    #[test]
    fn a_transition_into_a_nested_state_goes_around_the_rest() {
        let (root, scene) = linked(include_str!("../../tests/state/crossings.puml"));
        assert_eq!(root.crossings.len(), 2);
        let (_, off) = at(&root.nodes, &scene, "Off");
        let (on, placed) = at(&root.nodes, &scene, "On");
        let (_, idle) = at(&on.children.nodes, &placed.children, "Idle");
        let wake = scene
            .edges
            .iter()
            .find(|edge| block_text(&edge.label) == "wake")
            .expect("the crossing is drawn at the top");
        let points = &wake.points;
        assert!(touches(off.rect, points[0]), "{points:?}");
        assert!(touches(idle.rect, *points.last().unwrap()), "{points:?}");
        let mut rects = Vec::new();
        collect(&scene, &mut rects);
        for rect in rects {
            if rect == off.rect || rect == idle.rect || rect == placed.rect || rect.w == 0.0 {
                continue;
            }
            for pair in points.windows(2) {
                assert!(
                    !graph::route::cuts(rect, pair[0], pair[1]),
                    "{points:?} cuts {rect:?}"
                );
            }
        }
        for pair in points.windows(2) {
            assert!(
                pair[0].0 == pair[1].0 || pair[0].1 == pair[1].1,
                "{points:?}"
            );
        }
    }

    /// Laid out with the states around it, a nested state is entered from
    /// above, beside its name, and a transition to the nested state itself
    /// ends on its border, apart from the one that passes into it.
    #[test]
    fn a_transition_into_a_nested_state_comes_down_through_its_top() {
        let (root, scene) = laid(include_str!("../../tests/state/crossings.puml"));
        let (on, placed) = at(&root.nodes, &scene, "On");
        assert!(placed.opened);
        let (_, idle) = at(&on.children.nodes, &placed.children, "Idle");
        let line = |word: &str| {
            scene
                .edges
                .iter()
                .find(|edge| block_text(&edge.label) == word)
                .unwrap_or_else(|| panic!("{word}"))
        };
        let wake = &line("wake").points;
        let tip = *wake.last().unwrap();
        assert!((tip.1 - idle.rect.y).abs() < 0.1, "{wake:?}");
        let name = Rect {
            x: placed.rect.x + (placed.rect.w - placed.title.width) / 2.0,
            w: placed.title.width,
            h: COMP_PAD + placed.header_h,
            ..placed.rect
        };
        for pair in wake.windows(2) {
            assert!(!graph::route::cuts(name, pair[0], pair[1]), "{wake:?}");
        }
        let power = &line("power").points;
        let border = *power.last().unwrap();
        assert!((border.1 - placed.rect.y).abs() < 0.1, "{power:?}");
        // Where the other comes in through the same border.
        let through = wake
            .windows(2)
            .find(|pair| pair[0].1.min(pair[1].1) < border.1 && pair[0].1.max(pair[1].1) > border.1)
            .map(|pair| pair[0].0)
            .expect("wake crosses the border");
        assert!((through - border.0).abs() >= 8.0, "{power:?} {wake:?}");
    }

    fn block_text(block: &Block) -> String {
        block
            .lines
            .iter()
            .flat_map(|line| &line.runs)
            .map(|run| run.text.as_str())
            .collect()
    }

    #[test]
    fn a_gradient_names_its_line() {
        let (_, lines) = crate::source::body("@startuml\nstate A #red/blue\n@enduml\n").unwrap();
        let diagram = parse(&lines).unwrap();
        let err = draw(&diagram).unwrap_err();
        assert_eq!(err.line, Some(1));
        assert!(err.reason.contains("gradient"), "{}", err.reason);
    }
}
