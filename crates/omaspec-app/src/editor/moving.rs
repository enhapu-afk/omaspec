//! A play as a picture that moves: `omaspec play FILE -o play.gif`
//! draws the diagram's pane headless, as the window draws it, runs the
//! play on it and writes the frames as a GIF ([`super::gifs`]), for a
//! presentation or a pull request.
//!
//! The pane is the editor's own ([`super::screen`]), drawn on the GPU
//! without a window (by tiny-skia where there is none), so the light
//! along a transition is the one the window shows. Its motions run on the
//! recording's own clock ([`crate::clock`]): each frame a tick after the
//! one before, however long drawing it took. The events are taken by name (`--events open,close`;
//! with its guard, `"start [tank empty]"`, where two share a name),
//! else the first the machine offers at each state, for `--steps` steps.
//! With `--fold Brewing` no play runs: the container folds to its box,
//! the diagram glides to its new layout, and it unfolds again. With
//! `--dive Orders` on a project's file the map zooms into that tile, the
//! diagram stands in its own tab, and the way leads back out.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use iced::Size;
use omaspec_core::{Language, detect};
use omaspec_render::{Place, Renderer};
use omaspec_theme::Theme as Omarchy;

use super::screen::{Screen, Shot, Shows};
use super::{App, Command, Message, Start};
use crate::paper::{self, Ask, headless};
use crate::worker::{self, Drawn};

/// What the command line asks for.
#[derive(Debug, Clone)]
pub struct Moving {
    pub file: PathBuf,
    pub output: PathBuf,
    /// The events to take, in order; empty takes the first offered.
    pub events: Vec<String>,
    /// How many steps at most.
    pub steps: usize,
    /// The picture's width in pixels, the height following the diagram;
    /// `None` takes twice the diagram's size, up to [`WIDEST`].
    pub width: Option<u32>,
    pub fps: u32,
    /// The containers to fold and unfold instead of a play.
    pub folds: Vec<String>,
    /// The tiles of a project's map to zoom into and back out of.
    pub dives: Vec<String>,
    /// The colors: `None` the active theme's, `built-in` the palette
    /// Omaspec ships, else a theme's `colors.toml`.
    pub theme: Option<String>,
    pub forced: Option<Language>,
    /// The renderer that draws (`tiny-skia`, without a GPU); `None` takes
    /// `ICED_TEST_BACKEND`, else the first that can.
    pub renderer: Option<&'static str>,
}

/// How long the play holds after each step, and at the end.
const HOLD: Duration = Duration::from_millis(500);
/// How long a picture takes to reach the GPU, at most.
const UPLOAD: Duration = Duration::from_millis(300);
/// How long a zoom into a tile may take, or out of it.
const DIVE: Duration = Duration::from_millis(4_000);
/// How long a step may take to settle before the next.
const SETTLE: Duration = Duration::from_millis(2_500);
/// The pane's height is between these shares of its width.
const LEAST_HEIGHT: f32 = 0.3;
const MOST_HEIGHT: f32 = 1.4;
/// Around the diagram, and below it for the bar of the play.
const MARGIN: f32 = 40.0;
/// How wide a picture is at most when no width is asked for.
const WIDEST: f32 = 1200.0;
const BAR: f32 = 90.0;

/// Writes the play, or the fold; says what it wrote.
pub fn play(args: &Moving) -> Result<String> {
    anyhow::ensure!(args.fps >= 1 && args.fps <= 50, "--fps takes 1 to 50");
    anyhow::ensure!(
        args.width.is_none_or(|width| (200..=4000).contains(&width)),
        "--width takes 200 to 4000"
    );
    // An event the machine does not offer stops the play before anything
    // is drawn, not after the frames up to it.
    rehearse(args)?;
    // Each frame a tick later than the one before, however long drawing it
    // took: every frame is in the picture, and none waits.
    let _clock = crate::clock::Recording::start();
    let mut stage = Stage::new(args)?;
    let told = if !args.dives.is_empty() {
        stage.dive(&args.dives)?
    } else if !args.folds.is_empty() {
        stage.fold(&args.folds)?
    } else {
        stage.play(args)?
    };

    let Some(first) = stage.frames.first() else {
        bail!("no frame was drawn");
    };
    let (w, h) = (
        u16::try_from(first.width).context("too wide for a GIF")?,
        u16::try_from(first.height).context("too high for a GIF")?,
    );
    // A frame like the one before it only makes that one stay longer.
    let delay = u16::try_from((100 / args.fps).max(2)).unwrap_or(5);
    let mut kept: Vec<(Vec<u8>, u16)> = Vec::new();
    for frame in stage.frames {
        match kept.last_mut() {
            Some((last, stay)) if *last == frame.rgba => *stay = stay.saturating_add(delay),
            _ => kept.push((frame.rgba, delay)),
        }
    }
    let bytes = super::gifs::encode(w, h, &kept)?;
    crate::files::write_atomic(&args.output, &bytes)
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(format!(
        "{} → {} ({} frames, {}×{}, {})",
        args.file.display(),
        args.output.display(),
        kept.len(),
        w,
        h,
        told
    ))
}

