//! The lines glide with the boxes ([0024](../../../../docs/adr/0024-focus-and-collapse.md),
//! addendum). While the boxes of a new layout travel to their places
//! ([`super::morph`]), each line that was there before and is there after
//! travels too, drawn as a stroke between its old route and its new one,
//! with the marks at its ends; a line that goes fades with the old picture,
//! one that comes fades in with the new. The pictures under them have no
//! lines ([`omaspec_uml::without_lines`]), so nothing is drawn twice.
//!
//! A line is known by its key, the names of its two boxes. Folding a
//! container changes those: a line into a box inside it now ends on the
//! container. So a box that left stands as the smallest box that stayed
//! and held it, the other way round when a container unfolds, and several
//! lines that become one all travel to it.

use iced::widget::canvas::{Frame, LineDash, Path, Stroke};
use iced::{Color, Point, Rectangle, Size};
use omaspec_uml::map::{Head, HeadFill, Map, Wire};

use super::morph::{Move, Role};

/// How many points a line is drawn with while it glides: enough for its
/// curves to keep their shape.
const SAMPLES: usize = 48;

/// The lines of one glide.
#[derive(Clone, Debug)]
pub struct Glide {
    pub lines: Vec<Line>,
}

/// One line on its way.
#[derive(Clone, Debug)]
pub struct Line {
    /// Its route in the old picture, in that picture's units; none for a
    /// line that comes.
    pub from: Option<Shape>,
    /// Its route in the new picture; none for a line that goes.
    pub to: Option<Shape>,
    /// Its color in the old picture and in the new one.
    pub colors: (Color, Color),
    /// Its width and dash, in the picture's units.
    pub width: f32,
    pub dash: Option<(f32, f32)>,
    /// A link of a tree that goes with its box into the box above, or
    /// comes out of it: it has both routes, one of them a point on that
    /// box, and fades as a line that goes or comes does.
    pub fold: Option<Role>,
}

/// A line's route and the marks at its ends, in its picture's units.
#[derive(Clone, Debug)]
pub struct Shape {
    pub points: Vec<(f32, f32)>,
    pub heads: Vec<Head>,
}

impl Shape {
    fn of(wire: &Wire) -> Shape {
        Shape {
            points: resample(&wire.points, SAMPLES),
            heads: wire.heads.clone(),
        }
    }
}

fn ink(wire: &Wire) -> Color {
    let omaspec_uml::Color(r, g, b) = wire.stroke.color;
    Color::from_rgb8(r, g, b)
}

impl Line {
    fn dressed(wire: &Wire) -> Line {
        Line {
            from: None,
            to: None,
            colors: (ink(wire), ink(wire)),
            width: wire.stroke.width,
            dash: wire.stroke.dash,
            fold: None,
        }
    }

    /// A link of a tree whose lower box goes into the box above it
    /// ([`super::morph::gather`]), or comes out of it: the link shrinks
    /// to where it leaves that box, or grows from there.
    fn folding(wire: &Wire, moves: &[Move]) -> Option<Line> {
        let step = moves.iter().find(|step| step.name == wire.key.to)?;
        let (was, is) = step.into.filter(|_| wire.fixed)?;
        let first = *wire.points.first()?;
        // Where the link leaves the box above, in the other picture.
        let onto = |from: Rectangle, to: Rectangle| {
            let share = |at: f32, lo: f32, size: f32| ((at - lo) / size.max(1e-3)).clamp(0.0, 1.0);
            (
                to.x + share(first.0, from.x, from.width) * to.width,
                to.y + share(first.1, from.y, from.height) * to.height,
            )
        };
        let shape = Shape::of(wire);
        let point = |at: (f32, f32)| Shape {
            points: vec![at; shape.points.len()],
            heads: Vec::new(),
        };
        let (from, to) = match step.role {
            Role::Leave => (shape.clone(), point(onto(was, is))),
            Role::Arrive => (point(onto(is, was)), shape.clone()),
            Role::Stay => return None,
        };
        Some(Line {
            from: Some(from),
            to: Some(to),
            fold: Some(step.role),
            ..Line::dressed(wire)
        })
    }

    fn between(old: &Wire, new: &Wire) -> Line {
        Line {
            from: Some(Shape::of(old)),
            to: Some(Shape::of(new)),
            colors: (ink(old), ink(new)),
            ..Line::dressed(new)
        }
    }

