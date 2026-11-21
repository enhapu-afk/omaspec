//! A theme sweep (`omaspec play FILE --themes -o sweep.gif`): one diagram
//! through every bundled Omarchy theme, each theme's colors sweeping over
//! the one before from left to right, the theme's name under it. The
//! diagram is drawn in each theme as the pane draws it there; the frames
//! are mixed in pixels without a GPU, and as `.html` a page sweeps the
//! same pictures in vectors ([`omaspec_render::export::sweep`]).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use omaspec_core::Language;
use omaspec_render::export;
use omaspec_render::export::tiny_skia;
use omaspec_render::{Place, Renderer};

use super::films::Film;
use super::gifs::Frames;
use super::moving::{Moving, theme};
use super::{App, Start};

/// Where Omarchy keeps its themes, and where a writer keeps their own.
const BUNDLED: &str = "/usr/share/omarchy/themes";
const OWN: &str = ".config/omarchy/themes";
/// How high the caption's band is, in pixels.
const CAPTION: f32 = 44.0;
/// How long a sweep takes, how soft its edge is (of the width), and how
/// long each theme stays at most and at least: many go faster, to about
/// [`BUDGET`] in all.
const SWEEP: f32 = 0.9;
const EDGE: f32 = 0.12;
const LONGEST: f32 = 1.2;
const SHORTEST: f32 = 0.35;
const BUDGET: f32 = 26.0;
/// The picture's width where none is asked for, at least and at most.
const NARROWEST: f32 = 480.0;
const WIDEST: f32 = 1200.0;

/// A theme the diagram is drawn in.
struct Themed {
    /// As its caption says it: `Tokyo Night`.
    name: String,
    /// The picture, and its paper, words and quiet words.
    svg: String,
    paper: String,
    ink: String,
    muted: String,
}

/// Writes the sweep of `args.file` through `names` (every bundled theme
/// where none are named); what it swept.
pub(super) fn sweep(args: &Moving, names: &[String], film: Film, fps: u32) -> Result<String> {
    let files = theme_files(names)?;
    anyhow::ensure!(
        files.len() >= 2,
        "a sweep needs two themes or more; {} found",
        files.len()
    );
    let text = args.read()?;
    let (name, language) = args.named(&text);
    let themed: Vec<Themed> = files
        .iter()
        .map(|file| draw(args, &text, language, file))
        .collect::<Result<_>>()?;
    let told = format!("{} themes", themed.len());
    if film == Film::Page {
        let written = page(args, &text, &name, language, &themed)?;
        return Ok(format!(
            "{} → {} (a page, {} KB, {told} in vectors)",
            args.file.display(),
            args.output.display(),
            written.div_ceil(1024)
        ));
    }
    let size = export::parse(&themed[0].svg)
        .map_err(|err| anyhow::anyhow!("{err}"))?
        .size();
    let width = match args.width {
        Some(width) => width as f32,
        None => (size.width() * 1.6).clamp(NARROWEST, WIDEST).round(),
    };
    let scale = width / size.width().max(1.0);
    let count = themed.len();
    // Only the theme shown, the one sweeping in and the first are held as
    // pixels: many themes of a large picture would not fit at once.
    let first = still(&themed[0], 0, count, scale)?;
    let (w, h) = (first.width(), first.height());
    let delay = |seconds: f32| match film {
        Film::Gif => (seconds * 100.0).round().clamp(2.0, 65535.0) as u16,
        Film::Apng | Film::Webp | Film::Page => (seconds * fps as f32).round().max(1.0) as u16,
    };
    let frame = match film {
        Film::Gif => u16::try_from((100 / fps).max(2)).unwrap_or(5),
        Film::Apng | Film::Webp | Film::Page => 1,
    };
    let stay = stay(count);
    let mut frames = Frames::default();
    let sweeps = (SWEEP * fps as f32).ceil() as usize;
    // Each theme a while, then the next sweeps over it; the last sweeps
    // into the first, so the picture loops as one.
    let mut shown: Option<tiny_skia::Pixmap> = None;
    for at in 0..count {
        let coming = (at + 1 < count)
            .then(|| still(&themed[at + 1], at + 1, count, scale))
            .transpose()?;
        let (this, next) = (
            shown.as_ref().unwrap_or(&first),
            coming.as_ref().unwrap_or(&first),
        );
        anyhow::ensure!(
            next.width() == w && next.height() == h,
            "the diagram is drawn in another size in some theme: a sweep needs one size"
        );
        frames.push(w as usize, h as usize, this.data(), delay(stay));
        for k in 1..sweeps {
            let mixed = swept(this, next, k as f32 / sweeps as f32);
            frames.push(w as usize, h as usize, &mixed, frame);
        }
        shown = coming;
    }
    super::films::timed(
        &mut frames,
        args.timing,
        film,
        fps,
        super::gifs::Rest::First,
    )?;
    let (bytes, count) = match film {
        Film::Gif => frames.encode(args.quality)?,
        Film::Apng => super::films::apng(&frames, fps)?,
        Film::Webp => super::films::webp(&frames, fps)?,
        Film::Page => unreachable!("a page is written before a frame is drawn"),
    };
    crate::files::write_atomic(&args.output, &bytes)
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(format!(
        "{} → {} ({} frames, {}×{}, {told})",
        args.file.display(),
        args.output.display(),
        count,
        w,
        h
    ))
}

