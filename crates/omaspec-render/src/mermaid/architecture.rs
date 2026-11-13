//! Mermaid architecture diagrams on Omaspec's architecture drawer.
//!
//! The drawer reads the source itself, not mmdr
//! ([`from_mermaid`]) for the services, groups and ports, and paints them
//! in the Mermaid theme. On the stock theme a service keeps the familiar
//! architecture blue. On any other theme the tile is the accent and a
//! label uses the theme text when that still reads. A construct it
//! refuses, or a theme color it cannot read, leaves the diagram to mmdr.

use mermaid_rs_renderer::Theme;
use omaspec_uml::architecture::{self, Look, draw, from_mermaid};
use omaspec_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a service is not a message to jump to.
pub fn try_draw(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let mut diagram = from_mermaid(source)?;
    paint(&mut diagram, theme)?;
    diagram.look.sketch = super::head::hand_drawn(source);
    draw(&diagram).ok().map(|svg| (svg, Map::default()))
}

/// The stock theme keeps the architecture blue. Any other theme uses the
/// accent for the tile and the group stroke.
fn paint(diagram: &mut architecture::Diagram, theme: &Theme) -> Option<()> {
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
architecture-beta
    service web(server)[Web]
    service db(database)[DB]
    web:R --> L:db
";

    #[test]
    fn draws_the_classic_blue_on_the_stock_theme() {
        let theme = Theme::mermaid_default();
        let (svg, map) = try_draw(&theme, SAMPLE).unwrap();
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
    fn leaves_an_icon_pack_and_a_bad_theme() {
        let theme = Theme::mermaid_default();
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
                &theme,
                "architecture-beta\n  service db(logos:aws-aurora)[DB]\n"
            )
            .is_none()
        );
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
                && !svg.contains("#087EBF")
                && !svg.contains("#ECECFF"),
            "{svg}"
        );
    }
}
