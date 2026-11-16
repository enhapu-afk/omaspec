//! The boxes of a new picture glide to their places.
//!
//! A render that only recolors the same layout fades, as before. One that
//! moves a box, of a diagram that marks its parts (docs/adr/0029), waits
//! until the sheets of both pictures are drawn: the ground, without lines
//! and parts, and each part on its own. Then the sheet eases from the old
//! size to the new one and each box travels, as the picture of its own
//! part, from where it was to where it is; a box inside another is not in
//! its picture. The lines glide along ([`super::wires`]), their words with
//! them. The first frame is the old picture and the last frame is the new
//! one, so nothing pops at either end. A picture without parts fades.

use iced::{Point, Rectangle, Size};

use omaspec_render::pick;

use super::Diagram;

mod paint;

#[cfg(test)]
pub(in crate::paper) use paint::box_pictures_of;
pub use paint::{Paint, paint, prepare, sheets};

/// One named box of a picture, in that picture's units.
#[derive(Clone, Debug)]
pub struct Side {
    pub name: String,
    pub rect: Rectangle,
    /// The part it is drawn in, where the picture marks its parts
    /// (docs/adr/0029): a box's name, a mark's place.
    pub part: String,
    /// Its rows are laid out from its top left, and its size follows
    /// them: a box the layout alone lays, as a JSON card or a tree's box
    /// ([`omaspec_uml::map::Place::fixed`]). Drawn from its own part, each
    /// picture of it is drawn from there at its own size.
    pub anchored: bool,
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
    /// The part it is drawn in, before and after ([`Side::part`]).
    pub parts: [String; 2],
    /// Its rows are laid out from its top left ([`Side::anchored`]).
    pub anchored: bool,
    /// How it is drawn, worked out once a glide ([`prepare`]).
    pub looks: Looks,
    pub role: Role,
    /// The box of a tree it hangs below, which stays: where that one was
    /// and where it is. A box that leaves glides into it, one that
    /// arrives comes out of it ([`gather`]).
    pub into: Option<(Rectangle, Rectangle)>,
}

/// How a box of a glide is drawn, the same in every frame: worked out
/// once, from the two sheets it glides between ([`prepare`]). Each pair
/// is the picture before and the new one.
#[derive(Clone, Debug, Default)]
pub struct Looks {
    /// Its fill, when its own outline names one plain color.
    pub fill: [Option<[f32; 3]>; 2],
    /// How its own outline bends; none for a shape that is no box
    /// ([`pick::Fills::bend`]).
    pub bend: [Option<pick::Bend>; 2],
    /// The boxes inside it, each with its stroke ([`held`]).
    pub held: [Vec<Rectangle>; 2],
    /// Its new picture covers its box (a card with a fill), so the old one
    /// may stay opaque under it to the end.
    pub solid: bool,
}

/// The boxes of one glide.
#[derive(Clone, Debug)]
pub struct Morph {
    pub moves: Vec<Move>,
    /// The parts of the picture before and of the new one that no box and
    /// no line's words take ([`prepare`]): they go and come with the
    /// ground, so a part named otherwise than its box is never lost.
    pub orphans: [Vec<String>; 2],
    /// The parts no box takes that are in both pictures, as a title or
    /// the top card of a JSON diagram: each glides from where it was to
    /// where it is ([`paint_kept`]).
    pub kept: Vec<Kept>,
}

/// A part no box takes, in both pictures ([`Morph::kept`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub name: String,
    /// Its new picture covers its place, as a card does and words do not
    /// ([`Looks::solid`]).
    pub solid: bool,
    /// The last box around it (by its index among the moves), which it is
    /// drawn over: a note inside a state, over that state's picture. None
    /// for one no box holds, as a title, drawn under every box.
    pub over: Option<usize>,
}

/// How far a box may sit, as a share of the picture, and still count as
/// unmoved. A recolor stays under it. A fold does not.
const SHIFT: f32 = 0.012;
/// How far past a box's edge its stroke can reach, in diagram units: the
/// travelling picture takes it along, so the first frame still matches
/// the picture exactly.
const RIM: f32 = 2.0;
/// Old lines have gone by `LINE_OUT`. New ones begin at `LINE_IN`. The
/// quiet between them is short, so it reads as one change, and the lines
/// never cross.
const LINE_OUT: f32 = 0.45;
const LINE_IN: f32 = 0.55;

/// The named boxes of `diagram`, in the picture's units. A start or an end
/// travels too: it would otherwise vanish with the lines and pop back.
pub fn sides(diagram: &Diagram) -> Vec<Side> {
    let mut out: Vec<Side> = diagram
        .drawn
        .map
        .places
        .iter()
        .filter(|place| !place.name.is_empty() && !place.name.starts_with('['))
        .map(|place| {
            // The part of the state drawn at the place: a history mark is
            // `Brewing[H]` on the map and a mark among the parts.
            let part = map_part(&diagram.drawn.map, place);
            Side {
                anchored: place.fixed,
                ..side_at(&place.name, &part, place.rect)
            }
        })
        .collect();
    push_marks(diagram, omaspec_uml::map::Shape::Start, "start", &mut out);
    push_marks(diagram, omaspec_uml::map::Shape::End, "end", &mut out);
    out
}

