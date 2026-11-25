//! From sketch to ink (`omaspec play FILE --ink -o ink.gif`): the diagram
//! in the sketch look (docs/adr/0023) drawn stroke by stroke as with a
//! pen, each fill and each word coming as the pen reaches it, and then
//! inked into the clean look the pane draws it in. Both are drawn from
//! the same layout, so the ink lies on the sketch. The frames are drawn
//! without a GPU; as `.html` a page draws the same in vectors
//! ([`omaspec_render::export::ink`]).

use anyhow::{Context, Result, bail};
use omaspec_core::Language;
use omaspec_render::export;
use omaspec_render::export::tiny_skia;
use omaspec_render::{Place, Renderer};

use super::Start;
use super::films::Film;
use super::gifs::Frames;
use super::moving::Moving;

/// How fast the pen draws, in units a second, and how long it takes at
/// least and at most.
const SPEED: f32 = 700.0;
const QUICKEST: f32 = 2.0;
const SLOWEST: f32 = 10.0;
/// How high a band of the picture the pen draws at once is, in its own
/// units: what lies in one is drawn in the picture's order.
const BAND: f32 = 24.0;
/// How long a fill or a word takes to come once the pen reaches it.
const FADE: f32 = 0.3;
/// How long the sketch stays drawn, the ink takes, and the clean picture
/// stays before the picture starts again.
const DRAWN: f32 = 0.6;
const INK: f32 = 1.4;
const INKED: f32 = 1.8;
/// How long the clean picture takes to clear to the paper the pen draws on.
const CLEAR: f32 = 0.5;
/// The shares of the ink the sketch's words take to go and the clean
/// words to come; the lines and fills turn between.
const WORDS_GO: f32 = 0.25;
const WORDS_COME: f32 = 0.25;
/// The film's height is between these shares of its width.
const LEAST_HEIGHT: f32 = 0.3;
const MOST_HEIGHT: f32 = 1.4;
/// The picture's width where none is asked for, at least and at most.
const NARROWEST: f32 = 480.0;
const WIDEST: f32 = 1200.0;

/// What the pen does with one element of the sketch, and when.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Stroke {
    /// A line it draws from `start` to `end`, its longest run `length`
    /// long in its own units. A dash starts again with each run of a line
    /// (a sketch draws a line twice), so its runs are drawn at once, as
    /// far along as the longest; `share` is that run's part of the whole
    /// line, which a page's script measures.
    Drawn {
        start: f32,
        end: f32,
        length: f32,
        share: f32,
    },
    /// A fill, a word or a dashed line, which comes at `at`.
    Comes { at: f32 },
}

/// An element of the sketch the pen draws: where its start tag lies in
/// the picture's text, and what the pen does with it.
#[derive(Debug, Clone)]
pub(super) struct Element {
    pub(super) tag: std::ops::Range<usize>,
    pub(super) stroke: Stroke,
}

/// The sketch, and when the pen has drawn it all.
pub(super) struct Pen {
    pub(super) svg: String,
    pub(super) elements: Vec<Element>,
    pub(super) end: f32,
}

