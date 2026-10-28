//! Mermaid state diagrams as Omagram reads them
//! ([`omagram_uml::state::mermaid`]), put in the form of mmdr's graph
//! that the drawer lays out, with the line of every state and transition.
//! The picture then shows the machine the checks and the play go by.
//!
//! The graph follows mmdr's habits, so that the drawer sees what it knows:
//! a nested state is a subgraph listing every state inside it, a
//! concurrent region a frameless subgraph (`__region_N__`), and a start or
//! an end inside a state with several transitions a bar. Each start and
//! end has its own id, so there is nothing to split per region.

use std::collections::HashMap;

use mermaid_rs_renderer::ir::{NodeStyle, StateNote, StateNotePosition, Subgraph};
use mermaid_rs_renderer::{DiagramKind, Direction, Edge, EdgeStyle, Graph, NodeShape};
use omagram_uml::state::mermaid::{Chart, Direction as Way, Kind, Side};

use super::layered::Lines;

/// The graph the drawer lays out for `chart`, and where its states and
/// transitions are written.
pub(super) fn graph(chart: &Chart) -> (Graph, Lines) {
    let mut graph = Graph::new();
    graph.kind = DiagramKind::State;
    graph.direction = match chart.direction {
        None | Some(Way::Down) => Direction::TopDown,
        Some(Way::Up) => Direction::BottomTop,
        Some(Way::Right) => Direction::LeftRight,
        Some(Way::Left) => Direction::RightLeft,
    };
    let ids: Vec<String> = chart
        .states
        .iter()
        .enumerate()
        .map(|(i, s)| match s.kind {
            Kind::Start => format!("__start_{i}__"),
            Kind::End => format!("__end_{i}__"),
            _ => s.id.clone(),
        })
        .collect();
    let mut ways = vec![0usize; chart.states.len()];
    for t in &chart.transitions {
        if chart.states[t.from].kind == Kind::Start {
            ways[t.from] += 1;
        }
        if chart.states[t.to].kind == Kind::End {
            ways[t.to] += 1;
        }
    }
    let mut lines = Lines::default();
    for (i, s) in chart.states.iter().enumerate() {
        let (shape, label) = match s.kind {
            // mmdr's rule: inside a state, a start with several transitions
            // out, or an end with several in, is a bar.
            Kind::Start | Kind::End if s.parent.is_some() && ways[i] > 1 => {
                (NodeShape::ForkJoin, String::new())
            }
            Kind::Start => (NodeShape::Circle, String::new()),
            Kind::End => (NodeShape::DoubleCircle, String::new()),
            Kind::Fork | Kind::Join => (NodeShape::ForkJoin, String::new()),
            Kind::Choice => (NodeShape::Diamond, title(s)),
            Kind::State => (NodeShape::RoundRect, title(s)),
        };
        graph.ensure_node(&ids[i], Some(label), Some(shape));
        lines.states.insert(ids[i].clone(), s.line);
    }
    for t in &chart.transitions {
        graph.edges.push(Edge {
            from: ids[t.from].clone(),
            to: ids[t.to].clone(),
            label: (!t.label.is_empty()).then(|| t.label.clone()),
            start_label: None,
            end_label: None,
            directed: true,
            arrow_start: false,
            arrow_end: true,
            arrow_start_kind: None,
            arrow_end_kind: None,
            start_decoration: None,
            end_decoration: None,
            style: EdgeStyle::Solid,
        });
        lines.transitions.push(t.line);
    }
    nest(chart, &ids, &mut graph);
    for note in &chart.notes {
        graph.state_notes.push(StateNote {
            position: match note.side {
                Side::Left => StateNotePosition::LeftOf,
                Side::Right => StateNotePosition::RightOf,
            },
            target: ids[note.target].clone(),
            label: note.text.clone(),
        });
    }
    for (name, style) in &chart.class_defs {
        graph.class_defs.insert(name.clone(), node_style(style));
    }
    for (id, name) in &chart.classes {
        graph
            .node_classes
            .entry(id.clone())
            .or_default()
            .push(name.clone());
    }
    for (id, style) in &chart.styles {
        graph.node_styles.insert(id.clone(), node_style(style));
    }
    (graph, lines)
}

