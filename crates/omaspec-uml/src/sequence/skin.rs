//! How a sequence diagram looks: PlantUML's defaults, changed by the
//! diagram's `skinparam` lines and `<style>` blocks. A skinparam that would
//! change a sequence diagram but is not drawn yet declines it; skinparams
//! for other diagram types are ignored.

use super::Skinparam;
use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::normalize_key as normalize;
use crate::style::{Element, LEGEND, Prop, TEXT};
use crate::text::Font;

/// The look of a shape: its fill, its border, its text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub fill: Paint,
    pub border: Color,
    pub thickness: f32,
    pub font: Font,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub font_name: String,
    pub monochrome: bool,
    /// `skinparam style strictuml`: plain triangles for arrow heads.
    pub strict: bool,
    /// Corner radius of boxes.
    pub round: f32,
    pub background: Paint,
    pub arrow: Color,
    pub arrow_thickness: f32,
    pub arrow_font: Font,
    pub message_align: MessageAlign,
    pub participant: Look,
    pub actor: Look,
    pub boundary: Look,
    pub control: Look,
    pub entity: Look,
    pub database: Look,
    pub collections: Look,
    pub queue: Look,
    pub stereotype_font: Font,
    pub lifeline: Color,
    pub lifeline_thickness: f32,
    /// Activation bars; their border is the lifeline's color.
    pub activation_fill: Paint,
    pub group_border: Color,
    pub group_thickness: f32,
    pub group_header_fill: Paint,
    pub group_header_font: Font,
    pub group_label_font: Font,
    pub note: Look,
    pub divider: Look,
    pub reference: Look,
    pub reference_header_fill: Paint,
    pub delay_font: Font,
    pub box_look: Look,
    pub title_font: Font,
    pub header_font: Font,
    pub footer_font: Font,
    pub caption_font: Font,
    pub participant_padding: f32,
    pub box_padding: f32,
}

const DARK: Color = Color::hex(0x181818);

impl Default for Skin {
    fn default() -> Self {
        let black = Color::BLACK;
        let head = Look {
            fill: Some(Color::hex(0xE2E2F0)),
            border: DARK,
            thickness: 0.5,
            font: Font::new(14.0, black),
        };
        Skin {
            font_name: "sans-serif".into(),
            monochrome: false,
            strict: false,
            round: 2.5,
            background: None,
            arrow: DARK,
            arrow_thickness: 1.0,
            arrow_font: Font::new(13.0, black),
            message_align: MessageAlign::Left,
            participant: head,
            actor: head,
            boundary: head,
            control: head,
            entity: head,
            database: head,
            collections: head,
            queue: head,
            stereotype_font: Font::new(12.0, black).italic(),
            lifeline: DARK,
            lifeline_thickness: 0.5,
            activation_fill: Some(Color::WHITE),
            group_border: black,
            group_thickness: 1.5,
            group_header_fill: Some(Color::hex(0xEEEEEE)),
            group_header_font: Font::new(13.0, black).bold(),
            group_label_font: Font::new(11.0, black).bold(),
            note: Look {
                fill: Some(Color::hex(0xFEFFDD)),
                border: DARK,
                thickness: 0.5,
                font: Font::new(13.0, black),
            },
            divider: Look {
                fill: Some(Color::hex(0xEEEEEE)),
                border: black,
                thickness: 1.0,
                font: Font::new(13.0, black).bold(),
            },
            reference: Look {
                fill: Some(Color::WHITE),
                border: black,
                thickness: 1.5,
                font: Font::new(12.0, black),
            },
            reference_header_fill: Some(Color::hex(0xEEEEEE)),
            delay_font: Font::new(11.0, black),
            box_look: Look {
                fill: Some(Color::hex(0xDDDDDD)),
                border: DARK,
                thickness: 0.5,
                font: Font::new(13.0, black).bold(),
            },
            title_font: Font::new(14.0, black).bold(),
            header_font: Font::new(10.0, Color::hex(0x888888)),
            footer_font: Font::new(10.0, Color::hex(0x888888)),
            caption_font: Font::new(14.0, black),
            participant_padding: 0.0,
            box_padding: 10.0,
        }
    }
}

/// Skinparams of other diagram types, by the start of their key, and of
/// the legend and the main frame, which every type draws the same way.
const OTHER_TYPES: [&str; 34] = [
    "activity",
    "agent",
    "artifact",
    "card",
    "circle",
    "class",
    "cloud",
    "component",
    "diamond",
    "file",
    "folder",
    "frame",
    "genericdisplay",
    "interface",
    "json",
    "label",
    "legend",
    "linetype",
    "mainframe",
    "map",
    "mindmap",
    "node",
    "object",
    "package",
    "partition",
    "rectangle",
    "stack",
    "state",
    "storage",
    "swimlane",
    "timing",
    "usecase",
    "wbs",
    "yaml",
];

