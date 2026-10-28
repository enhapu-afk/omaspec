//! Mermaid architecture diagrams (`architecture-beta`).
//!
//! Services sit on a grid the ports decide: `db:L -- R:server` puts the
//! server on the left of the database. A group is a dashed frame around
//! its members, and its title stands in a band above them so the frame
//! does not cover a sibling. A junction is a dot. The five built-in icons
//! are drawn as marks. An icon pack
//! (`logos:…`) is refused by name, because a made-up glyph would be a
//! worse picture than the one mmdr draws for that pack.
//! On the stock theme a service is the familiar architecture blue. On any
//! other theme the tile is the accent and the label follows the page.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::Declined;
use crate::color::Color;
use crate::common::{Text, keyword, text_at};
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{self, Block, Family, Font};

const TILE: f32 = 56.0;
const JUNCTION: f32 = 14.0;
const MARGIN: f32 = 16.0;
const CLASSIC_TILE: u32 = 0x087EBF;

/// How a service or a group is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Cloud,
    Database,
    Disk,
    Internet,
    Server,
    Plain,
}

/// Which side of a service an edge leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// Colors and type for one diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    pub font_size: f32,
    pub preferred: Color,
    pub background: Option<Color>,
    pub accent: Color,
    pub line: Color,
    pub classic: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            font_size: 16.0,
            preferred: Color::hex(0x333333),
            background: Some(Color::WHITE),
            accent: Color::hex(0x7B88A8),
            line: Color::hex(0x2F3B4D),
            classic: true,
        }
    }
}

/// One architecture diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    pub groups: Vec<Group>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub look: Look,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: String,
    pub label: String,
    pub icon: Icon,
    pub parent: Option<usize>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub icon: Icon,
    pub junction: bool,
    pub parent: Option<usize>,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub from_port: Side,
    pub to_port: Side,
    pub from_group: bool,
    pub to_group: bool,
    pub arrow_start: bool,
    pub arrow_end: bool,
    pub line: usize,
}

/// The diagram `source` describes, or `None` when it is not one or it uses
/// a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not an architecture diagram, and one it accepts, are
/// `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

/// The SVG. Labels sit under the tiles, and a line leaves below a label
/// when the port is the bottom.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    if !(4.0..=100.0).contains(&diagram.look.font_size) {
        return Err(Declined::at(0, "the font size is not one Omagram draws"));
    }
    let mut canvas = Canvas::new(&diagram.look.font_name, false);
    let family = canvas.family();
    let laid = lay(diagram, family)?;
    paint(&mut canvas, diagram, &laid);
    Ok(canvas.finish(MARGIN, diagram.look.background))
}

struct Reason {
    line: usize,
    what: &'static str,
}

enum Stop {
    Foreign,
    At(Reason),
}

fn fail(line: usize, what: &'static str) -> Stop {
    Stop::At(Reason { line, what })
}

fn read(source: &str) -> Result<Diagram, Stop> {
    if !source.lines().any(|raw| header_word(strip_comment(raw))) {
        return Err(Stop::Foreign);
    }
    let mut diagram = Diagram {
        title: None,
        groups: Vec::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
        look: Look::default(),
    };
    let mut ids: HashMap<String, Kind> = HashMap::new();
    let mut saw_header = false;
    let mut skipping = false;
    let mut skip_line = 0usize;
    for (line_no, raw) in source.lines().enumerate() {
        let line = strip_comment(raw);
        if line.is_empty() {
            continue;
        }
        if skipping {
            if line.contains('}') {
                skipping = false;
            }
            continue;
        }
        if let Some(rest) = keyword(line, "acctitle").or_else(|| keyword(line, "accdescr")) {
            if rest.contains('{') && !rest.contains('}') {
                skipping = true;
                skip_line = line_no;
            }
            continue;
        }
        if let Some(rest) = keyword(line, "title") {
            if rest.is_empty() || diagram.title.is_some() {
                return Err(fail(line_no, "a title"));
            }
            diagram.title = Some(text_at(&unquote(rest), line_no));
            continue;
        }
        if keyword(line, "architecture-beta").is_some() || keyword(line, "architecture").is_some() {
            if saw_header
                || keyword(line, "architecture-beta").is_some_and(|rest| !rest.is_empty())
                || (keyword(line, "architecture-beta").is_none()
                    && keyword(line, "architecture").is_some_and(|rest| !rest.is_empty()))
            {
                return Err(fail(line_no, "an unknown line"));
            }
            saw_header = true;
            continue;
        }
        if let Some(rest) = keyword(line, "group") {
            push_group(&mut diagram, &mut ids, rest, line_no)?;
            continue;
        }
        if let Some(rest) = keyword(line, "service") {
            push_service(&mut diagram, &mut ids, rest, line_no)?;
            continue;
        }
        if let Some(rest) = keyword(line, "junction") {
            push_junction(&mut diagram, &mut ids, rest, line_no)?;
            continue;
        }
        if line.contains("--") || line.contains("->") {
            push_edge(&mut diagram, &ids, line, line_no)?;
            continue;
        }
        return Err(fail(line_no, "an unknown line"));
    }
    if skipping {
        return Err(fail(skip_line, "an unknown line"));
    }
    if !saw_header {
        return Err(Stop::Foreign);
    }
    Ok(diagram)
}

#[derive(Clone, Copy)]
enum Kind {
    Group,
    Node(usize),
}

