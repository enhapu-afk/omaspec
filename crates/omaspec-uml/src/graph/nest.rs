//! Frames: containers laid out together with everything around them, as
//! Graphviz lays out clusters.
//!
//! A frame's boxes are ranked among themselves first, inside out, and the
//! frame then moves down as a whole, as far as the lines between it and
//! what is around it ask ([`ranks`]). Every rank keeps the slots of one
//! frame together, and frames side by side in the same order on every
//! rank ([`sort_row`]). A line passing a rank belongs to the frame around
//! its nearer end that reaches that rank ([`passing`]), so it leaves a
//! frame through the side the flow leaves by, not through another.
//! Across the flow, frames keep their padding and stay clear of each
//! other on every rank they share ([`Placer`]); along it, the gaps between
//! ranks grow by the room above and below the frames that begin and end
//! there ([`Stacks`]).

use std::collections::HashMap;

use super::{
    CROSS_GAP, DUMMY_GAP, Direction, Edge, Frame, Name, PORT_STEP, Ports, Slot, TRACK, center,
};

/// The weight that holds a slot where it is while another rank moves.
pub(super) const HELD: f32 = 50.0;
/// The weight that pulls a frame's border to what it holds.
const LOOSE: f32 = 0.01;

/// A frame's first and last rank; `None` for one that holds no node.
pub(super) type Span = Option<(usize, usize)>;

/// A frame's room along and across the flow.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Pad {
    /// Along the flow, before its first rank: its name when the flow runs
    /// down.
    pub before: f32,
    pub after: f32,
    /// Across the flow: before (left when the flow runs down) and after.
    pub left: f32,
    pub right: f32,
    pub min_cross: f32,
    pub min_along: f32,
}

/// One end of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum End {
    Node(usize),
    Frame(usize),
}

/// The frames, and which hold which.
pub(super) struct Tree {
    /// Changes when a frame has to grow for the lines past its name.
    pub pads: Vec<Pad>,
    pub names: Vec<Name>,
    /// The frames around each frame, outermost first, itself excluded.
    pub around: Vec<Vec<usize>>,
    /// The frames around each node, outermost first.
    pub holders: Vec<Vec<usize>>,
    /// Every node inside each frame, however deep.
    pub members: Vec<Vec<usize>>,
}

impl Tree {
    pub fn new(frames: &[Frame], within: &[Option<usize>], n: usize, direction: Direction) -> Self {
        let count = frames.len();
        let parent = |f: usize| frames[f].parent.filter(|&p| p < count && p != f);
        let mut around = Vec::with_capacity(count);
        for f in 0..count {
            let mut chain = Vec::new();
            let mut at = parent(f);
            while let Some(p) = at {
                // A frame inside itself counts as one at the top.
                if p == f || chain.contains(&p) {
                    chain.clear();
                    break;
                }
                chain.push(p);
                at = parent(p);
            }
            chain.reverse();
            around.push(chain);
        }
        let holders: Vec<Vec<usize>> = (0..n)
            .map(
                |x| match within.get(x).copied().flatten().filter(|&f| f < count) {
                    Some(f) => {
                        let mut chain = around[f].clone();
                        chain.push(f);
                        chain
                    }
                    None => Vec::new(),
                },
            )
            .collect();
        let mut members = vec![Vec::new(); count];
        for (x, chain) in holders.iter().enumerate() {
            for &f in chain {
                members[f].push(x);
            }
        }
        let pads = frames
            .iter()
            .map(|frame| match direction {
                Direction::Down => Pad {
                    before: frame.top,
                    after: frame.bottom,
                    left: frame.left,
                    right: frame.right,
                    min_cross: frame.min_width,
                    min_along: 0.0,
                },
                Direction::Right => Pad {
                    before: frame.left,
                    after: frame.right,
                    left: frame.top,
                    right: frame.bottom,
                    min_cross: 0.0,
                    min_along: frame.min_width,
                },
            })
            .collect();
        Tree {
            pads,
            names: frames.iter().map(|frame| frame.name).collect(),
            around,
            holders,
            members,
        }
    }

    pub fn count(&self) -> usize {
        self.pads.len()
    }

    /// The end an edge names: a node, or past the nodes a frame.
    pub fn end(&self, index: usize) -> Option<End> {
        let n = self.holders.len();
        if index < n {
            Some(End::Node(index))
        } else {
            (index - n < self.count()).then(|| End::Frame(index - n))
        }
    }

    /// The frames around an end, outermost first; a frame is not around
    /// itself.
    pub fn around_end(&self, end: End) -> &[usize] {
        match end {
            End::Node(x) => &self.holders[x],
            End::Frame(f) => &self.around[f],
        }
    }

    /// `frame` and the frames around it, outermost first.
    pub fn chain(&self, frame: Option<usize>) -> Vec<usize> {
        frame.map_or_else(Vec::new, |f| {
            let mut chain = self.around[f].clone();
            chain.push(f);
            chain
        })
    }

    /// Whether `outer` is `inner` or around it.
    fn holds(&self, outer: usize, inner: usize) -> bool {
        outer == inner || self.around[inner].contains(&outer)
    }

    /// The frames right inside each frame.
    fn children(&self) -> Vec<Vec<usize>> {
        let mut children = vec![Vec::new(); self.count()];
        for (f, chain) in self.around.iter().enumerate() {
            if let Some(&p) = chain.last() {
                children[p].push(f);
            }
        }
        children
    }

    /// The frames, the innermost first.
    fn inside_out(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.count()).collect();
        order.sort_by_key(|&f| std::cmp::Reverse(self.around[f].len()));
        order
    }
}

