//! Mermaid state diagrams and flowcharts on the shared graph layout.
//!
//! The graph is mmdr's: mmdr parses a flowchart, and a state diagram
//! Omaspec reads is put in the same form ([`super::state`]); this lays
//! it out and draws it. A nested state or subgraph a line runs into from
//! outside is laid out with the container around it ([`open`]); any other
//! lays its own members out, in its own direction, and is placed as one
//! box. Concurrent regions, and lines into a box laid out on its own, are
//! in [`nest`]. Anything this
//! drawer does not know (a transition between a state and a state inside
//! it, a shape or an image it cannot draw, a color it cannot read) leaves
//! the diagram to mmdr, which still draws it. Notes stay in screen space,
//! so "right of" is to the right under every direction.

use std::collections::{HashMap, HashSet};

use mermaid_rs_renderer::{DiagramKind, Direction as Flow, Edge, EdgeDecoration, Graph, Theme};
use omaspec_uml::graph::across::{Drawn, meeting};
use omaspec_uml::graph::by_hand::{self, Arrow, Laid, LineMut};
use omaspec_uml::graph::{self, Rect};
use omaspec_uml::hand::{self, Key};
use omaspec_uml::map::{Map, Route, Shape as MapShape, Spot as MapSpot};
use omaspec_uml::{Block, Canvas, Color, Family, Font, Paint, Stroke};

use look::{Palette, edge_look, look_for, palette, styled_node};
use shape::{Kind, Shape};

mod flowchart;
mod label;
mod look;
mod nest;
mod open;
mod paint;
mod place;
mod shape;

#[cfg(test)]
mod tests;

const MARGIN: f32 = 10.0;
const PAD_X: f32 = 14.0;
const PAD_Y: f32 = 8.0;
const MIN_W: f32 = 40.0;
const MIN_H: f32 = 28.0;
const COMP_PAD: f32 = 12.0;
const COMP_GAP: f32 = 8.0;
const START: f32 = 22.0;
const END: f32 = 26.0;
const BAR_LONG: f32 = 80.0;
const BAR_SHORT: f32 = 8.0;
const CHOICE_MIN: f32 = 34.0;
const RADIUS: f32 = 8.0;
const NOTE_GAP: f32 = 10.0;
const NOTE_PAD: f32 = 6.0;
const HEAD_LENGTH: f32 = 10.0;
const HEAD_HALF: f32 = 4.0;
const LOOP_REACH: f32 = 16.0;
const LOOP_STEP: f32 = 12.0;
/// Where Mermaid wraps a flowchart label, at its default 16 px font.
const WRAP: f32 = 200.0;

/// Where the states and transitions of a state diagram Omaspec read are
/// written: each state by its id in the graph, each transition by its
/// index.
#[derive(Debug, Default)]
pub struct Lines {
    pub states: HashMap<String, usize>,
    pub transitions: Vec<usize>,
}

/// The SVG and its map: for a state diagram Omaspec read (`lines`), where
/// its states and transitions are, and for every diagram its lines, which
/// `hands` may have drawn by hand (docs/adr/0016). `None` when this drawer
/// leaves the diagram to mmdr. `source` is what mmdr parsed, for what its
/// graph does not keep. The lines' corners curve by up to `curve`.
pub fn try_draw(
    graph: &Graph,
    theme: &Theme,
    source: &str,
    lines: Option<&Lines>,
    hands: &[(usize, hand::Line)],
    curve: f32,
) -> Option<(String, Map)> {
    let own;
    // Omaspec's reading gives every region its own start and end.
    let graph = match lines.is_none().then(|| nest::own_markers(graph)).flatten() {
        Some(split) => {
            own = split;
            &own
        }
        None => graph,
    };
    let mut repaired = None;
    let (graph, fix) = flowchart::apply(graph, source, &mut repaired)?;
    let tree = prepare(graph, source, &fix)?;
    let palette = palette(theme, tree.flowchart)?;
    let family = Family::of(&theme.font_family);
    let mut scene = layout_spot(&tree, Spot::Root, &palette, family)?;
    nest::link_crossings(&tree, &mut scene, &palette, family)?;
    nest::link_inward(&tree, &mut scene, &palette, family)?;
    let arrows = arrows(&tree, lines);
    let misses = if hands.is_empty() {
        Vec::new()
    } else {
        let top = nest::placements(&scene.nodes);
        let mut laying = Laying {
            tree: &tree,
            scene: &mut scene,
        };
        by_hand::lay(&top, &arrows, &mut laying, hands)
    };
    let mut canvas = Canvas::new(&theme.font_family, false);
    paint::paint(&mut canvas, &palette, &scene, curve);
    let mut map = Map::default();
    let top = nest::placements(&scene.nodes);
    let laying = Laying {
        tree: &tree,
        scene: &mut scene,
    };
    map.wires = by_hand::wires(&top, &arrows, &laying, hands, |points| {
        omaspec_uml::curve::flatten(points, curve)
    });
    map.misses = misses;
    if let Some(lines) = lines.filter(|_| !tree.flowchart) {
        let mut routes = Vec::new();
        chart(
            &tree,
            Spot::Root,
            &scene,
            lines,
            &mut map,
            &mut routes,
            curve,
        );
        for (edge, points) in routes {
            if let Some(&line) = lines.transitions.get(edge) {
                map.transitions.push(Route { line, points });
            }
        }
    }
    let map = map.placed(canvas.fit(MARGIN));
    Some((canvas.finish(MARGIN, Some(palette.background)), map))
}

