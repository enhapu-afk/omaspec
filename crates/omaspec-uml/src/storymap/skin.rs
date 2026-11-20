//! How a story map looks. Its cards take the colors of PlantUML's own
//! elements, so a theme pack colors it as it colors any diagram: a step
//! is a `rectangle`, a story a `note`, and an activity is filled with the
//! rectangle's border, the theme's accent. A skinparam that would change
//! the map but is not drawn declines; those of other diagram types are
//! ignored, so a theme pack can name them all.

use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::{Skinparam, normalize_key};
use crate::flow::skin::{NO_EFFECT, OTHER};
use crate::text::Font;

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub font_name: String,
    pub monochrome: bool,
    /// `skinparam handwritten true`: shapes drawn by hand (docs/adr/0023).
    pub sketch: bool,
    /// `skinparam shadowing true`: the cards cast a shadow.
    pub shadow: bool,
    pub background: Paint,
    /// `roundCorner`, already halved.
    pub round: f32,
    /// The words of a story, and the ink on the page.
    pub label: Font,
    /// An activity's fill: the rectangle's border.
    pub accent: Color,
    /// A step's fill and border.
    pub card: Color,
    pub card_border: Color,
    /// A story's fill and border.
    pub note: Color,
    pub note_border: Color,
    pub title: Font,
    pub header: Font,
    pub footer: Font,
    pub caption: Font,
}

impl Default for Skin {
    fn default() -> Self {
        let black = Color::BLACK;
        let gray = Color::hex(0x888888);
        let line = Color::hex(0x181818);
        Skin {
            font_name: crate::FACE.into(),
            monochrome: false,
            sketch: false,
            shadow: false,
            background: None,
            round: 4.0,
            label: Font::new(13.0, black),
            accent: line,
            card: Color::hex(0xF1F1F1),
            card_border: line,
            note: Color::hex(0xFEFFDD),
            note_border: line,
            title: Font::new(14.0, black).bold(),
            header: Font::new(10.0, gray),
            footer: Font::new(10.0, gray),
            caption: Font::new(14.0, black),
        }
    }
}

/// Which fonts the file set itself, so the default font leaves them.
#[derive(Default)]
struct Owned {
    title: bool,
    header: bool,
    footer: bool,
    caption: bool,
}

impl Skin {
    pub fn from_params(params: &[Skinparam]) -> Result<Skin, Declined> {
        let mut skin = Skin::default();
        let mut size = None;
        let mut color = None;
        let mut owned = Owned::default();
        for p in params {
            let key = normalize_key(&p.key);
            let value = p.value.trim();
            match key.as_str() {
                "monochrome" => skin.monochrome = p.flag()?,
                "handwritten" => skin.sketch = p.flag()?,
                "shadowing" => skin.shadow = p.flag()?,
                "defaultfontname" => {
                    let name = value.trim_matches('"');
                    if name.is_empty() || name.contains(['<', '>', '"', '&']) {
                        return Err(p.unsupported());
                    }
                    skin.font_name = name.to_owned();
                }
                "defaultfontsize" => size = Some(p.number(4.0, 100.0)?),
                "defaultfontcolor" => color = Some(p.solid()?),
                "roundcorner" => skin.round = p.corner()?,
                "backgroundcolor" => skin.background = p.paint()?,
                "rectanglebackgroundcolor" => skin.card = p.solid()?,
                "rectanglebordercolor" => {
                    skin.card_border = p.solid()?;
                    skin.accent = skin.card_border;
                }
                "notebackgroundcolor" => skin.note = p.solid()?,
                "notebordercolor" => skin.note_border = p.solid()?,
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
                // The map has no arrows; a pack names them for every type.
                "arrowcolor" | "arrowthickness" | "arrowfontcolor" | "arrowfontsize" => {}
                _ if OTHER.iter().any(|t| key.starts_with(t))
                    || NO_EFFECT.contains(&key.as_str()) => {}
                _ => return Err(p.unsupported()),
            }
        }
        if let Some(size) = size {
            skin.label.size = size;
        }
        if let Some(color) = color {
            skin.label.color = color;
        } else if skin.background.is_some_and(Color::is_dark) {
            // Words on a dark page that set no color of their own.
            skin.label.color = Color::WHITE;
        }
        let ink = skin.label.color;
        let fonts = [
            (&mut skin.title, owned.title),
            (&mut skin.header, owned.header),
            (&mut skin.footer, owned.footer),
            (&mut skin.caption, owned.caption),
        ];
        for (font, own) in fonts {
            if own {
                continue;
            }
            if let Some(size) = size {
                font.size = size;
            }
            if color.is_some() || skin.background.is_some_and(Color::is_dark) {
                font.color = ink;
            }
        }
        Ok(skin)
    }

    /// The page under the map.
    pub fn paper(&self) -> Color {
        self.background.unwrap_or(Color::WHITE)
    }

    /// The ground of every other band: the page, a little toward the ink.
    pub fn tint(&self) -> Color {
        self.paper().mix(self.label.color, 0.045)
    }

    /// Words that read on `fill`: the ink, or the page where the ink does
    /// not, moved toward black or white, whichever reads better there,
    /// until they do.
    pub fn ink_on(&self, fill: Color) -> Color {
        let ink = self.label.color;
        let paper = self.paper();
        let base = if ink.contrast(fill) >= paper.contrast(fill) {
            ink
        } else {
            paper
        };
        if base.contrast(fill) >= 4.5 {
            return base;
        }
        let toward = if Color::BLACK.contrast(fill) >= Color::WHITE.contrast(fill) {
            Color::BLACK
        } else {
            Color::WHITE
        };
        base.legible_on(fill, toward, 4.5)
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
    fn a_theme_pack_colors_the_cards_and_other_types_are_ignored() {
        let skin = Skin::from_params(&[
            param("backgroundColor", "#16181d"),
            param("defaultFontColor", "#e3e5ea"),
            param("RectangleBackgroundColor", "#23272f"),
            param("RectangleBorderColor", "#79a6f0"),
            param("NoteBackgroundColor", "#3a3526"),
            param("ParticipantBackgroundColor", "red"),
            param("ArrowColor", "#79a6f0"),
        ])
        .unwrap();
        assert_eq!(skin.accent, Color::hex(0x79a6f0));
        assert_eq!(skin.card, Color::hex(0x23272f));
        assert_eq!(skin.note, Color::hex(0x3a3526));
        assert_eq!(skin.title.color, Color::hex(0xe3e5ea));
        // On the light accent the page's dark color reads, not the ink.
        assert!(skin.ink_on(skin.accent).contrast(skin.accent) >= 4.5);
        assert!(skin.ink_on(skin.accent).is_dark());
        // On a middle tone where white does not read, black does.
        let olive = Color(120, 130, 75);
        let on = Skin::from_params(&[
            param("backgroundColor", "#222222"),
            param("defaultFontColor", "#ffffff"),
        ])
        .unwrap()
        .ink_on(olive);
        assert!(on.contrast(olive) >= 4.5, "{on}");
        let err = Skin::from_params(&[param("FooColor", "red")]).unwrap_err();
        assert!(err.reason.contains("not supported"), "{}", err.reason);
    }
}
