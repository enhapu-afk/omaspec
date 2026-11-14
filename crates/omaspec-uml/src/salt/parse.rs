//! Reading Salt: the lines are cut into blocks that open with `{` and
//! close with `}`, on lines of their own, around a row, or in a row's
//! last cell, and rows of cells apart by `|`.

use super::{Diagram, Item, Lines, Row, Rule, Widget};
use crate::Declined;
use crate::common::{Frame, squeezed_eq};
use crate::source::Line;

/// How deep blocks may stand in each other.
const MAX_DEPTH: usize = 64;
/// Stands in a row's text for the block that opens in that cell.
const SLOT: &str = "\u{1}";

/// Whether `lines` are Salt in an `@startuml`: the first says `salt`.
pub fn is_salt(lines: &[Line]) -> bool {
    lines
        .first()
        .is_some_and(|line| squeezed_eq(line.text, "salt"))
}

/// A piece of the body: a block opened, one closed, or a row's text.
#[derive(Debug, Clone, PartialEq)]
enum Unit {
    Opens {
        kind: char,
        /// A tree's lines, as a grid's kind names them: `{T#`.
        lines: char,
        title: Option<String>,
        no: usize,
    },
    Closes(usize),
    Row(String, usize),
    /// The rest of a row after the block in it closed: `} | 3 files`.
    More(String, usize),
}

pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut frame = Frame::default();
    let mut units = Vec::new();
    let mut at = 0;
    let mut started = false;
    while at < lines.len() {
        let Line { no, text, .. } = lines[at];
        if !started && squeezed_eq(text, "salt") {
            at += 1;
            continue;
        }
        // The frame's lines stand outside the window.
        if (!started || units_closed(&units))
            && let Some(next) = frame.line(lines, at)?
        {
            at = next;
            continue;
        }
        started = true;
        cut(text, no, &mut units)?;
        at += 1;
    }
    let mut cursor = 0;
    let root = match units.first() {
        Some(Unit::Opens { .. }) => block(&units, &mut cursor, 0)?,
        Some(Unit::Row(_, no) | Unit::More(_, no) | Unit::Closes(no)) => {
            return Err(Declined::at(*no, "a Salt window starts with `{`"));
        }
        None => {
            return Err(Declined {
                line: None,
                reason: "a Salt diagram needs a window: `{ … }`".into(),
                refused: true,
            });
        }
    };
    if let Some(unit) = units.get(cursor) {
        let no = match unit {
            Unit::Opens { no, .. } | Unit::Closes(no) | Unit::Row(_, no) | Unit::More(_, no) => *no,
        };
        return Err(Declined::at(no, "a Salt diagram has one window"));
    }
    Ok(Diagram { root, frame })
}

/// Whether every block opened so far is closed again.
fn units_closed(units: &[Unit]) -> bool {
    let opened = units
        .iter()
        .filter(|u| matches!(u, Unit::Opens { .. }))
        .count();
    let closed = units
        .iter()
        .filter(|u| matches!(u, Unit::Closes(_)))
        .count();
    !units.is_empty() && opened == closed
}

/// A line cut into units.
fn cut(text: &str, no: usize, units: &mut Vec<Unit>) -> Result<(), Declined> {
    let mut rest = text.trim();
    // The block in a row closes, and the row goes on.
    if let Some(after) = rest.strip_prefix('}')
        && after.trim_start().starts_with('|')
    {
        units.push(Unit::Closes(no));
        return row_text(after.trim(), no, units, true);
    }
    if let Some(after) = rest.strip_prefix('{') {
        rest = opener(after, no, units)?;
    }
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(());
    }
    match closing(rest) {
        Some(before) => {
            if !before.trim().is_empty() {
                units.push(Unit::Row(before.trim().to_owned(), no));
            }
            units.push(Unit::Closes(no));
            Ok(())
        }
        None => row_text(rest, no, units, false),
    }
}

/// The block that opens after a `{`: its kind, a tree's lines and a
/// group's title; the text after them.
fn opener<'a>(after: &'a str, no: usize, units: &mut Vec<Unit>) -> Result<&'a str, Declined> {
    let kind = after
        .chars()
        .next()
        .filter(|c| matches!(c, '+' | '#' | '!' | '-' | '^' | '/' | '*' | 'T' | 'S'))
        .unwrap_or(' ');
    let mut rest = if kind == ' ' {
        after
    } else {
        &after[kind.len_utf8()..]
    };
    let mut lines = ' ';
    if kind == 'T'
        && let Some(c) = rest
            .chars()
            .next()
            .filter(|c| matches!(c, '+' | '#' | '!' | '-'))
    {
        lines = c;
        rest = &rest[c.len_utf8()..];
    }
    let mut title = None;
    if kind == '^' {
        let trimmed = rest.trim_start();
        if let Some(quoted) = trimmed.strip_prefix('"') {
            let end = quoted
                .find('"')
                .ok_or_else(|| Declined::at(no, "a group's title needs its closing quote"))?;
            title = Some(quoted[..end].to_owned());
            rest = &quoted[end + 1..];
        }
    }
    units.push(Unit::Opens {
        kind,
        lines,
        title,
        no,
    });
    Ok(rest)
}