/// Every line drawn, with what the block of lines by hand calls it: a
/// state diagram's starts and ends are `[*]`, anything else goes by its
/// id. Lines between the same two count in the order of the source; a
/// flowchart's are in that order already. A line that only ranks
/// (`~~~`) or runs from a subgraph into it is left alone.
fn arrows(tree: &Tree, lines: Option<&Lines>) -> Vec<(Arrow, Key)> {
    let names: HashMap<&Vec<usize>, &String> =
        tree.paths.iter().map(|(id, path)| (path, id)).collect();
    let mut found = Vec::new();
    for (index, edge) in tree.graph.edges.iter().enumerate() {
        let meet = &tree.meets[index];
        if meet.hidden || meet.inward {
            continue;
        }
        let (Some(from), Some(to)) = (tree.paths.get(&edge.from), tree.paths.get(&edge.to)) else {
            continue;
        };
        let line = lines.and_then(|l| l.transitions.get(index).copied());
        found.push(Arrow {
            path: Vec::new(),
            slot: index,
            from: from.clone(),
            to: to.clone(),
            order: line.unwrap_or(index),
            line,
        });
    }
    by_hand::keyed(found, |path| {
        let id = names.get(&path.to_vec()).map_or("", |id| id.as_str());
        let marker = id.starts_with("__start_") || id.starts_with("__end_");
        if !tree.flowchart && marker {
            "[*]".to_owned()
        } else {
            id.to_owned()
        }
    })
}

/// The lines laid out, each found by the edge of mmdr's graph it draws.
struct Laying<'a, 'b> {
    tree: &'a Tree<'b>,
    scene: &'a mut Scene,
}

fn edge_in(scene: &Scene, edge: usize) -> Option<&PlacedEdge> {
    scene
        .edges
        .iter()
        .find(|e| e.edge == edge)
        .or_else(|| scene.nodes.iter().find_map(|n| edge_in(&n.children, edge)))
}

fn edge_in_mut(scene: &mut Scene, edge: usize) -> Option<&mut PlacedEdge> {
    if let Some(at) = scene.edges.iter().position(|e| e.edge == edge) {
        return scene.edges.get_mut(at);
    }
    scene
        .nodes
        .iter_mut()
        .find_map(|n| edge_in_mut(&mut n.children, edge))
}

impl Laid for Laying<'_, '_> {
    fn points(&self, _path: &[usize], slot: usize) -> Option<&[(f32, f32)]> {
        edge_in(self.scene, slot).map(|e| e.points.as_slice())
    }

