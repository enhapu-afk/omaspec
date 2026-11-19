//! Words that do not read on the paper of a look, made to read. The colors
//! a file writes itself stay when a look is laid over it
//! ([`omaspec_uml::repaint`]); on paper of the other lightness the look's
//! words can then lie on a box of the file's that is as light as they
//! are. Once a picture is drawn, each text is measured against what lies
//! under it ([`crate::contrast`]), and where it reads below WCAG's 4.5:1:
//!
//! - a ground the file wrote (not one of the look's colors) is turned to
//!   the other side of the middle, its hue kept
//!   ([`omaspec_uml::repaint::Look::turn`]), at a lightness where every
//!   text on it reads; a light blue box under dark words on white is a
//!   dark blue one under light words on dark paper;
//! - else the words, where the file wrote their color.
//!
//! A line of the file's own color that reads below 3:1 on the paper is
//! turned the same way. What reads as it is stays as it is: a dark box
//! with white words of its own, the look's colors. Each color is turned
//! wherever it stands, in shapes for a ground and in words for words, so
//! one box keeps one color.

use std::collections::{HashMap, HashSet};

use omaspec_uml::Color;
use omaspec_uml::repaint::{Look, lightness};

use crate::contrast::{self, Kind, Rgb};

/// Least contrast of a text with what lies under it (WCAG AA).
const TEXT: f32 = 4.5;
/// Least contrast of a line with the paper.
const LINE: f32 = 3.0;

/// `svg`, drawn from a source with `look` laid over it, with what does not
/// read turned to read; as it is when everything reads, or when it cannot
/// be measured.
pub fn repair(svg: &str, look: &Look) -> String {
    if !look.has_own() || !suspect(svg, look) {
        return svg.to_owned();
    }
    let Ok(pairs) = contrast::pairs(svg) else {
        return svg.to_owned();
    };
    let color = |(r, g, b): Rgb| Color(r, g, b);
    // The grounds of the file's that words do not read on; the colors of
    // the file's words that do not read on the look's grounds; the
    // file's lines that do not read on the paper.
    let mut grounds: HashSet<Color> = HashSet::new();
    let mut words: HashMap<Color, Vec<Color>> = HashMap::new();
    let mut lines: HashSet<Color> = HashSet::new();
    for pair in &pairs {
        let (ink, on) = (color(pair.color), color(pair.on));
        match pair.kind {
            Kind::Text(_) if pair.ratio() < TEXT => {
                if look.owns(on) {
                    grounds.insert(on);
                } else if look.owns(ink) {
                    words.entry(ink).or_default().push(on);
                }
            }
            Kind::Line if pair.ratio() < LINE && look.owns(ink) => {
                lines.insert(ink);
            }
            _ => {}
        }
    }
    if grounds.is_empty() && words.is_empty() && lines.is_empty() {
        return svg.to_owned();
    }
    // Every text on a ground that turns, so the ground reads under all.
    let mut on_ground: HashMap<Color, Vec<Color>> = HashMap::new();
    for pair in &pairs {
        if matches!(pair.kind, Kind::Text(_)) && grounds.contains(&color(pair.on)) {
            on_ground
                .entry(color(pair.on))
                .or_default()
                .push(color(pair.color));
        }
    }
    let mut fills: HashMap<Color, Color> = grounds
        .iter()
        .map(|&ground| {
            let inks = on_ground.get(&ground).map_or(&[][..], Vec::as_slice);
            (ground, reading(look, ground, inks, TEXT))
        })
        .collect();
    let inks: HashMap<Color, Color> = words
        .iter()
        .map(|(&ink, on)| {
            let on: Vec<Color> = on.iter().map(|c| *fills.get(c).unwrap_or(c)).collect();
            (ink, reading(look, ink, &on, TEXT))
        })
        .collect();
    // A ground under words that turn too reads against them as drawn.
    for (ground, fill) in &mut fills {
        let inks_on: Vec<Color> = on_ground
            .get(ground)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .map(|ink| *inks.get(ink).unwrap_or(ink))
            .collect();
        if inks_on.iter().any(|ink| ink.contrast(*fill) < TEXT) {
            *fill = reading(look, *ground, &inks_on, TEXT);
        }
    }
    let strokes: HashMap<Color, Color> = lines
        .iter()
        .map(|&ink| (ink, reading(look, ink, &[look.paper()], LINE)))
        .collect();
    recolored(svg, &fills, &inks, &strokes)
}

