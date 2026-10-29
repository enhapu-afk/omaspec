//! Text: the part of PlantUML's Creole markup the engine draws, and widths
//! from built-in font metrics, so no font file is read.
//!
//! Markup it draws: `\n`, `<b>`, `<i>`, `<u>`, `<s>`, `<color:…>`,
//! `<size:…>`, `**bold**`, `//italic//`, `__underline__`, `--strike--`,
//! `""mono""`, `~` to escape, and Creole tables (`|= a |= b |` over
//! `| 1 | 2 |`). Markup it knows but does not draw (images, links, lists,
//! headings, rules, …) is an error naming it; anything else that looks
//! like a tag is shown as written, as PlantUML does.

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
    for line in text.split('\n') {
        if is_table_line(line) {
            rows.push(line);
        } else {
            flush(&mut lines, &mut rows)?;
            for piece in split_lines(line) {
                check_block_markup(piece)?;
                lines.push(line_of(
                    inline(piece, Style::from(font))?,
                    font.size,
                    family,
                ));
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
            .map(|pieces| line_of(pieces, font.size, family))
            .collect(),
    )
}

fn line_of(pieces: Vec<(String, Style)>, size: f32, family: Family) -> Line {
    let mut runs = Vec::new();
    let mut width: f32 = 0.0;
    let mut size = size;
    for (text, style) in pieces {
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
        });
    }
    Line {
        runs,
        width,
        height: size * LINE,
        ascent: size * ASCENT,
        table: None,
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
    if matches!(code, 0x0300..=0x036F | 0x200B..=0x200F | 0xFEFF) || c.is_control() {
        return 0;
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

/// CJK, Hangul, full-width forms and emoji take a whole em.
fn is_wide(code: u32) -> bool {
    matches!(code,
        0x1100..=0x115F
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
        | 0x1F300..=0x1FAFF
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

/// Creole that works on whole lines: lists, headings, rules. A table row
/// does not come through here.
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
fn inline(line: &str, base: Style) -> Result<Vec<(String, Style)>, String> {
    let mut out: Vec<(String, Style)> = Vec::new();
    let mut text = String::new();
    let mut style = base;
    // Open tags, to undo on their closing tag: the style before them.
    let mut tags: Vec<(String, Style)> = Vec::new();
    // Markers that are open: the value they replaced.
    let mut open: [Option<bool>; 5] = [None; 5];
    let mut i = 0;
    let flush = |out: &mut Vec<(String, Style)>, text: &mut String, style: Style| {
        if !text.is_empty() {
            out.push((std::mem::take(text), style));
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
        if rest.starts_with("[[") {
            return Err("links (`[[…]]`) are not supported yet".into());
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
    fn declines_what_it_cannot_draw() {
        for (text, part) in [
            ("<img:x.png>", "<img:x.png>"),
            ("<$sprite>", "<$sprite>"),
            ("<&key>", "<&key>"),
            ("[[http://x]]", "links"),
            ("* item", "lists"),
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
