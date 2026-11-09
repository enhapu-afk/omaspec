use super::*;
use crate::graph::Rect;
use crate::tree::Relation;

const SKIN: &str = "@startwbs\n* map\n@endwbs\n";

fn key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn within(inner: Rect, outer: Rect) -> bool {
    inner.x >= outer.x - 0.01
        && inner.y >= outer.y - 0.01
        && inner.x + inner.w <= outer.x + outer.w + 0.01
        && inner.y + inner.h <= outer.y + outer.h + 0.01
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn size(svg: &str) -> Rect {
    let attr = |name: &str| -> f32 {
        let at = svg.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
        svg[at..].split('"').next().unwrap().parse().unwrap()
    };
    Rect {
        x: 0.0,
        y: 0.0,
        w: attr("width"),
        h: attr("height"),
    }
}

/// Cards that say what they show; Checkout is tall, Payment wide.
fn card(key: &str) -> Option<Card> {
    let (size, facts) = match key {
        "checkout" => ((300.0, 600.0), "4 steps"),
        "payment" => ((780.0, 300.0), "3 states"),
        _ => ((420.0, 300.0), "2 states"),
    };
    Some(Card {
        kind: "State machine · PlantUML".into(),
        icon: Icon::State,
        facts: facts.into(),
        size,
    })
}

/// The card of a diagram, by the key of its name.
fn leaf(name: &str) -> Branch {
    Branch {
        words: name.into(),
        key: Some(key(name)),
        below: Vec::new(),
    }
}

fn folder(name: &str, below: Vec<Branch>) -> Branch {
    Branch {
        words: name.into(),
        key: None,
        below,
    }
}

fn leaves(names: &[&str]) -> Vec<Branch> {
    names.iter().map(|name| leaf(name)).collect()
}

fn drawn_in(
    skin: &str,
    boxes: &[Branch],
    relations: &[Relation],
    heading: Option<&Heading>,
) -> (String, crate::map::Map) {
    crate::render_branches(
        skin,
        boxes,
        &Tiles {
            card: &card,
            relations,
            heading,
        },
    )
    .unwrap()
}

fn drawn(boxes: &[Branch], relations: &[Relation]) -> (String, crate::map::Map) {
    drawn_in(SKIN, boxes, relations, None)
}

fn relation(from: &str, to: &str, kind: RelationKind) -> Relation {
    Relation {
        from: from.into(),
        to: to.into(),
        kind,
    }
}

fn plays(from: &str, to: &str) -> Relation {
    relation(from, to, RelationKind::Plays)
}

fn opens(from: &str, to: &str) -> Relation {
    relation(from, to, RelationKind::Opens)
}

/// The rectangles of a map with corners rounded by `rx`, read from its
/// drawing: 11 a pill of words, 20 a folder's frame, 22 the outline of a
/// block.
fn rounded(svg: &str, rx: &str) -> Vec<Rect> {
    svg.split("<rect")
        .skip(1)
        .filter_map(|rest| {
            let tag = rest.split('>').next()?;
            let attr = |name: &str| -> Option<f32> {
                let at = tag.find(&format!(" {name}=\""))? + name.len() + 3;
                tag[at..].split('"').next()?.parse().ok()
            };
            tag.contains(&format!(r#" rx="{rx}""#)).then_some(Rect {
                x: attr("x")?,
                y: attr("y")?,
                w: attr("width")?,
                h: attr("height")?,
            })
        })
        .collect()
}

fn pills(svg: &str) -> Vec<Rect> {
    rounded(svg, "11")
}

fn frames(svg: &str) -> Vec<Rect> {
    rounded(svg, "20")
}

/// How many arrowheads a map draws.
fn heads(svg: &str) -> usize {
    svg.matches(r##"Z" fill="#"##).count()
}

/// The corners of each line of a map's arrows, read from its drawing.
fn arrow_lines(svg: &str) -> Vec<Vec<(f32, f32)>> {
    svg.split("<path d=\"")
        .skip(1)
        .filter_map(|rest| {
            let (d, attrs) = rest.split_once('"')?;
            let attrs = attrs.split('>').next()?;
            attrs.contains("stroke-linejoin").then(|| {
                d.split(|c: char| c.is_ascii_alphabetic() || c == ' ')
                    .filter_map(|pair| {
                        let (x, y) = pair.split_once(',')?;
                        Some((x.parse().ok()?, y.parse().ok()?))
                    })
                    .collect()
            })
        })
        .collect()
}

fn tile<'a>(map: &'a crate::map::Map, key: &str) -> &'a crate::map::Tile {
    map.tiles
        .iter()
        .find(|t| t.key == key)
        .unwrap_or_else(|| panic!("no tile {key}"))
}

