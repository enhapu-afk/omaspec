//! Mermaid `erDiagram`.
//!
//! Entities are tables: the name, then each attribute on its own row.
//! A relationship is a line with a crow's foot at each end that has a
//! cardinality. `direction LR` grows to the right and `TB` downward;
//! `RL` and `BT` are the same picture mirrored. A style may set `fill`,
//! `stroke`, `stroke-width`, `stroke-dasharray` and `color`. Anything
//! else in a style, a cardinality this reader does not know, and a line
//! it cannot place are refused by name.

use std::collections::HashMap;

use crate::Declined;
use crate::color::{self, Color, Paint};
use crate::common::{Text, keyword, text_at};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const PAD: f32 = 10.0;
const ROUND: f32 = 6.0;
const MARGIN: f32 = 12.0;
/// Room between two ranks, before a label asks for more.
const RANK: f32 = 72.0;
/// Room between two entities of one rank, before a label asks for more.
const STACK: f32 = 48.0;
/// Added to a label so the chip, the jog and both crow's feet stay out of the boxes.
const LABEL_PAD: f32 = 100.0;
const LOOP: f32 = 36.0;

/// An ER diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    /// The way the ranks grow. `flip` mirrors that axis.
    pub axis: Axis,
    pub flip: bool,
    pub entities: Vec<Entity>,
    pub relations: Vec<Relation>,
    pub look: Look,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Right,
    Down,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub key: String,
    pub label: String,
    pub attributes: Vec<Attribute>,
    pub line: usize,
    pub fill: Option<Color>,
    pub border: Option<Color>,
    pub ink: Option<Color>,
    pub thickness: Option<f32>,
    pub dash: Option<(f32, f32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub type_name: String,
    pub name: String,
    pub keys: Vec<Key>,
    pub comment: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Pk,
    Fk,
    Uk,
}

impl Key {
    fn as_str(self) -> &'static str {
        match self {
            Key::Pk => "PK",
            Key::Fk => "FK",
            Key::Uk => "UK",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
    pub from_card: Option<Card>,
    pub to_card: Option<Card>,
    pub dashed: bool,
    pub line: usize,
}

/// Crow's foot. `None` on a relation is a plain line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    One,
    ZeroOne,
    Many,
    ZeroMany,
}

/// Colors the theme fills in. The reader leaves the Mermaid defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    pub font_size: f32,
    /// Theme text, before it is checked against a fill.
    pub preferred: Color,
    pub fill: Color,
    pub border: Color,
    pub line: Color,
    pub background: Paint,
    pub thickness: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            font_size: 14.0,
            preferred: Color::BLACK,
            fill: Color::hex(0xECECFF),
            border: Color::hex(0x7B88A8),
            line: Color::hex(0x2F3B4D),
            background: Some(Color::WHITE),
            thickness: 1.5,
        }
    }
}

/// The ER diagram `source` describes, or `None` when it is not an
/// `erDiagram` or it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not an ER diagram, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

/// The SVG. Labels sit in the gap the layout left for them.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    let mut canvas = Canvas::new(&diagram.look.font_name, false);
    let family = canvas.family();
    let laid = lay(diagram, family)?;
    paint_diagram(&mut canvas, diagram, &laid)?;
    Ok(canvas.finish(MARGIN, diagram.look.background))
}

struct Reason {
    line: usize,
    what: &'static str,
}

enum Stop {
    Foreign,
    At(Reason),
}

fn read(source: &str) -> Result<Diagram, Stop> {
    let mut title = None;
    let mut lines = Vec::new();
    let mut iter = source.lines().enumerate().peekable();
    if iter.peek().is_some_and(|(_, raw)| raw.trim() == "---") {
        iter.next();
        for (no, raw) in iter.by_ref() {
            let text = raw.trim();
            if text == "---" {
                break;
            }
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            let Some((key, value)) = text.split_once(':') else {
                return Err(Stop::At(Reason {
                    line: no,
                    what: "a front matter setting",
                }));
            };
            if key.trim().eq_ignore_ascii_case("title") {
                let value = unquote(value.trim());
                if !value.is_empty() {
                    title = Some(text_at(&value, no));
                }
                continue;
            }
            return Err(Stop::At(Reason {
                line: no,
                what: "a front matter setting",
            }));
        }
    }
    for (no, raw) in iter {
        let Some(text) = bare(raw) else { continue };
        lines.push((no, text.to_owned()));
    }
    let Some((header, rest)) = lines.split_first() else {
        return Err(Stop::Foreign);
    };
    if keyword(&header.1, "erDiagram").is_none() {
        return Err(Stop::Foreign);
    }
    if !header.1["erDiagram".len()..].trim().is_empty() {
        return Err(Stop::At(Reason {
            line: header.0,
            what: "an unknown line",
        }));
    }
    let mut chart = Chart::new(title);
    for (line, text) in rest {
        chart.line(*line, text).map_err(Stop::At)?;
    }
    chart.finish().map_err(Stop::At)
}

fn bare(raw: &str) -> Option<&str> {
    let text = raw.trim();
    if text.starts_with("%%") {
        return None;
    }
    let text = match text.find("%%") {
        Some(at) => text[..at].trim(),
        None => text,
    };
    (!text.is_empty()).then_some(text)
}

struct Chart {
    title: Option<Text>,
    axis: Axis,
    flip: bool,
    titled: bool,
    entities: Vec<Entity>,
    index: HashMap<String, usize>,
    relations: Vec<Relation>,
    open: Option<usize>,
}

impl Chart {
    fn new(title: Option<Text>) -> Self {
        Self {
            titled: title.is_some(),
            title,
            axis: Axis::Right,
            flip: false,
            entities: Vec::new(),
            index: HashMap::new(),
            relations: Vec::new(),
            open: None,
        }
    }

    fn line(&mut self, line: usize, text: &str) -> Result<(), Reason> {
        if let Some(entity) = self.open {
            let (body, closes) = split_close(text);
            if !body.is_empty() {
                self.entities[entity]
                    .attributes
                    .push(attribute(&body, line)?);
            }
            if closes {
                self.open = None;
            }
            return Ok(());
        }
        if let Some(rest) = keyword(text, "direction") {
            let (axis, flip) = direction(rest).ok_or(Reason {
                line,
                what: "a direction",
            })?;
            self.axis = axis;
            self.flip = flip;
            return Ok(());
        }
        if let Some(rest) = keyword(text, "title") {
            if self.titled {
                return Err(Reason {
                    line,
                    what: "a title",
                });
            }
            let value = unquote(rest);
            if value.is_empty() {
                return Err(Reason {
                    line,
                    what: "a title",
                });
            }
            self.title = Some(text_at(&value, line));
            self.titled = true;
            return Ok(());
        }
        if skipped_access(text) {
            return Ok(());
        }
        if let Some(rest) = keyword(text, "style") {
            return self.style(rest, line);
        }
        if keyword(text, "classDef").is_some() || keyword(text, "cssClass").is_some() {
            return Err(Reason {
                line,
                what: "a style",
            });
        }
        // A crow's foot uses `{`, so a relationship is recognized before an
        // entity block. An entity block does not carry `--` or `..`.
        if text.contains("--") || text.contains("..") {
            return self.relation(text, line);
        }
        if text.contains('{') {
            return self.open_entity(text, line);
        }
        if entity_ref(text).is_some() {
            self.ensure(text, line)?;
            return Ok(());
        }
        Err(Reason {
            line,
            what: "an unknown line",
        })
    }

