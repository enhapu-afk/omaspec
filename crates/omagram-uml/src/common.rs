//! What every diagram type shares: texts that keep their line, skinparams,
//! the lines around a diagram (`title`, `header`, `footer`, `caption`,
//! `legend`, `mainframe`, `scale`, `skinparam`, `<style>`) and the small
//! helpers the parsers use.

use std::fmt;

use crate::Declined;
use crate::color::{self, Color, Paint};
use crate::source::{Line, starts_with_ci};
use crate::style::{self, Element};
use crate::svg::Align;

/// Text as written, `\n` escapes and markup still in it; lines of a
/// multi-line block joined with newlines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub value: String,
    /// The line it starts on.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skinparam {
    /// As written; a block's name and its line's name joined
    /// (`skinparam participant { BorderColor red }` gives
    /// `participantBorderColor`).
    pub key: String,
    pub value: String,
    pub line: usize,
    /// The `<style>` declaration it stands for, if it comes from one.
    pub declared: Option<Declared>,
}

impl Skinparam {
    /// The setting as written, for messages: `skinparam ArrowColor red`,
    /// or `arrow { LineColor red }` in a style.
    pub fn written(&self) -> String {
        match &self.declared {
            Some(d) => d.to_string(),
            None => format!("skinparam {} {}", self.key, self.value),
        }
    }

    /// The setting's name, for messages: `skinparam ArrowColor`, or
    /// `arrow { LineColor }` in a style.
    pub fn setting(&self) -> String {
        match &self.declared {
            Some(d) => format!("{} {{ {} }}", d.selector, d.property),
            None => format!("skinparam {}", self.key),
        }
    }

    /// Declines the diagram at this setting.
    pub fn fail(&self, reason: String) -> Declined {
        Declined::at(self.line, reason)
    }

    /// Declines the diagram: the engine does not draw this setting yet.
    pub fn unsupported(&self) -> Declined {
        self.fail(format!("`{}` is not supported yet", self.written()))
    }

    /// The value as a color; `None` for `transparent`.
    pub fn paint(&self) -> Result<Paint, Declined> {
        color::parse(self.value.trim()).map_err(|e| self.fail(e))
    }

    /// The value as a color that is not `transparent`.
    pub fn solid(&self) -> Result<Color, Declined> {
        match self.paint()? {
            Some(c) => Ok(c),
            None => Err(self.fail(format!("`{}` must not be transparent", self.setting()))),
        }
    }

    /// The value as a number from `min` to `max`.
    pub fn number(&self, min: f32, max: f32) -> Result<f32, Declined> {
        match self.value.trim().parse::<f32>() {
            Ok(v) if (min..=max).contains(&v) => Ok(v),
            _ => Err(self.fail(format!(
                "`{}` needs a number from {min} to {max}",
                self.setting()
            ))),
        }
    }

    /// The value as `true` or `false`.
    pub fn flag(&self) -> Result<bool, Declined> {
        match self.value.trim().to_ascii_lowercase().as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(self.unsupported()),
        }
    }

    /// `RoundCorner` as the radius of a corner: PlantUML's value, from 0
    /// to 200, is twice it.
    pub fn corner(&self) -> Result<f32, Declined> {
        Ok(self.number(0.0, 200.0)? / 2.0)
    }
}

/// A property in a `<style>` block, as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    /// The innermost selector around it.
    pub selector: String,
    pub property: String,
    pub value: String,
}

impl fmt::Display for Declared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {{ {} {} }}",
            self.selector, self.property, self.value
        )
    }
}

/// The lines every diagram type takes the same way.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frame {
    pub title: Option<Text>,
    pub header: Option<Text>,
    pub footer: Option<Text>,
    pub caption: Option<Text>,
    pub legend: Option<Legend>,
    /// `mainframe`: the label of a frame around the diagram.
    pub mainframe: Option<Text>,
    pub scale: Option<Scale>,
    /// `skinparam` lines and what `<style>` blocks stand for, in order.
    pub skinparams: Vec<Skinparam>,
    /// The elements a `<style>` block reaches in this diagram type;
    /// without them it declines.
    pub styled: Option<&'static [Element]>,
}

