use super::*;
use crate::structure::{Dash, Element, Group, Head, Shape};

const BANK: &str = "@startuml
!include <C4/C4_Container>
LAYOUT_WITH_LEGEND()
title Internet banking
Person(customer, \"Customer\", \"A customer of the bank, with personal accounts\")
System_Ext(mail, \"E-mail system\", \"Microsoft Exchange\")
System_Boundary(bank, \"Internet banking\") {
  Container(web, \"Web application\", \"Java, Spring MVC\", \"Delivers the static content\")
  ContainerDb(db, \"Database\", \"Oracle\")
  ContainerQueue(events, \"Events\", \"Kafka\")
}
Rel(customer, web, \"Uses\", \"HTTPS\")
Rel_R(web, db, \"Reads from and writes to\", \"JDBC\")
Rel_Back(customer, mail, \"Sends e-mails to\")
BiRel_D(web, events, \"Publishes\")
Lay_R(mail, bank)
@enduml
";

fn diagram(source: &str) -> Diagram {
    match crate::parse(source).unwrap_or_else(|err| panic!("{err:?}")) {
        crate::Diagram::Structure(diagram) => diagram,
        other => panic!("not a structure diagram: {other:?}"),
    }
}

fn find<'a>(group: &'a Group, key: &str) -> &'a Element {
    group
        .nodes
        .iter()
        .find_map(|node| {
            if node.key == key {
                Some(node)
            } else {
                group_find(&node.children, key)
            }
        })
        .unwrap_or_else(|| panic!("{key}"))
}

fn group_find<'a>(group: &'a Group, key: &str) -> Option<&'a Element> {
    group.nodes.iter().find_map(|node| {
        if node.key == key {
            Some(node)
        } else {
            group_find(&node.children, key)
        }
    })
}

#[test]
fn the_library_is_known_by_its_name_wherever_it_comes_from() {
    for line in [
        "!include <C4/C4_Container>",
        "!include <C4/C4_Context.puml>",
        "!includeurl https://raw.githubusercontent.com/plantuml-stdlib/C4-PlantUML/master/C4_Component.puml",
        "!include C4_Deployment.puml",
        "!include ../lib/C4.puml",
    ] {
        assert!(library(line).is_some(), "{line}");
    }
    for line in [
        "!include <aws/common>",
        "!include shop.puml",
        "!includesub x.puml!A",
    ] {
        assert!(library(line).is_none(), "{line}");
    }
    let err = crate::parse("@startuml\n!include <C4/C4_Dynamic>\nPerson(a, \"A\")\n@enduml\n")
        .unwrap_err();
    assert_eq!(err.line, Some(1));
    assert!(err.reason.contains("dynamic"), "{}", err.reason);
}

#[test]
fn elements_are_the_boxes_of_their_kind_in_the_colors_of_c4() {
    let d = diagram(BANK);
    let customer = find(&d.root, "customer");
    assert_eq!(customer.shape, Shape::Person);
    assert_eq!(customer.fill.as_deref(), Some("08427B"));
    assert_eq!(customer.text_color.as_deref(), Some("FFFFFF"));
    assert!(customer.label.value.contains("<b>Customer</b>"));
    assert!(
        customer
            .label
            .value
            .contains("A customer of the bank, with</size>"),
        "{}",
        customer.label.value
    );
    assert_eq!(customer.line, 4);
    let mail = find(&d.root, "mail");
    assert_eq!(mail.shape, Shape::Rectangle);
    assert_eq!(mail.fill.as_deref(), Some("999999"));
    assert_eq!(
        mail.text_color.as_deref(),
        Some("000000"),
        "black reads on gray"
    );
    let bank = find(&d.root, "bank");
    assert!(bank.composite && bank.border_dash.is_some());
    assert!(bank.label.value.contains("[System]"));
    let web = find(&bank.children, "web");
    assert!(web.label.value.contains("[Container: Java, Spring MVC]"));
    assert_eq!(find(&bank.children, "db").shape, Shape::Database);
    assert_eq!(find(&bank.children, "events").shape, Shape::Queue);
    // The legend: one box of each kind drawn.
    let legend = find(&d.root, "c4_legend");
    let told: Vec<&str> = legend
        .children
        .nodes
        .iter()
        .map(|node| node.label.value.as_str())
        .collect();
    assert_eq!(
        told,
        [
            "<size:12>person</size>",
            "<size:12>external system</size>",
            "<size:12>container</size>"
        ]
    );
}

#[test]
fn relationships_keep_their_direction_and_their_words() {
    let d = diagram(BANK);
    let bank = find(&d.root, "bank");
    let web = |group: &Group| group.nodes.iter().position(|n| n.key == "web").unwrap();
    // Within the boundary: right of the web application, and down.
    let inside: Vec<(usize, usize, Option<bool>, Head, Head)> = bank
        .children
        .edges
        .iter()
        .map(|e| (e.from, e.to, e.level, e.from_head, e.to_head))
        .collect();
    let w = web(&bank.children);
    assert_eq!(
        inside,
        [
            (w, 1, Some(true), Head::None, Head::Arrow),
            (w, 2, None, Head::Arrow, Head::Arrow),
        ]
    );
    let label = bank.children.edges[0].label.as_ref().unwrap();
    assert!(label.value.contains("<b>Reads from and writes to</b>"));
    assert!(label.value.contains("<i>[JDBC]</i>"));
    // Back: from the mail system to the customer, the customer first.
    let customer = d
        .root
        .nodes
        .iter()
        .position(|n| n.key == "customer")
        .unwrap();
    let mail = d.root.nodes.iter().position(|n| n.key == "mail").unwrap();
    let back = d
        .root
        .edges
        .iter()
        .find(|e| {
            e.label
                .as_ref()
                .is_some_and(|l| l.value.contains("e-mails"))
        })
        .unwrap();
    assert_eq!(
        (back.from, back.to, back.from_head, back.to_head, back.dash),
        (customer, mail, Head::Arrow, Head::None, Dash::Solid)
    );
    // `Lay_R` between an element and a boundary at one level: held.
    assert_eq!(d.root.held.len(), 1);
}

#[test]
fn a_direction_across_the_flow_between_two_boundaries_is_dropped() {
    let source = "@startuml
!include <C4/C4_Container>
System_Boundary(a, \"A\") {
  Container(x, \"X\")
}
System_Boundary(b, \"B\") {
  Container(y, \"Y\")
}
Rel_R(x, y, \"calls\")
Lay_R(x, y)
Rel_D(x, y, \"along\")
@enduml
";
    let d = diagram(source);
    let told: Vec<Option<bool>> = d.root.crossings.iter().map(|c| c.arrow.level).collect();
    assert_eq!(told, [None, None], "two lines; the hidden one is gone");
}

#[test]
fn what_it_does_not_draw_is_declined_by_name() {
    for (body, line, part) in [
        ("Person(a, \"A\", $sprite=\"robot\")", 2, "robot"),
        ("Person(a, \"A\", $sprite=\"bolt\")", 2, "no sprite"),
        (
            "Container(a, \"A\", \"Go\", \"D\", \"img:x.png\")",
            2,
            "image",
        ),
        ("System(a, \"A\", $tags=\"v1\")", 2, "tags"),
        (
            "AddElementTag(\"v1\", $bgColor=\"#d73027\")",
            2,
            "tags and styles",
        ),
        (
            "UpdateElementStyle(person, $bgColor=\"red\")",
            2,
            "tags and styles",
        ),
        ("Person(\"a b\", \"A\")", 2, "name"),
        ("Person(a, \"A \\\"q\\\"\")", 2, "quote"),
        (
            "Person(a, \"A\")\nPerson(b, \"B\")\nRel(a, b, \"x\", $link=\"https://x\")",
            4,
            "link",
        ),
    ] {
        let source = format!("@startuml\n!include <C4/C4_Context>\n{body}\n@enduml\n");
        let err = crate::parse(&source).unwrap_err();
        assert_eq!(err.line, Some(line), "{body}: {err:?}");
        assert!(err.reason.contains(part), "{body}: {}", err.reason);
    }
}

#[test]
fn hints_that_only_move_boxes_and_plain_plantuml_are_read() {
    let source = "@startuml
!include <C4/C4_Context>
LAYOUT_LEFT_RIGHT()
HIDE_STEREOTYPE()
LAYOUT_AS_SKETCH()
skinparam shadowing false
Person(a, \"A\")
System(s, \"S\", $link=\"https://example.org/s\")
note right of s : a plain note
Rel_D(a, s, \"uses\")
@enduml
";
    let d = diagram(source);
    assert_eq!(d.root.direction, crate::structure::Direction::Right);
    assert!(
        d.frame
            .skinparams
            .iter()
            .any(|p| p.key == "handwritten" && p.value == "true")
    );
    assert_eq!(d.root.notes.len(), 1);
    let s = find(&d.root, "s");
    assert_eq!(s.link.as_ref().unwrap().url, "https://example.org/s");
    // Where the flow runs to the right, down is across it.
    assert_eq!(d.root.edges[0].level, Some(true));
    let svg = crate::render(source).unwrap();
    assert!(svg.contains(">A<") && svg.contains(">S<"), "{svg}");
}

#[test]
fn every_border_reads_on_a_light_page_and_a_dark_one() {
    // White and Catppuccin Latte's page; Tokyo Night's and Everforest's.
    let pages = [0xFFFFFF, 0xEFF1F5, 0x16181D, 0x2D353B].map(Color::hex);
    for level in [
        Level::Person,
        Level::System,
        Level::Container,
        Level::Component,
    ] {
        for external in [false, true] {
            let (fill, border) = colors(level, external);
            let border = color::parse(border).unwrap().unwrap();
            for page in pages {
                assert!(border.contrast(page) >= 3.0, "{level:?} {external} {page}");
            }
            let fill = color::parse(fill).unwrap().unwrap();
            let ink = color::parse(ink_on(&fill.to_string()[1..]))
                .unwrap()
                .unwrap();
            assert!(ink.contrast(fill) >= 4.5, "{level:?} {external}");
        }
    }
}

#[test]
fn an_empty_argument_keeps_the_places_of_those_after_it() {
    let source = "@startuml
!include <C4/C4_Container>
Container(api, \"API\", \"\", \"Handles requests\")
Container(cfg, \"Config\", \"Rust\", \"Reads key=value pairs\")
Rel(api, cfg, \"\", \"HTTPS\")
@enduml
";
    let d = diagram(source);
    let api = find(&d.root, "api");
    assert!(
        api.label.value.contains("[Container]"),
        "{}",
        api.label.value
    );
    assert!(
        api.label.value.contains("Handles requests"),
        "{}",
        api.label.value
    );
    let cfg = find(&d.root, "cfg");
    assert!(
        cfg.label.value.contains("Reads key=value pairs"),
        "{}",
        cfg.label.value
    );
    let rel = d.root.edges[0].label.as_ref().unwrap();
    assert!(!rel.value.contains("<b>HTTPS</b>"), "{}", rel.value);
    assert!(rel.value.contains("<i>[HTTPS]</i>"), "{}", rel.value);
}

#[test]
fn each_boundary_takes_its_arguments_in_its_own_order() {
    let source = "@startuml
!include <C4/C4_Container>
System_Boundary(bank, \"Bank\", \"\", \"https://example.org/bank\") {
  Container(web, \"Web\")
}
Boundary(zone, \"Zone\", \"Region\") {
  Container(db, \"Data\")
}
@enduml
";
    let d = diagram(source);
    let bank = find(&d.root, "bank");
    assert!(
        bank.label.value.contains("[System]"),
        "{}",
        bank.label.value
    );
    assert_eq!(bank.link.as_ref().unwrap().url, "https://example.org/bank");
    assert!(find(&d.root, "zone").label.value.contains("[Region]"));
    let err = crate::parse(
        "@startuml\n!include <C4/C4_Container>\nSystem_Boundary(b, \"Bank\", \"v1\") {\n}\n@enduml\n",
    )
    .unwrap_err();
    assert!(err.reason.contains("tags"), "{}", err.reason);
}

#[test]
fn a_block_inside_a_boundary_closes_by_itself() {
    let source = "@startuml
!include <C4/C4_Container>
System_Boundary(b, \"B\") {
  package P {
    Container(c, \"C\")
  }
  Container(d, \"D\")
}
@enduml
";
    let d = diagram(source);
    let b = find(&d.root, "b");
    assert!(
        b.children.nodes.iter().any(|n| n.key == "d"),
        "d stays in b"
    );
    // The writer knows `d` is in `b`: a direction to it from inside `b`
    // is not dropped as one between two boundaries.
    let mut writer = Writer::default();
    for (no, text) in source.lines().enumerate().skip(1) {
        writer
            .line(&Line {
                no,
                text: text.trim(),
                indent: 0,
                blank_before: 0,
            })
            .unwrap();
    }
    assert_eq!(writer.home["d"], ["b"]);
    assert_eq!(writer.home["c"], ["b"]);
}

#[test]
fn the_bank_is_drawn() {
    let svg = crate::render(BANK).unwrap();
    for words in [
        "Customer",
        "Web application",
        "[Container: Java, Spring MVC]",
        "Legend",
    ] {
        assert!(svg.contains(words), "{words}");
    }
}
