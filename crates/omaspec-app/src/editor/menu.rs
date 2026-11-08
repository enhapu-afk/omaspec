//! The context menu (docs/adr/0025): the right mouse button over the
//! diagram or the editor opens the commands the table places there, at
//! the pointer. In the diagram the caret first lands on what is under the
//! pointer, as Ctrl+click puts it there, so the commands at the caret act
//! on that box or line. The arrows move the highlight, Enter runs, Escape
//! and a click beside the menu close it; any other key closes it too.

use iced::widget::{column, container, mouse_area, row, stack, text};
use iced::{Alignment, Element, Length, Padding, Point, Size, Task, window};

use super::table::{self, Entry, Spot};
use super::{App, Command, Message};
use crate::ui;

/// The menu's width: the longest label with its keys fits.
const WIDTH: f32 = 380.0;
/// One entry's height, for keeping the menu inside the window.
const ROW: f32 = 24.0;
/// A line between groups, with its room.
const GAP: f32 = 7.0;
/// The panel's padding, top and bottom together.
const PADDING: f32 = 12.0;

/// A context menu open in one window.
#[derive(Debug, Clone, PartialEq)]
pub struct Context {
    pub on: window::Id,
    pub spot: Spot,
    /// Where it was opened, in the window.
    pub at: Point,
    /// The entry the keys or the mouse point at, as an index into
    /// [`Context::entries`].
    pub pointed: Option<usize>,
}

impl Context {
    pub fn entries(&self) -> Vec<&'static Entry> {
        table::context(self.spot).collect()
    }

    /// The menu's height, estimated from its entries.
    fn height(&self) -> f32 {
        let entries = self.entries();
        let gaps = entries.iter().skip(1).filter(|e| e.gap).count();
        entries.len() as f32 * ROW + gaps as f32 * GAP + PADDING
    }
}

/// Where a menu of `size` opens for a click at `at` in a window of
/// `window`: at the pointer, or moved left or up as far as needed to stay
/// inside, never past the window's top left.
pub fn place(at: Point, size: Size, window: Size) -> Point {
    let x = if at.x + size.width > window.width {
        (window.width - size.width).max(0.0)
    } else {
        at.x
    };
    let y = if at.y + size.height > window.height {
        (at.y - size.height).max(0.0)
    } else {
        at.y
    };
    Point::new(x, y)
}

impl App {
    /// The right mouse button at `at` of the diagram pane, which is at
    /// `in_window` of its window: the caret lands on what is there, and
    /// the diagram's menu opens.
    pub(super) fn context_in_diagram(
        &mut self,
        key: u64,
        at: Point,
        in_window: Point,
    ) -> Task<Message> {
        if key != self.doc().key || self.doc().paper.playing() {
            return Task::none();
        }
        let landed = self.land_on(at).unwrap_or_else(|_| Task::none());
        let on = self.detached.unwrap_or(self.main);
        self.open_context(on, Spot::Diagram, in_window);
        landed
    }

    /// A context menu at `at` of window `on`.
    pub(super) fn open_context(&mut self, on: window::Id, spot: Spot, at: Point) {
        self.close_picker();
        self.gallery = None;
        self.context = Some(Context {
            on,
            spot,
            at,
            pointed: None,
        });
    }

    pub(super) fn close_context(&mut self) {
        self.context = None;
    }

    /// The highlight moves by `by` entries, around the ends.
    pub(super) fn context_move(&mut self, by: isize) {
        let Some(context) = &mut self.context else {
            return;
        };
        let n = context.entries().len();
        if n == 0 {
            return;
        }
        context.pointed = Some(match context.pointed {
            None if by < 0 => n - 1,
            None => 0,
            Some(i) => (i as isize + by).rem_euclid(n as isize) as usize,
        });
    }

    /// Runs the entry the highlight is on and closes the menu.
    pub(super) fn context_choose(&mut self) -> Task<Message> {
        let command = self.context.as_ref().and_then(|c| {
            c.pointed
                .and_then(|i| c.entries().get(i).map(|e| e.command))
        });
        self.close_context();
        match command {
            Some(command) => self.run(command),
            None => Task::none(),
        }
    }

    /// `base` with the context menu over it when the menu is `window`'s.
    pub(super) fn with_context<'a>(
        &'a self,
        window: window::Id,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(context) = self.context.as_ref().filter(|c| c.on == window) else {
            return base;
        };
        let size = Size::new(WIDTH, context.height());
        let window_size = if window == self.main {
            self.main_size
        } else {
            self.detached_size
        };
        let at = place(context.at, size, window_size);
        let backdrop = mouse_area(container(text("")).width(Length::Fill).height(Length::Fill))
            .on_press(Message::Command(Command::Cancel))
            .on_right_press(Message::Command(Command::Cancel));
        let panel = container(self.context_view(context)).padding(Padding {
            top: at.y,
            left: at.x,
            ..Padding::ZERO
        });
        stack![base, backdrop, panel].into()
    }

    fn context_view<'a>(&'a self, context: &Context) -> Element<'a, Message> {
        let p = &self.palette;
        let quiet = ui::quiet(p);
        let mut rows = column![].spacing(1);
        for (i, entry) in context.entries().into_iter().enumerate() {
            if entry.gap && i > 0 {
                rows = rows.push(container(text("")).width(Length::Fill).height(1).style(
                    move |_: &iced::Theme| {
                        container::Style::default().background(ui::alpha(quiet, 0.5))
                    },
                ));
                rows = rows.push(container(text("")).height(GAP - 3.0));
            }
            let chosen = context.pointed == Some(i);
            let ink = if chosen {
                p.bright_foreground
            } else {
                p.foreground
            };
            let mut parts: Vec<Element<'a, Message>> = vec![
                container(self.label(entry.menu_label(), ink))
                    .width(Length::Fill)
                    .clip(true)
                    .into(),
            ];
            if !entry.keys.is_empty() {
                parts.push(self.keys(entry.keys));
            }
            let line = row(parts).spacing(10).align_y(Alignment::Center);
            let button = self.entry(line, 0, chosen, Message::MenuRun(entry.command));
            rows = rows.push(mouse_area(button).on_enter(Message::MenuPoint(i)));
        }
        let (fill, edge) = (ui::color(p.dark_background), ui::alpha(p.accent, 0.45));
        container(rows)
            .width(Length::Fixed(WIDTH))
            .padding(6)
            .style(move |_: &iced::Theme| {
                container::Style::default()
                    .background(fill)
                    .border(iced::Border {
                        color: edge,
                        width: 1.0,
                        radius: 10.0.into(),
                    })
                    .shadow(iced::Shadow {
                        color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                        offset: iced::Vector::new(0.0, 6.0),
                        blur_radius: 20.0,
                    })
            })
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_stays_inside_the_window() {
        let window = Size::new(1000.0, 600.0);
        let size = Size::new(200.0, 300.0);
        // Room enough: at the pointer.
        assert_eq!(
            place(Point::new(100.0, 100.0), size, window),
            Point::new(100.0, 100.0)
        );
        // Near the right edge: moved left to the edge.
        assert_eq!(
            place(Point::new(900.0, 100.0), size, window),
            Point::new(800.0, 100.0)
        );
        // Near the bottom: opens upward from the pointer.
        assert_eq!(
            place(Point::new(100.0, 500.0), size, window),
            Point::new(100.0, 200.0)
        );
        // A window too small for it: the top left, not beyond.
        assert_eq!(
            place(Point::new(10.0, 10.0), size, Size::new(100.0, 100.0)),
            Point::ORIGIN
        );
    }
}
