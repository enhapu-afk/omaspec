//! The words of a block diagram's line: ids with their shapes, arrows,
//! `&` between several, quoted labels, comments.

use crate::color::Color;
use crate::text::{self, Family, Font};

pub(super) fn tokens(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    let mut depth = 0i32;
    let mut quote = false;
    let mut byte = 0;
    for ch in line.chars() {
        let next = byte + ch.len_utf8();
        if quote {
            if ch == '"' {
                quote = false;
            }
        } else if ch == '"' && depth > 0 {
            quote = true;
            if start.is_none() {
                start = Some(byte);
            }
        } else if matches!(ch, '[' | '(' | '{') {
            depth += 1;
            if start.is_none() {
                start = Some(byte);
            }
        } else if matches!(ch, ']' | ')' | '}') {
            depth = depth.saturating_sub(1);
            if start.is_none() {
                start = Some(byte);
            }
        } else if ch.is_whitespace() && depth <= 0 {
            depth = 0;
            if let Some(at) = start.take() {
                out.push(&line[at..byte]);
            }
        } else if start.is_none() {
            start = Some(byte);
        }
        byte = next;
    }
    if let Some(at) = start {
        let token = line[at..].trim();
        if !token.is_empty() {
            out.push(token);
        }
    }
    out
}

pub(super) fn split_amp(side: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut byte = 0;
    let mut depth = 0i32;
    let mut quote = false;
    for ch in side.chars() {
        let next = byte + ch.len_utf8();
        if ch == '"' {
            quote = !quote;
        } else if !quote {
            match ch {
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth -= 1,
                '&' if depth == 0 => {
                    out.push(&side[start..byte]);
                    start = next;
                }
                _ => {}
            }
        }
        byte = next;
    }
    out.push(&side[start..]);
    out
}

pub(super) fn find_arrow(line: &str) -> Option<(usize, &'static str)> {
    const ARROWS: &[&str] = &[
        "<-.->", "<-->", "-.->", "<-.-", "==>", "<==", "===", "-->", "<--", "---", "-.-", "->",
        "<-",
    ];
    scan(line, |rest| {
        ARROWS.iter().copied().find(|arrow| rest.starts_with(arrow))
    })
}

pub(super) fn find_opener(line: &str, opener: &str) -> Option<usize> {
    scan(line, |rest| rest.starts_with(opener).then_some(())).map(|(at, _)| at)
}

pub(super) fn scan<T>(line: &str, mut hit: impl FnMut(&str) -> Option<T>) -> Option<(usize, T)> {
    let mut byte = 0;
    let mut depth = 0i32;
    let mut quote = false;
    for ch in line.chars() {
        let next = byte + ch.len_utf8();
        if ch == '"' {
            quote = !quote;
        } else if !quote {
            match ch {
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth -= 1,
                _ if depth == 0 => {
                    if let Some(found) = hit(&line[byte..]) {
                        return Some((byte, found));
                    }
                }
                _ => {}
            }
        }
        byte = next;
    }
    None
}

pub(super) fn strip_comment(line: &str) -> &str {
    let mut quote = false;
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    for (index, (byte, ch)) in chars.iter().enumerate() {
        if *ch == '"' {
            quote = !quote;
        }
        if !quote && *ch == '%' && chars.get(index + 1).is_some_and(|(_, next)| *next == '%') {
            return line[..*byte].trim();
        }
    }
    line.trim()
}

pub(super) fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

pub(super) fn plain(value: &str) -> String {
    value
        .replace("<br/>", "\n")
        .replace("<br>", "\n")
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

pub(super) fn readable(value: &str) -> bool {
    text::block(value, Font::new(16.0, Color::BLACK), Family::Sans).is_ok()
}
