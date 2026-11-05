//! What changed in the behavior of a whole project between two versions
//! (ROADMAP, 0.8): a changelog of the specification. Each diagram of the
//! old version is paired with its own in the new one: by its file, a
//! Markdown document's diagrams there by their names, then by their texts,
//! then in order; a diagram that left its file by its text, then by its
//! name, then by its file's name (its folder renamed). Each version's sequence diagrams are checked against that
//! version's state machines, so a scenario that did not change still tells
//! what a changed machine no longer takes.

use std::borrow::Cow;
use std::collections::HashMap;

use omaspec_core::lint::Finding;
use omaspec_core::markdown::{self, Block};
use omaspec_core::{DiagramKind as Classified, Kind as DiagramKind, Language, classify, project};

use super::{Change, ChangeKind, a_kind, between, capital, change, findings, without_lines};
use crate::{File, FileKind, Machine, Specs, across, check};

/// What changed in one diagram of a project, a diagram file or one of a
/// Markdown document's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagramChanges {
    /// Its file in the old version; `None` for a new diagram.
    pub old_file: Option<String>,
    /// Its file in the new version; `None` for a removed one.
    pub new_file: Option<String>,
    /// The changes, on the files' lines; that the diagram is new, removed
    /// or moved on its first line.
    pub changes: Vec<Change>,
}

/// The changes in behavior from the project `old` to the project `new`,
/// by the diagrams' files, each diagram's as [`super::diff`] tells them;
/// the diagrams that did not change are left out. A recorded trace is no
/// diagram.
pub fn diff_project(old: &[File], new: &[File]) -> Vec<DiagramChanges> {
    let (old_specs, new_specs) = (Specs::of_files(old), Specs::of_files(new));
    let (a, b) = (units(old), units(new));
    let pairs = pairs(&a, &b);
    let mut moves: Vec<(&str, &str)> = pairs
        .iter()
        .filter_map(|&(i, j)| Some((a[i?].file, b[j?].file)))
        .filter(|(from, to)| from != to)
        .collect();
    moves.sort_unstable();
    moves.dedup();
    let versions = Versions {
        old: &old_specs,
        new: &new_specs,
        machines_same: old_specs == new_specs,
        moves,
    };
    let mut out: Vec<(String, usize, DiagramChanges)> = pairs
        .into_iter()
        .filter_map(|pair| {
            let (changes, at) = match pair {
                (Some(i), Some(j)) => (versions.compared(&a[i], &b[j]), &b[j]),
                (None, Some(j)) => (versions.added(&b[j]), &b[j]),
                (Some(i), None) => (versions.removed(&a[i]), &a[i]),
                (None, None) => return None,
            };
            (!changes.is_empty()).then(|| {
                let diagram = DiagramChanges {
                    old_file: pair.0.map(|i| a[i].file.to_owned()),
                    new_file: pair.1.map(|j| b[j].file.to_owned()),
                    changes,
                };
                (at.file.to_owned(), at.first(), diagram)
            })
        })
        .collect();
    out.sort_by(|x, y| (&x.0, x.1).cmp(&(&y.0, y.1)));
    out.into_iter().map(|(_, _, diagram)| diagram).collect()
}

/// One diagram of a project version.
struct Unit<'a> {
    file: &'a str,
    /// The document's block it is, in a document.
    block: Option<Block>,
    code: Cow<'a, str>,
    language: Language,
    kind: Classified,
    /// The name it goes by first: after `@start…`, its title, a diagram
    /// file's name.
    name: Option<String>,
}

impl Unit<'_> {
    /// The file's line for the diagram's zero-based `line`.
    fn line(&self, line: usize) -> usize {
        match &self.block {
            Some(block) => block.doc_line(line),
            None => line,
        }
    }

    fn first(&self) -> usize {
        self.line(0)
    }

    /// `state machine Door`, `activity diagram`, as a sentence names it.
    fn called(&self) -> String {
        let kind = match self.kind.kind {
            DiagramKind::State => "state machine".to_owned(),
            DiagramKind::Sequence => "sequence diagram".to_owned(),
            _ if self.kind.name.is_empty() => "diagram".to_owned(),
            _ => {
                let name = &self.kind.name;
                let acronym = name.chars().nth(1).is_some_and(char::is_uppercase);
                if acronym {
                    name.to_string()
                } else {
                    name.to_lowercase()
                }
            }
        };
        match &self.name {
            Some(name) => format!("{kind} {name}"),
            None => kind,
        }
    }

    fn behavior(&self) -> bool {
        matches!(self.kind.kind, DiagramKind::State | DiagramKind::Sequence)
    }

    /// `change` on the files' lines: `self` is the old diagram, `new` the
    /// new one.
    fn placed(&self, new: &Unit, mut change: Change) -> Change {
        change.old_line = change.old_line.map(|l| self.line(l));
        change.new_line = change.new_line.map(|l| new.line(l));
        change
    }
}

