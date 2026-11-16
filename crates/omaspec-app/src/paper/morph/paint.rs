//! A frame of a glide: the two sheets easing from the old picture's size
//! to the new one's, the ground of each fading into the other, and each
//! box drawn from its own part where it is by then ([`super`]). What is
//! the same in every frame is worked out once, from the sheets
//! ([`prepare`]).

use iced::widget::canvas::Frame;
use iced::{Color, Point, Rectangle, Size, Vector};

use super::super::canvas::{self as draw, Grid};
use super::super::raster::Raster;
use super::super::sprite;
use super::super::view::View;
use super::*;

/// What one frame of the glide draws.
pub struct Paint<'a> {
    pub t: f32,
    pub morph: &'a Morph,
    pub old_view: View,
    pub old_size: Size,
    pub old: &'a Raster,
    pub new_view: View,
    pub new_size: Size,
    pub new: &'a Raster,
    pub background: Color,
}

/// Where the sheet and its pictures are for one frame.
struct Placed {
    t: f32,
    /// How large the part of each diagram is that its sheet's ground
    /// covers: as the pane maps a picture onto the screen, so the first
    /// frame lies on the picture before to the pixel, and the last on the
    /// new one.
    old_size: Size,
    new_size: Size,
    /// The sheet, eased from the old paper to the new one.
    paper: Rectangle,
    old_paper: Rectangle,
    new_paper: Rectangle,
    /// `paper` held one device pixel in, so the cleared edge of a picture
    /// does not draw a light line.
    visible: Rectangle,
    /// The pictures' pixels to a unit, eased from the old one's to the
    /// new one's, as a box that stays is drawn: across, and each way.
    natural: f32,
    scale: Vector,
    /// A pixel of the screen, in the pane's units.
    pixel: f32,
    out: f32,
    inn: f32,
}

/// Where the sheet and its pictures are at `paint.t`.
fn placed(grid: Grid, local: Rectangle, paint: &Paint<'_>) -> Placed {
    let t = paint.t.clamp(0.0, 1.0);
    let old_size = paint.old.ground().unwrap_or(paint.old_size);
    let new_size = paint.new.ground().unwrap_or(paint.new_size);
    let old_paper = grid.snap(paint.old_view.diagram_rect(old_size, local));
    let new_paper = grid.snap(paint.new_view.diagram_rect(new_size, local));
    // Lerped once. Snapping again would step the sheet by a pixel.
    let paper = lerp_rect(old_paper, new_paper, t);
    Placed {
        t,
        old_size,
        new_size,
        paper,
        old_paper,
        new_paper,
        visible: inset_px(paper, 1.0 / grid.scale),
        natural: lerp(
            old_paper.width / old_size.width.max(1e-3),
            new_paper.width / new_size.width.max(1e-3),
            t,
        ),
        scale: Vector::new(
            lerp(
                old_paper.width / old_size.width.max(1e-3),
                new_paper.width / new_size.width.max(1e-3),
                t,
            ),
            lerp(
                old_paper.height / old_size.height.max(1e-3),
                new_paper.height / new_size.height.max(1e-3),
                t,
            ),
        ),
        pixel: 1.0 / grid.scale,
        out: line_out(t),
        inn: line_in(t),
    }
}

/// Where the lines of the glide go ([`super::super::wires`]), for the layer that
/// draws them over the pictures: iced draws the pictures of a layer over
/// its shapes, so they cannot share one.
pub fn sheets<'a>(
    grid: Grid,
    local: Rectangle,
    paint: &Paint<'a>,
) -> super::super::wires::Sheets<'a> {
    let placed = placed(grid, local, paint);
    super::super::wires::Sheets {
        pictures: (paint.old, paint.new),
        t: placed.t,
        out: placed.out,
        inn: placed.inn,
        old: (placed.old_paper, placed.old_size),
        new: (placed.new_paper, placed.new_size),
        paper: placed.paper,
        paper_color: sheet_color(paint.old, paint.new, placed.t).unwrap_or(paint.background),
    }
}

