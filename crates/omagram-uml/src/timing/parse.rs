//! From the body's lines to a [`Diagram`]. A line the parser does not
//! understand declines the whole diagram, naming that line.
//!
//! `@` moves a cursor. A state written before any `@` is the row's initial
//! state. `Name@time` and `@time` are absolute, `@+time` is relative to the
//! cursor (or to the row's last state, in a `@Name` block).

use super::{Clock, Constraint, Diagram, Highlight, Mark, Message, Note, Row, RowKind, State};
use crate::Declined;
use crate::color::{self, Color};
use crate::common::{Frame, block, keyword, snippet, squeezed_eq};
use crate::source::{Line, starts_with_ci};

/// Whether these `@startuml` lines are a timing diagram: one of them
/// declares a row as `robust`, `concise`, `clock`, `binary` or `analog`
/// (not as `rectangle`). The word has to be followed by a name, so
/// `clock -> server` stays a message and `Analog : on` a state.
pub fn is_timing(lines: &[Line]) -> bool {
    lines.iter().any(|line| {
        ["robust", "concise", "clock", "binary", "analog"]
            .iter()
            .filter_map(|word| keyword(line.text, word))
            .any(declares_row)
    })
}

/// Whether `rest`, after a row's word, starts like a declaration: a quoted
/// label, or a name followed by nothing, a word, a color, a stereotype or
/// `{`.
fn declares_row(rest: &str) -> bool {
    if rest.starts_with('"') {
        return true;
    }
    let Some((_, after)) = ident(rest) else {
        return false;
    };
    after.is_empty()
        || after.starts_with(|c: char| c.is_alphabetic() || matches!(c, '#' | '{' | '"'))
        || (after.starts_with("<<") && !after.starts_with("<<-"))
}

pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut builder = Builder::default();
    let mut at = 0;
    while at < lines.len() {
        if let Some(next) = builder.frame.line(lines, at)? {
            at = next;
            continue;
        }
        at = builder.line(lines, at)?;
    }
    builder.finish()
}

#[derive(Default)]
struct Builder {
    rows: Vec<Row>,
    /// Last timed state of each row.
    last: Vec<Option<f64>>,
    messages: Vec<Message>,
    constraints: Vec<Constraint>,
    highlights: Vec<Highlight>,
    notes: Vec<Note>,
    anchors: Vec<(String, f64)>,
    cursor: f64,
    /// Set by a global `@time`, so a later `Name is` lands on the cursor
    /// instead of in the preamble.
    timed: bool,
    focus: Option<String>,
    hide_axis: bool,
    scale: Option<(f64, f64)>,
    frame: Frame,
}

enum At {
    Time(TimeExpr),
    Focus(String),
}

enum TimeExpr {
    Abs(f64),
    Rel(f64),
    Anchor { name: String, delta: f64 },
}

struct End {
    name: String,
    time: Option<TimeExpr>,
}

impl Builder {
    fn line(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line { no, text, .. } = lines[at];
        self.banned(text, no)?;
        if let Some(rest) = keyword(text, "note") {
            return self.note(lines, at, rest);
        }
        self.statement(text, no)?;
        Ok(at + 1)
    }

    fn banned(&self, text: &str, no: usize) -> Result<(), Declined> {
        if starts_with_ci(text, "<style") {
            return Err(fail(no, "`<style>` is not supported yet"));
        }
        if text.contains("<<") {
            return Err(fail(no, "stereotypes are not supported yet"));
        }
        if keyword(text, "use").is_some() {
            return Err(fail(no, "dates on the time axis are not supported yet"));
        }
        if keyword(text, "manual").is_some() {
            return Err(fail(no, "a manual time axis is not supported yet"));
        }
        if keyword(text, "mode").is_some() || keyword(text, "compact").is_some() {
            return Err(fail(no, "compact timing diagrams are not supported yet"));
        }
        if text.to_ascii_lowercase().contains("pixels height") {
            return Err(fail(no, "an analog pixel height is not supported yet"));
        }
        Ok(())
    }

