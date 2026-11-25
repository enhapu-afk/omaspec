//! The traffic's frames: the machine under a veil, each transition
//! glowing as the dots pass it, the dots, and a caption under it that
//! counts, drawn as SVG and rasterized without a GPU.

use std::fmt::Write as _;

use omaspec_theme::Palette;

use super::{BLINK, Flow};

/// How high the caption's band is, in pixels.
const CAPTION: f32 = 40.0;
/// How wide a frame is at least, in pixels: a narrow machine stands in
/// its middle.
const NARROWEST: f32 = 560.0;
/// The frame a picture is drawn as large as it fits, where no width is
/// asked for.
const ROOM: (f32, f32) = (1000.0, 860.0);
/// How strong the veil over the machine is.
const VEIL: f32 = 0.72;

/// The colors a frame is drawn in.
#[derive(Clone)]
pub(in crate::editor) struct Colors {
    /// The picture's own paper, which the veil takes.
    pub(in crate::editor) paper: String,
    pub(in crate::editor) ink: String,
    pub(in crate::editor) muted: String,
    pub(in crate::editor) accent: String,
    pub(in crate::editor) light: String,
    pub(in crate::editor) wrong: String,
}

impl Colors {
    pub(in crate::editor) fn of(palette: &Palette, svg: &str) -> Colors {
        Colors {
            paper: super::super::building::picture::paper_of(svg),
            ink: palette.foreground.to_string(),
            muted: palette.muted.to_string(),
            accent: palette.accent.to_string(),
            light: palette.bright_foreground.to_string(),
            wrong: palette.red.to_string(),
        }
    }

    pub(in crate::editor) fn page(&self) -> omaspec_render::export::traffic::Colors {
        let color = |hex: &str| {
            omaspec_uml::parse_color(hex)
                .ok()
                .flatten()
                .unwrap_or(omaspec_uml::Color::BLACK)
        };
        omaspec_render::export::traffic::Colors {
            paper: color(&self.paper),
            ink: color(&self.ink),
            muted: color(&self.muted),
            accent: color(&self.accent),
            light: color(&self.light),
            wrong: color(&self.wrong),
        }
    }
}

/// The picture a frame is drawn on: the machine with a band for the
/// caption under it, and how large it is drawn.
pub(super) struct Stage {
    svg: String,
    /// The picture's own units: where it starts, how large it is without
    /// the band, and how many pixels one is.
    view: (f32, f32, f32, f32),
    /// How wide the frame is in those units, the machine in its middle.
    wide: f32,
    unit: f32,
    /// What the picture is rasterized at, from its size to the frame's.
    pub(super) scale: f32,
    colors: Colors,
}

impl Stage {
    /// How high a frame is in pixels: the machine and its caption's band.
    pub(super) fn height(&self) -> f32 {
        self.view.3 * self.unit + CAPTION
    }

    /// `svg` on a frame `width` pixels wide, else as large as fits a
    /// frame of [`ROOM`], in `colors`.
    pub(super) fn of(svg: &str, width: Option<f32>, colors: &Colors) -> Stage {
        let (x, y, w, h) = view_of(svg);
        let size = omaspec_render::export::parse(svg).map_or(w, |tree| tree.size().width());
        // A width asked for is the frame's, the machine as large as it
        // fits it (three pixels a unit at most); else the machine as large
        // as fits a frame of `ROOM`, the frame `NARROWEST` at least.
        let (unit, frame) = match width {
            Some(width) => {
                let unit = (width / w.max(1.0)).min(3.0);
                (unit, width)
            }
            None => {
                let unit = (ROOM.0 / w.max(1.0))
                    .min((ROOM.1 - CAPTION) / h.max(1.0))
                    .clamp(0.25, 3.0);
                (unit, (w * unit).max(NARROWEST))
            }
        };
        let band = CAPTION / unit;
        // A narrow machine on a wider frame, in its middle, so the caption
        // has room.
        let wide = w.max(frame / unit);
        let svg = super::super::timelapse::widened(
            svg,
            (wide, h + band),
            (wide - w) / 2.0,
            &colors.paper,
        );
        Stage {
            svg,
            view: (x, y, w, h),
            wide,
            unit,
            // A unit of the picture `unit` pixels, however wide its frame.
            scale: unit * w / size.max(1.0),
            colors: colors.clone(),
        }
    }

