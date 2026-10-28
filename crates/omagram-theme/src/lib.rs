//! The active Omarchy theme as a palette Omagram can draw with.
//!
//! Omarchy keeps the current theme in `~/.local/state/omarchy/current/theme/`,
//! with its colors in `colors.toml`. Themes may be canonical (semantic names
//! such as `dark_background`), use the legacy short names (`bg`, `fg`) or only
//! the ANSI palette (`color0`..`color15`). [`Palette::parse`] resolves all of
//! them with the same alias and fallback cascade as `omarchy-theme-color`, so
//! Omagram shows the colors every other themed app shows.
//!
//! A `omagram.toml` beside `colors.toml` adjusts Omagram alone: Omarchy
//! writes it on every theme switch from the user's template
//! `~/.config/omarchy/themed/omagram.toml.tpl`, or a theme ships one. Its
//! `[palette]` takes `colors.toml` names, `[syntax]` the colors of the
//! source, `[marks]` the marks beside the line numbers; all optional.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub mod tokens;

/// Palette used when no Omarchy theme can be read: Omarchy's Tokyo Night.
const FALLBACK_COLORS: &str = include_str!("fallback-colors.toml");

/// An sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb::new(0, 0, 0);
    pub const WHITE: Rgb = Rgb::new(0xff, 0xff, 0xff);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Parses `#rrggbb`. Other notations (`rgb()`, short hex) are not colors
    /// Omagram reads; the cascade treats them as missing.
    pub fn parse_hex(value: &str) -> Option<Self> {
        let hex = value.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self::new(channel(0)?, channel(2)?, channel(4)?))
    }

    /// WCAG relative luminance.
    fn luminance(self) -> f32 {
        let channel = |v: u8| {
            let v = f32::from(v) / 255.0;
            if v <= 0.039_28 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// The WCAG contrast ratio with `other`, 1 to 21.
    pub fn contrast(self, other: Rgb) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// `self`, moved towards `toward` in small steps until it has at least
    /// `min` contrast with every one of `backgrounds` (or as far as
    /// `toward` goes).
    pub fn legible_on(self, backgrounds: &[Rgb], toward: Rgb, min: f32) -> Rgb {
        let reads = |c: Rgb| backgrounds.iter().all(|b| c.contrast(*b) >= min);
        (0..=20)
            .map(|step| self.mix(toward, step as f32 / 20.0))
            .find(|c| reads(*c))
            .unwrap_or(toward)
    }

    /// Mixes `amount` (0.0..=1.0) of `other` into `self`, rounded like
    /// Omarchy's `mix_color`.
    pub fn mix(self, other: Rgb, amount: f32) -> Rgb {
        let amount = amount.clamp(0.0, 1.0);
        let channel =
            |a: u8, b: u8| (f32::from(a) * (1.0 - amount) + f32::from(b) * amount + 0.5) as u8;
        Rgb::new(
            channel(self.r, other.r),
            channel(self.g, other.g),
            channel(self.b, other.b),
        )
    }

    /// The channel sum Omarchy uses to tell light from dark backgrounds.
    fn channel_sum(self) -> u16 {
        u16::from(self.r) + u16::from(self.g) + u16::from(self.b)
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Whether a theme is dark or light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Dark,
    Light,
}

/// The resolved colors of one theme. Every field is set, whatever the theme
/// file defines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub mode: Mode,

    pub accent: Rgb,
    pub selection: Rgb,
    pub selection_foreground: Rgb,
    pub muted: Rgb,

    pub background: Rgb,
    pub dark_background: Rgb,
    pub darker_background: Rgb,
    pub lighter_background: Rgb,

    pub foreground: Rgb,
    pub dark_foreground: Rgb,
    pub light_foreground: Rgb,
    pub bright_foreground: Rgb,

    pub red: Rgb,
    pub yellow: Rgb,
    pub orange: Rgb,
    pub green: Rgb,
    pub cyan: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub brown: Rgb,

    pub bright_red: Rgb,
    pub bright_yellow: Rgb,
    pub bright_green: Rgb,
    pub bright_cyan: Rgb,
    pub bright_blue: Rgb,
    pub bright_magenta: Rgb,
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("theme defines no background color (neither background, bg nor color0)")]
    NoBackground,
    #[error("theme defines no foreground color (neither foreground, fg nor color7)")]
    NoForeground,
}