    fn statement(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        if let Some(rest) = keyword(text, "highlight") {
            return self.highlight(rest, no);
        }
        if let Some(rest) = keyword(text, "hide") {
            if rest.eq_ignore_ascii_case("time-axis") {
                self.hide_axis = true;
                return Ok(());
            }
            return Err(fail(
                no,
                format!("`{}` is not supported yet", snippet(text)),
            ));
        }
        if let Some(rest) = keyword(text, "scale") {
            self.scale = Some(parse_scale(rest, no)?);
            return Ok(());
        }
        if text.contains("<->") {
            return self.constraint(text, no);
        }
        if text.starts_with('@') {
            return self.at_line(text, no);
        }
        if self.participant(text, no)? {
            return Ok(());
        }
        if let Some((_, rest)) = ident(text)
            && keyword(rest, "ticks").is_some()
        {
            return Err(fail(no, "analog ticks are not supported yet"));
        }
        if text.contains("->") {
            return self.message(text, no);
        }
        if self.try_has(text, no)? || self.try_state(text, no)? {
            return Ok(());
        }
        Err(fail(
            no,
            format!("`{}` is not supported yet", snippet(text)),
        ))
    }

    fn at_line(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        let (at, rest) = split_at(text, no)?;
        match at {
            At::Focus(name) => {
                self.row_index(&name, no)?;
                self.focus = Some(name);
                if rest.is_empty() {
                    return Ok(());
                }
                self.focused_state(rest, no)
            }
            At::Time(expr) => {
                let time = self.resolve(expr, self.cursor, no)?;
                self.cursor = time;
                self.timed = true;
                if let Some(rest) = keyword(rest, "as") {
                    let rest = rest
                        .strip_prefix(':')
                        .ok_or_else(|| fail(no, "an anchor needs `:`"))?;
                    let (name, rest) =
                        ident(rest).ok_or_else(|| fail(no, "an anchor needs a name"))?;
                    if !rest.is_empty() {
                        return Err(fail(
                            no,
                            format!("`{}` is not supported yet", snippet(rest)),
                        ));
                    }
                    if self.anchors.iter().any(|(got, _)| got == &name) {
                        return Err(fail(no, format!("anchor `{name}` is already there")));
                    }
                    self.anchors.push((name, time));
                    return Ok(());
                }
                // A global time leaves a `@Name` block.
                self.focus = None;
                if rest.is_empty() {
                    return Ok(());
                }
                self.statement(rest, no)
            }
        }
    }

    fn focused_state(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        let key = self
            .focus
            .clone()
            .ok_or_else(|| fail(no, "a state needs a participant"))?;
        let (expr, rest) = loose_time(text, no)?;
        let rest = keyword(rest, "is")
            .ok_or_else(|| fail(no, format!("`{}` is not supported yet", snippet(text))))?;
        let base = self.last_of(&key).unwrap_or(0.0);
        let time = self.resolve(expr, base, no)?;
        self.add_state(&key, time, rest, no)
    }

