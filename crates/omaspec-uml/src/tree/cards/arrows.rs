//! The arrows between the cards of a map: where a scenario plays against
//! a machine, a state opens a diagram, a diagram includes another. One
//! arrow for a pair, naming each relation.
//!
//! A card between the two an arrow joins would hide it, so no arrow runs
//! under a card:
//!
//! - Between two cards next to each other it runs straight across.
//! - Between cards of one row it runs on a lane below that row, which the
//!   rows further down make room for ([`Plan::room`]). The arrows that
//!   arrive alike at one card share a lane, a head and their words, as
//!   the lines into one box share a trunk
//!   ([0022](../../../../../docs/adr/0022-lines-bundled.md)); the short
//!   ones take the lanes next to the cards.
//! - Any other, from one row to another or to a diagram on no box, is
//!   routed around the cards and the labels ([`route`]), and pays for
//!   crossing the curves the cards hang by and the lanes.
//!
//! Its words sit on an arrow where nothing else is, beside it where they
//! would hide an end; the arrows routed after it keep off them.

use std::collections::HashMap;

use super::{Placed, Relation, RelationKind, hang};
use crate::graph::Rect;
use crate::graph::route::{self, End, LabelRoom, Point, Surroundings};

/// An arrow with its words placed.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Arrow {
    /// The relations it stands for, sorted; the first sets its line.
    pub kinds: Vec<RelationKind>,
    /// Its lines by their corners. The first ends in the head; the others
    /// join it, from the further cards of a shared lane.
    pub lines: Vec<Vec<Point>>,
    pub words: String,
    /// The middle of its words.
    pub chip: Point,
}

/// The height of a pill of words.
pub(super) const CHIP_H: f32 = 22.0;

/// How wide a pill with `words` is.
pub(super) fn chip_width(words: &str) -> f32 {
    words.chars().count() as f32 * 6.2 + 28.0
}

/// The pill of `words` around `middle`.
pub(super) fn chip_rect(words: &str, middle: Point) -> Rect {
    let w = chip_width(words);
    Rect {
        x: middle.0 - w / 2.0,
        y: middle.1 - CHIP_H / 2.0,
        w,
        h: CHIP_H,
    }
}

/// How far the corners of an arrow are rounded.
pub(super) const ROUND: f32 = 10.0;
/// From the lowest card of a row to its first lane, and from lane to lane.
const FIRST: f32 = 18.0;
const LANE: f32 = 14.0;
/// How far from a corner of a card an arrow meets its lower side, and
/// from its middle, where the curve to the cards below leaves.
const INSET: f32 = 16.0;
/// Between two arrows straight across the same two cards.
const ACROSS: f32 = 18.0;
/// How much of an arrow shows at each end: its head, and where it leaves.
const END: f32 = 16.0;
/// Straight pieces the curve a card hangs by is weighed as.
const PIECES: usize = 12;

/// The arrows that arrive alike at one card from cards of its row.
#[derive(Debug)]
struct Net {
    to: usize,
    from: Vec<usize>,
    kinds: Vec<RelationKind>,
    depth: usize,
    /// The lane below the row it runs on, counted from the cards; `None`
    /// straight across, between two cards next to each other.
    lane: Option<usize>,
}

/// What is known of the arrows once the boxes have their places from left
/// to right: enough to make room for the lanes between the rows.
#[derive(Debug)]
pub(super) struct Plan {
    nets: Vec<Net>,
    /// The pairs that are routed around the cards, by their keys.
    rest: Vec<(String, String, Vec<RelationKind>)>,
    /// How many lanes run below each level, and from where to where.
    lanes: Vec<usize>,
    reach: Vec<Option<(f32, f32)>>,
}

impl Plan {
    /// The room the lanes below the level at `depth` take.
    pub(super) fn room(&self, depth: usize) -> f32 {
        match self.lanes.get(depth) {
            Some(&lanes) if lanes > 0 => FIRST + (lanes - 1) as f32 * LANE,
            _ => 0.0,
        }
    }

    /// Whether a lane below the level at `depth` runs under `rect`.
    pub(super) fn under(&self, depth: usize, rect: Rect) -> bool {
        self.reach
            .get(depth)
            .copied()
            .flatten()
            .is_some_and(|(lo, hi)| lo < rect.x + rect.w && rect.x < hi)
    }
}