    fn open_entity(&mut self, text: &str, line: usize) -> Result<(), Reason> {
        let Some((head, tail)) = text.split_once('{') else {
            return Err(Reason {
                line,
                what: "an entity",
            });
        };
        let head = head.trim();
        if entity_ref(head).is_none() {
            return Err(Reason {
                line,
                what: "an entity",
            });
        }
        let entity = self.ensure(head, line)?;
        let (body, closes) = split_close(tail.trim());
        if !body.is_empty() {
            self.entities[entity]
                .attributes
                .push(attribute(&body, line)?);
        }
        if !closes {
            self.open = Some(entity);
        }
        Ok(())
    }

    fn ensure(&mut self, written: &str, line: usize) -> Result<usize, Reason> {
        let (key, label) = entity_ref(written).ok_or(Reason {
            line,
            what: "an entity",
        })?;
        if let Some(index) = self.index.get(&key).copied() {
            if label != key && self.entities[index].label == key {
                self.entities[index].label = label;
            }
            return Ok(index);
        }
        let index = self.entities.len();
        self.index.insert(key.clone(), index);
        self.entities.push(Entity {
            key,
            label,
            attributes: Vec::new(),
            line,
            fill: None,
            border: None,
            ink: None,
            thickness: None,
            dash: None,
        });
        Ok(index)
    }

    fn relation(&mut self, text: &str, line: usize) -> Result<(), Reason> {
        let (body, label) = match text.split_once(':') {
            Some((body, label)) => {
                let label = unquote(label.trim());
                (body.trim(), (!label.is_empty()).then_some(label))
            }
            None => (text.trim(), None),
        };
        let (at, dashed) = if let Some(at) = body.find("--") {
            (at, false)
        } else if let Some(at) = body.find("..") {
            (at, true)
        } else {
            return Err(Reason {
                line,
                what: "a relationship",
            });
        };
        let left = body[..at].trim();
        let right = body[at + 2..].trim();
        let (left_name, from_card) = split_card_left(left).ok_or(Reason {
            line,
            what: "a relationship",
        })?;
        let (to_card, right_name) = split_card_right(right).ok_or(Reason {
            line,
            what: "a relationship",
        })?;
        // A token that is not a cardinality and not an entity name is one
        // relationship this reader does not draw, not a broken entity.
        if entity_ref(left_name).is_none() || entity_ref(right_name).is_none() {
            return Err(Reason {
                line,
                what: "a relationship",
            });
        }
        let from = self.ensure(left_name, line)?;
        let to = self.ensure(right_name, line)?;
        self.relations.push(Relation {
            from,
            to,
            label,
            from_card,
            to_card,
            dashed,
            line,
        });
        Ok(())
    }

    fn style(&mut self, text: &str, line: usize) -> Result<(), Reason> {
        let mut parts = text.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("").trim();
        let rest = parts.next().unwrap_or("").trim();
        if name.is_empty() || rest.is_empty() || entity_ref(name).is_none() {
            return Err(Reason {
                line,
                what: "a style",
            });
        }
        let paint = paint_style(rest, line)?;
        let entity = self.ensure(name, line)?;
        let slot = &mut self.entities[entity];
        if let Some(fill) = paint.fill {
            slot.fill = Some(fill);
        }
        if let Some(border) = paint.border {
            slot.border = Some(border);
        }
        if let Some(ink) = paint.ink {
            slot.ink = Some(ink);
        }
        if paint.thickness.is_some() {
            slot.thickness = paint.thickness;
        }
        if paint.dash.is_some() {
            slot.dash = paint.dash;
        }
        Ok(())
    }

    fn finish(self) -> Result<Diagram, Reason> {
        if let Some(entity) = self.open {
            return Err(Reason {
                line: self.entities[entity].line,
                what: "an entity",
            });
        }
        Ok(Diagram {
            title: self.title,
            axis: self.axis,
            flip: self.flip,
            entities: self.entities,
            relations: self.relations,
            look: Look::default(),
        })
    }
}

fn direction(rest: &str) -> Option<(Axis, bool)> {
    if rest.split_whitespace().nth(1).is_some() {
        return None;
    }
    match rest.trim().to_ascii_lowercase().as_str() {
        "lr" => Some((Axis::Right, false)),
        "rl" => Some((Axis::Right, true)),
        "tb" | "td" => Some((Axis::Down, false)),
        "bt" => Some((Axis::Down, true)),
        _ => None,
    }
}

fn skipped_access(text: &str) -> bool {
    let head = text.split([':', ' ', '\t']).next().unwrap_or("");
    head.eq_ignore_ascii_case("accTitle") || head.eq_ignore_ascii_case("accDescr")
}

fn split_close(text: &str) -> (String, bool) {
    let text = text.trim();
    let Some(body) = text.strip_suffix('}') else {
        return (text.to_owned(), false);
    };
    if body.matches('"').count().is_multiple_of(2) {
        (body.trim().to_owned(), true)
    } else {
        (text.to_owned(), false)
    }
}

fn attribute(text: &str, line: usize) -> Result<Attribute, Reason> {
    let (head, comment) = take_comment(text).ok_or(Reason {
        line,
        what: "an attribute",
    })?;
    let (words, keys) = tokens(head);
    let (type_name, name) = match words.as_slice() {
        [name] => (String::new(), name.clone()),
        [type_name, name] => (type_name.clone(), name.clone()),
        [type_name, rest @ ..] if !rest.is_empty() => (type_name.clone(), rest.join(" ")),
        _ => {
            return Err(Reason {
                line,
                what: "an attribute",
            });
        }
    };
    if name.is_empty() {
        return Err(Reason {
            line,
            what: "an attribute",
        });
    }
    Ok(Attribute {
        type_name,
        name,
        keys,
        comment,
        line,
    })
}

fn take_comment(text: &str) -> Option<(&str, Option<String>)> {
    let text = text.trim();
    if !text.contains('"') {
        return Some((text, None));
    }
    let rest = text.strip_suffix('"')?;
    let open = rest.rfind('"')?;
    let comment = rest[open + 1..].to_owned();
    if comment.contains('"') {
        return None;
    }
    Some((rest[..open].trim(), Some(comment)))
}

