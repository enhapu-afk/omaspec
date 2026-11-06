//! Boxes held in place ([0021](../../../../docs/adr/0021-boxes-held-in-place.md)):
//! a box held beside a neighbor takes the neighbor's rank, the two ranked
//! as one as `dot` ranks `rank=same` ([`groups`]), and stands right after
//! or before it across the flow once the ranks are ordered ([`order`]).
//! Placement is as before. A hold that cannot be kept is said
//! ([`Unheld`]), and the box stands where the layout puts it.

use std::collections::HashMap;

use super::Edge;
use crate::hand;

/// A box held beside another: `node` stands in the rank of `beside`, right
/// after it across the flow (right of it when the flow runs down, below it
/// when it runs across) or right before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hold {
    pub node: usize,
    pub beside: usize,
    pub after: bool,
}

/// A box held beside another in one container, as a drawer gives it to
/// the layout ([`super::open::Nested::holds`]): the entry's zero-based
/// line, and the two boxes by their index among the container's members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    pub line: usize,
    pub index: usize,
    pub beside: usize,
    pub after: bool,
}

/// The boxes held in each container, by the path of the container; and
/// the entries that do not hold, by line, with why.
pub type Sorted = (HashMap<Vec<usize>, Vec<Held>>, Vec<(usize, String)>);

/// The boxes held (`held`, each with its entry's line) by the container
/// that holds both: by the path of the container (the indices of its
/// members from the top), in the order of the entries. `places` gives the
/// container and the index of the box each name names. Also what does
/// not hold, by line: a name no box has, two boxes in different
/// containers.
pub fn sort(held: &[(usize, hand::Hold)], places: &HashMap<String, (Vec<usize>, usize)>) -> Sorted {
    let mut sorted: HashMap<Vec<usize>, Vec<Held>> = HashMap::new();
    let mut misses = Vec::new();
    for (line, hold) in held {
        let (Some(at), Some(by)) = (places.get(&hold.name), places.get(&hold.beside)) else {
            let missing = if places.contains_key(&hold.name) {
                &hold.beside
            } else {
                &hold.name
            };
            misses.push((*line, hand::no_box(hold, missing)));
            continue;
        };
        if at.0 != by.0 {
            misses.push((*line, hand::unheld(hold, Unheld::Apart)));
            continue;
        }
        sorted.entry(at.0.clone()).or_default().push(Held {
            line: *line,
            index: at.1,
            beside: by.1,
            after: hold.after,
        });
    }
    (sorted, misses)
}

/// Why a hold is not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unheld {
    /// It names no box of the layout.
    Missing,
    /// The box is held beside itself.
    Itself,
    /// The two lie in different containers.
    Apart,
    /// Boxes it would put in one rank are joined by a line.
    Joined,
    /// The holds go round: the two are held in one rank already.
    Round,
    /// One of the two is a container laid out with what is around it.
    Frame,
}

/// The holds that can be kept, in their order, and why each other one
/// cannot: by its index in `holds`. `n` boxes, `within` the frame around
/// each (empty when there are none), `edges` every line between them.
pub(super) fn check(
    n: usize,
    within: &[Option<usize>],
    edges: &[Edge],
    holds: &[Hold],
) -> (Vec<Hold>, Vec<(usize, Unheld)>) {
    let mut sets = Sets::new(n);
    let mut kept = Vec::new();
    let mut unheld = Vec::new();
    let frame = |x: usize| within.get(x).copied().flatten();
    for (index, hold) in holds.iter().enumerate() {
        let (a, b) = (hold.node, hold.beside);
        let why = if a >= n || b >= n {
            Some(Unheld::Missing)
        } else if a == b {
            Some(Unheld::Itself)
        } else if frame(a) != frame(b) {
            Some(Unheld::Apart)
        } else if sets.find(a) == sets.find(b) {
            Some(Unheld::Round)
        } else {
            let (ga, gb) = (sets.find(a), sets.find(b));
            let joined = edges.iter().any(|edge| {
                if edge.from >= n || edge.to >= n {
                    return false;
                }
                let (u, v) = (sets.find(edge.from), sets.find(edge.to));
                (u == ga && v == gb) || (u == gb && v == ga)
            });
            joined.then_some(Unheld::Joined)
        };
        match why {
            Some(why) => unheld.push((index, why)),
            None => {
                sets.join(a, b);
                kept.push(*hold);
            }
        }
    }
    (kept, unheld)
}

/// For every box, the box it is ranked as: one of each group of boxes the
/// holds put in one rank, itself for a box held by none.
pub(super) fn groups(n: usize, holds: &[Hold]) -> Vec<usize> {
    let mut sets = Sets::new(n);
    for hold in holds {
        sets.join(hold.node, hold.beside);
    }
    (0..n).map(|x| sets.find(x)).collect()
}

/// `edges` with each end that is a box replaced by the box it is ranked as
/// (`group`); an end past the boxes, a frame, stays.
pub(super) fn ranked(edges: &[Edge], group: &[usize]) -> Vec<Edge> {
    let of = |end: usize| group.get(end).copied().unwrap_or(end);
    edges
        .iter()
        .map(|edge| Edge {
            from: of(edge.from),
            to: of(edge.to),
            ..*edge
        })
        .collect()
}

