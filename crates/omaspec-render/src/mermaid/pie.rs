//! Mermaid pie charts on Omaspec's pie drawer.
//!
//! mmdr has parsed the diagram. The drawer reads the same source again
//! ([`from_mermaid`]) for the slices and the title, and paints them in
//! the pie colors of the Mermaid theme. When the base colors move and no
//! `pieN` was set, the slices are mixed from primary, secondary, tertiary
//! and the accent, so a dark page does not keep the default pastels. A
//! construct it refuses, or a theme color it cannot read, leaves the
//! diagram to mmdr.

use mermaid_rs_renderer::{DiagramKind, Graph, Theme};
use omaspec_uml::Color;
use omaspec_uml::map::Map;
use omaspec_uml::pie::{Diagram, Look, draw, from_mermaid};

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a slice is not a message to jump to.
pub fn try_draw(graph: &Graph, theme: &Theme, source: &str) -> Option<(String, Map)> {
    if graph.kind != DiagramKind::Pie {
        return None;
    }
    let mut diagram = from_mermaid(source)?;
    paint(&mut diagram, theme)?;
    draw(&diagram).ok().map(|svg| (svg, Map::default()))
}

/// Slice fills take `pie1` onward, mixed toward the background by
/// `1 - pieOpacity`. A transparent background has nothing behind it, so
/// that mix uses white. When the base colors moved and a slot is still
/// the default pastel, that slot is mixed from the theme instead. Text
/// and strokes stay opaque.
fn paint(diagram: &mut Diagram, theme: &Theme) -> Option<()> {
    if !metrics_ok(theme) {
        return None;
    }
    let background = match omaspec_uml::parse_color(&theme.background) {
        Ok(paint) => paint,
        Err(_) => return None,
    };
    let paper = background.unwrap_or(Color::WHITE);
    let share = 1.0 - theme.pie_opacity;
    let stock = Theme::mermaid_default();
    let restyled = restyled(theme, &stock);
    let needs_marks = restyled
        && diagram.slices.iter().enumerate().any(|(index, _)| {
            let slot = index % theme.pie_colors.len();
            theme.pie_colors[slot] == stock.pie_colors[slot]
        });
    let derived = if needs_marks {
        Some(marks(theme)?)
    } else {
        None
    };
    for (index, slice) in diagram.slices.iter_mut().enumerate() {
        slice.color = Some(slice_fill(
            theme,
            &stock,
            derived.as_ref(),
            index,
            paper,
            share,
        )?);
    }
    let positive = diagram
        .slices
        .iter()
        .filter(|slice| slice.value > 0.0)
        .count();
    let title_ink = if diagram.title.is_some() {
        ink(
            &theme.pie_title_text_color,
            &stock.pie_title_text_color,
            &theme.text_color,
        )?
    } else {
        Color::BLACK
    };
    let legend_ink = if !diagram.slices.is_empty() {
        ink(
            &theme.pie_legend_text_color,
            &stock.pie_legend_text_color,
            &theme.text_color,
        )?
    } else {
        Color::BLACK
    };
    let accent = solid(&theme.primary_border_color).or_else(|| solid(&theme.line_color));
    if theme.pie_outer_stroke_width > 0.0 {
        line_ink(
            &theme.pie_outer_stroke_color,
            &stock.pie_outer_stroke_color,
            restyled,
            accent,
        )?;
    }
    // Two positive slices meet on a line. One slice is the whole circle.
    if theme.pie_stroke_width > 0.0 && positive >= 2 {
        line_ink(
            &theme.pie_stroke_color,
            &stock.pie_stroke_color,
            restyled,
            accent,
        )?;
    }
    diagram.look = Look {
        font_name: font_name(&theme.font_family)?,
        label_size: theme.pie_legend_text_size,
        title_size: theme.pie_title_text_size,
        ink: legend_ink,
        title_ink,
        background,
        fill_stroke: line_ink(
            &theme.pie_stroke_color,
            &stock.pie_stroke_color,
            restyled,
            accent,
        )
        .unwrap_or(Color::BLACK),
        ring: line_ink(
            &theme.pie_outer_stroke_color,
            &stock.pie_outer_stroke_color,
            restyled,
            accent,
        )
        .unwrap_or(Color::BLACK),
        stroke_width: theme.pie_stroke_width,
        ring_width: theme.pie_outer_stroke_width,
    };
    Some(())
}

