//! Looks a diagram is drawn in besides its file's own colors: a skin pack
//! laid over the text at render time, as the diagram pane lays the
//! Omarchy colors over it. The file stays as written, and so does the
//! source stored in an exported image; colors the file sets itself come
//! after the pack and stay its own, except where they do not read on a
//! look's paper of the other lightness: those keep their hue and change
//! sides (`omaspec_render::legible`). A line the engine names is read back
//! as a line of the file ([`LineShift`]).
//!
//! `render --look` exports in one: `plain` (black lines, no fills),
//! `sketch` and `vivid` (the packs with palettes of their own), `omarchy`
//! (the active theme), `light` and `dark` (Omaspec's own two palettes, the
//! same on every machine), and `auto`, one SVG with both of those, the
//! dark one under `prefers-color-scheme: dark`. A project may
//! pin a look in `omaspec-project.toml` (`[look]`); without `--look` its
//! diagrams are drawn in that, and `--look file` keeps the file's colors.
//! Either is a [`Skin`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use omaspec_core::Language;
use omaspec_core::skin::{self, Colors, Pack};
pub use omaspec_core::skin::{DARK, LIGHT};
use omaspec_render::export;
use omaspec_render::{Place, RenderError, Rendered, Renderer};
use omaspec_theme::Palette;

use crate::specs::Maps;

/// A look to draw in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Look {
    /// Black lines, no fills.
    Plain,
    /// Drawn by hand, in pastel colors on warm paper.
    Sketch,
    /// A palette by kind on white.
    Vivid,
    /// The colors of the active Omarchy theme.
    Omarchy,
    /// Omaspec's light palette.
    Light,
    /// Omaspec's dark palette.
    Dark,
    /// One SVG in the light palette and, under `prefers-color-scheme:
    /// dark`, the dark one.
    Auto,
    /// The file's own colors, also where its project pins a look.
    File,
}

impl Look {
    pub const ALL: [Look; 8] = [
        Look::Plain,
        Look::Sketch,
        Look::Vivid,
        Look::Omarchy,
        Look::Light,
        Look::Dark,
        Look::Auto,
        Look::File,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Look::Plain => "plain",
            Look::Sketch => "sketch",
            Look::Vivid => "vivid",
            Look::Omarchy => "omarchy",
            Look::Light => "light",
            Look::Dark => "dark",
            Look::Auto => "auto",
            Look::File => "file",
        }
    }

    pub fn from_name(name: &str) -> Option<Look> {
        Look::ALL.into_iter().find(|look| look.name() == name)
    }

    /// What this look draws in; `None` for the file's own colors.
    pub fn skin(self) -> Option<Skin> {
        let kind = match self {
            Look::Plain => Kind::Pack(Pack::Plain),
            Look::Sketch => Kind::Pack(Pack::Sketch),
            Look::Vivid => Kind::Pack(Pack::Vivid),
            Look::Omarchy => Kind::Colors(Hex::of_omarchy()),
            Look::Light => Kind::Colors(Hex::from(&LIGHT)),
            Look::Dark => Kind::Colors(Hex::from(&DARK)),
            Look::Auto => Kind::Auto(Hex::from(&LIGHT), Hex::from(&DARK)),
            Look::File => return None,
        };
        Some(Skin { kind, font: None })
    }
}

/// What a diagram is drawn in: a pack in its colors, with a font, laid
/// over the file; or the light and the dark one in one SVG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skin {
    kind: Kind,
    font: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    /// A pack that keeps its own colors: plain, spec, presentation,
    /// sketch, vivid.
    Pack(Pack),
    /// The color pack in these colors.
    Colors(Hex),
    /// The light and the dark colors, one SVG.
    Auto(Hex, Hex),
}

impl Skin {
    /// The look a project pins (`[look]`).
    pub fn pinned(look: &omaspec_core::project::Look) -> Skin {
        use omaspec_core::project::LookPack;
        let colors = |base: &Colors| {
            let mut hex = Hex::from(base);
            for (name, color) in &look.colors {
                hex.set(name, color);
            }
            hex
        };
        let kind = match look.pack {
            LookPack::Plain => Kind::Pack(Pack::Plain),
            LookPack::Spec => Kind::Pack(Pack::Spec),
            LookPack::Presentation => Kind::Pack(Pack::Presentation),
            LookPack::Sketch => Kind::Pack(Pack::Sketch),
            LookPack::Vivid => Kind::Pack(Pack::Vivid),
            LookPack::Light => Kind::Colors(colors(&LIGHT)),
            LookPack::Dark => Kind::Colors(colors(&DARK)),
            LookPack::Auto => Kind::Auto(Hex::from(&LIGHT), Hex::from(&DARK)),
        };
        Skin {
            kind,
            font: look.font.clone(),
        }
    }

