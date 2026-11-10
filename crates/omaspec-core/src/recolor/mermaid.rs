//! Mermaid's boxes: a `style Name fill:#…` line in a flowchart, a state,
//! class, ER or block diagram, its other properties kept; in a C4 diagram
//! the `$bgColor` of an `UpdateElementStyle` line.

use std::ops::Range;

use super::{Edit, Target, covering, indent_of, split_newline};
use crate::project::key;
use crate::text::{line_ranges, re};

/// A line after the last line of the diagram, as deep as that one; a
/// comment at the end stays the end.
fn appended(code: &str, text: &str) -> Result<Edit, String> {
    let last = line_ranges(code)
        .into_iter()
        .rfind(|range| {
            let line = code[range.clone()].trim();
            !line.is_empty() && !line.starts_with("%%")
        })
        .ok_or("an empty diagram")?;
    let (line, newline) = split_newline(&code[last.clone()]);
    let indent = indent_of(line);
    let indent = if indent.is_empty() { "  " } else { indent };
    // A last line without a break gets one first.
    let lead = if newline.is_empty() { "\n" } else { "" };
    Ok((last.end..last.end, format!("{lead}{indent}{text}\n")))
}

pub(super) fn style(code: &str, name: &str, color: Option<&str>) -> Result<Edit, String> {
    let wanted = key(name);
    for range in line_ranges(code) {
        let (line, newline) = split_newline(&code[range.clone()]);
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("style") else {
            continue;
        };
        if !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim_start();
        let (ids, props) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        if ids.contains(',') || key(ids) != wanted {
            continue;
        }
        let mut kept: Vec<&str> = props
            .split(',')
            .map(str::trim)
            .filter(|prop| !prop.is_empty() && !prop.starts_with("fill:"))
            .collect();
        let fill = color.map(|color| format!("fill:{color}"));
        if let Some(fill) = &fill {
            kept.insert(0, fill);
        }
        if kept.is_empty() {
            // The line held the fill alone: it goes.
            return Ok((range, String::new()));
        }
        let new = format!("{}style {ids} {}{newline}", indent_of(line), kept.join(","));
        if new == code[range.clone()] {
            return Err(format!("{name} has that color already"));
        }
        return Ok((range, new));
    }
    let Some(color) = color else {
        return Err(format!("{name} has no color of its own"));
    };
    appended(code, &format!("style {name} fill:{color}"))
}

/// The name a line of an ER diagram starts with: a word, or in quotes.
fn entity_name(text: &str) -> Option<&str> {
    if let Some(inner) = text.strip_prefix('"') {
        return inner.find('"').map(|end| &inner[..end]);
    }
    let end = text
        .find(|c: char| c.is_whitespace() || matches!(c, '{' | '[' | ':' | '|' | '}'))
        .unwrap_or(text.len());
    (end > 0).then(|| &text[..end])
}

/// The entity of an ER diagram the caret is on: its name in a
/// relationship, or anywhere in its block of attributes.
pub(super) fn entity_at(code: &str, row: usize, col: usize) -> Option<Target> {
    const WORDS: [&str; 8] = [
        "erDiagram",
        "style",
        "classDef",
        "class",
        "direction",
        "title",
        "accTitle",
        "accDescr",
    ];
    let named = |name: &str| Some(Target::Named(name.to_owned()));
    let mut block: Option<&str> = None;
    for (no, line) in code.split('\n').enumerate() {
        let text = line.trim();
        if let Some(name) = block {
            if no == row {
                return named(name);
            }
            if text.starts_with('}') {
                block = None;
            }
            continue;
        }
        let first = text.split_whitespace().next().unwrap_or("");
        if text.is_empty() || text.starts_with("%%") || WORDS.contains(&first) {
            if no == row {
                return None;
            }
            continue;
        }
        let name = entity_name(text);
        if text.ends_with('{') && !text.contains("--") && !text.contains("..") {
            if no == row {
                return name.and_then(named);
            }
            block = name;
            continue;
        }
        if no == row {
            // `A ||--o{ B : label`: the one the caret is on, else the first.
            let head = text.split(" : ").next().unwrap_or(text);
            let mut words = head.split_whitespace();
            let (a, _, b) = (words.next(), words.next(), words.next());
            let names = [a, b].into_iter().flatten().map(|n| n.trim_matches('"'));
            return covering(line, col, names).or(name).and_then(named);
        }
    }
    None
}

