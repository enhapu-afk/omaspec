//! One reading of a source line: the characters the drawing shows, and where
//! each of them sits in the source.
//!
//! Breaks (`\n`, `<br>`) and a few entities are resolved first, then space is
//! collapsed, and only then is Creole (`**`, `//`, tags, `~`) removed. That
//! is the order Ctrl+click already used, so a label search and the caret see
//! the same text. A comment line is empty.

/// The line as a label is read, with each character mapped back to the source.
pub(super) struct Norm {
    pub(super) text: String,
    /// One entry per character of `text`: its byte, and the source bytes it came from.
    chars: Vec<(usize, usize, usize)>,
}

impl Norm {
    /// Source bytes of `text[from..to]`.
    pub(super) fn map(&self, from: usize, to: usize) -> Option<(usize, usize)> {
        let start = self.chars.iter().find(|ch| ch.0 == from)?.1;
        let end = self.chars.iter().rev().find(|ch| ch.0 < to)?.2;
        Some((start, end))
    }

    /// Source bytes of the character of `text` that starts at `at`.
    fn span_at(&self, at: usize) -> (usize, usize) {
        let (_, start, end) = self
            .chars
            .iter()
            .find(|ch| ch.0 == at)
            .copied()
            .expect("index on a character");
        (start, end)
    }
}

/// `line` as the drawing shows it. `strip` also drops Creole. A comment is
/// nothing, so the caller treats it as an empty line.
pub(super) fn read(line: &str, strip: bool) -> Option<Norm> {
    if comment(line) {
        return None;
    }
    let norm = collapse(decoded(line));
    Some(if strip { strip_markup(norm) } else { norm })
}

/// [`read`] as the string a search compares. A comment is empty.
pub(super) fn visible(line: &str, strip: bool) -> String {
    read(line, strip).map(|norm| norm.text).unwrap_or_default()
}

/// A line a drawing skips: `'` or `%%` after indentation.
pub(super) fn comment(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with('\'') || trimmed.starts_with("%%")
}

/// Where `needle` stands in `hay` on its own. An ellipsis may end inside a word.
pub(super) fn locate_in(hay: &str, needle: &str, relax_end: bool) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = hay[from..].find(needle) {
        let at = from + rel;
        let end = at + needle.len();
        if bounded(hay, at, needle, relax_end) {
            out.push((at, end));
        }
        let step = needle.chars().next().map_or(1, char::len_utf8);
        from = at + step;
        if from > hay.len() {
            break;
        }
    }
    out
}

fn bounded(hay: &str, at: usize, needle: &str, relax_end: bool) -> bool {
    let word = |ch: Option<char>| ch.is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
    let before = hay[..at].chars().next_back();
    let after = hay[at + needle.len()..].chars().next();
    let starts = word(needle.chars().next());
    let ends = word(needle.chars().last());
    !(starts && word(before)) && (relax_end || !(ends && word(after)))
}

/// Breaks and entities, in the order the drawing resolves them.
fn decoded(line: &str) -> Vec<(char, usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < line.len() {
        if line[i..].starts_with("\\n") {
            out.push((' ', i, i + 2));
            i += 2;
            continue;
        }
        if let Some(len) = break_len(&line[i..]) {
            out.push((' ', i, i + len));
            i += len;
            continue;
        }
        if let Some((ch, len)) = entity(&line[i..]) {
            out.push((ch, i, i + len));
            i += len;
            continue;
        }
        let ch = line[i..].chars().next().expect("byte on a character");
        let len = ch.len_utf8();
        out.push((ch, i, i + len));
        i += len;
    }
    out
}

fn collapse(mut atoms: Vec<(char, usize, usize)>) -> Norm {
    while atoms.first().is_some_and(|atom| atom.0.is_whitespace()) {
        atoms.remove(0);
    }
    while atoms.last().is_some_and(|atom| atom.0.is_whitespace()) {
        atoms.pop();
    }
    let mut text = String::new();
    let mut chars = Vec::new();
    let mut pending: Option<(usize, usize)> = None;
    for (ch, start, end) in atoms {
        if ch.is_whitespace() {
            pending = Some(match pending {
                Some((from, _)) => (from, end),
                None => (start, end),
            });
            continue;
        }
        if let Some((from, to)) = pending.take()
            && !text.is_empty()
        {
            chars.push((text.len(), from, to));
            text.push(' ');
        }
        chars.push((text.len(), start, end));
        text.push(ch);
    }
    Norm { text, chars }
}

/// Creole taken off a line that was already collapsed, then collapsed again,
/// the same way the markup pass used to run on the drawn string.
fn strip_markup(norm: Norm) -> Norm {
    let mut kept = Vec::new();
    let mut i = 0;
    while i < norm.text.len() {
        if let Some(pair) = ["**", "//", "__", "\"\""]
            .iter()
            .find(|pair| norm.text[i..].starts_with(**pair))
        {
            i += pair.len();
            continue;
        }
        let ch = norm.text[i..].chars().next().expect("byte on a character");
        let len = ch.len_utf8();
        if ch == '~'
            && let Some(next) = norm.text[i + len..].chars().next()
        {
            let (from, _) = norm.span_at(i);
            let (_, to) = norm.span_at(i + len);
            kept.push((next, from, to));
            i += len + next.len_utf8();
            continue;
        }
        if ch == '<'
            && let Some(tag) = tag_len(&norm.text[i..])
        {
            i += tag;
            continue;
        }
        let (from, to) = norm.span_at(i);
        kept.push((ch, from, to));
        i += len;
    }
    collapse(kept)
}

fn break_len(text: &str) -> Option<usize> {
    ["<br/>", "<br />", "<br>", "<BR>", "<BR/>"]
        .iter()
        .find(|tag| text.starts_with(**tag))
        .map(|tag| tag.len())
}

fn entity(text: &str) -> Option<(char, usize)> {
    for (token, ch) in [
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("#quot;", '"'),
        ("&amp;", '&'),
    ] {
        if text.starts_with(token) {
            return Some((ch, token.len()));
        }
    }
    None
}

fn tag_len(text: &str) -> Option<usize> {
    const TAGS: [&str; 8] = ["b", "i", "u", "s", "strike", "del", "color", "size"];
    let rest = text.strip_prefix('<')?;
    let close = rest.find('>')?;
    let inner = rest[..close].trim_start_matches('/').to_ascii_lowercase();
    let name = inner.split(':').next().unwrap_or_default();
    TAGS.contains(&name).then_some(close + 2)
}
