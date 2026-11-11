//! Colors a host lays over the grammar's: a run of a line in a color of its
//! own, for what the host knows about a word and the grammar does not (a
//! state, an event). The grammar's spans stay as they are; a tint only
//! changes the color of the part it covers.
//!
//! Omaspec addition to scrive 0.3.0.

use std::borrow::Cow;
use std::ops::Range;

use iced::Color;
use scrive_core::{HighlightSpan, Rgba, SpanStyle};

/// A run of one line in a color of the host's.
#[derive(Clone, Debug, PartialEq)]
pub struct Tint {
    /// The buffer row.
    pub row: u32,
    /// Bytes within the line.
    pub range: Range<u32>,
    /// The color of the text there.
    pub color: Color,
}

/// The tints of `row` among `tints`, which lie in the order of their rows
/// and, within a row, of their starts.
pub fn of_row(tints: &[Tint], row: u32) -> &[Tint] {
    let start = tints.partition_point(|t| t.row < row);
    let end = start + tints[start..].partition_point(|t| t.row == row);
    &tints[start..end]
}

/// `spans` with `tints` laid over them. A line without spans is one run in
/// `plain`. A tint that does not fit the line (it was made for a text
/// before an edit) is left out, so no run ever cuts a character.
pub fn over<'a>(
    line: &str,
    spans: Option<&'a [HighlightSpan]>,
    tints: &[Tint],
    plain: Color,
) -> Option<Cow<'a, [HighlightSpan]>> {
    let fits = |t: &&Tint| {
        let (start, end) = (t.range.start as usize, t.range.end as usize);
        start < end && end <= line.len() && line.is_char_boundary(start) && line.is_char_boundary(end)
    };
    let tints: Vec<&Tint> = tints.iter().filter(fits).collect();
    if tints.is_empty() {
        return spans.map(Cow::Borrowed);
    }
    let whole;
    let base: &[HighlightSpan] = match spans {
        Some(spans) if !spans.is_empty() => spans,
        _ => {
            whole = [HighlightSpan {
                range: 0..line.len() as u32,
                style: SpanStyle { fg: rgba(plain), bold: false, italic: false },
            }];
            &whole
        }
    };
    let mut out = Vec::with_capacity(base.len() + 2 * tints.len());
    for span in base {
        let mut at = span.range.start;
        for tint in &tints {
            let start = tint.range.start.max(at);
            let end = tint.range.end.min(span.range.end);
            if start >= end {
                continue;
            }
            if start > at {
                out.push(HighlightSpan { range: at..start, style: span.style });
            }
            out.push(HighlightSpan {
                range: start..end,
                style: SpanStyle { fg: rgba(tint.color), ..span.style },
            });
            at = end;
        }
        if at < span.range.end {
            out.push(HighlightSpan { range: at..span.range.end, style: span.style });
        }
    }
    Some(Cow::Owned(out))
}

fn rgba(color: Color) -> Rgba {
    let [r, g, b, a] = color.into_rgba8();
    Rgba { r, g, b, a }
}
