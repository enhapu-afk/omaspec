//! From the body's lines to a [`Diagram`]. Dates are resolved before the
//! chart is returned, so the drawer only places bars.

use super::date;
use super::{Bar, Diagram, Marks, Row};
use crate::Declined;
use crate::color::{self, Color};
use crate::common::{Frame, keyword, snippet};
use crate::source::{Line, starts_with_ci};

pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut builder = Builder::empty();
    let mut at = 0;
    while at < lines.len() {
        if let Some(next) = builder.frame.line(lines, at)? {
            at = next;
            continue;
        }
        builder.statement(lines[at].text, lines[at].no)?;
        at += 1;
    }
    builder.finish()
}

struct Builder {
    tasks: Vec<Task>,
    events: Vec<Event>,
    project: Option<i64>,
    zoom: f32,
    zoom_set: bool,
    frame: Frame,
}

impl Builder {
    fn empty() -> Self {
        Self {
            tasks: Vec::new(),
            events: Vec::new(),
            project: None,
            zoom: 1.0,
            zoom_set: false,
            frame: Frame::default(),
        }
    }

    fn statement(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        // A section's title is text: `-- Closed beta --` closes no days.
        if let Some(title) = section(text) {
            self.events.push(Event::Section {
                text: title,
                line: no,
            });
            return Ok(());
        }
        banned(text, no)?;
        if let Some(rest) = keyword(text, "project") {
            return self.project_starts(rest, no);
        }
        if let Some(rest) = keyword(text, "printscale").or_else(|| keyword(text, "projectscale")) {
            if rest == "daily" {
                return Ok(());
            }
            return Err(fail(no, "only a daily scale is supported yet"));
        }
        if keyword(text, "ganttscale").is_some() {
            return Err(fail(no, "only a daily scale is supported yet"));
        }
        if keyword(text, "print").is_some() {
            return Err(fail(
                no,
                format!("`{}` is not supported yet", snippet(text)),
            ));
        }
        if let Some(rest) = keyword(text, "zoom") {
            return self.zoom(rest, no);
        }
        if let Some((from, to)) = arrow_of(text) {
            return self.arrow(&from, &to, no);
        }
        if text.starts_with('[') {
            return self.tasks_then(text, no);
        }
        Err(fail(
            no,
            format!("`{}` is not supported yet", snippet(text)),
        ))
    }

    fn project_starts(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        let rest = keyword(rest, "starts").ok_or_else(|| fail(no, "`project` needs `starts`"))?;
        let rest = keyword(rest, "at").unwrap_or(rest);
        let (when, rest) = parse_when(rest, no)?;
        if !rest.is_empty() {
            return Err(fail(
                no,
                format!("`{}` is not supported yet", snippet(rest)),
            ));
        }
        if self.project.is_some() {
            return Err(fail(no, "the project already has a start"));
        }
        match when {
            When::Date(date) => self.project = Some(date),
            When::Day(_) => return Err(fail(no, "the project start needs a date")),
        }
        Ok(())
    }

    fn zoom(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        let (n, rest) = leading_int(rest, no)?;
        if !rest.is_empty() {
            return Err(fail(
                no,
                format!("`{}` is not supported yet", snippet(rest)),
            ));
        }
        if n < 1 {
            return Err(fail(no, "zoom needs a whole number from 1 up"));
        }
        if self.zoom_set {
            return Err(fail(no, "zoom is already set"));
        }
        self.zoom = n as f32;
        self.zoom_set = true;
        Ok(())
    }

    fn tasks_then(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        let mut previous: Option<String> = None;
        for part in split_then(text) {
            let key = self.one_task(part, no, previous.as_deref())?;
            previous = Some(key);
        }
        Ok(())
    }

    fn one_task(
        &mut self,
        text: &str,
        no: usize,
        previous: Option<&str>,
    ) -> Result<String, Declined> {
        let (label, mut rest) = bracket(text, no)?;
        let alias = if let Some(next) = keyword(rest, "as") {
            let (name, next) = bracket(next, no)?;
            rest = next;
            Some(name)
        } else {
            None
        };
        let index = self.touch(label, alias, no)?;
        let follow = previous.map(str::to_owned);
        let task = &mut self.tasks[index];
        clauses(task, rest, no)?;
        if let Some(key) = follow
            && task.start.is_none()
            && !task.milestone
        {
            task.start = Some(Anchor::After {
                key,
                edge: Edge::End,
            });
        }
        Ok(self.tasks[index].key.clone())
    }

