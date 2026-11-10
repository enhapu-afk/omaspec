//! A play as a picture that moves: `omaspec play FILE -o play.gif`
//! draws the diagram's pane headless, as the window draws it, runs the
//! play on it and writes the frames as a GIF ([`super::gifs`]), for a
//! presentation or a pull request.
//!
//! The pane is the editor's own ([`super::screen`]), drawn on the GPU
//! without a window, so the light along a transition is the one the
//! window shows. The events are taken by name (`--events open,close`;
//! with its guard, `"start [tank empty]"`, where two share a name),
//! else the first the machine offers at each state, for `--steps` steps.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use iced::Size;
use omaspec_core::{Language, detect};
use omaspec_render::{Place, Renderer};
use omaspec_theme::Theme as Omarchy;

use super::screen::{Screen, Shot, Shows};
use super::{App, Command, Message, Start};
use crate::paper::{self, Ask, headless};
use crate::worker::Drawn;

/// What the command line asks for.
#[derive(Debug, Clone)]
pub struct Moving {
    pub file: PathBuf,
    pub output: PathBuf,
    /// The events to take, in order; empty takes the first offered.
    pub events: Vec<String>,
    /// How many steps at most.
    pub steps: usize,
    /// The picture's width in pixels at most; a small diagram takes less,
    /// drawn twice its size, and the height follows the diagram.
    pub width: u32,
    pub fps: u32,
    /// The colors: `None` the active theme's, `built-in` the palette
    /// Omaspec ships, else a theme's `colors.toml`.
    pub theme: Option<String>,
    pub forced: Option<Language>,
}

/// How long the play holds after each step, and at the end.
const HOLD: Duration = Duration::from_millis(500);
/// How long a step may take to settle before the next.
const SETTLE: Duration = Duration::from_millis(2_500);
/// The pane's height is between these shares of its width.
const LEAST_HEIGHT: f32 = 0.3;
const MOST_HEIGHT: f32 = 1.4;
/// Around the diagram, and below it for the bar of the play.
const MARGIN: f32 = 40.0;
const BAR: f32 = 90.0;

