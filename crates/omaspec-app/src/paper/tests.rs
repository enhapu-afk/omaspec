use super::testing::{answer, drawn, drawn_without_parts};
use super::*;

#[test]
fn a_new_diagram_fades_in_over_the_one_before() {
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(400.0, 300.0)));
    paper.pretend_requested(2);
    let first = drawn("flowchart LR\n  A --> B\n", Language::Mermaid);
    let task = paper.rendered(answer(1, 1, Ok(first)));
    paper.run(task);
    // The first picture comes in from white paper.
    assert_eq!(paper.fading(), Some(0));
    paper.send(Message::Faded);
    assert_eq!(paper.fading(), None);

    let second = drawn("flowchart LR\n  A --> C\n", Language::Mermaid);
    let task = paper.rendered(answer(1, 2, Ok(second)));
    // The old picture stays as it is until the new one is rasterized.
    assert_eq!(paper.fading(), None);
    paper.run(task);
    assert_eq!(paper.fading(), Some(1));

    // Zooming rasterizes again without a fade.
    paper.send(Message::Faded);
    let task = paper.zoom(2.0);
    paper.run(task);
    assert_eq!(paper.fading(), None);
}

#[test]
fn the_caret_aims_at_that_occurrence() {
    let source = "%% Edit\nflowchart LR\n  A[Edit] --> B[Edit]\n";
    let mut paper = Paper::default();
    show_source(&mut paper, source, Language::Mermaid);
    let comment = source.lines().next().unwrap();
    assert!(
        paper
            .place_of(source, 0, comment.find("Edit").unwrap())
            .is_none()
    );
    let line = source.lines().nth(2).unwrap();
    let first = paper
        .place_of(source, 2, line.find("Edit").unwrap())
        .unwrap();
    let second = paper
        .place_of(source, 2, line.rfind("Edit").unwrap())
        .unwrap();
    // Two names on one line light the two boxes, left then right.
    assert!(
        first.x < second.x,
        "left Edit then right Edit: {first:?} vs {second:?}"
    );
    assert!(paper.place_of(source, 2, 0).is_none());

    let source = "flowchart LR\n  A[Edit] --> B\n  C[Edit] --> D\n";
    show_source(&mut paper, source, Language::Mermaid);
    assert!(paper.place_of(source, 0, 0).is_none());
    let earlier = paper
        .place_of(
            source,
            1,
            source.lines().nth(1).unwrap().find("Edit").unwrap(),
        )
        .unwrap();
    let later = paper
        .place_of(
            source,
            2,
            source.lines().nth(2).unwrap().find("Edit").unwrap(),
        )
        .unwrap();
    let center = |rect: Rectangle| (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let (ax, ay) = center(earlier);
    let (cx, cy) = center(later);
    assert!(
        cx > ax + 1.0 || cy > ay + 1.0,
        "the later Edit is another box: {earlier:?} vs {later:?}"
    );
}

#[test]
fn a_phrase_lights_the_shape_that_draws_it() {
    let mut paper = Paper::default();
    let at = |source: &str, row: usize, word: &str| {
        let line = source.split('\n').nth(row).unwrap();
        line.find(word).unwrap()
    };
    let same = |paper: &Paper, source: &str, row: usize, a: &str, b: &str| {
        let left = paper.place_of(source, row, at(source, row, a)).unwrap();
        let right = paper.place_of(source, row, at(source, row, b)).unwrap();
        assert_eq!(left, right, "{a:?} and {b:?} on row {row}");
        left
    };

    let sequence = "\
@startuml
title Logging in
participant \"Web app\" as Web
actor User
User -> Web : open the login page
alt password ok
else wrong password
end
@enduml
";
    show_source(&mut paper, sequence, Language::PlantUml);
    let web = same(&paper, sequence, 2, "Web", "app");
    let alias = paper.place_of(
        sequence,
        2,
        sequence.split('\n').nth(2).unwrap().rfind("Web").unwrap(),
    );
    assert_eq!(alias, Some(web));
    let user = paper
        .place_of(sequence, 3, at(sequence, 3, "User"))
        .unwrap();
    assert_ne!((web.x, web.y), (user.x, user.y));
    let message = same(&paper, sequence, 4, "open", "page");
    assert_ne!((message.x, message.y), (user.x, user.y));
    assert_eq!(
        paper
            .aim_at(sequence, 2, at(sequence, 2, "Web"))
            .map(|aim| aim.mark),
        Some(super::breath::Mark::Shape),
        "a participant is a box"
    );
    assert_eq!(
        paper
            .aim_at(sequence, 3, at(sequence, 3, "User"))
            .map(|aim| aim.mark),
        Some(super::breath::Mark::Words),
        "an actor's name sits under the figure"
    );
    assert_eq!(
        paper
            .aim_at(sequence, 4, at(sequence, 4, "open"))
            .map(|aim| aim.mark),
        Some(super::breath::Mark::Words),
        "a message glows as words"
    );
    // The alias on the message still names the participant.
    let mention = paper.place_of(sequence, 4, at(sequence, 4, "Web")).unwrap();
    assert!(mention.y >= web.y);
    let title = same(&paper, sequence, 1, "Logging", "in");
    assert_ne!((title.x, title.y), (web.x, web.y));
    let ok = same(&paper, sequence, 5, "password", "ok");
    let wrong = same(&paper, sequence, 6, "wrong", "password");
    assert_ne!((ok.x, ok.y), (wrong.x, wrong.y));

    let state = "\
@startuml
state \"Request (idle)\" as Request
state Pick <<choice>>
Request --> Pick : next track
@enduml
";
    show_source(&mut paper, state, Language::PlantUml);
    let request = same(&paper, state, 1, "Request", "idle");
    let alias = paper.place_of(
        state,
        1,
        state.split('\n').nth(1).unwrap().rfind("Request").unwrap(),
    );
    assert_eq!(alias, Some(request));
    let again = paper.place_of(state, 3, at(state, 3, "Request")).unwrap();
    assert_eq!(again, request);
    let pick = paper.place_of(state, 2, at(state, 2, "Pick")).unwrap();
    let choice = paper.place_of(state, 2, at(state, 2, "choice")).unwrap();
    assert_eq!(pick, choice);
    assert_ne!((pick.x, pick.y), (request.x, request.y));
    let track = same(&paper, state, 3, "next", "track");
    assert_ne!((track.x, track.y), (request.x, request.y));
    assert_eq!(
        paper
            .aim_at(state, 1, at(state, 1, "Request"))
            .map(|aim| aim.mark),
        Some(super::breath::Mark::Shape),
        "a state is a box"
    );
    assert_eq!(
        paper
            .aim_at(state, 3, at(state, 3, "next"))
            .map(|aim| aim.mark),
        Some(super::breath::Mark::Words),
        "a label on a line glows as words"
    );

    // A transition inside a composite sits in that box. The light stays
    // on the words.
    let nested = "\
@startuml
left to right direction
state One
state Two
state Three
state Four
state Playing {
  [*] --> Buffering
  Buffering --> Streaming : buffer full
  Streaming --> Buffering : network slow
}
@enduml
";
    show_source(&mut paper, nested, Language::PlantUml);
    let playing = paper
        .aim_at(nested, 6, at(nested, 6, "Playing"))
        .expect("Playing");
    assert_eq!(playing.mark, super::breath::Mark::Shape);
    let full = paper
        .aim_at(nested, 8, at(nested, 8, "buffer"))
        .expect("buffer full");
    assert_eq!(
        full.mark,
        super::breath::Mark::Words,
        "buffer full glows, it does not take Playing"
    );
    assert!(full.rect.width < playing.rect.width * 0.8);
    assert!(full.rect.height < playing.rect.height * 0.5);
    let slow = paper
        .aim_at(nested, 9, at(nested, 9, "network"))
        .expect("network slow");
    assert_eq!(slow.mark, super::breath::Mark::Words);

    let activity = "\
@startuml
start
:Merge the last change;
if (all green?) then (yes)
  :Publish packages;
else (no)
  :Fix and tag again;
endif
stop
@enduml
";
    show_source(&mut paper, activity, Language::PlantUml);
    let merge = same(&paper, activity, 2, "Merge", "change");
    let guard = same(&paper, activity, 3, "green", "all");
    let yes = paper
        .place_of(
            activity,
            3,
            activity.split('\n').nth(3).unwrap().rfind("yes").unwrap(),
        )
        .unwrap();
    assert_ne!((guard.x, guard.y), (yes.x, yes.y));
    let publish = same(&paper, activity, 4, "Publish", "packages");
    assert_ne!((publish.x, publish.y), (merge.x, merge.y));
    assert_eq!(
        paper
            .aim_at(activity, 2, at(activity, 2, "Merge"))
            .map(|aim| aim.mark),
        Some(super::breath::Mark::Shape),
        "an activity is a box"
    );
    assert_eq!(
        paper
            .aim_at(activity, 3, at(activity, 3, "yes"))
            .map(|aim| aim.mark),
        Some(super::breath::Mark::Words),
        "a branch label glows as words"
    );

    let structure = "\
@startuml
interface Sellable
component Billing <<service>>
@enduml
";
    show_source(&mut paper, structure, Language::PlantUml);
    assert!(
        paper
            .place_of(structure, 1, at(structure, 1, "interface"))
            .is_some()
    );
    assert!(
        paper
            .place_of(structure, 1, at(structure, 1, "Sellable"))
            .is_some()
    );
    assert!(
        paper
            .place_of(structure, 2, at(structure, 2, "Billing"))
            .is_some()
    );
    assert!(
        paper
            .place_of(structure, 2, at(structure, 2, "service"))
            .is_some()
    );

    let timing = "\
@startuml
robust \"Web Browser\" as WB
WB has Idle, Processing
@0
WB is Idle
@100
WB is Processing
@enduml
";
    show_source(&mut paper, timing, Language::PlantUml);
    let browser = same(&paper, timing, 1, "Web", "Browser");
    let short = paper
        .place_of(
            timing,
            1,
            timing.split('\n').nth(1).unwrap().rfind("WB").unwrap(),
        )
        .unwrap();
    assert_eq!(short, browser);
    let idle = paper.place_of(timing, 2, at(timing, 2, "Idle")).unwrap();
    let again = paper.place_of(timing, 4, at(timing, 4, "Idle")).unwrap();
    assert_eq!(idle, again);
    assert_ne!((idle.x, idle.y), (browser.x, browser.y));
    let processing = paper
        .place_of(timing, 2, at(timing, 2, "Processing"))
        .unwrap();
    assert_ne!((processing.x, processing.y), (idle.x, idle.y));
    let later = paper.place_of(timing, 2, at(timing, 2, "WB")).unwrap();
    assert_eq!(later, browser);

    let flow = "\
@startflow
title Charge control
block Filter \"Low pass\" {
  in voltage
  out limited
}
@endflow
";
    show_source(&mut paper, flow, Language::PlantUml);
    let title = same(&paper, flow, 1, "Charge", "control");
    let block = same(&paper, flow, 2, "Low", "pass");
    // The block's own name is not the quoted label.
    assert!(paper.place_of(flow, 2, at(flow, 2, "Filter")).is_none());
    assert_ne!((title.x, title.y), (block.x, block.y));
    assert!(paper.place_of(flow, 3, at(flow, 3, "voltage")).is_some());
}