/// A row, or the rest of one (`more`). A block that opens in its last
/// cell, `Files | {T`, stands in it as a slot, and opens.
fn row_text(text: &str, no: usize, units: &mut Vec<Unit>, more: bool) -> Result<(), Declined> {
    let all = cells(text);
    let opens = all
        .last()
        .filter(|last| all.len() > 1 && last.trim_start().starts_with('{') && !last.contains('}'))
        .copied();
    let written = match opens {
        Some(last) => format!("{}{SLOT}", &text[..text.len() - last.len()]),
        None => text.to_owned(),
    };
    units.push(if more {
        Unit::More(written, no)
    } else {
        Unit::Row(written, no)
    });
    if let Some(last) = opens {
        let rest = opener(&last.trim_start()[1..], no, units)?;
        if !rest.trim().is_empty() {
            units.push(Unit::Row(rest.trim().to_owned(), no));
        }
    }
    Ok(())
}

/// The text before a `}` that ends `text`, outside quotes.
fn closing(text: &str) -> Option<&str> {
    let before = text.strip_suffix('}')?;
    let quotes = before.matches('"').count();
    (quotes % 2 == 0).then_some(before)
}

/// The block that opens at `units[*at]`, read up to its close.
fn block(units: &[Unit], at: &mut usize, depth: usize) -> Result<Widget, Declined> {
    let Some(Unit::Opens {
        kind,
        lines,
        title,
        no,
    }) = units.get(*at).cloned()
    else {
        return Err(Declined {
            line: None,
            reason: "a Salt block opens with `{`".into(),
            refused: true,
        });
    };
    if depth > MAX_DEPTH {
        return Err(Declined::at(no, "Salt blocks nest too deep"));
    }
    *at += 1;
    let mut rows: Vec<(String, usize)> = Vec::new();
    // The blocks inside, each in its row's slot: a block on a line of its
    // own starts a row of its own, which may go on after its close.
    let mut slotted: Vec<(usize, Widget)> = Vec::new();
    let mut open_slots = 0;
    loop {
        match units.get(*at).cloned() {
            None => return Err(Declined::at(no, "this block has no `}`")),
            Some(Unit::Closes(_)) => {
                *at += 1;
                break;
            }
            Some(Unit::Row(text, line)) => {
                open_slots = text.matches(SLOT).count();
                rows.push((text, line));
                *at += 1;
            }
            Some(Unit::More(text, line)) => {
                let Some((row, _)) = rows.last_mut() else {
                    return Err(Declined::at(line, "this `}` closes no block in a row"));
                };
                open_slots += text.matches(SLOT).count();
                row.push_str(&text);
                *at += 1;
            }
            Some(Unit::Opens { .. }) => {
                if open_slots == 0 {
                    rows.push((SLOT.to_owned(), no));
                    open_slots = 1;
                }
                let inner = block(units, at, depth + 1)?;
                open_slots -= 1;
                slotted.push((rows.len() - 1, inner));
            }
        }
    }
    if !slotted.is_empty() && matches!(kind, '/' | '*' | 'T') {
        return Err(Declined::at(
            no,
            "a block inside tabs, a menu or a tree is not supported yet",
        ));
    }
    match kind {
        '/' => Ok(Widget::Tabs(all_cells(&rows))),
        '*' => Ok(Widget::Menu(
            rows.first()
                .map(|(text, _)| {
                    cells(text)
                        .into_iter()
                        .map(|c| c.trim().to_owned())
                        .collect()
                })
                .unwrap_or_default(),
        )),
        'T' => Ok(Widget::Tree {
            lines: lines_of(lines),
            items: rows.iter().map(|(text, _)| item(text)).collect(),
        }),
        'S' => Err(Declined::at(
            no,
            "a scroll pane (`{S`) is not supported yet",
        )),
        _ => {
            let mut grid = Vec::with_capacity(rows.len());
            let mut slotted = slotted.into_iter().peekable();
            for (i, (text, line)) in rows.iter().enumerate() {
                let mut blocks = Vec::new();
                while let Some((_, widget)) = slotted.next_if(|(at, _)| *at == i) {
                    blocks.push(widget);
                }
                grid.push(row(text, *line, blocks)?);
            }
            let body = Widget::Grid {
                lines: lines_of(kind),
                rows: grid,
            };
            Ok(match (kind, title) {
                ('^', title) => Widget::Group {
                    title: title.unwrap_or_default(),
                    body: Box::new(body),
                },
                _ => body,
            })
        }
    }
}

