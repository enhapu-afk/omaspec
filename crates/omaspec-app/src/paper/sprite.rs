//! A box's pixels, cut out of its picture, drawn into a box on screen,
//! as a glide carries each box from one place to the next
//! ([`super::morph`]).
//!
//! Where the box keeps its shape, its pixels are only scaled. Where it
//! changes shape, as a container folds into a small box or a box unfolds
//! into a container, stretching them would draw its words three times as
//! high. So the box is drawn in parts ([`fit`]): its corners keep their
//! size and its sides are stretched only along their length, so the rim
//! stays a rim; what it holds keeps its proportions and is never larger
//! than the picture draws it. A container keeps its title at the top and
//! rolls up from below; a plain box keeps its words in its middle. Where
//! the inside is shorter or narrower than the box, its outermost row or
//! column of pixels fills the rest, as a box's own padding would. Only a
//! box is drawn so: a diamond, a circle, a box whose corners bend further
//! than its rim, or one whose boxes reach into its rim, is stretched.

use iced::widget::canvas::{self, Frame};
use iced::widget::image;
use iced::{Point, Rectangle, Size, Vector};

use omaspec_render::pick::Bend;

use super::raster::Raster;

/// Strips thinner than this are the same edge. Wide enough to miss a
/// hairline gap, narrow enough that two boxes still meet.
pub(super) const HAIR: f32 = 0.05;
/// A box's rim, in diagram units, beyond the radius of its corners: the
/// margin it is cut out with (2) and its stroke.
const RIM: f32 = 3.0;
/// A rim reaches this far past the box's sides, so the rows drawn out from
/// just inside it lie clear of its stroke and its blur, and at least a
/// little less where the boxes it holds come close.
const WIDEST_RIM: f32 = 6.0;
const NARROWEST_RIM: f32 = 5.0;
/// How far a box drawn may differ from its size in the picture, as a
/// share or in pixels of the screen (a paper snapped to whole pixels), and
/// the box still count as only scaled.
const SAME_SHAPE: f32 = 0.01;
const SAME_PIXELS: f32 = 1.5;
/// How far two pixels of one color may differ, in each channel.
const SAME_COLOR: u8 = 3;
/// How wide the row or column of pixels is that fills out an inside
/// shorter or narrower than its box, in diagram units.
const STRIP: f32 = 0.5;

/// One picture drawn into `dest`, from diagram units `src`.
pub(super) struct Blit<'a> {
    pub raster: &'a Raster,
    pub src: Rectangle,
    pub dest: Rectangle,
    /// The part of `dest` that is actually painted. The mapping stays `src`
    /// onto `dest`, so a tighter limit does not move the picture.
    pub limit: Rectangle,
    pub alpha: f32,
}

/// One part of a box: its pixels `src`, drawn into `dest`, painted where
/// `limit` is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Piece {
    pub src: Rectangle,
    pub dest: Rectangle,
    pub limit: Rectangle,
    /// It fills room the inside leaves, drawn out from the inside's
    /// outermost row or column.
    pub room: bool,
}

/// How a box's pixels fill a box on screen: its parts, and of them the
/// inside, where the boxes it holds are cut out.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Fit {
    pub pieces: Vec<Piece>,
    /// The index in `pieces` of the inside.
    pub inside: usize,
    /// The box within its rim, in the picture's units, where none of the
    /// boxes it holds reaches: its own color, which fills their holes.
    /// None for a shape that is no box.
    pub interior: Option<Rectangle>,
}

impl Fit {
    pub fn inside(&self) -> Piece {
        self.pieces[self.inside]
    }

    /// `child`, in the box's diagram units, where the inside draws it on
    /// screen; the part of it that is shown.
    pub fn place(&self, child: Rectangle) -> Option<Rectangle> {
        let inside = self.inside();
        map_through(child, inside.src, inside.dest).intersection(&inside.limit)
    }
}

/// How wide the rim is of the box cut out as `src`, whose outline bends
/// as `bend` says and which holds `children` (in the picture's units): as
/// wide as its corners and its stroke, and [`WIDEST_RIM`] past its sides,
/// and narrow enough that the rows drawn out from just inside it lie clear
/// of the boxes it holds. None for a shape that is no box, or a box too
/// small or too full for a rim around its corners and [`NARROWEST_RIM`]
/// past its sides.
pub(super) fn rim(src: Rectangle, bend: Option<Bend>, children: &[Rectangle]) -> Option<f32> {
    let Bend { radius, inset } = bend?;
    let room = children
        .iter()
        .map(|child| {
            (child.x - src.x)
                .min(child.y - src.y)
                .min(src.x + src.width - child.x - child.width)
                .min(src.y + src.height - child.y - child.height)
        })
        .fold(f32::INFINITY, f32::min);
    let rim = (radius + RIM).max(inset + WIDEST_RIM).min(room - STRIP);
    let least = (radius + 1.0).max(inset + NARROWEST_RIM);
    (rim >= least && rim <= src.width / 3.0 && rim <= src.height / 3.0).then_some(rim)
}

