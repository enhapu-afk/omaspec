//! A diagram on a pane without a render thread or a window: what the
//! tests and `omaspec play FILE -o play.gif` need to show one and draw
//! its pixels here and now.

use std::sync::Arc;

use iced::Size;
use omaspec_core::Language;
use omaspec_render::RenderError;

use super::view::Glide;
use super::{Message, Paper, raster};
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
        self.fold_spots()
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

    /// Rasterizes the overview the view asked for and hands it in as
    /// the background task's message would, with the pictures a glide
    /// needs.
    pub fn deliver_overview(&mut self) {
        let Some(tree) = self.shown_tree().map(Arc::clone) else {
            return;
        };
        if let Some(plan) = self.overview.asked {
            let raster = raster::render(&tree, plan);
            let _ = self.update(Message::Overview(self.overview.seq(), raster));
        }
        self.deliver_bare();
    }

    /// The pictures without lines a glide asked for, rasterized here.
    pub fn deliver_bare(&mut self) {
        let old = self
            .previous
            .as_ref()
            .and_then(|previous| previous.bare.clone())
            .zip(self.bare_old.asked);
        let new = self
            .diagram
            .as_ref()
            .and_then(|diagram| diagram.drawn.bare.clone())
            .zip(self.bare_new.asked);
        if let Some((tree, plan)) = old {
            let raster = raster::render(&tree, plan);
            let _ = self.update(Message::BareOld(self.bare_old.seq(), raster));
        }
        if let Some((tree, plan)) = new {
            let raster = raster::render(&tree, plan);
            let _ = self.update(Message::BareNew(self.bare_new.seq(), raster));
        }
    }

    /// How many pictures of the diagram before fade out, if a fade runs.
    pub fn fading(&self) -> Option<usize> {
        self.fade.as_ref().map(|fade| fade.old.len())
    }

    /// The pane's size.
    pub fn viewport(&self) -> Size {
        self.viewport
    }

    /// Makes the smaller copies a zoom into a tile or out of it asked
    /// for, as the background task would.
    pub fn finish_small(&mut self) {
        if let Some(passage) = self.passage.as_mut().filter(|p| p.small_of.is_some()) {
            let picture = passage.linked.as_ref().and_then(|l| l.picture.as_ref());
            passage.small = picture
                .map(raster::halves)
                .unwrap_or_default()
                .into_iter()
                .map(Some)
                .collect();
        }
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