/// `legend … endlegend`: below the diagram unless `top`, aligned in its
/// width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Legend {
    pub text: Text,
    pub top: bool,
    pub align: Align,
}

/// `scale`: how the finished picture is sized.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scale {
    /// `scale 1.5`, `scale 2/3`.
    Factor(f32),
    /// `scale 800 width`, `scale 600 height`, `scale 800*600` (whichever
    /// fits); the `max` forms only shrink.
    Fit {
        width: Option<f32>,
        height: Option<f32>,
        shrink_only: bool,
    },
}

impl Scale {
    /// The factor for a picture of `width` by `height`; no side grows
    /// past 20,000 pixels.
    pub fn factor(self, width: f32, height: f32) -> f32 {
        let f = match self {
            Scale::Factor(f) => f,
            Scale::Fit {
                width: w,
                height: h,
                shrink_only,
            } => {
                let by_width = w.filter(|_| width > 0.0).map(|w| w / width);
                let by_height = h.filter(|_| height > 0.0).map(|h| h / height);
                let f = match (by_width, by_height) {
                    (Some(a), Some(b)) => a.min(b),
                    (Some(f), None) | (None, Some(f)) => f,
                    (None, None) => 1.0,
                };
                if shrink_only { f.min(1.0) } else { f }
            }
        };
        f.min(MAX_SIZE / width.max(1.0))
            .min(MAX_SIZE / height.max(1.0))
    }
}

/// A `scale` factor may be at most this; a picture this much larger is
/// already larger than anything the preview or an export shows.
const MAX_FACTOR: f32 = 10.0;

/// A `scale` size in pixels may be at most this.
const MAX_SIZE: f32 = 20_000.0;

impl Frame {
    /// A frame for a diagram type whose elements `<style>` reaches.
    pub fn styled(elements: &'static [Element]) -> Frame {
        Frame {
            styled: Some(elements),
            ..Frame::default()
        }
    }

    /// Takes `lines[at]` (and the lines its block holds) if it is one of
    /// the shared lines; the index of the next line, `None` if it is not.
    pub fn line(&mut self, lines: &[Line], at: usize) -> Result<Option<usize>, Declined> {
        let Line { no, text, .. } = lines[at];
        if style::opens(text) {
            let Some(elements) = self.styled else {
                return Err(Declined::at(no, "`<style>` is not supported yet"));
            };
            let (declarations, next) = style::block(lines, at)?;
            self.skinparams
                .extend(style::skinparams(&declarations, elements)?);
            return Ok(Some(next));
        }
        if let Some(rest) = keyword(text, "skinparam") {
            return self.skinparam(lines, at, rest).map(Some);
        }
        for (word, end) in [
            ("title", "endtitle"),
            ("header", "endheader"),
            ("footer", "endfooter"),
        ] {
            if let Some(rest) = keyword(text, word) {
                let (value, next) = if rest.is_empty() {
                    block(lines, at, word, |l| squeezed_eq(l, end))?
                } else {
                    (rest.to_owned(), at + 1)
                };
                let (slot, value) = match word {
                    "title" => (&mut self.title, value),
                    // A diagram is one page here (`newpage` declines).
                    "header" => (&mut self.header, one_page(&value)),
                    _ => (&mut self.footer, one_page(&value)),
                };
                *slot = Some(Text { value, line: no });
                return Ok(Some(next));
            }
        }
        if let Some(rest) = keyword(text, "caption") {
            if rest.is_empty() {
                return Err(Declined::at(no, "`caption` needs a text"));
            }
            self.caption = Some(text_at(rest, no));
            return Ok(Some(at + 1));
        }
        if let Some(rest) = keyword(text, "legend")
            && let Some((top, align)) = legend_place(rest, no)?
        {
            let (value, next) = block(lines, at, "legend", |l| squeezed_eq(l, "endlegend"))?;
            self.legend = Some(Legend {
                text: Text { value, line: no },
                top,
                align,
            });
            return Ok(Some(next));
        }
        if let Some(rest) = keyword(text, "mainframe") {
            if rest.is_empty() {
                return Err(Declined::at(no, "`mainframe` needs a text"));
            }
            self.mainframe = Some(text_at(rest, no));
            return Ok(Some(at + 1));
        }
        // `scale 100 as 50 pixels` is the time scale of a timing diagram,
        // and `scale -> Bob` a message.
        if let Some(rest) = keyword(text, "scale")
            && (rest.starts_with(|c: char| c.is_ascii_digit() || c == '.')
                || keyword(rest, "max").is_some())
            && !rest
                .split_whitespace()
                .any(|w| w.eq_ignore_ascii_case("as"))
        {
            self.scale = Some(scale(rest).map_err(|reason| Declined::at(no, reason))?);
            return Ok(Some(at + 1));
        }
        Ok(None)
    }

