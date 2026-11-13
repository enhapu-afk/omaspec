//! The editor's views, each a switch in the View menu kept in the
//! session: motion, JSON and YAML nested or as a tree, the outline beside
//! the source, state machines as tables, and colors that follow the theme.

use iced::Task;
use omaspec_core::Language;

use super::{App, Message};

impl App {
    pub(super) fn toggle_motion(&mut self) -> Task<Message> {
        self.look.motion = !self.look.motion;
        for doc in &mut self.docs {
            doc.set_look(&self.look);
        }
        self.notice = Some(if self.look.motion {
            "the caret glides and the list arrives".into()
        } else {
            "no motion in the editor".into()
        });
        self.keep_session();
        Task::none()
    }

    pub(super) fn toggle_json_layout(&mut self) -> Task<Message> {
        self.look.views.nested = !self.look.views.nested;
        self.notice = Some(if self.look.views.nested {
            "JSON and YAML drawn nested, each value in its cell".into()
        } else {
            "JSON and YAML drawn as a tree, as PlantUML draws them".into()
        });
        // Only what holds a JSON or a YAML diagram looks different.
        self.redraw_views(crate::worker::holds_tables)
    }

    pub(super) fn toggle_outline(&mut self) -> Task<Message> {
        self.outline_shown = !self.outline_shown;
        self.notice = Some(if self.outline_shown {
            "the outline stands beside the source; a click goes to its line".into()
        } else {
            "the outline is hidden".into()
        });
        self.keep_session();
        Task::none()
    }

    pub(super) fn toggle_state_table(&mut self) -> Task<Message> {
        self.look.views.table = !self.look.views.table;
        self.notice = Some(if self.look.views.table {
            "state machines shown as tables, states down, events across; Ctrl+click a cell for its line".into()
        } else {
            "state machines shown as diagrams".into()
        });
        self.redraw_views(crate::worker::holds_machine)
    }

    pub(super) fn toggle_follow_theme(&mut self) -> Task<Message> {
        self.follow_theme = !self.follow_theme;
        let key = self.doc().key;
        let root = self.place_of(key).map(|p| p.root);
        self.notice = Some(if self.pinned_look(root.as_deref()).is_some() {
            "this project pins its look in omaspec-project.toml; other diagrams follow the change"
                .into()
        } else if self.follow_theme {
            "diagram colors follow the Omarchy theme".into()
        } else {
            "diagram colors are the ones in the file".into()
        });
        self.keep_session();
        self.rerender_all();
        self.draw_thumbs()
    }

    /// Hands the look's views to every tab and the comparison, keeps them
    /// in the session, and redraws the tabs whose diagrams `affected`
    /// says look different.
    pub(super) fn redraw_views(&mut self, affected: fn(&str, Language) -> bool) -> Task<Message> {
        for doc in &mut self.docs {
            doc.set_look(&self.look);
        }
        self.keep_session();
        if let Some(compare) = &mut self.compare {
            compare.paper.views = self.look.views;
        }
        let keys: Vec<u64> = self
            .docs
            .iter()
            .filter(|doc| affected(&doc.text(), doc.current_language()))
            .map(|doc| doc.key)
            .collect();
        Task::batch(keys.into_iter().map(|key| self.render(key, true)))
    }
}
