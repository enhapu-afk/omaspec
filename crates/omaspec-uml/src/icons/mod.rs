//! Pictures in a line of text: an icon of Open Iconic (`<&heart>`), drawn
//! as a path in the text's color, so it is drawn as the words are, in
//! any theme, rather than in an emoji's colors; and a sprite the diagram
//! defines (`<$bolt>`, [`sprite`]), its pixels in that color too.
//!
//! The tag is PlantUML's: `<&name>`, its size `<&name*2>`,
//! `<&name{scale=2}>` or `<&name,scale=2>`, its color `<#red&name>` or
//! `<&name,color=red>`. The icon stands on the baseline, as tall as a
//! capital of the text at scale 1. A name Open Iconic does not have is
//! drawn as nothing, as PlantUML draws it; lint names it. A picture file
//! (`<img:logo.png>`, [`raster`]) stands on the baseline in its own size.

pub(crate) mod archimate;
mod open_iconic;
pub(crate) mod raster;
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
    /// Line art in a box 16 by 16, stroked or filled: ArchiMate's icons
    /// (`<$archimate/business-actor>`).
    Line { d: String, filled: bool },
    /// An arrow the handwriting face lacks (`→`), drawn by hand in its
    /// place.
    Arrow(char),
    /// A diagram inside the text (`{{` to `}}`), as the engine drew it.
    Diagram(std::sync::Arc<crate::embedded::Embedded>),
    /// A picture file (`<img:logo.png>`), embedded as it is.
    Raster(std::sync::Arc<raster::Raster>),
}

/// The side of an Open Iconic icon in its own units.
pub const SIDE: f32 = 8.0;

/// How tall an icon at scale 1 is, as a share of the text's size.
const CAP: f32 = 0.78;

/// The icon the tag `tag` (between `<` and `>`) asks for in text of
/// `size`; `None` when it is no icon tag, an error when it cannot be
/// drawn, and no icon for a name Open Iconic does not have.
pub(crate) fn tag(tag: &str, size: f32) -> Option<Result<Option<Icon>, String>> {
    let (color, rest) = match tag.strip_prefix('#') {
        Some(after) => {
            let end = after.find(['&', '$'])?;
            (Some(&after[..end]), &after[end..])
        }
        None => (None, tag),
    };
    if let Some(rest) = rest.strip_prefix('$') {
        return Some(sprite_tag(rest, color).map(Some));
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

fn icon(name: &str, color: Option<&str>, options: &str, size: f32) -> Result<Option<Icon>, String> {
    let Some(at) = open_iconic_at(name) else {
        return Ok(None);
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
    Ok(Some(Icon {
        picture: Picture::Path { d, dx, dy },
        width: side,
        height: side,
        color,
    }))
}

fn open_iconic_at(name: &str) -> Option<usize> {
    open_iconic::ICONS
        .binary_search_by(|(n, ..)| n.cmp(&name))
        .ok()
}

/// The names of Open Iconic's icons, as `<&name>` takes them, in order.
pub fn open_iconic_names() -> impl Iterator<Item = &'static str> {
    open_iconic::ICONS.iter().map(|(name, ..)| *name)
}

/// The Open Iconic names that `text` asks for and the set does not have,
/// each with where its tag stands in `text` (`<&camera>`). PlantUML draws
/// nothing in their place.
pub fn unknown_open_iconic(text: &str) -> Vec<(std::ops::Range<usize>, &str)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(open) = text[from..].find('<').map(|at| from + at) {
        from = open + 1;
        let Some(close) = text[open..].find('>').map(|at| open + at) else {
            break;
        };
        let inside = &text[open + 1..close];
        let Some(amp) = inside.find('&') else {
            continue;
        };
        // `<&name…>` or `<#color&name…>`, nothing else between.
        if amp > 0 && !inside.starts_with('#') {
            continue;
        }
        let rest = &inside[amp + 1..];
        let name = &rest[..rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(rest.len())];
        if !name.is_empty() && open_iconic_at(name).is_none() {
            found.push((open..close + 1, name));
        }
    }
    found
}

/// The sprite a use carries (`:16x16:00F…`, as [`sprite::take`] wrote
/// it), drawn a unit a pixel at scale 1.
fn sprite_tag(rest: &str, color: Option<&str>) -> Result<Icon, String> {
    // PlantUML's own: ArchiMate's icons, which no diagram defines.
    if let Some(named) = rest.strip_prefix("archimate/") {
        let end = named
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_')))
            .unwrap_or(named.len());
        let (name, options) = named.split_at(end);
        let (d, filled) = archimate::icon(name).ok_or_else(|| {
            format!("`<$archimate/{name}>`: ArchiMate has no element of that name")
        })?;
        let (scale, own) = options_of(options)?;
        let color = match own.or(color) {
            Some(value) => {
                Some(color::parse(value)?.ok_or("an icon cannot be transparent".to_owned())?)
            }
            None => None,
        };
        let side = 16.0 * scale;
        return Ok(Icon {
            picture: Picture::Line { d, filled },
            width: side,
            height: side,
            color,
        });
    }
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
        let icon = |text: &str| tag(text, 14.0).unwrap().unwrap().unwrap();
        let heart = icon("&heart");
        assert!(matches!(heart.picture, Picture::Path { dy: 1.0, .. }));
        assert!((heart.width - 14.0 * CAP).abs() < 1e-4);
        assert_eq!(heart.color, None);
        let big = icon("&heart*2");
        assert!((big.width - 2.0 * heart.width).abs() < 1e-4);
        assert_eq!(icon("&heart{scale=2}").width, big.width);
        assert_eq!(icon("#red&heart").color, Some(Color::hex(0xFF0000)));
        assert_eq!(
            icon("&heart,color=#00ff00").color,
            Some(Color::hex(0x00FF00))
        );
        // Every icon of the table is found.
        for name in open_iconic_names() {
            assert!(
                matches!(tag(&format!("&{name}"), 12.0), Some(Ok(Some(_)))),
                "{name}"
            );
        }
        // A name the set lacks is no icon, as PlantUML draws it; a size
        // that is no number is still a mistake.
        assert_eq!(tag("&hart", 14.0), Some(Ok(None)));
        assert!(tag("&heart*big", 14.0).unwrap().is_err());
        assert!(tag("b", 14.0).is_none());
        assert!(tag("&", 14.0).is_none());
    }

    #[test]
    fn the_names_open_iconic_lacks_are_found_where_they_stand() {
        let text = "<&camera> photo <&camera-slr> <#red&kye,scale=2> <b>x</b> a<&";
        let found = unknown_open_iconic(text);
        let names: Vec<&str> = found.iter().map(|(_, name)| *name).collect();
        assert_eq!(names, ["camera", "kye"]);
        assert_eq!(&text[found[0].0.clone()], "<&camera>");
        assert_eq!(&text[found[1].0.clone()], "<#red&kye,scale=2>");
        // A sprite and a tag of another kind are not Open Iconic's.
        assert!(unknown_open_iconic("<$bolt> <color:red&x> <&heart>").is_empty());
    }
}
