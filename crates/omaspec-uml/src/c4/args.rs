//! The arguments of a C4 macro, by place or by name, quoted or bare, as
//! Mermaid's C4 and C4-PlantUML write them.

pub(crate) struct Args {
    pub(crate) positional: Vec<String>,
    pub(crate) kv: Vec<(String, String)>,
}

/// The arguments as written (`split_args`): a keyword argument is a name
/// not in quotes before its `=` (`$bgColor="#f7768e"`, the quotes around
/// the value); anything else is positional, an empty one too, so the
/// arguments after it keep their places. `"key=value"` is a text.
pub(crate) fn parse_args(args: &[String]) -> Args {
    let mut positional = Vec::new();
    let mut kv = Vec::new();
    for arg in args {
        let trimmed = arg.trim();
        let quoted = trimmed.starts_with(['"', '\'']);
        if !quoted && let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim().trim_start_matches('$');
            let named =
                !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if named {
                kv.push((key.to_string(), strip_quotes(value.trim()).to_string()));
                continue;
            }
        }
        positional.push(strip_quotes(trimmed).to_string());
    }
    Args { positional, kv }
}

pub(crate) fn get<'a>(args: &'a Args, key: &str) -> Option<&'a str> {
    let want = normalize_key(key);
    args.kv
        .iter()
        .find(|(name, _)| normalize_key(name) == want)
        .map(|(_, value)| value.as_str())
}

/// The arguments between a macro's parentheses, as written, quotes kept;
/// an empty one stays, so those after it keep their places.
pub(crate) fn split_args(input: &str) -> Vec<String> {
    if input.trim().is_empty() {
        return Vec::new();
    }
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in input.chars() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            current.push(ch);
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            current.push(ch);
            continue;
        }
        if ch == ',' {
            push_arg(&mut args, &mut current);
            continue;
        }
        current.push(ch);
    }
    push_arg(&mut args, &mut current);
    args
}

pub(super) fn push_arg(args: &mut Vec<String>, current: &mut String) {
    args.push(current.trim().to_string());
    current.clear();
}

pub(super) fn strip_quotes(text: &str) -> &str {
    let bytes = text.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

pub(super) fn strip_comment(raw: &str) -> &str {
    let line = raw.trim();
    let mut quote: Option<char> = None;
    for (index, ch) in line.char_indices() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == '%' && line[index..].starts_with("%%") {
            return line[..index].trim();
        }
    }
    line
}

pub(super) fn split_brace(line: &str) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    for (index, ch) in line.char_indices() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == '{' {
            return Some((line[..index].trim(), line[index + 1..].trim()));
        }
    }
    None
}

pub(super) fn split_trailing_close(line: &str) -> (&str, bool) {
    let mut quote: Option<char> = None;
    let mut close_at = None;
    for (index, ch) in line.char_indices() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == '}' {
            close_at = Some(index);
        } else if !ch.is_whitespace() {
            close_at = None;
        }
    }
    match close_at {
        Some(index) => (line[..index].trim(), true),
        None => (line.trim(), false),
    }
}

pub(super) fn access_head(line: &str) -> bool {
    let head = line
        .split(|ch: char| ch.is_whitespace() || ch == ':')
        .next()
        .unwrap_or("");
    head.eq_ignore_ascii_case("accTitle") || head.eq_ignore_ascii_case("accDescr")
}

pub(super) fn opens_block(line: &str) -> bool {
    split_brace(line).is_some() || line.contains('{')
}

pub(super) fn closes_block(line: &str) -> bool {
    split_trailing_close(line).1 || line.contains('}')
}

pub(crate) fn norm_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|ch| match ch {
            '-' => '_',
            _ => ch.to_ascii_lowercase(),
        })
        .collect()
}

pub(super) fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|ch| *ch != '_' && *ch != '-')
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub(super) fn unescape(text: &str) -> String {
    text.replace("\\n", "\n").replace("\\t", " ")
}

pub(super) fn optional(text: Option<&str>) -> Option<String> {
    text.map(unescape)
}

pub(super) fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|value| !value.trim().is_empty())
}
