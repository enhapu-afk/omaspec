//! Subgraphs and nested states laid out with what is around them.
//!
//! A subgraph or nested state that a line runs into from outside is
//! opened: its members take the ranks of the container around it, as
//! Graphviz (and Mermaid's dagre) lay out a cluster, and the line runs
//! through the ranks like any other, in through the side the flow enters
//! by ([`graph::framed`]). One with concurrent regions, with a direction of
//! its own or with a line from itself to itself stays a box, laid out on
//! its own; a line into one of those is routed once everything is placed.

use std::collections::{HashMap, HashSet};

use omaspec_uml::graph::{self, Rect};
use omaspec_uml::{Block, Family, Font, Stroke};

use super::{
    COMP_GAP, COMP_PAD, Cap, Flip, Kind, LOOP_REACH, LOOP_STEP, Measured, Member, Palette, Placed,
    PlacedEdge, Scene, Spot, Tree, axes, caps, clip_ends, edge_look, measure_edge, measure_member,
    nest, place, shape,
};

/// The path of the container at `spot` from the top.
fn path_of(tree: &Tree, spot: Spot) -> Option<Vec<usize>> {
    tree.spots
        .iter()
        .find(|&(_, &at)| at == spot)
        .map(|(path, _)| path.clone())
}

/// Which groups inside the container at `spot` (at path `base`) to open,
/// by their paths from it: every one a line from a container laid out
/// here passes into.
fn opened(tree: &Tree, spot: Spot, base: &[usize]) -> HashSet<Vec<usize>> {
    let mut open = HashSet::new();
    let mut todo = vec![spot];
    while let Some(here) = todo.pop() {
        for (index, meet) in tree.meets.iter().enumerate() {
            if meet.spot != here || !meet.crossing || meet.hidden || meet.inward {
                continue;
            }
            let edge = &tree.graph.edges[index];
            let (Some(from), Some(to)) = (tree.paths.get(&edge.from), tree.paths.get(&edge.to))
            else {
                continue;
            };
            let groups = |path: &[usize]| -> Option<Vec<(Vec<usize>, usize)>> {
                let mut out = Vec::new();
                for depth in base.len() + 1..path.len() {
                    let at = &path[..depth];
                    let Some(&Spot::In(group)) = tree.spots.get(at) else {
                        return None;
                    };
                    let around = *tree.spots.get(&path[..depth - 1])?;
                    if !openable(tree, group, around, path[depth - 1]) {
                        return None;
                    }
                    out.push((at[base.len()..].to_vec(), group));
                }
                Some(out)
            };
            let (Some(a), Some(b)) = (groups(from), groups(to)) else {
                continue;
            };
            for (path, group) in a.into_iter().chain(b) {
                if open.insert(path) {
                    todo.push(Spot::In(group));
                }
            }
        }
    }
    open
}

/// Whether `group`, member `slot` of the container at `around`, can be
/// opened.
fn openable(tree: &Tree, group: usize, around: Spot, slot: usize) -> bool {
    let inner = Spot::In(group);
    let members = tree.members(inner);
    let regions = members
        .iter()
        .any(|member| matches!(member, Member::Group(index) if tree.is_region(*index)));
    let looped = tree
        .meets
        .iter()
        .any(|meet| meet.spot == around && !meet.inward && meet.from == slot && meet.to == slot);
    !tree.is_region(group)
        && !members.is_empty()
        && !regions
        && !looped
        && tree.flow(inner) == tree.flow(around)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Node(usize),
    Frame(usize),
}

/// One line of a container: the edge of mmdr's graph, its graph edge,
/// and how it is drawn.
struct Arrow {
    edge: usize,
    at: usize,
    label: Block,
    stroke: Stroke,
    caps: (Cap, Cap),
    /// Drawn by this layout.
    drawn: bool,
    crossing: bool,
    from: usize,
    to: usize,
}

