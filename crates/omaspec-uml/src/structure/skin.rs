//! How a structure diagram looks. A skinparam that would change the
//! picture but is not drawn declines the diagram; skinparams of other
//! diagram types are ignored. `class…`, `component…`, `usecase…` and the
//! other boxes of this diagram are its own, so they are not ignored the
//! way a state diagram ignores them.

use super::{Shape, Skinparam};
use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::normalize_key;
use crate::style::{Element, Prop};
use crate::svg::Stroke;
use crate::text::Font;

/// The mark of a component box (`skinparam componentStyle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComponentStyle {
    /// UML 2: a small component icon at the top right.
    #[default]
    Uml2,
    /// UML 1: two small boxes across the left edge.
    Uml1,
    /// A plain box.
    Rectangle,
}

/// The look of a shape: its fill, its border, its name, its members and
/// the stereotype above the name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub fill: Paint,
    /// The gradient the fill runs as (`Wheat|CornflowerBlue`), where it is
    /// one; [`Self::fill`] is then its first color.
    pub gradient: Option<crate::color::Gradient>,
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
    /// `skinparam handwritten true`: shapes drawn by hand (docs/adr/0023).
    pub sketch: bool,
    /// `skinparam shadowing true`: the boxes cast a shadow.
    pub shadow: bool,
    /// `skinparam defaultTextAlignment left`: what a box says starts at
    /// its left edge, a card's title over its list.
    pub left: bool,
    /// `skinparam style strictuml`: an open arrow head on `>`, and no
    /// circle before a class's name.
    pub strict: bool,
    pub background: Paint,
    /// The gradient the page runs as (`AntiqueWhite/Gold`), where it is
    /// one; [`Self::background`] is then its first color.
    pub page_gradient: Option<crate::color::Gradient>,
    /// Corner radius of a box. PlantUML's `RoundCorner` is twice it.
    /// Classes start square; a skinparam rounds them.
    pub round: f32,
    /// `minClassWidth`. Zero means the text decides.
    pub min_class: f32,
    /// `groupInheritance`: from how many lines up into one box under a
    /// triangle (or alike) those lines join into one stem; 0 never.
    pub group_inheritance: usize,
    /// `skinparam componentStyle`: the mark a component box carries.
    pub component_style: ComponentStyle,
    /// `skinparam packageStyle`: the shape a package is drawn as, when
    /// another than its tab (`rectangle`, `folder`, `frame`, `node`,
    /// `cloud`, `database`).
    pub package_style: Option<Shape>,
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
    /// The circles before the names of classes.
    pub spots: super::spot::Spots,
    /// What the styles set for the boxes of a stereotype.
    pub rules: Vec<Rule>,
    pub arrow: Stroke,
    /// `skinparam ArrowHeadColor`, a style's `HeadColor`: the heads of
    /// the arrows in the diagram's arrow color; one with a color of its
    /// own keeps it in its head, as in PlantUML.
    pub arrow_head: Option<Color>,
    /// How far a line's corners curve; none with `linetype ortho`.
    pub curve: f32,
    pub arrow_font: Font,
    pub title_font: Font,
    pub header_font: Font,
    pub footer_font: Font,
    pub caption_font: Font,
}

/// What a stereotype's style changes on a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Change {
    Fill(Paint, Option<crate::color::Gradient>),
    Border(Color),
    Thickness(f32),
    FontColor(Color),
    FontSize(f32),
    FontStyle { bold: bool, italic: bool },
}

/// One setting for the boxes of a stereotype: those of one kind
/// (`class`, `node`), as the skinparams name the kinds.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    /// The stereotype, in lower case.
    pub class: String,
    pub kind: &'static str,
    pub change: Change,
}

/// The kind a shape's look is kept under.
fn kind_of(shape: Shape) -> &'static str {
    match shape {
        Shape::Class
        | Shape::Abstract
        | Shape::Enum
        | Shape::Entity
        | Shape::Annotation
        | Shape::Point
        | Shape::Diamond => "class",
        Shape::Interface | Shape::Lollipop => "interface",
        Shape::Component => "component",
        Shape::UseCase => "usecase",
        Shape::Actor | Shape::Agent => "actor",
        Shape::Person => "person",
        Shape::Queue => "queue",
        Shape::Node | Shape::Stack | Shape::Hexagon | Shape::Card | Shape::File => "node",
        Shape::Database | Shape::Storage => "database",
        Shape::Object => "object",
        Shape::Package => "package",
        Shape::Artifact => "artifact",
        Shape::Cloud => "cloud",
        Shape::Folder => "folder",
        Shape::Frame => "frame",
        Shape::Rectangle => "rectangle",
        Shape::Note => "note",
    }
}

