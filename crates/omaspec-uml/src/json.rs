//! JSON diagrams (`@startjson`).
//!
//! An object is a card of keys and values, an array a card of values. As
//! PlantUML draws them, what an object or an array holds stands in a card
//! of its own to the right, reached by a dotted arrow ([`tree`]);
//! `skinparam jsonLayout nested` (Omaspec's own) puts it in the cell
//! instead ([`nested`]). A string is drawn with its Creole, `true` and
//! `false` behind a box ticked or not, `null` as `␀` ([`cells`]).
//!
//! An object or an array folds in the pane, by its path as `#highlight`
//! writes one (`JSON_Array/4`, docs/adr/0024): it then says what it holds,
//! `{…} 4 keys`. A `#highlight` with a stereotype, and JSON this reader
//! cannot read, decline and name the line. PlantUML draws a JSON diagram
//! from its styles alone: a `<style>` colors the page, the texts around
//! it, the cards (`jsonDiagram { node {…} }`) and the highlighted rows
//! (`highlight`); skinparams change nothing, here as there, but for
//! `handwritten`, which draws every diagram by hand (docs/adr/0023), and
//! the layout.

mod cells;
mod nested;
mod tree;

pub use cells::path_name;

use crate::Declined;
use crate::annotations::Annotations;
use crate::color::Color;
use crate::color::Paint;
use crate::common::{self, Frame, Skinparam, normalize_key};
use crate::source::Line;
use crate::style::{Element, Prop};
use crate::svg::Canvas;
use crate::text::Font;

const FONT: f32 = 14.0;
/// How many objects and arrays may wrap each other. One more declines.
pub(crate) const NEST_LIMIT: u32 = 100;
const FILL: Color = Color::hex(0xF1F1F1);
const HIGHLIGHT: Color = Color::hex(0xCCFF02);
const MUTED: Color = Color::hex(0x888888);
/// The contrast a gray text keeps with its card (WCAG AA).
const LEGIBLE: f32 = 4.5;
const BAD: &str = "the JSON cannot be read here";

/// What a `<style>` reaches in a JSON diagram besides the page and the
/// texts around it.
const ELEMENTS: [Element; 2] = [
    Element::new(
        &["jsonDiagram", "node"],
        "JsonNode",
        &[Prop::BackGroundColor, Prop::LineColor, Prop::FontColor],
    ),
    Element::new(
        &["jsonDiagram", "highlight"],
        "JsonHighlight",
        &[Prop::BackGroundColor, Prop::FontColor],
    ),
];

/// How the diagram looks: PlantUML's JSON colors, changed by `<style>`.
struct Look {
    /// What an object or an array holds: in a card of its own to the
    /// right, or nested in its cell.
    nested: bool,
    page: Paint,
    fill: Paint,
    line: Color,
    ink: Color,
    lit_fill: Paint,
    lit_ink: Color,
    /// Title, header, footer and caption.
    around: [Font; 4],
    /// `skinparam handwritten true`: shapes drawn by hand.
    sketch: bool,
}

