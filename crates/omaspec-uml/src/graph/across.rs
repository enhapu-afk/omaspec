//! Lines between containers.
//!
//! A container that cannot be laid out with what is around it (one with
//! concurrent regions or a direction of its own) is laid out on its own,
//! so a transition from inside it to a state elsewhere has no route in
//! any one layout. The container that holds both ends ranks the two boxes
//! that hold them as if they were joined ([`meeting`] finds them). Once
//! everything is placed, [`link`] routes the line at right angles around
//! the boxes, into and out of the nested states it joins, and finds a
//! place for its label.

use super::Rect;
use super::route::{self, End, LabelRoom, Point, Surroundings};

/// How far routes may stray past the drawing: left and right, and above.
const STRAY: (f32, f32) = (60.0, 24.0);

/// A placed box of a nested layout, in the coordinates all boxes share.
#[derive(Debug, Clone, Default)]
pub struct Placement {
    pub rect: Rect,
    /// Lines may meet it anywhere along a side (a state, a bar), not only
    /// in the middle (a circle, a diamond).
    pub flat: bool,
    /// Drawn as a frame: a nested state. A concurrent region is not one.
    pub frame: bool,
    /// The title band of a nested state, which lines and labels keep off.
    pub title: Option<Rect>,
    pub children: Vec<Placement>,
}

/// A line to route. Each end is the path of child indices from the top
/// level down to its box.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub from: Vec<usize>,
    pub to: Vec<usize>,
    /// The label's width and height; zero for none.
    pub label: (f32, f32),
    /// Points the line passes through in turn: its bends by hand. One in
    /// a box the line may not cross is left out.
    pub vias: Vec<Point>,
}

/// What is drawn already, which new lines and labels keep clear of.
#[derive(Debug, Clone, Default)]
pub struct Drawn {
    pub lines: Vec<Vec<Point>>,
    pub labels: Vec<Rect>,
    pub notes: Vec<Rect>,
}

/// A routed link: its polyline, and the top left of its label.
#[derive(Debug, Clone, PartialEq)]
pub struct Linked {
    pub points: Vec<Point>,
    pub label_at: Point,
    /// How many of the link's vias lay in a box and were left out.
    pub left_out: usize,
}

/// Where the paths of two ends meet: how deep the container that holds
/// both is, and the box in it that holds each end. `None` when one end
/// holds the other, or a path is empty.
pub fn meeting(from: &[usize], to: &[usize]) -> Option<(usize, usize, usize)> {
    if from.is_empty() || to.is_empty() {
        return None;
    }
    let mut depth = 0;
    while depth + 1 < from.len() && depth + 1 < to.len() && from[depth] == to[depth] {
        depth += 1;
    }
    let (a, b) = (from[depth], to[depth]);
    (a != b).then_some((depth, a, b))
}