/// How the box of pixels `src` fills `dest` on screen, where the picture
/// is drawn at `natural` pixels to a unit and a pixel of the screen is
/// `pixel` wide; without `natural`, it is stretched as the sheet under it
/// is. `bend` is how its outline bends, none for a shape that is no box;
/// `children` are the boxes it holds, in the picture's units. A box
/// holding others is a container: its title stays at the top.
pub(super) fn fit(
    src: Rectangle,
    dest: Rectangle,
    natural: Option<f32>,
    pixel: f32,
    bend: Option<Bend>,
    children: &[Rectangle],
) -> Fit {
    let plain = |interior| Fit {
        pieces: vec![Piece {
            src,
            dest,
            limit: dest,
            room: false,
        }],
        inside: 0,
        interior,
    };
    let Some(rim) = rim(src, bend, children) else {
        return plain(None);
    };
    let interior = Some(inset(src, rim));
    let Some(natural) = natural.filter(|n| *n > 0.0) else {
        return plain(interior);
    };
    let near = |a: f32, b: f32| {
        (a - b).abs() <= (SAME_SHAPE * a.abs().max(b.abs())).max(SAME_PIXELS * pixel)
    };
    if near(dest.width, src.width * natural) && near(dest.height, src.height * natural) {
        return plain(interior);
    }
    // The corners at the picture's size, unless the box is too small for
    // them.
    let rim_scale = natural
        .min(dest.width / (2.0 * rim))
        .min(dest.height / (2.0 * rim));
    let edge = rim * rim_scale;
    let inner = |r: Rectangle, by: f32| (r.width - 2.0 * by, r.height - 2.0 * by);
    let (src_w, src_h) = inner(src, rim);
    let (room_w, room_h) = inner(dest, edge);
    if src_w < 1.0 || src_h < 1.0 || room_w < HAIR || room_h < HAIR {
        return plain(interior);
    }
    let (across, down) = (room_w / src_w, room_h / src_h);
    // A container as wide as there is room, cut short below; a plain box
    // whole, in the middle. Never larger than the picture draws it.
    let holding = !children.is_empty();
    let scale = if holding {
        natural.min(across)
    } else {
        natural.min(across).min(down)
    };
    let xs = axis(
        (src.x, src.x + src.width),
        (dest.x, dest.x + dest.width),
        rim,
        edge,
        scale,
        false,
    );
    let ys = axis(
        (src.y, src.y + src.height),
        (dest.y, dest.y + dest.height),
        rim,
        edge,
        scale,
        holding,
    );
    let mut pieces = Vec::with_capacity(xs.len() * ys.len());
    let mut inside = 0;
    for y in &ys {
        for x in &xs {
            if x.part == Part::Inside && y.part == Part::Inside {
                inside = pieces.len();
            }
            pieces.push(Piece {
                src: span(x.src, y.src),
                dest: span(x.dest, y.dest),
                limit: span(x.limit, y.limit),
                room: x.part != Part::Rim
                    && y.part != Part::Rim
                    && (x.part, y.part) != (Part::Inside, Part::Inside),
            });
        }
    }
    Fit {
        pieces,
        inside,
        interior,
    }
}

pub(super) fn inset(rect: Rectangle, by: f32) -> Rectangle {
    Rectangle::new(
        Point::new(rect.x + by, rect.y + by),
        Size::new(
            (rect.width - 2.0 * by).max(0.0),
            (rect.height - 2.0 * by).max(0.0),
        ),
    )
}

/// One stretch of a box along one axis.
struct Stretch {
    src: (f32, f32),
    dest: (f32, f32),
    limit: (f32, f32),
    part: Part,
}

/// What a stretch of a box along one axis is.
#[derive(Clone, Copy, PartialEq)]
enum Part {
    Rim,
    /// Room the inside leaves, filled from its outermost strip.
    Room,
    Inside,
}

