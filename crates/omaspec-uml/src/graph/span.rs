//! Boxes drawn as a bar across the flow, as long as what they split and
//! join: a state diagram's fork and join ([`super::open::Nested::spans`]).
//! A bar is measured before the layout knows where the boxes its lines
//! lead to go; boxes further apart than it is long would have their lines
//! run to its ends in a hook, and the lines into a join meet at one point
//! of it. Once laid out, each bar's boxes across the flow say how far from
//! its middle theirs are; the bar grows to reach past them, and the layout
//! is laid again while a bar grows ([`LAYOUTS`] at most). Then each bar is
//! cut back to [`PAST`] beyond the outermost line on it, as long as it was
//! measured at least: laid again, a box may come to stand nearer its
//! middle. A line within the bar's rank, into its end, keeps that end.

use super::{Direction, Edge, Frame, Layout, Node, Rect};

/// Past the middle of a bar's outer boxes, and past its outer lines, to
/// its ends.
pub(super) const PAST: f32 = 16.0;

/// How often the layout is laid at most: once, then again while a bar
/// grows, as the layout places a longer bar anew.
const LAYOUTS: usize = 3;

/// What the bars of one layout reach for.
pub(super) struct Bars<'a> {
    /// Which nodes are bars.
    pub bars: &'a [bool],
    pub edges: &'a [Edge],
    /// The frame each node stands in.
    pub within: &'a [Option<usize>],
    /// The frames, each with the one around it.
    pub frames: &'a [Frame],
    pub direction: Direction,
}

impl Bars<'_> {
    fn bar(&self, node: usize) -> bool {
        self.bars.get(node).copied().unwrap_or(false)
    }

    /// Whether `node` stands in the frame `bar` stands in, or in one
    /// inside it: a bar grows only as far as its frame can hold it.
    fn inside(&self, node: usize, bar: usize) -> bool {
        let Some(&target) = self.within.get(bar) else {
            return false;
        };
        let mut frame = self.within.get(node).copied().flatten();
        loop {
            if frame == target {
                return true;
            }
            match frame {
                Some(at) => frame = self.frames.get(at).and_then(|f| f.parent),
                None => return false,
            }
        }
    }
}

/// The layout `lay` makes of `nodes`, the bars among them as long as
/// their boxes are apart.
pub(super) fn laid(nodes: &mut [Node], bars: &Bars, lay: impl Fn(&[Node]) -> Layout) -> Layout {
    let mut layout = lay(nodes);
    if !bars.bars.contains(&true) {
        return layout;
    }
    let direction = bars.direction;
    let least: Vec<f32> = nodes.iter().map(|node| long(node, direction)).collect();
    for _ in 1..LAYOUTS {
        let mut grows = false;
        for (bar, length) in needed(&layout, bars) {
            if length > long(&nodes[bar], direction) + 0.5 {
                match direction {
                    Direction::Down => nodes[bar].width = length,
                    Direction::Right => nodes[bar].height = length,
                }
                grows = true;
            }
        }
        if !grows {
            break;
        }
        layout = lay(nodes);
    }
    trim(&mut layout, bars.bars, &least, direction);
    layout
}

/// How long `node` is across the flow.
fn long(node: &Node, direction: Direction) -> f32 {
    match direction {
        Direction::Down => node.width,
        Direction::Right => node.height,
    }
}

/// A rect's start and length across the flow, and along it.
fn axes(rect: Rect, direction: Direction) -> ((f32, f32), (f32, f32)) {
    match direction {
        Direction::Down => ((rect.x, rect.w), (rect.y, rect.h)),
        Direction::Right => ((rect.y, rect.h), (rect.x, rect.w)),
    }
}

/// How long each bar has to be to reach from its middle past the middles
/// of the boxes before and after it along the flow that its lines lead
/// to, in its frame. A line within its rank, or to a box beside it, asks
/// for nothing: that box moves out as the bar grows; nor does one out of
/// the bar's frame, which the bar could not reach.
fn needed(layout: &Layout, bars: &Bars) -> Vec<(usize, f32)> {
    let mut out: Vec<(usize, f32)> = Vec::new();
    for edge in bars.edges {
        if edge.hidden || edge.level.is_some() || edge.from == edge.to {
            continue;
        }
        for (at, other) in [(edge.from, edge.to), (edge.to, edge.from)] {
            if !bars.bar(at) || bars.bar(other) || !bars.inside(other, at) {
                continue;
            }
            let (Some(&bar), Some(&rect)) = (layout.nodes.get(at), layout.nodes.get(other)) else {
                continue;
            };
            let ((bar_x, bar_w), (bar_y, bar_h)) = axes(bar, bars.direction);
            let ((x, w), (y, h)) = axes(rect, bars.direction);
            let apart = y + h <= bar_y + 0.5 || y >= bar_y + bar_h - 0.5;
            if !apart {
                continue;
            }
            let length = 2.0 * ((x + w / 2.0 - (bar_x + bar_w / 2.0)).abs() + PAST);
            match out.iter_mut().find(|(bar, _)| *bar == at) {
                Some((_, most)) => *most = most.max(length),
                None => out.push((at, length)),
            }
        }
    }
    out
}

