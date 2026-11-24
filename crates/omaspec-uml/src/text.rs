//! Text: the part of PlantUML's Creole markup the engine draws, and widths
//! from built-in font metrics, so no font file is read.
//!
//! Markup it draws: `\n`, `<b>`, `<i>`, `<u>`, `<s>`, `<color:…>`,
//! `<size:…>`, `<font color=… size=…>`, `<font:face>`, `<back:…>`, lines
//! in a color of their own (`<u:red>`, `<s:red>`), wavy underlines
//! (`~~waved~~`, `<w>`, `<w:red>`), `<U+221E>`, `**bold**`, `//italic//`,
//! `__underline__`, `--strike--`, `""mono""`, `~` to escape, headings
//! (`= Big`, `== Smaller`, `===`, `====`), rules across the text's box
//! (`----`, `====`, `____`, `....`, with words in their middle as
//! `--title--`), lists, Creole tables (`|= a |= b |` over `| 1 | 2 |`),
//! links (`[[url]]`, `[[url label]]`, `[[url{tooltip} label]]`),
//! underlined in the text's color, and icons (`<&heart>`, `<$sprite>`,
//! [`crate::icons`]), and pictures (`<img:logo.png>`,
//! [`crate::icons::raster`]). Markup it knows but does not draw (`<sub>`,
//! `<sup>`, `<code>`, `<math>`, …) is an error naming it; anything else
//! that looks like a tag is shown as written, as PlantUML does. A value of
//! data and Mermaid's words read less of it ([`as_data`], [`as_mermaid`]).

use crate::color::{self, Color};
pub use crate::icons::{Icon, Picture};
use crate::metrics;

mod emoji;
pub use emoji::emoji;

/// The font family of the whole diagram, by its metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Inter, the face shipped with the program ([`crate::FACE`]), which a
    /// diagram is set in unless it names another.
    Inter,
    /// Any other sans-serif, as wide as Arial.
    Sans,
    Serif,
    Mono,
}

impl Family {
    /// The family whose metrics fit the font `name` best. `name` may be a
    /// CSS font list (`Inter, "Segoe UI", sans-serif`): the first name that
    /// tells a family decides, as the first one installed draws, and a name
    /// that tells none passes on to the next. Sans when none does: a face
    /// nobody knows is drawn by the viewer's own sans-serif.
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
        if n == "inter" {
            Some(Family::Inter)
        } else if n.contains("mono") || n.contains("courier") || n.contains("consol") {
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

/// A face words are set in apart from the diagram's: the family whose
/// metrics measure them, and, where it has a name of its own, its place
/// in the picture's list of faces ([`crate::svg::Canvas::faces`]), which
/// writes the name for those words alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    pub family: Family,
    pub name: Option<u8>,
}

impl Face {
    /// The face `name` (`Impact`, `Courier`), the `at`-th of the
    /// picture's list.
    pub fn named(name: &str, at: u8) -> Face {
        Face {
            family: Family::of(name),
            name: Some(at),
        }
    }
}

/// The names of the faces a skin gives its elements (`ParticipantFontName
/// Impact`), each once, in the order given: a [`Face`] holds its place
/// here, which the picture's list of faces takes
/// ([`crate::svg::Canvas::faces`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Faces(pub Vec<String>);

impl Faces {
    /// The face `name`, taken into the list if it is new. `None` for a
    /// name no SVG can carry, or one past the 256 the list holds.
    pub fn face(&mut self, name: &str) -> Option<Face> {
        let name = name.trim().trim_matches('"').trim();
        if name.is_empty() || name.contains(['<', '>', '"', '&']) {
            return None;
        }
        let at = match self.0.iter().position(|n| n == name) {
            Some(at) => at,
            None => {
                self.0.push(name.to_owned());
                self.0.len() - 1
            }
        };
        u8::try_from(at).ok().map(|at| Face::named(name, at))
    }
}

/// How an element's text starts out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Font {
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub color: Color,
    /// A face of its own (`ParticipantFontName`); the diagram's for
    /// `None`.
    pub face: Option<Face>,
}

impl Font {
    pub fn new(size: f32, color: Color) -> Font {
        Font {
            size,
            bold: false,
            italic: false,
            color,
            face: None,
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
    /// A wavy line under the words (`~~waved~~`, `<w>`).
    pub wave: bool,
    /// The color of the lines under and through the words where they
    /// have one of their own (`<u:red>`, `<s:green>`, `<w:#f0f>`).
    pub lines: Option<Color>,
    /// The color behind the words (`<back:yellow>`).
    pub back: Option<Color>,
    /// The face the words are set in where it is not the diagram's: an
    /// element's own (`ParticipantFontName`), or one they name
    /// (`<font:courier>`), by the family whose metrics fit it.
    pub face: Option<Face>,
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
            wave: false,
            lines: None,
            back: None,
            face: font.face,
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
    /// An icon in place of words (`<&heart>`); its text is empty.
    pub icon: Option<Icon>,
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
    icon: Option<Icon>,
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
    /// How many lines of the source it stands for: one, or those of a
    /// diagram inside the text with its `{{` and `}}`.
    pub rows: usize,
    /// A rule across the box the text is written in, in this color; its
    /// runs, if any, are the words in its middle (`--title--`).
    pub rule: Option<(Rule, Color)>,
}

/// A Creole rule: a line across the box the text is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// `----`
    Single,
    /// `====`
    Double,
    /// `____`
    Strong,
    /// `....`
    Dotted,
}

impl Rule {
    /// How tall a rule without words is.
    pub fn height(self) -> f32 {
        match self {
            Rule::Single | Rule::Dotted => 4.0,
            Rule::Double | Rule::Strong => 8.0,
        }
    }
}

/// Lines of text, measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub lines: Vec<Line>,
    pub width: f32,
    pub height: f32,
    /// Where one line of the source alone writes these words, where the
    /// drawer says so ([`Block::written_on`], [`Block::written_at`]): an
    /// action, a label, a note. Drawn, they are tied to it
    /// ([`crate::svg::Canvas::tie`]). None for a name many lines mention.
    pub written: Option<Written>,
}