fn middle_x(rect: Rect) -> f32 {
    rect.x + rect.w / 2.0
}

/// The arrows for `relations` between the boxes of the tree, `nodes`, each
/// at its place from left to right and at its `depth`.
pub(super) fn plan(nodes: &[Placed], depth: &[usize], relations: &[Relation]) -> Plan {
    // One arrow for a pair, naming each relation.
    let mut pairs: Vec<(&str, &str, Vec<RelationKind>)> = Vec::new();
    for r in relations {
        if r.from == r.to {
            continue;
        }
        match pairs
            .iter_mut()
            .find(|(f, t, _)| *f == r.from && *t == r.to)
        {
            Some((_, _, kinds)) if !kinds.contains(&r.kind) => kinds.push(r.kind),
            Some(_) => {}
            None => pairs.push((&r.from, &r.to, vec![r.kind])),
        }
    }
    let at = |key: &str| {
        nodes
            .iter()
            .position(|n| !n.block && n.key.as_deref() == Some(key))
    };
    let mut nets: Vec<Net> = Vec::new();
    let mut rest = Vec::new();
    for (from, to, mut kinds) in pairs {
        kinds.sort();
        match (at(from), at(to)) {
            (Some(a), Some(b)) if depth[a] == depth[b] => {
                match nets.iter_mut().find(|n| n.to == b && n.kinds == kinds) {
                    Some(net) => net.from.push(a),
                    None => nets.push(Net {
                        to: b,
                        from: vec![a],
                        kinds,
                        depth: depth[a],
                        lane: None,
                    }),
                }
            }
            _ => rest.push((from.to_owned(), to.to_owned(), kinds)),
        }
    }

    // Nothing of their row stands between two cards next to each other;
    // the room of the diagrams on no box counts as a part of its row.
    let neighbors = |a: usize, b: usize| {
        let (a_x, b_x) = (middle_x(nodes[a].rect), middle_x(nodes[b].rect));
        let (lo, hi) = (a_x.min(b_x), a_x.max(b_x));
        !nodes.iter().enumerate().any(|(i, n)| {
            let x = middle_x(n.rect);
            depth[i] == depth[a] && x > lo && x < hi
        })
    };
    // From the left of its leftmost card to the right of its rightmost.
    let span = |net: &Net| {
        net.from
            .iter()
            .chain([&net.to])
            .map(|&i| nodes[i].rect)
            .fold((f32::MAX, f32::MIN), |(lo, hi), r| {
                (lo.min(r.x), hi.max(r.x + r.w))
            })
    };
    // The shortest take the lanes next to the cards, so the fewest lines
    // cross on the way down to a lane.
    let mut order: Vec<usize> = (0..nets.len())
        .filter(|&i| !(nets[i].from.len() == 1 && neighbors(nets[i].from[0], nets[i].to)))
        .collect();
    let width = |i: usize| {
        let (lo, hi) = span(&nets[i]);
        hi - lo
    };
    order.sort_by(|&a, &b| width(a).total_cmp(&width(b)));
    let levels = depth.iter().copied().max().map_or(0, |d| d + 1);
    let mut taken: Vec<Vec<Vec<(f32, f32)>>> = vec![Vec::new(); levels];
    let mut reach: Vec<Option<(f32, f32)>> = vec![None; levels];
    let mut lane_of: Vec<Option<usize>> = vec![None; nets.len()];
    for i in order {
        let (lo, hi) = span(&nets[i]);
        let level = &mut taken[nets[i].depth];
        let lane = level
            .iter()
            .position(|lane| lane.iter().all(|&(a, b)| hi + LANE < a || b + LANE < lo))
            .unwrap_or_else(|| {
                level.push(Vec::new());
                level.len() - 1
            });
        level[lane].push((lo, hi));
        let all = &mut reach[nets[i].depth];
        *all = Some(all.map_or((lo, hi), |(a, b)| (a.min(lo), b.max(hi))));
        lane_of[i] = Some(lane);
    }
    for (net, lane) in nets.iter_mut().zip(lane_of) {
        net.lane = lane;
    }
    Plan {
        nets,
        rest,
        lanes: taken.iter().map(Vec::len).collect(),
        reach,
    }
}

