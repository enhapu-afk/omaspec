//! Label text as Mermaid shows it: the HTML it allows in a label (`<br>`,
//! `<b>`, `<i>`, …), entity codes (`#quot;`, `#9829;`, `&amp;`) and
//! markdown strings (`` "`**bold** words`" ``). Nothing else is markup:
//! `--`, `//` or `**` outside a markdown string are drawn as written.

use omaspec_uml::{Block, Family, Font, Style, measure_run, measure_styled};

type Piece = (String, Style);

/// `raw` in `font`, each line broken at its spaces so it is no wider than
/// `wrap`, when that is set. A word wider than that keeps its own line.
pub(super) fn measure(raw: &str, font: Font, family: Family, wrap: Option<f32>) -> Block {
    if raw.trim().is_empty() {
        return Block::empty();
    }
    let base = Style::from(font);
    let trimmed = raw.trim();
    let lines = match trimmed
        .strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
    {
        Some(inner) => markdown(inner, base),
        None => html(raw, base),
    };
    let lines = match wrap {
        Some(width) => lines
            .into_iter()
            .flat_map(|line| wrapped(line, width, family))
            .collect(),
        None => lines,
    };
    measure_styled(lines, font, family)
}

/// Lines of `text` with its tags read and its entities decoded. A tag
/// this does not know is left out and its content shown, as a browser
/// shows an unknown element.
fn html(text: &str, base: Style) -> Vec<Vec<Piece>> {
    let mut lines = vec![Vec::new()];
    let mut style = base;
    // Open tags: the name, and the style before it.
    let mut open: Vec<(String, Style)> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        if rest.starts_with('\n') {
            flush(&mut lines, &mut buf, style);
            lines.push(Vec::new());
            i += 1;
            continue;
        }
        if let Some(tag) = tag(rest) {
            flush(&mut lines, &mut buf, style);
            i += tag.len;
            if tag.name == "br" {
                lines.push(Vec::new());
            } else if tag.closing {
                if let Some(at) = open.iter().rposition(|(name, _)| *name == tag.name) {
                    style = open[at].1;
                    open.truncate(at);
                }
            } else if !tag.empty {
                open.push((tag.name.clone(), style));
                apply(&mut style, &tag.name);
            }
            continue;
        }
        if let Some((len, c)) = entity(rest) {
            buf.push(c);
            i += len;
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        buf.push(c);
        i += c.len_utf8();
    }
    flush(&mut lines, &mut buf, style);
    lines
}

fn flush(lines: &mut [Vec<Piece>], buf: &mut String, style: Style) {
    if buf.is_empty() {
        return;
    }
    if let Some(line) = lines.last_mut() {
        line.push((std::mem::take(buf), style));
    }
}

fn apply(style: &mut Style, tag: &str) {
    match tag {
        "b" | "strong" => style.bold = true,
        "i" | "em" => style.italic = true,
        "u" | "ins" => style.underline = true,
        "s" | "del" | "strike" => style.strike = true,
        "code" | "tt" | "kbd" | "samp" => style.mono = true,
        _ => {}
    }
}

struct Tag {
    len: usize,
    name: String,
    closing: bool,
    /// Closed where it opens: `<br/>`, `<span/>`.
    empty: bool,
}

/// The tag at the start of `rest`: `<name …>`, `</name>` or `<name/>`.
fn tag(rest: &str) -> Option<Tag> {
    let body = rest.strip_prefix('<')?;
    let (closing, body) = match body.strip_prefix('/') {
        Some(after) => (true, after),
        None => (false, body),
    };
    let name_len = body
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    if name_len == 0 || !body.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    let after = &body[name_len..];
    if !after.is_empty() && !after.starts_with(['>', '/', ' ', '\t']) {
        return None;
    }
    let close = after.find('>')?;
    if after[..close].contains(['<', '\n']) {
        return None;
    }
    let consumed = rest.len() - after.len() + close + 1;
    Some(Tag {
        len: consumed,
        name: body[..name_len].to_ascii_lowercase(),
        closing,
        empty: after[..close].trim_end().ends_with('/'),
    })
}

