//! JSON diagrams (`@startjson`).
//!
//! A value is one rounded card. An object is a table of keys and values. An
//! array of plain values is a single column; an array that holds an object
//! or an array numbers its rows. A nested object or array sits in the value
//! cell, so the key stays next to what it holds.
//!
//! Text is drawn as written: Creole in a string stays characters. A
//! `#highlight` with a stereotype, and JSON this reader cannot read,
//! decline and name the line. PlantUML draws a JSON diagram from its
//! styles alone: a `<style>` colors the page, the texts around it, the
//! cards (`jsonDiagram { node {…} }`) and the highlighted rows
//! (`highlight`); skinparams change nothing, here as there.

use crate::Declined;
use crate::annotations::Annotations;
use crate::color::Color;
use crate::color::Paint;
use crate::common::{self, Frame, Skinparam, normalize_key};
use crate::source::Line;
use crate::style::{Element, Prop};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font, Style};

const FONT: f32 = 14.0;
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 5.0;
/// Space between a cell's rule and a nested card.
const INSET: f32 = 6.0;
const RX: f32 = 5.0;
const STROKE: f32 = 1.5;
const RULE: f32 = 1.0;
/// How many objects and arrays may wrap each other. One more declines.
const NEST_LIMIT: u32 = 100;
const FILL: Color = Color::hex(0xF1F1F1);
const HIGHLIGHT: Color = Color::hex(0xCCFF02);
const MUTED: Color = Color::hex(0x888888);
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
    page: Paint,
    fill: Paint,
    line: Color,
    ink: Color,
    lit_fill: Paint,
    lit_ink: Color,
    /// Title, header, footer and caption.
    around: [Font; 4],
}

