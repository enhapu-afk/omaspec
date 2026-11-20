//! From the body's lines to a [`Diagram`]. The scale may be written
//! before or after the values on it; a value names one of its steps, or
//! it refuses. What is no line of a capability map is a mistake: PlantUML
//! does not know the diagram, so no other engine draws it.

use super::{Capability, DEEPEST, Diagram, Level, Levels};
use crate::Declined;
use crate::common::{Frame, keyword, snippet};
use crate::source::Line;

pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut frame = Frame::default();
    let mut levels: Option<Levels> = None;
    let mut areas: Vec<Capability> = Vec::new();
    // The path to the capability written last, an index at each depth.
    let mut path: Vec<usize> = Vec::new();
    // Each value as written, with its line, until the scale is known.
    let mut values: Vec<(Vec<usize>, String, usize)> = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        if let Some(next) = frame.line(lines, at)? {
            at = next;
            continue;
        }
        let Line {
            no,
            text,
            at: start,
            ..
        } = lines[at];
        if let Some(rest) = keyword(text, "levels") {
            if let Some(first) = &levels {
                return Err(Declined::refused(
                    no,
                    format!(
                        "a map has one scale; the first is on line {}",
                        first.line + 1
                    ),
                ));
            }
            levels = Some(scale(rest, no, start + offset(text, rest))?);
        } else if text.starts_with('*') {
            let depth = text.chars().take_while(|&c| c == '*').count();
            let rest = &text[depth..];
            if depth > DEEPEST {
                return Err(Declined::refused(
                    no,
                    format!("a capability stands at most {DEEPEST} levels deep"),
                ));
            }
            if depth > path.len() + 1 {
                return Err(Declined::refused(
                    no,
                    format!(
                        "a capability stands one level under the one before it: {} after {}",
                        "*".repeat(path.len() + 1),
                        if path.is_empty() {
                            "the start".to_owned()
                        } else {
                            "*".repeat(path.len())
                        }
                    ),
                ));
            }
            let (name, value) = split_last_colon(rest);
            let name = unquote(name);
            if name.is_empty() {
                return Err(Declined::refused(no, "a capability needs a name"));
            }
            path.truncate(depth - 1);
            let siblings = children(&mut areas, &path);
            siblings.push(Capability {
                name: name.to_owned(),
                value: None,
                line: no,
                col: start + offset(text, name),
                children: Vec::new(),
            });
            path.push(siblings.len() - 1);
            if let Some(value) = value.map(unquote).filter(|v| !v.is_empty()) {
                values.push((path.clone(), value.to_owned(), no));
            }
        } else {
            return Err(Declined::refused(
                no,
                format!(
                    "`{}` is no line of a capability map: write `* an area`, `** a capability` or `levels`",
                    snippet(text)
                ),
            ));
        }
        at += 1;
    }
    for (path, value, no) in values {
        let Some(scale) = &levels else {
            return Err(Declined::refused(
                no,
                format!(
                    "{value} is a value on no scale: write `levels Name : one, two, three` for the steps it is one of"
                ),
            ));
        };
        let Some(step) = scale.steps.iter().position(|s| same(&s.name, &value)) else {
            return Err(Declined::refused(
                no,
                format!(
                    "{value} is no step of {}: the steps are {}; a capability with a colon of its own goes in quotes",
                    scale.name,
                    scale
                        .steps
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        };
        let (last, around) = path.split_last().expect("a capability");
        children(&mut areas, around)[*last].value = Some(step);
    }
    Ok(Diagram {
        levels,
        areas,
        frame,
    })
}

/// The scale `levels Name : one, two #color, three`, on line `no`, from
/// its byte `col` on.
fn scale(rest: &str, no: usize, col: usize) -> Result<Levels, Declined> {
    let Some((name, steps)) = rest.split_once(':') else {
        return Err(Declined::refused(
            no,
            "a scale names its steps after a colon: `levels Maturity : none, basic, managed`",
        ));
    };
    let name = unquote(name);
    if name.is_empty() {
        return Err(Declined::refused(no, "a scale needs a name"));
    }
    let mut out: Vec<Level> = Vec::new();
    for step in steps.split(',') {
        let step = step.trim();
        // A color after the name: `managed #2F9E44`.
        let (word, color) = match step.rfind(" #") {
            Some(at) => (step[..at].trim(), Some(&step[at + 1..])),
            None => (step, None),
        };
        let word = unquote(word);
        if word.is_empty() {
            return Err(Declined::refused(no, "a step of the scale needs a name"));
        }
        if out.iter().any(|s| same(&s.name, word)) {
            return Err(Declined::refused(
                no,
                format!("the step {word} is written twice"),
            ));
        }
        let color = match color {
            Some(color) => crate::color::parse(color)
                .map_err(|reason| Declined::refused(no, format!("{word}: {reason}")))?,
            None => None,
        };
        out.push(Level {
            name: word.to_owned(),
            color,
            col: col + offset(rest, word),
        });
    }
    Ok(Levels {
        name: name.to_owned(),
        steps: out,
        line: no,
        col: col + offset(rest, name),
    })
}

/// Where `part`, a piece of `whole`, starts in it.
fn offset(whole: &str, part: &str) -> usize {
    part.as_ptr() as usize - whole.as_ptr() as usize
}

/// The capabilities at `path`: the areas when it is empty.
fn children<'a>(areas: &'a mut Vec<Capability>, path: &[usize]) -> &'a mut Vec<Capability> {
    match path.split_first() {
        Some((&first, rest)) => children(&mut areas[first].children, rest),
        None => areas,
    }
}

