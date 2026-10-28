//! How an activity diagram looks: PlantUML's default style for it, changed
//! by skinparams, which PlantUML turns into styles, and by `<style>`
//! blocks. A skinparam that would change the diagram but is not drawn yet
//! declines it; those for other diagram types are ignored.

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
    /// `skinparam style strictuml`: plain triangles for arrow heads.
    pub strict: bool,
    pub background: Paint,
    /// The box of an action.
    pub activity: Look,
    /// Corner radius of an action's box; PlantUML's `RoundCorner` is twice it.
    pub round: f32,
    /// Between an action's box and its text.
    pub padding: f32,
    /// The diamonds of `if`, `while` and `repeat`, with their tests.
    pub diamond: Look,
    /// `skinparam conditionStyle InsideDiamond`: a test in a diamond, not
    /// between pointed ends.
    pub test_in_diamond: bool,
    /// `skinparam ConditionEndStyle hline`: branches come together on a
    /// line, not in a diamond.
    pub merge_line: bool,
    pub arrow: Stroke,
    pub arrow_font: Font,
    pub start: Color,
    /// `stop` and `end`.
    pub end: Color,
    /// The bars of `fork`.
    pub bar: Color,
    pub lane_border: Color,
    pub lane_thickness: f32,
    pub lane_font: Font,
    pub note: Look,
    /// The frame of a partition or a group.
    pub partition: Look,
    pub title_font: Font,
    pub header_font: Font,
    pub footer_font: Font,
    pub caption_font: Font,
}

impl Default for Skin {
    fn default() -> Self {
        let black = Color::BLACK;
        let dark = Color::hex(0x181818);
        let gray = Color::hex(0x888888);
        let shape = Look {
            fill: Some(Color::hex(0xF1F1F1)),
            border: dark,
            thickness: 0.5,
            font: Font::new(12.0, black),
        };
        Skin {
            font_name: "sans-serif".into(),
            monochrome: false,
            strict: false,
            background: None,
            activity: shape,
            round: 12.5,
            padding: 10.0,
            diamond: Look {
                font: Font::new(11.0, black),
                ..shape
            },
            test_in_diamond: false,
            merge_line: false,
            arrow: Stroke::new(dark, 1.0),
            arrow_font: Font::new(11.0, black),
            start: Color::hex(0x222222),
            end: Color::hex(0x222222),
            bar: Color::hex(0x555555),
            lane_border: black,
            lane_thickness: 1.5,
            lane_font: Font::new(18.0, black),
            note: Look {
                fill: Some(Color::hex(0xFEFFDD)),
                border: dark,
                thickness: 0.5,
                font: Font::new(13.0, black),
            },
            partition: Look {
                fill: None,
                border: black,
                thickness: 1.5,
                font: Font::new(14.0, black),
            },
            title_font: Font::new(14.0, black).bold(),
            header_font: Font::new(10.0, gray),
            footer_font: Font::new(10.0, gray),
            caption_font: Font::new(14.0, black),
        }
    }
}

/// Skinparams of other diagram types, by the start of their key, and of
/// the legend and the main frame, which every type draws the same way.
const OTHER_TYPES: [&str; 44] = [
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
    "linetype",
    "mainframe",
    "map",
    "maxmessagesize",
    "mindmap",
    "node",
    "object",
    "package",
    "participant",
    "queue",
    "rectangle",
    "responsemessagebelowarrow",
    "sequence",
    "stack",
    "state",
    "stereotype",
    "storage",
    "timing",
    "usecase",
    "wbs",
    "yaml",
    "circledcharacter",
];

/// Skinparams that change nothing in an SVG of an activity diagram.
const NO_EFFECT: [&str; 5] = [
    "dpi",
    "nodesep",
    "ranksep",
    "svglinktarget",
    "minclasswidth",
];

/// Elements with a font, by their skinparam name.
const FONTS: [&str; 11] = [
    "activitydiamond",
    "activity",
    "arrow",
    "swimlanetitle",
    "note",
    "partition",
    "title",
    "header",
    "footer",
    "caption",
    "default",
];