    fn touch(
        &mut self,
        label: String,
        alias: Option<String>,
        no: usize,
    ) -> Result<usize, Declined> {
        if let Some(alias) = alias {
            if let Some(index) = self.tasks.iter().position(|task| task.key == alias) {
                return Ok(index);
            }
            return Ok(self.push_task(alias, label, no));
        }
        if let Some(index) = self.find(&label) {
            return Ok(index);
        }
        let key = label.clone();
        Ok(self.push_task(key, label, no))
    }

    fn push_task(&mut self, key: String, label: String, no: usize) -> usize {
        let index = self.tasks.len();
        self.tasks.push(Task::new(key, label, no));
        self.events.push(Event::Task(index));
        index
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.tasks
            .iter()
            .position(|task| task.key == name || task.label == name)
    }

    fn arrow(&mut self, from_name: &str, to_name: &str, no: usize) -> Result<(), Declined> {
        let from = self
            .find(from_name)
            .ok_or_else(|| fail(no, format!("no task `{from_name}`")))?;
        let to = self
            .find(to_name)
            .ok_or_else(|| fail(no, format!("no task `{to_name}`")))?;
        if from == to {
            return Err(fail(no, "a task cannot follow itself"));
        }
        let pred = self.tasks[from].key.clone();
        let task = &mut self.tasks[to];
        if task.milestone {
            return Err(fail(no, "a milestone has a duration"));
        }
        let same = match &task.start {
            None => false,
            Some(Anchor::After {
                key,
                edge: Edge::End,
            }) if key == &pred => true,
            Some(_) => return Err(fail(no, "this task already has a start")),
        };
        if let Some(existing) = &task.link_from
            && existing != &pred
        {
            return Err(fail(no, "this task already has a link"));
        }
        if !same {
            task.start = Some(Anchor::After {
                key: pred.clone(),
                edge: Edge::End,
            });
        }
        task.link_from = Some(pred);
        Ok(())
    }

    fn finish(self) -> Result<Diagram, Declined> {
        let (solved, origin) = solve(&self.tasks, self.project)?;
        check_span(&self.tasks, &solved)?;
        let mut rows = Vec::new();
        for event in self.events {
            match event {
                Event::Section { text, line } => rows.push(Row::Section { text, line }),
                Event::Task(index) => {
                    let task = &self.tasks[index];
                    let place = &solved[index];
                    rows.push(Row::Bar(Bar {
                        key: task.key.clone(),
                        label: task.label.clone(),
                        line: task.line,
                        start: place.start,
                        end: place.end,
                        shown_end: place.end,
                        milestone: task.milestone,
                        color: task.color,
                        done: task.done,
                        marks: Marks::default(),
                        link_from: task.link_from.clone(),
                    }));
                }
            }
        }
        Ok(Diagram {
            rows,
            origin,
            closed: Vec::new(),
            zoom: self.zoom,
            frame: self.frame,
        })
    }
}

/// `Frame::line` wants a slice. A one-line slice keeps the source line
/// number, which lives on [`Line`], not on the slice index.
fn banned(text: &str, no: usize) -> Result<(), Declined> {
    let outside = outside_brackets(text).to_ascii_lowercase();
    if outside.contains("closed") {
        return Err(fail(no, "closed days are not supported yet"));
    }
    if !text.starts_with('[') && (outside.contains("are colored") || outside.contains("is colored"))
    {
        return Err(fail(no, "colored days are not supported yet"));
    }
    if outside.contains("week numbering") || keyword(text, "weeks").is_some() {
        return Err(fail(no, "week numbering is not supported yet"));
    }
    if outside.contains("links to") || text.contains("[[") {
        return Err(fail(no, "links are not supported yet"));
    }
    if outside.contains('{') {
        return Err(fail(no, "resources are not supported yet"));
    }
    Ok(())
}

