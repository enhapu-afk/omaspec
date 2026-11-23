//! Lockstep: a scenario played beside the state machines of its
//! participants (`omaspec play SCENARIO --lockstep -o lockstep.gif`).
//! The scenario's pane runs a light along each message, and as it reaches
//! the receiver, that participant's machine takes its step in a pane of
//! its own: the transition the message fires lights up, and where the
//! machine cannot take it, the message runs red and the states flash.
//! What no drawing of either shows alone: that a scenario and the
//! behavior agree.
//!
//! The way through each machine is the scenario's own check played out
//! ([`omaspec_model::lockstep`]), so the play is red exactly where the
//! check reports. The panes are the editor's own, each playing as a
//! recorded trace plays, drawn headless together as one picture.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use iced::{Rectangle, Size};
use omaspec_core::Language;
use omaspec_model::Lockstep;

mod page;

use super::films::Film;
use super::moving::{BAR, FADE_IN_FRAMES, HOLD, Moving, SETTLE, shown, theme};
use super::screen::{Screen, Shows};
use super::{App, Doc, Message, Start};
use crate::paper::{self, Ask};

/// Around the panes and between them.
const GAP: f32 = 14.0;
/// The picture's width where none is asked for: the scenario and the
/// machines beside it each about as large as they are drawn.
const NARROWEST: f32 = 1200.0;
const WIDEST: f32 = 1800.0;
/// How high the picture may grow, by its width.
const LEAST_HEIGHT: f32 = 0.42;
const MOST_HEIGHT: f32 = 0.9;
/// How high a machine's picture is at least, beside its bar: many
/// machines make the picture higher than [`MOST_HEIGHT`] rather than
/// leave one out of it.
const LEAST_PICTURE: f32 = 48.0;
/// The second line a machine's bar may take for what went wrong.
const NOTE: f32 = 18.0;
/// How long the end stays before the picture starts again.
const END: Duration = Duration::from_millis(1_800);

/// Writes the lockstep of `args.file`; what it played.
pub(super) fn lockstep(args: &Moving, film: Film, fps: u32) -> Result<String> {
    let text = args.read()?;
    let (_, language) = args.named(&text);
    let root = crate::specs::root_of(Some(&args.file), args.root.as_deref())
        .context("the scenario lies in no folder")?;
    let (specs, _) = crate::specs::read_project(&root);
    let Some(step) = omaspec_model::lockstep(&text, language, &specs) else {
        bail!(
            "{} is no sequence diagram whose participants have state machines in its project ({}): a participant goes by the machine named like it",
            args.file.display(),
            root.display()
        );
    };
    let machines = machines(&step, &root)?;
    if film == Film::Page {
        return page::write(args, &text, language, &step, &machines);
    }
    let _clock = crate::clock::Recording::start();
    let mut stage = Stage::new(args, film, fps, &text, language, &machines)?;
    let told = stage.play(args, &step)?;
    super::films::timed(
        &mut stage.frames,
        args.timing,
        film,
        fps,
        super::gifs::Rest::Last,
    )?;
    let Some((w, h)) = stage.frames.size() else {
        bail!("no frame was drawn");
    };
    let (bytes, count) = match film {
        Film::Gif => stage.frames.encode(args.quality)?,
        Film::Apng => super::films::apng(&stage.frames, fps)?,
        Film::Webp => super::films::webp(&stage.frames, fps)?,
        Film::Page => unreachable!("a page is written before a frame is drawn"),
    };
    crate::files::write_atomic(&args.output, &bytes)
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(format!(
        "{} → {} ({} frames, {}×{}, {})",
        args.file.display(),
        args.output.display(),
        count,
        w,
        h,
        told
    ))
}

/// The source of each player's machine, its file and language.
fn machines(step: &Lockstep, root: &Path) -> Result<Vec<(PathBuf, String, Language)>> {
    step.players
        .iter()
        .map(|player| {
            let file = root.join(&player.spec.file);
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("cannot read {}", file.display()))?;
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let language = omaspec_core::detect(&text, Language::from_file_name(&name));
            anyhow::ensure!(
                omaspec_model::Machine::read(&text, language).is_some(),
                "the state machine of {} is in {}, among other diagrams: a lockstep draws a machine from a file of its own",
                player.name,
                file.display()
            );
            Ok((file, text, language))
        })
        .collect()
}

