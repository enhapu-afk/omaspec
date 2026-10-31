//! Mermaid through mmdr, and Omaspec's own drawers where they draw the
//! diagram.

mod architecture;
mod block;
mod c4;
mod class;
mod er;
mod gantt;
mod journey;
mod layered;
mod mindmap;
mod pie;
mod repair;
mod sequence;
mod state;
mod timeline;

use std::borrow::Cow;
use std::panic::{AssertUnwindSafe, catch_unwind};

use mermaid_rs_renderer::{
    DiagramKind, ParseError, RenderOptions, Theme, compute_layout, parse_mermaid_strict, render_svg,
};
use omaspec_core::lang::strip_mermaid_fence;

use crate::{Engine, Map, RenderError};

/// Why Omaspec left a Mermaid diagram to mmdr. The diagram still draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MermaidDeclined {
    /// Zero-based line in the source as given, when one line is to blame.
    pub line: Option<usize>,
    /// What an agent can change so Omaspec draws the diagram itself.
    pub reason: String,
}

/// Whether Omaspec draws this Mermaid diagram.
///
/// `Ok(None)` means Omaspec draws it. `Ok(Some(_))` means mmdr draws it
/// instead. `Err` means nothing could be drawn; the error is the same one
/// [`render`] returns.
pub fn coverage(source: &str) -> Result<Option<MermaidDeclined>, RenderError> {
    draw(source).map(|drawn| drawn.declined)
}

struct Drawn {
    svg: String,
    map: Map,
    declined: Option<MermaidDeclined>,
}

/// The SVG for a Mermaid source. A ```` ```mermaid ```` fence is removed
/// first; error lines refer to the source as given.
#[cfg(test)]
pub fn render(source: &str) -> Result<String, RenderError> {
    draw(source).map(|drawn| drawn.svg)
}

/// The SVG for a Mermaid source, the engine that drew it, and where its
/// messages are in it, on the lines of the source as given.
pub fn render_mapped(source: &str) -> Result<(String, Engine, Map), RenderError> {
    draw(source).map(|drawn| {
        let engine = match drawn.declined {
            None => Engine::NativeMermaid,
            Some(_) => Engine::Mermaid,
        };
        (drawn.svg, engine, drawn.map)
    })
}

/// Blocks nested deeper than this are refused: no diagram needs them, and
/// the layouts go into nested states and subgraphs by recursion, so a
/// file nested thousands deep would overflow the stack and end the
/// program.
const MAX_NESTING: usize = 100;

