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

use super::films::Film;
use super::gifs::Frames;
use super::moving::{Moving, theme};
use super::{App, Start};

/// How fast the pen draws, in units a second, and how long it takes at
/// least and at most.
const SPEED: f32 = 900.0;
const QUICKEST: f32 = 2.5;
const SLOWEST: f32 = 6.0;
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
/// The picture's width where none is asked for, at least and at most.
const NARROWEST: f32 = 480.0;
const WIDEST: f32 = 1200.0;

/// What the pen does with one element of the sketch, and when.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Stroke {
    /// A line it draws from `start` to `end`, `length` long in its own
    /// units.
    Drawn { start: f32, end: f32, length: f32 },
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
    let omarchy = theme(args.theme.as_deref())?;
    let start = Start {
        files: vec![args.file.clone()],
        folder: None,
        forced: args.forced,
    };
    let (mut app, _) = App::themed(start, None, omarchy);
    app.follow_theme = !args.file_colors;
    let clean_skin = app.pane_skin(args.root.as_deref());
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
    let pen = Pen::of(&sketch)?;
    let strokes = pen
        .elements
        .iter()
        .filter(|e| matches!(e.stroke, Stroke::Drawn { .. }))
        .count();
    let told = format!("{strokes} strokes, inked");
    if film == Film::Page {
        let written = page(args, &text, &name, language, &pen, &clean)?;
        return Ok(format!(
            "{} → {} (a page, {} KB, {told} in vectors)",
            args.file.display(),
            args.output.display(),
            written.div_ceil(1024)
        ));
    }
    let size = export::parse(&sketch)
        .map_err(|err| anyhow::anyhow!("{err}"))?
        .size();
    let width = match args.width {
        Some(width) => width as f32,
        None => (size.width() * 1.6).clamp(NARROWEST, WIDEST).round(),
    };
    let scale = width / size.width().max(1.0);
    let raster = |svg: &str| -> Result<tiny_skia::Pixmap> {
        let tree = export::parse(svg).map_err(|err| anyhow::anyhow!("{err}"))?;
        export::rasterize(&tree, scale).map_err(|err| anyhow::anyhow!("{err}"))
    };
    let delay = match film {
        Film::Gif => u16::try_from((100 / fps).max(2)).unwrap_or(5),
        Film::Apng | Film::Webp | Film::Page => 1,
    };
    let mut frames = Frames::default();
    let tick = 1.0 / fps as f32;
    let mut t = 0.0;
    while t < pen.end + DRAWN {
        let pixmap = raster(&pen.at(t))?;
        frames.push(
            pixmap.width() as usize,
            pixmap.height() as usize,
            pixmap.data(),
            delay,
        );
        t += tick;
    }
    // The ink: the sketch drawn fades into the clean picture.
    let drawn = raster(&sketch)?;
    let inked = raster(&clean)?;
    anyhow::ensure!(
        drawn.width() == inked.width() && drawn.height() == inked.height(),
        "the sketch and the clean picture are not of one size: the ink would not lie on the sketch"
    );
    let steps = (INK * fps as f32).ceil() as usize;
    for step in 1..=steps {
        let k = step as f32 / steps as f32;
        let k = k * k * (3.0 - 2.0 * k);
        let mixed: Vec<u8> = drawn
            .data()
            .iter()
            .zip(inked.data())
            .map(|(a, b)| (f32::from(*a) * (1.0 - k) + f32::from(*b) * k).round() as u8)
            .collect();
        frames.push(
            drawn.width() as usize,
            drawn.height() as usize,
            &mixed,
            delay,
        );
    }
    let hold = (INKED * fps as f32).ceil() as usize;
    for _ in 0..hold {
        frames.push(
            inked.width() as usize,
            inked.height() as usize,
            inked.data(),
            delay,
        );
    }
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

/// The tags the pen draws; those inside these are not drawn themselves.
const DRAWN_TAGS: [&str; 8] = [
    "path", "rect", "circle", "ellipse", "line", "polyline", "polygon", "text",
];
const UNDRAWN: [&str; 7] = [
    "defs", "clipPath", "mask", "marker", "pattern", "symbol", "style",
];

