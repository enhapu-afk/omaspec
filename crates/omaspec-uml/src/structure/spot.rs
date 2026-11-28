//! The circles before the names of a class diagram's classes, as PlantUML
//! colors them: each kind its own (`C` green, `I` violet, `A` cyan, `E`
//! orange, `@` red), set by `skinparam stereotypeCBackgroundColor` and
//! its kin for the five that have one, by a style for any
//! (`spot { spotEntity { BackgroundColor … } }`), and gone under
//! `skinparam style strictuml`. The letter takes the diagram's text
//! color, as in PlantUML; `circledCharacterRadius` and
//! `circledCharacterFontSize` size the circle and its letter. The
//! letter is set in the diagram's face, bold, so `circledCharacterFontName`
//! and `circledCharacterFontStyle` are taken and change nothing.

use super::{Skinparam, SpotKind};
use crate::Declined;
use crate::color::Color;

/// How the circles look.
#[derive(Debug, Clone, PartialEq)]
pub struct Spots {
    /// Each kind's fill, by [`SpotKind::index`].
    pub fill: [Color; 12],
    /// Each kind's border.
    pub border: [Color; 12],
    pub letter: Color,
    /// A kind's own letter color, set by its style (`spotStruct {
    /// FontColor … }`); else [`Self::letter`].
    pub letters: [Option<Color>; 12],
    /// A style set the letter's color: the diagram's text color does not.
    pub letter_owned: bool,
    pub radius: f32,
    /// The letter's size.
    pub size: f32,
}

impl Default for Spots {
    fn default() -> Self {
        let fill = SpotKind::ALL.map(|kind| {
            Color::hex(match kind {
                SpotKind::Class | SpotKind::Entity => 0xADD1B2,
                SpotKind::Abstract => 0xA9DCDF,
                SpotKind::Interface => 0xB4A7E5,
                SpotKind::Enum => 0xEB937F,
                SpotKind::Annotation => 0xE3664A,
                SpotKind::Protocol | SpotKind::Struct => 0xF1F1F1,
                SpotKind::Exception => 0xD94321,
                SpotKind::MetaClass => 0xCCCCCC,
                SpotKind::Record => 0xFF8F00,
                SpotKind::DataClass => 0x7E57C2,
            })
        });
        Spots {
            fill,
            border: [Color::hex(0x181818); 12],
            letter: Color::BLACK,
            letters: [None; 12],
            letter_owned: false,
            radius: 11.0,
            size: 17.0,
        }
    }
}

impl SpotKind {
    pub fn index(self) -> usize {
        SpotKind::ALL
            .iter()
            .position(|kind| *kind == self)
            .expect("every kind")
    }

    /// The end of its style's name: `spotAbstractClass`.
    pub fn style_name(self) -> &'static str {
        match self {
            SpotKind::Class => "Class",
            SpotKind::Abstract => "AbstractClass",
            SpotKind::Interface => "Interface",
            SpotKind::Enum => "Enum",
            SpotKind::Annotation => "Annotation",
            SpotKind::Entity => "Entity",
            SpotKind::Protocol => "Protocol",
            SpotKind::Struct => "Struct",
            SpotKind::Exception => "Exception",
            SpotKind::MetaClass => "MetaClass",
            SpotKind::Record => "Record",
            SpotKind::DataClass => "DataClass",
        }
    }

    /// The letter of its skinparam (`stereotypeNBackgroundColor` for an
    /// annotation), for the five kinds PlantUML has one for.
    fn skinparam_letter(self) -> Option<char> {
        match self {
            SpotKind::Class => Some('c'),
            SpotKind::Interface => Some('i'),
            SpotKind::Abstract => Some('a'),
            SpotKind::Enum => Some('e'),
            SpotKind::Annotation => Some('n'),
            _ => None,
        }
    }
}