/// The editor without a window, its diagram's pane on a screen, and the
/// frames drawn so far.
struct Stage {
    app: App,
    screen: Screen,
    frames: Vec<Shot>,
    /// How long a frame stays.
    tick: Duration,
    key: u64,
    text: String,
    language: Language,
    file: PathBuf,
}

impl Stage {
    /// The diagram of `args.file` drawn in a pane of its size, in the
    /// theme asked for.
    fn new(args: &Moving) -> Result<Self> {
        let text = std::fs::read_to_string(&args.file)
            .with_context(|| format!("cannot read {}", args.file.display()))?;
        let name = args
            .file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let language = args
            .forced
            .unwrap_or_else(|| detect(&text, Language::from_file_name(&name)));
        let project = name == omaspec_core::project::FILE;
        anyhow::ensure!(
            project || args.dives.is_empty(),
            "--dive zooms into the tiles of a project's map: give it the project's {}",
            omaspec_core::project::FILE
        );
        anyhow::ensure!(
            !project || !args.dives.is_empty(),
            "a project's map plays nothing; --dive TILE zooms into one of its tiles"
        );
        // A project is read from where it really lies, as the window
        // reads it.
        let file = if project {
            std::fs::canonicalize(&args.file)
                .with_context(|| format!("cannot find {}", args.file.display()))?
        } else {
            args.file.clone()
        };
        let start = Start {
            files: vec![file.clone()],
            folder: None,
            forced: args.forced,
        };
        let (mut app, _) = App::themed(start, None, theme(args.theme.as_deref())?);
        let key = app.doc().key;
        let _ = app.doc_mut().paper.set_scale_factor(1.0);
        let _ = app
            .doc_mut()
            .paper
            .update(paper::Message::Viewport(Size::new(800.0, 600.0)));
        if project {
            let root = file.parent().context("a project lies in a folder")?;
            let (specs, atlas) = crate::specs::read_project(root);
            let _ = app.update(Message::Specs(
                root.to_owned(),
                Arc::new(specs),
                Arc::new(atlas),
            ));
            map(&mut app, key)?;
        } else {
            shown(&mut app, key, &text, language, &file)?;
        }
        // The width asked for, else twice the diagram's size up to
        // WIDEST; the height from the diagram, with room for the bar a play
        // has.
        let bar = if args.folds.is_empty() && args.dives.is_empty() {
            BAR
        } else {
            0.0
        };
        let drawn = app.doc().paper.diagram_size();
        let width = match args.width {
            Some(width) => width as f32,
            None => (drawn.width * 2.0 + 2.0 * MARGIN)
                .clamp(320.0, WIDEST)
                .round(),
        };
        let height = (width * drawn.height / drawn.width.max(1.0) + bar)
            .clamp(width * LEAST_HEIGHT, width * MOST_HEIGHT)
            .round();
        let size = Size::new(width, height);
        app.doc_mut().paper.view.fit = true;
        // Through the window, which draws the diagram of a tile in the
        // pane's size before its tab shows.
        let _ = app.update(Message::Paper(key, paper::Message::Viewport(size)));
        app.doc_mut().paper.finish_rasters();

        let mut screen = match args.renderer {
            Some(renderer) => Screen::drawn_by(&app, 1.0, Some(renderer)),
            None => Screen::new(&app, 1.0),
        }
        .map_err(|why| anyhow::anyhow!(why))?;
        screen.size = size;
        screen.shows = Shows::Diagram;
        // The first frame starts uploading the pictures; they are on the
        // GPU a moment later.
        let _ = screen.shoot(&app);
        Ok(Self {
            app,
            screen,
            frames: Vec::new(),
            tick: Duration::from_micros(1_000_000 / u64::from(args.fps)),
            key,
            text,
            language,
            file,
        })
    }

