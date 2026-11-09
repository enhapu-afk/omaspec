//! A map laid out by its arrows (`docs/adr/0026-the-map-is-made.md`).
//!
//! The cards and the pills of the arrows are the boxes of a layered
//! layout, the folders its containers ([`crate::graph::open`]): a diagram
//! stands above the one it plays against, opens or includes, and a folder
//! a line runs into is a frame around its cards there. What no line joins
//! is not ranked together, which would make a row of it: each group of
//! joined boxes is laid out on its own, and the groups, the folders no
//! line runs into and the cards no arrow reaches are packed beside and
//! below one another ([`shelves`]). In a folder a line runs into, what
//! that line does not reach is packed the same and stands at the right
//! inside its frame, for a box without lines has no place of its own in a
//! layered layout.
//!
//! Cards that lead to the same diagrams and are led to by the same are
//! alike; more than [`FAN`] of them stand in a block with one line, so
//! twenty scenarios of one machine are no row either. The cards no arrow
//! reaches are such a block too, sorted by their kind.

use std::collections::HashMap;
use std::convert::Infallible;

use super::arrows::{self, Arrow, CHIP_H, chip_width};
use super::pack::{FAN, GAP, Scene, block, shelves};
use super::{
    Branch, HEADING, Icon, Item, LABEL, Layout, MARGIN, PAD, Placed, RelationKind, measure, width,
};
use crate::graph::open::{self, Line, Measured, Nested};
use crate::graph::route::Point;
use crate::graph::{Direction, Frame, Name, Rect};
use crate::text::{Block, Family, Font};
use crate::tree::Tiles;

/// A folder's frame: the room above its cards, with its name, and around
/// them.
pub(super) const FRAME_BAND: f32 = 40.0;
pub(super) const FRAME_PAD: f32 = 20.0;

/// A folder: the cards in it and the folders in it. The first is the map
/// itself.
#[derive(Debug, Default)]
struct Folder {
    name: String,
    parent: Option<usize>,
    depth: usize,
    cards: Vec<usize>,
    folders: Vec<usize>,
    /// Its cards as the layout's boxes.
    nodes: Vec<usize>,
}

/// A box of the layout: a card, or a block of cards that are alike.
#[derive(Debug)]
struct Node {
    folder: usize,
    scene: Scene,
}

/// The arrows that are one: from these boxes to those, through one pill.
#[derive(Debug)]
struct Net {
    from: Vec<usize>,
    to: Vec<usize>,
    kinds: Vec<RelationKind>,
    /// The innermost folder that holds all of its ends.
    home: usize,
}

/// A member of a container of the layout.
#[derive(Debug, Clone, Copy)]
enum Unit {
    Node(usize),
    /// The pill of a net.
    Pill(usize),
    /// A folder a line runs into: a frame around its own container.
    Folder(usize),
}

/// A line of a net, by the steps from its container down to each end.
#[derive(Debug)]
struct Wire {
    net: usize,
    from: Vec<usize>,
    to: Vec<usize>,
    head: bool,
}

/// What is laid out together.
#[derive(Debug, Default)]
struct Container {
    units: Vec<Unit>,
    wires: Vec<Wire>,
}

/// How a line is drawn: its net, and whether it ends in the head.
type Look = (usize, bool);

struct Plan {
    folders: Vec<Folder>,
    nodes: Vec<Node>,
    nets: Vec<Net>,
    /// How far up the nets with an end at a node, and in a folder, reach:
    /// the least depth of their homes.
    node_top: Vec<usize>,
    folder_top: Vec<usize>,
    containers: Vec<Container>,
    /// The container of a folder a line runs into.
    live: Vec<Option<usize>>,
    /// Where each node and each such folder is a member: the container
    /// and its index there.
    node_at: Vec<Option<(usize, usize)>>,
    folder_at: Vec<Option<(usize, usize)>>,
    /// What of such a folder no line from outside reaches, laid out.
    rest: Vec<Option<Scene>>,
    /// How wide each folder's name is.
    names: Vec<f32>,
}