struct Stage {
    app: App,
    screen: Screen,
    frames: super::gifs::Frames,
    delay: u16,
    tick: Duration,
    /// The tabs: the scenario's, and each player's machine's in the order
    /// of [`Lockstep::players`].
    scenario: u64,
    machines: Vec<u64>,
}

impl Stage {
    fn new(
        args: &Moving,
        film: Film,
        fps: u32,
        text: &str,
        language: Language,
        machines: &[(PathBuf, String, Language)],
    ) -> Result<Self> {
        let start = Start {
            files: vec![args.file.clone()],
            folder: None,
            forced: args.forced,
        };
        let (mut app, _) = App::themed(start, None, theme(args.theme.as_deref())?);
        app.follow_theme = !args.file_colors;
        let mut screen = match args.renderer {
            Some(renderer) => Screen::drawn_by(&app, 1.0, Some(renderer)),
            None => Screen::new(&app, 1.0),
        }
        .map_err(|why| anyhow::anyhow!(why))?;
        let first = app.doc().key;
        // A tab for each machine, though two participants share one.
        let mut keys = vec![first];
        for _ in machines {
            let key = app.take_key();
            app.docs.push(Doc::untitled(key, app.forced, &app.look));
            keys.push(key);
        }
        let sources = std::iter::once((args.file.as_path(), text, language)).chain(
            machines
                .iter()
                .map(|(f, t, l)| (f.as_path(), t.as_str(), *l)),
        );
        let mut sizes = Vec::new();
        for (index, (file, text, language)) in sources.enumerate() {
            app.active = index;
            let key = keys[index];
            let scaled = app.doc_mut().paper.set_scale_factor(1.0);
            screen.run_task(&mut app, scaled.map(move |m| Message::Paper(key, m)));
            shown(
                &mut app,
                &screen,
                key,
                text,
                language,
                file,
                args.root.as_deref(),
            )?;
            sizes.push(app.doc().paper.diagram_size());
        }
        let width = match args.width {
            Some(width) => width as f32,
            None => natural(&sizes),
        };
        let extra =
            |text: &str, language, width| BAR + paper::play_bar_extra(text, language, width);
        // A machine beside the scenario has a quiet bar: where it is and
        // what went wrong.
        let (size, scenario, placed) = layout(width, &sizes, |index, width| match index {
            0 => extra(text, language, width),
            _ => BAR + NOTE,
        });
        for (index, &key) in keys.iter().enumerate() {
            app.active = index;
            app.doc_mut().paper.view.fit = true;
            let at = if index == 0 {
                scenario
            } else {
                placed[index - 1]
            };
            screen.perform(
                &mut app,
                Message::Paper(key, paper::Message::Viewport(at.size())),
            );
        }
        app.active = 0;
        screen.size = size;
        screen.shows = Shows::Panes;
        screen.panes = std::iter::once((first, scenario))
            .chain(keys[1..].iter().copied().zip(placed.iter().copied()))
            .collect();
        let tick = Duration::from_micros(1_000_000 / u64::from(fps));
        let mut stage = Stage {
            app,
            screen,
            frames: super::gifs::Frames::default(),
            delay: match film {
                Film::Gif => u16::try_from((100 / fps).max(2)).unwrap_or(5),
                Film::Apng | Film::Webp | Film::Page => 1,
            },
            tick,
            scenario: first,
            machines: keys[1..].to_vec(),
        };
        // The pictures stand from the first frame kept.
        stage.screen.run_frames(
            &mut stage.app,
            FADE_IN_FRAMES,
            tick,
            |app| {
                keys.iter()
                    .all(|&key| doc(app, key).is_some_and(|d| d.paper.fading().is_none()))
            },
            |_| {},
        );
        Ok(stage)
    }

