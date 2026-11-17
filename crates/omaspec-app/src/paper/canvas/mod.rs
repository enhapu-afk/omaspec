//! The diagram on its paper, with pan and zoom by mouse, touchpad and touch.
//!
//! Mouse wheel: zoom at the pointer. A touchpad pinch arrives as the same
//! wheel (see `vendor/winit`): `1.2` to the power of the line delta is the
//! scale change. Two-finger scroll: pan; with Ctrl: zoom. Drag, and one
//! finger on a touchscreen: pan. Two fingers: pinch, zooming at their
//! midpoint. Double click: fit. Ctrl+click: what is drawn there (the window
//! jumps to its source). A click on a link (a tile of a map, a state with a
//! diagram of its own) follows it. With Alt, a line is drawn by hand
//! ([`super::hand`]): the pointer, a drag and a double click go to it, and
//! Escape stops a drag.

use std::time::{Duration, Instant};

use iced::keyboard::{self, Modifiers};
use iced::mouse::{self, Button, Cursor, ScrollDelta};
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Text};
use iced::window;
use iced::{Color, Font, Point, Rectangle, Renderer, Size, Theme, Vector, alignment};

use omaspec_render::Link;

use super::hand::Gesture;
use super::play::{self, Look, Play, Viewer};
use super::raster::Raster;
use super::view::{Glide, View};
use super::{Fade, Message, Previous, link_rect, link_under};

mod bands;
mod grid;
mod input;
mod marks;

use input::touch;

pub use bands::{Headings, KeptBands};
pub(super) use grid::{
    Grid, fill_paper_band, fill_paper_band_across, fill_paper_holed, rounded, shadow,
};
pub use marks::{BreathLight, LinkMarks, PaperEdge};

const DOUBLE_CLICK: Duration = Duration::from_millis(350);
/// A press that moves less than this far is a click, not a drag.
const CLICK_SLOP: f32 = 4.0;
/// The paper's rounded corners, in screen pixels.
pub(super) const RADIUS: f32 = 8.0;
/// Layers of the soft shadow, and how far the outermost one reaches.
const HALO_STEPS: u8 = 16;
const HALO_BLUR: f32 = 11.0;

pub struct DiagramCanvas<'a> {
    /// Diagram size in diagram units, once there is one.
    pub diagram: Option<Size>,
    /// The whole diagram, and when zoomed in far, the visible part sharp.
    pub overview: Option<&'a Raster>,
    pub detail: Option<&'a Raster>,
    /// The overview and the detail shown just before these, drawn under
    /// them for a moment ([`super::raster::Layer::under`]).
    pub under: [Option<&'a Raster>; 2],
    /// The pictures before, while the new one fades in over them.
    pub fade: Option<&'a Fade>,
    /// The pictures still on screen, where they were drawn, until the new
    /// raster arrives and the fade starts.
    pub previous: Option<&'a Previous>,
    pub view: View,
    /// The way the picture glides to `view`, while it does.
    pub glide: Option<Glide>,
    pub background: Color,
    pub placeholder: &'a str,
    pub text: Color,
    pub font: Font,
    /// The window's scale.
    pub scale: f32,
    /// Whether Alt draws lines by hand here.
    pub by_hand: bool,
    /// The pointer over a line lit to draw by hand.
    pub hand_cursor: Option<mouse::Interaction>,
    /// The tiles and states a click follows; none while a play runs.
    pub links: &'a [Link],
    /// The containers a click folds and the boxes it unfolds
    /// (docs/adr/0024).
    pub folds: Vec<super::fold::Spot>,
    /// Whether Shift and a click focus a box.
    pub focusing: bool,
    /// The headings held at the pane's edge: a point in one is the
    /// heading's own ([`bands::through`]).
    pub headings: Option<Headings<'a>>,
}