fn draw(source: &str) -> Result<Drawn, RenderError> {
    let fenced = strip_mermaid_fence(source);
    // `fenced` is a slice of `source`; lines before it belong to the fence.
    let offset = source
        .get(..(fenced.as_ptr() as usize).saturating_sub(source.as_ptr() as usize))
        .map_or(0, |before| before.matches('\n').count());
    if let Some(line) = too_deep(fenced) {
        return Err(RenderError::Diagram {
            engine: Engine::Mermaid,
            line: Some(line + offset),
            message: format!("blocks nest deeper than {MAX_NESTING} levels here"),
        });
    }
    if let Some(line) = init_too_deep(fenced) {
        return Err(RenderError::Diagram {
            engine: Engine::Mermaid,
            line: Some(line + offset),
            message: format!("a `%%{{init}}%%` value nests deeper than {MAX_NESTING} levels"),
        });
    }

    // Lines by hand (docs/adr/0016), on the lines of the fenced text.
    let hands = omaspec_uml::hand::numbered(fenced);
    let joined = one_line_init(fenced);
    // A note over several lines (`note right of X` … `end note`) is not a
    // construct mmdr reads. Joined onto one line, the same note is drawn.
    let folded = fold_multiline_notes(&joined);
    let body = folded.as_ref();

    // mmdr is young and has panicking code paths; a bad diagram must not
    // take the editor down with it.
    let result = catch_unwind(AssertUnwindSafe(|| -> Result<Drawn, ParseError> {
        let mut options = RenderOptions::default();
        let parsed = parse_mermaid_strict(body)?;
        if let Some(init) = &parsed.init_config {
            apply_init(&mut options, init);
        }
        // Sequence diagrams, class diagrams, mind maps, Gantt charts, pie
        // charts, user journeys, timelines, ER diagrams, C4 diagrams,
        // architecture diagrams and block diagrams use their engines.
        // State diagrams and flowcharts lay out on the shared graph.
        // Anything a drawer does not accept keeps mmdr's layout, and a
        // state diagram the repair.
        if let Some((svg, map)) = sequence::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        if let Some((svg, map)) = class::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        if let Some((svg, map)) = mindmap::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        if let Some((svg, map)) = gantt::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        if let Some((svg, map)) = pie::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        if let Some((svg, map)) = journey::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        if let Some((svg, map)) = timeline::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        if let Some((svg, map)) = er::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        if let Some((svg, map)) = c4::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        if let Some((svg, map)) = architecture::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        if let Some((svg, map)) = block::try_draw(&parsed.graph, &options.theme, body) {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        // A state diagram Omaspec reads is drawn from that reading, the
        // one its checks and its play go by, even where mmdr lays it out.
        let chart = (parsed.graph.kind == DiagramKind::State)
            .then(|| omaspec_uml::state::mermaid::read(fenced))
            .flatten();
        if let Some(chart) = chart {
            let (graph, lines) = state::graph(&chart);
            // Mermaid curves a state diagram's lines always.
            let curve = omaspec_uml::curve::RADIUS;
            if let Some((svg, map)) =
                layered::try_draw(&graph, &options.theme, body, Some(&lines), &hands, curve)
            {
                return Ok(Drawn {
                    svg,
                    map: map.shifted(offset),
                    declined: None,
                });
            }
            let mut layout = compute_layout(&graph, &options.theme, &options.layout);
            repair::repair(&mut layout);
            return Ok(Drawn {
                svg: render_svg(&layout, &options.theme, &options.layout),
                map: Map::default(),
                declined: Some(declined_by(graph.kind, body, source, offset)),
            });
        }
        let curve = flowchart_curve(parsed.init_config.as_ref());
        if let Some((svg, map)) =
            layered::try_draw(&parsed.graph, &options.theme, body, None, &hands, curve)
        {
            return Ok(Drawn {
                svg,
                map: map.shifted(offset),
                declined: None,
            });
        }
        let mut layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
        repair::repair(&mut layout);
        Ok(Drawn {
            svg: render_svg(&layout, &options.theme, &options.layout),
            map: Map::default(),
            declined: Some(declined_by(parsed.graph.kind, body, source, offset)),
        })
    }));
    match result {
        Ok(Ok(drawn)) => Ok(drawn),
        Ok(Err(err)) => Err(RenderError::Diagram {
            engine: Engine::Mermaid,
            line: error_line(&err).map(|l| l + offset),
            message: err.to_string(),
        }),
        Err(_) => Err(RenderError::Engine {
            engine: Engine::Mermaid,
            message: "mmdr crashed on this diagram".into(),
        }),
    }
}

/// Why the native drawers left this diagram to mmdr.
fn declined_by(kind: DiagramKind, body: &str, source: &str, offset: usize) -> MermaidDeclined {
    match kind {
        DiagramKind::Sequence => {
            if let Some((line, reason)) = sequence::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this sequence diagram; mmdr does".into(),
                }
            }
        }
        DiagramKind::Class => {
            if let Some((line, reason)) = class::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this class diagram; mmdr does".into(),
                }
            }
        }
        DiagramKind::Mindmap => {
            if let Some((line, reason)) = mindmap::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this mind map; mmdr does".into(),
                }
            }
        }
        DiagramKind::Gantt => {
            if let Some((line, reason)) = gantt::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this Gantt chart; mmdr does".into(),
                }
            }
        }
        DiagramKind::Pie => {
            if let Some((line, reason)) = pie::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this pie chart; mmdr does".into(),
                }
            }
        }
        DiagramKind::Journey => {
            if let Some((line, reason)) = journey::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this user journey; mmdr does".into(),
                }
            }
        }
        DiagramKind::Timeline => {
            if let Some((line, reason)) = timeline::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this timeline; mmdr does".into(),
                }
            }
        }
        DiagramKind::Er => {
            if let Some((line, reason)) = er::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this ER diagram; mmdr does".into(),
                }
            }
        }
        DiagramKind::C4 => {
            if let Some((line, reason)) = c4::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this C4 diagram; mmdr does".into(),
                }
            }
        }
        DiagramKind::Architecture => {
            if let Some((line, reason)) = architecture::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this architecture diagram; mmdr does".into(),
                }
            }
        }
        DiagramKind::Block => {
            if let Some((line, reason)) = block::named_decline(body) {
                MermaidDeclined {
                    line: Some(line + offset),
                    reason,
                }
            } else {
                MermaidDeclined {
                    line: None,
                    reason: "Omaspec does not draw this block diagram; mmdr does".into(),
                }
            }
        }
        DiagramKind::State => MermaidDeclined {
            line: None,
            reason: "Omaspec does not draw this state diagram; mmdr does".into(),
        },
        DiagramKind::Flowchart => MermaidDeclined {
            line: None,
            reason: "Omaspec does not draw this flowchart; mmdr does".into(),
        },
        _ => {
            let found = omaspec_core::classify(source, omaspec_core::Language::Mermaid);
            let reason = if found.name.is_empty() {
                "This diagram is not drawn by Omaspec; mmdr draws it".into()
            } else {
                format!(
                    "{name}s are not drawn by Omaspec; mmdr draws them",
                    name = found.name
                )
            };
            MermaidDeclined { line: None, reason }
        }
    }
}

