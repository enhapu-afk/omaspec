//! How a structure diagram looks. A skinparam that would change the
//! picture but is not drawn declines the diagram; skinparams of other
//! diagram types are ignored. `class…`, `component…`, `usecase…` and the
//! other boxes of this diagram are its own, so they are not ignored the
//! way a state diagram ignores them.

use super::{Shape, Skinparam};
use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::normalize_key;
use crate::svg::Stroke;
use crate::text::Font;

/// The look of a shape: its fill, its border, its name, its members and
/// the stereotype above the name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub fill: Paint,
    pub border: Color,
    pub thickness: f32,
    pub font: Font,
    pub member: Font,
    pub stereo: Font,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub font_name: String,
    pub monochrome: bool,
    /// `skinparam style strictuml`: an open arrow head on `>`.
    pub strict: bool,
    pub background: Paint,
    /// Corner radius of a box. PlantUML's `RoundCorner` is twice it.
    /// Classes start square; a skinparam rounds them.
    pub round: f32,
    /// `minClassWidth`. Zero means the text decides.
    pub min_class: f32,
    pub class_box: Look,
    pub interface_box: Look,
    pub component: Look,
    pub usecase: Look,
    pub actor: Look,
    pub node: Look,
    pub database: Look,
    pub object: Look,
    pub package: Look,
    pub artifact: Look,
    pub cloud: Look,
    pub folder: Look,
    pub frame: Look,
    pub rectangle: Look,
    pub note: Look,
    pub arrow: Stroke,
    /// How far a line's corners curve; none with `linetype ortho`.
    pub curve: f32,
    pub arrow_font: Font,
    pub title_font: Font,
    pub header_font: Font,
    pub footer_font: Font,
    pub caption_font: Font,
}

impl Skin {
    /// The look of `shape`. Enums, annotations and abstract classes use
    /// the class look; the name of an abstract class is drawn italic
    /// on top of it.
    pub fn of(&self, shape: Shape) -> &Look {
        match shape {
            Shape::Class | Shape::Abstract | Shape::Enum | Shape::Annotation => &self.class_box,
            Shape::Interface => &self.interface_box,
            Shape::Component => &self.component,
            Shape::UseCase => &self.usecase,
            Shape::Actor | Shape::Agent => &self.actor,
            Shape::Node | Shape::Stack | Shape::Hexagon | Shape::Card | Shape::File => &self.node,
            Shape::Database | Shape::Storage => &self.database,
            Shape::Object => &self.object,
            Shape::Package => &self.package,
            Shape::Artifact => &self.artifact,
            Shape::Cloud => &self.cloud,
            Shape::Folder => &self.folder,
            Shape::Frame => &self.frame,
            Shape::Rectangle => &self.rectangle,
            Shape::Note => &self.note,
        }
    }
}

impl Default for Skin {
    fn default() -> Self {
        let black = Color::BLACK;
        let dark = Color::hex(0x181818);
        let gray = Color::hex(0x888888);
        let yellow = Look {
            fill: Some(Color::hex(0xFEFECE)),
            border: dark,
            thickness: 1.0,
            font: Font::new(14.0, black),
            member: Font::new(13.0, black),
            stereo: Font::new(12.0, black).italic(),
        };
        let actor = Look {
            fill: None,
            border: dark,
            ..yellow
        };
        Skin {
            font_name: "sans-serif".into(),
            monochrome: false,
            strict: false,
            background: None,
            round: 0.0,
            min_class: 0.0,
            class_box: yellow,
            interface_box: yellow,
            component: yellow,
            usecase: yellow,
            actor,
            node: yellow,
            database: yellow,
            object: yellow,
            package: Look {
                fill: Some(Color::hex(0xEEEEEE)),
                ..yellow
            },
            artifact: yellow,
            cloud: yellow,
            folder: yellow,
            frame: yellow,
            rectangle: yellow,
            note: Look {
                fill: Some(Color::hex(0xFEFFDD)),
                border: dark,
                thickness: 0.5,
                font: Font::new(13.0, black),
                member: Font::new(13.0, black),
                stereo: Font::new(12.0, black).italic(),
            },
            arrow: Stroke::new(dark, 1.0),
            curve: crate::curve::RADIUS,
            arrow_font: Font::new(12.0, black),
            title_font: Font::new(14.0, black).bold(),
            header_font: Font::new(10.0, gray),
            footer_font: Font::new(10.0, gray),
            caption_font: Font::new(14.0, black),
        }
    }
}

