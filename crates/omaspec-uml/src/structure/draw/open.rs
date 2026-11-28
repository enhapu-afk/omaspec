//! Containers laid out with what is around them ([`open`]). A package,
//! node or frame with a line from itself to itself stays a box, laid out
//! on its own.

use super::super::{Head, Link};
use super::{
    Block, Declined, Direction, Draft, Family, Group, Item, Look, Measured, NOTE_GAP, Placed, Rect,
    Scene, Side, Skin, arrow_stroke, card_rects, child_origin, containers, frame_of, graph,
    graph_direction, legible_on, link_blocks, measure_element, name_band, note_colors, note_size,
    place_notes,
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
        let colors = note_colors(note, &self.skin.note)?;
        // On a fill of its own its words are moved until they read.
        let mut font = self.skin.note.font;
        if note.fill.is_some()
            && let Some(own) = colors
        {
            legible_on(&mut font, &crate::color::grounds(own.fill, own.gradient));
        }
        // A note's words are its line's, from the line it begins on.
        let text = note.text.measure_written(font, self.family)?;
        let (w, h) = note_size(&text);
        // A member's row among the members' rows: one for each member
        // before it that draws one.
        let row = note.member.map(|member| {
            use crate::structure::Member;
            let members = &group.nodes[note.target].members;
            members[..member.min(members.len())]
                .iter()
                .filter(|m| match m {
                    Member::Text { text, .. } | Member::Heading(text) => !text.value.is_empty(),
                    Member::Rule(_) => true,
                })
                .count()
        });
        Ok(Draft {
            target: note.target,
            row,
            side: note.side,
            w,
            h,
            text,
            line: note.line,
            colors,
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

    fn marked(&self, group: &'a Group, index: usize) -> bool {
        let arrow = match group.edges.get(index) {
            Some(edge) => edge,
            None => match group.crossings.get(index - group.edges.len()) {
                Some(crossing) => &crossing.arrow,
                None => return false,
            },
        };
        let written = |text: &Option<crate::common::Text>| {
            text.as_ref().is_some_and(|text| !text.value.is_empty())
        };
        written(&arrow.from_card) || written(&arrow.to_card)
    }

    /// The head at the end the line arrives at, its stroke, and whether
    /// it keeps its line's color ([`Look::own_color`]).
    fn arrival(&self, look: &Look) -> u32 {
        graph::arrival((look.to_head, look.own_color), &look.stroke)
    }

    /// A line with a head where it starts, as `Base <|-- Kind`, arrives
    /// at that box as one written the other way round would.
    fn departure(&self, look: &Look) -> u32 {
        match look.from_head {
            Head::None => 0,
            head => graph::arrival(head, &look.stroke),
        }
    }

    fn flow(&self, group: &'a Group) -> (graph::Direction, bool) {
        (graph_direction(group.direction), false)
    }

    fn lines(&self, group: &'a Group) -> Vec<Line> {
        // A line against the flow is turned when it is read, its heads
        // with it.
        let edges = group.edges.iter().map(|edge| Line {
            from: vec![edge.from],
            to: vec![edge.to],
            hidden: false,
            against: false,
        });
        let crossings = group.crossings.iter().map(|crossing| Line {
            from: crossing.from.clone(),
            to: crossing.to.clone(),
            hidden: false,
            against: false,
        });
        edges.chain(crossings).collect()
    }

    fn measure(&self, group: &'a Group, index: usize, open: bool) -> Result<Measured, Declined> {
        measure_element(
            &group.nodes[index],
            self.skin,
            self.family,
            open,
            group.direction,
        )
    }

    fn frame(&self, _group: &'a Group, item: &Measured) -> graph::Frame {
        frame_of(&item.item, self.skin)
    }

    fn level(&self, group: &'a Group, index: usize) -> Option<bool> {
        group.edges.get(index)?.level
    }

    /// Under `groupInheritance`, the lines up into one box under a
    /// triangle, alike and without words, share their stem when there
    /// are as many as it says.
    fn tree(&self, group: &'a Group, index: usize) -> Option<graph::Tree> {
        let least = self.skin.group_inheritance;
        if least == 0 {
            return None;
        }
        let edge = group.edges.get(index)?;
        let (parent, at_from) = inherits(edge)?;
        let look = |link: &'a Link| (link.dash, &link.color, link.bold);
        // The looks of the lines up into that box, in the order written:
        // a tree's key is its look's place among them.
        let mut looks = Vec::new();
        let mut count = 0;
        for other in &group.edges {
            if inherits(other).is_some_and(|(p, _)| p == parent) {
                if !looks.contains(&look(other)) {
                    looks.push(look(other));
                }
                count += usize::from(look(other) == look(edge));
            }
        }
        let key = looks.iter().position(|seen| *seen == look(edge))?;
        (count >= least.max(2)).then_some(graph::Tree {
            key: u8::try_from(key).ok()?,
            at_from,
        })
    }

    fn label_before(&self, group: &'a Group, index: usize) -> Option<bool> {
        match group.edges.get(index) {
            Some(edge) => edge.label_before,
            None => {
                group
                    .crossings
                    .get(index - group.edges.len())?
                    .arrow
                    .label_before
            }
        }
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
            note: blocks.note,
            ..Look::of(
                arrow,
                blocks.from_card,
                blocks.to_card,
                arrow_stroke(self.skin, arrow)?,
            )
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
        rooms.extend(lollipop_rooms(group, self.skin, self.family)?);
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
        let rows: Vec<&Placed> = scene.nodes.iter().collect();
        scene.notes = place_notes(&mut drafts, &rects, &rows, &scene.edges);
        Ok(())
    }
}

/// The box a line goes up into under a triangle, and whether that is its
/// first end: a line drawn, between two boxes, with no words and no
/// other head.
fn inherits(edge: &Link) -> Option<(usize, bool)> {
    let worded = |text: &Option<crate::common::Text>| {
        text.as_ref().is_some_and(|text| !text.value.is_empty())
    };
    if edge.hidden
        || edge.from == edge.to
        || worded(&edge.label)
        || worded(&edge.from_card)
        || worded(&edge.to_card)
    {
        return None;
    }
    match (edge.from_head, edge.to_head) {
        (Head::Triangle, Head::None) => Some((edge.from, true)),
        (Head::None, Head::Triangle) => Some((edge.to, false)),
        _ => None,
    }
}

/// The room each lollipop of `group` keeps for its name, after it across
/// the flow: beside the circle in a flow down the page, under it in one
/// to the right.
pub(super) fn lollipop_rooms(
    group: &Group,
    skin: &Skin,
    family: Family,
) -> Result<Vec<(usize, Room)>, Declined> {
    let mut rooms = Vec::new();
    for (index, node) in group.nodes.iter().enumerate() {
        if node.shape != super::Shape::Lollipop {
            continue;
        }
        let measured = measure_element(node, skin, family, false, group.direction)?;
        let rows = &measured.item.rows;
        let size = (
            rows.iter().map(super::Row::width).fold(0.0, f32::max),
            rows.iter().map(super::Row::height).sum(),
        );
        rooms.push((
            index,
            Room::After(super::lollipop_room(size, group.direction)),
        ));
    }
    Ok(rooms)
}
