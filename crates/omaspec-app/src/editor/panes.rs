//! The areas of the window: the project tree, the source and the diagram,
//! the diagram in a window of its own, and which area has the keyboard.

use iced::animation::Easing;
use iced::time::Instant;
use iced::widget::pane_grid;
use iced::{Animation, Task, window};

use super::{App, Message, PICKER, Pane, SIDEBAR_SHARE, TAB_FADE, blur, compare, layout};

impl App {
    pub(super) fn pane(&self, wanted: Pane) -> Option<pane_grid::Pane> {
        self.panes
            .iter()
            .find(|(_, pane)| **pane == wanted)
            .map(|(id, _)| *id)
    }

    /// Moves the diagram into a window of its own; the editor gets its
    /// room.
    pub(super) fn detach(&mut self) -> Task<Message> {
        let (id, open) = window::open(window::Settings {
            icon: crate::logo::icon(),
            platform_specific: window::settings::PlatformSpecific {
                application_id: "omaspec-preview".into(),
                ..Default::default()
            },
            exit_on_close_request: false,
            ..Default::default()
        });
        self.detached = Some(id);
        if self.focus == Pane::Diagram {
            self.set_focus(Pane::Source);
        }
        self.panes = layout(self.sidebar_open(), false);
        let scale =
            open.then(|id| window::scale_factor(id).map(move |f| Message::ScaleFactor(id, f)));
        // The compositor often gives this window less room once the diagram
        // has its own. The active tab's place in the row survives that.
        Task::batch([scale, self.apply_keyboard_focus(), self.reveal_tab()])
    }

    /// Closes the diagram's window; the diagram is back beside the source.
    pub(super) fn attach(&mut self) -> Task<Message> {
        let Some(window) = self.detached.take() else {
            return Task::none();
        };
        // The list was drawn in the window that is closing.
        let picker_back = if self.picker_on == Some(window) {
            self.picker_on = Some(self.main);
            Task::batch([
                window::gain_focus(self.main),
                iced::widget::operation::focus(PICKER),
            ])
        } else {
            Task::none()
        };
        self.panes = layout(self.sidebar_open(), true);
        let factor = self.main_scale;
        let key = self.doc().key;
        let a = self
            .doc_mut()
            .paper
            .set_scale_factor(factor)
            .map(move |m| Message::Paper(key, m));
        let b = match &mut self.compare {
            Some(c) => c
                .paper
                .set_scale_factor(factor)
                .map(|m| Message::Paper(compare::KEY, m)),
            None => Task::none(),
        };
        Task::batch([
            window::close(window),
            a,
            b,
            self.apply_keyboard_focus(),
            picker_back,
        ])
    }

    /// Hides the project tree, or shows it again left of the source.
    pub(super) fn toggle_sidebar(&mut self) {
        if self.sidebar_open() {
            if self.focus == Pane::Files {
                self.set_focus(Pane::Source);
            }
            if let Some(files) = self.pane(Pane::Files) {
                self.panes.close(files);
            }
            return;
        }
        let Some(source) = self.pane(Pane::Source) else {
            return;
        };
        let Some((files, split)) = self
            .panes
            .split(pane_grid::Axis::Vertical, source, Pane::Files)
        else {
            return;
        };
        self.panes.swap(files, source);
        let share = if self.detached.is_some() {
            SIDEBAR_SHARE / 2.0
        } else {
            SIDEBAR_SHARE
        };
        self.panes.resize(split, share);
    }

    pub(super) fn focus_editor(&self) -> Task<Message> {
        iced::widget::operation::focus(format!("editor-{}", self.doc().key))
    }

    /// The editor takes the keys in its own area. Anywhere else they would
    /// still change the source, so the caret gives way.
    pub(super) fn apply_keyboard_focus(&self) -> Task<Message> {
        if self.prompt.is_some() || self.picker.is_some() {
            return Task::none();
        }
        if self.focus == Pane::Source {
            self.focus_editor()
        } else {
            blur()
        }
    }

    pub(super) fn sidebar_open(&self) -> bool {
        self.pane(Pane::Files).is_some()
    }

    pub(super) fn kind_of(&self, id: pane_grid::Pane) -> Option<Pane> {
        self.panes
            .iter()
            .find(|(pane, _)| **pane == id)
            .map(|(_, kind)| *kind)
    }

    /// A click names the area. The widget that was clicked keeps the
    /// keyboard, so opening a file still lands in the editor.
    pub(super) fn focus_pane(&mut self, id: pane_grid::Pane) -> Task<Message> {
        if let Some(kind) = self.kind_of(id) {
            self.set_focus(kind);
        }
        Task::none()
    }

    /// Names `pane` the focused area; its frame fades to the accent while
    /// the one before fades back.
    pub(super) fn set_focus(&mut self, pane: Pane) {
        if pane != self.focus {
            self.left_focus = Some(self.focus);
            self.focus_fade = Animation::new(false)
                .duration(TAB_FADE)
                .easing(Easing::EaseOut)
                .go(true, Instant::now());
        }
        self.focus = pane;
    }

    pub(super) fn move_focus(&mut self, direction: pane_grid::Direction) -> Task<Message> {
        let Some(current) = self.pane(self.focus).or_else(|| self.pane(Pane::Source)) else {
            return Task::none();
        };
        let Some(next) = self.panes.adjacent(current, direction) else {
            return Task::none();
        };
        let Some(kind) = self.kind_of(next) else {
            return Task::none();
        };
        self.set_focus(kind);
        self.apply_keyboard_focus()
    }
}
