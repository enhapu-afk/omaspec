//! A state machine's life cycle on a circle (the pane's orbit view,
//! `omaspec orbit FILE -o`): its states round a ring in the order a run
//! reaches them from the start, clockwise from the top, where the ring
//! opens: the start on one side of the opening, the end on the other. A
//! way on to the next state runs along the ring, a way back is an arc
//! inside it, a way that skips ahead an arc outside it. For orders,
//! tickets, sprints: anything that comes round again.
//!
//! The states of the top level stand on the ring; a state around others
//! is one box that says how many it holds, and a way into or out of a
//! state inside it is the way of that box. The map tells where each state
//! and transition is, so a click lands on its line and a play runs round
//! the ring.

use std::f32::consts::{PI, TAU};

use super::skin::Skin;
use super::{Diagram, Kind};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::Color;
use crate::map::{self, Map, Shape, Spot};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Font};

const MARGIN: f32 = 10.0;
/// Room around a state's name in its box, and between boxes on the ring.
const PAD_X: f32 = 14.0;
const PAD_Y: f32 = 8.0;
const APART: f32 = 34.0;
/// The ring's radius at least.
const LEAST: f32 = 120.0;
/// How large the start and the end are.
const MARK: f32 = 8.0;
/// How far an arc that skips ahead runs outside the ring, a lane more for
/// each it passes.
const OUTSIDE: f32 = 26.0;
/// Points a way is drawn with, along its curve.
const SAMPLES: usize = 64;

/// A state on the ring.
struct Station {
    /// Its node at the top level.
    node: usize,
    kind: Kind,
    name: Block,
    /// What it holds, for a state around others.
    holds: Option<Block>,
    w: f32,
    h: f32,
    at: (f32, f32),
    angle: f32,
}

/// A way between two places on the ring, as written once.
struct Way {
    from: End,
    to: End,
    label: Option<Block>,
    line: usize,
    /// As the diagram draws it: its color, its weight and its dash, the
    /// marks of what recorded traces cover among them, and whether its
    /// color is its own, which its head keeps.
    stroke: Stroke,
    own_color: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    Start,
    Stop,
    Station(usize),
}

