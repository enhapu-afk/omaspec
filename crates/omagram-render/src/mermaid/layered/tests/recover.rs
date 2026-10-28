use mermaid_rs_renderer::{Graph, NodeShape, parse_mermaid_strict};

use super::super::flowchart::{Fix, apply, double_circles, empty_nesting};

fn fixed(source: &str) -> (Graph, Fix) {
    let parsed = parse_mermaid_strict(source).unwrap();
    let mut slot = None;
    let (graph, fix) = apply(&parsed.graph, source, &mut slot).unwrap();
    (graph.clone(), fix)
}

#[test]
fn finds_double_circles_by_their_id() {
    let ids = double_circles("flowchart LR\nA((one)) --> B(((two)))\nC---my-node(((x)))\n");
    assert!(ids.contains("B") && ids.contains("my-node"), "{ids:?}");
    assert!(!ids.contains("A"));
}

#[test]
fn comments_and_labels_are_not_double_circles() {
    let comment = double_circles("flowchart LR\nA((circle))\n%% A(((no)))\nB(((yes)))\n");
    assert!(
        comment.contains("B") && !comment.contains("A"),
        "{comment:?}"
    );
    assert!(double_circles("flowchart LR\nB[see A(((x)))]\n").is_empty());
    assert!(double_circles("flowchart LR\nA -->|B(((z)))| C\n").is_empty());
    assert!(double_circles("flowchart LR\nA[\"B(((z)))\"]\n").is_empty());
}

#[test]
fn an_invisible_link_keeps_the_node_mmdr_drops() {
    let (graph, fix) = fixed("flowchart LR\nA[text] ~~~ B ~~~ C\n");
    assert!(graph.nodes.contains_key("A"));
    assert_eq!(graph.nodes["A"].label, "text");
    assert!(graph.nodes.contains_key("B") && graph.nodes.contains_key("C"));
    assert!(!graph.nodes.contains_key("A[text]"));
    assert_eq!(fix.invisible.len(), 2, "{:?}", graph.edges);
}

#[test]
fn a_new_shape_renames_the_node_and_uses_its_label() {
    let (graph, fix) = fixed("flowchart LR\nA@{ shape: cyl, label: \"db\" }\n");
    let node = &graph.nodes["A"];
    assert_eq!(node.shape, NodeShape::Cylinder);
    assert_eq!(node.label, "db");
    assert!(!graph.nodes.contains_key("A@"));
    assert!(fix.doubles.is_empty());
    let (graph, fix) = fixed("flowchart LR\nA@{ shape: dbl-circ }\n");
    assert!(fix.doubles.contains("A"));
    assert_eq!(graph.nodes["A"].shape, NodeShape::DoubleCircle);
    assert_eq!(graph.nodes["A"].label, "A");
}

#[test]
fn a_shape_on_an_edge_keeps_the_class_and_the_shorthand_label() {
    let source = "flowchart LR\nclassDef hot fill:#ff0000\nA[box]@{ shape: diam }:::hot --> B\n";
    let (graph, _) = fixed(source);
    let node = &graph.nodes["A"];
    assert_eq!(node.shape, NodeShape::Diamond);
    assert_eq!(node.label, "box");
    assert!(graph.node_classes["A"].iter().any(|class| class == "hot"));
    assert_eq!(graph.edges[0].from, "A");
    assert!(!graph.nodes.contains_key("A[box]@"));
    let (graph, fix) = fixed("flowchart LR\nA --> B ~~~ C\n");
    assert!(graph.nodes.contains_key("C"));
    let index = fix.invisible.iter().next().copied().unwrap();
    assert_eq!(graph.edges[index].from, "B");
    assert_eq!(graph.edges[index].to, "C");
}

#[test]
fn an_unknown_shape_or_an_image_declines() {
    for source in [
        "flowchart LR\nA@{ shape: not-a-shape }\n",
        "flowchart LR\nA@{ img: \"https://example.com/a.png\" }\n",
        "flowchart LR\nA@{ icon: \"fa:user\", form: \"star\" }\n",
    ] {
        let parsed = parse_mermaid_strict(source).unwrap();
        let mut slot = None;
        assert!(
            apply(&parsed.graph, source, &mut slot).is_none(),
            "{source}"
        );
    }
}

#[test]
fn a_comment_is_not_repaired() {
    let source = "flowchart LR\nA --> B %% ~~~ @{ shape: rect }\n";
    let parsed = parse_mermaid_strict(source).unwrap();
    let mut slot = None;
    let (_graph, fix) = apply(&parsed.graph, source, &mut slot).unwrap();
    assert!(slot.is_none());
    assert!(fix.invisible.is_empty());
}

#[test]
fn an_icon_is_its_short_name() {
    let (graph, fix) =
        fixed("flowchart LR\nA@{ icon: \"fa:user\", label: \"User\", pos: \"t\" }\n");
    assert!(fix.bare.contains("A"));
    assert_eq!(graph.nodes["A"].label, "User\nuser");
    let (graph, fix) =
        fixed("flowchart LR\nA@{ icon: \"fa:user\", form: \"circle\", label: \"User\" }\n");
    assert!(fix.bare.is_empty());
    assert_eq!(graph.nodes["A"].shape, NodeShape::Circle);
    assert_eq!(graph.nodes["A"].label, "user\nUser");
}

#[test]
fn an_empty_subgraph_keeps_the_parent_from_the_source() {
    let parents = empty_nesting("flowchart TB\nsubgraph Outer\nsubgraph Inner\nend\nend\n");
    assert_eq!(parents, vec![None, Some(0)]);
}
