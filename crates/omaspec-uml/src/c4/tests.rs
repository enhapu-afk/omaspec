use super::*;

use super::lay::lay;
use super::paint::draw;
use crate::text::Family;

fn diagram(source: &str) -> super::Diagram {
    from_mermaid(source).unwrap_or_else(|| panic!("refused {source}"))
}

#[test]
fn reads_a_context_and_draws_the_marks() {
    let source = "\
C4Container
title Shop
Person(customer, \"Customer\", \"A customer\")
System_Boundary(shop, \"Shop\") {
  Container(web, \"Web\", \"Rust\", \"Pages\")
  ContainerDb(db, \"DB\", \"SQL\", \"Rows\")
  ContainerQueue(q, \"Bus\", \"Nats\", \"Events\")
  Rel(web, db, \"Reads\")
}
Rel(customer, web, \"Uses\")
";
    let diagram = diagram(source);
    assert_eq!(
        diagram.title.as_ref().map(|title| title.value.as_str()),
        Some("Shop")
    );
    assert_eq!(diagram.shapes.len(), 4);
    assert_eq!(diagram.shapes[0].kind, Kind::Person);
    assert_eq!(diagram.shapes[2].kind, Kind::ContainerDb);
    assert_eq!(diagram.shapes[3].kind, Kind::ContainerQueue);
    assert_eq!(diagram.boundaries.len(), 1);
    assert_eq!(diagram.shapes[1].parent, Some(0));
    assert_eq!(diagram.rels[1].label, "Uses");
    let svg = draw(&diagram).unwrap();
    assert!(
        svg.contains("Shop")
            && svg.contains("Customer")
            && svg.contains("Uses")
            && svg.contains("<ellipse")
            && svg.contains("<path")
            && svg.contains("#08427B")
            && svg.contains("#438DD5")
            && svg.contains("stroke-dasharray"),
        "{svg}"
    );
    let laid = lay(&diagram, Family::Sans).unwrap();
    assert!(laid.shapes[1].rect.x > laid.boundaries[0].rect.x);
    assert!(laid.shapes[1].rect.right() < laid.boundaries[0].rect.right());
    assert!(clear(&laid), "a label or a line crosses a box");
}

#[test]
fn a_busy_chart_keeps_lines_and_labels_off_the_boxes() {
    let source = "\
C4Container
title Banking
Person(customer, \"Customer\", \"A customer of the bank\")
Person_Ext(auditor, \"Auditor\", \"External reviewer\")
System_Boundary(bank, \"Internet Banking\") {
  Container(web, \"Web\", \"React\", \"Serves the UI\")
  ContainerDb(db, \"Database\", \"SQL\", \"Stores accounts\")
  ContainerQueue(q, \"Bus\", \"Kafka\", \"Events\")
  Component(api, \"API\", \"Rust\", \"Handles requests\")
  Rel(web, db, \"Reads\", \"JDBC\")
  Rel(web, q, \"Publishes\")
  Rel_Back(db, web, \"Migrates\")
}
Rel(customer, web, \"Uses\")
BiRel(auditor, web, \"Reviews\")
";
    let diagram = diagram(source);
    let laid = lay(&diagram, Family::Sans).unwrap();
    let api = &laid.shapes[5].rect;
    let boundary = &laid.boundaries[0].rect;
    assert!(api.x > boundary.x && api.right() < boundary.right());
    assert!(api.y > boundary.y && api.bottom() < boundary.bottom());
    assert!(clear(&laid), "a label or a line crosses a box");
}

#[test]
fn a_light_component_keeps_dark_text_and_a_back_link_is_dashed() {
    let component = draw(&diagram(
        "C4Component\nComponent(api, \"API\", \"Rust\", \"Handles\")\n",
    ))
    .unwrap();
    assert!(
        component.contains("#85BBF0")
            && component.contains("fill=\"#333333\"")
            && component.contains("API"),
        "{component}"
    );
    let back = draw(&diagram(
        "C4Context\nPerson(a, \"A\")\nSystem(s, \"S\")\nRel_Back(a, s, \"Back\")\nBiRel(a, s, \"Both\")\n",
    ))
    .unwrap();
    assert!(
        back.contains("stroke-dasharray") && back.matches("<polygon").count() >= 3,
        "{back}"
    );
    let up = diagram("C4Context\nPerson(a, \"A\")\nPerson(b, \"B\")\nRel_Up(a, b, \"Up\")\n");
    assert_eq!(up.rels[0].kind, RelKind::Up);
    let laid = lay(&up, Family::Sans).unwrap();
    assert!(laid.rels[0].points[1].1 < laid.rels[0].points[0].1);
}

#[test]
fn a_dark_page_steps_the_accent_instead_of_the_classic_blues() {
    let mut diagram =
        diagram("C4Context\nPerson(a, \"A\")\nSystem(s, \"S\")\nComponent(c, \"C\")\n");
    diagram.look.classic = false;
    diagram.look.accent = super::Color::hex(0xE68E0D);
    diagram.look.background = Some(super::Color::hex(0x121212));
    diagram.look.preferred = super::Color::hex(0xBEBEBE);
    diagram.look.line = super::Color::hex(0xE68E0D);
    let svg = draw(&diagram).unwrap();
    assert!(
        svg.contains("#E68E0D")
            && svg.contains("#AB6B0E")
            && svg.contains("#563A10")
            && svg.contains("#BEBEBE")
            && !svg.contains("#08427B")
            && !svg.contains("#1168BD")
            && !svg.contains("#438DD5")
            && !svg.contains("#85BBF0"),
        "{svg}"
    );
}

#[test]
fn a_sprite_a_boundary_and_a_bad_line_are_named() {
    let sprite = mermaid_refusal("C4Context\nPerson(admin, \"Admin\", $sprite=users)\n").unwrap();
    assert_eq!(sprite, (1, "a sprite"));
    let open = mermaid_refusal("C4Context\nBoundary(b, \"B\") {\nPerson(a, \"A\")\n").unwrap();
    assert_eq!(open.1, "a boundary");
    let rel = mermaid_refusal("C4Context\nPerson(a, \"A\")\nRel(a, missing, \"Uses\")\n").unwrap();
    assert_eq!(rel.1, "a relationship");
    let color =
        mermaid_refusal("C4Context\nPerson(a, \"A\")\nUpdateElementStyle(a, not-a-color)\n")
            .unwrap();
    assert_eq!(color.1, "a color");
    let style =
        mermaid_refusal("C4Context\nPerson(a, \"A\")\nUpdateElementStyle(a, #fff, $opacity=1)\n")
            .unwrap();
    assert_eq!(style.1, "a style");
    let layout = mermaid_refusal("C4Context\nUpdateLayoutConfig(0)\n").unwrap();
    assert_eq!(layout.1, "a layout");
    let unknown = mermaid_refusal("C4Context\ndirection LR\n").unwrap();
    assert_eq!(unknown.1, "an unknown line");
    let element = mermaid_refusal("C4Context\nFoo(a, \"A\")\n").unwrap();
    assert_eq!(element.1, "an element");
    assert!(from_mermaid("sequenceDiagram\nA->B: hi\n").is_none());
    assert!(mermaid_refusal("sequenceDiagram\nA->B: hi\n").is_none());
    let styled = diagram("C4Context\nPerson(a, \"Admin\")\nUpdateElementStyle(a, #112233)\n");
    assert_eq!(styled.shapes[0].fill, Some(super::Color::hex(0x112233)));
    let named =
        diagram("C4Context\nPerson(a, \"Admin\")\nUpdateElementStyle(a, $bgColor=\"#112233\")\n");
    assert_eq!(named.shapes[0].fill, Some(super::Color::hex(0x112233)));
    let svg = draw(&styled).unwrap();
    assert!(svg.contains("#112233"), "{svg}");
}

fn clear(laid: &super::lay::Laid) -> bool {
    let headers = super::lay::header_bands(&laid.boundaries);
    for (index, rel) in laid.rels.iter().enumerate() {
        for shape in &laid.shapes {
            if rel.chip.inflate(1.0).intersects(shape.rect) {
                return false;
            }
        }
        for header in &headers {
            if rel.chip.inflate(1.0).intersects(*header) {
                return false;
            }
        }
        for (other_index, other) in laid.rels.iter().enumerate() {
            if index != other_index && rel.chip.inflate(1.0).intersects(other.chip) {
                return false;
            }
        }
        for pair in rel.points.windows(2) {
            let hits_shape = laid
                .shapes
                .iter()
                .any(|shape| super::route::segment_hits(pair[0], pair[1], shape.rect));
            let hits_header = headers
                .iter()
                .any(|header| super::route::segment_hits(pair[0], pair[1], *header));
            if hits_shape || hits_header {
                return false;
            }
        }
    }
    true
}