/// `diagram`'s life cycle on a ring, in its look.
pub fn draw_orbit(diagram: &Diagram) -> Result<(String, Map), Declined> {
    let skin = Skin::from_params(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    canvas.sketch(skin.sketch);
    canvas.shadows(skin.shadow);
    canvas.faces(&skin.faces);
    let family = canvas.family();
    let annotations = Annotations::new(
        &diagram.frame,
        [
            skin.title_font,
            skin.header_font,
            skin.footer_font,
            skin.caption_font,
        ],
        family,
    )?;
    let page = skin.background.unwrap_or(Color::WHITE);
    let words = |text: &str, font: Font| -> Block {
        text::block(text, font, family).unwrap_or_else(|_| text::styled(Vec::new(), font, family))
    };
    let root = &diagram.root;
    let order = ring_order(diagram);
    let quiet = Font {
        size: (skin.state.font.size - 2.0).max(8.0),
        color: Color::hex(0x888888).legible_on(page, skin.state.font.color, 4.5),
        ..skin.state.font
    };
    let mut stations: Vec<Station> = order
        .iter()
        .map(|&node| {
            let state = &root.nodes[node];
            // In the color of its own text, as the diagram draws it; that
            // was read as a color already.
            let mut font = skin.state.font;
            if let Ok(Some(color)) = super::draw::name_color(state) {
                font.color = color;
            }
            let name = words(&state.label.value, font);
            let inside = states_in(&state.children);
            let holds = (state.composite && inside > 0).then(|| {
                words(
                    &match inside {
                        1 => "1 state".to_owned(),
                        n => format!("{n} states"),
                    },
                    quiet,
                )
            });
            let (w, h) = match state.kind {
                Kind::Choice => (22.0, 22.0),
                Kind::Fork | Kind::Join => (44.0, 8.0),
                _ => {
                    let inner_w = holds
                        .as_ref()
                        .map_or(name.width, |b| name.width.max(b.width));
                    let inner_h = name.height + holds.as_ref().map_or(0.0, |b| b.height + 2.0);
                    (inner_w + 2.0 * PAD_X, inner_h + 2.0 * PAD_Y)
                }
            };
            Station {
                node,
                kind: state.kind,
                name,
                holds,
                w,
                h,
                at: (0.0, 0.0),
                angle: 0.0,
            }
        })
        .collect();
    if stations.is_empty() {
        return Err(Declined {
            line: None,
            reason: "a life cycle needs a state on its ring".to_owned(),
            refused: false,
        });
    }

    // The ring: a slot for each state and one for the opening, as large
    // as the boxes on it need to stand apart.
    let slots = stations.len() + 1;
    let slot = TAU / slots as f32;
    let extent = |s: &Station| (s.w * s.w + s.h * s.h).sqrt();
    let mut radius = LEAST;
    for pair in stations.windows(2) {
        let need = (extent(&pair[0]) + extent(&pair[1])) / 2.0 + APART;
        radius = radius.max(need / (2.0 * (slot / 2.0).sin()));
    }
    let widest = stations.iter().map(extent).fold(0.0, f32::max);
    let outer = radius + widest / 2.0 + OUTSIDE * 3.0;
    let top = annotations.top();
    // Above the ring stand only the start and the end, in its opening,
    // and the loops of a state near the top, as far as the most loops on
    // one state reach.
    let loops = order
        .iter()
        .map(|&node| {
            lifted_edges(diagram)
                .iter()
                .filter(|(from, to, _)| *from == node && *to == node)
                .count()
        })
        .max()
        .unwrap_or(0);
    let looped = match loops {
        0 => 0.0,
        n => 34.0 + 14.0 * (n - 1) as f32,
    };
    let center = (
        outer,
        top + radius + (widest / 2.0).max(3.0 * MARK) + 24.0 + looped,
    );
    let on_ring = |angle: f32, r: f32| (center.0 + r * angle.cos(), center.1 + r * angle.sin());
    for (k, station) in stations.iter_mut().enumerate() {
        station.angle = -PI / 2.0 + (k + 1) as f32 * slot;
        station.at = on_ring(station.angle, radius);
    }
    let start_angle = -PI / 2.0 + slot * 0.3;
    let stop_angle = -PI / 2.0 - slot * 0.3;
    let start_at = on_ring(start_angle, radius);
    let stop_at = on_ring(stop_angle, radius);

    let ways = ways(diagram, &order, &words, &skin)?;
    let index = |end: End| -> f32 {
        match end {
            End::Start => -0.5,
            End::Stop => stations.len() as f32 - 0.5,
            End::Station(k) => k as f32,
        }
    };
    let angle_of = |end: End| match end {
        End::Start => start_angle,
        End::Stop => stop_angle + TAU,
        End::Station(k) => stations[k].angle,
    };
    let boxed = |end: End| -> (f32, f32, f32, f32) {
        match end {
            End::Start => (start_at.0 - MARK, start_at.1 - MARK, 2.0 * MARK, 2.0 * MARK),
            End::Stop => (stop_at.0 - MARK, stop_at.1 - MARK, 2.0 * MARK, 2.0 * MARK),
            End::Station(k) => {
                let s = &stations[k];
                (s.at.0 - s.w / 2.0, s.at.1 - s.h / 2.0, s.w, s.h)
            }
        }
    };

    // The ways, under the boxes.
    let mut transitions = Vec::new();
    let mut lanes: Vec<(f32, f32, usize)> = Vec::new();
    let mut ways_so_far: Vec<(End, End)> = Vec::new();
    let mut inner: Vec<(f32, f32, usize)> = Vec::new();
    // The words placed so far, which the next keep clear of.
    let mut placed: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut tied_labels: Vec<(Block, (f32, f32), usize)> = Vec::new();
    for way in &ways {
        let (a, b) = (index(way.from), index(way.to));
        let (from, to) = (angle_of(way.from), angle_of(way.to));
        // Ways from the same place to the same place so far: each a lane
        // apart. A way back runs inside the ring, so it leaves the ring to
        // the way on, whichever is written first.
        let twin = ways_so_far
            .iter()
            .filter(|&&pair| pair == (way.from, way.to))
            .count();
        ways_so_far.push((way.from, way.to));
        let span = {
            let mut d = to - from;
            while d < 0.0 {
                d += TAU;
            }
            d
        };
        let along = b > a && b - a <= 1.0;
        let first_or_last = matches!((way.from, way.to), (End::Start, End::Station(0)))
            || (way.to == End::Stop && a == stations.len() as f32 - 1.0);
        let points: Vec<(f32, f32)> = if way.from == way.to {
            // A way back to itself: a loop outside its box.
            let (x, y, w, h) = boxed(way.from);
            let out = (angle_of(way.from).cos(), angle_of(way.from).sin());
            let edge = ((w / 2.0) * out.0.abs() + (h / 2.0) * out.1.abs()).max(10.0);
            let c = (x + w / 2.0, y + h / 2.0);
            let tangent = (-out.1, out.0);
            let reach = 34.0 + 14.0 * twin as f32;
            let foot = |side: f32| {
                (
                    c.0 + out.0 * edge + tangent.0 * 9.0 * side,
                    c.1 + out.1 * edge + tangent.1 * 9.0 * side,
                )
            };
            let far = |side: f32| {
                (
                    c.0 + out.0 * (edge + reach) + tangent.0 * 18.0 * side,
                    c.1 + out.1 * (edge + reach) + tangent.1 * 18.0 * side,
                )
            };
            cubic(foot(-1.0), far(-1.0), far(1.0), foot(1.0))
        } else if (along || first_or_last) && twin == 0 {
            // On to the next: along the ring.
            arc(center, radius, from, to)
        } else if b > a && way.to != End::Stop && way.from != End::Start {
            // Skipping ahead: outside the ring, a lane further for each
            // that it runs beside, leaving and reaching its boxes softly.
            let lane = lanes
                .iter()
                .filter(|&&(s, e, _)| s < to && e > from)
                .map(|&(_, _, l)| l + 1)
                .max()
                .unwrap_or(0);
            lanes.push((from, to, lane));
            let r = radius + widest / 2.0 + OUTSIDE * (1.0 + lane as f32);
            let ramp = (span / 4.0).min(0.35);
            let mut points = cubic(
                on_ring(from, radius),
                on_ring(from, (radius + r) / 2.0),
                on_ring(from + ramp / 2.0, r),
                on_ring(from + ramp, r),
            );
            points.extend(arc(center, r, from + ramp, to - ramp));
            points.extend(cubic(
                on_ring(to - ramp, r),
                on_ring(to - ramp / 2.0, r),
                on_ring(to, (radius + r) / 2.0),
                on_ring(to, radius),
            ));
            points
        } else {
            // Back, and to the end or from the start across the ring: an
            // arc inside it that runs beside the part of the ring it
            // passes, back the other way for a way back; a lane further
            // in for each it runs beside.
            let backward = b < a;
            let (low, high) = if backward { (to, from) } else { (from, to) };
            let lane = inner
                .iter()
                .filter(|&&(s, e, _)| s < high && e > low)
                .map(|&(_, _, l)| l + 1)
                .max()
                .unwrap_or(0);
            inner.push((low, high, lane));
            let r = (radius - widest / 2.0 - 18.0 - 16.0 * lane as f32).max(radius * 0.25);
            let span = {
                let mut d = high - low;
                while d < 0.0 {
                    d += TAU;
                }
                d
            };
            let ramp = (span / 4.0).min(0.35);
            let mut points = cubic(
                on_ring(low, radius),
                on_ring(low, (radius + r) / 2.0),
                on_ring(low + ramp / 2.0, r),
                on_ring(low + ramp, r),
            );
            points.extend(arc(center, r, low + ramp, high - ramp));
            points.extend(cubic(
                on_ring(high - ramp, r),
                on_ring(high - ramp / 2.0, r),
                on_ring(high, (radius + r) / 2.0),
                on_ring(high, radius),
            ));
            if backward {
                points.reverse();
            }
            points
        };
        let points = clipped(&points, boxed(way.from), boxed(way.to));
        if points.len() < 2 {
            continue;
        }
        let drawn = way.stroke;
        canvas.polyline(Layer::Lines, &points, drawn);
        let look = super::draw::Look {
            stroke: drawn,
            own_color: way.own_color,
        };
        super::draw::arrow_head(&mut canvas, &skin, &points, look);
        transitions.push(map::Route {
            line: way.line,
            points: points.clone(),
        });
        if let Some(label) = &way.label {
            let inside = !(along || first_or_last) || twin > 0;
            let place = free_place(
                &points,
                label,
                center,
                inside,
                &placed,
                &stations
                    .iter()
                    .map(|s| (s.at.0 - s.w / 2.0, s.at.1 - s.h / 2.0, s.w, s.h))
                    .collect::<Vec<_>>(),
            );
            placed.push((
                place.0 - label.width / 2.0,
                place.1,
                label.width,
                label.height,
            ));
            tied_labels.push((label.clone(), place, way.line));
        }
    }
    // The start, the end, where the machine has them, and the boxes on
    // the ring.
    let ink = skin.arrow.color;
    let mut spots = Vec::new();
    // One mark for every start, `[*]` and those with a name of their own,
    // and one for every end; each has its spot there, so a play finds it.
    let of = |kind: Kind| root.nodes.iter().filter(move |n| n.kind == kind);
    for start in of(Kind::Start) {
        spots.push(mark_spot(Shape::Start, start_at, start));
    }
    for end in of(Kind::End) {
        spots.push(mark_spot(Shape::End, stop_at, end));
    }
    if of(Kind::Start).next().is_some() {
        canvas.ellipse(Layer::Front, start_at, (MARK, MARK), Some(ink), None);
    }
    if of(Kind::End).next().is_some() {
        canvas.ellipse(
            Layer::Front,
            stop_at,
            (MARK, MARK),
            Some(page),
            Some(Stroke::new(ink, 1.5)),
        );
        canvas.ellipse(
            Layer::Front,
            stop_at,
            (MARK - 3.5, MARK - 3.5),
            Some(ink),
            None,
        );
    }
    for station in &stations {
        let state = &root.nodes[station.node];
        let (x, y) = (
            station.at.0 - station.w / 2.0,
            station.at.1 - station.h / 2.0,
        );
        // As the diagram paints it: a color of its own, a mark of what
        // the traces cover.
        let (fill, border, _) = super::draw::paints(state, &skin)?;
        let shape = match station.kind {
            Kind::Choice => {
                let (cx, cy) = station.at;
                let r = station.w / 2.0;
                canvas.polygon(
                    Layer::Front,
                    &[(cx, cy - r), (cx + r, cy), (cx, cy + r), (cx - r, cy)],
                    fill,
                    Some(border),
                );
                Shape::Choice
            }
            Kind::Fork | Kind::Join => {
                canvas.rect(
                    Layer::Front,
                    (x, y, station.w, station.h),
                    2.0,
                    Some(ink),
                    None,
                );
                Shape::Bar
            }
            _ => {
                canvas.rect(
                    Layer::Front,
                    (x, y, station.w, station.h),
                    skin.round,
                    fill,
                    Some(border),
                );
                canvas.text(
                    Layer::Front,
                    &station.name,
                    (station.at.0, y + PAD_Y),
                    Align::Center,
                );
                if let Some(holds) = &station.holds {
                    canvas.text(
                        Layer::Front,
                        holds,
                        (station.at.0, y + PAD_Y + station.name.height + 2.0),
                        Align::Center,
                    );
                }
                Shape::State
            }
        };
        spots.push(Spot {
            name: state.key.clone(),
            shape,
            line: state.line,
            parent: None,
            rect: crate::graph::Rect {
                x,
                y,
                w: station.w,
                h: station.h,
            },
            radius: skin.round,
            part: String::new(),
        });
    }
    // The words of the ways, over all, each tied to its line.
    for (label, at, line) in tied_labels {
        let before = canvas.tie(Some(line));
        canvas.text(Layer::Front, &label, at, Align::Center);
        canvas.tie(before);
    }
    annotations.draw(&mut canvas);
    let map = canvas.place(
        Map {
            states: spots,
            transitions,
            ..Map::default()
        },
        MARGIN,
    );
    Ok((canvas.finish(MARGIN, skin.background), map))
}

/// Where the words `label` of the way through `points` stand: beside its
/// middle, away from the ring's middle for a way along or outside it and
/// towards it for one inside; else the first place along it, on either
/// side, that keeps clear of the words `placed` and the `boxes`. As the
/// top middle of the words.
fn free_place(
    points: &[(f32, f32)],
    label: &Block,
    center: (f32, f32),
    inside: bool,
    placed: &[(f32, f32, f32, f32)],
    boxes: &[(f32, f32, f32, f32)],
) -> (f32, f32) {
    let overlaps = |(x, y, w, h): (f32, f32, f32, f32), (ox, oy, ow, oh): (f32, f32, f32, f32)| {
        x < ox + ow + 2.0 && ox < x + w + 2.0 && y < oy + oh + 2.0 && oy < y + h + 2.0
    };
    let at = |share: f32, flip: bool| -> (f32, f32) {
        let mid = ((points.len() - 1) as f32 * share).round() as usize;
        let (p, q) = (
            points[mid.saturating_sub(1)],
            points[(mid + 1).min(points.len() - 1)],
        );
        let here = points[mid];
        let (tx, ty) = (q.0 - p.0, q.1 - p.1);
        let len = (tx * tx + ty * ty).sqrt().max(0.001);
        let mut normal = (-ty / len, tx / len);
        let outward = (here.0 - center.0) * normal.0 + (here.1 - center.1) * normal.1 > 0.0;
        if (outward == inside) != flip {
            normal = (-normal.0, -normal.1);
        }
        let lift = label.height / 2.0 + 5.0 + label.width / 2.0 * normal.0.abs();
        (
            here.0 + normal.0 * lift,
            here.1 + normal.1 * lift - label.height / 2.0,
        )
    };
    let rect = |(x, y): (f32, f32)| (x - label.width / 2.0, y, label.width, label.height);
    for share in [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8] {
        for flip in [false, true] {
            let place = at(share, flip);
            let r = rect(place);
            if !placed.iter().chain(boxes).any(|&o| overlaps(r, o)) {
                return place;
            }
        }
    }
    at(0.5, false)
}

/// The spot on the map of `node`, a start or an end, at its mark.
fn mark_spot(shape: Shape, (x, y): (f32, f32), node: &super::Node) -> Spot {
    Spot {
        name: node.key.clone(),
        shape,
        line: node.line,
        parent: None,
        rect: crate::graph::Rect {
            x: x - MARK,
            y: y - MARK,
            w: 2.0 * MARK,
            h: 2.0 * MARK,
        },
        radius: MARK,
        part: String::new(),
    }
}

/// The states of the top level in the order a run reaches them from the
/// start, the first way out of each followed to its end before the next,
/// so the way a run takes most stays together round the ring and what
/// branches off comes after it; those no way reaches, in the order
/// written.
fn ring_order(diagram: &Diagram) -> Vec<usize> {
    let root = &diagram.root;
    let on_ring = |k: Kind| {
        !matches!(
            k,
            Kind::Start | Kind::End | Kind::Region | Kind::Note | Kind::Data
        )
    };
    let lifted = lifted_edges(diagram);
    let mut order = Vec::new();
    let mut seen = vec![false; root.nodes.len()];
    // Depth first, the ways out of a state taken in the order written.
    let mut stack: Vec<usize> = root
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == Kind::Start)
        .map(|(i, _)| i)
        .rev()
        .collect();
    while let Some(at) = stack.pop() {
        if std::mem::replace(&mut seen[at], true) {
            continue;
        }
        if on_ring(root.nodes[at].kind) {
            order.push(at);
        }
        // A detour first, a state that only leads back to the ring (a
        // wait for an answer), right after the state it leaves; then the
        // way that reaches the most states not yet on the ring, so the
        // life cycle stays together and a branch that ends soon (a
        // cancel) comes after it; the order written between equals.
        let mut next: Vec<usize> = lifted
            .iter()
            .filter(|(from, to, _)| *from == at && !seen[*to])
            .map(|(_, to, _)| *to)
            .collect();
        next.dedup();
        let detour = |state: usize| {
            let out: Vec<usize> = lifted
                .iter()
                .filter(|(from, to, _)| *from == state && *to != state)
                .map(|(_, to, _)| *to)
                .collect();
            !out.is_empty()
                && out
                    .iter()
                    .all(|&to| to == at || (seen[to] && on_ring(root.nodes[to].kind)))
        };
        let mut reach: Vec<(bool, usize, usize)> = next
            .iter()
            .enumerate()
            .map(|(written, &to)| (detour(to), reached(&lifted, to, &seen), written))
            .collect();
        reach.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
        stack.extend(reach.iter().rev().map(|&(_, _, written)| next[written]));
    }
    for (i, node) in root.nodes.iter().enumerate() {
        if on_ring(node.kind) && !order.contains(&i) {
            order.push(i);
        }
    }
    order
}