/// A state's label as mmdr writes it: its name, then each description
/// under a rule.
fn title(s: &omagram_uml::state::mermaid::State) -> String {
    let mut label = s.label.clone().unwrap_or_else(|| s.id.clone());
    if !s.descriptions.is_empty() {
        label.push_str("\n---");
        for description in &s.descriptions {
            label.push('\n');
            label.push_str(description);
        }
    }
    label
}

/// The subgraphs of the nested states and their regions, in the order mmdr
/// makes them: a state when it opens, its regions when it closes.
fn nest(chart: &Chart, ids: &[String], graph: &mut Graph) {
    let n = chart.states.len();
    // Every state inside each composite, and inside each of its regions.
    let mut inside: Vec<Vec<String>> = vec![Vec::new(); n];
    let mut regions: HashMap<(usize, usize), Vec<String>> = HashMap::new();
    for (i, s) in chart.states.iter().enumerate() {
        if s.composite {
            continue;
        }
        let (mut at, mut region) = (s.parent, s.region);
        while let Some(up) = at {
            inside[up].push(ids[i].clone());
            regions
                .entry((up, region))
                .or_default()
                .push(ids[i].clone());
            (at, region) = (chart.states[up].parent, chart.states[up].region);
        }
    }
    let mut made = 0;
    let tops: Vec<usize> = (0..n)
        .filter(|&i| chart.states[i].composite && chart.states[i].parent.is_none())
        .collect();
    for top in tops {
        open(chart, ids, graph, top, &mut inside, &mut regions, &mut made);
    }
}

fn open(
    chart: &Chart,
    ids: &[String],
    graph: &mut Graph,
    state: usize,
    inside: &mut [Vec<String>],
    regions: &mut HashMap<(usize, usize), Vec<String>>,
    made: &mut usize,
) {
    let s = &chart.states[state];
    graph.subgraphs.push(Subgraph {
        id: Some(ids[state].clone()),
        label: s.label.clone().unwrap_or_else(|| s.id.clone()),
        nodes: std::mem::take(&mut inside[state]),
        direction: None,
        icon: None,
    });
    for child in 0..chart.states.len() {
        if chart.states[child].composite && chart.states[child].parent == Some(state) {
            open(chart, ids, graph, child, inside, regions, made);
        }
    }
    let mut own: Vec<(usize, Vec<String>)> = regions
        .iter()
        .filter(|((up, _), _)| *up == state)
        .map(|((_, region), nodes)| (*region, nodes.clone()))
        .collect();
    if own.len() < 2 {
        return;
    }
    own.sort_by_key(|(region, _)| *region);
    for (_, nodes) in own {
        let id = format!("__region_{made}__");
        *made += 1;
        graph.subgraphs.push(Subgraph {
            id: Some(id.clone()),
            label: String::new(),
            nodes,
            direction: None,
            icon: None,
        });
        graph.subgraph_styles.insert(
            id,
            NodeStyle {
                fill: Some("none".into()),
                stroke: Some("none".into()),
                text_color: None,
                stroke_width: Some(0.0),
                stroke_dasharray: None,
                line_color: None,
            },
        );
    }
}

/// `fill:#f00,stroke:#333,stroke-width:2px,color:#fff,stroke-dasharray:5 5`,
/// read as mmdr reads it.
fn node_style(text: &str) -> NodeStyle {
    let mut style = NodeStyle::default();
    for part in text.split(',') {
        let Some((key, value)) = part.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        if value.is_empty() {
            continue;
        }
        match key {
            "fill" => style.fill = Some(value.to_owned()),
            "stroke" => style.stroke = Some(value.to_owned()),
            "stroke-width" => style.stroke_width = value.trim_end_matches("px").parse().ok(),
            "stroke-dasharray" => style.stroke_dasharray = Some(value.to_owned()),
            "color" => style.text_color = Some(value.to_owned()),
            _ => {}
        }
    }
    style
}
