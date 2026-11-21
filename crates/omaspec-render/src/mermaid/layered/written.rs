//! Which line of a flowchart writes what it draws, and where on it.
//! mmdr's graph keeps neither, so each line is read by mmdr on its own,
//! under a header: what mmdr finds there is what that line writes, read
//! as mmdr reads the whole: its edges, also a chain's and an `&`'s, and
//! the labels it gives nodes and subgraphs. The graph's edges are then
//! found among them in the order written.
//!
//! Where a label stands on its line is read the same way. Where its words
//! stand on the line more than once (`A -->|End| End`), each place is
//! written as a mark of its own and the line read again: the label that
//! reads as a mark stands where that mark does.
//!
//! A line read on its own that mmdr refuses, a label more than one line
//! gives the same node, or a part the whole reads otherwise (a node mmdr
//! misread and the drawer put back) is found on no line: its words are
//! counted, as before.

use std::collections::{BTreeSet, HashMap, HashSet};

use mermaid_rs_renderer::{Graph, parse_mermaid_strict};
use omaspec_core::locate;
use omaspec_uml::Written as At;

use super::reading;
use crate::mermaid::head;

/// Where the source writes a graph's parts.
#[derive(Debug, Default, Clone)]
pub(super) struct Written {
    /// For each edge of the graph, by index, where its label is written.
    pub edges: Vec<Option<At>>,
    /// Where the label a node shows is written, by its id. A node that
    /// shows its id, which every line naming it writes, has none.
    pub nodes: HashMap<String, At>,
    /// For each subgraph, by index, where its title is written, when that
    /// is not its id.
    pub subgraphs: Vec<Option<At>>,
}

/// An edge one line writes.
struct Edge {
    from: String,
    to: String,
    label: Option<String>,
    at: At,
}

/// Where in `source`, the flowchart mmdr read as `graph`, its parts are
/// written. The edges of `skip` (added after mmdr's reading) are written
/// on none.
pub(super) fn read(graph: &Graph, source: &str, skip: &HashSet<usize>) -> Written {
    let mut edges: Vec<Edge> = Vec::new();
    let mut labels: HashMap<String, Vec<(String, At)>> = HashMap::new();
    let mut titles: Vec<(Option<String>, String, At)> = Vec::new();
    let mut header = false;
    // Within `accDescr { … }`: words, not statements.
    let mut described = false;
    for (row, raw, line) in head::significant(source) {
        if line.starts_with("%%") {
            continue;
        }
        if described {
            described = !line.contains('}');
            continue;
        }
        // The header may carry statements of its own (`graph TD; A-->B`).
        let first = !header;
        header = true;
        if !first {
            let word = first_word(line);
            if word == "accTitle" || word == "accDescr" {
                described = word == "accDescr" && line.contains('{') && !line.contains('}');
                continue;
            }
            if word == "subgraph" {
                if let Some((id, title, col)) = title_of(raw) {
                    titles.push((id, title, At { line: row, col }));
                }
                continue;
            }
            if skipped(word, line) {
                continue;
            }
        }
        let alone = |text: &str| {
            let text = reading::readable(text);
            if first {
                text.into_owned()
            } else {
                format!("flowchart TD\n{text}")
            }
        };
        let Some(read) = parsed(&alone(raw)) else {
            continue;
        };
        let places = places(raw, &read, alone);
        edges.extend(read.edges.iter().enumerate().map(|(index, edge)| Edge {
            from: edge.from.clone(),
            to: edge.to.clone(),
            label: edge.label.clone(),
            at: At {
                line: row,
                col: places.edges[index],
            },
        }));
        for (id, node) in &read.nodes {
            if node.label != *id {
                let col = places.nodes.get(id).copied().flatten();
                labels
                    .entry(id.clone())
                    .or_default()
                    .push((node.label.clone(), At { line: row, col }));
            }
        }
    }
    Written {
        edges: matched(graph, &edges, skip),
        nodes: labels
            .into_iter()
            .filter_map(|(id, given)| {
                // The label shown, where one line alone gives it.
                let shown = &graph.nodes.get(&id)?.label;
                let mut by = given.iter().filter(|(label, _)| label == shown);
                match (by.next(), by.next()) {
                    (Some((_, at)), None) => Some((id, *at)),
                    _ => None,
                }
            })
            .collect(),
        subgraphs: titled(graph, &titles),
    }
}

