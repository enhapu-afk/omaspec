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
use crate::text::{Block, Family, Font, Link};

/// Text as written, `\n` escapes and markup still in it; lines of a
/// multi-line block joined with newlines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    pub value: String,
    /// The line it starts on.
    pub line: usize,
    /// The byte on that line where it starts, where the reader keeps it:
    /// words written amid others on their line, as a multiplicity
    /// ([`crate::Written::col`]).
    pub col: Option<usize>,
}

impl Text {
    /// The words measured in `font`; an empty block where there are
    /// none.
    pub fn measure(&self, font: Font, family: Family) -> Result<Block, Declined> {
        if self.value.is_empty() {
            return Ok(Block::empty());
        }
        crate::text::block(&self.value, font, family).map_err(|err| Declined::at(self.line, err))
    }

    /// The words measured as the ones their line alone writes, where they
    /// stand on it if the reader kept that ([`Block::written_at`]): drawn,
    /// they are tied to that line.
    pub fn measure_written(&self, font: Font, family: Family) -> Result<Block, Declined> {
        self.measure(font, family)
            .map(|block| block.written_at(self.line, self.col))
    }
}

/// The line of what no line of the source writes: marks laid over a
/// diagram (a transition that went, where traces left a machine). Its
/// words are tied to no line ([`crate::svg::Canvas::tie`]).
pub const NO_LINE: usize = usize::MAX;

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

    /// The value as a fill, which may run from one color into another
    /// (`AntiqueWhite/Gold`): the fill is then the gradient's first color.
    pub fn fill(&self) -> Result<(Paint, Option<color::Gradient>), Declined> {
        color::parse_fill(self.value.trim()).map_err(|e| self.fail(e))
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
    /// Where `<style>` puts the title, the header, the footer and the
    /// caption (`HorizontalAlignment`); each its usual place when `None`.
    pub aligned: Aligned,
    pub scale: Option<Scale>,
    /// `skinparam` lines and what `<style>` blocks stand for, in order.
    pub skinparams: Vec<Skinparam>,
    /// How many pages the diagram has (`newpage`, 0 is one): a header's
    /// `%page%` is the first, a footer's the last, `%lastpage%` the count.
    pub pages: usize,
    /// The elements a `<style>` block reaches in this diagram type;
    /// without them it declines.
    pub styled: Option<&'static [Element]>,
    /// The lines the writer drew by hand, each with the line of its entry
    /// ([`crate::hand`]); only the graph types read them.
    pub by_hand: Vec<(usize, crate::hand::Line)>,
    /// The boxes the writer held in place, each with the line of its entry
    /// ([`crate::hand::Hold`]); the graph types give them to their
    /// containers.
    pub held: Vec<(usize, crate::hand::Hold)>,
    /// Entries of held boxes that name no box, or two in different
    /// containers: by line, and why.
    pub unplaced: Vec<(usize, String)>,
    /// `!pragma useVerticalIf on`: the `elseif`s of an activity diagram
    /// stacked, not in a row; the other types take no notice of it, as
    /// PlantUML does.
    pub vertical_if: bool,
    /// The font names `<style>` gives the whole diagram, from one block to
    /// the next.
    pub fonts: style::Fonts,
    /// How `title`, `header`, `footer` and `caption` read with a colon
    /// after them, as the diagram type reads them.
    pub colons: Colons,
    /// `!pragma useIntermediatePackages false`: a class named `a.b.C`
    /// stands in one package `a.b`, not in `b` inside `a`; only the class
    /// diagrams take notice of it, as PlantUML does.
    pub flat_packages: bool,
}

/// How a frame's word reads with a colon after it (`title: x`, `title :
/// x`), as each diagram type of PlantUML reads it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Colons {
    /// Both are the frame's, the colon left out: a sequence, an activity,
    /// a timing diagram.
    #[default]
    Both,
    /// `title: x` is the frame's, `title : x` a member of a class named
    /// `title`: a class diagram.
    Tight,
    /// Neither: a state named so, with a description.
    Neither,
}

