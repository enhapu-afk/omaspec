//! A specification played in the diagram: a state machine taking one
//! transition after another, as the writer picks them or as a recorded
//! trace says, or a scenario unfolding message by message.
//!
//! What is on screen is a function of the time alone. A step is scheduled
//! when it is taken (after the one before has arrived), and each frame
//! draws where everything is by then: the rest of the diagram under a
//! veil, the states the machine is in lit through it with a soft glow, a
//! light running along a transition's own route with a fading trail, a
//! ripple where it arrives. A scenario lifts the veil message by message
//! while a light runs along each arrow.

use std::time::Duration;

use iced::border::Radius;
use iced::time::Instant;
use iced::widget::canvas::gradient::Linear;
use iced::widget::canvas::{Frame, LineCap, LineJoin, Path, Stroke};
use iced::{Color, Point, Rectangle, Size, Vector};
use omaspec_core::Language;
use omaspec_core::lint::Severity;
use omaspec_model::{Followed, Machine, Placed, Run, Spec, Taken, Trigger};
use omaspec_render::Map;
use omaspec_uml::graph::Rect;
use omaspec_uml::map::{Shape, Spot, Step};

use super::canvas::fill_paper_band;

/// How long the veil takes to fall over the diagram, or lift off it.
const VEIL_FADE: Duration = Duration::from_millis(320);
/// How dark the veil gets, over the paper.
const VEIL: f32 = 0.62;
/// How long a state's light takes to come on or go out.
const LIGHT_FADE: Duration = Duration::from_millis(420);
/// A pause between two steps that follow each other by themselves.
const STEP_GAP: Duration = Duration::from_millis(70);
/// How long the ripple of an arrival spreads.
const RIPPLE: Duration = Duration::from_millis(620);
/// How long a travelled route stays lit after its light arrived.
const LINGER: Duration = Duration::from_millis(520);
/// How far behind its head a light's trail reaches, on screen.
const TRAIL: f32 = 150.0;
/// How long the veil takes to lift off the next message.
const REVEAL: Duration = Duration::from_millis(460);
/// Steps a machine remembers for going back.
const MAX_BACK: usize = 500;

/// A specification being played.
pub enum Play {
    Machine(Box<MachinePlay>),
    Scenario(ScenarioPlay),
}

/// What the player asks of a [`Play`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Take an option (a state machine) by its number in the list.
    Take(usize),
    /// Show where an option leads, or stop showing it.
    Hover(Option<usize>),
    Back,
    Restart,
    /// The next message (a scenario) or line (a trace), or the first
    /// option (a machine).
    Next,
}

impl Play {
    /// A play of the diagram drawn from `source`: its state machine, or the
    /// scenario of its messages. Why not, if neither.
    pub fn new(source: &str, language: Language, map: &Map, now: Instant) -> Result<Play, String> {
        if let Some(machine) = Machine::read(source, language) {
            let placed = placed(&machine, map)?;
            let (run, steps) = Run::start(&machine)
                .ok_or("the state machine has no start ([*] --> …), so it cannot run")?;
            let writer = Writer {
                start: run.clone(),
                run,
                before: Vec::new(),
                hover: None,
                steps: 0,
            };
            let mut play = MachinePlay::new(machine, placed, Driver::Writer(Box::new(writer)), now);
            play.schedule(&steps, now);
            return Ok(Play::Machine(Box::new(play)));
        }
        if map.messages.is_empty() {
            return Err("only a state machine or a sequence of messages can be played".into());
        }
        Ok(Play::Scenario(ScenarioPlay {
            steps: map.messages.clone(),
            at: 0,
            moved: now,
            from_y: map.messages[0].top,
            begun: now,
            jump: None,
        }))
    }

    /// A play of the recorded `trace` (named `name`) of `object`, on the
    /// state machine drawn from `source`, the diagram `file`.
    #[allow(clippy::too_many_arguments)]
    pub fn trace(
        source: &str,
        language: Language,
        map: &Map,
        file: &str,
        trace: &str,
        name: &str,
        object: &str,
        now: Instant,
    ) -> Result<Play, String> {
        let spec = Spec::read(source, language, file)
            .ok_or("only a state machine can play a trace, and this diagram is none")?;
        let followed = omaspec_model::follow_trace(trace, &spec, object);
        if followed.steps.is_empty() {
            return Err(format!("{name} has no line of {object}"));
        }
        let placed = placed(&spec.machine, map)?;
        let (start, begins) = (followed.start.clone(), followed.begins.clone());
        let driver = Driver::Trace(Box::new(TraceRun {
            name: name.to_owned(),
            object: object.to_owned(),
            followed,
            played: 0,
        }));
        let mut play = MachinePlay::new(spec.machine, placed, driver, now);
        play.schedule(&start, now);
        play.aim(&begins, now);
        Ok(Play::Machine(Box::new(play)))
    }

    pub fn ask(&mut self, ask: Ask, now: Instant) {
        match self {
            Play::Machine(play) => play.ask(ask, now),
            Play::Scenario(play) => play.ask(ask, now),
        }
    }

    /// Takes the places of a diagram drawn again: the same diagram in
    /// another theme or font.
    pub fn remap(&mut self, map: &Map) {
        match self {
            Play::Machine(play) => play.placed = play.machine.place(map),
            Play::Scenario(play) => {
                if map.messages.len() == play.steps.len() {
                    play.steps = map.messages.clone();
                }
            }
        }
    }

