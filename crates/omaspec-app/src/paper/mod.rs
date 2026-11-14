//! The diagram pane of the preview and the editor window: the rendered
//! diagram on white paper, zoom and pan, sharp at any zoom, and a frame
//! whose color tells whether the picture is up to date.
//!
//! The last good picture stays while a newer source fails to render. A
//! new picture fades in over the one before. When the same boxes stand
//! somewhere else, they glide there and the lines fade out and come back
//! on the new routes ([`morph`]). A glide already running finishes first.
//! A state machine or a scenario
//! can be played in it ([`play`]). The name under the caret lights on the
//! sheet. The light shrinks to a point, travels, and spreads over the next
//! place: a box takes it over its shape, and words take a quiet wash
//! ([`breath`], [`wash`]). With Alt, a line
//! can be drawn by hand ([`hand`]), and a box held in place
//! ([`held`]). A container folds to its box ([`fold`]), and the focus on
//! one box veils all but it and its neighbors ([`focus`]).
//!
//! A new picture comes in through [`arrive`]; zoom, pan and fit are
//! [`zoom`]; the way into a link and back out is [`passage`]; what the pane
//! draws, the play's bar with it, is [`pane`]; copy and export are
//! [`copy`].

mod arrive;
mod bar;
mod breath;
mod canvas;
mod cards;
mod copy;
mod find;
mod focus;
mod fold;
pub(crate) mod frames;
mod hand;
mod held;
mod light;
mod morph;
mod pane;
mod passage;
mod play;
mod raster;
pub(crate) mod view;
mod wash;
mod wires;
mod zoom;

use std::sync::Arc;
use std::time::Duration;

use iced::time::Instant;
use iced::{Animation, Point, Rectangle, Size, Task, Vector};
use omaspec_core::Language;
use omaspec_model::Spec;
use omaspec_render::Place;
use omaspec_render::export::usvg;
use omaspec_render::{Link, RenderError};
use omaspec_theme::{Palette, Rgb};

use crate::ui;
use crate::worker::{Drawn, Linked, Ticket, Worker};
pub use hand::Gesture;
use passage::Passage;
pub use passage::Showing;
pub(crate) use play::option_of;
pub use play::{Ask, Offer, Play};
use raster::{Layer, Raster};
pub use view::{Glide, View};

/// Wait this long after the last zoom or pan step before rasterizing sharp
/// again.
const RASTER_DELAY: Duration = Duration::from_millis(120);

#[derive(Debug, Clone)]
pub enum Message {
    Overview(u64, Option<Raster>),
    Detail(u64, Option<Raster>),
    /// The old picture and the new one without their lines, for a glide
    /// that draws the lines itself ([`wires`]).
    BareOld(u64, Option<Raster>),
    BareNew(u64, Option<Raster>),
    /// A smaller copy of the picture with this id, which a zoom into a
    /// tile or out of it shows, and its place among the copies
    /// ([`raster::halves`]).
    Half(iced::advanced::image::Id, usize, Option<Raster>),
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
    /// A click on a link at this point of the pane; the window asks
    /// [`Paper::link_at`] where it leads.
    Follow(Point),
    /// A new picture has faded in; the one before can go.
    Faded,
    /// The writer's step in the play.
    Play(Ask),
    /// A map's tiles with their pictures, parsed for the picture with this
    /// id ([`Diagram::pictures`]).
    Pictures(u64, Option<Arc<usvg::Tree>>),
    /// The play fades away.
    StopPlay,
    /// The play has faded away.
    PlayGone,
    /// What the pointer does with Alt: a line drawn by hand. Its new bends
    /// wait for the window in [`Paper::take_by_hand`].
    Hand(Gesture),
    /// A click on a container's fold button: it folds to its box.
    Fold(String),
    /// A click on a folded box: it unfolds.
    Unfold(String),
    /// Shift and a click at this point of the pane: the box there comes
    /// into focus, or the focus ends.
    FocusAt(Point),
    /// The right mouse button at this point of the pane, which is at the
    /// second point of its window: the window opens its context menu
    /// there (docs/adr/0025).
    Context(Point, Point),
}

