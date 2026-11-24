//! Pictures in text (`<img:logo.png>`, `<img logo.png>`,
//! `<img:logo.png{scale=0.5}>`). The engine reads no file: before a
//! diagram is drawn, the picture files its text names are read through
//! [`Includes::picture`] and kept for the thread while it is parsed and
//! drawn ([`within`]); a picture is then written into the SVG as it is,
//! PNG, JPEG, GIF or WebP. One that cannot be read is said in the text in
//! its place, as PlantUML says it cannot decode it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::Includes;

/// A picture file, as the SVG embeds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    /// `image/png`, …
    pub mime: &'static str,
    /// Its bytes in base64.
    pub data: String,
    /// Its size in pixels.
    pub width: u32,
    pub height: u32,
}

thread_local! {
    /// The pictures of the diagram being drawn, by the path its text
    /// writes.
    static PICTURES: RefCell<HashMap<String, Result<Arc<Raster>, Unread>>> =
        RefCell::new(HashMap::new());
}

/// Why a picture is not drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unread {
    /// The file cannot be read, or nothing says where the diagram is.
    Missing,
    /// It is read, and not a PNG, JPEG, GIF or WebP.
    Kind,
}

/// Runs `draw` with the pictures `source`'s text names read through
/// `includes`, and forgets them after, also when it panics.
pub(crate) fn within<T>(
    source: &str,
    includes: Option<&dyn Includes>,
    draw: impl FnOnce() -> T,
) -> T {
    let mut read = HashMap::new();
    for path in paths(source) {
        let picture = match includes.map(|includes| includes.picture(&path)) {
            Some(Ok(bytes)) => raster(&bytes).map(Arc::new).ok_or(Unread::Kind),
            _ => Err(Unread::Missing),
        };
        read.insert(path, picture);
    }
    struct Forget(HashMap<String, Result<Arc<Raster>, Unread>>);
    impl Drop for Forget {
        fn drop(&mut self) {
            let before = std::mem::take(&mut self.0);
            PICTURES.with_borrow_mut(|pictures| *pictures = before);
        }
    }
    let before = PICTURES.with_borrow_mut(|pictures| std::mem::replace(pictures, read));
    let _forget = Forget(before);
    draw()
}

/// The picture `path` names, as read for the diagram being drawn.
pub(crate) fn find(path: &str) -> Result<Arc<Raster>, Unread> {
    PICTURES.with_borrow(|pictures| pictures.get(path).cloned().unwrap_or(Err(Unread::Missing)))
}

/// The path and the scale of a picture tag, the text between `<` and `>`:
/// `img:logo.png`, `img logo.png`, `img:logo.png{scale=0.5}`.
pub(crate) fn tag(tag: &str) -> Option<(&str, f32)> {
    let head = tag.get(..3)?;
    if !head.eq_ignore_ascii_case("img") {
        return None;
    }
    let rest = &tag[3..];
    let rest = rest
        .strip_prefix(':')
        .or_else(|| rest.strip_prefix(char::is_whitespace))?
        .trim();
    let (path, options) = match rest.find('{') {
        Some(open) if rest.ends_with('}') => (&rest[..open], &rest[open + 1..rest.len() - 1]),
        _ => (rest, ""),
    };
    let scale = options
        .split([',', ';'])
        .filter_map(|option| option.split_once(['=', ':']))
        .find(|(key, _)| key.trim().eq_ignore_ascii_case("scale"))
        .and_then(|(_, value)| value.trim().parse::<f32>().ok())
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .unwrap_or(1.0)
        .min(10.0);
    let path = path.trim().trim_matches('"');
    (!path.is_empty()).then_some((path, scale))
}

/// The paths of the pictures `source` names, each once.
fn paths(source: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = source;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find(['>', '\n']) else {
            break;
        };
        if rest.as_bytes()[close] == b'>'
            && let Some((path, _)) = tag(&rest[..close])
            && !out.iter().any(|p| p == path)
        {
            out.push(path.to_owned());
        }
    }
    out
}

/// The picture in `bytes`, by its kind and size; `None` for one the SVG
/// does not embed.
pub fn raster(bytes: &[u8]) -> Option<Raster> {
    let (mime, width, height) = size(bytes)?;
    (width > 0 && height > 0).then(|| Raster {
        mime,
        data: base64(bytes),
        width,
        height,
    })
}

