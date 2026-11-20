//! The landing page's film (`docs/images/film.sh`), drawn when
//! OMASPEC_FILM_DIR is set: the editor in a window of 1280×720 at a scale
//! of 1.5, every frame on the GPU at 60 a second on a recording's clock
//! ([`crate::clock`]), so each motion is in it whole however long a frame
//! takes to draw. The hand is a real one: the pointer, Alt, Ctrl, the
//! button and the touchpad go through the window's layers as in a
//! window, and the pointer is painted over the frames. Each scene is a
//! folder of numbered frames: `1-type`, `2-hand`, `3-play`, `4-pills`,
//! `5-export`.

use iced::keyboard::Modifiers;
use iced::mouse::{self, Button, ScrollDelta};
use iced::{Point, Vector};

use super::*;
use crate::editor::moving;

const FPS: u32 = 60;
const WINDOW: Size = Size::new(1280.0, 720.0);
const SCALE: f32 = 1.5;

/// The camera: the screen, the scene being shot and the hand drawn over
/// its frames.
struct Reel {
    screen: Screen,
    out: PathBuf,
    scene: PathBuf,
    frame: usize,
    /// Where the pointer is drawn, in the window's units; not while
    /// `None`.
    hand: Option<Point>,
    /// The button is down: a ring round the pointer's tip.
    pressed: bool,
}

impl Reel {
    fn tick() -> Duration {
        Duration::from_nanos(1_000_000_000 / u64::from(FPS))
    }

    fn frames(seconds: f32) -> usize {
        (seconds * FPS as f32).round() as usize
    }

    fn scene(&mut self, name: &str) {
        self.scene = self.out.join(name);
        let _ = std::fs::remove_dir_all(&self.scene);
        std::fs::create_dir_all(&self.scene).unwrap();
        self.frame = 0;
    }

    /// One frame, kept: a frame of the window's clock, as its
    /// subscription sends one while something moves.
    fn shoot(&mut self, app: &mut App) {
        self.send(app, Message::Frame);
        let key = app.doc().key;
        self.send(
            app,
            Message::Editor(key, scrive_iced::Event::HighlightSweep),
        );
        let shot = self.screen.run_frame(app, Self::tick());
        let png = painted(&shot, self.hand, self.pressed);
        std::fs::write(self.scene.join(format!("{:05}.png", self.frame)), png).unwrap();
        self.frame += 1;
    }

    /// Frames drawn and not kept: what a scene needs before it starts.
    fn warm(&mut self, app: &mut App, frames: usize) {
        for _ in 0..frames {
            self.send(app, Message::Frame);
            let _ = self.screen.run_frame(app, Self::tick());
        }
    }

    fn hold(&mut self, app: &mut App, seconds: f32) {
        for _ in 0..Self::frames(seconds) {
            self.shoot(app);
        }
    }

    /// Frames until `done`, at most `seconds` of them.
    fn until(&mut self, app: &mut App, seconds: f32, done: impl Fn(&App) -> bool) {
        for _ in 0..Self::frames(seconds) {
            if done(app) {
                return;
            }
            self.shoot(app);
        }
    }

    /// The pointer moved to `to` over `seconds`, eased in and out, from
    /// where it is (or appearing there).
    fn point(&mut self, app: &mut App, to: Point, seconds: f32) {
        let from = self.hand.unwrap_or(to);
        let count = Self::frames(seconds).max(1);
        for i in 1..=count {
            let t = ease(i as f32 / count as f32);
            let at = from + (to - from) * t;
            self.hand = Some(at);
            self.screen.pointer = Some(at);
            self.shoot(app);
        }
    }

    /// Hands `message` to the editor and runs what it gives at once, as
    /// the window runs it: a tab chosen gets its picture back.
    fn send(&mut self, app: &mut App, message: Message) {
        self.screen.perform(app, message);
    }

    fn input(&mut self, event: iced::Event) {
        self.screen.input.push(event);
    }

    fn modifiers(&mut self, modifiers: Modifiers) {
        self.input(iced::Event::Keyboard(
            iced::keyboard::Event::ModifiersChanged(modifiers),
        ));
    }

    fn button(&mut self, down: bool) {
        self.pressed = down;
        self.input(iced::Event::Mouse(if down {
            mouse::Event::ButtonPressed(Button::Left)
        } else {
            mouse::Event::ButtonReleased(Button::Left)
        }));
    }

    /// A click where the pointer is, held a moment.
    fn click(&mut self, app: &mut App) {
        self.button(true);
        self.hold(app, 0.12);
        self.button(false);
    }

