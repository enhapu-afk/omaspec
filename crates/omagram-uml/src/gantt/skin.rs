//! How a Gantt chart looks. A skinparam that would change the bars but is
//! not drawn declines the chart; skinparams of other diagram types, and
//! the note keys the packs write, are ignored.

use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::{Skinparam, normalize_key};
use crate::svg::Stroke;
use crate::text::Font;

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub font_name: String,
    pub monochrome: bool,
    /// `style strictuml`: an open arrow head.
    pub strict: bool,
    pub background: Paint,
    /// `roundCorner`, already halved, for the bars.
    pub round: f32,
    pub label: Font,
    pub section: Font,
    pub bar: Color,
    pub bar_border: Color,
    pub section_fill: Color,
    pub grid: Color,
    /// A column a Mermaid chart skips. Only drawn when the chart has one.
    pub closed: Color,
    pub arrow: Stroke,
    pub arrow_font: Font,
    pub title: Font,
    pub header: Font,
    pub footer: Font,
    pub caption: Font,
}

impl Default for Skin {
    fn default() -> Self {
        let black = Color::BLACK;
        let dark = Color::hex(0x181818);
        let gray = Color::hex(0x888888);
        Skin {
            font_name: "sans-serif".into(),
            monochrome: false,
            strict: false,
            background: None,
            round: 0.0,
            label: Font::new(13.0, black),
            section: Font::new(13.0, black).bold(),
            bar: Color::hex(0x8EB8D8),
            bar_border: dark,
            section_fill: Color::hex(0xEEEEEE),
            grid: Color::hex(0xE6E6E6),
            closed: Color::hex(0xD0D0D0),
            arrow: Stroke::new(dark, 1.0),
            arrow_font: Font::new(12.0, black),
            title: Font::new(14.0, black).bold(),
            header: Font::new(10.0, gray),
            footer: Font::new(10.0, gray),
            caption: Font::new(14.0, black),
        }
    }
}

/// Skinparams of other diagram types, by the start of their normalized key.
/// `note` is accepted so a pack's note colors do not decline a chart that
/// draws no notes. `task` is not here: that would change the bars.
const OTHER: &[&str] = &[
    "activity",
    "actor",
    "agent",
    "analog",
    "artifact",
    "binary",
    "boundary",
    "box",
    "card",
    "circle",
    "class",
    "clock",
    "cloud",
    "collections",
    "component",
    "concise",
    "control",
    "database",
    "diamond",
    "entity",
    "file",
    "folder",
    "frame",
    "genericdisplay",
    "hexagon",
    "interface",
    "json",
    "legend",
    "lifeline",
    "map",
    "mindmap",
    "node",
    "note",
    "object",
    "package",
    "participant",
    "partition",
    "queue",
    "rectangle",
    "robust",
    "sequence",
    "stack",
    "state",
    "storage",
    "swimlane",
    "timing",
    "usecase",
    "wbs",
];

const NO_EFFECT: [&str; 5] = ["dpi", "nodesep", "ranksep", "svglinktarget", "linetype"];

#[derive(Default)]
struct Owned {
    label: bool,
    section: bool,
    arrow: bool,
    title: bool,
    header: bool,
    footer: bool,
    caption: bool,
}

impl Skin {
    pub fn from_params(params: &[Skinparam]) -> Result<Skin, Declined> {
        let mut skin = Skin::default();
        let mut font_size = None;
        let mut font_color = None;
        let mut owned = Owned::default();
        for p in params {
            let key = normalize_key(&p.key);
            let value = p.value.trim();
            if OTHER.iter().any(|t| key.starts_with(t)) || NO_EFFECT.contains(&key.as_str()) {
                continue;
            }
            match key.as_str() {
                "monochrome" => skin.monochrome = p.flag()?,
                "shadowing" | "handwritten" => {
                    if p.flag()? {
                        return Err(p.unsupported());
                    }
                }
                "style" => {
                    if value.eq_ignore_ascii_case("strictuml") {
                        skin.strict = true;
                    } else {
                        return Err(p.unsupported());
                    }
                }
                "defaultfontname" => {
                    let name = value.trim_matches('"');
                    if name.is_empty() || name.contains(['<', '>', '"', '&']) {
                        return Err(p.unsupported());
                    }
                    skin.font_name = name.to_owned();
                }
                "defaultfontsize" => font_size = Some(p.number(4.0, 100.0)?),
                "defaultfontcolor" => font_color = Some(p.solid()?),
                "roundcorner" => skin.round = p.corner()?,
                "backgroundcolor" => {
                    skin.background = p.paint()?;
                }
                "arrowcolor" => skin.arrow.color = p.solid()?,
                "arrowthickness" => skin.arrow.width = p.number(0.1, 20.0)?,
                "arrowfontsize" => {
                    skin.arrow_font.size = p.number(4.0, 100.0)?;
                    owned.arrow = true;
                }
                "arrowfontcolor" => {
                    skin.arrow_font.color = p.solid()?;
                    owned.arrow = true;
                }
                "titlefontsize" => {
                    skin.title.size = p.number(4.0, 100.0)?;
                    owned.title = true;
                }
                "titlefontcolor" => {
                    skin.title.color = p.solid()?;
                    owned.title = true;
                }
                "headerfontsize" => {
                    skin.header.size = p.number(4.0, 100.0)?;
                    owned.header = true;
                }
                "headerfontcolor" => {
                    skin.header.color = p.solid()?;
                    owned.header = true;
                }
                "footerfontsize" => {
                    skin.footer.size = p.number(4.0, 100.0)?;
                    owned.footer = true;
                }
                "footerfontcolor" => {
                    skin.footer.color = p.solid()?;
                    owned.footer = true;
                }
                "captionfontsize" => {
                    skin.caption.size = p.number(4.0, 100.0)?;
                    owned.caption = true;
                }
                "captionfontcolor" => {
                    skin.caption.color = p.solid()?;
                    owned.caption = true;
                }
                _ => return Err(p.unsupported()),
            }
        }
        apply_default(&mut skin, font_size, font_color, &owned);
        if let Some(background) = skin.background
            && background.is_dark()
        {
            skin.on_dark(background, font_color.unwrap_or(Color::WHITE));
        }
        Ok(skin)
    }

    /// PlantUML sets these colors only with `<style>`, and its light
    /// defaults vanish on a dark background. There they come from the
    /// background and the text instead, so a dark theme stays readable.
    fn on_dark(&mut self, background: Color, ink: Color) {
        self.section_fill = background.mix(ink, 0.1);
        self.grid = background.mix(ink, 0.14);
        self.bar_border = background.mix(ink, 0.6);
        self.closed = background.mix(ink, 0.22);
    }
}

fn apply_default(skin: &mut Skin, size: Option<f32>, color: Option<Color>, owned: &Owned) {
    let slots = [
        (&mut skin.label, owned.label),
        (&mut skin.section, owned.section),
        (&mut skin.arrow_font, owned.arrow),
        (&mut skin.title, owned.title),
        (&mut skin.header, owned.header),
        (&mut skin.footer, owned.footer),
        (&mut skin.caption, owned.caption),
    ];
    for (font, own) in slots {
        if let Some(size) = size
            && !own
        {
            font.size = size;
        }
        if let Some(c) = color
            && !own
        {
            font.color = c;
        }
    }
}
