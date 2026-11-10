//! A diagram on a pane without a render thread or a window: what the
//! tests and `omaspec play FILE -o play.gif` need to show one and draw
//! its pixels here and now.

use std::sync::Arc;

use omaspec_core::Language;
use omaspec_render::RenderError;

use super::Paper;
use crate::worker::{Answer, Drawn, Ticket};

/// A render thread's answer for the request `id` of the tab `doc`.
pub fn answer(doc: u64, id: u64, result: Result<Drawn, RenderError>) -> Answer {
    Answer {
        ticket: Ticket { doc, id },
        source: "".into(),
        language: Language::Mermaid,
        result,
    }
}

impl Paper {
    /// Pretends one more request went out; its number.
    pub fn pretend_next(&mut self) -> u64 {
        self.requested += 1;
        self.requested
    }

    /// What the play offers now, if one runs.
    pub fn offer(&self) -> Option<super::Offer> {
        self.play.as_ref().map(super::Play::offer)
    }

    /// Whether the play's light is still on its way at `now`.
    pub fn play_moving(&self, now: std::time::Instant) -> bool {
        self.play.as_ref().is_some_and(|play| play.animating(now))
    }

    /// Rasterizes what the pane asked for, here and now, as the
    /// background task would.
    pub fn finish_rasters(&mut self) {
        let Some(tree) = self.shown_tree().map(Arc::clone) else {
            return;
        };
        self.overview.finish(&tree);
        self.detail.finish(&tree);
    }
}
