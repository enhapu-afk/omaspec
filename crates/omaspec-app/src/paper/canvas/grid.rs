//! The screen's pixels as a canvas sees them, and the paper drawn on
//! them: its rounded corners, its bands, its shadow.

use iced::widget::canvas::{self, Frame, Path};
use iced::widget::image;
use iced::{Color, Point, Rectangle, Size, Vector, border};

use super::*;

/// The screen's pixels, as seen from a canvas: where it lies in the window,
/// the window's scale, and how big the canvas is.
#[derive(Clone, Copy)]
pub struct Grid {
    pub(in crate::paper::canvas) origin: Point,
    pub(in crate::paper) scale: f32,
    pub(in crate::paper::canvas) size: Size,
}

impl Grid {
    pub fn of(bounds: Rectangle, scale: f32) -> Self {
        Self {
            origin: bounds.position(),
            scale: if scale > 0.0 { scale } else { 1.0 },
            size: bounds.size(),
        }
    }

    /// `rect` with its edges on whole pixels. The paper and its pictures then
    /// share an edge instead of leaving a sliver of one past the other.
    pub fn snap(self, rect: Rectangle) -> Rectangle {
        let on = |at: f32, from: f32| ((from + at) * self.scale).round() / self.scale - from;
        let (left, top) = (on(rect.x, self.origin.x), on(rect.y, self.origin.y));
        let right = on(rect.x + rect.width, self.origin.x);
        let bottom = on(rect.y + rect.height, self.origin.y);
        Rectangle::new(Point::new(left, top), Size::new(right - left, bottom - top))
    }

    /// Where a picture may be drawn: inside `clip` and inside this canvas.
    ///
    /// `Frame::with_clip` replaces the clip, and a picture is cut only by
    /// that clip and by the window. A diagram dragged past the pane would
    /// otherwise paint over the editor. Shapes stay in the canvas on their
    /// own; pictures do not.
    ///
    /// A rounded picture is held one device pixel inside that edge. The
    /// pixmap is cleared white past the diagram, and the linear filter reads
    /// that into the last pixel; the picture then fades over the pixel past
    /// its edge. On a dark theme either one is a light line that comes and
    /// goes as the pan moves the texels across the pixels. The paper
    /// underneath is the diagram's own background and fills that pixel. The
    /// picture is not snapped again: a second rounding can step off this
    /// edge and fade onto the next one.
    fn picture_clip(self, clip: Rectangle, radius: f32) -> Option<Rectangle> {
        let canvas = Rectangle::new(Point::ORIGIN, self.size);
        let clip = self.snap(clip).intersection(&canvas)?;
        let clip = self.snap(clip);
        if clip.width <= 0.0 || clip.height <= 0.0 {
            return None;
        }
        if radius <= 0.0 {
            return Some(clip);
        }
        let inset = 1.0 / self.scale;
        let clip = Rectangle::new(
            Point::new(clip.x + inset, clip.y + inset),
            Size::new(clip.width - inset * 2.0, clip.height - inset * 2.0),
        );
        (clip.width > 0.0 && clip.height > 0.0).then_some(clip)
    }

    /// Draws `pictures` within `clip`, its corners rounded by `radius`.
    pub fn draw<'a>(
        self,
        frame: &mut Frame,
        clip: Rectangle,
        radius: f32,
        pictures: impl IntoIterator<Item = (Rectangle, &'a Raster, f32)>,
    ) {
        let Some(clip) = self.picture_clip(clip, radius) else {
            return;
        };
        // The GPU fades a picture's corner out over the pixel past its edge,
        // half a pixel further out than a shape's; a larger radius puts the
        // two curves on each other, so no rim of the picture shows past the
        // paper or a veil.
        let radius = if radius > 0.0 {
            radius + 0.5 / (std::f32::consts::SQRT_2 - 1.0) / self.scale
        } else {
            0.0
        };
        frame.with_clip(clip, |frame| {
            for (at, raster, opacity) in pictures {
                frame.draw_image(
                    self.snap(at),
                    canvas::Image {
                        border_radius: radius.into(),
                        ..canvas::Image::new(raster.handle.clone())
                            .filter_method(image::FilterMethod::Linear)
                            .opacity(opacity)
                    },
                );
            }
        });
    }
}