/// Routes `links`, the shortest first so it gets the straightest line.
/// `None` for a link whose end is missing or that found no way through.
pub fn link(top: &[Placement], links: &[Link], drawn: &Drawn) -> Vec<Option<Linked>> {
    let mut out = vec![None; links.len()];
    let ends: Vec<Option<(&Placement, &Placement)>> = links
        .iter()
        .map(|link| Some((find(top, &link.from)?, find(top, &link.to)?)))
        .collect();
    let reach = |index: usize| {
        ends[index].map_or(f32::INFINITY, |(a, b)| {
            let (a, b) = (center(a.rect), center(b.rect));
            (a.0 - b.0).abs() + (a.1 - b.1).abs()
        })
    };
    let mut order: Vec<usize> = (0..links.len()).filter(|&i| ends[i].is_some()).collect();
    order.sort_by(|&a, &b| reach(a).total_cmp(&reach(b)));

    let Some(extent) = top.iter().map(|place| place.rect).reduce(union) else {
        return out;
    };
    let origin = (extent.x - STRAY.0, extent.y - STRAY.1);
    let mut lines: Vec<(Point, Point)> = drawn.lines.iter().flat_map(|l| segments(l)).collect();
    let mut used: Vec<Point> = drawn
        .lines
        .iter()
        .flat_map(|l| [l.first(), l.last()])
        .flatten()
        .copied()
        .collect();
    let mut labels = drawn.labels.clone();
    let mut solids = drawn.notes.clone();
    let mut frames = Vec::new();
    leaves(top, &mut solids, &mut frames);

    for index in order {
        let link = &links[index];
        let Some((from, to)) = ends[index] else {
            continue;
        };
        let mut around = Surroundings {
            lines: lines.clone(),
            labels: labels.clone(),
            used: used.clone(),
            origin,
            ..Surroundings::default()
        };
        obstacles(top, Some(&link.from), Some(&link.to), &mut around);
        around.obstacles.extend(drawn.notes.iter().copied());
        let end = |place: &Placement| End {
            rect: place.rect,
            flat: place.flat,
        };
        let vias: Vec<Point> = link
            .vias
            .iter()
            .copied()
            .filter(|&v| !around.obstacles.iter().any(|&r| inside(r, v)))
            .collect();
        let Some(points) = route::route_via(end(from), end(to), &vias, &around) else {
            continue;
        };
        let (w, h) = link.label;
        let label_at = if w > 0.0 && h > 0.0 {
            // A line through bends by hand can double back past its label:
            // its own runs count as well.
            let mut near = lines.clone();
            if !link.vias.is_empty() {
                near.extend(segments(&points));
            }
            let room = LabelRoom {
                solids: &solids,
                frames: &frames,
                lines: &near,
                origin,
                on_line: false,
            };
            let (cx, cy) =
                route::label_spot(&points, (w, h), &room, None).unwrap_or_else(|| middle(&points));
            let rect = Rect {
                x: cx - w / 2.0,
                y: cy - h / 2.0,
                w,
                h,
            };
            labels.push(rect);
            solids.push(rect);
            (rect.x, rect.y)
        } else {
            middle(&points)
        };
        lines.extend(segments(&points));
        used.extend(points.first().copied());
        used.extend(points.last().copied());
        out[index] = Some(Linked {
            points,
            label_at,
            left_out: link.vias.len() - vias.len(),
        });
    }
    out
}

/// The box at the end of `path`, from the top level down.
pub fn find<'a>(level: &'a [Placement], path: &[usize]) -> Option<&'a Placement> {
    let (&first, rest) = path.split_first()?;
    let place = level.get(first)?;
    if rest.is_empty() {
        Some(place)
    } else {
        find(&place.children, rest)
    }
}

/// What a link must not pass through. A box that holds one of its ends
/// is open: the line goes into it, around what it holds and off its
/// title, and crosses its border rather than running along it. Every
/// other box is closed, with whatever it holds.
fn obstacles(
    level: &[Placement],
    from: Option<&[usize]>,
    to: Option<&[usize]>,
    around: &mut Surroundings,
) {
    for (index, place) in level.iter().enumerate() {
        let (from_rest, to_rest) = (below(from, index), below(to, index));
        if from_rest.is_none() && to_rest.is_none() {
            around.obstacles.push(place.rect);
            continue;
        }
        around.obstacles.extend(place.title);
        if place.frame {
            around.frames.push(place.rect);
        }
        obstacles(&place.children, from_rest, to_rest, around);
    }
}

/// The rest of `path` when it leads through the box at `index`.
fn below(path: Option<&[usize]>, index: usize) -> Option<&[usize]> {
    let (&first, rest) = path?.split_first()?;
    (first == index && !rest.is_empty()).then_some(rest)
}

/// Every leaf box into `solids` and every frame into `frames`, for the
/// labels to keep clear of.
fn leaves(level: &[Placement], solids: &mut Vec<Rect>, frames: &mut Vec<Rect>) {
    for place in level {
        if place.children.is_empty() && !place.frame {
            solids.push(place.rect);
            continue;
        }
        if place.frame {
            frames.push(place.rect);
        }
        solids.extend(place.title);
        leaves(&place.children, solids, frames);
    }
}

fn segments(points: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    points.windows(2).map(|w| (w[0], w[1]))
}

/// Whether `p` lies inside `rect`, not on its border.
fn inside(rect: Rect, (x, y): Point) -> bool {
    x > rect.x && x < rect.x + rect.w && y > rect.y && y < rect.y + rect.h
}

fn center(rect: Rect) -> Point {
    (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0)
}

fn union(a: Rect, b: Rect) -> Rect {
    let (x, y) = (a.x.min(b.x), a.y.min(b.y));
    let right = (a.x + a.w).max(b.x + b.w);
    let bottom = (a.y + a.h).max(b.y + b.h);
    Rect {
        x,
        y,
        w: right - x,
        h: bottom - y,
    }
}