    /// The source line of the step whose light set off last by `now`,
    /// once: the editor lights it.
    pub fn take_jump(&mut self, now: Instant) -> Option<usize> {
        match self {
            Play::Machine(play) => play.take_line(now),
            Play::Scenario(play) => play.jump.take(),
        }
    }

    /// When the lights of a machine's steps so far have all arrived.
    #[cfg(test)]
    pub fn arrived(&self) -> Option<Instant> {
        match self {
            Play::Machine(play) => Some(play.free_at),
            Play::Scenario(_) => None,
        }
    }

    /// Whether a step's line is still to be lit, when its light sets off.
    pub fn line_waiting(&self) -> bool {
        match self {
            Play::Machine(play) => !play.lines.is_empty(),
            Play::Scenario(_) => false,
        }
    }

    /// Whether it plays a recorded trace.
    pub fn tracing(&self) -> bool {
        matches!(self, Play::Machine(play) if matches!(play.driver, Driver::Trace(_)))
    }

    /// Whether something still moves at `now`.
    pub fn animating(&self, now: Instant) -> bool {
        match self {
            Play::Machine(play) => now < play.settled(),
            Play::Scenario(play) => now < play.settled(),
        }
    }

    /// What the bar under the diagram offers: each option's label and
    /// where it leads, or the step a scenario is at.
    pub fn offer(&self) -> Offer {
        match self {
            Play::Machine(play) => play.offer(),
            Play::Scenario(play) => play.offer(),
        }
    }

    /// Draws the veil and what comes through it: the layer right over the
    /// diagram.
    pub fn draw_under(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
        match self {
            Play::Machine(play) => play.draw_under(frame, look, view, now),
            Play::Scenario(play) => play.draw_under(frame, look, view, now),
        }
    }

    /// Draws the glows and the lights on their way: the layer on top.
    pub fn draw_over(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
        match self {
            Play::Machine(play) => play.draw_over(frame, look, view, now),
            Play::Scenario(play) => play.draw_over(frame, look, view, now),
        }
    }
}

/// How much of a play that is ending is still to be seen at `now`.
pub fn fading(now: Instant, since: Instant) -> f32 {
    1.0 - ramp(now, since, VEIL_FADE)
}

/// Whether a play that began ending at `since` is gone by `now`.
pub fn gone(now: Instant, since: Instant) -> bool {
    now.saturating_duration_since(since) >= VEIL_FADE
}

/// The colors a play draws with.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub accent: Color,
    /// The veil's color: the window's background.
    pub veil: Color,
    pub light: Color,
}

impl Look {
    /// The look at `share` of its strength, as the play fades away.
    pub fn faded(self, share: f32) -> Look {
        Look {
            accent: with_alpha(self.accent, share),
            veil: with_alpha(self.veil, share),
            light: with_alpha(self.light, share),
        }
    }
}

/// Where the diagram is on screen, and how to draw its pictures again.
pub struct Viewer<'a> {
    /// The paper on screen.
    pub paper: Rectangle,
    /// The window's scale.
    pub scale: f32,
    /// A rectangle of the diagram on screen.
    pub place: &'a dyn Fn(Rect) -> Rectangle,
    /// Draws the diagram's pictures within a rectangle, at an opacity.
    pub paint: &'a dyn Fn(&mut Frame, Rectangle, f32),
}

impl Viewer<'_> {
    fn point(&self, (x, y): (f32, f32)) -> Point {
        (self.place)(Rect {
            x,
            y,
            w: 0.0,
            h: 0.0,
        })
        .position()
    }
}

/// Where a machine's states and transitions are drawn, or why a play
/// cannot show them.
fn placed(machine: &Machine, map: &Map) -> Result<Placed, String> {
    let placed = machine.place(map);
    if placed.states.iter().all(Option::is_none) {
        return Err(
            "Omaspec cannot show this state machine's states yet, so it cannot play it".into(),
        );
    }
    // A machine the picture does not show in full would play steps nobody
    // sees.
    if placed.states.iter().any(Option::is_none) || placed.routes.iter().any(Option::is_none) {
        return Err(
            "the diagram does not show every state and transition its lines name, so it cannot be played"
                .into(),
        );
    }
    Ok(placed)
}

/// The bar under a playing diagram.
#[derive(Debug, Clone, PartialEq)]
pub enum Offer {
    Machine {
        /// Each option: its label and the state it leads to.
        options: Vec<(String, String)>,
        hover: Option<usize>,
        /// Where the machine is: its states, by name.
        at: String,
        steps: usize,
        ended: bool,
        can_go_back: bool,
    },
    Scenario {
        at: usize,
        of: usize,
    },
    Trace {
        /// The trace's file name.
        name: String,
        object: String,
        /// Lines of the object played so far, of all.
        played: usize,
        of: usize,
        /// The line just played (one-based), its time and what it says.
        line: Option<usize>,
        time: Option<String>,
        said: String,
        /// Where the object can be by now: its states, by name; empty while
        /// it is not followed.
        at: String,
        /// What deviates on that line, or why the object is not followed.
        finding: Option<(Severity, String)>,
    },
}

