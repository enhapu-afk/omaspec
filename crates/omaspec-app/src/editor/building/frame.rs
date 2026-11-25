//! A frame of the build: the source's panel and the diagram as far as it
//! is built, as one SVG.

use std::collections::HashMap;
use std::fmt::Write as _;

use anyhow::Result;
use omaspec_core::Language;
use omaspec_render::export::usvg;
use omaspec_theme::{Palette, Rgb};
use omaspec_uml::map::HeadFill;
use scrive_core::{HighlightSpan, Highlighter, SpanStyle};

use super::picture::{Part, Picture};
use super::{
    AHEAD, CODE_SIZE, HIGHEST, INSET, LETTER, LINE_HEIGHT, LOWEST, MARGIN, NARROWEST, SMALL,
    Script, WIDEST, WIDEST_LINE, syntax,
};

/// The source, each line with its colors.
pub(super) struct Code {
    pub(super) lines: Vec<String>,
    /// The lines shown, by their place in the source: all but its lines
    /// of comments, which draw nothing (REQ-123).
    pub(super) shown: Vec<usize>,
    pub(super) spans: Vec<Vec<HighlightSpan>>,
    pub(super) font: String,
    /// How wide a letter of its face is at [`CODE_SIZE`], as the frames
    /// set it.
    pub(super) letter: f32,
}

impl Code {
    pub(super) fn of(
        text: &str,
        language: Language,
        palette: &Palette,
        tweaks: &HashMap<String, Rgb>,
    ) -> Code {
        let highlighter = Highlighter::new(
            syntax::grammar(language),
            syntax::token_theme(palette, tweaks),
        );
        // A line ends before a carriage return, as the highlighter's do.
        let mut lines: Vec<String> = text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
            .collect();
        // A file that ends in a newline has no line after it.
        if lines.len() > 1 && lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        let font = crate::ui::mono_family().unwrap_or_else(|| "monospace".to_owned());
        let shown = comments(&lines, language)
            .into_iter()
            .enumerate()
            .filter(|(_, comment)| !comment)
            .map(|(at, _)| at)
            .collect();
        Code {
            spans: highlighter.highlight(text),
            shown,
            lines,
            letter: letter_of(&font),
            font,
        }
    }

    /// Each line as its runs of words in one color, a tab as four
    /// spaces: for a page, which shows them whole.
    pub(super) fn runs(&self, plain: Rgb) -> Vec<Vec<omaspec_render::export::building::Run>> {
        let color = |rgb: Rgb| omaspec_uml::Color(rgb.r, rgb.g, rgb.b);
        self.lines
            .iter()
            .enumerate()
            .map(|(at, line)| {
                let spans = self.spans.get(at).map_or(&[][..], Vec::as_slice);
                let mut runs: Vec<omaspec_render::export::building::Run> = Vec::new();
                for (byte, c) in line.char_indices() {
                    let style = spans
                        .iter()
                        .find(|span| {
                            (span.range.start as usize..span.range.end as usize).contains(&byte)
                        })
                        .map(|span| &span.style);
                    let (fill, bold, italic) = style.map_or((color(plain), false, false), |s| {
                        (omaspec_uml::Color(s.fg.r, s.fg.g, s.fg.b), s.bold, s.italic)
                    });
                    let text = if c == '\t' {
                        "    ".to_owned()
                    } else {
                        c.to_string()
                    };
                    match runs.last_mut() {
                        Some(run)
                            if run.color == fill && run.bold == bold && run.italic == italic =>
                        {
                            run.text.push_str(&text);
                        }
                        _ => runs.push(omaspec_render::export::building::Run {
                            text,
                            color: fill,
                            bold,
                            italic,
                        }),
                    }
                }
                runs
            })
            .collect()
    }

    /// How many letters the widest line shows.
    pub(super) fn letters(&self) -> usize {
        self.shown
            .iter()
            .map(|&at| self.lines[at].replace('\t', "    ").chars().count())
            .max()
            .unwrap_or(0)
            .min(WIDEST_LINE + 1)
    }
}

