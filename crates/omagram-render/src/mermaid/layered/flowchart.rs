//! What a flowchart needs from its source beyond mmdr's graph.
//!
//! mmdr reads `((text))` and `(((text)))` both as a double circle, drops
//! the node after an invisible link (`A ~~~ B`), and reads `A@{ shape: rect }`
//! as a diamond named `A@`. This puts those back before the layout. An
//! unknown `shape`, an unknown icon `form`, or an `img:` (nothing is
//! fetched) declines the diagram, and mmdr draws it.

use std::collections::{HashMap, HashSet};

use mermaid_rs_renderer::ir::NodeStyle;
use mermaid_rs_renderer::{DiagramKind, Edge, EdgeStyle, Graph, NodeLink, NodeShape};

/// Edges that rank nodes and are not drawn, nodes written
/// `@{ shape: dbl-circ }`, and icons with no `form` (text, no box).
#[derive(Default)]
pub(super) struct Fix {
    pub invisible: HashSet<usize>,
    pub doubles: HashSet<String>,
    pub bare: HashSet<String>,
}

struct Decline;

/// `graph` with what mmdr misread put back, or `graph` itself when the
/// source needs nothing. `None` when a shape or an image here cannot be
/// drawn. `slot` keeps the repaired graph alive for the caller.
pub(super) fn apply<'a>(
    graph: &'a Graph,
    source: &str,
    slot: &'a mut Option<Graph>,
) -> Option<(&'a Graph, Fix)> {
    if graph.kind != DiagramKind::Flowchart || !needs_repair(source) {
        return Some((graph, Fix::default()));
    }
    let (fixed, fix) = repair(graph, source).ok()?;
    *slot = Some(fixed);
    Some((slot.as_ref()?, fix))
}

/// Parent of each subgraph as the source nests it. Empty subgraphs have
/// no node set to nest them by, so the layout uses this when the length
/// matches the graph. A mismatch leaves them at the root.
pub(super) fn empty_nesting(source: &str) -> Vec<Option<usize>> {
    let mut parent = Vec::new();
    let mut stack = Vec::new();
    for statement in statements(source) {
        if statement == "end" {
            stack.pop();
            continue;
        }
        if subgraph_body(&statement).is_some() {
            parent.push(stack.last().copied());
            stack.push(parent.len() - 1);
        }
    }
    parent
}

fn needs_repair(source: &str) -> bool {
    statements(source)
        .iter()
        .any(|line| has_at(line) || has_invisible(line))
}

fn repair(graph: &Graph, source: &str) -> Result<(Graph, Fix), Decline> {
    let mut copy = graph.clone();
    let mut fix = Fix::default();
    let mut stack = Vec::new();
    let mut seen = 0usize;
    for statement in statements(source) {
        if statement == "end" {
            stack.pop();
            continue;
        }
        if let Some(rest) = subgraph_body(&statement) {
            if has_at(rest) {
                apply_subgraph(&mut copy, seen, rest, &mut fix)?;
            }
            stack.push(seen);
            seen += 1;
            continue;
        }
        if has_at(&statement) || has_invisible(&statement) {
            apply_statement(&mut copy, &statement, &stack, &mut fix)?;
        }
    }
    Ok((copy, fix))
}

fn apply_statement(
    graph: &mut Graph,
    line: &str,
    stack: &[usize],
    fix: &mut Fix,
) -> Result<(), Decline> {
    let pieces = split_top(line, tilde_sep);
    if pieces.is_empty() {
        return Err(Decline);
    }
    let mut parsed = Vec::with_capacity(pieces.len());
    for piece in &pieces {
        parsed.push(parse_piece(piece)?);
    }
    if let Some(id) = unread_id(line)
        && !parsed.iter().any(|piece| piece.holds(&id))
    {
        drop_unread(graph, &id);
    }
    for piece in &parsed {
        for spec in &piece.all {
            apply_spec(graph, spec, stack, fix)?;
        }
    }
    if parsed.len() < 2 {
        return Ok(());
    }
    for pair in parsed.windows(2) {
        for left in &pair[0].right {
            for right in &pair[1].left {
                fix.invisible
                    .insert(push_invisible(graph, &left.id, &right.id));
            }
        }
    }
    Ok(())
}

