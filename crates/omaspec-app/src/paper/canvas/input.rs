//! The pointer, the keys and the fingers on the diagram: a line drawn by
//! hand, a box held, a pinch.

use iced::keyboard::{self};
use iced::mouse::{self, Button, Cursor};
use iced::touch;
use iced::widget::canvas::{Action, Event};
use iced::{Point, Rectangle, Vector};

use super::*;

impl DiagramCanvas<'_> {
    /// The pointer with Alt, a drag that began with it, and Escape during
    /// one: to the line drawn by hand. `None` leaves the event to the pan
    /// and the zoom.
    pub(super) fn by_hand(
        &self,
        state: &mut State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<Message>> {
        let tell = |gesture| Action::publish(Message::Hand(gesture));
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) if state.hand_drag => {
                let at = cursor.position_from(bounds.position())?;
                Some(tell(Gesture::Move(at)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(Button::Left)) if state.hand_drag => {
                state.hand_drag = false;
                Some(tell(Gesture::Release).and_capture())
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) if state.hand_drag => {
                state.hand_drag = false;
                Some(tell(Gesture::Cancel).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(Button::Left))
                if self.by_hand && state.modifiers.alt() && !state.modifiers.control() =>
            {
                let at = cursor.position_in(bounds)?;
                let now = crate::clock::now();
                let double = state
                    .hand_click
                    .is_some_and(|t| now.duration_since(t) < DOUBLE_CLICK);
                state.last_click = None;
                if double {
                    state.hand_click = None;
                    return Some(tell(Gesture::Double(at)).and_capture());
                }
                state.hand_click = Some(now);
                state.hand_drag = true;
                Some(tell(Gesture::Press(at)).and_capture())
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                self.hover(state, bounds, cursor)
                    .map(|a| a.and_capture())
                    .or(Some(Action::request_redraw()))
            }
            // Without a pan going on, the pointer has nothing else to do.
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
                if state.drag.is_none() =>
            {
                self.hover(state, bounds, cursor)
            }
            _ => None,
        }
    }

    /// Tells the pane where the pointer is while Alt is held, and once that
    /// it is not; `None` when there is nothing new to tell.
    pub(super) fn hover(
        &self,
        state: &mut State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<Message>> {
        let at = cursor
            .position_in(bounds)
            .filter(|_| self.by_hand && state.modifiers.alt());
        match at {
            Some(at) => {
                state.hand_told = true;
                Some(Action::publish(Message::Hand(Gesture::Hover(Some(at)))))
            }
            None if std::mem::take(&mut state.hand_told) => {
                Some(Action::publish(Message::Hand(Gesture::Hover(None))))
            }
            None => None,
        }
    }
}

pub(super) fn touch(
    state: &mut State,
    event: &touch::Event,
    bounds: Rectangle,
) -> Option<Action<Message>> {
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