#[test]
fn a_breath_fades_in_and_glides() {
    let mut paper = Paper::default();
    let here = Rectangle::new(Point::new(10.0, 20.0), Size::new(30.0, 40.0));
    let there = Rectangle::new(Point::new(80.0, 20.0), Size::new(30.0, 40.0));
    paper.set_breath(Some(super::breath::Aim::shape(here)));
    assert_eq!(paper.breath_target(), Some(here));
    let started = paper.breath.unwrap().start;
    paper.set_breath(Some(super::breath::Aim::shape(here)));
    assert_eq!(paper.breath.unwrap().start, started);

    // Still invisible, so a new place spreads where it is going.
    paper.set_breath(Some(super::breath::Aim::shape(there)));
    let breath = paper.breath.unwrap();
    assert!(!breath.travels);
    assert_eq!(breath.from, there);
    assert_eq!(breath.to, there);
    assert_eq!(breath.from_a, 0.0);

    let now = Instant::now();
    paper.breath.as_mut().unwrap().start = now - std::time::Duration::from_millis(80);
    let shown = paper.breath.unwrap().at(now);
    // 80 ms of a 220 ms arrival: quick at first, 1 - (1 - 80/220)³.
    assert!(
        (shown.alpha - 0.74).abs() < 0.05 && (shown.spread - 0.74).abs() < 0.05,
        "{shown:?}"
    );

    paper.breath.as_mut().unwrap().start =
        now - (super::breath::ARRIVE + std::time::Duration::from_millis(20));
    paper.set_breath(Some(super::breath::Aim::shape(here)));
    let breath = paper.breath.unwrap();
    assert!(breath.travels);
    assert!((breath.from.x - there.x).abs() < 0.5, "{:?}", breath.from);
    assert_eq!(breath.to, here);
    assert!(breath.from_a > 0.9, "{}", breath.from_a);

    paper.set_breath(None);
    assert!(paper.breath_target().is_none());
    let leaving = paper.breath.unwrap();
    assert!(paper.breath_moving(Instant::now()));
    paper.set_breath(None);
    assert_eq!(paper.breath.unwrap().start, leaving.start);

    paper.breath.as_mut().unwrap().start =
        Instant::now() - (super::breath::LEAVE + std::time::Duration::from_millis(20));
    paper.set_breath(None);
    assert!(paper.breath.is_none());
}

