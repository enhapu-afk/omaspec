//! A lane's marble string read as the proposal's reference reads it
//! (`marbleTimeline.ts`), with RxJS's `TestScheduler.run()` meaning for
//! every marble RxJS knows and the proposal's spans and breaks besides.
//!
//! - leading blanks start the lane later, a column each; other blanks
//!   take no time
//! - `-` time passes, `/` time breaks, `9ms`, `1.4s`, `2m` time passes off
//!   the scale, in one column
//! - `(abc)` holds values of one frame; the group takes a column for each
//!   of its characters
//! - `^` subscribes, `!` unsubscribes and cancels an open span
//! - `[` with `=` or `~` after it opens a span, `]` closes it, `>` leaves
//!   it running
//! - `|` completes the lane, `#` errors it; what follows takes time and
//!   is not drawn
//! - any other character is a value; it closes an open span and opens
//!   one where a fill follows it
//! - a lane of only `-`, `^`, `!` and times is a subscription, one span

use super::{End, Item};

/// A column of a lane: its character, and its byte on the lane's line
/// (none for the blanks a lane that starts later is given).
pub(super) type Cell = (char, Option<usize>);

/// What a lane's string says.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Read {
    pub items: Vec<Item>,
    pub segments: Vec<(usize, usize)>,
    pub breaks: Vec<usize>,
    pub columns: usize,
}

/// The characters that fill a span; each is a character a label names.
pub(super) fn fills(c: char) -> bool {
    c == '=' || c == '~'
}

/// RxJS's time progression at `index`: a number and `ms`, `s` or `m`,
/// after a blank or at the start, and before a blank or the end. How many
/// characters it takes.
pub(super) fn duration(cells: &[Cell], index: usize) -> Option<usize> {
    if index > 0 && !cells[index - 1].0.is_whitespace() {
        return None;
    }
    let digits = |from: usize| {
        cells[from..]
            .iter()
            .take_while(|(c, _)| c.is_ascii_digit())
            .count()
    };
    let mut at = index + digits(index);
    if at == index {
        return None;
    }
    if cells.get(at).is_some_and(|(c, _)| *c == '.') {
        let more = digits(at + 1);
        if more == 0 {
            return None;
        }
        at += 1 + more;
    }
    let unit = match (cells.get(at).map(|c| c.0), cells.get(at + 1).map(|c| c.0)) {
        (Some('m'), Some('s')) => 2,
        (Some('s' | 'm'), _) => 1,
        _ => return None,
    };
    // `1ms` is read as `ms` where it can be, as the reference's pattern
    // tries `ms` first, and as `m` where `s` is no unit's end.
    for unit in [unit, 1] {
        let end = at + unit;
        if cells.get(end).is_none_or(|(c, _)| c.is_whitespace()) {
            return Some(end - index);
        }
    }
    None
}