impl Look {
    fn read(params: &[Skinparam]) -> Result<Look, Declined> {
        let mut look = Look {
            nested: false,
            page: Some(Color::WHITE),
            fill: Some(FILL),
            line: Color::BLACK,
            ink: Color::BLACK,
            lit_fill: Some(HIGHLIGHT),
            lit_ink: Color::BLACK,
            around: [
                Font::new(FONT, Color::BLACK).bold(),
                Font::new(10.0, MUTED),
                Font::new(10.0, MUTED),
                Font::new(FONT, Color::BLACK),
            ],
            sketch: false,
        };
        for p in params {
            match normalize_key(&p.key).as_str() {
                "handwritten" => look.sketch = p.flag()?,
                "jsonlayout" | "yamllayout" => {
                    look.nested = match p.value.trim().to_ascii_lowercase().as_str() {
                        "nested" => true,
                        "tree" => false,
                        _ => return Err(p.unsupported()),
                    };
                }
                _ => {}
            }
        }
        let mut lit_ink = None;
        // Skinparams do not reach the diagram; what a style stands for does.
        for p in params.iter().filter(|p| p.declared.is_some()) {
            let key = normalize_key(&p.key);
            let around = ["title", "header", "footer", "caption"]
                .iter()
                .position(|name| key.starts_with(name));
            match (key.as_str(), around) {
                ("backgroundcolor", _) => look.page = p.paint()?,
                ("jsonnodebackgroundcolor", _) => look.fill = p.paint()?,
                ("jsonnodebordercolor", _) => look.line = p.solid()?,
                ("jsonnodefontcolor", _) => look.ink = p.solid()?,
                ("jsonhighlightbackgroundcolor", _) => look.lit_fill = p.paint()?,
                ("jsonhighlightfontcolor", _) => lit_ink = Some(p.solid()?),
                (_, Some(at)) => {
                    let font = &mut look.around[at];
                    match &key[["title", "header", "footer", "caption"][at].len()..] {
                        "fontcolor" => font.color = p.solid()?,
                        "fontsize" => font.size = p.number(4.0, 100.0)?,
                        "fontstyle" => {
                            let value = p.value.trim().to_ascii_lowercase();
                            let words: Vec<&str> = value.split_whitespace().collect();
                            if words.is_empty()
                                || words
                                    .iter()
                                    .any(|w| !matches!(*w, "plain" | "bold" | "italic"))
                            {
                                return Err(p.unsupported());
                            }
                            font.bold = words.contains(&"bold");
                            font.italic = words.contains(&"italic");
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        // A highlighted row keeps the cards' text color unless it has its own.
        look.lit_ink = lit_ink.unwrap_or(look.ink);
        Ok(look)
    }
}

/// A parsed JSON diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub value: Value,
    pub frame: Frame,
}

/// One JSON value. `highlight` is the row a `#highlight` path ends on;
/// `folded` an object or an array the pane folded.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub kind: Kind,
    pub highlight: bool,
    pub folded: bool,
}

/// JSON values. A number keeps the spelling from the source.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    fn new(kind: Kind) -> Value {
        Value {
            kind,
            highlight: false,
            folded: false,
        }
    }

    fn composite(&self) -> bool {
        matches!(self.kind, Kind::Array(_) | Kind::Object(_))
    }
}

impl Diagram {
    /// Folds the objects and arrays at the paths `names`, written as
    /// `#highlight` writes them (`JSON_Array/4`); a name that is no path
    /// of the value folds nothing.
    pub fn fold(&mut self, names: &[String]) {
        for name in names {
            if let Ok(path) = highlight_path(name, 0) {
                fold_at(&mut self.value, &path);
            }
        }
    }
}

fn fold_at(value: &mut Value, path: &[String]) {
    let Some((head, rest)) = path.split_first() else {
        value.folded = value.composite();
        return;
    };
    match &mut value.kind {
        Kind::Object(rows) => {
            if let Some((_, child)) = rows.iter_mut().find(|(key, _)| key == head) {
                fold_at(child, rest);
            }
        }
        Kind::Array(items) => {
            if let Some(child) = head.parse::<usize>().ok().and_then(|i| items.get_mut(i)) {
                fold_at(child, rest);
            }
        }
        _ => {}
    }
}

/// Parses the body of an `@startjson` diagram.
pub fn parse(lines: &[Line<'_>]) -> Result<Diagram, Declined> {
    let Gathered {
        text,
        marks,
        paths,
        frame,
    } = gather(lines)?;
    if text.trim().is_empty() {
        let line = lines.first().map(|l| l.no).unwrap_or(0);
        return Err(Declined::at(line, "a JSON diagram needs one value"));
    }
    let mut value = read_json(&text, &marks)?;
    for path in &paths {
        mark(&mut value, path);
    }
    Ok(Diagram { value, frame })
}

/// The SVG. Cards keep the PlantUML JSON colors on a white page unless a
/// `<style>` gives others.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    draw_mapped(diagram).map(|(svg, _)| svg)
}

/// The SVG, and where its objects and arrays are, by their paths, for the
/// pane to fold them.
pub fn draw_mapped(diagram: &Diagram) -> Result<(String, crate::map::Map), Declined> {
    let look = Look::read(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new("sans-serif", false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let notes = Annotations::new(&diagram.frame, look.around, family)?;
    let ground = |fill: Paint| fill.or(look.page).unwrap_or(Color::WHITE);
    // The words of a highlighted row read on its fill, whatever a style
    // or a theme gave the two: moved towards black or white, whichever
    // reads better there, until they do.
    let lit_ground = ground(look.lit_fill);
    let far = if Color::WHITE.contrast(lit_ground) > Color::BLACK.contrast(lit_ground) {
        Color::WHITE
    } else {
        Color::BLACK
    };
    let lit_ink = look.lit_ink.legible_on(lit_ground, far, LEGIBLE);
    let fonts = cells::Fonts::new(look.ink, ground(look.fill), FONT, LEGIBLE);
    let lit = cells::Fonts::new(lit_ink, lit_ground, FONT, LEGIBLE);
    let paint = if look.nested {
        nested::paint
    } else {
        tree::paint
    };
    let places = paint(
        &mut canvas,
        &look,
        &diagram.value,
        (&fonts, &lit),
        notes.top(),
        family,
    );
    notes.draw(&mut canvas);
    let map = crate::map::Map {
        places,
        ..crate::map::Map::default()
    }
    .placed(canvas.fit(10.0));
    Ok((canvas.finish(10.0, look.page), map))
}

struct Scan {
    depth: i32,
    in_string: bool,
    escape: bool,
}

/// The JSON text, the source line of each of its bytes, the highlight
/// paths and the frame lines around them.
struct Gathered {
    text: String,
    marks: Vec<(usize, usize)>,
    paths: Vec<Vec<String>>,
    frame: Frame,
}

/// Frame lines and `#highlight` lines at the top level, and the JSON text
/// with the source line of each byte.
fn gather(lines: &[Line<'_>]) -> Result<Gathered, Declined> {
    let mut frame = Frame::styled(&ELEMENTS);
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut paths = Vec::new();
    let mut scan = Scan {
        depth: 0,
        in_string: false,
        escape: false,
    };
    let mut at = 0;
    while at < lines.len() {
        if scan.depth == 0 && !scan.in_string {
            if let Some(next) = frame.line(lines, at)? {
                at = next;
                continue;
            }
            if let Some(rest) = common::keyword(lines[at].text, "#highlight") {
                paths.push(highlight_path(rest, lines[at].no)?);
                at += 1;
                continue;
            }
        }
        let line = lines[at];
        text.push_str(line.text);
        text.push('\n');
        marks.push((text.len(), line.no));
        for ch in line.text.chars() {
            step(&mut scan, ch, line.no)?;
        }
        step(&mut scan, '\n', line.no)?;
        at += 1;
    }
    Ok(Gathered {
        text,
        marks,
        paths,
        frame,
    })
}

fn step(scan: &mut Scan, ch: char, line: usize) -> Result<(), Declined> {
    if scan.in_string {
        if scan.escape {
            scan.escape = false;
            if ch == '\n' {
                return Err(Declined::at(line, BAD));
            }
            return Ok(());
        }
        match ch {
            '\\' => scan.escape = true,
            '"' => scan.in_string = false,
            c if (c as u32) < 0x20 => return Err(Declined::at(line, BAD)),
            _ => {}
        }
        return Ok(());
    }
    match ch {
        '"' => scan.in_string = true,
        '{' | '[' => scan.depth += 1,
        '}' | ']' => {
            scan.depth -= 1;
            if scan.depth < 0 {
                return Err(Declined::at(line, BAD));
            }
        }
        _ => {}
    }
    Ok(())
}

/// `#highlight "address" / "city"`, or `#highlight a/b/0`. A stereotype
/// declines: it would ask for a color this drawer does not know.
pub(crate) fn highlight_path(rest: &str, line: usize) -> Result<Vec<String>, Declined> {
    if rest.contains("<<") {
        return Err(Declined::at(
            line,
            "`#highlight` with a stereotype is not supported yet",
        ));
    }
    let mut parts = Vec::new();
    let mut rest = rest.trim();
    if rest.is_empty() {
        return Err(Declined::at(line, "`#highlight` needs a path"));
    }
    loop {
        let (part, next) = if rest.starts_with('"') {
            quoted_part(rest, line)?
        } else {
            bare_part(rest, line)?
        };
        parts.push(part);
        let next = next.trim_start();
        if next.is_empty() {
            return Ok(parts);
        }
        let Some(after) = next.strip_prefix('/') else {
            return Err(Declined::at(line, "`#highlight` path is not understood"));
        };
        rest = after.trim_start();
        if rest.is_empty() {
            return Err(Declined::at(line, "`#highlight` path is not understood"));
        }
    }
}

fn bare_part(text: &str, line: usize) -> Result<(String, &str), Declined> {
    let end = text
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(text.len());
    let seg = &text[..end];
    let ok = !seg.is_empty()
        && seg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '+'));
    if !ok {
        return Err(Declined::at(line, "`#highlight` path is not understood"));
    }
    Ok((seg.to_owned(), &text[end..]))
}

fn quoted_part(text: &str, line: usize) -> Result<(String, &str), Declined> {
    let mut out = String::new();
    let mut chars = text[1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &text[1 + i + c.len_utf8()..])),
            '\\' => {
                let Some((_, e)) = chars.next() else {
                    return Err(Declined::at(line, "`#highlight` path is not understood"));
                };
                let escaped = match e {
                    '"' => '"',
                    '\\' => '\\',
                    '/' => '/',
                    'n' => '\n',
                    't' => '\t',
                    _ => {
                        return Err(Declined::at(line, "`#highlight` path is not understood"));
                    }
                };
                out.push(escaped);
            }
            c => out.push(c),
        }
    }
    Err(Declined::at(line, "`#highlight` path is not understood"))
}

