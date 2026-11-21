//! Code becomes a picture (`omaspec play FILE --build`): the source on the
//! left, the diagram on the right building itself up line by line. A
//! caret walks down the source; each line it reaches sets down what it
//! draws: a box grows in, a line draws itself to its head and its words
//! come after it, while the lines not reached yet wait, dimmed. A diagram
//! that marks its parts (docs/adr/0029: states, classes, flowcharts,
//! activities) builds part by part, each part at the line that writes it:
//! a state's first line, the first line whose words it holds, else the
//! first that names it; a line at its own, else with its words, else
//! after the boxes it joins. One that marks none (a sequence) builds from
//! the top down, as far as what each line wrote reaches.
//!
//! Each frame is one SVG, the source and the diagram side by side, drawn
//! by resvg: no GPU, the same picture each time. The frames are written
//! as the play's are: a GIF, or an animated PNG or WebP in full color.

use anyhow::{Context, Result, bail};
use omaspec_render::export;
use omaspec_render::{Place, Renderer};

use super::films::{self, Film};
use super::gifs::Frames;
use super::moving::Moving;
use super::{App, Start, syntax};

mod frame;
pub(super) mod picture;

use frame::{Code, Layout};
use picture::Picture;

/// The source's words, in pixels.
const CODE_SIZE: f32 = 13.0;
/// From one source line to the next.
const LINE_HEIGHT: f32 = 20.0;
/// How wide a letter of the code's face is, of its size.
const LETTER: f32 = 0.6;
/// The longest line shown whole; a longer one ends in an ellipsis.
const WIDEST_LINE: usize = 56;
/// Around the source and the diagram, and between them.
const MARGIN: f32 = 24.0;
/// Inside the source's panel.
const INSET: f32 = 14.0;
/// The frame's height at least, and at most.
const LOWEST: f32 = 360.0;
const HIGHEST: f32 = 900.0;
/// The frame's width when none is asked: as wide as the source and the
/// diagram at its size, within these.
const NARROWEST: f32 = 640.0;
const WIDEST: f32 = 1600.0;
/// How dim a line the caret has not reached is.
const AHEAD: f32 = 0.38;
/// How small a box is as it starts to grow in.
const SMALL: f32 = 0.82;

/// Seconds: before the first line, a box growing in, a line drawing
/// itself, the words after it, the pause after a line that draws, a line
/// that draws nothing, and the whole at the end.
const BEFORE: f32 = 0.5;
const GROW: f32 = 0.26;
const DRAW: f32 = 0.34;
const WORDS: f32 = 0.14;
const AFTER: f32 = 0.08;
const PASS: f32 = 0.06;
const HOLD: f32 = 2.2;
/// A line starts to draw itself this far into its boxes' growing.
const OVERLAP: f32 = 0.5;
/// Seconds the build takes at most, the hold not counted: a longer
/// source is gone through faster.
const BUDGET: f32 = 12.0;

