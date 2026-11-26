use super::draw::*;
use super::*;
#[test]
fn lights_fade_on_and_off() {
    let t0 = Instant::now();
    let ms = |n: u64| t0 + Duration::from_millis(n);
    let mut events = Vec::new();
    assert_eq!(level(&events, ms(100)), 0.0);
    light(&mut events, ms(0), true);
    assert!(level(&events, ms(40)) > 0.0 && level(&events, ms(40)) < 1.0);
    assert!((level(&events, ms(1000)) - 1.0).abs() < 1e-6);
    light(&mut events, ms(1000), false);
    assert!(level(&events, ms(1100)) < 1.0);
    assert_eq!(level(&events, ms(5000)), 0.0);
    // On again before it went quite out: it rises from where it was.
    light(&mut events, ms(1100), true);
    assert!(level(&events, ms(1100)) > 0.3);
}

#[test]
fn the_lit_line_follows_the_light_through_a_step_that_follows_by_itself() {
    // A state is a halt; a composite state whose region has ended goes on
    // by itself.
    let source = "@startuml\n[*] --> Brewing\nstate Brewing {\n  [*] --> Heating\n  Heating --> [*] : hot\n}\nBrewing --> Done\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
    // Long after the start has arrived, the writer picks hot.
    let t1 = t0 + Duration::from_secs(10);
    let _ = play.take_jump(t1);
    play.ask(Ask::Take(0), t1);
    let Play::Machine(machine) = &play else {
        panic!("no machine");
    };
    let lines: Vec<_> = machine.lines.clone();
    assert_eq!(
        lines.iter().map(|&(_, line)| line).collect::<Vec<_>>(),
        [4, 6],
        "hot, then Brewing --> Done by itself"
    );
    assert!(lines[0].0 < lines[1].0);
    assert_eq!(play.take_jump(lines[0].0), Some(4));
    assert_eq!(play.take_jump(lines[1].0 - Duration::from_millis(1)), None);
    assert_eq!(play.take_jump(lines[1].0), Some(6));
}

#[test]
fn lines_nobody_takes_do_not_pile_up() {
    // The preview lights no line; a long play keeps only the recent.
    let source = "@startuml\n[*] --> Off\nOff --> On : switch\nOn --> Off : switch\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
    for n in 1..=50 {
        play.ask(Ask::Take(0), t0 + Duration::from_secs(10 * n));
    }
    assert!(play.line_waiting());
    let Play::Machine(machine) = &play else {
        panic!("no machine");
    };
    assert_eq!(machine.lines.len(), 1);
}

/// From the start the bar knows where the machine can be and what it
/// offers there, so it can keep its rows: every state of `never.mmd`'s
/// kind, each once, and both regions of a parallel state together.
#[test]
fn the_offer_knows_every_place_of_the_play() {
    let source = "@startuml\n[*] --> Unstable\nUnstable --> Attempt1 : try 1\nAttempt1 --> Unstable : reverted\nUnstable --> Attempt2 : try 2\nAttempt2 --> Unstable : reverted\nUnstable --> Stable : Rust 1.100\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let play = Play::new(source, Language::PlantUml, &map, Instant::now()).unwrap();
    let Offer::Machine { every, .. } = play.offer() else {
        panic!("no machine");
    };
    let places: Vec<(&str, usize)> = every
        .iter()
        .map(|(at, options)| (at.as_str(), options.len()))
        .collect();
    assert_eq!(
        places,
        [
            ("Unstable", 3),
            ("Attempt1", 1),
            ("Attempt2", 1),
            ("Stable", 0)
        ]
    );
    let parallel = "@startuml\n[*] --> On\nstate On {\n  [*] --> A\n  A --> B : a\n  --\n  [*] --> X\n  X --> Y : x\n}\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(parallel, None).unwrap();
    let play = Play::new(parallel, Language::PlantUml, &map, Instant::now()).unwrap();
    let Offer::Machine { every, .. } = play.offer() else {
        panic!("no machine");
    };
    assert_eq!(every.len(), 4, "{every:?}");
    assert_eq!(every[0].1.len(), 2, "{every:?}");
}