pub(crate) fn mark(value: &mut Value, path: &[String]) {
    let Some((head, rest)) = path.split_first() else {
        return;
    };
    match &mut value.kind {
        Kind::Object(rows) => {
            for (key, child) in rows {
                if key == head {
                    follow(child, rest);
                }
            }
        }
        Kind::Array(items) => {
            let Ok(index) = head.parse::<usize>() else {
                return;
            };
            if let Some(child) = items.get_mut(index) {
                follow(child, rest);
            }
        }
        _ => {}
    }
}

fn follow(value: &mut Value, rest: &[String]) {
    if rest.is_empty() {
        value.highlight = true;
    } else {
        mark(value, rest);
    }
}

struct In<'a> {
    text: &'a str,
    at: usize,
    marks: &'a [(usize, usize)],
}

impl<'a> In<'a> {
    fn line_no(&self) -> usize {
        self.marks
            .iter()
            .find(|(end, _)| self.at < *end)
            .or_else(|| self.marks.last())
            .map(|(_, no)| *no)
            .unwrap_or(0)
    }

    fn bad(&self) -> Declined {
        Declined::at(self.line_no(), BAD)
    }

    fn deep(&self) -> Declined {
        Declined::at(
            self.line_no(),
            "JSON nested more than 100 levels deep is not supported yet",
        )
    }