/// `text` before and after its last colon outside quotes, trimmed.
fn split_last_colon(text: &str) -> (&str, Option<&str>) {
    let mut quote = false;
    let mut found = None;
    for (at, c) in text.char_indices() {
        match c {
            '"' => quote = !quote,
            ':' if !quote => found = Some(at),
            _ => {}
        }
    }
    match found {
        Some(at) => (text[..at].trim(), Some(text[at + 1..].trim())),
        None => (text.trim(), None),
    }
}

/// `text` without the quotes around it, trimmed.
fn unquote(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .map_or(text, str::trim)
}

/// Whether two step names are one: as written, or in another case.
fn same(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn read(body: &str) -> Result<Diagram, Declined> {
        let source = format!("@startcapabilities\n{body}\n@endcapabilities\n");
        match crate::parse(&source)? {
            crate::Diagram::Capabilities(diagram) => Ok(diagram),
            other => panic!("no capability map: {other:?}"),
        }
    }

    #[test]
    fn areas_hold_capabilities_each_on_the_scale() {
        let d = read(
            "title Bookshop\n* Customers\n** Marketing : basic\n** Sales\n*** \"Online: the shop\" : Leading\n*** Stores\n* Operations : none\nlevels Maturity : none #E8590C, basic, managed, leading #2F9E44",
        )
        .unwrap();
        let scale = d.levels.as_ref().unwrap();
        assert_eq!(scale.name, "Maturity");
        assert_eq!(
            scale
                .steps
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["none", "basic", "managed", "leading"]
        );
        assert_eq!(scale.steps[0].color, Some(Color::hex(0xE8590C)));
        assert_eq!(scale.steps[1].color, None);
        let line = "levels Maturity : none #E8590C, basic, managed, leading #2F9E44";
        assert_eq!(scale.col, line.find("Maturity").unwrap());
        assert_eq!(scale.steps[2].col, line.find("managed").unwrap());
        let customers = &d.areas[0];
        assert_eq!(customers.children[0].value, Some(1));
        let sales = &customers.children[1];
        assert_eq!(sales.value, None);
        assert_eq!(sales.children[0].name, "Online: the shop");
        // Where the name starts on its line, inside its quotes.
        assert_eq!(sales.children[0].col, "*** \"".len());
        assert_eq!(customers.children[0].col, "** ".len());
        assert_eq!(
            (sales.children[0].value, sales.children[0].line),
            (Some(3), 5)
        );
        assert_eq!(d.areas[1].value, Some(0));
        assert!(d.frame.title.is_some());
    }

    #[test]
    fn a_mistake_is_refused_on_its_line() {
        let refused = |body: &str| {
            let err = read(body).unwrap_err();
            assert!(err.refused, "{body}: {err:?}");
            (err.line, err.reason)
        };
        let (line, reason) = refused("levels Risk : low, high\n* Shop : medium");
        assert_eq!(line, Some(2));
        assert!(reason.contains("the steps are low, high"), "{reason}");
        let (_, reason) = refused("* Shop : medium");
        assert!(reason.contains("on no scale"), "{reason}");
        let (line, reason) = refused("* Shop\n*** Till");
        assert_eq!(line, Some(2));
        assert!(reason.contains("one level under"), "{reason}");
        let (_, reason) = refused("** Till");
        assert!(reason.contains("one level under"), "{reason}");
        let deep: String = (1..=7)
            .map(|n| format!("{} L{n}\n", "*".repeat(n)))
            .collect();
        let (_, reason) = refused(&deep);
        assert!(reason.contains("at most 6 levels"), "{reason}");
        let (_, reason) = refused("levels Risk");
        assert!(reason.contains("after a colon"), "{reason}");
        let (_, reason) = refused("levels Risk : low, low");
        assert!(reason.contains("written twice"), "{reason}");
        let (_, reason) = refused("levels A : x\nlevels B : y");
        assert!(reason.contains("one scale"), "{reason}");
        let (_, reason) = refused("Shop");
        assert!(reason.contains("no line of a capability map"), "{reason}");
    }
}
