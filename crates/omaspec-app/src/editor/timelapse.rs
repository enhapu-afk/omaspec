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
    /// The language it is in: a file renamed from `.mmd` to `.puml` was
    /// Mermaid before.
    language: Language,
}

/// Writes the time-lapse of `args.file`; what it played.
pub(super) fn timelapse(args: &Moving, film: Film, fps: u32) -> Result<String> {
    let versions = versions(args)?;
    if film == Film::Page {
        return page(args, versions);
    }
    let _clock = crate::clock::Recording::start();
    let mut stage = Stage::new(args, film, fps, versions)?;
    stage.play()?;
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
        "{} → {} ({} frames, {}×{}, {} commits{})",
        args.file.display(),
        args.output.display(),
        count,
        w,
        h,
        stage.versions.len(),
        passed_over(stage.passed)
    ))
}

/// What the summary says of the commits that left the diagram broken.
fn passed_over(passed: usize) -> String {
    match passed {
        0 => String::new(),
        1 => ", 1 passed over: it does not draw".to_owned(),
        n => format!(", {n} passed over: they do not draw"),
    }
}

/// Whether `kept` versions that draw make a history; `passed` did not draw.
fn enough(args: &Moving, kept: usize, passed: usize) -> Result<()> {
    anyhow::ensure!(
        kept >= 2,
        "{}: {kept} of its {} versions in git draw{}, and a history needs two",
        args.file.display(),
        kept + passed,
        if passed > 0 {
            " (a commit left it broken)"
        } else {
            ""
        }
    );
    Ok(())
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
        let (_, language) = args.named(&text);
        versions.push(Version {
            caption: format!("{} · {} · {}", commit.short, commit.date, commit.subject),
            text,
            language,
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
    /// The versions that draw, and how many did not.
    versions: Vec<Version>,
    passed: usize,
    /// The scale every commit is drawn at, and how high the highest is.
    zoom: f32,
    top: f32,
}

/// The picture on the pane at `zoom`, in the middle across, its top where
/// that of a picture `top` high is when it fits.
fn hold(app: &mut App, zoom: f32, top: f32) {
    let paper = &mut app.doc_mut().paper;
    let width = paper.diagram_size().width;
    paper.hold_view(zoom, iced::Point::new(width / 2.0, top / 2.0));
}

impl Stage {
    /// The pane in the size of the largest picture, the oldest on it. A
    /// version that does not draw (a commit left it broken) is passed over.
    fn new(args: &Moving, film: Film, fps: u32, versions: Vec<Version>) -> Result<Self> {
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
        let show = |app: &mut App, screen: &Screen, version: &Version| {
            shown(
                app,
                screen,
                key,
                &version.text,
                version.language,
                &args.file,
                args.root.as_deref(),
            )
        };
        // Each version drawn once while the pane has no size, so nothing
        // is rasterized: which draw, and how large the largest is.
        let mut kept: Vec<Version> = Vec::new();
        let mut passed = 0;
        let mut most = Size::new(1.0f32, 1.0f32);
        for version in versions {
            if kept.last().is_some_and(|last| last.text == version.text) {
                continue;
            }
            if show(&mut app, &screen, &version).is_err() {
                passed += 1;
                continue;
            }
            let drawn = app.doc().paper.diagram_size();
            most = Size::new(most.width.max(drawn.width), most.height.max(drawn.height));
            kept.push(version);
        }
        enough(args, kept.len(), passed)?;
        let width = match args.width {
            Some(width) => width as f32,
            None => (most.width * 1.6 + 2.0 * GAP)
                .clamp(NARROWEST, WIDEST)
                .round(),
        };
        let inner = width - 2.0 * GAP;
        let pane = (inner * most.height / most.width.max(1.0))
            .clamp(inner * 0.4, inner * 1.2)
            .round();
        let height = pane + CAPTION + 2.0 * GAP;
        app.doc_mut().paper.view.fit = false;
        let at = Rectangle::new(iced::Point::new(GAP, GAP), Size::new(inner, pane));
        screen.perform(
            &mut app,
            Message::Paper(key, paper::Message::Viewport(at.size())),
        );
        screen.size = Size::new(width, height);
        screen.shows = Shows::Panes;
        screen.panes = vec![(key, at)];
        let tick = Duration::from_micros(1_000_000 / u64::from(fps));
        // Every commit at the scale the largest fits at, its top where the
        // largest's is: the diagram grows as it grew, and none is cut.
        let zoom = crate::paper::View::fitted(most, at.size()).zoom;
        hold(&mut app, zoom, most.height);
        // The oldest stands from the first frame kept.
        show(&mut app, &screen, &kept[0])?;
        hold(&mut app, zoom, most.height);
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
            versions: kept,
            passed,
            zoom,
            top: most.height,
        })
    }

    /// Each commit glides into the next, the commit under it.
    fn play(&mut self) -> Result<()> {
        let versions = std::mem::take(&mut self.versions);
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
                    version.language,
                    &self.file,
                    self.root.as_deref(),
                )?;
                hold(&mut self.app, self.zoom, self.top);
                self.run(SETTLE, |app| app.doc().paper.arrived());
            }
            let last = at + 1 == versions.len();
            self.run(if last { END } else { stay }, |_| false);
        }
        self.versions = versions;
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
fn page(args: &Moving, versions: Vec<Version>) -> Result<String> {
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
    // A version that does not draw (a commit left it broken) is passed
    // over.
    let mut kept: Vec<Version> = Vec::new();
    let mut passed = 0;
    for version in versions {
        if kept.last().is_some_and(|last| last.text == version.text) {
            continue;
        }
        let language = version.language;
        let (source, _) = app.diagram_source_in(args.root.as_deref(), &version.text, language);
        let asked = crate::worker::folded(&source, language, &[], crate::worker::Views::default());
        let Ok(rendered) = renderer.render_at(&asked, language, place.as_ref()) else {
            passed += 1;
            continue;
        };
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
        kept.push(version);
    }
    enough(args, kept.len(), passed)?;
    let versions = kept;
    // Every commit at one scale, as the newest is drawn: each picture as
    // large as the largest, itself in the middle across and at the top,
    // and its glides moved with it; all in the pictures' own units, which
    // the page draws in.
    let units: Vec<Units> = pictures.iter().map(|p| Units::of(&p.svg, p.size)).collect();
    let own: Vec<(f32, f32)> = pictures
        .iter()
        .zip(&units)
        .map(|(p, u)| u.size(p.size))
        .collect();
    let most = own
        .iter()
        .fold((1.0f32, 1.0f32), |(w, h), s| (w.max(s.0), h.max(s.1)));
    let shifts: Vec<f32> = own.iter().map(|s| (most.0 - s.0) / 2.0).collect();
    let glides = maps
        .windows(2)
        .enumerate()
        .map(|(at, maps)| {
            let mut glide = paper::headless::fold_glide(
                &maps[0],
                &maps[1],
                pictures[at].size,
                pictures[at + 1].size,
            );
            remap(
                &mut glide,
                units[at].moved(shifts[at]),
                units[at + 1].moved(shifts[at + 1]),
            );
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
    let language = versions[versions.len() - 1].language;
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
        "{} → {} (a page, {} KB, {} commits in vectors{})",
        args.file.display(),
        args.output.display(),
        written.len().div_ceil(1024),
        versions.len(),
        passed_over(passed)
    ))
}