    fn participant(&mut self, text: &str, no: usize) -> Result<bool, Declined> {
        let Some((kind, mut rest)) = row_kind(text) else {
            return Ok(false);
        };
        let mut label = None;
        if rest.starts_with('"') {
            let (value, next) = quoted(rest, no)?;
            label = Some(value);
            rest = next;
        } else if let Some((word, next)) = ident(rest)
            && !matches!(
                word.to_ascii_lowercase().as_str(),
                "between" | "as" | "with"
            )
        {
            label = Some(word);
            rest = next;
        }
        let mut analog = None;
        if let Some(next) = keyword(rest, "between") {
            if kind != RowKind::Analog {
                return Err(fail(no, "`between` is only for an analog row"));
            }
            let (low, next) = number(next, no)?;
            let next = keyword(next, "and").ok_or_else(|| fail(no, "`between` needs `and`"))?;
            let (high, next) = number(next, no)?;
            analog = Some((low, high));
            rest = next;
        }
        let mut key = None;
        if let Some(next) = keyword(rest, "as") {
            let (name, next) = ident(next).ok_or_else(|| fail(no, "`as` needs a name"))?;
            key = Some(name);
            rest = next;
        }
        let mut period = None;
        let mut pulse = None;
        let mut offset = 0.0;
        if let Some(next) = keyword(rest, "with") {
            if kind != RowKind::Clock {
                return Err(fail(no, "only a clock has `with period`"));
            }
            rest = next;
        }
        let mut fill = None;
        while !rest.is_empty() {
            if let Some(next) = keyword(rest, "period") {
                if period.is_some() {
                    return Err(fail(no, "`period` is already set"));
                }
                let (value, next) = number(next, no)?;
                period = Some(value);
                rest = next;
            } else if let Some(next) = keyword(rest, "pulse") {
                if pulse.is_some() {
                    return Err(fail(no, "`pulse` is already set"));
                }
                let (value, next) = number(next, no)?;
                pulse = Some(value);
                rest = next;
            } else if let Some(next) = keyword(rest, "offset") {
                let (value, next) = number(next, no)?;
                offset = value;
                rest = next;
            } else if let Some(next) = keyword(rest, "between") {
                if kind != RowKind::Analog {
                    return Err(fail(no, "`between` is only for an analog row"));
                }
                if analog.is_some() {
                    return Err(fail(no, "`between` is already set"));
                }
                let (low, next) = number(next, no)?;
                let next = keyword(next, "and").ok_or_else(|| fail(no, "`between` needs `and`"))?;
                let (high, next) = number(next, no)?;
                analog = Some((low, high));
                rest = next;
            } else if rest.starts_with('#') {
                let (color, next) = take_color(rest, no)?;
                fill = Some(color);
                rest = next;
            } else {
                return Err(fail(
                    no,
                    format!("`{}` is not supported yet", snippet(rest)),
                ));
            }
        }
        let clock = if kind == RowKind::Clock {
            let Some(period) = period else {
                return Err(fail(no, "a clock needs `with period`"));
            };
            if period <= 0.0 {
                return Err(fail(no, "a clock period has to be greater than 0"));
            }
            let pulse = pulse.unwrap_or(period / 2.0);
            if !(0.0..=period).contains(&pulse) {
                return Err(fail(no, "the pulse has to be from 0 to the period"));
            }
            Some(Clock {
                period,
                pulse,
                offset,
            })
        } else if period.is_some() || pulse.is_some() || offset != 0.0 {
            return Err(fail(no, "only a clock has a period, a pulse or an offset"));
        } else {
            None
        };
        let Some(label) = label else {
            return Err(fail(no, "a participant needs a name"));
        };
        let key = key.unwrap_or_else(|| label.clone());
        if self.rows.iter().any(|row| row.key == key) {
            return Err(fail(no, format!("participant `{key}` is already there")));
        }
        self.rows.push(Row {
            key,
            label,
            kind,
            states: Vec::new(),
            initial: None,
            order: Vec::new(),
            clock,
            analog,
            fill,
            line: no,
        });
        self.last.push(None);
        Ok(true)
    }

    fn constraint(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        let (left, right) = text
            .split_once("<->")
            .ok_or_else(|| fail(no, "a constraint needs `<->`"))?;
        let (row, start_expr, left_rest) = constraint_point(left, no)?;
        if !left_rest.trim().is_empty() {
            return Err(fail(
                no,
                format!("`{}` is not supported yet", snippet(left_rest)),
            ));
        }
        let (right_row, end_expr, right_rest) = constraint_point(right, no)?;
        if right_row.is_some() {
            return Err(fail(
                no,
                "the right side of a constraint must not name a participant",
            ));
        }
        let label = match right_rest.trim_start().strip_prefix(':') {
            Some(text) => text.trim().to_owned(),
            None if right_rest.trim().is_empty() => String::new(),
            None => {
                return Err(fail(
                    no,
                    format!("`{}` is not supported yet", snippet(right_rest)),
                ));
            }
        };
        let start = self.resolve(start_expr, self.cursor, no)?;
        let end = self.resolve(end_expr, start, no)?;
        // The drawer finds the row by its key, whichever name was written.
        let row = match row {
            Some(name) => Some(self.rows[self.row_index(&name, no)?].key.clone()),
            None => None,
        };
        // As in PlantUML, the time moves to the left end and then to the
        // right one, where the next untimed state lands.
        self.cursor = end;
        self.timed = true;
        self.constraints.push(Constraint {
            row,
            start,
            end,
            label,
            line: no,
        });
        Ok(())
    }

    fn message(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        let (from, rest) = endpoint(text, no)?;
        let rest = rest
            .strip_prefix("->")
            .ok_or_else(|| fail(no, "a message needs `->`"))?
            .trim_start();
        let (to, rest) = endpoint(rest, no)?;
        let label = match rest.strip_prefix(':') {
            Some(text) => text.trim().to_owned(),
            None if rest.is_empty() => String::new(),
            None => {
                return Err(fail(
                    no,
                    format!("`{}` is not supported yet", snippet(rest)),
                ));
            }
        };
        let time = match from.time {
            None => self.cursor,
            Some(expr) => self.resolve(expr, self.cursor, no)?,
        };
        let to_time = match to.time {
            None => time,
            Some(expr) => self.resolve(expr, time, no)?,
        };
        self.row_index(&from.name, no)?;
        self.row_index(&to.name, no)?;
        self.messages.push(Message {
            from: from.name,
            to: to.name,
            time,
            to_time,
            label,
            line: no,
        });
        Ok(())
    }

