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

/// The crossings rank `r` takes part in.
fn around(rows: &[Vec<usize>], r: usize, slots: &[Slot], place: &[usize]) -> usize {
    let above = if r > 0 {
        between(&rows[r - 1], &rows[r], slots, place)
    } else {
        0
    };
    let below = match rows.get(r + 1) {
        Some(next) => between(&rows[r], next, slots, place),
        None => 0,
    };
    above + below
}

/// Swaps two neighbors of a rank wherever that crosses less, while
/// `together(a, b)` says the two may trade places (the same frames).
pub(super) fn transpose(
    rows: &mut [Vec<usize>],
    slots: &[Slot],
    together: impl Fn(usize, usize) -> bool,
) {
    let mut place = places(rows, slots.len());
    for _ in 0..TRANSPOSE_ROUNDS {
        let mut better = false;
        for r in 0..rows.len() {
            for i in 0..rows[r].len().saturating_sub(1) {
                let (a, b) = (rows[r][i], rows[r][i + 1]);
                if !together(a, b) {
                    continue;
                }
                let was = around(rows, r, slots, &place);
                if was == 0 {
                    continue;
                }
                rows[r].swap(i, i + 1);
                (place[a], place[b]) = (i + 1, i);
                if around(rows, r, slots, &place) < was {
                    better = true;
                } else {
                    rows[r].swap(i, i + 1);
                    (place[a], place[b]) = (i, i + 1);
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

#[cfg(test)]
mod tests {
    use super::super::{Direction, Edge, Node, layout};

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
    /// where that crosses less, 3058.
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
        assert!(total <= 3058, "the lines of 300 graphs cross {total} times");
    }
}
