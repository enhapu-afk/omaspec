//! One container of a layered graph, and the containers laid out with it.
//!
//! Ranks come from the longest path after a depth-first search has turned
//! edges into an acyclic set (an edge into a node still on the stack is a
//! back edge). A barycenter sweep orders each rank. Edges that skip a rank
//! get dummy nodes, and every route is an orthogonal polyline.
//!
//! The flow is top to bottom or left to right. A container a line runs
//! into from outside is a frame of the layout around it ([`framed`],
//! `nest`); any other is laid out on its own and passed upward as a
//! finished box, so a nested state's own direction stays inside it.
//!
//! Every bend sits in a gap between two ranks, never inside a rank, so no
//! line cuts through a box, however tall its neighbors are. A line bends a
//! little after the rank it leaves, each on a track of its own where
//! several run across the same gap, so none lies on another; its label
//! sits beside the line further down that gap, which was made tall enough
//! for it and for the tracks. A back edge runs the same way, through the
//! ranks between its boxes, and is drawn backward, as Graphviz draws it:
//! a transition and the one back lie side by side, not around the
//! diagram. Lines that meet one side of a box share it, in the order of
//! their other ends, each as near as that allows to the point facing its
//! other end ([`ports`]). Self-loops sit on the free side (right when the
//! flow is downward, below when it goes right) and reserve that room while
//! the rank is packed; the drawn box stays the size the caller measured.
//!
//! Containers can be laid out with everything around them, as Graphviz
//! lays out clusters ([`framed`]): their boxes take the ranks of the whole
//! diagram, and a line between two of them runs through the ranks like
//! any other, into a container through the side the flow enters by. A
//! line between two containers laid out on their own is not this layout's
//! to draw; [`across`] runs it around the boxes once everything is placed,
//! on [`route`].

pub mod across;
pub mod by_hand;
mod nest;
pub mod route;

/// A box the layout places, with empty room kept on either side across
/// the flow: `before` it (left when the flow runs down) for a note there,
/// and `loop_extra` after it for a self-loop and its label, or a note. The
/// room is packed with the box, so nothing else lands in it. `flat` when
/// lines may meet the box anywhere along a side, as on a state; otherwise
/// only in the middle of a side, as on a circle or a diamond.
#[derive(Debug, Clone, Copy)]
pub struct Node {
    pub width: f32,
    pub height: f32,
    pub before: f32,
    pub loop_extra: f32,
    pub flat: bool,
}

/// A connection. `from == to` is a self-loop and takes no rank.
#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub label_width: f32,
    pub label_height: f32,
    /// Ranks and orders the nodes like any other edge, but gets no route
    /// and no room of its own: a line between containers, which is routed
    /// once everything is placed.
    pub hidden: bool,
}

/// A frame around some boxes of a layout: a container laid out with
/// everything around it ([`framed`]).
#[derive(Debug, Clone, Copy, Default)]
pub struct Frame {
    /// The frame around this one; `None` at the top.
    pub parent: Option<usize>,
    /// Room inside the frame around what it holds, on screen. The name
    /// takes the room above.
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    /// How wide the frame is at least, for its name.
    pub min_width: f32,
    pub name: Name,
}

/// Where a frame's name lies on the band at its top, which lines coming
/// in from above pass beside. Its left edge lies `share` of the frame's
/// width plus `offset` from the frame's left border; a width of zero is
/// no name in the way.
#[derive(Debug, Clone, Copy, Default)]
pub struct Name {
    pub share: f32,
    pub offset: f32,
    pub width: f32,
    /// How far down from the frame's top the band reaches.
    pub height: f32,
}

/// Which way the ranks run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Down,
    Right,
}

/// A placed box, in the container's own coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    fn right(self) -> f32 {
        self.x + self.w
    }

    fn bottom(self) -> f32 {
        self.y + self.h
    }

    fn cx(self) -> f32 {
        self.x + self.w / 2.0
    }

    fn cy(self) -> f32 {
        self.y + self.h / 2.0
    }
}

/// One edge after layout: the polyline, and the label's top left and size.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub points: Vec<(f32, f32)>,
    pub label_at: (f32, f32),
    pub label_width: f32,
    pub label_height: f32,
}

/// Every node and edge of one container, and the frames of [`framed`].
/// The origin is the top left of what was placed, gutters and labels
/// included.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub nodes: Vec<Rect>,
    pub routes: Vec<Route>,
    pub frames: Vec<Rect>,
    pub width: f32,
    pub height: f32,
}

/// Room between two nodes of one rank.
const CROSS_GAP: f32 = 44.0;
/// Between two lines passing through a rank side by side.
const DUMMY_GAP: f32 = 12.0;
/// Down and up passes that straighten the lines.
const STRAIGHTEN_PASSES: usize = 6;
/// Between lines that meet one side of a box, if it is long enough.
const PORT_STEP: f32 = 16.0;
/// How far lines meet a flat side from its corners, which may be round;
/// on a short side a quarter of it.
const PORT_CORNER: f32 = 12.0;
/// Passes that place where lines meet their boxes.
const PORT_PASSES: usize = 3;
/// Room between two ranks, before a label asks for more.
const ALONG_PAD: f32 = 32.0;
/// Where a line bends, past the end of the rank it leaves.
const JOG: f32 = 12.0;
/// From that bend down to the label.
const LABEL_DROP: f32 = 5.0;
/// Between two lines running across one gap.
const TRACK: f32 = 10.0;
/// Lays `nodes` out. Edges that name a missing node are dropped.
pub fn layout(nodes: &[Node], edges: &[Edge], direction: Direction) -> Layout {
    framed(nodes, &[], &[], edges, direction)
}