/// Which of `lines` are comments alone: `%%` in Mermaid, an `%%{init}%%`
/// among them; `'` and `/' … '/` in PlantUML.
pub(super) fn comments(lines: &[String], language: Language) -> Vec<bool> {
    let mut block = false;
    lines
        .iter()
        .map(|line| {
            let text = line.trim();
            match language {
                Language::Mermaid => text.starts_with("%%"),
                Language::PlantUml if block => {
                    block = !text.contains("'/");
                    block || text.ends_with("'/")
                }
                Language::PlantUml => match text.strip_prefix("/'") {
                    Some(rest) if !rest.contains("'/") => {
                        block = true;
                        true
                    }
                    Some(_) => text.ends_with("'/"),
                    None => text.starts_with('\''),
                },
            }
        })
        .collect()
}

/// Where the source and the diagram stand in a frame.
pub(super) struct Layout {
    pub(super) width: f32,
    pub(super) height: f32,
    /// The source's panel: its left, its width; the gutter of numbers.
    pub(super) panel: (f32, f32),
    pub(super) gutter: f32,
    /// How many lines the panel shows.
    pub(super) rows: usize,
    /// The diagram: its left and top, and its scale.
    pub(super) at: (f32, f32),
    pub(super) scale: f32,
    /// The diagram alone, its source not beside it (`--no-code`).
    pub(super) bare: bool,
}

impl Layout {
    pub(super) fn of(
        code: &Code,
        picture: &Picture,
        asked: Option<u32>,
        bare: bool,
    ) -> Result<Layout> {
        let letter = code.letter;
        let digits = code.lines.len().to_string().len().max(2);
        let gutter = digits as f32 * letter + 14.0;
        // Without the source its panel takes no room, nor the gap after it.
        let (panel, gaps) = match bare {
            true => (0.0, 2.0),
            false => (gutter + code.letters() as f32 * letter + 2.0 * INSET, 3.0),
        };
        let narrowest = if bare { 200.0 } else { NARROWEST };
        let (w, h) = picture.size;
        anyhow::ensure!(w > 0.0 && h > 0.0, "the diagram is empty");
        let tallest = (HIGHEST - 2.0 * MARGIN) / h;
        let (width, scale) = match asked {
            Some(width) => {
                let width = width as f32;
                let room = width - panel - gaps * MARGIN;
                anyhow::ensure!(
                    room >= 120.0,
                    "--width {width} leaves the diagram no room beside the source"
                );
                (width, (room / w).min(tallest))
            }
            None => {
                // Never larger than drawn, and always whole in the frame.
                let scale = 1.0_f32
                    .min((WIDEST - panel - gaps * MARGIN) / w)
                    .min(tallest);
                let width = (panel + w * scale + gaps * MARGIN).clamp(narrowest, WIDEST);
                (width, scale)
            }
        };
        let height = (h * scale + 2.0 * MARGIN).clamp(LOWEST, HIGHEST);
        let rows = ((height - 2.0 * MARGIN - 2.0 * INSET) / LINE_HEIGHT)
            .floor()
            .max(1.0) as usize;
        // The diagram in the room right of the panel, in its middle.
        let room_left = MARGIN + panel + (gaps - 2.0) * MARGIN;
        let room = width - room_left - MARGIN;
        let at = (
            room_left + (room - w * scale).max(0.0) / 2.0,
            MARGIN + (height - 2.0 * MARGIN - h * scale).max(0.0) / 2.0,
        );
        Ok(Layout {
            width: width.round(),
            height: height.round(),
            panel: (MARGIN, panel),
            gutter,
            rows,
            at,
            scale,
            bare,
        })
    }