/// Writes the play; says what it wrote.
pub fn play(args: &Moving) -> Result<String> {
    anyhow::ensure!(args.fps >= 1 && args.fps <= 50, "--fps takes 1 to 50");
    anyhow::ensure!(
        args.width >= 200 && args.width <= 4000,
        "--width takes 200 to 4000"
    );
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

    // The editor, headless, in the active theme; the diagram's pane the
    // size asked for, the height from the diagram once it is drawn.
    let start = Start {
        files: vec![args.file.clone()],
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
    shown(&mut app, key, &text, language, &args.file)?;
    // Twice the diagram's size, with room for the bar of the play, up to
    // the width asked for.
    let drawn = app.doc().paper.diagram_size();
    let width = (drawn.width * 2.0 + 2.0 * MARGIN)
        .min(args.width as f32)
        .max(320.0)
        .round();
    let height = (width * drawn.height / drawn.width.max(1.0) + BAR)
        .clamp(width * LEAST_HEIGHT, width * MOST_HEIGHT)
        .round();
    let size = Size::new(width, height);
    app.doc_mut().paper.view.fit = true;
    let _ = app.doc_mut().paper.update(paper::Message::Viewport(size));
    app.doc_mut().paper.finish_rasters();

    let mut screen = Screen::new(&app, 1.0).map_err(|why| anyhow::anyhow!(why))?;
    screen.size = size;
    screen.shows = Shows::Diagram;
    // The first frame starts uploading the pictures; they are on the GPU
    // a moment later.
    let _ = screen.shoot(&app);

    let tick = Duration::from_micros(1_000_000 / u64::from(args.fps));
    let mut frames: Vec<Shot> = Vec::new();
    let mut run = |app: &mut App, most: Duration, done: &dyn Fn(&App) -> bool| {
        let count = (most.as_secs_f32() / tick.as_secs_f32()).ceil() as usize;
        screen.run_frames(app, count.max(1), tick, done, |shot| {
            frames.push(Shot {
                width: shot.width,
                height: shot.height,
                rgba: shot.rgba.clone(),
            });
        });
    };
    let moving = |app: &App| !app.doc().paper.play_moving(Instant::now());

    let _ = app.update(Message::Command(Command::Play));
    if !app.doc().paper.playing() {
        bail!(
            "{}",
            app.notice
                .clone()
                .unwrap_or_else(|| "the diagram cannot be played".into())
        );
    }
    run(&mut app, SETTLE, &moving);
    run(&mut app, HOLD, &|_| false);
    // The picture stays where it is from here on: in the window a fitted
    // one glides to its new fit when the bar takes a second line, which
    // in a picture that loops reads as a wobble.
    app.doc_mut().paper.view.fit = false;
    let mut taken = Vec::new();
    for step in 0..args.steps {
        let Some(paper::Offer::Machine { options, ended, .. }) = app.doc().paper.offer() else {
            // A scenario: its next message.
            let _ = app.update(Message::Command(Command::PlayAsk(Ask::Next)));
            if !app.doc().paper.playing() {
                break;
            }
            run(&mut app, SETTLE, &moving);
            run(&mut app, HOLD, &|_| false);
            continue;
        };
        if ended || options.is_empty() {
            break;
        }
        let choice = match args.events.get(step) {
            Some(wanted) => options
                .iter()
                .position(|(label, _)| label.trim() == wanted.trim())
                .or_else(|| {
                    options
                        .iter()
                        .position(|(label, _)| event_of(label) == wanted.trim())
                })
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
                })?,
            None if !args.events.is_empty() => break,
            None => 0,
        };
        taken.push(options[choice].0.trim().to_owned());
        let _ = app.update(Message::Command(Command::PlayAsk(Ask::Take(choice))));
        run(&mut app, SETTLE, &moving);
        run(&mut app, HOLD, &|_| false);
    }
    // The picture ends on the state the play reached, held a moment
    // longer, and loops from there: the play's leaving is not in it.
    run(&mut app, HOLD, &|_| false);

    let Some(first) = frames.first() else {
        bail!("no frame was drawn");
    };
    let (w, h) = (
        u16::try_from(first.width).context("too wide for a GIF")?,
        u16::try_from(first.height).context("too high for a GIF")?,
    );
    // A frame like the one before it only makes that one stay longer.
    let delay = u16::try_from((100 / args.fps).max(2)).unwrap_or(5);
    let mut kept: Vec<(Vec<u8>, u16)> = Vec::new();
    for frame in frames {
        match kept.last_mut() {
            Some((last, stay)) if *last == frame.rgba => *stay = stay.saturating_add(delay),
            _ => kept.push((frame.rgba, delay)),
        }
    }
    let shown_frames = kept.len();
    let bytes = super::gifs::encode(w, h, &kept)?;
    crate::files::write_atomic(&args.output, &bytes)
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(format!(
        "{} → {} ({} frames, {}×{}, {})",
        args.file.display(),
        args.output.display(),
        shown_frames,
        w,
        h,
        if taken.is_empty() {
            "played".to_owned()
        } else {
            taken.join(", ")
        }
    ))
}

/// The event of an option's label, without its guard and its action:
/// `start [tank empty] / beep` offers `start`, `power on` is `power on`.
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

/// Draws `text` as the pane shows it, in the theme's colors, here and
/// now.
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
        .render_at(&source, language, place.as_ref())
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
    use super::event_of;

    #[test]
    fn an_event_is_its_label_without_guard_and_action() {
        assert_eq!(event_of("open"), "open");
        assert_eq!(event_of("power on"), "power on");
        assert_eq!(event_of("start [tank empty]"), "start");
        assert_eq!(event_of("pay / beep"), "pay");
    }
}