/// Where one line of the source alone writes words that are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Written {
    /// The zero-based line.
    pub line: usize,
    /// The byte on it where the words start, where the drawer knows it:
    /// words written amid others (a line's label between its ends, a
    /// multiplicity). Words without it close their line, as a message
    /// does after its colon: they are the last such words there.
    pub col: Option<usize>,
}

impl Block {
    pub fn empty() -> Block {
        Block {
            lines: Vec::new(),
            width: 0.0,
            height: 0.0,
            written: None,
        }
    }

    /// These words as the ones the zero-based `line` alone writes, the
    /// last such words on it.
    pub fn written_on(mut self, line: usize) -> Block {
        self.written = Some(Written { line, col: None });
        self
    }

    /// These words as the ones the zero-based `line` alone writes, from
    /// its byte `col` on, where that is known.
    pub fn written_at(mut self, line: usize, col: Option<usize>) -> Block {
        self.written = Some(Written { line, col });
        self
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Line height and ascent as shares of the font size (Java's metrics for
/// the logical sans-serif font, which PlantUML's spacing is made for).
const LINE: f32 = 1.164;
const ASCENT: f32 = 0.928;

/// `words` broken into lines no wider than `width`, at its spaces. A
/// word wider than that stands on a line of its own; a link (`[[…]]`) is
/// one word. Markup open where a line breaks (`**bold words**`, `<b>`,
/// `<color:red>`) is closed at the end of that line and opened again at
/// the start of the next, so each line reads as its words do.
pub(crate) fn wrapped(
    words: &str,
    font: Font,
    family: Family,
    width: f32,
) -> Result<Block, String> {
    let mut lines: Vec<String> = Vec::new();
    for paragraph in words.split('\n') {
        // What is open where the line at hand starts, and its words.
        let mut carried: Vec<String> = Vec::new();
        let mut line = String::new();
        let words = wrap_words(paragraph);
        for (at, word) in words.iter().enumerate() {
            let longer = if line.is_empty() {
                word.clone()
            } else {
                format!("{line} {word}")
            };
            let past = words[at + 1..].join(" ");
            if !line.is_empty()
                && block(&with_markup(&carried, &longer, &past), font, family)?.width > width
            {
                let after = words[at..].join(" ");
                lines.push(with_markup(&carried, &line, &after));
                carried = open_after(carried, &line, &after);
                line = word.clone();
            } else {
                line = longer;
            }
        }
        lines.push(with_markup(&carried, &line, ""));
    }
    block(&lines.join("\n"), font, family)
}

/// The words of `text` at its spaces, a link (`[[…]]`) held as one.
fn wrap_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut link = false;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if !link && rest.starts_with("[[") {
            link = true;
        } else if link && rest.starts_with("]]") {
            link = false;
            word.push_str("]]");
            rest = &rest[2..];
            continue;
        }
        if c.is_whitespace() && !link {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
        rest = &rest[c.len_utf8()..];
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// `line` with the markup `carried` from the line before opened in front
/// of it, and what is open at its end closed after it; `after` is what
/// follows it in its paragraph.
fn with_markup(carried: &[String], line: &str, after: &str) -> String {
    let mut out: String = carried.concat();
    out.push_str(line);
    for open in open_after(carried.to_vec(), line, after).iter().rev() {
        match open.strip_prefix('<') {
            Some(tag) => {
                let name: String = tag
                    .chars()
                    .take_while(|c| c.is_ascii_alphabetic())
                    .collect();
                out.push_str(&format!("</{name}>"));
            }
            None => out.push_str(open),
        }
    }
    out
}

/// The markup open after `text`, with `open` open before it: the
/// toggles (`**`, `//`, …) and the tags the text draws (`<b>`,
/// `<color:red>`), in the order they opened. A link's insides and an
/// escaped mark (`~**`) open nothing.
fn open_after(mut open: Vec<String>, text: &str, after: &str) -> Vec<String> {
    const TAGS: [&str; 10] = [
        "b", "i", "u", "s", "strike", "color", "size", "font", "back", "w",
    ];
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        // `~~waved~~` opens and closes as a toggle does, where a `~~`
        // closes it later in its paragraph.
        if rest.starts_with("~~") && READING.get() != Reading::Mermaid {
            let at = open.iter().rposition(|o| o == "~~");
            if let Some(at) = at {
                open.remove(at);
                rest = &rest[2..];
                continue;
            }
            if rest[2..].contains("~~") || after.contains("~~") {
                open.push("~~".to_owned());
                rest = &rest[2..];
                continue;
            }
        }
        if c == '~' {
            rest = &rest[1..];
            if let Some(next) = rest.chars().next() {
                rest = &rest[next.len_utf8()..];
            }
            continue;
        }
        if rest.starts_with("[[") {
            rest = rest.find("]]").map_or("", |end| &rest[end + 2..]);
            continue;
        }
        if let Some((mark, _)) = TOGGLES.iter().find(|(mark, _)| rest.starts_with(mark)) {
            match open.iter().rposition(|o| o == mark) {
                Some(at) => {
                    open.remove(at);
                }
                None => open.push((*mark).to_owned()),
            }
            rest = &rest[mark.len()..];
            continue;
        }
        if c == '<'
            && let Some(end) = rest.find('>')
        {
            let tag = &rest[1..end];
            let closing = tag.strip_prefix('/');
            let name: String = closing
                .unwrap_or(tag)
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect::<String>()
                .to_ascii_lowercase();
            if TAGS.contains(&name.as_str()) {
                match closing {
                    Some(_) => {
                        let opened = |o: &String| {
                            o.strip_prefix('<').is_some_and(|t| {
                                let opened: String =
                                    t.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
                                opened.eq_ignore_ascii_case(&name)
                            })
                        };
                        if let Some(at) = open.iter().rposition(opened) {
                            open.remove(at);
                        }
                    }
                    None => open.push(rest[..=end].to_owned()),
                }
            }
            rest = &rest[end + 1..];
            continue;
        }
        rest = &rest[c.len_utf8()..];
    }
    open
}

/// `text` in `font`, measured. `\n` and real line breaks start new lines.
/// Consecutive lines that start with `|` are one Creole table. A diagram
/// between a line `{{` and a line `}}` is a picture on a line of its own
/// ([`crate::embedded`]), where the engine draws it.
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
    // Two or three marks are a rule only among other lines, as PlantUML
    // reads them: a message of `--` is words.
    let several = text.contains('\n') || find_newline_escape(text).is_some();
    let all: Vec<&str> = text.split('\n').collect();
    let mut skip = 0;
    for (at, &line) in all.iter().enumerate() {
        if skip > 0 {
            skip -= 1;
            continue;
        }
        if let Some(kind) = crate::embedded::opens(line)
            && let Some(end) = crate::embedded::closes(&all[at + 1..])
            && let Some(diagram) = crate::embedded::picture(kind, &all[at + 1..at + 1 + end])
        {
            flush(&mut lines, &mut rows)?;
            counts.clear();
            let icon = Icon {
                width: diagram.width,
                height: diagram.height,
                color: None,
                picture: Picture::Diagram(diagram),
            };
            let mut picture = line_of(
                vec![Piece {
                    text: String::new(),
                    style: Style::from(font),
                    link: None,
                    icon: Some(icon),
                }],
                font.size,
                family,
            );
            // The lines of the diagram, and its `{{` and `}}`.
            picture.rows = end + 2;
            lines.push(picture);
            skip = end + 1;
            continue;
        }
        if is_table_line(line) {
            rows.push(line);
        } else {
            flush(&mut lines, &mut rows)?;
            let reading = READING.get();
            let creole = reading == Reading::PlantUml;
            for piece in split_lines(line) {
                if let Some((rule, title)) = rule(piece, several).filter(|_| creole) {
                    counts.clear();
                    lines.push(ruled(rule, title, font, family)?);
                    continue;
                }
                if let Some((level, words)) = heading(piece).filter(|_| creole) {
                    counts.clear();
                    lines.push(line_of(
                        inline(words, heading_style(level, font))?,
                        font.size,
                        family,
                    ));
                    continue;
                }
                let item = list_item(piece).filter(|_| reading != Reading::Data);
                let Some((depth, numbered, words)) = item else {
                    counts.clear();
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
                    icon: None,
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

/// How the words drawn are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    /// As PlantUML reads a diagram's words: all of Creole.
    PlantUml,
    /// As PlantUML reads a value of data ([`as_data`]).
    Data,
    /// As Mermaid writes them ([`as_mermaid`]).
    Mermaid,
}

thread_local! {
    static READING: std::cell::Cell<Reading> = const { std::cell::Cell::new(Reading::PlantUml) };
}

/// Runs `draw` with the words read as `reading` says, and as before after.
fn read_as<T>(reading: Reading, draw: impl FnOnce() -> T) -> T {
    struct Restore(Reading);
    impl Drop for Restore {
        fn drop(&mut self) {
            READING.set(self.0);
        }
    }
    let _restore = Restore(READING.replace(reading));
    draw()
}

/// Runs `draw` with words read as Mermaid writes them. Creole's rules and
/// headings are PlantUML's alone: a line of `----` or `= Title` is drawn
/// as it stands. The markup only PlantUML draws (`~~waved~~`, `<font>`,
/// `<back>`, `<w>`, `<U+…>`, `<img>`) declines, so mmdr draws those words;
/// the rest of a line's markup the engine reads as it does for PlantUML.
pub fn as_mermaid<T>(draw: impl FnOnce() -> T) -> T {
    read_as(Reading::Mermaid, draw)
}

/// Runs `draw` with words read as PlantUML reads a value of data
/// (`@startjson`): a table is drawn, a line of `----`, `= Title` or
/// `* item` is written as it stands.
pub fn as_data<T>(draw: impl FnOnce() -> T) -> T {
    read_as(Reading::Data, draw)
}

/// A picture of a diagram, `diagram`, as a block of one line, standing on
/// its baseline as a diagram inside text does.
pub(crate) fn picture_block(
    diagram: std::sync::Arc<crate::embedded::Embedded>,
    font: Font,
    family: Family,
) -> Block {
    let icon = Icon {
        width: diagram.width,
        height: diagram.height,
        color: None,
        picture: Picture::Diagram(diagram),
    };
    let piece = Piece {
        text: String::new(),
        style: Style::from(font),
        link: None,
        icon: Some(icon),
    };
    stack(vec![line_of(vec![piece], font.size, family)])
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
                        icon: None,
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
    // The tallest icon stands on the baseline and lifts the line's top.
    let mut tall: f32 = 0.0;
    for Piece {
        text,
        style,
        link,
        icon,
    } in pieces
    {
        if text.is_empty() && icon.is_none() {
            continue;
        }
        let w = match &icon {
            Some(icon) => {
                tall = tall.max(icon.height);
                icon.width
            }
            None => measure(&text, &style, family),
        };
        width += w;
        size = size.max(style.size);
        runs.push(Run {
            text,
            style,
            width: w,
            link,
            icon,
        });
    }
    let ascent = (size * ASCENT).max(tall);
    Line {
        runs,
        width,
        height: ascent + size * (LINE - ASCENT),
        ascent,
        table: None,
        flush: false,
        rows: 1,
        rule: None,
    }
}

fn stack(lines: Vec<Line>) -> Block {
    let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    let height = lines.iter().map(|l| l.height).sum();
    Block {
        lines,
        width,
        height,
        written: None,
    }
}

/// The width of `text` in `style`. An emoji is one picture of the emoji
/// face however many characters write it ([`emoji`]).
pub fn measure(text: &str, style: &Style, family: Family) -> f32 {
    let family = if style.mono {
        Family::Mono
    } else {
        style.face.map_or(family, |face| face.family)
    };
    let letters =
        |text: &str| -> u32 { text.chars().map(|c| advance(c, family, style.bold)).sum() };
    let mut em = 0;
    let mut at = 0;
    for picture in emoji(text) {
        em += letters(&text[at..picture.start]) + PICTURE;
        at = picture.end;
    }
    em += letters(&text[at..]);
    em as f32 / 1000.0 * style.size
}

/// How wide a picture of the emoji face is, in thousandths of an em: wider
/// than its em, so a word after it does not stand on it.
const PICTURE: u32 = 1250;

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
    if emoji::is_picture(c) {
        return PICTURE;
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
        (Family::Inter, false) => &metrics::INTER,
        (Family::Inter, true) => &metrics::INTER_BOLD,
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
        None if family == Family::Inter => 580,
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
            rows: 1,
            rule: None,
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

/// A Creole rule, and the words in its middle (empty for none): a line of
/// two or more of one mark (`--`, `====`, `____`, `....`), or words between
/// two of them on each side (`--title--`, `== Title ==`), as PlantUML
/// reads them.
fn rule(line: &str, several: bool) -> Option<(Rule, &str)> {
    let t = line.trim();
    let mark = *t.as_bytes().first()?;
    let rule = match mark {
        b'-' => Rule::Single,
        b'=' => Rule::Double,
        b'_' => Rule::Strong,
        b'.' => Rule::Dotted,
        _ => return None,
    };
    if t.bytes().all(|c| c == mark) {
        return (t.len() >= 4 || (several && t.len() >= 2)).then_some((rule, ""));
    }
    // The marks are one byte each, so the words between them start and
    // end on a character's edge.
    let two = [mark, mark];
    let b = t.as_bytes();
    if b.len() > 4 && b.starts_with(&two) && b.ends_with(&two) {
        let title = t[2..t.len() - 2].trim();
        if !title.is_empty() {
            return Some((rule, title));
        }
    }
    None
}

/// A Creole heading: how deep (`=` is 1, `====` is 4) and its words. A
/// line of `=` alone is a rule ([`rule`]).
fn heading(line: &str) -> Option<(usize, &str)> {
    let t = line.trim_start();
    let level = t.chars().take_while(|c| *c == '=').count();
    let words = t[level..].trim();
    (level > 0 && !words.is_empty()).then_some((level.min(4), words))
}

/// How a heading of `level` is set in `font`: larger and bold, the fourth
/// in italics, as PlantUML sets them.
fn heading_style(level: usize, font: Font) -> Style {
    let mut style = Style::from(font);
    match level {
        1 => style.size += 4.0,
        2 => style.size += 2.0,
        3 => style.size += 1.0,
        _ => {}
    }
    if level < 4 {
        style.bold = true;
    } else {
        style.italic = true;
    }
    style
}

/// A rule's line, its words in its middle in `font`, with room for a
/// stretch of the rule on either side of them.
fn ruled(rule: Rule, title: &str, font: Font, family: Family) -> Result<Line, String> {
    let mut line = if title.is_empty() {
        let mut line = line_of(Vec::new(), font.size, family);
        line.height = rule.height();
        line.ascent = 0.0;
        // As long as PlantUML draws one alone, where no box gives it more.
        line.width = RULE_LEAST;
        line
    } else {
        let mut line = line_of(inline(title, Style::from(font))?, font.size, family);
        line.width += 2.0 * RULE_REACH;
        line.height += 4.0;
        line.ascent += 2.0;
        line
    };
    line.rule = Some((rule, font.color));
    Ok(line)
}

/// How far a rule reaches at least on either side of its words.
pub const RULE_REACH: f32 = 12.0;
/// How long a rule without words is at least.
pub const RULE_LEAST: f32 = 24.0;

/// Creole that works on whole lines, which a cell of a table does not
/// take.
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
const UNSUPPORTED_TAGS: [&str; 6] = ["sub", "sup", "code", "latex", "math", "plain"];

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
    // A wavy underline open with `~~`: the style before it.
    let mut waved: Option<bool> = None;
    let mut i = 0;
    let flush = |out: &mut Vec<Piece>, text: &mut String, style: Style| {
        if !text.is_empty() {
            out.push(Piece {
                text: std::mem::take(text),
                style,
                link: None,
                icon: None,
            });
        }
    };

    while i < line.len() {
        let rest = &line[i..];
        let c = rest.chars().next().unwrap_or(' ');
        // `~~waved~~`; a `~~` that closes nothing is an escaped `~`.
        let mermaid = READING.get() == Reading::Mermaid;
        if rest.starts_with("~~") && (waved.is_some() || rest[2..].contains("~~")) {
            if mermaid {
                return Err("wavy underlines are not supported yet".into());
            }
            flush(&mut out, &mut text, style);
            match waved.take() {
                Some(before) => style.wave = before,
                None => {
                    waved = Some(style.wave);
                    style.wave = true;
                }
            }
            i += 2;
            continue;
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
                icon: None,
            });
            i += end + 4;
            continue;
        }
        // A character by its code (`<U+221E>`).
        if c == '<'
            && let Some(close) = rest.find('>')
            && let Some(code) = unicode(&rest[1..close])
        {
            if mermaid {
                return Err(format!("`{}` is not supported yet", &rest[..=close]));
            }
            text.push(code);
            i += close + 1;
            continue;
        }
        // A picture file (`<img:logo.png>`), or what says it cannot be
        // drawn in its place.
        if c == '<'
            && let Some(close) = rest.find('>')
            && let Some((path, scale)) = crate::icons::raster::tag(&rest[1..close])
        {
            if mermaid {
                return Err(format!("`{}` is not supported yet", &rest[..=close]));
            }
            flush(&mut out, &mut text, style);
            match crate::icons::raster::find(path) {
                Ok(raster) => out.push(Piece {
                    text: String::new(),
                    style,
                    link: None,
                    icon: Some(Icon {
                        width: raster.width as f32 * scale,
                        height: raster.height as f32 * scale,
                        color: None,
                        picture: Picture::Raster(raster),
                    }),
                }),
                Err(unread) => {
                    let mut said = style;
                    said.mono = true;
                    let why = match unread {
                        crate::icons::raster::Unread::Missing => "cannot read",
                        crate::icons::raster::Unread::Kind => "cannot draw",
                    };
                    out.push(Piece {
                        text: format!("({why} {path})"),
                        style: said,
                        link: None,
                        icon: None,
                    });
                }
            }
            i += close + 1;
            continue;
        }
        // An icon (`<&heart>`), drawn in the text's color.
        if c == '<'
            && let Some(close) = rest.find('>')
            && let Some(icon) = crate::icons::tag(&rest[1..close], style.size)
        {
            flush(&mut out, &mut text, style);
            // A name Open Iconic does not have is drawn as nothing.
            if let Some(icon) = icon? {
                out.push(Piece {
                    text: String::new(),
                    style,
                    link: None,
                    icon: Some(icon),
                });
            }
            i += close + 1;
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
                    // It closes the tags opened inside it too.
                    if let Some(at) = tags.iter().rposition(|(n, _)| *n == name) {
                        for (name, before) in tags.drain(at..).rev() {
                            style = undo(&name, before, style);
                        }
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
            // A pair with nothing between (`----`) marks nothing.
            if !url && rest[2..].contains(marker) && !rest[2..].starts_with(marker) {
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
            "b" | "i" | "u" | "s" | "strike" | "del" | "color" | "size" | "font" | "back" | "w" => {
                Some(Tag::Close(canonical(name).into()))
            }
            _ => None,
        });
    }
    let (name, arg) = match lower.split_once([':', ' ', '=']) {
        Some((name, arg)) => (name.trim(), Some(arg.trim())),
        None => (lower.as_str(), None),
    };
    // What only PlantUML draws, Mermaid's words leave to mmdr; a line in a
    // color of its own they show as written.
    if READING.get() == Reading::Mermaid {
        match (name, arg) {
            ("font" | "back" | "w" | "wave", _) => {
                return Err(format!("`<{tag}>` is not supported yet"));
            }
            ("u" | "s" | "strike" | "del", Some(_)) => return Ok(None),
            _ => {}
        }
    }
    // The value as written: color names ignore case, a face's name keeps it.
    let value = || {
        tag.trim()
            .split_once([':', ' ', '='])
            .map_or("", |(_, v)| v.trim())
    };
    let mut new = style;
    match (name, arg) {
        ("b", None) => new.bold = true,
        ("i", None) => new.italic = true,
        ("u", None) => new.underline = true,
        ("s" | "strike" | "del", None) => new.strike = true,
        ("w", None) => new.wave = true,
        ("u" | "s" | "strike" | "del" | "w", Some(_)) => {
            match name {
                "u" => new.underline = true,
                "w" => new.wave = true,
                _ => new.strike = true,
            }
            new.lines = Some(opaque(value(), tag)?);
        }
        ("color", Some(_)) => new.color = opaque(value(), tag)?,
        ("back", Some(_)) => new.back = Some(opaque(value(), tag)?),
        ("size", Some(value)) => new.size = font_size(value, tag)?,
        // `<font:monospaced>`: a face.
        ("font", Some(_)) if tag.trim()[4..].trim_start().starts_with(':') => {
            new.face = Some(Face {
                family: Family::of(value()),
                name: None,
            })
        }
        // `<font color=red size=20>`, a value quoted or not.
        ("font", Some(_)) => {
            for (key, value) in attributes(tag.trim()[4..].trim()) {
                match key.to_ascii_lowercase().as_str() {
                    "color" => new.color = opaque(&value, tag)?,
                    "size" => new.size = font_size(&value, tag)?,
                    "face" => {
                        new.face = Some(Face {
                            family: Family::of(&value),
                            name: None,
                        })
                    }
                    _ => {}
                }
            }
        }
        _ => {
            let unsupported = UNSUPPORTED_TAGS.contains(&name) || lower.starts_with('$');
            if unsupported {
                return Err(format!("`<{tag}>` is not supported yet"));
            }
            return Ok(None);
        }
    }
    Ok(Some(Tag::Open(canonical(name).into(), new)))
}

/// The color `value` of the tag `<tag>`, which may not be transparent.
fn opaque(value: &str, tag: &str) -> Result<Color, String> {
    match color::parse(value)? {
        Some(c) => Ok(c),
        None => Err(format!("`<{tag}>`: transparent text is not supported")),
    }
}

/// The font size `value` of the tag `<tag>`.
fn font_size(value: &str, tag: &str) -> Result<f32, String> {
    let size: f32 = value
        .trim_matches(['"', '\''])
        .parse()
        .map_err(|_| format!("`<{tag}>` needs a number"))?;
    if !(1.0..=200.0).contains(&size) {
        return Err(format!("`<{tag}>` is out of range"));
    }
    Ok(size)
}

/// The `key=value` pairs of an HTML-like tag, a value quoted or not.
fn attributes(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text.trim();
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().to_owned();
        let after = rest[eq + 1..].trim_start();
        let (value, next) = match after.chars().next() {
            Some(q @ ('"' | '\'')) => match after[1..].find(q) {
                Some(end) => (&after[1..1 + end], &after[2 + end..]),
                None => (&after[1..], ""),
            },
            _ => {
                let end = after.find(char::is_whitespace).unwrap_or(after.len());
                (&after[..end], &after[end..])
            }
        };
        out.push((key, value.to_owned()));
        rest = next.trim_start();
    }
    out
}

/// The character a tag `<U+221E>` names, by its code in hex.
fn unicode(tag: &str) -> Option<char> {
    let hex = tag.strip_prefix("U+").or_else(|| tag.strip_prefix("u+"))?;
    if hex.is_empty() || hex.len() > 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
}

/// `now` with what the tag `name` changed taken back to `before`, and
/// nothing else: a toggle or another tag that opened or closed inside it
/// keeps its own.
fn undo(name: &str, before: Style, now: Style) -> Style {
    let mut style = now;
    match name {
        "b" => style.bold = before.bold,
        "i" => style.italic = before.italic,
        "u" => (style.underline, style.lines) = (before.underline, before.lines),
        "s" => (style.strike, style.lines) = (before.strike, before.lines),
        "w" => (style.wave, style.lines) = (before.wave, before.lines),
        "color" => style.color = before.color,
        "size" => style.size = before.size,
        "back" => style.back = before.back,
        "font" => {
            (style.color, style.size, style.face) = (before.color, before.size, before.face);
        }
        _ => style = before,
    }
    style
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
mod wrap_tests {
    use super::*;

    fn wrap(text: &str) -> Block {
        wrapped(text, Font::new(13.0, Color::BLACK), Family::Inter, 60.0).unwrap()
    }

    /// Found in review (2026-11-22): the words were split as written, so
    /// a line of a wrapped `**bold words**` showed its stars.
    #[test]
    fn markup_holds_across_the_lines_it_wraps_into() {
        for (text, bold, italic) in [
            ("**bold words that wrap**", true, false),
            ("<b>bold words that wrap</b>", true, false),
            ("//italic words that wrap//", false, true),
        ] {
            let b = wrap(text);
            assert!(b.lines.len() > 1, "{text}");
            for line in &b.lines {
                for run in line.runs.iter().filter(|r| !r.text.trim().is_empty()) {
                    assert!(
                        !run.text.contains("**") && !run.text.contains("//"),
                        "{text}: {}",
                        run.text
                    );
                    assert_eq!(
                        (run.style.bold, run.style.italic),
                        (bold, italic),
                        "{text}: {}",
                        run.text
                    );
                }
            }
        }
        // So does a wave, closed and opened again at each break.
        let b = wrap("~~one two three four five six seven~~");
        assert!(b.lines.len() > 1);
        for line in &b.lines {
            for run in line.runs.iter().filter(|r| !r.text.trim().is_empty()) {
                assert!(run.style.wave && !run.text.contains('~'), "{}", run.text);
            }
        }
        // A link is one word, whatever its label holds.
        let b = wrap("see [[http://example.com the link label here]] now");
        let linked: Vec<&Run> = b
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .filter(|r| r.link.is_some())
            .collect();
        assert_eq!(linked.len(), 1, "{:?}", b.lines);
        assert!(linked[0].text.contains("the link label here"));
        // Words without markup break as before.
        assert!(wrap("one two three four five six").lines.len() > 1);
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
        // The face of our own, wider than Arial: "Hi" = 743 + 242.
        assert_eq!(Family::of(crate::FACE), Family::Inter);
        assert!((measure("Hi", &style, Family::Inter) - 9.85).abs() < 0.01);
        assert!(measure("Hi", &bold, Family::Inter) > measure("Hi", &style, Family::Inter));
        assert_eq!(Family::of("Inter Display"), Family::Sans);
    }

    #[test]
    fn a_font_list_takes_the_family_of_its_first_telling_name() {
        // mmdr's list, with a mono fallback: Inter tells Sans first.
        let mmdr = "Inter, ui-sans-serif, \"Segoe UI\", \"Liberation Sans\", monospace";
        assert_eq!(Family::of(mmdr), Family::Inter);
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
        // With the mark that asks for its colors, no wider; a family, a
        // skin tone and a flag are one picture each.
        assert!((wide("\u{1F6E1}\u{FE0F}") - 1.25).abs() < 0.001);
        for one in [
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
            "\u{1F44D}\u{1F3FD}",
            "\u{1F1E9}\u{1F1EA}",
            "\u{2764}\u{FE0F}",
        ] {
            assert!((wide(one) - 1.25).abs() < 0.001, "{one}");
        }
        let words = wide("ab");
        assert!((wide("a\u{1F44D}\u{1F3FD}\u{1F408}b") - (words + 2.5)).abs() < 0.001);
        // A sign a text face draws keeps an em.
        assert!((wide("\u{2713}") - 1.0).abs() < 0.001);
    }

    #[test]
    fn declines_what_it_cannot_draw() {
        for (text, part) in [
            ("<$sprite>", "<$sprite>"),
            ("[[{tip} label]]", "no address"),
            ("[[\"a b label]]", "open quote"),
            ("<color:#red/blue>x", "gradients"),
            ("<back:#red/blue>x", "gradients"),
            ("<font color=transparent>x", "transparent"),
            ("<font size=0>x", "out of range"),
            ("<sup>x</sup>", "<sup>"),
        ] {
            let err = block(text, font(), Family::Sans).unwrap_err();
            assert!(err.contains(part), "{text}: {err}");
        }
    }

    #[test]
    fn rules_and_headings_are_lines_of_their_own() {
        let b = block("a\n----\nb", font(), Family::Sans).unwrap();
        assert_eq!(b.lines.len(), 3);
        assert_eq!(b.lines[1].rule.map(|r| r.0), Some(Rule::Single));
        assert!(b.lines[1].runs.is_empty());
        assert_eq!(b.lines[1].height, Rule::Single.height());
        for (text, rule, title) in [
            ("====", Rule::Double, ""),
            ("____", Rule::Strong, ""),
            ("....", Rule::Dotted, ""),
            ("--Another title--", Rule::Single, "Another title"),
            ("== Head ==", Rule::Double, "Head"),
            ("..My title..", Rule::Dotted, "My title"),
            ("__strong__", Rule::Strong, "strong"),
        ] {
            let b = block(text, font(), Family::Sans).unwrap();
            let line = &b.lines[0];
            assert_eq!(line.rule.map(|r| r.0), Some(rule), "{text}");
            let words: String = line.runs.iter().map(|r| r.text.as_str()).collect();
            assert_eq!(words, title, "{text}");
            if !title.is_empty() {
                // Room for a stretch of the rule on either side.
                let w = measure(title, &Style::from(font()), Family::Sans);
                assert!((line.width - w - 2.0 * RULE_REACH).abs() < 0.01, "{text}");
            }
        }
        // Words between marks amid others stay words.
        let b = block("a --struck-- b", font(), Family::Sans).unwrap();
        assert!(b.lines[0].rule.is_none());
        assert!(b.lines[0].runs.iter().any(|r| r.style.strike));
        let size = font().size;
        for (text, grown, bold, italic, words) in [
            ("= Heading 1", 4.0, true, false, "Heading 1"),
            ("=NoSpace", 4.0, true, false, "NoSpace"),
            ("== Two", 2.0, true, false, "Two"),
            ("==Title== after", 2.0, true, false, "Title== after"),
            ("=== Three", 1.0, true, false, "Three"),
            ("==== Four", 0.0, false, true, "Four"),
        ] {
            let b = block(text, font(), Family::Sans).unwrap();
            let run = &b.lines[0].runs[0];
            assert_eq!(run.text, words, "{text}");
            assert_eq!(run.style.size, size + grown, "{text}");
            assert_eq!((run.style.bold, run.style.italic), (bold, italic), "{text}");
        }
        // Two or three marks are a rule among other lines, words alone; a
        // rule alone is as long as PlantUML draws it. A mark that is not
        // one byte is words.
        let b = block("a\n--\n...", font(), Family::Sans).unwrap();
        assert!(b.lines[1].rule.is_some() && b.lines[2].rule.is_some());
        for text in ["--", "...", "=="] {
            let b = block(text, font(), Family::Sans).unwrap();
            assert!(b.lines[0].rule.is_none(), "{text}");
        }
        assert_eq!(
            block("----", font(), Family::Sans).unwrap().width,
            RULE_LEAST
        );
        for text in [
            "-\u{2192} next",
            "=\u{dc}bersicht",
            "_\u{e4}h",
            ".\u{2026}",
            "--\u{e4}--",
        ] {
            block(text, font(), Family::Sans).unwrap();
        }
        // Data reads a table, not a rule, a heading or a list.
        let b = as_data(|| block("----\n= x\n* item\n**b**\n| t |", font(), Family::Sans)).unwrap();
        assert!(b.lines.iter().all(|l| l.rule.is_none() && !l.flush));
        assert_eq!(b.lines[1].runs[0].text, "= x");
        assert_eq!(b.lines[2].runs[0].text, "* item");
        assert!(b.lines[3].runs[0].style.bold);
        assert!(b.lines[4].table.is_some());
    }

    #[test]
    fn fonts_lines_and_backgrounds_of_their_own() {
        let red = color::parse("red").unwrap().unwrap();
        let blue = color::parse("blue").unwrap().unwrap();
        let yellow = color::parse("yellow").unwrap().unwrap();
        let runs = |text: &str| -> Vec<Run> {
            block(text, font(), Family::Sans).unwrap().lines[0]
                .runs
                .clone()
        };
        let r = runs("<font color=red>r</font><font color=\"blue\" size='8'>b</font>x");
        assert_eq!((r[0].text.as_str(), r[0].style.color), ("r", red));
        assert_eq!((r[1].style.color, r[1].style.size), (blue, 8.0));
        assert_eq!(r[2].style, Style::from(font()));
        let r = runs("<font size=20>big</font>");
        assert_eq!(r[0].style.size, 20.0);
        let r = runs("<font:monospaced>m</font>");
        assert_eq!(r[0].style.face.map(|f| f.family), Some(Family::Mono));
        assert_eq!(r[0].width, measure("m", &Style::from(font()), Family::Mono));
        assert_eq!(runs("<U+0025>a<U+221E>")[0].text, "%a\u{221E}");
        // A code that names no character is written as it stands.
        assert_eq!(runs("<U+D800>")[0].text, "<U+D800>");
        let r = runs("~~waved~~ <w:red>wr</w> <u:red>u</u> <s:blue>s</s>");
        assert!(r[0].style.wave && r[0].style.lines.is_none());
        assert!(r[2].style.wave && r[2].style.lines == Some(red));
        assert!(r[4].style.underline && r[4].style.lines == Some(red));
        assert!(r[6].style.strike && r[6].style.lines == Some(blue));
        let r = runs("<back:yellow>on</back> off");
        assert_eq!(r[0].style.back, Some(yellow));
        assert_eq!(r[1].style.back, None);
        // `~~` that closes nothing is an escaped `~`.
        assert_eq!(runs("a ~~ b")[0].text, "a ~ b");
        // What PlantUML shows as written.
        assert_eq!(runs("<wave:blue>w</wave>")[0].text, "<wave:blue>w</wave>");
        // A wave's end takes back the wave alone, not what a tag changed
        // within it (found in review, 2026-11-24).
        let r = runs("<color:red>~~x</color>~~ after");
        assert_eq!(r.last().unwrap().text, " after");
        assert_eq!(r.last().unwrap().style, Style::from(font()));
        let r = runs("~~a <color:blue>b~~ c</color> d");
        let c = r.iter().find(|r| r.text == " c").unwrap();
        assert!(!c.style.wave && c.style.color == blue);
        assert!(!r.last().unwrap().style.wave);
        // Mermaid's words leave what only PlantUML draws to mmdr, and show
        // a line in a color of its own as written.
        let mermaid = |text: &str| as_mermaid(|| block(text, font(), Family::Sans));
        for text in [
            "~~w~~",
            "<font color=red>x</font>",
            "<back:red>x",
            "<w>x</w>",
            "<U+0025>",
            "<img:x.png>",
        ] {
            assert!(mermaid(text).is_err(), "{text}");
        }
        let b = mermaid("<u:red>u</u> ----").unwrap();
        let words: String = b.lines[0].runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(words, "<u:red>u ----");
    }

    #[test]
    fn an_icon_takes_its_room_in_the_line_and_is_drawn_beside_the_words() {
        let line = &block("<&key> Keys <&lock-locked*2>", font(), Family::Sans)
            .unwrap()
            .lines[0];
        let icons: Vec<f32> = line
            .runs
            .iter()
            .filter_map(|run| run.icon.as_ref().map(|icon| icon.width))
            .collect();
        assert_eq!(icons.len(), 2);
        assert!((icons[1] - 2.0 * icons[0]).abs() < 1e-4);
        let words = measure(" Keys ", &Style::from(font()), Family::Sans);
        assert!((line.width - (icons[0] + icons[1] + words)).abs() < 1e-3);
        let mut canvas = crate::svg::Canvas::new("sans-serif", false);
        let drawn = block("<&key> Keys", font(), Family::Sans).unwrap();
        canvas.text(
            crate::svg::Layer::Front,
            &drawn,
            (0.0, 0.0),
            crate::svg::Align::Left,
        );
        let svg = canvas.finish(0.0, None);
        assert!(svg.contains("<path d=\"M5.5 0c"), "{svg}");
        // The words start after the icon.
        let x = format!(r#"<tspan x="{}""#, crate::svg::n(icons[0]));
        assert!(svg.contains(&x), "{svg}");
    }

    /// A name Open Iconic does not have is drawn as nothing, as PlantUML
    /// draws it; the words around it stay.
    #[test]
    fn an_icon_open_iconic_lacks_is_left_out() {
        let line = &block("<&camera> photo", font(), Family::Sans)
            .unwrap()
            .lines[0];
        assert!(line.runs.iter().all(|run| run.icon.is_none()));
        let words: String = line.runs.iter().map(|run| run.text.as_str()).collect();
        assert_eq!(words, " photo");
    }

    #[test]
    fn an_arrow_is_drawn_in_the_hand_the_handwriting_face_lacks() {
        let drawn = block("Ingest → Clean", font(), Family::Sans).unwrap();
        let svg = |face: &str| {
            let mut canvas = crate::svg::Canvas::new(face, false);
            canvas.text(
                crate::svg::Layer::Front,
                &drawn,
                (0.0, 0.0),
                crate::svg::Align::Left,
            );
            canvas.finish(0.0, None)
        };
        let hand = svg(crate::svg::HAND_FONT);
        assert!(
            !hand.contains('→') && hand.contains("stroke-linecap=\"round\""),
            "{hand}"
        );
        assert!(
            hand.contains(">Ingest </tspan>") && hand.contains("> Clean</tspan>"),
            "{hand}"
        );
        // The words keep the room the line was measured with.
        let plain = svg("sans-serif");
        assert!(plain.contains('→'), "{plain}");
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
