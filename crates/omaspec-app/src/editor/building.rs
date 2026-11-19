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
mod picture;

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
    let layout = Layout::of(&code, &picture, args.width)?;
    let script = Script::of(&picture, code.lines.len());

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
    let Some((w, h)) = frames.size() else {
        bail!("no frame was drawn");
    };
    let (bytes, kept) = match film {
        Film::Gif => frames.encode(args.quality)?,
        Film::Apng => films::apng(&frames, fps)?,
        Film::Webp => films::webp(&frames, fps)?,
        Film::Page => bail!("a build is written as .gif, .webp or .png"),
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
        let err = super::super::moving::play(&moving("door.html"))
            .unwrap_err()
            .to_string();
        assert!(err.contains(".gif"), "{err}");
        let _ = std::fs::remove_dir_all(&work);
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
