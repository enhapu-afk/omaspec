//! What a map's cards say about their diagrams, and how the diagrams use
//! each other: the arrows between the cards and the words above them
//! (`docs/adr/0018-diagrams-that-link.md`, amended).

use std::collections::{HashMap, HashSet};

use omaspec_core::project::key;
use omaspec_core::{Kind, Language};
use omaspec_uml::map::{Map, Shape};
use omaspec_uml::tree::{Card, Heading, Icon, Relation, RelationKind};

use super::{Atlas, Entry, Picture, is_map, parts};

/// What the card of `entry` says: its kind, a few facts, its shape.
pub(super) fn card_of(
    kind: &omaspec_core::DiagramKind,
    entry: &Entry,
    drawn: &Result<(String, Map), String>,
) -> Card {
    let (word, icon) = match kind.kind {
        Kind::State => ("State machine".to_owned(), Icon::State),
        Kind::Sequence => ("Scenario".to_owned(), Icon::Scenario),
        Kind::Activity => ("Activity".to_owned(), Icon::Activity),
        Kind::Structure => (kind.name.to_string(), Icon::Structure),
        _ if is_map(&entry.source, entry.language) => ("Map".to_owned(), Icon::Map),
        _ => (kind.name.to_string(), Icon::Other),
    };
    let language = match entry.language {
        Language::PlantUml => "PlantUML",
        Language::Mermaid => "Mermaid",
    };
    let (facts, size) = match drawn {
        Ok((svg, map)) => {
            let states = map
                .states
                .iter()
                .filter(|s| s.shape == Shape::State)
                .count();
            let facts = match kind.kind {
                Kind::State if states > 0 => counted(states, "state"),
                Kind::Sequence if !map.messages.is_empty() => {
                    counted(map.messages.len(), "message")
                }
                _ => String::new(),
            };
            (facts, size_of(svg).unwrap_or((0.0, 0.0)))
        }
        Err(_) => ("not drawn".to_owned(), (0.0, 0.0)),
    };
    Card {
        kind: format!("{word} · {language}"),
        icon,
        facts,
        size,
    }
}