/// Lays `nodes` out with `frames` around some of them, all in one go:
/// `within[i]` is the innermost frame around node `i`, if any. An edge end
/// past the nodes, `nodes.len() + f`, is frame `f` itself: the line ends
/// at its border. Edges that name neither are dropped, and so is an edge
/// between a frame and what it holds.
pub fn framed(
    nodes: &[Node],
    frames: &[Frame],
    within: &[Option<usize>],
    edges: &[Edge],
    direction: Direction,
) -> Layout {
    let n = nodes.len();
    if n == 0 {
        return Layout {
            nodes: Vec::new(),
            routes: vec![
                Route {
                    points: Vec::new(),
                    label_at: (0.0, 0.0),
                    label_width: 0.0,
                    label_height: 0.0,
                };
                edges.len()
            ],
            frames: vec![Rect::default(); frames.len()],
            width: 0.0,
            height: 0.0,
        };
    }

    let mut tree = (!frames.is_empty()).then(|| nest::Tree::new(frames, within, n, direction));
    let (rank, back) = match &tree {
        Some(tree) => nest::ranks(tree, edges),
        None => ranks(n, edges),
    };
    let spans = tree
        .as_ref()
        .map_or_else(Vec::new, |tree| nest::spans(tree, &rank));
    let nested = tree.as_ref().map(|tree| (tree, spans.as_slice()));
    let Built {
        mut slots,
        mut rows,
        chains,
        first,
        frames: held,
    } = slots_of(nodes, edges, &rank, &back, direction, nested);
    let mut borders = Vec::new();
    let mut ports = Vec::new();
    let mut jogs = nest::Jogs::new();
    let mut room = Vec::new();
    match &mut tree {
        Some(tree) => {
            sweep_framed(&mut rows, &slots, &held, tree.count());
            // A frame too narrow for the lines to pass its name grows, once.
            for attempt in 0..2 {
                borders = place_framed(nodes, &mut slots, &rows, &held, tree);
                ports = self::ports(nodes, &slots, &chains, edges, direction);
                if direction != Direction::Down {
                    break;
                }
                let lines = nest::Lines {
                    slots: &slots,
                    chains: &chains,
                    first: &first,
                    ports: &ports,
                    frames: &held,
                    edges,
                };
                let short;
                (jogs, room, short) = nest::jogs(tree, &spans, &borders, &lines);
                if short.is_empty() || attempt == 1 {
                    break;
                }
                for (frame, width) in short {
                    let pad = &mut tree.pads[frame];
                    pad.min_cross = pad.min_cross.max(width);
                }
            }
        }
        None => {
            sweep(&mut rows, &slots);
            place_across(&mut slots, &rows);
            ports = self::ports(nodes, &slots, &chains, edges, direction);
        }
    }
    let mut gaps = rank_gaps(rows.len(), edges, &chains, &first, direction);
    let (tracks, counts) = tracks(&slots, edges, &chains, &ports, &first, &jogs, gaps.len());
    for (gap, count) in gaps.iter_mut().zip(counts) {
        *gap += count.saturating_sub(1) as f32 * TRACK;
    }
    // Where frames end and begin, a gap grows by their room; lines bend
    // past the frames that end there.
    let stacks = tree
        .as_ref()
        .map(|tree| nest::Stacks::new(tree, &spans, &room));
    let (ending, beginning) = match &stacks {
        Some(stacks) => stacks.zones(&spans, rows.len()),
        None => (vec![0.0; rows.len()], vec![0.0; rows.len()]),
    };
    for (index, gap) in gaps.iter_mut().enumerate() {
        *gap += ending[index] + beginning[index + 1];
    }
    let bands = place_along(&mut slots, &rows, &gaps);
    let rects: Vec<Rect> = (0..n).map(|i| rect_of(&slots, i, direction)).collect();
    let frame_rects: Vec<Rect> = match (&tree, &stacks) {
        (Some(tree), Some(stacks)) => (0..tree.count())
            .map(|f| frame_rect(f, &spans, &bands, &borders, stacks, tree, direction))
            .collect(),
        _ => Vec::new(),
    };
    // Each end of a chain is a box: a node, or a point on a frame's border
    // where its anchor lies, across the flow.
    let ends: Vec<Option<Rect>> = slots
        .iter()
        .map(|slot| match (slot.real, slot.anchor) {
            (Some(real), _) => Some(rects[real]),
            (None, Some((frame, at))) => frame_rects.get(frame).map(|rect| {
                let entered = spans[frame].is_some_and(|(first, _)| first == at);
                let cross = slot.cross;
                match direction {
                    Direction::Down => Rect {
                        x: cross,
                        y: if entered { rect.y } else { rect.bottom() },
                        w: 0.0,
                        h: 0.0,
                    },
                    Direction::Right => Rect {
                        x: if entered { rect.x } else { rect.right() },
                        y: cross,
                        w: 0.0,
                        h: 0.0,
                    },
                }
            }),
            (None, None) => None,
        })
        .collect();
    let valid = |end: usize| end < n + frames.len();
    let mut routes = Vec::with_capacity(edges.len());
    let mut self_seen = vec![0usize; n];
    for (index, edge) in edges.iter().enumerate() {
        let looped = edge.from == edge.to;
        if edge.hidden || !valid(edge.from) || !valid(edge.to) || looped && edge.from >= n {
            routes.push(Route {
                points: Vec::new(),
                label_at: (0.0, 0.0),
                label_width: 0.0,
                label_height: 0.0,
            });
            continue;
        }
        let (points, label_at) = if looped {
            let channel = self_seen[edge.from];
            self_seen[edge.from] += 1;
            let points = orthogonal(&self_loop(rects[edge.from], channel, direction), direction);
            let label_at = loop_label(&points, edge, direction);
            (points, label_at)
        } else {
            let bends: Vec<f32> = bands[first[index]..]
                .iter()
                .zip(&tracks[index])
                .enumerate()
                .map(|(step, (band, &track))| {
                    band.1 + ending[first[index] + step] + JOG + track as f32 * TRACK
                })
                .collect();
            let passes: Vec<Pass> = (0..chains[index].len().saturating_sub(1))
                .flat_map(|step| {
                    jogs.get(&(index, step))
                        .into_iter()
                        .flatten()
                        .map(move |jog| (step, jog))
                })
                .filter_map(|(step, jog)| {
                    let rect = frame_rects.get(jog.frame)?;
                    let name = frames[jog.frame].name;
                    let y = rect.y + name.height + nest::UNDER_NAME + jog.track as f32 * TRACK;
                    Some(Pass {
                        step,
                        beside: jog.above,
                        back: jog.below,
                        y,
                    })
                })
                .collect();
            let (mut points, label_at) = forward_route(
                &chains[index],
                &slots,
                &ends,
                (&bends, &passes),
                edge,
                ports[index],
                direction,
            );
            // A back edge's chain runs from its target down to its source.
            if back[index] {
                points.reverse();
            }
            (points, label_at)
        };
        routes.push(Route {
            points,
            label_at,
            label_width: edge.label_width,
            label_height: edge.label_height,
        });
    }

    // The room kept beside each box belongs to what was placed: a note in
    // it stays inside a nested state's frame.
    let kept: Vec<Rect> = slots
        .iter()
        .filter(|slot| slot.real.is_some())
        .map(|slot| match direction {
            Direction::Down => Rect {
                x: slot.cross,
                y: slot.primary,
                w: slot.pack_cross,
                h: slot.box_along,
            },
            Direction::Right => Rect {
                x: slot.primary,
                y: slot.cross,
                w: slot.box_along,
                h: slot.pack_cross,
            },
        })
        .collect();
    normalize(rects, routes, frame_rects, &kept)
}

/// A frame on screen: across the flow between its borders, along it from
/// the room before its first rank to the room after its last.
fn frame_rect(
    frame: usize,
    spans: &[nest::Span],
    bands: &[(f32, f32)],
    borders: &[(f32, f32)],
    stacks: &nest::Stacks,
    tree: &nest::Tree,
    direction: Direction,
) -> Rect {
    let Some((first, last)) = spans[frame] else {
        return Rect::default();
    };
    let mut start = bands[first].0 - stacks.before[frame];
    let mut end = bands[last].1 + stacks.after[frame];
    let short = tree.pads[frame].min_along - (end - start);
    if short > 0.0 {
        start -= short / 2.0;
        end += short / 2.0;
    }
    let (left, right) = borders[frame];
    match direction {
        Direction::Down => Rect {
            x: left,
            y: start,
            w: right - left,
            h: end - start,
        },
        Direction::Right => Rect {
            x: start,
            y: left,
            w: end - start,
            h: right - left,
        },
    }
}

