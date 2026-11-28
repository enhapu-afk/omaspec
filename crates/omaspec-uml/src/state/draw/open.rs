//! Nested states laid out with what is around them ([`open`]). A nested
//! state with concurrent regions or with a direction of its own stays a
//! box, laid out on its own.

use super::super::{Container, Dash, Direction, Kind, Point, Side, Transition};
use super::{
    COMP_GAP, COMP_PAD, Look, Measured, NOTE_GAP, Placed, Scene, Skin, arrow_look, edge_label,
    fill_regions, graph_direction, measure_node, note_size, place_notes,
};
use crate::Declined;
use crate::graph::open::{self, Line, Nested, Room};
use crate::graph::{self, Rect};
use crate::text::{Block, Family};

/// Lays `container` out, and every nested state it opens with it.
pub(super) fn layout_open(
    container: &Container,
    skin: &Skin,
    family: Family,
) -> Result<Scene, Declined> {
    open::layout(&States { skin, family }, container)
}

/// The states of one diagram, in its skin.
struct States<'a> {
    skin: &'a Skin,
    family: Family,
}

impl<'a> Nested for States<'a> {
    type Container = &'a Container;
    type Item = super::Item;
    type Look = Look;
    type Error = Declined;

    fn members(&self, container: &'a Container) -> usize {
        container.nodes.len()
    }

    fn inner(&self, container: &'a Container, index: usize) -> Option<&'a Container> {
        let node = container.nodes.get(index)?;
        let inner = &node.children;
        let opens = node.composite
            && node.kind == Kind::State
            && inner.split.is_none()
            && inner.direction == container.direction
            && !inner.nodes.is_empty();
        opens.then_some(inner)
    }

    /// The states held by hand, and the second state of a hidden arrow
    /// turned across the flow beside its first ([`held_beside`]): the hold
    /// of docs/adr/0021, written in the source, as in a class diagram.
    fn holds(&self, container: &'a Container) -> Vec<graph::hold::Held> {
        let turned = container.edges.iter().filter_map(|edge| {
            let after = held_beside(container, edge)?;
            Some(graph::hold::Held {
                line: edge.line,
                index: edge.to,
                beside: edge.from,
                word: match after {
                    true => graph::hold::Word::After,
                    false => graph::hold::Word::Before,
                },
            })
        });
        container.held.iter().cloned().chain(turned).collect()
    }

    fn bundled(&self, container: &'a Container) -> bool {
        container.bundled
    }

    /// A choice's diamond and the circles: their outlines reach the
    /// middle of each side, and a line's end is clipped to them.
    fn round(&self, container: &'a Container, index: usize) -> bool {
        container.nodes.get(index).is_some_and(|node| {
            matches!(
                node.kind,
                Kind::Choice | Kind::Start | Kind::End | Kind::History | Kind::DeepHistory
            ) || node.point() == Some(Point::Circle)
        })
    }

    /// A line's end on a diamond or a circle, where it meets the box
    /// around it off the middle of a side (as several lines on one side
    /// do), moved along its last run onto the outline.
    fn clip(&self, end: &Placed, points: &mut [graph::route::Point], at_start: bool) {
        let ellipse = match end.item.kind {
            Kind::Choice => false,
            Kind::Start | Kind::End | Kind::History | Kind::DeepHistory => true,
            Kind::Entry | Kind::Exit if end.item.point == Some(Point::Circle) => true,
            _ => return,
        };
        let n = points.len();
        if n < 2 {
            return;
        }
        let (at, before) = if at_start { (0, 1) } else { (n - 1, n - 2) };
        let (p, q) = (points[at], points[before]);
        let r = end.rect;
        let (cx, cy, a, b) = (r.x + r.w / 2.0, r.y + r.h / 2.0, r.w / 2.0, r.h / 2.0);
        if a <= 0.0 || b <= 0.0 {
            return;
        }
        // How far from the middle the outline lies, across at `t` of the
        // half the box is wide (or high), from 0 at the middle to 1.
        let reach = |t: f32| {
            if ellipse {
                (1.0 - t * t).max(0.0).sqrt()
            } else {
                1.0 - t
            }
        };
        if (p.0 - q.0).abs() < 0.01 {
            let t = ((p.0 - cx) / a).abs();
            if t < 1.0 {
                let dy = b * reach(t);
                points[at].1 = if p.1 < cy { cy - dy } else { cy + dy };
            }
        } else if (p.1 - q.1).abs() < 0.01 {
            let t = ((p.1 - cy) / b).abs();
            if t < 1.0 {
                let dx = a * reach(t);
                points[at].0 = if p.0 < cx { cx - dx } else { cx + dx };
            }
        }
    }

    fn spans(&self, container: &'a Container, index: usize) -> bool {
        container
            .nodes
            .get(index)
            .is_some_and(|node| matches!(node.kind, Kind::Fork | Kind::Join))
    }

    /// The members the writer kept apart, and every fork and join: the
    /// lines a bar splits and joins each meet it at a place of their own.
    fn apart_at(&self, container: &'a Container) -> Vec<usize> {
        let mut apart = container.apart.clone();
        apart.extend(
            container
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| matches!(node.kind, Kind::Fork | Kind::Join))
                .map(|(index, _)| index),
        );
        apart
    }

