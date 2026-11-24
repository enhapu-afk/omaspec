//! A file's own colors on the paper of a look. A look (a skin pack, the
//! Omarchy colors) is laid over a diagram as lines at its top, between
//! `' skin: …` and `' end of skin` (in Mermaid a `%% skin: …` line and
//! the `%%{init}%%` after it; `omaspec_core::skin` writes them). The
//! colors the file writes itself come after it and stay. On a paper as
//! light as the file's that is right; on a dark one a light box keeps its
//! light fill under words the look has made light, and they cannot be
//! read.
//!
//! [`look_of`] reads which look is laid over a source and whether its
//! paper lies on the other side of the middle than the paper the file was
//! written for (its own `backgroundColor`, else white).
//! [`Look::turn`] moves a color to the other side: its hue, at the
//! lightness that stands as far from the look's paper as it stood from
//! the file's. A light blue box on white paper is a dark blue one on dark
//! paper. The renderer turns what does not read once the picture is drawn
//! (`omaspec_render::legible`), so colors that read as they are, a dark
//! box with white words of its own, stay.

use crate::color::Color;

const PLANTUML_MARK: &str = "' skin: ";
const PLANTUML_END: &str = "' end of skin";
const MERMAID_MARK: &str = "%% skin: ";

/// How the colors of a file are moved onto the paper of a look.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// The lightness (OKLab) of the paper the file was written for.
    from: f32,
    /// The look's paper, and its lightness.
    paper: Color,
    to: f32,
    /// The colors the look writes.
    kept: Vec<Color>,
    /// The colors the file writes itself, outside the look's lines.
    own: Vec<Color>,
}

/// The look laid over `source`, when its paper lies on the other side of
/// the middle than the paper the file was written for; `None` when no look
/// is laid over it, or the file's colors read on its paper as they are.
pub fn look_of(source: &str) -> Option<Look> {
    let lines: Vec<&str> = source.lines().collect();
    let first = lines.iter().position(|line| {
        let line = line.trim();
        line.starts_with(PLANTUML_MARK) || line.starts_with(MERMAID_MARK)
    })?;
    let mermaid = lines[first].trim().starts_with(MERMAID_MARK);
    let last = if mermaid {
        let init = lines
            .get(first + 1)
            .is_some_and(|line| line.trim_start().starts_with("%%{"));
        first + usize::from(init)
    } else {
        lines[first..]
            .iter()
            .position(|line| line.trim() == PLANTUML_END)
            .map_or(first, |n| first + n)
    };
    let block = &lines[first..=last];
    let to = block.iter().rev().find_map(|line| paper(line, mermaid))?;
    let from = if mermaid {
        Color::WHITE
    } else {
        lines[..first]
            .iter()
            .chain(&lines[last + 1..])
            .filter_map(|line| paper(line, false))
            .next_back()
            .unwrap_or(Color::WHITE)
    };
    if from.is_dark() == to.is_dark() {
        return None;
    }
    let own: Vec<Color> = lines[..first]
        .iter()
        .chain(&lines[last + 1..])
        .flat_map(|line| written(line))
        .collect();
    Some(Look {
        from: oklab(from).0,
        paper: to,
        to: oklab(to).0,
        kept: block.iter().flat_map(|line| hexes(line)).collect(),
        own,
    })
}

impl Look {
    /// The look's paper.
    pub fn paper(&self) -> Color {
        self.paper
    }

    /// Whether the look writes `color` itself.
    pub fn writes(&self, color: Color) -> bool {
        color == self.paper || self.kept.contains(&color)
    }

    /// Whether the file writes `color` itself and the look does not: a
    /// color that may not read on the look's paper.
    pub fn owns(&self, color: Color) -> bool {
        self.own.contains(&color) && !self.writes(color)
    }

    /// Whether the file writes any color of its own.
    pub fn has_own(&self) -> bool {
        self.own.iter().any(|&color| !self.writes(color))
    }

    /// `color` on the look's paper: its hue, at the lightness as far
    /// from that paper as it was from the file's, on the other side.
    pub fn turn(&self, color: Color) -> Color {
        let (l, ..) = oklab(color);
        self.at(color, (self.to - (l - self.from)).clamp(0.0, 1.0))
    }

