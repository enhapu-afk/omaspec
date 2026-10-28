//! Mermaid Gantt charts on the PlantUML Gantt engine.
//!
//! mmdr has parsed the diagram. The engine reads the same source again
//! ([`from_mermaid`]) for the dates, the sections and the dependencies,
//! and draws it in the colors of the Mermaid theme. On a restyled page the
//! bar picks up the accent, so a surface-colored bar does not vanish.
//! A construct the day scale cannot place leaves the diagram to mmdr.

use mermaid_rs_renderer::{DiagramKind, Graph, Theme};
use omagram_uml::Color;
use omagram_uml::gantt::{Diagram, Marks, Row, Skinparam, draw, from_mermaid, mermaid_refusal};
use omagram_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a Gantt bar is not a message to jump to.
pub fn try_draw(graph: &Graph, theme: &Theme, source: &str) -> Option<(String, Map)> {
    if graph.kind != DiagramKind::Gantt {
        return None;
    }
    let mut diagram = from_mermaid(source)?;
    diagram.frame.skinparams = skin_of(theme)?;
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
    for param in &params {
        if param.key.to_ascii_lowercase().contains("color") {
            omagram_uml::parse_color(&param.value).ok()?;
        }
    }
    Some(params)
}

/// A plain bar takes the primary color. `done` lightens it, `active`
/// darkens it, and `crit` pulls it toward red. Mermaid's `done` is that
/// color, not a completion share.
fn paint(diagram: &mut Diagram, theme: &Theme) -> Option<()> {
    let stock = Theme::mermaid_default();
    let primary = solid(&theme.primary_color)?;
    let base = if follows(theme, &stock) {
        primary.mix(accent(theme)?, 0.45)
    } else {
        primary
    };
    for row in &mut diagram.rows {
        let Row::Bar(bar) = row else { continue };
        if bar.color.is_none() {
            bar.color = Some(status_color(base, bar.marks));
        }
    }
    Some(())
}

/// The page left the Mermaid default, so a bar in the raw primary color
/// can sit on a background of almost the same color.
fn follows(theme: &Theme, stock: &Theme) -> bool {
    theme.primary_color != stock.primary_color
        || theme.background != stock.background
        || theme.primary_border_color != stock.primary_border_color
        || theme.line_color != stock.line_color
        || theme.secondary_color != stock.secondary_color
}

fn accent(theme: &Theme) -> Option<Color> {
    solid(&theme.primary_border_color).or_else(|| solid(&theme.line_color))
}

fn status_color(base: Color, marks: Marks) -> Color {
    let mut color = if marks.crit {
        base.mix(Color::hex(0xEF_4444), 0.72)
    } else {
        base
    };
    if marks.done {
        color = color.mix(Color::WHITE, 0.42);
    } else if marks.active {
        color = color.mix(Color::BLACK, 0.18);
    }
    color
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

fn plain_number(value: f32) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{}", value as i32)
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_a_gantt_in_the_theme_colors() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Gantt;
        let source = "\
gantt
dateFormat YYYY-MM-DD
section Core
Plain :2026-01-01, 2d
Done :done, 2026-01-03, 2d
Hot :crit, 2026-01-05, 1d
Ship :milestone, m, 2026-01-06, 0d
";
        let (svg, map) = try_draw(&graph, &theme, source).unwrap();
        assert!(
            svg.contains("Plain")
                && svg.contains("Done")
                && svg.contains("Hot")
                && svg.contains("Ship"),
            "{svg}"
        );
        let base = Color::hex(0xECECFF);
        assert!(svg.contains(&base.to_string()), "{svg}");
        let done = status_color(
            base,
            Marks {
                done: true,
                ..Marks::default()
            },
        );
        let hot = status_color(
            base,
            Marks {
                crit: true,
                ..Marks::default()
            },
        );
        assert_ne!(done.to_string(), base.to_string());
        assert!(svg.contains(&done.to_string()), "{svg} missing {done}");
        assert!(svg.contains(&hot.to_string()), "{svg} missing {hot}");
        assert!(svg.contains("<polygon"), "{svg}");
        assert_eq!(map, Map::default());
    }

    #[test]
    fn leaves_another_kind_and_an_excluded_day() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        assert!(try_draw(&graph, &theme, "gantt\n").is_none());
        graph.kind = DiagramKind::Gantt;
        let weekends = try_draw(
            &graph,
            &theme,
            "gantt\ndateFormat YYYY-MM-DD\nexcludes weekends\nWork :2026-01-05, 5d\n",
        )
        .unwrap()
        .0;
        assert!(weekends.contains("Work"), "{weekends}");
        assert!(named_decline("gantt\nexcludes weekends\n").is_none());
        assert_eq!(
            named_decline("gantt\nexcludes holidays\n").map(|(_, reason)| reason),
            Some("an excluded day is not drawn by Omagram; mmdr draws it".into())
        );
        let mut bad = Theme::mermaid_default();
        bad.primary_color = "nope".into();
        assert!(
            try_draw(
                &graph,
                &bad,
                "gantt\ndateFormat YYYY-MM-DD\nA :2026-01-01, 1d\n"
            )
            .is_none()
        );
    }

    #[test]
    fn a_dark_page_keeps_the_bars_readable() {
        let mut theme = Theme::mermaid_default();
        theme.background = "#121212".into();
        theme.primary_color = "#1e1e1e".into();
        theme.secondary_color = "#1e1e1e".into();
        theme.tertiary_color = "#121212".into();
        theme.primary_border_color = "#e68e0d".into();
        theme.line_color = "#e68e0d".into();
        theme.primary_text_color = "#bebebe".into();
        let base = solid("#1e1e1e")
            .unwrap()
            .mix(solid("#e68e0d").unwrap(), 0.45);
        let page = solid("#121212").unwrap();
        let ink = solid("#bebebe").unwrap();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Gantt;
        let svg = try_draw(
            &graph,
            &theme,
            "gantt\ndateFormat YYYY-MM-DD\nsection Core\nPlain :2026-01-05, 2d\n",
        )
        .unwrap()
        .0;
        assert!(
            svg.contains(&base.to_string())
                && svg.contains(&page.mix(ink, 0.1).to_string())
                && svg.contains(&page.mix(ink, 0.14).to_string())
                && svg.contains(&page.mix(ink, 0.6).to_string())
                && svg.contains("#BEBEBE")
                && !svg.contains("#EEEEEE")
                && !svg.contains("#E6E6E6")
                && !svg.contains("#181818"),
            "{svg}"
        );
    }
}