/// What one level of the ranking holds: a node, or a frame as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item {
    Node(usize),
    Frame(usize),
}

/// Ranks of the nodes and which edges run backward. Each frame is ranked
/// on its own, inside out; one level up it is an item that keeps its ranks
/// and moves down as a whole. An edge between two items asks that its
/// lower end lies a rank below its upper one, where it leaves the upper
/// item and enters the lower: an edge into a frame itself enters it at its
/// first rank, one out of it leaves at its last. A depth-first search of
/// each level turns the edges that close a cycle around. An edge between a
/// frame and what it holds is not ranked.
pub(super) fn ranks(tree: &Tree, edges: &[Edge]) -> (Vec<usize>, Vec<bool>) {
    let n = tree.holders.len();
    let count = tree.count();
    let mut back = vec![false; edges.len()];
    // By level, a frame or `count` for the top: (edge, from item, to item).
    let mut levels: Vec<Vec<(usize, Item, Item)>> = vec![Vec::new(); count + 1];
    for (index, edge) in edges.iter().enumerate() {
        let (Some(from), Some(to)) = (tree.end(edge.from), tree.end(edge.to)) else {
            continue;
        };
        if from == to {
            continue;
        }
        let (a, b) = (tree.around_end(from), tree.around_end(to));
        let depth = a.iter().zip(b).take_while(|(x, y)| x == y).count();
        let item = |end: End, chain: &[usize]| match chain.get(depth) {
            Some(&f) => Item::Frame(f),
            None => match end {
                End::Node(x) => Item::Node(x),
                End::Frame(f) => Item::Frame(f),
            },
        };
        let (from_item, to_item) = (item(from, a), item(to, b));
        if from_item == to_item {
            continue;
        }
        let level = if depth == 0 { count } else { a[depth - 1] };
        levels[level].push((index, from_item, to_item));
    }

    let mut rel = vec![0i64; n];
    let mut order = tree.inside_out();
    order.push(count);
    for level in order {
        let depth = if level == count {
            0
        } else {
            tree.around[level].len() + 1
        };
        let right_inside =
            |chain: &[usize]| chain.len() == depth && (depth == 0 || chain[depth - 1] == level);
        let mut items: Vec<(usize, Item)> = (0..n)
            .filter(|&x| right_inside(&tree.holders[x]))
            .map(|x| (x, Item::Node(x)))
            .chain(
                (0..count)
                    .filter(|&f| {
                        tree.around[f].len() == depth
                            && (depth == 0 || tree.around[f][depth - 1] == level)
                    })
                    .map(|f| {
                        let first = tree.members[f].iter().copied().min().unwrap_or(usize::MAX);
                        (first, Item::Frame(f))
                    }),
            )
            .collect();
        items.sort_by_key(|&(first, _)| first);
        let local = |item: Item| items.iter().position(|&(_, it)| it == item);
        let m = items.len();

        // Where an end lies in its item: (where it is entered, where it is
        // left).
        let reach = |end: End| -> (i64, i64) {
            match end {
                End::Node(x) => (rel[x], rel[x]),
                End::Frame(f) => {
                    let ranks = tree.members[f].iter().map(|&x| rel[x]);
                    (ranks.clone().min().unwrap_or(0), ranks.max().unwrap_or(0))
                }
            }
        };
        let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); m];
        let mut joins = Vec::new();
        for &(index, from_item, to_item) in &levels[level] {
            let (Some(a), Some(b)) = (local(from_item), local(to_item)) else {
                continue;
            };
            adj[a].push((b, index));
            joins.push((index, a, b));
        }
        let mut color = vec![0u8; m];
        for start in 0..m {
            if color[start] == 0 {
                super::mark(start, &adj, &mut color, &mut back);
            }
        }
        // The edges turned the way the flow runs, each with how far below
        // its upper item the lower one starts at least.
        let mut succ: Vec<Vec<(usize, i64)>> = vec![Vec::new(); m];
        let mut indeg = vec![0usize; m];
        for (index, a, b) in joins {
            let edge = &edges[index];
            let (Some(from), Some(to)) = (tree.end(edge.from), tree.end(edge.to)) else {
                continue;
            };
            let (upper, lower, top, bottom) = if back[index] {
                (b, a, to, from)
            } else {
                (a, b, from, to)
            };
            let length = reach(top).1 - reach(bottom).0 + 1;
            succ[upper].push((lower, length));
            indeg[lower] += 1;
        }
        let mut base = vec![0i64; m];
        let mut queue: std::collections::VecDeque<usize> =
            (0..m).filter(|&item| indeg[item] == 0).collect();
        while let Some(item) = queue.pop_front() {
            for &(next, length) in &succ[item] {
                base[next] = base[next].max(base[item] + length);
                indeg[next] -= 1;
                if indeg[next] == 0 {
                    queue.push_back(next);
                }
            }
        }
        for (local, &(_, item)) in items.iter().enumerate() {
            match item {
                Item::Node(x) => rel[x] = base[local],
                Item::Frame(f) => {
                    for &x in &tree.members[f] {
                        rel[x] += base[local];
                    }
                }
            }
        }
    }
    let rank = rel.into_iter().map(|r| r.max(0) as usize).collect();
    (rank, back)
}

/// The first and last rank of every frame; `None` for one that holds no
/// node.
pub(super) fn spans(tree: &Tree, rank: &[usize]) -> Vec<Span> {
    tree.members
        .iter()
        .map(|members| {
            let ranks = members.iter().map(|&x| rank[x]);
            Some((ranks.clone().min()?, ranks.max()?))
        })
        .collect()
}