/// The places a style gives the frame's texts in its width.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Aligned {
    pub title: Option<Align>,
    pub header: Option<Align>,
    pub footer: Option<Align>,
    pub caption: Option<Align>,
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
    /// The header and the footer with their page numbers: a header stands
    /// over the first page, a footer under the last.
    pub fn numbered(&self) -> (Option<Text>, Option<Text>) {
        let pages = self.pages.max(1);
        let number = |text: Option<&Text>, page: usize| {
            text.map(|t| Text {
                value: t
                    .value
                    .replace("%page%", &page.to_string())
                    .replace("%lastpage%", &pages.to_string()),
                ..t.clone()
            })
        };
        (
            number(self.header.as_ref(), 1),
            number(self.footer.as_ref(), pages),
        )
    }

    /// The words after `word` where `text` writes the frame's (`title
    /// Login`), with a colon after it as the diagram type takes one
    /// ([`Colons`]).
    fn frame_word<'a>(&self, text: &'a str, word: &str) -> Option<&'a str> {
        match keyword(text, word) {
            Some(rest) if rest.starts_with(':') => match self.colons {
                Colons::Both => Some(rest[1..].trim_start()),
                Colons::Tight | Colons::Neither => None,
            },
            Some(rest) => Some(rest),
            None if self.colons == Colons::Neither => None,
            None => colon_after(text, word),
        }
    }

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
        let next = self.read(lines, at)?;
        // One font draws everything here: a diagram whose `<style>` gives
        // its parts another font than its title declines where it has one.
        if next.is_some()
            && self.fonts.apart()
            && [
                &self.title,
                &self.header,
                &self.footer,
                &self.caption,
                &self.mainframe,
            ]
            .iter()
            .any(|text| text.is_some())
        {
            return Err(Declined::at(
                lines[at].no,
                "a font name for the diagram's parts but not for its title is not supported yet",
            ));
        }
        Ok(next)
    }

    fn read(&mut self, lines: &[Line], at: usize) -> Result<Option<usize>, Declined> {
        let Line { no, text, .. } = lines[at];
        if style::opens(text) {
            let Some(elements) = self.styled else {
                return Err(Declined::at(no, "`<style>` is not supported yet"));
            };
            let (declarations, next) = style::block(lines, at)?;
            for param in style::skinparams(&declarations, elements, &mut self.fonts)? {
                // The frame's own: where its texts stand.
                let place = match normalize_key(&param.key).as_str() {
                    // The font of the diagram's parts: here that of all.
                    "diagramfontname" => {
                        self.skinparams.push(Skinparam {
                            key: "defaultFontName".to_owned(),
                            ..param
                        });
                        continue;
                    }
                    "titlehorizontalalignment" => &mut self.aligned.title,
                    "headerhorizontalalignment" => &mut self.aligned.header,
                    "footerhorizontalalignment" => &mut self.aligned.footer,
                    "captionhorizontalalignment" => &mut self.aligned.caption,
                    _ => {
                        self.skinparams.push(param);
                        continue;
                    }
                };
                *place = Some(match param.value.trim().to_ascii_lowercase().as_str() {
                    "left" => Align::Left,
                    "right" => Align::Right,
                    "center" => Align::Center,
                    _ => {
                        return Err(Declined::at(
                            param.line,
                            "`HorizontalAlignment` is left, center or right",
                        ));
                    }
                });
            }
            return Ok(Some(next));
        }
        if let Some(rest) = keyword(text, "skinparam") {
            return self.skinparam(lines, at, rest).map(Some);
        }
        // The pragmas the preprocessor hands on (`preprocess`).
        if let Some(rest) = text.strip_prefix('!').and_then(|t| keyword(t, "pragma")) {
            let words: Vec<&str> = rest.split_whitespace().collect();
            if let [name, value] = words[..]
                && name.eq_ignore_ascii_case("useintermediatepackages")
            {
                self.flat_packages = match value.to_ascii_lowercase().as_str() {
                    "on" | "true" => false,
                    "off" | "false" => true,
                    _ => {
                        return Err(Declined::at(no, "`useIntermediatePackages` is on or off"));
                    }
                };
                return Ok(Some(at + 1));
            }
            if let [name, value] = words[..]
                && name.eq_ignore_ascii_case("useverticalif")
            {
                self.vertical_if = match value.to_ascii_lowercase().as_str() {
                    "on" | "true" => true,
                    "off" | "false" => false,
                    _ => return Err(Declined::at(no, "`useVerticalIf` is on or off")),
                };
                return Ok(Some(at + 1));
            }
        }
        for (word, end) in [
            ("title", "endtitle"),
            ("header", "endheader"),
            ("footer", "endfooter"),
        ] {
            if let Some(rest) = self.frame_word(text, word) {
                let (value, next) = if rest.is_empty() && crate::source::shows_nothing(text) {
                    (crate::source::NO_BREAK_SPACE.to_string(), at + 1)
                } else if rest.is_empty() {
                    block(lines, at, word, |l| squeezed_eq(l, end))?
                } else {
                    (rest.to_owned(), at + 1)
                };
                // Its page numbers are filled in as it is drawn
                // ([`Frame::numbered`]): `newpage` may come after it.
                let slot = match word {
                    "title" => &mut self.title,
                    "header" => &mut self.header,
                    _ => &mut self.footer,
                };
                *slot = Some(Text {
                    value,
                    line: no,
                    col: None,
                });
                return Ok(Some(next));
            }
        }
        if let Some(rest) = self.frame_word(text, "caption") {
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
                text: Text {
                    value,
                    line: no,
                    col: None,
                },
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
                let (key, value) = skin_pair(line.text)
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
            skin_pair(rest).ok_or_else(|| Declined::at(no, "a skinparam needs a value"))?;
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
/// diagrams draw those of states, and so do activities in the old syntax,
/// drawn as states (`activityBackgroundColor<< Begin >>`; an activity in
/// the new syntax declines it); the others are not drawn yet.
fn for_states(key: &str) -> bool {
    let key = normalize_key(key);
    key.starts_with("state") || key.starts_with("activity")
}

/// A skinparam's key and value, a stereotype with spaces in it held in
/// the key (`BackgroundColor<< Begin >> Olive`).
fn skin_pair(text: &str) -> Option<(&str, &str)> {
    let t = text.trim();
    if let Some(open) = t.find("<<")
        && !t[..open].contains(char::is_whitespace)
        && let Some(close) = t[open..].find(">>")
    {
        let end = open + close + 2;
        let value = t[end..].trim();
        return (!value.is_empty()).then_some((&t[..end], value));
    }
    split_pair(t)
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

/// A name as a line writes it. A tree holds each name once, whichever
/// lines write it; the rules about names (lint: a name declared twice, one
/// that appears once, a namesake in another state) read the writing, and
/// read it here, as the reader of the diagram took it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    /// The name as other lines use it: a participant's code, a state's
    /// name, with its dots.
    pub name: String,
    /// What a declaration shows instead, where it has both (`"Web app" as
    /// Web`: `Web app`); empty otherwise.
    pub shown: String,
    /// Zero-based line.
    pub line: usize,
    /// Whether the line declares the name; it uses it otherwise.
    pub declares: bool,
    /// The composite states around the line, outermost first, each with
    /// its concurrent region (0 before any `--` or `||`). Empty outside
    /// any state, and in a sequence diagram.
    pub place: Vec<(String, usize)>,
}

impl Mention {
    /// `name` used on `line`, outside any state.
    pub fn used(name: &str, line: usize) -> Self {
        Self {
            name: name.to_owned(),
            shown: String::new(),
            line,
            declares: false,
            place: Vec::new(),
        }
    }

    /// `name` declared on `line`, shown as `shown`, outside any state.
    pub fn declared(name: &str, shown: &str, line: usize) -> Self {
        Self {
            shown: if shown == name {
                String::new()
            } else {
                shown.to_owned()
            },
            declares: true,
            ..Self::used(name, line)
        }
    }

    /// The same, written inside the states of `place`.
    #[must_use]
    pub fn within(mut self, place: &[(String, usize)]) -> Self {
        self.place = place.to_vec();
        self
    }
}

/// Where zero-based line `there` lies from line `here`, as a message says
/// it: "on the line above", "6 lines above", "3 lines below". Not a line's
/// number: what is read may not begin where the file does (a look's skin
/// before it, a Markdown document around it), and a count between two
/// lines holds either way.
/// `(C,#ADD1B2) Testable`, inside `<< >>`: the spot PlantUML draws before
/// a name (one letter, and its color as written) and what is left of the
/// stereotype, each if there is one. As PlantUML reads it, the letter
/// follows the parenthesis at once, and the color is a name or six
/// hexadecimal digits, `#` before them or not (given back with it):
/// `(long) words`, `( x , orchid )`, `(S,#F70)` and `(S,#red)` are words,
/// no spot.
pub(crate) fn spotted(stereotype: &str) -> (Option<(char, Option<String>)>, Option<String>) {
    let text = stereotype.trim();
    let some = |s: &str| (!s.is_empty()).then(|| s.to_owned());
    let Some((inner, rest)) = text.strip_prefix('(').and_then(|t| t.split_once(')')) else {
        return (None, some(text));
    };
    let (letter, color) = match inner.split_once(',') {
        Some((letter, color)) => (letter.trim_end(), Some(color.trim())),
        None => (inner.trim_end(), None),
    };
    let hex = |digits: &str| digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit());
    let read = |color: &str| match color.strip_prefix('#') {
        Some(digits) => hex(digits).then(|| color.to_owned()),
        None if hex(color) => Some(format!("#{color}")),
        None => (!color.is_empty() && color.chars().all(|c| c.is_ascii_alphabetic()))
            .then(|| color.to_owned()),
    };
    let color = match color {
        Some(written) => match read(written) {
            Some(color) => Some(color),
            None => return (None, some(text)),
        },
        None => None,
    };
    let mut chars = letter.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), None) if !letter.is_whitespace() => {
            (Some((letter, color)), some(rest.trim()))
        }
        _ => (None, some(text)),
    }
}

