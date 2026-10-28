//! Repairs to mmdr's layout of state diagrams, between its layout and its
//! SVG writer.
//!
//! mmdr places states well but routes some transitions badly: through other
//! states, back into their own, in a zigzag where two states nearly touch,
//! or at a slant. So gaps too narrow for an arrow are opened first; then
//! those transitions are routed again at right angles around the states,
//! and labels that cover a state or another label move along their line.
//! The routes and the label places come from [`route`], which also routes
//! Omagram's own state diagrams.

use mermaid_rs_renderer::layout::{DiagramData, StateNoteLayout};
use mermaid_rs_renderer::{DiagramKind, EdgeLayout, Layout, NodeLayout, NodeShape};
use omagram_uml::graph::{self, route};

type Point = (f32, f32);

/// Least gap between two states facing each other: room for an arrow (10
/// long) or a label (about 24 high).
const MIN_GAP: f32 = 32.0;
/// mmdr draws state labels at this share of their measured size, padded
/// (`render.rs` and `edge_label_padding` in mmdr).
const LABEL_SCALE: f32 = 0.85;
const LABEL_PAD: (f32, f32) = (3.0, 1.6);
/// Room mmdr leaves around a diagram.
const MARGIN: f32 = 8.0;

/// Repairs `layout` if it is a state diagram.
pub fn repair(layout: &mut Layout) {
    if layout.kind != DiagramKind::State {
        return;
    }
    spread(layout);
    let moved = reroute(layout);
    place_labels(layout, &moved);
    fit(layout);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

impl Axis {
    fn other(self) -> Self {
        match self {
            Self::X => Self::Y,
            Self::Y => Self::X,
        }
    }

    fn of(self, p: &mut Point) -> &mut f32 {
        match self {
            Self::X => &mut p.0,
            Self::Y => &mut p.1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Rect {
    fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        }
    }

    fn of(node: &NodeLayout) -> Self {
        Self::new(node.x, node.y, node.width, node.height)
    }

    fn of_note(note: &StateNoteLayout) -> Self {
        Self::new(note.x, note.y, note.width, note.height)
    }

    fn around(center: Point, (width, height): (f32, f32)) -> Self {
        Self::new(
            center.0 - width / 2.0,
            center.1 - height / 2.0,
            width,
            height,
        )
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

    fn graph(self) -> graph::Rect {
        graph::Rect {
            x: self.left,
            y: self.top,
            w: self.right - self.left,
            h: self.bottom - self.top,
        }
    }

    /// Whether the segment from `a` to `b` passes through the inside;
    /// running along the border does not count.
    fn cut(self, a: Point, b: Point) -> bool {
        route::cuts(self.graph(), a, b)
    }

    fn on_border(self, p: Point) -> bool {
        let near = |a: f32, b: f32| (a - b).abs() <= 1.5;
        let within = self.grow(1.5);
        let inside =
            p.0 >= within.left && p.0 <= within.right && p.1 >= within.top && p.1 <= within.bottom;
        inside
            && (near(p.0, self.left)
                || near(p.0, self.right)
                || near(p.1, self.top)
                || near(p.1, self.bottom))
    }
}

/// Something that takes up room: a state, a note, or a composite state
/// (`group`), which may stretch.
struct Area {
    rect: Rect,
    group: bool,
}

fn state_notes(layout: &Layout) -> &[StateNoteLayout] {
    match &layout.diagram {
        DiagramData::Graph { state_notes } => state_notes,
        _ => &[],
    }
}

fn areas(layout: &Layout) -> Vec<Area> {
    let states = layout.nodes.values().filter(|n| !n.hidden).map(Rect::of);
    let notes = state_notes(layout).iter().map(Rect::of_note);
    let groups = layout
        .subgraphs
        .iter()
        .map(|g| Rect::new(g.x, g.y, g.width, g.height));
    states
        .chain(notes)
        .map(|rect| Area { rect, group: false })
        .chain(groups.map(|rect| Area { rect, group: true }))
        .collect()
}

/// Opens gaps narrower than [`MIN_GAP`] between areas facing each other:
/// everything beyond the middle of the gap moves on, composite states
/// across it grow, and lines across it stretch.
fn spread(layout: &mut Layout) {
    for axis in [Axis::Y, Axis::X] {
        // Every step widens one gap and narrows none; the bound is a guard.
        for _ in 0..256 {
            let Some((at, by)) = narrowest_gap(&areas(layout), axis) else {
                break;
            };
            insert_space(layout, axis, at, by);
        }
    }
}

/// The narrowest gap along `axis` under [`MIN_GAP`]: where to cut, and how
/// much room to add there.
fn narrowest_gap(areas: &[Area], axis: Axis) -> Option<(f32, f32)> {
    let mut best: Option<(f32, f32)> = None;
    for a in areas {
        for b in areas {
            if a.rect.encloses(b.rect) || b.rect.encloses(a.rect) {
                continue;
            }
            let gap = b.rect.span(axis).0 - a.rect.span(axis).1;
            if !(0.0..MIN_GAP).contains(&gap) {
                continue;
            }
            let (a0, a1) = a.rect.span(axis.other());
            let (b0, b1) = b.rect.span(axis.other());
            if a1.min(b1) - a0.max(b0) < 1.0 {
                continue;
            }
            let at = a.rect.span(axis).1 + gap / 2.0;
            // Only composite states can stretch across the cut.
            let straddled = areas.iter().any(|c| {
                let (start, end) = c.rect.span(axis);
                !c.group && start < at && at < end
            });
            if !straddled && best.is_none_or(|b| (gap, at) < b) {
                best = Some((gap, at));
            }
        }
    }
    best.map(|(gap, at)| (at, MIN_GAP - gap))
}

/// Moves everything beyond `at` on `axis` by `by`.
fn insert_space(layout: &mut Layout, axis: Axis, at: f32, by: f32) {
    let push = |v: &mut f32| {
        if *v > at {
            *v += by;
        }
    };
    let push_box = |pos: &mut f32, size: f32| {
        if *pos + size / 2.0 > at {
            *pos += by;
        }
    };
    for node in layout.nodes.values_mut() {
        match axis {
            Axis::X => push_box(&mut node.x, node.width),
            Axis::Y => push_box(&mut node.y, node.height),
        }
    }
    if let DiagramData::Graph { state_notes } = &mut layout.diagram {
        for note in state_notes {
            match axis {
                Axis::X => push_box(&mut note.x, note.width),
                Axis::Y => push_box(&mut note.y, note.height),
            }
        }
    }
    for group in &mut layout.subgraphs {
        let (pos, size) = match axis {
            Axis::X => (&mut group.x, &mut group.width),
            Axis::Y => (&mut group.y, &mut group.height),
        };
        if *pos > at {
            *pos += by;
        } else if *pos + *size > at {
            *size += by;
        }
    }
    for edge in &mut layout.edges {
        let anchors = [
            &mut edge.label_anchor,
            &mut edge.start_label_anchor,
            &mut edge.end_label_anchor,
        ];
        let anchors = anchors.into_iter().filter_map(Option::as_mut);
        for point in edge.points.iter_mut().chain(anchors) {
            push(axis.of(point));
        }
    }
    match axis {
        Axis::X => layout.width += by,
        Axis::Y => layout.height += by,
    }
}

/// One end of a transition.
#[derive(Debug, Clone, Copy)]
struct End {
    rect: Rect,
    shape: NodeShape,
}

impl End {
    fn route(self) -> route::End {
        route::End {
            rect: self.rect.graph(),
            flat: has_flat_sides(self.shape),
        }
    }
}

/// The states a transition connects; `None` for loops and transitions to
/// composite states, which stay as mmdr drew them.
fn ends(layout: &Layout, edge: &EdgeLayout) -> Option<(End, End)> {
    let end = |id: &str| {
        let node = layout.nodes.get(id).filter(|n| !n.hidden)?;
        Some(End {
            rect: Rect::of(node),
            shape: node.shape,
        })
    };
    if edge.from == edge.to {
        return None;
    }
    Some((end(&edge.from)?, end(&edge.to)?))
}

/// What a line between `from` and `to` must not pass through: every state
/// and note, and every composite state that holds neither end.
fn obstacles(areas: &[Area], from: Rect, to: Rect) -> Vec<Rect> {
    areas
        .iter()
        .filter(|a| !a.group || !(a.rect.encloses(from) || a.rect.encloses(to)))
        .map(|a| a.rect)
        .collect()
}

fn segments(points: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    points.windows(2).map(|w| (w[0], w[1]))
}

/// Whether mmdr's line needs a new route: it does not start and end on its
/// states, runs at a slant, passes through a state, or turns back on itself.
fn is_broken(points: &[Point], from: Rect, to: Rect, obstacles: &[Rect]) -> bool {
    let mut points = points.to_vec();
    points.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01);
    let (Some(&first), Some(&last)) = (points.first(), points.last()) else {
        return true;
    };
    if points.len() < 2 || !from.on_border(first) || !to.on_border(last) {
        return true;
    }
    let slanted =
        segments(&points).any(|(a, b)| (a.0 - b.0).abs() > 0.5 && (a.1 - b.1).abs() > 0.5);
    let blocked = segments(&points).any(|(a, b)| obstacles.iter().any(|r| r.cut(a, b)));
    let turns_back = points.windows(3).any(|w| {
        let (a, b, c) = (w[0], w[1], w[2]);
        (b.0 - a.0) * (c.0 - b.0) + (b.1 - a.1) * (c.1 - b.1) < 0.0
    });
    slanted || blocked || turns_back
}

/// Routes again the transitions mmdr routed badly; returns which it did.
fn reroute(layout: &mut Layout) -> Vec<bool> {
    let areas = areas(layout);
    let mut todo: Vec<(usize, End, End)> = layout
        .edges
        .iter()
        .enumerate()
        .filter_map(|(i, edge)| {
            let (from, to) = ends(layout, edge)?;
            let obstacles = obstacles(&areas, from.rect, to.rect);
            is_broken(&edge.points, from.rect, to.rect, &obstacles).then_some((i, from, to))
        })
        .collect();
    // Short transitions first: they get the straight lines.
    let reach = |from: End, to: End| {
        let (a, b) = (from.rect.center(), to.rect.center());
        (a.0 - b.0).abs() + (a.1 - b.1).abs()
    };
    todo.sort_by(|x, y| reach(x.1, x.2).total_cmp(&reach(y.1, y.2)));

    let mut pending = vec![false; layout.edges.len()];
    for &(i, ..) in &todo {
        pending[i] = true;
    }
    // Labels of rerouted transitions move afterwards; these stay.
    let labels: Vec<graph::Rect> = layout
        .edges
        .iter()
        .zip(&pending)
        .filter(|&(_, &rerouted)| !rerouted)
        .filter_map(|(e, _)| label_rect(e))
        .map(Rect::graph)
        .collect();
    let mut moved = vec![false; layout.edges.len()];
    for (i, from, to) in todo {
        pending[i] = false;
        let settled = || {
            layout
                .edges
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i && !pending[j])
                .map(|(_, e)| e)
        };
        let around = route::Surroundings {
            obstacles: obstacles(&areas, from.rect, to.rect)
                .into_iter()
                .map(Rect::graph)
                .collect(),
            frames: Vec::new(),
            lines: settled().flat_map(|e| segments(&e.points)).collect(),
            labels: labels.clone(),
            used: settled()
                .flat_map(|e| [e.points.first(), e.points.last()])
                .flatten()
                .copied()
                .collect(),
            origin: (MARGIN / 2.0, MARGIN / 2.0),
        };
        if let Some(points) = route::route(from.route(), to.route(), &around) {
            layout.edges[i].points = points;
            moved[i] = true;
        }
    }
    moved
}

/// Shapes a line can meet anywhere along a side, not just in its middle.
fn has_flat_sides(shape: NodeShape) -> bool {
    matches!(
        shape,
        NodeShape::Rectangle
            | NodeShape::RoundRect
            | NodeShape::ForkJoin
            | NodeShape::Stadium
            | NodeShape::Subroutine
    )
}

fn label_size(edge: &EdgeLayout) -> Option<(f32, f32)> {
    let label = edge.label.as_ref()?;
    Some((
        label.width * LABEL_SCALE + 2.0 * LABEL_PAD.0,
        label.height * LABEL_SCALE + 2.0 * LABEL_PAD.1,
    ))
}

fn label_rect(edge: &EdgeLayout) -> Option<Rect> {
    Some(Rect::around(edge.label_anchor?, label_size(edge)?))
}

/// Moves the labels of rerouted transitions, and labels that cover a
/// state or another label, to where along their line they cover least.
fn place_labels(layout: &mut Layout, moved: &[bool]) {
    let solids: Vec<Rect> = layout
        .nodes
        .values()
        .filter(|n| !n.hidden)
        .map(Rect::of)
        .chain(state_notes(layout).iter().map(Rect::of_note))
        .collect();
    let groups: Vec<Rect> = layout
        .subgraphs
        .iter()
        .map(|g| Rect::new(g.x, g.y, g.width, g.height))
        .collect();
    // A label inside or outside a composite state is fine; across its
    // border it is not.
    let covered = |rect: Rect, placed: &[Rect]| -> f32 {
        // A little air around states and labels.
        let solid: f32 = solids
            .iter()
            .chain(placed)
            .map(|s| s.grow(2.0).overlap(rect))
            .sum();
        let border: f32 = groups
            .iter()
            .filter(|g| !g.encloses(rect))
            .map(|g| g.overlap(rect))
            .sum();
        solid + border
    };
    let mut placed = Vec::new();
    let mut todo = Vec::new();
    for (i, edge) in layout.edges.iter().enumerate() {
        if edge.label.is_none() {
            continue;
        }
        match label_rect(edge) {
            Some(rect) if !moved[i] && covered(rect, &placed) <= 1.0 => placed.push(rect),
            _ => todo.push(i),
        }
    }
    let frames: Vec<graph::Rect> = groups.iter().map(|g| g.graph()).collect();
    for i in todo {
        let edge = &layout.edges[i];
        let Some(size) = label_size(edge) else {
            continue;
        };
        let lines: Vec<(Point, Point)> = layout
            .edges
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .flat_map(|(_, e)| segments(&e.points))
            .collect();
        let taken: Vec<graph::Rect> = solids.iter().chain(&placed).map(|r| r.graph()).collect();
        let room = route::LabelRoom {
            solids: &taken,
            frames: &frames,
            lines: &lines,
            origin: (MARGIN, MARGIN),
            // mmdr draws labels on a background.
            on_line: true,
        };
        let keep = edge.label_anchor.filter(|_| !moved[i]);
        if let Some(at) = route::label_spot(&edge.points, size, &room, keep) {
            layout.edges[i].label_anchor = Some(at);
            placed.push(Rect::around(at, size));
        }
    }
}

/// Grows the picture to hold the new lines and labels, moving everything
/// right and down if they reach past its left or top.
fn fit(layout: &mut Layout) {
    let reach = |layout: &Layout| {
        let mut reach = Rect::new(MARGIN, MARGIN, 0.0, 0.0);
        for edge in &layout.edges {
            let line = edge.points.iter().map(|&p| Rect::new(p.0, p.1, 0.0, 0.0));
            for r in line.chain(label_rect(edge)) {
                reach = reach.union(r);
            }
        }
        reach
    };
    let before = reach(layout);
    if before.left < MARGIN {
        insert_space(layout, Axis::X, f32::NEG_INFINITY, MARGIN - before.left);
    }
    if before.top < MARGIN {
        insert_space(layout, Axis::Y, f32::NEG_INFINITY, MARGIN - before.top);
    }
    let after = reach(layout);
    layout.width = layout.width.max(after.right + MARGIN);
    layout.height = layout.height.max(after.bottom + MARGIN);
}

#[cfg(test)]
mod tests {
    use mermaid_rs_renderer::{RenderOptions, compute_layout, parse_mermaid_strict};

    use super::*;

    const STATE: &str = include_str!("../../../omagram-core/tests/fixtures/state.mmd");

    /// State diagrams with the features that shape lines: direction,
    /// choice, fork and join, notes, nesting, regions, loops, many back edges.
    const DIAGRAMS: &[&str] = &[
        STATE,
        "stateDiagram-v2
    direction LR
    [*] --> Idle
    Idle --> Loading : fetch
    Loading --> Ready : done
    Loading --> Failed : error
    Failed --> Loading : retry
    Ready --> Idle : reset
    Failed --> [*]",
        "stateDiagram-v2
    state check <<choice>>
    [*] --> Order
    Order --> check
    check --> Paid : amount > 0
    check --> Free : amount = 0
    Paid --> Shipped
    Free --> Shipped
    Shipped --> [*]",
        "stateDiagram-v2
    state fork_state <<fork>>
    [*] --> fork_state
    fork_state --> Build
    fork_state --> Test
    fork_state --> Lint
    state join_state <<join>>
    Build --> join_state
    Test --> join_state
    Lint --> join_state
    join_state --> Release
    Release --> [*]",
        "stateDiagram-v2
    [*] --> Draft
    Draft --> Review : submit
    Review --> Draft : changes requested
    Review --> Approved : approve
    note right of Review
        Two reviewers
        must approve.
    end note
    Approved --> Published : publish
    note left of Draft : Anyone can edit
    Published --> [*]",
        "stateDiagram-v2
    [*] --> Off
    Off --> On : power
    On --> Off : power
    state On {
        [*] --> Idle
        Idle --> Playing : play
        Playing --> Paused : pause
        Paused --> Playing : play
        Playing --> Idle : stop
        state Playing {
            [*] --> Buffering
            Buffering --> Streaming : ready
            Streaming --> Buffering : stall
        }
    }
    On --> Error : fault
    Error --> Off : reset",
        "stateDiagram-v2
    [*] --> Active
    state Active {
        [*] --> NumLockOff
        NumLockOff --> NumLockOn : EvNumLockPressed
        NumLockOn --> NumLockOff : EvNumLockPressed
        --
        [*] --> CapsLockOff
        CapsLockOff --> CapsLockOn : EvCapsLockPressed
        CapsLockOn --> CapsLockOff : EvCapsLockPressed
    }",
        "stateDiagram-v2
    [*] --> Waiting
    Waiting --> Waiting : tick
    Waiting --> Working : job
    Working --> Working : progress
    Working --> Waiting : done
    Working --> Crashed : panic
    Crashed --> Waiting : restart
    Crashed --> [*]",
        "stateDiagram-v2
    [*] --> New
    New --> Assigned : assign
    New --> Rejected : reject
    Assigned --> InProgress : start
    Assigned --> New : unassign
    InProgress --> Blocked : block
    Blocked --> InProgress : unblock
    InProgress --> Review : submit
    Review --> InProgress : rework
    Review --> Done : accept
    Review --> Rejected : decline
    Done --> Reopened : reopen
    Reopened --> Assigned : assign
    Rejected --> Reopened : reopen
    Done --> [*]
    Rejected --> [*]",
    ];

    fn layout(source: &str) -> Layout {
        let options = RenderOptions::default();
        let parsed = parse_mermaid_strict(source).unwrap();
        compute_layout(&parsed.graph, &options.theme, &options.layout)
    }

    fn repaired(source: &str) -> Layout {
        let mut layout = layout(source);
        repair(&mut layout);
        layout
    }

    #[test]
    fn zigzags_slants_and_detours_through_states_are_broken() {
        let from = Rect::new(0.0, 0.0, 40.0, 20.0);
        let to = Rect::new(0.0, 30.0, 40.0, 20.0);
        let other = Rect::new(60.0, 0.0, 40.0, 50.0);
        let obstacles = [from, to, other];
        let straight = [(20.0, 20.0), (20.0, 30.0)];
        assert!(!is_broken(&straight, from, to, &obstacles));
        let zigzag = [(20.0, 20.0), (20.0, 28.0), (15.0, 22.0), (15.0, 30.0)];
        assert!(is_broken(&zigzag, from, to, &obstacles));
        let slant = [(20.0, 20.0), (15.0, 30.0)];
        assert!(is_broken(&slant, from, to, &obstacles));
        let through = [(40.0, 10.0), (80.0, 10.0), (80.0, 40.0), (40.0, 40.0)];
        assert!(is_broken(&through, from, to, &obstacles));
        let short = [(20.0, 22.0), (20.0, 30.0)];
        assert!(
            is_broken(&short, from, to, &obstacles),
            "starts off the state"
        );
    }

    #[test]
    fn no_transition_is_left_broken() {
        for source in DIAGRAMS {
            let layout = repaired(source);
            let areas = areas(&layout);
            for edge in &layout.edges {
                let Some((from, to)) = ends(&layout, edge) else {
                    continue;
                };
                let obstacles = obstacles(&areas, from.rect, to.rect);
                assert!(
                    !is_broken(&edge.points, from.rect, to.rect, &obstacles),
                    "{} → {}: {:?}\n{source}",
                    edge.from,
                    edge.to,
                    edge.points,
                );
            }
        }
    }

    #[test]
    fn labels_cover_no_state_and_stay_in_the_picture() {
        for source in DIAGRAMS {
            let layout = repaired(source);
            let picture = Rect::new(0.0, 0.0, layout.width, layout.height);
            let states: Vec<Rect> = layout
                .nodes
                .values()
                .filter(|n| !n.hidden)
                .map(Rect::of)
                .chain(state_notes(&layout).iter().map(Rect::of_note))
                .collect();
            for edge in &layout.edges {
                let Some(label) = label_rect(edge) else {
                    continue;
                };
                assert!(
                    picture.encloses(label),
                    "{label:?} outside {picture:?}\n{source}"
                );
                for state in &states {
                    assert!(
                        state.overlap(label) <= 1.0,
                        "{label:?} covers {state:?}\n{source}"
                    );
                }
            }
        }
    }

    #[test]
    fn states_that_nearly_touch_get_room_and_a_straight_line() {
        // mmdr leaves 9 units between them and zigzags the arrow.
        let layout = repaired(STATE);
        let unlocked = Rect::of(&layout.nodes["Unlocked"]);
        let saver = Rect::of(&layout.nodes["Saver"]);
        assert!(saver.top - unlocked.bottom >= MIN_GAP - 0.01);
        let edge = layout
            .edges
            .iter()
            .find(|e| e.from == "Unlocked" && e.to == "Saver")
            .unwrap();
        assert_eq!(edge.points.len(), 2, "{:?}", edge.points);
    }

    #[test]
    fn other_diagrams_stay_as_mmdr_drew_them() {
        let source = "flowchart TD\n  A --> B\n  B --> C\n  C --> A\n  A --> C\n";
        let before = layout(source);
        let after = repaired(source);
        let points = |l: &Layout| l.edges.iter().map(|e| e.points.clone()).collect::<Vec<_>>();
        assert_eq!(points(&before), points(&after));
        assert_eq!((before.width, before.height), (after.width, after.height));
    }
}
