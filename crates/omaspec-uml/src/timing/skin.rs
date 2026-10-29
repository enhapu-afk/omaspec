//! How a timing diagram looks. A skinparam that would change the picture
//! but is not drawn declines the diagram; skinparams of other diagram
//! types are ignored.

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
    /// `roundCorner`, already halved, for concise and rectangle boxes.
    pub round: f32,
    pub label: Font,
    pub state: Font,
    pub note_fill: Color,
    pub note_border: Color,
    pub note: Font,
    /// The boxes of concise and rectangle rows, and an initial state.
    pub box_fill: Color,
    pub box_border: Color,
    /// `Rectangle…`, which PlantUML's styles give its rectangle rows too.
    pub rectangle_fill: Option<Color>,
    pub rectangle_border: Option<Color>,
    /// The lines of robust, binary, clock and analog rows.
    pub line: Color,
    pub highlight: Color,
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
            label: Font::new(14.0, black),
            state: Font::new(12.0, black),
            note_fill: Color::hex(0xFEFFDD),
            note_border: dark,
            note: Font::new(12.0, black),
            box_fill: Color::hex(0xF1F1F1),
            box_border: dark,
            rectangle_fill: None,
            rectangle_border: None,
            line: dark,
            highlight: Color::hex(0xFFF3C4),
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
const OTHER: &[&str] = &[
    "activity",
    "actor",
    "agent",
    "artifact",
    "boundary",
    "box",
    "card",
    "circle",
    "class",
    "cloud",
    "collections",
    "component",
    "control",
    "database",
    "diamond",
    "entity",
    "file",
    "folder",
    "frame",
    "gantt",
    "genericdisplay",
    "hexagon",
    "interface",
    "json",
    "legend",
    "lifeline",
    "map",
    "mindmap",
    "node",
    "object",
    "package",
    "participant",
    "partition",
    "queue",
    "sequence",
    "stack",
    "state",
    "storage",
    "swimlane",
    "usecase",
    "wbs",
];

const NO_EFFECT: [&str; 5] = ["dpi", "nodesep", "ranksep", "svglinktarget", "linetype"];

#[derive(Default)]
struct Owned {
    label: bool,
    state: bool,
    note: bool,
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
                "rectanglebackgroundcolor" => skin.rectangle_fill = Some(p.solid()?),
                "rectanglebordercolor" => skin.rectangle_border = Some(p.solid()?),
                "notebackgroundcolor" => skin.note_fill = p.solid()?,
                "notebordercolor" => skin.note_border = p.solid()?,
                "notefontsize" => {
                    skin.note.size = p.number(4.0, 100.0)?;
                    owned.note = true;
                }
                "notefontcolor" => {
                    skin.note.color = p.solid()?;
                    owned.note = true;
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
        self.box_fill = background.mix(ink, 0.12);
        self.box_border = background.mix(ink, 0.6);
        self.line = ink;
        let tint = if self.arrow.color.is_dark() {
            ink
        } else {
            self.arrow.color
        };
        self.highlight = background.mix(tint, 0.25);
    }
}

fn apply_default(skin: &mut Skin, size: Option<f32>, color: Option<Color>, owned: &Owned) {
    let slots = [
        (&mut skin.label, owned.label),
        (&mut skin.state, owned.state),
        (&mut skin.note, owned.note),
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
