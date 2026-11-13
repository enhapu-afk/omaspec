//! Mermaid block diagrams on Omaspec's block drawer.
//!
//! The drawer reads the source itself, not mmdr
//! ([`from_mermaid`]) for the grid, the nested frames and the edges, and
//! paints them in the Mermaid theme. On the stock theme a block keeps the
//! familiar lilac tile. On any other theme the tile mixes the accent onto
//! the page and a label uses the theme text when that still reads. A
//! construct it refuses, or a theme color it cannot read, leaves the
//! diagram to mmdr.

use mermaid_rs_renderer::Theme;
use omaspec_uml::block::{self, Look, draw, from_mermaid};
use omaspec_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a block is not a message to jump to.
pub fn try_draw(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let mut diagram = from_mermaid(source)?;
    paint(&mut diagram, theme)?;
    diagram.look.sketch = super::head::hand_drawn(source);
    draw(&diagram).ok().map(|svg| (svg, Map::default()))
}

/// The stock theme keeps the lilac tile. Any other theme uses the accent
/// for the stroke and mixes it into the fill.
fn paint(diagram: &mut block::Diagram, theme: &Theme) -> Option<()> {
    let themed = super::head::themed(theme)?;
    diagram.look = Look {
        font_name: themed.font_name,
        sketch: false,
        font_size: themed.font_size,
        preferred: themed.preferred,
        background: themed.background,
        accent: themed.accent,
        line: themed.line,
        classic: themed.classic,
    };
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mermaid_rs_renderer::DiagramKind;

    const SAMPLE: &str = "\
block-beta
  a[\"Web\"] b[\"DB\"]
  a --> b
";

    #[test]
    fn draws_the_classic_lilac_on_the_stock_theme() {
        let theme = Theme::mermaid_default();
        let (svg, map) = try_draw(&theme, SAMPLE).unwrap();
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
    fn leaves_a_class_and_a_bad_theme() {
        let theme = Theme::mermaid_default();
        assert_eq!(
            crate::mermaid::refused(DiagramKind::Block, "block-beta\nclassDef a fill:#fff\n")
                .map(|(_, reason)| reason),
            Some("a class is not drawn by Omaspec; mmdr draws it".into())
        );
        assert!(try_draw(&theme, "block-beta\nclassDef a fill:#fff\n").is_none());
        let mut bad = Theme::mermaid_default();
        bad.primary_color = "nope".into();
        assert!(try_draw(&bad, SAMPLE).is_none());
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
        let svg = try_draw(&theme, SAMPLE).unwrap().0;
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