    /// `skinparam key value`, or a block `skinparam key { Sub value … }`.
    fn skinparam(&mut self, lines: &[Line], at: usize, rest: &str) -> Result<usize, Declined> {
        let no = lines[at].no;
        if let Some(prefix) = rest.strip_suffix('{') {
            let prefix = prefix.trim();
            if !prefix.is_empty() {
                skinparam_name(prefix, no, true)?;
            }
            if prefix.contains("<<") {
                return Err(stereotyped(no));
            }
            for (offset, line) in lines[at + 1..].iter().enumerate() {
                if line.text == "}" {
                    return Ok(at + offset + 2);
                }
                if line.text.contains(['{', '}']) {
                    return Err(Declined::at(
                        line.no,
                        "this skinparam block is not supported",
                    ));
                }
                let (key, value) = split_pair(line.text)
                    .ok_or_else(|| Declined::at(line.no, "a skinparam needs a value"))?;
                skinparam_name(key, line.no, true)?;
                if key.contains("<<") && !for_states(&format!("{prefix}{key}")) {
                    return Err(stereotyped(line.no));
                }
                self.skinparams.push(Skinparam {
                    key: format!("{prefix}{key}"),
                    value: value.to_owned(),
                    line: line.no,
                    declared: None,
                });
            }
            return Err(Declined::at(no, "the skinparam block has no `}`"));
        }
        let (key, value) =
            split_pair(rest).ok_or_else(|| Declined::at(no, "a skinparam needs a value"))?;
        skinparam_name(key, no, false)?;
        if rest.contains("<<") && !(key.contains("<<") && for_states(key)) {
            return Err(stereotyped(no));
        }
        self.skinparams.push(Skinparam {
            key: key.to_owned(),
            value: value.to_owned(),
            line: no,
            declared: None,
        });
        Ok(at + 1)
    }
}

/// PlantUML reads a skinparam name of word characters and dots, with one
/// `<<stereotype>>`. Inside a block the stereotype may hold `<`; on one
/// line it may not. `*`, `[`, `"` and `#` are a syntax error there. An
/// unknown name of this shape is still read, and so is a color PlantUML
/// does not know: it draws the diagram anyway.
fn skinparam_name(key: &str, no: usize, in_block: bool) -> Result<(), Declined> {
    let mut rest = skip_word_dots(key);
    if let Some(inner) = rest.strip_prefix("<<") {
        let at = if in_block {
            inner.rfind(">>")
        } else {
            inner.find(">>")
        };
        if let Some(at) = at
            && (in_block || !inner[..at].contains(['<', '>']))
            && skip_word_dots(&inner[at + 2..]).is_empty()
        {
            return Ok(());
        }
        rest = key;
    }
    if rest.is_empty() {
        return Ok(());
    }
    Err(Declined::at(no, format!("`{key}` is not a skinparam name")))
}

/// The tail of `text` after a run of letters, digits, `_` and `.`.
fn skip_word_dots(text: &str) -> &str {
    let end = text
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(text.len());
    &text[end..]
}