fn push_group(
    diagram: &mut Diagram,
    ids: &mut HashMap<String, Kind>,
    rest: &str,
    line: usize,
) -> Result<(), Stop> {
    let (body, parent) = split_in(rest);
    let (id, icon, label) = declaration(body, line)?;
    if ids.contains_key(&id) {
        return Err(fail(line, "an element"));
    }
    let parent = parent_group(diagram, parent, line)?;
    diagram.groups.push(Group {
        id: id.clone(),
        label,
        icon,
        parent,
        line,
    });
    ids.insert(id, Kind::Group);
    Ok(())
}

fn push_service(
    diagram: &mut Diagram,
    ids: &mut HashMap<String, Kind>,
    rest: &str,
    line: usize,
) -> Result<(), Stop> {
    let (body, parent) = split_in(rest);
    let (id, icon, label) = declaration(body, line)?;
    if ids.contains_key(&id) {
        return Err(fail(line, "an element"));
    }
    let parent = parent_group(diagram, parent, line)?;
    let index = diagram.nodes.len();
    diagram.nodes.push(Node {
        id: id.clone(),
        label,
        icon,
        junction: false,
        parent,
        line,
    });
    ids.insert(id, Kind::Node(index));
    Ok(())
}

fn push_junction(
    diagram: &mut Diagram,
    ids: &mut HashMap<String, Kind>,
    rest: &str,
    line: usize,
) -> Result<(), Stop> {
    let (body, parent) = split_in(rest);
    let mut parts = body.split_whitespace();
    let id = parts.next().unwrap_or("").trim();
    if id.is_empty() || parts.next().is_some() {
        return Err(fail(line, "an unknown line"));
    }
    if ids.contains_key(id) {
        return Err(fail(line, "an element"));
    }
    let parent = parent_group(diagram, parent, line)?;
    let index = diagram.nodes.len();
    diagram.nodes.push(Node {
        id: id.to_string(),
        label: String::new(),
        icon: Icon::Plain,
        junction: true,
        parent,
        line,
    });
    ids.insert(id.to_string(), Kind::Node(index));
    Ok(())
}

fn push_edge(
    diagram: &mut Diagram,
    ids: &HashMap<String, Kind>,
    line: &str,
    line_no: usize,
) -> Result<(), Stop> {
    let arrows = [
        ("<-->", true, true),
        ("<--", true, false),
        ("-->", false, true),
        ("--", false, false),
        ("->", false, true),
    ];
    let Some((mark, arrow_start, arrow_end)) =
        arrows.iter().find(|(mark, _, _)| line.contains(mark))
    else {
        return Err(fail(line_no, "an unknown line"));
    };
    let Some((left, right)) = line.split_once(mark) else {
        return Err(fail(line_no, "an unknown line"));
    };
    if right.contains("--") || right.contains("->") {
        return Err(fail(line_no, "an unknown line"));
    }
    let (from_id, from_port, from_group) = end_left(left, line_no)?;
    let (to_id, to_port, to_group) = end_right(right, line_no)?;
    if from_port == to_port {
        return Err(fail(line_no, "a port"));
    }
    let from = node_id(ids, &from_id, line_no)?;
    let to = node_id(ids, &to_id, line_no)?;
    if from == to {
        return Err(fail(line_no, "an edge"));
    }
    if from_group && diagram.nodes[from].parent.is_none() {
        return Err(fail(line_no, "a group"));
    }
    if to_group && diagram.nodes[to].parent.is_none() {
        return Err(fail(line_no, "a group"));
    }
    diagram.edges.push(Edge {
        from,
        to,
        from_port,
        to_port,
        from_group,
        to_group,
        arrow_start: *arrow_start,
        arrow_end: *arrow_end,
        line: line_no,
    });
    Ok(())
}

fn node_id(ids: &HashMap<String, Kind>, id: &str, line: usize) -> Result<usize, Stop> {
    match ids.get(id) {
        Some(Kind::Node(index)) => Ok(*index),
        Some(Kind::Group) => Err(fail(line, "a service")),
        None => Err(fail(line, "a service")),
    }
}

fn end_left(token: &str, line: usize) -> Result<(String, Side, bool), Stop> {
    let (id, port) = token
        .trim()
        .split_once(':')
        .ok_or_else(|| fail(line, "a port"))?;
    let (id, group) = take_group(id.trim(), line)?;
    Ok((id, side_of(port, line)?, group))
}

fn end_right(token: &str, line: usize) -> Result<(String, Side, bool), Stop> {
    let (port, id) = token
        .trim()
        .split_once(':')
        .ok_or_else(|| fail(line, "a port"))?;
    let (id, group) = take_group(id.trim(), line)?;
    Ok((id, side_of(port, line)?, group))
}

fn take_group(token: &str, line: usize) -> Result<(String, bool), Stop> {
    if token.len() >= 7 && token[token.len() - 7..].eq_ignore_ascii_case("{group}") {
        let id = token[..token.len() - 7].trim();
        if id.is_empty() {
            return Err(fail(line, "a service"));
        }
        return Ok((id.to_string(), true));
    }
    if token.contains('{') {
        return Err(fail(line, "an unknown line"));
    }
    if token.is_empty() || token.contains(char::is_whitespace) {
        return Err(fail(line, "a service"));
    }
    Ok((token.to_string(), false))
}

fn side_of(token: &str, line: usize) -> Result<Side, Stop> {
    let mut chars = token.trim().chars();
    let Some(ch) = chars.next() else {
        return Err(fail(line, "a port"));
    };
    if chars.next().is_some() {
        return Err(fail(line, "a port"));
    }
    match ch.to_ascii_uppercase() {
        'L' => Ok(Side::Left),
        'R' => Ok(Side::Right),
        'T' => Ok(Side::Top),
        'B' => Ok(Side::Bottom),
        _ => Err(fail(line, "a port")),
    }
}

