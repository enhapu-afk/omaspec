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
pub(super) enum Drawn {
    /// One of those counted: the first drawn is the first written.
    Counted,
    /// Drawn for the item that begins on this line, on so many lines.
    Tied { line: usize, span: usize },
    /// Drawn again for what is drawn already: a lifeline's foot.
    Again,
}

/// The copies of one text the picture draws, in the order drawn, and the
/// lines of the source that write it, in the order written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Copies {
    pub drawn: Vec<Drawn>,
    /// For each line that writes it, the line and the byte on it.
    pub written: Vec<(usize, usize)>,
    /// The lines that write it more than once: what such a line itself
    /// draws is the last, and the mentions before it are counted.
    pub twice: Vec<usize>,
}

impl Copies {
    /// Where the words of the tied `copy` are written: on the line its
    /// item begins on, or, in a block that begins there (a note up to
    /// its `end note`), on one of the lines it is drawn on after it.
    /// `None` for a copy that is not tied, or whose words the count does
    /// not find there.
    fn home(&self, copy: usize) -> Option<(usize, usize)> {
        let Drawn::Tied { line, span } = self.drawn.get(copy).copied()? else {
            return None;
        };
        let on = |row: usize| self.written.iter().copied().find(|(at, _)| *at == row);
        on(line).or_else(|| (line + 1..=line + span).find_map(on))
    }

    /// The lines whose mention is a tied copy's and no other's: those the
    /// count passes over.
    fn taken(&self) -> Vec<usize> {
        let mut rows = self.homes();
        rows.retain(|row| !self.twice.contains(row));
        rows
    }

    /// The lines tied copies are written on, each once, in order.
    fn homes(&self) -> Vec<usize> {
        let mut rows: Vec<usize> = (0..self.drawn.len())
            .filter_map(|copy| self.home(copy).map(|(row, _)| row))
            .collect();
        rows.sort_unstable();
        rows.dedup();
        rows
    }

    /// The copies that are counted, in the order drawn.
    fn counted(&self) -> Vec<usize> {
        (0..self.drawn.len())
            .filter(|&copy| self.drawn[copy] == Drawn::Counted)
            .collect()
    }

    /// The copy the `nth` mention, on the line `row`, is drawn as. With
    /// `own`, the mention is what that line itself draws, and it is the
    /// copy tied there. Any other is counted among the counted ones,
    /// without the tied lines before it, and a later one than there are
    /// copies keeps the last.
    pub fn copy_of(&self, nth: usize, row: usize, own: bool) -> usize {
        if own
            && let Some(copy) =
                (0..self.drawn.len()).find(|&copy| self.home(copy).is_some_and(|(at, _)| at == row))
        {
            return copy;
        }
        let before = nth.saturating_sub(self.taken().iter().filter(|at| **at < row).count());
        let counted = self.counted();
        match counted.get(before).or(counted.last()) {
            Some(&copy) => copy,
            None => nth,
        }
    }

    /// Where the `copy` drawn is written: where its words are, or its
    /// item's line where the count does not find them; for one that is
    /// counted, the mention that many of the counted come before. One
    /// drawn again is written where the one it repeats is.
    pub fn written_at(&self, copy: usize) -> Option<(usize, usize)> {
        let among = |kind: Drawn| {
            self.drawn
                .iter()
                .take(copy)
                .filter(|drawn| **drawn == kind)
                .count()
        };
        let before = match self.drawn.get(copy).copied()? {
            Drawn::Tied { line, .. } => return Some(self.home(copy).unwrap_or((line, 0))),
            Drawn::Counted => among(Drawn::Counted),
            // The first drawn again repeats the first counted.
            Drawn::Again => among(Drawn::Again).min(self.counted().len().saturating_sub(1)),
        };
        let taken = self.taken();
        self.written
            .iter()
            .copied()
            .filter(|(line, _)| !taken.contains(line))
            .nth(before)
    }
}

