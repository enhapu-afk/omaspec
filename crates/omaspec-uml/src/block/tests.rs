use super::*;

use super::lay::layout;
use super::paint::draw;
use super::route::segment_hits;
use crate::color::Color;
use crate::text::{Family, Font};

fn source(text: &str) -> super::Diagram {
    from_mermaid(text).unwrap_or_else(|| panic!("refused {text:?}: {:?}", mermaid_refusal(text)))
}

fn placed(text: &str) -> super::lay::Laid {
    layout(&source(text), Family::Sans).unwrap()
}

fn holds(outer: super::lay::Rect, inner: super::lay::Rect) -> bool {
    inner.x >= outer.x - 0.2
        && inner.y >= outer.y - 0.2
        && inner.right() <= outer.right() + 0.2
        && inner.bottom() <= outer.bottom() + 0.2
}

#[test]
fn puts_a_row_on_one_line_and_a_column_in_a_stack() {
    let row = placed("block-beta\na[\"A\"] b[\"B\"] c[\"C\"]\n");
    assert!(row.nodes[1].x > row.nodes[0].right());
    assert!((row.nodes[0].y - row.nodes[2].y).abs() < 0.8);
    let stack = placed("block-beta\ncolumns 1\na\nb\n");
    assert!((stack.nodes[0].x - stack.nodes[1].x).abs() < 0.8);
    assert!(stack.nodes[1].y > stack.nodes[0].bottom());
}

#[test]
fn keeps_children_inside_the_group_and_the_sibling_outside() {
    let laid = placed("block-beta\nblock:g[\"Group\"]\n  a[\"Child\"]\nend\nc[\"Side\"]\n");
    let group = laid.groups[0];
    let child = laid.nodes[0];
    let side = laid.nodes[1];
    assert!(holds(group, child), "child {child:?} group {group:?}");
    assert!(child.y >= group.y + laid.bands[0] - 0.2);
    assert!(!side.intersects(group), "side {side:?} group {group:?}");
    let svg = draw(&source(
        "block-beta\nblock:g[\"Group\"]\n  a[\"Child\"]\nend\nc[\"Side\"]\n",
    ))
    .unwrap();
    assert!(
        svg.contains("Group") && svg.contains("Child") && svg.contains("stroke-dasharray=\"7,5\"")
    );
}

#[test]
fn an_edge_goes_around_the_block_between() {
    let laid = placed("block-beta\ncolumns 3\na b c\na -->|go| c\n");
    let middle = laid.nodes[1];
    assert!(laid.nodes[2].x > middle.right());
    for edge in &laid.edges {
        for window in edge.windows(2) {
            assert!(
                !segment_hits(window[0], window[1], middle.inflate(1.0)),
                "edge {edge:?} hits {middle:?}"
            );
        }
    }
    let label = laid.edge_labels[0].unwrap();
    for node in &laid.nodes {
        assert!(!label.intersects(*node), "label {label:?} node {node:?}");
    }
}

#[test]
fn an_edge_meets_the_side_it_enters() {
    let laid = placed(
        "block-beta\ncolumns 3\n  a[\"A\"] b[\"B\"] c[\"C\"]\n  d[\"D\"] e[\"E\"] f[\"F\"]\n  c --> e\n",
    );
    let target = laid.nodes[4];
    let edge = &laid.edges[0];
    let end = edge[edge.len() - 1];
    let prev = edge[edge.len() - 2];
    assert!(
        !super::route::segment_rides(prev, end, target),
        "edge {edge:?} rides {target:?}"
    );
    assert!(
        !segment_hits(prev, end, target.inflate(-0.5)),
        "edge {edge:?} enters {target:?}"
    );
}

#[test]
fn wraps_on_the_column_count_and_leaves_a_space() {
    let wrapped = placed("block-beta\ncolumns 2\nwide:2\nnext\n");
    assert!(wrapped.nodes[1].y > wrapped.nodes[0].bottom());
    let spaced = placed("block-beta\ncolumns 3\na space c\n");
    assert!(spaced.nodes[1].x > spaced.nodes[0].right() + 80.0);
}

#[test]
fn reads_shapes_labels_and_arrows() {
    let diagram = source(
        "block\n  a[\"A label\"] b(\"B\") c((\"C\")) d(((\"D\"))) e{\"E\"} f{{\"F\"}} g[[\"G\"]] h[(\"H\")] i([\"I\"]) j[/J/] k[\\K\\] l[/L\\] m[\\M/] n>N] o<[\"O\"]>(down) z\n",
    );
    let shapes = [
        Shape::Rectangle,
        Shape::Round,
        Shape::Circle,
        Shape::DoubleCircle,
        Shape::Diamond,
        Shape::Hexagon,
        Shape::Subroutine,
        Shape::Cylinder,
        Shape::Stadium,
        Shape::Parallelogram,
        Shape::ParallelogramAlt,
        Shape::Trapezoid,
        Shape::TrapezoidAlt,
        Shape::Asymmetric,
        Shape::Arrow(Direction::Down),
        Shape::Rectangle,
    ];
    assert_eq!(diagram.nodes.len(), shapes.len());
    for (node, shape) in diagram.nodes.iter().zip(shapes) {
        assert_eq!(node.shape, shape, "{}", node.id);
    }
    assert_eq!(diagram.nodes[0].label, "A label");
    assert_eq!(diagram.nodes[15].label, "z");
    let edge = source("block-beta\na b c\na -- go --> b\na & b --> c\n");
    assert_eq!(edge.edges[0].label.as_ref().unwrap().value, "go");
    assert!(edge.edges[0].arrow_end && !edge.edges[0].arrow_start);
    assert_eq!(edge.edges.len(), 3);
    let style = source(
        "block-beta\na b\nstyle a fill:#FF00AA,stroke:#112233,color:#fff,stroke-width:3px\n",
    );
    assert_eq!(style.nodes[0].fill, Some(Color::hex(0xFF00AA)));
    assert_eq!(style.nodes[0].stroke, Some(Color::hex(0x112233)));
    assert_eq!(style.nodes[0].text, Some(Color::hex(0xFFFFFF)));
    assert_eq!(style.nodes[0].stroke_width, Some(3.0));
}

