//! *Rename everywhere* (F2, `docs/adr/0019-where-a-name-is-used.md`): the
//! diagram, state or event named at the caret renamed at every use in the
//! project. An open document takes the rename as one undo step and stays
//! unsaved; a file not open is written as a save writes it.

use std::path::PathBuf;

use iced::Task;
use omaspec_model::{File, Named, Plan};

use super::{App, Asking, Message};
use crate::names::{self, Owned};

/// A rename asked for: what is at the caret, in which project.
pub struct Renaming {
    pub root: PathBuf,
    pub named: Named,
}

impl App {
    /// Asks for the new name of what the caret names.
    pub(super) fn start_rename(&mut self) -> Task<Message> {
        let doc = self.doc();
        let (Some(root), Some(path)) = (self.project_of(doc), doc.path.clone()) else {
            self.notice = Some("rename everywhere works on a saved file of a project".into());
            return Task::none();
        };
        let Some(file) = crate::specs::path_in(&root, &path) else {
            return Task::none();
        };
        let caret = doc.editor.cursor();
        let files = self.project_files(&root);
        let specs = names::specs_of(&files);
        let read: Vec<File> = files.iter().map(Owned::file).collect();
        let found =
            omaspec_model::named_at(&file, caret.row as usize, caret.col as usize, &read, &specs);
        let Some(named) = found else {
            self.notice = Some("no diagram, state or event is named at the caret".into());
            return Task::none();
        };
        self.renaming = Some(Renaming { root, named });
        self.ask(Asking::Rename)
    }

    /// Renames what the prompt was opened on to the name typed.
    pub(super) fn rename_submitted(&mut self) -> Task<Message> {
        let new = self
            .prompt
            .take()
            .map(|p| p.input.trim().to_owned())
            .unwrap_or_default();
        let Some(Renaming { root, named }) = self.renaming.take() else {
            return Task::none();
        };
        let files = self.project_files(&root);
        let specs = names::specs_of(&files);
        let read: Vec<File> = files.iter().map(Owned::file).collect();
        let plan = match omaspec_model::rename(
            &named.name,
            &new,
            named.machine.as_deref(),
            Some(named.thing),
            &read,
            &specs,
        ) {
            Ok(plan) => plan,
            Err(why) => {
                self.notice = Some(format!("not renamed: {why}"));
                return self.focus_editor();
            }
        };
        let (mut tabs, mut written, mut failed) = (0, 0, Vec::new());
        for file in plan.files() {
            let path = root.join(file);
            let open = self
                .docs
                .iter()
                .position(|d| d.path.as_deref() == Some(path.as_path()));
            let Some(text) = files.iter().find(|f| f.path == file).map(|f| &f.text) else {
                continue;
            };
            match open {
                Some(index) => {
                    let edits = edits_in(&plan, file, text);
                    let doc = &mut self.docs[index];
                    doc.replace(edits);
                    let key = doc.key;
                    self.edited(key);
                    tabs += 1;
                }
                None => {
                    match crate::files::write_atomic(&path, plan.apply(file, text).as_bytes()) {
                        Ok(()) => written += 1,
                        Err(err) => failed.push(format!("{file}: {err:#}")),
                    }
                }
            }
        }
        self.notice = Some(notice(&plan, &new, tabs, written, &failed));
        Task::batch([self.load_specs(), self.focus_editor()])
    }

    /// The project's files, an open document's as its tab has it.
    pub(super) fn project_files(&self, root: &std::path::Path) -> Vec<Owned> {
        let mut files = names::project_files(root);
        for file in &mut files {
            let path = root.join(&file.path);
            if let Some(doc) = self
                .docs
                .iter()
                .find(|d| d.path.as_deref() == Some(path.as_path()))
            {
                file.text = doc.text();
            }
        }
        files
    }
}

/// The plan's edits in the file at `file`, whose text is `text`, as byte
/// ranges of the whole text.
fn edits_in(plan: &Plan, file: &str, text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(at, _)| at + 1));
    plan.edits
        .iter()
        .filter(|e| e.file == file)
        .filter_map(|e| {
            let start = starts.get(e.line)?;
            Some((start + e.range.start..start + e.range.end, e.with.clone()))
        })
        .collect()
}

/// What the status line says after a rename.
fn notice(plan: &Plan, new: &str, tabs: usize, written: usize, failed: &[String]) -> String {
    let files = tabs + written;
    let mut out = format!(
        "renamed {} to {new} in {files} {}",
        plan.what,
        if files == 1 { "file" } else { "files" }
    );
    if tabs > 0 {
        out.push_str(&format!(
            " ({tabs} open, unsaved: Ctrl+Z in a tab undoes it there)"
        ));
    }
    if !plan.traces.is_empty() {
        let n = plan.traces.len();
        out.push_str(&format!(
            "; {n} {} of recorded traces left alone",
            if n == 1 { "line" } else { "lines" }
        ));
    }
    for note in &plan.notes {
        out.push_str("; ");
        out.push_str(note);
    }
    if !failed.is_empty() {
        out.push_str("; not written: ");
        out.push_str(&failed.join(", "));
    }
    out
}
