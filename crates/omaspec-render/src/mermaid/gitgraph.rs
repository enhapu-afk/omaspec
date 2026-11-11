//! Mermaid git graphs on Omaspec's git graph drawer.
//!
//! The drawer reads the source itself, not mmdr ([`from_mermaid`]): the
//! statements run on a small repository, and each branch becomes a lane.
//! The lanes' colors are the theme's accent turned round the color wheel,
//! so they follow the theme and are still told apart. A construct
//! the reader refuses, or a theme color it cannot read, leaves the
//! diagram to mmdr.

use super::font_name;
use mermaid_rs_renderer::Theme;
use omaspec_uml::Color;
use omaspec_uml::gitgraph::{Diagram, Look, draw, from_mermaid};
use omaspec_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the diagram to mmdr.
/// The map stays empty: a commit is not a box to fold or to hold.
pub fn try_draw(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let mut diagram = from_mermaid(source)?;
    paint(&mut diagram, theme)?;
    diagram.look.sketch = super::head::hand_drawn(source);
    draw(&diagram).ok().map(|svg| (svg, Map::default()))
}

/// Type, ink, page and the lanes' colors come from the theme. A color
/// this drawer uses and cannot read, or a font it cannot name, leaves the
/// diagram to mmdr.
fn paint(diagram: &mut Diagram, theme: &Theme) -> Option<()> {
    if !(4.0..=100.0).contains(&theme.font_size) {
        return None;
    }
    let background = omaspec_uml::parse_color(&theme.background).ok()?;
    diagram.look = Look {
        font_name: font_name(&theme.font_family)?,
        sketch: false,
        label_size: theme.font_size,
        title_size: theme.font_size * 1.25,
        ink: solid(&theme.text_color)?,
        background,
        lanes: lanes(theme)?,
    };
    Some(())
}

/// Eight lane colors: the accent for `main`, then the same color turned
/// round the color wheel, a step for each lane, so the lanes are told
/// apart whatever the theme's surfaces are; each kept legible on the page.
fn lanes(theme: &Theme) -> Option<[Color; 8]> {
    let accent = solid(&theme.primary_border_color).or_else(|| solid(&theme.line_color))?;
    let ink = solid(&theme.text_color)?;
    let paper = omaspec_uml::parse_color(&theme.background)
        .ok()?
        .unwrap_or(Color::WHITE);
    let (hue, saturation, lightness) = hsl(accent);
    // A gray accent has no color to turn: it gets one.
    let saturation = saturation.max(0.42);
    let lightness = lightness.clamp(0.38, 0.66);
    let mut lanes = [accent; 8];
    for (index, slot) in lanes.iter_mut().enumerate() {
        // 137.5°, the golden angle: no two of eight lanes come close.
        let turned = (hue + index as f32 * 137.5) % 360.0;
        *slot = rgb(turned, saturation, lightness).legible_on(paper, ink, 2.2);
    }
    Some(lanes)
}

/// A color's hue in degrees, its saturation and its lightness.
fn hsl(color: Color) -> (f32, f32, f32) {
    let [r, g, b] = [color.0, color.1, color.2].map(|part| f32::from(part) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let lightness = (max + min) / 2.0;
    let spread = max - min;
    if spread < 1e-4 {
        return (0.0, 0.0, lightness);
    }
    let saturation = spread / (1.0 - (2.0 * lightness - 1.0).abs());
    let hue = if max == r {
        ((g - b) / spread).rem_euclid(6.0)
    } else if max == g {
        (b - r) / spread + 2.0
    } else {
        (r - g) / spread + 4.0
    };
    (hue * 60.0, saturation, lightness)
}

fn rgb(hue: f32, saturation: f32, lightness: f32) -> Color {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let part = |n: f32| {
        let k = (n + hue / 30.0) % 12.0;
        let value = lightness - chroma / 2.0 * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    Color(part(0.0), part(8.0), part(4.0))
}

fn solid(value: &str) -> Option<Color> {
    omaspec_uml::parse_color(value).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mermaid_rs_renderer::DiagramKind;

    const GRAPH: &str = "gitGraph\n  commit id: \"start\"\n  branch develop\n  commit tag: \"v1\"\n  checkout main\n  merge develop\n";

    #[test]
    fn draws_a_git_graph_in_the_theme_s_colors() {
        let mut theme = Theme::mermaid_default();
        theme.text_color = "#112233".into();
        let (svg, map) = try_draw(&theme, GRAPH).unwrap();
        let lanes = lanes(&theme).unwrap();
        assert!(
            svg.contains(">main<")
                && svg.contains(">develop<")
                && svg.contains(">start<")
                && svg.contains(">v1<")
                && svg.contains("#112233")
                && svg.contains(&lanes[0].to_string())
                && svg.contains(&lanes[1].to_string()),
            "{svg}"
        );
        assert_eq!(map, Map::default());
        let mut names: Vec<String> = lanes.iter().map(ToString::to_string).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), lanes.len(), "the lanes stay apart: {lanes:?}");
    }

    #[test]
    fn leaves_what_it_does_not_draw_to_mmdr() {
        let theme = Theme::mermaid_default();
        assert!(try_draw(&theme, "gitGraph\n  commit\n  checkout nowhere\n").is_none());
        assert_eq!(
            crate::mermaid::refused(
                DiagramKind::GitGraph,
                "gitGraph\n  commit\n  checkout nowhere\n"
            ),
            Some((
                2,
                "a branch that is not there is not drawn by Omaspec; mmdr draws it".to_owned()
            ))
        );
        assert!(crate::mermaid::refused(DiagramKind::GitGraph, GRAPH).is_none());
        let mut bad = Theme::mermaid_default();
        bad.background = "nope".into();
        assert!(try_draw(&bad, GRAPH).is_none());
        bad.background = "transparent".into();
        bad.text_color = "nope".into();
        assert!(try_draw(&bad, GRAPH).is_none());
        bad.text_color = "#333".into();
        bad.font_family = "<bad>".into();
        assert!(try_draw(&bad, GRAPH).is_none());
        bad.font_family = Theme::mermaid_default().font_family;
        bad.font_size = 2.0;
        assert!(try_draw(&bad, GRAPH).is_none());
    }
}