fn outside_brackets(text: &str) -> String {
    let mut out = String::new();
    let mut depth: usize = 0;
    for c in text.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn section(text: &str) -> Option<String> {
    let text = text.trim();
    if text.len() < 4 || !text.starts_with("--") || !text.ends_with("--") {
        return None;
    }
    let inner = text.trim_matches('-').trim();
    (!inner.is_empty()).then(|| inner.to_owned())
}

struct Task {
    key: String,
    label: String,
    line: usize,
    start: Option<Anchor>,
    /// Inclusive last day, as written with `ends`.
    end: Option<Anchor>,
    duration: Option<i64>,
    happens: Vec<Anchor>,
    milestone: bool,
    color: Option<Color>,
    done: Option<f32>,
    link_from: Option<String>,
}

impl Task {
    fn new(key: String, label: String, line: usize) -> Self {
        Self {
            key,
            label,
            line,
            start: None,
            end: None,
            duration: None,
            happens: Vec::new(),
            milestone: false,
            color: None,
            done: None,
            link_from: None,
        }
    }
}

enum Event {
    Section { text: String, line: usize },
    Task(usize),
}

enum When {
    Day(i64),
    Date(i64),
}

enum Edge {
    Start,
    End,
}

enum Anchor {
    Fixed(When),
    After { key: String, edge: Edge },
}

fn clauses(task: &mut Task, mut rest: &str, no: usize) -> Result<(), Declined> {
    while !rest.is_empty() {
        let before = rest.len();
        rest = if let Some(next) = keyword(rest, "and") {
            next
        } else if let Some(next) = keyword(rest, "requires").or_else(|| keyword(rest, "lasts")) {
            let (days, next) = duration(next, no)?;
            set_duration(task, days, no)?;
            next
        } else if let Some(next) = keyword(rest, "starts") {
            let (anchor, next) = when_anchor(next, no)?;
            set_start(task, anchor, no)?;
            next
        } else if let Some(next) = keyword(rest, "ends") {
            let (anchor, next) = end_anchor(next, no)?;
            set_end(task, anchor, no)?;
            next
        } else if let Some(next) = keyword(rest, "happens") {
            let (anchor, next) = when_anchor(next, no)?;
            push_happens(task, anchor, no)?;
            next
        } else if let Some(next) = keyword(rest, "is") {
            is_clause(task, next, no)?
        } else {
            return Err(fail(
                no,
                format!("`{}` is not supported yet", snippet(rest)),
            ));
        };
        if rest.len() >= before {
            return Err(fail(
                no,
                format!("`{}` is not supported yet", snippet(rest)),
            ));
        }
    }
    Ok(())
}

fn set_duration(task: &mut Task, days: i64, no: usize) -> Result<(), Declined> {
    if task.milestone {
        return Err(fail(no, "a milestone has a duration"));
    }
    if task.duration.is_some() {
        return Err(fail(no, "the duration is already set"));
    }
    task.duration = Some(days);
    Ok(())
}

fn set_start(task: &mut Task, anchor: Anchor, no: usize) -> Result<(), Declined> {
    if task.milestone {
        return Err(fail(no, "a milestone has a duration"));
    }
    if task.start.is_some() {
        return Err(fail(no, "this task already has a start"));
    }
    task.start = Some(anchor);
    Ok(())
}

fn set_end(task: &mut Task, anchor: Anchor, no: usize) -> Result<(), Declined> {
    if task.milestone {
        return Err(fail(no, "a milestone has a duration"));
    }
    if task.end.is_some() {
        return Err(fail(no, "the end is already set"));
    }
    task.end = Some(anchor);
    Ok(())
}

fn push_happens(task: &mut Task, anchor: Anchor, no: usize) -> Result<(), Declined> {
    if task.duration.is_some() || task.start.is_some() || task.end.is_some() {
        return Err(fail(no, "a milestone has a duration"));
    }
    task.milestone = true;
    task.happens.push(anchor);
    Ok(())
}

fn is_clause<'a>(task: &mut Task, rest: &'a str, no: usize) -> Result<&'a str, Declined> {
    if let Some(rest) = keyword(rest, "colored") {
        let rest = keyword(rest, "in").ok_or_else(|| fail(no, "`colored` needs `in`"))?;
        let (token, rest) = split_word(rest);
        if token.is_empty() {
            return Err(fail(no, "a color is missing"));
        }
        let color = color::parse(token)
            .map_err(|reason| fail(no, reason))?
            .ok_or_else(|| fail(no, "a color must not be transparent"))?;
        if task.color.is_some() {
            return Err(fail(no, "the color is already set"));
        }
        task.color = Some(color);
        return Ok(rest);
    }
    let (token, after) = split_word(rest);
    if let Some(number) = token.strip_suffix('%') {
        if number.contains('.') || number.is_empty() {
            return Err(fail(no, "only whole numbers are supported yet"));
        }
        let percent: f32 = number
            .parse()
            .map_err(|_| fail(no, "only whole numbers are supported yet"))?;
        if !(0.0..=100.0).contains(&percent) {
            return Err(fail(no, "completion has to be from 0% to 100%"));
        }
        let after = keyword(after, "completed")
            .or_else(|| keyword(after, "complete"))
            .ok_or_else(|| fail(no, "completion needs `complete`"))?;
        if task.done.is_some() {
            return Err(fail(no, "completion is already set"));
        }
        task.done = Some(percent / 100.0);
        return Ok(after);
    }
    Err(fail(
        no,
        format!("`{}` is not supported yet", snippet(rest)),
    ))
}

