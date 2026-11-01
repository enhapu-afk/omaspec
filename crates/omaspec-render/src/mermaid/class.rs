//! Mermaid class diagrams on the structure engine.
//!
//! mmdr has parsed the diagram. The engine reads the same source again
//! ([`from_mermaid`]) for namespaces, members, notes and styles, which
//! mmdr's class graph drops, and draws it in the colors of the Mermaid
//! theme. A construct the engine cannot draw (a lollipop, `direction RL`
//! or `BT`, a style property other than fill, stroke, width, dash and
//! text color) leaves the diagram to mmdr.

use super::{font_name, plain_number};
use mermaid_rs_renderer::{DiagramKind, Graph, Theme};
use omaspec_uml::map::Map;
use omaspec_uml::structure::{Skinparam, draw_mapped, from_mermaid};

/// The SVG and its map of lines, or `None` when this drawer leaves the
/// diagram to mmdr. Lines by hand (docs/adr/0016) are drawn here.
pub fn try_draw(graph: &Graph, theme: &Theme, source: &str) -> Option<(String, Map)> {
    if graph.kind != DiagramKind::Class {
        return None;
    }
    let mut diagram = from_mermaid(source)?;
    diagram.frame.skinparams = skin_of(theme)?;
    diagram.frame.by_hand = omaspec_uml::hand::numbered(source);
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
    add("classBackgroundColor", &theme.primary_color);
    add("classBorderColor", &theme.primary_border_color);
    add("interfaceBackgroundColor", &theme.primary_color);
    add("interfaceBorderColor", &theme.primary_border_color);
    add("packageBackgroundColor", &theme.cluster_background);
    add("packageBorderColor", &theme.cluster_border);
    add("noteBackgroundColor", &theme.sequence_note_fill);
    add("noteBorderColor", &theme.sequence_note_border);
    // A color the engine cannot read would decline the drawing. Check
    // first, so a bad theme falls back instead of drawing the wrong look.
    for param in &params {
        if param.key.to_ascii_lowercase().contains("color") {
            omaspec_uml::parse_color(&param.value).ok()?;
        }
    }
    Some(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_a_class_in_the_theme_colors() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Class;
        let (svg, map) = try_draw(&graph, &theme, "classDiagram\nclass Animal\n").unwrap();
        assert!(svg.contains("Animal"), "{svg}");
        assert!(svg.contains("#ECECFF"), "{svg}");
        // One class, no line: nothing on the map to light or draw by hand.
        assert!(map.states.is_empty() && map.wires.is_empty() && map.misses.is_empty());
    }

    #[test]
    fn leaves_another_kind_and_a_lollipop() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        assert!(try_draw(&graph, &theme, "classDiagram\nclass Animal\n").is_none());
        graph.kind = DiagramKind::Class;
        assert!(try_draw(&graph, &theme, "classDiagram\nbar ()-- foo\n").is_none());
        assert_eq!(
            crate::mermaid::refused(DiagramKind::Class, "classDiagram\nbar ()-- foo\n")
                .map(|(_, reason)| reason),
            Some("a lollipop is not drawn by Omaspec; mmdr draws it".into())
        );
        let wide = try_draw(
            &graph,
            &theme,
            "classDiagram\nclass Animal\nstyle Animal stroke-width:4px\n",
        )
        .unwrap()
        .0;
        assert!(wide.contains("stroke-width=\"4\""), "{wide}");
        let mut bad = Theme::mermaid_default();
        bad.primary_color = "nope".into();
        assert!(try_draw(&graph, &bad, "classDiagram\nclass Animal\n").is_none());
    }

    #[test]
    fn a_dark_page_uses_the_theme_on_the_boxes() {
        let mut theme = Theme::mermaid_default();
        theme.background = "#121212".into();
        theme.primary_color = "#1e1e1e".into();
        theme.secondary_color = "#1e1e1e".into();
        theme.tertiary_color = "#121212".into();
        theme.primary_border_color = "#e68e0d".into();
        theme.line_color = "#e68e0d".into();
        theme.primary_text_color = "#bebebe".into();
        theme.text_color = "#bebebe".into();
        theme.cluster_background = "#121212".into();
        theme.cluster_border = "#555555".into();
        theme.sequence_note_fill = "#2a2a2a".into();
        theme.sequence_note_border = "#555555".into();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Class;
        let svg = try_draw(
            &graph,
            &theme,
            "classDiagram\nnamespace Shell {\n  class Bar\n}\nclass Theme\n",
        )
        .unwrap()
        .0;
        assert!(
            svg.contains("Bar")
                && svg.contains("Theme")
                && svg.contains("Shell")
                && svg.contains("#1E1E1E")
                && svg.contains("#E68E0D")
                && svg.contains("#BEBEBE")
                && svg.contains("#555555")
                && !svg.contains("#FEFECE")
                && !svg.contains("#ECECFF")
                && !svg.contains("#FFFFDE")
                && !svg.contains("#EEEEEE"),
            "{svg}"
        );
    }
}