/// The cards of `boxes` laid out with their arrows, under the heading.
pub(super) fn lay_out(boxes: &[Branch], tiles: &Tiles, family: Family) -> Layout {
    // The folders and their cards.
    fn gather(
        boxes: &[Branch],
        folder: usize,
        folders: &mut Vec<Folder>,
        items: &mut Vec<Item>,
        tiles: &Tiles,
        family: Family,
    ) {
        for branch in boxes {
            let into = match &branch.key {
                Some(key) => {
                    folders[folder].cards.push(items.len());
                    items.push(measure(&branch.words, key, tiles, family));
                    folder
                }
                None => {
                    let at = folders.len();
                    folders.push(Folder {
                        name: branch.words.replace('\n', " "),
                        parent: Some(folder),
                        depth: folders[folder].depth + 1,
                        ..Folder::default()
                    });
                    folders[folder].folders.push(at);
                    at
                }
            };
            gather(&branch.below, into, folders, items, tiles, family);
        }
    }
    let mut folders = vec![Folder::default()];
    let mut items = Vec::new();
    gather(boxes, 0, &mut folders, &mut items, tiles, family);
    let names = folders
        .iter()
        .map(|f| {
            width(
                &f.name,
                Font::new(LABEL, crate::Color::BLACK).bold(),
                family,
            )
        })
        .collect();

    // One arrow for a pair of cards, naming each relation.
    let by_key: HashMap<&str, usize> = items
        .iter()
        .enumerate()
        .map(|(i, item)| (item.key.as_str(), i))
        .collect();
    let mut pairs: Vec<(usize, usize, Vec<RelationKind>)> = Vec::new();
    let mut seen: HashMap<(usize, usize), usize> = HashMap::new();
    for r in tiles.relations {
        let (Some(&from), Some(&to)) = (by_key.get(r.from.as_str()), by_key.get(r.to.as_str()))
        else {
            continue;
        };
        if from == to {
            continue;
        }
        let at = *seen.entry((from, to)).or_insert_with(|| {
            pairs.push((from, to, Vec::new()));
            pairs.len() - 1
        });
        if !pairs[at].2.contains(&r.kind) {
            pairs[at].2.push(r.kind);
            pairs[at].2.sort();
        }
    }

    let mut plan = Plan {
        node_top: Vec::new(),
        folder_top: vec![usize::MAX; folders.len()],
        live: vec![None; folders.len()],
        folder_at: vec![None; folders.len()],
        node_at: Vec::new(),
        containers: Vec::new(),
        rest: vec![None; folders.len()],
        nodes: Vec::new(),
        nets: Vec::new(),
        folders,
        names,
    };
    let node_of = plan.boxes(&items, &pairs);
    plan.join(&pairs, &node_of);
    let all = plan.build(0);

    let heading = tiles.heading.cloned();
    let top = MARGIN + if heading.is_some() { HEADING } else { 0.0 };
    let mut laid = Scene::default();
    laid.put(&all, MARGIN, top);
    // In the order the map's maker gave them, wherever each one stands.
    laid.cards.sort_by_key(|&(item, _)| item);
    let heading_width = heading.as_ref().map_or(0.0, |h| {
        let ink = crate::Color::BLACK;
        width(&h.title, Font::new(22.0, ink).bold(), family).max(width(
            &h.summary,
            Font::new(12.5, ink),
            family,
        ))
    });
    Layout {
        cards: laid
            .cards
            .iter()
            .map(|&(i, rect)| {
                let item = &items[i];
                Placed {
                    text: item.text.clone(),
                    key: item.key.clone(),
                    card: item.card.clone(),
                    rect,
                    picture: Rect {
                        x: rect.x + (rect.w - item.picture.0) / 2.0,
                        y: rect.y + PAD,
                        w: item.picture.0,
                        h: item.picture.1,
                    },
                }
            })
            .collect(),
        frames: laid
            .frames
            .iter()
            .map(|&(g, rect)| (plan.folders[g].name.clone(), rect))
            .collect(),
        groups: laid.groups,
        arrows: laid.arrows,
        size: (
            all.size.0.max(heading_width) + 2.0 * MARGIN,
            top + all.size.1 + MARGIN,
        ),
        heading,
    }
}

/// The order of the kinds in a block of cards no arrow reaches: the
/// folders first.
fn kind_rank(item: &Item) -> u8 {
    match item.card.as_ref().map(|c| c.icon) {
        Some(Icon::Map) => 0,
        Some(Icon::State) => 1,
        Some(Icon::Scenario) => 2,
        Some(Icon::Activity) => 3,
        Some(Icon::Structure) => 4,
        Some(Icon::Other) => 5,
        None => 6,
    }
}