/// One layout, through the groups it opens.
#[derive(Default)]
struct Flat {
    ends: HashMap<Vec<usize>, End>,
    measured: Vec<Option<Measured>>,
    framed: Vec<Option<Measured>>,
    nodes: Vec<graph::Node>,
    frames: Vec<graph::Frame>,
    within: Vec<Option<usize>>,
    edges: Vec<graph::Edge>,
    levels: HashMap<Vec<usize>, Vec<Arrow>>,
    crossings: Vec<Crossing>,
}

/// A line between containers drawn here: the path to the container that
/// holds both ends, the edge, and the paths to its ends.
struct Crossing {
    home: Vec<usize>,
    edge: usize,
    from: Vec<usize>,
    to: Vec<usize>,
}

/// What every step of a layout reads.
struct Context<'a> {
    tree: &'a Tree<'a>,
    open: &'a HashSet<Vec<usize>>,
    /// The path of the container laid out, from the top.
    base: &'a [usize],
    flip: Flip,
    palette: &'a Palette,
    family: Family,
}

/// Lays the container at `spot` out, and every group it opens with it.
pub(super) fn layout_open(
    tree: &Tree,
    spot: Spot,
    palette: &Palette,
    family: Family,
) -> Option<Scene> {
    let base = path_of(tree, spot)?;
    let open = opened(tree, spot, &base);
    let (axis, flip) = axes(tree.flow(spot));
    let mut flat = Flat::default();
    let cx = Context {
        tree,
        open: &open,
        base: &base,
        flip,
        palette,
        family,
    };
    flat.gather(&cx, spot, &mut Vec::new(), None)?;
    flat.join(&cx, spot, &mut Vec::new(), axis)?;
    let laid = graph::framed(&flat.nodes, &flat.frames, &flat.within, &flat.edges, axis);
    let mut scene = flat.assemble(tree, spot, &mut Vec::new(), &laid)?;
    scene.width = laid.width;
    scene.height = laid.height;

    // Lines end on the outline of a round or slanted box, a line between
    // containers on that of the box at its end.
    clip_all(&mut scene);
    for Crossing {
        home,
        edge,
        from,
        to,
    } in &flat.crossings
    {
        let outline = |path: &[usize]| {
            let end = placed_at(&scene, path)?;
            end.kind.outline(end.rect)
        };
        let (start, tip) = (outline(from), outline(to));
        let Some(line) = scene_at(&mut scene, home)?
            .edges
            .iter_mut()
            .find(|line| line.edge == *edge)
        else {
            continue;
        };
        if let Some(outline) = start {
            shape::clip(&mut line.points, &outline, true);
        }
        if let Some(outline) = tip {
            shape::clip(&mut line.points, &outline, false);
        }
    }
    let (width, height) = (scene.width, scene.height);
    match flip {
        Flip::No => {}
        Flip::Y => flip_y(&mut scene, height),
        Flip::X => flip_x(&mut scene, width),
    }

    // Labels move off boxes, titles and each other, over the whole layout.
    let mut boxes = Vec::new();
    let mut edges = Vec::new();
    collect(&mut scene, &mut boxes, &mut edges);
    let lines: Vec<Vec<(f32, f32)>> = edges.iter().map(|edge| edge.points.clone()).collect();
    let mut labels: Vec<_> = edges
        .iter()
        .map(|edge| (edge.label_at, (edge.label.width, edge.label.height)))
        .collect();
    let room = graph::route::place_labels(&lines, &mut labels, &boxes, (width, height));
    for (edge, (at, _)) in edges.into_iter().zip(labels) {
        edge.label_at = at;
    }
    scene.offset(-room.x, -room.y);
    (scene.width, scene.height) = (room.w, room.h);
    place_all_notes(tree, spot, &mut scene, palette, family)?;
    Some(scene)
}

fn clip_all(scene: &mut Scene) {
    clip_ends(scene);
    for node in &mut scene.nodes {
        if node.opened {
            clip_all(&mut node.children);
        }
    }
}

