//! Fewer crossings, as Graphviz orders its ranks: the crossings between
//! each two neighboring ranks are counted after every sweep, an earlier
//! order that crossed less wins over the last one, and then two
//! neighbors of a rank swap wherever that crosses less. Both keep the
//! last order on a tie, so a diagram whose lines do not cross is laid out
//! as before.

use super::Slot;

/// Rounds of swapping, at most.
const TRANSPOSE_ROUNDS: usize = 8;

/// The crossings between every two neighboring ranks of `rows`.
pub(super) fn crossings(rows: &[Vec<usize>], slots: &[Slot]) -> usize {
    let place = places(rows, slots.len());
    rows.windows(2)
        .map(|pair| between(&pair[0], &pair[1], slots, &place))
        .sum()
}

/// Where each slot stands in its rank.
fn places(rows: &[Vec<usize>], n: usize) -> Vec<usize> {
    let mut place = vec![usize::MAX; n];
    for row in rows {
        for (index, &slot) in row.iter().enumerate() {
            place[slot] = index;
        }
    }
    place
}

/// The crossings between the lines from `upper` to `lower`, its next rank.
fn between(upper: &[usize], lower: &[usize], slots: &[Slot], place: &[usize]) -> usize {
    let mut ends: Vec<(usize, usize)> = Vec::new();
    for &u in upper {
        for &v in &slots[u].succs {
            if lower.contains(&v) {
                ends.push((place[u], place[v]));
            }
        }
    }
    let mut count = 0;
    for (i, &(a1, b1)) in ends.iter().enumerate() {
        for &(a2, b2) in &ends[i + 1..] {
            if (a1 < a2 && b1 > b2) || (a1 > a2 && b1 < b2) {
                count += 1;
            }
        }
    }
    count
}

/// Where each slot stands among the ranks; `usize::MAX` for one in none.
fn ranks(rows: &[Vec<usize>], n: usize) -> Vec<usize> {
    let mut rank = vec![usize::MAX; n];
    for (r, row) in rows.iter().enumerate() {
        for &slot in row {
            rank[slot] = r;
        }
    }
    rank
}

/// The crossings between the lines of `a` and those of `b`, two neighbors
/// of rank `r`, to the ranks on either side: with `a` first, and with `b`
/// first. Swapping the two changes no other crossing.
fn pair(
    a: usize,
    b: usize,
    r: usize,
    slots: &[Slot],
    place: &[usize],
    rank: &[usize],
) -> (usize, usize) {
    let (mut now, mut swapped) = (0, 0);
    let mut count = |ends_a: &[usize], ends_b: &[usize], side: usize| {
        for &x in ends_a.iter().filter(|&&x| rank[x] == side) {
            for &y in ends_b.iter().filter(|&&y| rank[y] == side) {
                match place[x].cmp(&place[y]) {
                    std::cmp::Ordering::Greater => now += 1,
                    std::cmp::Ordering::Less => swapped += 1,
                    std::cmp::Ordering::Equal => {}
                }
            }
        }
    };
    if r > 0 {
        count(&slots[a].preds, &slots[b].preds, r - 1);
    }
    count(&slots[a].succs, &slots[b].succs, r + 1);
    (now, swapped)
}

/// Swaps two neighbors of a rank wherever that crosses less, while
/// `together(a, b)` says the two may trade places (the same frames). Only
/// the crossings between the lines of the two change, so only those are
/// counted.
pub(super) fn transpose(
    rows: &mut [Vec<usize>],
    slots: &[Slot],
    together: impl Fn(usize, usize) -> bool,
) {
    let mut place = places(rows, slots.len());
    let rank = ranks(rows, slots.len());
    for _ in 0..TRANSPOSE_ROUNDS {
        let mut better = false;
        for (r, row) in rows.iter_mut().enumerate() {
            for i in 0..row.len().saturating_sub(1) {
                let (a, b) = (row[i], row[i + 1]);
                if !together(a, b) {
                    continue;
                }
                let (now, swapped) = pair(a, b, r, slots, &place, &rank);
                if swapped < now {
                    row.swap(i, i + 1);
                    (place[a], place[b]) = (i + 1, i);
                    better = true;
                }
            }
        }
        if !better {
            break;
        }
    }
}

/// Keeps the order with the fewest crossings: `rows` as they are, unless
/// `best` crossed less.
pub(super) fn keep_best(rows: &mut [Vec<usize>], slots: &[Slot], best: (usize, Vec<Vec<usize>>)) {
    if best.0 < crossings(rows, slots) {
        rows.clone_from_slice(&best.1);
    }
}

