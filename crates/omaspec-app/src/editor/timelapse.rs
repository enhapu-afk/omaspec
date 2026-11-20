//! A diagram's history as a time-lapse (`omaspec play FILE --history -o
//! history.gif`): every commit that changed the file, oldest first, each
//! picture gliding into the next as the pane glides when the diagram is
//! edited (docs/adr/0029), what is new coming in and what went fading,
//! the commit under it. As `.html` a page in vectors that plays the same
//! glides ([`omaspec_render::export::folding`]).

use std::time::Duration;

use anyhow::{Context, Result, bail};
use iced::{Rectangle, Size};
use omaspec_core::Language;
use omaspec_render::{Place, Renderer};

use super::films::Film;
use super::moving::{FADE_IN_FRAMES, Moving, SETTLE, shown, theme};
use super::screen::{Caption, Screen, Shows};
use super::{App, Message, Start};
use crate::paper;

/// The commits taken at most, the newest.
const MOST: usize = 60;
/// Around the pane, and the band its caption takes under it.
const GAP: f32 = 14.0;
const CAPTION: f32 = 40.0;
const NARROWEST: f32 = 640.0;
const WIDEST: f32 = 1400.0;
/// How long each commit stays once it has come, at most and at least:
/// a long history goes through faster, to about [`BUDGET`] in all.
const LONGEST: Duration = Duration::from_millis(1_300);
const SHORTEST: Duration = Duration::from_millis(350);
const BUDGET: f32 = 24.0;
/// How long the last commit stays before the picture starts again.
const END: Duration = Duration::from_millis(2_400);

/// A version of the file, as a commit left it.
struct Version {
    /// `a7cddc5 · 2026-11-20 · fix: what the review found`.
    caption: String,
    text: String,
}

/// Writes the time-lapse of `args.file`; what it played.
pub(super) fn timelapse(args: &Moving, film: Film, fps: u32) -> Result<String> {
    let versions = versions(args)?;
    let language = args.forced.unwrap_or_else(|| {
        let (_, language) = args.named(&versions[versions.len() - 1].text);
        language
    });
    if film == Film::Page {
        return page(args, &versions, language);
    }
    let _clock = crate::clock::Recording::start();
    let mut stage = Stage::new(args, film, fps, &versions, language)?;
    stage.play(&versions, language)?;
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
        "{} → {} ({} frames, {}×{}, {} commits)",
        args.file.display(),
        args.output.display(),
        count,
        w,
        h,
        versions.len()
    ))
}

/// The file as each commit that changed it left it, oldest first; a
/// commit that left its text as it was is passed over.
fn versions(args: &Moving) -> Result<Vec<Version>> {
    let commits = crate::git::file_log(&args.file, Some(MOST), crate::names::MAX_FILE)
        .with_context(|| format!("{} has no history in git", args.file.display()))?;
    let mut versions: Vec<Version> = Vec::new();
    for commit in commits.into_iter().rev() {
        let Some(text) = commit.after.and_then(|after| after.text) else {
            continue;
        };
        if versions.last().is_some_and(|last| last.text == text) {
            continue;
        }
        versions.push(Version {
            caption: format!("{} · {} · {}", commit.short, commit.date, commit.subject),
            text,
        });
    }
    anyhow::ensure!(
        versions.len() >= 2,
        "{} has {} in git: a history needs two commits that changed it",
        args.file.display(),
        match versions.len() {
            0 => "no commit".to_owned(),
            _ => "one commit".to_owned(),
        }
    );
    Ok(versions)
}

/// How long each commit stays, for `count` of them.
fn stay(count: usize) -> Duration {
    let seconds =
        (BUDGET / count.max(1) as f32 - 0.6).clamp(SHORTEST.as_secs_f32(), LONGEST.as_secs_f32());
    Duration::from_secs_f32(seconds)
}

struct Stage {
    app: App,
    screen: Screen,
    frames: super::gifs::Frames,
    delay: u16,
    tick: Duration,
    key: u64,
    caption: Rectangle,
    file: std::path::PathBuf,
    root: Option<std::path::PathBuf>,
    /// The scale every commit is drawn at, and how high the newest is.
    zoom: f32,
    top: f32,
}

/// The picture on the pane at `zoom`, in the middle across, its top where
/// that of a picture `top` high is when it fits.
fn hold(app: &mut App, zoom: f32, top: f32) {
    let width = app.doc().paper.diagram_size().width;
    let view = &mut app.doc_mut().paper.view;
    view.zoom = zoom;
    view.center = iced::Point::new(width / 2.0, top / 2.0);
}