/// The bar says what the diagram shows: a state by its long name, an
/// option with its action, and no end while another way still runs.
#[test]
fn the_offer_speaks_as_the_diagram_does() {
    let source = "@startuml\nstate \"Fix attempt\" as FixAttempt\n[*] --> Working\nWorking --> FixAttempt : restarts / reroute\n[*] --> Send\nSend --> [*] : sent\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
    let Offer::Machine { options, at, .. } = play.offer() else {
        panic!("no machine");
    };
    assert_eq!(at, "Working, Send");
    assert_eq!(
        options,
        [
            ("restarts / reroute".to_owned(), "Fix attempt".to_owned()),
            ("sent".to_owned(), "end".to_owned()),
        ]
    );
    play.ask(Ask::Take(1), t0);
    let Offer::Machine {
        options, at, ended, ..
    } = play.offer()
    else {
        panic!("no machine");
    };
    assert!(!ended, "{at}");
    // One way at its end, the other still running.
    assert_eq!(at, "Working, end");
    play.ask(Ask::Take(0), t0 + Duration::from_secs(10));
    let Offer::Machine { at, ended, .. } = play.offer() else {
        panic!("no machine");
    };
    assert!(!ended && at == "Fix attempt, end", "{at} {options:?}");
}

#[test]
fn a_machine_plays_steps_one_after_another() {
    let source = "@startuml\n[*] --> Closed\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
    let Offer::Machine { options, at, .. } = play.offer() else {
        panic!("no machine");
    };
    assert_eq!(options, [("lock".to_owned(), "Locked".to_owned())]);
    assert_eq!(at, "Closed");
    play.ask(Ask::Take(0), t0);
    // Each line lights when its step's light sets off: first the start.
    assert_eq!(play.take_jump(t0), None);
    assert_eq!(play.take_jump(t0 + VEIL_FADE), Some(1));
    assert_eq!(play.take_jump(t0 + Duration::from_secs(10)), Some(2));
    assert_eq!(play.take_jump(t0 + Duration::from_secs(10)), None);
    assert!(play.animating(t0 + Duration::from_millis(300)));
    assert!(!play.animating(t0 + Duration::from_secs(10)));
    let Offer::Machine {
        at,
        steps,
        can_go_back,
        ..
    } = play.offer()
    else {
        panic!("no machine");
    };
    assert_eq!((at.as_str(), steps, can_go_back), ("Locked", 1, true));
    play.ask(Ask::Back, t0);
    let Offer::Machine { at, steps, .. } = play.offer() else {
        panic!("no machine");
    };
    assert_eq!((at.as_str(), steps), ("Closed", 0));
    // The light fades back. The travel that was on the way does not
    // finish first.
    assert!(play.animating(t0 + Duration::from_millis(200)));
    assert!(!play.animating(t0 + Duration::from_millis(1500)));
}

#[test]
fn a_history_is_on_the_map_and_named_in_the_bar() {
    let source = "@startuml\n[*] --> Off\nOff --> On : power\nstate On {\n  [*] --> Low\n  Low --> High : up\n}\nOn --> Off : pause\nOff --> On[H] : resume\nOff --> On[H*] : deep\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let machine = Machine::read(source, Language::PlantUml).unwrap();
    for (state, place) in machine.states.iter().zip(&machine.place(&map).states) {
        assert!(place.is_some(), "{state:?} not on the map");
    }
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
    let Offer::Machine { options, .. } = play.offer() else {
        panic!("no machine");
    };
    let option = |label: &str, to: &str| (label.to_owned(), to.to_owned());
    assert_eq!(
        options,
        [
            option("power", "On"),
            option("resume", "history of On"),
            option("deep", "deep history of On"),
        ]
    );
    // On, Low, High, back to Off, and through the history to High.
    for n in [0, 0, 0, 1] {
        play.ask(Ask::Take(n), t0);
    }
    let Offer::Machine { at, .. } = play.offer() else {
        panic!("no machine");
    };
    assert_eq!(at, "High");
}

