//! A large map shown by its cards (`docs/adr/0018-diagrams-that-link.md`,
//! amended 2026-11-05): the pane shows the map without its tiles'
//! pictures (`Drawn::cards`) until a tile is large enough on screen to
//! read, and from there the whole map. That SVG is parsed once, on a
//! worker thread, when the zoom first asks for it: parsed on the render
//! thread with every drawing of the map, it would hold up each drawing
//! after it, for pictures that may never be looked at.

use std::sync::Arc;
use std::time::Duration;

use iced::Task;
use omaspec_render::Link;
use omaspec_render::export::{self, usvg};

use super::{Message, Paper};
use crate::worker::{Drawn, blocking};

/// How wide a tile is on screen, in logical pixels, when its picture can
/// be read: a state's name in it about as large as small print.
const READABLE: f32 = 96.0;

/// What a diagram shown by its cards keeps of its pictures.
#[derive(Default)]
pub(super) struct Pictures {
    /// The zoom from which a tile's picture can be read; `None` for a
    /// diagram shown whole.
    readable: Option<f32>,
    /// `drawn.svg` parsed, the tiles' pictures in it.
    tree: Option<Arc<usvg::Tree>>,
    /// Whether that parse was asked for.
    asked: bool,
}

impl Pictures {
    pub(super) fn of(drawn: &Drawn) -> Pictures {
        Pictures {
            readable: drawn.cards.then(|| readable_zoom(&drawn.links)).flatten(),
            ..Pictures::default()
        }
    }

    #[cfg(test)]
    pub(super) fn readable(&self) -> Option<f32> {
        self.readable
    }

    #[cfg(test)]
    pub(super) fn asked(&self) -> bool {
        self.asked
    }
}

/// The zoom at which the tiles of a map, the middle one by width, are
/// [`READABLE`] wide on screen; `None` without tiles.
fn readable_zoom(links: &[Link]) -> Option<f32> {
    let mut widths: Vec<f32> = links
        .iter()
        .filter(|l| l.tile && l.rect.w > 0.0)
        .map(|l| l.rect.w)
        .collect();
    widths.sort_by(f32::total_cmp);
    widths.get(widths.len() / 2).map(|w| READABLE / w)
}

impl Paper {
    /// Whether a map's tiles are large enough on screen to read their
    /// pictures.
    fn reads_pictures(&self) -> bool {
        self.diagram
            .as_ref()
            .and_then(|d| d.pictures.readable)
            .is_some_and(|zoom| self.view.zoom >= zoom)
    }

    /// What the pane shows: a map's tiles with their pictures when they
    /// can be read and are parsed, else what was drawn.
    pub(super) fn shown_tree(&self) -> Option<&Arc<usvg::Tree>> {
        let diagram = self.diagram.as_ref()?;
        match &diagram.pictures.tree {
            Some(full) if self.reads_pictures() => Some(full),
            _ => Some(&diagram.drawn.tree),
        }
    }

    /// The tree the layers rasterize, their plans forgotten when it is not
    /// the one before; and the parse of the pictures, when the view now
    /// reads them and nobody asked for it yet.
    pub(super) fn picture_tree(&mut self) -> (Task<Message>, Option<Arc<usvg::Tree>>) {
        let reads = self.reads_pictures();
        let shown = self.shown;
        let Some(diagram) = &mut self.diagram else {
            return (Task::none(), None);
        };
        let pictures = &mut diagram.pictures;
        let parse = if reads && pictures.tree.is_none() && !pictures.asked {
            pictures.asked = true;
            let svg = Arc::clone(&diagram.drawn.svg);
            Task::perform(
                blocking(move || export::parse(&svg).ok().map(Arc::new)),
                move |tree| Message::Pictures(shown, tree.flatten()),
            )
        } else {
            Task::none()
        };
        let full = reads && pictures.tree.is_some();
        if full != self.on_pictures {
            // The same plans, from the other tree.
            self.on_pictures = full;
            self.overview.forget();
            self.detail.forget();
        }
        (parse, self.shown_tree().cloned())
    }

    /// The pictures of the diagram with id `id`, parsed.
    pub(super) fn pictures_came(
        &mut self,
        id: u64,
        tree: Option<Arc<usvg::Tree>>,
    ) -> Task<Message> {
        if id != self.shown {
            return Task::none();
        }
        if let Some(diagram) = &mut self.diagram {
            diagram.pictures.tree = tree;
        }
        self.rasterize(Duration::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use iced::Size;

    use super::super::testing::{answer, map_by_cards};
    use super::*;

    #[test]
    fn a_large_map_shows_its_cards_and_the_pictures_once_its_tiles_can_be_read() {
        let mut paper = Paper::default();
        let _ = paper.update(Message::Viewport(Size::new(200.0, 150.0)));
        paper.pretend_requested(1);
        let map = map_by_cards();
        assert!(map.cards);
        let (cards, svg) = (Arc::clone(&map.tree), Arc::clone(&map.svg));
        let _ = paper.rendered(answer(1, 1, Ok(map)));
        let readable = paper.diagram.as_ref().unwrap().pictures.readable().unwrap();

        // Fitted into a small pane, the tiles are too small to read: the
        // cards, and nothing more is parsed.
        assert!(paper.view.zoom < readable, "{} {readable}", paper.view.zoom);
        assert!(Arc::ptr_eq(paper.shown_tree().unwrap(), &cards));
        assert!(!paper.diagram.as_ref().unwrap().pictures.asked());

        // Zoomed in until they can be: the pictures are asked for, and the
        // cards stay until they come.
        let _ = paper.zoom(readable / paper.view.zoom * 1.25);
        assert!(paper.diagram.as_ref().unwrap().pictures.asked());
        assert!(Arc::ptr_eq(paper.shown_tree().unwrap(), &cards));
        let asked = paper.overview.asked;
        let _ = paper.update(Message::Pictures(paper.shown, paper_pictures(&svg)));
        let full = Arc::clone(paper.shown_tree().unwrap());
        assert!(!Arc::ptr_eq(&full, &cards));
        // The same plans, rasterized again from the pictures.
        assert_eq!(paper.overview.asked, asked);
        assert!(paper.on_pictures);
        // The pictures of an older drawing are no answer.
        let _ = paper.update(Message::Pictures(paper.shown - 1, None));
        assert!(Arc::ptr_eq(paper.shown_tree().unwrap(), &full));

        // Zoomed out again: the cards; the pictures are kept for the way
        // back in.
        let _ = paper.zoom(0.5);
        assert!(Arc::ptr_eq(paper.shown_tree().unwrap(), &cards));
        assert!(!paper.on_pictures);
        let _ = paper.zoom(2.0);
        assert!(Arc::ptr_eq(paper.shown_tree().unwrap(), &full));
    }

    /// The parse the pane asks for when a map's tiles can be read.
    fn paper_pictures(svg: &str) -> Option<Arc<usvg::Tree>> {
        export::parse(svg).ok().map(Arc::new)
    }
}