    /// Plays the scenario, each machine on the beat of the message it
    /// receives; what it played.
    fn play(&mut self, args: &Moving, step: &Lockstep) -> Result<String> {
        let name = args
            .file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let scenario = self.scenario;
        let started = self.with(scenario, |paper| paper.start_play());
        if let Err(why) = started {
            bail!("the scenario cannot be played: {why}");
        }
        // The messages no machine takes run red.
        let wrong: Vec<usize> = step
            .players
            .iter()
            .flat_map(|p| &p.followed.steps)
            .filter(|s| s.finding.is_some())
            .map(|s| s.line)
            .collect();
        self.with(scenario, |paper| paper.mark_wrong(wrong.clone()));
        for (player, &key) in step.players.iter().zip(&self.machines.clone()) {
            let (machine, followed) = (player.spec.machine.clone(), player.followed.clone());
            let started = self.with(key, |paper| {
                paper.start_followed(machine, followed, &name, &player.name)
            });
            if let Err(why) = started {
                bail!(
                    "the state machine of {} cannot be played: {why}",
                    player.name
                );
            }
        }
        self.settle();
        // Each step of the scenario's play, a note's too, on which no
        // machine moves.
        let lines = self.step_lines();
        let messages = self.message_lines().len();
        // How far each player has come along its way.
        let mut played = vec![0usize; step.players.len()];
        for &line in &lines {
            // A group sends players back, or on, before the message.
            if self.advance(step, &mut played, |at| at < line) {
                self.settle();
            }
            self.send(scenario, Ask::Next);
            // The machine moves as the message's light reaches its receiver.
            let arrives = doc(&self.app, scenario).and_then(|d| d.paper.play_arrives());
            if let Some(arrives) = arrives {
                self.run(SETTLE, move |_| crate::clock::now() >= arrives);
            }
            self.advance(step, &mut played, |at| at == line);
            self.settle();
        }
        // What the groups at the end do.
        if self.advance(step, &mut played, |_| true) {
            self.settle();
        }
        self.run(END, |_| false);
        let wrong = wrong.len();
        Ok(match wrong {
            0 => format!("{messages} messages, every one taken"),
            1 => format!("{messages} messages, 1 not taken"),
            n => format!("{messages} messages, {n} not taken"),
        })
    }

    /// Steps each player whose next step's line `due` says; whether any
    /// moved.
    fn advance(
        &mut self,
        step: &Lockstep,
        played: &mut [usize],
        due: impl Fn(usize) -> bool,
    ) -> bool {
        let mut moved = false;
        for (index, player) in step.players.iter().enumerate() {
            let key = self.machines[index];
            while let Some(next) = player.followed.steps.get(played[index])
                && due(next.line)
            {
                played[index] += 1;
                self.send(key, Ask::Next);
                moved = true;
            }
        }
        moved
    }

    /// The source lines of the scenario's messages, in order.
    fn message_lines(&self) -> Vec<usize> {
        doc(&self.app, self.scenario).map_or_else(Vec::new, |d| d.paper.message_lines())
    }

    /// The source lines of the scenario's steps, its messages and its
    /// notes, in the order its play takes them.
    fn step_lines(&self) -> Vec<usize> {
        doc(&self.app, self.scenario).map_or_else(Vec::new, |d| d.paper.step_lines())
    }

    /// Runs `f` on the pane of tab `key`.
    fn with<T>(&mut self, key: u64, f: impl FnOnce(&mut crate::paper::Paper) -> T) -> T {
        let doc = self
            .app
            .docs
            .iter_mut()
            .find(|d| d.key == key)
            .expect("a tab of the stage");
        f(&mut doc.paper)
    }

    fn send(&mut self, key: u64, ask: Ask) {
        self.screen.perform(
            &mut self.app,
            Message::Paper(key, paper::Message::Play(ask)),
        );
    }

    /// Draws frames for `most`, or until `done`.
    fn run(&mut self, most: Duration, done: impl Fn(&App) -> bool) {
        let count = (most.as_secs_f32() / self.tick.as_secs_f32()).ceil() as usize;
        let (frames, delay) = (&mut self.frames, self.delay);
        self.screen
            .run_frames(&mut self.app, count.max(1), self.tick, done, |shot| {
                frames.push(shot.width, shot.height, &shot.rgba, delay);
            });
    }