fn when_anchor(rest: &str, no: usize) -> Result<(Anchor, &str), Declined> {
    let rest = keyword(rest, "at").unwrap_or(rest);
    if rest.starts_with('[') {
        task_anchor(rest, no)
    } else {
        let (when, rest) = parse_when(rest, no)?;
        Ok((Anchor::Fixed(when), rest))
    }
}

fn end_anchor(rest: &str, no: usize) -> Result<(Anchor, &str), Declined> {
    let rest = keyword(rest, "at").unwrap_or(rest);
    if rest.starts_with('[') {
        return Err(fail(no, "ending at another task is not supported yet"));
    }
    let (when, rest) = parse_when(rest, no)?;
    Ok((Anchor::Fixed(when), rest))
}

fn task_anchor(text: &str, no: usize) -> Result<(Anchor, &str), Declined> {
    let (name, rest) = bracket(text, no)?;
    let rest = rest
        .strip_prefix("'s")
        .ok_or_else(|| fail(no, "a link needs `'s start` or `'s end`"))?;
    let rest = rest.trim_start();
    let (edge, rest) = if let Some(rest) = keyword(rest, "start") {
        (Edge::Start, rest)
    } else if let Some(rest) = keyword(rest, "end") {
        (Edge::End, rest)
    } else {
        return Err(fail(no, "a link needs `'s start` or `'s end`"));
    };
    Ok((Anchor::After { key: name, edge }, rest))
}

fn duration(text: &str, no: usize) -> Result<(i64, &str), Declined> {
    let (mut total, mut rest) = one_span(text, no)?;
    // `and` continues a duration only when the next token is a number
    // (`1 week and 4 days`). `and is colored` stays for the clause loop.
    while let Some(next) = keyword(rest, "and") {
        if !next.starts_with(|c: char| c.is_ascii_digit()) {
            break;
        }
        let (more, next) = one_span(next, no)?;
        total = total
            .checked_add(more)
            .ok_or_else(|| fail(no, "this duration is too long"))?;
        rest = next;
    }
    Ok((total, rest))
}

fn one_span(text: &str, no: usize) -> Result<(i64, &str), Declined> {
    let (count, rest) = leading_int(text, no)?;
    if keyword(rest, "months").is_some() || keyword(rest, "month").is_some() {
        return Err(fail(no, "months are not supported yet"));
    }
    let (factor, rest) = if let Some(rest) = keyword(rest, "weeks") {
        (7, rest)
    } else if let Some(rest) = keyword(rest, "week") {
        (7, rest)
    } else if let Some(rest) = keyword(rest, "days") {
        (1, rest)
    } else if let Some(rest) = keyword(rest, "day") {
        (1, rest)
    } else {
        return Err(fail(no, "a duration needs days or weeks"));
    };
    let days = count
        .checked_mul(factor)
        .ok_or_else(|| fail(no, "this duration is too long"))?;
    Ok((days, rest))
}

fn leading_int(text: &str, no: usize) -> Result<(i64, &str), Declined> {
    let text = text.trim_start();
    if text.starts_with('-') {
        return Err(fail(no, "a negative number is not supported yet"));
    }
    let bytes = text.as_bytes();
    let mut end = 0;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == 0 {
        return Err(fail(no, "a number is missing"));
    }
    if bytes.get(end) == Some(&b'.') {
        return Err(fail(no, "only whole numbers are supported yet"));
    }
    let value: i64 = text[..end]
        .parse()
        .map_err(|_| fail(no, "this number is too big"))?;
    Ok((value, text[end..].trim_start()))
}

