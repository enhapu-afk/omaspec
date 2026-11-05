//! Markdown documents of diagrams: every fenced code block whose info
//! string names PlantUML or Mermaid (```` ```plantuml ````, `puml`,
//! `mermaid`, `mmd`; `~~~` fences too) is a diagram, with its lines mapped
//! to the document's. Fences follow CommonMark: up to three spaces before
//! them, a closing fence of the same character at least as long, and a
//! block left open runs to the end of the document.
//!
//! A PlantUML block without `@start…` is a diagram's body, as GitLab and
//! Kroki draw it, and is read inside `@startuml` and `@enduml`.

use std::ops::Range;

use crate::lang::{Language, puml_start};
use crate::text::normalize_newlines;

/// One diagram of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub language: Language,
    /// The diagram as the engines read it.
    pub code: String,
    /// The document's lines the block holds, from its opening fence to its
    /// closing one (or the document's end), zero-based and exclusive.
    pub lines: Range<usize>,
    /// Whether `code` was put inside `@startuml` and `@enduml`.
    pub wrapped: bool,
}

impl Block {
    /// The document's line of `code`'s first line.
    fn first(&self) -> usize {
        self.lines.start + 1
    }

    /// The document's line for `line` of [`Self::code`]: a line the block
    /// added is its fence's.
    pub fn doc_line(&self, line: usize) -> usize {
        let line = if self.wrapped {
            match line.checked_sub(1) {
                Some(inside) => inside,
                None => return self.lines.start,
            }
        } else {
            line
        };
        (self.first() + line).min(self.lines.end.saturating_sub(1))
    }

    /// The line of [`Self::code`] for the document's `line`, when the line
    /// is the block's code.
    pub fn code_line(&self, line: usize) -> Option<usize> {
        let inside = line.checked_sub(self.first())?;
        let count = self.code.lines().count() - if self.wrapped { 2 } else { 0 };
        (inside < count).then_some(inside + usize::from(self.wrapped))
    }

    /// Whether the document's `line` lies in the block, its fences too.
    pub fn holds(&self, line: usize) -> bool {
        self.lines.contains(&line)
    }
}

/// Whether a file of this name is a Markdown document.
pub fn is_markdown(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".md", ".markdown"].iter().any(|ext| lower.ends_with(ext))
}

/// The diagrams of the Markdown document `text`, in order.
pub fn blocks(text: &str) -> Vec<Block> {
    let text = normalize_newlines(text);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut found = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let Some(open) = fence(lines[at]) else {
            at += 1;
            continue;
        };
        let start = at;
        let close = lines[at + 1..]
            .iter()
            .position(|line| closes(line, &open))
            .map(|i| start + 1 + i);
        let body_end = close.unwrap_or(lines.len());
        let end = close.map_or(lines.len(), |c| c + 1);
        if let Some(language) = language_of(open.info) {
            let body: Vec<&str> = lines[start + 1..body_end]
                .iter()
                .map(|line| unindent(line, open.indent))
                .collect();
            let wrapped = language == Language::PlantUml
                && !body
                    .iter()
                    .any(|line| puml_start().is_match(line.trim_start()));
            let mut code = body.join("\n");
            if wrapped {
                code = format!("@startuml\n{code}\n@enduml");
            }
            found.push(Block {
                language,
                code,
                lines: start..end,
                wrapped,
            });
        }
        at = end;
    }
    found
}

/// An opening fence: its character, length, indentation and info string.
struct Fence<'a> {
    char: char,
    len: usize,
    indent: usize,
    info: &'a str,
}

fn fence(line: &str) -> Option<Fence<'_>> {
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let char = rest.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let len = rest.len() - rest.trim_start_matches(char).len();
    let info = rest[len..].trim();
    // A backtick fence's info string holds no backtick.
    (len >= 3 && !(char == '`' && info.contains('`'))).then_some(Fence {
        char,
        len,
        indent,
        info,
    })
}

fn closes(line: &str, open: &Fence) -> bool {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    let len = rest.len() - rest.trim_start_matches(open.char).len();
    indent <= 3 && len >= open.len && rest[len..].trim().is_empty()
}

/// The language the info string's first word names: `plantuml`, `puml`,
/// `mermaid`, `mmd`, also as `{.plantuml}`.
fn language_of(info: &str) -> Option<Language> {
    let word = info
        .split(|c: char| c.is_whitespace() || c == '}' || c == ',')
        .next()?
        .trim_start_matches(['{', '.']);
    Language::from_id(word)
}

/// `line` without up to `indent` spaces, as CommonMark takes them off a
/// fenced block's lines.
fn unindent(line: &str, indent: usize) -> &str {
    let spaces = line.len() - line.trim_start_matches(' ').len();
    &line[spaces.min(indent)..]
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "# Door\n\
                       \n\
                       The door's states:\n\
                       \n\
                       ```plantuml\n\
                       @startuml\n\
                       [*] --> Closed\n\
                       @enduml\n\
                       ```\n\
                       \n\
                       ~~~~ mermaid\n\
                       sequenceDiagram\n\
                       A->>B: hi\n\
                       ~~~~\n\
                       \n\
                       ```rust\n\
                       fn main() {}\n\
                       ```\n\
                       \n\
                       ```puml\n\
                       Alice -> Bob\n\
                       ```\n";

    #[test]
    fn finds_the_diagrams_and_their_lines() {
        let found = blocks(DOC);
        assert_eq!(found.len(), 3);
        assert_eq!(found[0].language, Language::PlantUml);
        assert_eq!(found[0].code, "@startuml\n[*] --> Closed\n@enduml");
        assert_eq!(found[0].lines, 4..9);
        assert!(!found[0].wrapped);
        assert_eq!(found[0].doc_line(1), 6);
        assert_eq!(found[1].language, Language::Mermaid);
        assert_eq!(found[1].code, "sequenceDiagram\nA->>B: hi");
        assert_eq!(found[1].lines, 10..14);
        // A body without `@startuml`: the lines added are the fences'.
        let body = &found[2];
        assert!(body.wrapped);
        assert_eq!(body.code, "@startuml\nAlice -> Bob\n@enduml");
        assert_eq!(body.doc_line(0), 19);
        assert_eq!(body.doc_line(1), 20);
        assert_eq!(body.doc_line(2), 21);
        assert_eq!(body.code_line(20), Some(1));
        assert_eq!(body.code_line(19), None);
        assert_eq!(found[0].code_line(6), Some(1));
        assert!(found[1].holds(13) && !found[1].holds(14));
    }

    #[test]
    fn fences_follow_commonmark() {
        // Indented up to three spaces, the lines lose as much; a longer
        // fence closes, a shorter one or one with words after it does not.
        let doc = "   ```mermaid\n   pie\n     \"a\": 1\n   ``\n   ``` x\n   `````\nafter";
        let found = blocks(doc);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "pie\n  \"a\": 1\n``\n``` x");
        assert_eq!(found[0].lines, 0..6);
        // Four spaces are an indented code block, not a fence.
        assert!(blocks("    ```mermaid\n    pie\n    ```").is_empty());
        // Open to the end.
        let open = blocks("```{.mermaid}\npie\n");
        assert_eq!(open[0].code, "pie\n");
        assert_eq!(open[0].lines, 0..3);
        // A backtick in a backtick fence's info string makes it no fence.
        assert!(blocks("```mermaid`\npie\n```").is_empty());
        assert!(is_markdown("README.md") && is_markdown("notes.Markdown"));
        assert!(!is_markdown("flow.mmd"));
    }
}
