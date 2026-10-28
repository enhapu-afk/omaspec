//! The diagram on its paper, with pan and zoom by mouse, touchpad and touch.
//!
//! Mouse wheel: zoom at the pointer. A touchpad pinch arrives as the same
//! wheel (see `vendor/winit`): `1.2` to the power of the line delta is the
//! scale change. Two-finger scroll: pan; with Ctrl: zoom. Drag, and one
//! finger on a touchscreen: pan. Two fingers: pinch, zooming at their
//! midpoint. Double click: fit. Ctrl+click: what is drawn there (the window
//! jumps to its source).

use std::time::{Duration, Instant};

use iced::keyboard::{self, Modifiers};
use iced::mouse::{self, Button, Cursor, ScrollDelta};
use iced::touch;
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path, Stroke, Text};
use iced::widget::image;
use iced::window;
use iced::{Color, Font, Point, Rectangle, Renderer, Size, Theme, Vector, alignment, border};

use super::play::{self, Look, Play, Viewer};
use super::raster::Raster;
use super::view::{Glide, View};
use super::{Fade, Message};

const DOUBLE_CLICK: Duration = Duration::from_millis(350);
/// The paper's rounded corners, in screen pixels.
const RADIUS: f32 = 8.0;
/// Layers of the soft shadow, and how far the outermost one reaches.
const HALO_STEPS: u8 = 16;
const HALO_BLUR: f32 = 11.0;

