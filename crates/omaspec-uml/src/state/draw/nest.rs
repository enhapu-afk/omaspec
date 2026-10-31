//! Concurrent regions, and transitions between containers.

use super::super::{Container, Crossing, Kind, Split};
use super::{
    COMP_PAD, Placed, PlacedEdge, Scene, Skin, arrow_stroke, drawn_route, edge_label,
    layout_container,
};
use crate::Declined;
use crate::graph::Rect;
use crate::graph::across::{self, Drawn, Link, Placement};
use crate::graph::by_hand::{self, Arrow, Laid, LineMut};
use crate::hand::{self, Key};
use crate::map::Wire;
use crate::svg::Stroke;
use crate::text::{Block, Family};

/// Between a concurrent region's states and the dashed line.
const REGION_GAP: f32 = 14.0;

/// The regions of a split state, each laid out on its own and stacked
/// along the split, in a row as wide as the widest (or as high as the
/// highest, side by side).
pub(super) fn layout_regions(
    container: &Container,
    split: Split,
    skin: &Skin,
    family: Family,
) -> Result<Scene, Declined> {
    let mut scenes = Vec::with_capacity(container.nodes.len());
    for node in &container.nodes {
        scenes.push(layout_container(&node.children, skin, family)?);
    }
    let across = scenes
        .iter()
        .map(|scene| match split {
            Split::Stacked => scene.width,
            Split::Beside => scene.height,
        })
        .fold(0.0, f32::max);
    let last = scenes.len().saturating_sub(1);
    let mut nodes = Vec::with_capacity(scenes.len());
    let mut cursor = 0.0;
    for (index, mut scene) in scenes.into_iter().enumerate() {
        let before = if index == 0 { 0.0 } else { REGION_GAP };
        let after = if index == last { 0.0 } else { REGION_GAP };
        let (along, rect) = match split {
            Split::Stacked => {
                let along = scene.height;
                scene.offset((across - scene.width) / 2.0, cursor + before);
                let rect = Rect {
                    x: 0.0,
                    y: cursor,
                    w: across,
                    h: before + along + after,
                };
                (along, rect)
            }
            Split::Beside => {
                let along = scene.width;
                scene.offset(cursor + before, (across - scene.height) / 2.0);
                let rect = Rect {
                    x: cursor,
                    y: 0.0,
                    w: before + along + after,
                    h: across,
                };
                (along, rect)
            }
        };
        cursor += before + along + after;
        nodes.push(Placed {
            rect,
            kind: Kind::Region,
            fill: None,
            border: Stroke::new(skin.state.border, skin.state.thickness),
            mark: skin.state.border,
            title: Block::empty(),
            description: Vec::new(),
            header_h: 0.0,
            composite: true,
            opened: false,
            children: scene,
        });
    }
    let (width, height) = match split {
        Split::Stacked => (across, cursor),
        Split::Beside => (cursor, across),
    };
    // Transitions between the regions are routed once everything is
    // placed, each in its place after the container's own.
    let mut edges = Vec::with_capacity(container.edges.len() + container.crossings.len());
    for arrow in container
        .edges
        .iter()
        .chain(container.crossings.iter().map(|crossing| &crossing.arrow))
    {
        edges.push(PlacedEdge {
            points: Vec::new(),
            label: Block::empty(),
            label_at: (0.0, 0.0),
            stroke: arrow_stroke(skin, arrow)?,
        });
    }
    Ok(Scene {
        nodes,
        edges,
        notes: Vec::new(),
        width,
        height,
    })
}