/// A skinparam for one stereotype (`BackgroundColor<<phase>>`): state
/// diagrams draw those of states; the others are not drawn yet.
fn for_states(key: &str) -> bool {
    normalize_key(key).starts_with("state")
}

fn stereotyped(no: usize) -> Declined {
    Declined::at(no, "skinparams for stereotypes are not supported yet")
}

/// Where `legend top right` puts the legend; `None` if the line is no
/// legend at all (`legend -> Bob` is a message from a participant).
fn legend_place(rest: &str, no: usize) -> Result<Option<(bool, Align)>, Declined> {
    let words: Vec<String> = rest
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    let place = ["top", "bottom", "left", "right", "center"];
    if !words.iter().all(|w| place.contains(&w.as_str())) {
        return Ok(None);
    }
    let (vertical, horizontal) = match words.as_slice() {
        [] => (None, None),
        [one] if one == "top" || one == "bottom" => (Some(one.as_str()), None),
        [one] => (None, Some(one.as_str())),
        [v, h] if (v == "top" || v == "bottom") && !(h == "top" || h == "bottom") => {
            (Some(v.as_str()), Some(h.as_str()))
        }
        _ => {
            return Err(Declined::at(
                no,
                "a legend takes `top` or `bottom`, then `left`, `right` or `center`",
            ));
        }
    };
    let align = match horizontal {
        Some("left") => Align::Left,
        Some("right") => Align::Right,
        _ => Align::Center,
    };
    Ok(Some((vertical == Some("top"), align)))
}

/// `1.5`, `2/3`, `800 width`, `600 height`, `800*600` and the same with
/// `max` first.
fn scale(rest: &str) -> Result<Scale, String> {
    let wrong = || {
        format!(
            "`scale {rest}` is not understood; write `scale 1.5`, `scale 2/3`, \
             `scale 800 width`, `scale 600 height`, `scale 800*600` or `scale max 800 width`"
        )
    };
    let size = |text: &str| match text.trim().parse::<f32>() {
        Ok(v) if (1.0..=MAX_SIZE).contains(&v) => Ok(v),
        Ok(_) => Err(format!(
            "a `scale` size must be from 1 to {MAX_SIZE} pixels"
        )),
        Err(_) => Err(wrong()),
    };
    let words: Vec<String> = rest
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    let (shrink_only, words) = match words.split_first() {
        Some((first, more)) if first == "max" => (true, more),
        _ => (false, words.as_slice()),
    };
    match words {
        [n, unit] if unit == "width" => Ok(Scale::Fit {
            width: Some(size(n)?),
            height: None,
            shrink_only,
        }),
        [n, unit] if unit == "height" => Ok(Scale::Fit {
            width: None,
            height: Some(size(n)?),
            shrink_only,
        }),
        [both] if both.contains('*') => {
            let (w, h) = both.split_once('*').ok_or_else(wrong)?;
            Ok(Scale::Fit {
                width: Some(size(w)?),
                height: Some(size(h)?),
                shrink_only,
            })
        }
        [factor] if !shrink_only => {
            let number = |text: &str| text.trim().parse::<f32>().map_err(|_| wrong());
            let f = match factor.split_once('/') {
                Some((a, b)) => number(a)? / number(b)?,
                None => number(factor)?,
            };
            if f.is_finite() && f > 0.0 && f <= MAX_FACTOR {
                Ok(Scale::Factor(f))
            } else {
                Err(format!(
                    "a `scale` factor must be greater than 0 and at most {MAX_FACTOR}"
                ))
            }
        }
        _ => Err(wrong()),
    }
}

/// `text` without the keyword `word` (any case) at its start, if it is
/// there as a whole word; trimmed.
pub fn keyword<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    if !starts_with_ci(text, word) {
        return None;
    }
    let rest = &text[word.len()..];
    match rest.chars().next() {
        None => Some(rest),
        Some(c) if c.is_whitespace() => Some(rest.trim()),
        _ => None,
    }
}

