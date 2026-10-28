//! A list to choose from by typing: Ctrl+P finds a file in the folder or
//! among the open tabs and Ctrl+Shift+P a command, both by a few letters;
//! Ctrl+Shift+F lists what the text of all tabs holds.

use std::path::PathBuf;

use iced::Task;

use super::compare::Target;
use super::doc::Doc;
use super::{App, Command, FLASH, Message, PALETTE, PICKER, compare, folder, home_relative};
use crate::worker::{Alarm, blocking};

/// Rows on screen at once.
pub const SHOWN: usize = 12;
/// Commands the palette remembers as chosen lately.
const RECENT: usize = 4;
/// Matches kept; nobody scrolls further.
const KEPT: usize = 200;

/// What choosing an item does.
#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    Open(PathBuf),
    Run(Command),
    /// Select `range` (bytes) in the tab with this key.
    Show {
        doc: u64,
        range: std::ops::Range<usize>,
    },
    /// Compare the tab with this version.
    Compare(super::compare::Target),
    /// Play the recorded trace of `object` in this file.
    Trace {
        path: PathBuf,
        object: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Files,
    Commands,
    /// Versions to compare with.
    Versions,
    /// Recorded traces to play.
    Traces,
    /// The owner lists the items for each query; no fuzzy filter.
    Search,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub label: String,
    /// Shown dimmed on the right, e.g. the command's keys.
    pub hint: String,
    pub choice: Choice,
}

pub struct Picker {
    pub kind: Kind,
    pub query: String,
    /// Shown while nothing is typed.
    pub placeholder: &'static str,
    /// The replacement, when the search replaces too.
    pub replace: Option<String>,
    /// Whether the replacement has the keyboard.
    pub on_replace: bool,
    items: Vec<Item>,
    /// How many of the first items were chosen lately (commands).
    recent: usize,
    /// Indices into `items`, best match first.
    matches: Vec<usize>,
    pub selected: usize,
}

impl Picker {
    pub fn new(kind: Kind, placeholder: &'static str, items: Vec<Item>) -> Self {
        let mut picker = Self {
            kind,
            query: String::new(),
            placeholder,
            replace: None,
            on_replace: false,
            items,
            recent: 0,
            matches: Vec::new(),
            selected: 0,
        };
        picker.refilter();
        picker
    }

    /// The commands, the first `recent` of them chosen lately: they lead
    /// the list while nothing is typed.
    pub fn commands(items: Vec<Item>, recent: usize) -> Self {
        let mut picker = Self::new(Kind::Commands, "type a command", items);
        picker.recent = recent;
        picker
    }

    /// Files: the label is the path as shown.
    pub fn files(files: Vec<(String, PathBuf)>) -> Self {
        let items = files
            .into_iter()
            .map(|(label, path)| Item {
                label,
                hint: String::new(),
                choice: Choice::Open(path),
            })
            .collect();
        Self::new(Kind::Files, "find a file", items)
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.refilter();
    }

    /// New items, all shown in their order (a search lists them anew for
    /// each query).
    pub fn set_items(&mut self, items: Vec<Item>) {
        self.items = items;
        self.refilter();
    }

    /// More items, found later (git's commits).
    pub fn add_items(&mut self, items: Vec<Item>) {
        self.items.extend(items);
        let selected = self.selected;
        self.refilter();
        self.selected = selected.min(self.matches.len().saturating_sub(1));
    }

