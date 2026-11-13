//! Reading a Mermaid C4 diagram: its title, its boundaries and the shapes
//! and relationships in them, their styles and the layout's settings.

use super::args::{
    Args, access_head, closes_block, get, non_empty, norm_name, normalize_key, opens_block,
    optional, parse_args, split_args, split_brace, split_trailing_close, strip_comment, unescape,
};
use super::lay::Mark;
use super::{Boundary, Diagram, HEADS, Kind, Look, Rel, RelKind, Shape};
use crate::color::{self, Color};
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
pub(super) enum Owner {
    Shape(usize),
    Boundary(usize),
}

pub(super) struct Frame {
    pub(super) index: usize,
    pub(super) line: usize,
}

pub(super) struct Chart {
    pub(super) diagram: Diagram,
    pub(super) ids: HashMap<String, Owner>,
    pub(super) stack: Vec<Frame>,
    pub(super) saw_header: bool,
    pub(super) skipping: bool,
}

pub(super) fn read(source: &str) -> Result<Diagram, Stop> {
    if !source.lines().any(|raw| {
        strip_comment(raw)
            .split_whitespace()
            .next()
            .is_some_and(|word| word.to_ascii_lowercase().starts_with("c4"))
    }) {
        return Err(Stop::Foreign);
    }
    let mut chart = Chart {
        diagram: Diagram {
            title: None,
            shapes: Vec::new(),
            boundaries: Vec::new(),
            rels: Vec::new(),
            shapes_per_row: 4,
            boundaries_per_row: 2,
            look: Look::default(),
        },
        ids: HashMap::new(),
        stack: Vec::new(),
        saw_header: false,
        skipping: false,
    };
    let front =
        crate::source::front_lines(source).map_err(|no| fail(no, "a front matter setting"))?;
    for (no, raw) in source.lines().enumerate().skip(front) {
        one_line(&mut chart, no, raw)?;
    }
    if let Some(frame) = chart.stack.last() {
        return Err(fail(frame.line, "a boundary"));
    }
    if !chart.saw_header {
        return Err(Stop::Foreign);
    }
    Ok(chart.diagram)
}

pub(super) fn one_line(chart: &mut Chart, no: usize, raw: &str) -> Result<(), Stop> {
    let line = strip_comment(raw);
    if chart.skipping {
        if closes_block(line) {
            chart.skipping = false;
        }
        return Ok(());
    }
    if line.is_empty() {
        return Ok(());
    }
    if access_head(line) {
        chart.skipping = opens_block(line) && !closes_block(line);
        return Ok(());
    }
    if let Some(rest) = keyword(line, "title") {
        return set_title(chart, no, rest);
    }
    if !chart.saw_header {
        if take_header(line, no)? {
            chart.saw_header = true;
            return Ok(());
        }
        return Err(fail(no, "an unknown line"));
    }
    if take_header(line, no)? {
        return Err(fail(no, "an unknown line"));
    }
    body_line(chart, no, line)
}

pub(super) fn set_title(chart: &mut Chart, no: usize, rest: &str) -> Result<(), Stop> {
    if rest.is_empty() || chart.diagram.title.is_some() {
        return Err(fail(no, "a title"));
    }
    chart.diagram.title = Some(text_at(&unescape(rest), no));
    Ok(())
}

pub(super) fn take_header(line: &str, no: usize) -> Result<bool, Stop> {
    let Some(word) = line.split_whitespace().next() else {
        return Ok(false);
    };
    if !word.to_ascii_lowercase().starts_with("c4") {
        return Ok(false);
    }
    for name in HEADS {
        if let Some(rest) = keyword(line, name) {
            return if rest.is_empty() {
                Ok(true)
            } else {
                Err(fail(no, "an unknown line"))
            };
        }
    }
    Err(fail(no, "an unknown line"))
}