/// Draws the glide at `t` (0 is the picture before, 1 the new one).
pub fn paint(frame: &mut Frame, grid: Grid, local: Rectangle, paint: &Paint<'_>) {
    let placed = placed(grid, local, paint);
    let (t, paper) = (placed.t, placed.paper);
    draw::shadow(frame, paper, paint.background, grid.scale, 1.0);
    if let Some(fill) = sheet_color(paint.old, paint.new, t) {
        frame.fill(&draw::rounded(paper, draw::RADIUS), fill);
    }
    paint_ground(frame, local, paint, true, &placed);
    paint_ground(frame, local, paint, false, &placed);
    paint_kept(frame, local, paint, &placed, None);
    for index in 0..paint.morph.moves.len() {
        paint_box(frame, local, paint, index, &placed);
        paint_kept(frame, local, paint, &placed, Some(index));
        paint_orphans(frame, local, paint, true, &placed, Some(index));
        paint_orphans(frame, local, paint, false, &placed, Some(index));
    }
}

fn sheet_color(old: &Raster, new: &Raster, t: f32) -> Option<Color> {
    let (from, to) = (old.paper.a > 0.0, new.paper.a > 0.0);
    match (from, to) {
        (true, true) => Some(Color::from_rgb(
            lerp(old.paper.r, new.paper.r, t),
            lerp(old.paper.g, new.paper.g, t),
            lerp(old.paper.b, new.paper.b, t),
        )),
        (true, false) => Some(Color {
            a: 1.0,
            ..old.paper
        }),
        (false, true) => Some(Color {
            a: 1.0,
            ..new.paper
        }),
        (false, false) => None,
    }
}

fn inset_px(rect: Rectangle, px: f32) -> Rectangle {
    let pad = px.max(0.0);
    Rectangle::new(
        Point::new(rect.x + pad, rect.y + pad),
        Size::new(
            (rect.width - pad * 2.0).max(0.0),
            (rect.height - pad * 2.0).max(0.0),
        ),
    )
}

/// The ground of the picture before or of the new one, without its lines
/// and its parts, at `alpha` on the easing sheet; with the parts of that
/// picture alone that nothing glides ([`Morph::orphans`]).
fn paint_ground(
    frame: &mut Frame,
    local: Rectangle,
    paint: &Paint<'_>,
    old: bool,
    placed: &Placed,
) {
    let alpha = if old { placed.out } else { placed.inn };
    if alpha <= 0.02 || placed.paper.width <= 1.0 || placed.paper.height <= 1.0 {
        return;
    }
    let (raster, size) = if old {
        (paint.old, placed.old_size)
    } else {
        (paint.new, placed.new_size)
    };
    sprite::blit(
        frame,
        local,
        &sprite::Blit {
            raster,
            src: Rectangle::with_size(size),
            dest: placed.paper,
            limit: placed.visible,
            alpha,
        },
    );
    paint_orphans(frame, local, paint, old, placed, None);
}

/// The parts of the picture before or of the new one alone that lie
/// `over` the box of that index, or on the ground ([`Morph::orphans`]):
/// where that picture has them, at its strength.
fn paint_orphans(
    frame: &mut Frame,
    local: Rectangle,
    paint: &Paint<'_>,
    old: bool,
    placed: &Placed,
    over: Option<usize>,
) {
    let (raster, size, alpha) = if old {
        (paint.old, placed.old_size, placed.out)
    } else {
        (paint.new, placed.new_size, placed.inn)
    };
    if alpha <= 0.02 {
        return;
    }
    let orphans = lying_over(&paint.morph.orphans[usize::from(!old)], over, |o| o.over);
    for orphan in orphans {
        let Some((part, at)) = raster.part(&orphan.name) else {
            continue;
        };
        let dest = onto(at, size, placed.paper);
        let Some(limit) = dest.intersection(&placed.visible) else {
            continue;
        };
        let blit = sprite::Blit {
            raster: &part,
            src: at,
            dest,
            limit,
            alpha,
        };
        sprite::blit(frame, local, &blit);
    }
}

fn paint_box(
    frame: &mut Frame,
    local: Rectangle,
    paint: &Paint<'_>,
    index: usize,
    placed: &Placed,
) {
    for picture in box_pictures(paint, index, placed) {
        let sheet = if picture.old { paint.old } else { paint.new };
        let swatch = picture.swatch.map(|patch| (sheet, patch));
        sprite::draw(
            frame,
            local,
            &picture.part,
            &picture.fit,
            swatch,
            picture.alpha,
        );
    }
}

