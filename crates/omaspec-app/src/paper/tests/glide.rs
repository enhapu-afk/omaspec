//! How a new layout comes: a fold or an edit that moves boxes glides them
//! from their parts (docs/adr/0029), a picture without parts or of too
//! many boxes fades, and what comes meanwhile waits its turn.

use super::*;

#[test]
fn a_fold_fades_and_a_zoomed_box_stays_put() {
    let open = "@startuml\nstate Idle\nstate Brewing {\n  state Grind\n  state Pour\n}\nIdle --> Brewing\n@enduml\n";
    let folded = "@startuml\nstate Idle\nstate Brewing {\n  state Grind\n  state Pour\n}\nIdle --> Brewing\n' fold (Omaspec) Brewing\n@enduml\n";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, open, Language::PlantUml);
    paper.send(Message::Faded);
    paper.send(Message::ZoomAt(2.0, Point::new(400.0, 300.0)));
    let before = paper.screen_of_box("Brewing");
    let open_box = paper.box_center("Brewing").expect("Brewing");
    paper.set_fold("Brewing", true);
    let task = draw_source(&mut paper, folded, Language::PlantUml);
    let folded_box = paper.box_center("Brewing").expect("Brewing folded");
    assert!(
        folded_box.distance(open_box) > 1.0
            || paper.diagram_size().height + 1.0
                < paper
                    .arrival
                    .previous()
                    .map(|p| p.size.height)
                    .unwrap_or(1.0),
        "folding Brewing changed nothing"
    );
    let after = paper.screen_of_box("Brewing");
    assert!(
        (after.x - before.x).abs() < 1.5 && (after.y - before.y).abs() < 1.5,
        "the box left the screen: {before:?} -> {after:?}"
    );
    assert!(!paper.view.fit);
    paper.run(task);
    let fade = paper.arrival.fade().expect("the fold fades");
    assert!(!fade.old.is_empty());
    assert!(fade.morph.is_some(), "folding moves the boxes");
    let left = fade.shown.remaining(Instant::now());
    assert!(
        left > Duration::from_millis(280),
        "a fold fades longer, {left:?}"
    );
    // The line into Brewing glides along, over the grounds of the sheets.
    assert!(fade.bare.is_some(), "both pictures without their lines");
    let lines = fade.lines.as_ref().expect("the lines glide");
    assert_eq!(lines.lines.len(), 1);
    assert!(lines.lines[0].from.is_some() && lines.lines[0].to.is_some());
}

/// Asked for its parts, a state machine's fold glides each box, mark and
/// line's words from a picture of its own: the pictures under the glide
/// are sheets of their ground and their parts, and a box drawn from its
/// part leaves no hole in the ground (docs/adr/0029).
#[test]
fn a_fold_glides_each_part_from_a_picture_of_its_own() {
    let open = "@startuml\n[*] --> Idle\nIdle --> Brewing : start\nstate Brewing {\n  [*] --> Grind\n  Grind --> Pour : ground\n}\nBrewing --> Idle : done\n' parts (Omaspec)\n@enduml\n";
    let folded = open.replace("@enduml", "' fold (Omaspec) Brewing\n@enduml");
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, open, Language::PlantUml);
    paper.send(Message::Faded);
    assert!(paper.diagram.as_ref().unwrap().drawn.parts);
    paper.set_fold("Brewing", true);
    show_source(&mut paper, &folded, Language::PlantUml);
    let fade = paper.arrival.fade().expect("the fold glides");
    let morph = fade.morph.as_ref().expect("the boxes move");
    let (old, new) = fade.bare.as_ref().expect("both sheets");
    for step in &morph.moves {
        let parts = [&step.parts[0], &step.parts[1]];
        if step.role != morph::Role::Arrive {
            assert!(
                old.part(parts[0]).is_some(),
                "{} not on the old sheet",
                parts[0]
            );
        }
        if step.role != morph::Role::Leave {
            assert!(
                new.part(parts[1]).is_some(),
                "{} not on the new sheet",
                parts[1]
            );
        }
    }
    // The inner start goes, the outer one stays, each its own part.
    let start = |name: &str| morph.moves.iter().find(|m| m.name == name).map(|m| m.role);
    assert_eq!(start("start::0"), Some(morph::Role::Stay));
    assert_eq!(start("start:Brewing:0"), Some(morph::Role::Leave));
    // The words of the lines travel with them, from the sheets too.
    let lines = fade.lines.as_ref().expect("the lines glide");
    // The line inside Brewing goes, its words ("ground") with it; the
    // start's line has none.
    let going = lines.lines.iter().filter(|line| line.to.is_none());
    assert!(
        going
            .filter_map(|line| line.words[0][0].as_deref())
            .any(|words| old.part(words).is_some()),
        "the words of a line that goes are on the old sheet"
    );
    // "start" and "done" stay, on both sheets.
    let staying = lines
        .lines
        .iter()
        .filter(|line| line.from.is_some() && line.to.is_some());
    let kept = staying
        .filter(|line| {
            let [was, is] = &line.words;
            was[0].as_deref().is_some_and(|w| old.part(w).is_some())
                && is[0].as_deref().is_some_and(|w| new.part(w).is_some())
        })
        .count();
    assert_eq!(kept, 2, "the words of the lines that stay");
}