impl Skin {
    pub fn from_params(params: &[Skinparam]) -> Result<Skin, Declined> {
        let mut skin = Skin::default();
        // Font settings for every element, applied after the rest so an
        // element's own setting wins whatever the order.
        let mut font_size = None;
        let mut font_color = None;
        let mut own_size = Vec::new();
        let mut own_color = Vec::new();
        for p in params {
            let key = normalize_key(&p.key);
            let value = p.value.trim();

            if OTHER_TYPES.iter().any(|t| key.starts_with(t)) || NO_EFFECT.contains(&key.as_str()) {
                continue;
            }
            match key.as_str() {
                "monochrome" => skin.monochrome = p.flag()?,
                "shadowing" | "handwritten" | "activityshadowing" => {
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
                "roundcorner" | "activityroundcorner" => skin.round = p.corner()?,
                "backgroundcolor" => skin.background = p.paint()?,
                "arrowcolor" => skin.arrow.color = p.solid()?,
                "arrowthickness" => skin.arrow.width = p.number(0.1, 20.0)?,
                "arrowstyle" => {
                    skin.arrow.dash = match value.to_ascii_lowercase().as_str() {
                        "dashed" => Some((7.0, 7.0)),
                        "dotted" => Some((1.0, 3.0)),
                        _ => return Err(p.unsupported()),
                    };
                }
                "activitybackgroundcolor" => skin.activity.fill = p.paint()?,
                "activitybordercolor" => skin.activity.border = p.solid()?,
                "activityborderthickness" => skin.activity.thickness = p.number(0.0, 20.0)?,
                "activitydiamondbackgroundcolor" => skin.diamond.fill = p.paint()?,
                "activitydiamondbordercolor" => skin.diamond.border = p.solid()?,
                "activitydiamondborderthickness" => skin.diamond.thickness = p.number(0.0, 20.0)?,
                "conditionstyle" => {
                    skin.test_in_diamond = match value.to_ascii_lowercase().as_str() {
                        "inside" => false,
                        // PlantUML's own name for it, besides the long one.
                        "insidediamond" | "foo1" => true,
                        _ => return Err(p.unsupported()),
                    };
                }
                "conditionendstyle" => {
                    skin.merge_line = match value.to_ascii_lowercase().as_str() {
                        "diamond" => false,
                        "hline" => true,
                        _ => return Err(p.unsupported()),
                    };
                }
                "activitystartcolor" => skin.start = p.solid()?,
                "activityendcolor" => skin.end = p.solid()?,
                "activitybarcolor" => skin.bar = p.solid()?,
                "swimlanebordercolor" => skin.lane_border = p.solid()?,
                "swimlaneborderthickness" => skin.lane_thickness = p.number(0.0, 20.0)?,
                "notebackgroundcolor" => skin.note.fill = p.paint()?,
                "notebordercolor" => skin.note.border = p.solid()?,
                "noteborderthickness" => skin.note.thickness = p.number(0.0, 20.0)?,
                "partitionbackgroundcolor" => skin.partition.fill = p.paint()?,
                "partitionbordercolor" => skin.partition.border = p.solid()?,
                "partitionborderthickness" => skin.partition.thickness = p.number(0.0, 20.0)?,
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
        Ok(skin)
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

/// The elements `<style>` reaches in an activity diagram, by the names
/// PlantUML looks them up with. The diamonds are activities too, so what
/// is set for `activity` reaches them, as in PlantUML. `stop` and `end`
/// share one color.
pub const STYLE: &[Element] = &[
    Element::new(
        &["element", "activityDiagram", "activity"],
        "Activity",
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
    Element::new(
        &["element", "activityDiagram", "activity", "diamond"],
        "ActivityDiamond",
        LOOK,
    ),
    Element::keys(
        &["element", "activityDiagram", "arrow"],
        &[
            (Prop::LineColor, "ArrowColor"),
            (Prop::LineThickness, "ArrowThickness"),
            (Prop::LineStyle, "ArrowStyle"),
            (Prop::FontColor, "ArrowFontColor"),
            (Prop::FontSize, "ArrowFontSize"),
            (Prop::FontStyle, "ArrowFontStyle"),
        ],
    ),
    Element::keys(
        &["element", "activityDiagram", "circle", "start"],
        &[(Prop::BackGroundColor, "ActivityStartColor")],
    ),
    Element::keys(
        &["element", "activityDiagram", "circle", "stop"],
        &[(Prop::LineColor, "ActivityEndColor")],
    ),
    Element::keys(
        &["element", "activityDiagram", "circle", "end"],
        &[(Prop::LineColor, "ActivityEndColor")],
    ),
    Element::keys(
        &["element", "activityDiagram", "activityBar"],
        &[(Prop::BackGroundColor, "ActivityBarColor")],
    ),
    Element::keys(
        &["element", "activityDiagram", "swimlane"],
        &[
            (Prop::LineColor, "SwimlaneBorderColor"),
            (Prop::LineThickness, "SwimlaneBorderThickness"),
            (Prop::FontColor, "SwimlaneTitleFontColor"),
            (Prop::FontSize, "SwimlaneTitleFontSize"),
            (Prop::FontStyle, "SwimlaneTitleFontStyle"),
        ],
    ),
    Element::new(&["element", "activityDiagram", "note"], "Note", LOOK),
    // Partitions and groups.
    Element::new(
        &["element", "activityDiagram", "composite"],
        "Partition",
        LOOK,
    ),
    Element::new(&["document", "activityDiagram", "legend"], "Legend", LEGEND),
];

/// `activityfontsize` as `("activity", "fontsize")`; the longest element
/// name wins, so `activitydiamondfontsize` is the diamond's.
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
        "activitydiamond" => &mut skin.diamond.font,
        "arrow" => &mut skin.arrow_font,
        "swimlanetitle" => &mut skin.lane_font,
        "note" => &mut skin.note.font,
        "partition" => &mut skin.partition.font,
        "title" => &mut skin.title_font,
        "header" => &mut skin.header_font,
        "footer" => &mut skin.footer_font,
        "caption" => &mut skin.caption_font,
        _ => &mut skin.activity.font,
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
    fn defaults_follow_plantuml_styles() {
        let s = skin(&[]).unwrap();
        assert_eq!(
            (
                s.activity.font.size,
                s.diamond.font.size,
                s.round,
                s.padding
            ),
            (12.0, 11.0, 12.5, 10.0)
        );
        assert_eq!(s.activity.fill, Some(Color::hex(0xF1F1F1)));
        assert_eq!((s.lane_font.size, s.lane_thickness), (18.0, 1.5));
    }

    #[test]
    fn skin_packs_and_element_fonts() {
        let s = skin(&[
            ("defaultFontSize", "16"),
            ("ActivityDiamondFontSize", "9"),
            ("roundCorner", "12"),
            ("ArrowThickness", "1.5"),
            ("ActivityBackgroundColor", "#1e1e1e"),
            ("ActivityDiamondBorderColor", "#e68e0d"),
            ("ParticipantBackgroundColor", "#ff0000"),
            ("SequenceLifeLineBorderColor", "#555555"),
            ("PartitionBorderColor", "#555555"),
            ("SwimlaneBorderColor", "#555555"),
            ("style", "strictuml"),
            ("ActivityFontStyle", "bold"),
        ])
        .unwrap();
        assert_eq!((s.activity.font.size, s.diamond.font.size), (16.0, 9.0));
        assert_eq!((s.round, s.arrow.width), (6.0, 1.5));
        assert_eq!(s.activity.fill, Some(Color::hex(0x1E1E1E)));
        assert_eq!(s.diamond.border, Color::hex(0xE68E0D));
        assert_eq!(s.lane_border, Color::hex(0x555555));
        assert!(s.strict && s.activity.font.bold);
    }

    #[test]
    fn declines_what_reaches_the_diagram_but_is_not_drawn() {
        for (key, value) in [
            ("handwritten", "true"),
            ("ActivityFontName", "Courier"),
            ("SwimlaneWidth", "same"),
            ("ActivityBackgroundColor", "#red/blue"),
            ("wrapWidth", "100"),
            ("conditionStyle", "diamond"),
            ("ConditionEndStyle", "dot"),
        ] {
            let err = skin(&[("roundCorner", "3"), (key, value)]).unwrap_err();
            assert_eq!(err.line, Some(1), "{key}");
        }
    }

    #[test]
    fn condition_styles() {
        let s = skin(&[
            ("conditionStyle", "inside"),
            ("ConditionEndStyle", "diamond"),
        ])
        .unwrap();
        assert!(!s.test_in_diamond && !s.merge_line);
        for name in ["InsideDiamond", "foo1"] {
            assert!(
                skin(&[("conditionStyle", name)]).unwrap().test_in_diamond,
                "{name}"
            );
        }
        assert!(skin(&[("ConditionEndStyle", "hline")]).unwrap().merge_line);
    }
}