/// A box along one axis: its rim at both ends, the inside at `scale`,
/// at the start of the room left or in its middle, and its outermost
/// strip of pixels drawn out over what the inside leaves free.
fn axis(
    src: (f32, f32),
    dest: (f32, f32),
    rim: f32,
    edge: f32,
    scale: f32,
    at_start: bool,
) -> Vec<Stretch> {
    let (a1, a2) = (src.0 + rim, src.1 - rim);
    let (b1, b2) = (dest.0 + edge, dest.1 - edge);
    let length = (a2 - a1) * scale;
    let from = if at_start {
        b1
    } else {
        b1 + (b2 - b1 - length) / 2.0
    };
    let to = from + length;
    let strip = STRIP.min((a2 - a1) / 2.0);
    let mut out = vec![Stretch {
        src: (src.0, a1),
        dest: (dest.0, b1),
        limit: (dest.0, b1),
        part: Part::Rim,
    }];
    if from > b1 + HAIR {
        out.push(Stretch {
            src: (a1, a1 + strip),
            dest: (b1, from),
            limit: (b1, from),
            part: Part::Room,
        });
    }
    out.push(Stretch {
        src: (a1, a2),
        dest: (from, to),
        limit: (from.max(b1), to.min(b2)),
        part: Part::Inside,
    });
    if to < b2 - HAIR {
        out.push(Stretch {
            src: (a2 - strip, a2),
            dest: (to, b2),
            limit: (to, b2),
            part: Part::Room,
        });
    }
    out.push(Stretch {
        src: (a2, src.1),
        dest: (b2, dest.1),
        limit: (b2, dest.1),
        part: Part::Rim,
    });
    out
}

fn span(x: (f32, f32), y: (f32, f32)) -> Rectangle {
    Rectangle::new(
        Point::new(x.0, y.0),
        Size::new((x.1 - x.0).max(0.0), (y.1 - y.0).max(0.0)),
    )
}

/// A box held by the one drawn: where it is in the picture, and where its
/// own pixels cover it on screen, whole and opaque, when they do.
#[derive(Clone, Copy, Debug)]
pub(super) struct Child {
    pub rect: Rectangle,
    pub covered: Option<Rectangle>,
}

/// The boxes a box holds as its picture is drawn, and how their holes are
/// filled.
pub(super) struct Holes<'a> {
    pub children: &'a [Child],
    /// For each child, a column of the box's own color beside it
    /// ([`columns`]).
    pub columns: &'a [Option<f32>],
    /// The box's own color, for a hole no column fills
    /// ([`Raster::swatch`]).
    pub swatch: Option<&'a Raster>,
}

/// The box `fit` of `raster` at `alpha`, without the boxes it holds:
/// their holes are cut out of its inside, so they glide on their own, and
/// filled with the box's own color, a column of it beside them drawn out
/// across, else its color as the picture names it, so the box stays one
/// color where they were whatever is under it. A hole a child covers
/// whole needs neither. Room the inside leaves is filled with the color
/// too where its outermost row or column crosses words or a line.
pub(super) fn draw(
    frame: &mut Frame,
    local: Rectangle,
    raster: &Raster,
    fit: &Fit,
    holes: &Holes<'_>,
    alpha: f32,
) {
    let placed: Vec<Option<Rectangle>> = holes
        .children
        .iter()
        .map(|child| fit.place(child.rect))
        .collect();
    let cut: Vec<Rectangle> = placed.iter().flatten().copied().collect();
    for (index, piece) in fit.pieces.iter().enumerate() {
        let limit = piece.limit.intersection(&local).unwrap_or_default();
        // Room drawn out from a row or a column that crosses words or a
        // line would draw them out: the box's color fills it instead.
        let crossed = piece.room
            && !one_color(
                raster,
                (piece.src.x, piece.src.x + piece.src.width),
                (piece.src.y, piece.src.y + piece.src.height),
            );
        let blit = match (crossed, holes.swatch) {
            (true, Some(swatch)) => Blit {
                raster: swatch,
                src: inset(swatch.region, swatch.region.width / 4.0),
                dest: piece.limit,
                limit,
                alpha,
            },
            _ => Blit {
                raster,
                src: piece.src,
                dest: piece.dest,
                limit,
                alpha,
            },
        };
        let cut = if index == fit.inside { &cut[..] } else { &[] };
        punched(frame, local, &blit, cut);
    }
    let inside = fit.inside();
    for (nth, (child, hole)) in holes.children.iter().zip(&placed).enumerate() {
        let Some(hole) = *hole else {
            continue;
        };
        // One within another is filled with it.
        let within = holes
            .children
            .iter()
            .any(|other| other.rect != child.rect && holds(other.rect, child.rect));
        let covered = child
            .covered
            .is_some_and(|covered| holds(inset(covered, -1e-3), hole));
        let Some(limit) = hole.intersection(&local) else {
            continue;
        };
        if within || covered {
            continue;
        }
        let blit = match (holes.columns.get(nth).copied().flatten(), holes.swatch) {
            (Some(column), _) => Blit {
                raster,
                src: Rectangle::new(
                    Point::new(column, child.rect.y),
                    Size::new(STRIP, child.rect.height),
                ),
                dest: map_through(child.rect, inside.src, inside.dest),
                limit,
                alpha,
            },
            // The middle of the swatch, which no edge blurs.
            (None, Some(swatch)) => Blit {
                raster: swatch,
                src: inset(swatch.region, swatch.region.width / 4.0),
                dest: hole,
                limit,
                alpha,
            },
            (None, None) => continue,
        };
        punched(frame, local, &blit, &[]);
    }
}