impl Palette {
    /// Resolves the text of a `colors.toml`. `light_marker` says whether a
    /// `light.mode` file sits next to it (the legacy way to mark light themes).
    pub fn parse(colors_toml: &str, light_marker: bool) -> Result<Self, ThemeError> {
        Cascade::new(parse_pairs(colors_toml)).resolve(light_marker)
    }

    /// Reads a `colors.toml` and the `light.mode` marker beside it.
    pub fn load(colors_file: &Path) -> Result<Self, ThemeError> {
        Self::load_with(colors_file, &HashMap::new())
    }

    /// Like [`load`](Self::load), with `extra` names overriding the file's.
    fn load_with(colors_file: &Path, extra: &HashMap<String, String>) -> Result<Self, ThemeError> {
        let text = fs::read_to_string(colors_file).map_err(|source| ThemeError::Read {
            path: colors_file.to_owned(),
            source,
        })?;
        let light_marker = colors_file
            .parent()
            .is_some_and(|dir| dir.join("light.mode").is_file());
        let mut pairs = parse_pairs(&text);
        pairs.extend(extra.iter().map(|(k, v)| (k.clone(), v.clone())));
        Cascade::new(pairs).resolve(light_marker)
    }

    /// The palette with its dimmed text (`dark_foreground`, for comments,
    /// hints and line numbers) moved towards the foreground where it would
    /// be too faint to read on the backgrounds Omagram puts it on.
    pub fn legible(&self) -> Self {
        Self {
            dark_foreground: self.dark_foreground.legible_on(
                &[self.background, self.dark_background],
                self.foreground,
                tokens::MIN_CONTRAST,
            ),
            ..self.clone()
        }
    }

    /// The built-in palette (Omarchy's Tokyo Night).
    pub fn fallback() -> Self {
        Self::parse(FALLBACK_COLORS, false).expect("the bundled fallback palette is complete")
    }
}

/// Where the palette of a [`Theme`] came from.
#[derive(Debug)]
pub enum Source {
    /// The active Omarchy theme.
    Omarchy { colors_file: PathBuf },
    /// The built-in palette, because the Omarchy theme could not be used.
    Fallback { reason: String },
}

/// A palette with its name and origin.
#[derive(Debug)]
pub struct Theme {
    /// The theme's directory name, e.g. `tokyo-night`.
    pub name: Option<String>,
    pub palette: Palette,
    pub source: Source,
    /// What `omagram.toml` sets beyond the palette.
    pub tweaks: Tweaks,
}

/// The `[syntax]` and `[marks]` of a `omagram.toml`: colors by name
/// (`keyword`, `comment` …; `changed`, `compared`, `removed`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tweaks {
    /// Where they come from; `None` without a `omagram.toml`.
    pub file: Option<PathBuf>,
    pub syntax: HashMap<String, Rgb>,
    pub marks: HashMap<String, Rgb>,
}

/// The file beside `colors.toml` that adjusts Omagram.
pub const TWEAKS_FILE: &str = "omagram.toml";

impl Theme {
    /// Loads the active Omarchy theme, or the built-in palette when there is
    /// none (another desktop, a broken theme). Never fails.
    pub fn load_current() -> Self {
        match current_dir() {
            Some(state) => Self::load_from(&state),
            None => Self::fallback("cannot locate the state directory (HOME is not set)".into()),
        }
    }

