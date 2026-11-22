//! Entry and exit points on the border of the state around them.
//!
//! Laid out among the states, the points are set on the border once all
//! is placed ([`onto_borders`]): on the top and bottom border where the
//! state runs down, on the left and right one where it runs left to
//! right, as PlantUML puts them. The lines inside the state reach on to
//! them there. A line between boxes at a point is laid anew by
//! [`super::nest::link_crossings`] to where the point stands for it
//! ([`stance`]): just outside its state, which is closed to the line, when
//! the line leads out of the state; just inside (under its title) when it
//! stays inside. [`meet`] then leads the line's end on to the point.

use super::super::{Container, Direction, Kind};
use super::{COMP_PAD, Placed, Rect, Scene, Side, stack_width};

type Point = (f32, f32);

/// Sets each point on its border, the lines inside its state and its
/// notes with it, and takes the route from every transition between
/// boxes that ends at one.
pub(super) fn onto_borders(root: &Container, scene: &mut Scene) {
    let mut moved = Vec::new();
    lift(root, scene, &mut Vec::new(), &mut moved);
    if !moved.is_empty() {
        unroute(root, scene, &mut Vec::new(), &moved);
    }
}

/// Where the end at `end` of a line to `other` stands for it, when it is
/// a point of `root`, laid out as `scene`, and whether the point's state
/// is closed to the line: a step outside the state for a line that leads
/// out of it, a step inside (under the title of a state whose states run
/// down, for an entry) for one that stays inside. `None` for an end that
/// is no point.
pub(super) fn stance(
    root: &Container,
    scene: &Scene,
    end: &[usize],
    other: &[usize],
) -> Option<(Rect, bool)> {
    let (last, holder) = end.split_last()?;
    let mut container = root;
    let mut laid = scene;
    let mut state = None;
    for &index in holder {
        state = Some(laid.nodes.get(index)?);
        container = &container.nodes.get(index)?.children;
        laid = &laid.nodes.get(index)?.children;
    }
    let node = container.nodes.get(*last)?;
    if !matches!(node.kind, Kind::Entry | Kind::Exit) {
        return None;
    }
    let rect = laid.nodes.get(*last)?.rect;
    let band = state.map_or(0.0, |state| state.item.band());
    let out = !other.starts_with(holder);
    // Off as far as room lets it, short of any box that does not hold the
    // point (a neighbor just past the border, a state inside).
    let mut boxes = Vec::new();
    rects(scene, &mut boxes);
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let holds = |r: &Rect| r.x <= cx && cx <= r.x + r.w && r.y <= cy && cy <= r.y + r.h;
    let clear = |stand: &Rect| !boxes.iter().any(|b| !holds(b) && overlap(b, stand));
    // Nearer, and smaller, where a neighbor leaves little room.
    let mut last = None;
    for size in [super::POINT, 8.0, 4.0] {
        let mut gap = OFF;
        while gap >= 0.0 {
            let stand = place(node.kind, container.direction, rect, band, out, gap, size);
            if clear(&stand) {
                return Some((stand, out));
            }
            last = Some(stand);
            gap -= 2.0;
        }
    }
    // No room at all: the line is laid to the point itself.
    let _ = last;
    None
}

/// Where a point of `kind` at `rect`, in a state whose states run
/// `direction` and whose title band is `band` high, stands for a line that
/// meets it from outside (`out`) or from inside: a square `size` large,
/// `gap` past the point's outer side, or its inner one (under the title
/// band for an entry where the states run down).
fn place(
    kind: Kind,
    direction: Direction,
    rect: Rect,
    band: f32,
    out: bool,
    gap: f32,
    size: f32,
) -> Rect {
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    // Toward the top or left (`true`), or the bottom or right.
    let before = (kind == Kind::Entry) == out;
    let (x, y) = match (direction, before) {
        (Direction::Down, true) => (cx - size / 2.0, rect.y - gap - size),
        (Direction::Down, false) if kind == Kind::Entry => (cx - size / 2.0, cy + band + gap),
        (Direction::Down, false) => (cx - size / 2.0, rect.y + rect.h + gap),
        (Direction::Right, true) => (rect.x - gap - size, cy - size / 2.0),
        (Direction::Right, false) => (rect.x + rect.w + gap, cy - size / 2.0),
    };
    Rect {
        x,
        y,
        w: size,
        h: size,
    }
}

