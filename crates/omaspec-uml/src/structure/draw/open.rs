//! Containers laid out with what is around them.
//!
//! A package, node or frame that a line runs into from outside is opened:
//! its elements take the ranks of the group around it, as Graphviz lays
//! out a cluster, and the line runs through the ranks like any other, in
//! through the side the flow enters by ([`graph::framed`]). The others are
//! laid out on their own and placed as boxes; a line into one of those is
//! routed once everything is placed.

use std::collections::{HashMap, HashSet};

use super::{
    Block, Declined, Direction, Draft, Family, Group, LOOP_REACH, LOOP_STEP, Labeled, Measured,
    NOTE_GAP, Placed, PlacedEdge, Rect, Scene, Side, Skin, arrow_stroke, child_origin, frame_of,
    graph, graph_direction, is_flat, link_blocks, measure_element, name_band, note_size,
    place_notes, text_block,
};

/// Which of the containers inside `group` to open, by their paths from
/// it: every one a line from a group laid out here passes into.
fn opened(group: &Group) -> HashSet<Vec<usize>> {
    let mut open = HashSet::new();
    mark(group, &mut Vec::new(), &mut open);
    open
}

fn mark(group: &Group, at: &mut Vec<usize>, open: &mut HashSet<Vec<usize>>) {
    for crossing in &group.crossings {
        if !openable(group, &crossing.from) || !openable(group, &crossing.to) {
            continue;
        }
        for path in [&crossing.from, &crossing.to] {
            for depth in 1..path.len() {
                open.insert([at.as_slice(), &path[..depth]].concat());
            }
        }
    }
    for (index, node) in group.nodes.iter().enumerate() {
        at.push(index);
        if open.contains(at.as_slice()) {
            mark(&node.children, at, open);
        }
        at.pop();
    }
}

/// Whether every container on the way down `path` can be opened: one
/// with a line from itself to itself stays a box.
fn openable(group: &Group, path: &[usize]) -> bool {
    let mut here = group;
    for &index in &path[..path.len().saturating_sub(1)] {
        let Some(node) = here.nodes.get(index) else {
            return false;
        };
        let looped = here.edges.iter().any(|e| e.from == index && e.to == index);
        if !node.composite || looped {
            return false;
        }
        here = &node.children;
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Node(usize),
    Frame(usize),
}

/// Where the edges and crossings of one group went among the graph's
/// edges, and its notes.
#[derive(Default)]
struct Level {
    edges: Vec<usize>,
    /// The graph edge of each crossing, and whether it is drawn here.
    crossings: Vec<(usize, bool)>,
    drafts: Vec<Draft>,
}

/// One layout, through the containers it opens.
#[derive(Default)]
struct Flat {
    ends: HashMap<Vec<usize>, End>,
    measured: Vec<Option<Measured>>,
    framed: Vec<Option<Measured>>,
    nodes: Vec<graph::Node>,
    frames: Vec<graph::Frame>,
    within: Vec<Option<usize>>,
    edges: Vec<graph::Edge>,
    labeled: Vec<Option<Labeled>>,
    levels: HashMap<Vec<usize>, Level>,
}

/// Lays `group` out, and every container it opens with it.
pub(super) fn layout_group(group: &Group, skin: &Skin, family: Family) -> Result<Scene, Declined> {
    if group.nodes.is_empty() {
        return Ok(Scene::empty());
    }
    let open = opened(group);
    let mut flat = Flat::default();
    flat.gather(group, &mut Vec::new(), None, &open, skin, family)?;
    flat.join(group, &mut Vec::new(), &open, skin, family)?;
    let laid = graph::framed(
        &flat.nodes,
        &flat.frames,
        &flat.within,
        &flat.edges,
        graph_direction(group.direction),
    );

    let mut scene = flat.assemble(group, &mut Vec::new(), &laid, skin)?;
    // Labels move off boxes, names and each other, over the whole layout.
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
    place_all_notes(&mut scene, &mut flat, &mut Vec::new());
    scene.width = room.w;
    scene.height = room.h;
    scene.offset(-room.x, -room.y);
    Ok(scene)
}

/// The boxes labels keep off, and every edge of this layout: through the
/// containers it opened, not into those laid out on their own.
fn collect<'a>(scene: &'a mut Scene, boxes: &mut Vec<Rect>, out: &mut Vec<&'a mut PlacedEdge>) {
    let Scene { nodes, edges, .. } = scene;
    out.extend(edges.iter_mut());
    for node in nodes.iter_mut() {
        if node.opened {
            boxes.push(name_band(node));
            collect(&mut node.children, boxes, out);
        } else {
            boxes.push(node.rect);
        }
    }
}