/// A package of a class diagram folds from parts too: each class and the
/// package itself come from the sheets.
#[test]
fn a_package_glides_from_its_parts() {
    let open = "@startuml\npackage Lending {\n  class Loan\n  class Copy\n  Loan --> Copy : lends\n}\nclass Member\nMember --> Loan : borrows\n' parts (Omaspec)\n@enduml\n";
    let folded = open.replace("@enduml", "' fold (Omaspec) Lending\n@enduml");
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, open, Language::PlantUml);
    paper.send(Message::Faded);
    paper.set_fold("Lending", true);
    show_source(&mut paper, &folded, Language::PlantUml);
    let fade = paper.arrival.fade().expect("the fold glides");
    let morph = fade.morph.as_ref().expect("the boxes move");
    let (old, new) = fade.bare.as_ref().expect("both sheets");
    for step in &morph.moves {
        if step.role != morph::Role::Arrive {
            assert!(
                old.part(&step.parts[0]).is_some(),
                "{} on the old sheet",
                step.name
            );
        }
        if step.role != morph::Role::Leave {
            assert!(
                new.part(&step.parts[1]).is_some(),
                "{} on the new sheet",
                step.name
            );
        }
    }
    let role = |name: &str| morph.moves.iter().find(|m| m.name == name).map(|m| m.role);
    assert_eq!(role("Loan"), Some(morph::Role::Leave));
    assert_eq!(role("Lending"), Some(morph::Role::Stay));
    assert!(morph.orphans[0].is_empty(), "{:?}", morph.orphans);
}

/// A subgraph of a Mermaid flowchart folds from parts too, its line's
/// words with it.
#[test]
fn a_subgraph_glides_from_its_parts() {
    let open = "flowchart LR\n  A[Cart] -->|pays| B[Till]\n  subgraph Back[Back office]\n    C[Stock] --> D[Ship]\n  end\n  B -->|orders| C\n%% parts (Omaspec)\n";
    let folded = format!("{open}%% fold (Omaspec) Back\n");
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, open, Language::Mermaid);
    paper.send(Message::Faded);
    assert!(paper.diagram.as_ref().unwrap().drawn.parts);
    paper.set_fold("Back", true);
    show_source(&mut paper, &folded, Language::Mermaid);
    let fade = paper.arrival.fade().expect("the fold glides");
    let morph = fade.morph.as_ref().expect("the boxes move");
    let (old, new) = fade.bare.as_ref().expect("both sheets");
    for step in &morph.moves {
        if step.role != morph::Role::Arrive {
            assert!(
                old.part(&step.parts[0]).is_some(),
                "{} on the old sheet",
                step.name
            );
        }
        if step.role != morph::Role::Leave {
            assert!(
                new.part(&step.parts[1]).is_some(),
                "{} on the new sheet",
                step.name
            );
        }
    }
    let role = |name: &str| morph.moves.iter().find(|m| m.name == name).map(|m| m.role);
    assert_eq!(role("C"), Some(morph::Role::Leave));
    assert_eq!(role("Back"), Some(morph::Role::Stay));
    assert!(morph.orphans[0].is_empty(), "{:?}", morph.orphans);
    let lines = fade.lines.as_ref().expect("the lines glide");
    assert!(
        lines.lines.iter().any(|line| {
            line.words[0][0]
                .as_deref()
                .is_some_and(|w| old.part(w).is_some())
                && line.words[1][0]
                    .as_deref()
                    .is_some_and(|w| new.part(w).is_some())
        }),
        "the words of a line that stays on both sheets"
    );
}