/// Rounds of laying the long lines again, at most.
const REROUTE_ROUNDS: usize = 4;
/// How much laying lines again may weigh in all: for each line, the places
/// of each two ranks it passes, times each other (`reroute_line` weighs
/// every pair of places). A dense diagram has hundreds of long lines
/// through wide ranks; past this the lines stay as the sweeps laid them,
/// and every diagram of ordinary size is laid as before.
const REROUTE_BUDGET: usize = 6_000_000;

/// Lays a line that passes several ranks again as a whole where that
/// crosses less: its points, one in each rank it passes, take the places
/// that cross the fewest other lines, each among the neighbors
/// `together` lets it trade places with (the same frames).
///
/// Swapping two neighbors of one rank cannot do this: a line that came
/// down on the wrong side of a box crosses more with every point that
/// moves alone, and only all of them on the other side cross less
/// (Reports to its replica, around the queue, in `examples/platform.mmd`).
/// The other slots stay as they are, so only the crossings of the line
/// itself change, and the places come from one pass down its ranks.
pub(super) fn reroute(
    rows: &mut [Vec<usize>],
    slots: &[Slot],
    together: impl Fn(usize, usize) -> bool,
) {
    let mut rank = vec![usize::MAX; slots.len()];
    for (r, row) in rows.iter().enumerate() {
        for &slot in row {
            rank[slot] = r;
        }
    }
    let point = |x: usize| {
        let s = &slots[x];
        s.real.is_none() && s.anchor.is_none() && s.preds.len() == 1 && s.succs.len() == 1
    };
    // Each line's points, from the first down.
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for x in 0..slots.len() {
        if rank[x] == usize::MAX || !point(x) || point(slots[x].preds[0]) {
            continue;
        }
        let mut line = vec![x];
        while let Some(&next) = slots[*line.last().expect("it starts")]
            .succs
            .first()
            .filter(|&&next| point(next) && rank[next] != usize::MAX)
        {
            line.push(next);
        }
        let down = line.windows(2).all(|w| rank[w[1]] == rank[w[0]] + 1);
        let (from, to) = (slots[x].preds[0], slots[line[line.len() - 1]].succs[0]);
        if down
            && rank[x] > 0
            && rank[from] == rank[x] - 1
            && rank[to] == rank[line[line.len() - 1]] + 1
        {
            lines.push(line);
        }
    }
    // Where each slot of one rank stands, for one rank at a time; every
    // other slot `usize::MAX`.
    let mut scratch = vec![usize::MAX; slots.len()];
    let mut spent = 0usize;
    for _ in 0..REROUTE_ROUNDS {
        let mut better = false;
        for line in &lines {
            let first = rank[line[0]];
            let weight: usize = (first - 1..first + line.len())
                .map(|r| (rows[r].len() + 1) * (rows[r + 1].len() + 1))
                .sum();
            spent += weight;
            if spent > REROUTE_BUDGET {
                return;
            }
            better |= reroute_line(rows, slots, line, first, &together, &mut scratch);
        }
        if !better {
            break;
        }
    }
}

