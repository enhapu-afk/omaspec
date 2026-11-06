//! Folding containers and the focus on a box, from the keyboard
//! (docs/adr/0024): the box whose name the caret is on.

use iced::Task;

use super::{App, Message};

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
