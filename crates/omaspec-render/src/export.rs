//! Exports a rendered SVG as SVG, PNG, JPEG, PDF, Markdown or a ZIP.
//!
//! SVG and PNG carry the diagram source (see `omaspec_core::embed`), so
//! dropping the file opens it again. The PDF stores that SVG and the source
//! as attachments, plus a title. The ZIP holds the SVG, the source file and
//! that PDF. JPEG is the picture only.

mod glyphs;

use std::collections::HashMap;
use std::fmt;
use std::io::{self, Cursor, Write};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use krilla::embed::{AssociationKind, EmbeddedFile, MimeType};
use krilla::geom::Size;
use krilla::metadata::{DateTime, Metadata};
use krilla::page::PageSettings;
use krilla::{Data, Document};
use krilla_svg::{SurfaceExt, SvgSettings};
use omaspec_core::{Language, embed, lang};
pub use resvg::{tiny_skia, usvg};
use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

/// Largest PNG or JPEG export, in pixels; beyond this use SVG or PDF.
const MAX_PIXELS: u64 = 100_000_000;
/// JPEG quality. 90 keeps full chroma, so thin lines stay sharp.
const JPEG_QUALITY: u8 = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Svg,
    Png,
    Jpeg,
    Pdf,
    Markdown,
    Zip,
}

impl Format {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "svg" => Some(Format::Svg),
            "png" => Some(Format::Png),
            "jpg" | "jpeg" => Some(Format::Jpeg),
            "pdf" => Some(Format::Pdf),
            "md" | "markdown" => Some(Format::Markdown),
            "zip" => Some(Format::Zip),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Svg => "svg",
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Pdf => "pdf",
            Format::Markdown => "md",
            Format::Zip => "zip",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Svg => "image/svg+xml",
            Format::Png => "image/png",
            Format::Jpeg => "image/jpeg",
            Format::Pdf => "application/pdf",
            Format::Markdown => "text/markdown",
            Format::Zip => "application/zip",
        }
    }
}

/// The diagram source to store in the export.
#[derive(Debug, Clone, Copy)]
pub struct Source<'a> {
    pub code: &'a str,
    pub language: Language,
    /// The file name, so the image can be opened under it again.
    pub file: Option<&'a str>,
}

#[derive(Debug, Clone, Copy)]
pub struct Options<'a> {
    /// PNG and JPEG pixels per SVG unit.
    pub scale: f32,
    /// Stored in SVG and PNG. The PDF attaches it, and Markdown and ZIP are
    /// built from it. JPEG leaves it out.
    pub source: Option<Source<'a>>,
    /// Laid over an SVG, in a ZIP's too ([`crate::anchors`]).
    pub links: &'a [crate::anchors::Anchor],
    /// How many folders the picture lies below its project's root: how far
    /// up its links may lead.
    pub up: usize,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self {
            scale: 2.0,
            source: None,
            links: &[],
            up: 0,
        }
    }
}

#[derive(Debug)]
pub enum ExportError {
    Svg(String),
    TooLarge {
        width: u32,
        height: u32,
        /// The largest scale at which the whole picture fits, when known.
        fits: Option<f32>,
    },
    Pdf(String),
    Jpeg(String),
    Zip(String),
    /// Markdown and ZIP need the diagram source.
    NeedsSource,
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::Svg(e) => write!(f, "cannot read the SVG: {e}"),
            ExportError::TooLarge {
                width,
                height,
                fits: Some(fits),
            } if *fits > 0.0 => write!(
                f,
                "a {width}×{height} px image is too large (at most {} million pixels); it fits at a scale of {fits} or less, or export SVG or PDF",
                MAX_PIXELS / 1_000_000
            ),
            ExportError::TooLarge { width, height, .. } => write!(
                f,
                "a {width}×{height} px image is too large (at most {} million pixels); export SVG or PDF",
                MAX_PIXELS / 1_000_000
            ),
            ExportError::Pdf(e) => write!(f, "cannot write the PDF: {e}"),
            ExportError::Jpeg(e) => write!(f, "cannot write the JPEG: {e}"),
            ExportError::Zip(e) => write!(f, "cannot write the ZIP: {e}"),
            ExportError::NeedsSource => f.write_str("this export needs the diagram source"),
        }
    }
}