    /// `color` at the OKLab `lightness`, its hue kept, giving up as much
    /// chroma as the colors of a screen need there.
    pub fn at(&self, color: Color, lightness: f32) -> Color {
        let (_, a, b) = oklab(color);
        let mut chroma = 1.0;
        loop {
            if let Some(rgb) = srgb(lightness, a * chroma, b * chroma) {
                return rgb;
            }
            chroma *= 0.9;
            if chroma < 0.01 {
                return srgb(lightness, 0.0, 0.0).unwrap_or(color);
            }
        }
    }
}

/// The OKLab lightness of `color`, 0 to 1.
pub fn lightness(color: Color) -> f32 {
    oklab(color).0
}

/// The color of the paper a line sets, if it sets one: `skinparam
/// backgroundColor X`, or Mermaid's `"background": "X"`.
fn paper(line: &str, mermaid: bool) -> Option<Color> {
    if mermaid {
        let at = line.find("\"background\"")?;
        return hexes(&line[at..]).into_iter().next();
    }
    let rest = crate::common::keyword(line.trim(), "skinparam")?;
    let (key, value) = rest.split_once(char::is_whitespace)?;
    if !key.eq_ignore_ascii_case("backgroundColor") {
        return None;
    }
    crate::color::parse(value.trim()).ok().flatten()
}