pub(super) fn body_line(chart: &mut Chart, no: usize, line: &str) -> Result<(), Stop> {
    if line == "}" || line.eq_ignore_ascii_case("end") {
        return pop(chart, no);
    }
    if let Some((before, after)) = split_brace(line) {
        if before.is_empty() {
            return Err(fail(no, "an unknown line"));
        }
        statement(chart, no, before)?;
        let (inner, closes) = split_trailing_close(after);
        if !inner.is_empty() {
            body_line(chart, no, inner)?;
        }
        if closes {
            pop(chart, no)?;
        }
        return Ok(());
    }
    let (body, closes) = split_trailing_close(line);
    if body.is_empty() {
        return if closes {
            pop(chart, no)
        } else {
            Err(fail(no, "an unknown line"))
        };
    }
    statement(chart, no, body)?;
    if closes {
        pop(chart, no)?;
    }
    Ok(())
}

pub(super) fn statement(chart: &mut Chart, no: usize, line: &str) -> Result<(), Stop> {
    let Some((name, args)) = function_call(line, no)? else {
        return Err(fail(no, "an unknown line"));
    };
    apply(chart, no, &name, &args)
}

pub(super) fn function_call(line: &str, no: usize) -> Result<Option<(String, Vec<String>)>, Stop> {
    let trimmed = line.trim();
    let Some(open) = trimmed.find('(') else {
        return Ok(None);
    };
    let Some(close) = trimmed.rfind(')') else {
        return Err(fail(no, "an element"));
    };
    if close < open {
        return Err(fail(no, "an element"));
    }
    let func = trimmed[..open].trim();
    if func.is_empty() || func.contains([' ', '{', '}']) {
        return Err(fail(no, "an element"));
    }
    if !trimmed[close + 1..].trim().is_empty() {
        return Err(fail(no, "an unknown line"));
    }
    Ok(Some((
        func.to_string(),
        split_args(&trimmed[open + 1..close]),
    )))
}

pub(super) fn apply(
    chart: &mut Chart,
    no: usize,
    name: &str,
    raw_args: &[String],
) -> Result<(), Stop> {
    let name = norm_name(name);
    let args = parse_args(raw_args);
    if is_boundary_name(&name) {
        return apply_boundary(chart, no, &name, &args);
    }
    if let Some(kind) = rel_kind(&name) {
        return apply_rel(chart, no, kind, &name, &args);
    }
    if is_element_style(&name) {
        return apply_element_style(chart, no, &args);
    }
    if is_rel_style(&name) {
        return apply_rel_style(chart, no, &args);
    }
    if is_layout_config(&name) {
        return apply_layout(chart, no, &args);
    }
    if let Some(kind) = shape_kind(&name) {
        return apply_shape(chart, no, kind, &args);
    }
    Err(fail(no, "an element"))
}

pub(super) fn apply_boundary(
    chart: &mut Chart,
    no: usize,
    name: &str,
    args: &Args,
) -> Result<(), Stop> {
    only_known(
        args,
        &["type", "descr", "description", "sprite", "tags", "link"],
        no,
    )?;
    refuse_sprite(args, None, no)?;
    let Some(id) = args.positional.first() else {
        return Err(fail(no, "a boundary"));
    };
    if id.is_empty() {
        return Err(fail(no, "a boundary"));
    }
    claim(chart, id, no)?;
    let label = args
        .positional
        .get(1)
        .map(|text| unescape(text))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| id.clone());
    let kind_name = get(args, "type")
        .map(unescape)
        .filter(|text| !text.is_empty())
        .or_else(|| args.positional.get(2).map(|text| unescape(text)))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| boundary_type(name).to_string());
    let index = chart.diagram.boundaries.len();
    chart.diagram.boundaries.push(Boundary {
        id: id.clone(),
        label,
        kind_name,
        descr: optional(get(args, "descr").or_else(|| get(args, "description"))),
        parent: chart.stack.last().map(|frame| frame.index),
        fill: None,
        border: None,
        ink: None,
        line: no,
    });
    chart.ids.insert(id.clone(), Owner::Boundary(index));
    push(chart, index, no)
}