/// The kind of picture in `bytes` and its size in pixels, from its header.
fn size(b: &[u8]) -> Option<(&'static str, u32, u32)> {
    let be16 = |at: usize| Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]) as u32);
    let le16 = |at: usize| Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]) as u32);
    let be32 = |at: usize| {
        Some(u32::from_be_bytes([
            *b.get(at)?,
            *b.get(at + 1)?,
            *b.get(at + 2)?,
            *b.get(at + 3)?,
        ]))
    };
    let le24 = |at: usize| {
        Some(u32::from_le_bytes([
            *b.get(at)?,
            *b.get(at + 1)?,
            *b.get(at + 2)?,
            0,
        ]))
    };
    if b.starts_with(b"\x89PNG\r\n\x1a\n") && b.get(12..16) == Some(b"IHDR") {
        return Some(("image/png", be32(16)?, be32(20)?));
    }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return Some(("image/gif", le16(6)?, le16(8)?));
    }
    if b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        return match b.get(12..16)? {
            b"VP8 " => Some(("image/webp", le16(26)? & 0x3fff, le16(28)? & 0x3fff)),
            b"VP8L" => {
                let bits = u32::from_le_bytes([*b.get(21)?, *b.get(22)?, *b.get(23)?, *b.get(24)?]);
                Some((
                    "image/webp",
                    (bits & 0x3fff) + 1,
                    ((bits >> 14) & 0x3fff) + 1,
                ))
            }
            b"VP8X" => Some(("image/webp", le24(24)? + 1, le24(27)? + 1)),
            _ => None,
        };
    }
    if b.starts_with(&[0xff, 0xd8]) {
        // The segments up to the frame's header, which holds the size.
        let mut at = 2;
        while at + 4 <= b.len() {
            if b[at] != 0xff {
                return None;
            }
            let marker = b[at + 1];
            if marker == 0xff {
                at += 1;
                continue;
            }
            let frame = matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc);
            if frame {
                return Some(("image/jpeg", be16(at + 7)?, be16(at + 5)?));
            }
            at += 2 + be16(at + 2)? as usize;
        }
    }
    None
}

/// `bytes` in base64, padded.
fn base64(bytes: &[u8]) -> String {
    const SIGNS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(SIGNS[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG of `w` by `h` pixels: its header alone is read.
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend(w.to_be_bytes());
        b.extend(h.to_be_bytes());
        b.extend([8, 6, 0, 0, 0]);
        b
    }

    #[test]
    fn the_size_comes_from_the_header() {
        assert_eq!(size(&png(40, 30)), Some(("image/png", 40, 30)));
        let gif = b"GIF89a\x20\x00\x10\x00".to_vec();
        assert_eq!(size(&gif), Some(("image/gif", 32, 16)));
        // A JPEG: an APP0 segment, then the frame's header.
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0, 0];
        jpeg.extend([0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x18, 0x00, 0x2a]);
        assert_eq!(size(&jpeg), Some(("image/jpeg", 42, 24)));
        let mut vp8x = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0".to_vec();
        vp8x.extend([9, 0, 0, 4, 0, 0]);
        assert_eq!(size(&vp8x), Some(("image/webp", 10, 5)));
        assert_eq!(size(b"<svg/>"), None);
        assert_eq!(size(&png(40, 30)[..20]), None);
    }

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn tags_and_paths() {
        assert_eq!(tag("img:logo.png"), Some(("logo.png", 1.0)));
        assert_eq!(tag("img logo.png"), Some(("logo.png", 1.0)));
        assert_eq!(tag("IMG:a b.png{scale=0.5}"), Some(("a b.png", 0.5)));
        assert_eq!(tag("img:x.png{scale=-1}"), Some(("x.png", 1.0)));
        assert_eq!(tag("img"), None);
        assert_eq!(tag("image:x"), None);
        assert_eq!(tag("img:"), None);
        assert_eq!(
            paths("a <img:x.png> <img y.gif{scale=2}> <img:x.png>\n<img:z"),
            ["x.png", "y.gif"]
        );
    }

    struct One;
    impl Includes for One {
        fn read(&self, path: &str, _: Option<&str>) -> Result<crate::Included, String> {
            Err(format!("`{path}` is not there"))
        }
        fn picture(&self, path: &str) -> Result<Vec<u8>, String> {
            match path {
                "logo.png" => Ok(png(40, 30)),
                "notes.txt" => Ok(b"words".to_vec()),
                _ => Err(format!("`{path}` is not there")),
            }
        }
    }

    #[test]
    fn pictures_are_kept_while_drawing_and_forgotten_after() {
        let source = "<img:logo.png> <img:notes.txt> <img:gone.png>";
        within(source, Some(&One), || {
            let logo = find("logo.png").unwrap();
            assert_eq!((logo.width, logo.height), (40, 30));
            assert_eq!(find("notes.txt"), Err(Unread::Kind));
            assert_eq!(find("gone.png"), Err(Unread::Missing));
            // A diagram inside reads its own, and the outer ones return.
            within("<img:other.png>", Some(&One), || {
                assert_eq!(find("logo.png"), Err(Unread::Missing));
            });
            assert!(find("logo.png").is_ok());
        });
        assert_eq!(find("logo.png"), Err(Unread::Missing));
        within(source, None, || {
            assert_eq!(find("logo.png"), Err(Unread::Missing));
        });
    }
}