/// One picture of a box in a frame: the part it is drawn from, how its
/// pieces fill the box's place of the moment, and how strongly.
pub(in crate::paper) struct BoxPicture {
    pub part: Raster,
    /// Where the box is in this frame.
    #[cfg(test)]
    pub dest: Rectangle,
    pub fit: sprite::Fit,
    /// Where on its sheet the patch of its own color lies.
    pub swatch: Option<Rectangle>,
    /// The picture before, not the new one.
    pub old: bool,
    pub alpha: f32,
}

/// The pictures box `index` is drawn from in a frame: the old one and the
/// new one over it, each from its own part. A box that stays is opaque
/// the whole way: the old pixels first, then the new ones over them;
/// drawing both at half would wash the box out. One whose new pixels are
/// another part's, as a JSON card that folds into a row of the card
/// above, fades out over that part. One drawn at its size ([`anchored`])
/// whose new picture does not cover the old one fades out too, so nothing
/// of it is left at the end.
fn box_pictures(paint: &Paint<'_>, index: usize, placed: &Placed) -> Vec<BoxPicture> {
    let step = &paint.morph.moves[index];
    let dest = sprite_dest(step, placed);
    if dest.width < 1.0 || dest.height < 1.0 {
        return Vec::new();
    }
    let keeps = !anchored(paint, step) || step.looks.solid || same_size(step.from, step.to);
    let old_a = match step.role {
        Role::Leave => placed.out,
        Role::Stay if placed.t < 1.0 && drawn(paint, step, false) && keeps => 1.0,
        Role::Stay if placed.t < 1.0 => 1.0 - placed.t,
        Role::Stay | Role::Arrive => 0.0,
    };
    let new_a = match step.role {
        Role::Arrive => placed.inn,
        Role::Stay => placed.t,
        Role::Leave => 0.0,
    };
    [(true, old_a), (false, new_a)]
        .into_iter()
        .filter(|(_, alpha)| *alpha > 0.02)
        .filter_map(|(old, alpha)| box_picture(paint, index, placed, dest, old, alpha))
        .collect()
}

/// Whether `step`, a box whose rows are laid from its top left
/// ([`Move::anchored`]), is drawn from a part of its own in both pictures.
fn anchored(paint: &Paint<'_>, step: &Move) -> bool {
    step.anchored && paint.old.has_part(&step.parts[0]) && paint.new.has_part(&step.parts[1])
}

/// Whether two places of a box, in their pictures' units, are as large.
fn same_size(a: Rectangle, b: Rectangle) -> bool {
    (a.width - b.width).abs() < 0.5 && (a.height - b.height).abs() < 0.5
}

/// Whether a side of `step` is drawn on its own, from its part. A box
/// without a part of its own is in the ground or in the part of the box
/// around it, and goes with that (docs/adr/0029).
fn drawn(paint: &Paint<'_>, step: &Move, old: bool) -> bool {
    let raster = if old { paint.old } else { paint.new };
    raster.has_part(&step.parts[usize::from(!old)])
}

/// The box's picture before, or the new one, in `dest`: the picture of its
/// own part, which holds nothing of another box and none of its lines'
/// words, shaped to `dest` ([`sprite::fit`]). One that stays, or glides
/// into the box it hung below, changes its shape on the way, its rim kept;
/// one that only goes or comes stretches with the sheet. Both pictures
/// keep to the box's own rectangle, so their rims meet.
fn box_picture(
    paint: &Paint<'_>,
    index: usize,
    placed: &Placed,
    dest: Rectangle,
    old: bool,
    alpha: f32,
) -> Option<BoxPicture> {
    let step = &paint.morph.moves[index];
    let sheet = if old { paint.old } else { paint.new };
    let (part, _) = sheet.part(&step.parts[usize::from(!old)])?;
    let side = usize::from(!old);
    let (size, src) = if old {
        (placed.old_size, grow(step.from))
    } else {
        (placed.new_size, grow(step.to))
    };
    let looks = &step.looks;
    // The eased sheet's scale for that picture, each way.
    let sheet_scale = Vector::new(
        placed.paper.width / size.width.max(1e-3),
        placed.paper.height / size.height.max(1e-3),
    );
    let scale = match step.role {
        Role::Stay => Some(placed.scale),
        _ if step.into.is_some() => Some(sheet_scale),
        _ => None,
    };
    let natural = match step.role {
        Role::Stay => Some(placed.natural),
        _ => scale.map(|scale| scale.x.min(scale.y)),
    };
    // Rows laid from the top left stay there, the box at its size within
    // its place of the moment, while it is a box in both pictures (not a
    // card that folds into a row, which shrinks into it). A container
    // keeps its title at the top.
    let fit = match scale {
        Some(scale) if anchored(paint, step) => sprite::anchored(src, dest, scale),
        _ => sprite::fit(
            src,
            dest,
            natural,
            placed.pixel,
            looks.bend[side],
            &looks.held[side],
            !looks.held[0].is_empty() || !looks.held[1].is_empty(),
        ),
    };
    Some(BoxPicture {
        part,
        #[cfg(test)]
        dest,
        fit,
        // The box's own color, from the patch of it on the same sheet.
        swatch: looks.fill[side].and_then(|fill| sheet.swatch(fill)),
        old,
        alpha,
    })
}

