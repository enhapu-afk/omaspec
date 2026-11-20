//! A project as one page (`omaspec export DIR --html`): its map, and a
//! click on a tile, or on a state that opens a diagram of its own, zooms
//! into what it opens, as the editor dives into it, and back out; every
//! diagram the map leads to is in it, in vectors, sharp at any size. The
//! page holds each picture once and, for each, where a click in it goes
//! and the room the next picture grows out of; the script only moves the
//! camera ([`super::flight`]'s, in `zoom.js`).

use omaspec_uml::Color;
use serde_json::{Value, json};

use super::ExportError;
use super::flight::{Picture, ZOOM, head, pictures_of};

const SCRIPT: &str = include_str!("project.js");

/// What the page of a project shows.
#[derive(Debug, Clone)]
pub struct Site {
    /// Each picture once, the project's map first; its name is what the
    /// trail calls it there.
    pub pictures: Vec<Picture>,
    /// For each picture, where a click in it goes.
    pub hops: Vec<Vec<Hop>>,
    /// The page's ground, its words, and the light around a hop.
    pub paper: Color,
    pub ink: Color,
    pub accent: Color,
    /// The same three under a viewer's dark scheme, where the pictures
    /// turn dark there too (`--look auto`).
    pub dark: Option<(Color, Color, Color)>,
}

/// Where a click in a picture goes: the box it lies in (a tile, a state)
/// and the room in it the picture `to` grows out of, both in the
/// picture's own units from its top left corner; `name` is what the box
/// says, which the trail and the page's address call it.
#[derive(Debug, Clone)]
pub struct Hop {
    pub rect: [f32; 4],
    pub room: [f32; 4],
    pub to: usize,
    pub name: String,
}

/// The page that shows `site`, called `title`.
pub fn page(site: &Site, title: &str) -> Result<String, ExportError> {
    let pictures = pictures_of(&site.pictures, None)?;
    let Color(r, g, b) = site.accent;
    let mut out = String::with_capacity(pictures.len() + SCRIPT.len() + 4096);
    out.push_str(&head(title));
    out.push_str(&format!(
        "<style>\nhtml, body {{ margin: 0; height: 100%; background: {paper}; color: {ink}; }}\n\
         #site {{ position: relative; height: 100%; overflow: hidden; font: 14px/20px system-ui, sans-serif; }}\n\
         #site.over {{ cursor: pointer; }}\n\
         #site .picture {{ position: absolute; visibility: hidden; }}\n\
         #site .picture > svg {{ display: block; width: 100%; height: 100%; }}\n\
         #lit {{ position: absolute; z-index: 4; display: none; pointer-events: none; border: 2px solid {accent}; border-radius: 8px; background: rgba({r}, {g}, {b}, 0.08); }}\n\
         #trail {{ position: absolute; z-index: 5; left: 14px; bottom: 10px; display: flex; gap: 0.4rem; align-items: baseline; }}\n\
         #trail button {{ font: inherit; color: inherit; background: none; border: 0; padding: 0; cursor: pointer; opacity: 0.7; }}\n\
         #trail button:hover {{ opacity: 1; text-decoration: underline; }}\n\
         #trail .here {{ font-weight: 600; }}\n\
         #trail .between {{ opacity: 0.5; }}\n\
         #help {{ position: absolute; z-index: 5; right: 14px; bottom: 10px; opacity: 0.55; }}\n\
         @media (max-width: 700px) {{ #help {{ display: none; }} }}\n{dark}{paints}</style>\n",
        paper = site.paper,
        ink = site.ink,
        accent = site.accent,
        dark = site.dark.map_or_else(String::new, |(paper, ink, accent)| {
            let Color(r, g, b) = accent;
            format!(
                "@media (prefers-color-scheme: dark) {{ html, body {{ background: {paper}; color: {ink}; }} \
                 #lit {{ border-color: {accent}; background: rgba({r}, {g}, {b}, 0.1); }} }}\n"
            )
        }),
        paints = dark_paints(&site.pictures),
    ));
    out.push_str("</head>\n<body>\n<main id=\"site\">\n");
    out.push_str(&pictures);
    out.push_str("<div id=\"lit\"></div>\n<nav id=\"trail\"></nav>\n");
    out.push_str(
        "<div id=\"help\">a click on a tile or a linked state goes in · Esc, ← or a name goes out · F full screen</div>\n</main>\n",
    );
    out.push_str(&format!(
        "<script type=\"application/json\" id=\"site-data\">{}</script>\n",
        data(site).to_string().replace('<', "\\u003c")
    ));
    out.push_str(&format!(
        "<script>\n{ZOOM}{SCRIPT}</script>\n</body>\n</html>\n"
    ));
    Ok(out)
}

