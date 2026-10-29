//! How a state diagram looks: PlantUML's defaults, changed by skinparams
//! and `<style>` blocks. A skinparam that would change the picture but is
//! not drawn declines the diagram; skinparams of other diagram types are
//! ignored. `state…` keys are this diagram's, so they are not
//! ignored the way an activity diagram ignores them.

use std::borrow::Cow;

use super::Skinparam;
use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::normalize_key;
use crate::style::{Element, LEGEND, Prop};
use crate::svg::Stroke;
use crate::text::Font;

/// The look of a shape: its fill, its border, its text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub fill: Paint,
    pub border: Color,
    pub thickness: f32,
    pub font: Font,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub font_name: String,
    pub monochrome: bool,
    /// `skinparam style strictuml`: an open arrow head.
    pub strict: bool,
    pub background: Paint,
    /// Corner radius of a state. PlantUML's `RoundCorner` is twice it.
    pub round: f32,
    /// States, choices and history. Start, end and bars use the border
    /// color, so a dark theme's accent stays visible on the background.
    pub state: Look,
    pub arrow: Stroke,
    /// How far a transition's corners curve; none with `linetype ortho`.
    pub curve: f32,
    pub arrow_font: Font,
    pub note: Look,
    pub title_font: Font,
    pub header_font: Font,
    pub footer_font: Font,
    pub caption_font: Font,
    /// The look of the states of a stereotype (`BackgroundColor<<phase>>`),
    /// by its name in lower case.
    pub stereotypes: Vec<(String, Look)>,
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
            round: 8.0,
            state: Look {
                fill: Some(Color::hex(0xF1F1F1)),
                border: dark,
                thickness: 1.0,
                font: Font::new(14.0, black),
            },
            arrow: Stroke::new(dark, 1.0),
            curve: crate::curve::RADIUS,
            arrow_font: Font::new(12.0, black),
            note: Look {
                fill: Some(Color::hex(0xFEFFDD)),
                border: dark,
                thickness: 0.5,
                font: Font::new(13.0, black),
            },
            title_font: Font::new(14.0, black).bold(),
            header_font: Font::new(10.0, gray),
            footer_font: Font::new(10.0, gray),
            caption_font: Font::new(14.0, black),
            stereotypes: Vec::new(),
        }
    }
}

/// Skinparams of other diagram types, by the start of their key, and of
/// the legend and the main frame, which every type draws the same way.
/// `state` is not here: those keys are this diagram's.
const OTHER_TYPES: [&str; 45] = [
    "activity",
    "actor",
    "agent",
    "artifact",
    "boundary",
    "box",
    "card",
    "class",
    "cloud",
    "collections",
    "component",
    "control",
    "database",
    "entity",
    "file",
    "folder",
    "frame",
    "genericdisplay",
    "interface",
    "json",
    "label",
    "legend",
    "lifeline",
    "mainframe",
    "map",
    "maxmessagesize",
    "mindmap",
    "node",
    "object",
    "package",
    "partition",
    "participant",
    "queue",
    "rectangle",
    "responsemessagebelowarrow",
    "sequence",
    "stack",
    "stereotype",
    "storage",
    "swimlane",
    "timing",
    "usecase",
    "wbs",
    "yaml",
    "circledcharacter",
];

/// Skinparams that change nothing in an SVG of a state diagram.
const NO_EFFECT: [&str; 5] = [
    "dpi",
    "nodesep",
    "ranksep",
    "svglinktarget",
    "minclasswidth",
];

/// Elements with a font, by their skinparam name.
const FONTS: [&str; 8] = [
    "state", "arrow", "note", "title", "header", "footer", "caption", "default",
];