/// Whether the words `text` at the byte `col` of `line` are the last of
/// them there: what a line itself draws stands after what it names
/// (`User -> Login : Login`). Words are read as the count reads them
/// ([`locate::spans_on`]): `Login` is not written in `LoginPage`.
fn last_on_line(line: &str, text: &str, col: usize) -> bool {
    !locate::spans_on(line, text)
        .iter()
        .any(|(from, _)| *from > col)
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
        // By the words themselves, not by a box they name: the drawer
        // told where it drew the words.
        let drawn: Vec<Drawn> = pick::words(tree, text)
            .into_iter()
            .map(|(x, y, w, h)| {
                let middle = Rectangle::new(Point::new(x, y), Size::new(w, h)).center();
                if map.echoes.iter().any(|echo| within(*echo, middle)) {
                    return Drawn::Again;
                }
                map.tied
                    .iter()
                    .find(|tied| within(tied.rect, middle))
                    .map_or(Drawn::Counted, |tied| Drawn::Tied {
                        line: tied.line,
                        span: tied.span,
                    })
            })
            .collect();
        if drawn.iter().all(|copy| *copy == Drawn::Counted) {
            return None;
        }
        let written = locate::columns_of(source, text);
        let lines: Vec<&str> = source.split('\n').collect();
        let twice = written
            .iter()
            .filter(|(row, _)| {
                lines
                    .get(*row)
                    .is_some_and(|line| locate::spans_on(line, text).len() > 1)
            })
            .map(|(row, _)| *row)
            .collect();
        Some(Copies {
            drawn,
            written,
            twice,
        })
    }

    /// The copy of `text` its `nth` mention in `source`, at the byte `col`
    /// of the line `row`, is drawn as.
    pub(super) fn copy_of(
        &self,
        source: &str,
        text: &str,
        nth: usize,
        (row, col): (usize, usize),
    ) -> usize {
        let Some(copies) = self.copies(source, text) else {
            return nth;
        };
        let own = source
            .split('\n')
            .nth(row)
            .is_none_or(|line| last_on_line(line, text, col));
        copies.copy_of(nth, row, own)
    }

    /// The line of the source that writes the `copy` of `text` drawn, and
    /// the byte on it.
    pub fn written_at(&self, source: &str, text: &str, copy: usize) -> Option<(usize, usize)> {
        let Some(copies) = self.copies(source, text) else {
            return locate::column_of(source, text, copy);
        };
        let (row, col) = copies.written_at(copy)?;
        // What the line itself draws is the last of such words on it.
        let tied = matches!(copies.drawn.get(copy), Some(Drawn::Tied { .. }));
        let last = source
            .split('\n')
            .nth(row)
            .filter(|_| tied)
            .and_then(|line| locate::spans_on(line, text).last().map(|(from, _)| *from));
        Some((row, last.unwrap_or(col)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tied(line: usize) -> Drawn {
        Drawn::Tied { line, span: 1 }
    }

    /// A participant's head, a divider of its name and its foot, written
    /// as a declaration, a divider and two messages.
    #[test]
    fn a_tied_copy_and_its_line_belong_to_each_other() {
        let copies = Copies {
            // Drawn: the head, the foot, then the divider over them.
            drawn: vec![Drawn::Counted, Drawn::Again, tied(26)],
            written: vec![(10, 12), (26, 3), (27, 8), (29, 2)],
            twice: Vec::new(),
        };
        // The divider's line is the divider; the declaration and every
        // other mention, before the divider or after it, is the head.
        assert_eq!(copies.copy_of(0, 10, true), 0);
        assert_eq!(copies.copy_of(1, 26, true), 2);
        assert_eq!(copies.copy_of(2, 27, true), 0);
        assert_eq!(copies.copy_of(3, 29, true), 0);
        assert_eq!(copies.copy_of(1, 20, true), 0);
        // A name the divider's line only mentions is the head too.
        assert_eq!(copies.copy_of(1, 26, false), 0);
        // A click on the divider lands on its line; one on the head or on
        // the foot, which repeats it, on the declaration.
        assert_eq!(copies.written_at(2), Some((26, 3)));
        assert_eq!(copies.written_at(0), Some((10, 12)));
        assert_eq!(copies.written_at(1), Some((10, 12)));
        // Tied to a line the count finds no such words on: that line.
        let unread = Copies {
            drawn: vec![tied(5)],
            written: Vec::new(),
            twice: Vec::new(),
        };
        assert_eq!(unread.written_at(0), Some((5, 0)));
    }

    /// The words of a block are written on the lines after the one it
    /// begins on: a note up to its `end note`.
    #[test]
    fn a_block_s_words_are_written_after_the_line_it_begins_on() {
        let copies = Copies {
            drawn: vec![Drawn::Counted, Drawn::Tied { line: 4, span: 2 }],
            written: vec![(1, 12), (6, 2), (9, 0)],
            twice: Vec::new(),
        };
        assert_eq!(copies.written_at(1), Some((6, 2)));
        assert_eq!(copies.copy_of(1, 6, true), 1);
        // The mention after the block is counted without it.
        assert_eq!(copies.copy_of(2, 9, true), 0);
    }

    /// Two boxes that read alike are counted, the first drawn the first
    /// written.
    #[test]
    fn what_is_counted_is_counted_in_order() {
        let copies = Copies {
            drawn: vec![Drawn::Counted, Drawn::Counted],
            written: vec![(1, 0), (4, 2), (6, 2)],
            twice: Vec::new(),
        };
        assert_eq!(copies.copy_of(1, 4, true), 1);
        assert_eq!(copies.copy_of(2, 6, true), 1);
        assert_eq!(copies.written_at(1), Some((4, 2)));
    }

    /// A line that names a state and labels its line with the same words
    /// is the state's first mention all the same.
    #[test]
    fn a_line_that_writes_it_twice_still_counts() {
        let copies = Copies {
            drawn: vec![Drawn::Counted, tied(2), tied(3)],
            written: vec![(2, 9), (3, 7), (5, 0)],
            twice: vec![2],
        };
        assert_eq!(copies.written_at(0), Some((2, 9)));
        assert_eq!(copies.copy_of(2, 5, true), 0);
    }

    #[test]
    fn what_a_line_draws_is_the_last_of_such_words_on_it() {
        let line = "User -> Login : Login";
        assert!(!last_on_line(line, "Login", 9));
        assert!(last_on_line(line, "Login", 17));
        assert!(last_on_line("== Payment ==", "Payment", 4));
        // A longer name that begins with the words does not write them.
        assert!(last_on_line("Busy --> BusyBox", "Busy", 0));
    }
}