/// The set of `x` among `sets`.
fn find(sets: &mut [usize], mut x: usize) -> usize {
    while sets[x] != x {
        sets[x] = sets[sets[x]];
        x = sets[x];
    }
    x
}

impl Plan {
    /// The boxes of the layout: each card, or a block of the cards of one
    /// folder that are alike. Says which box each card is in.
    fn boxes(&mut self, items: &[Item], pairs: &[(usize, usize, Vec<RelationKind>)]) -> Vec<usize> {
        // What a card leads to and what leads to it.
        type Ends = Vec<(usize, Vec<RelationKind>)>;
        let mut alike: Vec<(Ends, Ends)> = vec![Default::default(); items.len()];
        for (from, to, kinds) in pairs {
            alike[*from].0.push((*to, kinds.clone()));
            alike[*to].1.push((*from, kinds.clone()));
        }
        for (out, into) in &mut alike {
            out.sort();
            into.sort();
        }
        let mut node_of = vec![0; items.len()];
        for folder in 0..self.folders.len() {
            let mut classes: Vec<Vec<usize>> = Vec::new();
            let mut class_of: HashMap<&(Ends, Ends), usize> = HashMap::new();
            for &card in &self.folders[folder].cards {
                let at = *class_of.entry(&alike[card]).or_insert_with(|| {
                    classes.push(Vec::new());
                    classes.len() - 1
                });
                classes[at].push(card);
            }
            // In the order of their first cards.
            let mut made: Vec<(usize, Vec<usize>, bool)> = Vec::new();
            for mut class in classes {
                let joined = !alike[class[0]].0.is_empty() || !alike[class[0]].1.is_empty();
                let first = class[0];
                if !joined {
                    class.sort_by_key(|&card| kind_rank(&items[card]));
                    made.push((first, class, false));
                } else if class.len() > FAN {
                    made.push((first, class, true));
                } else {
                    made.extend(class.into_iter().map(|card| (card, vec![card], false)));
                }
            }
            made.sort_by_key(|(first, ..)| *first);
            for (_, cards, outlined) in made {
                let at = self.nodes.len();
                for &card in &cards {
                    node_of[card] = at;
                }
                self.nodes.push(Node {
                    folder,
                    scene: block(items, &cards, outlined),
                });
                self.folders[folder].nodes.push(at);
            }
        }
        self.node_top = vec![usize::MAX; self.nodes.len()];
        self.node_at = vec![None; self.nodes.len()];
        node_of
    }

    /// The nets of the arrows between the boxes: those that arrive alike
    /// at one box, and those that leave one box alike for boxes nothing
    /// else leads to that way.
    fn join(&mut self, pairs: &[(usize, usize, Vec<RelationKind>)], node_of: &[usize]) {
        let mut arriving: Vec<(usize, Vec<RelationKind>, Vec<usize>)> = Vec::new();
        let mut at: HashMap<(usize, Vec<RelationKind>), usize> = HashMap::new();
        for (from, to, kinds) in pairs {
            let (from, to) = (node_of[*from], node_of[*to]);
            if from == to {
                continue;
            }
            let net = *at.entry((to, kinds.clone())).or_insert_with(|| {
                arriving.push((to, kinds.clone(), Vec::new()));
                arriving.len() - 1
            });
            if !arriving[net].2.contains(&from) {
                arriving[net].2.push(from);
            }
        }
        let mut leaving: HashMap<(usize, Vec<RelationKind>), usize> = HashMap::new();
        for (to, kinds, from) in arriving {
            if let [only] = from[..] {
                let net = *leaving.entry((only, kinds.clone())).or_insert_with(|| {
                    self.nets.push(Net {
                        from: vec![only],
                        to: Vec::new(),
                        kinds,
                        home: 0,
                    });
                    self.nets.len() - 1
                });
                self.nets[net].to.push(to);
            } else {
                self.nets.push(Net {
                    from,
                    to: vec![to],
                    kinds,
                    home: 0,
                });
            }
        }
        for net in 0..self.nets.len() {
            let ends: Vec<usize> = self.nets[net]
                .from
                .iter()
                .chain(&self.nets[net].to)
                .copied()
                .collect();
            let home = ends
                .iter()
                .map(|&node| self.nodes[node].folder)
                .reduce(|a, b| self.shared(a, b))
                .unwrap_or(0);
            self.nets[net].home = home;
            let reach = self.folders[home].depth;
            for node in ends {
                self.node_top[node] = self.node_top[node].min(reach);
                let mut folder = Some(self.nodes[node].folder);
                while let Some(at) = folder {
                    self.folder_top[at] = self.folder_top[at].min(reach);
                    folder = self.folders[at].parent;
                }
            }
        }
    }