pub(super) fn apply_shape(
    chart: &mut Chart,
    no: usize,
    kind: Kind,
    args: &Args,
) -> Result<(), Stop> {
    only_known(
        args,
        &[
            "type",
            "techn",
            "technology",
            "descr",
            "description",
            "sprite",
            "tags",
            "link",
        ],
        no,
    )?;
    refuse_sprite(args, Some(4), no)?;
    let Some(id) = args.positional.first() else {
        return Err(fail(no, "an element"));
    };
    if id.is_empty() {
        return Err(fail(no, "an element"));
    }
    claim(chart, id, no)?;
    let label = args
        .positional
        .get(1)
        .map(|text| unescape(text))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| id.clone());
    let (type_label, techn) = if kind.uses_techn() {
        (
            optional(get(args, "type")),
            optional(get(args, "techn").or_else(|| get(args, "technology")))
                .or_else(|| args.positional.get(2).map(|text| unescape(text))),
        )
    } else {
        (
            optional(get(args, "type"))
                .or_else(|| args.positional.get(2).map(|text| unescape(text))),
            optional(get(args, "techn").or_else(|| get(args, "technology"))),
        )
    };
    let descr = optional(get(args, "descr").or_else(|| get(args, "description")))
        .or_else(|| args.positional.get(3).map(|text| unescape(text)));
    let index = chart.diagram.shapes.len();
    chart.diagram.shapes.push(Shape {
        id: id.clone(),
        label,
        kind,
        type_label: non_empty(type_label),
        techn: non_empty(techn),
        descr: non_empty(descr),
        parent: chart.stack.last().map(|frame| frame.index),
        fill: None,
        border: None,
        ink: None,
        line: no,
    });
    chart.ids.insert(id.clone(), Owner::Shape(index));
    Ok(())
}

pub(super) fn apply_rel(
    chart: &mut Chart,
    no: usize,
    mut kind: RelKind,
    name: &str,
    args: &Args,
) -> Result<(), Stop> {
    only_known(
        args,
        &[
            "techn",
            "technology",
            "descr",
            "description",
            "sprite",
            "tags",
            "link",
        ],
        no,
    )?;
    let mut positional = args.positional.clone();
    if name.starts_with("relindex") && positional.len() > 1 {
        positional.remove(0);
    }
    if sprite_value(args, positional.get(5).map(String::as_str)) {
        return Err(fail(no, "a sprite"));
    }
    if positional.len() < 3 {
        return Err(fail(no, "a relationship"));
    }
    let from = shape_index(chart, &positional[0], no)?;
    let to = shape_index(chart, &positional[1], no)?;
    let label = unescape(&positional[2]);
    if label.is_empty() {
        return Err(fail(no, "a relationship"));
    }
    if name.starts_with("relindex") {
        kind = RelKind::Rel;
    }
    chart.diagram.rels.push(Rel {
        kind,
        from,
        to,
        label,
        techn: non_empty(
            optional(get(args, "techn").or_else(|| get(args, "technology")))
                .or_else(|| positional.get(3).map(|text| unescape(text))),
        ),
        descr: non_empty(
            optional(get(args, "descr").or_else(|| get(args, "description")))
                .or_else(|| positional.get(4).map(|text| unescape(text))),
        ),
        offset_x: 0.0,
        offset_y: 0.0,
        line_color: None,
        text_color: None,
        line: no,
    });
    Ok(())
}

pub(super) fn apply_element_style(chart: &mut Chart, no: usize, args: &Args) -> Result<(), Stop> {
    only_known(
        args,
        &[
            "element",
            "bgcolor",
            "fontcolor",
            "bordercolor",
            "sprite",
            "techn",
            "technology",
        ],
        no,
    )?;
    refuse_sprite(args, Some(6), no)?;
    let Some(id) = args
        .positional
        .first()
        .map(String::as_str)
        .or_else(|| get(args, "element"))
    else {
        return Err(fail(no, "an element"));
    };
    let fill = optional_color(args, 1, "bgcolor", no)?;
    let ink = optional_color(args, 2, "fontcolor", no)?;
    let border = optional_color(args, 3, "bordercolor", no)?;
    let techn = non_empty(
        optional(get(args, "techn").or_else(|| get(args, "technology")))
            .or_else(|| args.positional.get(7).map(|text| unescape(text))),
    );
    match chart.ids.get(id).copied() {
        Some(Owner::Shape(index)) => {
            let shape = &mut chart.diagram.shapes[index];
            if let Some(color) = fill {
                shape.fill = Some(color);
            }
            if let Some(color) = ink {
                shape.ink = Some(color);
            }
            if let Some(color) = border {
                shape.border = Some(color);
            }
            if let Some(techn) = techn {
                shape.techn = Some(techn);
            }
            Ok(())
        }
        Some(Owner::Boundary(index)) => {
            let boundary = &mut chart.diagram.boundaries[index];
            if let Some(color) = fill {
                boundary.fill = Some(color);
            }
            if let Some(color) = ink {
                boundary.ink = Some(color);
            }
            if let Some(color) = border {
                boundary.border = Some(color);
            }
            Ok(())
        }
        None => Err(fail(no, "an element")),
    }
}