/// The frame a line from `top` down to `bottom` passes through at rank
/// `at`: the innermost one around either end that reaches that rank;
/// where the two ends lie in different frames that both do, the one
/// around the nearer end.
pub(super) fn passing(
    tree: &Tree,
    spans: &[Span],
    (top, bottom): (End, End),
    (from, to): (usize, usize),
    at: usize,
) -> Option<usize> {
    let reaches = |f: &&usize| spans[**f].is_some_and(|(a, b)| a <= at && at <= b);
    let upper = tree.around_end(top).iter().rev().find(reaches).copied();
    let lower = tree.around_end(bottom).iter().rev().find(reaches).copied();
    match (upper, lower) {
        (Some(u), Some(l)) if tree.holds(l, u) => Some(u),
        (Some(u), Some(l)) if tree.holds(u, l) => Some(l),
        (Some(u), Some(l)) => Some(if at - from <= to - at { u } else { l }),
        (u, l) => u.or(l),
    }
}

/// Where each slot lies in its rank, from 0 to 1, so ranks of different
/// lengths compare.
pub(super) fn positions(rows: &[Vec<usize>], slots: &[Slot]) -> Vec<f32> {
    let mut pos = vec![0.5; slots.len()];
    for row in rows {
        let len = row.len().max(1) as f32;
        for (index, &slot) in row.iter().enumerate() {
            pos[slot] = (index as f32 + 0.5) / len;
        }
    }
    pos
}

/// Where each frame lies, from 0 to 1: the mean place of its slots over
/// all its ranks. Sibling frames keep this order on every rank.
pub(super) fn keys(
    rows: &[Vec<usize>],
    pos: &[f32],
    frames: &[Vec<usize>],
    count: usize,
) -> Vec<f32> {
    let mut sum = vec![(0.0f32, 0usize); count];
    for &slot in rows.iter().flatten() {
        for &f in &frames[slot] {
            sum[f].0 += pos[slot];
            sum[f].1 += 1;
        }
    }
    sum.into_iter()
        .map(|(total, n)| if n == 0 { 0.5 } else { total / n as f32 })
        .collect()
}

/// What a rank is sorted by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum By {
    /// The rank before it.
    Preds,
    /// The rank after it.
    Succs,
    /// Both.
    Both,
    /// Where each slot is now.
    Place,
}

/// Sorts a rank by barycenter, the slots of one frame kept together and
/// sibling frames in the order of `keys`. `frames` holds the frames
/// around each slot, outermost first. A slot with no neighbor keeps its
/// place.
pub(super) fn sort_row(
    row: &mut [usize],
    slots: &[Slot],
    pos: &[f32],
    by: By,
    frames: &[Vec<usize>],
    keys: &[f32],
) {
    let bary: Vec<f32> = row
        .iter()
        .map(|&slot| {
            let s = &slots[slot];
            let neighbors: Vec<usize> = match by {
                By::Preds => s.preds.clone(),
                By::Succs => s.succs.clone(),
                By::Both => s.preds.iter().chain(&s.succs).copied().collect(),
                By::Place => Vec::new(),
            };
            if neighbors.is_empty() {
                pos[slot]
            } else {
                neighbors.iter().map(|&n| pos[n]).sum::<f32>() / neighbors.len() as f32
            }
        })
        .collect();
    // The first place in the rank each frame holds, which breaks a tie
    // between two of them, or between one and a slot.
    let mut first = vec![usize::MAX; keys.len()];
    for (index, &slot) in row.iter().enumerate() {
        for &f in &frames[slot] {
            first[f] = first[f].min(index);
        }
    }
    let key = |index: usize| -> Vec<(f32, usize)> {
        let slot = row[index];
        frames[slot]
            .iter()
            .map(|&f| (keys[f], first[f]))
            .chain([(bary[index], index)])
            .collect()
    };
    let mut keyed: Vec<(Vec<(f32, usize)>, usize)> = (0..row.len())
        .map(|index| (key(index), row[index]))
        .collect();
    keyed.sort_by(|a, b| {
        for (x, y) in a.0.iter().zip(&b.0) {
            let order = x.0.total_cmp(&y.0).then(x.1.cmp(&y.1));
            if order.is_ne() {
                return order;
            }
        }
        a.0.len().cmp(&b.0.len())
    });
    for (index, (_, slot)) in keyed.into_iter().enumerate() {
        row[index] = slot;
    }
}

/// Between two neighbors of a rank: as much as between two boxes, but
/// between lines passing by less, and next to where a line ends on a
/// frame's border as little, since that line stops short of the rank.
fn gap(slots: &[Slot], a: usize, b: usize, border: bool) -> f32 {
    let anchored = slots[a].anchor.is_some() || slots[b].anchor.is_some();
    if !border && (anchored || slots[a].real.is_none() && slots[b].real.is_none()) {
        DUMMY_GAP
    } else {
        CROSS_GAP
    }
}

/// The frames around `a` and around `b` that only one of them is in: the
/// outermost of each, if any.
fn parting(a: &[usize], b: &[usize]) -> (Option<usize>, Option<usize>) {
    let same = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    (a.get(same).copied(), b.get(same).copied())
}

