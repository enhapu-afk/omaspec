//! Draws a signal flow. The graph ranks the blocks left to right and
//! leaves room for a wire's label. The wires themselves are not the
//! graph's routes: those end at the center of a box, and a signal ends
//! at a port.

use super::skin::Skin;
use super::{Block, Diagram, End, Port, Wire};
use crate::Declined;
use crate::annotations::Annotations;
use crate::graph::{self, Direction, Edge, Node};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Family, Font};

const MARGIN: f32 = 10.0;
const PAD_X: f32 = 14.0;
const PAD_Y: f32 = 8.0;
const PORT_GAP: f32 = 8.0;
const NAME_GAP: f32 = 6.0;
const PITCH: f32 = 18.0;
const DOT: f32 = 2.5;
/// The smallest room kept for a port name inside the box.
const NAME_ROOM: f32 = 8.0;
const HEAD: f32 = 8.0;
const HEAD_HALF: f32 = 3.5;
/// How far a second wire from the same output stands off the first.
const FORK: f32 = 8.0;
const GUTTER: f32 = 20.0;
const BOX_PAD: f32 = 4.0;
/// Clear space between a wire and the line box of the name below it.
const PORT_CLEAR: f32 = 4.0;
/// Room under a frame-port name, from its line box down to its own wire.
/// Larger than [`PORT_CLEAR`]: the font draws its ink low in the line box,
/// so a descender or an underscore would meet the stroke while the gap
/// above the letters stayed wide. The two gaps then look alike.
const NAME_LIFT: f32 = 8.0;
/// Space between a boundary name and its dot.
const SIGNAL_GAP: f32 = 12.0;
/// Radius of the jump a vertical wire takes over a signal it crosses.
/// Two ports are at least [`PITCH`] apart, so the jump fits between them.
const HOP: f32 = 4.0;

pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    let skin = Skin::from_params(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    let family = canvas.family();
    let notes = Annotations::new(
        &diagram.frame,
        [skin.title, skin.header, skin.footer, skin.caption],
        family,
    )?;
    if !diagram.inputs.is_empty() || !diagram.blocks.is_empty() || !diagram.outputs.is_empty() {
        paint(&mut canvas, diagram, &skin, family, notes.top())?;
    }
    notes.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, skin.background))
}

struct Placed {
    rect: graph::Rect,
    /// The block name's height. A boundary signal has none.
    name_h: Option<f32>,
    /// Set when the block is a subsystem. Port dots are relative to `rect`.
    inner: Option<Inner>,
}

#[derive(Clone)]
struct BlockLayout {
    width: f32,
    height: f32,
    name_h: f32,
    inner: Option<Inner>,
}

/// A subsystem's contents, in coordinates whose origin is the frame's
/// top left.
#[derive(Clone)]
struct Inner {
    children: Vec<ChildAt>,
    wires: Vec<WirePath>,
    input_at: Vec<(f32, f32)>,
    output_at: Vec<(f32, f32)>,
}

#[derive(Clone)]
struct ChildAt {
    x: f32,
    y: f32,
    layout: BlockLayout,
}

#[derive(Clone)]
struct WirePath {
    points: Vec<(f32, f32)>,
    label: Option<String>,
    line: usize,
    /// Where a fan-out leaves its stem. Drawn once, on top of the corner.
    junction: Option<(f32, f32)>,
}

/// Not a child block. `route` must not treat two boundary ends as one block.
const BOUNDARY: usize = usize::MAX;

fn paint(
    canvas: &mut Canvas,
    diagram: &Diagram,
    skin: &Skin,
    family: Family,
    top: f32,
) -> Result<(), Declined> {
    let measured = metrics(diagram, skin, family)?;
    let nodes: Vec<Node> = measured
        .nodes
        .iter()
        .map(|m| Node {
            width: m.0,
            height: m.1,
            before: 0.0,
            loop_extra: 0.0,
            flat: true,
        })
        .collect();
    // A wire between two ports of one block is drawn around that block.
    // It is not an edge: a self-loop would reserve room the wire does not use.
    // The route is drawn below, to the ports. An edge only ranks the
    // blocks and widens the gap for the label, so it stays hidden.
    let mut edges = Vec::new();
    for wire in &diagram.wires {
        let from = node_index(diagram, &wire.from);
        let to = node_index(diagram, &wire.to);
        if from == to {
            continue;
        }
        edges.push(Edge {
            from,
            to,
            label_width: label_width(wire, skin, family)?,
            label_height: 0.0,
            hidden: true,
        });
    }
    let layout = graph::layout(&nodes, &edges, Direction::Right);
    let mut placed: Vec<Placed> = layout
        .nodes
        .into_iter()
        .zip(measured.nodes)
        .map(|(rect, (_, _, name_h))| Placed {
            rect: graph::Rect {
                x: rect.x,
                y: rect.y + top,
                w: rect.w,
                h: rect.h,
            },
            name_h,
            inner: None,
        })
        .collect();
    let n_in = diagram.inputs.len();
    for (index, layout) in measured.blocks.into_iter().enumerate() {
        placed[n_in + index].inner = layout.inner;
    }

    align_left_edges(&mut placed);
    align_placed(diagram, &mut placed);
    let rects: Vec<graph::Rect> = placed.iter().map(|p| p.rect).collect();
    let spines = fan_spines(diagram.wires.len(), &rects, |index| {
        let wire = &diagram.wires[index];
        (
            pin(diagram, &placed, &wire.from, true),
            pin(diagram, &placed, &wire.to, false),
            node_index(diagram, &wire.from),
        )
    });
    let scene = Scene {
        diagram,
        placed: &placed,
        rects: &rects,
        spines: &spines,
        gutter: rects.iter().map(|r| r.y).fold(f32::INFINITY, f32::min) - GUTTER,
    };
    let mut gutter_n = 0.0;
    let mut paths = Vec::new();
    for (index, wire) in diagram.wires.iter().enumerate() {
        let (points, next) = wire_points(&scene, wire, index, gutter_n)?;
        gutter_n = next;
        let junction = branch_point(spines[index], &points);
        paths.push(WirePath {
            points,
            label: wire.label.clone(),
            line: wire.line,
            junction,
        });
    }
    // A wire over the top would otherwise run through the title, which
    // occupies everything above `top`.
    let min_y = paths
        .iter()
        .flat_map(|wire| wire.points.iter())
        .map(|point| point.1)
        .chain(placed.iter().map(|item| item.rect.y))
        .fold(f32::INFINITY, f32::min);
    let shift = if min_y.is_finite() {
        (top - min_y).max(0.0)
    } else {
        0.0
    };
    if shift > 0.0 {
        for placed in &mut placed {
            placed.rect.y += shift;
        }
        for wire in &mut paths {
            for point in &mut wire.points {
                point.1 += shift;
            }
            if let Some(at) = &mut wire.junction {
                at.1 += shift;
            }
        }
    }
    // After the shift, so a jump uses the coordinates that are drawn.
    hop_paths(&mut paths);

    let n_blocks = diagram.blocks.len();
    for (index, port) in diagram.inputs.iter().enumerate() {
        boundary(canvas, skin, family, port, &placed[index].rect, true)?;
    }
    for (index, block) in diagram.blocks.iter().enumerate() {
        draw_block(canvas, skin, family, block, &placed[n_in + index])?;
    }
    for (index, port) in diagram.outputs.iter().enumerate() {
        boundary(
            canvas,
            skin,
            family,
            port,
            &placed[n_in + n_blocks + index].rect,
            false,
        )?;
    }
    let mut junctions = Vec::new();
    for wire in &paths {
        if let Some(at) = wire.junction {
            paint_junction(canvas, skin, at, &mut junctions);
        }
        paint_path(
            canvas,
            skin,
            family,
            Layer::Lifelines,
            &wire.points,
            wire.label.as_deref(),
            wire.line,
        )?;
    }
    Ok(())
}

struct Scene<'a> {
    diagram: &'a Diagram,
    placed: &'a [Placed],
    rects: &'a [graph::Rect],
    /// One shared bend x per wire that leaves a fanned output. `None` otherwise.
    spines: &'a [Option<f32>],
    gutter: f32,
}

struct Measured {
    /// `(width, height, name height)` in drawing order: inputs, blocks, outputs.
    nodes: Vec<(f32, f32, Option<f32>)>,
    blocks: Vec<BlockLayout>,
}

/// Node sizes in drawing order, and one layout per block.
fn metrics(diagram: &Diagram, skin: &Skin, family: Family) -> Result<Measured, Declined> {
    let mut nodes = Vec::new();
    // One width for the whole column, so the dots share an x and every
    // wire into the first block (and out of the last) has the same length.
    let mut inputs = Vec::new();
    for port in &diagram.inputs {
        inputs.push(boundary_size(port, skin, family)?);
    }
    let in_w = inputs.iter().map(|size| size.0).fold(0.0, f32::max);
    for (_, height) in inputs {
        nodes.push((in_w, height, None));
    }
    let mut blocks = Vec::new();
    for block in &diagram.blocks {
        let layout = measure_block(block, skin, family)?;
        nodes.push((layout.width, layout.height, Some(layout.name_h)));
        blocks.push(layout);
    }
    let mut outputs = Vec::new();
    for port in &diagram.outputs {
        outputs.push(boundary_size(port, skin, family)?);
    }
    let out_w = outputs.iter().map(|size| size.0).fold(0.0, f32::max);
    for (_, height) in outputs {
        nodes.push((out_w, height, None));
    }
    Ok(Measured { nodes, blocks })
}

