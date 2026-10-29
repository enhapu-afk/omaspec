//! Mermaid C4 diagrams: context, container, component, dynamic and deployment.
//!
//! A person is a head and shoulders, a database is a cylinder and a queue
//! is a box with a bowed side. Everything else is a rounded box. Boundaries
//! are dashed and hold their children. On the stock Mermaid theme the boxes
//! keep the classic C4 blues, and a light fill gets dark text. On any other
//! theme the fills are steps of the accent toward the page, so a person, a
//! system, a container, a component and an external element stay apart.
//! A sprite is refused by name: the picture would otherwise drop the icon
//! and keep the rest. Links and tags are not drawn. A relationship that
//! would cross another box is lifted into a free band above the row.

use std::collections::HashMap;

use crate::Declined;
use crate::color::{self, Color, Paint};
use crate::common::{Text, keyword, text_at};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const INNER: f32 = 14.0;
const MARGIN: f32 = 16.0;
const ROUND: f32 = 8.0;
const BOUND_ROUND: f32 = 12.0;
const HEADS: [&str; 5] = [
    "C4Context",
    "C4Container",
    "C4Component",
    "C4Dynamic",
    "C4Deployment",
];

/// A C4 diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    pub shapes: Vec<Shape>,
    pub boundaries: Vec<Boundary>,
    pub rels: Vec<Rel>,
    pub shapes_per_row: usize,
    pub boundaries_per_row: usize,
    pub look: Look,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub id: String,
    pub label: String,
    pub kind: Kind,
    pub type_label: Option<String>,
    pub techn: Option<String>,
    pub descr: Option<String>,
    pub parent: Option<usize>,
    pub fill: Option<Color>,
    pub border: Option<Color>,
    pub ink: Option<Color>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Boundary {
    pub id: String,
    pub label: String,
    pub kind_name: String,
    pub descr: Option<String>,
    pub parent: Option<usize>,
    pub fill: Option<Color>,
    pub border: Option<Color>,
    pub ink: Option<Color>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rel {
    pub kind: RelKind,
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub techn: Option<String>,
    pub descr: Option<String>,
    pub offset_x: f32,
    pub offset_y: f32,
    pub line_color: Option<Color>,
    pub text_color: Option<Color>,
    pub line: usize,
}

/// What a box stands for. The external forms are dashed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Person,
    PersonExt,
    System,
    SystemDb,
    SystemQueue,
    SystemExt,
    SystemDbExt,
    SystemQueueExt,
    Container,
    ContainerDb,
    ContainerQueue,
    ContainerExt,
    ContainerDbExt,
    ContainerQueueExt,
    Component,
    ComponentDb,
    ComponentQueue,
    ComponentExt,
    ComponentDbExt,
    ComponentQueueExt,
}

/// How a relationship is drawn. The four directions leave that side of the
/// source; a back relationship is dashed; a bi-relationship has two heads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelKind {
    Rel,
    Bi,
    Up,
    Down,
    Left,
    Right,
    Back,
}

/// Colors the theme fills in. `classic` keeps the C4 blues.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    pub font_size: f32,
    pub preferred: Color,
    pub background: Paint,
    pub accent: Color,
    pub line: Color,
    pub classic: bool,
    pub thickness: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            font_size: 14.0,
            preferred: Color::hex(0x333333),
            background: Some(Color::WHITE),
            accent: Color::hex(0x7B88A8),
            line: Color::hex(0x2F3B4D),
            classic: true,
            thickness: 1.5,
        }
    }
}

/// The C4 diagram `source` describes, or `None` when it is not one or it
/// uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a C4 diagram, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

/// The SVG. Relationship labels sit in the gap the layout left for them.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    if !(4.0..=100.0).contains(&diagram.look.font_size) {
        return Err(Declined::at(0, "the font size is not one Omaspec draws"));
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

#[derive(Clone, Copy)]
enum Owner {
    Shape(usize),
    Boundary(usize),
}

struct Frame {
    index: usize,
    line: usize,
}

struct Chart {
    diagram: Diagram,
    ids: HashMap<String, Owner>,
    stack: Vec<Frame>,
    saw_header: bool,
    skipping: bool,
}

