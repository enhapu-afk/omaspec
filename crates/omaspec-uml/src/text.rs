//! Text: the part of PlantUML's Creole markup the engine draws, and widths
//! from built-in font metrics, so no font file is read.
//!
//! Markup it draws: `\n`, `<b>`, `<i>`, `<u>`, `<s>`, `<color:…>`,
//! `<size:…>`, `**bold**`, `//italic//`, `__underline__`, `--strike--`,
//! `""mono""`, `~` to escape, Creole tables (`|= a |= b |` over
//! `| 1 | 2 |`) and links (`[[url]]`, `[[url label]]`, `[[url{tooltip}
//! label]]`), underlined in the text's color. Markup it knows but does not
//! draw (images, lists, headings, rules, …) is an error naming it; anything
//! else that looks like a tag is shown as written, as PlantUML does.

use crate::color::{self, Color};
use crate::metrics;

/// The font family of the whole diagram, by its metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Sans,
    Serif,
    Mono,
}

impl Family {
    /// The family whose metrics fit the font `name` best. `name` may be a
    /// CSS font list (`Inter, "Segoe UI", sans-serif`): the first name that
    /// tells a family decides, as the first one installed draws, and a name
    /// that tells none passes on to the next. Sans when none does.
    pub fn of(name: &str) -> Family {
        name.split(',')
            .find_map(|one| Family::named(one.trim().trim_matches(['"', '\''])))
            .unwrap_or(Family::Sans)
    }

    fn named(name: &str) -> Option<Family> {
        let n = name.to_ascii_lowercase();
        let word = |words: &[&str]| {
            n.split(|c: char| !c.is_ascii_alphanumeric())
                .any(|w| words.contains(&w))
        };
        if n.contains("mono") || n.contains("courier") || n.contains("consol") {
            Some(Family::Mono)
        } else if (n.contains("serif") && !n.contains("sans")) || word(&["times", "georgia"]) {
            Some(Family::Serif)
        } else if n.contains("sans")
            || word(&[
                "arial",
                "helvetica",
                "verdana",
                "trebuchet",
                "segoe",
                "inter",
                "system",
            ])
        {
            Some(Family::Sans)
        } else if word(&["menlo", "monaco"]) {
            Some(Family::Mono)
        } else {
            None
        }
    }
}

/// How an element's text starts out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Font {
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub color: Color,
}

impl Font {
    pub fn new(size: f32, color: Color) -> Font {
        Font {
            size,
            bold: false,
            italic: false,
            color,
        }
    }

    pub fn bold(self) -> Font {
        Font { bold: true, ..self }
    }

    pub fn italic(self) -> Font {
        Font {
            italic: true,
            ..self
        }
    }
}

/// The style of one piece of a line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub mono: bool,
    pub color: Color,
}

impl From<Font> for Style {
    fn from(font: Font) -> Style {
        Style {
            size: font.size,
            bold: font.bold,
            italic: font.italic,
            underline: false,
            strike: false,
            mono: false,
            color: font.color,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: Style,
    pub width: f32,
    /// Where a click on it leads, from `[[…]]`.
    pub link: Option<Link>,
}

/// A link in the text, as written: the sanitizer decides which addresses
/// a picture keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    /// Shown when the pointer rests on it.
    pub tooltip: Option<String>,
}

/// A piece of a line before it is measured.
struct Piece {
    text: String,
    style: Style,
    link: Option<Link>,
}

/// Space on the left and the right of a Creole table cell.
pub(crate) const TABLE_PAD: f32 = 4.0;

/// One cell of a Creole table row. `left` is the column's left edge inside
/// the row; `width` includes the padding on both sides.
#[derive(Debug, Clone, PartialEq)]
pub struct TableCell {
    pub left: f32,
    pub width: f32,
    /// The cell's lines, top aligned. None of them is itself a table.
    pub lines: Vec<Line>,
}

/// A row of a Creole table. Every row of one table has the same width, and
/// the first has `top`, so the grid's shared edges are drawn once.
#[derive(Debug, Clone, PartialEq)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
    /// The grid, in the text's color.
    pub color: Color,
    pub top: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub runs: Vec<Run>,
    pub width: f32,
    pub height: f32,
    /// From the line's top to its baseline.
    pub ascent: f32,
    /// Set on a table row; the words then live in the cells.
    pub table: Option<TableRow>,
    /// An item of a list: it starts at the left edge of its block, also
    /// where the block's other lines are centered or stand at the right.
    pub flush: bool,
}