/// How far past a point it stands for a line at most.
const OFF: f32 = 14.0;

/// Gives each point's name the first of its spots ([`super::point_names`])
/// where no line, box, words or other name is in its way, once every line
/// is laid; where none is free, the first.
pub(super) fn place_names(root: &Container, scene: &mut Scene) {
    let mut lines = Vec::new();
    let mut solids = Vec::new();
    gather(scene, &mut lines, &mut solids);
    let mut taken: Vec<Rect> = Vec::new();
    name(root, scene, None, &lines, &solids, &mut taken);
}

/// The state that holds points: its border and where its title's words
/// stand.
#[derive(Clone, Copy)]
struct Holder {
    rect: Rect,
    title: Rect,
}

impl Holder {
    fn of(placed: &Placed) -> Self {
        let rect = placed.rect;
        let w = stack_width(&placed.item.title, &placed.item.description);
        Holder {
            rect,
            title: Rect {
                x: rect.x + rect.w / 2.0 + placed.item.title_shift - w / 2.0,
                y: rect.y + super::HEAD_TOP,
                w,
                h: placed.item.header_h,
            },
        }
    }

    /// Whether a name at `spot`, for a point whose middle is `(cx, cy)`,
    /// runs into the title or across a border other than the point's.
    fn blocks(&self, spot: &Rect, (cx, cy): Point) -> bool {
        let r = self.rect;
        let sides = [
            (
                (cy - r.y).abs(),
                Rect {
                    x: r.x,
                    y: r.y,
                    w: r.w,
                    h: 0.0,
                },
            ),
            (
                (r.y + r.h - cy).abs(),
                Rect {
                    x: r.x,
                    y: r.y + r.h,
                    w: r.w,
                    h: 0.0,
                },
            ),
            (
                (cx - r.x).abs(),
                Rect {
                    x: r.x,
                    y: r.y,
                    w: 0.0,
                    h: r.h,
                },
            ),
            (
                (r.x + r.w - cx).abs(),
                Rect {
                    x: r.x + r.w,
                    y: r.y,
                    w: 0.0,
                    h: r.h,
                },
            ),
        ];
        let own = sides
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.0.total_cmp(&b.1.0))
            .map_or(0, |(i, _)| i);
        let crosses = |side: &Rect| {
            spot.x < side.x + side.w + 0.5
                && side.x - 0.5 < spot.x + spot.w
                && spot.y < side.y + side.h + 0.5
                && side.y - 0.5 < spot.y + spot.h
        };
        overlap(&self.title, spot)
            || sides
                .iter()
                .enumerate()
                .any(|(i, (_, side))| i != own && crosses(side))
    }
}

/// Every line drawn of `scene` and inside it into `lines`, every box, note
/// and words into `solids`.
fn gather(scene: &Scene, lines: &mut Vec<Vec<Point>>, solids: &mut Vec<Rect>) {
    for edge in &scene.edges {
        if edge.points.len() >= 2 && edge.look.width > 0.0 {
            lines.push(edge.points.clone());
        }
        if super::has_room(edge) {
            solids.push(Rect {
                x: edge.label_at.0,
                y: edge.label_at.1,
                w: edge.label.width,
                h: edge.label.height,
            });
        }
    }
    for note in &scene.notes {
        solids.push(Rect {
            x: note.x,
            y: note.y,
            w: note.w,
            h: note.h,
        });
    }
    for node in &scene.nodes {
        solids.push(node.rect);
        gather(&node.children, lines, solids);
    }
}