fn apply_subgraph(
    graph: &mut Graph,
    index: usize,
    rest: &str,
    _fix: &mut Fix,
) -> Result<(), Decline> {
    let spec = parse_spec(rest)?;
    // A cluster stays a frame. An unknown shape, form or image still
    // declines, so the diagram is not drawn under the name mmdr stored.
    if spec.at.is_some() {
        let _ = node_shape(&spec, &mut Fix::default())?;
    }
    if let Some(from) = &spec.from {
        rename_node(graph, from, &spec.id);
    }
    let sub = graph.subgraphs.get_mut(index).ok_or(Decline)?;
    if sub.id.as_deref() != Some(spec.id.as_str()) {
        sub.id = Some(spec.id.clone());
    }
    if spec.at.is_some() {
        sub.label = display_label(&spec);
    }
    let id = spec.id.clone();
    push_classes(&mut graph.subgraph_classes, &id, &spec.classes);
    Ok(())
}

/// The id mmdr keeps when `A[text] ~~~ B` is not an edge: the first word.
/// A real id parsed from that line is not unread.
fn unread_id(line: &str) -> Option<String> {
    if !has_invisible(line) || find_top(line, |rest| arrow_sep(rest).is_some()).is_some() {
        return None;
    }
    let token = line.split_whitespace().next()?;
    let (body, _) = split_classes(token);
    Some(body.split_whitespace().next()?.to_string())
}

fn drop_unread(graph: &mut Graph, id: &str) {
    if graph
        .edges
        .iter()
        .any(|edge| edge.from == id || edge.to == id)
        || graph
            .subgraphs
            .iter()
            .any(|sub| sub.id.as_deref() == Some(id))
    {
        return;
    }
    graph.nodes.remove(id);
    graph.node_order.remove(id);
    graph.node_classes.remove(id);
    graph.node_styles.remove(id);
    graph.node_links.remove(id);
    for sub in &mut graph.subgraphs {
        sub.nodes.retain(|node| node != id);
    }
}

fn apply_spec(
    graph: &mut Graph,
    spec: &Spec,
    stack: &[usize],
    fix: &mut Fix,
) -> Result<(), Decline> {
    let shape = node_shape(spec, fix)?;
    if let Some(from) = &spec.from {
        rename_node(graph, from, &spec.id);
    }
    let label = if spec.at.is_some() {
        Some(display_label(spec))
    } else {
        spec.shorthand_label.clone()
    };
    graph.ensure_node(&spec.id, label, shape);
    push_classes(&mut graph.node_classes, &spec.id, &spec.classes);
    add_to_stack(graph, stack, &spec.id);
    Ok(())
}

fn node_shape(spec: &Spec, fix: &mut Fix) -> Result<Option<NodeShape>, Decline> {
    let Some(at) = &spec.at else {
        return Ok(spec.shorthand_shape);
    };
    if at.image {
        return Err(Decline);
    }
    if let Some(name) = &at.shape {
        let shape = map_shape(name).ok_or(Decline)?;
        if shape == NodeShape::DoubleCircle {
            fix.doubles.insert(spec.id.clone());
        }
        return Ok(Some(shape));
    }
    if at.icon.is_some() {
        if let Some(form) = &at.form {
            return Ok(Some(map_form(form)?));
        }
        fix.bare.insert(spec.id.clone());
        return Ok(Some(NodeShape::Rectangle));
    }
    // `@{ label: … }` still replaces the diamond mmdr stored.
    Ok(Some(spec.shorthand_shape.unwrap_or(NodeShape::Rectangle)))
}