    /// Loads the theme from an Omarchy `current/` state directory, the one
    /// holding `theme.name` and `theme/colors.toml`.
    pub fn load_from(current: &Path) -> Self {
        let colors_file = current.join("theme").join("colors.toml");
        let tweaks_file = current.join("theme").join(TWEAKS_FILE);
        let sections = fs::read_to_string(&tweaks_file)
            .map(|text| parse_sections(&text))
            .unwrap_or_default();
        let colors = |section: &str| -> HashMap<String, Rgb> {
            sections
                .get(section)
                .into_iter()
                .flatten()
                .filter_map(|(k, v)| Some((k.clone(), Rgb::parse_hex(v)?)))
                .collect()
        };
        let tweaks = Tweaks {
            file: tweaks_file.is_file().then(|| tweaks_file.clone()),
            syntax: colors("syntax"),
            marks: colors("marks"),
        };
        let extra = sections.get("palette").cloned().unwrap_or_default();
        match Palette::load_with(&colors_file, &extra) {
            Ok(palette) => Self {
                name: read_name(current),
                palette,
                source: Source::Omarchy { colors_file },
                tweaks,
            },
            Err(err) => Self::fallback(err.to_string()),
        }
    }

    fn fallback(reason: String) -> Self {
        Self {
            name: None,
            palette: Palette::fallback(),
            source: Source::Fallback { reason },
            tweaks: Tweaks::default(),
        }
    }
}

/// `$XDG_STATE_HOME/omarchy/current`, by default `~/.local/state/omarchy/current`.
/// Omagram watches this directory: switching themes replaces what is in it.
pub fn current_dir() -> Option<PathBuf> {
    let state = match std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var_os("HOME")?).join(".local/state"),
    };
    Some(state.join("omarchy/current"))
}

fn read_name(current: &Path) -> Option<String> {
    let name = fs::read_to_string(current.join("theme.name")).ok()?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// Reads `key = "value"` lines the way `omarchy-theme-color` does: comments,
/// section headers and lines without `=` are skipped, quotes are optional and
/// a quoted value ends at its closing quote.
fn parse_pairs(text: &str) -> HashMap<String, String> {
    let mut pairs = HashMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key: String = key
            .chars()
            .filter(|c| !matches!(c, '"' | '\'' | ' ' | '\t'))
            .collect();
        if key.is_empty() || key.starts_with('#') {
            continue;
        }
        let value = match value.find(['"', '\'']) {
            Some(open) => {
                let rest = &value[open + 1..];
                &rest[..rest.find(['"', '\'']).unwrap_or(rest.len())]
            }
            None => value.trim(),
        };
        pairs.insert(key, value.to_owned());
    }
    pairs
}

/// `key = "value"` lines by `[section]`; lines before any section are
/// left out.
fn parse_sections(text: &str) -> HashMap<String, HashMap<String, String>> {
    let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = Some(name.trim().to_owned());
            continue;
        }
        if let Some(section) = &current {
            sections
                .entry(section.clone())
                .or_default()
                .extend(parse_pairs(line));
        }
    }
    sections
}

/// The fallback cascade of `omarchy-theme-color`, restricted to the keys
/// Omagram uses.
struct Cascade {
    raw: HashMap<String, String>,
    colors: HashMap<&'static str, Rgb>,
}

impl Cascade {
    fn new(raw: HashMap<String, String>) -> Self {
        Self {
            raw,
            colors: HashMap::new(),
        }
    }

    /// The color a theme file defines under `key`, if it is a hex color.
    fn raw(&self, key: &str) -> Option<Rgb> {
        self.raw.get(key).and_then(|v| Rgb::parse_hex(v))
    }

    fn get(&self, key: &str) -> Option<Rgb> {
        self.colors.get(key).copied()
    }

    /// Sets `key` from the first of `candidates` that resolves. A candidate is
    /// tried as an already resolved name first, then as a raw file key.
    fn alias(&mut self, key: &'static str, candidates: &[&str]) {
        if self.get(key).is_some() {
            return;
        }
        let found = candidates
            .iter()
            .find_map(|c| self.get(c).or_else(|| self.raw(c)));
        if let Some(color) = found {
            self.colors.insert(key, color);
        }
    }

    fn derive(&mut self, key: &'static str, from: &str, toward: Rgb, amount: f32) {
        if self.get(key).is_none()
            && let Some(base) = self.get(from)
        {
            self.colors.insert(key, base.mix(toward, amount));
        }
    }

