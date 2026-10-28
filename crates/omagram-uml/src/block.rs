//! Mermaid block diagrams (`block-beta`).
//!
//! Blocks sit on a column grid. A nested `block:` keeps its children inside
//! the frame, and the title stands in a band above them. An edge leaves the
//! border and takes the shortest orthogonal lane that misses every other
//! block and every title. A block arrow is a chevron.
//! On the stock theme a block is the familiar lilac tile. On any other
//! theme the tile is a mix of the accent onto the page, the stroke is the
//! accent, and the label follows the page when that still reads.

use std::collections::HashMap;

use crate::Declined;
use crate::color::Color;
use crate::common::{Text, keyword, text_at};
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{self, Block, Family, Font};

const CLASSIC_FILL: u32 = 0xECECFF;
const CLASSIC_BORDER: u32 = 0x7B88A8;
const NEST_LIMIT: usize = 100;

/// Which way a block arrow points. `x` and `y` have a head on both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Right,
    Left,
    Up,
    Down,
    BothAcross,
    BothDown,
}

/// The outline of one block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Rectangle,
    Round,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    DoubleCircle,
    Diamond,
    Hexagon,
    Parallelogram,
    ParallelogramAlt,
    Trapezoid,
    TrapezoidAlt,
    Asymmetric,
    Arrow(Direction),
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
            accent: Color::hex(CLASSIC_BORDER),
            line: Color::hex(0x2F3B4D),
            classic: true,
        }
    }
}

/// One end of an edge: a block or a nested group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Node(usize),
    Group(usize),
}

/// One block diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    pub columns: Option<usize>,
    pub items: Vec<Item>,
    pub groups: Vec<Group>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub look: Look,
}

/// A column slot in a frame, in reading order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Node(usize),
    Group(usize),
    Space(usize),
}

/// A nested `block:` frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub id: String,
    pub label: String,
    pub span: usize,
    pub columns: Option<usize>,
    pub items: Vec<Item>,
    pub line: usize,
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub text: Option<Color>,
    pub stroke_width: Option<f32>,
}

/// One block.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: Shape,
    pub span: usize,
    pub line: usize,
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub text: Option<Color>,
    pub stroke_width: Option<f32>,
}

/// A line between two blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: End,
    pub to: End,
    pub label: Option<Text>,
    pub arrow_start: bool,
    pub arrow_end: bool,
    pub dotted: bool,
    pub thick: bool,
    pub line: usize,
}

/// The diagram `source` describes, or `None` when it is not one or it uses
/// a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a block diagram, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

/// The SVG. Nested blocks contain their children. An edge does not cross
/// another block.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    if !(4.0..=100.0).contains(&diagram.look.font_size) {
        return Err(Declined::at(0, "the font size is not one Omagram draws"));
    }
    let mut canvas = Canvas::new(&diagram.look.font_name, false);
    let family = canvas.family();
    let laid = layout(diagram, family)?;
    paint(&mut canvas, diagram, &laid, family)?;
    Ok(canvas.finish(18.0, diagram.look.background))
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

#[derive(Clone, Copy)]
struct Metrics {
    gap: f32,
    pad: f32,
    min_w: f32,
    min_h: f32,
    font: f32,
}

fn metrics(font: f32) -> Metrics {
    Metrics {
        gap: font * 2.2 + 16.0,
        pad: (font * 0.9).max(12.0),
        min_w: font * 4.4,
        min_h: font * 2.5,
        font,
    }
}

fn read(source: &str) -> Result<Diagram, Stop> {
    let mut diagram = Diagram {
        title: None,
        columns: None,
        items: Vec::new(),
        groups: Vec::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
        look: Look::default(),
    };
    let mut ids: HashMap<String, End> = HashMap::new();
    let mut stack: Vec<usize> = Vec::new();
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
        if !saw_header {
            match header_rest(line) {
                None => return Err(Stop::Foreign),
                Some(rest) if !rest.is_empty() => return Err(fail(line_no, "an unknown line")),
                Some(_) => saw_header = true,
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
            let title = plain(&unquote(rest));
            if !readable(&title) {
                return Err(fail(line_no, "a title"));
            }
            diagram.title = Some(text_at(&title, line_no));
            continue;
        }
        if header_rest(line).is_some() {
            return Err(fail(line_no, "an unknown line"));
        }
        if let Some(rest) = keyword(line, "end") {
            if !rest.is_empty() || stack.is_empty() {
                return Err(fail(line_no, "a block"));
            }
            stack.pop();
            continue;
        }
        if let Some(rest) = keyword(line, "columns") {
            let columns = rest
                .parse::<usize>()
                .ok()
                .filter(|n| *n >= 1)
                .ok_or_else(|| fail(line_no, "a column"))?;
            let slot = frame_columns(&mut diagram, stack.last().copied());
            if slot.is_some() {
                return Err(fail(line_no, "a column"));
            }
            *slot = Some(columns);
            continue;
        }
        if keyword(line, "classdef").is_some() || keyword(line, "class").is_some() {
            return Err(fail(line_no, "a class"));
        }
        if keyword(line, "linkstyle").is_some() {
            return Err(fail(line_no, "a style"));
        }
        if let Some(rest) = keyword(line, "style") {
            apply_style(&mut diagram, &ids, rest, line_no)?;
            continue;
        }
        if unsupported(line) {
            return Err(fail(line_no, "an unknown line"));
        }
        if let Some(body) = block_body(line) {
            open_group(&mut diagram, &mut ids, &mut stack, body, line_no)?;
            continue;
        }
        if find_arrow(line).is_some() {
            add_edge(&mut diagram, &ids, line, line_no)?;
            continue;
        }
        add_nodes(&mut diagram, &mut ids, &stack, line, line_no)?;
    }
    if !saw_header {
        return Err(Stop::Foreign);
    }
    if skipping {
        return Err(fail(skip_line, "an unknown line"));
    }
    if let Some(open) = stack.last() {
        return Err(fail(diagram.groups[*open].line, "a block"));
    }
    Ok(diagram)
}

fn frame_columns(diagram: &mut Diagram, group: Option<usize>) -> &mut Option<usize> {
    match group {
        Some(index) => &mut diagram.groups[index].columns,
        None => &mut diagram.columns,
    }
}

fn push_item(diagram: &mut Diagram, stack: &[usize], item: Item) {
    match stack.last() {
        Some(index) => diagram.groups[*index].items.push(item),
        None => diagram.items.push(item),
    }
}

fn header_rest(line: &str) -> Option<&str> {
    keyword(line, "block-beta").or_else(|| keyword(line, "block"))
}

fn block_body(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    if bytes.len() >= 6 && line[..6].eq_ignore_ascii_case("block:") {
        Some(&line[6..])
    } else {
        None
    }
}

fn unsupported(line: &str) -> bool {
    let Some(word) = line.split_whitespace().next() else {
        return false;
    };
    matches!(
        word.to_ascii_lowercase().as_str(),
        "direction" | "click" | "href" | "callback" | "subgraph"
    )
}

fn open_group(
    diagram: &mut Diagram,
    ids: &mut HashMap<String, End>,
    stack: &mut Vec<usize>,
    body: &str,
    line: usize,
) -> Result<(), Stop> {
    if stack.len() >= NEST_LIMIT {
        return Err(fail(line, "a block"));
    }
    let body = body.trim();
    if body.is_empty() {
        return Err(fail(line, "a block"));
    }
    if has_class(body) {
        return Err(fail(line, "a class"));
    }
    let (body, span) = split_span(body).map_err(|()| fail(line, "a block"))?;
    let (id, label) = match shape_suffix(body) {
        Some((id, suffix)) => {
            let (_shape, label) = shape_of(suffix).map_err(|()| fail(line, "a block"))?;
            (id, label)
        }
        None => (body, String::new()),
    };
    if !is_id(id) {
        return Err(fail(line, "a block"));
    }
    let label = plain(&label);
    if !label.is_empty() && !readable(&label) {
        return Err(fail(line, "a block"));
    }
    remember(ids, id, End::Group(diagram.groups.len()), line)?;
    let index = diagram.groups.len();
    diagram.groups.push(Group {
        id: id.to_string(),
        label,
        span,
        columns: None,
        items: Vec::new(),
        line,
        fill: None,
        stroke: None,
        text: None,
        stroke_width: None,
    });
    push_item(diagram, stack, Item::Group(index));
    stack.push(index);
    Ok(())
}

fn add_nodes(
    diagram: &mut Diagram,
    ids: &mut HashMap<String, End>,
    stack: &[usize],
    line: &str,
    line_no: usize,
) -> Result<(), Stop> {
    let tokens = tokens(line);
    if tokens.is_empty() {
        return Err(fail(line_no, "an unknown line"));
    }
    for token in tokens {
        if has_class(token) {
            return Err(fail(line_no, "a class"));
        }
        let (token, span) = split_span(token).map_err(|()| fail(line_no, "an element"))?;
        if token.eq_ignore_ascii_case("space") {
            push_item(diagram, stack, Item::Space(span));
            continue;
        }
        let (id, shape, label) = node_of(token).map_err(|()| fail(line_no, "an unknown line"))?;
        if !is_id(id) {
            return Err(fail(line_no, "an unknown line"));
        }
        let label = {
            let written = plain(&label);
            if written.trim().is_empty() {
                id.to_string()
            } else {
                written
            }
        };
        if !readable(&label) {
            return Err(fail(line_no, "an unknown line"));
        }
        remember(ids, id, End::Node(diagram.nodes.len()), line_no)?;
        let index = diagram.nodes.len();
        diagram.nodes.push(Node {
            id: id.to_string(),
            label,
            shape,
            span,
            line: line_no,
            fill: None,
            stroke: None,
            text: None,
            stroke_width: None,
        });
        push_item(diagram, stack, Item::Node(index));
    }
    Ok(())
}