fn display_label(spec: &Spec) -> String {
    let user = spec
        .at
        .as_ref()
        .and_then(|at| at.label.clone())
        .or_else(|| spec.shorthand_label.clone());
    let Some(icon) = spec.at.as_ref().and_then(|at| at.icon.as_deref()) else {
        return user.unwrap_or_else(|| spec.id.clone());
    };
    let short = short_icon(icon);
    if short.is_empty() {
        return user.unwrap_or_else(|| spec.id.clone());
    }
    match user {
        Some(text) if spec.at.as_ref().is_some_and(|at| at.label_above) => {
            format!("{text}\n{short}")
        }
        Some(text) => format!("{short}\n{text}"),
        None => short.to_string(),
    }
}

/// The part after the last `:`, so `fa:user` is `user`. No icon font is
/// shipped, and none is fetched.
fn short_icon(name: &str) -> &str {
    let name = name.trim();
    let short = name.rsplit(':').next().unwrap_or(name).trim();
    if short.is_empty() { name } else { short }
}

fn map_shape(name: &str) -> Option<NodeShape> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "rect" | "proc" | "process" | "rectangle" => NodeShape::Rectangle,
        "rounded" | "event" => NodeShape::RoundRect,
        "stadium" | "terminal" | "pill" => NodeShape::Stadium,
        "fr-rect" | "subprocess" | "subproc" | "framed-rectangle" | "subroutine" => {
            NodeShape::Subroutine
        }
        "cyl" | "db" | "database" | "cylinder" => NodeShape::Cylinder,
        "circle" => NodeShape::Circle,
        "dbl-circ" | "double-circle" | "doublecircle" => NodeShape::DoubleCircle,
        "diam" | "decision" | "diamond" | "question" => NodeShape::Diamond,
        "hex" | "hexagon" | "prepare" => NodeShape::Hexagon,
        "lean-r" | "lean-right" | "in-out" => NodeShape::Parallelogram,
        "lean-l" | "lean-left" | "out-in" => NodeShape::ParallelogramAlt,
        "trap-b" | "trapezoid" | "priority" => NodeShape::Trapezoid,
        "trap-t" | "trapezoid-alt" | "inverted-trapezoid" => NodeShape::TrapezoidAlt,
        "odd" | "flag" | "asymmetric" => NodeShape::Asymmetric,
        _ => return None,
    })
}

fn map_form(name: &str) -> Result<NodeShape, Decline> {
    match name.trim().to_ascii_lowercase().as_str() {
        "square" => Ok(NodeShape::Rectangle),
        "circle" => Ok(NodeShape::Circle),
        "rounded" => Ok(NodeShape::RoundRect),
        _ => Err(Decline),
    }
}

fn push_invisible(graph: &mut Graph, from: &str, to: &str) -> usize {
    graph.edges.push(Edge {
        from: from.to_string(),
        to: to.to_string(),
        label: None,
        start_label: None,
        end_label: None,
        directed: false,
        arrow_start: false,
        arrow_end: false,
        arrow_start_kind: None,
        arrow_end_kind: None,
        start_decoration: None,
        end_decoration: None,
        style: EdgeStyle::Solid,
    });
    graph.edges.len() - 1
}

fn rename_node(graph: &mut Graph, from: &str, to: &str) {
    if from == to {
        return;
    }
    if let Some(mut node) = graph.nodes.remove(from) {
        node.id = to.to_string();
        if !graph.nodes.contains_key(to) {
            graph.nodes.insert(to.to_string(), node);
        }
    }
    if let Some(order) = graph.node_order.remove(from) {
        graph.node_order.entry(to.to_string()).or_insert(order);
    }
    for edge in &mut graph.edges {
        if edge.from == from {
            edge.from = to.to_string();
        }
        if edge.to == from {
            edge.to = to.to_string();
        }
    }
    for sub in &mut graph.subgraphs {
        if sub.id.as_deref() == Some(from) {
            sub.id = Some(to.to_string());
        }
        for id in &mut sub.nodes {
            if id == from {
                *id = to.to_string();
            }
        }
        let mut seen = HashSet::new();
        sub.nodes.retain(|id| seen.insert(id.clone()));
    }
    if let Some(classes) = graph.node_classes.remove(from) {
        push_classes(&mut graph.node_classes, to, &classes);
    }
    move_style(&mut graph.node_styles, from, to);
    move_link(&mut graph.node_links, from, to);
    if let Some(classes) = graph.subgraph_classes.remove(from) {
        push_classes(&mut graph.subgraph_classes, to, &classes);
    }
    move_style(&mut graph.subgraph_styles, from, to);
}

