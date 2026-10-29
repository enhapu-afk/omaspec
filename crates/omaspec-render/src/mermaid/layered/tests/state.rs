use mermaid_rs_renderer::{RenderOptions, parse_mermaid_strict};

use super::super::place::boxes_overlap;
use super::super::{Kind, NOTE_PAD, nest};

/// The SVG alone, without the map.
fn try_draw(
    graph: &mermaid_rs_renderer::Graph,
    theme: &mermaid_rs_renderer::Theme,
    source: &str,
) -> Option<String> {
    super::super::try_draw(graph, theme, source, None, 0.0).map(|(svg, _)| svg)
}
use super::{block_has, collect, encloses, find, laid, read, segment_cuts};

/// The way back from Review runs up beside the way there, between the
/// two states, as Graphviz draws it.
#[test]
fn lifecycle_steps_down_and_the_back_edge_runs_beside() {
    let scene = laid(include_str!("../../../../../../examples/lifecycle.mmd"));
    let draft = find(&scene, "Draft");
    let review = find(&scene, "Review");
    assert!(draft.rect.y + draft.rect.h <= review.rect.y + 0.5);
    let edge = scene
        .edges
        .iter()
        .find(|edge| block_has(&edge.label, "changes"))
        .unwrap();
    for &(x, y) in &edge.points {
        assert!(
            x >= draft.rect.x.max(review.rect.x) - 0.5,
            "{:?}, draft {:?}",
            edge.points,
            draft.rect
        );
        assert!(y >= draft.rect.y + draft.rect.h - 0.5 && y <= review.rect.y + 0.5);
    }
}

#[test]
fn a_nested_state_holds_its_children() {
    let scene = laid(include_str!(
        "../../../../../omaspec-core/tests/fixtures/state.mmd"
    ));
    let suspend = find(&scene, "Suspend");
    assert!(encloses(suspend.rect, find(&scene, "Saving").rect));
    assert!(encloses(suspend.rect, find(&scene, "Off").rect));
    assert!(!encloses(suspend.rect, find(&scene, "Lockd").rect));
    assert!(block_has(&find(&scene, "Screen").title, "saver"));
    for edge in &scene.edges {
        for pair in edge.points.windows(2) {
            assert!(
                !segment_cuts(pair[0], pair[1], suspend.rect),
                "a transition cuts Suspend: {pair:?}"
            );
        }
    }
}

#[test]
fn nested_states_without_a_crossing_edge_draw() {
    let source = "stateDiagram-v2
[*] --> Off
Off --> On : power
state On {
    [*] --> Idle
    Idle --> Playing : play
    state Playing {
        [*] --> Buffering
    }
}
On --> Error : fault";
    let scene = laid(source);
    let on = find(&scene, "On");
    let playing = find(&scene, "Playing");
    assert!(encloses(on.rect, playing.rect));
    assert!(encloses(playing.rect, find(&scene, "Buffering").rect));
    assert!(encloses(on.rect, find(&scene, "Idle").rect));
    assert!(!encloses(on.rect, find(&scene, "Error").rect));
}

#[test]
fn a_note_stays_on_the_right_under_left_to_right() {
    let scene = laid(
        "stateDiagram-v2\ndirection LR\n[*] --> Idle\nnote right of Idle : beside\nIdle --> Done\n",
    );
    let idle = find(&scene, "Idle");
    assert_eq!(scene.notes.len(), 1);
    assert!(scene.notes[0].x >= idle.rect.x + idle.rect.w);
}

#[test]
fn a_note_clears_the_self_loop_label() {
    let scene =
        laid("stateDiagram-v2\n[*] --> Idle\nIdle --> Idle : loop\nnote right of Idle : here\n");
    let note = &scene.notes[0];
    let edge = scene
        .edges
        .iter()
        .find(|edge| block_has(&edge.label, "loop"))
        .unwrap();
    let text = (
        note.x + NOTE_PAD,
        note.y + NOTE_PAD,
        note.text.width,
        note.text.height,
    );
    let label = (
        edge.label_at.0,
        edge.label_at.1,
        edge.label.width,
        edge.label.height,
    );
    assert!(
        !boxes_overlap(text, label),
        "note {text:?} covers {label:?}"
    );
}

#[test]
fn bottom_to_top_puts_the_later_state_above() {
    let scene = laid("stateDiagram-v2\ndirection BT\n[*] --> A\nA --> B\n");
    let a = find(&scene, "A");
    let b = find(&scene, "B");
    assert!(b.rect.y + b.rect.h <= a.rect.y + 0.5);
}

