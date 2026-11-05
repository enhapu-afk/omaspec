//! Compare A|B: the active tab (A) next to another version of it (B), an
//! open tab or the file in a git commit. B shows as a diagram, as the
//! source lines that changed from B to A (blank lines and indentation do
//! not count), or as what changed in the behavior
//! ([`omaspec_model::diff`]).

use anyhow::Result;
use iced::Task;
use omaspec_core::Language;
use omaspec_core::diff::{self, Op};
use omaspec_model::{Change, Specs};

use super::doc::Doc;
use super::picker::{Choice, Item, Kind, Picker};
use super::{App, Message, PICKER};
use crate::paper;
use crate::paper::Paper;
use crate::worker::blocking;

/// The render ticket's document key for B.
pub const KEY: u64 = u64::MAX;
/// Unchanged lines shown around a change.
const CONTEXT: usize = 2;

/// What B is.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// Another open tab, followed while it changes.
    Tab(u64),
    /// The file as of a git revision.
    Commit { rev: String, label: String },
}

pub struct Compare {
    /// The tab compared (A).
    pub a: u64,
    pub target: Target,
    /// What the B pane's title says.
    pub title: String,
    /// B's source.
    pub text: String,
    pub language: Language,
    pub paper: Paper,
    /// B as changed lines instead of a diagram.
    pub show_diff: bool,
    /// B as the changes in behavior instead of a diagram.
    pub show_behavior: bool,
    /// Zoom and pan both diagrams together.
    pub sync: bool,
    /// The changed lines from B to A; `None` when there are too many.
    pub rows: Option<Vec<Row>>,
    /// What changed in the behavior from B to A, or why that is not told.
    pub behavior: Result<Vec<Change>, String>,
}

impl Compare {
    pub fn new(a: u64, target: Target, title: String, text: String, language: Language) -> Self {
        Self {
            a,
            target,
            title,
            text,
            language,
            paper: Paper::default(),
            show_diff: false,
            show_behavior: false,
            sync: true,
            rows: Some(Vec::new()),
            behavior: Ok(Vec::new()),
        }
    }

    /// Recomputes the changed lines and the changes in behavior against
    /// A's `text`, in `language`, whose project has the state machines
    /// `specs`.
    pub fn refresh(&mut self, a_text: &str, language: Language, specs: &Specs) {
        self.rows = rows(&self.text, a_text);
        self.behavior = omaspec_model::diff(&self.text, self.language, a_text, language, specs);
    }
}

/// A line of the diff list.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    /// In both; `line` is A's (zero-based).
    Same { line: usize, text: String },
    /// New or changed in A.
    Added { line: usize, text: String },
    /// In B only; `line` is where it would be in A.
    Removed { line: usize, text: String },
    /// Unchanged lines left out.
    Gap,
}

/// The changes from `b` to `a` with a little context, or `None` when they
/// are too many to list.
pub fn rows(b: &str, a: &str) -> Option<Vec<Row>> {
    let found = diff::line_diff(b, a)?;
    let mut out = Vec::new();
    let hunks = diff::hunks(&found.ops, CONTEXT);
    for (i, hunk) in hunks.iter().enumerate() {
        if i > 0 {
            out.push(Row::Gap);
        }
        // A removed line sits where the next line of A is.
        let next_a = |from: usize| {
            hunk[from..]
                .iter()
                .find_map(|op| match op {
                    Op::Same { new, .. } | Op::Added { new } => Some(new.number - 1),
                    Op::Removed { .. } => None,
                })
                .unwrap_or_else(|| a.split('\n').count().saturating_sub(1))
        };
        for (k, op) in hunk.iter().enumerate() {
            out.push(match op {
                Op::Same { new, .. } => Row::Same {
                    line: new.number - 1,
                    text: new.text.clone(),
                },
                Op::Added { new } => Row::Added {
                    line: new.number - 1,
                    text: new.text.clone(),
                },
                Op::Removed { old } => Row::Removed {
                    line: next_a(k),
                    text: old.text.clone(),
                },
            });
        }
    }
    Some(out)
}

