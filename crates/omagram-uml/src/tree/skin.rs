//! How a mind map or a WBS looks: PlantUML's default style for them,
//! changed by skinparams. PlantUML draws these diagrams from styles only,
//! so the skinparams that reach them are those it turns into the styles
//! of the whole document, of every element, of arrows and of `node`s.
//! A later skinparam wins over an earlier one, as there. One that would
//! change the diagram but is not drawn yet declines it; the rest belong
//! to other diagram types and are ignored.

use super::Skinparam;
use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::normalize_key;
use crate::svg::{Align, Stroke};
use crate::text::Font;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    MindMap,
    Wbs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub font_name: String,
    pub monochrome: bool,
    pub background: Paint,
    pub fill: Paint,
    pub border: Color,
    pub thickness: f32,
    /// Corner radius of a box; PlantUML's `RoundCorner` is twice it.
    pub round: f32,
    pub font: Font,
    /// How the lines of a label line up in their box.
    pub align: Align,
    /// Between a box and its text.
    pub padding: f32,
    /// Around a box: between neighbors in a mind map, above each box of
    /// a WBS column.
    pub margin: f32,
    pub link: Stroke,
    pub title_font: Font,
    pub header_font: Font,
    pub footer_font: Font,
    pub caption_font: Font,
}

impl Skin {
    fn defaults(kind: TreeKind) -> Skin {
        let black = Color::BLACK;
        let dark = Color::hex(0x181818);
        let gray = Color::hex(0x888888);
        let (size, round, margin, link) = match kind {
            TreeKind::MindMap => (14.0, 25.0, 10.0, 1.0),
            TreeKind::Wbs => (12.0, 0.0, 15.0, 1.5),
        };
        Skin {
            font_name: "sans-serif".into(),
            monochrome: false,
            background: None,
            fill: Some(Color::hex(0xF1F1F1)),
            border: dark,
            thickness: 1.5,
            round: round / 2.0,
            font: Font::new(size, black),
            align: Align::Left,
            padding: 10.0,
            margin,
            link: Stroke::new(dark, link),
            title_font: Font::new(14.0, black).bold(),
            header_font: Font::new(10.0, gray),
            footer_font: Font::new(10.0, gray),
            caption_font: Font::new(14.0, black),
        }
    }

