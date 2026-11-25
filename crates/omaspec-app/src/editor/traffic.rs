//! Traffic: many recorded traces flowing through a state machine at once
//! (`omaspec play order.puml --traffic week.jsonl -o traffic.gif`). Each
//! object of the traces is a dot that rests in the state it is in and runs
//! along the transition each of its lines takes; a transition glows the
//! brighter the more dots have passed it, and once all have flowed, those
//! no dot took blink in grey. A line the machine cannot take turns its dot
//! red where it stands. What coverage counts, moving.
//!
//! The ways are those `omaspec trace` follows ([`omaspec_model::follow_traces`]),
//! so a red dot is a deviation the check reports. The traces flow side by
//! side, each from its first line to its last in the same time; the
//! frames are drawn without a GPU ([`frame`]), and as `.html` a page plays
//! the same in vectors ([`omaspec_render::export::traffic`]).

use anyhow::{Context, Result, bail};
use omaspec_core::Language;
use omaspec_model::play::travel_millis;
use omaspec_model::{Placed, Spec};
use omaspec_render::export;
use omaspec_render::{Place, Renderer};

use super::films::Film;
use super::gifs::Frames;
use super::moving::Moving;
use super::{App, Start};

mod frame;

/// When the first lines flow, and how long all of each trace takes.
const START: f32 = 0.5;
const FLOW: f32 = 10.0;
/// How long a dot takes along a transition, at most, by its length.
const TRAVEL: f32 = 0.7;
/// After the last dot: how long the ways never taken blink, and how long
/// the end stays after.
const BLINK: f32 = 2.0;
const HOLD: f32 = 1.2;

/// Where everything flows, and when.
pub(super) struct Flow {
    /// By transition: its route, and when dots set off along it and how
    /// long they take.
    pub(super) wires: Vec<Wire>,
    pub(super) dots: Vec<Dot>,
    /// When the last dot has arrived.
    pub(super) end: f32,
    /// What the caption counts: the objects, and the machine's name.
    pub(super) objects: usize,
    pub(super) machine: String,
}

pub(super) struct Wire {
    pub(super) points: Vec<(f32, f32)>,
    pub(super) passes: Vec<(f32, f32)>,
}

pub(super) struct Dot {
    pub(super) legs: Vec<Leg>,
    /// When it took a line its machine cannot take.
    pub(super) wrong: Option<f32>,
}

/// A way a dot runs: from where it rested, along a transition, to where
/// it rests next.
pub(super) struct Leg {
    pub(super) start: f32,
    pub(super) took: f32,
    pub(super) points: Vec<(f32, f32)>,
}

impl Flow {
    /// How often wire `w` has been passed by `t`, a pass on its way
    /// counting as far as it has come.
    pub(super) fn passed(&self, w: usize, t: f32) -> f32 {
        self.wires[w]
            .passes
            .iter()
            .map(|&(start, took)| ((t - start) / took.max(0.01)).clamp(0.0, 1.0))
            .sum()
    }

    /// The most passes any wire has in the end.
    pub(super) fn most(&self) -> f32 {
        self.wires
            .iter()
            .map(|w| w.passes.len())
            .max()
            .unwrap_or(0)
            .max(1) as f32
    }

    pub(super) fn length(&self) -> f32 {
        self.end + BLINK + HOLD
    }
}

impl Dot {
    /// Where the dot is at `t`; none before it comes.
    pub(super) fn at(&self, t: f32) -> Option<(f32, f32)> {
        let leg = self.legs.iter().rev().find(|leg| leg.start <= t)?;
        let k = ((t - leg.start) / leg.took.max(0.01)).clamp(0.0, 1.0);
        Some(along(&leg.points, ease(k)))
    }
}

/// The point `share` of the way along `points`.
pub(super) fn along(points: &[(f32, f32)], share: f32) -> (f32, f32) {
    let runs: Vec<f32> = points
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .collect();
    let mut left = runs.iter().sum::<f32>() * share.clamp(0.0, 1.0);
    for (w, run) in points.windows(2).zip(&runs) {
        if *run >= left {
            let k = if *run > 0.0 { left / run } else { 0.0 };
            return (
                w[0].0 + (w[1].0 - w[0].0) * k,
                w[0].1 + (w[1].1 - w[0].1) * k,
            );
        }
        left -= run;
    }
    points.last().copied().unwrap_or((0.0, 0.0))
}