/// Skinparams of other diagram types, by the start of their key.
const OTHER_TYPES: [&str; 22] = [
    "activity",
    "boundary",
    "box",
    "collections",
    "control",
    "entity",
    "genericdisplay",
    "lifeline",
    "maxmessagesize",
    "mindmap",
    "participant",
    "partition",
    "queue",
    "responsemessagebelowarrow",
    "sequence",
    "state",
    "swimlane",
    "timing",
    "wbs",
    "yaml",
    "circledcharacter",
    "legend",
];

/// These change nothing in the SVG.
const NO_EFFECT: [&str; 4] = ["dpi", "nodesep", "ranksep", "svglinktarget"];

/// Boxes and texts that have a font. A longer name is tried first, so
/// `namespace` is not read as something shorter.
const ELEMENTS: [&str; 28] = [
    "rectangle",
    "component",
    "interface",
    "namespace",
    "artifact",
    "database",
    "hexagon",
    "usecase",
    "caption",
    "package",
    "storage",
    "default",
    "folder",
    "header",
    "object",
    "footer",
    "actor",
    "arrow",
    "class",
    "cloud",
    "frame",
    "stack",
    "title",
    "agent",
    "card",
    "file",
    "node",
    "note",
];

impl Skin {
    pub fn from_params(params: &[Skinparam]) -> Result<Skin, Declined> {
        let mut skin = Skin::default();
        let mut font_size = None;
        let mut font_color = None;
        let mut owned = Owned::default();
        for p in params {
            let key = normalize_key(&p.key);
            let value = p.value.trim();

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
                "minclasswidth" => skin.min_class = p.number(0.0, 2000.0)?,
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
                _ => {
                    let Some((element, property)) = split_key(&key) else {
                        return Err(p.unsupported());
                    };
                    if property.contains("fontname") {
                        return Err(p.unsupported());
                    }
                    apply(&mut skin, &mut owned, element, property, value, p)?;
                }
            }
        }
        apply_default(&mut skin, font_size, font_color, &owned);
        Ok(skin)
    }
}

#[derive(Default)]
struct Owned {
    size: Vec<&'static str>,
    color: Vec<&'static str>,
    member: Vec<&'static str>,
    stereo: Vec<&'static str>,
}

fn apply(
    skin: &mut Skin,
    owned: &mut Owned,
    element: &str,
    property: &str,
    value: &str,
    p: &Skinparam,
) -> Result<(), Declined> {
    let box_prop = matches!(
        property,
        "backgroundcolor"
            | "bordercolor"
            | "borderthickness"
            | "attributefontsize"
            | "attributefontcolor"
            | "stereotypefontsize"
            | "stereotypefontcolor"
    );
    // A box or font this diagram does not have, named by the line.
    let no_box = || p.unsupported();
    if box_prop && look_mut(skin, element).is_none() {
        return Err(p.unsupported());
    }
    match property {
        "backgroundcolor" => look_mut(skin, element).ok_or_else(no_box)?.fill = p.paint()?,
        "bordercolor" => look_mut(skin, element).ok_or_else(no_box)?.border = p.solid()?,
        "borderthickness" => {
            look_mut(skin, element).ok_or_else(no_box)?.thickness = p.number(0.0, 20.0)?
        }
        "fontsize" => {
            let size = p.number(4.0, 100.0)?;
            owned.size.push(canonical(element));
            font_mut(skin, element).ok_or_else(no_box)?.size = size;
        }
        "fontcolor" => {
            let c = p.solid()?;
            owned.color.push(canonical(element));
            font_mut(skin, element).ok_or_else(no_box)?.color = c;
        }
        "fontstyle" => {
            set_style(font_mut(skin, element).ok_or_else(no_box)?, value)
                .map_err(|()| p.unsupported())?;
        }
        "attributefontsize" => {
            let size = p.number(4.0, 100.0)?;
            owned.member.push(canonical(element));
            look_mut(skin, element).ok_or_else(no_box)?.member.size = size;
        }
        "attributefontcolor" => {
            look_mut(skin, element).ok_or_else(no_box)?.member.color = p.solid()?
        }
        "attributefontstyle" => {
            set_style(
                &mut look_mut(skin, element).ok_or_else(no_box)?.member,
                value,
            )
            .map_err(|()| p.unsupported())?;
        }
        "stereotypefontsize" => {
            let size = p.number(4.0, 100.0)?;
            owned.stereo.push(canonical(element));
            look_mut(skin, element).ok_or_else(no_box)?.stereo.size = size;
        }
        "stereotypefontcolor" => {
            look_mut(skin, element).ok_or_else(no_box)?.stereo.color = p.solid()?
        }
        "stereotypefontstyle" => {
            set_style(
                &mut look_mut(skin, element).ok_or_else(no_box)?.stereo,
                value,
            )
            .map_err(|()| p.unsupported())?;
        }
        _ => return Err(p.unsupported()),
    }
    Ok(())
}