/// The lines a grid's kind, or a tree's, names.
fn lines_of(kind: char) -> Lines {
    match kind {
        '+' => Lines::Frame,
        '#' => Lines::All,
        '!' => Lines::Columns,
        '-' => Lines::Rows,
        _ => Lines::None,
    }
}

/// A tree's item: `++ Europe | 744` stands at 2, with a cell beside it.
fn item(text: &str) -> Item {
    let mut all = cells(text).into_iter();
    let first = all.next().unwrap_or_default();
    Item {
        depth: first.chars().take_while(|c| *c == '+').count().max(1),
        label: first.trim_start_matches('+').trim().to_owned(),
        cells: all.map(|cell| cell.trim().to_owned()).collect(),
    }
}

/// Every cell of `rows`, in order.
fn all_cells(rows: &[(String, usize)]) -> Vec<String> {
    rows.iter()
        .flat_map(|(text, _)| cells(text))
        .map(|cell| cell.trim().to_owned())
        .collect()
}

/// One row: a rule, or its cells; `blocks` stand in its slots.
fn row(text: &str, no: usize, blocks: Vec<Widget>) -> Result<Row, Declined> {
    let rule = match text.trim() {
        "--" => Some(Rule::Line),
        "==" => Some(Rule::Double),
        ".." => Some(Rule::Dotted),
        "~~" => Some(Rule::Dashed),
        _ => None,
    };
    if let Some(rule) = rule {
        return Ok(Row::Rule(rule));
    }
    let mut blocks = blocks.into_iter();
    cells(text)
        .into_iter()
        .map(|cell| {
            if cell.trim() == SLOT {
                blocks
                    .next()
                    .ok_or_else(|| Declined::at(no, "this row's block has no `}`"))
            } else {
                widget(cell, no)
            }
        })
        .collect::<Result<_, _>>()
        .map(Row::Cells)
}

/// The widget a cell's text writes.
fn widget(text: &str, no: usize) -> Result<Widget, Declined> {
    let text = text.trim();
    if text.is_empty() || text == "." || text == "*" {
        return Ok(Widget::Empty);
    }
    let lower = text.to_ascii_lowercase();
    for (mark, on) in [("[x]", true), ("[]", false), ("[ ]", false)] {
        if lower.starts_with(mark) {
            return Ok(Widget::Check {
                on,
                label: text[mark.len()..].trim().to_owned(),
            });
        }
    }
    for (mark, on) in [("(x)", true), ("()", false), ("( )", false)] {
        if lower.starts_with(mark) {
            return Ok(Widget::Radio {
                on,
                label: text[mark.len()..].trim().to_owned(),
            });
        }
    }
    if let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        return Ok(Widget::Button(inner.trim().to_owned()));
    }
    if let Some(inner) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        return Ok(Widget::Field(inner.trim_end().to_owned()));
    }
    if let Some(inner) = text.strip_prefix('^').and_then(|t| t.strip_suffix('^')) {
        return Ok(Widget::Drop(inner.trim().to_owned()));
    }
    if text.starts_with('{') {
        return Err(Declined::at(
            no,
            "a block inside a row is not supported yet; give it a row of its own",
        ));
    }
    Ok(Widget::Text(text.to_owned()))
}