#[test]
fn new_pixels_wait_until_the_view_stops_gliding() {
    let door =
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, door, Language::PlantUml);
    let before = paper.overview.shown.as_ref().map(|r| r.region);
    assert!(before.is_some());
    let zoomed = paper.zoom(1.2);
    assert!(paper.glide.is_some());
    // The glide outlasts the rasterizing here, however loaded the machine.
    paper.glide = paper
        .glide
        .map(|g| g.taking(std::time::Duration::from_secs(60)));
    let asked = paper.overview.asked;
    paper.run(zoomed);
    assert!(paper.held_overview.is_some(), "held while it glides");
    assert_eq!(paper.overview.shown.as_ref().map(|r| r.region), before);
    // Once it stops, the held pixels are shown.
    paper.glide = paper.glide.map(|g| g.taking(std::time::Duration::ZERO));
    paper.send(Message::Glided);
    assert!(paper.held_overview.is_none());
    assert_eq!(paper.overview.asked, asked);
    assert!(paper.overview.shown.is_some());

    // Without a picture on screen, pixels come at once.
    let mut bare = Paper::default();
    bare.send(Message::Viewport(Size::new(800.0, 600.0)));
    let mut reply = answer(1, bare.pretend_next(), Ok(drawn(door, Language::PlantUml)));
    reply.source = door.into();
    reply.language = Language::PlantUml;
    let drawn = bare.rendered(reply);
    let zoomed = bare.zoom(1.2);
    bare.run(drawn);
    bare.run(zoomed);
    assert!(bare.held_overview.is_none() && bare.overview.shown.is_some());
}