impl std::error::Error for ExportError {}

/// `svg`, sanitized, with `options.links` laid over it and the handwriting
/// face embedded when its words ask for it, and the whole sanitized once
/// more with links that may go up to the project. Every exported SVG is
/// this; only the embedded source is written after it.
fn linked(svg: &str, options: &Options) -> Result<String, ExportError> {
    let faced = crate::fonts::embed(svg);
    if options.links.is_empty() && faced.len() == svg.len() {
        return Ok(faced);
    }
    let links = crate::sanitize::Links { up: options.up };
    crate::sanitize::sanitize_with(&crate::anchors::lay(&faced, options.links), links)
        .map_err(|e| ExportError::Svg(e.to_string()))
}

pub fn export(svg: &str, format: Format, options: &Options) -> Result<Vec<u8>, ExportError> {
    match format {
        Format::Svg => {
            let svg = linked(svg, options)?;
            Ok(match options.source {
                Some(s) => embed::svg_with_source(&svg, s.code, s.language, s.file),
                None => svg,
            }
            .into_bytes())
        }
        Format::Png => {
            let png = png(&parse(svg)?, options.scale)?;
            Ok(match options.source {
                Some(s) => embed::png_with_source(&png, s.code, s.language, s.file)
                    .expect("resvg writes valid PNGs"),
                None => png,
            })
        }
        Format::Jpeg => jpeg(&parse(svg)?, options.scale),
        Format::Pdf => pdf(svg, &parse(svg)?, options),
        Format::Markdown => markdown(options.source.ok_or(ExportError::NeedsSource)?),
        Format::Zip => zip_bundle(svg, options),
    }
}

/// System fonts, loaded once (about 150 ms) and shared by every export.
static SVG_OPTIONS: LazyLock<usvg::Options<'static>> = LazyLock::new(|| {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_system_fonts();
    crate::fonts::load(&mut fonts);
    generic_families(&mut fonts);
    usvg::Options {
        fontdb: Arc::new(fonts),
        font_resolver: usvg::FontResolver {
            select_font: usvg::FontResolver::default_font_selector(),
            select_fallback: Box::new(|c, used, fonts| fallback(c, used, fonts)),
        },
        // Only embedded images: an SVG naming a local file must not pull
        // that file into an export.
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        resources_dir: None,
        ..Default::default()
    }
});

/// The face that draws `c`, which the faces `used` so far do not have: an
/// emoji face first, then one of symbols alone, then one as wide or as
/// narrow as the text's own, then any.
///
/// usvg takes the first face that has the character, and sets the whole
/// text in it when it has all of the text's characters. A text face found
/// first for an emoji (a monospace face with a check mark of its own) so
/// took over a whole label, in black. A face without letters cannot: it
/// draws the one character, and the label keeps its own.
fn fallback(
    c: char,
    used: &[usvg::fontdb::ID],
    fonts: &usvg::fontdb::Database,
) -> Option<usvg::fontdb::ID> {
    type Asked = (char, Vec<usvg::fontdb::ID>);
    // Every face is opened to ask it, so an answer is kept.
    static FOUND: LazyLock<Mutex<HashMap<Asked, Option<usvg::fontdb::ID>>>> =
        LazyLock::new(Mutex::default);
    let asked = (c, used.to_vec());
    let kept = FOUND.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(found) = kept.get(&asked) {
        return *found;
    }
    drop(kept);

    let has = |id, c| glyphs::face_has(fonts, id, c);
    let emoji = |face: &usvg::fontdb::FaceInfo| {
        face.families
            .iter()
            .any(|(name, _)| name.to_ascii_lowercase().contains("emoji"))
    };
    let own = used.first().and_then(|id| fonts.face(*id));
    let free = |face: &&usvg::fontdb::FaceInfo| !used.contains(&face.id);
    let mut found = fonts
        .faces()
        .filter(free)
        .find(|face| emoji(face) && has(face.id, c))
        .map(|face| (0, face.id));
    if found.is_none() {
        for face in fonts.faces().filter(free).filter(|face| !emoji(face)) {
            if !has(face.id, c) {
                continue;
            }
            let rank = if !has(face.id, 'a') {
                1
            } else if own.is_none_or(|own| own.monospaced == face.monospaced) {
                2
            } else {
                3
            };
            if found.is_none_or(|(best, _)| rank < best) {
                found = Some((rank, face.id));
            }
            if rank == 1 {
                break;
            }
        }
    }
    let found = found.map(|(_, id)| id);
    FOUND
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(asked, found);
    found
}

