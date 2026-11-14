//! The boxes of a new picture glide to their places.
//!
//! A render that only recolors the same layout fades, as before. One that
//! moves a box waits until the new picture is drawn, then the sheet eases
//! from the old size to the new one and each box travels, as its own
//! pixels, from where it was to where it is. A box inside another is not
//! carried along baked into it. The lines glide along ([`super::wires`]),
//! drawn over the two pictures without their lines; where a picture has
//! no such twin, the lines leave with the old picture and come back on the
//! new routes once the boxes are nearly there. The first frame is the old
//! picture and the last frame is the new one, so nothing pops at either
//! end.

use iced::widget::canvas::Frame;
use iced::{Color, Point, Rectangle, Size};

use omaspec_render::pick;

use super::Diagram;
use super::canvas::{self as draw, Grid};
use super::raster::Raster;
use super::sprite;
use super::view::View;

/// One named box of a picture, in that picture's units.
#[derive(Clone, Debug)]
pub struct Side {
    pub name: String,
    pub rect: Rectangle,
    /// Its fill, when the picture names one plain color.
    pub fill: Option<[f32; 3]>,
    /// How its outline bends; none for a shape that is no box
    /// ([`pick::Fills::bend`]).
    pub bend: Option<pick::Bend>,
    /// The part it is drawn in, where the picture marks its parts
    /// (docs/adr/0029): a box's name, a mark's place.
    pub part: String,
}

/// Whether a box was already there, appears, or goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Stay,
    Arrive,
    Leave,
}

/// One box on its way across the pane.
#[derive(Clone, Debug)]
pub struct Move {
    pub name: String,
    pub from: Rectangle,
    pub to: Rectangle,
    pub from_fill: Option<[f32; 3]>,
    pub to_fill: Option<[f32; 3]>,
    /// How its outline bends before and after; none for a shape that is
    /// no box.
    pub from_bend: Option<pick::Bend>,
    pub to_bend: Option<pick::Bend>,
    /// For each box it holds before and after, a column of its own color
    /// beside it ([`sprite::columns`]), worked out once a glide
    /// ([`prepare`]).
    pub columns: [Vec<Option<f32>>; 2],
    /// The part it is drawn in, before and after ([`Side::part`]).
    pub parts: [String; 2],
    /// Share of the box's height that is its title band. `1` is the whole
    /// box: nothing sits inside it.
    pub from_header: f32,
    pub role: Role,
    /// The box of a tree it hangs below, which stays: where that one was
    /// and where it is. A box that leaves glides into it, one that
    /// arrives comes out of it ([`gather`]).
    pub into: Option<(Rectangle, Rectangle)>,
}

/// The boxes of one glide.
#[derive(Clone, Debug)]
pub struct Morph {
    pub moves: Vec<Move>,
}

/// How far a box may sit, as a share of the picture, and still count as
/// unmoved. A recolor stays under it. A fold does not.
const SHIFT: f32 = 0.012;
/// How far past a box's edge its stroke can reach, in diagram units. The
/// hole and the travelling picture use the same rim, so the stroke goes
/// with the box and the first frame still matches the picture exactly.
const RIM: f32 = 2.0;
/// Old lines have gone by `LINE_OUT`. New ones begin at `LINE_IN`. The
/// quiet between them is short, so it reads as one change, and the lines
/// never cross.
const LINE_OUT: f32 = 0.45;
const LINE_IN: f32 = 0.55;

/// The named boxes of `diagram`, in the picture's units. A start or an end
/// travels too: it would otherwise vanish with the lines and pop back.
pub fn sides(diagram: &Diagram) -> Vec<Side> {
    // The picture is read once for every box, not once per box.
    let fills = pick::fills(&diagram.drawn.tree);
    let mut out: Vec<Side> = diagram
        .drawn
        .map
        .places
        .iter()
        .filter(|place| !place.name.is_empty() && !place.name.starts_with('['))
        .map(|place| side_at(&fills, &place.name, &place.name, place.rect))
        .collect();
    push_marks(
        diagram,
        &fills,
        omaspec_uml::map::Shape::Start,
        "start",
        &mut out,
    );
    push_marks(
        diagram,
        &fills,
        omaspec_uml::map::Shape::End,
        "end",
        &mut out,
    );
    out
}

