//! Lines by hand in the diagram pane ([0016](../../../../docs/adr/0016-lines-by-hand.md)).
//!
//! With Alt held, the line under the pointer lights in the accent color
//! and its corners show as handles. Alt and a drag on a straight run moves
//! that run sideways, the runs beside it stretching to follow; on a corner
//! it moves the corner, and the runs on either side with it. The line is
//! shown as it will be drawn ([`Wire::shaped`]) while the drag goes on.
//! Alt and a double click give the line back to the layout. Escape stops a
//! drag. On release the pane keeps the line's new corners for the window
//! ([`super::Paper::take_by_hand`]), which writes them into the source.

use iced::mouse::Interaction;
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use omaspec_uml::hand::{Line, MOST};
use omaspec_uml::map::{Map, Wire};

use super::Message;
use super::view::{Glide, View};

type At = (f32, f32);

/// How near the pointer a line or a handle counts as hit, in screen
/// pixels.
const REACH: f32 = 6.0;

/// [`REACH`] in diagram units at `zoom`.
pub fn reach(zoom: f32) -> f32 {
    REACH / zoom.max(1e-3)
}
/// How far a press moves before it is a drag, in screen pixels.
const SLOP: f32 = 3.0;
/// A handle's radius, in screen pixels.
const HANDLE: f32 = 4.5;

/// What the pointer does with Alt, in the pane's coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gesture {
    /// Alt is held and the pointer is here; `None` when it is not.
    Hover(Option<Point>),
    Press(Point),
    Move(Point),
    Release,
    /// Escape during a drag.
    Cancel,
    /// Alt and a double click.
    Double(Point),
}

/// What of a line the pointer holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    /// The corner at this index of the line's corners.
    Corner(usize),
    /// The run from point `index` of the line's path to the next; `down`
    /// when it runs down the page, so it moves across.
    Run { index: usize, down: bool },
}

/// The line under the pointer and what is being done to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Hand {
    /// Which of the map's wires.
    pub wire: usize,
    /// Its corners as they will be written, in the picture's units.
    pub corners: Vec<At>,
    /// What the pointer is over.
    pub hover: Option<Grip>,
    pub drag: Option<Drag>,
    /// Its new corners are on their way to the source; the line is shown
    /// as shaped until the new picture comes.
    pub sent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Drag {
    /// Where the press was, in diagram units.
    pub from: At,
    pub grip: Grip,
    /// The corners when the press came, and the line through them.
    pub start: Vec<At>,
    pub path: Vec<At>,
    /// The pointer has moved far enough to drag.
    pub moved: bool,
}

impl Hand {
    /// What the pointer at `at` (diagram units) does; `reach` is
    /// [`REACH`] in diagram units. Returns the line's new corners when they
    /// are to be written.
    pub fn gesture(
        hand: &mut Option<Hand>,
        gesture: Gesture,
        map: &Map,
        to_diagram: impl Fn(Point) -> At,
        reach: f32,
    ) -> Option<Line> {
        match gesture {
            Gesture::Hover(None) => {
                if hand.as_ref().is_some_and(|h| h.drag.is_none() && !h.sent) {
                    *hand = None;
                }
                None
            }
            Gesture::Hover(Some(at)) => {
                if hand.as_ref().is_some_and(|h| h.drag.is_some() || h.sent) {
                    return None;
                }
                *hand = hover(map, to_diagram(at), reach, hand.as_ref());
                None
            }
            Gesture::Press(at) => {
                // The corners just written are not on the map yet: a drag
                // now would start from the ones before them.
                if hand.as_ref().is_some_and(|h| h.sent) {
                    return None;
                }
                let p = to_diagram(at);
                let current = hover(map, p, reach, hand.as_ref())?;
                let wire = &map.wires[current.wire];
                // The line as its corners shape it, which the drag changes.
                let path = wire.shaped(wire.corners(), map.fit);
                let corners = inner(&path).to_vec();
                let grip = grip_at(&path, p, reach)?;
                if matches!(grip, Grip::Run { .. }) && corners.len() + 2 > MOST {
                    *hand = Some(current);
                    return None;
                }
                *hand = Some(Hand {
                    corners: corners.clone(),
                    hover: Some(grip),
                    drag: Some(Drag {
                        from: p,
                        grip,
                        start: corners,
                        path,
                        moved: false,
                    }),
                    ..current
                });
                None
            }
            Gesture::Move(at) => {
                let h = hand.as_mut()?;
                let drag = h.drag.as_mut()?;
                let p = to_diagram(at);
                let slop = reach * SLOP / REACH;
                if !drag.moved && (p.0 - drag.from.0).hypot(p.1 - drag.from.1) < slop {
                    return None;
                }
                drag.moved = true;
                h.corners = moved(drag, p);
                None
            }
            Gesture::Release => {
                let h = hand.as_mut()?;
                let drag = h.drag.take()?;
                if !drag.moved {
                    return None;
                }
                let wire = map.wires.get(h.wire)?;
                let corners = wire.simplest(&h.corners, map.fit);
                if same(&corners, wire.corners()) {
                    h.corners = wire.corners().to_vec();
                    return None;
                }
                h.corners.clone_from(&corners);
                h.sent = true;
                Some(wire.entry_for(&corners))
            }
            Gesture::Cancel => {
                let h = hand.as_mut()?;
                h.drag = None;
                h.corners = map.wires.get(h.wire)?.corners().to_vec();
                None
            }
            Gesture::Double(at) => {
                if hand.as_ref().is_some_and(|h| h.sent) {
                    return None;
                }
                let current = hover(map, to_diagram(at), reach, hand.as_ref())?;
                let wire = &map.wires[current.wire];
                wire.entry?;
                let key = wire.key.clone();
                *hand = Some(Hand {
                    corners: Vec::new(),
                    hover: None,
                    sent: true,
                    ..current
                });
                Some(Line {
                    key,
                    corners: Vec::new(),
                })
            }
        }
    }