impl Stage {
    /// The pane in the size of the newest picture, the oldest on it.
    fn new(
        args: &Moving,
        film: Film,
        fps: u32,
        versions: &[Version],
        language: Language,
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
        let key = app.doc().key;
        let scaled = app.doc_mut().paper.set_scale_factor(1.0);
        screen.run_task(&mut app, scaled.map(move |m| Message::Paper(key, m)));
        let show = |app: &mut App, screen: &Screen, text: &str| {
            shown(
                app,
                screen,
                key,
                text,
                language,
                &args.file,
                args.root.as_deref(),
            )
        };
        // The newest picture says how large the pane is.
        show(&mut app, &screen, &versions[versions.len() - 1].text)?;
        let drawn = app.doc().paper.diagram_size();
        let width = match args.width {
            Some(width) => width as f32,
            None => (drawn.width * 1.6 + 2.0 * GAP)
                .clamp(NARROWEST, WIDEST)
                .round(),
        };
        let inner = width - 2.0 * GAP;
        let pane = (inner * drawn.height / drawn.width.max(1.0))
            .clamp(inner * 0.4, inner * 1.2)
            .round();
        let height = pane + CAPTION + 2.0 * GAP;
        app.doc_mut().paper.view.fit = true;
        let at = Rectangle::new(iced::Point::new(GAP, GAP), Size::new(inner, pane));
        screen.perform(
            &mut app,
            Message::Paper(key, paper::Message::Viewport(at.size())),
        );
        screen.size = Size::new(width, height);
        screen.shows = Shows::Panes;
        screen.panes = vec![(key, at)];
        let tick = Duration::from_micros(1_000_000 / u64::from(fps));
        // Every commit at the scale the newest fits at, its top where the
        // newest's is: the diagram grows as it grew.
        let zoom = app.doc().paper.view.zoom;
        app.doc_mut().paper.view.fit = false;
        // The oldest stands from the first frame kept.
        show(&mut app, &screen, &versions[0].text)?;
        hold(&mut app, zoom, drawn.height);
        screen.run_frames(
            &mut app,
            FADE_IN_FRAMES * 3,
            tick,
            |app| app.doc().paper.arrived(),
            |_| {},
        );
        Ok(Stage {
            app,
            screen,
            frames: super::gifs::Frames::default(),
            delay: match film {
                Film::Gif => u16::try_from((100 / fps).max(2)).unwrap_or(5),
                Film::Apng | Film::Webp | Film::Page => 1,
            },
            tick,
            key,
            caption: Rectangle::new(
                iced::Point::new(GAP, GAP + pane + 4.0),
                Size::new(inner, CAPTION - 4.0),
            ),
            file: args.file.clone(),
            root: args.root.clone(),
            zoom,
            top: drawn.height,
        })
    }

