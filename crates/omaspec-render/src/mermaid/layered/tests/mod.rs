use mermaid_rs_renderer::{RenderOptions, parse_mermaid_strict};

use super::{Block, Family, Placed, Rect, Scene, Spot, layout_spot, nest, palette, prepare};

mod flowchart;
mod mutated;
mod recover;

/// The flowchart laid out and its lines between subgraphs routed, as
/// [`super::try_draw`] does before painting.
pub(super) fn laid(source: &str) -> Scene {
    let parsed = parse_mermaid_strict(source).unwrap();
    let graph = parsed.graph;
    let mut repaired = None;
    let (graph, fix) = super::flowchart::apply(&graph, source, &mut repaired).unwrap();
    let theme = RenderOptions::default().theme;
    let tree = prepare(graph, source, &fix).unwrap();
    let palette = palette(&theme).unwrap();
    let family = Family::of(&theme.font_family);
    let mut scene = layout_spot(&tree, Spot::Root, &palette, family).unwrap();
    nest::link_crossings(&tree, &mut scene, &palette, family).unwrap();
    nest::link_inward(&tree, &mut scene, &palette, family).unwrap();
    scene
}

pub(super) fn block_has(block: &Block, word: &str) -> bool {
    block.lines.iter().any(|line| {
        line.runs
            .iter()
            .any(|run| run.text.split_whitespace().any(|part| part == word))
    })
}

pub(super) fn walk<'a>(scene: &'a Scene, word: &str) -> Option<&'a Placed> {
    for node in &scene.nodes {
        if block_has(&node.title, word)
            || node.description.iter().any(|block| block_has(block, word))
        {
            return Some(node);
        }
        if let Some(found) = walk(&node.children, word) {
            return Some(found);
        }
    }
    None
}

pub(super) fn find<'a>(scene: &'a Scene, word: &str) -> &'a Placed {
    walk(scene, word).unwrap_or_else(|| panic!("missing {word}"))
}

pub(super) fn encloses(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x - 0.5
        && inner.y >= outer.y - 0.5
        && inner.x + inner.w <= outer.x + outer.w + 0.5
        && inner.y + inner.h <= outer.y + outer.h + 0.5
}

pub(super) fn segment_cuts(a: (f32, f32), b: (f32, f32), rect: Rect) -> bool {
    let (x0, y0) = a;
    let (x1, y1) = b;
    if (y0 - y1).abs() <= 0.01 {
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        y0 > rect.y + 0.5
            && y0 < rect.y + rect.h - 0.5
            && x0 < rect.x + rect.w - 0.5
            && rect.x < x1 - 0.5
    } else if (x0 - x1).abs() <= 0.01 {
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        x0 > rect.x + 0.5
            && x0 < rect.x + rect.w - 0.5
            && y0 < rect.y + rect.h - 0.5
            && rect.y < y1 - 0.5
    } else {
        true
    }
}

/// Every box.
pub(super) fn collect(scene: &Scene, out: &mut Vec<Rect>) {
    for node in &scene.nodes {
        out.push(node.rect);
        collect(&node.children, out);
    }
}