pub fn rounded(rect: Rectangle, radius: f32) -> Path {
    Path::rounded_rectangle(rect.position(), rect.size(), border::Radius::from(radius))
}

/// Fills the part of the rounded `paper` from `top` to `bottom`, for a veil
/// over some of it: the paper's outline one device pixel larger, cut at
/// the band's rows.
///
/// The veil reaches past the paper's edge. Its own outline, the paper's
/// grown, takes no point off the paper's, as a line of its own would in the
/// corners. And no fading edge lies on another: the GPU fades each layer's
/// edges before it lays one over the other, and where the paper's edge
/// pixel is three quarters white, a veil that covers it to three quarters
/// too lets more of that white through than the paper next to it shows.
///
/// The band is a shape of its own, not the whole outline in a clip: the
/// software renderer (no GPU) lays a canvas's fills over everything,
/// whatever the clip, so a soft spot's rows stacked into one solid column.
pub fn fill_paper_band(
    frame: &mut Frame,
    paper: Rectangle,
    scale: f32,
    top: f32,
    bottom: f32,
    fill: impl Into<canvas::Fill>,
) {
    fill_paper_band_across(frame, paper, scale, (top, bottom), (None, None), fill);
}

/// [`fill_paper_band`] between `left` and `right` alone, each on a whole
/// device pixel, so pieces side by side meet without a seam; `None` is the
/// paper's own side, its corners round as the band's.
pub fn fill_paper_band_across(
    frame: &mut Frame,
    paper: Rectangle,
    scale: f32,
    (top, bottom): (f32, f32),
    (left, right): (Option<f32>, Option<f32>),
    fill: impl Into<canvas::Fill>,
) {
    let Some(band) = band_rows(paper, scale, top, bottom) else {
        return;
    };
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let out = 1.0 / scale;
    let veil = Rectangle::new(
        paper.position() - Vector::new(out, out),
        paper.size() + Size::new(out * 2.0, out * 2.0),
    );
    let column = |at: f32| (at * scale).round() / scale;
    let across = (
        left.map_or(f32::NEG_INFINITY, column),
        right.map_or(f32::INFINITY, column),
    );
    if across.1 <= across.0 {
        return;
    }
    if let Some(shape) = rounded_band(veil, RADIUS + out, band.y, band.y + band.height, across) {
        frame.fill(&shape, fill);
    }
}

/// How many straight pieces a quarter of a rounded corner takes in
/// [`rounded_band`]; at the paper's radius they stay a hundredth of a
/// pixel off the arc.
const ARC_PIECES: usize = 16;

/// The part of the rounded rectangle `rect` from `top` to `bottom`, and
/// from the left to the right of `across`: its sides, bent where they run
/// round a corner, or straight where `across` cuts it short.
fn rounded_band(
    rect: Rectangle,
    radius: f32,
    top: f32,
    bottom: f32,
    across: (f32, f32),
) -> Option<Path> {
    let (top, bottom) = (top.max(rect.y), bottom.min(rect.y + rect.height));
    if bottom <= top {
        return None;
    }
    let radius = radius.min(rect.width / 2.0).min(rect.height / 2.0).max(0.0);
    // How far the outline lies in from the side at height `y`.
    let inset = |y: f32| {
        let from_edge = (y - rect.y).min(rect.y + rect.height - y);
        let up = (radius - from_edge).max(0.0);
        radius - (radius * radius - up * up).max(0.0).sqrt()
    };
    // The heights where the outline bends: the band's own, and the arcs'
    // points between them, evenly round each corner.
    let mut heights = vec![top, bottom];
    for i in 0..=ARC_PIECES {
        let turn = std::f32::consts::FRAC_PI_2 * i as f32 / ARC_PIECES as f32;
        let rise = radius * (1.0 - turn.sin());
        for y in [rect.y + rise, rect.y + rect.height - rise] {
            if y > top && y < bottom {
                heights.push(y);
            }
        }
    }
    heights.sort_by(f32::total_cmp);
    heights.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
    let left = |y: f32| (rect.x + inset(y)).max(across.0);
    let right = |y: f32| (rect.x + rect.width - inset(y)).min(across.1);
    Some(Path::new(|b| {
        b.move_to(Point::new(left(heights[0]), heights[0]));
        for &y in &heights[1..] {
            b.line_to(Point::new(left(y), y));
        }
        for &y in heights.iter().rev() {
            b.line_to(Point::new(right(y), y));
        }
        b.close();
    }))
}

