use mermaid_rs_renderer::{RenderOptions, parse_mermaid_strict};

use omaspec_uml::sequence::{
    Diagram, Head, Item, Mark, Message, Shape, from_mermaid as build, mermaid_refusal,
};

/// The SVG alone, without the map.
fn try_draw(
    graph: &mermaid_rs_renderer::Graph,
    theme: &mermaid_rs_renderer::Theme,
    source: &str,
) -> Option<String> {
    super::try_draw(graph, theme, source).map(|(svg, _)| svg)
}

fn built(source: &str) -> Diagram {
    build(source).unwrap_or_else(|| panic!("declined {source}"))
}

fn messages(diagram: &Diagram) -> Vec<&Message> {
    diagram
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Message(message) => Some(message),
            _ => None,
        })
        .collect()
}

#[test]
fn an_order_uses_the_alias_and_keeps_the_note_outside_the_alt() {
    let source = include_str!("../../../../../examples/order.mmd");
    let diagram = built(source);
    assert_eq!(diagram.participants[0].label, "Customer");
    assert!(!diagram.footbox);
    let paid = diagram.items.iter().position(
        |item| matches!(item, Item::GroupStart { label: Some(text), .. } if text.value == "paid"),
    );
    let afterwards = diagram
        .items
        .iter()
        .position(|item| matches!(item, Item::Note(note) if note.text.value.contains("cart")));
    let end = diagram
        .items
        .iter()
        .rposition(|item| matches!(item, Item::GroupEnd { .. }));
    assert!(afterwards.unwrap() < paid.unwrap());
    assert!(end.unwrap() < diagram.items.len());
    let theme = RenderOptions::default().theme;
    let svg = try_draw(&parse_mermaid_strict(source).unwrap().graph, &theme, source).unwrap();
    assert!(svg.to_ascii_lowercase().contains("#f8fafc"), "{svg}");
    assert_eq!(svg.matches(">Customer<").count(), 1);
    let customer = baseline(&svg, "Customer");
    let alt = baseline(&svg, "alt");
    let paid_y = baseline(&svg, "[paid]");
    assert!(
        customer < alt && alt <= paid_y + 0.1,
        "{customer} {alt} {paid_y}"
    );
}

#[test]
fn arrows_keep_the_head_mermaid_gives_them() {
    let diagram = built(
        "sequenceDiagram\nA->B: plain\nA->>B: call\nB-->>A: back\nA-xB: lost\nA--)B: async\nA<<->>B: both\n",
    );
    let arrows: Vec<_> = messages(&diagram)
        .into_iter()
        .map(|message| {
            (
                message.arrow.dashed,
                message.arrow.head,
                message.arrow.tail,
                message.arrow.end_mark,
            )
        })
        .collect();
    assert_eq!(
        arrows,
        vec![
            (false, Head::None, Head::None, Mark::None),
            (false, Head::Filled, Head::None, Mark::None),
            (true, Head::Filled, Head::None, Mark::None),
            (false, Head::None, Head::None, Mark::Cross),
            (true, Head::Open, Head::None, Mark::None),
            (false, Head::Filled, Head::Filled, Mark::None),
        ]
    );
}

#[test]
fn a_plus_activates_the_receiver_and_a_minus_the_sender() {
    let diagram = built("sequenceDiagram\nA->>+B: go\nB-->>-A: back\nB->>C+: again\n");
    let kinds: Vec<_> = diagram
        .items
        .iter()
        .map(|item| match item {
            Item::Activate { participant, .. } => format!("on {participant}"),
            Item::Deactivate { participant, .. } => format!("off {participant}"),
            Item::Message(_) => "msg".into(),
            _ => "other".into(),
        })
        .collect();
    assert_eq!(kinds, vec!["msg", "on 1", "msg", "off 1", "msg", "on 2"]);
}