    fn line_mut(&mut self, _path: &[usize], slot: usize) -> Option<LineMut<'_>> {
        let edge = edge_in_mut(self.scene, slot)?;
        Some(LineMut {
            label: (edge.label.width, edge.label.height),
            points: &mut edge.points,
            label_at: &mut edge.label_at,
        })
    }

    fn drawn(&self) -> Drawn {
        let mut drawn = Drawn::default();
        nest::drawn_in(self.scene, &mut drawn);
        drawn
    }

    /// Cut to the outline of a round or slanted box, as every line is.
    fn relaid(&mut self, _path: &[usize], slot: usize) {
        let Some(edge) = self.tree.graph.edges.get(slot) else {
            return;
        };
        let mut cuts = Vec::new();
        for (id, at_start) in [(&edge.from, true), (&edge.to, false)] {
            let end = self
                .tree
                .paths
                .get(id)
                .and_then(|path| nest::placed_at(self.scene, path));
            if let Some(outline) = end.and_then(|end| end.kind.outline(end.rect)) {
                cuts.push((outline, at_start));
            }
        }
        if let Some(placed) = edge_in_mut(self.scene, slot) {
            for (outline, at_start) in cuts {
                shape::clip(&mut placed.points, &outline, at_start);
            }
        }
    }
}