    /// Best match first; equal scores keep the given order.
    fn refilter(&mut self) {
        self.selected = 0;
        // Commands keep their order (the recent ones first) until a letter
        // is typed.
        let untyped = self.kind == Kind::Commands && self.query.trim().is_empty();
        if self.kind == Kind::Search || untyped {
            self.matches = (0..self.items.len()).collect();
            return;
        }
        let mut scored: Vec<(i32, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| Some((folder::score(&self.query, &item.label)?, i)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        self.matches = scored.into_iter().take(KEPT).map(|(_, i)| i).collect();
        self.selected = 0;
    }

    /// Moves the selection, wrapping around.
    pub fn move_by(&mut self, delta: isize) {
        let n = self.matches.len() as isize;
        if n > 0 {
            self.selected = (self.selected as isize + delta).rem_euclid(n) as usize;
        }
    }

    pub fn select(&mut self, index: usize) {
        if index < self.matches.len() {
            self.selected = index;
        }
    }

    pub fn chosen(&self) -> Option<&Choice> {
        let &i = self.matches.get(self.selected)?;
        Some(&self.items[i].choice)
    }

    pub fn count(&self) -> usize {
        self.matches.len()
    }

    /// Whether the item in `row` was chosen lately.
    pub fn is_recent(&self, row: usize) -> bool {
        self.matches.get(row).is_some_and(|&i| i < self.recent)
    }

    /// Where the typed letters are in `label`, to light them up.
    pub fn lit(&self, label: &str) -> Vec<usize> {
        match self.kind {
            Kind::Search => Vec::new(),
            _ => folder::matched(&self.query, label).unwrap_or_default(),
        }
    }

    /// The rows on screen: their place among the matches and their item,
    /// a window that keeps the selection in sight.
    pub fn visible(&self) -> impl Iterator<Item = (usize, &Item)> {
        let first = self.selected.saturating_sub(SHOWN - 1);
        self.matches
            .iter()
            .enumerate()
            .skip(first)
            .take(SHOWN)
            .map(|(row, &i)| (row, &self.items[i]))
    }
}

impl App {
    /// The command palette.
    pub(super) fn open_palette(&mut self) -> Task<Message> {
        let mut items = self.palette_items();
        // The commands chosen lately first, the latest on top.
        let mut recent = Vec::new();
        for command in &self.recent {
            if let Some(i) = items
                .iter()
                .position(|item| item.choice == Choice::Run(*command))
            {
                recent.push(items.remove(i));
            }
        }
        let count = recent.len();
        recent.extend(items);
        self.place_picker(Picker::commands(recent, count));
        iced::widget::operation::focus(PICKER)
    }

    /// Remembers that `command` was chosen from the palette.
    fn chose(&mut self, command: Command) {
        self.recent.retain(|c| *c != command);
        self.recent.insert(0, command);
        self.recent.truncate(RECENT);
    }

    /// Every command for the palette, named for the state it is in.
    pub(super) fn palette_items(&self) -> Vec<Item> {
        PALETTE
            .iter()
            .map(|&(label, keys, command)| {
                let (label, command) = match command {
                    Command::FollowTheme if self.follow_theme => {
                        ("Diagram theme: following Omarchy".to_owned(), command)
                    }
                    Command::OpenHelp if self.help.is_some() => {
                        ("Leave Help Omagram".to_owned(), Command::LeaveHelp)
                    }
                    _ => (label.to_owned(), command),
                };
                Item {
                    label,
                    hint: keys.to_owned(),
                    choice: Choice::Run(command),
                }
            })
            .collect()
    }

    /// Drops the file picker.
    pub(super) fn close_picker(&mut self) {
        self.picker = None;
        self.picker_on = None;
    }

    /// Draws `picker` in the window the command came from.
    pub(super) fn place_picker(&mut self, picker: Picker) {
        self.prompt = None;
        self.gallery = None;
        self.picker_on = Some(self.command_window.unwrap_or(self.main));
        self.picker = Some(picker);
    }

    /// The picker over the folder's diagrams and the open files elsewhere.
    pub(super) fn open_picker(&mut self) -> Task<Message> {
        let mut candidates: Vec<(String, PathBuf)> = match &self.folder {
            Some(folder) => self
                .files
                .iter()
                .map(|f| (f.display().to_string(), folder.join(f)))
                .collect(),
            None => Vec::new(),
        };
        for path in self.docs.iter().filter_map(|d| d.path.as_ref()) {
            if !candidates.iter().any(|(_, p)| p == path) {
                candidates.push((home_relative(path), path.clone()));
            }
        }
        self.place_picker(Picker::files(candidates));
        iced::widget::operation::focus(PICKER)
    }

    pub(super) fn pick(&mut self) -> Task<Message> {
        // A choice can open another list. It stays in this window.
        let host = self.picker_on.take();
        let from_palette = self
            .picker
            .as_ref()
            .is_some_and(|p| p.kind == Kind::Commands);
        let chosen = self.picker.take().and_then(|p| p.chosen().cloned());
        if let (true, Some(Choice::Run(command))) = (from_palette, &chosen) {
            self.chose(*command);
        }
        match chosen {
            Some(Choice::Open(path)) => self.open(&path),
            Some(Choice::Compare(target)) => {
                let key = self.doc().key;
                match &target {
                    Target::Tab(other) => {
                        let text = self.docs.iter().find(|d| d.key == *other).map(Doc::text);
                        if let Some(text) = text {
                            self.start_compare(key, target, text);
                        }
                        self.focus_editor()
                    }
                    Target::Commit { rev, .. } => {
                        let Some(path) = self.doc().path.clone() else {
                            return Task::none();
                        };
                        let rev = rev.clone();
                        let focus = self.focus_editor();
                        let read = Task::perform(
                            blocking(move || {
                                compare::show(&path, &rev).map_err(|e| format!("{e:#}"))
                            }),
                            move |text| {
                                Message::Version(
                                    key,
                                    target.clone(),
                                    text.unwrap_or_else(|| Err("git failed".into())),
                                )
                            },
                        );
                        Task::batch([focus, read])
                    }
                }
            }
            Some(Choice::Trace { path, object }) => {
                let played = self.play_trace(&path, &object);
                Task::batch([self.focus_editor(), played])
            }
            Some(Choice::Show { doc, range }) => {
                let Some(index) = self.docs.iter().position(|d| d.key == doc) else {
                    return self.focus_editor();
                };
                let shown = self.select(index);
                self.doc_mut().show_range(range);
                self.after(Alarm::Flash(doc), FLASH);
                shown
            }
            Some(Choice::Run(command)) => {
                let sets_area = matches!(command, Command::Focus(_));
                self.command_window = host;
                let ran = self.run(command);
                self.command_window = None;
                if sets_area {
                    ran
                } else {
                    Task::batch([self.focus_editor(), ran])
                }
            }
            None => self.focus_editor(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn picker() -> Picker {
        Picker::files(
            [
                "docs/flow.mmd",
                "auth/login.puml",
                "logs/in.puml",
                "state.puml",
            ]
            .iter()
            .map(|s| ((*s).to_owned(), PathBuf::from("/w").join(s)))
            .collect(),
        )
    }

    fn chosen(p: &Picker) -> Option<&Path> {
        match p.chosen()? {
            Choice::Open(path) => Some(path),
            Choice::Run(_) | Choice::Show { .. } | Choice::Compare(_) | Choice::Trace { .. } => {
                None
            }
        }
    }

    #[test]
    fn typing_narrows_and_ranks() {
        let mut p = picker();
        assert_eq!(p.count(), 4);
        p.set_query("login".into());
        assert_eq!(p.count(), 2);
        assert_eq!(chosen(&p), Some(Path::new("/w/auth/login.puml")));
        p.move_by(1);
        assert_eq!(chosen(&p), Some(Path::new("/w/logs/in.puml")));
        p.move_by(1);
        assert_eq!(chosen(&p), Some(Path::new("/w/auth/login.puml")));
        p.set_query("zzz".into());
        assert_eq!(p.chosen(), None);
    }

    #[test]
    fn the_window_follows_the_selection() {
        let mut p = Picker::files(
            (0..40)
                .map(|i| (format!("f{i:02}.puml"), PathBuf::from(format!("/f{i}"))))
                .collect(),
        );
        assert_eq!(p.visible().count(), SHOWN);
        p.move_by(-1);
        let rows: Vec<usize> = p.visible().map(|(row, _)| row).collect();
        assert_eq!(rows.last(), Some(&39));
    }
}