/// Between two neighbors of a rank: the gap, and the padding of every
/// frame the step from one to the other leaves or enters.
pub(super) fn sep(slots: &[Slot], frames: &[Vec<usize>], tree: &Tree, a: usize, b: usize) -> f32 {
    let (fa, fb) = (&frames[a], &frames[b]);
    let same = fa.iter().zip(fb).take_while(|(x, y)| x == y).count();
    let leaving: f32 = fa[same..].iter().map(|&f| tree.pads[f].right).sum();
    let entering: f32 = fb[same..].iter().map(|&f| tree.pads[f].left).sum();
    gap(slots, a, b, same < fa.len() || same < fb.len()) + leaving + entering
}

/// Separation constraints `x[right] >= x[left] + gap` over variables that
/// each have a target, a weight and bounds.
struct Problem {
    targets: Vec<f32>,
    weights: Vec<f32>,
    lo: Vec<f32>,
    hi: Vec<f32>,
    joins: Vec<(usize, usize, f32)>,
}

impl Problem {
    fn new(targets: Vec<f32>, weights: Vec<f32>) -> Self {
        let n = targets.len();
        Problem {
            targets,
            weights,
            lo: vec![f32::NEG_INFINITY; n],
            hi: vec![f32::INFINITY; n],
            joins: Vec::new(),
        }
    }

    /// The positions nearest the targets that keep the constraints, found
    /// as VPSC's "satisfy" does: variables are taken in order, and a block
    /// of them held at fixed distances sits at the weighted mean of its
    /// targets, within its bounds, merging with the block before it while
    /// a constraint between the two is broken.
    fn solve(&self) -> Vec<f32> {
        struct Block {
            vars: Vec<usize>,
            incoming: Vec<usize>,
            weight: f32,
            /// The weighted sum of the targets, each less its offset.
            sum: f32,
            lo: f32,
            hi: f32,
            at: f32,
        }
        let n = self.targets.len();
        let mut incoming = vec![Vec::new(); n];
        let mut outgoing = vec![Vec::new(); n];
        let mut indeg = vec![0usize; n];
        for (index, &(left, right, _)) in self.joins.iter().enumerate() {
            incoming[right].push(index);
            outgoing[left].push(right);
            indeg[right] += 1;
        }
        let mut queue: std::collections::VecDeque<usize> =
            (0..n).filter(|&v| indeg[v] == 0).collect();
        let mut order = Vec::with_capacity(n);
        while let Some(v) = queue.pop_front() {
            order.push(v);
            for &next in &outgoing[v] {
                indeg[next] -= 1;
                if indeg[next] == 0 {
                    queue.push_back(next);
                }
            }
        }
        // A cycle cannot be kept; its variables come last, as they are.
        if order.len() < n {
            let mut seen = vec![false; n];
            for &v in &order {
                seen[v] = true;
            }
            order.extend((0..n).filter(|&v| !seen[v]));
        }
        let place = |block: &Block| {
            let at = block.sum / block.weight;
            if block.lo > block.hi {
                block.lo
            } else {
                at.clamp(block.lo, block.hi)
            }
        };
        let mut block = vec![usize::MAX; n];
        let mut offset = vec![0.0f32; n];
        let mut blocks: Vec<Block> = Vec::new();
        for &v in &order {
            let id = blocks.len();
            let weight = self.weights[v].max(1e-4);
            block[v] = id;
            let mut fresh = Block {
                vars: vec![v],
                incoming: std::mem::take(&mut incoming[v]),
                weight,
                sum: weight * self.targets[v],
                lo: self.lo[v],
                hi: self.hi[v],
                at: 0.0,
            };
            fresh.at = place(&fresh);
            blocks.push(fresh);
            loop {
                let mut worst: Option<usize> = None;
                let mut most = 1e-4;
                let x_block = blocks[id].at;
                blocks[id]
                    .incoming
                    .retain(|&join| block[self.joins[join].0] != id);
                for &join in &blocks[id].incoming {
                    let (left, right, gap) = self.joins[join];
                    if block[left] == usize::MAX {
                        continue;
                    }
                    let x_left = blocks[block[left]].at + offset[left];
                    let broken = x_left + gap - (x_block + offset[right]);
                    if broken > most {
                        most = broken;
                        worst = Some(join);
                    }
                }
                let Some(join) = worst else {
                    break;
                };
                let (left, right, gap) = self.joins[join];
                let other = block[left];
                let shift = offset[right] - gap - offset[left];
                let vars = std::mem::take(&mut blocks[other].vars);
                let more = std::mem::take(&mut blocks[other].incoming);
                let into = &mut blocks[id];
                for &var in &vars {
                    offset[var] += shift;
                    block[var] = id;
                    let weight = self.weights[var].max(1e-4);
                    into.weight += weight;
                    into.sum += weight * (self.targets[var] - offset[var]);
                    into.lo = into.lo.max(self.lo[var] - offset[var]);
                    into.hi = into.hi.min(self.hi[var] - offset[var]);
                }
                into.vars.extend(vars);
                into.incoming.extend(more);
                into.at = place(into);
            }
        }
        let mut x: Vec<f32> = (0..n)
            .map(|v| {
                if block[v] == usize::MAX {
                    self.targets[v]
                } else {
                    blocks[block[v]].at + offset[v]
                }
            })
            .collect();
        // Rounding, or a block moved after a later one merged, can leave a
        // constraint a little broken; push right until none is.
        let mut into = vec![Vec::new(); n];
        for &(left, right, gap) in &self.joins {
            into[right].push((left, gap));
        }
        for &v in &order {
            for &(left, gap) in &into[v] {
                x[v] = x[v].max(x[left] + gap);
            }
        }
        x
    }
}

