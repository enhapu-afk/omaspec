//! Boxes written where they stand: a node of a PlantUML mind map or WBS
//! takes its color after its markers (`**[#f7768e] Tests`), an action
//! before its colon (`#f7768e:work;`), a swimlane before its name
//! (`|#f7768e|Kitchen|`). A Gantt task takes it as a clause of its own,
//! `[Design] is colored in #f7768e`.

use std::ops::Range;

use omaspec_uml::Diagram;
use omaspec_uml::activity::{action_end, action_start};

use super::{Edit, Target, changed, indent_of, short, split_newline};
use crate::text::{line_ranges, re};

/// The lines that hold the diagram's elements, each with its zero-based
/// line: without notes, legends, titles and comments.
fn element_lines(code: &str) -> Vec<(usize, &str)> {
    match omaspec_uml::element_lines(code) {
        Some(lines) => lines.into_iter().map(|line| (line.no, line.text)).collect(),
        None => code.lines().map(str::trim).enumerate().collect(),
    }
}

/// The range of line `no` in `code`, its text and its line break.
fn line_at(code: &str, no: usize) -> Result<(Range<usize>, &str, &str), String> {
    let range = line_ranges(code)
        .get(no)
        .cloned()
        .ok_or("that line is not in the source")?;
    let (line, newline) = split_newline(&code[range.clone()]);
    Ok((range, line, newline))
}

/// How many bytes of markers a tree's line starts with: `**`, `++`, `--`,
/// a mind map's `##`.
fn markers(text: &str) -> usize {
    text.len() - text.trim_start_matches(['*', '#', '+', '-']).len()
}

/// `[#color]` at the start of `text`: how many bytes it takes.
fn bracket_color(text: &str) -> Option<usize> {
    let inner = text.strip_prefix("[#")?;
    let end = inner.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    (end > 0 && inner[end..].starts_with(']')).then_some(end + 3)
}

/// `(code)` at the start of `text`: how many bytes it takes.
fn paren_code(text: &str) -> Option<usize> {
    let inner = text.strip_prefix('(')?;
    let end = inner.find(|c: char| !(c.is_alphanumeric() || c == '_'))?;
    (end > 0 && inner[end..].starts_with(')')).then_some(end + 2)
}

/// What follows a node's markers, from the byte `from` of `line`: `_`,
/// `<`, `>`, `[#color]` and a WBS's `(code)`.
struct Decoration {
    color: Option<Range<usize>>,
    /// Where a color goes when there is none.
    insert: usize,
    /// Where the label begins.
    end: usize,
}

fn decoration(line: &str, from: usize) -> Decoration {
    let mut at = from;
    let mut color = None;
    let mut code = None;
    loop {
        let rest = &line[at..];
        if rest.starts_with(['_', '<', '>']) {
            at += 1;
        } else if let Some(len) = bracket_color(rest).filter(|_| color.is_none()) {
            color = Some(at..at + len);
            at += len;
        } else if let Some(len) = paren_code(rest).filter(|_| code.is_none()) {
            code = Some(at);
            at += len;
        } else {
            break;
        }
    }
    Decoration {
        color,
        // PlantUML takes the color and the code together.
        insert: code.unwrap_or(from),
        end: at,
    }
}

/// A node's last line: it ends with `;`, a stereotype after it or not.
fn node_ends(text: &str) -> bool {
    let text = text.trim_end();
    text.ends_with(';')
        || text.rfind(';').is_some_and(|semi| {
            let after = text[semi + 1..].trim();
            after.len() >= 5 && after.starts_with("<<") && after.ends_with(">>")
        })
}

/// The node of a mind map or a WBS on line `row`: on its line, or on any
/// line of a label written over several.
pub(super) fn tree_at(code: &str, row: usize) -> Option<Target> {
    let lines = element_lines(code);
    let mut index = 0;
    while index < lines.len() {
        let (no, text) = lines[index];
        let count = markers(text);
        let mut last = index;
        let mut label = String::new();
        if count > 0 {
            let rest = &text[decoration(text, count).end..];
            label = rest.trim().to_owned();
            if let Some(first) = rest.strip_prefix(':') {
                label = first.trim().trim_end_matches(';').to_owned();
                // Over several lines, up to the one that ends with `;`.
                if !node_ends(first)
                    && let Some(more) = lines[index + 1..]
                        .iter()
                        .position(|(_, text)| node_ends(text))
                {
                    last = index + 1 + more;
                }
            }
        }
        if count > 0 && (no..=lines[last].0).contains(&row) {
            if label.is_empty() {
                return None;
            }
            return Some(Target::Line {
                line: no,
                label: short(&label),
            });
        }
        index = last + 1;
    }
    None
}

