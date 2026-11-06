//! Nested states laid out with what is around them ([`open`]). A nested
//! state with concurrent regions or with a direction of its own stays a
//! box, laid out on its own.

use super::super::{Container, Direction, Kind, Side};
use super::{
    COMP_GAP, COMP_PAD, Measured, NOTE_GAP, Placed, Scene, Skin, arrow_stroke, edge_label,
    fill_regions, graph_direction, measure_node, note_size, place_notes, text_block,
};
use crate::Declined;
use crate::graph::open::{self, Line, Nested, Room};
use crate::graph::{self, Rect};
use crate::svg::Stroke;
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
    type Look = Stroke;
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

    fn holds(&self, container: &'a Container) -> Vec<graph::hold::Held> {
        container.held.clone()
    }

    fn bundled(&self, container: &'a Container) -> bool {
        container.bundled
    }

    fn apart_at(&self, container: &'a Container) -> Vec<usize> {
        container.apart.clone()
    }

    fn flow(&self, container: &'a Container) -> (graph::Direction, bool) {
        (graph_direction(container.direction), container.mirrored)
    }

    fn lines(&self, container: &'a Container) -> Vec<Line> {
        let edges = container.edges.iter().map(|edge| Line {
            from: vec![edge.from],
            to: vec![edge.to],
            hidden: false,
        });
        let crossings = container.crossings.iter().map(|crossing| Line {
            from: crossing.from.clone(),
            to: crossing.to.clone(),
            hidden: false,
        });
        edges.chain(crossings).collect()
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
        let (top, bottom) = (COMP_PAD + item.header_h + COMP_GAP, COMP_PAD);
        let (top, bottom) = if upward { (bottom, top) } else { (top, bottom) };
        graph::Frame {
            parent: None,
            top,
            bottom,
            left: COMP_PAD,
            right: COMP_PAD,
            min_width: header_w + COMP_PAD * 2.0,
            name: if upward {
                graph::Name::default()
            } else {
                graph::Name {
                    share: 0.5,
                    offset: -name / 2.0,
                    width: name,
                    height: COMP_PAD + item.header_h,
                }
            },
        }
    }

    /// Every transition ends in the same head; its stroke may differ.
    fn arrival(&self, stroke: &Stroke) -> u64 {
        graph::arrival((), stroke)
    }

    fn line(&self, container: &'a Container, index: usize) -> Result<(Block, Stroke), Declined> {
        let arrow = match container.edges.get(index) {
            Some(edge) => edge,
            None => &container.crossings[index - container.edges.len()].arrow,
        };
        Ok((
            edge_label(arrow, self.skin, self.family)?,
            arrow_stroke(self.skin, arrow)?,
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
            let text = text_block(&note.text, self.skin.note.font, self.family)?;
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
        let oy = rect.y + COMP_PAD + item.item.header_h + item.item.gap;
        item.children.offset(ox, oy);
        fill_regions(
            &mut item.children,
            rect,
            rect.y + COMP_PAD + item.item.header_h,
        );
    }

    fn band(&self, node: &Placed) -> Rect {
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
            h: COMP_PAD + node.item.header_h,
        }
    }

    /// Notes go beside their states once the labels are placed, so they
    /// step off them.
    fn noted(&self, container: &'a Container, scene: &mut Scene) -> Result<(), Declined> {
        let rects: Vec<Rect> = scene.nodes.iter().map(|node| node.rect).collect();
        scene.notes = place_notes(container, &rects, &scene.edges, self.skin, self.family)?;
        Ok(())
    }
}
