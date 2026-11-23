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
    /// The names asked for that it holds, in the order asked, each once:
    /// folded.
    pub held: Vec<String>,
    /// Every container it holds, as drawn without a fold.
    pub offered: Vec<String>,
    /// The Java PlantUML drew it, which folds nothing and names no box.
    pub java: bool,
}

impl Folded {
    /// Adds what it held and offered to what several diagrams did, each
    /// name once.
    pub(crate) fn gather(&self, held: &mut Vec<String>, offered: &mut Vec<String>) {
        for (from, into) in [(&self.held, held), (&self.offered, offered)] {
            for name in from {
                if !into.contains(name) {
                    into.push(name.clone());
                }
            }
        }
    }
}

/// The names `values` of `--fold` give: apart at each comma, but for one
/// written `\,` or inside double quotes round a name, as `--events` takes
/// them; each trimmed and once. An empty name is an error.
pub(crate) fn names(values: &[String]) -> anyhow::Result<Vec<String>> {
    let split = crate::editor::moving::split_names(values, "--fold", "container")?;
    let mut out: Vec<String> = Vec::new();
    for name in split {
        if !out.contains(&name) {
            out.push(name);
        }
    }
    Ok(out)
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
    let found = of(text, &whole, folds);
    let already = omaspec_uml::fold::read(text);
    let more: Vec<String> = found
        .held
        .iter()
        .filter(|name| !already.contains(name))
        .cloned()
        .collect();
    if more.is_empty() {
        return Ok((whole, found));
    }
    let text = folded(text, &more, language);
    let mut drawn = crate::look::render(maps, renderer, &text, language, path, place, skin)?;
    // It was drawn twice: both take their time.
    drawn.elapsed += whole.elapsed;
    Ok((drawn, found))
}

/// What `text`, drawn as it is, holds of `folds`, without the picture
/// folded: for names to be checked before anything is written.
pub(crate) fn held(
    maps: &mut Maps,
    renderer: &mut Renderer,
    text: &str,
    language: Language,
    path: Option<&Path>,
    place: Option<&Place>,
    folds: &[String],
) -> Result<Folded, RenderError> {
    let whole = crate::look::render(maps, renderer, text, language, path, place, None)?;
    Ok(of(text, &whole, folds))
}

/// What `text`, drawn as `whole`, holds of `folds`.
fn of(text: &str, whole: &Rendered, folds: &[String]) -> Folded {
    let java = whole.engine == omaspec_render::Engine::JavaPlantUml;
    // A container the file folds itself (a fold line in it, as docs/adr/0024
    // lets one write) is drawn folded already: held, and offered, where the
    // picture has its box; Java's has none, nor a line left from a name.
    let already: Vec<String> = omaspec_uml::fold::read(text)
        .into_iter()
        .filter(|name| !java && whole.map.places.iter().any(|place| &place.name == name))
        .collect();
    let mut offered = containers(&whole.map);
    for name in &already {
        if !offered.contains(name) {
            offered.push(name.clone());
        }
    }
    let mut held: Vec<String> = Vec::new();
    for name in folds {
        if offered.contains(name) && !held.contains(name) {
            held.push(name.clone());
        }
    }
    Folded {
        held,
        offered,
        java,
    }
}

/// `names` as a list to read: one with a comma in it in double quotes.
fn listed(names: &[String]) -> String {
    if names.is_empty() {
        return "none".to_owned();
    }
    names
        .iter()
        .map(|name| match name.contains(',') {
            true => format!("\"{name}\""),
            false => name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// What is said of `name`, no container of a diagram that holds `offered`
/// (its [`Folded::offered`]); one the Java PlantUML drew folds nothing.
pub(crate) fn not_held(name: &str, offered: &[String]) -> String {
    format!(
        "{} is no container to fold; the diagram has {}",
        listed(&[name.to_owned()]),
        listed(offered)
    )
}

/// What is said of `name`, held by none of a diagram drawn as `folded`
/// says.
pub(crate) fn not_held_in(name: &str, folded: &Folded) -> String {
    if folded.java {
        format!(
            "{} is no container to fold: the Java PlantUML drew the diagram, which folds nothing",
            listed(&[name.to_owned()])
        )
    } else {
        not_held(name, &folded.offered)
    }
}

/// What is said of `name`, held by none of several diagrams, which have
/// the containers `offered` between them.
pub(crate) fn held_by_none(name: &str, offered: &[String]) -> String {
    format!(
        "{} is no container to fold in any of the diagrams; they have {}",
        listed(&[name.to_owned()]),
        listed(offered)
    )
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
        assert!(!folded.java);
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

    /// A name asked twice is folded once; one the file folds itself is
    /// held; names are taken apart at commas but for one in quotes.
    #[test]
    fn names_are_taken_once_and_a_fold_line_counts() {
        let (_, folded) = draw(BREWING, Language::PlantUml, &["Brewing", "Brewing"]);
        assert_eq!(folded.held, ["Brewing"]);
        let written = format!("{BREWING}' fold (Omaspec) Brewing\n");
        let (drawn, folded) = draw(&written, Language::PlantUml, &["Brewing"]);
        assert_eq!(folded.held, ["Brewing"]);
        assert!(!drawn.svg.contains(">Heat<"));
        assert_eq!(
            names(&[" Brewing, Payment ".into(), r#""Sales, EU""#.into()]).unwrap(),
            ["Brewing", "Payment", "Sales, EU"]
        );
        assert!(names(&["Brewing,,Payment".into()]).is_err());
        assert_eq!(
            not_held("Pantry", &["Sales, EU".into(), "Brewing".into()]),
            "Pantry is no container to fold; the diagram has \"Sales, EU\", Brewing"
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
