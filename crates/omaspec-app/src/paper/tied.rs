//! Drawn words that read alike but are different things: a sequence
//! diagram's divider `== Payment ==` and its participant `Payment`.
//!
//! A label in the picture is matched to the source by its words and by
//! how many of them came before: the first `Payment` drawn is the first
//! written. That holds while every copy drawn is the same thing. Where
//! the drawer ties a copy to its line ([`omaspec_uml::map::Map::tied`]),
//! that copy and that line belong to each other, and the others are
//! counted without them.

use iced::{Point, Rectangle, Size};
use omaspec_core::locate;
use omaspec_render::pick;

use super::Paper;

/// What a copy of a text drawn is to the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Copy {
    /// One of those counted: the first drawn is the first written.
    Counted,
    /// Drawn for this line alone.
    Tied(usize),
    /// Drawn again for what is drawn already: a lifeline's foot.
    Again,
}

/// The copies of one text the picture draws, in the order drawn, and the
/// lines of the source that write it, in the order written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Copies {
    pub drawn: Vec<Copy>,
    /// For each line that writes it, the line and the byte on it.
    pub written: Vec<(usize, usize)>,
}

impl Copies {
    fn tied_line(&self, line: usize) -> bool {
        self.drawn.contains(&Copy::Tied(line))
    }

    /// The copies that are counted, in the order drawn.
    fn counted(&self) -> Vec<usize> {
        (0..self.drawn.len())
            .filter(|&copy| self.drawn[copy] == Copy::Counted)
            .collect()
    }

    /// The copy the `nth` mention, on the line `row`, is drawn as. A
    /// mention on a tied line is that line's copy; another is counted
    /// among the counted ones, without the tied lines before it, and a
    /// later one than there are copies keeps the last.
    pub fn copy_of(&self, nth: usize, row: usize) -> usize {
        if let Some(copy) = self.drawn.iter().position(|c| *c == Copy::Tied(row)) {
            return copy;
        }
        let mut tied: Vec<usize> = self
            .drawn
            .iter()
            .filter_map(|copy| match copy {
                Copy::Tied(line) => Some(*line),
                _ => None,
            })
            .collect();
        tied.sort_unstable();
        tied.dedup();
        let before = nth.saturating_sub(tied.iter().filter(|line| **line < row).count());
        let counted = self.counted();
        match counted.get(before).or(counted.last()) {
            Some(&copy) => copy,
            None => nth,
        }
    }

    /// Where the `copy` drawn is written: its tied line, or the mention
    /// that many of the counted come before. One drawn again is written
    /// where the one it repeats is.
    pub fn written_at(&self, copy: usize) -> Option<(usize, usize)> {
        let among = |kind: Copy| {
            self.drawn
                .iter()
                .take(copy)
                .filter(|drawn| **drawn == kind)
                .count()
        };
        let before = match self.drawn.get(copy).copied()? {
            // Its line, at the words where the line has them: also where
            // they are written in a way the count does not read.
            Copy::Tied(line) => {
                let written = self.written.iter().copied().find(|(at, _)| *at == line);
                return Some(written.unwrap_or((line, 0)));
            }
            Copy::Counted => among(Copy::Counted),
            // The first drawn again repeats the first counted.
            Copy::Again => among(Copy::Again).min(self.counted().len().saturating_sub(1)),
        };
        self.written
            .iter()
            .copied()
            .filter(|(line, _)| !self.tied_line(*line))
            .nth(before)
    }
}