#[test]
fn an_actor_is_an_actor_and_autonumber_counts() {
    let diagram =
        built("sequenceDiagram\nautonumber 3 2\nactor U as User\nU->>S: login\nU->>S: again\n");
    assert_eq!(diagram.participants[0].shape, Shape::Actor);
    assert_eq!(diagram.participants[0].label, "User");
    let numbers: Vec<_> = messages(&diagram)
        .into_iter()
        .map(|message| message.number.clone())
        .collect();
    assert_eq!(numbers, vec![Some("3".into()), Some("5".into())]);
}

#[test]
fn a_note_after_a_group_follows_it() {
    let diagram =
        built("sequenceDiagram\nalt choice\nA->>B: yes\nend\nNote right of A: afterwards\n");
    let end = diagram
        .items
        .iter()
        .position(|item| matches!(item, Item::GroupEnd { .. }))
        .unwrap();
    assert!(matches!(diagram.items[end + 1], Item::Note(_)));
}

#[test]
fn a_rect_is_a_highlight_and_create_draws_the_head_on_the_message() {
    let theme = RenderOptions::default().theme;
    let rect = "sequenceDiagram\nrect rgb(200, 200, 200)\nA->>B: hi\nend\n";
    let diagram = built(rect);
    assert!(diagram.items.iter().any(|item| matches!(
        item,
        Item::RectStart { color, .. } if color == "rgb(200, 200, 200)"
    )));
    assert!(mermaid_refusal(rect).is_none());
    let parsed = parse_mermaid_strict(rect).unwrap();
    let svg = try_draw(&parsed.graph, &theme, rect).unwrap();
    assert!(svg.contains("#C8C8C8"), "{svg}");

    let nested = built("sequenceDiagram\nrect red\nrect blue\nA->>B: hi\nend\nend\n");
    let kinds: Vec<_> = nested
        .items
        .iter()
        .map(|item| match item {
            Item::RectStart { color, .. } => format!("rect {color}"),
            Item::RectEnd { .. } => "end".into(),
            Item::Message(_) => "msg".into(),
            _ => "other".into(),
        })
        .collect();
    assert_eq!(kinds, ["rect red", "rect blue", "msg", "end", "end"]);

    let create = "sequenceDiagram\ncreate participant B\nA->>B: hi\n";
    let diagram = built(create);
    assert!(messages(&diagram)[0].creates);
    assert_eq!(diagram.participants[0].code, "B");
    let parsed = parse_mermaid_strict(create).unwrap();
    let svg = try_draw(&parsed.graph, &theme, create).unwrap();
    assert!(baseline(&svg, "B") > baseline(&svg, "A") + 10.0, "{svg}");

    let actor = built("sequenceDiagram\ncreate actor D as Donald\nA->>D: hi\n");
    assert_eq!(actor.participants[0].shape, Shape::Actor);
    assert_eq!(actor.participants[0].label, "Donald");
    assert!(messages(&actor)[0].creates);

    let gone = built("sequenceDiagram\ndestroy B\nA->>B: hi\n");
    assert!(matches!(gone.items[0], Item::Message(_)));
    assert!(matches!(
        gone.items[1],
        Item::Destroy { participant: 0, .. }
    ));
    assert_eq!(gone.participants[0].code, "B");
    let source = "sequenceDiagram\ndestroy B\nA->>B: hi\n";
    let parsed = parse_mermaid_strict(source).unwrap();
    assert!(try_draw(&parsed.graph, &theme, source).is_some());
}

