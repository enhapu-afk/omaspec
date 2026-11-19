//! Drawn words that read alike but are different things: a sequence
//! diagram's divider `== Payment ==` and its participant `Payment`.
//!
//! A label in the picture is matched to the source by its words and by
//! how many of them came before: the first `Payment` drawn is the first
//! written. That holds while every copy drawn is the same thing. Where
//! the drawer ties a copy to its line ([`omaspec_uml::map::Map::tied`]),
//! that copy and that line belong to each other, and the others are
//! counted without them.
//!
//! On one line, what the line itself draws stands after what it names
//! (`User -> Login : Login`): of the words written there, the last are
//! the copies tied to it, in the order drawn, and those before them are
//! mentions, counted.

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
    /// Each line that writes it, and the byte on it where each writing
    /// starts, in order: at least one.
    pub written: Vec<(usize, Vec<usize>)>,
}

impl Copies {
    /// The line the words of the tied `copy` are written on: the line its
    /// item begins on, or, in a block that begins there (a note up to its
    /// `end note`), one of the lines it is drawn on after it. `None` for a
    /// copy that is not tied, or whose words the count does not find
    /// there.
    fn home(&self, copy: usize) -> Option<usize> {
        let Drawn::Tied { line, span } = self.drawn.get(copy).copied()? else {
            return None;
        };
        let on = |row: usize| self.written.iter().any(|(at, _)| *at == row).then_some(row);
        on(line).or_else(|| (line + 1..=line + span).find_map(on))
    }

    /// The copies tied to `row`, in the order drawn.
    fn tied_on(&self, row: usize) -> Vec<usize> {
        (0..self.drawn.len())
            .filter(|&copy| self.home(copy) == Some(row))
            .collect()
    }

    /// Where the words start on `row`.
    fn starts(&self, row: usize) -> &[usize] {
        self.written
            .iter()
            .find(|(at, _)| *at == row)
            .map_or(&[], |(_, starts)| starts.as_slice())
    }

    /// How many of the writings on `row` are tied copies': the last ones.
    fn tied_count(&self, row: usize) -> usize {
        self.tied_on(row).len().min(self.starts(row).len())
    }

    /// The copies that are counted, in the order drawn.
    fn counted(&self) -> Vec<usize> {
        (0..self.drawn.len())
            .filter(|&copy| self.drawn[copy] == Drawn::Counted)
            .collect()
    }

