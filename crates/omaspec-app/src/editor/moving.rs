//! A play as a picture that moves: `omaspec play FILE -o play.gif`
//! draws the diagram's pane headless, as the window draws it, runs the
//! play on it and writes the frames as a GIF ([`super::gifs`]), or in
//! full color as an animated WebP or PNG ([`super::films`]), by the
//! file's ending, for a presentation or a pull request.
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

use super::films::Film;
use super::screen::{Screen, Shows};
use super::{App, Command, Message, Start};
use crate::paper::{self, Ask, headless};
use crate::worker::{self, Drawn};

/// Time travel: the file as the commit `from` left it gliding into it as
/// `to` left it, or as it is where there is none (`--from v1.0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Travel {
    pub from: String,
    pub to: Option<String>,
}

/// What the command line asks for.
#[derive(Debug, Clone)]
pub struct Moving {
    pub file: PathBuf,
    /// The diagram's text where it is not the file's: a tab not saved.
    pub text: Option<String>,
    pub output: PathBuf,
    /// The events to take, in order; empty takes the first offered.
    pub events: Vec<String>,
    /// How many steps at most.
    pub steps: usize,
    /// The picture's width in pixels, the height following the diagram;
    /// `None` takes twice the diagram's size, up to [`WIDEST`].
    pub width: Option<u32>,
    /// Frames a second; `None` takes what the picture's kind usually has.
    pub fps: Option<u32>,
    /// The containers to fold and unfold instead of a play.
    pub folds: Vec<String>,
    /// The tiles of a project's map to zoom into and back out of.
    pub dives: Vec<String>,
    /// Instead of a play: the source beside the diagram, which builds
    /// itself up line by line ([`super::building`]).
    pub builds: bool,
    /// Instead of a play of the diagram alone: a scenario played beside
    /// the state machines of its participants ([`super::lockstep`]).
    pub lockstep: bool,
    /// Instead of a play: every commit that changed the file, each
    /// picture gliding into the next ([`super::timelapse`]).
    pub history: bool,
    /// Instead of a play: the file in one commit gliding into it in
    /// another, or as it is ([`super::timelapse`]).
    pub travel: Option<Travel>,
    /// Instead of a play: these recorded traces flowing through the
    /// machine at once ([`super::traffic`]).
    pub traffic: Vec<PathBuf>,
    /// Instead of a play: the diagram through these themes, each sweeping
    /// over the one before; empty takes every bundled one
    /// ([`super::sweep`]).
    pub themes: Option<Vec<String>>,
    /// Instead of a play: the diagram drawn in the sketch look as with a
    /// pen, then inked into the clean look ([`super::ink`]).
    pub ink: bool,
    /// The whole diagram in every frame, rather than a camera that follows
    /// the play where the whole is too large to read (REQ-071).
    pub whole: bool,
    /// The colors: `None` the active theme's, `built-in` the palette
    /// Omaspec ships, else a theme's `colors.toml`.
    pub theme: Option<String>,
    /// In the file's own colors, not the theme's, as the window draws it
    /// with *Diagram in Omarchy's colors* off.
    pub file_colors: bool,
    /// The project the diagram is of, whose pinned look it is drawn in as
    /// the pane draws it; `None` draws in the theme's colors, as
    /// `omaspec play` does.
    pub root: Option<PathBuf>,
    pub forced: Option<Language>,
    /// The renderer that draws (`tiny-skia`, without a GPU); `None` takes
    /// `ICED_TEST_BACKEND`, else the first that can.
    pub renderer: Option<&'static str>,
    /// What a GIF gives up to be smaller: colors, steps of a fade. A
    /// picture in full color gives up nothing.
    pub quality: super::gifs::Quality,
}

impl Moving {
    /// The diagram's text: the one given, else the file's.
    pub(super) fn read(&self) -> Result<String> {
        match &self.text {
            Some(text) => Ok(text.clone()),
            None => std::fs::read_to_string(&self.file)
                .with_context(|| format!("cannot read {}", self.file.display())),
        }
    }

    /// The file's name, and the language of `text`: the one forced, else
    /// what its words and the name say.
    pub(super) fn named(&self, text: &str) -> (String, Language) {
        let name = self
            .file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let language = self
            .forced
            .unwrap_or_else(|| detect(text, Language::from_file_name(&name)));
        (name, language)
    }
}

