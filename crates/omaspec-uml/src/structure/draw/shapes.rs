//! The outlines of a person and of a queue, as PlantUML's `person` and
//! `queue` draw them, and where their words go.

use super::{Rect, Stroke};
use crate::color::Paint;
use crate::svg::{Canvas, Layer};

/// The head over a person's box: its diameter, and how far it reaches
/// into the box.
pub(super) const HEAD: f32 = 26.0;
const HEAD_IN: f32 = 4.0;
/// How far a queue's round ends bow out.
pub(super) const BOW: f32 = 10.0;

/// The room above a person's box its head takes.
pub(super) fn head_room() -> f32 {
    HEAD - HEAD_IN
}

/// A person: a round head over a box rounder than the others.
pub(super) fn paint_person(canvas: &mut Canvas, rect: Rect, fill: Paint, stroke: Stroke) {
    let body = body_of(rect);
    let round = (body.h / 2.5).min(body.w / 4.0).clamp(8.0, 24.0);
    let r = HEAD / 2.0;
    canvas.ellipse(
        Layer::Frames,
        (rect.x + rect.w / 2.0, rect.y + r),
        (r, r),
        fill,
        Some(stroke),
    );
    canvas.rect(
        Layer::Frames,
        (body.x, body.y, body.w, body.h),
        round,
        fill,
        Some(stroke),
    );
}

/// The box of a person, under its head.
pub(super) fn body_of(rect: Rect) -> Rect {
    let top = head_room();
    Rect {
        x: rect.x,
        y: rect.y + top,
        w: rect.w,
        h: (rect.h - top).max(1.0),
    }
}

/// A queue: a cylinder on its side, its round end facing right; drawn
/// as the upright one is, turned.
pub(super) fn paint_queue(canvas: &mut Canvas, rect: Rect, fill: Paint, stroke: Stroke) {
    let rx = BOW.min(rect.w * 0.2).max(1.0);
    let ry = rect.h / 2.0;
    let cy = rect.y + ry;
    let (left, right) = (rect.x + rx, rect.x + rect.w - rx);
    canvas.ellipse(Layer::Frames, (left, cy), (rx, ry), fill, Some(stroke));
    if right > left {
        canvas.plain(|canvas| {
            canvas.rect(
                Layer::Frames,
                (left, rect.y, right - left, rect.h),
                0.0,
                fill,
                None,
            );
        });
        canvas.line(Layer::Frames, (left, rect.y), (right, rect.y), stroke);
        let bottom = rect.y + rect.h;
        canvas.line(Layer::Frames, (left, bottom), (right, bottom), stroke);
    }
    canvas.ellipse(Layer::Frames, (right, cy), (rx, ry), fill, Some(stroke));
}

/// The words of a queue stay between its round ends.
pub(super) fn queue_area(rect: Rect) -> Rect {
    let rx = BOW.min(rect.w * 0.2).max(1.0);
    Rect {
        x: rect.x + rx,
        y: rect.y,
        w: (rect.w - 3.0 * rx).max(1.0),
        h: rect.h,
    }
}