fn measure_block(block: &Block, skin: &Skin, family: Family) -> Result<BlockLayout, Declined> {
    if block.blocks.is_empty() && block.wires.is_empty() {
        let (width, height, name_h) = block_size(block, skin, family)?;
        return Ok(BlockLayout {
            width,
            height,
            name_h,
            inner: None,
        });
    }
    measure_subsystem(block, skin, family)
}

fn measure_subsystem(block: &Block, skin: &Skin, family: Family) -> Result<BlockLayout, Declined> {
    let name = measured(&block.label, skin.label.bold(), family, block.line)?;
    let header = PAD_Y + name.height + NAME_GAP;
    let label_h =
        label_height(&block.inputs, skin, family)?.max(label_height(&block.outputs, skin, family)?);
    let rows = block.inputs.len().max(block.outputs.len()).max(1);
    let pitch = port_pitch(rows, label_h);
    // The first wire sits half a row below `band`. A port name reaches
    // `label_h + NAME_LIFT` above that wire, and `band` keeps it under the title.
    let lift = if label_h > 0.0 {
        label_h + NAME_LIFT
    } else {
        0.0
    };
    let band = header + (lift - pitch / 2.0).max(0.0);
    let mut kids = Vec::new();
    for child in &block.blocks {
        kids.push(measure_block(child, skin, family)?);
    }
    let nodes: Vec<Node> = kids
        .iter()
        .map(|kid| Node {
            width: kid.width,
            height: kid.height,
            before: 0.0,
            loop_extra: 0.0,
            flat: true,
        })
        .collect();
    let mut edges = Vec::new();
    for wire in &block.wires {
        let Some(from) = child_index(&block.blocks, &wire.from) else {
            continue;
        };
        let Some(to) = child_index(&block.blocks, &wire.to) else {
            continue;
        };
        if from == to {
            continue;
        }
        edges.push(Edge {
            from,
            to,
            label_width: label_width(wire, skin, family)?,
            label_height: 0.0,
            hidden: true,
        });
    }
    let laid = graph::layout(&nodes, &edges, Direction::Right);
    let in_w = port_width(&block.inputs, skin, family)?;
    let out_w = port_width(&block.outputs, skin, family)?;
    // The name sits in the pad, and the wire runs under it in a straight
    // line to the inner port, so the pad is only as wide as that name.
    let left = if block.inputs.is_empty() {
        PAD_X
    } else {
        PAD_X + in_w + 8.0
    };
    let right = if block.outputs.is_empty() {
        PAD_X
    } else {
        8.0 + out_w + PAD_X
    };
    let content_h = laid.height.max(rows as f32 * pitch);
    let width = (left + laid.width + right).max(name.width + 2.0 * PAD_X);
    let mut children = Vec::new();
    for (rect, layout) in laid.nodes.into_iter().zip(kids) {
        children.push(ChildAt {
            x: left + rect.x,
            y: band + rect.y,
            layout,
        });
    }
    align_children(block, &mut children);
    let min_child = children
        .iter()
        .map(|child| child.y)
        .fold(f32::INFINITY, f32::min);
    let child_shift = if min_child.is_finite() {
        (band - min_child).max(0.0)
    } else {
        0.0
    };
    if child_shift > 0.0 {
        for child in &mut children {
            child.y += child_shift;
        }
    }
    align_left_edges_at(&mut children);
    let mut input_at: Vec<(f32, f32)> = centers_at(block.inputs.len(), rows, band, pitch)
        .into_iter()
        .map(|y| (0.0, y))
        .collect();
    let mut output_at: Vec<(f32, f32)> = centers_at(block.outputs.len(), rows, band, pitch)
        .into_iter()
        .map(|y| (width, y))
        .collect();
    snap_frame_dots(block, &children, &mut input_at, &mut output_at);
    let rects: Vec<graph::Rect> = children
        .iter()
        .map(|child| graph::Rect {
            x: child.x,
            y: child.y,
            w: child.layout.width,
            h: child.layout.height,
        })
        .collect();
    let gutter = rects
        .iter()
        .map(|rect| rect.y)
        .fold(f32::INFINITY, f32::min);
    let gutter = if gutter.is_finite() {
        gutter - GUTTER
    } else {
        band - GUTTER
    };
    let spines = fan_spines(block.wires.len(), &rects, |index| {
        let wire = &block.wires[index];
        (
            inner_pin(block, &children, &input_at, &output_at, &wire.from, true),
            inner_pin(block, &children, &input_at, &output_at, &wire.to, false),
            end_index(&block.blocks, &wire.from),
        )
    });
    let mut gutter_n = 0.0;
    let mut wires = Vec::new();
    for (index, wire) in block.wires.iter().enumerate() {
        let from = inner_pin(block, &children, &input_at, &output_at, &wire.from, true);
        let to = inner_pin(block, &children, &input_at, &output_at, &wire.to, false);
        let bump = block.wires[..index]
            .iter()
            .filter(|other| other.from == wire.from)
            .count() as f32
            * FORK;
        let ends = (
            end_index(&block.blocks, &wire.from),
            end_index(&block.blocks, &wire.to),
        );
        let lane = lane_of(&block.wires, index, ends, |wire| {
            (
                end_index(&block.blocks, &wire.from),
                end_index(&block.blocks, &wire.to),
            )
        });
        let (points, next) = route(
            from,
            to,
            ends,
            &rects,
            bump,
            (lane.0, lane.1, spines[index]),
            (gutter, gutter_n),
        );
        gutter_n = next;
        let points = simplify(points);
        let junction = branch_point(spines[index], &points);
        wires.push(WirePath {
            points,
            label: wire.label.clone(),
            line: wire.line,
            junction,
        });
    }
    // Keep every wire below the block's name.
    let mut min_x = 0.0_f32;
    let mut min_y = band;
    let mut max_x = width;
    let mut max_y = band + content_h;
    for child in &children {
        max_y = max_y.max(child.y + child.layout.height);
    }
    for wire in &wires {
        for point in &wire.points {
            min_x = min_x.min(point.0);
            min_y = min_y.min(point.1);
            max_x = max_x.max(point.0);
            max_y = max_y.max(point.1);
        }
    }
    let shift_x = (-min_x).max(0.0);
    let shift_y = (band - min_y).max(0.0);
    if shift_x > 0.0 || shift_y > 0.0 {
        for child in &mut children {
            child.x += shift_x;
            child.y += shift_y;
        }
        for dot in &mut input_at {
            dot.0 += shift_x;
            dot.1 += shift_y;
        }
        for dot in &mut output_at {
            dot.0 += shift_x;
            dot.1 += shift_y;
        }
        for wire in &mut wires {
            for point in &mut wire.points {
                point.0 += shift_x;
                point.1 += shift_y;
            }
            if let Some(at) = &mut wire.junction {
                at.0 += shift_x;
                at.1 += shift_y;
            }
        }
    }
    // The jump bulges past the straight stub. The frame has to contain it.
    hop_paths(&mut wires);
    let mut right = (max_x + shift_x).max(width + shift_x);
    let mut bottom = max_y + shift_y;
    for wire in &wires {
        for &(x, y) in &wire.points {
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    Ok(BlockLayout {
        width: right,
        height: bottom + PAD_Y,
        name_h: name.height,
        inner: Some(Inner {
            children,
            wires,
            input_at,
            output_at,
        }),
    })
}

fn label_height(ports: &[Port], skin: &Skin, family: Family) -> Result<f32, Declined> {
    let mut height = 0.0_f32;
    for port in ports {
        height = height.max(measured(&port.name, skin.label, family, port.line)?.height);
    }
    Ok(height)
}

fn child_index(blocks: &[Block], end: &End) -> Option<usize> {
    match end {
        End::Port { block, .. } => blocks.iter().position(|b| b.name == block.as_str()),
        End::Boundary(_) => None,
    }
}

fn end_index(blocks: &[Block], end: &End) -> usize {
    child_index(blocks, end).unwrap_or(BOUNDARY)
}

fn inner_pin(
    block: &Block,
    children: &[ChildAt],
    input_at: &[(f32, f32)],
    output_at: &[(f32, f32)],
    end: &End,
    source: bool,
) -> (f32, f32) {
    match end {
        End::Boundary(name) => {
            let (ports, dots) = if source {
                (&block.inputs, input_at)
            } else {
                (&block.outputs, output_at)
            };
            let at = ports
                .iter()
                .position(|port| port.name == name.as_str())
                .expect("a boundary end was declared");
            dots[at]
        }
        End::Port { block: name, port } => {
            let index = block
                .blocks
                .iter()
                .position(|child| child.name == name.as_str())
                .expect("a block end was declared");
            let child = &children[index];
            port_dot(
                &block.blocks[index],
                &child.layout,
                (child.x, child.y),
                port,
                source,
            )
        }
    }
}

fn port_dot(
    block: &Block,
    layout: &BlockLayout,
    origin: (f32, f32),
    port: &str,
    source: bool,
) -> (f32, f32) {
    if let Some(inner) = &layout.inner {
        let (ports, dots) = if source {
            (&block.outputs, &inner.output_at)
        } else {
            (&block.inputs, &inner.input_at)
        };
        let at = ports
            .iter()
            .position(|item| item.name == port)
            .expect("a port end was declared");
        return (origin.0 + dots[at].0, origin.1 + dots[at].1);
    }
    let rows = block.inputs.len().max(block.outputs.len()).max(1);
    let band = origin.1 + PAD_Y + layout.name_h + NAME_GAP;
    let (ports, x) = if source {
        (&block.outputs, origin.0 + layout.width)
    } else {
        (&block.inputs, origin.0)
    };
    let ys = centers_at(ports.len(), rows, band, port_pitch(rows, layout.name_h));
    let at = ports
        .iter()
        .position(|item| item.name == port)
        .expect("a port end was declared");
    (x, ys[at])
}

fn boundary_size(port: &Port, skin: &Skin, family: Family) -> Result<(f32, f32), Declined> {
    let text = measured(&port.name, skin.label, family, port.line)?;
    Ok((text.width + SIGNAL_GAP, text.height.max(PITCH)))
}

fn block_size(block: &Block, skin: &Skin, family: Family) -> Result<(f32, f32, f32), Declined> {
    let name = measured(&block.label, skin.label.bold(), family, block.line)?;
    let in_w = port_width(&block.inputs, skin, family)?;
    let out_w = port_width(&block.outputs, skin, family)?;
    let rows = block.inputs.len().max(block.outputs.len()).max(1);
    let pitch = port_pitch(rows, name.height);
    let width = (PAD_X + in_w.max(NAME_ROOM) + PORT_GAP + out_w.max(NAME_ROOM) + PAD_X)
        .max(name.width + 2.0 * PAD_X);
    let height = PAD_Y + name.height + NAME_GAP + rows as f32 * pitch + PAD_Y;
    Ok((width, height, name.height))
}

fn port_width(ports: &[Port], skin: &Skin, family: Family) -> Result<f32, Declined> {
    let mut width = 0.0_f32;
    for port in ports {
        width = width.max(measured(&port.name, skin.label, family, port.line)?.width);
    }
    Ok(width)
}

fn label_width(wire: &Wire, skin: &Skin, family: Family) -> Result<f32, Declined> {
    match &wire.label {
        Some(text) => Ok(measured(text, skin.arrow_font, family, wire.line)?.width),
        None => Ok(0.0),
    }
}

fn node_index(diagram: &Diagram, end: &End) -> usize {
    match end {
        End::Boundary(name) => {
            if let Some(index) = diagram.inputs.iter().position(|p| p.name == name.as_str()) {
                return index;
            }
            let index = diagram
                .outputs
                .iter()
                .position(|p| p.name == name.as_str())
                .expect("a boundary end was declared");
            diagram.inputs.len() + diagram.blocks.len() + index
        }
        End::Port { block, .. } => {
            let index = diagram
                .blocks
                .iter()
                .position(|b| b.name == block.as_str())
                .expect("a block end was declared");
            diagram.inputs.len() + index
        }
    }
}

fn boundary(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    port: &Port,
    rect: &graph::Rect,
    input: bool,
) -> Result<(), Declined> {
    let label = measured(&port.name, skin.label, family, port.line)?;
    let cy = rect.y + rect.h / 2.0;
    let text_y = rect.y + (rect.h - label.height) / 2.0;
    let dot = (DOT, DOT);
    if input {
        // Right edge against the dot, so a short name keeps the same gap as a long one.
        canvas.text(
            Layer::Front,
            &label,
            (rect.x + rect.w - SIGNAL_GAP, text_y),
            Align::Right,
        );
        canvas.ellipse(
            Layer::Front,
            (rect.x + rect.w, cy),
            dot,
            Some(skin.border),
            None,
        );
    } else {
        canvas.ellipse(Layer::Front, (rect.x, cy), dot, Some(skin.border), None);
        canvas.text(
            Layer::Front,
            &label,
            (rect.x + SIGNAL_GAP, text_y),
            Align::Left,
        );
    }
    Ok(())
}

fn draw_block(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    block: &Block,
    placed: &Placed,
) -> Result<(), Declined> {
    if placed.inner.is_some() {
        return draw_frame(canvas, skin, family, block, placed);
    }
    let rect = placed.rect;
    canvas.rect(
        Layer::Frames,
        (rect.x, rect.y, rect.w, rect.h),
        skin.round,
        Some(skin.fill),
        Some(Stroke::new(skin.border, 1.0)),
    );
    let name = measured(&block.label, skin.label.bold(), family, block.line)?;
    canvas.text(
        Layer::Front,
        &name,
        (rect.x + rect.w / 2.0, rect.y + PAD_Y),
        Align::Center,
    );
    let rows = block.inputs.len().max(block.outputs.len()).max(1);
    let band = rect.y + PAD_Y + placed.name_h.unwrap_or(name.height) + NAME_GAP;
    let pitch = port_pitch(rows, placed.name_h.unwrap_or(name.height));
    paint_ports(
        canvas,
        skin,
        family,
        &block.inputs,
        &centers_at(block.inputs.len(), rows, band, pitch),
        rect.x,
        true,
    )?;
    paint_ports(
        canvas,
        skin,
        family,
        &block.outputs,
        &centers_at(block.outputs.len(), rows, band, pitch),
        rect.x + rect.w,
        false,
    )?;
    Ok(())
}

fn draw_frame(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    block: &Block,
    placed: &Placed,
) -> Result<(), Declined> {
    let inner = placed.inner.as_ref().expect("a frame has contents");
    let rect = placed.rect;
    canvas.rect(
        Layer::Frames,
        (rect.x, rect.y, rect.w, rect.h),
        skin.round,
        Some(skin.fill),
        Some(Stroke::new(skin.border, 1.0)),
    );
    let name = measured(&block.label, skin.label.bold(), family, block.line)?;
    canvas.text(
        Layer::Front,
        &name,
        (rect.x + rect.w / 2.0, rect.y + PAD_Y),
        Align::Center,
    );
    for (child, at) in block.blocks.iter().zip(&inner.children) {
        let child_placed = Placed {
            rect: graph::Rect {
                x: rect.x + at.x,
                y: rect.y + at.y,
                w: at.layout.width,
                h: at.layout.height,
            },
            name_h: Some(at.layout.name_h),
            inner: at.layout.inner.clone(),
        };
        draw_block(canvas, skin, family, child, &child_placed)?;
    }
    let mut junctions = Vec::new();
    for wire in &inner.wires {
        let points: Vec<(f32, f32)> = wire
            .points
            .iter()
            .map(|(x, y)| (rect.x + x, rect.y + y))
            .collect();
        // Above the frame fill, which would hide a wire on the lifeline layer.
        paint_path(
            canvas,
            skin,
            family,
            Layer::Bars,
            &points,
            wire.label.as_deref(),
            wire.line,
        )?;
        if let Some((x, y)) = wire.junction {
            paint_junction(canvas, skin, (rect.x + x, rect.y + y), &mut junctions);
        }
    }
    paint_frame_ports(
        canvas,
        skin,
        family,
        &block.inputs,
        &inner.input_at,
        rect,
        true,
    )?;
    paint_frame_ports(
        canvas,
        skin,
        family,
        &block.outputs,
        &inner.output_at,
        rect,
        false,
    )
}

fn paint_frame_ports(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    ports: &[Port],
    dots: &[(f32, f32)],
    rect: graph::Rect,
    input: bool,
) -> Result<(), Declined> {
    for (port, &(dx, dy)) in ports.iter().zip(dots) {
        let x = rect.x + dx;
        let y = rect.y + dy;
        canvas.ellipse(Layer::Front, (x, y), (DOT, DOT), Some(skin.border), None);
        let label = measured(&port.name, skin.label, family, port.line)?;
        let (at, align) = if input {
            (x + 8.0, Align::Left)
        } else {
            (x - 8.0, Align::Right)
        };
        canvas.text(
            Layer::Front,
            &label,
            (at, y - label.height - NAME_LIFT),
            align,
        );
    }
    Ok(())
}

fn paint_ports(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    ports: &[Port],
    ys: &[f32],
    edge: f32,
    input: bool,
) -> Result<(), Declined> {
    for (port, &y) in ports.iter().zip(ys) {
        canvas.ellipse(Layer::Front, (edge, y), (DOT, DOT), Some(skin.border), None);
        let label = measured(&port.name, skin.label, family, port.line)?;
        let (x, align) = if input {
            (edge + 8.0, Align::Left)
        } else {
            (edge - 8.0, Align::Right)
        };
        canvas.text(Layer::Front, &label, (x, y - label.height / 2.0), align);
    }
    Ok(())
}

/// Row spacing. One port keeps [`PITCH`]. Several names sit between the
/// wires, with [`NAME_LIFT`] under the name and [`PORT_CLEAR`] above it.
fn port_pitch(rows: usize, label_h: f32) -> f32 {
    if rows > 1 {
        (label_h + NAME_LIFT + PORT_CLEAR).max(PITCH)
    } else {
        PITCH
    }
}

fn centers_at(n: usize, rows: usize, top: f32, pitch: f32) -> Vec<f32> {
    let offset = (rows - n) as f32 * pitch / 2.0;
    (0..n)
        .map(|i| top + offset + (i as f32 + 0.5) * pitch)
        .collect()
}

/// One wire's polyline, and the gutter offset after it.
fn wire_points(
    scene: &Scene<'_>,
    wire: &Wire,
    index: usize,
    gutter_n: f32,
) -> Result<(Vec<(f32, f32)>, f32), Declined> {
    let from_i = node_index(scene.diagram, &wire.from);
    let to_i = node_index(scene.diagram, &wire.to);
    let from = pin(scene.diagram, scene.placed, &wire.from, true);
    let to = pin(scene.diagram, scene.placed, &wire.to, false);
    let bump = scene.diagram.wires[..index]
        .iter()
        .filter(|other| other.from == wire.from)
        .count() as f32
        * FORK;
    let lane = lane_of(&scene.diagram.wires, index, (from_i, to_i), |wire| {
        (
            node_index(scene.diagram, &wire.from),
            node_index(scene.diagram, &wire.to),
        )
    });
    let (points, gutter_n) = route(
        from,
        to,
        (from_i, to_i),
        scene.rects,
        bump,
        (lane.0, lane.1, scene.spines[index]),
        (scene.gutter, gutter_n),
    );
    Ok((simplify(points), gutter_n))
}

fn paint_path(
    canvas: &mut Canvas,
    skin: &Skin,
    family: Family,
    layer: Layer,
    points: &[(f32, f32)],
    label: Option<&str>,
    line: usize,
) -> Result<(), Declined> {
    if points.len() < 2 {
        return Ok(());
    }
    let head = shaft_and_head(points);
    canvas.polyline(layer, &head.shaft, skin.arrow);
    paint_head(canvas, skin, head.tip, head.dir, head.travel);
    if let Some(text) = label {
        let label = measured(text, skin.arrow_font, family, line)?;
        if let Some((a, b)) = longest_horizontal(&head.shaft) {
            let x = (a.0 + b.0) / 2.0;
            let y = a.1 - label.height - NAME_LIFT;
            canvas.text(Layer::Front, &label, (x, y), Align::Center);
        }
    }
    Ok(())
}

fn pin(diagram: &Diagram, placed: &[Placed], end: &End, source: bool) -> (f32, f32) {
    let index = node_index(diagram, end);
    let rect = placed[index].rect;
    match end {
        End::Boundary(_) => {
            let y = rect.y + rect.h / 2.0;
            let x = if source { rect.x + rect.w } else { rect.x };
            (x, y)
        }
        End::Port { block, port } => {
            let found = diagram
                .blocks
                .iter()
                .find(|b| b.name == block.as_str())
                .expect("a block end was declared");
            if let Some(inner) = &placed[index].inner {
                let (ports, dots) = if source {
                    (&found.outputs, &inner.output_at)
                } else {
                    (&found.inputs, &inner.input_at)
                };
                let at = ports
                    .iter()
                    .position(|p| p.name == port.as_str())
                    .expect("a port end was declared");
                return (rect.x + dots[at].0, rect.y + dots[at].1);
            }
            let name_h = placed[index].name_h.unwrap_or(0.0);
            let rows = found.inputs.len().max(found.outputs.len()).max(1);
            let band = rect.y + PAD_Y + name_h + NAME_GAP;
            let (ports, x) = if source {
                (&found.outputs, rect.x + rect.w)
            } else {
                (&found.inputs, rect.x)
            };
            let ys = centers_at(ports.len(), rows, band, port_pitch(rows, name_h));
            let at = ports
                .iter()
                .position(|p| p.name == port.as_str())
                .expect("a port end was declared");
            (x, ys[at])
        }
    }
}

/// `(this wire's lane, how many wires share the two nodes)`.
fn lane_of(
    wires: &[Wire],
    index: usize,
    ends: (usize, usize),
    pair: impl Fn(&Wire) -> (usize, usize),
) -> (usize, usize) {
    let mut lane = 0;
    let mut lanes = 0;
    for (at, wire) in wires.iter().enumerate() {
        if pair(wire) == ends {
            if at < index {
                lane += 1;
            }
            lanes += 1;
        }
    }
    (lane, lanes)
}

/// Pulls each child onto the ports it shares with an already placed child,
/// so a bundle of signals runs level. A child that would then cover another
/// in the same column stays where the rank put it.
fn align_children(block: &Block, children: &mut [ChildAt]) {
    if children.is_empty() {
        return;
    }
    let mut y: Vec<f32> = children.iter().map(|child| child.y).collect();
    let h: Vec<f32> = children.iter().map(|child| child.layout.height).collect();
    let x: Vec<f32> = children.iter().map(|child| child.x).collect();
    let w: Vec<f32> = children.iter().map(|child| child.layout.width).collect();
    let mut links = Vec::new();
    for wire in &block.wires {
        let End::Port {
            block: from_name,
            port: from_port,
        } = &wire.from
        else {
            continue;
        };
        let End::Port {
            block: to_name,
            port: to_port,
        } = &wire.to
        else {
            continue;
        };
        let Some(from) = block
            .blocks
            .iter()
            .position(|child| child.name == from_name.as_str())
        else {
            continue;
        };
        let Some(to) = block
            .blocks
            .iter()
            .position(|child| child.name == to_name.as_str())
        else {
            continue;
        };
        if from == to {
            continue;
        }
        let from_y = port_dot(
            &block.blocks[from],
            &children[from].layout,
            (0.0, 0.0),
            from_port,
            true,
        )
        .1;
        let to_y = port_dot(
            &block.blocks[to],
            &children[to].layout,
            (0.0, 0.0),
            to_port,
            false,
        )
        .1;
        links.push((from, from_y, to, to_y));
    }
    align_ports(&mut y, &h, &x, &w, &links, 0);
    for (child, y) in children.iter_mut().zip(y) {
        child.y = y;
    }
}

/// Puts a frame port on the inner port its wire meets, so that wire is level.
fn snap_frame_dots(
    block: &Block,
    children: &[ChildAt],
    input_at: &mut [(f32, f32)],
    output_at: &mut [(f32, f32)],
) {
    for wire in &block.wires {
        match (&wire.from, &wire.to) {
            (End::Boundary(name), End::Port { block: child, port }) => {
                if let Some(y) = child_pin_y(block, children, child, port, false) {
                    set_dot(&block.inputs, input_at, name, y);
                }
            }
            (End::Port { block: child, port }, End::Boundary(name)) => {
                if let Some(y) = child_pin_y(block, children, child, port, true) {
                    set_dot(&block.outputs, output_at, name, y);
                }
            }
            (End::Boundary(src), End::Boundary(dst)) => {
                if let Some(index) = block
                    .inputs
                    .iter()
                    .position(|port| port.name == src.as_str())
                {
                    set_dot(&block.outputs, output_at, dst, input_at[index].1);
                }
            }
            (End::Port { .. }, End::Port { .. }) => {}
        }
    }
}

fn child_pin_y(
    block: &Block,
    children: &[ChildAt],
    name: &str,
    port: &str,
    source: bool,
) -> Option<f32> {
    let index = block.blocks.iter().position(|child| child.name == name)?;
    let child = &children[index];
    Some(
        port_dot(
            &block.blocks[index],
            &child.layout,
            (child.x, child.y),
            port,
            source,
        )
        .1,
    )
}

fn set_dot(ports: &[Port], dots: &mut [(f32, f32)], name: &str, y: f32) {
    if let Some(index) = ports.iter().position(|port| port.name == name) {
        dots[index].1 = y;
    }
}

/// Same as [`align_children`], for the blocks and boundary signals of the
/// whole diagram. The first block stays; the others slide onto its ports.
fn align_placed(diagram: &Diagram, placed: &mut [Placed]) {
    if placed.is_empty() {
        return;
    }
    let mut y: Vec<f32> = placed.iter().map(|item| item.rect.y).collect();
    let h: Vec<f32> = placed.iter().map(|item| item.rect.h).collect();
    let x: Vec<f32> = placed.iter().map(|item| item.rect.x).collect();
    let w: Vec<f32> = placed.iter().map(|item| item.rect.w).collect();
    let mut links = Vec::new();
    for wire in &diagram.wires {
        let from = node_index(diagram, &wire.from);
        let to = node_index(diagram, &wire.to);
        if from == to {
            continue;
        }
        let from_y = pin(diagram, placed, &wire.from, true).1 - placed[from].rect.y;
        let to_y = pin(diagram, placed, &wire.to, false).1 - placed[to].rect.y;
        links.push((from, from_y, to, to_y));
    }
    let anchor = if diagram.blocks.is_empty() {
        0
    } else {
        diagram.inputs.len()
    };
    align_ports(&mut y, &h, &x, &w, &links, anchor);
    for (item, y) in placed.iter_mut().zip(y) {
        item.rect.y = y;
    }
}

/// `links` is `(from node, y in that node, to node, y in that node)`.
fn align_ports(
    y: &mut [f32],
    h: &[f32],
    x: &[f32],
    w: &[f32],
    links: &[(usize, f32, usize, f32)],
    anchor: usize,
) {
    let n = y.len();
    if n == 0 || anchor >= n {
        return;
    }
    let mut fixed = vec![false; n];
    fixed[anchor] = true;
    for _ in 0..n {
        let mut changed = false;
        for node in 0..n {
            if fixed[node] {
                continue;
            }
            let mut wants = Vec::new();
            for &(from, from_y, to, to_y) in links {
                if to == node && from != node && fixed[from] {
                    wants.push(y[from] + from_y - to_y);
                }
                if from == node && to != node && fixed[to] {
                    wants.push(y[to] + to_y - from_y);
                }
            }
            if wants.is_empty() || wants.iter().any(|want| (want - wants[0]).abs() > 0.5) {
                continue;
            }
            let new_y = wants[0];
            let bottom = new_y + h[node];
            // Same column means the boxes overlap horizontally. A narrower
            // block is centered, so its left edge is not the column's edge.
            let hit = (0..n).any(|other| {
                other != node
                    && x[node] < x[other] + w[other] + 1.0
                    && x[other] < x[node] + w[node] + 1.0
                    && new_y < y[other] + h[other] - 0.5
                    && bottom > y[other] + 0.5
            });
            if hit {
                continue;
            }
            y[node] = new_y;
            fixed[node] = true;
            changed = true;
        }
        if !changed {
            break;
        }
    }
}

/// Lines up the left edge of every block that shares a column. The rank
/// centers a narrower block, so its ports would otherwise sit a few pixels
/// off the rest of the column.
fn align_left_edges(placed: &mut [Placed]) {
    let mut x: Vec<f32> = placed.iter().map(|item| item.rect.x).collect();
    let w: Vec<f32> = placed.iter().map(|item| item.rect.w).collect();
    share_left_edge(&mut x, &w);
    for (item, x) in placed.iter_mut().zip(x) {
        item.rect.x = x;
    }
}

fn align_left_edges_at(children: &mut [ChildAt]) {
    let mut x: Vec<f32> = children.iter().map(|child| child.x).collect();
    let w: Vec<f32> = children.iter().map(|child| child.layout.width).collect();
    share_left_edge(&mut x, &w);
    for (child, x) in children.iter_mut().zip(x) {
        child.x = x;
    }
}

fn share_left_edge(x: &mut [f32], w: &[f32]) {
    let n = x.len();
    let mut used = vec![false; n];
    for start in 0..n {
        if used[start] {
            continue;
        }
        let mut group = vec![start];
        used[start] = true;
        let mut left = x[start];
        let mut right = x[start] + w[start];
        let mut grew = true;
        while grew {
            grew = false;
            for other in 0..n {
                if used[other] {
                    continue;
                }
                let other_left = x[other];
                let other_right = other_left + w[other];
                if other_left < right + 1.0 && left < other_right + 1.0 {
                    used[other] = true;
                    group.push(other);
                    left = left.min(other_left);
                    right = right.max(other_right);
                    grew = true;
                }
            }
        }
        for index in group {
            x[index] = left;
        }
    }
}

/// Bend x of each wire that shares its source with another forward wire.
/// `end_at` is `(from pin, to pin, from node)`.
fn fan_spines(
    count: usize,
    rects: &[graph::Rect],
    mut end_at: impl FnMut(usize) -> ((f32, f32), (f32, f32), usize),
) -> Vec<Option<f32>> {
    let ends: Vec<_> = (0..count).map(&mut end_at).collect();
    let mut spines = vec![None; count];
    let mut seen = vec![false; count];
    for start in 0..count {
        if seen[start] {
            continue;
        }
        let mut group = vec![start];
        seen[start] = true;
        for other in start + 1..count {
            // Grouping is by the caller: wires are visited in order and the
            // source pin identifies the output. Compared below via the pin.
            if (ends[other].0.0 - ends[start].0.0).abs() < 0.5
                && (ends[other].0.1 - ends[start].0.1).abs() < 0.5
            {
                group.push(other);
                seen[other] = true;
            }
        }
        let from = ends[start].0;
        let forward: Vec<(f32, f32)> = group
            .iter()
            .map(|&index| ends[index].1)
            .filter(|to| to.0 > from.0 + FORK)
            .collect();
        if forward.len() < 2 {
            continue;
        }
        let min_left = forward.iter().map(|to| to.0).fold(f32::MAX, f32::min);
        let gap_left = from.0 + 6.0;
        let gap_right = min_left - 6.0;
        if gap_right <= gap_left {
            continue;
        }
        let spine = (gap_left + gap_right) / 2.0;
        let top = forward.iter().map(|to| to.1).fold(from.1, f32::min);
        let bottom = forward.iter().map(|to| to.1).fold(from.1, f32::max);
        if vertical_hits(spine, top, bottom, rects, ends[start].2, ends[start].2) {
            continue;
        }
        for index in group {
            if ends[index].1.0 > from.0 + FORK {
                spines[index] = Some(spine);
            }
        }
    }
    spines
}

/// One dot where the branches leave the stem. Several wires share the corner.
fn paint_junction(canvas: &mut Canvas, skin: &Skin, at: (f32, f32), seen: &mut Vec<(f32, f32)>) {
    if seen
        .iter()
        .any(|prev| (prev.0 - at.0).abs() < 0.5 && (prev.1 - at.1).abs() < 0.5)
    {
        return;
    }
    seen.push(at);
    canvas.ellipse(Layer::Front, at, (DOT, DOT), Some(skin.arrow.color), None);
}

/// The corner where a fan-out leaves the stem, when this wire is one branch.
fn branch_point(spine: Option<f32>, points: &[(f32, f32)]) -> Option<(f32, f32)> {
    let x = spine?;
    if points.len() >= 3 && (points[1].0 - x).abs() < 0.5 && (points[1].1 - points[0].1).abs() < 0.5
    {
        Some((points[1].0, points[1].1))
    } else {
        None
    }
}

/// The polyline from `from` to `to`, and the gutter offset after it.
/// A forward wire bends in the gap. A wire that would cut another block,
/// or that runs backwards, uses the gutter above the diagram. A wire
/// between two ports of one block loops over that block.
fn route(
    from: (f32, f32),
    to: (f32, f32),
    (from_i, to_i): (usize, usize),
    rects: &[graph::Rect],
    bump: f32,
    (lane, lanes, spine): (usize, usize, Option<f32>),
    (gutter, gutter_n): (f32, f32),
) -> (Vec<(f32, f32)>, f32) {
    if from_i == to_i && from_i != BOUNDARY {
        let rect = rects[from_i];
        let right = rect.x + rect.w + FORK + bump;
        let left = rect.x - FORK - bump;
        let y = rect.y - 16.0 - bump;
        return (
            vec![
                from,
                (right, from.1),
                (right, y),
                (left, y),
                (left, to.1),
                to,
            ],
            gutter_n,
        );
    }
    if to.0 > from.0 + FORK {
        let gap_left = from.0 + 6.0;
        let gap_right = to.0 - 6.0;
        if gap_right > gap_left {
            // A fan-out shares one vertical, so every branch leaves one stem.
            // Several wires between the same two blocks take separate lanes.
            let mid = if let Some(spine) = spine {
                spine.clamp(gap_left, gap_right)
            } else if lanes <= 1 {
                ((from.0 + to.0) / 2.0 + bump).clamp(gap_left, gap_right)
            } else {
                let span = gap_right - gap_left;
                gap_left + span * (lane as f32 + 0.5) / lanes as f32
            };
            if !vertical_hits(mid, from.1, to.1, rects, from_i, to_i) {
                // A level wire is one segment. A midpoint would split it, and
                // the label would sit on the longer piece, against the source.
                if (from.1 - to.1).abs() < 0.5 {
                    return (vec![from, to], gutter_n);
                }
                return (vec![from, (mid, from.1), (mid, to.1), to], gutter_n);
            }
        }
    }
    let y = gutter - gutter_n - bump;
    (
        vec![
            from,
            (from.0 + 12.0, from.1),
            (from.0 + 12.0, y),
            (to.0 - 12.0, y),
            (to.0 - 12.0, to.1),
            to,
        ],
        gutter_n + FORK,
    )
}

fn vertical_hits(
    mid: f32,
    y0: f32,
    y1: f32,
    rects: &[graph::Rect],
    skip_a: usize,
    skip_b: usize,
) -> bool {
    let (top, bot) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    rects.iter().enumerate().any(|(index, rect)| {
        if index == skip_a || index == skip_b {
            return false;
        }
        let left = rect.x - BOX_PAD;
        let right = rect.x + rect.w + BOX_PAD;
        let rtop = rect.y - BOX_PAD;
        let rbot = rect.y + rect.h + BOX_PAD;
        mid > left && mid < right && bot > rtop && top < rbot
    })
}

fn simplify(points: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for point in points {
        if out.last().is_some_and(|prev: &(f32, f32)| {
            (prev.0 - point.0).abs() < 0.1 && (prev.1 - point.1).abs() < 0.1
        }) {
            continue;
        }
        out.push(point);
    }
    out
}

struct Horiz {
    y: f32,
    x0: f32,
    x1: f32,
}

/// A vertical that crosses a horizontal signal bulges to the right, so the
/// crossing is not a join. Collected from the straight wires: a jump is not
/// itself a signal something else should jump.
fn hop_paths(wires: &mut [WirePath]) {
    let mut points: Vec<Vec<(f32, f32)>> = wires.iter().map(|wire| wire.points.clone()).collect();
    hop_wires(&mut points);
    for (wire, points) in wires.iter_mut().zip(points) {
        wire.points = points;
    }
}

fn hop_wires(wires: &mut [Vec<(f32, f32)>]) {
    let horiz = horizontals_of(wires);
    for points in wires {
        *points = with_hops(points, &horiz);
    }
}

fn horizontals_of(wires: &[Vec<(f32, f32)>]) -> Vec<Horiz> {
    let mut found = Vec::new();
    for points in wires {
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if (a.1 - b.1).abs() >= 0.5 || (a.0 - b.0).abs() <= HOP * 2.0 {
                continue;
            }
            let (x0, x1) = if a.0 <= b.0 { (a.0, b.0) } else { (b.0, a.0) };
            found.push(Horiz { y: a.1, x0, x1 });
        }
    }
    found
}

/// Interior to both segments, by [`HOP`]. A corner, a fan-out tap and the
/// end of a level run share an endpoint, so none of them jumps.
fn with_hops(points: &[(f32, f32)], horiz: &[Horiz]) -> Vec<(f32, f32)> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len());
    out.push(points[0]);
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() > HOP * 2.0 {
            let x = a.0;
            let (top, bot) = if a.1 <= b.1 { (a.1, b.1) } else { (b.1, a.1) };
            let mut crossings = crossing_ys(x, top, bot, horiz);
            if a.1 > b.1 {
                crossings.reverse();
            }
            for y in crossings {
                for point in hop_arc(x, y, b.1 >= a.1) {
                    push_point(&mut out, point);
                }
            }
        }
        push_point(&mut out, b);
    }
    out
}