fn add_edge(
    diagram: &mut Diagram,
    ids: &HashMap<String, End>,
    line: &str,
    line_no: usize,
) -> Result<(), Stop> {
    let Some((at, arrow)) = find_arrow(line) else {
        return Err(fail(line_no, "an edge"));
    };
    let mut left = line[..at].trim();
    let mut right = line[at + arrow.len()..].trim();
    let mut label = None;
    if let Some(rest) = right.strip_prefix('|') {
        let Some(end) = rest.find('|') else {
            return Err(fail(line_no, "an edge"));
        };
        label = Some(rest[..end].trim());
        right = rest[end + 1..].trim();
    }
    if let Some(peeled) = peel_label(left, arrow) {
        if label.is_some() {
            return Err(fail(line_no, "an edge"));
        }
        left = peeled.0;
        label = Some(peeled.1);
    }
    if find_arrow(right).is_some() || find_arrow(left).is_some() {
        return Err(fail(line_no, "an edge"));
    }
    let sources = endpoints(left, ids, line_no)?;
    let targets = endpoints(right, ids, line_no)?;
    if sources.is_empty() || targets.is_empty() {
        return Err(fail(line_no, "an edge"));
    }
    let (arrow_start, arrow_end, dotted, thick) = arrow_kind(arrow);
    let written = label
        .map(|text| plain(&unquote(text)))
        .filter(|text| !text.trim().is_empty());
    if let Some(text) = &written
        && !readable(text)
    {
        return Err(fail(line_no, "an edge"));
    }
    let label = written.map(|text| text_at(&text, line_no));
    for from in &sources {
        for to in &targets {
            if from == to || contains_end(diagram, *from, *to) {
                return Err(fail(line_no, "an edge"));
            }
            diagram.edges.push(Edge {
                from: *from,
                to: *to,
                label: label.clone(),
                arrow_start,
                arrow_end,
                dotted,
                thick,
                line: line_no,
            });
        }
    }
    Ok(())
}

fn endpoints(side: &str, ids: &HashMap<String, End>, line: usize) -> Result<Vec<End>, Stop> {
    let mut ends = Vec::new();
    for part in split_amp(side) {
        let part = part.trim();
        if part.is_empty() {
            return Err(fail(line, "an edge"));
        }
        if has_class(part) {
            return Err(fail(line, "a class"));
        }
        let (id, _shape, _label) = node_of(part).map_err(|()| fail(line, "an edge"))?;
        let Some(end) = ids.get(id) else {
            return Err(fail(line, "an edge"));
        };
        ends.push(*end);
    }
    Ok(ends)
}

fn arrow_kind(arrow: &str) -> (bool, bool, bool, bool) {
    match arrow {
        "<-->" => (true, true, false, false),
        "-->" | "->" => (false, true, false, false),
        "<--" | "<-" => (true, false, false, false),
        "---" => (false, false, false, false),
        "==>" => (false, true, false, true),
        "<==" => (true, false, false, true),
        "===" => (false, false, false, true),
        "-.->" => (false, true, true, false),
        "<-.-" => (true, false, true, false),
        "-.-" => (false, false, true, false),
        "<-.->" => (true, true, true, false),
        _ => (false, true, false, false),
    }
}

fn peel_label<'a>(left: &'a str, arrow: &str) -> Option<(&'a str, &'a str)> {
    let opener = match arrow {
        "-->" | "->" | "<--" | "<-" | "<-->" => "--",
        "-.->" | "<-.-" | "<-.->" => "-.",
        "==>" | "<==" => "==",
        _ => return None,
    };
    let at = find_opener(left, opener)?;
    let id = left[..at].trim();
    let text = left[at + opener.len()..].trim();
    if id.is_empty() || text.is_empty() || find_arrow(id).is_some() {
        None
    } else {
        Some((id, text))
    }
}

fn apply_style(
    diagram: &mut Diagram,
    ids: &HashMap<String, End>,
    rest: &str,
    line: usize,
) -> Result<(), Stop> {
    let mut parts = rest.split_whitespace();
    let Some(id) = parts.next() else {
        return Err(fail(line, "a style"));
    };
    if !is_id(id) {
        return Err(fail(line, "a style"));
    }
    let spec = rest[id.len()..].trim();
    if spec.is_empty() {
        return Err(fail(line, "a style"));
    }
    let Some(end) = ids.get(id).copied() else {
        return Err(fail(line, "a style"));
    };
    let mut fill = None;
    let mut stroke = None;
    let mut text = None;
    let mut width = None;
    for piece in spec.split(',') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        let Some((key, value)) = piece.split_once(':') else {
            return Err(fail(line, "a style"));
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "fill" => fill = Some(paint_color(value, line)?),
            "stroke" => stroke = Some(paint_color(value, line)?),
            "color" => text = Some(paint_color(value, line)?),
            "stroke-width" => width = Some(paint_width(value, line)?),
            _ => return Err(fail(line, "a style")),
        }
    }
    if fill.is_none() && stroke.is_none() && text.is_none() && width.is_none() {
        return Err(fail(line, "a style"));
    }
    match end {
        End::Node(index) => {
            let node = &mut diagram.nodes[index];
            if let Some(color) = fill {
                node.fill = Some(color);
            }
            if let Some(color) = stroke {
                node.stroke = Some(color);
            }
            if let Some(color) = text {
                node.text = Some(color);
            }
            if let Some(value) = width {
                node.stroke_width = Some(value);
            }
        }
        End::Group(index) => {
            let group = &mut diagram.groups[index];
            if let Some(color) = fill {
                group.fill = Some(color);
            }
            if let Some(color) = stroke {
                group.stroke = Some(color);
            }
            if let Some(color) = text {
                group.text = Some(color);
            }
            if let Some(value) = width {
                group.stroke_width = Some(value);
            }
        }
    }
    Ok(())
}

fn paint_color(value: &str, line: usize) -> Result<Color, Stop> {
    match crate::color::parse(value) {
        Ok(Some(color)) => Ok(color),
        _ => Err(fail(line, "a style")),
    }
}

fn paint_width(value: &str, line: usize) -> Result<f32, Stop> {
    let value = value.trim().trim_end_matches("px").trim();
    let width = value.parse::<f32>().map_err(|_| fail(line, "a style"))?;
    if (0.25..=8.0).contains(&width) {
        Ok(width)
    } else {
        Err(fail(line, "a style"))
    }
}

fn remember(ids: &mut HashMap<String, End>, id: &str, end: End, line: usize) -> Result<(), Stop> {
    if ids.contains_key(id) {
        return Err(fail(line, "an element"));
    }
    ids.insert(id.to_string(), end);
    Ok(())
}

fn node_of(token: &str) -> Result<(&str, Shape, String), ()> {
    match shape_suffix(token) {
        Some((id, suffix)) => {
            let (shape, label) = shape_of(suffix)?;
            Ok((id, shape, label))
        }
        None => Ok((token, Shape::Rectangle, String::new())),
    }
}

fn shape_suffix(token: &str) -> Option<(&str, &str)> {
    let mut byte = 0;
    for ch in token.chars() {
        let next = byte + ch.len_utf8();
        if matches!(ch, '[' | '(' | '{' | '>') || (ch == '<' && token[next..].starts_with('[')) {
            if byte == 0 {
                return None;
            }
            return Some((&token[..byte], &token[byte..]));
        }
        byte = next;
    }
    None
}

fn shape_of(suffix: &str) -> Result<(Shape, String), ()> {
    if suffix.starts_with("<[") {
        return arrow_of(suffix);
    }
    let (shape, sides) = if suffix.starts_with("(((") && suffix.ends_with(")))") {
        (Shape::DoubleCircle, 3)
    } else if suffix.starts_with("((") && suffix.ends_with("))") {
        (Shape::Circle, 2)
    } else if suffix.starts_with("([") && suffix.ends_with("])") {
        (Shape::Stadium, 2)
    } else if suffix.starts_with('(') && suffix.ends_with(')') {
        (Shape::Round, 1)
    } else if suffix.starts_with("[[") && suffix.ends_with("]]") {
        (Shape::Subroutine, 2)
    } else if suffix.starts_with("[(") && suffix.ends_with(")]") {
        (Shape::Cylinder, 2)
    } else if suffix.starts_with("[/") && suffix.ends_with("/]") {
        (Shape::Parallelogram, 2)
    } else if suffix.starts_with("[\\") && suffix.ends_with("\\]") {
        (Shape::ParallelogramAlt, 2)
    } else if suffix.starts_with("[/") && suffix.ends_with("\\]") {
        (Shape::Trapezoid, 2)
    } else if suffix.starts_with("[\\") && suffix.ends_with("/]") {
        (Shape::TrapezoidAlt, 2)
    } else if suffix.starts_with('[') && suffix.ends_with(']') {
        (Shape::Rectangle, 1)
    } else if suffix.starts_with("{{") && suffix.ends_with("}}") {
        (Shape::Hexagon, 2)
    } else if suffix.starts_with('{') && suffix.ends_with('}') {
        (Shape::Diamond, 1)
    } else if suffix.starts_with('>') && suffix.ends_with(']') && suffix.len() > 2 {
        (Shape::Asymmetric, 1)
    } else {
        return Err(());
    };
    Ok((shape, inner(suffix, sides)))
}