/// Places the slots across the flow and the frames' borders with them.
/// Every rank is packed and centered first, then the whole is moved as
/// little as keeping the frames apart needs; then a few passes down and up
/// move each rank toward the slots it connects to, the others held: the
/// constraints that join a slot of the rank, or a border of a frame it
/// holds, to anything else bound how far it can move.
pub(super) struct Placer {
    slots: usize,
    joins: Vec<(usize, usize, f32)>,
    /// The constraints on each variable.
    touching: Vec<Vec<usize>>,
    /// The frames around each slot, outermost first.
    holders: Vec<Vec<usize>>,
    /// Each frame's direct slots and the frames right inside it.
    direct: Vec<Vec<usize>>,
    children: Vec<Vec<usize>>,
    pads: Vec<Pad>,
    inside_out: Vec<usize>,
}

impl Placer {
    pub fn new(slots: &[Slot], rows: &[Vec<usize>], frames: &[Vec<usize>], tree: &Tree) -> Self {
        let s = slots.len();
        let count = tree.count();
        let left = |f: usize| s + 2 * f;
        let right = |f: usize| s + 2 * f + 1;
        let mut joins = Vec::new();
        for row in rows {
            for pair in row.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let (leaving, entering) = parting(&frames[a], &frames[b]);
                let gap = gap(slots, a, b, leaving.is_some() || entering.is_some());
                let (from, extra) = match leaving {
                    Some(f) => (right(f), 0.0),
                    None => (a, slots[a].pack_cross),
                };
                let to = entering.map_or(b, left);
                joins.push((from, to, extra + gap));
            }
        }
        let mut direct = vec![Vec::new(); count];
        for &slot in rows.iter().flatten() {
            if let Some(&f) = frames[slot].last() {
                direct[f].push(slot);
            }
        }
        let children = tree.children();
        let mut held = vec![false; count];
        for f in tree.inside_out() {
            held[f] = !direct[f].is_empty() || children[f].iter().any(|&c| held[c]);
        }
        for f in 0..count {
            if !held[f] {
                continue;
            }
            let pad = tree.pads[f];
            for &slot in &direct[f] {
                joins.push((left(f), slot, pad.left));
                joins.push((slot, right(f), slots[slot].pack_cross + pad.right));
            }
            for &c in children[f].iter().filter(|&&c| held[c]) {
                joins.push((left(f), left(c), pad.left));
                joins.push((right(c), right(f), pad.right));
            }
            joins.push((left(f), right(f), pad.min_cross.max(0.0)));
        }
        let mut touching = vec![Vec::new(); s + 2 * count];
        for (index, &(a, b, _)) in joins.iter().enumerate() {
            touching[a].push(index);
            touching[b].push(index);
        }
        Placer {
            slots: s,
            joins,
            touching,
            holders: frames.to_vec(),
            direct,
            children,
            pads: tree.pads.clone(),
            inside_out: tree.inside_out(),
        }
    }

    /// Where each frame's borders would be around what it holds now.
    fn natural(&self, slots: &[Slot]) -> Vec<(f32, f32)> {
        let mut borders = vec![(f32::INFINITY, f32::NEG_INFINITY); self.pads.len()];
        for &f in &self.inside_out {
            let mut lo = f32::INFINITY;
            let mut hi = f32::NEG_INFINITY;
            for &slot in &self.direct[f] {
                lo = lo.min(slots[slot].cross);
                hi = hi.max(slots[slot].cross + slots[slot].pack_cross);
            }
            for &c in &self.children[f] {
                lo = lo.min(borders[c].0);
                hi = hi.max(borders[c].1);
            }
            let pad = self.pads[f];
            borders[f] = if lo.is_finite() {
                (lo - pad.left, hi + pad.right)
            } else {
                (0.0, 0.0)
            };
        }
        borders
    }

    /// Moves every slot as little as keeping the frames apart needs, each
    /// held by `hold`; the frames' borders.
    pub fn settle(&self, slots: &mut [Slot], hold: f32) -> Vec<(f32, f32)> {
        let mut targets: Vec<f32> = slots.iter().map(|slot| slot.cross).collect();
        let mut weights = vec![hold; self.slots];
        for (lo, hi) in self.natural(slots) {
            targets.extend([lo, hi]);
            weights.extend([LOOSE, LOOSE]);
        }
        let mut problem = Problem::new(targets, weights);
        problem.joins.clone_from(&self.joins);
        let x = problem.solve();
        for (slot, &x) in slots.iter_mut().zip(&x) {
            slot.cross = x;
        }
        (0..self.pads.len())
            .map(|f| (x[self.slots + 2 * f], x[self.slots + 2 * f + 1]))
            .collect()
    }

    /// Moves the slots of one rank to `wants` (left of the packed room, and
    /// how much each pulls), as near as the rest allows, and the borders of
    /// the frames they lie in with them.
    pub fn fit(&self, slots: &mut [Slot], borders: &mut [(f32, f32)], wants: &[(usize, f32, f32)]) {
        let total = self.touching.len();
        let mut local = vec![usize::MAX; total];
        let mut free: Vec<usize> = Vec::new();
        let natural = self.natural(slots);
        let mut targets = Vec::new();
        let mut weights = Vec::new();
        for &(slot, want, weight) in wants {
            local[slot] = free.len();
            free.push(slot);
            targets.push(want);
            weights.push(weight);
        }
        for &(slot, ..) in wants {
            for &f in &self.holders[slot] {
                for (var, at) in [
                    (self.slots + 2 * f, natural[f].0),
                    (self.slots + 2 * f + 1, natural[f].1),
                ] {
                    if local[var] == usize::MAX {
                        local[var] = free.len();
                        free.push(var);
                        targets.push(at);
                        weights.push(LOOSE);
                    }
                }
            }
        }
        let value = |var: usize| {
            if var < self.slots {
                slots[var].cross
            } else {
                let (lo, hi) = borders[(var - self.slots) / 2];
                if (var - self.slots).is_multiple_of(2) {
                    lo
                } else {
                    hi
                }
            }
        };
        let mut problem = Problem::new(targets, weights);
        let mut seen = vec![false; self.joins.len()];
        for &var in &free {
            for &join in &self.touching[var] {
                if std::mem::replace(&mut seen[join], true) {
                    continue;
                }
                let (a, b, gap) = self.joins[join];
                match (local[a], local[b]) {
                    (usize::MAX, usize::MAX) => {}
                    (usize::MAX, b) => problem.lo[b] = problem.lo[b].max(value(a) + gap),
                    (a, usize::MAX) => problem.hi[a] = problem.hi[a].min(value(b) - gap),
                    (a, b) => problem.joins.push((a, b, gap)),
                }
            }
        }
        let x = problem.solve();
        for (&var, &x) in free.iter().zip(&x) {
            if var < self.slots {
                slots[var].cross = x;
            } else {
                let border = &mut borders[(var - self.slots) / 2];
                if (var - self.slots).is_multiple_of(2) {
                    border.0 = x;
                } else {
                    border.1 = x;
                }
            }
        }
    }
}