impl Skin {
    pub fn from_params(params: &[Skinparam]) -> Result<Skin, Declined> {
        let mut skin = Skin::default();
        let mut font_size = None;
        let mut font_color = None;
        let mut own_size = Vec::new();
        let mut own_color = Vec::new();
        let mut stereotyped = Vec::new();
        for p in params {
            let key = normalize_key(&p.key);
            let value = p.value.trim();
            if let Some((key, stereotype)) = key.split_once("<<") {
                let stereotype = stereotype.trim_end_matches(">>").trim().to_owned();
                stereotyped.push((key.to_owned(), stereotype, p));
                continue;
            }

            if OTHER_TYPES.iter().any(|t| key.starts_with(t)) || NO_EFFECT.contains(&key.as_str()) {
                continue;
            }
            match key.as_str() {
                "monochrome" => skin.monochrome = p.flag()?,
                "shadowing" | "handwritten" | "stateshadowing" => {
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
                "roundcorner" | "stateroundcorner" => skin.round = p.corner()?,
                "backgroundcolor" => skin.background = p.paint()?,
                "arrowcolor" => skin.arrow.color = p.solid()?,
                "linetype" => skin.curve = crate::curve::linetype(p)?,
                "arrowthickness" => skin.arrow.width = p.number(0.1, 20.0)?,
                "arrowstyle" => {
                    skin.arrow.dash = match value.to_ascii_lowercase().as_str() {
                        "dashed" => Some((7.0, 7.0)),
                        "dotted" => Some((1.0, 3.0)),
                        _ => return Err(p.unsupported()),
                    };
                }
                "statebackgroundcolor" => skin.state.fill = p.paint()?,
                "statebordercolor" => skin.state.border = p.solid()?,
                "stateborderthickness" => skin.state.thickness = p.number(0.0, 20.0)?,
                "notebackgroundcolor" => skin.note.fill = p.paint()?,
                "notebordercolor" => skin.note.border = p.solid()?,
                "noteborderthickness" => skin.note.thickness = p.number(0.0, 20.0)?,
                _ => {
                    let Some((element, property)) = split_font(&key) else {
                        return Err(p.unsupported());
                    };
                    match property {
                        "fontsize" => {
                            let size = p.number(4.0, 100.0)?;
                            own_size.push(element);
                            font_of(&mut skin, element).size = size;
                        }
                        "fontcolor" => {
                            let c = p.solid()?;
                            own_color.push(element);
                            font_of(&mut skin, element).color = c;
                        }
                        "fontstyle" => {
                            let v = value.to_ascii_lowercase();
                            let words: Vec<&str> = v.split_whitespace().collect();
                            if words.is_empty()
                                || words
                                    .iter()
                                    .any(|w| !matches!(*w, "plain" | "bold" | "italic"))
                            {
                                return Err(p.unsupported());
                            }
                            let font = font_of(&mut skin, element);
                            font.bold = words.contains(&"bold");
                            font.italic = words.contains(&"italic");
                        }
                        _ => return Err(p.unsupported()),
                    }
                }
            }
        }
        for element in FONTS {
            // `default` is the fallback applied above, not a font of its own.
            // `font_of` would otherwise hand it the state font and overwrite
            // a size set on the state.
            if element == "default" {
                continue;
            }
            let font = font_of(&mut skin, element);
            if let Some(size) = font_size
                && !own_size.contains(&element)
            {
                font.size = size;
            }
            if let Some(c) = font_color
                && !own_color.contains(&element)
            {
                font.color = c;
            }
        }
        // Over the look of every state, whichever line came first.
        for (key, stereotype, p) in stereotyped {
            let at = match skin.stereotypes.iter().position(|(s, _)| *s == stereotype) {
                Some(at) => at,
                None => {
                    skin.stereotypes.push((stereotype, skin.state));
                    skin.stereotypes.len() - 1
                }
            };
            let look = &mut skin.stereotypes[at].1;
            match key.as_str() {
                "statebackgroundcolor" => look.fill = p.paint()?,
                "statebordercolor" => look.border = p.solid()?,
                "stateborderthickness" => look.thickness = p.number(0.0, 20.0)?,
                "statefontcolor" => look.font.color = p.solid()?,
                "statefontsize" => look.font.size = p.number(4.0, 100.0)?,
                _ => return Err(p.unsupported()),
            }
        }
        Ok(skin)
    }

    /// This skin for a state of `stereotype`: its look in place of every
    /// state's, if skinparams give it one. PlantUML matches the name in any
    /// case.
    pub fn stereotyped(&self, stereotype: Option<&str>) -> Cow<'_, Skin> {
        let look = stereotype.and_then(|name| {
            let name = name.to_ascii_lowercase();
            self.stereotypes
                .iter()
                .find(|(s, _)| *s == name)
                .map(|(_, look)| *look)
        });
        match look {
            Some(look) => Cow::Owned(Skin {
                state: look,
                ..self.clone()
            }),
            None => Cow::Borrowed(self),
        }
    }
}

/// A shape with a text.
const LOOK: &[Prop] = &[
    Prop::BackGroundColor,
    Prop::LineColor,
    Prop::LineThickness,
    Prop::FontColor,
    Prop::FontSize,
    Prop::FontStyle,
];

/// The elements `<style>` reaches in a state diagram, by the names
/// PlantUML looks them up with. Start, end and composite states take the
/// state's look; their own draws nothing yet.
pub const STYLE: &[Element] = &[
    Element::new(
        &["element", "stateDiagram", "state"],
        "State",
        &[
            Prop::BackGroundColor,
            Prop::LineColor,
            Prop::LineThickness,
            Prop::FontColor,
            Prop::FontSize,
            Prop::FontStyle,
            Prop::RoundCorner,
        ],
    ),
    Element::keys(&["element", "stateDiagram", "state", "group"], &[]),
    Element::keys(&["element", "stateDiagram", "circle", "start"], &[]),
    Element::keys(&["element", "stateDiagram", "circle", "stop"], &[]),
    Element::keys(&["element", "stateDiagram", "circle", "end"], &[]),
    Element::keys(
        &["element", "stateDiagram", "arrow"],
        &[
            (Prop::LineColor, "ArrowColor"),
            (Prop::LineThickness, "ArrowThickness"),
            (Prop::LineStyle, "ArrowStyle"),
            (Prop::FontColor, "ArrowFontColor"),
            (Prop::FontSize, "ArrowFontSize"),
            (Prop::FontStyle, "ArrowFontStyle"),
        ],
    ),
    Element::new(&["element", "stateDiagram", "note"], "Note", LOOK),
    Element::new(&["document", "stateDiagram", "legend"], "Legend", LEGEND),
];

