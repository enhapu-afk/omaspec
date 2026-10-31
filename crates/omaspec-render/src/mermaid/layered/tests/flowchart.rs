use mermaid_rs_renderer::{RenderOptions, parse_mermaid_strict};

use super::super::shape::Shape;
use super::super::{Cap, Kind};

/// The SVG alone, without the map.
fn try_draw(
    graph: &mermaid_rs_renderer::Graph,
    theme: &mermaid_rs_renderer::Theme,
    source: &str,
) -> Option<String> {
    super::super::try_draw(graph, theme, source, None, &[], 0.0).map(|(svg, _)| svg)
}
use super::{block_has, collect, encloses, find, laid, segment_cuts};

fn draw(source: &str) -> Option<String> {
    let parsed = parse_mermaid_strict(source).unwrap();
    try_draw(&parsed.graph, &RenderOptions::default().theme, source)
}

#[test]
fn the_shop_steps_down_and_loops_back_beside_it() {
    let scene = laid(include_str!("../../../../../../examples/shop.mmd"));
    let browse = find(&scene, "Browse");
    let pay = find(&scene, "Pay");
    let saved = find(&scene, "Save");
    assert_eq!(pay.kind, Kind::Node(Shape::Diamond));
    assert_eq!(find(&scene, "Order").kind, Kind::Node(Shape::Stadium));
    assert!(browse.rect.y + browse.rect.h <= pay.rect.y + 0.5);
    assert!(pay.rect.y + pay.rect.h <= saved.rect.y + 0.5);
    let back = scene
        .edges
        .iter()
        .find(|edge| {
            edge.points
                .first()
                .is_some_and(|p| p.1 > browse.rect.y + 50.0)
                && edge.to == 0
        })
        .expect("the way back to Browse");
    for rect in [pay.rect, find(&scene, "Add").rect] {
        for pair in back.points.windows(2) {
            assert!(!segment_cuts(pair[0], pair[1], rect), "{:?}", back.points);
        }
    }
}

#[test]
fn a_subgraph_runs_its_own_way_and_holds_its_nodes() {
    let scene = laid(include_str!(
        "../../../../../omaspec-core/tests/fixtures/flowchart.mmd"
    ));
    let ci = find(&scene, "Continuous");
    assert_eq!(ci.kind, Kind::Cluster);
    for word in ["Push", "Tests", "Package", "Fix"] {
        assert!(encloses(ci.rect, find(&scene, word).rect), "{word}");
    }
    for word in ["Release", "Announce", "Mirror"] {
        assert!(!encloses(ci.rect, find(&scene, word).rect), "{word}");
    }
    // `direction LR` inside, top to bottom outside.
    let (push, tests) = (find(&scene, "Push").rect, find(&scene, "Tests").rect);
    assert!(tests.x >= push.x + push.w - 0.5);
    let (release, announce) = (find(&scene, "Release").rect, find(&scene, "Announce").rect);
    assert!(announce.y >= release.y + release.h - 0.5);
    assert!(release.y >= ci.rect.y + ci.rect.h - 0.5);
    // `((Release))` is a circle, as in Mermaid.
    assert_eq!(find(&scene, "Release").kind, Kind::Node(Shape::Circle));
    assert_eq!(find(&scene, "Announce").kind, Kind::Node(Shape::Flag));
    assert_eq!(find(&scene, "Mirror").kind, Kind::Node(Shape::Hexagon));
    assert_eq!(find(&scene, "Fix").kind, Kind::Node(Shape::Subroutine));
}

#[test]
fn a_line_out_of_a_subgraph_is_routed_around_the_rest() {
    let scene = laid(include_str!(
        "../../../../../omaspec-core/tests/fixtures/flowchart.mmd"
    ));
    // `C --> E` leaves the subgraph; it is drawn at the top level.
    let (package, release) = (find(&scene, "Package").rect, find(&scene, "Release").rect);
    let mut boxes = Vec::new();
    collect(&scene, &mut boxes);
    let line = scene
        .edges
        .iter()
        .find(|edge| {
            // On the circle, whichever side it comes in from.
            let end = edge.points.last().copied().unwrap_or_default();
            let (cx, cy) = (release.x + release.w / 2.0, release.y + release.h / 2.0);
            ((end.0 - cx).hypot(end.1 - cy) - release.w / 2.0).abs() < 1.0
        })
        .expect("the line into Release");
    let start = line.points[0];
    assert!(
        start.0 >= package.x - 0.5 && start.0 <= package.x + package.w + 0.5,
        "{:?} from {package:?}",
        line.points
    );
    let ci = find(&scene, "Continuous").rect;
    for rect in boxes {
        if [package, release, ci].contains(&rect) {
            continue;
        }
        for pair in line.points.windows(2) {
            assert!(
                !segment_cuts(pair[0], pair[1], rect),
                "{:?} cuts {rect:?}",
                line.points
            );
        }
    }
}

