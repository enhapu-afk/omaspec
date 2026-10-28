//! How a signal flow looks. A skinparam that would change the blocks or
//! the wires but is not drawn declines the diagram. Skinparams of other
//! diagram types are ignored, so a theme pack can name them all.
//! `rectangle` is this diagram's block, so those two colors are kept.

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
    /// `roundCorner`, already halved.
    pub round: f32,
    pub label: Font,
    pub fill: Color,
    pub border: Color,
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
        let gray = Color::hex(0x888888);
        Skin {
            font_name: "sans-serif".into(),
            monochrome: false,
            strict: false,
            background: None,
            round: 0.0,
            label: Font::new(13.0, black),
            fill: Color::WHITE,
            border: black,
            arrow: Stroke::new(black, 1.0),
            arrow_font: Font::new(12.0, black),
            title: Font::new(14.0, black).bold(),
            header: Font::new(10.0, gray),
            footer: Font::new(10.0, gray),
            caption: Font::new(14.0, black),
        }
    }
}

/// Skinparams of other diagram types, by the start of their normalized key.
/// `rectangle` is not here: it is the block. `legend` is read by the frame.
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
    arrow: bool,
    title: bool,
    header: bool,
    footer: bool,
    caption: bool,
    arrow_color: bool,
    fill: bool,
    border: bool,
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
                "backgroundcolor" => skin.background = p.paint()?,
                "arrowcolor" => {
                    skin.arrow.color = p.solid()?;
                    owned.arrow_color = true;
                }
                "arrowthickness" => skin.arrow.width = p.number(0.1, 20.0)?,
                "arrowfontsize" => {
                    skin.arrow_font.size = p.number(4.0, 100.0)?;
                    owned.arrow = true;
                }
                "arrowfontcolor" => {
                    skin.arrow_font.color = p.solid()?;
                    owned.arrow = true;
                }
                "rectanglebackgroundcolor" => {
                    skin.fill = p.solid()?;
                    owned.fill = true;
                }
                "rectanglebordercolor" => {
                    skin.border = p.solid()?;
                    owned.border = true;
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
            skin.on_dark(background, font_color.unwrap_or(Color::WHITE), &owned);
        }
        Ok(skin)
    }

    /// Wires sit on the page, not on the block. On a dark page a black
    /// arrow would vanish, so it takes the text color unless the file set
    /// one. A dark block whose border was not set gets a lighter edge. A
    /// page with no block color keeps the white cards.
    fn on_dark(&mut self, background: Color, ink: Color, owned: &Owned) {
        if !owned.arrow_color {
            self.arrow.color = ink;
        }
        if owned.fill && !owned.border && self.fill.is_dark() {
            self.border = background.mix(ink, 0.6);
        }
    }
}

fn apply_default(skin: &mut Skin, size: Option<f32>, color: Option<Color>, owned: &Owned) {
    let slots = [
        (&mut skin.label, owned.label),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn param(key: &str, value: &str) -> Skinparam {
        Skinparam {
            key: key.into(),
            value: value.into(),
            line: 0,
            declared: None,
        }
    }

    #[test]
    fn rectangle_colors_are_the_blocks_and_other_types_are_ignored() {
        let skin = Skin::from_params(&[
            param("backgroundColor", "#121212"),
            param("defaultFontColor", "white"),
            param("RectangleBackgroundColor", "#222222"),
            param("RectangleBorderColor", "#E68E0D"),
            param("ParticipantBackgroundColor", "red"),
            param("shadowing", "false"),
        ])
        .unwrap();
        assert_eq!(skin.background, Some(Color::hex(0x121212)));
        assert_eq!(skin.fill, Color::hex(0x222222));
        assert_eq!(skin.border, Color::hex(0xE68E0D));
        assert_eq!(skin.label.color, Color::WHITE);
        // The page is dark and no arrow color was written, so the wire
        // takes the text color. The participant color is not the block.
        assert_eq!(skin.arrow.color, Color::WHITE);
        assert_ne!(skin.fill, Color::hex(0xFF0000));
    }

    #[test]
    fn an_unknown_key_and_a_shadow_decline() {
        let err = Skin::from_params(&[param("FooColor", "red")]).unwrap_err();
        assert!(err.reason.contains("not supported"), "{}", err.reason);
        let err = Skin::from_params(&[param("shadowing", "true")]).unwrap_err();
        assert!(err.reason.contains("shadowing"), "{}", err.reason);
    }
}
