//! A theme sweep as a page: one diagram through many Omarchy themes, each
//! theme's colors sweeping over the one before from left to right, in
//! vectors and sharp at any size (`omaspec play FILE --themes -o
//! sweep.html`). The page holds the picture in each theme, laid on one
//! another; the script only moves the edge of the one sweeping in.

use omaspec_uml::Color;
use serde_json::json;

use super::{ExportError, Source};

const SCRIPT: &str = include_str!("sweep.js");

/// What a page of a sweep plays.
#[derive(Debug, Clone)]
pub struct Sweep {
    pub themes: Vec<Theme>,
    /// How long each theme stays, and how long a sweep takes, in seconds;
    /// how soft its edge is, of the width.
    pub stay: f32,
    pub sweep: f32,
    pub edge: f32,
}

/// The diagram in one theme.
#[derive(Debug, Clone)]
pub struct Theme {
    /// As its caption says it.
    pub name: String,
    pub svg: String,
    /// Its paper, words and quiet words, which the caption takes.
    pub paper: Color,
    pub ink: Color,
    pub muted: Color,
}

/// The page that plays `sweep`, called `title`, with the diagram's
/// `source` in its first picture. The pictures are sanitized, each with
/// ids of its own; the first carries the faces the words of all of them
/// are set in; only the source is written after.
pub fn page(sweep: &Sweep, source: Option<Source>, title: &str) -> Result<String, ExportError> {
    let mut pictures = String::new();
    for (at, theme) in sweep.themes.iter().enumerate() {
        let svg = if at == 0 {
            let others: Vec<&str> = sweep.themes[1..].iter().map(|t| t.svg.as_str()).collect();
            crate::fonts::embed_with(&theme.svg, &others)
        } else {
            theme.svg.clone()
        };
        let svg = crate::atlas::own_ids(&svg, &format!("t{at}-"));
        let svg = crate::sanitize::sanitize(&svg).map_err(|e| ExportError::Svg(e.to_string()))?;
        let svg = match (at, source) {
            (0, Some(s)) => omaspec_core::embed::svg_with_source(&svg, s.code, s.language, s.file),
            _ => svg,
        };
        pictures.push_str(&format!(
            "<section class=\"theme\" data-theme=\"{at}\" style=\"background:{};color:{}\"><div class=\"picture\">{svg}</div><div class=\"caption\"><b>{}</b><span style=\"color:{}\">{} / {}</span></div></section>\n",
            theme.paper,
            theme.ink,
            escape(&theme.name),
            theme.muted,
            at + 1,
            sweep.themes.len()
        ));
    }
    let data = json!({
        "kind": "sweep",
        "count": sweep.themes.len(),
        "stay": round(sweep.stay),
        "sweep": round(sweep.sweep),
        "edge": round(sweep.edge),
    });
    let mut out = String::with_capacity(pictures.len() + SCRIPT.len() + 4096);
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    // Nothing from anywhere: what the page shows is in it.
    out.push_str(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:\">\n",
    );
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<meta name=\"generator\" content=\"Omaspec\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape(title)));
    out.push_str(
        "<style>\nhtml, body { margin: 0; height: 100%; }\n\
         #sweep { position: relative; height: 100%; overflow: hidden; cursor: pointer; font: 15px/20px system-ui, sans-serif; }\n\
         .theme { position: absolute; inset: 0; display: flex; flex-direction: column; }\n\
         .picture { flex: 1; min-height: 0; display: flex; align-items: center; justify-content: center; padding: 14px; }\n\
         .picture > svg { width: 100%; height: 100%; }\n\
         .caption { display: flex; justify-content: space-between; padding: 0 18px 14px; }\n</style>\n",
    );
    out.push_str("</head>\n<body>\n<main id=\"sweep\">\n");
    out.push_str(&pictures);
    out.push_str("</main>\n");
    out.push_str(&format!(
        "<script type=\"application/json\" id=\"play\">{}</script>\n",
        data.to_string().replace('<', "\\u003c")
    ));
    out.push_str(&format!("<script>\n{SCRIPT}</script>\n</body>\n</html>\n"));
    Ok(out)
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
    fn a_page_lays_the_themes_on_one_another_with_ids_of_their_own() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><g id="lines"/><script>alert(1)</script></svg>"##;
        let theme = |name: &str| Theme {
            name: name.into(),
            svg: svg.into(),
            paper: Color::WHITE,
            ink: Color::BLACK,
            muted: Color::BLACK,
        };
        let sweep = Sweep {
            themes: vec![theme("Tokyo <Night>"), theme("Gruvbox")],
            stay: 1.0,
            sweep: 0.9,
            edge: 0.1,
        };
        let page = page(&sweep, None, "door").unwrap();
        assert!(!page.contains("alert(1)"));
        assert!(page.contains("Tokyo &lt;Night&gt;"));
        assert!(page.contains(r#"id="t0-lines""#) && page.contains(r#"id="t1-lines""#));
        assert!(page.contains(r#""kind":"sweep""#));
        assert_eq!(page.matches("<section class=\"theme\"").count(), 2);
    }
}
