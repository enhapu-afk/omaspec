//! A find in the diagram (Ctrl+F where it has the keyboard): the labels
//! it shows, and the one chosen lit as the caret's name is, the view
//! gliding to it when it is out of sight.

use iced::{Point, Rectangle, Size, Task};
use omaspec_core::locate;
use omaspec_render::pick;

use super::view::View;
use super::{Message, Paper};

impl Paper {
    /// Every label the diagram shows, each with how many before it read
    /// the same, and the source's line that writes it: what a find in the
    /// diagram chooses from.
    pub fn labels(&self) -> Vec<(String, usize, Option<usize>)> {
        let (Some(tree), Some(diagram)) = (self.shown_tree(), &self.diagram) else {
            return Vec::new();
        };
        let mut seen: Vec<String> = Vec::new();
        pick::texts(tree)
            .into_iter()
            .filter(|text| !text.trim().is_empty())
            .map(|text| {
                let nth = seen.iter().filter(|s| **s == text).count();
                seen.push(text.clone());
                let line = locate::line_of(&diagram.source, &text, nth);
                (text, nth, line)
            })
            .collect()
    }

    /// Lights the `nth` label that reads `text`, and glides the view to it
    /// when it is not in sight.
    pub fn light_label(&mut self, text: &str, nth: usize) -> Task<Message> {
        let Some(seat) = self
            .shown_tree()
            .and_then(|tree| pick::seat(tree, text, nth))
        else {
            return Task::none();
        };
        let (x, y, w, h) = seat.bounds();
        let place = Rectangle::new(Point::new(x, y), Size::new(w, h));
        let aim = match seat {
            pick::Seat::Shape { .. } => super::breath::Aim::shape(place),
            pick::Seat::Words { ink, ground, .. } => super::breath::Aim::words(place, ink, ground),
        };
        self.set_breath(Some(aim));
        self.found = true;
        let visible = self.view.visible(self.viewport);
        let inside = visible.x <= place.x
            && visible.y <= place.y
            && place.x + place.width <= visible.x + visible.width
            && place.y + place.height <= visible.y + visible.height;
        if inside || self.viewport == Size::ZERO {
            return Task::none();
        }
        let to = View {
            center: place.center(),
            fit: false,
            ..self.view
        };
        let middle = Point::new(self.viewport.width / 2.0, self.viewport.height / 2.0);
        self.glide_to(to, middle)
    }
}