impl DiagramCanvas<'_> {
    /// Where the point `at` of a pane this large lies in the picture: in a
    /// held heading, the heading's own point.
    fn through(&self, at: Point, pane: Size) -> Point {
        self.headings.map_or(at, |headings| {
            headings.through(at, Rectangle::with_size(pane))
        })
    }

    /// The link at `at`, a point of the pane.
    fn link_at(&self, at: Point, viewport: Size) -> Option<&Link> {
        link_under(self.links, self.view.to_diagram(at, viewport))
    }

    /// The boxes gliding across this frame, through the last one, drawn
    /// from the two sheets taken when the glide started: a newer render
    /// waits for its end ([`super::arrival::Next`]).
    fn paint_morph(
        &self,
        frame: &mut Frame,
        grid: Grid,
        local: Rectangle,
        now: iced::time::Instant,
    ) -> bool {
        let Some(fade) = self.fade else {
            return false;
        };
        let Some(morph) = fade.morph.as_ref() else {
            return false;
        };
        // The sheets of the two pictures: the ground, without lines, and
        // each part on its own (docs/adr/0029).
        let Some((old, new)) = &fade.bare else {
            return false;
        };
        let t = fade.shown.interpolate(0.0, 1.0, now).clamp(0.0, 1.0);
        let new_view = self.glide.map_or(fade.new_view, |glide| glide.at(now));
        super::morph::paint(
            frame,
            grid,
            local,
            &super::morph::Paint {
                t,
                morph,
                old_view: fade.old_view,
                old_size: fade.old_size,
                old,
                new_view,
                new_size: fade.new_size,
                new,
                background: self.background,
            },
        );
        true
    }

    /// The sheets to draw at `now`. Alpha is read here, when the frame is
    /// drawn, so a crossfade keeps moving between updates of the pane.
    ///
    /// A hold is the picture still on screen, at the view it was drawn in.
    /// A fold, or any change of view or size, is two sheets. An edit that
    /// keeps the view is one sheet, the old rasters under the new ones.
    /// With nothing to fade out, the new picture comes in on its own.
    fn sheets(&self, view: View, now: iced::time::Instant) -> Option<Vec<Sheet<'_>>> {
        let size = self.diagram?;
        let overview = self.overview?;
        let shown = self
            .fade
            .map_or(1.0, |fade| fade.shown.interpolate(0.0, 1.0, now));
        let at = |opacity: f32| -> Vec<(&Raster, f32)> {
            [self.under[0], Some(overview), self.under[1], self.detail]
                .into_iter()
                .flatten()
                .map(|raster| (raster, opacity))
                .collect()
        };
        if let Some(previous) = self.previous {
            return Some(vec![Sheet {
                view: previous.view_at(view),
                size: previous.size,
                pictures: previous
                    .pictures
                    .iter()
                    .map(|raster| (raster, 1.0))
                    .collect(),
                alpha: 1.0,
            }]);
        }
        if let Some(fade) = self.fade.filter(|fade| shown < 1.0 && !fade.old.is_empty()) {
            if fade.old_view != view || fade.old_size != size {
                return Some(vec![
                    Sheet {
                        view: fade.old_view,
                        size: fade.old_size,
                        pictures: fade.old.iter().map(|raster| (raster, 1.0)).collect(),
                        alpha: 1.0 - shown,
                    },
                    Sheet {
                        view,
                        size,
                        pictures: at(1.0),
                        alpha: shown,
                    },
                ]);
            }
            let under = fade.old.iter().map(|raster| (raster, 1.0 - shown));
            return Some(vec![Sheet {
                view,
                size,
                pictures: under.chain(at(shown)).collect(),
                alpha: 1.0,
            }]);
        }
        // Nothing old to cover: the new raster fades in, it does not pop.
        let opacity = if self.fade.is_some() && shown < 1.0 {
            shown
        } else {
            1.0
        };
        Some(vec![Sheet {
            view,
            size,
            pictures: at(opacity),
            alpha: 1.0,
        }])
    }
}

/// The lines of a glide ([`super::wires`]), over the pictures and the
/// boxes on their way: in the picture's layer the rasters would cover them.
pub struct GlideLines<'a> {
    pub fade: &'a Fade,
    pub glide: Option<Glide>,
    pub background: Color,
    /// The window's scale.
    pub scale: f32,
}

