//! Lines by hand in the diagram pane ([0016](../../../../docs/adr/0016-lines-by-hand.md)).
//!
//! With Alt held, the line under the pointer lights in the accent color
//! and its bends show as handles. Alt and a drag on the line puts a bend
//! where it was grabbed; on a handle it moves that bend. A bend dragged
//! back onto the straight way between its neighbors goes, and so does one
//! Alt and a double click take. Escape stops a drag. On release the pane
//! keeps the line's new bends for the window
//! ([`super::Paper::take_by_hand`]), which writes them into the source.

use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use omaspec_uml::hand::{Line, Via};
use omaspec_uml::map::{Fit, Map, Wire};

use super::Message;
use super::view::{Glide, View};

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

/// The line under the pointer and what is being done to it.
#[derive(Debug, Clone, PartialEq)]
pub struct Hand {
    /// Which of the map's wires.
    pub wire: usize,
    /// Its bends as they will be written.
    pub vias: Vec<Via>,
    /// The handle under the pointer.
    pub hover: Option<usize>,
    pub drag: Option<Drag>,
    /// Its new bends are on their way to the source; the line is shown as
    /// dragged until the new picture comes.
    pub sent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drag {
    /// Where the press was, in diagram units.
    pub from: (f32, f32),
    /// The bend that moves; a new one goes in at this place once the
    /// pointer has moved far enough.
    pub bend: usize,
    pub new: bool,
    /// The pointer has moved far enough to drag.
    pub moved: bool,
    /// The bend lies on the straight way between its neighbors: it goes
    /// on release.
    pub straight: bool,
}

impl Hand {
    /// What the pointer at `at` (diagram units) does; `reach` is
    /// [`REACH`] in diagram units. Returns the line's new bends when they
    /// are to be written.
    pub fn gesture(
        hand: &mut Option<Hand>,
        gesture: Gesture,
        map: &Map,
        to_diagram: impl Fn(Point) -> (f32, f32),
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
                let p = to_diagram(at);
                let current = hover(map, p, reach, hand.as_ref())?;
                let wire = &map.wires[current.wire];
                let (bend, new) = match current.hover {
                    Some(bend) => (bend, false),
                    None => (place_of(wire, &current.vias, p, map.fit), true),
                };
                *hand = Some(Hand {
                    drag: Some(Drag {
                        from: p,
                        bend,
                        new,
                        moved: false,
                        straight: false,
                    }),
                    ..current
                });
                None
            }
            Gesture::Move(at) => {
                let h = hand.as_mut()?;
                let wire = map.wires.get(h.wire)?;
                let drag = h.drag.as_mut()?;
                let p = to_diagram(at);
                let slop = reach * SLOP / REACH;
                if !drag.moved {
                    if (p.0 - drag.from.0).hypot(p.1 - drag.from.1) < slop {
                        return None;
                    }
                    drag.moved = true;
                    if drag.new {
                        h.vias.insert(drag.bend, wire.via_at(p, map.fit));
                    }
                }
                h.vias[drag.bend] = wire.via_at(p, map.fit);
                let points = way(wire, &h.vias, map.fit);
                // The bend's neighbors on the way: the ends, or other bends.
                let (before, after) = (points[drag.bend], points[drag.bend + 2]);
                drag.straight = distance(p, before, after) < reach;
                None
            }
            Gesture::Release => {
                let h = hand.as_mut()?;
                let drag = h.drag.take()?;
                if !drag.moved {
                    return None;
                }
                if drag.straight {
                    h.vias.remove(drag.bend);
                }
                h.sent = true;
                Some(Line {
                    key: map.wires.get(h.wire)?.key.clone(),
                    vias: h.vias.clone(),
                })
            }
            Gesture::Cancel => {
                let h = hand.as_mut()?;
                h.drag = None;
                h.vias = map.wires.get(h.wire)?.vias.clone();
                None
            }
            Gesture::Double(at) => {
                let current = hover(map, to_diagram(at), reach, hand.as_ref())?;
                let bend = current.hover?;
                let mut vias = current.vias.clone();
                vias.remove(bend);
                let key = map.wires[current.wire].key.clone();
                *hand = Some(Hand {
                    vias: vias.clone(),
                    hover: None,
                    sent: true,
                    ..current
                });
                Some(Line { key, vias })
            }
        }
    }
}

/// The wire whose line runs within `reach` of `p`, the nearest.
pub fn wire_at(map: &Map, p: (f32, f32), reach: f32) -> Option<usize> {
    hover(map, p, reach, None).map(|h| h.wire)
}

