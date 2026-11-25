//! Swimlanes (REQ-089): a flowchart laid out as it is along its flow, its
//! nodes then placed across it in bands of one size, one for each lane, in
//! the order the lanes are written, with the lane's name at the start of
//! its band. Within a lane the nodes keep the order the layout gave them;
//! two that would meet along the flow stand side by side. The lines are
//! routed again around the boxes and the names, so none runs through a
//! lane's name.
//!
//! The flow runs along the bands: to the right (`LR`, `RL`) the lanes are
//! rows and their names stand at their left; down (`TB`, `BT`) they are
//! columns and their names stand over them.

use std::collections::HashMap;

use mermaid_rs_renderer::Direction as Flow;
use omaspec_uml::graph::{Rect, route};
use omaspec_uml::{Align, Block, Canvas, Family, Font, Layer, Stroke};

use super::super::swimlane::Lanes;
use super::look::Palette;
use super::{PlacedEdge, Scene, Spot, Tree};

/// Around the nodes inside a band.
const PAD: f32 = 22.0;
/// Between two nodes of a lane that stand side by side.
const SIDE_GAP: f32 = 24.0;
/// Two nodes nearer than this along the flow stand side by side.
const ALONG_GAP: f32 = 18.0;
/// Around a lane's name in its place.
const TITLE_PAD: f32 = 12.0;
/// A band is never thinner than this.
const LEAST_BAND: f32 = 64.0;

/// The bands, as placed, to paint under the scene.
pub(super) struct Bands {
    bands: Vec<Band>,
}

struct Band {
    /// The whole band, its name's place included.
    rect: Rect,
    /// Where its name stands.
    title: Rect,
    words: Block,
    /// The line that opens the lane: its words are that line's.
    line: usize,
}