/// Skinparams that change nothing in an SVG of a sequence diagram.
const NO_EFFECT: [&str; 6] = [
    "dpi",
    "nodesep",
    "ranksep",
    "minclasswidth",
    "groupinheritance",
    "svglinktarget",
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
            let key = normalize(&p.key);

            if OTHER_TYPES.iter().any(|t| key.starts_with(t)) || NO_EFFECT.contains(&key.as_str()) {
                continue;
            }
            match key.as_str() {
                "monochrome" => skin.monochrome = p.flag()?,
                "shadowing" | "handwritten" => {
                    if p.flag()? {
                        return Err(p.unsupported());
                    }
                }
                "responsemessagebelowarrow" => {
                    if p.flag()? {
                        return Err(p.unsupported());
                    }
                }
                "style" => {
                    if p.value.trim().eq_ignore_ascii_case("strictuml") {
                        skin.strict = true;
                    } else {
                        return Err(p.unsupported());
                    }
                }
                "defaultfontname" => {
                    let name = p.value.trim().trim_matches('"');
                    if name.is_empty() || name.contains(['<', '>', '"', '&']) {
                        return Err(p.unsupported());
                    }
                    skin.font_name = name.to_owned();
                }
                "defaultfontsize" => font_size = Some(p.number(4.0, 100.0)?),
                "defaultfontcolor" => font_color = Some(p.solid()?),
                "roundcorner" => skin.round = p.corner()?,
                "backgroundcolor" => skin.background = p.paint()?,
                "arrowcolor" => skin.arrow = p.solid()?,
                "arrowthickness" => skin.arrow_thickness = p.number(0.1, 20.0)?,
                "lifelinebordercolor" | "sequencelifelinebordercolor" => {
                    skin.lifeline = p.solid()?
                }
                "sequencelifelinebackgroundcolor" => skin.activation_fill = p.paint()?,
                "sequencegroupbordercolor" => skin.group_border = p.solid()?,
                "sequencegroupborderthickness" => skin.group_thickness = p.number(0.1, 20.0)?,
                "sequencegroupbackgroundcolor" | "sequencegroupheaderbackgroundcolor" => {
                    skin.group_header_fill = p.paint()?;
                }
                "sequencereferenceheaderbackgroundcolor" => {
                    skin.reference_header_fill = p.paint()?;
                }
                "sequencemessagealignment" => {
                    skin.message_align = match p.value.trim().to_ascii_lowercase().as_str() {
                        "left" => MessageAlign::Left,
                        "center" => MessageAlign::Center,
                        "right" => MessageAlign::Right,
                        _ => return Err(p.unsupported()),
                    };
                }
                "participantpadding" => skin.participant_padding = p.number(0.0, 200.0)?,
                "boxpadding" => skin.box_padding = p.number(0.0, 200.0)?,
                _ => {
                    let Some((element, property)) = split_element(&key) else {
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
                            let v = p.value.trim().to_ascii_lowercase();
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
                        "backgroundcolor" => {
                            let fill = p.paint()?;
                            match look_of(&mut skin, element) {
                                Some(look) => look.fill = fill,
                                None if element == "sequencegroupheader" => {
                                    skin.group_header_fill = fill;
                                }
                                None => return Err(p.unsupported()),
                            }
                        }
                        "bordercolor" => {
                            let c = p.solid()?;
                            match look_of(&mut skin, element) {
                                Some(look) => look.border = c,
                                None => return Err(p.unsupported()),
                            }
                        }
                        "borderthickness" => {
                            let t = p.number(0.0, 20.0)?;
                            match look_of(&mut skin, element) {
                                Some(look) => look.thickness = t,
                                None => return Err(p.unsupported()),
                            }
                        }
                        _ => return Err(p.unsupported()),
                    }
                }
            }
        }
        for element in ELEMENTS {
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

    pub fn look(&self, shape: super::Shape) -> &Look {
        use super::Shape;
        match shape {
            Shape::Participant => &self.participant,
            Shape::Actor => &self.actor,
            Shape::Boundary => &self.boundary,
            Shape::Control => &self.control,
            Shape::Entity => &self.entity,
            Shape::Database => &self.database,
            Shape::Collections => &self.collections,
            Shape::Queue => &self.queue,
        }
    }
}

/// The elements with a font, by their skinparam name.
const ELEMENTS: [&str; 21] = [
    "arrow",
    "participant",
    "actor",
    "boundary",
    "control",
    "entity",
    "database",
    "collections",
    "queue",
    "sequencestereotype",
    "sequencegroupheader",
    "sequencegroup",
    "note",
    "sequencedivider",
    "sequencereference",
    "sequencedelay",
    "sequencebox",
    "title",
    "header",
    "footer",
    "caption",
];