#[test]
fn a_pane_that_only_grows_or_shrinks_in_height_holds_the_picture_then_glides() {
    // A play's bar takes a second line: the pane loses 50 at its foot.
    let door =
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    show_source(&mut paper, door, Language::PlantUml);
    let (tall, short) = (Size::new(800.0, 600.0), Size::new(800.0, 550.0));
    let before = paper.view;
    let top = Point::new(10.0, 10.0);
    let on_screen = |view: View, viewport: Size| {
        view.on_screen(
            Rectangle::new(top, Size::new(1.0, 1.0)),
            Rectangle::with_size(viewport),
        )
        .position()
    };
    paper.send(Message::Viewport(short));
    let glide = paper.glide.expect("a fitted picture glides to its new fit");
    // Where it starts, whatever time has passed on a loaded machine.
    let start = on_screen(glide.from(), short);
    let was = on_screen(before, tall);
    assert!(
        (start.x - was.x).abs() < 0.5 && (start.y - was.y).abs() < 0.5,
        "{start:?} {was:?}"
    );
    assert_eq!(paper.view, View::fitted(paper.diagram_size(), short));

    // Zoomed in, not fitted: it stays where it is, without a glide.
    let mut paper = Paper::default();
    paper.send(Message::Viewport(tall));
    show_source(&mut paper, door, Language::PlantUml);
    paper.send(Message::ZoomAt(2.0, Point::new(400.0, 300.0)));
    let zoomed = paper.view;
    paper.send(Message::Viewport(short));
    assert!(paper.glide.is_none());
    let (now, was) = (on_screen(paper.view, short), on_screen(zoomed, tall));
    assert!((now.y - was.y).abs() < 0.5, "{now:?} {was:?}");
}