/// How the pixels of a picture's size, which its map is in, become its
/// own units (its `viewBox`), which a page draws its glides in: a `scale`
/// sets the two apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Units {
    sx: f32,
    sy: f32,
    tx: f32,
    ty: f32,
}

impl Units {
    /// Those of `svg`, `size` pixels large.
    pub(super) fn of(svg: &str, size: (f32, f32)) -> Units {
        let root = super::building::picture::root_of(svg, size);
        let back = root.invert().unwrap_or_default();
        Units {
            sx: back.sx,
            sy: back.sy,
            tx: back.tx,
            ty: back.ty,
        }
    }

    /// These, then `dx` across.
    fn moved(self, dx: f32) -> Units {
        Units {
            tx: self.tx + dx,
            ..self
        }
    }

    /// How large `size` pixels are in units.
    pub(super) fn size(self, (w, h): (f32, f32)) -> (f32, f32) {
        (w * self.sx, h * self.sy)
    }

    fn point(self, (x, y): (f32, f32)) -> (f32, f32) {
        (x * self.sx + self.tx, y * self.sy + self.ty)
    }

    fn rect(self, r: &mut [f32; 4]) {
        let (x, y) = self.point((r[0], r[1]));
        *r = [x, y, r[2] * self.sx, r[3] * self.sy];
    }
}