/// The character an entity at the start of `rest` stands for, and its
/// length: Mermaid's `#quot;` and `#9829;`, HTML's `&amp;`, `&#35;` and
/// `&#x2665;`.
fn entity(rest: &str) -> Option<(usize, char)> {
    let html = rest.starts_with('&');
    if !html && !rest.starts_with('#') {
        return None;
    }
    let end = rest.bytes().take(12).position(|b| b == b';')?;
    let body = &rest[1..end];
    let c = if let Some(number) = body.strip_prefix('#').filter(|_| html) {
        code(number)?
    } else if !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) {
        char::from_u32(body.parse().ok()?)?
    } else {
        named(body)?
    };
    Some((end + 1, c))
}

fn code(number: &str) -> Option<char> {
    let value = match number.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => number.parse().ok()?,
    };
    char::from_u32(value)
}

fn named(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "deg" => '°',
        "hellip" => '…',
        "mdash" => '—',
        "ndash" => '–',
        "laquo" => '«',
        "raquo" => '»',
        "larr" => '←',
        "rarr" => '→',
        "uarr" => '↑',
        "darr" => '↓',
        "harr" => '↔',
        "lArr" => '⇐',
        "rArr" => '⇒',
        "hArr" => '⇔',
        "times" => '×',
        "divide" => '÷',
        "plusmn" => '±',
        "le" => '≤',
        "ge" => '≥',
        "ne" => '≠',
        "infin" => '∞',
        "middot" => '·',
        "bull" => '•',
        "micro" => 'µ',
        "euro" => '€',
        "pound" => '£',
        "sect" => '§',
        "para" => '¶',
        "hearts" => '♥',
        "auml" => 'ä',
        "ouml" => 'ö',
        "uuml" => 'ü',
        "Auml" => 'Ä',
        "Ouml" => 'Ö',
        "Uuml" => 'Ü',
        "szlig" => 'ß',
        _ => return None,
    })
}

/// A markdown string: `**bold**` or `__bold__`, `*italic*` or `_italic_`.
/// A marker without its partner later on the line is text.
fn markdown(text: &str, base: Style) -> Vec<Vec<Piece>> {
    let mut lines = Vec::new();
    for line in text.split('\n') {
        let mut pieces = Vec::new();
        let mut style = base;
        let mut buf = String::new();
        let mut i = 0;
        while i < line.len() {
            let rest = &line[i..];
            let marker = ["**", "__", "*", "_"]
                .into_iter()
                .find(|m| rest.starts_with(m));
            if let Some(marker) = marker {
                let bold = marker.len() == 2;
                let on = if bold { style.bold } else { style.italic };
                let inside_word = marker.starts_with('_')
                    && !on
                    && line[..i]
                        .chars()
                        .next_back()
                        .is_some_and(char::is_alphanumeric);
                if (on || rest[marker.len()..].contains(marker)) && !inside_word {
                    if !buf.is_empty() {
                        pieces.push((std::mem::take(&mut buf), style));
                    }
                    if bold {
                        style.bold = !on;
                    } else {
                        style.italic = !on;
                    }
                    i += marker.len();
                    continue;
                }
            }
            if let Some((len, c)) = entity(rest) {
                buf.push(c);
                i += len;
                continue;
            }
            let c = rest.chars().next().unwrap_or(' ');
            buf.push(c);
            i += c.len_utf8();
        }
        if !buf.is_empty() {
            pieces.push((buf, style));
        }
        lines.push(pieces);
    }
    lines
}

