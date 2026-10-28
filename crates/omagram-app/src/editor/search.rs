//! Search over the text of all tabs: plain text, case-sensitive only when
//! the query has a capital letter.

use std::ops::Range;

use iced::Task;
use regex::{Regex, RegexBuilder};

use super::picker::{Choice, Item, Kind, Picker};
use super::{App, Message, PICKER, REPLACE, plural};

/// Hits listed at most, over all tabs.
pub const MAX_HITS: usize = 1000;

/// What to look for; `None` for an empty query.
pub fn matcher(query: &str) -> Option<Regex> {
    if query.is_empty() {
        return None;
    }
    let sensitive = query.chars().any(char::is_uppercase);
    RegexBuilder::new(&regex::escape(query))
        .case_insensitive(!sensitive)
        .build()
        .ok()
}

/// One place `matcher` matched.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// Zero-based line.
    pub line: usize,
    /// Byte range in the whole text.
    pub range: Range<usize>,
    /// The line, trimmed, for the list.
    pub text: String,
}

/// The matches in `text`, at most `limit`.
pub fn hits(text: &str, matcher: &Regex, limit: usize) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut start = 0;
    for (line, content) in text.split('\n').enumerate() {
        for found in matcher.find_iter(content) {
            if hits.len() >= limit {
                return hits;
            }
            hits.push(Hit {
                line,
                range: start + found.start()..start + found.end(),
                text: content.trim().to_owned(),
            });
        }
        start += content.len() + 1;
    }
    hits
}

/// The edits that replace every match in `text` with `with`, in order.
pub fn replacements(text: &str, matcher: &Regex, with: &str) -> Vec<(Range<usize>, String)> {
    matcher
        .find_iter(text)
        .map(|m| (m.range(), with.to_owned()))
        .collect()
}

impl App {
    /// The search over all tabs, starting from the selected text; with
    /// `replace` also the replacement.
    pub(super) fn open_search(&mut self, replace: bool) -> Task<Message> {
        let selection = self.doc().editor.selection();
        let query = self
            .doc()
            .editor
            .document()
            .buffer()
            .slice(selection)
            .into_owned();
        let query = if query.contains('\n') {
            String::new()
        } else {
            query
        };
        let mut picker = Picker::new(Kind::Search, "find in all tabs", Vec::new());
        picker.replace = replace.then(String::new);
        picker.set_items(self.search_items(&query));
        picker.query = query;
        self.place_picker(picker);
        iced::widget::operation::focus(PICKER)
    }

    /// The hits of `query` in every tab, this one first.
    pub(super) fn search_items(&self, query: &str) -> Vec<Item> {
        let Some(matcher) = matcher(query) else {
            return Vec::new();
        };
        let order =
            std::iter::once(self.active).chain((0..self.docs.len()).filter(|&i| i != self.active));
        let mut items = Vec::new();
        for doc in order.map(|i| &self.docs[i]) {
            let left = MAX_HITS.saturating_sub(items.len());
            let name = doc.name();
            items.extend(
                hits(&doc.text(), &matcher, left)
                    .into_iter()
                    .map(|hit| Item {
                        label: hit.text,
                        hint: format!("{name}:{}", hit.line + 1),
                        choice: Choice::Show {
                            doc: doc.key,
                            range: hit.range,
                        },
                    }),
            );
        }
        items
    }

    /// Tab moves between the search and the replacement.
    pub(super) fn switch_field(&mut self) -> Task<Message> {
        let Some(picker) = &mut self.picker else {
            return Task::none();
        };
        if picker.replace.is_none() {
            return Task::none();
        }
        picker.on_replace = !picker.on_replace;
        iced::widget::operation::focus(if picker.on_replace { REPLACE } else { PICKER })
    }

    /// Replaces every match in every tab, each tab one undo step.
    pub(super) fn replace_all(&mut self) -> Task<Message> {
        let Some(picker) = self.picker.take() else {
            return Task::none();
        };
        let (Some(matcher), Some(with)) = (matcher(&picker.query), picker.replace.clone()) else {
            self.picker = Some(picker);
            return Task::none();
        };
        let (mut count, mut tabs) = (0, Vec::new());
        for doc in &mut self.docs {
            let edits = replacements(&doc.text(), &matcher, &with);
            if !edits.is_empty() {
                count += edits.len();
                doc.replace(edits);
                tabs.push(doc.key);
            }
        }
        for &key in &tabs {
            self.edited(key);
        }
        self.picker_on = None;
        self.notice = Some(match count {
            0 => "nothing to replace".to_owned(),
            n => format!(
                "replaced {n} in {} {} (Ctrl+Z in a tab undoes it there)",
                tabs.len(),
                plural(tabs.len(), "tab")
            ),
        });
        self.focus_editor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_counts_only_with_a_capital() {
        let text = "Alice -> Bob\nalice -> carol";
        let lower = matcher("alice").unwrap();
        assert_eq!(hits(text, &lower, 10).len(), 2);
        let upper = matcher("Alice").unwrap();
        let found = hits(text, &upper, 10);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].line, found[0].range.clone()), (0, 0..5));
        assert!(matcher("").is_none());
    }

    #[test]
    fn finds_plain_text_with_its_line() {
        let text = "a.b\n  x (a.b) a.b\nab";
        let m = matcher("a.b").unwrap();
        let found = hits(text, &m, 10);
        let lines: Vec<(usize, &str)> = found
            .iter()
            .map(|h| (h.line, &text[h.range.clone()]))
            .collect();
        assert_eq!(lines, [(0, "a.b"), (1, "a.b"), (1, "a.b")]);
        assert_eq!(found[1].text, "x (a.b) a.b");
        assert_eq!(hits(text, &m, 2).len(), 2);
    }

    #[test]
    fn replaces_every_match() {
        let m = matcher("bob").unwrap();
        let edits = replacements("Bob -> bob\n", &m, "Dave");
        assert_eq!(
            edits,
            [(0..3, "Dave".to_owned()), (7..10, "Dave".to_owned())]
        );
    }
}
