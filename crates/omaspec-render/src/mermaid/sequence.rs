//! Mermaid sequence diagrams on the PlantUML sequence engine.
//!
//! mmdr has parsed the diagram. The engine reads the same source again
//! ([`from_mermaid`]) for the arrow heads, the activations and the
//! participant shapes, which mmdr's graph does not keep, and draws it in
//! the colors of the Mermaid theme. A construct the engine cannot draw
//! (an `@{ }` that is not a participant's type or alias, an unknown line,
//! a color it cannot read) leaves the diagram to mmdr.

use super::{font_name, plain_number};
use mermaid_rs_renderer::{DiagramKind, Graph, Theme};
use omaspec_uml::map::Map;
use omaspec_uml::sequence::{Skinparam, draw_mapped, from_mermaid};

/// The SVG and where its messages are, or `None` when this drawer leaves
/// the diagram to mmdr.
pub fn try_draw(graph: &Graph, theme: &Theme, source: &str) -> Option<(String, Map)> {
    if graph.kind != DiagramKind::Sequence {
        return None;
    }
    let mut diagram = from_mermaid(source)?;
    diagram.frame.skinparams = skin_of(theme)?;
    draw_mapped(&diagram).ok()
}

fn skin_of(theme: &Theme) -> Option<Vec<Skinparam>> {
    let mut params = Vec::new();
    let mut add = |key: &str, value: &str| {
        params.push(Skinparam {
            key: key.to_owned(),
            value: value.to_owned(),
            line: 0,
            declared: None,
        });
    };
    if let Some(name) = font_name(&theme.font_family) {
        add("defaultFontName", &name);
    }
    add("defaultFontSize", &plain_number(theme.font_size));
    add("defaultFontColor", &theme.primary_text_color);
    add("backgroundColor", &theme.background);
    add("arrowColor", &theme.line_color);
    add("arrowFontColor", &theme.text_color);
    add("sequenceMessageAlignment", "center");
    add("sequenceLifelineBorderColor", &theme.sequence_actor_line);
    add(
        "sequenceLifelineBackgroundColor",
        &theme.sequence_activation_fill,
    );
    add("sequenceGroupBorderColor", &theme.cluster_border);
    add("sequenceGroupHeaderBackgroundColor", &theme.secondary_color);
    add("sequenceGroupHeaderFontColor", &theme.text_color);
    add("sequenceGroupFontColor", &theme.text_color);
    for element in [
        "participant",
        "actor",
        "boundary",
        "control",
        "entity",
        "database",
        "sequenceBox",
    ] {
        add(
            &format!("{element}BackgroundColor"),
            &theme.sequence_actor_fill,
        );
        add(
            &format!("{element}BorderColor"),
            &theme.sequence_actor_border,
        );
        add(&format!("{element}FontColor"), &theme.primary_text_color);
    }
    add("noteBackgroundColor", &theme.sequence_note_fill);
    add("noteBorderColor", &theme.sequence_note_border);
    add("noteFontColor", &theme.text_color);
    // A color the engine cannot read would decline the drawing. Check
    // first, so a bad theme falls back instead of panicking later.
    for param in &params {
        if param.key.to_ascii_lowercase().contains("color") {
            omaspec_uml::parse_color(&param.value).ok()?;
        }
    }
    Some(params)
}

#[cfg(test)]
mod tests;
