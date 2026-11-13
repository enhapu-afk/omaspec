//! Reading a Mermaid block diagram: the columns, the blocks and the groups
//! in them, the edges between them, and their styles.

use super::lay::contains_end;
use super::tokens::{
    find_arrow, find_opener, plain, readable, split_amp, strip_comment, tokens, unquote,
};
use super::{Diagram, Direction, Edge, End, Group, Item, Look, NEST_LIMIT, Node, Shape};
use crate::color::Color;
use crate::common::{keyword, text_at};
use std::collections::HashMap;

pub(super) struct Reason {
    pub(super) line: usize,
    pub(super) what: &'static str,
}

pub(super) enum Stop {
    Foreign,
    At(Reason),
}

pub(super) fn fail(line: usize, what: &'static str) -> Stop {
    Stop::At(Reason { line, what })
}

#[derive(Clone, Copy)]
pub(super) struct Metrics {
    pub(super) gap: f32,
    pub(super) pad: f32,
    pub(super) min_w: f32,
    pub(super) min_h: f32,
    pub(super) font: f32,
}

pub(super) fn metrics(font: f32) -> Metrics {
    Metrics {
        gap: font * 2.2 + 16.0,
        pad: (font * 0.9).max(12.0),
        min_w: font * 4.4,
        min_h: font * 2.5,
        font,
    }
}

pub(super) fn read(source: &str) -> Result<Diagram, Stop> {
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
    let front =
        crate::source::front_lines(source).map_err(|no| fail(no, "a front matter setting"))?;
    for (line_no, raw) in source.lines().enumerate().skip(front) {
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

pub(super) fn frame_columns(diagram: &mut Diagram, group: Option<usize>) -> &mut Option<usize> {
    match group {
        Some(index) => &mut diagram.groups[index].columns,
        None => &mut diagram.columns,
    }
}

pub(super) fn push_item(diagram: &mut Diagram, stack: &[usize], item: Item) {
    match stack.last() {
        Some(index) => diagram.groups[*index].items.push(item),
        None => diagram.items.push(item),
    }
}

pub(super) fn header_rest(line: &str) -> Option<&str> {
    keyword(line, "block-beta").or_else(|| keyword(line, "block"))
}

pub(super) fn block_body(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    if bytes.len() >= 6 && line[..6].eq_ignore_ascii_case("block:") {
        Some(&line[6..])
    } else {
        None
    }
}

pub(super) fn unsupported(line: &str) -> bool {
    let Some(word) = line.split_whitespace().next() else {
        return false;
    };
    matches!(
        word.to_ascii_lowercase().as_str(),
        "direction" | "click" | "href" | "callback" | "subgraph"
    )
}

pub(super) fn open_group(
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

pub(super) fn add_nodes(
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

pub(super) fn add_edge(
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

pub(super) fn endpoints(
    side: &str,
    ids: &HashMap<String, End>,
    line: usize,
) -> Result<Vec<End>, Stop> {
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

pub(super) fn arrow_kind(arrow: &str) -> (bool, bool, bool, bool) {
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

pub(super) fn peel_label<'a>(left: &'a str, arrow: &str) -> Option<(&'a str, &'a str)> {
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

pub(super) fn apply_style(
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

pub(super) fn paint_color(value: &str, line: usize) -> Result<Color, Stop> {
    match crate::color::parse(value) {
        Ok(Some(color)) => Ok(color),
        _ => Err(fail(line, "a style")),
    }
}

pub(super) fn paint_width(value: &str, line: usize) -> Result<f32, Stop> {
    let value = value.trim().trim_end_matches("px").trim();
    let width = value.parse::<f32>().map_err(|_| fail(line, "a style"))?;
    if (0.25..=8.0).contains(&width) {
        Ok(width)
    } else {
        Err(fail(line, "a style"))
    }
}

pub(super) fn remember(
    ids: &mut HashMap<String, End>,
    id: &str,
    end: End,
    line: usize,
) -> Result<(), Stop> {
    if ids.contains_key(id) {
        return Err(fail(line, "an element"));
    }
    ids.insert(id.to_string(), end);
    Ok(())
}

pub(super) fn node_of(token: &str) -> Result<(&str, Shape, String), ()> {
    match shape_suffix(token) {
        Some((id, suffix)) => {
            let (shape, label) = shape_of(suffix)?;
            Ok((id, shape, label))
        }
        None => Ok((token, Shape::Rectangle, String::new())),
    }
}

pub(super) fn shape_suffix(token: &str) -> Option<(&str, &str)> {
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

pub(super) fn shape_of(suffix: &str) -> Result<(Shape, String), ()> {
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

pub(super) fn arrow_of(suffix: &str) -> Result<(Shape, String), ()> {
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

pub(super) fn inner(suffix: &str, sides: usize) -> String {
    inner_text(&suffix[sides..suffix.len() - sides])
}

pub(super) fn inner_text(value: &str) -> String {
    unquote(value.trim())
}

pub(super) fn split_span(token: &str) -> Result<(&str, usize), ()> {
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

pub(super) fn has_class(token: &str) -> bool {
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

pub(super) fn is_id(id: &str) -> bool {
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
