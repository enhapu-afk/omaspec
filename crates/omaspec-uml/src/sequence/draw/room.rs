//! Room found by drawing. Before a sequence is drawn, the columns are set
//! apart by what each item asks for ([`super::Drawer::place`]); how far a
//! frame's side or a note beside a lifeline reaches then depends on what
//! the drawing finds there: bars nested on bars, a self message's loop, a
//! note inside the frame, the head of one created, the frames inside. So
//! the sequence is drawn, and what crowds a lifeline it does not belong to
//! is measured: a frame's side or a note closer than its margin to a
//! lifeline or a bar outside it, a frame's words over the last lifeline
//! it holds, a frame past the picture's edges. Each asks for the columns
//! between to stand that much further apart, and the sequence is drawn
//! again ([`PASSES`] at most): the layout follows what is drawn, not a
//! guess of it.

use super::{BAR, BAR_SHIFT, Bar, FRAME};

/// How often a sequence is drawn at most to find its room.
pub(super) const PASSES: usize = 4;
/// What a note beside a lifeline keeps from another one, or a bar on it.
const NOTE_ROOM: f32 = 5.0;
/// What the edges of the picture, where a message from outside starts or
/// ends, keep from a frame.
const EDGE_ROOM: f32 = 5.0;

/// A frame as drawn: its sides, and where its words are (its tab's and
/// its `else`s'), each as `[left, top, right, bottom]`.
pub(super) struct Frame {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    pub words: Vec<[f32; 4]>,
}

/// A note drawn beside a lifeline: the lifeline's column, whether it
/// stands right of it, and the note as `[left, top, right, bottom]`.
pub(super) struct Note {
    pub column: usize,
    pub right: bool,
    pub at: [f32; 4],
}

/// What one drawing placed that may crowd a lifeline.
#[derive(Default)]
pub(super) struct Drawn {
    pub frames: Vec<Frame>,
    pub notes: Vec<Note>,
}

/// Room the columns keep beyond what the items ask for: lifelines `a` and
/// `b` at least `d` apart, a column at least so far from the left edge,
/// and the picture so far right of a column.
#[derive(Default, Clone)]
pub(super) struct Extra {
    pub need: Vec<(usize, usize, f32)>,
    pub left: Vec<(usize, f32)>,
    pub right: Vec<(usize, f32)>,
}

impl Extra {
    pub(super) fn is_empty(&self) -> bool {
        self.need.is_empty() && self.left.is_empty() && self.right.is_empty()
    }

    pub(super) fn extend(&mut self, more: Extra) {
        self.need.extend(more.need);
        self.left.extend(more.left);
        self.right.extend(more.right);
    }
}

/// The lifelines as drawn: where each column is, its bars, and from where
/// to where it runs.
pub(super) struct Lives<'a> {
    pub x: &'a [f32],
    pub bars: &'a [Bar],
    pub start: &'a [f32],
    pub end: &'a [Option<f32>],
    /// Where a lifeline that is not destroyed ends.
    pub bottom: f32,
    /// Where a message from or to the right edge ends.
    pub right_edge: f32,
}

impl Lives<'_> {
    /// From where to where across what stands on lifeline `c` between `top`
    /// and `bottom` reaches: the lifeline and its bars; `None` where the
    /// lifeline does not run then.
    fn across(&self, c: usize, top: f32, bottom: f32) -> Option<(f32, f32)> {
        let x = *self.x.get(c)?;
        let start = self.start.get(c).copied().unwrap_or(0.0);
        let end = self.end.get(c).copied().flatten().unwrap_or(self.bottom);
        if end <= top || start >= bottom {
            return None;
        }
        let bars = self
            .bars
            .iter()
            .filter(|bar| {
                bar.column == c && bar.start < bottom && bar.end.max(bar.start + 4.0) > top
            })
            .map(|bar| {
                let left = x - BAR / 2.0 + BAR_SHIFT * bar.depth as f32;
                (left, left + BAR)
            });
        Some(bars.fold((x, x), |(l, r), (a, b)| (l.min(a), r.max(b))))
    }
}

/// The room `drawn` lacks among `lives`, as more room between columns.
pub(super) fn crowded(drawn: &Drawn, lives: &Lives) -> Extra {
    let mut extra = Extra::default();
    let x = lives.x;
    for frame in &drawn.frames {
        let inside: Vec<usize> = (0..x.len())
            .filter(|&c| x[c] > frame.left && x[c] < frame.right)
            .collect();
        let (Some(&first), Some(&last)) = (inside.first(), inside.last()) else {
            continue;
        };
        for c in 0..x.len() {
            let Some((left, right)) = lives.across(c, frame.top, frame.bottom) else {
                continue;
            };
            // A lifeline left of the frame, or right of it, and what
            // stands on it, keeps the frame's margin.
            if x[c] <= frame.left {
                let short = right + FRAME - frame.left;
                if short > 0.01 {
                    extra.need.push((c, first, x[first] - x[c] + short));
                }
            } else if x[c] >= frame.right {
                let short = frame.right + FRAME - left;
                if short > 0.01 {
                    extra.need.push((last, c, x[c] - x[last] + short));
                }
            }
        }
        // Its words end the margin before the last lifeline it holds.
        if first < last {
            for &[from, top, to, bottom] in &frame.words {
                let Some((left, _)) = lives.across(last, top, bottom) else {
                    continue;
                };
                let short = to + FRAME - left;
                if from < left && short > 0.01 {
                    extra.need.push((first, last, x[last] - x[first] + short));
                }
            }
        }
        // It stays in the picture, and messages from outside start and
        // end beyond it.
        if frame.left < -0.01 {
            extra.left.push((first, x[first] - frame.left));
        }
        let short = frame.right + EDGE_ROOM - lives.right_edge;
        if short > 0.01 {
            extra.right.push((last, lives.right_edge - x[last] + short));
        }
    }
    for note in &drawn.notes {
        let [from, top, to, bottom] = note.at;
        let a = note.column;
        for c in 0..x.len() {
            if c == a {
                continue;
            }
            let Some((left, right)) = lives.across(c, top, bottom) else {
                continue;
            };
            if note.right && x[c] > x[a] && from < left {
                let short = to + NOTE_ROOM - left;
                if short > 0.01 {
                    extra.need.push((a, c, x[c] - x[a] + short));
                }
            } else if !note.right && x[c] < x[a] && to > right {
                let short = right + NOTE_ROOM - from;
                if short > 0.01 {
                    extra.need.push((c, a, x[a] - x[c] + short));
                }
            }
        }
    }
    extra
}