/// `participantbordercolor` as `("participant", "bordercolor")`.
fn split_element(key: &str) -> Option<(&'static str, &str)> {
    let mut found = None;
    for element in ELEMENTS {
        if let Some(rest) = key.strip_prefix(element)
            && found.is_none_or(|(e, _): (&str, &str)| element.len() > e.len())
        {
            found = Some((element, rest));
        }
    }
    found
}

fn font_of<'a>(skin: &'a mut Skin, element: &str) -> &'a mut Font {
    match element {
        "sequencestereotype" => &mut skin.stereotype_font,
        "sequencegroupheader" => &mut skin.group_header_font,
        "sequencegroup" => &mut skin.group_label_font,
        "sequencedelay" => &mut skin.delay_font,
        "title" => &mut skin.title_font,
        "header" => &mut skin.header_font,
        "footer" => &mut skin.footer_font,
        "caption" => &mut skin.caption_font,
        "participant" => &mut skin.participant.font,
        "actor" => &mut skin.actor.font,
        "boundary" => &mut skin.boundary.font,
        "control" => &mut skin.control.font,
        "entity" => &mut skin.entity.font,
        "database" => &mut skin.database.font,
        "collections" => &mut skin.collections.font,
        "queue" => &mut skin.queue.font,
        "note" => &mut skin.note.font,
        "sequencedivider" => &mut skin.divider.font,
        "sequencereference" => &mut skin.reference.font,
        "sequencebox" => &mut skin.box_look.font,
        _ => &mut skin.arrow_font,
    }
}

fn look_of<'a>(skin: &'a mut Skin, element: &str) -> Option<&'a mut Look> {
    Some(match element {
        "participant" => &mut skin.participant,
        "actor" => &mut skin.actor,
        "boundary" => &mut skin.boundary,
        "control" => &mut skin.control,
        "entity" => &mut skin.entity,
        "database" => &mut skin.database,
        "collections" => &mut skin.collections,
        "queue" => &mut skin.queue,
        "note" => &mut skin.note,
        "sequencedivider" => &mut skin.divider,
        "sequencereference" => &mut skin.reference,
        "sequencebox" => &mut skin.box_look,
        _ => return None,
    })
}

/// A box with a text.
const LOOK: &[Prop] = &[
    Prop::BackGroundColor,
    Prop::LineColor,
    Prop::LineThickness,
    Prop::FontColor,
    Prop::FontSize,
    Prop::FontStyle,
];

