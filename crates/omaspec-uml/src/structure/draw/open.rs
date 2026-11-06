//! Containers laid out with what is around them ([`open`]). A package,
//! node or frame with a line from itself to itself stays a box, laid out
//! on its own.

use super::{
    Block, Declined, Direction, Draft, Family, Group, Item, Look, Measured, NOTE_GAP, Placed, Rect,
    Scene, Side, Skin, arrow_stroke, card_rects, child_origin, containers, frame_of, graph,
    graph_direction, link_blocks, measure_element, name_band, note_size, place_notes, text_block,
};
use crate::graph::open::{self, Line, Nested, Room};

/// Lays `group` out, and every container it opens with it.
pub(super) fn layout_group(group: &Group, skin: &Skin, family: Family) -> Result<Scene, Declined> {
    if group.nodes.is_empty() {
        return Ok(Scene::empty());
    }
    open::layout(&Elements { skin, family }, group)
}

/// The elements of one diagram, in its skin.
struct Elements<'a> {
    skin: &'a Skin,
    family: Family,
}

impl Elements<'_> {
    /// The note `index` of `group`, measured.
    fn draft(&self, group: &Group, index: usize) -> Result<Draft, Declined> {
        let note = &group.notes[index];
        if note.target >= group.nodes.len() {
            return Err(Declined::at(note.line, "a note needs an element"));
        }
        let text = text_block(&note.text, self.skin.note.font, self.family)?;
        let (w, h) = note_size(&text);
        Ok(Draft {
            target: note.target,
            side: note.side,
            w,
            h,
            text,
        })
    }
}

impl<'a> Nested for Elements<'a> {
    type Container = &'a Group;
    type Item = Item;
    type Look = Look;
    type Error = Declined;

    fn members(&self, group: &'a Group) -> usize {
        group.nodes.len()
    }

    fn inner(&self, group: &'a Group, index: usize) -> Option<&'a Group> {
        let node = group.nodes.get(index)?;
        node.composite.then_some(&node.children)
    }

    fn holds(&self, group: &'a Group) -> Vec<graph::hold::Held> {
        group.held.clone()
    }

    fn bundled(&self, group: &'a Group) -> bool {
        group.bundled
    }

    fn apart_at(&self, group: &'a Group) -> Vec<usize> {
        group.apart.clone()
    }

    fn flow(&self, group: &'a Group) -> (graph::Direction, bool) {
        (graph_direction(group.direction), false)
    }

    fn lines(&self, group: &'a Group) -> Vec<Line> {
        let edges = group.edges.iter().map(|edge| Line {
            from: vec![edge.from],
            to: vec![edge.to],
            hidden: false,
        });
        let crossings = group.crossings.iter().map(|crossing| Line {
            from: crossing.from.clone(),
            to: crossing.to.clone(),
            hidden: false,
        });
        edges.chain(crossings).collect()
    }

    fn measure(&self, group: &'a Group, index: usize, open: bool) -> Result<Measured, Declined> {
        measure_element(&group.nodes[index], self.skin, self.family, open)
    }

    fn frame(&self, _group: &'a Group, item: &Measured) -> graph::Frame {
        frame_of(&item.item, self.skin)
    }

    fn line(&self, group: &'a Group, index: usize) -> Result<(Block, Look), Declined> {
        let arrow = match group.edges.get(index) {
            Some(edge) => edge,
            None => &group.crossings[index - group.edges.len()].arrow,
        };
        if arrow.from >= group.nodes.len() || arrow.to >= group.nodes.len() {
            return Err(Declined::at(arrow.line, "a line needs two elements"));
        }
        let blocks = link_blocks(arrow, self.skin, self.family)?;
        let look = Look {
            from_card: blocks.from_card,
            to_card: blocks.to_card,
            from_head: arrow.from_head,
            to_head: arrow.to_head,
            stroke: arrow_stroke(self.skin, arrow)?,
        };
        Ok((blocks.label, look))
    }

    /// Room kept across the flow, so a note there is not covered by the
    /// next element. The other two sides hang off the box.
    fn rooms(&self, group: &'a Group) -> Result<Vec<(usize, Room)>, Declined> {
        let mut rooms = Vec::new();
        for index in 0..group.notes.len() {
            let draft = self.draft(group, index)?;
            let room = match (group.direction, draft.side) {
                (Direction::Down, Side::Left) => Room::Before(NOTE_GAP + draft.w),
                (Direction::Down, Side::Right) => Room::After(NOTE_GAP + draft.w),
                (Direction::Right, Side::Top) => Room::Before(NOTE_GAP + draft.h),
                (Direction::Right, Side::Bottom) => Room::After(NOTE_GAP + draft.h),
                _ => continue,
            };
            rooms.push((draft.target, room));
        }
        Ok(rooms)
    }

    fn settle(&self, item: &mut Measured, rect: Rect) {
        if item.item.composite {
            let (ox, oy) = child_origin(rect, item);
            item.children.offset(ox, oy);
        }
    }

    fn band(&self, node: &Placed) -> Rect {
        name_band(node)
    }

    /// The multiplicities, off the borders of every container.
    fn marks(&self, scene: &Scene) -> Vec<Rect> {
        let around = containers(scene);
        let around = around.as_slice();
        scene
            .wires()
            .into_iter()
            .flat_map(|edge| card_rects(edge, around))
            .collect()
    }

    /// Notes go beside their elements once the labels are placed, so they
    /// step off them.
    fn noted(&self, group: &'a Group, scene: &mut Scene) -> Result<(), Declined> {
        let mut drafts = (0..group.notes.len())
            .map(|index| self.draft(group, index))
            .collect::<Result<Vec<_>, _>>()?;
        let rects: Vec<Rect> = scene.nodes.iter().map(|node| node.rect).collect();
        scene.notes = place_notes(&mut drafts, &rects, &scene.edges);
        Ok(())
    }
}
