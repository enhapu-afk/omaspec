//! Which line of a flowchart writes what it draws. mmdr's graph keeps no
//! lines, so each line is read by mmdr on its own, under a header: what
//! mmdr finds there is what that line writes, read as mmdr reads the
//! whole: its edges, also a chain's and an `&`'s, and the labels it gives
//! nodes and subgraphs. The graph's edges are then found among them in
//! the order written.
//!
//! A line read on its own that mmdr refuses, or a part the whole reads
//! otherwise (a subgraph folded to its box, a node mmdr misread and the
//! drawer put back), is found on no line: its words are counted, as
//! before.

use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use mermaid_rs_renderer::{Graph, parse_mermaid_strict};

/// The zero-based lines of the source that write a graph's parts.
#[derive(Debug, Default)]
pub(super) struct Written {
    /// For each edge of the graph, by index.
    pub edges: Vec<Option<usize>>,
    /// The line that writes the label a node shows, by its id. A node
    /// that shows its id, which every line naming it writes, has none.
    pub nodes: HashMap<String, usize>,
    /// For each subgraph, by index, the line that writes its title, when
    /// that is not its id.
    pub subgraphs: Vec<Option<usize>>,
}

/// An edge one line writes.
struct Edge {
    from: String,
    to: String,
    label: Option<String>,
    line: usize,
}

/// The lines of `text`, the flowchart mmdr read as `graph`, that write
/// its parts. The edges of `skip` (added after mmdr's reading) are
/// written on none.
pub(super) fn read(graph: &Graph, text: &str, skip: &HashSet<usize>) -> Written {
    let mut edges: Vec<Edge> = Vec::new();
    let mut labels: HashMap<String, (String, usize)> = HashMap::new();
    let mut titles: Vec<(Option<String>, String, usize)> = Vec::new();
    let mut front = false;
    let mut header = false;
    for (row, line) in text.split('\n').enumerate() {
        let trimmed = line.trim();
        if trimmed == "---" {
            front = !front;
            continue;
        }
        if front || trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        // The header may carry statements of its own (`graph TD; A-->B`).
        let alone = if header {
            if trimmed.starts_with("subgraph") {
                if let Some(sub) =
                    parsed(&format!("flowchart TD\n{trimmed}\nend")).and_then(|g| first_sub(&g))
                {
                    titles.push((sub.0, sub.1, row));
                }
                continue;
            }
            if skipped(trimmed) {
                continue;
            }
            format!("flowchart TD\n{trimmed}")
        } else {
            header = true;
            trimmed.to_owned()
        };
        let Some(read) = parsed(&alone) else {
            continue;
        };
        edges.extend(read.edges.iter().map(|edge| Edge {
            from: edge.from.clone(),
            to: edge.to.clone(),
            label: edge.label.clone(),
            line: row,
        }));
        for (id, node) in &read.nodes {
            if node.label != *id {
                labels.insert(id.clone(), (node.label.clone(), row));
            }
        }
    }
    Written {
        edges: matched(graph, &edges, skip),
        nodes: labels
            .into_iter()
            .filter(|(id, (label, _))| graph.nodes.get(id).is_some_and(|n| n.label == *label))
            .map(|(id, (_, row))| (id, row))
            .collect(),
        subgraphs: titled(graph, &titles),
    }
}

/// A line that writes no edge and no label: a style, a class, a click.
fn skipped(line: &str) -> bool {
    const WORDS: [&str; 8] = [
        "end",
        "classDef",
        "class",
        "style",
        "linkStyle",
        "click",
        "direction",
        "acc",
    ];
    let word = line
        .split(|c: char| c.is_whitespace() || c == ';')
        .next()
        .unwrap_or("");
    WORDS
        .iter()
        .any(|w| word == *w || (*w == "acc" && word.starts_with("acc")))
        // A line with neither a link nor a shape names nodes only.
        || !line.contains(['-', '=', '[', '(', '{', '>', '@'])
}

/// `text` as mmdr reads it; `None` where it refuses it or fails on it.
fn parsed(text: &str) -> Option<Graph> {
    catch_unwind(AssertUnwindSafe(|| parse_mermaid_strict(text)))
        .ok()?
        .ok()
        .map(|output| output.graph)
}

/// The id and title of the first subgraph of `graph`, where its title is
/// words of its own.
fn first_sub(graph: &Graph) -> Option<(Option<String>, String)> {
    let sub = graph.subgraphs.first()?;
    (sub.id.as_deref() != Some(sub.label.as_str())).then(|| (sub.id.clone(), sub.label.clone()))
}

/// For each edge of `graph`, the line of the first edge in `found` not
/// taken yet that joins the same two and reads the same.
fn matched(graph: &Graph, found: &[Edge], skip: &HashSet<usize>) -> Vec<Option<usize>> {
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
            Some(found[at].line)
        })
        .collect()
}

/// For each subgraph of `graph`, the line of the first title in `found`
/// not taken yet with its id and its words.
fn titled(graph: &Graph, found: &[(Option<String>, String, usize)]) -> Vec<Option<usize>> {
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
            .map(|(edge, line)| (edge.from.clone(), edge.to.clone(), *line))
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

    /// A node's label is its declaration's line, the last one that gives
    /// it; a node that shows its id has none, nor has a subgraph titled by
    /// its id.
    #[test]
    fn a_label_is_found_where_it_is_given() {
        let text = "graph LR; X --> Y\n  A[Start] --> B\n  subgraph one [Outer]\n    C\n  end\n  subgraph two\n    D(Done)\n  end\n  subgraph Last words\n    E\n  end\n  A --> D\n";
        let found = written(text);
        assert_eq!(found.nodes.get("A"), Some(&1));
        assert_eq!(found.nodes.get("D"), Some(&6));
        assert_eq!(found.nodes.get("B"), None);
        assert_eq!(found.edges.iter().flatten().count(), 3);
        assert!(found.edges.contains(&Some(0)), "the header's own edge");
        let mut titles = found.subgraphs.clone();
        titles.sort();
        assert_eq!(titles, [None, Some(2), Some(8)]);
    }
}