#[test]
fn right_to_left_puts_the_later_state_on_the_left() {
    let scene = laid("stateDiagram-v2\ndirection RL\n[*] --> A\nA --> B\n");
    let a = find(&scene, "A");
    let b = find(&scene, "B");
    assert!(b.rect.x + b.rect.w <= a.rect.x + 0.5);
}

#[test]
fn a_description_is_drawn_without_the_divider() {
    let source = "stateDiagram-v2\n[*] --> Idle\nIdle : quiet\n";
    let scene = laid(source);
    assert!(block_has(&find(&scene, "Idle").description[0], "quiet"));
    let svg = try_draw(
        &parse_mermaid_strict(source).unwrap().graph,
        &RenderOptions::default().theme,
        source,
    )
    .unwrap();
    assert!(!svg.contains("---"), "{svg}");
    assert!(svg.contains("quiet"));
}

#[test]
fn bidirectional_arrows_have_two_heads_and_dotted_arrows_dash() {
    let theme = RenderOptions::default().theme;
    let both = "stateDiagram-v2\nA <--> B\n";
    let svg = try_draw(&parse_mermaid_strict(both).unwrap().graph, &theme, both).unwrap();
    assert_eq!(svg.matches("<polygon").count(), 2, "{svg}");
    let one = "stateDiagram-v2\nA --> B\n";
    let svg = try_draw(&parse_mermaid_strict(one).unwrap().graph, &theme, one).unwrap();
    assert_eq!(svg.matches("<polygon").count(), 1, "{svg}");
    let dotted = "stateDiagram-v2\nA ..> B\n";
    let svg = try_draw(&parse_mermaid_strict(dotted).unwrap().graph, &theme, dotted).unwrap();
    assert!(svg.contains("stroke-dasharray"), "{svg}");
}

#[test]
fn an_explicit_color_is_drawn_and_an_unreadable_one_falls_back() {
    let theme = RenderOptions::default().theme;
    let colored = "stateDiagram-v2\n[*] --> Idle\nstyle Idle fill:#ff00aa\n";
    let svg = try_draw(
        &parse_mermaid_strict(colored).unwrap().graph,
        &theme,
        colored,
    )
    .unwrap();
    assert!(svg.to_ascii_lowercase().contains("#ff00aa"), "{svg}");
    let odd = "stateDiagram-v2\n[*] --> Idle\nstyle Idle fill:rgb(1,2,3)\n";
    assert!(try_draw(&parse_mermaid_strict(odd).unwrap().graph, &theme, odd).is_none());
}

#[test]
fn concurrent_regions_sit_side_by_side_each_with_its_own_start() {
    let scene = laid(
        "stateDiagram-v2\n[*] --> Active\nstate Active {\n[*] --> One\nOne --> Done\n--\n[*] --> Two\n}\n",
    );
    let active = find(&scene, "Active");
    let regions = &active.children.nodes;
    assert_eq!(regions.len(), 2);
    assert!(regions.iter().all(|region| region.kind == Kind::Region));
    assert!(regions[1].rect.x >= regions[0].rect.x + regions[0].rect.w - 0.5);
    for region in regions {
        assert!(encloses(active.rect, region.rect));
        let starts = region
            .children
            .nodes
            .iter()
            .filter(|node| node.kind == Kind::Start)
            .count();
        assert_eq!(starts, 1, "each region starts on its own");
    }
    assert!(encloses(regions[0].rect, find(&scene, "One").rect));
    assert!(encloses(regions[1].rect, find(&scene, "Two").rect));
    assert_eq!(nest::separators(active).len(), 1);
}

#[test]
fn a_transition_out_of_a_nested_state_is_routed_around() {
    let scene = laid(
        "stateDiagram-v2\n[*] --> Off\nOff --> On\nstate On {\n[*] --> Idle\nIdle --> Busy\n}\nBusy --> Error : fault\nOff --> Idle : wake\n",
    );
    let mut placed = Vec::new();
    collect(&scene, &mut placed);
    let wake = scene
        .edges
        .iter()
        .find(|edge| block_has(&edge.label, "wake"))
        .expect("drawn at the top");
    let (off, idle) = (find(&scene, "Off").rect, find(&scene, "Idle").rect);
    let on = find(&scene, "On").rect;
    for rect in placed {
        if [off, idle, on].contains(&rect) {
            continue;
        }
        for pair in wake.points.windows(2) {
            assert!(
                !segment_cuts(pair[0], pair[1], rect),
                "{:?} cuts {rect:?}",
                wake.points
            );
        }
    }
    assert!(
        scene
            .edges
            .iter()
            .any(|edge| block_has(&edge.label, "fault"))
    );
}

