//! A diagram on a pane without a render thread or a window: what the
//! tests and `omaspec play FILE -o play.gif` need to show one and draw
//! its pixels here and now.

use iced::Size;
use omaspec_core::Language;
use omaspec_render::RenderError;

use super::Paper;
use super::view::Glide;
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

    /// The containers the picture offers to fold, and those folded in it.
    pub fn containers(&self) -> Vec<String> {
        self.containers_to_fold()
            .into_iter()
            .map(|spot| spot.name)
            .collect()
    }

    /// The containers folded, as the next drawing is asked for.
    pub fn folded(&self) -> &[String] {
        &self.folds
    }

    /// The glide under way, a dive too.
    pub fn glide(&self) -> Option<Glide> {
        self.glide
    }

    /// How many pictures of the diagram before fade out, if a fade runs.
    pub fn fading(&self) -> Option<usize> {
        self.arrival.fade().map(|fade| fade.old.len())
    }

    /// The pane's size.
    pub fn viewport(&self) -> Size {
        self.viewport
    }
}