/// The rows of [`fill_paper_band`], on whole device pixels of the canvas,
/// so neighbouring bands meet without a seam. Where the band reaches the
/// paper's edge, it reaches past the veil's.
fn band_rows(paper: Rectangle, scale: f32, top: f32, bottom: f32) -> Option<Rectangle> {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let (paper_bottom, paper_right) = (paper.y + paper.height, paper.x + paper.width);
    let below = |at: f32| ((at * scale).floor() - 3.0) / scale;
    let past = |at: f32| ((at * scale).ceil() + 3.0) / scale;
    let row = |at: f32| (at * scale).round() / scale;
    let top = if top <= paper.y {
        below(paper.y)
    } else {
        row(top)
    };
    let bottom = if bottom >= paper_bottom {
        past(paper_bottom)
    } else {
        row(bottom)
    };
    let (left, right) = (below(paper.x), past(paper_right));
    (bottom > top)
        .then(|| Rectangle::new(Point::new(left, top), Size::new(right - left, bottom - top)))
}

/// A soft shadow under the paper, rounded rectangles that grow and fade:
/// dark and a little below it on a light background, a faint glow all
/// round on a dark one, where a shadow would not show.
///
/// `paper` is already on whole pixels. Each layer grows and drops by whole
/// pixels too. A fraction of a pixel would land inside or outside that edge
/// depending on where the pan sits, and on a dark theme the white glow then
/// draws a line that appears and disappears as the diagram moves. `shown`
/// fades it, for a paper that comes or goes.
pub fn shadow(frame: &mut Frame, paper: Rectangle, background: Color, scale: f32, shown: f32) {
    let (color, strength, drop) = shade(background);
    // Each step adds as much; more of them lie near the edge, so the shadow
    // fades out softly, as (1 - distance / HALO_BLUR)².
    let alpha = strength * shown / f32::from(HALO_STEPS);
    for step in 0..HALO_STEPS {
        let Some(rect) = halo_rect(paper, step, drop, scale) else {
            continue;
        };
        let grow = (rect.width - paper.width) * 0.5;
        frame.fill(&rounded(rect, RADIUS + grow), Color { a: alpha, ..color });
    }
}

/// Ink of the halo: white and faint on a dark ground, black and a little
/// lower on a light one.
fn shade(background: Color) -> (Color, f32, f32) {
    let light = 0.2126 * background.r + 0.7152 * background.g + 0.0722 * background.b;
    if light < 0.5 {
        (Color::WHITE, 0.05, 0.0)
    } else {
        (Color::BLACK, 0.10, 2.0)
    }
}

