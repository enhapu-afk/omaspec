//! Scrolling an area the least that shows one of its items whole: a tab in
//! the tab bar, a tile in the gallery.

/// Finds a scrolling area and one item in it as they are laid out, then
/// scrolls the area the least that shows the whole item: a tab in the tab
/// bar, a tile in the gallery.
pub(super) struct Reveal {
    pub(super) scroller: iced::widget::Id,
    pub(super) item: iced::widget::Id,
    /// Along the height; else along the width.
    pub(super) vertical: bool,
    /// The area's bounds, its content's bounds and how far it is scrolled.
    pub(super) bar: Option<(iced::Rectangle, iced::Rectangle, f32)>,
    pub(super) found: Option<iced::Rectangle>,
}

impl Reveal {
    pub(super) fn new(scroller: iced::widget::Id, item: iced::widget::Id, vertical: bool) -> Self {
        Self {
            scroller,
            item,
            vertical,
            bar: None,
            found: None,
        }
    }
}

impl<T: Send + 'static> iced::advanced::widget::Operation<T> for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<T>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
        if id == Some(&self.item) {
            self.found = Some(bounds);
        }
    }

    fn scrollable(
        &mut self,
        id: Option<&iced::widget::Id>,
        bounds: iced::Rectangle,
        content_bounds: iced::Rectangle,
        translation: iced::Vector,
        _state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
        if id == Some(&self.scroller) {
            let scrolled = if self.vertical {
                translation.y
            } else {
                translation.x
            };
            self.bar = Some((bounds, content_bounds, scrolled));
        }
    }

    fn finish(&self) -> iced::advanced::widget::operation::Outcome<T> {
        use iced::advanced::widget::operation::{Outcome, scrollable};
        let (Some((bar, content, scrolled)), Some(item)) = (self.bar, self.found) else {
            return Outcome::None;
        };
        let (size, left, length) = if self.vertical {
            (bar.height, item.y - content.y, item.height)
        } else {
            (bar.width, item.x - content.x, item.width)
        };
        match reveal_offset(size, scrolled, left, length) {
            Some(at) => Outcome::Chain(Box::new(scrollable::scroll_to(
                self.scroller.clone(),
                scrollable::AbsoluteOffset {
                    x: (!self.vertical).then_some(at),
                    y: self.vertical.then_some(at),
                },
            ))),
            None => Outcome::None,
        }
    }
}

/// Where a bar `width` wide, scrolled to `scrolled`, has to scroll to show
/// a tab from `left` to `left + tab` whole; `None` if it already does.
pub(super) fn reveal_offset(width: f32, scrolled: f32, left: f32, tab: f32) -> Option<f32> {
    if left < scrolled {
        Some(left.max(0.0))
    } else if left + tab > scrolled + width {
        Some((left + tab - width).max(0.0))
    } else {
        None
    }
}