#[test]
fn a_participant_annotation_sets_the_shape_and_the_label() {
    let database =
        built("sequenceDiagram\nparticipant Alice@{ \"type\" : \"database\" }\nAlice->>B: hi\n");
    assert_eq!(database.participants[0].shape, Shape::Database);
    assert_eq!(database.participants[0].code, "Alice");

    let boundary = built(
        "sequenceDiagram\nparticipant API@{ \"type\": \"boundary\" } as Public API\nAPI->>B: hi\n",
    );
    assert_eq!(boundary.participants[0].code, "API");
    assert_eq!(boundary.participants[0].label, "Public API");
    assert_eq!(boundary.participants[0].shape, Shape::Boundary);

    let alias = built("sequenceDiagram\nparticipant A@{ \"alias\": \"Ann\" }\nA->>B: hi\n");
    assert_eq!(alias.participants[0].label, "Ann");
    assert_eq!(alias.participants[0].code, "A");

    let boxed =
        built("sequenceDiagram\nbox rgba(0, 0, 255, .1) Friends\nparticipant A\nend\nA->>B: hi\n");
    assert_eq!(boxed.boxes[0].color.as_deref(), Some("rgba(0, 0, 255, .1)"));
    assert_eq!(boxed.boxes[0].title.as_ref().unwrap().value, "Friends");
    // The translucent fill is mixed onto the background while measuring.
    assert!(omaspec_uml::sequence::draw(&boxed).is_ok());
}

#[test]
fn an_unclosed_annotation_or_a_bad_color_falls_back() {
    let theme = RenderOptions::default().theme;
    let source = "sequenceDiagram\nA->>B: hi @{\n";
    assert!(build(source).is_none());
    assert_eq!(mermaid_refusal(source), Some((1, "a @{ } annotation")));
    let parsed = parse_mermaid_strict(source).unwrap();
    assert!(try_draw(&parsed.graph, &theme, source).is_none());
    assert!(crate::mermaid::render(source).is_ok(), "{source}");

    let color = "sequenceDiagram\nrect #zzzzzz\nA->>B: hi\nend\n";
    assert!(build(color).is_none());
    assert_eq!(mermaid_refusal(color), Some((1, "a color")));
    assert!(build("sequenceDiagram\ncreate B\nA->>B: hi\n").is_none());
    assert!(build("sequenceDiagram\ncreate participant B\nB->>A: hi\n").is_none());
    assert!(build("sequenceDiagram\ncreate participant B\n").is_none());
    assert!(build("sequenceDiagram\nparticipant A@{ \"foo\": \"bar\" }\nA->>B: hi\n").is_none());
}

#[test]
fn broken_sequence_text_does_not_panic() {
    let source = include_str!("../../../../../examples/order.mmd");
    let theme = RenderOptions::default().theme;
    let mut state = 0x1234_5678u32;
    let bytes = source.as_bytes();
    for _ in 0..80 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let mut owned = bytes.to_vec();
        let at = (state as usize) % owned.len();
        owned[at] ^= 0x20;
        let text = String::from_utf8_lossy(&owned).into_owned();
        let drew = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if let Ok(parsed) = parse_mermaid_strict(&text) {
                let _ = try_draw(&parsed.graph, &theme, &text);
            }
        }));
        assert!(drew.is_ok());
    }
}

fn baseline(svg: &str, word: &str) -> f32 {
    let doc = roxmltree::Document::parse(svg).unwrap();
    doc.descendants()
        .find(|node| node.has_tag_name("tspan") && node.text() == Some(word))
        .and_then(|node| node.attribute("y"))
        .unwrap_or_else(|| panic!("missing {word}"))
        .parse()
        .unwrap()
}

#[test]
fn the_font_list_keeps_a_family_that_is_installed() {
    // mmdr asks for Inter first; on most systems the list's sans-serif draws.
    let list =
        super::font_name("Inter, ui-sans-serif, \"Segoe UI\", \"Liberation Sans\", sans-serif")
            .unwrap();
    assert_eq!(
        list,
        "Inter, ui-sans-serif, Segoe UI, Liberation Sans, sans-serif"
    );
    assert_eq!(
        super::font_name("'Fira Sans'").as_deref(),
        Some("Fira Sans")
    );
    assert_eq!(super::font_name(" , "), None);
    assert_eq!(super::font_name("Evil<script>"), None);
}
