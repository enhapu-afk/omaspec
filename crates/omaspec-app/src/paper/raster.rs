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

/// How many colors [`Raster::swatch`] keeps; a picture has a few.
const SWATCHES: usize = 256;

impl Raster {
    /// A small picture of one `color`, red green blue each 0 to 1, four
    /// pixels a side and four units large: the same picture for the same
    /// color, so it goes up to the GPU once.
    pub fn swatch(color: [f32; 3]) -> Raster {
        use std::cell::RefCell;
        use std::collections::HashMap;
        thread_local! {
            static KEPT: RefCell<HashMap<[u8; 3], image::Handle>> = RefCell::new(HashMap::new());
        }
        let rgb = color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
        let handle = KEPT.with(|kept| {
            let mut kept = kept.borrow_mut();
            if kept.len() >= SWATCHES && !kept.contains_key(&rgb) {
                kept.clear();
            }
            kept.entry(rgb)
                .or_insert_with(|| {
                    image::Handle::from_rgba(4, 4, [rgb[0], rgb[1], rgb[2], 255].repeat(16))
                })
                .clone()
        });
        Raster {
            handle,
            region: Rectangle::with_size(Size::new(4.0, 4.0)),
            paper: Color::from_rgb8(rgb[0], rgb[1], rgb[2]),
            _allocation: None,
            parts: None,
        }
    }

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
/// part's name (docs/adr/0029).
#[derive(Debug, Default)]
pub struct Cutouts(pub HashMap<String, Cutout>);

/// One part on a sheet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cutout {
    /// Where its picture lies on the sheet, in the sheet's units.
    pub sheet: Rectangle,
    /// Where it is drawn in the diagram, in diagram units: as large as
    /// `sheet`.
    pub at: Rectangle,
}

impl Raster {
    /// The picture of the part `name` of a sheet, as a picture of its own:
    /// the sheet, placed so that the part lies at its place in the diagram.
    pub fn part(&self, name: &str) -> Option<(Raster, Rectangle)> {
        let cutout = self.parts.as_ref()?.0.get(name)?;
        let shift = cutout.at.position() - cutout.sheet.position();
        Some((
            Raster {
                region: Rectangle::new(self.region.position() + shift, self.region.size()),
                ..self.clone()
            },
            cutout.at,
        ))
    }
}

/// Space between two parts on a sheet, in pixels, so the blur of one does
/// not reach the next, and a glide that takes a part by its box's
/// rectangle, a little wider than its pixels, takes nothing of another.
const SHEET_GAP: u32 = 8;