/// A macro call of a C4 diagram: its name in lower case and its first
/// argument.
fn c4_call(line: &str) -> Option<(String, &str)> {
    let caps = re!(r#"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*\(\s*"?([^,)"\s]+)"#).captures(line)?;
    Some((caps[1].to_ascii_lowercase(), caps.get(2)?.as_str()))
}

fn c4_style(name: &str) -> bool {
    matches!(
        name,
        "updateelementstyle" | "update_el_style" | "updateelstyle"
    )
}

/// Whether the macro `name` draws a box: a person, a system, a container,
/// a boundary. Not a relationship or a change to one.
fn c4_box(name: &str) -> bool {
    !["rel", "birel", "update", "add", "show", "hide", "lay"]
        .iter()
        .any(|word| name.starts_with(word))
}

/// The element of a C4 diagram the caret is on: on its own line, or its
/// alias in a relationship.
pub(super) fn c4_at(code: &str, row: usize, col: usize) -> Option<Target> {
    let line = code.split('\n').nth(row)?;
    match c4_call(line) {
        Some((name, alias)) if c4_box(&name) || c4_style(&name) => {
            return Some(Target::Named(alias.to_owned()));
        }
        _ => {}
    }
    let aliases: Vec<&str> = code
        .lines()
        .filter_map(c4_call)
        .filter(|(name, _)| c4_box(name))
        .map(|(_, alias)| alias)
        .collect();
    covering(line, col, aliases).map(|alias| Target::Named(alias.to_owned()))
}

/// The arguments between the brackets of a call, split at the commas
/// outside quotes: each with its range in `line`.
fn arguments(line: &str) -> Option<(Range<usize>, Vec<Range<usize>>)> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    let mut out = Vec::new();
    let mut quoted = false;
    let mut start = open + 1;
    for (at, c) in line[..close]
        .char_indices()
        .skip_while(|(at, _)| *at <= open)
    {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                out.push(start..at);
                start = at + 1;
            }
            _ => {}
        }
    }
    out.push(start..close);
    Some((open + 1..close, out))
}

pub(super) fn c4(code: &str, name: &str, color: Option<&str>) -> Result<Edit, String> {
    for range in line_ranges(code) {
        let (line, newline) = split_newline(&code[range.clone()]);
        match c4_call(line) {
            Some((call, alias)) if c4_style(&call) && alias == name => {}
            _ => continue,
        }
        let Some((inside, args)) = arguments(line) else {
            continue;
        };
        let mut texts: Vec<String> = args
            .iter()
            .map(|a| line[a.clone()].trim().to_owned())
            .collect();
        let is_named = |text: &str| text.starts_with('$');
        let named = texts.iter().position(|text| {
            text.split('=')
                .next()
                .is_some_and(|k| k.trim().eq_ignore_ascii_case("$bgcolor"))
        });
        let positional = (texts.len() > 1 && !is_named(&texts[1])).then_some(1);
        match (named.or(positional), color) {
            (Some(at), Some(color)) if is_named(&texts[at]) => {
                texts[at] = format!("$bgColor=\"{color}\"");
            }
            (Some(at), Some(color)) => {
                texts[at] = if texts[at].starts_with('"') {
                    format!("\"{color}\"")
                } else {
                    color.to_owned()
                };
            }
            (Some(at), None) => {
                if !is_named(&texts[at]) && texts.get(at + 1).is_some_and(|next| !is_named(next)) {
                    return Err(format!(
                        "the style of {name} holds more than its color; change it in the source"
                    ));
                }
                texts.remove(at);
            }
            (None, Some(color)) => texts.push(format!("$bgColor=\"{color}\"")),
            (None, None) => return Err(format!("{name} has no color of its own")),
        }
        if texts.len() <= 1 {
            // The line held the color alone: it goes.
            return Ok((range, String::new()));
        }
        let new = format!(
            "{}{}{}",
            &line[..inside.start],
            texts.join(", "),
            &line[inside.end..]
        );
        if new == line {
            return Err(format!("{name} has that color already"));
        }
        return Ok((range, format!("{new}{newline}")));
    }
    let Some(color) = color else {
        return Err(format!("{name} has no color of its own"));
    };
    appended(
        code,
        &format!("UpdateElementStyle({name}, $bgColor=\"{color}\")"),
    )
}
