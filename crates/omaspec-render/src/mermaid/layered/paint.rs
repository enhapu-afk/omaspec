//! Frames, boxes, lines and labels.

use omaspec_uml::map::{Head, HeadFill};
use omaspec_uml::{Align, Block, Canvas, Family, Font, Layer, Paint, Stroke};

use super::shape::{RING, SUBROUTINE, Shape, cap};
use super::{COMP_PAD, Cap, HEAD_HALF, HEAD_LENGTH, Kind, Palette, Placed, Scene};

/// The corners of a subgraph's frame and of a flowchart's `(rounded)` node.
const CLUSTER_RADIUS: f32 = 4.0;
const ROUNDED: f32 = 6.0;
/// The circle at a line's `--o` end.
const DOT: f32 = 4.0;
/// From a line's `--x` end to the middle of its cross, and half an arm.
const CROSS_AT: f32 = 6.0;
const CROSS_ARM: f32 = 4.0;
/// Around an edge label, on its background.
const LABEL_PAD: f32 = 2.0;

/// Between the title and the diagram below it, as the state diagrams keep.
const TITLE_GAP: f32 = 10.0;

/// The front matter's `title`, centered above what `canvas` holds, in bold
/// as the state diagrams draw theirs, as large as theirs.
pub(super) fn title(canvas: &mut Canvas, palette: &Palette, title: &str, family: Family) {
    let font = Font::new(palette.size * crate::mermaid::TITLE_SCALE, palette.text).bold();
    let block = super::label::measure(title, font, family, None);
    let (left, right) = canvas.width_span();
    let at = (
        (left + right - block.width) / 2.0,
        canvas.top() - TITLE_GAP - block.height,
    );
    // A part of its own, as a PlantUML title is (docs/adr/0029): it
    // glides from where it was to where it is.
    canvas.part(omaspec_uml::TITLE_PART, |canvas| {
        canvas.text(Layer::Front, &block, at, Align::Left);
    });
}

/// Draws `scene`; the lines' corners curve by up to `curve`.
pub(super) fn paint(
    canvas: &mut Canvas,
    palette: &Palette,
    scene: &Scene,
    (curve, names): (f32, &super::Names),
) {
    paint_shapes(canvas, scene, (&mut Vec::new(), names));
    paint_edges(canvas, scene, (curve, names));
    paint_labels(
        canvas,
        scene,
        Some(palette.background),
        (&mut Vec::new(), names),
    );
    paint_caps(canvas, scene, names);
}

/// Each node's and subgraph's shape, in its part (docs/adr/0029); `path` is
/// where `scene` is in the tree.
fn paint_shapes(
    canvas: &mut Canvas,
    scene: &Scene,
    (path, names): (&mut Vec<usize>, &super::Names),
) {
    for (index, node) in scene.nodes.iter().enumerate() {
        path.push(index);
        canvas.part(&names.node(path), |canvas| {
            paint_shape(canvas, node);
        });
        if node.item.nested {
            paint_shapes(canvas, &node.children, (path, names));
        }
        path.pop();
    }
}

fn paint_shape(canvas: &mut Canvas, node: &Placed) {
    canvas.named(&node.item.name, |canvas| {
        canvas.linked(node.item.link.as_ref(), &[Layer::Frames], |canvas| {
            paint_outline(canvas, node);
        });
    });
}

fn paint_outline(canvas: &mut Canvas, node: &Placed) {
    let rect = node.rect;
    let corners = |radius: f32| radius.min(rect.w / 2.0).min(rect.h / 2.0);
    let border = Some(node.item.border);
    match node.item.kind {
        Kind::Cluster => {
            let bounds = (rect.x, rect.y, rect.w, rect.h);
            canvas.rect(
                Layer::Frames,
                bounds,
                corners(CLUSTER_RADIUS),
                node.item.fill,
                border,
            );
        }
        Kind::Node(shape) => paint_node(canvas, node, shape),
    }
}

