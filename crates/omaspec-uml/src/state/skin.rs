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
    /// `skinparam handwritten true`: shapes drawn by hand (docs/adr/0023).
    pub sketch: bool,
    /// `skinparam shadowing true`: the boxes cast a shadow.
    pub shadow: bool,
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
    /// What skinparams say of the words under a state's name (`state A :
    /// words`, `AttributeFontColor`); the rest they take from the state's
    /// font, a stereotype's too ([`Skin::attribute_font`]).
    pub attribute: Attribute,
    /// A choice's fill and border where skinparams give it its own
    /// (`activityDiamondBackgroundColor` in the old activity syntax).
    pub diamond_fill: Option<Paint>,
    pub diamond_border: Option<Color>,
    /// The start, the end and the bars, where skinparams give them a color
    /// of their own (`StartColor`, `EndColor`, `BarColor`); else the
    /// border's, or the state's own color.
    pub start: Option<Color>,
    pub end: Option<Color>,
    pub bar: Option<Color>,
    /// The faces its elements' fonts name (`StateFontName Impact`).
    pub faces: crate::text::Faces,
    /// A state's name in bold and centered; an action's in an activity is
    /// plain, its lines starting at the left of its words.
    pub bold_names: bool,
    /// `activityShape octagon`: an action's box has its corners cut.
    pub octagon: bool,
}