pub fn lines_away(here: usize, there: usize) -> String {
    match (there < here, here.abs_diff(there)) {
        (_, 0) => "on the same line".to_owned(),
        (true, 1) => "on the line above".to_owned(),
        (false, 1) => "on the line below".to_owned(),
        (true, n) => format!("{n} lines above"),
        (false, n) => format!("{n} lines below"),
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
    // Inside a block, how deep in diagrams inside it.
    let mut inside = Inside::of(lines);
    for (at, &line) in lines.iter().enumerate() {
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
            if !inside.within(at, &end) && end(text) {
                until = None;
            }
            continue;
        }
        // `participant P [`: the declaration decides, its words do not.
        if description_opens(text) {
            out.push(line);
            until = Some(closes_description);
            continue;
        }
        // `usecase UC1 as "words` and the lines up to the closing quote,
        // where `--` and `==` are rules of the words.
        if keyword(text, "usecase").is_some() && text.matches('"').count() % 2 == 1 {
            out.push(line);
            until = Some(|l: &str| l.matches('"').count() % 2 == 1);
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

/// Whether `text` declares a participant whose words follow on the lines
/// up to a `]` (`participant P [`); a link (`[[…]]`) is no such bracket.
pub(crate) fn description_opens(text: &str) -> bool {
    let t = text.trim_end();
    t.ends_with('[')
        && !t.ends_with("[[")
        && DESCRIBED.iter().any(|word| keyword(t, word).is_some())
}

/// The elements whose words may stand in brackets over several lines: a
/// sequence diagram's participants, and the boxes of the structure
/// diagrams (`folder f [`, `node n [`).
const DESCRIBED: [&str; 30] = [
    "participant",
    "actor",
    "boundary",
    "control",
    "entity",
    "database",
    "collections",
    "queue",
    "action",
    "agent",
    "artifact",
    "card",
    "circle",
    "cloud",
    "component",
    "file",
    "folder",
    "frame",
    "hexagon",
    "interface",
    "label",
    "node",
    "package",
    "person",
    "process",
    "rectangle",
    "stack",
    "storage",
    "usecase",
    "port",
];

/// Whether `line` closes the words of [`description_opens`]: it ends with
/// a `]` that no `[` on it opens (`beta]` does, `alpha [x]` does not).
pub(crate) fn closes_description(line: &str) -> bool {
    let t = line.trim_end();
    t.ends_with(']') && t.matches(']').count() > t.matches('[').count()
}

/// A title, header, footer, caption or main frame written on one line.
/// Its text is not an element. A `[*]` only there (`title Sidewa[*] --> Idle`)
/// is the title's text: PlantUML takes a class diagram, not a state diagram.
pub(crate) fn one_line_frame(text: &str) -> bool {
    ["title", "header", "footer", "caption", "mainframe"]
        .iter()
        .any(|word| keyword(text, word).is_some_and(|rest| !rest.is_empty()))
        || ["title", "header", "footer", "caption"]
            .iter()
            .any(|word| colon_after(text, word).is_some())
}

/// The words after `title:`, `header:`, `footer:` or `caption:`, which
/// PlantUML reads as after the word and a space, but in a state diagram.
fn colon_after<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let head = text.get(..word.len())?;
    let rest = text[word.len()..].strip_prefix(':')?;
    head.eq_ignore_ascii_case(word).then(|| rest.trim())
}

/// The end of the block of text `text` opens, if it opens one.
fn block_end(text: &str) -> Option<fn(&str) -> bool> {
    if starts_with_ci(text, "<style") {
        return (!text.to_ascii_lowercase().contains("</style>"))
            .then_some(|l: &str| l.to_ascii_lowercase().contains("</style>"));
    }
    let own = without_members(text);
    if ["note", "rnote", "hnote"]
        .iter()
        .any(|word| keyword(text, word).is_some())
        && !own.contains('"')
        && !unquoted_colon(&own)
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
/// `text` without the members it names after `::` (`A::count`,
/// `A::"start(int ms)"`): their colons and quotes are no words of a note.
fn without_members(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("::") {
        out.push_str(&rest[..at]);
        let member = &rest[at + 2..];
        rest = match member.strip_prefix('"') {
            Some(quoted) => quoted.find('"').map_or("", |end| &quoted[end + 1..]),
            None => member.trim_start_matches(|c: char| !c.is_whitespace() && c != ':' && c != '#'),
        };
    }
    out.push_str(rest);
    out
}

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
        col: None,
    }
}

/// An element's link (`[[url]]`, `[[url{tooltip}]]`) taken out of its
/// declaration: the declaration without it, and the link. A label after
/// the address is the tooltip, as nothing in the element shows it.
pub fn take_link(input: &str, line: usize) -> Result<(String, Option<Link>), Declined> {
    let mut quote = false;
    for (i, c) in input.char_indices() {
        if c == '"' {
            quote = !quote;
        }
        if quote || !input[i..].starts_with("[[") {
            continue;
        }
        let Some(end) = input[i + 2..].find("]]") else {
            break;
        };
        let (mut link, label) = crate::text::creole_link(&input[i + 2..i + 2 + end])
            .map_err(|reason| Declined::at(line, reason))?;
        if link.tooltip.is_none() && label != link.url {
            link.tooltip = Some(label);
        }
        let rest = format!(
            "{} {}",
            input[..i].trim_end(),
            input[i + 4 + end..].trim_start()
        );
        return Ok((rest.trim().to_owned(), Some(link)));
    }
    Ok((input.to_owned(), None))
}

/// The lines after `lines[at]` up to the one `is_end` accepts, joined with
/// newlines, blank lines kept, and the index after that end. An end line
/// inside a diagram inside the block (`{{` to `}}`) is that diagram's.
pub fn block(
    lines: &[Line],
    at: usize,
    what: &str,
    is_end: impl Fn(&str) -> bool,
) -> Result<(String, usize), Declined> {
    let mut body = Vec::new();
    let rest = &lines[at + 1..];
    let mut inside = Inside::of(rest);
    for (offset, line) in rest.iter().enumerate() {
        body.extend(std::iter::repeat_n("", line.blank_before));
        if !inside.within(offset, &is_end) && is_end(line.text) {
            return Ok((body.join("\n"), at + offset + 2));
        }
        body.push(line.text);
    }
    Err(Declined::at(
        lines[at].no,
        format!("`{what}` has no end line"),
    ))
}

/// How deep the lines of a block lie in diagrams inside it
/// ([`crate::embedded`]): a line `{{` opens one where a `}}` closes it
/// before the block ends.
struct Inside<'a> {
    texts: Vec<&'a str>,
    depth: usize,
}