/// Puts the states of a state diagram, laid out as `scene` at `spot`, on
/// `map`, each on its line; notes each drawn transition by its edge. A
/// start or an end is nameless, as PlantUML's are, also where it is drawn
/// as a bar (several transitions out of it, or into it).
fn chart(
    tree: &Tree,
    spot: Spot,
    scene: &Scene,
    lines: &Lines,
    map: &mut Map,
    routes: &mut Vec<(usize, Vec<(f32, f32)>)>,
    curve: f32,
) {
    let owner = tree.owner(spot);
    for (member, placed) in tree.members(spot).iter().zip(&scene.nodes) {
        let (id, shape) = match member {
            Member::Leaf(id) => {
                let shape = match placed.kind {
                    Kind::State => MapShape::State,
                    Kind::Start => MapShape::Start,
                    Kind::End => MapShape::End,
                    Kind::Choice => MapShape::Choice,
                    Kind::Bar => MapShape::Bar,
                    _ => continue,
                };
                (Some(id.clone()), shape)
            }
            Member::Group(group) => {
                chart(
                    tree,
                    Spot::In(*group),
                    &placed.children,
                    lines,
                    map,
                    routes,
                    curve,
                );
                if tree.is_region(*group) {
                    continue;
                }
                (tree.graph.subgraphs[*group].id.clone(), MapShape::State)
            }
        };
        let Some(id) = id else {
            continue;
        };
        let Some(&line) = lines.states.get(&id) else {
            continue;
        };
        let marker = id.starts_with("__start_") || id.starts_with("__end_");
        let name = match shape {
            MapShape::Start | MapShape::End => String::new(),
            MapShape::Bar if marker => String::new(),
            _ => id.clone(),
        };
        let rect = placed.rect;
        let radius = match shape {
            MapShape::State => RADIUS.min(rect.w / 2.0).min(rect.h / 2.0),
            MapShape::Start | MapShape::End | MapShape::History => rect.w.min(rect.h) / 2.0,
            MapShape::Bar => 2.0,
            MapShape::Choice => 0.0,
        };
        map.states.push(MapSpot {
            name,
            shape,
            line,
            parent: owner.clone(),
            rect,
            radius,
        });
    }
    // The way each transition is drawn, its curve and then on to the tip,
    // for a play's light to run along.
    for edge in &scene.edges {
        let mut route =
            omaspec_uml::curve::flatten(&paint::shortened(&edge.points, edge.caps), curve);
        if let Some(&tip) = edge.points.last() {
            route.push(tip);
        }
        route.dedup();
        routes.push((edge.edge, route));
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Spot {
    Root,
    In(usize),
}

enum Member {
    Leaf(String),
    Group(usize),
}

struct Tree<'a> {
    graph: &'a Graph,
    flowchart: bool,
    parent: Vec<Option<usize>>,
    composite_of: HashMap<String, usize>,
    listing: HashMap<String, usize>,
    /// What each node is.
    kinds: HashMap<String, Kind>,
    /// Which way each subgraph runs.
    flows: Vec<Flow>,
    /// Every state's path of member indices from the top.
    paths: HashMap<String, Vec<usize>>,
    /// The container at the end of each path.
    spots: HashMap<Vec<usize>, Spot>,
    /// Where each transition is laid out, by index.
    meets: Vec<Meet>,
}

/// Where a transition is laid out: the container that holds both ends,
/// and the members there that hold them. A crossing joins states in
/// different members; it is ranked here and routed after the layout.
/// `hidden` ranks and is not drawn (`A ~~~ B`). `inward` joins a
/// flowchart subgraph to a node inside it and is routed after the layout.
struct Meet {
    spot: Spot,
    from: usize,
    to: usize,
    crossing: bool,
    hidden: bool,
    inward: bool,
}

fn prepare<'a>(graph: &'a Graph, source: &str, fix: &flowchart::Fix) -> Option<Tree<'a>> {
    let flowchart = match graph.kind {
        DiagramKind::State => false,
        DiagramKind::Flowchart => true,
        _ => return None,
    };
    if graph.subgraphs.iter().any(|sub| sub.icon.is_some()) {
        return None;
    }
    let doubles = if flowchart {
        let mut doubles = flowchart::double_circles(source);
        doubles.extend(fix.doubles.iter().cloned());
        doubles
    } else {
        HashSet::new()
    };
    let mut kinds = HashMap::with_capacity(graph.nodes.len());
    for (id, node) in &graph.nodes {
        if node.icon.is_some() {
            return None;
        }
        let kind = if flowchart {
            if fix.bare.contains(id) {
                Kind::Node(Shape::Label)
            } else {
                Kind::Node(Shape::of(node.shape, doubles.contains(id))?)
            }
        } else {
            Kind::of_state(node.shape)?
        };
        kinds.insert(id.clone(), kind);
    }
    if graph
        .edges
        .iter()
        .any(|edge| edge_unsupported(edge, flowchart))
    {
        return None;
    }
    for edge in &graph.edges {
        if !graph.nodes.contains_key(&edge.from) || !graph.nodes.contains_key(&edge.to) {
            return None;
        }
    }
    let mut composite_of = HashMap::new();
    for (index, sub) in graph.subgraphs.iter().enumerate() {
        if let Some(id) = &sub.id
            && composite_of.insert(id.clone(), index).is_some()
        {
            return None;
        }
    }
    let mut parent = parents(graph, flowchart);
    // An empty subgraph lists no nodes, so the set walk above cannot nest
    // it. The source's subgraph/end stack can, when it counts the same.
    if flowchart {
        let nested = flowchart::empty_nesting(source);
        if nested.len() == parent.len() {
            for (index, up) in nested.into_iter().enumerate() {
                if graph.subgraphs[index].nodes.is_empty() {
                    parent[index] = up;
                }
            }
        }
    }
    let mut tree = Tree {
        graph,
        flowchart,
        listing: listings(graph, &parent),
        flows: flows(graph, &parent, flowchart),
        parent,
        composite_of,
        kinds,
        paths: HashMap::new(),
        spots: HashMap::new(),
        meets: Vec::new(),
    };
    tree.walk(Spot::Root, &mut Vec::new());
    let mut meets = Vec::with_capacity(graph.edges.len());
    for (index, edge) in graph.edges.iter().enumerate() {
        let from = tree.paths.get(&edge.from)?;
        let to = tree.paths.get(&edge.to)?;
        let hidden = fix.invisible.contains(&index);
        let (from_home, to_home) = (&from[..from.len() - 1], &to[..to.len() - 1]);
        meets.push(if from_home == to_home {
            Meet {
                spot: *tree.spots.get(from_home)?,
                from: *from.last()?,
                to: *to.last()?,
                crossing: false,
                hidden,
                inward: false,
            }
        } else if let Some((depth, from_at, to_at)) = meeting(from, to) {
            Meet {
                spot: *tree.spots.get(&tree.paths[&edge.from][..depth])?,
                from: from_at,
                to: to_at,
                crossing: true,
                hidden,
                inward: false,
            }
        } else {
            // A flowchart line between a subgraph and a node inside it.
            // A state diagram still declines: one end holds the other.
            let outer = flowchart.then(|| held_by(from, to)).flatten()?;
            let member = *outer.last()?;
            Meet {
                spot: *tree.spots.get(&outer[..outer.len() - 1])?,
                from: member,
                to: member,
                crossing: false,
                hidden,
                inward: true,
            }
        });
    }
    tree.meets = meets;
    for note in &graph.state_notes {
        let known =
            graph.nodes.contains_key(&note.target) || tree.composite_of.contains_key(&note.target);
        if !known {
            return None;
        }
    }
    Some(tree)
}