/// A branch of a WBS folds from parts: the boxes below it go into it,
/// each from its own picture, and nothing is left over.
#[test]
fn a_wbs_branch_glides_from_its_parts() {
    let open =
        "@startwbs\n* Move\n** Pack\n*** Books\n*** Plates\n** Clean\n' parts (Omaspec)\n@endwbs\n";
    let folded = format!("{open}' fold (Omaspec) Pack\n");
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, open, Language::PlantUml);
    paper.send(Message::Faded);
    assert!(paper.diagram.as_ref().unwrap().drawn.parts);
    paper.set_fold("Pack", true);
    show_source(&mut paper, &folded, Language::PlantUml);
    let fade = paper.arrival.fade().expect("the fold glides");
    let morph = fade.morph.as_ref().expect("the boxes move");
    let (old, new) = fade.bare.as_ref().expect("both sheets");
    for step in &morph.moves {
        if step.role != morph::Role::Arrive {
            assert!(
                old.part(&step.parts[0]).is_some(),
                "{} on the old sheet",
                step.name
            );
        }
        if step.role != morph::Role::Leave {
            assert!(
                new.part(&step.parts[1]).is_some(),
                "{} on the new sheet",
                step.name
            );
        }
    }
    let role = |name: &str| morph.moves.iter().find(|m| m.name == name).map(|m| m.role);
    assert_eq!(role("Books"), Some(morph::Role::Leave));
    assert_eq!(role("Pack"), Some(morph::Role::Stay));
    assert!(
        morph.orphans.iter().all(Vec::is_empty),
        "{:?}",
        morph.orphans
    );
    assert!(fade.lines.is_some(), "the links glide");
}

/// A JSON card that folds glides from its part into the row of the card
/// above, whose part has the folded row; its arrow is a line that goes.
/// The top card and the title, in both pictures, glide on their own.
#[test]
fn a_json_card_folds_from_its_part_into_its_row() {
    let open = "@startjson\ntitle People\n{\"name\": \"Ada\", \"home\": {\"city\": \"London\", \"street\": {\"no\": 7}}}\n' parts (Omaspec)\n@endjson\n";
    let folded = format!("{open}' fold (Omaspec) home/street\n");
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, open, Language::PlantUml);
    paper.send(Message::Faded);
    assert!(paper.diagram.as_ref().unwrap().drawn.parts);
    paper.set_fold("home/street", true);
    show_source(&mut paper, &folded, Language::PlantUml);
    let fade = paper.arrival.fade().expect("the fold glides");
    let morph = fade.morph.as_ref().expect("the boxes move");
    let (old, new) = fade.bare.as_ref().expect("both sheets");
    let street = morph
        .moves
        .iter()
        .find(|m| m.name == "home/street")
        .expect("the street card moves");
    // From its card to the folded row in home's card: a part on the old
    // sheet, in home's part on the new one.
    assert_eq!(street.role, morph::Role::Stay);
    assert!(old.part(&street.parts[0]).is_some());
    assert!(new.part(&street.parts[1]).is_none());
    assert!(new.part("home").is_some() && old.part("home").is_some());
    assert!(
        morph.orphans.iter().all(Vec::is_empty),
        "{:?}",
        morph.orphans
    );
    // The title is words on clear pixels, the top card covers its place.
    let kept: Vec<(&str, bool)> = morph
        .kept
        .iter()
        .map(|kept| (kept.name.as_str(), kept.solid))
        .collect();
    assert_eq!(kept, [("frame:title", false), ("json:top", true)]);
    // The home card, which grows by its folded row, is drawn at its size
    // and covers the old one.
    let home = morph.moves.iter().find(|m| m.name == "home").unwrap();
    assert!(home.anchored && home.looks.solid, "{home:?}");
    let lines = fade.lines.as_ref().expect("the arrows glide");
    assert!(
        lines
            .lines
            .iter()
            .any(|line| line.from.is_some() && line.to.is_none()),
        "the arrow to the street card goes: {:?}",
        lines.lines
    );
}

