//! Mermaid mind maps on the PlantUML mind-map engine.
//!
//! mmdr has parsed the diagram. The engine reads the same source again
//! ([`from_mermaid`]) for the indentation, the shapes and the notes an
//! icon would hide, and draws it in the colors of the Mermaid theme.
//! On the default theme each branch keeps that theme's own color. When
//! the base colors move, the branches are mixed from primary, secondary,
//! tertiary and the accent, so a dark page does not paint them the same
//! color as the paper. A shape this drawer still refuses, or a color it
//! cannot read, leaves the diagram to mmdr.

use mermaid_rs_renderer::{DiagramKind, Graph, Theme};
use omagram_uml::Color;
use omagram_uml::map::Map;
use omagram_uml::tree::{Idea, MindMap, Skinparam, draw_mindmap, from_mermaid, mermaid_refusal};

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a mind-map box is not a message to jump to.
pub fn try_draw(graph: &Graph, theme: &Theme, source: &str) -> Option<(String, Map)> {
    if graph.kind != DiagramKind::Mindmap {
        return None;
    }
    let mut diagram = from_mermaid(source)?;
    diagram.frame.skinparams = skin_of(theme)?;
    paint(&mut diagram, theme)?;
    draw_mindmap(&diagram).ok().map(|svg| (svg, Map::default()))
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
    add("defaultTextAlignment", "center");
    add("backgroundColor", &theme.background);
    add("nodeBackgroundColor", &theme.primary_color);
    add("nodeBorderColor", &theme.primary_border_color);
    add("arrowColor", &theme.line_color);
    for param in &params {
        if param.key.to_ascii_lowercase().contains("color") {
            omagram_uml::parse_color(&param.value).ok()?;
        }
    }
    Some(params)
}

/// One color per branch of the root, and the root in the primary color.
/// A branch keeps its color down the tree, which is Mermaid's section.
/// The default theme keeps the colors it names. A restyled page mixes
/// each branch toward the accent, and the label takes the theme's text
/// when that still reads on the fill.
fn paint(diagram: &mut MindMap, theme: &Theme) -> Option<()> {
    let stock = Theme::mermaid_default();
    if !restyled(theme, &stock) {
        paint_stock(diagram, theme);
        return Some(());
    }
    let primary = solid(&theme.primary_color)?;
    let secondary = solid(&theme.secondary_color)?;
    let tertiary = solid(&theme.tertiary_color)?;
    let accent = solid(&theme.primary_border_color).or_else(|| solid(&theme.line_color))?;
    let preferred = solid(&theme.primary_text_color)?;
    let bases = [primary, secondary, tertiary];
    for map in &mut diagram.maps {
        if map.root.color.is_none() {
            map.root.color = Some(primary.to_string());
            map.root.ink = Some(readable(primary, preferred).to_string());
        }
        for (index, idea) in map.right.iter_mut().enumerate() {
            tint_mark(idea, bases, accent, preferred, 2 * index);
        }
        for (index, idea) in map.left.iter_mut().enumerate() {
            tint_mark(idea, bases, accent, preferred, 2 * index + 1);
        }
    }
    Some(())
}

fn paint_stock(diagram: &mut MindMap, theme: &Theme) {
    let mut palette = Vec::new();
    for color in [
        theme.secondary_color.as_str(),
        theme.tertiary_color.as_str(),
        theme.primary_color.as_str(),
    ] {
        if solid(color).is_some() {
            palette.push(color);
        }
    }
    if palette.is_empty() {
        return;
    }
    for map in &mut diagram.maps {
        if map.root.color.is_none() {
            map.root.color = Some(theme.primary_color.clone());
        }
        for (index, idea) in map.right.iter_mut().enumerate() {
            tint(idea, palette[(2 * index) % palette.len()], None);
        }
        for (index, idea) in map.left.iter_mut().enumerate() {
            tint(idea, palette[(2 * index + 1) % palette.len()], None);
        }
    }
}

fn restyled(theme: &Theme, stock: &Theme) -> bool {
    theme.primary_color != stock.primary_color
        || theme.secondary_color != stock.secondary_color
        || theme.tertiary_color != stock.tertiary_color
        || theme.primary_border_color != stock.primary_border_color
        || theme.line_color != stock.line_color
}

/// Twelve branch colors. Each slot has its own share of the accent, so
/// neighbors stay apart even when primary and secondary are the same surface.
fn branch_fill(bases: [Color; 3], accent: Color, index: usize) -> Color {
    let slot = index % 12;
    let share = 0.22 + slot as f32 * (0.66 / 11.0);
    bases[slot % 3].mix(accent, share)
}

