//! The Omagram mark, `packaging/omagram.svg`. The window and the status
//! line draw it; the desktop entry installs the same file.

use iced::widget::image;
use iced::window;

const MARK: &str = include_str!("../../../packaging/omagram.svg");

fn pixmap(px: u32) -> Option<omagram_render::export::tiny_skia::Pixmap> {
    let svg = omagram_render::sanitize::sanitize(MARK).ok()?;
    let tree = omagram_render::export::parse(&svg).ok()?;
    let scale = px as f32 / tree.size().width().max(1.0);
    omagram_render::export::rasterize_clear(&tree, scale).ok()
}

/// The window icon, 128 px on the long side.
pub fn icon() -> Option<window::Icon> {
    let pixmap = pixmap(128)?;
    let (width, height) = (pixmap.width(), pixmap.height());
    window::icon::from_rgba(pixmap.take(), width, height).ok()
}

/// The mark for the status line.
pub fn handle() -> image::Handle {
    sized(32)
}

/// The mark, large, for an empty diagram pane.
pub fn large() -> image::Handle {
    sized(128)
}

fn sized(px: u32) -> image::Handle {
    match pixmap(px) {
        Some(pixmap) => {
            let (width, height) = (pixmap.width(), pixmap.height());
            image::Handle::from_rgba(width, height, pixmap.take())
        }
        None => image::Handle::from_rgba(1, 1, vec![0, 0, 0, 0]),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_mark_becomes_a_window_icon() {
        assert!(super::icon().is_some());
    }
}