/// Every picture of a box a frame at `paint.t` draws, as [`paint`] draws
/// them: for the promises of a glide to be held against.
#[cfg(test)]
pub(in crate::paper) fn box_pictures_of(
    grid: Grid,
    local: Rectangle,
    paint: &Paint<'_>,
) -> Vec<BoxPicture> {
    let placed = placed(grid, local, paint);
    (0..paint.morph.moves.len())
        .flat_map(|index| box_pictures(paint, index, &placed))
        .collect()
}

/// Works out once, for a glide drawn from the sheet `old` to the sheet
/// `new`, what is the same in every frame: how each box's own outline is
/// filled and bends, which parts no box and no line's words take, and
/// which of them are in both pictures.
pub fn prepare(
    morph: &mut Morph,
    (old, new): (&Raster, &Raster),
    maps: [&omaspec_uml::map::Map; 2],
    lines: Option<&super::super::wires::Glide>,
) {
    let held: Vec<[Vec<Rectangle>; 2]> = (0..morph.moves.len())
        .map(|index| {
            [
                held(&morph.moves, index, true),
                held(&morph.moves, index, false),
            ]
        })
        .collect();
    for (step, held) in morph.moves.iter_mut().zip(held) {
        let [from_looks, to_looks] = [
            old.outline(&step.parts[0], step.from),
            new.outline(&step.parts[1], step.to),
        ];
        step.looks = Looks {
            fill: [from_looks.0, to_looks.0],
            bend: [from_looks.1, to_looks.1],
            held,
            solid: step.anchored && covers(new, &step.parts[1]),
        };
    }
    let mut names: [Vec<String>; 2] = Default::default();
    for (side, raster) in [(0, old), (1, new)] {
        let Some(parts) = &raster.parts else {
            continue;
        };
        let taken: std::collections::HashSet<&str> = morph
            .moves
            .iter()
            .map(|step| step.parts[side].as_str())
            .chain(
                lines
                    .iter()
                    .flat_map(|glide| &glide.lines)
                    .flat_map(|line| &line.words[side])
                    .filter_map(Option::as_deref),
            )
            .collect();
        let mut orphans: Vec<String> = parts
            .parts
            .keys()
            .filter(|name| !taken.contains(name.as_str()))
            .cloned()
            .collect();
        orphans.sort();
        names[side] = orphans;
    }
    // A part in both pictures that no box takes glides on its own.
    let [old_names, new_names] = &mut names;
    let kept: Vec<String> = old_names
        .iter()
        .filter(|name| new_names.binary_search(name).is_ok())
        .cloned()
        .collect();
    old_names.retain(|name| kept.binary_search(name).is_err());
    new_names.retain(|name| kept.binary_search(name).is_err());
    let moves = &morph.moves;
    // By the box each lies over, so a frame draws those of a box at once.
    for (side, names) in names.into_iter().enumerate() {
        let mut orphans: Vec<Loose> = names
            .into_iter()
            .map(|name| Loose {
                over: lies_in(moves, maps[side], side, &name),
                name,
            })
            .collect();
        orphans.sort_by_key(|orphan| orphan.over);
        morph.orphans[side] = orphans;
    }
    morph.kept = kept
        .into_iter()
        .map(|name| Kept {
            solid: covers(new, &name),
            over: lies_in(moves, maps[1], 1, &name).or_else(|| lies_in(moves, maps[0], 0, &name)),
            name,
        })
        .collect();
    morph.kept.sort_by_key(|kept| kept.over);
}

/// The ones of `parts`, sorted by the box they lie over, that lie `over`
/// that one.
fn lying_over<T>(parts: &[T], over: Option<usize>, of: impl Fn(&T) -> Option<usize>) -> &[T] {
    let start = parts.partition_point(|part| of(part) < over);
    let end = parts.partition_point(|part| of(part) <= over);
    &parts[start..end]
}