/// Moves each held box of `rows` (ranks of slots, a box's slot being its
/// index) right after or before its neighbor, in the order of `holds`:
/// boxes held after one neighbor follow it in that order.
pub(super) fn order(rows: &mut [Vec<usize>], holds: &[Hold]) {
    // The last box placed after each neighbor so far.
    let mut last: std::collections::HashMap<usize, usize> = Default::default();
    for hold in holds {
        let Some(row) = rows.iter_mut().find(|row| row.contains(&hold.beside)) else {
            continue;
        };
        let Some(from) = row.iter().position(|&slot| slot == hold.node) else {
            continue;
        };
        row.remove(from);
        let anchor = if hold.after {
            last.get(&hold.beside)
                .copied()
                .filter(|slot| row.contains(slot))
                .unwrap_or(hold.beside)
        } else {
            hold.beside
        };
        let Some(at) = row.iter().position(|&slot| slot == anchor) else {
            row.insert(from.min(row.len()), hold.node);
            continue;
        };
        if hold.after {
            row.insert(at + 1, hold.node);
            last.insert(hold.beside, hold.node);
        } else {
            row.insert(at, hold.node);
        }
    }
}

/// Disjoint sets of boxes.
struct Sets {
    parent: Vec<usize>,
}

impl Sets {
    fn new(n: usize) -> Self {
        Sets {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut at = x;
        while self.parent[at] != root {
            let next = self.parent[at];
            self.parent[at] = root;
            at = next;
        }
        root
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.parent[a.max(b)] = a.min(b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Direction, Edge, Frame, Layout, Node, framed, layout};
    use super::{Hold, Unheld};

    fn node() -> Node {
        Node {
            width: 60.0,
            height: 30.0,
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
            apart: false,
            marked: false,
            arrival: 0,
        }
    }

    fn held(nodes: usize, edges: &[Edge], holds: &[Hold]) -> Layout {
        framed(
            &vec![node(); nodes],
            &[],
            &[],
            edges,
            holds,
            Direction::Down,
        )
    }

    fn same_rank(laid: &Layout, a: usize, b: usize) -> bool {
        (laid.nodes[a].y - laid.nodes[b].y).abs() < 0.1
    }

    /// A chain 0 -> 1 -> 2 and 0 -> 3: 3 shares the rank of 1. Held after 2,
    /// it moves a rank down, right of 2; held before 1, it stands left of 1.
    #[test]
    fn a_held_box_takes_its_neighbors_rank_and_side() {
        let edges = [edge(0, 1), edge(1, 2), edge(0, 3)];
        let free = layout(&[node(); 4], &edges, Direction::Down);
        assert!(same_rank(&free, 1, 3));
        let after = held(
            4,
            &edges,
            &[Hold {
                node: 3,
                beside: 2,
                after: true,
            }],
        );
        assert!(after.unheld.is_empty(), "{:?}", after.unheld);
        assert!(same_rank(&after, 3, 2));
        assert!(after.nodes[3].x > after.nodes[2].x);
        let before = held(
            4,
            &edges,
            &[Hold {
                node: 3,
                beside: 1,
                after: false,
            }],
        );
        assert!(same_rank(&before, 3, 1));
        assert!(before.nodes[3].x < before.nodes[1].x);
    }

    /// Two boxes held after one neighbor follow it in the order written;
    /// a chain holds as written.
    #[test]
    fn holds_after_one_neighbor_keep_their_order() {
        let edges = [edge(0, 1), edge(0, 2), edge(0, 3)];
        let holds = [
            Hold {
                node: 3,
                beside: 1,
                after: true,
            },
            Hold {
                node: 2,
                beside: 1,
                after: true,
            },
        ];
        let laid = held(4, &edges, &holds);
        let x = |i: usize| laid.nodes[i].x;
        assert!(x(1) < x(3) && x(3) < x(2), "{:?}", laid.nodes);
    }

    #[test]
    fn holds_that_cannot_be_kept_are_said() {
        let edges = [edge(0, 1), edge(1, 2)];
        let free = layout(&[node(); 3], &edges, Direction::Down);
        let hold = |node: usize, beside: usize| Hold {
            node,
            beside,
            after: true,
        };
        let holds = [hold(1, 1), hold(1, 0), hold(2, 7), hold(2, 0), hold(0, 2)];
        let laid = held(3, &edges, &holds);
        assert_eq!(
            laid.unheld,
            [
                (0, Unheld::Itself),
                (1, Unheld::Joined),
                (2, Unheld::Missing),
                (4, Unheld::Round),
            ]
        );
        // 2 after 0 holds, though no line joins them: the line from 1
        // back up to their rank runs backward.
        assert!(same_rank(&laid, 0, 2));
        assert!(!same_rank(&free, 0, 2));
    }

    #[test]
    fn a_box_keeps_to_its_container() {
        let frames = [Frame::default()];
        let within = [None, Some(0), Some(0)];
        let laid = framed(
            &[node(); 3],
            &frames,
            &within,
            &[edge(1, 2)],
            &[Hold {
                node: 0,
                beside: 1,
                after: true,
            }],
            Direction::Down,
        );
        assert_eq!(laid.unheld, [(0, Unheld::Apart)]);
    }
}