/// How a line passes the name of a frame it comes into from above, in
/// the room under the name: it comes down `above`, beside the name, and
/// turns back to `below` on its track there.
#[derive(Debug, Clone, Copy)]
pub(super) struct Jog {
    pub frame: usize,
    pub above: f32,
    pub below: f32,
    pub track: usize,
}

/// The ways past names, by edge and step, the outermost frame first.
pub(super) type Jogs = HashMap<(usize, usize), Vec<Jog>>;

/// The lines of a layout, as [`jogs`] needs them.
pub(super) struct Lines<'a> {
    pub slots: &'a [Slot],
    pub chains: &'a [Vec<usize>],
    pub first: &'a [usize],
    pub ports: &'a [Ports],
    pub frames: &'a [Vec<usize>],
    pub edges: &'a [Edge],
}

/// Between a name and the lines passing it, and a frame's border and
/// those lines.
const BESIDE: f32 = 8.0;
/// From a name down to the first track under it, which is also how much
/// further down the frame's boxes move past the last.
pub(super) const UNDER_NAME: f32 = 7.0;

/// How the lines that come into a frame from above pass its name, when
/// the flow runs down: those that would cross it come down beside it, on
/// the side nearer where they head, fanned out so none crosses another,
/// and turn back under the name, the outer ones lower. A side with no
/// room sends its lines to the other; with none on either the lines cross
/// the name. Frames inside frames go first, so an outer frame's lines head
/// for where the inner ones come down. Also how much room each frame
/// keeps under its name for that, and how wide the frames without room
/// would have to be.
pub(super) fn jogs(
    tree: &Tree,
    spans: &[Span],
    borders: &[(f32, f32)],
    lines: &Lines,
) -> (Jogs, Vec<f32>, Vec<(usize, f32)>) {
    let mut jogs: Jogs = HashMap::new();
    let mut room = vec![0.0f32; tree.count()];
    let mut short = Vec::new();
    for f in tree.inside_out() {
        let name = tree.names[f];
        let (Some((first, _)), Some(&(left, right))) = (spans[f], borders.get(f)) else {
            continue;
        };
        if name.width <= 0.0 {
            continue;
        }
        let start = left + name.share * (right - left) + name.offset;
        let (lo, hi) = (start - BESIDE, start + name.width + BESIDE);
        // The steps that come in through the top: (edge, step, where they
        // head below the name).
        let mut coming: Vec<(usize, usize, f32)> = Vec::new();
        for (index, chain) in lines.chains.iter().enumerate() {
            if chain.len() < 2 || lines.edges[index].hidden {
                continue;
            }
            let last = chain.len() - 1;
            for step in 0..last {
                let (from, to) = (chain[step], chain[step + 1]);
                let inside = |slot: usize| lines.frames[slot].contains(&f);
                let on_border = lines.slots[to].anchor.is_some_and(|(frame, _)| frame == f);
                if lines.first[index] + step + 1 != first
                    || !inside(to)
                    || inside(from)
                    || on_border
                {
                    continue;
                }
                let below = match jogs.get(&(index, step)) {
                    Some(way) => way[0].above,
                    None if step + 1 == last => center(&lines.slots[to]) + lines.ports[index].end,
                    None => center(&lines.slots[to]),
                };
                coming.push((index, step, below));
            }
        }
        if !coming.iter().any(|&(.., x)| x > lo && x < hi) {
            continue;
        }
        let middle = (lo + hi) / 2.0;
        let (mut west, mut east): (Vec<_>, Vec<_>) =
            coming.into_iter().partition(|&(.., x)| x < middle);
        // Fans out one side, nearest the name first; and how far that
        // reaches past the frame's border.
        let spread = |side: &mut Vec<(usize, usize, f32)>, toward_left: bool| -> (Vec<f32>, f32) {
            if toward_left {
                side.sort_by(|a, b| b.2.total_cmp(&a.2));
            } else {
                side.sort_by(|a, b| a.2.total_cmp(&b.2));
            }
            let mut cursor = if toward_left { lo } else { hi };
            let mut out = Vec::with_capacity(side.len());
            let mut over = 0.0f32;
            for &(.., x) in side.iter() {
                let at = if toward_left {
                    x.min(cursor)
                } else {
                    x.max(cursor)
                };
                over = over.max(left + BESIDE - at).max(at - (right - BESIDE));
                out.push(at);
                cursor = if toward_left {
                    at - PORT_STEP
                } else {
                    at + PORT_STEP
                };
            }
            (out, over)
        };
        let fan = |side: &mut Vec<(usize, usize, f32)>, toward_left: bool| {
            let (out, over) = spread(side, toward_left);
            (over <= 0.0).then_some(out)
        };
        // A frame grows on both sides, so twice what the fuller side lacks.
        let (_, west_over) = spread(&mut west.clone(), true);
        let (_, east_over) = spread(&mut east.clone(), false);
        let lacking = west_over.max(east_over);
        let mut placed = None;
        for _ in 0..2 {
            if let (Some(w), Some(e)) = (fan(&mut west, true), fan(&mut east, false)) {
                placed = Some((w, e));
                break;
            }
            // One side has no room: everything goes to the other.
            if fan(&mut west.clone(), true).is_none() {
                east.append(&mut west);
            } else {
                west.append(&mut east);
            }
        }
        let Some((w, e)) = placed else {
            short.push((f, right - left + 2.0 * lacking + 2.0));
            continue;
        };
        let mut tracks = 0;
        for (side, aboves) in [(&west, w), (&east, e)] {
            let mut track = 0;
            for (&(index, step, below), above) in side.iter().zip(aboves) {
                if (above - below).abs() < 0.5 {
                    continue;
                }
                jogs.entry((index, step)).or_default().insert(
                    0,
                    Jog {
                        frame: f,
                        above,
                        below,
                        track,
                    },
                );
                track += 1;
            }
            tracks = tracks.max(track);
        }
        if tracks > 0 {
            room[f] = tracks as f32 * TRACK + UNDER_NAME;
        }
    }
    (jogs, room, short)
}

