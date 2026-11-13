//! Compare A|B: the active tab (A) next to another version of it (B), an
//! open tab or the file in a git commit. B shows as a diagram, as the
//! source lines that changed from B to A (blank lines and indentation do
//! not count), or as what changed in the behavior
//! ([`omaspec_model::diff`]). The project a tab is in compares too, since
//! a commit or a tag ([`omaspec_model::diff_project`]): its changes are a
//! list to pick from, and a change picked opens its file next to that
//! version.

use std::path::{Path, PathBuf};

use anyhow::Result;
use iced::Task;
use omaspec_core::Language;
use omaspec_core::diff::{self, Op};
use omaspec_model::{Change, ChangeKind, DiagramChanges, Specs};

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

/// A change's file as a revision had it, to compare the file with.
#[derive(Debug, Clone, PartialEq)]
pub struct Since {
    pub rev: String,
    /// What the B pane's title says.
    pub label: String,
    /// Where the file was then, if it moved since.
    pub old: PathBuf,
}

/// Changes listed at most.
const MAX_CHANGES: usize = 1000;

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
        let project = self.project_of(self.doc());
        self.place_picker(Picker::new(Kind::Versions, "compare with", items));
        let focus = iced::widget::operation::focus(PICKER);
        let commits = match git {
            Some(path) => Task::perform(
                blocking(move || crate::git::revisions(&path)),
                move |found| Message::Revisions(key, found.unwrap_or_default()),
            ),
            None => Task::none(),
        };
        let project = match project {
            Some(root) => Task::perform(
                blocking({
                    let root = root.clone();
                    move || crate::git::folder_revisions(&root)
                }),
                move |found| {
                    Message::ProjectRevisions(key, root.clone(), found.unwrap_or_default())
                },
            ),
            None => Task::none(),
        };
        Task::batch([focus, commits, project])
    }

    /// The project's revisions, offered while the compare picker of tab
    /// `key` is open.
    pub(super) fn project_revisions(
        &mut self,
        key: u64,
        root: PathBuf,
        found: Vec<(String, String)>,
    ) {
        let items: Vec<Item> = found
            .into_iter()
            .map(|(rev, label)| Item {
                label: format!("the project since {label}"),
                hint: "project".into(),
                choice: Choice::ProjectSince {
                    root: root.clone(),
                    rev,
                    label,
                },
            })
            .collect();
        let current = self.doc().key == key;
        if let Some(picker) = &mut self.picker
            && picker.kind == Kind::Versions
            && current
        {
            picker.add_items(items);
        }
    }

    /// Compares the project at `root` with its version in `rev`, in the
    /// background: what is saved, an open tab as it is now.
    pub(super) fn compare_project(
        &mut self,
        root: PathBuf,
        rev: String,
        label: String,
    ) -> Task<Message> {
        let open: Vec<(String, String)> = self
            .docs
            .iter()
            .filter_map(|d| {
                let path = d.path.as_deref()?;
                Some((crate::specs::path_in(&root, path)?, d.text()))
            })
            .collect();
        self.notice = Some(format!("comparing the project with {label}…"));
        Task::perform(
            blocking({
                let (root, rev) = (root.clone(), rev.clone());
                move || project_diff(&root, &rev, open)
            }),
            move |changes| {
                let changes = changes.unwrap_or_else(|| Err("the comparison stopped".into()));
                Message::ProjectChanges(root.clone(), rev.clone(), label.clone(), changes)
            },
        )
    }

    /// The project's changes since `label`, as a list to pick from.
    pub(super) fn project_changes(
        &mut self,
        root: &Path,
        rev: &str,
        label: &str,
        changes: Result<Vec<DiagramChanges>, String>,
    ) -> Task<Message> {
        let diagrams = match changes {
            Ok(diagrams) => diagrams,
            Err(err) => {
                self.notice = Some(format!("cannot compare the project: {err}"));
                return Task::none();
            }
        };
        if diagrams.is_empty() {
            self.notice = Some(format!("no diagram changed in its behavior since {label}"));
            return Task::none();
        }
        let items = change_items(root, rev, label, &diagrams);
        self.notice = Some(format!(
            "{} {} in {} {} since {label}",
            items.len(),
            super::plural(items.len(), "change"),
            diagrams.len(),
            super::plural(diagrams.len(), "diagram"),
        ));
        self.place_picker(Picker::new(Kind::Changes, "filter the changes", items));
        iced::widget::operation::focus(PICKER)
    }

    /// Opens the file at `path` on `line`, and with `since` compares it
    /// with that version.
    pub(super) fn show_change(
        &mut self,
        path: &Path,
        line: usize,
        since: Option<Since>,
    ) -> Task<Message> {
        let shown = self.show_in(path, line, 0..0);
        let Some(since) = since else {
            return shown;
        };
        let key = self.doc().key;
        let target = Target::Commit {
            rev: since.rev.clone(),
            label: since.label,
        };
        let read = Task::perform(
            blocking(move || {
                crate::git::show(&since.old, &since.rev).map_err(|e| format!("{e:#}"))
            }),
            move |text| {
                Message::Version(
                    key,
                    target.clone(),
                    text.unwrap_or_else(|| Err("git failed".into())),
                )
            },
        );
        Task::batch([shown, read])
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
        compare.paper.views = self.look.views;
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

    /// Shows the comparison's text diff, or hides it.
    pub(super) fn toggle_compare_diff(&mut self) -> Task<Message> {
        if let Some(c) = &mut self.compare {
            c.show_diff = !c.show_diff;
            c.show_behavior = false;
        }
        Task::none()
    }

    /// Shows what changed in the behavior, or hides it.
    pub(super) fn toggle_compare_behavior(&mut self) -> Task<Message> {
        if let Some(c) = &mut self.compare {
            c.show_behavior = !c.show_behavior;
            c.show_diff = false;
        }
        Task::none()
    }

    /// The other version glides into the pane of the tab compared.
    pub(super) fn glide_into_compared(&mut self) -> Task<Message> {
        let old = self
            .compare
            .as_ref()
            .and_then(|c| Some((c.a, c.paper.drawn_now()?)));
        let Some((a, (drawn, source, language))) = old else {
            self.notice = Some("compare with a tab or a git commit first".into());
            return Task::none();
        };
        // Into the pane of the tab that is compared, not any other.
        if self.doc().key != a {
            self.notice = Some("the comparison is in another tab".into());
            return Task::none();
        }
        self.notice = None;
        self.doc_mut()
            .paper
            .glide_from(drawn, source, language)
            .map(move |m| Message::Paper(a, m))
    }

    /// A and B zoom together, or apart.
    pub(super) fn toggle_compare_sync(&mut self) -> Task<Message> {
        if let Some(c) = &mut self.compare {
            c.sync = !c.sync;
            self.notice = Some(if c.sync {
                "A and B zoom together".into()
            } else {
                "A and B zoom apart".into()
            });
        }
        Task::none()
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

/// The project at `root` as saved, the texts of `open` (by their paths in
/// the project) as they are, compared with its version in `rev`.
fn project_diff(
    root: &Path,
    rev: &str,
    open: Vec<(String, String)>,
) -> Result<Vec<DiagramChanges>, String> {
    let old = crate::names::diagram_files_at(root, rev).map_err(|e| format!("{e:#}"))?;
    let mut new = crate::names::diagram_files(root);
    for (path, text) in open {
        if let Some(file) = new.iter_mut().find(|f| f.path == path) {
            *file = crate::names::owned(path, text);
        }
    }
    let files = crate::history::files;
    Ok(omaspec_model::diff_project(&files(&old), &files(&new)))
}

/// One item for each change of `diagrams`, at most [`MAX_CHANGES`]: the
/// change, its file and line; picked, its file opens there, next to the
/// version in `rev` when the diagram was there then.
fn change_items(root: &Path, rev: &str, label: &str, diagrams: &[DiagramChanges]) -> Vec<Item> {
    let mut items = Vec::new();
    for d in diagrams {
        let first_new = d.changes.iter().find_map(|c| c.new_line).unwrap_or(0);
        for c in &d.changes {
            // A line of the version in `rev` is named as `omaspec diff`
            // names it; the file opens where it changed first.
            let (hint, line) = match (&d.new_file, c.new_line, &d.old_file, c.old_line) {
                (Some(file), Some(line), ..) => (format!("{file}:{}", line + 1), line),
                (_, _, Some(old), Some(line)) => (format!("{old}@{rev}:{}", line + 1), first_new),
                (Some(file), None, ..) => (file.clone(), first_new),
                (None, _, Some(old), None) => (format!("{old}@{rev}"), 0),
                (None, _, None, _) => continue,
            };
            let what = match (c.kind, c.severity) {
                (ChangeKind::Found, Some(severity)) => {
                    format!("found {}: ", severity_word(severity))
                }
                (ChangeKind::Resolved, Some(severity)) => {
                    format!("resolved {}: ", severity_word(severity))
                }
                _ => String::new(),
            };
            let since = match (&d.new_file, &d.old_file) {
                (Some(_), Some(old)) => Some(Since {
                    rev: rev.to_owned(),
                    label: format!("{old} as of {label}"),
                    old: root.join(old),
                }),
                _ => None,
            };
            items.push(Item {
                label: format!("{what}{}", c.message),
                hint,
                choice: Choice::Changed {
                    path: d.new_file.as_ref().map(|f| root.join(f)),
                    line,
                    since,
                    note: format!(
                        "{} was removed since {label}",
                        d.old_file.as_deref().unwrap_or_default()
                    ),
                },
            });
            if items.len() >= MAX_CHANGES {
                return items;
            }
        }
    }
    items
}

fn severity_word(severity: omaspec_core::lint::Severity) -> &'static str {
    match severity {
        omaspec_core::lint::Severity::Error => "error",
        omaspec_core::lint::Severity::Warning => "warning",
        omaspec_core::lint::Severity::Info => "info",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: &str = "@startuml\nA -> B : one\nB -> C : two\nC -> D : three\nD -> E : four\nE -> F : five\nF -> G : six\nG -> H : seven\nH -> I : eight\nI -> J : nine\n@enduml\n";

    const DOOR: &str = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";

    /// A project in a fresh repository, committed; `None` without git.
    fn committed(name: &str, files: &[(&str, &str)]) -> Option<PathBuf> {
        let root = std::env::temp_dir().join(format!("omaspec-cmp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for (file, text) in files {
            std::fs::write(root.join(file), text).unwrap();
        }
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args)
                .output()
                .is_ok_and(|o| o.status.success())
        };
        (git(&["init", "-q"]) && git(&["add", "."]) && git(&["commit", "-qm", "one"]))
            .then_some(root)
    }

    #[test]
    fn the_project_since_a_commit_is_a_list_of_its_changes() {
        let Some(root) = committed(
            "project",
            &[
                ("door.puml", DOOR),
                ("visit.puml", "@startuml\nUser -> Door : lock\n@enduml\n"),
                ("flow.puml", "@startuml\nstart\n:fill;\nstop\n@enduml\n"),
            ],
        ) else {
            eprintln!("skipped: no git");
            return;
        };
        // Saved: the door no longer locks, the flow is gone. Open and not
        // saved: a state more, which counts as it is in the tab.
        let unlockable = DOOR
            .replace("Closed --> Locked : lock\n", "")
            .replace("Locked --> Closed : unlock\n", "");
        std::fs::write(root.join("door.puml"), &unlockable).unwrap();
        std::fs::remove_file(root.join("flow.puml")).unwrap();
        let open = vec![(
            "door.puml".to_owned(),
            unlockable.replace("@enduml", "Open --> Ajar : wind\n@enduml"),
        )];
        let diagrams = project_diff(&root, "HEAD", open).unwrap();

        let start = super::super::Start {
            files: vec![root.join("door.puml")],
            ..Default::default()
        };
        let mut app = App::new(start, None).0;
        let key = app.doc().key;
        // The compare picker offers the project since its revisions.
        let _ = app.update(Message::Command(super::super::Command::Compare));
        let _ = app.update(Message::ProjectRevisions(
            key,
            root.clone(),
            vec![("abc1234".into(), "HEAD · abc1234 one".into())],
        ));
        let picker = app.picker.as_ref().unwrap();
        assert!(
            picker
                .visible()
                .any(|(_, item)| item.label == "the project since HEAD · abc1234 one"),
        );

        let _ = app.update(Message::ProjectChanges(
            root.clone(),
            "HEAD".into(),
            "HEAD".into(),
            Ok(diagrams),
        ));
        let picker = app.picker.as_ref().unwrap();
        assert_eq!(picker.kind, Kind::Changes);
        let labels: Vec<(String, String)> = picker
            .visible()
            .map(|(_, item)| (item.label.clone(), item.hint.clone()))
            .collect();
        let has = |label: &str, hint: &str| {
            labels
                .iter()
                .any(|(l, h)| l.contains(label) && h.as_str() == hint)
        };
        // The open tab as it is: Ajar is on its sixth line.
        assert!(has("New state Ajar.", "door.puml:6"), "{labels:?}");
        // What is gone, on its line then.
        assert!(
            has(
                "Transition removed: Closed to Locked on lock.",
                "door.puml@HEAD:6"
            ),
            "{labels:?}"
        );
        assert!(
            has(
                "found warning: Door has no transition for lock",
                "visit.puml:2"
            ),
            "{labels:?}"
        );
        assert!(
            has("Activity diagram flow removed.", "flow.puml@HEAD:1"),
            "{labels:?}"
        );
        assert!(
            app.notice
                .as_ref()
                .unwrap()
                .ends_with("in 3 diagrams since HEAD")
        );

        // Typing a file's name keeps its changes.
        let picker = app.picker.as_mut().unwrap();
        picker.set_query("visit".into());
        assert!(
            picker
                .visible()
                .all(|(_, item)| item.hint.starts_with("visit.puml"))
        );
        // A removed diagram only tells that it went.
        picker.set_query("flow removed".into());
        let _ = app.update(Message::PickerSubmit);
        assert_eq!(
            app.notice.as_deref(),
            Some("flow.puml was removed since HEAD")
        );

        // A change picked opens its file on its line, and compares it with
        // the version then.
        let _ = app.update(Message::ProjectChanges(
            root.clone(),
            "HEAD".into(),
            "HEAD".into(),
            project_diff(&root, "HEAD", Vec::new()),
        ));
        let picker = app.picker.as_mut().unwrap();
        picker.set_query("Transition removed".into());
        let Some(Choice::Changed { path, since, .. }) = picker.chosen().cloned() else {
            panic!("a change");
        };
        assert_eq!(path.as_deref(), Some(root.join("door.puml").as_path()));
        let since = since.expect("the door was there then");
        assert_eq!(since.label, "door.puml as of HEAD");
        assert_eq!(crate::git::show(&since.old, &since.rev).unwrap(), DOOR);
        let _ = app.update(Message::PickerSubmit);
        assert!(app.doc().path.as_deref() == Some(root.join("door.puml").as_path()));
        let _ = std::fs::remove_dir_all(&root);
    }

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
