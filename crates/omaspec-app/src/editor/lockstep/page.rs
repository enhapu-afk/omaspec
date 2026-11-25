//! A lockstep as a page in vectors ([`omaspec_render::export::lockstep`]):
//! each picture drawn in the theme's colors, and when everything happens
//! worked out here, on the beat the panes keep: a message's light runs to
//! its receiver, and as it arrives, the receiver's machine takes its step.
//! Nothing is drawn in pixels, so it needs no GPU.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use omaspec_core::Language;
use omaspec_model::play::travel_millis;
use omaspec_model::{Lockstep, Machine, Placed, TraceStep};
use omaspec_render::export::lockstep as page;
use omaspec_render::{Place, Renderer};
use omaspec_uml::Color;

use super::super::Start;
use super::super::moving::Moving;

/// When the first message sets off.
const START: f32 = 0.6;
/// How long a group's sending players back or on takes before the
/// message after it.
const GROUP: f32 = 0.5;
/// How long a step's end stays before the next message sets off.
const BEAT: f32 = 0.55;
/// How long a flash where a machine cannot follow takes.
const FLASH: f32 = 1.1;
/// How long a state's light takes to come on, half of it: a step that
/// only lights where a machine can be waits that long for the next.
const LIT: f32 = 0.21;
/// How long the end stays before the page starts again.
const HOLD: f32 = 1.8;

