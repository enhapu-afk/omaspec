//! The diagram pane of the preview and the editor window: the rendered
//! diagram on white paper, zoom and pan, sharp at any zoom, and a frame
//! whose color tells whether the picture is up to date.
//!
//! The last good picture stays while a newer source fails to render. A
//! new picture fades in over the one before. A state machine or a scenario
//! can be played in it ([`play`]). The name under the caret lights on the
//! sheet, in the theme accent, and glides there the way a zoom does. With
//! Alt, a line can be drawn by hand ([`hand`]).

mod canvas;
mod hand;
mod play;
mod raster;
mod view;

use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use iced::animation::Easing;
use iced::time::Instant;
use iced::widget::{Canvas, Stack, button, column, container, mouse_area, row, text};
use iced::{Animation, Element, Font, Length, Point, Rectangle, Size, Task, Theme, Vector};
use omaspec_core::Language;
use omaspec_core::lint::Severity;
use omaspec_core::locate;
use omaspec_model::Spec;
use omaspec_render::Place;
use omaspec_render::export::{self, Format};
use omaspec_render::{RenderError, pick};
use omaspec_theme::{Palette, Rgb};

use crate::ui;
use crate::worker::{Answer, Drawn, Ticket, Worker, blocking};
pub use hand::Gesture;
pub use play::{Ask, Offer, Play};
use raster::{Layer, Plan, Raster};
pub use view::{Glide, View};

/// Wait this long after the last zoom or pan step before rasterizing sharp
/// again.
const RASTER_DELAY: Duration = Duration::from_millis(120);
/// How long a new picture takes to fade in over the one before.
const DIAGRAM_FADE: Duration = Duration::from_millis(180);

#[derive(Debug, Clone)]
pub enum Message {
    Overview(u64, Option<Raster>),
    Detail(u64, Option<Raster>),
    Viewport(Size),
    ZoomAt(f32, Point),
    Pan(Vector),
    /// Touchscreen pinch: zoom by `factor` at `at` after panning by `pan`.
    Pinch {
        factor: f32,
        at: Point,
        pan: Vector,
    },
    Fit,
    /// A wheel notch or a key: zooms by `factor` at this point, gliding.
    ZoomStep(f32, Point),
    /// A glide has arrived; the sharp pixels can come.
    Glided,
    /// Ctrl+click at this point of the pane; the window asks
    /// [`Paper::label_at`] what is there.
    Pick(Point),
    /// A new picture has faded in; the one before can go.
    Faded,
    /// The writer's step in the play.
    Play(Ask),
    /// The play fades away.
    StopPlay,
    /// The play has faded away.
    PlayGone,
    /// What the pointer does with Alt: a line drawn by hand. Its new bends
    /// wait for the window in [`Paper::take_by_hand`].
    Hand(Gesture),
}

/// The pictures of the diagram before, fading out under the new one.
pub struct Fade {
    old: Vec<Raster>,
    /// Goes to `true` as the new picture comes in.
    shown: Animation<bool>,
}

/// The diagram on screen.
pub struct Diagram {
    pub drawn: Drawn,
    pub size: Size,
    pub source: Arc<str>,
    pub language: Language,
}

pub enum Status {
    Waiting,
    Rendering,
    InSync,
    Failed(RenderError),
    /// The source could not be read.
    Unreadable(String),
}

pub struct Paper {
    pub diagram: Option<Diagram>,
    pub status: Status,
    /// Numbers render requests; answers to older ones are ignored.
    requested: u64,
    shown: u64,
    pub view: View,
    /// The way the picture glides to `view` after a stepped zoom.
    glide: Option<Glide>,
    viewport: Size,
    scale_factor: f32,
    overview: Layer,
    detail: Layer,
    /// A new diagram came; its first overview raster starts the fade.
    fresh: bool,
    /// The pictures on screen when the new diagram came.
    before: Vec<Raster>,
    fade: Option<Fade>,
    /// A state machine or scenario being played.
    play: Option<Play>,
    /// When a stopped play began to fade away.
    leaving: Option<Instant>,
    /// The moment a play is drawn at, for pictures of one moment; `None`
    /// draws each frame at its own moment.
    clock: Option<Instant>,
    /// The name under the caret, gliding to its place on the sheet.
    breath: Option<Breath>,
    /// The last caret lookup. A highlight sweep that does not move the
    /// caret must not search the sheet again.
    aim: Option<AimMemo>,
    /// Whether lines can be drawn by hand here: in the editor, not in the
    /// preview window, which follows a file another editor writes.
    pub by_hand: bool,
    /// The line drawn by hand, or lit with Alt.
    hand: Option<hand::Hand>,
    /// A line drawn by hand, with the bends to write into the source.
    by_hand_done: Option<omaspec_uml::hand::Line>,
    /// The latest request when the hand's bends were sent: a picture asked
    /// for after it has them.
    hand_sent_at: u64,
}

impl Default for Paper {
    fn default() -> Self {
        Self {
            diagram: None,
            status: Status::Waiting,
            requested: 0,
            shown: 0,
            view: View::default(),
            glide: None,
            viewport: Size::ZERO,
            scale_factor: 1.0,
            overview: Layer::default(),
            detail: Layer::default(),
            fresh: false,
            before: Vec::new(),
            fade: None,
            play: None,
            leaving: None,
            clock: None,
            breath: None,
            aim: None,
            by_hand: false,
            hand: None,
            by_hand_done: None,
            hand_sent_at: 0,
        }
    }
}

/// Inputs of the last [`Paper::place_of`], and what it returned.
struct AimMemo {
    source: u64,
    shown: u64,
    row: u32,
    col: u32,
    place: Option<Rectangle>,
}

/// The light on the caret's name, on its way from one place and strength
/// to another. It takes as long as a zoom glide.
#[derive(Clone, Copy)]
struct Breath {
    from: Rectangle,
    to: Rectangle,
    start: Instant,
    from_a: f32,
    to_a: f32,
}

impl Breath {
    /// Where it shows at `now`, and how strongly (0 to 1).
    fn at(self, now: Instant) -> (Rectangle, f32) {
        let t = now.saturating_duration_since(self.start).as_secs_f32() / view::GLIDE.as_secs_f32();
        if t >= 1.0 {
            return (self.to, self.to_a);
        }
        // Quick at first, settling at the end, as a zoom glide.
        let t = 1.0 - (1.0 - t).powi(3);
        (
            mix_rect(self.from, self.to, t),
            self.from_a + (self.to_a - self.from_a) * t,
        )
    }

    fn settled(self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= view::GLIDE
    }
}

fn mix_rect(from: Rectangle, to: Rectangle, t: f32) -> Rectangle {
    let mix = |a: f32, b: f32| a + (b - a) * t;
    Rectangle {
        x: mix(from.x, to.x),
        y: mix(from.y, to.y),
        width: mix(from.width, to.width),
        height: mix(from.height, to.height),
    }
}

/// The same place, within half a pixel: not worth a new glide.
fn same_place(a: Rectangle, b: Rectangle) -> bool {
    let close = |p: f32, q: f32| (p - q).abs() < 0.5;
    close(a.x, b.x) && close(a.y, b.y) && close(a.width, b.width) && close(a.height, b.height)
}

impl Paper {
    /// Asks `worker` to render `source` for document `doc`, whose file is
    /// at `place`.
    pub fn request(
        &mut self,
        worker: &Worker,
        doc: u64,
        source: Arc<str>,
        language: Language,
        place: Option<Place>,
    ) {
        self.requested += 1;
        let ticket = Ticket {
            doc,
            id: self.requested,
        };
        worker.render(ticket, source, language, place);
        self.status = Status::Rendering;
    }