/// Halfway along the line.
fn middle(points: &[Point]) -> Point {
    let length = |(a, b): (Point, Point)| (b.0 - a.0).abs() + (b.1 - a.1).abs();
    let total: f32 = segments(points).map(length).sum();
    let mut left = total / 2.0;
    for (a, b) in segments(points) {
        let step = length((a, b));
        if step >= left && step > 0.0 {
            let t = left / step;
            return (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        }
        left -= step;
    }
    points.first().copied().unwrap_or((0.0, 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    fn leaf(rect: Rect) -> Placement {
        Placement {
            rect,
            flat: true,
            ..Placement::default()
        }
    }

    fn cuts_any(points: &[Point], rect: Rect) -> bool {
        segments(points).any(|(a, b)| route::cuts(rect, a, b))
    }

    #[test]
    fn paths_meet_in_the_container_that_holds_both() {
        assert_eq!(meeting(&[0, 2], &[1]), Some((0, 0, 1)));
        assert_eq!(meeting(&[0, 2], &[0, 3]), Some((1, 2, 3)));
        assert_eq!(meeting(&[0, 1, 4], &[0, 2]), Some((1, 1, 2)));
        assert_eq!(meeting(&[0], &[0, 3]), None, "one holds the other");
        assert_eq!(meeting(&[0, 2, 5], &[0, 2]), None);
        assert_eq!(meeting(&[], &[1]), None);
    }

    /// Outside: `Idle` above a nested state that holds `Busy` and, right
    /// under its title, `Waiting`. The link runs from `Idle` to `Busy`.
    fn nested() -> (Vec<Placement>, Rect, Rect) {
        let waiting = rect(130.0, 140.0, 60.0, 30.0);
        let busy = rect(120.0, 220.0, 80.0, 30.0);
        let title = rect(140.0, 104.0, 40.0, 16.0);
        let frame = Placement {
            rect: rect(100.0, 100.0, 120.0, 170.0),
            flat: true,
            frame: true,
            title: Some(title),
            children: vec![leaf(waiting), leaf(busy)],
        };
        let idle = leaf(rect(120.0, 0.0, 80.0, 30.0));
        (vec![idle, frame], waiting, title)
    }

    #[test]
    fn a_link_into_a_nested_state_goes_around_what_it_holds() {
        let (top, waiting, title) = nested();
        let links = [Link {
            from: vec![0],
            to: vec![1, 1],
            label: (30.0, 12.0),
            vias: Vec::new(),
        }];
        let routed = link(&top, &links, &Drawn::default());
        let linked = routed[0].as_ref().expect("routed");
        let points = &linked.points;
        assert!(!cuts_any(points, waiting), "{points:?}");
        assert!(!cuts_any(points, title), "{points:?}");
        let busy = top[1].children[1].rect;
        let end = *points.last().unwrap();
        let on_busy = (end.1 - busy.y).abs() < 0.1
            || (end.1 - (busy.y + busy.h)).abs() < 0.1
            || (end.0 - busy.x).abs() < 0.1
            || (end.0 - (busy.x + busy.w)).abs() < 0.1;
        assert!(on_busy, "{points:?}");
        for pair in points.windows(2) {
            assert!(
                pair[0].0 == pair[1].0 || pair[0].1 == pair[1].1,
                "{points:?}"
            );
        }
        let label = rect(linked.label_at.0, linked.label_at.1, 30.0, 12.0);
        for other in [waiting, busy, title, top[0].rect] {
            assert!(!overlaps(label, other), "{label:?} on {other:?}");
        }
    }

    fn overlaps(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    #[test]
    fn a_closed_nested_state_is_one_obstacle() {
        let (mut top, _, _) = nested();
        let other = leaf(rect(300.0, 0.0, 60.0, 30.0));
        let below = leaf(rect(300.0, 300.0, 60.0, 30.0));
        top.push(other);
        top.push(below);
        // From the top right to the bottom right; the nested state is not
        // entered, so nothing may cut it.
        let links = [Link {
            from: vec![2],
            to: vec![3],
            label: (0.0, 0.0),
            vias: Vec::new(),
        }];
        let routed = link(&top, &links, &Drawn::default());
        let points = &routed[0].as_ref().unwrap().points;
        assert!(!cuts_any(points, top[1].rect), "{points:?}");
    }

    #[test]
    fn a_missing_end_is_not_routed() {
        let (top, _, _) = nested();
        let links = [Link {
            from: vec![0],
            to: vec![1, 7],
            label: (0.0, 0.0),
            vias: Vec::new(),
        }];
        assert_eq!(link(&top, &links, &Drawn::default()), vec![None]);
    }
}