/// Points `sans-serif`, `serif` and `monospace` at installed fonts,
/// those with Arial's, Times' and Courier's metrics first: Omaspec's
/// PlantUML engine measures text with them. fontdb's defaults are Arial,
/// Times New Roman and Courier New, which Linux rarely has; without them
/// usvg falls back to whichever font comes first.
fn generic_families(fonts: &mut usvg::fontdb::Database) {
    const SANS: [&str; 7] = [
        "Liberation Sans",
        "Arimo",
        "Arial",
        "Helvetica",
        "Nimbus Sans",
        "DejaVu Sans",
        "Noto Sans",
    ];
    const SERIF: [&str; 6] = [
        "Liberation Serif",
        "Tinos",
        "Times New Roman",
        "Nimbus Roman",
        "DejaVu Serif",
        "Noto Serif",
    ];
    const MONO: [&str; 6] = [
        "Liberation Mono",
        "Cousine",
        "Courier New",
        "Nimbus Mono PS",
        "DejaVu Sans Mono",
        "Noto Sans Mono",
    ];
    let first = |fonts: &usvg::fontdb::Database, names: &[&str]| {
        names
            .iter()
            .find(|name| {
                fonts
                    .faces()
                    .any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
            })
            .map(|name| (*name).to_owned())
    };
    if let Some(name) = first(fonts, &SANS) {
        fonts.set_sans_serif_family(name);
    }
    if let Some(name) = first(fonts, &SERIF) {
        fonts.set_serif_family(name);
    }
    if let Some(name) = first(fonts, &MONO) {
        fonts.set_monospace_family(name);
    }
}

/// Parses `svg` with Omaspec's fonts and image rules.
pub fn parse(svg: &str) -> Result<usvg::Tree, ExportError> {
    usvg::Tree::from_str(svg, &SVG_OPTIONS).map_err(|e| ExportError::Svg(e.to_string()))
}

/// The faces every export draws with.
fn fonts() -> &'static usvg::fontdb::Database {
    &SVG_OPTIONS.fontdb
}

/// The characters of `svg`'s words that no installed face draws, each
/// once: a PNG, JPEG or PDF of it shows an empty box for each.
pub fn undrawable(svg: &str) -> Result<Vec<char>, ExportError> {
    Ok(glyphs::undrawable(&parse(svg)?, fonts()))
}

/// Rasterizes `tree` at `scale` on white paper.
pub fn rasterize(tree: &usvg::Tree, scale: f32) -> Result<tiny_skia::Pixmap, ExportError> {
    let (width, height) = pixels(tree, scale)?;
    let whole = tiny_skia::IntRect::from_xywh(0, 0, width, height)
        .ok_or(too_large(tree, width, height, MAX_SIDE))?;
    rasterize_part(tree, scale, whole)
}

/// Pixels a side tiny-skia takes at most.
const MAX_SIDE: f32 = (i32::MAX / 4) as f32;

/// The size of `tree` drawn at `scale`, if it is no more than
/// [`MAX_PIXELS`].
fn pixels(tree: &usvg::Tree, scale: f32) -> Result<(u32, u32), ExportError> {
    let size = tree.size();
    let (width, height) = (
        (size.width() * scale).ceil().max(1.0) as u32,
        (size.height() * scale).ceil().max(1.0) as u32,
    );
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(too_large(tree, width, height, MAX_SIDE));
    }
    Ok((width, height))
}