pub(super) fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Writes the traffic of `args.traffic` through the machine of
/// `args.file`; what it played.
pub(super) fn traffic(args: &Moving, film: Film, fps: u32) -> Result<String> {
    let text = args.read()?;
    let (name, language) = args.named(&text);
    let Some(spec) = Spec::read(&text, language, &name) else {
        bail!(
            "{} is no state machine the readers understand: traffic flows through one",
            args.file.display()
        );
    };
    let traces: Vec<String> = args
        .traffic
        .iter()
        .map(|path| {
            std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))
        })
        .collect::<Result<_>>()?;
    let start = Start {
        files: vec![args.file.clone()],
        folder: None,
        forced: args.forced,
    };
    let mut app = super::moving::film_app(args, start)?;
    let rendered = draw(&mut app, args, &text, language)?;
    let mut placed = spec.machine.place(&rendered.map);
    in_units(&mut placed, &rendered.svg);
    let flow = flow(&spec, &placed, &traces);
    anyhow::ensure!(
        flow.objects > 0,
        "no object of {} goes by the state machine of {} (an object goes by the machine named like it, or the one its `machine` names: {})",
        args.traffic
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        args.file.display(),
        spec.names.join(", ")
    );
    let deviations = flow.dots.iter().filter(|d| d.wrong.is_some()).count();
    // As the caption counts them: a transition drawn as a line.
    let routed = flow.wires.iter().filter(|w| w.points.len() >= 2);
    let taken = routed.clone().filter(|w| !w.passes.is_empty()).count();
    let routed = routed.count();
    let told = format!(
        "{} objects, {taken} of {routed} transitions taken, {}",
        flow.objects,
        match deviations {
            0 => "no deviation".to_owned(),
            1 => "1 deviation".to_owned(),
            n => format!("{n} deviations"),
        }
    );
    let colors = frame::Colors::of(&app.palette, &rendered.svg);
    if film == Film::Page {
        let written = page(args, &text, &name, language, &rendered.svg, &flow, &colors)?;
        return Ok(format!(
            "{} → {} (a page, {} KB, {told} in vectors)",
            args.file.display(),
            args.output.display(),
            written.div_ceil(1024)
        ));
    }
    let stage = frame::Stage::of(&rendered.svg, args.width.map(|w| w as f32), &colors);
    let delay = match film {
        Film::Gif => u16::try_from((100 / fps).max(2)).unwrap_or(5),
        Film::Apng | Film::Webp | Film::Page => 1,
    };
    let mut frames = Frames::default();
    let count = (flow.length() * fps as f32).ceil() as usize + 1;
    for at in 0..count {
        let svg = stage.frame(&flow, at as f32 / fps as f32);
        let tree = export::parse(&svg).map_err(|err| anyhow::anyhow!("{err}"))?;
        let pixmap =
            export::rasterize(&tree, stage.scale).map_err(|err| anyhow::anyhow!("{err}"))?;
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
        h,
    ))
}

/// The machine drawn in the theme's colors, as the pane draws it.
fn draw(
    app: &mut App,
    args: &Moving,
    text: &str,
    language: Language,
) -> Result<omaspec_render::Rendered> {
    let skin = app.pane_skin(args.root.as_deref());
    let place = Place::of(&args.file, None);
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let mut maps = crate::specs::Maps::default();
    crate::look::render(
        &mut maps,
        &mut renderer,
        text,
        language,
        Some(&args.file),
        place.as_ref(),
        skin.as_ref(),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))
}