    fn peek(&self) -> Option<char> {
        self.text[self.at..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_whitespace()) {
            self.bump();
        }
    }

    fn starts(&self, prefix: &str) -> bool {
        self.text[self.at..].starts_with(prefix)
    }

    fn eat(&mut self, prefix: &str) -> bool {
        if self.starts(prefix) {
            self.at += prefix.len();
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if !self.starts(word) {
            return false;
        }
        let after = self.at + word.len();
        let boundary = self.text[after..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        if boundary {
            self.at = after;
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: u32) -> Result<Value, Declined> {
        self.skip_ws();
        match self.peek() {
            Some('{') => self.object(depth),
            Some('[') => self.array(depth),
            Some('"') => Ok(Value::new(Kind::String(self.string()?))),
            Some('t') if self.eat_word("true") => Ok(Value::new(Kind::Bool(true))),
            Some('f') if self.eat_word("false") => Ok(Value::new(Kind::Bool(false))),
            Some('n') if self.eat_word("null") => Ok(Value::new(Kind::Null)),
            Some('-' | '0'..='9') => Ok(Value::new(Kind::Number(self.number()?))),
            _ => Err(self.bad()),
        }
    }

    fn object(&mut self, depth: u32) -> Result<Value, Declined> {
        if depth >= NEST_LIMIT {
            return Err(self.deep());
        }
        self.bump();
        let mut rows = Vec::new();
        loop {
            self.skip_ws();
            if rows.is_empty() && self.eat("}") {
                break;
            }
            if self.peek() != Some('"') {
                return Err(self.bad());
            }
            let key = self.string()?;
            self.skip_ws();
            if !self.eat(":") {
                return Err(self.bad());
            }
            rows.push((key, self.value(depth + 1)?));
            self.skip_ws();
            if self.eat(",") {
                continue;
            }
            if self.eat("}") {
                break;
            }
            return Err(self.bad());
        }
        Ok(Value::new(Kind::Object(rows)))
    }

    fn array(&mut self, depth: u32) -> Result<Value, Declined> {
        if depth >= NEST_LIMIT {
            return Err(self.deep());
        }
        self.bump();
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if items.is_empty() && self.eat("]") {
                break;
            }
            items.push(self.value(depth + 1)?);
            self.skip_ws();
            if self.eat(",") {
                continue;
            }
            if self.eat("]") {
                break;
            }
            return Err(self.bad());
        }
        Ok(Value::new(Kind::Array(items)))
    }

    fn string(&mut self) -> Result<String, Declined> {
        self.bump();
        let mut out = String::new();
        loop {
            let Some(c) = self.bump() else {
                return Err(self.bad());
            };
            match c {
                '"' => return Ok(out),
                '\\' => out.push(self.escape()?),
                c if (c as u32) < 0x20 => return Err(self.bad()),
                c => out.push(c),
            }
        }
    }

    fn escape(&mut self) -> Result<char, Declined> {
        let Some(c) = self.bump() else {
            return Err(self.bad());
        };
        Ok(match c {
            '"' => '"',
            '\\' => '\\',
            '/' => '/',
            'b' => '\u{0008}',
            'f' => '\u{000c}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'u' => self.unicode()?,
            _ => return Err(self.bad()),
        })
    }

    fn unicode(&mut self) -> Result<char, Declined> {
        let unit = self.hex4()?;
        if (0xD800..=0xDBFF).contains(&unit) {
            if !self.eat("\\u") {
                return Err(self.bad());
            }
            let low = self.hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(self.bad());
            }
            let u = 0x10000 + (((unit - 0xD800) << 10) | (low - 0xDC00));
            return char::from_u32(u).ok_or_else(|| self.bad());
        }
        if (0xDC00..=0xDFFF).contains(&unit) {
            return Err(self.bad());
        }
        char::from_u32(unit).ok_or_else(|| self.bad())
    }

    fn hex4(&mut self) -> Result<u32, Declined> {
        let mut n = 0_u32;
        for _ in 0..4 {
            let Some(c) = self.bump() else {
                return Err(self.bad());
            };
            let Some(d) = c.to_digit(16) else {
                return Err(self.bad());
            };
            n = n * 16 + d;
        }
        Ok(n)
    }

    fn number(&mut self) -> Result<String, Declined> {
        let start = self.at;
        if self.peek() == Some('-') {
            self.bump();
        }
        match self.peek() {
            Some('0') => {
                self.bump();
            }
            Some('1'..='9') => {
                while matches!(self.peek(), Some('0'..='9')) {
                    self.bump();
                }
            }
            _ => return Err(self.bad()),
        }
        if self.peek() == Some('.') {
            self.bump();
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(self.bad());
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek(), Some('+' | '-')) {
                self.bump();
            }
            if !matches!(self.peek(), Some('0'..='9')) {
                return Err(self.bad());
            }
            while matches!(self.peek(), Some('0'..='9')) {
                self.bump();
            }
        }
        Ok(self.text[start..self.at].to_owned())
    }
}

