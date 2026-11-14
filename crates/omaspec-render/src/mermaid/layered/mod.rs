//! Mermaid flowcharts on the shared graph layout. (State diagrams are
//! drawn by the engine PlantUML's are drawn with, [`super::state`].)
//!
//! The graph is mmdr's: mmdr parses the flowchart, and this lays it out
//! and draws it. A subgraph a line runs into from outside is laid out with
//! the container around it ([`open`]); any other lays its own members out,
//! in its own direction, and is placed as one box. Lines into a box laid
//! out on its own are in [`nest`]. Anything this drawer does not know (a
//! shape or an image it cannot draw, a color it cannot read) leaves the
//! diagram to mmdr, which still draws it.

use std::collections::{HashMap, HashSet};

use mermaid_rs_renderer::{DiagramKind, Direction as Flow, Edge, EdgeDecoration, Graph, Theme};
use omaspec_uml::graph::across::{Drawn, meeting};
use omaspec_uml::graph::by_hand::{self, Arrow, Dress, Laid, LineMut};
use omaspec_uml::graph::{self, Rect};
use omaspec_uml::hand::{self, Key};
use omaspec_uml::map::Map;
use omaspec_uml::{Block, Canvas, Family, Font, Link, Paint, Stroke};

use look::{Palette, edge_look, look_for, palette, styled_node};
pub(super) use reading::readable;
use shape::{Kind, Shape};

mod flowchart;
mod fold;
mod label;
mod look;
mod nest;
mod open;
mod paint;
mod reading;
mod shape;

#[cfg(test)]
mod tests;

const MARGIN: f32 = 10.0;
const COMP_PAD: f32 = 12.0;
const COMP_GAP: f32 = 8.0;
const HEAD_LENGTH: f32 = 10.0;
const HEAD_HALF: f32 = 4.0;
/// Where Mermaid wraps a flowchart label, at its default 16 px font.
const WRAP: f32 = 200.0;

/// The SVG and its map: its lines, which `hands` may have drawn by hand
/// (docs/adr/0016), and its boxes, which `held` may hold in place
/// (docs/adr/0021). `None` when this drawer leaves the diagram to mmdr.
/// `source` is what mmdr parsed, for what its graph does not keep. The
/// lines' corners curve by up to `curve`.
pub fn try_draw(
    graph: &Graph,
    theme: &Theme,
    source: &str,
    hands: &[(usize, hand::Line)],
    held: &[(usize, hand::Hold)],
    curve: f32,
) -> Option<(String, Map)> {
    let mut repaired = None;
    let (graph, fix) = flowchart::apply(graph, source, &mut repaired)?;
    // Subgraphs the pane folded (docs/adr/0024), and the entries by hand
    // that name what they hold left out.
    let names = omaspec_uml::fold::read(source);
    let (folded, fix, hidden) = match fold::fold(graph, &fix, &names) {
        Some((graph, fix, hidden)) => (Some(graph), fix, hidden),
        None => (None, fix, HashSet::new()),
    };
    let graph = folded.as_ref().unwrap_or(graph);
    let (hands, held) = (fold::lines(hands, &hidden), fold::holds(held, &hidden));
    let (hands, held) = (hands.as_slice(), held.as_slice());
    let mut tree = prepare(graph, source, &fix)?;
    // Boxes held in place, by the container that holds both.
    let mut unplaced = hold_boxes(&mut tree, held);
    unplaced.extend(part_boxes(&mut tree, &hand::apart(source)));
    let palette = palette(theme, source)?;
    let family = Family::of(&theme.font_family);
    let mut scene = layout_spot(&tree, Spot::Root, &palette, family)?;
    nest::link_crossings(&tree, &mut scene, &palette, family)?;
    nest::link_inward(&tree, &mut scene, &palette, family)?;
    let arrows = arrows(&tree);
    // Lines by hand move no box: one look at where they are does for both.
    let top = nest::placements(&scene.nodes);
    let misses = if hands.is_empty() {
        Vec::new()
    } else {
        let mut laying = Laying {
            tree: &tree,
            scene: &mut scene,
        };
        by_hand::lay(&top, &arrows, &mut laying, hands)
    };
    let mut canvas = Canvas::new(&theme.font_family, false);
    canvas.sketch(super::head::hand_drawn(source));
    paint::paint(&mut canvas, &palette, &scene, curve);
    if let Some((_, title)) = omaspec_uml::front_title(source) {
        paint::title(&mut canvas, &palette, title, family);
    }
    let mut map = Map::default();
    let laying = Laying {
        tree: &tree,
        scene: &mut scene,
    };
    map.wires = by_hand::wires(
        &top,
        &arrows,
        &laying,
        hands,
        |points| omaspec_uml::curve::flatten(points, curve),
        |_, slot, drawn| {
            let Some(edge) = edge_in(laying.scene, slot) else {
                return Dress {
                    stroke: Stroke::new(omaspec_uml::Color::BLACK, 1.0),
                    heads: Vec::new(),
                };
            };
            let (start, end) = edge.look.caps;
            let mut heads = paint::cap_shape(drawn, true, start);
            heads.extend(paint::cap_shape(drawn, false, end));
            Dress {
                stroke: edge.look.stroke,
                heads,
            }
        },
    );
    map.places = places(&tree, &top, held);
    map.misses = misses;
    map.misses
        .extend(hand::said(held, &unplaced, &scene.all_unheld()));
    map.misses.sort();
    let map = map.placed(canvas.fit(MARGIN));
    Some((canvas.finish(MARGIN, Some(palette.background)), map))
}