fn paint_node(canvas: &mut Canvas, node: &Placed, shape: Shape) {
    let rect = node.rect;
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    let border = Some(node.item.border);
    let layer = Layer::Frames;
    match shape {
        Shape::Label => {}
        Shape::Form(form) => form.paint(canvas, rect, node.item.fill, node.item.border),
        Shape::Rect => canvas.rect(layer, (x, y, w, h), 0.0, node.item.fill, border),
        Shape::Rounded => {
            let radius = ROUNDED.min(w / 2.0).min(h / 2.0);
            canvas.rect(layer, (x, y, w, h), radius, node.item.fill, border);
        }
        Shape::Stadium => canvas.rect(layer, (x, y, w, h), h.min(w) / 2.0, node.item.fill, border),
        Shape::Subroutine => {
            canvas.rect(layer, (x, y, w, h), 0.0, node.item.fill, border);
            for side in [x + SUBROUTINE, x + w - SUBROUTINE] {
                canvas.line(layer, (side, y), (side, y + h), node.item.border);
            }
        }
        Shape::Cylinder => {
            let (rx, ry) = (w / 2.0, cap(w));
            let body = format!(
                "M{},{} A{},{} 0 0 1 {},{} L{},{} A{},{} 0 0 1 {},{} Z",
                num(x),
                num(y + ry),
                num(rx),
                num(ry),
                num(x + w),
                num(y + ry),
                num(x + w),
                num(y + h - ry),
                num(rx),
                num(ry),
                num(x),
                num(y + h - ry),
            );
            let bounds = (x, y, x + w, y + h);
            canvas.path(layer, &body, bounds, node.item.fill, border);
            // The near rim of the top.
            let rim = format!(
                "M{},{} A{},{} 0 0 0 {},{}",
                num(x),
                num(y + ry),
                num(rx),
                num(ry),
                num(x + w),
                num(y + ry),
            );
            canvas.path(layer, &rim, bounds, None, border);
        }
        Shape::Circle => {
            let center = (x + w / 2.0, y + h / 2.0);
            canvas.ellipse(layer, center, (w / 2.0, h / 2.0), node.item.fill, border);
        }
        Shape::DoubleCircle => {
            let center = (x + w / 2.0, y + h / 2.0);
            canvas.ellipse(layer, center, (w / 2.0, h / 2.0), node.item.fill, border);
            let inner = (w / 2.0 - RING, h / 2.0 - RING);
            canvas.ellipse(layer, center, inner, None, border);
        }
        Shape::Diamond
        | Shape::Hexagon
        | Shape::LeanRight
        | Shape::LeanLeft
        | Shape::Trapezoid
        | Shape::TrapezoidAlt
        | Shape::Flag => {
            canvas.polygon(layer, &shape.outline(rect), node.item.fill, border);
        }
    }
}

/// A coordinate in a path, as short as it can be.
fn num(value: f32) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    format!("{rounded}")
}

/// Each line in its part (docs/adr/0037), which its words and its caps
/// are drawn in too.
fn paint_edges(canvas: &mut Canvas, scene: &Scene, (curve, names): (f32, &super::Names)) {
    for edge in &scene.edges {
        if edge.points.len() < 2 {
            continue;
        }
        canvas.part(&names.edge(edge.look.edge), |canvas| {
            canvas.curve(
                Layer::Lines,
                &shortened(&edge.points, edge.look.caps),
                curve,
                edge.look.stroke,
            );
        });
    }
    for node in &scene.nodes {
        paint_edges(canvas, &node.children, (curve, names));
    }
}

/// Each node's and subgraph's words in its part, each line's in a part of
/// their own, by its edge; `ground` is the fill the scene sits on, `path`
/// where it is in the tree.
fn paint_labels(
    canvas: &mut Canvas,
    scene: &Scene,
    ground: Paint,
    (path, names): (&mut Vec<usize>, &super::Names),
) {
    for (index, node) in scene.nodes.iter().enumerate() {
        path.push(index);
        canvas.part(&names.node(path), |canvas| {
            node_words(canvas, node);
        });
        let inside = if node.item.nested {
            node.item.fill.or(ground)
        } else {
            ground
        };
        paint_labels(canvas, &node.children, inside, (path, names));
        path.pop();
    }
    for edge in &scene.edges {
        if edge.label.is_empty() {
            continue;
        }
        canvas.part(&names.edge(edge.look.edge), |canvas| {
            edge_words(canvas, scene, edge, ground);
        });
    }
}