    fn flow(&self, container: &'a Container) -> (graph::Direction, bool) {
        (graph_direction(container.direction), container.mirrored)
    }

    /// A transition turned against the flow (`-up->` where it runs down)
    /// is ranked from its second state to its first (docs/adr/0027).
    fn lines(&self, container: &'a Container) -> Vec<Line> {
        let against =
            |toward: Option<super::super::Toward>| toward.is_some_and(|toward| toward.laid().1);
        // A hidden arrow ranks its states and is neither routed nor drawn.
        let hidden = |arrow: &Transition| arrow.dash == Dash::Hidden;
        // One turned across the flow (`-right[hidden]->`) puts its second
        // state beside its first, as PlantUML does: by a hold between two
        // states without states inside ([`held_beside`]), which takes no
        // port; onto one with states inside, where no hold is kept, as a
        // line within the rank, drawn no more than another hidden one.
        let ranks = |edge: &Transition| {
            hidden(edge)
                && (edge.toward.and_then(|t| t.laid().0).is_none()
                    || held_beside(container, edge).is_some())
        };
        let edges = container.edges.iter().map(|edge| Line {
            from: vec![edge.from],
            to: vec![edge.to],
            hidden: ranks(edge),
            against: against(edge.toward),
        });
        let crossings = container.crossings.iter().map(|crossing| Line {
            from: crossing.from.clone(),
            to: crossing.to.clone(),
            hidden: hidden(&crossing.arrow),
            against: against(crossing.arrow.toward),
        });
        edges.chain(crossings).collect()
    }

    /// A transition turned across the flow (`-right->` where it runs down)
    /// runs within the rank, its second state right beside its first on
    /// that side (docs/adr/0027); one between boxes of different states
    /// is a line like any other.
    fn level(&self, container: &'a Container, index: usize) -> Option<bool> {
        container.edges.get(index)?.toward?.laid().0
    }

    fn measure(
        &self,
        container: &'a Container,
        index: usize,
        open: bool,
    ) -> Result<Measured, Declined> {
        let node = &container.nodes[index];
        measure_node(node, container.direction, self.skin, self.family, open)
    }

    /// The room for the name comes first along the flow, so a mirrored
    /// flow takes it from the other end, and lines from above no longer
    /// pass beside it.
    fn frame(&self, container: &'a Container, item: &Measured) -> graph::Frame {
        let item = &item.item;
        let header_w = item
            .description
            .iter()
            .map(|block| block.width)
            .fold(item.title.width, f32::max);
        let name = header_w + 8.0;
        let upward = container.mirrored && container.direction == Direction::Down;
        let (top, bottom) = (item.band() + COMP_GAP, COMP_PAD);
        let (top, bottom) = if upward { (bottom, top) } else { (top, bottom) };
        // Lines from above pass beside the name: a folder's tab on its
        // left, or the middle of the band.
        let (min_width, name) = match item.folder {
            true => {
                let slant = item.tab().1;
                let name = graph::Name {
                    share: 0.0,
                    offset: 0.0,
                    width: slant,
                    height: item.band(),
                };
                (slant + COMP_PAD, name)
            }
            false => {
                let name = graph::Name {
                    share: 0.5,
                    offset: -name / 2.0,
                    width: name,
                    height: item.band(),
                };
                (header_w + COMP_PAD * 2.0, name)
            }
        };
        graph::Frame {
            parent: None,
            top,
            bottom,
            left: COMP_PAD,
            right: COMP_PAD,
            min_width,
            name: if upward { graph::Name::default() } else { name },
        }
    }

