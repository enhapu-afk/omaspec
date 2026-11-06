//! Boxes held in place in the diagram pane
//! ([0021](../../../../docs/adr/0021-boxes-held-in-place.md)).
//!
//! With Alt held, a box under the pointer (where no line runs) lights in
//! the accent color. Alt and a drag on it takes its outline along, and a
//! bar shows where it would go: between two boxes of the rank nearest the
//! pointer, in the box's container. On release the entry that holds it
//! there is kept for the window ([`super::Paper::take_held`]), which writes
//! it into the source: `after` the box before the bar, or `before` the
//! first of the rank. A drop where the box stands already, beside a box it
//! is joined to, or in a rank of no other box writes nothing. Alt and a
//! double click give the box back to the layout. Escape stops a drag.

use iced::mouse::Interaction;
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};
use omaspec_uml::graph::Rect;
use omaspec_uml::hand::Hold;
use omaspec_uml::map::{Map, Place};

use super::Message;
use super::hand::Gesture;
use super::view::{Glide, View};

type At = (f32, f32);

/// How far a press moves before it is a drag, in screen pixels.
const SLOP: f32 = 3.0;

/// [`SLOP`] in diagram units at `zoom`.
pub fn slop(zoom: f32) -> f32 {
    SLOP / zoom.max(1e-3)
}
/// How far past the last box of a rank, or before the first, the bar
/// stands, in diagram units.
const OUTSIDE: f32 = 8.0;

/// A box lit with Alt, or held by the pointer.
#[derive(Debug, Clone, PartialEq)]
pub struct Grab {
    /// Which of the map's places.
    pub place: usize,
    pub drag: Option<Drag>,
    /// Its entry is on its way to the source; the box stays lit until the
    /// new picture comes.
    pub sent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Drag {
    /// Where the press was, and where the pointer is, in diagram units.
    pub from: At,
    pub at: At,
    /// The pointer has moved far enough to drag.
    pub moved: bool,
    /// Where the box would go if let go now.
    pub target: Option<Target>,
}

/// Where a dropped box would go: the entry that holds it there, and the
/// bar that shows the place, in diagram units.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub hold: Hold,
    pub bar: (At, At),
}

/// What a gesture asks the window to write: `hold` for the box `name`, or
/// its entry out when `hold` is `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct Drop {
    pub name: String,
    pub hold: Option<Hold>,
}

impl Grab {
    /// What the pointer at `at` does to a box; `slop` is how far a press
    /// moves before it is a drag ([`slop`]). Returns what to write, if
    /// anything.
    pub fn gesture(
        grab: &mut Option<Grab>,
        gesture: Gesture,
        map: &Map,
        to_diagram: impl Fn(Point) -> At,
        slop: f32,
    ) -> Option<Drop> {
        match gesture {
            Gesture::Hover(None) => {
                if grab.as_ref().is_some_and(|g| g.drag.is_none() && !g.sent) {
                    *grab = None;
                }
                None
            }
            Gesture::Hover(Some(at)) => {
                if grab.as_ref().is_some_and(|g| g.drag.is_some() || g.sent) {
                    return None;
                }
                *grab = place_at(map, to_diagram(at)).map(|place| Grab {
                    place,
                    drag: None,
                    sent: false,
                });
                None
            }
            Gesture::Press(at) => {
                if grab.as_ref().is_some_and(|g| g.sent) {
                    return None;
                }
                let p = to_diagram(at);
                let place = place_at(map, p)?;
                *grab = Some(Grab {
                    place,
                    drag: Some(Drag {
                        from: p,
                        at: p,
                        moved: false,
                        target: None,
                    }),
                    sent: false,
                });
                None
            }
            Gesture::Move(at) => {
                let g = grab.as_mut()?;
                let drag = g.drag.as_mut()?;
                let p = to_diagram(at);
                if !drag.moved && (p.0 - drag.from.0).hypot(p.1 - drag.from.1) < slop {
                    return None;
                }
                drag.moved = true;
                drag.at = p;
                drag.target = target(map, g.place, p);
                None
            }
            Gesture::Release => {
                let g = grab.as_mut()?;
                let drag = g.drag.take()?;
                let target = drag.target.filter(|_| drag.moved)?;
                g.sent = true;
                Some(Drop {
                    name: target.hold.name.clone(),
                    hold: Some(target.hold),
                })
            }
            Gesture::Cancel => {
                let g = grab.as_mut()?;
                g.drag = None;
                None
            }
            Gesture::Double(at) => {
                if grab.as_ref().is_some_and(|g| g.sent) {
                    return None;
                }
                let place = place_at(map, to_diagram(at))?;
                let held = map.places.get(place)?;
                held.entry?;
                *grab = Some(Grab {
                    place,
                    drag: None,
                    sent: true,
                });
                Some(Drop {
                    name: held.name.clone(),
                    hold: None,
                })
            }
        }
    }

