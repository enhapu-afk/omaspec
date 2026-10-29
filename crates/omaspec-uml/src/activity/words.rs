//! The small pieces of PlantUML's activity commands: keywords in any
//! case, `(…)` groups that end where the rest of the line fits, and what
//! may follow a test.

use crate::Declined;
use crate::source::{Line, starts_with_ci};

/// The line without spaces, in lower case, without a `;` at its end.
pub fn squeezed(text: &str) -> String {
    let s: String = text
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase();
    match s.strip_suffix(';') {
        Some(t) => t.to_owned(),
        None => s,
    }
}

/// `#color:` or `:` at the start of an action: the color and the rest.
pub fn action_start(text: &str) -> Option<(Option<&str>, &str)> {
    if let Some(rest) = text.strip_prefix(':') {
        return Some((None, rest));
    }
    let color = text.strip_prefix('#')?;
    let end = color.find(':')?;
    let name = &color[..end];
    (!name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/' | '\\' | '|')))
    .then(|| (Some(&text[..end + 1]), &color[end + 1..]))
}

/// How an action's last line ends it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionEnd {
    /// `;`
    Plain,
    /// `; <<input>>`, `; <<#pink>>`: what is between the brackets.
    Stereotype(String),
    /// One of PlantUML's SDL marks in place of `;`: `| < > / \ ] }`.
    Mark(char),
}

/// The text of a line that ends an action, and how it ends; `None` for a
/// line that does not.
pub fn action_end(line: &str) -> Option<(&str, ActionEnd)> {
    let line = line.trim_end();
    if let Some(text) = line.strip_suffix(';') {
        return Some((text, ActionEnd::Plain));
    }
    if let Some(inner) = line.strip_suffix(">>")
        && let Some(open) = inner.rfind("<<")
        && let Some(text) = inner[..open].trim_end().strip_suffix(';')
    {
        return Some((
            text,
            ActionEnd::Stereotype(inner[open + 2..].trim().to_owned()),
        ));
    }
    let mark = line.chars().next_back()?;
    let text = &line[..line.len() - mark.len_utf8()];
    // Not the end of creole or HTML in the text: `//italics//`, `</b>`,
    // `<&icon>`, a link's `]`, a table's `|`.
    let word = text.rsplit(char::is_whitespace).next().unwrap_or("");
    let mark_here = match mark {
        '<' => true,
        '|' => !line.starts_with('|'),
        '>' => !word.contains('<'),
        '/' => !text.ends_with(['/', ':']),
        '\\' => !text.ends_with('\\'),
        ']' => !word.contains('['),
        '}' => !word.contains('{'),
        _ => false,
    };
    mark_here.then_some((text, ActionEnd::Mark(mark)))
}

/// An action from `first` on up to the line that ends it ([`action_end`]):
/// its lines joined with newlines, blank lines kept; how it ends; and the
/// index after it.
pub fn until_action_end(
    lines: &[Line],
    at: usize,
    first: &str,
) -> Result<(String, ActionEnd, usize), Declined> {
    until(lines, at, first, action_end)
        .ok_or_else(|| Declined::at(lines[at].no, "the action has no line that ends it with `;`"))
}

/// The text from `first` on up to the line that ends with `;`, its lines
/// joined with newlines, blank lines kept; and the index after it.
pub fn until_semicolon(
    lines: &[Line],
    at: usize,
    first: &str,
    what: &str,
) -> Result<(String, usize), Declined> {
    until(lines, at, first, |text| {
        text.strip_suffix(';').map(|text| (text, ()))
    })
    .map(|(text, (), next)| (text, next))
    .ok_or_else(|| {
        Declined::at(
            lines[at].no,
            format!("{what} has no line that ends with `;`"),
        )
    })
}

/// The text from `first` on up to the first line after `at` that `end`
/// finds an end in, its lines joined with newlines, blank lines kept; what
/// `end` found; and the index after that line.
fn until<T>(
    lines: &[Line],
    at: usize,
    first: &str,
    end: impl Fn(&str) -> Option<(&str, T)>,
) -> Option<(String, T, usize)> {
    let mut body = vec![first];
    for (offset, line) in lines[at + 1..].iter().enumerate() {
        body.extend(std::iter::repeat_n("", line.blank_before));
        if let Some((last, found)) = end(line.text) {
            body.push(last);
            return Some((body.join("\n"), found, at + offset + 2));
        }
        body.push(line.text);
    }
    None
}

/// `text` without the keyword `w` (any case) at its start, if neither a
/// letter nor a digit follows it; trimmed at the start.
pub fn word<'a>(text: &'a str, w: &str) -> Option<&'a str> {
    if !starts_with_ci(text, w) {
        return None;
    }
    let rest = &text[w.len()..];
    rest.chars()
        .next()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
        .then(|| rest.trim_start())
}

