//! Mermaid's charts of numbers, each read from its text and drawn by a
//! module of its own: a packet's fields (`packet-beta`), a radar
//! (`radar-beta`), a treemap (`treemap-beta`). They share a [`Look`]: the
//! page, the ink and a palette the Mermaid theme gives, so a chart reads
//! in the colors of the diagrams around it.

pub mod packet;
pub mod radar;
pub mod treemap;

use crate::color::{Color, Paint};
use crate::svg::{Align, Canvas, Layer};
use crate::text::{self, Block, Family, Font, Style};

/// The colors of a chart's series, in turn: middle tones, each with a
/// luminance near 0.2, so a line of any of them reads on a light page and
/// on a dark one, and the words on a fill of them are moved to black or
/// white where they would not.
pub const PALETTE: [u32; 10] = [
    0x3E7CB1, 0xD9731A, 0x3A9A4A, 0xD64550, 0x8E5BB5, 0x2A9A93, 0xB8860B, 0xC9508A, 0x9C6B3F,
    0x7F7F7F,
];

/// How a chart looks: the Mermaid theme's colors.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    /// Mermaid's `look: handDrawn`.
    pub sketch: bool,
    pub size: f32,
    pub page: Paint,
    pub ink: Color,
    /// Axes, grids and the lines between parts.
    pub line: Color,
    /// A box's fill and its border.
    pub fill: Color,
    pub border: Color,
    /// The colors of series, slices and sections, in turn.
    pub palette: Vec<Color>,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            font_name: "sans-serif".into(),
            sketch: false,
            size: 14.0,
            page: None,
            ink: Color::hex(0x333333),
            line: Color::hex(0x333333),
            fill: Color::hex(0xECECFF),
            border: Color::hex(0x9370DB),
            palette: PALETTE.into_iter().map(Color::hex).collect(),
        }
    }
}

impl Look {
    /// The page, white where the theme leaves it open.
    pub fn ground(&self) -> Color {
        self.page.unwrap_or(Color::WHITE)
    }

    /// The `index`th color of the palette, round again past its end.
    pub fn color(&self, index: usize) -> Color {
        if self.palette.is_empty() {
            return self.fill;
        }
        self.palette[index % self.palette.len()]
    }

    /// Words that read on `ground`: the ink, or black or white.
    pub fn ink_on(&self, ground: Color) -> Color {
        if self.ink.contrast(ground) >= 4.5 {
            return self.ink;
        }
        let far = if Color::WHITE.contrast(ground) > Color::BLACK.contrast(ground) {
            Color::WHITE
        } else {
            Color::BLACK
        };
        self.ink.legible_on(ground, far, 4.5)
    }

    pub fn font(&self) -> Font {
        Font::new(self.size, self.ink)
    }
}

/// `text` as written, in `font`: a chart's words are no Creole.
pub(crate) fn words(text: &str, font: Font, family: Family) -> Block {
    text::styled(
        vec![vec![(text.to_owned(), Style::from(font))]],
        font,
        family,
    )
}

/// The title over a chart, centered on `middle`; how much room it takes.
pub(crate) fn title(canvas: &mut Canvas, title: Option<&str>, look: &Look, middle: f32) -> f32 {
    let Some(title) = title.filter(|t| !t.trim().is_empty()) else {
        return 0.0;
    };
    let font = Font::new(look.size + 4.0, look.ink).bold();
    let block = words(title, font, canvas.family());
    canvas.text(Layer::Front, &block, (middle, 0.0), Align::Center);
    block.height + 14.0
}

/// A chart's title and its lines, each with its zero-based line.
pub(crate) type Body<'a> = (Option<String>, Vec<(usize, &'a str)>);

/// The lines of a Mermaid chart after its header, without comments and
/// blank lines, each with its zero-based line in `source`; `title` and
/// the accessibility lines are taken out and the title given back.
pub(crate) fn body<'a>(source: &'a str, header: &str) -> Option<Body<'a>> {
    let mut lines = source
        .lines()
        .enumerate()
        .map(|(no, line)| (no, line.trim()))
        .filter(|(_, line)| !line.is_empty() && !line.starts_with("%%"));
    // Front matter before the header is the theme's, not the chart's.
    let mut title = None;
    let mut in_front = false;
    loop {
        let (_, line) = lines.next()?;
        if line == "---" {
            in_front = !in_front;
            continue;
        }
        if in_front {
            if let Some(t) = line.strip_prefix("title:") {
                title = Some(unquote(t.trim()).to_owned());
            }
            continue;
        }
        if line.split_whitespace().next()?.eq_ignore_ascii_case(header) {
            break;
        }
        return None;
    }
    let mut out = Vec::new();
    for (no, line) in lines {
        if let Some(t) = line
            .strip_prefix("title ")
            .or_else(|| (line == "title").then_some(""))
        {
            title = Some(unquote(t.trim()).to_owned());
        } else if line.starts_with("accTitle") || line.starts_with("accDescr") {
            continue;
        } else {
            out.push((no, line));
        }
    }
    Some((title, out))
}

/// Text without the quotes around it.
pub(crate) fn unquote(text: &str) -> &str {
    let text = text.trim();
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
}

/// A number as Mermaid writes one; `None` for anything else.
pub(crate) fn number(text: &str) -> Option<f64> {
    let value: f64 = text.trim().parse().ok()?;
    value.is_finite().then_some(value)
}

/// A value as a chart writes it: no needless decimals.
pub(crate) fn said(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded.fract() == 0.0 {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}