/// The theme's text when it contrasts with the fill, otherwise black or white.
fn readable(fill: Color, preferred: Color) -> Color {
    if fill.is_dark() != preferred.is_dark() {
        preferred
    } else if fill.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

fn tint_mark(idea: &mut Idea, bases: [Color; 3], accent: Color, preferred: Color, index: usize) {
    let fill = branch_fill(bases, accent, index);
    tint(
        idea,
        &fill.to_string(),
        Some(readable(fill, preferred).to_string()),
    );
}

fn tint(idea: &mut Idea, color: &str, ink: Option<String>) {
    if idea.node.color.is_none() {
        idea.node.color = Some(color.to_owned());
        idea.node.ink.clone_from(&ink);
    }
    for child in &mut idea.children {
        tint(child, color, ink.clone());
    }
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
    fn draws_a_mind_map_in_the_theme_colors() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        graph.kind = DiagramKind::Mindmap;
        let (svg, map) = try_draw(&graph, &theme, "mindmap\n  ((Root))\n    Branch\n").unwrap();
        assert!(svg.contains("Root") && svg.contains("Branch"), "{svg}");
        assert!(svg.contains("#ECECFF") && svg.contains("#FFFFDE"), "{svg}");
        assert!(svg.contains("<ellipse"), "{svg}");
        assert_eq!(map, Map::default());
    }

    #[test]
    fn draws_a_cloud_and_leaves_the_other_cloud() {
        let theme = Theme::mermaid_default();
        let mut graph = Graph::new();
        assert!(try_draw(&graph, &theme, "mindmap\n  Root\n").is_none());
        graph.kind = DiagramKind::Mindmap;
        let svg = try_draw(&graph, &theme, "mindmap\n  )cloud(\n").unwrap().0;
        assert!(svg.contains("cloud") && svg.contains("<path"), "{svg}");
        let burst = try_draw(&graph, &theme, "mindmap\n  ))bang((\n").unwrap().0;
        assert!(
            burst.contains("bang") && burst.contains("<polygon"),
            "{burst}"
        );
        assert_eq!(
            named_decline("mindmap\n  (-no-)\n").map(|(_, reason)| reason),
            Some("a cloud is not drawn by Omagram; mmdr draws it".into())
        );
        let mut bad = Theme::mermaid_default();
        bad.primary_color = "nope".into();
        assert!(try_draw(&graph, &bad, "mindmap\n  Root\n").is_none());
    }

    #[test]
    fn a_dark_page_keeps_the_branches_apart() {
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
        graph.kind = DiagramKind::Mindmap;
        let svg = try_draw(
            &graph,
            &theme,
            "mindmap\n  ((Root))\n    One\n    Two\n    Three\n",
        )
        .unwrap()
        .0;
        let primary = Color::hex(0x1E1E1E);
        let bases = [primary, primary, Color::hex(0x121212)];
        let accent = Color::hex(0xE68E0D);
        let marks = [
            branch_fill(bases, accent, 0),
            branch_fill(bases, accent, 1),
            branch_fill(bases, accent, 2),
        ];
        assert_ne!(marks[0], marks[1]);
        assert_ne!(marks[1], marks[2]);
        assert_ne!(marks[0], Color::hex(0x121212));
        assert!(
            svg.contains("Root")
                && svg.contains("One")
                && svg.contains("Two")
                && svg.contains("Three")
                && svg.contains("#1E1E1E")
                && svg.contains("#E68E0D")
                && svg.contains("#BEBEBE")
                && svg.contains(&marks[0].to_string())
                && svg.contains(&marks[1].to_string())
                && svg.contains(&marks[2].to_string())
                && !svg.contains("#ECECFF")
                && !svg.contains("#FFFFDE"),
            "{svg}"
        );

        let mut pale = Theme::mermaid_default();
        pale.primary_color = "#f4f4f4".into();
        pale.secondary_color = "#f4f4f4".into();
        pale.tertiary_color = "#f4f4f4".into();
        pale.primary_text_color = "#f4f4f4".into();
        pale.primary_border_color = "#111111".into();
        pale.line_color = "#111111".into();
        let washed = try_draw(&graph, &pale, "mindmap\n  ((Root))\n").unwrap().0;
        assert!(
            washed.contains("#F4F4F4") && washed.contains("#000000"),
            "{washed}"
        );
    }
}