impl Look {
    fn read(params: &[Skinparam]) -> Result<Look, Declined> {
        let mut look = Look {
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
        };
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

/// One JSON value. `highlight` is the row a `#highlight` path ends on.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub kind: Kind,
    pub highlight: bool,
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
        }
    }

    fn composite(&self) -> bool {
        matches!(self.kind, Kind::Array(_) | Kind::Object(_))
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
    let look = Look::read(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new("sans-serif", false);
    let family = canvas.family();
    let notes = Annotations::new(&diagram.frame, look.around, family)?;
    let card = lay(
        &diagram.value,
        &Fonts::new(look.ink),
        &Fonts::new(look.lit_ink),
        family,
    );
    paint(&mut canvas, &look, &card, 0.0, notes.top());
    notes.draw(&mut canvas);
    Ok(canvas.finish(10.0, look.page))
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
fn highlight_path(rest: &str, line: usize) -> Result<Vec<String>, Declined> {
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

fn mark(value: &mut Value, path: &[String]) {
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

struct Fonts {
    key: Font,
    value: Font,
    null: Font,
    index: Font,
    empty: Font,
}

impl Fonts {
    /// Keys and values in `ink`; `null`, indices and empty cards stay gray.
    fn new(ink: Color) -> Fonts {
        let value = Font::new(FONT, ink);
        Fonts {
            key: value.bold(),
            value,
            null: Font::new(FONT, MUTED).italic(),
            index: Font::new(FONT, MUTED),
            empty: Font::new(FONT, MUTED),
        }
    }
}

enum Node {
    Text(Block),
    Card(Card),
}

struct Card {
    rows: Vec<Row>,
    key_w: f32,
    width: f32,
    height: f32,
    /// `{}` or `[]` when the card has no rows.
    label: Option<Block>,
}

struct Row {
    key: Option<Block>,
    value: Node,
    highlight: bool,
    height: f32,
}

/// The cards of `value`; a highlighted row's text takes the `lit` fonts.
fn lay(value: &Value, fonts: &Fonts, lit: &Fonts, family: Family) -> Node {
    let fonts = if value.highlight { lit } else { fonts };
    match &value.kind {
        Kind::Object(rows) => Node::Card(lay_object(rows, fonts, lit, family)),
        Kind::Array(items) => Node::Card(lay_array(items, fonts, lit, family)),
        Kind::String(text) => Node::Text(words(text, fonts.value, family)),
        Kind::Number(text) => Node::Text(words(text, fonts.value, family)),
        Kind::Bool(true) => Node::Text(words("true", fonts.value, family)),
        Kind::Bool(false) => Node::Text(words("false", fonts.value, family)),
        Kind::Null => Node::Text(words("null", fonts.null, family)),
    }
}

fn lay_object(rows: &[(String, Value)], fonts: &Fonts, lit: &Fonts, family: Family) -> Card {
    if rows.is_empty() {
        return empty_card("{}", fonts, family);
    }
    let laid = rows
        .iter()
        .map(|(key, value)| {
            let highlight = value.highlight;
            let key_font = if highlight { lit.key } else { fonts.key };
            let key = words(key, key_font, family);
            let value = lay(value, fonts, lit, family);
            row(Some(key), value, highlight)
        })
        .collect();
    card(laid)
}

fn lay_array(items: &[Value], fonts: &Fonts, lit: &Fonts, family: Family) -> Card {
    if items.is_empty() {
        return empty_card("[]", fonts, family);
    }
    let indexed = items.iter().any(Value::composite);
    let laid = items
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let key = indexed.then(|| words(&index.to_string(), fonts.index, family));
            let highlight = value.highlight;
            let value = lay(value, fonts, lit, family);
            row(key, value, highlight)
        })
        .collect();
    card(laid)
}

fn empty_card(label: &str, fonts: &Fonts, family: Family) -> Card {
    let label = words(label, fonts.empty, family);
    Card {
        width: PAD_X + label.width + PAD_X,
        height: PAD_Y + label.height + PAD_Y,
        label: Some(label),
        rows: Vec::new(),
        key_w: 0.0,
    }
}

fn row(key: Option<Block>, value: Node, highlight: bool) -> Row {
    let key_h = key.as_ref().map(|b| b.height).unwrap_or(0.0);
    let (value_pad, value_h) = match &value {
        Node::Text(block) => (PAD_Y, block.height),
        Node::Card(card) => (INSET, card.height),
    };
    let height = (PAD_Y + key_h + PAD_Y).max(value_pad + value_h + value_pad);
    Row {
        key,
        value,
        highlight,
        height,
    }
}

fn card(rows: Vec<Row>) -> Card {
    let has_keys = rows.iter().any(|row| row.key.is_some());
    let key_text = rows
        .iter()
        .filter_map(|row| row.key.as_ref())
        .map(|block| block.width)
        .fold(0.0, f32::max);
    let key_w = if has_keys {
        PAD_X + key_text + PAD_X
    } else {
        0.0
    };
    let value_w = rows
        .iter()
        .map(|row| match &row.value {
            Node::Text(block) => PAD_X + block.width + PAD_X,
            Node::Card(inner) => INSET + inner.width + INSET,
        })
        .fold(0.0, f32::max);
    let height = rows.iter().map(|row| row.height).sum();
    Card {
        rows,
        key_w,
        width: key_w + value_w,
        height,
        label: None,
    }
}

fn words(text: &str, font: Font, family: Family) -> Block {
    let style = Style::from(font);
    let lines = text
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                Vec::new()
            } else {
                vec![(line.to_owned(), style)]
            }
        })
        .collect();
    text::styled(lines, font, family)
}

fn paint(canvas: &mut Canvas, look: &Look, node: &Node, x: f32, y: f32) {
    if let Node::Card(card) = node {
        paint_card(canvas, look, card, x, y);
    }
}

