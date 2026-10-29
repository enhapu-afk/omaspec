//! A Mermaid `gantt` read into the Gantt chart.
//!
//! The source is read again, the way the class and mind-map readers do.
//! `dateFormat` is `YYYY-MM-DD` or omitted. An end date is the first day
//! the task does not include, which is already how a bar's `end` works.
//! `inclusiveEndDates` keeps the written day as well. `after` starts at
//! the latest end of those tasks, and `until` ends at the earliest of
//! their starts. Mermaid draws no arrow for either, so the bar only moves.
//! A duration is a whole number of days or weeks, counted in calendar
//! days and then stretched across excluded ones: the bar stops before a
//! trailing excluded day, and the next task starts after it. `excludes`
//! takes `weekends`, a weekday, or a `YYYY-MM-DD`; `includes` puts a date
//! back. `weekend friday` makes the weekend Friday and Saturday, otherwise
//! it is Saturday and Sunday. Another date format, and a duration in
//! hours, minutes, months or years, are refused: each column is one day,
//! and a wrong day would be a wrong chart.

use super::date;
use super::{Bar, Diagram, Marks, Row};
use crate::common::{Frame, keyword, text_at};
use crate::source::starts_with_ci;

/// The widest chart the PlantUML drawer will paint, in days.
const MAX_DAYS: i64 = 3660;

/// The Gantt chart `source` describes, or `None` when it is not a `gantt`
/// or it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a Gantt chart, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

struct Reason {
    line: usize,
    what: &'static str,
}

enum Stop {
    Foreign,
    At(Reason),
}

fn read(source: &str) -> Result<Diagram, Stop> {
    let src = scan(source);
    let Some((header, rest)) = src.lines.split_first() else {
        return Err(Stop::Foreign);
    };
    if !header.1.eq_ignore_ascii_case("gantt") {
        return Err(Stop::Foreign);
    }
    if let Some(reason) = src.front {
        return Err(Stop::At(reason));
    }
    let mut chart = Chart::new(src.title);
    for (line, text) in rest {
        chart.line(*line, text).map_err(Stop::At)?;
    }
    chart.finish().map_err(Stop::At)
}

struct Scanned {
    title: Option<crate::common::Text>,
    /// Set only when the diagram is a Gantt chart. Another header stays foreign.
    front: Option<Reason>,
    lines: Vec<(usize, String)>,
}

/// Front matter only at the top. A `---` later is not a title.
fn scan(source: &str) -> Scanned {
    let mut title = None;
    let mut front = None;
    let mut lines = Vec::new();
    let mut iter = source.lines().enumerate().peekable();
    if iter.peek().is_some_and(|(_, raw)| raw.trim() == "---") {
        iter.next();
        for (no, raw) in iter.by_ref() {
            let text = raw.trim();
            if text == "---" {
                break;
            }
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            let Some((key, value)) = text.split_once(':') else {
                front.get_or_insert(Reason {
                    line: no,
                    what: "a front matter setting",
                });
                continue;
            };
            if key.trim().eq_ignore_ascii_case("title") {
                let value = unquote(value.trim());
                if !value.is_empty() {
                    title = Some(text_at(&value, no));
                }
                continue;
            }
            if !key.trim().is_empty() {
                front.get_or_insert(Reason {
                    line: no,
                    what: "a front matter setting",
                });
            }
        }
    }
    for (no, raw) in iter {
        let Some(text) = bare(raw) else { continue };
        lines.push((no, text.to_owned()));
    }
    Scanned {
        title,
        front,
        lines,
    }
}

/// The line without a `%%` comment. `None` when nothing is left.
fn bare(raw: &str) -> Option<&str> {
    let text = raw.trim();
    let text = match text.find("%%") {
        Some(at) => text[..at].trim(),
        None => text,
    };
    (!text.is_empty()).then_some(text)
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(text)
        .to_owned()
}

struct Chart {
    title: Option<crate::common::Text>,
    dated: bool,
    inclusive: bool,
    days: Days,
    in_descr: bool,
    auto: u32,
    tasks: Vec<Raw>,
    events: Vec<Item>,
}

/// Days a duration skips. Dates are absolute day counts, kept sorted.
struct Days {
    weekends: bool,
    /// ISO weekday the weekend starts on: 6 is Saturday (and Sunday),
    /// 5 is Friday (and Saturday).
    weekend_start: u8,
    /// ISO weekdays 1..=7 named in `excludes`.
    named: [bool; 8],
    dates: Vec<i64>,
    included: Vec<i64>,
}

