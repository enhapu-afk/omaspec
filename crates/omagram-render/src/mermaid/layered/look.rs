//! Colors and lines: the theme's palette, `classDef`, `style` and
//! `linkStyle`. A color this cannot read leaves the diagram to mmdr.

use mermaid_rs_renderer::ir::{EdgeStyleOverride, NodeStyle};
use mermaid_rs_renderer::{EdgeStyle, Graph, Theme};
use omagram_uml::{Color, Paint, Stroke, parse_color};

pub(super) struct Palette {
    pub size: f32,
    pub fill: Paint,
    pub text: Color,
    pub border: Color,
    pub line: Color,
    pub background: Color,
    pub cluster_fill: Paint,
    pub cluster_border: Color,
    pub note_fill: Paint,
    pub note_border: Color,
    /// Whether an edge label covers the lines under it, in the color of
    /// what it sits on, so another line does not cut its text.
    pub mask_labels: bool,
}

pub(super) fn palette(theme: &Theme, flowchart: bool) -> Option<Palette> {
    let fill = must_color(&theme.primary_color)?;
    let text = must_color(&theme.primary_text_color)?;
    let border = must_color(&theme.primary_border_color)?;
    let line = must_color(&theme.line_color)?;
    let background = must_color(&theme.background)?;
    let note_fill = must_color(&theme.sequence_note_fill)?;
    let note_border = must_color(&theme.sequence_note_border)?;
    // A cluster color mmdr stores as `rgba(...)` cannot be read. The frame
    // then uses the state colors; the diagram is still drawn.
    Some(Palette {
        size: theme.font_size.clamp(4.0, 96.0),
        fill: Some(fill),
        text,
        border,
        line,
        background,
        cluster_fill: Some(must_color(&theme.cluster_background).unwrap_or(fill)),
        cluster_border: must_color(&theme.cluster_border).unwrap_or(border),
        note_fill: Some(note_fill),
        note_border,
        mask_labels: flowchart,
    })
}

enum Ink {
    Color(Color),
    Clear,
}

fn ink(value: &str) -> Option<Ink> {
    let v = value.trim();
    if v.eq_ignore_ascii_case("none") || v.eq_ignore_ascii_case("transparent") {
        return Some(Ink::Clear);
    }
    match parse_color(v).ok()? {
        Some(color) => Some(Ink::Color(color)),
        None => Some(Ink::Clear),
    }
}

fn must_color(value: &str) -> Option<Color> {
    match ink(value)? {
        Ink::Color(color) => Some(color),
        Ink::Clear => None,
    }
}

pub(super) struct Look {
    pub fill: Paint,
    pub border: Stroke,
    pub text: Color,
    /// The color of a mark with no outline: a start dot, a bar.
    pub mark: Color,
}

pub(super) fn look_for(style: &NodeStyle, fill: Paint, border: Color, text: Color) -> Option<Look> {
    let own = match &style.fill {
        None => None,
        Some(value) => match ink(value)? {
            Ink::Color(color) => Some(color),
            Ink::Clear => None,
        },
    };
    let fill = if style.fill.is_some() { own } else { fill };
    let border_color = match style.stroke.as_ref().or(style.line_color.as_ref()) {
        None => border,
        Some(value) => match ink(value)? {
            Ink::Color(color) => color,
            Ink::Clear => return None,
        },
    };
    let text = match &style.text_color {
        None => text,
        Some(value) => match ink(value)? {
            Ink::Color(color) => color,
            Ink::Clear => return None,
        },
    };
    let width = match style.stroke_width {
        None => 1.0,
        Some(width) if width.is_finite() && width >= 0.0 => width,
        Some(_) => return None,
    };
    let dash = match &style.stroke_dasharray {
        None => None,
        Some(value) => dash_of(value)?,
    };
    let mut border = Stroke::new(border_color, width);
    border.dash = dash;
    Some(Look {
        fill,
        border,
        text,
        mark: own.unwrap_or(border_color),
    })
}