/// The pipeline of the gallery: its subgraphs take the ranks of the whole
/// chart, and a line from one to another runs through them like any
/// other, from the side the flow leaves by to the side it enters by.
#[test]
fn lines_between_subgraphs_run_through_the_ranks() {
    let scene = laid(include_str!(
        "../../../../../../examples/gallery/pipeline.mmd"
    ));
    let (unit, linux) = (find(&scene, "Unit").rect, find(&scene, "Linux").rect);
    let build = find(&scene, "Build");
    assert!(encloses(build.rect, linux));
    let line = scene
        .edges
        .iter()
        .chain(scene.nodes.iter().flat_map(|node| &node.children.edges))
        .find(|edge| {
            let (start, end) = (edge.points[0], *edge.points.last().unwrap());
            (start.0 - (unit.x + unit.w)).abs() < 0.5
                && (end.0 - linux.x).abs() < 0.5
                && end.1 > linux.y
                && end.1 < linux.y + linux.h
        })
        .expect("Unit tests to Linux binary, out right and in left");
    let mut boxes = Vec::new();
    collect(&scene, &mut boxes);
    for rect in boxes {
        if [unit, linux].contains(&rect) || encloses(rect, unit) || encloses(rect, linux) {
            continue;
        }
        for pair in line.points.windows(2) {
            assert!(
                !segment_cuts(pair[0], pair[1], rect),
                "{:?} cuts {rect:?}",
                line.points
            );
        }
    }
}

#[test]
fn nested_subgraphs_nest() {
    let source = "flowchart TB
    subgraph Outer[Outer box]
        subgraph Inner[Inner box]
            A[one] --> B[two]
        end
        C[three]
    end
    D[four]
    B --> C
    C --> D
    A --> D";
    let scene = laid(source);
    let outer = find(&scene, "Outer");
    let inner = find(&scene, "Inner");
    assert!(encloses(outer.rect, inner.rect));
    assert!(encloses(inner.rect, find(&scene, "one").rect));
    assert!(encloses(inner.rect, find(&scene, "two").rect));
    assert!(encloses(outer.rect, find(&scene, "three").rect));
    assert!(!encloses(inner.rect, find(&scene, "three").rect));
    assert!(!encloses(outer.rect, find(&scene, "four").rect));
}

#[test]
fn a_line_may_end_on_a_subgraph() {
    let source = "flowchart LR\n  A[start] --> S\n  subgraph S[The group]\n    B[inside]\n  end\n";
    let scene = laid(source);
    let group = find(&scene, "group");
    let line = &scene.edges[0];
    let end = *line.points.last().unwrap();
    assert!(
        (end.0 - group.rect.x).abs() < 0.5,
        "{:?} {:?}",
        line.points,
        group.rect
    );
}

#[test]
fn ends_are_arrows_circles_crosses_or_nothing() {
    let scene = laid("flowchart LR\nA --> B\nB --o C\nC --x D\nD --- E\nE <--> F\n");
    let caps: Vec<(Cap, Cap)> = scene.edges.iter().map(|edge| edge.caps).collect();
    assert_eq!(
        caps,
        [
            (Cap::None, Cap::Arrow),
            (Cap::None, Cap::Circle),
            (Cap::None, Cap::Cross),
            (Cap::None, Cap::None),
            (Cap::Arrow, Cap::Arrow),
        ]
    );
    let svg = draw("flowchart LR\nA --o B\n").unwrap();
    assert!(svg.contains("<ellipse"), "{svg}");
}

#[test]
fn labels_wrap_and_markup_is_read_the_mermaid_way() {
    let scene = laid(
        "flowchart TD\nA[\"Order history with filters, export and paging\"] -->|a -- b| B[\"<b>bold</b> #quot;q#quot;\"]\n",
    );
    let long = find(&scene, "Order");
    assert!(long.title.lines.len() > 1);
    assert!(long.title.width <= 200.0 * 14.0 / 16.0 + 0.5);
    let edge = &scene.edges[0];
    assert!(block_has(&edge.label, "--"), "{:?}", edge.label);
    let bold = find(&scene, "bold");
    assert!(bold.title.lines[0].runs[0].style.bold);
    assert!(block_has(&bold.title, "\"q\""));
}

#[test]
fn a_double_circle_is_drawn_with_two_rings() {
    let scene = laid("flowchart LR\nA((one)) --> B(((two)))\n");
    assert_eq!(find(&scene, "one").kind, Kind::Node(Shape::Circle));
    assert_eq!(find(&scene, "two").kind, Kind::Node(Shape::DoubleCircle));
}