/// `ground` at `plan`, and below it each part of `full` on its own, on one
/// sheet: one picture to rasterize and to upload for a glide that takes
/// its parts whole (docs/adr/0029). The parts lie in rows as wide as the
/// ground, each part's own pixels and nothing else, on clear pixels.
pub fn sheet(ground: &usvg::Tree, full: &usvg::Tree, plan: Plan) -> Option<Raster> {
    use export::tiny_skia::{Pixmap, PixmapPaint, Transform};
    let base = export::rasterize_part(ground, plan.scale, plan.part).ok()?;
    let scale = plan.scale;
    let mut parts: Vec<(String, Vec<&usvg::Node>)> = Vec::new();
    gather_parts(full.root(), &mut parts);
    // Each part's pixels: from the pixel its left edge lies in to the one
    // its right edge lies in, a pixel more on each side for its blur.
    let boxes: Vec<(IntRect, &(String, Vec<&usvg::Node>))> = parts
        .iter()
        .filter_map(|part| {
            let bounds = part
                .1
                .iter()
                .filter_map(|node| node.abs_layer_bounding_box())
                .reduce(|a, b| {
                    usvg::NonZeroRect::from_ltrb(
                        a.left().min(b.left()),
                        a.top().min(b.top()),
                        a.right().max(b.right()),
                        a.bottom().max(b.bottom()),
                    )
                    .unwrap_or(a)
                })?;
            let left = (bounds.left() * scale).floor() as i32 - 1;
            let top = (bounds.top() * scale).floor() as i32 - 1;
            let right = (bounds.right() * scale).ceil() as i32 + 1;
            let bottom = (bounds.bottom() * scale).ceil() as i32 + 1;
            Some((IntRect::from_ltrb(left, top, right, bottom)?, part))
        })
        .collect();
    // Shelves: a row of parts as wide as the ground, the next below it.
    let width = base
        .width()
        .max(boxes.iter().map(|(b, _)| b.width()).max().unwrap_or(0));
    let mut slots = Vec::with_capacity(boxes.len());
    let (mut x, mut y, mut row) = (0u32, base.height() + SHEET_GAP, 0u32);
    for (pixels, _) in &boxes {
        if x > 0 && x + pixels.width() > width {
            (x, y, row) = (0, y + row + SHEET_GAP, 0);
        }
        slots.push((x, y));
        x += pixels.width() + SHEET_GAP;
        row = row.max(pixels.height());
    }
    let height = y + row;
    if u64::from(width) * u64::from(height) > 4 * OVERVIEW_PIXELS as u64 {
        return None;
    }
    let mut pixmap = Pixmap::new(width, height)?;
    pixmap.draw_pixmap(
        0,
        0,
        base.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    let region = plan.region();
    let unit = |px: f32| px / scale;
    let mut cutouts = HashMap::with_capacity(boxes.len());
    for ((pixels, (name, nodes)), (sx, sy)) in boxes.iter().zip(&slots) {
        // Diagram units onto the part's slot of the sheet.
        let onto = Transform::from_translate(*sx as f32, *sy as f32)
            .pre_translate(-pixels.x() as f32, -pixels.y() as f32)
            .pre_scale(scale, scale);
        for node in nodes {
            // resvg draws a node from where its box begins: start there.
            let Some(bounds) = node.abs_layer_bounding_box() else {
                continue;
            };
            let at = onto
                .pre_concat(node.abs_transform())
                .pre_translate(bounds.x(), bounds.y());
            let _ = export::render_node(node, at, &mut pixmap.as_mut());
        }
        cutouts.insert(
            name.clone(),
            Cutout {
                sheet: Rectangle::new(
                    Point::new(region.x + unit(*sx as f32), region.y + unit(*sy as f32)),
                    Size::new(unit(pixels.width() as f32), unit(pixels.height() as f32)),
                ),
                at: Rectangle::new(
                    Point::new(unit(pixels.x() as f32), unit(pixels.y() as f32)),
                    Size::new(unit(pixels.width() as f32), unit(pixels.height() as f32)),
                ),
            },
        );
    }
    let paper = base.pixel(0, 0).map_or(Color::WHITE, |p| {
        Color::from_rgb8(p.red(), p.green(), p.blue())
    });
    // The screen takes colors as they are, not multiplied by how much is
    // seen of them: the parts' blurred edges are.
    let rgba: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let c = pixel.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Some(Raster {
        handle: image::Handle::from_rgba(width, height, rgba),
        region: Rectangle::new(
            region.position(),
            Size::new(unit(width as f32), unit(height as f32)),
        ),
        paper,
        _allocation: None,
        parts: Some(Arc::new(Cutouts(cutouts))),
    })
}

/// The groups of each part of the picture under `group`, by the part's
/// name, in the order drawn.
fn gather_parts<'a>(group: &'a usvg::Group, parts: &mut Vec<(String, Vec<&'a usvg::Node>)>) {
    for node in group.children() {
        let usvg::Node::Group(inner) = node else {
            continue;
        };
        match omaspec_uml::part_name(inner.id()) {
            Some(name) => match parts.iter_mut().find(|(have, _)| *have == name) {
                Some((_, nodes)) => nodes.push(node),
                None => parts.push((name, vec![node])),
            },
            None => gather_parts(inner, parts),
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
        self.ask_sheet(plan, tree, None, delay, done)
    }

    /// [`Self::ask`]; with `full`, the picture whose parts `tree` is the
    /// ground of, a [`sheet`] of the ground and the parts.
    pub fn ask_sheet(
        &mut self,
        plan: Option<Plan>,
        tree: &Arc<usvg::Tree>,
        full: Option<&Arc<usvg::Tree>>,
        delay: Duration,
        done: fn(u64, Option<Raster>) -> Message,
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
        let tree = Arc::clone(tree);
        let full = full.map(Arc::clone);
        Task::perform(
            blocking(move || {
                std::thread::sleep(delay);
                if latest.load(Ordering::Relaxed) != seq {
                    return None;
                }
                match &full {
                    Some(full) => sheet(&tree, full, plan),
                    None => render(&tree, plan),
                }
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
    pub fn seq(&self) -> u64 {
        self.latest.load(Ordering::Relaxed)
    }

    /// Rasterizes what was asked right here, as the background would.
    pub fn finish(&mut self, tree: &usvg::Tree) {
        if let Some(plan) = self.asked {
            let seq = self.latest.load(Ordering::Relaxed);
            let _ = self.receive(seq, render(tree, plan));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