/// For each of `children`, a column of the box's `interior` beside it
/// that is the box's own color ([`plain_column`]); none without an
/// interior. The same for a whole glide, so worked out once.
pub(super) fn columns(
    raster: &Raster,
    interior: Option<Rectangle>,
    children: &[Rectangle],
) -> Vec<Option<f32>> {
    children
        .iter()
        .map(|child| interior.and_then(|interior| plain_column(raster, interior, children, *child)))
        .collect()
}

/// Where a column of the box's `interior` beside `child`, at its left
/// edge or its right, crosses none of the boxes it holds and is one color
/// all down `child`'s rows: the box's own color, and no line or word.
fn plain_column(
    raster: &Raster,
    interior: Rectangle,
    children: &[Rectangle],
    child: Rectangle,
) -> Option<f32> {
    let rows = (
        child.y.max(interior.y),
        (child.y + child.height).min(interior.y + interior.height),
    );
    if rows.1 <= rows.0 {
        return None;
    }
    [interior.x, interior.x + interior.width - STRIP]
        .into_iter()
        .find(|&x| {
            children.iter().all(|other| {
                other.x + other.width <= x
                    || other.x >= x + STRIP
                    || other.y + other.height <= rows.0
                    || other.y >= rows.1
            }) && one_color(raster, (x, x + STRIP), rows)
        })
}

/// Whether `raster` is one color over `columns` and `rows`, in diagram
/// units, and a pixel around them, which a scaled picture blends in.
fn one_color(raster: &Raster, columns: (f32, f32), rows: (f32, f32)) -> bool {
    let image::Handle::Rgba {
        width,
        height,
        pixels,
        ..
    } = &raster.handle
    else {
        return false;
    };
    let region = raster.region;
    if region.width <= 0.0 || region.height <= 0.0 {
        return false;
    }
    let (w, h) = (i64::from(*width), i64::from(*height));
    let column = |x: f32| ((x - region.x) * w as f32 / region.width).floor() as i64;
    let row = |y: f32| ((y - region.y) * h as f32 / region.height).floor() as i64;
    let (left, right) = (column(columns.0) - 1, column(columns.1) + 1);
    let (top, bottom) = (row(rows.0) - 1, row(rows.1 - 1e-3) + 1);
    if left < 0 || right >= w || top < 0 || bottom >= h || bottom < top {
        return false;
    }
    let at = |x: i64, y: i64| {
        let i = ((y * w + x) * 4) as usize;
        pixels.get(i..i + 4)
    };
    let Some(first) = at(left, top) else {
        return false;
    };
    (top..=bottom).all(|y| {
        (left..=right).all(|x| {
            at(x, y).is_some_and(|pixel| {
                pixel
                    .iter()
                    .zip(first)
                    .all(|(a, b)| a.abs_diff(*b) <= SAME_COLOR)
            })
        })
    })
}

/// Whether `outer` holds `inner` whole.
fn holds(outer: Rectangle, inner: Rectangle) -> bool {
    outer.x <= inner.x
        && outer.y <= inner.y
        && inner.x + inner.width <= outer.x + outer.width
        && inner.y + inner.height <= outer.y + outer.height
}

/// `raster`'s `src` drawn into `dest`, with `holes` (in `dest`) left out.
pub(super) fn punched(frame: &mut Frame, local: Rectangle, blit: &Blit<'_>, holes: &[Rectangle]) {
    if blit.alpha <= 0.02
        || blit.src.width < 1e-3
        || blit.src.height < 1e-3
        || blit.dest.width < HAIR
        || blit.dest.height < HAIR
    {
        return;
    }
    let Some(bounds) = blit.dest.intersection(&blit.limit) else {
        return;
    };
    let cuts: Vec<Rectangle> = holes
        .iter()
        .filter_map(|hole| hole.intersection(&bounds))
        .collect();
    if cuts.is_empty() {
        map(frame, local, blit, bounds);
        return;
    }
    let mut ys = vec![bounds.y, bounds.y + bounds.height];
    for cut in &cuts {
        ys.push(cut.y.clamp(bounds.y, bounds.y + bounds.height));
        ys.push((cut.y + cut.height).clamp(bounds.y, bounds.y + bounds.height));
    }
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < HAIR);
    let right = bounds.x + bounds.width;
    for pair in ys.windows(2) {
        let (y0, y1) = (pair[0], pair[1]);
        if y1 - y0 < HAIR {
            continue;
        }
        let mut spans: Vec<(f32, f32)> = cuts
            .iter()
            .filter(|cut| cut.y <= y0 + HAIR && cut.y + cut.height >= y1 - HAIR)
            .map(|cut| (cut.x.max(bounds.x), (cut.x + cut.width).min(right)))
            .filter(|(from, to)| to - from > HAIR)
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut merged: Vec<(f32, f32)> = Vec::new();
        for (from, to) in spans {
            if let Some(last) = merged.last_mut()
                && from <= last.1 + HAIR
            {
                last.1 = last.1.max(to);
            } else {
                merged.push((from, to));
            }
        }
        let mut x = bounds.x;
        for (from, to) in merged {
            if from > x + HAIR {
                map(
                    frame,
                    local,
                    blit,
                    Rectangle::new(Point::new(x, y0), Size::new(from - x, y1 - y0)),
                );
            }
            x = x.max(to);
        }
        if right > x + HAIR {
            map(
                frame,
                local,
                blit,
                Rectangle::new(Point::new(x, y0), Size::new(right - x, y1 - y0)),
            );
        }
    }
}

