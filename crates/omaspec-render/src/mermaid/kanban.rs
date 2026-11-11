//! Mermaid kanban boards on Omaspec's kanban drawer.
//!
//! The drawer reads the source itself, not mmdr ([`from_mermaid`]): the
//! columns and the cards under them. A column is a quiet panel between
//! the page and the ink, a card has the theme's primary color and its
//! border. A construct the reader refuses, or a theme color it cannot
//! read, leaves the board to mmdr.

use super::font_name;
use mermaid_rs_renderer::Theme;
use omaspec_uml::Color;
use omaspec_uml::kanban::{Diagram, Look, draw, from_mermaid};
use omaspec_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the board to mmdr.
/// The map stays empty: a card is not a box to fold or to hold.
pub fn try_draw(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let mut diagram = from_mermaid(source)?;
    paint(&mut diagram, theme)?;
    diagram.look.sketch = super::head::hand_drawn(source);
    draw(&diagram).ok().map(|svg| (svg, Map::default()))
}

/// Type, ink, page and the colors of a column and a card come from the
/// theme; the four priorities keep their colors, moved towards the ink
/// until each reads on the card.
fn paint(diagram: &mut Diagram, theme: &Theme) -> Option<()> {
    if !(4.0..=100.0).contains(&theme.font_size) {
        return None;
    }
    let background = omaspec_uml::parse_color(&theme.background).ok()?;
    let paper = background.unwrap_or(Color::WHITE);
    let ink = solid(&theme.text_color)?;
    let card = solid(&theme.primary_color)?;
    let border = solid(&theme.primary_border_color).or_else(|| solid(&theme.line_color))?;
    let stock = Look::default();
    diagram.look = Look {
        font_name: font_name(&theme.font_family)?,
        sketch: false,
        label_size: theme.font_size,
        title_size: theme.font_size * 1.25,
        ink,
        background,
        column: paper.mix(ink, 0.09),
        card,
        border,
        priorities: stock
            .priorities
            .map(|color| color.legible_on(card, ink, 1.8)),
    };
    Some(())
}

fn solid(value: &str) -> Option<Color> {
    omaspec_uml::parse_color(value).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mermaid_rs_renderer::DiagramKind;

    const BOARD: &str = "kanban\n  Todo\n    [Write the guide]\n  [Doing]\n    a[Draw]@{ assigned: 'kim', priority: 'High' }\n";

    #[test]
    fn draws_a_board_in_the_theme_s_colors() {
        let mut theme = Theme::mermaid_default();
        theme.primary_color = "#102030".into();
        theme.primary_border_color = "#405060".into();
        let (svg, map) = try_draw(&theme, BOARD).unwrap();
        assert!(
            svg.contains(">Todo<")
                && svg.contains(">Doing<")
                && svg.contains(">Write the guide<")
                && svg.contains(">kim<")
                && svg.contains("#102030")
                && svg.contains("#405060")
                // On a dark card the words are light.
                && svg.contains("#FFFFFF"),
            "{svg}"
        );
        assert_eq!(map, Map::default());
    }

    #[test]
    fn leaves_what_it_does_not_draw_to_mmdr() {
        let theme = Theme::mermaid_default();
        let odd = "kanban\n  Todo\n    a[x]@{ priority: 'Urgent' }\n";
        assert!(try_draw(&theme, odd).is_none());
        assert_eq!(
            crate::mermaid::refused(DiagramKind::Kanban, odd),
            Some((
                2,
                "a priority is not drawn by Omaspec; mmdr draws it".to_owned()
            ))
        );
        assert!(crate::mermaid::refused(DiagramKind::Kanban, BOARD).is_none());
        let mut bad = Theme::mermaid_default();
        bad.primary_color = "nope".into();
        assert!(try_draw(&bad, BOARD).is_none());
        bad.primary_color = "#fff".into();
        bad.font_family = "<bad>".into();
        assert!(try_draw(&bad, BOARD).is_none());
    }
}
