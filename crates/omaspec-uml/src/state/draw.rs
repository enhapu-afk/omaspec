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

#[cfg(test)]
mod bars;
mod nest;
mod open;
mod points;

pub use nest::label_by_hand;
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
/// Above a nested state's title and below it, down to the line under it.
/// The words stand in the middle of their band as the eye has it: a line
/// of text has more room above its letters than below.
const HEAD_TOP: f32 = 8.0;
const HEAD_FOOT: f32 = 11.0;
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
/// How far a composite state's fill steps toward the paper.
const FRAME_LIGHT: f32 = 0.7;
const HEAD_HALF: f32 = 4.0;

pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    draw_mapped(diagram).map(|(svg, _)| svg)
}

/// The SVG, and where its states and transitions are in it.
pub fn draw_mapped(diagram: &Diagram) -> Result<(String, Map), Declined> {
    let skin = Skin::from_params(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    canvas.sketch(skin.sketch);
    canvas.shadows(skin.shadow);
    if diagram.frame.parts {
        canvas.mark_parts();
    }
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
    // Points on their borders; the lines inside their states laid with
    // them there, then those across a border with them outside it.
    points::onto_borders(&diagram.root, &mut scene);
    let root = &diagram.root;
    link_crossings(root, &mut scene, &skin, family, &|path, c| {
        !points::across(root, path, c)
    })?;
    points::step_out(root, &mut scene);
    link_crossings(root, &mut scene, &skin, family, &|_, _| true)?;
    points::step_in(root, &mut scene);
    let arrows = arrows(&diagram.root);
    let misses = lay_by_hand(&arrows, &mut scene, &diagram.frame.by_hand);
    link_notes(&diagram.root, &mut scene, &skin, family)?;
    scene.offset(0.0, annotations.top());
    paint(&mut canvas, &skin, &diagram.root, &scene);
    annotations.draw(&mut canvas);
    let mut map = Map::default();
    chart(
        &diagram.root,
        &scene,
        None,
        (skin.round, skin.curve),
        &mut Vec::new(),
        &mut map,
    );
    map.wires = wires(
        &arrows,
        &scene,
        &diagram.frame.by_hand,
        skin.curve,
        skin.strict,
    );
    map.places = nest::places(&diagram.root, &scene, &diagram.frame.held);
    notes_inside(&scene, &mut map.inside);
    map.misses = misses;
    map.misses.extend(crate::hand::said(
        &diagram.frame.held,
        &diagram.frame.unplaced,
        &scene.all_unheld(),
    ));
    map.misses.sort();
    let map = canvas.place(map, MARGIN);
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
    path: &mut Vec<usize>,
    map: &mut Map,
) {
    for (index, (node, placed)) in container.nodes.iter().zip(&scene.nodes).enumerate() {
        path.push(index);
        chart_node(node, placed, parent, (round, curve), path, map);
        path.pop();
    }
    let arrows = container
        .edges
        .iter()
        .chain(container.crossings.iter().map(|c| &c.arrow));
    for (arrow, edge) in arrows.zip(&scene.edges) {
        if hidden(edge) {
            continue;
        }
        map.transitions.push(Route {
            line: arrow.line,
            points: drawn_route(&edge.points, curve),
        });
    }
}

/// Puts the state `node`, placed as `placed` at `path` in the scene's
/// tree, on `map`, and what it holds.
fn chart_node(
    node: &super::Node,
    placed: &Placed,
    parent: Option<&str>,
    (round, curve): (f32, f32),
    path: &mut Vec<usize>,
    map: &mut Map,
) {
    let shape = match node.kind {
        // Words beside the machine, no place of it.
        Kind::Note => return,
        Kind::State => Shape::State,
        Kind::Start => Shape::Start,
        Kind::End => Shape::End,
        Kind::Choice => Shape::Choice,
        Kind::Fork | Kind::Join => Shape::Bar,
        Kind::History | Kind::DeepHistory => Shape::History,
        Kind::Entry | Kind::Exit => Shape::Point,
        Kind::Region => {
            chart(
                &node.children,
                &placed.children,
                parent,
                (round, curve),
                path,
                map,
            );
            return;
        }
    };
    let rect = placed.rect;
    let radius = match shape {
        Shape::State => round.min(rect.w / 2.0).min(rect.h / 2.0),
        Shape::Start | Shape::End | Shape::History => rect.w.min(rect.h) / 2.0,
        Shape::Bar => 2.0,
        Shape::Choice => 0.0,
        Shape::Point => match node.point() {
            Some(super::Point::Circle) => rect.w.min(rect.h) / 2.0,
            _ => 0.0,
        },
    };
    map.states.push(Spot {
        name: node.key.clone(),
        shape,
        line: node.line,
        parent: parent.map(str::to_owned),
        rect,
        radius,
        part: crate::svg::box_part(&node.key, path),
    });
    chart(
        &node.children,
        &placed.children,
        Some(&node.key),
        (round, curve),
        path,
        map,
    );
}

/// One container after layout ([`graph::open::Scene`]); a transition keeps its
/// stroke.
type Scene = graph::open::Scene<Item, Stroke>;
type Placed = graph::open::Placed<Item, Stroke>;
type PlacedEdge = graph::open::Wire<Stroke>;
type PlacedNote = graph::open::Note;
type Measured = graph::open::Measured<Item, Stroke>;

/// What a state keeps for painting.
struct Item {
    /// Its key: what the sketch look seeds its strokes with.
    name: String,
    kind: Kind,
    fill: Paint,
    border: Stroke,
    /// Start, end and bars. A color of their own, else the state border,
    /// so a dark theme's accent stays visible.
    mark: Color,
    title: Block,
    description: Vec<Block>,
    /// The words in the title band of a nested state; 0 for a leaf.
    header_h: f32,
    /// Between the title band and what a nested state holds.
    gap: f32,
    composite: bool,
    /// Followed on a click on the state and its words.
    link: Option<crate::text::Link>,
    /// A state that receives a signal: a tab in its corner
    /// ([`super::Node::receives`]), its words below it.
    receives: bool,
    /// How an entry or an exit point is drawn.
    point: Option<super::Point>,
    /// How far a nested state's title stands right of its middle, off the
    /// lines from its entry points ([`points::onto_borders`]).
    title_shift: f32,
    /// An entry or an exit point of a state whose states run left to
    /// right: on its left or right border, not on the top or bottom one.
    sideways: bool,
    /// How far a point stands off its place while lines are laid to it
    /// from outside ([`points::step_out`]).
    aside: (f32, f32),
}

/// How large an entry or an exit point is, or a cell of an expansion's
/// row, as PlantUML draws them.
const POINT: f32 = 12.0;

/// How high and wide the tab of a state that receives a signal is, as
/// PlantUML draws it, and where its slant starts.
const TAB: (f32, f32, f32) = (12.0, 33.0, 26.0);

impl Item {
    /// How high the title band of a nested state is, from its top border
    /// to the line under its words.
    fn band(&self) -> f32 {
        band(self.header_h)
    }
}

/// [`Item::band`] for words `header_h` high.
fn band(header_h: f32) -> f32 {
    HEAD_TOP + header_h + HEAD_FOOT
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
    let (fill, border, mark) = match node.kind {
        Kind::Note => {
            let border = Stroke::new(skin.note.border, skin.note.thickness);
            (skin.note.fill, border, skin.note.border)
        }
        _ => paints(node, skin)?,
    };
    if node.composite {
        // A state that holds others is a frame around them: its fill
        // steps toward the paper, so the states inside stand out on it. A
        // color written on the state stays as written.
        let fill = if node.fill.is_none() && !node.children.nodes.is_empty() {
            fill.map(|color| color.mix(skin.background.unwrap_or(Color::WHITE), FRAME_LIGHT))
        } else {
            fill
        };
        let children = if open {
            Scene::empty()
        } else {
            layout_container(&node.children, every, family)?
        };
        let title = node.label.measure(title_font(skin, node)?, family)?;
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
            height: band(header_h) + gap + children.height + COMP_PAD,
            flat: flat(node.kind),
            children,
            item: Item {
                name: node.key.clone(),
                kind: node.kind,
                fill,
                border,
                mark,
                title,
                description,
                header_h,
                gap,
                composite: true,
                link: node.link.clone(),
                receives: false,
                point: None,
                title_shift: 0.0,
                sideways: false,
                aside: (0.0, 0.0),
            },
        });
    }

    let (width, height, title, description) = match node.kind {
        Kind::State => {
            let title = node.label.measure(title_font(skin, node)?, family)?;
            let description = text_blocks(&node.description, desc_font(skin), family)?;
            let width = (stack_width(&title, &description) + PAD_X * 2.0).max(MIN_W);
            let mut height = (stack_height(&title, &description) + PAD_Y * 2.0).max(MIN_H);
            // Its words keep below its tab, and the tab inside its box.
            let width = if node.receives() {
                height += TAB.0;
                width.max(TAB.1 + PAD_X)
            } else {
                width
            };
            (width, height, title, description)
        }
        Kind::Choice => {
            let title = node.label.measure(skin.state.font, family)?;
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
            // As long as what they split and join, once laid out
            // (`graph::span`).
            let (width, height) = match direction {
                Direction::Down => (BAR_LONG, BAR_SHORT),
                Direction::Right => (BAR_SHORT, BAR_LONG),
            };
            (width, height, Block::empty(), Vec::new())
        }
        Kind::Start => (START, START, Block::empty(), Vec::new()),
        Kind::End => (END, END, Block::empty(), Vec::new()),
        // Its shape alone; its name stands outside the state around it,
        // over or under it once it is on that state's border
        // ([`onto_borders`]).
        Kind::Entry | Kind::Exit => {
            let title = node.label.measure(skin.state.font, family)?;
            let (width, height) = match node.point() {
                Some(super::Point::Expansion) => (POINT * 4.0, POINT),
                _ => (POINT, POINT),
            };
            (width, height, title, Vec::new())
        }
        // Its words are its own line's, or its block's.
        Kind::Note => {
            let text = node.label.measure_written(skin.note.font, family)?;
            let (width, height) = note_size(&text);
            (width, height, text, Vec::new())
        }
        // A region is laid out by its state, never as a node of its own.
        Kind::Region => (0.0, 0.0, Block::empty(), Vec::new()),
    };
    Ok(Measured {
        width,
        height,
        flat: flat(node.kind),
        children: Scene::empty(),
        item: Item {
            name: node.key.clone(),
            kind: node.kind,
            fill,
            border,
            mark,
            title,
            description,
            header_h: 0.0,
            gap: 0.0,
            composite: false,
            link: node.link.clone(),
            receives: node.receives(),
            point: node.point(),
            title_shift: 0.0,
            sideways: direction == Direction::Right,
            aside: (0.0, 0.0),
        },
    })
}