    /// Every pane's motion to its end, and a moment on what it reached.
    fn settle(&mut self) {
        let keys: Vec<u64> = std::iter::once(self.scenario)
            .chain(self.machines.iter().copied())
            .collect();
        self.run(SETTLE, move |app| {
            let now = crate::clock::now();
            keys.iter()
                .all(|&key| doc(app, key).is_none_or(|d| !d.paper.play_moving(now)))
        });
        self.run(HOLD, |_| false);
    }
}

fn doc(app: &App, key: u64) -> Option<&Doc> {
    app.docs.iter().find(|d| d.key == key)
}

/// A width at which the scenario and the machines beside it are each
/// about as large as they are drawn.
fn natural(sizes: &[Size]) -> f32 {
    let scenario = sizes[0].width;
    let machines = sizes[1..].iter().map(|s| s.width).fold(0.0, f32::max);
    ((scenario + machines) * 1.25 + 3.0 * GAP)
        .clamp(NARROWEST, WIDEST)
        .round()
}

/// The picture's size, the scenario's pane and each machine's, for a
/// picture `width` wide: the scenario on the left, the machines stacked
/// on the right, both columns about as high. The machines are drawn at
/// one scale, as large as the column lets them; `bar` is how high the
/// bar of pane `index` is at a width, which it gets on top.
fn layout(
    width: f32,
    sizes: &[Size],
    bar: impl Fn(usize, f32) -> f32,
) -> (Size, Rectangle, Vec<Rectangle>) {
    let ratio = |s: &Size| s.height / s.width.max(1.0);
    let machines = &sizes[1..];
    let count = machines.len() as f32;
    let inner = width - 3.0 * GAP;
    // The scenario's width w where both columns are as high: a·w and a
    // bar on the left, b·(inner − w) and a bar for each machine with the
    // gaps between them on the right.
    let a = ratio(&sizes[0]);
    let b: f32 = machines.iter().map(ratio).sum();
    let left =
        ((inner * b + (count - 1.0) * (BAR + GAP)) / (a + b)).clamp(inner * 0.32, inner * 0.5);
    let right = inner - left;
    let bars: Vec<f32> = (1..sizes.len()).map(|i| bar(i, right)).collect();
    let fixed = bars.iter().sum::<f32>() + (count - 1.0) * GAP;
    let tall: f32 = machines.iter().map(|s| s.height).sum();
    let wide = machines.iter().map(|s| s.width).fold(1.0, f32::max);
    // As large as the widest fills the column.
    let scale = right / wide;
    let column = tall * scale + fixed;
    let own = left * a + bar(0, left);
    let height = (own.max(column) + 2.0 * GAP)
        .clamp(width * LEAST_HEIGHT, width * MOST_HEIGHT)
        .max(fixed + count * LEAST_PICTURE + 2.0 * GAP)
        .round();
    let inside = height - 2.0 * GAP;
    let scenario = Rectangle::new(iced::Point::new(GAP, GAP), Size::new(left, inside));
    // Smaller where the column is not high enough; what is left over
    // shared out.
    let scale = scale.min(((inside - fixed) / tall.max(1.0)).max(0.0));
    let spare = ((inside - fixed - tall * scale) / count).max(0.0);
    let mut y = GAP;
    let placed = machines
        .iter()
        .zip(&bars)
        .map(|(s, bar)| {
            let h = (s.height * scale + bar + spare).floor();
            let at = Rectangle::new(iced::Point::new(2.0 * GAP + left, y), Size::new(right, h));
            y += h + GAP;
            at
        })
        .collect();
    (Size::new(width, height), scenario, placed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moving(file: &Path, output: PathBuf) -> Moving {
        Moving {
            file: file.to_owned(),
            text: None,
            output,
            events: Vec::new(),
            steps: Some(100),
            width: Some(400),
            fps: Some(4),
            folds: Vec::new(),
            dives: Vec::new(),
            builds: false,
            lockstep: true,
            history: false,
            travel: None,
            traffic: Vec::new(),
            themes: None,
            ink: false,
            look: None,
            timing: Default::default(),
            whole: false,
            theme: Some("built-in".into()),
            file_colors: false,
            root: None,
            forced: None,
            renderer: Some("tiny-skia"),
            quality: super::super::gifs::Quality::default(),
        }
    }

    /// A scenario of a project played beside the machines of its
    /// participants, each one in a pane: the message no machine takes is
    /// counted; a scenario without such participants is refused.
    #[test]
    fn a_scenario_plays_beside_the_machines_of_its_participants() {
        let work = std::env::temp_dir().join(format!("omaspec-lockstep-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(
            work.join("door.puml"),
            "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n",
        )
        .unwrap();
        std::fs::write(
            work.join("alarm.puml"),
            "@startuml\ntitle Alarm\n[*] --> Off\nOff --> Armed : arm\nArmed --> Off : disarm\n@enduml\n",
        )
        .unwrap();
        let keys = work.join("keys.puml");
        std::fs::write(
            &keys,
            "@startuml\nactor Guest\nGuest -> Door : lock\nGuest -> Alarm : arm\nGuest -> Door : open\n@enduml\n",
        )
        .unwrap();
        let told = super::super::moving::play(&moving(&keys, work.join("keys.gif"))).unwrap();
        assert!(told.contains("3 messages, 1 not taken"), "{told}");
        let gif = std::fs::read(work.join("keys.gif")).unwrap();
        assert_eq!(u16::from_le_bytes([gif[6], gif[7]]), 400);
        // As a page in vectors: each picture, and when the door flashes.
        let told = super::super::moving::play(&moving(&keys, work.join("keys.html"))).unwrap();
        assert!(
            told.contains("a page") && told.contains("1 not taken"),
            "{told}"
        );
        let page = std::fs::read_to_string(work.join("keys.html")).unwrap();
        assert!(page.contains(r#""kind":"lockstep""#));
        assert_eq!(page.matches(r#"<section class="pane machine""#).count(), 2);
        assert!(page.contains(r#""wrong":true"#));
        assert!(page.contains("Door does not take open here"));

        let alone = work.join("alone.puml");
        std::fs::write(&alone, "@startuml\nA -> B : hi\n@enduml\n").unwrap();
        let refused = super::super::moving::play(&moving(&alone, work.join("alone.gif")))
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("no sequence diagram whose participants"),
            "{refused}"
        );
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn the_columns_share_the_width_and_the_machines_the_height() {
        let sizes = [
            Size::new(500.0, 600.0),
            Size::new(400.0, 200.0),
            Size::new(400.0, 300.0),
        ];
        let (size, scenario, machines) = layout(1400.0, &sizes, |_, _| 90.0);
        assert_eq!(size.width, 1400.0);
        assert!(
            scenario.width > 300.0 && scenario.width < 900.0,
            "{scenario:?}"
        );
        assert_eq!(machines.len(), 2);
        // Stacked in one column beside the scenario, inside the picture.
        assert_eq!(machines[0].x, machines[1].x);
        assert!(machines[0].x > scenario.x + scenario.width);
        assert!(machines[1].y > machines[0].y + machines[0].height);
        assert!(machines[1].y + machines[1].height <= size.height - GAP + 0.5);
        // One scale: the taller machine gets more of the column, by the
        // height it is drawn, the bars the same.
        assert!(machines[1].height > machines[0].height);
        let drawn = |m: &Rectangle| m.height - 90.0;
        assert!((drawn(&machines[1]) / drawn(&machines[0]) - 1.5).abs() < 0.2);

        // Many machines in a narrow picture: each keeps a picture beside
        // its bar, and the last stays inside.
        let many = [Size::new(500.0, 600.0)]
            .into_iter()
            .chain(std::iter::repeat_n(Size::new(400.0, 200.0), 5))
            .collect::<Vec<_>>();
        let (size, _, machines) = layout(400.0, &many, |_, _| 108.0);
        let last = machines.last().unwrap();
        assert!(
            last.y + last.height <= size.height - GAP + 0.5,
            "{size:?} {last:?}"
        );
        assert!(
            machines
                .iter()
                .all(|m| m.height >= 108.0 + LEAST_PICTURE - 1.0)
        );
    }
}