/// The whole picture placed so that `src` lands on `dest`, painted only
/// inside `clip`.
fn map(frame: &mut Frame, local: Rectangle, blit: &Blit<'_>, clip: Rectangle) {
    if blit.alpha <= 0.02 || clip.width < HAIR || clip.height < HAIR {
        return;
    }
    let Some(clip) = clip.intersection(&local) else {
        return;
    };
    if clip.width < HAIR || clip.height < HAIR {
        return;
    }
    let (kx, ky) = (
        blit.dest.width / blit.src.width,
        blit.dest.height / blit.src.height,
    );
    let region = blit.raster.region;
    let origin = blit.dest.position()
        + Vector::new((region.x - blit.src.x) * kx, (region.y - blit.src.y) * ky);
    let at = Rectangle::new(origin, Size::new(region.width * kx, region.height * ky));
    frame.with_clip(clip, |frame| {
        frame.draw_image(
            at,
            canvas::Image {
                ..canvas::Image::new(blit.raster.handle.clone())
                    .filter_method(image::FilterMethod::Linear)
                    .opacity(blit.alpha)
            },
        );
    });
}

/// `inner`, which lies in `through`, placed where `through` lands on `dest`.
pub(super) fn map_through(inner: Rectangle, through: Rectangle, dest: Rectangle) -> Rectangle {
    let (tw, th) = (through.width.max(1e-3), through.height.max(1e-3));
    let (kx, ky) = (dest.width / tw, dest.height / th);
    Rectangle::new(
        Point::new(
            dest.x + (inner.x - through.x) * kx,
            dest.y + (inner.y - through.y) * ky,
        ),
        Size::new(inner.width * kx, inner.height * ky),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }

    fn close(a: Rectangle, b: Rectangle) -> bool {
        (a.x - b.x).abs() < 1e-3
            && (a.y - b.y).abs() < 1e-3
            && (a.width - b.width).abs() < 1e-3
            && (a.height - b.height).abs() < 1e-3
    }

    /// The parts painted cover the box on screen once, with no gap and no
    /// overlap.
    fn tiles(fit: &Fit, dest: Rectangle) {
        let painted: f32 = fit
            .pieces
            .iter()
            .filter_map(|piece| piece.dest.intersection(&piece.limit))
            .map(|r| r.width * r.height)
            .sum();
        let whole = dest.width * dest.height;
        assert!(
            (painted - whole).abs() < whole * 1e-3,
            "painted {painted} of {whole}: {fit:?}"
        );
        for (i, a) in fit.pieces.iter().enumerate() {
            for b in &fit.pieces[i + 1..] {
                if let Some(both) = a.limit.intersection(&b.limit) {
                    assert!(both.width * both.height < 1e-3, "{a:?} overlaps {b:?}");
                }
            }
        }
    }

    /// The boxes of these tests: their corners rounded by 8.
    const BENT: Bend = Bend {
        radius: 8.0,
        inset: 0.0,
    };

    fn bent_rim() -> f32 {
        BENT.radius + RIM
    }

    fn square(inset: f32) -> Option<Bend> {
        Some(Bend { radius: 0.0, inset })
    }

    fn round(radius: f32) -> Option<Bend> {
        Some(Bend { radius, inset: 0.0 })
    }

    #[test]
    fn a_box_that_keeps_its_shape_is_only_scaled() {
        let src = rect(10.0, 20.0, 100.0, 40.0);
        let dest = rect(5.0, 5.0, 200.0, 80.0);
        let fit = fit(src, dest, Some(2.0), 1.0, Some(BENT), &[]);
        assert_eq!(fit.pieces.len(), 1);
        assert!(close(fit.inside().dest, dest));
        assert!(close(fit.interior.unwrap(), inset(src, bent_rim())));
        // A paper snapped to whole pixels is a pixel off: still the box.
        let snapped = rect(5.0, 5.0, 201.0, 79.0);
        assert_eq!(fit_of(src, snapped).pieces.len(), 1);
    }

    fn fit_of(src: Rectangle, dest: Rectangle) -> Fit {
        fit(src, dest, Some(2.0), 1.0, Some(BENT), &[])
    }

    #[test]
    fn a_box_drawn_taller_keeps_its_words_and_its_rim() {
        // A small box on its way into a tall container's place.
        let src = rect(0.0, 0.0, 150.0, 50.0);
        let dest = rect(0.0, 0.0, 150.0, 200.0);
        let fit = fit(src, dest, Some(1.0), 1.0, Some(BENT), &[]);
        tiles(&fit, dest);
        let inside = fit.inside();
        // Its words as large as the picture draws them, in the middle.
        assert!((inside.dest.width / inside.src.width - 1.0).abs() < 1e-3);
        assert!((inside.dest.height / inside.src.height - 1.0).abs() < 1e-3);
        let middle = inside.dest.center_y();
        assert!((middle - 100.0).abs() < 1e-3, "{inside:?}");
        // Its corners at their size.
        let corner = fit.pieces[0];
        assert!(
            close(corner.dest, rect(0.0, 0.0, bent_rim(), bent_rim())),
            "{corner:?}"
        );
    }

    #[test]
    fn a_container_folding_rolls_up_from_below() {
        // A tall container on its way into a short wide box.
        let src = rect(0.0, 0.0, 110.0, 350.0);
        let dest = rect(0.0, 0.0, 130.0, 120.0);
        let child = rect(20.0, 40.0, 60.0, 30.0);
        let fit = fit(src, dest, Some(1.0), 1.0, Some(BENT), &[child]);
        tiles(&fit, dest);
        let inside = fit.inside();
        // As large as drawn, its title at the top, cut short below.
        assert!((inside.dest.width / inside.src.width - 1.0).abs() < 1e-3);
        assert!((inside.dest.height / inside.src.height - 1.0).abs() < 1e-3);
        assert!((inside.dest.y - bent_rim()).abs() < 1e-3);
        assert!(inside.limit.y + inside.limit.height <= 120.0 - bent_rim() + 1e-3);
        // A child near its top lands as drawn, one below the cut is gone.
        let top = fit.place(child).unwrap();
        assert!(close(top, rect(30.0, 40.0, 60.0, 30.0)), "{top:?}");
        assert!(fit.place(rect(20.0, 300.0, 60.0, 30.0)).is_none());
    }

    #[test]
    fn a_box_drawn_narrower_shrinks_whole() {
        let src = rect(0.0, 0.0, 200.0, 50.0);
        let dest = rect(0.0, 0.0, 100.0, 50.0);
        let fit = fit(src, dest, Some(1.0), 1.0, Some(BENT), &[]);
        tiles(&fit, dest);
        let inside = fit.inside();
        let (kx, ky) = (
            inside.dest.width / inside.src.width,
            inside.dest.height / inside.src.height,
        );
        assert!((kx - ky).abs() < 1e-3 && kx < 1.0, "{inside:?}");
        assert!(close(inside.limit, inside.dest), "not cut: {inside:?}");
    }

    #[test]
    fn a_shape_that_is_no_box_or_holds_boxes_in_its_rim_is_stretched() {
        let src = rect(0.0, 0.0, 110.0, 350.0);
        let dest = rect(0.0, 0.0, 130.0, 120.0);
        // A diamond, a circle: no corners to keep.
        let shape = fit(src, dest, Some(1.0), 1.0, None, &[]);
        assert_eq!(shape.pieces.len(), 1);
        assert!(shape.interior.is_none());
        // Corners that bend further than a third of the box.
        assert_eq!(
            fit(src, dest, Some(1.0), 1.0, round(40.0), &[])
                .pieces
                .len(),
            1
        );
        // A box held at its edge would be drawn out with the rim.
        let lane = rect(4.0, 40.0, 100.0, 30.0);
        let held = fit(src, dest, Some(1.0), 1.0, Some(BENT), &[lane]);
        assert_eq!(held.pieces.len(), 1);
        assert!(held.interior.is_none());
    }

    #[test]
    fn a_hole_is_filled_from_a_plain_column_beside_it() {
        let interior = rect(10.0, 10.0, 100.0, 200.0);
        let child = rect(30.0, 50.0, 60.0, 30.0);
        let white = Raster::blank(120, 220);
        // Nothing else beside it: the left edge of the interior.
        assert_eq!(plain_column(&white, interior, &[child], child), Some(10.0));
        // A box on the left at its height: the right edge.
        let left = rect(5.0, 60.0, 20.0, 10.0);
        assert_eq!(
            plain_column(&white, interior, &[child, left], child),
            Some(110.0 - STRIP)
        );
        // Boxes on both sides: none, and the hole stays as it is.
        let right = rect(100.0, 40.0, 20.0, 20.0);
        assert_eq!(
            plain_column(&white, interior, &[child, left, right], child),
            None
        );
        // A box beside it but above it is no matter.
        let above = rect(5.0, 10.0, 20.0, 20.0);
        assert_eq!(
            plain_column(&white, interior, &[child, above], child),
            Some(10.0)
        );
        // A line across the left column at its rows, as a transition into
        // it draws: the right column, which is plain.
        let crossed = Raster::painted(120, 220, |x, y| {
            if y == 64 && x < 30 {
                [0, 0, 0, 255]
            } else {
                [255; 4]
            }
        });
        assert_eq!(
            plain_column(&crossed, interior, &[child], child),
            Some(110.0 - STRIP)
        );
    }

    #[test]
    fn a_square_box_keeps_a_rim_clear_of_its_stroke() {
        let src = rect(0.0, 0.0, 120.0, 60.0);
        assert_eq!(rim(src, square(0.0), &[]), Some(WIDEST_RIM));
        // Its corners' arc and the stroke, where they need more.
        assert_eq!(rim(src, round(12.5), &[]), Some(15.5));
        // A side that steps in by 4: the rim reaches past the step.
        assert_eq!(rim(src, square(4.0), &[]), Some(4.0 + WIDEST_RIM));
    }

    #[test]
    fn a_rim_narrows_to_the_boxes_a_box_holds() {
        let src = rect(0.0, 0.0, 110.0, 350.0);
        // Corners of 10 want a rim of 13; the boxes held sit 12 in.
        let child = rect(12.0, 40.0, 60.0, 30.0);
        assert_eq!(rim(src, round(10.0), &[child]), Some(11.5));
        let fit = fit(
            src,
            rect(0.0, 0.0, 130.0, 120.0),
            Some(1.0),
            1.0,
            round(10.0),
            &[child],
        );
        assert!(fit.pieces.len() > 1, "drawn in parts: {fit:?}");
        // A box held closer than its corners leaves no rim: stretched.
        let close_in = rect(8.0, 40.0, 60.0, 30.0);
        assert_eq!(rim(src, round(10.0), &[close_in]), None);
    }

    #[test]
    fn a_pixel_off_on_a_dense_screen_is_a_change_of_shape() {
        let src = rect(0.0, 0.0, 100.0, 40.0);
        // 1.4 units taller: within a pixel and a half at scale 1, not at 2.
        let dest = rect(0.0, 0.0, 100.0, 41.4);
        assert_eq!(
            fit(src, dest, Some(1.0), 1.0, Some(BENT), &[]).pieces.len(),
            1
        );
        assert!(fit(src, dest, Some(1.0), 0.5, Some(BENT), &[]).pieces.len() > 1);
    }

    /// A hole beside which no column is plain is filled with the box's
    /// color as the picture names it, drawn as a picture over what lies
    /// under the box.
    #[test]
    fn a_hole_with_no_plain_column_is_filled_with_the_box_s_color() {
        let src = rect(0.0, 0.0, 60.0, 60.0);
        let child = rect(20.0, 20.0, 20.0, 20.0);
        let red = on_tiny_skia(move |frame, local| {
            // A white box whose every row is crossed by a black line, so
            // no column beside the child is one color.
            let picture =
                Raster::painted(
                    60,
                    60,
                    |x, _| {
                        if x % 2 == 0 { [0, 0, 0, 255] } else { [255; 4] }
                    },
                );
            let dest = rect(100.0, 100.0, 60.0, 60.0);
            let fit = fit(src, dest, Some(1.0), 1.0, square(0.0), &[child]);
            let children = [Child {
                rect: child,
                covered: None,
            }];
            let columns = columns(&picture, fit.interior, &[child]);
            assert_eq!(columns, vec![None]);
            let swatch = Raster::swatch([1.0, 0.0, 0.0]);
            let holes = Holes {
                children: &children,
                columns: &columns,
                swatch: Some(&swatch),
            };
            draw(frame, local, &picture, &fit, &holes, 1.0);
        });
        for (x, y) in [(121, 121), (130, 130), (138, 138)] {
            assert_eq!(red[y * 200 + x], 255, "({x}, {y}) is the box's red");
        }
    }

    /// A box's words reach up to its inside's first row: drawn taller, the
    /// room above them is filled with its color, not with that row drawn
    /// out into stripes; the room below, plain, is drawn out as before.
    #[test]
    fn room_whose_row_crosses_words_is_filled_with_the_box_s_color() {
        let red = on_tiny_skia(|frame, local| {
            let picture = Raster::painted(60, 40, |x, y| {
                if (6..9).contains(&y) && (20..40).contains(&x) {
                    [0, 0, 0, 255]
                } else {
                    [255; 4]
                }
            });
            let dest = rect(100.0, 40.0, 60.0, 120.0);
            let fit = fit(picture.region, dest, Some(1.0), 1.0, square(0.0), &[]);
            let swatch = Raster::swatch([0.0, 1.0, 1.0]);
            let holes = Holes {
                children: &[],
                columns: &[],
                swatch: Some(&swatch),
            };
            draw(frame, local, &picture, &fit, &holes, 1.0);
        });
        // Above the words, between them and the rim: the swatch's cyan.
        assert_eq!(red[60 * 200 + 130], 0, "above the words");
        // Below them: the white row drawn out.
        assert_eq!(red[140 * 200 + 130], 255, "below the words");
    }

    /// What `paint` draws on a canvas of 200 by 200 pixels over black, by
    /// iced's tiny-skia, as `omaspec play` draws where there is no GPU:
    /// its pixels' red, row by row.
    fn on_tiny_skia(paint: impl Fn(&mut Frame, Rectangle) + 'static) -> Vec<u8> {
        use iced::advanced::renderer::{Headless, Style};
        use iced::widget::canvas::{Canvas, Geometry, Program};
        use iced::{Color, Element, Length, Renderer, Theme, mouse};

        type Paint = Box<dyn Fn(&mut Frame, Rectangle)>;
        struct Painted(Paint);
        impl Program<()> for Painted {
            type State = ();
            fn draw(
                &self,
                _: &(),
                renderer: &Renderer,
                _: &Theme,
                bounds: Rectangle,
                _: mouse::Cursor,
            ) -> Vec<Geometry> {
                let mut frame = Frame::new(renderer, bounds.size());
                (self.0)(&mut frame, Rectangle::with_size(bounds.size()));
                vec![frame.into_geometry()]
            }
        }

        let mut renderer = iced::futures::executor::block_on(Renderer::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("tiny-skia draws without a GPU");
        let element: Element<'_, (), Theme, Renderer> = Canvas::new(Painted(Box::new(paint)))
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
        let size = Size::new(200.0, 200.0);
        let mut ui =
            iced_runtime::UserInterface::build(element, size, Default::default(), &mut renderer);
        let style = Style {
            text_color: Color::WHITE,
        };
        ui.draw(
            &mut renderer,
            &Theme::Dark,
            &style,
            mouse::Cursor::Unavailable,
        );
        let rgba = renderer.screenshot(iced::Size::new(200, 200), 1.0, Color::BLACK);
        drop(ui);
        rgba.chunks(4).map(|pixel| pixel[0]).collect()
    }

    /// A box's pixels drawn in parts without a GPU: each part keeps to its
    /// own clip, so nothing of the picture lands outside the box.
    /// iced_tiny_skia 0.14.1 kept an image to its layer only, and each part
    /// drew the whole picture, scaled as that part is
    /// (vendor/iced_tiny_skia).
    #[test]
    fn a_box_s_parts_keep_to_the_box_without_a_gpu() {
        let dest = rect(80.0, 40.0, 40.0, 120.0);
        let red = on_tiny_skia(move |frame, local| {
            // A square picture in a box three times as tall: its inside in
            // the middle, its own rows drawn out above and below.
            let picture = Raster::blank(40, 40);
            let fit = fit(picture.region, dest, Some(1.0), 1.0, Some(BENT), &[]);
            assert!(fit.pieces.len() > 9, "{fit:?}");
            let holes = Holes {
                children: &[],
                columns: &[],
                swatch: None,
            };
            draw(frame, local, &picture, &fit, &holes, 1.0);
        });
        for y in 0..200 {
            for x in 0..200 {
                let within = (80..120).contains(&x) && (40..160).contains(&y);
                let seen = red[y * 200 + x];
                if within {
                    assert!(seen > 0xf0, "({x}, {y}) in the box is not drawn");
                } else {
                    assert!(seen < 0x10, "({x}, {y}) outside the box is drawn");
                }
            }
        }
    }

    /// Parts side by side, half seen, meet without a seam: each pixel is
    /// drawn once, by the part its middle lies in, and a part thinner than
    /// that draws none, though tiny-skia fills a rectangle of no width one
    /// pixel wide.
    #[test]
    fn parts_side_by_side_draw_each_pixel_once() {
        let red = on_tiny_skia(|frame, local| {
            let picture = Raster::blank(40, 40);
            let cuts = [80.0, 90.5, 100.2, 100.45, 110.0, 120.0];
            for pair in cuts.windows(2) {
                let blit = Blit {
                    raster: &picture,
                    src: picture.region,
                    dest: rect(80.0, 40.0, 40.0, 40.0),
                    limit: rect(pair[0], 40.0, pair[1] - pair[0], 40.0),
                    alpha: 0.5,
                };
                punched(frame, local, &blit, &[]);
            }
        });
        let row = 60 * 200;
        for x in 80..120 {
            let seen = red[row + x];
            assert!(
                (0x70..=0x90).contains(&seen),
                "column {x}: {seen:#x}, drawn {} times",
                if seen > 0x90 { "two" } else { "no" }
            );
        }
        assert!(red[row + 79] < 0x10 && red[row + 120] < 0x10);
    }
}