pub(super) fn apply_rel_style(chart: &mut Chart, no: usize, args: &Args) -> Result<(), Stop> {
    only_known(
        args,
        &["from", "to", "textcolor", "linecolor", "offsetx", "offsety"],
        no,
    )?;
    let Some(from) = args
        .positional
        .first()
        .map(String::as_str)
        .or_else(|| get(args, "from"))
    else {
        return Err(fail(no, "a relationship"));
    };
    let Some(to) = args
        .positional
        .get(1)
        .map(String::as_str)
        .or_else(|| get(args, "to"))
    else {
        return Err(fail(no, "a relationship"));
    };
    let from = shape_index(chart, from, no)?;
    let to = shape_index(chart, to, no)?;
    let text_color = optional_color(args, 2, "textcolor", no)?;
    let line_color = optional_color(args, 3, "linecolor", no)?;
    let offset_x = optional_offset(args, 4, "offsetx", no)?;
    let offset_y = optional_offset(args, 5, "offsety", no)?;
    let Some(rel) = chart
        .diagram
        .rels
        .iter_mut()
        .find(|rel| rel.from == from && rel.to == to)
    else {
        return Err(fail(no, "a relationship"));
    };
    if let Some(color) = text_color {
        rel.text_color = Some(color);
    }
    if let Some(color) = line_color {
        rel.line_color = Some(color);
    }
    if let Some(offset) = offset_x {
        rel.offset_x = offset;
    }
    if let Some(offset) = offset_y {
        rel.offset_y = offset;
    }
    Ok(())
}

pub(super) fn apply_layout(chart: &mut Chart, no: usize, args: &Args) -> Result<(), Stop> {
    only_known(args, &["c4shapeinrow", "c4boundaryinrow"], no)?;
    let shape = optional_row(args, 0, "c4shapeinrow", no)?;
    let boundary = optional_row(args, 1, "c4boundaryinrow", no)?;
    if shape.is_none() && boundary.is_none() {
        return Err(fail(no, "a layout"));
    }
    if let Some(count) = shape {
        chart.diagram.shapes_per_row = count;
    }
    if let Some(count) = boundary {
        chart.diagram.boundaries_per_row = count;
    }
    Ok(())
}

pub(super) fn optional_color(
    args: &Args,
    index: usize,
    key: &str,
    line: usize,
) -> Result<Option<Color>, Stop> {
    match positional_or_key(args, index, &[key]) {
        Some(value) if !value.trim().is_empty() => solid(&value, line).map(Some),
        _ => Ok(None),
    }
}

pub(super) fn optional_offset(
    args: &Args,
    index: usize,
    key: &str,
    line: usize,
) -> Result<Option<f32>, Stop> {
    match positional_or_key(args, index, &[key]) {
        Some(value) if !value.trim().is_empty() => finite_offset(&value, line).map(Some),
        _ => Ok(None),
    }
}

pub(super) fn optional_row(
    args: &Args,
    index: usize,
    key: &str,
    line: usize,
) -> Result<Option<usize>, Stop> {
    match positional_or_key(args, index, &[key]) {
        Some(value) if !value.trim().is_empty() => row_count(&value, line).map(Some),
        _ => Ok(None),
    }
}

pub(super) fn positional_or_key(args: &Args, index: usize, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = get(args, key) {
            return Some(value.to_string());
        }
    }
    args.positional.get(index).cloned()
}