impl canvas::Program<Message> for GlideLines<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &Event,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> Option<Action<Message>> {
        // The picture's layer ends the fade; this one only keeps up.
        match event {
            Event::Window(window::Event::RedrawRequested(now))
                if self.fade.shown.is_animating(*now) =>
            {
                Some(Action::request_redraw())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let fade = self.fade;
        let (Some(morph), Some(lines), Some((old, new))) =
            (fade.morph.as_ref(), fade.lines.as_ref(), fade.bare.as_ref())
        else {
            return Vec::new();
        };
        let now = crate::clock::now();
        let t = fade.shown.interpolate(0.0, 1.0, now).clamp(0.0, 1.0);
        let new_view = self.glide.map_or(fade.new_view, |glide| glide.at(now));
        let grid = Grid::of(bounds, self.scale);
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let sheets = super::morph::sheets(
            grid,
            local,
            &super::morph::Paint {
                t,
                morph,
                old_view: fade.old_view,
                old_size: fade.old_size,
                old,
                new_view,
                new_size: fade.new_size,
                new,
                background: self.background,
            },
        );
        let mut frame = Frame::new(renderer, bounds.size());
        super::wires::paint(&mut frame, local, lines, &sheets);
        vec![frame.into_geometry()]
    }
}

/// A play over the diagram, in a layer of its own: iced draws pictures
/// over shapes within one layer, so the veil and the light that comes
/// through it (`over: false`) lie in one layer, and the glows and the
/// travelling lights (`over: true`) in the next.
pub struct PlayCanvas<'a> {
    pub diagram: Size,
    pub overview: &'a Raster,
    pub detail: Option<&'a Raster>,
    pub view: View,
    pub glide: Option<Glide>,
    pub play: &'a Play,
    pub look: Look,
    pub over: bool,
    /// The moment to draw; `None` is the moment of drawing.
    pub at: Option<iced::time::Instant>,
    /// When the play began to fade away, once it was stopped.
    pub leaving: Option<iced::time::Instant>,
    /// The window's scale.
    pub scale: f32,
}

impl canvas::Program<Message> for PlayCanvas<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &Event,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> Option<Action<Message>> {
        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        if !self.over {
            return None;
        }
        match self.leaving {
            Some(since) if play::gone(*now, since) => Some(Action::publish(Message::PlayGone)),
            Some(_) => Some(Action::request_redraw()),
            None if self.play.animating(*now) => Some(Action::request_redraw()),
            None => None,
        }
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let now = self.at.unwrap_or_else(crate::clock::now);
        let view = self.glide.map_or(self.view, |g| g.at(now));
        let place = |r: omaspec_uml::graph::Rect| {
            view.on_screen(
                Rectangle::new(Point::new(r.x, r.y), Size::new(r.w, r.h)),
                local,
            )
        };
        let grid = Grid::of(bounds, self.scale);
        let paint = |frame: &mut Frame, clip: Rectangle, opacity: f32| {
            let pictures = [Some(self.overview), self.detail]
                .into_iter()
                .flatten()
                .map(|raster| (view.on_screen(raster.region, local), raster, opacity));
            grid.draw(frame, clip, 0.0, pictures);
        };
        let viewer = Viewer {
            paper: grid.snap(view.diagram_rect(self.diagram, local)),
            scale: grid.scale,
            place: &place,
            paint: &paint,
        };
        let look = match self.leaving {
            Some(since) => self.look.faded(play::fading(now, since)),
            None => self.look,
        };
        if self.over {
            self.play.draw_over(&mut frame, &look, &viewer, now);
        } else {
            self.play.draw_under(&mut frame, &look, &viewer, now);
        }
        vec![frame.into_geometry()]
    }
}

#[derive(Default)]
pub struct State {
    drag: Option<Point>,
    last_click: Option<Instant>,
    viewport: Size,
    modifiers: Modifiers,
    /// Fingers on this paper, in pane coordinates.
    fingers: Vec<(u64, Point)>,
    /// Midpoint and span of the current two-finger pinch.
    pinch: Option<(Point, f32)>,
    /// A line is being drawn by hand: the pointer goes to it, not to a pan.
    hand_drag: bool,
    /// The pointer with Alt was last told to the pane.
    hand_told: bool,
    /// The last press with Alt; a plain click does not make it double.
    hand_click: Option<Instant>,
    /// Where the last plain press was, and whether the pointer has left
    /// it since: a click that stays follows a link.
    press: Option<(Point, bool)>,
}