#[test]
fn a_transition_into_its_own_nested_state_falls_back() {
    let theme = RenderOptions::default().theme;
    let source = "stateDiagram-v2\nstate On {\n[*] --> Idle\n}\nOn --> Idle\n";
    assert!(try_draw(&parse_mermaid_strict(source).unwrap().graph, &theme, source).is_none());
    let (graph, lines) = read(source);
    assert!(super::super::try_draw(&graph, &theme, source, Some(&lines), 0.0).is_none());
    // mmdr lays out Omaspec's reading, so the picture still shows the
    // machine that is checked.
    assert!(crate::mermaid::render(source).unwrap().contains("Idle"));
    assert!(crate::mermaid::coverage(source).unwrap().is_some());
}

#[test]
fn render_uses_this_drawing() {
    let source = "stateDiagram-v2\n[*] --> Idle\nIdle --> Busy : job\n";
    let (graph, lines) = read(source);
    let theme = RenderOptions::default().theme;
    let native = super::super::try_draw(&graph, &theme, source, Some(&lines), 0.0).unwrap();
    assert_eq!(crate::mermaid::render(source).unwrap(), native.0);
}

/// Where mmdr reads a state diagram as Omaspec does, Omaspec's reading
/// draws the picture mmdr's parse gave.
#[test]
fn the_own_reading_draws_what_mmdr_parsed() {
    let theme = RenderOptions::default().theme;
    for source in [
        include_str!("../../../../../omaspec-core/tests/fixtures/state.mmd"),
        include_str!("../../../../../../examples/lifecycle.mmd"),
        "stateDiagram-v2\ndirection LR\n[*] --> Idle\nnote right of Idle : beside\nIdle --> Done\n",
        "stateDiagram-v2\n[*] --> Active\nstate Active {\n[*] --> One\nOne --> Done\n--\n[*] --> Two\n}\n",
        "stateDiagram-v2\n[*] --> Off\nOff --> On\nstate On {\n[*] --> Idle\nIdle --> Busy\n}\nBusy --> Error : fault\nOff --> Idle : wake\n",
        "stateDiagram-v2\n[*] --> On\nstate On {\n[*] --> A\nA --> B\n[*] --> C\nB --> [*]\nC --> [*]\n}\n",
        "stateDiagram-v2\n[*] --> Idle\nIdle : quiet\nIdle : still\nstyle Idle fill:#ff00aa\nclassDef hot stroke:#f00,stroke-width:3px\nclass Idle hot\nstate pick <<choice>>\nIdle --> pick\npick --> [*] : yes\nstate \"Far away\" as Gone\npick --> Gone : no\n",
        "stateDiagram-v2\n[*] --> Draft\nnote right of Draft\n    two\n    lines\nend note\nDraft --> [*]\n",
    ] {
        let folded = crate::mermaid::fold_multiline_notes(source);
        let parsed = parse_mermaid_strict(&folded).unwrap();
        let before = try_draw(&parsed.graph, &theme, source).unwrap();
        let (graph, lines) = read(source);
        let after = super::super::try_draw(&graph, &theme, source, Some(&lines), 0.0).unwrap();
        assert_eq!(before, after.0, "{source}");
    }
}

#[test]
fn the_map_knows_the_lines() {
    let source = "```mermaid\nstateDiagram-v2\n[*] --> Idle\nIdle --> Busy : job\nstate Busy {\n[*] --> Work\nWork --> [*]\n[*] --> Rest\n}\n```\n";
    let (_, _, map) = crate::mermaid::render_mapped(source).unwrap();
    let mut states: Vec<(&str, usize)> = map
        .states
        .iter()
        .map(|s| (s.name.as_str(), s.line))
        .collect();
    states.sort();
    // Busy's start leads to two states, so it is a bar.
    assert_eq!(
        states,
        [
            ("", 2),
            ("", 5),
            ("", 6),
            ("Busy", 3),
            ("Idle", 2),
            ("Rest", 7),
            ("Work", 5)
        ]
    );
    let mut lines: Vec<usize> = map.transitions.iter().map(|t| t.line).collect();
    lines.sort();
    assert_eq!(lines, [2, 3, 5, 6, 7]);
}