/// The path of the end that contains the other, when one is a strict
/// prefix. `None` when the two only share a proper prefix or none.
fn held_by<'a>(from: &'a [usize], to: &'a [usize]) -> Option<&'a [usize]> {
    if to.len() > from.len() && to.starts_with(from) {
        Some(from)
    } else if from.len() > to.len() && from.starts_with(to) {
        Some(to)
    } else {
        None
    }
}

/// A flowchart line may end in a circle or a cross; anything else at an
/// end (a diamond, a label at the end) is not drawn here.
fn edge_unsupported(edge: &Edge, flowchart: bool) -> bool {
    let end = |decoration: Option<EdgeDecoration>| match decoration {
        None => false,
        Some(EdgeDecoration::Circle | EdgeDecoration::Cross) => !flowchart,
        Some(_) => true,
    };
    end(edge.start_decoration)
        || end(edge.end_decoration)
        || edge.arrow_start_kind.is_some()
        || edge.arrow_end_kind.is_some()
        || edge.start_label.is_some()
        || edge.end_label.is_some()
}

/// The subgraph each one sits in. mmdr lists every node in all the
/// subgraphs around it, so the parent is the smallest that holds all of
/// its nodes. Two can list the same: in a state diagram, a region with
/// one nested state holds it; in a flowchart, the one opened first holds
/// the other.
fn parents(graph: &Graph, flowchart: bool) -> Vec<Option<usize>> {
    let sets: Vec<HashSet<&str>> = graph
        .subgraphs
        .iter()
        .map(|sub| sub.nodes.iter().map(String::as_str).collect())
        .collect();
    let inside = |inner: usize, outer: usize| {
        if flowchart {
            inner > outer
        } else {
            nest::is_region(graph, outer) && !nest::is_region(graph, inner)
        }
    };
    let mut parent = vec![None; sets.len()];
    for (i, set) in sets.iter().enumerate() {
        if set.is_empty() {
            continue;
        }
        let mut best: Option<usize> = None;
        for (j, other) in sets.iter().enumerate() {
            let larger = other.len() > set.len() || (other.len() == set.len() && inside(i, j));
            if i == j || !larger || !set.iter().all(|id| other.contains(id)) {
                continue;
            }
            let closer = best.is_none_or(|b| {
                other.len() < sets[b].len() || (other.len() == sets[b].len() && inside(j, b))
            });
            if closer {
                best = Some(j);
            }
        }
        parent[i] = best;
    }
    parent
}

/// The subgraph each node sits in: the deepest that lists it.
fn listings(graph: &Graph, parent: &[Option<usize>]) -> HashMap<String, usize> {
    let depth = |mut index: usize| {
        let mut depth = 0;
        while let Some(up) = parent[index] {
            depth += 1;
            index = up;
            if depth > parent.len() {
                break;
            }
        }
        depth
    };
    let depths: Vec<usize> = (0..graph.subgraphs.len()).map(depth).collect();
    let mut best: HashMap<String, usize> = HashMap::new();
    for (index, sub) in graph.subgraphs.iter().enumerate() {
        for id in &sub.nodes {
            if let Some(&prev) = best.get(id)
                && depths[prev] >= depths[index]
            {
                continue;
            }
            best.insert(id.clone(), index);
        }
    }
    best
}

/// Which way each subgraph runs: in a flowchart its own `direction`, or
/// the way the one around it runs; in a state diagram the diagram's.
fn flows(graph: &Graph, parent: &[Option<usize>], flowchart: bool) -> Vec<Flow> {
    let mut flows = vec![graph.direction; graph.subgraphs.len()];
    if !flowchart {
        return flows;
    }
    // A parent opens before its children, so it is settled first.
    for index in 0..flows.len() {
        let around = parent[index]
            .filter(|&up| up < index)
            .map_or(graph.direction, |up| flows[up]);
        flows[index] = graph.subgraphs[index].direction.unwrap_or(around);
    }
    flows
}

