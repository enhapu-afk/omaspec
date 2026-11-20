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
//! while a light runs along each arrow. The light is one stroke: the part
//! it has passed is already as bright as it will rest, and the head eases
//! into that, so the joins of the highlight never show.

use std::sync::Arc;
use std::time::Duration;

use iced::time::Instant;
use iced::widget::canvas::Frame;
use iced::{Color, Point, Rectangle};
use omaspec_core::Language;
use omaspec_core::lint::Severity;
use omaspec_model::play::Graph;
use omaspec_model::{Machine, Placed, Spec};
use omaspec_render::Map;
use omaspec_uml::graph::Rect;

mod draw;
mod machine;
mod scenario;
#[cfg(test)]
mod tests;

use draw::*;
pub(super) use draw::{Glow, with_alpha};
pub use machine::MachinePlay;
use machine::{Driver, TraceRun, Writer};
pub use scenario::ScenarioPlay;

/// How deep the shade over the stretch of a message the light has not
/// reached yet is.
const AHEAD: f32 = 0.6;
/// How long the veil takes to fall over the diagram, or lift off it.
const VEIL_FADE: Duration = Duration::from_millis(320);
/// How dark the veil gets, over the paper.
pub(super) const VEIL: f32 = 0.62;
/// How far around the words of a transition offered the veil is open.
const WORDS_AIR: f32 = 2.0;
/// How long a state's light takes to come on or go out.
const LIGHT_FADE: Duration = Duration::from_millis(420);
/// How long the ripple of an arrival spreads.
const RIPPLE: Duration = Duration::from_millis(620);
/// Steps a machine remembers for going back.
const MAX_BACK: usize = 500;
/// How long the states flash when the machine cannot follow: two pulses.
const FLASH: Duration = Duration::from_millis(1100);

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
            let graph = Graph::of(&machine)
                .ok_or("the state machine has no start ([*] --> …), so it cannot run")?;
            let (run, steps) = (graph.runs[0].clone(), graph.begun.clone());
            let writer = Writer {
                every: Arc::new(machine::every_offer(&machine, &graph)),
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
            grounds: map.grounds.clone(),
            at: 0,
            moved: now,
            from_y: map.messages[0].top,
            begun: now,
            jump: None,
            onward: true,
            leave: None,
            wrong: Vec::new(),
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
        Play::followed(map, spec.machine, followed, name, object, false, now)
    }

    /// A play of `object`'s way through `machine`, drawn as `map`, line by
    /// line as a trace is played: one followed already, as a scenario's
    /// participant is beside it ([`omaspec_model::lockstep`]); `name` is
    /// what it was followed from. `beside`: a scenario's participant, which
    /// the scenario's play steps on.
    #[allow(clippy::too_many_arguments)]
    pub fn followed(
        map: &Map,
        machine: Machine,
        followed: omaspec_model::Followed,
        name: &str,
        object: &str,
        beside: bool,
        now: Instant,
    ) -> Result<Play, String> {
        let placed = placed(&machine, map)?;
        let (start, begins) = (followed.start.clone(), followed.begins.clone());
        let driver = Driver::Trace(Box::new(TraceRun {
            name: name.to_owned(),
            object: object.to_owned(),
            followed,
            played: 0,
            beside,
        }));
        let mut play = MachinePlay::new(machine, placed, driver, now);
        play.schedule(&start, now);
        play.aim(&begins, now);
        Ok(Play::Machine(Box::new(play)))
    }

    /// Runs the light of the messages on `lines` in the color of what
    /// deviates: their receivers' machines cannot take them.
    pub fn mark_wrong(&mut self, lines: Vec<usize>) {
        if let Play::Scenario(play) = self {
            play.wrong = lines;
        }
    }

    /// When the light of a scenario's message played last reaches its
    /// head; none for a machine, or before the first.
    pub fn arrives(&self) -> Option<Instant> {
        match self {
            Play::Scenario(play) => play.arrives(),
            Play::Machine(_) => None,
        }
    }

    pub fn ask(&mut self, ask: Ask, now: Instant) {
        match self {
            Play::Machine(play) => play.ask(ask, now),
            Play::Scenario(play) => play.ask(ask, now),
        }
    }

    /// Where a following camera looks ([`super::Paper::follow_play`]): what the
    /// last steps of a machine left and entered and the ways between them,
    /// or the message just played; none before the first.
    pub fn focus(&self) -> Option<iced::Rectangle> {
        match self {
            Play::Machine(play) => play.focus,
            Play::Scenario(play) => play.focus(),
        }
    }

    /// Takes each state's own shape from the picture `tree` it is played
    /// on, where the picture marks its parts (docs/adr/0029): a state comes
    /// through the veil as it is drawn, as the focus lets a box through.
    pub fn shape(&mut self, tree: &omaspec_render::export::usvg::Tree) {
        if let Play::Machine(play) = self {
            play.outlines = play
                .placed
                .states
                .iter()
                .map(|spot| super::focus::Outline::of(tree, &spot.as_ref()?.part))
                .collect();
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
                play.grounds = map.grounds.clone();
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
    /// The veil's color: the picture's own paper.
    pub veil: Color,
    pub light: Color,
    /// What deviates: a message its receiver's machine cannot take, a
    /// state that flashes as it cannot follow.
    pub wrong: Color,
}

impl Look {
    /// The look at `share` of its strength, as the play fades away.
    pub fn faded(self, share: f32) -> Look {
        Look {
            accent: with_alpha(self.accent, share),
            veil: with_alpha(self.veil, share),
            light: with_alpha(self.light, share),
            wrong: with_alpha(self.wrong, share),
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

/// Where the machine `source` draws can be and what it offers there, as a
/// play of it finds them; `None` for a diagram that is no state machine.
pub fn every_offer_of(source: &str, language: Language) -> Option<Vec<Offered>> {
    let machine = Machine::read(source, language)?;
    Some(machine::every_offer(&machine, &Graph::of(&machine)?))
}

/// Where a machine is, by the names of its states, and its options there,
/// each a label and the state it leads to.
pub type Offered = (String, Vec<(String, String)>);

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
        /// Where the machine can be in this play and what it offers there:
        /// the bar takes the rows the most of them need from the start, so
        /// it keeps its height.
        every: Arc<Vec<Offered>>,
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
        /// Played beside a scenario, which steps it: nothing to press.
        beside: bool,
    },
}
