//! Where the diagram sits in the window: zoom and the diagram point at the
//! center of the viewport. Pure geometry, so it is tested on its own.

use std::time::Duration;

use iced::time::Instant;
use iced::{Point, Rectangle, Size, Vector};

pub const MIN_ZOOM: f32 = 0.05;
pub const MAX_ZOOM: f32 = 16.0;
/// Space around the diagram when it is fitted.
const FIT_MARGIN: f32 = 24.0;
/// Closer than this, a two-finger span is not a pinch.
const MIN_PINCH_SPAN: f32 = 1.0;
/// How long a stepped zoom glides.
pub const GLIDE: Duration = Duration::from_millis(160);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub zoom: f32,
    /// The diagram point (in diagram units) shown at the viewport center.
    pub center: Point,
    /// Follow the window size: refit after every resize and new diagram.
    pub fit: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            center: Point::ORIGIN,
            fit: true,
        }
    }
}

impl View {
    /// The whole diagram in the viewport, never enlarged beyond 100 %.
    pub fn fitted(diagram: Size, viewport: Size) -> Self {
        let room = Size::new(
            (viewport.width - 2.0 * FIT_MARGIN).max(1.0),
            (viewport.height - 2.0 * FIT_MARGIN).max(1.0),
        );
        let zoom = (room.width / diagram.width)
            .min(room.height / diagram.height)
            .clamp(MIN_ZOOM, 1.0);
        Self {
            zoom,
            center: Point::new(diagram.width / 2.0, diagram.height / 2.0),
            fit: true,
        }
    }

    /// Diagram at 100 %, keeping the current center.
    pub fn actual_size(self) -> Self {
        Self {
            zoom: 1.0,
            fit: false,
            ..self
        }
    }

    /// Zooms by `factor`, keeping the diagram point under `anchor` (relative
    /// to the viewport's top left) where it is.
    pub fn zoom_at(self, factor: f32, anchor: Point, viewport: Size) -> Self {
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let from_center = anchor - Point::new(viewport.width / 2.0, viewport.height / 2.0);
        // The diagram point under the anchor stays under it.
        let under = self.center + from_center * (1.0 / self.zoom);
        Self {
            zoom,
            center: under - from_center * (1.0 / zoom),
            fit: false,
        }
    }

    /// Pans by `pan`, then zooms by `factor` at `at`. A touchscreen pinch:
    /// the midpoint moved, and the fingers moved apart or together.
    pub fn pinch(self, factor: f32, at: Point, pan: Vector, viewport: Size) -> Self {
        self.pan(pan).zoom_at(factor, at, viewport)
    }

    /// Moves the content by `delta` screen pixels.
    pub fn pan(self, delta: Vector) -> Self {
        Self {
            center: self.center - delta * (1.0 / self.zoom),
            fit: false,
            ..self
        }
    }

    /// The diagram's rectangle on screen, within `bounds`.
    pub fn diagram_rect(self, diagram: Size, bounds: Rectangle) -> Rectangle {
        self.on_screen(Rectangle::with_size(diagram), bounds)
    }

    /// Where `area` of the diagram (in diagram units) is on screen, within
    /// `bounds`.
    pub fn on_screen(self, area: Rectangle, bounds: Rectangle) -> Rectangle {
        let top_left = bounds.center() + (area.position() - self.center) * self.zoom;
        Rectangle::new(top_left, area.size() * self.zoom)
    }

    /// The diagram point (in diagram units) at `at`, a point of a viewport
    /// of this size.
    pub fn to_diagram(self, at: Point, viewport: Size) -> Point {
        let from_center = at - Point::new(viewport.width / 2.0, viewport.height / 2.0);
        self.center + from_center * (1.0 / self.zoom)
    }

    /// Midpoint and distance of two fingers, when they are far enough apart
    /// to pinch.
    pub fn pinch_sample(a: Point, b: Point) -> Option<(Point, f32)> {
        let span = a.distance(b);
        if span.is_finite() && span >= MIN_PINCH_SPAN {
            Some((Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5), span))
        } else {
            None
        }
    }

    /// The area of the diagram (in diagram units) a viewport of this size
    /// shows.
    pub fn visible(self, viewport: Size) -> Rectangle {
        let size = viewport * (1.0 / self.zoom);
        Rectangle::new(
            self.center - Vector::new(size.width / 2.0, size.height / 2.0),
            size,
        )
    }
}

