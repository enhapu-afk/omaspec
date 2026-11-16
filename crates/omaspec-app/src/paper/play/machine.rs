//! A state machine played: by the writer, step by step, or as a recorded
//! trace says, the states it is in lit, a light along each transition.

use std::time::Duration;

use iced::Point;
use iced::time::Instant;
use iced::widget::canvas::{Frame, Stroke};
use omaspec_model::{Followed, Machine, Placed, Run, Taken};

use super::super::canvas::fill_paper_band;
use super::super::light::{LINGER, MACHINE, appearance, draw_run, ease_out, fraction};

use super::draw::*;
use super::*;

pub struct MachinePlay {
    pub(super) machine: Machine,
    pub(super) placed: Placed,
    /// What moves the machine.
    pub(super) driver: Driver,
    /// The transitions on screen and when each one's light travels.
    pub(super) shown: Vec<Travel>,
    /// For each state, when its light went on or off.
    pub(super) lights: Vec<Vec<(Instant, bool)>>,
    /// When the steps scheduled so far are done; the next starts then.
    pub(super) free_at: Instant,
    pub(super) begun: Instant,
    /// The source line of each step scheduled, from when its light sets
    /// off; the editor lights each in turn.
    pub(super) lines: Vec<(Instant, usize)>,
    /// What the steps scheduled last leave and enter, and the ways between
    /// them, in diagram units: where a following camera looks.
    pub(super) focus: Option<iced::Rectangle>,
}

pub(super) enum Driver {
    /// The writer, picking what happens next.
    Writer(Box<Writer>),
    /// A recorded trace, line by line.
    Trace(Box<TraceRun>),
}

pub(super) struct Writer {
    /// Where the machine can be and what it offers there ([`every_offer`]).
    pub(super) every: std::sync::Arc<Vec<Offered>>,
    /// The run as it started, for a restart.
    pub(super) start: Run,
    pub(super) run: Run,
    /// The run before each step the writer took, for going back.
    pub(super) before: Vec<Run>,
    pub(super) hover: Option<usize>,
    pub(super) steps: usize,
}

pub(super) struct TraceRun {
    /// The trace's file name.
    pub(super) name: String,
    pub(super) object: String,
    pub(super) followed: Followed,
    /// Lines played so far.
    pub(super) played: usize,
}