/// A machine beside a scenario plays the participant's way as a trace
/// plays, with a bar that says where it is and nothing to press; a message
/// it cannot take flashes where it is, and runs red in the scenario.
#[test]
fn a_machine_beside_its_scenario_flashes_where_it_cannot_follow() {
    let source = "@startuml Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
    let scenario = "@startuml\nGuest -> Door : lock\nGuest -> Door : open\n@enduml\n";
    let specs = omaspec_model::Specs::new(vec![
        Spec::read(source, Language::PlantUml, "door.puml").unwrap(),
    ]);
    let step = omaspec_model::lockstep(scenario, Language::PlantUml, &specs).unwrap();
    let door = &step.players[0];
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let t0 = Instant::now();
    let mut play = Play::followed(
        &map,
        door.spec.machine.clone(),
        door.followed.clone(),
        "keys.puml",
        "Door",
        true,
        t0,
    )
    .unwrap();
    let flashes = |play: &Play| match play {
        Play::Machine(machine) => machine.flashes.len(),
        Play::Scenario(_) => panic!("a machine"),
    };
    play.ask(Ask::Next, t0);
    assert_eq!(flashes(&play), 0);
    play.ask(Ask::Next, t0);
    assert_eq!(flashes(&play), 1);
    // The play asks for frames until the flash is over.
    let flash = match &play {
        Play::Machine(machine) => machine.flashes[0],
        Play::Scenario(_) => panic!("a machine"),
    };
    assert!(play.animating(flash + FLASH - Duration::from_millis(10)));
    assert!(!play.animating(flash + FLASH + Duration::from_secs(1)));
    let Offer::Trace {
        beside,
        finding,
        at,
        ..
    } = play.offer()
    else {
        panic!("a trace");
    };
    assert!(beside);
    assert_eq!(at, "Locked");
    assert_eq!(finding.map(|(s, _)| s), Some(Severity::Error));
    // Going back takes the flash away.
    play.ask(Ask::Back, t0);
    assert_eq!(flashes(&play), 0);

    // An event the machine has no way for, while where it is is open:
    // nothing is lit, and the pane's edge flashes instead.
    let knock = "@startuml\nGuest -> Door : knock\n@enduml\n";
    let step = omaspec_model::lockstep(knock, Language::PlantUml, &specs).unwrap();
    let door = &step.players[0];
    let mut knocked = Play::followed(
        &map,
        door.spec.machine.clone(),
        door.followed.clone(),
        "knock.puml",
        "Door",
        true,
        t0,
    )
    .unwrap();
    knocked.ask(Ask::Next, t0);
    assert_eq!(flashes(&knocked), 1);
    let Offer::Trace { at, .. } = knocked.offer() else {
        panic!("a trace");
    };
    assert_eq!(at, "");

    // The scenario runs the message's light red, and says when it arrives.
    let (_, map) = omaspec_uml::render_mapped(scenario, None).unwrap();
    let mut play = Play::new(scenario, Language::PlantUml, &map, t0).unwrap();
    assert!(play.arrives().is_none());
    play.mark_wrong(vec![2]);
    play.ask(Ask::Next, t0);
    play.ask(Ask::Next, t0);
    let Play::Scenario(scenario) = &play else {
        panic!("a scenario");
    };
    let look = Look {
        accent: Color::from_rgb8(0x7a, 0xa2, 0xf7),
        veil: Color::BLACK,
        light: Color::WHITE,
        wrong: Color::from_rgb8(0xf7, 0x76, 0x8e),
        font: iced::Font::MONOSPACE,
    };
    assert_eq!(scenario.color(0, &look), look.accent);
    assert_eq!(scenario.color(1, &look), look.wrong);
    assert!(play.arrives().is_some_and(|at| at > t0));
}

#[test]
fn a_trace_plays_line_by_line() {
    let source = "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
    let trace = [
        r#"{"time":"10:30:01","object":"Door","event":"lock"}"#,
        r#"{"object":"Car","event":"park"}"#,
        r#"{"time":"10:30:02","object":"Door","event":"open"}"#,
        r#"{"object":"Door","event":"unlock","state":"Closed"}"#,
    ]
    .join("\n");
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let t0 = Instant::now();
    let mut play = Play::trace(
        source,
        Language::PlantUml,
        &map,
        "door.puml",
        &trace,
        "run.jsonl",
        "Door",
        t0,
    )
    .unwrap();
    let bar = |play: &Play| match play.offer() {
        Offer::Trace {
            played,
            line,
            said,
            at,
            finding,
            ..
        } => (played, line, said, at, finding.map(|(s, _)| s)),
        other => panic!("{other:?}"),
    };
    assert_eq!(bar(&play), (0, None, String::new(), "Closed".into(), None));
    play.ask(Ask::Next, t0);
    assert_eq!(
        play.take_jump(t0 + Duration::from_secs(10)),
        Some(4),
        "the lock line of the diagram"
    );
    assert_eq!(
        bar(&play),
        (1, Some(1), "lock".into(), "Locked".into(), None)
    );
    // Digits pick nothing in a trace.
    play.ask(Ask::Take(0), t0);
    play.ask(Ask::Next, t0);
    assert_eq!(
        bar(&play),
        (
            2,
            Some(3),
            "open".into(),
            "Locked".into(),
            Some(Severity::Error)
        )
    );
    play.ask(Ask::Next, t0);
    assert_eq!(
        bar(&play),
        (3, Some(4), "unlock → Closed".into(), "Closed".into(), None)
    );
    play.ask(Ask::Next, t0);
    assert_eq!(bar(&play).0, 3, "the end of the trace");
    play.ask(Ask::Back, t0);
    assert_eq!(bar(&play).0, 2);
    play.ask(Ask::Restart, t0);
    assert_eq!(bar(&play).0, 0);
    assert!(!play.animating(t0 + Duration::from_secs(10)));
    let err = Play::trace(
        source,
        Language::PlantUml,
        &map,
        "door.puml",
        &trace,
        "run.jsonl",
        "Ghost",
        t0,
    )
    .err();
    assert_eq!(err.as_deref(), Some("run.jsonl has no line of Ghost"));
}