    /// The id of the answer on screen; older ones are dropped.
    pub fn shown(&self) -> u64 {
        self.shown
    }

    /// The id of the latest [`Self::request`].
    pub fn requested(&self) -> u64 {
        self.requested
    }

    /// Takes a render answer: shows the diagram, or keeps the last one and
    /// shows the error. Answers older than the one on screen are dropped.
    pub fn rendered(&mut self, answer: Answer) -> Task<Message> {
        let id = answer.ticket.id;
        if id <= self.shown {
            return Task::none();
        }
        self.shown = id;
        let latest = id == self.requested;
        match answer.result {
            Ok(drawn) => {
                // The lines are drawn anew: the pointer finds them again,
                // but a drag goes on, and bends on their way wait for the
                // picture that has them.
                let theirs = id > self.hand_sent_at;
                self.hand = match (self.hand.take(), &self.diagram) {
                    (Some(hand), Some(old)) => hand.redrawn(&old.drawn.map, &drawn.map, theirs),
                    _ => None,
                };
                // A changed diagram ends its play; the same one drawn again
                // (in a new theme or font) keeps it, in the new places.
                if let Some(play) = &mut self.play {
                    let same = self.diagram.as_ref().is_some_and(|d| {
                        d.source == answer.source && d.language == answer.language
                    });
                    if same {
                        play.remap(&drawn.map);
                    } else {
                        self.play = None;
                    }
                }
                if !self.fresh {
                    self.before = [self.overview.shown.clone(), self.detail.shown.clone()]
                        .into_iter()
                        .flatten()
                        .collect();
                    self.fresh = true;
                }
                let size = drawn.tree.size();
                self.diagram = Some(Diagram {
                    size: Size::new(size.width(), size.height()),
                    drawn,
                    source: answer.source,
                    language: answer.language,
                });
                if latest {
                    self.status = Status::InSync;
                }
                if self.view.fit {
                    self.view = View::fitted(self.diagram_size(), self.viewport);
                }
                // Same plans, new pixels.
                self.overview.forget();
                self.detail.forget();
                self.rasterize(Duration::ZERO)
            }
            Err(err) => {
                // The bends sent did not draw; the old picture stays.
                if id > self.hand_sent_at && self.hand.as_ref().is_some_and(|h| h.sent) {
                    self.hand = None;
                }
                if latest {
                    self.status = Status::Failed(err);
                }
                Task::none()
            }
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Overview(seq, raster) => {
                if self.overview.receive(seq, raster) && std::mem::take(&mut self.fresh) {
                    self.start_fade(Instant::now());
                }
                Task::none()
            }
            Message::Detail(seq, raster) => {
                let _ = self.detail.receive(seq, raster);
                Task::none()
            }
            Message::Faded => {
                self.fade = None;
                Task::none()
            }
            Message::Viewport(size) => {
                self.settle();
                self.viewport = size;
                if self.view.fit {
                    self.refit()
                } else {
                    self.rasterize(RASTER_DELAY)
                }
            }
            Message::ZoomAt(factor, at) => {
                self.settle();
                self.view = self.view.zoom_at(factor, at, self.viewport);
                self.rasterize(RASTER_DELAY)
            }
            Message::ZoomStep(factor, at) => {
                let to = self.view.zoom_at(factor, at, self.viewport);
                self.glide_to(to, at)
            }
            Message::Glided => match self.glide {
                Some(glide) if glide.done(Instant::now()) => {
                    self.glide = None;
                    self.rasterize(Duration::ZERO)
                }
                _ => Task::none(),
            },
            Message::Pan(delta) => {
                self.settle();
                self.view = self.view.pan(delta);
                self.rasterize(RASTER_DELAY)
            }
            Message::Pinch { factor, at, pan } => {
                self.settle();
                self.view = self.view.pinch(factor, at, pan, self.viewport);
                self.rasterize(RASTER_DELAY)
            }
            Message::Fit => self.fit(),
            Message::Pick(_) => Task::none(),
            Message::Play(ask) => {
                // On the clock the play is drawn at, so that a picture of
                // one moment takes its steps at that moment too.
                let now = self.clock.unwrap_or_else(Instant::now);
                if let Some(play) = self.play.as_mut().filter(|_| self.leaving.is_none()) {
                    play.ask(ask, now);
                }
                Task::none()
            }
            Message::StopPlay => {
                if self.play.is_some() && self.leaving.is_none() {
                    self.leaving = Some(Instant::now());
                }
                Task::none()
            }
            Message::PlayGone => {
                self.play = None;
                self.leaving = None;
                Task::none()
            }
            Message::Hand(gesture) => {
                let Some(diagram) = self.diagram.as_ref().filter(|_| self.drawable()) else {
                    self.hand = None;
                    return Task::none();
                };
                let map = Arc::clone(&diagram.drawn.map);
                let (view, viewport) = (self.view, self.viewport);
                let to_diagram = |at: Point| {
                    let p = view.to_diagram(at, viewport);
                    (p.x, p.y)
                };
                if let Some(line) = hand::Hand::gesture(
                    &mut self.hand,
                    gesture,
                    &map,
                    to_diagram,
                    hand::reach(view.zoom),
                ) {
                    self.by_hand_done = Some(line);
                    self.hand_sent_at = self.requested;
                }
                Task::none()
            }
        }
    }

    /// The line just drawn by hand, with the bends to write into the
    /// source; once.
    pub fn take_by_hand(&mut self) -> Option<omaspec_uml::hand::Line> {
        self.by_hand_done.take()
    }

    /// The corners just taken changed nothing in the source: no picture
    /// will come for them.
    pub fn by_hand_unchanged(&mut self) {
        if let (Some(hand), Some(diagram)) = (&mut self.hand, &self.diagram) {
            hand.unchanged(&diagram.drawn.map);
        }
    }

    /// The line drawn at `at`, a point of the pane, if one runs there.
    pub fn wire_at(&self, at: Point) -> Option<&omaspec_uml::map::Wire> {
        let map = &self.diagram.as_ref()?.drawn.map;
        let p = self.view.to_diagram(at, self.viewport);
        let index = hand::wire_at(map, (p.x, p.y), hand::reach(self.view.zoom))?;
        map.wires.get(index)
    }

    /// The line that `row` of the source draws, or whose bends it writes.
    pub fn wire_on(&self, row: usize) -> Option<&omaspec_uml::map::Wire> {
        let map = &self.diagram.as_ref()?.drawn.map;
        map.wires
            .iter()
            .find(|w| w.line == Some(row) || w.entry == Some(row))
    }

    /// A pane where lines can be drawn by hand: the editor's.
    pub fn editable() -> Self {
        Self {
            by_hand: true,
            ..Self::default()
        }
    }

    /// Whether a line can be drawn by hand now: here, with a picture of
    /// lines, and no play over it.
    fn drawable(&self) -> bool {
        self.by_hand
            && self.play.is_none()
            && matches!(
                self.status,
                Status::InSync | Status::Rendering | Status::Waiting
            )
            && self
                .diagram
                .as_ref()
                .is_some_and(|d| !d.drawn.map.wires.is_empty())
    }

