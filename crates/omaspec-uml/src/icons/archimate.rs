//! The icons of ArchiMate's elements, drawn by Omaspec as line art in a
//! box 16 by 16, in the words' color: PlantUML's built-in sprites
//! `archimate/business-actor`, `archimate/technology-node`, … A name says
//! its layer and its type; the type alone chooses the icon, so
//! `business-process` and `application-process` share one.

/// The icon for `name` (`business-actor`, `archimate/node`): its path, and
/// whether it is filled rather than stroked.
pub fn icon(name: &str) -> Option<(String, bool)> {
    let name = name.trim().to_ascii_lowercase();
    let name = name.strip_prefix("archimate/").unwrap_or(&name);
    let kind = [
        "business-",
        "application-",
        "technology-",
        "physical-",
        "implementation-",
        "motivation-",
        "strategy-",
        "other-",
    ]
    .iter()
    .find_map(|layer| name.strip_prefix(layer))
    .unwrap_or(name);
    let kind = kind.replace('_', "-");
    let d = match kind.as_str() {
        "actor" => format!("{}M8 6v5M4 8h8M8 11l-3 4M8 11l3 4", circle(8.0, 3.5, 2.5)),
        "role" | "stakeholder" => {
            "M5 4h6a2 4 0 0 1 0 8h-6a2 4 0 0 1 0-8zM11 4a2 4 0 0 0 0 8".to_owned()
        }
        "collaboration" => format!("{}{}", circle(6.0, 8.0, 4.0), circle(10.0, 8.0, 4.0)),
        "interface" | "interface-provided" => format!("{}M1 8h6", circle(10.5, 8.0, 3.5)),
        "process" | "event-process" => "M2 6h8V3l5 5-5 5v-3H2z".to_owned(),
        "function" => "M3 14V6l5-4 5 4v8l-5-4z".to_owned(),
        "interaction" => "M7 3a5 5 0 0 0 0 10zM9 3a5 5 0 0 1 0 10z".to_owned(),
        "event" => "M2 4h9a4 4 0 0 1 0 8H2l3-4z".to_owned(),
        "service" => "M5 5h6a3 3 0 0 1 0 6H5a3 3 0 0 1 0-6z".to_owned(),
        "object" | "data-object" | "dataobject" => "M2 3h12v10H2zM2 6h12".to_owned(),
        "contract" => "M2 3h12v10H2zM2 6h12M2 10h12".to_owned(),
        "representation" | "deliverable" => "M2 3h12v8c-3-2-4 2-6 1s-4-2-6 0zM2 6h12".to_owned(),
        "product" => "M2 3h12v10H2zM2 6h6V3".to_owned(),
        "component" => "M5 2h9v12H5zM3 5h4v2H3zM3 9h4v2H3z".to_owned(),
        "node" => "M2 5h9v9H2zM2 5l3-3h9l-3 3M14 2v9l-3 3".to_owned(),
        "device" => "M3 3h10v7H3zM1 13h14l-2-3H3z".to_owned(),
        "system-software" | "systemsoftware" => {
            format!("{}M4 4.5a5 5 0 1 0 7 7", circle(9.5, 6.5, 4.5))
        }
        "artifact" => "M4 2h6l3 3v9H4zM10 2v3h3".to_owned(),
        "communication-network" | "communicationnetwork" | "network" => {
            format!("{}{}M5 8h6", circle(3.0, 8.0, 2.0), circle(13.0, 8.0, 2.0))
        }
        "path" | "communication-path" | "communicationpath" => {
            "M2 8h12M4.5 5.5L2 8l2.5 2.5M11.5 5.5L14 8l-2.5 2.5".to_owned()
        }
        "distribution-network" | "distributionnetwork" => {
            "M2 6h12M2 10h12M4.5 3.5L2 6l2.5 2.5M11.5 7.5L14 10l-2.5 2.5".to_owned()
        }
        "equipment" => format!("{}{}", circle(6.0, 10.0, 3.5), circle(11.5, 4.5, 2.5)),
        "facility" => "M2 14V6l4 3V6l4 3V3h3v11z".to_owned(),
        "material" => "M5 3h6l3 5-3 5H5L2 8zM5.5 5h5M4 8h8M5.5 11h5".to_owned(),
        "location" => "M8 15s-5-5-5-9a5 5 0 0 1 10 0c0 4-5 9-5 9z".to_owned(),
        "driver" => format!("{}M8 2v12M2 8h12M4 4l8 8M12 4l-8 8", circle(8.0, 8.0, 6.0)),
        "assessment" => format!("{}M9 9l5 5", circle(6.0, 6.0, 4.0)),
        "goal" => format!(
            "{}{}{}",
            circle(8.0, 8.0, 6.0),
            circle(8.0, 8.0, 3.5),
            circle(8.0, 8.0, 1.0)
        ),
        "outcome" => format!(
            "{}{}M8 8l6-6M11 2h3v3",
            circle(7.0, 9.0, 5.5),
            circle(7.0, 9.0, 2.5)
        ),
        "principle" => "M3 2h10v12H3zM8 4.5v5.5M8 11.8v.4".to_owned(),
        "requirement" => "M4.5 4H14l-2.5 8H2z".to_owned(),
        "constraint" => "M4.5 4H14l-2.5 8H2zM7 4l-2.5 8".to_owned(),
        "meaning" => "M4 12a3 3 0 0 1 0-6 4 4 0 0 1 7.5-1 3 3 0 0 1 .5 6H4z".to_owned(),
        "value" => "M2 8a6 4 0 1 0 12 0a6 4 0 1 0-12 0z".to_owned(),
        "capability" => "M2 14h12V2h-4v4H6v4H2zM6 10h8M10 6h4M6 10v4M10 6v8".to_owned(),
        "resource" => "M2 5h11v6H2zM13 7h1.5v2H13M5 5v6M8 5v6".to_owned(),
        "course-of-action" | "courseofaction" => format!(
            "{}{}M2 14l5-5M2 11v3h3",
            circle(10.0, 6.0, 4.5),
            circle(10.0, 6.0, 1.5)
        ),
        "value-stream" | "valuestream" => "M2 4h9l3 4-3 4H2l3-4z".to_owned(),
        "work-package" | "workpackage" => "M12.5 9.5A5 5 0 1 1 11 4.5M9 2l2.5 2.5L9 7".to_owned(),
        "plateau" => "M5 4h9M3.5 8h9M2 12h9".to_owned(),
        "gap" => format!("{}M2 6.5h12M2 9.5h12", circle(8.0, 8.0, 5.0)),
        "grouping" => "M2 5h12v9H2zM2 5V2h6v3".to_owned(),
        "junction" | "junction-and" => return Some((circle(8.0, 8.0, 3.0), true)),
        "junction-or" => circle(8.0, 8.0, 3.0),
        _ => return None,
    };
    Some((d, false))
}

