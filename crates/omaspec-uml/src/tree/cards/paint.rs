//! The cards painted: a palette taken from the map's skin, the folders'
//! frames, soft shadows, arrows for the relations, and the words.

use std::fmt::Write as _;

use super::arrows::{ROUND, chip_rect};
use super::layout::{FRAME_BAND, FRAME_PAD};
use super::{
    CAPTION, Font, Heading, Icon, LABEL, Layout, PAD, PICTURE_ROUND, Placed, RelationKind, SMALL,
    TEXT_X, TITLE, fitted,
};
use crate::color::Color;
use crate::graph::Rect;
use crate::svg::{escape, n};
use crate::text::Family;
use crate::tree::skin::Skin;

/// The map's colors.
struct Palette {
    page: Color,
    card: Color,
    edge: Color,
    /// The room a picture lies in.
    paper: Color,
    paper_edge: Color,
    ink: Color,
    quiet: Color,
    accent: Color,
    /// A folder's frame: its fill, a shade off the page.
    frame: Color,
    /// The shadow's darkness.
    shadow: f32,
    font: String,
}

/// The palette of a skin: its background as the page, cards a step off it
/// towards the boxes' fill, the text in the font's color, and the first
/// of the border and the line colors that is a color, not a gray, as the
/// accent.
fn palette(skin: &Skin) -> Palette {
    let page = skin.background.unwrap_or(Color::WHITE);
    let dark = page.is_dark();
    let ink = skin.font.color;
    let fill = skin.fill.unwrap_or(page);
    let accent = [skin.border, skin.link.color]
        .into_iter()
        .find(|c| colorful(*c))
        .unwrap_or(if dark {
            Color::hex(0x7aa2f7)
        } else {
            Color::hex(0x3d6fd6)
        });
    let card = if dark {
        page.mix(fill, 0.55)
    } else {
        page.mix(Color::WHITE, 0.6)
    };
    Palette {
        page,
        card,
        edge: page.mix(ink, if dark { 0.14 } else { 0.11 }),
        paper: page,
        paper_edge: page.mix(ink, 0.07),
        ink,
        quiet: page.mix(ink, if dark { 0.55 } else { 0.6 }),
        accent,
        frame: page.mix(ink, if dark { 0.045 } else { 0.03 }),
        shadow: if dark { 0.35 } else { 0.09 },
        font: skin.font_name.clone(),
    }
}

/// Whether `c` is a color rather than a gray, and neither too dark nor too
/// light to stand out as one.
fn colorful(c: Color) -> bool {
    let (r, g, b) = (f32::from(c.0), f32::from(c.1), f32::from(c.2));
    let (high, low) = (r.max(g).max(b), r.min(g).min(b));
    high > 60.0 && low < 235.0 && (high - low) / high.max(1.0) > 0.3
}

pub(super) fn svg(layout: &Layout, skin: &Skin, family: Family) -> String {
    let p = palette(skin);
    let (width, height) = layout.size;
    let mut out = String::with_capacity(4096 + layout.cards.len() * 1024);
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#,
        w = n(width),
        h = n(height)
    );
    let _ = write!(
        out,
        r#"<rect x="0" y="0" width="{}" height="{}" fill="{}"/>"#,
        n(width),
        n(height),
        p.page
    );
    let mut back = String::new();
    let mut lines = String::new();
    let mut front = String::new();
    let mut top = String::new();
    for (name, rect) in &layout.frames {
        frame(&mut back, name, *rect, &p, family);
    }
    for rect in &layout.groups {
        let _ = write!(
            back,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="22" fill="none" stroke="{}" stroke-width="1" stroke-dasharray="2,4"/>"#,
            n(rect.x),
            n(rect.y),
            n(rect.w),
            n(rect.h),
            p.quiet
        );
    }
    for node in &layout.cards {
        halo(&mut back, node.rect, Color::BLACK, p.shadow, 12.0, 5.0);
        card(&mut front, node, &p, family);
    }
    relations(&mut lines, &mut top, layout, &p);
    if let Some(h) = &layout.heading {
        heading(&mut top, h, &p);
    }
    for part in [back, lines, front, top] {
        out.push_str(&part);
    }
    out.push_str("</svg>");
    out
}