/// `statefontsize` as `("state", "fontsize")`; the longest element name
/// wins.
fn split_font(key: &str) -> Option<(&'static str, &str)> {
    let mut found: Option<(&'static str, &str)> = None;
    for element in FONTS {
        if element == "default" {
            continue;
        }
        if let Some(rest) = key.strip_prefix(element)
            && found.is_none_or(|(e, _)| element.len() > e.len())
        {
            found = Some((element, rest));
        }
    }
    found
}

fn font_of<'a>(skin: &'a mut Skin, element: &str) -> &'a mut Font {
    match element {
        "arrow" => &mut skin.arrow_font,
        "note" => &mut skin.note.font,
        "title" => &mut skin.title_font,
        "header" => &mut skin.header_font,
        "footer" => &mut skin.footer_font,
        "caption" => &mut skin.caption_font,
        _ => &mut skin.state.font,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skin(lines: &[(&str, &str)]) -> Result<Skin, Declined> {
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
        Skin::from_params(&params)
    }

    #[test]
    fn a_stereotype_has_its_own_look_over_every_state() {
        let blue = Color::hex(0xADD8E6);
        let s = skin(&[
            ("stateBackgroundColor<<phase>>", "LightBlue"),
            ("stateBorderColor<<phase>>", "DarkBlue"),
            ("stateFontColor<<Error>>", "Red"),
            // A line for every state after one for a stereotype: the
            // stereotype still wins, and takes what it does not set.
            ("stateBackgroundColor", "#EEEEEE"),
            ("stateFontSize", "10"),
        ])
        .unwrap();
        let phase = s.stereotyped(Some("Phase")).state;
        assert_eq!(phase.fill, Some(blue));
        assert_eq!(phase.border, Color::hex(0x00008B));
        assert_eq!(phase.font.size, 10.0);
        let error = s.stereotyped(Some("error")).state;
        assert_eq!(error.font.color, Color::hex(0xFF0000));
        assert_eq!(error.fill, Some(Color::hex(0xEEEEEE)));
        // A state of no or another stereotype keeps the look of every state.
        assert_eq!(s.stereotyped(None).state, s.state);
        assert_eq!(s.stereotyped(Some("sub")).state, s.state);
        // What is not drawn for a stereotype yet declines.
        assert!(skin(&[("stateFontStyle<<phase>>", "bold")]).is_err());
        assert!(skin(&[("stateRoundCorner<<phase>>", "5")]).is_err());
    }

    #[test]
    fn state_colors_are_not_ignored() {
        let s = skin(&[
            ("StateBackgroundColor", "#1e1e1e"),
            ("StateBorderColor", "#e68e0d"),
            ("defaultFontColor", "#bebebe"),
            ("defaultFontSize", "16"),
            ("StateFontSize", "18"),
            ("ArrowColor", "#e68e0d"),
            ("ArrowFontColor", "#bebebe"),
            ("NoteBackgroundColor", "#2a2a2a"),
            ("backgroundColor", "#121212"),
            ("roundCorner", "0"),
            ("ParticipantBackgroundColor", "#ff0000"),
            ("ActivityBackgroundColor", "#ff0000"),
            ("ActivityStartColor", "#ff0000"),
            ("PartitionBorderColor", "#ff0000"),
            ("SwimlaneBorderColor", "#ff0000"),
            ("shadowing", "false"),
            ("style", "strictuml"),
        ])
        .unwrap();
        assert_eq!(s.state.fill, Some(Color::hex(0x1E1E1E)));
        assert_eq!(s.state.border, Color::hex(0xE68E0D));
        assert_eq!(s.state.font.size, 18.0);
        assert_eq!(s.arrow_font.size, 16.0);
        assert_eq!(s.state.font.color, Color::hex(0xBEBEBE));
        assert_eq!(s.arrow.color, Color::hex(0xE68E0D));
        assert_eq!(s.note.fill, Some(Color::hex(0x2A2A2A)));
        assert_eq!(s.background, Some(Color::hex(0x121212)));
        assert_eq!(s.round, 0.0);
        assert!(s.strict);
    }

    #[test]
    fn declines_what_would_change_the_picture() {
        for (key, value) in [
            ("handwritten", "true"),
            ("shadowing", "true"),
            ("StateFontName", "Courier"),
            ("StateBackgroundColor", "#red/blue"),
        ] {
            let err = skin(&[("roundCorner", "3"), (key, value)]).unwrap_err();
            assert_eq!(err.line, Some(1), "{key}");
        }
    }
}
