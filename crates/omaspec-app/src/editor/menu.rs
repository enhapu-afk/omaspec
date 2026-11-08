//! The menus for the mouse (docs/adr/0025): a bar of File, Edit, View,
//! Diagram, Project and Help over the tabs, and the context menu on the
//! right mouse button over the diagram or the editor. Both show the
//! commands the table places there, with their keys beside them, and run
//! them; a child menu (File › Export) opens to the right of its row.
//!
//! In the diagram the caret first lands on what is under the pointer, as
//! Ctrl+click puts it there, so the commands at the caret act on that box
//! or line. The arrows move the highlight (Left and Right between the
//! bar's menus, into a child menu and out of it), Enter runs, Escape and a
//! click beside the menu close it; any other key closes it too. The bar is
//! hidden by default on Omarchy: a tap on Alt shows and hides it, F10
//! opens File, and the session keeps it as it was left.

use iced::widget::{button, column, container, mouse_area, row, stack, text};
use iced::{Alignment, Element, Length, Padding, Point, Size, Task, window};

use super::table::{self, Entry, Menu, Row, Spot};
use super::{App, Command, Message};
use crate::ui;

/// A menu's width: the longest label with its keys fits.
const WIDTH: f32 = 380.0;
/// A child menu's width.
const CHILD_WIDTH: f32 = 300.0;
/// One row's height.
const ROW: f32 = 24.0;
/// A line between groups, with its room.
const GAP: f32 = 7.0;
/// The panel's padding on each side.
const PAD: f32 = 6.0;
/// The bar's height, and each root's width in it.
pub const BAR_H: f32 = 26.0;
const ROOT_W: f32 = 84.0;
/// The bar's padding at the left.
const BAR_PAD: f32 = 6.0;

/// A menu open in one window.
#[derive(Debug, Clone, PartialEq)]
pub struct Open {
    pub on: window::Id,
    pub kind: Kind,
    /// The row the keys or the mouse point at.
    pub pointed: Option<usize>,
    /// The child menu open, if a pointed row has one.
    pub child: Option<Child>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// The context menu, opened at `at` of its window.
    Context { spot: Spot, at: Point },
    /// A menu of the bar.
    Bar(Menu),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Child {
    /// The row of the parent whose child this is.
    pub row: usize,
    pub pointed: Option<usize>,
}

impl Open {
    pub fn rows(&self) -> Vec<Row> {
        match self.kind {
            Kind::Context { spot, .. } => table::context(spot).map(Row::Entry).collect(),
            Kind::Bar(menu) => table::rows(menu),
        }
    }

    /// The menu the bar has open, if this is one of the bar's.
    pub fn bar(&self) -> Option<Menu> {
        match self.kind {
            Kind::Bar(menu) => Some(menu),
            Kind::Context { .. } => None,
        }
    }

    /// The entries of the child menu open.
    fn child_entries(&self) -> Vec<&'static Entry> {
        let Some(child) = self.child else {
            return Vec::new();
        };
        match self.rows().into_iter().nth(child.row) {
            Some(Row::Child { entries, .. }) => entries,
            _ => Vec::new(),
        }
    }

    /// The command Enter runs: the pointed entry of the child menu open,
    /// else of the menu; `None` on a child's heading or nowhere.
    fn chosen(&self) -> Option<Command> {
        if let Some(child) = self.child {
            return child
                .pointed
                .and_then(|i| self.child_entries().get(i).map(|e| e.command));
        }
        match self.rows().into_iter().nth(self.pointed?) {
            Some(Row::Entry(e)) => Some(e.command),
            _ => None,
        }
    }

    /// Whether the pointed row is a child's heading, with the child closed.
    fn on_heading(&self) -> bool {
        self.child.is_none()
            && self
                .pointed
                .is_some_and(|i| matches!(self.rows().get(i), Some(Row::Child { .. })))
    }
}

/// The height of `rows` as drawn, with the panel's padding.
fn height(rows: &[Row]) -> f32 {
    offset(rows, rows.len()) + 2.0 * PAD
}