    /// Whether it makes one SVG of two palettes.
    pub fn is_auto(&self) -> bool {
        matches!(self.kind, Kind::Auto(..))
    }

    /// The light one of `auto`, as a picture without color schemes (a
    /// PNG, a PDF) shows it; any other as it is.
    pub fn without_schemes(&self) -> Skin {
        match &self.kind {
            Kind::Auto(light, _) => Skin {
                kind: Kind::Colors(light.clone()),
                font: self.font.clone(),
            },
            _ => self.clone(),
        }
    }

    /// The color pack in `colors` (the Omarchy theme's, as the pane
    /// shows them).
    pub fn colors(colors: Hex) -> Skin {
        Skin {
            kind: Kind::Colors(colors),
            font: None,
        }
    }

    /// As shown on a dark page with `dark`, else on a light one: `auto`'s
    /// dark or light one; any other as it is.
    pub fn on(&self, dark: bool) -> Skin {
        if dark {
            self.dark()
        } else {
            self.without_schemes()
        }
    }

    /// `auto`'s dark one.
    fn dark(&self) -> Skin {
        match &self.kind {
            Kind::Auto(_, dark) => Skin {
                kind: Kind::Colors(dark.clone()),
                font: self.font.clone(),
            },
            _ => self.clone(),
        }
    }

    /// The pack and its colors.
    fn pack(&self) -> (Pack, Colors<'_>) {
        match &self.kind {
            Kind::Pack(pack) => (*pack, LIGHT),
            Kind::Colors(hex) | Kind::Auto(hex, _) => (Pack::Omarchy, hex.colors()),
        }
    }
}

/// The skin `chosen` (`--look`) draws in, else the look the project at
/// `root` pins, else none: the file's own colors. A project file that does
/// not read is an error.
pub fn skin_for(chosen: Option<Look>, root: Option<&Path>) -> anyhow::Result<Option<Skin>> {
    if let Some(look) = chosen {
        return Ok(look.skin());
    }
    let Some(root) = root else {
        return Ok(None);
    };
    match crate::project::read(root) {
        Some(Ok(project)) => Ok(project.look.as_ref().map(Skin::pinned)),
        Some(Err(err)) => Err(anyhow::anyhow!(err)),
        None => Ok(None),
    }
}

/// The looks projects pin, each project file read again only when its
/// size or time of change differ: the editor asks with every drawing.
#[derive(Default)]
pub struct Pinned(RefCell<HashMap<PathBuf, Kept>>);

/// What tells a changed project file.
type Stamp = (u64, Option<std::time::SystemTime>);

/// A project's look as its file was when read.
type Kept = (Option<Stamp>, Option<Skin>);

impl Pinned {
    /// The look the project at `root` pins, if its file reads.
    pub fn of(&self, root: Option<&Path>) -> Option<Skin> {
        let root = root?;
        let stamp = std::fs::metadata(root.join(omaspec_core::project::FILE))
            .ok()
            .map(|m| (m.len(), m.modified().ok()));
        if let Some((kept, skin)) = self.0.borrow().get(root)
            && *kept == stamp
        {
            return skin.clone();
        }
        let skin = skin_for(None, Some(root)).ok().flatten();
        self.0
            .borrow_mut()
            .insert(root.to_path_buf(), (stamp, skin.clone()));
        skin
    }
}

/// The paper and the accent of `skin`, a presentation's veil and light
/// (docs/adr/0031); the light palette's for the file's own colors.
pub fn stage(skin: Option<&Skin>) -> (omaspec_uml::Color, omaspec_uml::Color) {
    match skin.map(|skin| &skin.kind) {
        Some(Kind::Pack(pack)) => export::stage(&Colors {
            background: pack.paper(),
            ..LIGHT
        }),
        Some(Kind::Colors(hex) | Kind::Auto(hex, _)) => export::stage(&hex.colors()),
        None => export::stage(&LIGHT),
    }
}

