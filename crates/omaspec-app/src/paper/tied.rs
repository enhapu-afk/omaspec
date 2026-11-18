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

/// The copies of one text the picture draws, in the order drawn, and the
/// lines of the source that write it, in the order written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Copies {
    /// For each copy drawn, the line it is tied to, if any.
    pub drawn: Vec<Option<usize>>,
    /// For each mention, its zero-based line and the byte on it.
    pub written: Vec<(usize, usize)>,
}

impl Copies {
    fn tied_line(&self, line: usize) -> bool {
        self.drawn.contains(&Some(line))
    }

    /// The copy the `nth` mention, on the line `row`, is drawn as. A
    /// mention on a tied line is that line's copy; another is counted
    /// among the others, without the tied lines before it, and a later one
    /// than there are copies keeps the last (a lifeline's foot).
    pub fn copy_of(&self, nth: usize, row: usize) -> usize {
        if let Some(copy) = self.drawn.iter().position(|tied| *tied == Some(row)) {
            return copy;
        }
        let mut tied: Vec<usize> = self.drawn.iter().flatten().copied().collect();
        tied.sort_unstable();
        tied.dedup();
        let before = nth.saturating_sub(tied.iter().filter(|line| **line < row).count());
        let free: Vec<usize> = (0..self.drawn.len())
            .filter(|&copy| self.drawn[copy].is_none())
            .collect();
        match free.get(before).or(free.last()) {
            Some(&copy) => copy,
            None => nth,
        }
    }

    /// Where the `copy` drawn is written: its tied line, or the mention
    /// that many of the others come before.
    pub fn written_at(&self, copy: usize) -> Option<(usize, usize)> {
        // Its line, at the words where the line has them: also where
        // they are written in a way the count does not read.
        if let Some(line) = self.drawn.get(copy).copied().flatten() {
            let written = self.written.iter().copied().find(|(at, _)| *at == line);
            return Some(written.unwrap_or((line, 0)));
        }
        let before = self
            .drawn
            .iter()
            .take(copy)
            .filter(|tied| tied.is_none())
            .count();
        self.written
            .iter()
            .copied()
            .filter(|(line, _)| !self.tied_line(*line))
            .nth(before)
    }
}

impl Paper {
    /// The copies of `text` in the picture on screen and its mentions in
    /// `source`; none where the drawer ties no words to a line, which is
    /// every diagram but a few: the plain count holds there.
    pub(super) fn copies(&self, source: &str, text: &str) -> Option<Copies> {
        let tied = &self.diagram.as_ref()?.drawn.map.tied;
        if tied.is_empty() {
            return None;
        }
        let tree = self.shown_tree()?;
        let copies = pick::texts(tree)
            .iter()
            .filter(|drawn| drawn.as_str() == text)
            .count();
        let drawn: Vec<Option<usize>> = (0..copies)
            .map_while(|nth| pick::seat(tree, text, nth))
            .map(|seat| {
                let (x, y, w, h) = seat.bounds();
                let middle = Rectangle::new(Point::new(x, y), Size::new(w, h)).center();
                tied.iter()
                    .find(|tied| {
                        let r = tied.rect;
                        middle.x >= r.x - 2.0
                            && middle.x <= r.x + r.w + 2.0
                            && middle.y >= r.y - 2.0
                            && middle.y <= r.y + r.h + 2.0
                    })
                    .map(|tied| tied.line)
            })
            .collect();
        if !drawn.iter().any(Option::is_some) {
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
            drawn: vec![None, None, Some(26)],
            written: vec![(10, 12), (26, 3), (27, 8), (29, 2)],
        };
        // The declaration is the head, the divider's line the divider,
        // and each message after it the foot.
        assert_eq!(copies.copy_of(0, 10), 0);
        assert_eq!(copies.copy_of(1, 26), 2);
        assert_eq!(copies.copy_of(2, 27), 1);
        assert_eq!(copies.copy_of(3, 29), 1);
        // A mention before the divider is counted without it.
        assert_eq!(copies.copy_of(1, 20), 1);
        // A click on the divider lands on its line, one on the foot on
        // the first message, not on the divider's line.
        assert_eq!(copies.written_at(2), Some((26, 3)));
        assert_eq!(copies.written_at(0), Some((10, 12)));
        assert_eq!(copies.written_at(1), Some((27, 8)));
        // Tied to a line the count finds no such words on: that line.
        let unread = Copies {
            drawn: vec![Some(5)],
            written: Vec::new(),
        };
        assert_eq!(unread.written_at(0), Some((5, 0)));
    }

    /// With nothing tied the count is the plain one.
    #[test]
    fn nothing_tied_is_counted_plainly() {
        let copies = Copies {
            drawn: vec![None, None],
            written: vec![(1, 0), (4, 2), (6, 2)],
        };
        assert_eq!(copies.written_at(1), Some((4, 2)));
    }
}