fn tokens(text: &str) -> (Vec<String>, Vec<Key>) {
    let mut words = Vec::new();
    let mut keys = Vec::new();
    for raw in text.split_whitespace() {
        for piece in raw.split(',') {
            let piece = piece.trim().trim_matches(';');
            if piece.is_empty() {
                continue;
            }
            match piece.to_ascii_uppercase().as_str() {
                "PK" => keys.push(Key::Pk),
                "FK" => keys.push(Key::Fk),
                "UK" => keys.push(Key::Uk),
                _ => words.push(piece.to_owned()),
            }
        }
    }
    (words, keys)
}

fn entity_ref(text: &str) -> Option<(String, String)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let (name, alias) = if let Some(open) = text.find('[') {
        if !text.ends_with(']') {
            return None;
        }
        let alias = unquote(text[open + 1..text.len() - 1].trim());
        if alias.is_empty() {
            return None;
        }
        (text[..open].trim(), alias)
    } else {
        (text, String::new())
    };
    let key = quoted_or_word(name)?;
    let label = if alias.is_empty() { key.clone() } else { alias };
    Some((key, label))
}

fn quoted_or_word(text: &str) -> Option<String> {
    let text = text.trim();
    if let Some(inner) = text.strip_prefix('"') {
        let inner = inner.strip_suffix('"')?;
        return (!inner.is_empty() && !inner.contains('"')).then(|| inner.to_owned());
    }
    if text.chars().all(name_char) && text.chars().any(|c| c != '-' && c != '.') {
        return Some(text.to_owned());
    }
    None
}

fn name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

fn split_card_left(text: &str) -> Option<(&str, Option<Card>)> {
    let text = text.trim();
    let chars: Vec<char> = text.chars().collect();
    if let Some(token) = card_tail(&chars) {
        let name = text[..text.len() - token.len()].trim();
        return Some((name, Some(card(&token)?)));
    }
    Some((text, None))
}

fn split_card_right(text: &str) -> Option<(Option<Card>, &str)> {
    let text = text.trim();
    let chars: Vec<char> = text.chars().collect();
    if let Some(token) = card_head(&chars) {
        let name = text[token.len()..].trim();
        return Some((Some(card(&token)?), name));
    }
    Some((None, text))
}

/// The cardinality stuck to the end of an entity name: two marks when both
/// are cardinality characters, otherwise one.
fn card_tail(chars: &[char]) -> Option<String> {
    let n = chars.len();
    if n >= 2 && card_char(chars[n - 2]) && card_char(chars[n - 1]) {
        return Some(chars[n - 2..].iter().collect());
    }
    chars
        .last()
        .filter(|c| card_char(**c))
        .map(|c| c.to_string())
}

fn card_head(chars: &[char]) -> Option<String> {
    if chars.len() >= 2 && card_char(chars[0]) && card_char(chars[1]) {
        return Some(chars[..2].iter().collect());
    }
    chars
        .first()
        .filter(|c| card_char(**c))
        .map(|c| c.to_string())
}

fn card_char(c: char) -> bool {
    matches!(c, '|' | 'o' | '{' | '}')
}

fn card(token: &str) -> Option<Card> {
    match token {
        "||" | "|" => Some(Card::One),
        "|o" | "o|" | "o" => Some(Card::ZeroOne),
        "}|" | "|{" => Some(Card::Many),
        "}o" | "o{" | "}" | "{" => Some(Card::ZeroMany),
        _ => None,
    }
}

struct PaintStyle {
    fill: Option<Color>,
    border: Option<Color>,
    ink: Option<Color>,
    thickness: Option<f32>,
    dash: Option<(f32, f32)>,
}

fn paint_style(text: &str, line: usize) -> Result<PaintStyle, Reason> {
    let mut style = PaintStyle {
        fill: None,
        border: None,
        ink: None,
        thickness: None,
        dash: None,
    };
    let mut parts: Vec<String> = Vec::new();
    for part in text.split(',') {
        let part = part.trim().trim_end_matches(';').trim();
        if part.is_empty() {
            continue;
        }
        let continues_dash = !part.contains(':')
            && parts.last().is_some_and(|prev: &String| {
                prev.split_once(':')
                    .is_some_and(|(key, _)| key.trim().eq_ignore_ascii_case("stroke-dasharray"))
            });
        if continues_dash {
            let index = parts.len() - 1;
            parts[index].push(' ');
            parts[index].push_str(part);
        } else {
            parts.push(part.to_owned());
        }
    }
    if parts.is_empty() {
        return Err(Reason {
            line,
            what: "a style",
        });
    }
    for part in &parts {
        let Some((key, value)) = part.split_once(':') else {
            return Err(Reason {
                line,
                what: "a style",
            });
        };
        let value = value.trim();
        if value.is_empty() {
            return Err(Reason {
                line,
                what: "a style",
            });
        }
        match key.trim().to_ascii_lowercase().as_str() {
            "fill" => style.fill = Some(solid_color(value, line)?),
            "stroke" => style.border = Some(solid_color(value, line)?),
            "color" => style.ink = Some(solid_color(value, line)?),
            "stroke-width" => {
                style.thickness = Some(px(value).ok_or(Reason {
                    line,
                    what: "a style",
                })?);
            }
            "stroke-dasharray" => {
                style.dash = Some(dash_of(value).ok_or(Reason {
                    line,
                    what: "a style",
                })?);
            }
            _ => {
                return Err(Reason {
                    line,
                    what: "a style",
                });
            }
        }
    }
    Ok(style)
}

fn solid_color(value: &str, line: usize) -> Result<Color, Reason> {
    match color::parse(value) {
        Ok(Some(color)) => Ok(color),
        _ => Err(Reason {
            line,
            what: "a color",
        }),
    }
}

fn px(value: &str) -> Option<f32> {
    let value = value.trim();
    let value = value
        .strip_suffix("px")
        .or_else(|| value.strip_suffix("PX"))
        .unwrap_or(value)
        .trim();
    let n: f32 = value.parse().ok()?;
    (n.is_finite() && (0.0..=20.0).contains(&n)).then_some(n)
}

fn dash_of(value: &str) -> Option<(f32, f32)> {
    let mut parts = value.split_whitespace();
    let on = px(parts.next()?)?;
    let off = match parts.next() {
        Some(off) => px(off)?,
        None => on,
    };
    if parts.next().is_some() || on == 0.0 || off == 0.0 {
        return None;
    }
    Some((on, off))
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text)
        .to_owned()
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn right(self) -> f32 {
        self.x + self.w
    }
    fn bottom(self) -> f32 {
        self.y + self.h
    }
    fn cx(self) -> f32 {
        self.x + self.w / 2.0
    }
    fn cy(self) -> f32 {
        self.y + self.h / 2.0
    }
}

struct Row {
    type_name: Option<Block>,
    name: Block,
    keys: Option<Block>,
    comment: Option<Block>,
    height: f32,
}

struct Measured {
    title: Block,
    rows: Vec<Row>,
    width: f32,
    height: f32,
}