/// The base colors moved, and the pie palette was not given its own `pieN`.
fn restyled(theme: &Theme, stock: &Theme) -> bool {
    theme.primary_color != stock.primary_color
        || theme.secondary_color != stock.secondary_color
        || theme.tertiary_color != stock.tertiary_color
        || theme.primary_border_color != stock.primary_border_color
        || theme.line_color != stock.line_color
}

/// Twelve slice colors. Each slot has its own share of the accent, so
/// neighbors stay apart even when primary and secondary are the same surface.
fn marks(theme: &Theme) -> Option<[Color; 12]> {
    let primary = solid(&theme.primary_color)?;
    let secondary = solid(&theme.secondary_color)?;
    let tertiary = solid(&theme.tertiary_color)?;
    let accent = solid(&theme.primary_border_color).or_else(|| solid(&theme.line_color))?;
    let bases = [primary, secondary, tertiary];
    let mut colors = [accent; 12];
    for (index, slot) in colors.iter_mut().enumerate() {
        let share = 0.12 + index as f32 * (0.76 / 11.0);
        *slot = bases[index % 3].mix(accent, share);
    }
    Some(colors)
}

fn slice_fill(
    theme: &Theme,
    stock: &Theme,
    derived: Option<&[Color; 12]>,
    index: usize,
    paper: Color,
    share: f32,
) -> Option<Color> {
    let slot = index % theme.pie_colors.len();
    let custom = theme.pie_colors[slot] != stock.pie_colors[slot];
    let color = if !custom && derived.is_some() {
        derived?[slot]
    } else {
        solid(&theme.pie_colors[slot])?
    };
    Some(color.mix(paper, share))
}

/// An explicit pie text color wins. Otherwise the diagram text does.
fn ink(explicit: &str, stock: &str, text: &str) -> Option<Color> {
    if explicit != stock {
        solid(explicit)
    } else {
        solid(text).or_else(|| solid(explicit))
    }
}

/// An explicit pie stroke wins. On a restyled page the accent does,
/// so a black ring does not vanish into a dark background.
fn line_ink(explicit: &str, stock: &str, restyled: bool, accent: Option<Color>) -> Option<Color> {
    if explicit != stock {
        solid(explicit)
    } else if restyled {
        accent.or_else(|| solid(explicit))
    } else {
        solid(explicit)
    }
}

