//! Excalifont, the handwriting face of a sketched diagram
//! ([ADR 0023](../../../docs/adr/0023-sketch-look.md)).
//!
//! The Latin subset (SIL Open Font License, `fonts/OFL.txt`) is loaded into
//! the font database usvg draws with, and written into an exported SVG as a
//! `@font-face`, so a browser that has never heard of the face still sets
//! the words in it. The picture on screen does not carry those bytes.

use std::sync::LazyLock;

pub use omaspec_uml::HAND_FONT;
use resvg::usvg;

const TTF: &[u8] = include_bytes!("../fonts/Excalifont-Regular.ttf");
const WOFF2: &[u8] = include_bytes!("../fonts/Excalifont-Regular.woff2");

/// Loads the face into `fonts`, for the editor's raster and every export.
pub fn load(fonts: &mut usvg::fontdb::Database) {
    fonts.load_font_data(TTF.to_vec());
}

/// `svg` with the face embedded, just inside its `<svg>` tag, when it asks
/// for it. The caller sanitizes the result, as any SVG it exports.
pub fn embed(svg: &str) -> String {
    if !svg.contains(&format!("font-family=\"{HAND_FONT}\"")) {
        return svg.to_owned();
    }
    // Past an XML declaration or a comment, at the root's own tag.
    let Some(end) = svg
        .find("<svg")
        .and_then(|open| svg[open..].find('>').map(|end| open + end))
    else {
        return svg.to_owned();
    };
    let mut out = String::with_capacity(svg.len() + FACE.len());
    out.push_str(&svg[..=end]);
    out.push_str(&FACE);
    out.push_str(&svg[end + 1..]);
    out
}

static FACE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "<style>@font-face{{font-family:{HAND_FONT};src:url(\"data:font/woff2;base64,{}\") format(\"woff2\");}}</style>",
        base64(WOFF2)
    )
});

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
