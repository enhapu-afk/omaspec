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
use omaspec_uml::gitgraph::{Diagram, Look, draw_mapped, from_mermaid_with, mermaid_refusal};
use omaspec_uml::map::Map;

/// The SVG, or `None` when this drawer leaves the diagram to mmdr. The
/// map holds where each commit's id and tag stand, by its line; a commit
/// is no box to fold or to hold.
pub fn try_draw(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let mut diagram = from_mermaid_with(source, &omaspec_core::settings::git_graph(source))?;
    paint(&mut diagram, theme)?;
    diagram.look.sketch = super::head::hand_drawn(source);
    diagram.parts = omaspec_uml::fold::parts(source);
    draw_mapped(&diagram).ok()
}

/// The first construct the reader refuses by name, read with the graph's
/// settings: a checkout of the main branch by the name they give it is
/// none.
pub fn refusal(source: &str) -> Option<(usize, &'static str)> {
    mermaid_refusal(source, &omaspec_core::settings::git_graph(source))
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
        title_size: super::title_size(theme),
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
        // The words of a commit's line are tied to it: its id and its tag.
        let lines: Vec<usize> = map.tied.iter().map(|tied| tied.line).collect();
        assert_eq!(lines, [1, 3], "{map:?}");
        let mut names: Vec<String> = lanes.iter().map(ToString::to_string).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), lanes.len(), "the lanes stay apart: {lanes:?}");
    }

    /// The settings of the front matter and a directive are carried out
    /// by Omaspec's drawer, not left to mmdr (REQ-098).
    #[test]
    fn a_git_graph_s_settings_are_carried_out_by_its_drawer() {
        let code = "---\nconfig:\n  gitGraph:\n    mainBranchName: trunk\n---\n%%{init: {\"gitGraph\": {\"rotateCommitLabel\": true}}}%%\ngitGraph\n  commit id: \"one\"\n  branch dev\n  commit\n  checkout trunk\n  merge dev\n";
        let drawn = crate::mermaid::draw(code).unwrap();
        assert!(drawn.declined.is_none(), "{:?}", drawn.declined);
        assert!(drawn.svg.contains(">trunk<") && !drawn.svg.contains(">main<"));
        assert!(drawn.svg.contains("rotate(-45 "));
        assert!(refusal(code).is_none());
    }

    /// Two git graphs in one text, which Mermaid refuses, are refused on
    /// the second's line, and nothing is drawn; mmdr drew one picture of
    /// them without a word (REQ-099).
    #[test]
    fn a_second_git_graph_is_refused_and_not_drawn() {
        let twice = "%% two\ngitGraph\n  commit\ngitGraph\n  commit\n";
        match crate::mermaid::render(twice) {
            Err(crate::RenderError::Diagram { line, message, .. }) => {
                assert_eq!(line, Some(3));
                assert!(message.starts_with("a second `gitGraph`"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        assert!(crate::mermaid::render(GRAPH).is_ok());
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