    fn try_has(&mut self, text: &str, no: usize) -> Result<bool, Declined> {
        let Some((name, after)) = named(text, no)? else {
            return Ok(false);
        };
        let Some(rest) = keyword(after, "has") else {
            return Ok(false);
        };
        if rest.to_ascii_lowercase().contains(" as ") {
            return Err(fail(
                no,
                "a renamed state (`has … as`) is not supported yet",
            ));
        }
        let order: Vec<String> = rest
            .split(',')
            .map(|part| unquote(part.trim()))
            .filter(|part| !part.is_empty())
            .collect();
        if order.is_empty() {
            return Err(fail(no, "`has` needs states"));
        }
        let index = self.row_index(&name, no)?;
        if self.rows[index].kind != RowKind::Robust {
            return Err(fail(no, "`has` is only for a robust row"));
        }
        self.rows[index].order = order;
        Ok(true)
    }

    fn try_state(&mut self, text: &str, no: usize) -> Result<bool, Declined> {
        let trimmed = text.trim_start();
        if self.focus.is_some()
            && (trimmed.starts_with('+') || trimmed.starts_with(|c: char| c.is_ascii_digit()))
        {
            self.focused_state(trimmed, no)?;
            return Ok(true);
        }
        let Some((name, after)) = named(trimmed, no)? else {
            return Ok(false);
        };
        let Some(rest) = keyword(after, "is") else {
            return Ok(false);
        };
        if self.timed {
            self.add_state(&name, self.cursor, rest, no)?;
        } else {
            self.add_initial(&name, rest, no)?;
        }
        Ok(true)
    }

    fn highlight(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        let (start, rest) = loose_time(rest, no)?;
        let rest = keyword(rest, "to").ok_or_else(|| fail(no, "`highlight` needs `to`"))?;
        let (end, mut rest) = loose_time(rest, no)?;
        let mut color = None;
        if rest.starts_with('#') {
            let (parsed, next) = take_color(rest, no)?;
            color = Some(parsed);
            rest = next;
        }
        let caption = match rest.strip_prefix(':') {
            Some(text) => text.trim().to_owned(),
            None if rest.is_empty() => String::new(),
            None => {
                return Err(fail(
                    no,
                    format!("`{}` is not supported yet", snippet(rest)),
                ));
            }
        };
        let start = self.resolve(start, self.cursor, no)?;
        let end = self.resolve(end, self.cursor, no)?;
        self.highlights.push(Highlight {
            start,
            end,
            color,
            caption,
            line: no,
        });
        Ok(())
    }

    fn note(&mut self, lines: &[Line], at: usize, rest: &str) -> Result<usize, Declined> {
        let no = lines[at].no;
        let (top, rest) = if let Some(rest) = keyword(rest, "top") {
            (true, rest)
        } else if let Some(rest) = keyword(rest, "bottom") {
            (false, rest)
        } else {
            return Err(fail(
                no,
                "only `note top of` and `note bottom of` are supported yet",
            ));
        };
        let rest = keyword(rest, "of").ok_or_else(|| fail(no, "`note` needs `of`"))?;
        let (name, rest) = ident(rest).ok_or_else(|| fail(no, "`note` needs a participant"))?;
        let index = self.row_index(&name, no)?;
        if matches!(
            self.rows[index].kind,
            RowKind::Robust | RowKind::Analog | RowKind::Clock
        ) {
            return Err(fail(
                no,
                "a note on a robust, analog or clock row is not supported yet",
            ));
        }
        let (text, next) = if let Some(body) = rest.strip_prefix(':') {
            (body.trim().to_owned(), at + 1)
        } else if rest.is_empty() {
            block(lines, at, "note", |line| squeezed_eq(line, "endnote"))?
        } else {
            return Err(fail(
                no,
                format!("`{}` is not supported yet", snippet(rest)),
            ));
        };
        if text.is_empty() {
            return Err(fail(no, "`note` needs a text"));
        }
        self.notes.push(Note {
            row: self.rows[index].key.clone(),
            top,
            text,
            line: no,
        });
        Ok(next)
    }

