//! The diagrams of a project, as its map shows them and its states link
//! to them (`docs/adr/0018-diagrams-that-link.md`). The map itself is
//! made from them in [`folder`] (`docs/adr/0026-the-map-is-made.md`).
//!
//! The pictures come from Omaspec's own engines, never from Java: a map
//! with twenty tiles draws twenty diagrams, side by side on the cores
//! there are. Each is kept until its file changes, so a change of one
//! diagram draws only that one again. Each picture is nested into the map's SVG as
//! vectors, sharp at any zoom, and the whole passes [`crate::sanitize`];
//! [`Rendered::cards`] is the map with the pictures left out, for a pane
//! to show while its tiles are small.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::hash::{Hash, Hasher};

use omaspec_core::Language;
use omaspec_core::project::key;
use omaspec_uml::map::{Fit, Map, Shape};
use omaspec_uml::tree::{self, Card};

use crate::{Place, include, mermaid, sanitize};

mod cards;
pub(crate) mod folder;
use cards::card_of;

/// Pictures drawn side by side when this many or more are missing.
const SIDE_BY_SIDE: usize = 4;
/// Stack for each of those, as deep as the render thread's.
const STACK: usize = 64 << 20;

/// The diagrams of a project, as a map shows them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Atlas {
    pub diagrams: Vec<Entry>,
    /// The project's name, which heads its map.
    pub name: Option<String>,
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

impl Entry {
    /// The diagram in the file at `file`, a path in the project, with the
    /// names it goes by and, a map, the words on its boxes.
    pub fn new(file: &str, source: String, language: Language, place: Option<Place>) -> Entry {
        Entry {
            names: omaspec_core::project::names(&source, language, file),
            file: file.to_owned(),
            source,
            language,
            place,
        }
    }
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
    /// A tile's: the room in it where the picture of its diagram lies,
    /// which a zoom into the tile grows that diagram out of.
    pub room: Option<omaspec_uml::graph::Rect>,
    /// A tile that stands for a folder of the project, by its path there:
    /// it opens that folder's map ([`folder`]), and names no diagram.
    pub folder: Option<String>,
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

/// A diagram drawn for a tile, kept by its source.
#[derive(Clone)]
pub(crate) struct Picture {
    result: Result<(String, Map), String>,
    /// The names of its states: where it opens another diagram.
    names: Vec<String>,
    /// What its card says.
    card: Card,
}

/// Pictures for tiles, each kept by its file until the file changes; and
/// what a diagram's text alone tells of where it leads ([`folder::Facts`]),
/// kept the same way for the diagrams a map does not draw.
#[derive(Default)]
pub(crate) struct Pictures {
    drawn: HashMap<String, (u64, Picture)>,
    facts: HashMap<String, (u64, std::sync::Arc<folder::Facts>)>,
}

/// What `entry` is drawn from: its source, language and place.
fn fingerprint(entry: &Entry) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    entry.source.hash(&mut hasher);
    entry.language.extension().hash(&mut hasher);
    entry
        .place
        .as_ref()
        .map(|p| (&p.dir, &p.root))
        .hash(&mut hasher);
    hasher.finish()
}

impl Pictures {
    /// The picture of `entry`, if it was drawn from the file as it is now.
    fn kept(&self, entry: &Entry) -> Option<&Picture> {
        let (id, picture) = self.drawn.get(&entry.file)?;
        (*id == fingerprint(entry)).then_some(picture)
    }

    /// Those of `entries` without a picture of the file as it is now.
    fn missing<'e>(&self, entries: &[&'e Entry]) -> Vec<&'e Entry> {
        entries
            .iter()
            .copied()
            .filter(|entry| self.kept(entry).is_none())
            .collect()
    }

    /// Keeps `picture`, drawn from `entry` as it is now.
    fn keep(&mut self, entry: &Entry, picture: Picture) {
        self.drawn
            .insert(entry.file.clone(), (fingerprint(entry), picture));
    }

    /// Forgets the files that are not in `atlas` any more.
    fn keep_only(&mut self, atlas: &Atlas) {
        let files: HashSet<&str> = atlas.diagrams.iter().map(|e| e.file.as_str()).collect();
        self.drawn.retain(|file, _| files.contains(file.as_str()));
        self.facts.retain(|file, _| files.contains(file.as_str()));
    }

    /// What the text of `entry` tells of where it leads, read once for the
    /// file as it is now.
    fn facts(&mut self, entry: &Entry) -> std::sync::Arc<folder::Facts> {
        let id = fingerprint(entry);
        match self.facts.get(&entry.file) {
            Some((kept, facts)) if *kept == id => std::sync::Arc::clone(facts),
            _ => {
                let facts = std::sync::Arc::new(folder::Facts::of(entry));
                self.facts
                    .insert(entry.file.clone(), (id, std::sync::Arc::clone(&facts)));
                facts
            }
        }
    }
}