/// Notes go beside their elements once the labels are placed, so they
/// step off them.
fn place_all_notes(scene: &mut Scene, flat: &mut Flat, path: &mut Vec<usize>) {
    if let Some(level) = flat.levels.get_mut(path.as_slice()) {
        let rects: Vec<Rect> = scene.nodes.iter().map(|node| node.rect).collect();
        scene.notes = place_notes(&mut level.drafts, &rects, &scene.edges);
    }
    for (index, node) in scene.nodes.iter_mut().enumerate() {
        if node.opened {
            path.push(index);
            place_all_notes(&mut node.children, flat, path);
            path.pop();
        }
    }
}

impl Flat {
    /// Every element of `group` at `path` and of the containers it opens:
    /// a box, or a frame around what it holds.
    fn gather(
        &mut self,
        group: &Group,
        path: &mut Vec<usize>,
        frame: Option<usize>,
        open: &HashSet<Vec<usize>>,
        skin: &Skin,
        family: Family,
    ) -> Result<(), Declined> {
        for (index, el) in group.nodes.iter().enumerate() {
            path.push(index);
            if open.contains(path.as_slice()) {
                let item = measure_element(el, skin, family, true)?;
                let at = self.frames.len();
                self.frames.push(graph::Frame {
                    parent: frame,
                    ..frame_of(&item, skin)
                });
                self.framed.push(Some(item));
                self.ends.insert(path.clone(), End::Frame(at));
                self.gather(&el.children, path, Some(at), open, skin, family)?;
            } else {
                let item = measure_element(el, skin, family, false)?;
                self.ends.insert(path.clone(), End::Node(self.nodes.len()));
                self.nodes.push(graph::Node {
                    width: item.width,
                    height: item.height,
                    before: 0.0,
                    loop_extra: 0.0,
                    flat: is_flat(item.shape),
                });
                self.within.push(frame);
                self.measured.push(Some(item));
            }
            path.pop();
        }
        Ok(())
    }

    /// The graph index of the element at `path`.
    fn index(&self, path: &[usize]) -> Option<usize> {
        match self.ends.get(path)? {
            End::Node(at) => Some(*at),
            End::Frame(at) => Some(self.nodes.len() + at),
        }
    }

    /// The edges, crossings and notes of `group` at `path` and of the
    /// containers it opens.
    fn join(
        &mut self,
        group: &Group,
        path: &mut Vec<usize>,
        open: &HashSet<Vec<usize>>,
        skin: &Skin,
        family: Family,
    ) -> Result<(), Declined> {
        let count = group.nodes.len();
        let child = |path: &[usize], index: usize| [path, &[index]].concat();
        let mut level = Level::default();
        let mut loops = vec![0usize; count];
        for edge in &group.edges {
            if edge.from >= count || edge.to >= count {
                return Err(Declined::at(edge.line, "a line needs two elements"));
            }
            let labeled = link_blocks(edge, skin, family)?;
            let (from, to) = (
                self.index(&child(path, edge.from)),
                self.index(&child(path, edge.to)),
            );
            let (Some(from), Some(to)) = (from, to) else {
                return Err(Declined::at(edge.line, "a line needs two elements"));
            };
            if from == to && from < self.nodes.len() {
                let channel = loops[edge.from];
                loops[edge.from] += 1;
                let reach = LOOP_REACH + channel as f32 * LOOP_STEP;
                let extra = match group.direction {
                    Direction::Down => reach + 4.0 + labeled.label.width,
                    Direction::Right => reach + 4.0 + labeled.label.height,
                };
                let node = &mut self.nodes[from];
                node.loop_extra = node.loop_extra.max(extra);
            }
            level.edges.push(self.edges.len());
            self.edges.push(graph::Edge {
                from,
                to,
                label_width: labeled.label.width,
                label_height: labeled.label.height,
                hidden: false,
            });
            self.labeled.push(Some(labeled));
        }
        for crossing in &group.crossings {
            if crossing.arrow.from >= count || crossing.arrow.to >= count {
                return Err(Declined::at(
                    crossing.arrow.line,
                    "a line needs two elements",
                ));
            }
            let labeled = link_blocks(&crossing.arrow, skin, family)?;
            let (from, whole_from) = self.resolve(path, &crossing.from, open);
            let (to, whole_to) = self.resolve(path, &crossing.to, open);
            let (Some(from), Some(to)) = (from, to) else {
                return Err(Declined::at(
                    crossing.arrow.line,
                    "a line needs two elements",
                ));
            };
            // Drawn here when both ends are; otherwise it only ranks the
            // boxes that hold them, and is routed later.
            let drawn = whole_from && whole_to;
            level.crossings.push((self.edges.len(), drawn));
            self.edges.push(graph::Edge {
                from,
                to,
                label_width: labeled.label.width,
                label_height: labeled.label.height,
                hidden: !drawn,
            });
            self.labeled.push(drawn.then_some(labeled));
        }
        // Room kept across the flow, so a note there is not covered by the
        // next element. The other two sides hang off the box.
        for note in &group.notes {
            if note.target >= count {
                return Err(Declined::at(note.line, "a note needs an element"));
            }
            let text = text_block(&note.text, skin.note.font, family)?;
            let (w, h) = note_size(&text);
            if let Some(End::Node(at)) = self.ends.get(&child(path, note.target)).copied() {
                let node = &mut self.nodes[at];
                match (group.direction, note.side) {
                    (Direction::Down, Side::Left) => node.before = node.before.max(NOTE_GAP + w),
                    (Direction::Down, Side::Right) => {
                        node.loop_extra += NOTE_GAP + w;
                    }
                    (Direction::Right, Side::Top) => node.before = node.before.max(NOTE_GAP + h),
                    (Direction::Right, Side::Bottom) => {
                        node.loop_extra += NOTE_GAP + h;
                    }
                    _ => {}
                }
            }
            level.drafts.push(Draft {
                target: note.target,
                side: note.side,
                w,
                h,
                text,
            });
        }
        self.levels.insert(path.clone(), level);
        for (index, el) in group.nodes.iter().enumerate() {
            path.push(index);
            if open.contains(path.as_slice()) {
                self.join(&el.children, path, open, skin, family)?;
            }
            path.pop();
        }
        Ok(())
    }

