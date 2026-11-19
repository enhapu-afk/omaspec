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
//! On its line, a tied copy is the writing at the byte the drawer gives
//! (a line's label between its ends, a multiplicity). One without it
//! closes its line (`User -> Login : Login`): such copies are the last
//! writings left there, in the order drawn. The writings no tied copy
//! takes are mentions, counted.

use iced::{Point, Rectangle, Size};
use omaspec_core::locate;
use omaspec_render::pick;

use super::Paper;

/// What a copy of a text drawn is to the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Drawn {
    /// One of those counted: the first drawn is the first written.
    Counted,
    /// Drawn for the item that begins on `line`, on `span` lines, from
    /// the byte `col` of it where the drawer knows that.
    Tied {
        line: usize,
        col: Option<usize>,
        span: usize,
    },
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

/// A writing: the index of its line in [`Copies::written`], and its own
/// among that line's.
type Writing = (usize, usize);

/// The writings the tied copies take, worked out once for a text.
struct Taken {
    /// For each copy drawn, the writing it is, if it is tied and found.
    of: Vec<Option<Writing>>,
    /// For each line in [`Copies::written`], which of its writings a tied
    /// copy took.
    by_line: Vec<Vec<bool>>,
}

impl Copies {
    /// The index in `written` of the line the words of the tied `copy`
    /// are written on, and the byte it gives there: the line its item
    /// begins on, or, in a block that begins there (a note up to its
    /// `end note`), one of the lines it is drawn on after it, where the
    /// byte says nothing. `None` for a copy that is not tied, or whose
    /// words the count does not find there.
    fn home(&self, copy: usize) -> Option<(usize, Option<usize>)> {
        let Drawn::Tied { line, col, span } = self.drawn.get(copy).copied()? else {
            return None;
        };
        let on = |row: usize| self.written.binary_search_by_key(&row, |(at, _)| *at).ok();
        on(line)
            .map(|at| (at, col))
            .or_else(|| (line + 1..=line + span).find_map(on).map(|at| (at, None)))
    }

    /// Which writing each tied copy is. One that gives its byte takes
    /// the writing that starts there, or the first after it; the others
    /// take the last writings left on their line, in the order drawn.
    fn taken(&self) -> Taken {
        let mut of = vec![None; self.drawn.len()];
        let mut by_line: Vec<Vec<bool>> = self
            .written
            .iter()
            .map(|(_, starts)| vec![false; starts.len()])
            .collect();
        let homes: Vec<Option<(usize, Option<usize>)>> =
            (0..self.drawn.len()).map(|copy| self.home(copy)).collect();
        for (copy, home) in homes.iter().enumerate() {
            let Some((at, Some(col))) = *home else {
                continue;
            };
            let starts = &self.written[at].1;
            let free = |index: &usize| !by_line[at][*index];
            let found = (0..starts.len())
                .filter(free)
                .find(|&index| starts[index] == col)
                .or_else(|| {
                    (0..starts.len())
                        .filter(free)
                        .find(|&index| starts[index] >= col)
                });
            if let Some(index) = found {
                by_line[at][index] = true;
                of[copy] = Some((at, index));
            }
        }
        let mut rest: Vec<Vec<usize>> = vec![Vec::new(); self.written.len()];
        for (copy, home) in homes.iter().enumerate() {
            if let Some((at, _)) = home
                && of[copy].is_none()
            {
                rest[*at].push(copy);
            }
        }
        for (at, copies) in rest.into_iter().enumerate() {
            let free: Vec<usize> = (0..by_line[at].len())
                .filter(|&index| !by_line[at][index])
                .collect();
            let skip = free.len().saturating_sub(copies.len());
            for (copy, index) in copies.into_iter().zip(&free[skip..]) {
                by_line[at][*index] = true;
                of[copy] = Some((at, *index));
            }
        }
        Taken { of, by_line }
    }

    /// The copies that are counted, in the order drawn.
    fn counted(&self) -> Vec<usize> {
        (0..self.drawn.len())
            .filter(|&copy| self.drawn[copy] == Drawn::Counted)
            .collect()
    }

    /// The copy the `nth` mention, at the byte `col` of the line `row`, is
    /// drawn as: the tied copy that took the writing the caret is on, or
    /// else one counted, without the writings tied copies took before it.
    /// A later mention than there are copies keeps the last.
    pub fn copy_of(&self, nth: usize, row: usize, col: usize) -> usize {
        let taken = self.taken();
        if let Ok(at) = self.written.binary_search_by_key(&row, |(line, _)| *line)
            && let Some(index) = self.written[at].1.iter().rposition(|start| *start <= col)
            && let Some(copy) = taken.of.iter().position(|of| *of == Some((at, index)))
        {
            return copy;
        }
        let before = taken
            .of
            .iter()
            .flatten()
            .filter(|(at, index)| {
                let (line, starts) = &self.written[*at];
                *line < row || (*line == row && starts[*index] < col)
            })
            .count();
        let counted = self.counted();
        match counted.get(nth.saturating_sub(before)).or(counted.last()) {
            Some(&copy) => copy,
            None => nth,
        }
    }