    /// The hand once a new picture with `new` came in place of `old`: a
    /// drag goes on, on the same line, and so does a line whose corners
    /// wait for a later picture; `theirs` says this one has them.
    /// Otherwise the pointer finds the line again.
    pub fn redrawn(self, old: &Map, new: &Map, theirs: bool) -> Option<Hand> {
        if self.drag.is_none() && (!self.sent || theirs) {
            return None;
        }
        let key = &old.wires.get(self.wire)?.key;
        let wire = new.wires.iter().position(|w| w.key == *key)?;
        Some(Hand { wire, ..self })
    }

    /// The corners just sent changed nothing in the source: the line is
    /// lit as it is drawn.
    pub fn unchanged(&mut self, map: &Map) {
        self.sent = false;
        if let Some(wire) = map.wires.get(self.wire) {
            self.corners = wire.corners().to_vec();
        }
    }

    /// The pointer over this hand: a resize arrow over a run, a hand over
    /// a corner.
    pub fn cursor(&self) -> Interaction {
        let grip = self.drag.as_ref().map(|d| d.grip).or(self.hover);
        match grip {
            Some(Grip::Run { down: true, .. }) => Interaction::ResizingHorizontally,
            Some(Grip::Run { down: false, .. }) => Interaction::ResizingVertically,
            Some(Grip::Corner(_)) if self.drag.is_some() => Interaction::Grabbing,
            _ => Interaction::Grab,
        }
    }
}

/// The corners after the pointer moved to `p` in `drag`: a corner moves
/// with the runs on either side of it; a run moves sideways, and the
/// corners at its ends with it. A run that meets a box at both ends gets a
/// corner of its own, so the box ends can slide along their sides.
fn moved(drag: &Drag, p: At) -> Vec<At> {
    let mut corners = drag.start.clone();
    match drag.grip {
        Grip::Corner(i) => {
            let Some(&old) = corners.get(i) else {
                return corners;
            };
            corners[i] = p;
            for k in [i.checked_sub(1), Some(i + 1)].into_iter().flatten() {
                let Some(q) = corners.get_mut(k) else {
                    continue;
                };
                if (q.0 - old.0).abs() < 0.01 {
                    q.0 = p.0;
                } else if (q.1 - old.1).abs() < 0.01 {
                    q.1 = p.1;
                }
            }
        }
        Grip::Run { index, down } => {
            let d = if down {
                p.0 - drag.from.0
            } else {
                p.1 - drag.from.1
            };
            let shift = |q: &mut At| {
                if down {
                    q.0 += d;
                } else {
                    q.1 += d;
                }
            };
            // Path point `index` is corner `index - 1`; the next, corner
            // `index`.
            let held: Vec<usize> = [index.checked_sub(1), Some(index)]
                .into_iter()
                .flatten()
                .filter(|&k| k < corners.len())
                .collect();
            if held.is_empty() {
                let (a, b) = (drag.path[index], drag.path[index + 1]);
                let mut middle = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
                shift(&mut middle);
                corners = vec![middle];
            } else {
                for k in held {
                    shift(&mut corners[k]);
                }
            }
        }
    }
    corners
}

