//! Mermaid ER diagrams on Omaspec's ER drawer.
//!
//! mmdr has parsed the diagram. The drawer reads the same source again
//! ([`from_mermaid`]) for the entities, the attributes and the crow's feet,
//! and paints them in the Mermaid theme. The entity fill is the primary
//! color, the border is the accent, and a label uses the theme text when
//! that still reads on its fill. A construct it refuses, or a theme color
//! it cannot read, leaves the diagram to mmdr.

use mermaid_rs_renderer::{DiagramKind, Graph, Theme};
use omaspec_uml::Color;
use omaspec_uml::er::{self, Look, draw, from_mermaid, mermaid_refusal};
use omaspec_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: an entity is not a message to jump to.
pub fn try_draw(graph: &Graph, theme: &Theme, source: &str) -> Option<(String, Map)> {
    if graph.kind != DiagramKind::Er {
        return None;
    }
    let mut diagram = from_mermaid(source)?;
    paint(&mut diagram, theme)?;
    draw(&diagram).ok().map(|svg| (svg, Map::default()))
}

/// The first construct the engine refuses by name, with its line and why.
pub(super) fn named_decline(source: &str) -> Option<(usize, String)> {
    mermaid_refusal(source).map(|(line, what)| {
        (
            line,
            format!("{what} is not drawn by Omaspec; mmdr draws it"),
        )
    })
}

/// Fill, border, line and text come from the theme. An entity's own
/// `style` still wins, because the reader stored it on the entity.
fn paint(diagram: &mut er::Diagram, theme: &Theme) -> Option<()> {
    if !(4.0..=100.0).contains(&theme.font_size) {
        return None;
    }
    let background = match omaspec_uml::parse_color(&theme.background) {
        Ok(paint) => paint,
        Err(_) => return None,
    };
    diagram.look = Look {
        font_name: font_name(&theme.font_family)?,
        font_size: theme.font_size,
        preferred: solid(&theme.primary_text_color)?,
        fill: solid(&theme.primary_color)?,
        border: solid(&theme.primary_border_color)?,
        line: solid(&theme.line_color)?,
        background,
        thickness: diagram.look.thickness,
    };
    Some(())
}

fn solid(value: &str) -> Option<Color> {
    omaspec_uml::parse_color(value).ok().flatten()
}

fn font_name(family: &str) -> Option<String> {
    let names: Vec<&str> = family
        .split(',')
        .map(|name| name.trim().trim_matches(['"', '\'']).trim())
        .filter(|name| !name.is_empty())
        .collect();
    if names.is_empty() || names.iter().any(|name| name.contains(['<', '>', '"', '&'])) {
        return None;
    }
    Some(names.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_the_entities_in_the_default_colors() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Er;
        let source = include_str!("../../../omaspec-core/tests/fixtures/er.mmd");
        let (svg, map) = try_draw(&graph, &theme, source).unwrap();
        assert!(
            svg.contains("PACKAGE")
                && svg.contains("FILE")
                && svg.contains("contains")
                && svg.contains("#ECECFF"),
            "{svg}"
        );
        assert_eq!(map, Map::default());
    }

    #[test]
    fn leaves_another_kind_a_style_and_a_bad_theme() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        assert!(try_draw(&graph, &theme, "erDiagram\n  A ||--|| B : x\n").is_none());
        graph.kind = DiagramKind::Er;
        assert_eq!(
            named_decline("erDiagram\n  style A opacity:1\n").map(|(_, reason)| reason),
            Some("a style is not drawn by Omaspec; mmdr draws it".into())
        );
        let mut bad = Theme::mermaid_default();
        bad.primary_color = "nope".into();
        assert!(try_draw(&graph, &bad, "erDiagram\n  A ||--|| B : x\n").is_none());
        let painted = try_draw(
            &graph,
            &theme,
            "erDiagram\n  A ||--|| B : x\n  style A fill:#f9f\n",
        )
        .unwrap()
        .0;
        assert!(
            painted.contains("#FF99FF") && painted.contains("A"),
            "{painted}"
        );
    }

    #[test]
    fn a_dark_page_uses_the_theme_on_the_entities() {
        let mut theme = Theme::mermaid_default();
        theme.background = "#121212".into();
        theme.primary_color = "#1e1e1e".into();
        theme.secondary_color = "#1e1e1e".into();
        theme.tertiary_color = "#121212".into();
        theme.primary_border_color = "#e68e0d".into();
        theme.line_color = "#e68e0d".into();
        theme.primary_text_color = "#bebebe".into();
        theme.text_color = "#bebebe".into();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Er;
        let svg = try_draw(&graph, &theme, "erDiagram\n  USER ||--o{ ORDER : places\n")
            .unwrap()
            .0;
        assert!(
            svg.contains("USER")
                && svg.contains("ORDER")
                && svg.contains("places")
                && svg.contains("#1E1E1E")
                && svg.contains("#E68E0D")
                && svg.contains("#BEBEBE")
                && !svg.contains("#ECECFF")
                && !svg.contains("#FFFFDE")
                && !svg.contains("#FEFECE")
                && !svg.contains("#EEEEEE"),
            "{svg}"
        );
    }
}
