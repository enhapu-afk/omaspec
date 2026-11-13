//! Laying an architecture diagram out: the services on a board of tiles
//! by the sides their edges name, the groups as frames around them.

use super::paint::page_ink;
use super::route::route_all;
use super::{Diagram, Icon, JUNCTION, Side, TILE};
use crate::Declined;
use crate::text::{self, Block, Family, Font};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy)]
pub(super) struct Rect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl Rect {
    pub(super) fn cx(self) -> f32 {
        self.x + self.w / 2.0
    }

    pub(super) fn cy(self) -> f32 {
        self.y + self.h / 2.0
    }

    pub(super) fn right(self) -> f32 {
        self.x + self.w
    }

    pub(super) fn bottom(self) -> f32 {
        self.y + self.h
    }

    pub(super) fn intersects(self, other: Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

pub(super) struct PlacedNode {
    pub(super) tile: Rect,
    pub(super) label: Option<Block>,
    pub(super) label_rect: Option<Rect>,
}

pub(super) struct PlacedGroup {
    pub(super) rect: Rect,
    pub(super) label: Block,
    pub(super) label_rect: Rect,
    pub(super) icon: Icon,
    pub(super) empty: bool,
}

pub(super) struct PlacedEdge {
    pub(super) points: Vec<(f32, f32)>,
    pub(super) arrow_start: bool,
    pub(super) arrow_end: bool,
}

pub(super) struct Laid {
    pub(super) title: Option<Block>,
    pub(super) nodes: Vec<PlacedNode>,
    pub(super) groups: Vec<PlacedGroup>,
    pub(super) edges: Vec<PlacedEdge>,
}

pub(super) struct Link {
    pub(super) source: Side,
    pub(super) target: Side,
    pub(super) neighbor: usize,
}

pub(super) fn lay(diagram: &Diagram, family: Family) -> Result<Laid, Declined> {
    let ink = page_ink(&diagram.look);
    let font = Font::new(diagram.look.font_size, ink);
    let mut labels = Vec::new();
    let mut label_w = 0.0_f32;
    let mut label_h = 0.0_f32;
    for node in &diagram.nodes {
        if node.junction || node.label.is_empty() {
            labels.push(None);
            continue;
        }
        let block = measured(&wrap(&node.label, 22), font, family, node.line)?;
        label_w = label_w.max(block.width);
        label_h = label_h.max(block.height);
        labels.push(Some(block));
    }
    let cell_w = (label_w + 28.0).max(TILE + 64.0);
    // A row has to clear the frame of the group above it: that frame's
    // bottom pad, and the title band of the group below.
    let mut group_header = 0.0_f32;
    for group in &diagram.groups {
        let block = measured(&wrap(&group.label, 28), font, family, group.line)?;
        group_header = group_header.max(block.height.max(22.0));
    }
    let cell_h = if group_header == 0.0 {
        TILE + label_h + 44.0
    } else {
        TILE + label_h + group_header + 50.0
    };
    let title = match &diagram.title {
        Some(title) => Some(measured(
            &title.value,
            Font::new(diagram.look.font_size + 2.0, ink).bold(),
            family,
            title.line,
        )?),
        None => None,
    };
    let title_h = title
        .as_ref()
        .map(|block| block.height + 12.0)
        .unwrap_or(0.0);
    let boards = place_boards(diagram);
    let mut nodes: Vec<Option<PlacedNode>> = (0..diagram.nodes.len()).map(|_| None).collect();
    let mut top = title_h;
    for board in &boards {
        let min_x = board.iter().map(|(_, x, _)| *x).min().unwrap_or(0);
        let max_y = board.iter().map(|(_, _, y)| *y).max().unwrap_or(0);
        let rows = (max_y - board.iter().map(|(_, _, y)| *y).min().unwrap_or(0)) as f32;
        for &(id, gx, gy) in board {
            let col = (gx - min_x) as f32;
            let row = (max_y - gy) as f32;
            let tile_x = col * cell_w + (cell_w - TILE) / 2.0;
            let tile_y = top + row * cell_h;
            let (tile, label, label_rect) = if diagram.nodes[id].junction {
                let origin = (TILE - JUNCTION) / 2.0;
                (
                    Rect {
                        x: tile_x + origin,
                        y: tile_y + origin,
                        w: JUNCTION,
                        h: JUNCTION,
                    },
                    None,
                    None,
                )
            } else {
                let tile = Rect {
                    x: tile_x,
                    y: tile_y,
                    w: TILE,
                    h: TILE,
                };
                let label_rect = labels[id].as_ref().map(|block| Rect {
                    x: tile.cx() - block.width / 2.0,
                    y: tile.bottom() + 4.0,
                    w: block.width,
                    h: block.height,
                });
                (tile, labels[id].clone(), label_rect)
            };
            nodes[id] = Some(PlacedNode {
                tile,
                label,
                label_rect,
            });
        }
        top += (rows + 1.0) * cell_h + 28.0;
    }
    let mut nodes: Vec<PlacedNode> = nodes
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| Declined::at(0, "an element is not placed"))?;
    let groups = place_groups(diagram, &nodes, family, font)?;
    lift_outsiders(diagram, &mut nodes, &groups);
    let edges = route_all(diagram, &nodes, &groups);
    Ok(Laid {
        title,
        nodes,
        groups,
        edges,
    })
}

pub(super) fn place_boards(diagram: &Diagram) -> Vec<Vec<(usize, i32, i32)>> {
    let mut adjacency: Vec<Vec<Link>> = (0..diagram.nodes.len()).map(|_| Vec::new()).collect();
    for edge in &diagram.edges {
        adjacency[edge.from].push(Link {
            source: edge.from_port,
            target: edge.to_port,
            neighbor: edge.to,
        });
        adjacency[edge.to].push(Link {
            source: edge.to_port,
            target: edge.from_port,
            neighbor: edge.from,
        });
    }
    let mut visited = vec![false; diagram.nodes.len()];
    let mut raw = Vec::new();
    let mut loose = Vec::new();
    for seed in 0..diagram.nodes.len() {
        if visited[seed] {
            continue;
        }
        let mut component = Vec::new();
        let mut occupied = HashSet::new();
        let mut placed = HashMap::new();
        visited[seed] = true;
        placed.insert(seed, (0, 0));
        occupied.insert((0, 0));
        component.push((seed, 0, 0));
        let mut queue = VecDeque::from([seed]);
        while let Some(id) = queue.pop_front() {
            let pos = placed[&id];
            for link in &adjacency[id] {
                if visited[link.neighbor] {
                    continue;
                }
                let desired = step(pos, link.source, link.target);
                let cell = find_free(&occupied, desired);
                visited[link.neighbor] = true;
                placed.insert(link.neighbor, cell);
                occupied.insert(cell);
                component.push((link.neighbor, cell.0, cell.1));
                queue.push_back(link.neighbor);
            }
        }
        if component.len() == 1 && diagram.nodes[seed].parent.is_none() {
            loose.push(seed);
        } else {
            raw.push(component);
        }
    }
    let mut boards: Vec<Board> = Vec::new();
    for component in raw {
        let comp_groups: HashSet<usize> = component
            .iter()
            .filter_map(|(id, _, _)| diagram.nodes[*id].parent)
            .collect();
        let target = boards
            .iter()
            .position(|board| !board.groups.is_disjoint(&comp_groups));
        match target {
            Some(index) => absorb(&mut boards[index], diagram, component, &comp_groups),
            None => {
                let occupied = component.iter().map(|(_, x, y)| (*x, *y)).collect();
                boards.push(Board {
                    cells: component,
                    occupied,
                    groups: comp_groups,
                });
            }
        }
    }
    if !loose.is_empty() {
        boards.push(Board {
            cells: loose
                .into_iter()
                .enumerate()
                .map(|(index, id)| (id, index as i32, 0))
                .collect(),
            occupied: HashSet::new(),
            groups: HashSet::new(),
        });
    }
    boards.into_iter().map(|board| board.cells).collect()
}

pub(super) struct Board {
    pub(super) cells: Vec<(usize, i32, i32)>,
    pub(super) occupied: HashSet<(i32, i32)>,
    pub(super) groups: HashSet<usize>,
}

pub(super) fn absorb(
    board: &mut Board,
    diagram: &Diagram,
    component: Vec<(usize, i32, i32)>,
    groups: &HashSet<usize>,
) {
    let shared: Vec<(i32, i32)> = board
        .cells
        .iter()
        .filter(|(id, _, _)| {
            diagram.nodes[*id]
                .parent
                .is_some_and(|group| groups.contains(&group))
        })
        .map(|(_, x, y)| (*x, *y))
        .collect();
    let anchor_x = shared.iter().map(|cell| cell.0).min().unwrap_or(0);
    let anchor_y = shared.iter().map(|cell| cell.1).min().unwrap_or(0);
    let comp_min_x = component.iter().map(|(_, x, _)| *x).min().unwrap_or(0);
    let comp_max_y = component.iter().map(|(_, _, y)| *y).max().unwrap_or(0);
    let offset = (anchor_x - comp_min_x, anchor_y - 1 - comp_max_y);
    for (id, x, y) in component {
        let cell = find_free(&board.occupied, (x + offset.0, y + offset.1));
        board.occupied.insert(cell);
        board.cells.push((id, cell.0, cell.1));
    }
    board.groups.extend(groups.iter().copied());
}

/// The neighbor's cell. `source` is the port on the node already placed and
/// `target` is the port on the node being placed. Y grows upward here and
/// is flipped when the cells become rows.
pub(super) fn step(pos: (i32, i32), source: Side, target: Side) -> (i32, i32) {
    let (x, y) = pos;
    let across = |side: Side| if side == Side::Left { -1 } else { 1 };
    let rise = |side: Side| if side == Side::Top { 1 } else { -1 };
    match (side_horizontal(source), side_horizontal(target)) {
        (true, true) => (x + across(source), y),
        (false, false) => (x, y + rise(source)),
        (true, false) => (x + across(source), y + rise(target)),
        (false, true) => (
            x + if target == Side::Left { 1 } else { -1 },
            y + rise(source),
        ),
    }
}

pub(super) fn side_horizontal(side: Side) -> bool {
    matches!(side, Side::Left | Side::Right)
}

pub(super) fn find_free(occupied: &HashSet<(i32, i32)>, desired: (i32, i32)) -> (i32, i32) {
    if !occupied.contains(&desired) {
        return desired;
    }
    for radius in 1i32..48 {
        for dy in -radius..=radius {
            let remain = radius - dy.abs();
            for dx in [-remain, remain] {
                let candidate = (desired.0 + dx, desired.1 + dy);
                if !occupied.contains(&candidate) {
                    return candidate;
                }
                if remain == 0 {
                    break;
                }
            }
        }
    }
    desired
}

pub(super) fn place_groups(
    diagram: &Diagram,
    nodes: &[PlacedNode],
    family: Family,
    font: Font,
) -> Result<Vec<PlacedGroup>, Declined> {
    let mut order: Vec<usize> = (0..diagram.groups.len()).collect();
    order.sort_by_key(|&index| group_depth(diagram, index));
    let mut placed: Vec<Option<PlacedGroup>> = (0..diagram.groups.len()).map(|_| None).collect();
    for index in order.into_iter().rev() {
        let group = &diagram.groups[index];
        let label = measured(&wrap(&group.label, 28), font, family, group.line)?;
        let mut span: Option<Rect> = None;
        for (node_index, node) in diagram.nodes.iter().enumerate() {
            if node.parent == Some(index) {
                span = Some(union(span, node_bounds(&nodes[node_index])));
            }
        }
        for (child, child_group) in diagram.groups.iter().enumerate() {
            if child_group.parent == Some(index)
                && let Some(inner) = &placed[child]
            {
                span = Some(union(span, inner.rect));
            }
        }
        let header = label.height.max(22.0);
        let header_w = 8.0 + 22.0 + 6.0 + label.width + 12.0;
        let (rect, empty) = match span {
            Some(span) => (
                Rect {
                    x: span.x - 16.0,
                    y: span.y - 10.0 - header - 8.0,
                    w: (span.w + 32.0).max(header_w),
                    h: span.h + 16.0 + 10.0 + header + 8.0,
                },
                false,
            ),
            None => (
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: header_w,
                    h: 8.0 + header + 8.0,
                },
                true,
            ),
        };
        placed[index] = Some(PlacedGroup {
            label_rect: heading(rect, &label),
            rect,
            label,
            icon: group.icon,
            empty,
        });
    }
    // An empty group was measured at the origin. Park it under the picture
    // so it stays visible without covering a service.
    let floor = nodes
        .iter()
        .map(node_bounds)
        .chain(placed.iter().filter_map(|group| {
            group
                .as_ref()
                .filter(|group| !group.empty)
                .map(|group| group.rect)
        }))
        .map(|rect| rect.bottom())
        .fold(0.0_f32, f32::max);
    let mut x = 0.0_f32;
    for group in &mut placed {
        let Some(group) = group else {
            continue;
        };
        if group.empty {
            group.rect.y = floor + 24.0;
            group.rect.x = x;
            group.label_rect = heading(group.rect, &group.label);
            x += group.rect.w + 16.0;
        }
    }
    placed
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| Declined::at(0, "a group is not placed"))
}

