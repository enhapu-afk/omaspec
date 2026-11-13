//! Boxes held in place ([0021](../../../../docs/adr/0021-boxes-held-in-place.md),
//! [0028](../../../../docs/adr/0028-a-box-in-another-rank.md)): a box held
//! beside a neighbor takes the neighbor's rank, the two ranked as one as
//! `dot` ranks `rank=same` ([`groups`]), and stands right after or before
//! it across the flow once the ranks are ordered ([`order`]); a box held
//! level with it takes its rank and the order the layout gives. A box held
//! below or above a neighbor along the flow is ranked after or before it,
//! by a line that ranks and orders the two and is not drawn, as `dot`'s
//! invisible edge ([`super::framed`]). Placement is as before. A hold that
//! cannot be kept is said ([`Unheld`]), and the box stands where the layout
//! puts it.

use std::collections::HashMap;
use std::fmt;

use super::{Direction, Edge};
use crate::hand;

/// The word of an entry that holds a box against its neighbor. `after`
/// and `before` go across the flow, whichever way it runs; the others go
/// by the page: across the flow or along it, as the flow runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Word {
    After,
    Before,
    Below,
    Above,
    Right,
    Left,
    Level,
}

/// Where a word puts a box against its neighbor in a flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// In its rank, right after it across the flow, or right before it.
    Beside { after: bool },
    /// In its rank, where the layout orders it.
    Level,
    /// In a rank after its own along the flow (`later`), or before it.
    Rank { later: bool },
}

impl Word {
    pub const ALL: [Word; 7] = [
        Word::After,
        Word::Before,
        Word::Below,
        Word::Above,
        Word::Right,
        Word::Left,
        Word::Level,
    ];

    /// How the entry writes it.
    pub fn text(self) -> &'static str {
        match self {
            Word::After => "after",
            Word::Before => "before",
            Word::Below => "below",
            Word::Above => "above",
            Word::Right => "right of",
            Word::Left => "left of",
            Word::Level => "level with",
        }
    }

    /// Where it puts the box in a flow that runs `direction`: below a
    /// neighbor is a later rank when the flow runs down, after it across
    /// the flow when it runs to the right; right of it the other way round.
    pub fn place(self, direction: Direction) -> Place {
        use Direction::{Down, Right};
        match (self, direction) {
            (Word::After, _) => Place::Beside { after: true },
            (Word::Before, _) => Place::Beside { after: false },
            (Word::Level, _) => Place::Level,
            (Word::Below, Down) | (Word::Right, Right) => Place::Rank { later: true },
            (Word::Above, Down) | (Word::Left, Right) => Place::Rank { later: false },
            (Word::Right, Down) | (Word::Below, Right) => Place::Beside { after: true },
            (Word::Left, Down) | (Word::Above, Right) => Place::Beside { after: false },
        }
    }

    /// The word for a layout laid out unmirrored and then mirrored along
    /// its flow (a Mermaid `direction BT` or `RL`): what is below on the
    /// page is laid above, what is right of it laid left. Across the flow
    /// nothing turns.
    pub fn mirrored(self, direction: Direction) -> Word {
        match (self, direction) {
            (Word::Below, Direction::Down) => Word::Above,
            (Word::Above, Direction::Down) => Word::Below,
            (Word::Right, Direction::Right) => Word::Left,
            (Word::Left, Direction::Right) => Word::Right,
            (word, _) => word,
        }
    }

    /// `after` or `before`, the words a drag writes.
    pub fn beside(after: bool) -> Word {
        if after { Word::After } else { Word::Before }
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text())
    }
}

/// A box held against another: `node` stands where `word` puts it
/// against `beside`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hold {
    pub node: usize,
    pub beside: usize,
    pub word: Word,
}

/// A box held in its neighbor's rank, as ranking and ordering take it:
/// right after it across the flow (`Some(true)`), right before it, or
/// where the layout orders it (`None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Same {
    pub node: usize,
    pub beside: usize,
    pub after: Option<bool>,
}

/// A box held against another in one container, as a drawer gives it to
/// the layout ([`super::open::Nested::holds`]): the entry's zero-based
/// line, and the two boxes by their index among the container's members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    pub line: usize,
    pub index: usize,
    pub beside: usize,
    pub word: Word,
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
            word: hold.word,
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
    /// The lines between the two rank them the other way round.
    Against,
}