/// Where row `i` starts, from the top of the rows (after its own gap).
fn offset(rows: &[Row], i: usize) -> f32 {
    let before: f32 = rows
        .iter()
        .take(i)
        .enumerate()
        .map(|(j, r)| ROW + if r.gap() && j > 0 { GAP } else { 0.0 })
        .sum();
    let own = if rows.get(i).is_some_and(|r| r.gap() && i > 0) {
        GAP
    } else {
        0.0
    };
    before + own
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

/// Where the bar's menu `menu` hangs: under its root.
fn under_root(menu: Menu) -> Point {
    let index = Menu::ALL.iter().position(|m| *m == menu).unwrap_or(0);
    Point::new(BAR_PAD + ROOT_W * index as f32, BAR_H)
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
        self.open_menu(
            on,
            Kind::Context {
                spot: Spot::Diagram,
                at: in_window,
            },
        );
        landed
    }

    /// The context menu at `at` of the main window, over the editor.
    pub(super) fn context_at(&mut self, spot: Spot, at: Point) {
        let main = self.main;
        self.open_menu(main, Kind::Context { spot, at });
    }

    /// Opens `menu` of the bar; the bar shows for it. The menu already
    /// open closes instead.
    pub(super) fn open_bar_menu(&mut self, menu: Menu) {
        if self.menu.as_ref().and_then(Open::bar) == Some(menu) {
            self.close_menu();
            return;
        }
        self.bar_shown = true;
        let main = self.main;
        self.open_menu(main, Kind::Bar(menu));
    }

    /// The mouse came onto the root of `menu` while another menu of the
    /// bar is open: that one opens. The same root, or no menu open,
    /// changes nothing; the bar drawn over an open menu meets the pointer
    /// the moment it appears, and that must not close what was just
    /// clicked open.
    pub(super) fn switch_bar_menu(&mut self, menu: Menu) {
        if self
            .menu
            .as_ref()
            .and_then(Open::bar)
            .is_some_and(|open| open != menu)
        {
            self.open_bar_menu(menu);
        }
    }

    fn open_menu(&mut self, on: window::Id, kind: Kind) {
        self.close_picker();
        self.gallery = None;
        self.menu = Some(Open {
            on,
            kind,
            pointed: None,
            child: None,
        });
    }

    pub(super) fn close_menu(&mut self) {
        self.menu = None;
    }

    /// The bar shows or hides; the session keeps it.
    pub(super) fn toggle_menu_bar(&mut self) {
        self.bar_shown = !self.bar_shown;
        if !self.bar_shown && self.menu.as_ref().is_some_and(|m| m.bar().is_some()) {
            self.close_menu();
        }
        self.keep_session();
    }

    /// The mouse came onto row `i`: the highlight follows, and a child
    /// menu there opens.
    pub(super) fn menu_point(&mut self, i: usize) {
        let Some(open) = &mut self.menu else {
            return;
        };
        open.pointed = Some(i);
        open.child = match open.rows().get(i) {
            Some(Row::Child { .. }) => Some(Child {
                row: i,
                pointed: None,
            }),
            _ => None,
        };
    }

    /// The mouse came onto entry `i` of the child menu open.
    pub(super) fn menu_point_child(&mut self, i: usize) {
        if let Some(Open {
            child: Some(child), ..
        }) = &mut self.menu
        {
            child.pointed = Some(i);
        }
    }

    /// The highlight moves by `by` rows, around the ends, in the child menu
    /// when one is open.
    pub(super) fn menu_move(&mut self, by: isize) {
        let Some(open) = &mut self.menu else {
            return;
        };
        let step = |pointed: Option<usize>, n: usize| -> Option<usize> {
            if n == 0 {
                return None;
            }
            Some(match pointed {
                None if by < 0 => n - 1,
                None => 0,
                Some(i) => (i as isize + by).rem_euclid(n as isize) as usize,
            })
        };
        let in_child = open.child_entries().len();
        match &mut open.child {
            Some(child) => child.pointed = step(child.pointed, in_child),
            None => open.pointed = step(open.pointed, open.rows().len()),
        }
    }

    /// Right or Left: into the child menu of the pointed row and out of
    /// it, else to the next or the previous menu of the bar.
    pub(super) fn menu_side(&mut self, right: bool) {
        let Some(open) = &mut self.menu else {
            return;
        };
        if right && open.on_heading() {
            open.child = open.pointed.map(|row| Child {
                row,
                pointed: Some(0),
            });
            return;
        }
        if !right && open.child.take().is_some() {
            return;
        }
        if let Some(menu) = open.bar() {
            let n = Menu::ALL.len();
            let at = Menu::ALL.iter().position(|m| *m == menu).unwrap_or(0);
            let next = if right {
                (at + 1) % n
            } else {
                (at + n - 1) % n
            };
            open.kind = Kind::Bar(Menu::ALL[next]);
            open.pointed = None;
            open.child = None;
        }
    }

    /// Enter: runs the entry the highlight is on and closes the menu; on
    /// a child's heading it opens the child.
    pub(super) fn menu_choose(&mut self) -> Task<Message> {
        let Some(open) = &self.menu else {
            return Task::none();
        };
        if open.on_heading() {
            self.menu_side(true);
            return Task::none();
        }
        let command = open.chosen();
        self.close_menu();
        match command {
            Some(command) => self.run(command),
            None => Task::none(),
        }
    }

    /// The bar over the tabs: a root per menu; the open one is lit.
    pub(super) fn bar_view(&self) -> Element<'_, Message> {
        let p = &self.palette;
        let open = self.menu.as_ref().and_then(Open::bar);
        let roots = Menu::ALL.into_iter().map(|menu| {
            let lit = open == Some(menu);
            let ink = if lit {
                p.bright_foreground
            } else {
                p.foreground
            };
            let (chosen, hover) = (
                ui::color(p.lighter_background),
                ui::alpha(p.lighter_background, 0.6),
            );
            let root = button(
                container(self.label(menu.label(), ink))
                    .width(Length::Fill)
                    .align_x(Alignment::Center),
            )
            .width(ROOT_W)
            .height(BAR_H - 4.0)
            .padding(0)
            .on_press(Message::OpenMenu(menu))
            .style(move |_: &iced::Theme, status| {
                let background = if lit {
                    Some(chosen.into())
                } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    Some(hover.into())
                } else {
                    None
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..button::Style::default()
                }
            });
            // While a menu is open, the mouse coming onto another root
            // opens that one.
            match open {
                Some(_) => mouse_area(root).on_enter(Message::HoverMenu(menu)).into(),
                None => root.into(),
            }
        });
        let bg = ui::color(p.dark_background);
        container(row(roots).align_y(Alignment::Center))
            .width(Length::Fill)
            .height(BAR_H)
            .padding(Padding {
                top: 2.0,
                bottom: 2.0,
                left: BAR_PAD,
                right: BAR_PAD,
            })
            .style(move |_: &iced::Theme| container::Style::default().background(bg))
            .into()
    }

    /// `base` with the menu over it when the menu is `window`'s: a
    /// backdrop that closes it, the panel, and the child panel beside it.
    /// A menu of the bar draws the bar again on top, so its roots stay
    /// under the mouse.
    pub(super) fn with_menu<'a>(
        &'a self,
        window: window::Id,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let Some(open) = self.menu.as_ref().filter(|m| m.on == window) else {
            return base;
        };
        let rows = open.rows();
        let size = Size::new(WIDTH, height(&rows));
        let window_size = if window == self.main {
            self.main_size
        } else {
            self.detached_size
        };
        let at = match open.kind {
            Kind::Context { at, .. } => place(at, size, window_size),
            Kind::Bar(menu) => under_root(menu),
        };
        let backdrop = mouse_area(container(text("")).width(Length::Fill).height(Length::Fill))
            .on_press(Message::Command(Command::Cancel))
            .on_right_press(Message::Command(Command::Cancel));
        let panel = container(self.panel(self.menu_rows(open, &rows), WIDTH)).padding(Padding {
            top: at.y,
            left: at.x,
            ..Padding::ZERO
        });
        let mut layers = stack![base, backdrop];
        if open.bar().is_some() {
            layers = layers.push(self.bar_view());
        }
        layers = layers.push(panel);
        if let Some(child) = open.child {
            let entries = open.child_entries();
            let child_rows: Vec<Row> = entries.iter().copied().map(Row::Entry).collect();
            let child_size = Size::new(CHILD_WIDTH, height(&child_rows));
            // Its first row level with the heading, overlapping the edge.
            let beside = Point::new(at.x + WIDTH - PAD, at.y + offset(&rows, child.row));
            let child_at = place(beside, child_size, window_size);
            let panel = self.panel(self.child_rows(&entries, child.pointed), CHILD_WIDTH);
            layers = layers.push(container(panel).padding(Padding {
                top: child_at.y,
                left: child_at.x,
                ..Padding::ZERO
            }));
        }
        layers.into()
    }

    /// The rows of a menu: entries with their keys, child headings with a
    /// chevron, lines between the groups.
    fn menu_rows<'a>(&'a self, open: &Open, rows: &[Row]) -> Element<'a, Message> {
        let p = &self.palette;
        let mut col = column![];
        for (i, row) in rows.iter().enumerate() {
            if row.gap() && i > 0 {
                col = col.push(self.rule());
            }
            let chosen = open.pointed == Some(i);
            let button = match row {
                Row::Entry(entry) => self.entry(
                    self.entry_line(entry, chosen),
                    0,
                    chosen,
                    Message::MenuRun(entry.command),
                ),
                Row::Child { name, .. } => {
                    let ink = if chosen {
                        p.bright_foreground
                    } else {
                        p.foreground
                    };
                    let line = iced::widget::row![
                        container(self.label(*name, ink))
                            .width(Length::Fill)
                            .clip(true),
                        self.label("›", ui::quiet(p)),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center);
                    self.entry(line, 0, chosen, Message::MenuPoint(i))
                }
            };
            col =
                col.push(container(mouse_area(button).on_enter(Message::MenuPoint(i))).height(ROW));
        }
        col.into()
    }

    /// The rows of a child menu.
    fn child_rows<'a>(
        &'a self,
        entries: &[&'static Entry],
        pointed: Option<usize>,
    ) -> Element<'a, Message> {
        let mut col = column![];
        for (i, entry) in entries.iter().enumerate() {
            let chosen = pointed == Some(i);
            let button = self.entry(
                self.entry_line(entry, chosen),
                0,
                chosen,
                Message::MenuRun(entry.command),
            );
            col = col.push(
                container(mouse_area(button).on_enter(Message::MenuPointChild(i))).height(ROW),
            );
        }
        col.into()
    }

    /// An entry's label, and its keys at the right.
    fn entry_line<'a>(&'a self, entry: &Entry, chosen: bool) -> Element<'a, Message> {
        let p = &self.palette;
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
        row(parts).spacing(10).align_y(Alignment::Center).into()
    }

    /// A line between two groups of a menu.
    fn rule(&self) -> Element<'_, Message> {
        let quiet = ui::quiet(&self.palette);
        container(container(text("")).width(Length::Fill).height(1).style(
            move |_: &iced::Theme| container::Style::default().background(ui::alpha(quiet, 0.5)),
        ))
        .height(GAP)
        .padding(Padding {
            top: 3.0,
            bottom: 3.0,
            ..Padding::ZERO
        })
        .into()
    }

    /// The panel a menu's rows sit in.
    fn panel<'a>(&self, rows: Element<'a, Message>, width: f32) -> Element<'a, Message> {
        let p = &self.palette;
        let (fill, edge) = (ui::color(p.dark_background), ui::alpha(p.accent, 0.45));
        container(rows)
            .width(Length::Fixed(width))
            .padding(PAD)
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

    #[test]
    fn rows_are_measured_as_drawn() {
        let rows = table::rows(Menu::File);
        let gaps = rows.iter().skip(1).filter(|r| r.gap()).count();
        assert_eq!(
            height(&rows),
            rows.len() as f32 * ROW + gaps as f32 * GAP + 2.0 * PAD
        );
        // The first row starts at the top; a row after a gap starts past it.
        assert_eq!(offset(&rows, 0), 0.0);
        let after_gap = rows.iter().position(|r| r.gap()).unwrap();
        assert_eq!(offset(&rows, after_gap), after_gap as f32 * ROW + GAP);
    }
}