fn name(
    root: &Container,
    scene: &mut Scene,
    holder: Option<Holder>,
    lines: &[Vec<Point>],
    solids: &[Rect],
    taken: &mut Vec<Rect>,
) {
    for (node, placed) in root.nodes.iter().zip(scene.nodes.iter_mut()) {
        if matches!(node.kind, Kind::Entry | Kind::Exit) {
            let (cx, cy) = (
                placed.rect.x + placed.rect.w / 2.0,
                placed.rect.y + placed.rect.h / 2.0,
            );
            // What holds the point, and the point, are no obstacles.
            let holds = |r: &Rect| r.x <= cx && cx <= r.x + r.w && r.y <= cy && cy <= r.y + r.h;
            // What a name at `spot` runs into: nothing for a free spot; a
            // title, a border or another name weighs most, a box less, a
            // line least, so where no spot is free the name takes the one
            // it harms least.
            let harm = |spot: &Rect| {
                let crossed = lines
                    .iter()
                    .filter(|line| {
                        line.windows(2).any(|run| {
                            let (a, b) = (run[0], run[1]);
                            let r = Rect {
                                x: a.0.min(b.0) - 1.0,
                                y: a.1.min(b.1) - 1.0,
                                w: (a.0 - b.0).abs() + 2.0,
                                h: (a.1 - b.1).abs() + 2.0,
                            };
                            overlap(&r, spot)
                        })
                    })
                    .count();
                let hit = solids
                    .iter()
                    .filter(|r| !holds(r) && overlap(r, spot))
                    .count();
                let names = taken.iter().filter(|r| overlap(r, spot)).count();
                let held = usize::from(holder.is_some_and(|h| h.blocks(spot, (cx, cy))));
                50 * (held + names) + 20 * hit + 5 * crossed
            };
            let spots = super::point_names(placed);
            let spot = spots
                .iter()
                .min_by_key(|spot| harm(spot))
                .expect("a point has spots for its name");
            placed.item.name_at = Some((spot.x, spot.y));
            taken.push(*spot);
        }
        let inner = Holder::of(placed);
        name(
            &node.children,
            &mut placed.children,
            Some(inner),
            lines,
            solids,
            taken,
        );
    }
}

/// Every box of `scene` and of those inside it, into `out`.
fn rects(scene: &Scene, out: &mut Vec<Rect>) {
    for node in &scene.nodes {
        out.push(node.rect);
        rects(&node.children, out);
    }
}