fn crossing_ys(x: f32, top: f32, bot: f32, horiz: &[Horiz]) -> Vec<f32> {
    let mut ys: Vec<f32> = horiz
        .iter()
        .filter(|run| {
            run.y > top + HOP && run.y < bot - HOP && x > run.x0 + HOP && x < run.x1 - HOP
        })
        .map(|run| run.y)
        .collect();
    ys.sort_by(|a, b| a.total_cmp(b));
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    let mut kept = Vec::new();
    for y in ys {
        if kept.last().is_some_and(|prev| y - prev < HOP * 2.0) {
            continue;
        }
        kept.push(y);
    }
    kept
}

/// Semicircle to the right. `downward` keeps the points in travel order.
fn hop_arc(x: f32, y: f32, downward: bool) -> Vec<(f32, f32)> {
    const STEPS: u32 = 4;
    let mut arc = Vec::with_capacity(STEPS as usize + 1);
    for step in 0..=STEPS {
        let theta =
            -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * (step as f32) / (STEPS as f32);
        arc.push((x + HOP * theta.cos(), y + HOP * theta.sin()));
    }
    if !downward {
        arc.reverse();
    }
    arc
}

fn push_point(out: &mut Vec<(f32, f32)>, point: (f32, f32)) {
    if out
        .last()
        .is_some_and(|prev| (prev.0 - point.0).abs() < 0.05 && (prev.1 - point.1).abs() < 0.05)
    {
        return;
    }
    out.push(point);
}