/// The paper and the accent of the window's palette `p`, a presentation's
/// veil and light as the pane plays it.
pub fn palette_stage(p: &Palette) -> (omaspec_uml::Color, omaspec_uml::Color) {
    export::stage(&Hex::of(p).colors())
}

/// Colors as `#rrggbb`, for a skin pack: Omarchy's, or a look's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hex {
    pub background: String,
    pub surface: String,
    pub text: String,
    pub accent: String,
    pub muted: String,
    pub note: String,
    pub destroy: String,
    /// The circles before the names of classes: a class's, an
    /// interface's, an abstract class's, an enum's, an annotation's, and
    /// the letter in them.
    pub spots: Box<[String; 6]>,
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
            // A cross lies on the paper and on a group's box.
            destroy: hex(p.red.legible_on(
                &[p.background, p.lighter_background],
                p.foreground,
                omaspec_theme::tokens::MIN_CONTRAST,
            )),
            spots: Box::new(spots_of(p)),
        }
    }

    /// The colors of the active Omarchy theme, as the pane shows them.
    pub fn of_omarchy() -> Self {
        Self::of(&omaspec_theme::Theme::load_current().palette.legible())
    }

    /// The color `name` (`background`, `surface`, …) as `color`.
    fn set(&mut self, name: &str, color: &str) {
        let slot = match name {
            "background" => &mut self.background,
            "surface" => &mut self.surface,
            "text" => &mut self.text,
            "accent" => &mut self.accent,
            "muted" => &mut self.muted,
            "note" => &mut self.note,
            "destroy" => &mut self.destroy,
            _ => return,
        };
        color.clone_into(slot);
    }

    pub fn colors(&self) -> Colors<'_> {
        Colors {
            background: &self.background,
            surface: &self.surface,
            text: &self.text,
            accent: &self.accent,
            muted: &self.muted,
            note: &self.note,
            destroy: &self.destroy,
            spots: skin::Spots {
                class: &self.spots[0],
                interface: &self.spots[1],
                abstract_class: &self.spots[2],
                enumeration: &self.spots[3],
                annotation: &self.spots[4],
                letter: &self.spots[5],
            },
        }
    }
}

/// How far a circle's letter stands from its fill: as far as any words
/// from what they lie on, as WCAG asks of text.
const SPOT_CONTRAST: f32 = 4.5;

/// The circles of a class diagram in the theme's own colors, as PlantUML
/// colors them by kind (a class green, an interface violet, an abstract
/// class cyan, an enum orange, an annotation red), and in each the
/// theme's background or its text, whichever reads better on all five; a
/// fill it does not read on is moved away from it until it does.
fn spots_of(p: &Palette) -> [String; 6] {
    let fills = [p.green, p.magenta, p.cyan, p.yellow, p.red];
    let worst = |letter: omaspec_theme::Rgb| {
        fills
            .iter()
            .map(|fill| letter.contrast(*fill))
            .fold(f32::INFINITY, f32::min)
    };
    let (letter, away) = if worst(p.background) >= worst(p.foreground) {
        (p.background, p.foreground)
    } else {
        (p.foreground, p.background)
    };
    let [a, b, c, d, e] =
        fills.map(|fill| fill.legible_on(&[letter], away, SPOT_CONTRAST).to_string());
    [a, b, c, d, e, letter.to_string()]
}

impl From<&Colors<'_>> for Hex {
    fn from(c: &Colors) -> Self {
        Self {
            background: c.background.to_owned(),
            surface: c.surface.to_owned(),
            text: c.text.to_owned(),
            accent: c.accent.to_owned(),
            muted: c.muted.to_owned(),
            note: c.note.to_owned(),
            destroy: c.destroy.to_owned(),
            spots: Box::new(
                [
                    c.spots.class,
                    c.spots.interface,
                    c.spots.abstract_class,
                    c.spots.enumeration,
                    c.spots.annotation,
                    c.spots.letter,
                ]
                .map(str::to_owned),
            ),
        }
    }
}