pub struct DiagramCanvas<'a> {
    /// Diagram size in diagram units, once there is one.
    pub diagram: Option<Size>,
    /// The whole diagram, and when zoomed in far, the visible part sharp.
    pub overview: Option<&'a Raster>,
    pub detail: Option<&'a Raster>,
    /// The pictures before, while the new one fades in over them.
    pub fade: Option<&'a Fade>,
    pub view: View,
    /// The way the picture glides to `view`, while it does.
    pub glide: Option<Glide>,
    pub background: Color,
    pub placeholder: &'a str,
    pub text: Color,
    pub font: Font,
    /// The window's scale.
    pub scale: f32,
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
        let now = self.at.unwrap_or_else(iced::time::Instant::now);
        let view = self.glide.map_or(self.view, |g| g.at(now));
        let place = |r: omagram_uml::graph::Rect| {
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
                if state.modifiers.control() {
                    return Some(Action::publish(Message::Pick(at)).and_capture());
                }
                let now = Instant::now();
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
                Some(Action::request_redraw().and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let from = state.drag?;
                let to = cursor.position_from(bounds.position())?;
                state.drag = Some(to);
                Some(Action::publish(Message::Pan(to - from)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) => {
                state.drag.take()?;
                Some(Action::request_redraw())
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
        let view = self
            .glide
            .map_or(self.view, |g| g.at(iced::time::Instant::now()));
        match (self.diagram, self.overview) {
            (Some(size), Some(overview)) => {
                let paper = grid.snap(view.diagram_rect(size, local));
                shadow(&mut frame, paper, self.background, grid.scale);
                frame.fill(&rounded(paper, RADIUS), overview.paper);
                // Each raster at the place it shows, which after an edit may
                // differ from the new diagram until the next raster is in.
                let shown = self.fade.map_or(1.0, |fade| {
                    fade.shown.interpolate(0.0, 1.0, iced::time::Instant::now())
                });
                let old = self
                    .fade
                    .filter(|_| shown < 1.0)
                    .map_or(&[][..], |f| &f.old);
                let new = [Some(overview), self.detail].into_iter().flatten();
                let layers = old
                    .iter()
                    .map(|raster| (raster, 1.0 - shown))
                    .chain(new.map(|raster| (raster, shown)))
                    .map(|(raster, opacity)| {
                        (view.on_screen(raster.region, local), raster, opacity)
                    });
                grid.draw(&mut frame, paper, RADIUS, layers);
            }
            _ => frame.fill_text(Text {
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
        if state.drag.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(bounds) && self.overview.is_some() && state.modifiers.control() {
            mouse::Interaction::Pointer
        } else if cursor.is_over(bounds) && self.overview.is_some() {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::default()
        }
    }
}

/// The caret's name, lit in a layer of its own. In one layer iced draws
/// pictures over shapes, so this light would sit under the diagram if it
/// were drawn with the picture.
pub struct BreathLight {
    pub view: View,
    pub glide: Option<Glide>,
    pub breath: super::Breath,
    pub color: Color,
    pub scale: f32,
}

impl canvas::Program<Message> for BreathLight {
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
        (!self.breath.settled(*now)).then(Action::request_redraw)
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
        let now = iced::time::Instant::now();
        let (rect, alpha) = self.breath.at(now);
        if alpha < 0.01 || rect.width <= 0.0 || rect.height <= 0.0 {
            return vec![frame.into_geometry()];
        }
        let grid = Grid::of(bounds, self.scale);
        let view = self.glide.map_or(self.view, |glide| glide.at(now));
        let on_screen = grid.snap(view.on_screen(rect, Rectangle::with_size(bounds.size())));
        if on_screen.width <= 0.0 || on_screen.height <= 0.0 {
            return vec![frame.into_geometry()];
        }
        let radius = RADIUS.min(on_screen.width.min(on_screen.height) * 0.5);
        let path = rounded(on_screen, radius);
        frame.fill(
            &path,
            Color {
                a: 0.14 * alpha,
                ..self.color
            },
        );
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(Color {
                    a: 0.9 * alpha,
                    ..self.color
                })
                .with_width(1.5),
        );
        vec![frame.into_geometry()]
    }
}

/// While the text does not draw, a red edge round the last good picture:
/// in a layer over the pictures, as iced draws pictures over shapes within
/// one.
pub struct PaperEdge {
    pub diagram: Size,
    pub view: View,
    pub glide: Option<Glide>,
    pub color: Color,
    pub scale: f32,
}

impl canvas::Program<Message> for PaperEdge {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let grid = Grid::of(bounds, self.scale);
        let view = self
            .glide
            .map_or(self.view, |g| g.at(iced::time::Instant::now()));
        let paper = grid.snap(view.diagram_rect(self.diagram, Rectangle::with_size(bounds.size())));
        frame.stroke(
            &rounded(paper, RADIUS),
            Stroke::default().with_color(self.color).with_width(1.5),
        );
        vec![frame.into_geometry()]
    }
}

/// The screen's pixels, as seen from a canvas: where it lies in the window,
/// the window's scale, and how big the canvas is.
#[derive(Clone, Copy)]
struct Grid {
    origin: Point,
    scale: f32,
    size: Size,
}

impl Grid {
    fn of(bounds: Rectangle, scale: f32) -> Self {
        Self {
            origin: bounds.position(),
            scale: if scale > 0.0 { scale } else { 1.0 },
            size: bounds.size(),
        }
    }

    /// `rect` with its edges on whole pixels. The paper and its pictures then
    /// share an edge instead of leaving a sliver of one past the other.
    fn snap(self, rect: Rectangle) -> Rectangle {
        let on = |at: f32, from: f32| ((from + at) * self.scale).round() / self.scale - from;
        let (left, top) = (on(rect.x, self.origin.x), on(rect.y, self.origin.y));
        let right = on(rect.x + rect.width, self.origin.x);
        let bottom = on(rect.y + rect.height, self.origin.y);
        Rectangle::new(Point::new(left, top), Size::new(right - left, bottom - top))
    }

    /// Where a picture may be drawn: inside `clip` and inside this canvas.
    ///
    /// `Frame::with_clip` replaces the clip, and a picture is cut only by
    /// that clip and by the window. A diagram dragged past the pane would
    /// otherwise paint over the editor. Shapes stay in the canvas on their
    /// own; pictures do not.
    ///
    /// A rounded picture is held one device pixel inside that edge. The
    /// pixmap is cleared white past the diagram, and the linear filter reads
    /// that into the last pixel; the picture then fades over the pixel past
    /// its edge. On a dark theme either one is a light line that comes and
    /// goes as the pan moves the texels across the pixels. The paper
    /// underneath is the diagram's own background and fills that pixel. The
    /// picture is not snapped again: a second rounding can step off this
    /// edge and fade onto the next one.
    fn picture_clip(self, clip: Rectangle, radius: f32) -> Option<Rectangle> {
        let canvas = Rectangle::new(Point::ORIGIN, self.size);
        let clip = self.snap(clip).intersection(&canvas)?;
        let clip = self.snap(clip);
        if clip.width <= 0.0 || clip.height <= 0.0 {
            return None;
        }
        if radius <= 0.0 {
            return Some(clip);
        }
        let inset = 1.0 / self.scale;
        let clip = Rectangle::new(
            Point::new(clip.x + inset, clip.y + inset),
            Size::new(clip.width - inset * 2.0, clip.height - inset * 2.0),
        );
        (clip.width > 0.0 && clip.height > 0.0).then_some(clip)
    }

    /// Draws `pictures` within `clip`, its corners rounded by `radius`.
    fn draw<'a>(
        self,
        frame: &mut Frame,
        clip: Rectangle,
        radius: f32,
        pictures: impl IntoIterator<Item = (Rectangle, &'a Raster, f32)>,
    ) {
        let Some(clip) = self.picture_clip(clip, radius) else {
            return;
        };
        // The GPU fades a picture's corner out over the pixel past its edge,
        // half a pixel further out than a shape's; a larger radius puts the
        // two curves on each other, so no rim of the picture shows past the
        // paper or a veil.
        let radius = if radius > 0.0 {
            radius + 0.5 / (std::f32::consts::SQRT_2 - 1.0) / self.scale
        } else {
            0.0
        };
        frame.with_clip(clip, |frame| {
            for (at, raster, opacity) in pictures {
                frame.draw_image(
                    self.snap(at),
                    canvas::Image {
                        border_radius: radius.into(),
                        ..canvas::Image::new(raster.handle.clone())
                            .filter_method(image::FilterMethod::Linear)
                            .opacity(opacity)
                    },
                );
            }
        });
    }
}

fn rounded(rect: Rectangle, radius: f32) -> Path {
    Path::rounded_rectangle(rect.position(), rect.size(), border::Radius::from(radius))
}

/// Fills the part of the rounded `paper` from `top` to `bottom`, for a veil
/// over some of it: the paper's outline one device pixel larger, cut by a
/// clip.
///
/// The veil reaches past the paper's edge. Its own outline, the paper's
/// grown, takes no point off the paper's, as a line of its own would in the
/// corners. And no fading edge lies on another: the GPU fades each layer's
/// edges before it lays one over the other, and where the paper's edge
/// pixel is three quarters white, a veil that covers it to three quarters
/// too lets more of that white through than the paper next to it shows.
pub fn fill_paper_band(
    frame: &mut Frame,
    paper: Rectangle,
    scale: f32,
    top: f32,
    bottom: f32,
    fill: impl Into<canvas::Fill>,
) {
    let Some(clip) = band_clip(paper, scale, top, bottom) else {
        return;
    };
    let out = 1.0 / if scale > 0.0 { scale } else { 1.0 };
    let veil = Rectangle::new(
        paper.position() - Vector::new(out, out),
        paper.size() + Size::new(out * 2.0, out * 2.0),
    );
    frame.with_clip(clip, |frame| frame.fill(&rounded(veil, RADIUS + out), fill));
}

/// The clip for [`fill_paper_band`], on whole device pixels of the canvas.
/// iced moves what is drawn in a clip by as much as it takes to put the
/// clip's corner on a whole pixel; a clip that starts on one moves it as
/// far as the paper underneath, which is drawn without one. Where the band
/// reaches the paper's edge, the clip reaches past the veil's.
fn band_clip(paper: Rectangle, scale: f32, top: f32, bottom: f32) -> Option<Rectangle> {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let (paper_bottom, paper_right) = (paper.y + paper.height, paper.x + paper.width);
    let below = |at: f32| ((at * scale).floor() - 3.0) / scale;
    let past = |at: f32| ((at * scale).ceil() + 3.0) / scale;
    let row = |at: f32| (at * scale).round() / scale;
    let top = if top <= paper.y {
        below(paper.y)
    } else {
        row(top)
    };
    let bottom = if bottom >= paper_bottom {
        past(paper_bottom)
    } else {
        row(bottom)
    };
    let (left, right) = (below(paper.x), past(paper_right));
    (bottom > top)
        .then(|| Rectangle::new(Point::new(left, top), Size::new(right - left, bottom - top)))
}

/// A soft shadow under the paper, rounded rectangles that grow and fade:
/// dark and a little below it on a light background, a faint glow all
/// round on a dark one, where a shadow would not show.
///
/// `paper` is already on whole pixels. Each layer grows and drops by whole
/// pixels too. A fraction of a pixel would land inside or outside that edge
/// depending on where the pan sits, and on a dark theme the white glow then
/// draws a line that appears and disappears as the diagram moves.
fn shadow(frame: &mut Frame, paper: Rectangle, background: Color, scale: f32) {
    let (color, strength, drop) = shade(background);
    // Each step adds as much; more of them lie near the edge, so the shadow
    // fades out softly, as (1 - distance / HALO_BLUR)².
    let alpha = strength / f32::from(HALO_STEPS);
    for step in 0..HALO_STEPS {
        let Some(rect) = halo_rect(paper, step, drop, scale) else {
            continue;
        };
        let grow = (rect.width - paper.width) * 0.5;
        frame.fill(&rounded(rect, RADIUS + grow), Color { a: alpha, ..color });
    }
}

/// Ink of the halo: white and faint on a dark ground, black and a little
/// lower on a light one.
fn shade(background: Color) -> (Color, f32, f32) {
    let light = 0.2126 * background.r + 0.7152 * background.g + 0.0722 * background.b;
    if light < 0.5 {
        (Color::WHITE, 0.05, 0.0)
    } else {
        (Color::BLACK, 0.10, 2.0)
    }
}

/// One halo layer around `paper`, or `None` when it would sit entirely
/// underneath. `step` 0 is the outermost. Growth and drop are whole device
/// pixels at `scale`.
fn halo_rect(paper: Rectangle, step: u8, drop: f32, scale: f32) -> Option<Rectangle> {
    let grow = HALO_BLUR * (1.0 - (f32::from(step) / f32::from(HALO_STEPS)).sqrt());
    let grow_px = (grow * scale).round() as i32;
    let drop_px = (drop * scale).round() as i32;
    if grow_px == 0 && drop_px == 0 {
        return None;
    }
    let grow = grow_px as f32 / scale;
    let drop = drop_px as f32 / scale;
    Some(Rectangle::new(
        paper.position() + Vector::new(-grow, drop - grow),
        paper.size() + Size::new(grow * 2.0, grow * 2.0),
    ))
}

fn touch(state: &mut State, event: &touch::Event, bounds: Rectangle) -> Option<Action<Message>> {
    let (id, position, phase) = match *event {
        touch::Event::FingerPressed { id, position } => (id.0, position, Phase::Down),
        touch::Event::FingerMoved { id, position } => (id.0, position, Phase::Move),
        touch::Event::FingerLifted { id, position } | touch::Event::FingerLost { id, position } => {
            (id.0, position, Phase::Up)
        }
    };
    let at = local(position, bounds);
    match phase {
        Phase::Down => {
            if !bounds.contains(position) {
                return None;
            }
            if let Some(slot) = state.fingers.iter().position(|(finger, _)| *finger == id) {
                state.fingers[slot].1 = at;
            } else {
                state.fingers.push((id, at));
            }
            state.pinch = two_fingers(&state.fingers);
            Some(Action::capture())
        }
        Phase::Move => {
            let slot = state.fingers.iter().position(|(finger, _)| *finger == id)?;
            let previous = state.fingers[slot].1;
            state.fingers[slot].1 = at;
            if let Some((mid, span)) = two_fingers(&state.fingers) {
                let Some((prev_mid, prev_span)) = state.pinch.replace((mid, span)) else {
                    return Some(Action::capture());
                };
                let factor = span / prev_span;
                let pan = mid - prev_mid;
                if factor.is_finite() && factor > 0.0 && moved(factor, pan) {
                    Some(
                        Action::publish(Message::Pinch {
                            factor,
                            at: mid,
                            pan,
                        })
                        .and_capture(),
                    )
                } else {
                    Some(Action::capture())
                }
            } else if state.fingers.len() == 1 {
                state.pinch = None;
                let pan = at - previous;
                if moved(1.0, pan) {
                    Some(Action::publish(Message::Pan(pan)).and_capture())
                } else {
                    Some(Action::capture())
                }
            } else {
                state.pinch = None;
                Some(Action::capture())
            }
        }
        Phase::Up => {
            let slot = state.fingers.iter().position(|(finger, _)| *finger == id)?;
            state.fingers.remove(slot);
            state.pinch = two_fingers(&state.fingers);
            Some(Action::capture())
        }
    }
}

fn two_fingers(fingers: &[(u64, Point)]) -> Option<(Point, f32)> {
    if fingers.len() != 2 {
        return None;
    }
    View::pinch_sample(fingers[0].1, fingers[1].1)
}

fn moved(factor: f32, pan: Vector) -> bool {
    (factor - 1.0).abs() > 1e-4 || pan.x.abs() > 0.5 || pan.y.abs() > 0.5
}

fn local(position: Point, bounds: Rectangle) -> Point {
    let delta = position - bounds.position();
    Point::new(delta.x, delta.y)
}

enum Phase {
    Down,
    Move,
    Up,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(scale: f32) -> Grid {
        Grid::of(
            Rectangle::new(Point::new(13.4, 27.8), Size::new(400.0, 300.0)),
            scale,
        )
    }

    /// Device pixel of a canvas-local coordinate.
    fn device(grid: Grid, local: f32, axis: f32) -> i32 {
        ((axis + local) * grid.scale).round() as i32
    }

    #[test]
    fn a_picture_dragged_past_the_pane_stops_at_the_pane() {
        let grid = grid(2.0);
        // The paper hangs off the left and the bottom of the canvas.
        let paper = Rectangle::new(Point::new(-80.0, 40.0), Size::new(200.0, 400.0));
        // Square, as a lit state, and rounded, as the diagram itself.
        for radius in [0.0, RADIUS] {
            let clip = grid.picture_clip(paper, radius).unwrap();
            assert!(device(grid, clip.x, grid.origin.x) >= device(grid, 0.0, grid.origin.x));
            assert!(device(grid, clip.y, grid.origin.y) >= device(grid, 0.0, grid.origin.y));
            assert!(
                device(grid, clip.x + clip.width, grid.origin.x)
                    <= device(grid, grid.size.width, grid.origin.x)
            );
            assert!(
                device(grid, clip.y + clip.height, grid.origin.y)
                    <= device(grid, grid.size.height, grid.origin.y)
            );
        }
    }

    #[test]
    fn a_picture_outside_the_pane_is_not_drawn() {
        let grid = grid(1.0);
        let gone = Rectangle::new(Point::new(-500.0, 10.0), Size::new(40.0, 40.0));
        assert!(grid.picture_clip(gone, 0.0).is_none());
        assert!(grid.picture_clip(gone, RADIUS).is_none());
    }

    #[test]
    fn a_rounded_picture_stays_a_pixel_inside_the_paper() {
        let grid = grid(2.0);
        let paper = Rectangle::new(Point::new(40.0, 30.0), Size::new(180.0, 120.0));
        let snapped = grid.snap(paper);
        let clip = grid.picture_clip(paper, RADIUS).unwrap();
        let inset = 1.0 / grid.scale;
        for (got, edge) in [
            (clip.x, snapped.x + inset),
            (clip.y, snapped.y + inset),
            (clip.x + clip.width, snapped.x + snapped.width - inset),
            (clip.y + clip.height, snapped.y + snapped.height - inset),
        ] {
            assert!((got - edge).abs() < 1e-4, "{got} {edge}");
        }
        // A square clip, as a lit state uses, is not pulled in.
        let square = grid.picture_clip(paper, 0.0).unwrap();
        assert!((square.x - snapped.x).abs() < 1e-4);
        assert!((square.width - snapped.width).abs() < 1e-4);
    }

    #[test]
    fn the_dark_glow_keeps_its_width_while_the_paper_moves() {
        let scale = 2.0;
        let grid = grid(scale);
        let dark = Color::from_rgb(0.12, 0.12, 0.14);
        let (_, _, drop) = shade(dark);
        let mut seen = None;
        for step in 0..32 {
            let shift = step as f32 / 32.0;
            let paper = grid.snap(Rectangle::new(
                Point::new(40.0 + shift, 50.0 + shift * 0.7),
                Size::new(220.0, 140.0),
            ));
            let layers: Vec<(i32, i32)> = (0..HALO_STEPS)
                .filter_map(|step| halo_rect(paper, step, drop, scale))
                .map(|layer| {
                    let out = ((paper.x - layer.x) * scale).round() as i32;
                    let down = ((layer.y - paper.y) * scale).round() as i32;
                    assert_eq!(
                        out,
                        ((layer.width - paper.width) * 0.5 * scale).round() as i32
                    );
                    (out, down)
                })
                .collect();
            assert!(layers.iter().all(|(out, _)| *out >= 1), "{layers:?}");
            match &seen {
                None => seen = Some(layers),
                Some(first) => assert_eq!(&layers, first),
            }
        }
    }

    #[test]
    fn the_light_shadow_drops_by_whole_pixels() {
        let scale = 1.5;
        let grid = grid(scale);
        let (_, _, drop) = shade(Color::WHITE);
        let paper = grid.snap(Rectangle::new(
            Point::new(20.0, 20.0),
            Size::new(100.0, 80.0),
        ));
        let mut count = 0;
        for step in 0..HALO_STEPS {
            let Some(layer) = halo_rect(paper, step, drop, scale) else {
                continue;
            };
            count += 1;
            let grow = (paper.x - layer.x) * scale;
            let down = (layer.y - paper.y) * scale;
            assert!((grow - grow.round()).abs() < 1e-3, "{grow}");
            assert!((down - down.round()).abs() < 1e-3, "{down}");
        }
        assert!(count > 0);
    }

    #[test]
    fn a_veil_band_is_clipped_on_whole_pixels_and_past_the_paper() {
        let scale = 1.5;
        let paper = Rectangle::new(Point::new(40.3, 30.7), Size::new(180.0, 120.0));
        let whole = |at: f32| ((at * scale) - (at * scale).round()).abs() < 1e-3;
        let (right, bottom) = (paper.x + paper.width, paper.y + paper.height);
        // Over the whole paper the clip reaches past every edge.
        let all = band_clip(paper, scale, paper.y, bottom).unwrap();
        assert!(all.x < paper.x && all.y < paper.y);
        assert!(all.x + all.width > right && all.y + all.height > bottom);
        for edge in [all.x, all.y, all.x + all.width, all.y + all.height] {
            assert!(whole(edge), "{edge}");
        }
        // Bands that meet share a row: no gap, no overlap.
        let upper = band_clip(paper, scale, paper.y - 5.0, 77.4).unwrap();
        let lower = band_clip(paper, scale, 77.4, bottom + 5.0).unwrap();
        assert!((upper.y + upper.height - lower.y).abs() < 1e-4);
        assert!(whole(lower.y));
        // Less than half a pixel high is nothing.
        assert!(band_clip(paper, scale, 90.0, 90.1).is_none());
    }
}