fn arrow_of(suffix: &str) -> Result<(Shape, String), ()> {
    let rest = suffix.strip_prefix("<[").ok_or(())?;
    let end = rest.find("]>(").ok_or(())?;
    let label = inner_text(&rest[..end]);
    let dir = rest[end + 3..].strip_suffix(')').ok_or(())?.trim();
    let direction = match dir.to_ascii_lowercase().as_str() {
        "right" => Direction::Right,
        "left" => Direction::Left,
        "up" => Direction::Up,
        "down" => Direction::Down,
        "x" => Direction::BothAcross,
        "y" => Direction::BothDown,
        _ => return Err(()),
    };
    Ok((Shape::Arrow(direction), label))
}

fn inner(suffix: &str, sides: usize) -> String {
    inner_text(&suffix[sides..suffix.len() - sides])
}

fn inner_text(value: &str) -> String {
    unquote(value.trim())
}

fn split_span(token: &str) -> Result<(&str, usize), ()> {
    let mut byte = 0;
    let mut depth = 0i32;
    let mut quote = false;
    let mut colon = None;
    for ch in token.chars() {
        let next = byte + ch.len_utf8();
        if ch == '"' {
            quote = !quote;
        } else if !quote {
            match ch {
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth -= 1,
                ':' if depth == 0 => colon = Some(byte),
                _ => {}
            }
        }
        byte = next;
    }
    let Some(at) = colon else {
        return Ok((token, 1));
    };
    let num = &token[at + 1..];
    match num.parse::<usize>() {
        Ok(span) if span >= 1 => Ok((&token[..at], span)),
        _ => Err(()),
    }
}

fn has_class(token: &str) -> bool {
    let mut byte = 0;
    let mut depth = 0i32;
    let mut quote = false;
    let bytes = token.as_bytes();
    for ch in token.chars() {
        let next = byte + ch.len_utf8();
        if ch == '"' {
            quote = !quote;
        } else if !quote && depth == 0 && bytes[byte..].starts_with(b":::") {
            return true;
        } else if !quote {
            match ch {
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth -= 1,
                _ => {}
            }
        }
        byte = next;
    }
    false
}

fn is_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() && first != '_' {
        return false;
    }
    let mut hyphen = false;
    for ch in chars {
        if ch == '-' {
            if hyphen {
                return false;
            }
            hyphen = true;
        } else if ch.is_ascii_alphanumeric() || ch == '_' {
            hyphen = false;
        } else {
            return false;
        }
    }
    !id.ends_with('-')
}

fn tokens(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = None;
    let mut depth = 0i32;
    let mut quote = false;
    let mut byte = 0;
    for ch in line.chars() {
        let next = byte + ch.len_utf8();
        if quote {
            if ch == '"' {
                quote = false;
            }
        } else if ch == '"' && depth > 0 {
            quote = true;
            if start.is_none() {
                start = Some(byte);
            }
        } else if matches!(ch, '[' | '(' | '{') {
            depth += 1;
            if start.is_none() {
                start = Some(byte);
            }
        } else if matches!(ch, ']' | ')' | '}') {
            depth = depth.saturating_sub(1);
            if start.is_none() {
                start = Some(byte);
            }
        } else if ch.is_whitespace() && depth <= 0 {
            depth = 0;
            if let Some(at) = start.take() {
                out.push(&line[at..byte]);
            }
        } else if start.is_none() {
            start = Some(byte);
        }
        byte = next;
    }
    if let Some(at) = start {
        let token = line[at..].trim();
        if !token.is_empty() {
            out.push(token);
        }
    }
    out
}

fn split_amp(side: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut byte = 0;
    let mut depth = 0i32;
    let mut quote = false;
    for ch in side.chars() {
        let next = byte + ch.len_utf8();
        if ch == '"' {
            quote = !quote;
        } else if !quote {
            match ch {
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth -= 1,
                '&' if depth == 0 => {
                    out.push(&side[start..byte]);
                    start = next;
                }
                _ => {}
            }
        }
        byte = next;
    }
    out.push(&side[start..]);
    out
}

fn find_arrow(line: &str) -> Option<(usize, &'static str)> {
    const ARROWS: &[&str] = &[
        "<-.->", "<-->", "-.->", "<-.-", "==>", "<==", "===", "-->", "<--", "---", "-.-", "->",
        "<-",
    ];
    scan(line, |rest| {
        ARROWS.iter().copied().find(|arrow| rest.starts_with(arrow))
    })
}

fn find_opener(line: &str, opener: &str) -> Option<usize> {
    scan(line, |rest| rest.starts_with(opener).then_some(())).map(|(at, _)| at)
}

fn scan<T>(line: &str, mut hit: impl FnMut(&str) -> Option<T>) -> Option<(usize, T)> {
    let mut byte = 0;
    let mut depth = 0i32;
    let mut quote = false;
    for ch in line.chars() {
        let next = byte + ch.len_utf8();
        if ch == '"' {
            quote = !quote;
        } else if !quote {
            match ch {
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth -= 1,
                _ if depth == 0 => {
                    if let Some(found) = hit(&line[byte..]) {
                        return Some((byte, found));
                    }
                }
                _ => {}
            }
        }
        byte = next;
    }
    None
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

fn plain(value: &str) -> String {
    value
        .replace("<br/>", "\n")
        .replace("<br>", "\n")
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn readable(value: &str) -> bool {
    text::block(value, Font::new(16.0, Color::BLACK), Family::Sans).is_ok()
}

#[derive(Clone, Copy)]
struct Size {
    w: f32,
    h: f32,
}

struct GroupSize {
    outer: Size,
    band: f32,
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

    fn inflate(self, by: f32) -> Rect {
        Rect {
            x: self.x - by,
            y: self.y - by,
            w: self.w + 2.0 * by,
            h: self.h + 2.0 * by,
        }
    }

    fn intersects(self, other: Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }
}

struct Cell {
    item: Item,
    span: usize,
    col: usize,
    size: Size,
}

struct Laid {
    nodes: Vec<Rect>,
    groups: Vec<Rect>,
    bands: Vec<f32>,
    title: Option<Rect>,
    edges: Vec<Vec<(f32, f32)>>,
    edge_labels: Vec<Option<Rect>>,
}

fn layout(diagram: &Diagram, family: Family) -> Result<Laid, Declined> {
    let metrics = metrics(diagram.look.font_size);
    let node_size = measure_nodes(diagram, family, metrics)?;
    let group_size = measure_groups(diagram, family, metrics, &node_size)?;
    let mut nodes = vec![
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0
        };
        diagram.nodes.len()
    ];
    let mut groups = vec![
        Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0
        };
        diagram.groups.len()
    ];
    let mut bands = vec![0.0; diagram.groups.len()];
    let items = diagram.items.clone();
    let (grid_w, grid_h) = frame_size(
        diagram,
        &items,
        diagram.columns,
        &node_size,
        &group_size,
        metrics,
    );
    let title = diagram
        .title
        .as_ref()
        .map(|title| measure(title, diagram, family, true))
        .transpose()?;
    let title_h = title.as_ref().map(|block| block.height).unwrap_or(0.0);
    let title_w = title.as_ref().map(|block| block.width).unwrap_or(0.0);
    let picture_w = grid_w.max(title_w);
    let top = if title.is_some() {
        title_h + metrics.font * 0.75
    } else {
        0.0
    };
    let area = Rect {
        x: (picture_w - grid_w) / 2.0,
        y: top,
        w: grid_w,
        h: grid_h,
    };
    let title_rect = title.as_ref().map(|block| Rect {
        x: (picture_w - block.width) / 2.0,
        y: 0.0,
        w: block.width,
        h: block.height,
    });
    {
        let mut place = Place {
            diagram,
            node_size: &node_size,
            group_size: &group_size,
            metrics,
            nodes: &mut nodes,
            groups: &mut groups,
            bands: &mut bands,
        };
        place_frame(&mut place, &items, diagram.columns, area);
    }
    let mut laid = Laid {
        nodes,
        groups,
        bands,
        title: title_rect,
        edges: Vec::new(),
        edge_labels: Vec::new(),
    };
    route(diagram, family, &mut laid)?;
    Ok(laid)
}

fn measure_nodes(
    diagram: &Diagram,
    family: Family,
    metrics: Metrics,
) -> Result<Vec<Size>, Declined> {
    diagram
        .nodes
        .iter()
        .map(|node| {
            let block = measure(&text_at(&node.label, node.line), diagram, family, false)?;
            Ok(box_for(node.shape, block.width, block.height, metrics))
        })
        .collect()
}