/// A folder: a frame a shade off the page, its name at the top left.
fn frame(out: &mut String, name: &str, r: Rect, p: &Palette, family: Family) {
    let _ = write!(
        out,
        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="20" fill="{}" stroke="{}" stroke-width="1"/>"#,
        n(r.x),
        n(r.y),
        n(r.w),
        n(r.h),
        p.frame,
        p.edge
    );
    let room = r.w - 2.0 * FRAME_PAD;
    let name = fitted(name, Font::new(LABEL, p.ink).bold(), family, room);
    text(
        out,
        (r.x + FRAME_PAD, r.y + FRAME_BAND / 2.0 + 6.0),
        &name,
        LABEL,
        true,
        p.quiet,
        &p.font,
        "start",
    );
}

/// A soft shadow: rounded rectangles that grow and fade rather than a
/// blur, which takes a renderer seconds over a large map, these a moment.
///
/// Layers of `color` around `r`, reaching `reach` past it and `drop` down,
/// together as strong as `strength` near the edge and fading out as
/// (1 − distance / reach)². Each layer is a ring around the card, which
/// covers the rest, so a renderer blends only the edge.
fn halo(out: &mut String, r: Rect, color: Color, strength: f32, reach: f32, drop: f32) {
    const STEPS: u8 = 6;
    let alpha = strength / f32::from(STEPS);
    let hole = rounded(r, 16.0);
    for step in 0..STEPS {
        let grow = reach * (1.0 - (f32::from(step) / f32::from(STEPS)).sqrt());
        let layer = Rect {
            x: r.x - grow,
            y: r.y - grow + drop,
            w: r.w + 2.0 * grow,
            h: r.h + 2.0 * grow,
        };
        let _ = write!(
            out,
            r#"<path d="{}{hole}" fill="{color}" fill-opacity="{}" fill-rule="evenodd"/>"#,
            rounded(layer, 16.0 + grow),
            n(alpha)
        );
    }
}

/// The outline of `r` with its corners rounded by `radius`, as a path.
fn rounded(r: Rect, radius: f32) -> String {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let (w, h, q) = (r.w - 2.0 * radius, r.h - 2.0 * radius, n(radius));
    format!(
        "M{} {}h{}a{q} {q} 0 0 1 {q} {q}v{}a{q} {q} 0 0 1 -{q} {q}h-{}a{q} {q} 0 0 1 -{q} -{q}v-{}a{q} {q} 0 0 1 {q} -{q}z",
        n(r.x + radius),
        n(r.y),
        n(w),
        n(h),
        n(w),
        n(h)
    )
}