#[test]
fn the_way_back_out_of_a_link_runs_on_when_the_pane_reports_its_size() {
    // The tab left had a play's bar under its picture: the pane of the
    // diagram gone back to is taller, and says so on the next frame.
    let door =
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 540.0)));
    show_source(&mut paper, door, Language::PlantUml);
    let area = Rectangle::new(Point::new(10.0, 10.0), Size::new(40.0, 20.0));
    let back = View::fitted(paper.diagram_size(), Size::new(800.0, 540.0));
    let _ = paper.zoom_out_of(area, back, None);
    let started = paper.glide.expect("the way back runs");
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    let glide = paper.glide.expect("the way back runs on");
    assert_ne!(glide, started);
    // It ends where the taller pane fits the diagram, and goes on from
    // where it was, not from its start.
    let fitted = View::fitted(paper.diagram_size(), Size::new(800.0, 600.0));
    assert_eq!(paper.view, fitted);
    assert_eq!(glide.at(Instant::now() + glide.duration()), fitted);
    assert!(glide.duration() <= started.duration());

    // The same size again changes nothing.
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    assert_eq!(paper.glide, Some(glide));
}

#[test]
fn a_new_picture_stays_where_it_was_until_it_fades() {
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(400.0, 300.0)));
    show_source(
        &mut paper,
        "@startuml\n[*] --> A\n@enduml\n",
        Language::PlantUml,
    );
    paper.send(Message::Faded);
    let kept = paper.view;
    let task = draw_source(
        &mut paper,
        "@startuml\n[*] --> A\nA --> B\nB --> C\nC --> D\nD --> E\n@enduml\n",
        Language::PlantUml,
    );
    // Fitted already, and the old picture is still drawn at `kept`.
    assert_eq!(
        paper.held_picture().map(|held| held.view_at(paper.view)),
        Some(kept)
    );
    assert!(paper.fading().is_none());
    paper.run(task);
    let fade = paper.arrival.fade().expect("the new picture fades in");
    assert_eq!(fade.old_view, kept);
    assert!(fade.morph.is_some(), "the boxes moved, so they glide");
    let left = fade.shown.remaining(Instant::now());
    assert!(
        left > Duration::from_millis(350) && left < Duration::from_millis(450),
        "the boxes glide in {left:?}"
    );
    assert!(paper.held_picture().is_none());
}