/// Every object of `traces` that goes by `spec`'s machine, followed
/// through it as `omaspec trace` follows it, laid out in time: each
/// trace from its first line to its last in [`FLOW`].
fn flow(spec: &Spec, placed: &Placed, traces: &[String]) -> Flow {
    let mut wires: Vec<Wire> = placed
        .routes
        .iter()
        .map(|route| Wire {
            points: route.clone().unwrap_or_default(),
            passes: Vec::new(),
        })
        .collect();
    let mut dots = Vec::new();
    let mut objects = 0;
    let mut end: f32 = START;
    for trace in traces {
        let lines = trace.lines().count().max(1) as f32;
        let when = |line: usize| START + FLOW * line as f32 / lines;
        // Each trace read once, however many objects it has.
        for (_, followed) in omaspec_model::follow_traces(trace, spec) {
            // Counted over all the traces: the dots of two traces in one
            // state lie apart too.
            let ordinal = objects;
            objects += 1;
            let rest = |states: &[usize]| -> Option<(f32, f32)> {
                let spot = states
                    .iter()
                    .find_map(|&s| placed.states.get(s).and_then(Option::as_ref))?;
                Some(spread(spot.rect, ordinal))
            };
            let mut dot = Dot {
                legs: Vec::new(),
                wrong: None,
            };
            let first = followed.steps.first().map_or(START, |s| when(s.line));
            // From the machine's start to where it begins, just before its
            // first line.
            let mut here = rest(&followed.begins);
            if let Some(to) = here {
                // There by the time its first line comes.
                let start = (first - TRAVEL).max(0.0);
                let travel = (first - start).max(0.01);
                let route = followed
                    .start
                    .iter()
                    .find_map(|t| placed.routes[t.transition].clone());
                let mut points = route.clone().unwrap_or_else(|| vec![to]);
                points.push(to);
                for taken in &followed.start {
                    if !wires[taken.transition].points.is_empty() {
                        wires[taken.transition].passes.push((start, travel));
                    }
                }
                dot.legs.push(Leg {
                    start,
                    took: if route.is_some() { travel } else { 0.01 },
                    points,
                });
            }
            for (index, step) in followed.steps.iter().enumerate() {
                let at = when(step.line);
                // A leg ends before the object's next line sets off the
                // next: the dot does not leap from halfway along.
                let gap = followed
                    .steps
                    .get(index + 1)
                    .map_or(f32::MAX, |next| (when(next.line) - at) * 0.9);
                if dot.wrong.is_none()
                    && step
                        .finding
                        .as_ref()
                        .is_some_and(|f| f.severity == omaspec_core::lint::Severity::Error)
                {
                    dot.wrong = Some(at);
                }
                if dot.wrong.is_some() {
                    continue;
                }
                let route = step
                    .taken
                    .iter()
                    .find_map(|t| placed.routes[t.transition].clone());
                let took = route
                    .as_deref()
                    .map_or(0.3, |route| {
                        (travel_millis(route) as f32 / 1000.0).min(TRAVEL)
                    })
                    .min(gap)
                    .max(0.01);
                for taken in &step.taken {
                    if !wires[taken.transition].points.is_empty() {
                        wires[taken.transition].passes.push((at, took));
                    }
                }
                let next = rest(&step.at);
                let mut points: Vec<(f32, f32)> = here.into_iter().collect();
                points.extend(route.unwrap_or_default());
                points.extend(next);
                if points.len() >= 2 {
                    dot.legs.push(Leg {
                        start: at,
                        took,
                        points,
                    });
                    end = end.max(at + took);
                }
                here = next.or(here);
            }
            if let Some(wrong) = dot.wrong {
                end = end.max(wrong + 0.6);
            }
            dots.push(dot);
        }
    }
    Flow {
        wires,
        dots,
        end,
        objects,
        machine: spec.names.first().cloned().unwrap_or_default(),
    }
}

/// `placed` in the units of `svg`'s `viewBox`, where the dots and the
/// lines are drawn over it. Its map is in the picture's pixels: a `scale`
/// makes those larger or smaller than a unit.
fn in_units(placed: &mut Placed, svg: &str) {
    let Some((x, y, w, h)) = frame::view_box(svg) else {
        return;
    };
    let Ok(size) = export::parse(svg).map(|tree| tree.size()) else {
        return;
    };
    let (kx, ky) = (size.width() / w, size.height() / h);
    if (kx - 1.0).abs() < 1e-4 && (ky - 1.0).abs() < 1e-4 && x == 0.0 && y == 0.0 {
        return;
    }
    let unit = |(px, py): (f32, f32)| (x + px / kx, y + py / ky);
    for route in placed.routes.iter_mut().flatten() {
        for point in route.iter_mut() {
            *point = unit(*point);
        }
    }
    for spot in placed.states.iter_mut().flatten() {
        let (left, top) = unit((spot.rect.x, spot.rect.y));
        spot.rect.x = left;
        spot.rect.y = top;
        spot.rect.w /= kx;
        spot.rect.h /= ky;
    }
}