/// Places the nodes of `scene`, the root of `tree`'s layout, in the bands
/// of `lanes`, routes its lines again, to be drawn with `curve`, and gives
/// the bands to paint. `None` where the layout holds anything but nodes.
pub(super) fn place(
    tree: &Tree,
    scene: &mut Scene,
    lanes: &Lanes,
    palette: &Palette,
    family: Family,
    curve: f32,
) -> Option<Bands> {
    if lanes.lanes.is_empty()
        || scene
            .nodes
            .iter()
            .any(|node| !node.children.nodes.is_empty())
    {
        return None;
    }
    let rows = matches!(tree.flow(Spot::Root), Flow::LeftRight | Flow::RightLeft);
    // Which node of the scene each id is, and its lane.
    let ids: HashMap<usize, &String> = tree
        .paths
        .iter()
        .filter_map(|(id, path)| (path.len() == 1).then_some((path[0], id)))
        .collect();
    let lane_of = lanes_of(tree, scene, lanes, &ids);
    // What each node takes up: its box and the loops around it with their
    // words, which move with it.
    let index: HashMap<&String, usize> = ids.iter().map(|(&node, &id)| (id, node)).collect();
    let mut reach: Vec<Rect> = scene.nodes.iter().map(|n| n.rect).collect();
    for wire in &scene.edges {
        if let Some((from, to)) = ends(tree, &index, wire)
            && from == to
        {
            for &point in &wire.points {
                reach[from] = grown(reach[from], point);
            }
            if wire.label.width > 0.0 {
                let (x, y) = wire.label_at;
                reach[from] = grown(reach[from], (x, y));
                reach[from] = grown(reach[from], (x + wire.label.width, y + wire.label.height));
            }
        }
    }

    let font = Font::new(palette.size, palette.text).bold();
    let titles: Vec<Block> = lanes
        .lanes
        .iter()
        .map(|lane| super::label::measure(&lane.title, font, family, None))
        .collect();
    // The strip at the start of the bands where their names stand.
    let strip = titles
        .iter()
        .map(|t| if rows { t.width } else { t.height })
        .fold(0.0, f32::max)
        + 2.0 * TITLE_PAD;
    let least_band = titles
        .iter()
        .map(|t| if rows { t.height } else { t.width })
        .fold(0.0, f32::max)
        + 2.0 * TITLE_PAD;

    // Along the flow each node keeps its place; across it, the nodes of a
    // lane stand in tracks, in the order the layout gave them.
    let along = |r: Rect| if rows { (r.x, r.w) } else { (r.y, r.h) };
    let across = |r: Rect| if rows { (r.y, r.h) } else { (r.x, r.w) };
    let start = reach
        .iter()
        .map(|&r| along(r).0)
        .fold(f32::INFINITY, f32::min);
    let mut tracks: Vec<Vec<Track>> = vec![Vec::new(); lanes.lanes.len()];
    let mut placed: Vec<(usize, usize)> = vec![(0, 0); scene.nodes.len()];
    let mut order: Vec<usize> = (0..scene.nodes.len()).collect();
    order.sort_by(|&a, &b| across(reach[a]).0.total_cmp(&across(reach[b]).0));
    for &node in &order {
        let lane = lane_of[node];
        let (from, size) = along(reach[node]);
        let (_, thick) = across(reach[node]);
        let span = (from - ALONG_GAP, from + size + ALONG_GAP);
        let lane_tracks = &mut tracks[lane];
        let at = match lane_tracks
            .iter()
            .position(|track| track.spans.iter().all(|&(a, b)| b <= span.0 || a >= span.1))
        {
            Some(at) => at,
            None => {
                lane_tracks.push(Track::default());
                lane_tracks.len() - 1
            }
        };
        let track = &mut lane_tracks[at];
        track.spans.push((from, from + size));
        track.thick = track.thick.max(thick);
        placed[node] = (lane, at);
    }
    let content = |lane: &[Track]| {
        lane.iter().map(|t| t.thick).sum::<f32>() + SIDE_GAP * lane.len().saturating_sub(1) as f32
    };
    let band = tracks
        .iter()
        .map(|lane| content(lane) + 2.0 * PAD)
        .fold(LEAST_BAND.max(least_band), f32::max);

    // Each node in its band, the bands one after another across the flow.
    let mut length = 0.0f32;
    let mut moved = vec![(0.0, 0.0); scene.nodes.len()];
    for (node, &(lane, at)) in placed.iter().enumerate() {
        let lane_tracks = &tracks[lane];
        let inner = band - 2.0 * PAD;
        let mut offset = lane as f32 * band + PAD + (inner - content(lane_tracks)) / 2.0;
        for track in &lane_tracks[..at] {
            offset += track.thick + SIDE_GAP;
        }
        let r = reach[node];
        let (from, size) = along(r);
        let (_, thick) = across(r);
        let cross = offset + (lane_tracks[at].thick - thick) / 2.0;
        let along_at = from - start + strip + PAD;
        length = length.max(along_at + size + PAD);
        let (x, y) = if rows {
            (along_at, cross)
        } else {
            (cross, along_at)
        };
        moved[node] = (x - r.x, y - r.y);
        let rect = &mut scene.nodes[node].rect;
        rect.x += moved[node].0;
        rect.y += moved[node].1;
    }
    let bands: Vec<Band> = titles
        .into_iter()
        .zip(&lanes.lanes)
        .enumerate()
        .map(|(lane, (words, written))| {
            let at = lane as f32 * band;
            let (rect, title) = if rows {
                (
                    Rect {
                        x: 0.0,
                        y: at,
                        w: length,
                        h: band,
                    },
                    Rect {
                        x: 0.0,
                        y: at,
                        w: strip,
                        h: band,
                    },
                )
            } else {
                (
                    Rect {
                        x: at,
                        y: 0.0,
                        w: band,
                        h: length,
                    },
                    Rect {
                        x: at,
                        y: 0.0,
                        w: band,
                        h: strip,
                    },
                )
            };
            Band {
                rect,
                title,
                words,
                line: written.line,
            }
        })
        .collect();
    let across_all = band * lanes.lanes.len() as f32;
    (scene.width, scene.height) = if rows {
        (length, across_all)
    } else {
        (across_all, length)
    };
    reroute(tree, scene, &ids, &bands, &moved, curve);
    Some(Bands { bands })
}

/// `r` grown to hold `point`.
fn grown(r: Rect, (x, y): (f32, f32)) -> Rect {
    let (left, top) = (r.x.min(x), r.y.min(y));
    let (right, bottom) = ((r.x + r.w).max(x), (r.y + r.h).max(y));
    Rect {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    }
}

/// The nodes of the scene a line joins, by their place in it.
fn ends(tree: &Tree, index: &HashMap<&String, usize>, wire: &PlacedEdge) -> Option<(usize, usize)> {
    let edge = tree.graph.edges.get(wire.look.edge)?;
    Some((*index.get(&edge.from)?, *index.get(&edge.to)?))
}

/// The nodes of one lane that stand in one line along the flow.
#[derive(Debug, Clone, Default)]
struct Track {
    spans: Vec<(f32, f32)>,
    thick: f32,
}

