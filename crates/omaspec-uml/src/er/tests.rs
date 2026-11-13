use super::*;

use super::paint::draw;

fn diagram(source: &str) -> super::Diagram {
    from_mermaid(source).unwrap_or_else(|| panic!("refused {source}"))
}

#[test]
fn reads_the_fixture_and_draws_the_foot() {
    let diagram = diagram(include_str!("../../../omaspec-core/tests/fixtures/er.mmd"));
    assert_eq!(diagram.entities.len(), 2);
    assert_eq!(diagram.entities[0].key, "PACKAGE");
    assert_eq!(diagram.entities[0].attributes.len(), 2);
    assert_eq!(diagram.entities[0].attributes[0].name, "name");
    assert_eq!(diagram.entities[1].key, "FILE");
    assert_eq!(diagram.relations.len(), 1);
    assert_eq!(diagram.relations[0].from_card, Some(Card::One));
    assert_eq!(diagram.relations[0].to_card, Some(Card::ZeroMany));
    assert_eq!(diagram.relations[0].label.as_deref(), Some("contains"));
    let short = from_mermaid("erDiagram\n  A |--o B : has\n").unwrap();
    assert_eq!(short.relations[0].from_card, Some(Card::One));
    assert_eq!(short.relations[0].to_card, Some(Card::ZeroOne));
    let svg = draw(&diagram).unwrap();
    assert!(
        svg.contains("PACKAGE")
            && svg.contains("FILE")
            && svg.contains("contains")
            && svg.contains("name")
            && svg.contains("version")
            && svg.contains("path")
            && svg.contains("<ellipse"),
        "{svg}"
    );
    let laid = super::lay::lay(&diagram, crate::text::Family::Sans).unwrap();
    let a = laid.boxes[0];
    let b = laid.boxes[1];
    let font = crate::text::Font::new(diagram.look.font_size, crate::Color::BLACK);
    let label = crate::text::block("contains", font, crate::text::Family::Sans).unwrap();
    let gap = if a.x < b.x {
        b.x - a.right()
    } else {
        a.x - b.right()
    };
    assert!(
        gap >= label.width + 8.0,
        "label does not fit between the boxes: gap {gap}, {a:?} {b:?}"
    );
}

#[test]
fn a_self_loop_stays_off_the_neighbor() {
    let check = |source: &str| {
        let diagram = diagram(source);
        let laid = super::lay::lay(&diagram, crate::text::Family::Sans).unwrap();
        let index = |key: &str| {
            diagram
                .entities
                .iter()
                .position(|entity| entity.key == key)
                .unwrap()
        };
        let a = laid.boxes[index("A")];
        let b = laid.boxes[index("B")];
        let reach = super::paint::loop_reach(0, Some(Card::One), Some(Card::ZeroMany));
        let font = crate::text::Font::new(diagram.look.font_size, crate::Color::BLACK);
        let label = crate::text::block("parent", font, crate::text::Family::Sans).unwrap();
        let clear = reach + 6.0;
        match diagram.axis {
            super::Axis::Right if !diagram.flip => {
                assert!(a.right() + clear + label.width < b.x, "{a:?} {b:?}");
            }
            super::Axis::Right => {
                assert!(b.right() + clear + label.width < a.x, "{a:?} {b:?}");
            }
            super::Axis::Down if !diagram.flip => {
                assert!(a.bottom() + clear + label.height < b.y, "{a:?} {b:?}");
            }
            super::Axis::Down => {
                assert!(b.bottom() + clear + label.height < a.y, "{a:?} {b:?}");
            }
        }
        let _ = draw(&diagram).unwrap();
    };
    check("erDiagram\n  A ||--o{ A : parent\n  A ||--|| B : owns\n");
    check("erDiagram\n  direction RL\n  A ||--o{ A : parent\n  A ||--|| B : owns\n");
    check("erDiagram\n  direction TB\n  A ||--o{ A : parent\n  A ||--|| B : owns\n");
    check("erDiagram\n  direction BT\n  A ||--o{ A : parent\n  A ||--|| B : owns\n");
}

