//! Nested states laid out with what is around them.
//!
//! A nested state that a transition runs into from outside is opened: its
//! states take the ranks of the container around it, as Graphviz lays
//! out a cluster, and the transition runs through the ranks like any
//! other, in through the side the flow enters by ([`graph::framed`]). A
//! nested state with concurrent regions, with a direction of its own or
//! with a transition from itself to itself stays a box, laid out on its
//! own; a transition into one of those is routed once everything is
//! placed.

use std::collections::{HashMap, HashSet};

use super::super::{Container, Direction, Kind, Side};
use super::{
    COMP_GAP, COMP_PAD, LOOP_REACH, LOOP_STEP, Measured, NOTE_GAP, Placed, PlacedEdge, Scene, Skin,
    arrow_stroke, edge_label, fill_regions, graph_direction, measure_node, note_size, place_notes,
    text_block,
};
use crate::Declined;
use crate::graph::{self, Rect};
use crate::text::{Block, Family};

/// Which of the nested states inside `container` to open, by their paths
/// from it: every one a transition from a container laid out here passes
/// into.
fn opened(container: &Container) -> HashSet<Vec<usize>> {
    let mut open = HashSet::new();
    mark(container, &mut Vec::new(), &mut open);
    open
}

fn mark(container: &Container, at: &mut Vec<usize>, open: &mut HashSet<Vec<usize>>) {
    for crossing in &container.crossings {
        if !openable(container, &crossing.from) || !openable(container, &crossing.to) {
            continue;
        }
        for path in [&crossing.from, &crossing.to] {
            for depth in 1..path.len() {
                open.insert([at.as_slice(), &path[..depth]].concat());
            }
        }
    }
    for (index, node) in container.nodes.iter().enumerate() {
        at.push(index);
        if open.contains(at.as_slice()) {
            mark(&node.children, at, open);
        }
        at.pop();
    }
}

/// Whether every nested state on the way down `path` can be opened.
fn openable(container: &Container, path: &[usize]) -> bool {
    let mut here = container;
    for &index in &path[..path.len().saturating_sub(1)] {
        let Some(node) = here.nodes.get(index) else {
            return false;
        };
        let looped = here.edges.iter().any(|e| e.from == index && e.to == index);
        let inner = &node.children;
        if !node.composite
            || node.kind != Kind::State
            || looped
            || inner.split.is_some()
            || inner.direction != here.direction
            || inner.nodes.is_empty()
        {
            return false;
        }
        here = inner;
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Node(usize),
    Frame(usize),
}

/// Where the transitions and crossings of one container went among the
/// graph's edges.
#[derive(Default)]
struct Level {
    edges: Vec<usize>,
    /// The graph edge of each crossing, and whether it is drawn here.
    crossings: Vec<(usize, bool)>,
}

/// One layout, through the nested states it opens.
#[derive(Default)]
struct Flat {
    ends: HashMap<Vec<usize>, End>,
    measured: Vec<Option<Measured>>,
    framed: Vec<Option<Measured>>,
    nodes: Vec<graph::Node>,
    frames: Vec<graph::Frame>,
    within: Vec<Option<usize>>,
    edges: Vec<graph::Edge>,
    labels: Vec<Block>,
    levels: HashMap<Vec<usize>, Level>,
}

/// Lays `container` out, and every nested state it opens with it.
pub(super) fn layout_open(
    container: &Container,
    skin: &Skin,
    family: Family,
) -> Result<Scene, Declined> {
    let open = opened(container);
    let mut flat = Flat::default();
    flat.gather(container, &mut Vec::new(), None, &open, skin, family)?;
    flat.join(container, &mut Vec::new(), &open, skin, family)?;
    let laid = graph::framed(
        &flat.nodes,
        &flat.frames,
        &flat.within,
        &flat.edges,
        graph_direction(container.direction),
    );
    let mut scene = flat.assemble(container, &mut Vec::new(), &laid, skin)?;

    // Labels move off states, names and each other, over the whole layout.
    let mut boxes = Vec::new();
    let mut edges = Vec::new();
    collect(&mut scene, &mut boxes, &mut edges);
    let lines: Vec<Vec<(f32, f32)>> = edges.iter().map(|edge| edge.points.clone()).collect();
    let mut labels: Vec<_> = edges
        .iter()
        .map(|edge| (edge.label_at, (edge.label.width, edge.label.height)))
        .collect();
    let room = graph::route::place_labels(&lines, &mut labels, &boxes, (laid.width, laid.height));
    for (edge, (at, _)) in edges.into_iter().zip(labels) {
        edge.label_at = at;
    }
    place_all_notes(container, &mut scene, skin, family)?;
    scene.width = room.w;
    scene.height = room.h;
    scene.offset(-room.x, -room.y);
    Ok(scene)
}

/// The boxes labels keep off, and every edge of this layout: through the
/// nested states it opened, not into those laid out on their own.
fn collect<'a>(scene: &'a mut Scene, boxes: &mut Vec<Rect>, out: &mut Vec<&'a mut PlacedEdge>) {
    let Scene { nodes, edges, .. } = scene;
    out.extend(edges.iter_mut());
    for node in nodes.iter_mut() {
        if node.opened {
            let w = name_width(node);
            boxes.push(Rect {
                x: node.rect.x + (node.rect.w - w) / 2.0,
                y: node.rect.y,
                w,
                h: COMP_PAD + node.header_h,
            });
            collect(&mut node.children, boxes, out);
        } else {
            boxes.push(node.rect);
        }
    }
}