struct Laid {
    boxes: Vec<Rect>,
    measured: Vec<Measured>,
    title: Option<Block>,
}

fn lay(diagram: &Diagram, family: Family) -> Result<Laid, Declined> {
    let paper = diagram.look.background.unwrap_or(Color::WHITE);
    let page_ink = readable(paper, diagram.look.preferred);
    let font = Font::new(diagram.look.font_size, page_ink);
    let title_font = Font::new(diagram.look.font_size + 2.0, page_ink).bold();
    let measured = diagram
        .entities
        .iter()
        .map(|entity| measure_entity(entity, diagram, family))
        .collect::<Result<Vec<_>, _>>()?;
    let title = diagram
        .title
        .as_ref()
        .map(|text| {
            text::block(&text.value, title_font, family).map_err(|err| Declined::at(text.line, err))
        })
        .transpose()?;
    let label_room = label_room(diagram, font, family)?;
    let port_sep = port_sep(diagram, font, family)?;
    let loops = loop_extras(diagram, font, family)?;
    let rank_gap = RANK.max(label_room + LABEL_PAD);
    let stack_gap = STACK.max(label_room + LABEL_PAD);
    let mut measured = measured;
    spread_sides(&mut measured, diagram, port_sep);
    let mut boxes = place(diagram, &measured, rank_gap, stack_gap, &loops);
    let title_h = title
        .as_ref()
        .map(|block| block.height + 14.0)
        .unwrap_or(0.0);
    for rect in &mut boxes {
        rect.y += title_h;
    }
    Ok(Laid {
        boxes,
        measured,
        title,
    })
}

fn label_room(diagram: &Diagram, font: Font, family: Family) -> Result<f32, Declined> {
    let mut room = 0.0_f32;
    for relation in &diagram.relations {
        let Some(label) = &relation.label else {
            continue;
        };
        if relation.from == relation.to {
            continue;
        }
        let block =
            text::block(label, font, family).map_err(|err| Declined::at(relation.line, err))?;
        room = room.max(block.width.max(block.height));
    }
    Ok(room)
}

/// Room past the box for each self-loop, including its label.
/// How far apart two relationship lines must sit so their labels do not meet.
fn port_sep(diagram: &Diagram, font: Font, family: Family) -> Result<f32, Declined> {
    let mut sep = 28.0_f32;
    for relation in &diagram.relations {
        if relation.from == relation.to {
            continue;
        }
        let Some(label) = &relation.label else {
            continue;
        };
        let block =
            text::block(label, font, family).map_err(|err| Declined::at(relation.line, err))?;
        let across = match diagram.axis {
            Axis::Right => block.height,
            Axis::Down => block.width,
        };
        sep = sep.max(across + 10.0);
    }
    Ok(sep)
}

/// A side with several relationships grows until those lines clear each other.
fn spread_sides(measured: &mut [Measured], diagram: &Diagram, port_sep: f32) {
    let mut load = vec![0usize; measured.len()];
    for relation in &diagram.relations {
        if relation.from == relation.to {
            continue;
        }
        if let Some(count) = load.get_mut(relation.from) {
            *count += 1;
        }
        if let Some(count) = load.get_mut(relation.to) {
            *count += 1;
        }
    }
    for (item, count) in measured.iter_mut().zip(load) {
        if count <= 1 {
            continue;
        }
        let need = (count as f32 - 1.0) * port_sep / 0.64;
        match diagram.axis {
            Axis::Right => item.height = item.height.max(need),
            Axis::Down => item.width = item.width.max(need),
        }
    }
}

fn loop_extras(diagram: &Diagram, font: Font, family: Family) -> Result<Vec<f32>, Declined> {
    (0..diagram.entities.len())
        .map(|index| loop_extra(diagram, index, font, family))
        .collect()
}

fn loop_extra(
    diagram: &Diagram,
    index: usize,
    font: Font,
    family: Family,
) -> Result<f32, Declined> {
    let mut count = 0.0_f32;
    let mut label = 0.0_f32;
    for relation in &diagram.relations {
        if relation.from != index || relation.to != index {
            continue;
        }
        count += 1.0;
        if let Some(text) = &relation.label {
            let block =
                text::block(text, font, family).map_err(|err| Declined::at(relation.line, err))?;
            let along = match diagram.axis {
                Axis::Right => block.width,
                Axis::Down => block.height,
            };
            label = label.max(along + 8.0);
        }
    }
    if count == 0.0 {
        Ok(0.0)
    } else {
        Ok(LOOP + count * 8.0 + label)
    }
}