/// What a new picture is. A fold eases longer, and a named box stays put
/// when the view is zoomed. Only renders asked for after the fold, after
/// request `since`, draw it: the anchor is spent on the first of them,
/// and the kind stays until the fade of one of them starts.
enum Change {
    Edit,
    Fold { anchor: Option<String>, since: u64 },
}

/// The pictures on screen when a new diagram arrived, and where they sat.
struct Previous {
    /// Where they sat while the new diagram's view was `then`. They move
    /// as that view does ([`Self::view_at`]).
    view: View,
    then: View,
    size: Size,
    pictures: Vec<Raster>,
    /// The named boxes of the picture still on screen.
    boxes: Vec<morph::Side>,
    /// Its links and folds, whose marks stay on it.
    links: Vec<Link>,
    spots: Vec<fold::Spot>,
    /// Its picture without lines and the plan its overview was drawn to,
    /// and its map: the lines glide from these ([`wires`]).
    bare: Option<Arc<usvg::Tree>>,
    plan: Option<raster::Plan>,
    map: Arc<omaspec_render::Map>,
}

impl Previous {
    /// Where the old pictures sit while the new diagram is seen at `now`:
    /// a pan, a zoom, a glide or a new pane size moves them as it moves
    /// the new diagram, so they never stand still under the pointer.
    fn view_at(&self, now: View) -> View {
        let ratio = self.then.zoom / self.view.zoom.max(f32::EPSILON);
        View {
            zoom: now.zoom / ratio.max(f32::EPSILON),
            center: self.view.center + (now.center - self.then.center) * ratio,
            fit: self.view.fit,
        }
    }
}

/// The pictures of the diagram before, fading out under the new one.
pub struct Fade {
    old: Vec<Raster>,
    /// Goes to `true` as the new picture comes in.
    shown: Animation<bool>,
    /// Where `old` was on screen, and how big that diagram was.
    old_view: View,
    old_size: Size,
    /// The boxes gliding across, when the new picture moved them.
    morph: Option<morph::Morph>,
    /// The picture this glide travels to, taken when the glide starts.
    /// A later render must not swap it under the boxes.
    next: Option<Raster>,
    /// Where that picture sits. A pan or a zoom follows; the fit of a
    /// newer diagram waits until this glide has ended.
    new_view: View,
    new_size: Size,
    /// The lines gliding along, over the old and the new picture without
    /// their lines ([`wires`]); none when the pictures keep their lines.
    lines: Option<wires::Glide>,
    bare: Option<(Raster, Raster)>,
    /// The fade of a fold in a diagram of too many boxes to glide
    /// ([`arrive::MOST_GLIDING`]): it stands in for the glide.
    stands_in: bool,
}