#[test]
fn two_self_loops_lie_around_one_another_with_their_words_apart() {
    let svg = draw(&diagram(
        "erDiagram\n  A ||--o{ A : manages\n  A ||--o{ A : mentors\n  A }o--|| B : in\n",
    ))
    .unwrap();
    // Each word once, and the chips of the two at different heights.
    let chip_y = |word: &str| -> f32 {
        let at = svg
            .find(&format!(">{word}<"))
            .unwrap_or_else(|| panic!("{word}: {svg}"));
        let tag = svg[..at].rsplit('<').next().unwrap();
        let y = tag.split(" y=\"").nth(1).unwrap();
        y.split('"').next().unwrap().parse().unwrap()
    };
    let (manages, mentors) = (chip_y("manages"), chip_y("mentors"));
    assert!((manages - mentors).abs() > 12.0, "{manages} {mentors}");
    // The loops leave the box below its middle, where the line to B
    // leaves, and lie around one another.
    let d = diagram("erDiagram\n  A ||--o{ A : manages\n  A ||--o{ A : mentors\n");
    let laid = super::lay::lay(&d, crate::text::Family::Sans).unwrap();
    let a = laid.boxes[0];
    let away = super::paint::loop_outward(d.axis, d.flip);
    let (inner_start, inner_end) = super::paint::loop_ports(a, away, 0);
    let (outer_start, outer_end) = super::paint::loop_ports(a, away, 1);
    assert!(inner_start.1 > a.cy() && inner_end.1 > inner_start.1);
    assert!(outer_start.1 < inner_start.1 && outer_end.1 > inner_end.1);
    assert!(outer_end.1 <= a.bottom());
}

#[test]
fn parallel_lines_take_different_ports_and_bends() {
    let from = super::lay::Rect {
        x: 0.0,
        y: 0.0,
        w: 120.0,
        h: 90.0,
    };
    let to = super::lay::Rect {
        x: 280.0,
        y: 10.0,
        w: 100.0,
        h: 70.0,
    };
    let upper = super::route::port(from, to, 0, 3);
    let lower = super::route::port(from, to, 2, 3);
    assert!(lower.1 - upper.1 >= 28.0, "{upper:?} {lower:?}");
    let a = super::route::route(from, to, 0, 3, 0, 2);
    let b = super::route::route(from, to, 1, 3, 1, 2);
    assert!(a.len() == 4 && b.len() == 4, "{a:?} {b:?}");
    assert!((a[1].0 - b[1].0).abs() > 8.0, "{a:?} {b:?}");
    assert!((a[0].1 - b[0].1).abs() > 8.0, "{a:?} {b:?}");
}

#[test]
fn a_key_a_comment_and_a_style_stay_on_the_entity() {
    let source = "\
erDiagram
direction TB
A ||--o| B : uses
A {
    string id PK \"primary\"
}
style A fill:#112233,stroke:#e68e0d,stroke-width:4px,color:#bebebe,stroke-dasharray: 5,5
";
    let diagram = diagram(source);
    assert_eq!(diagram.axis, Axis::Down);
    assert_eq!(diagram.entities[0].attributes[0].keys, vec![Key::Pk]);
    assert_eq!(
        diagram.entities[0].attributes[0].comment.as_deref(),
        Some("primary")
    );
    assert_eq!(diagram.entities[0].fill, Some(crate::Color::hex(0x112233)));
    assert_eq!(diagram.entities[0].thickness, Some(4.0));
    assert_eq!(diagram.entities[0].dash, Some((5.0, 5.0)));
    let svg = draw(&diagram).unwrap();
    assert!(
        svg.contains("PK")
            && svg.contains("primary")
            && svg.contains("#112233")
            && svg.contains("stroke-width=\"4\"")
            && svg.contains("stroke-dasharray=\"5,5\""),
        "{svg}"
    );
}

#[test]
fn a_loop_and_a_mirror_still_draw() {
    let diagram =
        diagram("erDiagram\n  direction RL\n  A ||--o{ A : parent\n  A ||--|| B : owns\n");
    assert!(diagram.flip);
    let svg = draw(&diagram).unwrap();
    assert!(
        svg.contains("parent") && svg.contains("owns") && svg.contains("A"),
        "{svg}"
    );
}

#[test]
fn a_bad_cardinality_a_style_and_an_open_entity_are_named() {
    assert_eq!(
        mermaid_refusal("erDiagram\n  A xx--o{ B : no\n"),
        Some((1, "a relationship"))
    );
    assert_eq!(
        mermaid_refusal("erDiagram\n  A {\n    string id\n"),
        Some((1, "an entity"))
    );
    assert_eq!(
        mermaid_refusal("erDiagram\n  style A opacity:1\n"),
        Some((1, "a style"))
    );
    assert_eq!(
        mermaid_refusal("erDiagram\n  direction sideways\n"),
        Some((1, "a direction"))
    );
    assert!(from_mermaid("sequenceDiagram\nA->>B: hi\n").is_none());
    assert_eq!(mermaid_refusal("sequenceDiagram\nA->>B: hi\n"), None);
}