/// What `key` (lower case) sets of the circles, if it is theirs: the
/// skinparams, and the names the styles are read under
/// (`SpotClassBackgroundColor`, `SpotFontColor`).
pub(super) fn read(spots: &mut Spots, key: &str, p: &Skinparam) -> Option<Result<(), Declined>> {
    let set = |spots: &mut Spots| -> Result<(), Declined> {
        match key {
            "circledcharacterradius" => spots.radius = p.number(2.0, 100.0)?,
            "circledcharacterfontsize" => spots.size = p.number(4.0, 100.0)?,
            // PlantUML takes it and paints the letter black all the same.
            "circledcharacterfontcolor" => {
                p.solid()?;
            }
            _ if key.starts_with("circledcharacter") => {}
            "spotfontcolor" => {
                spots.letter = p.solid()?;
                spots.letter_owned = true;
            }
            _ => {
                let (kind, property) = kind_of(key).expect("a circle's key");
                let color = p.solid()?;
                match property {
                    "backgroundcolor" => spots.fill[kind.index()] = color,
                    "fontcolor" => spots.letters[kind.index()] = Some(color),
                    _ => spots.border[kind.index()] = color,
                }
            }
        }
        Ok(())
    };
    let ours =
        key.starts_with("circledcharacter") || key == "spotfontcolor" || kind_of(key).is_some();
    // `spotClassFontColor` and its kin are the names a style is read
    // under; written as a skinparam, PlantUML takes them and changes
    // nothing.
    let styled = p.declared.is_some() || !key.starts_with("spot");
    ours.then(|| if styled { set(spots) } else { Ok(()) })
}

/// `stereotypecbackgroundcolor`, `spotentitybordercolor`,
/// `spotstructfontcolor`: the kind and what of it. A kind's letter color
/// is a style's alone.
fn kind_of(key: &str) -> Option<(SpotKind, &str)> {
    let (subject, property) = ["backgroundcolor", "bordercolor", "fontcolor"]
        .iter()
        .find_map(|property| Some((key.strip_suffix(property)?, *property)))?;
    SpotKind::ALL
        .into_iter()
        .find(|kind| {
            subject
                .strip_prefix("spot")
                .is_some_and(|name| name.eq_ignore_ascii_case(kind.style_name()))
                || (property != "fontcolor"
                    && subject.strip_prefix("stereotype").is_some_and(|letter| {
                        kind.skinparam_letter()
                            .is_some_and(|own| letter.len() == 1 && letter.starts_with(own))
                    }))
        })
        .map(|kind| (kind, property))
}

#[cfg(test)]
mod tests {
    use super::super::{Group, Spot};
    use super::*;

    fn spots(source: &str) -> Vec<(String, Option<Spot>)> {
        fn walk(group: &Group, out: &mut Vec<(String, Option<Spot>)>) {
            for node in &group.nodes {
                out.push((node.key.clone(), node.spot.clone()));
                walk(&node.children, out);
            }
        }
        let source = format!("@startuml\n{source}\n@enduml\n");
        let (_, lines) = crate::source::body(&source).unwrap();
        let diagram = super::super::parse(&lines).unwrap_or_else(|err| panic!("{err:?}"));
        let mut out = Vec::new();
        walk(&diagram.root, &mut out);
        out
    }

    fn letters(source: &str) -> String {
        spots(source)
            .into_iter()
            .map(|(_, spot)| spot.map_or('-', |s| s.letter))
            .collect()
    }

    /// Each keyword its letter, a class a line makes its `C`, as PlantUML
    /// draws them.
    #[test]
    fn each_kind_its_letter() {
        assert_eq!(
            letters(
                "class C\ninterface I\nabstract A\nabstract class B\nenum E\nannotation N\n\
                 entity T {\n  id\n}\nprotocol P\nstruct S\nexception X\nmetaclass M\n\
                 record R\ndataclass D\nobject O\nC --> Made"
            ),
            "CIAAE@EPSXMRD-C"
        );
        let kinds: Vec<_> = spots("protocol P\nrecord R")
            .into_iter()
            .map(|(_, spot)| spot.unwrap().kind)
            .collect();
        assert_eq!(kinds, [SpotKind::Protocol, SpotKind::Record]);
    }

    /// `<< (S,#FF7700) Singleton >>`: a letter and a color of its own, the
    /// rest a stereotype; the kind stays the keyword's.
    #[test]
    fn a_stereotype_writes_a_circle_of_its_own() {
        let found = spots(
            "class S << (S,#FF7700) Singleton >>\nclass D << (D,orchid) >>\n\
             interface Q <<(Q)>>\nF << (F) >>",
        );
        let spot = |at: usize| found[at].1.clone().unwrap();
        assert_eq!(
            spot(0),
            Spot {
                kind: SpotKind::Class,
                letter: 'S',
                color: Some("#FF7700".into()),
                own: true,
            }
        );
        assert_eq!(
            (spot(1).letter, spot(1).color.as_deref()),
            ('D', Some("orchid"))
        );
        assert_eq!(
            (spot(2).kind, spot(2).letter, spot(2).color),
            (SpotKind::Interface, 'Q', None)
        );
        assert_eq!(spot(3).letter, 'F');
        let source = "@startuml\nclass S << (S,#FF7700) Singleton >>\n@enduml\n";
        let svg = crate::render(source).unwrap();
        assert!(
            svg.contains(">«Singleton»<") && svg.contains("fill=\"#FF7700\""),
            "{svg}"
        );
        // PlantUML refuses one on a component, and two on a class.
        for source in [
            "component C << (S,red) >>",
            "class A << (S,red) >> << (T,blue) >>",
        ] {
            let source = format!("@startuml\n{source}\n@enduml\n");
            let (_, lines) = crate::source::body(&source).unwrap();
            assert!(super::super::parse(&lines).is_err(), "{source}");
        }
    }

