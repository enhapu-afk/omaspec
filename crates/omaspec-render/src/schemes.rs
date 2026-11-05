//! One SVG in two color schemes. The same text drawn in a light and in a
//! dark palette gives two pictures that differ only in their colors (and
//! how strong they are: a shadow is stronger on a dark page); the
//! light one is kept as it is, and a style sheet at its end gives the dark
//! colors under `prefers-color-scheme: dark`. A light color that turns
//! into the same dark one everywhere is turned by its attribute
//! (`[fill="#ffffff"]`); one that turns into several gets a class on each
//! element. A viewer that does not ask for a scheme (resvg, a PDF) shows
//! the light picture.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use roxmltree::{Document, Node, NodeType};

use crate::sanitize::{self, SanitizeError};

/// Attributes and style properties that hold a color, or how strong one
/// is.
const COLORS: &[&str] = &[
    "fill",
    "stroke",
    "stop-color",
    "flood-color",
    "lighting-color",
    "color",
    "opacity",
    "fill-opacity",
    "stroke-opacity",
    "stop-opacity",
    "flood-opacity",
];

/// The two pictures differ in more than their colors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch(pub String);

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the light and the dark picture differ in more than colors ({})",
            self.0
        )
    }
}

impl std::error::Error for Mismatch {}

impl From<SanitizeError> for Mismatch {
    fn from(err: SanitizeError) -> Self {
        Mismatch(err.to_string())
    }
}

/// A color of an element in both pictures.
struct Paint {
    /// Where the light element starts, to put a class on it.
    element: usize,
    /// The attribute, or the style property.
    name: String,
    light: String,
    dark: String,
    /// From a `style` attribute, which outweighs a sheet unless `!important`.
    inline: bool,
}

/// `light` and `dark`, both sanitized SVG of one diagram, as one SVG that
/// takes the dark colors under `prefers-color-scheme: dark`.
pub fn both(light: &str, dark: &str) -> Result<String, Mismatch> {
    let l = Document::parse(light).map_err(|e| Mismatch(e.to_string()))?;
    let d = Document::parse(dark).map_err(|e| Mismatch(e.to_string()))?;
    let mut paints = Vec::new();
    let mut sheets = Vec::new();
    walk(l.root_element(), d.root_element(), &mut paints, &mut sheets)?;

    // A light color that turns into one dark color wherever it is, and is
    // not set inline, is turned by its attribute.
    let mut turns: BTreeMap<(&str, &str), BTreeSet<&str>> = BTreeMap::new();
    for p in &paints {
        turns
            .entry((&p.name, &p.light))
            .or_default()
            .insert(&p.dark);
    }
    let by_attribute =
        |p: &Paint| !p.inline && turns[&(p.name.as_str(), p.light.as_str())].len() == 1;
    let mut rules = Vec::new();
    for ((name, light), darks) in &turns {
        let dark = darks.iter().next().expect("a dark color");
        let changes = light != dark;
        let all_attributes = paints
            .iter()
            .filter(|p| p.name == *name && p.light == *light)
            .all(|p| !p.inline);
        if changes && darks.len() == 1 && all_attributes {
            rules.push(format!(
                "[{name}=\"{}\"] {{ {name}: {dark}; }}",
                css_string(light)
            ));
        }
    }
    // The rest by a class on each element, one for each turn.
    let mut classes: BTreeMap<(&str, &str, &str, bool), usize> = BTreeMap::new();
    let mut marks: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for p in paints
        .iter()
        .filter(|p| p.light != p.dark && !by_attribute(p))
    {
        let next = classes.len();
        let class = *classes
            .entry((&p.name, &p.light, &p.dark, p.inline))
            .or_insert(next);
        marks.entry(p.element).or_default().insert(class);
    }
    for ((name, _, dark, _), class) in &classes {
        rules.push(format!(
            ".omaspec-dark-{class} {{ {name}: {dark} !important; }}"
        ));
    }
    if rules.is_empty() && sheets.is_empty() {
        return Ok(light.to_owned());
    }

    let mut out = String::with_capacity(light.len() + 64 * rules.len());
    let mut at = 0;
    for (start, classes) in &marks {
        let element = l
            .descendants()
            .find(|n| n.is_element() && n.range().start == *start)
            .expect("the element a paint came from");
        let names: Vec<String> = classes
            .iter()
            .map(|c| format!("omaspec-dark-{c}"))
            .collect();
        let names = names.join(" ");
        match element.attributes().find(|a| a.name() == "class") {
            Some(class) => {
                let value = class.range_value();
                out.push_str(&light[at..value.end]);
                out.push(' ');
                out.push_str(&names);
                at = value.end;
            }
            None => {
                // Sanitized SVG names its elements without a prefix.
                let name = element.tag_name().name();
                let name_end = start + 1 + name.len();
                if light.get(start + 1..name_end) != Some(name) {
                    return Err(Mismatch(format!("<{name}> is written with a prefix")));
                }
                out.push_str(&light[at..name_end]);
                out.push_str(&format!(" class=\"{names}\""));
                at = name_end;
            }
        }
    }
    // The sheet goes last, so it comes after any the picture has.
    let root = l.root_element();
    let end = light[..root.range().end]
        .rfind("</")
        .ok_or_else(|| Mismatch("the picture does not close".into()))?;
    out.push_str(&light[at..end]);
    out.push_str("<style>@media (prefers-color-scheme: dark) { ");
    for rule in &rules {
        out.push_str(rule);
        out.push(' ');
    }
    for sheet in &sheets {
        out.push_str(sheet);
        out.push(' ');
    }
    out.push_str("}</style>");
    out.push_str(&light[end..]);
    Ok(sanitize::sanitize(&out)?)
}

