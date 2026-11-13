//! Mermaid's charts of numbers on Omaspec's own drawers
//! (`omaspec_uml::chart`): a packet's fields, a radar, a treemap, a
//! sankey, an XY chart and a quadrant chart. Each
//! reads the text itself; the Mermaid theme gives the page, the ink and
//! the palette, the pie's, so a chart sits among the other diagrams in
//! their colors. A chart a drawer does not read is left to mmdr.

use mermaid_rs_renderer::Theme;
use omaspec_uml::chart::{self, Look};
use omaspec_uml::map::Map;

use super::font_name;
use super::pie;

/// The look the theme gives a chart; `None` for a color it cannot read.
fn look(theme: &Theme, source: &str) -> Option<Look> {
    let stock = Theme::mermaid_default();
    let page = omaspec_uml::parse_color(&theme.background).ok()?;
    // The `pieN` colors the diagram sets itself; else the chart palette,
    // which reads on any page.
    let custom = omaspec_core::settings::merged(source)
        .and_then(|init| init.get("themeVariables").cloned())
        .and_then(|vars| {
            vars.as_object().map(|vars| {
                vars.keys().any(|key| {
                    key.strip_prefix("pie")
                        .is_some_and(|n| n.parse::<u32>().is_ok())
                })
            })
        })
        .unwrap_or(false);
    let palette = if custom {
        pie::palette(theme, &stock)?
    } else {
        chart::PALETTE
            .into_iter()
            .map(omaspec_uml::Color::hex)
            .collect()
    };
    let solid = |value: &str| omaspec_uml::parse_color(value).ok().flatten();
    Some(Look {
        font_name: font_name(&theme.font_family)?,
        sketch: super::head::hand_drawn(source),
        size: theme.font_size.clamp(8.0, 40.0),
        page,
        ink: solid(&theme.text_color)?,
        line: solid(&theme.line_color)?,
        fill: solid(&theme.primary_color)?,
        border: solid(&theme.primary_border_color)?,
        palette,
    })
}

pub fn packet(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let diagram = chart::packet::from_mermaid(source)?;
    let svg = chart::packet::draw(&diagram, &look(theme, source)?).ok()?;
    Some((svg, Map::default()))
}

pub fn radar(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let diagram = chart::radar::from_mermaid(source)?;
    let svg = chart::radar::draw(&diagram, &look(theme, source)?).ok()?;
    Some((svg, Map::default()))
}

pub fn treemap(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let diagram = chart::treemap::from_mermaid(source)?;
    let svg = chart::treemap::draw(&diagram, &look(theme, source)?).ok()?;
    Some((svg, Map::default()))
}

pub fn sankey(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let diagram = chart::sankey::from_mermaid(source)?;
    let svg = chart::sankey::draw(&diagram, &look(theme, source)?).ok()?;
    Some((svg, Map::default()))
}

pub fn xy(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let diagram = chart::xy::from_mermaid(source)?;
    let svg = chart::xy::draw(&diagram, &look(theme, source)?).ok()?;
    Some((svg, Map::default()))
}

pub fn quadrant(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let diagram = chart::quadrant::from_mermaid(source)?;
    let svg = chart::quadrant::draw(&diagram, &look(theme, source)?).ok()?;
    Some((svg, Map::default()))
}