    /// The copy the `nth` mention, at the byte `col` of the line `row`, is
    /// drawn as. Of the writings on a line, the last are the copies tied
    /// to it, in the order drawn. Any other mention is counted among the
    /// counted copies, without the tied writings before it, and a later
    /// one than there are copies keeps the last.
    pub fn copy_of(&self, nth: usize, row: usize, col: usize) -> usize {
        let here = self.tied_on(row);
        if !here.is_empty() {
            let starts = self.starts(row);
            // The writing the caret is on: the last that starts before it.
            let at = starts.iter().rposition(|start| *start <= col).unwrap_or(0);
            let first = starts.len().saturating_sub(here.len());
            if at >= first {
                return here[(at - first).min(here.len() - 1)];
            }
        }
        let tied_before: usize = self
            .written
            .iter()
            .filter(|(at, _)| *at < row)
            .map(|(at, _)| self.tied_count(*at))
            .sum();
        let before = nth.saturating_sub(tied_before);
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
            Drawn::Tied { line, .. } => {
                let Some(row) = self.home(copy) else {
                    return Some((line, 0));
                };
                let here = self.tied_on(row);
                let starts = self.starts(row);
                let nth = here.iter().position(|at| *at == copy).unwrap_or(0);
                let first = starts.len().saturating_sub(here.len());
                let col = starts.get(first + nth).or(starts.last()).copied();
                return Some((row, col.unwrap_or(0)));
            }
            Drawn::Counted => among(Drawn::Counted),
            // The first drawn again repeats the first counted.
            Drawn::Again => among(Drawn::Again).min(self.counted().len().saturating_sub(1)),
        };
        // The lines with a mention the tied copies leave: their first
        // writing.
        self.written
            .iter()
            .filter(|(row, starts)| starts.len() > self.tied_count(*row))
            .map(|(row, starts)| (*row, starts[0]))
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
        // Each line the count finds the words on, with every place they
        // stand there as words, as the count reads them.
        let lines: Vec<&str> = source.split('\n').collect();
        let written = locate::columns_of(source, text)
            .into_iter()
            .map(|(row, col)| {
                let mut starts: Vec<usize> = lines
                    .get(row)
                    .map(|line| locate::spans_on(line, text))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(from, _)| from)
                    .collect();
                if starts.is_empty() {
                    starts.push(col);
                }
                (row, starts)
            })
            .collect();
        Some(Copies { drawn, written })
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
        match self.copies(source, text) {
            Some(copies) => copies.copy_of(nth, row, col),
            None => nth,
        }
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
            written: vec![(10, vec![12]), (26, vec![3]), (27, vec![8]), (29, vec![2])],
        };
        // The divider's line is the divider; the declaration and every
        // other mention, before the divider or after it, is the head.
        assert_eq!(copies.copy_of(0, 10, 12), 0);
        assert_eq!(copies.copy_of(1, 26, 3), 2);
        assert_eq!(copies.copy_of(2, 27, 8), 0);
        assert_eq!(copies.copy_of(3, 29, 2), 0);
        assert_eq!(copies.copy_of(1, 20, 0), 0);
        // A click on the divider lands on its line; one on the head or on
        // the foot, which repeats it, on the declaration.
        assert_eq!(copies.written_at(2), Some((26, 3)));
        assert_eq!(copies.written_at(0), Some((10, 12)));
        assert_eq!(copies.written_at(1), Some((10, 12)));
        // Tied to a line the count finds no such words on: that line.
        let unread = Copies {
            drawn: vec![tied(5)],
            written: Vec::new(),
        };
        assert_eq!(unread.written_at(0), Some((5, 0)));
    }

    /// The words of a block are written on the lines after the one it
    /// begins on: a note up to its `end note`.
    #[test]
    fn a_block_s_words_are_written_after_the_line_it_begins_on() {
        let copies = Copies {
            drawn: vec![Drawn::Counted, Drawn::Tied { line: 4, span: 2 }],
            written: vec![(1, vec![12]), (6, vec![2]), (9, vec![0])],
        };
        assert_eq!(copies.written_at(1), Some((6, 2)));
        assert_eq!(copies.copy_of(1, 6, 2), 1);
        // The mention after the block is counted without it.
        assert_eq!(copies.copy_of(2, 9, 0), 0);
    }

    /// Two boxes that read alike are counted, the first drawn the first
    /// written.
    #[test]
    fn what_is_counted_is_counted_in_order() {
        let copies = Copies {
            drawn: vec![Drawn::Counted, Drawn::Counted],
            written: vec![(1, vec![0]), (4, vec![2]), (6, vec![2])],
        };
        assert_eq!(copies.copy_of(1, 4, 2), 1);
        assert_eq!(copies.copy_of(2, 6, 2), 1);
        assert_eq!(copies.written_at(1), Some((4, 2)));
    }

    /// `User -> Login : Login`: what the line draws is the last of the
    /// words on it; the name before it is a mention, counted, and a line
    /// that writes both still counts as the name's mention.
    #[test]
    fn what_a_line_draws_is_the_last_of_such_words_on_it() {
        let copies = Copies {
            // The participant's head, the message on line 2, a note on 3.
            drawn: vec![Drawn::Counted, tied(2), tied(3)],
            written: vec![(2, vec![8, 16]), (3, vec![7]), (5, vec![0])],
        };
        assert_eq!(copies.copy_of(0, 2, 8), 0);
        assert_eq!(copies.copy_of(1, 2, 16), 1);
        assert_eq!(copies.copy_of(1, 2, 20), 1);
        assert_eq!(copies.copy_of(2, 3, 7), 2);
        // Line 2 wrote it twice, once for the message, and line 3 once.
        assert_eq!(copies.copy_of(3, 5, 0), 0);
        assert_eq!(copies.written_at(0), Some((2, 8)));
        assert_eq!(copies.written_at(1), Some((2, 16)));
    }

    /// Two copies one line draws (`A[Edit] --> B[Edit]`, `"1" --> "1"`)
    /// are its writings in the order drawn.
    #[test]
    fn copies_one_line_draws_are_its_writings_in_order() {
        let copies = Copies {
            drawn: vec![tied(2), tied(2)],
            written: vec![(2, vec![4, 17])],
        };
        assert_eq!(copies.copy_of(0, 2, 4), 0);
        assert_eq!(copies.copy_of(1, 2, 17), 1);
        assert_eq!(copies.written_at(0), Some((2, 4)));
        assert_eq!(copies.written_at(1), Some((2, 17)));
    }
}