/// The first line where blocks nest deeper than [`MAX_NESTING`]: `{` … `}`
/// (states, namespaces) and `subgraph`, `block`, `loop`, `alt` and the
/// other sequence blocks … `end`.
fn too_deep(body: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (no, raw) in body.lines().enumerate() {
        let line = raw.trim();
        if line.starts_with("%%") {
            continue;
        }
        let first = line
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let opens = line.ends_with('{')
            || first.starts_with("block:")
            || matches!(
                first.as_str(),
                "subgraph"
                    | "block"
                    | "loop"
                    | "alt"
                    | "opt"
                    | "par"
                    | "critical"
                    | "break"
                    | "rect"
                    | "box"
            );
        if opens {
            depth += 1;
            if depth > MAX_NESTING {
                return Some(no);
            }
        } else if line.starts_with('}') || first == "end" {
            depth = depth.saturating_sub(1);
        }
    }
    None
}

/// The line where a `%%{…}%%` directive's values nest deeper than
/// [`MAX_NESTING`]. mmdr reads them by recursion, without a limit.
fn init_too_deep(body: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut inside = false;
    let mut quote = None;
    for (no, line) in body.lines().enumerate() {
        if !inside {
            if !line.trim_start().starts_with("%%{") {
                continue;
            }
            inside = true;
            depth = 0;
        }
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            match (quote, c) {
                (Some(_), '\\') => {
                    chars.next();
                }
                (Some(q), _) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '{' | '[') => {
                    depth += 1;
                    if depth > MAX_NESTING {
                        return Some(no);
                    }
                }
                (None, '}' | ']') => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        if quote.is_none() && line.contains("}%%") {
            inside = false;
        }
    }
    None
}

/// What a `%%{init: …}%%` directive sets: the theme by name, its
/// variables, the flowchart's spacing. mmdr parses the directive but
/// applies it only in its own command line tool.
fn apply_init(options: &mut RenderOptions, init: &serde_json::Value) {
    if let Some(theme) = init
        .get("theme")
        .and_then(|v| v.as_str())
        .and_then(Theme::from_name)
    {
        options.theme = theme;
    }
    if let Some(vars) = init.get("themeVariables") {
        let theme = &mut options.theme;
        let colors: [(&str, &mut String); 18] = [
            ("primaryColor", &mut theme.primary_color),
            ("primaryTextColor", &mut theme.primary_text_color),
            ("primaryBorderColor", &mut theme.primary_border_color),
            ("lineColor", &mut theme.line_color),
            ("secondaryColor", &mut theme.secondary_color),
            ("tertiaryColor", &mut theme.tertiary_color),
            ("textColor", &mut theme.text_color),
            ("background", &mut theme.background),
            ("edgeLabelBackground", &mut theme.edge_label_background),
            ("clusterBkg", &mut theme.cluster_background),
            ("clusterBorder", &mut theme.cluster_border),
            ("actorBkg", &mut theme.sequence_actor_fill),
            ("actorBorder", &mut theme.sequence_actor_border),
            ("actorLine", &mut theme.sequence_actor_line),
            ("noteBkg", &mut theme.sequence_note_fill),
            ("noteBorderColor", &mut theme.sequence_note_border),
            ("activationBkgColor", &mut theme.sequence_activation_fill),
            (
                "activationBorderColor",
                &mut theme.sequence_activation_border,
            ),
        ];
        for (name, field) in colors {
            if let Some(value) = vars.get(name).and_then(|v| v.as_str()) {
                *field = value.to_owned();
            }
        }
        if let Some(family) = vars.get("fontFamily").and_then(|v| v.as_str()) {
            theme.font_family = family.to_owned();
        }
        // `16` or `"16px"`.
        let size = vars.get("fontSize").and_then(|v| {
            v.as_f64().or_else(|| {
                v.as_str()?
                    .trim()
                    .trim_end_matches("px")
                    .trim()
                    .parse()
                    .ok()
            })
        });
        if let Some(size) = size.filter(|s| (4.0..=96.0).contains(s)) {
            theme.font_size = size as f32;
        }
        apply_pie_variables(theme, vars);
    }
    if let Some(flowchart) = init.get("flowchart") {
        if let Some(v) = flowchart.get("nodeSpacing").and_then(|v| v.as_f64()) {
            options.layout.node_spacing = v as f32;
        }
        if let Some(v) = flowchart.get("rankSpacing").and_then(|v| v.as_f64()) {
            options.layout.rank_spacing = v as f32;
        }
    }
}