impl Tree<'_> {
    /// Records the path of every state below `spot` and the container at
    /// the end of every path.
    fn walk(&mut self, spot: Spot, path: &mut Vec<usize>) {
        self.spots.insert(path.clone(), spot);
        for (index, member) in self.members(spot).into_iter().enumerate() {
            path.push(index);
            match member {
                Member::Leaf(id) => {
                    self.paths.insert(id, path.clone());
                }
                Member::Group(group) => {
                    if let Some(id) = &self.graph.subgraphs[group].id {
                        self.paths.insert(id.clone(), path.clone());
                    }
                    self.walk(Spot::In(group), path);
                }
            }
            path.pop();
        }
    }

    fn home(&self, id: &str) -> Spot {
        if let Some(&index) = self.composite_of.get(id) {
            return match self.parent[index] {
                Some(parent) => Spot::In(parent),
                None => Spot::Root,
            };
        }
        match self.listing.get(id) {
            Some(&index) => Spot::In(index),
            None => Spot::Root,
        }
    }

    /// The id of the state that holds `spot`; regions are passed by.
    fn owner(&self, spot: Spot) -> Option<String> {
        match spot {
            Spot::Root => None,
            Spot::In(index) if self.is_region(index) => self.owner(match self.parent[index] {
                Some(parent) => Spot::In(parent),
                None => Spot::Root,
            }),
            Spot::In(index) => self.graph.subgraphs[index].id.clone(),
        }
    }

    fn flow(&self, spot: Spot) -> Flow {
        match spot {
            Spot::Root => self.graph.direction,
            Spot::In(index) => self.flows[index],
        }
    }

    fn is_region(&self, index: usize) -> bool {
        !self.flowchart && nest::is_region(self.graph, index)
    }
    fn members(&self, spot: Spot) -> Vec<Member> {
        let mut members = Vec::new();
        match spot {
            Spot::Root => {
                for id in self.graph.nodes.keys() {
                    if !self.composite_of.contains_key(id) && self.home(id) == Spot::Root {
                        members.push(Member::Leaf(id.clone()));
                    }
                }
                for index in 0..self.parent.len() {
                    if self.parent[index].is_none() {
                        members.push(Member::Group(index));
                    }
                }
            }
            Spot::In(container) => {
                for id in &self.graph.subgraphs[container].nodes {
                    let seen = members
                        .iter()
                        .any(|member| matches!(member, Member::Leaf(leaf) if leaf == id));
                    if !seen && !self.composite_of.contains_key(id) && self.home(id) == spot {
                        members.push(Member::Leaf(id.clone()));
                    }
                }
                for (index, parent) in self.parent.iter().enumerate() {
                    if *parent == Some(container) {
                        members.push(Member::Group(index));
                    }
                }
            }
        }
        members.sort_by_key(|member| self.order(member));
        members
    }

    fn order(&self, member: &Member) -> (usize, usize) {
        match member {
            Member::Leaf(id) => (
                self.graph.node_order.get(id).copied().unwrap_or(usize::MAX),
                0,
            ),
            Member::Group(index) => (self.group_order(*index), index + 1),
        }
    }

    fn group_order(&self, index: usize) -> usize {
        let sub = &self.graph.subgraphs[index];
        if let Some(id) = &sub.id
            && let Some(order) = self.graph.node_order.get(id)
        {
            return *order;
        }
        sub.nodes
            .iter()
            .filter_map(|id| self.graph.node_order.get(id))
            .copied()
            .min()
            .unwrap_or(usize::MAX)
    }
}

fn slot(members: &[Member], id: &str, graph: &Graph) -> Option<usize> {
    members.iter().position(|member| match member {
        Member::Leaf(leaf) => leaf == id,
        Member::Group(index) => graph.subgraphs[*index].id.as_deref() == Some(id),
    })
}

#[derive(Clone, Copy)]
enum Flip {
    No,
    X,
    Y,
}

fn axes(flow: Flow) -> (graph::Direction, Flip) {
    match flow {
        Flow::TopDown => (graph::Direction::Down, Flip::No),
        Flow::BottomTop => (graph::Direction::Down, Flip::Y),
        Flow::LeftRight => (graph::Direction::Right, Flip::No),
        Flow::RightLeft => (graph::Direction::Right, Flip::X),
    }
}

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
    mark: Color,
    title: Block,
    description: Vec<Block>,
    header_h: f32,
    nested: bool,
    /// Laid out with the container around it, so its members and lines
    /// are that container's too.
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
            nested: item.nested,
            opened: false,
            children: item.children,
        }
    }
}

