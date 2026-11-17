//! The faces shipped with the program: Inter, which a diagram's words are
//! set in unless it names another ([`omaspec_uml::FACE`]), and Excalifont,
//! the handwriting face of a sketched diagram
//! ([ADR 0023](../../../docs/adr/0023-sketch-look.md)).
//!
//! Both are subsets (SIL Open Font License, `fonts/OFL.txt` and
//! `fonts/OFL-Inter.txt`). They are loaded into the font database usvg
//! draws with, and written into an exported SVG as `@font-face`s, so a
//! browser that has never heard of a face still sets the words in it. The
//! picture on screen does not carry those bytes.
//!
//! Inter comes in four cuts. The ones drawn with (`.ttf`) have Latin,
//! Greek, Cyrillic, arrows and signs; the ones an SVG carries (`.woff2`,
//! 16 KB each) have what [`omaspec_uml`] measures: Latin up to Extended-A,
//! the common punctuation and arrows. Neither has Inter's contextual
//! alternates, which would draw `->` as an arrow: a diagram shows what was
//! written.

use std::sync::LazyLock;

pub use omaspec_uml::{FACE, HAND_FONT};
use resvg::usvg;

const TTF: &[u8] = include_bytes!("../fonts/Excalifont-Regular.ttf");
const WOFF2: &[u8] = include_bytes!("../fonts/Excalifont-Regular.woff2");

/// A cut of Inter: whether it is bold and italic, the face drawn with and
/// the one an SVG carries.
struct Cut {
    bold: bool,
    italic: bool,
    ttf: &'static [u8],
    woff2: &'static [u8],
}

const CUTS: [Cut; 4] = [
    Cut {
        bold: false,
        italic: false,
        ttf: include_bytes!("../fonts/Inter-Regular.ttf"),
        woff2: include_bytes!("../fonts/Inter-Regular.woff2"),
    },
    Cut {
        bold: true,
        italic: false,
        ttf: include_bytes!("../fonts/Inter-Bold.ttf"),
        woff2: include_bytes!("../fonts/Inter-Bold.woff2"),
    },
    Cut {
        bold: false,
        italic: true,
        ttf: include_bytes!("../fonts/Inter-Italic.ttf"),
        woff2: include_bytes!("../fonts/Inter-Italic.woff2"),
    },
    Cut {
        bold: true,
        italic: true,
        ttf: include_bytes!("../fonts/Inter-BoldItalic.ttf"),
        woff2: include_bytes!("../fonts/Inter-BoldItalic.woff2"),
    },
];

/// Loads the faces into `fonts`, for the editor's raster and every export.
/// Called before the system's fonts are loaded: of two faces as fitting
/// the first one draws, so a picture is set in these cuts whether or not
/// an Inter is installed, and looks the same on every machine.
pub fn load(fonts: &mut usvg::fontdb::Database) {
    for cut in &CUTS {
        fonts.load_font_data(cut.ttf.to_vec());
    }
    fonts.load_font_data(TTF.to_vec());
}

/// `svg` with the faces its words ask for embedded, just inside its
/// `<svg>` tag: the handwriting face, and each cut of Inter that words
/// are set in. The caller sanitizes the result, as any SVG it exports.
pub fn embed(svg: &str) -> String {
    let mut faces = String::new();
    if svg.contains(&format!("font-family=\"{HAND_FONT}\"")) {
        faces.push_str(&HAND);
    }
    let used = cuts(svg);
    for (cut, face) in CUTS.iter().zip(INTER.iter()) {
        if used.contains(&(cut.bold, cut.italic)) {
            faces.push_str(face);
        }
    }
    if faces.is_empty() {
        return svg.to_owned();
    }
    // Past an XML declaration or a comment, at the root's own tag.
    let Some(end) = svg
        .find("<svg")
        .and_then(|open| svg[open..].find('>').map(|end| open + end))
    else {
        return svg.to_owned();
    };
    let mut out = String::with_capacity(svg.len() + faces.len() + 15);
    out.push_str(&svg[..=end]);
    out.push_str("<style>");
    out.push_str(&faces);
    out.push_str("</style>");
    out.push_str(&svg[end + 1..]);
    out
}