    /// A description diagram's boxes have none, nor a lollipop; under
    /// `allowmixing` the classes keep theirs. A class named like a
    /// description keyword keeps the class diagram.
    #[test]
    fn only_a_class_diagram_s_classes_have_one() {
        assert_eq!(letters("component C\nentity E\nC --> E"), "--");
        assert_eq!(
            letters("allowmixing\ncomponent C\nclass K\nentity E"),
            "-CE"
        );
        assert_eq!(letters("component C\ninterface I\nC --> I"), "--");
        assert_eq!(letters("class K\nPerson --> Team\nNode : name"), "CCCC");
    }

    /// `hide circle` takes them away, `hide stereotype` leaves them;
    /// `strictuml` draws none.
    #[test]
    fn hide_circle_and_strictuml() {
        assert_eq!(letters("class A\nclass B\nhide A circle"), "-C");
        assert_eq!(letters("class A <<x>>\nhide <<x>> circle\nclass B"), "-C");
        assert_eq!(letters("class A <<x>>\nhide stereotype"), "C");
        let svg = |body: &str| crate::render(&format!("@startuml\n{body}\n@enduml\n")).unwrap();
        assert_eq!(svg("class A\ninterface B").matches("<ellipse").count(), 2);
        assert_eq!(
            svg("skinparam style strictuml\nclass A")
                .matches("<ellipse")
                .count(),
            0
        );
    }

    /// What the review of the circles found, held as PlantUML draws it.
    #[test]
    fn as_plantuml_reads_them() {
        // Declared again without a stereotype, a box keeps its own circle;
        // with one, it takes its kind's.
        let found = spots("class A << (S,#FF7700) Singleton >>\nclass A {\n  +x\n}");
        let spot = found[0].1.clone().unwrap();
        assert_eq!((spot.letter, spot.color.as_deref()), ('S', Some("#FF7700")));
        assert_eq!(letters("class A << (S) >>\nclass A <<Other>>"), "C");
        // `actor :User:` makes a description diagram; a note's words do
        // not, an arrow in them aside.
        assert_eq!(letters("actor :User:\nUser --> Mail"), "--");
        assert_eq!(letters("class C\nnote left : x -> (y)"), "C");
        assert_eq!(letters("class C\nnote top of C : A --> [B]"), "C");
        // `hide class circle` is the classes', not a struct's.
        assert_eq!(
            letters("class C\nstruct S\nprotocol P\nhide class circle"),
            "-SP"
        );
        assert_eq!(letters("class C\nstruct S\nhide struct circle"), "C-");
        assert_eq!(
            letters("interface I\nprotocol P\nhide interface circle"),
            "-P"
        );
        // A spot is written as PlantUML reads one; else it is words.
        let found = spots("class A << (S,FF7700) >>");
        assert_eq!(
            found[0].1.clone().unwrap().color.as_deref(),
            Some("#FF7700")
        );
        let found = spots("class B << (S,#F70) >>\nclass D << ( x , orchid ) >>");
        assert!(
            found
                .iter()
                .all(|(_, spot)| spot.as_ref().is_some_and(|s| s.letter == 'C'))
        );
        let svg = crate::render("@startuml\nclass B << (S,#F70) >>\nclass E <<x, y>>\n@enduml\n")
            .unwrap();
        assert!(
            svg.contains(">«(S,#F70)»<") && svg.contains(">«x, y»<"),
            "{svg}"
        );
        // A stereotype's circle stays declared again as another kind.
        assert_eq!(letters("abstract A << (S,red) >>\nclass A {\n  +x\n}"), "S");
        assert_eq!(letters("class A << (S,#FF7700) >>\nabstract A"), "S");
        // `<< (S ) >>` is words.
        let found = spots("class A << (S ) >>");
        assert_eq!(found[0].1.as_ref().map(|s| s.letter), Some('C'));
        // The names a style is read under change nothing as skinparams.
        let svg = crate::render(
            "@startuml\nskinparam spotClassFontColor red\nskinparam spotClassBackgroundColor blue\nclass C\n@enduml\n",
        )
        .unwrap();
        assert!(
            svg.contains("fill=\"#ADD1B2\"") && !svg.contains("fill=\"#FF0000\""),
            "{svg}"
        );
        // The letter's face and style are the diagram's: taken, no change.
        crate::render(
            "@startuml\nskinparam circledCharacterFontName Courier\nskinparam circledCharacterFontStyle italic\nclass A\n@enduml\n",
        )
        .unwrap();
    }