/// The colors `line` writes: `#rgb`, `#rrggbb` and `#Name`, `rgb(…)` and
/// its kin, Mermaid's `fill:` and `stroke:` and `color:` of a `style`,
/// and the value of a skinparam or a `<style>` setting whose name ends in
/// `Color`.
fn written(line: &str) -> Vec<Color> {
    let parse = |value: &str| crate::color::parse(value.trim()).ok().flatten();
    let mut out = Vec::new();
    for (at, _) in line.match_indices('#') {
        let word: String = line[at + 1..]
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if !word.is_empty()
            && let Some(color) = parse(&format!("#{word}"))
        {
            out.push(color);
            // The color a gradient runs into (`#pink-lightyellow`).
            let after = &line[at + 1 + word.len()..];
            if let Some(rest) = after.strip_prefix(['-', '|', '/', '\\']) {
                let next: String = rest
                    .trim_start_matches('#')
                    .chars()
                    .take_while(char::is_ascii_alphanumeric)
                    .collect();
                if !next.is_empty()
                    && let Some(color) = parse(&format!("#{next}"))
                {
                    out.push(color);
                }
            }
        }
    }
    let lower = line.to_ascii_lowercase();
    for function in ["rgb(", "rgba(", "hsl(", "hsla("] {
        for (at, _) in lower.match_indices(function) {
            if let Some(end) = line[at..].find(')')
                && let Some(color) = parse(&line[at..=at + end])
            {
                out.push(color);
            }
        }
    }
    for key in ["fill:", "stroke:", "color:"] {
        for (at, _) in lower.match_indices(key) {
            let value = line[at + key.len()..]
                .split([',', ';', ' '])
                .next()
                .unwrap_or_default();
            out.extend(parse(value));
        }
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    let setting = match words.as_slice() {
        [skin, key, value, ..] if skin.eq_ignore_ascii_case("skinparam") => Some((*key, *value)),
        [key, value, ..] => Some((*key, *value)),
        _ => None,
    };
    if let Some((key, value)) = setting
        && key.to_ascii_lowercase().ends_with("color")
    {
        out.extend(parse(value));
    }
    let mut seen = Vec::with_capacity(out.len());
    out.retain(|color| {
        let new = !seen.contains(color);
        seen.push(*color);
        new
    });
    out
}

/// The colors written as `#rgb` or `#rrggbb` in `line`.
fn hexes(line: &str) -> Vec<Color> {
    let mut out = Vec::new();
    for (at, _) in line.match_indices('#') {
        let digits: String = line[at + 1..]
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        if matches!(digits.len(), 3 | 6)
            && let Ok(Some(color)) = crate::color::parse(&format!("#{digits}"))
        {
            out.push(color);
        }
    }
    out
}

/// `color` in OKLab: lightness 0 to 1, and the two axes of its hue.
fn oklab(Color(r, g, b): Color) -> (f32, f32, f32) {
    let linear = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (linear(r), linear(g), linear(b));
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    (
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    )
}

/// The OKLab color as an sRGB one; `None` where it lies outside what a
/// screen shows.
fn srgb(l: f32, a: f32, b: f32) -> Option<Color> {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
    let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
    let b = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
    let channel = |v: f32| -> Option<u8> {
        if !(-0.001..=1.001).contains(&v) {
            return None;
        }
        let v = v.clamp(0.0, 1.0);
        let v = if v <= 0.003_130_8 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        Some((v * 255.0).round() as u8)
    };
    Some(Color(channel(r)?, channel(g)?, channel(b)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DARK: &str = "@startuml\n' skin: omarchy\nskinparam backgroundColor #1A1B26\nskinparam defaultFontColor #C0CAF5\n' end of skin\nbox #LightBlue\nA -> B\nend box\n@enduml\n";

    #[test]
    fn a_dark_look_over_a_light_file_turns_its_colors_to_the_other_side() {
        let look = look_of(DARK).expect("a look");
        assert_eq!(look.paper(), Color::hex(0x1A1B26));
        assert!(look.writes(Color::hex(0xC0CAF5)) && !look.writes(Color::hex(0xADD8E6)));
        // The file writes the light blue, not the look's words.
        assert!(look.owns(Color::hex(0xADD8E6)) && !look.owns(Color::hex(0xC0CAF5)));
        // A light blue box is a dark blue one, as far from the dark paper
        // as it was from white; black words are white ones.
        let blue = look.turn(Color::hex(0xADD8E6));
        let (l, a, b) = oklab(blue);
        let (_, a0, b0) = oklab(Color::hex(0xADD8E6));
        assert!(l < 0.45, "{blue}");
        assert!(
            a.signum() == a0.signum() && b.signum() == b0.signum(),
            "the hue stays: {blue}"
        );
        assert!(
            blue.contrast(Color::hex(0xC0CAF5)) > 4.5,
            "the look's words read on it: {blue}"
        );
        assert_eq!(look.turn(Color::BLACK), Color::WHITE);
    }

    #[test]
    fn the_colors_a_file_writes() {
        let colors = |line: &str| written(line);
        assert_eq!(colors("box \"B\" #LightBlue"), [Color::hex(0xADD8E6)]);
        assert_eq!(
            colors("  style A fill:#FFF3B0,stroke:red"),
            [Color::hex(0xFFF3B0), Color::hex(0xFF0000)]
        );
        assert_eq!(colors("rect rgb(240, 248, 255)"), [Color(240, 248, 255)]);
        assert_eq!(
            colors("skinparam noteBackgroundColor LightYellow"),
            [Color::hex(0xFFFFE0)]
        );
        assert!(colors("A -> B : issue #42 fixed").len() <= 1);
        assert!(colors("A -> B : hello").is_empty());
    }

    #[test]
    fn a_look_on_paper_as_light_as_the_file_s_changes_nothing() {
        let light = DARK.replace("#1A1B26", "#FAFAFA");
        assert!(look_of(&light).is_none());
        assert!(look_of("@startuml\nA -> B\n@enduml\n").is_none());
        // A file written for dark paper under a dark look.
        let dark_file = DARK.replace(
            "box #LightBlue",
            "skinparam backgroundColor #000000\nbox #LightBlue",
        );
        assert!(look_of(&dark_file).is_none());
    }

    #[test]
    fn mermaid_s_look_is_its_init_line() {
        let source = "%% skin: omarchy\n%%{init: {\"theme\": \"base\", \"themeVariables\": {\"background\": \"#1A1B26\", \"primaryColor\": \"#24283B\"}}}%%\nsequenceDiagram\nrect rgb(240, 248, 255)\nA->>B: x\nend\n";
        let look = look_of(source).expect("a look");
        assert!(look.writes(Color::hex(0x24283B)));
        assert!(look.owns(Color(240, 248, 255)));
        assert!(look.turn(Color(240, 248, 255)).is_dark());
    }
}