/// The cuts of Inter the words of `svg` are set in, as (bold, italic):
/// those of the runs inside a text that names Inter first, without the
/// runs that name a face of their own (`""code""`). A text without runs
/// is set in the regular cut.
fn cuts(svg: &str) -> Vec<(bool, bool)> {
    let ours = format!("font-family=\"{FACE}");
    let mut found = Vec::new();
    let mut rest = svg;
    while let Some(at) = rest.find("<text") {
        rest = &rest[at..];
        let end = rest.find("</text>").unwrap_or(rest.len());
        let (text, after) = rest.split_at(end);
        rest = after;
        let open = &text[..text.find('>').unwrap_or(text.len())];
        if !open.contains(&ours) {
            continue;
        }
        let cut = |tag: &str| {
            (
                tag.contains("font-weight=\"bold\""),
                tag.contains("font-style=\"italic\""),
            )
        };
        let mut runs = 0;
        for run in text.split("<tspan").skip(1) {
            let tag = &run[..run.find('>').unwrap_or(run.len())];
            runs += 1;
            if !tag.contains("font-family=") && !found.contains(&cut(tag)) {
                found.push(cut(tag));
            }
        }
        if runs == 0 && !found.contains(&cut(open)) {
            found.push(cut(open));
        }
    }
    found
}

static HAND: LazyLock<String> = LazyLock::new(|| {
    format!(
        "@font-face{{font-family:{HAND_FONT};src:url(\"data:font/woff2;base64,{}\") format(\"woff2\");}}",
        base64(WOFF2)
    )
});

static INTER: LazyLock<Vec<String>> = LazyLock::new(|| {
    CUTS.iter()
        .map(|cut| {
            format!(
                "@font-face{{font-family:{FACE};font-weight:{};font-style:{};src:url(\"data:font/woff2;base64,{}\") format(\"woff2\");}}",
                if cut.bold { 700 } else { 400 },
                if cut.italic { "italic" } else { "normal" },
                base64(cut.woff2)
            )
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(texts: &str) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40">{texts}</svg>"#)
    }

    /// An exported SVG carries the cuts of Inter its words are set in and
    /// no other: 21 KB of text each.
    #[test]
    fn an_svg_carries_the_cuts_its_words_are_set_in() {
        let faces = omaspec_uml::INTER_FACES;
        let plain = picture(&format!(
            r#"<text font-family="{faces}"><tspan x="1" y="9" font-size="12">Idle</tspan></text>"#
        ));
        let carried = embed(&plain);
        assert_eq!(carried.matches("@font-face").count(), 1, "{carried}");
        assert!(carried.contains("font-family:Inter;font-weight:400;font-style:normal;"));
        assert!(carried.starts_with(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40"><style>@font-face"#
        ));
        assert!(carried.len() < plain.len() + 23_000, "{}", carried.len());

        let mixed = picture(&format!(
            r#"<text font-family="{faces}"><tspan font-weight="bold">Idle</tspan><tspan font-style="italic">on</tspan><tspan font-weight="bold" font-family="Courier New">x</tspan></text>"#
        ));
        assert_eq!(cuts(&mixed), [(true, false), (false, true)]);
        assert_eq!(embed(&mixed).matches("@font-face").count(), 2);

        // Another face's words, and a picture without words, carry none.
        let other = picture(r#"<text font-family="Lato"><tspan>Idle</tspan></text>"#);
        assert_eq!(embed(&other), other);
        assert_eq!(embed(&picture("")), picture(""));
        // A text without runs is set in the regular cut.
        let bare = picture(r#"<text font-family="Inter, sans-serif">Idle</text>"#);
        assert_eq!(cuts(&bare), [(false, false)]);
    }

    /// The cuts are the ones drawn with, whatever is installed, and each
    /// has the characters the engine measures.
    #[test]
    fn the_four_cuts_are_loaded_first() {
        let mut fonts = usvg::fontdb::Database::new();
        load(&mut fonts);
        let inter: Vec<_> = fonts
            .faces()
            .filter(|face| face.families.iter().any(|(name, _)| name == FACE))
            .collect();
        assert_eq!(inter.len(), 4);
        for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
            let query = usvg::fontdb::Query {
                families: &[usvg::fontdb::Family::Name(FACE)],
                weight: if bold {
                    usvg::fontdb::Weight::BOLD
                } else {
                    usvg::fontdb::Weight::NORMAL
                },
                style: if italic {
                    usvg::fontdb::Style::Italic
                } else {
                    usvg::fontdb::Style::Normal
                },
                ..Default::default()
            };
            let id = fonts.query(&query).expect("the cut is there");
            let face = fonts.face(id).unwrap();
            assert_eq!(face.weight.0 >= 700, bold);
            assert_eq!(face.style == usvg::fontdb::Style::Italic, italic);
        }
    }
}