/// Writes the sketch inked of `args.file`; what it drew.
pub(super) fn ink(args: &Moving, film: Film, fps: u32) -> Result<String> {
    let text = args.read()?;
    let (name, language) = args.named(&text);
    let start = Start {
        files: vec![args.file.clone()],
        folder: None,
        forced: args.forced,
    };
    let app = super::moving::film_app(args, start)?;
    // The look the ink ends in: the one asked for, else the colors the
    // play is drawn in (REQ-090).
    let clean_skin = match args.look {
        Some(crate::look::Look::Sketch) => {
            bail!("--look sketch is what the pen draws: ink into another look")
        }
        // A film shows one look, a page drops the dark one's style: auto
        // is drawn light alone, not light and dark to be merged.
        Some(look) => look.skin().map(|skin| skin.without_schemes()),
        None => app.pane_skin(args.root.as_deref()),
    };
    let sketch_skin = crate::look::Look::Sketch.skin();
    let draw = |skin: Option<&crate::look::Skin>| -> Result<String> {
        let place = Place::of(&args.file, None);
        let mut renderer = Renderer::new(&crate::commands::render_config());
        let mut maps = crate::specs::Maps::default();
        crate::look::render(
            &mut maps,
            &mut renderer,
            &text,
            language,
            Some(&args.file),
            place.as_ref(),
            skin,
        )
        .map(|rendered| rendered.svg)
        .map_err(|err| anyhow::anyhow!("{err}"))
    };
    let sketch = draw(sketch_skin.as_ref())?;
    let clean = draw(clean_skin.as_ref())?;
    let size = export::parse(&sketch)
        .map_err(|err| anyhow::anyhow!("{err}"))?
        .size();
    let clean_size = export::parse(&clean)
        .map_err(|err| anyhow::anyhow!("{err}"))?
        .size();
    // Before anything is drawn, for a page as for frames.
    anyhow::ensure!(
        (size.width() - clean_size.width()).abs() < 0.5
            && (size.height() - clean_size.height()).abs() < 0.5,
        "the sketch and the clean picture are not of one size: the ink would not lie on the sketch"
    );
    // How long the clean picture holds; the pen takes what the rest
    // leaves of a length asked for, else as long as the diagram asks
    // (REQ-092).
    let hold = args.timing.hold.unwrap_or(INKED);
    // A page does not clear: it starts on the paper.
    let clear = if film == Film::Page { 0.0 } else { CLEAR };
    let rest = hold + clear + FADE + DRAWN + INK;
    let time = match args.timing.duration {
        Some(duration) => {
            anyhow::ensure!(
                duration - rest >= 1.0,
                "--duration {duration} is too short for ink: holding, clearing and inking take {rest:.1} s, and the pen 1 s at least"
            );
            Some(duration - rest)
        }
        None => None,
    };
    let pen = Pen::of(&sketch, time)?;
    let strokes = pen
        .elements
        .iter()
        .filter(|e| matches!(e.stroke, Stroke::Drawn { .. }))
        .count();
    let told = format!("{strokes} strokes, inked");
    if film == Film::Page {
        let written = page(args, &text, &name, language, &pen, &clean, hold)?;
        return Ok(format!(
            "{} → {} (a page, {} KB, {told} in vectors)",
            args.file.display(),
            args.output.display(),
            written.div_ceil(1024)
        ));
    }
    // The film's height is between shares of its width, the picture amid
    // its paper: a diagram much taller or wider than a screen made a strip
    // (REQ-094).
    let width = match args.width {
        Some(width) => width as f32,
        None => (size.width() * 1.6).clamp(NARROWEST, WIDEST).round(),
    };
    let height = (width * size.height() / size.width().max(1.0))
        .clamp(width * LEAST_HEIGHT, width * MOST_HEIGHT)
        .round();
    let scale = (width / size.width().max(1.0)).min(height / size.height().max(1.0));
    let rasterized = |svg: &str| -> Result<tiny_skia::Pixmap> {
        let tree = export::parse(svg).map_err(|err| anyhow::anyhow!("{err}"))?;
        export::rasterize(&tree, scale).map_err(|err| anyhow::anyhow!("{err}"))
    };
    let drawn = rasterized(&sketch)?;
    let frame = (
        (width as u32).max(drawn.width()),
        (height as u32).max(drawn.height()),
    );
    let raster = |svg: &str| -> Result<tiny_skia::Pixmap> { framed(rasterized(svg)?, frame) };
    let drawn = framed(drawn, frame)?;
    let inked = raster(&clean)?;
    // The paper the pen draws on, and both pictures without their words.
    let paper = raster(&pen.at(-1.0))?;
    let drawn_bare = raster(&without_words(&sketch)?)?;
    let inked_bare = raster(&without_words(&clean)?)?;
    // How long a frame stays, in a GIF's hundredths (which 100 / fps
    // rounds down) or in ticks of the film's rate; the frames are counted
    // by it, so the film lasts as long as asked.
    let (delay, per_second) = match film {
        Film::Gif => (u16::try_from((100 / fps).max(2)).unwrap_or(5), 100.0),
        Film::Apng | Film::Webp | Film::Page => (1, fps as f32),
    };
    let tick = f32::from(delay) / per_second;
    let mut frames = Frames::default();
    let push = |frames: &mut Frames, rgba: &[u8]| {
        frames.push(frame.0 as usize, frame.1 as usize, rgba, delay);
    };
    let steps = |seconds: f32| (seconds / tick).round().max(1.0) as usize;
    let fade =
        |frames: &mut Frames, from: &tiny_skia::Pixmap, to: &tiny_skia::Pixmap, seconds: f32| {
            let count = steps(seconds);
            for step in 1..=count {
                let k = step as f32 / count as f32;
                let k = k * k * (3.0 - 2.0 * k);
                let mixed: Vec<u8> = from
                    .data()
                    .iter()
                    .zip(to.data())
                    .map(|(a, b)| (f32::from(*a) * (1.0 - k) + f32::from(*b) * k).round() as u8)
                    .collect();
                push(frames, &mixed);
            }
        };
    // The film starts on the clean picture, held, so its first frame
    // shows the diagram (REQ-091); the ink ends on it, so the film
    // starts again where it ends. It clears to the paper for the pen.
    let held = (hold * per_second)
        .round()
        .clamp(f32::from(delay), f32::from(u16::MAX)) as u16;
    frames.push(frame.0 as usize, frame.1 as usize, inked.data(), held);
    fade(&mut frames, &inked, &paper, CLEAR);
    let mut t = 0.0;
    while t < pen.end + DRAWN {
        push(&mut frames, raster(&pen.at(t))?.data());
        t += tick;
    }
    // The ink: the sketch's words go, its lines and fills turn into the
    // clean picture's, and the clean words come; words of both at once
    // stood as ghosts over each other (REQ-093).
    fade(&mut frames, &drawn, &drawn_bare, INK * WORDS_GO);
    fade(
        &mut frames,
        &drawn_bare,
        &inked_bare,
        INK * (1.0 - WORDS_GO - WORDS_COME),
    );
    fade(&mut frames, &inked_bare, &inked, INK * WORDS_COME);
    let Some((w, h)) = frames.size() else {
        bail!("no frame was drawn");
    };
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

impl Pen {
    /// The pen's way through `svg`: each line it draws in the order the
    /// picture has them, as fast as [`SPEED`], each fill and word when it
    /// gets there.
    pub(super) fn of(svg: &str, time: Option<f32>) -> Result<Pen> {
        // The elements that draw, the paper there before the pen.
        let found: Vec<(std::ops::Range<usize>, bool, bool)> = export::drawn(svg)
            .map_err(|err| anyhow::anyhow!("the sketch is no SVG: {err}"))?
            .into_iter()
            .map(|one| (one.tag, one.stroked, one.name == "text"))
            .collect();
        // How long each line is, as usvg reads it: each marked so it can
        // be found.
        let mut marked = String::with_capacity(svg.len() + found.len() * 16);
        let mut at = 0;
        for (index, (tag, _, _)) in found.iter().enumerate() {
            // An id of its own is taken over in the copy measured only.
            marked.push_str(&svg[at..tag.start]);
            marked.push_str(&with_attrs(
                &svg[tag.clone()],
                &[("id", format!("pen-{index}"))],
            ));
            at = tag.end;
        }
        marked.push_str(&svg[at..]);
        let tree = export::parse(&marked).map_err(|err| anyhow::anyhow!("{err}"))?;
        let mut measured = vec![Measured::default(); found.len()];
        measure(tree.root(), &mut measured);
        // The pen goes along each line's longest run, the others with it.
        let total: f32 = found
            .iter()
            .zip(&measured)
            .filter(|((_, stroked, _), _)| *stroked)
            .map(|(_, m)| m.run)
            .sum();
        let asked = time;
        let time = time.unwrap_or_else(|| (total / SPEED).clamp(QUICKEST, SLOWEST));
        // The pen goes down the picture as a hand would, not layer by
        // layer as the picture is written: a box, its words and the lines
        // that leave it together; in the picture's order within a band.
        let mut order: Vec<usize> = (0..found.len()).collect();
        let band = |i: usize| (measured[i].top / BAND).floor() as i64;
        order.sort_by_key(|&i| (band(i), i));
        let mut strokes = vec![Stroke::Comes { at: 0.0 }; found.len()];
        let mut cursor = 0.0f32;
        for i in order {
            let (_, stroked, _) = &found[i];
            let Measured { length, run, .. } = measured[i];
            strokes[i] = if *stroked && run > 0.0 {
                let took = (run / total.max(1.0) * time).max(0.02);
                let drawn = Stroke::Drawn {
                    start: cursor,
                    end: cursor + took,
                    length: run,
                    share: (run / length.max(run)).min(1.0),
                };
                cursor += took;
                drawn
            } else {
                Stroke::Comes { at: cursor }
            };
        }
        // A stroke takes a moment at least: many of them ran past a time
        // asked for, so all are drawn quicker into it.
        if let Some(asked) = asked
            && cursor > asked
        {
            let k = asked / cursor;
            for stroke in &mut strokes {
                match stroke {
                    Stroke::Drawn { start, end, .. } => {
                        *start *= k;
                        *end *= k;
                    }
                    Stroke::Comes { at } => *at *= k,
                }
            }
            cursor = asked;
        }
        let elements = found
            .into_iter()
            .zip(strokes)
            .map(|((tag, _, _), stroke)| Element { tag, stroke })
            .collect();
        Ok(Pen {
            svg: svg.to_owned(),
            elements,
            end: cursor + FADE,
        })
    }

    /// The sketch as the pen has drawn it by `t`.
    pub(super) fn at(&self, t: f32) -> String {
        let mut out = String::with_capacity(self.svg.len() + self.elements.len() * 48);
        let mut from = 0;
        for element in &self.elements {
            let tag = &self.svg[element.tag.clone()];
            out.push_str(&self.svg[from..element.tag.start]);
            let attrs = match element.stroke {
                Stroke::Drawn {
                    start, end, length, ..
                } => {
                    let k = ((t - start) / (end - start).max(0.001)).clamp(0.0, 1.0);
                    if k <= 0.0 {
                        vec![("opacity", "0".to_owned())]
                    } else if k >= 1.0 {
                        // Drawn: the line as it is, no dash left to cut
                        // its end short where the measure fell short.
                        Vec::new()
                    } else {
                        vec![
                            ("stroke-dasharray", format!("{length:.2} {length:.2}")),
                            ("stroke-dashoffset", format!("{:.2}", length * (1.0 - k))),
                        ]
                    }
                }
                Stroke::Comes { at } => {
                    let k = ((t - at) / FADE).clamp(0.0, 1.0);
                    vec![("opacity", format!("{k:.3}"))]
                }
            };
            out.push_str(&with_attrs(tag, &attrs));
            from = element.tag.end;
        }
        out.push_str(&self.svg[from..]);
        out
    }
}

/// What usvg read of an element of the sketch.
#[derive(Debug, Clone, Copy)]
struct Measured {
    /// How long its lines are, all runs together, and its longest run.
    length: f32,
    run: f32,
    /// Where its top is.
    top: f32,
}

impl Default for Measured {
    fn default() -> Self {
        Measured {
            length: 0.0,
            run: 0.0,
            top: f32::MAX,
        }
    }
}

/// The lengths of the lines usvg read, and where each element's top is,
/// by the mark of their element.
fn measure(group: &export::usvg::Group, measured: &mut [Measured]) {
    let count = measured.len();
    let index = |id: &str| {
        id.strip_prefix("pen-")
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|&i| i < count)
    };
    for node in group.children() {
        match node {
            export::usvg::Node::Group(inner) => measure(inner, measured),
            export::usvg::Node::Path(path) => {
                if let Some(index) = index(path.id()) {
                    let (length, run) = length_of(path.data());
                    let m = &mut measured[index];
                    m.length += length;
                    m.run = m.run.max(run);
                    m.top = m.top.min(path.abs_bounding_box().top());
                }
            }
            export::usvg::Node::Text(text) => {
                if let Some(index) = index(text.id()) {
                    let m = &mut measured[index];
                    m.top = m.top.min(text.abs_bounding_box().top());
                }
            }
            export::usvg::Node::Image(_) => {}
        }
    }
}

