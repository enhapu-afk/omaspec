//! Looks a diagram is drawn in besides its file's own colors: a skin pack
//! laid over the text at render time, as the diagram pane lays the
//! Omarchy colors over it. The file stays as written, and so does the
//! source stored in an exported image; colors the file sets itself come
//! after the pack and stay its own. A line the engine names is read back
//! as a line of the file ([`LineShift`]).
//!
//! `render --look` exports in one: `plain` (black lines, no fills),
//! `omarchy` (the active theme), `light` and `dark` (Omaspec's own two
//! palettes, the same on every machine), and `auto`, one SVG with both of
//! those, the dark one under `prefers-color-scheme: dark`.

use std::path::Path;

use clap::ValueEnum;
use omaspec_core::Language;
use omaspec_core::skin::{self, Colors, Pack};
use omaspec_render::{Place, RenderError, Rendered, Renderer};
use omaspec_theme::Palette;

use crate::specs::Maps;

/// A look to draw in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Look {
    /// Black lines, no fills.
    Plain,
    /// The colors of the active Omarchy theme.
    Omarchy,
    /// Omaspec's light palette.
    Light,
    /// Omaspec's dark palette.
    Dark,
    /// One SVG in the light palette and, under `prefers-color-scheme:
    /// dark`, the dark one.
    Auto,
}

impl Look {
    pub const ALL: [Look; 5] = [
        Look::Plain,
        Look::Omarchy,
        Look::Light,
        Look::Dark,
        Look::Auto,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Look::Plain => "plain",
            Look::Omarchy => "omarchy",
            Look::Light => "light",
            Look::Dark => "dark",
            Look::Auto => "auto",
        }
    }

    pub fn from_name(name: &str) -> Option<Look> {
        Look::ALL.into_iter().find(|look| look.name() == name)
    }
}

/// Omaspec's light palette: dark text on white, a blue accent.
pub const LIGHT: Colors<'static> = Colors {
    background: "#ffffff",
    surface: "#f3f4f6",
    text: "#1f2328",
    accent: "#2f6fdb",
    muted: "#6b7280",
    note: "#fdf6d8",
};

/// Omaspec's dark palette: light text on near black, a lighter blue.
pub const DARK: Colors<'static> = Colors {
    background: "#16181d",
    surface: "#23272f",
    text: "#e3e5ea",
    accent: "#79a6f0",
    muted: "#8b93a1",
    note: "#3a3526",
};

/// Omarchy colors as `#rrggbb`, for a skin pack.
pub struct Hex {
    pub background: String,
    pub surface: String,
    pub text: String,
    pub accent: String,
    pub muted: String,
    pub note: String,
}

impl Hex {
    pub fn of(p: &Palette) -> Self {
        let hex = |color: omaspec_theme::Rgb| color.to_string();
        Self {
            background: hex(p.background),
            surface: hex(p.lighter_background),
            text: hex(p.foreground),
            accent: hex(p.accent),
            muted: hex(p.dark_foreground),
            note: hex(p.selection),
        }
    }

    /// The colors of the active Omarchy theme, as the pane shows them.
    pub fn of_omarchy() -> Self {
        Self::of(&omaspec_theme::Theme::load_current().palette.legible())
    }

    pub fn colors(&self) -> Colors<'_> {
        Colors {
            background: &self.background,
            surface: &self.surface,
            text: &self.text,
            accent: &self.accent,
            muted: &self.muted,
            note: &self.note,
        }
    }
}

/// `text` with `pack` laid over it, and where it sits; `None` when the pack
/// changes nothing.
pub fn overlay(
    text: &str,
    language: Language,
    pack: Pack,
    colors: &Colors,
) -> Option<(String, LineShift)> {
    let (range, replacement) = skin::edit(text, language, Some(pack), colors)?;
    let shift = LineShift {
        at: text[..range.start].matches('\n').count(),
        added: replacement.matches('\n').count(),
        replaced: text[range.clone()].matches('\n').count(),
    };
    let shown = format!(
        "{}{replacement}{}",
        &text[..range.start],
        &text[range.end..]
    );
    Some((shown, shift))
}