/// The part the box at `place` is drawn in: that of the state drawn
/// there, else the one the map gives it, else the box's own name.
fn map_part(map: &omaspec_uml::map::Map, place: &omaspec_uml::map::Place) -> String {
    let same = |a: omaspec_uml::graph::Rect, b: omaspec_uml::graph::Rect| {
        (a.x - b.x).abs() < 0.5
            && (a.y - b.y).abs() < 0.5
            && (a.w - b.w).abs() < 0.5
            && (a.h - b.h).abs() < 0.5
    };
    map.states
        .iter()
        .find(|spot| spot.name == place.name && same(spot.rect, place.rect))
        .or_else(|| map.states.iter().find(|spot| same(spot.rect, place.rect)))
        .map(|spot| spot.part.clone())
        .or_else(|| place.part.clone())
        .unwrap_or_else(|| place.name.clone())
}

fn side_at(name: &str, part: &str, rect: omaspec_uml::graph::Rect) -> Side {
    Side {
        name: name.to_owned(),
        part: part.to_owned(),
        rect: Rectangle::new(Point::new(rect.x, rect.y), Size::new(rect.w, rect.h)),
        anchored: false,
    }
}

/// Starts, or ends, each named by the state it sits in and its place
/// among that state's, so the same dot matches next time: a start that
/// folds away with its state is never taken for one beside it.
fn push_marks(diagram: &Diagram, shape: omaspec_uml::map::Shape, label: &str, out: &mut Vec<Side>) {
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
        out.push(side_at(&name, &spot.part, spot.rect));
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
    let mut stays = Vec::new();
    let mut used_new = vec![false; new.len()];
    let mut leaves = Vec::new();
    for from in old {
        let Some(to_at) = new
            .iter()
            .enumerate()
            .position(|(nth, to)| !used_new[nth] && to.name == from.name)
        else {
            leaves.push(leave(from));
            continue;
        };
        used_new[to_at] = true;
        let to = &new[to_at];
        stays.push(stay(from, to));
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
            parts: [to.part.clone(), to.part.clone()],
            anchored: to.anchored,
            looks: Looks::default(),
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
    Some(Morph {
        moves,
        orphans: Default::default(),
        kept: Vec::new(),
    })
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

fn stay(from: &Side, to: &Side) -> Move {
    Move {
        name: from.name.clone(),
        from: from.rect,
        to: to.rect,
        parts: [from.part.clone(), to.part.clone()],
        anchored: from.anchored || to.anchored,
        looks: Looks::default(),
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
        parts: [from.parts[0].clone(), to.part.clone()],
        anchored: from.anchored || to.anchored,
        looks: Looks::default(),
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

fn leave(from: &Side) -> Move {
    Move {
        name: from.name.clone(),
        from: from.rect,
        to: from.rect,
        parts: [from.part.clone(), from.part.clone()],
        anchored: from.anchored,
        looks: Looks::default(),
        role: Role::Leave,
        into: None,
    }
}

fn area(step: &Move) -> f32 {
    let from = step.from.width * step.from.height;
    let to = step.to.width * step.to.height;
    from.max(to)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn side(name: &str, x: f32, y: f32, w: f32, h: f32) -> Side {
        Side {
            name: name.to_owned(),
            rect: Rectangle::new(Point::new(x, y), Size::new(w, h)),
            part: name.to_owned(),
            anchored: false,
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
        assert!((on.from.x - 10.0).abs() < 0.1 && (on.to.x - 40.0).abs() < 0.1);
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

    /// A history mark is `Brewing[H]` on the map and a mark among the
    /// parts: its box finds its part by the state drawn at its place.
    #[test]
    fn a_box_finds_the_part_of_the_state_drawn_at_its_place() {
        use omaspec_uml::graph::Rect;
        use omaspec_uml::map::{Map, Place, Shape, Spot};
        let rect = Rect {
            x: 10.0,
            y: 20.0,
            w: 22.0,
            h: 22.0,
        };
        let mut map = Map::default();
        map.states.push(Spot {
            name: String::new(),
            shape: Shape::History,
            line: 3,
            parent: Some("Brewing".into()),
            rect,
            radius: 11.0,
            part: "mark:1.2".into(),
        });
        let place = |name: &str, rect| Place {
            name: name.into(),
            rect,
            within: Vec::new(),
            across: false,
            entry: None,
            frame: false,
            fixed: false,
            part: None,
        };
        assert_eq!(map_part(&map, &place("Brewing[H]", rect)), "mark:1.2");
        let elsewhere = Rect { x: 200.0, ..rect };
        assert_eq!(map_part(&map, &place("Idle", elsewhere)), "Idle");
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
}
