//! Containers folded to their box in the pane (docs/adr/0024): a composite
//! state, a package or a subgraph drawn as a box of its own, as a state
//! with a diagram of its own is, the same two small squares on it. The
//! pane keeps which ones; the file never says so. A small button on the
//! corner of a container under the pointer folds it; a click on a folded
//! box unfolds it and glides to what it holds.

use iced::mouse::{self, Cursor};
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, window};

use omaspec_render::pick;

use super::canvas::{Grid, RADIUS, rounded};
use super::view::{Glide, View};
use super::{Message, Paper};

/// The fold button's side, in screen pixels.
const BUTTON: f32 = 16.0;
/// How far, edges summed, a drawn box may lie from its place in the map
/// and still be that place. A sketch's stroke bows.
const SAME_BOX: f32 = 48.0;

/// A container the pane may fold, or one it folded.
#[derive(Debug, Clone, PartialEq)]
pub struct Spot {
    pub name: String,
    /// Where it is, in the diagram's units.
    pub rect: Rectangle,
    pub folded: bool,
}

/// The fold button of a container whose box is `on_screen`: in its top
/// right corner, inside it. None on a box too small to hold one.
pub fn button(on_screen: Rectangle) -> Option<Rectangle> {
    (on_screen.width >= BUTTON * 3.0 && on_screen.height >= BUTTON * 2.0).then(|| {
        Rectangle::new(
            Point::new(
                on_screen.x + on_screen.width - BUTTON - 5.0,
                on_screen.y + 5.0,
            ),
            Size::new(BUTTON, BUTTON),
        )
    })
}

/// The spot under `at` (in the diagram's units), the smallest of those
/// around it.
pub fn spot_under(spots: &[Spot], at: Point) -> Option<usize> {
    spots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.rect.contains(at))
        .min_by(|(_, a), (_, b)| {
            (a.rect.width * a.rect.height).total_cmp(&(b.rect.width * b.rect.height))
        })
        .map(|(index, _)| index)
}

/// What a click at `at`, a point of the pane, does to a fold: unfolds the
/// folded box under it, or folds the container whose button it hits.
pub fn click(spots: &[Spot], view: View, at: Point, viewport: Size) -> Option<Message> {
    let p = view.to_diagram(at, viewport);
    let local = Rectangle::with_size(viewport);
    if let Some(index) = spot_under(spots, p)
        && spots[index].folded
    {
        return Some(Message::Unfold(spots[index].name.clone()));
    }
    // The button of the innermost container it lies on.
    spots
        .iter()
        .filter(|s| !s.folded)
        .filter_map(|s| Some((s, button(view.on_screen(s.rect, local))?)))
        .filter(|(_, b)| b.contains(at))
        .min_by(|(a, _), (b, _)| {
            (a.rect.width * a.rect.height).total_cmp(&(b.rect.width * b.rect.height))
        })
        .map(|(s, _)| Message::Fold(s.name.clone()))
}

impl Paper {
    /// Whether the folds changed since the last call: the owner draws the
    /// diagram again with them.
    pub fn take_refold(&mut self) -> bool {
        std::mem::take(&mut self.refold)
    }

    /// Folds the container `name`, or unfolds it; once drawn, the view
    /// moves to show what an unfold brought back if it lies outside.
    pub fn set_fold(&mut self, name: &str, folded: bool) {
        let had = self.folds.iter().any(|n| n == name);
        if had == folded {
            return;
        }
        if folded {
            self.folds.push(name.to_owned());
            self.unfolding = None;
        } else {
            self.folds.retain(|n| n != name);
            self.unfolding = Some(name.to_owned());
        }
        self.change = super::Change::Fold {
            anchor: Some(name.to_owned()),
            since: self.requested,
        };
        self.refold = true;
    }

    /// Unfolds every container; whether one was folded.
    pub fn unfold_all(&mut self) -> bool {
        if self.folds.is_empty() {
            return false;
        }
        self.folds.clear();
        self.unfolding = None;
        self.change = super::Change::Fold {
            anchor: None,
            since: self.requested,
        };
        self.refold = true;
        true
    }

    /// The containers of the picture and the boxes folded in it.
    pub(super) fn fold_spots(&self) -> Vec<Spot> {
        if !self.folding || self.document || self.play.is_some() {
            return Vec::new();
        }
        // The buttons sit on the finished boxes. While they glide, a button
        // would land where a box has not arrived yet.
        if self.fade.as_ref().is_some_and(|fade| fade.morph.is_some()) {
            return Vec::new();
        }
        let Some(diagram) = &self.diagram else {
            return Vec::new();
        };
        diagram
            .drawn
            .map
            .places
            .iter()
            .filter_map(|place| {
                let folded = self.folds.contains(&place.name);
                (place.frame || folded).then(|| Spot {
                    name: place.name.clone(),
                    rect: Rectangle::new(
                        Point::new(place.rect.x, place.rect.y),
                        Size::new(place.rect.w, place.rect.h),
                    ),
                    folded: folded && !place.frame,
                })
            })
            .collect()
    }