/// The points of `path` between its ends: its corners.
fn inner(path: &[At]) -> &[At] {
    match path.len() {
        0..=2 => &[],
        n => &path[1..n - 1],
    }
}

fn same(a: &[At], b: &[At]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| (p.0 - q.0).abs() < 0.01 && (p.1 - q.1).abs() < 0.01)
}

/// What of `path` lies at `p`: a corner within `reach`, else the run
/// nearest to it.
fn grip_at(path: &[At], p: At, reach: f32) -> Option<Grip> {
    let near = |q: At| (q.0 - p.0).hypot(q.1 - p.1) < reach;
    if let Some(i) = inner(path).iter().position(|&q| near(q)) {
        return Some(Grip::Corner(i));
    }
    let (index, _) = path
        .windows(2)
        .enumerate()
        .map(|(i, w)| (i, distance(p, w[0], w[1])))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    let (a, b) = (path[index], path[index + 1]);
    Some(Grip::Run {
        index,
        down: (a.0 - b.0).abs() < (a.1 - b.1).abs(),
    })
}

/// The wire whose line runs within `reach` of `p`, the nearest.
pub fn wire_at(map: &Map, p: At, reach: f32) -> Option<usize> {
    hover(map, p, reach, None).map(|h| h.wire)
}

/// The line and what of it lies at `p`, if any: a corner of the line lit
/// now first, then the nearest line.
fn hover(map: &Map, p: At, reach: f32, now: Option<&Hand>) -> Option<Hand> {
    let near_corner = |wire: &Wire| {
        wire.corners()
            .iter()
            .any(|q| (q.0 - p.0).hypot(q.1 - p.1) < reach)
    };
    let found = now
        .map(|h| h.wire)
        .filter(|&i| map.wires.get(i).is_some_and(near_corner))
        .or_else(|| {
            map.wires
                .iter()
                .enumerate()
                .map(|(i, wire)| (i, off(&wire.points, p).min(off(&wire.path, p))))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .filter(|&(_, off)| off < reach)
                .map(|(i, _)| i)
        })?;
    let wire = &map.wires[found];
    Some(Hand {
        wire: found,
        corners: wire.corners().to_vec(),
        hover: grip_at(&wire.path, p, reach),
        drag: None,
        sent: false,
    })
}

/// The distance from `p` to the nearest run of `points`.
fn off(points: &[At], p: At) -> f32 {
    points
        .windows(2)
        .map(|w| distance(p, w[0], w[1]))
        .fold(f32::INFINITY, f32::min)
}

fn distance(p: At, a: At, b: At) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length < 1e-6 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length).clamp(0.0, 1.0)
    };
    (a.0 + t * dx - p.0).hypot(a.1 + t * dy - p.1)
}

/// The lit line and its handles, in a layer over the picture: iced draws
/// pictures over shapes within one layer.
pub struct HandLayer<'a> {
    pub hand: &'a Hand,
    pub map: &'a Map,
    pub view: View,
    pub glide: Option<Glide>,
    pub accent: Color,
    pub paper: Color,
}

