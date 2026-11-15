//! The diagram as pixels for the screen, in two layers. The overview shows
//! the whole diagram, at most [`OVERVIEW_PIXELS`] large; that is sharp
//! unless a large diagram is zoomed in. Then the detail shows the visible
//! part at full resolution on top of it, with a margin so short pans need
//! nothing new. Rasterizing runs in the background; until it is done, the
//! layers on screen are drawn scaled. New pixels are shown only once they
//! are on the GPU: iced uploads large images in the background and draws
//! nothing meanwhile, so the paper would flash empty after every save.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use iced::widget::image;
use iced::{Color, Point, Rectangle, Size, Task, Vector};
use iced_runtime::image::Allocation;
use omaspec_render::export::{self, tiny_skia::IntRect, usvg};
use omaspec_render::pick;

use super::Message;
use crate::worker::blocking;

/// Largest overview, in pixels.
const OVERVIEW_PIXELS: f32 = 16_000_000.0;
/// The detail reaches this share of the visible size beyond each edge.
const MARGIN: f32 = 0.25;

/// Pixels to draw: `part` of the diagram drawn at `scale` pixels per unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub scale: f32,
    pub part: IntRect,
}

impl Plan {
    /// The whole diagram, as sharp as `scale` asks but at most
    /// [`OVERVIEW_PIXELS`].
    pub fn overview(diagram: Size, scale: f32) -> Option<Self> {
        let scale = scale.min(overview_limit(diagram));
        let part = pixels(Rectangle::with_size(diagram), diagram, scale)?;
        Some(Self { scale, part })
    }

    /// The `visible` part of the diagram at `scale`, when the overview is
    /// not sharp there. Keeps `current` while it still covers what is
    /// visible.
    pub fn detail(
        diagram: Size,
        scale: f32,
        visible: Rectangle,
        current: Option<Self>,
    ) -> Option<Self> {
        if scale <= overview_limit(diagram) {
            return None;
        }
        let seen = pixels(visible, diagram, scale)?;
        if let Some(current) = current
            && current.scale == scale
            && current.part.contains(&seen)
        {
            return Some(current);
        }
        let margin = Vector::new(visible.width * MARGIN, visible.height * MARGIN);
        let wide = Rectangle::new(
            visible.position() - margin,
            visible.size() + Size::new(margin.x, margin.y) * 2.0,
        );
        Some(Self {
            scale,
            part: pixels(wide, diagram, scale)?,
        })
    }

    /// The area of the diagram the pixels show, in diagram units.
    pub fn region(self) -> Rectangle {
        let s = self.scale;
        Rectangle::new(
            Point::new(self.part.x() as f32 / s, self.part.y() as f32 / s),
            Size::new(self.part.width() as f32 / s, self.part.height() as f32 / s),
        )
    }
}

/// The largest scale at which the whole diagram fits the overview.
fn overview_limit(diagram: Size) -> f32 {
    (OVERVIEW_PIXELS / (diagram.width * diagram.height).max(1.0)).sqrt()
}

/// The pixels showing `area` of the diagram at `scale`, rounded outwards;
/// `None` if `area` misses the diagram.
fn pixels(area: Rectangle, diagram: Size, scale: f32) -> Option<IntRect> {
    let area = area.intersection(&Rectangle::with_size(diagram))?;
    IntRect::from_ltrb(
        (area.x * scale).floor() as i32,
        (area.y * scale).floor() as i32,
        ((area.x + area.width) * scale).ceil() as i32,
        ((area.y + area.height) * scale).ceil() as i32,
    )
}

/// Pixels ready for the screen.
#[derive(Debug, Clone)]
pub struct Raster {
    pub handle: image::Handle,
    /// The area of the diagram they show, in diagram units.
    pub region: Rectangle,
    /// The color of the top-left pixel: for the whole diagram, the paper it
    /// lies on, white or what the diagram asks for.
    pub paper: Color,
    /// Keeps the pixels on the GPU while they are shown.
    _allocation: Option<Allocation>,
    /// A sheet's parts: where each of them lies on it ([`sheet`]).
    pub parts: Option<Arc<Cutouts>>,
}

impl Raster {
    /// A white picture of `width` by `height` pixels, of a diagram as
    /// large in its own units.
    #[cfg(test)]
    pub fn blank(width: u32, height: u32) -> Raster {
        Raster {
            handle: image::Handle::from_rgba(
                width,
                height,
                vec![255u8; width as usize * height as usize * 4],
            ),
            region: Rectangle::with_size(Size::new(width as f32, height as f32)),
            paper: Color::WHITE,
            _allocation: None,
            parts: None,
        }
    }