/// `a b` or `ab` at the start of `text`, in any case: the rest.
pub fn two_words<'a>(text: &'a str, a: &str, b: &str) -> Option<&'a str> {
    if !starts_with_ci(text, a) {
        return None;
    }
    let after = text[a.len()..].trim_start();
    word(after, b)
}

/// `(text)` at the start of `s`: the text and the rest. The `)` that ends
/// it is the first one after which `fits` accepts the rest, as PlantUML's
/// lazy `\((.*?)\)` takes it.
pub fn group(s: &str, fits: impl Fn(&str) -> bool) -> Option<(&str, &str)> {
    let inner = s.trim_start().strip_prefix('(')?;
    inner
        .match_indices(')')
        .map(|(i, _)| (&inner[..i], &inner[i + 1..]))
        .find(|(_, rest)| fits(rest))
}

/// `(label)` filling the whole of `s`.
pub fn whole_group(s: &str) -> Option<&str> {
    group(s, |rest| rest.trim().is_empty()).map(|(inner, _)| inner)
}

/// The test of an `if` or `elseif` and the label of its `then`: after the
/// keyword, `(test)` with `then`, `then (yes)`, `is (yes) then` or nothing.
pub fn test_and_label(rest: &str) -> Option<(&str, Option<&str>)> {
    let (test, tail) = group(rest, |tail| then_tail(tail).is_some())?;
    Some((test, then_tail(tail)?))
}

pub fn then_tail(tail: &str) -> Option<Option<&str>> {
    let t = tail.trim();
    let t = t.strip_suffix(';').unwrap_or(t).trim_end();
    if t.is_empty() {
        return Some(None);
    }
    for is in ["is", "equals", "equal"] {
        if let Some(after) = word(t, is) {
            let (when, _) = group(after, |r| {
                let r = r.trim();
                r.is_empty() || word(r, "then").is_some_and(str::is_empty)
            })?;
            return Some(Some(when).filter(|w| !w.trim().is_empty()));
        }
    }
    let after = word(t, "then")?;
    if after.is_empty() {
        return Some(None);
    }
    Some(Some(whole_group(after)?).filter(|w| !w.trim().is_empty()))
}

/// `elseif`, `else if`, `(no) elseif`: the incoming label and the rest.
pub fn else_if(text: &str) -> Option<(Option<&str>, &str)> {
    let (incoming, rest) = if text.starts_with('(') {
        let (inner, rest) = group(text, |r| two_words(r.trim_start(), "else", "if").is_some())?;
        (
            Some(inner.trim()).filter(|l| !l.is_empty()),
            rest.trim_start(),
        )
    } else {
        (None, text)
    };
    let rest = two_words(rest, "else", "if")?;
    rest.starts_with('(').then_some((incoming, rest))
}

/// After a `while`'s test: nothing or `is (yes)`.
pub fn while_tail(tail: &str) -> Option<Option<&str>> {
    let t = tail.trim();
    let t = t.strip_suffix(';').unwrap_or(t).trim_end();
    if t.is_empty() {
        return Some(None);
    }
    let after = ["is", "equals", "equal"].iter().find_map(|w| word(t, w))?;
    Some(Some(whole_group(after)?))
}

/// After a `repeat while`'s test: `is (yes)`, `not (no)`, both or nothing.
pub fn repeat_tail(tail: &str) -> Option<(Option<&str>, Option<&str>)> {
    let t = tail.trim();
    if t.is_empty() {
        return Some((None, None));
    }
    if let Some(after) = word(t, "not") {
        return Some((None, Some(whole_group(after)?)));
    }
    let after = ["is", "equals", "equal"].iter().find_map(|w| word(t, w))?;
    let (yes, rest) = group(after, |r| {
        let r = r.trim();
        r.is_empty() || word(r, "not").and_then(whole_group).is_some()
    })?;
    let rest = rest.trim();
    let out = if rest.is_empty() {
        None
    } else {
        Some(whole_group(word(rest, "not")?)?)
    };
    Some((Some(yes), out))
}

/// `(in) backward :text; (out)`: the label before, the text and the
/// label after.
pub fn backward(text: &str) -> Option<(Option<&str>, &str, Option<&str>)> {
    let (incoming, rest) = if text.starts_with('(') {
        let (inner, rest) = group(text, |r| word(r.trim_start(), "backward").is_some())?;
        (Some(inner.trim()), rest.trim_start())
    } else {
        (None, text)
    };
    let rest = word(rest, "backward")?.strip_prefix(':')?;
    rest.match_indices(';').find_map(|(i, _)| {
        let after = rest[i + 1..].trim();
        if after.is_empty() {
            Some((incoming, &rest[..i], None))
        } else {
            whole_group(after).map(|out| (incoming, &rest[..i], Some(out.trim())))
        }
    })
}
