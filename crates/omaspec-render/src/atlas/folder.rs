//! The map of a project, made from the project and written nowhere
//! (`docs/adr/0026-the-map-is-made.md`). A project is a folder of
//! diagrams; its map shows each of them in a tile, as it is drawn, and
//! arrows where one plays against, opens or includes another. Nobody
//! lists the diagrams by hand, so none is missing from it, and a mind map
//! or a WBS is a diagram like any other.
//!
//! A folder inside the project groups its diagrams. In a project of up to
//! [`WHOLE`] diagrams the map shows all of them: a folder is a frame
//! around its diagrams. In a larger one a folder is a tile that shows how many
//! diagrams it holds and opens its own map ([`super::Link::folder`]), and
//! an arrow to or from the folder stands for those of the diagrams in it. So a map shows
//! one level of a large project, and what a level holds stays in sight.

use std::collections::HashMap;
use std::sync::Arc;

use omaspec_core::Kind;
use omaspec_core::project::key;
use omaspec_uml::tree::{Branch, Card, Heading, Icon, Relation, RelationKind, Tiles};

use super::cards::{counted, includes, players, resolve, size_of};
use super::{Atlas, Entry, Link, Picture, Pictures, Shown, back, draw_all, first_line, nest};
use crate::{Engine, RenderError, Rendered, sanitize};

/// A project of up to this many diagrams is on one map, folders and all.
pub const WHOLE: usize = 36;

/// What the text of a diagram tells of where it leads, without drawing
/// it: enough for the arrows of a folder's tile.
#[derive(Debug, Default)]
pub(crate) struct Facts {
    /// A state machine: what a scenario plays against.
    machine: bool,
    /// A scenario's participants, by key.
    players: Vec<String>,
    /// The files it includes, as written.
    includes: Vec<String>,
}

impl Facts {
    pub(super) fn of(entry: &Entry) -> Facts {
        let kind = omaspec_core::classify(&entry.source, entry.language).kind;
        Facts {
            machine: kind == Kind::State,
            players: if kind == Kind::Sequence {
                players(&entry.source, entry.language)
            } else {
                Vec::new()
            },
            includes: includes(&entry.source, entry.language),
        }
    }
}

/// A folder of the project: the diagrams in it and the folders in it.
struct Dir {
    name: String,
    /// Its path in the project, `/` between folders; empty for the project.
    path: String,
    diagrams: Vec<usize>,
    dirs: Vec<Dir>,
}

impl Dir {
    /// The folder at `path` with the diagrams of `atlas` below it.
    fn of(atlas: &Atlas, path: &str) -> Dir {
        let mut top = Dir {
            name: path.rsplit('/').next().unwrap_or(path).to_owned(),
            path: path.to_owned(),
            diagrams: Vec::new(),
            dirs: Vec::new(),
        };
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        for (index, entry) in atlas.diagrams.iter().enumerate() {
            let Some(rest) = entry.file.strip_prefix(&prefix) else {
                continue;
            };
            let mut at = &mut top;
            let mut parts: Vec<&str> = rest.split('/').collect();
            parts.pop();
            for part in parts {
                let found = at.dirs.iter().position(|d| d.name == part);
                let next = found.unwrap_or_else(|| {
                    let path = if at.path.is_empty() {
                        part.to_owned()
                    } else {
                        format!("{}/{part}", at.path)
                    };
                    at.dirs.push(Dir {
                        name: part.to_owned(),
                        path,
                        diagrams: Vec::new(),
                        dirs: Vec::new(),
                    });
                    at.dirs.len() - 1
                });
                at = &mut at.dirs[next];
            }
            at.diagrams.push(index);
        }
        top
    }

    /// How many diagrams it holds, those of its folders too.
    fn count(&self) -> usize {
        self.diagrams.len() + self.dirs.iter().map(Dir::count).sum::<usize>()
    }
}