/// How many states `container` holds, at any depth, its regions' too.
fn states_in(container: &super::Container) -> usize {
    container
        .nodes
        .iter()
        .map(|node| usize::from(node.kind == Kind::State) + states_in(&node.children))
        .sum()
}

/// How many states not `seen` yet a run from `from` reaches.
fn reached(lifted: &[(usize, usize, &super::Transition)], from: usize, seen: &[bool]) -> usize {
    let mut found = seen.to_vec();
    let mut stack = vec![from];
    let mut count = 0;
    while let Some(at) = stack.pop() {
        if std::mem::replace(&mut found[at], true) {
            continue;
        }
        count += 1;
        stack.extend(
            lifted
                .iter()
                .filter(|(f, t, _)| *f == at && !found[*t])
                .map(|(_, t, _)| *t),
        );
    }
    count
}

/// The transitions of the top level, and those into and out of the states
/// inside a state as the ways of that state: its node at the top level at
/// each end, and the transition.
fn lifted_edges(diagram: &Diagram) -> Vec<(usize, usize, &super::Transition)> {
    let root = &diagram.root;
    // A hidden arrow only places states in the layout; the ring has none.
    let shown = |t: &super::Transition| t.dash != super::Dash::Hidden;
    let mut out: Vec<(usize, usize, &super::Transition)> = root
        .edges
        .iter()
        .filter(|t| shown(t))
        .map(|t| (t.from, t.to, t))
        .collect();
    for crossing in root.crossings.iter().filter(|c| shown(&c.arrow)) {
        if let (Some(&from), Some(&to)) = (crossing.from.first(), crossing.to.first()) {
            out.push((from, to, &crossing.arrow));
        }
    }
    out.sort_by_key(|(_, _, t)| t.line);
    out
}