/// The arrows of `plan` between the cards among `nodes`, now at their
/// places. `base` is how far down the cards of each level reach that its
/// lanes run below. The arrows stay right of and below `origin`, and off
/// `pills`, the other words on the map.
pub(super) fn lay(
    nodes: &[Placed],
    plan: &Plan,
    base: &[f32],
    origin: Point,
    pills: &[Rect],
) -> Vec<Arrow> {
    if plan.nets.is_empty() && plan.rest.is_empty() {
        return Vec::new();
    }
    let boxes: Vec<Rect> = nodes.iter().filter(|n| !n.block).map(|n| n.rect).collect();
    let mut around = Surroundings {
        obstacles: boxes.clone(),
        labels: pills.to_vec(),
        origin,
        ..Surroundings::default()
    };
    // The curves the cards hang by: crossing one costs, and an arrow ends
    // away from where one does.
    for node in nodes.iter().filter(|n| !n.block) {
        let Some(parent) = node.parent else {
            continue;
        };
        let curve = hang(nodes[parent].rect, node.rect);
        let mut last = curve[0];
        for step in 1..=PIECES {
            let next = cubic(curve, step as f32 / PIECES as f32);
            around.lines.push((last, next));
            last = next;
        }
        around.used.extend([curve[0], curve[3]]);
    }
    let mut solids = boxes;
    solids.extend(pills);

    // The arrows of a row, on their lanes and straight across.
    let drawn = lanes(nodes, plan, base);
    let segments = |lines: &[Vec<Point>]| -> Vec<(Point, Point)> {
        lines
            .iter()
            .flat_map(|line| line.windows(2).map(|w| (w[0], w[1])))
            .collect()
    };
    let mut out = Vec::with_capacity(plan.nets.len() + plan.rest.len());
    for (index, net) in plan.nets.iter().enumerate() {
        // Its words keep off every other line, its own that join it too.
        let mut others = around.lines.clone();
        for (other, lines) in drawn.iter().enumerate() {
            let own = usize::from(other == index);
            others.extend(segments(&lines[own..]));
        }
        let words = words(&net.kinds);
        let chip = spot(&drawn[index][0], &words, &solids, &others, origin);
        solids.push(chip_rect(&words, chip));
        out.push(Arrow {
            kinds: net.kinds.clone(),
            lines: drawn[index].clone(),
            words,
            chip,
        });
    }
    for arrow in &out {
        around.lines.extend(segments(&arrow.lines));
        around.used.extend(
            arrow
                .lines
                .iter()
                .flat_map(|l| [l.first(), l.last()])
                .flatten(),
        );
        // The words are painted over the lines: an arrow under them would
        // show in two pieces, so those routed now go around them.
        around.obstacles.push(chip_rect(&arrow.words, arrow.chip));
    }

    // The others around the cards, the shortest first so it gets the
    // straightest line.
    let at = |key: &str| {
        nodes
            .iter()
            .find(|n| !n.block && n.key.as_deref() == Some(key))
    };
    let mut rest: Vec<(Rect, Rect, &[RelationKind])> = plan
        .rest
        .iter()
        .filter_map(|(from, to, kinds)| Some((at(from)?.rect, at(to)?.rect, kinds.as_slice())))
        .collect();
    let far = |a: Rect, b: Rect| {
        (middle_x(a) - middle_x(b)).abs() + ((a.y + a.h / 2.0) - (b.y + b.h / 2.0)).abs()
    };
    rest.sort_by(|x, y| far(x.0, x.1).total_cmp(&far(y.0, y.1)));
    for (a, b, kinds) in rest {
        let end = |rect: Rect| End { rect, flat: true };
        let points = route::route(end(a), end(b), &around)
            .filter(|points| points.len() > 1)
            .unwrap_or_else(|| straight(a, b));
        let words = words(kinds);
        let chip = spot(&points, &words, &solids, &around.lines, origin);
        let pill = chip_rect(&words, chip);
        solids.push(pill);
        around.obstacles.push(pill);
        around.lines.extend(points.windows(2).map(|w| (w[0], w[1])));
        around.used.extend([points[0], points[points.len() - 1]]);
        out.push(Arrow {
            kinds: kinds.to_vec(),
            lines: vec![points],
            words,
            chip,
        });
    }
    out
}