/// A view on its way to another: a stepped zoom (a wheel notch, a key,
/// fit, 100 %) glides there instead of jumping. The zoom grows by the same
/// factor in each moment, and the diagram point under the anchor moves
/// straight from where it was to where it ends, so a zoom at the pointer
/// keeps that point under it all the way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glide {
    from: View,
    to: View,
    /// The anchor, from the viewport's center.
    anchor: Vector,
    start: Instant,
}

impl Glide {
    /// From `from` to `to` around `anchor` (relative to the viewport's top
    /// left), starting at `start`.
    pub fn new(from: View, to: View, anchor: Point, viewport: Size, start: Instant) -> Self {
        Self {
            from,
            to,
            anchor: anchor - Point::new(viewport.width / 2.0, viewport.height / 2.0),
            start,
        }
    }

    pub fn done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= GLIDE
    }

    /// The view shown at `now`.
    pub fn at(&self, now: Instant) -> View {
        let t = now.saturating_duration_since(self.start).as_secs_f32() / GLIDE.as_secs_f32();
        if t >= 1.0 {
            return self.to;
        }
        // Quick at first, settling at the end.
        let t = 1.0 - (1.0 - t).powi(3);
        let (from, to) = (self.from, self.to);
        let zoom = from.zoom * (to.zoom / from.zoom).powf(t);
        let start = from.center + self.anchor * (1.0 / from.zoom);
        let end = to.center + self.anchor * (1.0 / to.zoom);
        let under = start + (end - start) * t;
        View {
            zoom,
            center: under - self.anchor * (1.0 / zoom),
            fit: to.fit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_glide_keeps_the_point_under_the_pointer() {
        let from = View {
            zoom: 1.0,
            center: Point::new(200.0, 100.0),
            fit: false,
        };
        let at = Point::new(100.0, 60.0);
        let to = from.zoom_at(2.0, at, VIEWPORT);
        let start = Instant::now();
        let glide = Glide::new(from, to, at, VIEWPORT, start);
        let under = |v: View| {
            v.center
                + (at - Point::new(VIEWPORT.width / 2.0, VIEWPORT.height / 2.0)) * (1.0 / v.zoom)
        };
        let fixed = under(from);
        assert_eq!(glide.at(start), from);
        for ms in [20, 60, 100, 140] {
            let v = glide.at(start + Duration::from_millis(ms));
            assert!(v.zoom > from.zoom && v.zoom < to.zoom, "{ms}: {}", v.zoom);
            let p = under(v);
            assert!(
                (p.x - fixed.x).abs() < 1e-3 && (p.y - fixed.y).abs() < 1e-3,
                "{ms}: {p:?}"
            );
        }
        assert!(glide.done(start + GLIDE));
        assert_eq!(glide.at(start + GLIDE), to);
    }

    const DIAGRAM: Size = Size::new(400.0, 200.0);
    const VIEWPORT: Size = Size::new(848.0, 448.0);

    #[test]
    fn fit_never_enlarges() {
        let view = View::fitted(DIAGRAM, VIEWPORT);
        assert_eq!(view.zoom, 1.0);
        assert_eq!(view.center, Point::new(200.0, 100.0));
        let small = View::fitted(DIAGRAM, Size::new(248.0, 448.0));
        assert_eq!(small.zoom, 0.5);
    }

    #[test]
    fn fitted_diagram_is_centered() {
        let bounds = Rectangle::new(Point::ORIGIN, VIEWPORT);
        let rect = View::fitted(DIAGRAM, VIEWPORT).diagram_rect(DIAGRAM, bounds);
        assert_eq!(rect, Rectangle::new(Point::new(224.0, 124.0), DIAGRAM));
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let bounds = Rectangle::new(Point::ORIGIN, VIEWPORT);
        let view = View::fitted(DIAGRAM, VIEWPORT);
        let anchor = Point::new(300.0, 150.0);
        let before = view.diagram_rect(DIAGRAM, bounds);
        let zoomed = view.zoom_at(2.0, anchor, VIEWPORT);
        let after = zoomed.diagram_rect(DIAGRAM, bounds);
        // The same diagram point maps to the anchor before and after.
        let point = |r: Rectangle, z: f32| Point::new((anchor.x - r.x) / z, (anchor.y - r.y) / z);
        let (a, b) = (point(before, view.zoom), point(after, zoomed.zoom));
        assert!((a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3);
        assert!(!zoomed.fit);
    }

    #[test]
    fn the_visible_area_maps_onto_the_viewport() {
        let bounds = Rectangle::new(Point::ORIGIN, VIEWPORT);
        let view = View::fitted(DIAGRAM, VIEWPORT).zoom_at(2.5, Point::new(100.0, 80.0), VIEWPORT);
        let back = view.on_screen(view.visible(VIEWPORT), bounds);
        assert!((back.x).abs() < 1e-3 && (back.y).abs() < 1e-3);
        assert!((back.width - VIEWPORT.width).abs() < 1e-3);
        assert!((back.height - VIEWPORT.height).abs() < 1e-3);
    }

    #[test]
    fn screen_points_map_back_to_the_diagram() {
        let viewport = Size::new(800.0, 600.0);
        let view = View::fitted(Size::new(400.0, 300.0), viewport)
            .zoom_at(2.5, Point::new(100.0, 50.0), viewport)
            .pan(Vector::new(30.0, -20.0));
        let point = Point::new(123.0, 45.0);
        let screen = view.on_screen(
            Rectangle::new(point, Size::ZERO),
            Rectangle::with_size(viewport),
        );
        let back = view.to_diagram(screen.position(), viewport);
        assert!((back.x - point.x).abs() < 1e-3 && (back.y - point.y).abs() < 1e-3);
    }

    #[test]
    fn zoom_is_limited() {
        let view = View::default().zoom_at(1000.0, Point::ORIGIN, VIEWPORT);
        assert_eq!(view.zoom, MAX_ZOOM);
    }

    #[test]
    fn a_pinch_keeps_the_midpoint_and_pans_with_it() {
        let view = View::fitted(DIAGRAM, VIEWPORT);
        let at = Point::new(300.0, 160.0);
        let before = view.to_diagram(at, VIEWPORT);
        let zoomed = view.pinch(2.0, at, Vector::ZERO, VIEWPORT);
        let after = zoomed.to_diagram(at, VIEWPORT);
        assert!((after.x - before.x).abs() < 1e-3 && (after.y - before.y).abs() < 1e-3);
        assert!((zoomed.zoom - view.zoom * 2.0).abs() < 1e-3);

        let moved = Point::new(340.0, 140.0);
        let panned = view.pinch(1.0, moved, moved - at, VIEWPORT);
        let followed = panned.to_diagram(moved, VIEWPORT);
        assert!((followed.x - before.x).abs() < 1e-3 && (followed.y - before.y).abs() < 1e-3);
    }

    #[test]
    fn fingers_closer_than_a_pixel_are_not_a_pinch() {
        assert!(View::pinch_sample(Point::ORIGIN, Point::new(0.4, 0.0)).is_none());
        let (mid, span) =
            View::pinch_sample(Point::new(10.0, 20.0), Point::new(10.0, 40.0)).unwrap();
        assert_eq!(mid, Point::new(10.0, 30.0));
        assert!((span - 20.0).abs() < 1e-3);
    }

    #[test]
    fn pan_moves_the_content_with_the_pointer() {
        let bounds = Rectangle::new(Point::ORIGIN, VIEWPORT);
        let view = View::fitted(DIAGRAM, VIEWPORT).zoom_at(2.0, Point::ORIGIN, VIEWPORT);
        let before = view.diagram_rect(DIAGRAM, bounds);
        let after = view
            .pan(Vector::new(30.0, -10.0))
            .diagram_rect(DIAGRAM, bounds);
        assert!((after.x - before.x - 30.0).abs() < 1e-3);
        assert!((after.y - before.y + 10.0).abs() < 1e-3);
    }
}