    /// The graph index of a crossing's end, down `steps` from the group at
    /// `path`, and whether that is the end itself rather than the closed
    /// box around it.
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

    /// The scene of `group` at `path`, from the layout, with the scenes
    /// of the containers it opened inside.
    fn assemble(
        &mut self,
        group: &Group,
        path: &mut Vec<usize>,
        laid: &graph::Layout,
        skin: &Skin,
    ) -> Result<Scene, Declined> {
        let mut nodes = Vec::with_capacity(group.nodes.len());
        for (index, el) in group.nodes.iter().enumerate() {
            path.push(index);
            let placed = match self.ends.get(path.as_slice()).copied() {
                Some(End::Frame(at)) => {
                    let item = self.framed[at].take().expect("one frame each");
                    let children = self.assemble(&el.children, path, laid, skin)?;
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
                        let (ox, oy) = child_origin(rect, &item);
                        item.children.offset(ox, oy);
                    }
                    Placed::of(item, rect)
                }
                None => unreachable!("every element was gathered"),
            };
            nodes.push(placed);
            path.pop();
        }
        let Some(level) = self.levels.get(path.as_slice()) else {
            unreachable!("every group was joined");
        };
        let mut edges = Vec::with_capacity(group.edges.len() + group.crossings.len());
        let arrows = group
            .edges
            .iter()
            .zip(level.edges.iter().map(|&at| (at, true)))
            .chain(
                group
                    .crossings
                    .iter()
                    .map(|c| &c.arrow)
                    .zip(level.crossings.iter().copied()),
            );
        for (arrow, (at, drawn)) in arrows {
            let route = &laid.routes[at];
            let edge = match self.labeled[at].take() {
                Some(blocks) if drawn => PlacedEdge {
                    points: route.points.clone(),
                    label: blocks.label,
                    label_at: route.label_at,
                    from_card: blocks.from_card,
                    to_card: blocks.to_card,
                    from_head: arrow.from_head,
                    to_head: arrow.to_head,
                    stroke: arrow_stroke(skin, arrow)?,
                },
                // Routed once everything is placed.
                _ => PlacedEdge {
                    points: Vec::new(),
                    label: Block::empty(),
                    label_at: (0.0, 0.0),
                    from_card: Block::empty(),
                    to_card: Block::empty(),
                    from_head: arrow.from_head,
                    to_head: arrow.to_head,
                    stroke: arrow_stroke(skin, arrow)?,
                },
            };
            edges.push(edge);
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