/// The lane `cells`, measured from the shared origin, as an error's words
/// where Mermaid refuses it.
pub(super) fn read(cells: &[Cell]) -> Result<Read, String> {
    let mut items: Vec<Item> = Vec::new();
    let mut segments = Vec::new();
    let mut breaks = Vec::new();
    // The open span: its fill and its first column.
    let mut span: Option<(char, usize)> = None;
    let mut segment: Option<(usize, usize)> = None;
    // The open group: its column, and its values' places among the items.
    let mut group: Option<Group> = None;
    let mut ended = false;
    let mut started = false;
    let mut col = 0;
    let close = |items: &mut Vec<Item>, span: &mut Option<(char, usize)>, at: usize, end: End| {
        if let Some((symbol, from)) = span.take() {
            items.push(Item::Span {
                symbol,
                from,
                to: at,
                end,
                subscription: false,
            });
        }
    };
    let extend = |segment: &mut Option<(usize, usize)>, at: usize| {
        let s = segment.get_or_insert((at, at));
        s.1 = at;
    };
    let mut index = 0;
    while index < cells.len() {
        let (c, byte) = cells[index];
        if c.is_whitespace() {
            if !started {
                if c != ' ' {
                    return Err(
                        "a tab before a lane's first marble: columns line up with spaces only"
                            .into(),
                    );
                }
                col += 1;
            }
            index += 1;
            continue;
        }
        started = true;
        if let Some(length) = duration(cells, index) {
            if !ended {
                extend(&mut segment, col);
                let text = cells[index..index + length].iter().map(|c| c.0).collect();
                items.push(Item::Time { col, text, byte });
            }
            col += 1;
            index += length;
            continue;
        }
        let next = cells.get(index + 1).map(|c| c.0);
        index += 1;
        if c == '(' {
            if group.is_some() {
                return Err("a group inside a group: groups do not nest".into());
            }
            group = Some(Group {
                col,
                events: Vec::new(),
                terminal: false,
            });
            if !ended {
                extend(&mut segment, col);
            }
            col += 1;
            continue;
        }
        if c == ')' {
            let Some(open) = group.take() else {
                return Err("`)` closes a group, and no group is open".into());
            };
            let size = open.events.len();
            for (place, &at) in open.events.iter().enumerate() {
                if let Item::Event { stack, .. } = &mut items[at] {
                    *stack = Some((place, size));
                }
            }
            if size > 0 || open.terminal {
                items.push(Item::Group {
                    col: open.col,
                    to: col,
                    size,
                });
            }
            if !ended {
                extend(&mut segment, col);
            }
            col += 1;
            continue;
        }
        if ended {
            col += 1;
            continue;
        }
        let at = group.as_ref().map_or(col, |g| g.col);
        extend(&mut segment, col);
        col += 1;
        match c {
            '-' => continue,
            '/' => {
                breaks.push(at);
                continue;
            }
            '^' => {
                items.push(Item::Subscribe { col: at });
                continue;
            }
            '!' => {
                close(&mut items, &mut span, at, End::Cancel);
                items.push(Item::Cancel { col: at });
                continue;
            }
            '|' | '#' => {
                let error = c == '#';
                close(
                    &mut items,
                    &mut span,
                    at,
                    if error { End::Cancel } else { End::Close },
                );
                let grouped = group.is_some();
                items.push(if error {
                    Item::Error { col: at, grouped }
                } else {
                    Item::Complete { col: at, grouped }
                });
                if let Some(group) = &mut group {
                    group.terminal = true;
                }
                ended = true;
                if let Some(s) = &mut segment {
                    s.1 = at;
                }
                segments.extend(segment.take());
                continue;
            }
            '[' if group.is_none() && next.is_some_and(fills) => {
                close(&mut items, &mut span, at, End::Close);
                span = next.map(|fill| (fill, at));
                continue;
            }
            ']' | '>' if span.is_some() => {
                let end = if c == ']' { End::Close } else { End::Open };
                close(&mut items, &mut span, at, end);
                continue;
            }
            c if span.is_some() && fills(c) => continue,
            _ => {}
        }
        close(&mut items, &mut span, at, End::Close);
        items.push(Item::Event {
            symbol: c,
            col: at,
            stack: None,
            byte,
        });
        match &mut group {
            Some(group) => group.events.push(items.len() - 1),
            None => {
                if let Some(fill) = next.filter(|&n| fills(n)) {
                    span = Some((fill, at));
                }
            }
        }
    }
    if group.is_some() {
        return Err("a group opened and never closed with `)`".into());
    }
    close(&mut items, &mut span, col.saturating_sub(1), End::Open);
    segments.extend(segment.take());
    if let Some(items) = subscription(cells, &items, &segments, col) {
        return Ok(Read {
            items,
            segments: Vec::new(),
            breaks,
            columns: col,
        });
    }
    Ok(Read {
        items,
        segments,
        breaks,
        columns: col,
    })
}

/// A `( )` group while it is read.
struct Group {
    col: usize,
    events: Vec<usize>,
    terminal: bool,
}