/// What a diagram is called on its tile: its first name.
fn name_of(entry: &Entry) -> String {
    entry
        .names
        .first()
        .cloned()
        .unwrap_or_else(|| entry.file.clone())
}

/// The key of a folder's tile: its path, marked as no file.
fn folder_key(path: &str) -> String {
    format!("{path}/")
}

/// The boxes of `dir`'s map: a tile for each diagram, and for each folder
/// a frame around its own (`whole`) or a tile.
fn branches(dir: &Dir, atlas: &Atlas, whole: bool) -> Vec<Branch> {
    let mut out: Vec<Branch> = dir
        .diagrams
        .iter()
        .map(|&index| Branch {
            words: name_of(&atlas.diagrams[index]),
            key: Some(atlas.diagrams[index].file.clone()),
            below: Vec::new(),
        })
        .collect();
    for sub in &dir.dirs {
        out.push(if whole {
            Branch {
                words: sub.name.clone(),
                key: None,
                below: branches(sub, atlas, true),
            }
        } else {
            Branch {
                words: sub.name.clone(),
                key: Some(folder_key(&sub.path)),
                below: Vec::new(),
            }
        });
    }
    out
}

/// What a folder's tile shows: a block of small boxes, one for each
/// diagram in it, so its size is seen at a glance. Gray that reads on a
/// light page and on a dark one; a folder of more than [`BOXES`] diagrams
/// shows that many. Its own map, small, would be drawn and parsed for
/// every folder of a large project (4.8 MB for 40 folders of 50).
fn thumbnail(count: usize) -> String {
    const BOXES: usize = 96;
    const BOX: (f32, f32) = (26.0, 16.0);
    const GAP: f32 = 6.0;
    const PAD: f32 = 10.0;
    let shown = count.clamp(1, BOXES);
    let columns = ((shown as f32 * 1.6).sqrt().ceil() as usize).clamp(1, 12);
    let rows = shown.div_ceil(columns);
    let width = 2.0 * PAD + columns as f32 * (BOX.0 + GAP) - GAP;
    let height = 2.0 * PAD + rows as f32 * (BOX.1 + GAP) - GAP;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#
    );
    for at in 0..shown {
        let (column, row) = (at % columns, at / columns);
        svg.push_str(&format!(
            r##"<rect x="{}" y="{}" width="{}" height="{}" rx="3" fill="#808080" fill-opacity="0.3"/>"##,
            PAD + column as f32 * (BOX.0 + GAP),
            PAD + row as f32 * (BOX.1 + GAP),
            BOX.0,
            BOX.1
        ));
    }
    svg.push_str("</svg>");
    svg
}