fn name_width(node: &Placed) -> f32 {
    node.description
        .iter()
        .map(|block| block.width)
        .fold(node.title.width, f32::max)
        + 8.0
}

/// Notes go beside their states once the labels are placed, so they step
/// off them.
fn place_all_notes(
    container: &Container,
    scene: &mut Scene,
    skin: &Skin,
    family: Family,
) -> Result<(), Declined> {
    let rects: Vec<Rect> = scene.nodes.iter().map(|node| node.rect).collect();
    scene.notes = place_notes(container, &rects, &scene.edges, skin, family)?;
    for (node, placed) in container.nodes.iter().zip(scene.nodes.iter_mut()) {
        if placed.opened {
            place_all_notes(&node.children, &mut placed.children, skin, family)?;
        }
    }
    Ok(())
}

impl Flat {
    /// Every state of `container` at `path` and of the nested states it
    /// opens: a box, or a frame around what it holds.
    fn gather(
        &mut self,
        container: &Container,
        path: &mut Vec<usize>,
        frame: Option<usize>,
        open: &HashSet<Vec<usize>>,
        skin: &Skin,
        family: Family,
    ) -> Result<(), Declined> {
        for (index, node) in container.nodes.iter().enumerate() {
            path.push(index);
            if open.contains(path.as_slice()) {
                let item = measure_node(node, container.direction, skin, family, true)?;
                let at = self.frames.len();
                let header_w = item
                    .description
                    .iter()
                    .map(|block| block.width)
                    .fold(item.title.width, f32::max);
                let name = header_w + 8.0;
                self.frames.push(graph::Frame {
                    parent: frame,
                    top: COMP_PAD + item.header_h + COMP_GAP,
                    bottom: COMP_PAD,
                    left: COMP_PAD,
                    right: COMP_PAD,
                    min_width: header_w + COMP_PAD * 2.0,
                    name: graph::Name {
                        share: 0.5,
                        offset: -name / 2.0,
                        width: name,
                        height: COMP_PAD + item.header_h,
                    },
                });
                self.framed.push(Some(item));
                self.ends.insert(path.clone(), End::Frame(at));
                self.gather(&node.children, path, Some(at), open, skin, family)?;
            } else {
                let item = measure_node(node, container.direction, skin, family, false)?;
                self.ends.insert(path.clone(), End::Node(self.nodes.len()));
                self.nodes.push(graph::Node {
                    width: item.width,
                    height: item.height,
                    before: 0.0,
                    loop_extra: 0.0,
                    flat: matches!(
                        item.kind,
                        Kind::State | Kind::Fork | Kind::Join | Kind::Region
                    ),
                });
                self.within.push(frame);
                self.measured.push(Some(item));
            }
            path.pop();
        }
        Ok(())
    }

    /// The graph index of the state at `path`.
    fn index(&self, path: &[usize]) -> Option<usize> {
        match self.ends.get(path)? {
            End::Node(at) => Some(*at),
            End::Frame(at) => Some(self.nodes.len() + at),
        }
    }