pub(super) fn heading(rect: Rect, label: &Block) -> Rect {
    let header = label.height.max(22.0);
    Rect {
        x: rect.x + 36.0,
        y: rect.y + 8.0 + (header - label.height) / 2.0,
        w: label.width,
        h: label.height,
    }
}

/// A service that is not a member of a group must not sit under that
/// group's frame. Nested title bands can reach the row above.
pub(super) fn lift_outsiders(diagram: &Diagram, nodes: &mut [PlacedNode], groups: &[PlacedGroup]) {
    for _ in 0..12 {
        let mut moved = false;
        for (index, node) in nodes.iter_mut().enumerate() {
            let bounds = node_bounds(node);
            for (group_index, group) in groups.iter().enumerate() {
                if member_of(diagram, index, group_index) || !bounds.intersects(group.rect) {
                    continue;
                }
                let up = bounds.bottom() - group.rect.y + 12.0;
                let down = group.rect.bottom() - bounds.y + 12.0;
                let left = bounds.right() - group.rect.x + 12.0;
                let right = group.rect.right() - bounds.x + 12.0;
                let (dx, dy) = [(0.0, -up), (0.0, down), (-left, 0.0), (right, 0.0)]
                    .into_iter()
                    .min_by(|a, b| (a.0.abs() + a.1.abs()).total_cmp(&(b.0.abs() + b.1.abs())))
                    .unwrap_or((0.0, 0.0));
                node.tile.x += dx;
                node.tile.y += dy;
                if let Some(label) = &mut node.label_rect {
                    label.x += dx;
                    label.y += dy;
                }
                moved = true;
                break;
            }
        }
        if !moved {
            break;
        }
    }
}

