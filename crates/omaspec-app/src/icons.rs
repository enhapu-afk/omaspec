//! Small line icons for the tabs, the sidebar and the status line: a kind
//! of diagram, a folder. They are drawn like the mark, from SVG through
//! `sanitize`, in a theme color and at the window's scale, so they stay
//! sharp; a new theme or scale draws them again.

use std::collections::HashMap;

use iced::widget::image;
use omaspec_core::{DiagramKind, Kind};
use omaspec_theme::Rgb;

/// The size the icons are shown at, in logical pixels.
pub const SIZE: f32 = 13.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    State,
    Sequence,
    Flow,
    Structure,
    Timing,
    Other,
    Folder,
    Search,
    /// The gallery of examples.
    Gallery,
    // The toolbar's tools (docs/adr/0025).
    Fit,
    ZoomIn,
    ZoomOut,
    ActualSize,
    Play,
    Unfold,
    Copy,
    Export,
}

impl Icon {
    const ALL: [Icon; 17] = [
        Icon::State,
        Icon::Sequence,
        Icon::Flow,
        Icon::Structure,
        Icon::Timing,
        Icon::Other,
        Icon::Folder,
        Icon::Search,
        Icon::Gallery,
        Icon::Fit,
        Icon::ZoomIn,
        Icon::ZoomOut,
        Icon::ActualSize,
        Icon::Play,
        Icon::Unfold,
        Icon::Copy,
        Icon::Export,
    ];

    /// The icon of a kind of diagram; a document not read yet has
    /// [`Icon::Other`].
    pub fn of(kind: Option<&DiagramKind>) -> Icon {
        let Some(kind) = kind else {
            return Icon::Other;
        };
        if kind.name == "Flowchart" {
            return Icon::Flow;
        }
        match kind.kind {
            Kind::State => Icon::State,
            Kind::Sequence => Icon::Sequence,
            Kind::Activity => Icon::Flow,
            Kind::Structure => Icon::Structure,
            Kind::Timing => Icon::Timing,
            Kind::Other => Icon::Other,
        }
    }

    /// The shapes inside a 24 × 24 box, stroked.
    fn shapes(self) -> &'static str {
        match self {
            Icon::State => {
                r#"<rect x="3" y="9" width="10" height="7" rx="3"/><circle cx="18" cy="5" r="2"/><path d="M13 12h4a1 1 0 0 0 1-1V7"/>"#
            }
            Icon::Sequence => r#"<path d="M6 3v18M18 3v18M6 8h12M18 14H6"/>"#,
            Icon::Flow => {
                r#"<rect x="3" y="3" width="7" height="6" rx="1.5"/><rect x="14" y="15" width="7" height="6" rx="1.5"/><path d="M6.5 9v4a2 2 0 0 0 2 2H14"/>"#
            }
            Icon::Structure => {
                r#"<rect x="4" y="3" width="16" height="18" rx="2"/><path d="M4 9h16M4 15h16"/>"#
            }
            Icon::Timing => r#"<path d="M3 16h4V8h5v8h4V8h5"/>"#,
            Icon::Other => {
                r#"<circle cx="6" cy="7" r="2.5"/><circle cx="18" cy="7" r="2.5"/><circle cx="12" cy="18" r="2.5"/><path d="M8.5 7h7M7.3 9.2l3.4 6.6M16.7 9.2l-3.4 6.6"/>"#
            }
            Icon::Search => r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-4.2-4.2"/>"#,
            Icon::Gallery => {
                r#"<rect x="3" y="3" width="7.5" height="7.5" rx="2"/><rect x="13.5" y="3" width="7.5" height="7.5" rx="2"/><rect x="3" y="13.5" width="7.5" height="7.5" rx="2"/><rect x="13.5" y="13.5" width="7.5" height="7.5" rx="2"/>"#
            }
            Icon::Folder => {
                r#"<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#
            }
            Icon::Fit => {
                r#"<path d="M3 9V5a2 2 0 0 1 2-2h4M15 3h4a2 2 0 0 1 2 2v4M21 15v4a2 2 0 0 1-2 2h-4M9 21H5a2 2 0 0 1-2-2v-4"/>"#
            }
            Icon::ZoomIn => {
                r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-4.2-4.2M11 8v6M8 11h6"/>"#
            }
            Icon::ZoomOut => r#"<circle cx="11" cy="11" r="7"/><path d="m20 20-4.2-4.2M8 11h6"/>"#,
            Icon::ActualSize => {
                r#"<rect x="2.5" y="5" width="19" height="14" rx="2"/><path d="M7 10l1.5-1v6M15.5 10l1.5-1v6M12 10.5v.5M12 13.5v.5"/>"#
            }
            Icon::Play => r#"<path d="M7 4.5v15l12-7.5z"/>"#,
            Icon::Unfold => {
                r#"<rect x="3.5" y="3.5" width="17" height="17" rx="2"/><path d="M12 8v8M8 12h8"/>"#
            }
            Icon::Copy => {
                r#"<rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>"#
            }
            Icon::Export => {
                r#"<path d="M12 3v12M7 10l5 5 5-5M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/>"#
            }
        }
    }
}