struct Head {
    shaft: Vec<(f32, f32)>,
    tip: (f32, f32),
    dir: (f32, f32),
    travel: f32,
}

/// The shaft, stopping where the arrow head starts, plus the tip, the
/// direction into the tip, and the head's length.
fn shaft_and_head(points: &[(f32, f32)]) -> Head {
    let tip = points[points.len() - 1];
    let mut prev_at = points.len() - 1;
    let (dir, len) = loop {
        if prev_at == 0 {
            break ((1.0, 0.0), 0.0);
        }
        let prev = points[prev_at - 1];
        let (dx, dy) = (tip.0 - prev.0, tip.1 - prev.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len > 0.5 {
            break ((dx / len, dy / len), len);
        }
        prev_at -= 1;
    };
    let travel = HEAD.min(len * 0.75);
    let base = (tip.0 - dir.0 * travel, tip.1 - dir.1 * travel);
    let mut shaft = points[..points.len() - 1].to_vec();
    shaft.push(base);
    Head {
        shaft,
        tip,
        dir,
        travel,
    }
}

fn paint_head(canvas: &mut Canvas, skin: &Skin, tip: (f32, f32), dir: (f32, f32), travel: f32) {
    if travel < 0.5 {
        return;
    }
    let base = (tip.0 - dir.0 * travel, tip.1 - dir.1 * travel);
    let side = (-dir.1 * HEAD_HALF, dir.0 * HEAD_HALF);
    let left = (base.0 + side.0, base.1 + side.1);
    let right = (base.0 - side.0, base.1 - side.1);
    if skin.strict {
        canvas.line(Layer::Front, left, tip, skin.arrow);
        canvas.line(Layer::Front, right, tip, skin.arrow);
    } else {
        canvas.polygon(
            Layer::Front,
            &[left, tip, right],
            Some(skin.arrow.color),
            None,
        );
    }
}

/// The longest horizontal run. Consecutive level pieces count as one, so a
/// kink that does not change `y` does not pull the label toward one end.
fn longest_horizontal(points: &[(f32, f32)]) -> Option<((f32, f32), (f32, f32))> {
    let mut best = None;
    let mut best_len = 0.0;
    let mut i = 0;
    while i + 1 < points.len() {
        if (points[i].1 - points[i + 1].1).abs() >= 0.5 {
            i += 1;
            continue;
        }
        let start = points[i];
        let mut end = points[i + 1];
        let mut j = i + 1;
        while j + 1 < points.len() && (points[j].1 - points[j + 1].1).abs() < 0.5 {
            end = points[j + 1];
            j += 1;
        }
        let len = (start.0 - end.0).abs();
        if len > best_len {
            best_len = len;
            best = Some((start, end));
        }
        i = j;
    }
    best
}

fn measured(text: &str, font: Font, family: Family, line: usize) -> Result<text::Block, Declined> {
    text::block(text, font, family).map_err(|reason| Declined::at(line, reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(body: &str) -> Result<String, Declined> {
        crate::render(&format!("@startflow\n{body}\n@endflow\n"))
    }

    #[test]
    fn a_chain_draws_the_blocks_the_ports_and_a_wire() {
        let svg = drawn(
            "title Charge control\n\
             block Limit {\n\
             in voltage\n\
             out limited\n\
             }\n\
             voltage -> Limit.voltage\n\
             Limit.limited -> current : limited\n",
        )
        .unwrap();
        for word in ["Charge control", "Limit", "voltage", "limited", "current"] {
            assert!(
                svg.contains(&format!(">{word}<")),
                "{word} missing in {svg}"
            );
        }
        assert!(svg.contains("<polyline"), "{svg}");
        assert!(svg.contains("<polygon"), "{svg}");
    }

    #[test]
    fn one_output_fans_out_to_four_blocks() {
        let svg = drawn(
            "block Source {\n\
             out cmd\n\
             }\n\
             block Alpha {\n\
             in cmd\n\
             }\n\
             block Beta {\n\
             in cmd\n\
             }\n\
             block Gamma {\n\
             in cmd\n\
             }\n\
             block Delta {\n\
             in cmd\n\
             }\n\
             Source.cmd -> Alpha.cmd\n\
             Source.cmd -> Beta.cmd\n\
             Source.cmd -> Gamma.cmd\n\
             Source.cmd -> Delta.cmd\n",
        )
        .unwrap();
        let wires = polylines(&svg);
        assert_eq!(wires.len(), 4, "{svg}");
        let mut spine = None;
        for wire in &wires {
            assert_eq!(wire.len(), 4, "a branch left the gap: {wire:?}");
            assert!(
                (wire[1].0 - wire[2].0).abs() < 0.5,
                "the branch is not vertical: {wire:?}"
            );
            match spine {
                None => spine = Some(wire[1].0),
                Some(x) => assert!((wire[1].0 - x).abs() < 0.5, "spines differ: {wires:?}"),
            }
        }
        let rects = svg_rects(&svg);
        assert_eq!(rects.len(), 5, "{svg}");
        for (i, a) in rects.iter().enumerate() {
            for b in rects.iter().skip(i + 1) {
                let apart = a.0 + a.2 <= b.0 + 0.5
                    || b.0 + b.2 <= a.0 + 0.5
                    || a.1 + a.3 <= b.1 + 0.5
                    || b.1 + b.3 <= a.1 + 0.5;
                assert!(apart, "blocks overlap: {a:?} {b:?}");
            }
        }
        let mut lefts: Vec<f32> = rects.iter().map(|rect| rect.0).collect();
        lefts.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // The source stands alone. The four targets share a left edge.
        assert!(
            (lefts[1] - lefts[4]).abs() < 0.5,
            "targets are not in one column: {lefts:?}"
        );
    }

    #[test]
    fn a_skip_hops_over_the_signals_it_crosses() {
        let body = "\
block Sampler {\n\
out a\n\
out b\n\
out tap\n\
}\n\
block Mid {\n\
in a\n\
in b\n\
out on\n\
}\n\
block Far {\n\
in tap\n\
in on\n\
}\n\
Sampler.a -> Mid.a\n\
Sampler.b -> Mid.b\n\
Mid.on -> Far.on\n\
Sampler.tap -> Far.tap\n";
        assert_skip_hops(&drawn(body).unwrap());
        assert_skip_hops(&drawn(&format!("block Plant {{\n{body}}}\n")).unwrap());
    }

    fn assert_skip_hops(svg: &str) {
        let wires = polylines(svg);
        assert_eq!(wires.len(), 4, "{svg}");
        let straight = wires.iter().filter(|wire| wire.len() == 2).count();
        assert!(
            straight >= 2,
            "the crossed signals gained a bend: {wires:?}"
        );
        assert!(
            wires.iter().any(|wire| bulges_right(wire)),
            "no wire jumps a crossing: {wires:?}"
        );
    }

    /// The outer points share the stub's x. The middle stands to the right.
    fn bulges_right(wire: &[(f32, f32)]) -> bool {
        for (start, a) in wire.iter().enumerate() {
            for (end_at, b) in wire.iter().enumerate().skip(start + 2) {
                if (a.0 - b.0).abs() >= 0.6 || (a.1 - b.1).abs() <= 2.0 {
                    continue;
                }
                if wire[start + 1..end_at].iter().any(|mid| mid.0 > a.0 + 2.0) {
                    return true;
                }
            }
        }
        false
    }

    #[test]
    fn a_corner_is_not_a_hop_and_one_crossing_jumps_once() {
        // The vertical ends on the spine. The trunk meets it, so it is a corner.
        let mut fan = vec![
            vec![(0.0, 10.0), (40.0, 10.0), (40.0, 30.0), (80.0, 30.0)],
            vec![(0.0, 10.0), (40.0, 10.0), (40.0, 50.0), (80.0, 50.0)],
        ];
        hop_wires(&mut fan);
        assert_eq!(fan[0].len(), 4, "{:?}", fan[0]);
        assert_eq!(fan[1].len(), 4, "{:?}", fan[1]);

        // Two copies of one signal, and a stub that crosses it.
        let mut skip = vec![
            vec![(0.0, 20.0), (40.0, 20.0)],
            vec![(0.0, 20.0), (40.0, 20.0)],
            vec![(0.0, 40.0), (12.0, 40.0), (12.0, 0.0), (80.0, 0.0)],
        ];
        hop_wires(&mut skip);
        assert_eq!(skip[0].len(), 2, "{:?}", skip[0]);
        assert_eq!(skip[2].len(), 9, "one jump is five points: {:?}", skip[2]);
        assert!(bulges_right(&skip[2]), "{:?}", skip[2]);
        assert!(
            skip[2]
                .iter()
                .any(|point| (point.0 - 16.0).abs() < 0.1 && (point.1 - 20.0).abs() < 0.1),
            "the jump does not clear the signal: {:?}",
            skip[2]
        );
    }

    #[test]
    fn a_fork_a_loop_and_a_way_back_draw() {
        drawn(
            "block A {\n\
             out x\n\
             }\n\
             block B {\n\
             in x\n\
             }\n\
             block C {\n\
             in x\n\
             }\n\
             A.x -> B.x\n\
             A.x -> C.x\n",
        )
        .unwrap();
        drawn(
            "block A {\n\
             in x\n\
             out y\n\
             }\n\
             A.y -> A.x\n",
        )
        .unwrap();
        drawn(
            "block A {\n\
             in x\n\
             out y\n\
             }\n\
             block B {\n\
             in y\n\
             out x\n\
             }\n\
             A.y -> B.y\n\
             B.x -> A.x\n",
        )
        .unwrap();
    }

    #[test]
    fn a_title_alone_draws_and_a_theme_pack_does_not_decline() {
        let svg = drawn("title Only\n").unwrap();
        assert!(svg.contains(">Only<"), "{svg}");
        let svg = drawn(
            "skinparam backgroundColor #121212\n\
             skinparam defaultFontColor white\n\
             skinparam RectangleBackgroundColor #222222\n\
             skinparam RectangleBorderColor #E68E0D\n\
             skinparam ParticipantBackgroundColor red\n\
             skinparam ArrowColor #E68E0D\n\
             block Limit {\n\
             in voltage\n\
             out limited\n\
             }\n\
             voltage -> Limit.voltage\n\
             Limit.limited -> limited\n",
        )
        .unwrap();
        assert!(svg.contains(">Limit<"), "{svg}");
        assert!(svg.contains("#121212"), "{svg}");
        assert!(svg.contains("#222222"), "{svg}");
        assert!(svg.contains("#E68E0D"), "{svg}");
        assert!(!svg.contains("#FF0000"), "{svg}");
    }

    #[test]
    fn a_level_wire_keeps_its_label_between_the_blocks() {
        let svg = drawn(
            "block Limit {\n\
             out a\n\
             }\n\
             block Measure {\n\
             in b\n\
             }\n\
             Limit.a -> Measure.b : named\n",
        )
        .unwrap();
        let mut boxes = Vec::new();
        let mut rest = svg.as_str();
        while let Some(at) = rest.find("<rect ") {
            rest = &rest[at..];
            boxes.push((attr(rest, "x"), attr(rest, "width")));
            rest = &rest["<rect ".len()..];
        }
        assert_eq!(boxes.len(), 2, "{svg}");
        let gap_left = boxes[0].0 + boxes[0].1;
        let gap_right = boxes[1].0;
        let at = svg.find(">named<").expect("label");
        let tspan = svg[..at].rfind("<tspan").expect("tspan");
        let piece = &svg[tspan..at];
        let x = attr(piece, "x");
        let width = attr(piece, "textLength");
        assert!(
            x > gap_left && x + width < gap_right,
            "label {x}..{} is outside {gap_left}..{gap_right}",
            x + width
        );
    }

    #[test]
    fn a_straight_run_is_one_span_for_the_label() {
        let span = longest_horizontal(&[(0.0, 10.0), (30.0, 10.0), (50.0, 10.0)]);
        assert_eq!(span, Some(((0.0, 10.0), (50.0, 10.0))));
    }

    #[test]
    fn a_wire_back_stays_below_the_title() {
        let svg = drawn(
            "title Back\n\
             block A {\n\
             in x\n\
             out y\n\
             }\n\
             block B {\n\
             in y\n\
             out x\n\
             }\n\
             A.y -> B.y\n\
             B.x -> A.x\n",
        )
        .unwrap();
        let title = svg.find(">Back<").expect("title");
        let title_y = attr(&svg[svg[..title].rfind("<tspan").unwrap()..title], "y");
        let wire_y = polyline_min_y(&svg);
        assert!(
            wire_y > title_y,
            "wire at {wire_y} crosses the title at {title_y}: {svg}"
        );
    }

    #[test]
    fn a_subsystem_draws_its_blocks_inside() {
        let svg = drawn(
            "block Charger {\n\
             in voltage\n\
             out current\n\
             block Limit {\n\
             in voltage\n\
             out limited\n\
             }\n\
             voltage -> Limit.voltage\n\
             Limit.limited -> current : limited\n\
             }\n\
             voltage -> Charger.voltage\n\
             Charger.current -> current\n",
        )
        .unwrap();
        for word in ["Charger", "Limit", "voltage", "limited", "current"] {
            assert!(
                svg.contains(&format!(">{word}<")),
                "{word} missing in {svg}"
            );
        }
        assert!(svg.matches("<rect ").count() >= 2, "{svg}");
    }

    #[test]
    fn frame_port_names_sit_between_the_wires() {
        let svg = drawn(
            "block Box {\n\
             in ready\n\
             in voltage\n\
             block Pass {\n\
             in ready\n\
             in voltage\n\
             out kept\n\
             }\n\
             ready -> Pass.ready\n\
             voltage -> Pass.voltage\n\
             Pass.kept -> kept\n\
             }\n",
        )
        .unwrap();
        let labels = port_labels(&svg, &["ready", "voltage"]);
        assert_eq!(labels.len(), 2, "{svg}");
        let wires = horizontal_wires(&svg);
        for label in &labels {
            for wire in &wires {
                let crosses = wire.y > label.top && wire.y < label.bottom;
                let over = wire.x0 < label.right && wire.x1 > label.left;
                assert!(
                    !(crosses && over),
                    "{} is crossed at {}: {svg}",
                    label.name,
                    wire.y
                );
            }
        }
        let mut tops: Vec<f32> = labels.iter().map(|l| l.top).collect();
        tops.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut bottoms: Vec<f32> = labels.iter().map(|l| l.bottom).collect();
        bottoms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            bottoms[0] < tops[1],
            "port names overlap: {bottoms:?} {tops:?}"
        );
    }

    #[test]
    fn parallel_signals_run_level_and_evenly() {
        let svg = drawn(
            "block Source {\n\
             out a\n\
             out b\n\
             out c\n\
             }\n\
             block Sink {\n\
             in a\n\
             in b\n\
             in c\n\
             }\n\
             Source.a -> Sink.a\n\
             Source.b -> Sink.b\n\
             Source.c -> Sink.c\n",
        )
        .unwrap();
        let wires = polylines(&svg);
        assert_eq!(wires.len(), 3, "{svg}");
        let mut ys = Vec::new();
        for wire in &wires {
            assert_eq!(wire.len(), 2, "a parallel signal steps: {wire:?}");
            assert!((wire[0].1 - wire[1].1).abs() < 0.5, "{wire:?}");
            ys.push(wire[0].1);
        }
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let gap = ys[1] - ys[0];
        assert!((ys[2] - ys[1] - gap).abs() < 0.5, "gaps {ys:?}");
        assert!(gap > 10.0, "gap {gap}");
    }

    #[test]
    fn a_frame_keeps_its_signals_level() {
        let svg = drawn(
            "block Box {\n\
             in a\n\
             in b\n\
             out c\n\
             out d\n\
             block Mid {\n\
             in a\n\
             in b\n\
             out c\n\
             out d\n\
             }\n\
             a -> Mid.a\n\
             b -> Mid.b\n\
             Mid.c -> c\n\
             Mid.d -> d\n\
             }\n\
             a -> Box.a\n\
             b -> Box.b\n\
             Box.c -> c\n\
             Box.d -> d\n",
        )
        .unwrap();
        let wires = polylines(&svg);
        assert_eq!(wires.len(), 8, "{svg}");
        for wire in &wires {
            assert_eq!(wire.len(), 2, "a frame signal steps: {wire:?} in {svg}");
            assert!((wire[0].1 - wire[1].1).abs() < 0.5, "not level: {wire:?}");
        }
    }

    #[test]
    fn boundary_signals_share_one_length() {
        let svg = drawn(
            "in v_bus\n\
             in run\n\
             out fault\n\
             out temp_o\n\
             block Sampler {\n\
             in v_bus\n\
             in run\n\
             out fault\n\
             out temp_o\n\
             }\n\
             v_bus -> Sampler.v_bus\n\
             run -> Sampler.run\n\
             Sampler.fault -> fault\n\
             Sampler.temp_o -> temp_o\n",
        )
        .unwrap();
        let mut wires = polylines(&svg);
        assert_eq!(wires.len(), 4, "{svg}");
        for wire in &wires {
            assert_eq!(wire.len(), 2, "{wire:?}");
            assert!((wire[0].1 - wire[1].1).abs() < 0.5, "{wire:?}");
        }
        wires.sort_by(|a, b| a[0].0.partial_cmp(&b[0].0).unwrap());
        let incoming = &wires[..2];
        let outgoing = &wires[2..];
        assert!(
            (incoming[0][0].0 - incoming[1][0].0).abs() < 0.5,
            "input dots differ: {incoming:?}"
        );
        let in_len = (incoming[0][1].0 - incoming[0][0].0).abs();
        assert!(
            (in_len - (incoming[1][1].0 - incoming[1][0].0).abs()).abs() < 0.5,
            "input lengths differ: {incoming:?}"
        );
        assert!(
            (outgoing[0][1].0 - outgoing[1][1].0).abs() < 0.5,
            "output dots differ: {outgoing:?}"
        );
        let out_len = (outgoing[0][1].0 - outgoing[0][0].0).abs();
        assert!(
            (out_len - (outgoing[1][1].0 - outgoing[1][0].0).abs()).abs() < 0.5,
            "output lengths differ: {outgoing:?}"
        );
    }

    #[test]
    fn a_frame_name_clears_the_wire_above_and_below() {
        let svg = drawn(
            "block Box {\n\
             in v_y\n\
             in q_g\n\
             block Mid {\n\
             in v_y\n\
             in q_g\n\
             }\n\
             v_y -> Mid.v_y\n\
             q_g -> Mid.q_g\n\
             }\n",
        )
        .unwrap();
        let mut labels = port_labels(&svg, &["v_y", "q_g"]);
        labels.sort_by(|a, b| a.top.partial_cmp(&b.top).unwrap());
        let wires = horizontal_wires(&svg);
        for label in &labels {
            let under = wires
                .iter()
                .filter(|wire| overlaps(label, wire) && wire.y + 0.1 >= label.bottom)
                .map(|wire| wire.y)
                .fold(f32::INFINITY, f32::min);
            assert!(
                under - label.bottom >= NAME_LIFT - 0.6,
                "{} sits on its wire: bottom {} wire {under}",
                label.name,
                label.bottom
            );
        }
        let lower = &labels[1];
        let over = wires
            .iter()
            .filter(|wire| overlaps(lower, wire) && wire.y <= lower.top + 0.1)
            .map(|wire| wire.y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            lower.top - over >= PORT_CLEAR - 0.6,
            "the lower name meets the wire above it: top {} wire {over}",
            lower.top
        );
    }

    struct PortLabel {
        name: &'static str,
        left: f32,
        right: f32,
        top: f32,
        bottom: f32,
    }

    struct HWire {
        x0: f32,
        x1: f32,
        y: f32,
    }

    fn overlaps(label: &PortLabel, wire: &HWire) -> bool {
        wire.x0 < label.right && wire.x1 > label.left
    }

    /// The leftmost label of each name: the frame's, not the inner block's.
    fn port_labels(svg: &str, names: &[&'static str]) -> Vec<PortLabel> {
        let mut found = Vec::new();
        for name in names {
            let needle = format!(">{name}<");
            let mut rest = svg;
            let mut best: Option<PortLabel> = None;
            while let Some(at) = rest.find(&needle) {
                let head = &rest[..at];
                let tspan = head.rfind("<tspan").expect("tspan");
                let piece = &head[tspan..];
                let left = attr(piece, "x");
                let size = attr(piece, "font-size");
                let baseline = attr(piece, "y");
                let width = attr(piece, "textLength");
                // The same shares `text::block` uses for the logical font.
                let top = baseline - size * 0.928;
                let label = PortLabel {
                    name,
                    left,
                    right: left + width,
                    top,
                    bottom: top + size * 1.164,
                };
                best = Some(match best {
                    Some(prev) if prev.left <= label.left => prev,
                    _ => label,
                });
                rest = &rest[at + needle.len()..];
            }
            found.push(best.expect("label"));
        }
        found
    }

    fn polylines(svg: &str) -> Vec<Vec<(f32, f32)>> {
        let mut wires = Vec::new();
        let mut rest = svg;
        while let Some(at) = rest.find("<polyline ") {
            rest = &rest[at..];
            let at = rest.find("points=\"").expect("polyline points");
            rest = &rest[at + "points=\"".len()..];
            let body = rest.split('"').next().unwrap();
            wires.push(
                body.split_whitespace()
                    .filter_map(|pair| {
                        let (x, y) = pair.split_once(',')?;
                        Some((x.parse().ok()?, y.parse().ok()?))
                    })
                    .collect(),
            );
        }
        wires
    }

    fn horizontal_wires(svg: &str) -> Vec<HWire> {
        let mut wires = Vec::new();
        let mut rest = svg;
        while let Some(at) = rest.find("points=\"") {
            rest = &rest[at + "points=\"".len()..];
            let body = rest.split('"').next().unwrap();
            let points: Vec<(f32, f32)> = body
                .split_whitespace()
                .filter_map(|pair| {
                    let (x, y) = pair.split_once(',')?;
                    Some((x.parse().ok()?, y.parse().ok()?))
                })
                .collect();
            for pair in points.windows(2) {
                if (pair[0].1 - pair[1].1).abs() < 0.2 {
                    wires.push(HWire {
                        x0: pair[0].0.min(pair[1].0),
                        x1: pair[0].0.max(pair[1].0),
                        y: pair[0].1,
                    });
                }
            }
        }
        wires
    }

    #[test]
    fn a_pass_through_and_a_nested_subsystem_draw() {
        drawn(
            "block Pass {\n\
             in a\n\
             out b\n\
             a -> b\n\
             }\n",
        )
        .unwrap();
        let svg = drawn(
            "block Outer {\n\
             block Inner {\n\
             block Leaf {\n\
             out x\n\
             }\n\
             Leaf.x -> x\n\
             }\n\
             }\n",
        )
        .unwrap();
        for word in ["Outer", "Inner", "Leaf"] {
            assert!(
                svg.contains(&format!(">{word}<")),
                "{word} missing in {svg}"
            );
        }
    }

    fn polyline_min_y(svg: &str) -> f32 {
        let mut min = f32::INFINITY;
        let mut rest = svg;
        while let Some(at) = rest.find("points=\"") {
            rest = &rest[at + "points=\"".len()..];
            let body = rest.split('"').next().unwrap();
            for pair in body.split_whitespace() {
                if let Some((_, y)) = pair.split_once(',')
                    && let Ok(y) = y.parse::<f32>()
                {
                    min = min.min(y);
                }
            }
        }
        min
    }

    fn svg_rects(svg: &str) -> Vec<(f32, f32, f32, f32)> {
        let mut rects = Vec::new();
        let mut rest = svg;
        while let Some(at) = rest.find("<rect ") {
            rest = &rest[at..];
            let end = rest.find('>').unwrap();
            let tag = &rest[..end];
            rects.push((
                attr(tag, "x"),
                attr(tag, "y"),
                attr(tag, "width"),
                attr(tag, "height"),
            ));
            rest = &rest[end..];
        }
        rects
    }

    fn attr(svg: &str, key: &str) -> f32 {
        let key = format!("{key}=\"");
        let at = svg.find(&key).unwrap_or_else(|| panic!("{key} in {svg}"));
        svg[at + key.len()..]
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    }
}