/// `line` broken at its spaces into lines no wider than `width`.
fn wrapped(line: Vec<Piece>, width: f32, family: Family) -> Vec<Vec<Piece>> {
    let mut out = Vec::new();
    let mut current: Vec<Piece> = Vec::new();
    let mut used = 0.0;
    let mut space: Option<Piece> = None;
    for (text, style) in line {
        for (word, blank) in words(&text) {
            if blank {
                if !current.is_empty() {
                    space = Some((word.to_owned(), style));
                }
                continue;
            }
            let w = measure_run(word, &style, family);
            let gap = space
                .as_ref()
                .map_or(0.0, |(s, style)| measure_run(s, style, family));
            if space.is_some() && used + gap + w > width + 0.5 {
                out.push(merged(std::mem::take(&mut current)));
                used = 0.0;
                space = None;
            }
            if let Some(piece) = space.take() {
                current.push(piece);
                used += gap;
            }
            current.push((word.to_owned(), style));
            used += w;
        }
    }
    out.push(merged(current));
    out
}

/// `text` cut into runs of spaces and runs of the rest.
fn words(text: &str) -> impl Iterator<Item = (&str, bool)> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let blank = first == ' ';
        let len = rest
            .find(|c: char| (c == ' ') != blank)
            .unwrap_or(rest.len());
        let (word, after) = rest.split_at(len);
        rest = after;
        Some((word, blank))
    })
}

/// Neighbors in the same style joined into one piece.
fn merged(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    for (text, style) in pieces {
        match out.last_mut() {
            Some((last, last_style)) if *last_style == style => last.push_str(&text),
            _ => out.push((text, style)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use omaspec_uml::Color;

    use super::*;

    fn font() -> Font {
        Font::new(16.0, Color(0, 0, 0))
    }

    fn text_of(block: &Block) -> Vec<String> {
        block
            .lines
            .iter()
            .map(|line| line.runs.iter().map(|run| run.text.as_str()).collect())
            .collect()
    }

    #[test]
    fn plantuml_markup_is_text() {
        let block = measure("a -- b // c **d**", font(), Family::Sans, None);
        assert_eq!(text_of(&block), ["a -- b // c **d**"]);
        let style = block.lines[0].runs[0].style;
        assert!(!style.bold && !style.italic && !style.strike);
    }

    #[test]
    fn tags_break_lines_and_style_runs() {
        let block = measure(
            "one<br/>two <b>bold</b> <span class=\"x\">kept</span>",
            font(),
            Family::Sans,
            None,
        );
        assert_eq!(text_of(&block), ["one", "two bold kept"]);
        let runs = &block.lines[1].runs;
        assert!(runs.iter().any(|run| run.text == "bold" && run.style.bold));
        assert!(runs.iter().all(|run| run.text == "bold" || !run.style.bold));
        // Not a tag: shown as written.
        assert_eq!(
            text_of(&measure("a < b > c", font(), Family::Sans, None)),
            ["a < b > c"]
        );
    }

    #[test]
    fn entities_are_decoded() {
        let block = measure(
            "#quot;x#quot; #9829; &amp; &#35; #1 item;",
            font(),
            Family::Sans,
            None,
        );
        assert_eq!(text_of(&block), ["\"x\" ♥ & # #1 item;"]);
    }

    #[test]
    fn markdown_strings_are_styled() {
        let block = measure("`**bold** and _it_ snake_case`", font(), Family::Sans, None);
        assert_eq!(text_of(&block), ["bold and it snake_case"]);
        let runs = &block.lines[0].runs;
        assert!(runs[0].style.bold);
        assert!(runs.iter().any(|run| run.text == "it" && run.style.italic));
    }

    #[test]
    fn long_lines_wrap_at_spaces() {
        let block = measure(
            "Order history with filters, export and paging",
            font(),
            Family::Sans,
            Some(200.0),
        );
        assert!(block.lines.len() > 1, "{:?}", text_of(&block));
        assert!(block.width <= 200.5);
        let joined = text_of(&block).join(" ");
        assert_eq!(joined, "Order history with filters, export and paging");
        let long = measure(
            "Supercalifragilisticexpialidocious",
            font(),
            Family::Sans,
            Some(50.0),
        );
        assert_eq!(long.lines.len(), 1);
    }
}