fn side_at(fills: &pick::Fills, name: &str, part: &str, rect: omaspec_uml::graph::Rect) -> Side {
    Side {
        name: name.to_owned(),
        part: part.to_owned(),
        rect: Rectangle::new(Point::new(rect.x, rect.y), Size::new(rect.w, rect.h)),
        fill: fills.of(rect.x, rect.y, rect.w, rect.h),
        bend: fills.bend(rect.x, rect.y, rect.w, rect.h),
    }
}

/// Starts, or ends, each named by the state it sits in and its place
/// among that state's, so the same dot matches next time: a start that
/// folds away with its state is never taken for one beside it.
fn push_marks(
    diagram: &Diagram,
    fills: &pick::Fills,
    shape: omaspec_uml::map::Shape,
    label: &str,
    out: &mut Vec<Side>,
) {
    let marks: Vec<_> = diagram
        .drawn
        .map
        .states
        .iter()
        .filter(|spot| spot.shape == shape && spot.rect.w >= 1.0 && spot.rect.h >= 1.0)
        .collect();
    let names = mark_names(
        label,
        &marks
            .iter()
            .map(|spot| (spot.parent.as_deref(), spot.rect))
            .collect::<Vec<_>>(),
    );
    for (spot, name) in marks.iter().zip(names) {
        out.push(side_at(fills, &name, &spot.part, spot.rect));
    }
}

/// The names of marks, each given by the state it sits in (none at the
/// top) and where it is: `start:Brewing:0` is the first start in Brewing,
/// from the top, then from the left.
fn mark_names(label: &str, marks: &[(Option<&str>, omaspec_uml::graph::Rect)]) -> Vec<String> {
    let mut order: Vec<usize> = (0..marks.len()).collect();
    order.sort_by(|&a, &b| {
        let (a, b) = (marks[a].1, marks[b].1);
        a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x))
    });
    let mut names = vec![String::new(); marks.len()];
    let mut counted: std::collections::HashMap<Option<&str>, usize> = Default::default();
    for at in order {
        let parent = marks[at].0;
        let nth = counted.entry(parent).or_default();
        names[at] = format!("{label}:{}:{nth}", parent.unwrap_or(""));
        *nth += 1;
    }
    names
}

/// The glide from `old` to `new`, when a box changed place. `None` when
/// the layout is the same one, so a recolor keeps the short fade.
#[cfg(test)]
pub fn plan(old: &[Side], new: &[Side], old_size: Size, new_size: Size) -> Option<Morph> {
    plan_with(old, new, old_size, new_size, false)
}

/// [`plan`]; with `folding`, a glide also when boxes only went or came
/// and none of the others moved: a branch of a tree folded on the side,
/// whose boxes still go into the box above them ([`gather`]).
pub fn plan_with(
    old: &[Side],
    new: &[Side],
    old_size: Size,
    new_size: Size,
    folding: bool,
) -> Option<Morph> {
    let old_header = headers(old);
    let new_header = headers(new);
    let mut stays = Vec::new();
    let mut used_new = vec![false; new.len()];
    let mut leaves = Vec::new();
    for (index, from) in old.iter().enumerate() {
        let Some(to_at) = new
            .iter()
            .enumerate()
            .position(|(nth, to)| !used_new[nth] && to.name == from.name)
        else {
            leaves.push(leave(from, old_header[index]));
            continue;
        };
        used_new[to_at] = true;
        let to = &new[to_at];
        stays.push(stay(from, to, old_header[index]));
    }
    // A name the writer is still typing (`Pla` becoming `Playing`) is the
    // same box. Matching it as gone and new would pop it out and in.
    let mut still_leaving = Vec::new();
    for step in leaves {
        let Some(to_at) = typed_partner(&step, new, &used_new, old_size, new_size) else {
            still_leaving.push(step);
            continue;
        };
        used_new[to_at] = true;
        stays.push(stay_move(&step, &new[to_at]));
    }
    let mut moves = stays;
    moves.extend(still_leaving);
    for (index, to) in new.iter().enumerate() {
        if used_new[index] {
            continue;
        }
        moves.push(Move {
            name: to.name.clone(),
            from: to.rect,
            to: to.rect,
            from_fill: to.fill,
            to_fill: to.fill,
            from_bend: to.bend,
            to_bend: to.bend,
            columns: Default::default(),
            parts: [to.part.clone(), to.part.clone()],
            from_header: new_header[index],
            role: Role::Arrive,
            into: None,
        });
    }
    let moved = moves
        .iter()
        .any(|step| step.role == Role::Stay && shifted(step.from, step.to, old_size, new_size));
    let changed = folding && moves.iter().any(|step| step.role != Role::Stay);
    if !moved && !changed {
        return None;
    }
    moves.sort_by(|a, b| {
        area(b)
            .total_cmp(&area(a))
            .then_with(|| a.name.cmp(&b.name))
    });
    Some(Morph { moves })
}