    /// Plays the diagram on screen: its state machine, or its scenario.
    /// Says why not, if it cannot.
    pub fn start_play(&mut self) -> Result<(), String> {
        let diagram = self.diagram.as_ref().ok_or("no diagram yet")?;
        let play = Play::new(
            &diagram.source,
            diagram.language,
            &diagram.drawn.map,
            Instant::now(),
        )?;
        self.play = Some(play);
        self.leaving = None;
        // A play has lights of its own.
        self.set_breath(None);
        Ok(())
    }

    /// Plays the recorded `trace` (named `name`) of `object` on the state
    /// machine on screen, the diagram `file`. Says why not, if it cannot.
    pub fn start_trace(
        &mut self,
        trace: &str,
        name: &str,
        object: &str,
        file: &str,
    ) -> Result<(), String> {
        let diagram = self.diagram.as_ref().ok_or("no diagram yet")?;
        let play = Play::trace(
            &diagram.source,
            diagram.language,
            &diagram.drawn.map,
            file,
            trace,
            name,
            object,
            Instant::now(),
        )?;
        self.play = Some(play);
        self.leaving = None;
        self.set_breath(None);
        Ok(())
    }

    /// The state machine on screen, as the diagram `file`; `None` if the
    /// diagram is no state machine.
    pub fn spec(&self, file: &str) -> Option<Spec> {
        let diagram = self.diagram.as_ref()?;
        Spec::read(&diagram.source, diagram.language, file)
    }

    /// Whether a play runs and takes steps; one fading away does not.
    pub fn playing(&self) -> bool {
        self.play.is_some() && self.leaving.is_none()
    }

    /// Whether the play that runs is a recorded trace.
    pub fn tracing(&self) -> bool {
        self.playing() && self.play.as_ref().is_some_and(Play::tracing)
    }

    /// Lights the place on the sheet that the label at `row`/`col` draws,
    /// or lets the light go out. `row` is a zero-based line and `col` a
    /// byte on that line. Nothing lights while a play runs.
    pub fn follow_source(&mut self, source: &str, row: u32, col: u32) {
        if self.playing() {
            self.set_breath(None);
            return;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        source.hash(&mut hasher);
        let source_hash = hasher.finish();
        if let Some(memo) = &self.aim
            && memo.source == source_hash
            && memo.shown == self.shown
            && memo.row == row
            && memo.col == col
        {
            self.set_breath(memo.place);
            return;
        }
        let place = self.place_of(
            source,
            usize::try_from(row).unwrap_or(usize::MAX),
            usize::try_from(col).unwrap_or(usize::MAX),
        );
        self.aim = Some(AimMemo {
            source: source_hash,
            shown: self.shown,
            row,
            col,
            place,
        });
        self.set_breath(place);
    }

    /// Whether the light on the caret's name is still on its way.
    pub fn breath_moving(&self, now: Instant) -> bool {
        self.breath.is_some_and(|breath| !breath.settled(now))
    }

    /// The rectangle the light is gliding toward, in diagram units, while
    /// it is coming in or holding.
    #[cfg(test)]
    pub fn breath_target(&self) -> Option<Rectangle> {
        self.breath
            .filter(|breath| breath.to_a > 0.0)
            .map(|breath| breath.to)
    }

    /// The box of the label at `row`/`col`: the smallest shape around that
    /// occurrence. A comment line draws nothing. When the sheet draws fewer
    /// copies than the source mentions (a lifeline's head and its foot), the
    /// later mention keeps the last copy.
    fn place_of(&self, source: &str, row: usize, col: usize) -> Option<Rectangle> {
        let diagram = self.diagram.as_ref()?;
        let texts = pick::texts(&diagram.drawn.tree);
        let (text, nth) = locate::aimed(source, row, col, texts.iter().map(String::as_str))?;
        let copies = texts.iter().filter(|drawn| drawn.as_str() == text).count();
        if copies == 0 {
            return None;
        }
        let nth = nth.min(copies - 1);
        let (x, y, w, h) = pick::spot(&diagram.drawn.tree, &text, nth)?;
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        Some(Rectangle::new(Point::new(x, y), Size::new(w, h)))
    }

    /// Moves the light to `target`, or fades it out. The same place does
    /// not restart the glide. From nothing it fades in where it is going;
    /// from a light already showing it glides across.
    fn set_breath(&mut self, target: Option<Rectangle>) {
        let now = Instant::now();
        match target {
            Some(to) => {
                let Some(breath) = self.breath else {
                    self.breath = Some(Breath {
                        from: to,
                        to,
                        start: now,
                        from_a: 0.0,
                        to_a: 1.0,
                    });
                    return;
                };
                if breath.to_a == 1.0 && same_place(breath.to, to) {
                    return;
                }
                let (shown, alpha) = breath.at(now);
                if alpha > 0.05 {
                    self.breath = Some(Breath {
                        from: shown,
                        to,
                        start: now,
                        from_a: alpha,
                        to_a: 1.0,
                    });
                } else {
                    self.breath = Some(Breath {
                        from: to,
                        to,
                        start: now,
                        from_a: 0.0,
                        to_a: 1.0,
                    });
                }
            }
            None => {
                let Some(breath) = self.breath else {
                    return;
                };
                if breath.to_a == 0.0 {
                    if breath.settled(now) {
                        self.breath = None;
                    }
                    return;
                }
                let (shown, alpha) = breath.at(now);
                if alpha <= 0.01 {
                    self.breath = None;
                    return;
                }
                self.breath = Some(Breath {
                    from: shown,
                    to: shown,
                    start: now,
                    from_a: alpha,
                    to_a: 0.0,
                });
            }
        }
    }

    /// The source line of the play's step whose light set off last by
    /// `now`, once; none once the play stops.
    pub fn take_jump(&mut self, now: Instant) -> Option<usize> {
        if !self.playing() {
            return None;
        }
        let now = self.clock.unwrap_or(now);
        self.play.as_mut()?.take_jump(now)
    }

    /// Whether a step's line waits for its light to set off.
    pub fn line_waiting(&self) -> bool {
        self.playing() && self.play.as_ref().is_some_and(Play::line_waiting)
    }

    /// Fades from the pictures before to the new diagram's overview. A
    /// detail of the diagram before goes with them; the new one comes sharp
    /// once it is rasterized.
    fn start_fade(&mut self, now: Instant) {
        let old = std::mem::take(&mut self.before);
        if self
            .detail
            .shown
            .as_ref()
            .is_some_and(|detail| old.iter().any(|o| o.handle.id() == detail.handle.id()))
        {
            self.detail.shown = None;
        }
        self.fade = Some(Fade {
            old,
            shown: Animation::new(false)
                .duration(DIAGRAM_FADE)
                .easing(Easing::EaseOut)
                .go(true, now),
        });
    }

    /// The label drawn at `at` (a point of the pane), with the source the
    /// picture was drawn from.
    pub fn label_at(&self, at: Point) -> Option<(pick::Label, Arc<str>)> {
        let diagram = self.diagram.as_ref()?;
        let point = self.view.to_diagram(at, self.viewport);
        let label = pick::label_at(&diagram.drawn.tree, point.x, point.y)?;
        Some((label, Arc::clone(&diagram.source)))
    }

    pub fn set_scale_factor(&mut self, factor: f32) -> Task<Message> {
        self.scale_factor = factor;
        self.rasterize(Duration::ZERO)
    }

    /// Zooms by `factor` around the middle of the pane.
    pub fn zoom(&mut self, factor: f32) -> Task<Message> {
        let center = Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0);
        self.update(Message::ZoomStep(factor, center))
    }

