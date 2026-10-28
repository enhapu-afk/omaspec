//! Mermaid timelines on Omagram's timeline drawer.
//!
//! mmdr has parsed the diagram. The drawer reads the same source again
//! ([`from_mermaid`]) for the sections, the periods and the events. Section
//! colors are mixed from the theme's primary, secondary and tertiary colors
//! and the accent. A construct it refuses, or a theme color it cannot read,
//! leaves the diagram to mmdr.

use mermaid_rs_renderer::{DiagramKind, Graph, Theme};
use omagram_uml::Color;
use omagram_uml::map::Map;
use omagram_uml::timeline::{Diagram, Look, draw, from_mermaid, mermaid_refusal};

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a period is not a message to jump to.
pub fn try_draw(graph: &Graph, theme: &Theme, source: &str) -> Option<(String, Map)> {
    if graph.kind != DiagramKind::Timeline {
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

/// Type, ink, page and the section colors come from the theme. A color
/// this drawer actually uses, or a font it cannot name, leaves the diagram
/// to mmdr.
fn paint(diagram: &mut Diagram, theme: &Theme) -> Option<()> {
    if !(4.0..=100.0).contains(&theme.font_size) {
        return None;
    }
    let background = match omagram_uml::parse_color(&theme.background) {
        Ok(paint) => paint,
        Err(_) => return None,
    };
    diagram.look = Look {
        font_name: font_name(&theme.font_family)?,
        label_size: theme.font_size,
        title_size: theme.font_size,
        ink: solid(&theme.text_color)?,
        background,
        sections: marks(theme)?,
    };
    Some(())
}

/// Twelve section colors. Each slot has its own share of the accent, so
/// neighbors stay apart even when primary and secondary are the same surface.
fn marks(theme: &Theme) -> Option<[Color; 12]> {
    let primary = solid(&theme.primary_color)?;
    let secondary = solid(&theme.secondary_color)?;
    let tertiary = solid(&theme.tertiary_color)?;
    let accent = solid(&theme.primary_border_color).or_else(|| solid(&theme.line_color))?;
    let bases = [primary, secondary, tertiary];
    let mut sections = [accent; 12];
    for (index, slot) in sections.iter_mut().enumerate() {
        let share = 0.12 + index as f32 * (0.76 / 11.0);
        *slot = bases[index % 3].mix(accent, share);
    }
    Some(sections)
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

    #[test]
    fn draws_a_timeline() {
        let mut theme = Theme::mermaid_default();
        theme.text_color = "#112233".into();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Timeline;
        let source = "timeline\ntitle History\nsection Early\n2002 : LinkedIn\n";
        let (svg, map) = try_draw(&graph, &theme, source).unwrap();
        let sections = marks(&theme).unwrap();
        assert!(
            svg.contains("History")
                && svg.contains("Early")
                && svg.contains("2002")
                && svg.contains("LinkedIn")
                && svg.contains("#112233")
                && svg.contains(&sections[0].to_string())
                && !svg.contains("#B9B9FF"),
            "{svg}"
        );
        assert_eq!(map, Map::default());
    }

    #[test]
    fn leaves_another_kind_and_draws_down_the_page() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        assert!(try_draw(&graph, &theme, "timeline\n2002 : LinkedIn\n").is_none());
        graph.kind = DiagramKind::Timeline;
        let down = try_draw(&graph, &theme, "timeline TD\n2002 : LinkedIn\n")
            .unwrap()
            .0;
        assert!(down.contains("LinkedIn"), "{down}");
        assert!(named_decline("timeline TD\n2002 : LinkedIn\n").is_none());
        let mut bad = Theme::mermaid_default();
        bad.background = "nope".into();
        assert!(try_draw(&graph, &bad, "timeline\n2002 : LinkedIn\n").is_none());
        bad.background = "transparent".into();
        bad.text_color = "nope".into();
        assert!(try_draw(&graph, &bad, "timeline\n2002 : LinkedIn\n").is_none());
        bad.text_color = "#333".into();
        bad.font_family = "<bad>".into();
        assert!(try_draw(&graph, &bad, "timeline\n2002 : LinkedIn\n").is_none());
        bad.font_family = Theme::mermaid_default().font_family;
        bad.font_size = 2.0;
        assert!(try_draw(&graph, &bad, "timeline\n2002 : LinkedIn\n").is_none());
        let mut primary = Theme::mermaid_default();
        primary.primary_color = "nope".into();
        assert!(try_draw(&graph, &primary, "timeline\n2002 : LinkedIn\n").is_none());
        primary.primary_color = Theme::mermaid_default().primary_color;
        primary.secondary_color = "nope".into();
        assert!(try_draw(&graph, &primary, "timeline\n2002 : LinkedIn\n").is_none());
        primary.secondary_color = Theme::mermaid_default().secondary_color;
        primary.primary_border_color = "nope".into();
        assert!(try_draw(&graph, &primary, "timeline\n2002 : LinkedIn\n").is_some());
        primary.line_color = "nope".into();
        assert!(try_draw(&graph, &primary, "timeline\n2002 : LinkedIn\n").is_none());
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
        let sections = marks(&theme).unwrap();
        assert!(sections[0].is_dark());
        assert_unique(&sections);
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Timeline;
        let svg = try_draw(&graph, &theme, "timeline\nsection Go\n2002 : LinkedIn\n")
            .unwrap()
            .0;
        assert!(
            svg.contains(&sections[0].to_string())
                && svg.contains("#FFFFFF")
                && svg.contains("#BEBEBE")
                && !svg.contains("#B9B9FF"),
            "{svg}"
        );
        let plain = marks(&Theme::mermaid_default()).unwrap();
        assert_ne!(plain[0], sections[0]);
        assert_unique(&plain);
    }

    fn assert_unique(colors: &[Color]) {
        let mut names: Vec<String> = colors.iter().map(ToString::to_string).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), colors.len(), "{colors:?}");
    }
}