fn paint_card(canvas: &mut Canvas, look: &Look, card: &Card, x: f32, y: f32) {
    let border = Stroke::new(look.line, STROKE);
    canvas.rect(
        Layer::Background,
        (x, y, card.width, card.height),
        RX,
        look.fill,
        None,
    );
    if let Some(label) = &card.label {
        let tx = x + (card.width - label.width) / 2.0;
        let ty = y + (card.height - label.height) / 2.0;
        canvas.text(Layer::Front, label, (tx, ty), Align::Left);
        canvas.rect(
            Layer::Frames,
            (x, y, card.width, card.height),
            RX,
            None,
            Some(border),
        );
        return;
    }
    let rule = Stroke::new(look.line, RULE);
    let mut row_y = y;
    for (index, row) in card.rows.iter().enumerate() {
        if row.highlight {
            let inset = 2.0;
            canvas.rect(
                Layer::Background,
                (
                    x + inset,
                    row_y + inset,
                    card.width - 2.0 * inset,
                    row.height - 2.0 * inset,
                ),
                2.0,
                look.lit_fill,
                None,
            );
        }
        let (key_y, value_y) = row_places(row_y, row);
        if let Some(key) = &row.key {
            canvas.text(Layer::Front, key, (x + PAD_X, key_y), Align::Left);
        }
        match &row.value {
            Node::Text(block) => {
                canvas.text(
                    Layer::Front,
                    block,
                    (x + card.key_w + PAD_X, value_y),
                    Align::Left,
                );
            }
            Node::Card(inner) => {
                paint_card(canvas, look, inner, x + card.key_w + INSET, value_y);
            }
        }
        row_y += row.height;
        if index + 1 != card.rows.len() {
            canvas.line(Layer::Frames, (x, row_y), (x + card.width, row_y), rule);
        }
    }
    if card.key_w > 0.0 {
        canvas.line(
            Layer::Frames,
            (x + card.key_w, y),
            (x + card.key_w, y + card.height),
            rule,
        );
    }
    canvas.rect(
        Layer::Frames,
        (x, y, card.width, card.height),
        RX,
        None,
        Some(border),
    );
}

/// The top of the key and of the value. Both sit at the top of the row:
/// a tall nested card keeps its key beside the first line, not the middle.
fn row_places(row_y: f32, row: &Row) -> (f32, f32) {
    let value_y = match &row.value {
        Node::Text(_) => row_y + PAD_Y,
        Node::Card(_) => row_y + INSET,
    };
    (row_y + PAD_Y, value_y)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_table_keeps_the_words_and_nests_the_array() {
        let svg = drawn(r#"{"fruit": "Apple", "size": "Large", "color": ["Red", "Green"]}"#);
        assert!(svg.contains(">fruit</tspan>"));
        assert!(svg.contains(">Apple</tspan>"));
        assert!(svg.contains(">Red</tspan>"));
        assert!(svg.contains(">Green</tspan>"));
        assert!(svg.contains("fill=\"#F1F1F1\""));
        assert!(svg.contains("font-family=\"sans-serif\""));
        assert!(!svg.contains("stroke-dasharray"), "{svg}");
        assert!(!svg.contains('☑'));
        assert!(!svg.contains(">0</tspan>"), "a list of words has no index");
        let boxes = cards(&svg);
        assert_eq!(boxes.len(), 2, "{boxes:?}");
        assert!(inside(&boxes[0], &boxes[1]) || inside(&boxes[1], &boxes[0]));
        cards_only_nest(&svg);
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
        assert!(svg.contains(">true</tspan>"));
        assert!(svg.contains(">false</tspan>"));
        assert!(svg.contains(">null</tspan>"));
        assert!(svg.contains("font-style=\"italic\""));
        assert!(svg.contains(">1.50</tspan>"));
        assert!(!svg.contains('☑'));
        assert!(!svg.contains('\u{2400}'));
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
        let svg = drawn(r#"{"s": "**bold** <tag>"}"#);
        assert!(svg.contains(">**bold** &lt;tag&gt;</tspan>"), "{svg}");
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
