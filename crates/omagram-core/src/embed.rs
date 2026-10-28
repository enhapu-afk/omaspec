//! Diagram source inside exported images, so a PNG or SVG can be opened as
//! a diagram again.
//!
//! PNG: an `iTXt` chunk named after the language (`plantuml`, the name
//! PlantUML itself uses, or `mermaid`) and one named `Omagram file` with
//! the file name. SVG: a `<metadata>` element holding `<sf:source>` in the
//! namespace [`NAMESPACE`]. SVGs from PlantUML carry the source as a
//! `<?plantuml-src …?>` instruction, which is read too.
//!
//! Reading never trusts the image: chunk lengths are bounds-checked and
//! compressed text is inflated with a size limit.

use miniz_oxide::inflate::{decompress_to_vec_with_limit, decompress_to_vec_zlib_with_limit};

use crate::lang::Language;
use crate::text::re;

pub const NAMESPACE: &str = "urn:omagram:source";

/// The PNG text chunk holding the file name.
const FILE_KEY: &str = "Omagram file";

/// Largest text inflated from one chunk or instruction.
const MAX_TEXT: usize = 8 << 20;

const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// Source read back from an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub code: String,
    /// `None` when the image does not say; detect it from the code.
    pub language: Option<Language>,
    /// The file name the source was saved under, if known.
    pub file: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EmbedError {
    /// Not a PNG, or no `IHDR` chunk where the PNG format requires it.
    NotPng,
}

impl std::fmt::Display for EmbedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmbedError::NotPng => f.write_str("not a PNG image"),
        }
    }
}

impl std::error::Error for EmbedError {}

/// `png` with the source added as text chunks right after `IHDR`.
pub fn png_with_source(
    png: &[u8],
    code: &str,
    language: Language,
    file: Option<&str>,
) -> Result<Vec<u8>, EmbedError> {
    if !png.starts_with(&PNG_SIGNATURE) || png.get(12..16) != Some(b"IHDR") {
        return Err(EmbedError::NotPng);
    }
    let ihdr_len = u32::from_be_bytes(png[8..12].try_into().expect("4 bytes")) as usize;
    let after_ihdr = 8 + 12 + ihdr_len;
    if after_ihdr > png.len() {
        return Err(EmbedError::NotPng);
    }
    let mut out = Vec::with_capacity(png.len() + code.len() + 64);
    out.extend_from_slice(&png[..after_ihdr]);
    out.extend(itxt_chunk(language.id(), code));
    if let Some(file) = file {
        out.extend(itxt_chunk(FILE_KEY, file));
    }
    out.extend_from_slice(&png[after_ihdr..]);
    Ok(out)
}