/// How long a path is, its curves as runs of short lines; and how long
/// its longest run is, from one move to the next.
fn length_of(path: &tiny_skia::Path) -> (f32, f32) {
    use tiny_skia::PathSegment;
    let mut length = 0.0;
    let mut longest = 0.0f32;
    let mut run_from = 0.0;
    let mut at = tiny_skia::Point::zero();
    let mut first = at;
    let dist = |a: tiny_skia::Point, b: tiny_skia::Point| (b.x - a.x).hypot(b.y - a.y);
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                longest = longest.max(length - run_from);
                run_from = length;
                at = p;
                first = p;
            }
            PathSegment::LineTo(p) => {
                length += dist(at, p);
                at = p;
            }
            PathSegment::QuadTo(c, p) => {
                let mut last = at;
                for i in 1..=8 {
                    let k = i as f32 / 8.0;
                    let q = tiny_skia::Point::from_xy(
                        (1.0 - k).powi(2) * at.x + 2.0 * (1.0 - k) * k * c.x + k * k * p.x,
                        (1.0 - k).powi(2) * at.y + 2.0 * (1.0 - k) * k * c.y + k * k * p.y,
                    );
                    length += dist(last, q);
                    last = q;
                }
                at = p;
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let mut last = at;
                for i in 1..=8 {
                    let k = i as f32 / 8.0;
                    let m = 1.0 - k;
                    let q = tiny_skia::Point::from_xy(
                        m.powi(3) * at.x
                            + 3.0 * m * m * k * c1.x
                            + 3.0 * m * k * k * c2.x
                            + k.powi(3) * p.x,
                        m.powi(3) * at.y
                            + 3.0 * m * m * k * c1.y
                            + 3.0 * m * k * k * c2.y
                            + k.powi(3) * p.y,
                    );
                    length += dist(last, q);
                    last = q;
                }
                at = p;
            }
            PathSegment::Close => {
                length += dist(at, first);
                at = first;
            }
        }
    }
    (length, longest.max(length - run_from))
}