/// The glide waits for the sheets of parts of both pictures; when one
/// could not be drawn, the pictures fade instead (docs/adr/0029).
#[test]
fn a_glide_waits_for_the_sheets_of_parts_and_fades_without_one() {
    let first = "@startuml
[*] --> A
@enduml
";
    let next = "@startuml
[*] --> A
A --> B
B --> C
C --> D
D --> E
@enduml
";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(400.0, 300.0)));
    show_source(&mut paper, first, Language::PlantUml);
    paper.send(Message::Faded);
    // Its pictures on their way: the overview comes alone, and the fade
    // waits for the sheets.
    let _on_their_way = draw_source(&mut paper, next, Language::PlantUml);
    let tree = Arc::clone(paper.shown_tree().unwrap());
    let raster = raster::render(&tree, paper.overview.asked.unwrap());
    let _sheets = paper.update(Message::Overview(paper.overview.seq(), raster));
    assert!(paper.arrival.fade().is_none(), "waits for the sheets");
    assert!(paper.bare_old.asked.is_some() && paper.bare_new.asked.is_some());
    assert!(
        paper.held_picture().is_some(),
        "the old picture stays meanwhile"
    );
    // One of them fails: the pictures fade, nothing is cut out of them.
    paper.send(Message::BareOld(paper.bare_old.seq(), None));
    let fade = paper.arrival.fade().expect("the fade starts without them");
    assert!(fade.morph.is_none());
    assert!(fade.lines.is_none() && fade.bare.is_none());
}

/// A picture that marks no parts, as the Java engine's does not, fades
/// into the next, and the next into it: nothing of it is read for a
/// glide, no sheet is asked for, and a fold's buttons wait for the new
/// picture as they do while boxes glide (docs/adr/0029).
#[test]
fn a_picture_without_parts_fades() {
    let first = "@startuml\nstate Big {\n  [*] --> A\n}\n[*] --> Big\n@enduml\n";
    let next = "@startuml\nstate Big {\n  [*] --> A\n  A --> B\n  B --> C\n}\n[*] --> Big\nBig --> D\n@enduml\n";
    let show = |paper: &mut Paper, source: &str, parts: bool| {
        let drawn = if parts {
            drawn(source, Language::PlantUml)
        } else {
            drawn_without_parts(source, Language::PlantUml)
        };
        assert_eq!(drawn.parts, parts);
        let mut reply = answer(1, paper.pretend_next(), Ok(drawn));
        reply.source = source.into();
        reply.language = Language::PlantUml;
        paper.rendered(reply)
    };
    for (from, to) in [(false, false), (true, false), (false, true)] {
        let what = format!("parts {from} to {to}");
        let mut paper = Paper {
            folding: true,
            ..Paper::default()
        };
        paper.send(Message::Viewport(Size::new(400.0, 300.0)));
        let task = show(&mut paper, first, from);
        paper.run(task);
        paper.send(Message::Faded);
        paper.set_fold("Big", true);
        let task = show(&mut paper, next, to);
        assert_eq!(
            paper.arrival.previous().unwrap().boxes.is_empty(),
            !from,
            "{what}: its boxes read only for a glide"
        );
        paper.run(task);
        assert!(paper.bare_old.asked.is_none() && paper.bare_new.asked.is_none());
        let fade = paper.arrival.fade().expect("the fade starts at once");
        assert!(fade.morph.is_none() && fade.bare.is_none(), "{what}");
        assert!(fade.stands_in, "{what}: a fold that does not glide");
        assert!(paper.fold_spots().is_empty(), "{what}: the buttons wait");
    }
}