#[test]
fn a_line_into_a_diamond_ends_on_its_outline() {
    let scene = laid("flowchart TD\nA[a] --> C{choose}\nB[b] --> C\nC --> D[d]\nC --> E[e]\n");
    let choose = find(&scene, "choose").rect;
    let outline = Shape::Diamond.outline(choose);
    for edge in &scene.edges {
        for end in [edge.points[0], *edge.points.last().unwrap()] {
            if !encloses(
                choose,
                super::super::Rect {
                    x: end.0,
                    y: end.1,
                    w: 0.0,
                    h: 0.0,
                },
            ) {
                continue;
            }
            let near = outline
                .iter()
                .zip(outline.iter().cycle().skip(1))
                .any(|(a, b)| {
                    let cross = (b.0 - a.0) * (end.1 - a.1) - (b.1 - a.1) * (end.0 - a.0);
                    let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                    (cross / len).abs() < 0.6
                });
            assert!(near, "{end:?} is not on {outline:?}");
        }
    }
}

#[test]
fn class_default_and_styles_color_the_nodes() {
    let svg = draw(
        "flowchart LR\nclassDef default fill:#123456\nclassDef hot fill:#ff0000\nA --> B:::hot\nstyle A stroke:#00ff00\n",
    )
    .unwrap()
    .to_ascii_lowercase();
    assert!(svg.contains("#123456") && svg.contains("#ff0000") && svg.contains("#00ff00"));
}

#[test]
fn an_invisible_link_ranks_and_is_not_drawn() {
    let scene = laid("flowchart LR\nA ~~~ B\n");
    let (a, b) = (find(&scene, "A").rect, find(&scene, "B").rect);
    assert!(
        b.x >= a.x + a.w - 0.5,
        "B {b:?} is not to the right of A {a:?}"
    );
    assert!(scene.edges.is_empty(), "{:?}", scene.edges.len());
    let chained = laid("flowchart LR\nA[text] ~~~ B ~~~ C\n");
    assert!(find(&chained, "text").rect.x < find(&chained, "C").rect.x);
    assert!(chained.edges.is_empty());
}

#[test]
fn a_new_shape_is_the_shape_it_names() {
    let scene = laid("flowchart LR\nA@{ shape: cyl, label: \"db\" }\n");
    let node = find(&scene, "db");
    assert_eq!(node.kind, Kind::Node(Shape::Cylinder));
    assert!(block_has(&node.title, "db"));
    let diamond = laid("flowchart LR\nA@{ shape: diam }\n");
    assert_eq!(find(&diamond, "A").kind, Kind::Node(Shape::Diamond));
    let svg = draw("flowchart LR\nA@{ shape: cyl, label: \"db\" }\n").unwrap();
    assert!(svg.contains("db"), "{svg}");
    assert!(svg.contains('A'), "{svg}");
    let colored = draw("flowchart LR\nclassDef hot fill:#ff0000\nA@{ shape: rect }:::hot\n")
        .unwrap()
        .to_ascii_lowercase();
    assert!(colored.contains("#ff0000"), "{colored}");
}

#[test]
fn an_empty_subgraph_is_a_frame_and_a_nested_one_stays_inside() {
    let scene = laid("flowchart LR\nA --> B\nsubgraph Empty\nend\n");
    assert_eq!(find(&scene, "Empty").kind, Kind::Cluster);
    assert!(!encloses(
        find(&scene, "Empty").rect,
        find(&scene, "A").rect
    ));
    let nested = laid("flowchart TB\nsubgraph Outer\nsubgraph Inner\nend\nend\n");
    let (outer, inner) = (find(&nested, "Outer"), find(&nested, "Inner"));
    assert_eq!(outer.kind, Kind::Cluster);
    assert_eq!(inner.kind, Kind::Cluster);
    assert!(encloses(outer.rect, inner.rect));
}

#[test]
fn a_line_into_a_node_inside_its_subgraph_stays_inside() {
    let scene = laid("flowchart LR\nsubgraph S\nA\nend\nS --> A\n");
    let group = find(&scene, "S");
    let node = find(&scene, "A");
    assert!(encloses(group.rect, node.rect));
    let line = scene
        .edges
        .iter()
        .find(|edge| edge.caps.1 == Cap::Arrow)
        .expect("the arrow into A");
    let start = line.points[0];
    let end = *line.points.last().unwrap();
    assert!(on_border(start, group.rect), "{start:?} {:?}", group.rect);
    assert!(on_border(end, node.rect), "{end:?} {:?}", node.rect);
    for point in &line.points {
        assert!(
            inside(*point, group.rect),
            "{point:?} leaves {:?}",
            group.rect
        );
    }
    let back = laid("flowchart LR\nsubgraph S\nA\nend\nA --> S\n");
    let group = find(&back, "S").rect;
    let line = back
        .edges
        .iter()
        .find(|edge| edge.caps.1 == Cap::Arrow)
        .unwrap();
    assert!(on_border(*line.points.last().unwrap(), group));
    assert!(
        inside(line.points[0], find(&back, "A").rect)
            || on_border(line.points[0], find(&back, "A").rect)
    );
}