/// Whether `text` is `word` with any spaces in it, in any case.
pub fn squeezed_eq(text: &str, word: &str) -> bool {
    text.split_whitespace()
        .collect::<String>()
        .eq_ignore_ascii_case(word)
}

/// A header's or footer's text with its page numbers filled in, as
/// `Page %page% of %lastpage%` reads on the one page there is.
fn one_page(text: &str) -> String {
    text.replace("%page%", "1").replace("%lastpage%", "1")
}

/// The lines that may hold the elements of a diagram and tell which type
/// an `@startuml` diagram is: multi-line notes, legends, titles, headers,
/// footers, captions and styles are left out as a whole, and so are the
/// bodies of classes, enums, objects and skinparams, where `state : State`
/// or `START` are members. A body's first line and its `}` stay. A title
/// written on one line stays: the outline reads it. The type decision
/// leaves that line out.
pub fn deciding<'a>(lines: &[Line<'a>]) -> Vec<Line<'a>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut until: Option<fn(&str) -> bool> = None;
    let mut depth = 0isize;
    for &line in lines {
        let text = line.text;
        if depth > 0 {
            depth += brace_delta(text);
            if depth <= 0 {
                depth = 0;
                out.push(line);
            }
            continue;
        }
        if let Some(end) = until {
            if end(text) {
                until = None;
            }
            continue;
        }
        until = block_end(text);
        if until.is_some() {
            continue;
        }
        out.push(line);
        if opens_body(text) && text.ends_with('{') {
            depth = brace_delta(text).max(0);
        }
    }
    out
}

/// A title, header, footer, caption or main frame written on one line.
/// Its text is not an element. A `[*]` only there (`title Sidewa[*] --> Idle`)
/// is the title's text: PlantUML takes a class diagram, not a state diagram.
pub(crate) fn one_line_frame(text: &str) -> bool {
    ["title", "header", "footer", "caption", "mainframe"]
        .iter()
        .any(|word| keyword(text, word).is_some_and(|rest| !rest.is_empty()))
}

/// The end of the block of text `text` opens, if it opens one.
fn block_end(text: &str) -> Option<fn(&str) -> bool> {
    if starts_with_ci(text, "<style") {
        return (!text.to_ascii_lowercase().contains("</style>"))
            .then_some(|l: &str| l.to_ascii_lowercase().contains("</style>"));
    }
    if ["note", "rnote", "hnote"]
        .iter()
        .any(|word| keyword(text, word).is_some())
        && !text.contains('"')
        && !unquoted_colon(text)
    {
        return Some(|l| {
            ["endnote", "endrnote", "endhnote"]
                .iter()
                .any(|end| squeezed_eq(l, end))
        });
    }
    if keyword(text, "legend").is_some() {
        return Some(|l| squeezed_eq(l, "endlegend"));
    }
    let end: fn(&str) -> bool = match text.to_ascii_lowercase().as_str() {
        "title" => |l| squeezed_eq(l, "endtitle"),
        "header" => |l| squeezed_eq(l, "endheader"),
        "footer" => |l| squeezed_eq(l, "endfooter"),
        "caption" => |l| squeezed_eq(l, "endcaption"),
        _ => return None,
    };
    Some(end)
}

