//! Compare A|B: the active tab (A) next to another version of it (B), an
//! open tab or the file in a git commit. B shows as a diagram, as the
//! source lines that changed from B to A (blank lines and indentation do
//! not count), or as what changed in the behavior
//! ([`omagram_model::diff`]).

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use iced::Task;
use omagram_core::Language;
use omagram_core::diff::{self, Op};
use omagram_model::{Change, Spec};

use super::doc::Doc;
use super::picker::{Choice, Item, Kind, Picker};
use super::{App, Message, PICKER};
use crate::paper;
use crate::paper::Paper;
use crate::worker::blocking;

/// The render ticket's document key for B.
pub const KEY: u64 = u64::MAX;
/// Commits offered at most.
const MAX_COMMITS: usize = 30;
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
    pub fn refresh(&mut self, a_text: &str, language: Language, specs: &[Spec]) {
        self.rows = rows(&self.text, a_text);
        self.behavior = omagram_model::diff(&self.text, self.language, a_text, language, specs);
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

/// Git in `dir`, kept from starting programs a repository names.
///
/// A repository's own `.git/config` comes with a folder someone hands over
/// (an archive, a stick), and git trusts it. `log.showSignature` with
/// `gpg.program` would make `git log` run any program on a signed commit;
/// the others switch off what could run one too: a file system monitor, a
/// pager, external diffs and text conversions, hooks. No prompt waits for a
/// password, and no lock is taken.
fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command.arg("-C").arg(dir);
    for setting in [
        "log.showSignature=false",
        "core.fsmonitor=false",
        "core.pager=cat",
        "core.hooksPath=/dev/null",
        "diff.external=",
    ] {
        command.args(["-c", setting]);
    }
    command
        .env("GIT_PAGER", "cat")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0");
    command
}

/// `HEAD` and the commits that changed `file`, newest first, as
/// (revision, what to show); empty outside a git repository.
pub fn revisions(file: &Path) -> Vec<(String, String)> {
    let (Some(dir), Some(name)) = (file.parent(), file.file_name()) else {
        return Vec::new();
    };
    let out = git(dir)
        .args([
            "log",
            "--no-show-signature",
            "--no-color",
            &format!("-n{MAX_COMMITS}"),
            "--format=%h%x09%ad%x09%s",
            "--date=short",
            "--",
        ])
        .arg(name)
        .output();
    let Some(out) = out.ok().filter(|o| o.status.success()) else {
        return Vec::new();
    };
    let log = String::from_utf8_lossy(&out.stdout);
    let mut found: Vec<(String, String)> = log
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let (rev, date, subject) = (parts.next()?, parts.next()?, parts.next()?);
            Some((rev.to_owned(), format!("{rev} {date} {subject}")))
        })
        .collect();
    if let Some((_, label)) = found.first_mut() {
        *label = format!("HEAD · {label}");
    }
    found
}

