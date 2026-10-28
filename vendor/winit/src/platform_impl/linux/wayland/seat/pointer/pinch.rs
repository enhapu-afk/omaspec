//! Omagram addition to winit 0.30.13: Wayland touchpad pinch.
//!
//! `zwp_pointer_gestures_v1` reports an absolute scale since the gesture
//! began (1 at the start). The canvas zooms on a mouse-wheel line delta
//! by `1.2^y`, so each update is sent as that wheel with `y` chosen to
//! match the scale change. A cancelled gesture sends the opposite delta.

use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_pointer::WlPointer;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_pinch_v1::{self, ZwpPointerGesturePinchV1},
    zwp_pointer_gestures_v1::ZwpPointerGesturesV1,
};

use sctk::compositor::SurfaceData;
use sctk::globals::GlobalData;

use crate::event::{MouseScrollDelta, TouchPhase, WindowEvent};
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId, WindowId};

/// The line-delta base the diagram canvas uses for `1.2^y`.
const WHEEL_BASE: f64 = 1.2;

/// Manager for `zwp_pointer_gestures_v1`.
pub struct PointerGesturesState {
    manager: ZwpPointerGesturesV1,
}

#[derive(Debug)]
pub struct Pinch {
    gesture: ZwpPointerGesturePinchV1,
    /// Last absolute scale. The protocol starts a gesture at 1.
    scale: f64,
    window: Option<WindowId>,
}

impl PointerGesturesState {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=3, GlobalData)?;
        Ok(Self { manager })
    }

    pub fn pinch(&self, pointer: &WlPointer, queue_handle: &QueueHandle<WinitState>) -> Pinch {
        Pinch {
            gesture: self.manager.get_pinch_gesture(pointer, queue_handle, GlobalData),
            scale: 1.0,
            window: None,
        }
    }
}

impl Pinch {
    pub fn destroy(self) {
        self.gesture.destroy();
    }
}

impl Dispatch<ZwpPointerGesturesV1, GlobalData, WinitState> for PointerGesturesState {
    fn event(
        _state: &mut WinitState,
        _proxy: &ZwpPointerGesturesV1,
        _event: <ZwpPointerGesturesV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpPointerGesturePinchV1, GlobalData, WinitState> for PointerGesturesState {
    fn event(
        state: &mut WinitState,
        proxy: &ZwpPointerGesturePinchV1,
        event: <ZwpPointerGesturePinchV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin { surface, .. } => {
                let window = window_of(&surface);
                let known = state.windows.get_mut().contains_key(&window);
                if let Some(pinch) = pinch_mut(state, proxy) {
                    pinch.scale = 1.0;
                    pinch.window = known.then_some(window);
                }
            },
            zwp_pointer_gesture_pinch_v1::Event::Update { scale, .. } => {
                let step = {
                    let Some(pinch) = pinch_mut(state, proxy) else {
                        return;
                    };
                    let Some(window) = pinch.window else {
                        return;
                    };
                    let Some(y) = line_delta(pinch.scale, scale) else {
                        return;
                    };
                    pinch.scale = scale;
                    (window, y)
                };
                emit(state, step.0, Some(step.1));
            },
            zwp_pointer_gesture_pinch_v1::Event::End { cancelled, .. } => {
                let step = {
                    let Some(pinch) = pinch_mut(state, proxy) else {
                        return;
                    };
                    let window = pinch.window.take();
                    let scale = pinch.scale;
                    pinch.scale = 1.0;
                    (cancelled != 0).then_some((window, scale))
                };
                if let Some((Some(window), scale)) = step {
                    emit(state, window, line_delta(scale, 1.0));
                }
            },
            _ => {},
        }
    }
}

fn pinch_mut<'a>(
    state: &'a mut WinitState,
    proxy: &ZwpPointerGesturePinchV1,
) -> Option<&'a mut Pinch> {
    let id = proxy.id();
    state.seats.values_mut().find_map(|seat| {
        seat.pinch.as_mut().filter(|pinch| pinch.gesture.id() == id)
    })
}

fn window_of(surface: &WlSurface) -> WindowId {
    let parent = match surface.data::<SurfaceData>() {
        Some(data) => data.parent_surface().unwrap_or(surface),
        None => surface,
    };
    wayland::make_wid(parent)
}

fn emit(state: &mut WinitState, window: WindowId, y: Option<f32>) {
    let Some(y) = y.filter(|y| *y != 0.0) else {
        return;
    };
    let device_id = crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(DeviceId));
    state.events_sink.push_window_event(
        WindowEvent::MouseWheel {
            device_id,
            delta: MouseScrollDelta::LineDelta(0.0, y),
            phase: TouchPhase::Moved,
        },
        window,
    );
}

/// Wheel lines whose `WHEEL_BASE.powf(y)` equals `scale / previous`.
fn line_delta(previous: f64, scale: f64) -> Option<f32> {
    if !(previous.is_finite() && scale.is_finite() && previous > 0.0 && scale > 0.0) {
        return None;
    }
    let y = (scale / previous).ln() / WHEEL_BASE.ln();
    y.is_finite().then_some(y as f32)
}

delegate_dispatch!(WinitState: [ZwpPointerGesturePinchV1: GlobalData] => PointerGesturesState);
delegate_dispatch!(WinitState: [ZwpPointerGesturesV1: GlobalData] => PointerGesturesState);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_doubled_span_is_the_wheel_that_doubles() {
        let y = line_delta(1.0, 2.0).unwrap();
        let back = line_delta(2.0, 1.0).unwrap();
        let grown = WHEEL_BASE.powf(f64::from(y));
        let restored = grown * WHEEL_BASE.powf(f64::from(back));
        assert!((grown - 2.0).abs() < 1e-6);
        assert!((restored - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_flat_or_broken_scale_sends_nothing() {
        assert_eq!(line_delta(1.0, 1.0), Some(0.0));
        assert_eq!(line_delta(1.0, 0.0), None);
        assert_eq!(line_delta(1.0, f64::NAN), None);
        assert_eq!(line_delta(-1.0, 1.2), None);
    }
}
