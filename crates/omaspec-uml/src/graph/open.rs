//! Containers laid out with what is around them (docs/adr/0013), once for
//! every drawer.
//!
//! A container a line runs into from outside is opened: its members take
//! the ranks of the container around it, as Graphviz lays out a cluster,
//! and the line runs through the ranks like any other, in through the side
//! the flow enters by ([`super::framed`]). A container stays a box laid out
//! on its own when no line runs into it, when it has a line from itself to
//! itself, or when its drawer says it cannot open (concurrent regions, a
//! direction of its own); a line into one of those is ranked where both
//! ends meet and left without points, for [`super::across`].
//!
//! The drawer says what its containers hold and how big each box, frame
//! and label is ([`Nested`]); gathering the boxes into one layout,
//! building the nested [`Scene`] from it, mirroring it and placing the
//! labels happens here.

use std::collections::{HashMap, HashSet};

use super::hold::{Held, Hold, Unheld};
use super::{Direction, Edge, Frame, Layout, Node, Rect, framed, route};
use crate::text::Block;

type Point = (f32, f32);

/// One container after layout: its boxes in the order of its members, its
/// lines in the order the drawer gave them, and its notes. `width` and
/// `height` are the graph's, notes excluded, which is what a box around it
/// is sized from. `N` is what the drawer keeps for a box, `E` for a line.
#[derive(Debug)]
pub struct Scene<N, E> {
    pub nodes: Vec<Placed<N, E>>,
    pub edges: Vec<Wire<E>>,
    pub notes: Vec<Note>,
    pub width: f32,
    pub height: f32,
    /// The boxes held ([`Nested::holds`]) that this layout could not keep,
    /// by the line of each entry, and why; the scenes of the boxes laid
    /// out on their own inside hold their own ([`Scene::unheld`]).
    pub unheld: Vec<(usize, Unheld)>,
}

/// A box placed, or a frame around what an opened container holds.
#[derive(Debug)]
pub struct Placed<N, E> {
    pub rect: Rect,
    /// Laid out with the container around it, so its members and lines are
    /// that container's too.
    pub opened: bool,
    pub children: Scene<N, E>,
    pub item: N,
}

/// A line placed. A line into a container laid out on its own has no
/// points until [`super::across`] routes it.
#[derive(Debug)]
pub struct Wire<E> {
    pub points: Vec<Point>,
    pub label: Block,
    pub label_at: Point,
    pub look: E,
}

/// A note placed beside its box.
#[derive(Debug)]
pub struct Note {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub text: Block,
}

/// A box measured, before it is placed: a box of `children` laid out on
/// its own, or none. `flat` when lines may meet it anywhere along a side
/// ([`Node::flat`]).
#[derive(Debug)]
pub struct Measured<N, E> {
    pub width: f32,
    pub height: f32,
    pub flat: bool,
    pub children: Scene<N, E>,
    pub item: N,
}

/// A line of a container, by the steps from it down to each end: one step
/// for a line between two of its members, more for one into a member's
/// container (a crossing). `hidden` ranks the boxes and is not drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub from: Vec<usize>,
    pub to: Vec<usize>,
    pub hidden: bool,
}

impl Line {
    fn direct(&self) -> bool {
        self.from.len() == 1 && self.to.len() == 1
    }
}

/// Room a box keeps across the flow for a note: `Before` it, or `After`
/// it, past its self-loops.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Room {
    Before(f32),
    After(f32),
}

/// What a drawer tells the layout about its containers.
pub trait Nested {
    /// One container: the diagram, a nested state, a subgraph.
    type Container: Copy;
    /// What a box keeps for its drawer.
    type Item;
    /// What a line keeps for its drawer.
    type Look;
    type Error;

