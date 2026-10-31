//! Right-angled routes around boxes, for the lines a container's layout
//! cannot draw itself: a transition from inside one nested state to
//! another, or a line a fallback layout drew through a box.
//!
//! A route runs on a grid of channels: past every obstacle at
//! [`CLEARANCE`], through every port, and midway between neighbors. A*
//! finds the cheapest one; length, bends, crossings, running under a
//! label and hugging a box all cost.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::Rect;

pub type Point = (f32, f32);

/// How far routes keep from boxes.
const CLEARANCE: f32 = 12.0;
/// Rounded corners: lines end at least this far from a corner.
const CORNER: f32 = 10.0;
/// Cost of a bend, in units of length.
const BEND: f32 = 30.0;
/// Cost of crossing another line.
const CROSSING: f32 = 60.0;
/// Cost of running straight on through a bend the writer placed: they
/// put it where the line is to turn.
const THROUGH: f32 = 3.0 * BEND;
/// Cost per unit of running under a label.
const UNDER_LABEL: f32 = 3.0;
/// Cost per unit of running on top of another line.
const SHARED: f32 = 6.0;
/// Extra cost per unit of running right along a box, falling to nothing
/// at [`CLEARANCE`].
const HUG: f32 = 3.0;
/// Lines ending on the same side of a box keep this far apart.
const PORT_GAP: f32 = 14.0;
/// The side of a cell of the lookup that finds what is near a step.
const NEAR_CELL: f32 = 48.0;

/// One end of a route. `flat` when a line may meet the box anywhere along
/// a side, as on a state; otherwise only in the middle of a side, as on a
/// circle or a diamond.
#[derive(Debug, Clone, Copy)]
pub struct End {
    pub rect: Rect,
    pub flat: bool,
}

/// What a route has to know about everything else, in one coordinate
/// space.
#[derive(Debug, Clone, Default)]
pub struct Surroundings {
    /// What the route must not pass through. The ends may be among them.
    pub obstacles: Vec<Rect>,
    /// Nested states the route goes into or out of: it crosses their
    /// border, as few times as it can, and does not run along it.
    pub frames: Vec<Rect>,
    /// Other lines: crossing one costs, running on top of one costs more.
    pub lines: Vec<(Point, Point)>,
    /// Labels a route should not run under.
    pub labels: Vec<Rect>,
    /// Where other lines start and end.
    pub used: Vec<Point>,
    /// Routes stay right of and below this.
    pub origin: Point,
}

/// A route at right angles from `from` to `to`, around everything in the
/// way, as cheap as its length, bends and crossings make it.
pub fn route(from: End, to: End, around: &Surroundings) -> Option<Vec<Point>> {
    route_via(from, to, &[], around)
}

/// A route as [`route`] lays it, through `vias` in their order: it may turn
/// at one, but never doubles back there. `None` when a via lies in a box,
/// or no way leads through them all.
pub fn route_via(from: End, to: End, vias: &[Point], around: &Surroundings) -> Option<Vec<Point>> {
    let mut scene = Scene::of(around);
    // A bend the writer put above or left of the drawing is reached too.
    for &(x, y) in vias {
        scene.origin.0 = scene.origin.0.min(x - 2.0 * CLEARANCE);
        scene.origin.1 = scene.origin.1.min(y - 2.0 * CLEARANCE);
    }
    let from = Side::of(from);
    let to = Side::of(to);
    let dot = |p: Point| Bounds::around(p, (0.0, 0.0));
    // Each end looks toward the bend next to it.
    let toward = |via: Option<&Point>, other: Side| {
        via.map_or(other, |&p| Side {
            rect: dot(p),
            flat: false,
        })
    };
    let starts = ports(from, toward(vias.first(), to), &scene);
    let goals = ports(to, toward(vias.last(), from), &scene);
    if starts.is_empty() || goals.is_empty() {
        return None;
    }
    let near = vias
        .iter()
        .fold(from.rect.union(to.rect), |r, &p| r.union(dot(p)))
        .grow(4.0 * CLEARANCE);
    let all = scene
        .obstacles
        .iter()
        .fold(near, |r, o| r.union(*o))
        .grow(2.0 * CLEARANCE);
    // Two heads on top of each other read worse than a detour: a place
    // too near another line's end is taken only when no free one is
    // left, or no free one leads anywhere.
    let (free_starts, free_goals) = (free(&starts), free(&goals));
    let mut tries = vec![(&free_starts, &free_goals)];
    if free_starts.len() < starts.len() || free_goals.len() < goals.len() {
        tries.push((&starts, &goals));
    }
    tries.into_iter().find_map(|(starts, goals)| {
        // Most routes stay near their ends; a small grid finds them faster.
        [near, all].into_iter().find_map(|window| {
            let window = Bounds {
                left: window.left.max(scene.origin.0),
                top: window.top.max(scene.origin.1),
                ..window
            };
            search(window, starts, goals, vias, &scene)
        })
    })
}

/// The ports away from where other lines end, or all if none is.
fn free(ports: &[Port]) -> Vec<Port> {
    let free: Vec<Port> = ports.iter().filter(|port| !port.crowded).copied().collect();
    if free.is_empty() {
        ports.to_vec()
    } else {
        free
    }
}

/// Whether the segment from `a` to `b` passes through the inside of
/// `rect`; running along the border does not count.
pub fn cuts(rect: Rect, a: Point, b: Point) -> bool {
    Bounds::of(rect).cut(a, b)
}

/// What a label must keep clear of.
#[derive(Debug, Clone, Copy)]
pub struct LabelRoom<'a> {
    /// Boxes, notes and the labels already placed.
    pub solids: &'a [Rect],
    /// Nested states: a label inside or outside one is fine, across its
    /// border it is not.
    pub frames: &'a [Rect],
    /// Every other line.
    pub lines: &'a [(Point, Point)],
    /// Labels stay right of and below this if they can.
    pub origin: Point,
    /// Whether a label may sit on its line. Only one drawn on a
    /// background may; otherwise the line would strike it through.
    pub on_line: bool,
}