impl TraceRun {
    /// Where the object can be after the lines played.
    pub(super) fn lit(&self) -> &[usize] {
        match self.played.checked_sub(1) {
            Some(last) => &self.followed.steps[last].at,
            None => &self.followed.begins,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Travel {
    pub(super) route: Option<Vec<(f32, f32)>>,
    pub(super) start: Instant,
    pub(super) took: Duration,
    /// The states it arrives in, for the ripple.
    pub(super) entered: Vec<usize>,
}

/// Where the machine can be in `graph`, and what it offers there, each
/// once: the bar takes its rows from them (the first runs of a machine
/// that can be in more ways).
pub(super) fn every_offer(m: &Machine, graph: &Graph) -> Vec<Offered> {
    let mut offers: Vec<Offered> = Vec::new();
    for (run, ways) in graph.runs.iter().zip(&graph.options) {
        let offer = (
            m.shown_at(run.active()),
            ways.iter().map(|way| m.option(way.transition)).collect(),
        );
        if !offers.contains(&offer) {
            offers.push(offer);
        }
    }
    offers
}

/// What `steps` leave and enter and the ways they take, as `placed` has
/// them: where a following camera looks.
fn focus_of(placed: &Placed, steps: &[Taken]) -> Option<iced::Rectangle> {
    let rect = |x: f32, y: f32, w: f32, h: f32| {
        iced::Rectangle::new(
            iced::Point::new(x, y),
            iced::Size::new(w.max(1.0), h.max(1.0)),
        )
    };
    let mut boxes = Vec::new();
    for step in steps {
        for &state in step.left.iter().chain(&step.entered) {
            if let Some(spot) = &placed.states[state] {
                boxes.push(rect(spot.rect.x, spot.rect.y, spot.rect.w, spot.rect.h));
            }
        }
        if let Some(points) = &placed.routes[step.transition] {
            let (xs, ys): (Vec<f32>, Vec<f32>) = points.iter().copied().unzip();
            let (left, top) = (
                xs.iter().copied().fold(f32::MAX, f32::min),
                ys.iter().copied().fold(f32::MAX, f32::min),
            );
            let (right, bottom) = (
                xs.iter().copied().fold(f32::MIN, f32::max),
                ys.iter().copied().fold(f32::MIN, f32::max),
            );
            if left <= right && top <= bottom {
                boxes.push(rect(left, top, right - left, bottom - top));
            }
        }
    }
    boxes.into_iter().reduce(|a, b| a.union(&b))
}

impl MachinePlay {
    pub(super) fn new(machine: Machine, placed: Placed, driver: Driver, now: Instant) -> Self {
        MachinePlay {
            lights: vec![Vec::new(); machine.states.len()],
            machine,
            placed,
            driver,
            shown: Vec::new(),
            free_at: now + VEIL_FADE / 2,
            begun: now,
            lines: Vec::new(),
            focus: None,
        }
    }

    pub(super) fn ask(&mut self, ask: Ask, now: Instant) {
        match self.driver {
            Driver::Writer(_) => self.ask_writer(ask, now),
            Driver::Trace(_) => self.ask_trace(ask, now),
        }
    }

    pub(super) fn ask_writer(&mut self, ask: Ask, now: Instant) {
        let Driver::Writer(writer) = &mut self.driver else {
            return;
        };
        let options = writer.run.options(&self.machine);
        match ask {
            Ask::Take(n) => {
                if let Some(&t) = options.get(n) {
                    self.take(t, now);
                }
            }
            Ask::Next => {
                if let Some(&t) = options.first() {
                    self.take(t, now);
                }
            }
            Ask::Hover(n) => writer.hover = n.and_then(|n| options.get(n).copied()),
            Ask::Back => {
                if let Some(run) = writer.before.pop() {
                    writer.steps = writer.steps.saturating_sub(1);
                    self.switch(run, now);
                }
            }
            Ask::Restart => {
                writer.before.clear();
                writer.steps = 0;
                let start = writer.start.clone();
                self.switch(start, now);
            }
        }
    }

    /// A trace goes on line by line; there is nothing to pick.
    pub(super) fn ask_trace(&mut self, ask: Ask, now: Instant) {
        let Driver::Trace(trace) = &mut self.driver else {
            return;
        };
        match ask {
            Ask::Next => {
                let Some(step) = trace.followed.steps.get(trace.played).cloned() else {
                    return;
                };
                trace.played += 1;
                self.schedule(&step.taken, now);
                self.aim(&step.at, now);
            }
            Ask::Back => {
                if trace.played > 0 {
                    trace.played -= 1;
                    let lit = trace.lit().to_vec();
                    self.show(&lit, now);
                }
            }
            Ask::Restart => {
                trace.played = 0;
                let lit = trace.lit().to_vec();
                self.show(&lit, now);
            }
            Ask::Take(_) | Ask::Hover(_) => {}
        }
    }

    pub(super) fn take(&mut self, transition: usize, now: Instant) {
        let Driver::Writer(writer) = &mut self.driver else {
            return;
        };
        if writer.before.len() >= MAX_BACK {
            writer.before.remove(0);
        }
        writer.before.push(writer.run.clone());
        let steps = writer.run.take(&self.machine, transition);
        writer.hover = None;
        writer.steps += 1;
        self.schedule(&steps, now);
    }

    /// Goes to `run` at once.
    pub(super) fn switch(&mut self, run: Run, now: Instant) {
        self.show(run.active(), now);
        if let Driver::Writer(writer) = &mut self.driver {
            writer.run = run;
            writer.hover = None;
        }
    }

    /// Lights `lit` at once. A travel still on the way is dropped, and the
    /// lights fade from where they actually are.
    pub(super) fn show(&mut self, lit: &[usize], now: Instant) {
        self.shown.clear();
        self.lines.clear();
        for events in &mut self.lights {
            events.retain(|(t, _)| *t <= now);
        }
        for state in 0..self.machine.states.len() {
            let was = aimed(&self.lights[state], now);
            let is = lit.contains(&state);
            if was != is {
                light(&mut self.lights[state], now, is);
            }
        }
        self.free_at = now + LIGHT_FADE / 2;
    }

    /// Makes the lights end on `lit` once what is scheduled has arrived:
    /// where a trace could go several ways, or picks the object up again.
    pub(super) fn aim(&mut self, lit: &[usize], now: Instant) {
        let at = self.free_at.max(now);
        let mut changed = false;
        for (state, events) in self.lights.iter_mut().enumerate() {
            let on = lit.contains(&state);
            if events.last().is_some_and(|&(_, was)| was) != on {
                light(events, at, on);
                changed = true;
            }
        }
        if changed {
            self.free_at = self.free_at.max(at + LIGHT_FADE / 2);
        }
    }

    /// Lines the steps up after what is already on screen, in the model's
    /// time ([`omaspec_model::play::timed`]): one beat travels at once; the
    /// next beat waits until the longest of them has arrived.
    pub(super) fn schedule(&mut self, steps: &[Taken], now: Instant) {
        let began = now.max(self.free_at);
        let routes = &self.placed.routes;
        let (timed, through) = omaspec_model::play::timed(steps, |step| {
            routes[step.transition]
                .as_deref()
                .map_or(0, omaspec_model::play::travel_millis)
        });
        for omaspec_model::play::Timed { index, at, took } in timed {
            let step = &steps[index];
            let (at, took) = (
                began + Duration::from_millis(at),
                Duration::from_millis(took),
            );
            for &state in &step.left {
                light(&mut self.lights[state], at, false);
            }
            for &state in &step.entered {
                light(&mut self.lights[state], at + took, true);
            }
            self.shown.push(Travel {
                route: self.placed.routes[step.transition].clone(),
                start: at,
                took,
                entered: step.entered.clone(),
            });
            self.lines
                .push((at, self.machine.transitions[step.transition].line));
        }
        if !steps.is_empty() {
            self.free_at = began + Duration::from_millis(through);
            self.focus = focus_of(&self.placed, steps).or(self.focus);
        }
        // What has finished long ago is not drawn again.
        let old = now.checked_sub(Duration::from_secs(5)).unwrap_or(now);
        self.shown
            .retain(|t| t.start + t.took + LINGER + RIPPLE > old);
        // Nor waits a line nobody took: the preview lights none.
        self.lines.retain(|&(at, _)| at > old);
    }

    pub(super) fn settled(&self) -> Instant {
        self.free_at + LIGHT_FADE.max(LINGER).max(RIPPLE) + VEIL_FADE
    }

    /// The line of the last step whose light has set off by `now`; the
    /// lines before it are passed over.
    pub(super) fn take_line(&mut self, now: Instant) -> Option<usize> {
        let due = self.lines.iter().rposition(|&(at, _)| at <= now)?;
        let (_, line) = self.lines[due];
        self.lines.drain(..=due);
        Some(line)
    }

    pub(super) fn offer(&self) -> Offer {
        let m = &self.machine;
        let names = |states: &[usize]| m.shown_at(states);
        let writer = match &self.driver {
            Driver::Writer(writer) => writer,
            Driver::Trace(trace) => {
                let step = trace
                    .played
                    .checked_sub(1)
                    .map(|last| &trace.followed.steps[last]);
                let said = step.map_or_else(String::new, |step| match (&step.event, &step.state) {
                    (Some(event), Some(state)) => format!("{event} → {state}"),
                    (Some(event), None) => event.clone(),
                    (None, Some(state)) => format!("in {state}"),
                    (None, None) => String::new(),
                });
                return Offer::Trace {
                    name: trace.name.clone(),
                    object: trace.object.clone(),
                    played: trace.played,
                    of: trace.followed.steps.len(),
                    line: step.map(|s| s.line + 1),
                    time: step.and_then(|s| s.time.clone()),
                    said,
                    at: names(trace.lit()),
                    finding: step
                        .and_then(|s| s.finding.as_ref())
                        .map(|f| (f.severity, f.message.clone())),
                };
            }
        };
        let options = writer
            .run
            .options(m)
            .into_iter()
            .map(|t| m.option(t))
            .collect();
        let hover = writer
            .hover
            .and_then(|h| writer.run.options(m).iter().position(|&t| t == h));
        Offer::Machine {
            options,
            hover,
            at: names(writer.run.active()),
            steps: writer.steps,
            ended: writer.run.ended(m),
            can_go_back: !writer.before.is_empty(),
            every: writer.every.clone(),
        }
    }

    pub(super) fn levels(&self, now: Instant) -> Vec<f32> {
        self.lights.iter().map(|l| level(l, now)).collect()
    }

    pub(super) fn draw_under(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
        let veil = ramp(now, self.begun, VEIL_FADE);
        let paper = view.paper;
        fill_paper_band(
            frame,
            paper,
            view.scale,
            paper.y,
            paper.y + paper.height,
            with_alpha(look.veil, VEIL * veil),
        );
        // The states the machine is in come through the veil.
        for (state, lit) in self.levels(now).into_iter().enumerate() {
            let Some(spot) = self.placed.states[state].as_ref().filter(|_| lit > 0.004) else {
                continue;
            };
            let glow = Glow::of(spot, view);
            if let Some(clip) = glow.bounds().intersection(&view.paper) {
                (view.paint)(frame, clip, lit);
            }
        }
    }

    pub(super) fn draw_over(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
        let m = &self.machine;
        let levels = self.levels(now);
        // What the clips of the states let through beyond their outlines
        // goes back under the veil: in this layer, as iced draws the
        // pictures of the one below over its shapes.
        let veil = ramp(now, self.begun, VEIL_FADE);
        for (state, &lit) in levels.iter().enumerate() {
            if let Some(spot) = self.placed.states[state].as_ref().filter(|_| lit > 0.004) {
                let glow = Glow::of(spot, view);
                if glow.bounds().intersects(&view.paper) {
                    glow.mask(frame, with_alpha(look.veil, VEIL * veil * lit));
                }
            }
        }
        // States around them get a quiet frame.
        for (state, placed) in self.placed.states.iter().enumerate().take(m.states.len()) {
            if !m.is_composite(state) {
                continue;
            }
            let inside = (0..m.states.len())
                .filter(|&s| m.ancestors(s).any(|a| a == state))
                .map(|s| levels[s])
                .fold(0.0, f32::max);
            if let Some(spot) = placed.as_ref().filter(|_| inside > 0.004) {
                let r = (view.place)(spot.rect);
                let radius = spot.radius * zoom(spot, view);
                frame.stroke(
                    &rounded(grow(r, 2.0), radius + 2.0),
                    Stroke::default()
                        .with_color(with_alpha(look.accent, 0.55 * inside))
                        .with_width(1.5),
                );
            }
        }
        // Where the writer's pointer rests: the way an option goes.
        let hover = match &self.driver {
            Driver::Writer(writer) => writer.hover,
            Driver::Trace(_) => None,
        };
        if let Some(points) = hover.and_then(|t| self.placed.routes[t].as_ref()) {
            let path = polyline(points.iter().map(|&p| view.point(p)));
            frame.stroke(
                &path,
                Stroke {
                    line_dash: iced::widget::canvas::LineDash {
                        segments: &[7.0, 6.0],
                        offset: 0,
                    },
                    ..soft(look.accent, 2.5, 0.85)
                },
            );
        }
        for (state, &lit) in levels.iter().enumerate() {
            if let Some(spot) = self.placed.states[state].as_ref().filter(|_| lit > 0.004) {
                Glow::of(spot, view).draw(frame, look.accent, lit);
            }
        }
        for travel in &self.shown {
            self.draw_travel(frame, look, view, travel, now);
        }
    }

    pub(super) fn draw_travel(
        &self,
        frame: &mut Frame,
        look: &Look,
        view: &Viewer,
        t: &Travel,
        now: Instant,
    ) {
        if now < t.start {
            return;
        }
        let arrived = t.start + t.took;
        if let Some(route) = &t.route {
            let screen: Vec<Point> = route.iter().map(|&p| view.point(p)).collect();
            let since = now.saturating_duration_since(t.start);
            let light = appearance(since, t.took, true, 1.0, MACHINE);
            draw_run(frame, &screen, look.accent, look.light, light);
        }
        // A ripple spreads from each state it arrives in.
        if now >= arrived && now < arrived + RIPPLE {
            let k = ease_out(fraction(now - arrived, RIPPLE));
            for &state in &t.entered {
                if let Some(spot) = &self.placed.states[state] {
                    Glow::of(spot, view).ripple(frame, look.accent, k);
                }
            }
        }
    }
}
