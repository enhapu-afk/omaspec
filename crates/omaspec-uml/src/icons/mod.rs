//! Pictures in a line of text: an icon of Open Iconic (`<&heart>`), drawn
//! as a path in the text's color, so it is drawn as the words are, in
//! any theme, rather than in an emoji's colors; and a sprite the diagram
//! defines (`<$bolt>`, [`sprite`]), its pixels in that color too.
//!
//! The tag is PlantUML's: `<&name>`, its size `<&name*2>`,
//! `<&name{scale=2}>` or `<&name,scale=2>`, its color `<#red&name>` or
//! `<&name,color=red>`. The icon stands on the baseline, as tall as a
//! capital of the text at scale 1.

mod open_iconic;
pub(crate) mod sprite;

use crate::color::{self, Color};

/// An icon in a line of text.
#[derive(Debug, Clone, PartialEq)]
pub struct Icon {
    pub picture: Picture,
    /// Its size in the picture.
    pub width: f32,
    pub height: f32,
    /// Its own color; the text's when `None`.
    pub color: Option<Color>,
}

/// What an icon shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Picture {
    /// An Open Iconic path in a box 8 by 8, moved right and down.
    Path { d: &'static str, dx: f32, dy: f32 },
    /// A sprite's pixels, each as dark as its level of 16.
    Pixels(std::sync::Arc<sprite::Sprite>),
    /// An arrow the handwriting face lacks (`→`), drawn by hand in its
    /// place.
    Arrow(char),
}

/// The side of an Open Iconic icon in its own units.
pub const SIDE: f32 = 8.0;

/// How tall an icon at scale 1 is, as a share of the text's size.
const CAP: f32 = 0.78;

/// The icon the tag `tag` (between `<` and `>`) asks for in text of
/// `size`; `None` when it is no icon tag, an error when it names no icon.
pub(crate) fn tag(tag: &str, size: f32) -> Option<Result<Icon, String>> {
    let (color, rest) = match tag.strip_prefix('#') {
        Some(after) => {
            let end = after.find(['&', '$'])?;
            (Some(&after[..end]), &after[end..])
        }
        None => (None, tag),
    };
    if let Some(rest) = rest.strip_prefix('$') {
        return Some(sprite_tag(rest, color));
    }
    let rest = rest.strip_prefix('&')?;
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    let (name, options) = rest.split_at(end);
    if name.is_empty() {
        return None;
    }
    Some(icon(name, color, options, size))
}

fn icon(name: &str, color: Option<&str>, options: &str, size: f32) -> Result<Icon, String> {
    let Ok(at) = open_iconic::ICONS.binary_search_by(|(n, ..)| n.cmp(&name)) else {
        return Err(format!("`<&{name}>`: Open Iconic has no icon of that name"));
    };
    let (_, d, dx, dy) = open_iconic::ICONS[at];
    let (scale, own) = options_of(options)?;
    let color = match own.or(color) {
        Some(value) => {
            Some(color::parse(value)?.ok_or_else(|| format!("`<&{name}>` cannot be transparent"))?)
        }
        None => None,
    };
    let side = size * CAP * scale;
    Ok(Icon {
        picture: Picture::Path { d, dx, dy },
        width: side,
        height: side,
        color,
    })
}

/// The sprite a use carries (`:16x16:00F…`, as [`sprite::take`] wrote
/// it), drawn a unit a pixel at scale 1.
fn sprite_tag(rest: &str, color: Option<&str>) -> Result<Icon, String> {
    let Some(inline) = rest.strip_prefix(':') else {
        let name: String = rest
            .chars()
            .take_while(|c| !matches!(c, '*' | '{' | ','))
            .collect();
        return Err(format!("`<${name}>`: no sprite of that name is defined"));
    };
    let end = inline
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == ':'))
        .unwrap_or(inline.len());
    let (data, options) = inline.split_at(end);
    let sprite = sprite::Sprite::read_inline(data).ok_or("a sprite's pixels cannot be read")?;
    let (scale, own) = options_of(options)?;
    let color = match own.or(color) {
        Some(value) => {
            Some(color::parse(value)?.ok_or("a sprite cannot be transparent".to_owned())?)
        }
        None => None,
    };
    Ok(Icon {
        width: sprite.width as f32 * scale,
        height: sprite.height as f32 * scale,
        picture: Picture::Pixels(std::sync::Arc::new(sprite)),
        color,
    })
}

/// The scale and the color an icon's options give: `*2`, `{scale=2}`,
/// `,scale=2`, `,color=red`, `{scale=2,color=#f00}`.
fn options_of(options: &str) -> Result<(f32, Option<&str>), String> {
    let inner = options
        .trim()
        .trim_start_matches(['{', ','])
        .trim_end_matches('}');
    let mut scale = 1.0;
    let mut color = None;
    for part in inner.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if let Some(value) = part
            .strip_prefix('*')
            .or_else(|| part.strip_prefix("scale="))
        {
            scale = value
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|s| (0.05..=50.0).contains(s))
                .ok_or_else(|| format!("`{part}`: a scale is a number"))?;
        } else if let Some(value) = part
            .strip_prefix("color")
            .map(|v| v.trim_start_matches(['=', ':', ' ']))
        {
            color = Some(value.trim());
        } else {
            return Err(format!("`{part}` is no option of an icon"));
        }
    }
    Ok((scale, color))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_icon_is_found_by_name_with_its_size_and_color() {
        let heart = tag("&heart", 14.0).unwrap().unwrap();
        assert!(matches!(heart.picture, Picture::Path { dy: 1.0, .. }));
        assert!((heart.width - 14.0 * CAP).abs() < 1e-4);
        assert_eq!(heart.color, None);
        let big = tag("&heart*2", 14.0).unwrap().unwrap();
        assert!((big.width - 2.0 * heart.width).abs() < 1e-4);
        let braced = tag("&heart{scale=2}", 14.0).unwrap().unwrap();
        assert_eq!(braced.width, big.width);
        let red = tag("#red&heart", 14.0).unwrap().unwrap();
        assert_eq!(red.color, Some(Color::hex(0xFF0000)));
        let own = tag("&heart,color=#00ff00", 14.0).unwrap().unwrap();
        assert_eq!(own.color, Some(Color::hex(0x00FF00)));
        // Every icon of the table is found.
        for (name, ..) in open_iconic::ICONS {
            assert!(tag(&format!("&{name}"), 12.0).unwrap().is_ok(), "{name}");
        }
        assert!(tag("&hart", 14.0).unwrap().is_err());
        assert!(tag("&heart*big", 14.0).unwrap().is_err());
        assert!(tag("b", 14.0).is_none());
        assert!(tag("&", 14.0).is_none());
    }
}