/// Mirrors the layout top to bottom, through the groups it opened; a
/// group laid out on its own moves as a whole.
fn flip_y(scene: &mut Scene, height: f32) {
    for node in &mut scene.nodes {
        let old = node.rect.y;
        node.rect.y = height - (node.rect.y + node.rect.h);
        if node.opened {
            flip_y(&mut node.children, height);
        } else {
            node.children.offset(0.0, node.rect.y - old);
        }
    }
    for edge in &mut scene.edges {
        for point in &mut edge.points {
            point.1 = height - point.1;
        }
        edge.label_at.1 = height - (edge.label_at.1 + edge.label.height);
    }
}

/// The same, left to right.
fn flip_x(scene: &mut Scene, width: f32) {
    for node in &mut scene.nodes {
        let old = node.rect.x;
        node.rect.x = width - (node.rect.x + node.rect.w);
        if node.opened {
            flip_x(&mut node.children, width);
        } else {
            node.children.offset(node.rect.x - old, 0.0);
        }
    }
    for edge in &mut scene.edges {
        for point in &mut edge.points {
            point.0 = width - point.0;
        }
        edge.label_at.0 = width - (edge.label_at.0 + edge.label.width);
    }
}

/// The boxes labels keep off, and every edge of this layout: through the
/// groups it opened, not into those laid out on their own.
fn collect<'a>(scene: &'a mut Scene, boxes: &mut Vec<Rect>, out: &mut Vec<&'a mut PlacedEdge>) {
    let Scene { nodes, edges, .. } = scene;
    out.extend(edges.iter_mut());
    for node in nodes.iter_mut() {
        if node.opened {
            boxes.extend(nest::cluster_title(node).or(Some(Rect {
                h: COMP_PAD + node.header_h,
                ..node.rect
            })));
            collect(&mut node.children, boxes, out);
        } else {
            boxes.push(node.rect);
        }
    }
}

/// Notes go beside their states once the labels are placed.
fn place_all_notes(
    tree: &Tree,
    spot: Spot,
    scene: &mut Scene,
    palette: &Palette,
    family: Family,
) -> Option<()> {
    let members = tree.members(spot);
    scene.notes = place::place_notes(tree, spot, &members, scene, palette, family)?;
    for (member, node) in members.iter().zip(scene.nodes.iter_mut()) {
        if let (Member::Group(group), true) = (member, node.opened) {
            place_all_notes(tree, Spot::In(*group), &mut node.children, palette, family)?;
        }
    }
    Some(())
}

fn placed_at<'a>(scene: &'a Scene, path: &[usize]) -> Option<&'a Placed> {
    let (&last, above) = path.split_last()?;
    let mut scene = scene;
    for &index in above {
        scene = &scene.nodes.get(index)?.children;
    }
    scene.nodes.get(last)
}

fn scene_at<'a>(scene: &'a mut Scene, path: &[usize]) -> Option<&'a mut Scene> {
    match path.split_first() {
        Some((&first, rest)) => scene_at(&mut scene.nodes.get_mut(first)?.children, rest),
        None => Some(scene),
    }
}

impl Flat {
    /// Every member of the container at `spot` and of the groups it opens:
    /// a box, or a frame around what it holds.
    fn gather(
        &mut self,
        cx: &Context,
        spot: Spot,
        path: &mut Vec<usize>,
        frame: Option<usize>,
    ) -> Option<()> {
        let Context {
            tree,
            open,
            flip,
            palette,
            family,
            ..
        } = *cx;
        let (axis, _) = axes(tree.flow(spot));
        for (index, member) in tree.members(spot).iter().enumerate() {
            path.push(index);
            match member {
                Member::Group(group) if open.contains(path.as_slice()) => {
                    let item = super::measure_group(tree, *group, palette, family, true)?;
                    let at = self.frames.len();
                    self.frames.push(frame_of(&item, frame, flip));
                    self.framed.push(Some(item));
                    self.ends.insert(path.clone(), End::Frame(at));
                    self.gather(cx, Spot::In(*group), path, Some(at))?;
                }
                _ => {
                    let item = measure_member(tree, member, axis, palette, family)?;
                    self.ends.insert(path.clone(), End::Node(self.nodes.len()));
                    self.nodes.push(graph::Node {
                        width: item.width,
                        height: item.height,
                        before: 0.0,
                        loop_extra: 0.0,
                        flat: item.kind.flat(),
                    });
                    self.within.push(frame);
                    self.measured.push(Some(item));
                }
            }
            path.pop();
        }
        Some(())
    }

