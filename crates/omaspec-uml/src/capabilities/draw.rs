//! Lays a capability map out and paints it. The areas stand side by side
//! as columns, in rows of a few where there are many, each as high as the
//! highest in its row. Inside an area its capabilities stand one under
//! the other; deeper ones stand side by side as narrow as they read, in
//! as many columns as fit. A capability with a value is filled with its
//! step's color, and dots under its name count the step, so the map reads
//! without the colors too; one without is drawn with a dashed edge. The
//! legend under the map names the steps.
//!
//! The colors of the steps run from near the page to the theme's accent,
//! where the file gives none of its own (`levels Risk : low #2F9E44, …`).
//! The words of each capability are tied to its line, a step's name in
//! the legend to where the scale's line writes it.

use super::{Capability, Diagram, Levels};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::Color;
use crate::map::Map;
use crate::own_skin::Skin;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const MARGIN: f32 = 10.0;
/// Inside a tile, around its words.
const PAD: f32 = 8.0;
/// Between two tiles in one area.
const GAP: f32 = 8.0;
/// Between two areas.
const APART: f32 = 16.0;
/// The widest the areas of one row stand together.
const WIDE: f32 = 1400.0;
/// The narrowest a capability under an area stands, and one deeper.
const NARROWEST: [f32; 2] = [150.0, 90.0];
/// A dot that counts a step: its radius, and from one to the next.
const DOT: f32 = 2.5;
const DOTS: f32 = 7.0;
/// The most steps counted in dots; a longer scale is read by its colors.
const MOST_DOTS: usize = 7;
/// What the legend calls a capability without a value.
const UNRATED: &str = "not rated";

pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    draw_mapped(diagram).map(|(svg, _)| svg)
}

/// [`draw`], and the map: the words of each capability tied to its line.
pub fn draw_mapped(diagram: &Diagram) -> Result<(String, Map), Declined> {
    let skin = Skin::from_params(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    canvas.sketch(skin.sketch);
    canvas.shadows(skin.shadow);
    let family = canvas.family();
    let notes = Annotations::new(
        &diagram.frame,
        [skin.title, skin.header, skin.footer, skin.caption],
        family,
    )?;
    let look = Look::of(&skin, diagram.levels.as_ref());
    let mut bottom = notes.top();
    if !diagram.areas.is_empty() {
        bottom = paint(&mut canvas, diagram, &look, family, notes.top())?;
    }
    if let Some(levels) = &diagram.levels {
        legend(&mut canvas, diagram, levels, &look, family, bottom)?;
    }
    notes.draw(&mut canvas);
    let map = canvas.place(Map::default(), MARGIN);
    Ok((canvas.finish(MARGIN, skin.background), map))
}

/// The colors and fonts of the map.
struct Look<'a> {
    skin: &'a Skin,
    paper: Color,
    /// Each step's fill, and the edge drawn around it.
    steps: Vec<(Color, Color)>,
    /// The fill of an area, and of the capabilities deeper in it that
    /// hold others: every other depth a little toward the ink.
    ground: [Color; 2],
    /// The edge of what has no value, which reads on the page.
    edge: Color,
}

