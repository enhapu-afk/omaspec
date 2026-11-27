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
//! Ranks are doubled, as `dot` doubles them where lines have labels
//! (docs/adr/0020): a box of rank *r* takes rank *2r*, so every line
//! passes a rank between the boxes it joins, where it has a point of its
//! own. A labelled line's point there is a box as big as its label, which
//! ordering and placement keep free like any box, so the label lies
//! beside its line and nothing else lies on it.
//!
//! Every bend sits in a gap between two ranks, never inside a rank, so no
//! line cuts through a box, however tall its neighbors are. A line bends a
//! little after the rank it leaves, each on a track of its own where its
//! run across the gap overlaps another's, so none lies on another; runs
//! that lie apart share a track, and the gap is made tall enough for the
//! tracks. A back edge runs the same way, through the
//! ranks between its boxes, and is drawn backward, as Graphviz draws it:
//! a transition and the one back lie side by side, not around the
//! diagram. Lines that meet one side of a box share it, in the order of
//! their other ends, each as near as that allows to the point facing its
//! other end ([`ports`]). Lines that then arrive together at one side of a
//! box, and carry no words, share the last stem ([`bundle`]), so many
//! lines into one box meet it once. Self-loops sit on the free side (right when the
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
mod bundle;
pub mod by_hand;
mod corner;
mod doubled;
pub mod hold;
mod join;
mod loops;
mod nest;
pub mod open;
mod order;
pub mod route;
mod span;
mod straight;

pub use join::Tree;
use loops::{LOOP_WORDS, Loops, self_loop};

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
    /// Its outline reaches the middle of each side of its box (a diamond,
    /// a circle, a stadium): a line back against the flow may leave or
    /// reach it there ([`corner`]).
    pub round: bool,
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
    /// Stays out of a bundle ([`bundle`]): the writer kept this line, or
    /// a box it meets, apart.
    pub apart: bool,
    /// A mark at an end, a multiplicity. It sits on the stem, so the line
    /// stays out of a trunk.
    pub marked: bool,
    /// How the line looks where it arrives ([`arrival`]). A trunk is
    /// shared only by lines of one key, so it never stacks two kinds of
    /// head, or a dashed line under a solid one.
    pub arrival: u32,
    /// How it looks where it leaves, where it has a head there, as
    /// `Base <|-- Kind` writes a subclass: such lines share a trunk into
    /// the box they leave as lines that end in their heads do into theirs
    /// ([`bundle`]). 0 where it has no head at its start.
    pub departure: u32,
    /// The side of the line its label takes, where the writer chose one:
    /// before the line across the flow (left of a line running down,
    /// above one running across) or after it. `None` lets the layout
    /// choose ([`doubled::label_sides`]).
    pub label_before: Option<bool>,
    /// A line within a rank
    /// ([0027](../../../../docs/adr/0027-a-line-within-a-rank.md)): its
    /// second box stands in the rank of its first, right after it across
    /// the flow (`Some(true)`) or right before it, and the line runs
    /// straight between the two. `None` is a line from rank to rank.
    pub level: Option<bool>,
    /// A line of a tree: the lines of one key that meet one box share one
    /// stem into it ([`join`]), as subclasses under one triangle.
    pub tree: Option<Tree>,
}

/// The key of a line's arrival ([`Edge::arrival`]): the head it ends in,
/// and its color, width and dash. Half a hash: a diagram's few kinds of
/// line tell apart in it, and an edge stays as small as it was.
pub fn arrival(head: impl std::hash::Hash, stroke: &crate::svg::Stroke) -> u32 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    head.hash(&mut hasher);
    let crate::color::Color(r, g, b) = stroke.color;
    (r, g, b, stroke.width.to_bits()).hash(&mut hasher);
    stroke
        .dash
        .map(|(on, off)| (on.to_bits(), off.to_bits()))
        .hash(&mut hasher);
    hasher.finish() as u32
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
    /// The holds not kept, by their index, and why ([`hold`]).
    pub unheld: Vec<(usize, hold::Unheld)>,
}

/// Room between two nodes of one rank, unless a drawer asks for other
/// ([`Spacing`]).
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
/// Room between two ranks of boxes, before labels and tracks ask for
/// more: half before the rank between them, half after; unless a drawer
/// asks for other ([`Spacing`]).
const ALONG_PAD: f32 = 32.0;

/// How far apart boxes stand before lines and labels ask for more: two
/// of one rank across the flow, two ranks along it. A drawer asks for
/// other room than the default where its diagram does (Mermaid's
/// `nodeSpacing` and `rankSpacing`, REQ-110).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spacing {
    pub cross: f32,
    pub along: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing {
            cross: CROSS_GAP,
            along: ALONG_PAD,
        }
    }
}
/// Where a line bends, past the end of the rank it leaves.
const JOG: f32 = 12.0;
/// Between two lines running across one gap.
const TRACK: f32 = 10.0;
/// Further a line bends from the rank it arrives at across the gap after
/// it, a line back up to a box above: its head and some of the line
/// before it, so the head does not sit on the curve of the bend.
const ARRIVE: f32 = 14.0;
/// Room at each end of a line within a rank, for its head.
const LEVEL_END: f32 = 20.0;
/// Between a line within a rank and its label.
const LEVEL_LABEL: f32 = 4.0;
/// More room at each end that carries a mark: a multiplicity.
const LEVEL_MARK: f32 = 28.0;
/// Between two lines that join the same two boxes within a rank.
const LEVEL_STEP: f32 = 16.0;
/// How far such a line stays from the corners of its boxes.
const LEVEL_CORNER: f32 = 8.0;
/// How far before the boxes a line runs that passes over one between
/// its two.
const LEVEL_OVER: f32 = 14.0;
/// Lays `nodes` out. Edges that name a missing node are dropped.
pub fn layout(nodes: &[Node], edges: &[Edge], direction: Direction) -> Layout {
    framed(nodes, &[], &[], edges, &[], direction)
}

/// Lays `nodes` out with `frames` around some of them, all in one go:
/// `within[i]` is the innermost frame around node `i`, if any. An edge end
/// past the nodes, `nodes.len() + f`, is frame `f` itself: the line ends
/// at its border. Edges that name neither are dropped, and so is an edge
/// between a frame and what it holds. `holds` keep boxes against others
/// ([`hold`]): one held into another rank is a line that ranks and orders
/// the two and is not drawn, as `dot`'s invisible edge; the rest hold a
/// box in its neighbor's rank.
pub fn framed(
    nodes: &[Node],
    frames: &[Frame],
    within: &[Option<usize>],
    edges: &[Edge],
    holds: &[hold::Hold],
    direction: Direction,
) -> Layout {
    spaced(
        nodes,
        frames,
        within,
        edges,
        holds,
        direction,
        Spacing::default(),
    )
}

/// [`framed`], the boxes as far apart as `spacing` asks.
pub fn spaced(
    nodes: &[Node],
    frames: &[Frame],
    within: &[Option<usize>],
    edges: &[Edge],
    holds: &[hold::Hold],
    direction: Direction,
    spacing: Spacing,
) -> Layout {
    let n = nodes.len();
    let mut same: Vec<(usize, hold::Same)> = Vec::new();
    let mut along: Vec<(usize, usize, usize)> = Vec::new();
    let mut unheld: Vec<(usize, hold::Unheld)> = Vec::new();
    let mut lines = edges.to_vec();
    // Where the lines lead along the flow, between boxes: a hold into
    // another rank that they lead the other way would turn a line around.
    let mut leads: Vec<Vec<usize>> = vec![Vec::new(); n];
    for edge in edges {
        if edge.from < n && edge.to < n && edge.from != edge.to && edge.level.is_none() {
            leads[edge.from].push(edge.to);
        }
    }
    for (index, held) in holds.iter().enumerate() {
        let after = match held.word.place(direction) {
            hold::Place::Beside { after } => Some(after),
            hold::Place::Level => None,
            hold::Place::Rank { later } => {
                if let Some(why) = hold::ranked_check(n, within, held) {
                    unheld.push((index, why));
                    continue;
                }
                let (upper, lower) = if later {
                    (held.beside, held.node)
                } else {
                    (held.node, held.beside)
                };
                if reaches(&leads, lower, upper) {
                    unheld.push((index, hold::Unheld::Against));
                    continue;
                }
                leads[upper].push(lower);
                lines.push(Edge {
                    from: upper,
                    to: lower,
                    label_width: 0.0,
                    label_height: 0.0,
                    hidden: true,
                    apart: true,
                    marked: false,
                    arrival: 0,
                    departure: 0,
                    label_before: None,
                    level: None,
                    tree: None,
                });
                along.push((index, upper, lower));
                continue;
            }
        };
        same.push((
            index,
            hold::Same {
                node: held.node,
                beside: held.beside,
                after,
            },
        ));
    }
    let kept: Vec<hold::Same> = same.iter().map(|(_, hold)| *hold).collect();
    let mut laid = in_ranks(nodes, frames, within, &lines, &kept, (direction, spacing));
    laid.routes.truncate(edges.len());
    unheld.extend(laid.unheld.iter().map(|&(at, why)| (same[at].0, why)));
    // A box held into another rank that the lines rank the other way.
    let middle = |r: Rect| match direction {
        Direction::Down => r.y + r.h / 2.0,
        Direction::Right => r.x + r.w / 2.0,
    };
    for (index, upper, lower) in along {
        if middle(laid.nodes[lower]) <= middle(laid.nodes[upper]) + 0.5 {
            unheld.push((index, hold::Unheld::Against));
        }
    }
    unheld.sort_by_key(|(index, _)| *index);
    laid.unheld = unheld;
    laid
}