fn measure_groups(
    diagram: &Diagram,
    family: Family,
    metrics: Metrics,
    node_size: &[Size],
) -> Result<Vec<GroupSize>, Declined> {
    let mut group_size = Vec::with_capacity(diagram.groups.len());
    for _ in &diagram.groups {
        group_size.push(GroupSize {
            outer: Size { w: 0.0, h: 0.0 },
            band: 0.0,
        });
    }
    for index in (0..diagram.groups.len()).rev() {
        let items = diagram.groups[index].items.clone();
        let columns = diagram.groups[index].columns;
        let line = diagram.groups[index].line;
        let caption = if diagram.groups[index].label.is_empty() {
            diagram.groups[index].id.clone()
        } else {
            diagram.groups[index].label.clone()
        };
        let (mut inner_w, mut inner_h) =
            frame_size(diagram, &items, columns, node_size, &group_size, metrics);
        if items.is_empty() {
            inner_w = metrics.min_w * 0.8;
            inner_h = metrics.min_h * 0.7;
        }
        let title = measure(&text_at(&caption, line), diagram, family, false)?;
        let band = (title.height + 8.0).max(metrics.font + 8.0);
        group_size[index] = GroupSize {
            outer: Size {
                w: inner_w.max(title.width) + 2.0 * metrics.pad,
                h: band + inner_h + metrics.pad,
            },
            band,
        };
    }
    Ok(group_size)
}

fn measure(text: &Text, diagram: &Diagram, family: Family, bold: bool) -> Result<Block, Declined> {
    let mut font = Font::new(diagram.look.font_size, diagram.look.preferred);
    if bold {
        font = font.bold();
    }
    text::block(&text.value, font, family).map_err(|_| Declined::at(text.line, "a label"))
}

fn box_for(shape: Shape, text_w: f32, text_h: f32, metrics: Metrics) -> Size {
    let w = text_w + metrics.font * 1.8;
    let h = text_h + metrics.font * 1.0;
    let (w, h) = match shape {
        Shape::Circle => {
            let d = text_w.hypot(text_h) + metrics.font;
            (d, d)
        }
        Shape::DoubleCircle => {
            let d = text_w.hypot(text_h) + metrics.font * 1.7;
            (d, d)
        }
        Shape::Diamond => (text_w * 2.0 + metrics.font, text_h * 2.0 + metrics.font),
        Shape::Hexagon => (w + metrics.font * 1.3, h + metrics.font * 0.4),
        Shape::Stadium => (w.max(h * 1.7), h),
        Shape::Arrow(Direction::Up | Direction::Down | Direction::BothDown) => (
            w.max(metrics.min_w * 0.8),
            (h + metrics.font).max(metrics.min_h * 1.6),
        ),
        Shape::Arrow(_) => ((w + metrics.font * 1.4).max(metrics.min_w * 1.3), h),
        Shape::Parallelogram
        | Shape::ParallelogramAlt
        | Shape::Trapezoid
        | Shape::TrapezoidAlt
        | Shape::Asymmetric => (w + metrics.font * 1.2, h),
        Shape::Subroutine => (w + metrics.font, h),
        Shape::Cylinder => (w, h + metrics.font * 0.8),
        _ => (w, h),
    };
    Size {
        w: w.max(metrics.min_w),
        h: h.max(metrics.min_h),
    }
}

fn columns_of(set: Option<usize>, items: usize) -> usize {
    if items == 0 {
        1
    } else {
        set.unwrap_or(items).max(1)
    }
}

fn frame_size(
    diagram: &Diagram,
    items: &[Item],
    columns: Option<usize>,
    node_size: &[Size],
    group_size: &[GroupSize],
    metrics: Metrics,
) -> (f32, f32) {
    let rows = rows_for(diagram, items, columns, node_size, group_size, metrics);
    let widths = column_widths(&rows, columns_of(columns, items.len()), metrics);
    grid_size(&rows, &widths, metrics)
}

fn item_span(item: Item, diagram_nodes: &[Node], diagram_groups: &[Group]) -> usize {
    match item {
        Item::Node(index) => diagram_nodes[index].span,
        Item::Group(index) => diagram_groups[index].span,
        Item::Space(span) => span,
    }
}

fn item_size(item: Item, node_size: &[Size], group_size: &[GroupSize], metrics: Metrics) -> Size {
    match item {
        Item::Node(index) => node_size[index],
        Item::Group(index) => group_size[index].outer,
        Item::Space(_) => Size {
            w: metrics.min_w * 0.55,
            h: metrics.min_h,
        },
    }
}