/// A tree of `branches` branches with `leaves` leaves each, and `more`
/// boxes beside them: `1 + branches * (1 + leaves) + more` boxes.
fn wide_tree(branches: usize, leaves: usize, more: usize) -> String {
    let mut wbs = String::from("@startwbs\n* Life\n");
    for branch in 0..branches {
        wbs.push_str(&format!("** Branch {branch}\n"));
        for leaf in 0..leaves {
            wbs.push_str(&format!("*** Leaf {branch}.{leaf}\n"));
        }
    }
    for extra in 0..more {
        wbs.push_str(&format!("** Extra {extra}\n"));
    }
    wbs.push_str("@endwbs\n");
    wbs
}

/// The pane after `first` was shown and `next` came: its fade.
fn from_to(first: &str, next: &str) -> Paper {
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(400.0, 300.0)));
    show_source(&mut paper, first, Language::PlantUml);
    paper.send(Message::Faded);
    show_source(&mut paper, next, Language::PlantUml);
    paper
}

/// A diagram of more boxes than glide in time fades to its new layout:
/// one picture into the other, with no sheet of parts to wait for. The most that glide still do, and one more
/// on either side does not.
#[test]
fn a_diagram_of_too_many_boxes_fades_to_its_new_layout() {
    // 13 branches of 22 leaves and the root are 300 boxes.
    assert_eq!(1 + 13 * 23, arrive::MOST_GLIDING);
    let most = wide_tree(13, 22, 0);
    let one_more = wide_tree(13, 22, 1);
    let small = wide_tree(3, 4, 0);
    let less = wide_tree(12, 22, 0);
    let boxes = |paper: &Paper| paper.diagram.as_ref().unwrap().drawn.map.places.len();
    for (first, next, glides, what) in [
        (&most, &less, true, "the most that glide"),
        (&one_more, &less, false, "one more before"),
        (&less, &one_more, false, "one more after"),
        (&one_more, &small, false, "many to few"),
        (&small, &most, true, "few to the most"),
    ] {
        let paper = from_to(first, next);
        let fade = paper.arrival.fade().expect("the new layout comes");
        assert_eq!(fade.morph.is_some(), glides, "{what}");
        assert_eq!(fade.bare.is_some(), glides, "{what}");
        assert!(!fade.stands_in, "{what}: an edit, not a fold");
    }
    assert_eq!(boxes(&from_to(&small, &most)), arrive::MOST_GLIDING);
    assert_eq!(boxes(&from_to(&small, &one_more)), arrive::MOST_GLIDING + 1);
    // Nothing of a picture that will not glide is read for a glide.
    let mut paper = from_to(&small, &one_more);
    paper.send(Message::Faded);
    let _on_their_way = draw_source(&mut paper, &most, Language::PlantUml);
    assert!(paper.arrival.previous().unwrap().boxes.is_empty());
}

/// A fold in such a diagram fades as long as a fold does, and its
/// buttons wait as they do while boxes glide: the old picture is still
/// to be seen where no button is.
#[test]
fn a_fold_of_too_many_boxes_fades_and_its_buttons_wait() {
    let open = wide_tree(13, 22, 1);
    let folded = open.replace("@endwbs", "' fold (Omaspec) Branch 0\n@endwbs");
    let mut paper = Paper {
        folding: true,
        ..Paper::default()
    };
    paper.send(Message::Viewport(Size::new(400.0, 300.0)));
    show_source(&mut paper, &open, Language::PlantUml);
    paper.send(Message::Faded);
    assert!(!paper.fold_spots().is_empty(), "the branches fold");
    paper.set_fold("Branch 0", true);
    show_source(&mut paper, &folded, Language::PlantUml);
    let fade = paper.arrival.fade().expect("the fold fades");
    assert!(fade.morph.is_none() && fade.stands_in);
    let left = fade.shown.remaining(Instant::now());
    assert!(left > Duration::from_millis(280), "{left:?}");
    assert!(paper.fold_spots().is_empty(), "the buttons wait");
    paper.send(Message::Faded);
    assert!(!paper.fold_spots().is_empty(), "and come with the picture");
}

