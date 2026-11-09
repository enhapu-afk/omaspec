//! What a map's cards say about their diagrams, and what a diagram's
//! text tells of the diagrams it uses (`docs/adr/0026-the-map-is-made.md`).

use omaspec_core::project::key;
use omaspec_core::{Kind, Language};
use omaspec_uml::map::{Map, Shape};
use omaspec_uml::tree::{Card, Icon};

use super::{Entry, parts};

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
pub(super) fn counted(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// The width and height of a drawn SVG.
pub(super) fn size_of(svg: &str) -> Option<(f32, f32)> {
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
pub(super) fn resolve(file: &str, written: &str) -> Option<String> {
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