impl Days {
    fn new() -> Self {
        Self {
            weekends: false,
            weekend_start: 6,
            named: [false; 8],
            dates: Vec::new(),
            included: Vec::new(),
        }
    }

    fn active(&self) -> bool {
        self.weekends || self.named.iter().any(|day| *day) || !self.dates.is_empty()
    }

    fn prepare(&mut self) {
        self.dates.sort_unstable();
        self.dates.dedup();
        self.included.sort_unstable();
        self.included.dedup();
    }

    fn skips(&self, day: i64) -> bool {
        if self.included.binary_search(&day).is_ok() {
            return false;
        }
        let iso = date::iso_weekday(day);
        if self.weekends && (iso == self.weekend_start || iso == self.weekend_start + 1) {
            return true;
        }
        self.named[usize::from(iso)] || self.dates.binary_search(&day).is_ok()
    }

    /// Mermaid adds the duration in calendar days, then pushes the end
    /// forward once for every excluded day inside it. The painted end is
    /// the last end that was still a working day; the returned end is
    /// where the next task starts.
    fn stretch(&self, start: i64, mut end: i64, line: usize) -> Result<(i64, i64), Reason> {
        let mut cursor = start.checked_add(1).ok_or(Reason {
            line,
            what: "a chart past 3660 days",
        })?;
        let mut invalid = false;
        let mut shown = end;
        while cursor <= end {
            if !invalid {
                shown = end;
            }
            if self.skips(cursor) {
                invalid = true;
                end = end.checked_add(1).ok_or(Reason {
                    line,
                    what: "a chart past 3660 days",
                })?;
                if end.saturating_sub(start) > MAX_DAYS {
                    return Err(Reason {
                        line,
                        what: "a chart past 3660 days",
                    });
                }
            } else {
                invalid = false;
            }
            cursor = cursor.checked_add(1).ok_or(Reason {
                line,
                what: "a chart past 3660 days",
            })?;
        }
        Ok((end, shown))
    }
}

struct Raw {
    key: String,
    label: String,
    line: usize,
    milestone: bool,
    marks: Marks,
    start: Start,
    end: End,
}

enum Start {
    Date(i64),
    After(Vec<String>),
    Previous,
}

enum End {
    Date(i64),
    Days(i64),
    Until(Vec<String>),
}

enum Item {
    Section { text: String, line: usize },
    Task(usize),
}

impl Chart {
    fn new(title: Option<crate::common::Text>) -> Self {
        Self {
            title,
            dated: false,
            inclusive: false,
            days: Days::new(),
            in_descr: false,
            auto: 0,
            tasks: Vec::new(),
            events: Vec::new(),
        }
    }

    fn line(&mut self, line: usize, text: &str) -> Result<(), Reason> {
        if self.in_descr {
            if text.contains('}') {
                self.in_descr = false;
            }
            return Ok(());
        }
        if starts_with_ci(text, "accDescr") && text.contains('{') && !text.contains('}') {
            self.in_descr = true;
            return Ok(());
        }
        if starts_with_ci(text, "accTitle")
            || starts_with_ci(text, "accDescr")
            || keyword(text, "click").is_some()
            || keyword(text, "todayMarker").is_some()
            || text.eq_ignore_ascii_case("topAxis")
        {
            return Ok(());
        }
        if let Some(rest) = keyword(text, "title") {
            if rest.is_empty() {
                return Err(Reason {
                    line,
                    what: "an unknown line",
                });
            }
            self.title = Some(text_at(rest, line));
            return Ok(());
        }
        if let Some(rest) = keyword(text, "dateFormat") {
            if rest != "YYYY-MM-DD" {
                return Err(Reason {
                    line,
                    what: "a date format",
                });
            }
            if self.dated {
                return Err(Reason {
                    line,
                    what: "a second date format",
                });
            }
            self.dated = true;
            return Ok(());
        }
        if let Some(rest) = keyword(text, "excludes") {
            return self.exclude(rest, line);
        }
        if let Some(rest) = keyword(text, "includes") {
            return self.include(rest, line);
        }
        if let Some(rest) = keyword(text, "inclusiveEndDates") {
            if !rest.is_empty() {
                return Err(Reason {
                    line,
                    what: "an inclusive end date",
                });
            }
            self.inclusive = true;
            return Ok(());
        }
        if keyword(text, "axisFormat").is_some() {
            return Err(Reason {
                line,
                what: "an axis format",
            });
        }
        if keyword(text, "tickInterval").is_some() {
            return Err(Reason {
                line,
                what: "a tick interval",
            });
        }
        // The axis is one column per day, so where the week starts does
        // not move a bar. A weekly tick would, and that still declines.
        if let Some(rest) = keyword(text, "weekday") {
            if weekday(rest).is_none() {
                return Err(Reason {
                    line,
                    what: "a weekday",
                });
            }
            return Ok(());
        }
        if let Some(rest) = keyword(text, "weekend") {
            self.days.weekend_start = match rest.to_ascii_lowercase().as_str() {
                "friday" => 5,
                "saturday" => 6,
                _ => {
                    return Err(Reason {
                        line,
                        what: "a weekend",
                    });
                }
            };
            return Ok(());
        }
        if let Some(rest) = keyword(text, "section") {
            if rest.is_empty() {
                return Err(Reason {
                    line,
                    what: "an unknown line",
                });
            }
            self.events.push(Item::Section {
                text: rest.to_owned(),
                line,
            });
            return Ok(());
        }
        if let Some((label, meta)) = text.split_once(':') {
            return self.task(label, meta, line);
        }
        Err(Reason {
            line,
            what: "an unknown line",
        })
    }

