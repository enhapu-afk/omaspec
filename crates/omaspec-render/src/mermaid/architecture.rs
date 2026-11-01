//! Mermaid architecture diagrams on Omaspec's architecture drawer.
//!
//! mmdr has parsed the diagram. The drawer reads the same source again
//! ([`from_mermaid`]) for the services, groups and ports, and paints them
//! in the Mermaid theme. On the stock theme a service keeps the familiar
//! architecture blue. On any other theme the tile is the accent and a
//! label uses the theme text when that still reads. A construct it
//! refuses, or a theme color it cannot read, leaves the diagram to mmdr.

use super::font_name;
use mermaid_rs_renderer::{DiagramKind, Graph, Theme};
use omaspec_uml::Color;
use omaspec_uml::architecture::{self, Look, draw, from_mermaid};
use omaspec_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a service is not a message to jump to.
pub fn try_draw(graph: &Graph, theme: &Theme, source: &str) -> Option<(String, Map)> {
    if graph.kind != DiagramKind::Architecture {
        return None;
    }
    let mut diagram = from_mermaid(source)?;
    paint(&mut diagram, theme)?;
    draw(&diagram).ok().map(|svg| (svg, Map::default()))
}

/// The stock theme keeps the architecture blue. Any other theme uses the
/// accent for the tile and the group stroke.
fn paint(diagram: &mut architecture::Diagram, theme: &Theme) -> Option<()> {
    if !(4.0..=100.0).contains(&theme.font_size) {
        return None;
    }
    let background = match omaspec_uml::parse_color(&theme.background) {
        Ok(paint) => paint,
        Err(_) => return None,
    };
    let stock = Theme::mermaid_default();
    let classic = same(&theme.primary_color, &stock.primary_color)
        && same(&theme.secondary_color, &stock.secondary_color)
        && same(&theme.tertiary_color, &stock.tertiary_color)
        && same(&theme.primary_border_color, &stock.primary_border_color)
        && same(&theme.line_color, &stock.line_color)
        && same(&theme.background, &stock.background)
        && same(&theme.primary_text_color, &stock.primary_text_color);
    let accent = solid(&theme.primary_border_color)?;
    solid(&theme.primary_color)?;
    solid(&theme.secondary_color)?;
    solid(&theme.tertiary_color)?;
    diagram.look = Look {
        font_name: font_name(&theme.font_family)?,
        font_size: theme.font_size,
        preferred: solid(&theme.primary_text_color)?,
        background,
        accent,
        line: if classic {
            solid(&theme.line_color)?
        } else {
            accent
        },
        classic,
    };
    Some(())
}

fn same(left: &str, right: &str) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn solid(value: &str) -> Option<Color> {
    omaspec_uml::parse_color(value).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
architecture-beta
    service web(server)[Web]
    service db(database)[DB]
    web:R --> L:db
";

    #[test]
    fn draws_the_classic_blue_on_the_stock_theme() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Architecture;
        let (svg, map) = try_draw(&graph, &theme, SAMPLE).unwrap();
        assert!(
            svg.contains("Web")
                && svg.contains("DB")
                && svg.contains("#087EBF")
                && svg.contains("#2F3B4D")
                && svg.contains("#333333")
                && !svg.contains("#ECECFF"),
            "{svg}"
        );
        assert_eq!(map, Map::default());
    }

    #[test]
    fn leaves_another_kind_an_icon_pack_and_a_bad_theme() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        assert!(try_draw(&graph, &theme, SAMPLE).is_none());
        graph.kind = DiagramKind::Architecture;
        assert_eq!(
            crate::mermaid::refused(
                DiagramKind::Architecture,
                "architecture-beta\n  service db(logos:aws-aurora)[DB]\n"
            )
            .map(|(_, reason)| reason),
            Some("an icon is not drawn by Omaspec; mmdr draws it".into())
        );
        assert!(
            try_draw(
                &graph,
                &theme,
                "architecture-beta\n  service db(logos:aws-aurora)[DB]\n"
            )
            .is_none()
        );
        let mut bad = Theme::mermaid_default();
        bad.primary_color = "nope".into();
        assert!(try_draw(&graph, &bad, SAMPLE).is_none());
    }

    #[test]
    fn a_dark_page_uses_the_accent() {
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
        graph.kind = DiagramKind::Architecture;
        let svg = try_draw(&graph, &theme, SAMPLE).unwrap().0;
        assert!(
            svg.contains("Web")
                && svg.contains("#E68E0D")
                && svg.contains("#BEBEBE")
                && svg.contains("#121212")
                && !svg.contains("#087EBF")
                && !svg.contains("#ECECFF"),
            "{svg}"
        );
    }
}
