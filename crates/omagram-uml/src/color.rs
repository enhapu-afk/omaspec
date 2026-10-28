//! PlantUML colors: `#RGB`, `#RRGGBB`, the HTML color names and the few
//! names PlantUML adds, with or without `#`. Also `rgb()`, `rgba()`,
//! `hsl()` and `hsla()`, as Mermaid writes a box or a `rect`.

use std::fmt;

/// An opaque color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl Color {
    pub const BLACK: Color = Color(0, 0, 0);
    pub const WHITE: Color = Color(255, 255, 255);

    pub const fn hex(rgb: u32) -> Color {
        Color((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    }

    /// The gray of the same lightness, as PlantUML's `monochrome` draws it.
    pub fn gray(self) -> Color {
        let Color(r, g, b) = self;
        let l = (0.3 * f32::from(r) + 0.59 * f32::from(g) + 0.11 * f32::from(b)).round() as u8;
        Color(l, l, l)
    }

    /// Whether text in black would be hard to read on this color.
    pub fn is_dark(self) -> bool {
        self.gray().0 < 128
    }

    /// This color moved `share` (0 to 1) of the way to `other`.
    pub fn mix(self, other: Color, share: f32) -> Color {
        let step = |from: u8, to: u8| {
            (f32::from(from) + (f32::from(to) - f32::from(from)) * share).round() as u8
        };
        Color(
            step(self.0, other.0),
            step(self.1, other.1),
            step(self.2, other.2),
        )
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

/// A fill or stroke: a color or nothing.
pub type Paint = Option<Color>;

/// `value` as a paint (`transparent` is none). An error names what is not
/// understood; gradients (`#red/blue`) are not supported yet.
///
/// Opacity is dropped, except a fully transparent ink, which is none.
/// Callers that mix a highlight onto a background use [`parse_ink`].
pub fn parse(value: &str) -> Result<Paint, String> {
    let (paint, alpha) = parse_ink(value)?;
    if paint.is_none() || alpha == 0 {
        Ok(None)
    } else {
        Ok(paint)
    }
}

/// A color and its opacity, 0 to 255. `transparent` is no color.
///
/// `rgb()` and `hsl()` are opaque. In `rgba()` and `hsla()` an alpha of
/// at most 1 (including `.1`) is a fraction; a larger one is on the 0–255
/// scale, which is how Mermaid writes the two.
pub(crate) fn parse_ink(value: &str) -> Result<(Paint, u8), String> {
    let v = value.trim();
    if let Some(parsed) = css_function(v) {
        return parsed;
    }
    let bare = v.strip_prefix('#').unwrap_or(v);
    if bare.contains(['/', '|', '\\', '-']) {
        return Err(format!("color gradients (`{v}`) are not supported yet"));
    }
    if bare.eq_ignore_ascii_case("transparent") {
        return Ok((None, 0));
    }
    if matches!(bare.len(), 3 | 6) && bare.bytes().all(|b| b.is_ascii_hexdigit()) {
        let n = u32::from_str_radix(bare, 16).map_err(|e| e.to_string())?;
        let color = if bare.len() == 3 {
            let (r, g, b) = ((n >> 8) & 0xf, (n >> 4) & 0xf, n & 0xf);
            Color((r * 17) as u8, (g * 17) as u8, (b * 17) as u8)
        } else {
            Color::hex(n)
        };
        return Ok((Some(color), 255));
    }
    NAMES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(bare))
        .map(|&(_, rgb)| (Some(Color::hex(rgb)), 255))
        .ok_or_else(|| format!("unknown color `{v}`"))
}

/// `rgb()`, `rgba()`, `hsl()` or `hsla()`. `None` when `value` is not one
/// of those; an error when it is one and cannot be read.
fn css_function(value: &str) -> Option<Result<(Paint, u8), String>> {
    let lower = value.to_ascii_lowercase();
    // `rgba` before `rgb`, and `hsla` before `hsl`: the shorter prefix
    // also matches the longer name.
    let (kind, inner) = ["rgba(", "rgb(", "hsla(", "hsl("]
        .into_iter()
        .find_map(|prefix| {
            lower
                .strip_prefix(prefix)
                .map(|rest| (&prefix[..prefix.len() - 1], rest))
        })?;
    let Some(inner) = inner.strip_suffix(')') else {
        return Some(Err(format!("unknown color `{value}`")));
    };
    if inner.contains(['(', ')']) {
        return Some(Err(format!("unknown color `{value}`")));
    }
    Some(read_function(kind, inner.trim(), value))
}

fn read_function(kind: &str, inner: &str, value: &str) -> Result<(Paint, u8), String> {
    let fail = || format!("unknown color `{value}`");
    let (body, slash) = match inner.split_once('/') {
        Some((body, alpha)) => (body.trim(), Some(alpha.trim())),
        None => (inner, None),
    };
    let parts: Vec<&str> = if body.contains(',') {
        body.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .collect()
    } else {
        body.split_whitespace().collect()
    };
    let hsl = kind.starts_with("hsl");
    let with_alpha = kind.ends_with('a');
    let (rgb, alpha_token) = if hsl {
        if parts.len() < 3 {
            return Err(fail());
        }
        let color = hsl_color(parts[0], parts[1], parts[2]).map_err(|_| fail())?;
        let extra = if parts.len() == 4 {
            Some(parts[3])
        } else if parts.len() == 3 {
            None
        } else {
            return Err(fail());
        };
        (color, extra)
    } else {
        if parts.len() < 3 {
            return Err(fail());
        }
        let color = Color(
            channel(parts[0]).map_err(|_| fail())?,
            channel(parts[1]).map_err(|_| fail())?,
            channel(parts[2]).map_err(|_| fail())?,
        );
        let extra = if parts.len() == 4 {
            Some(parts[3])
        } else if parts.len() == 3 {
            None
        } else {
            return Err(fail());
        };
        (color, extra)
    };
    let alpha = match (alpha_token, slash, with_alpha) {
        (None, None, false) => 255,
        (Some(token), None, _) | (None, Some(token), _) => alpha(token).map_err(|_| fail())?,
        _ => return Err(fail()),
    };
    Ok((Some(rgb), alpha))
}

fn channel(token: &str) -> Result<u8, ()> {
    if let Some(pct) = token.strip_suffix('%') {
        let n: f32 = pct.trim().parse().map_err(|_| ())?;
        if !(0.0..=100.0).contains(&n) {
            return Err(());
        }
        return Ok((n / 100.0 * 255.0).round() as u8);
    }
    let n: f32 = token.parse().map_err(|_| ())?;
    if !(0.0..=255.0).contains(&n) {
        return Err(());
    }
    Ok(n.round() as u8)
}

/// A fraction when `token` is at most 1, otherwise a 0–255 level.
fn alpha(token: &str) -> Result<u8, ()> {
    if let Some(pct) = token.strip_suffix('%') {
        let n: f32 = pct.trim().parse().map_err(|_| ())?;
        if !(0.0..=100.0).contains(&n) {
            return Err(());
        }
        return Ok((n / 100.0 * 255.0).round() as u8);
    }
    let n: f32 = token.parse().map_err(|_| ())?;
    if n < 0.0 {
        return Err(());
    }
    if n <= 1.0 {
        Ok((n * 255.0).round() as u8)
    } else if n <= 255.0 {
        Ok(n.round() as u8)
    } else {
        Err(())
    }
}

fn hsl_color(h: &str, s: &str, l: &str) -> Result<Color, ()> {
    let h = h.strip_suffix("deg").unwrap_or(h).trim();
    let h: f32 = h.parse().map_err(|_| ())?;
    let s = percent(s)?;
    let l = percent(l)?;
    Ok(hsl_to_rgb(h, s, l))
}

fn percent(token: &str) -> Result<f32, ()> {
    let Some(n) = token.strip_suffix('%') else {
        return Err(());
    };
    let n: f32 = n.trim().parse().map_err(|_| ())?;
    if !(0.0..=100.0).contains(&n) {
        return Err(());
    }
    Ok(n / 100.0)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Color {
    let h = h.rem_euclid(360.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h1 = h / 60.0;
    let x = c * (1.0 - ((h1 % 2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h1.floor() as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color(byte(r), byte(g), byte(b))
}

const NAMES: &[(&str, u32)] = &[
    ("AliceBlue", 0xF0F8FF),
    ("AntiqueWhite", 0xFAEBD7),
    ("Aqua", 0x00FFFF),
    ("Aquamarine", 0x7FFFD4),
    ("Azure", 0xF0FFFF),
    ("Beige", 0xF5F5DC),
    ("Bisque", 0xFFE4C4),
    ("Black", 0x000000),
    ("BlanchedAlmond", 0xFFEBCD),
    ("Blue", 0x0000FF),
    ("BlueViolet", 0x8A2BE2),
    ("Brown", 0xA52A2A),
    ("BurlyWood", 0xDEB887),
    ("CadetBlue", 0x5F9EA0),
    ("Chartreuse", 0x7FFF00),
    ("Chocolate", 0xD2691E),
    ("Coral", 0xFF7F50),
    ("CornflowerBlue", 0x6495ED),
    ("Cornsilk", 0xFFF8DC),
    ("Crimson", 0xDC143C),
    ("Cyan", 0x00FFFF),
    ("DarkBlue", 0x00008B),
    ("DarkCyan", 0x008B8B),
    ("DarkGoldenRod", 0xB8860B),
    ("DarkGray", 0xA9A9A9),
    ("DarkGrey", 0xA9A9A9),
    ("DarkGreen", 0x006400),
    ("DarkKhaki", 0xBDB76B),
    ("DarkMagenta", 0x8B008B),
    ("DarkOliveGreen", 0x556B2F),
    ("DarkOrange", 0xFF8C00),
    ("DarkOrchid", 0x9932CC),
    ("DarkRed", 0x8B0000),
    ("DarkSalmon", 0xE9967A),
    ("DarkSeaGreen", 0x8FBC8F),
    ("DarkSlateBlue", 0x483D8B),
    ("DarkSlateGray", 0x2F4F4F),
    ("DarkSlateGrey", 0x2F4F4F),
    ("DarkTurquoise", 0x00CED1),
    ("DarkViolet", 0x9400D3),
    ("DeepPink", 0xFF1493),
    ("DeepSkyBlue", 0x00BFFF),
    ("DimGray", 0x696969),
    ("DimGrey", 0x696969),
    ("DodgerBlue", 0x1E90FF),
    ("FireBrick", 0xB22222),
    ("FloralWhite", 0xFFFAF0),
    ("ForestGreen", 0x228B22),
    ("Fuchsia", 0xFF00FF),
    ("Gainsboro", 0xDCDCDC),
    ("GhostWhite", 0xF8F8FF),
    ("Gold", 0xFFD700),
    ("GoldenRod", 0xDAA520),
    ("Gray", 0x808080),
    ("Grey", 0x808080),
    ("Green", 0x008000),
    ("GreenYellow", 0xADFF2F),
    ("HoneyDew", 0xF0FFF0),
    ("HotPink", 0xFF69B4),
    ("IndianRed", 0xCD5C5C),
    ("Indigo", 0x4B0082),
    ("Ivory", 0xFFFFF0),
    ("Khaki", 0xF0E68C),
    ("Lavender", 0xE6E6FA),
    ("LavenderBlush", 0xFFF0F5),
    ("LawnGreen", 0x7CFC00),
    ("LemonChiffon", 0xFFFACD),
    ("LightBlue", 0xADD8E6),
    ("LightCoral", 0xF08080),
    ("LightCyan", 0xE0FFFF),
    ("LightGoldenRodYellow", 0xFAFAD2),
    ("LightGray", 0xD3D3D3),
    ("LightGrey", 0xD3D3D3),
    ("LightGreen", 0x90EE90),
    ("LightPink", 0xFFB6C1),
    ("LightSalmon", 0xFFA07A),
    ("LightSeaGreen", 0x20B2AA),
    ("LightSkyBlue", 0x87CEFA),
    ("LightSlateGray", 0x778899),
    ("LightSlateGrey", 0x778899),
    ("LightSteelBlue", 0xB0C4DE),
    ("LightYellow", 0xFFFFE0),
    ("Lime", 0x00FF00),
    ("LimeGreen", 0x32CD32),
    ("Linen", 0xFAF0E6),
    ("Magenta", 0xFF00FF),
    ("Maroon", 0x800000),
    ("MediumAquaMarine", 0x66CDAA),
    ("MediumBlue", 0x0000CD),
    ("MediumOrchid", 0xBA55D3),
    ("MediumPurple", 0x9370DB),
    ("MediumSeaGreen", 0x3CB371),
    ("MediumSlateBlue", 0x7B68EE),
    ("MediumSpringGreen", 0x00FA9A),
    ("MediumTurquoise", 0x48D1CC),
    ("MediumVioletRed", 0xC71585),
    ("MidnightBlue", 0x191970),
    ("MintCream", 0xF5FFFA),
    ("MistyRose", 0xFFE4E1),
    ("Moccasin", 0xFFE4B5),
    ("NavajoWhite", 0xFFDEAD),
    ("Navy", 0x000080),
    ("OldLace", 0xFDF5E6),
    ("Olive", 0x808000),
    ("OliveDrab", 0x6B8E23),
    ("Orange", 0xFFA500),
    ("OrangeRed", 0xFF4500),
    ("Orchid", 0xDA70D6),
    ("PaleGoldenRod", 0xEEE8AA),
    ("PaleGreen", 0x98FB98),
    ("PaleTurquoise", 0xAFEEEE),
    ("PaleVioletRed", 0xDB7093),
    ("PapayaWhip", 0xFFEFD5),
    ("PeachPuff", 0xFFDAB9),
    ("Peru", 0xCD853F),
    ("Pink", 0xFFC0CB),
    ("Plum", 0xDDA0DD),
    ("PowderBlue", 0xB0E0E6),
    ("Purple", 0x800080),
    ("RebeccaPurple", 0x663399),
    ("Red", 0xFF0000),
    ("RosyBrown", 0xBC8F8F),
    ("RoyalBlue", 0x4169E1),
    ("SaddleBrown", 0x8B4513),
    ("Salmon", 0xFA8072),
    ("SandyBrown", 0xF4A460),
    ("SeaGreen", 0x2E8B57),
    ("SeaShell", 0xFFF5EE),
    ("Sienna", 0xA0522D),
    ("Silver", 0xC0C0C0),
    ("SkyBlue", 0x87CEEB),
    ("SlateBlue", 0x6A5ACD),
    ("SlateGray", 0x708090),
    ("SlateGrey", 0x708090),
    ("Snow", 0xFFFAFA),
    ("SpringGreen", 0x00FF7F),
    ("SteelBlue", 0x4682B4),
    ("Tan", 0xD2B48C),
    ("Teal", 0x008080),
    ("Thistle", 0xD8BFD8),
    ("Tomato", 0xFF6347),
    ("Turquoise", 0x40E0D0),
    ("Violet", 0xEE82EE),
    ("Wheat", 0xF5DEB3),
    ("White", 0xFFFFFF),
    ("WhiteSmoke", 0xF5F5F5),
    ("Yellow", 0xFFFF00),
    ("YellowGreen", 0x9ACD32),
    // PlantUML's own, from ArchiMate.
    ("Application", 0xC2F0FF),
    ("Business", 0xFFFFCC),
    ("Implementation", 0xE5FFE5),
    ("Motivation", 0xEFEFFF),
    ("Physical", 0x97FF97),
    ("Strategy", 0xF8E7C0),
    ("Technology", 0x97FF97),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_names_and_transparent() {
        assert_eq!(parse("#ff8000"), Ok(Some(Color(255, 128, 0))));
        assert_eq!(parse("#F80"), Ok(Some(Color(255, 136, 0))));
        assert_eq!(parse("#LightBlue"), Ok(Some(Color(0xAD, 0xD8, 0xE6))));
        assert_eq!(parse("red"), Ok(Some(Color(255, 0, 0))));
        assert_eq!(parse("#transparent"), Ok(None));
        assert!(parse("#red/blue").unwrap_err().contains("gradients"));
        assert!(parse("#nocolor").unwrap_err().contains("unknown"));
        assert!(parse("#12345").is_err());
        assert_eq!(Color(255, 128, 0).to_string(), "#FF8000");
    }

    #[test]
    fn css_functions() {
        assert_eq!(parse("rgb(33, 66, 99)"), Ok(Some(Color(33, 66, 99))));
        assert_eq!(parse("rgb(33,66,99)"), Ok(Some(Color(33, 66, 99))));
        assert_eq!(parse("rgb(100%, 0%, 0%)"), Ok(Some(Color(255, 0, 0))));
        assert_eq!(parse("hsl(0, 100%, 50%)"), Ok(Some(Color(255, 0, 0))));
        assert_eq!(parse("hsl(120, 100%, 50%)"), Ok(Some(Color(0, 255, 0))));
        assert_eq!(parse("hsl(240, 100%, 50%)"), Ok(Some(Color(0, 0, 255))));
        assert_eq!(parse("rgba(0, 0, 255, 0)"), Ok(None));
        assert_eq!(parse("rgba(0, 0, 255, .5)"), Ok(Some(Color(0, 0, 255))));
        assert_eq!(
            parse_ink("rgba(0, 0, 255, 0.5)"),
            Ok((Some(Color(0, 0, 255)), 128))
        );
        assert_eq!(
            parse_ink("rgba(10, 20, 30, 255)"),
            Ok((Some(Color(10, 20, 30)), 255))
        );
        assert_eq!(
            parse_ink("hsla(120, 100%, 50%, .5)"),
            Ok((Some(Color(0, 255, 0)), 128))
        );
        let (_, faint) = parse_ink("rgba(0, 0, 255, .1)").unwrap();
        assert!(faint > 0 && faint < 255, "{faint}");
        assert!(parse("rgb(1, 2").is_err());
        assert!(parse("#red/blue").unwrap_err().contains("gradients"));
    }

    #[test]
    fn monochrome_keeps_lightness() {
        assert_eq!(Color::WHITE.gray(), Color::WHITE);
        assert_eq!(Color(255, 0, 0).gray(), Color(77, 77, 77));
    }
}