/// The move whose box, in the picture before (`side` 0) or the new one,
/// is the one `map` says the part `name` lies inside
/// ([`omaspec_uml::map::Map::inside`]).
fn lies_in(moves: &[Move], map: &omaspec_uml::map::Map, side: usize, name: &str) -> Option<usize> {
    let (_, owner) = map.inside.iter().find(|(part, _)| part == name)?;
    let there = |step: &Move| match side {
        0 => step.role != Role::Arrive,
        _ => step.role != Role::Leave,
    };
    moves
        .iter()
        .position(|step| there(step) && step.parts[side] == *owner)
}

/// The parts in both pictures that no box takes ([`Morph::kept`]), each
/// from its old place to its new one. Each picture of it is as large as
/// that picture draws it, from the top left of its place of the moment
/// and within it, so the rows of a card that changes size stay where they
/// are. The old pixels stay opaque where the new ones will cover them (a
/// card, or one that keeps its size), the new ones come over them; words
/// that change fade from the old to the new. Those `over` the box of that
/// index, right after it ([`Kept::over`]); the rest under every box, so a
/// card that holds others is under them.
fn paint_kept(
    frame: &mut Frame,
    local: Rectangle,
    paint: &Paint<'_>,
    placed: &Placed,
    over: Option<usize>,
) {
    for kept in lying_over(&paint.morph.kept, over, |kept| kept.over) {
        let (Some((old, was)), Some((new, is))) =
            (paint.old.part(&kept.name), paint.new.part(&kept.name))
        else {
            continue;
        };
        let dest = lerp_rect(
            onto(was, placed.old_size, placed.old_paper),
            onto(is, placed.new_size, placed.new_paper),
            placed.t,
        );
        if dest.width < 1.0 || dest.height < 1.0 {
            continue;
        }
        let old_alpha = match placed.t {
            t if t >= 1.0 => 0.0,
            _ if kept.solid || same_size(was, is) => 1.0,
            t => 1.0 - t,
        };
        for (part, src, alpha) in [(&old, was, old_alpha), (&new, is, placed.t)] {
            if alpha <= 0.02 {
                continue;
            }
            // On the paper, as the orphans are.
            let mut fit = sprite::anchored(src, dest, placed.scale);
            let Some(limit) = fit.pieces[0].limit.intersection(&placed.visible) else {
                continue;
            };
            fit.pieces[0].limit = limit;
            sprite::draw(frame, local, part, &fit, None, alpha);
        }
    }
}

/// Whether the part called `name` of `raster` covers its place: the
/// inside of a card with a fill is opaque, words and lines on clear pixels
/// are not. Its rim and its corners are left out of the count.
fn covers(raster: &Raster, name: &str) -> bool {
    raster.part(name).is_some_and(|(part, at)| {
        let inner = Rectangle::new(
            Point::new(at.x + at.width * 0.2, at.y + at.height * 0.2),
            Size::new(at.width * 0.6, at.height * 0.6),
        );
        sprite::opaque(&part, inner)
    })
}

/// Where this box is drawn. One that stays eases from its old place on the
/// old sheet to its new place on the new sheet. One that leaves or arrives
/// stays on the picture it belongs to, so it scales with that picture.
fn sprite_dest(step: &Move, placed: &Placed) -> Rectangle {
    match step.role {
        Role::Stay => lerp_rect(
            onto(grow(step.from), placed.old_size, placed.old_paper),
            onto(grow(step.to), placed.new_size, placed.new_paper),
            placed.t,
        ),
        Role::Leave => {
            let own = onto(grow(step.from), placed.old_size, placed.paper);
            // Into its box, while it fades.
            match parent_dest(step, placed) {
                Some(parent) => {
                    let gone = (placed.t / LINE_OUT).clamp(0.0, 1.0);
                    lerp_rect(own, parent, smooth(gone))
                }
                None => own,
            }
        }
        Role::Arrive => {
            let own = onto(grow(step.to), placed.new_size, placed.paper);
            // Out of its box, while it comes.
            match parent_dest(step, placed) {
                Some(parent) => {
                    let come = ((placed.t - LINE_IN) / (1.0 - LINE_IN)).clamp(0.0, 1.0);
                    lerp_rect(parent, own, smooth(come))
                }
                None => own,
            }
        }
    }
}