/// The center of the best place along `points` for a label of `size`:
/// near the middle, covering nothing, crossing no other line. `keep` is
/// where it is now, which wins a tie.
pub fn label_spot(
    points: &[Point],
    size: (f32, f32),
    room: &LabelRoom,
    keep: Option<Point>,
) -> Option<Point> {
    let &first = points.first()?;
    let solids: Vec<Bounds> = room.solids.iter().map(|r| Bounds::of(*r)).collect();
    let frames: Vec<Bounds> = room.frames.iter().map(|r| Bounds::of(*r)).collect();
    let covered = |rect: Bounds| -> f32 {
        // A little air around boxes and labels.
        let solid: f32 = solids.iter().map(|s| s.grow(2.0).overlap(rect)).sum();
        let border: f32 = frames
            .iter()
            .filter(|g| !g.encloses(rect))
            .map(|g| g.overlap(rect))
            .sum();
        solid + border
    };
    // Only lines near this one can meet its label.
    let dot = |p: Point| Bounds::around(p, (0.0, 0.0));
    let reach = points
        .iter()
        .fold(dot(first), |r, &p| r.union(dot(p)))
        .grow(size.0.max(size.1));
    let lines: Vec<(Point, Point)> = room
        .lines
        .iter()
        .copied()
        .filter(|&(a, b)| reach.cut(a, b))
        .collect();
    let total: f32 = segments(points).map(|(a, b)| distance(a, b)).sum();
    let score = |(at, along, beside): (Point, f32, bool)| {
        let rect = Bounds::around(at, size);
        let covered = covered(rect);
        // Beside a line and across another, a label seems to belong to
        // the other.
        let crossed = lines.iter().filter(|&&(a, b)| rect.cut(a, b)).count() as f32
            * if beside { 5.0 } else { 1.0 };
        // Clear of the arrowhead and the start; inside the picture if that
        // is as good, since the picture grows to make room.
        let ends = along < 4.0 || along > total - 16.0;
        let outside = rect.left < room.origin.0 || rect.top < room.origin.1;
        covered * 20.0
            + crossed * CROSSING
            + if ends { 2.0 * CROSSING } else { 0.0 }
            + if outside { CROSSING / 2.0 } else { 0.0 }
            + (along - total / 2.0).abs() * 0.5
            + if beside { 15.0 } else { 0.0 }
    };
    let current = keep.map(|at| (at, total / 2.0, false));
    spots(points, size)
        .filter(|&(_, _, beside)| beside || room.on_line)
        .chain(current)
        .min_by(|&a, &b| score(a).total_cmp(&score(b)))
        .map(|(at, ..)| at)
}