/// The diagram on screen.
pub struct Diagram {
    pub drawn: Drawn,
    pub size: Size,
    pub source: Arc<str>,
    pub language: Language,
    /// A large map shown by its cards: when its pictures show, and them.
    pictures: cards::Pictures,
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
    /// A zoom into a link, or out of it, with the diagram it leads to.
    passage: Option<Passage>,
    viewport: Size,
    scale_factor: f32,
    overview: Layer,
    detail: Layer,
    /// The old and the new picture without their lines, for a glide
    /// ([`wires`]); `no_bare` once one of them could not be drawn.
    bare_old: Layer,
    bare_new: Layer,
    no_bare: bool,
    /// The picture is made from a project, not drawn from the text in the
    /// tab (a map): an export of it carries no source.
    made: bool,
    /// A new diagram came; its first overview raster starts the fade.
    fresh: bool,
    /// The diagram to come back to once the one that just came is shown:
    /// the way from an older drawing of it, shown as a glide
    /// ([`Paper::glide_from`]).
    back: Option<crate::worker::Answer>,
    /// That overview is in. A glide still on screen finishes before the
    /// next one starts from it.
    landed: bool,
    /// Whether the layers' plans are of a map with its pictures
    /// ([`Diagram::pictures`]); a change rasterizes them again.
    on_pictures: bool,
    /// The pictures on screen when the new diagram came, until they fade.
    previous: Option<Previous>,
    /// The fold waiting to fade. An ordinary edit is [`Change::Edit`].
    change: Change,
    fade: Option<Fade>,
    /// A state machine or scenario being played.
    play: Option<Play>,
    /// When a stopped play began to fade away.
    leaving: Option<Instant>,
    /// The moment a play is drawn at, for pictures of one moment; `None`
    /// draws each frame at its own moment.
    clock: Option<Instant>,
    /// The name under the caret, gliding to its place on the sheet.
    breath: Option<breath::Breath>,
    /// The last caret lookup. A highlight sweep that does not move the
    /// caret must not search the sheet again.
    aim: Option<AimMemo>,
    /// The light is on a label a find chose ([`Self::light_label`]): it
    /// stays there until the caret moves or another drawing comes.
    found: bool,
    /// Whether lines can be drawn by hand here: in the editor, not in the
    /// preview window, which follows a file another editor writes.
    pub by_hand: bool,
    /// Whether the picture is a diagram of a Markdown document, drawn from
    /// the document with every other line empty: no lines by hand, and an
    /// image stores the diagram without those lines.
    pub document: bool,
    /// The line drawn by hand, or lit with Alt.
    hand: Option<hand::Hand>,
    /// A line drawn by hand, with the bends to write into the source.
    by_hand_done: Option<omaspec_uml::hand::Line>,
    /// The latest request when the hand's bends were sent: a picture asked
    /// for after it has them.
    hand_sent_at: u64,
    /// The box lit with Alt, or held by the pointer (docs/adr/0021).
    grab: Option<held::Grab>,
    /// A box dropped, with the entry to write into the source.
    grab_done: Option<held::Drop>,
    /// Pixels that came while the view glided, to replace those on screen
    /// once it stops: shown at once, the frame that first draws them comes
    /// late (`OMASPEC_FRAMES` showed a 16-megapixel overview cost one).
    held_overview: Option<(u64, Option<Raster>)>,
    held_detail: Option<(u64, Option<Raster>)>,
    /// Whether containers fold here and a box takes the focus: in the
    /// editor and the preview window, not in Compare.
    pub folding: bool,
    /// The containers folded to their box, by name (docs/adr/0024).
    folds: Vec<String>,
    /// How the diagram is asked drawn besides its file (View menu).
    pub views: crate::worker::Views,
    /// Recorded traces whose coverage is drawn over the state machine.
    pub covered: Option<std::sync::Arc<crate::worker::Covered>>,
    /// The folds changed: the owner draws the diagram again.
    refold: bool,
    /// The container just folded or unfolded: the view glides to it once
    /// it is drawn.
    unfolding: Option<String>,
    /// The box in focus, by name.
    focus: Option<String>,
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
            passage: None,
            viewport: Size::ZERO,
            scale_factor: 1.0,
            overview: Layer::default(),
            detail: Layer::default(),
            bare_old: Layer::default(),
            bare_new: Layer::default(),
            no_bare: false,
            made: false,
            fresh: false,
            back: None,
            landed: false,
            on_pictures: false,
            previous: None,
            change: Change::Edit,
            fade: None,
            play: None,
            leaving: None,
            clock: None,
            breath: None,
            aim: None,
            found: false,
            by_hand: false,
            document: false,
            hand: None,
            by_hand_done: None,
            hand_sent_at: 0,
            grab: None,
            grab_done: None,
            held_overview: None,
            held_detail: None,
            folding: false,
            folds: Vec::new(),
            views: crate::worker::Views::default(),
            covered: None,
            refold: false,
            unfolding: None,
            focus: None,
        }
    }
}

/// Inputs of the last caret lookup, and what it returned.
struct AimMemo {
    source: u64,
    shown: u64,
    row: u32,
    col: u32,
    place: Option<breath::Aim>,
}

/// The smallest of `links` around `p` (diagram units): a state inside a
/// tile's picture is not a link of the map.
/// A play lights the states and lines of a diagram: a state machine shown
/// as its table has none.
fn tabled(diagram: &Diagram) -> Result<(), String> {
    if diagram.drawn.map.cells.is_empty() {
        Ok(())
    } else {
        Err("a play runs in the diagram: View → State machines: as a diagram or a table".into())
    }
}