/// Writes the page of the lockstep `step` of the scenario `text`, its
/// participants' machines drawn from `machines`; what it wrote.
pub(super) fn write(
    args: &Moving,
    text: &str,
    language: Language,
    step: &Lockstep,
    machines: &[(PathBuf, String, Language)],
) -> Result<String> {
    let start = Start {
        files: vec![args.file.clone()],
        folder: None,
        forced: args.forced,
    };
    let app = super::super::moving::film_app(args, start)?;
    let skin = app.pane_skin(args.root.as_deref());
    let palette = app.palette.clone();
    let mut renderer = Renderer::new(&crate::commands::render_config());
    let mut maps = crate::specs::Maps::default();
    let mut draw = |file: &Path, text: &str, language: Language| {
        let place = Place::of(file, None);
        crate::look::render(
            &mut maps,
            &mut renderer,
            text,
            language,
            Some(file),
            place.as_ref(),
            skin.as_ref(),
        )
        .map_err(|err| anyhow::anyhow!("{}: {err}", file.display()))
    };
    let scenario = draw(&args.file, text, language)?;
    let mut drawn = Vec::new();
    for (file, text, language) in machines {
        drawn.push(draw(file, text, *language)?);
    }

    let wrong: Vec<usize> = step
        .players
        .iter()
        .flat_map(|p| &p.followed.steps)
        .filter(|s| s.finding.is_some())
        .map(|s| s.line)
        .collect();
    let widest = drawn.iter().map(|d| svg_size(&d.svg).0).fold(1.0, f32::max);
    let mut timelines: Vec<Timeline> = step
        .players
        .iter()
        .zip(&drawn)
        .map(|(player, drawn)| {
            Timeline::new(
                &player.spec.machine,
                player.spec.machine.place(&drawn.map),
                &player.followed.begins,
            )
        })
        .collect();
    let mut played = vec![0usize; step.players.len()];
    let mut messages = Vec::new();
    let mut t = START;
    // Each step of the scenario, a note of its own too, on which no
    // machine moves (REQ-106).
    for message in &scenario.map.steps() {
        // A group sends players back, or on, before the message: a
        // player's steps one after another, as the panes take them.
        let mut moved = false;
        let mut longest: f32 = 0.0;
        for (index, player) in step.players.iter().enumerate() {
            let mut at = t;
            while let Some(next) = player.followed.steps.get(played[index])
                && next.line < message.line
            {
                played[index] += 1;
                at += timelines[index].step(&player.spec.machine, next, at);
                moved = true;
            }
            longest = longest.max(at - t);
        }
        if moved {
            t += longest.max(GROUP);
        }
        let start = t;
        let arrive = start + travel_millis(&message.points) as f32 / 1000.0;
        messages.push(page::Message {
            points: message.points.clone(),
            also: message.also.clone(),
            top: message.top,
            bottom: message.bottom,
            start,
            arrive,
            wrong: wrong.contains(&message.line),
        });
        // The receiver moves as the light arrives.
        let mut longest: f32 = 0.0;
        for (index, player) in step.players.iter().enumerate() {
            let mut at = arrive;
            while let Some(next) = player.followed.steps.get(played[index])
                && next.line == message.line
            {
                played[index] += 1;
                at += timelines[index].step(&player.spec.machine, next, at);
            }
            longest = longest.max(at - arrive);
        }
        t = arrive + longest + BEAT;
    }
    // What the groups at the end do, before the end stays.
    let mut longest: f32 = 0.0;
    for (index, player) in step.players.iter().enumerate() {
        let mut at = t;
        for next in &player.followed.steps[played[index]..] {
            at += timelines[index].step(&player.spec.machine, next, at);
        }
        longest = longest.max(at - t);
    }
    t += longest;

    let color = |rgb: omaspec_theme::Rgb| Color(rgb.r, rgb.g, rgb.b);
    let paper = |svg: &str| {
        omaspec_uml::parse_color(&super::super::building::picture::paper_of(svg))
            .ok()
            .flatten()
            .unwrap_or(Color::WHITE)
    };
    let lockstep = page::Lockstep {
        scenario: page::Scenario {
            paper: paper(&scenario.svg),
            svg: scenario.svg.clone(),
            messages,
        },
        machines: step
            .players
            .iter()
            .zip(drawn)
            .zip(timelines)
            .map(|((player, drawn), timeline)| page::Machine {
                name: player.name.clone(),
                paper: paper(&drawn.svg),
                share: svg_size(&drawn.svg).1 / widest,
                states: timeline.glows,
                lights: timeline.lights,
                travels: timeline.travels,
                flashes: timeline.flashes,
                bars: timeline.bars,
                svg: drawn.svg,
            })
            .collect(),
        end: t,
        hold: HOLD,
        colors: page::Colors {
            ground: color(palette.dark_background),
            pane: color(palette.background),
            ink: color(palette.foreground),
            muted: color(palette.muted),
            accent: color(palette.accent),
            light: color(palette.bright_foreground),
            wrong: color(palette.red),
        },
    };
    let (name, _) = args.named(text);
    let written = page::page(
        &lockstep,
        Some(omaspec_render::export::Source {
            code: text,
            language,
            file: Some(&name),
            folds: &[],
        }),
        name.rsplit_once('.').map_or(&name, |(stem, _)| stem),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    crate::files::write_atomic(&args.output, written.as_bytes())
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    let not_taken = wrong.len();
    Ok(format!(
        "{} → {} (a page, {} KB, {} messages, {} in vectors)",
        args.file.display(),
        args.output.display(),
        written.len().div_ceil(1024),
        lockstep
            .scenario
            .messages
            .iter()
            .filter(|m| m.points.len() > 1)
            .count(),
        match not_taken {
            0 => "every one taken".to_owned(),
            n => format!("{n} not taken"),
        }
    ))
}

/// A picture's width and height in its own units, as its `viewBox` says.
fn svg_size(svg: &str) -> (f32, f32) {
    let view = svg
        .find("<svg")
        .and_then(|start| {
            let tag = &svg[start..start + svg[start..].find('>')?];
            tag.split(" viewBox=\"").nth(1)?.split('"').next()
        })
        .map(|view| {
            view.split(|c: char| c == ',' || c.is_whitespace())
                .filter(|n| !n.is_empty())
                .filter_map(|n| n.parse::<f32>().ok())
                .collect::<Vec<_>>()
        });
    match view.as_deref() {
        Some(&[_, _, w, h]) if w > 0.0 && h > 0.0 => (w, h),
        _ => (1.0, 1.0),
    }
}

/// One machine's play on the page: when its states light, the lights
/// along its transitions, its flashes and what its bar says.
struct Timeline {
    placed: Placed,
    glows: Vec<Option<page::Glow>>,
    lit: Vec<usize>,
    lights: Vec<Vec<(f32, bool)>>,
    travels: Vec<page::Travel>,
    flashes: Vec<f32>,
    bars: Vec<page::Bar>,
}

impl Timeline {
    fn new(machine: &Machine, placed: Placed, begins: &[usize]) -> Timeline {
        let glows = placed
            .states
            .iter()
            .map(|spot| {
                spot.as_ref().map(|spot| page::Glow {
                    x: spot.rect.x,
                    y: spot.rect.y,
                    w: spot.rect.w,
                    h: spot.rect.h,
                    radius: spot.radius,
                })
            })
            .collect();
        let mut lights = vec![Vec::new(); machine.states.len()];
        for &state in begins {
            lights[state].push((0.0, true));
        }
        Timeline {
            placed,
            glows,
            lit: begins.to_vec(),
            lights,
            travels: Vec::new(),
            flashes: Vec::new(),
            bars: vec![page::Bar {
                from: 0.0,
                at: machine.shown_at(begins),
                said: String::new(),
                note: String::new(),
                wrong: false,
            }],
        }
    }

    /// Takes `step` at `at`; how long it takes until the next may come.
    fn step(&mut self, machine: &Machine, step: &TraceStep, at: f32) -> f32 {
        let took = step
            .taken
            .iter()
            .filter_map(|taken| self.placed.routes[taken.transition].as_deref())
            .map(|route| travel_millis(route) as f32 / 1000.0)
            .fold(0.0, f32::max);
        for taken in &step.taken {
            if let Some(route) = &self.placed.routes[taken.transition] {
                self.travels.push(page::Travel {
                    points: route.clone(),
                    start: at,
                    took: travel_millis(route) as f32 / 1000.0,
                });
            }
        }
        // What it leaves goes dark as the light sets off; what it enters
        // lights as the light arrives.
        for &state in &self.lit {
            if !step.at.contains(&state) {
                self.lights[state].push((at, false));
            }
        }
        for &state in &step.at {
            if !self.lit.contains(&state) {
                self.lights[state].push((at + took, true));
            }
        }
        self.lit = step.at.clone();
        let wrong = step
            .finding
            .as_ref()
            .is_some_and(|f| f.severity == omaspec_core::lint::Severity::Error);
        if wrong {
            self.flashes.push(at + took);
        }
        self.bars.push(page::Bar {
            from: at,
            at: machine.shown_at(&step.at),
            said: step.event.clone().unwrap_or_default(),
            note: match &step.finding {
                Some(finding) => finding.message.clone(),
                None if step.event.is_some() => "as the machine allows".to_owned(),
                None => String::new(),
            },
            wrong,
        });
        if wrong {
            took + FLASH
        } else if step.taken.is_empty() {
            LIT
        } else {
            took
        }
    }
}