    /// The innermost folder that holds both `a` and `b`.
    fn shared(&self, mut a: usize, mut b: usize) -> usize {
        while a != b {
            if self.folders[a].depth >= self.folders[b].depth {
                a = self.folders[a].parent.unwrap_or(0);
            } else {
                b = self.folders[b].parent.unwrap_or(0);
            }
        }
        a
    }

    /// Lays out what `folder` holds. A folder a line runs into from
    /// outside gets a container with what that line reaches, for the
    /// layout around it to open, and keeps the rest of it for the side of
    /// its frame; of any other, all it holds comes back.
    fn build(&mut self, folder: usize) -> Scene {
        let inside: Vec<usize> = self.folders[folder].folders.clone();
        let mut held: HashMap<usize, Scene> = HashMap::new();
        for &sub in &inside {
            let scene = self.build(sub);
            held.insert(sub, scene);
        }
        let depth = self.folders[folder].depth;
        let mut units: Vec<Unit> = self.folders[folder]
            .nodes
            .iter()
            .map(|&n| Unit::Node(n))
            .collect();
        let mut unit_of_node: HashMap<usize, usize> = HashMap::new();
        for (at, unit) in units.iter().enumerate() {
            if let Unit::Node(n) = unit {
                unit_of_node.insert(*n, at);
            }
        }
        let mut unit_of_folder: HashMap<usize, usize> = HashMap::new();
        for &sub in &inside {
            unit_of_folder.insert(sub, units.len());
            units.push(Unit::Folder(sub));
        }
        let nets: Vec<usize> = (0..self.nets.len())
            .filter(|&net| self.nets[net].home == folder)
            .collect();
        let first_pill = units.len();
        units.extend(nets.iter().map(|&net| Unit::Pill(net)));

        // What the nets of this folder join, and what a line from outside
        // reaches.
        let mut sets: Vec<usize> = (0..units.len()).collect();
        for (at, &net) in nets.iter().enumerate() {
            let pill = first_pill + at;
            for &end in self.nets[net].from.iter().chain(&self.nets[net].to) {
                let mut at = self.nodes[end].folder;
                let unit = if at == folder {
                    unit_of_node[&end]
                } else {
                    while self.folders[at].parent != Some(folder) {
                        at = self.folders[at].parent.unwrap_or(folder);
                    }
                    unit_of_folder[&at]
                };
                let (a, b) = (find(&mut sets, pill), find(&mut sets, unit));
                sets[a] = b;
            }
        }
        let mut outside: Option<usize> = None;
        for (at, unit) in units.iter().enumerate() {
            let reached = match unit {
                Unit::Node(n) => self.node_top[*n] < depth,
                Unit::Folder(sub) => self.folder_top[*sub] < depth,
                _ => false,
            };
            if reached {
                let a = find(&mut sets, at);
                match outside {
                    Some(other) => {
                        let b = find(&mut sets, other);
                        sets[a] = b;
                    }
                    None => outside = Some(a),
                }
            }
        }
        let outside = outside.map(|at| find(&mut sets, at));
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut group_of: HashMap<usize, usize> = HashMap::new();
        for at in 0..units.len() {
            let set = find(&mut sets, at);
            let group = *group_of.entry(set).or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
            groups[group].push(at);
        }

        // What is joined first, then the cards, then the folders.
        let mut packed: Vec<(u8, Scene)> = Vec::new();
        let mut reached: Vec<Unit> = Vec::new();
        for group in groups {
            if Some(find(&mut sets, group[0])) == outside {
                reached = group.iter().map(|&at| units[at]).collect();
                continue;
            }
            match group[..] {
                [only] => match units[only] {
                    Unit::Node(n) => packed.push((1, self.nodes[n].scene.clone())),
                    Unit::Folder(sub) => {
                        let scene = held.remove(&sub).unwrap_or_default();
                        if !scene.cards.is_empty() {
                            packed.push((2, self.framed(sub, &scene)));
                        }
                    }
                    Unit::Pill(_) => {}
                },
                _ => {
                    let members = group.iter().map(|&at| units[at]).collect();
                    let container = self.contain(members);
                    packed.push((0, self.laid(container)));
                }
            }
        }
        packed.sort_by_key(|(rank, _)| *rank);
        let packed: Vec<Scene> = packed.into_iter().map(|(_, scene)| scene).collect();
        let rest = shelves(&packed);
        if outside.is_none() {
            return rest;
        }
        if !rest.cards.is_empty() {
            self.rest[folder] = Some(rest);
        }
        self.live[folder] = Some(self.contain(reached));
        Scene::default()
    }