/// Lays one line again ([`reroute`]); whether it crosses less now.
fn reroute_line(
    rows: &mut [Vec<usize>],
    slots: &[Slot],
    line: &[usize],
    first: usize,
    together: &impl Fn(usize, usize) -> bool,
    scratch: &mut [usize],
) -> bool {
    // The rows without the line's points, and where each point stood: a
    // point at `k` stands before the slot at `k` there.
    let others: Vec<Vec<usize>> = line
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            rows[first + i]
                .iter()
                .copied()
                .filter(|&s| s != x)
                .collect()
        })
        .collect();
    let stood: Vec<usize> = line
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            rows[first + i]
                .iter()
                .position(|&s| s == x)
                .expect("a point is in its rank")
        })
        .collect();
    // The places a point may take: across neighbors of the same frames.
    let room: Vec<(usize, usize)> = line
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let (row, k) = (&others[i], stood[i]);
            let left = row[..k]
                .iter()
                .rev()
                .take_while(|&&s| together(x, s))
                .count();
            let right = row[k..].iter().take_while(|&&s| together(x, s)).count();
            (k - left, k + right)
        })
        .collect();
    // The lines between two ranks that a stretch of this line crosses,
    // which starts left of `a` slots above and ends left of `b` below.
    let mut crossed =
        |upper: &[usize], lower: &[usize], skip: (usize, usize)| -> Vec<(usize, usize)> {
            for (index, &slot) in lower.iter().enumerate() {
                scratch[slot] = index;
            }
            let mut ends = Vec::new();
            for (index, &u) in upper.iter().enumerate() {
                for &v in &slots[u].succs {
                    if (u, v) != skip && scratch[v] != usize::MAX {
                        ends.push((index, scratch[v]));
                    }
                }
            }
            for &slot in lower {
                scratch[slot] = usize::MAX;
            }
            ends
        };
    let cost = |ends: &[(usize, usize)], a: usize, b: usize| -> usize {
        ends.iter().filter(|&&(u, v)| (u < a) != (v < b)).count()
    };
    let last = line.len() - 1;
    let (from, to) = (slots[line[0]].preds[0], slots[line[last]].succs[0]);
    // Into the first point, between the points, out of the last. The
    // line's own ends stand where they are; lines of the same end do not
    // cross it there.
    let above = &rows[first - 1];
    let from_at = above
        .iter()
        .position(|&s| s == from)
        .expect("it has an end");
    let into: Vec<(usize, usize)> = crossed(above, &others[0], (from, line[0]))
        .into_iter()
        .filter(|&(u, _)| u != from_at)
        .map(|(u, v)| (u + usize::from(u > from_at), v))
        .collect();
    let below = &rows[first + last + 1];
    let to_at = below.iter().position(|&s| s == to).expect("it has an end");
    let out: Vec<(usize, usize)> = crossed(&others[last], below, (line[last], to))
        .into_iter()
        .filter(|&(_, v)| v != to_at)
        .map(|(u, v)| (u, v + usize::from(v > to_at)))
        .collect();
    let (from_edge, to_edge) = (from_at + 1, to_at + 1);
    // Between two of its points, for every pair of places at once.
    let between: Vec<Vec<Vec<usize>>> = (0..last)
        .map(|i| {
            let ends = crossed(&others[i], &others[i + 1], (line[i], line[i + 1]));
            costs(&ends, others[i].len(), others[i + 1].len())
        })
        .collect();
    // The fewest crossings down to each place of each rank, and the place
    // above it they came through; a tie keeps the place nearest the old.
    let near = |k: usize, old: usize| k.abs_diff(old);
    let mut best: Vec<Vec<(usize, usize)>> = Vec::with_capacity(line.len());
    best.push(
        (room[0].0..=room[0].1)
            .map(|k| (cost(&into, from_edge, k), usize::MAX))
            .collect(),
    );
    for i in 1..line.len() {
        let row = (room[i].0..=room[i].1)
            .map(|k| {
                (room[i - 1].0..=room[i - 1].1)
                    .map(|j| {
                        let so_far = best[i - 1][j - room[i - 1].0].0;
                        (so_far + between[i - 1][j][k], j)
                    })
                    .min_by_key(|&(total, j)| (total, near(j, stood[i - 1])))
                    .expect("a rank has a place")
            })
            .collect();
        best.push(row);
    }
    let was = cost(&into, from_edge, stood[0])
        + (0..last)
            .map(|i| between[i][stood[i]][stood[i + 1]])
            .sum::<usize>()
        + cost(&out, stood[last], to_edge);
    let (total, mut at) = (room[last].0..=room[last].1)
        .map(|k| (best[last][k - room[last].0].0 + cost(&out, k, to_edge), k))
        .min_by_key(|&(total, k)| (total, near(k, stood[last])))
        .expect("a rank has a place");
    if total >= was {
        return false;
    }
    for i in (0..line.len()).rev() {
        let mut row = others[i].clone();
        row.insert(at, line[i]);
        rows[first + i] = row;
        at = best[i][at - room[i].0].1;
    }
    true
}

