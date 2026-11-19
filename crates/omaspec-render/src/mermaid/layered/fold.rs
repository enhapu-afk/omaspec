//! Flowchart subgraphs folded to their box (docs/adr/0024): the subgraph
//! becomes a node of its id and title, what it held is gone, and a line
//! into it or out of it ends on that node. Read from the fold lines the
//! pane adds after the diagram ([`omaspec_uml::fold`]).

use std::collections::{HashMap, HashSet};

use mermaid_rs_renderer::{Graph, Node, NodeShape};
use omaspec_uml::hand;

use super::flowchart::Fix;
use super::written::Written;

/// A graph with subgraphs folded, and what goes with it.
pub(super) struct Folded {
    pub graph: Graph,
    pub fix: Fix,
    /// Where its parts are written: a folded box's title where the
    /// subgraph's is, a line moved onto it where it was written, and one
    /// that stands for several lines alike on none.
    pub written: Written,
    /// The ids of the nodes the folded subgraphs held.
    pub hidden: HashSet<String>,
}

/// `graph`, `fix` and `written` with the subgraphs whose ids `names`
/// holds folded; `None` when none of them is a subgraph to fold.
pub(super) fn fold(
    graph: &Graph,
    fix: &Fix,
    written: &Written,
    names: &[String],
) -> Option<Folded> {
    // The outer one first, so one folded inside it folds nothing more.
    let mut chosen: Vec<usize> = graph
        .subgraphs
        .iter()
        .enumerate()
        .filter(|(_, sub)| {
            sub.id
                .as_ref()
                .is_some_and(|id| names.contains(id) && !graph.nodes.contains_key(id))
        })
        .map(|(index, _)| index)
        .collect();
    chosen.sort_by_key(|&index| std::cmp::Reverse(graph.subgraphs[index].nodes.len()));
    let mut hidden: HashSet<String> = HashSet::new();
    // Where each hidden node's lines go now.
    let mut redirect: HashMap<String, String> = HashMap::new();
    let mut gone: HashSet<usize> = HashSet::new();
    let mut folded: Vec<usize> = Vec::new();
    for index in chosen {
        let sub = &graph.subgraphs[index];
        let id = sub.id.clone()?;
        if hidden.contains(&id) || gone.contains(&index) {
            continue;
        }
        folded.push(index);
        for node in &sub.nodes {
            if hidden.insert(node.clone()) {
                redirect.insert(node.clone(), id.clone());
            }
        }
        // Subgraphs inside it go with it.
        for (other, inner) in graph.subgraphs.iter().enumerate() {
            if other != index
                && !inner.nodes.is_empty()
                && inner.nodes.iter().all(|n| sub.nodes.contains(n))
            {
                gone.insert(other);
                if let Some(inner_id) = &inner.id {
                    hidden.insert(inner_id.clone());
                }
            }
        }
    }
    if folded.is_empty() {
        return None;
    }
    let mut out = graph.clone();
    for id in &hidden {
        out.nodes.remove(id);
        out.node_order.remove(id);
    }
    for &index in &folded {
        let sub = &graph.subgraphs[index];
        let id = sub.id.clone()?;
        let order = sub
            .nodes
            .iter()
            .filter_map(|n| graph.node_order.get(n))
            .min()
            .copied()
            .unwrap_or(graph.node_order.len());
        out.node_order.insert(id.clone(), order);
        out.nodes.insert(
            id.clone(),
            Node {
                id,
                label: sub.label.clone(),
                shape: NodeShape::Rectangle,
                value: None,
                icon: None,
            },
        );
    }
    let to = |node: &String| redirect.get(node).unwrap_or(node).clone();
    let kept_subgraphs: Vec<usize> = (0..graph.subgraphs.len())
        .filter(|index| !folded.contains(index) && !gone.contains(index))
        .collect();
    out.subgraphs = kept_subgraphs
        .iter()
        .map(|&index| &graph.subgraphs[index])
        .map(|sub| {
            let mut sub = sub.clone();
            let mut seen = HashSet::new();
            sub.nodes = sub
                .nodes
                .iter()
                .map(to)
                .filter(|n| seen.insert(n.clone()))
                .collect();
            sub
        })
        .collect();
    let mut kept = HashMap::new();
    let mut moved: HashMap<String, usize> = HashMap::new();
    let mut edges_written = Vec::with_capacity(graph.edges.len());
    out.edges = Vec::with_capacity(graph.edges.len());
    for (index, edge) in graph.edges.iter().enumerate() {
        let (from, end) = (to(&edge.from), to(&edge.to));
        let shifted = redirect.contains_key(&edge.from) || redirect.contains_key(&edge.to);
        // A line within a folded subgraph is inside its box.
        if from == end && shifted {
            continue;
        }
        let mut edge = edge.clone();
        edge.from = from;
        edge.to = end;
        // Of the lines moved onto a folded box, one of those alike, whose
        // words then no one line writes.
        if shifted {
            let key = format!("{edge:?}");
            if let Some(&first) = moved.get(&key) {
                edges_written[first] = None;
                continue;
            }
            moved.insert(key, out.edges.len());
        }
        kept.insert(index, out.edges.len());
        edges_written.push(written.edges.get(index).copied().flatten());
        out.edges.push(edge);
    }
    let mut nodes = written.nodes.clone();
    nodes.retain(|id, _| !hidden.contains(id));
    for &index in &folded {
        if let (Some(id), Some(Some(at))) =
            (&graph.subgraphs[index].id, written.subgraphs.get(index))
        {
            nodes.insert(id.clone(), *at);
        }
    }
    let written = Written {
        edges: edges_written,
        nodes,
        subgraphs: kept_subgraphs
            .iter()
            .map(|&index| written.subgraphs.get(index).copied().flatten())
            .collect(),
    };
    let fix = Fix {
        invisible: fix
            .invisible
            .iter()
            .filter_map(|i| kept.get(i).copied())
            .collect(),
        doubles: fix
            .doubles
            .iter()
            .filter(|n| !hidden.contains(*n))
            .cloned()
            .collect(),
        bare: fix
            .bare
            .iter()
            .filter(|n| !hidden.contains(*n))
            .cloned()
            .collect(),
        forms: fix
            .forms
            .iter()
            .filter(|(n, _)| !hidden.contains(*n))
            .map(|(n, form)| (n.clone(), *form))
            .collect(),
    };
    Some(Folded {
        graph: out,
        fix,
        written,
        hidden,
    })
}

/// The entries of lines by hand that name no hidden node.
pub(super) fn lines(
    hands: &[(usize, hand::Line)],
    hidden: &HashSet<String>,
) -> Vec<(usize, hand::Line)> {
    hands
        .iter()
        .filter(|(_, line)| !hidden.contains(&line.key.from) && !hidden.contains(&line.key.to))
        .cloned()
        .collect()
}

/// The holds that name no hidden node.
pub(super) fn holds(
    held: &[(usize, hand::Hold)],
    hidden: &HashSet<String>,
) -> Vec<(usize, hand::Hold)> {
    held.iter()
        .filter(|(_, hold)| !hidden.contains(&hold.name) && !hidden.contains(&hold.beside))
        .cloned()
        .collect()
}
