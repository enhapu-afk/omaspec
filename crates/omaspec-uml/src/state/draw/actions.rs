//! The look of an activity in PlantUML's old syntax, which the state
//! drawer draws ([`super::super::legacy`]): an action's lines start at the
//! left of its words, its box has its corners cut for `activityShape
//! octagon`, and an `if`'s test stands to the left above its diamond, in
//! room the layout keeps for it.

use super::{CHOICE, CHOICE_ROOM, Placed};
use crate::color::Paint;
use crate::graph::Rect;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::Block;

/// How far the test stands from the diamond's middle: its right edge
/// left of the line coming in at the top, its foot over the words on a
/// way out to the left.
const TEST_IN: f32 = 6.0;
const TEST_OVER: f32 = CHOICE / 2.0 + 4.0;

/// An action's lines, from the left of its words, as PlantUML sets them.
pub(super) fn from_left(title: &mut Block) {
    for line in &mut title.lines {
        line.flush = true;
    }
}

/// The room a choice with `test` beside it takes: as wide and as high,
/// about its middle, as the test reaches to the left and up, so the
/// layout keeps it clear of what stands around; a choice's own room where
/// it has none.
pub(super) fn test_room(test: &Block) -> (f32, f32) {
    if test.is_empty() {
        return (CHOICE_ROOM, CHOICE_ROOM);
    }
    (
        (2.0 * (test.width + TEST_IN)).max(CHOICE_ROOM),
        (2.0 * (test.height + TEST_OVER)).max(CHOICE_ROOM),
    )
}

/// The test of the choice `node`, in its room ([`test_room`]).
pub(super) fn paint_test(canvas: &mut Canvas, node: &Placed) {
    let (cx, cy) = (
        node.rect.x + node.rect.w / 2.0,
        node.rect.y + node.rect.h / 2.0,
    );
    let test = &node.item.title;
    canvas.text(
        Layer::Front,
        test,
        (cx - TEST_IN, cy - TEST_OVER - test.height),
        Align::Right,
    );
}

/// An action's box with its corners cut, as `activityShape octagon` asks.
pub(super) fn octagon(canvas: &mut Canvas, rect: Rect, fill: Paint, border: Stroke) {
    let cut = (rect.h / 4.0).min(rect.w / 4.0);
    let (l, t, r, b) = (rect.x, rect.y, rect.x + rect.w, rect.y + rect.h);
    canvas.polygon(
        Layer::Frames,
        &[
            (l + cut, t),
            (r - cut, t),
            (r, t + cut),
            (r, b - cut),
            (r - cut, b),
            (l + cut, b),
            (l, b - cut),
            (l, t + cut),
        ],
        fill,
        Some(border),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn a_test_takes_room_about_the_diamond() {
        assert_eq!(test_room(&Block::empty()), (CHOICE_ROOM, CHOICE_ROOM));
        let test = crate::text::block(
            "a rather long test",
            crate::text::Font::new(11.0, Color::BLACK),
            crate::text::Family::Sans,
        )
        .unwrap();
        let (w, h) = test_room(&test);
        assert!(w / 2.0 >= test.width + TEST_IN, "{w}");
        assert!(h / 2.0 >= test.height + TEST_OVER, "{h}");
    }
}