/// Where the box `step` hangs below is drawn now, if it is a box of a
/// tree ([`gather`]).
fn parent_dest(step: &Move, placed: &Placed) -> Option<Rectangle> {
    let (from, to) = step.into?;
    Some(lerp_rect(
        onto(grow(from), placed.old_size, placed.old_paper),
        onto(grow(to), placed.new_size, placed.new_paper),
        placed.t,
    ))
}

/// The boxes inside box `index` in the picture before or the new one: the
/// place of each, with its stroke.
fn held(moves: &[Move], index: usize, old: bool) -> Vec<Rectangle> {
    let missing =
        |step: &Move| (old && step.role == Role::Arrive) || (!old && step.role == Role::Leave);
    let step = &moves[index];
    if missing(step) {
        return Vec::new();
    }
    let rect = |step: &Move| if old { step.from } else { step.to };
    let parent = rect(step);
    moves
        .iter()
        .enumerate()
        .filter(|(other_i, other)| *other_i != index && !missing(other))
        .filter(|(_, other)| inside(rect(other), parent))
        .map(|(_, other)| grow(rect(other)))
        .collect()
}

fn grow(rect: Rectangle) -> Rectangle {
    Rectangle::new(
        Point::new(rect.x - RIM, rect.y - RIM),
        Size::new(rect.width + RIM * 2.0, rect.height + RIM * 2.0),
    )
}

/// `rect` of a diagram of `size`, placed on `paper`.
fn onto(rect: Rectangle, size: Size, paper: Rectangle) -> Rectangle {
    sprite::map_through(rect, Rectangle::with_size(size), paper)
}

/// Old lines leave over the first part of the glide. New ones come in over
/// the last part. Both are still for a moment in between.
fn line_out(t: f32) -> f32 {
    if t >= LINE_OUT {
        0.0
    } else {
        1.0 - smooth(t / LINE_OUT)
    }
}

fn line_in(t: f32) -> f32 {
    if t <= LINE_IN {
        0.0
    } else {
        smooth((t - LINE_IN) / (1.0 - LINE_IN))
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

fn lerp_rect(from: Rectangle, to: Rectangle, t: f32) -> Rectangle {
    Rectangle::new(
        Point::new(lerp(from.x, to.x, t), lerp(from.y, to.y, t)),
        Size::new(
            lerp(from.width, to.width, t),
            lerp(from.height, to.height, t),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_leave_before_the_boxes_arrive() {
        assert!((line_out(0.0) - 1.0).abs() < 0.01);
        assert!(line_in(0.0).abs() < 0.01);
        assert_eq!(line_out(0.5), 0.0);
        assert_eq!(line_in(0.5), 0.0);
        assert!(line_out(1.0).abs() < 0.01);
        assert!((line_in(1.0) - 1.0).abs() < 0.01);
        assert!(line_out(0.25) > 0.2);
        assert!(line_in(0.25).abs() < 0.01);
    }

    /// A box drawn on the easing sheet is where the picture before draws
    /// it at the start and where the new one does at the end.
    #[test]
    fn a_sprite_lands_on_its_places_at_both_ends() {
        let old_size = Size::new(400.0, 300.0);
        let new_size = Size::new(500.0, 360.0);
        let from = Rectangle::new(Point::new(10.0, 20.0), Size::new(80.0, 40.0));
        let to = Rectangle::new(Point::new(40.0, 30.0), Size::new(90.0, 50.0));
        let old_paper = Rectangle::new(Point::new(8.0, 12.0), Size::new(200.0, 150.0));
        let new_paper = Rectangle::new(Point::new(4.0, 6.0), Size::new(240.0, 180.0));
        let from_at = onto(grow(from), old_size, old_paper);
        let to_at = onto(grow(to), new_size, new_paper);
        close(lerp_rect(from_at, to_at, 0.0), from_at);
        let paper_end = lerp_rect(old_paper, new_paper, 1.0);
        close(
            lerp_rect(from_at, to_at, 1.0),
            onto(grow(to), new_size, paper_end),
        );
    }

    fn close(a: Rectangle, b: Rectangle) {
        assert!(
            (a.x - b.x).abs() < 1e-3
                && (a.y - b.y).abs() < 1e-3
                && (a.width - b.width).abs() < 1e-3
                && (a.height - b.height).abs() < 1e-3,
            "{a:?} != {b:?}"
        );
    }
}