fn set_style(font: &mut Font, value: &str) -> Result<(), ()> {
    let raw: Vec<String> = value
        .split_whitespace()
        .map(|w| w.to_ascii_lowercase())
        .collect();
    if raw.is_empty()
        || raw
            .iter()
            .any(|w| !matches!(w.as_str(), "plain" | "bold" | "italic"))
    {
        return Err(());
    }
    font.bold = raw.iter().any(|w| w == "bold");
    font.italic = raw.iter().any(|w| w == "italic");
    Ok(())
}

/// `classfontsize` as `("class", "fontsize")`. `namespace` is a package.
fn split_key(key: &str) -> Option<(&'static str, &str)> {
    let mut found: Option<(&'static str, &str)> = None;
    for element in ELEMENTS {
        if let Some(rest) = key.strip_prefix(element)
            && !rest.is_empty()
            && found.is_none_or(|(e, _)| element.len() > e.len())
        {
            found = Some((element, rest));
        }
    }
    found
}

fn canonical(element: &str) -> &'static str {
    if element == "namespace" {
        "package"
    } else {
        ELEMENTS
            .iter()
            .copied()
            .find(|e| *e == element)
            .unwrap_or("class")
    }
}

fn apply_default(
    skin: &mut Skin,
    font_size: Option<f32>,
    font_color: Option<Color>,
    owned: &Owned,
) {
    let boxes = [
        "class",
        "interface",
        "component",
        "usecase",
        "actor",
        "node",
        "database",
        "object",
        "package",
        "artifact",
        "cloud",
        "folder",
        "frame",
        "rectangle",
        "note",
    ];
    for element in boxes {
        let look = look_mut(skin, element).expect("a box");
        if let Some(size) = font_size {
            if !owned.size.contains(&element) {
                look.font.size = size;
            }
            if !owned.member.contains(&element) {
                look.member.size = size;
            }
            if !owned.stereo.contains(&element) {
                look.stereo.size = (size - 2.0).max(8.0);
            }
        }
        if let Some(c) = font_color
            && !owned.color.contains(&element)
        {
            look.font.color = c;
            look.member.color = c;
            look.stereo.color = c;
        }
    }
    if let Some(size) = font_size
        && !owned.size.contains(&"arrow")
    {
        skin.arrow_font.size = size;
    }
    if let Some(c) = font_color
        && !owned.color.contains(&"arrow")
    {
        skin.arrow_font.color = c;
    }
    for (element, font) in [
        ("title", &mut skin.title_font),
        ("header", &mut skin.header_font),
        ("footer", &mut skin.footer_font),
        ("caption", &mut skin.caption_font),
    ] {
        if let Some(size) = font_size
            && !owned.size.contains(&element)
        {
            font.size = size;
        }
        if let Some(c) = font_color
            && !owned.color.contains(&element)
        {
            font.color = c;
        }
    }
}