    /// Draws frames for `most`, or until `done`.
    fn run(&mut self, most: Duration, done: impl Fn(&App) -> bool) {
        let count = (most.as_secs_f32() / self.tick.as_secs_f32()).ceil() as usize;
        let frames = &mut self.frames;
        self.screen
            .run_frames(&mut self.app, count.max(1), self.tick, done, |shot| {
                frames.push(Shot {
                    width: shot.width,
                    height: shot.height,
                    rgba: shot.rgba.clone(),
                });
            });
    }

    /// A step's motion to its end, and a moment on what it reached.
    fn step(&mut self, done: impl Fn(&App) -> bool) {
        self.run(SETTLE, done);
        self.run(HOLD, |_| false);
    }

    /// Runs the play; the events it took.
    fn play(&mut self, args: &Moving) -> Result<String> {
        let moving = |app: &App| !app.doc().paper.play_moving(crate::clock::now());
        let _ = self.app.update(Message::Command(Command::Play));
        if !self.app.doc().paper.playing() {
            bail!(
                "{}",
                self.app
                    .notice
                    .clone()
                    .unwrap_or_else(|| "the diagram cannot be played".into())
            );
        }
        self.step(moving);
        let mut taken = Vec::new();
        let mut walked = Vec::new();
        for step in 0..args.steps {
            let Some(paper::Offer::Machine { options, ended, .. }) = self.app.doc().paper.offer()
            else {
                // A scenario: its next message.
                let _ = self
                    .app
                    .update(Message::Command(Command::PlayAsk(Ask::Next)));
                if !self.app.doc().paper.playing() {
                    break;
                }
                self.step(moving);
                continue;
            };
            if ended || options.is_empty() {
                break;
            }
            let Some(choice) = choose(&options, &args.events, step, &walked)? else {
                break;
            };
            walked.push(options[choice].clone());
            taken.push(options[choice].0.trim().to_owned());
            let _ = self
                .app
                .update(Message::Command(Command::PlayAsk(Ask::Take(choice))));
            self.step(moving);
        }
        // The picture ends on the state the play reached, held a moment
        // longer, and loops from there: the play's leaving is not in it.
        self.run(HOLD, |_| false);
        Ok(if taken.is_empty() {
            "played".to_owned()
        } else {
            taken.join(", ")
        })
    }