impl Paper {
    /// The copies of `text` in the picture on screen and its mentions in
    /// `source`; none where the drawer says nothing of its words, which
    /// is most diagrams: the plain count holds there.
    pub(super) fn copies(&self, source: &str, text: &str) -> Option<Copies> {
        let map = &self.diagram.as_ref()?.drawn.map;
        if map.tied.is_empty() && map.echoes.is_empty() {
            return None;
        }
        let tree = self.shown_tree()?;
        let within = |r: omaspec_uml::graph::Rect, at: Point| {
            at.x >= r.x - 2.0
                && at.x <= r.x + r.w + 2.0
                && at.y >= r.y - 2.0
                && at.y <= r.y + r.h + 2.0
        };
        let copies = pick::texts(tree)
            .iter()
            .filter(|drawn| drawn.as_str() == text)
            .count();
        let drawn: Vec<Copy> = (0..copies)
            .map_while(|nth| pick::spot(tree, text, nth))
            .map(|(x, y, w, h)| {
                // The words themselves, not the box round them.
                let middle = Rectangle::new(Point::new(x, y), Size::new(w, h)).center();
                if map.echoes.iter().any(|echo| within(*echo, middle)) {
                    return Copy::Again;
                }
                map.tied
                    .iter()
                    .find(|tied| within(tied.rect, middle))
                    .map_or(Copy::Counted, |tied| Copy::Tied(tied.line))
            })
            .collect();
        if drawn.iter().all(|copy| *copy == Copy::Counted) {
            return None;
        }
        // The count gives the last mention again for every one after it.
        let mut written: Vec<(usize, usize)> = Vec::new();
        for nth in 0..source.len() {
            match locate::column_of(source, text, nth) {
                Some(at) if written.last() != Some(&at) => written.push(at),
                _ => break,
            }
        }
        Some(Copies { drawn, written })
    }

    /// The copy of `text` its `nth` mention in `source`, on the line
    /// `row`, is drawn as.
    pub(super) fn copy_of(&self, source: &str, text: &str, nth: usize, row: usize) -> usize {
        self.copies(source, text)
            .map_or(nth, |copies| copies.copy_of(nth, row))
    }

    /// The line of the source that writes the `copy` of `text` drawn, and
    /// the byte on it.
    pub fn written_at(&self, source: &str, text: &str, copy: usize) -> Option<(usize, usize)> {
        match self.copies(source, text) {
            Some(copies) => copies.written_at(copy),
            None => locate::column_of(source, text, copy),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A participant's head, a divider of its name and its foot, written
    /// as a declaration, a divider and two messages.
    #[test]
    fn a_tied_copy_and_its_line_belong_to_each_other() {
        let copies = Copies {
            // Drawn: the head, the foot, then the divider over them.
            drawn: vec![Copy::Counted, Copy::Again, Copy::Tied(26)],
            written: vec![(10, 12), (26, 3), (27, 8), (29, 2)],
        };
        // The divider's line is the divider; the declaration and every
        // other mention, before the divider or after it, is the head.
        assert_eq!(copies.copy_of(0, 10), 0);
        assert_eq!(copies.copy_of(1, 26), 2);
        assert_eq!(copies.copy_of(2, 27), 0);
        assert_eq!(copies.copy_of(3, 29), 0);
        assert_eq!(copies.copy_of(1, 20), 0);
        // A click on the divider lands on its line; one on the head or on
        // the foot, which repeats it, on the declaration.
        assert_eq!(copies.written_at(2), Some((26, 3)));
        assert_eq!(copies.written_at(0), Some((10, 12)));
        assert_eq!(copies.written_at(1), Some((10, 12)));
        // Tied to a line the count finds no such words on: that line.
        let unread = Copies {
            drawn: vec![Copy::Tied(5)],
            written: Vec::new(),
        };
        assert_eq!(unread.written_at(0), Some((5, 0)));
    }

    /// Two boxes that read alike are counted, the first drawn the first
    /// written.
    #[test]
    fn what_is_counted_is_counted_in_order() {
        let copies = Copies {
            drawn: vec![Copy::Counted, Copy::Counted],
            written: vec![(1, 0), (4, 2), (6, 2)],
        };
        assert_eq!(copies.copy_of(1, 4), 1);
        assert_eq!(copies.copy_of(2, 6), 1);
        assert_eq!(copies.written_at(1), Some((4, 2)));
    }
}