/// Lets the boxes of a tree that leave glide into the box they hung below,
/// and those that arrive come out of it: a branch folds into its box and
/// unfolds out of it. The links of `old` and `new` say what hung where; a
/// box whose own parent went too takes the nearest one above that stays.
pub fn gather(morph: &mut Morph, old: &omaspec_uml::map::Map, new: &omaspec_uml::map::Map) {
    let stays: Vec<(String, Rectangle, Rectangle)> = morph
        .moves
        .iter()
        .filter(|step| step.role == Role::Stay)
        .map(|step| (step.name.clone(), step.from, step.to))
        .collect();
    for step in &mut morph.moves {
        let map = match step.role {
            Role::Leave => old,
            Role::Arrive => new,
            Role::Stay => continue,
        };
        let fixed = |name: &str| map.places.iter().any(|p| p.fixed && p.name == name);
        if !fixed(&step.name) {
            continue;
        }
        let mut name = step.name.as_str();
        // No tree is deeper than its boxes are many.
        for _ in 0..map.wires.len() {
            let Some(wire) = map.wires.iter().find(|w| w.fixed && w.key.to == name) else {
                break;
            };
            name = wire.key.from.as_str();
            if let Some((_, from, to)) = stays.iter().find(|(stay, ..)| stay == name) {
                step.into = Some((*from, *to));
                break;
            }
        }
    }
}

fn stay(from: &Side, to: &Side, from_header: f32) -> Move {
    Move {
        name: from.name.clone(),
        from: from.rect,
        to: to.rect,
        from_fill: from.fill,
        to_fill: to.fill,
        from_bend: from.bend,
        to_bend: to.bend,
        columns: Default::default(),
        parts: [from.part.clone(), to.part.clone()],
        from_header,
        role: Role::Stay,
        into: None,
    }
}

/// `from` was a box that had left; the writer was typing its name into `to`.
fn stay_move(from: &Move, to: &Side) -> Move {
    Move {
        name: to.name.clone(),
        from: from.from,
        to: to.rect,
        from_fill: from.from_fill,
        to_fill: to.fill,
        from_bend: from.from_bend,
        to_bend: to.bend,
        columns: Default::default(),
        parts: [from.parts[0].clone(), to.part.clone()],
        from_header: from.from_header,
        role: Role::Stay,
        into: None,
    }
}

/// The arriving box whose name continues `step`'s, the nearest if several do.
fn typed_partner(
    step: &Move,
    new: &[Side],
    used: &[bool],
    old_size: Size,
    new_size: Size,
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, to) in new.iter().enumerate() {
        if used[index] || !continues(&step.name, &to.name) {
            continue;
        }
        let dist = center_shift(step.from, to.rect, old_size, new_size);
        if best.is_none_or(|(_, have)| dist < have) {
            best = Some((index, dist));
        }
    }
    best.map(|(index, _)| index)
}

/// One name is the other with more letters, the way a word grows as it is typed.
fn continues(old: &str, new: &str) -> bool {
    let (short, long) = if old.len() <= new.len() {
        (old, new)
    } else {
        (new, old)
    };
    !short.is_empty() && short.len() < long.len() && long.starts_with(short)
}

