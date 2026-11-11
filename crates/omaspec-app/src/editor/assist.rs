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
use omaspec_model::assist::{self, Suggestion, Tone, What};
use omaspec_theme::tokens::Tones;
use scrive_core::{
    CompletionCx, CompletionItem, CompletionKind, CompletionTrigger, Completions, Hover, HoverCx,
    HoverInfo, InsertText,
};
use scrive_iced::Tint;

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

/// The colors of what the words of `text` are, for the editor to lay over
/// the grammar's: in a diagram file, or in each fence of a document.
pub(super) fn tints(text: &str, holds: Holds, specs: &Specs, tones: &Tones) -> Vec<Tint> {
    let tint = |line: usize, mark: assist::Mark| {
        let color = match mark.tone {
            Tone::Name => tones.name,
            Tone::Event => tones.event,
            Tone::Guard => tones.guard,
            Tone::Action => tones.action,
        };
        Some(Tint {
            row: u32::try_from(line).ok()?,
            range: u32::try_from(mark.range.start).ok()?..u32::try_from(mark.range.end).ok()?,
            color: crate::ui::color(color),
        })
    };
    match holds {
        Holds::Diagram(language) => assist::marks(text, language, specs)
            .into_iter()
            .filter_map(|mark| tint(mark.line, mark))
            .collect(),
        Holds::Document => markdown::blocks(text)
            .iter()
            .flat_map(|block| {
                assist::marks(&block.code, block.language, specs)
                    .into_iter()
                    .filter_map(|mark| tint(block.doc_line(mark.line), mark))
                    .collect::<Vec<_>>()
            })
            .collect(),
        Holds::Other => Vec::new(),
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

    // The colors laid over the grammar's (`scrive_iced::tint`, ours in
    // `vendor/`, which the workspace does not test).
    use scrive_core::{HighlightSpan, Rgba, SpanStyle};
    use scrive_iced::tint::{of_row, over};
    use std::borrow::Cow;
    use std::ops::Range;
    type Color = iced::Color;

    const RED: Color = Color::from_rgb(1.0, 0.0, 0.0);
    const GRAY: Color = Color::from_rgb(0.5, 0.5, 0.5);

    fn span(range: Range<u32>, bold: bool) -> HighlightSpan {
        HighlightSpan {
            range,
            style: SpanStyle {
                fg: Rgba {
                    r: 9,
                    g: 9,
                    b: 9,
                    a: 255,
                },
                bold,
                italic: false,
            },
        }
    }

    fn tint(row: u32, range: Range<u32>) -> Tint {
        Tint {
            row,
            range,
            color: RED,
        }
    }

    #[test]
    fn a_tint_colors_its_part_and_keeps_the_rest() {
        let spans = [span(0..4, false), span(4..10, true)];
        let out = over("Idle --> X", Some(&spans), &[tint(0, 2..6)], GRAY).unwrap();
        let got: Vec<(Range<u32>, u8, bool)> = out
            .iter()
            .map(|s| (s.range.clone(), s.style.fg.r, s.style.bold))
            .collect();
        assert_eq!(
            got,
            [
                (0..2, 9, false),
                (2..4, 255, false),
                (4..6, 255, true),
                (6..10, 9, true)
            ]
        );
    }

    #[test]
    fn a_line_without_spans_is_one_run_in_the_plain_color() {
        let out = over("Idle", None, &[tint(0, 0..2)], GRAY).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].style.fg.r, 255);
        assert_eq!((out[1].range.clone(), out[1].style.fg.r), (2..4, 128));
        assert!(over("Idle", None, &[], GRAY).is_none());
    }

    #[test]
    fn a_tint_that_does_not_fit_the_line_is_left_out() {
        let spans = [span(0..3, false)];
        // Past the end, and through the middle of "é".
        for stale in [tint(0, 2..9), tint(0, 1..2), tint(0, 2..2)] {
            let out = over("é!", Some(&spans[..1]), &[stale], GRAY).unwrap();
            assert!(matches!(out, Cow::Borrowed(_)));
        }
    }

    #[test]
    fn the_tints_of_a_row_are_found_among_all() {
        let all = [tint(1, 0..1), tint(3, 0..1), tint(3, 2..3), tint(7, 0..1)];
        assert_eq!(of_row(&all, 3).len(), 2);
        assert_eq!(of_row(&all, 2).len(), 0);
        assert_eq!(of_row(&all, 9).len(), 0);
        assert_eq!(of_row(&[], 0).len(), 0);
    }

    #[test]
    fn the_words_of_a_document_s_diagram_are_colored_on_its_lines() {
        let tones = omaspec_theme::tokens::tones(
            &omaspec_theme::Palette::fallback(),
            &std::collections::HashMap::new(),
        );
        let text = "# Door\n\n```plantuml\n[*] --> Idle\nIdle --> Busy : start [ready]\n```\n";
        let found = tints(text, Holds::Document, &Specs::default(), &tones);
        let rows: Vec<(u32, Range<u32>)> = found.iter().map(|t| (t.row, t.range.clone())).collect();
        assert_eq!(
            rows,
            [(3, 8..12), (4, 0..4), (4, 9..13), (4, 16..21), (4, 22..29)]
        );
        assert_eq!(found[3].color, crate::ui::color(tones.event));
        assert_eq!(found[4].color, crate::ui::color(tones.guard));
        assert!(tints(text, Holds::Other, &Specs::default(), &tones).is_empty());
    }

    // Motion in the editor (`scrive_iced::motion`, ours in `vendor/`).
    use scrive_iced::motion::{ARRIVE, Arrive, CARET, Glide, ease};
    use std::time::{Duration, Instant};

    #[test]
    fn the_caret_glides_to_where_it_goes_and_stands_after_a_scroll() {
        let start = Instant::now();
        let still = (0.0, 0.0);
        let mut glide = Glide::default();
        // It starts in its place, and stays while nothing moves it.
        assert_eq!(glide.at((10.0, 20.0), still, start, true), (10.0, 20.0));
        assert!(!glide.running(start));
        // A new place: it sets out from where it was, is on its way, and
        // arrives.
        assert_eq!(glide.at((110.0, 20.0), still, start, true), (10.0, 20.0));
        assert!(glide.running(start));
        let half = glide.at((110.0, 20.0), still, start + CARET / 2, true);
        assert!(half.0 > 60.0 && half.0 < 110.0, "{half:?}: fast at first");
        assert_eq!(
            glide.at((110.0, 20.0), still, start + CARET, true),
            (110.0, 20.0)
        );
        assert!(!glide.running(start + CARET));
        // Moved again on its way, it goes on from where it is.
        let _ = glide.at((10.0, 20.0), still, start + CARET, true);
        let on_its_way = glide.at((10.0, 20.0), still, start + CARET + CARET / 2, true);
        assert_eq!(
            glide.at((10.0, 60.0), still, start + CARET + CARET / 2, true),
            on_its_way
        );
        // Scrolled text takes its caret along at once; so does no motion.
        let later = start + CARET * 4;
        assert_eq!(
            glide.at((10.0, 90.0), (3.0, 0.0), later, true),
            (10.0, 90.0)
        );
        assert_eq!(
            glide.at((50.0, 90.0), (3.0, 0.0), later, false),
            (50.0, 90.0)
        );
        assert!(!glide.running(later));
    }

    #[test]
    fn the_list_arrives_once_and_anew_after_it_closed() {
        let start = Instant::now();
        let mut arrive = Arrive::default();
        assert!(!arrive.done(start), "closed");
        assert_eq!(arrive.shown(start, true), 0.0);
        let half = arrive.shown(start + ARRIVE / 2, true);
        assert!(half > 0.5 && half < 1.0, "{half}");
        assert_eq!(arrive.shown(start + ARRIVE, true), 1.0);
        assert!(arrive.done(start + ARRIVE));
        arrive.closed();
        assert_eq!(arrive.shown(start + ARRIVE * 2, true), 0.0);
        arrive.closed();
        assert_eq!(arrive.shown(start + ARRIVE * 3, false), 1.0, "no motion");
        assert_eq!((ease(0.0), ease(1.0), ease(7.0)), (0.0, 1.0, 1.0));
        assert!(ARRIVE > Duration::ZERO);
    }
}