/// Stretches the regions of a split state to its border and up to its
/// title, so each owns its whole part of the box.
pub(super) fn fill_regions(children: &mut Scene, rect: Rect, top: f32) {
    let regions = &mut children.nodes;
    if regions.first().is_none_or(|node| node.kind != Kind::Region) {
        return;
    }
    let stacked =
        regions.len() < 2 || regions[1].rect.y >= regions[0].rect.y + regions[0].rect.h - 0.5;
    let last = regions.len() - 1;
    for (index, region) in regions.iter_mut().enumerate() {
        let r = &mut region.rect;
        let (start, end) = if stacked {
            r.x = rect.x;
            r.w = rect.w;
            (r.y, r.y + r.h)
        } else {
            r.y = top;
            r.h = rect.y + rect.h - top;
            (r.x, r.x + r.w)
        };
        let start = if index == 0 {
            if stacked { top } else { rect.x }
        } else {
            start
        };
        let end = if index == last {
            if stacked {
                rect.y + rect.h
            } else {
                rect.x + rect.w
            }
        } else {
            end
        };
        if stacked {
            r.y = start;
            r.h = end - start;
        } else {
            r.x = start;
            r.w = end - start;
        }
    }
}

/// Routes every crossing the layouts left once the diagram is placed, in
/// its place among the edges of the container that holds both its ends.
pub(super) fn link_crossings(
    root: &Container,
    scene: &mut Scene,
    skin: &Skin,
    family: Family,
) -> Result<(), Declined> {
    let mut found = Vec::new();
    gather(root, &mut Vec::new(), &mut found);
    // The transitions laid out with the containers around them are drawn
    // already.
    found.retain(|(path, slot, _)| {
        scene_at(scene, path)
            .edges
            .get(*slot)
            .is_some_and(|edge| edge.points.len() < 2)
    });
    if found.is_empty() {
        return Ok(());
    }
    let mut labels = Vec::with_capacity(found.len());
    let mut links = Vec::with_capacity(found.len());
    for (path, _, crossing) in &found {
        let label = edge_label(&crossing.arrow, skin, family)?;
        links.push(Link {
            from: [path.as_slice(), &crossing.from].concat(),
            to: [path.as_slice(), &crossing.to].concat(),
            label: (label.width, label.height),
            shape: None,
        });
        labels.push(label);
    }
    let top = placements(&scene.nodes);
    let mut drawn = Drawn::default();
    drawn_in(scene, &mut drawn);
    let routed = across::link(&top, &links, &drawn);
    for (((path, slot, crossing), label), linked) in found.iter().zip(labels).zip(routed) {
        let Some(linked) = linked else {
            return Err(Declined::at(
                crossing.arrow.line,
                "no way was found for this transition between the states",
            ));
        };
        let stroke = arrow_stroke(skin, &crossing.arrow)?;
        if let Some(edge) = scene_at(scene, path).edges.get_mut(*slot) {
            *edge = PlacedEdge {
                points: linked.points,
                label,
                label_at: linked.label_at,
                stroke,
            };
        }
    }
    Ok(())
}

/// Every crossing, with the path to the container that holds it and its
/// place among that container's edges, which come first.
fn gather<'a>(
    container: &'a Container,
    path: &mut Vec<usize>,
    out: &mut Vec<(Vec<usize>, usize, &'a Crossing)>,
) {
    for (index, crossing) in container.crossings.iter().enumerate() {
        out.push((path.clone(), container.edges.len() + index, crossing));
    }
    for (index, node) in container.nodes.iter().enumerate() {
        path.push(index);
        gather(&node.children, path, out);
        path.pop();
    }
}

/// Every transition of the diagram, with what the block of lines by hand
/// calls it ([`by_hand`]).
pub(super) fn arrows(root: &Container) -> Vec<(Arrow, Key)> {
    let mut found = Vec::new();
    collect(root, &mut Vec::new(), &mut found);
    by_hand::keyed(found, |path| name(root, path))
}

fn collect(container: &Container, path: &mut Vec<usize>, out: &mut Vec<Arrow>) {
    let within = |end: &[usize]| [path.as_slice(), end].concat();
    for (slot, edge) in container.edges.iter().enumerate() {
        out.push(Arrow {
            path: path.clone(),
            slot,
            from: within(&[edge.from]),
            to: within(&[edge.to]),
            order: edge.line,
            line: Some(edge.line),
        });
    }
    for (index, crossing) in container.crossings.iter().enumerate() {
        out.push(Arrow {
            path: path.clone(),
            slot: container.edges.len() + index,
            from: within(&crossing.from),
            to: within(&crossing.to),
            order: crossing.arrow.line,
            line: Some(crossing.arrow.line),
        });
    }
    for (index, node) in container.nodes.iter().enumerate() {
        path.push(index);
        collect(&node.children, path, out);
        path.pop();
    }
}