/// How far a flowchart's corners curve: Mermaid's own default curves them;
/// `"curve": "linear"` and the steps of `%%{init: {"flowchart": …}}%%`
/// keep straight runs and right angles.
fn flowchart_curve(init: Option<&serde_json::Value>) -> f32 {
    let curve = init
        .and_then(|init| init.get("flowchart")?.get("curve")?.as_str())
        .unwrap_or("basis");
    match curve {
        "linear" | "step" | "stepBefore" | "stepAfter" => 0.0,
        _ => omaspec_uml::curve::RADIUS,
    }
}

/// `pie1`..`pie12` and the pie text, stroke and opacity variables.
/// A number outside the range the drawer accepts is ignored, so a bad
/// init does not replace the theme's own value.
fn apply_pie_variables(theme: &mut Theme, vars: &serde_json::Value) {
    for (index, field) in theme.pie_colors.iter_mut().enumerate() {
        let key = format!("pie{}", index + 1);
        if let Some(value) = vars.get(&key).and_then(|value| value.as_str()) {
            *field = value.to_owned();
        }
    }
    let colors: [(&str, &mut String); 5] = [
        ("pieTitleTextColor", &mut theme.pie_title_text_color),
        ("pieSectionTextColor", &mut theme.pie_section_text_color),
        ("pieLegendTextColor", &mut theme.pie_legend_text_color),
        ("pieStrokeColor", &mut theme.pie_stroke_color),
        ("pieOuterStrokeColor", &mut theme.pie_outer_stroke_color),
    ];
    for (name, field) in colors {
        if let Some(value) = vars.get(name).and_then(|value| value.as_str()) {
            *field = value.to_owned();
        }
    }
    let numbers: [(&str, &mut f32); 6] = [
        ("pieTitleTextSize", &mut theme.pie_title_text_size),
        ("pieSectionTextSize", &mut theme.pie_section_text_size),
        ("pieLegendTextSize", &mut theme.pie_legend_text_size),
        ("pieStrokeWidth", &mut theme.pie_stroke_width),
        ("pieOuterStrokeWidth", &mut theme.pie_outer_stroke_width),
        ("pieOpacity", &mut theme.pie_opacity),
    ];
    for (name, field) in numbers {
        if let Some(value) = json_number(vars.get(name)).filter(|value| pie_number_ok(name, *value))
        {
            *field = value;
        }
    }
}

fn json_number(value: Option<&serde_json::Value>) -> Option<f32> {
    let value = value?;
    let number = value.as_f64().or_else(|| {
        value
            .as_str()?
            .trim()
            .trim_end_matches("px")
            .trim()
            .parse()
            .ok()
    })?;
    number.is_finite().then_some(number as f32)
}

fn pie_number_ok(name: &str, value: f32) -> bool {
    match name {
        "pieOpacity" => (0.0..=1.0).contains(&value),
        "pieStrokeWidth" | "pieOuterStrokeWidth" => (0.0..=20.0).contains(&value),
        _ => (4.0..=100.0).contains(&value),
    }
}

/// mmdr reads `%%{init: …}%%` only on one line. A directive spanning lines
/// is joined into its first line, followed by blank lines so that every
/// later line keeps its number.
fn one_line_init(body: &str) -> Cow<'_, str> {
    let lines: Vec<&str> = body.split('\n').collect();
    let Some(start) = lines.iter().position(|l| l.trim_start().starts_with("%%{")) else {
        return Cow::Borrowed(body);
    };
    if lines[start].contains("}%%") {
        return Cow::Borrowed(body);
    }
    let Some(end) = (start + 1..lines.len()).find(|&i| lines[i].contains("}%%")) else {
        return Cow::Borrowed(body);
    };
    let joined: Vec<&str> = lines[start..=end].iter().map(|l| l.trim()).collect();
    let mut out: Vec<String> = lines[..start].iter().map(|l| (*l).to_owned()).collect();
    out.push(joined.join(" "));
    out.extend(std::iter::repeat_n(String::new(), end - start));
    out.extend(lines[end + 1..].iter().map(|l| (*l).to_owned()));
    Cow::Owned(out.join("\n"))
}