    fn exclude(&mut self, rest: &str, line: usize) -> Result<(), Reason> {
        let mut any = false;
        for token in tokens(rest) {
            any = true;
            if token.eq_ignore_ascii_case("weekends") {
                self.days.weekends = true;
            } else if let Some(day) = weekday(token) {
                self.days.named[usize::from(day)] = true;
            } else if let Some(date) = civil(token) {
                self.days.dates.push(date);
            } else {
                return Err(Reason {
                    line,
                    what: "an excluded day",
                });
            }
        }
        if !any {
            return Err(Reason {
                line,
                what: "an excluded day",
            });
        }
        Ok(())
    }

    fn include(&mut self, rest: &str, line: usize) -> Result<(), Reason> {
        let mut any = false;
        for token in tokens(rest) {
            any = true;
            let Some(date) = civil(token) else {
                return Err(Reason {
                    line,
                    what: "an included day",
                });
            };
            self.days.included.push(date);
        }
        if !any {
            return Err(Reason {
                line,
                what: "an included day",
            });
        }
        Ok(())
    }

    fn task(&mut self, label: &str, meta: &str, line: usize) -> Result<(), Reason> {
        let label = label.trim();
        if label.is_empty() {
            return Err(Reason {
                line,
                what: "an unknown line",
            });
        }
        let mut parts: Vec<&str> = meta.split(',').map(str::trim).collect();
        let mut marks = Marks::default();
        let mut milestone = false;
        while let Some(part) = parts.first().copied() {
            if part.is_empty() {
                break;
            }
            match part.to_ascii_lowercase().as_str() {
                "done" => marks.done = true,
                "active" => marks.active = true,
                "crit" => marks.crit = true,
                "milestone" => milestone = true,
                "vert" => {
                    return Err(Reason {
                        line,
                        what: "a vertical line",
                    });
                }
                _ => break,
            }
            parts.remove(0);
        }
        let (key, start, end) = match parts.as_slice() {
            [end] => (None, Start::Previous, end_of(end, line)?),
            [start, end] => (None, start_of(start, line)?, end_of(end, line)?),
            [key, start, end] => (
                Some((*key).to_owned()),
                start_of(start, line)?,
                end_of(end, line)?,
            ),
            _ => {
                return Err(Reason {
                    line,
                    what: "an extra field",
                });
            }
        };
        if matches!(start, Start::Previous) && self.tasks.is_empty() {
            return Err(Reason {
                line,
                what: "a start",
            });
        }
        let key = match key {
            Some(key) if key.is_empty() => {
                return Err(Reason {
                    line,
                    what: "an extra field",
                });
            }
            Some(key) => key,
            None => {
                self.auto += 1;
                format!("task{}", self.auto)
            }
        };
        if self.tasks.iter().any(|task| task.key == key) {
            return Err(Reason {
                line,
                what: "a repeated task id",
            });
        }
        let index = self.tasks.len();
        self.tasks.push(Raw {
            key,
            label: label.to_owned(),
            line,
            milestone,
            marks,
            start,
            end,
        });
        self.events.push(Item::Task(index));
        Ok(())
    }