pub(super) fn member_of(diagram: &Diagram, node: usize, group: usize) -> bool {
    let mut parent = diagram.nodes[node].parent;
    while let Some(index) = parent {
        if index == group {
            return true;
        }
        parent = diagram.groups[index].parent;
    }
    false
}

pub(super) fn group_depth(diagram: &Diagram, index: usize) -> usize {
    match diagram.groups[index].parent {
        None => 0,
        Some(parent) => 1 + group_depth(diagram, parent),
    }
}

pub(super) fn node_bounds(node: &PlacedNode) -> Rect {
    match node.label_rect {
        Some(label) => union(Some(node.tile), label),
        None => node.tile,
    }
}

pub(super) fn union(span: Option<Rect>, rect: Rect) -> Rect {
    let Some(span) = span else {
        return rect;
    };
    let x = span.x.min(rect.x);
    let y = span.y.min(rect.y);
    Rect {
        x,
        y,
        w: span.right().max(rect.right()) - x,
        h: span.bottom().max(rect.bottom()) - y,
    }
}

pub(super) fn measured(
    text: &str,
    font: Font,
    family: Family,
    line: usize,
) -> Result<Block, Declined> {
    text::block(text, font, family).map_err(|reason| Declined::at(line, reason))
}

pub(super) fn wrap(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut out = String::new();
    let mut width = 0usize;
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        if width > 0 && width + 1 + word_len > limit {
            out.push('\n');
            width = 0;
        } else if width > 0 {
            out.push(' ');
            width += 1;
        }
        out.push_str(word);
        width += word_len;
    }
    out
}