/// The first word of a statement.
fn first_word(line: &str) -> &str {
    line.split(|c: char| c.is_whitespace() || matches!(c, ';' | ':' | '{'))
        .next()
        .unwrap_or("")
}

/// A line that writes no edge and no label: a style, a class, a click.
fn skipped(word: &str, line: &str) -> bool {
    const WORDS: [&str; 7] = [
        "end",
        "classDef",
        "class",
        "style",
        "linkStyle",
        "click",
        "direction",
    ];
    WORDS.contains(&word)
        // A line with neither a link nor a shape names nodes only.
        || !line.contains(['-', '=', '[', '(', '{', '>', '@'])
}

/// The id, the title and where the title starts on `raw`, a line that
/// opens a subgraph, where its title is words of its own.
fn title_of(raw: &str) -> Option<(Option<String>, String, Option<usize>)> {
    let alone = |text: &str| format!("flowchart TD\n{}\nend", reading::readable(text));
    let read = parsed(&alone(raw))?;
    let sub = read.subgraphs.first()?;
    if sub.id.as_deref() == Some(sub.label.as_str()) {
        return None;
    }
    let spans = locate::spans_on(raw, &sub.label);
    let col = match spans.as_slice() {
        [] => None,
        [(start, _)] => Some(*start),
        _ => parsed(&alone(&marked(raw, &spans)))
            .and_then(|again| mark_of(&again.subgraphs.first()?.label))
            .map(|index| spans[index].0),
    };
    Some((sub.id.clone(), sub.label.clone(), col))
}

/// Where the labels of `read`, the line `raw` as mmdr reads it alone
/// (`alone` puts a line under its header), start on it.
struct Places {
    /// For each edge of `read`, by index.
    edges: Vec<Option<usize>>,
    /// For each node of `read` with a label of its own, by id.
    nodes: HashMap<String, Option<usize>>,
}

fn places(raw: &str, read: &Graph, alone: impl Fn(&str) -> String) -> Places {
    let mut places = Places {
        edges: vec![None; read.edges.len()],
        nodes: HashMap::new(),
    };
    let own = |id: &String| read.nodes.get(id).filter(|node| node.label != *id);
    let order = by_order(read);
    let texts: BTreeSet<&str> = read
        .edges
        .iter()
        .filter_map(|edge| edge.label.as_deref())
        .chain(order.iter().filter_map(|id| Some(own(id)?.label.as_str())))
        .collect();
    for text in texts {
        let spans = locate::spans_on(raw, text);
        let edges: Vec<usize> = (0..read.edges.len())
            .filter(|&index| read.edges[index].label.as_deref() == Some(text))
            .collect();
        let nodes: Vec<usize> = (0..order.len())
            .filter(|&at| own(order[at]).is_some_and(|node| node.label == text))
            .collect();
        if spans.is_empty() {
            continue;
        }
        // Written once, by one part: there.
        if spans.len() == 1 && edges.len() + nodes.len() == 1 {
            let start = Some(spans[0].0);
            match (edges.first(), nodes.first()) {
                (Some(&index), _) => places.edges[index] = start,
                (None, Some(&at)) => {
                    places.nodes.insert(order[at].clone(), start);
                }
                (None, None) => {}
            }
            continue;
        }
        // Each place a mark of its own, and the line read again: the same
        // statements, the marks where the words were.
        let Some(again) = parsed(&alone(&marked(raw, &spans))) else {
            continue;
        };
        let again_order = by_order(&again);
        if again.edges.len() != read.edges.len() || again_order.len() != order.len() {
            continue;
        }
        let start = |label: &str| mark_of(label).map(|index| spans[index].0);
        for index in edges {
            places.edges[index] = again.edges[index].label.as_deref().and_then(start);
        }
        for at in nodes {
            let col = again
                .nodes
                .get(again_order[at])
                .and_then(|node| start(&node.label));
            places.nodes.insert(order[at].clone(), col);
        }
    }
    places
}

