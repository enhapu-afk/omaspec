//! Reading a Mermaid architecture diagram: its groups, services and
//! junctions, and the edges with the sides they leave and reach.

use super::{Diagram, Edge, Group, Icon, Look, Node, Side};
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

pub(super) fn read(source: &str) -> Result<Diagram, Stop> {
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
pub(super) enum Kind {
    Group,
    Node(usize),
}

pub(super) fn push_group(
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

pub(super) fn push_service(
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

pub(super) fn push_junction(
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

pub(super) fn push_edge(
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

pub(super) fn node_id(ids: &HashMap<String, Kind>, id: &str, line: usize) -> Result<usize, Stop> {
    match ids.get(id) {
        Some(Kind::Node(index)) => Ok(*index),
        Some(Kind::Group) => Err(fail(line, "a service")),
        None => Err(fail(line, "a service")),
    }
}

pub(super) fn end_left(token: &str, line: usize) -> Result<(String, Side, bool), Stop> {
    let (id, port) = token
        .trim()
        .split_once(':')
        .ok_or_else(|| fail(line, "a port"))?;
    let (id, group) = take_group(id.trim(), line)?;
    Ok((id, side_of(port, line)?, group))
}

pub(super) fn end_right(token: &str, line: usize) -> Result<(String, Side, bool), Stop> {
    let (port, id) = token
        .trim()
        .split_once(':')
        .ok_or_else(|| fail(line, "a port"))?;
    let (id, group) = take_group(id.trim(), line)?;
    Ok((id, side_of(port, line)?, group))
}

pub(super) fn take_group(token: &str, line: usize) -> Result<(String, bool), Stop> {
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

pub(super) fn side_of(token: &str, line: usize) -> Result<Side, Stop> {
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

pub(super) fn parent_group(
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

pub(super) fn declaration(body: &str, line: usize) -> Result<(String, Icon, String), Stop> {
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

pub(super) fn icon_of(name: &str, line: usize) -> Result<Icon, Stop> {
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

pub(super) fn split_in(rest: &str) -> (&str, Option<&str>) {
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

pub(super) fn unquote(value: &str) -> String {
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

pub(super) fn header_word(line: &str) -> bool {
    let Some(word) = line.split_whitespace().next() else {
        return false;
    };
    word.eq_ignore_ascii_case("architecture-beta") || word.eq_ignore_ascii_case("architecture")
}

pub(super) fn strip_comment(line: &str) -> &str {
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