fn parent_group(
    diagram: &Diagram,
    parent: Option<&str>,
    line: usize,
) -> Result<Option<usize>, Stop> {
    let Some(name) = parent else {
        return Ok(None);
    };
    if name.is_empty() || name.split_whitespace().nth(1).is_some() {
        return Err(fail(line, "a group"));
    }
    diagram
        .groups
        .iter()
        .position(|group| group.id == name)
        .map(Some)
        .ok_or_else(|| fail(line, "a group"))
}

fn declaration(body: &str, line: usize) -> Result<(String, Icon, String), Stop> {
    let body = body.trim();
    let id_end = body.find(['(', '[']).unwrap_or(body.len());
    let id = body[..id_end].trim();
    if id.is_empty() || id.contains(char::is_whitespace) || id.contains([':', '{', '}']) {
        return Err(fail(line, "an element"));
    }
    let mut rest = body[id_end..].trim();
    let mut icon = Icon::Plain;
    if let Some(stripped) = rest.strip_prefix('(') {
        let Some(end) = stripped.find(')') else {
            return Err(fail(line, "an unknown line"));
        };
        icon = icon_of(stripped[..end].trim(), line)?;
        rest = stripped[end + 1..].trim();
    }
    let mut label = id.to_string();
    if let Some(stripped) = rest.strip_prefix('[') {
        let Some(end) = stripped.rfind(']') else {
            return Err(fail(line, "an unknown line"));
        };
        if !stripped[end + 1..].trim().is_empty() {
            return Err(fail(line, "an unknown line"));
        }
        let written = unquote(stripped[..end].trim());
        if !written.is_empty() {
            label = written;
        }
        rest = "";
    }
    if !rest.is_empty() {
        return Err(fail(line, "an unknown line"));
    }
    Ok((id.to_string(), icon, label))
}

fn icon_of(name: &str, line: usize) -> Result<Icon, Stop> {
    if name.contains(':') || name.contains(['<', '>', '"', '&']) {
        return Err(fail(line, "an icon"));
    }
    Ok(match name.to_ascii_lowercase().as_str() {
        "cloud" => Icon::Cloud,
        "database" => Icon::Database,
        "disk" => Icon::Disk,
        "internet" | "globe" => Icon::Internet,
        "server" => Icon::Server,
        _ => Icon::Plain,
    })
}

fn split_in(rest: &str) -> (&str, Option<&str>) {
    let mut depth = 0i32;
    for (index, ch) in rest.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            _ if depth == 0 => {
                let tail = &rest[index..];
                if tail.len() >= 4 && tail.as_bytes()[..4].eq_ignore_ascii_case(b" in ") {
                    return (rest[..index].trim(), Some(rest[index + 4..].trim()));
                }
            }
            _ => {}
        }
    }
    (rest.trim(), None)
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    }
}

fn header_word(line: &str) -> bool {
    let Some(word) = line.split_whitespace().next() else {
        return false;
    };
    word.eq_ignore_ascii_case("architecture-beta") || word.eq_ignore_ascii_case("architecture")
}