/// The ids of `graph`'s nodes in the order first written.
fn by_order(graph: &Graph) -> Vec<&String> {
    let mut ids: Vec<&String> = graph.nodes.keys().collect();
    ids.sort_by_key(|id| graph.node_order.get(*id).copied().unwrap_or(usize::MAX));
    ids
}

/// The mark the `index`th place of some words is written as: one word
/// that no flowchart writes.
fn mark(index: usize) -> String {
    format!("omaspecmark{index}x")
}

/// Which place `label` marks, if it is a mark.
fn mark_of(label: &str) -> Option<usize> {
    label
        .trim()
        .strip_prefix("omaspecmark")?
        .strip_suffix('x')?
        .parse()
        .ok()
}

/// `raw` with each of `spans`, in order and apart, written as its mark.
fn marked(raw: &str, spans: &[(usize, usize)]) -> String {
    let mut out = String::with_capacity(raw.len() + spans.len() * 16);
    let mut from = 0;
    for (index, &(start, end)) in spans.iter().enumerate() {
        if start < from || end > raw.len() {
            continue;
        }
        out.push_str(&raw[from..start]);
        out.push_str(&mark(index));
        from = end;
    }
    out.push_str(&raw[from..]);
    out
}

/// `text` as mmdr reads it; `None` where it refuses it or fails on it.
fn parsed(text: &str) -> Option<Graph> {
    crate::caught::caught(|| parse_mermaid_strict(text))
        .ok()?
        .ok()
        .map(|output| output.graph)
}

/// For each edge of `graph`, where the first edge in `found` not taken
/// yet that joins the same two and reads the same is written.
fn matched(graph: &Graph, found: &[Edge], skip: &HashSet<usize>) -> Vec<Option<At>> {
    let mut taken = vec![false; found.len()];
    graph
        .edges
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            if skip.contains(&index) {
                return None;
            }
            let at = found.iter().enumerate().position(|(at, written)| {
                !taken[at]
                    && written.from == edge.from
                    && written.to == edge.to
                    && written.label == edge.label
            })?;
            taken[at] = true;
            Some(found[at].at)
        })
        .collect()
}

