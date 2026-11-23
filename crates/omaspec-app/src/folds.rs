//! `--fold NAME` (REQ-104): containers drawn folded to their box, as the
//! pane folds them (docs/adr/0024), by `omaspec render`, `omaspec export`
//! and the MCP server's `render`. The names reach the engine as the pane
//! hands its folds over, as lines after the diagram; the file, and the
//! source an export stores, stay as written. A diagram is drawn once as it
//! is first, for the containers it holds, as the pane offers them to fold.

use std::path::Path;

use omaspec_core::Language;
use omaspec_render::{Map, Place, RenderError, Rendered, Renderer};

use crate::look::Skin;
use crate::specs::Maps;

/// What a diagram drawn with folds asked for holds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Folded {
    /// The names asked for that it holds, in the order asked: folded.
    pub held: Vec<String>,
    /// Every container it holds, as drawn without a fold.
    pub offered: Vec<String>,
}

/// The containers of a picture: the boxes that hold others, which the
/// pane offers to fold.
pub(crate) fn containers(map: &Map) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for place in map.places.iter().filter(|place| place.frame) {
        if !out.contains(&place.name) {
            out.push(place.name.clone());
        }
    }
    out
}

/// `text` with the lines that fold `names` after it.
pub(crate) fn folded(text: &str, names: &[String], language: Language) -> String {
    let mut out = text.to_owned();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    for name in names {
        out.push_str(&omaspec_uml::fold::line(
            name,
            language == Language::Mermaid,
        ));
    }
    out
}

/// `text` drawn as [`crate::look::render`] draws it, with the containers
/// of `folds` it holds folded; drawn once, as it is, where `folds` is
/// empty or names none of them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    maps: &mut Maps,
    renderer: &mut Renderer,
    text: &str,
    language: Language,
    path: Option<&Path>,
    place: Option<&Place>,
    skin: Option<&Skin>,
    folds: &[String],
) -> Result<(Rendered, Folded), RenderError> {
    let whole = crate::look::render(maps, renderer, text, language, path, place, skin)?;
    if folds.is_empty() {
        return Ok((whole, Folded::default()));
    }
    let offered = containers(&whole.map);
    let held: Vec<String> = folds
        .iter()
        .filter(|name| offered.contains(name))
        .cloned()
        .collect();
    if held.is_empty() {
        return Ok((whole, Folded { held, offered }));
    }
    let text = folded(text, &held, language);
    let drawn = crate::look::render(maps, renderer, &text, language, path, place, skin)?;
    Ok((drawn, Folded { held, offered }))
}

/// What is said of `name`, no container of a diagram that holds
/// `offered`.
pub(crate) fn not_held(name: &str, offered: &[String]) -> String {
    format!(
        "{name} is no container to fold; the diagram has {}",
        if offered.is_empty() {
            "none".to_owned()
        } else {
            offered.join(", ")
        }
    )
}

/// What is said of `name`, held by none of several diagrams.
pub(crate) fn held_by_none(name: &str) -> String {
    format!("{name} is no container to fold in any of the diagrams")
}

/// The first of `folds` that `held` lacks.
pub(crate) fn first_unheld<'a>(folds: &'a [String], held: &[String]) -> Option<&'a str> {
    folds
        .iter()
        .find(|name| !held.contains(name))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(text: &str, language: Language, folds: &[&str]) -> (Rendered, Folded) {
        let mut renderer = Renderer::new(&omaspec_render::Config {
            java_fallback: false,
            ..omaspec_render::Config::default()
        });
        let folds: Vec<String> = folds.iter().map(|name| (*name).to_owned()).collect();
        render(
            &mut Maps::default(),
            &mut renderer,
            text,
            language,
            None,
            None,
            None,
            &folds,
        )
        .unwrap()
    }

    const BREWING: &str = "@startuml\n[*] --> Idle\nIdle --> Brewing : start\nstate Brewing {\n  [*] --> Heat\n  Heat --> Pour : hot\n}\nBrewing --> Idle : done\n@enduml\n";

    #[test]
    fn a_container_asked_for_is_folded_and_the_rest_is_said() {
        let (whole, none) = draw(BREWING, Language::PlantUml, &[]);
        assert_eq!(none, Folded::default());
        assert!(whole.svg.contains(">Heat<"));
        let (drawn, folded) = draw(BREWING, Language::PlantUml, &["Brewing", "Pantry"]);
        assert_eq!(folded.held, ["Brewing"]);
        assert_eq!(folded.offered, ["Brewing"]);
        assert!(!drawn.svg.contains(">Heat<"), "what it holds is gone");
        assert!(drawn.svg.contains(">Brewing<"));
        assert_eq!(
            first_unheld(&["Brewing".into(), "Pantry".into()], &folded.held),
            Some("Pantry")
        );
        assert_eq!(
            not_held("Pantry", &folded.offered),
            "Pantry is no container to fold; the diagram has Brewing"
        );
    }

    #[test]
    fn a_mermaid_subgraph_folds_too() {
        let text = "flowchart LR\n  A --> B\n  subgraph Shop\n    B --> C\n  end\n";
        let (drawn, folded) = draw(text, Language::Mermaid, &["Shop"]);
        assert_eq!(folded.held, ["Shop"]);
        assert!(
            !drawn.map.places.iter().any(|p| p.name == "C"),
            "{:?}",
            drawn.map.places
        );
    }
}