/// One halo layer around `paper`, or `None` when it would sit entirely
/// underneath. `step` 0 is the outermost. Growth and drop are whole device
/// pixels at `scale`.
fn halo_rect(paper: Rectangle, step: u8, drop: f32, scale: f32) -> Option<Rectangle> {
    let grow = HALO_BLUR * (1.0 - (f32::from(step) / f32::from(HALO_STEPS)).sqrt());
    let grow_px = (grow * scale).round() as i32;
    let drop_px = (drop * scale).round() as i32;
    if grow_px == 0 && drop_px == 0 {
        return None;
    }
    let grow = grow_px as f32 / scale;
    let drop = drop_px as f32 / scale;
    Some(Rectangle::new(
        paper.position() + Vector::new(-grow, drop - grow),
        paper.size() + Size::new(grow * 2.0, grow * 2.0),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(scale: f32) -> Grid {
        Grid::of(
            Rectangle::new(Point::new(13.4, 27.8), Size::new(400.0, 300.0)),
            scale,
        )
    }

    /// Device pixel of a canvas-local coordinate.
    fn device(grid: Grid, local: f32, axis: f32) -> i32 {
        ((axis + local) * grid.scale).round() as i32
    }

    #[test]
    fn a_picture_dragged_past_the_pane_stops_at_the_pane() {
        let grid = grid(2.0);
        // The paper hangs off the left and the bottom of the canvas.
        let paper = Rectangle::new(Point::new(-80.0, 40.0), Size::new(200.0, 400.0));
        // Square, as a lit state, and rounded, as the diagram itself.
        for radius in [0.0, RADIUS] {
            let clip = grid.picture_clip(paper, radius).unwrap();
            assert!(device(grid, clip.x, grid.origin.x) >= device(grid, 0.0, grid.origin.x));
            assert!(device(grid, clip.y, grid.origin.y) >= device(grid, 0.0, grid.origin.y));
            assert!(
                device(grid, clip.x + clip.width, grid.origin.x)
                    <= device(grid, grid.size.width, grid.origin.x)
            );
            assert!(
                device(grid, clip.y + clip.height, grid.origin.y)
                    <= device(grid, grid.size.height, grid.origin.y)
            );
        }
    }

    #[test]
    fn a_picture_outside_the_pane_is_not_drawn() {
        let grid = grid(1.0);
        let gone = Rectangle::new(Point::new(-500.0, 10.0), Size::new(40.0, 40.0));
        assert!(grid.picture_clip(gone, 0.0).is_none());
        assert!(grid.picture_clip(gone, RADIUS).is_none());
    }

    #[test]
    fn a_rounded_picture_stays_a_pixel_inside_the_paper() {
        let grid = grid(2.0);
        let paper = Rectangle::new(Point::new(40.0, 30.0), Size::new(180.0, 120.0));
        let snapped = grid.snap(paper);
        let clip = grid.picture_clip(paper, RADIUS).unwrap();
        let inset = 1.0 / grid.scale;
        for (got, edge) in [
            (clip.x, snapped.x + inset),
            (clip.y, snapped.y + inset),
            (clip.x + clip.width, snapped.x + snapped.width - inset),
            (clip.y + clip.height, snapped.y + snapped.height - inset),
        ] {
            assert!((got - edge).abs() < 1e-4, "{got} {edge}");
        }
        // A square clip, as a lit state uses, is not pulled in.
        let square = grid.picture_clip(paper, 0.0).unwrap();
        assert!((square.x - snapped.x).abs() < 1e-4);
        assert!((square.width - snapped.width).abs() < 1e-4);
    }

    #[test]
    fn the_dark_glow_keeps_its_width_while_the_paper_moves() {
        let scale = 2.0;
        let grid = grid(scale);
        let dark = Color::from_rgb(0.12, 0.12, 0.14);
        let (_, _, drop) = shade(dark);
        let mut seen = None;
        for step in 0..32 {
            let shift = step as f32 / 32.0;
            let paper = grid.snap(Rectangle::new(
                Point::new(40.0 + shift, 50.0 + shift * 0.7),
                Size::new(220.0, 140.0),
            ));
            let layers: Vec<(i32, i32)> = (0..HALO_STEPS)
                .filter_map(|step| halo_rect(paper, step, drop, scale))
                .map(|layer| {
                    let out = ((paper.x - layer.x) * scale).round() as i32;
                    let down = ((layer.y - paper.y) * scale).round() as i32;
                    assert_eq!(
                        out,
                        ((layer.width - paper.width) * 0.5 * scale).round() as i32
                    );
                    (out, down)
                })
                .collect();
            assert!(layers.iter().all(|(out, _)| *out >= 1), "{layers:?}");
            match &seen {
                None => seen = Some(layers),
                Some(first) => assert_eq!(&layers, first),
            }
        }
    }

    #[test]
    fn the_light_shadow_drops_by_whole_pixels() {
        let scale = 1.5;
        let grid = grid(scale);
        let (_, _, drop) = shade(Color::WHITE);
        let paper = grid.snap(Rectangle::new(
            Point::new(20.0, 20.0),
            Size::new(100.0, 80.0),
        ));
        let mut count = 0;
        for step in 0..HALO_STEPS {
            let Some(layer) = halo_rect(paper, step, drop, scale) else {
                continue;
            };
            count += 1;
            let grow = (paper.x - layer.x) * scale;
            let down = (layer.y - paper.y) * scale;
            assert!((grow - grow.round()).abs() < 1e-3, "{grow}");
            assert!((down - down.round()).abs() < 1e-3, "{down}");
        }
        assert!(count > 0);
    }

    #[test]
    fn a_veil_band_lies_on_whole_pixels_and_past_the_paper() {
        let scale = 1.5;
        let paper = Rectangle::new(Point::new(40.3, 30.7), Size::new(180.0, 120.0));
        let whole = |at: f32| ((at * scale) - (at * scale).round()).abs() < 1e-3;
        let (right, bottom) = (paper.x + paper.width, paper.y + paper.height);
        // Over the whole paper the rows reach past every edge.
        let all = band_rows(paper, scale, paper.y, bottom).unwrap();
        assert!(all.x < paper.x && all.y < paper.y);
        assert!(all.x + all.width > right && all.y + all.height > bottom);
        for edge in [all.x, all.y, all.x + all.width, all.y + all.height] {
            assert!(whole(edge), "{edge}");
        }
        // Bands that meet share a row: no gap, no overlap.
        let upper = band_rows(paper, scale, paper.y - 5.0, 77.4).unwrap();
        let lower = band_rows(paper, scale, 77.4, bottom + 5.0).unwrap();
        assert!((upper.y + upper.height - lower.y).abs() < 1e-4);
        assert!(whole(lower.y));
        // Less than half a pixel high is nothing.
        assert!(band_rows(paper, scale, 90.0, 90.1).is_none());
    }

    /// The points of a path, in order.
    /// Across the whole band.
    const WHOLE: (f32, f32) = (f32::NEG_INFINITY, f32::INFINITY);

    fn corners(path: &Path) -> Vec<Point> {
        use iced::widget::canvas::path::lyon_path::Event;
        path.raw()
            .iter()
            .filter_map(|event| match event {
                Event::Begin { at } | Event::Line { to: at, .. } => Some(Point::new(at.x, at.y)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_band_is_the_paper_between_its_rows_and_nothing_more() {
        let rect = Rectangle::new(Point::new(10.0, 20.0), Size::new(200.0, 300.0));
        let radius = 8.0;
        // In the middle: a rectangle as wide as the paper, as high as the band.
        let middle = corners(&rounded_band(rect, radius, 100.0, 140.0, WHOLE).unwrap());
        assert_eq!(middle.len(), 4);
        for at in &middle {
            assert!(at.y == 100.0 || at.y == 140.0, "{at:?}");
            assert!(at.x == 10.0 || at.x == 210.0, "{at:?}");
        }
        // At the top: from the paper's edge, round its corners. Every point
        // lies on the rounded outline, none above the paper or below the band.
        let top = corners(&rounded_band(rect, radius, 0.0, 60.0, WHOLE).unwrap());
        let (left, right) = (Point::new(18.0, 28.0), Point::new(202.0, 28.0));
        for at in &top {
            assert!((20.0..=60.0).contains(&at.y), "{at:?}");
            if at.y < 28.0 {
                let center = if at.x < 110.0 { left } else { right };
                assert!((at.distance(center) - radius).abs() < 1e-3, "{at:?}");
            }
        }
        let highest = top.iter().map(|at| at.y).fold(f32::INFINITY, f32::min);
        assert_eq!(highest, 20.0, "the band starts at the paper's edge");
        assert!(top.len() > 2 * ARC_PIECES, "the corner is bent, not cut");
        // Past the paper there is nothing.
        assert!(rounded_band(rect, radius, 330.0, 400.0, WHOLE).is_none());
    }
}
