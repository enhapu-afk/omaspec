//! Reading Salt: the lines are cut into blocks that open with `{` and
//! close with `}`, on lines of their own or around a row, and rows of
//! cells apart by `|`.

use super::{Diagram, Lines, Row, Rule, Widget};
use crate::Declined;
use crate::common::{Frame, squeezed_eq};
use crate::source::Line;

/// How deep blocks may stand in each other.
const MAX_DEPTH: usize = 64;

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
        title: Option<String>,
        no: usize,
    },
    Closes(usize),
    Row(String, usize),
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
        Some(Unit::Row(_, no) | Unit::Closes(no)) => {
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
            Unit::Opens { no, .. } | Unit::Closes(no) | Unit::Row(_, no) => *no,
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
    if let Some(after) = rest.strip_prefix('{') {
        let kind = after
            .chars()
            .next()
            .filter(|c| matches!(c, '+' | '#' | '!' | '-' | '^' | '/' | '*' | 'T' | 'S'))
            .unwrap_or(' ');
        rest = if kind == ' ' {
            after
        } else {
            &after[kind.len_utf8()..]
        };
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
        units.push(Unit::Opens { kind, title, no });
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
        }
        None => units.push(Unit::Row(rest.to_owned(), no)),
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
    let Some(Unit::Opens { kind, title, no }) = units.get(*at).cloned() else {
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
    let mut nested: Vec<(usize, Widget)> = Vec::new();
    loop {
        match units.get(*at).cloned() {
            None => return Err(Declined::at(no, "this block has no `}`")),
            Some(Unit::Closes(_)) => {
                *at += 1;
                break;
            }
            Some(Unit::Row(text, line)) => {
                rows.push((text, line));
                *at += 1;
            }
            Some(Unit::Opens { .. }) => {
                let inner = block(units, at, depth + 1)?;
                nested.push((rows.len(), inner));
                rows.push((String::new(), no));
            }
        }
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
        'T' => Ok(Widget::Tree(
            rows.iter()
                .map(|(text, _)| {
                    let first = cells(text).into_iter().next().unwrap_or_default();
                    let depth = first.chars().take_while(|c| *c == '+').count().max(1);
                    (depth, first.trim_start_matches('+').trim().to_owned())
                })
                .collect(),
        )),
        'S' => Err(Declined::at(
            no,
            "a scroll pane (`{S`) is not supported yet",
        )),
        _ => {
            let mut grid = Vec::with_capacity(rows.len());
            let mut nested = nested.into_iter().peekable();
            for (i, (text, line)) in rows.iter().enumerate() {
                if nested.peek().is_some_and(|(at, _)| *at == i) {
                    let (_, widget) = nested.next().expect("a nested block");
                    grid.push(Row::Cells(vec![widget]));
                    continue;
                }
                grid.push(row(text, *line)?);
            }
            let lines = match kind {
                '+' => Lines::Frame,
                '#' => Lines::All,
                '!' => Lines::Columns,
                '-' => Lines::Rows,
                _ => Lines::None,
            };
            let body = Widget::Grid { lines, rows: grid };
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

/// Every cell of `rows`, in order.
fn all_cells(rows: &[(String, usize)]) -> Vec<String> {
    rows.iter()
        .flat_map(|(text, _)| cells(text))
        .map(|cell| cell.trim().to_owned())
        .collect()
}

/// One row: a rule, or its cells.
fn row(text: &str, no: usize) -> Result<Row, Declined> {
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
    cells(text)
        .into_iter()
        .map(|cell| widget(cell, no))
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
            Row::Cells(vec![Widget::Tree(vec![(1, "a".into()), (2, "b".into())])])
        );
        assert_eq!(
            rows[6],
            Row::Cells(vec![
                Widget::Button("Cancel".into()),
                Widget::Button("OK".into())
            ])
        );
    }

    #[test]
    fn what_it_does_not_read_names_its_line() {
        for (text, line, part) in [
            ("{\nA | B\n", 0, "no `}`"),
            ("{S\nA\n}", 0, "scroll"),
            ("A | B", 0, "starts with"),
            ("{\nA\n}\n{\nB\n}", 3, "one window"),
        ] {
            let err = parse(&lines(text)).unwrap_err();
            assert_eq!(err.line, Some(line), "{text}");
            assert!(err.reason.contains(part), "{text}: {}", err.reason);
        }
    }
}