/// How long each theme stays, for `count` of them.
fn stay(count: usize) -> f32 {
    (BUDGET / count.max(1) as f32 - SWEEP).clamp(SHORTEST, LONGEST)
}

/// The `colors.toml` of each theme `names` names: a file, `built-in`,
/// or a theme's name, looked for among Omarchy's and the writer's own;
/// every bundled theme where none is named.
fn theme_files(names: &[String]) -> Result<Vec<String>> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dirs: Vec<PathBuf> = [Some(PathBuf::from(BUNDLED)), home.map(|h| h.join(OWN))]
        .into_iter()
        .flatten()
        .collect();
    // `--themes "tokyo-night, gruvbox"` names two themes.
    let names: Vec<&str> = names
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    if names.is_empty() {
        // By their names, as Omarchy lists them.
        let mut found: Vec<(String, String)> = std::fs::read_dir(BUNDLED)
            .with_context(|| format!("no bundled themes in {BUNDLED}: name the themes, as --themes a/colors.toml,b/colors.toml"))?
            .flatten()
            .map(|entry| (entry.file_name().to_string_lossy().into_owned(), entry.path().join("colors.toml")))
            .filter(|(_, file)| file.is_file())
            .map(|(name, file)| (name, file.to_string_lossy().into_owned()))
            .collect();
        found.sort();
        return Ok(found.into_iter().map(|(_, file)| file).collect());
    }
    names
        .iter()
        .map(|&name| {
            if name == "built-in" || Path::new(name).is_file() {
                return Ok(name.to_owned());
            }
            dirs.iter()
                .map(|dir| dir.join(name).join("colors.toml"))
                .find(|file| file.is_file())
                .map(|file| file.to_string_lossy().into_owned())
                .with_context(|| {
                    format!("{name} is no theme: a colors.toml, built-in, or a theme in {BUNDLED} or ~/{OWN}")
                })
        })
        .collect()
}

/// The diagram drawn in the theme of `file`, as the pane draws it there.
fn draw(args: &Moving, text: &str, language: Language, file: &str) -> Result<Themed> {
    // Its own words: the shared one speaks of `--theme`.
    let omarchy = theme(Some(file))
        .map_err(|err| anyhow::anyhow!("{file} is no theme: {}", err.root_cause()))?;
    let name = shown_name(omarchy.name.as_deref().unwrap_or(file));
    let start = Start {
        files: vec![args.file.clone()],
        folder: None,
        forced: args.forced,
    };
    let (mut app, _) = App::themed(start, None, omarchy);
    app.follow_theme = !args.file_colors;
    let skin = app.pane_skin(args.root.as_deref());
    let place = Place::of(&args.file, None);
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let mut maps = crate::specs::Maps::default();
    let rendered = crate::look::render(
        &mut maps,
        &mut renderer,
        text,
        language,
        Some(&args.file),
        place.as_ref(),
        skin.as_ref(),
    )
    .map_err(|err| anyhow::anyhow!("{name}: {err}"))?;
    let p = &app.palette;
    Ok(Themed {
        name,
        paper: super::building::picture::paper_of(&rendered.svg),
        ink: p.foreground.to_string(),
        muted: p.muted.to_string(),
        svg: rendered.svg,
    })
}

