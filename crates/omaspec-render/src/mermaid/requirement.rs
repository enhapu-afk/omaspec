//! Mermaid requirement diagrams on the structure engine.
//!
//! The engine reads the source itself, not mmdr
//! ([`from_mermaid`]): the requirements and elements as boxes with a row
//! for each thing said about them, the relationships as lines that name
//! their kind. It draws them in the colors of the Mermaid theme, as it
//! draws a class diagram, with lines by hand (docs/adr/0016) and boxes
//! held in place. A construct the reader refuses, or a theme color it
//! cannot read, leaves the diagram to mmdr.

use mermaid_rs_renderer::Theme;
use omaspec_uml::map::Map;
use omaspec_uml::structure::draw_mapped;
use omaspec_uml::structure::requirement::from_mermaid;

/// The SVG and its map of lines, or `None` when this drawer leaves the
/// diagram to mmdr.
pub fn try_draw(theme: &Theme, source: &str) -> Option<(String, Map)> {
    let mut diagram = from_mermaid(source)?;
    diagram.frame.skinparams = super::class::skin_of(theme)?;
    diagram.frame.skinparams.extend(super::sketch_param(source));
    diagram.frame.by_hand = omaspec_uml::hand::numbered(source);
    diagram.frame.held = omaspec_uml::hand::held(source);
    diagram.frame.unplaced = diagram.root.hold(&diagram.frame.held);
    diagram
        .frame
        .unplaced
        .extend(diagram.root.part(&omaspec_uml::hand::apart(source)));
    omaspec_uml::structure::label_by_hand(&mut diagram.root, &omaspec_uml::hand::labels(source));
    draw_mapped(&diagram).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mermaid_rs_renderer::DiagramKind;

    const BRAKES: &str = "requirementDiagram\n  requirement stop {\n    id: 1\n    risk: high\n  }\n  element track_test {\n    type: test suite\n  }\n  track_test - verifies -> stop\n";

    #[test]
    fn draws_the_boxes_in_the_theme_s_colors() {
        let mut theme = Theme::mermaid_default();
        theme.primary_color = "#102030".into();
        theme.primary_border_color = "#405060".into();
        let (svg, map) = try_draw(&theme, BRAKES).unwrap();
        assert!(
            svg.contains("Requirement")
                && svg.contains("track_test")
                && svg.contains("Risk: High")
                && svg.contains("«verifies»")
                && svg.contains("#102030")
                && svg.contains("#405060"),
            "{svg}"
        );
        assert_ne!(map, Map::default(), "its boxes and its line are places");
    }

    #[test]
    fn leaves_what_it_does_not_draw_to_mmdr() {
        let theme = Theme::mermaid_default();
        let odd = "requirementDiagram\n  requirement a {\n    risk: huge\n  }\n";
        assert!(try_draw(&theme, odd).is_none());
        assert_eq!(
            crate::mermaid::refused(DiagramKind::Requirement, odd),
            Some((
                2,
                "a risk is not drawn by Omaspec; mmdr draws it".to_owned()
            ))
        );
        assert!(crate::mermaid::refused(DiagramKind::Requirement, BRAKES).is_none());
        let mut bad = Theme::mermaid_default();
        bad.primary_color = "nope".into();
        assert!(try_draw(&bad, BRAKES).is_none());
    }
}