#[test]
fn a_glide_finishes_before_the_next_one_starts() {
    let first = "@startuml\n[*] --> A\n@enduml\n";
    let mid = "@startuml\n[*] --> A\nA --> B\nB --> C\nC --> D\nD --> E\n@enduml\n";
    let last =
        "@startuml\n[*] --> A\nA --> B\nB --> C\nC --> D\nD --> E\nE --> F\nF --> G\n@enduml\n";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(400.0, 300.0)));
    show_source(&mut paper, first, Language::PlantUml);
    paper.send(Message::Faded);

    show_source(&mut paper, mid, Language::PlantUml);
    let mid_size = paper.diagram_size();
    let fade = paper.arrival.fade().expect("the first glide");
    assert!(fade.morph.is_some());
    let first_size = fade.old_size;
    let pinned = fade.new_size;
    assert_eq!(pinned, mid_size);

    // A newer diagram arrives while that glide is still going.
    show_source(&mut paper, last, Language::PlantUml);
    let fade = paper.arrival.fade().expect("the glide in flight");
    assert_eq!(fade.old_size, first_size);
    assert_eq!(fade.new_size, pinned);

    let sheets = paper.update(Message::Faded);
    // The next glide asks for the sheets of parts first.
    assert!(paper.arrival.fade().is_none());
    paper.run(sheets);
    let fade = paper
        .arrival
        .fade()
        .expect("the next glide starts where the last one landed");
    assert_eq!(fade.old_size, mid_size);
    assert_eq!(fade.new_size, paper.diagram_size());
    assert!(fade.lines.is_some(), "with its lines gliding");
}

/// A new picture that came while the view glided is held; a pan that
/// stops the glide shows it, and what it starts comes with it: the sheets
/// of its glide, which a dropped task left waiting for ever.
#[test]
fn a_pan_that_stops_a_zoom_brings_the_held_picture_and_its_glide() {
    let first = "@startuml\n[*] --> A\n@enduml\n";
    let next = "@startuml\n[*] --> A\nA --> B\nB --> C\nC --> D\n@enduml\n";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(400.0, 300.0)));
    show_source(&mut paper, first, Language::PlantUml);
    paper.send(Message::Faded);
    // A zoom glides, long enough for the new picture to come meanwhile.
    let _ = paper.update(Message::ZoomStep(1.5, Point::new(200.0, 150.0)));
    paper.glide = paper
        .glide
        .map(|g| g.taking(std::time::Duration::from_secs(60)));
    show_source(&mut paper, next, Language::PlantUml);
    assert!(paper.held_overview.is_some(), "held while the view glides");
    assert!(paper.arrival.fade().is_none());
    paper.send(Message::Pan(Vector::new(10.0, 0.0)));
    assert!(paper.held_overview.is_none());
    assert!(
        paper.fading_lines(),
        "the boxes glide from the picture before"
    );
}

#[test]
fn a_render_asked_before_a_fold_leaves_the_fold_to_the_next() {
    let open = "@startuml\nstate Idle\nstate Brewing {\n  state Grind\n  state Pour\n}\nIdle --> Brewing\n@enduml\n";
    let folded = "@startuml\nstate Idle\nstate Brewing {\n  state Grind\n  state Pour\n}\nIdle --> Brewing\n' fold (Omaspec) Brewing\n@enduml\n";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, open, Language::PlantUml);
    paper.send(Message::Faded);
    paper.send(Message::ZoomAt(2.0, Point::new(400.0, 300.0)));
    // An edit is on its way when the fold is asked for.
    let edit = paper.pretend_next();
    paper.set_fold("Brewing", true);
    let before = paper.screen_of_box("Brewing");
    let mut reply = answer(1, edit, Ok(drawn(open, Language::PlantUml)));
    reply.source = open.into();
    reply.language = Language::PlantUml;
    let task = paper.rendered(reply);
    assert!(
        matches!(&paper.change, Change::Fold { anchor: Some(name), .. } if name == "Brewing"),
        "the edit took the fold's anchor"
    );
    paper.run(task);
    assert!(
        matches!(paper.change, Change::Fold { .. }),
        "the edit's fade took the fold"
    );
    paper.send(Message::Faded);
    let task = draw_source(&mut paper, folded, Language::PlantUml);
    let after = paper.screen_of_box("Brewing");
    assert!(
        (after.x - before.x).abs() < 1.5 && (after.y - before.y).abs() < 1.5,
        "the box left the screen: {before:?} -> {after:?}"
    );
    paper.run(task);
    assert!(matches!(paper.change, Change::Edit));
    let fade = paper.arrival.fade().expect("the fold fades");
    let left = fade.shown.remaining(Instant::now());
    assert!(
        left > Duration::from_millis(280),
        "a fold fades longer, {left:?}"
    );
}