    fn add_state(&mut self, name: &str, time: f64, tail: &str, no: usize) -> Result<(), Declined> {
        let mark = self.mark(name, tail, no)?;
        let index = self.row_index(name, no)?;
        if let Some(state) = self.rows[index]
            .states
            .iter_mut()
            .find(|state| (state.time - time).abs() < 1e-9)
        {
            state.mark = mark;
        } else {
            self.rows[index].states.push(State { time, mark });
        }
        self.last[index] = Some(time);
        self.cursor = time;
        Ok(())
    }

    fn add_initial(&mut self, name: &str, tail: &str, no: usize) -> Result<(), Declined> {
        let mark = self.mark(name, tail, no)?;
        let index = self.row_index(name, no)?;
        self.rows[index].initial = Some(mark);
        Ok(())
    }

    /// Checks the tail against the row `name` and returns the mark. The row
    /// has to exist.
    fn mark(&self, name: &str, tail: &str, no: usize) -> Result<Mark, Declined> {
        let index = self.row_index(name, no)?;
        let (state_name, color, note) = split_tail(tail, no)?;
        match self.rows[index].kind {
            RowKind::Binary if !binary_level(&state_name) => {
                return Err(fail(
                    no,
                    "a binary row only has high, low, 1, 0, true or false",
                ));
            }
            RowKind::Analog
                if state_name
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .is_none() =>
            {
                return Err(fail(no, "an analog row needs a number"));
            }
            RowKind::Clock => return Err(fail(no, "a clock has no state")),
            _ => {}
        }
        Ok(Mark {
            name: state_name,
            color,
            note,
            line: no,
        })
    }

    fn resolve(&self, expr: TimeExpr, base: f64, no: usize) -> Result<f64, Declined> {
        let time = match expr {
            TimeExpr::Abs(time) => time,
            TimeExpr::Rel(delta) => base + delta,
            TimeExpr::Anchor { name, delta } => {
                let Some((_, time)) = self.anchors.iter().find(|(got, _)| got == &name) else {
                    return Err(fail(no, format!("no anchor `{name}`")));
                };
                time + delta
            }
        };
        if time.is_finite() {
            Ok(time)
        } else {
            Err(fail(no, "this time is not a finite number"))
        }
    }

    fn row_index(&self, name: &str, no: usize) -> Result<usize, Declined> {
        self.rows
            .iter()
            .position(|row| row.key == name || row.label == name)
            .ok_or_else(|| fail(no, format!("no participant `{name}`")))
    }

    fn last_of(&self, name: &str) -> Option<f64> {
        self.rows
            .iter()
            .position(|row| row.key == name || row.label == name)
            .and_then(|index| self.last[index])
    }

    fn finish(mut self) -> Result<Diagram, Declined> {
        for row in &mut self.rows {
            row.states.sort_by(|a, b| a.time.total_cmp(&b.time));
            if row.kind == RowKind::Robust {
                let mut order = std::mem::take(&mut row.order);
                let names: Vec<String> = row
                    .initial
                    .iter()
                    .map(|mark| mark.name.clone())
                    .chain(row.states.iter().map(|state| state.mark.name.clone()))
                    .collect();
                for name in names {
                    if !order.contains(&name) {
                        order.push(name);
                    }
                }
                row.order = order;
            }
        }
        Ok(Diagram {
            rows: self.rows,
            messages: self.messages,
            constraints: self.constraints,
            highlights: self.highlights,
            notes: self.notes,
            hide_axis: self.hide_axis,
            scale: self.scale,
            frame: self.frame,
        })
    }
}

fn row_kind(text: &str) -> Option<(RowKind, &str)> {
    [
        ("robust", RowKind::Robust),
        ("concise", RowKind::Concise),
        ("binary", RowKind::Binary),
        ("analog", RowKind::Analog),
        ("clock", RowKind::Clock),
        ("rectangle", RowKind::Rectangle),
    ]
    .into_iter()
    .find_map(|(word, kind)| keyword(text, word).map(|rest| (kind, rest)))
}