    /// Each commit glides into the next, the commit under it.
    fn play(&mut self, versions: &[Version], language: Language) -> Result<()> {
        let stay = stay(versions.len());
        for (at, version) in versions.iter().enumerate() {
            self.screen.captions = vec![Caption {
                at: self.caption,
                left: version.caption.clone(),
                right: format!("{} / {}", at + 1, versions.len()),
            }];
            if at > 0 {
                shown(
                    &mut self.app,
                    &self.screen,
                    self.key,
                    &version.text,
                    language,
                    &self.file,
                    self.root.as_deref(),
                )?;
                hold(&mut self.app, self.zoom, self.top);
                self.run(SETTLE, |app| app.doc().paper.arrived());
            }
            let last = at + 1 == versions.len();
            self.run(if last { END } else { stay }, |_| false);
        }
        Ok(())
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
}

/// Writes the time-lapse as a page in vectors: each commit's picture
/// drawn in the theme's colors with its parts marked, and the glide from
/// each to the next as the pane works it out. Nothing is drawn in pixels.
fn page(args: &Moving, versions: &[Version], language: Language) -> Result<String> {
    use omaspec_render::export::folding;

    let start = Start {
        files: vec![args.file.clone()],
        folder: None,
        forced: args.forced,
    };
    let (mut app, _) = App::themed(start, None, theme(args.theme.as_deref())?);
    app.follow_theme = !args.file_colors;
    let place = Place::of(&args.file, None);
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let mut maps = Vec::new();
    let mut pictures = Vec::new();
    for version in versions {
        let (source, _) = app.diagram_source_in(args.root.as_deref(), &version.text, language);
        let asked = crate::worker::folded(&source, language, &[], crate::worker::Views::default());
        let rendered = renderer
            .render_at(&asked, language, place.as_ref())
            .map_err(|err| anyhow::anyhow!("{}: {err}", version.caption))?;
        anyhow::ensure!(
            omaspec_uml::has_parts(&rendered.svg),
            "this kind of diagram does not glide by its parts yet: write the history as .webp, .png or .gif"
        );
        let size = omaspec_render::export::parse(&rendered.svg)
            .map_err(|err| anyhow::anyhow!("{err}"))?
            .size();
        pictures.push(folding::Picture {
            svg: rendered.svg,
            size: (size.width(), size.height()),
        });
        maps.push(rendered.map);
    }
    // Every commit at one scale, as the newest is drawn: each picture as
    // large as the largest, itself in the middle across and at the top,
    // and its glides moved with it.
    let most = pictures.iter().fold((1.0f32, 1.0f32), |(w, h), p| {
        (w.max(p.size.0), h.max(p.size.1))
    });
    let shifts: Vec<f32> = pictures.iter().map(|p| (most.0 - p.size.0) / 2.0).collect();
    let glides = maps
        .windows(2)
        .zip(pictures.windows(2))
        .zip(shifts.windows(2))
        .map(|((maps, pictures), shifts)| {
            let mut glide =
                paper::headless::fold_glide(&maps[0], &maps[1], pictures[0].size, pictures[1].size);
            shift(&mut glide, shifts[0], shifts[1]);
            glide
        })
        .collect();
    for (picture, &dx) in pictures.iter_mut().zip(&shifts) {
        let paper = super::building::picture::paper_of(&picture.svg);
        picture.svg = widened(&picture.svg, most, dx, &paper);
        picture.size = most;
    }
    let (paper, accent) = crate::look::palette_stage(&app.palette);
    let (name, _) = args.named(&versions[versions.len() - 1].text);
    let newest = &versions[versions.len() - 1].text;
    let written = folding::page(
        &folding::Folding {
            names: Vec::new(),
            captions: versions.iter().map(|v| v.caption.clone()).collect(),
            pictures,
            glides,
            paper,
            accent,
        },
        Some(omaspec_render::export::Source {
            code: newest,
            language,
            file: Some(&name),
        }),
        name.rsplit_once('.')
            .map_or(name.as_str(), |(stem, _)| stem),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    crate::files::write_atomic(&args.output, written.as_bytes())
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(format!(
        "{} → {} (a page, {} KB, {} commits in vectors)",
        args.file.display(),
        args.output.display(),
        written.len().div_ceil(1024),
        versions.len()
    ))
}

/// `glide` with what it says of the picture before moved `old` across,
/// and of the one after `new`.
fn shift(glide: &mut omaspec_render::export::folding::Glide, old: f32, new: f32) {
    let rect = |r: &mut [f32; 4], dx: f32| r[0] += dx;
    let route = |route: &mut omaspec_render::export::folding::Route, dx: f32| {
        for point in &mut route.points {
            point.0 += dx;
        }
        for head in &mut route.heads {
            for point in &mut head.points {
                point.0 += dx;
            }
        }
    };
    for moved in &mut glide.boxes {
        rect(&mut moved.from, old);
        rect(&mut moved.to, new);
        if let Some((was, is)) = &mut moved.into {
            rect(was, old);
            rect(is, new);
        }
    }
    for line in &mut glide.lines {
        if let Some(from) = &mut line.from {
            route(from, old);
        }
        if let Some(to) = &mut line.to {
            route(to, new);
        }
    }
}

/// `svg` as large as `size` in its own units, what it draws `dx` across
/// on its paper (`paper`) grown to the whole.
fn widened(svg: &str, (width, height): (f32, f32), dx: f32, paper: &str) -> String {
    let Some(start) = svg.find("<svg") else {
        return svg.to_owned();
    };
    let Some(end) = svg[start..].find('>').map(|end| start + end) else {
        return svg.to_owned();
    };
    let Some(close) = svg.rfind("</svg>") else {
        return svg.to_owned();
    };
    let tag = &svg[start..end];
    let number = |name: &str| -> Option<f32> {
        tag.split(&format!(" {name}=\""))
            .nth(1)?
            .split('"')
            .next()?
            .trim_end_matches("px")
            .parse()
            .ok()
    };
    let view: Vec<f32> = tag
        .split(" viewBox=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .map(|view| {
            view.split(|c: char| c == ',' || c.is_whitespace())
                .filter_map(|n| n.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    let (x, y, w, h) = match view[..] {
        [x, y, w, h] if w > 0.0 && h > 0.0 => (x, y, w, h),
        _ => (
            0.0,
            0.0,
            number("width").unwrap_or(1.0),
            number("height").unwrap_or(1.0),
        ),
    };
    // A `scale` draws a unit larger or smaller than one: it stays so.
    let (kx, ky) = (
        number("width").map_or(1.0, |shown| shown / w),
        number("height").map_or(1.0, |shown| shown / h),
    );
    let mut root = tag.to_owned();
    for (name, value) in [
        ("viewBox", format!("{x} {y} {width} {height}")),
        ("width", format!("{}", width * kx)),
        ("height", format!("{}", height * ky)),
    ] {
        let attr = format!(" {name}=\"");
        root = match root.find(&attr) {
            Some(at) => {
                let from = at + attr.len();
                let to = from + root[from..].find('"').unwrap_or(0);
                format!("{}{value}{}", &root[..from], &root[to..])
            }
            None => format!("{root}{attr}{value}\""),
        };
    }
    format!(
        "{}{root}><rect x=\"{x}\" y=\"{y}\" width=\"{width}\" height=\"{height}\" fill=\"{paper}\"/><g transform=\"translate({dx} 0)\">{}</g>{}",
        &svg[..start],
        &svg[end + 1..close],
        &svg[close..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_widened_to_the_largest_with_itself_in_the_middle() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><rect width="40" height="20" fill="#111"/><g id="part-61-1"/></svg>"##;
        let wide = widened(svg, (100.0, 50.0), 30.0, "#111");
        assert!(wide.contains(r#"viewBox="0 0 100 50""#), "{wide}");
        assert!(wide.contains(r#"width="100""#) && wide.contains(r#"height="50""#));
        assert!(
            wide.contains(r#"<g transform="translate(30 0)"><rect width="40""#),
            "{wide}"
        );
        assert!(wide.ends_with("</g></svg>"));
        let mut glide = omaspec_render::export::folding::Glide {
            boxes: vec![omaspec_render::export::folding::Moved {
                parts: ["a".into(), "a".into()],
                from: [1.0, 2.0, 3.0, 4.0],
                to: [1.0, 2.0, 3.0, 4.0],
                role: omaspec_render::export::folding::Role::Stay,
                into: None,
            }],
            lines: Vec::new(),
        };
        shift(&mut glide, 10.0, 5.0);
        assert_eq!(glide.boxes[0].from[0], 11.0);
        assert_eq!(glide.boxes[0].to[0], 6.0);
    }

    /// A file of three commits, one of them leaving it as it was, played
    /// as a GIF and as a page; a file with one commit is refused.
    #[test]
    fn a_history_glides_from_commit_to_commit() {
        let work = std::env::temp_dir().join(format!("omaspec-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).unwrap();
        let git = |args: &[&str]| {
            let done = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=Omaspec",
                    "-c",
                    "user.email=omaspec@example.org",
                ])
                .args(args)
                .current_dir(&work)
                .output()
                .unwrap();
            assert!(
                done.status.success(),
                "{}",
                String::from_utf8_lossy(&done.stderr)
            );
        };
        git(&["init", "-q"]);
        let file = work.join("door.puml");
        let commit = |text: &str, message: &str| {
            std::fs::write(&file, text).unwrap();
            std::fs::write(work.join("notes.txt"), message).unwrap();
            git(&["add", "-A"]);
            git(&["commit", "-q", "-m", message]);
        };
        commit("@startuml\n[*] --> Closed\n@enduml\n", "feat: a door");
        let moving = |output: &str| Moving {
            file: file.clone(),
            text: None,
            output: work.join(output),
            events: Vec::new(),
            steps: 100,
            width: Some(320),
            fps: Some(4),
            folds: Vec::new(),
            dives: Vec::new(),
            builds: false,
            lockstep: false,
            history: true,
            whole: false,
            theme: Some("built-in".into()),
            file_colors: false,
            root: None,
            forced: None,
            renderer: Some("tiny-skia"),
            quality: super::super::gifs::Quality::default(),
        };
        let refused = super::super::moving::play(&moving("one.gif"))
            .unwrap_err()
            .to_string();
        assert!(refused.contains("one commit"), "{refused}");
        commit(
            "@startuml\n[*] --> Closed\nClosed --> Open : open\n@enduml\n",
            "feat: it opens",
        );
        // Only the notes change: the door stays as it was.
        commit(
            "@startuml\n[*] --> Closed\nClosed --> Open : open\n@enduml\n",
            "docs: notes",
        );
        commit(
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
            "feat: and closes",
        );
        let told = super::super::moving::play(&moving("door.gif")).unwrap();
        assert!(told.contains("3 commits"), "{told}");
        let gif = std::fs::read(work.join("door.gif")).unwrap();
        assert_eq!(u16::from_le_bytes([gif[6], gif[7]]), 320);
        let told = super::super::moving::play(&moving("door.html")).unwrap();
        assert!(
            told.contains("a page") && told.contains("3 commits"),
            "{told}"
        );
        let page = std::fs::read_to_string(work.join("door.html")).unwrap();
        assert!(page.contains(r#""kind":"history""#));
        assert!(page.contains("feat: and closes"));
        assert!(!page.contains("docs: notes"));
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn a_long_history_goes_through_faster() {
        let near = |a: Duration, b: Duration| (a.as_secs_f32() - b.as_secs_f32()).abs() < 1e-3;
        assert!(near(stay(2), LONGEST));
        assert!(stay(30) < LONGEST);
        assert!(near(stay(200), SHORTEST));
    }
}