/// `tree` too large as `width` × `height` pixels, with the largest scale,
/// to two places, at which it fits in [`MAX_PIXELS`] and `side` pixels a
/// side.
fn too_large(tree: &usvg::Tree, width: u32, height: u32, side: f32) -> ExportError {
    let size = tree.size();
    let by_area = (MAX_PIXELS as f32 / (size.width() * size.height())).sqrt();
    let by_side = side / size.width().max(size.height());
    let fits = (by_area.min(by_side) * 100.0).floor() / 100.0;
    ExportError::TooLarge {
        width,
        height,
        fits: fits.is_finite().then_some(fits),
    }
}

/// Rasterizes the pixels `part` of `tree` drawn at `scale`, on white paper.
/// The preview uses it to show a zoomed-in piece of a large diagram sharp.
pub fn rasterize_part(
    tree: &usvg::Tree,
    scale: f32,
    part: tiny_skia::IntRect,
) -> Result<tiny_skia::Pixmap, ExportError> {
    let (width, height) = (part.width(), part.height());
    let too_large = ExportError::TooLarge {
        width,
        height,
        fits: None,
    };
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(too_large);
    }
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(too_large)?;
    pixmap.fill(tiny_skia::Color::WHITE);
    let transform = tiny_skia::Transform::from_scale(scale, scale)
        .post_translate(-part.x() as f32, -part.y() as f32);
    resvg::render(tree, transform, &mut pixmap.as_mut());
    Ok(pixmap)
}

/// Rasterizes `tree` at `scale` on a clear pixmap, so a rounded icon keeps
/// transparent corners.
pub fn rasterize_clear(tree: &usvg::Tree, scale: f32) -> Result<tiny_skia::Pixmap, ExportError> {
    let (width, height) = pixels(tree, scale)?;
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).ok_or(too_large(tree, width, height, MAX_SIDE))?;
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

fn png(tree: &usvg::Tree, scale: f32) -> Result<Vec<u8>, ExportError> {
    rasterize(tree, scale)?
        .encode_png()
        .map_err(|e| ExportError::Svg(e.to_string()))
}

fn jpeg(tree: &usvg::Tree, scale: f32) -> Result<Vec<u8>, ExportError> {
    let pixmap = rasterize(tree, scale)?;
    let side = || too_large(tree, pixmap.width(), pixmap.height(), f32::from(u16::MAX));
    let width = u16::try_from(pixmap.width()).map_err(|_| side())?;
    let height = u16::try_from(pixmap.height()).map_err(|_| side())?;
    // JPEG has no alpha. Edges are premultiplied against the white paper.
    let mut rgb = Vec::with_capacity(pixmap.width() as usize * pixmap.height() as usize * 3);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        rgb.extend_from_slice(&[color.red(), color.green(), color.blue()]);
    }
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, JPEG_QUALITY)
        .encode(&rgb, width, height, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| ExportError::Jpeg(e.to_string()))?;
    Ok(out)
}

fn markdown(source: Source<'_>) -> Result<Vec<u8>, ExportError> {
    let title = stem(source.file);
    // A Mermaid source may sit in its own fence already; wrap it once.
    let code = match source.language {
        Language::Mermaid => lang::strip_mermaid_fence(source.code),
        Language::PlantUml => source.code.strip_prefix('\u{feff}').unwrap_or(source.code),
    };
    let fence = "`".repeat(3.max(longest_run(code, '`') + 1));
    let mut body = code.to_owned();
    if !body.ends_with('\n') {
        body.push('\n');
    }
    Ok(format!(
        "# {title}\n\n{fence}{}\n{body}{fence}\n",
        source.language.id()
    )
    .into_bytes())
}

fn pdf(svg: &str, tree: &usvg::Tree, options: &Options) -> Result<Vec<u8>, ExportError> {
    let size = Size::from_wh(tree.size().width(), tree.size().height())
        .ok_or_else(|| ExportError::Pdf("the diagram has no size".into()))?;
    let mut document = Document::new();
    if let Some(source) = options.source {
        document.set_metadata(
            Metadata::new()
                .title(stem(source.file))
                .creator("Omaspec".into())
                .producer("Omaspec".into())
                .keywords(vec![source.language.id().into()]),
        );
    }
    let mut page = document.start_page_with(PageSettings::new(size));
    let mut surface = page.surface();
    surface.draw_svg(tree, size, SvgSettings::default());
    surface.finish();
    page.finish();
    if let Some(source) = options.source {
        attach(&mut document, source, svg)?;
    }
    document
        .finish()
        .map_err(|e| ExportError::Pdf(format!("{e:?}")))
}