fn read(source: &str) -> Result<Diagram, Stop> {
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
    for (no, raw) in source.lines().enumerate() {
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

fn one_line(chart: &mut Chart, no: usize, raw: &str) -> Result<(), Stop> {
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

fn set_title(chart: &mut Chart, no: usize, rest: &str) -> Result<(), Stop> {
    if rest.is_empty() || chart.diagram.title.is_some() {
        return Err(fail(no, "a title"));
    }
    chart.diagram.title = Some(text_at(&unescape(rest), no));
    Ok(())
}

fn take_header(line: &str, no: usize) -> Result<bool, Stop> {
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

fn body_line(chart: &mut Chart, no: usize, line: &str) -> Result<(), Stop> {
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

fn statement(chart: &mut Chart, no: usize, line: &str) -> Result<(), Stop> {
    let Some((name, args)) = function_call(line, no)? else {
        return Err(fail(no, "an unknown line"));
    };
    apply(chart, no, &name, &args)
}

fn function_call(line: &str, no: usize) -> Result<Option<(String, Vec<String>)>, Stop> {
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

fn apply(chart: &mut Chart, no: usize, name: &str, raw_args: &[String]) -> Result<(), Stop> {
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

fn apply_boundary(chart: &mut Chart, no: usize, name: &str, args: &Args) -> Result<(), Stop> {
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

fn apply_shape(chart: &mut Chart, no: usize, kind: Kind, args: &Args) -> Result<(), Stop> {
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

fn apply_rel(
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

fn apply_element_style(chart: &mut Chart, no: usize, args: &Args) -> Result<(), Stop> {
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

fn apply_rel_style(chart: &mut Chart, no: usize, args: &Args) -> Result<(), Stop> {
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

fn apply_layout(chart: &mut Chart, no: usize, args: &Args) -> Result<(), Stop> {
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

fn optional_color(
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

fn optional_offset(args: &Args, index: usize, key: &str, line: usize) -> Result<Option<f32>, Stop> {
    match positional_or_key(args, index, &[key]) {
        Some(value) if !value.trim().is_empty() => finite_offset(&value, line).map(Some),
        _ => Ok(None),
    }
}

fn optional_row(args: &Args, index: usize, key: &str, line: usize) -> Result<Option<usize>, Stop> {
    match positional_or_key(args, index, &[key]) {
        Some(value) if !value.trim().is_empty() => row_count(&value, line).map(Some),
        _ => Ok(None),
    }
}

fn positional_or_key(args: &Args, index: usize, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = get(args, key) {
            return Some(value.to_string());
        }
    }
    args.positional.get(index).cloned()
}

fn solid(value: &str, line: usize) -> Result<Color, Stop> {
    match color::parse(value.trim()) {
        Ok(Some(color)) => Ok(color),
        _ => Err(fail(line, "a color")),
    }
}

fn finite_offset(value: &str, line: usize) -> Result<f32, Stop> {
    let number: f32 = value.trim().parse().map_err(|_| fail(line, "a style"))?;
    if number.is_finite() && number.abs() <= 2000.0 {
        Ok(number)
    } else {
        Err(fail(line, "a style"))
    }
}

fn row_count(value: &str, line: usize) -> Result<usize, Stop> {
    let number: usize = value.trim().parse().map_err(|_| fail(line, "a layout"))?;
    if number >= 1 {
        Ok(number)
    } else {
        Err(fail(line, "a layout"))
    }
}

fn refuse_sprite(args: &Args, index: Option<usize>, line: usize) -> Result<(), Stop> {
    let from_index = index.and_then(|index| args.positional.get(index).map(String::as_str));
    if sprite_value(args, from_index) {
        Err(fail(line, "a sprite"))
    } else {
        Ok(())
    }
}

fn sprite_value(args: &Args, positional: Option<&str>) -> bool {
    get(args, "sprite")
        .map(unescape)
        .or_else(|| positional.map(unescape))
        .is_some_and(|text| !text.trim().is_empty())
}

fn only_known(args: &Args, allowed: &[&str], line: usize) -> Result<(), Stop> {
    for (key, _) in &args.kv {
        let name = normalize_key(key);
        if !allowed.iter().any(|allowed| normalize_key(allowed) == name) {
            return Err(fail(line, "a style"));
        }
    }
    Ok(())
}

fn claim(chart: &Chart, id: &str, line: usize) -> Result<(), Stop> {
    if chart.ids.contains_key(id) {
        Err(fail(line, "an element"))
    } else {
        Ok(())
    }
}

fn shape_index(chart: &Chart, id: &str, line: usize) -> Result<usize, Stop> {
    match chart.ids.get(id) {
        Some(Owner::Shape(index)) => Ok(*index),
        _ => Err(fail(line, "a relationship")),
    }
}

fn push(chart: &mut Chart, index: usize, line: usize) -> Result<(), Stop> {
    if chart.stack.len() >= 100 {
        return Err(fail(line, "a boundary"));
    }
    chart.stack.push(Frame { index, line });
    Ok(())
}

fn pop(chart: &mut Chart, line: usize) -> Result<(), Stop> {
    if chart.stack.pop().is_none() {
        Err(fail(line, "a boundary"))
    } else {
        Ok(())
    }
}

fn is_boundary_name(name: &str) -> bool {
    name.contains("boundary")
        || name.starts_with("deployment_node")
        || name == "node"
        || name == "node_l"
        || name == "node_r"
}

fn boundary_type(name: &str) -> &'static str {
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

fn shape_kind(name: &str) -> Option<Kind> {
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
    fn uses_techn(self) -> bool {
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

    fn external(self) -> bool {
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

    fn mark(self) -> Mark {
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

    fn stereotype(self) -> &'static str {
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

fn rel_kind(name: &str) -> Option<RelKind> {
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

fn is_element_style(name: &str) -> bool {
    name == "updateelementstyle" || name == "update_el_style" || name == "updateelstyle"
}

fn is_rel_style(name: &str) -> bool {
    name == "updaterelstyle" || name == "update_rel_style"
}

fn is_layout_config(name: &str) -> bool {
    name == "updatelayoutconfig" || name == "update_layout_config"
}

struct Args {
    positional: Vec<String>,
    kv: Vec<(String, String)>,
}

fn parse_args(args: &[String]) -> Args {
    let mut positional = Vec::new();
    let mut kv = Vec::new();
    for arg in args {
        let trimmed = arg.trim();
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim().trim_start_matches('$');
            if !key.is_empty() {
                kv.push((key.to_string(), value.trim().to_string()));
                continue;
            }
        }
        if !trimmed.is_empty() {
            positional.push(trimmed.to_string());
        }
    }
    Args { positional, kv }
}

fn get<'a>(args: &'a Args, key: &str) -> Option<&'a str> {
    let want = normalize_key(key);
    args.kv
        .iter()
        .find(|(name, _)| normalize_key(name) == want)
        .map(|(_, value)| value.as_str())
}

fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in input.chars() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            current.push(ch);
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            current.push(ch);
            continue;
        }
        if ch == ',' {
            push_arg(&mut args, &mut current);
            continue;
        }
        current.push(ch);
    }
    push_arg(&mut args, &mut current);
    args
}

fn push_arg(args: &mut Vec<String>, current: &mut String) {
    let trimmed = strip_quotes(current.trim());
    if !trimmed.is_empty() {
        args.push(trimmed.to_string());
    }
    current.clear();
}

fn strip_quotes(text: &str) -> &str {
    let bytes = text.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

fn strip_comment(raw: &str) -> &str {
    let line = raw.trim();
    let mut quote: Option<char> = None;
    for (index, ch) in line.char_indices() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == '%' && line[index..].starts_with("%%") {
            return line[..index].trim();
        }
    }
    line
}

fn split_brace(line: &str) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    for (index, ch) in line.char_indices() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == '{' {
            return Some((line[..index].trim(), line[index + 1..].trim()));
        }
    }
    None
}

fn split_trailing_close(line: &str) -> (&str, bool) {
    let mut quote: Option<char> = None;
    let mut close_at = None;
    for (index, ch) in line.char_indices() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == '}' {
            close_at = Some(index);
        } else if !ch.is_whitespace() {
            close_at = None;
        }
    }
    match close_at {
        Some(index) => (line[..index].trim(), true),
        None => (line.trim(), false),
    }
}

fn access_head(line: &str) -> bool {
    let head = line
        .split(|ch: char| ch.is_whitespace() || ch == ':')
        .next()
        .unwrap_or("");
    head.eq_ignore_ascii_case("accTitle") || head.eq_ignore_ascii_case("accDescr")
}

fn opens_block(line: &str) -> bool {
    split_brace(line).is_some() || line.contains('{')
}

fn closes_block(line: &str) -> bool {
    split_trailing_close(line).1 || line.contains('}')
}

fn norm_name(name: &str) -> String {
    name.trim()
        .chars()
        .map(|ch| match ch {
            '-' => '_',
            _ => ch.to_ascii_lowercase(),
        })
        .collect()
}

fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|ch| *ch != '_' && *ch != '-')
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn unescape(text: &str) -> String {
    text.replace("\\n", "\n").replace("\\t", " ")
}

fn optional(text: Option<&str>) -> Option<String> {
    text.map(unescape)
}

fn non_empty(text: Option<String>) -> Option<String> {
    text.filter(|value| !value.trim().is_empty())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Figure,
    Cylinder,
    Queue,
    Box,
}

#[derive(Clone, Copy, Debug)]
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

    fn inflate(self, pad: f32) -> Rect {
        Rect {
            x: self.x - pad,
            y: self.y - pad,
            w: self.w + pad * 2.0,
            h: self.h + pad * 2.0,
        }
    }
}

struct Span {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

impl Span {
    fn at(x: f32, y: f32) -> Span {
        Span {
            left: x,
            right: x,
            top: y,
            bottom: y,
        }
    }
}

#[derive(Clone, Copy)]
struct Gaps {
    x: f32,
    y: f32,
    lane: f32,
    top: f32,
    bottom: f32,
    side: f32,
    /// Tallest relationship chip, used to keep the overhead band clear.
    chip_h: f32,
    /// Distance between two overhead lanes.
    pitch: f32,
}

/// Which relationships use the overhead band, and where that band starts.
struct Lane<'a> {
    sky: &'a [usize],
    y0: f32,
}

struct PlacedShape {
    rect: Rect,
    lines: Vec<Block>,
}

struct PlacedBoundary {
    rect: Rect,
    lines: Vec<Block>,
}

struct PlacedRel {
    points: Vec<(f32, f32)>,
    lines: Vec<Block>,
    chip: Rect,
    color: Color,
    dashed: bool,
    bi: bool,
}

struct Laid {
    title: Option<Block>,
    shapes: Vec<PlacedShape>,
    boundaries: Vec<PlacedBoundary>,
    rels: Vec<PlacedRel>,
}

struct Measured {
    w: f32,
    h: f32,
    lines: Vec<Block>,
}

fn lay(diagram: &Diagram, family: Family) -> Result<Laid, Declined> {
    let gaps = gaps(diagram, family)?;
    let title = match &diagram.title {
        Some(title) => Some(measured(
            &title.value,
            Font::new(diagram.look.font_size + 2.0, page_ink(&diagram.look)).bold(),
            family,
            title.line,
        )?),
        None => None,
    };
    let title_h = title
        .as_ref()
        .map(|block| block.height + 14.0)
        .unwrap_or(0.0);
    let mut placed = place(diagram, family, &gaps, title.clone(), title_h, 0.0)?;
    let mut rels = route_all(
        diagram,
        family,
        &shape_rects(&placed),
        &gaps,
        &header_bands(&placed.boundaries),
        &Lane {
            sky: &[],
            y0: title_h,
        },
    )?;
    let mut sky = Vec::new();
    for (index, rel) in rels.iter().enumerate() {
        let ends = (diagram.rels[index].from, diagram.rels[index].to);
        if route_hits(
            &rel.points,
            ends,
            &shape_rects(&placed),
            &header_bands(&placed.boundaries),
        ) {
            sky.push(index);
        }
    }
    if sky.is_empty() {
        placed.rels = rels;
        return Ok(placed);
    }
    // Crossing lines move into a band under the title. The boxes are placed
    // again below that band; the straight links keep their gaps.
    let band = sky.len() as f32 * gaps.pitch + 6.0;
    placed = place(diagram, family, &gaps, title, title_h, band)?;
    rels = route_all(
        diagram,
        family,
        &shape_rects(&placed),
        &gaps,
        &header_bands(&placed.boundaries),
        &Lane {
            sky: &sky,
            y0: title_h,
        },
    )?;
    placed.rels = rels;
    Ok(placed)
}

fn place(
    diagram: &Diagram,
    family: Family,
    gaps: &Gaps,
    title: Option<Block>,
    title_h: f32,
    band: f32,
) -> Result<Laid, Declined> {
    let mut shapes = Vec::new();
    shapes.resize_with(diagram.shapes.len(), || None);
    let mut boundaries = Vec::new();
    boundaries.resize_with(diagram.boundaries.len(), || None);
    {
        let mut geom = Geom {
            diagram,
            family,
            gaps: *gaps,
            shapes: &mut shapes,
            boundaries: &mut boundaries,
        };
        geom.place_level(None, 0.0, title_h + band)?;
    }
    Ok(Laid {
        title,
        shapes: shapes.into_iter().flatten().collect(),
        boundaries: boundaries.into_iter().flatten().collect(),
        rels: Vec::new(),
    })
}

fn shape_rects(laid: &Laid) -> Vec<Rect> {
    laid.shapes.iter().map(|shape| shape.rect).collect()
}

fn header_bands(boundaries: &[PlacedBoundary]) -> Vec<Rect> {
    boundaries
        .iter()
        .map(|placed| {
            let width = placed
                .lines
                .iter()
                .fold(0.0_f32, |wide, line| wide.max(line.width))
                + 8.0;
            Rect {
                x: placed.rect.cx() - width / 2.0,
                y: placed.rect.y + 2.0,
                w: width,
                h: stack_height(&placed.lines) + 6.0,
            }
        })
        .collect()
}

struct Geom<'a> {
    diagram: &'a Diagram,
    family: Family,
    gaps: Gaps,
    shapes: &'a mut [Option<PlacedShape>],
    boundaries: &'a mut [Option<PlacedBoundary>],
}

impl Geom<'_> {
    fn place_level(&mut self, parent: Option<usize>, ox: f32, oy: f32) -> Result<Span, Declined> {
        let shape_ids = self.child_shapes(parent);
        let bound_ids = self.child_bounds(parent);
        let top = oy + self.gaps.top;
        let left = ox + INNER;
        let shape_span = self.place_shapes(&shape_ids, left, top)?;
        let bound_x = if shape_ids.is_empty() {
            left
        } else {
            shape_span.right + self.gaps.x
        };
        let bound_span = self.place_bounds(&bound_ids, bound_x, top)?;
        if !shape_ids.is_empty() && !bound_ids.is_empty() {
            let shape_h = shape_span.bottom - shape_span.top;
            let bound_h = bound_span.bottom - bound_span.top;
            if shape_h + 1.0 < bound_h {
                self.shift_shapes(&shape_ids, (bound_h - shape_h) / 2.0);
            } else if bound_h + 1.0 < shape_h {
                for id in &bound_ids {
                    self.shift_tree(*id, (shape_h - bound_h) / 2.0);
                }
            }
        }
        Ok(self.enclose(parent, left, top))
    }

    fn place_shapes(&mut self, ids: &[usize], x0: f32, y0: f32) -> Result<Span, Declined> {
        if ids.is_empty() {
            return Ok(Span::at(x0, y0));
        }
        let per = self.diagram.shapes_per_row.max(1);
        let mut y = y0;
        let mut right = x0;
        let mut start = 0;
        while start < ids.len() {
            let end = (start + per).min(ids.len());
            let mut x = x0;
            let mut row_h = 0.0_f32;
            let mut row = Vec::new();
            for &id in &ids[start..end] {
                let measured = self.measure_shape(id)?;
                row_h = row_h.max(measured.h);
                row.push((id, measured));
            }
            for (index, (id, measured)) in row.into_iter().enumerate() {
                self.shapes[id] = Some(PlacedShape {
                    rect: Rect {
                        x,
                        y,
                        w: measured.w,
                        h: measured.h,
                    },
                    lines: measured.lines,
                });
                right = right.max(x + measured.w);
                x += measured.w;
                if index + 1 != end - start {
                    x += self.gaps.x;
                }
            }
            y += row_h;
            start = end;
            if start < ids.len() {
                y += self.gaps.y;
            }
        }
        Ok(Span {
            left: x0,
            right,
            top: y0,
            bottom: y,
        })
    }

    fn place_bounds(&mut self, ids: &[usize], x0: f32, y0: f32) -> Result<Span, Declined> {
        if ids.is_empty() {
            return Ok(Span::at(x0, y0));
        }
        let per = self.diagram.boundaries_per_row.max(1);
        let mut y = y0;
        let mut right = x0;
        let mut start = 0;
        while start < ids.len() {
            let end = (start + per).min(ids.len());
            let mut x = x0;
            let mut row_h = 0.0_f32;
            for (index, &id) in ids[start..end].iter().enumerate() {
                let (w, h) = self.place_boundary(id, x, y)?;
                right = right.max(x + w);
                row_h = row_h.max(h);
                x += w;
                if index + 1 != end - start {
                    x += self.gaps.x;
                }
            }
            y += row_h;
            start = end;
            if start < ids.len() {
                y += self.gaps.y;
            }
        }
        Ok(Span {
            left: x0,
            right,
            top: y0,
            bottom: y,
        })
    }

    fn place_boundary(&mut self, id: usize, x: f32, y: f32) -> Result<(f32, f32), Declined> {
        let lines = self.header_lines(id)?;
        let header_w = lines
            .iter()
            .fold(0.0_f32, |wide, line| wide.max(line.width));
        let header_h = stack_height(&lines) + 10.0;
        let inner = self.place_level(Some(id), x, y + header_h)?;
        let right = inner.right.max(x + INNER + header_w).max(x + INNER) + self.gaps.side;
        let bottom = inner.bottom + self.gaps.bottom;
        let w = (right - x).max(header_w + INNER * 2.0);
        let h = (bottom - y).max(header_h + INNER);
        self.boundaries[id] = Some(PlacedBoundary {
            rect: Rect { x, y, w, h },
            lines,
        });
        Ok((w, h))
    }

    fn measure_shape(&self, id: usize) -> Result<Measured, Declined> {
        let shape = &self.diagram.shapes[id];
        let look = &self.diagram.look;
        let style = shape_paint(shape, look);
        let ink = if style.mark == Mark::Figure && shape.ink.is_none() {
            page_ink(look)
        } else {
            style.ink
        };
        let mut lines = Vec::new();
        if style.mark != Mark::Figure || shape.kind.external() {
            lines.push(measured(
                shape.kind.stereotype(),
                Font::new(look.font_size * 0.75, ink).italic(),
                self.family,
                shape.line,
            )?);
        }
        lines.push(measured(
            &wrap(&shape.label, 28),
            Font::new(look.font_size, ink).bold(),
            self.family,
            shape.line,
        )?);
        if let Some(type_label) = &shape.type_label {
            lines.push(measured(
                &wrap(type_label, 32),
                Font::new(look.font_size * 0.78, ink).italic(),
                self.family,
                shape.line,
            )?);
        }
        if let Some(techn) = &shape.techn {
            lines.push(measured(
                &format!("[{}]", wrap(techn, 28)),
                Font::new(look.font_size * 0.78, ink).italic(),
                self.family,
                shape.line,
            )?);
        }
        if let Some(descr) = &shape.descr {
            lines.push(measured(
                &wrap(descr, 32),
                Font::new(look.font_size * 0.75, ink),
                self.family,
                shape.line,
            )?);
        }
        let text_w = lines
            .iter()
            .fold(0.0_f32, |wide, line| wide.max(line.width));
        let text_h = stack_height(&lines);
        let (w, h) = match style.mark {
            Mark::Figure => ((text_w + 20.0).max(96.0), 58.0 + text_h + 12.0),
            Mark::Cylinder => ((text_w + 28.0).max(132.0), (text_h + 36.0).max(88.0)),
            Mark::Queue => ((text_w + 44.0).max(148.0), (text_h + 28.0).max(76.0)),
            Mark::Box => ((text_w + 28.0).max(128.0), (text_h + 28.0).max(72.0)),
        };
        Ok(Measured { w, h, lines })
    }

    fn header_lines(&self, id: usize) -> Result<Vec<Block>, Declined> {
        let boundary = &self.diagram.boundaries[id];
        let look = &self.diagram.look;
        let ink = boundary.ink.unwrap_or_else(|| {
            boundary
                .fill
                .map(|fill| readable(fill, look.preferred))
                .unwrap_or_else(|| page_ink(look))
        });
        let mut lines = vec![
            measured(
                &wrap(&boundary.label, 32),
                Font::new(look.font_size, ink).bold(),
                self.family,
                boundary.line,
            )?,
            measured(
                &format!("[{}]", boundary.kind_name),
                Font::new(look.font_size * 0.78, ink).italic(),
                self.family,
                boundary.line,
            )?,
        ];
        if let Some(descr) = &boundary.descr {
            lines.push(measured(
                &wrap(descr, 36),
                Font::new(look.font_size * 0.75, ink),
                self.family,
                boundary.line,
            )?);
        }
        Ok(lines)
    }

    fn child_shapes(&self, parent: Option<usize>) -> Vec<usize> {
        self.diagram
            .shapes
            .iter()
            .enumerate()
            .filter(|(_, shape)| shape.parent == parent)
            .map(|(index, _)| index)
            .collect()
    }

    fn child_bounds(&self, parent: Option<usize>) -> Vec<usize> {
        self.diagram
            .boundaries
            .iter()
            .enumerate()
            .filter(|(_, boundary)| boundary.parent == parent)
            .map(|(index, _)| index)
            .collect()
    }

    fn shift_shapes(&mut self, ids: &[usize], dy: f32) {
        for id in ids {
            if let Some(shape) = &mut self.shapes[*id] {
                shape.rect.y += dy;
            }
        }
    }

    fn shift_tree(&mut self, id: usize, dy: f32) {
        if let Some(boundary) = &mut self.boundaries[id] {
            boundary.rect.y += dy;
        }
        let shapes = self.child_shapes(Some(id));
        self.shift_shapes(&shapes, dy);
        let children = self.child_bounds(Some(id));
        for child in children {
            self.shift_tree(child, dy);
        }
    }

    fn enclose(&self, parent: Option<usize>, left: f32, top: f32) -> Span {
        let mut span = Span::at(left, top);
        for (index, shape) in self.diagram.shapes.iter().enumerate() {
            if shape.parent == parent
                && let Some(placed) = &self.shapes[index]
            {
                span = union(span, placed.rect);
            }
        }
        for (index, boundary) in self.diagram.boundaries.iter().enumerate() {
            if boundary.parent == parent
                && let Some(placed) = &self.boundaries[index]
            {
                span = union(span, placed.rect);
            }
        }
        span
    }
}

fn union(span: Span, rect: Rect) -> Span {
    Span {
        left: span.left.min(rect.x),
        right: span.right.max(rect.right()),
        top: span.top.min(rect.y),
        bottom: span.bottom.max(rect.bottom()),
    }
}

fn gaps(diagram: &Diagram, family: Family) -> Result<Gaps, Declined> {
    let mut chip_w = 0.0_f32;
    let mut chip_h = 0.0_f32;
    for rel in &diagram.rels {
        let (width, height) = chip_size(diagram, rel, family)?;
        chip_w = chip_w.max(width);
        chip_h = chip_h.max(height);
    }
    let mut leave = vec![0usize; diagram.shapes.len()];
    let mut self_count = 0usize;
    let mut elbow = false;
    for rel in &diagram.rels {
        if rel.from == rel.to {
            self_count += 1;
        } else {
            leave[rel.from] += 1;
            leave[rel.to] += 1;
        }
        if !matches!(rel.kind, RelKind::Rel | RelKind::Bi | RelKind::Back) {
            elbow = true;
        }
    }
    let max_side = leave.into_iter().max().unwrap_or(1);
    let spread = max_side.saturating_sub(1) as f32 * 12.0;
    let x = (chip_w + 40.0 + spread).max(72.0);
    let y = (chip_h + 28.0).max(40.0);
    let lane = (y - chip_h / 2.0 - 8.0).clamp(14.0, 40.0);
    let self_pad = if self_count == 0 {
        0.0
    } else {
        20.0 + self_count as f32 * 14.0 + chip_w * 0.5
    };
    Ok(Gaps {
        x,
        y,
        lane,
        top: if elbow || self_count > 0 { y } else { 6.0 },
        bottom: if elbow { y * 0.45 } else { 8.0 },
        side: if self_count > 0 {
            x.max(self_pad)
        } else if elbow {
            lane + 20.0
        } else {
            10.0
        },
        chip_h,
        pitch: chip_h + 10.0,
    })
}

fn chip_size(diagram: &Diagram, rel: &Rel, family: Family) -> Result<(f32, f32), Declined> {
    let lines = chip_lines(diagram, rel, family, page_ink(&diagram.look))?;
    let width = lines
        .iter()
        .fold(0.0_f32, |wide, line| wide.max(line.width));
    Ok((width + 12.0, stack_height(&lines) + 8.0))
}

fn chip_lines(
    diagram: &Diagram,
    rel: &Rel,
    family: Family,
    ink: Color,
) -> Result<Vec<Block>, Declined> {
    let size = diagram.look.font_size;
    let mut lines = vec![measured(
        &wrap(&rel.label, 28),
        Font::new(size * 0.86, ink).bold(),
        family,
        rel.line,
    )?];
    if let Some(techn) = &rel.techn {
        lines.push(measured(
            &format!("[{}]", wrap(techn, 28)),
            Font::new(size * 0.75, ink).italic(),
            family,
            rel.line,
        )?);
    }
    if let Some(descr) = &rel.descr {
        lines.push(measured(
            &wrap(descr, 32),
            Font::new(size * 0.72, ink),
            family,
            rel.line,
        )?);
    }
    Ok(lines)
}

fn route_all(
    diagram: &Diagram,
    family: Family,
    rects: &[Rect],
    gaps: &Gaps,
    headers: &[Rect],
    lane: &Lane<'_>,
) -> Result<Vec<PlacedRel>, Declined> {
    let sides: Vec<(Side, Side)> = diagram
        .rels
        .iter()
        .map(|rel| {
            if rel.from == rel.to {
                (Side::Right, Side::Right)
            } else {
                ends(rel.kind, rects[rel.from], rects[rel.to])
            }
        })
        .collect();
    let (exit_slot, entry_slot) = slot_maps(diagram, &sides);
    let mut obstacles = Vec::with_capacity(rects.len() + headers.len());
    obstacles.extend_from_slice(rects);
    obstacles.extend_from_slice(headers);
    let mut placed = Vec::new();
    for (index, rel) in diagram.rels.iter().enumerate() {
        let points = if let Some(slot) = lane.sky.iter().position(|sky| *sky == index) {
            skyway(
                rects[rel.from],
                rects[rel.to],
                slot,
                lane.y0,
                gaps,
                &obstacles,
            )
        } else if rel.from == rel.to {
            self_route(rects[rel.from], exit_slot[index].0)
        } else if faces(rel.kind, rects[rel.from], rects[rel.to], sides[index]) {
            short_route(
                rects[rel.from],
                rects[rel.to],
                sides[index],
                exit_slot[index],
                entry_slot[index],
            )
        } else {
            tour(
                rects[rel.from],
                rects[rel.to],
                sides[index],
                exit_slot[index].0,
                gaps.lane,
            )
        };
        let ink = rel.text_color.unwrap_or_else(|| page_ink(&diagram.look));
        let lines = chip_lines(diagram, rel, family, ink)?;
        let width = lines
            .iter()
            .fold(0.0_f32, |wide, line| wide.max(line.width))
            + 12.0;
        let height = stack_height(&lines) + 8.0;
        let anchor = label_anchor(&points);
        let mut chip = Rect {
            x: anchor.0 - width / 2.0 + rel.offset_x,
            y: anchor.1 - height / 2.0 + rel.offset_y,
            w: width,
            h: height,
        };
        chip = nudge(chip, &obstacles, &placed);
        placed.push(PlacedRel {
            points,
            lines,
            chip,
            color: rel.line_color.unwrap_or(diagram.look.line),
            dashed: rel.kind == RelKind::Back,
            bi: rel.kind == RelKind::Bi,
        });
    }
    Ok(placed)
}

type Slot = (usize, usize);

fn slot_maps(diagram: &Diagram, sides: &[(Side, Side)]) -> (Vec<Slot>, Vec<Slot>) {
    let mut exit_count: HashMap<(usize, Side), usize> = HashMap::new();
    let mut entry_count: HashMap<(usize, Side), usize> = HashMap::new();
    for (index, rel) in diagram.rels.iter().enumerate() {
        if rel.from == rel.to {
            *exit_count.entry((rel.from, Side::Right)).or_insert(0) += 1;
        } else {
            *exit_count.entry((rel.from, sides[index].0)).or_insert(0) += 1;
            *entry_count.entry((rel.to, sides[index].1)).or_insert(0) += 1;
        }
    }
    let mut exit_seen: HashMap<(usize, Side), usize> = HashMap::new();
    let mut entry_seen: HashMap<(usize, Side), usize> = HashMap::new();
    let mut exits = Vec::new();
    let mut entries = Vec::new();
    for (index, rel) in diagram.rels.iter().enumerate() {
        if rel.from == rel.to {
            let key = (rel.from, Side::Right);
            let seen = exit_seen.entry(key).or_insert(0);
            let slot = *seen;
            *seen += 1;
            let slots = exit_count[&key];
            exits.push((slot, slots));
            entries.push((0, 1));
            continue;
        }
        let exit_key = (rel.from, sides[index].0);
        let entry_key = (rel.to, sides[index].1);
        let exit = exit_seen.entry(exit_key).or_insert(0);
        let exit_slot = *exit;
        *exit += 1;
        let entry = entry_seen.entry(entry_key).or_insert(0);
        let entry_slot = *entry;
        *entry += 1;
        exits.push((exit_slot, exit_count[&exit_key]));
        entries.push((entry_slot, entry_count[&entry_key]));
    }
    (exits, entries)
}

fn ends(kind: RelKind, from: Rect, to: Rect) -> (Side, Side) {
    match kind {
        RelKind::Up => (Side::Top, Side::Bottom),
        RelKind::Down => (Side::Bottom, Side::Top),
        RelKind::Left => (Side::Left, Side::Right),
        RelKind::Right => (Side::Right, Side::Left),
        RelKind::Rel | RelKind::Bi | RelKind::Back => facing(from, to),
    }
}

fn facing(from: Rect, to: Rect) -> (Side, Side) {
    let dx = to.cx() - from.cx();
    let dy = to.cy() - from.cy();
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            (Side::Right, Side::Left)
        } else {
            (Side::Left, Side::Right)
        }
    } else if dy >= 0.0 {
        (Side::Bottom, Side::Top)
    } else {
        (Side::Top, Side::Bottom)
    }
}

fn faces(kind: RelKind, from: Rect, to: Rect, sides: (Side, Side)) -> bool {
    if !matches!(kind, RelKind::Rel | RelKind::Bi | RelKind::Back) {
        let toward = match sides.0 {
            Side::Top => to.cy() < from.cy(),
            Side::Bottom => to.cy() > from.cy(),
            Side::Left => to.cx() < from.cx(),
            Side::Right => to.cx() > from.cx(),
        };
        if !toward {
            return false;
        }
    }
    match sides {
        (Side::Right, Side::Left) => to.cx() >= from.cx(),
        (Side::Left, Side::Right) => to.cx() <= from.cx(),
        (Side::Bottom, Side::Top) => to.cy() >= from.cy(),
        (Side::Top, Side::Bottom) => to.cy() <= from.cy(),
        _ => false,
    }
}

fn short_route(
    from: Rect,
    to: Rect,
    sides: (Side, Side),
    exit: (usize, usize),
    entry: (usize, usize),
) -> Vec<(f32, f32)> {
    let from_port = port(from, sides.0, exit.0, exit.1);
    let to_port = port(to, sides.1, entry.0, entry.1);
    let jog = match sides.0 {
        Side::Left | Side::Right => {
            let mid = (from_port.0 + to_port.0) / 2.0;
            let spread = spread(exit.0, exit.1);
            (
                clamp_between(from_port.0, to_port.0, mid + spread),
                from_port.1,
            )
        }
        Side::Top | Side::Bottom => {
            let mid = (from_port.1 + to_port.1) / 2.0;
            let spread = spread(exit.0, exit.1);
            (
                from_port.0,
                clamp_between(from_port.1, to_port.1, mid + spread),
            )
        }
    };
    let mut points = vec![from_port, step(from_port, sides.0, 12.0)];
    match sides.0 {
        Side::Left | Side::Right => {
            points.push((jog.0, points[1].1));
            points.push((jog.0, to_port.1));
        }
        Side::Top | Side::Bottom => {
            points.push((points[1].0, jog.1));
            points.push((to_port.0, jog.1));
        }
    }
    points.push(step(to_port, sides.1, 12.0));
    points.push(to_port);
    dedup(points)
}

fn tour(from: Rect, to: Rect, sides: (Side, Side), slot: usize, lane: f32) -> Vec<(f32, f32)> {
    let out = (14.0 + slot as f32 * 8.0).min(lane);
    let from_port = port(from, sides.0, slot, slot + 1);
    let to_port = port(to, sides.1, 0, 1);
    let left = from.x.min(to.x) - out;
    let right = from.right().max(to.right()) + out;
    let top = from.y.min(to.y) - out;
    let bottom = from.bottom().max(to.bottom()) + out;
    let a = step(from_port, sides.0, out);
    let b = step(to_port, sides.1, out);
    let mut points = vec![from_port, a];
    points.push(lane_point(a, sides.0, left, right, top, bottom));
    points.push(lane_point(b, sides.1, left, right, top, bottom));
    points.push(b);
    points.push(to_port);
    dedup(points)
}

fn lane_point(
    point: (f32, f32),
    side: Side,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
) -> (f32, f32) {
    match side {
        Side::Top => (point.0, top),
        Side::Bottom => (point.0, bottom),
        Side::Left => (left, point.1),
        Side::Right => (right, point.1),
    }
}

fn self_route(rect: Rect, slot: usize) -> Vec<(f32, f32)> {
    let y1 = rect.y + rect.h * 0.32;
    let y2 = rect.y + rect.h * 0.68;
    let x = rect.right();
    let out = 18.0 + slot as f32 * 14.0;
    vec![(x, y1), (x + out, y1), (x + out, y2), (x, y2)]
}

fn port(rect: Rect, side: Side, slot: usize, slots: usize) -> (f32, f32) {
    let t = if slots <= 1 {
        0.5
    } else {
        0.22 + 0.56 * (slot as f32) / (slots as f32 - 1.0)
    };
    match side {
        Side::Top => (rect.x + rect.w * t, rect.y),
        Side::Bottom => (rect.x + rect.w * t, rect.bottom()),
        Side::Left => (rect.x, rect.y + rect.h * t),
        Side::Right => (rect.right(), rect.y + rect.h * t),
    }
}

fn step(point: (f32, f32), side: Side, dist: f32) -> (f32, f32) {
    match side {
        Side::Top => (point.0, point.1 - dist),
        Side::Bottom => (point.0, point.1 + dist),
        Side::Left => (point.0 - dist, point.1),
        Side::Right => (point.0 + dist, point.1),
    }
}

fn spread(slot: usize, slots: usize) -> f32 {
    if slots <= 1 {
        0.0
    } else {
        (slot as f32 - (slots as f32 - 1.0) / 2.0) * 12.0
    }
}

fn clamp_between(a: f32, b: f32, value: f32) -> f32 {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    value.clamp(lo, hi)
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

fn skyway(
    from: Rect,
    to: Rect,
    slot: usize,
    band_y0: f32,
    gaps: &Gaps,
    obstacles: &[Rect],
) -> Vec<(f32, f32)> {
    let y = band_y0 + gaps.chip_h * 0.5 + 4.0 + slot as f32 * gaps.pitch;
    let x1 = drop_x(from, obstacles, y);
    let x2 = drop_x(to, obstacles, y);
    dedup(vec![(x1, from.y), (x1, y), (x2, y), (x2, to.y)])
}

/// An x on the top of `rect` whose drop from the overhead band misses every
/// other box and every boundary title.
fn drop_x(rect: Rect, obstacles: &[Rect], band_y: f32) -> f32 {
    for share in [0.5_f32, 0.3, 0.7, 0.16, 0.84] {
        let x = rect.x + rect.w * share;
        let clear = obstacles
            .iter()
            .all(|obstacle| !segment_hits((x, band_y), (x, rect.y), *obstacle));
        if clear {
            return x;
        }
    }
    rect.cx()
}

fn route_hits(
    points: &[(f32, f32)],
    ends: (usize, usize),
    rects: &[Rect],
    headers: &[Rect],
) -> bool {
    for pair in points.windows(2) {
        for (index, rect) in rects.iter().enumerate() {
            if index != ends.0 && index != ends.1 && segment_hits(pair[0], pair[1], *rect) {
                return true;
            }
        }
        if headers
            .iter()
            .any(|rect| segment_hits(pair[0], pair[1], *rect))
        {
            return true;
        }
    }
    false
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
        } else {
            if t < t0 {
                return false;
            }
            if t < t1 {
                t1 = t;
            }
        }
    }
    true
}

fn label_anchor(points: &[(f32, f32)]) -> (f32, f32) {
    let mut best = 0.0_f32;
    let mut at = points.first().copied().unwrap_or((0.0, 0.0));
    for pair in points.windows(2) {
        let len = ((pair[1].0 - pair[0].0).powi(2) + (pair[1].1 - pair[0].1).powi(2)).sqrt();
        if len > best {
            best = len;
            at = ((pair[0].0 + pair[1].0) / 2.0, (pair[0].1 + pair[1].1) / 2.0);
        }
    }
    at
}

fn nudge(chip: Rect, obstacles: &[Rect], placed: &[PlacedRel]) -> Rect {
    let fits = |chip: Rect| {
        obstacles
            .iter()
            .all(|obstacle| !chip.inflate(2.0).intersects(*obstacle))
            && placed
                .iter()
                .all(|rel| !chip.inflate(2.0).intersects(rel.chip))
    };
    if fits(chip) {
        return chip;
    }
    for ring in 1..18 {
        let distance = ring as f32 * 6.0;
        let shifts = [
            (0.0, -distance),
            (0.0, distance),
            (distance, 0.0),
            (-distance, 0.0),
            (distance, -distance),
            (-distance, -distance),
            (distance, distance),
            (-distance, distance),
        ];
        for (dx, dy) in shifts {
            let moved = Rect {
                x: chip.x + dx,
                y: chip.y + dy,
                w: chip.w,
                h: chip.h,
            };
            if fits(moved) {
                return moved;
            }
        }
    }
    chip
}

fn measured(text: &str, font: Font, family: Family, line: usize) -> Result<Block, Declined> {
    text::block(text, font, family).map_err(|reason| Declined::at(line, reason))
}

fn stack_height(lines: &[Block]) -> f32 {
    let gaps = lines.len().saturating_sub(1) as f32 * 2.0;
    lines.iter().map(|line| line.height).sum::<f32>() + gaps
}

fn wrap(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        wrap_line(&mut out, line, limit);
    }
    out
}

fn wrap_line(out: &mut String, line: &str, limit: usize) {
    if line.chars().count() <= limit {
        out.push_str(line);
        return;
    }
    let mut width = 0usize;
    let mut word = String::new();
    let flush = |out: &mut String, word: &mut String, width: &mut usize| {
        if word.is_empty() {
            return;
        }
        if *width > 0 {
            out.push(' ');
        }
        out.push_str(word);
        *width += word.chars().count();
        word.clear();
    };
    for ch in line.chars() {
        if ch.is_whitespace() {
            if width > 0 && width + 1 + word.chars().count() > limit {
                out.push('\n');
                width = 0;
            }
            flush(out, &mut word, &mut width);
        } else {
            word.push(ch);
        }
    }
    if width > 0 && width + 1 + word.chars().count() > limit {
        out.push('\n');
        width = 0;
    }
    flush(out, &mut word, &mut width);
}

struct ShapePaint {
    fill: Color,
    border: Color,
    ink: Color,
    dashed: bool,
    mark: Mark,
}

fn shape_paint(shape: &Shape, look: &Look) -> ShapePaint {
    let (fill, border) = shape
        .fill
        .zip(shape.border)
        .unwrap_or_else(|| family_colors(shape.kind, look));
    let fill = shape.fill.unwrap_or(fill);
    let border = shape.border.unwrap_or(border);
    let mark = shape.kind.mark();
    let ink = shape.ink.unwrap_or_else(|| {
        if mark == Mark::Figure {
            page_ink(look)
        } else {
            readable(fill, look.preferred)
        }
    });
    ShapePaint {
        fill,
        border,
        ink,
        dashed: shape.kind.external(),
        mark,
    }
}

fn family_colors(kind: Kind, look: &Look) -> (Color, Color) {
    if look.classic {
        classic_colors(kind)
    } else {
        let page = look.background.unwrap_or(Color::WHITE);
        let share = match kind {
            Kind::Person => 0.0,
            Kind::System | Kind::SystemDb | Kind::SystemQueue => 0.28,
            Kind::Container | Kind::ContainerDb | Kind::ContainerQueue => 0.48,
            Kind::Component | Kind::ComponentDb | Kind::ComponentQueue => 0.68,
            Kind::PersonExt => 0.82,
            Kind::SystemExt | Kind::SystemDbExt | Kind::SystemQueueExt => 0.62,
            Kind::ContainerExt | Kind::ContainerDbExt | Kind::ContainerQueueExt => 0.75,
            Kind::ComponentExt | Kind::ComponentDbExt | Kind::ComponentQueueExt => 0.86,
        };
        let fill = look.accent.mix(page, share);
        let border = if kind.external() {
            look.accent
        } else {
            look.accent.mix(Color::BLACK, 0.18)
        };
        (fill, border)
    }
}

fn classic_colors(kind: Kind) -> (Color, Color) {
    let (fill, border) = match kind {
        Kind::Person => (0x08427B, 0x073B6F),
        Kind::PersonExt => (0x686868, 0x8A8A8A),
        Kind::System | Kind::SystemDb | Kind::SystemQueue => (0x1168BD, 0x3C7FC0),
        Kind::SystemExt | Kind::SystemDbExt | Kind::SystemQueueExt => (0x999999, 0x8A8A8A),
        Kind::Container | Kind::ContainerDb | Kind::ContainerQueue => (0x438DD5, 0x3C7FC0),
        Kind::ContainerExt | Kind::ContainerDbExt | Kind::ContainerQueueExt => (0xB3B3B3, 0xA6A6A6),
        Kind::Component | Kind::ComponentDb | Kind::ComponentQueue => (0x85BBF0, 0x78A8D8),
        Kind::ComponentExt | Kind::ComponentDbExt | Kind::ComponentQueueExt => (0xCCCCCC, 0xBFBFBF),
    };
    (Color::hex(fill), Color::hex(border))
}

fn boundary_paint(boundary: &Boundary, look: &Look) -> (Paint, Color, Color) {
    let border = boundary
        .border
        .unwrap_or(if look.classic { look.line } else { look.accent });
    let ink = boundary.ink.unwrap_or_else(|| {
        boundary
            .fill
            .map(|fill| readable(fill, look.preferred))
            .unwrap_or_else(|| page_ink(look))
    });
    (boundary.fill, border, ink)
}

fn page_ink(look: &Look) -> Color {
    readable(look.background.unwrap_or(Color::WHITE), look.preferred)
}

fn readable(fill: Color, preferred: Color) -> Color {
    if fill.is_dark() != preferred.is_dark() {
        preferred
    } else if fill.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

fn paint(canvas: &mut Canvas, diagram: &Diagram, laid: &Laid) {
    for (index, placed) in laid.boundaries.iter().enumerate() {
        let (fill, border, _) = boundary_paint(&diagram.boundaries[index], &diagram.look);
        canvas.rect(
            Layer::Frames,
            (placed.rect.x, placed.rect.y, placed.rect.w, placed.rect.h),
            BOUND_ROUND,
            fill,
            Some(Stroke::new(border, 1.25).dashed(7.0, 5.0)),
        );
        paint_lines(canvas, placed.rect.cx(), placed.rect.y + 8.0, &placed.lines);
    }
    for rel in &laid.rels {
        let mut stroke = Stroke::new(rel.color, diagram.look.thickness);
        if rel.dashed {
            stroke = stroke.dashed(7.0, 4.0);
        }
        let mut points = rel.points.clone();
        shorten(&mut points, 9.0, true);
        if rel.bi {
            shorten(&mut points, 9.0, false);
        }
        if points.len() >= 2 {
            canvas.polyline(Layer::Lifelines, &points, stroke);
        }
    }
    for (index, placed) in laid.shapes.iter().enumerate() {
        let style = shape_paint(&diagram.shapes[index], &diagram.look);
        paint_mark(canvas, placed.rect, &style, diagram.look.thickness);
        let top = match style.mark {
            Mark::Figure => placed.rect.y + 56.0,
            Mark::Cylinder => placed.rect.y + 20.0,
            _ => placed.rect.y + 10.0,
        };
        let text_h = stack_height(&placed.lines);
        let room = placed.rect.bottom() - top - 8.0;
        let y = top + (room - text_h).max(0.0) / 2.0;
        paint_lines(canvas, placed.rect.cx(), y, &placed.lines);
    }
    for rel in &laid.rels {
        if let (Some(first), Some(last)) = (rel.points.first(), rel.points.last()) {
            arrow(canvas, *last, prev_point(&rel.points, false), rel.color);
            if rel.bi {
                arrow(canvas, *first, prev_point(&rel.points, true), rel.color);
            }
        }
        let page = diagram.look.background.unwrap_or(Color::WHITE);
        canvas.rect(
            Layer::Front,
            (rel.chip.x, rel.chip.y, rel.chip.w, rel.chip.h),
            4.0,
            Some(page),
            Some(Stroke::new(rel.color.mix(page, 0.45), 1.0)),
        );
        paint_lines(
            canvas,
            rel.chip.x + rel.chip.w / 2.0,
            rel.chip.y + 4.0,
            &rel.lines,
        );
    }
    if let Some(title) = &laid.title {
        let right = laid
            .shapes
            .iter()
            .map(|shape| shape.rect.right())
            .chain(laid.boundaries.iter().map(|boundary| boundary.rect.right()))
            .fold(title.width, f32::max);
        let left = laid
            .shapes
            .iter()
            .map(|shape| shape.rect.x)
            .chain(laid.boundaries.iter().map(|boundary| boundary.rect.x))
            .fold(0.0_f32, f32::min);
        canvas.text(
            Layer::Front,
            title,
            ((left + right) / 2.0, 0.0),
            Align::Center,
        );
    }
}

fn paint_lines(canvas: &mut Canvas, cx: f32, mut y: f32, lines: &[Block]) {
    for line in lines {
        canvas.text(Layer::Front, line, (cx, y), Align::Center);
        y += line.height + 2.0;
    }
}

fn paint_mark(canvas: &mut Canvas, rect: Rect, style: &ShapePaint, width: f32) {
    let mut stroke = Stroke::new(style.border, width);
    if style.dashed {
        stroke = stroke.dashed(6.0, 4.0);
    }
    match style.mark {
        Mark::Figure => paint_figure(canvas, rect, style.fill, stroke),
        Mark::Cylinder => paint_cylinder(canvas, rect, style.fill, stroke),
        Mark::Queue => paint_queue(canvas, rect, style.fill, stroke),
        Mark::Box => canvas.rect(
            Layer::Bars,
            (rect.x, rect.y, rect.w, rect.h),
            ROUND,
            Some(style.fill),
            Some(stroke),
        ),
    }
}

fn paint_figure(canvas: &mut Canvas, rect: Rect, fill: Color, stroke: Stroke) {
    let cx = rect.cx();
    let top = rect.y + 4.0;
    let radius = 12.0;
    canvas.ellipse(
        Layer::Bars,
        (cx, top + radius),
        (radius, radius),
        Some(fill),
        Some(stroke),
    );
    let neck = top + radius * 2.0 + 1.0;
    let d = format!(
        "M{},{} c-16,2 -20,16 -18,26 h36 c2,-10 -2,-24 -18,-26 z",
        crate::svg::n(cx),
        crate::svg::n(neck)
    );
    canvas.path(
        Layer::Bars,
        &d,
        (cx - 22.0, top, cx + 22.0, neck + 28.0),
        Some(fill),
        Some(stroke),
    );
}

fn paint_cylinder(canvas: &mut Canvas, rect: Rect, fill: Color, stroke: Stroke) {
    let ry = 9.0;
    let x = rect.x;
    let y = rect.y + ry;
    let w = rect.w;
    let h = rect.h - ry * 2.0;
    let d = format!(
        "M{},{} A{},{} 0 0 1 {},{} L{},{} A{},{} 0 0 1 {},{} Z",
        crate::svg::n(x),
        crate::svg::n(y),
        crate::svg::n(w / 2.0),
        crate::svg::n(ry),
        crate::svg::n(x + w),
        crate::svg::n(y),
        crate::svg::n(x + w),
        crate::svg::n(y + h),
        crate::svg::n(w / 2.0),
        crate::svg::n(ry),
        crate::svg::n(x),
        crate::svg::n(y + h)
    );
    canvas.path(
        Layer::Bars,
        &d,
        (rect.x, rect.y, rect.right(), rect.bottom()),
        Some(fill),
        Some(stroke),
    );
    let lip = format!(
        "M{},{} A{},{} 0 0 0 {},{}",
        crate::svg::n(x),
        crate::svg::n(y),
        crate::svg::n(w / 2.0),
        crate::svg::n(ry),
        crate::svg::n(x + w),
        crate::svg::n(y)
    );
    canvas.path(
        Layer::Bars,
        &lip,
        (rect.x, rect.y, rect.right(), y + ry),
        None,
        Some(stroke),
    );
}

fn paint_queue(canvas: &mut Canvas, rect: Rect, fill: Color, stroke: Stroke) {
    let bulge = 16.0;
    let x2 = rect.right() - bulge;
    let mid = rect.cy();
    let d = format!(
        "M{},{} L{},{} Q{},{} {},{} L{},{} Z",
        crate::svg::n(rect.x),
        crate::svg::n(rect.y),
        crate::svg::n(x2),
        crate::svg::n(rect.y),
        crate::svg::n(rect.right()),
        crate::svg::n(mid),
        crate::svg::n(x2),
        crate::svg::n(rect.bottom()),
        crate::svg::n(rect.x),
        crate::svg::n(rect.bottom())
    );
    canvas.path(
        Layer::Bars,
        &d,
        (rect.x, rect.y, rect.right(), rect.bottom()),
        Some(fill),
        Some(stroke),
    );
}

fn prev_point(points: &[(f32, f32)], at_start: bool) -> (f32, f32) {
    if at_start {
        points.iter().skip(1).copied().next().unwrap_or(points[0])
    } else {
        points
            .iter()
            .rev()
            .nth(1)
            .copied()
            .unwrap_or(points[points.len() - 1])
    }
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
    let base = (tip.0 - ux * 9.0, tip.1 - uy * 9.0);
    let px = -uy * 3.8;
    let py = ux * 3.8;
    canvas.polygon(
        Layer::Front,
        &[tip, (base.0 + px, base.1 + py), (base.0 - px, base.1 - py)],
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
        let from = points[last - 1];
        pull(&mut points[last], from, len);
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
    use super::{Kind, RelKind, draw, from_mermaid, lay, mermaid_refusal};
    use crate::text::Family;

    fn diagram(source: &str) -> super::Diagram {
        from_mermaid(source).unwrap_or_else(|| panic!("refused {source}"))
    }

    #[test]
    fn reads_a_context_and_draws_the_marks() {
        let source = "\
C4Container
title Shop
Person(customer, \"Customer\", \"A customer\")
System_Boundary(shop, \"Shop\") {
  Container(web, \"Web\", \"Rust\", \"Pages\")
  ContainerDb(db, \"DB\", \"SQL\", \"Rows\")
  ContainerQueue(q, \"Bus\", \"Nats\", \"Events\")
  Rel(web, db, \"Reads\")
}
Rel(customer, web, \"Uses\")
";
        let diagram = diagram(source);
        assert_eq!(
            diagram.title.as_ref().map(|title| title.value.as_str()),
            Some("Shop")
        );
        assert_eq!(diagram.shapes.len(), 4);
        assert_eq!(diagram.shapes[0].kind, Kind::Person);
        assert_eq!(diagram.shapes[2].kind, Kind::ContainerDb);
        assert_eq!(diagram.shapes[3].kind, Kind::ContainerQueue);
        assert_eq!(diagram.boundaries.len(), 1);
        assert_eq!(diagram.shapes[1].parent, Some(0));
        assert_eq!(diagram.rels[1].label, "Uses");
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("Shop")
                && svg.contains("Customer")
                && svg.contains("Uses")
                && svg.contains("<ellipse")
                && svg.contains("<path")
                && svg.contains("#08427B")
                && svg.contains("#438DD5")
                && svg.contains("stroke-dasharray"),
            "{svg}"
        );
        let laid = lay(&diagram, Family::Sans).unwrap();
        assert!(laid.shapes[1].rect.x > laid.boundaries[0].rect.x);
        assert!(laid.shapes[1].rect.right() < laid.boundaries[0].rect.right());
        assert!(clear(&laid), "a label or a line crosses a box");
    }

    #[test]
    fn a_busy_chart_keeps_lines_and_labels_off_the_boxes() {
        let source = "\
C4Container
title Banking
Person(customer, \"Customer\", \"A customer of the bank\")
Person_Ext(auditor, \"Auditor\", \"External reviewer\")
System_Boundary(bank, \"Internet Banking\") {
  Container(web, \"Web\", \"React\", \"Serves the UI\")
  ContainerDb(db, \"Database\", \"SQL\", \"Stores accounts\")
  ContainerQueue(q, \"Bus\", \"Kafka\", \"Events\")
  Component(api, \"API\", \"Rust\", \"Handles requests\")
  Rel(web, db, \"Reads\", \"JDBC\")
  Rel(web, q, \"Publishes\")
  Rel_Back(db, web, \"Migrates\")
}
Rel(customer, web, \"Uses\")
BiRel(auditor, web, \"Reviews\")
";
        let diagram = diagram(source);
        let laid = lay(&diagram, Family::Sans).unwrap();
        let api = &laid.shapes[5].rect;
        let boundary = &laid.boundaries[0].rect;
        assert!(api.x > boundary.x && api.right() < boundary.right());
        assert!(api.y > boundary.y && api.bottom() < boundary.bottom());
        assert!(clear(&laid), "a label or a line crosses a box");
    }

    #[test]
    fn a_light_component_keeps_dark_text_and_a_back_link_is_dashed() {
        let component = draw(&diagram(
            "C4Component\nComponent(api, \"API\", \"Rust\", \"Handles\")\n",
        ))
        .unwrap();
        assert!(
            component.contains("#85BBF0")
                && component.contains("fill=\"#333333\"")
                && component.contains("API"),
            "{component}"
        );
        let back = draw(&diagram(
            "C4Context\nPerson(a, \"A\")\nSystem(s, \"S\")\nRel_Back(a, s, \"Back\")\nBiRel(a, s, \"Both\")\n",
        ))
        .unwrap();
        assert!(
            back.contains("stroke-dasharray") && back.matches("<polygon").count() >= 3,
            "{back}"
        );
        let up = diagram("C4Context\nPerson(a, \"A\")\nPerson(b, \"B\")\nRel_Up(a, b, \"Up\")\n");
        assert_eq!(up.rels[0].kind, RelKind::Up);
        let laid = lay(&up, Family::Sans).unwrap();
        assert!(laid.rels[0].points[1].1 < laid.rels[0].points[0].1);
    }

    #[test]
    fn a_dark_page_steps_the_accent_instead_of_the_classic_blues() {
        let mut diagram =
            diagram("C4Context\nPerson(a, \"A\")\nSystem(s, \"S\")\nComponent(c, \"C\")\n");
        diagram.look.classic = false;
        diagram.look.accent = super::Color::hex(0xE68E0D);
        diagram.look.background = Some(super::Color::hex(0x121212));
        diagram.look.preferred = super::Color::hex(0xBEBEBE);
        diagram.look.line = super::Color::hex(0xE68E0D);
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("#E68E0D")
                && svg.contains("#AB6B0E")
                && svg.contains("#563A10")
                && svg.contains("#BEBEBE")
                && !svg.contains("#08427B")
                && !svg.contains("#1168BD")
                && !svg.contains("#438DD5")
                && !svg.contains("#85BBF0"),
            "{svg}"
        );
    }

    #[test]
    fn a_sprite_a_boundary_and_a_bad_line_are_named() {
        let sprite =
            mermaid_refusal("C4Context\nPerson(admin, \"Admin\", $sprite=users)\n").unwrap();
        assert_eq!(sprite, (1, "a sprite"));
        let open = mermaid_refusal("C4Context\nBoundary(b, \"B\") {\nPerson(a, \"A\")\n").unwrap();
        assert_eq!(open.1, "a boundary");
        let rel =
            mermaid_refusal("C4Context\nPerson(a, \"A\")\nRel(a, missing, \"Uses\")\n").unwrap();
        assert_eq!(rel.1, "a relationship");
        let color =
            mermaid_refusal("C4Context\nPerson(a, \"A\")\nUpdateElementStyle(a, not-a-color)\n")
                .unwrap();
        assert_eq!(color.1, "a color");
        let style = mermaid_refusal(
            "C4Context\nPerson(a, \"A\")\nUpdateElementStyle(a, #fff, $opacity=1)\n",
        )
        .unwrap();
        assert_eq!(style.1, "a style");
        let layout = mermaid_refusal("C4Context\nUpdateLayoutConfig(0)\n").unwrap();
        assert_eq!(layout.1, "a layout");
        let unknown = mermaid_refusal("C4Context\ndirection LR\n").unwrap();
        assert_eq!(unknown.1, "an unknown line");
        let element = mermaid_refusal("C4Context\nFoo(a, \"A\")\n").unwrap();
        assert_eq!(element.1, "an element");
        assert!(from_mermaid("sequenceDiagram\nA->B: hi\n").is_none());
        assert!(mermaid_refusal("sequenceDiagram\nA->B: hi\n").is_none());
        let styled = diagram("C4Context\nPerson(a, \"Admin\")\nUpdateElementStyle(a, #112233)\n");
        assert_eq!(styled.shapes[0].fill, Some(super::Color::hex(0x112233)));
        let svg = draw(&styled).unwrap();
        assert!(svg.contains("#112233"), "{svg}");
    }

    fn clear(laid: &super::Laid) -> bool {
        let headers = super::header_bands(&laid.boundaries);
        for (index, rel) in laid.rels.iter().enumerate() {
            for shape in &laid.shapes {
                if rel.chip.inflate(1.0).intersects(shape.rect) {
                    return false;
                }
            }
            for header in &headers {
                if rel.chip.inflate(1.0).intersects(*header) {
                    return false;
                }
            }
            for (other_index, other) in laid.rels.iter().enumerate() {
                if index != other_index && rel.chip.inflate(1.0).intersects(other.chip) {
                    return false;
                }
            }
            for pair in rel.points.windows(2) {
                let hits_shape = laid
                    .shapes
                    .iter()
                    .any(|shape| super::segment_hits(pair[0], pair[1], shape.rect));
                let hits_header = headers
                    .iter()
                    .any(|header| super::segment_hits(pair[0], pair[1], *header));
                if hits_shape || hits_header {
                    return false;
                }
            }
        }
        true
    }
}