/// A node's words, or a subgraph's title.
fn node_words(canvas: &mut Canvas, node: &Placed) {
    let rect = node.rect;
    match node.item.kind {
        Kind::Cluster if node.item.nested => {
            paint_stack(
                canvas,
                &node.item.title,
                &node.item.description,
                rect.x + rect.w / 2.0,
                rect.y + COMP_PAD,
            );
        }
        Kind::Node(shape) if !shape.shows_text() => {}
        Kind::Node(shape) => {
            let (cx, cy) = shape.text_center(rect);
            let top = cy - node.item.title.height / 2.0;
            canvas.linked(node.item.link.as_ref(), &[Layer::Front], |canvas| {
                canvas.text(Layer::Front, &node.item.title, (cx, top), Align::Center);
            });
        }
        Kind::Cluster => {}
    }
}

/// A line's words, on the ground of the frame they sit in.
fn edge_words(canvas: &mut Canvas, scene: &Scene, edge: &super::PlacedEdge, ground: Paint) {
    // A line between containers lies in the one that holds both, but
    // its label may sit in a subgraph inside it.
    let (x, y) = edge.label_at;
    let center = (x + edge.label.width / 2.0, y + edge.label.height / 2.0);
    let ground = ground_at(scene, center, ground);
    if ground.is_some() {
        let bounds = (
            x - LABEL_PAD,
            y - LABEL_PAD,
            edge.label.width + 2.0 * LABEL_PAD,
            edge.label.height + 2.0 * LABEL_PAD,
        );
        // The ground keeps its form, so it still covers the line under the words.
        canvas.plain(|canvas| {
            canvas.rect(Layer::Front, bounds, 2.0, ground, None);
        });
    }
    canvas.text(Layer::Front, &edge.label, edge.label_at, Align::Left);
}