/// Lines of text, measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub lines: Vec<Line>,
    pub width: f32,
    pub height: f32,
}

impl Block {
    pub fn empty() -> Block {
        Block {
            lines: Vec::new(),
            width: 0.0,
            height: 0.0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Line height and ascent as shares of the font size (Java's metrics for
/// the logical sans-serif font, which PlantUML's spacing is made for).
const LINE: f32 = 1.164;
const ASCENT: f32 = 0.928;

/// `text` in `font`, measured. `\n` and real line breaks start new lines.
/// Consecutive lines that start with `|` are one Creole table.
pub fn block(text: &str, font: Font, family: Family) -> Result<Block, String> {
    let mut lines = Vec::new();
    let mut rows: Vec<&str> = Vec::new();
    let flush = |lines: &mut Vec<Line>, rows: &mut Vec<&str>| -> Result<(), String> {
        if !rows.is_empty() {
            lines.extend(table_rows(rows, font, family)?);
            rows.clear();
        }
        Ok(())
    };
    // How far a numbered list has counted, by its depth.
    let mut counts: Vec<u32> = Vec::new();
    for line in text.split('\n') {
        if is_table_line(line) {
            rows.push(line);
        } else {
            flush(&mut lines, &mut rows)?;
            for piece in split_lines(line) {
                let Some((depth, numbered, words)) = list_item(piece) else {
                    counts.clear();
                    check_block_markup(piece)?;
                    lines.push(line_of(
                        inline(piece, Style::from(font))?,
                        font.size,
                        family,
                    ));
                    continue;
                };
                counts.resize(depth, 0);
                let mark = if numbered {
                    counts[depth - 1] += 1;
                    format!("{}. ", counts[depth - 1])
                } else {
                    "\u{2022}  ".to_owned()
                };
                let mut pieces = vec![Piece {
                    text: format!("{}{mark}", "    ".repeat(depth - 1)),
                    style: Style::from(font),
                    link: None,
                }];
                pieces.extend(inline(words, Style::from(font))?);
                let mut item = line_of(pieces, font.size, family);
                item.flush = true;
                lines.push(item);
            }
        }
    }
    flush(&mut lines, &mut rows)?;
    Ok(stack(lines))
}

/// Lines of styled pieces, measured as they are: no markup is read. An
/// empty line is as tall as `font`.
pub fn styled(lines: Vec<Vec<(String, Style)>>, font: Font, family: Family) -> Block {
    stack(
        lines
            .into_iter()
            .map(|pieces| {
                let pieces = pieces
                    .into_iter()
                    .map(|(text, style)| Piece {
                        text,
                        style,
                        link: None,
                    })
                    .collect();
                line_of(pieces, font.size, family)
            })
            .collect(),
    )
}

fn line_of(pieces: Vec<Piece>, size: f32, family: Family) -> Line {
    let mut runs = Vec::new();
    let mut width: f32 = 0.0;
    let mut size = size;
    for Piece { text, style, link } in pieces {
        if text.is_empty() {
            continue;
        }
        let w = measure(&text, &style, family);
        width += w;
        size = size.max(style.size);
        runs.push(Run {
            text,
            style,
            width: w,
            link,
        });
    }
    Line {
        runs,
        width,
        height: size * LINE,
        ascent: size * ASCENT,
        table: None,
        flush: false,
    }
}

fn stack(lines: Vec<Line>) -> Block {
    let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    let height = lines.iter().map(|l| l.height).sum();
    Block {
        lines,
        width,
        height,
    }
}

/// The width of `text` in `style`.
pub fn measure(text: &str, style: &Style, family: Family) -> f32 {
    let family = if style.mono { Family::Mono } else { family };
    let em: u32 = text.chars().map(|c| advance(c, family, style.bold)).sum();
    em as f32 / 1000.0 * style.size
}

/// The advance of `c` in thousandths of an em.
fn advance(c: char, family: Family, bold: bool) -> u32 {
    let code = c as u32;
    if c == '\t' {
        return 4 * advance(' ', family, bold);
    }
    if matches!(code, 0x0300..=0x036F | 0x200B..=0x200F | 0xFE00..=0xFE0F | 0xFEFF)
        || c.is_control()
    {
        return 0;
    }
    // A picture of the emoji font is wider than its em, and a word after
    // it would stand on it.
    if matches!(code, 0x1F000..=0x1FAFF) || emoji_sign(code) {
        return 1250;
    }
    if is_wide(code) {
        return 1000;
    }
    if family == Family::Mono {
        return 600;
    }
    let table: &[u16] = match (family, bold) {
        (Family::Serif, false) => &metrics::SERIF,
        (Family::Serif, true) => &metrics::SERIF_BOLD,
        (_, false) => &metrics::SANS,
        (_, true) => &metrics::SANS_BOLD,
    };
    let index = match code {
        0x20..=0x7E => Some(code as usize - 0x20),
        0xA0..=0x17F => Some(95 + code as usize - 0xA0),
        _ => metrics::EXTRA.iter().position(|&e| e == c).map(|i| 319 + i),
    };
    match index {
        Some(i) => u32::from(table[i]),
        // Greek, Cyrillic and the rest: an average letter.
        None if family == Family::Serif => 500,
        None => 556,
    }
}

/// The older signs Unicode shows as an emoji unless told otherwise
/// (`⭐`, `✋`, `✅`, `⚡`): the emoji font draws them, as wide as its
/// other pictures.
fn emoji_sign(code: u32) -> bool {
    matches!(code,
        0x231A..=0x231B
        | 0x23E9..=0x23EC
        | 0x23F0
        | 0x23F3
        | 0x25FD..=0x25FE
        | 0x2614..=0x2615
        | 0x2648..=0x2653
        | 0x267F
        | 0x2693
        | 0x26A1
        | 0x26AA..=0x26AB
        | 0x26BD..=0x26BE
        | 0x26C4..=0x26C5
        | 0x26CE
        | 0x26D4
        | 0x26EA
        | 0x26F2..=0x26F3
        | 0x26F5
        | 0x26FA
        | 0x26FD
        | 0x2705
        | 0x270A..=0x270B
        | 0x2728
        | 0x274C
        | 0x274E
        | 0x2753..=0x2755
        | 0x2757
        | 0x2795..=0x2797
        | 0x27B0
        | 0x27BF
        | 0x2B1B..=0x2B1C
        | 0x2B50
        | 0x2B55)
}

/// CJK, Hangul, full-width forms, signs and dingbats (`☀`, `✋`) take a
/// whole em.
fn is_wide(code: u32) -> bool {
    matches!(code,
        0x1100..=0x115F
        | 0x2600..=0x27BF
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x3FFFD)
}

/// Real line breaks and the escape `\n`.
fn split_lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for part in text.split('\n') {
        let mut rest = part;
        while let Some(at) = find_newline_escape(rest) {
            out.push(&rest[..at]);
            rest = &rest[at + 2..];
        }
        out.push(rest);
    }
    out
}

/// Where `\n` starts in `text`, not counting `\\n` and `~\n`.
fn find_newline_escape(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'~' => i += 2,
            b'\\' if bytes[i + 1] == b'\\' => i += 2,
            b'\\' if bytes[i + 1] == b'n' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// A Creole table row: the first character that is not a space is `|`.
fn is_table_line(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// The rows of one table, each as wide as the whole grid.
fn table_rows(raw: &[&str], font: Font, family: Family) -> Result<Vec<Line>, String> {
    let mut grid = Vec::new();
    for row in raw {
        let mut cells = Vec::new();
        for cell in split_cells(row) {
            let (header, text) = if let Some(rest) = cell.strip_prefix('=') {
                (true, rest.trim())
            } else {
                (false, cell.trim())
            };
            let mut style = Style::from(font);
            if header {
                style.bold = true;
            }
            let lines = cell_lines(text, style, family)?;
            let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
            cells.push((lines, width));
        }
        if !cells.is_empty() {
            grid.push(cells);
        }
    }
    if grid.is_empty() {
        return Ok(Vec::new());
    }
    let cols = grid.iter().map(Vec::len).max().unwrap_or(0);
    let mut col_w = vec![TABLE_PAD * 2.0; cols];
    for row in &grid {
        for (i, (_, width)) in row.iter().enumerate() {
            col_w[i] = col_w[i].max(width + TABLE_PAD * 2.0);
        }
    }
    let table_w: f32 = col_w.iter().sum();
    let mut out = Vec::new();
    for (r, row) in grid.into_iter().enumerate() {
        let mut height = 0.0_f32;
        let mut cells = Vec::new();
        let mut x = 0.0;
        for (i, (lines, _)) in row.into_iter().enumerate() {
            height = height.max(lines.iter().map(|l| l.height).sum());
            cells.push(TableCell {
                left: x,
                width: col_w[i],
                lines,
            });
            x += col_w[i];
        }
        for &width in &col_w[cells.len()..] {
            cells.push(TableCell {
                left: x,
                width,
                lines: Vec::new(),
            });
            x += width;
        }
        if height == 0.0 {
            height = font.size * LINE;
        }
        out.push(Line {
            runs: Vec::new(),
            width: table_w,
            height,
            ascent: 0.0,
            flush: false,
            table: Some(TableRow {
                cells,
                color: font.color,
                top: r == 0,
            }),
        });
    }
    Ok(out)
}

/// The lines of one cell. `\n` breaks inside the cell, not into a new row.
fn cell_lines(text: &str, style: Style, family: Family) -> Result<Vec<Line>, String> {
    split_lines(text)
        .into_iter()
        .map(|piece| {
            check_other_markup(piece)?;
            Ok(line_of(inline(piece, style)?, style.size, family))
        })
        .collect()
}

/// Cells of one row. `\|` is a bar in the text. A cell of only spaces is
/// kept; `||` drops the empty token, as PlantUML's splitter does.
fn split_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut buf = String::new();
    let mut chars = line.trim().chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'|') {
            chars.next();
            buf.push('|');
            continue;
        }
        if c == '|' {
            if !buf.is_empty() {
                cells.push(std::mem::take(&mut buf));
            }
            continue;
        }
        buf.push(c);
    }
    if !buf.is_empty() {
        cells.push(buf);
    }
    cells
}

/// An item of a Creole list: how deep it stands (`*` is 1, `**` is 2),
/// whether its list is numbered (`#`), and its words. A line of other
/// marks, or of marks with no space after them, is none: `**bold**`
/// stays bold.
fn list_item(line: &str) -> Option<(usize, bool, &str)> {
    let text = line.trim_start();
    let mark = text.chars().next().filter(|c| matches!(c, '*' | '#'))?;
    let depth = text.chars().take_while(|c| *c == mark).count();
    let words = text[depth..].strip_prefix(' ')?;
    Some((depth, mark == '#', words.trim_start()))
}

/// Creole that works on whole lines: headings and rules. A table row and
/// an item of a list do not come through here.
fn check_block_markup(line: &str) -> Result<(), String> {
    check_other_markup(line)
}

fn check_other_markup(line: &str) -> Result<(), String> {
    let t = line.trim();
    let rule = |c: char| t.len() >= 4 && t.chars().all(|x| x == c);
    let what = if ['-', '=', '.', '_'].into_iter().any(rule) {
        Some("horizontal rules")
    } else if t.starts_with('=') && t.trim_start_matches('=').starts_with(' ') {
        Some("headings")
    } else if t.starts_with(['*', '#']) && t.trim_start_matches(['*', '#']).starts_with(' ') {
        Some("lists")
    } else {
        None
    };
    match what {
        Some(what) => Err(format!("Creole {what} (`{}`) are not supported yet", t)),
        None => Ok(()),
    }
}

/// Tags PlantUML knows that the engine does not draw.
const UNSUPPORTED_TAGS: [&str; 11] = [
    "img", "font", "back", "sub", "sup", "code", "latex", "math", "w", "wave", "plain",
];

/// The markers that switch a style on and off, in pairs.
const TOGGLES: [(&str, Toggle); 5] = [
    ("**", Toggle::Bold),
    ("//", Toggle::Italic),
    ("__", Toggle::Underline),
    ("--", Toggle::Strike),
    ("\"\"", Toggle::Mono),
];

/// One line with its inline markup, as pieces of text and their style.
fn inline(line: &str, base: Style) -> Result<Vec<Piece>, String> {
    let mut out: Vec<Piece> = Vec::new();
    let mut text = String::new();
    let mut style = base;
    // Open tags, to undo on their closing tag: the style before them.
    let mut tags: Vec<(String, Style)> = Vec::new();
    // Markers that are open: the value they replaced.
    let mut open: [Option<bool>; 5] = [None; 5];
    let mut i = 0;
    let flush = |out: &mut Vec<Piece>, text: &mut String, style: Style| {
        if !text.is_empty() {
            out.push(Piece {
                text: std::mem::take(text),
                style,
                link: None,
            });
        }
    };

    while i < line.len() {
        let rest = &line[i..];
        let c = rest.chars().next().unwrap_or(' ');
        if rest.starts_with("~~") && rest[2..].contains("~~") {
            return Err("wavy underlines are not supported yet".into());
        }
        if c == '~' {
            // The next character as written.
            match rest[1..].chars().next() {
                Some(n) => {
                    text.push(n);
                    i += 1 + n.len_utf8();
                }
                None => {
                    text.push('~');
                    i += 1;
                }
            }
            continue;
        }
        if rest.starts_with("\\t") {
            text.push('\t');
            i += 2;
            continue;
        }
        if rest.starts_with("\\\\") {
            text.push('\\');
            i += 2;
            continue;
        }
        if rest.starts_with("[[")
            && let Some(end) = rest[2..].find("]]")
        {
            let (link, label) = creole_link(&rest[2..2 + end])?;
            flush(&mut out, &mut text, style);
            let mut linked = style;
            linked.underline = true;
            out.push(Piece {
                text: label,
                style: linked,
                link: Some(link),
            });
            i += end + 4;
            continue;
        }
        if c == '<'
            && let Some(close) = rest.find('>')
            && let Some(change) = tag_change(&rest[1..close], style)?
        {
            flush(&mut out, &mut text, style);
            match change {
                Tag::Open(name, new) => {
                    tags.push((name, style));
                    style = new;
                }
                Tag::Close(name) => {
                    if let Some(at) = tags.iter().rposition(|(n, _)| *n == name) {
                        style = tags[at].1;
                        tags.truncate(at);
                    }
                }
            }
            i += close + 1;
            continue;
        }
        if let Some(k) = TOGGLES.iter().position(|(m, _)| rest.starts_with(m)) {
            let (marker, toggle) = TOGGLES[k];
            let url = toggle == Toggle::Italic && line[..i].ends_with(':');
            if let Some(before) = open[k] {
                flush(&mut out, &mut text, style);
                toggle.set(&mut style, before);
                open[k] = None;
                i += 2;
                continue;
            }
            if !url && rest[2..].contains(marker) {
                flush(&mut out, &mut text, style);
                open[k] = Some(toggle.get(&style));
                toggle.set(&mut style, true);
                i += 2;
                continue;
            }
        }
        text.push(c);
        i += c.len_utf8();
    }
    flush(&mut out, &mut text, style);
    Ok(out)
}

/// What is between `[[` and `]]`: an address, quoted if it holds spaces,
/// then an optional `{tooltip}` and an optional label, which is drawn
/// instead of the address.
pub fn creole_link(inner: &str) -> Result<(Link, String), String> {
    let inner = inner.trim();
    let (url, rest) = match inner.strip_prefix('"') {
        Some(quoted) => {
            let end = quoted
                .find('"')
                .ok_or_else(|| format!("the link `[[{inner}]]` has an open quote"))?;
            (&quoted[..end], &quoted[end + 1..])
        }
        None => inner.split_at(
            inner
                .find(|c: char| c.is_whitespace() || c == '{')
                .unwrap_or(inner.len()),
        ),
    };
    let url = url.trim();
    if url.is_empty() {
        return Err(format!("the link `[[{inner}]]` has no address"));
    }
    let mut rest = rest.trim_start();
    let mut tooltip = None;
    if let Some(open) = rest.strip_prefix('{') {
        let end = open
            .find('}')
            .ok_or_else(|| format!("the link `[[{inner}]]` has an open `{{`"))?;
        tooltip = Some(open[..end].trim())
            .filter(|t| !t.is_empty())
            .map(str::to_owned);
        rest = &open[end + 1..];
    }
    let label = match rest.trim() {
        "" => url,
        label => label,
    };
    Ok((
        Link {
            url: url.to_owned(),
            tooltip,
        },
        label.to_owned(),
    ))
}

enum Tag {
    Open(String, Style),
    Close(String),
}

/// What the tag `<tag>` does to `style`; `None` for text that only looks
/// like a tag.
fn tag_change(tag: &str, style: Style) -> Result<Option<Tag>, String> {
    let lower = tag.trim().to_ascii_lowercase();
    if let Some(name) = lower.strip_prefix('/') {
        let name = name.trim();
        return Ok(match name {
            "b" | "i" | "u" | "s" | "strike" | "del" | "color" | "size" => {
                Some(Tag::Close(canonical(name).into()))
            }
            _ => None,
        });
    }
    let (name, arg) = match lower.split_once([':', ' ']) {
        Some((name, arg)) => (name.trim(), Some(arg.trim())),
        None => (lower.as_str(), None),
    };
    let mut new = style;
    match (name, arg) {
        ("b", None) => new.bold = true,
        ("i", None) => new.italic = true,
        ("u", None) => new.underline = true,
        ("s" | "strike" | "del", None) => new.strike = true,
        ("color", Some(value)) => {
            // Keep the case of the value; color names ignore it anyway.
            let value = tag.split_once([':', ' ']).map_or(value, |(_, v)| v.trim());
            match color::parse(value)? {
                Some(c) => new.color = c,
                None => return Err("transparent text is not supported".into()),
            }
        }
        ("size", Some(value)) => {
            let size: f32 = value
                .parse()
                .map_err(|_| format!("`<size:{value}>` needs a number"))?;
            if !(1.0..=200.0).contains(&size) {
                return Err(format!("`<size:{value}>` is out of range"));
            }
            new.size = size;
        }
        _ => {
            let unsupported = UNSUPPORTED_TAGS.contains(&name)
                || lower.starts_with(['$', '&'])
                || lower.starts_with("u+");
            if unsupported {
                return Err(format!("`<{tag}>` is not supported yet"));
            }
            return Ok(None);
        }
    }
    Ok(Some(Tag::Open(canonical(name).into(), new)))
}

fn canonical(name: &str) -> &str {
    match name {
        "strike" | "del" => "s",
        other => other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Toggle {
    Bold,
    Italic,
    Underline,
    Strike,
    Mono,
}

impl Toggle {
    fn get(self, style: &Style) -> bool {
        match self {
            Toggle::Bold => style.bold,
            Toggle::Italic => style.italic,
            Toggle::Underline => style.underline,
            Toggle::Strike => style.strike,
            Toggle::Mono => style.mono,
        }
    }

    fn set(self, style: &mut Style, on: bool) {
        match self {
            Toggle::Bold => style.bold = on,
            Toggle::Italic => style.italic = on,
            Toggle::Underline => style.underline = on,
            Toggle::Strike => style.strike = on,
            Toggle::Mono => style.mono = on,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        Font::new(10.0, Color::BLACK)
    }

    fn pieces(text: &str) -> Vec<(String, &'static str)> {
        let b = block(text, font(), Family::Sans).unwrap();
        b.lines
            .iter()
            .enumerate()
            .flat_map(|(n, l)| {
                l.runs.iter().map(move |r| {
                    let s = &r.style;
                    let kind = match (s.bold, s.italic, s.underline, s.strike, s.mono) {
                        (true, _, _, _, _) => "b",
                        (_, true, _, _, _) => "i",
                        (_, _, true, _, _) => "u",
                        (_, _, _, true, _) => "s",
                        (_, _, _, _, true) => "m",
                        _ if s.color != Color::BLACK => "c",
                        _ => "",
                    };
                    (format!("{n}:{}", r.text), kind)
                })
            })
            .collect()
    }

    #[test]
    fn widths_follow_the_metrics() {
        let style = Style::from(font());
        // Helvetica: "Hi" = 722 + 222.
        assert!((measure("Hi", &style, Family::Sans) - 9.44).abs() < 0.01);
        let bold = Style::from(font().bold());
        assert!(measure("Hi", &bold, Family::Sans) > measure("Hi", &style, Family::Sans));
        assert_eq!(measure("iii", &style, Family::Mono), 18.0);
        assert_eq!(measure("日本", &style, Family::Sans), 20.0);
        assert_eq!(Family::of("Liberation Serif"), Family::Serif);
        assert_eq!(Family::of("sans-serif"), Family::Sans);
        assert_eq!(Family::of("JetBrains Mono"), Family::Mono);
        assert_eq!(Family::of("Fira Sans"), Family::Sans);
    }

    #[test]
    fn a_font_list_takes_the_family_of_its_first_telling_name() {
        // mmdr's list, with a mono fallback: Inter tells Sans first.
        let mmdr = "Inter, ui-sans-serif, \"Segoe UI\", \"Liberation Sans\", monospace";
        assert_eq!(Family::of(mmdr), Family::Sans);
        // A name that tells nothing passes on to the generic family.
        assert_eq!(Family::of("Georgia Pro, serif"), Family::Serif);
        assert_eq!(Family::of("'Fira Code', monospace"), Family::Mono);
        assert_eq!(Family::of("Lato, sans-serif"), Family::Sans);
        // The writer's first choice beats a fallback of another family.
        assert_eq!(
            Family::of("Times New Roman, Arial, sans-serif"),
            Family::Serif
        );
        assert_eq!(Family::of("Menlo, \"DejaVu Sans\""), Family::Mono);
        assert_eq!(Family::of("Unknown, Nameless"), Family::Sans);
    }

    #[test]
    fn lines_and_escapes() {
        let b = block("one\\ntwo\nthree", font(), Family::Sans).unwrap();
        assert_eq!(b.lines.len(), 3);
        assert!((b.height - 3.0 * 11.64).abs() < 0.01);
        assert_eq!(pieces("a ~**b~** c\\\\n"), [("0:a **b** c\\n".into(), "")]);
    }

    #[test]
    fn inline_markup() {
        assert_eq!(
            pieces("x <b>bold</b> **two** //it// <color:red>red</color> \"\"mono\"\""),
            [
                ("0:x ".to_string(), ""),
                ("0:bold".into(), "b"),
                ("0: ".into(), ""),
                ("0:two".into(), "b"),
                ("0: ".into(), ""),
                ("0:it".into(), "i"),
                ("0: ".into(), ""),
                ("0:red".into(), "c"),
                ("0: ".into(), ""),
                ("0:mono".into(), "m"),
            ]
        );
        // Single markers, URLs and tags PlantUML does not know stay text.
        assert_eq!(pieces("2**3"), [("0:2**3".into(), "")]);
        assert_eq!(
            pieces("see http://a.b and http://c.d"),
            [("0:see http://a.b and http://c.d".into(), "")]
        );
        assert_eq!(pieces("a <b> c</b>")[1].1, "b");
        assert_eq!(pieces("if a<b and c>d"), [("0:if a<b and c>d".into(), "")]);
        let sized = block("<size:20>big</size>", font(), Family::Sans).unwrap();
        assert!((sized.height - 20.0 * LINE).abs() < 0.01);
    }

    #[test]
    fn a_list_has_bullets_or_numbers_and_stands_at_the_left() {
        let said = block(
            "**Title**\n* one\n** deeper\n# first\n# second\nplain\n# again",
            font(),
            Family::Sans,
        )
        .unwrap();
        let lines: Vec<(String, bool)> = said
            .lines
            .iter()
            .map(|line| {
                let text: String = line.runs.iter().map(|run| run.text.as_str()).collect();
                (text, line.flush)
            })
            .collect();
        assert_eq!(
            lines,
            [
                ("Title".to_owned(), false),
                ("\u{2022}  one".to_owned(), true),
                ("    \u{2022}  deeper".to_owned(), true),
                ("1. first".to_owned(), true),
                ("2. second".to_owned(), true),
                ("plain".to_owned(), false),
                ("1. again".to_owned(), true),
            ]
        );
        assert!(said.lines[0].runs[0].style.bold);
        // Marks without a space are no list.
        let bold = block("**all bold**", font(), Family::Sans).unwrap();
        assert!(!bold.lines[0].flush && bold.lines[0].runs[0].style.bold);
    }

    #[test]
    fn an_emoji_is_as_wide_as_it_is_drawn() {
        let style = Style::from(font());
        let wide = |text: &str| measure(text, &style, Family::Sans) / style.size;
        for picture in ["\u{1F916}", "\u{2B50}", "\u{270B}", "\u{2705}"] {
            assert!((wide(picture) - 1.25).abs() < 0.001, "{picture}");
        }
        // With the mark that asks for its colors, no wider.
        assert!((wide("\u{1F6E1}\u{FE0F}") - 1.25).abs() < 0.001);
        // A sign a text face draws keeps an em.
        assert!((wide("\u{2713}") - 1.0).abs() < 0.001);
    }

    #[test]
    fn declines_what_it_cannot_draw() {
        for (text, part) in [
            ("<img:x.png>", "<img:x.png>"),
            ("<$sprite>", "<$sprite>"),
            ("<&key>", "<&key>"),
            ("[[{tip} label]]", "no address"),
            ("[[\"a b label]]", "open quote"),
            ("----", "rules"),
            ("<color:#red/blue>x", "gradients"),
            ("<back:red>x", "<back:red>"),
            ("<U+1F600>", "<U+1F600>"),
        ] {
            let err = block(text, font(), Family::Sans).unwrap_err();
            assert!(err.contains(part), "{text}: {err}");
        }
    }

    #[test]
    fn links_are_underlined_and_keep_their_address() {
        let links = |text: &str| -> Vec<(String, Option<Link>)> {
            block(text, font(), Family::Sans).unwrap().lines[0]
                .runs
                .iter()
                .map(|r| (r.text.clone(), r.link.clone()))
                .collect()
        };
        let link = |url: &str, tooltip: Option<&str>| {
            Some(Link {
                url: url.into(),
                tooltip: tooltip.map(Into::into),
            })
        };
        assert_eq!(
            links("see [[https://a.b]] or [[docs/x.html the guide]]"),
            [
                ("see ".into(), None),
                ("https://a.b".into(), link("https://a.b", None)),
                (" or ".into(), None),
                ("the guide".into(), link("docs/x.html", None)),
            ]
        );
        assert_eq!(
            links("[[https://a.b{Read me} more]]"),
            [("more".into(), link("https://a.b", Some("Read me")))]
        );
        assert_eq!(
            links("[[\"x y.html\"{}]]"),
            [("x y.html".into(), link("x y.html", None))]
        );
        assert_eq!(pieces("<b>[[a b]]</b>"), [("0:b".into(), "b")]);
        let run = &block("[[a b]]", font(), Family::Sans).unwrap().lines[0].runs[0];
        assert!(run.style.underline);
        // Without its end it is text.
        assert_eq!(pieces("a [[b"), [("0:a [[b".into(), "")]);
    }

    #[test]
    fn a_creole_table_is_a_grid_of_cells() {
        let b = block(
            "|= a |= b |\n| 1 | 2 |\n| a \\| b | c |",
            font(),
            Family::Sans,
        )
        .unwrap();
        assert_eq!(b.lines.len(), 3);
        assert!(b.lines.iter().all(|l| l.table.is_some()));
        assert!((b.lines[0].width - b.lines[1].width).abs() < 0.01);
        let words = |row: usize| -> Vec<String> {
            b.lines[row]
                .table
                .as_ref()
                .unwrap()
                .cells
                .iter()
                .map(|cell| {
                    cell.lines
                        .iter()
                        .flat_map(|l| l.runs.iter().map(|r| r.text.clone()))
                        .collect()
                })
                .collect()
        };
        assert_eq!(words(0), ["a", "b"]);
        assert_eq!(words(1), ["1", "2"]);
        assert_eq!(words(2), ["a | b", "c"]);
        assert!(b.lines[0].table.as_ref().unwrap().top);
        assert!(!b.lines[1].table.as_ref().unwrap().top);
        let header = &b.lines[0].table.as_ref().unwrap().cells[0].lines[0].runs[0];
        assert!(header.style.bold);
        let body = &b.lines[1].table.as_ref().unwrap().cells[0].lines[0].runs[0];
        assert!(!body.style.bold);

        let mut canvas = crate::svg::Canvas::new("sans-serif", false);
        canvas.text(
            crate::svg::Layer::Front,
            &b,
            (0.0, 0.0),
            crate::svg::Align::Left,
        );
        let svg = canvas.finish(0.0, None);
        assert!(svg.contains("font-weight=\"bold\""), "{svg}");
        assert!(svg.contains(">a</tspan>"), "{svg}");
        assert!(svg.matches("<line ").count() >= 6, "{svg}");
    }
}