#[test]
fn names_the_line_it_refuses() {
    let cases = [
        ("block-beta\nclassDef a fill:#fff\n", 1, "a class"),
        ("block-beta\na\nclass a foo\n", 2, "a class"),
        ("block-beta\na:::hidden\n", 1, "a class"),
        ("block-beta\na b\nlinkStyle 0 stroke:#fff\n", 2, "a style"),
        ("block-beta\na\na\n", 2, "an element"),
        ("block-beta\na\na --> a\n", 2, "an edge"),
        ("block-beta\na --> b\n", 1, "an edge"),
        ("block-beta\nblock:g\n  a\n", 1, "a block"),
        ("block-beta\nend\n", 1, "a block"),
        ("block-beta\nend extra\n", 1, "a block"),
        ("block-beta\nblock:\nend\n", 1, "a block"),
        ("block-beta\ncolumns\n", 1, "a column"),
        ("block-beta\ncolumns 0\n", 1, "a column"),
        ("block-beta\ncolumns two\n", 1, "a column"),
        ("block-beta\ncolumns 2\ncolumns 1\n", 2, "a column"),
        ("block-beta\ntitle\n", 1, "a title"),
        ("block-beta\ntitle One\ntitle Two\n", 2, "a title"),
        ("block-beta\ndirection LR\n", 1, "an unknown line"),
        ("block-beta extra\n", 0, "an unknown line"),
        ("block-beta\nblock\n", 1, "an unknown line"),
        ("block-beta\nstyle missing fill:#fff\n", 1, "a style"),
        ("block-beta\na\nstyle a fill:nope\n", 2, "a style"),
        ("block-beta\na\nstyle a opacity:1\n", 2, "a style"),
        ("block-beta\naccDescr {\nstill\n", 1, "an unknown line"),
    ];
    for (text, line, what) in cases {
        assert_eq!(mermaid_refusal(text), Some((line, what)), "{text}");
        assert!(from_mermaid(text).is_none(), "{text}");
    }
    assert!(from_mermaid("flowchart TB\nblock --> next\n").is_none());
    assert!(mermaid_refusal("flowchart TB\nblock --> next\n").is_none());
}

#[test]
fn the_stock_theme_is_lilac_and_a_dark_page_uses_the_accent() {
    let text = "block-beta\ntitle Blocks\na[\"Web\"] b[\"DB\"]\na --> b\n";
    let svg = draw(&source(text)).unwrap();
    assert_eq!(draw(&source(text)).unwrap(), svg);
    assert!(svg.contains("Web") && svg.contains("Blocks"), "{svg}");
    assert!(
        svg.contains("#ECECFF")
            && svg.contains("#7B88A8")
            && svg.contains("#333333")
            && svg.contains("#2F3B4D"),
        "{svg}"
    );
    let mut dark = source(text);
    dark.look.classic = false;
    dark.look.accent = Color::hex(0xE68E0D);
    dark.look.line = Color::hex(0xE68E0D);
    dark.look.preferred = Color::hex(0xBEBEBE);
    dark.look.background = Some(Color::hex(0x121212));
    let svg = draw(&dark).unwrap();
    assert!(
        svg.contains("#E68E0D") && svg.contains("#BEBEBE") && svg.contains("#121212"),
        "{svg}"
    );
    assert!(!svg.contains("#ECECFF"), "{svg}");
    let chevron = draw(&source("block-beta\ncolumns 1\ngo<[\"flow\"]>(right)\n")).unwrap();
    assert!(
        chevron.contains("<polygon") && chevron.contains("flow"),
        "{chevron}"
    );
}

#[test]
fn a_label_stays_inside_its_rectangle() {
    let laid = placed("block-beta\na[\"Hello\"] b[\"World\"]\n");
    for (node, label) in laid.nodes.iter().zip(["Hello", "World"]) {
        let block = crate::text::block(label, Font::new(16.0, Color::BLACK), Family::Sans).unwrap();
        let left = node.cx() - block.width / 2.0;
        let top = node.cy() - block.height / 2.0;
        assert!(left >= node.x && top >= node.y, "{label}");
        assert!(left + block.width <= node.right() && top + block.height <= node.bottom());
    }
    assert!(!laid.nodes[0].intersects(laid.nodes[1]));
}