/// The color of the node that starts on line `no`.
pub(super) fn tree(
    code: &str,
    no: usize,
    label: &str,
    color: Option<&str>,
) -> Result<Edit, String> {
    let (range, line, newline) = line_at(code, no)?;
    let start = indent_of(line).len();
    let count = markers(&line[start..]);
    if count == 0 {
        return Err("put the caret on the box to color".into());
    }
    let found = decoration(line, start + count);
    let new = match (found.color, color) {
        (Some(at), Some(color)) => {
            Some(format!("{}[{color}]{}", &line[..at.start], &line[at.end..]))
        }
        (Some(at), None) => Some(format!("{}{}", &line[..at.start], &line[at.end..])),
        (None, Some(color)) => Some(format!(
            "{}[{color}]{}",
            &line[..found.insert],
            &line[found.insert..]
        )),
        (None, None) => None,
    };
    changed(label, range, line, new, newline)
}

/// `|Name|`, `|#color|Name|` or `|Name| shown`: the color's range in the
/// line, where one goes, and the name.
fn lane(text: &str) -> Option<(Option<Range<usize>>, &str)> {
    let inner = text.strip_prefix('|')?;
    let (color, inner) = match inner.strip_prefix('#') {
        Some(_) => {
            let end = inner.find('|')?;
            (Some(1..end + 2), &inner[end + 1..])
        }
        None => (None, inner),
    };
    let (name, _) = inner.split_once('|')?;
    (!name.trim().is_empty()).then_some((color, name.trim()))
}

/// A connector's line (`(A)`, `#blue:(A);`): the range of its color, `#`
/// and `:` included, where it has one, and its name.
fn connector(text: &str) -> Option<(Option<Range<usize>>, &str)> {
    // `#red:(A)` and `(A)`; `:(A);` is an action that reads `(A)`.
    let (color, rest) = match action_start(text) {
        Some((Some(color), rest)) => (Some(0..color.len() + 1), rest),
        Some((None, _)) => return None,
        None => (None, text),
    };
    let body = rest.trim();
    let body = body.strip_suffix(';').unwrap_or(body).trim_end();
    let name = body.strip_prefix('(')?.strip_suffix(')')?;
    (name.chars().count() == 1 && name.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .then_some((color, name))
}

/// The action, the connector or the swimlane of an activity on line
/// `row`: an action on any of its lines, a lane on the line that names it
/// first.
pub(super) fn activity_at(code: &str, row: usize) -> Option<Target> {
    let lines = element_lines(code);
    let here = lines
        .iter()
        .find(|(no, _)| *no == row)
        .map(|(_, text)| *text);
    if let Some((_, name)) = here.and_then(lane) {
        let first = lines
            .iter()
            .find(|(_, text)| lane(text).is_some_and(|(_, other)| other == name))?;
        return Some(Target::Line {
            line: first.0,
            label: short(name),
        });
    }
    let mut index = 0;
    while index < lines.len() {
        let (no, text) = lines[index];
        // A connector is one line, no action that runs on; a line inside
        // an action that reads `(A)` is the action's.
        if let Some((_, name)) = connector(text) {
            if no == row {
                return Some(Target::Line {
                    line: row,
                    label: format!("({name})"),
                });
            }
            index += 1;
            continue;
        }
        let mut last = index;
        let started = action_start(text);
        if let Some((_, rest)) = started
            && action_end(rest).is_none()
        {
            last = lines[index + 1..]
                .iter()
                .position(|(_, text)| action_end(text).is_some())
                .map_or(index, |more| index + 1 + more);
        }
        if let Some((_, rest)) = started
            && (no..=lines[last].0).contains(&row)
        {
            let label = action_end(rest).map_or(rest, |(text, _)| text);
            return Some(Target::Line {
                line: no,
                label: short(label),
            });
        }
        index = last + 1;
    }
    None
}

/// The color of the action or the swimlane on line `no`.
pub(super) fn activity(
    code: &str,
    no: usize,
    label: &str,
    color: Option<&str>,
) -> Result<Edit, String> {
    let (range, line, newline) = line_at(code, no)?;
    let indent = indent_of(line);
    let text = &line[indent.len()..];
    let new = if let Some((old, _)) = lane(text) {
        match (old, color) {
            (Some(at), Some(color)) => Some(format!("{indent}|{color}|{}", &text[at.end..])),
            (Some(at), None) => Some(format!("{indent}|{}", &text[at.end..])),
            (None, Some(color)) => Some(format!("{indent}|{color}{text}")),
            (None, None) => None,
        }
    } else if let Some((old, _)) = connector(text) {
        let rest = old.clone().map_or(text, |at| &text[at.end..]);
        match (old, color) {
            (_, Some(color)) => Some(format!("{indent}{color}:{rest}")),
            (Some(_), None) => Some(format!("{indent}{rest}")),
            (None, None) => None,
        }
    } else {
        let (old, rest) =
            action_start(text).ok_or("put the caret on the action or the swimlane to color")?;
        // `:(y);` in a color is `#red:(y);`, which PlantUML reads as a
        // connector.
        if let Some(color) = color
            && connector(&format!("{color}:{rest}")).is_some()
        {
            return Err(format!(
                "an action that reads only `{}` would be a connector in a color",
                rest.trim().trim_end_matches(';')
            ));
        }
        match (old, color) {
            (_, Some(color)) => Some(format!("{indent}{color}:{rest}")),
            (Some(_), None) => Some(format!("{indent}:{rest}")),
            (None, None) => None,
        }
    };
    changed(label, range, line, new, newline)
}

/// The names in the brackets of `line`, each with its range, brackets
/// included. A link's `[[…]]` is none.
fn bracketed(line: &str) -> Vec<(Range<usize>, &str)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(open) = line[at..].find('[').map(|i| at + i) {
        if line[open..].starts_with("[[") {
            at = line[open..]
                .find("]]")
                .map_or(line.len(), |end| open + end + 2);
            continue;
        }
        let Some(close) = line[open..].find(']').map(|i| open + i) else {
            break;
        };
        out.push((open..close + 1, line[open + 1..close].trim()));
        at = close + 1;
    }
    out
}