fn strip_comment(line: &str) -> &str {
    let mut quote = false;
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    for (index, (byte, ch)) in chars.iter().enumerate() {
        if *ch == '"' {
            quote = !quote;
        }
        if !quote && *ch == '%' && chars.get(index + 1).is_some_and(|(_, next)| *next == '%') {
            return line[..*byte].trim();
        }
    }
    line.trim()
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Rect {
    fn cx(self) -> f32 {
        self.x + self.w / 2.0
    }

    fn cy(self) -> f32 {
        self.y + self.h / 2.0
    }

    fn right(self) -> f32 {
        self.x + self.w
    }

    fn bottom(self) -> f32 {
        self.y + self.h
    }

    fn intersects(self, other: Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

struct PlacedNode {
    tile: Rect,
    label: Option<Block>,
    label_rect: Option<Rect>,
}

struct PlacedGroup {
    rect: Rect,
    label: Block,
    label_rect: Rect,
    icon: Icon,
    empty: bool,
}

struct PlacedEdge {
    points: Vec<(f32, f32)>,
    arrow_start: bool,
    arrow_end: bool,
}

struct Laid {
    title: Option<Block>,
    nodes: Vec<PlacedNode>,
    groups: Vec<PlacedGroup>,
    edges: Vec<PlacedEdge>,
}

struct Link {
    source: Side,
    target: Side,
    neighbor: usize,
}

fn lay(diagram: &Diagram, family: Family) -> Result<Laid, Declined> {
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

fn place_boards(diagram: &Diagram) -> Vec<Vec<(usize, i32, i32)>> {
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

struct Board {
    cells: Vec<(usize, i32, i32)>,
    occupied: HashSet<(i32, i32)>,
    groups: HashSet<usize>,
}

fn absorb(
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
fn step(pos: (i32, i32), source: Side, target: Side) -> (i32, i32) {
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

fn side_horizontal(side: Side) -> bool {
    matches!(side, Side::Left | Side::Right)
}

fn find_free(occupied: &HashSet<(i32, i32)>, desired: (i32, i32)) -> (i32, i32) {
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

fn place_groups(
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

fn heading(rect: Rect, label: &Block) -> Rect {
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
fn lift_outsiders(diagram: &Diagram, nodes: &mut [PlacedNode], groups: &[PlacedGroup]) {
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

fn member_of(diagram: &Diagram, node: usize, group: usize) -> bool {
    let mut parent = diagram.nodes[node].parent;
    while let Some(index) = parent {
        if index == group {
            return true;
        }
        parent = diagram.groups[index].parent;
    }
    false
}

fn group_depth(diagram: &Diagram, index: usize) -> usize {
    match diagram.groups[index].parent {
        None => 0,
        Some(parent) => 1 + group_depth(diagram, parent),
    }
}

fn node_bounds(node: &PlacedNode) -> Rect {
    match node.label_rect {
        Some(label) => union(Some(node.tile), label),
        None => node.tile,
    }
}

fn union(span: Option<Rect>, rect: Rect) -> Rect {
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

fn route_all(diagram: &Diagram, nodes: &[PlacedNode], groups: &[PlacedGroup]) -> Vec<PlacedEdge> {
    let mut used: HashMap<(usize, Side, bool), usize> = HashMap::new();
    for edge in &diagram.edges {
        *used
            .entry((edge.from, edge.from_port, edge.from_group))
            .or_insert(0) += 1;
        *used
            .entry((edge.to, edge.to_port, edge.to_group))
            .or_insert(0) += 1;
    }
    let mut seen: HashMap<(usize, Side, bool), usize> = HashMap::new();
    let obstacles = obstacles(nodes, groups);
    let mut placed = Vec::new();
    for edge in &diagram.edges {
        let from_key = (edge.from, edge.from_port, edge.from_group);
        let to_key = (edge.to, edge.to_port, edge.to_group);
        let from_slot = *seen.entry(from_key).or_insert(0);
        seen.insert(from_key, from_slot + 1);
        let to_slot = *seen.entry(to_key).or_insert(0);
        seen.insert(to_key, to_slot + 1);
        let start = anchor(
            diagram,
            nodes,
            groups,
            edge.from,
            edge.from_port,
            edge.from_group,
            (from_slot, used[&from_key]),
        );
        let end = anchor(
            diagram,
            nodes,
            groups,
            edge.to,
            edge.to_port,
            edge.to_group,
            (to_slot, used[&to_key]),
        );
        placed.push(PlacedEdge {
            points: route(start, edge.from_port, end, edge.to_port, &obstacles),
            arrow_start: edge.arrow_start,
            arrow_end: edge.arrow_end,
        });
    }
    placed
}

fn obstacles(nodes: &[PlacedNode], groups: &[PlacedGroup]) -> Vec<Rect> {
    let mut rects = Vec::new();
    for node in nodes {
        rects.push(node.tile);
        if let Some(label) = node.label_rect {
            rects.push(label);
        }
    }
    for group in groups {
        rects.push(group.label_rect);
        rects.push(Rect {
            x: group.rect.x + 8.0,
            y: group.rect.y + 8.0,
            w: 22.0,
            h: 22.0,
        });
    }
    rects
}

fn anchor(
    diagram: &Diagram,
    nodes: &[PlacedNode],
    groups: &[PlacedGroup],
    node: usize,
    side: Side,
    on_group: bool,
    spread: (usize, usize),
) -> (f32, f32) {
    if on_group && let Some(parent) = diagram.nodes[node].parent {
        return group_anchor(groups[parent].rect, nodes[node].tile, side);
    }
    port_on(&nodes[node], side, spread.0, spread.1)
}

fn group_anchor(group: Rect, service: Rect, side: Side) -> (f32, f32) {
    let y = service.cy().clamp(group.y + 28.0, group.bottom() - 10.0);
    let x = service.cx().clamp(group.x + 28.0, group.right() - 10.0);
    match side {
        Side::Left => (group.x, y),
        Side::Right => (group.right(), y),
        Side::Top => (x, group.y),
        Side::Bottom => (x, group.bottom()),
    }
}

fn port_on(node: &PlacedNode, side: Side, slot: usize, slots: usize) -> (f32, f32) {
    let share = if slots <= 1 {
        0.5
    } else {
        0.32 + 0.36 * (slot as f32) / (slots as f32 - 1.0)
    };
    let tile = node.tile;
    match side {
        Side::Top => (tile.x + tile.w * share, tile.y),
        Side::Left => (tile.x, tile.y + tile.h * share),
        Side::Right => (tile.right(), tile.y + tile.h * share),
        Side::Bottom => {
            let bottom = node
                .label_rect
                .map(|label| label.bottom())
                .unwrap_or_else(|| tile.bottom());
            (tile.x + tile.w * share, bottom)
        }
    }
}

fn route(
    start: (f32, f32),
    start_side: Side,
    end: (f32, f32),
    end_side: Side,
    obstacles: &[Rect],
) -> Vec<(f32, f32)> {
    let direct = elbow(start, end, start_side, end_side);
    if !hits(&direct, obstacles) {
        return direct;
    }
    let mut best: Option<(f32, Vec<(f32, f32)>)> = None;
    let mut keep = |points: Vec<(f32, f32)>| {
        if hits(&points, obstacles) {
            return;
        }
        let len = path_len(&points);
        if best.as_ref().is_none_or(|(current, _)| len < *current) {
            best = Some((len, points));
        }
    };
    if !obstacles.is_empty() {
        let mut left = f32::MAX;
        let mut right = f32::MIN;
        let mut top = f32::MAX;
        let mut bottom = f32::MIN;
        for rect in obstacles {
            left = left.min(rect.x);
            right = right.max(rect.right());
            top = top.min(rect.y);
            bottom = bottom.max(rect.bottom());
        }
        keep(lane(start, start_side, end, end_side, top - 18.0, true));
        keep(lane(start, start_side, end, end_side, bottom + 18.0, true));
        keep(lane(start, start_side, end, end_side, left - 18.0, false));
        keep(lane(start, start_side, end, end_side, right + 18.0, false));
    }
    // Every obstacle, not only the one the straight elbow hits: the free
    // gap is often just past a label the elbow never touched.
    for rect in obstacles {
        keep(lane(start, start_side, end, end_side, rect.y - 14.0, true));
        keep(lane(
            start,
            start_side,
            end,
            end_side,
            rect.bottom() + 14.0,
            true,
        ));
        keep(lane(start, start_side, end, end_side, rect.x - 14.0, false));
        keep(lane(
            start,
            start_side,
            end,
            end_side,
            rect.right() + 14.0,
            false,
        ));
    }
    best.map(|(_, points)| points).unwrap_or(direct)
}

fn path_len(points: &[(f32, f32)]) -> f32 {
    points
        .windows(2)
        .map(|pair| {
            let dx = pair[1].0 - pair[0].0;
            let dy = pair[1].1 - pair[0].1;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

fn lane(
    start: (f32, f32),
    start_side: Side,
    end: (f32, f32),
    end_side: Side,
    at: f32,
    horizontal: bool,
) -> Vec<(f32, f32)> {
    let start_out = depart(start, start_side, 14.0);
    let end_out = depart(end, end_side, 14.0);
    let bend = if horizontal {
        vec![(start_out.0, at), (end_out.0, at)]
    } else {
        vec![(at, start_out.1), (at, end_out.1)]
    };
    let mut points = vec![start, start_out];
    points.extend(bend);
    points.push(end_out);
    points.push(end);
    dedup(points)
}

fn elbow(start: (f32, f32), end: (f32, f32), start_side: Side, end_side: Side) -> Vec<(f32, f32)> {
    let mut points = vec![start];
    match (side_horizontal(start_side), side_horizontal(end_side)) {
        (true, true) if (start.1 - end.1).abs() > 0.5 => {
            let mid = (start.0 + end.0) / 2.0;
            points.push((mid, start.1));
            points.push((mid, end.1));
        }
        (false, false) if (start.0 - end.0).abs() > 0.5 => {
            let mid = (start.1 + end.1) / 2.0;
            points.push((start.0, mid));
            points.push((end.0, mid));
        }
        (true, false) => points.push((end.0, start.1)),
        (false, true) => points.push((start.0, end.1)),
        _ => {}
    }
    points.push(end);
    dedup(points)
}

fn depart(point: (f32, f32), side: Side, dist: f32) -> (f32, f32) {
    match side {
        Side::Left => (point.0 - dist, point.1),
        Side::Right => (point.0 + dist, point.1),
        Side::Top => (point.0, point.1 - dist),
        Side::Bottom => (point.0, point.1 + dist),
    }
}

fn hits(points: &[(f32, f32)], obstacles: &[Rect]) -> bool {
    points.windows(2).any(|pair| {
        obstacles
            .iter()
            .any(|rect| segment_hits(pair[0], pair[1], *rect))
    })
}

fn segment_hits(start: (f32, f32), end: (f32, f32), rect: Rect) -> bool {
    let left = rect.x + 1.5;
    let right = rect.right() - 1.5;
    let top = rect.y + 1.5;
    let bottom = rect.bottom() - 1.5;
    if right <= left || bottom <= top {
        return false;
    }
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let mut t0 = 0.0_f32;
    let mut t1 = 1.0_f32;
    let p = [-dx, dx, -dy, dy];
    let q = [
        start.0 - left,
        right - start.0,
        start.1 - top,
        bottom - start.1,
    ];
    for index in 0..4 {
        if p[index].abs() < 1.0e-4 {
            if q[index] < 0.0 {
                return false;
            }
            continue;
        }
        let t = q[index] / p[index];
        if p[index] < 0.0 {
            if t > t1 {
                return false;
            }
            if t > t0 {
                t0 = t;
            }
        } else if t < t0 {
            return false;
        } else if t < t1 {
            t1 = t;
        }
    }
    true
}

fn dedup(points: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for point in points {
        if out.last().is_none_or(|prev: &(f32, f32)| {
            (prev.0 - point.0).abs() > 0.4 || (prev.1 - point.1).abs() > 0.4
        }) {
            out.push(point);
        }
    }
    out
}

fn measured(text: &str, font: Font, family: Family, line: usize) -> Result<Block, Declined> {
    text::block(text, font, family).map_err(|reason| Declined::at(line, reason))
}

fn wrap(text: &str, limit: usize) -> String {
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

fn page_ink(look: &Look) -> Color {
    let page = look.background.unwrap_or(Color::WHITE);
    if page.is_dark() != look.preferred.is_dark() {
        look.preferred
    } else if page.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

fn tile_color(look: &Look) -> Color {
    if look.classic {
        Color::hex(CLASSIC_TILE)
    } else {
        look.accent
    }
}

fn glyph_ink(tile: Color) -> Color {
    if tile.is_dark() {
        Color::WHITE
    } else {
        Color::hex(0x1A1A1A)
    }
}

fn paint(canvas: &mut Canvas, diagram: &Diagram, laid: &Laid) {
    let look = &diagram.look;
    let tile = tile_color(look);
    let glyph = glyph_ink(tile);
    for group in &laid.groups {
        canvas.rect(
            Layer::Frames,
            (group.rect.x, group.rect.y, group.rect.w, group.rect.h),
            10.0,
            None,
            Some(Stroke::new(look.line, 1.25).dashed(7.0, 5.0)),
        );
        let icon = Rect {
            x: group.rect.x + 8.0,
            y: group.rect.y + 8.0,
            w: 22.0,
            h: 22.0,
        };
        paint_tile(canvas, icon, tile, group.icon, glyph);
        canvas.text(
            Layer::Front,
            &group.label,
            (
                icon.right() + 6.0,
                icon.y + (icon.h - group.label.height) / 2.0,
            ),
            Align::Left,
        );
    }
    for edge in &laid.edges {
        let mut points = edge.points.clone();
        if edge.arrow_end {
            shorten(&mut points, 8.0, true);
        }
        if edge.arrow_start {
            shorten(&mut points, 8.0, false);
        }
        if points.len() >= 2 {
            canvas.polyline(Layer::Bars, &points, Stroke::new(look.line, 1.75));
        }
        if edge.arrow_end
            && let (Some(tip), Some(from)) = (edge.points.last(), edge.points.iter().rev().nth(1))
        {
            arrow(canvas, *tip, *from, look.line);
        }
        if edge.arrow_start
            && let (Some(tip), Some(from)) = (edge.points.first(), edge.points.get(1))
        {
            arrow(canvas, *tip, *from, look.line);
        }
    }
    for (index, node) in laid.nodes.iter().enumerate() {
        if diagram.nodes[index].junction {
            canvas.ellipse(
                Layer::Bars,
                (node.tile.cx(), node.tile.cy()),
                (node.tile.w / 2.0, node.tile.h / 2.0),
                Some(look.line),
                None,
            );
            continue;
        }
        paint_tile(canvas, node.tile, tile, diagram.nodes[index].icon, glyph);
        if let Some(label) = &node.label {
            canvas.text(
                Layer::Front,
                label,
                (node.tile.cx(), node.tile.bottom() + 4.0),
                Align::Center,
            );
        }
    }
    if let Some(title) = &laid.title {
        let right = laid
            .nodes
            .iter()
            .map(|node| node.tile.right())
            .chain(laid.groups.iter().map(|group| group.rect.right()))
            .fold(title.width, f32::max);
        let left = laid
            .nodes
            .iter()
            .map(|node| node.tile.x)
            .chain(laid.groups.iter().map(|group| group.rect.x))
            .fold(0.0_f32, f32::min);
        canvas.text(
            Layer::Front,
            title,
            ((left + right) / 2.0, 0.0),
            Align::Center,
        );
    }
}

fn paint_tile(canvas: &mut Canvas, tile: Rect, fill: Color, icon: Icon, glyph: Color) {
    canvas.rect(
        Layer::Bars,
        (tile.x, tile.y, tile.w, tile.h),
        8.0,
        Some(fill),
        None,
    );
    match icon {
        Icon::Cloud => paint_cloud(canvas, tile, glyph),
        Icon::Database => paint_database(canvas, tile, glyph),
        Icon::Disk => paint_disk(canvas, tile, glyph),
        Icon::Internet => paint_internet(canvas, tile, glyph),
        Icon::Server => paint_server(canvas, tile, glyph),
        Icon::Plain => paint_plain(canvas, tile, glyph),
    }
}

fn paint_cloud(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let cx = tile.cx();
    let cy = tile.cy();
    let xy = |x: f32, y: f32| format!("{},{}", n(cx + x * scale), n(cy + y * scale));
    // A flat base and three humps. Filled circles at this size merge into a blob.
    let path = format!(
        "M{} C{} {} {} C{} {} {} C{} {} {} C{} {} {} Z",
        xy(-14.0, 6.0),
        xy(-22.0, 6.0),
        xy(-22.0, -3.0),
        xy(-12.0, -4.0),
        xy(-14.0, -14.0),
        xy(-2.0, -14.0),
        xy(-1.0, -5.0),
        xy(1.0, -16.0),
        xy(14.0, -14.0),
        xy(13.0, -4.0),
        xy(22.0, -6.0),
        xy(22.0, 6.0),
        xy(12.0, 6.0),
    );
    canvas.path(
        Layer::Front,
        &path,
        (
            cx - 22.0 * scale,
            cy - 16.0 * scale,
            cx + 22.0 * scale,
            cy + 8.0 * scale,
        ),
        Some(glyph),
        None,
    );
}

fn paint_database(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let rx = 14.0 * scale;
    let ry = 5.5 * scale;
    let cx = tile.cx();
    let top = tile.cy() - 12.0 * scale;
    let bot = tile.cy() + 12.0 * scale;
    let x = cx - rx;
    let stroke = Stroke::new(glyph, 1.6);
    let body = format!(
        "M{},{} A{},{} 0 0 1 {},{} L{},{} A{},{} 0 0 1 {},{} Z",
        n(x),
        n(top),
        n(rx),
        n(ry),
        n(x + rx * 2.0),
        n(top),
        n(x + rx * 2.0),
        n(bot),
        n(rx),
        n(ry),
        n(x),
        n(bot)
    );
    canvas.path(
        Layer::Front,
        &body,
        (x, top - ry, x + rx * 2.0, bot + ry),
        None,
        Some(stroke),
    );
    let lip = format!(
        "M{},{} A{},{} 0 0 0 {},{}",
        n(x),
        n(top),
        n(rx),
        n(ry),
        n(x + rx * 2.0),
        n(top)
    );
    canvas.path(
        Layer::Front,
        &lip,
        (x, top - ry, x + rx * 2.0, top + ry),
        None,
        Some(stroke),
    );
}

fn paint_disk(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let stroke = Stroke::new(glyph, 1.6);
    canvas.ellipse(
        Layer::Front,
        (tile.cx(), tile.cy()),
        (14.0 * scale, 14.0 * scale),
        None,
        Some(stroke),
    );
    canvas.ellipse(
        Layer::Front,
        (tile.cx(), tile.cy()),
        (4.0 * scale, 4.0 * scale),
        Some(glyph),
        None,
    );
}

fn paint_internet(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let stroke = Stroke::new(glyph, 1.6);
    let radius = 13.0 * scale;
    canvas.ellipse(
        Layer::Front,
        (tile.cx(), tile.cy()),
        (radius, radius),
        None,
        Some(stroke),
    );
    canvas.ellipse(
        Layer::Front,
        (tile.cx(), tile.cy()),
        (radius * 0.45, radius),
        None,
        Some(stroke),
    );
    canvas.line(
        Layer::Front,
        (tile.cx() - radius, tile.cy()),
        (tile.cx() + radius, tile.cy()),
        stroke,
    );
}

fn paint_server(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let width = 28.0 * scale;
    let height = 7.0 * scale;
    let x = tile.cx() - width / 2.0;
    let y = tile.cy() - height * 2.1;
    let stroke = Stroke::new(glyph, 1.4);
    for row in 0..3 {
        let top = y + row as f32 * (height + 3.0 * scale);
        canvas.rect(
            Layer::Front,
            (x, top, width, height),
            2.0,
            None,
            Some(stroke),
        );
    }
}

fn paint_plain(canvas: &mut Canvas, tile: Rect, glyph: Color) {
    let scale = tile.w / TILE;
    let side = 18.0 * scale;
    let x = tile.cx() - side / 2.0;
    let y = tile.cy() - side / 2.0;
    canvas.rect(
        Layer::Front,
        (x, y, side, side),
        3.0,
        None,
        Some(Stroke::new(glyph, 1.6)),
    );
}

fn arrow(canvas: &mut Canvas, tip: (f32, f32), from: (f32, f32), color: Color) {
    let dx = tip.0 - from.0;
    let dy = tip.1 - from.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.1 {
        return;
    }
    let ux = dx / len;
    let uy = dy / len;
    let px = -uy;
    let py = ux;
    let base = (tip.0 - ux * 9.0, tip.1 - uy * 9.0);
    canvas.polygon(
        Layer::Front,
        &[
            tip,
            (base.0 + px * 4.2, base.1 + py * 4.2),
            (base.0 - px * 4.2, base.1 - py * 4.2),
        ],
        Some(color),
        None,
    );
}

fn shorten(points: &mut [(f32, f32)], len: f32, at_end: bool) {
    if points.len() < 2 {
        return;
    }
    if at_end {
        let last = points.len() - 1;
        let toward = points[last - 1];
        pull(&mut points[last], toward, len);
    } else {
        let toward = points[1];
        pull(&mut points[0], toward, len);
    }
}

fn pull(point: &mut (f32, f32), toward: (f32, f32), len: f32) {
    let dx = toward.0 - point.0;
    let dy = toward.1 - point.1;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 0.1 {
        return;
    }
    let step = len.min(dist);
    point.0 += dx / dist * step;
    point.1 += dy / dist * step;
}

#[cfg(test)]
mod tests {
    use super::{Icon, Side, draw, from_mermaid, lay, mermaid_refusal};
    use crate::text::Family;

    fn diagram(source: &str) -> super::Diagram {
        from_mermaid(source).unwrap_or_else(|| panic!("refused {source}"))
    }

    #[test]
    fn ports_put_the_server_left_of_the_database_and_the_disks_below() {
        let source = "\
architecture-beta
    group api(cloud)[API]
    service db(database)[Database] in api
    service disk1(disk)[Storage] in api
    service disk2(disk)[Storage] in api
    service server(server)[Server] in api
    db:L -- R:server
    disk1:T -- B:server
    disk2:T -- B:db
";
        let diagram = diagram(source);
        assert_eq!(diagram.nodes.len(), 4);
        assert_eq!(diagram.groups.len(), 1);
        assert_eq!(diagram.nodes[0].icon, Icon::Database);
        let laid = lay(&diagram, Family::Sans).unwrap();
        let db = &laid.nodes[0].tile;
        let disk1 = &laid.nodes[1].tile;
        let disk2 = &laid.nodes[2].tile;
        let server = &laid.nodes[3].tile;
        assert!(server.right() <= db.x + 1.0, "server left of db");
        assert!(disk1.y >= server.bottom() - 1.0, "disk1 below server");
        assert!(disk2.y >= db.bottom() - 1.0, "disk2 below db");
        let group = laid.groups[0].rect;
        for tile in [db, disk1, disk2, server] {
            assert!(tile.x > group.x && tile.right() < group.right());
            assert!(tile.y > group.y && tile.bottom() < group.bottom());
        }
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("Database")
                && svg.contains("Server")
                && svg.contains("#087EBF")
                && svg.contains("stroke-dasharray")
                && svg.contains("<path"),
            "{svg}"
        );
        assert!(clear(&laid), "a label or a line crosses a box");
    }

    #[test]
    fn a_junction_is_a_dot_and_a_group_edge_leaves_the_frame() {
        let source = "\
architecture-beta
    group one(cloud)[One]
    group two(cloud)[Two]
    service server(server)[Server] in one
    service subnet(database)[Subnet] in two
    junction hub
    server{group}:B --> T:subnet{group}
    server:R -- L:hub
";
        let diagram = diagram(source);
        assert!(diagram.nodes[2].junction);
        assert!(diagram.edges[0].from_group && diagram.edges[0].to_group);
        assert!(diagram.edges[0].arrow_end && !diagram.edges[0].arrow_start);
        let laid = lay(&diagram, Family::Sans).unwrap();
        let svg = draw(&diagram).unwrap();
        assert!(!svg.contains(">hub<"), "{svg}");
        assert!(svg.contains("<polygon"), "{svg}");
        let group_edge = &laid.edges[0];
        let start = group_edge.points[0];
        let one = laid.groups[0].rect;
        assert!(
            (start.1 - one.bottom()).abs() < 1.0
                || (start.0 - one.x).abs() < 1.0
                || (start.0 - one.right()).abs() < 1.0
        );
        assert!(clear(&laid), "a label or a line crosses a box");
    }

    #[test]
    fn a_nested_group_stays_inside_its_parent_and_a_dark_page_uses_the_accent() {
        let source = "\
architecture-beta
    title Platform
    group outer(cloud)[Outer]
    group inner(server)[Inner] in outer
    service db(database)[DB] in inner
";
        let diagram = diagram(source);
        assert_eq!(diagram.groups[1].parent, Some(0));
        assert_eq!(
            diagram.title.as_ref().map(|title| title.value.as_str()),
            Some("Platform")
        );
        let laid = lay(&diagram, Family::Sans).unwrap();
        let inner = laid.groups[1].rect;
        let outer = laid.groups[0].rect;
        assert!(inner.x > outer.x && inner.right() < outer.right());
        assert!(inner.y > outer.y && inner.bottom() < outer.bottom());
        let mut themed = diagram.clone();
        themed.look.classic = false;
        themed.look.accent = super::Color::hex(0xE68E0D);
        themed.look.background = Some(super::Color::hex(0x121212));
        themed.look.preferred = super::Color::hex(0xBEBEBE);
        themed.look.line = super::Color::hex(0xE68E0D);
        let svg = draw(&themed).unwrap();
        assert!(
            svg.contains("Platform")
                && svg.contains("#E68E0D")
                && svg.contains("#BEBEBE")
                && !svg.contains("#087EBF"),
            "{svg}"
        );
    }

    #[test]
    fn a_nested_group_stays_off_its_sibling() {
        let source = "\
architecture-beta
    title Platform
    group api(cloud)[API]
    group data(disk)[Data] in api
    service db(database)[Database] in data
    service disk1(disk)[Storage] in api
    service disk2(disk)[Archive] in api
    service server(server)[Server] in api
    service web(internet)[Web]
    junction hub
    db:L -- R:server
    disk1:T -- B:server
    disk2:T -- B:db
    server:R --> L:hub
    web:B --> T:server{group}
";
        let laid = lay(&diagram(source), Family::Sans).unwrap();
        let api = laid.groups[0].rect;
        let data = laid.groups[1].rect;
        let server = laid.nodes[3].tile;
        let web = laid.nodes[4].tile;
        assert!(data.x > api.x && data.right() < api.right());
        assert!(data.y > api.y && data.bottom() < api.bottom());
        assert!(
            !data.intersects(server),
            "the inner frame covers the server"
        );
        assert!(
            !api.intersects(web),
            "the outer frame covers the web service"
        );
        let end = *laid.edges[4].points.last().unwrap();
        assert!(
            (end.1 - api.y).abs() < 1.5,
            "the group edge meets the frame"
        );
        assert!(clear(&laid), "a label or a line crosses a box");
    }

    #[test]
    fn an_icon_pack_a_port_and_a_stray_line_are_named() {
        let icon =
            mermaid_refusal("architecture-beta\n  service db(logos:aws-aurora)[DB]\n").unwrap();
        assert_eq!(icon, (1, "an icon"));
        let port = mermaid_refusal(
            "architecture-beta\n  service a(server)[A]\n  service b(server)[B]\n  a:L -- L:b\n",
        )
        .unwrap();
        assert_eq!(port.1, "a port");
        let missing = mermaid_refusal(
            "architecture-beta\n  service a(server)[A]\n  service b(server)[B]\n  a --> b\n",
        )
        .unwrap();
        assert_eq!(missing.1, "a port");
        let unknown = mermaid_refusal("architecture-beta\n  direction LR\n").unwrap();
        assert_eq!(unknown.1, "an unknown line");
        let group =
            mermaid_refusal("architecture-beta\n  service a(server)[A] in missing\n").unwrap();
        assert_eq!(group.1, "a group");
        assert!(from_mermaid("sequenceDiagram\nA->B: hi\n").is_none());
        assert!(mermaid_refusal("sequenceDiagram\nA->B: hi\n").is_none());
        let plain = diagram("architecture-beta\n  service a[Api]\n");
        assert_eq!(plain.nodes[0].icon, Icon::Plain);
        assert_eq!(plain.nodes[0].label, "Api");
        let _ = Side::Left;
    }

    fn clear(laid: &super::Laid) -> bool {
        let mut boxes = Vec::new();
        for node in &laid.nodes {
            boxes.push(node.tile);
            if let Some(label) = node.label_rect {
                boxes.push(label);
            }
        }
        for group in &laid.groups {
            boxes.push(group.label_rect);
            boxes.push(super::Rect {
                x: group.rect.x + 8.0,
                y: group.rect.y + 8.0,
                w: 22.0,
                h: 22.0,
            });
        }
        for (index, rect) in boxes.iter().enumerate() {
            for other in boxes.iter().skip(index + 1) {
                if rect.intersects(*other) {
                    return false;
                }
            }
        }
        for edge in &laid.edges {
            for pair in edge.points.windows(2) {
                if boxes
                    .iter()
                    .any(|rect| super::segment_hits(pair[0], pair[1], *rect))
                {
                    return false;
                }
            }
        }
        true
    }
}