#[test]
fn the_same_layout_keeps_the_short_fade() {
    let source = "@startuml\n[*] --> A\nA --> B\n@enduml\n";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(400.0, 300.0)));
    show_source(&mut paper, source, Language::PlantUml);
    paper.send(Message::Faded);
    show_source(&mut paper, source, Language::PlantUml);
    let fade = paper.arrival.fade().expect("a new raster still fades");
    assert!(fade.morph.is_none(), "the boxes did not move");
    let left = fade.shown.remaining(Instant::now());
    assert!(
        left < Duration::from_millis(200),
        "the same layout fades in {left:?}"
    );
}

#[test]
fn the_caret_on_a_containers_name_is_the_container() {
    let at = |source: &str, row: usize, word: &str| {
        let line = source.split('\n').nth(row).unwrap();
        line.find(word).unwrap()
    };
    let cases = [
        (
            "@startuml\npackage Shop {\n  class Cart\n}\n@enduml\n",
            Language::PlantUml,
            1,
            "Shop",
        ),
        (
            "@startuml\nstate Playing {\n  state Buffering\n}\n@enduml\n",
            Language::PlantUml,
            1,
            "Playing",
        ),
        (
            "flowchart LR\n  subgraph Shop\n    Cart\n  end\n",
            Language::Mermaid,
            1,
            "Shop",
        ),
    ];
    for (source, language, row, name) in cases {
        let mut paper = Paper::default();
        show_source(&mut paper, source, language);
        let col = at(source, row, name);
        assert_eq!(
            paper.box_at_caret(source, row, col).as_deref(),
            Some(name),
            "{source}"
        );
    }
    // What the container holds is still its own box.
    let source = "@startuml\npackage Shop {\n  class Cart\n}\n@enduml\n";
    let mut paper = Paper::default();
    show_source(&mut paper, source, Language::PlantUml);
    assert_eq!(
        paper
            .box_at_caret(source, 2, at(source, 2, "Cart"))
            .as_deref(),
        Some("Cart")
    );
}

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

#[test]
fn the_way_back_waits_until_the_diagram_has_a_picture() {
    let door =
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(800.0, 600.0)));
    let drawn = draw_source(&mut paper, door, Language::PlantUml);
    assert!(paper.overview.shown.is_none());
    let area = Rectangle::new(Point::new(10.0, 10.0), Size::new(40.0, 20.0));
    let back = paper.view;
    let from = Showing {
        size: Size::new(200.0, 100.0),
        place: Rectangle::new(Point::new(40.0, 30.0), Size::new(80.0, 40.0)),
        picture: None,
    };
    let out = paper.zoom_out_of(area, back, Some(from));
    assert!(paper.glide.is_none(), "no glide over an empty pane");
    let passage = paper.passage.as_ref().expect("waiting to leave");
    assert!(passage.goal.is_some());
    assert_eq!(passage.shown(0.2), 1.0);
    paper.run(drawn);
    paper.run(out);
    let glide = paper
        .glide
        .expect("the way back starts once the picture is in");
    assert!(paper.passage.as_ref().unwrap().goal.is_none());
    assert_eq!(glide.at(Instant::now() + glide.duration()), back);
}

#[test]
fn the_way_back_stops_waiting_for_a_pan_or_a_picture_that_failed() {
    let door = "@startuml\n[*] --> Closed\nClosed --> Open : open\n@enduml\n";
    let area = Rectangle::new(Point::new(10.0, 10.0), Size::new(40.0, 20.0));
    let from = || Showing {
        size: Size::new(200.0, 100.0),
        place: Rectangle::new(Point::new(40.0, 30.0), Size::new(80.0, 40.0)),
        picture: None,
    };
    let waiting = || {
        let mut paper = Paper::default();
        paper.send(Message::Viewport(Size::new(800.0, 600.0)));
        // Its picture on its way.
        let _ = draw_source(&mut paper, door, Language::PlantUml);
        let back = paper.view;
        let _ = paper.zoom_out_of(area, back, Some(from()));
        assert!(paper.passage.as_ref().unwrap().goal.is_some());
        (paper, back)
    };
    // A pan lifts the veil where the way back was going.
    let (mut paper, back) = waiting();
    paper.send(Message::Pan(Vector::new(5.0, 0.0)));
    assert!(paper.passage.is_none());
    assert!((paper.view.zoom - back.zoom).abs() < 1e-4);
    // A picture that could not be drawn does too.
    let (mut paper, back) = waiting();
    let seq = paper.overview.seq();
    paper.send(Message::Overview(seq, None));
    assert!(paper.passage.is_none(), "the veil stays up for nothing");
    assert_eq!(paper.view, back);
}