/// `text` with `pack` laid over it, written in `font`, and where it sits;
/// `None` when the pack changes nothing.
pub fn overlay(
    text: &str,
    language: Language,
    pack: Pack,
    colors: &Colors,
    font: Option<&str>,
) -> Option<(String, LineShift)> {
    let (range, replacement) = skin::edit_in(text, language, Some(pack), colors, font)?;
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

/// `text` as drawn in `skin`, with where the pack sits; for `auto`, in
/// the light palette.
pub fn apply(text: &str, language: Language, skin: &Skin) -> (String, Option<LineShift>) {
    let (pack, colors) = skin.pack();
    let laid = overlay(text, language, pack, &colors, skin.font.as_deref());
    match laid {
        Some((shown, shift)) => (shown, Some(shift)),
        None => (text.to_owned(), None),
    }
}

/// `text` drawn in `skin` as `maps` draws it; a line the engine names is
/// a line of the file. `auto` draws the light and the dark one and makes
/// one SVG of them.
pub fn render(
    maps: &mut Maps,
    renderer: &mut Renderer,
    text: &str,
    language: Language,
    path: Option<&Path>,
    place: Option<&Place>,
    skin: Option<&Skin>,
) -> Result<Rendered, RenderError> {
    let Some(skin) = skin else {
        return maps.render(renderer, text, language, path, place);
    };
    if skin.is_auto() {
        let light = render(
            maps,
            renderer,
            text,
            language,
            path,
            place,
            Some(&skin.without_schemes()),
        )?;
        let dark = render(
            maps,
            renderer,
            text,
            language,
            path,
            place,
            Some(&skin.dark()),
        )?;
        let svg = omaspec_render::schemes::both(&light.svg, &dark.svg).map_err(|err| {
            RenderError::Engine {
                engine: light.engine,
                message: format!("the look auto does not draw here: {err}; take light or dark"),
            }
        })?;
        // One picture in two schemes; the pane shows it whole.
        return Ok(Rendered {
            svg,
            elapsed: light.elapsed + dark.elapsed,
            cards: None,
            ..light
        });
    }
    let (shown, shift) = apply(text, language, skin);
    let mut result = maps.render(renderer, &shown, language, path, place);
    if let Some(shift) = shift {
        shift.settle(&mut result);
    }
    result
}

/// The look a map that is made takes its colors from: a WBS of one box,
/// which a skin is laid over as over any diagram.
pub const MAP_SKIN: &str = "@startwbs\n* map\n@endwbs\n";

/// The project whose map stands beside `path`: the folder of a project's
/// file, which is drawn as that map.
pub fn map_root(path: &Path) -> Option<std::path::PathBuf> {
    if path.file_name()? != omaspec_core::project::FILE {
        return None;
    }
    Some(std::path::absolute(path).ok()?.parent()?.to_path_buf())
}

/// The map of `folder`, a folder of the project at `root` by its path
/// there (the empty one: the project itself), drawn in `skin` with its
/// tiles in the skin too (`docs/adr/0026-the-map-is-made.md`). `auto`
/// draws the light and the dark one and makes one SVG of them.
pub fn render_map(
    maps: &mut Maps,
    renderer: &mut Renderer,
    root: &Path,
    folder: &str,
    skin: Option<&Skin>,
) -> Result<Rendered, RenderError> {
    let atlas = maps.project(root);
    let Some(skin) = skin else {
        return renderer.render_map(&atlas, folder, MAP_SKIN);
    };
    if skin.is_auto() {
        let light = render_map(maps, renderer, root, folder, Some(&skin.without_schemes()))?;
        let dark = render_map(maps, renderer, root, folder, Some(&skin.dark()))?;
        let svg = omaspec_render::schemes::both(&light.svg, &dark.svg).map_err(|err| {
            RenderError::Engine {
                engine: light.engine,
                message: format!("the look auto does not draw here: {err}; take light or dark"),
            }
        })?;
        return Ok(Rendered {
            svg,
            elapsed: light.elapsed + dark.elapsed,
            cards: None,
            ..light
        });
    }
    let mut painted = omaspec_render::Atlas::clone(&atlas);
    for entry in &mut painted.diagrams {
        entry.source = apply(&entry.source, entry.language, skin).0;
    }
    let look = apply(MAP_SKIN, Language::PlantUml, skin).0;
    renderer.render_map(&painted, folder, &look)
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

    /// `map` with the lines its states, transitions, messages, lines, the
    /// words and shapes tied to a line and the parts one line writes come
    /// from read as lines of the file, where a play, a build and a
    /// Ctrl+click look for them.
    pub fn map(self, map: &omaspec_render::Map) -> omaspec_render::Map {
        let mut map = map.clone();
        let lines = map
            .states
            .iter_mut()
            .map(|spot| &mut spot.line)
            .chain(map.transitions.iter_mut().map(|route| &mut route.line))
            .chain(map.messages.iter_mut().map(|step| &mut step.line))
            .chain(map.notes.iter_mut().map(|step| &mut step.line))
            .chain(map.cells.iter_mut().map(|cell| &mut cell.line))
            .chain(map.misses.iter_mut().map(|(line, _)| line))
            .chain(map.tied.iter_mut().map(|tied| &mut tied.line))
            .chain(map.shapes.iter_mut().map(|shape| &mut shape.line))
            .chain(map.written.iter_mut().map(|(_, line)| line))
            .chain(
                map.wires
                    .iter_mut()
                    .flat_map(|wire| [wire.line.as_mut(), wire.entry.as_mut()])
                    .flatten(),
            );
        for line in lines {
            if let Some(file) = self.file_line(*line) {
                *line = file;
            }
        }
        map
    }

    /// What was drawn from the source with the pack laid over it, told in
    /// the file's lines: the map of a picture, the line of an error. The
    /// one place a look's lines are taken out again; who draws through
    /// [`render`] never sees them.
    pub fn settle(self, result: &mut Result<Rendered, RenderError>) {
        match result {
            Ok(rendered) => rendered.map = self.map(&rendered.map),
            Err(err) => self.remap(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cross of a destroyed participant reads on the paper and on a
    /// group's box: in Omaspec's two palettes and in every theme Omarchy
    /// ships (where it is installed), whose red it takes.
    #[test]
    fn a_destroyed_participant_s_cross_reads_in_every_look() {
        use omaspec_theme::{Rgb, tokens::MIN_CONTRAST};
        let reads = |hex: &Hex, name: &str| {
            let rgb = |c: &str| Rgb::parse_hex(c).unwrap_or_else(|| panic!("{name}: {c}"));
            let cross = rgb(&hex.destroy);
            for paper in [&hex.background, &hex.surface] {
                let ratio = cross.contrast(rgb(paper));
                assert!(ratio >= MIN_CONTRAST, "{name}: {ratio:.2} on {paper}");
            }
        };
        reads(&Hex::from(&LIGHT), "light");
        reads(&Hex::from(&DARK), "dark");
        let Ok(dirs) = std::fs::read_dir("/usr/share/omarchy/themes") else {
            return;
        };
        for dir in dirs.flatten() {
            let Ok(p) = Palette::load(&dir.path().join("colors.toml")) else {
                continue;
            };
            let p = p.legible();
            let hex = Hex::of(&p);
            reads(&hex, &dir.file_name().to_string_lossy());
            // A red that reads already is the theme's own.
            if [p.background, p.lighter_background]
                .iter()
                .all(|paper| p.red.contrast(*paper) >= MIN_CONTRAST)
            {
                assert_eq!(hex.destroy, p.red.to_string());
            }
        }
    }

    /// The letter in a class's circle reads on every kind's fill (4.5:1), in
    /// every theme Omarchy ships (where it is installed); a fill that
    /// reads already is the theme's own.
    #[test]
    fn the_circles_of_classes_read_in_every_theme() {
        use omaspec_theme::Rgb;
        let Ok(dirs) = std::fs::read_dir("/usr/share/omarchy/themes") else {
            return;
        };
        for dir in dirs.flatten() {
            let Ok(p) = Palette::load(&dir.path().join("colors.toml")) else {
                continue;
            };
            let p = p.legible();
            let hex = Hex::of(&p);
            let name = dir.file_name().to_string_lossy().into_owned();
            let rgb = |c: &str| Rgb::parse_hex(c).unwrap_or_else(|| panic!("{name}: {c}"));
            let letter = rgb(&hex.spots[5]);
            let own = [p.green, p.magenta, p.cyan, p.yellow, p.red];
            for (fill, own) in hex.spots[..5].iter().zip(own) {
                let ratio = letter.contrast(rgb(fill));
                assert!(
                    ratio >= SPOT_CONTRAST - 0.05,
                    "{name}: {ratio:.2} on {fill}"
                );
                if letter.contrast(own) >= SPOT_CONTRAST {
                    assert_eq!(*fill, own.to_string(), "{name}");
                }
            }
        }
    }

    /// A look lays its skin before the diagram, some seventy lines of it;
    /// what is drawn through [`render`] tells the file's lines all the
    /// same: the map's, which a play and a click read, and an error's.
    #[test]
    fn a_look_gives_the_files_lines_back() {
        let text =
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
        let mut maps = Maps::default();
        let mut renderer = Renderer::new(&crate::commands::render_config());
        let mut draw = |text: &str, look: Option<Look>| {
            let skin = look.and_then(Look::skin);
            render(
                &mut maps,
                &mut renderer,
                text,
                Language::PlantUml,
                None,
                None,
                skin.as_ref(),
            )
        };
        let lines = |rendered: &Rendered| -> Vec<usize> {
            let map = &rendered.map;
            map.states
                .iter()
                .map(|spot| spot.line)
                .chain(map.transitions.iter().map(|route| route.line))
                .collect()
        };
        let plain = lines(&draw(text, None).unwrap());
        assert!(
            plain.iter().all(|&line| (1..=3).contains(&line)),
            "{plain:?}"
        );
        for look in [Look::Light, Look::Plain] {
            assert_eq!(lines(&draw(text, Some(look)).unwrap()), plain, "{look:?}");
        }
        // Refused by Omaspec's own engine, on its eighth line.
        let broken = "@startuml\nstate \"Engine cut off\" as Cutoff\n[*] --> Flight\nstate Flight {\n  [*] --> Ascent\n  ||\n  [*] --> Idle\n  Idle --> Cutoff\n}\n@enduml\n";
        let mut line = |look| match draw(broken, look) {
            Err(RenderError::Diagram { line, .. } | RenderError::Unavailable { line, .. }) => line,
            other => panic!("{:?}", other.map(|r| r.engine)),
        };
        assert_eq!(line(None), Some(7));
        assert_eq!(line(Some(Look::Light)), Some(7));
        // A scenario's steps, its notes of their own too (REQ-106).
        let scenario = "@startuml\nA -> B : one\nnote over A, B: between\nB -> A : two\n@enduml\n";
        let mut steps = |look| -> Vec<usize> {
            draw(scenario, look)
                .unwrap()
                .map
                .steps()
                .iter()
                .map(|step| step.line)
                .collect()
        };
        assert_eq!(steps(None), [1, 2, 3]);
        assert_eq!(steps(Some(Look::Light)), [1, 2, 3]);
    }

    #[test]
    fn a_pinned_look_is_read_again_only_when_its_file_changed() {
        let root = std::env::temp_dir().join(format!("omaspec-pinned-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join(omaspec_core::project::FILE);
        std::fs::write(&file, "name = \"house\"\n\n[look]\npack = \"light\"\n").unwrap();
        let looks = Pinned::default();
        let light = looks.of(Some(&root)).expect("a pinned look");
        assert_eq!(looks.of(Some(&root)), Some(light.clone()));
        std::fs::write(&file, "name = \"house\"\n\n[look]\npack = \"dark\"\n").unwrap();
        let dark = looks.of(Some(&root)).expect("the new look");
        assert_ne!(dark, light);
        std::fs::remove_file(&file).unwrap();
        assert_eq!(looks.of(Some(&root)), None);
        assert_eq!(looks.of(None), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_look_is_laid_over_and_lines_read_back_as_the_file_s() {
        let text = "@startuml\nA -> B : hi\n@enduml\n";
        let (shown, shift) = apply(text, Language::PlantUml, &Look::Dark.skin().unwrap());
        let shift = shift.expect("a pack");
        assert!(shown.contains("skinparam backgroundColor #16181d"));
        let rendered = shown.lines().position(|l| l == "A -> B : hi").unwrap();
        assert_eq!(shift.file_line(rendered), Some(1));
        assert_eq!(shift.file_line(shift.at), None);
        let (plain, _) = apply(text, Language::PlantUml, &Look::Plain.skin().unwrap());
        assert!(plain.contains("skinparam monochrome true"));
        for look in Look::ALL {
            assert_eq!(Look::from_name(look.name()), Some(look));
        }
    }
}