/// `picture` amid a frame of `size` pixels of its own paper, the color of
/// its corner.
fn framed(picture: tiny_skia::Pixmap, size: (u32, u32)) -> Result<tiny_skia::Pixmap> {
    if (picture.width(), picture.height()) == size {
        return Ok(picture);
    }
    let mut frame = tiny_skia::Pixmap::new(size.0, size.1).context("no frame of that size")?;
    if let Some(corner) = picture.pixel(0, 0) {
        let paper = corner.demultiply();
        frame.fill(tiny_skia::Color::from_rgba8(
            paper.red(),
            paper.green(),
            paper.blue(),
            paper.alpha(),
        ));
    }
    let at = |frame: u32, picture: u32| (i64::from(frame) - i64::from(picture)) / 2;
    frame.draw_pixmap(
        i32::try_from(at(size.0, picture.width())).unwrap_or(0),
        i32::try_from(at(size.1, picture.height())).unwrap_or(0),
        picture.as_ref(),
        &tiny_skia::PixmapPaint::default(),
        tiny_skia::Transform::identity(),
        None,
    );
    Ok(frame)
}

/// `svg` with its words left out, each as it stands.
fn without_words(svg: &str) -> Result<String> {
    let found = export::drawn(svg).map_err(|err| anyhow::anyhow!("{err}"))?;
    let mut out = String::with_capacity(svg.len());
    let mut at = 0;
    for one in found.into_iter().filter(|one| one.name == "text") {
        out.push_str(&svg[at..one.tag.start]);
        out.push_str(&with_attrs(
            &svg[one.tag.clone()],
            &[("opacity", "0".to_owned())],
        ));
        at = one.tag.end;
    }
    out.push_str(&svg[at..]);
    Ok(out)
}