impl canvas::Program<Message> for DiagramCanvas<'_> {
    type State = State;

    fn update(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<Message>> {
        if bounds.size() != state.viewport {
            state.viewport = bounds.size();
            return Some(Action::publish(Message::Viewport(bounds.size())));
        }
        // A glide draws itself frame by frame, then asks for sharp pixels.
        if let Event::Window(window::Event::RedrawRequested(now)) = event
            && let Some(glide) = self.glide
        {
            return Some(if glide.done(*now) {
                Action::publish(Message::Glided)
            } else {
                Action::request_redraw()
            });
        }
        // The fade draws itself frame by frame, then lets the old picture go.
        if let Event::Window(window::Event::RedrawRequested(now)) = event
            && let Some(fade) = self.fade
        {
            return Some(if fade.shown.is_animating(*now) {
                Action::request_redraw()
            } else {
                Action::publish(Message::Faded)
            });
        }
        if let Some(action) = self.by_hand(state, event, bounds, cursor) {
            return Some(action);
        }
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                // The pointer shows whether a click jumps.
                Some(Action::request_redraw())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let at = cursor.position_in(bounds)?;
                let message = match *delta {
                    ScrollDelta::Lines { y, .. } => Message::ZoomStep(1.2f32.powf(y), at),
                    ScrollDelta::Pixels { y, .. } if state.modifiers.control() => {
                        Message::ZoomAt((y / 300.0).exp(), at)
                    }
                    ScrollDelta::Pixels { x, y } => Message::Pan(Vector::new(x, y)),
                };
                Some(Action::publish(message).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(Button::Left)) => {
                let at = cursor.position_in(bounds)?;
                let picked = self.through(at, bounds.size());
                if state.modifiers.control() {
                    return Some(Action::publish(Message::Pick(picked)).and_capture());
                }
                if state.modifiers.shift() && self.focusing && self.overview.is_some() {
                    return Some(Action::publish(Message::FocusAt(picked)).and_capture());
                }
                let now = crate::clock::now();
                state.hand_click = None;
                if state
                    .last_click
                    .is_some_and(|t| now.duration_since(t) < DOUBLE_CLICK)
                {
                    state.last_click = None;
                    state.drag = None;
                    return Some(Action::publish(Message::Fit).and_capture());
                }
                state.last_click = Some(now);
                state.drag = Some(at);
                state.press = Some((at, false));
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(Button::Right)) => {
                let at = self.through(cursor.position_in(bounds)?, bounds.size());
                let in_window = cursor.position()?;
                Some(Action::publish(Message::Context(at, in_window)).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let from = state.drag?;
                let to = cursor.position_from(bounds.position())?;
                state.drag = Some(to);
                if let Some((at, moved)) = &mut state.press {
                    *moved |= at.distance(to) > CLICK_SLOP;
                }
                Some(Action::publish(Message::Pan(to - from)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) => {
                state.drag.take()?;
                // Where it was pressed, in a held heading the heading's.
                let press = state
                    .press
                    .take()
                    .map(|(at, moved)| (self.through(at, bounds.size()), moved));
                // A click on a fold button or a folded box (docs/adr/0024).
                let fold = match press {
                    Some((at, false)) => {
                        super::fold::click(&self.folds, self.view, at, bounds.size())
                    }
                    _ => None,
                };
                if let Some(message) = fold {
                    state.last_click = None;
                    return Some(Action::publish(message));
                }
                match press {
                    Some((at, false)) if self.link_at(at, bounds.size()).is_some() => {
                        state.last_click = None;
                        Some(Action::publish(Message::Follow(at)))
                    }
                    _ => Some(Action::request_redraw()),
                }
            }
            Event::Touch(event) => touch(state, event, bounds),
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), self.background);
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let grid = Grid::of(bounds, self.scale);
        let now = crate::clock::now();
        let view = self.glide.map_or(self.view, |g| g.at(now));
        if self.paint_morph(&mut frame, grid, local, now) {
            told(|| "draw: a glide by parts".to_owned());
            return vec![frame.into_geometry()];
        }
        match self.sheets(view, now) {
            Some(sheets) => {
                told(|| {
                    let sheets: Vec<String> = sheets
                        .iter()
                        .map(|sheet| {
                            let pictures: Vec<String> = sheet
                                .pictures
                                .iter()
                                .map(|(raster, opacity)| {
                                    format!(
                                        "{:.0}x{:.0}@{opacity:.2} #{:02x}{:02x}{:02x}",
                                        raster.region.width,
                                        raster.region.height,
                                        (raster.paper.r * 255.0) as u8,
                                        (raster.paper.g * 255.0) as u8,
                                        (raster.paper.b * 255.0) as u8,
                                    )
                                })
                                .collect();
                            format!("sheet alpha {:.2} [{}]", sheet.alpha, pictures.join(", "))
                        })
                        .collect();
                    format!(
                        "draw: previous {}, fade {}, {}",
                        self.previous.is_some(),
                        self.fade.is_some(),
                        sheets.join("; ")
                    )
                });
                ground_under(&mut frame, grid, local, &sheets);
                for sheet in sheets {
                    draw_sheet(&mut frame, grid, self.background, local, sheet);
                }
            }
            None => frame.fill_text(Text {
                content: self.placeholder.to_owned(),
                position: local.center(),
                color: self.text,
                size: 15.0.into(),
                font: self.font,
                align_x: alignment::Horizontal::Center.into(),
                align_y: alignment::Vertical::Center,
                ..Text::default()
            }),
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        if state.hand_drag {
            self.hand_cursor.unwrap_or(mouse::Interaction::Grabbing)
        } else if state.drag.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) && self.by_hand && state.modifiers.alt() {
            self.hand_cursor.unwrap_or_default()
        } else if cursor.is_over(bounds)
            && self.overview.is_some()
            && (state.modifiers.control()
                || cursor.position_in(bounds).is_some_and(|at| {
                    let at = self.through(at, bounds.size());
                    self.link_at(at, bounds.size()).is_some()
                        || super::fold::click(&self.folds, self.view, at, bounds.size()).is_some()
                }))
        {
            // Ctrl+click jumps to the source; a click on a link follows it.
            mouse::Interaction::Pointer
        } else if cursor.is_over(bounds) && self.overview.is_some() {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

/// One sheet of paper: where it sits, the pictures on it, and how present it is.
struct Sheet<'a> {
    view: View,
    size: Size,
    pictures: Vec<(&'a Raster, f32)>,
    alpha: f32,
}

/// Where two sheets cross while one fades into the other, solid paper
/// under both. Each is only partly there, the old at `1 - t` and the new at
/// `t`, so without it the pane's background would show through by
/// `t (1 - t)`, a quarter halfway: a dark flash on a dark theme. With it
/// the crossing fades as a single sheet does, over its own paper.
fn ground_under(frame: &mut Frame, grid: Grid, local: Rectangle, sheets: &[Sheet<'_>]) {
    let [old, new] = sheets else {
        return;
    };
    if old.alpha <= 0.01 || new.alpha <= 0.01 {
        return;
    }
    let Some((first, _)) = new.pictures.first().or(old.pictures.first()) else {
        return;
    };
    let a = grid.snap(old.view.diagram_rect(old.size, local));
    let b = grid.snap(new.view.diagram_rect(new.size, local));
    if let Some(both) = a.intersection(&b) {
        frame.fill(&rounded(both, RADIUS), first.paper);
    }
}

/// Draws one sheet: its shadow, its ground and its pictures.
/// A fold draws the old sheet and the new one, each in its own view.
fn draw_sheet(
    frame: &mut Frame,
    grid: Grid,
    background: Color,
    local: Rectangle,
    sheet: Sheet<'_>,
) {
    if sheet.alpha <= 0.01 {
        return;
    }
    let paper = grid.snap(sheet.view.diagram_rect(sheet.size, local));
    shadow(frame, paper, background, grid.scale, sheet.alpha);
    if let Some((first, _)) = sheet.pictures.first() {
        frame.fill(
            &rounded(paper, RADIUS),
            Color {
                a: sheet.alpha,
                ..first.paper
            },
        );
    }
    let Sheet {
        view,
        pictures,
        alpha,
        ..
    } = sheet;
    grid.draw(
        frame,
        paper,
        RADIUS,
        pictures.into_iter().map(|(raster, opacity)| {
            (
                view.on_screen(raster.region, local),
                raster,
                opacity * alpha,
            )
        }),
    );
}

/// Says what the diagram's canvas draws in a frame, on standard error,
/// where `OMASPEC_DRAWS` is set: to find a frame in which the picture is
/// gone. Each line with the milliseconds since the first.
fn told(what: impl FnOnce() -> String) {
    use std::sync::OnceLock;
    static ON: OnceLock<Option<std::time::Instant>> = OnceLock::new();
    let since =
        ON.get_or_init(|| std::env::var_os("OMASPEC_DRAWS").map(|_| std::time::Instant::now()));
    if let Some(since) = since {
        eprintln!("{:>8} {}", since.elapsed().as_millis(), what());
    }
}