    /// The box whose name the caret is on, in a picture of boxes. A name
    /// that names a box picks that box: by the name, else by the box drawn
    /// where the name's shape is. The middle of a container lies in what it
    /// holds, so it is never asked. Words that name no box pick the
    /// smallest box around them.
    pub fn box_at_caret(&self, source: &str, row: usize, col: usize) -> Option<String> {
        let (text, seat) = self.seat_at(source, row, col)?;
        let map = &self.diagram.as_ref()?.drawn.map;
        let (x, y, w, h) = seat.bounds();
        if let pick::Seat::Shape { .. } = seat {
            if let Some(place) = map.places.iter().find(|p| p.name == text) {
                return Some(place.name.clone());
            }
            let off = |p: &&omaspec_uml::map::Place| {
                (p.rect.x - x).abs()
                    + (p.rect.y - y).abs()
                    + (p.rect.w - w).abs()
                    + (p.rect.h - h).abs()
            };
            if let Some(place) = map
                .places
                .iter()
                .filter(|p| off(p) <= SAME_BOX)
                .min_by(|a, b| off(a).total_cmp(&off(b)))
            {
                return Some(place.name.clone());
            }
        }
        let middle = Point::new(x + w / 2.0, y + h / 2.0);
        map.places
            .iter()
            .filter(|p| {
                p.rect.x <= middle.x
                    && middle.x <= p.rect.x + p.rect.w
                    && p.rect.y <= middle.y
                    && middle.y <= p.rect.y + p.rect.h
            })
            .min_by(|a, b| (a.rect.w * a.rect.h).total_cmp(&(b.rect.w * b.rect.h)))
            .map(|p| p.name.clone())
    }

    /// The container at the caret folded, or unfolded when it is; what
    /// that did, for the status bar.
    pub fn fold_at_caret(
        &mut self,
        source: &str,
        row: usize,
        col: usize,
    ) -> Result<String, String> {
        let name = self
            .box_at_caret(source, row, col)
            .ok_or("put the caret on the name of a nested state, a package or a subgraph")?;
        let spots = self.fold_spots();
        match spots.iter().find(|s| s.name == name) {
            Some(spot) if spot.folded => {
                self.set_fold(&name, false);
                Ok(format!("{name} unfolded"))
            }
            Some(_) => {
                self.set_fold(&name, true);
                Ok(format!("{name} folded"))
            }
            None => Err(format!("{name} holds nothing to fold")),
        }
    }

    /// Once the picture with a container unfolded is in: the view that
    /// shows it, when the one on screen does not. A fitted view fits the
    /// new picture as it is.
    pub(super) fn view_for_unfold(&mut self) -> Option<View> {
        let name = self.unfolding.take()?;
        if self.view.fit || self.viewport == Size::ZERO {
            return None;
        }
        let map = &self.diagram.as_ref()?.drawn.map;
        let place = map.places.iter().find(|p| p.name == name)?;
        let margin = 24.0;
        let area = Rectangle::new(
            Point::new(place.rect.x - margin, place.rect.y - margin),
            Size::new(place.rect.w + 2.0 * margin, place.rect.h + 2.0 * margin),
        );
        let visible = self.view.visible(self.viewport);
        if visible.contains(area.position())
            && visible.contains(Point::new(area.x + area.width, area.y + area.height))
        {
            return None;
        }
        // Moved there at the same zoom, or zoomed out until it fits.
        let shown = View::showing(area, self.viewport);
        Some(View {
            zoom: shown.zoom.min(self.view.zoom),
            center: area.center(),
            fit: false,
        })
    }
}

/// The marks of the folds, in a layer of their own: the two small squares
/// on a folded box, and on the container under the pointer the button
/// that folds it.
pub struct FoldMarks {
    pub spots: Vec<Spot>,
    pub view: View,
    pub glide: Option<Glide>,
    pub color: Color,
    pub paper: Color,
    pub scale: f32,
}

#[derive(Default)]
pub struct MarkState {
    /// The spot the pointer is over, and whether over its button.
    over: Option<(usize, bool)>,
}

impl FoldMarks {
    fn under(&self, cursor: Cursor, bounds: Rectangle) -> Option<(usize, bool)> {
        let at = cursor.position_in(bounds)?;
        let index = spot_under(&self.spots, self.view.to_diagram(at, bounds.size()))?;
        let local = Rectangle::with_size(bounds.size());
        let on_button = button(self.view.on_screen(self.spots[index].rect, local))
            .is_some_and(|b| b.contains(at));
        Some((index, on_button))
    }
}