    /// The touchpad scrolled by `by` over `seconds`, eased: what each
    /// frame scrolls is the step of the eased way.
    fn scroll(&mut self, app: &mut App, by: Vector, seconds: f32) {
        let count = Self::frames(seconds).max(1);
        let mut done = Vector::new(0.0, 0.0);
        for i in 1..=count {
            let to = by * ease(i as f32 / count as f32);
            let step = to - done;
            done = to;
            self.input(iced::Event::Mouse(mouse::Event::WheelScrolled {
                delta: ScrollDelta::Pixels {
                    x: step.x,
                    y: step.y,
                },
            }));
            self.shoot(app);
        }
    }

    /// Types `text` a key at a time, at a writer's pace.
    fn type_in(&mut self, app: &mut App, text: &str) {
        const PACE: [usize; 7] = [5, 4, 6, 4, 5, 7, 4];
        for (i, c) in text.chars().enumerate() {
            act(
                app,
                match c {
                    '\n' => Action::Enter,
                    c => Action::Type(c),
                },
            );
            for _ in 0..PACE[i % PACE.len()] {
                self.shoot(app);
            }
        }
    }

    /// Where the diagram's pane lies in the window: Alt held with the
    /// pointer at `inside`, the pane says where in it the pointer is.
    fn pane_origin(&mut self, app: &mut App, inside: Point) -> Vector {
        self.screen.pointer = Some(inside);
        self.modifiers(Modifiers::ALT);
        let (_, published) = self.screen.redraw(app);
        let at = published.iter().find_map(|message| match message {
            Message::Paper(_, paper::Message::Hand(paper::Gesture::Hover(Some(at)))) => Some(*at),
            _ => None,
        });
        for message in published {
            self.screen.perform(app, message);
        }
        self.modifiers(Modifiers::empty());
        self.warm(app, 3);
        self.screen.pointer = None;
        let at = at.expect("the pane tells where the pointer is with Alt held");
        inside - at
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The frame as a PNG, the pointer drawn over it: an arrow, white with a
/// dark edge and a soft shadow, and a ring in the accent while the
/// button is down.
fn painted(shot: &Shot, hand: Option<Point>, pressed: bool) -> Vec<u8> {
    use omaspec_render::export::tiny_skia::{
        Color, FillRule, IntSize, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform,
    };
    let size = IntSize::from_wh(shot.width as u32, shot.height as u32).unwrap();
    let mut pixmap = Pixmap::from_vec(shot.rgba.clone(), size).unwrap();
    if let Some(at) = hand {
        let place = Transform::from_row(SCALE, 0.0, 0.0, SCALE, at.x * SCALE, at.y * SCALE);
        let mut paint = Paint {
            anti_alias: true,
            ..Paint::default()
        };
        if pressed {
            let ring = PathBuilder::from_circle(0.0, 0.0, 13.0).unwrap();
            paint.set_color(Color::from_rgba8(0x7a, 0xa2, 0xf7, 0x55));
            pixmap.fill_path(&ring, &paint, FillRule::Winding, place, None);
        }
        let mut arrow = PathBuilder::new();
        arrow.move_to(0.0, 0.0);
        arrow.line_to(0.0, 17.0);
        arrow.line_to(4.2, 13.1);
        arrow.line_to(7.0, 19.4);
        arrow.line_to(9.9, 18.2);
        arrow.line_to(7.1, 12.0);
        arrow.line_to(12.6, 12.0);
        arrow.close();
        let arrow = arrow.finish().unwrap();
        paint.set_color(Color::from_rgba8(0, 0, 0, 0x60));
        let shadow = place.pre_translate(0.8, 1.4);
        pixmap.fill_path(&arrow, &paint, FillRule::Winding, shadow, None);
        paint.set_color(Color::from_rgba8(0xff, 0xff, 0xff, 0xff));
        pixmap.fill_path(&arrow, &paint, FillRule::Winding, place, None);
        paint.set_color(Color::from_rgba8(0x15, 0x16, 0x1e, 0xff));
        let edge = Stroke {
            width: 1.1,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        pixmap.stroke_path(&arrow, &paint, &edge, place, None);
    }
    pixmap.encode_png().unwrap()
}

/// The window's area of the text `label` shows, as a pointer finds it.
fn bounds_of(app: &App, label: &str) -> iced::Rectangle {
    let settings = iced::Settings {
        default_font: app.font,
        fonts: scrive_iced::required_fonts()
            .iter()
            .map(|f| std::borrow::Cow::Borrowed(*f))
            .collect(),
        ..Default::default()
    };
    let mut ui = iced_test::simulator::Simulator::with_size(settings, WINDOW, app.main_view());
    ui.find(label)
        .unwrap_or_else(|_| panic!("{label} is on screen"))
        .visible_bounds()
        .unwrap_or_else(|| panic!("{label} is seen"))
}

/// The active tab's text drawn, as the worker answers a render: in the
/// theme's colors, through the window, so it fades or glides.
fn draw(app: &mut App, reel: &Reel, file: &Path) {
    let key = app.doc().key;
    let (text, language) = (app.doc().text(), app.doc().language);
    moving::shown(app, &reel.screen, key, &text, language, file, None).unwrap();
}

/// Whether the play's bar offers `event`, with a guard or an action or
/// without.
fn offered(app: &App, event: &str) -> bool {
    let Some(paper::Offer::Machine { options, .. }) = app.doc().paper.offer() else {
        return false;
    };
    options
        .iter()
        .any(|(label, _)| label.split(['[', '/']).next().unwrap().trim() == event)
}

#[test]
fn film() {
    let Some(out) = std::env::var_os("OMASPEC_FILM_DIR") else {
        return;
    };
    let out = PathBuf::from(out);
    let work = temp_dir("film").join("specs");
    std::fs::create_dir_all(&work).unwrap();
    let example = |file: &str| {
        crate::gallery::EXAMPLES
            .iter()
            .find(|e| e.file == file)
            .unwrap()
            .text
    };
    // The door without its jam, typed in the first scene.
    let door = without(&without(DOOR, "Jammed"), "repaired");
    for (file, text) in [
        ("door.puml", door.as_str()),
        ("media-player.mmd", example("media-player.mmd")),
        ("checkout.puml", example("checkout.puml")),
        ("leave.puml", LEAVE),
        ("night.jsonl", NIGHT),
    ] {
        std::fs::write(work.join(file), text).unwrap();
    }
    let tabs = ["door.puml", "media-player.mmd", "checkout.puml"];
    let theme = theme_named("tokyo-night", omaspec_theme::Palette::fallback());
    let start = Start {
        files: tabs.iter().map(|f| work.join(f)).collect(),
        folder: Some(work.clone()),
        forced: None,
    };
    let mut app = App::themed(start, None, theme).0;
    let main = app.main;
    let _ = app.update(Message::ScaleFactor(main, SCALE));
    let _ = app.update(Message::WindowSized(main, WINDOW));
    let _ = app.update(Message::Scanned(work.clone(), folder::scan(&work)));
    follow_theme(&mut app, true);
    let _clock = crate::clock::Recording::start();
    let mut screen = Screen::new(&app, SCALE).unwrap();
    screen.size = WINDOW;
    let mut reel = Reel {
        screen,
        out: out.clone(),
        scene: out.clone(),
        frame: 0,
        hand: None,
        pressed: false,
    };
    // Each tab drawn in its pane before the film starts.
    for (i, file) in tabs.iter().enumerate().rev() {
        reel.send(&mut app, Message::Select(i));
        let key = app.doc().key;
        due(&mut app, Alarm::Lint(key));
        reel.warm(&mut app, 3);
        draw(&mut app, &reel, &work.join(file));
        reel.warm(&mut app, 90);
    }
    app.notice = None;

    // 1. Type, and it draws: the door gets its jam, a finding comes and
    // goes.
    reel.scene("1-type");
    app.set_focus(Pane::Source);
    let text = app.doc().text();
    let end = text.find(": unlock").unwrap() + ": unlock".len();
    act(&mut app, Action::PlaceCaret(u32::try_from(end).unwrap()));
    reel.hold(&mut app, 0.8);
    for line in ["\nOpen --> Jammed : stuck", "\nJammed --> Open : repaired"] {
        reel.type_in(&mut app, line);
        // The pause after the last key, and the render it asks for.
        reel.hold(&mut app, 0.4);
        let key = app.doc().key;
        due(&mut app, Alarm::Lint(key));
        draw(&mut app, &reel, &work.join("door.puml"));
        reel.until(&mut app, 2.0, |app| {
            app.doc().paper.glide().is_none() && app.doc().paper.fading().is_none()
        });
        reel.hold(&mut app, 0.9);
    }

    // 2. A line moved by hand with Alt; the layout stays in the file.
    let pane = reel.pane_origin(&mut app, Point::new(1000.0, 360.0));
    reel.scene("2-hand");
    // The unlock line, out to the right.
    let (at, across) = app.doc().paper.screen_of_line("Locked -> Closed");
    let across = if across.x < 0.0 {
        across * -1.0
    } else {
        across
    };
    let at = at + pane;
    let to = at + across * 110.0;
    reel.hand = Some(at + Vector::new(-150.0, 110.0));
    reel.point(&mut app, at, 0.8);
    reel.hold(&mut app, 0.3);
    reel.modifiers(Modifiers::ALT);
    reel.hold(&mut app, 0.6);
    reel.button(true);
    reel.hold(&mut app, 0.15);
    reel.point(&mut app, to, 1.1);
    reel.hold(&mut app, 0.2);
    reel.button(false);
    reel.hold(&mut app, 0.4);
    draw(&mut app, &reel, &work.join("door.puml"));
    reel.hold(&mut app, 0.3);
    reel.modifiers(Modifiers::empty());
    reel.point(&mut app, to + Vector::new(50.0, 80.0), 0.5);
    reel.hold(&mut app, 1.2);
    reel.hand = None;
    reel.screen.pointer = None;

    // 3. F5 plays the machine, a Mermaid one: in its two regions at
    // once.
    reel.send(&mut app, Message::Select(1));
    reel.warm(&mut app, 2);
    reel.scene("3-play");
    app.set_focus(Pane::Diagram);
    reel.hold(&mut app, 0.5);
    let moving = |app: &App| !app.doc().paper.play_moving(crate::clock::now());
    reel.send(&mut app, Message::Command(Command::Play));
    reel.until(&mut app, 2.0, moving);
    reel.hold(&mut app, 0.4);
    // Each option clicked in the play's bar.
    for event in ["power", "play", "plugged in", "buffer full", "pause"] {
        assert!(offered(&app, event), "{event} is offered");
        let pill = bounds_of(&app, event).center();
        if reel.hand.is_none() {
            reel.hand = Some(pill + Vector::new(90.0, 60.0));
        }
        reel.point(&mut app, pill, 0.45);
        reel.click(&mut app);
        reel.until(&mut app, 3.0, moving);
        reel.hold(&mut app, 0.2);
    }
    reel.point(&mut app, Point::new(1180.0, 640.0), 0.5);
    reel.hold(&mut app, 0.3);
    reel.hand = None;
    reel.screen.pointer = None;
    reel.send(&mut app, Message::Command(Command::StopPlay));
    reel.warm(&mut app, 40);

    // 4. A long scenario scrolled: its participants stay on top as
    // pills.
    reel.send(&mut app, Message::Select(2));
    reel.warm(&mut app, 2);
    reel.scene("4-pills");
    let viewport = app.doc().paper.viewport();
    let top = Point::ORIGIN + pane + Vector::new(viewport.width / 2.0, 50.0);
    reel.hand = Some(top + Vector::new(80.0, 200.0));
    reel.screen.pointer = reel.hand;
    reel.point(&mut app, top, 0.6);
    // Ctrl and the touchpad: zoomed in till the diagram is as wide as
    // the pane.
    let wide = 0.96 * viewport.width / app.doc().paper.diagram_size().width;
    let by = 300.0 * (wide / app.doc().paper.view.zoom).ln();
    reel.modifiers(Modifiers::CTRL);
    reel.scroll(&mut app, Vector::new(0.0, by), 0.9);
    reel.modifiers(Modifiers::empty());
    reel.hold(&mut app, 0.4);
    reel.scroll(&mut app, Vector::new(0.0, -900.0), 3.2);
    reel.hold(&mut app, 0.8);
    reel.scroll(&mut app, Vector::new(0.0, 900.0), 1.8);
    reel.hold(&mut app, 0.6);

    // 5. Export as: the ways a diagram leaves the editor. The menu bar
    // comes with the cut, as a tap on Alt brings it: the window moves
    // down under it at once.
    reel.send(&mut app, Message::Command(Command::ToggleMenuBar));
    reel.warm(&mut app, 2);
    reel.scene("5-export");
    reel.hold(&mut app, 0.4);
    let file = bounds_of(&app, "File").center();
    reel.point(&mut app, file, 0.6);
    reel.click(&mut app);
    reel.hold(&mut app, 0.3);
    let export = bounds_of(&app, "Export as");
    reel.point(&mut app, export.center(), 0.6);
    reel.hold(&mut app, 0.3);
    for label in [
        "SVG…",
        "PNG…",
        "JPEG…",
        "PDF…",
        "Markdown…",
        "ZIP…",
        "Presentation…",
        "Animation…",
    ] {
        let row = bounds_of(&app, label);
        reel.point(&mut app, Point::new(row.x + 40.0, row.center_y()), 0.32);
        reel.hold(&mut app, 0.12);
    }
    let row = bounds_of(&app, "Presentation…");
    reel.point(&mut app, Point::new(row.x + 40.0, row.center_y()), 0.4);
    reel.hold(&mut app, 1.2);
    reel.send(&mut app, Message::Command(Command::Cancel));
    reel.point(&mut app, Point::new(1040.0, 420.0), 0.6);
    reel.hold(&mut app, 0.6);

    let _ = std::fs::remove_dir_all(work.parent().unwrap());
}