/// How far each frame reaches before its first rank and after its last,
/// with the frames inside it that begin or end there.
pub(super) struct Stacks {
    pub before: Vec<f32>,
    pub after: Vec<f32>,
}

impl Stacks {
    /// `room` is what each frame keeps under its name for the lines that
    /// pass it.
    pub fn new(tree: &Tree, spans: &[Span], room: &[f32]) -> Self {
        let count = tree.count();
        let children = tree.children();
        let mut before = vec![0.0f32; count];
        let mut after = vec![0.0f32; count];
        for f in tree.inside_out() {
            let Some((first, last)) = spans[f] else {
                continue;
            };
            let inner = |stack: &[f32], at: fn((usize, usize)) -> usize, rank: usize| {
                children[f]
                    .iter()
                    .filter(|&&c| spans[c].is_some_and(|span| at(span) == rank))
                    .map(|&c| stack[c])
                    .fold(0.0f32, f32::max)
            };
            let under_name = room.get(f).copied().unwrap_or(0.0);
            before[f] = tree.pads[f].before + under_name + inner(&before, |s| s.0, first);
            after[f] = tree.pads[f].after + inner(&after, |s| s.1, last);
        }
        Stacks { before, after }
    }

    /// The room the frames need in each rank's gap: after rank `g` for
    /// the frames that end there, and before rank `g + 1` for those that
    /// begin there.
    pub fn zones(&self, spans: &[Span], ranks: usize) -> (Vec<f32>, Vec<f32>) {
        let mut ending = vec![0.0f32; ranks];
        let mut beginning = vec![0.0f32; ranks];
        for (f, span) in spans.iter().enumerate() {
            if let Some((first, last)) = *span {
                beginning[first] = beginning[first].max(self.before[f]);
                ending[last] = ending[last].max(self.after[f]);
            }
        }
        (ending, beginning)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Edge, Frame, Node, Rect, framed, route};
    use super::Direction;

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

    fn frame(parent: Option<usize>) -> Frame {
        Frame {
            parent,
            name: Default::default(),
            top: 30.0,
            bottom: 12.0,
            left: 12.0,
            right: 12.0,
            min_width: 60.0,
        }
    }

    fn inside(inner: Rect, outer: Rect) -> bool {
        inner.x >= outer.x - 0.1
            && inner.y >= outer.y - 0.1
            && inner.x + inner.w <= outer.x + outer.w + 0.1
            && inner.y + inner.h <= outer.y + outer.h + 0.1
    }

    fn apart(a: Rect, b: Rect) -> bool {
        a.x + a.w <= b.x + 0.1
            || b.x + b.w <= a.x + 0.1
            || a.y + a.h <= b.y + 0.1
            || b.y + b.h <= a.y + 0.1
    }

    fn cuts(points: &[(f32, f32)], rect: Rect) -> bool {
        points.windows(2).any(|w| route::cuts(rect, w[0], w[1]))
    }

