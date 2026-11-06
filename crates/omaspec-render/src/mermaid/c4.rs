//! Mermaid C4 diagrams on Omaspec's C4 drawer.
//!
//! The drawer reads the source itself, not mmdr
//! ([`from_mermaid`]) for the people, boxes, cylinders and relationships,
//! and paints them in the Mermaid theme. On the stock theme the boxes keep
//! the classic C4 colors. On any other theme a person, a system, a
//! container and a component are steps of the accent toward the page, and
//! a label uses the theme text when that still reads. A construct it
//! refuses, or a theme color it cannot read, leaves the diagram to mmdr.

use super::font_name;
use mermaid_rs_renderer::Theme;
use omaspec_uml::Color;
use omaspec_uml::c4::{self, Look, draw, from_mermaid};
use omaspec_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a box is not a message to jump to.
pub fn try_draw(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let mut diagram = from_mermaid(source)?;
    paint(&mut diagram, theme)?;
    diagram.look.sketch = super::head::hand_drawn(source);
    draw(&diagram).ok().map(|svg| (svg, Map::default()))
}

/// The stock theme keeps the C4 blues. Any other theme steps the accent
/// toward the page. An element's own colors still win.
fn paint(diagram: &mut c4::Diagram, theme: &Theme) -> Option<()> {
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
        sketch: false,
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
        thickness: diagram.look.thickness,
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
    use mermaid_rs_renderer::DiagramKind;

    const SAMPLE: &str = "\
C4Context
Person(admin, \"Admin\")
System(sys, \"System\")
Component(api, \"API\")
Rel(admin, sys, \"Uses\")
";

    #[test]
    fn draws_the_classic_colors_and_dark_text_on_a_light_box() {
        let theme = Theme::mermaid_default();
        let (svg, map) = try_draw(&theme, SAMPLE).unwrap();
        assert!(
            svg.contains("Admin")
                && svg.contains("Uses")
                && svg.contains("#08427B")
                && svg.contains("#1168BD")
                && svg.contains("#85BBF0")
                && svg.contains("#333333")
                && !svg.contains("#ECECFF"),
            "{svg}"
        );
        assert_eq!(map, Map::default());
    }

    #[test]
    fn leaves_a_sprite_and_a_bad_theme() {
        let theme = Theme::mermaid_default();
        assert_eq!(
            crate::mermaid::refused(
                DiagramKind::C4,
                "C4Context\nPerson(admin, \"Admin\", $sprite=users)\n"
            )
            .map(|(_, reason)| reason),
            Some("a sprite is not drawn by Omaspec; mmdr draws it".into())
        );
        let mut bad = Theme::mermaid_default();
        bad.primary_color = "nope".into();
        assert!(try_draw(&bad, SAMPLE).is_none());
        let painted = try_draw(
            &theme,
            "C4Context\nPerson(admin, \"Admin\")\nUpdateElementStyle(admin, #f9f)\n",
        )
        .unwrap()
        .0;
        assert!(
            painted.contains("#FF99FF") && painted.contains("Admin"),
            "{painted}"
        );
    }

    #[test]
    fn a_dark_page_uses_the_accent_for_each_kind() {
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
            svg.contains("Admin")
                && svg.contains("#E68E0D")
                && svg.contains("#AB6B0E")
                && svg.contains("#563A10")
                && svg.contains("#BEBEBE")
                && !svg.contains("#08427B")
                && !svg.contains("#1168BD")
                && !svg.contains("#438DD5")
                && !svg.contains("#85BBF0")
                && !svg.contains("#ECECFF"),
            "{svg}"
        );
    }
}
