//! What the editor offers while a diagram is written, and what it says
//! about the name under the pointer: `omaspec_model::assist`, asked by the
//! editor widget.
//!
//! The widget asks while it takes a key, before the tab has seen the edit,
//! so what a tab knows is kept here for it: its text as of the edit before
//! (the line the caret is in is not read anyway), its language and its
//! project's state machines.

use std::sync::{Arc, Mutex, PoisonError};

use omaspec_core::{Language, markdown};
use omaspec_model::Specs;
use omaspec_model::assist::{self, Suggestion, What};
use scrive_core::{
    CompletionCx, CompletionItem, CompletionKind, CompletionTrigger, Completions, Hover, HoverCx,
    HoverInfo, InsertText,
};

/// What a tab holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Holds {
    Diagram(Language),
    /// A Markdown document: each fence has its diagram and its language.
    Document,
    /// Nothing to assist with, as a project's file.
    Other,
}

#[derive(Debug)]
struct Known {
    text: String,
    holds: Holds,
    specs: Arc<Specs>,
}

/// What a tab knows, shared with the two askers its editor holds.
#[derive(Debug, Clone)]
pub(super) struct Shared(Arc<Mutex<Known>>);

impl Default for Shared {
    fn default() -> Self {
        Shared(Arc::new(Mutex::new(Known {
            text: String::new(),
            holds: Holds::Other,
            specs: Arc::default(),
        })))
    }
}

impl Shared {
    /// The tab's text, what it holds and its project's machines, as of now.
    pub(super) fn tell(&self, text: &str, holds: Holds, specs: &Arc<Specs>) {
        let mut known = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if known.text != text {
            text.clone_into(&mut known.text);
        }
        known.holds = holds;
        known.specs = specs.clone();
    }

    /// Who offers what to write.
    pub(super) fn offers(&self) -> impl Completions + 'static {
        Offers(self.clone())
    }

    /// Who says what a name is.
    pub(super) fn says(&self) -> impl Hover + 'static {
        Says(self.clone())
    }

    /// The diagram that holds the tab's line `row`, its language and the
    /// line in it.
    fn diagram_at(&self, row: usize) -> Option<(String, Language, usize, Arc<Specs>)> {
        let known = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match known.holds {
            Holds::Diagram(language) => {
                Some((known.text.clone(), language, row, known.specs.clone()))
            }
            Holds::Document => {
                let block = markdown::blocks(&known.text)
                    .into_iter()
                    .find(|b| b.lines.contains(&row))?;
                let line = (0..block.code.split('\n').count())
                    .find(|line| block.doc_line(*line) == row)
                    .filter(|_| row > block.lines.start)?;
                Some((block.code, block.language, line, known.specs.clone()))
            }
            Holds::Other => None,
        }
    }
}

struct Offers(Shared);

impl Completions for Offers {
    fn complete(&mut self, cx: &CompletionCx) -> Vec<CompletionItem> {
        let Some((code, language, line, specs)) = self.0.diagram_at(cx.position.row as usize)
        else {
            return Vec::new();
        };
        let before = cx.lookback.rsplit('\n').next().unwrap_or_default();
        let asked = cx.trigger == CompletionTrigger::Manual;
        assist::suggest(&code, language, line, before, asked, &specs)
            .into_iter()
            .enumerate()
            .map(|(at, s)| item(at, s))
            .collect()
    }
}

/// A suggestion as the widget lists it: in its tier, then as it came.
fn item(at: usize, s: Suggestion) -> CompletionItem {
    let kind = match s.what {
        What::Name => CompletionKind::Symbol,
        What::Event => CompletionKind::Event,
        What::Machine => CompletionKind::Type,
        What::Keyword => CompletionKind::Keyword,
        What::Block => CompletionKind::Construct,
    };
    let insert = match s.snippet {
        Some(text) => InsertText::Snippet(text),
        None => InsertText::Plain(s.label.clone()),
    };
    let item =
        CompletionItem::new(s.label, kind, insert).with_sort_key(format!("{}_{at:05}", s.tier));
    if s.detail.is_empty() {
        item
    } else {
        item.with_detail(s.detail)
    }
}

struct Says(Shared);

impl Hover for Says {
    fn hover(&mut self, cx: &HoverCx) -> Option<HoverInfo> {
        let (code, language, _, specs) = self.0.diagram_at(cx.position.row as usize)?;
        // The text before the pointer runs to the end of its word.
        let len = (cx.word.end - cx.word.start) as usize;
        let word = cx.lookback.get(cx.lookback.len().checked_sub(len)?..)?;
        let markdown = assist::describe(&code, language, word, &specs)?;
        Some(HoverInfo {
            markdown,
            range: cx.word.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scrive_core::Snippet;

    #[test]
    fn the_widget_reads_every_block() {
        let blocks = assist::block_texts();
        assert!(blocks.len() > 20);
        for text in blocks {
            let read = Snippet::parse(text).unwrap_or_else(|err| panic!("{text}: {err:?}"));
            assert!(!read.text.contains('$'), "{text}");
        }
    }

    #[test]
    fn a_document_s_line_is_its_diagram_s() {
        let shared = Shared::default();
        let text = "# Door\n\n```mermaid\nstateDiagram-v2\n  [*] --> Idle\n```\n\nprose\n";
        shared.tell(text, Holds::Document, &Arc::default());
        let (code, language, line, _) = shared.diagram_at(4).expect("in the fence");
        assert_eq!(language, Language::Mermaid);
        assert_eq!(code.split('\n').nth(line), Some("  [*] --> Idle"));
        assert!(shared.diagram_at(7).is_none(), "prose");
        assert!(shared.diagram_at(2).is_none(), "the fence itself");
        shared.tell(text, Holds::Other, &Arc::default());
        assert!(shared.diagram_at(4).is_none());
    }
}