    /// A container of `units`, with the lines of the pills among them.
    fn contain(&mut self, units: Vec<Unit>) -> usize {
        let container = self.containers.len();
        for (at, unit) in units.iter().enumerate() {
            match unit {
                Unit::Node(n) => self.node_at[*n] = Some((container, at)),
                Unit::Folder(sub) => self.folder_at[*sub] = Some((container, at)),
                Unit::Pill(_) => {}
            }
        }
        let mut wires = Vec::new();
        for (pill, unit) in units.iter().enumerate() {
            let Unit::Pill(net) = *unit else {
                continue;
            };
            for &from in &self.nets[net].from {
                if let Some(from) = self.path(container, from) {
                    wires.push(Wire {
                        net,
                        from,
                        to: vec![pill],
                        head: false,
                    });
                }
            }
            for &to in &self.nets[net].to {
                if let Some(to) = self.path(container, to) {
                    wires.push(Wire {
                        net,
                        from: vec![pill],
                        to,
                        head: true,
                    });
                }
            }
        }
        self.containers.push(Container { units, wires });
        container
    }

    /// The steps from `container` down to `node`, through the folders
    /// around it.
    fn path(&self, container: usize, node: usize) -> Option<Vec<usize>> {
        let (mut at, index) = self.node_at[node]?;
        let mut path = vec![index];
        let mut folder = self.nodes[node].folder;
        while at != container {
            let (above, index) = self.folder_at[folder]?;
            path.push(index);
            at = above;
            folder = self.folders[folder].parent?;
        }
        path.reverse();
        Some(path)
    }

    /// The frame of `folder` around `inside`, all it holds.
    fn framed(&self, folder: usize, inside: &Scene) -> Scene {
        let w = (inside.size.0).max(self.names[folder]) + 2.0 * FRAME_PAD;
        let h = inside.size.1 + FRAME_BAND + FRAME_PAD;
        let mut scene = Scene {
            size: (w, h),
            frames: vec![(
                folder,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w,
                    h,
                },
            )],
            ..Scene::default()
        };
        scene.put(inside, FRAME_PAD, FRAME_BAND);
        scene
    }

    /// `container` laid out, with every folder it opens.
    fn laid(&self, container: usize) -> Scene {
        // A frame is as high as the boxes in it. Where the rest of its
        // folder, at its side, is higher, it is laid out again with the
        // room that takes below its boxes.
        let mut grown: HashMap<usize, f32> = HashMap::new();
        let Ok(mut placed) = open::layout(&Open(self, &grown), container);
        self.short(&placed, &mut grown);
        if !grown.is_empty() {
            let Ok(again) = open::layout(&Open(self, &grown), container);
            placed = again;
        }
        let mut scene = Scene {
            size: (placed.width, placed.height),
            ..Scene::default()
        };
        let mut pills: HashMap<usize, Rect> = HashMap::new();
        let mut lines: Vec<(Look, Vec<Point>)> = Vec::new();
        self.flatten(&placed, &mut scene, &mut pills, &mut lines);
        let mut nets: Vec<usize> = pills.keys().copied().collect();
        nets.sort_unstable();
        for net in nets {
            let pill = pills[&net];
            let of = |head: bool| -> Vec<Vec<Point>> {
                lines
                    .iter()
                    .filter(|((n, h), _)| *n == net && *h == head)
                    .map(|(_, points)| points.clone())
                    .collect()
            };
            let mut heads = of(true);
            arrows::leave_as_one(pill, &mut heads);
            let kinds = self.nets[net].kinds.clone();
            scene.arrows.push(Arrow {
                words: arrows::words(&kinds),
                kinds,
                lines: of(false)
                    .into_iter()
                    .map(|line| (line, false))
                    .chain(heads.into_iter().map(|line| (line, true)))
                    .collect(),
                chip: (pill.x + pill.w / 2.0, pill.y + pill.h / 2.0),
            });
        }
        scene
    }

    /// By how much each frame in `placed` is lower than the rest of its
    /// folder needs.
    fn short(&self, placed: &open::Scene<Unit, Look>, grown: &mut HashMap<usize, f32>) {
        for node in &placed.nodes {
            let Unit::Folder(folder) = node.item else {
                continue;
            };
            if let Some(rest) = &self.rest[folder] {
                let room = node.rect.h - FRAME_BAND - FRAME_PAD;
                if rest.size.1 > room + 0.5 {
                    grown.insert(folder, rest.size.1 - room);
                }
            }
            self.short(&node.children, grown);
        }
    }

    fn flatten(
        &self,
        placed: &open::Scene<Unit, Look>,
        scene: &mut Scene,
        pills: &mut HashMap<usize, Rect>,
        lines: &mut Vec<(Look, Vec<Point>)>,
    ) {
        for node in &placed.nodes {
            let r = node.rect;
            match node.item {
                Unit::Node(n) => scene.put(&self.nodes[n].scene, r.x, r.y),
                Unit::Pill(net) => {
                    pills.insert(net, r);
                }
                Unit::Folder(folder) => {
                    scene.frames.push((folder, r));
                    if let Some(rest) = &self.rest[folder] {
                        let x = r.x + r.w - FRAME_PAD - rest.size.0;
                        scene.put(rest, x, r.y + FRAME_BAND);
                    }
                    self.flatten(&node.children, scene, pills, lines);
                }
            }
        }
        for wire in &placed.edges {
            if wire.points.len() > 1 {
                lines.push((wire.look, wire.points.clone()));
            }
        }
    }
}