/// The line and handle at `p`, if any: a handle of the line lit now
/// first, then the nearest line.
fn hover(map: &Map, p: (f32, f32), reach: f32, now: Option<&Hand>) -> Option<Hand> {
    let near = |q: (f32, f32)| (q.0 - p.0).hypot(q.1 - p.1) < reach;
    let lit = now.map(|h| h.wire);
    let mut order: Vec<usize> = lit.into_iter().collect();
    order.extend((0..map.wires.len()).filter(|&i| Some(i) != lit));
    for &index in &order {
        let wire = &map.wires[index];
        if let Some(bend) = wire.bends(map.fit).into_iter().position(near) {
            return Some(Hand {
                wire: index,
                vias: wire.vias.clone(),
                hover: Some(bend),
                drag: None,
                sent: false,
            });
        }
    }
    let (index, off) = map
        .wires
        .iter()
        .enumerate()
        .map(|(i, wire)| (i, off(&wire.points, p)))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    (off < reach).then(|| Hand {
        wire: index,
        vias: map.wires[index].vias.clone(),
        hover: None,
        drag: None,
        sent: false,
    })
}

/// Where among `vias` a new bend at `p` goes: after the bends that come
/// before it along the line.
fn place_of(wire: &Wire, vias: &[Via], p: (f32, f32), fit: Fit) -> usize {
    let at = along(&wire.points, p);
    let mut bends = wire.clone();
    bends.vias = vias.to_vec();
    bends
        .bends(fit)
        .into_iter()
        .filter(|&b| along(&wire.points, b) < at)
        .count()
}

/// The way the line takes through `vias`, as straight runs from its start
/// to its end: what a drag shows until the new picture comes.
fn way(wire: &Wire, vias: &[Via], fit: Fit) -> Vec<(f32, f32)> {
    let mut bends = wire.clone();
    bends.vias = vias.to_vec();
    let first = wire.points.first().copied().unwrap_or(wire.from);
    let last = wire.points.last().copied().unwrap_or(wire.to);
    std::iter::once(first)
        .chain(bends.bends(fit))
        .chain(std::iter::once(last))
        .collect()
}

/// How far along `points` the point nearest `p` lies.
fn along(points: &[(f32, f32)], p: (f32, f32)) -> f32 {
    let mut best = (f32::INFINITY, 0.0);
    let mut walked = 0.0;
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let length = (b.0 - a.0).hypot(b.1 - a.1);
        let t = project(p, a, b);
        let q = (a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1));
        let d = (q.0 - p.0).hypot(q.1 - p.1);
        if d < best.0 {
            best = (d, walked + t * length);
        }
        walked += length;
    }
    best.1
}

/// The distance from `p` to the nearest run of `points`.
fn off(points: &[(f32, f32)], p: (f32, f32)) -> f32 {
    points
        .windows(2)
        .map(|w| distance(p, w[0], w[1]))
        .fold(f32::INFINITY, f32::min)
}

fn distance(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let t = project(p, a, b);
    (a.0 + t * (b.0 - a.0) - p.0).hypot(a.1 + t * (b.1 - a.1) - p.1)
}