/// Draws the map of `folder` of the project in `atlas`, in the look of
/// `skin` (a WBS whose skinparams color it).
pub(crate) fn render(
    atlas: &Atlas,
    folder: &str,
    skin: &str,
    pictures: &mut Pictures,
) -> Result<Rendered, RenderError> {
    let start = std::time::Instant::now();
    let folder = folder.trim_matches('/');
    let top = Dir::of(atlas, folder);
    let whole = top.count() <= WHOLE;
    let title = if folder.is_empty() {
        atlas.name.clone().unwrap_or_else(|| "Map".to_owned())
    } else {
        top.name.clone()
    };
    let roots = branches(&top, atlas, whole);

    // The diagrams with a tile of their own, and the folders with one.
    fn tiles_of<'d>(dir: &'d Dir, whole: bool, shown: &mut Vec<usize>, folded: &mut Vec<&'d Dir>) {
        shown.extend(&dir.diagrams);
        for sub in &dir.dirs {
            if whole {
                tiles_of(sub, true, shown, folded);
            } else {
                folded.push(sub);
            }
        }
    }
    let (mut shown, mut folded) = (Vec::new(), Vec::new());
    tiles_of(&top, whole, &mut shown, &mut folded);

    pictures.keep_only(atlas);
    let wanted: Vec<&Entry> = shown.iter().map(|&i| &atlas.diagrams[i]).collect();
    let missing = pictures.missing(&wanted);
    for (entry, picture) in missing.iter().zip(draw_all(&missing)) {
        pictures.keep(entry, picture);
    }
    let drawn: HashMap<usize, Picture> = shown
        .iter()
        .filter_map(|&i| Some((i, pictures.kept(&atlas.diagrams[i])?.clone())))
        .collect();
    let thumbs: HashMap<String, (String, Card)> = folded
        .iter()
        .map(|dir| {
            let small = thumbnail(dir.count());
            let size = size_of(&small).unwrap_or((0.0, 0.0));
            let card = Card {
                kind: "Folder".to_owned(),
                icon: Icon::Map,
                facts: counted(dir.count(), "diagram"),
                size,
            };
            (folder_key(&dir.path), (small, card))
        })
        .collect();

    // Where each diagram of the folder shows on this map: on its own
    // tile, or on the tile of the folder it is in.
    let home = |index: usize| -> Option<String> {
        let file = &atlas.diagrams[index].file;
        if drawn.contains_key(&index) || shown.contains(&index) {
            return Some(file.clone());
        }
        folded
            .iter()
            .map(|dir| folder_key(&dir.path))
            .find(|key| file.starts_with(key.as_str()))
    };
    let inside: Vec<usize> = {
        let mut all = shown.clone();
        for dir in &folded {
            let key = folder_key(&dir.path);
            all.extend(
                atlas
                    .diagrams
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.file.starts_with(&key))
                    .map(|(i, _)| i),
            );
        }
        all
    };
    let relations = relations(atlas, &inside, &drawn, pictures, &home);

    let of_project = atlas.diagrams.len();
    let heading = Heading {
        title,
        summary: if folder.is_empty() {
            format!("System map · {}", counted(of_project, "diagram"))
        } else {
            format!("{folder} · {}", counted(top.count(), "diagram"))
        },
    };
    let by_file: HashMap<&str, usize> = shown
        .iter()
        .map(|&i| (atlas.diagrams[i].file.as_str(), i))
        .collect();
    let card = |k: &str| {
        by_file
            .get(k)
            .and_then(|i| drawn.get(i))
            .map(|p| p.card.clone())
            .or_else(|| thumbs.get(k).map(|(_, card)| card.clone()))
    };
    let (svg, map) = omaspec_uml::render_branches(
        skin,
        &roots,
        &Tiles {
            card: &card,
            relations: &relations,
            heading: Some(&heading),
        },
    )
    .map_err(|declined| RenderError::Unavailable {
        line: declined.line,
        reason: declined.reason,
    })?;
    let engine = Engine::NativePlantUml;

    // Each tile's picture, or why there is none.
    let pictured: Vec<(usize, Shown)> = map
        .tiles
        .iter()
        .enumerate()
        .map(|(at, tile)| {
            let diagram = by_file.get(tile.key.as_str()).and_then(|i| drawn.get(i));
            let shown = match (diagram, thumbs.get(&tile.key)) {
                (Some(picture), _) => match &picture.result {
                    Ok((svg, _)) => Shown::Picture(svg),
                    Err(why) => Shown::Words(format!("not drawn: {}", first_line(why))),
                },
                (None, Some((small, _))) => Shown::Picture(small),
                _ => Shown::Words("no diagram".into()),
            };
            (at, shown)
        })
        .collect();
    let invalid = |e: sanitize::SanitizeError| RenderError::Engine {
        engine,
        message: format!("invalid SVG: {e}"),
    };
    let cards = sanitize::sanitize(&nest(&svg, &map, &pictured, false)).map_err(invalid)?;
    let svg = sanitize::sanitize(&nest(&svg, &map, &pictured, true)).map_err(invalid)?;
    let links = map
        .tiles
        .iter()
        .map(|tile| {
            let (x, y, w, h) = back(map.fit, tile.picture);
            let file = by_file.contains_key(tile.key.as_str());
            Link {
                rect: tile.frame,
                name: tile.name.clone(),
                files: if file {
                    vec![tile.key.clone()]
                } else {
                    Vec::new()
                },
                tile: true,
                room: Some(omaspec_uml::graph::Rect { x, y, w, h }),
                folder: (!file).then(|| tile.key.trim_end_matches('/').to_owned()),
            }
        })
        .collect();
    Ok(Rendered {
        svg,
        engine,
        elapsed: start.elapsed(),
        map,
        links,
        cards: Some(cards),
    })
}