/// The lane of each node of `scene`: the one that writes it, else that of
/// the first node a line joins it to that has one, else the first.
fn lanes_of(
    tree: &Tree,
    scene: &Scene,
    lanes: &Lanes,
    ids: &HashMap<usize, &String>,
) -> Vec<usize> {
    let own = |node: usize| ids.get(&node).and_then(|id| lanes.member.get(*id)).copied();
    let index: HashMap<&String, usize> = ids.iter().map(|(&node, &id)| (id, node)).collect();
    (0..scene.nodes.len())
        .map(|node| {
            own(node)
                .or_else(|| {
                    let id = ids.get(&node)?;
                    tree.graph.edges.iter().find_map(|edge| {
                        let other = if &&edge.from == id {
                            &edge.to
                        } else if &&edge.to == id {
                            &edge.from
                        } else {
                            return None;
                        };
                        index.get(other).and_then(|&n| own(n))
                    })
                })
                .unwrap_or(0)
        })
        .collect()
}

/// Routes every line of `scene` again around the boxes and the lanes'
/// names, and puts its words on it again, off the line as `curve` draws
/// it and inside one band. A loop moves with its node, by `moved`.
fn reroute(
    tree: &Tree,
    scene: &mut Scene,
    ids: &HashMap<usize, &String>,
    bands: &[Band],
    moved: &[(f32, f32)],
    curve: f32,
) {
    let index: HashMap<&String, usize> = ids.iter().map(|(&node, &id)| (id, node)).collect();
    let boxes: Vec<Rect> = scene.nodes.iter().map(|n| n.rect).collect();
    let names: Vec<Rect> = bands.iter().map(|b| b.title).collect();
    let mut around = route::Surroundings {
        obstacles: boxes.iter().chain(&names).copied().collect(),
        ..route::Surroundings::default()
    };
    let mut lines: Vec<Vec<(f32, f32)>> = Vec::with_capacity(scene.edges.len());
    for wire in &mut scene.edges {
        let Some((from, to)) = ends(tree, &index, wire) else {
            lines.push(wire.points.clone());
            continue;
        };
        if from == to {
            let (dx, dy) = moved[from];
            for point in &mut wire.points {
                *point = (point.0 + dx, point.1 + dy);
            }
            wire.label_at = (wire.label_at.0 + dx, wire.label_at.1 + dy);
            around
                .lines
                .extend(wire.points.windows(2).map(|pair| (pair[0], pair[1])));
            lines.push(wire.points.clone());
            continue;
        }
        let end = |node: usize| route::End {
            rect: boxes[node],
            flat: scene_flat(tree, ids, node),
        };
        let points = route::route(end(from), end(to), &around)
            .unwrap_or_else(|| straight(boxes[from], boxes[to]));
        around
            .lines
            .extend(points.windows(2).map(|pair| (pair[0], pair[1])));
        around.used.extend([points[0], points[points.len() - 1]]);
        wire.points = points.clone();
        wire.label_at = super::nest::label_on(&wire.points, &wire.label);
        lines.push(points);
    }
    // Words that meet a box, a name or other words move along their line.
    let mut labels: Vec<((f32, f32), (f32, f32))> = scene
        .edges
        .iter()
        .map(|wire| (wire.label_at, (wire.label.width, wire.label.height)))
        .collect();
    let solids: Vec<Rect> = boxes.iter().chain(&names).copied().collect();
    let drawn: Vec<Vec<(f32, f32)>> = lines
        .iter()
        .map(|line| omaspec_uml::curve::flatten(line, curve))
        .collect();
    let frames: Vec<Rect> = bands.iter().map(|band| band.rect).collect();
    let _ = route::place_labels(
        &drawn,
        &mut labels,
        &solids,
        &frames,
        (scene.width, scene.height),
    );
    for (wire, (at, _)) in scene.edges.iter_mut().zip(labels) {
        wire.label_at = at;
    }
}

/// Whether a line may meet the node anywhere along a side.
fn scene_flat(tree: &Tree, ids: &HashMap<usize, &String>, node: usize) -> bool {
    ids.get(&node)
        .and_then(|id| tree.kinds.get(*id))
        .is_some_and(|kind| kind.flat())
}