    /// PlantUML's colors by kind, set by skinparam and by style; the
    /// letter in the diagram's text color.
    #[test]
    fn the_colors_and_the_size_are_set_as_in_plantuml() {
        let svg = |body: &str| crate::render(&format!("@startuml\n{body}\n@enduml\n")).unwrap();
        let plain = svg("class C\ninterface I\nabstract A\nenum E\nannotation N");
        for fill in ["#ADD1B2", "#B4A7E5", "#A9DCDF", "#EB937F", "#E3664A"] {
            assert!(
                plain.contains(&format!("fill=\"{fill}\" stroke=\"#181818\"")),
                "{fill}: {plain}"
            );
        }
        let set = svg(
            "skinparam stereotypeCBackgroundColor red\nskinparam stereotypeIBorderColor blue\n\
             skinparam defaultFontColor green\nskinparam circledCharacterRadius 15\n\
             class C\ninterface I",
        );
        assert!(
            set.contains("rx=\"15\" ry=\"15\" fill=\"#FF0000\""),
            "{set}"
        );
        assert!(set.contains("fill=\"#B4A7E5\" stroke=\"#0000FF\""), "{set}");
        assert!(set.contains("fill=\"#008000\""), "{set}");
        let styled = svg(
            "<style>\nspot {\n  FontColor navy\n  spotEntity {\n    BackgroundColor pink\n  }\n}\n\
             </style>\nentity E {\n  id\n}",
        );
        assert!(styled.contains("fill=\"#FFC0CB\""), "{styled}");
        assert!(styled.contains("fill=\"#000080\""), "{styled}");
    }

    /// The circle stands left of the words, the two centered together,
    /// in the middle of the header.
    #[test]
    fn the_circle_stands_before_the_name() {
        let svg = crate::render("@startuml\nclass Name\n@enduml\n").unwrap();
        let number = |text: &str, attr: &str| -> f32 {
            let at = text.find(&format!(" {attr}=\"")).unwrap() + attr.len() + 3;
            text[at..].split('"').next().unwrap().parse().unwrap()
        };
        let ellipse = &svg[svg.find("<ellipse").unwrap()..];
        let rect = &svg[svg.find("<rect").unwrap()..];
        let rect = &rect[rect[1..].find("<rect").map_or(0, |at| at + 1)..];
        let name = &svg[svg
            .find(">Name<")
            .map(|at| svg[..at].rfind("<text").unwrap())
            .unwrap()..];
        let (cx, cy) = (number(ellipse, "cx"), number(ellipse, "cy"));
        let (x, y, w, h) = (
            number(rect, "x"),
            number(rect, "y"),
            number(rect, "width"),
            number(rect, "height"),
        );
        assert!(cx - 11.0 > x && number(name, "x") > cx + 11.0, "{svg}");
        assert!((cy - (y + h / 2.0)).abs() < 2.0, "{svg}");
        assert!(cx - 11.0 - x < w / 2.0, "{svg}");
    }

    #[test]
    fn keys_name_the_kinds() {
        assert_eq!(
            kind_of("stereotypecbackgroundcolor"),
            Some((SpotKind::Class, "backgroundcolor"))
        );
        assert_eq!(
            kind_of("stereotypenbordercolor"),
            Some((SpotKind::Annotation, "bordercolor"))
        );
        assert_eq!(
            kind_of("spotabstractclassbackgroundcolor"),
            Some((SpotKind::Abstract, "backgroundcolor"))
        );
        assert_eq!(
            kind_of("spotentitybackgroundcolor"),
            Some((SpotKind::Entity, "backgroundcolor"))
        );
        // PlantUML has no skinparam for an entity's circle.
        assert_eq!(kind_of("stereotypexbackgroundcolor"), None);
        assert_eq!(kind_of("stereotypefontcolor"), None);
    }
}