    /// Where the `copy` drawn is written: the writing it took, or its
    /// item's line where the count does not find its words; for one that
    /// is counted, the line with a writing no tied copy took that many of
    /// the counted come before. One drawn again is written where the one
    /// it repeats is.
    pub fn written_at(&self, copy: usize) -> Option<(usize, usize)> {
        let among = |kind: Drawn| {
            self.drawn
                .iter()
                .take(copy)
                .filter(|drawn| **drawn == kind)
                .count()
        };
        let taken = self.taken();
        let before = match self.drawn.get(copy).copied()? {
            Drawn::Tied { line, col, .. } => {
                return Some(match taken.of[copy] {
                    Some((at, index)) => (self.written[at].0, self.written[at].1[index]),
                    None => (line, col.unwrap_or(0)),
                });
            }
            Drawn::Counted => among(Drawn::Counted),
            // The first drawn again repeats the first counted.
            Drawn::Again => among(Drawn::Again).min(self.counted().len().saturating_sub(1)),
        };
        self.written
            .iter()
            .zip(&taken.by_line)
            .filter_map(|((row, starts), took)| {
                let index = took.iter().position(|took| !took)?;
                Some((*row, starts[index]))
            })
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
                        col: tied.col,
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
        Drawn::Tied {
            line,
            col: None,
            span: 1,
        }
    }

    fn tied_at(line: usize, col: usize) -> Drawn {
        Drawn::Tied {
            line,
            col: Some(col),
            span: 1,
        }
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
            drawn: vec![
                Drawn::Counted,
                Drawn::Tied {
                    line: 4,
                    col: None,
                    span: 2,
                },
            ],
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

    /// Two copies one line draws that close it are its last writings in
    /// the order drawn.
    #[test]
    fn copies_that_close_their_line_are_its_writings_in_order() {
        let copies = Copies {
            drawn: vec![tied(2), tied(2)],
            written: vec![(2, vec![4, 17])],
        };
        assert_eq!(copies.copy_of(0, 2, 4), 0);
        assert_eq!(copies.copy_of(1, 2, 17), 1);
        assert_eq!(copies.written_at(0), Some((2, 4)));
        assert_eq!(copies.written_at(1), Some((2, 17)));
    }

    /// Copies that say where they stand take those writings, in whatever
    /// order they are drawn (`B[Edit] --> A[Edit]` with A drawn first, a
    /// line turned `-up->` between two multiplicities).
    #[test]
    fn a_copy_that_says_where_it_stands_is_the_writing_there() {
        let copies = Copies {
            drawn: vec![tied_at(2, 17), tied_at(2, 4)],
            written: vec![(2, vec![4, 17])],
        };
        assert_eq!(copies.copy_of(0, 2, 4), 1);
        assert_eq!(copies.copy_of(1, 2, 18), 0);
        assert_eq!(copies.written_at(0), Some((2, 17)));
        assert_eq!(copies.written_at(1), Some((2, 4)));
    }

    /// `A -->|End| End`: the label stands between the line's ends, before
    /// the name the line mentions. A caret on the name is the node, which
    /// is counted, and one on the label the label; and a click on each
    /// lands on its own words.
    #[test]
    fn a_label_amid_the_names_is_where_the_drawer_says() {
        let copies = Copies {
            // The node `End` (it shows its id), then the label.
            drawn: vec![Drawn::Counted, tied_at(1, 7)],
            written: vec![(1, vec![7, 12]), (3, vec![2])],
        };
        assert_eq!(copies.copy_of(0, 1, 7), 1);
        assert_eq!(copies.copy_of(1, 1, 12), 0);
        assert_eq!(copies.copy_of(2, 3, 2), 0);
        assert_eq!(copies.written_at(1), Some((1, 7)));
        assert_eq!(copies.written_at(0), Some((1, 12)));
    }

    /// A caret on a name that stands before every writing of these words
    /// (an alias, `L -> DB : Login` for `participant "Login" as L`) is a
    /// mention, not the message the line closes with.
    #[test]
    fn a_caret_before_the_words_is_a_mention() {
        let copies = Copies {
            drawn: vec![Drawn::Counted, tied(2)],
            written: vec![(1, vec![13]), (2, vec![10])],
        };
        assert_eq!(copies.copy_of(1, 2, 0), 0);
        assert_eq!(copies.copy_of(1, 2, 10), 1);
    }
}