    /// Folds the containers `names` one after another, as a click on
    /// their buttons does, and unfolds them the other way round, so the
    /// picture ends as it began; what it folded.
    fn fold(&mut self, names: &[String]) -> Result<String> {
        let offered = self.app.doc().paper.containers();
        for name in names {
            anyhow::ensure!(
                offered.contains(name),
                "{name} is no container to fold; the diagram has {}",
                if offered.is_empty() {
                    "none".to_owned()
                } else {
                    offered.join(", ")
                }
            );
        }
        // The first picture has faded in: the next glides from it.
        let paper = &mut self.app.doc_mut().paper;
        paper.deliver_overview();
        let _ = paper.update(paper::Message::Faded);
        // Its picture is on the GPU a moment after a frame asked for it.
        let _ = self.screen.frame(&self.app);
        std::thread::sleep(UPLOAD);
        self.run(HOLD, |_| false);
        let folds = names.iter().map(|name| (name, true));
        let unfolds = names.iter().rev().map(|name| (name, false));
        for (name, fold) in folds.chain(unfolds) {
            let ask = if fold {
                paper::Message::Fold(name.clone())
            } else {
                paper::Message::Unfold(name.clone())
            };
            let _ = self.app.update(Message::Paper(self.key, ask));
            shown(
                &mut self.app,
                self.key,
                &self.text,
                self.language,
                &self.file,
            )?;
            self.app.doc_mut().paper.deliver_overview();
            self.step(|app| {
                let paper = &app.doc().paper;
                paper.fading().is_none() && paper.glide().is_none()
            });
        }
        Ok(format!("{} folded and unfolded", names.join(", ")))
    }
    /// Zooms into each of the map's tiles `names` and back out, as a
    /// click on the tile and Alt+Left do; what it zoomed into.
    fn dive(&mut self, names: &[String]) -> Result<String> {
        let map = self.key;
        let still = |app: &App| app.doc().paper.glide().is_none();
        self.run(HOLD, |_| false);
        for name in names {
            let paper = &self.app.doc().paper;
            let at = paper
                .links()
                .iter()
                .find(|link| &link.name == name)
                .map(|link| {
                    paper
                        .view
                        .on_screen(
                            paper::link_rect(link),
                            iced::Rectangle::with_size(paper.viewport()),
                        )
                        .center()
                })
                .with_context(|| {
                    format!(
                        "{name} is no tile of the map; it has {}",
                        paper
                            .links()
                            .iter()
                            .map(|link| link.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?;
            let _ = self
                .app
                .update(Message::Paper(map, paper::Message::Follow(at)));
            let target = self
                .app
                .docs
                .iter()
                .map(|doc| doc.key)
                .find(|key| self.app.is_arriving(*key))
                .with_context(|| format!("{name} opens no diagram"))?;
            linked(&mut self.app, target)?;
            let _ = self.app.arrive();
            self.app.doc_mut().paper.finish_small();
            // The way in, the diagram in its own tab, and a moment there.
            self.run(self.tick, |_| false);
            self.run(DIVE, still);
            let _ = self
                .app
                .update(Message::Worker(worker::Event::Due(worker::Alarm::Follow)));
            self.run(self.tick * 2, |_| false);
            self.app.doc_mut().paper.finish_rasters();
            self.run(HOLD, |_| false);
            // The way back out, and the map again.
            let _ = self.app.update(Message::Command(Command::Back));
            self.app.doc_mut().paper.finish_small();
            self.run(self.tick, |_| false);
            self.run(DIVE, still);
            self.app.doc_mut().paper.finish_rasters();
            self.run(HOLD, |_| false);
        }
        Ok(format!("into {} and back", names.join(", ")))
    }
}

/// The event of an option's label, without its guard and its action:
/// `start [tank empty] / beep` offers `start`, `power on` is `power on`.
/// The option step `step` takes: the event `--events` names there, by
/// its label or by its event alone; without `--events`, the first way not
/// `walked` yet, else the first, so a play does not circle while others
/// wait. `None` ends the play: the events are all taken.
fn choose(
    options: &[(String, String)],
    events: &[String],
    step: usize,
    walked: &[(String, String)],
) -> Result<Option<usize>> {
    let Some(wanted) = events.get(step) else {
        return Ok(events.is_empty().then(|| {
            options
                .iter()
                .position(|option| !walked.contains(option))
                .unwrap_or(0)
        }));
    };
    options
        .iter()
        .position(|(label, _)| label.trim() == wanted.trim())
        .or_else(|| {
            options
                .iter()
                .position(|(label, _)| event_of(label) == wanted.trim())
        })
        .map(Some)
        .with_context(|| {
            format!(
                "step {}: {} is not offered here; the machine offers {}",
                step + 1,
                wanted,
                options
                    .iter()
                    .map(|(label, _)| label.trim())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
}

/// Walks `--events` on the state machine alone, as the play will, before
/// anything is drawn: an event it does not offer stops it at once, with
/// the words the play would say.
fn rehearse(args: &Moving) -> Result<()> {
    if args.events.is_empty() || !args.folds.is_empty() || !args.dives.is_empty() {
        return Ok(());
    }
    let text = std::fs::read_to_string(&args.file)
        .with_context(|| format!("cannot read {}", args.file.display()))?;
    let name = args
        .file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let language = args
        .forced
        .unwrap_or_else(|| detect(&text, Language::from_file_name(&name)));
    let Some(machine) = omaspec_model::Machine::read(&text, language) else {
        return Ok(());
    };
    let Some((mut run, _)) = omaspec_model::Run::start(&machine) else {
        return Ok(());
    };
    let mut walked = Vec::new();
    for step in 0..args.steps {
        let ways = run.options(&machine);
        if run.ended(&machine) || ways.is_empty() {
            break;
        }
        let options: Vec<(String, String)> = ways
            .iter()
            .map(|&way| paper::option_of(&machine, way))
            .collect();
        let Some(choice) = choose(&options, &args.events, step, &walked)? else {
            break;
        };
        walked.push(options[choice].clone());
        let _ = run.take(&machine, ways[choice]);
    }
    Ok(())
}

fn event_of(label: &str) -> &str {
    label.split(['[', '/']).next().unwrap_or(label).trim()
}

/// The theme the frames are drawn in.
fn theme(named: Option<&str>) -> Result<Omarchy> {
    let built_in = |name: &str| Omarchy {
        name: Some(name.to_owned()),
        palette: omaspec_theme::Palette::fallback(),
        source: omaspec_theme::Source::Fallback {
            reason: "asked for".into(),
        },
        tweaks: omaspec_theme::Tweaks::default(),
    };
    match named {
        None => Ok(Omarchy::load_current()),
        Some("built-in") => Ok(built_in("built-in")),
        Some(file) => {
            let colors_file = PathBuf::from(file);
            let palette = omaspec_theme::Palette::load(&colors_file)
                .with_context(|| format!("--theme takes built-in or a colors.toml: {file}"))?;
            Ok(Omarchy {
                name: colors_file
                    .parent()
                    .and_then(|dir| dir.file_name())
                    .map(|name| name.to_string_lossy().into_owned()),
                palette,
                source: omaspec_theme::Source::Omarchy { colors_file },
                tweaks: omaspec_theme::Tweaks::default(),
            })
        }
    }
}

/// Draws the project's map in the pane of its file, as the window makes
/// it from the project's diagrams (docs/adr/0026).
fn map(app: &mut App, key: u64) -> Result<()> {
    let (atlas, _) = app
        .linked_of(key)
        .context("the project could not be read")?;
    let (skin, _) = app.diagram_source(crate::look::MAP_SKIN, Language::PlantUml);
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let rendered = renderer
        .render_map(&atlas, &app.doc().folder, &skin)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let drawn = Drawn::of(rendered).map_err(|err| anyhow::anyhow!("{err}"))?;
    let id = app.doc_mut().paper.pretend_next();
    let mut reply = headless::answer(key, id, Ok(drawn));
    reply.language = Language::PlantUml;
    reply.source = skin;
    let paper = &mut app.doc_mut().paper;
    let _ = paper.rendered(reply);
    paper.finish_rasters();
    Ok(())
}

/// Draws the diagram of the tab `key` as one of its project, with its
/// links, and hands it in as the render thread does.
fn linked(app: &mut App, key: u64) -> Result<()> {
    let (atlas, own) = app
        .linked_of(key)
        .context("the project could not be read")?;
    let doc = app.find(key).context("the diagram's tab")?;
    let (text, language) = (doc.text(), doc.language);
    let (source, pending) = app.diagram_source(&text, language);
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let rendered = renderer
        .render_linked(&source, language, None, own.as_deref(), &atlas)
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let drawn = Drawn::of(rendered).map_err(|err| anyhow::anyhow!("{err}"))?;
    let id = app
        .find(key)
        .context("the diagram's tab")?
        .paper
        .pretend_next();
    let mut reply = headless::answer(key, id, Ok(drawn));
    reply.language = language;
    reply.source = source;
    if let Some(pending) = pending {
        app.pending.insert((key, id), pending);
    }
    let _ = app.update(Message::Worker(worker::Event::Rendered(Box::new(reply))));
    app.find(key)
        .context("the diagram's tab")?
        .paper
        .finish_rasters();
    Ok(())
}

/// Draws `text` as the pane shows it, in the theme's colors and with
/// the containers folded that the pane has folded, here and now.
fn shown(
    app: &mut App,
    key: u64,
    text: &str,
    language: Language,
    file: &std::path::Path,
) -> Result<()> {
    let (source, pending) = app.diagram_source(text, language);
    let place = Place::of(file, None);
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let rendered = renderer
        .render_at(
            &crate::worker::folded(
                &source,
                language,
                app.doc().paper.folded(),
                app.doc().paper.views,
            ),
            language,
            place.as_ref(),
        )
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    let drawn = Drawn::of(rendered).map_err(|err| anyhow::anyhow!("{err}"))?;
    let id = app.doc_mut().paper.pretend_next();
    let mut reply = headless::answer(key, id, Ok(drawn));
    reply.language = language;
    reply.source = source;
    if let Some(pending) = pending {
        app.pending.insert((key, id), pending);
        app.untangle(&mut reply);
    }
    let _ = app.doc_mut().paper.rendered(reply);
    app.doc_mut().paper.finish_rasters();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drawn without a GPU, which takes longer than a frame lasts, a play
    /// still has every frame: each is a tick after the one before, on the
    /// recording's own clock. So it comes out the same each time.
    #[test]
    fn a_recorded_play_has_every_frame_however_slow_the_drawing() {
        let work = std::env::temp_dir().join(format!("omaspec-moving-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let file = work.join("door.puml");
        std::fs::write(
            &file,
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
        )
        .unwrap();
        let record = |name: &str| {
            let output = work.join(name);
            let told = play(&Moving {
                file: file.clone(),
                output: output.clone(),
                events: vec!["open".into()],
                steps: 1,
                width: Some(200),
                fps: 5,
                folds: Vec::new(),
                dives: Vec::new(),
                theme: Some("built-in".into()),
                forced: None,
                renderer: Some("tiny-skia"),
            })
            .unwrap();
            (told, std::fs::read(output).unwrap())
        };
        let (told, first) = record("first.gif");
        let (_, second) = record("second.gif");
        let frames: usize = told
            .split('(')
            .nth(1)
            .and_then(|rest| rest.split(' ').next())
            .and_then(|count| count.parse().ok())
            .unwrap();
        // The light runs along the arrow for most of a second, at 5 frames
        // a second, each frame a little further on.
        assert!(frames >= 4, "{told}");
        assert!(first == second, "two recordings of one play differ");
        // As wide as asked, though the door drawn twice would be wider.
        assert_eq!(u16::from_le_bytes([first[6], first[7]]), 200);
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn a_play_takes_a_way_not_walked_before_one_walked() {
        let options = |labels: &[&str]| -> Vec<(String, String)> {
            labels
                .iter()
                .map(|label| ((*label).to_owned(), "There".to_owned()))
                .collect()
        };
        let walked = options(&["nap"]);
        assert_eq!(
            choose(&options(&["nap", "hungry"]), &[], 3, &walked).unwrap(),
            Some(1)
        );
        // All walked: the first again.
        assert_eq!(
            choose(&options(&["nap"]), &[], 3, &walked).unwrap(),
            Some(0)
        );
        // Events asked for: past their end the play ends.
        let asked = vec!["nap".to_owned()];
        assert_eq!(choose(&options(&["nap"]), &asked, 0, &[]).unwrap(), Some(0));
        assert_eq!(choose(&options(&["nap"]), &asked, 1, &[]).unwrap(), None);
    }

    #[test]
    fn an_event_not_offered_stops_the_play_before_it_draws() {
        let work = std::env::temp_dir().join(format!("omaspec-rehearse-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let file = work.join("door.puml");
        std::fs::write(
            &file,
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
        )
        .unwrap();
        let moving = |events: &[&str]| Moving {
            file: file.clone(),
            output: work.join("never.gif"),
            events: events.iter().map(|e| (*e).to_owned()).collect(),
            steps: 8,
            width: None,
            fps: 10,
            folds: Vec::new(),
            dives: Vec::new(),
            theme: Some("built-in".into()),
            forced: None,
            renderer: Some("tiny-skia"),
        };
        rehearse(&moving(&["open", "close", "open"])).unwrap();
        let err = rehearse(&moving(&["open", "fly"])).unwrap_err().to_string();
        assert_eq!(
            err,
            "step 2: fly is not offered here; the machine offers close"
        );
        // The play itself says so too, and writes nothing.
        assert!(play(&moving(&["open", "fly"])).is_err());
        assert!(!work.join("never.gif").exists());
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn an_event_is_its_label_without_guard_and_action() {
        assert_eq!(event_of("open"), "open");
        assert_eq!(event_of("power on"), "power on");
        assert_eq!(event_of("start [tank empty]"), "start");
        assert_eq!(event_of("pay / beep"), "pay");
    }
}
