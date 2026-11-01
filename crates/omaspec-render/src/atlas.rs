//! A project's map (`docs/adr/0018-diagrams-that-link.md`): a mind map or
//! a WBS whose boxes name diagrams of the project, drawn with each of
//! those diagrams in a tile. The main diagram also shows the diagrams on
//! no box, and dashed arrows show where one diagram links to another.
//!
//! The pictures come from Omaspec's own engines, never from Java: a map
//! with twenty tiles draws twenty diagrams. They are kept between
//! drawings, so typing in the map draws only the map again. Each picture
//! is nested into the map's SVG as vectors, sharp at any zoom, and the
//! whole passes [`crate::sanitize`].

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};

use omaspec_core::Language;
use omaspec_core::project::key;
use omaspec_uml::map::{Fit, Map, Shape};
use omaspec_uml::tree::{self, Tiles};

use crate::{Engine, Place, RenderError, Rendered, include, mermaid, sanitize};

/// The room for a picture in a tile, in the map's units.
pub const PICTURE: (f32, f32) = (240.0, 150.0);
/// The words above the diagrams on no box.
pub const LOOSE_HEADING: &str = "Not on the map";
/// Pictures kept between drawings, at most.
const KEPT: usize = 256;

/// The diagrams of a project, as a map shows them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Atlas {
    pub diagrams: Vec<Entry>,
    /// The project's main diagram, by its path: its map also shows the
    /// diagrams that are on no box.
    pub main: Option<String>,
}

/// A diagram of the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its path in the project, `/` between folders.
    pub file: String,
    /// The names it goes by ([`omaspec_core::project::names`]).
    pub names: Vec<String>,
    /// As it is drawn, in the map's colors.
    pub source: String,
    pub language: Language,
    /// Where its `!include`s are read.
    pub place: Option<Place>,
}

/// A box of a drawn diagram that names diagrams of the project: a tile
/// of a map, or a state.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// Where it is in the picture.
    pub rect: omaspec_uml::graph::Rect,
    /// What the box says.
    pub name: String,
    /// The diagrams it names, by their paths in the project: the one it
    /// opens, or those it cannot choose between.
    pub files: Vec<String>,
    /// A tile of a map; else a state.
    pub tile: bool,
}

impl Atlas {
    /// The diagrams a name calls, without the one in `own`.
    pub fn called(&self, name: &str, own: Option<&str>) -> Vec<&Entry> {
        let wanted = key(name);
        if wanted.is_empty() {
            return Vec::new();
        }
        self.diagrams
            .iter()
            .filter(|e| Some(e.file.as_str()) != own)
            .filter(|e| e.names.iter().any(|n| key(n) == wanted))
            .collect()
    }
}

/// Whether `source` is a mind map or a WBS: a map when its boxes name
/// diagrams.
pub fn is_map(source: &str, language: Language) -> bool {
    matches!(
        omaspec_core::classify(source, language).name.as_ref(),
        "Mind map" | "WBS"
    )
}

/// A diagram drawn for a tile, kept by its source.
#[derive(Clone)]
pub(crate) struct Picture {
    result: Result<(String, Map), String>,
    /// The names its states and boxes give: where it links.
    names: Vec<String>,
}

/// Pictures for tiles, kept between drawings.
#[derive(Default)]
pub(crate) struct Pictures(HashMap<u64, Picture>);

impl Pictures {
    fn of(&mut self, entry: &Entry) -> Picture {
        let mut hasher = std::hash::DefaultHasher::new();
        entry.source.hash(&mut hasher);
        entry.language.extension().hash(&mut hasher);
        entry
            .place
            .as_ref()
            .map(|p| (&p.dir, &p.root))
            .hash(&mut hasher);
        let id = hasher.finish();
        if let Some(found) = self.0.get(&id) {
            return found.clone();
        }
        if self.0.len() >= KEPT {
            self.0.clear();
        }
        let picture = draw_native(entry);
        self.0.insert(id, picture.clone());
        picture
    }
}