/// The ways between the places on the ring, each transition once.
fn ways(
    diagram: &Diagram,
    order: &[usize],
    words: &dyn Fn(&str, Font) -> Block,
    skin: &Skin,
) -> Result<Vec<Way>, Declined> {
    let root = &diagram.root;
    let end_of = |node: usize| -> Option<End> {
        match root.nodes[node].kind {
            Kind::Start => Some(End::Start),
            Kind::End => Some(End::Stop),
            Kind::Region => None,
            _ => order.iter().position(|&o| o == node).map(End::Station),
        }
    };
    let mut out = Vec::new();
    for (from, to, t) in lifted_edges(diagram) {
        let (Some(from), Some(to)) = (end_of(from), end_of(to)) else {
            continue;
        };
        out.push(Way {
            from,
            to,
            label: t
                .label
                .as_ref()
                .filter(|l| !l.value.trim().is_empty())
                .map(|l| words(&l.value, skin.arrow_font)),
            line: t.line,
            stroke: super::draw::arrow_stroke(skin, t)?,
            own_color: t.color.is_some(),
        });
    }
    Ok(out)
}

/// Points along the circle round `center` of radius `r`, clockwise from
/// angle `from` to `to`.
fn arc(center: (f32, f32), r: f32, from: f32, to: f32) -> Vec<(f32, f32)> {
    let mut to = to;
    while to < from {
        to += TAU;
    }
    (0..=SAMPLES)
        .map(|i| {
            let a = from + (to - from) * i as f32 / SAMPLES as f32;
            (center.0 + r * a.cos(), center.1 + r * a.sin())
        })
        .collect()
}