/// Whether lines may meet a box of `kind` anywhere along a side.
fn flat(kind: Kind) -> bool {
    matches!(kind, Kind::State | Kind::Fork | Kind::Join | Kind::Region)
}

pub(super) fn paints(node: &Node, skin: &Skin) -> Result<(Paint, Stroke, Color), Declined> {
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
    // A state's border is never hidden; `Hidden` is a transition's.
    border.dash = match node.border_dash {
        Dash::Solid | Dash::Hidden => None,
        Dash::Dashed => Some((7.0, 5.0)),
        Dash::Dotted => Some((1.5, 3.0)),
    };
    let mark = own.unwrap_or(border_color);
    Ok((fill, border, mark))
}

/// A transition's words, and the room the layout keeps for them: a note
/// on it (`note on link`) stands under them ([`link_note`]), so the room
/// grows by the note.
pub(super) fn edge_label(
    edge: &Transition,
    skin: &Skin,
    family: Family,
) -> Result<Block, Declined> {
    // PlantUML draws no words on a hidden arrow.
    if edge.dash == Dash::Hidden {
        return Ok(Block::empty());
    }
    let mut label = match &edge.label {
        Some(text) => text.measure(skin.arrow_font, family)?,
        None => Block::empty(),
    };
    if let Some(note) = &edge.note {
        let (w, h) = note_size(&note.measure(skin.note.font, family)?);
        let gap = if label.is_empty() { 0.0 } else { LINK_NOTE_GAP };
        label.width = label.width.max(w);
        label.height += gap + h;
    }
    Ok(label)
}

/// Between a transition's words and the note on it.
const LINK_NOTE_GAP: f32 = 4.0;

/// The notes on the transitions of `container`, laid out as `scene`, and
/// of every one inside, once each line and the room for its words lie
/// where they end (after lines by hand): each note at the foot of that
/// room ([`edge_label`]), its own words tied to their line. The words then
/// take their own size again, so they are tied, drawn and kept apart as
/// they are; both stand on the side of the room toward the line.
fn link_notes(
    container: &Container,
    scene: &mut Scene,
    skin: &Skin,
    family: Family,
) -> Result<(), Declined> {
    let arrows = container
        .edges
        .iter()
        .chain(container.crossings.iter().map(|crossing| &crossing.arrow));
    for (slot, arrow) in arrows.enumerate() {
        let (Some(note), Some(edge)) = (&arrow.note, scene.edges.get_mut(slot)) else {
            continue;
        };
        if edge.points.len() < 2 {
            continue;
        }
        let text = note.measure_written(skin.note.font, family)?;
        let (w, h) = note_size(&text);
        let words = match &arrow.label {
            Some(label) => label.measure(skin.arrow_font, family)?,
            None => Block::empty(),
        };
        let (at, room) = (edge.label_at, (edge.label.width, edge.label.height));
        let toward = line_after(&edge.points, at, room);
        let x = |width: f32| if toward { at.0 + room.0 - width } else { at.0 };
        edge.label_at = (x(words.width), at.1);
        edge.label = words;
        scene.notes.push(PlacedNote {
            x: x(w),
            y: at.1 + room.1 - h,
            w,
            h,
            target: arrow.from,
            side: Side::Right,
            nth: 0,
            text,
            tether: None,
            on: Some(slot),
            line: note.line,
        });
    }
    for (node, placed) in container.nodes.iter().zip(scene.nodes.iter_mut()) {
        link_notes(&node.children, &mut placed.children, skin, family)?;
    }
    Ok(())
}

/// Whether the line `points` runs past the right half of the room at `at`,
/// `room` large, at its middle's height: the room lies left of it.
fn line_after(points: &[(f32, f32)], at: (f32, f32), room: (f32, f32)) -> bool {
    let y = at.1 + room.1 / 2.0;
    points
        .windows(2)
        .find_map(|run| {
            let ((x0, y0), (x1, y1)) = (run[0], run[1]);
            let (low, high) = (y0.min(y1), y0.max(y1));
            (low <= y && y <= high && high > low).then(|| x0 + (x1 - x0) * (y - y0) / (y1 - y0))
        })
        .is_some_and(|x| x >= at.0 + room.0 / 2.0)
}

pub(super) fn arrow_stroke(skin: &Skin, edge: &Transition) -> Result<Stroke, Declined> {
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
    let width = match edge.weight {
        Some(weight) => skin.arrow.width * weight,
        None if edge.bold => skin.arrow.width * 2.0,
        None => skin.arrow.width,
    };
    // A hidden one has no width: what paints a line, a head or a map
    // entry passes over it ([`hidden`]).
    let width = if edge.dash == Dash::Hidden {
        0.0
    } else {
        width
    };
    let mut stroke = Stroke::new(color, width);
    stroke.dash = match edge.dash {
        Dash::Dashed => Some((7.0, 7.0)),
        Dash::Dotted => Some((1.0, 3.0)),
        Dash::Solid | Dash::Hidden => skin.arrow.dash,
    };
    Ok(stroke)
}