    /// The grab once a new picture with `new` came in place of `old`: a
    /// drag goes on, on the same box; a box whose entry was sent waits for
    /// the picture that has it (`theirs` says this one does).
    pub fn redrawn(self, old: &Map, new: &Map, theirs: bool) -> Option<Grab> {
        if self.drag.is_none() && (!self.sent || theirs) {
            return None;
        }
        let name = &old.places.get(self.place)?.name;
        let place = new.places.iter().position(|p| p.name == *name)?;
        Some(Grab { place, ..self })
    }

    /// The pointer over a box: a hand that grabs.
    pub fn cursor(&self) -> Interaction {
        if self.drag.as_ref().is_some_and(|d| d.moved) {
            Interaction::Grabbing
        } else {
            Interaction::Grab
        }
    }
}

/// The innermost box under `p`, if any.
pub fn place_at(map: &Map, p: At) -> Option<usize> {
    let inside = |r: Rect| p.0 >= r.x && p.0 <= r.x + r.w && p.1 >= r.y && p.1 <= r.y + r.h;
    map.places
        .iter()
        .enumerate()
        .filter(|(_, place)| inside(place.rect))
        .min_by(|a, b| (a.1.rect.w * a.1.rect.h).total_cmp(&(b.1.rect.w * b.1.rect.h)))
        .map(|(index, _)| index)
}

/// Where the box `index` of `map` would go if dropped at `p`: in the rank
/// of its container nearest `p` along the flow, between the two boxes
/// whose middles lie on either side of `p` across it. `None` where it
/// stands already, where a line joins it to a box of that rank, or where
/// the rank holds no other box.
pub fn target(map: &Map, index: usize, p: At) -> Option<Target> {
    let place = map.places.get(index)?;
    let across = place.across;
    // Along the flow and across it.
    let along = |r: Rect| {
        if across {
            (r.x, r.x + r.w)
        } else {
            (r.y, r.y + r.h)
        }
    };
    let side = |r: Rect| {
        if across {
            (r.y, r.y + r.h)
        } else {
            (r.x, r.x + r.w)
        }
    };
    let (pa, pc) = if across { (p.0, p.1) } else { (p.1, p.0) };
    let mates: Vec<&Place> = map
        .places
        .iter()
        .filter(|q| q.within == place.within)
        .collect();
    let ranks = ranks(&mates, along);
    let distance = |(lo, hi): (f32, f32)| {
        if pa < lo {
            lo - pa
        } else if pa > hi {
            pa - hi
        } else {
            0.0
        }
    };
    let (band, members) = ranks
        .iter()
        .min_by(|a, b| distance(a.0).total_cmp(&distance(b.0)))?;
    let mut others: Vec<&Place> = members
        .iter()
        .copied()
        .filter(|q| q.name != place.name)
        .collect();
    if others.is_empty() {
        return None;
    }
    let joined = |q: &Place| {
        map.wires.iter().any(|w| {
            (w.key.from == place.name && w.key.to == q.name)
                || (w.key.to == place.name && w.key.from == q.name)
        })
    };
    if others.iter().any(|q| joined(q)) {
        return None;
    }
    let middle = |r: Rect| {
        let (lo, hi) = side(r);
        (lo + hi) / 2.0
    };
    others.sort_by(|a, b| middle(a.rect).total_cmp(&middle(b.rect)));
    let at = others.iter().filter(|q| middle(q.rect) < pc).count();
    // Where it stands now: the same rank, between the same two.
    let own = members.iter().any(|q| q.name == place.name);
    if own
        && others
            .iter()
            .filter(|q| middle(q.rect) < middle(place.rect))
            .count()
            == at
    {
        return None;
    }
    let hold = match at.checked_sub(1) {
        Some(before) => Hold {
            name: place.name.clone(),
            beside: others[before].name.clone(),
            after: true,
        },
        None => Hold {
            name: place.name.clone(),
            beside: others[0].name.clone(),
            after: false,
        },
    };
    let cross = match (at.checked_sub(1).map(|i| others[i]), others.get(at)) {
        (Some(a), Some(b)) => (side(a.rect).1 + side(b.rect).0) / 2.0,
        (Some(a), None) => side(a.rect).1 + OUTSIDE,
        (None, Some(b)) => side(b.rect).0 - OUTSIDE,
        (None, None) => return None,
    };
    let (lo, hi) = *band;
    let bar = if across {
        ((lo, cross), (hi, cross))
    } else {
        ((cross, lo), (cross, hi))
    };
    Some(Target { hold, bar })
}