/// The kind a skinparam's element belongs to: `storage` is a database.
fn kind_named(element: &str) -> Option<&'static str> {
    Some(match element {
        "class" | "enum" | "abstract" | "annotation" => "class",
        "interface" => "interface",
        "component" => "component",
        "usecase" => "usecase",
        "actor" | "agent" => "actor",
        "person" => "person",
        "queue" => "queue",
        "node" | "stack" | "hexagon" | "card" | "file" => "node",
        "database" | "storage" => "database",
        "object" => "object",
        "package" | "namespace" => "package",
        "artifact" => "artifact",
        "cloud" => "cloud",
        "folder" => "folder",
        "frame" => "frame",
        "rectangle" => "rectangle",
        "note" => "note",
        _ => return None,
    })
}

impl Skin {
    /// The look of a box of `shape` with `stereotypes`: its kind's, and
    /// over it what the styles set for its stereotypes, in their order.
    pub fn look<'a>(
        &self,
        shape: Shape,
        stereotypes: impl Iterator<Item = &'a str> + Clone,
    ) -> Look {
        let mut look = *self.of(shape);
        let kind = kind_of(shape);
        for rule in &self.rules {
            if rule.kind != kind
                || !stereotypes
                    .clone()
                    .any(|s| s.trim().eq_ignore_ascii_case(&rule.class))
            {
                continue;
            }
            match rule.change {
                Change::Fill(fill, gradient) => {
                    look.fill = fill;
                    look.gradient = gradient;
                }
                Change::Border(color) => look.border = color,
                Change::Thickness(width) => look.thickness = width,
                Change::FontColor(color) => look.font.color = color,
                Change::FontSize(size) => look.font.size = size,
                Change::FontStyle { bold, italic } => {
                    look.font.bold = bold;
                    look.font.italic = italic;
                }
            }
        }
        look
    }

    /// The look of `shape`. Enums, annotations and abstract classes use
    /// the class look; the name of an abstract class is drawn italic
    /// on top of it.
    pub fn of(&self, shape: Shape) -> &Look {
        match shape {
            Shape::Class
            | Shape::Abstract
            | Shape::Enum
            | Shape::Entity
            | Shape::Annotation
            | Shape::Point
            | Shape::Diamond => &self.class_box,
            Shape::Interface | Shape::Lollipop => &self.interface_box,
            Shape::Component => &self.component,
            Shape::UseCase => &self.usecase,
            Shape::Actor | Shape::Agent => &self.actor,
            Shape::Person => &self.rectangle,
            Shape::Queue => &self.database,
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
            gradient: None,
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
            font_name: crate::FACE.into(),
            monochrome: false,
            sketch: false,
            shadow: false,
            left: false,
            strict: false,
            background: None,
            page_gradient: None,
            round: 0.0,
            min_class: 0.0,
            group_inheritance: 0,
            component_style: ComponentStyle::Uml2,
            package_style: None,
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
                gradient: None,
                border: dark,
                thickness: 0.5,
                font: Font::new(13.0, black),
                member: Font::new(13.0, black),
                stereo: Font::new(12.0, black).italic(),
            },
            spots: super::spot::Spots::default(),
            rules: Vec::new(),
            arrow: Stroke::new(dark, 1.0),
            arrow_head: None,
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
const OTHER_TYPES: [&str; 21] = [
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
    "legend",
];

/// What a `<style>` sets on a box of a structure diagram.
const BOX: &[Prop] = &[
    Prop::BackGroundColor,
    Prop::LineColor,
    Prop::LineThickness,
    Prop::FontColor,
    Prop::FontSize,
    Prop::FontStyle,
];

/// A box by its style name, under each of the diagrams PlantUML calls a
/// structure diagram, with the start of its skinparams.
macro_rules! boxed {
    ($name:literal, $prefix:literal) => {
        Element::new(
            &[
                "element",
                "classDiagram",
                "componentDiagram",
                "objectDiagram",
                $name,
            ],
            $prefix,
            BOX,
        )
        .varied()
    };
}

/// A kind of class's circle, as styles reach it: under `element` and
/// `spot`, not under `classDiagram`, as PlantUML looks it up.
macro_rules! spotted {
    ($name:literal, $fill:literal, $line:literal, $font:literal) => {
        Element::keys(
            &["element", "spot", $name],
            &[
                (Prop::BackGroundColor, $fill),
                (Prop::LineColor, $line),
                (Prop::FontColor, $font),
            ],
        )
    };
}

/// What a `<style>` reaches in a structure diagram: each kind of box
/// with its fill, its border and its font, and the lines; a box by its
/// stereotype too (`.important`, `<<important>>` on the box).
pub const STYLE: &[Element] = &[
    boxed!("class", "Class"),
    boxed!("interface", "Interface"),
    boxed!("component", "Component"),
    boxed!("usecase", "Usecase"),
    boxed!("actor", "Actor"),
    boxed!("node", "Node"),
    boxed!("database", "Database"),
    boxed!("object", "Object"),
    boxed!("package", "Package"),
    boxed!("artifact", "Artifact"),
    boxed!("cloud", "Cloud"),
    boxed!("folder", "Folder"),
    boxed!("frame", "Frame"),
    boxed!("rectangle", "Rectangle"),
    boxed!("note", "Note"),
    boxed!("storage", "Storage"),
    boxed!("card", "Card"),
    boxed!("file", "File"),
    boxed!("stack", "Stack"),
    boxed!("hexagon", "Hexagon"),
    boxed!("agent", "Agent"),
    spotted!(
        "spotClass",
        "SpotClassBackgroundColor",
        "SpotClassBorderColor",
        "SpotClassFontColor"
    ),
    spotted!(
        "spotAbstractClass",
        "SpotAbstractClassBackgroundColor",
        "SpotAbstractClassBorderColor",
        "SpotAbstractClassFontColor"
    ),
    spotted!(
        "spotInterface",
        "SpotInterfaceBackgroundColor",
        "SpotInterfaceBorderColor",
        "SpotInterfaceFontColor"
    ),
    spotted!(
        "spotEnum",
        "SpotEnumBackgroundColor",
        "SpotEnumBorderColor",
        "SpotEnumFontColor"
    ),
    spotted!(
        "spotAnnotation",
        "SpotAnnotationBackgroundColor",
        "SpotAnnotationBorderColor",
        "SpotAnnotationFontColor"
    ),
    spotted!(
        "spotEntity",
        "SpotEntityBackgroundColor",
        "SpotEntityBorderColor",
        "SpotEntityFontColor"
    ),
    spotted!(
        "spotProtocol",
        "SpotProtocolBackgroundColor",
        "SpotProtocolBorderColor",
        "SpotProtocolFontColor"
    ),
    spotted!(
        "spotStruct",
        "SpotStructBackgroundColor",
        "SpotStructBorderColor",
        "SpotStructFontColor"
    ),
    spotted!(
        "spotException",
        "SpotExceptionBackgroundColor",
        "SpotExceptionBorderColor",
        "SpotExceptionFontColor"
    ),
    spotted!(
        "spotMetaClass",
        "SpotMetaClassBackgroundColor",
        "SpotMetaClassBorderColor",
        "SpotMetaClassFontColor"
    ),
    spotted!(
        "spotRecord",
        "SpotRecordBackgroundColor",
        "SpotRecordBorderColor",
        "SpotRecordFontColor"
    ),
    spotted!(
        "spotDataClass",
        "SpotDataClassBackgroundColor",
        "SpotDataClassBorderColor",
        "SpotDataClassFontColor"
    ),
    Element::keys(&["element", "spot"], &[(Prop::FontColor, "SpotFontColor")]),
    Element::keys(
        &[
            "element",
            "classDiagram",
            "componentDiagram",
            "objectDiagram",
            "arrow",
        ],
        &[
            (Prop::LineColor, "ArrowColor"),
            (Prop::HeadColor, "ArrowHeadColor"),
            (Prop::LineThickness, "ArrowThickness"),
            (Prop::LineStyle, "ArrowStyle"),
            (Prop::FontColor, "ArrowFontColor"),
            (Prop::FontSize, "ArrowFontSize"),
            (Prop::FontStyle, "ArrowFontStyle"),
        ],
    ),
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
            // A style for a stereotype says so before the name:
            // `class=hot|ClassBackgroundColor`.
            if let Some((only, name)) = p.key.split_once('|') {
                let key = normalize_key(name);
                let class = only.strip_prefix("class=").filter(|c| !c.contains('&'));
                let found = split_key(&key)
                    .and_then(|(element, property)| Some((kind_named(element)?, property)));
                let (Some(class), Some((kind, property))) = (class, found) else {
                    return Err(p.unsupported());
                };
                let change = match property {
                    "backgroundcolor" => {
                        let (fill, gradient) = p.fill()?;
                        Change::Fill(fill, gradient)
                    }
                    "bordercolor" => Change::Border(p.solid()?),
                    "borderthickness" => Change::Thickness(p.number(0.0, 20.0)?),
                    "fontcolor" => Change::FontColor(p.solid()?),
                    "fontsize" => Change::FontSize(p.number(4.0, 100.0)?),
                    "fontstyle" => {
                        let mut font = Font::new(1.0, Color::BLACK);
                        set_style(&mut font, value).map_err(|()| p.unsupported())?;
                        Change::FontStyle {
                            bold: font.bold,
                            italic: font.italic,
                        }
                    }
                    _ => return Err(p.unsupported()),
                };
                skin.rules.push(Rule {
                    class: class.to_owned(),
                    kind,
                    change,
                });
                continue;
            }
            // A style from further up (`root`, `document`) reaches the
            // main frame too, whose words this skin has no font for.
            if p.declared.is_some() && key.starts_with("mainframe") {
                continue;
            }
            if let Some(read) = super::spot::read(&mut skin.spots, &key, p) {
                read?;
                continue;
            }
            match key.as_str() {
                "monochrome" => skin.monochrome = p.flag()?,
                "handwritten" => skin.sketch = p.flag()?,
                "shadowing" => skin.shadow = p.flag()?,
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
                "defaulttextalignment" => {
                    skin.left = match value.to_ascii_lowercase().as_str() {
                        "left" => true,
                        "center" => false,
                        _ => return Err(p.unsupported()),
                    };
                }
                "roundcorner" => skin.round = p.corner()?,
                "minclasswidth" => skin.min_class = p.number(0.0, 2000.0)?,
                "groupinheritance" => {
                    let count = p.number(0.0, 1000.0)?;
                    if count.fract() != 0.0 {
                        return Err(p.unsupported());
                    }
                    skin.group_inheritance = count as usize;
                }
                "componentstyle" => {
                    skin.component_style = match value.to_ascii_lowercase().as_str() {
                        "uml2" => ComponentStyle::Uml2,
                        "uml1" => ComponentStyle::Uml1,
                        "rectangle" => ComponentStyle::Rectangle,
                        _ => return Err(p.unsupported()),
                    };
                }
                "packagestyle" => {
                    skin.package_style = match value.to_ascii_lowercase().as_str() {
                        "package" => None,
                        "rectangle" | "rect" => Some(Shape::Rectangle),
                        "folder" => Some(Shape::Folder),
                        "frame" => Some(Shape::Frame),
                        "node" => Some(Shape::Node),
                        "cloud" => Some(Shape::Cloud),
                        "database" => Some(Shape::Database),
                        _ => return Err(p.unsupported()),
                    };
                }
                // The members' marks are written as `+ - # ~` here at any
                // size, as PlantUML writes them at size 0.
                "classattributeiconsize" => {
                    p.number(0.0, 100.0)?;
                }
                // How PlantUML lays out and keeps circle labels clear: the
                // layout here does both its own way.
                "svek" | "fixcirclelabeloverlapping" => {
                    p.flag()?;
                }
                "backgroundcolor" => (skin.background, skin.page_gradient) = p.fill()?,
                "arrowcolor" => skin.arrow.color = p.solid()?,
                "arrowheadcolor" => skin.arrow_head = Some(p.solid()?),
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
        if let Some(color) = font_color
            && !skin.spots.letter_owned
        {
            skin.spots.letter = color;
        }
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
        "backgroundcolor" => {
            let look = look_mut(skin, element).ok_or_else(no_box)?;
            (look.fill, look.gradient) = p.fill()?;
        }
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

    /// REQ-055: the styles of a component and a package, and what
    /// changes nothing here, are read; a style no shape draws declines.
    #[test]
    fn the_styles_of_components_and_packages() {
        let s = skin(&[
            ("componentStyle", "rectangle"),
            ("packageStyle", "Folder"),
            ("classAttributeIconSize", "0"),
            ("svek", "true"),
            ("FixCircleLabelOverlapping", "true"),
        ])
        .unwrap();
        assert_eq!(s.component_style, ComponentStyle::Rectangle);
        assert_eq!(s.package_style, Some(Shape::Folder));
        let s = skin(&[("componentStyle", "uml1"), ("packageStyle", "package")]).unwrap();
        assert_eq!(
            (s.component_style, s.package_style),
            (ComponentStyle::Uml1, None)
        );
        for (key, value) in [("componentStyle", "fancy"), ("packageStyle", "hexagon")] {
            assert!(skin(&[(key, value)]).is_err(), "{key} {value}");
        }
        // Drawn: a plain component box has no icon, a package is a folder.
        let svg = |style: &str| {
            crate::render(&format!(
                "@startuml\nskinparam componentStyle {style}\n[Web]\n@enduml\n"
            ))
            .unwrap()
        };
        assert!(svg("rectangle").matches("<rect").count() < svg("uml2").matches("<rect").count());
    }

    #[test]
    fn declines_what_would_change_the_picture() {
        for (key, value) in [
            ("ClassFontName", "Courier"),
            ("ClassBorderColor", "#red/blue"),
            ("ClassBackgroundColor", "#red/nocolor"),
            ("defaultFontStyle", "bold"),
        ] {
            let err = skin(&[(key, value)]).unwrap_err();
            assert_eq!(err.line, Some(0), "{key}");
        }
        // A fill may run as a gradient; its first color is the plain fill.
        let s = skin(&[("ClassBackgroundColor", "#red/blue")]).unwrap();
        assert_eq!(s.class_box.fill, Some(Color::hex(0xFF0000)));
        assert!(s.class_box.gradient.is_some());
    }

    #[test]
    fn a_style_colors_the_boxes_and_the_lines() {
        let source = "@startuml\n<style>\nclassDiagram {\n  class { BackgroundColor #ddeeff; LineColor #000080 }\n  arrow { LineColor #ff0000; LineThickness 2 }\n}\ncomponentDiagram {\n  database { BackgroundColor #ffd700 }\n}\n</style>\nclass Order\nOrder --> Line\n@enduml\n";
        let Ok(crate::Diagram::Structure(diagram)) = crate::parse(source) else {
            panic!("a structure diagram");
        };
        let skin = Skin::from_params(&diagram.frame.skinparams).unwrap();
        assert_eq!(skin.class_box.fill, Some(Color::hex(0xddeeff)));
        assert_eq!(skin.class_box.border, Color::hex(0x000080));
        assert_eq!(skin.database.fill, Some(Color::hex(0xffd700)));
        assert_eq!(
            (skin.arrow.color, skin.arrow.width),
            (Color::hex(0xff0000), 2.0)
        );
        // A stereotype's style is for the boxes that carry it.
        let hot = "@startuml\n<style>\n.hot { BackgroundColor #ff0000; FontStyle bold }\nclass { BackgroundColor #00ff00 }\n</style>\nclass A <<hot>>\nclass B\n[C] <<hot>>\n@enduml\n";
        let Ok(crate::Diagram::Structure(diagram)) = crate::parse(hot) else {
            panic!("a structure diagram");
        };
        let skin = Skin::from_params(&diagram.frame.skinparams).unwrap();
        let a = skin.look(Shape::Class, ["hot"].into_iter());
        assert!(a.fill == Some(Color::hex(0xff0000)) && a.font.bold);
        let b = skin.look(Shape::Class, [].into_iter());
        assert!(b.fill == Some(Color::hex(0x00ff00)) && !b.font.bold);
        let c = skin.look(Shape::Component, ["Hot"].into_iter());
        assert_eq!(c.fill, Some(Color::hex(0xff0000)));
        // What a box does not take, and a depth, decline on their line.
        for (sheet, line) in [
            ("class { RoundCorner 9 }", 2),
            (":depth(1) { BackgroundColor red }", 2),
        ] {
            let source = format!("@startuml\n<style>\n{sheet}\n</style>\nclass A\n@enduml\n");
            let err = crate::render(&source).unwrap_err();
            assert_eq!(err.line, Some(line), "{sheet}: {err:?}");
        }
    }
}