/// The arrows between the tiles of a map: from each diagram `inside` it to
/// the one it plays against, opens or includes, both at their `home`, the
/// tile they show on. A diagram drawn for its tile says which states it
/// has, so where it opens another; one inside a folder's tile is only read.
fn relations(
    atlas: &Atlas,
    inside: &[usize],
    drawn: &HashMap<usize, Picture>,
    pictures: &mut Pictures,
    home: &dyn Fn(usize) -> Option<String>,
) -> Vec<Relation> {
    // Key → the diagrams of the project that go by it.
    let mut called: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, entry) in atlas.diagrams.iter().enumerate() {
        let mut keys: Vec<String> = entry.names.iter().map(|n| key(n)).collect();
        keys.sort();
        keys.dedup();
        for k in keys {
            called.entry(k).or_default().push(index);
        }
    }
    let facts: HashMap<usize, Arc<Facts>> = inside
        .iter()
        .map(|&i| (i, pictures.facts(&atlas.diagrams[i])))
        .collect();
    let mut machine = |index: usize| -> bool {
        match facts.get(&index) {
            Some(facts) => facts.machine,
            None => pictures.facts(&atlas.diagrams[index]).machine,
        }
    };
    let mut out: Vec<Relation> = Vec::new();
    let mut add = |from: usize, to: usize, kind: RelationKind| {
        let (Some(from), Some(to)) = (home(from), home(to)) else {
            return;
        };
        let relation = Relation { from, to, kind };
        if relation.from != relation.to && !out.contains(&relation) {
            out.push(relation);
        }
    };
    for &index in inside {
        let entry = &atlas.diagrams[index];
        let own = &facts[&index];
        // A state that goes by the name of one other diagram. The boxes of
        // a mind map or a WBS are its own words and open nothing.
        if let Some(picture) = drawn.get(&index).filter(|_| own.machine) {
            for name in &picture.names {
                let k = key(name);
                if entry.names.iter().any(|n| key(n) == k) {
                    continue;
                }
                if let Some([to]) = called.get(&k).map(Vec::as_slice)
                    && *to != index
                {
                    add(index, *to, RelationKind::Opens);
                }
            }
        }
        for player in &own.players {
            let machines: Vec<usize> = called
                .get(player)
                .map(|found| {
                    found
                        .iter()
                        .copied()
                        .filter(|&i| i != index && machine(i))
                        .collect()
                })
                .unwrap_or_default();
            // Two machines of one name leave the participant unchecked.
            if let [to] = machines.as_slice() {
                add(index, *to, RelationKind::Plays);
            }
        }
        for written in &own.includes {
            let Some(file) = resolve(&entry.file, written) else {
                continue;
            };
            if let Some(to) = atlas.diagrams.iter().position(|e| e.file == file) {
                add(index, to, RelationKind::Includes);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use omaspec_core::Language;

    use super::*;
    use crate::{Config, Renderer};

    const SKIN: &str = "@startwbs\n* map\n@endwbs\n";

    fn entry(file: &str, source: &str) -> Entry {
        Entry::new(
            file,
            source.into(),
            Language::from_file_name(file).unwrap(),
            None,
        )
    }

    fn machine(name: &str) -> String {
        format!("@startuml\ntitle {name}\n[*] --> Idle\nIdle --> Busy : start\n@enduml\n")
    }

    fn scenario(name: &str, player: &str) -> String {
        format!(
            "@startuml\ntitle {name}\nparticipant User\nparticipant {player}\nUser -> {player} : start\n@enduml\n"
        )
    }

    fn renderer() -> Renderer {
        Renderer::new(&Config {
            java_fallback: false,
            ..Config::default()
        })
    }

    /// A console, what surrounds it, a plan, and a folder of scenarios.
    fn console() -> Atlas {
        Atlas {
            diagrams: vec![
                entry("console.puml", &machine("Console")),
                entry(
                    "plan.puml",
                    "@startwbs\n* Plan\n** Console\n** Later\n@endwbs\n",
                ),
                entry("scenarios/boot.puml", &scenario("Boot", "Console")),
                entry("scenarios/play.puml", &scenario("Play", "Console")),
            ],
            name: Some("First Console".into()),
        }
    }

    #[test]
    fn a_project_s_map_is_made_from_its_diagrams() {
        let atlas = console();
        let drawn = renderer().render_map(&atlas, "", SKIN).unwrap();
        // Every diagram has a tile, by its file; nobody listed them.
        let tiles: Vec<(&str, &[String], Option<&str>)> = drawn
            .links
            .iter()
            .map(|l| (l.name.as_str(), l.files.as_slice(), l.folder.as_deref()))
            .collect();
        assert_eq!(
            tiles,
            [
                ("Console", &["console.puml".to_owned()][..], None),
                // A diagram without a title goes by its file.
                ("plan", &["plan.puml".to_owned()][..], None),
                ("Boot", &["scenarios/boot.puml".to_owned()][..], None),
                ("Play", &["scenarios/play.puml".to_owned()][..], None),
            ]
        );
        assert!(drawn.links.iter().all(|l| l.tile && l.room.is_some()));
        // The project's name heads it, with what it holds; the folder is a
        // frame around its scenarios.
        assert!(drawn.svg.contains(">First Console<"));
        assert!(drawn.svg.contains("System map · 4 diagrams"));
        assert!(drawn.svg.contains(">scenarios<"));
        let place = |key: &str| drawn.map.tiles.iter().find(|t| t.key == key).unwrap();
        // The scenarios play the console and stand above it: their lines
        // share the words and the head.
        assert!(place("scenarios/boot.puml").frame.y < place("console.puml").frame.y);
        assert_eq!(drawn.svg.matches(">plays<").count(), 1);
        // The plan is a WBS like any other: a tile with its own picture,
        // and its boxes place nothing and open nothing.
        assert!(drawn.svg.contains("WBS · PlantUML"));
        assert!(!drawn.svg.contains(">opens<"));
        // The pictures nested, and the cards without them.
        assert_eq!(drawn.svg.matches("<svg").count(), 5);
        let cards = drawn.cards.as_deref().expect("a map's cards");
        assert_eq!(cards.matches("<svg").count(), 1);
        assert!(crate::export::parse(&drawn.svg).is_ok());
    }

    #[test]
    fn a_state_that_names_a_diagram_opens_it_on_the_map() {
        // Orders has a state called Checkout, and so has Payment; Checkout
        // is a diagram of its own, a folder down.
        let atlas = Atlas {
            diagrams: vec![
                entry(
                    "orders.puml",
                    "@startuml\ntitle Orders\n[*] --> Open\nOpen --> Checkout : pay\n@enduml\n",
                ),
                entry(
                    "orders/checkout.puml",
                    "@startuml\ntitle Checkout\nstart\n:Pay;\nstop\n@enduml\n",
                ),
                entry(
                    "payment.puml",
                    "@startuml\ntitle Payment\n[*] --> Checkout\nCheckout --> Paid : ok\n@enduml\n",
                ),
                entry("common.puml", &machine("Common")),
                entry(
                    "uses.puml",
                    "@startuml\ntitle Uses\n!include common.puml\n[*] --> Here\n@enduml\n",
                ),
            ],
            name: None,
        };
        let drawn = renderer().render_map(&atlas, "", SKIN).unwrap();
        // The two that open Checkout share their words and their head;
        // and an arrow for the file included.
        assert_eq!(drawn.svg.matches(">opens<").count(), 1);
        assert_eq!(drawn.svg.matches(">includes<").count(), 1);
        // Without a name of its own the project's map is called Map.
        assert!(drawn.svg.contains(">Map<"));
    }

    #[test]
    fn a_folder_has_a_map_of_its_own() {
        let atlas = console();
        let drawn = renderer().render_map(&atlas, "scenarios", SKIN).unwrap();
        let names: Vec<&str> = drawn.links.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Boot", "Play"]);
        assert!(drawn.svg.contains("scenarios · 2 diagrams"));
        // The console is not on it, so nothing is played here.
        assert!(!drawn.svg.contains(">plays<"));
        // A folder that is not there is an empty map, no error.
        let none = renderer().render_map(&atlas, "nowhere", SKIN).unwrap();
        assert!(none.links.is_empty());
    }

    #[test]
    fn a_large_project_shows_its_folders_as_tiles() {
        // Two areas of twenty machines, and a scenario in the second that
        // plays a machine of the first: over the size of one map.
        let mut diagrams = vec![entry("overview.puml", &machine("Overview"))];
        for area in ["east", "west"] {
            for n in 0..20 {
                let name = format!("{area}{n}");
                diagrams.push(entry(&format!("{area}/{name}.puml"), &machine(&name)));
            }
        }
        diagrams.push(entry("west/run.puml", &scenario("Run", "east3")));
        diagrams.sort_by(|a, b| a.file.cmp(&b.file));
        assert!(diagrams.len() > WHOLE);
        let atlas = Atlas {
            diagrams,
            name: None,
        };
        let mut renderer = renderer();
        let drawn = renderer.render_map(&atlas, "", SKIN).unwrap();
        let tiles: Vec<(&str, usize, Option<&str>)> = drawn
            .links
            .iter()
            .map(|l| (l.name.as_str(), l.files.len(), l.folder.as_deref()))
            .collect();
        // The diagram beside the folders, and a tile for each folder that
        // opens its map and names no file.
        assert_eq!(
            tiles,
            [
                ("Overview", 1, None),
                ("east", 0, Some("east")),
                ("west", 0, Some("west"))
            ]
        );
        assert!(drawn.svg.contains(">20 diagrams<") && drawn.svg.contains(">21 diagrams<"));
        assert!(drawn.svg.contains("System map · 42 diagrams"));
        // The scenario's arrow, from its folder's tile to the other's.
        assert_eq!(drawn.svg.matches(">plays<").count(), 1);
        // One level: only the diagram beside the folders was drawn.
        assert_eq!(renderer.pictures.drawn.len(), 1);
        // A folder's own map holds its diagrams, small enough for one.
        let west = renderer.render_map(&atlas, "west", SKIN).unwrap();
        assert_eq!(west.links.len(), 21);
        assert!(west.links.iter().all(|l| l.folder.is_none()));
    }

    #[test]
    fn a_map_takes_the_colors_of_its_skin() {
        let atlas = console();
        let dark = "@startwbs\nskinparam backgroundColor #1a1b26\nskinparam defaultFontColor #a9b1d6\n* map\n@endwbs\n";
        let drawn = renderer().render_map(&atlas, "", dark).unwrap();
        assert!(drawn.svg.contains(r##"fill="#1A1B26""##), "the page");
        // What is no WBS gives no look.
        assert!(
            renderer()
                .render_map(&atlas, "", "@startuml\nA -> B\n@enduml\n")
                .is_err()
        );
    }
}