pub struct MachinePlay {
    machine: Machine,
    placed: Placed,
    /// What moves the machine.
    driver: Driver,
    /// The transitions on screen and when each one's light travels.
    shown: Vec<Travel>,
    /// For each state, when its light went on or off.
    lights: Vec<Vec<(Instant, bool)>>,
    /// When the steps scheduled so far are done; the next starts then.
    free_at: Instant,
    begun: Instant,
    /// The source line of each step scheduled, from when its light sets
    /// off; the editor lights each in turn.
    lines: Vec<(Instant, usize)>,
}

enum Driver {
    /// The writer, picking what happens next.
    Writer(Box<Writer>),
    /// A recorded trace, line by line.
    Trace(Box<TraceRun>),
}

struct Writer {
    /// The run as it started, for a restart.
    start: Run,
    run: Run,
    /// The run before each step the writer took, for going back.
    before: Vec<Run>,
    hover: Option<usize>,
    steps: usize,
}

struct TraceRun {
    /// The trace's file name.
    name: String,
    object: String,
    followed: Followed,
    /// Lines played so far.
    played: usize,
}

impl TraceRun {
    /// Where the object can be after the lines played.
    fn lit(&self) -> &[usize] {
        match self.played.checked_sub(1) {
            Some(last) => &self.followed.steps[last].at,
            None => &self.followed.begins,
        }
    }
}

#[derive(Debug, Clone)]
struct Travel {
    route: Option<Vec<(f32, f32)>>,
    start: Instant,
    took: Duration,
    /// The states it arrives in, for the ripple.
    entered: Vec<usize>,
}

impl MachinePlay {
    fn new(machine: Machine, placed: Placed, driver: Driver, now: Instant) -> Self {
        MachinePlay {
            lights: vec![Vec::new(); machine.states.len()],
            machine,
            placed,
            driver,
            shown: Vec::new(),
            free_at: now + VEIL_FADE / 2,
            begun: now,
            lines: Vec::new(),
        }
    }

    fn ask(&mut self, ask: Ask, now: Instant) {
        match self.driver {
            Driver::Writer(_) => self.ask_writer(ask, now),
            Driver::Trace(_) => self.ask_trace(ask, now),
        }
    }