pub(super) fn solid(value: &str, line: usize) -> Result<Color, Stop> {
    match color::parse(value.trim()) {
        Ok(Some(color)) => Ok(color),
        _ => Err(fail(line, "a color")),
    }
}

pub(super) fn finite_offset(value: &str, line: usize) -> Result<f32, Stop> {
    let number: f32 = value.trim().parse().map_err(|_| fail(line, "a style"))?;
    if number.is_finite() && number.abs() <= 2000.0 {
        Ok(number)
    } else {
        Err(fail(line, "a style"))
    }
}

pub(super) fn row_count(value: &str, line: usize) -> Result<usize, Stop> {
    let number: usize = value.trim().parse().map_err(|_| fail(line, "a layout"))?;
    if number >= 1 {
        Ok(number)
    } else {
        Err(fail(line, "a layout"))
    }
}

pub(super) fn refuse_sprite(args: &Args, index: Option<usize>, line: usize) -> Result<(), Stop> {
    let from_index = index.and_then(|index| args.positional.get(index).map(String::as_str));
    if sprite_value(args, from_index) {
        Err(fail(line, "a sprite"))
    } else {
        Ok(())
    }
}

pub(super) fn sprite_value(args: &Args, positional: Option<&str>) -> bool {
    get(args, "sprite")
        .map(unescape)
        .or_else(|| positional.map(unescape))
        .is_some_and(|text| !text.trim().is_empty())
}

pub(super) fn only_known(args: &Args, allowed: &[&str], line: usize) -> Result<(), Stop> {
    for (key, _) in &args.kv {
        let name = normalize_key(key);
        if !allowed.iter().any(|allowed| normalize_key(allowed) == name) {
            return Err(fail(line, "a style"));
        }
    }
    Ok(())
}

pub(super) fn claim(chart: &Chart, id: &str, line: usize) -> Result<(), Stop> {
    if chart.ids.contains_key(id) {
        Err(fail(line, "an element"))
    } else {
        Ok(())
    }
}

pub(super) fn shape_index(chart: &Chart, id: &str, line: usize) -> Result<usize, Stop> {
    match chart.ids.get(id) {
        Some(Owner::Shape(index)) => Ok(*index),
        _ => Err(fail(line, "a relationship")),
    }
}

pub(super) fn push(chart: &mut Chart, index: usize, line: usize) -> Result<(), Stop> {
    if chart.stack.len() >= 100 {
        return Err(fail(line, "a boundary"));
    }
    chart.stack.push(Frame { index, line });
    Ok(())
}

pub(super) fn pop(chart: &mut Chart, line: usize) -> Result<(), Stop> {
    if chart.stack.pop().is_none() {
        Err(fail(line, "a boundary"))
    } else {
        Ok(())
    }
}

pub(super) fn is_boundary_name(name: &str) -> bool {
    name.contains("boundary")
        || name.starts_with("deployment_node")
        || name == "node"
        || name == "node_l"
        || name == "node_r"
}

pub(super) fn boundary_type(name: &str) -> &'static str {
    if name.contains("enterprise") {
        "Enterprise"
    } else if name.contains("container") {
        "Container"
    } else if name.contains("system") {
        "System"
    } else if name.contains("node") {
        "Node"
    } else {
        "System"
    }
}

pub(super) fn shape_kind(name: &str) -> Option<Kind> {
    let ext = name.contains("ext");
    let db = name.contains("db");
    let queue = name.contains("queue");
    if name.contains("person") {
        return Some(if ext { Kind::PersonExt } else { Kind::Person });
    }
    if name.contains("system") {
        return Some(match (ext, db, queue) {
            (true, true, _) => Kind::SystemDbExt,
            (true, _, true) => Kind::SystemQueueExt,
            (true, _, _) => Kind::SystemExt,
            (false, true, _) => Kind::SystemDb,
            (false, _, true) => Kind::SystemQueue,
            (false, _, _) => Kind::System,
        });
    }
    if name.contains("container") {
        return Some(match (ext, db, queue) {
            (true, true, _) => Kind::ContainerDbExt,
            (true, _, true) => Kind::ContainerQueueExt,
            (true, _, _) => Kind::ContainerExt,
            (false, true, _) => Kind::ContainerDb,
            (false, _, true) => Kind::ContainerQueue,
            (false, _, _) => Kind::Container,
        });
    }
    if name.contains("component") {
        return Some(match (ext, db, queue) {
            (true, true, _) => Kind::ComponentDbExt,
            (true, _, true) => Kind::ComponentQueueExt,
            (true, _, _) => Kind::ComponentExt,
            (false, true, _) => Kind::ComponentDb,
            (false, _, true) => Kind::ComponentQueue,
            (false, _, _) => Kind::Component,
        });
    }
    None
}