fn split_at(text: &str, no: usize) -> Result<(At, &str), Declined> {
    let rest = text
        .strip_prefix('@')
        .ok_or_else(|| fail(no, "a time needs `@`"))?;
    if rest.contains('*') {
        return Err(fail(
            no,
            "a time counted in clock periods is not supported yet",
        ));
    }
    let timed = rest.starts_with([':', '+', '-']) || rest.starts_with(|c: char| c.is_ascii_digit());
    if !timed {
        let (name, after) = ident(rest)
            .ok_or_else(|| fail(no, format!("`{}` is not supported yet", snippet(text))))?;
        return Ok((At::Focus(name), after));
    }
    let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let token = &rest[..token_end];
    if token.contains('/')
        || (token.contains(':') && rest.starts_with(|c: char| c.is_ascii_digit()))
    {
        return Err(fail(no, "dates and clock times are not supported yet"));
    }
    let (expr, after) = loose_time(rest, no)?;
    Ok((At::Time(expr), after))
}

fn loose_time(text: &str, no: usize) -> Result<(TimeExpr, &str), Declined> {
    let text = text.trim_start();
    if let Some(rest) = text.strip_prefix(':') {
        let (name, rest) = ident(rest).ok_or_else(|| fail(no, "an anchor needs a name"))?;
        let (delta, rest) = glued_delta(rest, no)?;
        return Ok((TimeExpr::Anchor { name, delta }, rest));
    }
    if let Some(rest) = text.strip_prefix('+') {
        let (value, rest) = number(rest, no)?;
        return Ok((TimeExpr::Rel(value), rest));
    }
    let (value, rest) = number(text, no)?;
    Ok((TimeExpr::Abs(value), rest))
}

fn glued_delta(text: &str, no: usize) -> Result<(f64, &str), Declined> {
    let signed = matches!(text.as_bytes().first(), Some(b'+' | b'-'))
        && text
            .as_bytes()
            .get(1)
            .is_some_and(|byte| byte.is_ascii_digit());
    if signed {
        number(text, no)
    } else {
        Ok((0.0, text.trim_start()))
    }
}

fn constraint_point(text: &str, no: usize) -> Result<(Option<String>, TimeExpr, &str), Declined> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('@') {
        let (expr, rest) = loose_time(rest, no)?;
        return Ok((None, expr, rest));
    }
    let (name, rest) = ident(text).ok_or_else(|| fail(no, "a constraint needs a time"))?;
    let rest = rest
        .strip_prefix('@')
        .ok_or_else(|| fail(no, "a constraint needs `@`"))?;
    let (expr, rest) = loose_time(rest, no)?;
    Ok((Some(name), expr, rest))
}

fn endpoint(text: &str, no: usize) -> Result<(End, &str), Declined> {
    let text = text.trim_start();
    let (name, rest) = if text.starts_with('"') {
        quoted(text, no)?
    } else {
        ident(text).ok_or_else(|| fail(no, "a message needs a participant"))?
    };
    if let Some(rest) = rest.strip_prefix('@') {
        let (expr, rest) = loose_time(rest, no)?;
        return Ok((
            End {
                name,
                time: Some(expr),
            },
            rest.trim_start(),
        ));
    }
    Ok((End { name, time: None }, rest.trim_start()))
}

fn parse_scale(rest: &str, no: usize) -> Result<(f64, f64), Declined> {
    let (across, rest) = number(rest, no)?;
    let rest = keyword(rest, "as").ok_or_else(|| fail(no, "`scale` needs `N as M pixels`"))?;
    let (pixels, rest) = number(rest, no)?;
    let rest = keyword(rest, "pixels").ok_or_else(|| fail(no, "`scale` needs `N as M pixels`"))?;
    if !rest.is_empty() {
        return Err(fail(
            no,
            format!("`{}` is not supported yet", snippet(rest)),
        ));
    }
    if across <= 0.0 || pixels <= 0.0 {
        return Err(fail(no, "`scale` needs numbers greater than 0"));
    }
    Ok((across, pixels))
}

fn split_tail(tail: &str, no: usize) -> Result<(String, Option<Color>, Option<String>), Declined> {
    let tail = tail.trim();
    if tail.starts_with('{') {
        return Err(fail(no, "a state in braces is not supported yet"));
    }
    let (name, mut rest) = if tail.starts_with('"') {
        quoted(tail, no)?
    } else {
        let hash = tail.find(" #").unwrap_or(tail.len());
        let colon = tail.find(':').unwrap_or(tail.len());
        let name = tail[..hash.min(colon)].trim();
        if name.is_empty() {
            return Err(fail(no, "a state needs a name"));
        }
        (name.to_owned(), tail[hash.min(colon)..].trim_start())
    };
    let mut color = None;
    if rest.starts_with('#') {
        let (parsed, next) = take_color(rest, no)?;
        color = Some(parsed);
        rest = next.trim_start();
    }
    let note = if let Some(note) = rest.strip_prefix(':') {
        let note = note.trim();
        (!note.is_empty()).then(|| note.to_owned())
    } else if rest.is_empty() {
        None
    } else {
        return Err(fail(
            no,
            format!("`{}` is not supported yet", snippet(rest)),
        ));
    };
    Ok((name, color, note))
}