fn project(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    if length < 1e-6 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length).clamp(0.0, 1.0)
    }
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
        let screen = |(x, y): (f32, f32)| {
            view.on_screen(Rectangle::new(Point::new(x, y), Size::ZERO), local)
                .position()
        };
        let line = |points: &[(f32, f32)]| {
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
        let dragged = self.hand.sent || self.hand.drag.is_some_and(|d| d.moved);
        let glow = Color {
            a: 0.35,
            ..self.accent
        };
        if dragged {
            // The drawn line faint, the way through the bends in front.
            frame.stroke(
                &line(&wire.points),
                Stroke::default().with_color(glow).with_width(3.0),
            );
            let way = way(wire, &self.hand.vias, self.map.fit);
            frame.stroke(
                &line(&way),
                Stroke::default().with_color(self.accent).with_width(2.0),
            );
        } else {
            frame.stroke(
                &line(&wire.points),
                Stroke::default().with_color(self.accent).with_width(3.0),
            );
        }
        let mut bends = wire.clone();
        bends.vias = self.hand.vias.clone();
        for (i, bend) in bends.bends(self.map.fit).into_iter().enumerate() {
            let at = screen(bend);
            let grabbed = self.hand.drag.is_some_and(|d| d.moved && d.bend == i);
            let lit = grabbed || self.hand.hover == Some(i);
            let radius = if lit { HANDLE + 1.5 } else { HANDLE };
            let dot = Path::circle(at, radius);
            let going = grabbed && self.hand.drag.is_some_and(|d| d.straight);
            // A bend about to go is only a ring.
            frame.fill(&dot, if going { self.paper } else { self.accent });
            frame.stroke(
                &dot,
                Stroke::default()
                    .with_color(if going { self.accent } else { self.paper })
                    .with_width(1.5),
            );
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use omaspec_uml::hand::Key;

    use super::*;

    /// One line straight down from (0, 0) to (0, 100), with no bends.
    fn map() -> Map {
        Map {
            wires: vec![Wire {
                key: Key {
                    from: "A".into(),
                    to: "B".into(),
                    nth: 1,
                },
                line: Some(1),
                points: vec![(0.0, 10.0), (0.0, 90.0)],
                from: (0.0, 0.0),
                to: (0.0, 100.0),
                vias: Vec::new(),
                entry: None,
            }],
            ..Map::default()
        }
    }

    fn go(hand: &mut Option<Hand>, map: &Map, gesture: Gesture) -> Option<Line> {
        Hand::gesture(hand, gesture, map, |p| (p.x, p.y), REACH)
    }

    #[test]
    fn a_drag_on_the_line_puts_a_bend_where_it_is_let_go() {
        let map = map();
        let mut hand = None;
        assert!(
            go(
                &mut hand,
                &map,
                Gesture::Hover(Some(Point::new(30.0, 50.0)))
            )
            .is_none()
        );
        assert!(hand.is_none(), "nothing under the pointer");
        go(&mut hand, &map, Gesture::Hover(Some(Point::new(2.0, 50.0))));
        assert_eq!(hand.as_ref().map(|h| h.wire), Some(0));
        go(&mut hand, &map, Gesture::Press(Point::new(2.0, 50.0)));
        // A press that does not move is a click: nothing is written.
        assert!(go(&mut hand, &map, Gesture::Release).is_none());
        go(&mut hand, &map, Gesture::Press(Point::new(2.0, 50.0)));
        go(&mut hand, &map, Gesture::Move(Point::new(-40.0, 50.0)));
        let line = go(&mut hand, &map, Gesture::Release).unwrap();
        assert_eq!(line.key.to_string(), "A -> B");
        // Half way down, 40 to the left: right of the way, going down.
        let via = line.vias[0];
        assert!(
            (via.along - 0.5).abs() < 1e-4 && (via.aside - 40.0).abs() < 1e-3,
            "{via:?}"
        );
        assert!(hand.as_ref().is_some_and(|h| h.sent));
        // While it is on its way, Alt let go keeps the line shown.
        go(&mut hand, &map, Gesture::Hover(None));
        assert!(hand.is_some());
    }

    #[test]
    fn a_bend_moves_goes_back_to_straight_and_is_taken_away() {
        let mut map = map();
        map.wires[0].vias = vec![Via {
            along: 0.5,
            aside: 40.0,
        }];
        // The line as drawn through the bend at (-40, 50).
        map.wires[0].points = vec![
            (0.0, 10.0),
            (0.0, 30.0),
            (-40.0, 30.0),
            (-40.0, 70.0),
            (0.0, 70.0),
            (0.0, 90.0),
        ];
        let mut hand = None;
        go(
            &mut hand,
            &map,
            Gesture::Hover(Some(Point::new(-41.0, 51.0))),
        );
        assert_eq!(hand.as_ref().and_then(|h| h.hover), Some(0));
        // Moved further out.
        go(&mut hand, &map, Gesture::Press(Point::new(-41.0, 51.0)));
        go(&mut hand, &map, Gesture::Move(Point::new(-80.0, 50.0)));
        let line = go(&mut hand, &map, Gesture::Release).unwrap();
        assert_eq!(line.vias.len(), 1);
        assert!((line.vias[0].aside - 80.0).abs() < 1e-3);
        // Dragged back onto the straight way, it goes.
        let mut hand = None;
        go(&mut hand, &map, Gesture::Press(Point::new(-40.0, 50.0)));
        go(&mut hand, &map, Gesture::Move(Point::new(1.0, 50.0)));
        assert!(hand.as_ref().unwrap().drag.unwrap().straight);
        assert!(
            go(&mut hand, &map, Gesture::Release)
                .unwrap()
                .vias
                .is_empty()
        );
        // Escape puts it back where it was.
        let mut hand = None;
        go(&mut hand, &map, Gesture::Press(Point::new(-40.0, 50.0)));
        go(&mut hand, &map, Gesture::Move(Point::new(-90.0, 20.0)));
        go(&mut hand, &map, Gesture::Cancel);
        assert_eq!(hand.as_ref().unwrap().vias, map.wires[0].vias);
        assert!(go(&mut hand, &map, Gesture::Release).is_none());
        // A double click on it takes it.
        let mut hand = None;
        let line = go(&mut hand, &map, Gesture::Double(Point::new(-40.0, 50.0))).unwrap();
        assert!(line.vias.is_empty());
    }

    #[test]
    fn a_new_bend_goes_in_its_place_along_the_line() {
        let mut map = map();
        map.wires[0].vias = vec![Via {
            along: 0.8,
            aside: 0.0,
        }];
        let mut hand = None;
        go(&mut hand, &map, Gesture::Press(Point::new(0.0, 30.0)));
        go(&mut hand, &map, Gesture::Move(Point::new(-30.0, 30.0)));
        let line = go(&mut hand, &map, Gesture::Release).unwrap();
        assert_eq!(line.vias.len(), 2);
        assert!((line.vias[0].along - 0.3).abs() < 1e-4, "{:?}", line.vias);
        assert!((line.vias[1].along - 0.8).abs() < 1e-4, "{:?}", line.vias);
    }
}