fn dash_of(value: &str) -> Option<Option<(f32, f32)>> {
    let parts: Vec<&str> = value
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|part| !part.is_empty())
        .collect();
    if value.trim().eq_ignore_ascii_case("none") {
        return Some(None);
    }
    match parts.as_slice() {
        [] => Some(None),
        [one] => Some(Some((one.parse().ok()?, one.parse().ok()?))),
        [on, off, ..] => Some(Some((on.parse().ok()?, off.parse().ok()?))),
    }
}

/// The style of a node or subgraph: `classDef default` (in a flowchart,
/// where Mermaid gives it to every node), its classes, then its own
/// `style` lines.
pub(super) fn styled_node(graph: &Graph, id: &str, flowchart: bool) -> NodeStyle {
    let mut style = NodeStyle::default();
    if flowchart && let Some(def) = graph.class_defs.get("default") {
        overlay_node(&mut style, def);
    }
    apply_classes(&mut style, graph, graph.node_classes.get(id));
    apply_classes(&mut style, graph, graph.subgraph_classes.get(id));
    if let Some(add) = graph.node_styles.get(id) {
        overlay_node(&mut style, add);
    }
    if let Some(add) = graph.subgraph_styles.get(id) {
        overlay_node(&mut style, add);
    }
    style
}

fn apply_classes(style: &mut NodeStyle, graph: &Graph, names: Option<&Vec<String>>) {
    let Some(names) = names else {
        return;
    };
    for name in names {
        if let Some(def) = graph.class_defs.get(name) {
            overlay_node(style, def);
        }
    }
}

fn overlay_node(style: &mut NodeStyle, add: &NodeStyle) {
    if add.fill.is_some() {
        style.fill.clone_from(&add.fill);
    }
    if add.stroke.is_some() {
        style.stroke.clone_from(&add.stroke);
    }
    if add.text_color.is_some() {
        style.text_color.clone_from(&add.text_color);
    }
    if add.stroke_width.is_some() {
        style.stroke_width = add.stroke_width;
    }
    if add.stroke_dasharray.is_some() {
        style.stroke_dasharray.clone_from(&add.stroke_dasharray);
    }
    if add.line_color.is_some() {
        style.line_color.clone_from(&add.line_color);
    }
}

/// The line of edge `index` and the color of its label.
pub(super) fn edge_look(graph: &Graph, index: usize, palette: &Palette) -> Option<(Stroke, Color)> {
    let mut style = EdgeStyleOverride::default();
    if let Some(add) = &graph.edge_style_default {
        overlay_edge(&mut style, add);
    }
    if let Some(add) = graph.edge_styles.get(&index) {
        overlay_edge(&mut style, add);
    }
    let edge = &graph.edges[index];
    let color = match &style.stroke {
        None => palette.line,
        Some(value) => match ink(value)? {
            Ink::Color(color) => color,
            Ink::Clear => return None,
        },
    };
    let width = match style.stroke_width {
        Some(width) if width.is_finite() && width >= 0.0 => width,
        Some(_) => return None,
        None if edge.style == EdgeStyle::Thick => 2.0,
        None => 1.0,
    };
    let dash = match &style.dasharray {
        Some(value) => dash_of(value)?,
        None if edge.style == EdgeStyle::Dotted => Some((1.5, 3.0)),
        None => None,
    };
    let label = match &style.label_color {
        None => palette.text,
        Some(value) => match ink(value)? {
            Ink::Color(color) => color,
            Ink::Clear => return None,
        },
    };
    let mut stroke = Stroke::new(color, width);
    stroke.dash = dash;
    Some((stroke, label))
}

fn overlay_edge(style: &mut EdgeStyleOverride, add: &EdgeStyleOverride) {
    if add.stroke.is_some() {
        style.stroke.clone_from(&add.stroke);
    }
    if add.stroke_width.is_some() {
        style.stroke_width = add.stroke_width;
    }
    if add.dasharray.is_some() {
        style.dasharray.clone_from(&add.dasharray);
    }
    if add.label_color.is_some() {
        style.label_color.clone_from(&add.label_color);
    }
}