/// The SVG (already carrying the source) and the source file, named as in the ZIP.
fn attach(document: &mut Document, source: Source<'_>, svg: &str) -> Result<(), ExportError> {
    let when = utc_now();
    let svg_name = format!("{}.svg", stem(source.file));
    let code_name = source_name(source);
    let stamped = embed::svg_with_source(svg, source.code, source.language, source.file);
    embed_file(
        document,
        Attachment {
            path: &svg_name,
            mime: "image/svg+xml",
            description: "The diagram as SVG, with its source.",
            bytes: stamped.into_bytes(),
            kind: AssociationKind::Alternative,
            compress: true,
        },
        when,
    )?;
    embed_file(
        document,
        Attachment {
            path: &code_name,
            mime: "text/plain",
            description: "The diagram source.",
            bytes: source.code.as_bytes().to_vec(),
            kind: AssociationKind::Source,
            compress: false,
        },
        when,
    )
}

struct Attachment<'a> {
    path: &'a str,
    mime: &'a str,
    description: &'a str,
    bytes: Vec<u8>,
    kind: AssociationKind,
    compress: bool,
}

fn embed_file(
    document: &mut Document,
    file: Attachment<'_>,
    when: DateTime,
) -> Result<(), ExportError> {
    let mime = MimeType::new(file.mime)
        .ok_or_else(|| ExportError::Pdf(format!("{} is not a mime type", file.mime)))?;
    document
        .embed_file(EmbeddedFile {
            path: file.path.to_owned(),
            mime_type: Some(mime),
            description: Some(file.description.to_owned()),
            association_kind: file.kind,
            data: Data::from(file.bytes),
            modification_date: Some(when),
            compress: Some(file.compress),
            location: None,
        })
        .ok_or_else(|| ExportError::Pdf(format!("{} is already attached", file.path)))
}

fn zip_bundle(svg: &str, options: &Options<'_>) -> Result<Vec<u8>, ExportError> {
    let source = options.source.ok_or(ExportError::NeedsSource)?;
    let stem = stem(source.file);
    let laid = linked(svg, options)?;
    let svg_bytes =
        embed::svg_with_source(&laid, source.code, source.language, source.file).into_bytes();
    let pdf_bytes = pdf(svg, &parse(svg)?, options)?;
    let mut cursor = Cursor::new(Vec::new());
    let mut archive = ZipWriter::new(&mut cursor);
    write_zip(&mut archive, &format!("{stem}.svg"), &svg_bytes, true)?;
    write_zip(
        &mut archive,
        &source_name(source),
        source.code.as_bytes(),
        true,
    )?;
    write_zip(&mut archive, &format!("{stem}.pdf"), &pdf_bytes, false)?;
    archive
        .finish()
        .map_err(|e| ExportError::Zip(e.to_string()))?;
    Ok(cursor.into_inner())
}

/// `files` as one ZIP, each under its path (`/` between folders),
/// compressed.
pub fn zip<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<Vec<u8>, ExportError> {
    let mut cursor = Cursor::new(Vec::new());
    let mut archive = ZipWriter::new(&mut cursor);
    for (name, bytes) in files {
        write_zip(&mut archive, name, bytes, true)?;
    }
    archive
        .finish()
        .map_err(|e| ExportError::Zip(e.to_string()))?;
    Ok(cursor.into_inner())
}

fn write_zip<W: Write + io::Seek>(
    archive: &mut ZipWriter<W>,
    name: &str,
    bytes: &[u8],
    deflate: bool,
) -> Result<(), ExportError> {
    let method = if deflate {
        CompressionMethod::Deflated
    } else {
        CompressionMethod::Stored
    };
    archive
        .start_file(
            name,
            SimpleFileOptions::default().compression_method(method),
        )
        .map_err(|e| ExportError::Zip(e.to_string()))?;
    archive
        .write_all(bytes)
        .map_err(|e| ExportError::Zip(e.to_string()))
}