/// A line straight from one box to the other, from border to border,
/// where no route is found: its head stands on the box it points to.
fn straight(from: Rect, to: Rect) -> Vec<(f32, f32)> {
    let middle = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let (a, b) = (middle(from), middle(to));
    // Where the line from the middle of `r` toward `toward` leaves it.
    let border = |r: Rect, (cx, cy): (f32, f32), (tx, ty): (f32, f32)| {
        let (dx, dy) = (tx - cx, ty - cy);
        let scale = [(dx, r.w), (dy, r.h)]
            .iter()
            .filter(|(d, _)| d.abs() > f32::EPSILON)
            .map(|(d, size)| size / 2.0 / d.abs())
            .fold(f32::INFINITY, f32::min);
        if scale.is_finite() {
            (cx + dx * scale.min(1.0), cy + dy * scale.min(1.0))
        } else {
            (cx, cy)
        }
    };
    vec![border(from, a, b), border(to, b, a)]
}

/// Paints the bands under what the scene draws: each a frame, its name in
/// its place, the lanes apart by their borders.
pub(super) fn paint(canvas: &mut Canvas, palette: &Palette, bands: &Bands) {
    let line = Stroke::new(palette.cluster_border, 1.0);
    for (lane, band) in bands.bands.iter().enumerate() {
        let r = band.rect;
        let fill = if lane % 2 == 0 {
            palette.cluster_fill
        } else {
            Some(palette.background)
        };
        canvas.rect(
            Layer::Background,
            (r.x, r.y, r.w, r.h),
            0.0,
            fill,
            Some(line),
        );
        let t = band.title;
        canvas.rect(
            Layer::Background,
            (t.x, t.y, t.w, t.h),
            0.0,
            palette.cluster_fill,
            Some(line),
        );
        let at = (
            t.x + (t.w - band.words.width) / 2.0,
            t.y + (t.h - band.words.height) / 2.0,
        );
        let before = canvas.tie(Some(band.line));
        canvas.text(Layer::Front, &band.words, at, Align::Left);
        canvas.tie(before);
    }
}

#[cfg(test)]
mod tests {
    use mermaid_rs_renderer::{RenderOptions, parse_mermaid_strict};

    use super::super::{Family, Spot, layout_spot, palette, prepare};
    use super::*;

    /// The swimlane diagram `source` laid out and placed in its bands, as
    /// [`super::super::try_draw_lanes`] does before painting.
    fn laid(source: &str) -> (Scene, Bands, Vec<String>) {
        let (text, lanes) = super::super::super::swimlane::read(source).unwrap();
        let parsed = parse_mermaid_strict(&text).unwrap();
        let mut repaired = None;
        let (graph, fix) =
            super::super::flowchart::apply(&parsed.graph, &text, &mut repaired).unwrap();
        let theme = RenderOptions::default().theme;
        let written = super::super::written::read(graph, &text, &fix.invisible);
        let tree = prepare(graph, &text, &fix, written).unwrap();
        let palette = palette(&theme, &text).unwrap();
        let family = Family::of(&theme.font_family);
        let mut scene = layout_spot(&tree, Spot::Root, &palette, family).unwrap();
        let curve = super::super::super::flowchart_asked(None).curve;
        let bands = place(&tree, &mut scene, &lanes, &palette, family, curve).unwrap();
        let mut ids: Vec<(usize, String)> = tree
            .paths
            .iter()
            .filter(|(_, path)| path.len() == 1)
            .map(|(id, path)| (path[0], id.clone()))
            .collect();
        ids.sort();
        (scene, bands, ids.into_iter().map(|(_, id)| id).collect())
    }

    const DOCS: &str = "swimlane-beta LR\n  subgraph Customer\n    request[Request service]\n    receive[Receive update]\n  end\n\n  subgraph Support\n    triage[Triage request]\n    answer[Send answer]\n  end\n\n  subgraph Engineering\n    investigate[Investigate issue]\n    fix[Prepare fix]\n  end\n\n  request --> triage\n  triage -->|Known issue| answer\n  triage -->|Needs code change| investigate\n  investigate --> fix --> answer\n  answer --> receive\n";

    /// Mermaid #8853: three dotted lines into one lane's node.
    const ISSUE: &str = "swimlane-beta LR\n  subgraph Modeling\n   A(A)\n   B(B)\n   C(C)\n   D(D)\n  end\n  subgraph Compliance\n   E(E)\n   F(F)\n   G(G)\n   H(H)\n  end\n  subgraph Problems\n   I(I)\n  end\n  A-->B\n  A-->C\n  B-->D\n  C-->D\n  A-->F\n  B-->E\n  D-->G\n  F-->H\n  E-->H\n  G-->H\n  C & F & G -.- I\n";