impl Kind {
    pub(super) fn uses_techn(self) -> bool {
        !matches!(
            self,
            Kind::Person
                | Kind::PersonExt
                | Kind::System
                | Kind::SystemExt
                | Kind::SystemDb
                | Kind::SystemDbExt
                | Kind::SystemQueue
                | Kind::SystemQueueExt
        )
    }

    pub(super) fn external(self) -> bool {
        matches!(
            self,
            Kind::PersonExt
                | Kind::SystemExt
                | Kind::SystemDbExt
                | Kind::SystemQueueExt
                | Kind::ContainerExt
                | Kind::ContainerDbExt
                | Kind::ContainerQueueExt
                | Kind::ComponentExt
                | Kind::ComponentDbExt
                | Kind::ComponentQueueExt
        )
    }

    pub(super) fn mark(self) -> Mark {
        match self {
            Kind::Person | Kind::PersonExt => Mark::Figure,
            Kind::SystemDb
            | Kind::SystemDbExt
            | Kind::ContainerDb
            | Kind::ContainerDbExt
            | Kind::ComponentDb
            | Kind::ComponentDbExt => Mark::Cylinder,
            Kind::SystemQueue
            | Kind::SystemQueueExt
            | Kind::ContainerQueue
            | Kind::ContainerQueueExt
            | Kind::ComponentQueue
            | Kind::ComponentQueueExt => Mark::Queue,
            _ => Mark::Box,
        }
    }

    pub(super) fn stereotype(self) -> &'static str {
        match self {
            Kind::Person => "Person",
            Kind::PersonExt => "External",
            Kind::System => "System",
            Kind::SystemExt => "External system",
            Kind::SystemDb | Kind::ContainerDb | Kind::ComponentDb => "Database",
            Kind::SystemDbExt | Kind::ContainerDbExt | Kind::ComponentDbExt => "External database",
            Kind::SystemQueue | Kind::ContainerQueue | Kind::ComponentQueue => "Queue",
            Kind::SystemQueueExt | Kind::ContainerQueueExt | Kind::ComponentQueueExt => {
                "External queue"
            }
            Kind::Container => "Container",
            Kind::ContainerExt => "External container",
            Kind::Component => "Component",
            Kind::ComponentExt => "External component",
        }
    }
}

pub(super) fn rel_kind(name: &str) -> Option<RelKind> {
    if name.starts_with("birel") {
        Some(RelKind::Bi)
    } else if name.starts_with("rel_u") || name.starts_with("rel_up") {
        Some(RelKind::Up)
    } else if name.starts_with("rel_d") || name.starts_with("rel_down") {
        Some(RelKind::Down)
    } else if name.starts_with("rel_l") || name.starts_with("rel_left") {
        Some(RelKind::Left)
    } else if name.starts_with("rel_r") || name.starts_with("rel_right") {
        Some(RelKind::Right)
    } else if name.starts_with("rel_b") || name.starts_with("rel_back") {
        Some(RelKind::Back)
    } else if name.starts_with("rel") {
        Some(RelKind::Rel)
    } else {
        None
    }
}

pub(super) fn is_element_style(name: &str) -> bool {
    name == "updateelementstyle" || name == "update_el_style" || name == "updateelstyle"
}

pub(super) fn is_rel_style(name: &str) -> bool {
    name == "updaterelstyle" || name == "update_rel_style"
}

pub(super) fn is_layout_config(name: &str) -> bool {
    name == "updatelayoutconfig" || name == "update_layout_config"
}
