//! The self-loops of a box: lines from it back to itself, on its free
//! side (right when the flow runs down, below when it runs right).
//!
//! One loop is a small hook beside the box. Several lie around one
//! another, from the box outward, in the order they are written: each
//! leaves and arrives at places of its own, so each ends in a head of its
//! own, and each reaches out past the words of the one before, which lie
//! in the ring between the two. So no loop runs through words, as two
//! loops of one size and their words did. A box grows along that side
//! until every loop ends on it ([`Loops::side`]).

use super::{Direction, Rect};

/// Half of what a box's first self-loop takes of its side, and how far it
/// reaches out.
const LOOP_HALF: f32 = 7.0;
const LOOP_REACH: f32 = 16.0;
/// Each next loop leaves and arrives this much further from the middle of
/// the side, so each has a head of its own; and reaches at least this much
/// further out when the one before has no words.
pub(super) const LOOP_STEP: f32 = 10.0;
pub(super) const LOOP_RING: f32 = 12.0;
/// From a loop to its words, and from those to the next loop around them.
pub(super) const LOOP_WORDS: f32 = 4.0;
const LOOP_CLEAR: f32 = 6.0;
/// From the outermost loop's ends to the corners of the side.
const LOOP_INSET: f32 = 6.0;

/// The self-loops of one box, from the one next to it outward. All sit on
/// its free side. Each leaves and arrives at places of its own, further
/// from the middle of the side than the one before, and reaches out past
/// the words of the one before: those lie in the ring between the two, so
/// every loop ends in its own head and no line runs through words.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Loops {
    count: usize,
    /// The last loop: half of what it takes of the side, how far it
    /// reaches, and how far its words reach beyond it and along the side.
    half: f32,
    reach: f32,
    words_out: f32,
    words_along: f32,
}

impl Loops {
    /// The next loop outward, with words of `label`'s width and height:
    /// half of what it takes of the side, and how far it reaches out.
    pub(super) fn next(&mut self, label: (f32, f32), direction: Direction) -> (f32, f32) {
        let (out, along) = match direction {
            Direction::Down => label,
            Direction::Right => (label.1, label.0),
        };
        let (half, reach) = if self.count == 0 {
            (LOOP_HALF, LOOP_REACH)
        } else if self.words_out > 0.0 {
            (
                (self.half + LOOP_STEP).max(around(self.words_along / 2.0)),
                (self.reach + LOOP_WORDS + self.words_out + LOOP_CLEAR).max(self.reach + LOOP_RING),
            )
        } else {
            (self.half + LOOP_STEP, self.reach + LOOP_RING)
        };
        let worded = label.0 > 0.0 || label.1 > 0.0;
        *self = Loops {
            count: self.count + 1,
            half,
            reach,
            words_out: if worded { out } else { 0.0 },
            words_along: if worded { along } else { 0.0 },
        };
        (half, reach)
    }

    /// The room they take beside the box, the last one's words included.
    pub(super) fn room(&self) -> f32 {
        match self.count {
            0 => 0.0,
            _ => self.reach + LOOP_WORDS + self.words_out,
        }
    }

    /// How long the side has to be for all of them to end on it, clear of
    /// its corners; nothing for a single loop, which any box holds.
    pub(super) fn side(&self) -> f32 {
        if self.count < 2 {
            0.0
        } else {
            2.0 * (self.half + LOOP_INSET)
        }
    }
}

/// Half of what a loop takes of the side to pass around words that take
/// `words` to either side of its middle. The loop turns in a curve at its
/// two far corners, as wide as half of it or [`crate::curve::RADIUS`],
/// and that curve cuts the corner the words reach into, [`LOOP_CLEAR`]
/// before the loop's far run: the loop is wide enough for the curve to
/// pass that corner.
fn around(words: f32) -> f32 {
    let (clear, widest) = (LOOP_CLEAR, crate::curve::RADIUS);
    // As wide as the curve it turns in, the loop passes the corner at
    // this half; wider, its curve is the widest there is.
    let tight = (words * words + clear * clear) / (2.0 * clear);
    let curved = if tight <= widest {
        tight
    } else {
        words + widest - (2.0 * widest * clear - clear * clear).sqrt()
    };
    curved.max(words + LOOP_WORDS)
}

/// A self-loop on the free side of `rect`: `half` to either side of the
/// middle of that side, reaching `reach` out.
pub(super) fn self_loop(
    rect: Rect,
    half: f32,
    reach: f32,
    direction: Direction,
) -> Vec<(f32, f32)> {
    match direction {
        Direction::Down => {
            let x = rect.right();
            let y = rect.cy();
            vec![
                (x, y - half),
                (x + reach, y - half),
                (x + reach, y + half),
                (x, y + half),
            ]
        }
        Direction::Right => {
            let x = rect.cx();
            let y = rect.bottom();
            vec![
                (x - half, y),
                (x - half, y + reach),
                (x + half, y + reach),
                (x + half, y),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loop_passes_the_words_it_lies_around_in_its_curve() {
        // The corner of the words, LOOP_CLEAR before the loop's far run,
        // lies inside the curve the loop turns in there.
        for words in [4.0f32, 7.5, 15.0, 30.0, 90.0] {
            let half = around(words);
            assert!(half >= words + LOOP_WORDS - 0.01);
            let curve = crate::curve::RADIUS.min(half);
            // From the middle of the curve's circle to that corner.
            let (along, out) = (words - (half - curve), curve - LOOP_CLEAR);
            assert!(
                along <= 0.0 || along.hypot(out) <= curve + 0.01,
                "{words}: {along} {out} {curve}"
            );
        }
    }

    #[test]
    fn a_box_with_several_self_loops_needs_a_side_for_all_their_ends() {
        let mut loops = Loops::default();
        assert_eq!((loops.room(), loops.side()), (0.0, 0.0));
        // One loop sits on any box, as it always has.
        assert_eq!(loops.next((30.0, 14.0), Direction::Down), (7.0, 16.0));
        assert_eq!((loops.room(), loops.side()), (50.0, 0.0));
        // The second lies around the first and its words.
        let (half, reach) = loops.next((40.0, 14.0), Direction::Down);
        assert_eq!((half, reach), (17.0, 56.0));
        assert_eq!(loops.room(), 100.0);
        assert_eq!(loops.side(), 46.0);
        // Under a box, words are as wide along the side as they are long.
        let mut below = Loops::default();
        let _ = below.next((120.0, 14.0), Direction::Right);
        let (half, _) = below.next((0.0, 0.0), Direction::Right);
        assert!(half > 60.0 + LOOP_WORDS, "{half}");
        assert!(below.side() > 120.0);
    }
}