/// An uncompressed `iTXt` chunk: keyword, NUL, flag 0, method 0, empty
/// language tag, empty translated keyword, UTF-8 text.
fn itxt_chunk(keyword: &str, text: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity(keyword.len() + 5 + text.len());
    data.extend_from_slice(keyword.as_bytes());
    data.extend_from_slice(&[0, 0, 0, 0, 0]);
    data.extend_from_slice(text.as_bytes());

    let mut chunk = Vec::with_capacity(12 + data.len());
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    chunk.extend_from_slice(b"iTXt");
    chunk.extend_from_slice(&data);
    let crc = crc32(&chunk[4..]);
    chunk.extend_from_slice(&crc.to_be_bytes());
    chunk
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// The source in a PNG's text chunks, if it has one.
pub fn source_from_png(png: &[u8]) -> Option<Source> {
    if !png.starts_with(&PNG_SIGNATURE) {
        return None;
    }
    let (mut plantuml, mut mermaid, mut file) = (None, None, None);
    let mut pos = 8;
    while pos + 12 <= png.len() {
        let len = u32::from_be_bytes(png[pos..pos + 4].try_into().expect("4 bytes")) as usize;
        let kind = &png[pos + 4..pos + 8];
        let data = &png[pos + 8..png.len().min(pos + 8 + len)];
        pos = pos.saturating_add(12).saturating_add(len);
        if kind == b"IEND" {
            break;
        }
        if !matches!(kind, b"tEXt" | b"iTXt" | b"zTXt") {
            continue;
        }
        let Some(nul) = data.iter().position(|&b| b == 0).filter(|&n| n > 0) else {
            continue;
        };
        let slot = match latin1(&data[..nul]).to_lowercase().as_str() {
            "plantuml" => &mut plantuml,
            "mermaid" => &mut mermaid,
            k if k == FILE_KEY.to_lowercase() => &mut file,
            _ => continue,
        };
        if let Some(text) = chunk_text(kind, &data[nul + 1..]) {
            *slot = Some(text);
        }
    }
    let (code, language) = match (plantuml, mermaid) {
        (Some(code), _) => (code, Language::PlantUml),
        (None, Some(code)) => (code, Language::Mermaid),
        (None, None) => return None,
    };
    if code.trim().is_empty() {
        return None;
    }
    Some(Source {
        code,
        language: Some(language),
        file: file.filter(|f| !f.is_empty()),
    })
}

/// The text of a `tEXt`, `zTXt` or `iTXt` chunk after its keyword.
fn chunk_text(kind: &[u8], rest: &[u8]) -> Option<String> {
    match kind {
        b"tEXt" => Some(latin1(rest)),
        b"zTXt" => {
            let inflated = decompress_to_vec_zlib_with_limit(rest.get(1..)?, MAX_TEXT).ok()?;
            Some(latin1(&inflated))
        }
        _ => {
            // Compression flag, method, language tag NUL, translated keyword NUL, text.
            let tag_end = 2 + rest.get(2..)?.iter().position(|&b| b == 0)?;
            let key_end = tag_end + 1 + rest[tag_end + 1..].iter().position(|&b| b == 0)?;
            let body = &rest[key_end + 1..];
            let bytes = if rest[0] == 1 {
                decompress_to_vec_zlib_with_limit(body, MAX_TEXT).ok()?
            } else {
                body.to_vec()
            };
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
    }
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// `svg` with the source as `<metadata>` right after the opening `<svg>` tag.
/// A self-closing or missing `<svg>` tag leaves it unchanged.
pub fn svg_with_source(svg: &str, code: &str, language: Language, file: Option<&str>) -> String {
    let Some(tag) = re!(r"<svg\b[^>]*>").find(svg) else {
        return svg.to_owned();
    };
    // An empty `<svg …/>` gets opened up to hold the metadata.
    let (open, close) = match tag.as_str().strip_suffix("/>") {
        Some(open) => (format!("{}>", open.trim_end()), "</svg>"),
        None => (tag.as_str().to_owned(), ""),
    };
    let file_attr = file
        .map(|f| format!(r#" file="{}""#, xml_escape(f, true)))
        .unwrap_or_default();
    let metadata = format!(
        r#"<metadata><sf:source xmlns:sf="{NAMESPACE}" lang="{}"{file_attr}>{}</sf:source></metadata>"#,
        language.id(),
        xml_escape(code, false),
    );
    let mut out = String::with_capacity(svg.len() + metadata.len() + 8);
    out.push_str(&svg[..tag.start()]);
    out.push_str(&open);
    out.push_str(&metadata);
    out.push_str(close);
    out.push_str(&svg[tag.end()..]);
    out
}

/// Escapes for XML text or attribute values and drops characters XML forbids.
fn xml_escape(s: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\n' if attribute => out.push_str("&#10;"),
            '\r' if attribute => out.push_str("&#13;"),
            '\t' if attribute => out.push_str("&#9;"),
            '\t' | '\n' | '\r' => out.push(c),
            '\u{0}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}' => {}
            _ => out.push(c),
        }
    }
    out
}

/// The source in an SVG: Omagram's own `<sf:source>` element, else
/// PlantUML's `<?plantuml-src …?>` (which has no comments and no
/// `@startuml`; both are added back).
pub fn source_from_svg(svg: &str) -> Option<Source> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: 4_000_000,
        // No resolver: external entities are never loaded.
        ..Default::default()
    };
    if let Ok(doc) = roxmltree::Document::parse_with_options(svg, options)
        && let Some(el) = doc.descendants().find(|n| {
            n.tag_name().namespace() == Some(NAMESPACE) && n.tag_name().name() == "source"
        })
    {
        let code: String = el
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect();
        if !code.trim().is_empty() {
            return Some(Source {
                code,
                language: el.attribute("lang").and_then(Language::from_id),
                file: el
                    .attribute("file")
                    .filter(|f| !f.is_empty())
                    .map(str::to_owned),
            });
        }
    }

    let encoded = re!(r"<\?plantuml-src\s+([0-9A-Za-z_-]+)\s*\?>").captures(svg)?;
    let bytes = decompress_to_vec_with_limit(&plantuml_decode64(&encoded[1]), MAX_TEXT).ok()?;
    let mut code = String::from_utf8_lossy(&bytes).into_owned();
    if code.trim().is_empty() {
        return None;
    }
    if !code.trim_start().to_ascii_lowercase().starts_with("@start") {
        code = format!("@startuml\n{}\n@enduml\n", code.trim_end());
    }
    Some(Source {
        code,
        language: Some(Language::PlantUml),
        file: None,
    })
}

/// PlantUML's base64 variant: `0-9A-Za-z-_` for 0..63.
fn plantuml_decode64(s: &str) -> Vec<u8> {
    let six = |c: u8| -> u32 {
        match c {
            b'0'..=b'9' => u32::from(c - b'0'),
            b'A'..=b'Z' => u32::from(c - b'A') + 10,
            b'a'..=b'z' => u32::from(c - b'a') + 36,
            b'-' => 62,
            b'_' => 63,
            _ => 0,
        }
    };
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3 + 3);
    for group in bytes.chunks(4) {
        let get = |i: usize| six(*group.get(i).unwrap_or(&b'A'));
        let n = (get(0) << 18) | (get(1) << 12) | (get(2) << 6) | get(3);
        out.extend_from_slice(&[(n >> 16) as u8, (n >> 8) as u8, n as u8]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest valid PNG: 1x1 transparent pixel.
    const PIXEL: [u8; 67] = [
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 10, 73, 68, 65, 84, 120, 156, 99, 0, 1, 0, 0, 5, 0, 1,
        13, 10, 45, 180, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];

    #[test]
    fn crc_matches_the_png_spec() {
        // The IEND chunk's CRC over its type.
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
    }

    #[test]
    fn png_round_trip() {
        let code = "@startuml\nA -> B : ü\n@enduml\n";
        let png = png_with_source(&PIXEL, code, Language::PlantUml, Some("flow.puml")).unwrap();
        let src = source_from_png(&png).unwrap();
        assert_eq!(src.code, code);
        assert_eq!(src.language, Some(Language::PlantUml));
        assert_eq!(src.file.as_deref(), Some("flow.puml"));
        assert_eq!(source_from_png(&PIXEL), None);
    }

    #[test]
    fn rejects_non_png() {
        assert_eq!(
            png_with_source(b"GIF89a", "x", Language::Mermaid, None),
            Err(EmbedError::NotPng)
        );
        assert_eq!(source_from_png(b"\x89PNG\r\n\x1a\n\0\0\xff\xffiTXt"), None);
    }

    #[test]
    fn svg_round_trip() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10"><rect/></svg>"#;
        let code = "flowchart TD\n  A[\"<b>&</b>\"] --> B\u{1}";
        let out = svg_with_source(svg, code, Language::Mermaid, Some("a \"b\".mmd"));
        let src = source_from_svg(&out).unwrap();
        assert_eq!(src.code, code.replace('\u{1}', ""));
        assert_eq!(src.language, Some(Language::Mermaid));
        assert_eq!(src.file.as_deref(), Some("a \"b\".mmd"));
    }

    #[test]
    fn reads_plantuml_svg_instruction() {
        // "Bob -> Alice : hello" as PlantUML encodes it.
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><?plantuml-src SyfFKj2rKt3CoKnELR1Io4ZDoSa70000?></svg>";
        let src = source_from_svg(svg).unwrap();
        assert_eq!(src.code, "@startuml\nBob -> Alice : hello\n@enduml\n");
    }

    #[test]
    fn svg_without_source() {
        assert_eq!(
            source_from_svg("<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            None
        );
        assert_eq!(source_from_svg("not xml"), None);
    }
}