/// Whether `text` declares something whose `{ … }` holds members or
/// settings rather than diagram lines. The body opens with a `{` at the
/// end of the line.
fn opens_body(text: &str) -> bool {
    [
        "abstract",
        "class",
        "interface",
        "enum",
        "annotation",
        "protocol",
        "struct",
        "exception",
        "metaclass",
        "dataclass",
        "record",
        "entity",
        "object",
        "map",
        "json",
        "skinparam",
    ]
    .iter()
    .any(|word| {
        starts_with_ci(text, word)
            && text[word.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_')
    })
}

/// `{` minus `}` in `text`, outside quotes.
pub fn brace_delta(text: &str) -> isize {
    let mut n = 0isize;
    let mut quote = false;
    for c in text.chars() {
        if c == '"' {
            quote = !quote;
            continue;
        }
        if quote {
            continue;
        }
        if c == '{' {
            n += 1;
        } else if c == '}' {
            n -= 1;
        }
    }
    n
}

/// Whether `text` has a `:` outside quotes.
pub fn unquoted_colon(text: &str) -> bool {
    let mut quote = false;
    for c in text.chars() {
        if c == '"' {
            quote = !quote;
        } else if c == ':' && !quote {
            return true;
        }
    }
    false
}

pub fn text_at(value: &str, line: usize) -> Text {
    Text {
        value: value.to_owned(),
        line,
    }
}

/// The lines after `lines[at]` up to the one `is_end` accepts, joined with
/// newlines, blank lines kept, and the index after that end.
pub fn block(
    lines: &[Line],
    at: usize,
    what: &str,
    is_end: impl Fn(&str) -> bool,
) -> Result<(String, usize), Declined> {
    let mut body = Vec::new();
    for (offset, line) in lines[at + 1..].iter().enumerate() {
        body.extend(std::iter::repeat_n("", line.blank_before));
        if is_end(line.text) {
            return Ok((body.join("\n"), at + offset + 2));
        }
        body.push(line.text);
    }
    Err(Declined::at(
        lines[at].no,
        format!("`{what}` has no end line"),
    ))
}

pub fn split_pair(text: &str) -> Option<(&str, &str)> {
    let (key, value) = text.trim().split_once(char::is_whitespace)?;
    let value = value.trim();
    (!value.is_empty()).then_some((key, value))
}

/// The start of a line for a message, at most 40 characters.
pub fn snippet(text: &str) -> String {
    match text.char_indices().nth(40) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_owned(),
    }
}