/// A note of its own is a step of the play, without a light along an
/// arrow (REQ-106); the counter counts it.
#[test]
fn a_scenario_s_note_is_a_step_of_its_own() {
    let source = "@startuml\nA -> B : one\nnote over A, B: between\nB -> A : two\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
    assert_eq!(
        play.offer(),
        Offer::Scenario {
            at: 0,
            of: 3,
            commits: false
        }
    );
    play.ask(Ask::Next, t0);
    play.ask(Ask::Next, t0);
    // On the note: its line is where the play stands.
    assert_eq!(play.take_jump(t0), Some(2));
}

#[test]
fn a_scenario_unfolds_message_by_message() {
    let source = "@startuml\nA -> B : one\nB -> A : two\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
    assert_eq!(
        play.offer(),
        Offer::Scenario {
            at: 0,
            of: 2,
            commits: false
        }
    );
    play.ask(Ask::Next, t0);
    play.ask(Ask::Next, t0);
    play.ask(Ask::Next, t0);
    assert_eq!(
        play.offer(),
        Offer::Scenario {
            at: 2,
            of: 2,
            commits: false
        }
    );
    assert_eq!(play.take_jump(t0), Some(2));
    play.ask(Ask::Restart, t0);
    assert_eq!(
        play.offer(),
        Offer::Scenario {
            at: 0,
            of: 2,
            commits: false
        }
    );
    let flow = Play::new(
        "@startuml\nstart\n:a;\nstop\n@enduml\n",
        Language::PlantUml,
        &Map::default(),
        t0,
    );
    assert!(flow.is_err());
}

#[test]
fn mermaid_machines_find_their_places() {
    let source = "stateDiagram-v2\n    [*] --> Idle\n    Idle --> Busy : job\n    state Busy {\n        [*] --> Work\n        Work --> Check : done\n        --\n        [*] --> Watch\n        Watch --> Watch : tick\n    }\n    Busy --> Idle : stop\n    Idle --> [*] : off\n";
    let drawn = super::super::testing::drawn(source, Language::Mermaid);
    let machine = Machine::read(source, Language::Mermaid).unwrap();
    let placed = machine.place(&drawn.map);
    for (state, place) in machine.states.iter().zip(&placed.states) {
        assert!(place.is_some(), "{state:?} not on {:#?}", drawn.map.states);
    }
    for (t, route) in machine.transitions.iter().zip(&placed.routes) {
        assert!(route.is_some(), "{t:?} not on {:#?}", drawn.map.transitions);
    }
    let mut play = Play::new(source, Language::Mermaid, &drawn.map, Instant::now()).unwrap();
    play.ask(Ask::Take(0), Instant::now());
    let Offer::Machine { at, .. } = play.offer() else {
        panic!("no machine");
    };
    assert_eq!(at, "Work, Watch");
}

#[test]
fn the_next_message_fades_the_line_it_leaves() {
    let source = "@startuml\nA -> B : one\nB -> A : two\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
    play.ask(Ask::Next, t0);
    let later = t0 + Duration::from_secs(2);
    play.ask(Ask::Next, later);
    let Play::Scenario(scenario) = &play else {
        panic!("scenario");
    };
    let left = scenario
        .leave
        .as_ref()
        .expect("the first message fades out");
    assert_eq!(left.index, 0);
    assert!(
        (left.light.head - 1.0).abs() < 0.01,
        "the light had arrived, {}",
        left.light.head
    );
    // Two seconds later the line has lingered down to half. The fade
    // starts there, not back at the full resting brightness.
    assert!(
        (left.light.rest - 0.45).abs() < 0.01,
        "the fade starts from the dimmed line, {}",
        left.light.rest
    );
    assert!(
        left.light.extra < 0.01,
        "the head had dissolved, {}",
        left.light.extra
    );
}