fn link_under(links: &[Link], p: Point) -> Option<&Link> {
    links
        .iter()
        .filter(|l| link_rect(l).contains(p))
        .min_by(|a, b| {
            let area = |l: &Link| l.rect.w * l.rect.h;
            area(a).total_cmp(&area(b))
        })
}

/// Where a link is, in diagram units.
pub fn link_rect(link: &Link) -> Rectangle {
    Rectangle::new(
        Point::new(link.rect.x, link.rect.y),
        Size::new(link.rect.w, link.rect.h),
    )
}

/// Where the diagram a link opens grows out of, in diagram units: the
/// room of a tile's picture, which shows that diagram already; a state's
/// whole box.
pub fn link_room(link: &Link) -> Rectangle {
    link.room.map_or_else(
        || link_rect(link),
        |room| Rectangle::new(Point::new(room.x, room.y), Size::new(room.w, room.h)),
    )
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
        self.request_linked(worker, doc, source, language, place, None);
    }

    /// [`Self::request`] for a diagram that links to the diagrams of its
    /// project (`docs/adr/0018-diagrams-that-link.md`).
    pub fn request_linked(
        &mut self,
        worker: &Worker,
        doc: u64,
        source: Arc<str>,
        language: Language,
        place: Option<Place>,
        linked: Option<Linked>,
    ) {
        self.requested += 1;
        let ticket = Ticket {
            doc,
            id: self.requested,
        };
        let folds = if self.folding {
            self.folds.clone()
        } else {
            Vec::new()
        };
        let asked = crate::worker::Asked {
            folds,
            views: self.views,
            covered: self.covered.clone(),
        };
        worker.render_folded(ticket, source, language, place, linked, asked);
        self.made = false;
        self.asked();
    }

    /// A new drawing was asked for: it is what comes next, not a glide
    /// back to the one before ([`Self::glide_from`]).
    fn asked(&mut self) {
        self.status = Status::Rendering;
        self.back = None;
    }

    /// Asks `worker` for the map of `folder` of the project in `atlas`,
    /// made from its diagrams, in the look of `skin` (docs/adr/0026).
    pub fn request_map(
        &mut self,
        worker: &Worker,
        doc: u64,
        skin: Arc<str>,
        atlas: Arc<omaspec_render::Atlas>,
        folder: String,
    ) {
        self.requested += 1;
        let ticket = Ticket {
            doc,
            id: self.requested,
        };
        worker.render_map(ticket, skin, atlas, folder);
        self.made = true;
        self.asked();
    }

    /// Asks `worker` to render every diagram of the Markdown document
    /// `source`, one under another, as the preview window shows it.
    pub fn request_document(
        &mut self,
        worker: &Worker,
        doc: u64,
        source: Arc<str>,
        place: Option<Place>,
    ) {
        self.requested += 1;
        let ticket = Ticket {
            doc,
            id: self.requested,
        };
        worker.render_document(ticket, source, place, self.views);
        self.made = false;
        self.asked();
    }

    /// The id of the answer on screen; older ones are dropped.
    pub fn shown(&self) -> u64 {
        self.shown
    }

    /// The id of the latest [`Self::request`].
    pub fn requested(&self) -> u64 {
        self.requested
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // A picture on screen is replaced once the view stops gliding; a
            // missing one comes at once.
            Message::Overview(seq, raster) if self.holds() => {
                self.held_overview = Some((seq, raster));
                Task::none()
            }
            Message::Detail(seq, raster) if self.holds() => {
                self.held_detail = Some((seq, raster));
                Task::none()
            }
            Message::Overview(seq, raster) => self.show_overview(seq, raster),
            Message::Pictures(id, tree) => self.pictures_came(id, tree),
            Message::Detail(seq, raster) => {
                self.show_detail(seq, raster);
                Task::none()
            }
            Message::Half(of, at, copy) => {
                self.half_came(of, at, copy);
                Task::none()
            }
            Message::BareOld(seq, raster) => self.bare_came(seq, raster, true),
            Message::BareNew(seq, raster) => self.bare_came(seq, raster, false),
            Message::Faded => {
                self.fade = None;
                if self.fresh && self.landed {
                    return self.start_fade(crate::clock::now());
                }
                Task::none()
            }
            Message::Viewport(size) => {
                let now = crate::clock::now();
                let before = self.viewport;
                let resized = size != before;
                self.viewport = size;
                // A glide runs on: the panes share their widgets, so a tab
                // whose pane is sized otherwise (a play's bar) reports its
                // size on the frame after it is chosen, as the way back out
                // of a link starts. A fitted end moves with the pane.
                if let Some(glide) = self.glide.filter(|g| !g.done(now)) {
                    if resized && self.view.fit && self.diagram.is_some() {
                        let to = View::fitted(self.diagram_size(), size);
                        self.glide = Some(glide.toward(to, size, now));
                        self.view = to;
                    }
                    if self.inward() {
                        return Task::none();
                    }
                    return self.rasterize(RASTER_DELAY);
                }
                // Only the height changed, as when a play's bar comes, goes
                // or takes a second line: the picture stays where it is on
                // screen, held at the pane's top, and a fitted one glides to
                // its new fit.
                if resized
                    && before.width == size.width
                    && before.height > 0.0
                    && self.diagram.is_some()
                {
                    self.view.center.y -= (before.height - size.height) / (2.0 * self.view.zoom);
                    if self.view.fit {
                        let to = View::fitted(self.diagram_size(), size);
                        let middle = Point::new(size.width / 2.0, size.height / 2.0);
                        return self.glide_to(to, middle);
                    }
                    self.note_view();
                    return self.rasterize(RASTER_DELAY);
                }
                self.settle();
                if self.view.fit {
                    self.refit()
                } else {
                    self.note_view();
                    self.rasterize(RASTER_DELAY)
                }
            }
            Message::ZoomAt(factor, at) => {
                self.stop_waiting();
                self.settle();
                self.view = self.view.zoom_at(factor, at, self.viewport);
                self.note_view();
                self.rasterize(RASTER_DELAY)
            }
            Message::ZoomStep(factor, at) => {
                self.stop_waiting();
                let to = self.view.zoom_at(factor, at, self.viewport);
                self.glide_to(to, at)
            }
            Message::Glided => match self.glide {
                Some(glide) if glide.done(crate::clock::now()) => {
                    self.glide = None;
                    self.note_view();
                    let held = self.show_held();
                    // Out of a link, the diagram it leads to has gone; into
                    // one, the veil covers this diagram until its tab shows.
                    if self.inward() {
                        return held;
                    }
                    self.passage = None;
                    Task::batch([held, self.rasterize(Duration::ZERO)])
                }
                _ => Task::none(),
            },
            Message::Pan(delta) => {
                self.stop_waiting();
                self.settle();
                self.view = self.view.pan(delta);
                self.note_view();
                self.rasterize(RASTER_DELAY)
            }
            Message::Pinch { factor, at, pan } => {
                self.stop_waiting();
                self.settle();
                self.view = self.view.pinch(factor, at, pan, self.viewport);
                self.note_view();
                self.rasterize(RASTER_DELAY)
            }
            Message::Fit => self.fit(),
            Message::Pick(_) | Message::Follow(_) | Message::Context(..) => Task::none(),
            Message::Fold(name) => {
                self.set_fold(&name, true);
                Task::none()
            }
            Message::Unfold(name) => {
                self.set_fold(&name, false);
                Task::none()
            }
            Message::FocusAt(at) => {
                self.focus_at(at);
                Task::none()
            }
            Message::Play(ask) => {
                // On the clock the play is drawn at, so that a picture of
                // one moment takes its steps at that moment too.
                let now = self.clock.unwrap_or_else(crate::clock::now);
                if let Some(play) = self.play.as_mut().filter(|_| self.leaving.is_none()) {
                    play.ask(ask, now);
                }
                Task::none()
            }
            Message::StopPlay => {
                if self.play.is_some() && self.leaving.is_none() {
                    self.leaving = Some(crate::clock::now());
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
                    self.grab = None;
                    return Task::none();
                };
                let map = Arc::clone(&diagram.drawn.map);
                let (view, viewport) = (self.view, self.viewport);
                let to_diagram = |at: Point| {
                    let p = view.to_diagram(at, viewport);
                    (p.x, p.y)
                };
                // A box under no line takes the gesture, and keeps it
                // while it is dragged (docs/adr/0021).
                let reach = hand::reach(view.zoom);
                let on_box = |at: Point| {
                    let p = to_diagram(at);
                    hand::wire_at(&map, p, reach).is_none() && held::place_at(&map, p).is_some()
                };
                let to_box = match gesture {
                    Gesture::Hover(Some(at)) | Gesture::Press(at) | Gesture::Double(at) => {
                        self.hand
                            .as_ref()
                            .is_none_or(|h| h.drag.is_none() && !h.sent)
                            && on_box(at)
                    }
                    Gesture::Hover(None) => true,
                    Gesture::Move(_) | Gesture::Release | Gesture::Cancel => {
                        self.grab.as_ref().is_some_and(|g| g.drag.is_some())
                    }
                };
                if to_box {
                    if !matches!(gesture, Gesture::Hover(None)) {
                        self.hand = None;
                    }
                    if let Some(drop) = held::Grab::gesture(
                        &mut self.grab,
                        gesture,
                        &map,
                        to_diagram,
                        held::slop(view.zoom),
                    ) {
                        self.grab_done = Some(drop);
                        self.hand_sent_at = self.requested;
                    }
                    if !matches!(gesture, Gesture::Hover(None)) {
                        return Task::none();
                    }
                } else if self.grab.as_ref().is_some_and(|g| !g.sent) {
                    self.grab = None;
                }
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

    /// The box just dropped, with the entry to write into the source (or
    /// to take out); once.
    pub fn take_held(&mut self) -> Option<(String, Option<omaspec_uml::hand::Hold>)> {
        self.grab_done.take().map(|drop| (drop.name, drop.hold))
    }

    /// The entry just taken changed nothing in the source: the box is lit
    /// as it is drawn.
    pub fn held_unchanged(&mut self) {
        if let Some(grab) = &mut self.grab {
            grab.sent = false;
        }
    }

    /// The box held by the entry on `row` of the source.
    pub fn place_on(&self, row: usize) -> Option<&omaspec_uml::map::Place> {
        let map = &self.diagram.as_ref()?.drawn.map;
        map.places.iter().find(|p| p.entry == Some(row))
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

    /// The line of the source that the line of a table's cell at `at` says
    /// (a state machine shown as its table).
    pub fn cell_at(&self, at: Point) -> Option<usize> {
        let map = &self.diagram.as_ref()?.drawn.map;
        let p = self.view.to_diagram(at, self.viewport);
        map.cells
            .iter()
            .find(|cell| {
                let r = cell.rect;
                r.x <= p.x && p.x <= r.x + r.w && r.y <= p.y && p.y <= r.y + r.h
            })
            .map(|cell| cell.line)
    }

    /// The line that `row` of the source draws, or whose bends it writes.
    pub fn wire_on(&self, row: usize) -> Option<&omaspec_uml::map::Wire> {
        let map = &self.diagram.as_ref()?.drawn.map;
        map.wires
            .iter()
            .find(|w| w.line == Some(row) || w.entry == Some(row))
    }

    /// A pane where containers fold and a box takes the focus: the
    /// preview window's.
    pub fn foldable() -> Self {
        Self {
            folding: true,
            ..Self::default()
        }
    }

    /// A pane where lines can be drawn by hand and containers fold: the
    /// editor's.
    pub fn editable() -> Self {
        Self {
            by_hand: true,
            folding: true,
            ..Self::default()
        }
    }

    /// Whether a line can be drawn by hand now: here, with a picture of
    /// lines, and no play over it.
    fn drawable(&self) -> bool {
        self.by_hand
            && !self.document
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
        tabled(diagram)?;
        let play = Play::new(
            &diagram.source,
            diagram.language,
            &diagram.drawn.map,
            crate::clock::now(),
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
        tabled(diagram)?;
        let play = Play::trace(
            &diagram.source,
            diagram.language,
            &diagram.drawn.map,
            file,
            trace,
            name,
            object,
            crate::clock::now(),
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

pub mod headless;
#[cfg(test)]
pub mod testing;

#[cfg(test)]
mod tests;