/// A circle as a path.
fn circle(cx: f32, cy: f32, r: f32) -> String {
    format!(
        "M{} {cy}a{r} {r} 0 1 0 {} 0a{r} {r} 0 1 0 -{} 0z",
        cx - r,
        2.0 * r,
        2.0 * r
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_says_its_type_and_its_layer() {
        assert!(icon("business-actor").is_some());
        assert_eq!(icon("archimate/business-actor"), icon("application-actor"));
        assert_eq!(
            icon("technology-system-software"),
            icon("technology-systemsoftware")
        );
        assert_eq!(icon("junction-and").map(|(_, filled)| filled), Some(true));
        assert!(icon("business-unicorn").is_none());
        for name in [
            "business-role",
            "business-collaboration",
            "business-interface",
            "business-process",
            "business-function",
            "business-interaction",
            "business-event",
            "business-service",
            "business-object",
            "business-contract",
            "business-representation",
            "business-product",
            "application-component",
            "application-data-object",
            "technology-node",
            "technology-device",
            "technology-artifact",
            "technology-communication-network",
            "technology-path",
            "physical-equipment",
            "physical-facility",
            "physical-distribution-network",
            "physical-material",
            "motivation-stakeholder",
            "motivation-driver",
            "motivation-assessment",
            "motivation-goal",
            "motivation-outcome",
            "motivation-principle",
            "motivation-requirement",
            "motivation-constraint",
            "motivation-meaning",
            "motivation-value",
            "strategy-capability",
            "strategy-resource",
            "strategy-course-of-action",
            "strategy-value-stream",
            "implementation-workpackage",
            "implementation-deliverable",
            "implementation-event",
            "implementation-plateau",
            "implementation-gap",
            "other-location",
            "other-grouping",
        ] {
            assert!(icon(name).is_some(), "{name}");
        }
    }
}