/// A box of a scenario under the veil keeps its color: the veil dims
/// what lies on it toward the box's own fill, so the box is as even below
/// the veil's edge as above it, and only the paper beside it darkens.
#[test]
fn a_box_under_a_scenario_s_veil_keeps_its_color() {
    use iced::advanced::renderer::{Headless, Style};
    use iced::widget::canvas::{Canvas, Geometry, Program};
    use iced::{Element, Length, Renderer, Size, Theme, mouse};

    let source = "@startuml\nparticipant Alice\nbox \"Shop\" #E0E8FF\nparticipant Bob\nparticipant Carol\nend box\nAlice -> Bob : order\nBob -> Carol : total\nCarol --> Bob : sum\nBob --> Alice : price\n@enduml\n";
    let (svg, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let size = |name: &str| -> f32 {
        let at = svg.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
        svg[at..].split('"').next().unwrap().parse().unwrap()
    };
    let paper = Rectangle::new(Point::ORIGIN, Size::new(size("width"), size("height")));
    let ground = map.grounds[0];
    let fill = Color::from_rgb8(ground.fill.0, ground.fill.1, ground.fill.2);
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
    play.ask(Ask::Next, t0);
    let now = t0 + Duration::from_secs(10);

    struct Painted<'a>(&'a dyn Fn(&mut Frame));
    impl Program<()> for Painted<'_> {
        type State = ();
        fn draw(
            &self,
            _: &(),
            renderer: &Renderer,
            _: &Theme,
            bounds: Rectangle,
            _: mouse::Cursor,
        ) -> Vec<Geometry> {
            let mut frame = Frame::new(renderer, bounds.size());
            (self.0)(&mut frame);
            vec![frame.into_geometry()]
        }
    }
    let paint = |frame: &mut Frame| {
        // The picture: white paper, and the box on it.
        frame.fill_rectangle(paper.position(), paper.size(), Color::WHITE);
        let r = ground.rect;
        frame.fill_rectangle(Point::new(r.x, r.y), Size::new(r.w, r.h), fill);
        let place = |r: Rect| Rectangle::new(Point::new(r.x, r.y), Size::new(r.w, r.h));
        let view = Viewer {
            paper,
            scale: 1.0,
            place: &place,
            paint: &|_, _, _| {},
        };
        let look = Look {
            accent: Color::from_rgb8(0xff, 0x9e, 0x3b),
            veil: Color::from_rgb8(0x1a, 0x1b, 0x26),
            light: Color::WHITE,
            wrong: Color::from_rgb8(0xf7, 0x76, 0x8e),
            font: iced::Font::MONOSPACE,
        };
        play.draw_under(frame, &look, &view, now);
    };
    let mut renderer = iced::futures::executor::block_on(Renderer::new(
        iced::Font::DEFAULT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("tiny-skia draws without a GPU");
    let element: Element<'_, (), Theme, Renderer> = Canvas::new(Painted(&paint))
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    let (w, h) = (paper.width.ceil() as u32, paper.height.ceil() as u32);
    let mut ui = iced_runtime::UserInterface::build(
        element,
        Size::new(w as f32, h as f32),
        Default::default(),
        &mut renderer,
    );
    let style = Style {
        text_color: Color::WHITE,
    };
    ui.draw(
        &mut renderer,
        &Theme::Dark,
        &style,
        mouse::Cursor::Unavailable,
    );
    let rgba = renderer.screenshot(iced::Size::new(w, h), 1.0, Color::BLACK);
    let pixel = |x: f32, y: f32| {
        let at = ((y as u32 * w + x as u32) * 4) as usize;
        [rgba[at], rgba[at + 1], rgba[at + 2]]
    };
    // Far below the veil's edge: in the box, its own color; beside it, the
    // paper dimmed toward the window's background.
    let r = ground.rect;
    let low = r.y + r.h - 12.0;
    let in_box = pixel(r.x + r.w / 2.0, low);
    let want = [0xE0, 0xE8, 0xFF];
    assert!(
        (0..3).all(|c| in_box[c].abs_diff(want[c]) <= 3),
        "the box under the veil is {in_box:?}, not {want:?}"
    );
    let beside = pixel(r.x + r.w + 8.0, low);
    assert!(beside[0] < 160, "the paper beside it is veiled: {beside:?}");
}

/// A git graph plays commit by commit (REQ-037), the veil drawn back the
/// way its time runs: to the right across the page, up from below; a
/// merge's light runs along what it took in.
#[test]
fn a_git_graph_plays_commit_by_commit_along_its_time() {
    let mut renderer = omaspec_render::Renderer::new(&crate::commands::render_config());
    for (head, run) in [
        ("gitGraph", omaspec_uml::map::Run::Right),
        ("gitGraph TB:", omaspec_uml::map::Run::Down),
        ("gitGraph BT:", omaspec_uml::map::Run::Up),
    ] {
        let source = format!(
            "{head}\n  commit id: \"a\"\n  branch dev\n  commit id: \"b\"\n  checkout main\n  merge dev\n"
        );
        let map = renderer
            .render_at(&source, Language::Mermaid, None)
            .unwrap()
            .map;
        assert_eq!(map.run, run, "{head}");
        let lines: Vec<usize> = map.messages.iter().map(|step| step.line).collect();
        assert_eq!(lines, [1, 3, 5], "{head}");
        // The merge's light comes from the lane of what it took in.
        let merge = &map.messages[2];
        let along = |p: (f32, f32)| {
            if run == omaspec_uml::map::Run::Right {
                p.1
            } else {
                p.0
            }
        };
        let first = along(merge.points[0]);
        let last = along(*merge.points.last().unwrap());
        assert!((first - last).abs() > 10.0, "{head}: {:?}", merge.points);
        let t0 = Instant::now();
        let mut play = Play::new(&source, Language::Mermaid, &map, t0).unwrap();
        assert_eq!(
            play.offer(),
            Offer::Scenario {
                at: 0,
                of: 3,
                commits: true
            }
        );
        let mut edges = Vec::new();
        for _ in 0..=3 {
            let Play::Scenario(scenario) = &play else {
                panic!("a scenario");
            };
            edges.push(scenario.target());
            play.ask(Ask::Next, t0);
        }
        let onward = edges.windows(2).all(|pair| match run {
            omaspec_uml::map::Run::Up => pair[1] < pair[0],
            _ => pair[1] > pair[0],
        });
        assert!(onward, "{head}: {edges:?}");
        assert!(play.focus().is_some(), "{head}");
        // Going back to the right or up, the veil keeps clear of what
        // stands over the commits.
        let Play::Scenario(scenario) = &play else {
            panic!("a scenario");
        };
        let highest = map
            .messages
            .iter()
            .flat_map(|step| step.points.iter().map(|p| p.1))
            .fold(f32::INFINITY, f32::min);
        match run {
            omaspec_uml::map::Run::Down => assert_eq!(scenario.clear, None),
            _ => assert!(scenario.clear.is_some_and(|at| at < highest), "{head}"),
        }
    }
}

/// A trace followed while it is written: what is written by now stands at
/// once, each object a pill beside its state; a line half written waits
/// for its end; an object whose last line deviates stands in red; a file
/// written anew is followed from its start (docs/adr/0036).
#[test]
fn a_trace_followed_live_moves_as_it_is_written() {
    use std::io::Write;
    let source =
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let dir = std::env::temp_dir().join(format!("omaspec-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("run.jsonl");
    let line = |object: &str, event: &str| {
        format!("{{\"object\":\"{object}\",\"machine\":\"door\",\"event\":\"{event}\"}}\n")
    };
    std::fs::write(&path, line("front", "open")).unwrap();
    let append = |text: &str| {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    };
    let t0 = Instant::now();
    let mut play = Play::live(
        source,
        Language::PlantUml,
        &map,
        "door.puml",
        path.clone(),
        t0,
    )
    .unwrap();
    let standing = |play: &Play| -> Vec<(String, Vec<(String, bool)>)> {
        let Play::Machine(machine) = play else {
            panic!("a machine");
        };
        let Driver::Live(live) = &machine.driver else {
            panic!("live");
        };
        live.standing()
            .into_iter()
            .map(|(state, names)| {
                (
                    machine.machine.states[state].name.clone(),
                    names.into_iter().map(|one| (one.name, one.wrong)).collect(),
                )
            })
            .collect()
    };
    let at = |state: &str, names: &[(&str, bool)]| {
        (
            state.to_owned(),
            names
                .iter()
                .map(|(n, w)| ((*n).to_owned(), *w))
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(play.following(), Some(path.as_path()));
    assert_eq!(standing(&play), [at("Open", &[("front", false)])]);
    // A second object, and a line half written that waits for its end.
    append(&line("back", "open"));
    append("{\"object\":\"front\",");
    assert!(play.follow_more(t0));
    assert_eq!(
        standing(&play),
        [at("Open", &[("front", false), ("back", false)])]
    );
    append("\"machine\":\"door\",\"event\":\"open\"}\n");
    assert!(play.follow_more(t0));
    // The front door cannot open again: it stands in red where it was.
    assert_eq!(
        standing(&play),
        [at("Open", &[("front", true), ("back", false)])]
    );
    assert!(!play.follow_more(t0), "nothing new");
    let Offer::Trace {
        live,
        object,
        at: lit,
        finding,
        played,
        ..
    } = play.offer()
    else {
        panic!("a trace");
    };
    assert!(live);
    assert_eq!(object, "live · 2 objects · 1 deviates");
    assert_eq!(played, 3);
    assert_eq!(lit, "Open");
    assert_eq!(finding.map(|(s, _)| s), Some(Severity::Error));
    // Written anew, shorter: followed from its start.
    std::fs::write(
        &path,
        format!("{}{}", line("back", "open"), line("back", "close")),
    )
    .unwrap();
    assert!(play.follow_more(t0));
    assert_eq!(standing(&play), [at("Closed", &[("back", false)])]);
    // A letter written across two reads stays whole.
    let whole = line("Prüfung", "open");
    let cut = whole.find('ü').unwrap() + 1;
    let write = |bytes: &[u8]| {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(bytes).unwrap();
    };
    write(&whole.as_bytes()[..cut]);
    let _ = play.follow_more(t0);
    write(&whole.as_bytes()[cut..]);
    assert!(play.follow_more(t0));
    assert_eq!(
        standing(&play),
        [
            at("Closed", &[("back", false)]),
            at("Open", &[("Prüfung", false)])
        ]
    );
    // Another file put in its place by name, longer than this one: from
    // its start too.
    let other = dir.join("next.jsonl");
    let longer: String = ["open", "close", "open", "close", "open"]
        .iter()
        .map(|event| line("side", event))
        .collect();
    std::fs::write(&other, longer).unwrap();
    std::fs::rename(&other, &path).unwrap();
    assert!(play.follow_more(t0));
    assert_eq!(standing(&play), [at("Open", &[("side", false)])]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An object where a person decides (`<<decide>>`) waits for them: it is
/// counted in the head, and its pills are drawn filled (docs/adr/0036).
#[test]
fn an_object_where_a_person_decides_waits_for_them() {
    let source = "@startuml\n[*] --> Coding\nCoding --> Review : pushed\nReview --> Coding : changes asked\nstate Review <<decide>>\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let dir = std::env::temp_dir().join(format!("omaspec-decide-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("run.jsonl");
    std::fs::write(
        &path,
        "{\"object\":\"pr-42\",\"machine\":\"flow\",\"event\":\"pushed\"}\n{\"object\":\"pr-43\",\"machine\":\"flow\",\"state\":\"Coding\"}\n",
    )
    .unwrap();
    let play = Play::live(
        source,
        Language::PlantUml,
        &map,
        "flow.puml",
        path,
        Instant::now(),
    )
    .unwrap();
    let Play::Machine(machine) = &play else {
        panic!("a machine");
    };
    let Driver::Live(live) = &machine.driver else {
        panic!("live");
    };
    assert_eq!(live.waiting(), 1);
    let review = machine
        .machine
        .states
        .iter()
        .position(|s| s.name == "Review")
        .unwrap();
    assert!(machine.machine.states[review].decide);
    let Offer::Trace { object, .. } = play.offer() else {
        panic!("a trace");
    };
    assert_eq!(object, "live · 2 objects · 1 waits");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Inside a state where a person decides, an object waits too; one whose
/// last line deviates there does not, and stands in red.
#[test]
fn an_object_waits_inside_where_a_person_decides_unless_it_deviates() {
    let source = "@startuml\n[*] --> Coding\nCoding --> Review : pushed\nstate Review <<decide>> {\n  [*] --> Reading\n  Reading --> Asking : unclear\n}\nReview --> Coding : changes asked\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let dir = std::env::temp_dir().join(format!("omaspec-decide-in-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("run.jsonl");
    let line = |object: &str, event: &str| {
        format!("{{\"object\":\"{object}\",\"machine\":\"flow\",\"event\":\"{event}\"}}\n")
    };
    std::fs::write(
        &path,
        [
            line("pr-1", "pushed"),
            line("pr-2", "pushed"),
            line("pr-2", "pushed"),
        ]
        .concat(),
    )
    .unwrap();
    let play = Play::live(
        source,
        Language::PlantUml,
        &map,
        "flow.puml",
        path,
        Instant::now(),
    )
    .unwrap();
    let Play::Machine(machine) = &play else {
        panic!("a machine");
    };
    let Driver::Live(live) = &machine.driver else {
        panic!("live");
    };
    let standing: Vec<(String, bool, bool)> = live
        .standing()
        .into_iter()
        .flat_map(|(_, names)| names)
        .map(|one| (one.name, one.wrong, one.waits))
        .collect();
    assert_eq!(
        standing,
        [
            ("pr-1".to_owned(), false, true),
            ("pr-2".to_owned(), true, false)
        ]
    );
    assert_eq!(live.waiting(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A note on a state comes through the veil with it (REQ-129): the play
/// finds each note in the picture by the part of its state, one inside a
/// composite state with that one too; a note of its own stays under it.
#[test]
fn a_note_comes_through_with_its_state() {
    let source = "@startuml\n[*] --> New\nNew --> Paid : pay\nstate Paid {\n  [*] --> Held\n}\nPaid --> Shipped : ship\nnote right of Paid : held until it leaves\nnote left of New\n  waits a week\nend note\nnote left of New : at most\nnote \"of its own\" as N1\n@enduml\n";
    let picture = crate::paper::testing::drawn(source, Language::PlantUml);
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &picture.map, t0).unwrap();
    play.shape(&picture.tree);
    let Play::Machine(machine) = &play else {
        panic!("no machine");
    };
    let named = |name: &str| {
        machine
            .machine
            .states
            .iter()
            .position(|state| state.name == name)
            .unwrap()
    };
    let mut on: Vec<usize> = machine.notes.iter().map(|(state, _)| *state).collect();
    on.sort_unstable();
    let mut want = vec![named("New"), named("New"), named("Paid")];
    want.sort_unstable();
    assert_eq!(on, want);
    // Paid's note in its own shape, right of its state.
    let paid = machine.placed.states[named("Paid")].as_ref().unwrap().rect;
    let (_, note) = machine
        .notes
        .iter()
        .find(|(state, _)| *state == named("Paid"))
        .unwrap();
    let bounds = note.bounds();
    assert!(bounds.x >= paid.x + paid.w, "{bounds:?} {paid:?}");
    // Lit with Paid, or with Held inside it; not with New.
    let mut levels = vec![0.0; machine.machine.states.len()];
    levels[named("Held")] = 1.0;
    let lit = machine.note_levels(&levels);
    assert_eq!(
        lit.iter()
            .map(|&(note, level)| (machine.notes[note].0, level))
            .collect::<Vec<_>>(),
        [(named("Paid"), 1.0)]
    );
}

/// Once a scenario's last message is shown and its light has come, the
/// veil goes: the lifelines' feet under it are not still to come, and a
/// head bright over its veiled foot read as two colors (REQ-139). A step
/// back from the end brings it back, faded in.
#[test]
fn the_veil_goes_once_the_last_message_is_shown() {
    let source = "@startuml\nactor Uncle\nparticipant Dev\nUncle -> Dev : hello\nDev -> Uncle : hi\n@enduml\n";
    let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
    let t0 = Instant::now();
    let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
    let standing = |play: &Play, now: Instant| match play {
        Play::Scenario(scenario) => scenario.standing(now),
        _ => panic!("a scenario"),
    };
    let later = |at: Instant| at + Duration::from_secs(10);
    assert!(standing(&play, later(t0)) > 0.99);
    play.ask(Ask::Next, t0);
    assert!(standing(&play, later(t0)) > 0.99, "a message is to come");
    let t1 = later(t0);
    play.ask(Ask::Next, t1);
    // While the last message's light runs, the veil stands; then it goes.
    assert!(standing(&play, t1 + Duration::from_millis(1)) > 0.99);
    assert!(standing(&play, later(t1)) < 0.01);
    let t2 = later(t1);
    play.ask(Ask::Back, t2);
    assert!(standing(&play, t2) < 0.01, "back, it fades in");
    assert!(standing(&play, later(t2)) > 0.99);
}
