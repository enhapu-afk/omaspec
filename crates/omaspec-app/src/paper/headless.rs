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

    /// The pictures the pane may draw next: those shown or held back,
    /// those a fade, a glide or a zoom goes from and to, the pictures
    /// without lines it asked for, and the colors a glide fills holes with. A recording puts them on the GPU before
    /// each frame ([`crate::editor::Screen::upload`]), as the window waits
    /// for its pixels to be there.
    pub fn pictures(&self) -> Vec<iced::widget::image::Handle> {
        let mut out: Vec<&raster::Raster> = Vec::new();
        for layer in [&self.overview, &self.detail, &self.bare_old, &self.bare_new] {
            out.extend(layer.shown.iter());
        }
        // Those that came while the view glided, shown when it lands.
        for held in [&self.held_overview, &self.held_detail] {
            out.extend(held.iter().filter_map(|(_, raster)| raster.as_ref()));
        }
        if let Some(fade) = &self.fade {
            out.extend(fade.old.iter());
            out.extend(fade.next.iter());
            if let Some((old, new)) = &fade.bare {
                out.extend([old, new]);
            }
        }
        if let Some(previous) = &self.previous {
            out.extend(previous.pictures.iter());
        }
        if let Some(passage) = &self.passage {
            out.extend(passage.linked.iter().filter_map(|l| l.picture.as_ref()));
            out.extend(passage.small.iter().flatten());
        }
        let mut handles: Vec<_> = out.into_iter().map(|r| r.handle.clone()).collect();
        // The colors a glide fills the holes of its boxes with.
        let moves = self
            .fade
            .iter()
            .flat_map(|fade| fade.morph.iter())
            .flat_map(|m| &m.moves);
        for step in moves {
            for fill in [step.from_fill, step.to_fill].into_iter().flatten() {
                handles.push(raster::Raster::swatch(fill).handle);
            }
        }
        handles
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