/// `tag` with `attrs` set: an attribute it has takes the value (an
/// opacity multiplied), one it lacks is added after its name.
fn with_attrs(tag: &str, attrs: &[(&str, String)]) -> String {
    let mut out = tag.to_owned();
    for (name, value) in attrs {
        let key = format!(" {name}=\"");
        match out.find(&key) {
            Some(at) => {
                let from = at + key.len();
                let to = from + out[from..].find('"').unwrap_or(0);
                let value = if *name == "opacity" {
                    let own: f32 = out[from..to].parse().unwrap_or(1.0);
                    format!("{:.3}", own * value.parse::<f32>().unwrap_or(1.0))
                } else {
                    value.clone()
                };
                out.replace_range(from..to, &value);
            }
            None => {
                let name_end = out
                    .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
                    .unwrap_or(out.len());
                out.insert_str(name_end, &format!(" {name}=\"{value}\""));
            }
        }
    }
    out
}

/// Writes the page; how large it is.
fn page(
    args: &Moving,
    text: &str,
    name: &str,
    language: Language,
    pen: &Pen,
    clean: &str,
    hold: f32,
) -> Result<usize> {
    use omaspec_render::export::ink as page;
    let ink = page::Ink {
        sketch: pen.svg.clone(),
        clean: clean.to_owned(),
        strokes: pen
            .elements
            .iter()
            .map(|e| match e.stroke {
                Stroke::Drawn {
                    start, end, share, ..
                } => page::Stroke::Drawn { start, end, share },
                Stroke::Comes { at } => page::Stroke::Comes { at },
            })
            .collect(),
        end: pen.end,
        drawn: DRAWN,
        ink: INK,
        inked: hold,
        fade: FADE,
        words_go: WORDS_GO,
        words_come: WORDS_COME,
        paper: omaspec_uml::parse_color(&super::building::picture::paper_of(clean))
            .ok()
            .flatten()
            .unwrap_or(omaspec_uml::Color::WHITE),
    };
    let written = page::page(
        &ink,
        Some(export::Source {
            code: text,
            language,
            file: Some(name),
            folds: &[],
        }),
        name.rsplit_once('.').map_or(name, |(stem, _)| stem),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    crate::files::write_atomic(&args.output, written.as_bytes())
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(written.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKETCH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40" viewBox="0 0 100 40"><rect width="100" height="40" fill="#fff"/><path d="M0,10L100,10" fill="none" stroke="#000"/><path d="M0,20L50,20" fill="#ccc"/><path d="M0,30L50,30" fill="none" stroke="#000" opacity="0.5"/><text x="5" y="35">hi</text></svg>"##;

    /// A door from sketch to ink, as a GIF and as a page.
    #[test]
    fn a_door_is_sketched_and_inked() {
        let work = std::env::temp_dir().join(format!("omaspec-ink-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let door = work.join("door.puml");
        std::fs::write(
            &door,
            "@startuml\n[*] --> Closed\nClosed --> Open : open\n@enduml\n",
        )
        .unwrap();
        let moving = |output: &str| Moving {
            file: door.clone(),
            text: None,
            output: work.join(output),
            events: Vec::new(),
            steps: Some(100),
            width: Some(300),
            fps: Some(4),
            folds: Vec::new(),
            dives: Vec::new(),
            builds: false,
            bare: false,
            lockstep: false,
            history: false,
            travel: None,
            traffic: Vec::new(),
            themes: None,
            ink: true,
            look: None,
            timing: Default::default(),
            whole: false,
            theme: Some("built-in".into()),
            file_colors: false,
            root: None,
            forced: None,
            renderer: Some("tiny-skia"),
            quality: super::super::gifs::Quality::default(),
        };
        let told = super::super::moving::play(&moving("door.gif")).unwrap();
        assert!(told.contains("strokes, inked"), "{told}");
        let gif = std::fs::read(work.join("door.gif")).unwrap();
        assert_eq!(u16::from_le_bytes([gif[6], gif[7]]), 300);
        let told = super::super::moving::play(&moving("door.html")).unwrap();
        assert!(told.contains("a page"), "{told}");
        let page = std::fs::read_to_string(work.join("door.html")).unwrap();
        assert!(page.contains(r#""kind":"ink""#));
        assert!(page.contains(r#"id="sketch""#) && page.contains(r#"id="clean""#));
        // The page gives its diagram back, as `omaspec extract` reads it.
        let kept = omaspec_core::embed::source_from_svg(&page).unwrap();
        assert!(kept.code.contains("Closed --> Open"), "{}", kept.code);
        let _ = std::fs::remove_dir_all(&work);
    }

    /// The film starts on the clean picture, held as long as asked, in the
    /// look asked for (REQ-090, REQ-091), lasts as long as asked
    /// (REQ-092), and keeps a frame of bounded proportions around a tall
    /// diagram (REQ-094).
    #[test]
    fn an_ink_starts_on_its_clean_picture_in_the_look_asked_for() {
        let work = std::env::temp_dir().join(format!("omaspec-ink-look-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let tall = work.join("tall.mmd");
        std::fs::write(
            &tall,
            "stateDiagram-v2\n  [*] --> A\n  A --> B\n  B --> C\n  C --> D\n  D --> E\n  E --> [*]\n",
        )
        .unwrap();
        let film = |look: Option<crate::look::Look>, timing: super::super::gifs::Timing| {
            let output = work.join("tall.gif");
            let moving = Moving {
                file: tall.clone(),
                text: None,
                output: output.clone(),
                events: Vec::new(),
                steps: Some(100),
                width: Some(300),
                fps: Some(10),
                folds: Vec::new(),
                dives: Vec::new(),
                builds: false,
                bare: false,
                lockstep: false,
                history: false,
                travel: None,
                traffic: Vec::new(),
                themes: None,
                ink: true,
                look,
                timing,
                whole: false,
                theme: Some("built-in".into()),
                file_colors: false,
                root: None,
                forced: None,
                renderer: Some("tiny-skia"),
                quality: super::super::gifs::Quality::default(),
            };
            super::super::moving::play(&moving).unwrap();
            let mut options = gif::DecodeOptions::new();
            options.set_color_output(gif::ColorOutput::RGBA);
            let mut decoder = options
                .read_info(std::fs::File::open(&output).unwrap())
                .unwrap();
            let (width, height) = (decoder.width(), decoder.height());
            let first = decoder.read_next_frame().unwrap().unwrap().clone();
            let mut delays = vec![first.delay];
            while let Some(frame) = decoder.read_next_frame().unwrap() {
                delays.push(frame.delay);
            }
            (width, height, first, delays)
        };
        let (width, height, first, delays) = film(None, Default::default());
        // A frame of bounded proportions, the tall diagram amid its paper.
        assert_eq!(width, 300);
        assert!(f32::from(height) <= 300.0 * MOST_HEIGHT + 1.0, "{height}");
        // The first frame is the clean picture, not paper, held.
        let colors: std::collections::HashSet<&[u8]> = first.buffer.chunks(4).collect();
        assert!(colors.len() > 4, "{} colors", colors.len());
        assert_eq!(first.delay, (INKED * 100.0).round() as u16);
        let natural: u32 = delays.iter().map(|&d| u32::from(d)).sum();
        // In the look asked for: light paper, where the theme's is dark.
        let corner =
            |frame: &gif::Frame| frame.buffer[..3].iter().map(|&c| u32::from(c)).sum::<u32>();
        let (_, _, light, _) = film(Some(crate::look::Look::Light), Default::default());
        assert!(
            corner(&light) > 600 && corner(&first) < 300,
            "{} {}",
            corner(&light),
            corner(&first)
        );
        // As long as asked, the clean picture held as asked.
        let timing = super::super::gifs::Timing {
            duration: Some(12.0),
            hold: Some(2.0),
        };
        let (_, _, held, delays) = film(None, timing);
        assert_eq!(held.delay, 200);
        let total: u32 = delays.iter().map(|&d| u32::from(d)).sum();
        assert!((total as i32 - 1200).abs() <= 20, "{total} after {natural}");
        let _ = std::fs::remove_dir_all(&work);
    }

    /// A time asked for holds however many strokes there are: each takes
    /// a moment at least, so many ran past it, and all are drawn quicker.
    #[test]
    fn the_pen_keeps_to_the_time_asked_for() {
        let lines: String = (0..200)
            .map(|i| format!(r##"<path d="M0,{i}L100,{i}" fill="none" stroke="#000"/>"##))
            .collect();
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="200" viewBox="0 0 100 200"><g>{lines}</g></svg>"##
        );
        let pen = Pen::of(&svg, Some(1.4)).unwrap();
        assert!((pen.end - (1.4 + FADE)).abs() < 0.01, "{}", pen.end);
        let natural = Pen::of(&svg, None).unwrap();
        assert!(natural.end > pen.end, "{} {}", natural.end, pen.end);
    }

    /// A sketch's line drawn twice: a dash starts again with each run, so
    /// both are drawn at once, as long as one of them takes.
    #[test]
    fn the_runs_of_a_line_are_drawn_together() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40" viewBox="0 0 100 40"><g><path d="M0,10L100,10M0,12L100,12" fill="none" stroke="#000"/><path d="M0,30L100,30" fill="none" stroke="#000"/></g></svg>"##;
        let pen = Pen::of(svg, None).unwrap();
        let Stroke::Drawn {
            start,
            end,
            length,
            share,
        } = pen.elements[0].stroke
        else {
            panic!("{:?}", pen.elements);
        };
        assert!((length - 100.0).abs() < 0.5, "{length}");
        assert!((share - 0.5).abs() < 0.01, "{share}");
        let Stroke::Drawn {
            start: s2, end: e2, ..
        } = pen.elements[1].stroke
        else {
            panic!("{:?}", pen.elements);
        };
        assert!(((end - start) - (e2 - s2)).abs() < 0.01);
    }

    #[test]
    fn the_pen_draws_each_line_as_long_as_it_is() {
        let pen = Pen::of(SKETCH, None).unwrap();
        let strokes: Vec<&Stroke> = pen.elements.iter().map(|e| &e.stroke).collect();
        // The paper is there before the pen.
        assert_eq!(strokes.len(), 4);
        let Stroke::Drawn {
            start, end, length, ..
        } = *strokes[0]
        else {
            panic!("{strokes:?}");
        };
        assert!((length - 100.0).abs() < 0.5, "{length}");
        assert_eq!(start, 0.0);
        // The second line, half as long, takes half the time.
        let Stroke::Drawn {
            start: s2, end: e2, ..
        } = *strokes[2]
        else {
            panic!("{strokes:?}");
        };
        assert!(((e2 - s2) * 2.0 - (end - start)).abs() < 0.01);
        // The fill comes when the pen reaches it.
        assert_eq!(*strokes[1], Stroke::Comes { at: end });
    }

    #[test]
    fn a_line_is_drawn_as_far_as_the_pen_has_come() {
        let pen = Pen::of(SKETCH, None).unwrap();
        let Stroke::Drawn { start, end, .. } = pen.elements[0].stroke else {
            panic!();
        };
        let half = pen.at((start + end) / 2.0);
        assert!(half.contains(r#"stroke-dashoffset="50.00""#), "{half}");
        // Not begun: hidden; its own opacity kept.
        let begun = pen.at(0.0);
        assert!(begun.contains(r#"opacity="0.000" "#) || begun.contains(r#"opacity="0""#));
        let done = pen.at(pen.end + 1.0);
        assert!(done.contains(r#"opacity="0.5""#), "{done}");
        // Drawn: the line as it is.
        assert!(!done.contains("stroke-dashoffset"), "{done}");
        // Every frame is an SVG.
        for t in [0.0, start, end, pen.end] {
            assert!(export::parse(&pen.at(t)).is_ok());
        }
    }
}