fn read_json(text: &str, marks: &[(usize, usize)]) -> Result<Value, Declined> {
    let mut src = In { text, at: 0, marks };
    let value = src.value(0)?;
    src.skip_ws();
    if src.at < src.text.len() {
        return Err(src.bad());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_of_a_highlighted_row_read_on_its_fill() {
        // A green neither dark nor light, and words nearly its color.
        let source = "@startjson\n<style>\njsonDiagram {\n  highlight {\n    BackGroundColor #78824B\n    FontColor #6F7A45\n  }\n}\n</style>\n#highlight \"a\"\n{\"a\": 1}\n@endjson\n";
        let crate::Diagram::Json(diagram) = crate::parse(source).unwrap() else {
            panic!("a JSON diagram");
        };
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("#78824B") && svg.contains("#000000") && !svg.contains("#6F7A45"),
            "{svg}"
        );
    }

    fn source(body: &str) -> String {
        format!("@startjson\n{body}\n@endjson\n")
    }

    fn parsed(body: &str) -> Diagram {
        match crate::parse(&source(body)).unwrap() {
            crate::Diagram::Json(diagram) => diagram,
            other => panic!("not json: {other:?}"),
        }
    }

    fn drawn(body: &str) -> String {
        crate::render(&source(body)).unwrap()
    }

    fn declined(body: &str) -> Declined {
        crate::parse(&source(body)).unwrap_err()
    }

    fn attr(tag: &str, name: &str) -> f32 {
        let key = format!("{name}=\"");
        let at = tag.find(&key).unwrap() + key.len();
        tag[at..].split('"').next().unwrap().parse().unwrap()
    }

    /// Filled cards, not the page and not a highlight.
    fn cards(svg: &str) -> Vec<(f32, f32, f32, f32)> {
        let mut out = Vec::new();
        let mut rest = svg;
        while let Some(at) = rest.find("<rect ") {
            rest = &rest[at..];
            let end = rest.find("/>").unwrap();
            let tag = &rest[..end];
            if tag.contains("rx=\"5\"") && tag.contains("fill=\"#F1F1F1\"") {
                out.push((
                    attr(tag, "x"),
                    attr(tag, "y"),
                    attr(tag, "width"),
                    attr(tag, "height"),
                ));
            }
            rest = &rest[end..];
        }
        out
    }

    fn inside(outer: &(f32, f32, f32, f32), inner: &(f32, f32, f32, f32)) -> bool {
        inner.0 >= outer.0 + 0.5
            && inner.1 >= outer.1 + 0.5
            && inner.0 + inner.2 <= outer.0 + outer.2 - 0.5
            && inner.1 + inner.3 <= outer.1 + outer.3 - 0.5
    }

    fn cards_only_nest(svg: &str) {
        let boxes = cards(svg);
        assert!(!boxes.is_empty(), "{svg}");
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                let overlap =
                    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3;
                if overlap {
                    assert!(
                        inside(a, b) || inside(b, a),
                        "cards overlap without nesting: {a:?} {b:?}"
                    );
                }
            }
        }
    }

    fn object<'a>(value: &'a Value, key: &str) -> &'a Value {
        let Kind::Object(rows) = &value.kind else {
            panic!("not an object");
        };
        &rows.iter().find(|(k, _)| k == key).unwrap().1
    }

    #[test]
    fn a_table_reaches_the_array_beside_it_by_a_dotted_arrow() {
        let body = r#"{"fruit": "Apple", "size": "Large", "color": ["Red", "Green"]}"#;
        let svg = drawn(body);
        for words in [
            ">fruit</tspan>",
            ">Apple</tspan>",
            ">Red</tspan>",
            ">Green</tspan>",
        ] {
            assert!(svg.contains(words), "{words}: {svg}");
        }
        assert!(svg.contains("fill=\"#F1F1F1\""));
        assert!(svg.contains("font-family=\"sans-serif\""));
        assert!(svg.contains("stroke-dasharray=\"3,3\""), "{svg}");
        assert!(!svg.contains(">0</tspan>"), "an array has no index");
        // The array's card stands to the right of the table, apart.
        let boxes = cards(&svg);
        assert_eq!(boxes.len(), 2, "{boxes:?}");
        assert!(boxes[1].0 > boxes[0].0 + boxes[0].2, "{boxes:?}");
        // Nested, it sits in the cell.
        let svg = drawn(&format!("skinparam jsonLayout nested\n{body}"));
        assert!(!svg.contains("stroke-dasharray"), "{svg}");
        let boxes = cards(&svg);
        assert_eq!(boxes.len(), 2, "{boxes:?}");
        assert!(inside(&boxes[0], &boxes[1]) || inside(&boxes[1], &boxes[0]));
        cards_only_nest(&svg);
        let err =
            crate::render(&source(&format!("skinparam jsonLayout sideways\n{body}"))).unwrap_err();
        assert!(err.reason.contains("jsonLayout"), "{}", err.reason);
    }

    #[test]
    fn an_object_or_an_array_folds_by_its_path_and_says_what_it_holds() {
        let body = "{\"a\": {\"b\": [1, 2, {\"c\": 3}], \"d\": 4}, \"e\": []}\n' fold (Omaspec) a/b\n' fold (Omaspec) e\n' fold (Omaspec) nothing/here";
        let mut diagram = parsed(body);
        diagram.fold(&crate::fold::read(body));
        let a = object(&diagram.value, "a");
        assert!(!a.folded && object(a, "b").folded && object(&diagram.value, "e").folded);
        for nested in [false, true] {
            let source = if nested {
                format!("skinparam jsonLayout nested\n{body}")
            } else {
                body.to_owned()
            };
            let (svg, map) = crate::render_mapped(&self::source(&source), None).unwrap();
            assert!(svg.contains(">[…] 3 items</tspan>"), "{svg}");
            assert!(svg.contains(">[…] 0 items</tspan>"), "{svg}");
            assert!(
                !svg.contains(">c</tspan>"),
                "what a folded array holds is gone"
            );
            let told: Vec<(&str, bool)> = map
                .places
                .iter()
                .map(|p| (p.name.as_str(), p.frame))
                .collect();
            assert!(told.contains(&("a", true)), "{nested}: {told:?}");
            assert!(told.contains(&("a/b", false)), "{nested}: {told:?}");
            assert!(told.contains(&("e", false)), "{nested}: {told:?}");
            assert!(map.places.iter().all(|p| p.fixed && p.rect.w > 0.0));
        }
        // A key with a line break stays on the line of its fold.
        let name = path_name(&["k\nl".to_owned(), "x".to_owned()]);
        assert_eq!(name, "\"k\\nl\"/x");
        assert_eq!(highlight_path(&name, 0).unwrap(), ["k\nl", "x"]);
        // A key a path has to quote.
        let body = "{\"a b\": {\"x\": 1}}\n' fold (Omaspec) \"a b\"";
        let svg = crate::render(&source(body)).unwrap();
        assert!(svg.contains(">{…} 1 key</tspan>"), "{svg}");
    }

    #[test]
    fn words_for_the_three_literals_and_the_number_as_written() {
        let diagram = parsed(r#"{"on": true, "off": false, "none": null, "n": 1.50}"#);
        assert!(matches!(
            object(&diagram.value, "on").kind,
            Kind::Bool(true)
        ));
        assert!(matches!(
            object(&diagram.value, "off").kind,
            Kind::Bool(false)
        ));
        assert!(matches!(object(&diagram.value, "none").kind, Kind::Null));
        assert!(matches!(
            &object(&diagram.value, "n").kind,
            Kind::Number(n) if n == "1.50"
        ));
        let svg = drawn(r#"{"on": true, "off": false, "none": null, "n": 1.50}"#);
        // As PlantUML: a box ticked or not before the word, `␀` for null.
        assert!(svg.contains(">true</tspan>"));
        assert!(svg.contains(">false</tspan>"));
        assert!(svg.contains(">\u{2400}</tspan>"), "{svg}");
        assert!(!svg.contains(">null</tspan>"));
        assert!(svg.contains(">1.50</tspan>"));
        assert_eq!(svg.matches("<polyline").count(), 1, "one tick: {svg}");
    }

    #[test]
    fn a_person_nests_and_highlights_only_the_named_rows() {
        let body = r#"
title People
#highlight "lastName"
#highlight "address" / "city"
#highlight "missing"
{
  "firstName": "John",
  "lastName": "Smith",
  "isAlive": true,
  "age": 27,
  "address": {
    "streetAddress": "21 2nd Street",
    "city": "New York",
    "state": "NY"
  },
  "phoneNumbers": [
    {"type": "home", "number": "212 555-1234"},
    {"type": "office", "number": "646 555-4567"}
  ],
  "children": [],
  "spouse": null
}
"#;
        let diagram = parsed(body);
        assert!(object(&diagram.value, "lastName").highlight);
        assert!(!object(&diagram.value, "firstName").highlight);
        let address = object(&diagram.value, "address");
        assert!(!address.highlight);
        assert!(object(address, "city").highlight);
        assert!(!object(address, "state").highlight);
        let svg = crate::render(&source(body)).unwrap();
        assert!(svg.contains(">People</tspan>"));
        assert!(svg.contains(">Smith</tspan>"));
        assert!(svg.contains(">New York</tspan>"));
        assert_eq!(svg.matches("fill=\"#CCFF02\"").count(), 2, "{svg}");
        // The root, the address, the numbers, each of them, the children.
        assert_eq!(cards(&svg).len(), 6, "{:?}", cards(&svg));
        let svg = crate::render(&source(&format!("skinparam jsonLayout nested\n{body}"))).unwrap();
        assert!(svg.contains(">[]</tspan>"));
        assert!(svg.contains(">0</tspan>"));
        assert!(svg.contains(">1</tspan>"));
        assert_eq!(svg.matches("fill=\"#CCFF02\"").count(), 2, "{svg}");
        assert!(!svg.contains("stroke-dasharray"));
        cards_only_nest(&svg);
        assert!(cards(&svg).len() >= 5, "{:?}", cards(&svg));
    }

    #[test]
    fn an_unquoted_path_marks_the_array_row() {
        let diagram = parsed("#highlight a/b/0\n{\"a\": {\"b\": [{\"c\": 1}, {\"c\": 2}]}}");
        let row = match &object(object(&diagram.value, "a"), "b").kind {
            Kind::Array(items) => &items[0],
            _ => panic!("array"),
        };
        assert!(row.highlight);
        assert!(!object(row, "c").highlight);
    }

    #[test]
    fn duplicate_keys_stay_in_order() {
        let diagram = parsed(r#"{"a": 1, "a": 2}"#);
        let Kind::Object(rows) = &diagram.value.kind else {
            panic!("object");
        };
        assert_eq!(rows.len(), 2);
        assert!(matches!(&rows[0].1.kind, Kind::Number(n) if n == "1"));
        assert!(matches!(&rows[1].1.kind, Kind::Number(n) if n == "2"));
        let svg = drawn(r#"{"a": 1, "a": 2}"#);
        assert_eq!(svg.matches(">a</tspan>").count(), 2);
    }

    #[test]
    fn escapes_and_a_surrogate_pair() {
        let diagram = parsed(r#"{"s": "A\"\\\/\b\f\n\r\t\u0041"}"#);
        assert!(matches!(
            &object(&diagram.value, "s").kind,
            Kind::String(s) if s == "A\"\\/\u{8}\u{c}\n\r\tA"
        ));
        let diagram = parsed(r#""\uD800\uDC00""#);
        assert!(matches!(diagram.value.kind, Kind::String(s) if s == "\u{10000}"));
        let err = declined(r#""\uD800""#);
        assert_eq!(err.reason, BAD);
        // Creole, as PlantUML reads it; markup the engine does not draw
        // is drawn as written rather than not at all.
        let svg = drawn(r#"{"s": "**bold** <tag> <color:green>g</color>"}"#);
        assert!(svg.contains("font-weight=\"bold\""), "{svg}");
        assert!(
            svg.contains("&lt;tag&gt;") && svg.contains("fill=\"#008000\""),
            "{svg}"
        );
        let svg = drawn(r#"{"s": "----"}"#);
        assert!(svg.contains(">----</tspan>"), "{svg}");
    }

    #[test]
    fn a_root_array_of_words_is_one_column() {
        let svg = drawn(r#"["Red", "Green"]"#);
        assert!(svg.contains(">Red</tspan>"));
        assert!(!svg.contains(">0</tspan>"));
        assert_eq!(cards(&svg).len(), 1);
    }

    #[test]
    fn skinparams_the_cards_ignore_do_not_decline() {
        let svg = drawn(
            "skinparam backgroundColor #111111\n\
             skinparam defaultFontSize 20\n\
             skinparam defaultFontName serif\n\
             {\"a\": 1}",
        );
        assert!(svg.contains("fill=\"#F1F1F1\""));
        assert!(svg.contains("fill=\"#FFFFFF\""));
        assert!(svg.contains("font-size=\"14\""));
        assert!(svg.contains("font-family=\"sans-serif\""));
        assert!(!svg.contains("font-family=\"serif\""));
    }

    /// As in PlantUML: a style colors the page, the title, the cards and a
    /// highlighted row; the same colors as skinparams change nothing.
    #[test]
    fn a_style_colors_the_cards_and_skinparams_do_not() {
        let styled = drawn(
            "<style>\n\
             document {\n  BackGroundColor #121212\n  FontColor #BEBEBE\n}\n\
             jsonDiagram {\n  node {\n    BackGroundColor #1E1E1E\n    LineColor #E68E0D\n    FontColor #BEBEBE\n  }\n\
               highlight {\n    BackGroundColor #E68E0D\n    FontColor #121212\n  }\n}\n\
             </style>\n\
             title Settings\n\
             #highlight \"size\"\n\
             {\"theme\": \"dark\", \"size\": 14}",
        );
        assert!(styled.contains("#121212"), "{styled}");
        assert!(styled.contains("fill=\"#1E1E1E\""), "{styled}");
        assert!(styled.contains("stroke=\"#E68E0D\""), "{styled}");
        assert!(styled.contains("fill=\"#E68E0D\""), "{styled}");
        assert!(!styled.contains("#F1F1F1") && !styled.contains("#CCFF02"));
        // The title and the plain rows in the text color, the highlighted
        // row in its own.
        assert!(styled.contains("fill=\"#BEBEBE\""));
        assert!(styled.contains("fill=\"#121212\""));
        assert!(!styled.contains("fill=\"#000000\""), "{styled}");

        let skinned = drawn(
            "skinparam backgroundColor #121212\n\
             skinparam JsonNodeBackgroundColor #1E1E1E\n\
             {\"theme\": \"dark\"}",
        );
        assert!(skinned.contains("#F1F1F1") && !skinned.contains("#1E1E1E"));
        assert!(!skinned.contains("#121212"));
    }

    #[test]
    fn bad_json_and_a_stereotyped_highlight_decline_on_their_line() {
        let err = declined("{ \"a\": 1, }");
        assert_eq!(err.reason, BAD);
        assert_eq!(err.line, Some(1));
        let err = declined("{\"a\": 01}");
        assert_eq!(err.reason, BAD);
        let err = declined("1 2");
        assert_eq!(err.reason, BAD);
        let err = declined("");
        assert!(err.reason.contains("needs one value"), "{err:?}");
        let err = declined("#highlight \"a\" <<h1>>\n{\"a\": 1}");
        assert!(err.reason.contains("stereotype"), "{}", err.reason);
        assert_eq!(err.line, Some(1));
        let err = declined("#highlight\n{\"a\": 1}");
        assert!(err.reason.contains("needs a path"), "{}", err.reason);
    }

    #[test]
    fn a_comment_is_not_part_of_the_value() {
        let diagram = parsed("'\n{\"a\": 1}");
        assert!(matches!(
            &object(&diagram.value, "a").kind,
            Kind::Number(n) if n == "1"
        ));
    }

    #[test]
    fn nesting_stops_at_one_hundred() {
        let mut deep = String::new();
        for _ in 0..100 {
            deep.push_str("{\"a\":");
        }
        deep.push('1');
        for _ in 0..100 {
            deep.push('}');
        }
        assert!(crate::render(&source(&deep)).is_ok());
        deep.insert_str(0, "{\"a\":");
        deep.push('}');
        let err = declined(&deep);
        assert!(err.reason.contains("100"), "{}", err.reason);
    }
}