    pub fn from_params(params: &[Skinparam], kind: TreeKind) -> Result<Skin, Declined> {
        let mut skin = Skin::defaults(kind);
        for p in params {
            let key = normalize_key(&p.key);
            let value = p.value.trim();
            let style = |font: &mut Font| -> Result<(), Declined> {
                let v = value.to_ascii_lowercase();
                let words: Vec<&str> = v.split_whitespace().collect();
                if words.is_empty()
                    || words
                        .iter()
                        .any(|w| !matches!(*w, "plain" | "bold" | "italic"))
                {
                    return Err(p.unsupported());
                }
                font.bold = words.contains(&"bold");
                font.italic = words.contains(&"italic");
                Ok(())
            };
            match key.as_str() {
                "monochrome" => skin.monochrome = p.flag()?,
                "shadowing" | "handwritten" | "nodeshadowing" => {
                    if p.flag()? {
                        return Err(p.unsupported());
                    }
                }
                "style" => {
                    if !value.eq_ignore_ascii_case("strictuml") {
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
                "defaultfontsize" | "nodefontsize" => skin.font.size = p.number(4.0, 100.0)?,
                "defaultfontcolor" => {
                    let c = p.solid()?;
                    for font in [
                        &mut skin.font,
                        &mut skin.title_font,
                        &mut skin.header_font,
                        &mut skin.footer_font,
                        &mut skin.caption_font,
                    ] {
                        font.color = c;
                    }
                }
                "nodefontcolor" => skin.font.color = p.solid()?,
                "nodefontstyle" => style(&mut skin.font)?,
                "defaulttextalignment" => {
                    skin.align = match value.to_ascii_lowercase().as_str() {
                        "left" => Align::Left,
                        "center" => Align::Center,
                        "right" => Align::Right,
                        _ => return Err(p.unsupported()),
                    };
                }
                "roundcorner" | "noderoundcorner" => skin.round = p.corner()?,
                "backgroundcolor" => skin.background = p.paint()?,
                "nodebackgroundcolor" => skin.fill = p.paint()?,
                "nodebordercolor" => skin.border = p.solid()?,
                "nodeborderthickness" => skin.thickness = p.number(0.0, 20.0)?,
                "arrowcolor" => skin.link.color = p.solid()?,
                "arrowthickness" => skin.link.width = p.number(0.1, 20.0)?,
                "arrowstyle" => {
                    skin.link.dash = match value.to_ascii_lowercase().as_str() {
                        "dashed" => Some((7.0, 7.0)),
                        "dotted" => Some((1.0, 3.0)),
                        _ => return Err(p.unsupported()),
                    };
                }
                "titlefontsize" => skin.title_font.size = p.number(4.0, 100.0)?,
                "titlefontcolor" => skin.title_font.color = p.solid()?,
                "titlefontstyle" => style(&mut skin.title_font)?,
                "headerfontsize" => skin.header_font.size = p.number(4.0, 100.0)?,
                "headerfontcolor" => skin.header_font.color = p.solid()?,
                "headerfontstyle" => style(&mut skin.header_font)?,
                "footerfontsize" => skin.footer_font.size = p.number(4.0, 100.0)?,
                "footerfontcolor" => skin.footer_font.color = p.solid()?,
                "footerfontstyle" => style(&mut skin.footer_font)?,
                "captionfontsize" => skin.caption_font.size = p.number(4.0, 100.0)?,
                "captionfontcolor" => skin.caption_font.color = p.solid()?,
                "captionfontstyle" => style(&mut skin.caption_font)?,
                // Reach the diagram, not drawn yet.
                "nodefontname"
                | "titlefontname"
                | "headerfontname"
                | "footerfontname"
                | "captionfontname"
                | "nodediagonalcorner"
                | "nodeborderstyle"
                | "wrapwidth"
                | "minclasswidth"
                | "titlebordercolor"
                | "titleborderthickness"
                | "titlebackgroundcolor"
                | "titleborderroundcorner" => return Err(p.unsupported()),
                _ => {}
            }
        }
        Ok(skin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skin(kind: TreeKind, lines: &[(&str, &str)]) -> Result<Skin, Declined> {
        let params: Vec<Skinparam> = lines
            .iter()
            .enumerate()
            .map(|(line, (key, value))| Skinparam {
                key: (*key).into(),
                value: (*value).into(),
                line,
                declared: None,
            })
            .collect();
        Skin::from_params(&params, kind)
    }

    #[test]
    fn defaults_follow_plantuml_styles() {
        let m = skin(TreeKind::MindMap, &[]).unwrap();
        assert_eq!(
            (m.font.size, m.round, m.margin, m.link.width),
            (14.0, 12.5, 10.0, 1.0)
        );
        let w = skin(TreeKind::Wbs, &[]).unwrap();
        assert_eq!(
            (w.font.size, w.round, w.margin, w.link.width),
            (12.0, 0.0, 15.0, 1.5)
        );
        assert_eq!(w.fill, Some(Color::hex(0xF1F1F1)));
    }

    #[test]
    fn the_later_skinparam_wins_and_other_types_are_ignored() {
        let s = skin(
            TreeKind::MindMap,
            &[
                ("NodeRoundCorner", "4"),
                ("roundCorner", "12"),
                ("defaultFontSize", "16"),
                ("Node.FontSize", "11"),
                ("ParticipantBackgroundColor", "#ff0000"),
                ("NodeBackgroundColor", "#1e1e1e"),
                ("ArrowColor", "#e68e0d"),
                ("defaultFontColor", "#eeeeee"),
                ("ArrowStyle", "dashed"),
            ],
        )
        .unwrap();
        assert_eq!((s.round, s.font.size), (6.0, 11.0));
        assert_eq!(s.fill, Some(Color::hex(0x1E1E1E)));
        assert_eq!(s.link.color, Color::hex(0xE68E0D));
        assert_eq!(s.title_font.color, Color::hex(0xEEEEEE));
        assert_eq!(s.link.dash, Some((7.0, 7.0)));
    }

    #[test]
    fn declines_what_reaches_the_diagram_but_is_not_drawn() {
        for (key, value) in [
            ("handwritten", "true"),
            ("NodeFontName", "Courier"),
            ("wrapWidth", "100"),
            ("ArrowStyle", "bold"),
            ("NodeBackgroundColor", "#red/blue"),
        ] {
            let err = skin(TreeKind::Wbs, &[("roundCorner", "3"), (key, value)]).unwrap_err();
            assert_eq!(err.line, Some(1), "{key}");
        }
    }
}