    /// Every transition ends in the same head; its stroke may differ, and
    /// whether its head keeps its color.
    fn arrival(&self, look: &Look) -> u32 {
        graph::arrival(look.own_color, &look.stroke)
    }

    fn label_before(&self, container: &'a Container, index: usize) -> Option<bool> {
        match container.edges.get(index) {
            Some(edge) => edge.label_before,
            None => {
                container
                    .crossings
                    .get(index - container.edges.len())?
                    .arrow
                    .label_before
            }
        }
    }

    fn line(&self, container: &'a Container, index: usize) -> Result<(Block, Look), Declined> {
        let arrow = match container.edges.get(index) {
            Some(edge) => edge,
            None => &container.crossings[index - container.edges.len()].arrow,
        };
        Ok((
            edge_label(arrow, self.skin, self.family)?,
            arrow_look(self.skin, arrow)?,
        ))
    }

    /// A note left or right of a state, when the states run down, sits in
    /// room kept beside it, past its self-loop.
    fn rooms(&self, container: &'a Container) -> Result<Vec<(usize, Room)>, Declined> {
        let mut rooms = Vec::new();
        if container.direction != Direction::Down {
            return Ok(rooms);
        }
        for note in &container.notes {
            let room = match note.side {
                Side::Left => Room::Before,
                Side::Right => Room::After,
                Side::Top | Side::Bottom => continue,
            };
            let text = note
                .text
                .measure_written(self.skin.note.font, self.family)?;
            let (w, _) = note_size(&text);
            rooms.push((note.target, room(NOTE_GAP + w)));
        }
        Ok(rooms)
    }

    fn settle(&self, item: &mut Measured, rect: Rect) {
        if !item.item.composite {
            return;
        }
        let ox = rect.x + (rect.w - item.children.width) / 2.0;
        let oy = rect.y + item.item.band() + item.item.gap;
        item.children.offset(ox, oy);
        fill_regions(&mut item.children, rect, rect.y + item.item.band());
    }

    fn band(&self, node: &Placed) -> Rect {
        // A folder's name is in its tab, on its left.
        if node.item.folder {
            return Rect {
                w: node.item.tab().1,
                h: node.item.band(),
                ..node.rect
            };
        }
        let w = node
            .item
            .description
            .iter()
            .map(|block| block.width)
            .fold(node.item.title.width, f32::max)
            + 8.0;
        Rect {
            x: node.rect.x + (node.rect.w - w) / 2.0,
            y: node.rect.y,
            w,
            h: node.item.band(),
        }
    }

    /// Notes go beside their states once the labels are placed, so they
    /// step off them.
    fn noted(&self, container: &'a Container, scene: &mut Scene) -> Result<(), Declined> {
        let rects: Vec<Rect> = scene.nodes.iter().map(|node| node.rect).collect();
        // A note on a transition comes once everything is laid out
        // ([`super::link_notes`]); its room is kept apart already.
        scene.notes = place_notes(container, &rects, &scene.edges, self.skin, self.family)?;
        Ok(())
    }
}

/// Whether the hidden arrow `edge` of `container`, turned across the flow
/// (`-right[hidden]->`), holds its second state after its first (`true`)
/// or before it: only between two states without states inside, which a
/// hold keeps beside each other (docs/adr/0021); `None` for any other.
fn held_beside(container: &Container, edge: &Transition) -> Option<bool> {
    if edge.dash != Dash::Hidden {
        return None;
    }
    let after = edge.toward?.laid().0?;
    let leaf = |index: usize| container.nodes.get(index).is_some_and(|n| !n.composite);
    (leaf(edge.from) && leaf(edge.to)).then_some(after)
}