fn push_classes(map: &mut HashMap<String, Vec<String>>, id: &str, classes: &[String]) {
    if classes.is_empty() {
        return;
    }
    let entry = map.entry(id.to_string()).or_default();
    for class in classes {
        if !entry.contains(class) {
            entry.push(class.clone());
        }
    }
}

fn move_style(map: &mut HashMap<String, NodeStyle>, from: &str, to: &str) {
    if let Some(style) = map.remove(from) {
        map.entry(to.to_string()).or_insert(style);
    }
}

fn move_link(map: &mut HashMap<String, NodeLink>, from: &str, to: &str) {
    if let Some(link) = map.remove(from) {
        map.entry(to.to_string()).or_insert(link);
    }
}

fn add_to_stack(graph: &mut Graph, stack: &[usize], id: &str) {
    if stack.is_empty() {
        return;
    }
    let claimed = graph
        .subgraphs
        .iter()
        .enumerate()
        .any(|(index, sub)| !stack.contains(&index) && sub.nodes.iter().any(|node| node == id));
    if claimed {
        return;
    }
    for index in stack {
        if let Some(sub) = graph.subgraphs.get_mut(*index)
            && !sub.nodes.contains(&id.to_string())
        {
            sub.nodes.push(id.to_string());
        }
    }
}

#[derive(Clone)]
struct Spec {
    id: String,
    /// The id mmdr stored, `A@` or `A[text]@`, when the token has `@{ }`.
    from: Option<String>,
    shorthand_label: Option<String>,
    shorthand_shape: Option<NodeShape>,
    classes: Vec<String>,
    at: Option<AtMeta>,
}

#[derive(Clone, Default)]
struct AtMeta {
    shape: Option<String>,
    label: Option<String>,
    icon: Option<String>,
    form: Option<String>,
    /// `pos: t` draws the label above the icon name.
    label_above: bool,
    image: bool,
}

struct Piece {
    left: Vec<Spec>,
    right: Vec<Spec>,
    all: Vec<Spec>,
}

impl Piece {
    fn holds(&self, id: &str) -> bool {
        self.all.iter().any(|spec| spec.id == id)
    }
}

fn parse_piece(piece: &str) -> Result<Piece, Decline> {
    let segments = split_top(piece, arrow_sep);
    if segments.is_empty() {
        return Err(Decline);
    }
    let mut all = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let face = index == 0 || index + 1 == segments.len();
        if face || interesting(segment) {
            all.extend(parse_nodes(segment)?);
        }
    }
    let left = parse_nodes(&segments[0])?;
    let right = if segments.len() == 1 {
        left.clone()
    } else {
        parse_nodes(segments.last().unwrap())?
    };
    Ok(Piece { left, right, all })
}

/// A word between two arrows is a label (`A -- yes --> B`), not a node.
fn interesting(segment: &str) -> bool {
    has_at(segment)
        || segment.contains('[')
        || segment.contains('(')
        || segment.contains('{')
        || segment.contains('>')
        || segment.contains('&')
}

fn parse_nodes(segment: &str) -> Result<Vec<Spec>, Decline> {
    let parts = split_top(segment, |rest| rest.starts_with('&').then_some(1));
    if parts.is_empty() {
        return Err(Decline);
    }
    parts.iter().map(|part| parse_spec(part)).collect()
}

