//! A flight through a project as a page: the camera zooms from the map
//! into a tile, into its diagram, on along a link into the next one, and
//! back out, in vectors and sharp at any size (`omaspec play
//! omaspec-project.toml --dive Day/Order -o flight.html`). The page holds
//! each picture on the way and, for each step, the room in one picture
//! the next grows out of; the script only moves the camera.

use omaspec_uml::Color;
use serde_json::{Value, json};

use super::{ExportError, Source};

const SCRIPT: &str = include_str!("flight.js");
/// The camera both pages of a project fly with ([`super::project`]).
pub(super) const ZOOM: &str = include_str!("zoom.js");

/// What a page of flights plays.
#[derive(Debug, Clone)]
pub struct Flight {
    /// Each picture on the way, once: the map first.
    pub pictures: Vec<Picture>,
    /// The flights, one after another: each a way in, step by step, and
    /// back out the same way.
    pub flights: Vec<Vec<Step>>,
    /// The page's ground.
    pub paper: Color,
    pub ink: Color,
}

#[derive(Debug, Clone)]
pub struct Picture {
    pub svg: String,
    /// Its size in its own units, and what the caption calls it.
    pub size: (f32, f32),
    pub name: String,
}

/// From one picture into another, which grows out of `room` in the
/// first, in its own units from its top left corner.
#[derive(Debug, Clone, Copy)]
pub struct Step {
    pub from: usize,
    pub to: usize,
    pub room: [f32; 4],
}

/// The page that plays `flight`, called `title`, with `source`, where it
/// has one, in its first picture. The pictures are sanitized, each with
/// ids and no style sheet of its own, the faces once; only the source is
/// written after.
pub fn page(flight: &Flight, source: Option<Source>, title: &str) -> Result<String, ExportError> {
    let pictures = pictures_of(&flight.pictures, source)?;
    let mut out = String::with_capacity(pictures.len() + SCRIPT.len() + 4096);
    out.push_str(&head(title));
    out.push_str(&format!(
        "<style>\nhtml, body {{ margin: 0; height: 100%; background: {}; color: {}; }}\n\
         #flight {{ position: relative; height: 100%; overflow: hidden; cursor: pointer; font: 14px/20px system-ui, sans-serif; }}\n\
         #flight .picture {{ position: absolute; visibility: hidden; transform-origin: 0 0; }}\n\
         #flight .picture > svg {{ display: block; width: 100%; height: 100%; }}\n\
         #trail {{ position: absolute; z-index: 3; left: 14px; bottom: 10px; opacity: 0.8; }}\n</style>\n",
        flight.paper, flight.ink
    ));
    out.push_str("</head>\n<body>\n<main id=\"flight\">\n");
    out.push_str(&pictures);
    out.push_str("<div id=\"trail\"></div>\n</main>\n");
    out.push_str(&format!(
        "<script type=\"application/json\" id=\"play\">{}</script>\n",
        data(flight).to_string().replace('<', "\\u003c")
    ));
    out.push_str(&format!(
        "<script>\n{ZOOM}{SCRIPT}</script>\n</body>\n</html>\n"
    ));
    Ok(out)
}

/// The start of a page called `title` that fetches nothing, up to its
/// style.
pub(super) fn head(title: &str) -> String {
    let mut out =
        String::from("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    // Nothing from anywhere: what the page shows is in it.
    out.push_str(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:\">\n",
    );
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<meta name=\"generator\" content=\"Omaspec\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape(title)));
    out
}

/// `pictures`, each in a `div` of its own with its index: sanitized, each
/// with ids and no style sheet of its own, the faces once; `source`,
/// where there is one, in the first, written after.
pub(super) fn pictures_of(
    pictures: &[Picture],
    source: Option<Source>,
) -> Result<String, ExportError> {
    let plain: Vec<String> = pictures
        .iter()
        .map(|p| crate::atlas::without_styles(&p.svg))
        .collect();
    let mut pictures = String::new();
    for (at, svg) in plain.iter().enumerate() {
        let svg = if at == 0 {
            let others: Vec<&str> = plain[1..].iter().map(String::as_str).collect();
            crate::fonts::embed_with(svg, &others)
        } else {
            svg.clone()
        };
        let svg = crate::atlas::own_ids(&svg, &format!("f{at}-"));
        let svg = crate::sanitize::sanitize(&svg).map_err(|e| ExportError::Svg(e.to_string()))?;
        let svg = match (at, source) {
            (0, Some(s)) => omaspec_core::embed::svg_with_source(&svg, s.code, s.language, s.file),
            _ => svg,
        };
        pictures.push_str(&format!(
            "<div class=\"picture\" data-picture=\"{at}\">{svg}</div>\n"
        ));
    }
    Ok(pictures)
}

fn data(flight: &Flight) -> Value {
    json!({
        "kind": "flight",
        "pictures": flight.pictures.iter().map(|p| json!({
            "size": [round(p.size.0), round(p.size.1)],
            "name": p.name,
        })).collect::<Vec<_>>(),
        "flights": flight.flights.iter().map(|steps| steps.iter().map(|s| json!({
            "from": s.from,
            "to": s.to,
            "room": s.room.iter().map(|&v| round(v)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>()).collect::<Vec<_>>(),
    })
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

/// `text` for HTML: `&`, `<`, `>` and `"` written as entities.
pub(super) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_holds_each_picture_once_and_the_rooms_between_them() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><g id="lines"/><script>alert(1)</script></svg>"##;
        let picture = |name: &str| Picture {
            svg: svg.into(),
            size: (40.0, 20.0),
            name: name.into(),
        };
        let flight = Flight {
            pictures: vec![picture("cafe"), picture("Day <1>")],
            flights: vec![vec![Step {
                from: 0,
                to: 1,
                room: [1.0, 2.0, 10.0, 5.0],
            }]],
            paper: Color::WHITE,
            ink: Color::BLACK,
        };
        let page = page(&flight, None, "cafe").unwrap();
        assert!(!page.contains("alert(1)"));
        assert!(page.contains(r#"id="f0-lines""#) && page.contains(r#"id="f1-lines""#));
        assert!(page.contains(r#""kind":"flight""#));
        assert!(page.contains(r#""room":[1.0,2.0,10.0,5.0]"#), "{page}");
        assert!(page.contains("Day \\u003c1>"), "{page}");
    }
}