    fn resolve(mut self, light_marker: bool) -> Result<Palette, ThemeError> {
        // Canonical names win; then the legacy short names; then ANSI colors.
        for key in CANONICAL_KEYS {
            if let Some(color) = self.raw(key) {
                self.colors.insert(key, color);
            }
        }
        self.alias("background", &["bg", "color0"]);
        self.alias("dark_background", &["dark_bg"]);
        self.alias("darker_background", &["darker_bg"]);
        self.alias("lighter_background", &["lighter_bg"]);
        self.alias("foreground", &["fg", "color7"]);
        self.alias("dark_foreground", &["dark_fg"]);
        self.alias("light_foreground", &["light_fg"]);
        self.alias("bright_foreground", &["bright_fg"]);

        let background = self.get("background").ok_or(ThemeError::NoBackground)?;
        let foreground = self.get("foreground").ok_or(ThemeError::NoForeground)?;

        self.alias("red", &["color1"]);
        self.alias("green", &["color2"]);
        self.alias("yellow", &["color3"]);
        self.alias("blue", &["color4"]);
        self.alias("magenta", &["color5", "purple"]);
        self.alias("cyan", &["color6"]);
        self.alias("bright_red", &["color9"]);
        self.alias("bright_green", &["color10"]);
        self.alias("bright_yellow", &["color11"]);
        self.alias("bright_blue", &["color12"]);
        self.alias("bright_magenta", &["color13", "bright_purple"]);
        self.alias("bright_cyan", &["color14"]);

        // Omarchy maps background to color0 and foreground to color7 before
        // these fallbacks run.
        self.alias("light_foreground", &["foreground"]);
        self.alias("bright_foreground", &["color15", "foreground"]);
        self.alias("lighter_background", &["background"]);
        self.alias("dark_foreground", &["color8", "foreground"]);
        self.alias("muted", &["color8", "dark_foreground"]);
        self.alias(
            "selection",
            &["selection_background", "color8", "background"],
        );
        self.alias("selection_foreground", &["bright_foreground"]);
        self.alias("orange", &["yellow"]);
        self.derive("brown", "orange", Rgb::BLACK, 0.5);

        self.derive("dark_background", "background", Rgb::BLACK, 0.25);
        self.derive("darker_background", "background", Rgb::BLACK, 0.5);
        for (bright, base) in [
            ("bright_red", "red"),
            ("bright_yellow", "yellow"),
            ("bright_green", "green"),
            ("bright_cyan", "cyan"),
            ("bright_blue", "blue"),
            ("bright_magenta", "magenta"),
        ] {
            self.derive(bright, base, Rgb::WHITE, 0.2);
        }
        // Omarchy has no accent fallback (every bundled theme sets one).
        self.alias("accent", &["blue", "foreground"]);

        let mode = self.mode(light_marker, background);
        // ANSI-only themes may lack hues; plain text color keeps them visible.
        let c = |key: &str| self.get(key).unwrap_or(foreground);
        Ok(Palette {
            mode,
            accent: c("accent"),
            selection: c("selection"),
            selection_foreground: c("selection_foreground"),
            muted: c("muted"),
            background,
            dark_background: c("dark_background"),
            darker_background: c("darker_background"),
            lighter_background: c("lighter_background"),
            foreground,
            dark_foreground: c("dark_foreground"),
            light_foreground: c("light_foreground"),
            bright_foreground: c("bright_foreground"),
            red: c("red"),
            yellow: c("yellow"),
            orange: c("orange"),
            green: c("green"),
            cyan: c("cyan"),
            blue: c("blue"),
            magenta: c("magenta"),
            brown: c("brown"),
            bright_red: c("bright_red"),
            bright_yellow: c("bright_yellow"),
            bright_green: c("bright_green"),
            bright_cyan: c("bright_cyan"),
            bright_blue: c("bright_blue"),
            bright_magenta: c("bright_magenta"),
        })
    }

    /// `mode`, then legacy `theme_type`, then the `light.mode` marker, then
    /// the background's brightness.
    fn mode(&self, light_marker: bool, background: Rgb) -> Mode {
        let declared = self.raw.get("mode").or_else(|| self.raw.get("theme_type"));
        match declared.map(String::as_str) {
            Some("light") => Mode::Light,
            Some("dark") => Mode::Dark,
            _ if light_marker => Mode::Light,
            _ if background.channel_sum() > 382 => Mode::Light,
            _ => Mode::Dark,
        }
    }
}