/// The dark colors of the pictures drawn in both schemes (`--look auto`),
/// under the viewer's dark scheme. A picture's own style sheet is not
/// kept on the page, where its rules would reach every other picture: its
/// dark paints are, each rule kept to its picture.
fn dark_paints(pictures: &[Picture]) -> String {
    let mut rules = String::new();
    for (at, picture) in pictures.iter().enumerate() {
        for rule in crate::schemes::dark_paints(&picture.svg) {
            // Its ids are the picture's own on the page, and nothing in it
            // closes the page's sheet.
            let rule = rule
                .replace("url(#", &format!("url(#f{at}-"))
                .replace('<', "\\3c ");
            rules.push_str(&format!("[data-picture=\"{at}\"] {rule}\n"));
        }
    }
    if rules.is_empty() {
        return rules;
    }
    format!("@media (prefers-color-scheme: dark) {{\n{rules}}}\n")
}

fn data(site: &Site) -> Value {
    let rect = |r: &[f32; 4]| r.iter().map(|&v| round(v)).collect::<Vec<_>>();
    json!({
        "kind": "project",
        "pictures": site.pictures.iter().map(|p| json!({
            "size": [round(p.size.0), round(p.size.1)],
            "name": p.name,
        })).collect::<Vec<_>>(),
        "hops": site.hops.iter().map(|hops| hops.iter().map(|hop| json!({
            "rect": rect(&hop.rect),
            "room": rect(&hop.room),
            "to": hop.to,
            "name": hop.name,
        })).collect::<Vec<_>>()).collect::<Vec<_>>(),
    })
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_holds_each_picture_once_and_where_a_click_goes() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><g id="lines"/><script>alert(1)</script></svg>"##;
        let picture = |name: &str| Picture {
            svg: svg.into(),
            size: (40.0, 20.0),
            name: name.into(),
        };
        let site = Site {
            pictures: vec![picture("cafe"), picture("Day")],
            hops: vec![
                vec![Hop {
                    rect: [1.0, 2.0, 10.0, 5.0],
                    room: [2.0, 3.0, 8.0, 3.0],
                    to: 1,
                    name: "Day <1>".into(),
                }],
                Vec::new(),
            ],
            paper: Color::WHITE,
            ink: Color::BLACK,
            accent: Color(47, 111, 219),
            dark: Some((Color(22, 24, 29), Color::WHITE, Color(121, 166, 240))),
        };
        let page = page(&site, "cafe").unwrap();
        assert!(!page.contains("alert(1)"));
        assert!(page.contains(r#"id="f0-lines""#) && page.contains(r#"id="f1-lines""#));
        assert!(page.contains(r#""kind":"project""#));
        assert!(page.contains(r#""rect":[1.0,2.0,10.0,5.0]"#), "{page}");
        assert!(page.contains("Day \\u003c1>"), "{page}");
        assert!(page.contains("const Zoom") && page.contains("rgba(47, 111, 219, 0.08)"));
        assert!(page.contains("default-src 'none'"));
        assert!(page.contains("prefers-color-scheme: dark"), "{page}");
    }

    /// In the look auto a picture's dark colors stay on the page, each
    /// rule kept to its picture, though its own sheet does not.
    #[test]
    fn the_pictures_turn_dark_each_by_its_own_rules() {
        let light = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><rect width="40" height="20" fill="#ffffff"/><rect x="5" y="5" width="20" height="10" fill="#ffffff" stroke="#2f6fdb"/></svg>"##;
        let dark = light
            .replacen("#ffffff", "#16181d", 1)
            .replacen("#ffffff", "#23272f", 1)
            .replace("#2f6fdb", "#79a6f0");
        let both = crate::schemes::both(light, &dark).unwrap();
        let site = Site {
            pictures: vec![
                Picture {
                    svg: light.into(),
                    size: (40.0, 20.0),
                    name: "cafe".into(),
                },
                Picture {
                    svg: both,
                    size: (40.0, 20.0),
                    name: "Day".into(),
                },
            ],
            hops: vec![Vec::new(), Vec::new()],
            paper: Color::WHITE,
            ink: Color::BLACK,
            accent: Color(47, 111, 219),
            dark: None,
        };
        let page = page(&site, "cafe").unwrap();
        assert!(
            page.contains(r##"[data-picture="1"] [stroke="#2f6fdb"] { stroke: #79a6f0; }"##),
            "{page}"
        );
        assert!(
            page.contains(r#"[data-picture="1"] .omaspec-dark-0 {"#),
            "{page}"
        );
        assert!(!page.contains(r#"[data-picture="0"]"#), "{page}");
    }
}