/// A's lines that are new or changed against `b`, as runs, and the lines
/// of A before which B had lines A lacks; `None` when too many changed.
pub fn marks(b: &str, a: &str) -> Option<(Vec<std::ops::Range<usize>>, Vec<usize>)> {
    let found = diff::line_diff(b, a)?;
    let mut added: Vec<std::ops::Range<usize>> = Vec::new();
    let mut removed = Vec::new();
    let mut pending_removal = false;
    for op in &found.ops {
        match op {
            Op::Added { new } => {
                let line = new.number - 1;
                match added.last_mut() {
                    Some(run) if run.end == line => run.end += 1,
                    _ => added.push(line..line + 1),
                }
                pending_removal = false;
            }
            Op::Same { new, .. } => {
                if pending_removal {
                    removed.push(new.number - 1);
                }
                pending_removal = false;
            }
            Op::Removed { .. } => pending_removal = true,
        }
    }
    if pending_removal {
        removed.push(a.split('\n').count().saturating_sub(1));
    }
    Some((added, removed))
}

impl App {
    /// The versions to compare this tab with: the other tabs at once, the
    /// file's git commits when git has listed them.
    pub(super) fn open_versions(&mut self) -> Task<Message> {
        let doc = self.doc();
        let key = doc.key;
        let mut items = Vec::new();
        if self.compare.as_ref().is_some_and(|c| c.a == key) {
            items.push(Item {
                label: "Stop comparing".into(),
                hint: String::new(),
                choice: Choice::Run(super::Command::StopCompare),
            });
        }
        items.extend(self.docs.iter().filter(|d| d.key != key).map(|d| Item {
            label: d.name(),
            hint: "tab".into(),
            choice: Choice::Compare(Target::Tab(d.key)),
        }));
        let git = doc.path.clone().filter(|_| doc.on_disk());
        self.place_picker(Picker::new(Kind::Versions, "compare with", items));
        let focus = iced::widget::operation::focus(PICKER);
        let Some(path) = git else {
            return focus;
        };
        let commits = Task::perform(
            blocking(move || crate::git::revisions(&path)),
            move |found| Message::Revisions(key, found.unwrap_or_default()),
        );
        Task::batch([focus, commits])
    }

    /// Shows tab `key` (A) next to `text` (B).
    pub(super) fn start_compare(&mut self, key: u64, target: Target, text: String) {
        let Some(index) = self.docs.iter().position(|d| d.key == key) else {
            return;
        };
        self.stop_compare();
        let title = match &target {
            Target::Tab(other) => {
                let name = self.docs.iter().find(|d| d.key == *other).map(Doc::name);
                format!("tab {}", name.unwrap_or_default())
            }
            Target::Commit { label, .. } => label.clone(),
        };
        let a = &mut self.docs[index];
        let language = a
            .forced
            .unwrap_or_else(|| omaspec_core::detect(&text, Language::from_file_name(&a.name())));
        a.set_baseline(Some(text.clone()));
        let mut compare = Compare::new(key, target, title, text, language);
        compare.refresh(&a.text(), a.current_language(), &a.specs);
        let (source, pending) = self.diagram_source(&compare.text, language);
        let place = self.place_of(key);
        let worker = self.worker.clone();
        if let Some(worker) = worker {
            compare.paper.request(&worker, KEY, source, language, place);
            if let Some(pending) = pending {
                self.pending
                    .insert((KEY, compare.paper.requested()), pending);
            }
        }
        let _ = compare.paper.set_scale_factor(self.paper_scale());
        self.notice = Some(format!("comparing with {}", compare.title));
        self.compare = Some(compare);
    }

    pub(super) fn stop_compare(&mut self) {
        if let Some(c) = self.compare.take()
            && let Some(a) = self.find(c.a)
        {
            a.set_baseline(None);
        }
    }