    fn index(&self, path: &[usize]) -> Option<usize> {
        match self.ends.get(path)? {
            End::Node(at) => Some(*at),
            End::Frame(at) => Some(self.nodes.len() + at),
        }
    }

    /// The lines and room for notes of the container at `spot`, at `path`
    /// from the one laid out, and of the groups it opens.
    fn join(
        &mut self,
        cx: &Context,
        spot: Spot,
        path: &mut Vec<usize>,
        axis: graph::Direction,
    ) -> Option<()> {
        let Context {
            tree,
            open,
            base,
            palette,
            family,
            ..
        } = *cx;
        let child = |path: &[usize], index: usize| [path, &[index]].concat();
        let mut arrows = Vec::new();
        for (index, edge) in tree.graph.edges.iter().enumerate() {
            let meet = &tree.meets[index];
            if meet.spot != spot || meet.inward {
                continue;
            }
            let (stroke, color) = edge_look(tree.graph, index, palette)?;
            let label = measure_edge(tree, &edge.label, Font::new(palette.size, color), family);
            let (from, to, drawn) = if meet.crossing {
                let ends = |id: &String| -> Option<(Option<usize>, bool, Vec<usize>)> {
                    let full = tree.paths.get(id)?;
                    let steps = full.get(base.len() + path.len()..)?;
                    let (at, whole) = self.resolve(path, steps, open);
                    Some((at, whole, full[base.len()..].to_vec()))
                };
                let (from, whole_from, from_path) = ends(&edge.from)?;
                let (to, whole_to, to_path) = ends(&edge.to)?;
                let drawn = whole_from && whole_to && !meet.hidden;
                if drawn {
                    self.crossings.push(Crossing {
                        home: path.clone(),
                        edge: index,
                        from: from_path,
                        to: to_path,
                    });
                }
                (from, to, drawn)
            } else {
                let from = self.index(&child(path, meet.from));
                let to = self.index(&child(path, meet.to));
                (from, to, !meet.hidden)
            };
            let (from, to) = (from.unwrap_or(usize::MAX), to.unwrap_or(usize::MAX));
            if !meet.crossing && meet.from == meet.to && from < self.nodes.len() {
                let reach = LOOP_REACH
                    + arrows
                        .iter()
                        .filter(|arrow: &&Arrow| arrow.from == from && arrow.to == from)
                        .count() as f32
                        * LOOP_STEP;
                let extra = match axis {
                    graph::Direction::Down => reach + 4.0 + label.width,
                    graph::Direction::Right => reach + 4.0 + label.height,
                };
                let node = &mut self.nodes[from];
                node.loop_extra = node.loop_extra.max(extra);
            }
            arrows.push(Arrow {
                edge: index,
                at: self.edges.len(),
                label: if drawn { label.clone() } else { Block::empty() },
                stroke,
                caps: caps(edge),
                drawn,
                crossing: meet.crossing,
                from,
                to,
            });
            self.edges.push(graph::Edge {
                from,
                to,
                label_width: label.width,
                label_height: label.height,
                hidden: !drawn,
            });
        }
        // A note beside a state, when the states run down, sits in room
        // kept beside it, past its self-loop.
        let members = tree.members(spot);
        if axis == graph::Direction::Down {
            let room = place::note_room(tree, spot, &members, palette, family)?;
            for (index, (left, right)) in room.into_iter().enumerate() {
                if let Some(End::Node(at)) = self.ends.get(&child(path, index)).copied() {
                    let node = &mut self.nodes[at];
                    node.before = left;
                    node.loop_extra += right;
                }
            }
        }
        self.levels.insert(path.clone(), arrows);
        for (index, member) in members.iter().enumerate() {
            path.push(index);
            if let (Member::Group(group), true) = (member, open.contains(path.as_slice())) {
                self.join(cx, Spot::In(*group), path, axis)?;
            }
            path.pop();
        }
        Some(())
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

    /// The scene of the container at `spot` and `path`, with the scenes of
    /// the groups it opened inside.
    fn assemble(
        &mut self,
        tree: &Tree,
        spot: Spot,
        path: &mut Vec<usize>,
        laid: &graph::Layout,
    ) -> Option<Scene> {
        let members = tree.members(spot);
        let mut nodes = Vec::with_capacity(members.len());
        for (index, member) in members.iter().enumerate() {
            path.push(index);
            let placed = match (self.ends.get(path.as_slice()).copied()?, member) {
                (End::Frame(at), Member::Group(group)) => {
                    let item = self.framed[at].take()?;
                    let children = self.assemble(tree, Spot::In(*group), path, laid)?;
                    let rect = *laid.frames.get(at)?;
                    Placed {
                        opened: true,
                        children: Scene {
                            width: rect.w,
                            height: rect.h,
                            ..children
                        },
                        ..Placed::of(item, rect)
                    }
                }
                (End::Node(at), _) => {
                    let mut item = self.measured[at].take()?;
                    let rect = *laid.nodes.get(at)?;
                    if item.nested {
                        let ox = rect.x + (rect.w - item.children.width) / 2.0;
                        let oy = rect.y + COMP_PAD + item.header_h + item.gap;
                        item.children.offset(ox, oy);
                        nest::fill_regions(
                            &mut item.children,
                            rect,
                            rect.y + COMP_PAD + item.header_h,
                        );
                    }
                    Placed::of(item, rect)
                }
                (End::Frame(_), Member::Leaf(_)) => return None,
            };
            nodes.push(placed);
            path.pop();
        }
        let arrows = self.levels.remove(path.as_slice())?;
        let mut edges = Vec::with_capacity(arrows.len());
        for arrow in arrows {
            if !arrow.drawn {
                continue;
            }
            let route = laid.routes.get(arrow.at)?;
            let meet = &tree.meets[arrow.edge];
            // A crossing's ends are not in this container; it is clipped by
            // its paths.
            let (from, to) = if arrow.crossing {
                (usize::MAX, usize::MAX)
            } else {
                (meet.from, meet.to)
            };
            edges.push(PlacedEdge {
                from,
                to,
                edge: arrow.edge,
                points: route.points.clone(),
                label: arrow.label,
                label_at: route.label_at,
                stroke: arrow.stroke,
                caps: arrow.caps,
            });
        }
        Some(Scene {
            nodes,
            edges,
            notes: Vec::new(),
            width: 0.0,
            height: 0.0,
        })
    }
}

/// The room an opened group keeps around what it holds, as
/// [`super::measure_group`] sizes one around a layout of its own; the room
/// for the title comes first along the flow, so a mirrored flow takes it
/// from the other end. Lines from above pass beside the title, unless the
/// flow is mirrored.
fn frame_of(item: &Measured, parent: Option<usize>, flip: Flip) -> graph::Frame {
    let header_w = item
        .description
        .iter()
        .map(|block| block.width)
        .fold(item.title.width, f32::max);
    let (top, bottom) = (COMP_PAD + item.header_h + COMP_GAP, COMP_PAD);
    let (top, bottom) = match flip {
        Flip::Y => (bottom, top),
        _ => (top, bottom),
    };
    let width = match item.kind {
        Kind::Cluster => item.title.width + 2.0 * nest::TITLE_AIR + 4.0,
        _ => header_w + 8.0,
    };
    let name = match flip {
        Flip::Y => graph::Name::default(),
        _ => graph::Name {
            share: 0.5,
            offset: -width / 2.0,
            width,
            height: COMP_PAD + item.header_h,
        },
    };
    graph::Frame {
        parent,
        top,
        bottom,
        left: COMP_PAD,
        right: COMP_PAD,
        min_width: header_w + COMP_PAD * 2.0,
        name,
    }
}