/// A kind of diagram in a word or two, for a small label: `state`,
/// `sequence`, `flow`, `class`, `mind map`.
pub fn label(kind: &DiagramKind) -> String {
    let name = kind.name.to_lowercase();
    let name = name
        .strip_suffix(" diagram")
        .or_else(|| name.strip_suffix(" chart"))
        .unwrap_or(&name);
    match name {
        "flowchart" => "flow".to_owned(),
        "component and deployment" => "component".to_owned(),
        _ => name.to_owned(),
    }
}

/// Every icon in a few colors, ready to show.
pub struct Icons {
    handles: HashMap<(Icon, Rgb), image::Handle>,
}

impl Icons {
    /// The icons in each of `colors`, for a window at `scale`.
    pub fn new(colors: &[Rgb], scale: f32) -> Icons {
        let px = (SIZE * scale.max(1.0) * 2.0).ceil() as u32;
        let mut handles = HashMap::new();
        for &color in colors {
            for icon in Icon::ALL {
                if let Some(handle) = draw(icon, color, px) {
                    handles.insert((icon, color), handle);
                }
            }
        }
        Icons { handles }
    }

    /// `icon` in `color`, or nothing if that color was not drawn.
    pub fn get(&self, icon: Icon, color: Rgb) -> Option<image::Handle> {
        self.handles.get(&(icon, color)).cloned()
    }
}

fn draw(icon: Icon, color: Rgb, px: u32) -> Option<image::Handle> {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="{color}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{}</svg>"#,
        icon.shapes()
    );
    let svg = omaspec_render::sanitize::sanitize(&svg).ok()?;
    let tree = omaspec_render::export::parse(&svg).ok()?;
    let pixmap = omaspec_render::export::rasterize_clear(&tree, px as f32 / 24.0).ok()?;
    let (width, height) = (pixmap.width(), pixmap.height());
    Some(image::Handle::from_rgba(width, height, pixmap.take()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_draws_in_every_color() {
        let colors = [Rgb::new(0x7a, 0xa2, 0xf7), Rgb::new(0xad, 0x8e, 0xe6)];
        let icons = Icons::new(&colors, 1.5);
        for color in colors {
            for icon in Icon::ALL {
                assert!(icons.get(icon, color).is_some(), "{icon:?} in {color}");
            }
        }
    }

    #[test]
    fn kinds_take_their_icon_and_a_short_label() {
        let kind = |code: &str, lang| omaspec_core::classify(code, lang);
        let state = kind(
            "@startuml\n[*] --> A\n@enduml\n",
            omaspec_core::Language::PlantUml,
        );
        assert_eq!(
            (Icon::of(Some(&state)), label(&state).as_str()),
            (Icon::State, "state")
        );
        let flow = kind("flowchart TD\nA --> B\n", omaspec_core::Language::Mermaid);
        assert_eq!(
            (Icon::of(Some(&flow)), label(&flow).as_str()),
            (Icon::Flow, "flow")
        );
        let class = kind(
            "@startuml\nclass A\nclass B\n@enduml\n",
            omaspec_core::Language::PlantUml,
        );
        assert_eq!(
            (Icon::of(Some(&class)), label(&class).as_str()),
            (Icon::Structure, "class")
        );
        assert_eq!(Icon::of(None), Icon::Other);
    }
}