/// The words of an arrow that stands for `kinds`.
fn words(kinds: &[RelationKind]) -> String {
    kinds
        .iter()
        .map(|k| k.word())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Where a net meets the lower side of one of its cards.
struct Meet {
    card: usize,
    net: usize,
    /// Its lane leads left from here.
    left: bool,
    lane: usize,
}

/// The lines of each net of `plan`: the first from its furthest card along
/// its lane into the card it arrives at, the others joining that lane.
fn lanes(nodes: &[Placed], plan: &Plan, base: &[f32]) -> Vec<Vec<Vec<Point>>> {
    let rect = |i: usize| nodes[i].rect;
    // Where each net meets each of its cards: the ones that lead left on
    // the left half, the nearest lane next to the corner, so no two lines
    // of one card cross; and so on the right half.
    let mut meets: Vec<Meet> = Vec::new();
    for (index, net) in plan.nets.iter().enumerate() {
        let Some(lane) = net.lane else {
            continue;
        };
        let to = middle_x(rect(net.to));
        let from = net.from.iter().map(|&i| middle_x(rect(i))).sum::<f32>() / net.from.len() as f32;
        meets.push(Meet {
            card: net.to,
            net: index,
            left: from < to,
            lane,
        });
        for &card in &net.from {
            meets.push(Meet {
                card,
                net: index,
                left: to < middle_x(rect(card)),
                lane,
            });
        }
    }
    meets.sort_by_key(|m| (m.card, m.left, m.lane, m.net));
    let mut port: HashMap<(usize, usize), f32> = HashMap::new();
    for side in meets.chunk_by(|a, b| a.card == b.card && a.left == b.left) {
        let r = rect(side[0].card);
        let middle = middle_x(r);
        let (lo, hi) = if side[0].left {
            (r.x + INSET, middle - INSET)
        } else {
            (middle + INSET, r.x + r.w - INSET)
        };
        let (lo, hi) = (lo.min(hi), hi.max(lo));
        for (i, meet) in side.iter().enumerate() {
            let step = (hi - lo) * (i + 1) as f32 / (side.len() + 1) as f32;
            let x = if meet.left { lo + step } else { hi - step };
            port.insert((meet.net, meet.card), x);
        }
    }

    // Arrows straight across the same two cards each get a height of
    // their own.
    let pair = |net: &Net| (net.to.min(net.from[0]), net.to.max(net.from[0]));
    let across = |net: &Net| {
        plan.nets
            .iter()
            .filter(|n| n.lane.is_none() && pair(n) == pair(net))
            .count()
    };
    let mut seen: Vec<(usize, usize)> = Vec::new();

    plan.nets
        .iter()
        .enumerate()
        .map(|(index, net)| {
            let Some(lane) = net.lane else {
                let (a, b) = (rect(net.from[0]), rect(net.to));
                let (top, bottom) = (a.y.max(b.y), (a.y + a.h).min(b.y + b.h));
                let nth = seen.iter().filter(|&&p| p == pair(net)).count();
                seen.push(pair(net));
                let y =
                    (top + bottom) / 2.0 + (nth as f32 - (across(net) - 1) as f32 / 2.0) * ACROSS;
                return vec![if middle_x(b) < middle_x(a) {
                    vec![(a.x, y), (b.x + b.w, y)]
                } else {
                    vec![(a.x + a.w, y), (b.x, y)]
                }];
            };
            let y = base[net.depth] + FIRST + lane as f32 * LANE;
            let foot = |card: usize| (port[&(index, card)], rect(card).y + rect(card).h);
            let head = foot(net.to);
            // The furthest card first: its line runs all the way.
            let mut from: Vec<Point> = net.from.iter().map(|&card| foot(card)).collect();
            from.sort_by(|a, b| (b.0 - head.0).abs().total_cmp(&(a.0 - head.0).abs()));
            let left_of = |p: Point| p.0 <= head.0;
            let mut lines = vec![vec![from[0], (from[0].0, y), (head.0, y), head]];
            let mut other_side = false;
            for &start in &from[1..] {
                if left_of(start) != left_of(from[0]) && !other_side {
                    // The furthest on the other side runs up to the first
                    // line and turns into it.
                    other_side = true;
                    let up = ROUND.min(y - head.1);
                    lines.push(vec![start, (start.0, y), (head.0, y), (head.0, y - up)]);
                } else {
                    // The nearer ones turn onto the lane.
                    let reach = ROUND.min((head.0 - start.0).abs());
                    let toward = if left_of(start) { reach } else { -reach };
                    lines.push(vec![start, (start.0, y), (start.0 + toward, y)]);
                }
            }
            lines
        })
        .collect()
}

/// Where the words of the arrow along `points` go: on it, since a pill
/// has the page's color behind its words; beside it where they would hide
/// an end, as on the short arrow between two cards next to each other.
fn spot(
    points: &[Point],
    words: &str,
    solids: &[Rect],
    lines: &[(Point, Point)],
    origin: Point,
) -> Point {
    let size = (chip_width(words), CHIP_H);
    let at = |on_line: bool| {
        let room = LabelRoom {
            solids,
            frames: &[],
            lines,
            origin,
            on_line,
        };
        route::label_spot(points, size, &room, None)
    };
    at(true)
        .filter(|&at| !hides_an_end(chip_rect(words, at), points))
        .or_else(|| at(false))
        .unwrap_or_else(|| middle(points))
}

/// Whether `pill` covers the first or the last [`END`] of `points`.
fn hides_an_end(pill: Rect, points: &[Point]) -> bool {
    let [first, second, ..] = points[..] else {
        return false;
    };
    let [.., before, last] = points[..] else {
        return false;
    };
    // The point up to `END` from `a` toward `b`.
    let inward = |a: Point, b: Point| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let run = dx.hypot(dy);
        let t = if run > 0.0 { END.min(run) / run } else { 0.0 };
        (a.0 + dx * t, a.1 + dy * t)
    };
    [(first, inward(first, second)), (last, inward(last, before))]
        .into_iter()
        .any(|(a, b)| route::cuts(pill, a, b))
}