    fn leaving(wire: &Wire) -> Line {
        Line {
            from: Some(Shape::of(wire)),
            ..Line::dressed(wire)
        }
    }

    fn arriving(wire: &Wire) -> Line {
        Line {
            to: Some(Shape::of(wire)),
            ..Line::dressed(wire)
        }
    }
}

/// The lines of the glide from `old` to `new`, whose boxes are `moves`.
pub fn plan(old: &Map, new: &Map, moves: &[Move]) -> Glide {
    let old_name = |name: &str| stands_for(name, moves, true);
    let new_name = |name: &str| stands_for(name, moves, false);
    let alike = |a: &Wire, b: &Wire| {
        old_name(&a.key.from) == new_name(&b.key.from) && old_name(&a.key.to) == new_name(&b.key.to)
    };
    let mut used = vec![false; new.wires.len()];
    let mut lines = Vec::with_capacity(old.wires.len().max(new.wires.len()));
    for wire in &old.wires {
        let partner = pick(&new.wires, &used, |w| w.key == wire.key)
            .or_else(|| pick(&new.wires, &used, |w| alike(wire, w)));
        lines.push(match partner {
            Some(at) => {
                used[at] = true;
                Line::between(wire, &new.wires[at])
            }
            None => Line::folding(wire, moves).unwrap_or_else(|| Line::leaving(wire)),
        });
    }
    for (at, wire) in new.wires.iter().enumerate() {
        if used[at] {
            continue;
        }
        // One line became several: each comes from the one it was.
        let was = old.wires.iter().find(|w| alike(w, wire));
        lines.push(match was {
            Some(w) => Line::between(w, wire),
            None => Line::folding(wire, moves).unwrap_or_else(|| Line::arriving(wire)),
        });
    }
    Glide { lines }
}

/// The first of `wires` that `fits`: one not taken yet, else any, so
/// several lines may travel to the one they become.
fn pick(wires: &[Wire], used: &[bool], fits: impl Fn(&Wire) -> bool) -> Option<usize> {
    let fitting = |(at, wire): (usize, &Wire)| fits(wire).then_some(at);
    wires
        .iter()
        .enumerate()
        .filter(|&(at, _)| !used[at])
        .find_map(fitting)
        .or_else(|| wires.iter().enumerate().find_map(fitting))
}

/// The box that stands for `name` in both pictures: itself when it is in
/// both, else the smallest box in both that holds it, in the old picture
/// when `old`, in the new one otherwise.
fn stands_for(name: &str, moves: &[Move], old: bool) -> String {
    let Some(step) = moves.iter().find(|step| step.name == name) else {
        return name.to_owned();
    };
    if step.role == Role::Stay {
        return name.to_owned();
    }
    let side = |step: &Move| if old { step.from } else { step.to };
    let inner = side(step);
    moves
        .iter()
        .filter(|step| step.role == Role::Stay && holds(side(step), inner))
        .min_by(|a, b| {
            let (a, b) = (side(a), side(b));
            (a.width * a.height).total_cmp(&(b.width * b.height))
        })
        .map_or_else(|| name.to_owned(), |step| step.name.clone())
}

fn holds(outer: Rectangle, inner: Rectangle) -> bool {
    inner.x >= outer.x - 0.5
        && inner.y >= outer.y - 0.5
        && inner.x + inner.width <= outer.x + outer.width + 0.5
        && inner.y + inner.height <= outer.y + outer.height + 0.5
        && inner.width * inner.height < outer.width * outer.height
}