/// A lane in RxJS's subscription syntax (`--^---!`, `^ 32ms !`): only
/// `-`, `^`, `!`, times and parentheses. It is one span from `^` to `!`.
fn subscription(
    cells: &[Cell],
    items: &[Item],
    segments: &[(usize, usize)],
    end: usize,
) -> Option<Vec<Item>> {
    if !items
        .iter()
        .any(|item| matches!(item, Item::Subscribe { .. } | Item::Cancel { .. }))
    {
        return None;
    }
    let mut index = 0;
    while index < cells.len() {
        if let Some(length) = duration(cells, index) {
            index += length;
            continue;
        }
        let c = cells[index].0;
        if !c.is_whitespace() && !matches!(c, '-' | '^' | '!' | '(' | ')') {
            return None;
        }
        index += 1;
    }
    let subscribed = items.iter().find_map(|item| match item {
        Item::Subscribe { col } => Some(*col),
        _ => None,
    });
    let cancelled = items.iter().find_map(|item| match item {
        Item::Cancel { col } => Some(*col),
        _ => None,
    });
    let from = subscribed
        .or_else(|| segments.first().map(|s| s.0))
        .unwrap_or(0);
    let to = cancelled.unwrap_or_else(|| from.max(end.saturating_sub(1)));
    let mut out = vec![Item::Span {
        symbol: '^',
        from,
        to,
        end: if cancelled.is_some() {
            End::Cancel
        } else {
            End::Open
        },
        subscription: true,
    }];
    out.extend(
        items
            .iter()
            .filter(|item| matches!(item, Item::Cancel { .. } | Item::Time { .. }))
            .cloned(),
    );
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(text: &str) -> Vec<Cell> {
        text.chars()
            .enumerate()
            .map(|(i, c)| (c, Some(i)))
            .collect()
    }

    fn lane(text: &str) -> Read {
        read(&cells(text)).unwrap()
    }

    fn event(symbol: char, col: usize) -> Item {
        Item::Event {
            symbol,
            col,
            stack: None,
            byte: Some(col),
        }
    }

    /// What each item is, a value by its character.
    fn kinds(read: &Read) -> Vec<String> {
        read.items
            .iter()
            .map(|item| match item {
                Item::Event { symbol, .. } => symbol.to_string(),
                Item::Span { .. } => "span".into(),
                Item::Complete { .. } => "|".into(),
                Item::Error { .. } => "#".into(),
                Item::Cancel { .. } => "!".into(),
                Item::Subscribe { .. } => "^".into(),
                Item::Group { .. } => "group".into(),
                Item::Time { text, .. } => text.clone(),
            })
            .collect()
    }

    #[test]
    fn values_time_and_completion() {
        let read = lane("-a--b-|");
        assert_eq!(
            read.items,
            [
                event('a', 1),
                event('b', 4),
                Item::Complete {
                    col: 6,
                    grouped: false
                }
            ]
        );
        assert_eq!(read.segments, [(0, 6)]);
    }

    #[test]
    fn leading_blanks_start_later_and_other_blanks_take_no_time() {
        let read = lane("  -a - b -    c |");
        let cols: Vec<usize> = read
            .items
            .iter()
            .map(|item| match item {
                Item::Event { col, .. } | Item::Complete { col, .. } => *col,
                _ => usize::MAX,
            })
            .collect();
        assert_eq!(cols, [3, 5, 7, 8]);
        assert_eq!((read.segments.clone(), read.columns), (vec![(2, 8)], 9));
    }

    #[test]
    fn a_group_stacks_in_one_frame_and_takes_its_characters() {
        let read = lane("-(abc)-d");
        assert_eq!(
            read.items[1],
            Item::Event {
                symbol: 'b',
                col: 1,
                stack: Some((1, 3)),
                byte: Some(3)
            }
        );
        assert_eq!(
            read.items[3],
            Item::Group {
                col: 1,
                to: 5,
                size: 3
            }
        );
        assert_eq!((read.items[4].clone(), read.columns), (event('d', 7), 8));
        // A completion inside a group is in its frame.
        assert_eq!(
            lane("--(a|)").items[1],
            Item::Complete {
                col: 2,
                grouped: true
            }
        );
        assert_eq!(
            lane("(#)").items,
            [
                Item::Error {
                    col: 0,
                    grouped: true
                },
                Item::Group {
                    col: 0,
                    to: 2,
                    size: 0
                }
            ]
        );
    }

    #[test]
    fn times_take_a_column_each_and_keep_their_words() {
        let read = lane("10.2ms a 1.2s b 1m c|");
        assert_eq!(kinds(&read), ["10.2ms", "a", "1.2s", "b", "1m", "c", "|"]);
        assert_eq!(read.columns, 7);
        // Not a whole time: values.
        assert_eq!(kinds(&lane("-1ms a1s")), ["1", "m", "s", "a", "1", "s"]);
    }

    #[test]
    fn a_subscription_lane_is_one_span() {
        let read = lane("--^---!-");
        assert_eq!(
            read.items,
            [
                Item::Span {
                    symbol: '^',
                    from: 2,
                    to: 6,
                    end: End::Cancel,
                    subscription: true
                },
                Item::Cancel { col: 6 }
            ]
        );
        assert!(read.segments.is_empty());
        assert_eq!(kinds(&lane("^ 32ms !")), ["span", "32ms", "!"]);
        assert_eq!(
            lane("---^").items,
            [Item::Span {
                symbol: '^',
                from: 3,
                to: 3,
                end: End::Open,
                subscription: true
            }]
        );
    }

    #[test]
    fn spans_open_close_cancel_and_run_on() {
        let span = |symbol, from, to, end| Item::Span {
            symbol,
            from,
            to,
            end,
            subscription: false,
        };
        assert_eq!(lane("-[~~~]-").items, [span('~', 1, 5, End::Close)]);
        assert_eq!(lane("-[== =]-").items, [span('=', 1, 5, End::Close)]);
        assert_eq!(
            lane("-S~~E-").items,
            [event('S', 1), span('~', 1, 4, End::Close), event('E', 4)]
        );
        let cancelled = lane("[===!--");
        assert_eq!(
            cancelled.items,
            [span('=', 0, 4, End::Cancel), Item::Cancel { col: 4 }]
        );
        assert_eq!(cancelled.segments, [(0, 6)]);
        assert_eq!(lane("[==>").items, [span('=', 0, 3, End::Open)]);
        assert_eq!(lane("[===").items, [span('=', 0, 3, End::Open)]);
        // Where no span opens or closes, they are values.
        assert_eq!(kinds(&lane("-]->-=-[-")), ["]", ">", "=", "["]);
    }

    #[test]
    fn nothing_is_drawn_after_the_end_and_time_still_counts() {
        let read = lane("--a--#--d--|");
        assert_eq!(
            read.items,
            [
                event('a', 2),
                Item::Error {
                    col: 5,
                    grouped: false
                }
            ]
        );
        assert_eq!((read.segments.clone(), read.columns), (vec![(0, 5)], 12));
        assert_eq!(lane("-a-/-b").breaks, [3]);
    }

    #[test]
    fn what_mermaid_refuses() {
        for (text, said) in [
            ("-(ab", "never closed"),
            ("-ab)", "no group is open"),
            ("(a(b))", "do not nest"),
            ("\t-a", "a tab"),
        ] {
            let error = read(&cells(text)).unwrap_err();
            assert!(error.contains(said), "{text}: {error}");
        }
    }
}