/// For a stretch that starts left of `a` slots of the rank above and
/// ends left of `b` slots of the rank below, how many of the lines `ends`
/// it crosses: `[a][b]`, for every `a` up to `above` and `b` up to `below`.
fn costs(ends: &[(usize, usize)], above: usize, below: usize) -> Vec<Vec<usize>> {
    // The lines that start left of `a` and end left of `b`.
    let mut both = vec![vec![0usize; below + 1]; above + 1];
    for &(u, v) in ends {
        both[u + 1][v + 1] += 1;
    }
    for a in 1..=above {
        for b in 1..=below {
            both[a][b] += both[a - 1][b] + both[a][b - 1];
            both[a][b] -= both[a - 1][b - 1];
        }
    }
    // A line crosses that starts left and ends right, or the other way.
    (0..=above)
        .map(|a| {
            (0..=below)
                .map(|b| both[a][below] + both[above][b] - 2 * both[a][b])
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::{Direction, Edge, Node, Slot, layout, link};
    use super::{crossings, reroute, transpose};

    /// A line through two ranks that came down on the wrong side of a
    /// box: each of its points alone crosses as much on the other side,
    /// so no swap of two neighbors helps; laid again as a whole it crosses
    /// nothing. Where it may not pass the box (another frame), it stays.
    #[test]
    fn a_long_line_changes_sides_as_a_whole() {
        // Mailer and Reports over the queue; Reports' line to the replica
        // under it passes the queue's rank.
        let [mailer, reports, to_queue, on, on_too, past, queue, replica] =
            [0, 1, 2, 3, 4, 5, 6, 7];
        let mut slots: Vec<Slot> = (0..8)
            .map(|x| {
                let real = [mailer, reports, queue, replica].contains(&x);
                Slot::point(real.then_some(x), None)
            })
            .collect();
        for (from, to) in [
            (mailer, to_queue),
            (to_queue, queue),
            (reports, on_too),
            (on_too, queue),
            (reports, on),
            (on, past),
            (past, replica),
        ] {
            link(&mut slots, from, to);
        }
        let stuck = vec![
            vec![mailer, reports],
            vec![to_queue, on, on_too],
            vec![past, queue],
            vec![replica],
        ];
        let mut rows = stuck.clone();
        assert_eq!(crossings(&rows, &slots), 1);
        transpose(&mut rows, &slots, |_, _| true);
        assert_eq!(rows, stuck, "no two neighbors swap for less");
        reroute(&mut rows, &slots, |a, b| a != queue && b != queue);
        assert_eq!(rows, stuck, "the line may not pass the queue");
        reroute(&mut rows, &slots, |_, _| true);
        assert_eq!(crossings(&rows, &slots), 0);
        assert_eq!(rows[1], [to_queue, on_too, on]);
        assert_eq!(rows[2], [queue, past]);
    }

    struct Rng(u64);
    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n.max(1) as u64) as usize
        }
    }

    fn cross(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> bool {
        let side = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| {
            (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
        };
        let (d1, d2) = (side(c, d, a), side(c, d, b));
        let (d3, d4) = (side(a, b, c), side(a, b, d));
        ((d1 > 0.5 && d2 < -0.5) || (d1 < -0.5 && d2 > 0.5))
            && ((d3 > 0.5 && d4 < -0.5) || (d3 < -0.5 && d4 > 0.5))
    }

    /// 300 graphs of 6 to 19 boxes and up to three lines a box, drawn
    /// down: with only the barycenter sweeps their lines crossed 3479
    /// times; keeping the order that crosses least and swapping neighbors
    /// where that crosses less, 3058; with the long lines laid again as a
    /// whole, 2203.
    #[test]
    fn random_graphs_cross_less() {
        let mut rng = Rng(0x1234_5678_9abc_def1);
        let mut total = 0;
        for _ in 0..300 {
            let n = 6 + rng.below(14);
            let nodes: Vec<Node> = (0..n)
                .map(|_| Node {
                    width: 40.0 + rng.below(60) as f32,
                    height: 28.0,
                    before: 0.0,
                    loop_extra: 0.0,
                    flat: true,
                })
                .collect();
            let m = n + rng.below(n * 2);
            let edges: Vec<Edge> = (0..m)
                .map(|_| {
                    let a = rng.below(n);
                    let b = rng.below(n);
                    Edge {
                        from: a,
                        to: b,
                        label_width: 0.0,
                        label_height: 0.0,
                        hidden: false,
                        apart: false,
                        marked: false,
                        arrival: 0,
                        label_before: None,
                        level: None,
                    }
                })
                .filter(|e| e.from != e.to)
                .collect();
            let laid = layout(&nodes, &edges, Direction::Down);
            let mut count = 0;
            for i in 0..laid.routes.len() {
                for j in i + 1..laid.routes.len() {
                    for a in laid.routes[i].points.windows(2) {
                        for b in laid.routes[j].points.windows(2) {
                            if cross(a[0], a[1], b[0], b[1]) {
                                count += 1;
                            }
                        }
                    }
                }
            }
            total += count;
        }
        assert!(total <= 2203, "the lines of 300 graphs cross {total} times");
    }
}