fn parse_spec(token: &str) -> Result<Spec, Decline> {
    let (body, classes) = split_classes(token.trim());
    let body = body.trim();
    if body.is_empty() {
        return Err(Decline);
    }
    let mut id_end = 0;
    for ch in body.chars() {
        if is_id_char(ch) {
            id_end += ch.len_utf8();
        } else {
            break;
        }
    }
    if id_end == 0 {
        return Err(Decline);
    }
    let id = body[..id_end].to_string();
    let mut rest = &body[id_end..];
    let mut shorthand_label = None;
    let mut shorthand_shape = None;
    if rest.starts_with(['[', '(', '{', '>']) {
        let (group, after) = take_group(rest).ok_or(Decline)?;
        let (label, shape) = shorthand(group).ok_or(Decline)?;
        shorthand_label = Some(label);
        shorthand_shape = Some(shape);
        rest = after;
    }
    rest = rest.trim_start();
    let mut at = None;
    let mut from = None;
    if let Some(after_at) = rest.strip_prefix('@') {
        let (group, trailing) = take_group(after_at).ok_or(Decline)?;
        if !group.starts_with('{') || !trailing.trim().is_empty() {
            return Err(Decline);
        }
        at = Some(properties(&group[1..group.len() - 1])?);
        from = at_brace(body).map(|brace| body[..brace].trim().to_string());
    } else if !rest.is_empty() {
        return Err(Decline);
    }
    Ok(Spec {
        id,
        from,
        shorthand_label,
        shorthand_shape,
        classes,
        at,
    })
}

fn properties(body: &str) -> Result<AtMeta, Decline> {
    let mut meta = AtMeta::default();
    for part in split_top(body, |rest| rest.starts_with(',').then_some(1)) {
        let Some((key, value)) = part.split_once(':') else {
            continue;
        };
        let value = unquote(value.trim());
        match key.trim().to_ascii_lowercase().as_str() {
            "shape" => meta.shape = Some(value),
            "label" => meta.label = Some(value),
            "icon" => meta.icon = Some(value),
            "form" => meta.form = Some(value),
            "pos" => {
                meta.label_above = match value.to_ascii_lowercase().as_str() {
                    "t" => true,
                    "b" => false,
                    _ => return Err(Decline),
                };
            }
            "img" | "image" => meta.image = true,
            _ => {}
        }
    }
    Ok(meta)
}

fn shorthand(group: &str) -> Option<(String, NodeShape)> {
    let pair = |prefix: &str, suffix: &str, shape: NodeShape| {
        group
            .strip_prefix(prefix)
            .and_then(|inner| inner.strip_suffix(suffix))
            .map(|inner| (unquote(inner.trim()), shape))
    };
    pair("[[", "]]", NodeShape::Subroutine)
        .or_else(|| pair("[(", ")]", NodeShape::Cylinder))
        .or_else(|| pair("[/", "/]", NodeShape::Parallelogram))
        .or_else(|| pair("[\\", "\\]", NodeShape::ParallelogramAlt))
        .or_else(|| pair("[/", "\\]", NodeShape::Trapezoid))
        .or_else(|| pair("[\\", "/]", NodeShape::TrapezoidAlt))
        .or_else(|| {
            group
                .strip_prefix('[')
                .and_then(|inner| inner.strip_suffix(']'))
                .map(|inner| {
                    if let Some(label) = inner.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
                        (unquote(label.trim()), NodeShape::Stadium)
                    } else {
                        (unquote(inner.trim()), NodeShape::Rectangle)
                    }
                })
        })
        .or_else(|| pair("(((", ")))", NodeShape::DoubleCircle))
        .or_else(|| pair("((", "))", NodeShape::DoubleCircle))
        .or_else(|| {
            group
                .strip_prefix('(')
                .and_then(|inner| inner.strip_suffix(')'))
                .map(|inner| {
                    if let Some(label) = inner.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                        (unquote(label.trim()), NodeShape::Stadium)
                    } else {
                        (unquote(inner.trim()), NodeShape::RoundRect)
                    }
                })
        })
        .or_else(|| pair("{{", "}}", NodeShape::Hexagon))
        .or_else(|| pair("{", "}", NodeShape::Diamond))
        .or_else(|| pair(">", "]", NodeShape::Asymmetric))
}