    /// A picture of `width` by `height` pixels, each `paint`ed by its
    /// column and row, of a diagram as large in its own units.
    #[cfg(test)]
    pub fn painted(width: u32, height: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Raster {
        let pixels = (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| paint(x, y))
            .collect::<Vec<u8>>();
        Raster {
            handle: image::Handle::from_rgba(width, height, pixels),
            region: Rectangle::with_size(Size::new(width as f32, height as f32)),
            paper: Color::WHITE,
            _allocation: None,
            parts: None,
        }
    }

    /// How many pixels it has, across and down; `None` unless they are
    /// held in memory.
    pub fn pixels(&self) -> Option<(u32, u32)> {
        match &self.handle {
            image::Handle::Rgba { width, height, .. } => Some((*width, *height)),
            _ => None,
        }
    }
}

/// A copy is made while both of its sides have this many pixels.
const SMALLEST: usize = 48;
/// At most this many copies: the last shows a picture at a 64th.
const HALVES: usize = 6;

/// Copies of `raster` for showing it small: each half as wide and as high
/// as the one before. The GPU fills a pixel from the four texels around
/// one point, so a picture drawn at less than half its size loses its
/// thin lines, a few with every step of a zoom; a copy near the size it
/// is shown at keeps them. None for pixels that are not held in memory.
pub fn halves(raster: &Raster) -> Vec<Raster> {
    let image::Handle::Rgba {
        width,
        height,
        pixels,
        ..
    } = &raster.handle
    else {
        return Vec::new();
    };
    let (mut w, mut h) = (*width as usize, *height as usize);
    let mut out: Vec<Raster> = Vec::new();
    let mut last: Option<Vec<u8>> = None;
    while out.len() < HALVES && w >= 2 * SMALLEST && h >= 2 * SMALLEST {
        let from: &[u8] = last.as_deref().unwrap_or(pixels);
        if from.len() < w * h * 4 {
            break;
        }
        let (nw, nh) = (w.div_ceil(2), h.div_ceil(2));
        let mut half = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            let (top, bottom) = (2 * y * w, (2 * y + 1).min(h - 1) * w);
            for x in 0..nw {
                let (left, right) = (2 * x, (2 * x + 1).min(w - 1));
                let texels = [top + left, top + right, bottom + left, bottom + right];
                let at = (y * nw + x) * 4;
                for channel in 0..4 {
                    let sum: u32 = texels
                        .iter()
                        .map(|&texel| u32::from(from[texel * 4 + channel]))
                        .sum();
                    half[at + channel] = ((sum + 2) / 4) as u8;
                }
            }
        }
        out.push(Raster {
            handle: image::Handle::from_rgba(nw as u32, nh as u32, half.clone()),
            region: raster.region,
            paper: raster.paper,
            _allocation: None,
            parts: None,
        });
        last = Some(half);
        (w, h) = (nw, nh);
    }
    out
}

/// Makes the [`halves`] of `raster` in the background and uploads them;
/// `done` makes the message for each, with its place among them.
pub fn ask_halves(
    raster: Raster,
    done: impl Fn(usize, Option<Raster>) -> Message + Clone + Send + 'static,
) -> Task<Message> {
    Task::perform(blocking(move || halves(&raster)), Option::unwrap_or_default).then(
        move |copies: Vec<Raster>| {
            let done = done.clone();
            Task::batch(copies.into_iter().enumerate().map(move |(at, copy)| {
                let done = done.clone();
                upload(copy).map(move |copy| done(at, copy))
            }))
        },
    )
}

pub fn render(tree: &usvg::Tree, plan: Plan) -> Option<Raster> {
    let pixmap = export::rasterize_part(tree, plan.scale, plan.part).ok()?;
    // Opaque, on white.
    let paper = pixmap.pixel(0, 0).map_or(Color::WHITE, |p| {
        Color::from_rgb8(p.red(), p.green(), p.blue())
    });
    Some(Raster {
        paper,
        handle: image::Handle::from_rgba(pixmap.width(), pixmap.height(), pixmap.take()),
        region: plan.region(),
        _allocation: None,
        parts: None,
    })
}

/// Where each part of a picture lies on its sheet ([`sheet`]), by the
/// part's name (docs/adr/0029), and the colors its boxes are filled with.
#[derive(Debug, Default)]
pub struct Cutouts {
    pub parts: HashMap<String, Cutout>,
    /// A patch of each color the parts' shapes are filled with, by the
    /// color: what a glide fills a box's room with ([`super::sprite::draw`]),
    /// on the GPU with the sheet.
    swatches: HashMap<[u8; 3], Rectangle>,
}

/// How many pixels a side a patch of [`Cutouts::swatches`] is, and between
/// two of them: a glide takes its middle half, which no edge blurs.
const SWATCH: u32 = 8;
const SWATCH_GAP: u32 = 2;