/// The elements `<style>` reaches in a sequence diagram, by the names
/// PlantUML looks them up with. Destroy crosses, `hnote` and `rnote` draw
/// nothing of their own yet.
pub const STYLE: &[Element] = &[
    Element::new(
        &["element", "sequenceDiagram", "participant"],
        "Participant",
        LOOK,
    ),
    Element::new(&["element", "sequenceDiagram", "actor"], "Actor", LOOK),
    Element::new(
        &["element", "sequenceDiagram", "boundary"],
        "Boundary",
        LOOK,
    ),
    Element::new(&["element", "sequenceDiagram", "control"], "Control", LOOK),
    Element::new(&["element", "sequenceDiagram", "entity"], "Entity", LOOK),
    Element::new(
        &["element", "sequenceDiagram", "database"],
        "Database",
        LOOK,
    ),
    Element::new(
        &["element", "sequenceDiagram", "collections"],
        "Collections",
        LOOK,
    ),
    Element::new(&["element", "sequenceDiagram", "queue"], "Queue", LOOK),
    Element::new(
        &["element", "sequenceDiagram", "stereotype"],
        "SequenceStereotype",
        TEXT,
    ),
    Element::keys(
        &["element", "sequenceDiagram", "arrow"],
        &[
            (Prop::LineColor, "ArrowColor"),
            (Prop::LineThickness, "ArrowThickness"),
            (Prop::FontColor, "ArrowFontColor"),
            (Prop::FontSize, "ArrowFontSize"),
            (Prop::FontStyle, "ArrowFontStyle"),
            (Prop::HorizontalAlignment, "SequenceMessageAlignment"),
        ],
    ),
    Element::keys(
        &["element", "sequenceDiagram", "lifeLine"],
        &[(Prop::LineColor, "SequenceLifeLineBorderColor")],
    ),
    Element::keys(
        &["element", "sequenceDiagram", "activationBox"],
        &[(Prop::BackGroundColor, "SequenceLifeLineBackgroundColor")],
    ),
    Element::new(
        &["element", "sequenceDiagram", "lifeLine", "delay"],
        "SequenceDelay",
        TEXT,
    ),
    Element::keys(&["element", "sequenceDiagram", "lifeLine", "destroy"], &[]),
    Element::new(&["element", "sequenceDiagram", "note"], "Note", LOOK),
    Element::keys(&["element", "sequenceDiagram", "note", "hnote"], &[]),
    Element::keys(&["element", "sequenceDiagram", "note", "rnote"], &[]),
    Element::keys(
        &["element", "sequenceDiagram", "group"],
        &[
            (Prop::LineColor, "SequenceGroupBorderColor"),
            (Prop::LineThickness, "SequenceGroupBorderThickness"),
            (Prop::FontColor, "SequenceGroupFontColor"),
            (Prop::FontSize, "SequenceGroupFontSize"),
            (Prop::FontStyle, "SequenceGroupFontStyle"),
        ],
    ),
    Element::keys(
        &["element", "sequenceDiagram", "groupHeader"],
        &[
            (Prop::BackGroundColor, "SequenceGroupHeaderBackgroundColor"),
            (Prop::FontColor, "SequenceGroupHeaderFontColor"),
            (Prop::FontSize, "SequenceGroupHeaderFontSize"),
            (Prop::FontStyle, "SequenceGroupHeaderFontStyle"),
        ],
    ),
    Element::new(
        &["element", "sequenceDiagram", "separator"],
        "SequenceDivider",
        LOOK,
    ),
    Element::new(
        &["element", "sequenceDiagram", "reference"],
        "SequenceReference",
        LOOK,
    ),
    Element::keys(
        &["element", "sequenceDiagram", "referenceHeader"],
        &[(
            Prop::BackGroundColor,
            "SequenceReferenceHeaderBackgroundColor",
        )],
    ),
    Element::new(&["element", "sequenceDiagram", "box"], "SequenceBox", LOOK),
    Element::new(&["document", "sequenceDiagram", "legend"], "Legend", LEGEND),
];

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
    fn the_skin_packs_are_understood() {
        let s = skin(&[
            ("monochrome", "true"),
            ("shadowing", "false"),
            ("defaultFontName", "serif"),
            ("style", "strictuml"),
            ("roundCorner", "0"),
        ])
        .unwrap();
        assert!(s.monochrome && s.strict);
        assert_eq!((s.font_name.as_str(), s.round), ("serif", 0.0));
        // `RoundCorner` reads as in every other type: up to 200, the
        // radius half of it.
        assert_eq!(skin(&[("roundCorner", "150")]).unwrap().round, 75.0);
        assert!(skin(&[("roundCorner", "201")]).is_err());

        let s = skin(&[
            ("defaultFontSize", "16"),
            ("ArrowThickness", "1.5"),
            ("NoteFontSize", "12"),
        ])
        .unwrap();
        assert_eq!(s.participant.font.size, 16.0);
        assert_eq!(s.arrow_font.size, 16.0);
        assert_eq!(s.note.font.size, 12.0);
        assert_eq!(s.arrow_thickness, 1.5);

        let s = skin(&[
            ("backgroundColor", "#121212"),
            ("defaultFontColor", "#eeeeee"),
            ("ArrowColor", "#e68e0d"),
            ("ParticipantBackgroundColor", "#1e1e1e"),
            ("ActorBorderColor", "#e68e0d"),
            ("ActivityBackgroundColor", "#1e1e1e"),
            ("SequenceLifeLineBorderColor", "#888888"),
            ("SequenceGroupBorderColor", "#888888"),
            ("NoteBackgroundColor", "#333333"),
            ("SwimlaneBorderColor", "#888888"),
        ])
        .unwrap();
        assert_eq!(s.background, Some(Color::hex(0x121212)));
        assert_eq!(s.participant.fill, Some(Color::hex(0x1E1E1E)));
        assert_eq!(s.actor.border, Color::hex(0xE68E0D));
        assert_eq!(s.note.font.color, Color::hex(0xEEEEEE));
        assert_eq!(s.lifeline, Color::hex(0x888888));
    }

    #[test]
    fn keys_are_normalized() {
        let s = skin(&[("sequence_Participant.Border Color", "red")]).unwrap();
        assert_eq!(s.participant.border, Color(255, 0, 0));
        let s = skin(&[
            ("SequenceArrowColor", "blue"),
            ("sequenceMessageAlign", "center"),
        ])
        .unwrap();
        assert_eq!(s.arrow, Color(0, 0, 255));
        assert_eq!(s.message_align, MessageAlign::Center);
    }

    #[test]
    fn declines_what_it_does_not_draw() {
        for (key, value) in [
            ("handwritten", "true"),
            ("shadowing", "true"),
            ("monochrome", "reverse"),
            ("maxMessageSize", "100"),
            ("ParticipantFontName", "Courier"),
            ("ArrowColor", "#red/blue"),
            ("SequenceTitleFontSize", "x"),
        ] {
            let err = skin(&[("roundCorner", "3"), (key, value)]).unwrap_err();
            assert_eq!(err.line, Some(1), "{key}");
        }
    }
}