/// Whether `svg` holds a color of the file's that may not read:
/// a ground on the side of the file's paper (light on a dark look), or
/// words and lines on the side of the look's. Measuring a picture takes
/// far longer than drawing it, so only such a picture is measured.
fn suspect(svg: &str, look: &Look) -> bool {
    let dark = look.paper().is_dark();
    let mut rest = svg;
    while let Some(open) = rest.find('<') {
        let close = rest[open..]
            .find('>')
            .map_or(rest.len(), |at| open + at + 1);
        let tag = &rest[open..close];
        let words = tag.starts_with("<text") || tag.starts_with("<tspan");
        for (name, ground) in [("fill", !words), ("stroke", false)] {
            if let Some(color) = value(tag, name)
                && look.owns(color)
                && (color.is_dark() == dark) != ground
            {
                return true;
            }
        }
        rest = &rest[close..];
    }
    false
}

/// The color the attribute `name` of `tag` names.
fn value(tag: &str, name: &str) -> Option<Color> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    let len = tag[start..].find('"')?;
    omaspec_uml::parse_color(&tag[start..start + len])
        .ok()
        .flatten()
}

/// `color` turned to the other side, at the lightness nearest the turned
/// one at which it reads at `least` against each of `others`.
fn reading(look: &Look, color: Color, others: &[Color], least: f32) -> Color {
    let reads = |c: Color| others.iter().all(|other| other.contrast(c) >= least);
    let turned = look.turn(color);
    if reads(turned) {
        return turned;
    }
    let from = lightness(turned);
    (1..=40)
        .flat_map(|step| {
            let off = step as f32 * 0.025;
            [from - off, from + off]
        })
        .filter(|l| (0.0..=1.0).contains(l))
        .map(|l| look.at(color, l))
        .find(|&c| reads(c))
        .unwrap_or(turned)
}

/// `svg` with the `fill` of its shapes turned by `fills`, of its words by
/// `inks`, and the `stroke` of either by `strokes`.
fn recolored(
    svg: &str,
    fills: &HashMap<Color, Color>,
    inks: &HashMap<Color, Color>,
    strokes: &HashMap<Color, Color>,
) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let close = rest[open..]
            .find('>')
            .map_or(rest.len(), |at| open + at + 1);
        let tag = &rest[open..close];
        let name = tag[1..]
            .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .next()
            .unwrap_or_default();
        let words = matches!(name, "text" | "tspan");
        let tag = attribute(tag, "fill", if words { inks } else { fills });
        out.push_str(&attribute(&tag, "stroke", strokes));
        rest = &rest[close..];
    }
    out.push_str(rest);
    out
}

/// `tag` with its attribute `name` turned by `turns`, where it names a
/// color there.
fn attribute(tag: &str, name: &str, turns: &HashMap<Color, Color>) -> String {
    if turns.is_empty() {
        return tag.to_owned();
    }
    let key = format!(" {name}=\"");
    let Some(at) = tag.find(&key) else {
        return tag.to_owned();
    };
    let start = at + key.len();
    let Some(len) = tag[start..].find('"') else {
        return tag.to_owned();
    };
    let value = &tag[start..start + len];
    match omaspec_uml::parse_color(value)
        .ok()
        .flatten()
        .and_then(|c| turns.get(&c))
    {
        Some(turned) => format!("{}{turned}{}", &tag[..start], &tag[start + len..]),
        None => tag.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Language, Renderer};
    use omaspec_core::skin::{self, DARK, Pack};

    fn drawn(source: &str) -> String {
        let mut renderer = Renderer::new(&Config {
            java_fallback: false,
            ..Config::default()
        });
        renderer.render(source, Language::PlantUml).unwrap().svg
    }

    /// `code` with Omaspec's dark look laid over it.
    fn dark(code: &str) -> String {
        let (range, text) =
            skin::edit(code, Language::PlantUml, Some(Pack::Omarchy), &DARK).unwrap();
        format!("{}{text}{}", &code[..range.start], &code[range.end..])
    }

    const BOX: &str = "@startuml\nbox \"Backend\" #LightBlue\nparticipant A\nparticipant B\nend box\nA -> B : inside\n@enduml\n";

    #[test]
    fn a_light_box_of_the_file_s_turns_dark_under_light_words() {
        let svg = drawn(&dark(BOX));
        // The box is no longer light blue, and every text reads.
        assert!(!svg.contains("#ADD8E6"), "{svg}");
        for pair in contrast::pairs(&svg).unwrap() {
            if let Kind::Text(words) = &pair.kind {
                assert!(pair.ratio() >= TEXT, "{words}: {pair:?}");
            }
        }
        // Without a look, as written.
        assert!(drawn(BOX).contains("#ADD8E6"));
    }

    #[test]
    fn what_reads_stays() {
        // A dark box reads under the dark look's light words as it is.
        let source = dark(&BOX.replace("#LightBlue", "#08427B"));
        let look = omaspec_uml::repaint::look_of(&source).expect("a dark look");
        let svg = drawn(&source);
        assert!(svg.contains("#08427B"), "{svg}");
        assert_eq!(repair(&svg, &look), svg);
    }
}