    /// The frame `t` seconds in.
    pub(super) fn frame(&self, flow: &Flow, t: f32) -> String {
        let (x, y, w, h) = self.view;
        let c = &self.colors;
        let px = |pixels: f32| pixels / self.unit;
        let mut out = String::new();
        let _ = write!(
            out,
            r#"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{}" fill-opacity="{VEIL}"/>"#,
            c.paper
        );
        let most = flow.most();
        for (index, wire) in flow.wires.iter().enumerate() {
            if wire.points.len() < 2 {
                continue;
            }
            let points = points(&wire.points);
            let passed = flow.passed(index, t);
            if passed > 0.0 {
                // Brighter and wider the more have passed.
                let k = (passed / most).sqrt().min(1.0);
                let _ = write!(
                    out,
                    r#"<polyline points="{points}" fill="none" stroke="{}" stroke-width="{:.2}" stroke-opacity="{:.3}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                    c.accent,
                    px(4.0 + 10.0 * k),
                    0.3 * k,
                );
                let _ = write!(
                    out,
                    r#"<polyline points="{points}" fill="none" stroke="{}" stroke-width="{:.2}" stroke-opacity="{:.3}" stroke-linecap="round" stroke-linejoin="round"/>"#,
                    c.light,
                    px(1.4 + 3.2 * k),
                    0.45 + 0.55 * k,
                );
            } else if wire.passes.is_empty() && t > flow.end {
                // Never taken: grey and dashed, blinking a while.
                let since = t - flow.end;
                let blink = if since < BLINK {
                    0.25 + 0.75 * (since * std::f32::consts::PI * 2.5).sin().abs()
                } else {
                    0.7
                };
                let _ = write!(
                    out,
                    r#"<polyline points="{points}" fill="none" stroke="{}" stroke-width="{:.2}" stroke-dasharray="{:.2} {:.2}" stroke-opacity="{blink:.3}" stroke-linecap="round"/>"#,
                    c.muted,
                    px(2.4),
                    px(6.0),
                    px(5.0),
                );
            }
        }
        for dot in &flow.dots {
            let Some((dx, dy)) = dot.at(t) else {
                continue;
            };
            let wrong = dot.wrong.is_some_and(|at| at <= t);
            let fill = if wrong { &c.wrong } else { &c.accent };
            if let Some(at) = dot.wrong.filter(|&at| at <= t) {
                let k = ((t - at) / 0.9).fract();
                let _ = write!(
                    out,
                    r#"<circle cx="{dx:.2}" cy="{dy:.2}" r="{:.2}" fill="none" stroke="{}" stroke-width="{:.2}" stroke-opacity="{:.3}"/>"#,
                    px(5.0 + 12.0 * k),
                    c.wrong,
                    px(1.6),
                    (1.0 - k) * 0.9,
                );
            }
            let _ = write!(
                out,
                r#"<circle cx="{dx:.2}" cy="{dy:.2}" r="{:.2}" fill="{fill}" stroke="{}" stroke-width="{:.2}"/>"#,
                px(5.2),
                c.paper,
                px(1.0),
            );
        }
        // The caption under the whole frame, not in the machine's group.
        let mut under = String::new();
        let (said, wrong) = caption(flow, t);
        let line = y + h + px(CAPTION) / 2.0 + px(5.0);
        let out_of_group = &mut under;
        let _ = write!(
            out_of_group,
            r#"<text x="{:.2}" y="{line:.2}" font-family="sans-serif" font-size="{:.2}" fill="{}">{}</text>"#,
            x + px(12.0),
            px(14.0),
            c.ink,
            xml(&said),
        );
        if wrong > 0 {
            let _ = write!(
                out_of_group,
                r#"<text x="{:.2}" y="{line:.2}" font-family="sans-serif" font-size="{:.2}" text-anchor="end" fill="{}">{}</text>"#,
                x + self.wide - px(12.0),
                px(14.0),
                c.wrong,
                match wrong {
                    1 => "1 deviation".to_owned(),
                    n => format!("{n} deviations"),
                }
            );
        }
        // What moves goes into the machine's group, which `widened` closes
        // just before the picture ends; the caption after it.
        let close = self.svg.rfind("</svg>").unwrap_or(self.svg.len());
        let group = self.svg[..close].rfind("</g>").unwrap_or(close);
        format!(
            "{}{out}{}{under}{}",
            &self.svg[..group],
            &self.svg[group..close],
            &self.svg[close..]
        )
    }
}

/// What the caption says at `t`: the objects, the transitions taken by
/// then of all; and the deviations by then.
pub(super) fn caption(flow: &Flow, t: f32) -> (String, usize) {
    let routed = flow.wires.iter().filter(|w| w.points.len() >= 2).count();
    let taken = flow
        .wires
        .iter()
        .filter(|w| w.points.len() >= 2 && w.passes.iter().any(|&(start, took)| start + took <= t))
        .count();
    let wrong = flow
        .dots
        .iter()
        .filter(|d| d.wrong.is_some_and(|at| at <= t))
        .count();
    let objects = match flow.objects {
        1 => "1 object".to_owned(),
        n => format!("{n} objects"),
    };
    let of = if flow.machine.is_empty() {
        String::new()
    } else {
        format!(" of {}", flow.machine)
    };
    (
        format!("{objects}{of} · {taken} of {routed} transitions taken"),
        wrong,
    )
}

fn points(points: &[(f32, f32)]) -> String {
    points
        .iter()
        .map(|(x, y)| format!("{x:.2},{y:.2}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The picture's `viewBox`: where it starts and how large it is.
fn view_of(svg: &str) -> (f32, f32, f32, f32) {
    view_box(svg).unwrap_or((0.0, 0.0, 400.0, 300.0))
}

/// The picture's `viewBox`, if it has one.
pub(super) fn view_box(svg: &str) -> Option<(f32, f32, f32, f32)> {
    let start = svg.find("<svg")?;
    let tag = &svg[start..start + svg[start..].find('>')?];
    let view: Vec<f32> = tag
        .split(" viewBox=\"")
        .nth(1)?
        .split('"')
        .next()?
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|n| n.parse().ok())
        .collect();
    match view[..] {
        [x, y, w, h] if w > 0.0 && h > 0.0 => Some((x, y, w, h)),
        _ => None,
    }
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
