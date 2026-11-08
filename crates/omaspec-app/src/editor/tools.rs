//! The toolbar over the diagram pane (docs/adr/0025): the handful of
//! commands the table marks as tools, as icons, each with a tooltip that
//! names the command and its key. It shows with the menu bar and has a
//! setting of its own in the View menu; the editor pane has none. The
//! diagram's own window carries it too, since it is the pane's.

use iced::widget::{button, container, row, tooltip};
use iced::{Alignment, Element, Length, Padding};

use super::table;
use super::{App, Command, Message};
use crate::icons::Icon;
use crate::ui;

/// The bar's height.
pub const HEIGHT: f32 = 28.0;
/// A tool's button.
const TOOL: f32 = 24.0;

/// The icon of a tool; every command the table marks as a tool has one.
pub fn icon_of(command: Command) -> Option<Icon> {
    Some(match command {
        Command::Fit => Icon::Fit,
        Command::ZoomIn => Icon::ZoomIn,
        Command::ZoomOut => Icon::ZoomOut,
        Command::ActualSize => Icon::ActualSize,
        Command::Play => Icon::Play,
        Command::UnfoldAll => Icon::Unfold,
        Command::CopyPng => Icon::Copy,
        Command::ExportSvg => Icon::Export,
        _ => return None,
    })
}

impl App {
    /// Whether the toolbar is drawn: with the menu bar, and when its own
    /// setting is on.
    pub(super) fn toolbar_shown(&self) -> bool {
        self.bar_shown && self.toolbar
    }

    /// The toolbar's setting flips; the session keeps it.
    pub(super) fn toggle_toolbar(&mut self) {
        self.toolbar = !self.toolbar;
        self.keep_session();
    }

    /// The toolbar: the tools in the table's order, the play lit while
    /// one runs.
    pub(super) fn toolbar(&self) -> Element<'_, Message> {
        let p = &self.palette;
        let playing = self.doc().paper.playing();
        let (lit, hover) = (
            ui::color(p.lighter_background),
            ui::alpha(p.lighter_background, 0.6),
        );
        let tools = table::tools().map(|entry| {
            let on = entry.command == Command::Play && playing;
            let ink = if on { p.accent } else { p.foreground };
            let icon = match icon_of(entry.command) {
                Some(icon) => self.icon(icon, ink),
                None => self.label(entry.menu_label(), ink).into(),
            };
            let tool = button(
                container(icon)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
            )
            .width(TOOL + 4.0)
            .height(TOOL)
            .padding(0)
            .on_press(Message::Command(entry.command))
            .style(move |_: &iced::Theme, status| {
                let background = if on {
                    Some(lit.into())
                } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                    Some(hover.into())
                } else {
                    None
                };
                button::Style {
                    background,
                    border: iced::Border {
                        radius: 5.0.into(),
                        ..Default::default()
                    },
                    ..button::Style::default()
                }
            });
            let tip = if entry.keys.is_empty() {
                entry.label.to_owned()
            } else {
                format!("{} · {}", entry.label, entry.keys)
            };
            let (fill, edge) = (ui::color(p.dark_background), ui::alpha(p.accent, 0.45));
            tooltip(
                tool,
                container(self.small(tip, p.foreground))
                    .padding([3, 8])
                    .style(move |_: &iced::Theme| {
                        container::Style::default()
                            .background(fill)
                            .border(iced::Border {
                                color: edge,
                                width: 1.0,
                                radius: 6.0.into(),
                            })
                    }),
                tooltip::Position::Bottom,
            )
            .gap(4)
            .into()
        });
        let bg = ui::color(p.dark_background);
        container(row(tools).spacing(2).align_y(Alignment::Center))
            .width(Length::Fill)
            .height(HEIGHT)
            .padding(Padding {
                top: 2.0,
                bottom: 2.0,
                left: 8.0,
                right: 8.0,
            })
            .style(move |_: &iced::Theme| container::Style::default().background(bg))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_an_icon() {
        for entry in table::tools() {
            assert!(icon_of(entry.command).is_some(), "{}", entry.label);
        }
        assert!(icon_of(Command::Save).is_none());
    }
}