fn card(out: &mut String, node: &Placed, p: &Palette, family: Family) {
    let r = node.rect;
    let stroke = format!(r#" stroke="{}" stroke-width="1""#, p.edge);
    let _ = write!(
        out,
        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="16" fill="{}"{stroke}/>"#,
        n(r.x),
        n(r.y),
        n(r.w),
        n(r.h),
        p.card
    );
    // The room is as wide as the card's inside, the picture centered.
    let room = Rect {
        x: r.x + PAD,
        y: node.picture.y,
        w: r.w - 2.0 * PAD,
        h: node.picture.h,
    };
    let _ = write!(
        out,
        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{}" stroke="{}" stroke-width="1"/>"#,
        n(room.x),
        n(room.y),
        n(room.w),
        n(room.h),
        n(PICTURE_ROUND),
        p.paper,
        p.paper_edge
    );
    let below = r.y + r.h - CAPTION;
    let room = r.w - 2.0 * TEXT_X;
    let title = fitted(&node.text, Font::new(TITLE, p.ink).bold(), family, room);
    let ty = below + 26.0;
    text(
        out,
        (r.x + TEXT_X, ty),
        &title,
        TITLE,
        true,
        p.ink,
        &p.font,
        "start",
    );
    if let Some(c) = &node.card {
        let my = ty + 21.0;
        icon(out, c.icon, (r.x + TEXT_X, my - 10.0), p.quiet);
        let facts_w = super::width(&c.facts, Font::new(SMALL, p.ink), family);
        let room = r.w - 2.0 * TEXT_X - 20.0 - facts_w - 12.0;
        let kind = fitted(&c.kind, Font::new(SMALL, p.ink), family, room);
        text(
            out,
            (r.x + TEXT_X + 20.0, my),
            &kind,
            SMALL,
            false,
            p.quiet,
            &p.font,
            "start",
        );
        text(
            out,
            (r.x + r.w - TEXT_X, my),
            &c.facts,
            SMALL,
            false,
            p.quiet,
            &p.font,
            "end",
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn text(
    out: &mut String,
    (x, y): (f32, f32),
    words: &str,
    size: f32,
    bold: bool,
    color: Color,
    font: &str,
    anchor: &str,
) {
    if words.is_empty() {
        return;
    }
    let weight = if bold { r#" font-weight="bold""# } else { "" };
    let _ = write!(
        out,
        r#"<text x="{}" y="{}" text-anchor="{anchor}" font-family="{}" font-size="{}"{weight} fill="{}">{}</text>"#,
        n(x),
        n(y),
        escape(font),
        n(size),
        color,
        escape(words)
    );
}

/// The sign of a diagram's kind, 14 units wide, at `(x, y)`.
fn icon(out: &mut String, icon: Icon, (x, y): (f32, f32), color: Color) {
    let d = match icon {
        Icon::State => "M0.5 0.5h6v5h-6z M7.5 7h6v5h-6z M3.5 5.5v4h4",
        Icon::Scenario => "M2 0v12 M12 0v12 M2 4h8 M8.5 2.5l1.5 1.5l-1.5 1.5 M12 8.5h-8",
        Icon::Activity => "M7 0.5v3 M2 3.5h10v3h-10z M7 6.5v2 M4 11.5l3-3l3 3",
        Icon::Structure => "M0.5 0.5h7v5h-7z M6.5 6.5h7v5h-7z M4 5.5v3.5h2.5",
        Icon::Map => "M7 0.5v3 M2 6v-2.5h10v2.5 M0.5 6h3v5h-3z M5.5 6h3v5h-3z M10.5 6h3v5h-3z",
        Icon::Other => "M0.5 0.5h13v11h-13z M0.5 4h13",
    };
    let _ = write!(
        out,
        r#"<path transform="translate({},{})" d="{d}" fill="none" stroke="{color}" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/>"#,
        n(x),
        n(y)
    );
}

/// A pill with `words` centered at `(x, y)`, over the lines.
fn chip(out: &mut String, (x, y): (f32, f32), words: &str, p: &Palette) {
    let pill = chip_rect(words, (x, y));
    let _ = write!(
        out,
        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{}" stroke="{}" stroke-width="1"/>"#,
        n(pill.x),
        n(pill.y),
        n(pill.w),
        n(pill.h),
        n(pill.h / 2.0),
        p.page,
        p.edge
    );
    text(
        out,
        (x, y + 4.0),
        words,
        11.0,
        false,
        p.quiet,
        &p.font,
        "middle",
    );
}

/// The map's title and what it holds.
fn heading(out: &mut String, h: &Heading, p: &Palette) {
    let left = super::MARGIN;
    let top = super::MARGIN;
    text(
        out,
        (left, top + 22.0),
        &h.title,
        22.0,
        true,
        p.ink,
        &p.font,
        "start",
    );
    text(
        out,
        (left, top + 46.0),
        &h.summary,
        12.5,
        false,
        p.quiet,
        &p.font,
        "start",
    );
}

/// The arrows between cards: their lines under the cards, their pills
/// over them, a head where a line arrives at a card.
fn relations(lines: &mut String, top: &mut String, layout: &Layout, p: &Palette) {
    for arrow in &layout.arrows {
        let (color, dash, width) = match arrow.kinds[0] {
            RelationKind::Plays => (p.accent, "", 1.6),
            RelationKind::Opens => (p.accent, r#" stroke-dasharray="5,4""#, 1.4),
            RelationKind::Includes => (p.quiet, r#" stroke-dasharray="1.5,3.5""#, 1.4),
        };
        for (line, head) in &arrow.lines {
            let _ = write!(
                lines,
                r#"<path d="{}" fill="none" stroke="{color}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"{dash}/>"#,
                crate::curve::path(line, ROUND),
                n(width)
            );
            let [.., before, end] = line[..] else {
                continue;
            };
            if !head {
                continue;
            }
            // The head along the last run.
            let (dx, dy) = (end.0 - before.0, end.1 - before.1);
            let len = dx.hypot(dy).max(f32::EPSILON);
            let (ux, uy) = (dx / len, dy / len);
            let back = (end.0 - ux * 9.0, end.1 - uy * 9.0);
            let _ = write!(
                lines,
                r#"<path d="M{} {} L{} {} L{} {} Z" fill="{color}"/>"#,
                n(end.0),
                n(end.1),
                n(back.0 - uy * 4.5),
                n(back.1 + ux * 4.5),
                n(back.0 + uy * 4.5),
                n(back.1 - ux * 4.5)
            );
        }
        chip(top, arrow.chip, &arrow.words, p);
    }
}
