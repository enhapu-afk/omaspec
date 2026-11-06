//! Subgraphs laid out with what is around them ([`open`]). A subgraph with
//! a direction of its own stays a box, laid out on its own.

use std::collections::HashMap;

use omaspec_uml::graph::open::{self, Line, Nested};
use omaspec_uml::graph::{self, Rect};
use omaspec_uml::{Block, Family, Font};

use super::{
    COMP_PAD, Flip, Item, Look, Measured, Member, Palette, Placed, Scene, Spot, Tree, axes, caps,
    edge_look, measure_edge, measure_member, nest, shape,
};

/// Lays the container at `spot` out, and every subgraph it opens with it.
pub(super) fn layout_open(
    tree: &Tree,
    spot: Spot,
    palette: &Palette,
    family: Family,
) -> Option<Scene> {
    open::layout(&Charts::new(tree, palette, family), spot).ok()
}

/// A flowchart's containers, with what each holds and the edges of mmdr's
/// graph each draws.
struct Charts<'a> {
    tree: &'a Tree<'a>,
    palette: &'a Palette,
    family: Family,
    members: HashMap<Spot, Vec<Member>>,
    /// How deep each container lies.
    depths: HashMap<Spot, usize>,
    /// The edges each container draws, in the order of the graph.
    edges: HashMap<Spot, Vec<usize>>,
}

impl<'a> Charts<'a> {
    fn new(tree: &'a Tree<'a>, palette: &'a Palette, family: Family) -> Charts<'a> {
        let members = tree
            .spots
            .values()
            .map(|&spot| (spot, tree.members(spot)))
            .collect();
        let depths = tree
            .spots
            .iter()
            .map(|(path, &spot)| (spot, path.len()))
            .collect();
        let mut edges: HashMap<Spot, Vec<usize>> = HashMap::new();
        for (index, meet) in tree.meets.iter().enumerate() {
            if !meet.inward {
                edges.entry(meet.spot).or_default().push(index);
            }
        }
        Charts {
            tree,
            palette,
            family,
            members,
            depths,
            edges,
        }
    }

    fn member(&self, spot: Spot, index: usize) -> Option<&Member> {
        self.members.get(&spot)?.get(index)
    }

    fn edges(&self, spot: Spot) -> &[usize] {
        self.edges.get(&spot).map_or(&[], Vec::as_slice)
    }
}

impl Nested for Charts<'_> {
    type Container = Spot;
    type Item = Item;
    type Look = Look;
    type Error = ();

    fn members(&self, spot: Spot) -> usize {
        self.members.get(&spot).map_or(0, Vec::len)
    }

    fn inner(&self, spot: Spot, index: usize) -> Option<Spot> {
        let Member::Group(group) = self.member(spot, index)? else {
            return None;
        };
        let inner = Spot::In(*group);
        let opens = self.members(inner) > 0 && self.tree.flow(inner) == self.tree.flow(spot);
        opens.then_some(inner)
    }

    fn holds(&self, spot: Spot) -> Vec<graph::hold::Held> {
        self.tree.held.get(&spot).cloned().unwrap_or_default()
    }

    fn bundled(&self, _spot: Spot) -> bool {
        self.tree.bundled
    }

    fn apart_at(&self, spot: Spot) -> Vec<usize> {
        self.tree.apart.get(&spot).cloned().unwrap_or_default()
    }

    fn flow(&self, spot: Spot) -> (graph::Direction, bool) {
        let (axis, flip) = axes(self.tree.flow(spot));
        (axis, !matches!(flip, Flip::No))
    }

    fn lines(&self, spot: Spot) -> Vec<Line> {
        let depth = self.depths.get(&spot).copied().unwrap_or(0);
        let below = |id: &String| {
            self.tree
                .paths
                .get(id)
                .and_then(|path| path.get(depth..))
                .map_or_else(Vec::new, <[usize]>::to_vec)
        };
        self.edges(spot)
            .iter()
            .map(|&index| {
                let meet = &self.tree.meets[index];
                let edge = &self.tree.graph.edges[index];
                let (from, to) = if meet.crossing {
                    (below(&edge.from), below(&edge.to))
                } else {
                    (vec![meet.from], vec![meet.to])
                };
                Line {
                    from,
                    to,
                    hidden: meet.hidden,
                }
            })
            .collect()
    }

    fn measure(&self, spot: Spot, index: usize, open: bool) -> Result<Measured, ()> {
        let measured = match self.member(spot, index).ok_or(())? {
            Member::Group(group) if open => {
                super::measure_group(self.tree, *group, self.palette, self.family, true)
            }
            member => measure_member(self.tree, member, self.palette, self.family),
        };
        measured.ok_or(())
    }

    fn frame(&self, spot: Spot, item: &Measured) -> graph::Frame {
        let (_, flip) = axes(self.tree.flow(spot));
        frame_of(item, flip)
    }

    fn line(&self, spot: Spot, index: usize) -> Result<(Block, Look), ()> {
        let edge = *self.edges(spot).get(index).ok_or(())?;
        let (stroke, color) = edge_look(self.tree.graph, edge, self.palette).ok_or(())?;
        let label = measure_edge(
            &self.tree.graph.edges[edge].label,
            Font::new(self.palette.size, color),
            self.family,
        );
        let caps = caps(&self.tree.graph.edges[edge]);
        Ok((label, Look { edge, stroke, caps }))
    }

    /// The cap at the end the line arrives at, and its stroke.
    fn arrival(&self, look: &Look) -> u64 {
        graph::arrival(look.caps.1, &look.stroke)
    }

    fn settle(&self, item: &mut Measured, rect: Rect) {
        if item.item.nested {
            let ox = rect.x + (rect.w - item.children.width) / 2.0;
            let oy = rect.y + COMP_PAD + item.item.header_h + item.item.gap;
            item.children.offset(ox, oy);
        }
    }

    fn clip(&self, end: &Placed, points: &mut [(f32, f32)], at_start: bool) {
        if let Some(outline) = end.item.kind.outline(end.rect) {
            shape::clip(points, &outline, at_start);
        }
    }

    fn band(&self, node: &Placed) -> Rect {
        nest::cluster_title(node).unwrap_or(Rect {
            h: COMP_PAD + node.item.header_h,
            ..node.rect
        })
    }
}

/// The room an opened subgraph keeps around what it holds, as
/// [`super::measure_group`] sizes one around a layout of its own; the room
/// for the title comes first along the flow, so a mirrored flow takes it
/// from the other end. Lines from above pass beside the title, unless the
/// flow is mirrored.
fn frame_of(item: &Measured, flip: Flip) -> graph::Frame {
    let item = &item.item;
    let header_w = item
        .description
        .iter()
        .map(|block| block.width)
        .fold(item.title.width, f32::max);
    let (top, bottom) = (COMP_PAD + item.header_h + super::COMP_GAP, COMP_PAD);
    let (top, bottom) = match flip {
        Flip::Y => (bottom, top),
        _ => (top, bottom),
    };
    let width = item.title.width + 2.0 * nest::TITLE_AIR + 4.0;
    let name = match flip {
        Flip::Y => graph::Name::default(),
        _ => graph::Name {
            share: 0.5,
            offset: -width / 2.0,
            width,
            height: COMP_PAD + item.header_h,
        },
    };
    graph::Frame {
        parent: None,
        top,
        bottom,
        left: COMP_PAD,
        right: COMP_PAD,
        min_width: header_w + COMP_PAD * 2.0,
        name,
    }
}
