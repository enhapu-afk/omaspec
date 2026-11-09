//! The arrows between the cards of a map: where a scenario plays against
//! a machine, a state opens a diagram, a diagram includes another.
//!
//! The arrows that arrive alike at one card are one: their lines meet in
//! a pill with their words, and one line leads on from it into the card,
//! with one head. So do those that leave one card alike for cards nothing
//! else leads to. The pill is a box of the layout like a card
//! ([`super::layout`]), so it has room of its own and no line runs under
//! it; the lines into it share a trunk as the lines into any box do
//! ([0022](../../../../../docs/adr/0022-lines-bundled.md)), and those
//! that leave it are joined here ([`leave_as_one`]).

use super::RelationKind;
use crate::graph::Rect;
use crate::graph::route::Point;

/// An arrow with its words placed.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Arrow {
    /// The relations it stands for, sorted; the first sets its line.
    pub kinds: Vec<RelationKind>,
    /// Its lines by their corners, each from a card to the pill or from
    /// the pill to a card; the latter end in a head.
    pub lines: Vec<(Vec<Point>, bool)>,
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

/// The words of an arrow that stands for `kinds`.
pub(super) fn words(kinds: &[RelationKind]) -> String {
    kinds
        .iter()
        .map(|k| k.word())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The lines that leave the pill at `pill` downward leave it as one: each
/// starts in the middle below it and turns off where it did before, so
/// they are a trunk with branches, not a comb. One that runs straight down
/// into its card stays, and the others start where it does.
pub(super) fn leave_as_one(pill: Rect, lines: &mut [Vec<Point>]) {
    if lines.len() < 2 {
        return;
    }
    let level = |a: f32, b: f32| (a - b).abs() < 0.5;
    let straight = lines
        .iter()
        .find(|line| line.len() == 2 && level(line[0].0, line[1].0))
        .map(|line| line[0].0);
    let trunk = straight.unwrap_or(pill.x + pill.w / 2.0);
    for line in lines.iter_mut() {
        // Down from the pill, then across: the first run moves onto the
        // trunk, and the run across starts there.
        if line.len() >= 3 && level(line[0].0, line[1].0) && level(line[1].1, line[2].1) {
            line[0].0 = trunk;
            line[1].0 = trunk;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_that_leave_a_pill_start_in_one_place() {
        let pill = Rect {
            x: 100.0,
            y: 0.0,
            w: 60.0,
            h: 22.0,
        };
        let mut lines = vec![
            vec![(112.0, 22.0), (112.0, 40.0), (20.0, 40.0), (20.0, 80.0)],
            vec![(148.0, 22.0), (148.0, 50.0), (240.0, 50.0), (240.0, 80.0)],
        ];
        leave_as_one(pill, &mut lines);
        assert_eq!(lines[0][..2], [(130.0, 22.0), (130.0, 40.0)]);
        assert_eq!(lines[1][..2], [(130.0, 22.0), (130.0, 50.0)]);
        // The ends stay where the cards are.
        assert_eq!(lines[0][3], (20.0, 80.0));

        // One runs straight down: the other starts with it.
        let mut lines = vec![
            vec![(120.0, 22.0), (120.0, 80.0)],
            vec![(140.0, 22.0), (140.0, 50.0), (240.0, 50.0), (240.0, 80.0)],
        ];
        leave_as_one(pill, &mut lines);
        assert_eq!(lines[0], [(120.0, 22.0), (120.0, 80.0)]);
        assert_eq!(lines[1][..2], [(120.0, 22.0), (120.0, 50.0)]);
    }
}