/// `entries` drawn by Omaspec's own engines, side by side on the cores
/// there are when there are several, in their order.
fn draw_all(entries: &[&Entry]) -> Vec<Picture> {
    if entries.len() < SIDE_BY_SIDE {
        return entries.iter().map(|entry| draw_native(entry)).collect();
    }
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(entries.len());
    let chunk = entries.len().div_ceil(threads);
    std::thread::scope(|scope| {
        let workers: Vec<_> = entries
            .chunks(chunk)
            .map(|part| {
                std::thread::Builder::new()
                    .name("omaspec-tiles".into())
                    .stack_size(STACK)
                    .spawn_scoped(scope, move || {
                        part.iter()
                            .map(|entry| draw_native(entry))
                            .collect::<Vec<_>>()
                    })
            })
            .collect();
        workers
            .into_iter()
            .zip(entries.chunks(chunk))
            .flat_map(|(worker, part)| match worker {
                Ok(worker) => match worker.join() {
                    Ok(done) => done,
                    // The engine's panic ends this drawing, as on one thread.
                    Err(panic) => std::panic::resume_unwind(panic),
                },
                // No thread to spare: drawn here.
                Err(_) => part.iter().map(|entry| draw_native(entry)).collect(),
            })
            .collect()
    })
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
    let names: Vec<String> = match &result {
        Ok((_, map)) => map
            .states
            .iter()
            .filter(|s| s.shape == Shape::State && !s.name.is_empty())
            .map(|s| s.name.clone())
            .collect(),
        Err(_) => Vec::new(),
    };
    let kind = omaspec_core::classify(&entry.source, entry.language);
    let card = card_of(&kind, entry, &result);
    Picture {
        result,
        names,
        card,
    }
}

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
                room: None,
                folder: None,
            })
        })
        .collect()
}

fn first_line(text: &str) -> &str {
    let line = text.lines().next().unwrap_or(text);
    match line.char_indices().nth(120) {
        Some((at, _)) => &line[..at],
        None => line,
    }
}

/// The map's SVG with each picture nested into its tile's room, or with
/// the rooms left empty without `pictures`; what a tile says instead of a
/// picture is there either way.
fn nest(svg: &str, map: &Map, shown: &[(usize, Shown)], pictures: bool) -> String {
    let Some(end) = svg.rfind("</svg>") else {
        return svg.to_owned();
    };
    let mut out = String::with_capacity(svg.len() + 4096);
    out.push_str(&svg[..end]);
    for (at, what) in shown {
        let tile = &map.tiles[*at];
        let room = back(map.fit, tile.picture);
        match what {
            Shown::Picture(_) if !pictures => {}
            Shown::Picture(inner) => {
                if let Some((view_box, body)) = parts(inner) {
                    // The picture keeps to its card's rounded room.
                    let _ = write!(
                        out,
                        r#"<clipPath id="room-{at}"><rect x="{}" y="{}" width="{}" height="{}" rx="{}"/></clipPath><g clip-path="url(#room-{at})"><svg x="{}" y="{}" width="{}" height="{}" viewBox="{view_box}" preserveAspectRatio="xMidYMid meet">"#,
                        room.0,
                        room.1,
                        room.2,
                        room.3,
                        tree::PICTURE_ROUND,
                        room.0,
                        room.1,
                        room.2,
                        room.3
                    );
                    out.push_str(&own_ids(&body, &format!("t{at}-")));
                    out.push_str("</svg></g>");
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
pub(crate) fn own_ids(body: &str, prefix: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Renderer};

    fn entry(file: &str, source: &str) -> Entry {
        Entry::new(
            file,
            source.into(),
            Language::from_file_name(file).unwrap(),
            None,
        )
    }

    fn project() -> Atlas {
        Atlas {
            diagrams: vec![
                entry(
                    "orders.puml",
                    "@startuml\ntitle Orders\n[*] --> Open\nOpen --> Checkout : pay\nCheckout --> Open : back\n@enduml\n",
                ),
                entry(
                    "orders/checkout.puml",
                    "@startuml\nstart\n:Pay;\nstop\n@enduml\n",
                ),
                entry(
                    "plan.puml",
                    "@startwbs\n* Plan\n** Checkout\n** Later\n@endwbs\n",
                ),
            ],
            name: Some("shop".into()),
        }
    }

    #[test]
    fn a_mind_map_and_a_wbs_draw_as_themselves_in_a_project() {
        let mut renderer = Renderer::new(&Config {
            java_fallback: false,
            ..Config::default()
        });
        let atlas = project();
        // A plan of the project, its boxes reading like names of diagrams:
        // drawn as the WBS it is, linking nowhere.
        let plan = &atlas.diagrams[2];
        let drawn = renderer
            .render_linked(&plan.source, plan.language, None, Some(&plan.file), &atlas)
            .unwrap();
        let alone = renderer.render(&plan.source, plan.language).unwrap();
        assert_eq!(drawn.svg, alone.svg);
        assert!(drawn.links.is_empty() && drawn.cards.is_none());
        // Not saved yet, as an example opened in the project's window.
        let mind = "mindmap\n  root((Shop))\n    Orders\n    Checkout\n";
        let drawn = renderer
            .render_linked(mind, Language::Mermaid, None, None, &atlas)
            .unwrap();
        let alone = renderer.render(mind, Language::Mermaid).unwrap();
        assert_eq!(drawn.svg, alone.svg);
        assert!(drawn.links.is_empty() && drawn.cards.is_none());
    }

    #[test]
    fn a_state_links_to_the_diagram_of_its_name() {
        let mut renderer = Renderer::new(&Config {
            java_fallback: false,
            ..Config::default()
        });
        let atlas = project();
        let orders = &atlas.diagrams[0];
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