/// `glide`, made in pixels, in units: what it says of the picture before
/// in `old`'s, of the one after in `new`'s.
pub(super) fn remap(glide: &mut omaspec_render::export::folding::Glide, old: Units, new: Units) {
    let route = |route: &mut omaspec_render::export::folding::Route, units: Units| {
        for point in &mut route.points {
            *point = units.point(*point);
        }
        for head in &mut route.heads {
            for point in &mut head.points {
                *point = units.point(*point);
            }
        }
    };
    for moved in &mut glide.boxes {
        old.rect(&mut moved.from);
        new.rect(&mut moved.to);
        if let Some((was, is)) = &mut moved.into {
            old.rect(was);
            new.rect(is);
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
pub(super) fn widened(svg: &str, (width, height): (f32, f32), dx: f32, paper: &str) -> String {
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
        let plain = Units {
            sx: 1.0,
            sy: 1.0,
            tx: 0.0,
            ty: 0.0,
        };
        remap(&mut glide, plain.moved(10.0), plain.moved(5.0));
        assert_eq!(glide.boxes[0].from[0], 11.0);
        assert_eq!(glide.boxes[0].to[0], 6.0);
        // A picture drawn twice its units: its pixels halved.
        let scaled = r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40" viewBox="0 0 40 20"></svg>"##;
        let units = Units::of(scaled, (80.0, 40.0));
        assert_eq!(units.size((80.0, 40.0)), (40.0, 20.0));
        remap(&mut glide, units, units);
        assert_eq!(glide.boxes[0].from, [5.5, 1.0, 1.5, 2.0]);
    }

    /// A file of five commits, one of them leaving it as it was and one
    /// leaving it broken, played as a GIF and as a page; a file with one
    /// commit is refused.
    #[test]
    fn a_history_glides_from_commit_to_commit() {
        if std::process::Command::new("git")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipped: no git");
            return;
        }
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
                    "-c",
                    "commit.gpgsign=false",
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
            traffic: Vec::new(),
            themes: None,
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
        // A commit that leaves the door broken is passed over.
        commit(
            "@startuml\n[*] --> Closed\nClosed --> : open\n@enduml\n",
            "wip: half a transition",
        );
        commit(
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
            "feat: and closes",
        );
        let told = super::super::moving::play(&moving("door.gif")).unwrap();
        assert!(
            told.contains("3 commits, 1 passed over: it does not draw"),
            "{told}"
        );
        let gif = std::fs::read(work.join("door.gif")).unwrap();
        assert_eq!(u16::from_le_bytes([gif[6], gif[7]]), 320);
        let told = super::super::moving::play(&moving("door.html")).unwrap();
        assert!(
            told.contains("a page") && told.contains("3 commits in vectors, 1 passed over"),
            "{told}"
        );
        let page = std::fs::read_to_string(work.join("door.html")).unwrap();
        assert!(page.contains(r#""kind":"history""#));
        assert!(page.contains("feat: and closes"));
        assert!(!page.contains("docs: notes"));
        assert!(!page.contains("wip: half a transition"));
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