/// The room `arrows` take with their words, or `None` without any.
pub(super) fn extent(arrows: &[Arrow]) -> Option<Rect> {
    let mut sides: Option<(f32, f32, f32, f32)> = None;
    let mut add = |x: f32, y: f32| {
        let (l, t, r, b) = sides.unwrap_or((x, y, x, y));
        sides = Some((l.min(x), t.min(y), r.max(x), b.max(y)));
    };
    for arrow in arrows {
        for &(x, y) in arrow.lines.iter().flatten() {
            add(x, y);
        }
        let pill = chip_rect(&arrow.words, arrow.chip);
        add(pill.x, pill.y);
        add(pill.x + pill.w, pill.y + pill.h);
    }
    sides.map(|(l, t, r, b)| Rect {
        x: l,
        y: t,
        w: r - l,
        h: b - t,
    })
}

/// The point at `t` of the cubic `c`.
fn cubic(c: [Point; 4], t: f32) -> Point {
    let u = 1.0 - t;
    let (k0, k1, k2, k3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (
        k0 * c[0].0 + k1 * c[1].0 + k2 * c[2].0 + k3 * c[3].0,
        k0 * c[0].1 + k1 * c[1].1 + k2 * c[2].1 + k3 * c[3].1,
    )
}

/// The middle of a line along its length.
fn middle(points: &[Point]) -> Point {
    let length = |a: Point, b: Point| (b.0 - a.0).hypot(b.1 - a.1);
    let total: f32 = points.windows(2).map(|w| length(w[0], w[1])).sum();
    let mut left = total / 2.0;
    for w in points.windows(2) {
        let run = length(w[0], w[1]);
        if run >= left && run > 0.0 {
            let t = left / run;
            return (
                w[0].0 + (w[1].0 - w[0].0) * t,
                w[0].1 + (w[1].1 - w[0].1) * t,
            );
        }
        left -= run;
    }
    points.first().copied().unwrap_or_default()
}

/// From the border of `a` straight to that of `b`, between their middles:
/// what is left when no way leads around the cards.
fn straight(a: Rect, b: Rect) -> Vec<Point> {
    let center = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let (from, to) = (center(a), center(b));
    // Where the line from the middle of `r` toward `other` leaves it.
    let leave = |r: Rect, own: Point, other: Point| {
        let (dx, dy) = (other.0 - own.0, other.1 - own.1);
        let tx = if dx.abs() > f32::EPSILON {
            r.w / 2.0 / dx.abs()
        } else {
            f32::INFINITY
        };
        let ty = if dy.abs() > f32::EPSILON {
            r.h / 2.0 / dy.abs()
        } else {
            f32::INFINITY
        };
        let t = tx.min(ty).min(1.0);
        (own.0 + dx * t, own.1 + dy * t)
    };
    vec![leave(a, from, to), leave(b, to, from)]
}