/// Writes the build of `args.file` as `film` at `fps` frames a second.
pub(super) fn build(args: &Moving, film: Film, fps: u32) -> Result<String> {
    let text = args.read()?;
    let (_, language) = args.named(&text);
    let omarchy = super::moving::theme(args.theme.as_deref())?;
    let tweaks = omarchy.tweaks.syntax.clone();
    let start = Start {
        files: vec![args.file.clone()],
        folder: None,
        forced: args.forced,
    };
    let (mut app, _) = App::themed(start, None, omarchy);
    app.follow_theme = !args.file_colors;
    let skin = app.pane_skin(args.root.as_deref());
    // Its parts marked, so each comes in on its own.
    let marked = crate::worker::folded(&text, language, &[], crate::worker::Views::default());
    let place = Place::of(&args.file, None);
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let mut maps = crate::specs::Maps::default();
    let rendered = crate::look::render(
        &mut maps,
        &mut renderer,
        &marked,
        language,
        Some(&args.file),
        place.as_ref(),
        skin.as_ref(),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    let picture = Picture::of(&rendered.svg, &rendered.map, &text)?;
    let code = Code::of(&text, language, &app.palette, &tweaks);
    let script = Script::of(&picture, code.lines.len());
    if film == Film::Page {
        let (name, _) = args.named(&text);
        return page(
            args,
            &text,
            &name,
            language,
            &picture,
            &code,
            &script,
            &app.palette,
        );
    }
    let layout = Layout::of(&code, &picture, args.width)?;

    let delay = match film {
        Film::Gif => u16::try_from((100 / fps).max(2)).unwrap_or(5),
        Film::Apng | Film::Webp | Film::Page => 1,
    };
    let mut frames = Frames::default();
    let total = script.length();
    let count = (total * fps as f32).ceil() as usize + 1;
    for frame in 0..count {
        let at = frame as f32 / fps as f32;
        let svg = layout.frame(&code, &picture, &script, at, &app.palette);
        let tree = export::parse(&svg).map_err(|err| anyhow::anyhow!("{err}"))?;
        let pixmap = export::rasterize(&tree, 1.0).map_err(|err| anyhow::anyhow!("{err}"))?;
        frames.push(
            pixmap.width() as usize,
            pixmap.height() as usize,
            pixmap.data(),
            delay,
        );
    }
    super::films::timed(&mut frames, args.timing, film, fps, super::gifs::Rest::Last)?;
    let Some((w, h)) = frames.size() else {
        bail!("no frame was drawn");
    };
    let (bytes, kept) = match film {
        Film::Gif => frames.encode(args.quality)?,
        Film::Apng => films::apng(&frames, fps)?,
        Film::Webp => films::webp(&frames, fps)?,
        Film::Page => unreachable!("a page is written before a frame is drawn"),
    };
    crate::files::write_atomic(&args.output, &bytes)
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(format!(
        "{} → {} ({} frames, {}×{}, built in {:.1} s)",
        args.file.display(),
        args.output.display(),
        kept,
        w,
        h,
        total
    ))
}

/// Writes the build as a page that plays it in vectors
/// ([`omaspec_render::export::building`]): the source in its colors, the
/// picture with its parts marked, and when each line comes and what it
/// sets down. Nothing is drawn in pixels.
#[allow(clippy::too_many_arguments)]
fn page(
    args: &Moving,
    text: &str,
    name: &str,
    language: omaspec_core::Language,
    picture: &Picture,
    code: &Code,
    script: &Script,
    palette: &omaspec_theme::Palette,
) -> Result<String> {
    use omaspec_render::export::building as page;
    let color = |rgb: omaspec_theme::Rgb| omaspec_uml::Color(rgb.r, rgb.g, rgb.b);
    let paper = omaspec_uml::parse_color(&picture.paper)
        .ok()
        .flatten()
        .unwrap_or(omaspec_uml::Color::WHITE);
    // The script lays its mask and its boxes' middles in the picture's own
    // units, which a `scale` sets apart from its size.
    let own_y = |y: f32| picture.own((0.0, y)).1;
    let building = page::Building {
        svg: picture.svg.clone(),
        size: picture.own(picture.size),
        lines: code.runs(palette.foreground),
        starts: script.starts.clone(),
        end: script.end,
        hold: HOLD,
        grow: GROW,
        draw: DRAW,
        words: WORDS,
        overlap: OVERLAP,
        pace: script.pace,
        boxes: script.draws.iter().map(|&(boxes, _)| boxes).collect(),
        parts: picture
            .parts
            .iter()
            .map(|part| page::PartAt {
                prefix: format!("{}-", omaspec_uml::part_id(&part.name)),
                line: part.line,
                middle: picture.own(part.middle),
                words: part.of_wire.is_some(),
            })
            .collect(),
        wires: picture
            .wires
            .iter()
            .map(|wire| page::WireAt {
                line: wire.line,
                points: wire.points.clone(),
                heads: wire.heads.clone(),
                color: omaspec_uml::parse_color(&wire.color)
                    .ok()
                    .flatten()
                    .unwrap_or(omaspec_uml::Color::BLACK),
                width: wire.width,
                dash: wire.dash,
            })
            .collect(),
        reach: picture
            .reach
            .iter()
            .map(|&(line, y)| (line, own_y(y)))
            .collect(),
        first_reach: own_y(picture.first_reach()),
        // Only a picture built by its parts draws its lines anew: one
        // uncovered from the top keeps them as drawn (a chart's).
        lines_matrix: picture
            .lines_group
            .filter(|_| picture.by_parts())
            .and_then(|transform| transform.invert())
            .map(|m| [m.sx, m.ky, m.kx, m.sy, m.tx, m.ty]),
        colors: page::Colors {
            paper: color(palette.background),
            panel: color(palette.dark_background),
            ink: color(palette.foreground),
            muted: color(palette.muted),
            selection: color(palette.selection),
            accent: color(palette.accent),
            picture: paper,
        },
    };
    let written = page::page(
        &building,
        Some(omaspec_render::export::Source {
            code: text,
            language,
            file: Some(name),
        }),
        name.rsplit_once('.').map_or(name, |(stem, _)| stem),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    crate::files::write_atomic(&args.output, written.as_bytes())
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    Ok(format!(
        "{} → {} (a page, {} KB, built in {:.1} s in vectors)",
        args.file.display(),
        args.output.display(),
        written.len().div_ceil(1024),
        script.end
    ))
}

/// When each source line comes, and what it sets down when.
struct Script {
    /// When the caret reaches each line.
    starts: Vec<f32>,
    /// What each line draws: boxes, lines.
    draws: Vec<(bool, bool)>,
    end: f32,
    /// How much faster than its own pace the build goes: 1, or more for a
    /// source too long for [`BUDGET`].
    pace: f32,
}

impl Script {
    fn of(picture: &Picture, lines: usize) -> Script {
        let draws: Vec<(bool, bool)> = (0..lines).map(|line| picture.draws(line)).collect();
        let took = |draws: &(bool, bool)| match *draws {
            (false, false) => PASS,
            (true, false) => GROW + AFTER,
            (false, true) => DRAW + WORDS + AFTER,
            (true, true) => GROW * OVERLAP + DRAW + WORDS + AFTER,
        };
        let natural: f32 = draws.iter().map(took).sum();
        let pace = (natural / BUDGET).max(1.0);
        let mut at = BEFORE;
        let mut starts = Vec::with_capacity(lines);
        for draw in &draws {
            starts.push(at);
            at += took(draw) / pace;
        }
        Script {
            starts,
            draws,
            end: at,
            pace,
        }
    }

    fn length(&self) -> f32 {
        self.end + HOLD
    }

    /// The line the caret stands on at `at`; none before the first.
    fn line(&self, at: f32) -> Option<usize> {
        self.starts.iter().rposition(|&start| start <= at)
    }

    /// How far a box of `line` has grown in at `at`, from 0 to 1.
    fn grown(&self, line: usize, at: f32) -> f32 {
        let Some(&start) = self.starts.get(line) else {
            return 1.0;
        };
        ((at - start) * self.pace / GROW).clamp(0.0, 1.0)
    }

    /// When the lines of `line` start to draw themselves: once its boxes
    /// are partly grown in.
    fn drawing(&self, line: usize) -> f32 {
        let start = self.starts.get(line).copied().unwrap_or(0.0);
        let boxes = self.draws.get(line).is_some_and(|&(boxes, _)| boxes);
        start
            + if boxes {
                GROW * OVERLAP / self.pace
            } else {
                0.0
            }
    }

    /// How far a line of `line` has drawn itself at `at`, from 0 to 1.
    fn drawn(&self, line: usize, at: f32) -> f32 {
        ((at - self.drawing(line)) * self.pace / DRAW).clamp(0.0, 1.0)
    }

    /// How far its words have come in at `at`, from 0 to 1.
    fn worded(&self, line: usize, at: f32) -> f32 {
        ((at - self.drawing(line) - DRAW / self.pace) * self.pace / WORDS).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use omaspec_core::Language;
    use omaspec_theme::{Palette, Rgb};

    use super::frame::{stretch, styled};
    use super::picture::named_at;
    use super::*;

    /// `source` drawn with its parts marked, as a build draws it.
    fn picture(source: &str, language: Language) -> Picture {
        let marked = crate::worker::folded(source, language, &[], crate::worker::Views::default());
        let mut renderer = Renderer::new(&crate::commands::render_config());
        let rendered = renderer.render_at(&marked, language, None).unwrap();
        Picture::of(&rendered.svg, &rendered.map, source).unwrap()
    }

    const DOOR: &str =
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";

    /// A git graph builds lane by lane and commit by commit, each at the
    /// line that makes it, a merge's line with the merge (REQ-037).
    #[test]
    fn a_git_graph_builds_each_lane_and_commit_at_its_own_line() {
        let source = "---\ntitle: Release flow\n---\ngitGraph\n  commit id: \"init\"\n  commit\n  branch develop\n  commit id: \"login\"\n  checkout main\n  merge develop\n";
        let graph = picture(source, Language::Mermaid);
        assert!(graph.by_parts());
        let line_of = |name: &str| {
            graph
                .parts
                .iter()
                .find(|part| part.name == name)
                .map(|part| part.line)
        };
        let lines: Vec<(String, usize)> = graph
            .parts
            .iter()
            .map(|p| (p.name.clone(), p.line))
            .collect();
        assert_eq!(line_of("branch main"), Some(3), "{lines:?}");
        assert_eq!(line_of("commit init"), Some(4), "{lines:?}");
        assert_eq!(line_of("commit main #0"), Some(5), "{lines:?}");
        assert_eq!(line_of("branch develop"), Some(6), "{lines:?}");
        assert_eq!(line_of("commit login"), Some(7), "{lines:?}");
        assert_eq!(line_of("commit main #1"), Some(9), "{lines:?}");
        // So in a look, which lays its skin before the diagram, and in a
        // fence, whose lines come before the diagram's.
        let mut maps = crate::specs::Maps::default();
        let mut renderer = Renderer::new(&crate::commands::render_config());
        let marked = crate::worker::folded(
            source,
            Language::Mermaid,
            &[],
            crate::worker::Views::default(),
        );
        let skin = crate::look::Look::Light.skin();
        let looked = crate::look::render(
            &mut maps,
            &mut renderer,
            &marked,
            Language::Mermaid,
            None,
            None,
            skin.as_ref(),
        )
        .unwrap();
        let looked = Picture::of(&looked.svg, &looked.map, source).unwrap();
        let commit = looked.parts.iter().find(|part| part.name == "commit init");
        assert_eq!(commit.map(|part| part.line), Some(4));
        let fenced = format!("```mermaid\n{marked}```\n");
        let rendered = renderer
            .render_at(&fenced, Language::Mermaid, None)
            .unwrap();
        let line = rendered
            .map
            .written
            .iter()
            .find(|(part, _)| part == "commit init");
        assert_eq!(line.map(|(_, line)| *line), Some(5));
    }

    #[test]
    fn a_machine_builds_each_state_and_line_at_its_own_line() {
        let door = picture(DOOR, Language::PlantUml);
        assert!(door.by_parts());
        let line_of = |name: &str| {
            door.parts
                .iter()
                .find(|part| part.name.ends_with(name))
                .map(|part| part.line)
        };
        assert_eq!(line_of("Closed"), Some(1));
        assert_eq!(line_of("Open"), Some(2));
        let mut lines: Vec<usize> = door.wires.iter().map(|wire| wire.line).collect();
        lines.sort_unstable();
        assert_eq!(lines, [1, 2, 3]);
        // Before its line nothing of Open is drawn; after, all of it.
        let code = Code::of(
            DOOR,
            Language::PlantUml,
            &Palette::fallback(),
            &HashMap::new(),
        );
        let script = Script::of(&door, code.lines.len());
        let layout = Layout::of(&code, &door, None).unwrap();
        let before = layout.built(&door, &script, script.starts[2] - 0.01);
        let after = layout.built(&door, &script, script.end);
        let open = door
            .parts
            .iter()
            .find(|p| p.name.ends_with("Open"))
            .unwrap();
        let (id, _) = &open.groups[0];
        assert!(
            before.contains(&format!(r#"<g id="{id}" opacity="0.000""#)),
            "{before}"
        );
        assert!(!after.contains(&format!(r#"<g id="{id}" opacity="#)));
    }

    #[test]
    fn a_class_comes_with_the_line_that_opens_it() {
        let classes = picture(
            "@startuml\ntitle A B C\nclass B {\n  +x : int\n}\nclass C\nB --> C\n@enduml\n",
            Language::PlantUml,
        );
        // Not with its member (3), not with the title that names it (1).
        let b = classes
            .parts
            .iter()
            .filter(|part| part.of_wire.is_none())
            .map(|part| part.line)
            .min();
        assert_eq!(
            b,
            Some(2),
            "{:?}",
            classes
                .parts
                .iter()
                .map(|p| (&p.name, p.line))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_code_s_letters_are_measured_in_its_face() {
        let code = Code::of(
            "a\n",
            Language::PlantUml,
            &Palette::fallback(),
            &HashMap::new(),
        );
        assert!(
            (CODE_SIZE * 0.4..CODE_SIZE * 0.75).contains(&code.letter),
            "{}",
            code.letter
        );
    }

    #[test]
    fn a_sequence_builds_from_the_top_down() {
        let sequence = picture(
            "@startuml\nAlice -> Bob : hello\nBob -> Alice : hi\n@enduml\n",
            Language::PlantUml,
        );
        assert!(!sequence.by_parts());
        let lines: Vec<usize> = sequence.reach.iter().map(|&(line, _)| line).collect();
        assert!(lines.contains(&1) && lines.contains(&2), "{lines:?}");
        let (first, second) = (
            sequence.reach.iter().find(|r| r.0 == 1).unwrap().1,
            sequence.reach.iter().find(|r| r.0 == 2).unwrap().1,
        );
        assert!(first < second);
    }

    #[test]
    fn a_build_is_written_as_a_picture_that_moves() {
        let work = std::env::temp_dir().join(format!("omaspec-build-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let file = work.join("door.puml");
        std::fs::write(&file, DOOR).unwrap();
        let moving = |name: &str| Moving {
            file: file.clone(),
            text: None,
            output: work.join(name),
            events: Vec::new(),
            steps: 0,
            width: Some(700),
            fps: Some(5),
            folds: Vec::new(),
            dives: Vec::new(),
            builds: true,
            lockstep: false,
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
            renderer: None,
            quality: super::super::gifs::Quality::default(),
        };
        let told = super::super::moving::play(&moving("door.webp")).unwrap();
        assert!(told.contains("built in"), "{told}");
        let bytes = std::fs::read(work.join("door.webp")).unwrap();
        let webp = image_webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!(webp.dimensions().0, 700);
        assert!(webp.num_frames() >= 4, "{}", webp.num_frames());
        // As a page: its source, its picture and its play.
        let told = super::super::moving::play(&moving("door.html")).unwrap();
        assert!(told.contains("a page"), "{told}");
        let page = std::fs::read_to_string(work.join("door.html")).unwrap();
        assert!(page.contains(r#""kind":"build""#));
        assert!(page.contains("Closed --&gt; Open"), "{page}");
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn a_sequence_with_a_picture_inside_keeps_the_whole() {
        // A note that draws a diagram holds an `<svg>` of its own.
        let source = "@startuml\nAlice -> Bob : hello\nnote right\n  {{\n  B -> C : inside\n  }}\nend note\n@enduml\n";
        let sequence = picture(source, Language::PlantUml);
        assert!(!sequence.by_parts() && sequence.svg.contains("<svg x="));
        let code = Code::of(
            source,
            Language::PlantUml,
            &Palette::fallback(),
            &HashMap::new(),
        );
        let script = Script::of(&sequence, code.lines.len());
        let layout = Layout::of(&code, &sequence, None).unwrap();
        let frame = layout.frame(&code, &sequence, &script, script.end, &Palette::fallback());
        assert!(
            frame.contains("build-reveal") && frame.contains(">hello<"),
            "{frame}"
        );
    }

    #[test]
    fn a_scaled_picture_is_built_in_its_size() {
        use super::picture::{paper_of, root_of};
        let root = root_of(
            r#"<svg width="200" height="100" viewBox="0 0 100 50">"#,
            (200.0, 100.0),
        );
        assert_eq!((root.sx, root.sy, root.tx, root.ty), (2.0, 2.0, 0.0, 0.0));
        assert!(root_of(r#"<svg width="9" height="9">"#, (9.0, 9.0)).is_identity());
        let scaled = picture(
            "@startuml\nscale 2\n[*] --> Closed\nClosed --> Open : open\n@enduml\n",
            Language::PlantUml,
        );
        assert!((scaled.root.sx - 2.0).abs() < 0.05, "{:?}", scaled.root);
        let code = Code::of(
            DOOR,
            Language::PlantUml,
            &Palette::fallback(),
            &HashMap::new(),
        );
        let script = Script::of(&scaled, code.lines.len());
        let layout = Layout::of(&code, &scaled, None).unwrap();
        assert!(
            layout
                .built(&scaled, &script, 0.0)
                .starts_with(r#"<g transform="matrix(2"#)
        );
        // The paper is the rectangle under everything, else white.
        assert_eq!(
            paper_of(r##"<svg><rect x="0" fill="#123456"/><g/></svg>"##),
            "#123456"
        );
        assert_eq!(
            paper_of(r##"<svg><g><rect fill="#123456"/></g></svg>"##),
            "#FFFFFF"
        );
    }

    #[test]
    fn a_line_is_drawn_as_far_as_its_share() {
        let points = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        assert_eq!(stretch(&points, 0.25), [(0.0, 0.0), (5.0, 0.0)]);
        assert_eq!(
            stretch(&points, 0.75),
            [(0.0, 0.0), (10.0, 0.0), (10.0, 5.0)]
        );
        assert_eq!(stretch(&points, 1.0), points);
    }

    #[test]
    fn a_name_is_found_as_a_word_of_its_own() {
        let source = ["state Opening", "Open --> Closed", "Opening --> Open"];
        assert_eq!(named_at("Open", &source), Some(1));
        assert_eq!(named_at("Opening", &source), Some(0));
        assert_eq!(named_at("Shut", &source), None);
    }

    #[test]
    fn a_long_line_ends_in_an_ellipsis() {
        let line = "x".repeat(80);
        let (words, shown) = styled(&line, &[], Rgb::new(0, 0, 0));
        assert_eq!(shown, WIDEST_LINE + 1);
        assert!(words.contains('…'));
        let (_, shown) = styled("\ta", &[], Rgb::new(0, 0, 0));
        assert_eq!(shown, 5);
    }

    #[test]
    fn the_script_waits_on_lines_that_draw() {
        let picture = Picture {
            svg: String::new(),
            size: (100.0, 100.0),
            paper: "#FFFFFF".into(),
            parts: Vec::new(),
            wires: Vec::new(),
            reach: vec![(1, 40.0)],
            lines_group: None,
            root: export::usvg::Transform::identity(),
        };
        let script = Script::of(&picture, 3);
        assert_eq!(script.starts[0], BEFORE);
        assert!((script.starts[1] - (BEFORE + PASS)).abs() < 1e-5);
        assert!((script.starts[2] - (BEFORE + PASS + GROW + AFTER)).abs() < 1e-5);
        // A long source goes faster, within the budget.
        let reach = (0..400).map(|line| (line, line as f32)).collect();
        let long = Script::of(&Picture { reach, ..picture }, 400);
        assert!(long.end - BEFORE <= BUDGET + 1e-3, "{}", long.end);
        assert!(long.pace > 1.0);
        assert_eq!(script.line(0.0), None);
        assert_eq!(script.line(script.starts[1]), Some(1));
    }
}