    fn finish(mut self) -> Result<Diagram, Reason> {
        self.days.prepare();
        let places = resolve(&self.tasks, &self.days, self.inclusive)?;
        let origin = places.iter().map(|place| place.start).min();
        if let Some(low) = origin {
            let high = places.iter().map(|place| place.end).max().unwrap_or(low);
            if high.saturating_sub(low) > MAX_DAYS {
                let index = (0..places.len())
                    .max_by_key(|&index| {
                        let place = &places[index];
                        place
                            .end
                            .saturating_sub(low)
                            .max(low.saturating_sub(place.start))
                    })
                    .unwrap_or(0);
                return Err(Reason {
                    line: self.tasks[index].line,
                    what: "a chart past 3660 days",
                });
            }
        }
        let base = origin.unwrap_or(0);
        let mut rows = Vec::new();
        for event in self.events {
            match event {
                Item::Section { text, line } => rows.push(Row::Section { text, line }),
                Item::Task(index) => {
                    let task = &self.tasks[index];
                    let place = &places[index];
                    rows.push(Row::Bar(Bar {
                        key: task.key.clone(),
                        label: task.label.clone(),
                        line: task.line,
                        start: place.start - base,
                        end: place.end - base,
                        shown_end: place.shown - base,
                        milestone: task.milestone,
                        color: None,
                        done: None,
                        marks: task.marks,
                        link_from: None,
                    }));
                }
            }
        }
        Ok(Diagram {
            rows,
            origin,
            closed: closed_days(&self.days, &places),
            zoom: 1.0,
            frame: Frame {
                title: self.title,
                ..Frame::default()
            },
        })
    }
}

fn closed_days(days: &Days, places: &[Place]) -> Vec<i64> {
    if !days.active() {
        return Vec::new();
    }
    let Some(low) = places.iter().map(|place| place.start).min() else {
        return Vec::new();
    };
    let high = places.iter().map(|place| place.end).max().unwrap_or(low);
    let mut closed = Vec::new();
    let mut day = low;
    while day < high {
        if days.skips(day) {
            closed.push(day);
        }
        day = match day.checked_add(1) {
            Some(next) => next,
            None => break,
        };
    }
    closed
}

fn start_of(text: &str, line: usize) -> Result<Start, Reason> {
    if let Some(rest) = keyword(text, "after") {
        return Ok(Start::After(ids(rest, line)?));
    }
    Ok(Start::Date(date_of(text, line)?))
}

fn end_of(text: &str, line: usize) -> Result<End, Reason> {
    if let Some(rest) = keyword(text, "until") {
        return Ok(End::Until(ids(rest, line)?));
    }
    if let Some(days) = duration(text, line)? {
        return Ok(End::Days(days));
    }
    Ok(End::Date(date_of(text, line)?))
}

fn ids(text: &str, line: usize) -> Result<Vec<String>, Reason> {
    let ids: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
    if ids.is_empty() {
        return Err(Reason {
            line,
            what: "a predecessor",
        });
    }
    Ok(ids)
}

/// Whole days, or `None` when `text` is not a duration.
/// A unit this day scale cannot place is an error.
fn duration(text: &str, line: usize) -> Result<Option<i64>, Reason> {
    let Some((number, unit)) = split_unit(text) else {
        return Ok(None);
    };
    let refuse = |what| Err(Reason { line, what });
    if !matches!(unit, "d" | "D" | "w" | "W") {
        return match unit {
            "h" | "H" => refuse("a duration in hours"),
            "m" => refuse("a duration in minutes"),
            "M" => refuse("a month"),
            "y" | "Y" => refuse("a year"),
            "s" | "S" => refuse("a duration in seconds"),
            "ms" | "MS" | "Ms" | "mS" => refuse("a duration in milliseconds"),
            _ => Ok(None),
        };
    }
    if number.contains('.') {
        return refuse("a fraction");
    }
    let Ok(n) = number.parse::<i64>() else {
        return refuse("a duration");
    };
    let days = if unit.eq_ignore_ascii_case("w") {
        n.checked_mul(7)
    } else {
        Some(n)
    };
    days.map(Some).ok_or(Reason {
        line,
        what: "a chart past 3660 days",
    })
}