/// The containers of a plan, as the layout asks for them, and by how
/// much each folder's frame reaches further down than its boxes.
struct Open<'a>(&'a Plan, &'a HashMap<usize, f32>);

impl Nested for Open<'_> {
    type Container = usize;
    type Item = Unit;
    type Look = Look;
    type Error = Infallible;

    fn members(&self, container: usize) -> usize {
        self.0.containers[container].units.len()
    }

    fn inner(&self, container: usize, index: usize) -> Option<usize> {
        match self.0.containers[container].units.get(index)? {
            Unit::Folder(folder) => self.0.live[*folder],
            _ => None,
        }
    }

    fn flow(&self, _container: usize) -> (Direction, bool) {
        (Direction::Down, false)
    }

    fn lines(&self, container: usize) -> Vec<Line> {
        self.0.containers[container]
            .wires
            .iter()
            .map(|wire| Line {
                from: wire.from.clone(),
                to: wire.to.clone(),
                hidden: false,
            })
            .collect()
    }

    fn measure(
        &self,
        container: usize,
        index: usize,
        _open: bool,
    ) -> Result<Measured<Unit, Look>, Infallible> {
        let unit = self.0.containers[container].units[index];
        let (width, height) = match unit {
            Unit::Node(n) => self.0.nodes[n].scene.size,
            Unit::Pill(net) => (chip_width(&arrows::words(&self.0.nets[net].kinds)), CHIP_H),
            Unit::Folder(_) => (0.0, 0.0),
        };
        Ok(Measured {
            width,
            height,
            flat: true,
            children: open::Scene::empty(),
            item: unit,
        })
    }

    fn frame(&self, _container: usize, item: &Measured<Unit, Look>) -> Frame {
        let Unit::Folder(folder) = item.item else {
            return Frame::default();
        };
        let name = self.0.names[folder];
        let rest = self.0.rest[folder].as_ref();
        Frame {
            parent: None,
            top: FRAME_BAND,
            bottom: FRAME_PAD + self.1.get(&folder).copied().unwrap_or(0.0),
            left: FRAME_PAD,
            right: FRAME_PAD + rest.map_or(0.0, |rest| GAP + rest.size.0),
            min_width: name + 2.0 * FRAME_PAD,
            name: Name {
                share: 0.0,
                offset: FRAME_PAD,
                width: name,
                height: FRAME_BAND,
            },
        }
    }

    fn line(&self, container: usize, index: usize) -> Result<(Block, Look), Infallible> {
        let wire = &self.0.containers[container].wires[index];
        Ok((Block::empty(), (wire.net, wire.head)))
    }

    fn arrival(&self, look: &Look) -> u64 {
        look.0 as u64 * 2 + u64::from(look.1)
    }

    fn band(&self, node: &open::Placed<Unit, Look>) -> Rect {
        Rect {
            h: FRAME_BAND,
            ..node.rect
        }
    }
}