/// The task of a Gantt chart the caret is on: the brackets around the
/// caret, or the task the line is about.
pub(super) fn task_at(line: &str, col: usize) -> Option<Target> {
    let names = bracketed(line);
    let about = || names.first().filter(|_| line.trim_start().starts_with('['));
    names
        .iter()
        .find(|(range, _)| range.start <= col && col <= range.end)
        .or_else(about)
        .filter(|(_, name)| !name.is_empty())
        .map(|(_, name)| Target::Named((*name).to_owned()))
}

/// What on `line` opens a bar on a lifeline: after which byte its color
/// is written, the bytes of a color written there, and whose bar it is,
/// if the line says. An `activate` line, or a message with `++`.
struct Opens {
    after: usize,
    color: Option<Range<usize>>,
    whose: Option<Range<usize>>,
    /// The bytes that say so: the caret on them means the bar.
    sign: Range<usize>,
}

/// Whether the zero-based `row` of `code` opens a bar, as the engine
/// reads it: a `++` in a name, a title or a note's text opens none.
fn opens_a_bar(code: &str, row: usize) -> bool {
    let Ok(Diagram::Sequence(diagram)) = omaspec_uml::parse(code) else {
        return false;
    };
    diagram.items.iter().any(
        |item| matches!(item, omaspec_uml::sequence::Item::Activate { line, .. } if *line == row),
    )
}