/// The holds in one rank that can be kept, in their order, and why each
/// other one cannot: by its index in `holds`. `n` boxes, `within` the
/// frame around each (empty when there are none), `edges` every line
/// between them.
pub(super) fn check(
    n: usize,
    within: &[Option<usize>],
    edges: &[Edge],
    holds: &[Same],
) -> (Vec<Same>, Vec<(usize, Unheld)>) {
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

/// Why a hold into another rank cannot be kept before the layout, if it
/// cannot: `n` boxes, `within` the frame around each.
pub(super) fn ranked_check(n: usize, within: &[Option<usize>], hold: &Hold) -> Option<Unheld> {
    let (a, b) = (hold.node, hold.beside);
    let frame = |x: usize| within.get(x).copied().flatten();
    if a >= n || b >= n {
        Some(Unheld::Missing)
    } else if a == b {
        Some(Unheld::Itself)
    } else if frame(a) != frame(b) {
        Some(Unheld::Apart)
    } else {
        None
    }
}

/// For every box, the box it is ranked as: one of each group of boxes the
/// holds put in one rank, itself for a box held by none.
pub(super) fn groups(n: usize, holds: &[Same]) -> Vec<usize> {
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
/// boxes held after one neighbor follow it in that order. A box held level
/// with its neighbor stays where the sweeps put it.
pub(super) fn order(rows: &mut [Vec<usize>], holds: &[Same]) {
    let holds: Vec<(Same, bool)> = holds
        .iter()
        .filter_map(|hold| hold.after.map(|after| (*hold, after)))
        .collect();
    // A box is moved before the boxes held beside it, however the holds
    // were written: moved after them, it would leave them behind.
    let steps = |hold: &Same| {
        let (mut at, mut steps) = (hold.beside, 0);
        while let Some((next, _)) = holds.iter().find(|(other, _)| other.node == at) {
            (at, steps) = (next.beside, steps + 1);
            if steps > holds.len() {
                break;
            }
        }
        steps
    };
    let mut sorted: Vec<&(Same, bool)> = holds.iter().collect();
    sorted.sort_by_cached_key(|(hold, _)| steps(hold));
    // The last box placed after each neighbor so far.
    let mut last: std::collections::HashMap<usize, usize> = Default::default();
    for &&(hold, after) in &sorted {
        let Some(row) = rows.iter_mut().find(|row| row.contains(&hold.beside)) else {
            continue;
        };
        let Some(from) = row.iter().position(|&slot| slot == hold.node) else {
            continue;
        };
        row.remove(from);
        let anchor = if after {
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
        if after {
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
    use super::{Hold, Unheld, Word};

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
            label_before: None,
            level: None,
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
                word: Word::After,
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
                word: Word::Before,
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
                word: Word::After,
            },
            Hold {
                node: 2,
                beside: 1,
                word: Word::After,
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
            word: Word::After,
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
                word: Word::After,
            }],
            Direction::Down,
        );
        assert_eq!(laid.unheld, [(0, Unheld::Apart)]);
    }

    /// A chain 0 -> 1 and a box 2 on its own: held below 1 it takes the
    /// rank after 1's, above 0 a rank before 0's; level with 1 it takes
    /// 1's rank where the layout orders it. Right of 1 is after it.
    #[test]
    fn a_box_goes_into_another_rank_by_the_page() {
        let edges = [edge(0, 1)];
        let at = |word: Word, beside: usize| {
            held(
                3,
                &edges,
                &[Hold {
                    node: 2,
                    beside,
                    word,
                }],
            )
        };
        let below = at(Word::Below, 1);
        assert!(below.unheld.is_empty(), "{:?}", below.unheld);
        assert!(below.nodes[2].y > below.nodes[1].y + below.nodes[1].h);
        let above = at(Word::Above, 0);
        assert!(above.nodes[2].y + above.nodes[2].h < above.nodes[0].y);
        let level = at(Word::Level, 1);
        assert!(same_rank(&level, 2, 1));
        let right = at(Word::Right, 1);
        assert!(same_rank(&right, 2, 1) && right.nodes[2].x > right.nodes[1].x);
        // Across the flow, right of a box is the rank after its own.
        let across = framed(
            &[node(); 3],
            &[],
            &[],
            &edges,
            &[Hold {
                node: 2,
                beside: 1,
                word: Word::Right,
            }],
            Direction::Right,
        );
        assert!(across.nodes[2].x > across.nodes[1].x + across.nodes[1].w);
        // No line is drawn for it.
        assert_eq!(below.routes.len(), 1);
    }

    /// 0 -> 1: held above 1, 0 is above it already; 1 held above 0 goes
    /// against the line, which keeps its way down.
    #[test]
    fn a_rank_against_the_lines_is_said() {
        let edges = [edge(0, 1), edge(1, 2)];
        let hold = |node, beside, word| Hold { node, beside, word };
        let kept = held(3, &edges, &[hold(0, 1, Word::Above)]);
        assert!(kept.unheld.is_empty(), "{:?}", kept.unheld);
        let against = held(
            3,
            &edges,
            &[
                hold(2, 2, Word::Below),
                hold(2, 0, Word::Above),
                hold(0, 9, Word::Below),
            ],
        );
        assert_eq!(
            against.unheld,
            [
                (0, Unheld::Itself),
                (1, Unheld::Against),
                (2, Unheld::Missing)
            ]
        );
        // The line B -> A keeps its way whichever box comes first: B below
        // A is said, not drawn by turning the line around.
        let back = [edge(1, 0)];
        let laid = held(2, &back, &[hold(1, 0, Word::Below)]);
        assert_eq!(laid.unheld, [(0, Unheld::Against)]);
        assert!(laid.nodes[0].y > laid.nodes[1].y, "{:?}", laid.nodes);
    }
}