/// The cells of a row, apart by `|` outside quotes, brackets and tags.
fn cells(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut quoted, mut square, mut angle) = (false, 0usize, 0usize);
    let mut start = 0;
    for (i, c) in text.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '[' if !quoted => square += 1,
            ']' if !quoted => square = square.saturating_sub(1),
            '<' if !quoted => angle += 1,
            '>' if !quoted => angle = angle.saturating_sub(1),
            '|' if !quoted && square == 0 && angle == 0 => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<Line<'_>> {
        text.lines()
            .enumerate()
            .map(|(no, text)| Line {
                no,
                text: text.trim(),
                indent: 0,
                blank_before: 0,
            })
            .collect()
    }

    #[test]
    fn a_window_of_widgets_is_read() {
        let d = parse(&lines(
            "{+\nLogin | \"MyName  \"\n[X] Remember | () Never\n--\n{^\"Box\"\n  Plan | ^Pro^\n}\n{/ General | Fonts }\n{T\n+ a\n++ b\n}\n[Cancel] | [ OK ] }",
        ))
        .unwrap();
        let Widget::Grid { lines, rows } = d.root else {
            panic!("a grid");
        };
        assert_eq!(lines, Lines::Frame);
        assert_eq!(
            rows[0],
            Row::Cells(vec![
                Widget::Text("Login".into()),
                Widget::Field("MyName".into())
            ])
        );
        assert_eq!(
            rows[1],
            Row::Cells(vec![
                Widget::Check {
                    on: true,
                    label: "Remember".into()
                },
                Widget::Radio {
                    on: false,
                    label: "Never".into()
                }
            ])
        );
        assert_eq!(rows[2], Row::Rule(Rule::Line));
        let Row::Cells(group) = &rows[3] else {
            panic!()
        };
        assert!(matches!(&group[0], Widget::Group { title, .. } if title == "Box"));
        assert_eq!(
            rows[4],
            Row::Cells(vec![Widget::Tabs(vec!["General".into(), "Fonts".into()])])
        );
        assert_eq!(
            rows[5],
            Row::Cells(vec![Widget::Tree {
                lines: Lines::None,
                items: vec![
                    Item {
                        depth: 1,
                        label: "a".into(),
                        cells: Vec::new()
                    },
                    Item {
                        depth: 2,
                        label: "b".into(),
                        cells: Vec::new()
                    }
                ]
            }])
        );
        assert_eq!(
            rows[6],
            Row::Cells(vec![
                Widget::Button("Cancel".into()),
                Widget::Button("OK".into())
            ])
        );
    }

    fn item(depth: usize, label: &str, cells: &[&str]) -> Item {
        Item {
            depth,
            label: label.into(),
            cells: cells.iter().map(|c| (*c).into()).collect(),
        }
    }

    #[test]
    fn a_tree_keeps_its_columns_and_its_lines() {
        let d = parse(&lines(
            "{#\nName | Value\n{T#\n+ World | Population\n++ Europe | 744\n}\nNotes | 12 KB\n}",
        ))
        .unwrap();
        let Widget::Grid { rows, .. } = d.root else {
            panic!("a grid");
        };
        assert_eq!(
            rows[1],
            Row::Cells(vec![Widget::Tree {
                lines: Lines::All,
                items: vec![
                    item(1, "World", &["Population"]),
                    item(2, "Europe", &["744"])
                ]
            }])
        );
        assert_eq!(
            rows[2],
            Row::Cells(vec![
                Widget::Text("Notes".into()),
                Widget::Text("12 KB".into())
            ])
        );
    }

    #[test]
    fn a_block_stands_in_a_row_s_last_cell_and_the_row_goes_on() {
        let d = parse(&lines(
            "{#\nFolder | Size\nFiles | {T\n+ src\n++ main.rs\n} | 3 files\nNotes | {\n[OK] | [Cancel]\n}\nLast | end\n}",
        ))
        .unwrap();
        let Widget::Grid { rows, .. } = d.root else {
            panic!("a grid");
        };
        assert_eq!(rows.len(), 4, "{rows:?}");
        assert_eq!(
            rows[1],
            Row::Cells(vec![
                Widget::Text("Files".into()),
                Widget::Tree {
                    lines: Lines::None,
                    items: vec![item(1, "src", &[]), item(2, "main.rs", &[])]
                },
                Widget::Text("3 files".into())
            ])
        );
        let Row::Cells(notes) = &rows[2] else {
            panic!("a row");
        };
        assert!(matches!(&notes[1], Widget::Grid { rows, .. } if rows.len() == 1));
        assert_eq!(
            rows[3],
            Row::Cells(vec![
                Widget::Text("Last".into()),
                Widget::Text("end".into())
            ])
        );
        // A block on a line of its own starts its row, which goes on too.
        let d = parse(&lines(
            "{#
{T
+ src
} | 3 files
Notes | 12 KB
}",
        ))
        .unwrap();
        let Widget::Grid { rows, .. } = d.root else {
            panic!("a grid");
        };
        assert_eq!(
            rows[0],
            Row::Cells(vec![
                Widget::Tree {
                    lines: Lines::None,
                    items: vec![item(1, "src", &[])]
                },
                Widget::Text("3 files".into())
            ])
        );
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn what_it_does_not_read_names_its_line() {
        for (text, line, part) in [
            ("{\nA | B\n", 0, "no `}`"),
            ("{S\nA\n}", 0, "scroll"),
            ("{T\n+ a\n{\nB\n}\n}", 0, "inside tabs, a menu or a tree"),
            ("A | B", 0, "starts with"),
            ("{\nA\n}\n{\nB\n}", 3, "one window"),
        ] {
            let err = parse(&lines(text)).unwrap_err();
            assert_eq!(err.line, Some(line), "{text}");
            assert!(err.reason.contains(part), "{text}: {}", err.reason);
        }
    }
}