/// The diagrams of `files`: a diagram file's, each of a document's.
fn units<'a>(files: &[File<'a>]) -> Vec<Unit<'a>> {
    let mut out = Vec::new();
    for f in files {
        match f.kind {
            FileKind::Diagram(language) => {
                let name = project::names(f.text, language, f.path).into_iter().next();
                out.push(Unit {
                    file: f.path,
                    block: None,
                    code: Cow::Borrowed(f.text),
                    language,
                    kind: classify(f.text, language),
                    name,
                });
            }
            FileKind::Document => {
                for block in markdown::blocks(f.text) {
                    let code = block.code.clone();
                    out.push(Unit {
                        file: f.path,
                        kind: classify(&code, block.language),
                        name: project::names(&code, block.language, "").into_iter().next(),
                        language: block.language,
                        code: Cow::Owned(code),
                        block: Some(block),
                    });
                }
            }
            FileKind::Trace => {}
        }
    }
    out
}

/// An old diagram and its new version by their places in `a` and `b`; a
/// diagram on one side only is new or removed.
type Pair = (Option<usize>, Option<usize>);

/// The diagrams of `a` paired with their new versions in `b`.
fn pairs(a: &[Unit], b: &[Unit]) -> Vec<Pair> {
    let mut files: HashMap<&str, (Vec<usize>, Vec<usize>)> = HashMap::new();
    for (i, u) in a.iter().enumerate() {
        files.entry(u.file).or_default().0.push(i);
    }
    for (j, u) in b.iter().enumerate() {
        files.entry(u.file).or_default().1.push(j);
    }
    let mut out = Vec::new();
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for (mut olds, mut news) in files.into_values() {
        // In its file: by its name, by its text, then in order.
        by_key(&mut olds, &mut news, a, b, name_key, &mut out);
        by_key(&mut olds, &mut news, a, b, text_key, &mut out);
        let paired = olds.len().min(news.len());
        for (&i, &j) in olds.iter().zip(&news) {
            out.push((Some(i), Some(j)));
        }
        left.extend_from_slice(&olds[paired..]);
        right.extend_from_slice(&news[paired..]);
    }
    left.sort_unstable();
    right.sort_unstable();
    // Moved to another file: by its text, by its name, by its file's name.
    by_key(&mut left, &mut right, a, b, text_key, &mut out);
    by_key(&mut left, &mut right, a, b, name_key, &mut out);
    by_key(&mut left, &mut right, a, b, file_key, &mut out);
    out.extend(left.into_iter().map(|i| (Some(i), None)));
    out.extend(right.into_iter().map(|j| (None, Some(j))));
    out
}

/// The key a diagram pairs by when its name is the same.
fn name_key(u: &Unit) -> Option<String> {
    u.name.as_deref().map(project::key)
}

/// A diagram file's name without its folders; a document's diagrams go by
/// their own names.
fn file_key(u: &Unit) -> Option<String> {
    let name = u.file.rsplit('/').next().unwrap_or(u.file);
    u.block.is_none().then(|| name.to_owned())
}

fn text_key(u: &Unit) -> Option<String> {
    Some(u.code.to_string())
}

/// Pairs the diagrams of `olds` (in `a`) and `news` (in `b`) whose key is
/// the same and no other's on either side, and takes them out of both.
fn by_key(
    olds: &mut Vec<usize>,
    news: &mut Vec<usize>,
    a: &[Unit],
    b: &[Unit],
    key: impl Fn(&Unit) -> Option<String>,
    out: &mut Vec<Pair>,
) {
    if olds.is_empty() || news.is_empty() {
        return;
    }
    // A key → how many have it on each side, and the last of each.
    let mut keys: HashMap<String, (usize, usize, usize, usize)> = HashMap::new();
    for &i in olds.iter() {
        if let Some(k) = key(&a[i]).filter(|k| !k.is_empty()) {
            let entry = keys.entry(k).or_default();
            entry.0 += 1;
            entry.1 = i;
        }
    }
    for &j in news.iter() {
        if let Some(k) = key(&b[j]).filter(|k| !k.is_empty()) {
            let entry = keys.entry(k).or_default();
            entry.2 += 1;
            entry.3 = j;
        }
    }
    let mut found: Vec<(usize, usize)> = keys
        .into_values()
        .filter(|&(olds, _, news, _)| olds == 1 && news == 1)
        .map(|(_, i, _, j)| (i, j))
        .collect();
    if found.is_empty() {
        return;
    }
    found.sort_unstable();
    olds.retain(|i| !found.iter().any(|(x, _)| x == i));
    news.retain(|j| !found.iter().any(|(_, y)| y == j));
    out.extend(found.into_iter().map(|(i, j)| (Some(i), Some(j))));
}

