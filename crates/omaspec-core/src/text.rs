//! Line handling shared by the modules.

use std::borrow::Cow;
use std::ops::Range;

/// A lazily compiled, process-wide regex: `re!(r"...")`.
macro_rules! re {
    ($pattern:expr) => {{
        static RE: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| regex::Regex::new($pattern).expect("valid regex"));
        &*RE
    }};
}
pub(crate) use re;

pub(crate) fn strip_bom(code: &str) -> &str {
    code.strip_prefix('\u{feff}').unwrap_or(code)
}

/// `\r\n` as `\n`. A lone `\r` stays inside its line, as in the editor
/// and every other line count here; counting it as a break would put
/// findings on the wrong lines.
pub(crate) fn normalize_newlines(code: &str) -> Cow<'_, str> {
    if code.contains("\r\n") {
        Cow::Owned(code.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(code)
    }
}

/// Empties `"…"` and `'…'`, so keywords and braces inside strings don't count.
pub(crate) fn strip_strings(s: &str) -> String {
    let s = re!(r#""(?:\\.|[^"\\])*""#).replace_all(s, r#""""#);
    re!(r"'(?:\\.|[^'\\])*'").replace_all(&s, "''").into_owned()
}

/// `"name"` → `name`.
pub(crate) fn unquote(s: &str) -> &str {
    let s = s.strip_prefix('"').unwrap_or(s);
    s.strip_suffix('"').unwrap_or(s).trim()
}

/// The trimmed lines that carry meaning, at most `limit`: no blank lines, no
/// PlantUML comments (`'`, `/' … '/`), no Mermaid comments (`%%`, but
/// `%%{init}%%` stays) and no leading YAML front matter.
pub(crate) fn significant_lines(code: &str, limit: usize) -> Vec<&str> {
    let raw: Vec<&str> = strip_bom(code)
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut i = 0;
    let first = raw
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(raw.len());
    if raw.get(first).is_some_and(|l| l.trim() == "---") {
        let max = raw.len().min(first + 41);
        if let Some(closer) = (first + 1..max).find(|&j| raw[j].trim() == "---") {
            i = closer + 1;
        }
    }

    let mut out = Vec::new();
    let mut in_block_comment = false;
    // A Mermaid `%%{init: …}%%` spanning lines counts as its first line.
    let mut in_init = false;
    while i < raw.len() && out.len() < limit {
        let line = raw[i];
        i += 1;
        if in_block_comment {
            if line.contains("'/") {
                in_block_comment = false;
            }
            continue;
        }
        if in_init {
            in_init = !line.contains("}%%");
            continue;
        }
        let lead = line.trim_start();
        if lead.starts_with("/'") {
            in_block_comment = !line.trim_end().ends_with("'/");
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || lead.starts_with('\'') {
            continue;
        }
        if lead.starts_with("%%") && !trimmed.starts_with("%%{") {
            continue;
        }
        in_init = trimmed.starts_with("%%{") && !trimmed.contains("}%%");
        out.push(trimmed);
    }
    out
}

/// `note`, `hnote`, `rnote`, also `floating note` and `/ note` (aligned).
pub(crate) fn is_note_keyword(t: &str) -> bool {
    re!(r"(?i)^/?\s*(?:floating\s+)?(?:rnote|hnote|note)\b").is_match(t)
}

/// A note that ends on its own line: `note left: text` or `note "x" as N`.
pub(crate) fn is_note_one_line(t: &str) -> bool {
    is_note_keyword(t) && (t.contains(':') || re!(r#"(?i)"[^"]*"\s+as\b"#).is_match(t))
}

/// A note whose text follows on the next lines, up to `end note`.
pub(crate) fn is_note_block_open(t: &str) -> bool {
    is_note_keyword(t) && !is_note_one_line(t)
}

pub(crate) fn is_note_closer(t: &str) -> bool {
    re!(r"(?i)^end\s*(?:rnote|hnote|note)\b").is_match(t)
}

/// `{` minus `}` in `line`.
pub(crate) fn brace_balance(line: &str) -> isize {
    line.chars().fold(0, |n, c| match c {
        '{' => n + 1,
        '}' => n - 1,
        _ => n,
    })
}

/// Each line's byte range, with its newline.
pub(crate) fn line_ranges(code: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for line in code.split_inclusive('\n') {
        out.push(start..start + line.len());
        start += line.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_comments_and_front_matter() {
        let code = "---\ntitle: x\n---\n%% note\n%%{init: {}}%%\n' c\n/' a\nb '/\n\n  graph TD\n";
        assert_eq!(significant_lines(code, 10), ["%%{init: {}}%%", "graph TD"]);
    }

    #[test]
    fn multi_line_init_is_one_line() {
        let code = "%%{init: {\n  \"theme\": \"dark\"\n}}%%\nflowchart LR";
        assert_eq!(significant_lines(code, 10), ["%%{init: {", "flowchart LR"]);
    }

    #[test]
    fn one_line_block_comment() {
        assert_eq!(significant_lines("/' x '/\n@startuml", 5), ["@startuml"]);
    }

    #[test]
    fn strips_strings() {
        assert_eq!(strip_strings(r#"a "x { y" 'z' b"#), r#"a "" '' b"#);
    }
}