/// For each subgraph of `graph`, where the first title in `found` not
/// taken yet with its id and its words is written.
fn titled(graph: &Graph, found: &[(Option<String>, String, At)]) -> Vec<Option<At>> {
    let mut taken = vec![false; found.len()];
    graph
        .subgraphs
        .iter()
        .map(|sub| {
            let at = found.iter().enumerate().position(|(at, (id, label, _))| {
                !taken[at] && *id == sub.id && *label == sub.label
            })?;
            taken[at] = true;
            Some(found[at].2)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(text: &str) -> Written {
        let graph = parse_mermaid_strict(text).unwrap().graph;
        read(&graph, text, &HashSet::new())
    }

    fn at(line: usize, col: usize) -> At {
        At {
            line,
            col: Some(col),
        }
    }

    /// Each edge is found on the line that writes it, also those a chain
    /// and an `&` write, and those that read alike.
    #[test]
    fn each_edge_is_found_on_its_line() {
        let text = "flowchart TD\n  A[Start] -->|yes| B\n  B --> C --> D\n  A & B -- no --> E\n  C -->|yes| B\n";
        let graph = parse_mermaid_strict(text).unwrap().graph;
        let found = read(&graph, text, &HashSet::new());
        let mut lines: Vec<(String, String, Option<usize>)> = graph
            .edges
            .iter()
            .zip(&found.edges)
            .map(|(edge, at)| (edge.from.clone(), edge.to.clone(), at.map(|at| at.line)))
            .collect();
        lines.sort();
        let edge = |from: &str, to: &str, line: usize| (from.to_owned(), to.to_owned(), Some(line));
        assert_eq!(
            lines,
            [
                edge("A", "B", 1),
                edge("A", "E", 3),
                edge("B", "C", 2),
                edge("B", "E", 3),
                edge("C", "B", 4),
                edge("C", "D", 2),
            ]
        );
    }

    /// A chain with a labelled link is found link by link on its line, the
    /// label where its words stand.
    #[test]
    fn a_labelled_chain_is_found_on_its_line() {
        let text = "flowchart TD\n  a -->|yes| b --> c -- no --> d\n";
        let graph = parse_mermaid_strict(&reading::readable(text))
            .unwrap()
            .graph;
        let found = read(&graph, text, &HashSet::new());
        let edges: Vec<(&str, &str, Option<At>)> = graph
            .edges
            .iter()
            .zip(&found.edges)
            .map(|(edge, at)| (edge.from.as_str(), edge.to.as_str(), *at))
            .collect();
        assert_eq!(
            edges,
            [
                ("a", "b", Some(at(1, 8))),
                ("b", "c", Some(At { line: 1, col: None })),
                ("c", "d", Some(at(1, 24))),
            ]
        );
    }

    /// A node's label is its declaration's line; a node that shows its id
    /// has none, nor has a subgraph titled by its id.
    #[test]
    fn a_label_is_found_where_it_is_given() {
        let text = "graph LR; X --> Y\n  A[Start] --> B\n  subgraph one [Outer]\n    C\n  end\n  subgraph two\n    D(Done)\n  end\n  subgraph Last words\n    E\n  end\n  A --> D\n";
        let found = written(text);
        assert_eq!(found.nodes.get("A"), Some(&at(1, 4)));
        assert_eq!(found.nodes.get("D"), Some(&at(6, 6)));
        assert_eq!(found.nodes.get("B"), None);
        assert_eq!(found.edges.iter().flatten().count(), 3);
        assert!(
            found.edges.iter().flatten().any(|at| at.line == 0),
            "the header's own edge"
        );
        let mut titles: Vec<Option<At>> = found.subgraphs.clone();
        titles.sort_by_key(|at| at.map(|at| at.line));
        assert_eq!(titles, [None, Some(at(2, 16)), Some(at(8, 11))]);
    }

    /// Where a label's words stand more than once on its line, the label
    /// is the place mmdr reads as the label: between the line's ends, in
    /// the brackets, not the name.
    #[test]
    fn a_label_stands_where_mmdr_reads_it() {
        let text = "flowchart LR\n  A -->|End| End\n  End[yes] -->|yes| B[yes]\n  subgraph S [S]\n    S1\n  end\n";
        let graph = parse_mermaid_strict(text).unwrap().graph;
        let found = read(&graph, text, &HashSet::new());
        let edge = |from: &str, to: &str| {
            let index = graph
                .edges
                .iter()
                .position(|edge| edge.from == from && edge.to == to)
                .unwrap();
            found.edges[index]
        };
        assert_eq!(edge("A", "End"), Some(at(1, 8)));
        assert_eq!(edge("End", "B"), Some(at(2, 15)));
        // Line 1 gives `End` no label; line 2 does.
        assert_eq!(found.nodes.get("End"), Some(&at(2, 6)));
        assert_eq!(found.nodes.get("B"), Some(&at(2, 22)));
    }

    /// A label two lines give the same node is written by neither alone;
    /// one given again otherwise is the last line's.
    #[test]
    fn a_label_given_twice_is_counted() {
        let text =
            "flowchart LR\n  A[Start] --> B\n  A[Start] --> C\n  D[Go] --> E\n  D[Run] --> F\n";
        let found = written(text);
        assert_eq!(found.nodes.get("A"), None);
        assert_eq!(found.nodes.get("D"), Some(&at(4, 4)));
    }

    /// The lines about the diagram are not read as statements: neither an
    /// accessible title or description, nor the lines of one in braces,
    /// while a node whose name begins alike is.
    #[test]
    fn what_describes_the_diagram_is_not_a_statement() {
        let text = "flowchart LR\n  accTitle: A to B\n  accDescr {\n    A -->|ok| B\n  }\n  access -->|ok| B\n  A -->|ok| B\n";
        let graph = parse_mermaid_strict(text).unwrap().graph;
        let found = read(&graph, text, &HashSet::new());
        let mut lines: Vec<usize> = found.edges.iter().flatten().map(|at| at.line).collect();
        lines.sort_unstable();
        assert_eq!(lines, [5, 6]);
    }
}