fn parse_when(text: &str, no: usize) -> Result<(When, &str), Declined> {
    let text = text.trim_start();
    if let Some(rest) = strip_dplus(text) {
        let (day, rest) = leading_int(rest, no)?;
        return Ok((When::Day(day), rest));
    }
    let (token, rest) = split_word(text);
    if token_is_iso(token) {
        return Ok((When::Date(iso_date(token, no)?), rest));
    }
    if looks_natural(text) {
        let (date, rest) = natural(text, no)?;
        return Ok((When::Date(date), rest));
    }
    Err(fail(no, "this date is not supported yet"))
}

fn strip_dplus(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(['D', 'd'])?;
    rest.strip_prefix('+')
}

fn token_is_iso(token: &str) -> bool {
    token.starts_with(|c: char| c.is_ascii_digit()) && token.contains(['-', '/'])
}

fn iso_date(token: &str, no: usize) -> Result<i64, Declined> {
    let sep = if token.contains('-') { '-' } else { '/' };
    let mut parts = token.split(sep);
    let year = parts.next().unwrap_or("");
    let month = parts.next().unwrap_or("");
    let day = parts.next().unwrap_or("");
    if parts.next().is_some() || year.len() != 4 {
        return Err(fail(no, "this date is not supported yet"));
    }
    let year: i32 = year
        .parse()
        .map_err(|_| fail(no, "this date is not supported yet"))?;
    let month: u32 = month
        .parse()
        .map_err(|_| fail(no, "this date is not supported yet"))?;
    let day: u32 = day
        .parse()
        .map_err(|_| fail(no, "this date is not supported yet"))?;
    date::checked(year, month, day).ok_or_else(|| fail(no, "this date is not a real day"))
}

fn looks_natural(text: &str) -> bool {
    let rest = keyword(text, "the").unwrap_or(text);
    let (token, _) = split_word(rest);
    ordinal(token).is_some()
}

fn natural(text: &str, no: usize) -> Result<(i64, &str), Declined> {
    let rest = keyword(text, "the").unwrap_or(text);
    let (day_token, rest) = split_word(rest);
    let day = ordinal(day_token).ok_or_else(|| fail(no, "this date is not supported yet"))?;
    let rest = keyword(rest, "of").ok_or_else(|| fail(no, "this date is not supported yet"))?;
    let (month_word, rest) = split_word(rest);
    let month =
        month_index(month_word).ok_or_else(|| fail(no, "this date is not supported yet"))?;
    let (year_word, rest) = split_word(rest);
    let year: i32 = year_word
        .parse()
        .map_err(|_| fail(no, "this date is not supported yet"))?;
    let date =
        date::checked(year, month, day).ok_or_else(|| fail(no, "this date is not a real day"))?;
    Ok((date, rest))
}

fn ordinal(token: &str) -> Option<u32> {
    let digits = token.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let suffix = &token[digits.len()..];
    if !suffix.is_empty()
        && !matches!(
            suffix.to_ascii_lowercase().as_str(),
            "st" | "nd" | "rd" | "th"
        )
    {
        return None;
    }
    digits.parse().ok()
}

fn month_index(word: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    MONTHS
        .iter()
        .position(|name| word.eq_ignore_ascii_case(name))
        .map(|index| index as u32 + 1)
}

fn split_word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (&text[..end], text[end..].trim_start())
}

fn bracket(text: &str, no: usize) -> Result<(String, &str), Declined> {
    let text = text.trim_start();
    let rest = text
        .strip_prefix('[')
        .ok_or_else(|| fail(no, "a task needs `[name]`"))?;
    let mut depth = 1;
    for (index, c) in rest.char_indices() {
        if c == '[' {
            depth += 1;
        } else if c == ']' {
            depth -= 1;
            if depth == 0 {
                let name = rest[..index].trim();
                if name.is_empty() {
                    return Err(fail(no, "a task needs a name"));
                }
                return Ok((name.to_owned(), rest[index + c.len_utf8()..].trim_start()));
            }
        }
    }
    Err(fail(no, "a bracket is not closed"))
}

fn split_then(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth: usize = 0;
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap_or(' ');
        if c == '[' {
            depth += 1;
        } else if c == ']' {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && starts_with_ci(&text[i..], " then ") {
            let part = text[start..i].trim();
            if !part.is_empty() {
                parts.push(part);
            }
            i += " then ".len();
            start = i;
            continue;
        }
        i += c.len_utf8();
    }
    let part = text[start..].trim();
    if !part.is_empty() {
        parts.push(part);
    }
    parts
}