impl Pen {
    /// The pen's way through `svg`: each line it draws in the order the
    /// picture has them, as fast as [`SPEED`], each fill and word when it
    /// gets there.
    pub(super) fn of(svg: &str) -> Result<Pen> {
        let doc = roxmltree::Document::parse(svg).context("the sketch is no SVG")?;
        // The paper, a rectangle before the first group, is there before
        // the pen.
        let first_group = svg.find("<g").unwrap_or(svg.len());
        let mut found: Vec<(std::ops::Range<usize>, bool, bool)> = Vec::new();
        for node in doc.descendants().filter(roxmltree::Node::is_element) {
            let name = node.tag_name().name();
            if name == "rect" && node.range().start < first_group {
                continue;
            }
            if !DRAWN_TAGS.contains(&name)
                || node
                    .ancestors()
                    .any(|a| UNDRAWN.contains(&a.tag_name().name()))
            {
                continue;
            }
            let start = node.range().start;
            let Some(end) = tag_end(svg, start) else {
                continue;
            };
            let stroked = name != "text"
                && node.attribute("stroke").is_some_and(|s| s != "none")
                && node.attribute("stroke-dasharray").is_none();
            found.push((start..end, stroked, name == "text"));
        }
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
        let mut lengths = vec![0.0f32; found.len()];
        let mut tops = vec![f32::MAX; found.len()];
        measure(tree.root(), &mut lengths, &mut tops);
        let total: f32 = found
            .iter()
            .zip(&lengths)
            .filter(|((_, stroked, _), _)| *stroked)
            .map(|(_, length)| *length)
            .sum();
        let time = (total / SPEED).clamp(QUICKEST, SLOWEST);
        // The pen goes down the picture as a hand would, not layer by
        // layer as the picture is written: a box, its words and the lines
        // that leave it together; in the picture's order within a band.
        let mut order: Vec<usize> = (0..found.len()).collect();
        let band = |i: usize| (tops[i] / BAND).floor() as i64;
        order.sort_by_key(|&i| (band(i), i));
        let mut strokes = vec![Stroke::Comes { at: 0.0 }; found.len()];
        let mut cursor = 0.0f32;
        for i in order {
            let (_, stroked, _) = &found[i];
            let length = lengths[i];
            strokes[i] = if *stroked && length > 0.0 {
                let took = (length / total.max(1.0) * time).max(0.02);
                let drawn = Stroke::Drawn {
                    start: cursor,
                    end: cursor + took,
                    length,
                };
                cursor += took;
                drawn
            } else {
                Stroke::Comes { at: cursor }
            };
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
                Stroke::Drawn { start, end, length } => {
                    let k = ((t - start) / (end - start).max(0.001)).clamp(0.0, 1.0);
                    if k <= 0.0 {
                        vec![("opacity", "0".to_owned())]
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

/// The lengths of the lines usvg read, and where each element's top is,
/// by the mark of their element.
fn measure(group: &export::usvg::Group, lengths: &mut [f32], tops: &mut [f32]) {
    let index = |id: &str| {
        id.strip_prefix("pen-")
            .and_then(|n| n.parse::<usize>().ok())
    };
    for node in group.children() {
        match node {
            export::usvg::Node::Group(inner) => measure(inner, lengths, tops),
            export::usvg::Node::Path(path) => {
                if let Some(index) = index(path.id()).filter(|&i| i < lengths.len()) {
                    lengths[index] += length_of(path.data());
                    tops[index] = tops[index].min(path.abs_bounding_box().top());
                }
            }
            export::usvg::Node::Text(text) => {
                if let Some(index) = index(text.id()).filter(|&i| i < tops.len()) {
                    tops[index] = tops[index].min(text.abs_bounding_box().top());
                }
            }
            export::usvg::Node::Image(_) => {}
        }
    }
}

/// How long a path is, its curves as runs of short lines.
fn length_of(path: &tiny_skia::Path) -> f32 {
    use tiny_skia::PathSegment;
    let mut length = 0.0;
    let mut at = tiny_skia::Point::zero();
    let mut first = at;
    let dist = |a: tiny_skia::Point, b: tiny_skia::Point| (b.x - a.x).hypot(b.y - a.y);
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
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
    length
}

/// Where the start tag that begins at `start` ends, past its `>`; quotes
/// read as such.
fn tag_end(svg: &str, start: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in svg[start..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(start + i + 1),
            _ => {}
        }
    }
    None
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
) -> Result<usize> {
    use omaspec_render::export::ink as page;
    let ink = page::Ink {
        sketch: pen.svg.clone(),
        clean: clean.to_owned(),
        strokes: pen
            .elements
            .iter()
            .map(|e| match e.stroke {
                Stroke::Drawn { start, end, .. } => page::Stroke::Drawn { start, end },
                Stroke::Comes { at } => page::Stroke::Comes { at },
            })
            .collect(),
        end: pen.end,
        drawn: DRAWN,
        ink: INK,
        inked: INKED,
        fade: FADE,
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
            steps: 100,
            width: Some(300),
            fps: Some(4),
            folds: Vec::new(),
            dives: Vec::new(),
            builds: false,
            lockstep: false,
            history: false,
            traffic: Vec::new(),
            themes: None,
            ink: true,
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
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn the_pen_draws_each_line_as_long_as_it_is() {
        let pen = Pen::of(SKETCH).unwrap();
        let strokes: Vec<&Stroke> = pen.elements.iter().map(|e| &e.stroke).collect();
        // The paper is there before the pen.
        assert_eq!(strokes.len(), 4);
        let Stroke::Drawn { start, end, length } = *strokes[0] else {
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
        let pen = Pen::of(SKETCH).unwrap();
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
        assert!(done.contains(r#"stroke-dashoffset="0.00""#));
        // Every frame is an SVG.
        for t in [0.0, start, end, pen.end] {
            assert!(export::parse(&pen.at(t)).is_ok());
        }
    }
}