fn is_id_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-'
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let quote = bytes[0];
        if (quote == b'"' || quote == b'\'') && bytes[bytes.len() - 1] == quote {
            return unescape(&value[1..value.len() - 1]);
        }
    }
    value.to_string()
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn take_group(text: &str) -> Option<(&str, &str)> {
    let mut chars = text.char_indices();
    let (_, open) = chars.next()?;
    let close = match open {
        '[' => ']',
        '(' => ')',
        '{' => '}',
        '>' => ']',
        _ => return None,
    };
    let mut depth = if open == '>' { 0 } else { 1 };
    let mut quote: Option<char> = None;
    let mut escape = false;
    for (index, ch) in chars {
        if escape {
            escape = false;
            continue;
        }
        if quote.is_some() && ch == '\\' {
            escape = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if open != '>' && ch == open {
            depth += 1;
        } else if ch == close {
            if open == '>' || depth == 1 {
                let end = index + ch.len_utf8();
                return Some((&text[..end], &text[end..]));
            }
            depth -= 1;
        }
    }
    None
}

fn subgraph_body(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("subgraph")?;
    (rest.is_empty() || rest.starts_with(char::is_whitespace)).then_some(rest.trim())
}

fn statements(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut front = false;
    for raw in source.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed == "---" {
            front = !front;
            continue;
        }
        if front || trimmed.starts_with("%%") {
            continue;
        }
        let code = cut_comment(trimmed);
        let code = code.trim();
        if code.is_empty() {
            continue;
        }
        out.extend(split_top(code, |rest| rest.starts_with(';').then_some(1)));
    }
    out
}

fn cut_comment(line: &str) -> &str {
    find_top(line, |rest| rest.starts_with("%%")).map_or(line, |at| &line[..at])
}

fn has_at(text: &str) -> bool {
    find_top(text, |rest| rest.starts_with("@{")).is_some()
}

fn has_invisible(text: &str) -> bool {
    find_top(text, |rest| tilde_sep(rest).is_some()).is_some()
}

fn at_brace(text: &str) -> Option<usize> {
    // The `{` of `@{`, which is the byte after `@`.
    find_top(text, |rest| rest.starts_with("@{")).map(|at| at + 1)
}

fn tilde_sep(rest: &str) -> Option<usize> {
    let count = rest.chars().take_while(|ch| *ch == '~').count();
    (count >= 3).then_some(count)
}

fn arrow_sep(rest: &str) -> Option<usize> {
    let mut len = 0;
    for ch in rest.chars() {
        if !matches!(ch, '<' | '-' | '.' | '=' | 'o' | 'x' | '>') {
            break;
        }
        len += ch.len_utf8();
    }
    if len == 0 {
        return None;
    }
    let run = &rest[..len];
    let dashes = run.chars().filter(|ch| *ch == '-' || *ch == '=').count();
    let marked = run
        .chars()
        .any(|ch| matches!(ch, '<' | '>' | '.' | 'o' | 'x'));
    (dashes >= 2 || (dashes >= 1 && marked)).then_some(len)
}

fn split_classes(token: &str) -> (String, Vec<String>) {
    let mut parts = split_top(token, |rest| rest.starts_with(":::").then_some(3));
    if parts.is_empty() {
        return (String::new(), Vec::new());
    }
    let body = parts.remove(0);
    (body, parts)
}