/// Ranks and which edges run backward. A self-loop is neither.
fn ranks(n: usize, edges: &[Edge]) -> (Vec<usize>, Vec<bool>) {
    let mut back = vec![false; edges.len()];
    let mut adj = vec![Vec::new(); n];
    for (index, edge) in edges.iter().enumerate() {
        if edge.from == edge.to || edge.from >= n || edge.to >= n {
            continue;
        }
        adj[edge.from].push((edge.to, index));
    }
    let mut color = vec![0u8; n];
    for start in 0..n {
        if color[start] == 0 {
            mark(start, &adj, &mut color, &mut back);
        }
    }

    let mut succ = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for (index, edge) in edges.iter().enumerate() {
        if edge.from == edge.to || back[index] || edge.from >= n || edge.to >= n {
            continue;
        }
        succ[edge.from].push(edge.to);
        indeg[edge.to] += 1;
    }
    let mut rank = vec![0usize; n];
    let mut queue: std::collections::VecDeque<usize> =
        (0..n).filter(|&node| indeg[node] == 0).collect();
    while let Some(node) = queue.pop_front() {
        for &next in &succ[node] {
            rank[next] = rank[next].max(rank[node] + 1);
            indeg[next] -= 1;
            if indeg[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    (rank, back)
}

fn mark(node: usize, adj: &[Vec<(usize, usize)>], color: &mut [u8], back: &mut [bool]) {
    color[node] = 1;
    for &(next, index) in &adj[node] {
        if color[next] == 1 {
            back[index] = true;
        } else if color[next] == 0 {
            mark(next, adj, color, back);
        }
    }
    color[node] = 2;
}

struct Slot {
    real: Option<usize>,
    /// The drawn box, across the flow.
    box_cross: f32,
    /// The drawn box, along the flow.
    box_along: f32,
    /// The room kept before the box, across the flow.
    before: f32,
    /// `box_cross` plus the room kept on both sides.
    pack_cross: f32,
    preds: Vec<usize>,
    succs: Vec<usize>,
    cross: f32,
    primary: f32,
    /// Where a line ends on a frame's border: a point in the frame's first
    /// rank or its last, which the line stops short of at the border. The
    /// frame, and that rank.
    anchor: Option<(usize, usize)>,
}

impl Slot {
    fn point(real: Option<usize>, anchor: Option<(usize, usize)>) -> Slot {
        Slot {
            real,
            box_cross: 0.0,
            box_along: 0.0,
            before: 0.0,
            pack_cross: 0.0,
            preds: Vec::new(),
            succs: Vec::new(),
            cross: 0.0,
            primary: 0.0,
            anchor,
        }
    }
}

/// The slots, by rank, and what each edge's line passes through.
struct Built {
    slots: Vec<Slot>,
    rows: Vec<Vec<usize>>,
    /// For every edge the slots its line passes through, down the ranks: a
    /// back edge's from its target to its source. Empty for a self-loop
    /// and a hidden back edge.
    chains: Vec<Vec<usize>>,
    /// The rank each chain starts at.
    first: Vec<usize>,
    /// The frames around each slot, outermost first.
    frames: Vec<Vec<usize>>,
}

fn slots_of(
    nodes: &[Node],
    edges: &[Edge],
    rank: &[usize],
    back: &[bool],
    direction: Direction,
    nested: Option<(&nest::Tree, &[nest::Span])>,
) -> Built {
    let n = nodes.len();
    let rows_len = rank.iter().copied().max().unwrap_or(0) + 1;
    let mut slots = Vec::with_capacity(n);
    let mut frames = Vec::with_capacity(n);
    let mut rows = vec![Vec::new(); rows_len];
    let mut real_slot = vec![0usize; n];
    for (index, node) in nodes.iter().enumerate() {
        real_slot[index] = slots.len();
        let (box_cross, box_along) = match direction {
            Direction::Down => (node.width, node.height),
            Direction::Right => (node.height, node.width),
        };
        slots.push(Slot {
            box_cross,
            box_along,
            before: node.before,
            pack_cross: node.before + box_cross + node.loop_extra,
            ..Slot::point(Some(index), None)
        });
        frames.push(nested.map_or_else(Vec::new, |(tree, _)| tree.holders[index].clone()));
        rows[rank[index]].push(real_slot[index]);
    }

    use nest::End;
    let end_of = |index: usize| match nested {
        Some((tree, _)) => tree.end(index),
        None => (index < n).then_some(End::Node(index)),
    };
    // The rank a line leaves an end at, or enters it at.
    let rank_of = |end: End, leaving: bool| match (end, nested) {
        (End::Node(x), _) => Some(rank[x]),
        (End::Frame(f), Some((_, spans))) => spans[f].map(|(a, b)| if leaving { b } else { a }),
        (End::Frame(_), None) => None,
    };
    let mut chains = vec![Vec::new(); edges.len()];
    let mut first = vec![0; edges.len()];
    for (index, edge) in edges.iter().enumerate() {
        // A hidden back edge only breaks a cycle; its line is routed later
        // and should not pull the ranks it would pass.
        let (Some(from), Some(to)) = (end_of(edge.from), end_of(edge.to)) else {
            continue;
        };
        if from == to || back[index] && edge.hidden {
            continue;
        }
        let (top, bottom) = if back[index] { (to, from) } else { (from, to) };
        let (Some(upper), Some(lower)) = (rank_of(top, true), rank_of(bottom, false)) else {
            continue;
        };
        if lower <= upper {
            continue;
        }
        // A line to a frame ends at a point of its own in the frame's
        // first rank, or starts at one in its last.
        let slot_of = |end: End,
                       leaving: bool,
                       slots: &mut Vec<Slot>,
                       frames: &mut Vec<Vec<usize>>,
                       rows: &mut Vec<Vec<usize>>| match end {
            End::Node(x) => real_slot[x],
            End::Frame(f) => {
                let slot = slots.len();
                let at = if leaving { upper } else { lower };
                slots.push(Slot::point(None, Some((f, at))));
                frames.push(nested.map_or_else(Vec::new, |(tree, _)| tree.chain(Some(f))));
                rows[at].push(slot);
                slot
            }
        };
        let mut chain = vec![slot_of(top, true, &mut slots, &mut frames, &mut rows)];
        for (at, row) in rows.iter_mut().enumerate().take(lower).skip(upper + 1) {
            let dummy = slots.len();
            slots.push(Slot::point(None, None));
            frames.push(nested.map_or_else(Vec::new, |(tree, spans)| {
                tree.chain(nest::passing(
                    tree,
                    spans,
                    (top, bottom),
                    (upper, lower),
                    at,
                ))
            }));
            row.push(dummy);
            link(&mut slots, *chain.last().unwrap(), dummy);
            chain.push(dummy);
        }
        let end = slot_of(bottom, false, &mut slots, &mut frames, &mut rows);
        link(&mut slots, *chain.last().unwrap(), end);
        chain.push(end);
        chains[index] = chain;
        first[index] = upper;
    }
    Built {
        slots,
        rows,
        chains,
        first,
        frames,
    }
}

fn link(slots: &mut [Slot], from: usize, to: usize) {
    slots[from].succs.push(to);
    slots[to].preds.push(from);
}

/// A few down and up passes. A node with no neighbor keeps its place.
fn sweep(rows: &mut [Vec<usize>], slots: &[Slot]) {
    for _ in 0..4 {
        for rank in 1..rows.len() {
            let pos = positions(rows, slots.len());
            sort_row(&mut rows[rank], slots, &pos, true);
        }
        for rank in (0..rows.len().saturating_sub(1)).rev() {
            let pos = positions(rows, slots.len());
            sort_row(&mut rows[rank], slots, &pos, false);
        }
    }
}

/// The same with frames: each rank keeps the slots of a frame together,
/// and frames side by side in one order on every rank.
fn sweep_framed(rows: &mut [Vec<usize>], slots: &[Slot], frames: &[Vec<usize>], count: usize) {
    let pos = nest::positions(rows, slots);
    let keys = nest::keys(rows, &pos, frames, count);
    for row in rows.iter_mut() {
        nest::sort_row(row, slots, &pos, nest::By::Place, frames, &keys);
    }
    for _ in 0..4 {
        let pos = nest::positions(rows, slots);
        let keys = nest::keys(rows, &pos, frames, count);
        for rank in 1..rows.len() {
            let pos = nest::positions(rows, slots);
            nest::sort_row(&mut rows[rank], slots, &pos, nest::By::Preds, frames, &keys);
        }
        let pos = nest::positions(rows, slots);
        let keys = nest::keys(rows, &pos, frames, count);
        for rank in (0..rows.len().saturating_sub(1)).rev() {
            let pos = nest::positions(rows, slots);
            nest::sort_row(&mut rows[rank], slots, &pos, nest::By::Succs, frames, &keys);
        }
    }
    // Last, by the ranks on both sides, so a line passing through lies
    // between where it comes from and where it goes.
    let pos = nest::positions(rows, slots);
    let keys = nest::keys(rows, &pos, frames, count);
    for rank in 0..rows.len() {
        let pos = nest::positions(rows, slots);
        nest::sort_row(&mut rows[rank], slots, &pos, nest::By::Both, frames, &keys);
    }
}

fn positions(rows: &[Vec<usize>], n: usize) -> Vec<f32> {
    let mut pos = vec![0.0; n];
    for row in rows {
        for (index, &slot) in row.iter().enumerate() {
            pos[slot] = index as f32;
        }
    }
    pos
}

fn sort_row(row: &mut [usize], slots: &[Slot], pos: &[f32], by_preds: bool) {
    let mut keyed: Vec<(f32, usize, usize)> = row
        .iter()
        .enumerate()
        .map(|(index, &slot)| {
            let neighbors = if by_preds {
                &slots[slot].preds
            } else {
                &slots[slot].succs
            };
            let bary = if neighbors.is_empty() {
                index as f32
            } else {
                neighbors.iter().map(|&n| pos[n]).sum::<f32>() / neighbors.len() as f32
            };
            (bary, index, slot)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (index, (_, _, slot)) in keyed.into_iter().enumerate() {
        row[index] = slot;
    }
}

/// Places every slot across the flow.
fn place_across(slots: &mut [Slot], rows: &[Vec<usize>]) {
    let widths: Vec<f32> = rows.iter().map(|row| row_width(slots, row)).collect();
    let max_width = widths.iter().copied().fold(0.0, f32::max);
    for (row, width) in rows.iter().zip(widths) {
        let mut cursor = (max_width - width) / 2.0;
        for (index, &slot) in row.iter().enumerate() {
            slots[slot].cross = cursor;
            cursor += slots[slot].pack_cross;
            if let Some(&next) = row.get(index + 1) {
                cursor += gap(slots, slot, next);
            }
        }
    }
    straighten(slots, rows);
}

/// The same with frames, which keep their padding and stay apart on every
/// rank they share ([`nest::Placer`]); the frames' borders across the
/// flow.
fn place_framed(
    nodes: &[Node],
    slots: &mut [Slot],
    rows: &[Vec<usize>],
    frames: &[Vec<usize>],
    tree: &nest::Tree,
) -> Vec<(f32, f32)> {
    let flat: Vec<bool> = slots
        .iter()
        .map(|slot| slot.real.is_some_and(|real| nodes[real].flat))
        .collect();
    let sep = |slots: &[Slot], a: usize, b: usize| nest::sep(slots, frames, tree, a, b);
    let widths: Vec<f32> = rows
        .iter()
        .map(|row| {
            let packed: f32 = row.iter().map(|&slot| slots[slot].pack_cross).sum();
            packed + row.windows(2).map(|w| sep(slots, w[0], w[1])).sum::<f32>()
        })
        .collect();
    let max_width = widths.iter().copied().fold(0.0, f32::max);
    for (row, width) in rows.iter().zip(widths) {
        let mut cursor = (max_width - width) / 2.0;
        for (index, &slot) in row.iter().enumerate() {
            slots[slot].cross = cursor;
            cursor += slots[slot].pack_cross;
            if let Some(&next) = row.get(index + 1) {
                cursor += sep(slots, slot, next);
            }
        }
    }
    let placer = nest::Placer::new(slots, rows, frames, tree);
    let mut borders = placer.settle(slots, 1.0);
    for pass in 0..STRAIGHTEN_PASSES {
        let down = pass % 2 == 0;
        let ranks: Vec<usize> = if down {
            (1..rows.len()).collect()
        } else {
            (0..rows.len().saturating_sub(1)).rev().collect()
        };
        for rank in ranks {
            let wants = row_wants(slots, &rows[rank], down, &flat);
            placer.fit(slots, &mut borders, &wants);
        }
    }
    placer.settle(slots, nest::HELD)
}

/// Where the slots of `row` want to lie, as [`fit_row`] reckons it: the
/// left of the room each packs, and how much it pulls.
///
/// A line passing through heads for where it can still meet the flat box
/// it comes from or goes to, not for the box's middle, so it stays
/// straight where it already reaches the box.
fn row_wants(slots: &[Slot], row: &[usize], down: bool, flat: &[bool]) -> Vec<(usize, f32, f32)> {
    row.iter()
        .map(|&slot| {
            let s = &slots[slot];
            let (near, far) = if down {
                (&s.preds, &s.succs)
            } else {
                (&s.succs, &s.preds)
            };
            let neighbors = if near.is_empty() { far } else { near };
            let own = center(s);
            let toward = |n: usize| {
                let other = &slots[n];
                let at = center(other);
                if s.real.is_some() || !flat[n] {
                    return at;
                }
                let reach =
                    (other.box_cross / 2.0 - PORT_CORNER.min(other.box_cross / 4.0)).max(0.0);
                own.clamp(at - reach, at + reach)
            };
            let (want, weight) = if neighbors.is_empty() {
                (own, 0.05)
            } else {
                let sum: f32 = neighbors.iter().map(|&n| toward(n)).sum();
                let weight = if s.real.is_none() { 2.0 } else { 1.0 };
                (sum / neighbors.len() as f32, weight)
            };
            (slot, want - s.before - s.box_cross / 2.0, weight)
        })
        .collect()
}

/// Places every slot along the flow, the ranks `gaps` apart; where each
/// rank starts and ends.
fn place_along(slots: &mut [Slot], rows: &[Vec<usize>], gaps: &[f32]) -> Vec<(f32, f32)> {
    let mut bands = Vec::with_capacity(rows.len());
    let mut primary = 0.0;
    for (index, row) in rows.iter().enumerate() {
        let along = row
            .iter()
            .map(|&slot| slots[slot].box_along)
            .fold(0.0, f32::max);
        for &slot in row {
            slots[slot].primary = primary + (along - slots[slot].box_along) / 2.0;
        }
        bands.push((primary, primary + along));
        primary += along;
        if index + 1 < rows.len() {
            primary += gaps[index];
        }
    }
    bands
}

/// For every forward edge, the track it runs across each gap on; and how
/// many tracks each gap needs. Only a drawn line that changes place across
/// the flow needs one; a hidden edge only ranks its ends. Lines moving the
/// same way take tracks in the order that keeps one from crossing another:
/// of two heading right, the one that ends further right runs first, and
/// of two that end together the one that starts nearer; to the left the
/// same, mirrored.
fn tracks(
    slots: &[Slot],
    edges: &[Edge],
    chains: &[Vec<usize>],
    ports: &[Ports],
    first: &[usize],
    jogs: &nest::Jogs,
    gaps: usize,
) -> (Vec<Vec<usize>>, Vec<usize>) {
    let cross = |slot: usize, port: f32| center(&slots[slot]) + port;
    let mut tracks: Vec<Vec<usize>> = chains
        .iter()
        .map(|c| vec![0; c.len().saturating_sub(1)])
        .collect();
    // By gap: (edge, step, start, end) of each line that moves across.
    let mut movers: Vec<Vec<(usize, usize, f32, f32)>> = vec![Vec::new(); gaps];
    for (index, chain) in chains.iter().enumerate() {
        if chain.len() < 2 || edges[index].hidden {
            continue;
        }
        let last = chain.len() - 1;
        for step in 0..last {
            let start = cross(
                chain[step],
                if step == 0 { ports[index].start } else { 0.0 },
            );
            // A line passing a frame's name heads beside it first.
            let end = match jogs.get(&(index, step)) {
                Some(way) => way[0].above,
                None => cross(
                    chain[step + 1],
                    if step + 1 == last {
                        ports[index].end
                    } else {
                        0.0
                    },
                ),
            };
            let gap = first[index] + step;
            if (end - start).abs() > 0.5 && gap < gaps {
                movers[gap].push((index, step, start, end));
            }
        }
    }
    let mut counts = vec![0; gaps];
    for (gap, mut lines) in movers.into_iter().enumerate() {
        lines.sort_by(|a, b| {
            let (right_a, right_b) = (a.3 > a.2, b.3 > b.2);
            right_b.cmp(&right_a).then_with(|| {
                if right_a {
                    b.3.total_cmp(&a.3).then(b.2.total_cmp(&a.2))
                } else {
                    a.3.total_cmp(&b.3).then(a.2.total_cmp(&b.2))
                }
            })
        });
        counts[gap] = lines.len();
        for (track, (index, step, ..)) in lines.into_iter().enumerate() {
            tracks[index][step] = track;
        }
    }
    (tracks, counts)
}

fn row_width(slots: &[Slot], row: &[usize]) -> f32 {
    let mut width = 0.0;
    for (index, &slot) in row.iter().enumerate() {
        width += slots[slot].pack_cross;
        if let Some(&next) = row.get(index + 1) {
            width += gap(slots, slot, next);
        }
    }
    width
}

/// Between two neighbors of a rank: less between two lines passing by.
fn gap(slots: &[Slot], a: usize, b: usize) -> f32 {
    if slots[a].real.is_none() && slots[b].real.is_none() {
        DUMMY_GAP
    } else {
        CROSS_GAP
    }
}

/// Moves the slots of every rank across the flow toward the slots they
/// connect to, keeping their order and gaps, so a line between two boxes
/// runs straight where it can. A few passes down and up; each fits one
/// rank by least squares under its order.
fn straighten(slots: &mut [Slot], rows: &[Vec<usize>]) {
    for pass in 0..STRAIGHTEN_PASSES {
        let down = pass % 2 == 0;
        let ranks: Vec<usize> = if down {
            (1..rows.len()).collect()
        } else {
            (0..rows.len().saturating_sub(1)).rev().collect()
        };
        for rank in ranks {
            fit_row(slots, &rows[rank], down);
        }
    }
}

fn center(slot: &Slot) -> f32 {
    slot.cross + slot.before + slot.box_cross / 2.0
}

/// Places `row` as near as its order allows to where its neighbors are:
/// those in the rank before it when going `down`, else those after. A
/// line passing through counts double, so long lines stay straight; a box
/// with no neighbor barely pulls and stays put.
fn fit_row(slots: &mut [Slot], row: &[usize], down: bool) {
    let mut targets = Vec::with_capacity(row.len());
    let mut offset = 0.0;
    for (index, &slot) in row.iter().enumerate() {
        let s = &slots[slot];
        let (near, far) = if down {
            (&s.preds, &s.succs)
        } else {
            (&s.succs, &s.preds)
        };
        let neighbors = if near.is_empty() { far } else { near };
        let (want, weight) = if neighbors.is_empty() {
            (center(s), 0.05)
        } else {
            let sum: f32 = neighbors.iter().map(|&n| center(&slots[n])).sum();
            let weight = if s.real.is_none() { 2.0 } else { 1.0 };
            (sum / neighbors.len() as f32, weight)
        };
        targets.push((want - s.before - s.box_cross / 2.0 - offset, weight));
        offset += s.pack_cross;
        if let Some(&next) = row.get(index + 1) {
            offset += gap(slots, slot, next);
        }
    }
    let fitted = monotone(&targets);
    let mut offset = 0.0;
    for (index, &slot) in row.iter().enumerate() {
        slots[slot].cross = fitted[index] + offset;
        offset += slots[slot].pack_cross;
        if let Some(&next) = row.get(index + 1) {
            offset += gap(slots, slot, next);
        }
    }
}

/// The non-decreasing values nearest to the weighted `targets`, by least
/// squares (pool adjacent violators).
fn monotone(targets: &[(f32, f32)]) -> Vec<f32> {
    // Pooled runs: value, weight, length.
    let mut pools: Vec<(f32, f32, usize)> = Vec::with_capacity(targets.len());
    for &(value, weight) in targets {
        pools.push((value, weight, 1));
        while pools.len() >= 2 && pools[pools.len() - 2].0 > pools[pools.len() - 1].0 {
            let (v2, w2, n2) = pools.pop().unwrap_or_default();
            let (v1, w1, n1) = pools.pop().unwrap_or_default();
            let weight = w1 + w2;
            pools.push(((v1 * w1 + v2 * w2) / weight, weight, n1 + n2));
        }
    }
    pools
        .into_iter()
        .flat_map(|(value, _, len)| std::iter::repeat_n(value, len))
        .collect()
}

/// How much room the labels need in the gap after each rank.
fn rank_gaps(
    ranks: usize,
    edges: &[Edge],
    chains: &[Vec<usize>],
    first: &[usize],
    direction: Direction,
) -> Vec<f32> {
    let mut gaps = vec![ALONG_PAD; ranks.saturating_sub(1)];
    for ((edge, chain), &top) in edges.iter().zip(chains).zip(first) {
        if chain.is_empty() {
            continue;
        }
        let bottom = top + chain.len() - 1;
        if bottom == top {
            continue;
        }
        // The label sits in the gap nearest the middle of a long edge.
        let at = top + (bottom - top) / 2;
        let at = at.min(gaps.len().saturating_sub(1));
        let need = match direction {
            Direction::Down => edge.label_height,
            Direction::Right => edge.label_width,
        };
        if let Some(gap) = gaps.get_mut(at) {
            *gap = gap.max(ALONG_PAD + need);
        }
    }
    gaps
}

fn rect_of(slots: &[Slot], real: usize, direction: Direction) -> Rect {
    let slot = slots.iter().find(|s| s.real == Some(real)).unwrap();
    match direction {
        Direction::Down => Rect {
            x: slot.cross + slot.before,
            y: slot.primary,
            w: slot.box_cross,
            h: slot.box_along,
        },
        Direction::Right => Rect {
            x: slot.primary,
            y: slot.cross + slot.before,
            w: slot.box_along,
            h: slot.box_cross,
        },
    }
}

/// Where a line meets the boxes at the two ends of its chain, across the
/// flow from the middle of each side, and which side of it its label
/// takes.
#[derive(Debug, Clone, Copy, Default)]
struct Ports {
    start: f32,
    end: f32,
    /// Left of the line rather than right when the flow runs down, below
    /// rather than above when it runs across: the first line of several
    /// into one side, whose neighbors lie on the usual side of it.
    flip_label: bool,
}

/// Where every line meets its boxes. The lines that meet one side of a
/// box share it in the order of their other ends, so none crosses another
/// there, [`PORT_STEP`] apart where the side is long enough, and further
/// where a label lies between two of them. Each lies as near as that
/// order allows to where it heads: where the box at its other end is flat
/// too and the two overlap across the flow, the middle of the overlap, so
/// the line runs straight; otherwise the point facing its other end. A
/// circle or a diamond is met in the middle of its side; only lines
/// between the same two boxes lie side by side there. A few more passes
/// then move each end toward where the other end of its line lies, so a
/// pair squeezed onto a short side runs straight to a long one too.
fn ports(
    nodes: &[Node],
    slots: &[Slot],
    chains: &[Vec<usize>],
    edges: &[Edge],
    direction: Direction,
) -> Vec<Ports> {
    let mut ports = vec![Ports::default(); chains.len()];
    let flat: Vec<bool> = slots
        .iter()
        .map(|slot| slot.real.is_some_and(|real| nodes[real].flat))
        .collect();
    // At each slot, the lines leaving it and those arriving: (edge, the
    // slot at the other end of the step), in the order of the other ends.
    let mut sides: Vec<[Vec<(usize, usize)>; 2]> =
        (0..slots.len()).map(|_| Default::default()).collect();
    for (index, chain) in chains.iter().enumerate() {
        if chain.len() < 2 || edges[index].hidden {
            continue;
        }
        let last = chain.len() - 1;
        sides[chain[0]][0].push((index, chain[1]));
        sides[chain[last]][1].push((index, chain[last - 1]));
    }
    for side in sides.iter_mut().flatten() {
        side.sort_by(|a, b| {
            center(&slots[a.1])
                .total_cmp(&center(&slots[b.1]))
                .then(a.0.cmp(&b.0))
        });
    }
    for pair in &sides {
        let side = &pair[1];
        let flipped = match direction {
            Direction::Down => side.first(),
            Direction::Right => side.last(),
        };
        if let (Some(&(index, _)), true) = (flipped, side.len() > 1) {
            ports[index].flip_label = true;
        }
    }
    // How much room the label of a line takes beside it, above or below,
    // when the flow runs across. Where it runs down, labels are too wide
    // to make room for; one in the way moves along its line later.
    let label_room = |index: usize, below: bool| {
        let edge = &edges[index];
        let beside = direction == Direction::Right
            && chains[index].len() == 2
            && edge.label_height > 0.0
            && ports[index].flip_label == below;
        if beside { edge.label_height + 8.0 } else { 0.0 }
    };
    // How far apart neighbors on each side keep. On a flat side, room for
    // a label between them; on a circle or a diamond, lines to other boxes
    // meet in the middle unless lines side by side would cross them.
    let steps: Vec<[Vec<f32>; 2]> = sides
        .iter()
        .enumerate()
        .map(|(slot, pair)| {
            let flat = flat[slot];
            pair.clone().map(|side| {
                let paired = side.windows(2).any(|lines| lines[0].1 == lines[1].1);
                side.windows(2)
                    .map(|lines| match (flat, lines[0].1 == lines[1].1) {
                        (true, _) => PORT_STEP
                            .max(label_room(lines[0].0, true) + label_room(lines[1].0, false)),
                        (false, true) => PORT_STEP,
                        (false, false) if paired => PORT_STEP / 2.0,
                        (false, false) => 0.0,
                    })
                    .collect()
            })
        })
        .collect();
    // How far from its middle a line may meet each side.
    let reach: Vec<f32> = slots
        .iter()
        .zip(&flat)
        .map(|(slot, &flat)| {
            if flat {
                let along = slot.box_cross;
                (along / 2.0 - PORT_CORNER.min(along / 4.0)).max(0.0)
            } else {
                f32::INFINITY
            }
        })
        .collect();
    // Short sides first, which have the least room, so the long ones
    // follow them.
    let mut order: Vec<usize> = (0..slots.len())
        .filter(|&slot| slots[slot].real.is_some())
        .collect();
    order.sort_by(|&a, &b| slots[a].box_cross.total_cmp(&slots[b].box_cross));
    for pass in 0..PORT_PASSES {
        for &slot in &order {
            let own = center(&slots[slot]);
            let half = reach[slot];
            for (end, side) in sides[slot].iter().enumerate() {
                let wants: Vec<f32> = side
                    .iter()
                    .map(|&(index, other)| {
                        let far_flat = flat[other];
                        let at = center(&slots[other]);
                        if half.is_infinite() {
                            0.0
                        } else if !far_flat {
                            at - own
                        } else if pass > 0 {
                            // Where the other end lies now.
                            let there = if end == 0 {
                                ports[index].end
                            } else {
                                ports[index].start
                            };
                            at + there - own
                        } else {
                            let (lo, hi) = (
                                (own - half).max(at - reach[other]),
                                (own + half).min(at + reach[other]),
                            );
                            if lo <= hi {
                                ((own + at) / 2.0).clamp(lo, hi) - own
                            } else {
                                at - own
                            }
                        }
                    })
                    .collect();
                let offsets = share(&wants, &steps[slot][end], half);
                for (&(index, _), offset) in side.iter().zip(offsets) {
                    if end == 0 {
                        ports[index].start = offset;
                    } else {
                        ports[index].end = offset;
                    }
                }
            }
        }
    }
    ports
}

/// Offsets from `-half` to `half` for lines along a side, in the order
/// given: at least `steps` apart (squeezed where the side is too short),
/// and as near to `wants` as that allows (least squares).
fn share(wants: &[f32], steps: &[f32], half: f32) -> Vec<f32> {
    let total: f32 = steps.iter().sum();
    let squeeze = if total > 2.0 * half {
        2.0 * half / total
    } else {
        1.0
    };
    let mut before = Vec::with_capacity(wants.len());
    let mut sum = 0.0;
    for at in 0..wants.len() {
        before.push(sum);
        sum += steps.get(at).copied().unwrap_or(0.0) * squeeze;
    }
    let targets: Vec<(f32, f32)> = wants
        .iter()
        .zip(&before)
        .map(|(&want, &before)| (want.clamp(-half, half) - before, 1.0))
        .collect();
    monotone(&targets)
        .into_iter()
        .zip(&before)
        .map(|(value, &before)| {
            (value + before)
                .max(before - half)
                .min(half - (sum - before))
        })
        .collect()
}

/// Where a line passes the name of a frame it comes into from above, in
/// the gap after rank `step` of its chain: it comes down `beside` the
/// name and turns back to where it heads at height `y`.
struct Pass {
    step: usize,
    beside: f32,
    back: f32,
    y: f32,
}

/// Along the chain, bending in each gap at `bends`, a little past the rank
/// before it and on the line's track; `ports` moves the ends across the
/// flow. The label sits beside the line after the bend, in the gap
/// [`rank_gaps`] made room in.
fn forward_route(
    chain: &[usize],
    slots: &[Slot],
    ends: &[Option<Rect>],
    (bends, passes): (&[f32], &[Pass]),
    edge: &Edge,
    ports: Ports,
    direction: Direction,
) -> (Vec<(f32, f32)>, (f32, f32)) {
    if chain.len() < 2 {
        return (Vec::new(), (0.0, 0.0));
    }
    let across = |(x, y): (f32, f32), by: f32| match direction {
        Direction::Down => (x + by, y),
        Direction::Right => (x, y + by),
    };
    let point = |index: usize| {
        let slot = &slots[chain[index]];
        match ends[chain[index]] {
            Some(rect) if index == 0 => across(exit_point(rect, direction), ports.start),
            Some(rect) => across(entry_point(rect, direction), ports.end),
            // A dummy is a point the edge passes through.
            None => match direction {
                Direction::Down => (slot.cross, slot.primary),
                Direction::Right => (slot.primary, slot.cross),
            },
        }
    };
    let labeled = edge.label_width > 0.0 || edge.label_height > 0.0;
    let label_step = (chain.len() - 1) / 2;
    // Only beside the last step do other lines into the same side lie on
    // the usual side of this one.
    let flip = ports.flip_label && chain.len() == 2;
    let mut label_at = (0.0, 0.0);
    let mut points = vec![point(0)];
    for (step, &bend) in bends.iter().enumerate().take(chain.len() - 1) {
        let next = point(step + 1);
        let prev = points[points.len() - 1];
        let way: Vec<&Pass> = passes.iter().filter(|pass| pass.step == step).collect();
        // Beside the name of a frame it comes into, then back below it.
        let heading = way.first().map_or(next.0, |pass| pass.beside);
        match direction {
            Direction::Down => {
                points.push((prev.0, bend));
                points.push((heading, bend));
                for pass in &way {
                    points.push((pass.beside, pass.y));
                    points.push((pass.back, pass.y));
                }
            }
            Direction::Right => {
                points.push((bend, prev.1));
                points.push((bend, next.1));
            }
        }
        if labeled && step == label_step {
            label_at = match (direction, flip) {
                (Direction::Down, false) => (heading + 6.0, bend + LABEL_DROP),
                (Direction::Down, true) => (heading - 6.0 - edge.label_width, bend + LABEL_DROP),
                (Direction::Right, false) => (bend + LABEL_DROP, next.1 - edge.label_height - 4.0),
                (Direction::Right, true) => (bend + LABEL_DROP, next.1 + 4.0),
            };
        }
        points.push(next);
    }
    (simplify(points), label_at)
}

fn exit_point(rect: Rect, direction: Direction) -> (f32, f32) {
    match direction {
        Direction::Down => (rect.cx(), rect.bottom()),
        Direction::Right => (rect.right(), rect.cy()),
    }
}

fn entry_point(rect: Rect, direction: Direction) -> (f32, f32) {
    match direction {
        Direction::Down => (rect.cx(), rect.y),
        Direction::Right => (rect.x, rect.cy()),
    }
}

fn self_loop(rect: Rect, channel: usize, direction: Direction) -> Vec<(f32, f32)> {
    let reach = 16.0 + channel as f32 * 12.0;
    match direction {
        Direction::Down => {
            let x = rect.right();
            let y = rect.cy();
            vec![
                (x, y - 7.0),
                (x + reach, y - 7.0),
                (x + reach, y + 7.0),
                (x, y + 7.0),
            ]
        }
        Direction::Right => {
            let x = rect.cx();
            let y = rect.bottom();
            vec![
                (x - 7.0, y),
                (x - 7.0, y + reach),
                (x + 7.0, y + reach),
                (x + 7.0, y),
            ]
        }
    }
}

fn orthogonal(points: &[(f32, f32)], direction: Direction) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(points.len() * 2);
    for (index, &point) in points.iter().enumerate() {
        if index == 0 {
            out.push(point);
            continue;
        }
        let prev = *out.last().unwrap();
        if (prev.0 - point.0).abs() > 0.01 && (prev.1 - point.1).abs() > 0.01 {
            match direction {
                Direction::Down => {
                    let y = (prev.1 + point.1) / 2.0;
                    out.push((prev.0, y));
                    out.push((point.0, y));
                }
                Direction::Right => {
                    let x = (prev.0 + point.0) / 2.0;
                    out.push((x, prev.1));
                    out.push((x, point.1));
                }
            }
        }
        if out.last() != Some(&point) {
            out.push(point);
        }
    }
    simplify(out)
}

/// Drops a point that sits on the straight line between its neighbors.
fn simplify(points: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = Vec::with_capacity(points.len());
    for point in points {
        while out.len() >= 2 && between(out[out.len() - 2], out[out.len() - 1], point) {
            out.pop();
        }
        if out.last() != Some(&point) {
            out.push(point);
        }
    }
    out
}

fn between(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let vertical = (a.0 - b.0).abs() < 0.01 && (b.0 - c.0).abs() < 0.01;
    let horizontal = (a.1 - b.1).abs() < 0.01 && (b.1 - c.1).abs() < 0.01;
    if vertical {
        (b.1 - a.1) * (c.1 - b.1) >= -0.01 && (a.1 - c.1).abs() > 0.01
    } else if horizontal {
        (b.0 - a.0) * (c.0 - b.0) >= -0.01 && (a.0 - c.0).abs() > 0.01
    } else {
        false
    }
}

/// Beside a self-loop, on its far side.
fn loop_label(points: &[(f32, f32)], edge: &Edge, direction: Direction) -> (f32, f32) {
    if edge.label_width == 0.0 && edge.label_height == 0.0 || points.len() < 2 {
        return (0.0, 0.0);
    }
    let mid_x = points.iter().map(|p| p.0).sum::<f32>() / points.len() as f32;
    let mid_y = points.iter().map(|p| p.1).sum::<f32>() / points.len() as f32;
    let far_x = points.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
    let far_y = points.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
    match direction {
        Direction::Down => (far_x + 4.0, mid_y - edge.label_height / 2.0),
        Direction::Right => (mid_x - edge.label_width / 2.0, far_y + 4.0),
    }
}

fn normalize(
    mut nodes: Vec<Rect>,
    mut routes: Vec<Route>,
    mut frames: Vec<Rect>,
    kept: &[Rect],
) -> Layout {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut grow = |x: f32, y: f32| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };
    for rect in nodes.iter().chain(&frames).chain(kept) {
        grow(rect.x, rect.y);
        grow(rect.right(), rect.bottom());
    }
    for route in &routes {
        for &(x, y) in &route.points {
            grow(x, y);
        }
        if route.label_width > 0.0 || route.label_height > 0.0 {
            grow(route.label_at.0, route.label_at.1);
            grow(
                route.label_at.0 + route.label_width,
                route.label_at.1 + route.label_height,
            );
        }
    }
    if !min_x.is_finite() {
        return Layout {
            nodes,
            routes,
            frames,
            width: 0.0,
            height: 0.0,
        };
    }
    let (dx, dy) = (-min_x, -min_y);
    for rect in nodes.iter_mut().chain(&mut frames) {
        rect.x += dx;
        rect.y += dy;
    }
    for route in &mut routes {
        for point in &mut route.points {
            point.0 += dx;
            point.1 += dy;
        }
        route.label_at.0 += dx;
        route.label_at.1 += dy;
    }
    Layout {
        nodes,
        routes,
        frames,
        width: max_x + dx,
        height: max_y + dy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(w: f32, h: f32) -> Node {
        Node {
            width: w,
            height: h,
            before: 0.0,
            loop_extra: 0.0,
            flat: true,
        }
    }

    fn edge(from: usize, to: usize) -> Edge {
        Edge {
            from,
            to,
            label_width: 0.0,
            label_height: 0.0,
            hidden: false,
        }
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.right() - 0.5
            && b.x < a.right() - 0.5
            && a.y < b.bottom() - 0.5
            && b.y < a.bottom() - 0.5
    }

    #[test]
    fn a_chain_steps_downward() {
        let nodes = [node(40.0, 20.0), node(40.0, 20.0), node(40.0, 20.0)];
        let edges = [edge(0, 1), edge(1, 2)];
        let laid = layout(&nodes, &edges, Direction::Down);
        assert!(laid.nodes[1].y >= laid.nodes[0].bottom() - 0.1);
        assert!(laid.nodes[2].y >= laid.nodes[1].bottom() - 0.1);
        assert!(!overlaps(laid.nodes[0], laid.nodes[1]));
    }

    #[test]
    fn a_fork_shares_a_rank() {
        let nodes = [node(30.0, 20.0), node(30.0, 20.0), node(30.0, 20.0)];
        let edges = [edge(0, 1), edge(0, 2)];
        let laid = layout(&nodes, &edges, Direction::Down);
        assert!((laid.nodes[1].y - laid.nodes[2].y).abs() < 0.1);
        assert!(laid.nodes[1].y >= laid.nodes[0].bottom() - 0.1);
        assert!(laid.nodes[1].x != laid.nodes[2].x);
        assert!(!overlaps(laid.nodes[1], laid.nodes[2]));
    }

    /// As Graphviz draws it: the way back runs between the two boxes,
    /// beside the way there, not around the diagram.
    #[test]
    fn a_back_edge_runs_beside_its_forward_edge() {
        let nodes = [node(40.0, 20.0), node(40.0, 20.0)];
        let edges = [edge(0, 1), edge(1, 0)];
        let laid = layout(&nodes, &edges, Direction::Down);
        let (top, bottom) = (laid.nodes[0], laid.nodes[1]);
        assert!(bottom.y > top.y);
        assert!(!overlaps(top, bottom));
        for route in &laid.routes {
            for &(x, y) in &route.points {
                assert!(x >= top.x && x <= top.right(), "{:?}", route.points);
                assert!(y >= top.bottom() - 0.1 && y <= bottom.y + 0.1);
            }
        }
        // Up into the box it returns to, on a line of its own.
        let back = &laid.routes[1].points;
        let tip = back[back.len() - 1];
        assert!((tip.1 - top.bottom()).abs() < 0.1, "{back:?}");
        assert!((tip.0 - laid.routes[0].points[0].0).abs() >= PORT_STEP - 0.1);
    }

    /// Lines leaving one side spread along it in the order of where they
    /// go, and two boxes that overlap across the flow are joined by a
    /// straight line.
    #[test]
    fn lines_share_a_side_and_run_straight_where_boxes_overlap() {
        let nodes = [node(120.0, 20.0), node(60.0, 20.0), node(60.0, 20.0)];
        let edges = [edge(0, 1), edge(0, 2)];
        let laid = layout(&nodes, &edges, Direction::Down);
        let starts: Vec<f32> = laid.routes.iter().map(|r| r.points[0].0).collect();
        let (left, right) = if laid.nodes[1].x < laid.nodes[2].x {
            (starts[0], starts[1])
        } else {
            (starts[1], starts[0])
        };
        assert!(right - left >= PORT_STEP, "{starts:?}");
        for route in &laid.routes {
            assert_eq!(route.points.len(), 2, "{:?}", route.points);
            assert!((route.points[0].0 - route.points[1].0).abs() < 0.01);
        }
    }

    /// A diamond is met in the middle of its side; a line to it and the
    /// one back lie side by side, and a line to another box keeps its
    /// order with them.
    #[test]
    fn a_point_side_keeps_its_lines_in_order() {
        let diamond = Node {
            flat: false,
            ..node(40.0, 40.0)
        };
        let nodes = [diamond, node(60.0, 20.0), node(60.0, 20.0)];
        let edges = [edge(0, 1), edge(1, 0), edge(0, 2)];
        let laid = layout(&nodes, &edges, Direction::Down);
        let at_diamond = |route: &Route, first: bool| {
            let points = &route.points;
            if first {
                points[0].0
            } else {
                points[points.len() - 1].0
            }
        };
        let pair = [
            at_diamond(&laid.routes[0], true),
            at_diamond(&laid.routes[1], false),
        ];
        let other = at_diamond(&laid.routes[2], true);
        assert!((pair[0] - pair[1]).abs() >= PORT_STEP - 0.1, "{pair:?}");
        let pair_right = laid.nodes[1].x > laid.nodes[2].x;
        for end in pair {
            assert_eq!(end > other, pair_right, "{pair:?} {other}");
        }
    }

    #[test]
    fn a_self_loop_stays_beside_its_node_and_off_the_ranking() {
        let nodes = [node(40.0, 24.0)];
        let edges = [Edge {
            label_width: 48.0,
            label_height: 12.0,
            ..edge(0, 0)
        }];
        let laid = layout(&nodes, &edges, Direction::Down);
        assert_eq!(laid.nodes.len(), 1);
        let far = laid.routes[0]
            .points
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(far > laid.nodes[0].right());
    }

    /// Two lines across one gap run on two tracks, and the gap grows for
    /// the second; a hidden edge ranks its ends and takes none.
    #[test]
    fn a_hidden_edge_takes_no_track() {
        let nodes = [node(40.0, 20.0), node(40.0, 20.0), node(40.0, 20.0)];
        let gap = |edges: &[Edge]| {
            let laid = layout(&nodes, edges, Direction::Down);
            laid.nodes[1].y - laid.nodes[0].bottom()
        };
        let drawn = gap(&[edge(0, 1), edge(0, 2)]);
        let hidden = gap(&[
            edge(0, 1),
            Edge {
                hidden: true,
                ..edge(0, 2)
            },
        ]);
        assert!((drawn - hidden - TRACK).abs() < 0.1, "{drawn} {hidden}");
    }

    #[test]
    fn left_to_right_steps_across() {
        let nodes = [node(40.0, 20.0), node(40.0, 20.0)];
        let edges = [edge(0, 1)];
        let laid = layout(&nodes, &edges, Direction::Right);
        assert!(laid.nodes[1].x >= laid.nodes[0].right() - 0.1);
        assert!(!overlaps(laid.nodes[0], laid.nodes[1]));
    }

    fn route_enters(points: &[(f32, f32)], rect: Rect) -> bool {
        points
            .windows(2)
            .any(|pair| route::cuts(rect, pair[0], pair[1]))
    }

    #[test]
    fn a_forward_edge_steps_around_a_tall_sibling() {
        let nodes = [node(22.0, 22.0), node(80.0, 160.0), node(70.0, 28.0)];
        let edges = [edge(0, 2)];
        let laid = layout(&nodes, &edges, Direction::Down);
        assert!(
            !route_enters(&laid.routes[0].points, laid.nodes[1]),
            "points {:?}, sibling {:?}",
            laid.routes[0].points,
            laid.nodes[1]
        );
        let end = laid.routes[0].points.last().copied().unwrap();
        assert!((end.0 - laid.nodes[2].cx()).abs() < 0.1);
        assert!((end.1 - laid.nodes[2].y).abs() < 0.1);
    }

    #[test]
    fn a_forward_edge_steps_around_a_wide_sibling() {
        let nodes = [node(22.0, 22.0), node(160.0, 40.0), node(28.0, 70.0)];
        let edges = [edge(0, 2)];
        let laid = layout(&nodes, &edges, Direction::Right);
        assert!(
            !route_enters(&laid.routes[0].points, laid.nodes[1]),
            "points {:?}, sibling {:?}",
            laid.routes[0].points,
            laid.nodes[1]
        );
        let end = laid.routes[0].points.last().copied().unwrap();
        assert!((end.1 - laid.nodes[2].cy()).abs() < 0.1);
        assert!((end.0 - laid.nodes[2].x).abs() < 0.1);
    }

    #[test]
    fn boxes_of_a_diamond_do_not_overlap() {
        let nodes = [
            node(36.0, 22.0),
            node(50.0, 28.0),
            node(44.0, 22.0),
            node(36.0, 22.0),
        ];
        let edges = [edge(0, 1), edge(0, 2), edge(1, 3), edge(2, 3)];
        let laid = layout(&nodes, &edges, Direction::Down);
        for (i, a) in laid.nodes.iter().enumerate() {
            for b in laid.nodes.iter().skip(i + 1) {
                assert!(!overlaps(*a, *b), "{a:?} overlaps {b:?}");
            }
        }
        assert!(laid.nodes[1].y > laid.nodes[0].y);
        assert!(laid.nodes[3].y > laid.nodes[1].y);
    }
}