fn split_once_outside<'a>(text: &'a str, needle: &str) -> Option<(&'a str, &'a str)> {
    let mut depth: usize = 0;
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap_or(' ');
        if c == '[' {
            depth += 1;
        } else if c == ']' {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && text[i..].starts_with(needle) {
            return Some((&text[..i], &text[i + needle.len()..]));
        }
        i += c.len_utf8();
    }
    None
}

fn arrow_of(text: &str) -> Option<(String, String)> {
    let (left, right) = split_once_outside(text, "->")?;
    let (from, left_rest) = bracket(left, 0).ok()?;
    let (to, right_rest) = bracket(right, 0).ok()?;
    (left_rest.is_empty() && right_rest.is_empty()).then_some((from, to))
}

struct Solved {
    start: i64,
    end: i64,
}

fn solve(tasks: &[Task], project: Option<i64>) -> Result<(Vec<Solved>, Option<i64>), Declined> {
    let origin = origin_of(project, tasks);
    let mut solved: Vec<Option<Solved>> = (0..tasks.len()).map(|_| None).collect();
    for _ in 0..=tasks.len() {
        let mut progress = false;
        for index in 0..tasks.len() {
            if solved[index].is_some() {
                continue;
            }
            if let Some(done) = solve_one(&tasks[index], tasks, &solved, origin, index)? {
                solved[index] = Some(done);
                progress = true;
            }
        }
        if solved.iter().all(Option::is_some) {
            return Ok((solved.into_iter().flatten().collect(), origin));
        }
        if !progress {
            let index = solved.iter().position(Option::is_none).unwrap_or(0);
            return Err(fail(tasks[index].line, "the tasks depend on each other"));
        }
    }
    let index = solved.iter().position(Option::is_none).unwrap_or(0);
    Err(fail(
        tasks.get(index).map(|task| task.line).unwrap_or(0),
        "the tasks depend on each other",
    ))
}

fn solve_one(
    task: &Task,
    tasks: &[Task],
    solved: &[Option<Solved>],
    origin: Option<i64>,
    index: usize,
) -> Result<Option<Solved>, Declined> {
    if task.milestone {
        if task.duration.is_some() || task.start.is_some() || task.end.is_some() {
            return Err(fail(task.line, "a milestone has a duration"));
        }
        if task.happens.is_empty() {
            return Err(fail(task.line, "a milestone needs a date"));
        }
        let mut best = None;
        for anchor in &task.happens {
            let Some(time) = anchor_time(anchor, tasks, solved, origin, index, task.line)? else {
                return Ok(None);
            };
            best = Some(best.map_or(time, |got: i64| got.max(time)));
        }
        let time = best.ok_or_else(|| fail(task.line, "a milestone needs a date"))?;
        return Ok(Some(Solved {
            start: time,
            end: time,
        }));
    }
    let start = ready(&task.start, tasks, solved, origin, index, task.line)?;
    let end = ready(&task.end, tasks, solved, origin, index, task.line)?;
    if (start.is_none() && task.start.is_some()) || (end.is_none() && task.end.is_some()) {
        return Ok(None);
    }
    Ok(Some(schedule(task, start, end)?))
}

/// `None` while a predecessor is still open. An absent anchor is not pending.
fn ready(
    anchor: &Option<Anchor>,
    tasks: &[Task],
    solved: &[Option<Solved>],
    origin: Option<i64>,
    index: usize,
    line: usize,
) -> Result<Option<i64>, Declined> {
    match anchor {
        None => Ok(None),
        Some(anchor) => anchor_time(anchor, tasks, solved, origin, index, line),
    }
}

fn anchor_time(
    anchor: &Anchor,
    tasks: &[Task],
    solved: &[Option<Solved>],
    origin: Option<i64>,
    index: usize,
    line: usize,
) -> Result<Option<i64>, Declined> {
    match anchor {
        Anchor::Fixed(when) => Ok(Some(when_index(when, origin, line)?)),
        Anchor::After { key, edge } => {
            let pred = tasks
                .iter()
                .position(|task| task.key == *key || task.label == *key)
                .ok_or_else(|| fail(line, format!("no task `{key}`")))?;
            if pred == index {
                return Err(fail(line, "a task cannot follow itself"));
            }
            let Some(done) = &solved[pred] else {
                return Ok(None);
            };
            Ok(Some(match edge {
                Edge::Start => done.start,
                Edge::End => done.end,
            }))
        }
    }
}