/// `entry` drawn by Omaspec's own engines, never Java, and sanitized.
fn draw_native(entry: &Entry) -> Picture {
    let drawn = match entry.language {
        Language::Mermaid => mermaid::render_mapped(&entry.source)
            .map(|(svg, _, map)| (svg, map))
            .map_err(|e| e.to_string()),
        Language::PlantUml => {
            let folder = entry.place.as_ref().map(include::Folder::new);
            let includes = folder.as_ref().map(|f| f as &dyn omaspec_uml::Includes);
            omaspec_uml::render_mapped(&entry.source, includes).map_err(|d| {
                if omaspec_core::kind::plantuml_block(&entry.source) == "uml" {
                    format!("only the Java PlantUML draws it: {}", d.reason)
                } else {
                    d.reason
                }
            })
        }
    };
    let result = drawn.and_then(|(svg, map)| {
        sanitize::sanitize(&svg)
            .map(|svg| (svg, map))
            .map_err(|e| e.to_string())
    });
    let mut names: Vec<String> = match &result {
        Ok((_, map)) => map
            .states
            .iter()
            .filter(|s| s.shape == Shape::State && !s.name.is_empty())
            .map(|s| s.name.clone())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.extend(box_names(&entry.source, entry.language));
    Picture { result, names }
}

/// The words of every box of a mind map or a WBS.
fn box_names(source: &str, language: Language) -> Vec<String> {
    fn ideas(list: &[tree::Idea], out: &mut Vec<String>) {
        for idea in list {
            out.push(idea.node.text.value.clone());
            ideas(&idea.children, out);
        }
    }
    fn elements(e: &tree::Element, out: &mut Vec<String>) {
        out.push(e.node.text.value.clone());
        for child in e.left.iter().chain(&e.right) {
            elements(child, out);
        }
    }
    let mut out = Vec::new();
    let mind = |m: &tree::MindMap, out: &mut Vec<String>| {
        for map in &m.maps {
            out.push(map.root.text.value.clone());
            ideas(&map.right, out);
            ideas(&map.left, out);
        }
    };
    if !is_map(source, language) {
        return out;
    }
    match language {
        Language::Mermaid => {
            if let Some(m) = tree::from_mermaid(omaspec_core::lang::strip_mermaid_fence(source)) {
                mind(&m, &mut out);
            }
        }
        Language::PlantUml => match omaspec_uml::parse(source) {
            Ok(omaspec_uml::Diagram::MindMap(m)) => mind(&m, &mut out),
            Ok(omaspec_uml::Diagram::Wbs(w)) => elements(&w.root, &mut out),
            _ => {}
        },
    }
    out
}

/// A map drawn with a given set of tiles: its SVG, engine and map.
pub(crate) type Drawing = Result<(String, Engine, Map), RenderError>;

/// What a tile shows.
enum Shown<'a> {
    Picture(&'a str),
    Words(String),
}

/// The states of a drawn diagram that name diagrams of the project, the
/// diagram in `own` aside.
pub(crate) fn state_links(map: &Map, atlas: &Atlas, own: Option<&str>) -> Vec<Link> {
    map.states
        .iter()
        .filter(|s| s.shape == Shape::State && !s.name.is_empty())
        .filter_map(|state| {
            let files: Vec<String> = atlas
                .called(&state.name, own)
                .into_iter()
                .map(|e| e.file.clone())
                .collect();
            (!files.is_empty()).then(|| Link {
                rect: state.rect,
                name: state.name.clone(),
                files,
                tile: false,
            })
        })
        .collect()
}

/// Draws the map in `own` with the tiles of `atlas`; `draw` draws it with
/// a given set of tiles.
pub(crate) fn render(
    own: Option<&str>,
    atlas: &Atlas,
    pictures: &mut Pictures,
    draw: &mut dyn FnMut(&Tiles) -> Drawing,
) -> Result<Rendered, RenderError> {
    let start = std::time::Instant::now();
    // Key → the diagrams that go by it.
    let mut called: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, entry) in atlas.diagrams.iter().enumerate() {
        if Some(entry.file.as_str()) == own {
            continue;
        }
        let keys: HashSet<String> = entry.names.iter().map(|n| key(n)).collect();
        for k in keys {
            called.entry(k).or_default().push(index);
        }
    }
    let pick = |text: &str| {
        let k = key(text);
        called.contains_key(&k).then_some(k)
    };

    // First the boxes that name diagrams.
    let first = draw(&Tiles {
        pick: &pick,
        picture: PICTURE,
        loose: &[],
        heading: LOOSE_HEADING,
        links: &[],
    })?;
    let on_map: Vec<String> = unique(first.2.tiles.iter().map(|t| t.key.clone()));
    let on_map_set: HashSet<&str> = on_map.iter().map(String::as_str).collect();
    let named: HashSet<usize> = on_map
        .iter()
        .filter_map(|k| called.get(k))
        .flatten()
        .copied()
        .collect();

    // Then the diagrams on no box, by their first name.
    let mut loose: Vec<(String, String)> = Vec::new();
    let mut loose_entries: Vec<(String, usize)> = Vec::new();
    if own.is_some() && atlas.main.as_deref() == own {
        let mut taken: HashSet<String> = on_map.iter().cloned().collect();
        for (index, entry) in atlas.diagrams.iter().enumerate() {
            if Some(entry.file.as_str()) == own || named.contains(&index) {
                continue;
            }
            let Some(name) = entry.names.first() else {
                continue;
            };
            let mut k = key(name);
            // Two diagrams of one first name still get a tile each.
            while !taken.insert(k.clone()) {
                k.push('~');
            }
            loose.push((name.clone(), k.clone()));
            loose_entries.push((k, index));
        }
    }

    // The pictures, and where each one links.
    let mut tile_of: HashMap<String, usize> = HashMap::new();
    for k in &on_map {
        if let Some(found) = called.get(k)
            && found.len() == 1
        {
            tile_of.insert(k.clone(), found[0]);
        }
    }
    tile_of.extend(loose_entries.iter().cloned());
    let drawn: HashMap<usize, Picture> = tile_of
        .values()
        .map(|&i| (i, pictures.of(&atlas.diagrams[i])))
        .collect();
    let parents: HashMap<&str, Option<&str>> = first
        .2
        .tiles
        .iter()
        .map(|t| (t.key.as_str(), t.parent.as_deref()))
        .collect();
    let tile_keys: HashSet<&str> = on_map_set
        .iter()
        .copied()
        .chain(loose_entries.iter().map(|(k, _)| k.as_str()))
        .collect();
    let mut links: Vec<(String, String)> = Vec::new();
    let mut by_entry: Vec<(&String, &usize)> = tile_of.iter().collect();
    by_entry.sort();
    for (from, index) in by_entry {
        let Some(picture) = drawn.get(index) else {
            continue;
        };
        for name in &picture.names {
            let to = key(name);
            // A diagram's own name, or a link the map's own line shows,
            // either way.
            if to == *from
                || !tile_keys.contains(to.as_str())
                || parents.get(to.as_str()) == Some(&Some(from.as_str()))
                || parents.get(from.as_str()) == Some(&Some(to.as_str()))
                || atlas.diagrams[*index].names.iter().any(|n| key(n) == to)
            {
                continue;
            }
            let link = (from.clone(), to);
            if !links.contains(&link) {
                links.push(link);
            }
        }
    }

    let (svg, engine, map) = draw(&Tiles {
        pick: &pick,
        picture: PICTURE,
        loose: &loose,
        heading: LOOSE_HEADING,
        links: &links,
    })?;

    // Each tile's picture, or why there is none.
    let shown: Vec<(usize, Shown)> = map
        .tiles
        .iter()
        .enumerate()
        .map(|(at, tile)| {
            let found = tile_of.get(&tile.key).map(|i| &drawn[i]);
            let shown = match (found, called.get(&tile.key)) {
                (Some(picture), _) => match &picture.result {
                    Ok((svg, _)) => Shown::Picture(svg),
                    Err(why) => Shown::Words(format!("not drawn: {}", first_line(why))),
                },
                (None, Some(many)) if many.len() > 1 => Shown::Words(format!(
                    "{} diagrams go by this name: {}",
                    many.len(),
                    many.iter()
                        .map(|&i| atlas.diagrams[i].file.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
                _ => Shown::Words("no diagram".into()),
            };
            (at, shown)
        })
        .collect();
    let svg = nest(&svg, &map, &shown);
    let svg = sanitize::sanitize(&svg).map_err(|e| RenderError::Engine {
        engine,
        message: format!("invalid SVG: {e}"),
    })?;
    let links = map
        .tiles
        .iter()
        .map(|tile| {
            let files = match tile_of.get(&tile.key) {
                Some(&i) => vec![atlas.diagrams[i].file.clone()],
                None => called
                    .get(&tile.key)
                    .map(|many| {
                        many.iter()
                            .map(|&i| atlas.diagrams[i].file.clone())
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            Link {
                rect: tile.frame,
                name: tile.name.clone(),
                files,
                tile: true,
            }
        })
        .collect();
    Ok(Rendered {
        svg,
        engine,
        elapsed: start.elapsed(),
        map,
        links,
    })
}

fn unique(keys: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    keys.filter(|k| seen.insert(k.clone())).collect()
}

fn first_line(text: &str) -> &str {
    let line = text.lines().next().unwrap_or(text);
    match line.char_indices().nth(120) {
        Some((at, _)) => &line[..at],
        None => line,
    }
}

/// The map's SVG with each picture nested into its tile's room.
fn nest(svg: &str, map: &Map, shown: &[(usize, Shown)]) -> String {
    let Some(end) = svg.rfind("</svg>") else {
        return svg.to_owned();
    };
    let mut out = String::with_capacity(svg.len() + 4096);
    out.push_str(&svg[..end]);
    for (at, what) in shown {
        let tile = &map.tiles[*at];
        let room = back(map.fit, tile.picture);
        match what {
            Shown::Picture(inner) => {
                if let Some((view_box, body)) = parts(inner) {
                    let _ = write!(
                        out,
                        r#"<svg x="{}" y="{}" width="{}" height="{}" viewBox="{view_box}" preserveAspectRatio="xMidYMid meet">"#,
                        room.0, room.1, room.2, room.3
                    );
                    out.push_str(&own_ids(&body, &format!("t{at}-")));
                    out.push_str("</svg>");
                }
            }
            Shown::Words(words) => {
                let _ = write!(
                    out,
                    r##"<text x="{}" y="{}" text-anchor="middle" font-family="sans-serif" font-size="11" fill="#888888">{}</text>"##,
                    room.0 + room.2 / 2.0,
                    room.1 + room.3 / 2.0,
                    escape(&shorten(words, room.2))
                );
            }
        }
    }
    out.push_str(&svg[end..]);
    out
}

/// A rectangle of the picture in the units of the map's SVG.
fn back(fit: Fit, r: omaspec_uml::graph::Rect) -> (f32, f32, f32, f32) {
    let (sx, sy) = (fit.sx.max(f32::EPSILON), fit.sy.max(f32::EPSILON));
    (r.x / sx, r.y / sy, r.w / sx, r.h / sy)
}

/// The view box and the content of a sanitized SVG.
fn parts(svg: &str) -> Option<(String, String)> {
    let doc = roxmltree::Document::parse(svg).ok()?;
    let root = doc.root_element();
    let number = |name: &str| {
        root.attribute(name)
            .and_then(|v| v.trim_end_matches("px").parse::<f32>().ok())
    };
    let view_box = match root.attribute("viewBox") {
        Some(v) => v.to_owned(),
        None => format!("0 0 {} {}", number("width")?, number("height")?),
    };
    let open = svg.find("<svg")?;
    let body_start = open + svg[open..].find('>')? + 1;
    let body_end = svg.rfind("</svg>")?;
    let body = svg.get(body_start..body_end)?;
    // A style sheet would reach the whole map.
    let mut kept = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find("<style") {
        kept.push_str(&rest[..at]);
        match rest[at..].find("</style>") {
            Some(close) => rest = &rest[at + close + "</style>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    kept.push_str(rest);
    Some((view_box, kept))
}

/// `body` with its ids and the references to them made its own.
fn own_ids(body: &str, prefix: &str) -> String {
    body.replace(" id=\"", &format!(" id=\"{prefix}"))
        .replace("url(#", &format!("url(#{prefix}"))
        .replace("href=\"#", &format!("href=\"#{prefix}"))
}

/// Words that fit about `width` units at the size they are drawn in.
fn shorten(words: &str, width: f32) -> String {
    let most = ((width / 6.0) as usize).max(8);
    if words.chars().count() <= most {
        return words.to_owned();
    }
    let mut out: String = words.chars().take(most - 1).collect();
    out.push('…');
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The function a map in `language` is drawn with, for [`render`].
pub(crate) fn drawer<'a>(
    source: &'a str,
    language: Language,
    place: Option<&'a Place>,
) -> impl FnMut(&Tiles) -> Drawing + 'a {
    move |tiles: &Tiles| match language {
        Language::Mermaid => mermaid::render_tiled(source, tiles),
        Language::PlantUml => {
            let folder = place.map(include::Folder::new);
            let includes = folder.as_ref().map(|f| f as &dyn omaspec_uml::Includes);
            omaspec_uml::render_tiled(source, includes, Some(tiles))
                .map(|(svg, map)| (svg, Engine::NativePlantUml, map))
                .map_err(|d| RenderError::Unavailable {
                    line: d.line,
                    reason: d.reason,
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Renderer};

    fn entry(file: &str, source: &str) -> Entry {
        let language = Language::from_file_name(file).unwrap();
        Entry {
            file: file.into(),
            names: omaspec_core::project::names(source, language, file),
            source: source.into(),
            language,
            place: None,
        }
    }

    fn project() -> Atlas {
        Atlas {
            diagrams: vec![
                entry("system.puml", MAP),
                entry(
                    "orders.puml",
                    "@startuml\ntitle Orders\n[*] --> Open\nOpen --> Checkout : pay\nCheckout --> Open : back\n@enduml\n",
                ),
                entry(
                    "orders/checkout.puml",
                    "@startuml\nstart\n:Pay;\nstop\n@enduml\n",
                ),
                entry(
                    "payment.puml",
                    "@startuml\n[*] --> Checkout\nCheckout --> Paid : ok\n@enduml\n",
                ),
                entry(
                    "refund.mmd",
                    "stateDiagram-v2\n[*] --> Asked\nAsked --> Paid : pay\n",
                ),
                entry("ideas.puml", "@startuml\nstart\n:A;\nstop\n@enduml\n"),
                entry("ideas.mmd", "flowchart LR\nA --> B\n"),
            ],
            main: Some("system.puml".into()),
        }
    }

    const MAP: &str =
        "@startwbs\n* Shop\n** Orders\n*** Checkout\n** Payment\n** Ideas\n** Later\n@endwbs\n";

    #[test]
    fn a_map_shows_its_diagrams_in_tiles_and_where_they_link() {
        let mut renderer = Renderer::new(&Config {
            java_fallback: false,
            ..Config::default()
        });
        let atlas = project();
        let drawn = renderer
            .render_linked(MAP, Language::PlantUml, None, Some("system.puml"), &atlas)
            .unwrap();
        let tiles: Vec<(&str, Option<&str>, Vec<&str>)> = drawn
            .map
            .tiles
            .iter()
            .zip(&drawn.links)
            .map(|(t, l)| {
                (
                    t.name.as_str(),
                    t.parent.as_deref(),
                    l.files.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            tiles,
            [
                ("Orders", None, vec!["orders.puml"]),
                ("Checkout", Some("orders"), vec!["orders/checkout.puml"]),
                ("Payment", None, vec!["payment.puml"]),
                ("Ideas", None, vec!["ideas.puml", "ideas.mmd"]),
                ("refund", None, vec!["refund.mmd"]),
            ]
        );
        assert!(drawn.links.iter().all(|l| l.tile));
        // Four pictures nested in the map; the two diagrams called Ideas
        // say so; Later names nothing and stays a box.
        assert_eq!(drawn.svg.matches("<svg").count(), 5, "{}", drawn.svg);
        assert!(drawn.svg.contains("2 diagrams go by this name"));
        assert!(drawn.svg.contains(">Later<"));
        assert!(drawn.svg.contains(LOOSE_HEADING));
        // Orders links to Checkout, which hangs below it: no arrow. Payment
        // links to Checkout too: an arrow.
        assert_eq!(drawn.svg.matches("stroke-dasharray").count(), 1);
        assert!(crate::export::parse(&drawn.svg).is_ok());

        // The same map, not the main one: no tiles below it.
        let side = Atlas {
            main: None,
            ..atlas.clone()
        };
        let drawn = renderer
            .render_linked(MAP, Language::PlantUml, None, Some("system.puml"), &side)
            .unwrap();
        assert_eq!(drawn.links.len(), 4);
        assert!(!drawn.svg.contains(LOOSE_HEADING));
    }

    #[test]
    fn a_state_links_to_the_diagram_of_its_name() {
        let mut renderer = Renderer::new(&Config {
            java_fallback: false,
            ..Config::default()
        });
        let atlas = project();
        let orders = &atlas.diagrams[1];
        let drawn = renderer
            .render_linked(
                &orders.source,
                orders.language,
                None,
                Some(&orders.file),
                &atlas,
            )
            .unwrap();
        let links: Vec<(&str, &[String], bool)> = drawn
            .links
            .iter()
            .map(|l| (l.name.as_str(), l.files.as_slice(), l.tile))
            .collect();
        assert_eq!(
            links,
            [("Checkout", &["orders/checkout.puml".to_owned()][..], false)]
        );
        let state = drawn
            .map
            .states
            .iter()
            .find(|s| s.name == "Checkout")
            .unwrap();
        assert_eq!(drawn.links[0].rect, state.rect);
    }

    #[test]
    fn a_mermaid_mind_map_is_a_map_as_well() {
        let mut renderer = Renderer::new(&Config::default());
        let map = "mindmap\n  root((Shop))\n    Orders\n    Payment\n";
        let atlas = Atlas {
            main: None,
            ..project()
        };
        let drawn = renderer
            .render_linked(map, Language::Mermaid, None, Some("system.mmd"), &atlas)
            .unwrap();
        let names: Vec<&str> = drawn.links.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Orders", "Payment"]);
        assert_eq!(drawn.svg.matches("<svg").count(), 3);
    }

    #[test]
    fn a_nested_picture_keeps_its_ids_and_no_style_sheet() {
        let body = parts(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="5"><style>rect{fill:red}</style><defs><marker id="m"/></defs><path marker-end="url(#m)"/><use href="#m"/></svg>"##,
        )
        .unwrap();
        assert_eq!(body.0, "0 0 10 5");
        let owned = own_ids(&body.1, "t3-");
        assert!(!owned.contains("style"), "{owned}");
        assert!(owned.contains(r#"id="t3-m""#) && owned.contains("url(#t3-m)"));
        assert!(owned.contains(r##"href="#t3-m""##), "{owned}");
    }
}