    #[test]
    fn a_loop_moves_with_its_node_and_stays_in_its_band() {
        let source = DOCS.replace(
            "  answer --> receive\n",
            "  answer --> receive\n  fix -->|again| fix\n",
        );
        let (scene, bands, ids) = laid(&source);
        let fix = ids.iter().position(|id| id == "fix").unwrap();
        let rect = scene.nodes[fix].rect;
        let looped: Vec<_> = scene
            .edges
            .iter()
            .filter(|wire| {
                wire.label.width > 0.0 && super::super::tests::block_has(&wire.label, "again")
            })
            .collect();
        assert_eq!(looped.len(), 1);
        let wire = looped[0];
        // Around its node, not a dot at its middle.
        let near = |&(x, y): &(f32, f32)| {
            x >= rect.x - 80.0
                && x <= rect.x + rect.w + 80.0
                && y >= rect.y - 80.0
                && y <= rect.y + rect.h + 80.0
        };
        assert!(
            wire.points.len() > 2 && wire.points.iter().all(near),
            "{:?} {rect:?}",
            wire.points
        );
        let band = bands.bands[2].rect;
        for &(x, y) in &wire.points {
            assert!(
                x >= band.x && x <= band.x + band.w && y >= band.y && y <= band.y + band.h,
                "{x},{y} out of {band:?}"
            );
        }
    }

    #[test]
    fn a_line_without_a_route_runs_from_border_to_border() {
        let a = Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 40.0,
        };
        let b = Rect {
            x: 300.0,
            y: 0.0,
            w: 100.0,
            h: 40.0,
        };
        assert_eq!(straight(a, b), vec![(100.0, 20.0), (300.0, 20.0)]);
        let below = Rect {
            x: 0.0,
            y: 200.0,
            w: 100.0,
            h: 40.0,
        };
        assert_eq!(straight(a, below), vec![(50.0, 40.0), (50.0, 200.0)]);
    }

    fn inside(outer: Rect, inner: Rect) -> bool {
        inner.x >= outer.x - 0.01
            && inner.y >= outer.y - 0.01
            && inner.x + inner.w <= outer.x + outer.w + 0.01
            && inner.y + inner.h <= outer.y + outer.h + 0.01
    }

    /// The acceptance of REQ-089: bands of one size with their names, in
    /// the direction asked for, each node in its lane's band, and no line
    /// through a lane's name, in the example of Mermaid's documentation and
    /// in that of #8853, across and down.
    #[test]
    fn the_lanes_are_bands_of_one_size_and_no_line_runs_through_a_name() {
        for (source, lanes) in [
            (DOCS, vec!["Customer", "Support", "Engineering"]),
            (ISSUE, vec!["Modeling", "Compliance", "Problems"]),
            (
                &*DOCS.replace("swimlane-beta LR", "swimlane-beta"),
                vec!["Customer", "Support", "Engineering"],
            ),
        ] {
            let rows = source.starts_with("swimlane-beta LR");
            let (scene, bands, ids) = laid(source);
            let (_, read) = super::super::super::swimlane::read(source).unwrap();
            assert_eq!(bands.bands.len(), lanes.len());
            let size = |r: Rect| if rows { r.h } else { r.w };
            for (band, title) in bands.bands.iter().zip(&lanes) {
                assert_eq!(size(band.rect), size(bands.bands[0].rect), "{title}");
                // The name at the start of its band, along the flow.
                assert!(inside(band.rect, band.title));
                if rows {
                    assert_eq!(band.title.x, band.rect.x);
                } else {
                    assert_eq!(band.title.y, band.rect.y);
                }
            }
            // One band after the other across the flow.
            for pair in bands.bands.windows(2) {
                let (a, b) = (pair[0].rect, pair[1].rect);
                if rows {
                    assert!((a.y + a.h - b.y).abs() < 0.01);
                } else {
                    assert!((a.x + a.w - b.x).abs() < 0.01);
                }
            }
            for (node, id) in ids.iter().enumerate() {
                let lane = read.member[id];
                let rect = scene.nodes[node].rect;
                assert!(inside(bands.bands[lane].rect, rect), "{id} in its lane");
                assert!(
                    !bands.bands.iter().any(|b| b.title.x < rect.x + rect.w
                        && rect.x < b.title.x + b.title.w
                        && b.title.y < rect.y + rect.h
                        && rect.y < b.title.y + b.title.h),
                    "{id} on a name"
                );
            }
            for wire in &scene.edges {
                for pair in wire.points.windows(2) {
                    for band in &bands.bands {
                        assert!(
                            !route::cuts(band.title, pair[0], pair[1]),
                            "a line through {:?}",
                            band.words.lines
                        );
                    }
                }
            }
        }
    }
}