/// File stem of `file`, or `diagram`.
fn stem(file: Option<&str>) -> String {
    file.and_then(|name| {
        std::path::Path::new(name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
    })
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "diagram".into())
}

/// The source's own file name, or `diagram`, with `.puml` or `.mmd` added
/// when it has no extension (an unsaved tab is `untitled`).
fn source_name(source: Source<'_>) -> String {
    let name = source
        .file
        .and_then(|name| {
            std::path::Path::new(name)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| stem(None));
    if std::path::Path::new(&name).extension().is_some() {
        name
    } else {
        format!("{name}.{}", source.language.extension())
    }
}

/// How many times `needle` occurs in a row.
fn longest_run(text: &str, needle: char) -> usize {
    let mut best = 0;
    let mut run = 0;
    for c in text.chars() {
        if c == needle {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

/// UTC calendar fields of `secs` after 1970-01-01.
fn utc_parts(secs: u64) -> (u16, u8, u8, u8, u8, u8) {
    let days = (secs / 86_400) as i64;
    let tod = (secs % 86_400) as u32;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (
        year as u16,
        month as u8,
        day as u8,
        (tod / 3600) as u8,
        ((tod % 3600) / 60) as u8,
        (tod % 60) as u8,
    )
}

fn utc_now() -> DateTime {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month, day, hour, minute, second) = utc_parts(secs);
    DateTime::new(year)
        .month(month)
        .day(day)
        .hour(hour)
        .minute(minute)
        .second(second)
        .utc_offset_hour(0)
        .utc_offset_minute(0)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="20" height="10" fill="red"/><text x="2" y="18" font-size="8">hi</text></svg>"#;

    #[test]
    fn png_with_source() {
        let source = Source {
            code: "@startuml\nA -> B\n@enduml\n",
            language: Language::PlantUml,
            file: Some("a.puml"),
        };
        let png = export(
            SVG,
            Format::Png,
            &Options {
                scale: 2.0,
                source: Some(source),
                links: &[],
                up: 0,
            },
        )
        .unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        let back = embed::source_from_png(&png).unwrap();
        assert_eq!(back.code, source.code);
        assert_eq!(back.file.as_deref(), Some("a.puml"));
        let pixmap = rasterize(&parse(SVG).unwrap(), 2.0).unwrap();
        assert_eq!((pixmap.width(), pixmap.height()), (80, 40));
        // Top-left is the red rectangle, bottom-right the white paper.
        assert_eq!(pixmap.pixel(1, 1).unwrap().red(), 255);
        assert_eq!(pixmap.pixel(1, 1).unwrap().green(), 0);
        assert_eq!(pixmap.pixel(79, 39).unwrap().green(), 255);
    }

    #[test]
    fn a_part_is_a_crop_of_the_whole() {
        let tree = parse(SVG).unwrap();
        let whole = rasterize(&tree, 3.0).unwrap();
        let part = tiny_skia::IntRect::from_xywh(50, 20, 40, 25).unwrap();
        let piece = rasterize_part(&tree, 3.0, part).unwrap();
        assert_eq!((piece.width(), piece.height()), (40, 25));
        for (x, y) in [(0, 0), (9, 9), (10, 10), (39, 24)] {
            let at = whole.pixel(50 + x, 20 + y).unwrap();
            assert_eq!(piece.pixel(x, y).unwrap(), at, "pixel {x},{y}");
        }
    }

    #[test]
    fn an_emoji_is_drawn_by_a_face_that_cannot_take_the_label_over() {
        let fonts = &SVG_OPTIONS.fontdb;
        let face = |name: &str| {
            fonts
                .faces()
                .find(|f| f.families.iter().any(|(n, _)| n == name))
                .map(|f| f.id)
        };
        let family = |id| {
            let face = fonts.face(id).expect("the face");
            face.families[0].0.clone()
        };
        // Told only where the machine has a text face and an emoji face.
        let (Some(sans), Some(_)) = (face("Liberation Sans"), face("Noto Color Emoji")) else {
            return;
        };
        for emoji in ['\u{2705}', '\u{1f680}', '\u{1f534}'] {
            let found = fallback(emoji, &[sans], fonts).expect("a face with it");
            assert_eq!(family(found), "Noto Color Emoji", "{emoji}");
        }
        // A face used already is not taken again, and a letter no face
        // has gives none.
        let emoji = face("Noto Color Emoji").unwrap();
        assert_ne!(fallback('\u{2705}', &[sans, emoji], fonts), Some(emoji));
        assert_eq!(fallback('\u{10FFFF}', &[sans], fonts), None);
    }

    #[test]
    fn the_handwriting_face_is_loaded_and_an_export_embeds_it() {
        assert!(SVG_OPTIONS.fontdb.faces().any(|face| {
            face.families
                .iter()
                .any(|(name, _)| name == crate::fonts::HAND_FONT)
        }));
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><text font-family="{}" font-size="8">Hi</text></svg>"#,
            crate::fonts::HAND_FONT
        );
        let out =
            String::from_utf8(export(&svg, Format::Svg, &Options::default()).unwrap()).unwrap();
        assert!(out.contains("@font-face"), "{out}");
        assert!(out.contains("data:font/woff2;base64,"), "{out}");
        // The face went through the sanitizer with the rest of the picture.
        assert_eq!(crate::sanitize::sanitize(&out).unwrap(), out);
        // Behind an XML declaration, the face still lands inside the root.
        let declared = format!("<?xml version=\"1.0\"?>{svg}");
        let out = String::from_utf8(export(&declared, Format::Svg, &Options::default()).unwrap())
            .unwrap();
        let root = out.find("<svg").unwrap();
        assert!(out.find("<style>").unwrap() > root, "{out}");

        // The SVG in a ZIP is the same export.
        let options = Options {
            source: Some(Source {
                code: "@startuml\n@enduml\n",
                language: Language::PlantUml,
                file: Some("hand.puml"),
            }),
            ..Options::default()
        };
        let bytes = export(&svg, Format::Zip, &options).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut inside = String::new();
        archive
            .by_name("hand.svg")
            .unwrap()
            .read_to_string(&mut inside)
            .unwrap();
        assert!(inside.contains("data:font/woff2;base64,"), "{inside}");
    }

    #[test]
    fn svg_and_pdf() {
        let svg = export(SVG, Format::Svg, &Options::default()).unwrap();
        assert_eq!(svg, SVG.as_bytes());
        let pdf = export(SVG, Format::Pdf, &Options::default()).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
    }

    fn sample() -> Source<'static> {
        Source {
            code: "@startuml\nA -> B\n@enduml\n",
            language: Language::PlantUml,
            file: Some("a.puml"),
        }
    }

    fn with_source(format: Format) -> Vec<u8> {
        export(
            SVG,
            format,
            &Options {
                scale: 2.0,
                source: Some(sample()),
                links: &[],
                up: 0,
            },
        )
        .unwrap()
    }

    #[test]
    fn jpeg_is_a_jpeg_and_stops_at_the_same_size_as_png() {
        let jpeg = export(SVG, Format::Jpeg, &Options::default()).unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8, 0xFF]));
        let huge = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20000" height="20000"/>"#;
        let err = export(huge, Format::Jpeg, &Options::default()).unwrap_err();
        assert!(matches!(err, ExportError::TooLarge { .. }));
    }

    #[test]
    fn markdown_wraps_the_source_once() {
        let text = String::from_utf8(with_source(Format::Markdown)).unwrap();
        assert_eq!(
            text,
            "# a\n\n```plantuml\n@startuml\nA -> B\n@enduml\n```\n"
        );
        assert!(matches!(
            export(SVG, Format::Markdown, &Options::default()),
            Err(ExportError::NeedsSource)
        ));
        let fenced = Source {
            code: "\u{feff}```mermaid\npie\n  \"a\": 1\n```\n",
            language: Language::Mermaid,
            file: Some("pie.md"),
        };
        let options = Options {
            scale: 1.0,
            source: Some(fenced),
            links: &[],
            up: 0,
        };
        let text = String::from_utf8(export(SVG, Format::Markdown, &options).unwrap()).unwrap();
        assert_eq!(text, "# pie\n\n```mermaid\npie\n  \"a\": 1\n```\n");
    }

    #[test]
    fn a_source_without_extension_gets_the_language_one() {
        let named = |file| {
            source_name(Source {
                code: "",
                language: Language::Mermaid,
                file,
            })
        };
        assert_eq!(named(Some("untitled")), "untitled.mmd");
        assert_eq!(named(None), "diagram.mmd");
        assert_eq!(named(Some("/tmp/notes.md")), "notes.md");
    }

    #[test]
    fn pdf_attaches_the_svg_and_the_source() {
        let pdf = with_source(Format::Pdf);
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(contains(&pdf, b"a.svg"));
        assert!(contains(&pdf, b"a.puml"));
        assert!(contains(&pdf, b"@startuml\nA -> B\n@enduml\n"));
        let bare = export(SVG, Format::Pdf, &Options::default()).unwrap();
        assert!(bare.starts_with(b"%PDF-"));
        assert!(!contains(&bare, b"@startuml"));
    }

    #[test]
    fn zip_holds_svg_source_and_pdf() {
        let bytes = with_source(Format::Zip);
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut names: Vec<_> = archive
            .file_names()
            .map(|name| name.unwrap().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["a.pdf", "a.puml", "a.svg"]);
        let mut svg = String::new();
        archive
            .by_name("a.svg")
            .unwrap()
            .read_to_string(&mut svg)
            .unwrap();
        assert_eq!(embed::source_from_svg(&svg).unwrap().code, sample().code);
        let mut code = String::new();
        archive
            .by_name("a.puml")
            .unwrap()
            .read_to_string(&mut code)
            .unwrap();
        assert_eq!(code, sample().code);
        let mut pdf = Vec::new();
        archive
            .by_name("a.pdf")
            .unwrap()
            .read_to_end(&mut pdf)
            .unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(contains(&pdf, b"@startuml\nA -> B\n@enduml\n"));
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|window| window == needle)
    }

    #[test]
    fn utc_parts_reads_a_known_day() {
        assert_eq!(utc_parts(1_792_281_600), (2026, 10, 18, 0, 0, 0));
    }

    #[test]
    fn refuses_huge_pngs() {
        let huge = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20000" height="20000"/>"#;
        let err = export(huge, Format::Png, &Options::default()).unwrap_err();
        assert!(matches!(err, ExportError::TooLarge { .. }));
        // It names the scale that fits, and that scale does.
        assert_eq!(
            err.to_string(),
            "a 40000×40000 px image is too large (at most 100 million pixels); it fits at a scale of 0.5 or less, or export SVG or PDF"
        );
        let tall = r#"<svg xmlns="http://www.w3.org/2000/svg" width="8951" height="57239"/>"#;
        let at = |scale: f32| {
            export(
                tall,
                Format::Png,
                &Options {
                    scale,
                    ..Options::default()
                },
            )
        };
        let Err(ExportError::TooLarge {
            fits: Some(fits), ..
        }) = at(0.45)
        else {
            panic!("too large at 0.45");
        };
        assert_eq!(fits, 0.44);
        // Measured, not drawn: 99 million pixels take long in a test.
        assert!(pixels(&parse(tall).unwrap(), fits).is_ok());
    }

    #[test]
    fn never_reads_local_images() {
        let dir = std::env::temp_dir().join(format!("omaspec-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let secret = dir.join("secret.png");
        std::fs::write(
            &secret,
            export(SVG, Format::Png, &Options::default()).unwrap(),
        )
        .unwrap();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><image href="{}" width="40" height="20"/></svg>"#,
            secret.display()
        );
        let pixmap = rasterize(&parse(&svg).unwrap(), 1.0).unwrap();
        // Still blank paper: the file was not loaded.
        assert_eq!(pixmap.pixel(1, 1).unwrap().green(), 255);
    }
}