/// The text of `file` as of `rev`.
pub fn show(file: &Path, rev: &str) -> Result<String> {
    // A revision, never an option: `--output=…` would make git write.
    let known = |c: char| c.is_ascii_alphanumeric() || "_.^~/@{}-".contains(c);
    anyhow::ensure!(
        !rev.is_empty() && !rev.starts_with('-') && rev.chars().all(known),
        "`{rev}` is no git revision"
    );
    let file = std::path::absolute(file)?;
    let dir = file.parent().context("no folder")?;
    let name = file.file_name().context("no file name")?.to_string_lossy();
    // `./` makes the path relative to `dir`, not to the repository.
    let out = git(dir)
        .args([
            "show",
            "--no-color",
            "--no-textconv",
            "--no-ext-diff",
            &format!("{rev}:./{name}"),
        ])
        .output()
        .context("cannot run git")?;
    anyhow::ensure!(
        out.status.success(),
        "git: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
        let commits = Task::perform(blocking(move || revisions(&path)), move |found| {
            Message::Revisions(key, found.unwrap_or_default())
        });
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
            .unwrap_or_else(|| omagram_core::detect(&text, Language::from_file_name(&a.name())));
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
                    match omagram_core::locate::line_of(&source, &label.text, label.nth) {
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

    /// A repository handed over with a `.git/config` that names programs:
    /// listing and reading its versions starts none of them.
    #[test]
    fn a_prepared_repository_starts_nothing() {
        let dir = std::env::temp_dir().join(format!("omagram-git-trap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let run = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(args)
                .output()
                .is_ok_and(|o| o.status.success())
        };
        if !run(&["init", "-q"]) {
            eprintln!("skipped: no git");
            return;
        }
        let file = dir.join("door.puml");
        std::fs::write(&file, "@startuml\nA -> B : x\n@enduml\n").unwrap();
        std::fs::write(dir.join(".gitattributes"), "*.puml diff=trap\n").unwrap();
        assert!(run(&["add", "."]) && run(&["commit", "-qm", "one", "--no-gpg-sign"]));
        // A commit that looks signed, touching the file.
        std::fs::write(&file, "@startuml\nA -> B : y\n@enduml\n").unwrap();
        assert!(run(&["add", "door.puml"]));
        let out = |args: &[&str]| {
            let o = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .unwrap();
            String::from_utf8_lossy(&o.stdout).trim().to_owned()
        };
        let (tree, parent) = (out(&["write-tree"]), out(&["rev-parse", "HEAD"]));
        let commit = format!(
            "tree {tree}\nparent {parent}\nauthor t <t@t> 1700000000 +0000\ncommitter t <t@t> 1700000000 +0000\ngpgsig -----BEGIN PGP SIGNATURE-----\n \n iQEzBAABCAAdFiEE\n -----END PGP SIGNATURE-----\n\nsigned\n"
        );
        std::fs::write(dir.join("commit.txt"), commit).unwrap();
        let signed = out(&["hash-object", "-t", "commit", "-w", "commit.txt"]);
        assert!(run(&["update-ref", "HEAD", &signed]));
        let trap = dir.join("trap.sh");
        let marker = dir.join("ran");
        std::fs::write(
            &trap,
            format!("#!/bin/sh\necho \"$0 $*\" >> {}\n", marker.display()),
        )
        .unwrap();
        std::fs::set_permissions(&trap, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        let trap = trap.display().to_string();
        for (key, value) in [
            ("log.showSignature", "true"),
            ("gpg.program", trap.as_str()),
            ("core.fsmonitor", trap.as_str()),
            ("core.pager", trap.as_str()),
            ("diff.trap.textconv", trap.as_str()),
            ("diff.external", trap.as_str()),
        ] {
            assert!(run(&["config", key, value]));
        }
        assert_eq!(revisions(&file).len(), 2);
        assert!(show(&file, "HEAD").unwrap().contains("A -> B : y"));
        assert!(
            !marker.exists(),
            "git started {}",
            std::fs::read_to_string(&marker).unwrap_or_default()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_versions_from_git() {
        let dir = std::env::temp_dir().join(format!("omagram-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let ok = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        let file = dir.join("flow.mmd");
        assert!(revisions(&file).is_empty(), "no repository yet");
        git(&["init", "-q"]);
        std::fs::write(&file, "flowchart LR\n  A --> B\n").unwrap();
        git(&["add", "flow.mmd"]);
        git(&["commit", "-q", "-m", "first"]);
        std::fs::write(&file, "flowchart LR\n  A --> C\n").unwrap();
        git(&["commit", "-q", "-am", "second"]);
        std::fs::write(&file, "flowchart LR\n  A --> D\n").unwrap();

        let revs = revisions(&file);
        assert_eq!(revs.len(), 2);
        assert!(revs[0].1.starts_with("HEAD · ") && revs[0].1.ends_with("second"));
        assert_eq!(
            show(&file, &revs[0].0).unwrap(),
            "flowchart LR\n  A --> C\n"
        );
        assert_eq!(
            show(&file, &revs[1].0).unwrap(),
            "flowchart LR\n  A --> B\n"
        );
        assert!(show(&file, "nosuchrev").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