/// Each bar of `layout` cut back to [`PAST`] beyond the outermost end of a
/// line on it, `least` long at least, within the bar as laid out.
fn trim(layout: &mut Layout, bars: &[bool], least: &[f32], direction: Direction) {
    let ends: Vec<(f32, f32)> = layout
        .routes
        .iter()
        .flat_map(|route| route.points.first().into_iter().chain(route.points.last()))
        .copied()
        .collect();
    for (index, rect) in layout.nodes.iter_mut().enumerate() {
        if !bars.get(index).copied().unwrap_or(false) {
            continue;
        }
        let ((start, length), (at, thick)) = axes(*rect, direction);
        let end = start + length;
        let on: Vec<f32> = ends
            .iter()
            .map(|&(x, y)| match direction {
                Direction::Down => (x, y),
                Direction::Right => (y, x),
            })
            .filter(|&(along, across)| {
                along > start - 0.75
                    && along < end + 0.75
                    && across > at - 0.75
                    && across < at + thick + 0.75
            })
            .map(|(along, _)| along.clamp(start, end))
            .collect();
        let Some(low) = on.iter().copied().reduce(f32::min) else {
            continue;
        };
        let high = on.iter().copied().fold(low, f32::max);
        // Past the outer lines, within the bar as laid out, which kept its
        // room; a window shorter than the bar was measured grows about its
        // middle, moved back from an end it would run past.
        let (mut from, mut to) = ((low - PAST).max(start), (high + PAST).min(end));
        let least = least[index].min(length);
        if to - from < least {
            let window = ((from + to - least) / 2.0).clamp(start, end - least);
            (from, to) = (from.min(window), to.max(window + least));
        }
        if to - from >= length - 0.5 {
            continue;
        }
        match direction {
            Direction::Down => (rect.x, rect.w) = (from, to - from),
            Direction::Right => (rect.y, rect.h) = (from, to - from),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Route;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    fn route(points: &[(f32, f32)]) -> Route {
        Route {
            points: points.to_vec(),
            label_at: (0.0, 0.0),
            label_width: 0.0,
            label_height: 0.0,
        }
    }

    fn layout(nodes: Vec<Rect>, routes: Vec<Route>) -> Layout {
        Layout {
            nodes,
            routes,
            frames: Vec::new(),
            width: 0.0,
            height: 0.0,
            unheld: Vec::new(),
        }
    }

    #[test]
    fn a_bar_is_cut_back_to_its_lines_and_never_below_its_length() {
        // Lines at 100 and 160 on a bar from 0 to 400: 84 to 176.
        let mut laid = layout(
            vec![rect(0.0, 100.0, 400.0, 8.0)],
            vec![
                route(&[(100.0, 108.0), (100.0, 200.0)]),
                route(&[(160.0, 50.0), (160.0, 100.0)]),
            ],
        );
        trim(&mut laid, &[true], &[80.0], Direction::Down);
        assert_eq!(laid.nodes[0], rect(84.0, 100.0, 92.0, 8.0));
        // Lines near one end: the window moves in from it, 80 long.
        let mut laid = layout(
            vec![rect(0.0, 100.0, 400.0, 8.0)],
            vec![
                route(&[(4.0, 108.0), (4.0, 200.0)]),
                route(&[(10.0, 50.0), (10.0, 100.0)]),
            ],
        );
        trim(&mut laid, &[true], &[80.0], Direction::Down);
        assert_eq!(laid.nodes[0], rect(0.0, 100.0, 80.0, 8.0));
    }

    #[test]
    fn a_line_into_a_bar_s_end_keeps_that_end() {
        let mut laid = layout(
            vec![rect(0.0, 100.0, 400.0, 8.0)],
            vec![
                route(&[(100.0, 108.0), (100.0, 200.0)]),
                route(&[(400.0, 104.0), (480.0, 104.0)]),
            ],
        );
        trim(&mut laid, &[true], &[80.0], Direction::Down);
        assert_eq!(laid.nodes[0], rect(84.0, 100.0, 316.0, 8.0));
    }

    #[test]
    fn a_box_beside_a_bar_asks_for_no_length() {
        let edge = |from, to, level| Edge {
            level,
            ..crate::graph::tests::edge(from, to)
        };
        let laid = layout(
            vec![
                rect(0.0, 100.0, 80.0, 8.0),
                rect(300.0, 90.0, 60.0, 30.0),
                rect(250.0, 200.0, 60.0, 30.0),
            ],
            Vec::new(),
        );
        // The box beside the bar, in its rank, by a line within the rank or
        // not, asks for nothing; the one below asks to be reached.
        let edges = [edge(0, 1, Some(true)), edge(0, 1, None), edge(0, 2, None)];
        let bars = Bars {
            bars: &[true, false, false],
            edges: &edges,
            within: &[None, None, None],
            frames: &[],
            direction: Direction::Down,
        };
        assert_eq!(needed(&laid, &bars), [(0, 2.0 * (280.0 - 40.0 + PAST))]);
        // Nor does a box out of the bar's frame.
        let frame = Frame {
            parent: None,
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            right: 0.0,
            min_width: 0.0,
            name: Default::default(),
        };
        let bars = Bars {
            within: &[Some(0), Some(0), None],
            frames: &[frame],
            ..bars
        };
        assert_eq!(needed(&laid, &bars), []);
    }
}