/// A color as the patches of a sheet are told apart.
fn swatch_key(color: [f32; 3]) -> [u8; 3] {
    color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// One part on a sheet.
#[derive(Debug, Clone)]
pub struct Cutout {
    /// Where its picture lies on the sheet, in the sheet's units.
    pub sheet: Rectangle,
    /// Where it is drawn in the diagram, in diagram units: as large as
    /// `sheet`.
    pub at: Rectangle,
    /// Its own outlines, which say how its box bends and how it is filled.
    pub outlines: pick::Fills,
}

impl Raster {
    /// Whether the sheet has a part called `name`.
    pub fn has_part(&self, name: &str) -> bool {
        self.parts
            .as_ref()
            .is_some_and(|parts| parts.parts.contains_key(name))
    }

    /// How the box at `rect` of the part called `name` is filled and how
    /// its outline bends, read from that part's own outlines alone.
    pub fn outline(&self, name: &str, rect: Rectangle) -> (Option<[f32; 3]>, Option<pick::Bend>) {
        let Some(cutout) = self.parts.as_ref().and_then(|parts| parts.parts.get(name)) else {
            return (None, None);
        };
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        (
            cutout.outlines.of(x, y, w, h),
            cutout.outlines.bend(x, y, w, h),
        )
    }

    /// Where on a sheet its patch of `color` lies, in the units of its
    /// region: a box's own fill, read from its part ([`Self::outline`]).
    pub fn swatch(&self, color: [f32; 3]) -> Option<Rectangle> {
        self.parts
            .as_ref()?
            .swatches
            .get(&swatch_key(color))
            .copied()
    }

    /// The picture of the part `name` of a sheet, as a picture of its own:
    /// the sheet, placed so that the part lies at its place in the diagram.
    pub fn part(&self, name: &str) -> Option<(Raster, Rectangle)> {
        let cutout = self.parts.as_ref()?.parts.get(name)?;
        // The sheet placed so that the part's pixels lie on its place in
        // the diagram, at the scale the part was drawn at.
        let k = cutout.at.width / cutout.sheet.width.max(f32::EPSILON);
        Some((
            Raster {
                region: Rectangle::new(
                    Point::new(
                        cutout.at.x + (self.region.x - cutout.sheet.x) * k,
                        cutout.at.y + (self.region.y - cutout.sheet.y) * k,
                    ),
                    Size::new(self.region.width * k, self.region.height * k),
                ),
                ..self.clone()
            },
            cutout.at,
        ))
    }
}

/// How far past a part's own pixels a glide may take it, in diagram
/// units: by its box's rectangle, which reaches this far past the box
/// (`morph::RIM`), a little more than its stroke.
const SHEET_REACH: f32 = 2.0;

/// Space between two parts on a sheet, in pixels at `scale`, so the blur
/// of one does not reach the next and a glide that takes a part a little
/// wider than its pixels takes nothing of another, nor of the ground.
fn sheet_gap(scale: f32) -> u32 {
    (SHEET_REACH * scale).ceil() as u32 + 3
}

/// What a [`sheet`] is drawn from: the ground of a picture that marks its
/// parts, and the picture itself, whose parts are drawn each on its own
/// (docs/adr/0029).
#[derive(Clone)]
pub struct Sheet {
    pub ground: Arc<usvg::Tree>,
    pub full: Arc<usvg::Tree>,
}

impl Sheet {
    /// The sheet of `drawn`; none when its parts are not marked, or its
    /// ground could not be made, and it fades rather than glides.
    pub fn of(drawn: &crate::worker::Drawn) -> Option<Self> {
        Some(Self {
            ground: Arc::clone(drawn.bare.as_ref().filter(|_| drawn.parts)?),
            full: Arc::clone(&drawn.tree),
        })
    }

    /// The sheet at `plan`.
    pub fn draw(&self, plan: Plan) -> Option<Raster> {
        sheet(&self.ground, &self.full, plan)
    }
}

/// `ground` at `plan`, and below it each part of `full` on its own, on one
/// sheet: one picture to rasterize and to upload for a glide that takes
/// its parts whole (docs/adr/0029). The parts lie in rows as wide as the
/// ground, each part's own pixels and nothing else, on clear pixels.
pub fn sheet(ground: &usvg::Tree, full: &usvg::Tree, plan: Plan) -> Option<Raster> {
    use export::tiny_skia::{Pixmap, PixmapPaint, Transform};
    let scale = plan.scale;
    let (ground_w, ground_h) = (plan.part.width(), plan.part.height());
    let mut parts: Vec<Part<'_>> = Vec::new();
    gather_parts(full.root(), 1.0, &mut parts);
    // Each part's box in the diagram, its shadow too.
    let bounds: Vec<(usvg::NonZeroRect, &Part<'_>)> = parts
        .iter()
        .filter_map(|part| {
            let bounds = part
                .nodes
                .iter()
                .filter_map(|(node, _)| node.abs_layer_bounding_box())
                .reduce(|a, b| {
                    usvg::NonZeroRect::from_ltrb(
                        a.left().min(b.left()),
                        a.top().min(b.top()),
                        a.right().max(b.right()),
                        a.bottom().max(b.bottom()),
                    )
                    .unwrap_or(a)
                })?;
            Some((bounds, part))
        })
        .collect();
    let most = 4 * OVERVIEW_PIXELS as u64;
    let mut layout = shelves(&bounds, scale, ground_w, ground_h);
    // Too large at the ground's scale (packages in packages each as large as
    // all they hold): the parts drawn smaller, to be drawn larger as they
    // glide, rather than not at all.
    if layout.area() > most {
        let room = most.saturating_sub(u64::from(ground_w) * u64::from(ground_h)) as f32;
        let parts = (layout.area() - u64::from(ground_w) * u64::from(ground_h)).max(1) as f32;
        let smaller = scale * (room / parts).sqrt() * 0.9;
        layout = shelves(&bounds, smaller, ground_w, ground_h);
        if layout.area() > most {
            return None;
        }
    }
    let Shelves {
        part_scale,
        boxes,
        slots,
        width,
        height,
    } = layout;
    let outlines: Vec<pick::Fills> = boxes
        .iter()
        .map(|(_, part)| {
            pick::fills_of(part.nodes.iter().filter_map(|(node, _)| match node {
                usvg::Node::Group(group) => Some(group.as_ref()),
                _ => None,
            }))
        })
        .collect();
    // Below the parts, a patch of each color they are filled with, in
    // rows as wide as the sheet.
    let mut colors: Vec<[u8; 3]> = outlines
        .iter()
        .flat_map(pick::Fills::colors)
        .map(swatch_key)
        .collect();
    colors.sort_unstable();
    colors.dedup();
    let step = SWATCH + SWATCH_GAP;
    let across = ((width.saturating_sub(SWATCH_GAP)) / step).max(1);
    let rows = (colors.len() as u32).div_ceil(across);
    let patches_at = height + SWATCH_GAP;
    let height = height + rows * step;
    let base = export::rasterize_part(ground, scale, plan.part).ok()?;
    let mut pixmap = Pixmap::new(width.max(step + SWATCH_GAP), height)?;
    pixmap.draw_pixmap(
        0,
        0,
        base.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    let region = plan.region();
    let mut swatches = HashMap::with_capacity(colors.len());
    for (index, color) in colors.into_iter().enumerate() {
        let index = index as u32;
        let (x, y) = (
            SWATCH_GAP + (index % across) * step,
            patches_at + (index / across) * step,
        );
        let mut paint = export::tiny_skia::Paint::default();
        paint.set_color_rgba8(color[0], color[1], color[2], 255);
        if let Some(rect) =
            export::tiny_skia::Rect::from_xywh(x as f32, y as f32, SWATCH as f32, SWATCH as f32)
        {
            pixmap.fill_rect(rect, &paint, Transform::identity(), None);
        }
        swatches.insert(
            color,
            Rectangle::new(
                Point::new(region.x + x as f32 / scale, region.y + y as f32 / scale),
                Size::new(SWATCH as f32 / scale, SWATCH as f32 / scale),
            ),
        );
    }
    let mut cutouts = HashMap::with_capacity(boxes.len());
    for (((pixels, part), (sx, sy)), outlines) in boxes.iter().zip(&slots).zip(outlines) {
        // Diagram units onto the part's slot of the sheet.
        let onto = Transform::from_translate(*sx as f32, *sy as f32)
            .pre_translate(-pixels.x() as f32, -pixels.y() as f32)
            .pre_scale(part_scale, part_scale);
        for (node, opacity) in &part.nodes {
            // resvg draws a node from where its box begins: start there.
            let Some(bounds) = node.abs_layer_bounding_box() else {
                continue;
            };
            let at = onto
                .pre_concat(node.abs_transform())
                .pre_translate(bounds.x(), bounds.y());
            if *opacity >= 0.999 {
                let _ = export::render_node(node, at, &mut pixmap.as_mut());
                continue;
            }
            // Seen through a group around it (a shadow's): drawn alone on
            // a picture of the part's size, then laid on at that strength.
            let Some(mut alone) = Pixmap::new(pixels.width(), pixels.height()) else {
                continue;
            };
            let local = Transform::from_translate(-pixels.x() as f32, -pixels.y() as f32)
                .pre_scale(part_scale, part_scale)
                .pre_concat(node.abs_transform())
                .pre_translate(bounds.x(), bounds.y());
            let _ = export::render_node(node, local, &mut alone.as_mut());
            pixmap.draw_pixmap(
                *sx as i32,
                *sy as i32,
                alone.as_ref(),
                &PixmapPaint {
                    opacity: *opacity,
                    ..PixmapPaint::default()
                },
                Transform::identity(),
                None,
            );
        }
        let (w, h) = (pixels.width() as f32, pixels.height() as f32);
        cutouts.insert(
            part.name.clone(),
            Cutout {
                sheet: Rectangle::new(
                    Point::new(region.x + *sx as f32 / scale, region.y + *sy as f32 / scale),
                    Size::new(w / scale, h / scale),
                ),
                at: Rectangle::new(
                    Point::new(
                        pixels.x() as f32 / part_scale,
                        pixels.y() as f32 / part_scale,
                    ),
                    Size::new(w / part_scale, h / part_scale),
                ),
                outlines,
            },
        );
    }
    let paper = base.pixel(0, 0).map_or(Color::WHITE, |p| {
        Color::from_rgb8(p.red(), p.green(), p.blue())
    });
    // The screen takes colors as they are, not multiplied by how much is
    // seen of them: the parts' blurred edges are.
    let (width, height) = (pixmap.width(), pixmap.height());
    let rgba = straight(pixmap, ground_h);
    Some(Raster {
        handle: image::Handle::from_rgba(width, height, rgba),
        region: Rectangle::new(
            region.position(),
            Size::new(width as f32 / scale, height as f32 / scale),
        ),
        paper,
        _allocation: None,
        parts: Some(Arc::new(Cutouts {
            parts: cutouts,
            swatches,
        })),
    })
}

/// One part of a picture: its name, and its groups, each with how much of
/// it the groups around it let be seen.
struct Part<'a> {
    name: String,
    nodes: Vec<(&'a usvg::Node, f32)>,
}

/// The parts laid out on a sheet below its ground, drawn at `part_scale`.
struct Shelves<'a, 'b> {
    part_scale: f32,
    /// Each part's pixels at that scale, and the part.
    boxes: Vec<(IntRect, &'b Part<'a>)>,
    /// Where each lies on the sheet, in pixels.
    slots: Vec<(u32, u32)>,
    width: u32,
    height: u32,
}

impl Shelves<'_, '_> {
    fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// The parts of `bounds` at `part_scale`, in rows as wide as the ground
/// below it, each `sheet_gap` from the next and from the edges, so nothing
/// a glide takes past a part reaches another.
fn shelves<'a, 'b>(
    bounds: &[(usvg::NonZeroRect, &'b Part<'a>)],
    part_scale: f32,
    ground_w: u32,
    ground_h: u32,
) -> Shelves<'a, 'b> {
    let gap = sheet_gap(part_scale);
    // From the pixel its left edge lies in to the one its right edge lies
    // in, a pixel more on each side for its blur.
    let boxes: Vec<(IntRect, &Part<'_>)> = bounds
        .iter()
        .filter_map(|(bounds, part)| {
            let left = (bounds.left() * part_scale).floor() as i32 - 1;
            let top = (bounds.top() * part_scale).floor() as i32 - 1;
            let right = (bounds.right() * part_scale).ceil() as i32 + 1;
            let bottom = (bounds.bottom() * part_scale).ceil() as i32 + 1;
            Some((IntRect::from_ltrb(left, top, right, bottom)?, *part))
        })
        .collect();
    let width = ground_w.max(
        boxes
            .iter()
            .map(|(b, _)| b.width() + 2 * gap)
            .max()
            .unwrap_or(0),
    );
    let mut slots = Vec::with_capacity(boxes.len());
    let (mut x, mut y, mut row) = (gap, ground_h + gap, 0u32);
    for (pixels, _) in &boxes {
        if x > gap && x + pixels.width() + gap > width {
            (x, y, row) = (gap, y + row + gap, 0);
        }
        slots.push((x, y));
        x += pixels.width() + gap;
        row = row.max(pixels.height());
    }
    Shelves {
        part_scale,
        boxes,
        slots,
        width,
        height: y + row + gap,
    }
}

/// The sheet's pixels as the screen takes them: colors as they are, not
/// multiplied by how much is seen of them, as a part's blurred edges are;
/// and each clear pixel beside a part in its neighbor's color, so a part
/// scaled on the GPU blends its edge with its own color, not with black.
/// The ground, `ground` rows high, is opaque and stays as it is.
fn straight(pixmap: export::tiny_skia::Pixmap, ground: u32) -> Vec<u8> {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let ground = ground as usize;
    let mut rgba = pixmap.take();
    for pixel in rgba[ground * w * 4..].as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        if alpha > 0 && alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    // One pass of bleeding: enough for a picture sampled between two
    // pixels.
    let seen: Vec<bool> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[3] > 0)
        .collect();
    for y in ground..h {
        for x in 0..w {
            let at = y * w + x;
            if seen[at] {
                continue;
            }
            let neighbor = [
                (x > 0).then(|| at - 1),
                (x + 1 < w).then(|| at + 1),
                (y > ground).then(|| at - w),
                (y + 1 < h).then(|| at + w),
            ]
            .into_iter()
            .flatten()
            .find(|&n| seen[n]);
            if let Some(n) = neighbor {
                let color = [rgba[n * 4], rgba[n * 4 + 1], rgba[n * 4 + 2]];
                rgba[at * 4..at * 4 + 3].copy_from_slice(&color);
            }
        }
    }
    rgba
}

/// The groups of each part of the picture under `group`, by the part's
/// name, in the order drawn, each with how much of it `seen`, the groups
/// around it, let be seen: a shadow's 0.22.
fn gather_parts<'a>(group: &'a usvg::Group, seen: f32, parts: &mut Vec<Part<'a>>) {
    for node in group.children() {
        let usvg::Node::Group(inner) = node else {
            continue;
        };
        match omaspec_uml::part_name(inner.id()) {
            Some(name) => match parts.iter_mut().find(|part| part.name == name) {
                Some(part) => part.nodes.push((node, seen)),
                None => parts.push(Part {
                    name,
                    nodes: vec![(node, seen)],
                }),
            },
            None => gather_parts(inner, seen * inner.opacity().get(), parts),
        }
    }
}

/// Uploads the pixels to the GPU; `None` if that failed.
fn upload(raster: Raster) -> Task<Option<Raster>> {
    iced_runtime::image::allocate(raster.handle.clone()).map(move |allocation| {
        Some(Raster {
            _allocation: Some(allocation.ok()?),
            ..raster.clone()
        })
    })
}

/// One layer: the raster on screen and the newest request for it.
#[derive(Default)]
pub struct Layer {
    pub shown: Option<Raster>,
    /// What the newest request asks for.
    pub asked: Option<Plan>,
    /// Numbers the requests; one that is no longer the newest gives up.
    latest: Arc<AtomicU64>,
}

impl Layer {
    /// Rasterizes `plan` in the background after `delay` (zooming in steps
    /// should not rasterize every step), unless that is asked already.
    /// `None` empties the layer. `done` makes the answer message.
    pub fn ask(
        &mut self,
        plan: Option<Plan>,
        tree: &Arc<usvg::Tree>,
        delay: Duration,
        done: fn(u64, Option<Raster>) -> Message,
    ) -> Task<Message> {
        let tree = Arc::clone(tree);
        self.request(plan, delay, done, move |plan| render(&tree, plan))
    }

    /// [`Self::ask`] for the [`Sheet`] `sheet`.
    pub fn ask_sheet(
        &mut self,
        plan: Option<Plan>,
        sheet: Sheet,
        delay: Duration,
        done: fn(u64, Option<Raster>) -> Message,
    ) -> Task<Message> {
        self.request(plan, delay, done, move |plan| sheet.draw(plan))
    }

    fn request(
        &mut self,
        plan: Option<Plan>,
        delay: Duration,
        done: fn(u64, Option<Raster>) -> Message,
        draw: impl FnOnce(Plan) -> Option<Raster> + Send + 'static,
    ) -> Task<Message> {
        if plan == self.asked {
            return Task::none();
        }
        self.asked = plan;
        let seq = self.latest.fetch_add(1, Ordering::Relaxed) + 1;
        let Some(plan) = plan else {
            self.shown = None;
            return Task::none();
        };
        let latest = Arc::clone(&self.latest);
        Task::perform(
            blocking(move || {
                std::thread::sleep(delay);
                if latest.load(Ordering::Relaxed) != seq {
                    return None;
                }
                draw(plan)
            }),
            Option::flatten,
        )
        .then(|raster| raster.map_or_else(|| Task::done(None), upload))
        .map(move |raster| done(seq, raster))
    }

    /// Shows a finished raster unless a newer one was asked for since;
    /// says whether it did.
    pub fn receive(&mut self, seq: u64, raster: Option<Raster>) -> bool {
        if let Some(raster) = raster
            && seq == self.latest.load(Ordering::Relaxed)
        {
            self.shown = Some(raster);
            return true;
        }
        false
    }

    /// Whether `seq` is the raster asked for last.
    pub fn is_latest(&self, seq: u64) -> bool {
        seq == self.latest.load(Ordering::Relaxed)
    }

    /// Forgets what was asked, so the next request renders again: the
    /// diagram changed.
    pub fn forget(&mut self) {
        self.asked = None;
    }

    /// Empties the layer; rasters still on their way are dropped.
    pub fn clear(&mut self) {
        self.latest.fetch_add(1, Ordering::Relaxed);
        self.asked = None;
        self.shown = None;
    }

    /// The number the newest request carries.
    #[cfg(test)]
    pub fn seq(&self) -> u64 {
        self.latest.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On a sheet drawn sharp, each part lies far enough from every other
    /// and from the ground that a glide taking it a little wider than its
    /// pixels takes nothing of them.
    #[test]
    fn a_sheet_keeps_its_parts_apart_at_any_scale() {
        let source = "@startuml\n[*] --> Idle\nIdle --> Busy : go\nBusy --> Idle : done\n' parts (Omaspec)\n@enduml\n";
        let drawn = super::super::testing::drawn(source, omaspec_core::Language::PlantUml);
        assert!(drawn.parts);
        let size = drawn.tree.size();
        let size = Size::new(size.width(), size.height());
        let plan = Plan::overview(size, 7.0).unwrap();
        let sheet = sheet(drawn.bare.as_ref().unwrap(), &drawn.tree, plan).unwrap();
        let parts = sheet.parts.as_ref().unwrap();
        assert!(parts.parts.contains_key("state:Idle") && parts.parts.contains_key("state:Busy"));
        let reach = |r: Rectangle| {
            Rectangle::new(
                Point::new(r.x - SHEET_REACH, r.y - SHEET_REACH),
                Size::new(r.width + 2.0 * SHEET_REACH, r.height + 2.0 * SHEET_REACH),
            )
        };
        let ground_bottom = sheet.region.y + size.height;
        for (name, cutout) in &parts.parts {
            let wide = reach(cutout.sheet);
            assert!(wide.y >= ground_bottom, "{name} reaches the ground");
            for (other, them) in &parts.parts {
                if other != name {
                    assert!(
                        wide.intersection(&them.sheet).is_none(),
                        "{name} reaches {other}"
                    );
                }
            }
        }
    }

    /// A sheet carries a patch of each color its parts' shapes are filled
    /// with, the color itself in its middle: a glide fills a box's room
    /// from it, and it is on the GPU with the sheet.
    #[test]
    fn a_sheet_has_a_patch_of_each_fill() {
        let source = "@startuml\n[*] --> Idle\nIdle --> Busy : go\nnote right of Busy : hot\n' parts (Omaspec)\n@enduml\n";
        let drawn = super::super::testing::drawn(source, omaspec_core::Language::PlantUml);
        let size = drawn.tree.size();
        let plan = Plan::overview(Size::new(size.width(), size.height()), 2.0).unwrap();
        let sheet = Sheet::of(&drawn).unwrap().draw(plan).unwrap();
        let image::Handle::Rgba { width, pixels, .. } = &sheet.handle else {
            panic!("a sheet is pixels");
        };
        let mut colors = 0;
        for cutout in sheet.parts.as_ref().unwrap().parts.values() {
            for color in cutout.outlines.colors() {
                colors += 1;
                let patch = sheet.swatch(color).expect("a patch of each fill");
                let x = ((patch.center_x() - sheet.region.x) * plan.scale) as usize;
                let y = ((patch.center_y() - sheet.region.y) * plan.scale) as usize;
                let at = (y * *width as usize + x) * 4;
                for (channel, value) in color.iter().enumerate() {
                    let want = (value * 255.0).round() as i32;
                    let got = i32::from(pixels[at + channel]);
                    assert!((want - got).abs() <= 1, "{color:?} at {patch:?}");
                }
            }
        }
        assert!(colors > 0, "the states are filled");
        assert!(sheet.swatch([0.123, 0.456, 0.789]).is_none());
    }

    /// A shadow, a group seen at 0.22 around its box's part, is drawn on
    /// the sheet at that strength, not as solid black.
    #[test]
    fn a_part_s_shadow_is_drawn_at_its_strength() {
        let source =
            "@startuml\nskinparam shadowing true\nclass Loan\n' parts (Omaspec)\n@enduml\n";
        let drawn = super::super::testing::drawn(source, omaspec_core::Language::PlantUml);
        let size = drawn.tree.size();
        let plan = Plan::overview(Size::new(size.width(), size.height()), 2.0).unwrap();
        let sheet = sheet(drawn.bare.as_ref().unwrap(), &drawn.tree, plan).unwrap();
        let cutout = sheet
            .parts
            .as_ref()
            .unwrap()
            .parts
            .values()
            .next()
            .unwrap()
            .clone();
        let image::Handle::Rgba { width, pixels, .. } = &sheet.handle else {
            panic!("a sheet is pixels");
        };
        // The shadow lies past the box's right edge, three units down.
        let k = 2.0;
        let x = ((cutout.sheet.x - sheet.region.x + cutout.sheet.width) * k) as usize - 4;
        let y = ((cutout.sheet.y - sheet.region.y + cutout.sheet.height / 2.0) * k) as usize;
        let alpha = pixels[(y * *width as usize + x) * 4 + 3];
        assert!((30..90).contains(&alpha), "the shadow's alpha {alpha}");
    }

    /// A clear pixel beside a part takes the part's color, so the GPU,
    /// blending the two while it scales the part, blends no black in.
    #[test]
    fn a_part_s_edge_bleeds_its_color_into_the_clear_pixels_beside_it() {
        use export::tiny_skia::{Color as Skia, Pixmap};
        let mut pixmap = Pixmap::new(3, 2).unwrap();
        pixmap.fill(Skia::TRANSPARENT);
        // The ground's row, then a half-seen red pixel and two clear ones.
        let data = pixmap.data_mut();
        data[..12].copy_from_slice(&[255, 255, 255, 255].repeat(3));
        data[12..16].copy_from_slice(&[100, 0, 0, 128]);
        let rgba = straight(pixmap, 1);
        assert_eq!(&rgba[12..16], &[199, 0, 0, 128], "demultiplied");
        assert_eq!(&rgba[16..20], &[199, 0, 0, 0], "bled into its neighbor");
        assert_eq!(&rgba[20..24], &[0, 0, 0, 0], "two pixels off, clear");
        assert_eq!(&rgba[..4], &[255, 255, 255, 255], "the ground as it is");
    }

    /// A long sequence diagram: 34 million units², more than the overview holds.
    const TALL: Size = Size::new(2850.0, 12093.0);
    const SMALL: Size = Size::new(400.0, 200.0);

    fn visible(x: f32, y: f32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(900.0, 500.0))
    }

    #[test]
    fn small_diagrams_need_no_detail() {
        let overview = Plan::overview(SMALL, 8.0).unwrap();
        assert_eq!(overview.scale, 8.0);
        assert_eq!(
            (overview.part.width(), overview.part.height()),
            (3200, 1600)
        );
        assert_eq!(Plan::detail(SMALL, 8.0, visible(0.0, 0.0), None), None);
    }

    #[test]
    fn large_diagrams_get_a_limited_overview_and_a_sharp_detail() {
        let overview = Plan::overview(TALL, 2.0).unwrap();
        assert!(overview.scale < 1.0);
        let pixels = overview.part.width() as f32 * overview.part.height() as f32;
        assert!(pixels <= OVERVIEW_PIXELS * 1.001, "{pixels}");

        let detail = Plan::detail(TALL, 2.0, visible(1000.0, 5000.0), None).unwrap();
        assert_eq!(detail.scale, 2.0);
        // The visible part and a quarter of it beyond each edge.
        let region = detail.region();
        assert_eq!(
            region,
            Rectangle::new(Point::new(775.0, 4875.0), Size::new(1350.0, 750.0))
        );
    }

    #[test]
    fn the_detail_stays_while_it_covers_the_view() {
        let first = Plan::detail(TALL, 2.0, visible(1000.0, 5000.0), None);
        let nudged = Plan::detail(TALL, 2.0, visible(1100.0, 5050.0), first);
        assert_eq!(nudged, first);
        let moved = Plan::detail(TALL, 2.0, visible(1300.0, 5000.0), first);
        assert_ne!(moved, first);
        let zoomed = Plan::detail(TALL, 3.0, visible(1000.0, 5000.0), first);
        assert_eq!(zoomed.unwrap().scale, 3.0);
    }

    #[test]
    fn the_detail_ends_at_the_diagram() {
        let corner = Plan::detail(TALL, 2.0, visible(-400.0, -300.0), None).unwrap();
        assert_eq!(corner.region().position(), Point::ORIGIN);
        assert_eq!(Plan::detail(TALL, 2.0, visible(-5000.0, 0.0), None), None);
    }

    #[test]
    fn a_picture_is_halved_for_showing_it_small() {
        // A picture of 800 by 400 pixels, white but for a black column.
        let (w, h) = (800usize, 400usize);
        let mut pixels = vec![255u8; w * h * 4];
        for y in 0..h {
            for channel in 0..3 {
                pixels[(y * w + 10) * 4 + channel] = 0;
            }
        }
        let picture = Raster {
            handle: image::Handle::from_rgba(w as u32, h as u32, pixels),
            region: Rectangle::with_size(Size::new(400.0, 200.0)),
            paper: Color::WHITE,
            _allocation: None,
            parts: None,
        };
        let copies = halves(&picture);
        let sizes: Vec<(u32, u32)> = copies.iter().filter_map(Raster::pixels).collect();
        // Down to the last whose sides are both 48 pixels or more.
        assert_eq!(sizes, [(400, 200), (200, 100), (100, 50)]);
        for copy in &copies {
            assert_eq!(copy.region, picture.region);
        }
        // The thin line is still there, fainter: half of one pixel in two.
        let image::Handle::Rgba { pixels, .. } = &copies[0].handle else {
            panic!("pixels in memory");
        };
        assert_eq!(&pixels[5 * 4..5 * 4 + 4], [128, 128, 128, 255]);
        assert_eq!(&pixels[4 * 4..4 * 4 + 4], [255, 255, 255, 255]);
        // A picture too small to halve has no copies.
        assert!(halves(&copies[2]).is_empty());
    }

    #[test]
    fn regions_are_whole_pixels() {
        let plan = Plan::overview(Size::new(100.5, 50.25), 1.0).unwrap();
        assert_eq!((plan.part.width(), plan.part.height()), (101, 51));
        assert_eq!(plan.region().size(), Size::new(101.0, 51.0));
    }
}
