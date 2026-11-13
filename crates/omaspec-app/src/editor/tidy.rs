//! Tidying the source by command: formatted, written evenly, made
//! portable for mermaid.js, and the fixes lint offers, one or all.

use iced::Task;

use super::{App, Message};

impl App {
    pub(super) fn format_source(&mut self) -> Task<Message> {
        self.tidy(|doc| doc.format(), "already formatted")
    }

    pub(super) fn prettify_source(&mut self) -> Task<Message> {
        self.tidy(|doc| doc.prettify(), "already written evenly")
    }

    pub(super) fn make_portable(&mut self) -> Task<Message> {
        self.tidy(|doc| doc.portable(), "mermaid.js reads it as it is")
    }

    /// `change` on the tab; `unchanged` said when it changed nothing.
    fn tidy(
        &mut self,
        change: impl FnOnce(&mut super::doc::Doc) -> bool,
        unchanged: &str,
    ) -> Task<Message> {
        let key = self.doc().key;
        if change(self.doc_mut()) {
            self.edited(key);
        } else {
            self.notice = Some(unchanged.into());
        }
        Task::none()
    }

    pub(super) fn fix_at_caret(&mut self) -> Task<Message> {
        let key = self.doc().key;
        match self.doc_mut().fix_at_caret() {
            Some(title) => {
                self.edited(key);
                self.notice = Some(format!("fixed: {title}"));
            }
            None => self.notice = Some("no finding on this line has a fix".into()),
        }
        Task::none()
    }

    pub(super) fn fix_all(&mut self) -> Task<Message> {
        let key = self.doc().key;
        match self.doc_mut().fix_all() {
            0 => self.notice = Some("no finding has a fix".into()),
            done => {
                self.edited(key);
                self.notice = Some(match done {
                    1 => "1 finding fixed".to_owned(),
                    done => format!("{done} findings fixed"),
                });
            }
        }
        Task::none()
    }
}