    /// The frame at `at` seconds, as an SVG.
    pub(super) fn frame(
        &self,
        code: &Code,
        picture: &Picture,
        script: &Script,
        at: f32,
        palette: &Palette,
    ) -> String {
        let mut svg = String::new();
        let _ = write!(
            svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><rect width="{w}" height="{h}" fill="{bg}"/>"#,
            w = self.width,
            h = self.height,
            bg = palette.background,
        );
        if !self.bare {
            self.source(&mut svg, code, script.line(at), palette);
        }
        self.diagram(&mut svg, picture, script, at);
        svg.push_str("</svg>");
        svg
    }

    /// The source's panel: the lines the caret passed, the one it stands
    /// on lit, the ones ahead dimmed.
    pub(super) fn source(
        &self,
        svg: &mut String,
        code: &Code,
        current: Option<usize>,
        palette: &Palette,
    ) {
        let (left, width) = self.panel;
        let top = MARGIN;
        let height = self.height - 2.0 * MARGIN;
        let _ = write!(
            svg,
            r#"<rect x="{left}" y="{top}" width="{width}" height="{height}" rx="10" fill="{}"/>"#,
            palette.dark_background,
        );
        // The lines shown; the caret on a comment stays on the line shown
        // before it.
        let shown = &code.shown;
        let lines = shown.len();
        let lit = current.and_then(|c| shown.partition_point(|&at| at <= c).checked_sub(1));
        // The caret's line stays in sight, a third down the panel.
        let first = lit
            .unwrap_or(0)
            .saturating_sub(self.rows / 3)
            .min(lines.saturating_sub(self.rows));
        let letter = code.letter;
        let text_left = left + INSET + self.gutter;
        // Quoted, so a family whose name starts with a digit still reads;
        // the generic one where the family is missing.
        let font = format!("'{}', monospace", xml(&code.font.replace(['\'', '\\'], "")));
        for row in 0..self.rows.min(lines - first.min(lines)) {
            let line = shown[first + row];
            let y = top + INSET + row as f32 * LINE_HEIGHT;
            let ahead = current.is_none_or(|current| line > current);
            let here = lit == Some(first + row);
            if here {
                let _ = write!(
                    svg,
                    r#"<rect x="{}" y="{}" width="{}" height="{LINE_HEIGHT}" rx="4" fill="{}" fill-opacity="0.55"/>"#,
                    left + 4.0,
                    y,
                    width - 8.0,
                    palette.selection,
                );
            }
            let baseline = y + LINE_HEIGHT * 0.72;
            let _ = write!(
                svg,
                r#"<text x="{}" y="{baseline}" font-family="{font}" font-size="{CODE_SIZE}" text-anchor="end" fill="{}" fill-opacity="{}">{}</text>"#,
                text_left - 10.0,
                palette.muted,
                if ahead { AHEAD } else { 1.0 },
                line + 1,
            );
            let (words, shown) = styled(
                &code.lines[line],
                code.spans.get(line).map_or(&[][..], Vec::as_slice),
                palette.foreground,
            );
            let _ = write!(
                svg,
                r#"<text x="{text_left}" y="{baseline}" font-family="{font}" font-size="{CODE_SIZE}" xml:space="preserve" fill-opacity="{}">{words}</text>"#,
                if ahead { AHEAD } else { 1.0 },
            );
            if here {
                let _ = write!(
                    svg,
                    r#"<rect x="{}" y="{}" width="2" height="{}" fill="{}"/>"#,
                    text_left + shown as f32 * letter + 1.0,
                    y + 3.0,
                    LINE_HEIGHT - 6.0,
                    palette.accent,
                );
            }
        }
    }

    /// The diagram as far as it is built at `at`.
    pub(super) fn diagram(&self, svg: &mut String, picture: &Picture, script: &Script, at: f32) {
        let (w, h) = picture.size;
        let built = self.built(picture, script, at);
        let (x, y) = self.at;
        let _ = write!(
            svg,
            r#"<svg x="{x}" y="{y}" width="{}" height="{}" viewBox="0 0 {w} {h}">{built}</svg>"#,
            w * self.scale,
            h * self.scale,
        );
    }