/// Labels a layout put on another label, on a box or across another line
/// take the best place along their own line instead; the others stay.
/// `labels` holds the label of each of `lines`: its top left and its
/// size, zero for none. Returns the room a scene of `size` takes with its
/// labels: a label moved left of or above it starts that room before
/// `(0, 0)`, and the scene moves along by the negated corner.
pub fn place_labels(
    lines: &[Vec<Point>],
    labels: &mut [(Point, (f32, f32))],
    boxes: &[Rect],
    (width, height): (f32, f32),
) -> Rect {
    let mut placed: Vec<Rect> = Vec::new();
    for index in 0..labels.len().min(lines.len()) {
        let ((x, y), (w, h)) = labels[index];
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        let others: Vec<(Point, Point)> = lines
            .iter()
            .enumerate()
            .filter(|&(other, _)| other != index)
            .flat_map(|(_, line)| segments(line).collect::<Vec<_>>())
            .collect();
        let rect = Rect { x, y, w, h };
        // A little air around boxes and labels.
        let near = Bounds::of(rect).grow(2.0);
        let clash = placed
            .iter()
            .chain(boxes)
            .any(|&other| near.overlap(Bounds::of(other)) > 0.0)
            || others.iter().any(|&(a, b)| cuts(rect, a, b));
        if clash {
            let solids: Vec<Rect> = boxes.iter().chain(&placed).copied().collect();
            let room = LabelRoom {
                solids: &solids,
                frames: &[],
                lines: &others,
                origin: (0.0, 0.0),
                on_line: false,
            };
            let keep = (x + w / 2.0, y + h / 2.0);
            if let Some((cx, cy)) = label_spot(&lines[index], (w, h), &room, Some(keep)) {
                labels[index].0 = (cx - w / 2.0, cy - h / 2.0);
            }
        }
        let ((x, y), _) = labels[index];
        placed.push(Rect { x, y, w, h });
    }
    let (mut left, mut top) = (0.0f32, 0.0f32);
    let (mut right, mut bottom) = (width, height);
    for &((x, y), (w, h)) in labels.iter() {
        if w > 0.0 && h > 0.0 {
            (left, top) = (left.min(x), top.min(y));
            (right, bottom) = (right.max(x + w), bottom.max(y + h));
        }
    }
    Rect {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

/// A direction on screen, and the side of a box a line leaves it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    const ALL: [Self; 4] = [Self::Up, Self::Down, Self::Left, Self::Right];

    fn outward(self) -> Point {
        match self {
            Self::Up => (0.0, -1.0),
            Self::Down => (0.0, 1.0),
            Self::Left => (-1.0, 0.0),
            Self::Right => (1.0, 0.0),
        }
    }

    fn reverse(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// The axis along a side facing this way.
    fn along(self) -> Axis {
        match self {
            Self::Up | Self::Down => Axis::X,
            Self::Left | Self::Right => Axis::Y,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// A box by its sides, which is what the search works with.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Bounds {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Bounds {
    fn of(rect: Rect) -> Self {
        Self {
            left: rect.x,
            top: rect.y,
            right: rect.x + rect.w,
            bottom: rect.y + rect.h,
        }
    }

    fn around(center: Point, (width, height): (f32, f32)) -> Self {
        Self {
            left: center.0 - width / 2.0,
            top: center.1 - height / 2.0,
            right: center.0 + width / 2.0,
            bottom: center.1 + height / 2.0,
        }
    }

    fn grow(self, by: f32) -> Self {
        Self {
            left: self.left - by,
            top: self.top - by,
            right: self.right + by,
            bottom: self.bottom + by,
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    fn center(self) -> Point {
        (
            (self.left + self.right) / 2.0,
            (self.top + self.bottom) / 2.0,
        )
    }

    fn span(self, axis: Axis) -> (f32, f32) {
        match axis {
            Axis::X => (self.left, self.right),
            Axis::Y => (self.top, self.bottom),
        }
    }

    fn encloses(self, other: Self) -> bool {
        other.left >= self.left - 0.5
            && other.top >= self.top - 0.5
            && other.right <= self.right + 0.5
            && other.bottom <= self.bottom + 0.5
    }

    fn overlap(self, other: Self) -> f32 {
        let width = self.right.min(other.right) - self.left.max(other.left);
        let height = self.bottom.min(other.bottom) - self.top.max(other.top);
        width.max(0.0) * height.max(0.0)
    }

    fn cut(self, a: Point, b: Point) -> bool {
        let inside = self.grow(-0.75);
        if inside.left >= inside.right || inside.top >= inside.bottom {
            return false;
        }
        // Liang–Barsky clipping.
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let (mut enter, mut leave) = (0.0f32, 1.0f32);
        for (p, q) in [
            (-dx, a.0 - inside.left),
            (dx, inside.right - a.0),
            (-dy, a.1 - inside.top),
            (dy, inside.bottom - a.1),
        ] {
            if p == 0.0 {
                if q < 0.0 {
                    return false;
                }
            } else if p < 0.0 {
                enter = enter.max(q / p);
            } else {
                leave = leave.min(q / p);
            }
        }
        enter < leave
    }
}

/// An end, by its sides.
#[derive(Debug, Clone, Copy)]
struct Side {
    rect: Bounds,
    flat: bool,
}

impl Side {
    fn of(end: End) -> Self {
        Self {
            rect: Bounds::of(end.rect),
            flat: end.flat,
        }
    }
}

/// [`Surroundings`] by sides.
struct Scene {
    obstacles: Vec<Bounds>,
    frames: Vec<Bounds>,
    lines: Vec<(Point, Point)>,
    labels: Vec<Bounds>,
    used: Vec<Point>,
    origin: Point,
}

impl Scene {
    fn of(around: &Surroundings) -> Self {
        Self {
            obstacles: around.obstacles.iter().map(|r| Bounds::of(*r)).collect(),
            frames: around.frames.iter().map(|r| Bounds::of(*r)).collect(),
            lines: around.lines.clone(),
            labels: around.labels.iter().map(|r| Bounds::of(*r)).collect(),
            used: around.used.clone(),
            origin: around.origin,
        }
    }
}

fn segments(points: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    points.windows(2).map(|w| (w[0], w[1]))
}

/// Where a line may leave or enter a box, and what choosing it costs.
#[derive(Debug, Clone, Copy)]
struct Port {
    at: Point,
    side: Dir,
    cost: f32,
    /// Nearer than [`PORT_GAP`] to where another line ends.
    crowded: bool,
}

/// The part of the sides along `axis` where lines may meet `end`.
fn usable(end: Side, axis: Axis) -> (f32, f32) {
    let (lo, hi) = end.rect.span(axis);
    if end.flat {
        let inset = CORNER.min((hi - lo) / 2.0);
        (lo + inset, hi - inset)
    } else {
        let mid = (lo + hi) / 2.0;
        (mid, mid)
    }
}

/// Places on `end` for a line to or from `other`.
fn ports(end: Side, other: Side, scene: &Scene) -> Vec<Port> {
    let mut ports = Vec::new();
    for side in Dir::ALL {
        let axis = side.along();
        let (lo, hi) = end.rect.span(axis);
        let mid = (lo + hi) / 2.0;
        let (a, b) = usable(end, axis);
        let (c, d) = usable(other, axis);
        let mut spots = vec![
            mid,
            (c + d) / 2.0,
            lo + (hi - lo) / 3.0,
            hi - (hi - lo) / 3.0,
        ];
        // Where both ends can meet the same line, so it can run straight:
        // the middle of that stretch, and its ends for when the middle is
        // taken.
        if a.max(c) <= b.min(d) {
            spots.extend([a.max(c), (a.max(c) + b.min(d)) / 2.0, b.min(d)]);
        }
        let mut spots: Vec<f32> = spots.into_iter().map(|s| s.clamp(a, b)).collect();
        spots.sort_by(f32::total_cmp);
        spots.dedup_by(|x, y| (*x - *y).abs() < 1.0);
        for pos in spots {
            let at = match side {
                Dir::Up => (pos, end.rect.top),
                Dir::Down => (pos, end.rect.bottom),
                Dir::Left => (end.rect.left, pos),
                Dir::Right => (end.rect.right, pos),
            };
            if clear_distance(at, side, end.rect, &scene.obstacles) < 4.0 {
                continue;
            }
            let crowded = scene
                .used
                .iter()
                .any(|u| (u.0 - at.0).abs() + (u.1 - at.1).abs() < PORT_GAP);
            // Lines leave toward where they go and arrive from where they
            // come, unless that costs a detour.
            let (ux, uy) = side.outward();
            let (cx, cy) = end.rect.center();
            let (ox, oy) = other.rect.center();
            let away = (ox - cx) * ux + (oy - cy) * uy <= 0.0;
            ports.push(Port {
                at,
                side,
                cost: (pos - mid).abs() * 0.2
                    + if crowded { CROSSING } else { 0.0 }
                    + if away { BEND } else { 0.0 },
                crowded,
            });
        }
    }
    ports
}

/// How far a line can run from `at` on `own`'s border toward `side`
/// before it meets something.
fn clear_distance(at: Point, side: Dir, own: Bounds, obstacles: &[Bounds]) -> f32 {
    let (x, y) = at;
    obstacles
        .iter()
        .filter(|r| **r != own)
        .filter_map(|r| {
            let across_x = r.left < x && x < r.right;
            let across_y = r.top < y && y < r.bottom;
            match side {
                Dir::Down if across_x && r.top >= y - 0.5 => Some(r.top - y),
                Dir::Up if across_x && r.bottom <= y + 0.5 => Some(y - r.bottom),
                Dir::Right if across_y && r.left >= x - 0.5 => Some(r.left - x),
                Dir::Left if across_y && r.right <= x + 0.5 => Some(x - r.right),
                _ => None,
            }
        })
        .fold(f32::INFINITY, f32::min)
        .max(0.0)
}

/// Lines a route may run on: past every box at [`CLEARANCE`], through
/// every port, and midway between neighbors.
fn channels(mut lines: Vec<f32>, lo: f32, hi: f32) -> Vec<f32> {
    lines.retain(|c| (lo..=hi).contains(c));
    lines.sort_by(f32::total_cmp);
    lines.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    let mids: Vec<f32> = lines
        .windows(2)
        .filter(|w| w[1] - w[0] > 2.0)
        .map(|w| (w[0] + w[1]) / 2.0)
        .collect();
    lines.extend(mids);
    lines.sort_by(f32::total_cmp);
    lines
}

/// What running from `a` to `b` costs besides bends, with `near` what
/// lies close to it.
fn step_cost(a: Point, b: Point, scene: &Scene, near: &[Item]) -> f32 {
    let length = (b.0 - a.0).abs() + (b.1 - a.1).abs();
    let mut crossings = 0;
    let mut shared_run = 0.0;
    let mut under_run = 0.0;
    let mut hugging_run = 0.0;
    for &item in near {
        match item {
            Item::Line(i) => {
                let (c, d) = scene.lines[i];
                crossings += usize::from(crosses(a, b, c, d));
                shared_run += shared(a, b, c, d);
            }
            Item::Label(i) => under_run += under(a, b, scene.labels[i]),
            Item::Obstacle(i) => hugging_run += hugging(a, b, scene.obstacles[i]),
            Item::Frame(i) => {
                let r = scene.frames[i];
                hugging_run += along_border(a, b, r);
                crossings += border(r)
                    .into_iter()
                    .filter(|&(c, d)| crosses(a, b, c, d))
                    .count();
            }
        }
    }
    length
        + crossings as f32 * CROSSING
        + shared_run * SHARED
        + under_run * UNDER_LABEL
        + hugging_run * HUG
}

/// Something a step may have to weigh, by its index in the [`Scene`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Item {
    Line(usize),
    Label(usize),
    Obstacle(usize),
    Frame(usize),
}

/// What lies near each cell of a coarse grid over the window, so a step
/// weighs only what is close: nothing a step costs or is blocked by
/// reaches further than [`CLEARANCE`] from it.
struct Near {
    left: f32,
    top: f32,
    cell: f32,
    nx: usize,
    ny: usize,
    cells: Vec<Vec<Item>>,
}

impl Near {
    fn of(scene: &Scene, window: Bounds) -> Self {
        let (w, h) = (window.right - window.left, window.bottom - window.top);
        let cell = NEAR_CELL.max(w.max(h) / 128.0);
        let nx = ((w / cell).ceil() as usize).max(1);
        let ny = ((h / cell).ceil() as usize).max(1);
        let mut near = Self {
            left: window.left,
            top: window.top,
            cell,
            nx,
            ny,
            cells: vec![Vec::new(); nx * ny],
        };
        let span = |(a, b): (Point, Point), by: f32| {
            Bounds {
                left: a.0.min(b.0),
                top: a.1.min(b.1),
                right: a.0.max(b.0),
                bottom: a.1.max(b.1),
            }
            .grow(by)
        };
        for (i, &line) in scene.lines.iter().enumerate() {
            near.insert(Item::Line(i), span(line, 2.0));
        }
        for (i, &r) in scene.labels.iter().enumerate() {
            near.insert(Item::Label(i), r.grow(1.0));
        }
        for (i, &r) in scene.obstacles.iter().enumerate() {
            near.insert(Item::Obstacle(i), r.grow(CLEARANCE));
        }
        for (i, &r) in scene.frames.iter().enumerate() {
            for side in border(r) {
                near.insert(Item::Frame(i), span(side, CLEARANCE));
            }
        }
        near
    }

    /// The cells `b` covers, clamped to the grid.
    fn cells_of(&self, b: Bounds) -> (usize, usize, usize, usize) {
        let col =
            |x: f32| (((x - self.left) / self.cell).floor().max(0.0) as usize).min(self.nx - 1);
        let row =
            |y: f32| (((y - self.top) / self.cell).floor().max(0.0) as usize).min(self.ny - 1);
        (col(b.left), col(b.right), row(b.top), row(b.bottom))
    }

    fn insert(&mut self, item: Item, b: Bounds) {
        let (c0, c1, r0, r1) = self.cells_of(b);
        for row in r0..=r1 {
            for col in c0..=c1 {
                let cell = &mut self.cells[row * self.nx + col];
                if cell.last() != Some(&item) {
                    cell.push(item);
                }
            }
        }
    }

    /// Everything near the run from `a` to `b`, once each.
    fn around(&self, a: Point, b: Point, out: &mut Vec<Item>) {
        out.clear();
        let b = Bounds {
            left: a.0.min(b.0),
            top: a.1.min(b.1),
            right: a.0.max(b.0),
            bottom: a.1.max(b.1),
        };
        let (c0, c1, r0, r1) = self.cells_of(b);
        for row in r0..=r1 {
            for col in c0..=c1 {
                out.extend_from_slice(&self.cells[row * self.nx + col]);
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

/// The four sides of `r`.
fn border(r: Bounds) -> [(Point, Point); 4] {
    let (tl, tr) = ((r.left, r.top), (r.right, r.top));
    let (bl, br) = ((r.left, r.bottom), (r.right, r.bottom));
    [(tl, tr), (tr, br), (br, bl), (bl, tl)]
}

/// How much of `a`–`b` runs along a side of the frame `r`, inside or
/// outside, closer than [`CLEARANCE`], weighted by how close.
fn along_border(a: Point, b: Point, r: Bounds) -> f32 {
    let (along, at, sides, span) = if a.1 == b.1 {
        (
            (a.0.min(b.0), a.0.max(b.0)),
            a.1,
            (r.top, r.bottom),
            r.span(Axis::X),
        )
    } else {
        (
            (a.1.min(b.1), a.1.max(b.1)),
            a.0,
            (r.left, r.right),
            r.span(Axis::Y),
        )
    };
    let beside = (along.1.min(span.1) - along.0.max(span.0)).max(0.0);
    let gap = (at - sides.0).abs().min((at - sides.1).abs());
    beside * (1.0 - gap / CLEARANCE).max(0.0)
}

/// How much of the straight run `a`–`b` lies inside `r`.
fn under(a: Point, b: Point, r: Bounds) -> f32 {
    let overlap =
        |p: f32, q: f32, (lo, hi): (f32, f32)| (p.max(q).min(hi) - p.min(q).max(lo)).max(0.0);
    let (x0, x1) = r.span(Axis::X);
    let (y0, y1) = r.span(Axis::Y);
    if a.1 == b.1 && y0 < a.1 && a.1 < y1 {
        overlap(a.0, b.0, (x0, x1))
    } else if a.0 == b.0 && x0 < a.0 && a.0 < x1 {
        overlap(a.1, b.1, (y0, y1))
    } else {
        0.0
    }
}

/// How much of `a`–`b` runs alongside `r` closer than [`CLEARANCE`],
/// weighted by how close.
fn hugging(a: Point, b: Point, r: Bounds) -> f32 {
    let (along, across, at) = if a.1 == b.1 {
        ((a.0.min(b.0), a.0.max(b.0)), r.span(Axis::Y), a.1)
    } else {
        ((a.1.min(b.1), a.1.max(b.1)), r.span(Axis::X), a.0)
    };
    let (side_lo, side_hi) = if a.1 == b.1 {
        r.span(Axis::X)
    } else {
        r.span(Axis::Y)
    };
    let beside = (along.1.min(side_hi) - along.0.max(side_lo)).max(0.0);
    let gap = if at <= across.0 {
        across.0 - at
    } else if at >= across.1 {
        at - across.1
    } else {
        0.0
    };
    beside * (1.0 - gap / CLEARANCE).max(0.0)
}

/// Whether `a`–`b` crosses the inside of `c`–`d`; `b` itself does not count,
/// so a crossing at a corner of a route counts once.
fn crosses(a: Point, b: Point, c: Point, d: Point) -> bool {
    let cross = |u: Point, v: Point| u.0 * v.1 - u.1 * v.0;
    let r = (b.0 - a.0, b.1 - a.1);
    let s = (d.0 - c.0, d.1 - c.1);
    let denom = cross(r, s);
    if denom.abs() < 1e-6 {
        return false;
    }
    let ac = (c.0 - a.0, c.1 - a.1);
    let t = cross(ac, s) / denom;
    let u = cross(ac, r) / denom;
    (0.0..1.0).contains(&t) && u > 0.01 && u < 0.99
}

/// How long `a`–`b` runs on top of `c`–`d`.
fn shared(a: Point, b: Point, c: Point, d: Point) -> f32 {
    let overlap =
        |p: f32, q: f32, r: f32, s: f32| (p.max(q).min(r.max(s)) - p.min(q).max(r.min(s))).max(0.0);
    if a.1 == b.1 && c.1 == d.1 && (a.1 - c.1).abs() < 2.0 {
        overlap(a.0, b.0, c.0, d.0)
    } else if a.0 == b.0 && c.0 == d.0 && (a.0 - c.0).abs() < 2.0 {
        overlap(a.1, b.1, c.1, d.1)
    } else {
        0.0
    }
}

#[derive(PartialEq)]
struct Entry {
    /// Cost so far plus the least that is left.
    estimate: f32,
    cost: f32,
    state: usize,
}

impl Eq for Entry {}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        // The cheapest first out of the max-heap.
        other.estimate.total_cmp(&self.estimate)
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The routing grid: lines a route may run on, and what stepping from one
/// crossing of them to the next costs, worked out when first needed.
struct Grid<'a> {
    xs: Vec<f32>,
    ys: Vec<f32>,
    scene: &'a Scene,
    near: Near,
    /// What is near the step being weighed.
    close: Vec<Item>,
    free: Vec<Option<bool>>,
    /// Cost of the step right of and below each point; `Some(None)` if blocked.
    right: Vec<Option<Option<f32>>>,
    down: Vec<Option<Option<f32>>>,
}

impl<'a> Grid<'a> {
    fn new(
        window: Bounds,
        starts: &[Port],
        goals: &[Port],
        vias: &[Point],
        scene: &'a Scene,
    ) -> Self {
        let mut xs = vec![window.left, window.right];
        let mut ys = vec![window.top, window.bottom];
        for r in &scene.obstacles {
            xs.extend([r.left - CLEARANCE, r.right + CLEARANCE]);
            ys.extend([r.top - CLEARANCE, r.bottom + CLEARANCE]);
        }
        // Clear of a frame's border on both sides.
        for r in &scene.frames {
            xs.extend([r.left - CLEARANCE, r.left + CLEARANCE]);
            xs.extend([r.right - CLEARANCE, r.right + CLEARANCE]);
            ys.extend([r.top - CLEARANCE, r.top + CLEARANCE]);
            ys.extend([r.bottom - CLEARANCE, r.bottom + CLEARANCE]);
        }
        for port in starts.iter().chain(goals) {
            xs.push(port.at.0);
            ys.push(port.at.1);
        }
        for &(x, y) in vias {
            xs.push(x);
            ys.push(y);
        }
        let xs = channels(xs, window.left, window.right);
        let ys = channels(ys, window.top, window.bottom);
        let n = xs.len() * ys.len();
        Self {
            xs,
            ys,
            scene,
            near: Near::of(scene, window),
            close: Vec::new(),
            free: vec![None; n],
            right: vec![None; n],
            down: vec![None; n],
        }
    }

    fn len(&self) -> usize {
        self.xs.len() * self.ys.len()
    }

    fn point(&self, k: usize) -> Point {
        let nx = self.xs.len();
        (self.xs[k % nx], self.ys[k / nx])
    }

    fn find(&self, p: Point) -> Option<usize> {
        let i = self.xs.iter().position(|&x| (x - p.0).abs() < 0.01)?;
        let j = self.ys.iter().position(|&y| (y - p.1).abs() < 0.01)?;
        Some(j * self.xs.len() + i)
    }

    fn free(&mut self, k: usize) -> bool {
        if let Some(free) = self.free[k] {
            return free;
        }
        let p = self.point(k);
        self.near.around(p, p, &mut self.close);
        let free = !self.blocked(p, p);
        self.free[k] = Some(free);
        free
    }

    /// Whether a box near the run from `a` to `b`, as last looked up,
    /// stands in its way.
    fn blocked(&self, a: Point, b: Point) -> bool {
        self.close.iter().any(|item| match *item {
            Item::Obstacle(i) => self.scene.obstacles[i].cut(a, b),
            _ => false,
        })
    }

    /// Where a step from `k` toward `dir` leads, and its cost.
    fn step(&mut self, k: usize, dir: Dir) -> Option<(usize, f32)> {
        let nx = self.xs.len();
        let (from, to, horizontal) = match dir {
            Dir::Right if !(k + 1).is_multiple_of(nx) => (k, k + 1, true),
            Dir::Left if !k.is_multiple_of(nx) => (k - 1, k, true),
            Dir::Down if k + nx < self.len() => (k, k + nx, false),
            Dir::Up if k >= nx => (k - nx, k, false),
            _ => return None,
        };
        let known = if horizontal {
            self.right[from]
        } else {
            self.down[from]
        };
        let cost = match known {
            Some(cost) => cost,
            None => {
                let (a, b) = (self.point(from), self.point(to));
                let open = self.free(from) && self.free(to) && {
                    self.near.around(a, b, &mut self.close);
                    !self.blocked(a, b)
                };
                let cost = open.then(|| step_cost(a, b, self.scene, &self.close));
                if horizontal {
                    self.right[from] = Some(cost);
                } else {
                    self.down[from] = Some(cost);
                }
                cost
            }
        };
        let next = if to == k { from } else { to };
        cost.map(|c| (next, c))
    }
}

/// The cheapest route inside `window` from one of `starts` to one of
/// `goals` through `vias`, on a grid of channels; A* over (vias passed,
/// grid point, heading).
fn search(
    window: Bounds,
    starts: &[Port],
    goals: &[Port],
    vias: &[Point],
    scene: &Scene,
) -> Option<Vec<Point>> {
    // Only what is near the window can matter.
    let near = window.grow(CLEARANCE);
    let touches = |r: &Bounds| {
        r.left <= near.right && r.right >= near.left && r.top <= near.bottom && r.bottom >= near.top
    };
    let span = |&(a, b): &(Point, Point)| Bounds {
        left: a.0.min(b.0),
        top: a.1.min(b.1),
        right: a.0.max(b.0),
        bottom: a.1.max(b.1),
    };
    let local = Scene {
        obstacles: scene.obstacles.iter().copied().filter(touches).collect(),
        frames: scene.frames.iter().copied().filter(touches).collect(),
        lines: scene
            .lines
            .iter()
            .copied()
            .filter(|l| touches(&span(l)))
            .collect(),
        labels: scene.labels.iter().copied().filter(touches).collect(),
        used: Vec::new(),
        origin: scene.origin,
    };
    let mut grid = Grid::new(window, starts, goals, vias, &local);
    let manhattan = |a: Point, b: Point| (a.0 - b.0).abs() + (a.1 - b.1).abs();
    let to_goal = |p: Point| {
        goals
            .iter()
            .map(|g| manhattan(g.at, p))
            .fold(f32::INFINITY, f32::min)
    };
    // From each via on, at least: to the next ones in turn, then a goal.
    let mut rest = vec![0.0; vias.len()];
    for s in (0..vias.len()).rev() {
        rest[s] = match vias.get(s + 1) {
            Some(&next) => manhattan(vias[s], next) + rest[s + 1],
            None => to_goal(vias[s]),
        };
    }
    let left = |p: Point, stage: usize| match vias.get(stage) {
        Some(&via) => manhattan(p, via) + rest[stage],
        None => to_goal(p),
    };
    let via_at: Vec<usize> = vias.iter().map(|&v| grid.find(v)).collect::<Option<_>>()?;
    let points = grid.len();
    let state_of = |stage: usize, k: usize, dir: Dir| (stage * points + k) * 4 + dir.index();

    let goal = (vias.len() + 1) * points * 4;
    let mut cost = vec![f32::INFINITY; goal + 1];
    let mut came: Vec<Option<usize>> = vec![None; goal + 1];
    let mut heap = BinaryHeap::new();
    // Lines leave and enter boxes straight; they never turn on a border.
    let mut on_border = vec![false; grid.len()];
    for port in starts {
        let Some(k) = grid.find(port.at) else {
            continue;
        };
        on_border[k] = true;
        let state = state_of(0, k, port.side);
        if port.cost < cost[state] {
            cost[state] = port.cost;
            heap.push(Entry {
                estimate: port.cost + left(port.at, 0),
                cost: port.cost,
                state,
            });
        }
    }
    let arrivals: Vec<(usize, &Port)> = goals
        .iter()
        .filter_map(|port| Some((grid.find(port.at)?, port)))
        .collect();
    for &(k, _) in &arrivals {
        on_border[k] = true;
    }

    while let Some(Entry { cost: c, state, .. }) = heap.pop() {
        if c > cost[state] {
            continue;
        }
        if state == goal {
            break;
        }
        let (stage, k, heading) = (state / 4 / points, state / 4 % points, Dir::ALL[state % 4]);
        // On its via, a route goes on as one that has passed it.
        if via_at.get(stage) == Some(&k) {
            let next = state_of(stage + 1, k, heading);
            if c < cost[next] {
                cost[next] = c;
                came[next] = Some(state);
                heap.push(Entry {
                    estimate: c + left(grid.point(k), stage + 1),
                    cost: c,
                    state: next,
                });
            }
            continue;
        }
        let arriving = if stage == vias.len() {
            &arrivals[..]
        } else {
            &[]
        };
        for &(_, port) in arriving.iter().filter(|&&(at, _)| at == k) {
            let total = c + port.cost;
            if heading == port.side.reverse() && total < cost[goal] {
                cost[goal] = total;
                came[goal] = Some(state);
                heap.push(Entry {
                    estimate: total,
                    cost: total,
                    state: goal,
                });
            }
        }
        for dir in Dir::ALL {
            if dir == heading.reverse() || (on_border[k] && dir != heading) {
                continue;
            }
            let Some((l, step)) = grid.step(k, dir) else {
                continue;
            };
            let next = state_of(stage, l, dir);
            let through = stage > 0 && via_at[stage - 1] == k && dir == heading;
            let total = c
                + step
                + if dir == heading { 0.0 } else { BEND }
                + if through { THROUGH } else { 0.0 };
            if total < cost[next] {
                cost[next] = total;
                came[next] = Some(state);
                heap.push(Entry {
                    estimate: total + left(grid.point(l), stage),
                    cost: total,
                    state: next,
                });
            }
        }
    }

    let mut state = came[goal]?;
    let mut path = Vec::new();
    loop {
        path.push(grid.point(state / 4 % points));
        match came[state] {
            Some(previous) => state = previous,
            None => break,
        }
    }
    path.reverse();
    Some(straighten(path))
}

/// Drops points in the middle of straight runs.
fn straighten(mut points: Vec<Point>) -> Vec<Point> {
    points.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01);
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if let [.., a, b] = out[..]
            && (((a.0 - b.0).abs() < 0.01 && (b.0 - p.0).abs() < 0.01)
                || ((a.1 - b.1).abs() < 0.01 && (b.1 - p.1).abs() < 0.01))
        {
            out.pop();
        }
        out.push(p);
    }
    out
}

fn distance(a: Point, b: Point) -> f32 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

/// Candidate places for a label of `size` along a line: on it, and beside
/// it where it runs straight; with how far along each lies, and whether it
/// is beside.
fn spots(points: &[Point], size: (f32, f32)) -> impl Iterator<Item = (Point, f32, bool)> + '_ {
    let mut along = 0.0;
    segments(points).flat_map(move |(a, b)| {
        let start = along;
        let length = distance(a, b);
        along += length;
        // Beside a vertical run to the left and right, beside a horizontal
        // one above and below.
        let aside = if a.0 == b.0 {
            (size.0 / 2.0 + 3.0, 0.0)
        } else if a.1 == b.1 {
            (0.0, size.1 / 2.0 + 3.0)
        } else {
            (0.0, 0.0)
        };
        // Every 4 units, at most 24 places a run.
        let steps = (length / 4.0).ceil().clamp(1.0, 24.0) as usize;
        (0..=steps).flat_map(move |n| {
            let t = n as f32 / steps as f32;
            let (x, y) = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            let at = start + length * t;
            [
                ((x, y), at, false),
                ((x - aside.0, y - aside.1), at, true),
                ((x + aside.0, y + aside.1), at, true),
            ]
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn a_segment_cuts_a_rect_only_through_its_inside() {
        let r = rect(0.0, 0.0, 10.0, 10.0);
        assert!(cuts(r, (-5.0, 5.0), (15.0, 5.0)));
        assert!(cuts(r, (-5.0, -5.0), (15.0, 15.0)));
        assert!(cuts(r, (5.0, 5.0), (5.0, 5.0)));
        assert!(!cuts(r, (-5.0, 0.0), (15.0, 0.0)), "along the border");
        assert!(!cuts(r, (5.0, 10.0), (5.0, 20.0)), "away from the border");
        assert!(!cuts(r, (-5.0, -5.0), (-1.0, 20.0)));
    }

    #[test]
    fn straight_runs_lose_their_middle_points() {
        let path = vec![
            (0.0, 0.0),
            (0.0, 5.0),
            (0.0, 10.0),
            (5.0, 10.0),
            (5.0, 10.0),
            (10.0, 10.0),
        ];
        assert_eq!(
            straighten(path),
            vec![(0.0, 0.0), (0.0, 10.0), (10.0, 10.0)]
        );
    }

    #[test]
    fn a_route_goes_around_the_box_between() {
        let from = rect(0.0, 0.0, 40.0, 20.0);
        let wall = rect(-20.0, 50.0, 80.0, 20.0);
        let to = rect(0.0, 100.0, 40.0, 20.0);
        let around = Surroundings {
            obstacles: vec![from, wall, to],
            origin: (-100.0, -100.0),
            ..Surroundings::default()
        };
        let end = |rect| End { rect, flat: true };
        let points = route(end(from), end(to), &around).unwrap();
        assert!(points.len() >= 4, "{points:?}");
        for pair in points.windows(2) {
            assert!(!cuts(wall, pair[0], pair[1]), "{points:?}");
            let straight = pair[0].0 == pair[1].0 || pair[0].1 == pair[1].1;
            assert!(straight, "{points:?}");
        }
    }

    /// Whether `points` passes through `p`, at a corner or on a run.
    fn passes(points: &[Point], p: Point) -> bool {
        points.windows(2).any(|w| {
            let (a, b) = (w[0], w[1]);
            let on_x = (a.0 - p.0).abs() < 0.01 && (a.0 - b.0).abs() < 0.01;
            let on_y = (a.1 - p.1).abs() < 0.01 && (a.1 - b.1).abs() < 0.01;
            (on_x && p.1 >= a.1.min(b.1) - 0.01 && p.1 <= a.1.max(b.1) + 0.01)
                || (on_y && p.0 >= a.0.min(b.0) - 0.01 && p.0 <= a.0.max(b.0) + 0.01)
        })
    }

    #[test]
    fn a_route_turns_at_its_via() {
        let from = rect(0.0, 0.0, 40.0, 40.0);
        let to = rect(300.0, 200.0, 40.0, 40.0);
        let around = Surroundings {
            obstacles: vec![from, to],
            origin: (-200.0, -200.0),
            ..Surroundings::default()
        };
        let end = |rect| End { rect, flat: true };
        // In line with where the line leaves: it could run on past it and
        // turn later, but it turns there.
        for via in [(100.0, 20.0), (320.0, 100.0), (150.0, 120.0)] {
            let points = route_via(end(from), end(to), &[via], &around).unwrap();
            let inner = &points[1..points.len() - 1];
            assert!(
                inner
                    .iter()
                    .any(|p| (p.0 - via.0).abs() < 0.01 && (p.1 - via.1).abs() < 0.01),
                "{via:?}: {points:?}"
            );
        }
    }

    #[test]
    fn a_route_goes_through_its_vias_in_turn() {
        let from = rect(0.0, 0.0, 40.0, 20.0);
        let to = rect(0.0, 100.0, 40.0, 20.0);
        let around = Surroundings {
            obstacles: vec![from, to],
            origin: (-200.0, -200.0),
            ..Surroundings::default()
        };
        let end = |rect| End { rect, flat: true };
        // Straight down without; out to the right and back with a via.
        let straight = route(end(from), end(to), &around).unwrap();
        assert_eq!(straight.len(), 2, "{straight:?}");
        let via = (120.0, 60.0);
        let points = route_via(end(from), end(to), &[via], &around).unwrap();
        assert!(passes(&points, via), "{points:?}");
        // Two vias: past the left one first, then the right one.
        let (left, right) = ((-80.0, 40.0), (100.0, 80.0));
        let points = route_via(end(from), end(to), &[left, right], &around).unwrap();
        let at = |p: Point| {
            points
                .windows(2)
                .position(|w| passes(w, p))
                .unwrap_or_else(|| panic!("{p:?} not on {points:?}"))
        };
        assert!(at(left) <= at(right), "{points:?}");
        for pair in points.windows(2) {
            assert!(
                pair[0].0 == pair[1].0 || pair[0].1 == pair[1].1,
                "{points:?}"
            );
            assert!(!cuts(from, pair[0], pair[1]) && !cuts(to, pair[0], pair[1]));
        }
        // It never runs back over itself.
        for w in points.windows(3) {
            let (a, b, c) = (w[0], w[1], w[2]);
            let back = (b.0 - a.0) * (c.0 - b.0) + (b.1 - a.1) * (c.1 - b.1) < 0.0;
            assert!(!back, "{points:?}");
        }
    }

    #[test]
    fn a_via_in_a_box_leads_nowhere() {
        let from = rect(0.0, 0.0, 40.0, 20.0);
        let wall = rect(80.0, 40.0, 60.0, 40.0);
        let to = rect(0.0, 100.0, 40.0, 20.0);
        let around = Surroundings {
            obstacles: vec![from, wall, to],
            origin: (-200.0, -200.0),
            ..Surroundings::default()
        };
        let end = |rect| End { rect, flat: true };
        assert!(route_via(end(from), end(to), &[(110.0, 60.0)], &around).is_none());
        // Above the drawing's origin, a via is still reached.
        let tight = Surroundings {
            origin: (-10.0, -10.0),
            ..around
        };
        let via = (20.0, -60.0);
        let points = route_via(end(from), end(to), &[via], &tight).unwrap();
        assert!(passes(&points, via), "{points:?}");
    }

    #[test]
    fn a_label_sits_off_the_boxes_beside_its_line() {
        let block = rect(-30.0, 40.0, 25.0, 20.0);
        let points = [(0.0, 0.0), (0.0, 100.0)];
        let room = LabelRoom {
            solids: &[block],
            frames: &[],
            lines: &[],
            origin: (-100.0, -100.0),
            on_line: false,
        };
        let at = label_spot(&points, (20.0, 10.0), &room, None).unwrap();
        let label = Bounds::around(at, (20.0, 10.0));
        assert_eq!(Bounds::of(block).grow(2.0).overlap(label), 0.0, "{at:?}");
    }
}