/// The name a line by hand gives the state at `path`: its key, `[*]` for a
/// start or an end, `Owner[H]` and `Owner[H*]` for history.
fn name(root: &Container, path: &[usize]) -> String {
    let mut container = root;
    let mut owner = "";
    for (depth, &index) in path.iter().enumerate() {
        let Some(node) = container.nodes.get(index) else {
            break;
        };
        if depth + 1 == path.len() {
            return match node.kind {
                Kind::Start | Kind::End => "[*]".to_owned(),
                Kind::History => format!("{owner}[H]"),
                Kind::DeepHistory => format!("{owner}[H*]"),
                _ => node.key.clone(),
            };
        }
        if node.kind != Kind::Region {
            owner = &node.key;
        }
        container = &node.children;
    }
    String::new()
}

/// Lays the transitions the writer drew by hand again, through their
/// bends; returns the entries that did not hold.
pub(super) fn lay_by_hand(
    arrows: &[(Arrow, Key)],
    scene: &mut Scene,
    hands: &[(usize, hand::Line)],
) -> Vec<(usize, String)> {
    if hands.is_empty() {
        return Vec::new();
    }
    let top = placements(&scene.nodes);
    by_hand::lay(&top, arrows, scene, hands)
}

/// Every transition drawn, as a [`Wire`] for the map. `curve` is how far
/// the lines curve.
pub(super) fn wires(
    arrows: &[(Arrow, Key)],
    scene: &Scene,
    hands: &[(usize, hand::Line)],
    curve: f32,
) -> Vec<Wire> {
    let top = placements(&scene.nodes);
    by_hand::wires(&top, arrows, scene, hands, |points| {
        drawn_route(points, curve)
    })
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
        let mut here = self;
        for &index in path {
            here = &mut here.nodes.get_mut(index)?.children;
        }
        let edge = here.edges.get_mut(slot)?;
        Some(LineMut {
            label: (edge.label.width, edge.label.height),
            points: &mut edge.points,
            label_at: &mut edge.label_at,
        })
    }

    fn drawn(&self) -> Drawn {
        let mut drawn = Drawn::default();
        drawn_in(self, &mut drawn);
        drawn
    }
}

fn scene_at<'a>(scene: &'a mut Scene, path: &[usize]) -> &'a mut Scene {
    match path.split_first() {
        Some((&first, rest)) => scene_at(&mut scene.nodes[first].children, rest),
        None => scene,
    }
}

fn placements(nodes: &[Placed]) -> Vec<Placement> {
    nodes
        .iter()
        .map(|node| {
            let frame = node.composite && node.kind != Kind::Region;
            // The whole title band: lines come in below it.
            let title = (frame && node.header_h > 0.0).then_some(Rect {
                h: COMP_PAD + node.header_h,
                ..node.rect
            });
            Placement {
                rect: node.rect,
                flat: matches!(
                    node.kind,
                    Kind::State | Kind::Fork | Kind::Join | Kind::Region
                ),
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

/// The lines, labels, notes and region separators already drawn.
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
        for (a, b) in separators(node) {
            drawn.lines.push(vec![a, b]);
        }
        drawn_in(&node.children, drawn);
    }
}

/// The dashed lines between the concurrent regions of `node`, border to
/// border.
pub(super) fn separators(node: &Placed) -> Vec<((f32, f32), (f32, f32))> {
    let regions = &node.children.nodes;
    regions
        .windows(2)
        .filter(|pair| pair[1].kind == Kind::Region)
        .map(|pair| {
            let (a, b) = (pair[0].rect, pair[1].rect);
            if b.y >= a.y + a.h - 0.5 {
                ((node.rect.x, b.y), (node.rect.x + node.rect.w, b.y))
            } else {
                ((b.x, b.y), (b.x, b.y + b.h))
            }
        })
        .collect()
}
