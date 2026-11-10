//! Folding containers and the focus on a box, from the keyboard
//! (docs/adr/0024): the box whose name the caret is on.

use iced::Task;

use super::picker::{Choice, Item, Kind, Picker};
use super::{App, Message, PICKER};

impl App {
    /// The source on screen and where the caret is in it.
    fn caret_in_source(&self) -> (String, usize, usize) {
        let doc = self.doc();
        let cursor = doc.editor.cursor();
        (doc.shown_source(), cursor.row as usize, cursor.col as usize)
    }

    /// Focuses the box at the caret, or ends the focus.
    pub(super) fn focus_box(&mut self) -> Task<Message> {
        if self.doc().paper.focused().is_some() {
            self.doc_mut().paper.set_focus(None);
            self.notice = Some("the focus ended".into());
            return Task::none();
        }
        let (text, row, col) = self.caret_in_source();
        match self.doc().paper.box_at_caret(&text, row, col) {
            Some(name) => {
                self.notice = Some(format!(
                    "focus on {name}: it follows the caret; Shift+click a box or the paper"
                ));
                self.doc_mut().paper.set_focus(Some(name));
            }
            None => self.notice = Some("put the caret on the name of a box in the diagram".into()),
        }
        Task::none()
    }

    /// Folds the container at the caret, or unfolds it.
    pub(super) fn fold_box(&mut self) -> Task<Message> {
        let (text, row, col) = self.caret_in_source();
        let key = self.doc().key;
        match self.doc_mut().paper.fold_at_caret(&text, row, col) {
            Ok(done) => {
                self.notice = Some(done);
                self.doc_mut().paper.take_refold();
                self.render(key, true)
            }
            Err(why) => {
                self.notice = Some(why);
                Task::none()
            }
        }
    }

    /// Offers the colors of the window for the box at the caret.
    pub(super) fn color_box(&mut self) -> Task<Message> {
        let (text, row, col) = self.caret_in_source();
        let Some(name) = self.doc().paper.box_at_caret(&text, row, col) else {
            self.notice = Some("put the caret on the name of a box in the diagram".into());
            return Task::none();
        };
        let p = &self.palette;
        let colors = [
            ("Accent", p.accent),
            ("Red", p.red),
            ("Orange", p.orange),
            ("Yellow", p.yellow),
            ("Green", p.green),
            ("Cyan", p.cyan),
            ("Blue", p.blue),
            ("Magenta", p.magenta),
        ];
        let mut items: Vec<Item> = colors
            .into_iter()
            .map(|(label, rgb)| {
                let color = rgb.to_string();
                Item {
                    label: format!("{name}: {label}"),
                    hint: color.clone(),
                    choice: Choice::Color {
                        name: name.clone(),
                        color: Some(color),
                    },
                }
            })
            .collect();
        items.push(Item {
            label: format!("{name}: no color of its own"),
            hint: String::new(),
            choice: Choice::Color {
                name: name.clone(),
                color: None,
            },
        });
        self.place_picker(Picker::new(Kind::Colors, "color the box", items));
        iced::widget::operation::focus(PICKER)
    }

    /// Colors the box `name` in the source, as one undo step, and draws
    /// the diagram again.
    pub(super) fn colored(&mut self, name: &str, color: Option<&str>) -> Task<Message> {
        let key = self.doc().key;
        let doc = self.doc();
        let edit = omaspec_core::recolor::edit(&doc.text(), doc.language, name, color);
        match edit {
            Ok(edit) => {
                self.doc_mut().replace(vec![edit]);
                self.edited(key);
                self.notice = Some(match color {
                    Some(color) => format!("{name} colored {color} (Ctrl+Z undoes it)"),
                    None => format!("{name} in the diagram's colors again (Ctrl+Z undoes it)"),
                });
            }
            Err(why) => self.notice = Some(why),
        }
        Task::none()
    }

    /// Unfolds every container of the diagram on screen.
    pub(super) fn unfold_all(&mut self) -> Task<Message> {
        let key = self.doc().key;
        if !self.doc_mut().paper.unfold_all() {
            self.notice = Some("nothing is folded".into());
            return Task::none();
        }
        self.doc_mut().paper.take_refold();
        self.notice = Some("every container unfolded".into());
        self.render(key, true)
    }

    /// The focus moves with the caret onto another box, while it is on.
    pub(super) fn focus_follows(&mut self, index: usize, text: &str, row: usize, col: usize) {
        let paper = &mut self.docs[index].paper;
        if paper.focused().is_none() {
            return;
        }
        if let Some(name) = paper.box_at_caret(text, row, col) {
            paper.set_focus(Some(name));
        }
    }
}