    /// The transitions, crossings and room for notes of `container` at
    /// `path` and of the nested states it opens.
    fn join(
        &mut self,
        container: &Container,
        path: &mut Vec<usize>,
        open: &HashSet<Vec<usize>>,
        skin: &Skin,
        family: Family,
    ) -> Result<(), Declined> {
        let count = container.nodes.len();
        let child = |path: &[usize], index: usize| [path, &[index]].concat();
        let mut level = Level::default();
        let mut loops = vec![0usize; count];
        for edge in &container.edges {
            let label = edge_label(edge, skin, family)?;
            let ends = (
                self.index(&child(path, edge.from)),
                self.index(&child(path, edge.to)),
            );
            let (from, to) = match ends {
                (Some(from), Some(to)) => (from, to),
                // An edge the layout drops: it names no state.
                _ => (usize::MAX, usize::MAX),
            };
            if from == to && from < self.nodes.len() && edge.from < count {
                let channel = loops[edge.from];
                loops[edge.from] += 1;
                let reach = LOOP_REACH + channel as f32 * LOOP_STEP;
                let extra = match container.direction {
                    Direction::Down => reach + 4.0 + label.width,
                    Direction::Right => reach + 4.0 + label.height,
                };
                let node = &mut self.nodes[from];
                node.loop_extra = node.loop_extra.max(extra);
            }
            level.edges.push(self.edges.len());
            self.edges.push(graph::Edge {
                from,
                to,
                label_width: label.width,
                label_height: label.height,
                hidden: false,
            });
            self.labels.push(label);
        }
        for crossing in &container.crossings {
            let label = edge_label(&crossing.arrow, skin, family)?;
            let (from, whole_from) = self.resolve(path, &crossing.from, open);
            let (to, whole_to) = self.resolve(path, &crossing.to, open);
            // Drawn here when both ends are; otherwise it only ranks the
            // boxes that hold them, and is routed later.
            let drawn = whole_from && whole_to;
            level.crossings.push((self.edges.len(), drawn));
            self.edges.push(graph::Edge {
                from: from.unwrap_or(usize::MAX),
                to: to.unwrap_or(usize::MAX),
                label_width: label.width,
                label_height: label.height,
                hidden: !drawn,
            });
            self.labels.push(if drawn { label } else { Block::empty() });
        }
        // A note left or right of a state, when the states run down, sits
        // in room kept beside it, past its self-loop.
        if container.direction == Direction::Down {
            for note in &container.notes {
                let Some(End::Node(at)) = self.ends.get(&child(path, note.target)).copied() else {
                    continue;
                };
                let (w, _) = note_size(&text_block(&note.text, skin.note.font, family)?);
                let node = &mut self.nodes[at];
                match note.side {
                    Side::Left => node.before = node.before.max(NOTE_GAP + w),
                    Side::Right => node.loop_extra += NOTE_GAP + w,
                    Side::Top | Side::Bottom => {}
                }
            }
        }
        self.levels.insert(path.clone(), level);
        for (index, node) in container.nodes.iter().enumerate() {
            path.push(index);
            if open.contains(path.as_slice()) {
                self.join(&node.children, path, open, skin, family)?;
            }
            path.pop();
        }
        Ok(())
    }

    /// The graph index of a crossing's end, down `steps` from the
    /// container at `path`, and whether that is the end itself rather than
    /// the closed box around it.
    fn resolve(
        &self,
        path: &[usize],
        steps: &[usize],
        open: &HashSet<Vec<usize>>,
    ) -> (Option<usize>, bool) {
        let mut at = path.to_vec();
        for (depth, &step) in steps.iter().enumerate() {
            at.push(step);
            let last = depth + 1 == steps.len();
            if last || !open.contains(&at) {
                return (self.index(&at), last);
            }
        }
        (None, false)
    }

    /// The scene of `container` at `path`, from the layout, with the
    /// scenes of the nested states it opened inside.
    fn assemble(
        &mut self,
        container: &Container,
        path: &mut Vec<usize>,
        laid: &graph::Layout,
        skin: &Skin,
    ) -> Result<Scene, Declined> {
        let mut nodes = Vec::with_capacity(container.nodes.len());
        for (index, node) in container.nodes.iter().enumerate() {
            path.push(index);
            let placed = match self.ends.get(path.as_slice()).copied() {
                Some(End::Frame(at)) => {
                    let item = self.framed[at].take().expect("one frame each");
                    let children = self.assemble(&node.children, path, laid, skin)?;
                    let rect = laid.frames[at];
                    Placed {
                        children: Scene {
                            width: rect.w,
                            height: rect.h,
                            ..children
                        },
                        opened: true,
                        ..Placed::of(item, rect)
                    }
                }
                Some(End::Node(at)) => {
                    let mut item = self.measured[at].take().expect("one box each");
                    let rect = laid.nodes[at];
                    if item.composite {
                        let ox = rect.x + (rect.w - item.children.width) / 2.0;
                        let oy = rect.y + COMP_PAD + item.header_h + item.gap;
                        item.children.offset(ox, oy);
                        fill_regions(&mut item.children, rect, rect.y + COMP_PAD + item.header_h);
                    }
                    Placed::of(item, rect)
                }
                None => unreachable!("every state was gathered"),
            };
            nodes.push(placed);
            path.pop();
        }
        let Some(level) = self.levels.get(path.as_slice()) else {
            unreachable!("every container was joined");
        };
        let mut edges = Vec::with_capacity(container.edges.len() + container.crossings.len());
        let arrows = container
            .edges
            .iter()
            .zip(level.edges.iter().map(|&at| (at, true)))
            .chain(
                container
                    .crossings
                    .iter()
                    .map(|crossing| &crossing.arrow)
                    .zip(level.crossings.iter().copied()),
            );
        for (arrow, (at, drawn)) in arrows {
            let route = &laid.routes[at];
            let label = std::mem::replace(&mut self.labels[at], Block::empty());
            edges.push(if drawn {
                PlacedEdge {
                    points: route.points.clone(),
                    label,
                    label_at: route.label_at,
                    stroke: arrow_stroke(skin, arrow)?,
                }
            } else {
                // Routed once everything is placed.
                PlacedEdge {
                    points: Vec::new(),
                    label: Block::empty(),
                    label_at: (0.0, 0.0),
                    stroke: arrow_stroke(skin, arrow)?,
                }
            });
        }
        Ok(Scene {
            nodes,
            edges,
            notes: Vec::new(),
            width: 0.0,
            height: 0.0,
        })
    }
}
