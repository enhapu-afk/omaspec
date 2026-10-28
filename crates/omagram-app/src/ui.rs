//! The Omarchy look shared by Omagram's windows: colors from the active
//! theme, the monospace font Omarchy is set to.

use std::sync::LazyLock;

use iced::widget::container;
use iced::{Color, Element, Font, Length, Theme};
use omagram_theme::{Palette, Rgb};

pub fn color(c: Rgb) -> Color {
    Color::from_rgb8(c.r, c.g, c.b)
}

/// The color `t` of the way from `a` to `b` (`t` from 0 to 1).
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Rgb {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
    }
}

/// Text that keeps to the background of the bars and the sidebar: dimmed,
/// yet as readable as body text (4.5:1) on both backgrounds.
pub fn quiet(p: &Palette) -> Rgb {
    p.dark_foreground
        .legible_on(&[p.background, p.dark_background], p.foreground, 4.5)
}

/// The shape of the play bar's options, which tabs, the sidebar and the
/// status line take too: round ends, `fill` behind, `edge` round it.
pub fn pill<'a, M: 'a>(
    content: impl Into<Element<'a, M>>,
    fill: Option<Color>,
    edge: Option<Color>,
) -> container::Container<'a, M> {
    container(content).style(move |_: &Theme| container::Style {
        background: fill.map(Into::into),
        border: iced::Border {
            color: edge.unwrap_or(Color::TRANSPARENT),
            width: if edge.is_some() { 1.0 } else { 0.0 },
            radius: 999.0.into(),
        },
        ..container::Style::default()
    })
}

/// A number or a sign on a color, as the play bar numbers its options.
pub fn badge<'a, M: 'a>(s: impl Into<String>, fill: Rgb, ink: Rgb, font: Font) -> Element<'a, M> {
    pill(
        iced::widget::text(s.into())
            .font(Font {
                weight: iced::font::Weight::Bold,
                ..font
            })
            .size(10.5)
            .color(color(ink))
            .wrapping(iced::widget::text::Wrapping::None),
        Some(color(fill)),
        None,
    )
    .padding([0, 6])
    .into()
}

/// `c` at opacity `a`.
pub fn alpha(c: Rgb, a: f32) -> Color {
    Color { a, ..color(c) }
}

/// An iced theme from an Omarchy palette.
pub fn iced_theme(name: &str, p: &Palette) -> Theme {
    Theme::custom(
        name.to_owned(),
        iced::theme::Palette {
            background: color(p.background),
            text: color(p.foreground),
            primary: color(p.accent),
            success: color(p.green),
            warning: color(p.yellow),
            danger: color(p.red),
        },
    )
}

/// The family fontconfig resolved `monospace` to when Omagram started;
/// `omarchy font set` changes it. Falls back to iced's monospace font.
pub fn mono_font() -> Font {
    static FONT: LazyLock<Font> = LazyLock::new(current_mono_font);
    *FONT
}

/// The family fontconfig resolves `monospace` to now (a new `fc-match`
/// reads the current configuration).
pub fn current_mono_font() -> Font {
    let family = std::process::Command::new("fc-match")
        .args(["-f", "%{family[0]}", "monospace"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|f| f.trim().to_owned())
        .filter(|f| !f.is_empty());
    match family {
        Some(family) => Font::with_name(static_name(family)),
        None => Font::MONOSPACE,
    }
}

/// Fonts are looked up by a `'static` name: each family is kept once.
fn static_name(family: String) -> &'static str {
    static NAMES: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let Ok(mut names) = NAMES.lock() else {
        return Box::leak(family.into_boxed_str());
    };
    if let Some(name) = names.iter().find(|n| **n == family) {
        return name;
    }
    let name: &'static str = Box::leak(family.into_boxed_str());
    names.push(name);
    name
}

/// Where `omarchy font set` writes the user's fontconfig
/// (`$XDG_CONFIG_HOME/fontconfig`, by default `~/.config/fontconfig`).
pub fn fontconfig_dir() -> Option<std::path::PathBuf> {
    let config = match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        Some(dir) => std::path::PathBuf::from(dir),
        None => std::path::PathBuf::from(std::env::var_os("HOME")?).join(".config"),
    };
    Some(config.join("fontconfig"))
}

/// The first line of `s`, cut to 140 characters.
pub fn one_line(s: &str) -> String {
    let line = s.lines().next().unwrap_or_default();
    if line.chars().count() > 140 {
        line.chars().take(139).chain(['…']).collect()
    } else {
        line.to_owned()
    }
}

/// A one-line bar on the theme's dark background.
pub fn bar<'a, M: 'a>(content: Element<'a, M>, p: &Palette) -> Element<'a, M> {
    let bg = color(p.dark_background);
    container(content)
        .width(Length::Fill)
        .padding([4, 12])
        .style(move |_: &Theme| container::Style::default().background(bg))
        .into()
}

/// Writes a headless screenshot to `dir/name.png`; iced_test adds the
/// renderer's name (`name-wgpu.png`).
#[cfg(test)]
pub fn snapshot<M>(
    ui: &mut iced_test::simulator::Simulator<'_, M>,
    theme: &Theme,
    dir: &std::path::Path,
    name: &str,
) {
    // The first frame starts uploading large images in the background.
    let _ = ui.snapshot(theme).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let shot = ui.snapshot(theme).unwrap();
    // iced_test never overwrites a file.
    for renderer in ["wgpu", "tiny-skia"] {
        let _ = std::fs::remove_file(dir.join(format!("{name}-{renderer}.png")));
    }
    shot.matches_image(dir.join(format!("{name}.png"))).unwrap();
}

/// A desktop notification: Omarchy's own when there is one (it looks like
/// the rest of the desktop), else `notify-send`. Best effort, in the
/// background; nothing in tests.
pub fn notify(headline: &str, body: &str) {
    if cfg!(test) {
        return;
    }
    let (headline, body) = (headline.to_owned(), body.to_owned());
    std::thread::spawn(move || {
        let omarchy = std::process::Command::new("omarchy-notification-send")
            .args(["--app-name", "Omagram", "-g", "󰈸", &headline, &body])
            .status();
        if !omarchy.is_ok_and(|s| s.success()) {
            let _ = std::process::Command::new("notify-send")
                .args(["--app-name=Omagram", &headline, &body])
                .status();
        }
    });
}