fn split_unit(text: &str) -> Option<(&str, &str)> {
    let last = text.chars().next_back()?;
    if !last.is_ascii_alphabetic() {
        return None;
    }
    let unit_at = text.len() - last.len_utf8();
    if last.eq_ignore_ascii_case(&'s')
        && let Some(prev) = text[..unit_at].chars().next_back()
        && prev.eq_ignore_ascii_case(&'m')
    {
        let number = &text[..unit_at - prev.len_utf8()];
        if is_number(number) {
            return Some((number, &text[unit_at - prev.len_utf8()..]));
        }
    }
    let number = &text[..unit_at];
    is_number(number).then_some((number, &text[unit_at..]))
}

fn is_number(text: &str) -> bool {
    let mut dot = false;
    let mut digit = false;
    if text.is_empty() {
        return false;
    }
    for c in text.chars() {
        if c.is_ascii_digit() {
            digit = true;
        } else if c == '.' && !dot {
            dot = true;
        } else {
            return false;
        }
    }
    digit
}

fn date_of(text: &str, line: usize) -> Result<i64, Reason> {
    civil(text).ok_or(Reason {
        line,
        what: "a date",
    })
}

fn civil(text: &str) -> Option<i64> {
    let mut parts = text.split('-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    date::checked(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)
}

/// Monday is 1, Sunday is 7.
fn weekday(name: &str) -> Option<u8> {
    Some(match name.to_ascii_lowercase().as_str() {
        "monday" => 1,
        "tuesday" => 2,
        "wednesday" => 3,
        "thursday" => 4,
        "friday" => 5,
        "saturday" => 6,
        "sunday" => 7,
        _ => return None,
    })
}

fn tokens(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|token| !token.is_empty())
}

#[derive(Clone, Copy)]
struct Place {
    start: i64,
    end: i64,
    shown: i64,
}

fn resolve(tasks: &[Raw], days: &Days, inclusive: bool) -> Result<Vec<Place>, Reason> {
    let mut places = vec![None; tasks.len()];
    for _ in 0..=tasks.len() {
        let mut progress = false;
        for index in 0..tasks.len() {
            if places[index].is_some() {
                continue;
            }
            if let Some(place) = place_one(&tasks[index], tasks, &places, index, days, inclusive)? {
                places[index] = Some(place);
                progress = true;
            }
        }
        if places.iter().all(Option::is_some) {
            return Ok(places.into_iter().flatten().collect());
        }
        if !progress {
            let index = places.iter().position(Option::is_none).unwrap_or(0);
            return Err(Reason {
                line: tasks[index].line,
                what: "a cycle",
            });
        }
    }
    let index = places.iter().position(Option::is_none).unwrap_or(0);
    Err(Reason {
        line: tasks.get(index).map(|task| task.line).unwrap_or(0),
        what: "a cycle",
    })
}

fn place_one(
    task: &Raw,
    tasks: &[Raw],
    places: &[Option<Place>],
    index: usize,
    days: &Days,
    inclusive: bool,
) -> Result<Option<Place>, Reason> {
    let start = match &task.start {
        Start::Date(day) => Some(*day),
        Start::Previous => {
            if index == 0 {
                return Err(Reason {
                    line: task.line,
                    what: "a start",
                });
            }
            places[index - 1].map(|place| place.end)
        }
        Start::After(ids) => edge(ids, tasks, places, task.line, false)?,
    };
    let Some(start) = start else {
        return Ok(None);
    };
    let end = match &task.end {
        End::Date(day) => Some(*day),
        End::Days(days) => Some(start.checked_add(*days).ok_or(Reason {
            line: task.line,
            what: "a chart past 3660 days",
        })?),
        End::Until(ids) => edge(ids, tasks, places, task.line, true)?,
    };
    let Some(mut end) = end else {
        return Ok(None);
    };
    if end < start {
        return Err(Reason {
            line: task.line,
            what: "an end before the start",
        });
    }
    let manual = matches!(task.end, End::Date(_));
    let mut shown = end;
    if manual && inclusive {
        end = end.checked_add(1).ok_or(Reason {
            line: task.line,
            what: "a chart past 3660 days",
        })?;
        shown = end;
    }
    if !manual && days.active() {
        let (logical, paint) = days.stretch(start, end, task.line)?;
        end = logical;
        shown = paint;
    }
    Ok(Some(Place { start, end, shown }))
}

