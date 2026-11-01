//! Mermaid state diagrams on the engine PlantUML's are drawn with: Omaspec
//! reads them into the same tree (`omaspec_uml::state::from_mermaid`) and
//! draws them in the colors of the Mermaid theme. What it reads is what
//! its checks and its play go by; a diagram it does not read is left to
//! mmdr, and not checked.

use mermaid_rs_renderer::Theme;
use omaspec_uml::map::Map;
use omaspec_uml::state::{Skinparam, draw_mapped, from_mermaid};

use super::{font_name, plain_number};

/// The SVG and its map, or `None` when mmdr draws the diagram. `fenced`
/// is the text inside a Markdown fence, on its own lines.
pub fn try_draw(theme: &Theme, fenced: &str) -> Option<(String, Map)> {
    let mut diagram = from_mermaid(fenced)?;
    // The theme first; the states' own colors over it.
    let mut params = skin_of(theme)?;
    params.append(&mut diagram.frame.skinparams);
    diagram.frame.skinparams = params;
    diagram.frame.by_hand = omaspec_uml::hand::numbered(fenced);
    draw_mapped(&diagram).ok()
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
    add("arrowFontColor", &theme.primary_text_color);
    add("stateBackgroundColor", &theme.primary_color);
    add("stateBorderColor", &theme.primary_border_color);
    add("noteBackgroundColor", &theme.sequence_note_fill);
    add("noteBorderColor", &theme.sequence_note_border);
    // A color the engine cannot read would decline the drawing. Check
    // first, so a bad theme falls back instead of drawing the wrong look.
    for param in &params {
        if param.key.to_ascii_lowercase().contains("color") {
            omaspec_uml::parse_color(&param.value).ok()?;
        }
    }
    Some(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_a_machine_in_the_theme_colors_with_its_lines() {
        let theme = Theme::mermaid_default();
        let source = "stateDiagram-v2\n[*] --> Idle\nIdle --> Busy : job\nBusy --> [*]\n";
        let (svg, map) = try_draw(&theme, source).unwrap();
        assert!(svg.contains("Idle") && svg.contains("job"), "{svg}");
        assert!(svg.contains("#ECECFF"), "{svg}");
        let states: Vec<(&str, usize)> = map
            .states
            .iter()
            .map(|s| (s.name.as_str(), s.line))
            .collect();
        assert_eq!(states, [("", 1), ("Idle", 1), ("Busy", 2), ("", 3)]);
        let lines: Vec<usize> = map.transitions.iter().map(|t| t.line).collect();
        assert_eq!(lines, [1, 2, 3]);
        assert!(try_draw(&theme, "stateDiagram\nA --> B\nwhatever this is\n").is_none());
    }

    /// Where a state is in the picture.
    fn spot(source: &str, name: &str) -> omaspec_uml::graph::Rect {
        let (_, _, map) = crate::mermaid::render_mapped(source).unwrap();
        map.states
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no {name}"))
            .rect
    }

    #[test]
    fn bottom_to_top_and_right_to_left_turn_the_layout_round() {
        let up = "stateDiagram-v2\ndirection BT\n[*] --> A\nA --> B\n";
        let (a, b) = (spot(up, "A"), spot(up, "B"));
        assert!(b.y + b.h <= a.y + 0.5, "{a:?} {b:?}");
        let left = "stateDiagram-v2\ndirection RL\n[*] --> A\nA --> B\n";
        let (a, b) = (spot(left, "A"), spot(left, "B"));
        assert!(b.x + b.w <= a.x + 0.5, "{a:?} {b:?}");
        // A nested state turns round with the rest, its name still on top.
        let nested =
            "stateDiagram-v2\ndirection BT\n[*] --> On\nstate On {\n[*] --> A\nA --> B\n}\n";
        let (on, a, b) = (spot(nested, "On"), spot(nested, "A"), spot(nested, "B"));
        assert!(b.y + b.h <= a.y + 0.5);
        assert!(on.y < b.y && a.y + a.h < on.y + on.h, "{on:?} {a:?} {b:?}");
    }

    #[test]
    fn concurrent_regions_sit_side_by_side() {
        let source = "stateDiagram-v2\n[*] --> Active\nstate Active {\n[*] --> One\nOne --> Done\n--\n[*] --> Two\n}\n";
        let (one, two) = (spot(source, "One"), spot(source, "Two"));
        assert!(two.x >= one.x + one.w, "{one:?} {two:?}");
        let (_, _, map) = crate::mermaid::render_mapped(source).unwrap();
        let starts = map
            .states
            .iter()
            .filter(|s| s.shape == omaspec_uml::map::Shape::Start)
            .count();
        assert_eq!(starts, 3, "the diagram's and each region's own");
    }

    #[test]
    fn nested_states_hold_their_children() {
        let source = include_str!("../../../omaspec-core/tests/fixtures/state.mmd");
        let (_, _, map) = crate::mermaid::render_mapped(source).unwrap();
        let parent = |name: &str| {
            map.states
                .iter()
                .find(|s| s.name == name)
                .and_then(|s| s.parent.clone())
        };
        assert_eq!(parent("Saving").as_deref(), Some("Suspend"));
        assert_eq!(parent("Lockd"), None);
        let (outer, inner) = (spot(source, "Suspend"), spot(source, "Saving"));
        assert!(inner.x >= outer.x && inner.x + inner.w <= outer.x + outer.w);
    }

    #[test]
    fn colors_are_drawn_and_what_omaspec_cannot_read_falls_back() {
        let colored = "stateDiagram-v2\n[*] --> Idle\nstyle Idle fill:#ff00aa\nIdle : quiet\n";
        let svg = crate::mermaid::render(colored).unwrap();
        assert!(svg.to_ascii_lowercase().contains("#ff00aa"), "{svg}");
        assert!(svg.contains("quiet"));
        assert!(crate::mermaid::coverage(colored).unwrap().is_none());
        for refused in [
            "stateDiagram-v2\n[*] --> Idle\nstyle Idle fill:rgb(1,2,3)\n",
            // A transition into the state's own nested state.
            "stateDiagram-v2\nstate On {\n[*] --> Idle\n}\nOn --> Idle\n",
        ] {
            let declined = crate::mermaid::coverage(refused).unwrap().unwrap();
            assert!(declined.reason.contains("state diagram"), "{refused}");
            assert!(crate::mermaid::render(refused).is_ok());
        }
    }

    #[test]
    fn the_map_knows_the_lines_inside_a_fence() {
        let source = "```mermaid\nstateDiagram-v2\n[*] --> Idle\nIdle --> Busy : job\nstate Busy {\n[*] --> Work\nWork --> [*]\n[*] --> Rest\n}\n```\n";
        let (_, _, map) = crate::mermaid::render_mapped(source).unwrap();
        let mut states: Vec<(&str, usize)> = map
            .states
            .iter()
            .map(|s| (s.name.as_str(), s.line))
            .collect();
        states.sort();
        assert_eq!(
            states,
            [
                ("", 2),
                ("", 5),
                ("", 6),
                ("Busy", 3),
                ("Idle", 2),
                ("Rest", 7),
                ("Work", 5)
            ]
        );
        let mut lines: Vec<usize> = map.transitions.iter().map(|t| t.line).collect();
        lines.sort();
        assert_eq!(lines, [2, 3, 5, 6, 7]);
    }
}