fn take_color(text: &str, no: usize) -> Result<(Color, &str), Declined> {
    let text = text
        .trim_start()
        .strip_prefix('#')
        .ok_or_else(|| fail(no, "a color needs `#`"))?;
    let end = text
        .find(|c: char| c.is_whitespace() || c == ':')
        .unwrap_or(text.len());
    let name = &text[..end];
    if name.is_empty() || name.contains([';', '/', '|', '\\']) {
        return Err(fail(
            no,
            format!("color gradients (`#{name}`) are not supported yet"),
        ));
    }
    let color = color::parse(name)
        .map_err(|reason| fail(no, reason))?
        .ok_or_else(|| fail(no, "a color must not be transparent"))?;
    Ok((color, text[end..].trim_start()))
}

fn binary_level(name: &str) -> bool {
    ["high", "low", "1", "0", "true", "false"]
        .iter()
        .any(|word| name.eq_ignore_ascii_case(word))
}

fn named(text: &str, no: usize) -> Result<Option<(String, &str)>, Declined> {
    let text = text.trim_start();
    if text.starts_with('"') {
        return Ok(Some(quoted(text, no)?));
    }
    Ok(ident(text))
}

fn ident(text: &str) -> Option<(String, &str)> {
    let text = text.trim_start();
    let mut end = 0;
    for (index, c) in text.char_indices() {
        if index == 0 && c.is_ascii_digit() {
            return None;
        }
        if c.is_ascii_alphanumeric() || c == '_' || c == '.' {
            end = index + c.len_utf8();
        } else {
            break;
        }
    }
    (end > 0).then(|| (text[..end].to_owned(), text[end..].trim_start()))
}

fn quoted(text: &str, no: usize) -> Result<(String, &str), Declined> {
    let text = text.trim_start();
    let rest = text
        .strip_prefix('"')
        .ok_or_else(|| fail(no, "a quote is not closed"))?;
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((index, c)) = chars.next() {
        if c == '\\' {
            let Some((_, escaped)) = chars.next() else {
                break;
            };
            out.push(escaped);
            continue;
        }
        if c == '"' {
            return Ok((out, rest[index + 1..].trim_start()));
        }
        out.push(c);
    }
    Err(fail(no, "a quote is not closed"))
}

fn number(text: &str, no: usize) -> Result<(f64, &str), Declined> {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return Err(fail(no, "a number is missing"));
    }
    let mut end = usize::from(bytes[0] == b'+' || bytes[0] == b'-');
    let digits_at = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        let after_dot = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == after_dot {
            return Err(fail(no, format!("`{}` is not a number", snippet(text))));
        }
    }
    if end == digits_at {
        return Err(fail(no, format!("`{}` is not a number", snippet(text))));
    }
    let value: f64 = text[..end]
        .parse()
        .map_err(|_| fail(no, format!("`{}` is not a number", snippet(text))))?;
    if !value.is_finite() {
        return Err(fail(no, "this number is not finite"));
    }
    Ok((value, text[end..].trim_start()))
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text)
        .to_owned()
}