/// Whether a placed transition is a hidden one (`-[hidden]->`).
fn hidden(edge: &PlacedEdge) -> bool {
    edge.look.width <= 0.0
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
        line: usize,
    }
    let mut sized = Vec::with_capacity(container.notes.len());
    for note in &container.notes {
        if note.target >= rects.len() {
            return Err(Declined::at(note.line, "a note needs a state"));
        }
        let text = note.text.measure_written(skin.note.font, family)?;
        let (w, h) = note_size(&text);
        sized.push(Draft {
            target: note.target,
            side: note.side,
            w,
            h,
            text,
            line: note.line,
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
        for (nth, index) in indices.into_iter().enumerate() {
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
                target,
                side,
                nth,
                text: std::mem::replace(&mut note.text, Block::empty()),
                tether: None,
                on: None,
                line: note.line,
            });
            let last = placed.len() - 1;
            // An edge label can sit in the same gap. Step the note off it,
            // onto free paper rather than onto another state.
            clear_note(&mut placed[last], side, edges, rects);
            // Lines into the state on that side would run under it.
            let (before, note) = placed.split_at_mut(last);
            clear_lines(&mut note[0], side, rect, (edges, rects, before));
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
            if !has_room(edge) {
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

/// Moves a note that lies on a line, a label or a state along its side
/// and further out, to where it covers the fewest, and the nearest such
/// place; off its state, a dashed line ties it to it. A note under a state
/// lay on every line that came in from below.
fn clear_lines(
    note: &mut PlacedNote,
    side: Side,
    state: Rect,
    (edges, rects, notes): (&[PlacedEdge], &[Rect], &[PlacedNote]),
) {
    let (w, h) = (note.w, note.h);
    let covers = |x: f32, y: f32| -> f32 {
        let area = (x, y, w, h);
        let crossed: usize = edges
            .iter()
            .map(|edge| {
                edge.points
                    .windows(2)
                    .filter(|pair| segment_meets(pair[0], pair[1], area))
                    .count()
            })
            .sum();
        let labels = edges
            .iter()
            .filter(|edge| has_room(edge) && boxes_overlap(area, label_box(edge)))
            .count()
            + notes
                .iter()
                .filter(|other| boxes_overlap(area, (other.x, other.y, other.w, other.h)))
                .count();
        let states: f32 = rects
            .iter()
            .map(|rect| overlap_area(area, (rect.x, rect.y, rect.w, rect.h)))
            .sum();
        (crossed + labels) as f32 * 10_000.0 + states
    };
    let (x0, y0) = (note.x, note.y);
    let mut best = (covers(x0, y0), 0.0, x0, y0);
    if best.0 <= 0.5 {
        return;
    }
    const STEP: f32 = 12.0;
    for level in 0..4 {
        let out = level as f32 * (h.min(w) / 2.0 + NOTE_GAP);
        for k in -40..=40 {
            let along = k as f32 * STEP;
            let (x, y) = match side {
                Side::Bottom => (x0 + along, y0 + out),
                Side::Top => (x0 + along, y0 - out),
                Side::Right => (x0 + out, y0 + along),
                Side::Left => (x0 - out, y0 + along),
            };
            let cost = covers(x, y);
            let far = along.abs() + out;
            if cost < best.0 - 0.5 || ((cost - best.0).abs() <= 0.5 && far < best.1) {
                best = (cost, far, x, y);
            }
        }
    }
    (note.x, note.y) = (best.2, best.3);
    let box_of = (note.x, note.y, w, h);
    if gap(box_of, (state.x, state.y, state.w, state.h)) > NOTE_GAP + 1.0 {
        note.tether = Some((
            nearest(box_of, center(state)),
            nearest(
                (state.x, state.y, state.w, state.h),
                (note.x + w / 2.0, note.y + h / 2.0),
            ),
        ));
    }
}

fn center(rect: Rect) -> (f32, f32) {
    (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
}

/// The point of the box `(x, y, w, h)` nearest `to`.
fn nearest((x, y, w, h): (f32, f32, f32, f32), to: (f32, f32)) -> (f32, f32) {
    (to.0.clamp(x, x + w), to.1.clamp(y, y + h))
}

/// How far apart two boxes stand; 0 where they meet.
fn gap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let dx = (b.0 - (a.0 + a.2)).max(a.0 - (b.0 + b.2)).max(0.0);
    let dy = (b.1 - (a.1 + a.3)).max(a.1 - (b.1 + b.3)).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// Whether the segment from `a` to `b` runs through the box.
fn segment_meets(a: (f32, f32), b: (f32, f32), (x, y, w, h): (f32, f32, f32, f32)) -> bool {
    // Clipped to the box's slabs, one axis after the other.
    let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    for (p, q) in [
        (-dx, a.0 - x),
        (dx, x + w - a.0),
        (-dy, a.1 - y),
        (dy, y + h - a.1),
    ] {
        if p.abs() < 1e-6 {
            if q < 0.0 {
                return false;
            }
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            t0 = t0.max(r);
        } else {
            t1 = t1.min(r);
        }
        if t0 > t1 {
            return false;
        }
    }
    true
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

/// Whether the layout keeps room for `edge`'s words: they are there, or a
/// note on it is ([`edge_label`]).
pub(super) fn has_room(edge: &PlacedEdge) -> bool {
    edge.label.width > 0.0 && edge.label.height > 0.0
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

/// The font of a state's name, in the color of its own text where it has
/// one ([`name_color`]); its description keeps the skin's.
fn title_font(skin: &Skin, node: &Node) -> Result<Font, Declined> {
    let mut font = name_font(skin);
    if let Some(color) = name_color(node)? {
        font.color = color;
    }
    Ok(font)
}

/// The color a state's own text gives its name (`#…;text:red`). PlantUML
/// gives it to a state without a body only, not to the title of one with
/// states inside.
pub(super) fn name_color(node: &Node) -> Result<Option<Color>, Declined> {
    match &node.text_color {
        Some(value) if !node.composite => {
            color::parse(value).map_err(|err| Declined::at(node.line, err))
        }
        _ => Ok(None),
    }
}

fn desc_font(skin: &Skin) -> Font {
    Font {
        size: (skin.state.font.size - 2.0).max(4.0),
        ..skin.state.font
    }
}

fn text_blocks(items: &[super::Text], font: Font, family: Family) -> Result<Vec<Block>, Declined> {
    items
        .iter()
        .map(|text| text.measure(font, family))
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

fn paint(canvas: &mut Canvas, skin: &Skin, root: &Container, scene: &Scene) {
    paint_shapes(canvas, skin, scene, &mut Vec::new());
    paint_edges(canvas, scene, skin.curve);
    paint_notes(canvas, skin, scene, &mut Vec::new());
    paint_labels(canvas, Some(root), scene, &mut Vec::new());
    paint_heads(canvas, skin, scene);
}

/// The part of each of `scene`'s notes, at `path` in the tree: a part of
/// its own by its state's ([`crate::svg::note_part`]).
fn note_parts(scene: &Scene, path: &[usize]) -> Vec<String> {
    scene.note_parts(path, |node, at| crate::svg::box_part(&node.item.name, at))
}

/// Each state's shape, in its part; `path` is where `scene` is in the
/// tree.
fn paint_shapes(canvas: &mut Canvas, skin: &Skin, scene: &Scene, path: &mut Vec<usize>) {
    for (index, node) in scene.nodes.iter().enumerate() {
        path.push(index);
        canvas.part(&crate::svg::box_part(&node.item.name, path), |canvas| {
            canvas.named(&node.item.name, |canvas| {
                paint_shape(canvas, skin, node);
                if node.item.composite {
                    paint_divider(canvas, node);
                    for (a, b) in separators(node) {
                        let mut stroke =
                            Stroke::new(node.item.border.color, node.item.border.width.max(0.5));
                        stroke.dash = Some((5.0, 4.0));
                        canvas.line(Layer::Frames, a, b, stroke);
                    }
                }
            });
        });
        if node.item.composite {
            paint_shapes(canvas, skin, &node.children, path);
        }
        path.pop();
    }
}

fn paint_shape(canvas: &mut Canvas, skin: &Skin, node: &Placed) {
    canvas.linked(node.item.link.as_ref(), &[Layer::Frames], |canvas| {
        paint_outline(canvas, skin, node);
    });
}

fn paint_outline(canvas: &mut Canvas, skin: &Skin, node: &Placed) {
    let rect = node.rect;
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    match node.item.kind {
        Kind::State => {
            let radius = skin.round.min(rect.w / 2.0).min(rect.h / 2.0);
            canvas.rect(
                Layer::Frames,
                (rect.x, rect.y, rect.w, rect.h),
                radius,
                node.item.fill,
                Some(node.item.border),
            );
            if node.item.receives {
                // The tab: down from the top border, a slant, and across to
                // the left border.
                let (high, wide, slant) = TAB;
                let (x, y) = (rect.x, rect.y);
                let tab = format!(
                    "M{},{} L{},{} L{},{} L{},{}",
                    n(x + wide),
                    n(y),
                    n(x + wide),
                    n(y + high - (wide - slant)),
                    n(x + slant),
                    n(y + high),
                    n(x),
                    n(y + high)
                );
                let bounds = (x, y, x + wide, y + high);
                canvas.path(Layer::Frames, &tab, bounds, None, Some(node.item.border));
            }
        }
        Kind::Choice => {
            let points = [
                (cx, rect.y),
                (rect.x + rect.w, cy),
                (cx, rect.y + rect.h),
                (rect.x, cy),
            ];
            canvas.polygon(
                Layer::Frames,
                &points,
                node.item.fill,
                Some(node.item.border),
            );
        }
        Kind::Fork | Kind::Join => {
            canvas.card(
                Layer::Frames,
                (rect.x, rect.y, rect.w, rect.h),
                2.0,
                node.item.mark,
            );
        }
        Kind::History | Kind::DeepHistory => {
            canvas.ellipse(
                Layer::Frames,
                (cx, cy),
                (rect.w / 2.0, rect.h / 2.0),
                node.item.fill,
                Some(node.item.border),
            );
        }
        Kind::Start => {
            let stroke = Stroke::new(node.item.mark, 1.0);
            canvas.ellipse(
                Layer::Frames,
                (cx, cy),
                (rect.w / 2.0, rect.h / 2.0),
                Some(node.item.mark),
                Some(stroke),
            );
        }
        Kind::End => {
            let stroke = Stroke::new(node.item.mark, 1.5);
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
                Some(node.item.mark),
                Some(Stroke::new(node.item.mark, 1.0)),
            );
        }
        Kind::Note => {
            let rect = (rect.x, rect.y, rect.w, rect.h);
            note_shape(canvas, skin, Layer::Frames, rect);
        }
        Kind::Entry | Kind::Exit => paint_point(canvas, node),
        Kind::Region => {}
    }
}

/// Where the name of an entry or an exit point stands: outside the state on
/// whose border it is, left of it, over an entry and under an exit, so the
/// lines that meet it from outside keep off it (lines between boxes keep
/// off it too, [`super::nest`]).
fn point_name(node: &Placed) -> Rect {
    let (gap, title) = (2.0, &node.item.title);
    // Where it ends, though it stands off its place for now.
    let (dx, dy) = node.item.aside;
    let (x, y) = (node.rect.x - dx, node.rect.y - dy);
    let middle = y + node.rect.h / 2.0;
    let entry = node.item.kind == Kind::Entry;
    let y = if entry {
        middle - gap - title.height
    } else {
        middle + gap
    };
    // On a right border the name of an exit stands right of it, outside.
    let x = match node.item.sideways && !entry {
        true => x + node.rect.w + gap,
        false => x - gap - title.width,
    };
    Rect {
        x,
        y,
        w: title.width,
        h: title.height,
    }
}

/// An entry or an exit point, as PlantUML draws it: a circle, a cross in
/// an exit's; a pin's square; an expansion's row of cells.
fn paint_point(canvas: &mut Canvas, node: &Placed) {
    let rect = node.rect;
    let stroke = Stroke::new(node.item.border.color, 1.5);
    let (fill, layer) = (node.item.fill, Layer::Frames);
    match node.item.point {
        Some(super::Point::Circle) | None => {
            let (cx, cy, r) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0, rect.w / 2.0);
            canvas.ellipse(layer, (cx, cy), (r, r), fill, Some(stroke));
            if node.item.kind == Kind::Exit {
                let d = r * std::f32::consts::FRAC_1_SQRT_2;
                canvas.line(layer, (cx - d, cy - d), (cx + d, cy + d), stroke);
                canvas.line(layer, (cx - d, cy + d), (cx + d, cy - d), stroke);
            }
        }
        Some(point) => {
            canvas.rect(
                layer,
                (rect.x, rect.y, rect.w, rect.h),
                0.0,
                fill,
                Some(stroke),
            );
            if point == super::Point::Expansion {
                for cell in 1..4 {
                    let x = rect.x + rect.w * cell as f32 / 4.0;
                    canvas.line(layer, (x, rect.y), (x, rect.y + rect.h), stroke);
                }
            }
        }
    }
}

fn paint_divider(canvas: &mut Canvas, node: &Placed) {
    if node.item.header_h <= 0.0 {
        return;
    }
    let y = node.rect.y + node.item.band();
    canvas.line(
        Layer::Frames,
        (node.rect.x, y),
        (node.rect.x + node.rect.w, y),
        Stroke::new(node.item.border.color, node.item.border.width.max(0.5)),
    );
}

fn paint_edges(canvas: &mut Canvas, scene: &Scene, curve: f32) {
    for edge in &scene.edges {
        if edge.points.len() < 2 {
            continue;
        }
        if hidden(edge) {
            continue;
        }
        canvas.curve(Layer::Lines, &shortened(&edge.points), curve, edge.look);
    }
    for node in &scene.nodes {
        paint_edges(canvas, &node.children, curve);
    }
}

/// Each note's part with the part of the state it lies inside, a
/// region's state for a region, into `out`.
fn notes_inside(scene: &Scene, out: &mut Vec<(String, String)>) {
    let part = |node: &Placed, at: &[usize]| {
        (node.item.kind != Kind::Region).then(|| crate::svg::box_part(&node.item.name, at))
    };
    scene.notes_inside(&mut Vec::new(), None, &part, out);
}

/// Each note, in a part of its own; `path` is where `scene` is in the
/// tree.
fn paint_notes(canvas: &mut Canvas, skin: &Skin, scene: &Scene, path: &mut Vec<usize>) {
    for (note, part) in scene.notes.iter().zip(note_parts(scene, path)) {
        canvas.part(&part, |canvas| note_box(canvas, skin, note));
    }
    for (index, node) in scene.nodes.iter().enumerate() {
        path.push(index);
        paint_notes(canvas, skin, &node.children, path);
        path.pop();
    }
}

fn note_box(canvas: &mut Canvas, skin: &Skin, note: &PlacedNote) {
    if let Some((from, to)) = note.tether {
        let line = Stroke::new(skin.note.border, skin.note.thickness).dashed(4.0, 3.0);
        canvas.line(Layer::Front, from, to, line);
    }
    note_shape(canvas, skin, Layer::Front, (note.x, note.y, note.w, note.h));
}

/// A note's sheet with its corner folded, in `layer`.
fn note_shape(canvas: &mut Canvas, skin: &Skin, layer: Layer, (x, y, w, h): (f32, f32, f32, f32)) {
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
    canvas.path(layer, &body, (x, y, x + w, y + h), skin.note.fill, stroke);
    let corner = format!(
        "M{},{} L{},{} L{},{}",
        n(x + w - fold),
        n(y),
        n(x + w - fold),
        n(y + fold),
        n(x + w),
        n(y + fold)
    );
    canvas.path(layer, &corner, (x, y, x + w, y + h), skin.note.fill, stroke);
}

/// Each state's words in its part, each line's and each note's in a part
/// of their own; `path` is where `scene` is in the tree. `container` is
/// what `scene` lays out, its nodes in their order, and its edges, then
/// its crossings ([`chart`]): the lines of the source its words come from.
///
/// What one line alone writes is tied to it ([`Canvas::tie`]): a
/// transition's label, a line of a state's description, a note. A state's
/// name, which many lines mention, is not. Words whose line is not known
/// are drawn all the same, untied.
fn paint_labels(
    canvas: &mut Canvas,
    container: Option<&Container>,
    scene: &Scene,
    path: &mut Vec<usize>,
) {
    for (index, node) in scene.nodes.iter().enumerate() {
        path.push(index);
        let written = container.and_then(|container| container.nodes.get(index));
        canvas.part(&crate::svg::box_part(&node.item.name, path), |canvas| {
            canvas.linked(node.item.link.as_ref(), &[Layer::Front], |canvas| {
                paint_words(canvas, node, written);
            });
        });
        paint_labels(
            canvas,
            written.map(|written| &written.children),
            &node.children,
            path,
        );
        path.pop();
    }
    let lines: Vec<usize> = container.map_or_else(Vec::new, |container| {
        container
            .edges
            .iter()
            .chain(container.crossings.iter().map(|c| &c.arrow))
            .map(|arrow| arrow.line)
            .collect()
    });
    for (slot, edge) in scene.edges.iter().enumerate() {
        if hidden(edge) {
            continue;
        }
        canvas.part(&crate::svg::words_part(path, slot), |canvas| {
            let before = canvas.tie(lines.get(slot).copied());
            canvas.text(Layer::Front, &edge.label, edge.label_at, Align::Left);
            canvas.tie(before);
        });
    }
    for (note, part) in scene.notes.iter().zip(note_parts(scene, path)) {
        canvas.part(&part, |canvas| {
            canvas.text(
                Layer::Front,
                &note.text,
                (note.x + NOTE_PAD, note.y + NOTE_PAD),
                Align::Left,
            );
        });
    }
}

/// A node's own words: a state's title and description, a choice's label.
/// `written` is the state as read, whose description lines are each one
/// line's.
fn paint_words(canvas: &mut Canvas, node: &Placed, written: Option<&super::Node>) {
    let lines: Vec<Option<usize>> = written.map_or_else(Vec::new, |written| {
        written
            .description
            .iter()
            .map(|text| Some(text.line))
            .collect()
    });
    match node.item.kind {
        Kind::State if node.item.composite => {
            paint_stack(
                canvas,
                (&node.item.title, &node.item.description, &lines),
                node.rect.x + node.rect.w / 2.0 + node.item.title_shift,
                node.rect.y + HEAD_TOP,
            );
        }
        Kind::State => {
            let height = stack_height(&node.item.title, &node.item.description);
            // Below the tab of a state that receives a signal.
            let tab = if node.item.receives { TAB.0 } else { 0.0 };
            paint_stack(
                canvas,
                (&node.item.title, &node.item.description, &lines),
                node.rect.x + node.rect.w / 2.0,
                node.rect.y + tab + (node.rect.h - tab - height) / 2.0,
            );
        }
        Kind::Choice | Kind::History | Kind::DeepHistory => {
            let y = node.rect.y + (node.rect.h - node.item.title.height) / 2.0;
            canvas.text(
                Layer::Front,
                &node.item.title,
                (node.rect.x + node.rect.w / 2.0, y),
                Align::Center,
            );
        }
        Kind::Note => canvas.text(
            Layer::Front,
            &node.item.title,
            (node.rect.x + NOTE_PAD, node.rect.y + NOTE_PAD),
            Align::Left,
        ),
        Kind::Entry | Kind::Exit => {
            let at = point_name(node);
            canvas.text(Layer::Front, &node.item.title, (at.x, at.y), Align::Left);
        }
        Kind::Fork | Kind::Join | Kind::Start | Kind::End | Kind::Region => {}
    }
}

/// A state's title, and under it its description, each block tied to
/// the line of `lines` that writes it.
fn paint_stack(
    canvas: &mut Canvas,
    (title, description, lines): (&Block, &[Block], &[Option<usize>]),
    x: f32,
    mut y: f32,
) {
    if !title.is_empty() {
        canvas.text(Layer::Front, title, (x, y), Align::Center);
        y += title.height;
    }
    for (index, block) in description.iter().enumerate() {
        let before = canvas.tie(lines.get(index).copied().flatten());
        canvas.text(Layer::Front, block, (x, y), Align::Center);
        canvas.tie(before);
        y += block.height;
    }
}

fn paint_heads(canvas: &mut Canvas, skin: &Skin, scene: &Scene) {
    for edge in scene.edges.iter().filter(|edge| !hidden(edge)) {
        arrow_head(canvas, skin, &edge.points, edge.look);
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

pub(super) fn arrow_head(canvas: &mut Canvas, skin: &Skin, points: &[(f32, f32)], stroke: Stroke) {
    let Some(head) = head_shape(points, skin.strict) else {
        return;
    };
    let solid = Stroke {
        dash: None,
        width: stroke.width.min(1.5),
        ..stroke
    };
    canvas.polygon(Layer::Lines, &head, Some(stroke.color), Some(solid));
}

/// The arrowhead at the end of `points`: a filled triangle, with a notch
/// in its base unless `strict`.
fn head_shape(points: &[(f32, f32)], strict: bool) -> Option<Vec<(f32, f32)>> {
    if points.len() < 2 {
        return None;
    }
    let tip = points[points.len() - 1];
    let from = points[points.len() - 2];
    let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (vx * vx + vy * vy).sqrt();
    if len < 0.01 {
        return None;
    }
    let (ux, uy) = (vx / len, vy / len);
    let back = (tip.0 - ux * HEAD_LENGTH, tip.1 - uy * HEAD_LENGTH);
    let side = (-uy * HEAD_HALF, ux * HEAD_HALF);
    let mut head = vec![
        (back.0 + side.0, back.1 + side.1),
        tip,
        (back.0 - side.0, back.1 - side.1),
    ];
    if !strict {
        head.push((tip.0 - ux * 6.0, tip.1 - uy * 6.0));
    }
    Some(head)
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

    /// Lines side by side between two states with a label between them:
    /// each runs straight, the box they meet is wide enough for that, and
    /// the label keeps off the line that is not its own. `start` read on
    /// into `cup full` before, or its neighbor bent around it just before
    /// its arrowhead.
    #[test]
    fn a_label_between_two_lines_keeps_off_the_other_one() {
        let (_, scene) = laid(
            "@startuml\n[*] --> Ready\nReady --> Brewing : start\nBrewing --> Ready : cup full\nBrewing --> Ready : cancel\n@enduml\n",
        );
        let labelled: Vec<&PlacedEdge> = scene
            .edges
            .iter()
            .filter(|edge| !edge.label.is_empty())
            .collect();
        assert_eq!(labelled.len(), 3);
        for edge in &labelled {
            let word = block_text(&edge.label);
            let x = edge.points[0].0;
            assert!(
                edge.points.iter().all(|point| (point.0 - x).abs() < 0.1),
                "{word} bends: {:?}",
                edge.points
            );
            let (left, right) = (edge.label_at.0, edge.label_at.0 + edge.label.width);
            for other in labelled
                .iter()
                .filter(|other| !std::ptr::eq(**other, *edge))
            {
                let line = other.points[0].0;
                assert!(
                    line < left - 6.0 || line > right + 6.0,
                    "{word} at {left}..{right} lies on the line at {line}"
                );
            }
        }
    }

    /// Each note is drawn in a part of its own, by its state's, inside a
    /// state or at the top: a fold glides it from where it was to where
    /// it is, rather than inside the picture of the state around it, where
    /// its old place and its new one showed both at once (REQ-063).
    #[test]
    fn a_hidden_arrow_is_nowhere_but_in_the_layout() {
        let source = "@startuml\nstate Setup {\n  [*] --> Hardware\n  Hardware -[hidden]-> Software : quiet\n}\n[*] --> Setup\nSetup -[hidden,dashed]-> Running : secret\n@enduml\n";
        let (svg, map) = crate::render_mapped(source, None).unwrap();
        // Neither its words, nor a line the pane glides, lights or shapes,
        // nor a step of a play.
        assert!(!svg.contains("secret") && !svg.contains("quiet"), "{svg}");
        let lines = |of: Vec<usize>| {
            let mut of = of;
            of.sort();
            of
        };
        assert_eq!(
            lines(map.wires.iter().filter_map(|w| w.line).collect()),
            [2, 5]
        );
        assert_eq!(
            lines(map.transitions.iter().map(|t| t.line).collect()),
            [2, 5]
        );
        // Folded, the box counts no hidden arrow among its transitions.
        let Ok(crate::Diagram::State(mut d)) = crate::parse(source) else {
            panic!("a state diagram");
        };
        d.fold(&["Setup".to_owned()]);
        let said = &d
            .root
            .nodes
            .iter()
            .find(|n| n.key == "Setup")
            .unwrap()
            .description;
        assert!(
            said.iter().any(|t| t.value == "2 states · 1 transition"),
            "{said:?}"
        );
    }

    /// A note on a transition stands under its words, in a part of its
    /// own named after them, its words tied to their line; one on a
    /// transition into a state's inside as well.
    #[test]
    fn a_note_on_link_stands_under_the_words_of_its_transition() {
        let source = "@startuml\n[*] --> Idle\nIdle --> Busy : start\nnote on link : counts\nstate Busy {\n  [*] --> Working\n}\nWorking --> Idle : fail\nnote on link : a crossing\n' parts (Omaspec)\n@enduml\n";
        let (svg, map) = crate::render_mapped(source, None).unwrap();
        // Each in a part of its own, by its transition's words: the second
        // line of the diagram, and its first crossing after its two edges.
        let groups = crate::part_groups(&svg);
        for (name, words) in [
            ("note:words::1#on0", "counts"),
            ("note:words::2#on0", "a crossing"),
        ] {
            let group: String = groups
                .iter()
                .filter(|(n, _)| n == name)
                .map(|(_, group)| *group)
                .collect();
            assert!(
                group.contains("<path") && group.contains(words),
                "{name}: {svg}"
            );
        }
        // Its words lead to its line, not to the transition's.
        let tied = |line: usize| map.tied.iter().filter(|t| t.line == line).count();
        assert!(tied(3) >= 1 && tied(8) >= 1, "{:?}", map.tied);
        // Under its transition's words, which keep their own size: their
        // tie and their place on the map hold no note, so a click on the
        // note leads to the note.
        let label = map.wires.iter().find(|w| w.line == Some(2)).unwrap().labels[0];
        let words = map.tied.iter().find(|t| t.line == 2).unwrap().rect;
        let note = map.tied.iter().find(|t| t.line == 3).unwrap().rect;
        assert!(note.y >= words.y + words.h, "{note:?} under {words:?}");
        assert!(label.h < note.y - label.y, "{label:?} holds {note:?}");
    }

    /// `text:` colors the name of a state without a body, not the title of
    /// one with states inside, as PlantUML draws them.
    #[test]
    fn its_own_text_colors_a_leaf_s_name() {
        let source = "@startuml\nstate Comp #pink;text:blue {\n  state Leaf #pink;text:red : said\n}\n@enduml\n";
        let (svg, _) = crate::render_mapped(source, None).unwrap();
        let fill_of = |word: &str| {
            let at = svg.find(&format!(">{word}</tspan>")).unwrap();
            let start = svg[..at].rfind("<tspan").unwrap();
            svg[start..at].split("fill=\"").nth(1).unwrap()[..7].to_owned()
        };
        assert_eq!(fill_of("Leaf"), "#FF0000");
        assert_eq!(fill_of("said"), "#000000");
        assert_eq!(fill_of("Comp"), "#000000");
    }

    /// An entry point stands on the top border of its state and an exit
    /// point on the bottom one, each a place a play finds, and the lines
    /// from outside reach them there.
    #[test]
    fn points_stand_on_the_border_of_their_state() {
        let source = "@startuml\n[*] --> in1\nstate Somp {\n  state in1 <<entryPoint>>\n  state out1 <<exitPoint>>\n  state ex <<expansionInput>>\n  in1 --> Work\n  ex --> Work\n  Work --> out1\n}\nout1 --> Done\nFoo --> ex\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let spot = |name: &str| map.states.iter().find(|s| s.name == name).unwrap();
        let somp = spot("Somp").rect;
        for (name, y) in [("in1", somp.y), ("ex", somp.y), ("out1", somp.y + somp.h)] {
            let point = spot(name);
            assert_eq!(point.shape, Shape::Point, "{name}");
            assert!(
                (point.rect.y + point.rect.h / 2.0 - y).abs() < 0.5,
                "{name}: {:?}",
                point.rect
            );
        }
        assert!((spot("ex").rect.w - 48.0).abs() < 0.5);
        // The lines from outside end on them.
        let wire = |line: usize| map.wires.iter().find(|w| w.line == Some(line)).unwrap();
        let ends_at = |line: usize, name: &str| {
            let r = spot(name).rect;
            let (x, y) = *wire(line).points.last().unwrap();
            x >= r.x - 1.0 && x <= r.x + r.w + 1.0 && (y - r.y).abs() < 6.0
        };
        assert!(
            ends_at(1, "in1") && ends_at(11, "ex"),
            "{:?}",
            wire(11).points
        );
        let (x, y) = wire(10).points[0];
        let out = spot("out1").rect;
        assert!(
            y >= out.y + out.h / 2.0 && x > out.x && x < out.x + out.w,
            "{:?}",
            wire(10).points
        );
    }

    /// Where the states run left to right, an entry point stands on the
    /// left border and an exit point on the right one, as PlantUML puts
    /// them; a note beside a point keeps its room, tied to it, and a loop
    /// on a point moves with it as a whole.
    #[test]
    fn points_follow_the_flow_and_take_their_notes_and_loops_along() {
        let source = "@startuml\nleft to right direction\n[*] --> Idle\nstate Busy {\n  state side <<entryPoint>>\n  state done <<exitPoint>>\n  [*] --> Usual\n  side --> Special\n  Usual --> Special\n  Special --> done : finish\n}\nIdle --> side : urgent\nIdle --> Busy : normal\ndone --> Idle\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let spot = |name: &str| map.states.iter().find(|s| s.name == name).unwrap().rect;
        let busy = spot("Busy");
        let middle = |r: crate::graph::Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
        assert!(
            (middle(spot("side")).0 - busy.x).abs() < 0.5,
            "{:?}",
            spot("side")
        );
        assert!((middle(spot("done")).0 - (busy.x + busy.w)).abs() < 0.5);
        let noted = "@startuml\n[*] --> Idle\nstate Busy {\n  state side <<entryPoint>>\n  note right of side : the urgent way\n  [*] --> Usual\n  side --> Special\n  side --> side : again\n}\nIdle --> side : urgent\n@enduml\n";
        let (root, mut scene) = laid(noted);
        points::onto_borders(&root, &mut scene);
        let inside = &scene.nodes[2].children;
        assert!(inside.notes[0].tether.is_some(), "{:?}", inside.notes[0]);
        // The loop's ends and its middle all moved up to the border.
        let point = inside.nodes[0].rect;
        let looped = &inside.edges[2].points;
        let top = point.y - point.h - 1.0;
        assert!(looped.iter().all(|p| p.1 < point.y + 60.0), "{looped:?}");
        assert!(looped.iter().any(|p| p.1 < top + 30.0), "{looped:?}");
    }

    /// A hidden arrow turned across the flow onto a state with states inside
    /// ranks its states, as a hold beside such a state cannot be kept.
    #[test]
    fn a_hidden_arrow_onto_a_state_with_states_ranks_them() {
        let source = "@startuml\n[*] --> A\nA -right[hidden]-> B\nstate B {\n  [*] --> X\n}\nA --> C\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let spot = |name: &str| map.states.iter().find(|s| s.name == name).unwrap().rect;
        assert!(
            spot("B").y > spot("A").y + spot("A").h,
            "{:?} {:?}",
            spot("A"),
            spot("B")
        );
    }

    /// A state that receives a signal has a tab in its top left corner,
    /// and its name below it.
    #[test]
    fn a_receiving_state_has_a_tab_and_its_name_below_it() {
        let source = "@startuml\nstate R <<sdlreceive>>\nstate P\n@enduml\n";
        let (svg, _) = crate::render_mapped(source, None).unwrap();
        let tabs = svg.matches(" L").count();
        let plain = crate::render_mapped("@startuml\nstate R\nstate P\n@enduml\n", None)
            .unwrap()
            .0;
        assert_eq!(tabs, plain.matches(" L").count() + 3, "{svg}");
        let y_of = |svg: &str, word: &str| -> f32 {
            let at = svg.find(&format!(">{word}</tspan>")).unwrap();
            let start = svg[..at].rfind(" y=\"").unwrap() + 4;
            svg[start..].split('"').next().unwrap().parse().unwrap()
        };
        assert!(y_of(&svg, "R") > y_of(&plain, "R") + 5.0, "{svg}");
    }

    /// A note on a transition follows its words where lines by hand move
    /// them; a state's note keeps off it, words or none; words and note
    /// stand on the side of their room toward the line.
    #[test]
    fn a_note_on_link_goes_where_its_words_go() {
        let note_and_words = |source: &str| {
            let (_, map) = crate::render_mapped(source, None).unwrap();
            let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
            (at(2), at(3))
        };
        let plain = "@startuml\n[*] --> Idle\nIdle --> Busy : start\nnote on link : counts the jobs\nBusy --> Idle : done\n@enduml\n";
        let by_hand = plain.replace(
            "@enduml",
            "' lines by hand (Omaspec)\n' Idle -> Busy at 1.5 0.5\n@enduml",
        );
        let (words, note) = note_and_words(plain);
        let (moved, moved_note) = note_and_words(&by_hand);
        assert!((moved.x - words.x).abs() > 5.0, "{words:?} {moved:?}");
        assert!(
            moved_note.y >= moved.y + moved.h,
            "{moved_note:?} under {moved:?}"
        );
        assert!(
            (moved_note.x - moved.x).abs() < (note.x - moved.x).abs(),
            "{moved_note:?}"
        );
        // A state's note keeps off the room of a transition without words.
        let source = "@startuml\nleft to right direction\n[*] --> Idle\nIdle --> Busy\nnote on link : the link's note\nnote top of Idle : a rather wide note on top of Idle\nBusy --> Done\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
        let (a, b) = (at(4), at(5));
        let apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
        assert!(apart, "{a:?} {b:?}");
        // Left of its line, the words stay by it, the note under them.
        let left = "@startuml\n[*] --> Idle\nIdle --> Busy : start\nnote on link : a much longer note than its label\n' lines by hand (Omaspec)\n' Idle -> Busy label left\n@enduml\n";
        let (words, note) = note_and_words(left);
        // Flush at the right with the note's sheet, whose words keep its
        // padding and fold inside it.
        let sheet = note.x + note.w + NOTE_PAD + 4.0;
        assert!(
            (words.x + words.w - sheet).abs() < 1.0,
            "{words:?} {note:?}"
        );
    }

    #[test]
    fn a_note_is_drawn_in_a_part_of_its_own() {
        let source = "@startuml\nstate Brewing {\n  state Grind\n  note right of Grind : fine\n  note left of Grind : dark\n}\nstate Idle\nnote left of Idle : coarse\n' parts (Omaspec)\n@enduml\n";
        let (svg, _) = crate::render_mapped(source, None).unwrap();
        let ground = crate::without_parts(&svg).unwrap();
        assert!(!ground.contains("<path d=\"M"), "{ground}");
        let groups = crate::part_groups(&svg);
        let part = |name: &str| -> String {
            groups
                .iter()
                .filter(|(n, _)| n == name)
                .map(|(_, group)| *group)
                .collect()
        };
        for (name, words) in [
            ("note:state:Grind#right0", "fine"),
            ("note:state:Grind#left0", "dark"),
            ("note:state:Idle#left0", "coarse"),
        ] {
            let group = part(name);
            assert!(
                group.contains("<path") && group.contains(words),
                "{name}: {group}"
            );
        }
        assert!(!part("state:Brewing").contains("fine"));
        // The map says which state each note lies inside.
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let inside = |part: &str| {
            map.inside
                .iter()
                .find(|(note, _)| note == part)
                .map(|(_, owner)| owner.as_str())
        };
        assert_eq!(inside("note:state:Grind#right0"), Some("state:Brewing"));
        assert_eq!(inside("note:state:Idle#left0"), None);
        // A note added before it on another side renames it not.
        let added = source.replace(
            "  note right of Grind : fine\n",
            "  note left of Grind : first\n  note right of Grind : fine\n",
        );
        let (svg, _) = crate::render_mapped(&added, None).unwrap();
        assert!(
            crate::part_groups(&svg)
                .iter()
                .any(|(n, g)| n == "note:state:Grind#right0" && g.contains("fine"))
        );
    }

    /// REQ-072: a transition from a state to the state it lies in runs
    /// inside that state up to its title, and one from the state to a
    /// state inside it comes down from there.
    #[test]
    fn a_transition_into_its_own_state_runs_to_its_title() {
        let source = "@startuml\n[*] --> Playing\nstate Playing {\n  [*] --> Live\n  Live --> Extra\n}\nExtra --> Playing : again\nPlaying --> Live : restart\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let playing = map
            .states
            .iter()
            .find(|s| s.name == "Playing")
            .unwrap()
            .rect;
        let route = |line: usize| {
            &map.transitions
                .iter()
                .find(|r| r.line == line)
                .unwrap()
                .points
        };
        let inside = |(x, y): (f32, f32)| {
            x > playing.x && x < playing.x + playing.w && y > playing.y && y < playing.y + playing.h
        };
        let (again, restart) = (route(6), route(7));
        // Inside the state all the way, its head at the title's foot.
        assert!(again.iter().all(|&p| inside(p)), "{again:?} in {playing:?}");
        let head = *again.last().unwrap();
        assert!(
            head.1 < playing.y + 50.0,
            "{head:?} below the title of {playing:?}"
        );
        assert!(restart.iter().all(|&p| inside(p)), "{restart:?}");
        assert!(restart[0].1 < playing.y + 50.0, "{restart:?}");
        // Its end on the map, which the hand tool shapes it to, is the
        // title too, not the whole state.
        let wire = map.wires.iter().find(|w| w.line == Some(6)).unwrap();
        assert!(
            wire.to.rect.h < playing.h / 2.0,
            "{:?} of {playing:?}",
            wire.to.rect
        );
    }

    /// A state whose name is that of another part (the title's, a
    /// mark's) or that has a name of its own besides its key is drawn in
    /// a part of its own, the one the map names.
    #[test]
    fn a_state_never_shares_another_part() {
        let source = "@startuml\ntitle Year\nstate \"frame:title\"\nstate \"mark:0\"\nstate \"Long name\" as L\n[*] --> L\n' parts (Omaspec)\n@enduml\n";
        let (svg, map) = crate::render_mapped(source, None).unwrap();
        let groups = crate::part_groups(&svg);
        let title: Vec<&str> = groups
            .iter()
            .filter(|(n, _)| n == crate::TITLE_PART)
            .map(|(_, g)| *g)
            .collect();
        assert!(
            title.len() == 1 && !title[0].contains("frame:title"),
            "{title:?}"
        );
        for spot in &map.states {
            let mine: Vec<&str> = groups
                .iter()
                .filter(|(n, _)| *n == spot.part)
                .map(|(_, g)| *g)
                .collect();
            assert!(!mine.is_empty(), "{} not drawn: {groups:?}", spot.part);
            let marks = mine.iter().filter(|g| g.contains("<ellipse")).count();
            assert!(
                spot.name.is_empty() || marks == 0,
                "{}: {mine:?}",
                spot.name
            );
        }
    }

    /// Asked for its parts, a state diagram draws each state, each mark
    /// and each line's words in groups of their own, named as its map
    /// names them; a composite state's groups hold nothing of the states
    /// in it.
    #[test]
    fn the_pane_s_parts_are_named_as_the_map_names_them() {
        let source = "@startuml\n[*] --> Off\nOff --> Brewing : start\nstate Brewing {\n  [*] --> Grinding\n  Grinding --> Pouring : ground\n  Pouring --> [*]\n}\nBrewing --> Off : done\n' parts (Omaspec)\n@enduml\n";
        let (svg, map) = crate::render_mapped(source, None).unwrap();
        let groups = crate::part_groups(&svg);
        let named: std::collections::HashSet<&str> =
            groups.iter().map(|(name, _)| name.as_str()).collect();
        for spot in &map.states {
            assert!(
                named.contains(spot.part.as_str()),
                "{} not drawn: {named:?}",
                spot.part
            );
        }
        for wire in map.wires.iter().filter(|w| !w.labels.is_empty()) {
            let words = wire.words[crate::map::WORDS].as_ref().unwrap();
            assert!(
                named.contains(words.as_str()),
                "{words} not drawn: {named:?}"
            );
        }
        assert!(
            named.contains("mark:0") && named.contains("mark:2.0"),
            "{named:?}"
        );
        // Brewing's groups: its frame, its divider and its title only.
        for (_, group) in groups.iter().filter(|(name, _)| name == "state:Brewing") {
            assert!(group.matches("<rect").count() <= 1, "{group}");
            assert!(!group.contains("Grinding"), "{group}");
        }
        // Without the line, the same picture has none.
        let plain = crate::render(&source.replace("' parts (Omaspec)\n", "")).unwrap();
        assert!(!plain.contains("part-"));
    }

    /// A state that holds others is a frame: its fill steps toward the
    /// paper, the states inside keep theirs; one colored by hand stays.
    #[test]
    fn a_composite_state_is_filled_lighter_than_the_states_in_it() {
        let (root, scene) = laid(
            "@startuml\nstate On {\n  state Idle\n}\nstate Off #lightblue {\n  state Dark\n}\nstate Alone\n@enduml\n",
        );
        let (on_node, on) = at(&root.nodes, &scene, "On");
        let (_, idle) = at(&on_node.children.nodes, &on.children, "Idle");
        let (_, alone) = at(&root.nodes, &scene, "Alone");
        let fill = |placed: &Placed| placed.item.fill.expect("filled");
        assert_eq!(fill(idle), fill(alone));
        assert_eq!(fill(on), fill(alone).mix(Color::WHITE, FRAME_LIGHT));
        assert_ne!(fill(on), fill(alone));
        let (_, off) = at(&root.nodes, &scene, "Off");
        assert_eq!(fill(off), color::parse("lightblue").unwrap().unwrap());
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
            .find(|node| node.item.kind == kind)
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

    const BY_HAND: &str = "@startuml\nskinparam linetype ortho\n[*] --> Idle\nIdle --> Running : start\nRunning --> Idle : stop\nRunning --> Idle : fail\nstate Busy {\n  [*] --> Work\n  Work --> [H]\n}\nIdle --> Busy\n@enduml\n";

    fn wire(map: &crate::map::Map, key: &str) -> crate::map::Wire {
        map.wires
            .iter()
            .find(|w| w.key.to_string() == key)
            .unwrap_or_else(|| panic!("no line {key}"))
            .clone()
    }

    #[test]
    fn a_label_takes_the_side_the_writer_chose() {
        let source = "@startuml\n[*] --> Idle\nIdle --> Busy : go\n@enduml\n";
        let (_, plain) = crate::render_mapped(source, None).unwrap();
        let go = wire(&plain, "Idle -> Busy");
        let label = go.labels[0];
        let line_x = go.path[0].0;
        // Right of a line running down, as the layout has it.
        assert!(label.x > line_x, "{label:?} right of {line_x}");
        let left = format!("{source}' lines by hand (Omaspec)\n' Idle -> Busy label left\n");
        let (_, plain) = crate::render_mapped(&left, None).unwrap();
        let go = wire(&plain, "Idle -> Busy");
        let label = go.labels[0];
        let line_x = go.path[0].0;
        assert!(label.x + label.w < line_x, "{label:?} left of {line_x}");
        // Across the flow: above or below the line.
        let across =
            "@startuml\nleft to right direction\n[*] --> Idle\nIdle --> Busy : go\n@enduml\n";
        let (_, plain) = crate::render_mapped(across, None).unwrap();
        let above = wire(&plain, "Idle -> Busy").labels[0];
        let below = format!("{across}' lines by hand (Omaspec)\n' Idle -> Busy label below\n");
        let (_, plain) = crate::render_mapped(&below, None).unwrap();
        let moved = wire(&plain, "Idle -> Busy").labels[0];
        assert!(moved.y > above.y, "{moved:?} below {above:?}");
        // The entry reads back as written.
        let labels = crate::hand::labels(&below);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].to_string(), "Idle -> Busy label below");
        assert!(!labels[0].side.before());
    }

    #[test]
    fn a_line_by_hand_is_drawn_as_it_was_shaped() {
        let (_, plain) = crate::render_mapped(BY_HAND, None).unwrap();
        let keys: Vec<String> = plain.wires.iter().map(|w| w.key.to_string()).collect();
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
        assert!(plain.misses.is_empty() && plain.wires.iter().all(|w| w.entry.is_none()));
        // Out to the left of both boxes, down, and back in.
        let stop = wire(&plain, "Running -> Idle");
        let (a, b) = (stop.from.rect, stop.to.rect);
        let x = a.x.min(b.x) - 80.0;
        let corners = [(x, a.y + a.h / 2.0), (x, b.y + b.h / 2.0)];
        let shaped = stop.entry_for(&corners);
        // A corner in its own box, a line that is not there, a corner far
        // off.
        let fail = wire(&plain, "Running -> Idle #2");
        let inside = fail.entry_for(&[(fail.from.rect.x + 5.0, fail.from.rect.y + 5.0)]);
        let busy = wire(&plain, "Idle -> Busy");
        let far = crate::hand::Line {
            key: busy.key.clone(),
            corners: vec![crate::hand::Corner { x: 0.5, y: 900.0 }],
        };
        let source = format!(
            "{}\n' lines by hand (Omaspec)\n' {shaped}\n' {inside}\n' Nowhere -> Idle at 0.5 0.5\n' {far}\n@enduml\n",
            BY_HAND.trim_end_matches("@enduml\n")
        );
        let (_, map) = crate::render_mapped(&source, None).unwrap();
        let drawn = wire(&map, "Running -> Idle");
        assert_eq!((drawn.line, drawn.entry), (Some(4), Some(13)));
        // Exactly as the shape the pane shows while dragging, in the
        // drawing's units; the picture grew to the left for it.
        let expected: Vec<(f32, f32)> = stop
            .shaped(&corners, plain.fit)
            .into_iter()
            .map(|p| plain.fit.back(p))
            .collect();
        let got: Vec<(f32, f32)> = drawn.path.iter().map(|&p| map.fit.back(p)).collect();
        assert_eq!(got.len(), 4, "{got:?}");
        for (g, e) in got.iter().zip(&expected) {
            assert!(
                (g.0 - e.0).abs() < 0.5 && (g.1 - e.1).abs() < 0.5,
                "{got:?} {expected:?}"
            );
        }
        // The drawn line turns close by each corner.
        for corner in corners {
            let c = map.fit.point(plain.fit.back(corner));
            let near = drawn
                .points
                .iter()
                .map(|p| (p.0 - c.0).hypot(p.1 - c.1))
                .fold(f32::INFINITY, f32::min);
            assert!(near < 4.0, "{near} from {c:?}");
        }
        assert_eq!(
            map.misses,
            [
                (
                    14,
                    "A corner of Running -> Idle #2 lies in a box at its end and is left out"
                        .to_owned()
                ),
                (
                    15,
                    "No line runs as Nowhere -> Idle; its shape by hand is left out".to_owned()
                ),
                (
                    16,
                    "A corner of Idle -> Busy lies far off the diagram and is left out".to_owned()
                ),
            ]
        );
        // The others stay as the layout drew them.
        let start = |m: &crate::map::Map| -> Vec<(i32, i32)> {
            wire(m, "[*] -> Idle")
                .path
                .iter()
                .map(|&p| {
                    let (x, y) = m.fit.back(p);
                    (x.round() as i32, y.round() as i32)
                })
                .collect()
        };
        assert_eq!(start(&plain), start(&map));
    }

    /// docs/adr/0021: a state held before another stands left of it, in
    /// its rank; an entry that does not hold is said.
    #[test]
    fn a_state_held_beside_another_stands_there() {
        let source = "@startuml\n[*] --> Idle\nIdle --> Running : start\nRunning --> Idle : stop\nIdle --> Paused : pause\nPaused --> Idle : resume\nRunning --> Done : finish\n@enduml\n";
        let spot = |m: &crate::map::Map, name: &str| {
            m.states.iter().find(|s| s.name == name).unwrap().rect
        };
        let (_, free) = crate::render_mapped(source, None).unwrap();
        assert!(spot(&free, "Running").x < spot(&free, "Paused").x);
        let held = source.replace(
            "@enduml",
            "' lines by hand (Omaspec)\n' Paused before Running\n' Done after Ghost\n' Idle after Running\n@enduml",
        );
        let (_, map) = crate::render_mapped(&held, None).unwrap();
        assert!(spot(&map, "Paused").x < spot(&map, "Running").x);
        assert!((spot(&map, "Paused").y - spot(&map, "Running").y).abs() < 0.5);
        assert_eq!(
            map.misses,
            [
                (
                    9,
                    "No box is named Ghost; Done after Ghost is left out".to_owned()
                ),
                (
                    10,
                    "Idle after Running: a line joins boxes this puts in one rank as Running, \
                     which cannot hold both; Idle stands where the layout puts it"
                        .to_owned()
                ),
            ]
        );
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
    fn a_note_under_a_state_keeps_off_the_lines_into_it() {
        let (_, scene) = laid(
            "@startuml\n[*] --> Docked\nDocked --> Cleaning : schedule\nCleaning --> Returning : battery low\nReturning --> Docked : arrived\nCleaning --> Stuck : wheel blocked\nStuck --> Docked : human lifts it\nCleaning --> Docked : done\nnote bottom of Docked : Where it spends most of its life\n@enduml\n",
        );
        let note = &scene.notes[0];
        let area = (note.x, note.y, note.w, note.h);
        for edge in &scene.edges {
            for pair in edge.points.windows(2) {
                assert!(
                    !segment_meets(pair[0], pair[1], area),
                    "{pair:?} through the note at {area:?}"
                );
            }
            if !edge.label.is_empty() {
                assert!(!boxes_overlap(area, label_box(edge)), "{area:?}");
            }
        }
        // Moved off its state, it is tied to it.
        assert!(note.tether.is_some());
        assert!(segment_meets(
            (0.0, 5.0),
            (10.0, 5.0),
            (4.0, 0.0, 2.0, 10.0)
        ));
        assert!(!segment_meets(
            (0.0, 5.0),
            (3.0, 5.0),
            (4.0, 0.0, 2.0, 10.0)
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
        link_crossings(
            &diagram.root,
            &mut scene,
            &skin,
            canvas.family(),
            &|_, _| true,
        )
        .unwrap();
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
            x: placed.rect.x + (placed.rect.w - placed.item.title.width) / 2.0,
            w: placed.item.title.width,
            h: placed.item.band(),
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
