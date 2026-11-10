//! Lines and boxes placed by hand, written into the source from the
//! diagram (docs/adr/0016, 0021): a line's bends, a box held beside its
//! neighbor, a label's side, and the way back to the layout.

use iced::Task;

use super::{App, Message};

impl App {
    /// Writes the bends of a line drawn by hand in the diagram into the
    /// source, as one undo step (docs/adr/0016).
    pub(super) fn draw_by_hand(
        &mut self,
        key: u64,
        line: &omaspec_uml::hand::Line,
    ) -> Task<Message> {
        let Some(doc) = self.find(key) else {
            return Task::none();
        };
        let language = doc.current_language();
        let Some(edit) = omaspec_core::hand::edit(&doc.text(), language, line) else {
            doc.paper.by_hand_unchanged();
            return Task::none();
        };
        doc.replace(vec![edit]);
        self.edited(key);
        self.notice = Some(if line.corners.is_empty() {
            format!("{} runs its own way again (Ctrl+Z undoes it)", line.key)
        } else {
            format!("{} drawn by hand (Ctrl+Z undoes it)", line.key)
        });
        self.render(key, false)
    }

    /// Holds the box `name` where `hold` says, or gives it back to the
    /// layout, in the source, as one undo step (docs/adr/0021).
    pub(super) fn hold_box(
        &mut self,
        key: u64,
        name: &str,
        hold: Option<&omaspec_uml::hand::Hold>,
    ) -> Task<Message> {
        let Some(doc) = self.find(key) else {
            return Task::none();
        };
        let language = doc.current_language();
        let Some(edit) = omaspec_core::hand::hold(&doc.text(), language, name, hold) else {
            doc.paper.held_unchanged();
            return Task::none();
        };
        doc.replace(vec![edit]);
        self.edited(key);
        self.notice = Some(match hold {
            Some(hold) => format!("{hold}: held in place (Ctrl+Z undoes it)"),
            None => format!("{name} stands where the layout puts it again (Ctrl+Z undoes it)"),
        });
        self.render(key, false)
    }

    /// The line at the caret back to the way the layout gives it, or the
    /// box whose entry is at the caret.
    /// Puts the label of the line at the caret on the other side of its
    /// line, in the source, as one undo step: the side it is not on now,
    /// as drawn.
    pub(super) fn label_side(&mut self) -> Task<Message> {
        let row = self.doc().editor.cursor().row as usize;
        let Some(wire) = self.doc().paper.wire_on(row).cloned() else {
            self.notice = Some("no line of the diagram at the caret".into());
            return Task::none();
        };
        let Some(side) = other_side(&wire) else {
            self.notice = Some(format!("{} has no label to move", wire.key));
            return Task::none();
        };
        let key = self.doc().key;
        let doc = self.doc();
        let edit = omaspec_core::hand::label(&doc.text(), doc.language, &wire.key, Some(side));
        let Some(edit) = edit else {
            self.notice = Some(format!("the label of {} is there already", wire.key));
            return Task::none();
        };
        self.doc_mut().replace(vec![edit]);
        self.edited(key);
        self.notice = Some(format!(
            "the label of {} goes {side} (Ctrl+Z undoes it)",
            wire.key
        ));
        self.render(key, true)
    }

    pub(super) fn reset_line(&mut self) -> Task<Message> {
        let row = self.doc().editor.cursor().row as usize;
        if let Some(name) = self.doc().paper.place_on(row).map(|p| p.name.clone()) {
            let doc = self.doc().key;
            return self.hold_box(doc, &name, None);
        }
        let found = self
            .doc()
            .paper
            .wire_on(row)
            .map(|w| (w.key.clone(), w.entry));
        match found {
            Some((key, Some(_))) => {
                let doc = self.doc().key;
                let line = omaspec_uml::hand::Line {
                    key,
                    corners: Vec::new(),
                };
                self.draw_by_hand(doc, &line)
            }
            Some((key, None)) => {
                // Its label's side by hand goes back to the layout too.
                let doc = self.doc();
                let edit = omaspec_core::hand::label(&doc.text(), doc.language, &key, None);
                match edit {
                    Some(edit) => {
                        let at = doc.key;
                        self.doc_mut().replace(vec![edit]);
                        self.edited(at);
                        self.notice = Some(format!(
                            "the label of {key} takes its side from the layout again (Ctrl+Z undoes it)"
                        ));
                        self.render(at, true)
                    }
                    None => {
                        self.notice = Some(format!("{key} runs its own way already"));
                        Task::none()
                    }
                }
            }
            None => {
                self.notice = Some("no line of the diagram at the caret".into());
                Task::none()
            }
        }
    }

    /// Every line and every box back to the way the layout gives them.
    pub(super) fn reset_lines(&mut self) -> Task<Message> {
        let key = self.doc().key;
        let Some(edit) = omaspec_core::hand::clear(&self.doc().text()) else {
            self.notice = Some("no line is drawn by hand and no box held".into());
            return Task::none();
        };
        self.doc_mut().replace(vec![edit]);
        self.edited(key);
        self.notice = Some("every line and box goes back to the layout (Ctrl+Z undoes it)".into());
        self.render(key, false)
    }
}

/// The side of its line the label of `wire` is not on, as drawn: left or
/// right of a run that goes down, above or below one that goes across.
/// `None` without a label.
fn other_side(wire: &omaspec_uml::map::Wire) -> Option<omaspec_uml::hand::Side> {
    use omaspec_uml::hand::Side;
    let label = wire.labels.first()?;
    let (cx, cy) = (label.x + label.w / 2.0, label.y + label.h / 2.0);
    // The run of the line nearest the label's middle.
    let nearest = wire.path.windows(2).min_by(|a, b| {
        let gap = |run: &[(f32, f32)]| {
            let (p, q) = (run[0], run[1]);
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            let len = (dx * dx + dy * dy).max(f32::EPSILON);
            let t = (((cx - p.0) * dx + (cy - p.1) * dy) / len).clamp(0.0, 1.0);
            let (x, y) = (p.0 + dx * t, p.1 + dy * t);
            (cx - x).hypot(cy - y)
        };
        gap(a).total_cmp(&gap(b))
    })?;
    let (p, q) = (nearest[0], nearest[1]);
    Some(if (q.0 - p.0).abs() <= (q.1 - p.1).abs() {
        // A run that goes down: the label lies left or right of it.
        if cx < (p.0 + q.0) / 2.0 {
            Side::Right
        } else {
            Side::Left
        }
    } else if cy < (p.1 + q.1) / 2.0 {
        Side::Below
    } else {
        Side::Above
    })
}