/// Every box a writer may hold in place, for the map (docs/adr/0021): a
/// node or a subgraph, by its id.
fn places(
    tree: &Tree,
    top: &[omaspec_uml::graph::across::Placement],
    held: &[(usize, hand::Hold)],
) -> Vec<omaspec_uml::map::Place> {
    let names: HashMap<&Vec<usize>, &String> =
        tree.paths.iter().map(|(id, path)| (path, id)).collect();
    let across = |within: &[usize]| {
        let spot = if within.is_empty() {
            Some(Spot::Root)
        } else {
            tree.spots.get(within).copied()
        };
        spot.is_some_and(|spot| axes(tree.flow(spot)).0 == graph::Direction::Right)
    };
    by_hand::places(
        top,
        |path| {
            names
                .get(&path.to_vec())
                .map_or_else(String::new, |id| (*id).clone())
        },
        across,
        held,
    )
}

/// Gives each container of `tree` the boxes of `held` it holds both of,
/// by their ids; returns the entries that name no box, or two in
/// different containers.
fn hold_boxes(tree: &mut Tree, held: &[(usize, hand::Hold)]) -> Vec<(usize, String)> {
    if held.is_empty() {
        return Vec::new();
    }
    let places: HashMap<String, (Vec<usize>, usize)> = tree
        .paths
        .iter()
        .filter_map(|(id, path)| {
            let (&index, container) = path.split_last()?;
            Some((id.clone(), (container.to_vec(), index)))
        })
        .collect();
    let (sorted, misses) = omaspec_uml::graph::hold::sort(held, &places);
    for (path, holds) in sorted {
        let spot = if path.is_empty() {
            Some(Spot::Root)
        } else {
            tree.spots.get(&path).copied()
        };
        if let Some(spot) = spot {
            tree.held.insert(spot, holds);
        }
    }
    misses
}

/// Keeps the lines of `entries` apart. An entry that names no box is
/// returned, to be said.
fn part_boxes(tree: &mut Tree, entries: &[(usize, hand::Apart)]) -> Vec<(usize, String)> {
    if entries.is_empty() {
        return Vec::new();
    }
    let places: HashMap<String, (Vec<usize>, usize)> = tree
        .paths
        .iter()
        .filter_map(|(id, path)| {
            let (&index, container) = path.split_last()?;
            Some((id.clone(), (container.to_vec(), index)))
        })
        .collect();
    let parted = hand::parted(entries, &places);
    tree.bundled = parted.bundled;
    for (path, members) in parted.members {
        let spot = if path.is_empty() {
            Some(Spot::Root)
        } else {
            tree.spots.get(&path).copied()
        };
        if let Some(spot) = spot {
            tree.apart.insert(spot, members);
        }
    }
    parted.misses
}