impl<'a> Inside<'a> {
    fn of(lines: &[Line<'a>]) -> Self {
        Inside {
            texts: lines.iter().map(|line| line.text).collect(),
            depth: 0,
        }
    }

    /// Whether the line at `at` lies inside such a diagram, its opening
    /// and closing lines too, in a block that `is_end` ends; called for
    /// each line in turn.
    fn within(&mut self, at: usize, is_end: &dyn Fn(&str) -> bool) -> bool {
        let text = self.texts[at];
        let before = self.depth;
        if crate::embedded::opens(text).is_some()
            && (self.depth > 0 || closed(&self.texts[at + 1..], is_end))
        {
            self.depth += 1;
        } else if self.depth > 0 && text.trim() == "}}" {
            self.depth -= 1;
        }
        before > 0 || self.depth > 0
    }
}

/// Whether a `}}` among `lines`, those after a line `{{`, closes it before
/// a line `is_end` takes ends the block around it. An end line there is
/// the diagram's own only after a block of its own opens: a `note` in a
/// diagram inside a note.
fn closed(lines: &[&str], is_end: &dyn Fn(&str) -> bool) -> bool {
    let mut depth = 0usize;
    let mut blocks: Vec<fn(&str) -> bool> = Vec::new();
    for &line in lines {
        if let Some(end) = blocks.last() {
            if end(line) {
                blocks.pop();
            }
            continue;
        }
        if crate::embedded::opens(line).is_some() {
            depth += 1;
        } else if line.trim() == "}}" {
            if depth == 0 {
                return true;
            }
            depth -= 1;
        } else if is_end(line) {
            return false;
        } else if let Some(end) = block_end(line) {
            blocks.push(end);
        }
    }
    false
}

/// The words of a block, as [`block`] reads them, where they start: on
/// the line after the one at `at`, which opens it.
pub fn block_text(
    lines: &[Line],
    at: usize,
    what: &str,
    is_end: impl Fn(&str) -> bool,
) -> Result<(Text, usize), Declined> {
    let (value, next) = block(lines, at, what, is_end)?;
    let line = lines.get(at + 1).map_or(lines[at].no, |line| line.no);
    Ok((
        Text {
            value,
            line,
            col: None,
        },
        next,
    ))
}

pub fn split_pair(text: &str) -> Option<(&str, &str)> {
    let (key, value) = text.trim().split_once(char::is_whitespace)?;
    let value = value.trim();
    (!value.is_empty()).then_some((key, value))
}

/// The start of a line for a message, at most 40 characters. A character
/// nobody sees is written as its code point (`Al<U+200B>ice`): it is often
/// what the message is about.
pub fn snippet(text: &str) -> String {
    let (shown, more) = match text.char_indices().nth(40) {
        Some((cut, _)) => (&text[..cut], "…"),
        None => (text, ""),
    };
    let mut out = String::with_capacity(shown.len() + more.len());
    for c in shown.chars() {
        if unseen(c).is_some() {
            out.push_str(&format!("<U+{:04X}>", c as u32));
        } else {
            out.push(c);
        }
    }
    out.push_str(more);
    out
}

/// What `c` is called, if it is a character that leaves no mark and has
/// no place in a diagram's text. The one list of them: a message writes
/// such a character as its code point, and lint's `unseen-character`
/// warns of it.
///
/// A tab and a no-break space are none (the one is a space, the other a
/// way to write a text that shows nothing), nor are the joiners and
/// direction marks that scripts and emoji need (U+200C to U+200F).
pub fn unseen(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{ad}' => "soft hyphen",
        '\u{200b}' => "zero-width space",
        '\u{2060}' => "word joiner",
        '\u{feff}' => "zero-width no-break space",
        '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {
            "a mark that turns the direction of the text"
        }
        '\t' => return None,
        c if c.is_control() => "a control character",
        _ => return None,
    })
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

    /// `title:`, `header:` and `footer:` with a colon are the frame's, as
    /// PlantUML reads them, not a member of a class named so.
    #[test]
    fn a_title_may_take_a_colon() {
        let svg =
            crate::render("@startuml\ntitle: Hello\nheader: Head\nclass Wifi\n@enduml\n").unwrap();
        assert!(
            svg.contains(">Hello</tspan>") && svg.contains(">Head</tspan>"),
            "{svg}"
        );
        assert!(!svg.contains(">title</tspan>"), "{svg}");
        assert!(crate::render("@startuml\ntitle: Hello\nA -> B\n@enduml\n").is_ok());
        // As each type of PlantUML reads it: a sequence's `title : x` is
        // its title too, a class diagram's a class named `title`, and a
        // state diagram's `title:` and `header:` are states.
        let svg = crate::render("@startuml\ntitle : Hello\nA -> B\n@enduml\n").unwrap();
        assert!(
            svg.contains(">Hello</tspan>") && !svg.contains(">: Hello"),
            "{svg}"
        );
        let svg = crate::render("@startuml\ntitle : Hello\nclass A\n@enduml\n").unwrap();
        assert!(svg.contains(">title</tspan>"), "{svg}");
        let svg = crate::render(
            "@startuml\n[*] --> A\nheader: page head\ntitle: page title\nA --> [*]\n@enduml\n",
        )
        .unwrap();
        assert!(
            svg.contains(">header</tspan>") && svg.contains(">title</tspan>"),
            "{svg}"
        );
        // A description diagram's `title : x` is its title, as a sequence's.
        let svg = crate::render("@startuml\ntitle : Hi\nactor U\nU --> (x)\n@enduml\n").unwrap();
        assert!(
            svg.contains(">Hi</tspan>") && !svg.contains(">title</tspan>"),
            "{svg}"
        );
        let svg = crate::render("@startuml\ncaption: Cap\nclass A\n@enduml\n").unwrap();
        assert!(
            svg.contains(">Cap</tspan>") && !svg.contains(">caption</tspan>"),
            "{svg}"
        );
    }

    /// A note on a member opens a block as any note does: its words are no
    /// lines that decide the diagram's type.
    #[test]
    fn a_note_on_a_member_holds_its_words() {
        for body in ["state of the counter", "start", "[*] --> x"] {
            for target in ["A::counter", "A::\"counter\""] {
                let source = format!(
                    "@startuml\nclass A {{\n  int counter\n}}\nnote right of {target}\n  {body}\nend note\n@enduml\n"
                );
                assert!(
                    matches!(crate::parse(&source), Ok(crate::Diagram::Structure(_))),
                    "{target} {body}: {:?}",
                    crate::parse(&source).err()
                );
            }
        }
    }

    /// What nobody sees is written as its code point; a tab, a no-break
    /// space and the joiner of an emoji are left as they are.
    #[test]
    fn a_snippet_names_what_nobody_sees() {
        assert_eq!(snippet("Al\u{200b}ice"), "Al<U+200B>ice");
        let kept = "a\tb\u{a0}c \u{1f468}\u{200d}\u{1f469}";
        assert_eq!(snippet(kept), kept);
    }

    #[test]
    fn an_element_s_link_comes_out_of_its_declaration() {
        let (rest, link) = take_link("Foo [[https://a.b{Tip}]] #red", 3).unwrap();
        assert_eq!(rest, "Foo #red");
        let link = link.unwrap();
        assert_eq!(link.url, "https://a.b");
        assert_eq!(link.tooltip.as_deref(), Some("Tip"));
        let (rest, link) = take_link("[Comp][[docs/c.html The component]]", 3).unwrap();
        assert_eq!(rest, "[Comp]");
        assert_eq!(link.unwrap().tooltip.as_deref(), Some("The component"));
        let (rest, link) = take_link("\"a [[b]]\" as A", 3).unwrap();
        assert_eq!((rest.as_str(), link), ("\"a [[b]]\" as A", None));
        assert!(take_link("A [[{tip}]]", 3).is_err());
    }

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
    fn a_header_and_a_footer_count_the_pages() {
        let mut f =
            frame("header Page %page%\nfooter Page %page% of %lastpage%\ntitle %page%").unwrap();
        let (header, footer) = f.numbered();
        assert_eq!(header.unwrap().value, "Page 1");
        assert_eq!(footer.unwrap().value, "Page 1 of 1");
        assert_eq!(
            f.title.as_ref().unwrap().value,
            "%page%",
            "only headers and footers count pages"
        );
        f.pages = 3;
        let (header, footer) = f.numbered();
        assert_eq!(header.unwrap().value, "Page 1");
        assert_eq!(footer.unwrap().value, "Page 3 of 3");
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