/// `text` as drawn in `look`, with where the pack sits; for `auto`, in the
/// light palette (see [`dark_of_auto`]).
pub fn apply(text: &str, language: Language, look: Look) -> (String, Option<LineShift>) {
    let laid = match look {
        Look::Plain => overlay(text, language, Pack::Plain, &LIGHT),
        Look::Omarchy => overlay(text, language, Pack::Omarchy, &Hex::of_omarchy().colors()),
        Look::Light | Look::Auto => overlay(text, language, Pack::Omarchy, &LIGHT),
        Look::Dark => overlay(text, language, Pack::Omarchy, &DARK),
    };
    match laid {
        Some((shown, shift)) => (shown, Some(shift)),
        None => (text.to_owned(), None),
    }
}

/// `text` drawn in `look` as `maps` draws it, a map with its tiles in the
/// look too; a line the engine names is a line of the file. `auto` draws
/// the light and the dark look and makes one SVG of them.
pub fn render(
    maps: &mut Maps,
    renderer: &mut Renderer,
    text: &str,
    language: Language,
    path: Option<&Path>,
    place: Option<&Place>,
    look: Option<Look>,
) -> Result<Rendered, RenderError> {
    let Some(look) = look else {
        return maps.render(renderer, text, language, path, place);
    };
    if look == Look::Auto {
        let light = render(
            maps,
            renderer,
            text,
            language,
            path,
            place,
            Some(Look::Light),
        )?;
        let dark = render(
            maps,
            renderer,
            text,
            language,
            path,
            place,
            Some(Look::Dark),
        )?;
        let svg = omaspec_render::schemes::both(&light.svg, &dark.svg).map_err(|err| {
            RenderError::Engine {
                engine: light.engine,
                message: format!("the look auto does not draw here: {err}; take light or dark"),
            }
        })?;
        return Ok(Rendered {
            svg,
            elapsed: light.elapsed + dark.elapsed,
            ..light
        });
    }
    let (shown, shift) = apply(text, language, look);
    let paint = |text: &str, language: Language| apply(text, language, look).0;
    maps.render_painted(renderer, &shown, language, path, place, &paint)
        .map_err(|mut err| {
            if let Some(shift) = shift {
                shift.remap(&mut err);
            }
            err
        })
}

/// Where a laid-over pack sits in the rendered source, so a reported line
/// can be read as a line of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineShift {
    /// First rendered line of the pack.
    pub at: usize,
    /// Lines the pack takes.
    pub added: usize,
    /// Lines of the file that the pack replaced.
    pub replaced: usize,
}

impl LineShift {
    /// The file line for rendered line `rendered`. `None` when that line
    /// is part of the pack.
    pub fn file_line(self, rendered: usize) -> Option<usize> {
        if rendered < self.at {
            Some(rendered)
        } else if rendered < self.at + self.added {
            None
        } else {
            Some(rendered - self.added + self.replaced)
        }
    }

    /// `err`'s line read as a line of the file.
    pub fn remap(self, err: &mut RenderError) {
        let line = match err {
            RenderError::Diagram { line, .. } | RenderError::Unavailable { line, .. } => line,
            RenderError::Engine { .. } => return,
        };
        if let Some(at) = *line {
            *line = self.file_line(at);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_is_laid_over_and_lines_read_back_as_the_file_s() {
        let text = "@startuml\nA -> B : hi\n@enduml\n";
        let (shown, shift) = apply(text, Language::PlantUml, Look::Dark);
        let shift = shift.expect("a pack");
        assert!(shown.contains("skinparam backgroundColor #16181d"));
        let rendered = shown.lines().position(|l| l == "A -> B : hi").unwrap();
        assert_eq!(shift.file_line(rendered), Some(1));
        assert_eq!(shift.file_line(shift.at), None);
        let (plain, _) = apply(text, Language::PlantUml, Look::Plain);
        assert!(plain.contains("skinparam monochrome true"));
        for look in Look::ALL {
            assert_eq!(Look::from_name(look.name()), Some(look));
        }
    }
}