fn font_mut<'a>(skin: &'a mut Skin, element: &str) -> Option<&'a mut Font> {
    Some(match element {
        "arrow" => &mut skin.arrow_font,
        "title" => &mut skin.title_font,
        "header" => &mut skin.header_font,
        "footer" => &mut skin.footer_font,
        "caption" => &mut skin.caption_font,
        _ => &mut look_mut(skin, element)?.font,
    })
}

fn look_mut<'a>(skin: &'a mut Skin, element: &str) -> Option<&'a mut Look> {
    Some(match element {
        "class" | "enum" | "abstract" | "annotation" => &mut skin.class_box,
        "interface" => &mut skin.interface_box,
        "component" => &mut skin.component,
        "usecase" => &mut skin.usecase,
        "actor" | "agent" => &mut skin.actor,
        "node" | "stack" | "hexagon" | "card" | "file" => &mut skin.node,
        "database" | "storage" => &mut skin.database,
        "object" => &mut skin.object,
        "package" | "namespace" => &mut skin.package,
        "artifact" => &mut skin.artifact,
        "cloud" => &mut skin.cloud,
        "folder" => &mut skin.folder,
        "frame" => &mut skin.frame,
        "rectangle" => &mut skin.rectangle,
        "note" => &mut skin.note,
        _ => return None,
    })
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
    fn class_colors_are_this_diagrams() {
        let s = skin(&[
            ("ClassBackgroundColor", "#1e1e1e"),
            ("ClassBorderColor", "#e68e0d"),
            ("ComponentBackgroundColor", "#333333"),
            ("UsecaseBorderColor", "#e68e0d"),
            ("NodeBackgroundColor", "#1e1e1e"),
            ("ActorBorderColor", "#e68e0d"),
            ("DatabaseBackgroundColor", "#1e1e1e"),
            ("defaultFontColor", "#bebebe"),
            ("defaultFontSize", "16"),
            ("ArrowColor", "#e68e0d"),
            ("NoteBackgroundColor", "#2a2a2a"),
            ("backgroundColor", "#121212"),
            ("roundCorner", "8"),
            ("ParticipantBackgroundColor", "#ff0000"),
            ("StateBackgroundColor", "#ff0000"),
            ("shadowing", "false"),
            ("style", "strictuml"),
            ("minClassWidth", "80"),
        ])
        .unwrap();
        assert_eq!(s.class_box.fill, Some(Color::hex(0x1E1E1E)));
        assert_eq!(s.class_box.border, Color::hex(0xE68E0D));
        assert_eq!(s.class_box.font.size, 16.0);
        assert_eq!(s.class_box.font.color, Color::hex(0xBEBEBE));
        assert_eq!(s.component.fill, Some(Color::hex(0x333333)));
        assert_eq!(s.usecase.border, Color::hex(0xE68E0D));
        assert_eq!(s.node.fill, Some(Color::hex(0x1E1E1E)));
        assert_eq!(s.actor.border, Color::hex(0xE68E0D));
        assert_eq!(s.database.fill, Some(Color::hex(0x1E1E1E)));
        assert_eq!(s.arrow.color, Color::hex(0xE68E0D));
        assert_eq!(s.note.fill, Some(Color::hex(0x2A2A2A)));
        assert_eq!(s.background, Some(Color::hex(0x121212)));
        assert_eq!(s.round, 4.0);
        assert_eq!(s.min_class, 80.0);
        assert!(s.strict);
    }

    #[test]
    fn declines_what_would_change_the_picture() {
        for (key, value) in [
            ("handwritten", "true"),
            ("shadowing", "true"),
            ("ClassFontName", "Courier"),
            ("ClassBackgroundColor", "#red/blue"),
            ("defaultFontStyle", "bold"),
        ] {
            let err = skin(&[(key, value)]).unwrap_err();
            assert_eq!(err.line, Some(0), "{key}");
        }
    }
}