impl canvas::Program<Message> for FoldMarks {
    type State = MarkState;

    fn update(
        &self,
        state: &mut MarkState,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<Message>> {
        match event {
            Event::Window(window::Event::RedrawRequested(now)) => self
                .glide
                .filter(|g| !g.done(*now))
                .map(|_| Action::request_redraw()),
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let over = self.under(cursor, bounds);
                (over != state.over).then(|| {
                    state.over = over;
                    Action::request_redraw()
                })
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &MarkState,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let now = iced::time::Instant::now();
        let view = self.glide.map_or(self.view, |glide| glide.at(now));
        let grid = Grid::of(bounds, self.scale);
        let local = Rectangle::with_size(bounds.size());
        for (index, spot) in self.spots.iter().enumerate() {
            let on_screen = grid.snap(view.on_screen(spot.rect, local));
            if on_screen.width <= 1.0 || on_screen.height <= 1.0 {
                continue;
            }
            let over = state.over.filter(|(i, _)| *i == index);
            if spot.folded {
                if over.is_some() {
                    let radius = RADIUS.min(on_screen.width.min(on_screen.height) * 0.5);
                    let path = rounded(on_screen, radius);
                    frame.fill(
                        &path,
                        Color {
                            a: 0.08,
                            ..self.color
                        },
                    );
                    frame.stroke(
                        &path,
                        Stroke::default().with_color(self.color).with_width(2.0),
                    );
                }
                squares(&mut frame, on_screen, self.color);
            } else if let (Some((_, on_button)), Some(b)) = (over, button(on_screen)) {
                let path = rounded(b, 4.0);
                let (fill, ink) = if on_button {
                    (self.color, self.paper)
                } else {
                    (self.paper, self.color)
                };
                frame.fill(&path, fill);
                frame.stroke(
                    &path,
                    Stroke::default().with_color(self.color).with_width(1.2),
                );
                let c = b.center();
                let dash = Path::line(Point::new(c.x - 4.0, c.y), Point::new(c.x + 4.0, c.y));
                frame.stroke(&dash, Stroke::default().with_color(ink).with_width(1.6));
            }
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &MarkState,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> mouse::Interaction {
        // A Pointer here levitates the cursor in the stack, and the diagram
        // canvas under this layer is what publishes the fold. It already
        // shows the hand where a click would fold or unfold.
        mouse::Interaction::None
    }
}

/// Two small squares, one behind the other, on the box's top right
/// corner: there is more inside, as on a state with a diagram of its own.
pub fn squares(frame: &mut Frame, on_screen: Rectangle, color: Color) {
    let size = (on_screen.height * 0.22).clamp(5.0, 9.0);
    let at = Point::new(
        on_screen.x + on_screen.width - size * 0.5,
        on_screen.y - size * 0.5,
    );
    let back = Path::rectangle(Point::new(at.x - 2.5, at.y + 2.5), Size::new(size, size));
    let front = Path::rectangle(at, Size::new(size, size));
    frame.stroke(&back, Stroke::default().with_color(color).with_width(1.2));
    frame.fill(&front, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spot(name: &str, x: f32, y: f32, w: f32, h: f32, folded: bool) -> Spot {
        Spot {
            name: name.into(),
            rect: Rectangle::new(Point::new(x, y), Size::new(w, h)),
            folded,
        }
    }

    #[test]
    fn a_click_folds_by_the_button_and_unfolds_anywhere_on_the_box() {
        let viewport = Size::new(400.0, 300.0);
        let view = View {
            zoom: 1.0,
            center: Point::new(200.0, 150.0),
            fit: false,
        };
        let spots = [
            spot("Outer", 10.0, 10.0, 300.0, 200.0, false),
            spot("Inner", 40.0, 60.0, 120.0, 80.0, false),
            spot("Done", 320.0, 20.0, 60.0, 30.0, true),
        ];
        // The inner container's button, not the outer one's.
        let inner = button(Rectangle::new(
            Point::new(40.0, 60.0),
            Size::new(120.0, 80.0),
        ))
        .unwrap();
        assert!(matches!(
            click(&spots, view, inner.center(), viewport),
            Some(Message::Fold(name)) if name == "Inner"
        ));
        // Inside a container but off its button: nothing.
        assert!(click(&spots, view, Point::new(60.0, 120.0), viewport).is_none());
        assert!(matches!(
            click(&spots, view, Point::new(340.0, 40.0), viewport),
            Some(Message::Unfold(name)) if name == "Done"
        ));
    }
}