fn center_shift(from: Rectangle, to: Rectangle, old: Size, new: Size) -> f32 {
    let (a, b) = (norm(from, old), norm(to, new));
    let dx = (a[0] + a[2] * 0.5) - (b[0] + b[2] * 0.5);
    let dy = (a[1] + a[3] * 0.5) - (b[1] + b[3] * 0.5);
    dx * dx + dy * dy
}

fn leave(from: &Side, header: f32) -> Move {
    Move {
        name: from.name.clone(),
        from: from.rect,
        to: from.rect,
        from_fill: from.fill,
        to_fill: from.fill,
        from_bend: from.bend,
        to_bend: from.bend,
        columns: Default::default(),
        parts: [from.part.clone(), from.part.clone()],
        from_header: header,
        role: Role::Leave,
        into: None,
    }
}

fn area(step: &Move) -> f32 {
    let from = step.from.width * step.from.height;
    let to = step.to.width * step.to.height;
    from.max(to)
}

/// `1` when nothing is drawn inside the box. Otherwise the band above the
/// first box it holds, as a share of its height.
fn headers(boxes: &[Side]) -> Vec<f32> {
    boxes
        .iter()
        .map(|box_| {
            let mut top = f32::MAX;
            for other in boxes {
                if std::ptr::eq(box_, other) {
                    continue;
                }
                if inside(other.rect, box_.rect) {
                    top = top.min(other.rect.y);
                }
            }
            if top == f32::MAX {
                return 1.0;
            }
            let band = (top - box_.rect.y - 2.0).max(box_.rect.height * 0.08);
            (band / box_.rect.height.max(1.0)).clamp(0.08, 0.92)
        })
        .collect()
}

fn inside(inner: Rectangle, outer: Rectangle) -> bool {
    inner.x > outer.x + 1.0
        && inner.y > outer.y + 1.0
        && inner.x + inner.width < outer.x + outer.width - 1.0
        && inner.y + inner.height < outer.y + outer.height - 1.0
        && inner.width * inner.height < outer.width * outer.height * 0.92
}

fn shifted(from: Rectangle, to: Rectangle, old: Size, new: Size) -> bool {
    let (a, b) = (norm(from, old), norm(to, new));
    a.iter()
        .zip(b)
        .any(|(left, right)| (left - right).abs() > SHIFT)
}

fn norm(rect: Rectangle, size: Size) -> [f32; 4] {
    let (w, h) = (size.width.max(1.0), size.height.max(1.0));
    [rect.x / w, rect.y / h, rect.width / w, rect.height / h]
}

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
    /// The sheet, eased from the old paper to the new one.
    paper: Rectangle,
    old_paper: Rectangle,
    new_paper: Rectangle,
    /// `paper` held one device pixel in, so the cleared edge of a picture
    /// does not draw a light line.
    visible: Rectangle,
    /// The pictures' pixels to a unit, eased from the old one's to the
    /// new one's, as a box that stays is drawn.
    natural: f32,
    /// A pixel of the screen, in the pane's units.
    pixel: f32,
    out: f32,
    inn: f32,
}

/// Where the sheet and its pictures are at `paint.t`.
fn placed(grid: Grid, local: Rectangle, paint: &Paint<'_>) -> Placed {
    let t = paint.t.clamp(0.0, 1.0);
    let old_paper = grid.snap(paint.old_view.diagram_rect(paint.old_size, local));
    let new_paper = grid.snap(paint.new_view.diagram_rect(paint.new_size, local));
    // Lerped once. Snapping again would step the sheet by a pixel.
    let paper = lerp_rect(old_paper, new_paper, t);
    Placed {
        t,
        paper,
        old_paper,
        new_paper,
        visible: inset_px(paper, 1.0 / grid.scale),
        natural: lerp(
            old_paper.width / paint.old_size.width.max(1e-3),
            new_paper.width / paint.new_size.width.max(1e-3),
            t,
        ),
        pixel: 1.0 / grid.scale,
        out: line_out(t),
        inn: line_in(t),
    }
}