impl<'a> Look<'a> {
    fn of(skin: &'a Skin, levels: Option<&Levels>) -> Look<'a> {
        let paper = skin.paper();
        let ink = skin.label.color;
        let edge = paper.mix(ink, 0.35).legible_on(paper, ink, 3.0);
        let count = levels.map_or(0, |l| l.steps.len());
        let steps = levels
            .map(|l| {
                l.steps
                    .iter()
                    .enumerate()
                    .map(|(i, step)| {
                        let fill = step.color.unwrap_or_else(|| {
                            let share = if count < 2 {
                                1.0
                            } else {
                                0.2 + 0.8 * i as f32 / (count - 1) as f32
                            };
                            paper.mix(skin.accent, share)
                        });
                        (fill, fill.mix(ink, 0.25).legible_on(paper, ink, 3.0))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Look {
            skin,
            paper,
            steps,
            ground: [paper.mix(ink, 0.05), paper],
            edge,
        }
    }

    /// The words of a capability `depth` levels deep: an area's larger.
    fn font(&self, depth: usize, fill: Color) -> Font {
        let label = self.skin.label;
        let font = Font {
            color: self.skin.ink_on(fill),
            ..label
        };
        match depth {
            1 => Font {
                size: label.size + 1.0,
                ..font
            }
            .bold(),
            2 => font.bold(),
            _ => font,
        }
    }

    /// What a capability is filled with, and its edge: its step's colors;
    /// for one that holds others the ground of its depth, its step's color
    /// under its name alone ([`Self::head`]), so what it holds reads on
    /// it; a dashed edge for one with neither a value nor others.
    fn fill(&self, cap: &Capability, depth: usize) -> (Color, Stroke) {
        let holds = depth == 1 || !cap.children.is_empty();
        let ground = self.ground[(depth + 1) % 2];
        match (cap.value.and_then(|v| self.steps.get(v)), holds) {
            (Some(&(fill, edge)), false) => (fill, Stroke::new(edge, 1.0)),
            (Some(&(_, edge)), true) => (ground, Stroke::new(edge, 1.0)),
            (None, true) => (ground, Stroke::new(self.edge, 1.0)),
            (None, false) => (self.paper, Stroke::new(self.edge, 1.0).dashed(4.0, 3.0)),
        }
    }

    /// The color under a capability's name: its step's where it has a
    /// value, its fill where not.
    fn head(&self, cap: &Capability, depth: usize) -> Color {
        match cap.value.and_then(|v| self.steps.get(v)) {
            Some(&(fill, _)) => fill,
            None => self.fill(cap, depth).0,
        }
    }
}

/// A capability measured: where it stands from its parent's corner, its
/// words, and where its own capabilities stand.
struct Tile<'a> {
    cap: &'a Capability,
    depth: usize,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    words: Block,
    /// The height of its name with the dots.
    head: f32,
    inner: Vec<Tile<'a>>,
}

/// Paints the areas below `top`; where they end.
fn paint(
    canvas: &mut Canvas,
    diagram: &Diagram,
    look: &Look,
    family: Family,
    top: f32,
) -> Result<f32, Declined> {
    let column = (look.skin.label.size * 18.0).max(220.0);
    let count = diagram.areas.len();
    let fit = (((WIDE + APART) / (column + APART)).floor() as usize).max(1);
    let rows = count.div_ceil(fit);
    let per_row = count.div_ceil(rows);
    let mut y = top;
    for row in diagram.areas.chunks(per_row) {
        let mut tiles = row
            .iter()
            .map(|cap| measure(cap, 1, column, look, family))
            .collect::<Result<Vec<_>, _>>()?;
        let high = tiles.iter().map(|t| t.h).fold(0.0, f32::max);
        for (i, tile) in tiles.iter_mut().enumerate() {
            tile.x = i as f32 * (column + APART);
            tile.y = y;
            tile.h = high;
            draw_tile(canvas, tile, (0.0, 0.0), look);
        }
        y += high + APART;
    }
    Ok(y - APART)
}

/// `cap`, `depth` levels deep, measured `width` wide.
fn measure<'a>(
    cap: &'a Capability,
    depth: usize,
    width: f32,
    look: &Look,
    family: Family,
) -> Result<Tile<'a>, Declined> {
    let font = look.font(depth, look.head(cap, depth));
    let words = wrap(&cap.name, font, family, width - 2.0 * PAD, cap.line)?.written_on(cap.line);
    let dots = if dotted(cap, look) { DOTS + 2.0 } else { 0.0 };
    let head = words.height.max(font.size * 1.2) + dots + 2.0 * PAD;
    if cap.children.is_empty() {
        return Ok(Tile {
            cap,
            depth,
            x: 0.0,
            y: 0.0,
            w: width,
            h: head,
            words,
            head,
            inner: Vec::new(),
        });
    }
    let room = width - 2.0 * PAD;
    let narrowest = NARROWEST[usize::from(depth > 1)];
    let columns =
        (((room + GAP) / (narrowest + GAP)).floor() as usize).clamp(1, cap.children.len());
    let each = (room - GAP * (columns - 1) as f32) / columns as f32;
    let mut inner = cap
        .children
        .iter()
        .map(|c| measure(c, depth + 1, each, look, family))
        .collect::<Result<Vec<_>, _>>()?;
    // Under a head in its step's color, room before what it holds.
    let mut y = if cap.value.is_some() {
        head + 2.0
    } else {
        head - PAD / 2.0
    };
    for row in inner.chunks_mut(columns) {
        let high = row.iter().map(|t| t.h).fold(0.0, f32::max);
        for (i, tile) in row.iter_mut().enumerate() {
            tile.x = PAD + i as f32 * (each + GAP);
            tile.y = y;
            tile.h = high;
        }
        y += high + GAP;
    }
    Ok(Tile {
        cap,
        depth,
        x: 0.0,
        y: 0.0,
        w: width,
        h: y - GAP + PAD,
        words,
        head,
        inner,
    })
}

/// Whether `cap` counts its step in dots under its name.
fn dotted(cap: &Capability, look: &Look) -> bool {
    cap.value.is_some() && look.steps.len() <= MOST_DOTS
}

/// Paints `tile`, whose parent's corner is at `at`, and what it holds.
fn draw_tile(canvas: &mut Canvas, tile: &Tile, at: (f32, f32), look: &Look) {
    let (x, y) = (at.0 + tile.x, at.1 + tile.y);
    let (fill, edge) = look.fill(tile.cap, tile.depth);
    let round = look.skin.round.max(2.0) / tile.depth as f32;
    canvas.named(&format!("capability {}", tile.cap.line), |c| {
        c.rect(
            Layer::Bars,
            (x, y, tile.w, tile.h),
            round,
            Some(fill),
            Some(edge),
        );
        // One that holds others has its step's color under its name alone,
        // square below, the edge drawn again over it.
        let head = look.head(tile.cap, tile.depth);
        if head != fill {
            let h = tile.head - PAD / 2.0;
            c.rect(Layer::Bars, (x, y, tile.w, h), round, Some(head), None);
            c.rect(
                Layer::Bars,
                (x, y + h / 2.0, tile.w, h / 2.0),
                0.0,
                Some(head),
                None,
            );
            c.rect(Layer::Bars, (x, y, tile.w, tile.h), round, None, Some(edge));
        }
        let left = if tile.inner.is_empty() || tile.depth > 1 {
            (x + PAD, Align::Left)
        } else {
            (x + tile.w / 2.0, Align::Center)
        };
        c.text(Layer::Front, &tile.words, (left.0, y + PAD), left.1);
        if let Some(step) = tile.cap.value.filter(|_| dotted(tile.cap, look)) {
            let ink = look.skin.ink_on(head);
            let dots_y = y + PAD + tile.words.height + 2.0 + DOT;
            let count = look.steps.len();
            let first = match left.1 {
                Align::Center => left.0 - (count as f32 - 1.0) * DOTS / 2.0,
                _ => left.0 + DOT,
            };
            // Seen as the words above them are, on the tile.
            c.begin_group(Layer::Front, crate::ICON);
            // A step reached is a dot filled, one not yet a ring.
            for i in 0..count {
                let at = (first + i as f32 * DOTS, dots_y);
                if i <= step {
                    c.ellipse(Layer::Front, at, (DOT + 0.4, DOT + 0.4), Some(ink), None);
                } else {
                    c.ellipse(
                        Layer::Front,
                        at,
                        (DOT, DOT),
                        None,
                        Some(Stroke::new(ink, 0.8)),
                    );
                }
            }
            c.end_group(Layer::Front);
        }
    });
    for inner in &tile.inner {
        draw_tile(canvas, inner, (x, y), look);
    }
}

/// The legend under the map, from `top`: the scale's name, each step's
/// color and dots with its name, and the edge of what has no value where
/// a capability has none.
fn legend(
    canvas: &mut Canvas,
    diagram: &Diagram,
    levels: &Levels,
    look: &Look,
    family: Family,
    top: f32,
) -> Result<(), Declined> {
    let label = look.skin.label;
    let y = top + 2.0 * GAP;
    let name = wrap(&levels.name, label.bold(), family, 400.0, levels.line)?
        .written_at(levels.line, Some(levels.col));
    canvas.text(Layer::Front, &name, (0.0, y), Align::Left);
    let swatch = label.size;
    let mut x = name.width + 2.0 * GAP;
    let mut unrated = Vec::new();
    for area in &diagram.areas {
        unrated_leaves(area, &mut unrated);
    }
    for (i, step) in levels.steps.iter().enumerate() {
        let (fill, edge) = look.steps[i];
        canvas.rect(
            Layer::Bars,
            (x, y, swatch * 1.6, swatch),
            2.0,
            Some(fill),
            Some(Stroke::new(edge, 1.0)),
        );
        x += swatch * 1.6 + GAP / 2.0;
        let words = wrap(&step.name, label, family, 300.0, levels.line)?
            .written_at(levels.line, Some(step.col));
        canvas.text(Layer::Front, &words, (x, y), Align::Left);
        x += words.width + 2.0 * GAP;
    }
    if !unrated.is_empty() {
        let edge = Stroke::new(look.edge, 1.0).dashed(4.0, 3.0);
        canvas.rect(
            Layer::Bars,
            (x, y, swatch * 1.6, swatch),
            2.0,
            Some(look.paper),
            Some(edge),
        );
        x += swatch * 1.6 + GAP / 2.0;
        let words = wrap(UNRATED, label.italic(), family, 300.0, levels.line)?;
        canvas.text(Layer::Front, &words, (x, y), Align::Left);
    }
    Ok(())
}

/// The capabilities without a value that hold no others, under `cap`.
fn unrated_leaves<'a>(cap: &'a Capability, out: &mut Vec<&'a Capability>) {
    if cap.children.is_empty() {
        if cap.value.is_none() {
            out.push(cap);
        }
        return;
    }
    for child in &cap.children {
        unrated_leaves(child, out);
    }
}

/// `words` in `font`, broken into lines no wider than `width`.
fn wrap(
    words: &str,
    font: Font,
    family: Family,
    width: f32,
    line: usize,
) -> Result<Block, Declined> {
    text::wrapped(words, font, family, width.max(20.0)).map_err(|reason| Declined::at(line, reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOP: &str = "@startcapabilities\ntitle What a bookshop can do\nlevels Maturity : none, basic, managed, leading\n* Customers\n** Marketing : basic\n** Sales\n*** Online shop : leading\n*** Stores : managed\n* Operations\n** Buying : managed\n** Stock\n@endcapabilities\n";

    fn drawn(source: &str) -> (String, Map) {
        crate::render_mapped(source, None).unwrap()
    }

    fn at(map: &Map, line: usize) -> crate::graph::Rect {
        map.tied
            .iter()
            .find(|t| t.line == line)
            .unwrap_or_else(|| panic!("line {line} is tied: {:?}", map.tied))
            .rect
    }

    #[test]
    fn areas_stand_side_by_side_and_hold_their_capabilities() {
        let (svg, map) = drawn(SHOP);
        for words in ["Customers", "Online shop", "Maturity", "leading", UNRATED] {
            assert!(svg.contains(words), "{words}: {svg}");
        }
        // The two areas side by side, at one height.
        assert!((at(&map, 3).y - at(&map, 8).y).abs() < 0.5);
        assert!(at(&map, 8).x > at(&map, 3).x + 100.0);
        // Inside an area one under the other; deeper side by side.
        assert!(at(&map, 4).y > at(&map, 3).y && at(&map, 5).y > at(&map, 4).y);
        assert!((at(&map, 6).y - at(&map, 7).y).abs() < 0.5);
        assert!(at(&map, 7).x > at(&map, 6).x);
        // The legend's steps are the scale's words, where they stand.
        let legend: Vec<_> = map.tied.iter().filter(|t| t.line == 2).collect();
        assert_eq!(legend.len(), 5, "the name and four steps");
        assert!(
            legend
                .iter()
                .any(|t| t.col == Some("levels Maturity : ".len()))
        );
        assert!(legend.iter().all(|t| t.rect.y > at(&map, 10).y));
        // Each of the four capabilities with a value counts its step in
        // four dots, one for each step of the scale.
        assert_eq!(
            svg.matches("<ellipse").count() + svg.matches("<circle").count(),
            4 * 4
        );
    }

    #[test]
    fn the_steps_run_from_the_page_to_the_accent_or_take_their_own_colors() {
        let themed = SHOP.replace(
            "title What a bookshop can do\n",
            "title What a bookshop can do\nskinparam RectangleBorderColor #2f6fdb\n",
        );
        let svg = drawn(&themed).0.to_ascii_uppercase();
        assert!(
            svg.contains("#2F6FDB"),
            "the last step is the accent: {svg}"
        );
        let own = SHOP.replacen("leading", "leading #2F9E44", 1);
        let svg = drawn(&own).0.to_ascii_uppercase();
        assert!(svg.contains("#2F9E44"), "{svg}");
    }

    #[test]
    fn many_areas_stand_in_rows_and_a_map_without_a_scale_draws() {
        let many: String = (1..=9)
            .map(|n| format!("* Area {n}\n** Thing {n}\n"))
            .collect();
        let (_, map) = drawn(&format!("@startcapabilities\n{many}@endcapabilities\n"));
        let tops: std::collections::BTreeSet<i32> = (0..9)
            .map(|n| at(&map, 1 + 2 * n).y.round() as i32)
            .collect();
        assert_eq!(tops.len(), 2, "two rows: {tops:?}");
        let empty = drawn("@startcapabilities\n@endcapabilities\n").0;
        assert!(empty.starts_with("<svg"), "{empty}");
    }
}