fn fail(line: usize, reason: impl Into<String>) -> Declined {
    Declined::at(line, reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Diagram as Kind;
    use crate::parse as parse_source;

    fn timing(source: &str) -> Diagram {
        match parse_source(source).unwrap() {
            Kind::Timing(diagram) => diagram,
            other => panic!("not timing: {other:?}"),
        }
    }

    #[test]
    fn states_messages_and_a_constraint() {
        let diagram = timing(
            "@startuml\n\
             robust \"Web Browser\" as WB\n\
             concise \"Web User\" as WU\n\
             WB is Initializing\n\
             @0\n\
             WU is Idle\n\
             WB is Idle\n\
             @100\n\
             WU -> WB : URL\n\
             WB is Processing\n\
             @300\n\
             WB is Waiting\n\
             WB@0 <-> @50 : {50 ms lag}\n\
             @enduml\n",
        );
        assert_eq!(
            diagram.rows[0].initial.as_ref().unwrap().name,
            "Initializing"
        );
        assert_eq!(
            diagram.rows[0].order,
            ["Initializing", "Idle", "Processing", "Waiting"]
        );
        assert_eq!(diagram.messages[0].label, "URL");
        assert!((diagram.messages[0].time - 100.0).abs() < 1e-9);
        assert!((diagram.constraints[0].end - 50.0).abs() < 1e-9);
    }

    #[test]
    fn anchors_binary_and_a_declined_date() {
        let diagram = timing(
            "clock clk with period 1\n\
             binary \"Enable\" as EN\n\
             @0 as :start\n\
             EN is low\n\
             @5 as :en_high\n\
             @:en_high EN is high\n\
             @:en_high-2 EN is low\n",
        );
        let times: Vec<f64> = diagram.rows[1]
            .states
            .iter()
            .map(|state| state.time)
            .collect();
        assert_eq!(times, [0.0, 3.0, 5.0]);
        let err = parse_source("@startuml\nrobust \"W\" as W\n@2019/07/02\nW is Idle\n@enduml")
            .unwrap_err();
        assert_eq!(err.line, Some(2));
        assert!(err.reason.contains("dates"), "{}", err.reason);
        let err =
            parse_source("@startuml\nbinary \"E\" as E\n@0\nE is maybe\n@enduml").unwrap_err();
        assert_eq!(err.line, Some(3));
    }

    #[test]
    fn a_rectangle_row_needs_a_timing_keyword() {
        assert!(matches!(
            parse_source("@startuml\nrectangle \"Prep\" as P\n@enduml").unwrap(),
            Kind::Structure(_)
        ));
        let diagram = timing(
            "@startuml\nrobust \"Web\" as W\nrectangle \"Prep\" as P\n@0\nW is Idle\nP is Idle\n@enduml",
        );
        assert_eq!(diagram.rows.len(), 2);
        assert_eq!(diagram.rows[1].kind, RowKind::Rectangle);
    }

    #[test]
    fn a_relative_constraint_counts_from_its_left_time() {
        let diagram = timing(
            "@startuml\n\
             robust \"W\" as W\n\
             @200 <-> @+150 : {150 ms}\n\
             @enduml\n",
        );
        let constraint = &diagram.constraints[0];
        assert!((constraint.start - 200.0).abs() < 1e-9);
        assert!((constraint.end - 350.0).abs() < 1e-9);
        assert!(constraint.row.is_none());
    }

    #[test]
    fn a_constraint_finds_its_row_by_label_and_ends_at_its_right_time() {
        let diagram = timing(
            "@startuml\n\
             robust WebBrowser as WB\n\
             WB is Idle\n\
             @200\n\
             WB is Busy\n\
             WebBrowser@100 <-> @+100 : lag\n\
             WB is Waiting\n\
             @enduml\n",
        );
        assert_eq!(diagram.constraints[0].row.as_deref(), Some("WB"));
        let times: Vec<f64> = diagram.rows[0].states.iter().map(|s| s.time).collect();
        assert_eq!(times, [200.0]);
        assert_eq!(diagram.rows[0].states[0].mark.name, "Waiting");
    }

    #[test]
    fn a_message_does_not_move_the_row() {
        let diagram = timing(
            "@startuml\n\
             concise Client\n\
             concise Server\n\
             @Client\n\
             0 is send\n\
             Client -> Server@+25\n\
             +25 is await\n\
             @enduml\n",
        );
        let times: Vec<f64> = diagram.rows[0]
            .states
            .iter()
            .map(|state| state.time)
            .collect();
        assert_eq!(times, [0.0, 25.0]);
        assert!((diagram.messages[0].to_time - 25.0).abs() < 1e-9);
        assert_eq!(diagram.rows[0].states[1].mark.name, "await");
    }

    #[test]
    fn hide_time_axis_is_a_flag() {
        let diagram = timing("@startuml\nclock clk with period 1\nhide time-axis\n@enduml\n");
        assert!(diagram.hide_axis);
    }

    #[test]
    fn the_core_fixture_draws() {
        let source = include_str!("../../../omagram-core/tests/fixtures/timing.puml");
        let svg = crate::render(source).unwrap();
        assert!(svg.starts_with("<svg"), "{svg}");
        assert!(svg.contains("Idle"), "{svg}");
    }
}