/// A theme's name as a caption says it: `tokyo-night` is `Tokyo Night`.
fn shown_name(name: &str) -> String {
    name.split(['-', '_', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut letters = word.chars();
            letters.next().map_or_else(String::new, |first| {
                first.to_uppercase().collect::<String>() + letters.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The picture of `one` with its caption under it, the theme `at` of
/// `count`, rasterized at `scale`.
fn still(one: &Themed, at: usize, count: usize, scale: f32) -> Result<tiny_skia::Pixmap> {
    let svg = captioned(one, at, count, scale);
    let tree = export::parse(&svg).map_err(|err| anyhow::anyhow!("{err}"))?;
    export::rasterize(&tree, scale).map_err(|err| anyhow::anyhow!("{err}"))
}

/// `one`'s picture with a band under it that names its theme and says
/// which of `count` it is, in the theme's own colors.
fn captioned(one: &Themed, at: usize, count: usize, scale: f32) -> String {
    let (x, y, w, h) = view_of(&one.svg);
    let size = export::parse(&one.svg).map_or(w, |tree| tree.size().width());
    // Pixels of the frame in the picture's own units.
    let px = |pixels: f32| pixels / (scale * size / w.max(1.0));
    let band = px(CAPTION);
    let wide = super::timelapse::widened(&one.svg, (w, h + band), 0.0, &one.paper);
    let line = y + h + band / 2.0 + px(5.0);
    let words = format!(
        r#"<text x="{:.2}" y="{line:.2}" font-family="sans-serif" font-size="{:.2}" font-weight="bold" fill="{}">{}</text><text x="{:.2}" y="{line:.2}" font-family="sans-serif" font-size="{:.2}" text-anchor="end" fill="{}">{} / {count}</text>"#,
        x + px(14.0),
        px(15.0),
        one.ink,
        xml(&one.name),
        x + w - px(14.0),
        px(13.0),
        one.muted,
        at + 1,
    );
    let close = wide.rfind("</svg>").unwrap_or(wide.len());
    format!("{}{words}{}", &wide[..close], &wide[close..])
}

/// `next` swept over `this` from the left, `k` of the way, its edge soft.
fn swept(this: &tiny_skia::Pixmap, next: &tiny_skia::Pixmap, k: f32) -> Vec<u8> {
    let w = this.width() as usize;
    let edge = (w as f32 * EDGE).max(1.0);
    // Eased, and from fully left of the picture to fully right of it.
    let k = k * k * (3.0 - 2.0 * k);
    let front = -edge + k * (w as f32 + 2.0 * edge);
    // How much of the next theme each column shows, worked out once.
    let shares: Vec<f32> = (0..w)
        .map(|x| {
            let m = ((front - x as f32) / edge).clamp(0.0, 1.0);
            m * m * (3.0 - 2.0 * m)
        })
        .collect();
    let (a, b) = (this.data(), next.data());
    let mut out = vec![0u8; a.len()];
    // Row by row, as the pixels lie.
    for (i, ((mixed, was), comes)) in out
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(a.as_chunks::<4>().0)
        .zip(b.as_chunks::<4>().0)
        .enumerate()
    {
        let m = shares[i % w.max(1)];
        for c in 0..4 {
            mixed[c] = (f32::from(was[c]) * (1.0 - m) + f32::from(comes[c]) * m).round() as u8;
        }
    }
    out
}

/// The picture's `viewBox`: where it starts and how large it is; without
/// one, its width and height, as [`super::timelapse::widened`] takes them.
fn view_of(svg: &str) -> (f32, f32, f32, f32) {
    let Some(tag) = svg
        .find("<svg")
        .and_then(|start| Some(&svg[start..start + svg[start..].find('>')?]))
    else {
        return (0.0, 0.0, 1.0, 1.0);
    };
    let attr = |name: &str| -> Option<&str> {
        tag.split(&format!(" {name}=\"")).nth(1)?.split('"').next()
    };
    let view: Vec<f32> = attr("viewBox")
        .map(|view| {
            view.split(|c: char| c == ',' || c.is_whitespace())
                .filter_map(|n| n.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    let number = |name: &str| {
        attr(name)
            .and_then(|value| value.trim_end_matches("px").parse().ok())
            .unwrap_or(1.0)
    };
    match view[..] {
        [x, y, w, h] if w > 0.0 && h > 0.0 => (x, y, w, h),
        _ => (0.0, 0.0, number("width"), number("height")),
    }
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Writes the page; how large it is.
fn page(
    args: &Moving,
    text: &str,
    name: &str,
    language: Language,
    themed: &[Themed],
) -> Result<usize> {
    use omaspec_render::export::sweep as page;
    let color = |hex: &str| {
        omaspec_uml::parse_color(hex)
            .ok()
            .flatten()
            .unwrap_or(omaspec_uml::Color::BLACK)
    };
    let sweep = page::Sweep {
        themes: themed
            .iter()
            .map(|one| page::Theme {
                name: one.name.clone(),
                svg: one.svg.clone(),
                paper: color(&one.paper),
                ink: color(&one.ink),
                muted: color(&one.muted),
            })
            .collect(),
        stay: stay(themed.len()),
        sweep: SWEEP,
        edge: EDGE,
    };
    let written = page::page(
        &sweep,
        Some(export::Source {
            code: text,
            language,
            file: Some(name),
        }),
        name.rsplit_once('.').map_or(name, |(stem, _)| stem),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    if written.is_empty() {
        bail!("nothing was written");
    }
    crate::files::write_atomic(&args.output, written.as_bytes())
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(written.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A door through the built-in palette and a theme of its own, as a
    /// GIF and as a page; one theme alone, or one that is not there, is
    /// refused.
    #[test]
    fn a_diagram_sweeps_through_its_themes() {
        let work = std::env::temp_dir().join(format!("omaspec-sweep-{}", std::process::id()));
        std::fs::create_dir_all(work.join("ember")).unwrap();
        let door = work.join("door.puml");
        std::fs::write(
            &door,
            "@startuml\n[*] --> Closed\nClosed --> Open : open\n@enduml\n",
        )
        .unwrap();
        let ember = work.join("ember/colors.toml");
        std::fs::write(
            &ember,
            "mode = \"dark\"\naccent = \"#ff9e3b\"\nbackground = \"#1a1410\"\nforeground = \"#f2d5b8\"\n",
        )
        .unwrap();
        let moving = |themes: Vec<String>, output: &str| Moving {
            file: door.clone(),
            text: None,
            output: work.join(output),
            events: Vec::new(),
            steps: 100,
            width: Some(300),
            fps: Some(4),
            folds: Vec::new(),
            dives: Vec::new(),
            builds: false,
            lockstep: false,
            history: false,
            travel: None,
            traffic: Vec::new(),
            themes: Some(themes),
            ink: false,
            look: None,
            timing: Default::default(),
            whole: false,
            theme: None,
            file_colors: false,
            root: None,
            forced: None,
            renderer: Some("tiny-skia"),
            quality: super::super::gifs::Quality::default(),
        };
        let both = vec!["built-in".to_owned(), ember.to_string_lossy().into_owned()];
        let told = super::super::moving::play(&moving(both.clone(), "door.gif")).unwrap();
        assert!(told.contains("2 themes"), "{told}");
        let gif = std::fs::read(work.join("door.gif")).unwrap();
        assert_eq!(u16::from_le_bytes([gif[6], gif[7]]), 300);
        let told = super::super::moving::play(&moving(both, "door.html")).unwrap();
        assert!(told.contains("a page"), "{told}");
        let page = std::fs::read_to_string(work.join("door.html")).unwrap();
        assert!(page.contains("Built In") && page.contains("Ember"));
        let one = super::super::moving::play(&moving(vec!["built-in".into()], "one.gif"))
            .unwrap_err()
            .to_string();
        assert!(one.contains("two themes or more"), "{one}");
        let none = super::super::moving::play(&moving(
            vec!["built-in".into(), "nowhere-at-all".into()],
            "none.gif",
        ))
        .unwrap_err()
        .to_string();
        assert!(none.contains("nowhere-at-all is no theme"), "{none}");
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn a_theme_is_named_as_a_caption_says_it() {
        assert_eq!(shown_name("tokyo-night"), "Tokyo Night");
        assert_eq!(shown_name("rose_pine"), "Rose Pine");
        assert_eq!(shown_name("built-in"), "Built In");
    }

    #[test]
    fn the_next_theme_sweeps_in_from_the_left() {
        let mut this = tiny_skia::Pixmap::new(100, 2).unwrap();
        this.fill(tiny_skia::Color::BLACK);
        let mut next = tiny_skia::Pixmap::new(100, 2).unwrap();
        next.fill(tiny_skia::Color::WHITE);
        let half = swept(&this, &next, 0.5);
        // The left side is the next theme's, the right still this one's.
        assert_eq!(half[0], 255);
        assert_eq!(half[99 * 4], 0);
        assert!(swept(&this, &next, 0.0).iter().step_by(4).all(|&v| v == 0));
    }
}
