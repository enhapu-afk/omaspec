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
use crate::style::{Element, Prop};
use crate::svg::{Align, Stroke};
use crate::text::Font;

/// What a `<style>` sets on a box.
const BOX: &[Prop] = &[
    Prop::BackGroundColor,
    Prop::LineColor,
    Prop::LineThickness,
    Prop::FontColor,
    Prop::FontSize,
    Prop::FontStyle,
    Prop::RoundCorner,
];

const ARROW: &[(Prop, &str)] = &[
    (Prop::LineColor, "ArrowColor"),
    (Prop::LineThickness, "ArrowThickness"),
    (Prop::LineStyle, "ArrowStyle"),
];

/// What a `<style>` reaches in a mind map: every box (`node`), the root
/// and the leaves among them, a label without a box, the links; each box
/// by its stereotype (`.green`) and its depth (`:depth(2)`) too.
pub const MINDMAP_STYLE: &[Element] = &[
    Element::new(&["element", "mindmapDiagram", "node"], "Node", BOX).varied(),
    Element::new(
        &["element", "mindmapDiagram", "node", "rootNode"],
        "RootNode",
        BOX,
    )
    .varied(),
    Element::new(
        &["element", "mindmapDiagram", "node", "leafNode"],
        "LeafNode",
        BOX,
    )
    .varied(),
    Element::new(
        &["element", "mindmapDiagram", "node", "boxless"],
        "Boxless",
        crate::style::TEXT,
    )
    .varied(),
    Element::keys(&["element", "mindmapDiagram", "arrow"], ARROW),
];

/// The same in a WBS.
pub const WBS_STYLE: &[Element] = &[
    Element::new(&["element", "wbsDiagram", "node"], "Node", BOX).varied(),
    Element::new(
        &["element", "wbsDiagram", "node", "rootNode"],
        "RootNode",
        BOX,
    )
    .varied(),
    Element::new(
        &["element", "wbsDiagram", "node", "leafNode"],
        "LeafNode",
        BOX,
    )
    .varied(),
    Element::new(
        &["element", "wbsDiagram", "node", "boxless"],
        "Boxless",
        crate::style::TEXT,
    )
    .varied(),
    Element::keys(&["element", "wbsDiagram", "arrow"], ARROW),
];

/// Which boxes a setting is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum When {
    Root,
    /// A box with none below it.
    Leaf,
    /// A label without a box (`_`).
    Boxless,
    /// The root is 0.
    Depth(usize),
    /// A stereotype, in lower case.
    Class(String),
}

/// What a setting changes on a box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Change {
    Fill(Paint),
    Border(Color),
    Thickness(f32),
    Round(f32),
    FontColor(Color),
    FontSize(f32),
    FontStyle { bold: bool, italic: bool },
}

/// One setting of the boxes: for those every [`When`] holds for, all of
/// them when there is none.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub when: Vec<When>,
    pub change: Change,
}

/// Where a box stands in its tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot<'a> {
    pub depth: usize,
    pub leaf: bool,
    pub boxless: bool,
    pub class: Option<&'a str>,
}