    fn ask_writer(&mut self, ask: Ask, now: Instant) {
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
    fn ask_trace(&mut self, ask: Ask, now: Instant) {
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

    fn take(&mut self, transition: usize, now: Instant) {
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
    fn switch(&mut self, run: Run, now: Instant) {
        self.show(run.active(), now);
        if let Driver::Writer(writer) = &mut self.driver {
            writer.run = run;
            writer.hover = None;
        }
    }

    /// Lights `lit` at once. A travel still on the way is dropped, and the
    /// lights fade from where they actually are.
    fn show(&mut self, lit: &[usize], now: Instant) {
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
    fn aim(&mut self, lit: &[usize], now: Instant) {
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

    /// Lines the steps up after what is already on screen. One beat
    /// travels at once; the next beat waits until the longest of them
    /// has arrived.
    fn schedule(&mut self, steps: &[Taken], now: Instant) {
        let mut began = now.max(self.free_at);
        let mut beats: Vec<u32> = Vec::new();
        for step in steps {
            if !beats.contains(&step.beat) {
                beats.push(step.beat);
            }
        }
        for beat in beats {
            let mut longest = Duration::ZERO;
            for step in steps.iter().filter(|s| s.beat == beat) {
                let route = self.placed.routes[step.transition].clone();
                let took = route
                    .as_ref()
                    .map_or(Duration::ZERO, |points| travel_time(points));
                longest = longest.max(took);
                let at = began;
                for &state in &step.left {
                    light(&mut self.lights[state], at, false);
                }
                for &state in &step.entered {
                    light(&mut self.lights[state], at + took, true);
                }
                self.shown.push(Travel {
                    route,
                    start: at,
                    took,
                    entered: step.entered.clone(),
                });
                self.lines
                    .push((at, self.machine.transitions[step.transition].line));
            }
            began += longest + STEP_GAP;
        }
        if !steps.is_empty() {
            self.free_at = began;
        }
        // What has finished long ago is not drawn again.
        let old = now.checked_sub(Duration::from_secs(5)).unwrap_or(now);
        self.shown
            .retain(|t| t.start + t.took + LINGER + RIPPLE > old);
        // Nor waits a line nobody took: the preview lights none.
        self.lines.retain(|&(at, _)| at > old);
    }

    fn settled(&self) -> Instant {
        self.free_at + LIGHT_FADE.max(LINGER).max(RIPPLE) + VEIL_FADE
    }

    /// The line of the last step whose light has set off by `now`; the
    /// lines before it are passed over.
    fn take_line(&mut self, now: Instant) -> Option<usize> {
        let due = self.lines.iter().rposition(|&(at, _)| at <= now)?;
        let (_, line) = self.lines[due];
        self.lines.drain(..=due);
        Some(line)
    }

    fn offer(&self) -> Offer {
        let m = &self.machine;
        let names = |states: &[usize]| {
            let mut at: Vec<String> = states
                .iter()
                .map(|&s| name_of(m, s))
                .filter(|n| !n.is_empty())
                .collect();
            at.dedup();
            at.join(", ")
        };
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
            .map(|t| {
                let tr = &m.transitions[t];
                (label(&tr.trigger), name_of(m, tr.to))
            })
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
        }
    }

    fn levels(&self, now: Instant) -> Vec<f32> {
        self.lights.iter().map(|l| level(l, now)).collect()
    }

    fn draw_under(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
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

    fn draw_over(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
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
        for state in 0..m.states.len() {
            if !m.is_composite(state) {
                continue;
            }
            let inside = (0..m.states.len())
                .filter(|&s| m.ancestors(s).any(|a| a == state))
                .map(|s| levels[s])
                .fold(0.0, f32::max);
            if let Some(spot) = self.placed.states[state]
                .as_ref()
                .filter(|_| inside > 0.004)
            {
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

    fn draw_travel(&self, frame: &mut Frame, look: &Look, view: &Viewer, t: &Travel, now: Instant) {
        if now < t.start {
            return;
        }
        let arrived = t.start + t.took;
        if let Some(route) = &t.route {
            let screen: Vec<Point> = route.iter().map(|&p| view.point(p)).collect();
            let length = polyline_length(&screen);
            if now < arrived {
                let p = ease_in_out(fraction(now - t.start, t.took));
                comet(frame, &screen, length * p, look);
            } else {
                // The route stays lit a while and fades.
                let k = 1.0 - ease_out(fraction(now - arrived, LINGER));
                if k > 0.0 {
                    frame.stroke(
                        &polyline(screen.iter().copied()),
                        soft(look.accent, 2.5, 0.75 * k),
                    );
                }
            }
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

pub struct ScenarioPlay {
    steps: Vec<Step>,
    /// How many messages are shown.
    at: usize,
    /// When the veil last moved, and from where.
    moved: Instant,
    from_y: f32,
    begun: Instant,
    jump: Option<usize>,
}

impl ScenarioPlay {
    fn ask(&mut self, ask: Ask, now: Instant) {
        let to = match ask {
            Ask::Next | Ask::Take(_) => (self.at + 1).min(self.steps.len()),
            Ask::Back => self.at.saturating_sub(1),
            Ask::Restart => 0,
            Ask::Hover(_) => return,
        };
        if to == self.at {
            return;
        }
        self.from_y = self.edge(now);
        self.moved = now;
        self.at = to;
        self.jump = to.checked_sub(1).map(|i| self.steps[i].line);
    }

    /// Where the veil's upper edge is at `now`, in diagram units.
    fn edge(&self, now: Instant) -> f32 {
        let target = self.target();
        let k = ease_out(fraction(now.saturating_duration_since(self.moved), REVEAL));
        self.from_y + (target - self.from_y) * k
    }

    fn target(&self) -> f32 {
        match self.at {
            0 => self.steps[0].top,
            n => self.steps[n - 1].bottom,
        }
    }

    fn settled(&self) -> Instant {
        self.moved + REVEAL.max(self.travel()) + LINGER + VEIL_FADE
    }

    fn travel(&self) -> Duration {
        self.at
            .checked_sub(1)
            .map_or(Duration::ZERO, |i| travel_time(&self.steps[i].points))
    }

    fn offer(&self) -> Offer {
        Offer::Scenario {
            at: self.at,
            of: self.steps.len(),
        }
    }

    fn draw_under(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
        let veil = ramp(now, self.begun, VEIL_FADE);
        let edge = view.point((0.0, self.edge(now))).y;
        let paper = view.paper;
        // A soft edge, then the veil over what is still to come.
        const SOFT: f32 = 30.0;
        let top = edge.max(paper.y);
        let soft_end = (top + SOFT).min(paper.y + paper.height);
        if soft_end > top {
            let full = with_alpha(look.veil, VEIL * veil);
            let fade = Linear::new(Point::new(paper.x, top), Point::new(paper.x, top + SOFT))
                .add_stop(0.0, with_alpha(full, 0.0))
                .add_stop(0.45, with_alpha(full, 0.3))
                .add_stop(1.0, full);
            fill_paper_band(frame, paper, view.scale, top, soft_end, fade);
        }
        let below = top + SOFT;
        if below < paper.y + paper.height {
            fill_paper_band(
                frame,
                paper,
                view.scale,
                below,
                paper.y + paper.height,
                with_alpha(look.veil, VEIL * veil),
            );
        }
    }

    fn draw_over(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
        let paper = view.paper;
        let Some(current) = self.at.checked_sub(1).map(|i| &self.steps[i]) else {
            return;
        };
        let since = now.saturating_duration_since(self.moved);
        // The message's band glows faintly, its arrow brightly.
        let band_top = view.point((0.0, current.top)).y;
        let band_bottom = view.point((0.0, current.bottom)).y;
        let glow = 1.0 - 0.5 * ease_out(fraction(since.saturating_sub(REVEAL), LINGER));
        fill_paper_band(
            frame,
            paper,
            view.scale,
            band_top,
            band_bottom,
            with_alpha(look.accent, 0.07 * glow),
        );
        let screen: Vec<Point> = current.points.iter().map(|&p| view.point(p)).collect();
        let length = polyline_length(&screen);
        let took = travel_time(&current.points);
        if since < took {
            let p = ease_in_out(fraction(since, took));
            comet(frame, &screen, length * p, look);
        } else {
            frame.stroke(
                &polyline(screen.iter().copied()),
                soft(look.accent, 2.5, 0.9 * glow),
            );
            let k = ease_out(fraction(since - took, RIPPLE));
            if k < 1.0
                && let Some(&tip) = screen.last()
            {
                frame.stroke(
                    &Path::circle(tip, 4.0 + 16.0 * k),
                    soft(look.accent, 2.0, 0.6 * (1.0 - k)),
                );
            }
        }
    }
}

/// A transition's label as a writer reads it: `event [guard]`, or what
/// happens without one.
fn label(trigger: &Trigger) -> String {
    let mut out = trigger.events.join(", ");
    if let Some(guard) = &trigger.guard {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&format!("[{guard}]"));
    }
    if out.is_empty() {
        "go on".to_owned()
    } else {
        out
    }
}

fn name_of(m: &Machine, state: usize) -> String {
    use omaspec_model::Kind;
    let s = &m.states[state];
    let what = match s.kind {
        Kind::End => "end",
        Kind::History => "history",
        Kind::DeepHistory => "deep history",
        _ => return s.name.clone(),
    };
    match s.parent {
        Some(p) => format!("{what} of {}", m.states[p].name),
        None => what.to_owned(),
    }
}

/// Notes that a state's light goes on or off at `at`; older notes go.
fn light(events: &mut Vec<(Instant, bool)>, at: Instant, on: bool) {
    events.push((at, on));
    events.sort_by_key(|(t, _)| *t);
    if events.len() > 6 {
        events.remove(0);
    }
}

/// Whether the light is on its way to being on at `now`.
fn aimed(events: &[(Instant, bool)], now: Instant) -> bool {
    events
        .iter()
        .rev()
        .find(|(t, _)| *t <= now)
        .is_some_and(|(_, on)| *on)
}

/// How bright a light is at `now`, from when it went on and off.
fn level(events: &[(Instant, bool)], now: Instant) -> f32 {
    let mut since: Option<(Instant, f32, bool)> = None;
    let value = |since: Option<(Instant, f32, bool)>, at: Instant| match since {
        None => 0.0,
        Some((t, from, on)) => {
            let k = ease_out(fraction(at.saturating_duration_since(t), LIGHT_FADE));
            if on {
                from + (1.0 - from) * k
            } else {
                from * (1.0 - k)
            }
        }
    };
    for &(t, on) in events {
        if t > now {
            break;
        }
        since = Some((t, value(since, t), on));
    }
    value(since, now)
}

/// How long a light takes along `points` (diagram units): longer ways a
/// little longer, never so long that the writer waits.
fn travel_time(points: &[(f32, f32)]) -> Duration {
    let length: f32 = points
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum();
    Duration::from_millis((280.0 + length * 1.1).clamp(380.0, 980.0) as u64)
}

fn fraction(elapsed: Duration, of: Duration) -> f32 {
    if of.is_zero() {
        1.0
    } else {
        (elapsed.as_secs_f32() / of.as_secs_f32()).clamp(0.0, 1.0)
    }
}

fn ramp(now: Instant, since: Instant, over: Duration) -> f32 {
    ease_out(fraction(now.saturating_duration_since(since), over))
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color {
        a: (c.a * a).clamp(0.0, 1.0),
        ..c
    }
}

fn soft(color: Color, width: f32, alpha: f32) -> Stroke<'static> {
    Stroke::default()
        .with_color(with_alpha(color, alpha))
        .with_width(width)
        .with_line_cap(LineCap::Round)
        .with_line_join(LineJoin::Round)
}

fn grow(r: Rectangle, by: f32) -> Rectangle {
    Rectangle::new(
        r.position() - Vector::new(by, by),
        Size::new(r.width + 2.0 * by, r.height + 2.0 * by),
    )
}

fn rounded(r: Rectangle, radius: f32) -> Path {
    let radius = radius.min(r.width / 2.0).min(r.height / 2.0).max(0.0);
    Path::rounded_rectangle(r.position(), r.size(), Radius::from(radius))
}

fn polygon(points: &[Point]) -> Path {
    Path::new(|b| {
        if let Some((&first, rest)) = points.split_first() {
            b.move_to(first);
            for &p in rest {
                b.line_to(p);
            }
            b.close();
        }
    })
}

fn polyline(points: impl IntoIterator<Item = Point>) -> Path {
    Path::new(|b| {
        let mut first = true;
        for p in points {
            if first {
                b.move_to(p);
                first = false;
            } else {
                b.line_to(p);
            }
        }
    })
}

fn polyline_length(points: &[Point]) -> f32 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// The part of `points` from `from` to `to` along it (screen lengths).
fn stretch(points: &[Point], from: f32, to: f32) -> Vec<Point> {
    let mut out = Vec::new();
    let mut walked = 0.0;
    for w in points.windows(2) {
        let len = w[0].distance(w[1]);
        let (a, b) = (walked, walked + len);
        walked = b;
        if b < from || a > to || len == 0.0 {
            continue;
        }
        let at = |d: f32| {
            let k = ((d - a) / len).clamp(0.0, 1.0);
            Point::new(
                w[0].x + (w[1].x - w[0].x) * k,
                w[0].y + (w[1].y - w[0].y) * k,
            )
        };
        if out.is_empty() {
            out.push(at(from.max(a)));
        }
        out.push(at(to.min(b)));
    }
    out
}

/// A light at `head` along `points` (screen lengths), with a trail that
/// fades behind it.
fn comet(frame: &mut Frame, points: &[Point], head: f32, look: &Look) {
    // A faint line shows the way it has come.
    frame.stroke(
        &polyline(stretch(points, 0.0, head)),
        soft(look.accent, 2.0, 0.28),
    );
    const PIECES: usize = 14;
    let tail = (head - TRAIL).max(0.0);
    for piece in 0..PIECES {
        let a = tail + (head - tail) * piece as f32 / PIECES as f32;
        let b = tail + (head - tail) * (piece + 1) as f32 / PIECES as f32;
        let k = (piece + 1) as f32 / PIECES as f32;
        frame.stroke(
            &polyline(stretch(points, a, b)),
            soft(look.accent, 1.5 + 2.5 * k, 0.9 * k * k),
        );
    }
    let Some(&at) = stretch(points, head, head).last() else {
        return;
    };
    for (radius, alpha) in [(18.0, 0.06), (12.0, 0.13), (7.5, 0.28), (4.6, 0.7)] {
        frame.fill(&Path::circle(at, radius), with_alpha(look.accent, alpha));
    }
    frame.fill(&Path::circle(at, 2.6), look.light);
}

/// How much bigger the diagram is on screen than in its own units.
fn zoom(spot: &Spot, view: &Viewer) -> f32 {
    let screen = (view.place)(spot.rect);
    if spot.rect.w > 0.0 {
        screen.width / spot.rect.w
    } else {
        1.0
    }
}

/// The outline a state's light follows, on screen: a rounded box (a bar
/// is one), a ring, a diamond.
enum Glow {
    Box(Rectangle, f32),
    Ring(Point, f32),
    Diamond(Rectangle),
}

impl Glow {
    fn of(spot: &Spot, view: &Viewer) -> Glow {
        let r = (view.place)(spot.rect);
        let radius = spot.radius * zoom(spot, view);
        match spot.shape {
            Shape::Start | Shape::End | Shape::History => Glow::Ring(r.center(), radius),
            Shape::Choice => Glow::Diamond(r),
            Shape::State | Shape::Bar => Glow::Box(r, radius),
        }
    }

    /// The box the outline fits in.
    fn bounds(&self) -> Rectangle {
        match *self {
            Glow::Box(r, _) | Glow::Diamond(r) => r,
            Glow::Ring(c, radius) => Rectangle::new(
                c - Vector::new(radius, radius),
                Size::new(2.0 * radius, 2.0 * radius),
            ),
        }
    }

    fn path(&self, out: f32) -> Path {
        match *self {
            Glow::Box(r, radius) => rounded(grow(r, out), radius + out),
            Glow::Ring(c, radius) => Path::circle(c, radius + out),
            Glow::Diamond(r) => {
                let r = grow(r, out);
                let c = r.center();
                polygon(&[
                    Point::new(c.x, r.y),
                    Point::new(r.x + r.width, c.y),
                    Point::new(c.x, r.y + r.height),
                    Point::new(r.x, c.y),
                ])
            }
        }
    }

    /// Covers in `color` what lies in its bounds but outside the outline:
    /// the corners a rectangular clip lets through.
    fn mask(&self, frame: &mut Frame, color: Color) {
        let r = self.bounds();
        let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.width, r.y + r.height);
        match *self {
            Glow::Box(_, radius) | Glow::Ring(_, radius) => {
                let radius = radius.min(r.width / 2.0).min(r.height / 2.0);
                if radius <= 0.5 {
                    return;
                }
                // Each corner: from the corner along one side, round the
                // arc, back to the corner.
                for (corner, along, then) in [
                    (
                        Point::new(x0, y0),
                        Vector::new(radius, 0.0),
                        Vector::new(0.0, radius),
                    ),
                    (
                        Point::new(x1, y0),
                        Vector::new(0.0, radius),
                        Vector::new(-radius, 0.0),
                    ),
                    (
                        Point::new(x1, y1),
                        Vector::new(-radius, 0.0),
                        Vector::new(0.0, -radius),
                    ),
                    (
                        Point::new(x0, y1),
                        Vector::new(0.0, -radius),
                        Vector::new(radius, 0.0),
                    ),
                ] {
                    let path = Path::new(|b| {
                        b.move_to(corner);
                        b.line_to(corner + along);
                        b.arc_to(corner, corner + then, radius);
                        b.close();
                    });
                    frame.fill(&path, color);
                }
            }
            Glow::Diamond(_) => {
                let c = r.center();
                for corner in [
                    [Point::new(x0, y0), Point::new(c.x, y0), Point::new(x0, c.y)],
                    [Point::new(x1, y0), Point::new(x1, c.y), Point::new(c.x, y0)],
                    [Point::new(x1, y1), Point::new(c.x, y1), Point::new(x1, c.y)],
                    [Point::new(x0, y1), Point::new(x0, c.y), Point::new(c.x, y1)],
                ] {
                    frame.fill(&polygon(&corner), color);
                }
            }
        }
    }

    /// A soft bloom around the outline and a crisp line on it.
    fn draw(&self, frame: &mut Frame, accent: Color, level: f32) {
        const LAYERS: usize = 7;
        for layer in 0..LAYERS {
            let k = 1.0 - layer as f32 / LAYERS as f32;
            frame.stroke(
                &self.path(1.5 + layer as f32 * 1.6),
                soft(accent, 2.0 + layer as f32 * 1.4, 0.26 * level * k.powf(1.7)),
            );
        }
        frame.stroke(&self.path(0.5), soft(accent, 2.2, level));
    }

    /// A ring spreading out from the outline as `k` goes from 0 to 1.
    fn ripple(&self, frame: &mut Frame, accent: Color, k: f32) {
        frame.stroke(
            &self.path(2.0 + 20.0 * k),
            soft(accent, 2.0, 0.6 * (1.0 - k)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lights_fade_on_and_off() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        let mut events = Vec::new();
        assert_eq!(level(&events, ms(100)), 0.0);
        light(&mut events, ms(0), true);
        assert!(level(&events, ms(40)) > 0.0 && level(&events, ms(40)) < 1.0);
        assert!((level(&events, ms(1000)) - 1.0).abs() < 1e-6);
        light(&mut events, ms(1000), false);
        assert!(level(&events, ms(1100)) < 1.0);
        assert_eq!(level(&events, ms(5000)), 0.0);
        // On again before it went quite out: it rises from where it was.
        light(&mut events, ms(1100), true);
        assert!(level(&events, ms(1100)) > 0.3);
    }

    #[test]
    fn the_lit_line_follows_the_light_through_a_step_that_follows_by_itself() {
        let source = "@startuml\n[*] --> Brewing\nstate Brewing {\n  [*] --> Heating\n  Heating --> Pouring : hot\n  Pouring --> [*]\n}\n@enduml\n";
        let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
        let t0 = Instant::now();
        let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
        // Long after the start has arrived, the writer picks hot.
        let t1 = t0 + Duration::from_secs(10);
        let _ = play.take_jump(t1);
        play.ask(Ask::Take(0), t1);
        let Play::Machine(machine) = &play else {
            panic!("no machine");
        };
        let lines: Vec<_> = machine.lines.clone();
        assert_eq!(
            lines.iter().map(|&(_, line)| line).collect::<Vec<_>>(),
            [4, 5],
            "hot, then Pouring --> [*] by itself"
        );
        assert!(lines[0].0 < lines[1].0);
        assert_eq!(play.take_jump(lines[0].0), Some(4));
        assert_eq!(play.take_jump(lines[1].0 - Duration::from_millis(1)), None);
        assert_eq!(play.take_jump(lines[1].0), Some(5));
    }

    #[test]
    fn lines_nobody_takes_do_not_pile_up() {
        // The preview lights no line; a long play keeps only the recent.
        let source = "@startuml\n[*] --> Off\nOff --> On : switch\nOn --> Off : switch\n@enduml\n";
        let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
        let t0 = Instant::now();
        let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
        for n in 1..=50 {
            play.ask(Ask::Take(0), t0 + Duration::from_secs(10 * n));
        }
        assert!(play.line_waiting());
        let Play::Machine(machine) = &play else {
            panic!("no machine");
        };
        assert_eq!(machine.lines.len(), 1);
    }

    #[test]
    fn a_machine_plays_steps_one_after_another() {
        let source = "@startuml\n[*] --> Closed\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
        let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
        let t0 = Instant::now();
        let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
        let Offer::Machine { options, at, .. } = play.offer() else {
            panic!("no machine");
        };
        assert_eq!(options, [("lock".to_owned(), "Locked".to_owned())]);
        assert_eq!(at, "Closed");
        play.ask(Ask::Take(0), t0);
        // Each line lights when its step's light sets off: first the start.
        assert_eq!(play.take_jump(t0), None);
        assert_eq!(play.take_jump(t0 + VEIL_FADE), Some(1));
        assert_eq!(play.take_jump(t0 + Duration::from_secs(10)), Some(2));
        assert_eq!(play.take_jump(t0 + Duration::from_secs(10)), None);
        assert!(play.animating(t0 + Duration::from_millis(300)));
        assert!(!play.animating(t0 + Duration::from_secs(10)));
        let Offer::Machine {
            at,
            steps,
            can_go_back,
            ..
        } = play.offer()
        else {
            panic!("no machine");
        };
        assert_eq!((at.as_str(), steps, can_go_back), ("Locked", 1, true));
        play.ask(Ask::Back, t0);
        let Offer::Machine { at, steps, .. } = play.offer() else {
            panic!("no machine");
        };
        assert_eq!((at.as_str(), steps), ("Closed", 0));
        // The light fades back. The travel that was on the way does not
        // finish first.
        assert!(play.animating(t0 + Duration::from_millis(200)));
        assert!(!play.animating(t0 + Duration::from_millis(1500)));
    }

    #[test]
    fn a_history_is_on_the_map_and_named_in_the_bar() {
        let source = "@startuml\n[*] --> Off\nOff --> On : power\nstate On {\n  [*] --> Low\n  Low --> High : up\n}\nOn --> Off : pause\nOff --> On[H] : resume\nOff --> On[H*] : deep\n@enduml\n";
        let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
        let machine = Machine::read(source, Language::PlantUml).unwrap();
        for (state, place) in machine.states.iter().zip(&machine.place(&map).states) {
            assert!(place.is_some(), "{state:?} not on the map");
        }
        let t0 = Instant::now();
        let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
        let Offer::Machine { options, .. } = play.offer() else {
            panic!("no machine");
        };
        let option = |label: &str, to: &str| (label.to_owned(), to.to_owned());
        assert_eq!(
            options,
            [
                option("power", "On"),
                option("resume", "history of On"),
                option("deep", "deep history of On"),
            ]
        );
        // On, Low, High, back to Off, and through the history to High.
        for n in [0, 0, 0, 1] {
            play.ask(Ask::Take(n), t0);
        }
        let Offer::Machine { at, .. } = play.offer() else {
            panic!("no machine");
        };
        assert_eq!(at, "High");
    }

    #[test]
    fn a_trace_plays_line_by_line() {
        let source = "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
        let trace = [
            r#"{"time":"10:30:01","object":"Door","event":"lock"}"#,
            r#"{"object":"Car","event":"park"}"#,
            r#"{"time":"10:30:02","object":"Door","event":"open"}"#,
            r#"{"object":"Door","event":"unlock","state":"Closed"}"#,
        ]
        .join("\n");
        let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
        let t0 = Instant::now();
        let mut play = Play::trace(
            source,
            Language::PlantUml,
            &map,
            "door.puml",
            &trace,
            "run.jsonl",
            "Door",
            t0,
        )
        .unwrap();
        let bar = |play: &Play| match play.offer() {
            Offer::Trace {
                played,
                line,
                said,
                at,
                finding,
                ..
            } => (played, line, said, at, finding.map(|(s, _)| s)),
            other => panic!("{other:?}"),
        };
        assert_eq!(bar(&play), (0, None, String::new(), "Closed".into(), None));
        play.ask(Ask::Next, t0);
        assert_eq!(
            play.take_jump(t0 + Duration::from_secs(10)),
            Some(4),
            "the lock line of the diagram"
        );
        assert_eq!(
            bar(&play),
            (1, Some(1), "lock".into(), "Locked".into(), None)
        );
        // Digits pick nothing in a trace.
        play.ask(Ask::Take(0), t0);
        play.ask(Ask::Next, t0);
        assert_eq!(
            bar(&play),
            (
                2,
                Some(3),
                "open".into(),
                "Locked".into(),
                Some(Severity::Error)
            )
        );
        play.ask(Ask::Next, t0);
        assert_eq!(
            bar(&play),
            (3, Some(4), "unlock → Closed".into(), "Closed".into(), None)
        );
        play.ask(Ask::Next, t0);
        assert_eq!(bar(&play).0, 3, "the end of the trace");
        play.ask(Ask::Back, t0);
        assert_eq!(bar(&play).0, 2);
        play.ask(Ask::Restart, t0);
        assert_eq!(bar(&play).0, 0);
        assert!(!play.animating(t0 + Duration::from_secs(10)));
        let err = Play::trace(
            source,
            Language::PlantUml,
            &map,
            "door.puml",
            &trace,
            "run.jsonl",
            "Ghost",
            t0,
        )
        .err();
        assert_eq!(err.as_deref(), Some("run.jsonl has no line of Ghost"));
    }

    #[test]
    fn a_scenario_unfolds_message_by_message() {
        let source = "@startuml\nA -> B : one\nB -> A : two\n@enduml\n";
        let (_, map) = omaspec_uml::render_mapped(source, None).unwrap();
        let t0 = Instant::now();
        let mut play = Play::new(source, Language::PlantUml, &map, t0).unwrap();
        assert_eq!(play.offer(), Offer::Scenario { at: 0, of: 2 });
        play.ask(Ask::Next, t0);
        play.ask(Ask::Next, t0);
        play.ask(Ask::Next, t0);
        assert_eq!(play.offer(), Offer::Scenario { at: 2, of: 2 });
        assert_eq!(play.take_jump(t0), Some(2));
        play.ask(Ask::Restart, t0);
        assert_eq!(play.offer(), Offer::Scenario { at: 0, of: 2 });
        let flow = Play::new(
            "@startuml\nstart\n:a;\nstop\n@enduml\n",
            Language::PlantUml,
            &Map::default(),
            t0,
        );
        assert!(flow.is_err());
    }

    #[test]
    fn mermaid_machines_find_their_places() {
        let source = "stateDiagram-v2\n    [*] --> Idle\n    Idle --> Busy : job\n    state Busy {\n        [*] --> Work\n        Work --> Check : done\n        --\n        [*] --> Watch\n        Watch --> Watch : tick\n    }\n    Busy --> Idle : stop\n    Idle --> [*] : off\n";
        let drawn = super::super::testing::drawn(source, Language::Mermaid);
        let machine = Machine::read(source, Language::Mermaid).unwrap();
        let placed = machine.place(&drawn.map);
        for (state, place) in machine.states.iter().zip(&placed.states) {
            assert!(place.is_some(), "{state:?} not on {:#?}", drawn.map.states);
        }
        for (t, route) in machine.transitions.iter().zip(&placed.routes) {
            assert!(route.is_some(), "{t:?} not on {:#?}", drawn.map.transitions);
        }
        let mut play = Play::new(source, Language::Mermaid, &drawn.map, Instant::now()).unwrap();
        play.ask(Ask::Take(0), Instant::now());
        let Offer::Machine { at, .. } = play.offer() else {
            panic!("no machine");
        };
        assert_eq!(at, "Work, Watch");
    }

    #[test]
    fn a_stretch_follows_the_route() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
        ];
        let part = stretch(&points, 5.0, 15.0);
        assert_eq!(
            part,
            [
                Point::new(5.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 5.0)
            ]
        );
        // A head past the end rests on the last point.
        assert_eq!(
            stretch(&points, 20.0, 20.0).last(),
            Some(&Point::new(10.0, 10.0))
        );
    }
}
