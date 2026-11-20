//! From sketch to ink as a page: the diagram in the sketch look drawn
//! stroke by stroke as with a pen, then inked into the clean look, in
//! vectors and sharp at any size (`omaspec play FILE --ink -o ink.html`).
//! The page holds both pictures and when the pen reaches each element of
//! the sketch; the script measures each line itself and only shows the
//! moment it is at.

use omaspec_uml::Color;
use serde_json::{Value, json};

use super::{ExportError, Source};

const SCRIPT: &str = include_str!("ink.js");

/// What a page of the ink plays. Times are in seconds from its start.
#[derive(Debug, Clone)]
pub struct Ink {
    /// The sketch and the clean picture, of one layout.
    pub sketch: String,
    pub clean: String,
    /// By drawn element of the sketch, in the order of the picture: what
    /// the pen does with it ([`DRAWN`] says which elements those are).
    pub strokes: Vec<Stroke>,
    /// When the pen is done; how long the sketch stays, the ink takes and
    /// the clean picture stays; how long a fill or a word takes to come.
    pub end: f32,
    pub drawn: f32,
    pub ink: f32,
    pub inked: f32,
    pub fade: f32,
    /// The clean picture's paper, the page's ground.
    pub paper: Color,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stroke {
    /// A line the pen draws from `start` to `end`.
    Drawn { start: f32, end: f32 },
    /// A fill, a word or a dashed line, which comes at `at`.
    Comes { at: f32 },
}

/// The elements the pen draws, in the order of the picture, as the
/// script finds them: those not in a definition, the paper before the
/// first group left out.
pub const DRAWN: &str = "path, rect, circle, ellipse, line, polyline, polygon, text";

/// The page that plays `ink`, called `title`, with the diagram's `source`
/// in its clean picture. The pictures are sanitized, each with ids of its
/// own and the faces once; only the source is written after.
pub fn page(ink: &Ink, source: Option<Source>, title: &str) -> Result<String, ExportError> {
    let sketch = crate::atlas::without_styles(&ink.sketch);
    let clean = crate::atlas::without_styles(&ink.clean);
    let sketch = crate::fonts::embed_with(&sketch, &[clean.as_str()]);
    let clean_svg = |svg: &str, prefix: &str| -> Result<String, ExportError> {
        let svg = crate::atlas::own_ids(svg, prefix);
        crate::sanitize::sanitize(&svg).map_err(|e| ExportError::Svg(e.to_string()))
    };
    let sketch = clean_svg(&sketch, "s-")?;
    let clean = clean_svg(&clean, "c-")?;
    let clean = match source {
        Some(s) => omaspec_core::embed::svg_with_source(&clean, s.code, s.language, s.file),
        None => clean,
    };
    let mut out = String::with_capacity(sketch.len() + clean.len() + SCRIPT.len() + 4096);
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    // Nothing from anywhere: what the page shows is in it.
    out.push_str(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:\">\n",
    );
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<meta name=\"generator\" content=\"Omaspec\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape(title)));
    out.push_str(&format!(
        "<style>\nhtml, body {{ margin: 0; height: 100%; background: {}; }}\n\
         #ink {{ position: relative; height: 100%; cursor: pointer; }}\n\
         #ink > div {{ position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; padding: 14px; box-sizing: border-box; }}\n\
         #ink svg {{ width: 100%; height: 100%; }}\n</style>\n",
        ink.paper
    ));
    out.push_str("</head>\n<body>\n<main id=\"ink\">\n<div id=\"sketch\">");
    out.push_str(&sketch);
    out.push_str("</div>\n<div id=\"clean\" style=\"opacity:0\">");
    out.push_str(&clean);
    out.push_str("</div>\n</main>\n");
    out.push_str(&format!(
        "<script type=\"application/json\" id=\"play\">{}</script>\n",
        data(ink).to_string().replace('<', "\\u003c")
    ));
    out.push_str(&format!("<script>\n{SCRIPT}</script>\n</body>\n</html>\n"));
    Ok(out)
}

fn data(ink: &Ink) -> Value {
    json!({
        "kind": "ink",
        "drawn": DRAWN,
        "end": round(ink.end),
        "hold": round(ink.drawn),
        "ink": round(ink.ink),
        "inked": round(ink.inked),
        "fade": round(ink.fade),
        "strokes": ink.strokes.iter().map(|s| match s {
            Stroke::Drawn { start, end } => json!([round(*start), round(*end)]),
            Stroke::Comes { at } => json!(round(*at)),
        }).collect::<Vec<_>>(),
    })
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

/// `text` for HTML: `&`, `<`, `>` and `"` written as entities.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_holds_the_sketch_the_clean_picture_and_the_pen() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><g id="lines"><path d="M0,0L10,0" stroke="#000"/></g><script>alert(1)</script></svg>"##;
        let ink = Ink {
            sketch: svg.into(),
            clean: svg.into(),
            strokes: vec![Stroke::Drawn {
                start: 0.0,
                end: 1.0,
            }],
            end: 1.3,
            drawn: 0.6,
            ink: 1.4,
            inked: 1.8,
            fade: 0.3,
            paper: Color::WHITE,
        };
        let page = page(&ink, None, "door").unwrap();
        assert!(!page.contains("alert(1)"));
        assert!(page.contains(r#"id="s-lines""#) && page.contains(r#"id="c-lines""#));
        assert!(page.contains(r#""kind":"ink""#));
        assert!(page.contains(r#""strokes":[[0.0,1.0]]"#), "{page}");
    }
}