impl canvas::Program<Message> for HandLayer<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        _event: &Event,
        _bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Option<Action<Message>> {
        None
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let Some(wire) = self.map.wires.get(self.hand.wire) else {
            return vec![frame.into_geometry()];
        };
        let view = self
            .glide
            .map_or(self.view, |g| g.at(iced::time::Instant::now()));
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let screen = |(x, y): At| {
            view.on_screen(Rectangle::new(Point::new(x, y), Size::ZERO), local)
                .position()
        };
        let line = |points: &[At]| {
            Path::new(|b| {
                for (i, &p) in points.iter().enumerate() {
                    if i == 0 {
                        b.move_to(screen(p));
                    } else {
                        b.line_to(screen(p));
                    }
                }
            })
        };
        let shaped = self.hand.sent || self.hand.drag.as_ref().is_some_and(|d| d.moved);
        let glow = Color {
            a: 0.35,
            ..self.accent
        };
        // While shaped: the drawn line faint, in front the line as it will
        // be drawn.
        let path = if shaped {
            frame.stroke(
                &line(&wire.points),
                Stroke::default().with_color(glow).with_width(3.0),
            );
            if self.hand.corners.is_empty() {
                return vec![frame.into_geometry()];
            }
            let path = wire.shaped(&self.hand.corners, self.map.fit);
            frame.stroke(
                &line(&path),
                Stroke::default().with_color(self.accent).with_width(2.0),
            );
            path
        } else {
            frame.stroke(
                &line(&wire.points),
                Stroke::default().with_color(self.accent).with_width(3.0),
            );
            wire.path.clone()
        };
        let grip = self.hand.drag.as_ref().map(|d| d.grip).or(self.hand.hover);
        // The run held, or under the pointer, stands out.
        if let Some(Grip::Run { index, .. }) = grip
            && let Some(run) = path.get(index..index + 2)
        {
            frame.stroke(
                &line(run),
                Stroke::default().with_color(self.accent).with_width(5.0),
            );
        }
        for (i, &corner) in inner(&path).iter().enumerate() {
            let lit = grip == Some(Grip::Corner(i));
            let radius = if lit { HANDLE + 1.5 } else { HANDLE };
            let dot = Path::circle(screen(corner), radius);
            frame.fill(&dot, self.accent);
            frame.stroke(
                &dot,
                Stroke::default().with_color(self.paper).with_width(1.5),
            );
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use omaspec_uml::graph::Rect;
    use omaspec_uml::hand::{End, Key};

    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    /// A on the left, B lower right; the line out of A's right side,
    /// down, and into B's left side.
    fn map() -> Map {
        let (a, b) = (rect(0.0, 0.0, 80.0, 40.0), rect(300.0, 200.0, 80.0, 40.0));
        let path = vec![(80.0, 20.0), (150.0, 20.0), (150.0, 220.0), (300.0, 220.0)];
        Map {
            wires: vec![Wire {
                key: Key {
                    from: "A".into(),
                    to: "B".into(),
                    nth: 1,
                },
                line: Some(1),
                points: path.clone(),
                path,
                from: End {
                    rect: a,
                    flat: true,
                },
                to: End {
                    rect: b,
                    flat: true,
                },
                entry: None,
            }],
            ..Map::default()
        }
    }

    fn go(hand: &mut Option<Hand>, map: &Map, gesture: Gesture) -> Option<Line> {
        Hand::gesture(hand, gesture, map, |p| (p.x, p.y), REACH)
    }

    fn drag(map: &Map, from: At, to: At) -> (Option<Hand>, Option<Line>) {
        let mut hand = None;
        go(
            &mut hand,
            map,
            Gesture::Hover(Some(Point::new(from.0, from.1))),
        );
        go(&mut hand, map, Gesture::Press(Point::new(from.0, from.1)));
        go(&mut hand, map, Gesture::Move(Point::new(to.0, to.1)));
        let line = go(&mut hand, map, Gesture::Release);
        (hand, line)
    }

    /// The corners of `line` in the picture.
    fn points(map: &Map, line: &Line) -> Vec<At> {
        let wire = &map.wires[0];
        let span = omaspec_uml::hand::span(wire.from.rect, wire.to.rect);
        line.corners.iter().map(|c| c.at(span)).collect()
    }

    fn close(a: &[At], b: &[At]) -> bool {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(p, q)| (p.0 - q.0).abs() < 0.01 && (p.1 - q.1).abs() < 0.01)
    }

    #[test]
    fn a_run_dragged_sideways_moves_alone() {
        let map = map();
        // The run down the middle, 60 to the right: its two corners with it.
        let (hand, line) = drag(&map, (151.0, 120.0), (211.0, 125.0));
        let line = line.unwrap();
        assert_eq!(line.key.to_string(), "A -> B");
        let corners = points(&map, &line);
        assert!(
            close(&corners, &[(210.0, 20.0), (210.0, 220.0)]),
            "{corners:?}"
        );
        assert!(hand.unwrap().sent);
        // The first run, down 10: it slides along A's side, and its corner
        // with it; the run down stays where it was.
        let (_, line) = drag(&map, (110.0, 20.0), (112.0, 30.0));
        let corners = points(&map, &line.unwrap());
        assert!(
            close(&corners, &[(150.0, 30.0), (150.0, 220.0)]),
            "{corners:?}"
        );
    }

    #[test]
    fn a_corner_dragged_takes_its_runs_along() {
        let map = map();
        let (_, line) = drag(&map, (150.0, 20.0), (120.0, 10.0));
        let corners = points(&map, &line.unwrap());
        assert!(
            close(&corners, &[(120.0, 10.0), (120.0, 220.0)]),
            "{corners:?}"
        );
    }

    #[test]
    fn a_straight_line_gets_a_corner_to_hold_where_it_was_moved() {
        let mut map = map();
        // A and B side by side, the line straight across.
        map.wires[0].to.rect = rect(300.0, 0.0, 80.0, 40.0);
        map.wires[0].path = vec![(80.0, 20.0), (300.0, 20.0)];
        map.wires[0].points = map.wires[0].path.clone();
        let (_, line) = drag(&map, (190.0, 21.0), (190.0, 31.0));
        let corners = points(&map, &line.unwrap());
        assert!(close(&corners, &[(190.0, 30.0)]), "{corners:?}");
        // Drawn, it still runs straight across, ten lower.
        assert_eq!(
            map.wires[0].shaped(&corners, map.fit),
            [(80.0, 30.0), (300.0, 30.0)]
        );
    }

    #[test]
    fn a_drag_that_changes_nothing_writes_nothing_and_lets_go() {
        let map = map();
        let (mut hand, line) = drag(&map, (151.0, 120.0), (151.0, 121.0));
        assert!(line.is_none(), "a press is not a drag");
        // There and back again.
        go(&mut hand, &map, Gesture::Press(Point::new(151.0, 120.0)));
        go(&mut hand, &map, Gesture::Move(Point::new(190.0, 120.0)));
        go(&mut hand, &map, Gesture::Move(Point::new(151.0, 120.0)));
        assert!(go(&mut hand, &map, Gesture::Release).is_none());
        let h = hand.as_ref().unwrap();
        assert!(!h.sent && h.drag.is_none());
        go(&mut hand, &map, Gesture::Hover(None));
        assert!(hand.is_none());
    }

    #[test]
    fn escape_puts_it_back_and_a_double_click_gives_it_back() {
        let mut map = map();
        let mut hand = None;
        go(&mut hand, &map, Gesture::Press(Point::new(151.0, 120.0)));
        go(&mut hand, &map, Gesture::Move(Point::new(200.0, 120.0)));
        go(&mut hand, &map, Gesture::Cancel);
        assert_eq!(hand.as_ref().unwrap().corners, map.wires[0].corners());
        assert!(go(&mut hand, &map, Gesture::Release).is_none());
        // Laid by the layout, there is nothing to give back.
        let mut hand = None;
        assert!(go(&mut hand, &map, Gesture::Double(Point::new(151.0, 120.0))).is_none());
        map.wires[0].entry = Some(9);
        let line = go(&mut hand, &map, Gesture::Double(Point::new(151.0, 120.0))).unwrap();
        assert!(line.corners.is_empty());
    }

    #[test]
    fn corners_on_their_way_hold_the_line_until_their_picture_comes() {
        let map = map();
        let (mut hand, line) = drag(&map, (151.0, 120.0), (211.0, 120.0));
        line.unwrap();
        // A second drag before the new picture would start from the old
        // corners and write over the first: it waits.
        assert!(go(&mut hand, &map, Gesture::Press(Point::new(211.0, 120.0))).is_none());
        assert!(hand.as_ref().unwrap().drag.is_none());
        assert!(go(&mut hand, &map, Gesture::Double(Point::new(211.0, 120.0))).is_none());
        let mut moved = map.clone();
        let mut other = moved.wires[0].clone();
        other.key.from = "C".into();
        moved.wires.insert(0, other);
        let kept = hand.clone().unwrap().redrawn(&map, &moved, false).unwrap();
        assert!(kept.sent && kept.wire == 1);
        assert!(hand.unwrap().redrawn(&map, &moved, true).is_none());
        // Corners that changed nothing in the source: lit as drawn again.
        let (hand, _) = drag(&map, (151.0, 120.0), (211.0, 120.0));
        let mut hand = hand.unwrap();
        hand.unchanged(&map);
        assert!(!hand.sent && hand.corners == map.wires[0].corners());
    }

    #[test]
    fn the_pointer_shows_which_way_a_run_moves() {
        let map = map();
        let mut hand = None;
        go(
            &mut hand,
            &map,
            Gesture::Hover(Some(Point::new(151.0, 120.0))),
        );
        assert_eq!(
            hand.as_ref().unwrap().cursor(),
            Interaction::ResizingHorizontally
        );
        go(
            &mut hand,
            &map,
            Gesture::Hover(Some(Point::new(110.0, 21.0))),
        );
        assert_eq!(
            hand.as_ref().unwrap().cursor(),
            Interaction::ResizingVertically
        );
        go(
            &mut hand,
            &map,
            Gesture::Hover(Some(Point::new(150.0, 21.0))),
        );
        assert_eq!(hand.as_ref().unwrap().hover, Some(Grip::Corner(0)));
    }
}