/// Walks `l` and `d` together: they may differ in colors only.
fn walk<'a>(
    l: Node<'a, '_>,
    d: Node<'_, '_>,
    paints: &mut Vec<Paint>,
    sheets: &mut Vec<String>,
) -> Result<(), Mismatch> {
    if l.node_type() != d.node_type() {
        return Err(Mismatch("a node of another kind".into()));
    }
    match l.node_type() {
        NodeType::Text => {
            return if l.text() == d.text() {
                Ok(())
            } else {
                Err(Mismatch(format!("the text {:?}", l.text().unwrap_or(""))))
            };
        }
        NodeType::Element => {}
        _ => return Ok(()),
    }
    if l.tag_name() != d.tag_name() {
        return Err(Mismatch(format!(
            "<{}> against <{}>",
            l.tag_name().name(),
            d.tag_name().name()
        )));
    }
    let names = |n: Node| -> Vec<String> {
        let mut names: Vec<String> = n.attributes().map(|a| a.name().to_owned()).collect();
        names.sort();
        names
    };
    if names(l) != names(d) {
        return Err(Mismatch(format!(
            "the attributes of <{}>",
            l.tag_name().name()
        )));
    }
    let element = l.range().start;
    for a in l.attributes() {
        let dark = d
            .attribute((a.namespace().unwrap_or(""), a.name()))
            .or_else(|| d.attribute(a.name()));
        let dark = dark.unwrap_or_default();
        let (name, light) = (a.name(), a.value());
        if COLORS.contains(&name) && a.namespace().is_none() {
            paints.push(Paint {
                element,
                name: name.to_owned(),
                light: light.to_owned(),
                dark: dark.to_owned(),
                inline: false,
            });
        } else if name == "style" {
            styles(element, light, dark, paints)?;
        } else if light != dark {
            return Err(Mismatch(format!("`{name}` of <{}>", l.tag_name().name())));
        }
    }
    // A style sheet differs as a whole: the dark one comes under the query.
    if l.tag_name().name() == "style" {
        let text = |n: Node| n.children().filter_map(|c| c.text()).collect::<String>();
        let (lt, dt) = (text(l), text(d));
        if lt != dt {
            sheets.push(dt);
        }
        return Ok(());
    }
    let (lc, dc): (Vec<Node>, Vec<Node>) = (l.children().collect(), d.children().collect());
    if lc.len() != dc.len() {
        return Err(Mismatch(format!(
            "the children of <{}>",
            l.tag_name().name()
        )));
    }
    for (l, d) in lc.into_iter().zip(dc) {
        walk(l, d, paints, sheets)?;
    }
    Ok(())
}