/// Each of `names` is a container the diagram `offered` to fold; else
/// which it has.
fn offered_all<S: AsRef<str>>(names: &[String], offered: &[S]) -> Result<()> {
    for name in names {
        anyhow::ensure!(
            offered.iter().any(|one| one.as_ref() == name),
            "{name} is no container to fold; the diagram has {}",
            if offered.is_empty() {
                "none".to_owned()
            } else {
                offered
                    .iter()
                    .map(AsRef::as_ref)
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        );
    }
    Ok(())
}

/// The frames the first picture may take to fade in, before the frames
/// the picture keeps.
pub(super) const FADE_IN_FRAMES: usize = 60;
/// How long the play holds after each step, and at the end.
pub(super) const HOLD: Duration = Duration::from_millis(500);
/// How long a zoom into a tile may take, or out of it.
const DIVE: Duration = Duration::from_millis(4_000);
/// How long a step may take to settle before the next.
pub(super) const SETTLE: Duration = Duration::from_millis(2_500);
/// The pane's height is between these shares of its width.
const LEAST_HEIGHT: f32 = 0.3;
const MOST_HEIGHT: f32 = 1.4;
/// Around the diagram, and below it for the bar of the play.
const MARGIN: f32 = 40.0;
/// How wide a picture is at most when no width is asked for.
const WIDEST: f32 = 1200.0;
pub(super) const BAR: f32 = 90.0;

/// Writes the play, or the fold; says what it wrote.
pub fn play(args: &Moving) -> Result<String> {
    let film = Film::of(&args.output).with_context(|| {
        format!(
            "{} is no picture that moves: .gif, .webp, .png (an animated PNG), or .html (a fold, a dive, a build, a lockstep, a history, traffic, a sweep or ink as a page)",
            args.output.display()
        )
    })?;
    let fps = args.fps.unwrap_or(film.usual_fps());
    anyhow::ensure!(
        fps >= 1 && fps <= film.most_fps(),
        "--fps takes 1 to {} for {}",
        film.most_fps(),
        match film {
            Film::Gif => "a GIF",
            Film::Apng => "an animated PNG",
            Film::Webp => "a WebP",
            Film::Page => "a page",
        }
    );
    if args.builds {
        anyhow::ensure!(
            args.width.is_none_or(|width| (200..=4000).contains(&width)),
            "--width takes 200 to 4000"
        );
        return super::building::build(args, film, fps);
    }
    if args.ink {
        anyhow::ensure!(
            args.width.is_none_or(|width| (200..=4000).contains(&width)),
            "--width takes 200 to 4000"
        );
        return super::ink::ink(args, film, fps);
    }
    if let Some(names) = &args.themes {
        anyhow::ensure!(
            args.width.is_none_or(|width| (200..=4000).contains(&width)),
            "--width takes 200 to 4000"
        );
        return super::sweep::sweep(args, names, film, fps);
    }
    if !args.traffic.is_empty() {
        anyhow::ensure!(
            args.width.is_none_or(|width| (200..=4000).contains(&width)),
            "--width takes 200 to 4000"
        );
        return super::traffic::traffic(args, film, fps);
    }
    if args.history || args.travel.is_some() {
        anyhow::ensure!(
            args.width.is_none_or(|width| (200..=4000).contains(&width)),
            "--width takes 200 to 4000"
        );
        return super::timelapse::timelapse(args, film, fps);
    }
    if args.lockstep {
        anyhow::ensure!(
            args.width.is_none_or(|width| (400..=4000).contains(&width)),
            "--width takes 400 to 4000 for a lockstep"
        );
        return super::lockstep::lockstep(args, film, fps);
    }
    if film == Film::Page {
        if !args.dives.is_empty() {
            let (name, _) = args.named("");
            anyhow::ensure!(
                name == omaspec_core::project::FILE,
                "--dive zooms into the tiles of a project's map: give it the project's {}",
                omaspec_core::project::FILE
            );
            return super::flight::write(args);
        }
        return fold_page(args);
    }
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
    let mut stage = Stage::new(args, film, fps)?;
    let told = if !args.dives.is_empty() {
        stage.dive(&args.dives)?
    } else if !args.folds.is_empty() {
        stage.fold(&args.folds, !args.whole)?
    } else {
        stage.play(args)?
    };

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

/// Writes the fold of `args.folds` as a page that plays it in vectors
/// ([`omaspec_render::export::folding`]): the picture before the first
/// fold and the one after each, drawn here in the theme's colors with
/// their parts marked, and the glide from each to the next as the pane
/// works it out. Nothing is drawn in pixels, so it needs no GPU.
fn fold_page(args: &Moving) -> Result<String> {
    use omaspec_render::export::folding;

    anyhow::ensure!(
        !args.folds.is_empty() && args.dives.is_empty(),
        "a page plays a fold: name the containers with --fold (a play as a page is \
         `omaspec render FILE -o play.html`)"
    );
    let text = args.read()?;
    let (name, language) = args.named(&text);
    let start = Start {
        files: vec![args.file.clone()],
        folder: None,
        forced: args.forced,
    };
    let (mut app, _) = App::themed(start, None, theme(args.theme.as_deref())?);
    app.follow_theme = !args.file_colors;
    let (source, _) = app.diagram_source_in(args.root.as_deref(), &text, language);
    let place = Place::of(&args.file, None);
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let mut maps = Vec::new();
    let mut pictures = Vec::new();
    for folded in 0..=args.folds.len() {
        let asked = crate::worker::folded(
            &source,
            language,
            &args.folds[..folded],
            crate::worker::Views::default(),
        );
        let rendered = renderer
            .render_at(&asked, language, place.as_ref())
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        if folded == 0 {
            let offered: Vec<&str> = rendered
                .map
                .places
                .iter()
                .filter(|place| place.frame)
                .map(|place| place.name.as_str())
                .collect();
            offered_all(&args.folds, &offered)?;
            anyhow::ensure!(
                omaspec_uml::has_parts(&rendered.svg),
                "this kind of diagram does not glide by its parts yet: write the fold as .webp, .png or .gif"
            );
        }
        let size = omaspec_render::export::parse(&rendered.svg)
            .map_err(|err| anyhow::anyhow!("{err}"))?
            .size();
        pictures.push(folding::Picture {
            svg: rendered.svg,
            size: (size.width(), size.height()),
        });
        maps.push(rendered.map);
    }
    // The page draws in each picture's own units, which a `scale` sets
    // apart from the pixels the map is in.
    let units: Vec<super::timelapse::Units> = pictures
        .iter()
        .map(|p| super::timelapse::Units::of(&p.svg, p.size))
        .collect();
    let glides = maps
        .windows(2)
        .enumerate()
        .map(|(at, maps)| {
            let mut glide = paper::headless::fold_glide(
                &maps[0],
                &maps[1],
                pictures[at].size,
                pictures[at + 1].size,
                false,
            );
            super::timelapse::remap(&mut glide, units[at], units[at + 1]);
            glide
        })
        .collect();
    for (picture, units) in pictures.iter_mut().zip(&units) {
        picture.size = units.size(picture.size);
    }
    let (paper, accent) = crate::look::palette_stage(&app.palette);
    let page = folding::page(
        &folding::Folding {
            names: args.folds.clone(),
            captions: Vec::new(),
            pictures,
            glides,
            paper,
            accent,
        },
        Some(omaspec_render::export::Source {
            code: &text,
            language,
            file: Some(&name),
        }),
        name.rsplit_once('.')
            .map_or(name.as_str(), |(stem, _)| stem),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    crate::files::write_atomic(&args.output, page.as_bytes())
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(format!(
        "{} → {} (a page, {} KB, {} folded and unfolded in vectors)",
        args.file.display(),
        args.output.display(),
        page.len().div_ceil(1024),
        args.folds.join(", ")
    ))
}

/// The editor without a window, its diagram's pane on a screen, and the
/// frames drawn so far.
struct Stage {
    app: App,
    screen: Screen,
    frames: super::gifs::Frames,
    /// How long each frame stays: hundredths of a second in a GIF, one
    /// tick in a picture in full color.
    delay: u16,
    /// How long a frame stays.
    tick: Duration,
    key: u64,
    text: String,
    language: Language,
    file: PathBuf,
    /// The project whose look the diagram is drawn in ([`Moving::root`]).
    root: Option<PathBuf>,
}

impl Stage {
    /// The diagram of `args.file` drawn in a pane of its size, in the
    /// theme asked for.
    fn new(args: &Moving, film: Film, fps: u32) -> Result<Self> {
        let text = args.read()?;
        let (name, language) = args.named(&text);
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
        app.follow_theme = !args.file_colors;
        let key = app.doc().key;
        // The pane's pictures go up to this screen's GPU as they are made,
        // as the window's go up to its own.
        let mut screen = match args.renderer {
            Some(renderer) => Screen::drawn_by(&app, 1.0, Some(renderer)),
            None => Screen::new(&app, 1.0),
        }
        .map_err(|why| anyhow::anyhow!(why))?;
        let scaled = app.doc_mut().paper.set_scale_factor(1.0);
        screen.run_task(&mut app, scaled.map(move |m| Message::Paper(key, m)));
        screen.perform(
            &mut app,
            Message::Paper(key, paper::Message::Viewport(Size::new(800.0, 600.0))),
        );
        if project {
            let root = file.parent().context("a project lies in a folder")?;
            let (specs, atlas) = crate::specs::read_project(root);
            screen.perform(
                &mut app,
                Message::Specs(root.to_owned(), Arc::new(specs), Arc::new(atlas)),
            );
            map(&mut app, &screen, key)?;
        } else {
            shown(
                &mut app,
                &screen,
                key,
                &text,
                language,
                &file,
                args.root.as_deref(),
            )?;
        }
        // The width asked for, else twice the diagram's size up to
        // WIDEST; the height from the diagram, with room for the bar a play
        // has, in as many rows as its options will need.
        let drawn = app.doc().paper.diagram_size();
        let width = match args.width {
            Some(width) => width as f32,
            None => (drawn.width * 2.0 + 2.0 * MARGIN)
                .clamp(320.0, WIDEST)
                .round(),
        };
        let bar = if args.folds.is_empty() && args.dives.is_empty() {
            BAR + paper::play_bar_extra(&text, language, width)
        } else {
            0.0
        };
        let height = (width * drawn.height / drawn.width.max(1.0) + bar)
            .clamp(width * LEAST_HEIGHT, width * MOST_HEIGHT)
            .round();
        let size = Size::new(width, height);
        app.doc_mut().paper.view.fit = true;
        // Through the window, which draws the diagram of a tile in the
        // pane's size before its tab shows.
        screen.perform(
            &mut app,
            Message::Paper(key, paper::Message::Viewport(size)),
        );
        screen.size = size;
        screen.shows = Shows::Diagram;
        let tick = Duration::from_micros(1_000_000 / u64::from(fps));
        // The picture stands from the first frame kept: it fades in over
        // frames that are drawn and left out, as it has faded in by the
        // time a window's reader looks at it.
        screen.run_frames(
            &mut app,
            FADE_IN_FRAMES,
            tick,
            |app| app.doc().paper.fading().is_none(),
            |_| {},
        );
        Ok(Self {
            app,
            screen,
            frames: super::gifs::Frames::default(),
            delay: match film {
                Film::Gif => u16::try_from((100 / fps).max(2)).unwrap_or(5),
                Film::Apng | Film::Webp | Film::Page => 1,
            },
            tick,
            key,
            text,
            language,
            file,
            root: args.root.clone(),
        })
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

    /// A step's motion to its end, and a moment on what it reached.
    fn step(&mut self, done: impl Fn(&App) -> bool) {
        self.run(SETTLE, done);
        self.run(HOLD, |_| false);
    }

    /// Hands `message` to the editor, its tasks run at once.
    fn send(&mut self, message: Message) {
        self.screen.perform(&mut self.app, message);
    }

    /// Runs the play; the events it took.
    fn play(&mut self, args: &Moving) -> Result<String> {
        let moving = |app: &App| !app.doc().paper.play_moving(crate::clock::now());
        // Where the whole is too large to read, the view follows each step.
        self.app.doc_mut().paper.follow_play = !args.whole;
        self.send(Message::Command(Command::Play));
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
                self.send(Message::Command(Command::PlayAsk(Ask::Next)));
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
            self.send(Message::Command(Command::PlayAsk(Ask::Take(choice))));
            self.step(moving);
        }
        // The picture ends on the state the play reached, held a moment
        // longer, and loops from there: the play's leaving is not in it. A
        // camera that followed it goes back to the whole as the play starts
        // again, so the loop begins where it ends.
        self.run(HOLD, |_| false);
        if !self.app.doc().paper.view.fit {
            self.send(Message::Command(Command::PlayAsk(Ask::Restart)));
            self.step(moving);
        }
        Ok(if taken.is_empty() {
            "played".to_owned()
        } else {
            taken.join(", ")
        })
    }

    /// Folds the containers `names` one after another, as a click on
    /// their buttons does, and unfolds them the other way round, so the
    /// picture ends as it began; what it folded.
    /// With `follow`, where the whole is too large to read, the view goes
    /// to each container before it folds and to its box after, as a play's
    /// camera follows its steps, and ends on the whole (REQ-071).
    fn fold(&mut self, names: &[String], follow: bool) -> Result<String> {
        let offered = self.app.doc().paper.containers();
        offered_all(names, &offered)?;
        self.run(HOLD, |_| false);
        let folds = names.iter().map(|name| (name, true));
        let unfolds = names.iter().rev().map(|name| (name, false));
        let still = |app: &App| {
            let paper = &app.doc().paper;
            paper.fading().is_none() && paper.glide().is_none()
        };
        // Only where the camera moves does the picture wait for it: a
        // diagram that reads whole folds as it did.
        let camera = |stage: &mut Self, name: Option<&String>| {
            if follow {
                let ask = paper::Message::Camera(name.cloned());
                stage.send(Message::Paper(stage.key, ask));
                if stage.app.doc().paper.glide().is_some() {
                    stage.step(still);
                }
            }
        };
        for (name, fold) in folds.chain(unfolds) {
            camera(self, Some(name));
            let ask = if fold {
                paper::Message::Fold(name.clone())
            } else {
                paper::Message::Unfold(name.clone())
            };
            self.send(Message::Paper(self.key, ask));
            shown(
                &mut self.app,
                &self.screen,
                self.key,
                &self.text,
                self.language,
                &self.file,
                self.root.as_deref(),
            )?;
            self.step(still);
            camera(self, Some(name));
        }
        camera(self, None);
        Ok(format!("{} folded and unfolded", names.join(", ")))
    }
    /// Zooms into each of the map's tiles `names` and back out, as a
    /// click on the tile and Alt+Left do; what it zoomed into.
    fn dive(&mut self, names: &[String]) -> Result<String> {
        let still = |app: &App| app.doc().paper.glide().is_none();
        self.run(HOLD, |_| false);
        for name in names {
            // `Orders/Checkout`: into the tile, and on along the link of
            // the diagram there, a flight through the project
            // (docs/adr/0018); back out the same way. A name with a `/`
            // of its own (`Login/Logout`) is taken whole where it names
            // a link.
            let parts: Vec<&str> = name.split('/').collect();
            anyhow::ensure!(
                parts.iter().all(|part| !part.trim().is_empty()),
                "{name}: a flight names a tile and the links after it, as Orders/Checkout"
            );
            // Each flight starts on the map, where the one before came
            // back out to.
            anyhow::ensure!(
                self.app.doc().key == self.key,
                "{name}: the way back out of the flight before did not reach the map"
            );
            // What each step was: a diagram followed, or a folder's map
            // shown in the map's own tab, which the way back draws again.
            let mut taken: Vec<(String, bool)> = Vec::new();
            let mut next = 0;
            while next < parts.len() {
                let here = self.app.doc().key;
                let paper = &self.app.doc().paper;
                let links = paper.links();
                // The longest run of the parts left that names a link here.
                let found = (next + 1..=parts.len()).rev().find_map(|end| {
                    let wanted = parts[next..end].join("/");
                    links
                        .iter()
                        .find(|one| one.name.trim() == wanted.trim())
                        .map(|one| (end, one))
                });
                let Some((end, link)) = found else {
                    let what = match taken.last() {
                        None => "no tile of the map".to_owned(),
                        Some((last, _)) => format!("no link of {last}"),
                    };
                    let has = if links.is_empty() {
                        "none".to_owned()
                    } else {
                        links
                            .iter()
                            .map(|one| one.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    bail!("{} is {what}; it has {has}", parts[next].trim());
                };
                let at = click_on(paper, link).with_context(|| {
                    format!("{} lies under other links: no click reaches it", link.name)
                })?;
                let followed = link.name.clone();
                let folder = link.folder.is_some();
                self.send(Message::Paper(here, paper::Message::Follow(at)));
                // A folder's tile shows the folder's map in this tab: the
                // flight goes on there.
                if folder {
                    map(&mut self.app, &self.screen, here)?;
                    self.run(self.tick, |_| false);
                    self.run(DIVE, |app| app.doc().paper.arrived());
                    self.run(HOLD, |_| false);
                    taken.push((followed, true));
                    next = end;
                    continue;
                }
                let target = self
                    .app
                    .docs
                    .iter()
                    .map(|doc| doc.key)
                    .find(|key| self.app.is_arriving(*key))
                    .with_context(|| match &self.app.notice {
                        Some(why) => format!("{followed} opens no diagram: {why}"),
                        None => format!("{followed} opens no diagram"),
                    })?;
                // Its picture arrives with the zoom.
                linked(&mut self.app, &self.screen, target)?;
                // The way in, the diagram in its own tab, and a moment there.
                self.run(self.tick, |_| false);
                self.run(DIVE, still);
                self.send(Message::Worker(worker::Event::Due(worker::Alarm::Follow)));
                self.run(self.tick * 2, |_| false);
                self.run(HOLD, |_| false);
                taken.push((followed, false));
                next = end;
            }
            // The way back out, a step for each link, and the map again;
            // out of a folder, the map of the folder around it drawn.
            for (_, folder) in taken.iter().rev() {
                self.send(Message::Command(Command::Back));
                if *folder {
                    let here = self.app.doc().key;
                    map(&mut self.app, &self.screen, here)?;
                    self.run(self.tick, |_| false);
                    self.run(DIVE, |app| app.doc().paper.arrived());
                } else {
                    self.run(self.tick, |_| false);
                    self.run(DIVE, still);
                }
                self.run(HOLD, |_| false);
            }
        }
        Ok(format!("into {} and back", names.join(", ")))
    }
}

/// A point of the pane where a click takes `link`: its middle, else the
/// first of a grid over it where no smaller link lies over it, as the
/// states inside a state's box do.
fn click_on(paper: &paper::Paper, link: &omaspec_render::Link) -> Option<iced::Point> {
    const GRID: usize = 16;
    let rect = paper.view.on_screen(
        paper::link_rect(link),
        iced::Rectangle::with_size(paper.viewport()),
    );
    std::iter::once(rect.center())
        .chain((0..GRID * GRID).map(|i| {
            let (column, row) = ((i % GRID) as f32, (i / GRID) as f32);
            iced::Point::new(
                rect.x + rect.width * (column + 0.5) / GRID as f32,
                rect.y + rect.height * (row + 0.5) / GRID as f32,
            )
        }))
        .find(|&at| {
            paper
                .link_at(at)
                .is_some_and(|under| std::ptr::eq(under, link))
        })
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
    let text = args.read()?;
    let (_, language) = args.named(&text);
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
        let options: Vec<(String, String)> = ways.iter().map(|&way| machine.option(way)).collect();
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
pub(super) fn theme(named: Option<&str>) -> Result<Omarchy> {
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
fn map(app: &mut App, screen: &Screen, key: u64) -> Result<()> {
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
    screen.perform(
        app,
        Message::Worker(worker::Event::Rendered(Box::new(reply))),
    );
    Ok(())
}

/// Draws the diagram of the tab `key` as one of its project, with its
/// links, and hands it in as the render thread does.
fn linked(app: &mut App, screen: &Screen, key: u64) -> Result<()> {
    let (atlas, own) = app
        .linked_of(key)
        .context("the project could not be read")?;
    let doc = app.find(key).context("the diagram's tab")?;
    let (text, language) = (doc.text(), doc.language);
    // Its folder, as the window draws it there: an `!include` is found.
    let place = doc
        .path
        .clone()
        .and_then(|path| Place::of(&path, app.folder.as_deref()));
    let (source, pending) = app.diagram_source(&text, language);
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let rendered = renderer
        .render_linked(&source, language, place.as_ref(), own.as_deref(), &atlas)
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
    screen.perform(
        app,
        Message::Worker(worker::Event::Rendered(Box::new(reply))),
    );
    Ok(())
}

/// Draws `text` as the pane shows it, in the theme's colors and with
/// the containers folded that the pane has folded, here and now.
pub(super) fn shown(
    app: &mut App,
    screen: &Screen,
    key: u64,
    text: &str,
    language: Language,
    file: &std::path::Path,
    root: Option<&std::path::Path>,
) -> Result<()> {
    let (source, pending) = app.diagram_source_in(root, text, language);
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
    }
    screen.perform(
        app,
        Message::Worker(worker::Event::Rendered(Box::new(reply))),
    );
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
                text: None,
                output: output.clone(),
                events: vec!["open".into()],
                steps: 1,
                width: Some(200),
                fps: Some(5),
                folds: Vec::new(),
                dives: Vec::new(),
                builds: false,
                lockstep: false,
                history: false,
                travel: None,
                traffic: Vec::new(),
                themes: None,
                ink: false,
                whole: false,
                theme: Some("built-in".into()),
                file_colors: false,
                root: None,
                forced: None,
                renderer: Some("tiny-skia"),
                quality: super::super::gifs::Quality::default(),
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

    /// The same play in full color: as many frames as the GIF's, each as
    /// long as a tick, in a WebP and in an animated PNG; another ending is
    /// refused before anything is drawn.
    #[test]
    fn a_play_is_written_in_full_color_by_its_ending() {
        let work = std::env::temp_dir().join(format!("omaspec-films-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let file = work.join("door.puml");
        std::fs::write(
            &file,
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
        )
        .unwrap();
        let moving = |name: &str, fps: Option<u32>| Moving {
            file: file.clone(),
            text: None,
            output: work.join(name),
            events: vec!["open".into()],
            steps: 1,
            width: Some(200),
            fps,
            folds: Vec::new(),
            dives: Vec::new(),
            builds: false,
            lockstep: false,
            history: false,
            travel: None,
            traffic: Vec::new(),
            themes: None,
            ink: false,
            whole: false,
            theme: Some("built-in".into()),
            file_colors: false,
            root: None,
            forced: None,
            renderer: Some("tiny-skia"),
            quality: super::super::gifs::Quality::default(),
        };
        play(&moving("door.webp", Some(5))).unwrap();
        let bytes = std::fs::read(work.join("door.webp")).unwrap();
        let webp = image_webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(webp.dimensions().0, 200);
        assert!(webp.num_frames() >= 4, "{}", webp.num_frames());

        play(&moving("door.png", Some(5))).unwrap();
        let bytes = std::fs::read(work.join("door.png")).unwrap();
        let png = png::Decoder::new(std::io::Cursor::new(&bytes))
            .read_info()
            .unwrap();
        assert_eq!(
            png.info().animation_control.unwrap().num_frames,
            webp.num_frames()
        );

        let err = play(&moving("door.mp4", None)).unwrap_err().to_string();
        assert!(err.contains("is no picture that moves"), "{err}");
        let err = play(&moving("door.gif", Some(60))).unwrap_err().to_string();
        assert_eq!(err, "--fps takes 1 to 50 for a GIF");
        assert!(play(&moving("door.webp", Some(61))).is_err());
        let _ = std::fs::remove_dir_all(&work);
    }

    /// A fold of a text that is in no file, as the editor hands over a tab
    /// not saved: drawn from the text, the file only its name.
    #[test]
    fn a_fold_is_drawn_from_the_text_given() {
        let work = std::env::temp_dir().join(format!("omaspec-fold-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let moving = |fold: &str| {
            Moving {
            file: work.join("never-saved.puml"),
            text: Some(
                "@startuml\n[*] --> Off\nstate Brewing {\n  [*] --> Grinding\n}\nOff --> Brewing : start\n@enduml\n"
                    .into(),
            ),
            output: work.join("fold.webp"),
            events: Vec::new(),
            steps: 0,
            width: Some(240),
            fps: Some(10),
            folds: vec![fold.to_owned()],
            dives: Vec::new(),
            builds: false,
                lockstep: false,
                history: false,
                travel: None,
                traffic: Vec::new(),
                themes: None,
                ink: false,
            whole: true,
            theme: Some("built-in".into()),
            file_colors: false,
            root: None,
            forced: Some(Language::PlantUml),
            renderer: Some("tiny-skia"),
            quality: super::super::gifs::Quality::default(),
        }
        };
        let told = play(&moving("Brewing")).unwrap();
        assert!(told.contains("Brewing folded and unfolded"), "{told}");
        let bytes = std::fs::read(work.join("fold.webp")).unwrap();
        let webp = image_webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
        assert!(webp.num_frames() >= 8, "{}", webp.num_frames());
        let err = play(&moving("Milk")).unwrap_err().to_string();
        assert_eq!(err, "Milk is no container to fold; the diagram has Brewing");
        let _ = std::fs::remove_dir_all(&work);
    }

    /// A fold as a page: both pictures in it, each with ids of its own,
    /// the glide between them as data, the source in the first, and no
    /// address the page would fetch. A play is refused there.
    #[test]
    fn a_fold_is_a_page_in_vectors() {
        let work = std::env::temp_dir().join(format!("omaspec-fold-page-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let text = "@startuml\n[*] --> Off\nstate Brewing {\n  [*] --> Grinding\n}\nOff --> Brewing : start\n@enduml\n";
        let moving = |folds: &[&str]| Moving {
            file: work.join("coffee.puml"),
            text: Some(text.into()),
            output: work.join("fold.html"),
            events: Vec::new(),
            steps: 0,
            width: None,
            fps: None,
            folds: folds.iter().map(|f| (*f).to_owned()).collect(),
            dives: Vec::new(),
            builds: false,
            lockstep: false,
            history: false,
            travel: None,
            traffic: Vec::new(),
            themes: None,
            ink: false,
            whole: false,
            theme: Some("built-in".into()),
            file_colors: false,
            root: None,
            forced: Some(Language::PlantUml),
            renderer: Some("tiny-skia"),
            quality: super::super::gifs::Quality::default(),
        };
        let told = play(&moving(&["Brewing"])).unwrap();
        assert!(
            told.contains("Brewing folded and unfolded in vectors"),
            "{told}"
        );
        let page = std::fs::read_to_string(work.join("fold.html")).unwrap();
        assert_eq!(page.matches("<svg ").count(), 2);
        assert!(page.contains(r#"id="p0-part-"#) && page.contains(r#"id="p1-part-"#));
        assert!(!page.contains(r#"id="part-"#), "an id two pictures share");
        assert!(page.contains("default-src 'none'"));
        let kept = omaspec_core::embed::source_from_svg(&page).unwrap();
        assert_eq!(kept.code, text);
        let json = page
            .split(r#"<script type="application/json" id="play">"#)
            .nth(1)
            .and_then(|rest| rest.split("</script>").next())
            .unwrap();
        let data: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(data["kind"], "fold");
        assert_eq!(data["names"][0], "Brewing");
        assert_eq!(data["sizes"].as_array().unwrap().len(), 2);
        let boxes = data["glides"][0]["boxes"].as_array().unwrap();
        // Brewing stays, another size; Grinding, inside it, leaves.
        let part = |name: &str| format!("p0-{}-", omaspec_uml::part_id(name));
        let of = |name: &str| {
            boxes
                .iter()
                .find(|b| b["parts"][0] == part(name).as_str())
                .unwrap_or_else(|| panic!("{name} in {boxes:?}"))
        };
        assert_eq!(of("state:Brewing")["role"], "stay");
        assert_ne!(of("state:Brewing")["from"], of("state:Brewing")["to"]);
        assert_eq!(of("state:Grinding")["role"], "leave");
        assert!(!data["glides"][0]["lines"].as_array().unwrap().is_empty());

        let err = play(&moving(&[])).unwrap_err().to_string();
        assert!(err.starts_with("a page plays a fold"), "{err}");
        let err = play(&moving(&["Milk"])).unwrap_err().to_string();
        assert_eq!(err, "Milk is no container to fold; the diagram has Brewing");
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
            text: None,
            output: work.join("never.gif"),
            events: events.iter().map(|e| (*e).to_owned()).collect(),
            steps: 8,
            width: None,
            fps: Some(10),
            folds: Vec::new(),
            dives: Vec::new(),
            builds: false,
            lockstep: false,
            history: false,
            travel: None,
            traffic: Vec::new(),
            themes: None,
            ink: false,
            whole: false,
            theme: Some("built-in".into()),
            file_colors: false,
            root: None,
            forced: None,
            renderer: Some("tiny-skia"),
            quality: super::super::gifs::Quality::default(),
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
