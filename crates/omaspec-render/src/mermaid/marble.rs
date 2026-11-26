//! Mermaid's marble diagrams on Omaspec's own drawer
//! (`omaspec_uml::marble`), a kind mmdr does not read. The theme gives
//! the page, the type and the colors as the reference's theme does (a
//! marble the primary color, a span the secondary, the lines the line
//! color), each `marble…` variable of `themeVariables` over them, and
//! the `marble` section the sizes. A color the theme names that cannot
//! be read is Mermaid's default: there is no other drawer to leave the
//! diagram to.

use mermaid_rs_renderer::Theme;
use omaspec_uml::Color;
use omaspec_uml::map::Map;
use omaspec_uml::marble::{self, Look};
use serde_json::Value;

use super::font_name;

/// The SVG; `None` only for a text that is no marble diagram, which the
/// entry's `wrong` has said on its line.
pub fn try_draw(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let mut diagram = marble::from_mermaid(source).ok()?;
    diagram.look = look(theme, source);
    diagram.config = omaspec_core::settings::marble(source);
    Some(marble::draw(&diagram))
}

/// The look the theme and the diagram's `marble…` variables give.
fn look(theme: &Theme, source: &str) -> Look {
    let stock = Look::default();
    let solid = |value: &str| omaspec_uml::parse_color(value).ok().flatten();
    let vars = omaspec_core::settings::merged(source)
        .and_then(|root| root.get("themeVariables").cloned())
        .unwrap_or(Value::Null);
    let var = |name: &str| vars.get(name).and_then(Value::as_str).and_then(solid);
    let page = omaspec_uml::parse_color(&theme.background).unwrap_or(None);
    let ground = page.unwrap_or(Color::WHITE);
    let ink = solid(&theme.text_color).unwrap_or(stock.label);
    // The lines read on the page, a step toward the words where they
    // would not.
    let line = solid(&theme.line_color)
        .unwrap_or(stock.lane)
        .legible_on(ground, ink, 3.0);
    let error = stock.error.legible_on(ground, ink, 3.0);
    Look {
        font_name: font_name(&theme.font_family).unwrap_or_else(|| omaspec_uml::FACE.into()),
        sketch: super::head::hand_drawn(source),
        size: theme.font_size.clamp(8.0, 40.0),
        title_size: super::title_size(theme).clamp(8.0, 50.0),
        page,
        title: var("marbleTitleColor").unwrap_or(ink),
        lane: var("marbleLaneColor").unwrap_or(line),
        lane_label: var("marbleLaneLabelColor").unwrap_or(ink),
        axis_text: var("marbleAxisTextColor").unwrap_or(ink),
        grid: var("marbleGridColor").unwrap_or(line),
        event_fill: var("marbleEventFill")
            .or_else(|| solid(&theme.primary_color))
            .unwrap_or(stock.event_fill),
        event_stroke: var("marbleEventStroke").unwrap_or(line),
        event_text: var("marbleEventTextColor")
            .or_else(|| solid(&theme.primary_text_color))
            .unwrap_or(ink),
        label: var("marbleLabelColor").unwrap_or(ink),
        span_fill: var("marbleSpanFill")
            .or_else(|| solid(&theme.secondary_color))
            .unwrap_or(stock.span_fill),
        span_stroke: var("marbleSpanStroke").unwrap_or(line),
        span_text: var("marbleSpanTextColor").unwrap_or(ink),
        complete: var("marbleCompleteColor").unwrap_or(line),
        error: var("marbleErrorColor").unwrap_or(error),
        cancel: var("marbleCancelColor").unwrap_or(line),
        break_mark: var("marbleBreakColor").unwrap_or(line),
        break_ground: var("marbleBreakBkg").unwrap_or(ground),
        palette: stock.palette,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SWITCH_MAP: &str = "marble-beta\n  title switchMap: a new search cancels the one in flight\n  axis       : 0    5    10   15\n  input      : -a-----b--c--------|\n  \"search a\" :  [=====!\n  \"search b\" :        [==!\n  \"search c\" :           [=====]\n  output     : ----------------r--|\n  label = \"request\"\n  label ! \"cancelled\"\n  label r \"results for c\"\n";

    #[test]
    fn draws_the_proposals_switch_map() {
        let (svg, map) = try_draw(&Theme::mermaid_default(), SWITCH_MAP).unwrap();
        for words in [
            "switchMap",
            "search a",
            "output",
            "request",
            "cancelled",
            "results for c",
            "15",
        ] {
            assert!(svg.contains(words), "{words}: {svg}");
        }
        // The lanes' names, the marbles, the ticks and the labels are
        // tied to the lines that write them.
        let lines: std::collections::BTreeSet<usize> = map.tied.iter().map(|t| t.line).collect();
        assert!((1..=10).all(|line| lines.contains(&line)), "{lines:?}");
        assert!(!map.echoes.is_empty(), "the second `cancelled` is an echo");
    }

    #[test]
    fn the_theme_and_the_marble_variables_color_it() {
        let mut theme = Theme::mermaid_default();
        theme.background = "#1a1b26".into();
        theme.text_color = "#c0caf5".into();
        theme.line_color = "#1f2335".into();
        theme.primary_color = "nope".into();
        let source = "---\nconfig:\n  themeVariables:\n    marbleErrorColor: '#c62828'\n  marble:\n    columnWidth: 16\n---\nmarble-beta\n  source : -1-2-3-#\n";
        let look = look(&theme, source);
        // A line color that would sink into the dark page reads on it.
        assert!(look.lane.contrast(Color::hex(0x1a1b26)) >= 3.0);
        assert_eq!(look.error, Color::hex(0xc62828));
        assert_eq!(look.event_fill, Look::default().event_fill);
        let (svg, _) = try_draw(&theme, source).unwrap();
        assert!(svg.contains("#C62828"), "{svg}");
        assert_eq!(omaspec_core::settings::marble(source).column_width, 16.0);
    }
}