    /// How many members `container` has.
    fn members(&self, container: Self::Container) -> usize;
    /// The container member `index` holds, when the layout around may
    /// open it. One with a line from itself to itself never opens.
    fn inner(&self, container: Self::Container, index: usize) -> Option<Self::Container>;
    /// Which way its members run, and whether against that way.
    fn flow(&self, container: Self::Container) -> (Direction, bool);
    /// Its lines, in the order they are drawn.
    fn lines(&self, container: Self::Container) -> Vec<Line>;
    /// Member `index`, measured as a frame (`open`) or as a box.
    fn measure(
        &self,
        container: Self::Container,
        index: usize,
        open: bool,
    ) -> Result<Measured<Self::Item, Self::Look>, Self::Error>;
    /// The frame around an opened member (its `parent` is set here).
    fn frame(&self, container: Self::Container, item: &Measured<Self::Item, Self::Look>) -> Frame;
    /// Line `index` of `container`: its label, and how it is drawn.
    fn line(
        &self,
        container: Self::Container,
        index: usize,
    ) -> Result<(Block, Self::Look), Self::Error>;
    /// The boxes of `container` held beside others
    /// ([0021](../../../../docs/adr/0021-boxes-held-in-place.md)), in the
    /// order of their entries.
    fn holds(&self, _container: Self::Container) -> Vec<Held> {
        Vec::new()
    }
    /// Whether the lines of `container` may share a trunk
    /// ([0022](../../../../docs/adr/0022-lines-bundled.md)).
    fn bundled(&self, _container: Self::Container) -> bool {
        true
    }
    /// Members of `container` whose lines stay out of a bundle.
    fn apart_at(&self, _container: Self::Container) -> Vec<usize> {
        Vec::new()
    }
    /// Line `index` carries a mark at an end (a multiplicity). The mark
    /// sits on the stem, so the line stays out of a trunk.
    fn marked(&self, _container: Self::Container, _index: usize) -> bool {
        false
    }
    /// The side of line `index` its label takes, where the writer chose
    /// one: before the line across the flow, or after it
    /// ([`Edge::label_before`]).
    fn label_before(&self, _container: Self::Container, _index: usize) -> Option<bool> {
        None
    }
    /// Line `index` runs within a rank ([`Edge::level`]): its second box
    /// right after its first across the flow, or right before it.
    fn level(&self, _container: Self::Container, _index: usize) -> Option<bool> {
        None
    }
    /// How a line drawn as `look` arrives at the box it ends at
    /// ([`super::arrival`]): lines share a trunk only when this is the
    /// same, so the heads that stack on it are one kind.
    fn arrival(&self, _look: &Self::Look) -> u64 {
        0
    }
    /// The room the members keep for notes.
    fn rooms(&self, _container: Self::Container) -> Result<Vec<(usize, Room)>, Self::Error> {
        Ok(Vec::new())
    }
    /// Moves what a box laid out on its own holds into `rect`.
    fn settle(&self, _item: &mut Measured<Self::Item, Self::Look>, _rect: Rect) {}
    /// Ends a line on the outline of `end` instead of its box.
    fn clip(&self, _end: &Placed<Self::Item, Self::Look>, _points: &mut [Point], _at_start: bool) {}
    /// The band of an opened member's name, which labels keep off.
    fn band(&self, node: &Placed<Self::Item, Self::Look>) -> Rect;
    /// What else in `scene`, the whole layout, labels keep off: marks
    /// beside the ends of its lines, as multiplicities.
    fn marks(&self, _scene: &Scene<Self::Item, Self::Look>) -> Vec<Rect> {
        Vec::new()
    }
    /// Places the notes of `container` once its labels are placed.
    fn noted(
        &self,
        _container: Self::Container,
        _scene: &mut Scene<Self::Item, Self::Look>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<N, E> Scene<N, E> {
    pub fn empty() -> Scene<N, E> {
        Scene {
            nodes: Vec::new(),
            edges: Vec::new(),
            notes: Vec::new(),
            width: 0.0,
            height: 0.0,
            unheld: Vec::new(),
        }
    }

    /// The holds not kept here and in every box laid out inside, by the
    /// line of each entry.
    pub fn all_unheld(&self) -> Vec<(usize, Unheld)> {
        let mut out = self.unheld.clone();
        for node in &self.nodes {
            out.extend(node.children.all_unheld());
        }
        out.sort_by_key(|&(line, _)| line);
        out
    }

    pub fn offset(&mut self, dx: f32, dy: f32) {
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

    /// The box at `path`, down through the boxes' scenes.
    pub fn placed_at(&self, path: &[usize]) -> Option<&Placed<N, E>> {
        let (&last, above) = path.split_last()?;
        let mut scene = self;
        for &index in above {
            scene = &scene.nodes.get(index)?.children;
        }
        scene.nodes.get(last)
    }

    /// Its lines and those of the containers it opened, in the order their
    /// labels are placed; not those of the containers laid out on their own.
    pub fn wires(&self) -> Vec<&Wire<E>> {
        let mut out: Vec<&Wire<E>> = self.edges.iter().collect();
        for node in &self.nodes {
            if node.opened {
                out.extend(node.children.wires());
            }
        }
        out
    }

    /// The scene at `path`, down through the boxes' scenes.
    pub fn scene_at(&mut self, path: &[usize]) -> Option<&mut Scene<N, E>> {
        match path.split_first() {
            Some((&first, rest)) => self.nodes.get_mut(first)?.children.scene_at(rest),
            None => Some(self),
        }
    }
}

impl<N, E> Placed<N, E> {
    pub fn of(item: Measured<N, E>, rect: Rect) -> Placed<N, E> {
        Placed {
            rect,
            opened: false,
            children: item.children,
            item: item.item,
        }
    }
}

impl<E> Wire<E> {
    /// The label's box, when there is a label.
    pub fn label_rect(&self) -> Option<Rect> {
        (self.label.width > 0.0 && self.label.height > 0.0).then_some(Rect {
            x: self.label_at.0,
            y: self.label_at.1,
            w: self.label.width,
            h: self.label.height,
        })
    }
}

/// Lays `top` out, and every container it opens with it.
pub fn layout<C: Nested>(cx: &C, top: C::Container) -> Result<Scene<C::Item, C::Look>, C::Error> {
    let mut open = HashSet::new();
    mark(cx, top, &mut Vec::new(), &mut open);
    let (axis, mirrored) = cx.flow(top);
    let mut flat = Flat {
        open,
        containers: HashMap::from([(Vec::new(), top)]),
        ends: HashMap::new(),
        measured: Vec::new(),
        framed: Vec::new(),
        nodes: Vec::new(),
        frames: Vec::new(),
        within: Vec::new(),
        edges: Vec::new(),
        levels: HashMap::new(),
    };
    flat.gather(cx, top, &mut Vec::new(), None)?;
    let apart = flat.apart_nodes(cx);
    flat.join(cx, top, &mut Vec::new(), axis, &apart)?;
    let (mut holds, lines, mut unheld) = flat.holds(cx);
    // The layout is laid unmirrored and turned after: a box below on the
    // page is laid above.
    if mirrored {
        for hold in &mut holds {
            hold.word = hold.word.mirrored(axis);
        }
    }
    let laid = framed(
        &flat.nodes,
        &flat.frames,
        &flat.within,
        &flat.edges,
        &holds,
        axis,
    );
    unheld.extend(laid.unheld.iter().map(|&(at, why)| (lines[at], why)));
    let mut scene = flat.assemble(cx, top, &mut Vec::new(), &laid);
    scene.unheld = unheld;
    if mirrored {
        match axis {
            Direction::Down => flip_y(&mut scene, laid.height),
            Direction::Right => flip_x(&mut scene, laid.width),
        }
    }

    // Labels move off boxes, names, marks and each other, over the whole
    // layout.
    let marks = cx.marks(&scene);
    let (mut boxes, mut frames) = (Vec::new(), Vec::new());
    let mut wires = Vec::new();
    collect(cx, &mut scene, &mut boxes, &mut frames, &mut wires);
    boxes.extend(marks);
    let lines: Vec<Vec<Point>> = wires.iter().map(|wire| wire.points.clone()).collect();
    let mut labels: Vec<_> = wires
        .iter()
        .map(|wire| (wire.label_at, (wire.label.width, wire.label.height)))
        .collect();
    let room = route::place_labels(
        &lines,
        &mut labels,
        &boxes,
        &frames,
        (laid.width, laid.height),
    );
    for (wire, (at, _)) in wires.into_iter().zip(labels) {
        wire.label_at = at;
    }
    flat.note(cx, top, &mut scene, &mut Vec::new())?;
    scene.width = room.w;
    scene.height = room.h;
    scene.offset(-room.x, -room.y);
    Ok(scene)
}

/// Which containers inside `container` (at `at`) to open, by their paths
/// from the top: every one a line from a container laid out here passes
/// into, when every container on its way can open.
fn mark<C: Nested>(
    cx: &C,
    container: C::Container,
    at: &mut Vec<usize>,
    open: &mut HashSet<Vec<usize>>,
) {
    for line in cx.lines(container) {
        if line.hidden || !openable(cx, container, &line.from) || !openable(cx, container, &line.to)
        {
            continue;
        }
        for path in [&line.from, &line.to] {
            for depth in 1..path.len() {
                open.insert([at.as_slice(), &path[..depth]].concat());
            }
        }
    }
    for index in 0..cx.members(container) {
        at.push(index);
        if open.contains(at.as_slice())
            && let Some(inner) = cx.inner(container, index)
        {
            mark(cx, inner, at, open);
        }
        at.pop();
    }
}

/// Whether every container on the way down `path` can open.
fn openable<C: Nested>(cx: &C, container: C::Container, path: &[usize]) -> bool {
    let mut here = container;
    for &index in &path[..path.len().saturating_sub(1)] {
        let looped = cx
            .lines(here)
            .iter()
            .any(|line| line.from == [index] && line.to == [index]);
        match cx.inner(here, index) {
            Some(inner) if !looped => here = inner,
            _ => return false,
        }
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Node(usize),
    Frame(usize),
}

/// A line of one container on its way through the layout.
struct Pending<E> {
    /// Its edge in the layout.
    at: usize,
    drawn: bool,
    label: Block,
    look: E,
    /// The members at its ends, for a line between two of them.
    ends: Option<(usize, usize)>,
    /// The paths to its ends, for a crossing drawn here.
    paths: Option<(Vec<usize>, Vec<usize>)>,
}

/// One layout, through the containers it opens.
struct Flat<C: Nested> {
    open: HashSet<Vec<usize>>,
    containers: HashMap<Vec<usize>, C::Container>,
    ends: HashMap<Vec<usize>, End>,
    measured: Vec<Option<Measured<C::Item, C::Look>>>,
    framed: Vec<Option<Measured<C::Item, C::Look>>>,
    nodes: Vec<Node>,
    frames: Vec<Frame>,
    within: Vec<Option<usize>>,
    edges: Vec<Edge>,
    levels: HashMap<Vec<usize>, Vec<Pending<C::Look>>>,
}

impl<C: Nested> Flat<C> {
    /// Every member of `container` at `path` and of the containers it
    /// opens: a box, or a frame around what it holds.
    fn gather(
        &mut self,
        cx: &C,
        container: C::Container,
        path: &mut Vec<usize>,
        frame: Option<usize>,
    ) -> Result<(), C::Error> {
        for index in 0..cx.members(container) {
            path.push(index);
            let inner = if self.open.contains(path.as_slice()) {
                cx.inner(container, index)
            } else {
                None
            };
            if let Some(inner) = inner {
                let item = cx.measure(container, index, true)?;
                let at = self.frames.len();
                self.frames.push(Frame {
                    parent: frame,
                    ..cx.frame(container, &item)
                });
                self.framed.push(Some(item));
                self.ends.insert(path.clone(), End::Frame(at));
                self.containers.insert(path.clone(), inner);
                self.gather(cx, inner, path, Some(at))?;
            } else {
                let item = cx.measure(container, index, false)?;
                self.ends.insert(path.clone(), End::Node(self.nodes.len()));
                self.nodes.push(Node {
                    width: item.width,
                    height: item.height,
                    before: 0.0,
                    loop_extra: 0.0,
                    flat: item.flat,
                });
                self.within.push(frame);
                self.measured.push(Some(item));
            }
            path.pop();
        }
        Ok(())
    }

    /// The holds of every container this layout lays out, as the layout's
    /// boxes, with the line of each; and those that cannot be, a container
    /// laid out here being held.
    fn holds(&self, cx: &C) -> (Vec<Hold>, Vec<usize>, Vec<(usize, Unheld)>) {
        let mut paths: Vec<&Vec<usize>> = self.containers.keys().collect();
        paths.sort();
        let (mut holds, mut lines, mut unheld) = (Vec::new(), Vec::new(), Vec::new());
        for path in paths {
            for held in cx.holds(self.containers[path]) {
                let end = |index: usize| self.ends.get(&[path.as_slice(), &[index]].concat());
                match (end(held.index), end(held.beside)) {
                    (Some(&End::Node(node)), Some(&End::Node(beside))) => {
                        holds.push(Hold {
                            node,
                            beside,
                            word: held.word,
                        });
                        lines.push(held.line);
                    }
                    (Some(_), Some(_)) => unheld.push((held.line, Unheld::Frame)),
                    _ => unheld.push((held.line, Unheld::Missing)),
                }
            }
        }
        (holds, lines, unheld)
    }

    /// The boxes whose lines stay out of a bundle: every box of a
    /// container that bundles nothing, and the members named apart.
    fn apart_nodes(&self, cx: &C) -> HashSet<usize> {
        let mut set = HashSet::new();
        for (path, &container) in &self.containers {
            if !cx.bundled(container) {
                for (at, end) in &self.ends {
                    if at.starts_with(path.as_slice())
                        && let End::Node(node) = end
                    {
                        set.insert(*node);
                    }
                }
            }
            for index in cx.apart_at(container) {
                let mut at = path.clone();
                at.push(index);
                if let Some(End::Node(node)) = self.ends.get(&at) {
                    set.insert(*node);
                }
            }
        }
        set
    }

    /// The layout's index of the member at `path`.
    fn index(&self, path: &[usize]) -> Option<usize> {
        match self.ends.get(path)? {
            End::Node(at) => Some(*at),
            End::Frame(at) => Some(self.nodes.len() + at),
        }
    }

    /// The lines and the room for notes of `container` at `path`, and of
    /// the containers it opens.
    fn join(
        &mut self,
        cx: &C,
        container: C::Container,
        path: &mut Vec<usize>,
        axis: Direction,
        apart: &HashSet<usize>,
    ) -> Result<(), C::Error> {
        let count = cx.members(container);
        let child = |path: &[usize], index: usize| [path, &[index]].concat();
        let mut level = Vec::new();
        let mut loops = vec![super::Loops::default(); count];
        for (index, line) in cx.lines(container).into_iter().enumerate() {
            let (label, look) = cx.line(container, index)?;
            let (ends, drawn, ends_at, paths) = if line.direct() {
                let (from, to) = (line.from[0], line.to[0]);
                let ends = (self.index(&child(path, from)), self.index(&child(path, to)));
                (ends, !line.hidden, Some((from, to)), None)
            } else {
                let (from, whole_from) = self.resolve(path, &line.from);
                let (to, whole_to) = self.resolve(path, &line.to);
                // Drawn here when both ends are; otherwise it only ranks
                // the boxes that hold them, and is routed later.
                let drawn = whole_from && whole_to && !line.hidden;
                let paths = drawn.then(|| (line.from.clone(), line.to.clone()));
                ((from, to), drawn, None, paths)
            };
            let (from, to) = match ends {
                (Some(from), Some(to)) => (from, to),
                // A line the layout drops: it names no member.
                _ => (usize::MAX, usize::MAX),
            };
            if let Some((member, _)) = ends_at
                && from == to
                && from < self.nodes.len()
                && member < count
            {
                // Each loop lies around the ones before and their words;
                // a box grows along its free side until all of them end
                // on it, each at a place of its own.
                let _ = loops[member].next((label.width, label.height), axis);
                let node = &mut self.nodes[from];
                node.loop_extra = node.loop_extra.max(loops[member].room());
                if node.flat {
                    let side = loops[member].side();
                    match axis {
                        Direction::Down => node.height = node.height.max(side),
                        Direction::Right => node.width = node.width.max(side),
                    }
                }
            }
            self.edges.push(Edge {
                from,
                to,
                label_width: label.width,
                label_height: label.height,
                hidden: !drawn,
                apart: apart.contains(&from) || apart.contains(&to),
                marked: cx.marked(container, index),
                arrival: cx.arrival(&look),
                label_before: cx.label_before(container, index),
                level: cx.level(container, index).filter(|_| drawn),
            });
            level.push(Pending {
                at: self.edges.len() - 1,
                drawn,
                label: if drawn { label } else { Block::empty() },
                look,
                ends: ends_at,
                paths,
            });
        }
        for (member, room) in cx.rooms(container)? {
            let Some(End::Node(at)) = self.ends.get(&child(path, member)).copied() else {
                continue;
            };
            let node = &mut self.nodes[at];
            match room {
                Room::Before(width) => node.before = node.before.max(width),
                Room::After(width) => node.loop_extra += width,
            }
        }
        self.levels.insert(path.clone(), level);
        for index in 0..count {
            path.push(index);
            if let Some(&inner) = self.containers.get(path.as_slice()) {
                self.join(cx, inner, path, axis, apart)?;
            }
            path.pop();
        }
        Ok(())
    }

    /// The layout's index of a crossing's end, down `steps` from the
    /// container at `path`, and whether that is the end itself rather than
    /// the closed box around it.
    fn resolve(&self, path: &[usize], steps: &[usize]) -> (Option<usize>, bool) {
        let mut at = path.to_vec();
        for (depth, &step) in steps.iter().enumerate() {
            at.push(step);
            let last = depth + 1 == steps.len();
            if last || !self.open.contains(&at) {
                return (self.index(&at), last);
            }
        }
        (None, false)
    }

    /// The scene of `container` at `path`, from the layout, with the scenes
    /// of the containers it opened inside. Lines end on the outlines of
    /// their boxes.
    fn assemble(
        &mut self,
        cx: &C,
        container: C::Container,
        path: &mut Vec<usize>,
        laid: &Layout,
    ) -> Scene<C::Item, C::Look> {
        let count = cx.members(container);
        let mut nodes = Vec::with_capacity(count);
        for _ in 0..count {
            path.push(nodes.len());
            let placed = match self.ends.get(path.as_slice()).copied() {
                Some(End::Frame(at)) => {
                    let item = self.framed[at].take().expect("one frame each");
                    let inner = self.containers[path.as_slice()];
                    let children = self.assemble(cx, inner, path, laid);
                    let rect = laid.frames[at];
                    Placed {
                        rect,
                        opened: true,
                        children: Scene {
                            width: rect.w,
                            height: rect.h,
                            ..children
                        },
                        item: item.item,
                    }
                }
                Some(End::Node(at)) => {
                    let mut item = self.measured[at].take().expect("one box each");
                    let rect = laid.nodes[at];
                    cx.settle(&mut item, rect);
                    Placed::of(item, rect)
                }
                None => unreachable!("every member was gathered"),
            };
            nodes.push(placed);
            path.pop();
        }
        let level = self.levels.remove(path.as_slice()).unwrap_or_default();
        let mut edges = Vec::with_capacity(level.len());
        for line in level {
            let mut wire = if line.drawn {
                let route = &laid.routes[line.at];
                Wire {
                    points: route.points.clone(),
                    label: line.label,
                    label_at: route.label_at,
                    look: line.look,
                }
            } else {
                // Routed once everything is placed.
                Wire {
                    points: Vec::new(),
                    label: Block::empty(),
                    label_at: (0.0, 0.0),
                    look: line.look,
                }
            };
            if line.drawn {
                let ends = match (&line.ends, &line.paths) {
                    (Some((from, to)), _) => (nodes.get(*from), nodes.get(*to)),
                    (None, Some((from, to))) => (placed_in(&nodes, from), placed_in(&nodes, to)),
                    (None, None) => (None, None),
                };
                if let Some(end) = ends.0 {
                    cx.clip(end, &mut wire.points, true);
                }
                if let Some(end) = ends.1 {
                    cx.clip(end, &mut wire.points, false);
                }
            }
            edges.push(wire);
        }
        Scene {
            nodes,
            edges,
            notes: Vec::new(),
            width: 0.0,
            height: 0.0,
            unheld: Vec::new(),
        }
    }

    /// Notes go beside their boxes once the labels are placed, so they
    /// step off them.
    fn note(
        &self,
        cx: &C,
        container: C::Container,
        scene: &mut Scene<C::Item, C::Look>,
        path: &mut Vec<usize>,
    ) -> Result<(), C::Error> {
        cx.noted(container, scene)?;
        for (index, node) in scene.nodes.iter_mut().enumerate() {
            path.push(index);
            if node.opened
                && let Some(&inner) = self.containers.get(path.as_slice())
            {
                self.note(cx, inner, &mut node.children, path)?;
            }
            path.pop();
        }
        Ok(())
    }
}

fn placed_in<'a, N, E>(nodes: &'a [Placed<N, E>], path: &[usize]) -> Option<&'a Placed<N, E>> {
    let (&first, rest) = path.split_first()?;
    let node = nodes.get(first)?;
    if rest.is_empty() {
        Some(node)
    } else {
        node.children.placed_at(rest)
    }
}

/// Mirrors the layout top to bottom, through the containers it opened; one
/// laid out on its own moves as a whole.
fn flip_y<N, E>(scene: &mut Scene<N, E>, height: f32) {
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
fn flip_x<N, E>(scene: &mut Scene<N, E>, width: f32) {
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

/// The boxes labels keep off, the borders of the containers this layout
/// opened, which they keep inside or outside of, and every line of it:
/// through the containers it opened, not into those laid out on their own.
fn collect<'a, C: Nested>(
    cx: &C,
    scene: &'a mut Scene<C::Item, C::Look>,
    boxes: &mut Vec<Rect>,
    frames: &mut Vec<Rect>,
    out: &mut Vec<&'a mut Wire<C::Look>>,
) {
    let Scene { nodes, edges, .. } = scene;
    out.extend(edges.iter_mut());
    for node in nodes.iter_mut() {
        if node.opened {
            frames.push(node.rect);
            boxes.push(cx.band(node));
            collect(cx, &mut node.children, boxes, frames, out);
        } else {
            boxes.push(node.rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use super::*;

    /// A container of boxes and containers, with its lines.
    struct Group {
        members: Vec<Option<Group>>,
        lines: Vec<Line>,
    }

    /// Boxes 40 by 20; a closed container is its layout and some room.
    struct Boxes<'a>(PhantomData<&'a Group>);

    impl<'a> Nested for Boxes<'a> {
        type Container = &'a Group;
        type Item = ();
        type Look = ();
        type Error = ();

        fn members(&self, group: &'a Group) -> usize {
            group.members.len()
        }

        fn inner(&self, group: &'a Group, index: usize) -> Option<&'a Group> {
            group.members.get(index)?.as_ref()
        }

        fn flow(&self, _group: &'a Group) -> (Direction, bool) {
            (Direction::Down, false)
        }

        fn lines(&self, group: &'a Group) -> Vec<Line> {
            group.lines.clone()
        }

        fn measure(
            &self,
            group: &'a Group,
            index: usize,
            open: bool,
        ) -> Result<Measured<(), ()>, ()> {
            let (width, height, children) = match &group.members[index] {
                Some(inner) if !open => {
                    let children = layout(self, inner)?;
                    (children.width + 24.0, children.height + 32.0, children)
                }
                _ => (40.0, 20.0, Scene::empty()),
            };
            Ok(Measured {
                width,
                height,
                flat: true,
                children,
                item: (),
            })
        }

        fn frame(&self, _group: &'a Group, _item: &Measured<(), ()>) -> Frame {
            Frame {
                top: 20.0,
                bottom: 12.0,
                left: 12.0,
                right: 12.0,
                min_width: 40.0,
                ..Frame::default()
            }
        }

        fn line(&self, _group: &'a Group, _index: usize) -> Result<(Block, ()), ()> {
            Ok((Block::empty(), ()))
        }

        fn settle(&self, item: &mut Measured<(), ()>, rect: Rect) {
            item.children.offset(rect.x + 12.0, rect.y + 20.0);
        }

        fn band(&self, node: &Placed<(), ()>) -> Rect {
            Rect {
                h: 20.0,
                ..node.rect
            }
        }
    }

    fn line(from: &[usize], to: &[usize], hidden: bool) -> Line {
        Line {
            from: from.to_vec(),
            to: to.to_vec(),
            hidden,
        }
    }

    /// A box, and a container holding one box; `lines` between them.
    fn laid(lines: Vec<Line>) -> Scene<(), ()> {
        let top = Group {
            members: vec![
                None,
                Some(Group {
                    members: vec![None],
                    lines: Vec::new(),
                }),
            ],
            lines,
        };
        layout(&Boxes(PhantomData), &top).unwrap()
    }

    #[test]
    fn a_line_from_outside_opens_the_container_and_runs_to_the_box_in_it() {
        let scene = laid(vec![line(&[0], &[1, 0], false)]);
        let (frame, inner) = (&scene.nodes[1], &scene.nodes[1].children.nodes[0]);
        assert!(frame.opened);
        let tip = *scene.edges[0].points.last().unwrap();
        assert!(
            (tip.1 - inner.rect.y).abs() < 0.5,
            "{tip:?} {:?}",
            inner.rect
        );
        let r = frame.rect;
        assert!(inner.rect.x > r.x && inner.rect.x + inner.rect.w < r.x + r.w);
    }

    #[test]
    fn a_container_with_a_line_to_itself_stays_a_box_and_the_line_waits() {
        let scene = laid(vec![line(&[0], &[1, 0], false), line(&[1], &[1], false)]);
        assert!(!scene.nodes[1].opened);
        assert!(scene.edges[0].points.is_empty());
        // What it holds was laid out on its own and moved into its box.
        let (outer, inner) = (scene.nodes[1].rect, scene.nodes[1].children.nodes[0].rect);
        assert!(inner.x >= outer.x && inner.y >= outer.y);
    }

    #[test]
    fn a_hidden_line_opens_nothing_and_is_not_drawn() {
        let scene = laid(vec![line(&[0], &[1, 0], true)]);
        assert!(!scene.nodes[1].opened);
        assert!(scene.edges[0].points.is_empty());
    }
}