/// The colors of two `style` attributes, which may differ in colors only.
fn styles(
    element: usize,
    light: &str,
    dark: &str,
    paints: &mut Vec<Paint>,
) -> Result<(), Mismatch> {
    let declarations = |s: &str| -> Vec<(String, String)> {
        s.split(';')
            .filter_map(|d| {
                let (k, v) = d.split_once(':')?;
                Some((k.trim().to_ascii_lowercase(), v.trim().to_owned()))
            })
            .collect()
    };
    let (l, d) = (declarations(light), declarations(dark));
    if l.len() != d.len() {
        return Err(Mismatch("a style".into()));
    }
    for ((lk, lv), (dk, dv)) in l.into_iter().zip(d) {
        if lk != dk {
            return Err(Mismatch("a style".into()));
        }
        if COLORS.contains(&lk.as_str()) {
            paints.push(Paint {
                element,
                name: lk,
                light: lv,
                dark: dv,
                inline: true,
            });
        } else if lv != dv {
            return Err(Mismatch(format!("`{lk}` in a style")));
        }
    }
    Ok(())
}

/// `value` for a CSS string in double quotes.
fn css_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIGHT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect width="100" height="50" fill="#ffffff"/><rect x="5" y="5" width="20" height="10" fill="#ffffff" stroke="#2f6fdb"/><text x="5" y="40" fill="#1f2328">Idle</text></svg>"##;
    const DARK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect width="100" height="50" fill="#16181d"/><rect x="5" y="5" width="20" height="10" fill="#23272f" stroke="#79a6f0"/><text x="5" y="40" fill="#e3e5ea">Idle</text></svg>"##;

    #[test]
    fn a_light_color_that_turns_into_one_dark_color_is_turned_by_its_attribute() {
        let both = both(LIGHT, DARK).unwrap();
        assert!(
            both.contains(r##"[stroke="#2f6fdb"] { stroke: #79a6f0; }"##),
            "{both}"
        );
        assert!(
            both.contains(r##"[fill="#1f2328"] { fill: #e3e5ea; }"##),
            "{both}"
        );
        // White turns into the page and into the box: a class each.
        assert!(!both.contains(r##"[fill="#ffffff"]"##), "{both}");
        assert_eq!(both.matches("omaspec-dark-").count(), 4, "{both}");
        assert!(both.contains("@media (prefers-color-scheme: dark)"));
        // The light picture is what a viewer without the query draws.
        let tree = crate::export::parse(&both).unwrap();
        let png = crate::export::rasterize(&tree, 1.0).unwrap();
        let p = png.pixel(50, 45).unwrap();
        assert_eq!((p.red(), p.green(), p.blue()), (255, 255, 255));
    }

    #[test]
    fn pictures_that_differ_in_more_than_colors_do_not_merge() {
        let moved = DARK.replace(r#"x="5" y="5""#, r#"x="6" y="5""#);
        let err = both(LIGHT, &moved).unwrap_err();
        assert!(err.0.contains("`x` of <rect>"), "{err}");
        let other = DARK.replace("Idle", "Busy");
        assert!(both(LIGHT, &other).is_err());
        // The same picture twice needs no sheet.
        assert_eq!(both(LIGHT, LIGHT).unwrap(), LIGHT);
    }

    #[test]
    fn a_shadow_may_be_stronger_on_the_dark_page() {
        let shadow = |fill: &str, opacity: &str| {
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><path d="M0 0h10v10z" fill="#000000" fill-opacity="{opacity}"/><rect width="5" height="5" fill="{fill}"/></svg>"##
            )
        };
        let both = both(&shadow("#ffffff", "0.02"), &shadow("#23272f", "0.06")).unwrap();
        assert!(
            both.contains(r#"[fill-opacity="0.02"] { fill-opacity: 0.06; }"#),
            "{both}"
        );
    }
}