struct PlacedEdge {
    from: usize,
    to: usize,
    /// The edge of mmdr's graph it draws.
    edge: usize,
    points: Vec<(f32, f32)>,
    label: Block,
    label_at: (f32, f32),
    stroke: Stroke,
    /// How the line starts and ends.
    caps: (Cap, Cap),
}

/// What a line ends in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cap {
    None,
    Arrow,
    Circle,
    Cross,
}

fn caps(edge: &Edge) -> (Cap, Cap) {
    let cap = |arrow: bool, decoration: Option<EdgeDecoration>| match decoration {
        Some(EdgeDecoration::Circle) => Cap::Circle,
        Some(EdgeDecoration::Cross) => Cap::Cross,
        _ if arrow => Cap::Arrow,
        _ => Cap::None,
    };
    (
        cap(edge.arrow_start, edge.start_decoration),
        cap(edge.arrow_end, edge.end_decoration),
    )
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
    nested: bool,
    children: Scene,
}

fn layout_spot(tree: &Tree, spot: Spot, palette: &Palette, family: Family) -> Option<Scene> {
    let members = tree.members(spot);
    if members.is_empty() {
        let stray = tree.meets.iter().any(|meet| meet.spot == spot)
            || tree
                .graph
                .state_notes
                .iter()
                .any(|note| tree.home(&note.target) == spot);
        return if stray { None } else { Some(Scene::empty()) };
    }
    let (axis, _) = axes(tree.flow(spot));
    let regions = members
        .iter()
        .filter(|member| matches!(member, Member::Group(index) if tree.is_region(*index)))
        .count();
    if regions > 0 {
        if regions < members.len() {
            return None;
        }
        return nest::layout_regions(tree, &members, axis, palette, family);
    }
    open::layout_open(tree, spot, palette, family)
}

fn measure_member(
    tree: &Tree,
    member: &Member,
    axis: graph::Direction,
    palette: &Palette,
    family: Family,
) -> Option<Measured> {
    match member {
        Member::Group(index) => measure_group(tree, *index, palette, family, false),
        Member::Leaf(id) => measure_leaf(tree, id, axis, palette, family),
    }
}

/// A group the layout around it opens holds no layout of its own; it is
/// measured as if it held something.
fn measure_group(
    tree: &Tree,
    index: usize,
    palette: &Palette,
    family: Family,
    open: bool,
) -> Option<Measured> {
    let children = if open {
        Scene::empty()
    } else {
        layout_spot(tree, Spot::In(index), palette, family)?
    };
    let sub = &tree.graph.subgraphs[index];
    let id = sub.id.as_deref().unwrap_or("");
    let style = styled_node(tree.graph, id, tree.flowchart);
    let look = look_for(
        &style,
        palette.cluster_fill,
        palette.cluster_border,
        palette.text,
    )?;
    let (kind, title, description) = if tree.flowchart {
        let font = Font::new(palette.size, look.text);
        let title = label::measure(&sub.label, font, family, None);
        (Kind::Cluster, title, Vec::new())
    } else {
        let (title, description) = measure_label(
            &state_label(tree.graph, index),
            Font::new(palette.size, look.text).bold(),
            Font::new((palette.size - 2.0).max(4.0), look.text),
            family,
        );
        (Kind::State, title, description)
    };
    let header_h = stack_height(&title, &description);
    let header_w = stack_width(&title, &description);
    let gap = if !open && children.width == 0.0 && children.height == 0.0 {
        0.0
    } else {
        COMP_GAP
    };
    Some(Measured {
        width: header_w.max(children.width) + COMP_PAD * 2.0,
        height: COMP_PAD + header_h + gap + children.height + COMP_PAD,
        kind,
        fill: look.fill,
        border: look.border,
        mark: look.mark,
        title,
        description,
        header_h,
        gap,
        nested: true,
        children,
    })
}