/// A start or an end is `[*]` in every key: the end of a state inside a
/// folding container is no end of the diagram. Folding the cat's Awake,
/// its Eating's line to Hungry's end goes with Awake, and only "nine lives
/// used" travels to the diagram's own end; the inner line once travelled
/// there too, its head beside the end circle to the last frames.
#[test]
fn an_inner_end_s_line_does_not_glide_to_the_diagram_s_end() {
    let open = "stateDiagram-v2\n[*] --> Awake\nstate Awake {\n  [*] --> Hungry\n  state Hungry {\n    [*] --> Eating\n    Eating --> [*]\n  }\n}\nAwake --> [*]: nine lives used\n";
    let folded = format!("{open}%% fold (Omaspec) Awake\n");
    let mut paper = Paper {
        folding: true,
        ..Paper::default()
    };
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, open, Language::Mermaid);
    paper.send(Message::Faded);
    paper.set_fold("Awake", true);
    show_source(&mut paper, &folded, Language::Mermaid);
    assert!(paper.fading_lines(), "the fold glides");
    let map = &paper.diagram.as_ref().unwrap().drawn.map;
    let end = map
        .wires
        .iter()
        .find(|w| w.key.from == "Awake" && w.key.to == "[*]")
        .and_then(|w| w.points.last().copied())
        .expect("nine lives used");
    let fade = paper.arrival.fade().unwrap();
    let onto_end = fade
        .lines
        .as_ref()
        .unwrap()
        .lines
        .iter()
        .filter(|line| {
            line.to.as_ref().is_some_and(|to| {
                to.points
                    .last()
                    .is_some_and(|&(x, y)| (x - end.0).abs() < 0.5 && (y - end.1).abs() < 0.5)
            })
        })
        .count();
    assert_eq!(onto_end, 1, "one line ends on the diagram's end");
}

/// REQ-063: a note beside a state that folds is a part of its own in
/// both pictures, and glides from where it was to where it is; inside the
/// picture of the state around it, its old place and its new one showed
/// both at once.
#[test]
fn a_note_glides_on_its_own_when_its_state_folds() {
    let open = "stateDiagram-v2\n[*] --> Playing\nstate Playing {\n  [*] --> Live\n  Live --> Multiball\n  state Multiball {\n    [*] --> Three\n    Three --> Two\n  }\n}\nnote right of Multiball\n  then the drain anyway\nend note\n%% parts (Omaspec)\n";
    let folded = format!("{open}%% fold (Omaspec) Multiball\n");
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, open, Language::Mermaid);
    paper.send(Message::Faded);
    assert!(paper.diagram.as_ref().unwrap().drawn.parts);
    paper.set_fold("Multiball", true);
    show_source(&mut paper, &folded, Language::Mermaid);
    let fade = paper.arrival.fade().expect("the fold glides");
    let morph = fade.morph.as_ref().expect("the boxes move");
    let (old, new) = fade.bare.as_ref().expect("both sheets");
    let note = "note:state:Multiball#0";
    assert!(old.part(note).is_some() && new.part(note).is_some());
    let kept = morph
        .kept
        .iter()
        .find(|kept| kept.name == note)
        .unwrap_or_else(|| panic!("kept {:?}, orphans {:?}", morph.kept, morph.orphans));
    // Drawn over Playing, which holds it, not under its picture.
    let over = kept.over.expect("a box holds it");
    assert_eq!(morph.moves[over].name, "Playing");
}