    /// A message for B's diagram; zoom and pan go to A too while they are
    /// synced. Ctrl+click names B's line.
    pub(super) fn compare_paper(&mut self, message: paper::Message) -> Task<Message> {
        let Some(c) = &mut self.compare else {
            return Task::none();
        };
        if let paper::Message::Pick(at) = message {
            self.notice = Some(match c.paper.label_at(at) {
                Some((label, source)) => {
                    match omaspec_core::locate::line_of(&source, &label.text, label.nth) {
                        Some(line) => format!("B line {}: {}", line + 1, label.text),
                        None => format!("“{}” is not in B", label.text),
                    }
                }
                None => "no label there".into(),
            });
            return Task::none();
        }
        let moves = paper::Paper::moves_view(&message);
        let task = c.paper.update(message).map(|m| Message::Paper(KEY, m));
        if !(moves && c.sync) {
            return task;
        }
        let (a, view) = (c.a, c.paper.view);
        let follow = match self.find(a) {
            Some(doc) => doc.paper.follow(view).map(move |m| Message::Paper(a, m)),
            None => Task::none(),
        };
        Task::batch([task, follow])
    }

    /// After tab `key` changed: B follows it when it is B; the changed
    /// lines follow A.
    pub(super) fn compare_edited(&mut self, key: u64) {
        let Some(c) = &self.compare else {
            return;
        };
        let (a, target) = (c.a, c.target.clone());
        let mut render_b = None;
        if target == Target::Tab(key) {
            let text = self.docs.iter().find(|d| d.key == key).map(Doc::text);
            if let (Some(text), Some(c)) = (text, &mut self.compare) {
                if c.text == text {
                    return;
                }
                c.text = text.clone();
                let language = c.language;
                if let Some(doc) = self.find(a) {
                    doc.set_baseline(Some(text.clone()));
                }
                render_b = Some((text, language));
            }
        }
        if key == a || target == Target::Tab(key) {
            let a_doc = self.docs.iter().find(|d| d.key == a);
            let a_side = a_doc.map(|d| (d.text(), d.current_language(), d.specs.clone()));
            if let (Some((a_text, language, specs)), Some(c)) = (a_side, &mut self.compare) {
                c.refresh(&a_text, language, &specs);
            }
        }
        if let Some((text, language)) = render_b {
            self.dispatch(KEY, &text, language);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: &str = "@startuml\nA -> B : one\nB -> C : two\nC -> D : three\nD -> E : four\nE -> F : five\nF -> G : six\nG -> H : seven\nH -> I : eight\nI -> J : nine\n@enduml\n";

    #[test]
    fn lists_changes_with_context() {
        let a = B.replace("two", "TWO").replace("I -> J : nine\n", "");
        let rows = rows(B, &a).unwrap();
        assert!(rows.contains(&Row::Added {
            line: 2,
            text: "B -> C : TWO".into()
        }));
        assert!(rows.contains(&Row::Removed {
            line: 2,
            text: "B -> C : two".into()
        }));
        assert!(rows.contains(&Row::Removed {
            line: 9,
            text: "I -> J : nine".into()
        }));
        // Two changes far apart: a gap between them, context around them.
        assert!(rows.contains(&Row::Gap));
        assert!(rows.contains(&Row::Same {
            line: 1,
            text: "A -> B : one".into()
        }));
        assert!(
            !rows
                .iter()
                .any(|r| matches!(r, Row::Same { text, .. } if text.contains("five")))
        );
    }

    #[test]
    fn indentation_and_blank_lines_do_not_count() {
        let a = B
            .replace("A -> B", "  A -> B")
            .replace("@enduml", "\n@enduml");
        assert_eq!(rows(B, &a), Some(Vec::new()));
        assert_eq!(marks(B, &a), Some((Vec::new(), Vec::new())));
    }

    #[test]
    fn marks_runs_of_new_lines_and_removals() {
        let a = B
            .replace("B -> C : two\n", "B -> X\nX -> C\n")
            .replace("F -> G : six\n", "");
        let (added, removed) = marks(B, &a).unwrap();
        assert_eq!(added, vec![std::ops::Range { start: 2, end: 4 }]);
        // "six" was before "seven", now line 7 of A.
        assert_eq!(removed, [7]);
    }
}
