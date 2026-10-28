//! Mermaid block diagrams on Omagram's block drawer.
//!
//! mmdr has parsed the diagram. The drawer reads the same source again
//! ([`from_mermaid`]) for the grid, the nested frames and the edges, and
//! paints them in the Mermaid theme. On the stock theme a block keeps the
//! familiar lilac tile. On any other theme the tile mixes the accent onto
//! the page and a label uses the theme text when that still reads. A
//! construct it refuses, or a theme color it cannot read, leaves the
//! diagram to mmdr.

use mermaid_rs_renderer::{DiagramKind, Graph, Theme};
use omagram_uml::Color;
use omagram_uml::block::{self, Look, draw, from_mermaid, mermaid_refusal};
use omagram_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a block is not a message to jump to.
pub fn try_draw(graph: &Graph, theme: &Theme, source: &str) -> Option<(String, Map)> {
    if graph.kind != DiagramKind::Block {
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
            format!("{what} is not drawn by Omagram; mmdr draws it"),
        )
    })
}

/// The stock theme keeps the lilac tile. Any other theme uses the accent
/// for the stroke and mixes it into the fill.
fn paint(diagram: &mut block::Diagram, theme: &Theme) -> Option<()> {
    if !(4.0..=100.0).contains(&theme.font_size) {
        return None;
    }
    let background = match omagram_uml::parse_color(&theme.background) {
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
    omagram_uml::parse_color(value).ok().flatten()
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

    const SAMPLE: &str = "\
block-beta
  a[\"Web\"] b[\"DB\"]
  a --> b
";

    #[test]
    fn draws_the_classic_lilac_on_the_stock_theme() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Block;
        let (svg, map) = try_draw(&graph, &theme, SAMPLE).unwrap();
        assert!(
            svg.contains("Web")
                && svg.contains("DB")
                && svg.contains("#ECECFF")
                && svg.contains("#7B88A8")
                && svg.contains("#333333")
                && svg.contains("#2F3B4D"),
            "{svg}"
        );
        assert_eq!(map, Map::default());
    }

    #[test]
    fn leaves_another_kind_a_class_and_a_bad_theme() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        assert!(try_draw(&graph, &theme, SAMPLE).is_none());
        graph.kind = DiagramKind::Block;
        assert_eq!(
            named_decline("block-beta\nclassDef a fill:#fff\n").map(|(_, reason)| reason),
            Some("a class is not drawn by Omagram; mmdr draws it".into())
        );
        assert!(try_draw(&graph, &theme, "block-beta\nclassDef a fill:#fff\n").is_none());
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
        graph.kind = DiagramKind::Block;
        let svg = try_draw(&graph, &theme, SAMPLE).unwrap().0;
        assert!(
            svg.contains("Web")
                && svg.contains("#E68E0D")
                && svg.contains("#BEBEBE")
                && svg.contains("#121212")
                && !svg.contains("#ECECFF"),
            "{svg}"
        );
    }
}