impl Default for Skin {
    fn default() -> Self {
        let black = Color::BLACK;
        let dark = Color::hex(0x181818);
        let gray = Color::hex(0x888888);
        Skin {
            font_name: crate::FACE.into(),
            monochrome: false,
            sketch: false,
            shadow: false,
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
            attribute: Attribute::default(),
            diamond_fill: None,
            diamond_border: None,
            start: None,
            end: None,
            bar: None,
            faces: Default::default(),
            bold_names: true,
            octagon: false,
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
const FONTS: [&str; 9] = [
    "state",
    "stateattribute",
    "arrow",
    "note",
    "title",
    "header",
    "footer",
    "caption",
    "default",
];

/// What skinparams say of the words under a state's name
/// (`StateAttributeFontSize`, …); `None` for what the state's font gives.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Attribute {
    pub size: Option<f32>,
    pub color: Option<Color>,
    /// Bold and italic.
    pub style: Option<(bool, bool)>,
    pub face: Option<crate::text::Face>,
}

impl Skin {
    pub fn from_params(params: &[Skinparam]) -> Result<Skin, Declined> {
        Skin::from_params_on(Skin::default(), params, false)
    }

    /// The skin of an activity in PlantUML's old syntax, drawn as states:
    /// its look ([`Skin::activity`]) with its `activity…` skinparams read
    /// as the state's (`activityBackgroundColor<< Begin >>`,
    /// `activityBarColor`, `activityShape`, `activityDiamond…` for its
    /// choices). A skinparam it declines is named as written.
    pub fn from_activity_params(params: &[Skinparam]) -> Result<Skin, Declined> {
        Skin::from_params_on(Skin::activity(), params, true)
    }

    /// The font of the words under a state's name: the state's, two points
    /// smaller, as skinparams change it. The skin of a stereotype
    /// ([`Skin::stereotyped`]) gives its state's font.
    pub fn attribute_font(&self) -> Font {
        let state = self.state.font;
        let a = self.attribute;
        let (bold, italic) = a.style.unwrap_or((state.bold, state.italic));
        Font {
            size: a.size.unwrap_or((state.size - 2.0).max(4.0)),
            color: a.color.unwrap_or(state.color),
            bold,
            italic,
            face: a.face.or(state.face),
        }
    }

    /// The look of an activity in PlantUML's old syntax, before its
    /// skinparams: plain names of 12 points in boxes rounded as PlantUML
    /// rounds them, thin borders, dark bars, its start and end.
    pub fn activity() -> Skin {
        let dark = Color::hex(0x222222);
        let mut skin = Skin::default();
        skin.state.font = Font::new(12.0, Color::BLACK);
        skin.state.thickness = 0.5;
        skin.round = 12.5;
        skin.arrow_font = Font::new(11.0, Color::BLACK);
        skin.bold_names = false;
        skin.start = Some(dark);
        skin.end = Some(dark);
        skin.bar = Some(Color::hex(0x555555));
        skin
    }

    /// `base` changed by `params`, an activity's `activity…` keys read as
    /// the state's where `activity` says so.
    fn from_params_on(base: Skin, params: &[Skinparam], activity: bool) -> Result<Skin, Declined> {
        let mut skin = base;
        // The faces named, put in once the whole skin is read: the hand
        // writes every word where the diagram is drawn by hand.
        let mut named: Vec<(&'static str, crate::text::Face)> = Vec::new();
        let mut font_size = None;
        let mut font_color = None;
        let mut own_size = Vec::new();
        let mut own_color = Vec::new();
        let mut stereotyped = Vec::new();
        for p in params {
            let mut key = normalize_key(&p.key);
            if activity && let Some(rest) = key.strip_prefix("activity") {
                key = format!("state{rest}");
            }
            let value = p.value.trim();
            // A style for a stereotype (`.foo { state { … } }`) says so
            // before the name, `class=foo|StateBackgroundColor`: the look of
            // the states of that stereotype, as `BackgroundColor<<foo>>` is.
            if let Some((only, name)) = p.key.split_once('|') {
                let class = only.strip_prefix("class=").filter(|c| !c.contains('&'));
                let Some(class) = class else {
                    let only = only
                        .replace("class=", ".")
                        .replace("depth=", ":depth(")
                        .replace('&', " ");
                    let only = if only.contains(":depth(") {
                        format!("{only})")
                    } else {
                        only
                    };
                    return Err(Declined::at(
                        p.line,
                        format!(
                            "`{}` for `{only}` is not supported for states yet",
                            p.written()
                        ),
                    ));
                };
                stereotyped.push((normalize_key(name), class.to_ascii_lowercase(), p));
                continue;
            }
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
                "handwritten" => skin.sketch = p.flag()?,
                "shadowing" | "stateshadowing" => skin.shadow = p.flag()?,
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
                "stateshape" => {
                    skin.octagon = match value.to_ascii_lowercase().as_str() {
                        "octagon" => true,
                        "roundbox" => false,
                        _ => return Err(p.unsupported()),
                    };
                }
                "statestartcolor" => skin.start = Some(p.solid()?),
                "stateendcolor" => skin.end = Some(p.solid()?),
                "statebarcolor" => skin.bar = Some(p.solid()?),
                "statediamondbackgroundcolor" if activity => skin.diamond_fill = Some(p.paint()?),
                "statediamondbordercolor" if activity => skin.diamond_border = Some(p.solid()?),
                "notebackgroundcolor" => skin.note.fill = p.paint()?,
                "notebordercolor" => skin.note.border = p.solid()?,
                "noteborderthickness" => skin.note.thickness = p.number(0.0, 20.0)?,
                _ => {
                    let Some((element, property)) = split_font(&key) else {
                        return Err(p.unsupported());
                    };
                    if element == "stateattribute" {
                        let a = &mut skin.attribute;
                        match property {
                            "fontsize" => a.size = Some(p.number(4.0, 100.0)?),
                            "fontcolor" => a.color = Some(p.solid()?),
                            "fontstyle" => {
                                a.style = Some(font_style(value).ok_or_else(|| p.unsupported())?)
                            }
                            "fontname" => {
                                let face = skin.faces.face(value).ok_or_else(|| p.unsupported())?;
                                named.push((element, face));
                            }
                            _ => return Err(p.unsupported()),
                        }
                        continue;
                    }
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
                        "fontname" => {
                            let face = skin.faces.face(value).ok_or_else(|| p.unsupported())?;
                            named.push((element, face));
                        }
                        "fontstyle" => {
                            let (bold, italic) =
                                font_style(value).ok_or_else(|| p.unsupported())?;
                            let font = font_of(&mut skin, element);
                            (font.bold, font.italic) = (bold, italic);
                        }
                        _ => return Err(p.unsupported()),
                    }
                }
            }
        }
        for element in FONTS {
            // `default` is the fallback applied above, not a font of its own.
            // `font_of` would otherwise hand it the state font and overwrite
            // a size set on the state. A state's words take its font where
            // their own say nothing.
            if element == "default" || element == "stateattribute" {
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
        // Faces, but none where the hand writes every word.
        if skin.sketch {
            skin.faces = Default::default();
        } else {
            for (element, face) in named {
                match element {
                    "stateattribute" => skin.attribute.face = Some(face),
                    _ => font_of(&mut skin, element).face = Some(face),
                }
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
                "statefontstyle" => {
                    let (bold, italic) = font_style(&p.value).ok_or_else(|| p.unsupported())?;
                    (look.font.bold, look.font.italic) = (bold, italic);
                }
                _ => {
                    return Err(Declined::at(
                        p.line,
                        format!(
                            "`{}` for the states of a stereotype is not supported yet",
                            p.written()
                        ),
                    ));
                }
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
    )
    .varied(),
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

/// Bold and italic, as `FontStyle` says them (`bold italic`, `plain`).
fn font_style(value: &str) -> Option<(bool, bool)> {
    let v = value.to_ascii_lowercase();
    let words: Vec<&str> = v.split_whitespace().collect();
    let known = |w: &&str| matches!(*w, "plain" | "bold" | "italic");
    (!words.is_empty() && words.iter().all(known))
        .then(|| (words.contains(&"bold"), words.contains(&"italic")))
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
        assert!(skin(&[("stateRoundCorner<<phase>>", "5")]).is_err());
        let bold = skin(&[("stateFontStyle<<phase>>", "bold")]).unwrap();
        assert!(bold.stereotyped(Some("phase")).state.font.bold);
        // A style for a stereotype is the same (`.foo { state { … } }`).
        let s = skin(&[("class=foo|StateBackGroundColor", "lightblue")]).unwrap();
        assert_eq!(s.stereotyped(Some("Foo")).state.fill, Some(blue));
        let err = skin(&[("class=foo&depth=2|StateBackGroundColor", "red")]).unwrap_err();
        assert!(err.reason.contains("`.foo :depth(2)`"), "{}", err.reason);
        let err = skin(&[("class=foo|StateRoundCorner", "5")]).unwrap_err();
        assert!(
            err.reason.contains("states of a stereotype"),
            "{}",
            err.reason
        );
        let svg = crate::render(
            "@startuml\n<style>\n.foo {\n  state,stateBody {\n    BackGroundColor lightblue;\n  }\n}\n</style>\nstate Main <<foo>> {\n  state Sub\n}\nstate Other\n@enduml\n",
        )
        .unwrap();
        assert_eq!(svg.matches("fill=\"#ADD8E6\"").count(), 1, "{svg}");
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

    /// The words under a state's name, the start, the end and the bars
    /// take what skinparams give them; a face too.
    #[test]
    fn attributes_starts_ends_and_faces() {
        let s = skin(&[("StateFontSize", "15"), ("StateFontColor", "red")]).unwrap();
        let a = s.attribute_font();
        assert_eq!((a.size, a.color), (13.0, Color(255, 0, 0)));
        let s = skin(&[
            ("StateAttributeFontColor", "blue"),
            ("StateAttributeFontSize", "9"),
            ("StateAttributeFontStyle", "italic"),
            ("StateAttributeFontName", "serif"),
            ("StateFontName", "Impact"),
            ("StateStartColor", "MediumBlue"),
            ("StateEndColor", "Red"),
            ("StateBarColor", "SaddleBrown"),
        ])
        .unwrap();
        let a = s.attribute_font();
        assert_eq!((a.size, a.color), (9.0, Color(0, 0, 255)));
        assert!(a.italic && !a.bold);
        assert_eq!(a.face.unwrap().family, crate::text::Family::Serif);
        assert_eq!(s.state.font.face.unwrap().name, Some(1));
        assert_eq!(s.faces.0, ["serif", "Impact"]);
        assert_eq!(s.start, Some(Color(0, 0, 205)));
        assert_eq!(s.end, Some(Color(255, 0, 0)));
        assert_eq!(s.bar, Some(Color(139, 69, 19)));
        let svg = crate::render(
            "@startuml\nskinparam state {\n  StartColor MediumBlue\n  EndColor Red\n}\n[*] --> A\nA --> [*]\n@enduml\n",
        )
        .unwrap();
        assert!(svg.contains("#0000CD") && svg.contains("#FF0000"), "{svg}");
        // A stereotype's words under its name follow its font (found in
        // review, 2026-11-24).
        let s = skin(&[
            ("StateFontSize<<Error>>", "20"),
            ("StateFontColor<<Error>>", "red"),
        ])
        .unwrap();
        let a = s.stereotyped(Some("Error")).attribute_font();
        assert_eq!((a.size, a.color), (18.0, Color(255, 0, 0)));
        // The hand writes every word: no face of an element's own.
        let s = skin(&[("StateFontName", "Impact"), ("handwritten", "true")]).unwrap();
        assert_eq!((s.state.font.face, s.faces.0.len()), (None, 0));
    }

    /// An activity in the old syntax reads its `activity…` skinparams as
    /// the state's, its diamonds' too, and names a key it declines as
    /// written (found in review, 2026-11-24).
    #[test]
    fn an_activity_reads_its_own_keys() {
        let params = |lines: &[(&str, &str)]| -> Vec<Skinparam> {
            lines
                .iter()
                .enumerate()
                .map(|(line, (key, value))| Skinparam {
                    key: (*key).into(),
                    value: (*value).into(),
                    line,
                    declared: None,
                })
                .collect()
        };
        let s = Skin::from_activity_params(&params(&[
            ("ActivityBackgroundColor", "Peru"),
            ("ActivityDiamondBackgroundColor", "#23272f"),
            ("ActivityDiamondBorderColor", "red"),
            ("ActivityBackgroundColor<< Begin >>", "Olive"),
        ]))
        .unwrap();
        assert_eq!(s.state.fill, Some(Color(205, 133, 63)));
        assert_eq!(s.diamond_fill, Some(Some(Color(0x23, 0x27, 0x2f))));
        assert_eq!(s.diamond_border, Some(Color(255, 0, 0)));
        assert_eq!(
            s.stereotyped(Some("begin")).state.fill,
            Some(Color(128, 128, 0))
        );
        let err = Skin::from_activity_params(&params(&[("ActivityNoSuchKey", "1")])).unwrap_err();
        assert!(err.reason.contains("ActivityNoSuchKey"), "{}", err.reason);
        // A state diagram has no diamonds of its own.
        assert!(skin(&[("StateDiamondBackgroundColor", "red")]).is_err());
    }

    #[test]
    fn handwritten_draws_by_hand() {
        assert!(!skin(&[]).unwrap().sketch);
        assert!(skin(&[("handwritten", "true")]).unwrap().sketch);
        assert!(!skin(&[("handwritten", "false")]).unwrap().sketch);
        assert_eq!(skin(&[("handwritten", "maybe")]).unwrap_err().line, Some(0));
    }

    #[test]
    fn declines_what_would_change_the_picture() {
        for (key, value) in [
            ("StateFontName", "a<b"),
            ("StateBackgroundColor", "#red/blue"),
        ] {
            let err = skin(&[("roundCorner", "3"), (key, value)]).unwrap_err();
            assert_eq!(err.line, Some(1), "{key}");
        }
    }
}