/// `points` as `n` points spaced evenly along them; the ends stay.
fn resample(points: &[(f32, f32)], n: usize) -> Vec<(f32, f32)> {
    if points.len() < 2 || n < 2 {
        return points.to_vec();
    }
    let mut along = Vec::with_capacity(points.len());
    let mut total = 0.0;
    along.push(0.0);
    for pair in points.windows(2) {
        total += ((pair[1].0 - pair[0].0).powi(2) + (pair[1].1 - pair[0].1).powi(2)).sqrt();
        along.push(total);
    }
    if total <= 0.0 {
        return vec![points[0]; n];
    }
    let mut out = Vec::with_capacity(n);
    let mut segment = 0;
    for i in 0..n {
        let at = total * i as f32 / (n - 1) as f32;
        while segment + 2 < along.len() && along[segment + 1] < at {
            segment += 1;
        }
        let (a, b) = (along[segment], along[segment + 1]);
        let share = if b > a {
            ((at - a) / (b - a)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (p, q) = (points[segment], points[segment + 1]);
        out.push((lerp(p.0, q.0, share), lerp(p.1, q.1, share)));
    }
    out
}

/// Where the pictures lie for one frame of the glide.
pub struct Sheets {
    /// How far the glide is, and how far the old picture has gone and the
    /// new one has come.
    pub t: f32,
    pub out: f32,
    pub inn: f32,
    /// The old picture's sheet and size, and the new one's.
    pub old: (Rectangle, Size),
    pub new: (Rectangle, Size),
    /// The easing sheet, for lines that go or come.
    pub paper: Rectangle,
    /// What an empty mark is filled with.
    pub paper_color: Color,
}

/// One mark to draw: its points on screen, whether it closes, its fill
/// and how strong it is.
struct Mark {
    points: Vec<Point>,
    closed: bool,
    fill: HeadFill,
    alpha: f32,
}

/// Draws every line of `glide` where it is at `sheets.t`.
pub fn paint(frame: &mut Frame, glide: &Glide, sheets: &Sheets) {
    for line in &glide.lines {
        paint_line(frame, line, sheets);
    }
}

fn paint_line(frame: &mut Frame, line: &Line, s: &Sheets) {
    let scale = |(paper, size): (Rectangle, Size)| paper.width / size.width.max(1e-3);
    let (points, marks, alpha, k) = match (&line.from, &line.to) {
        (Some(from), Some(to)) => {
            let a = onto_all(&from.points, s.old);
            let b = onto_all(&to.points, s.new);
            // A link that folds away is there while the old picture is,
            // and has shrunk when that is gone; one that unfolds grows
            // as the new picture comes.
            let (way, alpha) = match line.fold {
                Some(Role::Leave) => (1.0 - s.out, s.out),
                Some(Role::Arrive) => (s.inn, s.inn),
                _ => (s.t, 1.0),
            };
            let points = a
                .iter()
                .zip(&b)
                .map(|(&p, &q)| lerp_point(p, q, way))
                .collect();
            let marks = marks_between(from, to, s);
            let k = lerp(scale(s.old), scale(s.new), s.t);
            (points, marks, alpha, k)
        }
        (Some(from), None) => {
            let sheet = (s.paper, s.old.1);
            let marks = marks_on(&from.heads, sheet, s.out);
            (onto_all(&from.points, sheet), marks, s.out, scale(sheet))
        }
        (None, Some(to)) => {
            let sheet = (s.paper, s.new.1);
            let marks = marks_on(&to.heads, sheet, s.inn);
            (onto_all(&to.points, sheet), marks, s.inn, scale(sheet))
        }
        (None, None) => return,
    };
    if alpha <= 0.02 || points.len() < 2 {
        return;
    }
    let (from, to) = line.colors;
    let ink = Color::from_rgb(
        lerp(from.r, to.r, s.t),
        lerp(from.g, to.g, s.t),
        lerp(from.b, to.b, s.t),
    );
    let color = Color { a: alpha, ..ink };
    let path = polyline(&points, false);
    let segments;
    let mut stroke = Stroke::default()
        .with_color(color)
        .with_width((line.width * k).max(0.75));
    if let Some((on, off)) = line.dash {
        segments = [on * k, off * k];
        stroke = Stroke {
            line_dash: LineDash {
                segments: &segments,
                offset: 0,
            },
            ..stroke
        };
    }
    frame.stroke(&path, stroke);
    let rim = Stroke::default().with_width((line.width.min(1.5) * k).max(0.75));
    for mark in marks {
        if mark.alpha <= 0.02 || mark.points.len() < 2 {
            continue;
        }
        let ink = Color {
            a: mark.alpha,
            ..ink
        };
        let path = polyline(&mark.points, mark.closed);
        match mark.fill {
            HeadFill::Ink if mark.closed => frame.fill(&path, ink),
            HeadFill::Paper if mark.closed => frame.fill(
                &path,
                Color {
                    a: mark.alpha,
                    ..s.paper_color
                },
            ),
            _ => {}
        }
        frame.stroke(&path, rim.with_color(ink));
    }
}

/// The marks of a line that stays: each eased from its old place to its
/// new one when old and new are alike, else the old ones going and the
/// new ones coming.
fn marks_between(from: &Shape, to: &Shape, s: &Sheets) -> Vec<Mark> {
    let alike = from.heads.len() == to.heads.len()
        && from.heads.iter().zip(&to.heads).all(|(a, b)| {
            a.points.len() == b.points.len() && a.closed == b.closed && a.fill == b.fill
        });
    if alike {
        return from
            .heads
            .iter()
            .zip(&to.heads)
            .map(|(a, b)| {
                let (old, new) = (onto_all(&a.points, s.old), onto_all(&b.points, s.new));
                Mark {
                    points: old
                        .iter()
                        .zip(&new)
                        .map(|(&p, &q)| lerp_point(p, q, s.t))
                        .collect(),
                    closed: a.closed,
                    fill: a.fill,
                    alpha: 1.0,
                }
            })
            .collect();
    }
    let mut marks = marks_on(&from.heads, s.old, 1.0 - s.t);
    marks.extend(marks_on(&to.heads, s.new, s.t));
    marks
}

fn marks_on(heads: &[Head], sheet: (Rectangle, Size), alpha: f32) -> Vec<Mark> {
    heads
        .iter()
        .map(|head| Mark {
            points: onto_all(&head.points, sheet),
            closed: head.closed,
            fill: head.fill,
            alpha,
        })
        .collect()
}

fn polyline(points: &[Point], closed: bool) -> Path {
    Path::new(|b| {
        for (i, &p) in points.iter().enumerate() {
            if i == 0 {
                b.move_to(p);
            } else {
                b.line_to(p);
            }
        }
        if closed {
            b.close();
        }
    })
}

/// `points` of a picture of `size`, placed on `paper`.
fn onto_all(points: &[(f32, f32)], (paper, size): (Rectangle, Size)) -> Vec<Point> {
    let (kx, ky) = (
        paper.width / size.width.max(1e-3),
        paper.height / size.height.max(1e-3),
    );
    points
        .iter()
        .map(|&(x, y)| Point::new(paper.x + x * kx, paper.y + y * ky))
        .collect()
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

fn lerp_point(from: Point, to: Point, t: f32) -> Point {
    Point::new(lerp(from.x, to.x, t), lerp(from.y, to.y, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use omaspec_uml::graph::Rect;
    use omaspec_uml::hand::{End, Key};

    fn wire(from: &str, to: &str, points: &[(f32, f32)]) -> Wire {
        let end = |x: f32, y: f32| End {
            rect: Rect {
                x,
                y,
                w: 40.0,
                h: 20.0,
            },
            flat: true,
        };
        Wire {
            key: Key {
                from: from.to_owned(),
                to: to.to_owned(),
                nth: 0,
            },
            line: None,
            points: points.to_vec(),
            path: points.to_vec(),
            from: end(points[0].0, points[0].1),
            to: end(points[points.len() - 1].0, points[points.len() - 1].1),
            entry: None,
            labels: Vec::new(),
            stroke: omaspec_uml::Stroke::new(omaspec_uml::Color::BLACK, 1.0),
            heads: Vec::new(),
            fixed: false,
        }
    }

    fn map(wires: Vec<Wire>) -> Map {
        Map {
            wires,
            ..Map::default()
        }
    }

    fn step(name: &str, from: Rectangle, to: Rectangle, role: Role) -> Move {
        Move {
            name: name.to_owned(),
            from,
            to,
            from_fill: [0.9; 3],
            to_fill: [0.9; 3],
            from_header: 1.0,
            role,
            into: None,
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rectangle {
        Rectangle::new(Point::new(x, y), Size::new(w, h))
    }

    #[test]
    fn a_line_in_both_pictures_travels_and_the_others_go_or_come() {
        let old = map(vec![
            wire("A", "B", &[(0.0, 0.0), (0.0, 100.0)]),
            wire("A", "C", &[(0.0, 0.0), (100.0, 100.0)]),
        ]);
        let new = map(vec![
            wire("A", "B", &[(0.0, 0.0), (50.0, 100.0)]),
            wire("B", "C", &[(50.0, 100.0), (100.0, 100.0)]),
        ]);
        let glide = plan(&old, &new, &[]);
        assert_eq!(glide.lines.len(), 3);
        assert!(glide.lines[0].from.is_some() && glide.lines[0].to.is_some());
        assert!(glide.lines[1].from.is_some() && glide.lines[1].to.is_none());
        assert!(glide.lines[2].from.is_none() && glide.lines[2].to.is_some());
    }

    #[test]
    fn lines_into_a_folded_container_travel_onto_its_box() {
        // Brewing held Heating and Pouring; folded, both lines from Idle
        // end on Brewing, drawn once.
        let brewing_old = rect(0.0, 100.0, 300.0, 200.0);
        let brewing_new = rect(0.0, 100.0, 120.0, 40.0);
        let moves = [
            step(
                "Idle",
                rect(0.0, 0.0, 60.0, 30.0),
                rect(0.0, 0.0, 60.0, 30.0),
                Role::Stay,
            ),
            step("Brewing", brewing_old, brewing_new, Role::Stay),
            step(
                "Heating",
                rect(20.0, 150.0, 60.0, 30.0),
                rect(20.0, 150.0, 60.0, 30.0),
                Role::Leave,
            ),
            step(
                "Pouring",
                rect(200.0, 150.0, 60.0, 30.0),
                rect(200.0, 150.0, 60.0, 30.0),
                Role::Leave,
            ),
        ];
        let old = map(vec![
            wire("Idle", "Heating", &[(30.0, 30.0), (50.0, 150.0)]),
            wire("Idle", "Pouring", &[(30.0, 30.0), (230.0, 150.0)]),
        ]);
        let new = map(vec![wire(
            "Idle",
            "Brewing",
            &[(30.0, 30.0), (60.0, 100.0)],
        )]);
        let glide = plan(&old, &new, &moves);
        assert_eq!(glide.lines.len(), 2, "both travel to the one line");
        for line in &glide.lines {
            let to = line.to.as_ref().expect("stays");
            assert_eq!(*to.points.last().unwrap(), (60.0, 100.0));
        }
        // Unfolded again: the one line becomes two, each from it.
        let back = plan(
            &new,
            &old,
            &moves.map(|s| Move {
                from: s.to,
                to: s.from,
                role: match s.role {
                    Role::Leave => Role::Arrive,
                    other => other,
                },
                ..s
            }),
        );
        assert_eq!(back.lines.len(), 2);
        assert!(
            back.lines
                .iter()
                .all(|l| l.from.is_some() && l.to.is_some())
        );
    }

    #[test]
    fn a_line_is_resampled_evenly_with_its_ends_kept() {
        let points = resample(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], 5);
        assert_eq!(points.len(), 5);
        assert_eq!(points[0], (0.0, 0.0));
        assert_eq!(points[4], (10.0, 10.0));
        assert!((points[2].0 - 10.0).abs() < 1e-4 && points[2].1.abs() < 1e-4);
        assert!((points[1].0 - 5.0).abs() < 1e-4);
    }

    #[test]
    fn a_link_of_a_tree_shrinks_into_the_box_above_when_its_box_folds_away() {
        let rect = |x: f32, y: f32| Rectangle::new(Point::new(x, y), Size::new(60.0, 30.0));
        let mut link = wire("Docs", "API", &[(30.0, 30.0), (30.0, 60.0), (50.0, 60.0)]);
        link.fixed = true;
        let old = Map {
            wires: vec![link],
            ..Map::default()
        };
        // Docs stays and moves 100 to the right; API goes into it.
        let moves = [
            step("Docs", rect(0.0, 0.0), rect(100.0, 0.0), Role::Stay),
            Move {
                into: Some((rect(0.0, 0.0), rect(100.0, 0.0))),
                ..step("API", rect(50.0, 45.0), rect(50.0, 45.0), Role::Leave)
            },
        ];
        let glide = plan(&old, &Map::default(), &moves);
        let line = &glide.lines[0];
        assert_eq!(line.fold, Some(Role::Leave));
        // Every point ends where the link left Docs, in the new picture.
        let to = line.to.as_ref().unwrap();
        assert!(
            to.points.iter().all(|&p| p == (130.0, 30.0)),
            "{:?}",
            to.points
        );
        // A line of a graph diagram that goes fades where it is.
        let mut old = old;
        old.wires[0].fixed = false;
        assert_eq!(plan(&old, &Map::default(), &moves).lines[0].fold, None);
    }
}