/// Where the dot `ordinal` rests in a box at `rect`: spread around its
/// middle on a spiral, so many in one state do not lie on one another.
fn spread(rect: omaspec_uml::graph::Rect, ordinal: usize) -> (f32, f32) {
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    if ordinal == 0 {
        return (cx, cy);
    }
    let k = ordinal as f32;
    let angle = k * 2.399_963;
    let reach = ((k % 9.0) + 1.0) / 9.0;
    let (rx, ry) = ((rect.w / 2.0 - 6.0).max(0.0), (rect.h / 2.0 - 5.0).max(0.0));
    (cx + angle.cos() * rx * reach, cy + angle.sin() * ry * reach)
}

/// Writes the page; how large it is.
#[allow(clippy::too_many_arguments)]
fn page(
    args: &Moving,
    text: &str,
    name: &str,
    language: Language,
    svg: &str,
    flow: &Flow,
    colors: &frame::Colors,
) -> Result<usize> {
    use omaspec_render::export::traffic as page;
    let traffic = page::Traffic {
        svg: svg.to_owned(),
        wires: flow
            .wires
            .iter()
            .map(|w| page::Wire {
                points: w.points.clone(),
                passes: w.passes.clone(),
            })
            .collect(),
        dots: flow
            .dots
            .iter()
            .map(|d| page::Dot {
                legs: d
                    .legs
                    .iter()
                    .map(|l| page::Leg {
                        start: l.start,
                        took: l.took,
                        points: l.points.clone(),
                    })
                    .collect(),
                wrong: d.wrong,
            })
            .collect(),
        end: flow.end,
        blink: BLINK,
        hold: HOLD,
        caption: frame::caption(flow, flow.length()).0,
        objects: flow.objects,
        machine: flow.machine.clone(),
        colors: colors.page(),
    };
    let written = page::page(
        &traffic,
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

    /// Three objects of two traces flow through a door: the transitions
    /// they take and the one that deviates are counted, as a GIF and as a
    /// page; traces of no object of the machine are refused.
    #[test]
    fn traces_flow_through_their_machine_at_once() {
        let work = std::env::temp_dir().join(format!("omaspec-traffic-{}", std::process::id()));
        std::fs::create_dir_all(&work).unwrap();
        let door = work.join("door.puml");
        std::fs::write(
            &door,
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n",
        )
        .unwrap();
        let one = work.join("one.jsonl");
        std::fs::write(
            &one,
            "{\"object\":\"front\",\"machine\":\"door\",\"event\":\"open\"}\n{\"object\":\"back\",\"machine\":\"door\",\"event\":\"lock\"}\n{\"object\":\"front\",\"machine\":\"door\",\"event\":\"close\"}\n",
        )
        .unwrap();
        let two = work.join("two.jsonl");
        std::fs::write(
            &two,
            "{\"object\":\"side\",\"machine\":\"door\",\"event\":\"open\"}\n{\"object\":\"side\",\"machine\":\"door\",\"event\":\"lock\"}\n",
        )
        .unwrap();
        let moving = |traces: Vec<std::path::PathBuf>, output: &str| Moving {
            file: door.clone(),
            text: None,
            output: work.join(output),
            events: Vec::new(),
            steps: Some(100),
            width: Some(400),
            fps: Some(4),
            folds: Vec::new(),
            dives: Vec::new(),
            builds: false,
            bare: false,
            no_bar: false,
            lockstep: false,
            history: false,
            travel: None,
            traffic: traces,
            themes: None,
            ink: false,
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
        let told = super::super::moving::play(&moving(vec![one.clone(), two.clone()], "door.gif"))
            .unwrap();
        assert!(
            told.contains("3 objects, 4 of 5 transitions taken, 1 deviation"),
            "{told}"
        );
        let gif = std::fs::read(work.join("door.gif")).unwrap();
        // As wide as asked, though the door is narrow.
        assert_eq!(u16::from_le_bytes([gif[6], gif[7]]), 400);
        let told = super::super::moving::play(&moving(vec![one, two], "door.html")).unwrap();
        assert!(told.contains("a page"), "{told}");
        let page = std::fs::read_to_string(work.join("door.html")).unwrap();
        assert!(page.contains(r#""kind":"traffic""#));
        assert_eq!(page.matches(r#""legs":"#).count(), 3);

        let nobody = work.join("nobody.jsonl");
        std::fs::write(&nobody, "{\"object\":\"Car\",\"event\":\"park\"}\n").unwrap();
        let refused = super::super::moving::play(&moving(vec![nobody], "no.gif"))
            .unwrap_err()
            .to_string();
        assert!(refused.contains("no object of"), "{refused}");
        let _ = std::fs::remove_dir_all(&work);
    }

    /// The door drawn natively, and found on its picture in the units of
    /// its `viewBox`.
    fn door(head: &str) -> (Spec, Placed, String) {
        let text = format!(
            "@startuml\n{head}\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n"
        );
        let spec = Spec::read(&text, Language::PlantUml, "door.puml").unwrap();
        let mut renderer = Renderer::new(&omaspec_render::Config {
            java_fallback: false,
            ..omaspec_render::Config::default()
        });
        let rendered = renderer.render(&text, Language::PlantUml).unwrap();
        let mut placed = spec.machine.place(&rendered.map);
        in_units(&mut placed, &rendered.svg);
        (spec, placed, rendered.svg)
    }

    /// With `scale 2` the map is in pixels twice a unit: the dots and the
    /// lines still lie on the machine.
    #[test]
    fn a_scaled_machine_has_its_traffic_on_it() {
        let (_, plain, _) = door("");
        let (_, placed, svg) = door("scale 2");
        let (_, _, w, h) = frame::view_box(&svg).unwrap();
        let routes: Vec<&(f32, f32)> = placed.routes.iter().flatten().flatten().collect();
        assert!(!routes.is_empty());
        for &&(x, y) in &routes {
            assert!(x <= w + 0.5 && y <= h + 0.5, "({x}, {y}) off {w}×{h}");
        }
        for (scaled, plain) in placed.states.iter().zip(&plain.states) {
            if let (Some(scaled), Some(plain)) = (scaled, plain) {
                assert!((scaled.rect.w - plain.rect.w).abs() < 0.5, "{scaled:?}");
                assert!(scaled.rect.x + scaled.rect.w <= w + 0.5);
            }
        }
    }

    /// Two traces' objects in one state rest apart, and an object whose
    /// lines come close after one another ends each leg before the next.
    #[test]
    fn dots_rest_apart_and_end_their_legs() {
        let (spec, placed, _) = door("");
        let line = |object: &str, event: &str| {
            format!("{{\"object\":\"{object}\",\"machine\":\"door\",\"event\":\"{event}\"}}\n")
        };
        let flow = super::flow(&spec, &placed, &[line("a", "open"), line("b", "open")]);
        let rests: Vec<_> = flow.dots.iter().map(|d| d.at(100.0).unwrap()).collect();
        assert_eq!(rests.len(), 2);
        assert_ne!(rests[0], rests[1]);

        let busy: String = (0..60)
            .map(|n| line("a", if n % 2 == 0 { "open" } else { "close" }))
            .collect();
        let flow = super::flow(&spec, &placed, &[busy]);
        let legs = &flow.dots[0].legs;
        assert!(legs.len() > 50);
        for pair in legs.windows(2) {
            assert!(
                pair[0].start + pair[0].took <= pair[1].start + 1e-4,
                "{} + {} > {}",
                pair[0].start,
                pair[0].took,
                pair[1].start
            );
        }
    }

    #[test]
    fn a_dot_runs_along_its_leg_and_rests_at_its_end() {
        let dot = Dot {
            legs: vec![Leg {
                start: 1.0,
                took: 1.0,
                points: vec![(0.0, 0.0), (10.0, 0.0)],
            }],
            wrong: None,
        };
        assert_eq!(dot.at(0.5), None);
        assert_eq!(dot.at(1.5), Some((5.0, 0.0)));
        assert_eq!(dot.at(9.0), Some((10.0, 0.0)));
    }
}