    /// The smart home of the gallery: three rooms of two boxes, a hub
    /// below them, and every line into the hub's rules from above.
    #[test]
    fn lines_into_a_frame_come_in_from_above() {
        // Thermostat, lamp; door, bell; rules, history; resident.
        let nodes = [
            node(90.0, 30.0),
            node(50.0, 30.0),
            node(100.0, 30.0),
            node(80.0, 30.0),
            node(60.0, 30.0),
            node(70.0, 40.0),
            node(40.0, 60.0),
        ];
        let frames = [frame(None), frame(None), frame(None)];
        let within = [Some(0), Some(0), Some(1), Some(1), Some(2), Some(2), None];
        let edges = [
            edge(0, 4),
            edge(2, 4),
            edge(3, 4),
            edge(4, 1),
            edge(4, 5),
            edge(6, 4),
        ];
        let laid = framed(&nodes, &frames, &within, &edges, Direction::Down);
        for (index, &f) in within.iter().enumerate() {
            if let Some(f) = f {
                assert!(
                    inside(laid.nodes[index], laid.frames[f]),
                    "{index} {:?}",
                    laid.frames[f]
                );
            }
        }
        for a in 0..3 {
            for b in a + 1..3 {
                assert!(
                    apart(laid.frames[a], laid.frames[b]),
                    "{:?} {:?}",
                    laid.frames[a],
                    laid.frames[b]
                );
            }
            assert!(apart(laid.frames[a], laid.nodes[6]));
        }
        let rules = laid.nodes[4];
        assert!(
            rules.y > laid.frames[0].y + laid.frames[0].h,
            "{:?}",
            laid.frames
        );
        for (index, route) in laid.routes.iter().enumerate() {
            let points = &route.points;
            for (other, rect) in laid.nodes.iter().enumerate() {
                if other != edges[index].from && other != edges[index].to {
                    assert!(!cuts(points, *rect), "{index} cuts {other}: {points:?}");
                }
            }
        }
        // Into the rules from above, each at its own place.
        let mut tops: Vec<f32> = [0, 1, 2, 5]
            .iter()
            .map(|&index| {
                let end = *laid.routes[index].points.last().unwrap();
                assert!(
                    (end.1 - rules.y).abs() < 0.1,
                    "{index}: {:?}",
                    laid.routes[index].points
                );
                end.0
            })
            .collect();
        let out = laid.routes[3].points[0];
        assert!((out.1 - rules.y).abs() < 0.1, "{:?}", laid.routes[3].points);
        tops.push(out.0);
        tops.sort_by(f32::total_cmp);
        for pair in tops.windows(2) {
            assert!(pair[1] - pair[0] > 1.0, "{tops:?}");
        }
    }

    /// A line to a frame itself ends at its border.
    #[test]
    fn a_line_to_a_frame_ends_at_its_border() {
        let nodes = [node(40.0, 20.0), node(40.0, 20.0), node(40.0, 20.0)];
        let frames = [frame(None)];
        let within = [None, Some(0), Some(0)];
        let edges = [edge(0, 3), edge(1, 2), edge(3, 0)];
        let laid = framed(&nodes, &frames, &within, &edges, Direction::Down);
        let border = laid.frames[0];
        let into = &laid.routes[0].points;
        let end = *into.last().unwrap();
        assert!((end.1 - border.y).abs() < 0.1, "{into:?} {border:?}");
        assert!(end.0 > border.x && end.0 < border.x + border.w);
        let back = &laid.routes[2].points;
        assert!((back[0].1 - border.y).abs() < 0.1, "{back:?} {border:?}");
        assert!(inside(laid.nodes[1], border) && inside(laid.nodes[2], border));
        assert!(laid.nodes[0].y + laid.nodes[0].h < border.y);
    }

    /// As Graphviz ranks clusters: a frame keeps its own ranks and moves
    /// down only as far as the lines from outside ask, so two frames can
    /// overlap along the flow and sit side by side.
    #[test]
    fn frames_move_down_as_far_as_their_lines_ask() {
        // Members: member, level. Lending: loan, copy.
        let nodes = [
            node(60.0, 30.0),
            node(60.0, 30.0),
            node(60.0, 30.0),
            node(60.0, 30.0),
        ];
        let frames = [frame(None), frame(None)];
        let within = [Some(0), Some(0), Some(1), Some(1)];
        let edges = [edge(0, 1), edge(2, 3), edge(0, 2)];
        let laid = framed(&nodes, &frames, &within, &edges, Direction::Down);
        let (members, lending) = (laid.frames[0], laid.frames[1]);
        assert!(apart(members, lending), "{members:?} {lending:?}");
        // The loan sits a rank below the member, level with the level.
        assert!(
            (laid.nodes[2].y - laid.nodes[1].y).abs() < 0.1,
            "{:?}",
            laid.nodes
        );
        assert!(lending.y < members.y + members.h, "{members:?} {lending:?}");
    }

    /// Frames inside frames hold what they hold, and their padding.
    #[test]
    fn nested_frames_hold_their_boxes() {
        let nodes = [
            node(40.0, 20.0),
            node(40.0, 20.0),
            node(40.0, 20.0),
            node(40.0, 20.0),
        ];
        let frames = [frame(None), frame(Some(0)), frame(None)];
        let within = [Some(1), Some(0), Some(2), None];
        let edges = [edge(3, 0), edge(0, 1), edge(1, 2), edge(0, 2)];
        let laid = framed(&nodes, &frames, &within, &edges, Direction::Down);
        assert!(inside(laid.frames[1], laid.frames[0]), "{:?}", laid.frames);
        assert!(inside(laid.nodes[0], laid.frames[1]));
        assert!(inside(laid.nodes[1], laid.frames[0]));
        assert!(!inside(laid.nodes[1], laid.frames[1]));
        assert!(apart(laid.frames[0], laid.frames[2]));
        assert!(laid.frames[1].y >= laid.frames[0].y + 30.0 - 0.1);
        for (index, route) in laid.routes.iter().enumerate() {
            for (other, rect) in laid.nodes.iter().enumerate() {
                if other != edges[index].from && other != edges[index].to {
                    assert!(!cuts(&route.points, *rect), "{index} cuts {other}");
                }
            }
        }
    }
}