    /// What the picture's SVG holds, in the units of its size, with what
    /// is not built yet left out, a box that grows in grown so far and a
    /// line drawn so far.
    pub(super) fn built(&self, picture: &Picture, script: &Script, at: f32) -> String {
        if !picture.by_parts() {
            return uncovered(picture, script, at);
        }
        sized(picture, inner(&self.set_down(picture, script, at)))
    }

    /// The picture's SVG, whole, with what is not built yet left out.
    fn set_down(&self, picture: &Picture, script: &Script, at: f32) -> String {
        let mut svg = picture.svg.clone();
        for part in &picture.parts {
            let (opacity, scale) = match part.of_wire {
                Some(_) => (script.worded(part.line, at), 1.0),
                None => {
                    let grown = ease(script.grown(part.line, at));
                    (grown, SMALL + (1.0 - SMALL) * grown)
                }
            };
            if script.line(at).is_none_or(|current| current < part.line) {
                attribute(&mut svg, part, 0.0, 1.0);
            } else if opacity < 1.0 || scale < 1.0 {
                attribute(&mut svg, part, opacity, scale);
            }
        }
        let Some(transform) = picture.lines_group else {
            return svg;
        };
        let Some(lines) = drawn_lines(picture, script, at, transform) else {
            return svg;
        };
        let open = format!(r#"<g id="{}">"#, omaspec_uml::LINES);
        let Some(start) = svg.find(&open) else {
            return svg;
        };
        let rest = &svg[start..];
        let Some(length) = group_length(rest) else {
            return svg;
        };
        format!(
            "{}{open}{lines}</g>{}",
            &svg[..start],
            &svg[start + length..]
        )
    }
}

/// `part`'s groups at `opacity`, scaled by `scale` about its middle.
pub(super) fn attribute(svg: &mut String, part: &Part, opacity: f32, scale: f32) {
    for (id, transform) in &part.groups {
        let open = format!(r#"<g id="{id}""#);
        let Some(at) = svg.find(&open) else { continue };
        // Its middle where the group draws.
        let local = transform
            .invert()
            .map(|inverse| {
                let mut point = usvg::tiny_skia_path::Point::from_xy(part.middle.0, part.middle.1);
                inverse.map_point(&mut point);
                (point.x, point.y)
            })
            .unwrap_or(part.middle);
        let moved = if (scale - 1.0).abs() > 0.001 {
            format!(
                r#" transform="translate({x} {y}) scale({scale}) translate({nx} {ny})""#,
                x = local.0,
                y = local.1,
                nx = -local.0,
                ny = -local.1,
            )
        } else {
            String::new()
        };
        svg.insert_str(
            at + open.len(),
            &format!(r#" opacity="{opacity:.3}"{moved}"#),
        );
    }
}

/// The map's lines, each drawn as far as it is at `at`, in the drawing's
/// own units, ready to stand in the lines' group drawn in `transform`.
pub(super) fn drawn_lines(
    picture: &Picture,
    script: &Script,
    at: f32,
    transform: usvg::Transform,
) -> Option<String> {
    let inverse = transform.invert()?;
    let mut out = format!(
        r#"<g transform="matrix({} {} {} {} {} {})">"#,
        inverse.sx, inverse.ky, inverse.kx, inverse.sy, inverse.tx, inverse.ty
    );
    let current = script.line(at);
    for wire in &picture.wires {
        if current.is_none_or(|current| current < wire.line) {
            continue;
        }
        let drawn = ease(script.drawn(wire.line, at));
        if drawn <= 0.0 || wire.points.len() < 2 {
            continue;
        }
        let points = stretch(&wire.points, drawn);
        let dash = wire
            .dash
            .map(|(on, off)| format!(r#" stroke-dasharray="{on} {off}""#))
            .unwrap_or_default();
        let _ = write!(
            out,
            r#"<polyline points="{}" fill="none" stroke="{}" stroke-width="{}" stroke-linejoin="round"{dash}/>"#,
            joined(&points),
            wire.color,
            wire.width,
        );
        if drawn >= 1.0 {
            for head in &wire.heads {
                let fill = match head.fill {
                    HeadFill::Ink => wire.color.clone(),
                    HeadFill::Paper => picture.paper.clone(),
                    HeadFill::None => "none".to_owned(),
                };
                let shape = if head.closed { "polygon" } else { "polyline" };
                let _ = write!(
                    out,
                    r#"<{shape} points="{}" fill="{fill}" stroke="{}" stroke-width="{}" stroke-linejoin="round"/>"#,
                    joined(&head.points),
                    wire.color,
                    wire.width.min(1.5),
                );
            }
        }
    }
    out.push_str("</g>");
    Some(out)
}

/// A picture without parts uncovered from the top down, as far as the
/// lines up to the caret's have drawn, the edge soft.
pub(super) fn uncovered(picture: &Picture, script: &Script, at: f32) -> String {
    let (w, h) = picture.size;
    let current = script.line(at);
    let reached = |line: Option<usize>| -> f32 {
        line.map_or(picture.first_reach(), |line| {
            picture
                .reach
                .iter()
                .filter(|&&(at, _)| at <= line)
                .map(|&(_, bottom)| bottom + 6.0)
                .fold(picture.first_reach(), f32::max)
        })
    };
    // From where the line before left off to where this one goes, as it
    // is written.
    let to = reached(current);
    let from = reached(current.and_then(|line| line.checked_sub(1)));
    let grown = current.map_or(1.0, |line| ease(script.grown(line, at)));
    let edge = if script
        .line(at)
        .is_some_and(|line| line + 1 >= script.starts.len())
        && at >= script.end
    {
        h + 40.0
    } else {
        from + (to - from) * grown
    };
    let soft = 24.0;
    let body = sized(picture, inner(&picture.svg));
    format!(
        r#"<defs><linearGradient id="build-fade" gradientUnits="userSpaceOnUse" x1="0" y1="{a}" x2="0" y2="{b}"><stop offset="0" stop-color="white"/><stop offset="1" stop-color="white" stop-opacity="0"/></linearGradient><mask id="build-reveal" maskUnits="userSpaceOnUse" x="0" y="0" width="{w}" height="{h}"><rect width="{w}" height="{b}" fill="url(#build-fade)"/></mask></defs><g mask="url(#build-reveal)">{body}</g>"#,
        a = edge - soft,
        b = edge,
    )
}

/// `body`, the children of the picture's root, in the units of its size:
/// its own units scaled as its root scales them.
pub(super) fn sized(picture: &Picture, body: &str) -> String {
    let m = picture.root;
    if m.is_identity() {
        return body.to_owned();
    }
    format!(
        r#"<g transform="matrix({} {} {} {} {} {})">{body}</g>"#,
        m.sx, m.ky, m.kx, m.sy, m.tx, m.ty
    )
}

/// The children of `svg`'s root element.
pub(super) fn inner(svg: &str) -> &str {
    let Some(open) = svg.find("<svg") else {
        return svg;
    };
    let Some(close) = svg[open..].find('>') else {
        return svg;
    };
    let start = open + close + 1;
    let end = svg.rfind("</svg>").unwrap_or(svg.len()).max(start);
    &svg[start..end]
}

/// The length of the group `svg` starts with, to its closing tag.
pub(super) fn group_length(svg: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = 0;
    while let Some(next) = svg[at..].find('<') {
        let tag = at + next;
        let end = tag + svg[tag..].find('>')? + 1;
        let text = &svg[tag..end];
        if text.starts_with("</g") {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(end);
            }
        } else if text.starts_with("<g") && !text.ends_with("/>") {
            depth += 1;
        }
        at = end;
    }
    None
}

/// The first `share` of the line through `points`, by its length.
pub(super) fn stretch(points: &[(f32, f32)], share: f32) -> Vec<(f32, f32)> {
    let length: f32 = points
        .windows(2)
        .map(|pair| (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1))
        .sum();
    let mut left = length * share.clamp(0.0, 1.0);
    let mut out = vec![points[0]];
    for pair in points.windows(2) {
        let run = (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
        if run >= left {
            let t = if run > 0.0 { left / run } else { 0.0 };
            out.push((
                pair[0].0 + (pair[1].0 - pair[0].0) * t,
                pair[0].1 + (pair[1].1 - pair[0].1) * t,
            ));
            return out;
        }
        left -= run;
        out.push(pair[1]);
    }
    out
}

pub(super) fn joined(points: &[(f32, f32)]) -> String {
    points
        .iter()
        .map(|(x, y)| format!("{x:.2},{y:.2}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Smooth in and out, from 0 to 1.
pub(super) fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How wide a letter of `family` is at [`CODE_SIZE`], set as the frames
/// set it: twenty letters measured against ten, so the bounds of the
/// first and the last cancel out; [`LETTER`] of the size where it cannot
/// be measured.
fn letter_of(family: &str) -> f32 {
    let family = format!("'{}', monospace", xml(&family.replace(['\'', '\\'], "")));
    let width = |count: usize| -> Option<f32> {
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="4000" height="200"><text x="0" y="150" font-family="{family}" font-size="100" xml:space="preserve">{}</text></svg>"#,
            "0".repeat(count)
        );
        let tree = omaspec_render::export::parse(&svg).ok()?;
        Some(tree.root().abs_bounding_box().width())
    };
    match (width(10), width(20)) {
        (Some(ten), Some(twenty)) if twenty > ten => (twenty - ten) / 10.0 / 100.0 * CODE_SIZE,
        _ => CODE_SIZE * LETTER,
    }
}

/// `text` escaped for XML.
pub(super) fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `line` as tspans in the colors of `spans`, a tab as four spaces, cut
/// to [`WIDEST_LINE`] letters with an ellipsis; and how many letters show.
pub(super) fn styled(line: &str, spans: &[HighlightSpan], plain: Rgb) -> (String, usize) {
    let style_at = |byte: usize| -> Option<&SpanStyle> {
        spans
            .iter()
            .find(|span| (span.range.start as usize..span.range.end as usize).contains(&byte))
            .map(|span| &span.style)
    };
    let mut out = String::new();
    let mut run = String::new();
    let mut run_style: Option<(String, bool, bool)> = None;
    let mut shown = 0;
    let flush = |out: &mut String, run: &mut String, style: &Option<(String, bool, bool)>| {
        if run.is_empty() {
            return;
        }
        let (fill, bold, italic) = style.clone().unwrap_or((plain.to_string(), false, false));
        let _ = write!(
            out,
            r#"<tspan fill="{fill}"{}{}>{}</tspan>"#,
            if bold { r#" font-weight="bold""# } else { "" },
            if italic {
                r#" font-style="italic""#
            } else {
                ""
            },
            xml(run),
        );
        run.clear();
    };
    for (byte, c) in line.char_indices() {
        let letters = if c == '\t' { 4 } else { 1 };
        if shown + letters > WIDEST_LINE {
            flush(&mut out, &mut run, &run_style);
            run_style = None;
            run.push('…');
            shown += 1;
            break;
        }
        let style = style_at(byte).map(|s| {
            (
                format!("#{:02x}{:02x}{:02x}", s.fg.r, s.fg.g, s.fg.b),
                s.bold,
                s.italic,
            )
        });
        if style != run_style {
            flush(&mut out, &mut run, &run_style);
            run_style = style;
        }
        if c == '\t' {
            run.push_str("    ");
        } else {
            run.push(c);
        }
        shown += letters;
    }
    flush(&mut out, &mut run, &run_style);
    (out, shown)
}