/// Whether `leads` (for each box, the boxes its lines lead to) lead from
/// `from` to `to`.
fn reaches(leads: &[Vec<usize>], from: usize, to: usize) -> bool {
    let mut seen = vec![false; leads.len()];
    let mut stack = vec![from];
    while let Some(at) = stack.pop() {
        if at == to {
            return true;
        }
        if std::mem::replace(&mut seen[at], true) {
            continue;
        }
        stack.extend(leads[at].iter().copied().filter(|&next| !seen[next]));
    }
    false
}

/// [`framed`] with every hold in its neighbor's rank.
fn in_ranks(
    nodes: &[Node],
    frames: &[Frame],
    within: &[Option<usize>],
    edges: &[Edge],
    holds: &[hold::Same],
    (direction, spacing): (Direction, Spacing),
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
            unheld: (0..holds.len())
                .map(|i| (i, hold::Unheld::Missing))
                .collect(),
        };
    }

    let mut tree = (!frames.is_empty()).then(|| nest::Tree::new(frames, within, n, direction));
    // A line within a rank holds its second box beside its first, as a
    // hold does. Where that cannot be it is a line like any other: the
    // two are joined by another line, or lie in different frames.
    let wanted: Vec<(usize, hold::Same)> = edges
        .iter()
        .enumerate()
        .filter_map(|(index, edge)| {
            let after = edge.level?;
            let hold = hold::Same {
                node: edge.to,
                beside: edge.from,
                after: Some(after),
            };
            let boxes = edge.from < n && edge.to < n && edge.from != edge.to;
            // Room a box keeps on the side the line would run, for a note
            // or its own loop, is in its way.
            let clear = boxes && {
                let (first, second) = (&nodes[edge.from], &nodes[edge.to]);
                let (leaves, arrives) = if after {
                    (first.loop_extra, second.before)
                } else {
                    (first.before, second.loop_extra)
                };
                leaves == 0.0 && arrives == 0.0
            };
            (clear && !edge.hidden).then_some((index, hold))
        })
        .collect();
    let between: Vec<Edge> = edges
        .iter()
        .filter(|edge| edge.level.is_none())
        .copied()
        .collect();
    let every: Vec<hold::Same> = wanted
        .iter()
        .map(|(_, hold)| *hold)
        .chain(holds.iter().copied())
        .collect();
    // A held box is ranked as one with its neighbor.
    let (holds, unheld) = hold::check(n, within, &between, &every);
    let group = hold::groups(n, &holds);
    // Each such line that holds, with the room it asks for. One whose
    // hold is not kept runs within the rank all the same where its boxes
    // stand in one already (the same line written twice, or back): as
    // an edge it would join two boxes of one rank, and be lost.
    let mut level: Vec<Option<Level>> = vec![None; edges.len()];
    for (at, (index, hold)) in wanted.iter().enumerate() {
        let kept = !unheld.iter().any(|(not, _)| *not == at);
        if kept || group[hold.node] == group[hold.beside] {
            level[*index] = Some(Level {
                after: hold.after == Some(true),
                kept,
                span: spacing.cross,
            });
        }
    }
    let unheld: Vec<(usize, hold::Unheld)> = unheld
        .into_iter()
        .filter(|(index, _)| *index >= wanted.len())
        .map(|(index, why)| (index - wanted.len(), why))
        .collect();
    // The ranks know nothing of such a line: it is no edge to them. The
    // box it holds keeps room on the side it is held from, for the line,
    // its heads, its multiplicities and its label.
    let written = edges;
    let mut roomy = nodes.to_vec();
    let mut inner = edges.to_vec();
    // One whose hold could not be is a line like any other from here on.
    for (edge, line) in inner.iter_mut().zip(&level) {
        if line.is_none() {
            edge.level = None;
        }
    }
    let span_of = |edge: &Edge| {
        let label = match direction {
            Direction::Down => edge.label_width,
            Direction::Right => edge.label_height,
        };
        let marks = if edge.marked { 2.0 * LEVEL_MARK } else { 0.0 };
        (label + 2.0 * LEVEL_END + marks).max(spacing.cross)
    };
    let leveled: Vec<bool> = level.iter().map(Option::is_some).collect();
    for (index, line) in level.iter_mut().enumerate() {
        let Some(line) = line else { continue };
        let edge = written[index];
        // Of the lines between the same two only one holds: the room it
        // asks for is that of the widest label among them, which all
        // stand between the two.
        line.span = written
            .iter()
            .enumerate()
            .filter(|(at, other)| leveled[*at] && same_pair(other, &edge))
            .map(|(_, other)| span_of(other))
            .fold(span_of(&edge), f32::max);
        let held = &mut roomy[edge.to];
        let side = if line.after {
            &mut held.before
        } else {
            &mut held.loop_extra
        };
        if line.kept {
            *side += line.span - spacing.cross;
        }
        inner[index] = Edge {
            from: usize::MAX,
            to: usize::MAX,
            ..edge
        };
    }
    let (nodes, edges) = (roomy.as_slice(), inner.as_slice());
    let grouped = hold::ranked(edges, &group);
    let (rank, back) = match &tree {
        Some(tree) => nest::ranks(tree, &grouped),
        None => ranks(n, &grouped),
    };
    // A box of rank r takes rank 2r: every line passes a rank between its
    // boxes, where a label takes room (docs/adr/0020).
    let rank: Vec<usize> = (0..n).map(|x| 2 * rank[group[x]]).collect();
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
        labels,
    } = slots_of(nodes, edges, &rank, &back, direction, nested);
    for slot in &mut slots {
        slot.gap = spacing.cross;
    }
    let mut borders = Vec::new();
    let mut ports = Vec::new();
    let mut jogs = nest::Jogs::new();
    let mut room = Vec::new();
    let framing = tree.as_ref().map(|tree| (held.as_slice(), tree.count()));
    let flat: Vec<bool> = slots
        .iter()
        .map(|slot| slot.real.is_some_and(|real| nodes[real].flat))
        .collect();
    doubled::order_ranks(&mut rows, &mut slots, framing, &holds);
    let flips = doubled::label_sides(&mut slots, &rows, &chains, &labels, edges, direction);
    room_for_labels(nodes, &mut slots, &rows, &chains, edges, &labels);
    match &mut tree {
        Some(tree) => {
            // A frame too narrow for the lines to pass its name grows, once.
            for attempt in 0..2 {
                borders = place_framed(nodes, &mut slots, &rows, &held, tree);
                ports = self::ports(nodes, &slots, &chains, edges, &labels);
                let placer = nest::Placer::new(&slots, &rows, &held, tree);
                for wants in doubled::point_wants(&slots, &rows, &chains, &ports, &flat) {
                    placer.fit(&mut slots, &mut borders, &wants);
                }
                // Where a line meets a frame's border follows it.
                for wants in doubled::anchor_wants(&slots, &rows) {
                    placer.fit(&mut slots, &mut borders, &wants);
                }
                ports = self::ports(nodes, &slots, &chains, edges, &labels);
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
                // A point before a pass beside a frame's name heads for it.
                for wants in doubled::pass_wants(&slots, &rows, &chains, &jogs) {
                    placer.fit(&mut slots, &mut borders, &wants);
                }
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
            place_across(&mut slots, &rows);
            ports = self::ports(nodes, &slots, &chains, edges, &labels);
            for (row, wants) in rows
                .iter()
                .zip(doubled::point_wants(&slots, &rows, &chains, &ports, &flat))
            {
                fit_to(&mut slots, row, &wants);
            }
            ports = self::ports(nodes, &slots, &chains, edges, &labels);
        }
    }
    let gap_count = rows.len().saturating_sub(1);
    let Tracked {
        tracks,
        counts,
        into_box,
        arrive,
    } = tracks(
        &slots, edges, &chains, &ports, &first, &jogs, gap_count, &back,
    );
    let mut gaps = doubled::rank_gaps(&counts, &into_box, spacing.along);
    // A gap a line arrives back up from bends its lines further down.
    for (gap, &arrives) in gaps.iter_mut().zip(&arrive) {
        if arrives {
            *gap += ARRIVE;
        }
    }
    // Lines within a rank that pass boxes between their two run over and
    // under them in lanes (`level_route`): the gaps beside their rank, or
    // the frame they stand in where it begins or ends there, keep room.
    let lanes = Lanes::of(
        &level,
        written,
        (&rank, &rows, &slots),
        (within, frames, &spans),
        direction,
    );
    // Where frames end and begin, a gap grows by their room; lines bend
    // past the frames that end there.
    let stacks = tree
        .as_ref()
        .map(|tree| nest::Stacks::new(tree, &spans, &room, (&lanes.above, &lanes.below)));
    let (ending, beginning) = match &stacks {
        Some(stacks) => stacks.zones(&spans, rows.len()),
        None => (vec![0.0; rows.len()], vec![0.0; rows.len()]),
    };
    for (index, gap) in gaps.iter_mut().enumerate() {
        *gap += ending[index] + beginning[index + 1];
        *gap = gap.max(lanes.gaps.get(index).copied().unwrap_or(0.0));
    }
    join::room(&mut gaps, edges, &chains, &rows, n);
    let bands = place_along(&mut slots, &rows, &gaps);
    let rects: Vec<Rect> = (0..n).map(|i| rect_of(&slots, i, direction)).collect();
    let frame_rects: Vec<Rect> = match (&tree, &stacks) {
        (Some(tree), Some(stacks)) => (0..tree.count())
            .map(|f| frame_rect(f, &spans, &bands, &borders, stacks, tree, direction))
            .collect(),
        _ => Vec::new(),
    };
    // The lines within a rank, each between its two boxes; those that
    // join the same two run side by side, their labels on either side.
    // Those that pass boxes between stay inside the frame both stand in.
    let mut level_routes: Vec<Option<Route>> = vec![None; written.len()];
    for (index, line) in level.iter().enumerate() {
        let Some(line) = line else { continue };
        let edge = &written[index];
        let same = |at: &usize| level[*at].is_some() && same_pair(&written[*at], edge);
        let count = (0..written.len()).filter(same).count();
        let nth = (0..index).filter(same).count();
        let along = (0..written.len())
            .filter(same)
            .map(|at| match direction {
                Direction::Down => written[at].label_height,
                Direction::Right => written[at].label_width,
            })
            .fold(0.0, f32::max);
        let others: Vec<Rect> = rects
            .iter()
            .enumerate()
            .filter(|(at, _)| *at != edge.from && *at != edge.to)
            .map(|(_, rect)| *rect)
            .collect();
        let bound = within
            .get(edge.from)
            .copied()
            .flatten()
            .filter(|f| within.get(edge.to).copied().flatten() == Some(*f))
            .and_then(|f| Some((*frame_rects.get(f)?, frames.get(f)?.name.height)));
        level_routes[index] = Some(level_route(
            (rects[edge.from], rects[edge.to]),
            &others,
            bound,
            edge,
            line,
            (nth, count, along),
            direction,
        ));
    }
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
    let mut loops = vec![Loops::default(); n];
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
            let (half, reach) = loops[edge.from].next(
                (edge.label_width, edge.label_height),
                edge.marked,
                direction,
            );
            let points = orthogonal(
                &self_loop(rects[edge.from], half, reach, direction),
                direction,
            );
            let label_at = loop_label(&points, edge, direction);
            (points, label_at)
        } else {
            let bends: Vec<f32> = bands[first[index]..]
                .iter()
                .zip(&tracks[index])
                .enumerate()
                .map(|(step, (band, &track))| {
                    let gap = first[index] + step;
                    let arrives = arrive.get(gap).is_some_and(|&a| a);
                    band.1
                        + ending[gap]
                        + JOG
                        + if arrives { ARRIVE } else { 0.0 }
                        + track as f32 * TRACK
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
            let mut points = forward_route(
                &chains[index],
                &slots,
                &ends,
                (&bends, &passes),
                ports[index],
                direction,
            );
            // A back edge's chain runs from its target down to its source.
            if back[index] {
                points.reverse();
            }
            // Placed beside its line once the small steps are gone.
            (points, (0.0, 0.0))
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
    // No small steps (`straight`): each line between the boxes it joins.
    let end_of = |at: usize| -> Option<straight::End> {
        if at < n {
            Some(straight::End {
                rect: rects[at],
                flat: nodes[at].flat,
            })
        } else {
            frame_rects
                .get(at - n)
                .map(|&rect| straight::End { rect, flat: true })
        }
    };
    let ends: Vec<_> = edges
        .iter()
        .map(|edge| {
            if edge.from == edge.to || edge.hidden {
                (None, None)
            } else {
                (end_of(edge.from), end_of(edge.to))
            }
        })
        .collect();
    // The labels keep their room: a run moves off them too, and each label
    // then follows its own line.
    let label_rect = |index: usize, points: &[(f32, f32)]| {
        let slot = &slots[chains[index][labels[index]?]];
        let edge = &edges[index];
        let (x, y) = doubled::label_beside(points, slot, edge, flips[index], direction);
        Some(Rect {
            x,
            y,
            w: edge.label_width,
            h: edge.label_height,
        })
    };
    let mut lines: Vec<Vec<(f32, f32)>> = routes.iter().map(|r| r.points.clone()).collect();
    // The names of the frames, which lines keep off.
    let names: Vec<Rect> = frames
        .iter()
        .zip(&frame_rects)
        .filter_map(|(frame, rect)| {
            let name = frame.name;
            (name.width > 0.0).then_some(match direction {
                Direction::Down => Rect {
                    x: rect.x + name.share * rect.w + name.offset,
                    y: rect.y,
                    w: name.width,
                    h: name.height,
                },
                Direction::Right => Rect {
                    x: rect.x,
                    y: rect.y + name.share * rect.h + name.offset,
                    w: name.height,
                    h: name.width,
                },
            })
        })
        .collect();
    // A line back against the flow leaves a diamond's corner another line
    // takes.
    let level_ends: Vec<(f32, f32)> = level_routes
        .iter()
        .flatten()
        .flat_map(|route| route.points.first().into_iter().chain(route.points.last()))
        .copied()
        .collect();
    corner::apart(
        &mut lines,
        &corner::Around {
            edges,
            back: &back,
            nodes,
            rects: &rects,
            frames: &frame_rects,
            names: &names,
            taken: &level_ends,
            label: &label_rect,
            direction,
        },
    );
    straight::unjog(&mut lines, &ends, &rects, &label_rect);
    // Then one step where there were several: a run moved keeps off the
    // boxes and the names of the frames, and out of frames it was not in.
    // The label of a line within a rank is in the way like a box.
    let level_labels = level_routes.iter().flatten().filter_map(|route| {
        (route.label_width > 0.0).then_some(Rect {
            x: route.label_at.0,
            y: route.label_at.1,
            w: route.label_width,
            h: route.label_height,
        })
    });
    let solids: Vec<Rect> = rects
        .iter()
        .copied()
        .chain(names)
        .chain(level_labels)
        .collect();
    straight::merge(
        &mut lines,
        &solids,
        &frame_rects,
        &label_rect,
        direction == Direction::Down,
    );
    bundle::share(&mut bundle::Drawn {
        routes: &mut lines,
        edges,
        chains: &chains,
        rows: &rows,
        slots: &slots,
        rects: &rects,
        nodes,
        back: &back,
        direction,
    });
    join::trees(&mut lines, edges, &rects, direction);
    for (index, (route, line)) in routes.iter_mut().zip(lines).enumerate() {
        if let Some(rect) = label_rect(index, &line) {
            route.label_at = (rect.x, rect.y);
        }
        route.points = line;
    }
    for (route, level) in routes.iter_mut().zip(level_routes) {
        if let Some(level) = level {
            *route = level;
        }
    }
    Layout {
        unheld,
        ..normalize(rects, routes, frame_rects, &kept)
    }
}

/// The room the lanes of lines within a rank take, which pass boxes
/// between their two: above and below a frame's ranks (from its top,
/// its name's band included; from its last rank's end), and in the gaps
/// between ranks (after the rank of the same index).
struct Lanes {
    above: Vec<f32>,
    below: Vec<f32>,
    gaps: Vec<f32>,
}

impl Lanes {
    fn of(
        level: &[Option<Level>],
        written: &[Edge],
        (rank, rows, slots): (&[usize], &[Vec<usize>], &[Slot]),
        (within, frames, spans): (&[Option<usize>], &[Frame], &[nest::Span]),
        direction: Direction,
    ) -> Lanes {
        let mut lanes = Lanes {
            above: vec![0.0; frames.len()],
            below: vec![0.0; frames.len()],
            gaps: vec![0.0; rows.len().saturating_sub(1)],
        };
        for (index, edge) in written.iter().enumerate() {
            let same = |at: usize| level[at].is_some() && same_pair(&written[at], edge);
            // Once for the two: at their first line within the rank.
            if !same(index) || (0..index).any(same) {
                continue;
            }
            let Some(&r) = rank.get(edge.from) else {
                continue;
            };
            let Some(row) = rows.get(r) else { continue };
            let at = |node: usize| row.iter().position(|&slot| slots[slot].real == Some(node));
            let (Some(i), Some(j)) = (at(edge.from), at(edge.to)) else {
                continue;
            };
            let (i, j) = (i.min(j), i.max(j));
            if !row[i + 1..j].iter().any(|&slot| slots[slot].real.is_some()) {
                continue;
            }
            // As `level_route` lays them: in turns over and under, each
            // lane a label and room further out.
            let count = (0..written.len()).filter(|&at| same(at)).count();
            let widest = (0..written.len())
                .filter(|&at| same(at))
                .map(|at| match direction {
                    Direction::Down => written[at].label_height,
                    Direction::Right => written[at].label_width,
                })
                .fold(0.0, f32::max);
            let reach = widest + LEVEL_LABEL;
            let step = (reach + LEVEL_LABEL).max(LEVEL_STEP);
            let need = |lanes: usize| match lanes {
                0 => 0.0,
                _ => LEVEL_OVER + (lanes - 1) as f32 * step + reach,
            };
            let (over, under) = (need(count.div_ceil(2)), need(count / 2));
            let frame = within
                .get(edge.from)
                .copied()
                .flatten()
                .filter(|f| within.get(edge.to).copied().flatten() == Some(*f));
            let span = frame.and_then(|f| Some((f, spans.get(f).copied().flatten()?)));
            match span {
                Some((f, (first, _))) if first == r => {
                    lanes.above[f] = lanes.above[f].max(frames[f].name.height + over);
                }
                _ if r > 0 && over > 0.0 => {
                    lanes.gaps[r - 1] = lanes.gaps[r - 1].max(over + LEVEL_OVER);
                }
                _ => {}
            }
            match span {
                Some((f, (_, last))) if last == r => {
                    lanes.below[f] = lanes.below[f].max(under);
                }
                _ if r < lanes.gaps.len() && under > 0.0 => {
                    lanes.gaps[r] = lanes.gaps[r].max(under + LEVEL_OVER);
                }
                _ => {}
            }
        }
        lanes
    }
}

/// Whether two lines join the same two boxes, either way.
fn same_pair(a: &Edge, b: &Edge) -> bool {
    (a.from, a.to) == (b.from, b.to) || (a.from, a.to) == (b.to, b.from)
}

/// A line within a rank as the layout keeps it.
#[derive(Debug, Clone, Copy)]
struct Level {
    /// Its second box stands after its first across the flow.
    after: bool,
    /// Its hold is kept: the room it asked for is there.
    kept: bool,
    /// How far apart the two boxes stand for it, across the flow.
    span: f32,
}

/// A line within a rank, between the boxes `a` and `b`: straight between
/// the sides that face each other, where the two overlap along the flow;
/// the `nth` of `count` lines between the two, side by side. Its label
/// lies in the room its second box keeps for it, on the side of the line
/// the writer chose, else over it (after it where the flow runs to the
/// right); every second line of several takes the other side. Boxes
/// that do not overlap are joined by one step between them. Where one
/// of `others` stands between the two, the line leaves and arrives
/// before the boxes along the flow and runs over them, its label over
/// that run, or after them and under, in turns, each line of a side in a
/// lane of its own, as wide as the `widest` label along the flow;
/// within `bound`, the frame both stand in and the height of its name,
/// a line takes the side it fits on.
fn level_route(
    (a, b): (Rect, Rect),
    others: &[Rect],
    bound: Option<(Rect, f32)>,
    edge: &Edge,
    line: &Level,
    (nth, count, widest): (usize, usize, f32),
    direction: Direction,
) -> Route {
    // Turned so the flow runs down: across is x, along is y.
    let turn = |r: Rect| match direction {
        Direction::Down => r,
        Direction::Right => Rect {
            x: r.y,
            y: r.x,
            w: r.h,
            h: r.w,
        },
    };
    let back = |(x, y): (f32, f32)| match direction {
        Direction::Down => (x, y),
        Direction::Right => (y, x),
    };
    let (a, b) = (turn(a), turn(b));
    let (start, end) = if a.x <= b.x {
        (a.right(), b.x)
    } else {
        (a.x, b.right())
    };
    let (top, bottom) = (a.y.max(b.y), a.bottom().min(b.bottom()));
    let (points, at) = if top < bottom {
        let reach = ((bottom - top) / 2.0 - LEVEL_CORNER).max(0.0);
        let shift = (nth as f32 - (count as f32 - 1.0) / 2.0) * LEVEL_STEP;
        let y = (top + bottom) / 2.0 + shift.clamp(-reach, reach);
        (vec![(start, y), (end, y)], y)
    } else {
        let mid = (start + end) / 2.0;
        (
            vec![(start, a.cy()), (mid, a.cy()), (mid, b.cy()), (end, b.cy())],
            a.cy().min(b.cy()),
        )
    };
    // The middle of the room before the second box; where the hold is
    // not kept that room is not there, and the box may stand on the
    // other side: the middle of the line itself.
    let middle = match (line.kept, line.after) {
        (false, _) => (start + end) / 2.0,
        (true, true) => b.x - line.span / 2.0,
        (true, false) => b.right() + line.span / 2.0,
    };
    let (across, along) = match direction {
        Direction::Down => (edge.label_width, edge.label_height),
        Direction::Right => (edge.label_height, edge.label_width),
    };
    let (low, high) = (start.min(end), start.max(end));
    let passed: Vec<Rect> = others
        .iter()
        .map(|&other| turn(other))
        .filter(|o| o.x < high && o.right() > low && o.y < at + 1.0 && o.bottom() > at - 1.0)
        .collect();
    if !passed.is_empty() {
        // The lines between the same two take turns over the boxes and
        // under them, each lane further out than the one before on its
        // side by a label and room; a side where a line and its label
        // would leave the frame both stand in (or run over its name) is
        // left to the other.
        let reach = widest.max(along) + LEVEL_LABEL;
        let step = (reach + LEVEL_LABEL).max(LEVEL_STEP);
        let over = passed.iter().map(|o| o.y).fold(a.y.min(b.y), f32::min) - LEVEL_OVER;
        let under = passed
            .iter()
            .map(|o| o.bottom())
            .fold(a.bottom().max(b.bottom()), f32::max)
            + LEVEL_OVER;
        let bound = bound.map(|(frame, name)| {
            let name = if direction == Direction::Down {
                name
            } else {
                0.0
            };
            (turn(frame), name)
        });
        // How far a lane and its label reach past what is above or below
        // the two, or past the frame: next to nothing where it fits, as
        // the room kept for it is just as large (`Lanes`).
        let (low_x, high_x) = (a.x.min(b.x), a.right().max(b.right()));
        let ahead: Vec<Rect> = others
            .iter()
            .map(|&other| turn(other))
            .filter(|o| o.x < high_x && o.right() > low_x)
            .collect();
        let past = |below: bool, lane: usize| {
            let out = lane as f32 * step + reach;
            match below {
                true => {
                    let reached = under + out;
                    let next = ahead
                        .iter()
                        .map(|o| o.y)
                        .filter(|&y| y >= under - LEVEL_OVER)
                        .chain(bound.map(|(frame, _)| frame.bottom()))
                        .fold(f32::INFINITY, f32::min);
                    (reached - next).max(0.0)
                }
                false => {
                    let reached = over - out;
                    let next = ahead
                        .iter()
                        .map(|o| o.bottom())
                        .filter(|&y| y <= over + LEVEL_OVER)
                        .chain(bound.map(|(frame, name)| frame.y + name))
                        .fold(f32::NEG_INFINITY, f32::max);
                    (next - reached).max(0.0)
                }
            }
        };
        let mut lanes = [0usize; 2];
        let mut mine = (false, 0);
        for k in 0..count.max(nth + 1) {
            // Its turn's side where it fits, else the other; where neither
            // does, the one it reaches past the least.
            let wanted = k % 2 == 1;
            let lane = |below: bool| lanes[usize::from(below)];
            let below = [wanted, !wanted]
                .into_iter()
                .find(|&below| past(below, lane(below)) < 0.5)
                .unwrap_or_else(|| {
                    match past(!wanted, lane(!wanted)) < past(wanted, lane(wanted)) {
                        true => !wanted,
                        false => wanted,
                    }
                });
            if k == nth {
                mine = (below, lanes[usize::from(below)]);
            }
            lanes[usize::from(below)] += 1;
        }
        let (below, lane) = mine;
        // The lanes of a side nest: an outer one leaves its box further
        // from the side that faces the other, short of the middle, where
        // the lines along the flow leave.
        let total = lanes[usize::from(below)];
        let inset = |r: Rect| {
            let first = (r.w / 4.0).min(LEVEL_END);
            let last = (r.w / 2.0 - LEVEL_CORNER).max(first);
            match total {
                0 | 1 => first,
                _ => {
                    let spread = (last - first).min((total - 1) as f32 * LEVEL_STEP / 2.0);
                    first + spread * lane as f32 / (total - 1) as f32
                }
            }
        };
        let (leave, arrive) = if a.x <= b.x {
            (a.right() - inset(a), b.x + inset(b))
        } else {
            (a.x + inset(a), b.right() - inset(b))
        };
        let out = lane as f32 * step;
        let (from, to, run, words) = match below {
            true => (
                a.bottom(),
                b.bottom(),
                under + out,
                under + out + LEVEL_LABEL,
            ),
            false => (a.y, b.y, over - out, over - out - along - LEVEL_LABEL),
        };
        let points = [(leave, from), (leave, run), (arrive, run), (arrive, to)];
        let label = ((leave + arrive - across) / 2.0, words);
        return Route {
            points: points.into_iter().map(back).collect(),
            label_at: back(label),
            label_width: edge.label_width,
            label_height: edge.label_height,
        };
    }
    let chosen = edge.label_before.unwrap_or(direction == Direction::Down);
    let before = chosen != (nth % 2 == 1);
    let beside = if before {
        at - along - LEVEL_LABEL
    } else {
        at + LEVEL_LABEL
    };
    Route {
        points: points.into_iter().map(back).collect(),
        label_at: back((middle - across / 2.0, beside)),
        label_width: edge.label_width,
        label_height: edge.label_height,
    }
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
    /// For a point right before the end of its line where that end is met
    /// in the middle of its side (a circle, a diamond): that end's slot.
    /// Two such points of one end keep no room between them, so their
    /// lines run together into it, as they meet it.
    meets: Option<usize>,
    /// The room it keeps to a box beside it in its rank ([`Spacing`]).
    gap: f32,
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
            meets: None,
            gap: CROSS_GAP,
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
    /// For every labelled edge, where along its chain its label lies: a
    /// point in a rank between two ranks of boxes, the one nearest the
    /// middle, as big as the label.
    labels: Vec<Option<usize>>,
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
    let mut labels = vec![None; edges.len()];
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
            // A line from one rank of boxes to the next passes between
            // them inside the frames around both ends only, as it did in the
            // gap between them before the ranks were doubled.
            frames.push(nested.map_or_else(Vec::new, |(tree, spans)| {
                if lower == upper + 2 {
                    let (a, b) = (tree.around_end(top), tree.around_end(bottom));
                    let same = a.iter().zip(b).take_while(|(x, y)| x == y).count();
                    a[..same].to_vec()
                } else {
                    tree.chain(nest::passing(
                        tree,
                        spans,
                        (top, bottom),
                        (upper, lower),
                        at,
                    ))
                }
            }));
            row.push(dummy);
            link(&mut slots, *chain.last().unwrap(), dummy);
            chain.push(dummy);
        }
        let end = slot_of(bottom, false, &mut slots, &mut frames, &mut rows);
        let before = *chain.last().unwrap();
        link(&mut slots, before, end);
        if before != chain[0] && slots[end].real.is_some_and(|x| !nodes[x].flat) {
            slots[before].meets = Some(end);
        }
        chain.push(end);
        // The label takes the rank between two ranks of boxes nearest the
        // middle of the line; of two, the lower. A hidden line keeps that
        // room too, for a label its drawer puts there.
        let labelled = edge.label_width > 0.0 || edge.label_height > 0.0;
        if labelled && lower > upper + 1 {
            let at = 2 * ((lower - upper) / 4) + 1;
            let (cross, along) = doubled::label_room(edge, direction);
            let slot = &mut slots[chain[at]];
            slot.pack_cross = cross;
            slot.box_along = along;
            labels[index] = Some(at);
        }
        chains[index] = chain;
        first[index] = upper;
    }
    Built {
        slots,
        rows,
        chains,
        first,
        frames,
        labels,
    }
}