/// Points along the cubic curve from `p0` to `p3`.
fn cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32)) -> Vec<(f32, f32)> {
    (0..=SAMPLES)
        .map(|i| {
            let t = i as f32 / SAMPLES as f32;
            let m = 1.0 - t;
            let (a, b, c, d) = (m * m * m, 3.0 * m * m * t, 3.0 * m * t * t, t * t * t);
            (
                a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
            )
        })
        .collect()
}

/// `points` from where they leave the box `from` to where they reach the
/// box `to`, cut where they cross the edges.
fn clipped(
    points: &[(f32, f32)],
    from: (f32, f32, f32, f32),
    to: (f32, f32, f32, f32),
) -> Vec<(f32, f32)> {
    let inside = |(x, y): (f32, f32), (bx, by, bw, bh): (f32, f32, f32, f32)| {
        x >= bx - 1.0 && x <= bx + bw + 1.0 && y >= by - 1.0 && y <= by + bh + 1.0
    };
    // Where the run from `a` (in) to `b` (out) of `bx` crosses its edge.
    let edge = |a: (f32, f32), b: (f32, f32), bx: (f32, f32, f32, f32)| {
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        for _ in 0..20 {
            let mid = (lo + hi) / 2.0;
            let p = (a.0 + (b.0 - a.0) * mid, a.1 + (b.1 - a.1) * mid);
            if inside(p, bx) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        (a.0 + (b.0 - a.0) * hi, a.1 + (b.1 - a.1) * hi)
    };
    let Some(first) = points.iter().position(|&p| !inside(p, from)) else {
        return Vec::new();
    };
    let Some(last) = points.iter().rposition(|&p| !inside(p, to)) else {
        return Vec::new();
    };
    if first > last {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(last - first + 3);
    if first > 0 {
        out.push(edge(points[first - 1], points[first], from));
    }
    out.extend_from_slice(&points[first..=last]);
    if last + 1 < points.len() {
        out.push(edge(points[last + 1], points[last], to));
    }
    out.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagram(code: &str) -> Diagram {
        match crate::parse(code).unwrap() {
            crate::Diagram::State(d) => d,
            _ => panic!("a state diagram"),
        }
    }

    fn names(d: &Diagram, order: &[usize]) -> Vec<String> {
        order.iter().map(|&i| d.root.nodes[i].key.clone()).collect()
    }

    #[test]
    fn the_ring_holds_the_life_cycle_together_and_what_branches_after() {
        let d = diagram(
            "@startuml\n[*] --> Ordered\nOrdered --> Cancelled : cancel\nOrdered --> Paid : pay\nPaid --> Shipped : ship\nPaid --> Checking : check\nChecking --> Paid : checked\nShipped --> Paid : back\nCancelled --> [*]\nShipped --> [*]\nstate Lost\n@enduml\n",
        );
        // The way that reaches the most first, the cancel after it, a state
        // no way reaches last; a detour right after the state it leaves.
        assert_eq!(
            names(&d, &ring_order(&d)),
            [
                "Ordered",
                "Paid",
                "Checking",
                "Shipped",
                "Cancelled",
                "Lost"
            ]
        );
    }

    #[test]
    fn the_map_says_where_each_state_and_way_is_for_a_play() {
        let code = "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nstate Big {\n  [*] --> Inside\n}\nOpen --> Big : grow\nBig --> [*]\n@enduml\n";
        let (svg, map) = draw_orbit(&diagram(code)).unwrap();
        assert!(svg.starts_with("<svg"));
        let named: Vec<(&str, usize)> = map
            .states
            .iter()
            .filter(|s| s.shape == Shape::State)
            .map(|s| (s.name.as_str(), s.line))
            .collect();
        assert_eq!(named, [("Closed", 1), ("Open", 2), ("Big", 4)]);
        // Every transition of the top level, by its line; the way out of
        // the state around another as that state's.
        let mut lines: Vec<usize> = map.transitions.iter().map(|t| t.line).collect();
        lines.sort();
        assert_eq!(lines, [1, 2, 3, 7, 8]);
        // Its words tied to their lines.
        assert!(map.tied.iter().any(|t| t.line == 2));
        // A state around others says how many it holds.
        assert!(svg.contains("1 state"));
    }

    /// A state's name in its own text's color, as on its fill in the
    /// diagram.
    #[test]
    fn a_station_s_name_takes_its_own_text_color() {
        let code = "@startuml\n[*] --> Dark\nstate Dark #black;text:white\n@enduml\n";
        let (svg, _) = draw_orbit(&diagram(code)).unwrap();
        let at = svg.find(">Dark</tspan>").unwrap();
        let start = svg[..at].rfind("<tspan").unwrap();
        assert!(
            svg[start..at].contains("fill=\"#FFFFFF\""),
            "{}",
            &svg[start..at]
        );
    }

    /// A hidden arrow is no way on the ring, and a start with a name has
    /// its spot at the start's mark as `[*]` has.
    #[test]
    fn the_ring_leaves_hidden_arrows_out_and_finds_each_start() {
        let code = "@startuml\nstate s1 <<start>>\n[*] --> A\ns1 --> B\nA -[hidden]-> B : secret\nB --> [*]\n@enduml\n";
        let (svg, map) = draw_orbit(&diagram(code)).unwrap();
        assert!(!svg.contains("secret"));
        let mut lines: Vec<usize> = map.transitions.iter().map(|t| t.line).collect();
        lines.sort();
        assert_eq!(lines, [2, 3, 5]);
        let starts: Vec<(&str, usize)> = map
            .states
            .iter()
            .filter(|s| s.shape == Shape::Start)
            .map(|s| (s.name.as_str(), s.line))
            .collect();
        assert_eq!(starts, [("s1", 1), ("", 2)]);
    }

    /// The route of the transition on `line` in the orbit of `code`.
    fn route(code: &str, line: usize) -> Vec<(f32, f32)> {
        let (_, map) = draw_orbit(&diagram(code)).unwrap();
        map.transitions
            .iter()
            .find(|t| t.line == line)
            .map(|t| t.points.clone())
            .unwrap()
    }

    #[test]
    fn a_way_on_runs_along_the_ring_whether_its_way_back_is_written_first_or_not() {
        let on_first = "@startuml\n[*] --> A\nA --> B : on\nB --> A : back\nB --> [*]\n@enduml\n";
        let back_first = "@startuml\n[*] --> A\nB --> A : back\nA --> B : on\nB --> [*]\n@enduml\n";
        assert_eq!(route(on_first, 2), route(back_first, 3));
    }

    #[test]
    fn a_machine_without_a_start_or_an_end_has_neither_on_its_ring() {
        let code = "@startuml\nstate Red\nRed --> Green : go\nGreen --> Yellow : slow\nYellow --> Red : stop\n@enduml\n";
        let (svg, map) = draw_orbit(&diagram(code)).unwrap();
        assert!(
            !map.states
                .iter()
                .any(|s| matches!(s.shape, Shape::Start | Shape::End)),
            "{:?}",
            map.states
        );
        assert!(!svg.contains("<ellipse"), "{svg}");
    }

    /// What recorded traces cover is marked on the ring as in the
    /// diagram: a way taken bold in green with how often, one never taken
    /// dashed in red, and so its state.
    #[test]
    fn coverage_is_marked_on_the_ring() {
        let code = "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n' mark (Omaspec) line 2 taken 3\n' mark (Omaspec) line 3 missed\n' mark (Omaspec) state missed Open\n@enduml\n";
        let (svg, _) = crate::render_orbit(code, None).unwrap();
        assert!(svg.contains("open ×3"), "{svg}");
        assert!(svg.to_ascii_uppercase().contains("#2F9E44"), "{svg}");
        assert!(svg.contains("stroke-dasharray"), "{svg}");
    }
}