fn opens(code: &str, row: usize) -> Option<Opens> {
    let line = code.split('\n').nth(row)?;
    let line = line.strip_suffix('\r').unwrap_or(line);
    if !opens_a_bar(code, row) {
        return None;
    }
    if let Some(caps) =
        re!(r#"(?i)^\s*activate\s+("[^"]*"|[^\s#]+)(?:\s+(#\S+))?\s*$"#).captures(line)
    {
        let name = caps.get(1)?.range();
        return Some(Opens {
            after: name.end,
            color: caps.get(2).map(|m| m.range()),
            whose: Some(name),
            sign: 0..line.len(),
        });
    }
    // After the second end of a message and before its text: `++`, also
    // with a `--` beside it, and a color. What stands in quotes is a
    // name, whatever it holds: it is read as letters here.
    let mut quoted = false;
    let mut colon = None;
    let masked: String = line
        .char_indices()
        .map(|(at, c)| {
            if c == '"' {
                quoted = !quoted;
                return c;
            }
            if c == ':' && !quoted && colon.is_none() {
                colon = Some(at);
            }
            // One byte for each byte, so that what is found lies where it
            // lies in the line.
            if quoted && c.is_ascii() { 'x' } else { c }
        })
        .collect();
    let head = &masked[..colon.unwrap_or(masked.len())];
    // The last one: the ends of the message come before it.
    let caps = re!(r"(\+\+--|--\+\+|\+\+)(?:\s*(#[0-9A-Za-z]+))?")
        .captures_iter(head)
        .last()?;
    let sign = caps.get(1)?.range();
    let color = caps.get(2).map(|m| m.range());
    Some(Opens {
        after: sign.end,
        sign: sign.start..color.as_ref().map_or(sign.end, |c| c.end),
        color,
        whose: None,
    })
}

/// Where the zero-based `row` of `code` says that it opens a bar on a
/// lifeline: the bytes of its `++` and the color after it, or the whole
/// of an `activate` line. A caret there means the bar.
pub fn bar_sign(code: &str, row: usize) -> Option<Range<usize>> {
    opens(code, row).map(|opens| opens.sign)
}

/// The bar the line at the zero-based `row` opens, if the caret at the
/// byte `col` means it: anywhere on an `activate` line, on the `++` of a
/// message or on its color.
pub(super) fn activation_at(code: &str, row: usize, col: usize) -> Option<Target> {
    let opens = opens(code, row)?;
    let line = code.split('\n').nth(row)?;
    if col < opens.sign.start || col > opens.sign.end {
        return None;
    }
    Some(Target::Line {
        line: row,
        label: match opens.whose {
            Some(name) => format!("the bar on {}", line[name].trim_matches('"')),
            None => "the bar this message opens".to_owned(),
        },
    })
}

/// The color of the bar a line opens on a lifeline, as PlantUML writes
/// it: after the participant (`activate Bob #f7768e`), or after a
/// message's `++` (`Alice -> Bob ++ #f7768e : hi`).
pub(super) fn activation(
    code: &str,
    no: usize,
    label: &str,
    color: Option<&str>,
) -> Result<Edit, String> {
    let (range, line, newline) = line_at(code, no)?;
    let opens = opens(code, no).ok_or("that line opens no bar on a lifeline")?;
    let new = match (color, opens.color) {
        (Some(color), Some(old)) => {
            Some(format!("{}{color}{}", &line[..old.start], &line[old.end..]))
        }
        (Some(color), None) => Some(format!(
            "{} {color}{}",
            &line[..opens.after],
            &line[opens.after..]
        )),
        (None, Some(old)) => Some(format!(
            "{}{}",
            line[..old.start].trim_end(),
            &line[old.end..]
        )),
        (None, None) => None,
    };
    changed(label, range, line, new, newline)
}

/// The color of a Gantt task: its `is colored in` clause, or a line of
/// its own after the last line that is about the task.
pub(super) fn task(code: &str, name: &str, color: Option<&str>) -> Result<Edit, String> {
    // A task is written by its name or by its alias.
    let mut names = vec![name.to_owned()];
    if let Ok(Diagram::Gantt(diagram)) = omaspec_uml::parse(code) {
        for row in &diagram.rows {
            if let omaspec_uml::gantt::Row::Bar(bar) = row
                && (bar.key == name || bar.label == name)
            {
                names.push(bar.key.clone());
                names.push(bar.label.clone());
            }
        }
    }
    let ranges = line_ranges(code);
    let about: Vec<(Range<usize>, usize)> = ranges
        .iter()
        .filter_map(|range| {
            let (line, _) = split_newline(&code[range.clone()]);
            let (subject, written) = bracketed(line).into_iter().next()?;
            (line.trim_start().starts_with('[') && names.iter().any(|n| n == written))
                .then_some((range.clone(), subject.end))
        })
        .collect();
    let clause = re!(r"(?i)^(\s+and)?\s+is\s+colou?red\s+in\s+(\S+)(\s+and\b)?");
    for (range, subject) in &about {
        let (line, newline) = split_newline(&code[range.clone()]);
        // The clause right after the task, or after an `and` further on.
        let mut found = None;
        let mut at = *subject;
        while at < line.len() {
            if let Some(caps) = clause.captures(&line[at..]) {
                found = Some((at, caps));
                break;
            }
            at += line[at..].chars().next().map_or(1, char::len_utf8);
        }
        let Some((at, caps)) = found else {
            continue;
        };
        let whole = caps.get(0).map_or(0..0, |m| m.range());
        let token = caps.get(2).map_or(0..0, |m| m.range());
        let new = match color {
            Some(color) => format!(
                "{}{color}{}",
                &line[..at + token.start],
                &line[at + token.end..]
            ),
            None => {
                let before = &line[..at];
                let after = &line[at + whole.end..];
                if caps.get(1).is_some() {
                    // `… and is colored in X [and …]`
                    let and = if caps.get(3).is_some() { " and" } else { "" };
                    format!("{before}{and}{after}")
                } else if caps.get(3).is_some() {
                    // `[T] is colored in X and …`
                    format!("{before}{after}")
                } else if after.trim().is_empty() {
                    // The line held the color alone: it goes.
                    return Ok((range.clone(), String::new()));
                } else {
                    format!("{before}{after}")
                }
            }
        };
        return changed(name, range.clone(), line, Some(new), newline);
    }
    let color = color.ok_or_else(|| format!("{name} has no color of its own"))?;
    let (last, _) = about
        .last()
        .ok_or_else(|| format!("{name} is not in the source"))?;
    let (line, newline) = split_newline(&code[last.clone()]);
    let lead = if newline.is_empty() { "\n" } else { "" };
    let written = bracketed(line).first().map_or(name, |(_, written)| written);
    Ok((
        last.end..last.end,
        format!(
            "{lead}{}[{written}] is colored in {color}\n",
            indent_of(line)
        ),
    ))
}