/// A note written over several lines, the way Mermaid allows it:
///
/// ```text
/// note right of Idle
///     waiting
/// end note
/// ```
///
/// mmdr reads a note only when the label sits on the header (`note right
/// of Idle : waiting`). The lines of the block would otherwise be lost, and
/// in a state diagram each of them becomes a state. The label is joined
/// with `<br/>`, which mmdr already turns into line breaks, and the block
/// keeps its line count so a later error still points at the right line.
fn fold_multiline_notes(body: &str) -> Cow<'_, str> {
    let lines: Vec<&str> = body.split('\n').collect();
    let mut out = Vec::with_capacity(lines.len());
    let mut changed = false;
    let mut i = 0;
    while i < lines.len() {
        let Some(header) = note_header(lines[i]) else {
            out.push(lines[i].to_owned());
            i += 1;
            continue;
        };
        let Some(end) = (i + 1..lines.len()).find(|&j| is_end_note(lines[j])) else {
            out.push(lines[i].to_owned());
            i += 1;
            continue;
        };
        let label = lines[i + 1..end]
            .iter()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("<br/>");
        if label.is_empty() {
            out.push(lines[i].to_owned());
            i += 1;
            continue;
        }
        out.push(format!("{header} : {label}"));
        out.extend(std::iter::repeat_n(String::new(), end - i));
        changed = true;
        i = end + 1;
    }
    if changed {
        Cow::Owned(out.join("\n"))
    } else {
        Cow::Borrowed(body)
    }
}

/// The header of a note whose label is not on this line: `note right of X`,
/// `note left of X`, `note over A, B`, also with a trailing `:`.
fn note_header(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let lower = trimmed.to_ascii_lowercase();
    let rest = lower.strip_prefix("note ")?.trim_start();
    let after = rest
        .strip_prefix("right of ")
        .or_else(|| rest.strip_prefix("left of "))
        .or_else(|| rest.strip_prefix("over "))?;
    if after.trim().is_empty() {
        return None;
    }
    match trimmed.split_once(':') {
        Some((header, label)) if label.trim().is_empty() => Some(header.trim_end()),
        Some(_) => None,
        None => Some(trimmed),
    }
}

fn is_end_note(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("end note")
}