/// Every card on the page, its picture in it, and no two on one another.
fn tidy(svg: &str, map: &crate::map::Map) {
    let page = size(svg);
    for tile in &map.tiles {
        assert!(within(tile.picture, tile.frame), "{tile:?}");
        assert!(within(tile.frame, page), "{tile:?} on {page:?}");
    }
    for (i, a) in map.tiles.iter().enumerate() {
        for b in &map.tiles[i + 1..] {
            assert!(!overlap(a.frame, b.frame), "{a:?} on {b:?}");
        }
    }
}

#[test]
fn a_map_shows_each_diagram_as_a_card_under_its_heading() {
    let heading = Heading {
        title: "shop".into(),
        summary: "System map · 4 diagrams".into(),
    };
    let boxes = leaves(&["Shop", "Orders", "Checkout", "Payment"]);
    let (svg, map) = drawn_in(SKIN, &boxes, &[], Some(&heading));
    let keys: Vec<&str> = map.tiles.iter().map(|t| t.key.as_str()).collect();
    assert_eq!(keys, ["shop", "orders", "checkout", "payment"]);
    tidy(&svg, &map);
    for words in [
        ">shop<",
        "System map · 4 diagrams",
        ">Orders<",
        "State machine · PlantUML",
        ">4 steps<",
    ] {
        assert!(svg.contains(words), "{words}: {svg}");
    }
    // The cards lie below the heading, and each picture keeps its shape.
    assert!(map.tiles.iter().all(|t| t.frame.y >= MARGIN + HEADING));
    let (tall, wide) = (
        tile(&map, "checkout").picture,
        tile(&map, "payment").picture,
    );
    assert!(
        tall.h > tall.w && wide.w > 2.0 * wide.h,
        "{tall:?} {wide:?}"
    );
    // No arrows without relations, no frame without a folder.
    assert!(pills(&svg).is_empty() && heads(&svg) == 0 && frames(&svg).is_empty());
    // An empty project is a heading alone.
    let (svg, map) = drawn_in(SKIN, &[], &[], Some(&heading));
    assert!(map.tiles.is_empty() && svg.contains(">shop<"));
}

#[test]
fn a_small_diagram_grows_a_little_at_most() {
    let small = |key: &str| {
        Some(Card {
            kind: String::new(),
            icon: Icon::Other,
            facts: String::new(),
            size: if key == "shop" {
                (100.0, 200.0)
            } else {
                (420.0, 300.0)
            },
        })
    };
    let tiles = Tiles {
        card: &small,
        relations: &[],
        heading: None,
    };
    let (_, map) = crate::render_branches(SKIN, &leaves(&["Shop", "Orders"]), &tiles).unwrap();
    let shop = tile(&map, "shop").picture;
    assert!(
        (shop.w - 125.0).abs() < 0.01 && (shop.h - 250.0).abs() < 0.01,
        "{shop:?}"
    );
}