fn split_top(text: &str, sep: impl Fn(&str) -> Option<usize>) -> Vec<String> {
    let mut parts = Vec::new();
    let mut source = text;
    loop {
        let Some(at) = find_top(source, |rest| sep(rest).is_some()) else {
            let part = source.trim();
            if !part.is_empty() {
                parts.push(part.to_string());
            }
            break;
        };
        let part = source[..at].trim();
        if !part.is_empty() {
            parts.push(part.to_string());
        }
        let len = sep(&source[at..]).unwrap_or(0);
        if len == 0 {
            break;
        }
        source = &source[at + len..];
    }
    parts
}

/// Index of the first character where `pred` holds outside quotes and
/// brackets. `{` `(` `[` open a group, so a `~~~` inside a label is not a link.
fn find_top(text: &str, pred: impl Fn(&str) -> bool) -> Option<usize> {
    let mut index = 0;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escape = false;
    while index < text.len() {
        let rest = &text[index..];
        let ch = rest.chars().next()?;
        let len = ch.len_utf8();
        if escape {
            escape = false;
            index += len;
            continue;
        }
        if quote.is_some() && ch == '\\' {
            escape = true;
            index += len;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            index += len;
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            index += len;
            continue;
        }
        if depth == 0 && pred(rest) {
            return Some(index);
        }
        if matches!(ch, '[' | '(' | '{') {
            depth += 1;
        } else if matches!(ch, ']' | ')' | '}') && depth > 0 {
            depth -= 1;
        }
        index += len;
    }
    None
}

/// The ids of the nodes written as a double circle, `id(((text)))`.
///
/// A comment, a quoted string, a square-bracket label and a `|label|`
/// on an edge are skipped, so `(((…)))` written there does not change
/// another node's shape.
pub(super) fn double_circles(source: &str) -> HashSet<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut ids = HashSet::new();
    let mut index = 0;
    let mut brackets = 0i32;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '%' && chars.get(index + 1) == Some(&'%') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            index = skip_quoted(&chars, index);
            continue;
        }
        if ch == '[' {
            brackets += 1;
            index += 1;
            continue;
        }
        if ch == ']' {
            brackets = (brackets - 1).max(0);
            index += 1;
            continue;
        }
        if ch == '|' && brackets == 0 {
            index = skip_edge_label(&chars, index + 1);
            continue;
        }
        if brackets == 0
            && ch == '('
            && chars.get(index + 1) == Some(&'(')
            && chars.get(index + 2) == Some(&'(')
        {
            if let Some(id) = id_before(&chars, index) {
                ids.insert(id);
            }
            index += 3;
            continue;
        }
        index += 1;
    }
    ids
}

fn skip_quoted(chars: &[char], start: usize) -> usize {
    let quote = chars[start];
    let mut index = start + 1;
    while index < chars.len() {
        if chars[index] == '\\' {
            index += 2;
            continue;
        }
        if chars[index] == quote || chars[index] == '\n' {
            break;
        }
        index += 1;
    }
    if index < chars.len() {
        index + 1
    } else {
        index
    }
}

fn skip_edge_label(chars: &[char], mut index: usize) -> usize {
    while index < chars.len() && chars[index] != '\n' {
        if chars[index] == '"' || chars[index] == '\'' {
            index = skip_quoted(chars, index);
            continue;
        }
        if chars[index] == '|' {
            return index + 1;
        }
        index += 1;
    }
    index
}

/// The id written immediately before `(((` . `A---my-node(((x)))` keeps
/// `my-node`: the id starts after the link.
fn id_before(chars: &[char], end: usize) -> Option<String> {
    let mut start = end;
    while start > 0 {
        let ch = chars[start - 1];
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            start -= 1;
        } else {
            break;
        }
    }
    if start == end {
        return None;
    }
    let run: String = chars[start..end].iter().collect();
    let id = run
        .rfind("--")
        .map_or(run.as_str(), |at| &run[at + 2..])
        .trim_start_matches('-');
    (!id.is_empty()).then(|| id.to_owned())
}