fn when_index(when: &When, origin: Option<i64>, line: usize) -> Result<i64, Declined> {
    match *when {
        When::Day(day) => Ok(day),
        When::Date(date) => {
            let origin =
                origin.ok_or_else(|| fail(line, "this task's date cannot be worked out"))?;
            Ok(date - origin)
        }
    }
}

fn schedule(task: &Task, start: Option<i64>, inclusive: Option<i64>) -> Result<Solved, Declined> {
    let exclusive = match inclusive {
        Some(day) => Some(
            day.checked_add(1)
                .ok_or_else(|| fail(task.line, "this duration is too long"))?,
        ),
        None => None,
    };
    let (start, end) = match (start, exclusive, task.duration) {
        (Some(start), Some(end), Some(days)) => {
            let got = start
                .checked_add(days)
                .ok_or_else(|| fail(task.line, "this duration is too long"))?;
            if got != end {
                return Err(fail(
                    task.line,
                    "the start, the end and the duration disagree",
                ));
            }
            (start, end)
        }
        (Some(start), Some(end), None) => (start, end),
        (Some(start), None, Some(days)) => (
            start,
            start
                .checked_add(days)
                .ok_or_else(|| fail(task.line, "this duration is too long"))?,
        ),
        (None, Some(end), Some(days)) => (
            end.checked_sub(days)
                .ok_or_else(|| fail(task.line, "this duration is too long"))?,
            end,
        ),
        (None, None, Some(days)) => (0, days),
        _ => return Err(fail(task.line, "this task needs a duration")),
    };
    if end < start {
        return Err(fail(task.line, "the end is before the start"));
    }
    Ok(Solved { start, end })
}

/// The most days a chart may span, day 0 included: ten years. The chart
/// draws a line for every day, so a date typed as `D+20200701` would
/// otherwise take minutes and gigabytes.
const MAX_DAYS: i64 = 3660;

/// Declines a chart wider than [`MAX_DAYS`], naming the task farthest
/// from day 0.
fn check_span(tasks: &[Task], solved: &[Solved]) -> Result<(), Declined> {
    let low = solved.iter().map(|s| s.start).fold(0, i64::min);
    let high = solved.iter().map(|s| s.end).fold(0, i64::max);
    if high.saturating_sub(low) <= MAX_DAYS {
        return Ok(());
    }
    let far = |s: &Solved| s.start.unsigned_abs().max(s.end.unsigned_abs());
    let index = (0..solved.len())
        .max_by_key(|&index| far(&solved[index]))
        .unwrap_or(0);
    Err(fail(
        tasks[index].line,
        format!("a chart spans at most {MAX_DAYS} days, and this task is outside them"),
    ))
}

fn origin_of(project: Option<i64>, tasks: &[Task]) -> Option<i64> {
    if let Some(project) = project {
        return Some(project);
    }
    let mut earliest = None;
    for task in tasks {
        for anchor in task.start.iter().chain(&task.end).chain(&task.happens) {
            if let Anchor::Fixed(When::Date(date)) = anchor {
                earliest = Some(earliest.map_or(*date, |got: i64| got.min(*date)));
            }
        }
    }
    earliest
}

fn fail(line: usize, reason: impl Into<String>) -> Declined {
    Declined::at(line, reason)
}

#[cfg(test)]
mod tests {
    use super::date;
    use super::*;
    use crate::Diagram as Kind;
    use crate::color::Color;

    fn gantt(source: &str) -> Diagram {
        match crate::parse(source).unwrap() {
            Kind::Gantt(diagram) => diagram,
            other => panic!("not gantt: {other:?}"),
        }
    }

    fn bar(diagram: &Diagram, index: usize) -> &Bar {
        match &diagram.rows[index] {
            Row::Bar(bar) => bar,
            other => panic!("not a bar: {other:?}"),
        }
    }

    #[test]
    fn fifteen_days_from_july_end_on_the_sixteenth() {
        let diagram = gantt(
            "@startgantt\n\
             Project starts 2020-07-01\n\
             [Design] starts 2020-07-01 and requires 15 days\n\
             @endgantt\n",
        );
        let design = bar(&diagram, 0);
        assert_eq!(design.start, 0);
        assert_eq!(design.end, 15);
        let origin = diagram.origin.unwrap();
        assert_eq!(date::format(origin), "2020-07-01");
        assert_eq!(date::format(origin + design.end), "2020-07-16");
    }

