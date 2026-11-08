//! A `note`, `hnote` or `rnote` is one mark.
//!
//! The caret anywhere in it, including on a participant the note is placed
//! over, lights the note's own text. `end note` lights nothing.

use super::text;

pub(super) enum Aim {
    /// This column is not inside a note.
    Outside,
    /// `end note`, which draws nothing.
    Blank,
    /// The drawn note text. `row` and `start` are where its copy is counted.
    Text {
        label: String,
        row: usize,
        start: usize,
    },
}

/// The note the caret is in, when the line belongs to one.
pub(super) fn aim(source: &str, row: usize, col: usize, labels: &[&str]) -> Aim {
    let Some(span) = span_at(source, row) else {
        return Aim::Outside;
    };
    if span.end == Some(row) {
        return Aim::Blank;
    }
    let Some(found) = label_at(source, &span, row, col, labels) else {
        return Aim::Outside;
    };
    Aim::Text {
        label: found.text,
        row: span.header,
        start: found.start,
    }
}

struct Span {
    header: usize,
    /// The `end note` line, when the note has one.
    end: Option<usize>,
    inline: bool,
}

struct Found {
    text: String,
    /// Byte on the header line where this copy starts, or past the line
    /// when the text is on a later line.
    start: usize,
}

fn span_at(source: &str, row: usize) -> Option<Span> {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        if !is_header(lines[i]) {
            i += 1;
            continue;
        }
        let header = i;
        if colon(lines[i]).is_some() {
            if row == header {
                return Some(Span {
                    header,
                    end: None,
                    inline: true,
                });
            }
            i += 1;
            continue;
        }
        let mut end = None;
        let mut j = i + 1;
        while j < lines.len() {
            if is_end(lines[j]) {
                end = Some(j);
                break;
            }
            if is_header(lines[j]) {
                break;
            }
            j += 1;
        }
        let covers = match end {
            Some(end) => (header..=end).contains(&row),
            None => row == header,
        };
        if covers {
            return Some(Span {
                header,
                end,
                inline: false,
            });
        }
        i = end.map_or(j, |end| end + 1);
    }
    None
}

fn label_at(source: &str, span: &Span, row: usize, col: usize, labels: &[&str]) -> Option<Found> {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut pieces = Vec::new();
    let mut mark = usize::MAX;
    if span.inline {
        let line = lines[span.header];
        // The drawing's match starts at the `:`, which hugs the text.
        mark = colon(line)?;
        let body = mark + 1;
        for piece in pieces_of(&line[body..]) {
            pieces.push(Piece {
                row: span.header,
                start: body + piece.start,
                end: body + piece.end,
                text: piece.text,
            });
        }
    } else {
        let last = span.end.unwrap_or(span.header);
        for (at, line) in lines.iter().enumerate().take(last).skip(span.header + 1) {
            for piece in pieces_of(line) {
                pieces.push(Piece {
                    row: at,
                    start: piece.start,
                    end: piece.end,
                    text: piece.text,
                });
            }
        }
    }
    let full = pieces
        .iter()
        .map(|piece| piece.text.as_str())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !full.is_empty() && labels.iter().any(|label| *label == full) {
        return Some(Found {
            text: full,
            start: mark,
        });
    }
    let drawn = |text: &str| labels.contains(&text);
    let hit = pieces.iter().find(|piece| {
        piece.row == row && col >= piece.start && col <= piece.end && drawn(&piece.text)
    });
    let piece = hit.or_else(|| pieces.iter().find(|piece| drawn(&piece.text)))?;
    let start = if piece.row == span.header {
        mark
    } else {
        usize::MAX
    };
    Some(Found {
        text: piece.text.clone(),
        start,
    })
}

/// A drawn stretch of the note. `start..end` are its source bytes on `row`,
/// not the length of `text`, which has lost its indent and markup.
struct Piece {
    row: usize,
    start: usize,
    end: usize,
    text: String,
}

struct Raw {
    start: usize,
    end: usize,
    text: String,
}

/// The visible stretches of `line`, split where `\n` breaks the drawing.
fn pieces_of(line: &str) -> Vec<Raw> {
    let mut out = Vec::new();
    let bytes = line.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' && bytes[i + 1] == b'n' && (i == 0 || bytes[i - 1] != b'\\') {
            push_raw(&mut out, line, start, i);
            i += 2;
            start = i;
            continue;
        }
        i += 1;
    }
    push_raw(&mut out, line, start, line.len());
    out
}

fn push_raw(out: &mut Vec<Raw>, line: &str, start: usize, end: usize) {
    let Some(norm) = text::read(&line[start..end], true) else {
        return;
    };
    if norm.text.is_empty() {
        return;
    }
    let (from, to) = norm.map(0, norm.text.len()).unwrap_or((0, end - start));
    out.push(Raw {
        start: start + from,
        end: start + to,
        text: norm.text,
    });
}

/// A note header names where it sits: `over`, `left`, `right` or `across`.
/// A body line that only says `note` is not another header.
fn is_header(line: &str) -> bool {
    let trimmed = line.trim_start();
    let Some(word) = ["hnote", "rnote", "note"]
        .into_iter()
        .find(|word| word_at(trimmed, word))
    else {
        return false;
    };
    let rest = trimmed[word.len()..].trim_start();
    ["over", "left", "right", "across"]
        .into_iter()
        .any(|place| word_at(rest, place))
}

fn is_end(line: &str) -> bool {
    let flat: String = line
        .split_whitespace()
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(flat.as_str(), "endnote" | "endhnote" | "endrnote")
}

fn word_at(line: &str, word: &str) -> bool {
    let Some(head) = line.get(..word.len()) else {
        return false;
    };
    if !head.eq_ignore_ascii_case(word) {
        return false;
    }
    line[word.len()..]
        .chars()
        .next()
        .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
}

/// Byte of the first `:` that is not inside quotes.
fn colon(line: &str) -> Option<usize> {
    let mut quoted = false;
    for (i, byte) in line.bytes().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b':' if !quoted => return Some(i),
            _ => {}
        }
    }
    None
}