    pub fn pan(&mut self, dx: f32, dy: f32) -> Task<Message> {
        self.update(Message::Pan(Vector::new(dx, dy)))
    }

    /// The diagram at 100 %, gliding there.
    pub fn actual_size(&mut self) -> Task<Message> {
        let center = Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0);
        self.glide_to(self.view.actual_size(), center)
    }

    /// Glides from the view on screen now to `to`, around `at`. The sharp
    /// pixels for `to` are asked for at once, so they are in about when the
    /// glide arrives; a few wheel notches in a row ask once.
    fn glide_to(&mut self, to: View, at: Point) -> Task<Message> {
        let now = Instant::now();
        let from = self.glide.map_or(self.view, |g| g.at(now));
        self.view = to;
        self.glide = Some(Glide::new(from, to, at, self.viewport, now));
        self.rasterize(RASTER_DELAY)
    }

    /// Stops a glide where it is: a pan or a pinch goes on from there.
    fn settle(&mut self) {
        if let Some(glide) = self.glide.take() {
            self.view = glide.at(Instant::now());
        }
    }

    /// Takes `view` from another pane (Compare A|B zooms both together): a
    /// fitted one fits this diagram.
    pub fn follow(&mut self, view: View) -> Task<Message> {
        self.settle();
        if view.fit {
            return self.refit();
        }
        self.view = view;
        self.rasterize(RASTER_DELAY)
    }

    /// Whether `message` moves or zooms the view (and another pane should
    /// follow).
    pub fn moves_view(message: &Message) -> bool {
        matches!(
            message,
            Message::ZoomAt(..)
                | Message::ZoomStep(..)
                | Message::Pan(_)
                | Message::Pinch { .. }
                | Message::Fit
        )
    }

    /// The whole diagram in the pane, following its size from now on;
    /// it glides there.
    pub fn fit(&mut self) -> Task<Message> {
        if self.diagram.is_none() || self.viewport == Size::ZERO {
            self.view.fit = true;
            return Task::none();
        }
        let center = Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0);
        self.glide_to(View::fitted(self.diagram_size(), self.viewport), center)
    }

    /// Fits again at once, after the pane or the diagram changed.
    fn refit(&mut self) -> Task<Message> {
        if self.diagram.is_none() || self.viewport == Size::ZERO {
            self.view.fit = true;
            return Task::none();
        }
        self.view = View::fitted(self.diagram_size(), self.viewport);
        self.rasterize(Duration::ZERO)
    }

    /// Drops the pixels while the pane is hidden; a viewport report brings
    /// them back.
    pub fn release(&mut self) {
        self.overview.clear();
        self.detail.clear();
        self.fresh = false;
        self.before.clear();
        self.fade = None;
    }

    fn diagram_size(&self) -> Size {
        self.diagram
            .as_ref()
            .map_or(Size::new(1.0, 1.0), |d| d.size)
    }

    /// Rasterizes what the view needs sharp, after `delay` (see
    /// [`raster`]); asks nothing when the rasters in hand will do.
    fn rasterize(&mut self, delay: Duration) -> Task<Message> {
        let Some(diagram) = &self.diagram else {
            return Task::none();
        };
        if self.viewport == Size::ZERO {
            return Task::none();
        }
        let scale = self.view.zoom * self.scale_factor;
        let visible = self.view.visible(self.viewport);
        let overview = Plan::overview(diagram.size, scale);
        let detail = Plan::detail(diagram.size, scale, visible, self.detail.asked);
        let tree = &diagram.drawn.tree;
        Task::batch([
            self.overview.ask(overview, tree, delay, Message::Overview),
            self.detail.ask(detail, tree, delay, Message::Detail),
        ])
    }

    /// Copies the diagram, with its source inside, to the clipboard; says
    /// what it copied.
    pub fn copy(&self, format: Format, file: &str) -> Task<Result<&'static str, String>> {
        self.exported(format, file, move |bytes| {
            crate::files::copy_to_clipboard(&bytes, format.mime()).map_err(|e| format!("{e:#}"))?;
            Ok(match format {
                Format::Png => "PNG",
                Format::Svg => "SVG",
                Format::Jpeg => "JPEG",
                Format::Pdf => "PDF",
                Format::Markdown => "Markdown",
                Format::Zip => "ZIP",
            })
        })
    }

    /// Writes the diagram, with its source inside, to `out`; says where.
    pub fn export_to(
        &self,
        format: Format,
        file: &str,
        out: PathBuf,
    ) -> Task<Result<String, String>> {
        self.exported(format, file, move |bytes| {
            crate::files::write_atomic(&out, &bytes).map_err(|e| format!("{e:#}"))?;
            Ok(out.display().to_string())
        })
    }

    /// Exports the diagram on screen in the background and hands the bytes
    /// to `then`. `file` is the source's file name, stored with it.
    fn exported<T: Send + 'static>(
        &self,
        format: Format,
        file: &str,
        then: impl FnOnce(Vec<u8>) -> Result<T, String> + Send + 'static,
    ) -> Task<Result<T, String>> {
        let Some(diagram) = &self.diagram else {
            return Task::done(Err("no diagram yet".into()));
        };
        let svg = Arc::clone(&diagram.drawn.svg);
        let source = Arc::clone(&diagram.source);
        let language = diagram.language;
        let file = file.to_owned();
        Task::perform(
            blocking(move || {
                let options = export::Options {
                    scale: 2.0,
                    source: Some(export::Source {
                        code: &source,
                        language,
                        file: Some(&file),
                    }),
                };
                let bytes = export::export(&svg, format, &options).map_err(|e| e.to_string())?;
                then(bytes)
            }),
            |result| result.unwrap_or_else(|| Err("export failed".into())),
        )
    }

    /// The diagram, and the play over it. Without a picture it says what
    /// is missing, unless `quiet` (the editor shows an empty state of its
    /// own then).
    pub fn view(&self, palette: &Palette, font: Font, quiet: bool) -> Element<'_, Message> {
        let p = palette;
        let (stale, placeholder) = match &self.status {
            Status::InSync => (false, ""),
            Status::Waiting | Status::Rendering => (false, "rendering…"),
            Status::Failed(_) | Status::Unreadable(_) => {
                (true, if quiet { "" } else { "no diagram yet" })
            }
        };
        let canvas: Element<'_, Message> = Canvas::new(canvas::DiagramCanvas {
            diagram: self.diagram.as_ref().map(|d| d.size),
            overview: self.overview.shown.as_ref(),
            detail: self.detail.shown.as_ref(),
            fade: self.fade.as_ref(),
            view: self.view,
            glide: self.glide,
            background: ui::color(p.background),
            placeholder,
            text: ui::color(p.dark_foreground),
            font,
            scale: self.scale_factor,
            by_hand: self.drawable(),
            hand_cursor: self.hand.as_ref().map(hand::Hand::cursor),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
        let hand = match (&self.hand, &self.diagram) {
            (Some(hand), Some(diagram)) if self.overview.shown.is_some() => Some(
                Canvas::new(hand::HandLayer {
                    hand,
                    map: &diagram.drawn.map,
                    view: self.view,
                    glide: self.glide,
                    accent: ui::color(p.accent),
                    paper: ui::color(p.background),
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            ),
            _ => None,
        };
        // Its own layer: in the picture's layer the raster would cover it.
        let light = self.breath.map(|breath| {
            Canvas::new(canvas::BreathLight {
                view: self.view,
                glide: self.glide,
                breath,
                color: ui::color(p.accent),
                scale: self.scale_factor,
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        });
        // The red edge lies over everything else, a play's veil too.
        let edge = match (&self.diagram, &self.overview.shown) {
            (Some(diagram), Some(_)) if stale => Some(
                Canvas::new(canvas::PaperEdge {
                    diagram: diagram.size,
                    view: self.view,
                    glide: self.glide,
                    color: ui::color(p.red),
                    scale: self.scale_factor,
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            ),
            _ => None,
        };
        let (Some(play), Some(diagram), Some(overview)) =
            (&self.play, &self.diagram, self.overview.shown.as_ref())
        else {
            return Stack::with_children([Some(canvas), light, hand, edge].into_iter().flatten())
                .into();
        };
        let layer = |over: bool| -> Element<'_, Message> {
            Canvas::new(canvas::PlayCanvas {
                diagram: diagram.size,
                overview,
                detail: self.detail.shown.as_ref(),
                view: self.view,
                glide: self.glide,
                play,
                look: play::Look {
                    accent: ui::color(p.accent),
                    veil: ui::color(p.background),
                    light: ui::color(p.bright_foreground),
                },
                over,
                at: self.clock,
                leaving: self.leaving,
                scale: self.scale_factor,
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };
        let diagram: Element<'_, Message> = Stack::with_children(
            [
                Some(canvas),
                light,
                Some(layer(false)),
                Some(layer(true)),
                edge,
            ]
            .into_iter()
            .flatten(),
        )
        .into();
        // The strip stays while the play fades, so the diagram does not
        // jump under it. Steps are ignored once it is leaving.
        column![diagram, play_bar(&play.offer(), p, font)].into()
    }

    /// The state for a status bar: what happened, in which color.
    pub fn state(&self, palette: &Palette) -> (String, Rgb) {
        let p = palette;
        match &self.status {
            Status::Waiting => ("waiting".to_owned(), p.dark_foreground),
            Status::Rendering => ("● rendering".to_owned(), p.yellow),
            Status::InSync => {
                let took = self.diagram.as_ref().map_or_else(String::new, |d| {
                    format!(
                        "✓ {} {:.0} ms",
                        d.drawn.engine.name(),
                        d.drawn.elapsed.as_secs_f64() * 1000.0
                    )
                });
                (took, p.green)
            }
            Status::Failed(err) => (format!("✕ {}", ui::one_line(&err.to_string())), p.red),
            Status::Unreadable(err) => (format!("✕ cannot read: {err}"), p.red),
        }
    }

    /// The diagram's state in a few quiet words, for the status line. What
    /// keeps the text from drawing is told at the diagram ([`Self::trouble`]).
    pub fn summary(&self) -> String {
        match &self.status {
            Status::Waiting => "waiting".to_owned(),
            Status::Rendering => "rendering…".to_owned(),
            Status::InSync => self.diagram.as_ref().map_or_else(String::new, |d| {
                format!(
                    "{} · {:.0} ms",
                    d.drawn.engine.name(),
                    d.drawn.elapsed.as_secs_f64() * 1000.0
                )
            }),
            Status::Failed(_) | Status::Unreadable(_) => match self.overview.shown {
                Some(_) => "the last good picture".to_owned(),
                None => "no picture".to_owned(),
            },
        }
    }

    /// What keeps the text from drawing: the line to blame (zero-based),
    /// if one is, and what is wrong, in one line.
    pub fn trouble(&self) -> Option<(Option<usize>, String)> {
        let (line, what) = match &self.status {
            Status::Failed(RenderError::Diagram {
                engine,
                line,
                message,
            }) => (*line, format!("{message} ({})", engine.name())),
            Status::Failed(RenderError::Unavailable { line, reason }) => (*line, reason.clone()),
            Status::Failed(err) => (None, err.to_string()),
            Status::Unreadable(err) => (None, format!("cannot read: {err}")),
            Status::Waiting | Status::Rendering | Status::InSync => return None,
        };
        Some((line, ui::one_line(&what)))
    }

    /// Whether a picture of the diagram is on screen, the last good one
    /// included.
    pub fn has_picture(&self) -> bool {
        self.overview.shown.is_some()
    }

    /// The zoom in percent.
    pub fn zoom_percent(&self) -> f32 {
        self.view.zoom * 100.0
    }
}

/// The bar under a playing diagram: where the machine is and what can
/// happen next, or how far the scenario is; the way back, and out.
fn play_bar<'a>(offer: &Offer, p: &Palette, font: Font) -> Element<'a, Message> {
    let accent = ui::color(p.accent);
    let muted = ui::color(p.dark_foreground);
    let words = move |s: String, color: iced::Color| text(s).font(font).size(13).color(color);
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    // A line of its own under the bar's row: what a trace says is wrong.
    let mut note: Option<(Severity, String)> = None;
    match offer {
        Offer::Machine {
            options,
            hover,
            at,
            steps,
            ended,
            can_go_back,
        } => {
            let shown = if *ended {
                "ended".to_owned()
            } else {
                at.clone()
            };
            items.push(words(format!("● {shown}"), accent).into());
            for (i, (event, to)) in options.iter().enumerate() {
                let chip = chip(i, event, to, *hover == Some(i), p, font);
                items.push(
                    mouse_area(chip)
                        .on_enter(Message::Play(Ask::Hover(Some(i))))
                        .on_exit(Message::Play(Ask::Hover(None)))
                        .into(),
                );
            }
            if options.is_empty() && !*ended {
                items.push(words("nothing can happen here".into(), muted).into());
            }
            if *can_go_back {
                items.push(tool("↶ back", Some(Ask::Back), p, font));
            }
            if *steps > 0 || *ended {
                items.push(tool("⟲ again", Some(Ask::Restart), p, font));
            }
        }
        Offer::Scenario { at, of } => {
            items.push(words(format!("● {at} / {of}"), accent).into());
            items.push(tool("‹ back", (*at > 0).then_some(Ask::Back), p, font));
            items.push(chip_next((*at < *of).then_some(Ask::Next), p, font));
            items.push(tool("⟲ again", (*at > 0).then_some(Ask::Restart), p, font));
        }
        Offer::Trace {
            name,
            object,
            played,
            of,
            line,
            time,
            said,
            at,
            finding,
        } => {
            let (head, color) = if at.is_empty() {
                (format!("○ {object}: not followed here"), muted)
            } else {
                (format!("● {object}: {at}"), accent)
            };
            items.push(words(head, color).into());
            let place = match line {
                Some(line) => format!("{name}:{line} · {played} / {of}"),
                None => format!("{name} · {played} / {of}"),
            };
            items.push(words(place, muted).into());
            if !said.is_empty() {
                let said = match time {
                    Some(time) => format!("{time}  {said}"),
                    None => said.clone(),
                };
                items.push(words(said, ui::color(p.bright_foreground)).into());
            }
            items.push(tool("‹ back", (*played > 0).then_some(Ask::Back), p, font));
            items.push(chip_next((*played < *of).then_some(Ask::Next), p, font));
            items.push(tool(
                "⟲ again",
                (*played > 0).then_some(Ask::Restart),
                p,
                font,
            ));
            note.clone_from(finding);
        }
    }
    let accent_rgb = p.accent;
    let close = button(words("✕".into(), muted))
        .on_press(Message::StopPlay)
        .padding([3, 8])
        .style(move |_: &Theme, status| quiet(status, accent_rgb));
    items.push(close.into());
    let fill = ui::color(p.dark_background);
    let wrong = note
        .as_ref()
        .is_some_and(|(severity, _)| *severity == Severity::Error);
    let edge = if wrong {
        iced::Color {
            a: 0.8,
            ..ui::color(p.red)
        }
    } else {
        iced::Color { a: 0.45, ..accent }
    };
    let items = row(items)
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .wrap();
    let content: Element<'a, Message> = match note {
        Some((severity, message)) => {
            let color = if severity == Severity::Error {
                ui::color(p.red)
            } else {
                muted
            };
            column![items, words(message, color)].spacing(6).into()
        }
        None => items.into(),
    };
    let bar = container(content)
        .padding([8, 12])
        .max_width(900)
        .style(move |_: &Theme| {
            container::Style::default()
                .background(iced::Color { a: 0.94, ..fill })
                .border(iced::Border {
                    color: edge,
                    width: 1.0,
                    radius: 14.0.into(),
                })
                .shadow(iced::Shadow {
                    color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.35),
                    offset: Vector::new(0.0, 4.0),
                    blur_radius: 18.0,
                })
        });
    container(bar)
        .width(Length::Fill)
        .align_x(iced::Alignment::Center)
        .padding([10, 18])
        .into()
}

/// An option of a playing machine: its number, its event, where it leads.
fn chip<'a>(
    index: usize,
    event: &str,
    to: &str,
    hot: bool,
    p: &Palette,
    font: Font,
) -> Element<'a, Message> {
    let accent = ui::color(p.accent);
    let on_accent = ui::color(p.background);
    let number = container(
        text((index + 1).to_string())
            .font(font)
            .size(11)
            .color(on_accent),
    )
    .padding([0, 6])
    .style(move |_: &Theme| {
        container::Style::default()
            .background(accent)
            .border(iced::Border {
                radius: 999.0.into(),
                ..iced::Border::default()
            })
    });
    let content = row![
        number,
        text(event.to_owned())
            .font(font)
            .size(13)
            .color(ui::color(p.bright_foreground)),
        text(format!("→ {to}"))
            .font(font)
            .size(12)
            .color(ui::color(p.dark_foreground)),
    ]
    .spacing(7)
    .align_y(iced::Alignment::Center);
    let (lighter, accent_rgb) = (p.lighter_background, p.accent);
    button(content)
        .on_press(Message::Play(Ask::Take(index)))
        .padding([4, 10])
        .style(move |_: &Theme, status| {
            let hovered =
                hot || matches!(status, button::Status::Hovered | button::Status::Pressed);
            let base = ui::color(lighter);
            let glow = ui::color(accent_rgb);
            button::Style {
                background: Some(
                    if hovered {
                        iced::Color { a: 0.22, ..glow }
                    } else {
                        base
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: if hovered {
                        glow
                    } else {
                        iced::Color { a: 0.0, ..glow }
                    },
                    width: 1.0,
                    radius: 999.0.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

/// The scenario's step forward, the bar's main button.
fn chip_next<'a>(ask: Option<Ask>, p: &Palette, font: Font) -> Element<'a, Message> {
    let accent = p.accent;
    let on_accent = ui::color(p.background);
    let muted = ui::color(p.dark_foreground);
    let enabled = ask.is_some();
    button(
        text("next ›")
            .font(font)
            .size(13)
            .color(if enabled { on_accent } else { muted }),
    )
    .on_press_maybe(ask.map(Message::Play))
    .padding([4, 14])
    .style(move |_: &Theme, status| {
        let glow = ui::color(accent);
        let strong = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: enabled.then(|| {
                if strong {
                    glow.into()
                } else {
                    iced::Color { a: 0.85, ..glow }.into()
                }
            }),
            border: iced::Border {
                color: if enabled { glow } else { muted },
                width: 1.0,
                radius: 999.0.into(),
            },
            ..button::Style::default()
        }
    })
    .into()
}

/// A small text button of the bar; greyed out without `ask`.
fn tool<'a>(label: &str, ask: Option<Ask>, p: &Palette, font: Font) -> Element<'a, Message> {
    let color = if ask.is_some() {
        ui::color(p.foreground)
    } else {
        iced::Color {
            a: 0.4,
            ..ui::color(p.dark_foreground)
        }
    };
    let accent = p.accent;
    button(text(label.to_owned()).font(font).size(13).color(color))
        .on_press_maybe(ask.map(Message::Play))
        .padding([3, 8])
        .style(move |_: &Theme, status| quiet(status, accent))
        .into()
}

/// A button that shows only when the pointer is on it.
fn quiet(status: button::Status, accent: omaspec_theme::Rgb) -> button::Style {
    let glow = ui::color(accent);
    button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then(|| iced::Color { a: 0.16, ..glow }.into()),
        border: iced::Border {
            radius: 999.0.into(),
            ..iced::Border::default()
        },
        ..button::Style::default()
    }
}

#[cfg(test)]
pub mod testing {
    //! Helpers for window tests: diagrams on screen without a render thread.

    use omaspec_render::{Config, Engine, Renderer};

    use super::*;

    pub fn drawn(source: &str, language: Language) -> Drawn {
        let mut renderer = Renderer::new(&Config {
            java_fallback: false,
            ..Config::default()
        });
        let out = renderer.render(source, language).unwrap();
        Drawn {
            tree: Arc::new(export::parse(&out.svg).unwrap()),
            svg: out.svg.into(),
            engine: out.engine,
            elapsed: out.elapsed,
            map: Arc::new(out.map),
        }
    }

    /// A box of 3000 × 12000 units, larger than the overview holds sharp.
    pub fn tall() -> Drawn {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="3000" height="12000"><rect x="10" y="10" width="2980" height="11980" fill="none" stroke="black"/></svg>"#;
        Drawn {
            tree: Arc::new(export::parse(svg).unwrap()),
            svg: svg.into(),
            engine: Engine::Mermaid,
            elapsed: Duration::ZERO,
            map: Arc::default(),
        }
    }

    pub fn answer(doc: u64, id: u64, result: Result<Drawn, RenderError>) -> Answer {
        Answer {
            ticket: Ticket { doc, id },
            source: "".into(),
            language: Language::Mermaid,
            result,
        }
    }

    impl Paper {
        /// Pretends `id` requests went out.
        pub fn pretend_requested(&mut self, id: u64) {
            self.requested = id;
        }

        /// Draws the play `after` from now on, whenever it is drawn.
        pub fn freeze_play(&mut self, after: Duration) {
            self.clock = Some(Instant::now() + after);
        }

        /// Draws the play at `at`, whenever it is drawn.
        pub fn freeze_play_at(&mut self, at: Instant) {
            self.clock = Some(at);
        }

        /// When the lights of the play's steps so far have all arrived.
        pub fn play_arrived(&self) -> Option<Instant> {
            self.play.as_ref()?.arrived()
        }

        /// Pretends one more request went out; its number.
        pub fn pretend_next(&mut self) -> u64 {
            self.requested += 1;
            self.requested
        }

        pub fn viewport(&self) -> Size {
            self.viewport
        }

        /// What the detail layer asked for, and whether one is shown.
        pub fn detail(&self) -> (Option<Plan>, bool) {
            (self.detail.asked, self.detail.shown.is_some())
        }

        pub fn overview_shown(&self) -> bool {
            self.overview.shown.is_some()
        }

        /// Rasterizes the overview the view asked for and hands it in as
        /// the background task's message would.
        pub fn deliver_overview(&mut self) {
            let tree = Arc::clone(&self.diagram.as_ref().unwrap().drawn.tree);
            let raster = raster::render(&tree, self.overview.asked.unwrap());
            let _ = self.update(Message::Overview(self.overview.seq(), raster));
        }

        /// How many pictures of the diagram before fade out, if a fade runs.
        pub fn fading(&self) -> Option<usize> {
            self.fade.as_ref().map(|fade| fade.old.len())
        }

        /// Rasterizes what the view asked for, as the background tasks would.
        pub fn finish_rasters(&mut self) {
            let tree = Arc::clone(&self.diagram.as_ref().unwrap().drawn.tree);
            self.overview.finish(&tree);
            self.detail.finish(&tree);
        }

        /// Where the first label reading `text` is in the pane.
        pub fn screen_of(&self, text: &str) -> Point {
            fn find(group: &export::usvg::Group, text: &str) -> Option<export::usvg::Rect> {
                group.children().iter().find_map(|node| match node {
                    export::usvg::Node::Group(g) => find(g, text),
                    export::usvg::Node::Text(t) => {
                        let content: String = t.chunks().iter().map(|c| c.text()).collect();
                        (content.trim() == text).then(|| t.abs_bounding_box())
                    }
                    _ => None,
                })
            }
            let tree = &self.diagram.as_ref().unwrap().drawn.tree;
            let rect = find(tree.root(), text).expect(text);
            let middle = Point::new(
                rect.left() + rect.width() / 2.0,
                rect.top() + rect.height() / 2.0,
            );
            let pane = iced::Rectangle::with_size(self.viewport);
            let on_screen = self
                .view
                .on_screen(iced::Rectangle::new(middle, Size::ZERO), pane);
            on_screen.position()
        }

        /// Where the line `key` draws runs, half way along its longest run,
        /// in the pane; and the way down the pane, across that run.
        pub fn screen_of_line(&self, key: &str) -> (Point, Vector) {
            let map = &self.diagram.as_ref().unwrap().drawn.map;
            let wire = map
                .wires
                .iter()
                .find(|w| w.key.to_string() == key)
                .unwrap_or_else(|| panic!("no line {key}"));
            let (a, b) = wire
                .points
                .windows(2)
                .map(|w| (w[0], w[1]))
                .max_by(|x, y| {
                    let len = |(a, b): ((f32, f32), (f32, f32))| (b.0 - a.0).hypot(b.1 - a.1);
                    len(*x).total_cmp(&len(*y))
                })
                .unwrap();
            let middle = Point::new((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            let pane = iced::Rectangle::with_size(self.viewport);
            let at = self
                .view
                .on_screen(iced::Rectangle::new(middle, Size::ZERO), pane)
                .position();
            let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-3);
            (at, Vector::new(-(b.1 - a.1) / len, (b.0 - a.0) / len))
        }

        /// The line the hand shapes now, in the drawing's units.
        pub fn hand_shape(&self) -> Option<Vec<(f32, f32)>> {
            let hand = self.hand.as_ref()?;
            let map = &self.diagram.as_ref()?.drawn.map;
            let wire = map.wires.get(hand.wire)?;
            Some(
                wire.shaped(&hand.corners, map.fit)
                    .into_iter()
                    .map(|p| map.fit.back(p))
                    .collect(),
            )
        }

        /// The runs of line `key` as laid, in the drawing's units.
        pub fn path_of_line(&self, key: &str) -> Vec<(f32, f32)> {
            let map = &self.diagram.as_ref().unwrap().drawn.map;
            let wire = map.wires.iter().find(|w| w.key.to_string() == key).unwrap();
            wire.path.iter().map(|&p| map.fit.back(p)).collect()
        }

        /// A raster of what the detail asks for now.
        pub fn early_detail(&self) -> Option<Raster> {
            let tree = &self.diagram.as_ref().unwrap().drawn.tree;
            raster::render(tree, self.detail.asked?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{answer, drawn};
    use super::*;

    #[test]
    fn a_new_diagram_fades_in_over_the_one_before() {
        let mut paper = Paper::default();
        let _ = paper.update(Message::Viewport(Size::new(400.0, 300.0)));
        paper.pretend_requested(2);
        let first = drawn("flowchart LR\n  A --> B\n", Language::Mermaid);
        let _ = paper.rendered(answer(1, 1, Ok(first)));
        paper.deliver_overview();
        // The first picture comes in from white paper.
        assert_eq!(paper.fading(), Some(0));
        let _ = paper.update(Message::Faded);
        assert_eq!(paper.fading(), None);

        let second = drawn("flowchart LR\n  A --> C\n", Language::Mermaid);
        let _ = paper.rendered(answer(1, 2, Ok(second)));
        // The old picture stays as it is until the new one is rasterized.
        assert_eq!(paper.fading(), None);
        paper.deliver_overview();
        assert_eq!(paper.fading(), Some(1));

        // Zooming rasterizes again without a fade.
        let _ = paper.update(Message::Faded);
        let _ = paper.zoom(2.0);
        paper.deliver_overview();
        assert_eq!(paper.fading(), None);
    }

    #[test]
    fn the_caret_aims_at_that_occurrence() {
        let source = "%% Edit\nflowchart LR\n  A[Edit] --> B[Edit]\n";
        let mut paper = Paper::default();
        show_source(&mut paper, source, Language::Mermaid);
        let comment = source.lines().next().unwrap();
        assert!(
            paper
                .place_of(source, 0, comment.find("Edit").unwrap())
                .is_none()
        );
        let line = source.lines().nth(2).unwrap();
        let first = paper
            .place_of(source, 2, line.find("Edit").unwrap())
            .unwrap();
        let second = paper
            .place_of(source, 2, line.rfind("Edit").unwrap())
            .unwrap();
        // Two names on one line light the two boxes, left then right.
        assert!(
            first.x < second.x,
            "left Edit then right Edit: {first:?} vs {second:?}"
        );
        assert!(paper.place_of(source, 2, 0).is_none());

        let source = "flowchart LR\n  A[Edit] --> B\n  C[Edit] --> D\n";
        show_source(&mut paper, source, Language::Mermaid);
        assert!(paper.place_of(source, 0, 0).is_none());
        let earlier = paper
            .place_of(
                source,
                1,
                source.lines().nth(1).unwrap().find("Edit").unwrap(),
            )
            .unwrap();
        let later = paper
            .place_of(
                source,
                2,
                source.lines().nth(2).unwrap().find("Edit").unwrap(),
            )
            .unwrap();
        let center = |rect: Rectangle| (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        let (ax, ay) = center(earlier);
        let (cx, cy) = center(later);
        assert!(
            cx > ax + 1.0 || cy > ay + 1.0,
            "the later Edit is another box: {earlier:?} vs {later:?}"
        );
    }

    #[test]
    fn a_phrase_lights_the_shape_that_draws_it() {
        let mut paper = Paper::default();
        let at = |source: &str, row: usize, word: &str| {
            let line = source.split('\n').nth(row).unwrap();
            line.find(word).unwrap()
        };
        let same = |paper: &Paper, source: &str, row: usize, a: &str, b: &str| {
            let left = paper.place_of(source, row, at(source, row, a)).unwrap();
            let right = paper.place_of(source, row, at(source, row, b)).unwrap();
            assert_eq!(left, right, "{a:?} and {b:?} on row {row}");
            left
        };

        let sequence = "\
@startuml
title Logging in
participant \"Web app\" as Web
actor User
User -> Web : open the login page
alt password ok
else wrong password
end
@enduml
";
        show_source(&mut paper, sequence, Language::PlantUml);
        let web = same(&paper, sequence, 2, "Web", "app");
        let alias = paper.place_of(
            sequence,
            2,
            sequence.split('\n').nth(2).unwrap().rfind("Web").unwrap(),
        );
        assert_eq!(alias, Some(web));
        let user = paper
            .place_of(sequence, 3, at(sequence, 3, "User"))
            .unwrap();
        assert_ne!((web.x, web.y), (user.x, user.y));
        let message = same(&paper, sequence, 4, "open", "page");
        assert_ne!((message.x, message.y), (user.x, user.y));
        // The alias on the message still names the participant.
        let mention = paper.place_of(sequence, 4, at(sequence, 4, "Web")).unwrap();
        assert!(mention.y >= web.y);
        let title = same(&paper, sequence, 1, "Logging", "in");
        assert_ne!((title.x, title.y), (web.x, web.y));
        let ok = same(&paper, sequence, 5, "password", "ok");
        let wrong = same(&paper, sequence, 6, "wrong", "password");
        assert_ne!((ok.x, ok.y), (wrong.x, wrong.y));

        let state = "\
@startuml
state \"Request (idle)\" as Request
state Pick <<choice>>
Request --> Pick : next track
@enduml
";
        show_source(&mut paper, state, Language::PlantUml);
        let request = same(&paper, state, 1, "Request", "idle");
        let alias = paper.place_of(
            state,
            1,
            state.split('\n').nth(1).unwrap().rfind("Request").unwrap(),
        );
        assert_eq!(alias, Some(request));
        let again = paper.place_of(state, 3, at(state, 3, "Request")).unwrap();
        assert_eq!(again, request);
        let pick = paper.place_of(state, 2, at(state, 2, "Pick")).unwrap();
        let choice = paper.place_of(state, 2, at(state, 2, "choice")).unwrap();
        assert_eq!(pick, choice);
        assert_ne!((pick.x, pick.y), (request.x, request.y));
        let track = same(&paper, state, 3, "next", "track");
        assert_ne!((track.x, track.y), (request.x, request.y));

        let activity = "\
@startuml
start
:Merge the last change;
if (all green?) then (yes)
  :Publish packages;
else (no)
  :Fix and tag again;
endif
stop
@enduml
";
        show_source(&mut paper, activity, Language::PlantUml);
        let merge = same(&paper, activity, 2, "Merge", "change");
        let guard = same(&paper, activity, 3, "green", "all");
        let yes = paper
            .place_of(
                activity,
                3,
                activity.split('\n').nth(3).unwrap().rfind("yes").unwrap(),
            )
            .unwrap();
        assert_ne!((guard.x, guard.y), (yes.x, yes.y));
        let publish = same(&paper, activity, 4, "Publish", "packages");
        assert_ne!((publish.x, publish.y), (merge.x, merge.y));

        let structure = "\
@startuml
interface Sellable
component Billing <<service>>
@enduml
";
        show_source(&mut paper, structure, Language::PlantUml);
        assert!(
            paper
                .place_of(structure, 1, at(structure, 1, "interface"))
                .is_some()
        );
        assert!(
            paper
                .place_of(structure, 1, at(structure, 1, "Sellable"))
                .is_some()
        );
        assert!(
            paper
                .place_of(structure, 2, at(structure, 2, "Billing"))
                .is_some()
        );
        assert!(
            paper
                .place_of(structure, 2, at(structure, 2, "service"))
                .is_some()
        );

        let timing = "\
@startuml
robust \"Web Browser\" as WB
WB has Idle, Processing
@0
WB is Idle
@100
WB is Processing
@enduml
";
        show_source(&mut paper, timing, Language::PlantUml);
        let browser = same(&paper, timing, 1, "Web", "Browser");
        let short = paper
            .place_of(
                timing,
                1,
                timing.split('\n').nth(1).unwrap().rfind("WB").unwrap(),
            )
            .unwrap();
        assert_eq!(short, browser);
        let idle = paper.place_of(timing, 2, at(timing, 2, "Idle")).unwrap();
        let again = paper.place_of(timing, 4, at(timing, 4, "Idle")).unwrap();
        assert_eq!(idle, again);
        assert_ne!((idle.x, idle.y), (browser.x, browser.y));
        let processing = paper
            .place_of(timing, 2, at(timing, 2, "Processing"))
            .unwrap();
        assert_ne!((processing.x, processing.y), (idle.x, idle.y));
        let later = paper.place_of(timing, 2, at(timing, 2, "WB")).unwrap();
        assert_eq!(later, browser);

        let flow = "\
@startflow
title Charge control
block Filter \"Low pass\" {
  in voltage
  out limited
}
@endflow
";
        show_source(&mut paper, flow, Language::PlantUml);
        let title = same(&paper, flow, 1, "Charge", "control");
        let block = same(&paper, flow, 2, "Low", "pass");
        // The block's own name is not the quoted label.
        assert!(paper.place_of(flow, 2, at(flow, 2, "Filter")).is_none());
        assert_ne!((title.x, title.y), (block.x, block.y));
        assert!(paper.place_of(flow, 3, at(flow, 3, "voltage")).is_some());
    }

    #[test]
    fn a_breath_fades_in_and_glides() {
        let mut paper = Paper::default();
        let here = Rectangle::new(Point::new(10.0, 20.0), Size::new(30.0, 40.0));
        let there = Rectangle::new(Point::new(80.0, 20.0), Size::new(30.0, 40.0));
        paper.set_breath(Some(here));
        assert_eq!(paper.breath_target(), Some(here));
        let started = paper.breath.unwrap().start;
        paper.set_breath(Some(here));
        assert_eq!(paper.breath.unwrap().start, started);

        // Still invisible, so a new place fades in where it is going.
        paper.set_breath(Some(there));
        let breath = paper.breath.unwrap();
        assert_eq!(breath.from, there);
        assert_eq!(breath.to, there);
        assert_eq!(breath.from_a, 0.0);

        let now = Instant::now();
        paper.breath.as_mut().unwrap().start = now - std::time::Duration::from_millis(80);
        let (_, alpha) = paper.breath.unwrap().at(now);
        // Halfway, eased: 1 - (1 - 0.5)³.
        assert!((alpha - 0.875).abs() < 0.02, "{alpha}");

        paper.breath.as_mut().unwrap().start =
            now - (super::view::GLIDE + std::time::Duration::from_millis(20));
        paper.set_breath(Some(here));
        let breath = paper.breath.unwrap();
        assert!((breath.from.x - there.x).abs() < 0.5, "{:?}", breath.from);
        assert_eq!(breath.to, here);
        assert!(breath.from_a > 0.9, "{}", breath.from_a);

        paper.set_breath(None);
        assert!(paper.breath_target().is_none());
        let leaving = paper.breath.unwrap();
        assert_eq!(leaving.to_a, 0.0);
        assert!(paper.breath_moving(Instant::now()));
        paper.set_breath(None);
        assert_eq!(paper.breath.unwrap().start, leaving.start);

        paper.breath.as_mut().unwrap().start =
            Instant::now() - (super::view::GLIDE + std::time::Duration::from_millis(20));
        paper.set_breath(None);
        assert!(paper.breath.is_none());
    }

    /// Puts `source` on the sheet, as a render would.
    fn show_source(paper: &mut Paper, source: &str, language: Language) {
        let mut reply = answer(1, paper.pretend_next(), Ok(drawn(source, language)));
        reply.source = source.into();
        reply.language = language;
        let _ = paper.rendered(reply);
    }
}
