//! Reading a Mermaid entity relationship diagram: its entities with their
//! attributes and keys, the relations with their cardinalities, styles.

use super::{Attribute, Axis, Card, Diagram, Entity, Key, Look, Relation};
use crate::color::{self, Color};
use crate::common::{Text, keyword, text_at};
use std::collections::HashMap;

pub(super) struct Reason {
    pub(super) line: usize,
    pub(super) what: &'static str,
}

pub(super) enum Stop {
    Foreign,
    At(Reason),
}

pub(super) fn read(source: &str) -> Result<Diagram, Stop> {
    let mut title = None;
    let mut lines = Vec::new();
    let mut iter = source.lines().enumerate().peekable();
    if iter.peek().is_some_and(|(_, raw)| raw.trim() == "---") {
        iter.next();
        let mut kept = crate::source::Front::default();
        for (no, raw) in iter.by_ref() {
            let text = raw.trim();
            if text == "---" {
                break;
            }
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            if kept.drawn(raw) {
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

pub(super) fn bare(raw: &str) -> Option<&str> {
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

pub(super) struct Chart {
    pub(super) title: Option<Text>,
    pub(super) axis: Axis,
    pub(super) flip: bool,
    pub(super) titled: bool,
    pub(super) entities: Vec<Entity>,
    pub(super) index: HashMap<String, usize>,
    pub(super) relations: Vec<Relation>,
    pub(super) open: Option<usize>,
}

impl Chart {
    pub(super) fn new(title: Option<Text>) -> Self {
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

    pub(super) fn line(&mut self, line: usize, text: &str) -> Result<(), Reason> {
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

    pub(super) fn open_entity(&mut self, text: &str, line: usize) -> Result<(), Reason> {
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

    pub(super) fn ensure(&mut self, written: &str, line: usize) -> Result<usize, Reason> {
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

    pub(super) fn relation(&mut self, text: &str, line: usize) -> Result<(), Reason> {
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

    pub(super) fn style(&mut self, text: &str, line: usize) -> Result<(), Reason> {
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

    pub(super) fn finish(self) -> Result<Diagram, Reason> {
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

pub(super) fn direction(rest: &str) -> Option<(Axis, bool)> {
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

pub(super) fn skipped_access(text: &str) -> bool {
    let head = text.split([':', ' ', '\t']).next().unwrap_or("");
    head.eq_ignore_ascii_case("accTitle") || head.eq_ignore_ascii_case("accDescr")
}

pub(super) fn split_close(text: &str) -> (String, bool) {
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

pub(super) fn attribute(text: &str, line: usize) -> Result<Attribute, Reason> {
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

pub(super) fn take_comment(text: &str) -> Option<(&str, Option<String>)> {
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

pub(super) fn tokens(text: &str) -> (Vec<String>, Vec<Key>) {
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

pub(super) fn entity_ref(text: &str) -> Option<(String, String)> {
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

pub(super) fn quoted_or_word(text: &str) -> Option<String> {
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

pub(super) fn name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || c == '.'
}

pub(super) fn split_card_left(text: &str) -> Option<(&str, Option<Card>)> {
    let text = text.trim();
    let chars: Vec<char> = text.chars().collect();
    if let Some(token) = card_tail(&chars) {
        let name = text[..text.len() - token.len()].trim();
        return Some((name, Some(card(&token)?)));
    }
    Some((text, None))
}

pub(super) fn split_card_right(text: &str) -> Option<(Option<Card>, &str)> {
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
pub(super) fn card_tail(chars: &[char]) -> Option<String> {
    let n = chars.len();
    if n >= 2 && card_char(chars[n - 2]) && card_char(chars[n - 1]) {
        return Some(chars[n - 2..].iter().collect());
    }
    chars
        .last()
        .filter(|c| card_char(**c))
        .map(|c| c.to_string())
}

pub(super) fn card_head(chars: &[char]) -> Option<String> {
    if chars.len() >= 2 && card_char(chars[0]) && card_char(chars[1]) {
        return Some(chars[..2].iter().collect());
    }
    chars
        .first()
        .filter(|c| card_char(**c))
        .map(|c| c.to_string())
}

pub(super) fn card_char(c: char) -> bool {
    matches!(c, '|' | 'o' | '{' | '}')
}

pub(super) fn card(token: &str) -> Option<Card> {
    match token {
        "||" | "|" => Some(Card::One),
        "|o" | "o|" | "o" => Some(Card::ZeroOne),
        "}|" | "|{" => Some(Card::Many),
        "}o" | "o{" | "}" | "{" => Some(Card::ZeroMany),
        _ => None,
    }
}

pub(super) struct PaintStyle {
    pub(super) fill: Option<Color>,
    pub(super) border: Option<Color>,
    pub(super) ink: Option<Color>,
    pub(super) thickness: Option<f32>,
    pub(super) dash: Option<(f32, f32)>,
}

pub(super) fn paint_style(text: &str, line: usize) -> Result<PaintStyle, Reason> {
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

pub(super) fn solid_color(value: &str, line: usize) -> Result<Color, Reason> {
    match color::parse(value) {
        Ok(Some(color)) => Ok(color),
        _ => Err(Reason {
            line,
            what: "a color",
        }),
    }
}

pub(super) fn px(value: &str) -> Option<f32> {
    let value = value.trim();
    let value = value
        .strip_suffix("px")
        .or_else(|| value.strip_suffix("PX"))
        .unwrap_or(value)
        .trim();
    let n: f32 = value.parse().ok()?;
    (n.is_finite() && (0.0..=20.0).contains(&n)).then_some(n)
}

pub(super) fn dash_of(value: &str) -> Option<(f32, f32)> {
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

pub(super) fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(text)
        .to_owned()
}