#[test]
fn the_cards_take_the_colors_of_the_skin() {
    let skin = "@startwbs\nskinparam backgroundColor #1a1b26\nskinparam defaultFontColor #a9b1d6\nskinparam nodeBackgroundColor #24283b\nskinparam arrowColor #7aa2f7\n* map\n@endwbs\n";
    let boxes = leaves(&["Shop", "Orders"]);
    let relations = [opens("shop", "orders")];
    let (svg, _) = drawn_in(skin, &boxes, &relations, None);
    assert!(svg.contains(r##"fill="#1A1B26""##), "the page");
    assert!(svg.contains(r##"fill="#7AA2F7""##), "the accent");
    assert!(svg.contains(r##"fill="#A9B1D6">Shop<"##), "the words");
    // Plain, a light page with a blue accent.
    let (svg, _) = drawn(&boxes, &relations);
    assert!(svg.contains(r##"fill="#FFFFFF""##));
    assert!(svg.contains(r##"fill="#3D6FD6""##));
}

#[test]
fn a_diagram_stands_above_the_one_it_leads_to() {
    let boxes = leaves(&["Checkout", "Orders", "Buy"]);
    let relations = [plays("buy", "orders"), opens("orders", "checkout")];
    let (svg, map) = drawn(&boxes, &relations);
    tidy(&svg, &map);
    let bottom = |key: &str| tile(&map, key).frame.y + tile(&map, key).frame.h;
    let top = |key: &str| tile(&map, key).frame.y;
    assert!(bottom("buy") < top("orders") && bottom("orders") < top("checkout"));
    // Each arrow has its words in a pill between the two, and one head.
    assert_eq!(svg.matches(">plays<").count(), 1);
    assert_eq!(svg.matches(">opens<").count(), 1);
    assert_eq!(heads(&svg), 2);
    let found = pills(&svg);
    assert_eq!(found.len(), 2);
    for pill in &found {
        assert!(
            map.tiles.iter().all(|t| !overlap(*pill, t.frame)),
            "{pill:?}"
        );
    }
    // No line runs under a card.
    for line in arrow_lines(&svg) {
        for run in line.windows(2) {
            let (a, b) = (run[0], run[1]);
            let under = Rect {
                x: a.0.min(b.0) + 1.0,
                y: a.1.min(b.1) + 1.0,
                w: ((a.0 - b.0).abs() - 2.0).max(0.1),
                h: ((a.1 - b.1).abs() - 2.0).max(0.1),
            };
            assert!(
                map.tiles.iter().all(|t| !overlap(under, t.frame)),
                "{a:?} to {b:?}"
            );
        }
    }
    // A pair that is joined both ways, and a card that leads to itself.
    let round = [opens("orders", "checkout"), opens("checkout", "orders")];
    let (svg, map) = drawn(&boxes, &round);
    tidy(&svg, &map);
    assert_eq!(heads(&svg), 2);
    let (svg, _) = drawn(&boxes, &[opens("orders", "orders")]);
    assert_eq!(heads(&svg), 0);
}

#[test]
fn arrows_that_arrive_alike_share_their_words_and_their_head() {
    let boxes = leaves(&["Orders", "Buy", "Return", "Cancel"]);
    let relations = [
        plays("buy", "orders"),
        plays("return", "orders"),
        plays("cancel", "orders"),
    ];
    let (svg, map) = drawn(&boxes, &relations);
    tidy(&svg, &map);
    assert_eq!(svg.matches(">plays<").count(), 1, "{svg}");
    assert_eq!(heads(&svg), 1);
    // The three stand in one row above the one they play.
    let row: Vec<f32> = ["buy", "return", "cancel"]
        .iter()
        .map(|key| tile(&map, key).frame.y)
        .collect();
    assert!(row.iter().all(|y| (y - row[0]).abs() < 1.0), "{row:?}");
    assert!(row[0] < tile(&map, "orders").frame.y);
    // Two kinds for one pair are one arrow that says both.
    let both = [
        plays("buy", "orders"),
        relation("buy", "orders", RelationKind::Includes),
    ];
    let (svg, _) = drawn(&boxes, &both);
    assert!(svg.contains(">plays · includes<"), "{svg}");
    assert_eq!(heads(&svg), 1);
}

#[test]
fn arrows_that_leave_alike_share_their_words() {
    let boxes = leaves(&["Shop", "Orders", "Payment"]);
    let relations = [opens("shop", "orders"), opens("shop", "payment")];
    let (svg, map) = drawn(&boxes, &relations);
    tidy(&svg, &map);
    assert_eq!(svg.matches(">opens<").count(), 1, "{svg}");
    assert_eq!(heads(&svg), 2);
    // The two lines leave the pill as one.
    let pill = pills(&svg)[0];
    let leaving: Vec<(f32, f32)> = arrow_lines(&svg)
        .iter()
        .map(|line| line[0])
        .filter(|start| (start.1 - (pill.y + pill.h)).abs() < 1.0)
        .collect();
    assert_eq!(leaving.len(), 2, "{svg}");
    assert!((leaving[0].0 - leaving[1].0).abs() < 0.5, "{leaving:?}");
}

#[test]
fn a_folder_is_a_frame_around_its_cards() {
    let boxes = vec![
        leaf("Orders"),
        folder("scenarios", leaves(&["Buy", "Return"])),
    ];
    // No line runs into it: laid out on its own.
    let (svg, map) = drawn(&boxes, &[]);
    tidy(&svg, &map);
    let frame = frames(&svg);
    assert_eq!(frame.len(), 1, "{svg}");
    assert!(svg.contains(">scenarios<"));
    for (key, inside) in [("buy", true), ("return", true), ("orders", false)] {
        assert_eq!(
            within(tile(&map, key).frame, frame[0]),
            inside,
            "{key}: {frame:?}"
        );
        assert_eq!(overlap(tile(&map, key).frame, frame[0]), inside, "{key}");
    }
    // A line out of it: the frame is around the same cards, the one no
    // line reaches beside the other, and the arrow leaves it.
    let (svg, map) = drawn(&boxes, &[plays("buy", "orders")]);
    tidy(&svg, &map);
    let frame = frames(&svg);
    assert_eq!(frame.len(), 1, "{svg}");
    for (key, inside) in [("buy", true), ("return", true), ("orders", false)] {
        assert_eq!(
            within(tile(&map, key).frame, frame[0]),
            inside,
            "{key}: {frame:?}"
        );
        assert_eq!(overlap(tile(&map, key).frame, frame[0]), inside, "{key}");
    }
    assert_eq!(heads(&svg), 1);
    assert!(tile(&map, "buy").frame.y < tile(&map, "orders").frame.y);
    // A folder in a folder is a frame in a frame.
    let nested = vec![folder(
        "billing",
        vec![leaf("Payment"), folder("parts", leaves(&["Checkout"]))],
    )];
    for relations in [vec![], vec![opens("payment", "checkout")]] {
        let (svg, map) = drawn(&nested, &relations);
        tidy(&svg, &map);
        let frame = frames(&svg);
        assert_eq!(frame.len(), 2, "{svg}");
        assert!(within(frame[1], frame[0]), "{frame:?}");
        assert!(within(tile(&map, "checkout").frame, frame[1]));
        assert!(within(tile(&map, "payment").frame, frame[0]));
        assert!(!overlap(tile(&map, "payment").frame, frame[1]));
    }
}

#[test]
fn a_rest_higher_than_its_frame_makes_the_frame_higher() {
    // Buy plays Orders from inside the folder; the eight no line reaches
    // stand in rows beside it, higher than Buy alone.
    let mut inside = vec![leaf("Buy")];
    let names: Vec<String> = (0..8).map(|i| format!("Note {i}")).collect();
    inside.extend(names.iter().map(|name| leaf(name)));
    let boxes = vec![leaf("Orders"), folder("scenarios", inside)];
    let (svg, map) = drawn(&boxes, &[plays("buy", "orders")]);
    tidy(&svg, &map);
    let frame = frames(&svg)[0];
    for t in map.tiles.iter().filter(|t| t.key != "orders") {
        assert!(within(t.frame, frame), "{t:?} in {frame:?}");
    }
    assert!(!overlap(tile(&map, "orders").frame, frame));
}

#[test]
fn many_cards_that_are_alike_stand_in_a_block_with_one_line() {
    let names: Vec<String> = (0..7).map(|i| format!("Buy {i}")).collect();
    let mut boxes = vec![leaf("Orders")];
    boxes.extend(names.iter().map(|name| leaf(name)));
    let relations: Vec<Relation> = names.iter().map(|n| plays(&key(n), "orders")).collect();
    let (svg, map) = drawn(&boxes, &relations);
    tidy(&svg, &map);
    // One outline around the seven, in more than one row; one line from
    // it through the pill into Orders.
    let outline = rounded(&svg, "22");
    assert_eq!(outline.len(), 1, "{svg}");
    let mut rows: Vec<i32> = Vec::new();
    for name in &names {
        let t = tile(&map, &key(name));
        assert!(within(t.frame, outline[0]));
        if !rows.contains(&(t.frame.y as i32)) {
            rows.push(t.frame.y as i32);
        }
    }
    assert!(rows.len() > 1, "{rows:?}");
    assert_eq!(svg.matches(">plays<").count(), 1);
    assert_eq!((heads(&svg), arrow_lines(&svg).len()), (1, 2));
    assert!(outline[0].y + outline[0].h < tile(&map, "orders").frame.y);
    // Four that are alike are four cards with a line each.
    let (svg, _) = drawn(&boxes[..5], &relations[..4]);
    assert!(rounded(&svg, "22").is_empty());
    assert_eq!((heads(&svg), arrow_lines(&svg).len()), (1, 5));
}

#[test]
fn cards_no_arrow_reaches_fill_a_block_and_no_row() {
    let names: Vec<String> = (0..30).map(|i| format!("Diagram {i}")).collect();
    let boxes: Vec<Branch> = names.iter().map(|name| leaf(name)).collect();
    let (svg, map) = drawn(&boxes, &[]);
    tidy(&svg, &map);
    let page = size(&svg);
    assert!(
        page.w > page.h && page.w < 2.5 * page.h,
        "{} by {}",
        page.w,
        page.h
    );
    assert!(rounded(&svg, "22").is_empty(), "no outline around them");
    // A folder's tile comes first among them.
    let kinds = |key: &str| {
        Some(Card {
            kind: String::new(),
            icon: if key == "areas" {
                Icon::Map
            } else {
                Icon::State
            },
            facts: String::new(),
            size: (300.0, 200.0),
        })
    };
    let tiles = Tiles {
        card: &kinds,
        relations: &[],
        heading: None,
    };
    let boxes = leaves(&["Orders", "Payment", "Areas"]);
    let (_, map) = crate::render_branches(SKIN, &boxes, &tiles).unwrap();
    let x = |key: &str| tile(&map, key).frame.x;
    assert!(x("areas") < x("orders") && x("orders") < x("payment"));
}

#[test]
fn what_no_arrow_joins_stands_beside_one_another() {
    // Six pairs, each a scenario above its machine: side by side in rows,
    // not twelve cards in two long rows.
    let mut boxes = Vec::new();
    let mut relations = Vec::new();
    for i in 0..6 {
        let (scenario, machine) = (format!("Run {i}"), format!("Machine {i}"));
        relations.push(plays(&key(&scenario), &key(&machine)));
        boxes.extend([leaf(&machine), leaf(&scenario)]);
    }
    boxes.push(leaf("Notes"));
    let (svg, map) = drawn(&boxes, &relations);
    tidy(&svg, &map);
    for i in 0..6 {
        let (run, machine) = (
            tile(&map, &format!("run{i}")).frame,
            tile(&map, &format!("machine{i}")).frame,
        );
        assert!(run.y + run.h < machine.y, "{i}");
        assert!(run.x < machine.x + machine.w && machine.x < run.x + run.w);
    }
    assert_eq!((svg.matches(">plays<").count(), heads(&svg)), (6, 6));
    let page = size(&svg);
    assert!(page.w < 2.5 * page.h, "{} by {}", page.w, page.h);
    // What is joined comes first, the card no arrow reaches after it.
    let notes = tile(&map, "notes").frame;
    let first = tile(&map, "run0").frame;
    assert!(notes.y > first.y || notes.x > first.x);
}
