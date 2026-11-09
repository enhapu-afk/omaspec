use super::*;
use crate::graph::Rect;

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

fn known(text: &str) -> Option<String> {
    let k = key(text);
    ["orders", "checkout", "payment", "shop", "refund"]
        .contains(&k.as_str())
        .then_some(k)
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

fn tiles<'a>(
    loose: &'a [(String, String)],
    relations: &'a [Relation],
    heading: Option<&'a Heading>,
) -> Tiles<'a> {
    Tiles {
        pick: &known,
        card: &card,
        loose,
        relations,
        heading,
    }
}

#[test]
fn a_wbs_map_draws_its_diagrams_as_cards_from_the_top_down() {
    let source =
        "@startwbs\n* Shop\n** Orders\n*** Checkout\n*** Notes\n** Payment\n** Ideas\n@endwbs\n";
    let loose = [("Refund".to_owned(), "refund".to_owned())];
    let heading = Heading {
        title: "shop".into(),
        summary: "System map · 5 diagrams · 4 on the map".into(),
        main: true,
        place: Some("** Refund".into()),
    };
    let (svg, map) =
        crate::render_tiled(source, None, Some(&tiles(&loose, &[], Some(&heading)))).unwrap();
    let keys: Vec<(&str, Option<usize>, Option<&str>)> = map
        .tiles
        .iter()
        .map(|t| (t.key.as_str(), t.line, t.parent.as_deref()))
        .collect();
    assert_eq!(
        keys,
        [
            ("shop", Some(1), None),
            ("orders", Some(2), Some("shop")),
            ("checkout", Some(3), Some("orders")),
            ("payment", Some(5), Some("shop")),
            ("refund", None, Some("shop")),
        ]
    );
    let page = size(&svg);
    for tile in &map.tiles {
        assert!(within(tile.picture, tile.frame), "{tile:?}");
        assert!(within(tile.frame, page), "{tile:?} {page:?}");
    }
    for (i, a) in map.tiles.iter().enumerate() {
        for b in &map.tiles[i + 1..] {
            assert!(!overlap(a.frame, b.frame), "{a:?} on {b:?}");
        }
    }
    let at = |k: &str| map.tiles.iter().find(|t| t.key == k).unwrap();
    // Below one another, level by level; a card shaped like its diagram.
    assert!(at("orders").frame.y > at("shop").frame.y + at("shop").frame.h);
    assert!(at("checkout").frame.y > at("orders").frame.y + at("orders").frame.h);
    assert!(at("checkout").picture.h > at("checkout").picture.w);
    assert!(at("payment").picture.w > 2.0 * at("payment").picture.h);
    // The root's card is larger; the diagram on no box hangs from it
    // beside its children.
    assert!(
        at("shop").picture.w * at("shop").picture.h
            > at("orders").picture.w * at("orders").picture.h
    );
    assert_eq!(at("refund").frame.y, at("orders").frame.y);
    assert!(at("refund").frame.x > at("payment").frame.x + at("payment").frame.w);
    // Labels for boxes that name nothing, the heading, the way to place
    // the loose one, a dashed curve to it and its dashed card.
    for words in [
        ">Notes<",
        ">Ideas<",
        ">shop<",
        "4 on the map",
        "** Refund",
        "not on the map yet",
        ">MAIN<",
    ] {
        assert!(svg.contains(words), "{words}");
    }
    assert_eq!(svg.matches(r#"stroke-dasharray="5,5""#).count(), 1);
    assert_eq!(svg.matches(r#"stroke-dasharray="5,4""#).count(), 1);
    // No line of PlantUML's own WBS look.
    assert!(!svg.contains("<line"));
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
        pick: &known,
        card: &small,
        loose: &[],
        relations: &[],
        heading: None,
    };
    let (_, map) = crate::render_tiled(
        "@startwbs\n* Shop\n** Orders\n@endwbs\n",
        None,
        Some(&tiles),
    )
    .unwrap();
    let shop = map.tiles[0].picture;
    assert!(
        (shop.w - 125.0).abs() < 0.01 && (shop.h - 250.0).abs() < 0.01,
        "{shop:?}"
    );
}

#[test]
fn a_map_with_only_its_root_draws_no_stem() {
    let source = "@startwbs\n* Shop\n@endwbs\n";
    let stems = |svg: &str| svg.matches("<line").count();
    // PlantUML draws a stem under a lone root; a map has no use for it.
    assert_eq!(stems(&crate::render(source).unwrap()), 1);
    let (svg, map) = crate::render_tiled(source, None, Some(&tiles(&[], &[], None))).unwrap();
    assert_eq!(stems(&svg), 0, "{svg}");
    // No curve from a lone root, and no anchor for one.
    assert!(!svg.contains(" C") && !svg.contains("<circle"), "{svg}");
    assert_eq!(map.tiles.len(), 1);
}

#[test]
fn relations_are_arrows_with_their_words() {
    let source = "@startwbs\n* Shop\n** Orders\n** Payment\n** Checkout\n@endwbs\n";
    let relations = [
        Relation {
            from: "payment".into(),
            to: "checkout".into(),
            kind: RelationKind::Opens,
        },
        Relation {
            from: "payment".into(),
            to: "checkout".into(),
            kind: RelationKind::Plays,
        },
        Relation {
            from: "orders".into(),
            to: "shop".into(),
            kind: RelationKind::Includes,
        },
        // To a diagram not on the map: no arrow.
        Relation {
            from: "orders".into(),
            to: "nowhere".into(),
            kind: RelationKind::Plays,
        },
    ];
    let (svg, _) = crate::render_tiled(source, None, Some(&tiles(&[], &relations, None))).unwrap();
    // One arrow for a pair, naming both relations, solid as the first.
    assert!(svg.contains(">plays · opens<"), "{svg}");
    assert!(svg.contains(">includes<"));
    assert_eq!(svg.matches(r##"Z" fill="#"##).count(), 2, "two heads");
}

#[test]
fn a_mind_map_map_hangs_both_sides_below_its_root() {
    let source = "@startmindmap\n* Shop\n** Orders\n*** Checkout\n-- Payment\n@endmindmap\n";
    let (svg, map) = crate::render_tiled(source, None, Some(&tiles(&[], &[], None))).unwrap();
    let keys: Vec<(&str, Option<&str>)> = map
        .tiles
        .iter()
        .map(|t| (t.key.as_str(), t.parent.as_deref()))
        .collect();
    assert_eq!(
        keys,
        [
            ("shop", None),
            ("orders", Some("shop")),
            ("checkout", Some("orders")),
            ("payment", Some("shop")),
        ]
    );
    let page = size(&svg);
    let (shop, payment) = (map.tiles[0].frame, map.tiles[3].frame);
    assert!(payment.y > shop.y + shop.h);
    for tile in &map.tiles {
        assert!(
            within(tile.picture, tile.frame) && within(tile.frame, page),
            "{tile:?}"
        );
    }
    assert!(!svg.contains("stroke-dasharray"));
}

#[test]
fn the_cards_take_the_colors_of_the_skin() {
    let source = "@startwbs\nskinparam backgroundColor #1a1b26\nskinparam defaultFontColor #a9b1d6\nskinparam nodeBackgroundColor #24283b\nskinparam arrowColor #7aa2f7\n* Shop\n** Orders\n@endwbs\n";
    let (svg, _) = crate::render_tiled(source, None, Some(&tiles(&[], &[], None))).unwrap();
    assert!(svg.contains(r##"fill="#1A1B26""##), "the page");
    assert!(svg.contains(r##"fill="#7AA2F7""##), "the accent");
    assert!(svg.contains(r##"fill="#A9B1D6">Shop<"##), "the words");
    // Plain, a light page with a blue accent.
    let (svg, _) = crate::render_tiled(
        "@startwbs\n* Shop\n** Orders\n@endwbs\n",
        None,
        Some(&tiles(&[], &[], None)),
    )
    .unwrap();
    assert!(svg.contains(r##"fill="#FFFFFF""##));
    assert!(svg.contains(r##"fill="#3D6FD6""##));
}

/// The corners of each arrow of a map, read from its drawing.
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

/// The pills of words on a map, read from its drawing.
fn pills(svg: &str) -> Vec<Rect> {
    svg.split("<rect ")
        .skip(1)
        .filter_map(|rest| {
            let tag = rest.split('>').next()?;
            let attr = |name: &str| -> Option<f32> {
                let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
                tag[at..].split('"').next()?.parse().ok()
            };
            tag.contains(r#"rx="11""#).then_some(Rect {
                x: attr("x")?,
                y: attr("y")?,
                w: attr("width")?,
                h: attr("height")?,
            })
        })
        .collect()
}

fn plays(from: &str, to: &str) -> Relation {
    Relation {
        from: from.into(),
        to: to.into(),
        kind: RelationKind::Plays,
    }
}

/// How many arrowheads a map draws.
fn heads(svg: &str) -> usize {
    svg.matches(r##"Z" fill="#"##).count()
}

#[test]
fn arrows_into_one_card_share_a_lane_below_the_cards_between() {
    // Three scenarios in a row play the machine at its start: a card or
    // two lie between each of them and it.
    let source = "@startwbs\n* Shop\n** Orders\n** Payment\n** Checkout\n** Refund\n@endwbs\n";
    let relations = [
        plays("payment", "orders"),
        plays("checkout", "orders"),
        plays("refund", "orders"),
    ];
    let (svg, map) =
        crate::render_tiled(source, None, Some(&tiles(&[], &relations, None))).unwrap();
    let page = size(&svg);
    let lines = arrow_lines(&svg);
    // A line from each scenario, one head, the words once.
    assert_eq!(lines.len(), 3, "{svg}");
    assert_eq!(heads(&svg), 1);
    assert_eq!(svg.matches(">plays<").count(), 1);
    let lowest = map
        .tiles
        .iter()
        .map(|t| t.frame.y + t.frame.h)
        .fold(0.0, f32::max);
    for line in &lines {
        for run in line.windows(2) {
            for tile in &map.tiles {
                assert!(
                    !crate::graph::route::cuts(tile.frame, run[0], run[1]),
                    "{run:?} under {}",
                    tile.key
                );
            }
        }
        for &(x, y) in line {
            assert!(x > 0.0 && y > 0.0 && x < page.w && y < page.h, "{x} {y}");
        }
        // Down to one lane below every card of the row.
        let lane = line.iter().map(|p| p.1).fold(0.0, f32::max);
        let first = lines[0].iter().map(|p| p.1).fold(0.0, f32::max);
        assert!(lane > lowest + 10.0 && lane == first, "{lane} {lowest}");
    }
    // Each starts below its own card; the first ends below the machine.
    let below = |key: &str, (x, y): (f32, f32)| {
        let frame = map.tiles.iter().find(|t| t.key == key).unwrap().frame;
        (y - frame.y - frame.h).abs() < 0.01 && x > frame.x && x < frame.x + frame.w
    };
    let mut starts: Vec<&str> = ["payment", "checkout", "refund"]
        .into_iter()
        .filter(|key| lines.iter().any(|line| below(key, line[0])))
        .collect();
    starts.dedup();
    assert_eq!(starts.len(), 3, "{lines:?}");
    assert!(below("orders", *lines[0].last().unwrap()), "{:?}", lines[0]);
    // The words lie on the page and on no card.
    let pills = pills(&svg);
    let [pill] = pills.as_slice() else {
        panic!("{svg}");
    };
    assert!(within(*pill, page), "{pill:?}");
    for tile in &map.tiles {
        assert!(!overlap(*pill, tile.frame), "{pill:?} on {}", tile.key);
    }
}

#[test]
fn the_lanes_of_a_row_have_room_above_the_next_one() {
    let relations = [plays("checkout", "orders"), plays("payment", "checkout")];
    let draw = |relations: &[Relation]| {
        let source = "@startwbs\n* Shop\n** Orders\n*** Refund\n** Notes\n** Payment\n** Checkout\n@endwbs\n";
        crate::render_tiled(source, None, Some(&tiles(&[], relations, None))).unwrap()
    };
    let (svg, map) = draw(&relations);
    let (_, plain) = draw(&[]);
    let at =
        |map: &crate::map::Map, key: &str| map.tiles.iter().find(|t| t.key == key).unwrap().frame;
    // Checkout plays Orders past a label and a card: on a lane. Payment
    // plays Checkout next to it: straight across.
    let lines = arrow_lines(&svg);
    assert_eq!(lines.len(), 2, "{svg}");
    assert_eq!(heads(&svg), 2);
    let lane = lines.iter().flatten().map(|p| p.1).fold(0.0, f32::max);
    let row = ["orders", "payment", "checkout"]
        .iter()
        .map(|key| at(&map, key).y + at(&map, key).h)
        .fold(0.0, f32::max);
    assert!(lane > row + 10.0, "{lane} {row}");
    // The card below moved down by the lane and keeps its distance.
    let refund = at(&map, "refund");
    assert!(refund.y > at(&plain, "refund").y + 10.0);
    assert!(refund.y - lane > 40.0, "{} {lane}", refund.y);
    for line in &lines {
        for run in line.windows(2) {
            for tile in &map.tiles {
                assert!(!crate::graph::route::cuts(tile.frame, run[0], run[1]));
            }
        }
    }
}

#[test]
fn an_arrow_between_rows_runs_around_the_cards() {
    // From a card of the third row up to one of the second that is not
    // above it, and to the root.
    let source = "@startwbs\n* Shop\n** Orders\n*** Refund\n** Payment\n** Checkout\n@endwbs\n";
    let relations = [plays("refund", "checkout"), plays("refund", "shop")];
    let (svg, map) =
        crate::render_tiled(source, None, Some(&tiles(&[], &relations, None))).unwrap();
    let page = size(&svg);
    let lines = arrow_lines(&svg);
    assert_eq!(lines.len(), 2, "{svg}");
    assert_eq!(heads(&svg), 2);
    for line in &lines {
        for run in line.windows(2) {
            for tile in &map.tiles {
                assert!(
                    !crate::graph::route::cuts(tile.frame, run[0], run[1]),
                    "{run:?} under {}",
                    tile.key
                );
            }
        }
    }
    let pills = pills(&svg);
    assert_eq!(pills.len(), 2);
    assert!(!overlap(pills[0], pills[1]));
    for pill in &pills {
        assert!(within(*pill, page), "{pill:?}");
    }
}

#[test]
fn the_words_of_a_short_arrow_leave_its_ends_to_see() {
    // Two cards next to each other: the arrow between them is shorter
    // than its words are wide.
    let source = "@startwbs\n* Shop\n** Orders\n** Payment\n@endwbs\n";
    let relations = [plays("payment", "orders")];
    let (svg, map) =
        crate::render_tiled(source, None, Some(&tiles(&[], &relations, None))).unwrap();
    let lines = arrow_lines(&svg);
    let [line] = lines.as_slice() else {
        panic!("{svg}");
    };
    let (orders, payment) = (map.tiles[1].frame, map.tiles[2].frame);
    // Straight across the room between the two.
    assert_eq!(line.len(), 2, "{line:?}");
    assert!((line[0].0 - payment.x).abs() < 0.01, "{line:?}");
    assert!((line[1].0 - orders.x - orders.w).abs() < 0.01, "{line:?}");
    assert_eq!(line[0].1, line[1].1);
    let pills = pills(&svg);
    let [pill] = pills.as_slice() else {
        panic!("{svg}");
    };
    assert!(
        !crate::graph::route::cuts(*pill, line[0], line[1]),
        "{pill:?} on {line:?}"
    );
}

#[test]
fn the_map_grows_by_the_arrows_around_its_cards() {
    let source = "@startwbs\n* Shop\n** Orders\n** Payment\n** Checkout\n@endwbs\n";
    let plain = crate::render_tiled(source, None, Some(&tiles(&[], &[], None))).unwrap();
    let relations = [plays("checkout", "orders")];
    let (svg, map) =
        crate::render_tiled(source, None, Some(&tiles(&[], &relations, None))).unwrap();
    // The cards stay where they are without the arrow, or move together.
    let moved = map.tiles[0].frame.x - plain.1.tiles[0].frame.x;
    for (with, without) in map.tiles.iter().zip(&plain.1.tiles) {
        assert!((with.frame.x - without.frame.x - moved).abs() < 0.01);
        assert_eq!(with.frame.y, without.frame.y);
        assert!(within(with.picture, with.frame));
    }
    let page = size(&svg);
    for pill in pills(&svg) {
        assert!(within(pill, page), "{pill:?} {page:?}");
    }
}