fn link(slots: &mut [Slot], from: usize, to: usize) {
    slots[from].succs.push(to);
    slots[to].preds.push(from);
}

/// A few down and up passes. A node with no neighbor keeps its place.
/// The order that crosses least stays ([`order`]).
fn sweep(rows: &mut [Vec<usize>], slots: &[Slot]) {
    let mut best = (order::crossings(rows, slots), rows.to_vec());
    for _ in 0..4 {
        for rank in 1..rows.len() {
            let pos = positions(rows, slots.len());
            sort_row(&mut rows[rank], slots, &pos, true);
        }
        for rank in (0..rows.len().saturating_sub(1)).rev() {
            let pos = positions(rows, slots.len());
            sort_row(&mut rows[rank], slots, &pos, false);
        }
        order::transpose(rows, slots, |_, _| true);
        let now = order::crossings(rows, slots);
        if now < best.0 {
            best = (now, rows.to_vec());
        }
    }
    order::keep_best(rows, slots, best);
    order::transpose(rows, slots, |_, _| true);
    order::reroute(rows, slots, |_, _| true);
}

/// The same with frames: each rank keeps the slots of a frame together,
/// and frames side by side in one order on every rank.
fn sweep_framed(rows: &mut [Vec<usize>], slots: &[Slot], frames: &[Vec<usize>], count: usize) {
    let pos = nest::positions(rows, slots);
    let keys = nest::keys(rows, &pos, frames, count);
    for row in rows.iter_mut() {
        nest::sort_row(row, slots, &pos, nest::By::Place, frames, &keys);
    }
    let mut best = (order::crossings(rows, slots), rows.to_vec());
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
        order::transpose(rows, slots, |a, b| frames[a] == frames[b]);
        let now = order::crossings(rows, slots);
        if now < best.0 {
            best = (now, rows.to_vec());
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
    // The order that crosses least stays, and two neighbors in the same
    // frames swap where that crosses less ([`order`]).
    order::keep_best(rows, slots, best);
    order::transpose(rows, slots, |a, b| frames[a] == frames[b]);
    order::reroute(rows, slots, |a, b| frames[a] == frames[b]);
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
            // The one point of a line from one rank of boxes to the next
            // heads for the boxes' middles, as the boxes did for each other
            // before the ranks were doubled; it moves onto its line later.
            let end = |n: &usize| slots[*n].real.is_some() || slots[*n].anchor.is_some();
            let short = s.real.is_none()
                && s.preds.len() == 1
                && s.succs.len() == 1
                && s.preds.iter().chain(&s.succs).all(end);
            let toward = |n: usize| {
                let other = &slots[n];
                let at = center(other);
                if s.real.is_some() || !flat[n] || short {
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

/// What [`tracks`] finds: each line's track in each gap it crosses, how
/// many tracks each gap holds, whether a line goes on into a box there,
/// and whether one arrives at the box before it (a line back up).
struct Tracked {
    tracks: Vec<Vec<usize>>,
    counts: Vec<usize>,
    into_box: Vec<bool>,
    arrive: Vec<bool>,
}

/// For every forward edge, the track it runs across each gap on; and how
/// many tracks each gap needs. Only a drawn line that changes place across
/// the flow needs one; a hidden edge only ranks its ends. Lines moving the
/// same way take tracks in the order that keeps one from crossing another:
/// of two heading right, the one that ends further right runs first, and
/// of two that end together the one that starts nearer; to the left the
/// same, mirrored. Lines whose runs across lie apart, [`TRACK`] at least,
/// share a track: each takes the one after the last track of the lines
/// before it whose runs its own overlaps, so every two that overlap keep
/// that order and cross as they did, and a gap is as tall as the most
/// runs that overlap there (a dense diagram's gaps half as tall).
#[allow(clippy::too_many_arguments)]
fn tracks(
    slots: &[Slot],
    edges: &[Edge],
    chains: &[Vec<usize>],
    ports: &[Ports],
    first: &[usize],
    jogs: &nest::Jogs,
    gaps: usize,
    back: &[bool],
) -> Tracked {
    let cross = |slot: usize, port: f32| center(&slots[slot]) + port;
    let mut tracks: Vec<Vec<usize>> = chains
        .iter()
        .map(|c| vec![0; c.len().saturating_sub(1)])
        .collect();
    // By gap: (edge, step, start, end) of each line that moves across; and
    // whether one of them goes on into a box there.
    let mut into_box = vec![false; gaps];
    let mut arrive = vec![false; gaps];
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
                into_box[gap] |= step + 1 == last && slots[chain[last]].real.is_some();
                // A line back up runs from its source below to its target
                // above, its chain from the target down: its head is at
                // the box before its first gap.
                // A step the line is straightened by is no bend.
                arrive[gap] |= step == 0
                    && (end - start).abs() >= straight::SNAP
                    && back.get(index).is_some_and(|&b| b)
                    && slots[chain[0]].real.is_some();
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
        // The run of each line given a track so far, and its track.
        let mut runs: Vec<(f32, f32, usize)> = Vec::with_capacity(lines.len());
        for (index, step, start, end) in lines {
            let (low, high) = (start.min(end), start.max(end));
            let track = runs
                .iter()
                .filter(|run| run.0 < high + TRACK && low < run.1 + TRACK)
                .map(|run| run.2 + 1)
                .max()
                .unwrap_or(0);
            runs.push((low, high, track));
            tracks[index][step] = track;
            counts[gap] = counts[gap].max(track + 1);
        }
    }
    Tracked {
        tracks,
        counts,
        into_box,
        arrive,
    }
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

/// Between a label and the line beside it that is not its own, where two
/// lines run together.
const LABEL_CLEAR: f32 = 8.0;

/// Between two neighbors of a rank whose lines run together into a box
/// met in the middle: nothing, but where a label lies between the two
/// lines, what keeps it off the one that is not its own. Without it the
/// label's words read on into the next label's.
fn together(slots: &[Slot], a: usize, b: usize) -> f32 {
    let (a, b) = (&slots[a], &slots[b]);
    let after_a = a.real.is_none() && a.pack_cross > 0.0 && a.before == 0.0;
    let before_b = b.real.is_none() && b.before > 0.0;
    if after_a || before_b {
        LABEL_CLEAR
    } else {
        0.0
    }
}

/// Between two neighbors of a rank: less between two lines passing by,
/// none between two that run together into a box met in the middle.
fn gap(slots: &[Slot], a: usize, b: usize) -> f32 {
    if slots[a].meets.is_some() && slots[a].meets == slots[b].meets {
        together(slots, a, b)
    } else if slots[a].real.is_none() && slots[b].real.is_none() {
        DUMMY_GAP
    } else {
        slots[a].gap.max(slots[b].gap)
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
/// with no neighbor barely pulls, and wants to stay beside the box it
/// stands next to in its rank, so it goes where that one goes.
fn fit_row(slots: &mut [Slot], row: &[usize], down: bool) {
    let lone: Vec<bool> = row
        .iter()
        .map(|&slot| slots[slot].preds.is_empty() && slots[slot].succs.is_empty())
        .collect();
    let mut wants: Vec<(usize, f32, f32)> = row
        .iter()
        .map(|&slot| {
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
            (slot, want - s.before - s.box_cross / 2.0, weight)
        })
        .collect();
    // After the box before it; those that lead the rank, before the
    // first box that has a neighbor.
    for index in 1..row.len() {
        if lone[index] {
            let before = row[index - 1];
            wants[index].1 =
                wants[index - 1].1 + slots[before].pack_cross + gap(slots, before, row[index]);
        }
    }
    if let Some(first) = lone.iter().position(|lone| !lone) {
        for index in (0..first).rev() {
            let slot = row[index];
            wants[index].1 =
                wants[index + 1].1 - slots[slot].pack_cross - gap(slots, slot, row[index + 1]);
        }
    }
    fit_to(slots, row, &wants);
}

/// Places `row` as near as its order and gaps allow to `wants`: for each
/// slot in turn, where the left of its packed room wants to lie, and how
/// much it pulls.
fn fit_to(slots: &mut [Slot], row: &[usize], wants: &[(usize, f32, f32)]) {
    if wants.is_empty() {
        return;
    }
    let mut targets = Vec::with_capacity(row.len());
    let mut offset = 0.0;
    for (index, (&slot, &(_, want, weight))) in row.iter().zip(wants).enumerate() {
        targets.push((want - offset, weight));
        offset += slots[slot].pack_cross;
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
/// flow from the middle of each side.
#[derive(Debug, Clone, Copy, Default)]
struct Ports {
    start: f32,
    end: f32,
}

/// A line on one side of a slot, for [`ports`]: the edge, the slot at the
/// other end of its step, the box at the other end of the line, and the
/// slot it heads for.
type Meeting = (usize, usize, usize, usize);

/// Where every line meets its boxes. The lines that meet one side of a
/// box share it in the order of their other ends, so none crosses another
/// there, [`PORT_STEP`] apart where the side is long enough; their labels
/// lie in the ranks between, not at the box. Each lies as near as that
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
    labels: &[Option<usize>],
) -> Vec<Ports> {
    let mut ports = vec![Ports::default(); chains.len()];
    let flat: Vec<bool> = slots
        .iter()
        .map(|slot| slot.real.is_some_and(|real| nodes[real].flat))
        .collect();
    let sides = meetings(slots.len(), chains, edges, labels, |slot| {
        center(&slots[slot])
    });
    let steps: Vec<[Vec<f32>; 2]> = sides
        .iter()
        .zip(&flat)
        .map(|(pair, &flat)| {
            pair.each_ref()
                .map(|side| port_steps(side, flat, slots, chains, labels))
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
                    .map(|&(index, .., other)| {
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
                for (&(index, ..), offset) in side.iter().zip(offsets) {
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

/// The room of the labels that lie between two lines side by side, the
/// line `first` before the line `second` across the flow, with what keeps
/// each off the line that is not its own; nothing where no label lies
/// between them.
fn label_between(
    slots: &[Slot],
    chains: &[Vec<usize>],
    labels: &[Option<usize>],
    first: usize,
    second: usize,
) -> f32 {
    let point = |index: usize| labels[index].map(|at| &slots[chains[index][at]]);
    let after = point(first)
        .filter(|slot| slot.before == 0.0)
        .map_or(0.0, |slot| slot.pack_cross);
    let before = point(second).map_or(0.0, |slot| slot.before);
    if after + before > 0.0 {
        after + before + LABEL_CLEAR
    } else {
        0.0
    }
}

/// Widens each box with flat sides to what the lines on one side of it
/// need, where labels lie between lines that run side by side to the same
/// box: [`PORT_STEP`] between two lines, the label's room where one lies
/// between them, and the corners. A box too narrow for that would squeeze
/// its lines together, and a line would bend around the label just before
/// its arrowhead. Once the ranks are ordered and the labels have their
/// sides, before anything is placed.
fn room_for_labels(
    nodes: &[Node],
    slots: &mut [Slot],
    rows: &[Vec<usize>],
    chains: &[Vec<usize>],
    edges: &[Edge],
    labels: &[Option<usize>],
) {
    // Nothing is placed yet: a point lies where it stands in its rank.
    let mut place = vec![0.0; slots.len()];
    for row in rows {
        for (at, &slot) in row.iter().enumerate() {
            place[slot] = at as f32;
        }
    }
    let sides = meetings(slots.len(), chains, edges, labels, |slot| place[slot]);
    for (slot, pair) in sides.iter().enumerate() {
        // A circle or a diamond is met wherever its lines ask.
        if !slots[slot].real.is_some_and(|real| nodes[real].flat) {
            continue;
        }
        let mut need: f32 = 0.0;
        for side in pair {
            let steps = port_steps(side, true, slots, chains, labels);
            if steps.iter().any(|&step| step > PORT_STEP) {
                need = need.max(steps.iter().sum::<f32>() + 2.0 * PORT_CORNER);
            }
        }
        let more = need - slots[slot].box_cross;
        if more > 0.0 {
            slots[slot].box_cross += more;
            slots[slot].pack_cross += more;
        }
    }
}

/// At each slot, the lines leaving it and those arriving: (edge, the slot
/// at the other end of the step, the box at the other end of the line, the
/// slot it heads for), in the order of the other ends of the steps, each
/// where `at` says it lies. A line from one rank of boxes to the next with
/// no label heads for the box at its other end, as if the point between
/// were not there; that point moves onto the line afterwards
/// ([`point_wants`]). One reading for [`ports`] and for the room they
/// need ([`room_for_labels`]).
fn meetings(
    slots: usize,
    chains: &[Vec<usize>],
    edges: &[Edge],
    labels: &[Option<usize>],
    at: impl Fn(usize) -> f32,
) -> Vec<[Vec<Meeting>; 2]> {
    let mut sides: Vec<[Vec<Meeting>; 2]> = (0..slots).map(|_| Default::default()).collect();
    for (index, chain) in chains.iter().enumerate() {
        if chain.len() < 2 || edges[index].hidden {
            continue;
        }
        let last = chain.len() - 1;
        let plain = chain.len() == 3 && labels[index].is_none();
        let (ahead, behind) = if plain {
            (chain[last], chain[0])
        } else {
            (chain[1], chain[last - 1])
        };
        sides[chain[0]][0].push((index, chain[1], chain[last], ahead));
        sides[chain[last]][1].push((index, chain[last - 1], chain[0], behind));
    }
    for side in sides.iter_mut().flatten() {
        side.sort_by(|a, b| at(a.1).total_cmp(&at(b.1)).then(a.0.cmp(&b.0)));
    }
    sides
}

/// How far apart the neighbors on `side` of a box keep. On a circle or a
/// diamond (not `flat`), lines to other boxes meet in the middle unless
/// lines side by side would cross them.
fn port_steps(
    side: &[Meeting],
    flat: bool,
    slots: &[Slot],
    chains: &[Vec<usize>],
    labels: &[Option<usize>],
) -> Vec<f32> {
    let paired = side.windows(2).any(|lines| lines[0].2 == lines[1].2);
    side.windows(2)
        .map(|lines| match (flat, lines[0].2 == lines[1].2) {
            // Side by side between the same two boxes, with a label
            // between them: its room, so both run straight past it.
            (_, true) => {
                PORT_STEP.max(label_between(slots, chains, labels, lines[0].0, lines[1].0))
            }
            (true, false) => PORT_STEP,
            (false, false) if paired => PORT_STEP / 2.0,
            (false, false) => 0.0,
        })
        .collect()
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
/// flow.
fn forward_route(
    chain: &[usize],
    slots: &[Slot],
    ends: &[Option<Rect>],
    (bends, passes): (&[f32], &[Pass]),
    ports: Ports,
    direction: Direction,
) -> Vec<(f32, f32)> {
    if chain.len() < 2 {
        return Vec::new();
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
            // A dummy is a point the edge passes through, beside its label
            // if it has one.
            None => match direction {
                Direction::Down => (center(slot), slot.primary),
                Direction::Right => (slot.primary, center(slot)),
            },
        }
    };
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
        points.push(next);
    }
    simplify(points)
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
        Direction::Down => (far_x + LOOP_WORDS, mid_y - edge.label_height / 2.0),
        Direction::Right => (mid_x - edge.label_width / 2.0, far_y + LOOP_WORDS),
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
            unheld: Vec::new(),
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
        unheld: Vec::new(),
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
            round: false,
        }
    }

    pub(super) fn edge(from: usize, to: usize) -> Edge {
        Edge {
            from,
            to,
            label_width: 0.0,
            label_height: 0.0,
            hidden: false,
            apart: false,
            marked: false,
            arrival: 0,
            departure: 0,
            label_before: None,
            level: None,
            tree: None,
        }
    }

    #[test]
    fn a_line_within_a_rank_runs_straight_between_two_boxes_side_by_side() {
        let nodes = [
            node(80.0, 40.0),
            node(60.0, 70.0),
            node(50.0, 30.0),
            node(90.0, 40.0),
        ];
        let level = |from, to, after| Edge {
            level: Some(after),
            label_width: 120.0,
            label_height: 14.0,
            ..edge(from, to)
        };
        // 0 over 3; 1 right of 0 and 2 left of it, written follower first.
        let edges = [edge(0, 3), level(0, 1, true), level(2, 0, true)];
        for direction in [Direction::Down, Direction::Right] {
            let laid = layout(&nodes, &edges, direction);
            assert!(laid.unheld.is_empty());
            // Across the flow and along it, whichever way it runs.
            let turn = |r: Rect| match direction {
                Direction::Down => (r.x, r.y, r.w, r.h),
                Direction::Right => (r.y, r.x, r.h, r.w),
            };
            let [a, b, c, below] = [0, 1, 2, 3].map(|i| turn(laid.nodes[i]));
            assert!(
                c.0 + c.2 <= a.0 && a.0 + a.2 <= b.0,
                "{direction:?}: c, a, b"
            );
            assert!(below.1 >= a.1 + a.3, "{direction:?}: the rank after");
            // Room for the label and the heads between the two.
            let label = match direction {
                Direction::Down => 120.0,
                Direction::Right => 14.0,
            };
            assert!(
                b.0 - (a.0 + a.2) >= label + 2.0 * LEVEL_END - 0.5,
                "{direction:?}: {a:?} {b:?}"
            );
            let route = &laid.routes[1];
            let ends: Vec<(f32, f32)> = route
                .points
                .iter()
                .map(|&(x, y)| match direction {
                    Direction::Down => (x, y),
                    Direction::Right => (y, x),
                })
                .collect();
            assert_eq!(ends.len(), 2, "{direction:?}: straight");
            assert!((ends[0].0 - (a.0 + a.2)).abs() < 0.5 && (ends[1].0 - b.0).abs() < 0.5);
            assert!((ends[0].1 - ends[1].1).abs() < 0.01, "{direction:?}: level");
            assert!(ends[0].1 > a.1.max(b.1) && ends[0].1 < (a.1 + a.3).min(b.1 + b.3));
            assert!(
                !laid
                    .nodes
                    .iter()
                    .enumerate()
                    .any(|(i, &x)| { laid.nodes[i + 1..].iter().any(|&y| overlaps(x, y)) })
            );
        }
    }

    #[test]
    fn lines_within_a_rank_written_twice_back_or_past_a_box_are_all_drawn() {
        let nodes = [node(80.0, 60.0), node(60.0, 60.0), node(70.0, 60.0)];
        let level = |from, to, after| Edge {
            level: Some(after),
            label_width: 30.0,
            label_height: 14.0,
            ..edge(from, to)
        };
        // There, back, on, and from the first past the second.
        let edges = [
            level(0, 1, true),
            level(1, 0, false),
            level(1, 2, true),
            level(0, 2, true),
        ];
        let laid = layout(&nodes, &edges, Direction::Down);
        let [a, b, c] = [0, 1, 2].map(|i| laid.nodes[i]);
        assert!(a.right() <= b.x && b.right() <= c.x, "a b c");
        assert!(laid.routes.iter().all(|route| route.points.len() >= 2));
        // The two between a and b lie side by side, a label on either side.
        let (there, back) = (&laid.routes[0], &laid.routes[1]);
        assert!((there.points[0].1 - back.points[0].1).abs() >= LEVEL_STEP - 0.5);
        assert!(there.label_at.1 < there.points[0].1 && back.label_at.1 > back.points[0].1);
        // Past b the line runs over it, and its label over that run.
        let far = &laid.routes[3];
        assert_eq!(far.points.len(), 4);
        let over = far.points[1].1;
        assert!(over < b.y && (over - far.points[2].1).abs() < 0.01);
        assert!(far.label_at.1 + far.label_height <= over);
        // A label of one line is not on a box or on another label.
        let labels: Vec<Rect> = laid
            .routes
            .iter()
            .map(|route| Rect {
                x: route.label_at.0,
                y: route.label_at.1,
                w: route.label_width,
                h: route.label_height,
            })
            .collect();
        for (i, &label) in labels.iter().enumerate() {
            assert!(!laid.nodes.iter().any(|&node| overlaps(label, node)), "{i}");
            assert!(
                !labels[i + 1..].iter().any(|&other| overlaps(label, other)),
                "{i}"
            );
        }
    }

    #[test]
    fn a_line_within_a_rank_keeps_out_of_a_note_s_room_and_room_for_its_marks() {
        // The first box keeps room after it: a note stands there.
        let mut nodes = [node(80.0, 40.0), node(60.0, 40.0)];
        nodes[0].loop_extra = 90.0;
        let level = Edge {
            level: Some(true),
            ..edge(0, 1)
        };
        let laid = layout(&nodes, &[level], Direction::Down);
        assert!(
            laid.nodes[1].y >= laid.nodes[0].bottom(),
            "a line like any other"
        );
        // Multiplicities at its ends ask for more room between the two.
        let nodes = [node(80.0, 40.0), node(60.0, 40.0)];
        let gap = |marked| {
            let laid = layout(&nodes, &[Edge { marked, ..level }], Direction::Down);
            laid.nodes[1].x - laid.nodes[0].right()
        };
        assert!(gap(true) > gap(false));
        assert!(gap(true) >= 2.0 * (LEVEL_END + LEVEL_MARK) - 0.5);
        // A hidden one is no line of the picture.
        let hidden = Edge {
            hidden: true,
            ..level
        };
        assert!(
            layout(&nodes, &[hidden], Direction::Down).routes[0]
                .points
                .is_empty()
        );
    }

    #[test]
    fn a_line_within_a_rank_between_boxes_another_line_joins_is_a_line_like_any() {
        let nodes = [node(80.0, 40.0), node(60.0, 40.0)];
        let edges = [
            edge(0, 1),
            Edge {
                level: Some(true),
                ..edge(0, 1)
            },
        ];
        let laid = layout(&nodes, &edges, Direction::Down);
        assert!(
            laid.nodes[1].y >= laid.nodes[0].bottom(),
            "one under the other"
        );
        assert!(laid.routes.iter().all(|route| route.points.len() >= 2));
    }

    /// Lines within a rank that point against each other (a toggle,
    /// `a -> b` and `b -> a`) both run within it; the second's hold is not
    /// kept, so its label stands at the middle of its own line, not in
    /// room on the far side of its box. Past a box between them, the
    /// lines between the same two take turns over the boxes and under.
    #[test]
    fn lines_within_a_rank_against_each_other_keep_their_labels_and_ways() {
        let labelled = |from, to| Edge {
            level: Some(true),
            label_width: 30.0,
            label_height: 14.0,
            ..edge(from, to)
        };
        let nodes = [node(80.0, 40.0), node(60.0, 40.0)];
        let laid = layout(&nodes, &[labelled(0, 1), labelled(1, 0)], Direction::Down);
        let (a, b) = (laid.nodes[0], laid.nodes[1]);
        assert!(b.x > a.right() && (a.y - b.y).abs() < 0.5, "side by side");
        for route in &laid.routes {
            let middle = route.label_at.0 + route.label_width / 2.0;
            assert!(middle > a.right() && middle < b.x, "{:?}", route.label_at);
        }
        // The room between the two is that of the wider label, the one of
        // the line that does not hold too.
        let wide = Edge {
            label_width: 200.0,
            ..labelled(1, 0)
        };
        let laid = layout(&nodes, &[labelled(0, 1), wide], Direction::Down);
        let (a, b) = (laid.nodes[0], laid.nodes[1]);
        for route in &laid.routes {
            let (x, w) = (route.label_at.0, route.label_width);
            assert!(
                x >= a.right() && x + w <= b.x,
                "{x} {w} between {a:?} {b:?}"
            );
        }
        // Four lines past a box between, a box under each of the three:
        // two lanes over, two under, a label and room apart, clear of the
        // boxes under them.
        let nodes = [
            node(80.0, 40.0),
            node(60.0, 40.0),
            node(60.0, 40.0),
            node(60.0, 40.0),
            node(60.0, 40.0),
            node(60.0, 40.0),
        ];
        let edges = [
            labelled(0, 1),
            labelled(1, 2),
            labelled(0, 2),
            labelled(2, 0),
            labelled(0, 2),
            labelled(2, 0),
            edge(0, 3),
            edge(1, 4),
            edge(2, 5),
        ];
        let laid = layout(&nodes, &edges, Direction::Down);
        let row = &laid.nodes[..3];
        let top = row.iter().map(|r| r.y).fold(f32::INFINITY, f32::min);
        let bottom = row.iter().map(|r| r.bottom()).fold(0.0, f32::max);
        let next = laid.nodes[3..]
            .iter()
            .map(|r| r.y)
            .fold(f32::INFINITY, f32::min);
        let run = |at: usize| laid.routes[at].points[1].1;
        assert!(
            run(4) < run(2) && run(2) < top,
            "over: {} {}",
            run(2),
            run(4)
        );
        assert!(
            run(5) > run(3) && run(3) > bottom,
            "under: {} {}",
            run(3),
            run(5)
        );
        assert!(
            run(2) - run(4) >= 14.0 + LEVEL_LABEL,
            "{} {}",
            run(2),
            run(4)
        );
        let label = |at: usize| {
            let route = &laid.routes[at];
            Rect {
                x: route.label_at.0,
                y: route.label_at.1,
                w: route.label_width,
                h: route.label_height,
            }
        };
        assert!(label(5).bottom() <= next, "{:?} over {next}", label(5));
        for at in 2..6 {
            assert!(
                !laid.nodes.iter().any(|&node| overlaps(label(at), node)),
                "{at}"
            );
            for other in 2..6 {
                assert!(
                    at == other || !overlaps(label(at), label(other)),
                    "{at} {other}"
                );
            }
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
            round: true,
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

    #[test]
    fn the_self_loops_of_a_box_lie_around_one_another_each_with_its_own_end() {
        use super::loops::{LOOP_RING, LOOP_STEP};
        // Words on the first, none on the second, tall ones on the third.
        let labels = [(30.0, 14.0), (0.0, 0.0), (80.0, 28.0), (24.0, 14.0)];
        for direction in [Direction::Down, Direction::Right] {
            let nodes = [node(160.0, 160.0)];
            let edges: Vec<Edge> = labels
                .iter()
                .map(|&(label_width, label_height)| Edge {
                    label_width,
                    label_height,
                    ..edge(0, 0)
                })
                .collect();
            let laid = layout(&nodes, &edges, direction);
            let rect = laid.nodes[0];
            // Along the free side, and out from it.
            let on_side = |p: (f32, f32)| match direction {
                Direction::Down => (p.1, p.0 - rect.right()),
                Direction::Right => (p.0, p.1 - rect.bottom()),
            };
            let mut ends: Vec<f32> = Vec::new();
            let mut reach = 0.0;
            for route in &laid.routes {
                let (first, last) = (route.points[0], *route.points.last().unwrap());
                // It leaves and arrives on the free side, between its
                // corners, at places no other loop has.
                for end in [first, last] {
                    let (along, out) = on_side(end);
                    assert!(out.abs() < 0.01, "{direction:?} {end:?}");
                    for other in &ends {
                        assert!(
                            (along - other).abs() >= LOOP_STEP - 0.01,
                            "{ends:?} {along}"
                        );
                    }
                    ends.push(along);
                }
                // And reaches further out than the one before.
                let far = route
                    .points
                    .iter()
                    .map(|&p| on_side(p).1)
                    .fold(0.0, f32::max);
                assert!(far >= reach + LOOP_RING - 0.01, "{far} {reach}");
                reach = far;
            }
            // No words on a line, nor on other words.
            let words: Vec<Rect> = laid
                .routes
                .iter()
                .filter(|r| r.label_width > 0.0)
                .map(|r| Rect {
                    x: r.label_at.0,
                    y: r.label_at.1,
                    w: r.label_width,
                    h: r.label_height,
                })
                .collect();
            assert_eq!(words.len(), 3);
            for (i, word) in words.iter().enumerate() {
                for route in &laid.routes {
                    for run in route.points.windows(2) {
                        assert!(
                            !route::cuts(*word, run[0], run[1]),
                            "{direction:?}: {run:?} through {word:?}"
                        );
                    }
                }
                for other in &words[i + 1..] {
                    assert!(!overlaps(*word, *other), "{word:?} on {other:?}");
                }
            }
        }
    }

    /// Two lines whose runs across one gap overlap run on two tracks; a
    /// hidden edge ranks its ends and takes none, and has no route. Two
    /// boxes each joined to both boxes below them: the two lines that cross
    /// over cannot help it, and hiding one of them takes its track away.
    #[test]
    fn a_hidden_edge_takes_no_track() {
        let nodes = [node(40.0, 20.0); 4];
        // The heights across the flow that some line runs at: its tracks.
        let tracks = |edges: &[Edge]| {
            let laid = layout(&nodes, edges, Direction::Down);
            let mut heights: Vec<i32> = laid
                .routes
                .iter()
                .flat_map(|route| route.points.windows(2))
                .filter(|pair| (pair[0].1 - pair[1].1).abs() < 0.01)
                .map(|pair| pair[0].1.round() as i32)
                .collect();
            heights.sort_unstable();
            heights.dedup();
            let routed = laid
                .routes
                .iter()
                .filter(|route| route.points.len() >= 2)
                .count();
            (routed, heights.len())
        };
        let (drawn_routes, drawn) = tracks(&[edge(0, 2), edge(0, 3), edge(1, 2), edge(1, 3)]);
        let (hidden_routes, hidden) = tracks(&[
            edge(0, 2),
            edge(0, 3),
            Edge {
                hidden: true,
                ..edge(1, 2)
            },
            edge(1, 3),
        ]);
        assert_eq!((drawn_routes, hidden_routes), (4, 3));
        assert_eq!(drawn, hidden + 1, "the hidden edge's track is gone");
    }

    /// Two lines that turn away from each other across one gap, a fork
    /// from one box to two below it, share one track: their runs lie
    /// apart, and the gap is as tall as for one.
    #[test]
    fn lines_whose_runs_lie_apart_share_a_track() {
        let nodes = [node(40.0, 20.0); 3];
        let gap = |edges: &[Edge]| {
            let laid = layout(&nodes, edges, Direction::Down);
            laid.nodes[1].y - laid.nodes[0].bottom()
        };
        let both = gap(&[edge(0, 1), edge(0, 2)]);
        let one = gap(&[
            edge(0, 1),
            Edge {
                hidden: true,
                ..edge(0, 2)
            },
        ]);
        assert!((both - one).abs() < 0.1, "{both} {one}");
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