fn measure_entity(
    entity: &Entity,
    diagram: &Diagram,
    family: Family,
) -> Result<Measured, Declined> {
    let fill = entity.fill.unwrap_or(diagram.look.fill);
    let ink = entity
        .ink
        .unwrap_or_else(|| readable(fill, diagram.look.preferred));
    let font = Font::new(diagram.look.font_size, ink);
    let title = text::block(&entity.label, font.bold(), family)
        .map_err(|err| Declined::at(entity.line, err))?;
    let mut rows = Vec::new();
    let mut width = title.width;
    for attribute in &entity.attributes {
        let type_name = if attribute.type_name.is_empty() {
            None
        } else {
            Some(
                text::block(&attribute.type_name, font, family)
                    .map_err(|err| Declined::at(attribute.line, err))?,
            )
        };
        let name = text::block(&attribute.name, font, family)
            .map_err(|err| Declined::at(attribute.line, err))?;
        let keys = if attribute.keys.is_empty() {
            None
        } else {
            let written = attribute
                .keys
                .iter()
                .map(|key| key.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let key_ink = readable(fill, entity.border.unwrap_or(diagram.look.border));
            let key_font = Font::new(diagram.look.font_size, key_ink);
            Some(
                text::block(&written, key_font, family)
                    .map_err(|err| Declined::at(attribute.line, err))?,
            )
        };
        let comment = attribute
            .comment
            .as_ref()
            .map(|comment| {
                text::block(comment, font, family).map_err(|err| Declined::at(attribute.line, err))
            })
            .transpose()?;
        let type_w = type_name
            .as_ref()
            .map(|block| block.width + 8.0)
            .unwrap_or(0.0);
        let key_w = keys.as_ref().map(|block| block.width + 12.0).unwrap_or(0.0);
        let comment_w = comment.as_ref().map(|block| block.width).unwrap_or(0.0);
        width = width.max(type_w + name.width + key_w).max(comment_w);
        let height = name
            .height
            .max(type_name.as_ref().map(|block| block.height).unwrap_or(0.0))
            .max(keys.as_ref().map(|block| block.height).unwrap_or(0.0))
            + comment
                .as_ref()
                .map(|block| block.height + 2.0)
                .unwrap_or(0.0);
        rows.push(Row {
            type_name,
            name,
            keys,
            comment,
            height,
        });
    }
    width += 2.0 * PAD;
    let mut height = title.height + 2.0 * PAD;
    if !rows.is_empty() {
        height += 6.0;
        for row in &rows {
            height += row.height + 4.0;
        }
        height += 4.0;
    }
    Ok(Measured {
        title,
        rows,
        width,
        height,
    })
}

fn place(
    diagram: &Diagram,
    measured: &[Measured],
    rank_gap: f32,
    stack_gap: f32,
    loops: &[f32],
) -> Vec<Rect> {
    let n = measured.len();
    let ranks = ranks(n, &diagram.relations);
    let max_rank = ranks.iter().copied().max().unwrap_or(0);
    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (index, rank) in ranks.into_iter().enumerate() {
        columns[rank].push(index);
    }
    let mut along = 0.0_f32;
    let mut across_at = vec![0.0_f32; n];
    let mut along_at = vec![0.0_f32; n];
    let mut column_across = Vec::new();
    for column in &columns {
        let span = column_span(column, measured, stack_gap, diagram.axis);
        column_across.push(span);
    }
    let max_across = column_across.iter().copied().fold(0.0_f32, f32::max);
    for (column, span) in columns.iter().zip(column_across) {
        let mut cursor = (max_across - span) / 2.0;
        let mut col_along = 0.0_f32;
        for &index in column {
            let (box_along, box_across) = extents(&measured[index], diagram.axis);
            along_at[index] = along;
            across_at[index] = cursor;
            cursor += box_across + stack_gap;
            col_along = col_along.max(box_along + loops[index]);
        }
        along += col_along + rank_gap;
    }
    if !columns.is_empty() {
        along -= rank_gap;
    }
    let mut rects = Vec::with_capacity(n);
    for index in 0..n {
        let (box_along, box_across) = extents(&measured[index], diagram.axis);
        let (mut x, mut y, w, h) = match diagram.axis {
            Axis::Right => (along_at[index], across_at[index], box_along, box_across),
            Axis::Down => (across_at[index], along_at[index], box_across, box_along),
        };
        if diagram.flip {
            match diagram.axis {
                Axis::Right => x = along - (x + w),
                Axis::Down => y = along - (y + h),
            }
        }
        rects.push(Rect { x, y, w, h });
    }
    rects
}

fn column_span(column: &[usize], measured: &[Measured], stack_gap: f32, axis: Axis) -> f32 {
    if column.is_empty() {
        return 0.0;
    }
    let mut span = 0.0;
    for (nth, &index) in column.iter().enumerate() {
        let (_, across) = extents(&measured[index], axis);
        span += across;
        if nth + 1 != column.len() {
            span += stack_gap;
        }
    }
    span
}

fn extents(measured: &Measured, axis: Axis) -> (f32, f32) {
    match axis {
        Axis::Right => (measured.width, measured.height),
        Axis::Down => (measured.height, measured.width),
    }
}

fn ranks(n: usize, relations: &[Relation]) -> Vec<usize> {
    let mut rank = vec![0usize; n];
    for _ in 0..n {
        for relation in relations {
            if relation.from == relation.to || relation.from >= n || relation.to >= n {
                continue;
            }
            let next = rank[relation.from] + 1;
            if next < n && rank[relation.to] < next {
                rank[relation.to] = next;
            }
        }
    }
    rank
}

fn paint_diagram(canvas: &mut Canvas, diagram: &Diagram, laid: &Laid) -> Result<(), Declined> {
    for (index, relation) in diagram.relations.iter().enumerate() {
        paint_relation(canvas, diagram, laid, relation, index)?;
    }
    for (index, entity) in diagram.entities.iter().enumerate() {
        paint_entity(
            canvas,
            diagram,
            &laid.measured[index],
            laid.boxes[index],
            entity,
        );
    }
    if let Some(title) = &laid.title {
        let width = laid
            .boxes
            .iter()
            .map(|rect| rect.right())
            .fold(title.width, f32::max);
        canvas.text(Layer::Front, title, (width / 2.0, 0.0), Align::Center);
    }
    Ok(())
}

fn paint_entity(
    canvas: &mut Canvas,
    diagram: &Diagram,
    measured: &Measured,
    rect: Rect,
    entity: &Entity,
) {
    let fill = entity.fill.unwrap_or(diagram.look.fill);
    let border = entity.border.unwrap_or(diagram.look.border);
    let mut stroke = Stroke::new(border, entity.thickness.unwrap_or(diagram.look.thickness));
    stroke.dash = entity.dash;
    canvas.rect(
        Layer::Front,
        (rect.x, rect.y, rect.w, rect.h),
        ROUND,
        Some(fill),
        Some(stroke),
    );
    let title_x = rect.x + rect.w / 2.0;
    let title_y = rect.y + PAD;
    canvas.text(
        Layer::Front,
        &measured.title,
        (title_x, title_y),
        Align::Center,
    );
    if measured.rows.is_empty() {
        return;
    }
    let line_y = title_y + measured.title.height + 4.0;
    canvas.line(
        Layer::Front,
        (rect.x + PAD, line_y),
        (rect.right() - PAD, line_y),
        Stroke::new(border, 1.0),
    );
    let mut y = line_y + 6.0;
    for row in &measured.rows {
        let mut x = rect.x + PAD;
        if let Some(type_name) = &row.type_name {
            canvas.text(Layer::Front, type_name, (x, y), Align::Left);
            x += type_name.width + 8.0;
        }
        canvas.text(Layer::Front, &row.name, (x, y), Align::Left);
        if let Some(keys) = &row.keys {
            canvas.text(Layer::Front, keys, (rect.right() - PAD, y), Align::Right);
        }
        y += row.name.height;
        if let Some(comment) = &row.comment {
            y += 2.0;
            canvas.text(Layer::Front, comment, (rect.x + PAD, y), Align::Left);
            y += comment.height;
        }
        y += 4.0;
    }
}

fn paint_relation(
    canvas: &mut Canvas,
    diagram: &Diagram,
    laid: &Laid,
    relation: &Relation,
    index: usize,
) -> Result<(), Declined> {
    let from = laid.boxes[relation.from];
    let to = laid.boxes[relation.to];
    let mut stroke = Stroke::new(diagram.look.line, 1.5);
    if relation.dashed {
        stroke.dash = Some((4.0, 3.0));
    }
    let label = relation_label(diagram, relation, canvas.family())?;
    if relation.from == relation.to {
        let slot = diagram.relations[..index]
            .iter()
            .filter(|other| other.from == relation.from && other.to == relation.to)
            .count();
        paint_loop(
            canvas,
            diagram,
            relation,
            from,
            stroke,
            slot,
            label.as_ref(),
        );
        return Ok(());
    }
    let (from_slot, from_slots) = side_slot(diagram, laid, relation.from, relation.to, index);
    let (to_slot, to_slots) = side_slot(diagram, laid, relation.to, relation.from, index);
    let mut points = route(from, to, from_slot, from_slots, to_slot, to_slots);
    let start = points[0];
    let end = *points.last().unwrap_or(&start);
    shorten(&mut points, foot_len(relation.to_card));
    shorten_first(&mut points, foot_len(relation.from_card));
    for pair in points.windows(2) {
        canvas.line(Layer::Lifelines, pair[0], pair[1], stroke);
    }
    paint_foot(canvas, end, outward(to, end), relation.to_card, stroke);
    paint_foot(
        canvas,
        start,
        outward(from, start),
        relation.from_card,
        stroke,
    );
    if let Some(block) = label {
        let along_x = (from.cx() - to.cx()).abs() >= (from.cy() - to.cy()).abs();
        let at = label_anchor(&points, along_x);
        paint_chip(canvas, diagram, &block, at);
    }
    Ok(())
}

/// Where this relationship sits among the ones that leave `entity` toward
/// the same side. `index` is the relationship's place in the list.
fn side_slot(
    diagram: &Diagram,
    laid: &Laid,
    entity: usize,
    other: usize,
    index: usize,
) -> (usize, usize) {
    let Some(side) = facing(&laid.boxes[entity], &laid.boxes[other]) else {
        return (0, 1);
    };
    let mut slot = 0usize;
    let mut slots = 0usize;
    for (at, relation) in diagram.relations.iter().enumerate() {
        let (here, there) = if relation.from == entity {
            (relation.from, relation.to)
        } else if relation.to == entity {
            (relation.to, relation.from)
        } else {
            continue;
        };
        if here == there {
            continue;
        }
        if facing(&laid.boxes[here], &laid.boxes[there]) != Some(side) {
            continue;
        }
        if at < index {
            slot += 1;
        }
        slots += 1;
    }
    (slot, slots.max(1))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

fn facing(from: &Rect, to: &Rect) -> Option<Side> {
    if from.x == to.x && from.y == to.y && from.w == to.w && from.h == to.h {
        return None;
    }
    if (to.cx() - from.cx()).abs() >= (to.cy() - from.cy()).abs() {
        if to.cx() >= from.cx() {
            Some(Side::Right)
        } else {
            Some(Side::Left)
        }
    } else if to.cy() >= from.cy() {
        Some(Side::Bottom)
    } else {
        Some(Side::Top)
    }
}

fn relation_label(
    diagram: &Diagram,
    relation: &Relation,
    family: Family,
) -> Result<Option<Block>, Declined> {
    let Some(label) = &relation.label else {
        return Ok(None);
    };
    let paper = diagram.look.background.unwrap_or(Color::WHITE);
    let font = Font::new(
        diagram.look.font_size,
        readable(paper, diagram.look.preferred),
    );
    text::block(label, font, family)
        .map(Some)
        .map_err(|err| Declined::at(relation.line, err))
}

fn paint_chip(canvas: &mut Canvas, diagram: &Diagram, block: &Block, at: (f32, f32)) {
    let chip = Rect {
        x: at.0 - block.width / 2.0 - 4.0,
        y: at.1 - block.height / 2.0 - 2.0,
        w: block.width + 8.0,
        h: block.height + 4.0,
    };
    canvas.rect(
        Layer::Frames,
        (chip.x, chip.y, chip.w, chip.h),
        3.0,
        diagram.look.background.or(Some(Color::WHITE)),
        None,
    );
    canvas.text(
        Layer::Front,
        block,
        (chip.x + 4.0, chip.y + 2.0),
        Align::Left,
    );
}

fn route(
    from: Rect,
    to: Rect,
    from_slot: usize,
    from_slots: usize,
    to_slot: usize,
    to_slots: usize,
) -> Vec<(f32, f32)> {
    let start = port(from, to, from_slot, from_slots);
    let end = port(to, from, to_slot, to_slots);
    if (from.cx() - to.cx()).abs() >= (from.cy() - to.cy()).abs() {
        if (start.1 - end.1).abs() < 0.5 {
            return vec![start, end];
        }
        // The long run stays on the source port. The jog sits near the
        // target, and each relationship jogs at its own x.
        let sign = if end.0 >= start.0 { 1.0 } else { -1.0 };
        let span = (end.0 - start.0).abs();
        let inset = (22.0 + to_slot as f32 * 10.0).min(span * 0.35).max(16.0);
        let jog = end.0 - sign * inset;
        vec![start, (jog, start.1), (jog, end.1), end]
    } else if (start.0 - end.0).abs() < 0.5 {
        vec![start, end]
    } else {
        let sign = if end.1 >= start.1 { 1.0 } else { -1.0 };
        let span = (end.1 - start.1).abs();
        let inset = (22.0 + to_slot as f32 * 10.0).min(span * 0.35).max(16.0);
        let jog = end.1 - sign * inset;
        vec![start, (start.0, jog), (end.0, jog), end]
    }
}

fn port(from: Rect, to: Rect, slot: usize, slots: usize) -> (f32, f32) {
    // The middle of the side, or spread through it when several lines share it.
    let t = if slots <= 1 {
        0.5
    } else {
        0.18 + 0.64 * (slot as f32 / (slots as f32 - 1.0))
    };
    if (to.cx() - from.cx()).abs() >= (to.cy() - from.cy()).abs() {
        let x = if to.cx() >= from.cx() {
            from.right()
        } else {
            from.x
        };
        (x, from.y + t * from.h)
    } else {
        let y = if to.cy() >= from.cy() {
            from.bottom()
        } else {
            from.y
        };
        (from.x + t * from.w, y)
    }
}

fn outward(rect: Rect, port: (f32, f32)) -> (f32, f32) {
    let left = (port.0 - rect.x).abs();
    let right = (port.0 - rect.right()).abs();
    let top = (port.1 - rect.y).abs();
    let bottom = (port.1 - rect.bottom()).abs();
    let nearest = left.min(right).min(top).min(bottom);
    if nearest == left {
        (-1.0, 0.0)
    } else if nearest == right {
        (1.0, 0.0)
    } else if nearest == top {
        (0.0, -1.0)
    } else {
        (0.0, 1.0)
    }
}

fn shorten(points: &mut [(f32, f32)], len: f32) {
    let Some(last) = points.len().checked_sub(1) else {
        return;
    };
    if last == 0 || len <= 0.0 {
        return;
    }
    let toward = points[last - 1];
    pull_toward(&mut points[last], toward, len);
}

fn shorten_first(points: &mut [(f32, f32)], len: f32) {
    if points.len() < 2 || len <= 0.0 {
        return;
    }
    let toward = points[1];
    pull_toward(&mut points[0], toward, len);
}

fn pull_toward(point: &mut (f32, f32), toward: (f32, f32), len: f32) {
    let dx = toward.0 - point.0;
    let dy = toward.1 - point.1;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 0.01 {
        return;
    }
    let t = (len / dist).min(0.85);
    point.0 += dx * t;
    point.1 += dy * t;
}

fn foot_len(card: Option<Card>) -> f32 {
    match card {
        None => 0.0,
        Some(Card::One) => 10.0,
        Some(Card::ZeroOne) => 14.0,
        Some(Card::Many) => 14.0,
        Some(Card::ZeroMany) => 18.0,
    }
}

fn paint_foot(
    canvas: &mut Canvas,
    port: (f32, f32),
    away: (f32, f32),
    card: Option<Card>,
    stroke: Stroke,
) {
    let Some(card) = card else {
        return;
    };
    let at = |distance: f32| (port.0 + away.0 * distance, port.1 + away.1 * distance);
    let perp = (-away.1, away.0);
    let shift =
        |point: (f32, f32), amount: f32| (point.0 + perp.0 * amount, point.1 + perp.1 * amount);
    let mut bar = |distance: f32| {
        let center = at(distance);
        canvas.line(
            Layer::Front,
            shift(center, -6.0),
            shift(center, 6.0),
            stroke,
        );
    };
    match card {
        Card::One => {
            bar(2.0);
            bar(7.0);
        }
        Card::ZeroOne => {
            bar(2.0);
            canvas.ellipse(
                Layer::Front,
                at(11.0),
                (3.5, 3.5),
                diagram_hole(canvas),
                Some(stroke),
            );
        }
        Card::Many => {
            let origin = at(14.0);
            canvas.line(Layer::Front, origin, port, stroke);
            canvas.line(Layer::Front, origin, shift(port, 7.0), stroke);
            canvas.line(Layer::Front, origin, shift(port, -7.0), stroke);
        }
        Card::ZeroMany => {
            canvas.ellipse(
                Layer::Front,
                at(16.0),
                (3.5, 3.5),
                diagram_hole(canvas),
                Some(stroke),
            );
            let origin = at(11.0);
            canvas.line(Layer::Front, origin, port, stroke);
            canvas.line(Layer::Front, origin, shift(port, 7.0), stroke);
            canvas.line(Layer::Front, origin, shift(port, -7.0), stroke);
        }
    }
}

/// The circle of a zero is a hole, so the page shows through it.
fn diagram_hole(_canvas: &Canvas) -> Paint {
    None
}

fn paint_loop(
    canvas: &mut Canvas,
    diagram: &Diagram,
    relation: &Relation,
    rect: Rect,
    stroke: Stroke,
    slot: usize,
    label: Option<&Block>,
) {
    let away = loop_outward(diagram.axis, diagram.flip);
    let out = 14.0 + slot as f32 * 10.0;
    let (start_port, end_port) = loop_ports(rect, away);
    let outer_start = (start_port.0 + away.0 * out, start_port.1 + away.1 * out);
    let outer_end = (end_port.0 + away.0 * out, end_port.1 + away.1 * out);
    canvas.line(Layer::Lifelines, outer_start, outer_end, stroke);
    let mut start = [start_port, outer_start];
    let mut end = [outer_end, end_port];
    shorten_first(&mut start, foot_len(relation.from_card));
    shorten(&mut end, foot_len(relation.to_card));
    canvas.line(Layer::Lifelines, start[0], start[1], stroke);
    canvas.line(Layer::Lifelines, end[0], end[1], stroke);
    paint_foot(canvas, start_port, away, relation.from_card, stroke);
    paint_foot(canvas, end_port, away, relation.to_card, stroke);
    if let Some(block) = label {
        let mid = (
            (outer_start.0 + outer_end.0) / 2.0,
            (outer_start.1 + outer_end.1) / 2.0,
        );
        let at = (
            mid.0 + away.0 * (block.width / 2.0 + 6.0),
            mid.1 + away.1 * (block.height / 2.0 + 6.0),
        );
        paint_chip(canvas, diagram, block, at);
    }
}

/// The side of a box where its self-loop was given room: toward the next
/// rank, which is the low side after `RL` or `BT` mirrors the picture.
fn loop_outward(axis: Axis, flip: bool) -> (f32, f32) {
    match (axis, flip) {
        (Axis::Right, false) => (1.0, 0.0),
        (Axis::Right, true) => (-1.0, 0.0),
        (Axis::Down, false) => (0.0, 1.0),
        (Axis::Down, true) => (0.0, -1.0),
    }
}

fn loop_ports(rect: Rect, away: (f32, f32)) -> ((f32, f32), (f32, f32)) {
    let across = (-away.1, away.0);
    let origin = (
        rect.cx() + away.0 * rect.w / 2.0,
        rect.cy() + away.1 * rect.h / 2.0,
    );
    let span = if away.0.abs() >= away.1.abs() {
        rect.h
    } else {
        rect.w
    };
    let delta = span * 0.15;
    (
        (origin.0 - across.0 * delta, origin.1 - across.1 * delta),
        (origin.0 + across.0 * delta, origin.1 + across.1 * delta),
    )
}

/// How far a self-loop's mark reaches past the box, not counting its label.
#[cfg(test)]
fn loop_reach(slot: usize, from: Option<Card>, to: Option<Card>) -> f32 {
    let out = 14.0 + slot as f32 * 10.0;
    let foot = |card: Option<Card>| match card {
        None => 0.0,
        Some(Card::One) => 7.0,
        Some(Card::ZeroOne) => 14.5,
        Some(Card::Many) => 14.0,
        Some(Card::ZeroMany) => 19.5,
    };
    out.max(foot(from)).max(foot(to))
}

/// The middle of the longest run along the ranks, so the label sits in the
/// gap and not on a short jog.
fn label_anchor(points: &[(f32, f32)], along_x: bool) -> (f32, f32) {
    let mut best_along = -1.0_f32;
    let mut at_along = points.first().copied().unwrap_or((0.0, 0.0));
    let mut best = -1.0_f32;
    let mut at = at_along;
    for pair in points.windows(2) {
        let dx = pair[1].0 - pair[0].0;
        let dy = pair[1].1 - pair[0].1;
        let len = dx * dx + dy * dy;
        let mid = ((pair[0].0 + pair[1].0) / 2.0, (pair[0].1 + pair[1].1) / 2.0);
        if len > best {
            best = len;
            at = mid;
        }
        let along = if along_x {
            dx.abs() >= dy.abs()
        } else {
            dy.abs() >= dx.abs()
        };
        if along && len > best_along {
            best_along = len;
            at_along = mid;
        }
    }
    if best_along > 0.0 { at_along } else { at }
}

/// The theme text when it contrasts with the fill, otherwise black or white.
fn readable(fill: Color, preferred: Color) -> Color {
    if fill.is_dark() != preferred.is_dark() {
        preferred
    } else if fill.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

#[cfg(test)]
mod tests {
    use super::{Axis, Card, Key, draw, from_mermaid, mermaid_refusal};

    fn diagram(source: &str) -> super::Diagram {
        from_mermaid(source).unwrap_or_else(|| panic!("refused {source}"))
    }

    #[test]
    fn reads_the_fixture_and_draws_the_foot() {
        let diagram = diagram(include_str!("../../omaspec-core/tests/fixtures/er.mmd"));
        assert_eq!(diagram.entities.len(), 2);
        assert_eq!(diagram.entities[0].key, "PACKAGE");
        assert_eq!(diagram.entities[0].attributes.len(), 2);
        assert_eq!(diagram.entities[0].attributes[0].name, "name");
        assert_eq!(diagram.entities[1].key, "FILE");
        assert_eq!(diagram.relations.len(), 1);
        assert_eq!(diagram.relations[0].from_card, Some(Card::One));
        assert_eq!(diagram.relations[0].to_card, Some(Card::ZeroMany));
        assert_eq!(diagram.relations[0].label.as_deref(), Some("contains"));
        let short = from_mermaid("erDiagram\n  A |--o B : has\n").unwrap();
        assert_eq!(short.relations[0].from_card, Some(Card::One));
        assert_eq!(short.relations[0].to_card, Some(Card::ZeroOne));
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("PACKAGE")
                && svg.contains("FILE")
                && svg.contains("contains")
                && svg.contains("name")
                && svg.contains("version")
                && svg.contains("path")
                && svg.contains("<ellipse"),
            "{svg}"
        );
        let laid = super::lay(&diagram, crate::text::Family::Sans).unwrap();
        let a = laid.boxes[0];
        let b = laid.boxes[1];
        let font = crate::text::Font::new(diagram.look.font_size, crate::Color::BLACK);
        let label = crate::text::block("contains", font, crate::text::Family::Sans).unwrap();
        let gap = if a.x < b.x {
            b.x - a.right()
        } else {
            a.x - b.right()
        };
        assert!(
            gap >= label.width + 8.0,
            "label does not fit between the boxes: gap {gap}, {a:?} {b:?}"
        );
    }

    #[test]
    fn a_self_loop_stays_off_the_neighbor() {
        let check = |source: &str| {
            let diagram = diagram(source);
            let laid = super::lay(&diagram, crate::text::Family::Sans).unwrap();
            let index = |key: &str| {
                diagram
                    .entities
                    .iter()
                    .position(|entity| entity.key == key)
                    .unwrap()
            };
            let a = laid.boxes[index("A")];
            let b = laid.boxes[index("B")];
            let reach = super::loop_reach(0, Some(Card::One), Some(Card::ZeroMany));
            let font = crate::text::Font::new(diagram.look.font_size, crate::Color::BLACK);
            let label = crate::text::block("parent", font, crate::text::Family::Sans).unwrap();
            let clear = reach + 6.0;
            match diagram.axis {
                super::Axis::Right if !diagram.flip => {
                    assert!(a.right() + clear + label.width < b.x, "{a:?} {b:?}");
                }
                super::Axis::Right => {
                    assert!(b.right() + clear + label.width < a.x, "{a:?} {b:?}");
                }
                super::Axis::Down if !diagram.flip => {
                    assert!(a.bottom() + clear + label.height < b.y, "{a:?} {b:?}");
                }
                super::Axis::Down => {
                    assert!(b.bottom() + clear + label.height < a.y, "{a:?} {b:?}");
                }
            }
            let _ = draw(&diagram).unwrap();
        };
        check("erDiagram\n  A ||--o{ A : parent\n  A ||--|| B : owns\n");
        check("erDiagram\n  direction RL\n  A ||--o{ A : parent\n  A ||--|| B : owns\n");
        check("erDiagram\n  direction TB\n  A ||--o{ A : parent\n  A ||--|| B : owns\n");
        check("erDiagram\n  direction BT\n  A ||--o{ A : parent\n  A ||--|| B : owns\n");
    }

    #[test]
    fn parallel_lines_take_different_ports_and_bends() {
        let from = super::Rect {
            x: 0.0,
            y: 0.0,
            w: 120.0,
            h: 90.0,
        };
        let to = super::Rect {
            x: 280.0,
            y: 10.0,
            w: 100.0,
            h: 70.0,
        };
        let upper = super::port(from, to, 0, 3);
        let lower = super::port(from, to, 2, 3);
        assert!(lower.1 - upper.1 >= 28.0, "{upper:?} {lower:?}");
        let a = super::route(from, to, 0, 3, 0, 2);
        let b = super::route(from, to, 1, 3, 1, 2);
        assert!(a.len() == 4 && b.len() == 4, "{a:?} {b:?}");
        assert!((a[1].0 - b[1].0).abs() > 8.0, "{a:?} {b:?}");
        assert!((a[0].1 - b[0].1).abs() > 8.0, "{a:?} {b:?}");
    }

    #[test]
    fn a_key_a_comment_and_a_style_stay_on_the_entity() {
        let source = "\
erDiagram
    direction TB
    A ||--o| B : uses
    A {
        string id PK \"primary\"
    }
    style A fill:#112233,stroke:#e68e0d,stroke-width:4px,color:#bebebe,stroke-dasharray: 5,5
";
        let diagram = diagram(source);
        assert_eq!(diagram.axis, Axis::Down);
        assert_eq!(diagram.entities[0].attributes[0].keys, vec![Key::Pk]);
        assert_eq!(
            diagram.entities[0].attributes[0].comment.as_deref(),
            Some("primary")
        );
        assert_eq!(diagram.entities[0].fill, Some(crate::Color::hex(0x112233)));
        assert_eq!(diagram.entities[0].thickness, Some(4.0));
        assert_eq!(diagram.entities[0].dash, Some((5.0, 5.0)));
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("PK")
                && svg.contains("primary")
                && svg.contains("#112233")
                && svg.contains("stroke-width=\"4\"")
                && svg.contains("stroke-dasharray=\"5,5\""),
            "{svg}"
        );
    }

    #[test]
    fn a_loop_and_a_mirror_still_draw() {
        let diagram =
            diagram("erDiagram\n  direction RL\n  A ||--o{ A : parent\n  A ||--|| B : owns\n");
        assert!(diagram.flip);
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("parent") && svg.contains("owns") && svg.contains("A"),
            "{svg}"
        );
    }

    #[test]
    fn a_bad_cardinality_a_style_and_an_open_entity_are_named() {
        assert_eq!(
            mermaid_refusal("erDiagram\n  A xx--o{ B : no\n"),
            Some((1, "a relationship"))
        );
        assert_eq!(
            mermaid_refusal("erDiagram\n  A {\n    string id\n"),
            Some((1, "an entity"))
        );
        assert_eq!(
            mermaid_refusal("erDiagram\n  style A opacity:1\n"),
            Some((1, "a style"))
        );
        assert_eq!(
            mermaid_refusal("erDiagram\n  direction sideways\n"),
            Some((1, "a direction"))
        );
        assert!(from_mermaid("sequenceDiagram\nA->>B: hi\n").is_none());
        assert_eq!(mermaid_refusal("sequenceDiagram\nA->>B: hi\n"), None);
    }
}