/// How one box looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub fill: Paint,
    pub border: Color,
    pub thickness: f32,
    pub round: f32,
    pub font: Font,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeKind {
    MindMap,
    Wbs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Skin {
    pub font_name: String,
    pub monochrome: bool,
    /// `skinparam handwritten true`: shapes drawn by hand (docs/adr/0023).
    pub sketch: bool,
    /// `skinparam shadowing true`: the boxes cast a shadow.
    pub shadow: bool,
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
    /// The settings of the boxes in the order written: a later one
    /// counts over an earlier one, whichever boxes each is for.
    pub rules: Vec<Rule>,
}

/// `value` as a font style: `bold`, `italic`, `plain`.
fn font_style(p: &Skinparam) -> Result<(bool, bool), Declined> {
    let v = p.value.trim().to_ascii_lowercase();
    let words: Vec<&str> = v.split_whitespace().collect();
    if words.is_empty()
        || words
            .iter()
            .any(|w| !matches!(*w, "plain" | "bold" | "italic"))
    {
        return Err(p.unsupported());
    }
    Ok((words.contains(&"bold"), words.contains(&"italic")))
}

/// What the property `name` of a box, a skinparam's name without the
/// box's (`backgroundcolor`), changes; `None` for one the boxes do not
/// take.
fn change(name: &str, p: &Skinparam) -> Result<Option<Change>, Declined> {
    Ok(Some(match name {
        "backgroundcolor" => Change::Fill(p.paint()?),
        "bordercolor" => Change::Border(p.solid()?),
        "borderthickness" => Change::Thickness(p.number(0.0, 20.0)?),
        "roundcorner" => Change::Round(p.corner()?),
        "fontcolor" => Change::FontColor(p.solid()?),
        "fontsize" => Change::FontSize(p.number(4.0, 100.0)?),
        "fontstyle" => {
            let (bold, italic) = font_style(p)?;
            Change::FontStyle { bold, italic }
        }
        _ => return Ok(None),
    }))
}

impl Skin {
    /// How the box at `spot` looks: the boxes' own look, then every
    /// setting that is for it, in the order written.
    pub fn look(&self, spot: Spot) -> Look {
        let mut look = Look {
            fill: self.fill,
            border: self.border,
            thickness: self.thickness,
            round: self.round,
            font: self.font,
        };
        let holds = |when: &When| match when {
            When::Root => spot.depth == 0,
            When::Leaf => spot.leaf,
            When::Boxless => spot.boxless,
            When::Depth(depth) => spot.depth == *depth,
            When::Class(class) => spot.class.is_some_and(|c| c.eq_ignore_ascii_case(class)),
        };
        for rule in self.rules.iter().filter(|r| r.when.iter().all(holds)) {
            match rule.change {
                Change::Fill(fill) => look.fill = fill,
                Change::Border(color) => look.border = color,
                Change::Thickness(width) => look.thickness = width,
                Change::Round(round) => look.round = round,
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
            sketch: false,
            shadow: false,
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
            rules: Vec::new(),
        }
    }

    pub fn from_params(params: &[Skinparam], kind: TreeKind) -> Result<Skin, Declined> {
        let mut skin = Skin::defaults(kind);
        for p in params {
            // A style for a stereotype or a depth says so before the
            // name: `class=green&depth=2|NodeBackgroundColor`.
            let (only, name) = match p.key.split_once('|') {
                Some((only, name)) => (only, name),
                None => ("", p.key.as_str()),
            };
            let key = normalize_key(name);
            let value = p.value.trim();
            let mut when: Vec<When> = only
                .split('&')
                .filter_map(|one| match one.split_once('=')? {
                    ("class", class) => Some(When::Class(class.to_owned())),
                    ("depth", depth) => depth.parse().ok().map(When::Depth),
                    _ => None,
                })
                .collect();
            let part = [
                ("rootnode", Some(When::Root)),
                ("leafnode", Some(When::Leaf)),
                ("boxless", Some(When::Boxless)),
                ("node", None),
            ]
            .into_iter()
            .find_map(|(prefix, part)| Some((key.strip_prefix(prefix)?, part)));
            // The boxes a setting is for, when it is not for all of them.
            if let Some((property, part)) = part
                && (part.is_some() || !when.is_empty())
            {
                when.extend(part);
                match change(property, p)? {
                    Some(change) => skin.rules.push(Rule { when, change }),
                    None => return Err(p.unsupported()),
                }
                continue;
            }
            // For all of them: in its place among the others, so one
            // written after a stereotype's counts over it.
            let property = match key.as_str() {
                "roundcorner" => Some("roundcorner"),
                "defaultfontsize" => Some("fontsize"),
                "defaultfontcolor" => Some("fontcolor"),
                key => key.strip_prefix("node"),
            };
            if let Some(change) = property.map(|name| change(name, p)).transpose()?.flatten() {
                skin.rules.push(Rule {
                    when: Vec::new(),
                    change,
                });
            }
            let style = |font: &mut Font| -> Result<(), Declined> {
                (font.bold, font.italic) = font_style(p)?;
                Ok(())
            };
            match key.as_str() {
                "monochrome" => skin.monochrome = p.flag()?,
                "handwritten" => skin.sketch = p.flag()?,
                "shadowing" | "nodeshadowing" => skin.shadow = p.flag()?,
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
            ("NodeFontName", "Courier"),
            ("wrapWidth", "100"),
            ("ArrowStyle", "bold"),
            ("NodeBackgroundColor", "#red/blue"),
        ] {
            let err = skin(TreeKind::Wbs, &[("roundCorner", "3"), (key, value)]).unwrap_err();
            assert_eq!(err.line, Some(1), "{key}");
        }
    }

    #[test]
    fn a_style_reaches_a_box_by_its_place_its_stereotype_and_its_depth() {
        let source = "@startmindmap\n<style>\nmindmapDiagram {\n  node { BackgroundColor #00ff00 }\n  :depth(1) { BackgroundColor #ffffff }\n  rootNode { FontStyle bold }\n  leafNode { FontColor #000080 }\n  .urgent { BackgroundColor #ff0000; RoundCorner 0 }\n  arrow { LineColor #0000ff }\n}\n</style>\n* Root\n** First\n*** Leaf <<urgent>>\n*** Other\n@endmindmap\n";
        let Ok(crate::Diagram::MindMap(map)) = crate::parse(source) else {
            panic!("a mind map");
        };
        let skin = Skin::from_params(&map.frame.skinparams, TreeKind::MindMap).unwrap();
        assert_eq!(skin.link.color, Color::hex(0x0000ff));
        let at = |depth, leaf, class| {
            skin.look(Spot {
                depth,
                leaf,
                boxless: false,
                class,
            })
        };
        let root = at(0, false, None);
        assert!(root.font.bold && root.fill == Some(Color::hex(0x00ff00)));
        assert_eq!(at(1, false, None).fill, Some(Color::hex(0xffffff)));
        let other = at(2, true, None);
        assert_eq!(other.fill, Some(Color::hex(0x00ff00)));
        assert_eq!(other.font.color, Color::hex(0x000080));
        let urgent = at(2, true, Some("urgent"));
        assert_eq!(
            (urgent.fill, urgent.round),
            (Some(Color::hex(0xff0000)), 0.0)
        );
        // The stereotype is read off the label, which keeps its space
        // before it, as PlantUML measures it.
        let leaf = &map.maps[0].right[0].children[0].node;
        assert_eq!(
            (leaf.text.value.as_str(), leaf.class.as_deref()),
            ("Leaf ", Some("urgent"))
        );
        // A setting for every box written later counts over the stereotype's.
        let later = "@startwbs\n<style>\n.done { BackgroundColor #ff0000 }\nnode { BackgroundColor #00ff00 }\n</style>\n* A <<done>>\n@endwbs\n";
        let Ok(crate::Diagram::Wbs(wbs)) = crate::parse(later) else {
            panic!("a WBS");
        };
        let skin = Skin::from_params(&wbs.frame.skinparams, TreeKind::Wbs).unwrap();
        assert_eq!(at_done(&skin).fill, Some(Color::hex(0x00ff00)));
        // In a diagram whose boxes tell no stereotype apart it declines.
        let scenario =
            "@startuml\n<style>\n.done { BackgroundColor red }\n</style>\nA -> B\n@enduml\n";
        assert!(crate::parse(scenario).is_err());
    }

    fn at_done(skin: &Skin) -> Look {
        skin.look(Spot {
            depth: 0,
            leaf: true,
            boxless: false,
            class: Some("done"),
        })
    }
}