    #[test]
    fn an_inclusive_end_lasts_one_day_longer() {
        let diagram = gantt(
            "@startgantt\n\
             Project starts 2020-07-01\n\
             [A] starts D+0 and ends D+14\n\
             @endgantt\n",
        );
        let task = bar(&diagram, 0);
        assert_eq!((task.start, task.end), (0, 15));
    }

    #[test]
    fn then_schedules_and_an_arrow_links() {
        let diagram = gantt(
            "@startgantt\n\
             [A] requires 4 days then [B] requires 4 days\n\
             [C] requires 4 days\n\
             [A] -> [C]\n\
             @endgantt\n",
        );
        assert_eq!((bar(&diagram, 0).start, bar(&diagram, 0).end), (0, 4));
        assert_eq!((bar(&diagram, 1).start, bar(&diagram, 1).end), (4, 8));
        assert!(bar(&diagram, 1).link_from.is_none());
        assert_eq!((bar(&diagram, 2).start, bar(&diagram, 2).end), (4, 8));
        assert_eq!(bar(&diagram, 2).link_from.as_deref(), Some("A"));
    }

    #[test]
    fn a_milestone_takes_the_latest_moment() {
        let diagram = gantt(
            "@startgantt\n\
             [A] requires 5 days\n\
             [M] happens at [A]'s start\n\
             [M] happens at [A]'s end\n\
             @endgantt\n",
        );
        let milestone = bar(&diagram, 1);
        assert!(milestone.milestone);
        assert_eq!(milestone.start, 5);
        assert_eq!(milestone.end, 5);
    }

    #[test]
    fn weeks_add_and_undated_tasks_share_day_zero() {
        let diagram = gantt(
            "@startgantt\n\
             [Week] requires 1 week\n\
             [Span] requires 1 week and 4 days\n\
             [A] requires 3 days\n\
             [B] requires 2 days\n\
             @endgantt\n",
        );
        assert_eq!(bar(&diagram, 0).end, 7);
        assert_eq!(bar(&diagram, 1).end, 11);
        assert_eq!(bar(&diagram, 2).start, 0);
        assert_eq!(bar(&diagram, 3).start, 0);
        assert!(diagram.origin.is_none());
    }

    #[test]
    fn a_written_date_is_the_origin() {
        let diagram = gantt(
            "@startgantt\n\
             Project starts the 20th of september 2017\n\
             [A] requires 1 day\n\
             [A] is 40% complete\n\
             [A] is colored in Fuchsia\n\
             @endgantt\n",
        );
        assert_eq!(date::format(diagram.origin.unwrap()), "2017-09-20");
        let task = bar(&diagram, 0);
        assert_eq!(task.color, Some(Color::hex(0xFF00FF)));
        assert!((task.done.unwrap() - 0.4).abs() < 1e-5);
    }

    #[test]
    fn a_section_title_is_only_text() {
        let diagram = gantt(
            "@startgantt\n-- Closed beta --\n[A] requires 2 days then [B] requires 1 day\n@endgantt\n",
        );
        assert!(matches!(&diagram.rows[0], Row::Section { text, .. } if text == "Closed beta"));
        assert_eq!(bar(&diagram, 2).start, 2);
    }

    #[test]
    fn a_chart_spans_at_most_ten_years() {
        let wide = |task: &str| {
            crate::render(&format!(
                "@startgantt\n[A] requires 3 days\n{task}\n@endgantt\n"
            ))
        };
        assert!(wide("[B] requires 3600 days").is_ok());
        for task in [
            "[B] requires 200000 days",
            "[B] requires 1000000000 days",
            "[M] happens D+20200701",
            "[M] happens D+9223372036854775807",
        ] {
            let err = wide(task).unwrap_err();
            assert_eq!(err.line, Some(2), "{task}");
            assert!(err.reason.contains("3660 days"), "{}", err.reason);
        }
    }

    #[test]
    fn closed_days_and_a_bad_date_name_the_line() {
        let err = crate::parse("@startgantt\nsaturday are closed\n@endgantt\n").unwrap_err();
        assert_eq!(err.line, Some(1));
        assert!(err.reason.contains("closed"), "{}", err.reason);
        let err = crate::parse("@startgantt\nProject starts 2019-02-29\n@endgantt\n").unwrap_err();
        assert_eq!(err.line, Some(1));
        assert!(err.reason.contains("real day"), "{}", err.reason);
    }
}