#[test]
fn the_picture_held_for_the_next_one_moves_with_the_view() {
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(400.0, 300.0)));
    show_source(
        &mut paper,
        "@startuml\n[*] --> A\n@enduml\n",
        Language::PlantUml,
    );
    paper.send(Message::Faded);
    let kept = paper.view;
    // The new one's pictures on their way while the view moves.
    let _ = draw_source(
        &mut paper,
        "@startuml\n[*] --> A\nA --> B\nB --> C\nC --> D\n@enduml\n",
        Language::PlantUml,
    );
    let held = |paper: &Paper| {
        paper
            .held_picture()
            .map(|held| held.view_at(paper.view))
            .expect("the old picture is held")
    };
    assert_eq!(held(&paper), kept);
    // A pan moves it as it moves the new diagram, by the same pixels.
    let _ = paper.update(Message::Pan(Vector::new(30.0, -10.0)));
    let moved = held(&paper);
    let panned = kept.pan(Vector::new(30.0, -10.0));
    assert!((moved.zoom - panned.zoom).abs() < 1e-4, "{moved:?}");
    assert!(moved.center.distance(panned.center) < 1e-3, "{moved:?}");
    // So does a zoom, around the pointer.
    let at = Point::new(100.0, 80.0);
    let _ = paper.update(Message::ZoomAt(1.5, at));
    let zoomed = held(&paper);
    let expected = panned.zoom_at(1.5, at, paper.viewport);
    assert!((zoomed.zoom - expected.zoom).abs() < 1e-3, "{zoomed:?}");
    assert!(zoomed.center.distance(expected.center) < 1e-2, "{zoomed:?}");
    // A raster that cannot be drawn lets it go.
    let seq = paper.overview.seq();
    paper.send(Message::Overview(seq, None));
    assert!(paper.held_picture().is_none());
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
fn a_zoom_gliding_when_a_picture_comes_goes_on_to_its_end() {
    let source = "@startuml\n[*] --> A\nA --> B\n@enduml\n";
    let mut paper = Paper::default();
    paper.send(Message::Viewport(Size::new(400.0, 300.0)));
    show_source(&mut paper, source, Language::PlantUml);
    paper.send(Message::Faded);
    // Its pixels on their way.
    let _ = paper.update(Message::ZoomStep(2.0, Point::new(200.0, 150.0)));
    let asked = paper.view;
    assert!(paper.glide.is_some());
    let _ = draw_source(&mut paper, source, Language::PlantUml);
    let glide = paper.glide.expect("the zoom glides on");
    assert_eq!(paper.view, asked);
    assert_eq!(glide.at(Instant::now() + glide.duration()), asked);
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

/// Puts `source` on the sheet, as a render would, and makes the pictures
/// it asks for, as the window makes them.
fn show_source(paper: &mut Paper, source: &str, language: Language) {
    let task = draw_source(paper, source, language);
    paper.run(task);
}

/// Hands `source` to the pane, as a render would: the tasks that make its
/// pictures are still to run.
fn draw_source(paper: &mut Paper, source: &str, language: Language) -> Task<Message> {
    let mut reply = answer(1, paper.pretend_next(), Ok(drawn(source, language)));
    reply.source = source.into();
    reply.language = language;
    paper.rendered(reply)
}