/// The ranks of `places`: boxes whose spans along the flow overlap, each
/// with that span, in their order along the flow.
fn ranks<'a>(
    places: &[&'a Place],
    along: impl Fn(Rect) -> (f32, f32),
) -> Vec<((f32, f32), Vec<&'a Place>)> {
    let mut sorted: Vec<&Place> = places.to_vec();
    sorted.sort_by(|a, b| along(a.rect).0.total_cmp(&along(b.rect).0));
    let mut out: Vec<((f32, f32), Vec<&Place>)> = Vec::new();
    for place in sorted {
        let (lo, hi) = along(place.rect);
        match out.last_mut() {
            Some((span, members)) if lo < span.1 => {
                span.1 = span.1.max(hi);
                members.push(place);
            }
            _ => out.push(((lo, hi), vec![place])),
        }
    }
    out
}

/// The lit box, its outline under the pointer and the bar, in a layer over
/// the picture.
pub struct HeldLayer<'a> {
    pub grab: &'a Grab,
    pub map: &'a Map,
    pub view: View,
    pub glide: Option<Glide>,
    pub accent: Color,
}

impl canvas::Program<Message> for HeldLayer<'_> {
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
        let Some(place) = self.map.places.get(self.grab.place) else {
            return vec![frame.into_geometry()];
        };
        let view = self
            .glide
            .map_or(self.view, |g| g.at(iced::time::Instant::now()));
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let screen = |r: Rect| {
            view.on_screen(
                Rectangle::new(Point::new(r.x, r.y), Size::new(r.w, r.h)),
                local,
            )
        };
        let outline = |r: Rect| {
            let s = screen(r);
            Path::rectangle(s.position(), s.size())
        };
        let glow = Color {
            a: 0.35,
            ..self.accent
        };
        let moving = self.grab.drag.as_ref().filter(|d| d.moved);
        frame.stroke(
            &outline(place.rect),
            Stroke::default()
                .with_color(if moving.is_some() { glow } else { self.accent })
                .with_width(3.0),
        );
        if let Some(drag) = moving {
            let moved = Rect {
                x: place.rect.x + drag.at.0 - drag.from.0,
                y: place.rect.y + drag.at.1 - drag.from.1,
                ..place.rect
            };
            frame.stroke(
                &outline(moved),
                Stroke::default().with_color(self.accent).with_width(2.0),
            );
            if let Some(target) = &drag.target {
                let (a, b) = target.bar;
                let at = |(x, y): At| {
                    screen(Rect {
                        x,
                        y,
                        w: 0.0,
                        h: 0.0,
                    })
                    .position()
                };
                frame.stroke(
                    &Path::line(at(a), at(b)),
                    Stroke::default().with_color(self.accent).with_width(4.0),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use omaspec_uml::graph::Rect;
    use omaspec_uml::hand::Key;
    use omaspec_uml::map::{Map, Place};

    use super::target;

    fn place(name: &str, x: f32, y: f32) -> Place {
        Place {
            name: name.into(),
            rect: Rect {
                x,
                y,
                w: 60.0,
                h: 30.0,
            },
            within: Vec::new(),
            across: false,
            entry: None,
        }
    }

    /// A on top; B, C and D in the rank below, from left to right.
    fn map() -> Map {
        Map {
            places: vec![
                place("A", 100.0, 0.0),
                place("B", 0.0, 80.0),
                place("C", 100.0, 80.0),
                place("D", 200.0, 80.0),
            ],
            ..Map::default()
        }
    }

    #[test]
    fn a_drop_names_the_box_before_the_bar() {
        let map = map();
        // D dropped between B and C: after B.
        let t = target(&map, 3, (90.0, 95.0)).unwrap();
        assert_eq!((t.hold.beside.as_str(), t.hold.after), ("B", true));
        assert_eq!(t.bar, ((80.0, 80.0), (80.0, 110.0)));
        // D dropped left of B: before B.
        let t = target(&map, 3, (-20.0, 90.0)).unwrap();
        assert_eq!((t.hold.beside.as_str(), t.hold.after), ("B", false));
        // B dropped beside A, a rank up: after A.
        let t = target(&map, 1, (190.0, 10.0)).unwrap();
        assert_eq!((t.hold.beside.as_str(), t.hold.after), ("A", true));
    }

    #[test]
    fn a_drop_where_it_stands_or_beside_its_line_writes_nothing() {
        let mut map = map();
        // C dropped between B and D, where it is.
        assert_eq!(target(&map, 2, (130.0, 95.0)), None);
        // A alone in its rank: dropped there, nothing beside it.
        assert_eq!(target(&map, 0, (300.0, 10.0)), None);
        // B joined to A by a line: not beside A.
        map.wires.push(omaspec_uml::map::Wire {
            key: Key {
                from: "A".into(),
                to: "B".into(),
                nth: 1,
            },
            line: None,
            points: Vec::new(),
            path: Vec::new(),
            from: omaspec_uml::hand::End {
                rect: map.places[0].rect,
                flat: true,
            },
            to: omaspec_uml::hand::End {
                rect: map.places[1].rect,
                flat: true,
            },
            entry: None,
            labels: Vec::new(),
        });
        assert_eq!(target(&map, 1, (190.0, 10.0)), None);
    }
}