fn overlap(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// The start (or the end) of the line `points`, laid to where its point
/// stood for it (`stand`), led on to the point at `rect`: along the line's
/// end run where that runs toward the point; else, its end run turned
/// onto the point, so it meets it square and not along the border.
pub(super) fn meet(points: &mut Vec<Point>, start: bool, stand: Rect, rect: Rect) {
    let n = points.len();
    if n < 2 {
        return;
    }
    let (end, next) = match start {
        true => (0, 1),
        false => (n - 1, n - 2),
    };
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let (sx, sy) = (stand.x + stand.w / 2.0, stand.y + stand.h / 2.0);
    // Off the point up or down (`true`), or to a side.
    let upright = (sy - cy).abs() >= (sx - cx).abs();
    // Where the line meets the point: the middle of its side toward
    // where it stood.
    let side = match upright {
        true => (cx, if sy < cy { rect.y } else { rect.y + rect.h }),
        false => (if sx < cx { rect.x } else { rect.x + rect.w }, cy),
    };
    let (p, q) = (points[end], points[next]);
    // The end run comes from where the point stood toward it, not back
    // over it.
    // The end run lies on the stand's side of the point, toward it.
    let toward = match upright {
        true => (p.0 - q.0).abs() < 0.5 && (q.1 - cy) * (sy - cy) > 0.0,
        false => (p.1 - q.1).abs() < 0.5 && (q.0 - cx) * (sx - cx) > 0.0,
    };
    if toward {
        points[end] = match upright {
            true => (p.0, side.1),
            false => (side.0, p.1),
        };
        return;
    }
    let turn = match upright {
        true => (side.0, p.1),
        false => (p.0, side.1),
    };
    points[end] = turn;
    match start {
        true => points.insert(0, side),
        false => points.push(side),
    }
}

/// The line `points` without a short step between two runs that lie side
/// by side (a jog, which the line takes to meet a point where it stood
/// for it): one of the two runs moves onto the other's line, where the
/// run beyond it lets it, so no run turns slant and the ends stay, and
/// where it then lies on none of the lines of `others`.
pub(super) fn straighten(points: &mut Vec<Point>, others: &[Vec<Point>]) {
    // Whether the run `a` to `b` would lie on another line.
    let on_another = |a: Point, b: Point| {
        others.iter().any(|line| {
            line.windows(2).any(|run| {
                let (c, d) = (run[0], run[1]);
                let across =
                    |u: f32, v: f32, s: f32, t: f32| u.min(v) < s.max(t) && s.min(t) < u.max(v);
                if (a.0 - b.0).abs() < 0.5 && (c.0 - d.0).abs() < 0.5 && (a.0 - c.0).abs() < 1.0 {
                    across(a.1, b.1, c.1, d.1)
                } else if (a.1 - b.1).abs() < 0.5
                    && (c.1 - d.1).abs() < 0.5
                    && (a.1 - c.1).abs() < 1.0
                {
                    across(a.0, b.0, c.0, d.0)
                } else {
                    false
                }
            })
        })
    };
    const JOG: f32 = 8.0;
    let flat = |a: Point, b: Point| (a.1 - b.1).abs() < 0.5;
    let upright = |a: Point, b: Point| (a.0 - b.0).abs() < 0.5;
    let parallel = |a: Point, b: Point, c: Point, d: Point| {
        (flat(a, b) && flat(c, d)) || (upright(a, b) && upright(c, d))
    };
    let mut i = 1;
    while i + 2 < points.len() {
        let (a, b, c, d) = (points[i - 1], points[i], points[i + 1], points[i + 2]);
        let step = (c.0 - b.0, c.1 - b.1);
        let short = step.0.abs().max(step.1.abs());
        if short < 0.5 || short >= JOG || !parallel(a, b, c, d) {
            i += 1;
            continue;
        }
        let along = |p: Point, q: Point| match flat(b, c) {
            true => flat(p, q),
            false => upright(p, q),
        };
        // The run after the step moves back onto the one before it, where
        // the run after it lies along the step; else the run before moves
        // on, where the run before it does. Never an end.
        let back = ((c.0 - step.0, c.1 - step.1), (d.0 - step.0, d.1 - step.1));
        let on = ((a.0 + step.0, a.1 + step.1), (b.0 + step.0, b.1 + step.1));
        if i + 3 < points.len() && along(d, points[i + 3]) && !on_another(back.0, back.1) {
            (points[i + 1], points[i + 2]) = back;
        } else if i >= 2 && along(points[i - 2], a) && !on_another(on.0, on.1) {
            (points[i - 1], points[i]) = on;
        } else {
            i += 1;
            continue;
        }
        // The step is gone: one point of the two it joined is left over,
        // and one between two runs in a line.
        points.dedup_by(|p, q| (p.0 - q.0).abs() < 0.5 && (p.1 - q.1).abs() < 0.5);
        let mut k = 1;
        while k + 1 < points.len() {
            let (p, q, r) = (points[k - 1], points[k], points[k + 1]);
            if (flat(p, q) && flat(q, r)) || (upright(p, q) && upright(q, r)) {
                points.remove(k);
            } else {
                k += 1;
            }
        }
        i = 1;
    }
}

/// The straight line from a point at `rect`, which stood at `stand` for
/// the line, to the box `other` it faces across the gap toward where it
/// stood, the box's side reaching across the point's line, and none of
/// `boxes` in between; `None` for any other.
pub(super) fn faces_box(
    rect: Rect,
    stand: Rect,
    other: Rect,
    boxes: &[Rect],
) -> Option<Vec<Point>> {
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let (sx, sy) = (stand.x + stand.w / 2.0, stand.y + stand.h / 2.0);
    let line = if (sy - cy).abs() < 0.5 {
        if !(other.y < cy && cy < other.y + other.h) {
            return None;
        }
        match sx > cx {
            true if other.x >= rect.x + rect.w => [(rect.x + rect.w, cy), (other.x, cy)],
            false if other.x + other.w <= rect.x => [(rect.x, cy), (other.x + other.w, cy)],
            _ => return None,
        }
    } else if (sx - cx).abs() < 0.5 {
        if !(other.x < cx && cx < other.x + other.w) {
            return None;
        }
        match sy > cy {
            true if other.y >= rect.y + rect.h => [(cx, rect.y + rect.h), (cx, other.y)],
            false if other.y + other.h <= rect.y => [(cx, rect.y), (cx, other.y + other.h)],
            _ => return None,
        }
    } else {
        return None;
    };
    let [a, b] = line;
    let run = Rect {
        x: a.0.min(b.0),
        y: a.1.min(b.1),
        w: (a.0 - b.0).abs().max(0.1),
        h: (a.1 - b.1).abs().max(0.1),
    };
    let between = boxes.iter().any(|r| {
        *r != other
            && *r != rect
            && overlap(r, &run)
            && !(r.x <= cx && cx <= r.x + r.w && r.y <= cy && cy <= r.y + r.h)
    });
    (!between).then(|| line.to_vec())
}

/// Every box of `scene` and of those inside it.
pub(super) fn all_rects(scene: &Scene) -> Vec<Rect> {
    let mut out = Vec::new();
    rects(scene, &mut out);
    out
}

/// Where words `size` large at `at` stand clear of the line `points`: where
/// they are, if the line runs past them; else beside the line's longest
/// run, right of an upright one and over a flat one.
pub(super) fn words_clear(points: &[Point], at: Point, (w, h): Point) -> Point {
    if w <= 0.0 || h <= 0.0 {
        return at;
    }
    let words = Rect {
        x: at.0,
        y: at.1,
        w,
        h,
    };
    let through = points.windows(2).any(|run| {
        let (a, b) = (run[0], run[1]);
        let r = Rect {
            x: a.0.min(b.0) - 0.5,
            y: a.1.min(b.1) - 0.5,
            w: (a.0 - b.0).abs() + 1.0,
            h: (a.1 - b.1).abs() + 1.0,
        };
        overlap(&r, &words)
    });
    if !through {
        return at;
    }
    let longest = points.windows(2).max_by(|r, s| {
        let len = |p: &[Point]| (p[0].0 - p[1].0).abs() + (p[0].1 - p[1].1).abs();
        len(r).total_cmp(&len(s))
    });
    match longest {
        Some(run) if (run[0].0 - run[1].0).abs() < 0.5 => {
            (run[0].0 + 4.0, (run[0].1 + run[1].1) / 2.0 - h / 2.0)
        }
        Some(run) => ((run[0].0 + run[1].0) / 2.0 - w / 2.0, run[0].1 - h - 2.0),
        None => at,
    }
}

/// The straight line between two points, each at its place and where it
/// stood for the line, that face each other on one line across the gap
/// between their states: from the side of the one to the side of the
/// other; `None` for any others.
pub(super) fn facing((ra, sa): (Rect, Rect), (rb, sb): (Rect, Rect)) -> Option<Vec<Point>> {
    let middle = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let ((ax, ay), (bx, by)) = (middle(ra), middle(rb));
    let (out_a, out_b) = (
        (middle(sa).0 - ax, middle(sa).1 - ay),
        (middle(sb).0 - bx, middle(sb).1 - by),
    );
    // Side by side, each stepping out toward the other.
    if (ay - by).abs() < 0.5
        && out_a.1 == 0.0
        && out_b.1 == 0.0
        && (bx - ax) * out_a.0 > 0.0
        && (ax - bx) * out_b.0 > 0.0
    {
        let (a, b) = if ax < bx {
            (ra.x + ra.w, rb.x)
        } else {
            (ra.x, rb.x + rb.w)
        };
        return Some(vec![(a, ay), (b, by)]);
    }
    if (ax - bx).abs() < 0.5
        && out_a.0 == 0.0
        && out_b.0 == 0.0
        && (by - ay) * out_a.1 > 0.0
        && (ay - by) * out_b.1 > 0.0
    {
        let (a, b) = if ay < by {
            (ra.y + ra.h, rb.y)
        } else {
            (ra.y, rb.y + rb.h)
        };
        return Some(vec![(ax, a), (bx, b)]);
    }
    None
}

/// Where the point at `path` of `root` stands in `scene`; `None` where no
/// point is.
pub(super) fn rect_at(root: &Container, scene: &Scene, path: &[usize]) -> Option<Rect> {
    let (last, holder) = path.split_last()?;
    let mut container = root;
    let mut laid = scene;
    for &index in holder {
        container = &container.nodes.get(index)?.children;
        laid = &laid.nodes.get(index)?.children;
    }
    let node = container.nodes.get(*last)?;
    if !matches!(node.kind, Kind::Entry | Kind::Exit) {
        return None;
    }
    laid.nodes.get(*last).map(|placed| placed.rect)
}

/// Moves the points of every state in `container`, laid out as `scene` at
/// `path` in the tree, onto its border; where each is, in `moved`.
fn lift(
    container: &Container,
    scene: &mut Scene,
    path: &mut Vec<usize>,
    moved: &mut Vec<Vec<usize>>,
) {
    // Room a state's own notes over and under it give its points' names.
    let mut room: Vec<(usize, f32, f32)> = Vec::new();
    for (index, (node, placed)) in container
        .nodes
        .iter()
        .zip(scene.nodes.iter_mut())
        .enumerate()
    {
        path.push(index);
        let border = placed.rect;
        let (inside, laid) = (&node.children, &mut placed.children);
        let (mut over, mut under) = (0.0_f32, 0.0_f32);
        for (at, (point, spot)) in inside.nodes.iter().zip(laid.nodes.iter_mut()).enumerate() {
            if !matches!(point.kind, Kind::Entry | Kind::Exit) {
                continue;
            }
            let (cx, cy) = (
                spot.rect.x + spot.rect.w / 2.0,
                spot.rect.y + spot.rect.h / 2.0,
            );
            let by = match (inside.direction, point.kind) {
                (Direction::Down, Kind::Entry) => (0.0, border.y - cy),
                (Direction::Down, _) => (0.0, border.y + border.h - cy),
                (Direction::Right, Kind::Entry) => (border.x - cx, 0.0),
                (Direction::Right, _) => (border.x + border.w - cx, 0.0),
            };
            spot.rect.x += by.0;
            spot.rect.y += by.1;
            let name = spot.item.title.height + 4.0;
            match point.kind {
                Kind::Entry => over = over.max(name),
                _ => under = under.max(name),
            }
            for (edge, line) in inside.edges.iter().zip(laid.edges.iter_mut()) {
                if edge.from == at && edge.to == at {
                    // A loop on the point moves with it as a whole.
                    for p in &mut line.points {
                        *p = (p.0 + by.0, p.1 + by.1);
                    }
                    line.label_at = (line.label_at.0 + by.0, line.label_at.1 + by.1);
                    continue;
                }
                if edge.from == at {
                    reach(&mut line.points, true, by);
                }
                if edge.to == at {
                    reach(&mut line.points, false, by);
                }
            }
            // Its notes keep the room the layout gave them, tied to it by a
            // dashed line, as a note that stands off its state is.
            let rect = spot.rect;
            for note in laid
                .notes
                .iter_mut()
                .filter(|n| n.target == at && n.on.is_none())
            {
                note.tether = Some(tether(note, rect));
            }
            moved.push([path.as_slice(), &[at]].concat());
        }
        lift(inside, laid, path, moved);
        if inside.direction == Direction::Down {
            clear_title(placed);
            room.push((index, over, under));
        }
        path.pop();
    }
    for (index, over, under) in room {
        for note in scene
            .notes
            .iter_mut()
            .filter(|n| n.target == index && n.on.is_none())
        {
            let dy = match note.side {
                Side::Top => -over,
                Side::Bottom => under,
                Side::Left | Side::Right => 0.0,
            };
            note.y += dy;
            // The note's end of its dashed line goes with it.
            if let Some((from, _)) = &mut note.tether {
                from.1 += dy;
            }
        }
    }
}

/// Moves the title of a state with entry points along its band to the
/// free stretch nearest its middle, so the lines from the points keep off
/// its words; where none is wide enough, it stays in the middle.
fn clear_title(placed: &mut Placed) {
    let rect = placed.rect;
    let blocked: Vec<(f32, f32)> = placed
        .children
        .nodes
        .iter()
        .filter(|node| node.item.kind == Kind::Entry)
        .map(|node| (node.rect.x - GAP, node.rect.x + node.rect.w + GAP))
        .collect();
    if blocked.is_empty() {
        return;
    }
    let half = stack_width(&placed.item.title, &placed.item.description) / 2.0;
    let middle = rect.x + rect.w / 2.0;
    let (low, high) = (rect.x + COMP_PAD + half, rect.x + rect.w - COMP_PAD - half);
    let free = |c: f32| blocked.iter().all(|&(a, b)| c + half <= a || c - half >= b);
    if free(middle) || low > high {
        return;
    }
    let best = blocked
        .iter()
        .flat_map(|&(a, b)| [b + half, a - half])
        .filter(|&c| (low..=high).contains(&c) && free(c))
        .min_by(|a, b| (a - middle).abs().total_cmp(&(b - middle).abs()));
    if let Some(c) = best {
        placed.item.title_shift = c - middle;
    }
}

/// Between a title and the line from an entry point beside it.
const GAP: f32 = 4.0;

/// The dashed line from `note` to the point `rect` it is about: from the
/// middle of the note's side toward the point to the point's nearest side.
fn tether(note: &super::PlacedNote, rect: Rect) -> (Point, Point) {
    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
    let (nx, ny) = (note.x + note.w / 2.0, note.y + note.h / 2.0);
    if (nx - cx).abs() * rect.h >= (ny - cy).abs() * rect.w {
        let (from_x, to_x) = match nx > cx {
            true => (note.x, rect.x + rect.w),
            false => (note.x + note.w, rect.x),
        };
        ((from_x, ny), (to_x, cy))
    } else {
        let (from_y, to_y) = match ny > cy {
            true => (note.y, rect.y + rect.h),
            false => (note.y + note.h, rect.y),
        };
        ((nx, from_y), (cx, to_y))
    }
}

/// The start (or the end) of the line `points` moved `by` with the point
/// it meets: along its end run where that runs the way the point moved;
/// across, the run slides with the point where the run before it lies
/// along the move; else by a run of its own.
fn reach(points: &mut Vec<Point>, start: bool, (dx, dy): Point) {
    let n = points.len();
    if n < 2 || (dx == 0.0 && dy == 0.0) {
        return;
    }
    let (end, next, before) = match start {
        true => (0, 1, 2),
        false => (n - 1, n - 2, n.wrapping_sub(3)),
    };
    // Whether `a` to `b` runs the way the point moved.
    let along = |a: Point, b: Point| match dy != 0.0 {
        true => (a.0 - b.0).abs() < 0.5,
        false => (a.1 - b.1).abs() < 0.5,
    };
    let moved = |p: Point| (p.0 + dx, p.1 + dy);
    let (p, q) = (points[end], points[next]);
    if along(p, q) {
        points[end] = moved(p);
    } else if let Some(&r) = points.get(before)
        && along(r, q)
    {
        points[end] = moved(p);
        points[next] = moved(q);
    } else if start {
        points.insert(0, moved(p));
    } else {
        points.push(moved(p));
    }
}

/// Takes the route from every transition between boxes that ends at a
/// point in `moved`, at any depth of `container`, laid out as `scene` at
/// `path`: [`super::nest::link_crossings`] lays them anew.
fn unroute(container: &Container, scene: &mut Scene, path: &mut Vec<usize>, moved: &[Vec<usize>]) {
    let ends = |end: &[usize]| {
        moved
            .iter()
            .any(|point| point.as_slice() == [path.as_slice(), end].concat())
    };
    let gone: Vec<usize> = container
        .crossings
        .iter()
        .enumerate()
        .filter(|(_, crossing)| ends(&crossing.from) || ends(&crossing.to))
        .map(|(index, _)| container.edges.len() + index)
        .collect();
    for slot in gone {
        if let Some(line) = scene.edges.get_mut(slot) {
            line.points.clear();
        }
    }
    for (index, (node, placed)) in container
        .nodes
        .iter()
        .zip(scene.nodes.iter_mut())
        .enumerate()
    {
        path.push(index);
        unroute(&node.children, &mut placed.children, path, moved);
        path.pop();
    }
}
