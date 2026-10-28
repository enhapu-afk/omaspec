//! Concurrent regions, and transitions between containers.

use super::super::{Container, Crossing, Kind, Split};
use super::{
    COMP_PAD, Placed, PlacedEdge, Scene, Skin, arrow_stroke, edge_label, layout_container,
};
use crate::Declined;
use crate::graph::Rect;
use crate::graph::across::{self, Drawn, Link, Placement};
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