/// "1 state", "7 states".
fn counted(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// The width and height of a drawn SVG.
fn size_of(svg: &str) -> Option<(f32, f32)> {
    let (view_box, _) = parts(svg)?;
    let numbers: Vec<f32> = view_box
        .split([' ', ','])
        .filter(|v| !v.is_empty())
        .filter_map(|v| v.parse().ok())
        .collect();
    match numbers.as_slice() {
        [_, _, w, h] if *w > 0.0 && *h > 0.0 => Some((*w, *h)),
        _ => None,
    }
}

/// The participants of a scenario, by key: the code messages use and the
/// name its head shows.
pub(super) fn players(source: &str, language: Language) -> Vec<String> {
    let diagram = match language {
        Language::PlantUml => match omaspec_uml::parse(source) {
            Ok(omaspec_uml::Diagram::Sequence(d)) => Some(d),
            _ => None,
        },
        Language::Mermaid => {
            omaspec_uml::sequence::from_mermaid(omaspec_core::lang::mermaid_body(source).0)
        }
    };
    let mut out: Vec<String> = diagram
        .map(|d| {
            d.participants
                .iter()
                .flat_map(|p| [key(&p.code), key(&p.label)])
                .filter(|k| !k.is_empty())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.dedup();
    out
}

/// The files a PlantUML diagram includes, as written: `!include`,
/// `!include_once`, `!include_many` and `!includesub` (without the part's
/// name). The standard library (`<…>`) and addresses are not files of the
/// project.
pub(super) fn includes(source: &str, language: Language) -> Vec<String> {
    if language != Language::PlantUml {
        return Vec::new();
    }
    source
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("!include")?;
            let (word, path) = rest.split_once(char::is_whitespace)?;
            if !matches!(word, "" | "_once" | "_many" | "sub") {
                return None;
            }
            let path = path.trim().trim_matches('"');
            let path = if word == "sub" {
                path.split('!').next().unwrap_or(path)
            } else {
                path
            };
            (!path.is_empty() && !path.starts_with('<') && !path.contains("://"))
                .then(|| path.to_owned())
        })
        .collect()
}

/// `written`, a path from the folder of the diagram at `file`, as a path
/// in the project; `None` past its root.
fn resolve(file: &str, written: &str) -> Option<String> {
    let mut parts: Vec<&str> = file.split('/').collect();
    parts.pop();
    for part in written.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    Some(parts.join("/"))
}

/// The arrows between the tiles: where a state or a box opens another
/// diagram (beyond the tree), where a scenario plays against the state
/// machine of a participant, and where a diagram includes another one.
pub(super) fn relations(
    atlas: &Atlas,
    tile_of: &HashMap<String, usize>,
    drawn: &HashMap<usize, Picture>,
    parents: &HashMap<&str, Option<&str>>,
    tile_keys: &HashSet<&str>,
    called: &HashMap<String, Vec<usize>>,
) -> Vec<Relation> {
    let key_of: HashMap<usize, &str> = tile_of.iter().map(|(k, &i)| (i, k.as_str())).collect();
    let mut out: Vec<Relation> = Vec::new();
    let mut add = |from: &str, to: &str, kind: RelationKind| {
        let relation = Relation {
            from: from.to_owned(),
            to: to.to_owned(),
            kind,
        };
        if from != to && tile_keys.contains(to) && !out.contains(&relation) {
            out.push(relation);
        }
    };
    let mut by_entry: Vec<(&String, &usize)> = tile_of.iter().collect();
    by_entry.sort();
    for (from, index) in by_entry {
        let Some(picture) = drawn.get(index) else {
            continue;
        };
        let entry = &atlas.diagrams[*index];
        for name in &picture.names {
            let to = key(name);
            // A diagram's own name, or a link the map's own line shows.
            if parents.get(to.as_str()) == Some(&Some(from.as_str()))
                || parents.get(from.as_str()) == Some(&Some(to.as_str()))
                || entry.names.iter().any(|n| key(n) == to)
            {
                continue;
            }
            add(from, &to, RelationKind::Opens);
        }
        for player in &picture.players {
            let machines: Vec<usize> = called
                .get(player)
                .map(|found| {
                    found
                        .iter()
                        .copied()
                        .filter(|i| drawn.get(i).is_some_and(|p| p.card.icon == Icon::State))
                        .collect()
                })
                .unwrap_or_default();
            // Two machines of one name leave the participant unchecked.
            if let [machine] = machines.as_slice()
                && let Some(to) = key_of.get(machine)
            {
                add(from, to, RelationKind::Plays);
            }
        }
        for written in &picture.includes {
            let Some(file) = resolve(&entry.file, written) else {
                continue;
            };
            if let Some(to) = atlas
                .diagrams
                .iter()
                .position(|e| e.file == file)
                .and_then(|i| key_of.get(&i))
            {
                add(from, to, RelationKind::Includes);
            }
        }
    }
    out
}

/// What stands above the map in `own`: the project's name over its main
/// map, else the map's own; how many diagrams it shows; and the line that
/// puts the first diagram on no box on it.
pub(super) fn heading(
    atlas: &Atlas,
    own: Option<&str>,
    loose: &[(String, String)],
    on_map: usize,
) -> Heading {
    let entry = own.and_then(|f| atlas.diagrams.iter().find(|e| e.file == f));
    let main = own.is_some() && atlas.main.as_deref() == own;
    let own_name = entry.and_then(|e| e.names.first().cloned());
    let title = if main {
        atlas.name.clone().or(own_name)
    } else {
        own_name
    }
    .unwrap_or_else(|| "Map".to_owned());
    let others = atlas.diagrams.len() - usize::from(entry.is_some());
    let summary = if main {
        format!(
            "System map · {} · {on_map} on the map",
            counted(others, "diagram")
        )
    } else {
        format!("Map · {} on it", counted(on_map, "diagram"))
    };
    let place = loose
        .first()
        .map(|(name, _)| match entry.map(|e| e.language) {
            Some(Language::Mermaid) => format!("    {name}"),
            _ => format!("** {name}"),
        });
    Heading {
        title,
        summary,
        main,
        place,
    }
}