/// The state machines of both versions.
struct Versions<'a> {
    old: &'a Specs,
    new: &'a Specs,
    /// Whether no state machine changed, so no scenario's findings did.
    machines_same: bool,
    /// The files that moved, from where to where.
    moves: Vec<(&'a str, &'a str)>,
}

impl Versions<'_> {
    /// What changed from `a` to `b`, the same diagram.
    fn compared(&self, a: &Unit, b: &Unit) -> Vec<Change> {
        let mut out = Vec::new();
        if a.file != b.file {
            out.push(change(
                ChangeKind::Changed,
                format!("Moved from {}.", a.file),
                Some(a.first()),
                Some(b.first()),
            ));
        }
        let same_text = a.code == b.code && a.language == b.language;
        let scenario = b.kind.kind == DiagramKind::Sequence;
        if same_text && (self.machines_same || !scenario) {
            return out;
        }
        if !a.behavior() && !b.behavior() {
            if same_lines(&a.code, &b.code) {
                return out;
            }
            let message = if a.kind.name != b.kind.name && !a.kind.name.is_empty() {
                format!(
                    "It is now {}; it was {}.",
                    a_kind(&b.kind.name),
                    a_kind(&a.kind.name)
                )
            } else {
                format!(
                    "Lines changed; Omaspec compares the behavior of state machines and sequence diagrams, not of {}.",
                    a_kind(&b.kind.name)
                )
            };
            out.push(change(
                ChangeKind::Changed,
                message,
                Some(a.first()),
                Some(b.first()),
            ));
            return out;
        }
        match between(
            (&a.code, a.language, self.old),
            (&b.code, b.language, self.new),
        ) {
            Ok(mut changes) => {
                self.unmoved(&mut changes);
                out.extend(changes.into_iter().map(|c| a.placed(b, c)));
            }
            Err(reason) => out.push(change(
                ChangeKind::Changed,
                format!("{reason}."),
                Some(a.first()),
                Some(b.first()),
            )),
        }
        out
    }

    /// Leaves out a finding resolved and found again that names the file
    /// of a state machine that only moved.
    fn unmoved(&self, changes: &mut Vec<Change>) {
        if self.moves.is_empty() {
            return;
        }
        let mut gone = vec![false; changes.len()];
        for i in 0..changes.len() {
            if changes[i].kind != ChangeKind::Resolved {
                continue;
            }
            let mut moved = changes[i].message.clone();
            for (from, to) in &self.moves {
                moved = moved.replace(&format!("({from})"), &format!("({to})"));
            }
            if moved == changes[i].message {
                continue;
            }
            let moved = without_lines(&moved);
            let again = (0..changes.len()).find(|&j| {
                !gone[j]
                    && changes[j].kind == ChangeKind::Found
                    && changes[j].severity == changes[i].severity
                    && without_lines(&changes[j].message) == moved
            });
            if let Some(j) = again {
                gone[i] = true;
                gone[j] = true;
            }
        }
        let mut gone = gone.into_iter();
        changes.retain(|_| !gone.next().unwrap_or(false));
    }

    /// A new diagram, and what the checks find in it.
    fn added(&self, b: &Unit) -> Vec<Change> {
        let mut out = vec![change(
            ChangeKind::Added,
            format!("New {}.", b.called()),
            None,
            Some(b.first()),
        )];
        let found = findings(Vec::new(), checked(b, self.new));
        out.extend(found.into_iter().map(|c| b.placed(b, c)));
        out
    }

    /// A removed diagram, and what the checks found in it.
    fn removed(&self, a: &Unit) -> Vec<Change> {
        let mut out = vec![change(
            ChangeKind::Removed,
            format!("{} removed.", capital(&a.called())),
            Some(a.first()),
            None,
        )];
        let resolved = findings(checked(a, self.old), Vec::new());
        out.extend(resolved.into_iter().map(|c| a.placed(a, c)));
        out
    }
}

/// What the specification checks find in `u`: a state machine's in itself,
/// a sequence diagram's against `specs`.
fn checked(u: &Unit, specs: &Specs) -> Vec<Finding> {
    match u.kind.kind {
        DiagramKind::State => Machine::read(&u.code, u.language)
            .map(|m| check::machine(&m))
            .unwrap_or_default(),
        DiagramKind::Sequence => across::check(&u.code, u.language, specs),
        _ => Vec::new(),
    }
}

/// Whether `a` and `b` have the same lines, blank lines and indentation
/// aside.
fn same_lines(a: &str, b: &str) -> bool {
    significant(a).eq(significant(b))
}

fn significant(text: &str) -> impl Iterator<Item = &str> {
    text.lines().map(str::trim).filter(|l| !l.is_empty())
}

#[cfg(test)]
mod tests;