/// Every line drawn, with what the block of lines by hand calls it: a
/// state diagram's starts and ends are `[*]`, anything else goes by its
/// id. Lines between the same two count in the order of the source; a
/// flowchart's are in that order already. A line that only ranks
/// (`~~~`) or runs from a subgraph into it is left alone.
fn arrows(tree: &Tree) -> Vec<(Arrow, Key)> {
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
        found.push(Arrow {
            path: Vec::new(),
            slot: index,
            from: from.clone(),
            to: to.clone(),
            order: index,
            line: None,
        });
    }
    by_hand::keyed(found, |path| {
        names
            .get(&path.to_vec())
            .map_or_else(String::new, |id| (*id).clone())
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
        .find(|e| e.look.edge == edge)
        .or_else(|| scene.nodes.iter().find_map(|n| edge_in(&n.children, edge)))
}

fn edge_in_mut(scene: &mut Scene, edge: usize) -> Option<&mut PlacedEdge> {
    if let Some(at) = scene.edges.iter().position(|e| e.look.edge == edge) {
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

    fn labels(&self, _path: &[usize], slot: usize) -> Vec<Rect> {
        edge_in(self.scene, slot)
            .and_then(PlacedEdge::label_rect)
            .into_iter()
            .collect()
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
                .and_then(|path| self.scene.placed_at(path));
            if let Some(outline) = end.and_then(|end| end.item.kind.outline(end.rect)) {
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
    /// `click … href`: the address a click on a node opens, read from the
    /// source, as mmdr takes a callback's name for an address.
    links: HashMap<String, Link>,
    /// The boxes held beside others, by the container that holds both.
    held: HashMap<Spot, Vec<omaspec_uml::graph::hold::Held>>,
    /// When false, no line of the diagram bundles
    /// ([0022](../../../../../docs/adr/0022-lines-bundled.md)).
    bundled: bool,
    /// Members whose lines stay out of a bundle, by the container.
    apart: HashMap<Spot, Vec<usize>>,
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
    if graph.kind != DiagramKind::Flowchart {
        return None;
    }
    if graph.subgraphs.iter().any(|sub| sub.icon.is_some()) {
        return None;
    }
    let mut doubles = flowchart::double_circles(source);
    doubles.extend(fix.doubles.iter().cloned());
    let mut kinds = HashMap::with_capacity(graph.nodes.len());
    for (id, node) in &graph.nodes {
        if node.icon.is_some() {
            return None;
        }
        let kind = if fix.bare.contains(id) {
            Kind::Node(Shape::Label)
        } else {
            Kind::Node(Shape::of(node.shape, doubles.contains(id))?)
        };
        kinds.insert(id.clone(), kind);
    }
    if graph.edges.iter().any(edge_unsupported) {
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
    let mut parent = parents(graph);
    // An empty subgraph lists no nodes, so the set walk above cannot nest
    // it. The source's subgraph/end stack can, when it counts the same.
    let nested = flowchart::empty_nesting(source);
    if nested.len() == parent.len() {
        for (index, up) in nested.into_iter().enumerate() {
            if graph.subgraphs[index].nodes.is_empty() {
                parent[index] = up;
            }
        }
    }
    let mut tree = Tree {
        graph,
        listing: listings(graph, &parent),
        flows: flows(graph, &parent),
        parent,
        composite_of,
        kinds,
        paths: HashMap::new(),
        spots: HashMap::new(),
        held: HashMap::new(),
        bundled: true,
        apart: HashMap::new(),
        meets: Vec::new(),
        links: source
            .lines()
            .filter_map(omaspec_uml::click::click)
            .filter_map(|click| Some((click.id, click.link?)))
            .collect(),
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
            // A line between a subgraph and a node inside it.
            let outer = held_by(from, to)?;
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

/// A line may end in a circle or a cross; anything else at an end (a
/// diamond, a label at the end) is not drawn here.
fn edge_unsupported(edge: &Edge) -> bool {
    let end = |decoration: Option<EdgeDecoration>| match decoration {
        None | Some(EdgeDecoration::Circle | EdgeDecoration::Cross) => false,
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
/// its nodes. Two can list the same: the one opened first holds the
/// other.
fn parents(graph: &Graph) -> Vec<Option<usize>> {
    let sets: Vec<HashSet<&str>> = graph
        .subgraphs
        .iter()
        .map(|sub| sub.nodes.iter().map(String::as_str).collect())
        .collect();
    let inside = |inner: usize, outer: usize| inner > outer;
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

/// Which way each subgraph runs: its own `direction`, or the way the one
/// around it runs.
fn flows(graph: &Graph, parent: &[Option<usize>]) -> Vec<Flow> {
    let mut flows = vec![graph.direction; graph.subgraphs.len()];
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

    fn flow(&self, spot: Spot) -> Flow {
        match spot {
            Spot::Root => self.graph.direction,
            Spot::In(index) => self.flows[index],
        }
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
                for (index, parent) in self.parent.iter().enumerate() {
                    if parent.is_none() {
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

/// One container after layout ([`graph::open::Scene`]).
type Scene = graph::open::Scene<Item, Look>;
type Placed = graph::open::Placed<Item, Look>;
type PlacedEdge = graph::open::Wire<Look>;
type Measured = graph::open::Measured<Item, Look>;

/// What a box keeps for painting.
struct Item {
    /// Its id: what the sketch look seeds its strokes with.
    name: String,
    kind: Kind,
    fill: Paint,
    border: Stroke,
    title: Block,
    description: Vec<Block>,
    header_h: f32,
    /// Between the title and what a subgraph holds.
    gap: f32,
    nested: bool,
    /// Followed on a click on the node and its words.
    link: Option<Link>,
}

/// How a line is drawn, and the edge of mmdr's graph it draws.
struct Look {
    edge: usize,
    stroke: Stroke,
    /// How the line starts and ends.
    caps: (Cap, Cap),
}

/// What a line ends in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

fn layout_spot(tree: &Tree, spot: Spot, palette: &Palette, family: Family) -> Option<Scene> {
    let members = tree.members(spot);
    if members.is_empty() {
        let stray = tree.meets.iter().any(|meet| meet.spot == spot);
        return if stray { None } else { Some(Scene::empty()) };
    }
    open::layout_open(tree, spot, palette, family)
}

fn measure_member(
    tree: &Tree,
    member: &Member,
    palette: &Palette,
    family: Family,
) -> Option<Measured> {
    match member {
        Member::Group(index) => measure_group(tree, *index, palette, family, false),
        Member::Leaf(id) => measure_leaf(tree, id, palette, family),
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
    let style = styled_node(tree.graph, id);
    let look = look_for(
        &style,
        palette.cluster_fill,
        palette.cluster_border,
        palette.text,
    )?;
    let font = Font::new(palette.size, look.text);
    let (kind, title, description) = (
        Kind::Cluster,
        label::measure(&sub.label, font, family, None),
        Vec::new(),
    );
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
        flat: kind.flat(),
        children,
        item: Item {
            name: id.to_owned(),
            kind,
            fill: look.fill,
            border: look.border,
            title,
            description,
            header_h,
            gap,
            nested: true,
            link: None,
        },
    })
}

fn measure_leaf(tree: &Tree, id: &str, palette: &Palette, family: Family) -> Option<Measured> {
    let node = tree.graph.nodes.get(id)?;
    let kind = *tree.kinds.get(id)?;
    let style = styled_node(tree.graph, id);
    let (fill, border) = match kind {
        Kind::Node(shape) => palette.of(shape),
        Kind::Cluster => (palette.fill, palette.border),
    };
    let look = look_for(&style, fill, border, palette.text)?;
    let (title, description, width, height) = match kind {
        Kind::Node(shape) => {
            let font = Font::new(palette.size, look.text);
            let title = label::measure(&node.label, font, family, Some(wrap(palette)));
            let (width, height) = shape.size(title.width, title.height);
            (title, Vec::new(), width, height)
        }
        Kind::Cluster => return None,
    };
    Some(Measured {
        width,
        height,
        flat: kind.flat(),
        children: Scene::empty(),
        item: Item {
            name: id.to_owned(),
            kind,
            fill: look.fill,
            border: look.border,
            title,
            description,
            header_h: 0.0,
            gap: 0.0,
            nested: false,
            link: tree.links.get(id).cloned(),
        },
    })
}

/// Where a flowchart label wraps, in proportion to the font.
fn wrap(palette: &Palette) -> f32 {
    WRAP * palette.size / 16.0
}

/// An edge's label, which wraps as the nodes' do.
fn measure_edge(label: &Option<String>, font: Font, family: Family) -> Block {
    match label {
        Some(text) => label::measure(text, font, family, Some(WRAP * font.size / 16.0)),
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