#[test]
fn an_icon_is_its_short_name_in_its_form() {
    let scene = laid("flowchart LR\nA@{ icon: \"fa:user\", form: \"circle\", label: \"User\" }\n");
    let node = find(&scene, "user");
    assert_eq!(node.kind, Kind::Node(Shape::Circle));
    assert!(block_has(&node.title, "User"));
    let above = laid("flowchart LR\nA@{ icon: \"fa:user\", label: \"User\", pos: \"t\" }\n");
    let node = find(&above, "user");
    assert_eq!(node.kind, Kind::Node(Shape::Label));
    let first: String = node.title.lines[0]
        .runs
        .iter()
        .map(|run| run.text.as_str())
        .collect();
    let second: String = node.title.lines[1]
        .runs
        .iter()
        .map(|run| run.text.as_str())
        .collect();
    assert_eq!(first, "User");
    assert_eq!(second, "user");
    let group = laid("flowchart LR\nsubgraph S@{ icon: \"fa:user\", label: \"People\" }\nA\nend\n");
    assert!(block_has(&find(&group, "user").title, "People"));
}

#[test]
fn an_unknown_shape_or_an_image_falls_back() {
    for source in [
        "flowchart LR\nA@{ shape: not-a-shape }\n",
        "flowchart LR\nA@{ img: \"https://example.com/a.png\" }\n",
    ] {
        assert!(draw(source).is_none(), "{source}");
        assert!(crate::mermaid::render(source).is_ok(), "{source}");
    }
}

fn on_border(point: (f32, f32), rect: super::super::Rect) -> bool {
    let (x, y) = point;
    let near = |a: f32, b: f32| (a - b).abs() < 0.8;
    let in_x = x >= rect.x - 0.8 && x <= rect.x + rect.w + 0.8;
    let in_y = y >= rect.y - 0.8 && y <= rect.y + rect.h + 0.8;
    in_x && in_y
        && (near(x, rect.x)
            || near(x, rect.x + rect.w)
            || near(y, rect.y)
            || near(y, rect.y + rect.h))
}

fn inside(point: (f32, f32), rect: super::super::Rect) -> bool {
    point.0 >= rect.x - 0.8
        && point.0 <= rect.x + rect.w + 0.8
        && point.1 >= rect.y - 0.8
        && point.1 <= rect.y + rect.h + 0.8
}

#[test]
fn bottom_to_top_and_right_to_left_mirror() {
    let up = laid("flowchart BT\nA[first] --> B[second]\n");
    assert!(
        find(&up, "second").rect.y + find(&up, "second").rect.h <= find(&up, "first").rect.y + 0.5
    );
    let left = laid("flowchart RL\nA[first] --> B[second]\n");
    assert!(
        find(&left, "second").rect.x + find(&left, "second").rect.w
            <= find(&left, "first").rect.x + 0.5
    );
}

#[test]
fn render_uses_this_drawing() {
    let source = "flowchart LR\n  A[one] --> B(two)\n";
    assert_eq!(
        crate::mermaid::render(source).unwrap(),
        draw(source).unwrap()
    );
}

/// Found by fuzzing: a subgraph whose title is wider than what it holds
/// leaves no way in from above, so the free places on the node lead
/// nowhere; a line then ends beside another line's end rather than the
/// whole diagram going to mmdr.
#[test]
fn a_wide_title_still_lets_lines_in() {
    let words = "word ".repeat(20);
    let source = format!(
        "flowchart TB\n    subgraph Cloud[\"Web platform\"]\n        subgraph Edge[\"Edge\"]\n        end\n        subgraph App[\"{words}\"]\n            Db[(\"Primary database\")]\n        end\n    end\n    Orders --> Db\n    Billing --> Db\n    Auth --> Db\n    Db ==> Replica\n"
    );
    let scene = laid(&source);
    let title = super::super::nest::cluster_title(find(&scene, "word")).unwrap();
    assert_eq!(scene.edges.len(), 4);
    for edge in &scene.edges {
        for pair in edge.points.windows(2) {
            assert!(!segment_cuts(pair[0], pair[1], title), "{:?}", edge.points);
        }
    }
}