const CANONICAL_KEYS: [&str; 26] = [
    "accent",
    "selection",
    "selection_foreground",
    "muted",
    "background",
    "dark_background",
    "darker_background",
    "lighter_background",
    "foreground",
    "dark_foreground",
    "light_foreground",
    "bright_foreground",
    "red",
    "yellow",
    "orange",
    "green",
    "cyan",
    "blue",
    "magenta",
    "brown",
    "bright_red",
    "bright_yellow",
    "bright_green",
    "bright_cyan",
    "bright_blue",
    "bright_magenta",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omagram_toml_adjusts_palette_syntax_and_marks() {
        let dir = std::env::temp_dir().join(format!("omagram-tweaks-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("theme")).unwrap();
        fs::write(
            dir.join("theme/colors.toml"),
            "background = \"#101010\"\nforeground = \"#c0c0c0\"\naccent = \"#111111\"\n",
        )
        .unwrap();
        let theme = Theme::load_from(&dir);
        assert_eq!(theme.palette.accent.to_string(), "#111111");
        assert_eq!(theme.tweaks, Tweaks::default());

        fs::write(
            dir.join("theme").join(TWEAKS_FILE),
            "# made by Omarchy\nignored = \"#ffffff\"\n[palette]\naccent = \"#222222\"\n\n[syntax]\nkeyword = \"#333333\"\ncomment = \"not a color\"\n[marks]\nchanged = \"#444444\"\n",
        )
        .unwrap();
        let theme = Theme::load_from(&dir);
        assert_eq!(theme.palette.accent.to_string(), "#222222");
        assert_eq!(theme.palette.background.to_string(), "#101010");
        assert_eq!(theme.tweaks.syntax.len(), 1);
        assert_eq!(theme.tweaks.syntax["keyword"].to_string(), "#333333");
        assert_eq!(theme.tweaks.marks["changed"].to_string(), "#444444");
        assert!(theme.tweaks.file.is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_hex() {
        assert_eq!(Rgb::parse_hex("#1a1B26"), Some(Rgb::new(0x1a, 0x1b, 0x26)));
        assert_eq!(Rgb::parse_hex("1a1b26"), None);
        assert_eq!(Rgb::parse_hex("#fff"), None);
        assert_eq!(Rgb::parse_hex("rgb(1,2,3)"), None);
    }

    #[test]
    fn mixes_like_omarchy() {
        // mix_color "#1a1b26" "#000000" 25% -> #14141d
        let mixed = Rgb::new(0x1a, 0x1b, 0x26).mix(Rgb::BLACK, 0.25);
        assert_eq!(mixed.to_string(), "#14141d");
    }

    #[test]
    fn reads_canonical_theme() {
        let p = Palette::fallback();
        assert_eq!(p.mode, Mode::Dark);
        assert_eq!(p.accent.to_string(), "#7aa2f7");
        assert_eq!(p.dark_background.to_string(), "#13141c");
        // Not in the file: derived from bright_foreground.
        assert_eq!(p.selection_foreground, p.bright_foreground);
    }

    #[test]
    fn resolves_ansi_only_theme() {
        let text = r##"
            color0 = "#101010"
            color1 = "#aa0000"
            color4 = "#0000aa"
            color7 = "#c0c0c0"
            color8 = "#505050"
            color15 = '#ffffff' # inline comment
        "##;
        let p = Palette::parse(text, false).unwrap();
        assert_eq!(p.background.to_string(), "#101010");
        assert_eq!(p.foreground.to_string(), "#c0c0c0");
        assert_eq!(p.red.to_string(), "#aa0000");
        assert_eq!(p.bright_red, Rgb::new(0xaa, 0, 0).mix(Rgb::WHITE, 0.2));
        assert_eq!(p.muted.to_string(), "#505050");
        assert_eq!(p.bright_foreground.to_string(), "#ffffff");
        assert_eq!(p.accent.to_string(), "#0000aa");
        assert_eq!(
            p.dark_background,
            Rgb::new(0x10, 0x10, 0x10).mix(Rgb::BLACK, 0.25)
        );
        // No green anywhere: plain text color rather than nothing.
        assert_eq!(p.green, p.foreground);
    }

    #[test]
    fn short_names_and_mode_detection() {
        let text = "[colors]\nbg = \"#fafafa\"\nfg = \"#202020\"\n";
        let p = Palette::parse(text, false).unwrap();
        assert_eq!(p.background.to_string(), "#fafafa");
        assert_eq!(p.mode, Mode::Light);

        let dark_bg = "background = \"#202020\"\nforeground = \"#fafafa\"\n";
        assert_eq!(Palette::parse(dark_bg, false).unwrap().mode, Mode::Dark);
        assert_eq!(Palette::parse(dark_bg, true).unwrap().mode, Mode::Light);
        let typed = format!("theme_type = \"light\"\n{dark_bg}");
        assert_eq!(Palette::parse(&typed, false).unwrap().mode, Mode::Light);
    }

    #[test]
    fn canonical_wins_over_legacy() {
        let text = "background = \"#111111\"\nbg = \"#222222\"\nforeground = \"#eeeeee\"\n";
        let p = Palette::parse(text, false).unwrap();
        assert_eq!(p.background.to_string(), "#111111");
    }

    #[test]
    fn missing_background_is_an_error() {
        let err = Palette::parse("foreground = \"#ffffff\"\n", false).unwrap_err();
        assert!(matches!(err, ThemeError::NoBackground));
    }

    #[test]
    fn falls_back_without_theme() {
        let theme = Theme::load_from(Path::new("/nonexistent/omarchy/current"));
        assert!(matches!(theme.source, Source::Fallback { .. }));
        assert_eq!(theme.palette, Palette::fallback());
    }

    /// Every theme Omarchy ships resolves without falling back to defaults
    /// for its main colors. Skipped where Omarchy is not installed.
    #[test]
    fn contrast_is_wcag() {
        assert!((Rgb::BLACK.contrast(Rgb::WHITE) - 21.0).abs() < 0.01);
        assert!((Rgb::WHITE.contrast(Rgb::WHITE) - 1.0).abs() < 0.001);
        let faint = Rgb::new(0xdd, 0xdd, 0xdd);
        let fixed = faint.legible_on(&[Rgb::WHITE], Rgb::BLACK, 3.0);
        assert!(fixed.contrast(Rgb::WHITE) >= 3.0 && fixed != Rgb::BLACK);
    }

    /// Every bundled Omarchy theme reads: text on the background, every
    /// token color, dimmed text on both backgrounds.
    #[test]
    fn bundled_omarchy_themes_read_well() {
        let Ok(dirs) = fs::read_dir("/usr/share/omarchy/themes") else {
            return;
        };
        for dir in dirs.flatten() {
            let Ok(raw) = Palette::load(&dir.path().join("colors.toml")) else {
                continue;
            };
            let name = dir.file_name();
            let p = raw.legible();
            assert!(p.foreground.contrast(p.background) >= 4.5, "{name:?} text");
            for (rule, color) in tokens::token_colors(&p) {
                let ratio = color.contrast(p.background);
                assert!(ratio >= tokens::MIN_CONTRAST, "{name:?} {rule}: {ratio:.2}");
            }
            for bg in [p.background, p.dark_background] {
                let ratio = p.dark_foreground.contrast(bg);
                assert!(
                    ratio >= tokens::MIN_CONTRAST,
                    "{name:?} dimmed text: {ratio:.2}"
                );
            }
        }
    }

    #[test]
    fn bundled_omarchy_themes_resolve() {
        let Ok(entries) = fs::read_dir("/usr/share/omarchy/themes") else {
            return;
        };
        for entry in entries.flatten() {
            let colors = entry.path().join("colors.toml");
            if !colors.is_file() {
                continue;
            }
            let palette =
                Palette::load(&colors).unwrap_or_else(|e| panic!("{}: {e}", colors.display()));
            assert_ne!(
                palette.background,
                palette.foreground,
                "{}",
                colors.display()
            );
        }
    }
}