/// Where the lines of the glide go ([`super::wires`]), for the layer that
/// draws them over the pictures: iced draws the pictures of a layer over
/// its shapes, so they cannot share one.
pub fn sheets<'a>(grid: Grid, local: Rectangle, paint: &Paint<'a>) -> super::wires::Sheets<'a> {
    let placed = placed(grid, local, paint);
    super::wires::Sheets {
        pictures: (paint.old, paint.new),
        t: placed.t,
        out: placed.out,
        inn: placed.inn,
        old: (placed.old_paper, paint.old_size),
        new: (placed.new_paper, paint.new_size),
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
    paint_lines(frame, local, paint, true, &placed);
    paint_lines(frame, local, paint, false, &placed);
    for index in 0..paint.morph.moves.len() {
        paint_box(frame, local, paint, index, &placed);
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

/// The picture with its boxes left out, at `alpha`. A box is cut where it
/// sits in that picture, mapped onto the easing sheet, so the hole and the
/// sprite that covers it are the same pixels at either end.
fn paint_lines(frame: &mut Frame, local: Rectangle, paint: &Paint<'_>, old: bool, placed: &Placed) {
    let alpha = if old { placed.out } else { placed.inn };
    if alpha <= 0.02 || placed.paper.width <= 1.0 || placed.paper.height <= 1.0 {
        return;
    }
    let (raster, size) = if old {
        (paint.old, paint.old_size)
    } else {
        (paint.new, paint.new_size)
    };
    let holes = box_holes(paint, old, size, placed.paper);
    sprite::punched(
        frame,
        local,
        &sprite::Blit {
            raster,
            src: Rectangle::with_size(size),
            dest: placed.paper,
            limit: placed.visible,
            alpha,
        },
        &holes,
    );
}

/// Where the boxes of the picture before, or of the new one, are cut out
/// of it. A box drawn from a part of its own is not in the picture under
/// it (its ground), and needs no hole.
fn box_holes(paint: &Paint<'_>, old: bool, size: Size, paper: Rectangle) -> Vec<Rectangle> {
    let raster = if old { paint.old } else { paint.new };
    paint
        .morph
        .moves
        .iter()
        .filter_map(|step| {
            if (old && step.role == Role::Arrive) || (!old && step.role == Role::Leave) {
                return None;
            }
            if raster.part(&step.parts[usize::from(!old)]).is_some() {
                return None;
            }
            let rect = if old { step.from } else { step.to };
            Some(onto(grow(rect), size, paper))
        })
        .collect()
}

fn paint_box(
    frame: &mut Frame,
    local: Rectangle,
    paint: &Paint<'_>,
    index: usize,
    placed: &Placed,
) {
    let step = &paint.morph.moves[index];
    let dest = sprite_dest(step, paint, placed);
    if dest.width < 1.0 || dest.height < 1.0 {
        return;
    }
    // A box that stays is opaque the whole way: the old pixels first, then
    // the new ones over them. Drawing both at half would wash the box out.
    let old_a = match step.role {
        Role::Leave => placed.out,
        Role::Stay if placed.t < 1.0 => 1.0,
        Role::Stay | Role::Arrive => 0.0,
    };
    let new_a = match step.role {
        Role::Arrive => placed.inn,
        Role::Stay => placed.t,
        Role::Leave => 0.0,
    };
    if old_a <= 0.02 && new_a <= 0.02 {
        return;
    }
    // The holes of each picture filled with its own box's color: the
    // container's, not the folded box's it becomes.
    for (old, alpha) in [(true, old_a), (false, new_a)] {
        if alpha > 0.02 {
            paint_side(frame, local, paint, index, placed, dest, old, alpha);
        }
    }
}

/// The box's pixels of the picture before, or of the new one, in `dest`:
/// shaped to it ([`sprite::fit`]), with the boxes it holds cut out and
/// their holes filled with its own color. One that stays, or glides into
/// the box it hung below, changes its shape on the way; one that only goes
/// or comes stretches with the sheet.
#[allow(clippy::too_many_arguments)]
fn paint_side(
    frame: &mut Frame,
    local: Rectangle,
    paint: &Paint<'_>,
    index: usize,
    placed: &Placed,
    dest: Rectangle,
    old: bool,
    alpha: f32,
) {
    let step = &paint.morph.moves[index];
    let (raster, size, src, bend, fill) = if old {
        let src = grow(step.from);
        (
            paint.old,
            paint.old_size,
            src,
            step.from_bend,
            step.from_fill,
        )
    } else {
        (
            paint.new,
            paint.new_size,
            grow(step.to),
            step.to_bend,
            step.to_fill,
        )
    };
    let natural = match step.role {
        Role::Stay => Some(placed.natural),
        // The eased sheet's scale for that picture.
        _ if step.into.is_some() => Some(
            (placed.paper.width / size.width.max(1e-3))
                .min(placed.paper.height / size.height.max(1e-3)),
        ),
        _ => None,
    };
    let children = children(paint, index, old, placed);
    let rects: Vec<Rectangle> = children.iter().map(|child| child.rect).collect();
    // Drawn from a part of its own, the box holds nothing of another and
    // none of its lines' words: nothing to cut out, fill or fade.
    // Both pictures keep to the box's own rectangle, so their rims meet.
    if let Some((part, _)) = raster.part(&step.parts[usize::from(!old)]) {
        let fit = sprite::fit(src, dest, natural, placed.pixel, bend, &rects);
        let swatch = fill.map(Raster::swatch);
        let whole = sprite::Holes {
            children: &[],
            columns: &[],
            swatch: swatch.as_ref(),
            content: alpha,
        };
        sprite::draw(frame, local, &part, &fit, &whole, alpha);
        return;
    }
    let fit = sprite::fit(src, dest, natural, placed.pixel, bend, &rects);
    let prepared = &step.columns[usize::from(!old)];
    let worked;
    let columns = if prepared.len() == rects.len() {
        &prepared[..]
    } else {
        worked = sprite::columns(raster, fit.interior, &rects);
        &worked[..]
    };
    // A container whose boxes go, or come: its words go with their lines.
    let held = held(&paint.morph.moves, index, old);
    let moving = |role| held.iter().any(|(i, _)| paint.morph.moves[*i].role == role);
    let content = match step.role {
        Role::Stay if old && moving(Role::Leave) => alpha.min(placed.out),
        Role::Stay if !old && moving(Role::Arrive) => alpha.min(placed.inn),
        _ => alpha,
    };
    let swatch = fill.map(Raster::swatch);
    let holes = sprite::Holes {
        children: &children,
        columns,
        swatch: swatch.as_ref(),
        content,
    };
    sprite::draw(frame, local, raster, &fit, &holes, alpha);
}

/// Works out once, for a glide drawn from `old` to `new`, the columns of
/// each box's own color that fill the holes of the boxes it holds
/// ([`sprite::columns`]): the pictures and the boxes are the same in every
/// frame.
pub fn prepare(morph: &mut Morph, old: &Raster, new: &Raster) {
    for index in 0..morph.moves.len() {
        for (side, raster) in [(0, old), (1, new)] {
            let held = held(&morph.moves, index, side == 0);
            let step = &morph.moves[index];
            let (src, bend) = if side == 0 {
                (grow(step.from), step.from_bend)
            } else {
                (grow(step.to), step.to_bend)
            };
            let rects: Vec<Rectangle> = held.iter().map(|(_, rect)| *rect).collect();
            let interior = sprite::rim(src, bend, &rects).map(|rim| sprite::inset(src, rim));
            morph.moves[index].columns[side] = sprite::columns(raster, interior, &rects);
        }
    }
}

/// Where this box is drawn. One that stays eases from its old place on the
/// old sheet to its new place on the new sheet. One that leaves or arrives
/// stays on the picture it belongs to, so it scales with that picture.
fn sprite_dest(step: &Move, paint: &Paint<'_>, placed: &Placed) -> Rectangle {
    match step.role {
        Role::Stay => lerp_rect(
            onto(grow(step.from), paint.old_size, placed.old_paper),
            onto(grow(step.to), paint.new_size, placed.new_paper),
            placed.t,
        ),
        Role::Leave => {
            let own = onto(grow(step.from), paint.old_size, placed.paper);
            // Into its box, while it fades.
            match parent_dest(step, paint, placed) {
                Some(parent) => {
                    let gone = (placed.t / LINE_OUT).clamp(0.0, 1.0);
                    lerp_rect(own, parent, smooth(gone))
                }
                None => own,
            }
        }
        Role::Arrive => {
            let own = onto(grow(step.to), paint.new_size, placed.paper);
            // Out of its box, while it comes.
            match parent_dest(step, paint, placed) {
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
fn parent_dest(step: &Move, paint: &Paint<'_>, placed: &Placed) -> Option<Rectangle> {
    let (from, to) = step.into?;
    Some(lerp_rect(
        onto(grow(from), paint.old_size, placed.old_paper),
        onto(grow(to), paint.new_size, placed.new_paper),
        placed.t,
    ))
}

/// The boxes drawn inside box `index`, in the picture before or in the
/// new one, in that picture's units as they are cut out: they glide on
/// their own. One that stays covers its place whole while the picture
/// before is drawn: its own pixels, opaque, over the hole it leaves.
fn children(paint: &Paint<'_>, index: usize, old: bool, placed: &Placed) -> Vec<sprite::Child> {
    held(&paint.morph.moves, index, old)
        .into_iter()
        .map(|(other, rect)| {
            let other = &paint.morph.moves[other];
            sprite::Child {
                rect,
                covered: (old && other.role == Role::Stay && placed.t < 1.0)
                    .then(|| sprite_dest(other, paint, placed)),
            }
        })
        .collect()
}

/// The boxes inside box `index` in the picture before or the new one: each
/// one's index, and its place as it is cut out.
fn held(moves: &[Move], index: usize, old: bool) -> Vec<(usize, Rectangle)> {
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
        .map(|(other_i, other)| (other_i, grow(rect(other))))
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

    fn side(name: &str, x: f32, y: f32, w: f32, h: f32) -> Side {
        Side {
            name: name.to_owned(),
            rect: Rectangle::new(Point::new(x, y), Size::new(w, h)),
            fill: Some([0.9, 0.9, 0.9]),
            part: name.to_owned(),
            bend: Some(pick::Bend {
                radius: 8.0,
                inset: 0.0,
            }),
        }
    }

    fn size() -> Size {
        Size::new(400.0, 300.0)
    }

    #[test]
    fn a_recolor_keeps_the_fade() {
        let boxes = vec![
            side("On", 10.0, 20.0, 80.0, 40.0),
            side("Off", 120.0, 20.0, 80.0, 40.0),
        ];
        assert!(plan(&boxes, &boxes, size(), size()).is_none());
    }

    #[test]
    fn a_moved_box_glides_and_a_nested_one_keeps_its_own_body() {
        let old = vec![
            side("On", 10.0, 10.0, 200.0, 180.0),
            side("Playing", 30.0, 50.0, 80.0, 40.0),
        ];
        let new = vec![
            side("On", 40.0, 30.0, 220.0, 160.0),
            side("Playing", 60.0, 90.0, 70.0, 36.0),
            side("Updating", 300.0, 40.0, 60.0, 30.0),
        ];
        let morph = plan(&old, &new, size(), size()).expect("the boxes moved");
        let on = morph.moves.iter().find(|step| step.name == "On").unwrap();
        assert_eq!(on.role, Role::Stay);
        assert!(on.from_header < 0.5, "the title band is {}", on.from_header);
        assert!((on.from.x - 10.0).abs() < 0.1 && (on.to.x - 40.0).abs() < 0.1);
        let playing = morph
            .moves
            .iter()
            .find(|step| step.name == "Playing")
            .unwrap();
        assert_eq!(playing.from_header, 1.0);
        let updating = morph
            .moves
            .iter()
            .find(|step| step.name == "Updating")
            .unwrap();
        assert_eq!(updating.role, Role::Arrive);
        // The frame is drawn before the box it holds.
        let on_at = morph
            .moves
            .iter()
            .position(|step| step.name == "On")
            .unwrap();
        let playing_at = morph
            .moves
            .iter()
            .position(|step| step.name == "Playing")
            .unwrap();
        assert!(on_at < playing_at);
    }

    #[test]
    fn a_box_that_goes_leaves_where_it_was() {
        let old = vec![
            side("On", 10.0, 10.0, 80.0, 40.0),
            side("Gone", 200.0, 10.0, 40.0, 20.0),
        ];
        let new = vec![side("On", 80.0, 60.0, 90.0, 40.0)];
        let morph = plan(&old, &new, size(), size()).unwrap();
        let gone = morph.moves.iter().find(|step| step.name == "Gone").unwrap();
        assert_eq!(gone.role, Role::Leave);
        assert_eq!(gone.from, gone.to);
    }

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

    #[test]
    fn a_sprite_meets_its_hole_at_both_ends() {
        let old_size = Size::new(400.0, 300.0);
        let new_size = Size::new(500.0, 360.0);
        let from = Rectangle::new(Point::new(10.0, 20.0), Size::new(80.0, 40.0));
        let to = Rectangle::new(Point::new(40.0, 30.0), Size::new(90.0, 50.0));
        let child = Rectangle::new(Point::new(24.0, 36.0), Size::new(30.0, 16.0));
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
        // A child cut out of its parent lands on the child's own place.
        let parent_dest = onto(grow(from), old_size, old_paper);
        let natural = old_paper.width / old_size.width;
        let fit = sprite::fit(
            grow(from),
            parent_dest,
            Some(natural),
            1.0,
            Some(pick::Bend {
                radius: 8.0,
                inset: 0.0,
            }),
            &[grow(child)],
        );
        close(
            fit.place(grow(child)).unwrap(),
            onto(grow(child), old_size, old_paper),
        );
    }

    /// A start is known by the state it sits in: the one inside a state
    /// that folds goes, and the one beside it does not glide into its place,
    /// though it is first from the top once the other is gone.
    #[test]
    fn a_mark_is_named_by_the_state_it_sits_in() {
        let at = |x: f32, y: f32| omaspec_uml::graph::Rect {
            x,
            y,
            w: 22.0,
            h: 22.0,
        };
        let open = mark_names(
            "end",
            &[(Some("Awake"), at(50.0, 200.0)), (None, at(50.0, 400.0))],
        );
        assert_eq!(open, ["end:Awake:0", "end::0"]);
        let folded = mark_names("end", &[(None, at(50.0, 300.0))]);
        assert_eq!(folded, ["end::0"]);
        let side = |name: &str, y: f32| Side {
            rect: Rectangle::new(Point::new(50.0, y), Size::new(22.0, 22.0)),
            ..super::tests::side(name, 0.0, 0.0, 1.0, 1.0)
        };
        let morph = plan(
            &[side(&open[0], 200.0), side(&open[1], 400.0)],
            &[side(&folded[0], 300.0)],
            size(),
            size(),
        )
        .unwrap();
        let role = |name: &str| morph.moves.iter().find(|m| m.name == name).unwrap().role;
        assert_eq!(role("end:Awake:0"), Role::Leave);
        assert_eq!(role("end::0"), Role::Stay);
    }

    #[test]
    fn a_name_being_typed_keeps_the_box() {
        let old = vec![
            side("Pla", 10.0, 20.0, 80.0, 40.0),
            side("Off", 200.0, 20.0, 80.0, 40.0),
        ];
        let new = vec![
            side("Playing", 40.0, 50.0, 100.0, 48.0),
            side("Off", 200.0, 20.0, 80.0, 40.0),
        ];
        let morph = plan(&old, &new, size(), size()).expect("Pla moved as it grew");
        let playing = morph
            .moves
            .iter()
            .find(|step| step.name == "Playing")
            .unwrap();
        assert_eq!(playing.role, Role::Stay);
        assert!((playing.from.x - 10.0).abs() < 0.1);
        assert!(
            morph
                .moves
                .iter()
                .all(|step| step.role != Role::Leave && step.role != Role::Arrive)
        );

        // Deleting letters is the same box. A different word is not.
        let back = plan(&new, &old, size(), size()).unwrap();
        assert_eq!(
            back.moves
                .iter()
                .find(|step| step.name == "Pla")
                .unwrap()
                .role,
            Role::Stay
        );
        let renamed = plan(
            &[side("On", 10.0, 10.0, 80.0, 40.0)],
            &[
                side("On", 80.0, 60.0, 90.0, 40.0),
                side("Closed", 200.0, 10.0, 40.0, 20.0),
            ],
            size(),
            size(),
        )
        .unwrap();
        assert!(
            renamed
                .moves
                .iter()
                .any(|step| step.name == "Closed" && step.role == Role::Arrive)
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