/// A nested state's name: the label its state was given, else the
/// subgraph's.
fn state_label(graph: &Graph, index: usize) -> String {
    let sub = &graph.subgraphs[index];
    if let Some(id) = &sub.id
        && let Some(node) = graph.nodes.get(id)
        && !node.label.is_empty()
    {
        return node.label.clone();
    }
    sub.label.clone()
}

fn measure_leaf(
    tree: &Tree,
    id: &str,
    axis: graph::Direction,
    palette: &Palette,
    family: Family,
) -> Option<Measured> {
    let node = tree.graph.nodes.get(id)?;
    let kind = *tree.kinds.get(id)?;
    let style = styled_node(tree.graph, id, tree.flowchart);
    let look = look_for(&style, palette.fill, palette.border, palette.text)?;
    let (title, description, width, height) = match kind {
        Kind::State => {
            let (title, description) = measure_label(
                &node.label,
                Font::new(palette.size, look.text).bold(),
                Font::new((palette.size - 2.0).max(4.0), look.text),
                family,
            );
            let width = (stack_width(&title, &description) + PAD_X * 2.0).max(MIN_W);
            let height = (stack_height(&title, &description) + PAD_Y * 2.0).max(MIN_H);
            (title, description, width, height)
        }
        Kind::Choice => {
            let (title, description) = measure_label(
                &node.label,
                Font::new(palette.size, look.text),
                Font::new((palette.size - 2.0).max(4.0), look.text),
                family,
            );
            let width = (stack_width(&title, &description) * 2.0 + 16.0).max(CHOICE_MIN);
            let height = (stack_height(&title, &description) * 2.0 + 16.0).max(CHOICE_MIN);
            (title, description, width, height)
        }
        Kind::Node(shape) => {
            let font = Font::new(palette.size, look.text);
            let title = label::measure(&node.label, font, family, Some(wrap(palette)));
            let (width, height) = shape.size(title.width, title.height);
            (title, Vec::new(), width, height)
        }
        Kind::Bar => {
            let (width, height) = match axis {
                graph::Direction::Down => (BAR_LONG, BAR_SHORT),
                graph::Direction::Right => (BAR_SHORT, BAR_LONG),
            };
            (Block::empty(), Vec::new(), width, height)
        }
        Kind::Start => (Block::empty(), Vec::new(), START, START),
        Kind::End => (Block::empty(), Vec::new(), END, END),
        Kind::Region | Kind::Cluster => return None,
    };
    Some(Measured {
        width,
        height,
        kind,
        fill: look.fill,
        border: look.border,
        mark: look.mark,
        title,
        description,
        header_h: 0.0,
        gap: 0.0,
        nested: false,
        children: Scene::empty(),
    })
}

/// Where a flowchart label wraps, in proportion to the font.
fn wrap(palette: &Palette) -> f32 {
    WRAP * palette.size / 16.0
}

/// A state's title, and the lines of its description below `---`.
fn measure_label(
    raw: &str,
    title_font: Font,
    desc_font: Font,
    family: Family,
) -> (Block, Vec<Block>) {
    let (title, rest) = raw.split_once("\n---\n").unwrap_or((raw, ""));
    let title = label::measure(title, title_font, family, None);
    let description = rest
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| label::measure(line, desc_font, family, None))
        .collect();
    (title, description)
}

/// An edge's label; a flowchart's wraps as its nodes' do.
fn measure_edge(tree: &Tree, label: &Option<String>, font: Font, family: Family) -> Block {
    match label {
        Some(text) => {
            let wrap = tree.flowchart.then(|| WRAP * font.size / 16.0);
            label::measure(text, font, family, wrap)
        }
        None => Block::empty(),
    }
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

/// Carries every line on to the outline of a round or slanted box it
/// meets.
fn clip_ends(scene: &mut Scene) {
    let Scene { nodes, edges, .. } = scene;
    for edge in edges {
        if let Some(outline) = nodes.get(edge.from).and_then(|n| n.kind.outline(n.rect)) {
            shape::clip(&mut edge.points, &outline, true);
        }
        if let Some(outline) = nodes.get(edge.to).and_then(|n| n.kind.outline(n.rect)) {
            shape::clip(&mut edge.points, &outline, false);
        }
    }
}