/// PlantUML's skinparam key normalization: case, `_`, `.` and spaces do
/// not count, and some prefixes mean the same.
pub fn normalize_key(key: &str) -> String {
    let mut k: String = key
        .chars()
        .filter(|c| !matches!(c, '_' | '.') && !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    for (from, to) in [
        ("sequenceparticipant", "participant"),
        ("sequenceactor", "actor"),
        ("sequencearrow", "arrow"),
        ("activityarrow", "arrow"),
        ("classarrow", "arrow"),
        ("componentarrow", "arrow"),
        ("objectarrow", "arrow"),
        ("statearrow", "arrow"),
        ("usecasearrow", "arrow"),
    ] {
        if let Some(rest) = k.strip_prefix(from) {
            k = format!("{to}{rest}");
        }
    }
    if let Some(stem) = k.strip_suffix("align") {
        k = format!("{stem}alignment");
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skinparams_for_stereotypes_only_of_states() {
        let lines = |text: &str| {
            let source = format!("@startuml\n{text}\n@enduml\n");
            let (_, lines) = crate::source::body(&source).unwrap();
            let mut frame = Frame::default();
            let mut at = 0;
            while at < lines.len() {
                at = match frame.line(&lines, at) {
                    Ok(Some(next)) => next,
                    Ok(None) => at + 1,
                    Err(err) => return Err(err),
                };
            }
            Ok(frame.skinparams)
        };
        let params = lines("skinparam state {\n  BackgroundColor<<phase>> LightBlue\n}\nskinparam stateBorderColor<<phase>> DarkBlue").unwrap();
        let keys: Vec<&str> = params.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(
            keys,
            ["stateBackgroundColor<<phase>>", "stateBorderColor<<phase>>"]
        );
        // `]` inside `<< >>` is still a name; `*`, `[` and `"` are not.
        assert!(lines("skinparam NotAKey red").is_ok());
        assert!(lines("skinparam state {\n  BackgroundColor<<p]hase>> LightBlue\n}").is_ok());
        for text in [
            "skinparam Foo*Bar red",
            "skinparam Legend[H*]FontStyle italic",
            "skinparam state {\n  Backg\"roundColor<<phase>> LightBlue\n}",
            "skinparam Foo#Bar red",
        ] {
            let err = lines(text).unwrap_err();
            assert!(
                err.reason.contains("not a skinparam name"),
                "{text}: {}",
                err.reason
            );
        }
        for text in [
            "skinparam participant {\n  BackgroundColor<<x>> red\n}",
            "skinparam classBackgroundColor<<x>> red",
            "skinparam state<<x>> {\n  BackgroundColor red\n}",
        ] {
            let err = lines(text).unwrap_err();
            assert!(
                err.reason.contains("for stereotypes"),
                "{text}: {}",
                err.reason
            );
        }
    }
    use crate::source::body;

    fn frame(source: &str) -> Result<Frame, Declined> {
        let source = format!("@startuml\n{source}\n@enduml");
        let (_, lines) = body(&source).unwrap();
        let mut frame = Frame::default();
        let mut at = 0;
        while at < lines.len() {
            at = frame.line(&lines, at)?.unwrap_or(at + 1);
        }
        Ok(frame)
    }

    #[test]
    fn a_header_and_a_footer_count_the_one_page() {
        let f =
            frame("header Page Header\nfooter Page %page% of %lastpage%\ntitle %page%").unwrap();
        assert_eq!(f.header.unwrap().value, "Page Header");
        assert_eq!(f.footer.unwrap().value, "Page 1 of 1");
        assert_eq!(
            f.title.unwrap().value,
            "%page%",
            "only headers and footers count pages"
        );
    }

    #[test]
    fn legends_main_frames_and_scales() {
        let f = frame("legend top right\n  **Key**\n  a: b\nend legend\nmainframe sd Login\nscale max 800*600")
            .unwrap();
        let legend = f.legend.unwrap();
        assert_eq!(legend.text.value, "**Key**\na: b");
        assert_eq!(legend.text.line, 1);
        assert!(legend.top);
        assert_eq!(legend.align, Align::Right);
        assert_eq!(f.mainframe.unwrap().value, "sd Login");
        assert_eq!(
            f.scale,
            Some(Scale::Fit {
                width: Some(800.0),
                height: Some(600.0),
                shrink_only: true,
            })
        );

        let plain = frame("legend\nx\nendlegend").unwrap().legend.unwrap();
        assert!(!plain.top);
        assert_eq!(plain.align, Align::Center);
        assert!(
            frame("legend left\nx\nendlegend")
                .unwrap()
                .legend
                .unwrap()
                .align
                == Align::Left
        );

        for (scale, factor) in [
            ("1.5", 1.5),
            ("2/3", 2.0 / 3.0),
            ("200 width", 0.5),
            ("100 height", 0.5),
            ("200*50", 0.25),
            ("max 800 width", 1.0),
            ("max 100 width", 0.25),
        ] {
            let s = frame(&format!("scale {scale}")).unwrap().scale.unwrap();
            assert!((s.factor(400.0, 200.0) - factor).abs() < 1e-6, "{scale}");
        }
        // No side grows past 20,000 pixels.
        let big = frame("scale 10").unwrap().scale.unwrap();
        assert_eq!(big.factor(4000.0, 100.0), 5.0);

        // Lines that only start like these belong to the diagram.
        for line in [
            "legend -> Bob : hi",
            "scale -> Bob",
            "scale 100 as 50 pixels",
        ] {
            let f = frame(line).unwrap();
            assert!(f.legend.is_none() && f.scale.is_none(), "{line}");
        }
        for (source, part) in [
            ("legend right top\nx\nendlegend", "`top` or `bottom`"),
            ("legend\nx", "no end line"),
            ("mainframe", "needs a text"),
            ("scale 0", "greater than 0"),
            ("scale 11", "at most 10"),
            ("scale 1/0", "greater than 0"),
            ("scale 30000 width", "from 1 to"),
            ("scale max 2", "not understood"),
            ("scale 2 inches", "not understood"),
        ] {
            let err = frame(source).unwrap_err();
            assert_eq!(err.line, Some(1), "{source}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
    }
}
