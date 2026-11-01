//! Mermaid user journeys on Omaspec's journey drawer.
//!
//! mmdr has parsed the diagram. The drawer reads the same source again
//! ([`from_mermaid`]) for the sections, the tasks and the actors. Section
//! bands and actor dots are mixed from the theme's primary, secondary and
//! tertiary colors and the accent. The face stays the cream journey mark.
//! A construct it refuses, or a theme color it cannot read, leaves the
//! diagram to mmdr.

use super::font_name;
use mermaid_rs_renderer::{DiagramKind, Graph, Theme};
use omaspec_uml::Color;
use omaspec_uml::journey::{Diagram, Look, draw, from_mermaid};
use omaspec_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a task is not a message to jump to.
pub fn try_draw(graph: &Graph, theme: &Theme, source: &str) -> Option<(String, Map)> {
    if graph.kind != DiagramKind::Journey {
        return None;
    }
    let mut diagram = from_mermaid(source)?;
    paint(&mut diagram, theme)?;
    draw(&diagram).ok().map(|svg| (svg, Map::default()))
}

/// Type, ink, page and the two color cycles come from the theme. A color
/// this drawer actually uses, or a font it cannot name, leaves the diagram
/// to mmdr.
fn paint(diagram: &mut Diagram, theme: &Theme) -> Option<()> {
    if !(4.0..=100.0).contains(&theme.font_size) {
        return None;
    }
    let background = match omaspec_uml::parse_color(&theme.background) {
        Ok(paint) => paint,
        Err(_) => return None,
    };
    let (sections, actors) = marks(theme)?;
    diagram.look = Look {
        font_name: font_name(&theme.font_family)?,
        label_size: theme.font_size,
        title_size: theme.font_size,
        ink: solid(&theme.text_color)?,
        background,
        sections,
        actors,
    };
    Some(())
}

/// Section bands, then actor dots. The accent is the primary border, or
/// the line color when that border is not a color. Neighbors stay apart
/// even when primary and secondary are the same surface.
fn marks(theme: &Theme) -> Option<([Color; 7], [Color; 6])> {
    let primary = solid(&theme.primary_color)?;
    let secondary = solid(&theme.secondary_color)?;
    let tertiary = solid(&theme.tertiary_color)?;
    let accent = solid(&theme.primary_border_color).or_else(|| solid(&theme.line_color))?;
    let sections = [
        primary.mix(accent, 0.15),
        secondary.mix(accent, 0.40),
        tertiary.mix(accent, 0.25),
        primary.mix(accent, 0.62),
        secondary.mix(accent, 0.78),
        tertiary.mix(accent, 0.55),
        primary.mix(secondary, 0.5).mix(accent, 0.3),
    ];
    let actors = [
        accent,
        accent.mix(primary, 0.35),
        accent.mix(secondary, 0.2),
        accent.mix(Color::WHITE, 0.35),
        accent.mix(tertiary, 0.45),
        accent.mix(Color::BLACK, 0.25),
    ];
    Some((sections, actors))
}

fn solid(value: &str) -> Option<Color> {
    omaspec_uml::parse_color(value).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_the_fixture() {
        let mut theme = Theme::mermaid_default();
        theme.text_color = "#112233".into();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Journey;
        let source = include_str!("../../../omaspec-core/tests/fixtures/journey.mmd");
        let (svg, map) = try_draw(&graph, &theme, source).unwrap();
        let (sections, actors) = marks(&theme).unwrap();
        assert!(
            svg.contains("First boot")
                && svg.contains("Install")
                && svg.contains("Partition disk")
                && svg.contains("User")
                && svg.contains("#112233")
                && svg.contains(&sections[0].to_string())
                && svg.contains(&actors[0].to_string())
                && !svg.contains("#191970"),
            "{svg}"
        );
        assert_eq!(map, Map::default());
    }

    #[test]
    fn leaves_another_kind_and_a_bad_score() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        assert!(try_draw(&graph, &theme, "journey\nNap: 5: Me\n").is_none());
        graph.kind = DiagramKind::Journey;
        let zero = try_draw(&graph, &theme, "journey\nNap: 0: Me\n").unwrap().0;
        assert!(zero.contains("Nap"), "{zero}");
        assert!(crate::mermaid::refused(DiagramKind::Journey, "journey\nNap: 0: Me\n").is_none());
        let six = try_draw(&graph, &theme, "journey\nNap: 6: Me\n").unwrap().0;
        assert!(six.contains("Nap"), "{six}");
        assert_eq!(
            crate::mermaid::refused(DiagramKind::Journey, "journey\nNap: nope: Me\n")
                .map(|(_, reason)| reason),
            Some("a score is not drawn by Omaspec; mmdr draws it".into())
        );
        let mut bad = Theme::mermaid_default();
        bad.background = "nope".into();
        assert!(try_draw(&graph, &bad, "journey\nNap: 5: Me\n").is_none());
        bad.background = "transparent".into();
        bad.text_color = "nope".into();
        assert!(try_draw(&graph, &bad, "journey\nNap: 5: Me\n").is_none());
        bad.text_color = "#333".into();
        bad.font_family = "<bad>".into();
        assert!(try_draw(&graph, &bad, "journey\nNap: 5: Me\n").is_none());
        bad.font_family = Theme::mermaid_default().font_family;
        bad.font_size = 2.0;
        assert!(try_draw(&graph, &bad, "journey\nNap: 5: Me\n").is_none());
        let mut primary = Theme::mermaid_default();
        primary.primary_color = "nope".into();
        assert!(try_draw(&graph, &primary, "journey\nNap: 5: Me\n").is_none());
        primary.primary_color = Theme::mermaid_default().primary_color;
        primary.secondary_color = "nope".into();
        assert!(try_draw(&graph, &primary, "journey\nNap: 5: Me\n").is_none());
        primary.secondary_color = Theme::mermaid_default().secondary_color;
        primary.primary_border_color = "nope".into();
        assert!(try_draw(&graph, &primary, "journey\nNap: 5: Me\n").is_some());
        primary.line_color = "nope".into();
        assert!(try_draw(&graph, &primary, "journey\nNap: 5: Me\n").is_none());
    }

    #[test]
    fn marks_follow_a_dark_theme_and_stay_apart() {
        let mut theme = Theme::mermaid_default();
        theme.background = "#121212".into();
        theme.primary_color = "#1e1e1e".into();
        theme.secondary_color = "#1e1e1e".into();
        theme.tertiary_color = "#121212".into();
        theme.primary_border_color = "#e68e0d".into();
        theme.line_color = "#e68e0d".into();
        theme.text_color = "#bebebe".into();
        let (sections, actors) = marks(&theme).unwrap();
        assert!(sections.iter().all(|color| color.is_dark()));
        assert_unique(&sections);
        assert_unique(&actors);
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Journey;
        let svg = try_draw(&graph, &theme, "journey\nsection Go\nNap: 5: Me\n")
            .unwrap()
            .0;
        assert!(
            svg.contains(&sections[0].to_string())
                && svg.contains(&actors[0].to_string())
                && svg.contains("#FFFFFF")
                && svg.contains("#BEBEBE")
                && !svg.contains("#191970"),
            "{svg}"
        );
        let plain = marks(&Theme::mermaid_default()).unwrap();
        assert_ne!(plain.0[0], sections[0]);
        assert_unique(&plain.0);
        assert_unique(&plain.1);
    }

    fn assert_unique(colors: &[Color]) {
        let mut names: Vec<String> = colors.iter().map(ToString::to_string).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), colors.len(), "{colors:?}");
    }
}