fn rows_for(
    diagram: &Diagram,
    items: &[Item],
    columns: Option<usize>,
    node_size: &[Size],
    group_size: &[GroupSize],
    metrics: Metrics,
) -> Vec<Vec<Cell>> {
    let columns = columns_of(columns, items.len());
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut col = 0usize;
    for &item in items {
        let span = item_span(item, &diagram.nodes, &diagram.groups).clamp(1, columns);
        if col > 0 && col + span > columns {
            rows.push(std::mem::take(&mut row));
            col = 0;
        }
        let mut size = item_size(item, node_size, group_size, metrics);
        if matches!(item, Item::Space(_)) {
            size.w *= span as f32;
        }
        row.push(Cell {
            item,
            span,
            col,
            size,
        });
        col += span;
        if col >= columns {
            rows.push(std::mem::take(&mut row));
            col = 0;
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

fn column_widths(rows: &[Vec<Cell>], columns: usize, metrics: Metrics) -> Vec<f32> {
    let mut width = vec![0.0_f32; columns];
    for row in rows {
        for cell in row {
            if cell.span == 1 {
                width[cell.col] = width[cell.col].max(cell.size.w);
            }
        }
    }
    for row in rows {
        for cell in row {
            if cell.span <= 1 {
                continue;
            }
            let have = width[cell.col..cell.col + cell.span].iter().sum::<f32>()
                + metrics.gap * (cell.span - 1) as f32;
            if have + 0.01 < cell.size.w {
                let extra = (cell.size.w - have) / cell.span as f32;
                for slot in &mut width[cell.col..cell.col + cell.span] {
                    *slot += extra;
                }
            }
        }
    }
    width
}

fn grid_size(rows: &[Vec<Cell>], widths: &[f32], metrics: Metrics) -> (f32, f32) {
    let used = rows
        .iter()
        .flat_map(|row| row.iter())
        .map(|cell| cell.col + cell.span)
        .max()
        .unwrap_or(0);
    if used == 0 {
        return (0.0, 0.0);
    }
    let width = widths[..used].iter().sum::<f32>() + metrics.gap * (used - 1) as f32;
    let mut height = 0.0;
    for (index, row) in rows.iter().enumerate() {
        height += row.iter().map(|cell| cell.size.h).fold(0.0_f32, f32::max);
        if index + 1 != rows.len() {
            height += metrics.gap;
        }
    }
    (width, height)
}

struct Place<'a> {
    diagram: &'a Diagram,
    node_size: &'a [Size],
    group_size: &'a [GroupSize],
    metrics: Metrics,
    nodes: &'a mut [Rect],
    groups: &'a mut [Rect],
    bands: &'a mut [f32],
}

fn place_frame(place: &mut Place<'_>, items: &[Item], columns: Option<usize>, area: Rect) {
    let rows = rows_for(
        place.diagram,
        items,
        columns,
        place.node_size,
        place.group_size,
        place.metrics,
    );
    let widths = column_widths(&rows, columns_of(columns, items.len()), place.metrics);
    let (grid_w, grid_h) = grid_size(&rows, &widths, place.metrics);
    let origin_x = area.x + (area.w - grid_w).max(0.0) / 2.0;
    let mut y = area.y + (area.h - grid_h).max(0.0) / 2.0;
    for row in &rows {
        let row_h = row.iter().map(|cell| cell.size.h).fold(0.0_f32, f32::max);
        let cells: Vec<(Item, usize, usize)> = row
            .iter()
            .map(|cell| (cell.item, cell.col, cell.span))
            .collect();
        for (item, col, span) in cells {
            let x = origin_x + widths[..col].iter().sum::<f32>() + place.metrics.gap * col as f32;
            let cell_w = widths[col..col + span].iter().sum::<f32>()
                + place.metrics.gap * span.saturating_sub(1) as f32;
            match item {
                Item::Node(index) => {
                    let size = place.node_size[index];
                    place.nodes[index] = Rect {
                        x: x + (cell_w - size.w) / 2.0,
                        y: y + (row_h - size.h) / 2.0,
                        w: size.w,
                        h: size.h,
                    };
                }
                Item::Space(_) => {}
                Item::Group(index) => {
                    place.groups[index] = Rect {
                        x,
                        y,
                        w: cell_w,
                        h: row_h,
                    };
                    let band = place.group_size[index].band;
                    place.bands[index] = band;
                    let pad = place.metrics.pad;
                    let frame = place.groups[index];
                    let inner = Rect {
                        x: frame.x + pad,
                        y: frame.y + band,
                        w: (frame.w - 2.0 * pad).max(0.0),
                        h: (frame.h - band - pad).max(0.0),
                    };
                    let children = place.diagram.groups[index].items.clone();
                    let nested = place.diagram.groups[index].columns;
                    place_frame(place, &children, nested, inner);
                }
            }
        }
        y += row_h + place.metrics.gap;
    }
}

fn contains_end(diagram: &Diagram, from: End, to: End) -> bool {
    match from {
        End::Group(index) if holds(diagram, index, to) => true,
        _ => match to {
            End::Group(index) => holds(diagram, index, from),
            End::Node(_) => false,
        },
    }
}

fn holds(diagram: &Diagram, group: usize, end: End) -> bool {
    for item in &diagram.groups[group].items {
        match *item {
            Item::Node(index) if End::Node(index) == end => return true,
            Item::Group(index) if End::Group(index) == end || holds(diagram, index, end) => {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn route(diagram: &Diagram, family: Family, laid: &mut Laid) -> Result<(), Declined> {
    let mut reserved = Vec::new();
    if let Some(rect) = laid.title {
        reserved.push(rect);
    }
    for (index, group) in laid.groups.iter().enumerate() {
        let band = laid.bands[index];
        if band > 0.0 {
            reserved.push(Rect {
                x: group.x,
                y: group.y,
                w: group.w,
                h: band,
            });
        }
    }
    for edge in &diagram.edges {
        let from = end_rect(edge.from, laid);
        let to = end_rect(edge.to, laid);
        let mut hard = Vec::new();
        if let End::Node(index) = edge.from {
            hard.push(laid.nodes[index]);
        }
        if let End::Node(index) = edge.to {
            hard.push(laid.nodes[index]);
        }
        let mut soft = Vec::new();
        for (index, node) in laid.nodes.iter().enumerate() {
            let own = matches!(edge.from, End::Node(n) if n == index)
                || matches!(edge.to, End::Node(n) if n == index);
            if !own {
                soft.push(node.inflate(6.0));
            }
        }
        for rect in reserved.iter() {
            soft.push(rect.inflate(2.0));
        }
        let path = choose_path(from, to, &soft, &hard);
        let label = match &edge.label {
            Some(text) => {
                let block = measure(text, diagram, family, false)?;
                let mut blocked = soft.clone();
                blocked.extend(hard.iter().copied());
                Some(place_label(&path, block.width, block.height, &blocked))
            }
            None => None,
        };
        for window in path.windows(2) {
            reserved.push(corridor(window[0], window[1]));
        }
        if let Some(rect) = label {
            reserved.push(rect);
        }
        laid.edges.push(path);
        laid.edge_labels.push(label);
    }
    Ok(())
}

fn end_rect(end: End, laid: &Laid) -> Rect {
    match end {
        End::Node(index) => laid.nodes[index],
        End::Group(index) => laid.groups[index],
    }
}

fn choose_path(from: Rect, to: Rect, soft: &[Rect], hard: &[Rect]) -> Vec<(f32, f32)> {
    let start = exit_point(from, to);
    let end = exit_point(to, from);
    let mut best: Option<(f32, Vec<(f32, f32)>)> = None;
    let mut offer = |points: Vec<(f32, f32)>| {
        let path = clean(points);
        if path.len() < 2 || !path_clear(&path, soft, hard) {
            return;
        }
        let len = path_len(&path);
        if best.as_ref().is_none_or(|(have, _)| len + 0.05 < *have) {
            best = Some((len, path));
        }
    };
    if (start.1 - end.1).abs() < 0.8 || (start.0 - end.0).abs() < 0.8 {
        offer(vec![start, end]);
    }
    for x in lanes(start.0, end.0, soft, true) {
        offer(vec![start, (x, start.1), (x, end.1), end]);
    }
    for y in lanes(start.1, end.1, soft, false) {
        offer(vec![start, (start.0, y), (end.0, y), end]);
    }
    // Leave each side at a right angle, then use a lane. A one-bend path
    // can only do that by travelling along the side it just left.
    let leave = jog(from, start);
    let arrive = jog(to, end);
    for y in lanes(leave.1, arrive.1, soft, false) {
        offer(vec![start, leave, (leave.0, y), (arrive.0, y), arrive, end]);
    }
    for x in lanes(leave.0, arrive.0, soft, true) {
        offer(vec![start, leave, (x, leave.1), (x, arrive.1), arrive, end]);
    }
    best.map(|(_, path)| path).unwrap_or_else(|| {
        clean(vec![
            start,
            ((start.0 + end.0) / 2.0, start.1),
            ((start.0 + end.0) / 2.0, end.1),
            end,
        ])
    })
}

fn jog(rect: Rect, port: (f32, f32)) -> (f32, f32) {
    const STEP: f32 = 12.0;
    if (port.0 - rect.x).abs() <= 0.2 || (port.0 - rect.right()).abs() <= 0.2 {
        let sign = if port.0 >= rect.cx() { 1.0 } else { -1.0 };
        (port.0 + sign * STEP, port.1)
    } else {
        let sign = if port.1 >= rect.cy() { 1.0 } else { -1.0 };
        (port.0, port.1 + sign * STEP)
    }
}

fn exit_point(rect: Rect, other: Rect) -> (f32, f32) {
    let dx = other.cx() - rect.cx();
    let dy = other.cy() - rect.cy();
    let margin = 8.0;
    if dx.abs() >= dy.abs() {
        let x = if dx >= 0.0 { rect.right() } else { rect.x };
        let y = rect.cy().clamp(
            rect.y + margin,
            (rect.bottom() - margin).max(rect.y + margin),
        );
        (x, y)
    } else {
        let y = if dy >= 0.0 { rect.bottom() } else { rect.y };
        let x = rect.cx().clamp(
            rect.x + margin,
            (rect.right() - margin).max(rect.x + margin),
        );
        (x, y)
    }
}

fn lanes(a: f32, b: f32, soft: &[Rect], vertical: bool) -> Vec<f32> {
    let mid = (a + b) / 2.0;
    let mut values = vec![mid];
    for step in [12.0, 24.0, 36.0, 52.0, 72.0] {
        values.push(mid - step);
        values.push(mid + step);
    }
    for rect in soft {
        if vertical {
            values.push(rect.x - 8.0);
            values.push(rect.right() + 8.0);
        } else {
            values.push(rect.y - 8.0);
            values.push(rect.bottom() + 8.0);
        }
    }
    values.sort_by(|left, right| (left - mid).abs().total_cmp(&(right - mid).abs()));
    let mut kept = Vec::new();
    for value in values {
        if kept.iter().any(|have: &f32| (have - value).abs() < 0.8) {
            continue;
        }
        kept.push(value);
        if kept.len() == 16 {
            break;
        }
    }
    kept
}

fn path_clear(path: &[(f32, f32)], soft: &[Rect], hard: &[Rect]) -> bool {
    path.windows(2).all(|window| {
        let misses_soft = soft
            .iter()
            .all(|rect| !segment_hits(window[0], window[1], *rect));
        let misses_hard = hard.iter().all(|rect| {
            !segment_hits(window[0], window[1], *rect)
                && !segment_rides(window[0], window[1], *rect)
        });
        misses_soft && misses_hard
    })
}

fn segment_hits(a: (f32, f32), b: (f32, f32), rect: Rect) -> bool {
    if (a.1 - b.1).abs() <= 0.05 {
        let (x1, x2) = if a.0 < b.0 { (a.0, b.0) } else { (b.0, a.0) };
        a.1 > rect.y && a.1 < rect.bottom() && x2 > rect.x && x1 < rect.right()
    } else if (a.0 - b.0).abs() <= 0.05 {
        let (y1, y2) = if a.1 < b.1 { (a.1, b.1) } else { (b.1, a.1) };
        a.0 > rect.x && a.0 < rect.right() && y2 > rect.y && y1 < rect.bottom()
    } else {
        true
    }
}

/// A segment that travels along a side, rather than meeting it.
fn segment_rides(a: (f32, f32), b: (f32, f32), rect: Rect) -> bool {
    if (a.1 - b.1).abs() <= 0.05 {
        let on = (a.1 - rect.y).abs() <= 0.2 || (a.1 - rect.bottom()).abs() <= 0.2;
        if on {
            let (x1, x2) = if a.0 < b.0 { (a.0, b.0) } else { (b.0, a.0) };
            return x2.min(rect.right()) - x1.max(rect.x) > 1.0;
        }
    } else if (a.0 - b.0).abs() <= 0.05 {
        let on = (a.0 - rect.x).abs() <= 0.2 || (a.0 - rect.right()).abs() <= 0.2;
        if on {
            let (y1, y2) = if a.1 < b.1 { (a.1, b.1) } else { (b.1, a.1) };
            return y2.min(rect.bottom()) - y1.max(rect.y) > 1.0;
        }
    }
    false
}

fn path_len(path: &[(f32, f32)]) -> f32 {
    path.windows(2)
        .map(|window| (window[0].0 - window[1].0).hypot(window[0].1 - window[1].1))
        .sum()
}

fn clean(path: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut points = Vec::new();
    for point in path {
        if points.last().is_some_and(|have: &(f32, f32)| {
            (have.0 - point.0).abs() < 0.05 && (have.1 - point.1).abs() < 0.05
        }) {
            continue;
        }
        points.push(point);
    }
    if points.len() < 3 {
        return points;
    }
    let mut simple = vec![points[0]];
    for index in 1..points.len() - 1 {
        let a = simple[simple.len() - 1];
        let b = points[index];
        let c = points[index + 1];
        let same_x = (a.0 - b.0).abs() < 0.05 && (b.0 - c.0).abs() < 0.05;
        let same_y = (a.1 - b.1).abs() < 0.05 && (b.1 - c.1).abs() < 0.05;
        if !(same_x || same_y) {
            simple.push(b);
        }
    }
    simple.push(points[points.len() - 1]);
    simple
}

fn corridor(a: (f32, f32), b: (f32, f32)) -> Rect {
    if (a.1 - b.1).abs() <= (a.0 - b.0).abs() {
        Rect {
            x: a.0.min(b.0),
            y: a.1 - 5.0,
            w: (a.0 - b.0).abs().max(1.0),
            h: 10.0,
        }
    } else {
        Rect {
            x: a.0 - 5.0,
            y: a.1.min(b.1),
            w: 10.0,
            h: (a.1 - b.1).abs().max(1.0),
        }
    }
}

fn place_label(path: &[(f32, f32)], w: f32, h: f32, blocked: &[Rect]) -> Rect {
    let Some((a, b)) = longest(path) else {
        return Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
        };
    };
    let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let horizontal = (a.1 - b.1).abs() <= (a.0 - b.0).abs();
    let span = if horizontal { b.0 - a.0 } else { b.1 - a.1 };
    let mut fallback = Rect {
        x: mid.0 - w / 2.0,
        y: mid.1 - h - 4.0,
        w,
        h,
    };
    for sign in [1.0_f32, -1.0] {
        for slide in [0.0, 0.28, -0.28, 0.5, -0.5] {
            let along = slide * span;
            let rect = if horizontal {
                Rect {
                    x: mid.0 + along - w / 2.0,
                    y: if sign > 0.0 {
                        mid.1 - 4.0 - h
                    } else {
                        mid.1 + 4.0
                    },
                    w,
                    h,
                }
            } else {
                let left = if sign > 0.0 {
                    mid.0 + 6.0
                } else {
                    mid.0 - 6.0 - w
                };
                Rect {
                    x: left,
                    y: mid.1 + along - h / 2.0,
                    w,
                    h,
                }
            };
            fallback = rect;
            if blocked.iter().all(|have| !rect.intersects(*have)) {
                return rect;
            }
        }
    }
    fallback
}

fn longest(path: &[(f32, f32)]) -> Option<((f32, f32), (f32, f32))> {
    path.windows(2)
        .max_by(|left, right| {
            let len = |pair: &[(f32, f32)]| (pair[0].0 - pair[1].0).hypot(pair[0].1 - pair[1].1);
            len(left).total_cmp(&len(right))
        })
        .map(|window| (window[0], window[1]))
}

fn paint(
    canvas: &mut Canvas,
    diagram: &Diagram,
    laid: &Laid,
    family: Family,
) -> Result<(), Declined> {
    let look = &diagram.look;
    for (index, group) in diagram.groups.iter().enumerate() {
        let rect = laid.groups[index];
        if rect.w <= 0.0 || rect.h <= 0.0 {
            continue;
        }
        let fill = group.fill.unwrap_or_else(|| group_fill(look));
        let color = group.stroke.unwrap_or_else(|| border(look));
        let width = group.stroke_width.unwrap_or(1.4);
        canvas.rect(
            Layer::Frames,
            (rect.x, rect.y, rect.w, rect.h),
            8.0,
            Some(fill),
            Some(Stroke::new(color, width).dashed(7.0, 5.0)),
        );
    }
    for (index, node) in diagram.nodes.iter().enumerate() {
        draw_node(canvas, node, laid.nodes[index], look);
    }
    for (index, edge) in diagram.edges.iter().enumerate() {
        draw_edge(canvas, edge, &laid.edges[index], look);
    }
    for (index, group) in diagram.groups.iter().enumerate() {
        let caption = if group.label.is_empty() {
            &group.id
        } else {
            &group.label
        };
        let fill = group.fill.unwrap_or_else(|| group_fill(look));
        let color = group.text.unwrap_or_else(|| ink(fill, look));
        let block = text::block(caption, Font::new(look.font_size, color), family)
            .map_err(|_| Declined::at(group.line, "a label"))?;
        let rect = laid.groups[index];
        let band = laid.bands[index];
        let y = rect.y + (band - block.height).max(2.0) / 2.0;
        canvas.text(Layer::Front, &block, (rect.cx(), y), Align::Center);
    }
    for (index, node) in diagram.nodes.iter().enumerate() {
        let fill = node.fill.unwrap_or_else(|| tile(look));
        let color = node.text.unwrap_or_else(|| ink(fill, look));
        let block = text::block(&node.label, Font::new(look.font_size, color), family)
            .map_err(|_| Declined::at(node.line, "a label"))?;
        let (x, y) = anchor(node.shape, laid.nodes[index]);
        canvas.text(
            Layer::Front,
            &block,
            (x, y - block.height / 2.0),
            Align::Center,
        );
    }
    for (index, edge) in diagram.edges.iter().enumerate() {
        if let (Some(label), Some(rect)) = (&edge.label, laid.edge_labels[index]) {
            let block = text::block(
                &label.value,
                Font::new(look.font_size, look.preferred),
                family,
            )
            .map_err(|_| Declined::at(label.line, "a label"))?;
            canvas.text(Layer::Front, &block, (rect.cx(), rect.y), Align::Center);
        }
    }
    if let (Some(title), Some(rect)) = (&diagram.title, laid.title) {
        let block = text::block(
            &title.value,
            Font::new(look.font_size, look.preferred).bold(),
            family,
        )
        .map_err(|_| Declined::at(title.line, "a title"))?;
        canvas.text(Layer::Front, &block, (rect.cx(), rect.y), Align::Center);
    }
    Ok(())
}

fn tile(look: &Look) -> Color {
    if look.classic {
        Color::hex(CLASSIC_FILL)
    } else {
        let page = look.background.unwrap_or(Color::WHITE);
        page.mix(look.accent, if page.is_dark() { 0.34 } else { 0.16 })
    }
}

fn border(look: &Look) -> Color {
    if look.classic {
        Color::hex(CLASSIC_BORDER)
    } else {
        look.accent
    }
}

fn group_fill(look: &Look) -> Color {
    let page = look.background.unwrap_or(Color::WHITE);
    if look.classic {
        page.mix(Color::hex(CLASSIC_FILL), 0.45)
    } else {
        page.mix(look.accent, if page.is_dark() { 0.14 } else { 0.07 })
    }
}

fn ink(fill: Color, look: &Look) -> Color {
    if fill.is_dark() != look.preferred.is_dark() {
        look.preferred
    } else if fill.is_dark() {
        Color::WHITE
    } else {
        Color::hex(0x1A1A1A)
    }
}

fn anchor(shape: Shape, rect: Rect) -> (f32, f32) {
    match shape {
        Shape::Arrow(Direction::Right | Direction::BothAcross) => {
            (rect.cx() - rect.w * 0.06, rect.cy())
        }
        Shape::Arrow(Direction::Left) => (rect.cx() + rect.w * 0.06, rect.cy()),
        Shape::Arrow(Direction::Down | Direction::BothDown) => {
            (rect.cx(), rect.cy() - rect.h * 0.06)
        }
        Shape::Arrow(Direction::Up) => (rect.cx(), rect.cy() + rect.h * 0.06),
        Shape::Asymmetric => (rect.cx() - rect.w * 0.08, rect.cy()),
        _ => (rect.cx(), rect.cy()),
    }
}

fn draw_node(canvas: &mut Canvas, node: &Node, rect: Rect, look: &Look) {
    let fill = node.fill.unwrap_or_else(|| tile(look));
    let color = node.stroke.unwrap_or_else(|| border(look));
    let stroke = Stroke::new(color, node.stroke_width.unwrap_or(1.6));
    match node.shape {
        Shape::Rectangle => canvas.rect(Layer::Bars, box_of(rect), 4.0, Some(fill), Some(stroke)),
        Shape::Round => {
            canvas.rect(
                Layer::Bars,
                box_of(rect),
                (rect.h * 0.28).min(16.0),
                Some(fill),
                Some(stroke),
            );
        }
        Shape::Stadium => canvas.rect(
            Layer::Bars,
            box_of(rect),
            rect.h / 2.0,
            Some(fill),
            Some(stroke),
        ),
        Shape::Subroutine => {
            canvas.rect(Layer::Bars, box_of(rect), 0.0, Some(fill), Some(stroke));
            let inset = (rect.w * 0.08).clamp(6.0, 14.0);
            canvas.line(
                Layer::Bars,
                (rect.x + inset, rect.y),
                (rect.x + inset, rect.bottom()),
                stroke,
            );
            canvas.line(
                Layer::Bars,
                (rect.right() - inset, rect.y),
                (rect.right() - inset, rect.bottom()),
                stroke,
            );
        }
        Shape::Circle => {
            canvas.ellipse(
                Layer::Bars,
                (rect.cx(), rect.cy()),
                (rect.w / 2.0, rect.h / 2.0),
                Some(fill),
                Some(stroke),
            );
        }
        Shape::DoubleCircle => {
            canvas.ellipse(
                Layer::Bars,
                (rect.cx(), rect.cy()),
                (rect.w / 2.0, rect.h / 2.0),
                Some(fill),
                Some(stroke),
            );
            let inset = (rect.w * 0.08).clamp(4.0, 8.0);
            canvas.ellipse(
                Layer::Bars,
                (rect.cx(), rect.cy()),
                (
                    (rect.w / 2.0 - inset).max(1.0),
                    (rect.h / 2.0 - inset).max(1.0),
                ),
                None,
                Some(stroke),
            );
        }
        Shape::Cylinder => draw_cylinder(canvas, rect, fill, stroke),
        Shape::Arrow(direction) => {
            canvas.polygon(
                Layer::Bars,
                &chevron(rect, direction),
                Some(fill),
                Some(stroke),
            );
        }
        shape => canvas.polygon(Layer::Bars, &outline(shape, rect), Some(fill), Some(stroke)),
    }
}

fn box_of(rect: Rect) -> (f32, f32, f32, f32) {
    (rect.x, rect.y, rect.w, rect.h)
}

fn draw_cylinder(canvas: &mut Canvas, rect: Rect, fill: Color, stroke: Stroke) {
    let ry = (rect.h * 0.14).clamp(6.0, 14.0);
    let d = format!(
        "M {x} {top} A {rx} {ry} 0 0 1 {right} {top} L {right} {bot} A {rx} {ry} 0 0 1 {x} {bot} Z",
        x = n(rect.x),
        right = n(rect.right()),
        top = n(rect.y + ry),
        bot = n(rect.bottom() - ry),
        rx = n(rect.w / 2.0),
        ry = n(ry),
    );
    canvas.path(
        Layer::Bars,
        &d,
        (rect.x, rect.y, rect.right(), rect.bottom()),
        Some(fill),
        Some(stroke),
    );
    canvas.ellipse(
        Layer::Bars,
        (rect.cx(), rect.y + ry),
        (rect.w / 2.0, ry),
        Some(fill),
        Some(stroke),
    );
}

fn outline(shape: Shape, rect: Rect) -> Vec<(f32, f32)> {
    let Rect { x, y, w, h } = rect;
    let (cx, cy) = (rect.cx(), rect.cy());
    match shape {
        Shape::Diamond => vec![(cx, y), (x + w, cy), (cx, y + h), (x, cy)],
        Shape::Hexagon => {
            let inset = w * 0.2;
            vec![
                (x + inset, y),
                (x + w - inset, y),
                (x + w, cy),
                (x + w - inset, y + h),
                (x + inset, y + h),
                (x, cy),
            ]
        }
        Shape::Parallelogram => {
            let skew = w * 0.16;
            vec![(x + skew, y), (x + w, y), (x + w - skew, y + h), (x, y + h)]
        }
        Shape::ParallelogramAlt => {
            let skew = w * 0.16;
            vec![(x, y), (x + w - skew, y), (x + w, y + h), (x + skew, y + h)]
        }
        Shape::Trapezoid => {
            let skew = w * 0.14;
            vec![(x + skew, y), (x + w - skew, y), (x + w, y + h), (x, y + h)]
        }
        Shape::TrapezoidAlt => {
            let skew = w * 0.14;
            vec![(x, y), (x + w, y), (x + w - skew, y + h), (x + skew, y + h)]
        }
        Shape::Asymmetric => {
            let skew = w * 0.22;
            vec![
                (x, y),
                (x + w - skew, y),
                (x + w, cy),
                (x + w - skew, y + h),
                (x, y + h),
            ]
        }
        _ => vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)],
    }
}

fn chevron(rect: Rect, direction: Direction) -> Vec<(f32, f32)> {
    let Rect { x, y, w, h } = rect;
    let inset = (w.min(h) * 0.28).clamp(10.0, 22.0);
    let (cx, cy) = (rect.cx(), rect.cy());
    match direction {
        Direction::Right => vec![
            (x, y),
            (x + w - inset, y),
            (x + w, cy),
            (x + w - inset, y + h),
            (x, y + h),
            (x + inset * 0.75, cy),
        ],
        Direction::Left => vec![
            (x + w, y),
            (x + inset, y),
            (x, cy),
            (x + inset, y + h),
            (x + w, y + h),
            (x + w - inset * 0.75, cy),
        ],
        Direction::Down => vec![
            (x, y),
            (x, y + h - inset),
            (cx, y + h),
            (x + w, y + h - inset),
            (x + w, y),
            (cx, y + inset * 0.75),
        ],
        Direction::Up => vec![
            (x, y + h),
            (x, y + inset),
            (cx, y),
            (x + w, y + inset),
            (x + w, y + h),
            (cx, y + h - inset * 0.75),
        ],
        Direction::BothAcross => vec![
            (x + inset, y),
            (x + w - inset, y),
            (x + w, cy),
            (x + w - inset, y + h),
            (x + inset, y + h),
            (x, cy),
        ],
        Direction::BothDown => vec![
            (x, y + inset),
            (cx, y),
            (x + w, y + inset),
            (x + w, y + h - inset),
            (cx, y + h),
            (x, y + h - inset),
        ],
    }
}

fn draw_edge(canvas: &mut Canvas, edge: &Edge, points: &[(f32, f32)], look: &Look) {
    if points.len() < 2 {
        return;
    }
    let mut stroke = Stroke::new(look.line, if edge.thick { 2.8 } else { 1.6 });
    if edge.dotted {
        stroke = stroke.dashed(5.0, 4.0);
    }
    let head = (look.font_size * 0.55).clamp(8.0, 14.0);
    let mut shaft = points.to_vec();
    if edge.arrow_end {
        pull_end(&mut shaft, head * 0.85);
    }
    if edge.arrow_start {
        pull_start(&mut shaft, head * 0.85);
    }
    canvas.polyline(Layer::Front, &shaft, stroke);
    if edge.arrow_end
        && let Some(dir) = unit(points[points.len() - 2], points[points.len() - 1])
    {
        canvas.polygon(
            Layer::Front,
            &arrow_head(points[points.len() - 1], dir, head),
            Some(look.line),
            None,
        );
    }
    if edge.arrow_start
        && let Some(dir) = unit(points[1], points[0])
    {
        canvas.polygon(
            Layer::Front,
            &arrow_head(points[0], dir, head),
            Some(look.line),
            None,
        );
    }
}

fn pull_end(points: &mut [(f32, f32)], amount: f32) {
    let last = points.len() - 1;
    if let Some((dx, dy)) = unit(points[last - 1], points[last]) {
        let len = (points[last].0 - points[last - 1].0).hypot(points[last].1 - points[last - 1].1);
        let cut = amount.min(len * 0.6);
        points[last] = (points[last].0 - dx * cut, points[last].1 - dy * cut);
    }
}

fn pull_start(points: &mut [(f32, f32)], amount: f32) {
    if let Some((dx, dy)) = unit(points[1], points[0]) {
        let len = (points[0].0 - points[1].0).hypot(points[0].1 - points[1].1);
        let cut = amount.min(len * 0.6);
        points[0] = (points[0].0 - dx * cut, points[0].1 - dy * cut);
    }
}

fn unit(a: (f32, f32), b: (f32, f32)) -> Option<(f32, f32)> {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = dx.hypot(dy);
    (len >= 0.5).then_some((dx / len, dy / len))
}

fn arrow_head(tip: (f32, f32), dir: (f32, f32), size: f32) -> Vec<(f32, f32)> {
    let base = (tip.0 - dir.0 * size, tip.1 - dir.1 * size);
    let px = -dir.1 * size * 0.42;
    let py = dir.0 * size * 0.42;
    vec![tip, (base.0 + px, base.1 + py), (base.0 - px, base.1 - py)]
}

#[cfg(test)]
mod tests {
    use super::{Direction, Shape, draw, from_mermaid, layout, mermaid_refusal, segment_hits};
    use crate::color::Color;
    use crate::text::{Family, Font};

    fn source(text: &str) -> super::Diagram {
        from_mermaid(text)
            .unwrap_or_else(|| panic!("refused {text:?}: {:?}", mermaid_refusal(text)))
    }

    fn placed(text: &str) -> super::Laid {
        layout(&source(text), Family::Sans).unwrap()
    }

    fn holds(outer: super::Rect, inner: super::Rect) -> bool {
        inner.x >= outer.x - 0.2
            && inner.y >= outer.y - 0.2
            && inner.right() <= outer.right() + 0.2
            && inner.bottom() <= outer.bottom() + 0.2
    }

    #[test]
    fn puts_a_row_on_one_line_and_a_column_in_a_stack() {
        let row = placed("block-beta\na[\"A\"] b[\"B\"] c[\"C\"]\n");
        assert!(row.nodes[1].x > row.nodes[0].right());
        assert!((row.nodes[0].y - row.nodes[2].y).abs() < 0.8);
        let stack = placed("block-beta\ncolumns 1\na\nb\n");
        assert!((stack.nodes[0].x - stack.nodes[1].x).abs() < 0.8);
        assert!(stack.nodes[1].y > stack.nodes[0].bottom());
    }

    #[test]
    fn keeps_children_inside_the_group_and_the_sibling_outside() {
        let laid = placed("block-beta\nblock:g[\"Group\"]\n  a[\"Child\"]\nend\nc[\"Side\"]\n");
        let group = laid.groups[0];
        let child = laid.nodes[0];
        let side = laid.nodes[1];
        assert!(holds(group, child), "child {child:?} group {group:?}");
        assert!(child.y >= group.y + laid.bands[0] - 0.2);
        assert!(!side.intersects(group), "side {side:?} group {group:?}");
        let svg = draw(&source(
            "block-beta\nblock:g[\"Group\"]\n  a[\"Child\"]\nend\nc[\"Side\"]\n",
        ))
        .unwrap();
        assert!(
            svg.contains("Group")
                && svg.contains("Child")
                && svg.contains("stroke-dasharray=\"7,5\"")
        );
    }

    #[test]
    fn an_edge_goes_around_the_block_between() {
        let laid = placed("block-beta\ncolumns 3\na b c\na -->|go| c\n");
        let middle = laid.nodes[1];
        assert!(laid.nodes[2].x > middle.right());
        for edge in &laid.edges {
            for window in edge.windows(2) {
                assert!(
                    !segment_hits(window[0], window[1], middle.inflate(1.0)),
                    "edge {edge:?} hits {middle:?}"
                );
            }
        }
        let label = laid.edge_labels[0].unwrap();
        for node in &laid.nodes {
            assert!(!label.intersects(*node), "label {label:?} node {node:?}");
        }
    }

    #[test]
    fn an_edge_meets_the_side_it_enters() {
        let laid = placed(
            "block-beta\ncolumns 3\n  a[\"A\"] b[\"B\"] c[\"C\"]\n  d[\"D\"] e[\"E\"] f[\"F\"]\n  c --> e\n",
        );
        let target = laid.nodes[4];
        let edge = &laid.edges[0];
        let end = edge[edge.len() - 1];
        let prev = edge[edge.len() - 2];
        assert!(
            !super::segment_rides(prev, end, target),
            "edge {edge:?} rides {target:?}"
        );
        assert!(
            !segment_hits(prev, end, target.inflate(-0.5)),
            "edge {edge:?} enters {target:?}"
        );
    }

    #[test]
    fn wraps_on_the_column_count_and_leaves_a_space() {
        let wrapped = placed("block-beta\ncolumns 2\nwide:2\nnext\n");
        assert!(wrapped.nodes[1].y > wrapped.nodes[0].bottom());
        let spaced = placed("block-beta\ncolumns 3\na space c\n");
        assert!(spaced.nodes[1].x > spaced.nodes[0].right() + 80.0);
    }

    #[test]
    fn reads_shapes_labels_and_arrows() {
        let diagram = source(
            "block\n  a[\"A label\"] b(\"B\") c((\"C\")) d(((\"D\"))) e{\"E\"} f{{\"F\"}} g[[\"G\"]] h[(\"H\")] i([\"I\"]) j[/J/] k[\\K\\] l[/L\\] m[\\M/] n>N] o<[\"O\"]>(down) z\n",
        );
        let shapes = [
            Shape::Rectangle,
            Shape::Round,
            Shape::Circle,
            Shape::DoubleCircle,
            Shape::Diamond,
            Shape::Hexagon,
            Shape::Subroutine,
            Shape::Cylinder,
            Shape::Stadium,
            Shape::Parallelogram,
            Shape::ParallelogramAlt,
            Shape::Trapezoid,
            Shape::TrapezoidAlt,
            Shape::Asymmetric,
            Shape::Arrow(Direction::Down),
            Shape::Rectangle,
        ];
        assert_eq!(diagram.nodes.len(), shapes.len());
        for (node, shape) in diagram.nodes.iter().zip(shapes) {
            assert_eq!(node.shape, shape, "{}", node.id);
        }
        assert_eq!(diagram.nodes[0].label, "A label");
        assert_eq!(diagram.nodes[15].label, "z");
        let edge = source("block-beta\na b c\na -- go --> b\na & b --> c\n");
        assert_eq!(edge.edges[0].label.as_ref().unwrap().value, "go");
        assert!(edge.edges[0].arrow_end && !edge.edges[0].arrow_start);
        assert_eq!(edge.edges.len(), 3);
        let style = source(
            "block-beta\na b\nstyle a fill:#FF00AA,stroke:#112233,color:#fff,stroke-width:3px\n",
        );
        assert_eq!(style.nodes[0].fill, Some(Color::hex(0xFF00AA)));
        assert_eq!(style.nodes[0].stroke, Some(Color::hex(0x112233)));
        assert_eq!(style.nodes[0].text, Some(Color::hex(0xFFFFFF)));
        assert_eq!(style.nodes[0].stroke_width, Some(3.0));
    }

    #[test]
    fn names_the_line_it_refuses() {
        let cases = [
            ("block-beta\nclassDef a fill:#fff\n", 1, "a class"),
            ("block-beta\na\nclass a foo\n", 2, "a class"),
            ("block-beta\na:::hidden\n", 1, "a class"),
            ("block-beta\na b\nlinkStyle 0 stroke:#fff\n", 2, "a style"),
            ("block-beta\na\na\n", 2, "an element"),
            ("block-beta\na\na --> a\n", 2, "an edge"),
            ("block-beta\na --> b\n", 1, "an edge"),
            ("block-beta\nblock:g\n  a\n", 1, "a block"),
            ("block-beta\nend\n", 1, "a block"),
            ("block-beta\nend extra\n", 1, "a block"),
            ("block-beta\nblock:\nend\n", 1, "a block"),
            ("block-beta\ncolumns\n", 1, "a column"),
            ("block-beta\ncolumns 0\n", 1, "a column"),
            ("block-beta\ncolumns two\n", 1, "a column"),
            ("block-beta\ncolumns 2\ncolumns 1\n", 2, "a column"),
            ("block-beta\ntitle\n", 1, "a title"),
            ("block-beta\ntitle One\ntitle Two\n", 2, "a title"),
            ("block-beta\ndirection LR\n", 1, "an unknown line"),
            ("block-beta extra\n", 0, "an unknown line"),
            ("block-beta\nblock\n", 1, "an unknown line"),
            ("block-beta\nstyle missing fill:#fff\n", 1, "a style"),
            ("block-beta\na\nstyle a fill:nope\n", 2, "a style"),
            ("block-beta\na\nstyle a opacity:1\n", 2, "a style"),
            ("block-beta\naccDescr {\nstill\n", 1, "an unknown line"),
        ];
        for (text, line, what) in cases {
            assert_eq!(mermaid_refusal(text), Some((line, what)), "{text}");
            assert!(from_mermaid(text).is_none(), "{text}");
        }
        assert!(from_mermaid("flowchart TB\nblock --> next\n").is_none());
        assert!(mermaid_refusal("flowchart TB\nblock --> next\n").is_none());
    }

    #[test]
    fn the_stock_theme_is_lilac_and_a_dark_page_uses_the_accent() {
        let text = "block-beta\ntitle Blocks\na[\"Web\"] b[\"DB\"]\na --> b\n";
        let svg = draw(&source(text)).unwrap();
        assert_eq!(draw(&source(text)).unwrap(), svg);
        assert!(svg.contains("Web") && svg.contains("Blocks"), "{svg}");
        assert!(
            svg.contains("#ECECFF")
                && svg.contains("#7B88A8")
                && svg.contains("#333333")
                && svg.contains("#2F3B4D"),
            "{svg}"
        );
        let mut dark = source(text);
        dark.look.classic = false;
        dark.look.accent = Color::hex(0xE68E0D);
        dark.look.line = Color::hex(0xE68E0D);
        dark.look.preferred = Color::hex(0xBEBEBE);
        dark.look.background = Some(Color::hex(0x121212));
        let svg = draw(&dark).unwrap();
        assert!(
            svg.contains("#E68E0D") && svg.contains("#BEBEBE") && svg.contains("#121212"),
            "{svg}"
        );
        assert!(!svg.contains("#ECECFF"), "{svg}");
        let chevron = draw(&source("block-beta\ncolumns 1\ngo<[\"flow\"]>(right)\n")).unwrap();
        assert!(
            chevron.contains("<polygon") && chevron.contains("flow"),
            "{chevron}"
        );
    }

    #[test]
    fn a_label_stays_inside_its_rectangle() {
        let laid = placed("block-beta\na[\"Hello\"] b[\"World\"]\n");
        for (node, label) in laid.nodes.iter().zip(["Hello", "World"]) {
            let block =
                crate::text::block(label, Font::new(16.0, Color::BLACK), Family::Sans).unwrap();
            let left = node.cx() - block.width / 2.0;
            let top = node.cy() - block.height / 2.0;
            assert!(left >= node.x && top >= node.y, "{label}");
            assert!(left + block.width <= node.right() && top + block.height <= node.bottom());
        }
        assert!(!laid.nodes[0].intersects(laid.nodes[1]));
    }
}