/// The fill at `point`: of the innermost frame around it, else `ground`.
fn ground_at(scene: &Scene, point: (f32, f32), ground: Paint) -> Paint {
    for node in &scene.nodes {
        let r = node.rect;
        let inside = point.0 > r.x && point.0 < r.x + r.w && point.1 > r.y && point.1 < r.y + r.h;
        if node.item.nested && inside {
            return ground_at(&node.children, point, node.item.fill.or(ground));
        }
    }
    ground
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

/// Each line's caps, in its part.
fn paint_caps(canvas: &mut Canvas, scene: &Scene, names: &super::Names) {
    for edge in &scene.edges {
        let (start, end) = edge.look.caps;
        canvas.part(&names.edge(edge.look.edge), |canvas| {
            paint_cap(canvas, &edge.points, false, end, edge.look.stroke);
            paint_cap(canvas, &edge.points, true, start, edge.look.stroke);
        });
    }
    for node in &scene.nodes {
        paint_caps(canvas, &node.children, names);
    }
}

/// How much of the line a cap takes at its end.
fn cap_length(cap: Cap) -> f32 {
    match cap {
        Cap::None => 0.0,
        Cap::Arrow => HEAD_LENGTH,
        Cap::Circle => 2.0 * DOT,
        Cap::Cross => CROSS_AT,
    }
}

pub(super) fn shortened(points: &[(f32, f32)], (start, end): (Cap, Cap)) -> Vec<(f32, f32)> {
    let mut line = points.to_vec();
    retract(&mut line, false, cap_length(end));
    retract(&mut line, true, cap_length(start));
    line
}

fn retract(line: &mut [(f32, f32)], start: bool, by: f32) {
    if line.len() < 2 || by <= 0.0 {
        return;
    }
    let (tip_i, from_i) = if start {
        (0, 1)
    } else {
        (line.len() - 1, line.len() - 2)
    };
    let tip = line[tip_i];
    let from = line[from_i];
    let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (vx * vx + vy * vy).sqrt();
    if len > by + 0.5 {
        let scale = by / len;
        line[tip_i] = (tip.0 - vx * scale, tip.1 - vy * scale);
    }
}

fn paint_cap(canvas: &mut Canvas, points: &[(f32, f32)], start: bool, cap: Cap, stroke: Stroke) {
    let solid = Stroke {
        dash: None,
        width: stroke.width.min(1.5),
        ..stroke
    };
    if cap == Cap::Circle && points.len() >= 2 {
        // A true circle in the picture; the map has it as a polygon.
        let (tip, from) = if start {
            (points[0], points[1])
        } else {
            (points[points.len() - 1], points[points.len() - 2])
        };
        let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
        let len = (vx * vx + vy * vy).sqrt();
        if len < 0.01 {
            return;
        }
        let center = (tip.0 - vx / len * DOT, tip.1 - vy / len * DOT);
        canvas.ellipse(
            Layer::Lines,
            center,
            (DOT, DOT),
            Some(stroke.color),
            Some(solid),
        );
        return;
    }
    for head in cap_shape(points, start, cap) {
        if head.closed {
            canvas.polygon(Layer::Lines, &head.points, Some(stroke.color), Some(solid));
        } else {
            let arm = Stroke {
                width: solid.width.max(1.5),
                ..solid
            };
            canvas.polyline(Layer::Lines, &head.points, arm);
        }
    }
}

/// The mark `cap` at the `start` or the end of `points`, as the picture
/// draws it: an arrowhead, a dot as a polygon, a cross as two strokes.
pub(super) fn cap_shape(points: &[(f32, f32)], start: bool, cap: Cap) -> Vec<Head> {
    if points.len() < 2 || cap == Cap::None {
        return Vec::new();
    }
    let (tip, from) = if start {
        (points[0], points[1])
    } else {
        (points[points.len() - 1], points[points.len() - 2])
    };
    let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
    let len = (vx * vx + vy * vy).sqrt();
    if len < 0.01 {
        return Vec::new();
    }
    let (ux, uy) = (vx / len, vy / len);
    match cap {
        Cap::None => Vec::new(),
        Cap::Arrow => {
            let back = (tip.0 - ux * HEAD_LENGTH, tip.1 - uy * HEAD_LENGTH);
            let side = (-uy * HEAD_HALF, ux * HEAD_HALF);
            vec![Head {
                points: vec![
                    (back.0 + side.0, back.1 + side.1),
                    tip,
                    (back.0 - side.0, back.1 - side.1),
                    (tip.0 - ux * 6.0, tip.1 - uy * 6.0),
                ],
                closed: true,
                fill: HeadFill::Ink,
            }]
        }
        Cap::Circle => {
            let center = (tip.0 - ux * DOT, tip.1 - uy * DOT);
            let points = (0..16)
                .map(|i| {
                    let angle = std::f32::consts::TAU * i as f32 / 16.0;
                    (center.0 + DOT * angle.cos(), center.1 + DOT * angle.sin())
                })
                .collect();
            vec![Head {
                points,
                closed: true,
                fill: HeadFill::Ink,
            }]
        }
        Cap::Cross => {
            let center = (tip.0 - ux * CROSS_AT, tip.1 - uy * CROSS_AT);
            [(1.0, 1.0), (1.0, -1.0)]
                .into_iter()
                .map(|(dx, dy)| {
                    let (ax, ay) = (dx * CROSS_ARM, dy * CROSS_ARM);
                    Head {
                        points: vec![
                            (center.0 - ax, center.1 - ay),
                            (center.0 + ax, center.1 + ay),
                        ],
                        closed: false,
                        fill: HeadFill::None,
                    }
                })
                .collect()
        }
    }
}