fn metrics_ok(theme: &Theme) -> bool {
    (4.0..=100.0).contains(&theme.pie_title_text_size)
        && (4.0..=100.0).contains(&theme.pie_legend_text_size)
        && theme.pie_opacity.is_finite()
        && (0.0..=1.0).contains(&theme.pie_opacity)
        && theme.pie_stroke_width.is_finite()
        && (0.0..=20.0).contains(&theme.pie_stroke_width)
        && theme.pie_outer_stroke_width.is_finite()
        && (0.0..=20.0).contains(&theme.pie_outer_stroke_width)
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
    fn draws_the_fixture_in_the_theme_colors() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Pie;
        let source = include_str!("../../../omaspec-core/tests/fixtures/pie.mmd");
        let (svg, map) = try_draw(&graph, &theme, source).unwrap();
        assert!(
            svg.contains("Disk usage")
                && svg.contains("Packages")
                && svg.contains("Home")
                && svg.contains("Cache"),
            "{svg}"
        );
        let paper = Color::WHITE;
        let share = 1.0 - theme.pie_opacity;
        let packages = solid(&theme.pie_colors[0]).unwrap().mix(paper, share);
        let home = solid(&theme.pie_colors[1]).unwrap().mix(paper, share);
        let cache = solid(&theme.pie_colors[2]).unwrap().mix(paper, share);
        assert_ne!(
            packages.to_string(),
            theme.pie_colors[0].to_ascii_uppercase()
        );
        assert!(
            svg.contains(&packages.to_string()),
            "{svg} missing {packages}"
        );
        assert!(svg.contains(&home.to_string()), "{svg} missing {home}");
        assert!(svg.contains(&cache.to_string()), "{svg} missing {cache}");
        assert_eq!(map, Map::default());
    }

    #[test]
    fn show_data_prints_the_written_number() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Pie;
        let (svg, _) = try_draw(&graph, &theme, "pie showData\n\"Packages\" : 42\n").unwrap();
        assert!(svg.contains("Packages [42]"), "{svg}");
    }

    #[test]
    fn leaves_another_kind_and_a_negative_slice() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        assert!(try_draw(&graph, &theme, "pie\n\"a\" : 1\n").is_none());
        graph.kind = DiagramKind::Pie;
        let negative = try_draw(&graph, &theme, "pie showData\n\"a\" : -1\n\"b\" : 2\n")
            .unwrap()
            .0;
        assert!(
            negative.contains("a [-1]") && negative.contains("b [2]"),
            "{negative}"
        );
        assert!(crate::mermaid::refused(DiagramKind::Pie, "pie\n\"a\" : -1\n").is_none());
        let mut bad = Theme::mermaid_default();
        bad.pie_colors[0] = "nope".into();
        assert!(try_draw(&graph, &bad, "pie\n\"a\" : 1\n").is_none());
        let mut wide = Theme::mermaid_default();
        wide.pie_opacity = 2.0;
        assert!(try_draw(&graph, &wide, "pie\n\"a\" : 1\n").is_none());
        let mut clear = Theme::mermaid_default();
        clear.background = "transparent".into();
        assert!(try_draw(&graph, &clear, "pie\n\"a\" : 1\n").is_some());
        clear.background = "nope".into();
        assert!(try_draw(&graph, &clear, "pie\n\"a\" : 1\n").is_none());
    }

    #[test]
    fn a_dark_page_mixes_slices_from_the_theme() {
        let mut theme = Theme::mermaid_default();
        theme.background = "#121212".into();
        theme.primary_color = "#1e1e1e".into();
        theme.secondary_color = "#1e1e1e".into();
        theme.tertiary_color = "#121212".into();
        theme.primary_border_color = "#e68e0d".into();
        theme.line_color = "#e68e0d".into();
        theme.text_color = "#bebebe".into();
        let derived = marks(&theme).unwrap();
        assert_unique(&derived);
        let paper = solid("#121212").unwrap();
        let share = 1.0 - theme.pie_opacity;
        let first = derived[0].mix(paper, share);
        let second = derived[1].mix(paper, share);
        assert_ne!(first, second);
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Pie;
        let svg = try_draw(&graph, &theme, "pie title Disk\n\"A\" : 1\n\"B\" : 1\n")
            .unwrap()
            .0;
        let pastel = solid(&Theme::mermaid_default().pie_colors[0])
            .unwrap()
            .mix(Color::WHITE, share);
        assert!(
            svg.contains(&first.to_string())
                && svg.contains(&second.to_string())
                && svg.contains("#BEBEBE")
                && svg.contains("#E68E0D")
                && !svg.contains(&pastel.to_string()),
            "{svg}"
        );
        theme.pie_colors[0] = "#112233".into();
        theme.pie_opacity = 1.0;
        let explicit = try_draw(&graph, &theme, "pie\n\"A\" : 1\n\"B\" : 1\n")
            .unwrap()
            .0;
        assert!(
            explicit.to_ascii_lowercase().contains("#112233"),
            "{explicit}"
        );
    }

    fn assert_unique(colors: &[Color]) {
        let mut names: Vec<String> = colors.iter().map(ToString::to_string).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), colors.len(), "{colors:?}");
    }
}