/// The latest end (`start` of an `after`) or the earliest start (`until`).
/// `None` while a named task is not placed yet.
fn edge(
    ids: &[String],
    tasks: &[Raw],
    places: &[Option<Place>],
    line: usize,
    until: bool,
) -> Result<Option<i64>, Reason> {
    let mut found: Option<i64> = None;
    for id in ids {
        let Some(index) = tasks.iter().position(|task| task.key == *id) else {
            return Err(Reason {
                line,
                what: "a missing task",
            });
        };
        let Some(place) = places[index] else {
            return Ok(None);
        };
        let time = if until { place.start } else { place.end };
        found = Some(match found {
            None => time,
            Some(got) if until => got.min(time),
            Some(got) => got.max(time),
        });
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::super::draw;
    use super::{from_mermaid, mermaid_refusal};
    use crate::gantt::{Bar, Diagram, Row};

    fn chart(source: &str) -> Diagram {
        from_mermaid(source)
            .unwrap_or_else(|| panic!("refused {source}: {:?}", mermaid_refusal(source)))
    }

    fn bar<'a>(diagram: &'a Diagram, label: &str) -> &'a Bar {
        diagram
            .rows
            .iter()
            .find_map(|row| match row {
                Row::Bar(bar) if bar.label == label => Some(bar),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no {label}"))
    }

    fn refuses(source: &str, line: usize, what: &str) {
        assert_eq!(mermaid_refusal(source), Some((line, what)), "{source}");
        assert!(from_mermaid(source).is_none(), "{source}");
    }

    #[test]
    fn the_fixture_schedules_ten_days_then_seven() {
        let source = include_str!("../../../omaspec-core/tests/fixtures/gantt.mmd");
        let diagram = chart(source);
        assert_eq!(
            diagram.frame.title.as_ref().map(|text| text.value.as_str()),
            Some("Omaspec 1.0")
        );
        let sections: Vec<_> = diagram
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::Section { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(sections, ["Core", "Editor"]);
        let origin = diagram.origin.unwrap();
        assert_eq!(super::date::format(origin), "2026-10-16");
        let lint = bar(&diagram, "Lint and outline");
        assert_eq!(lint.key, "a1");
        assert_eq!((lint.start, lint.end), (0, 10));
        assert!(lint.link_from.is_none());
        assert!(!lint.marks.done && !lint.marks.active && !lint.marks.crit);
        let next = bar(&diagram, "Highlighting");
        assert_eq!(next.key, "task1");
        assert_eq!((next.start, next.end), (10, 17));
        assert!(next.link_from.is_none());
        assert_eq!(super::date::format(origin + next.end), "2026-11-02");
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("Omaspec 1.0")
                && svg.contains("Lint and outline")
                && svg.contains("Highlighting")
                && svg.contains("2026-10-16"),
            "{svg}"
        );
    }

    #[test]
    fn dates_are_exclusive_and_dependencies_use_the_other_task() {
        let diagram = chart(
            "gantt\n\
             dateFormat YYYY-MM-DD\n\
             section One\n\
             Short :a, 2014-01-01, 2014-01-03\n\
             Week :b, 2014-01-01, 1w\n\
             Later :after a b, 1d\n\
             Hold :h, 2014-01-01, until m\n\
             section Two\n\
             Again :2d\n\
             Ship :milestone, m, 2014-01-04, 0d\n\
             Old :crit, done, c, 2014-01-01, 2d\n",
        );
        let origin = diagram.origin.unwrap();
        assert_eq!(super::date::format(origin), "2014-01-01");
        assert_eq!(
            (bar(&diagram, "Short").start, bar(&diagram, "Short").end),
            (0, 2)
        );
        assert_eq!(
            (bar(&diagram, "Week").start, bar(&diagram, "Week").end),
            (0, 7)
        );
        // `b` ends last, so `after a b` starts on day 7.
        assert_eq!(
            (bar(&diagram, "Later").start, bar(&diagram, "Later").end),
            (7, 8)
        );
        // `until m` ends where Ship starts, even though Ship is written later.
        // A new section does not break the chain: Again follows Hold.
        assert_eq!(
            (bar(&diagram, "Hold").start, bar(&diagram, "Hold").end),
            (0, 3)
        );
        let again = bar(&diagram, "Again");
        assert_eq!(again.key, "task2");
        assert_eq!((again.start, again.end), (3, 5));
        let old = bar(&diagram, "Old");
        assert!(old.marks.crit && old.marks.done && !old.marks.active);
        assert!(old.done.is_none());
        assert_eq!((old.start, old.end), (0, 2));
        let ship = bar(&diagram, "Ship");
        assert!(ship.milestone);
        assert_eq!(ship.key, "m");
        assert_eq!((ship.start, ship.end), (3, 3));
        let svg = draw(&diagram).unwrap();
        assert!(svg.contains("<polygon"), "{svg}");
    }

    #[test]
    fn a_click_and_the_front_matter_do_not_move_a_bar() {
        let diagram = chart(
            "---\n\
             title: Plan\n\
             ---\n\
             gantt\n\
             %% a note\n\
             accTitle: hidden\n\
             accDescr {\n\
               several lines\n\
             }\n\
             dateFormat YYYY-MM-DD\n\
             section A\n\
             One :a1, 2026-01-01, 1d %% trailing\n\
             click a1 href \"https://example.com\"\n\
             todayMarker off\n\
             topAxis\n",
        );
        assert_eq!(
            diagram.frame.title.as_ref().map(|text| text.value.as_str()),
            Some("Plan")
        );
        let one = bar(&diagram, "One");
        assert_eq!((one.start, one.end), (0, 1));
        assert_eq!(super::date::format(diagram.origin.unwrap()), "2026-01-01");
        let replaced = chart(
            "---\n\
             title: Plan\n\
             ---\n\
             gantt\n\
             title Body\n\
             dateFormat YYYY-MM-DD\n\
             One :2026-01-02, 1d\n",
        );
        assert_eq!(
            replaced
                .frame
                .title
                .as_ref()
                .map(|text| text.value.as_str()),
            Some("Body")
        );
    }

    #[test]
    fn weekends_are_columns_the_next_task_waits_out() {
        // 2026-01-05 is a Monday. Five days end on Friday; Saturday and
        // Sunday stay columns, and the next task starts on the 12th.
        let diagram = chart(
            "gantt\n\
             dateFormat YYYY-MM-DD\n\
             excludes weekends\n\
             Work :2026-01-05, 5d\n\
             Next :after task1, 1d\n",
        );
        let origin = diagram.origin.unwrap();
        let work = bar(&diagram, "Work");
        assert_eq!((work.start, work.shown_end, work.end), (0, 5, 7));
        assert_eq!(super::date::format(origin + work.end), "2026-01-12");
        let next = bar(&diagram, "Next");
        assert_eq!((next.start, next.shown_end, next.end), (7, 8, 8));
        assert_eq!(
            diagram
                .closed
                .iter()
                .map(|day| super::date::format(*day))
                .collect::<Vec<_>>(),
            ["2026-01-10", "2026-01-11"]
        );
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("Work") && svg.contains("width=\"80\"") && svg.contains("#D0D0D0"),
            "{svg}"
        );
    }

    #[test]
    fn an_included_saturday_is_a_working_day() {
        let diagram = chart(
            "gantt\n\
             dateFormat YYYY-MM-DD\n\
             excludes weekends\n\
             includes 2026-01-10\n\
             Work :2026-01-05, 5d\n",
        );
        let work = bar(&diagram, "Work");
        assert_eq!((work.start, work.shown_end, work.end), (0, 5, 5));
        assert!(diagram.closed.is_empty());
    }

    #[test]
    fn a_written_end_is_not_stretched_and_an_inclusive_one_keeps_its_day() {
        let open = chart(
            "gantt\n\
             dateFormat YYYY-MM-DD\n\
             excludes weekends\n\
             Work :2026-01-05, 2026-01-12\n",
        );
        let work = bar(&open, "Work");
        assert_eq!((work.shown_end, work.end), (7, 7));
        assert_eq!(open.closed.len(), 2);
        let kept = chart(
            "gantt\n\
             dateFormat YYYY-MM-DD\n\
             inclusiveEndDates\n\
             Day :2026-01-01, 2026-01-01\n",
        );
        assert_eq!((bar(&kept, "Day").start, bar(&kept, "Day").end), (0, 1));
        let plain = chart(
            "gantt\n\
             dateFormat YYYY-MM-DD\n\
             inclusiveEndDates\n\
             Day :2026-01-01, 1d\n",
        );
        assert_eq!((bar(&plain, "Day").start, bar(&plain, "Day").end), (0, 1));
    }

    #[test]
    fn friday_is_the_weekend_when_the_chart_says_so() {
        let diagram = chart(
            "gantt\n\
             dateFormat YYYY-MM-DD\n\
             weekend friday\n\
             excludes weekends\n\
             excludes sunday\n\
             Work :2026-01-05, 1d\n",
        );
        // Monday plus one day does not meet Friday. Sunday is named apart
        // from the weekend, and a one-day bar never reaches it.
        let work = bar(&diagram, "Work");
        assert_eq!((work.shown_end, work.end), (1, 1));
        assert!(diagram.closed.is_empty());
        let across = chart(
            "gantt\n\
             dateFormat YYYY-MM-DD\n\
             weekend friday\n\
             excludes weekends\n\
             Work :2026-01-09, 1d\n",
        );
        // Friday the 9th plus one day lands on Saturday, which is excluded,
        // so the bar stays on Friday and the next task waits until Sunday.
        let work = bar(&across, "Work");
        assert_eq!((work.shown_end, work.end), (1, 2));
        assert_eq!(
            super::date::format(across.origin.unwrap() + work.end),
            "2026-01-11"
        );
        assert_eq!(
            across
                .closed
                .iter()
                .map(|day| super::date::format(*day))
                .collect::<Vec<_>>(),
            ["2026-01-09", "2026-01-10"]
        );
    }

    #[test]
    fn a_weekday_does_not_move_a_day_column() {
        let plain = chart("gantt\ndateFormat YYYY-MM-DD\nWork :2026-01-05, 2d\n");
        let shifted = chart(
            "gantt\n\
             dateFormat YYYY-MM-DD\n\
             weekday monday\n\
             Work :2026-01-05, 2d\n",
        );
        assert_eq!(
            (
                bar(&plain, "Work").start,
                bar(&plain, "Work").end,
                bar(&plain, "Work").shown_end
            ),
            (
                bar(&shifted, "Work").start,
                bar(&shifted, "Work").end,
                bar(&shifted, "Work").shown_end
            )
        );
    }

    #[test]
    fn refuses_a_day_it_cannot_place() {
        refuses("gantt\nexcludes holidays\n", 1, "an excluded day");
        refuses("gantt\nexcludes\n", 1, "an excluded day");
        refuses("gantt\nincludes someday\n", 1, "an included day");
        refuses(
            "gantt\ninclusiveEndDates false\n",
            1,
            "an inclusive end date",
        );
        refuses("gantt\ndateFormat DD-MM-YYYY\n", 1, "a date format");
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\ndateFormat YYYY-MM-DD\n",
            2,
            "a second date format",
        );
        refuses("gantt\naxisFormat %d\n", 1, "an axis format");
        refuses("gantt\ntickInterval 1week\n", 1, "a tick interval");
        refuses("gantt\nweekday blursday\n", 1, "a weekday");
        refuses("gantt\nweekend sunday\n", 1, "a weekend");
        refuses(
            "---\ndisplayMode: compact\n---\ngantt\n",
            1,
            "a front matter setting",
        );
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nNap :2026-01-01, 12h\n",
            2,
            "a duration in hours",
        );
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nNap :2026-01-01, 30m\n",
            2,
            "a duration in minutes",
        );
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nLong :2026-01-01, 1M\n",
            2,
            "a month",
        );
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nHalf :2026-01-01, 1.5d\n",
            2,
            "a fraction",
        );
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nLine :vert, 2026-01-01, 1d\n",
            2,
            "a vertical line",
        );
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nA :2026-02-31, 1d\n",
            2,
            "a date",
        );
        refuses("gantt\ndateFormat YYYY-MM-DD\nA :5d\n", 2, "a start");
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nA :after z, 1d\n",
            2,
            "a missing task",
        );
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nA :a, after b, 1d\nB :b, after a, 1d\n",
            2,
            "a cycle",
        );
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nA :2026-01-05, 2026-01-01\n",
            2,
            "an end before the start",
        );
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nA :a1, 2026-01-01, 1d\nB :a1, 2026-01-02, 1d\n",
            3,
            "a repeated task id",
        );
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nA :2026-01-01, 3661d\n",
            2,
            "a chart past 3660 days",
        );
        refuses("gantt\nhello\n", 1, "an unknown line");
        refuses(
            "gantt\ndateFormat YYYY-MM-DD\nA :a1, 2026-01-01, 1d, extra\n",
            2,
            "an extra field",
        );
        assert!(from_mermaid("sequenceDiagram\nA->>B: hi\n").is_none());
        assert_eq!(mermaid_refusal("sequenceDiagram\nA->>B: hi\n"), None);
        assert!(from_mermaid("gantt\n").is_some());
    }
}