/// Zero-based line of a parse error, if it has one.
fn error_line(err: &ParseError) -> Option<usize> {
    let line = match err {
        ParseError::UnknownParticipant { line, .. } => *line,
        ParseError::UnclosedSubgraph { opened_at } => *opened_at,
        ParseError::UnexpectedToken { line, .. } => *line,
        ParseError::InvalidDirective { line, .. } => *line,
        _ => return None,
    };
    (line as usize).checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_sequence() {
        let svg = render("sequenceDiagram\n  Alice->>Bob: hi\n").unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("Alice"));
    }

    /// A flowchart curves its lines as Mermaid does, unless its `curve` is
    /// linear or a step.
    #[test]
    fn flowchart_lines_curve_unless_linear() {
        let body = "flowchart TD\n  A --> B\n  A --> C\n  B --> D\n  C --> D\n";
        let curved = render(body).unwrap();
        assert!(curves(&curved), "{curved}");
        let linear = render(&format!(
            "%%{{init: {{\"flowchart\": {{\"curve\": \"linear\"}}}}}}%%\n{body}"
        ))
        .unwrap();
        assert!(!curves(&linear) && linear.contains("<polyline"), "{linear}");
    }

    /// Whether a path of `svg` turns in a curve.
    fn curves(svg: &str) -> bool {
        svg.split("<path d=\"")
            .skip(1)
            .any(|rest| rest.split('"').next().is_some_and(|d| d.contains(" C")))
    }

    #[test]
    fn renders_inside_a_fence() {
        assert!(render("```mermaid\npie\n  \"a\": 1\n```\n").is_ok());
    }

    #[test]
    fn joins_a_multi_line_init() {
        let body = "%%{init: {\n  \"theme\": \"forest\"\n}}%%\nflowchart LR\n  A --> B";
        let joined = one_line_init(body);
        assert_eq!(
            joined,
            "%%{init: { \"theme\": \"forest\" }}%%\n\n\nflowchart LR\n  A --> B"
        );
        assert!(render(body).is_ok());
    }

    #[test]
    fn applies_the_init_theme_and_its_variables() {
        let plain = render("flowchart LR\n  A --> B\n").unwrap();
        let dark = render("%%{init: {'theme': 'dark'}}%%\nflowchart LR\n  A --> B\n").unwrap();
        assert_ne!(plain, dark);
        let colored = render(
            "%%{init: {\"themeVariables\": {\"primaryColor\": \"#123456\", \"background\": \"#0a0b0c\", \"fontSize\": \"20px\"}}}%%\nflowchart LR\n  A --> B\n",
        )
        .unwrap();
        let lower = colored.to_ascii_lowercase();
        assert!(
            lower.contains("#123456") && lower.contains("#0a0b0c"),
            "{colored}"
        );
        assert!(colored.contains("20"));
        let pie = render(
            "%%{init: {\"themeVariables\": {\"pie1\": \"#112233\", \"pieOpacity\": 1}}}%%\npie\n\"A\" : 1\n",
        )
        .unwrap();
        assert!(pie.to_ascii_lowercase().contains("#112233"), "{pie}");
    }

    #[test]
    fn reports_errors_with_lines() {
        let err = render("flowchart TD\n  subgraph S\n  A --> B\n").unwrap_err();
        let RenderError::Diagram { line, .. } = err else {
            panic!("{err:?}");
        };
        assert_eq!(line, Some(1));
        let fenced = render("```mermaid\nflowchart TD\n  subgraph S\n  A --> B\n```").unwrap_err();
        let RenderError::Diagram { line, .. } = fenced else {
            panic!("{fenced:?}");
        };
        assert_eq!(line, Some(2));
    }

    #[test]
    fn says_whether_omaspec_draws_it() {
        assert!(
            coverage("sequenceDiagram\nAlice->>Bob: hi\n")
                .unwrap()
                .is_none()
        );
        assert!(
            coverage("stateDiagram-v2\n[*] --> Idle\n")
                .unwrap()
                .is_none()
        );
        assert!(coverage("flowchart LR\n  A --> B\n").unwrap().is_none());
        assert!(coverage("classDiagram\nclass Animal\n").unwrap().is_none());
        let svg = render("classDiagram\nclass Animal\nclass Duck\nAnimal <|-- Duck\n").unwrap();
        assert!(svg.contains("Animal") && svg.contains("Duck"), "{svg}");
        let library = include_str!("../../../examples/library.mmd");
        assert!(coverage(library).unwrap().is_none());
        let svg = render(library).unwrap();
        assert!(svg.contains("Library") && svg.contains("holds"), "{svg}");

        let mind = include_str!("../../omaspec-core/tests/fixtures/mindmap.mmd");
        assert!(coverage(mind).unwrap().is_none());
        let svg = render(mind).unwrap();
        assert!(svg.contains("Omarchy") && svg.contains("Ghostty"), "{svg}");
        assert!(coverage("mindmap\n  )cloud(\n").unwrap().is_none());
        let svg = render("mindmap\n  )cloud(\n    ))bang((\n").unwrap();
        assert!(
            svg.contains("cloud") && svg.contains("bang") && svg.contains("<path"),
            "{svg}"
        );
        let cloud = coverage("mindmap\n  (-no-)\n").unwrap().unwrap();
        assert_eq!(cloud.line, Some(1));
        assert_eq!(
            cloud.reason,
            "a cloud is not drawn by Omaspec; mmdr draws it"
        );

        let gantt = include_str!("../../omaspec-core/tests/fixtures/gantt.mmd");
        assert!(coverage(gantt).unwrap().is_none());
        let svg = render(gantt).unwrap();
        assert!(
            svg.contains("Omaspec 1.0")
                && svg.contains("Lint and outline")
                && svg.contains("Highlighting"),
            "{svg}"
        );
        assert!(coverage("gantt\nexcludes weekends\n").unwrap().is_none());
        let svg = render(
            "gantt\n\
             dateFormat YYYY-MM-DD\n\
             excludes weekends\n\
             Work :2026-01-05, 5d\n",
        )
        .unwrap();
        assert!(svg.contains("Work"), "{svg}");
        let excluded = coverage("gantt\nexcludes holidays\n").unwrap().unwrap();
        assert_eq!(excluded.line, Some(1));
        assert_eq!(
            excluded.reason,
            "an excluded day is not drawn by Omaspec; mmdr draws it"
        );

        assert!(
            coverage("classDiagram\nclass Animal\nstyle Animal stroke-width:4px\n")
                .unwrap()
                .is_none()
        );
        let svg = render("classDiagram\nclass Animal\nstyle Animal stroke-width:4px\n").unwrap();
        assert!(
            svg.contains("Animal") && svg.contains("stroke-width=\"4\""),
            "{svg}"
        );
        let style = coverage("classDiagram\nclass Animal\nstyle Animal opacity:1\n")
            .unwrap()
            .unwrap();
        assert_eq!(style.line, Some(2));
        assert_eq!(
            style.reason,
            "a style is not drawn by Omaspec; mmdr draws it"
        );
        let pop = coverage("classDiagram\nbar ()-- foo\n").unwrap().unwrap();
        assert_eq!(pop.line, Some(1));
        assert_eq!(
            pop.reason,
            "a lollipop is not drawn by Omaspec; mmdr draws it"
        );

        let pie = include_str!("../../omaspec-core/tests/fixtures/pie.mmd");
        assert!(coverage(pie).unwrap().is_none());
        let svg = render(pie).unwrap();
        assert!(
            svg.contains("Disk usage")
                && svg.contains("Packages")
                && svg.contains("Home")
                && svg.contains("Cache"),
            "{svg}"
        );
        assert!(coverage("pie\n  \"Minus\": -1\n").unwrap().is_none());
        let svg = render("pie\n  \"Minus\": -1\n").unwrap();
        assert!(svg.contains("Minus"), "{svg}");
        let journey = include_str!("../../omaspec-core/tests/fixtures/journey.mmd");
        assert!(coverage(journey).unwrap().is_none());
        let svg = render(journey).unwrap();
        assert!(
            svg.contains("First boot")
                && svg.contains("Install")
                && svg.contains("Use")
                && svg.contains("Partition disk")
                && svg.contains("Pick theme")
                && svg.contains("Open terminal")
                && svg.contains("User"),
            "{svg}"
        );
        assert!(coverage("journey\n  Nap: 0: Me\n").unwrap().is_none());
        let svg = render("journey\n  Nap: 0: Me\n").unwrap();
        assert!(svg.contains("Nap"), "{svg}");
        let score = coverage("journey\n  Nap: nope: Me\n").unwrap().unwrap();
        assert_eq!(score.line, Some(1));
        assert_eq!(
            score.reason,
            "a score is not drawn by Omaspec; mmdr draws it"
        );
        assert!(
            coverage("timeline\n  title History\n  2020 : Launch\n")
                .unwrap()
                .is_none()
        );
        let svg = render("timeline\n  title History\n  section Early\n  2020 : Launch\n").unwrap();
        assert!(
            svg.contains("History") && svg.contains("Early") && svg.contains("Launch"),
            "{svg}"
        );
        assert!(
            coverage("timeline TD\n  2020 : Launch\n")
                .unwrap()
                .is_none()
        );
        let svg = render("timeline TD\n  2020 : Launch\n").unwrap();
        assert!(svg.contains("Launch"), "{svg}");
        let er = include_str!("../../omaspec-core/tests/fixtures/er.mmd");
        assert!(coverage(er).unwrap().is_none());
        let svg = render(er).unwrap();
        assert!(
            svg.contains("PACKAGE") && svg.contains("FILE") && svg.contains("contains"),
            "{svg}"
        );
        let style = coverage("erDiagram\n  A ||--|| B : x\n  style A opacity:1\n")
            .unwrap()
            .unwrap();
        assert_eq!(style.line, Some(2));
        assert_eq!(
            style.reason,
            "a style is not drawn by Omaspec; mmdr draws it"
        );
        assert!(
            coverage("C4Context\nPerson(admin, \"Admin\")\nSystem(sys, \"System\")\nRel(admin, sys, \"Uses\")\n")
                .unwrap()
                .is_none()
        );
        let sprite = coverage("C4Context\nPerson(admin, \"Admin\", $sprite=users)\n")
            .unwrap()
            .unwrap();
        assert_eq!(sprite.line, Some(1));
        assert_eq!(
            sprite.reason,
            "a sprite is not drawn by Omaspec; mmdr draws it"
        );
        assert!(
            coverage(
                "architecture-beta\n  service web(server)[Web]\n  service db(database)[DB]\n  web:R --> L:db\n"
            )
            .unwrap()
            .is_none()
        );
        let icon = coverage("architecture-beta\n  service db(logos:aws-aurora)[DB]\n")
            .unwrap()
            .unwrap();
        assert_eq!(icon.line, Some(1));
        assert_eq!(
            icon.reason,
            "an icon is not drawn by Omaspec; mmdr draws it"
        );
        assert!(
            coverage("block-beta\n  a[\"Web\"] b[\"DB\"]\n  a --> b\n")
                .unwrap()
                .is_none()
        );
        let svg = render("block-beta\n  a[\"Web\"] b[\"DB\"]\n  a --> b\n").unwrap();
        assert!(
            svg.contains("Web") && svg.contains("#94A3B8") && svg.contains("#0F172A"),
            "{svg}"
        );
        assert!(coverage(
            "block-beta\ncolumns 3\n  db[(\"DB\")] api[\"API\"] web(\"Web\")\n  block:side\n    columns 1\n    reader[\"Reader\"]\n    writer[\"Writer\"]\n  end\n  queue([\"Queue\"])\n  disk[(\"Disk\")]\n  go<[\"flow\"]>(right)\n  db --> api\n  api --> web\n  reader --> writer\n  web --> queue\n"
        )
        .unwrap()
        .is_none());
        let class = coverage("block-beta\nclassDef a fill:#fff\n")
            .unwrap()
            .unwrap();
        assert_eq!(class.line, Some(1));
        assert_eq!(
            class.reason,
            "a class is not drawn by Omaspec; mmdr draws it"
        );
        let git = coverage("gitGraph\n  commit\n").unwrap().unwrap();
        assert!(git.line.is_none());
        assert_eq!(
            git.reason,
            "Git graphs are not drawn by Omaspec; mmdr draws them"
        );

        assert!(
            coverage("sequenceDiagram\nAlice->>Bob: hi\nrect rgb(0, 0, 0)\nend\n")
                .unwrap()
                .is_none()
        );
        let color = coverage("sequenceDiagram\nrect #zzzzzz\nA->>B: hi\nend\n")
            .unwrap()
            .unwrap();
        assert_eq!(color.line, Some(1));
        assert!(color.reason.contains("a color"), "{}", color.reason);

        let fenced = coverage("```mermaid\nsequenceDiagram\nA->>B: hi @{\n```")
            .unwrap()
            .unwrap();
        assert_eq!(fenced.line, Some(2));
        assert!(fenced.reason.contains("@{"), "{}", fenced.reason);

        assert!(coverage("flowchart TD\n  subgraph S\n  A --> B\n").is_err());
    }

    #[test]
    fn refuses_init_values_nested_too_deep() {
        let init = |n: usize| {
            format!(
                "%%{{init: {}1{}}}%%\nflowchart LR\nA --> B\n",
                "{\"a\": ".repeat(n),
                "}".repeat(n)
            )
        };
        // The directive's own braces count: `{init: …}` is one level.
        assert!(render(&init(MAX_NESTING - 1)).is_ok());
        let err = render(&init(100_000)).unwrap_err();
        let RenderError::Diagram { line, message, .. } = err else {
            panic!("{err:?}");
        };
        assert_eq!(line, Some(0));
        assert!(message.contains("init"), "{message}");
        // Braces in a string are text.
        let quoted = format!(
            "%%{{init: {{\"a\": \"{}\"}}}}%%\nflowchart LR\nA --> B\n",
            "{".repeat(500)
        );
        assert!(render(&quoted).is_ok());
    }

    #[test]
    fn refuses_blocks_nested_too_deep() {
        let nested = |n: usize, open: &str, close: &str| {
            let mut lines = vec!["flowchart TD".to_owned()];
            lines.extend((0..n).map(|i| format!("{open}{i}")));
            lines.push("A --> B".into());
            lines.extend((0..n).map(|_| close.to_owned()));
            lines.join("\n")
        };
        assert!(render(&nested(MAX_NESTING, "subgraph G", "end")).is_ok());
        let err = render(&nested(5000, "subgraph G", "end")).unwrap_err();
        let RenderError::Diagram { line, message, .. } = err else {
            panic!("{err:?}");
        };
        assert_eq!(line, Some(MAX_NESTING + 1));
        assert!(message.contains("deeper than 100"), "{message}");
        let states = nested(5000, "state S", "}")
            .replace("flowchart TD", "stateDiagram-v2")
            .replace("A --> B", "[*] --> X")
            .replace("\nstate S", " {\nstate S");
        assert!(render(&states).is_err());
    }

    #[test]
    fn draws_a_state_note_written_over_several_lines() {
        let source = "\
stateDiagram-v2
[*] --> Idle
note right of Idle
    waiting
    here
end note
Idle --> [*]";
        assert_eq!(
            fold_multiline_notes(source),
            "stateDiagram-v2\n[*] --> Idle\nnote right of Idle : waiting<br/>here\n\n\n\nIdle --> [*]"
        );
        let svg = render(source).unwrap();
        assert!(svg.contains("waiting") && svg.contains("here"), "{svg}");
        // One line already, so nothing is rewritten.
        let one = "stateDiagram-v2\nnote left of Idle : stay\n";
        assert_eq!(fold_multiline_notes(one), one);
    }

    #[test]
    fn draws_a_sequence_note_written_over_several_lines() {
        let source = "\
sequenceDiagram
Alice->>Bob: hi
Note over Alice,Bob
    both see this
end note";
        let svg = render(source).unwrap();
        assert!(svg.contains("both see this"), "{svg}");
    }
}
