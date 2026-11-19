//! Fork and join bars as long as what they split and join (F4 of the
//! review of the three diagrams). A bar is measured before the layout
//! knows where its states go, at [`BAR_LONG`]; states wider than that
//! stand further apart, so their lines ran to the bar's ends in a jog,
//! and the lines into a join met at one point of it. Once a diagram is
//! laid out, each bar's states say how far from its middle theirs are;
//! the bar is then long enough to reach past them, and the diagram laid
//! out again, as long as a bar grows ([`LAYOUTS`] at most): each line
//! runs straight into it, at a place of its own, as an activity
//! diagram's bar spans its branches. The bars are kept out of bundles
//! for that, as circles and diamonds are.

use std::cell::RefCell;
use std::collections::HashMap;

use super::super::{Container, Direction, Kind};
use super::{BAR_LONG, Scene, Skin, layout_container};
use crate::Declined;
use crate::graph::route::Point;
use crate::text::Family;

/// Past the middle of a bar's outer states, to its ends.
const PAST: f32 = 16.0;

thread_local! {
    /// How long each bar is, by its state's key, while a diagram is laid
    /// out again ([`laid_out`]).
    static LONG: RefCell<HashMap<String, f32>> = RefCell::new(HashMap::new());
}

/// How long the bar `key` is drawn.
pub(super) fn long(key: &str) -> f32 {
    LONG.with_borrow(|long| long.get(key).copied().unwrap_or(BAR_LONG))
}

/// Takes the lengths back when dropped, also when a layout panics.
struct Clear;

impl Drop for Clear {
    fn drop(&mut self) {
        LONG.with_borrow_mut(HashMap::clear);
    }
}

/// How often a diagram is laid out at most: once, then again while a bar
/// grows, as the layout places a longer bar anew.
const LAYOUTS: usize = 3;

/// `root` laid out with each bar long enough to reach, from its middle,
/// the middles of the states it splits and joins.
pub(super) fn laid_out(root: &Container, skin: &Skin, family: Family) -> Result<Scene, Declined> {
    let mut scene = layout_container(root, skin, family)?;
    let _clear = Clear;
    for _ in 1..LAYOUTS {
        let mut needed: HashMap<String, f32> = HashMap::new();
        gather(root, &scene, &mut needed);
        let grows = LONG.with_borrow_mut(|long| {
            let mut grows = false;
            for (key, length) in needed {
                let now = long.get(&key).copied().unwrap_or(BAR_LONG);
                if length > now + 0.5 {
                    long.insert(key, length);
                    grows = true;
                }
            }
            grows
        });
        if !grows {
            break;
        }
        scene = layout_container(root, skin, family)?;
    }
    let mut ends = Vec::new();
    ends_in(&scene, &mut ends);
    trim(&mut scene, &ends);
    Ok(scene)
}

/// The first and last point of every line of `scene`, and of those inside.
fn ends_in(scene: &Scene, ends: &mut Vec<Point>) {
    for edge in &scene.edges {
        ends.extend(edge.points.first());
        ends.extend(edge.points.last());
    }
    for node in &scene.nodes {
        ends_in(&node.children, ends);
    }
}

/// Each bar of `scene` cut back to [`PAST`] beyond the outermost of `ends`
/// on it, as long as [`BAR_LONG`] at least: laid out anew, a state may
/// come to stand nearer the bar's middle than before.
fn trim(scene: &mut Scene, ends: &[Point]) {
    for node in &mut scene.nodes {
        trim(&mut node.children, ends);
        if !matches!(node.item.kind, Kind::Fork | Kind::Join) {
            continue;
        }
        let rect = node.rect;
        let lying = rect.w >= rect.h;
        // Along the bar, and across it, the two sides lines meet.
        let (start, long, at, thick) = if lying {
            (rect.x, rect.w, rect.y, rect.h)
        } else {
            (rect.y, rect.h, rect.x, rect.w)
        };
        let on: Vec<f32> = ends
            .iter()
            .map(|&(x, y)| if lying { (x, y) } else { (y, x) })
            .filter(|&(along, across)| {
                ((across - at).abs() < 0.75 || (across - at - thick).abs() < 0.75)
                    && along > start - 0.5
                    && along < start + long + 0.5
            })
            .map(|(along, _)| along)
            .collect();
        let Some(low) = on.iter().copied().reduce(f32::min) else {
            continue;
        };
        let high = on.iter().copied().fold(low, f32::max);
        let (mut from, mut to) = (low - PAST, high + PAST);
        let short = BAR_LONG - (to - from);
        if short > 0.0 {
            (from, to) = (from - short / 2.0, to + short / 2.0);
        }
        // Never past the bar as laid out, which kept its room.
        let (from, to) = (from.max(start), to.min(start + long));
        if to - from >= long - 0.5 {
            continue;
        }
        if lying {
            (node.rect.x, node.rect.w) = (from, to - from);
        } else {
            (node.rect.y, node.rect.h) = (from, to - from);
        }
    }
}

/// How long each bar of `container`, laid out as `scene`, has to be to
/// reach from its middle past the middles of the states it has lines to,
/// and those of the bars inside.
fn gather(container: &Container, scene: &Scene, needed: &mut HashMap<String, f32>) {
    let bar = |index: usize| {
        container
            .nodes
            .get(index)
            .is_some_and(|node| matches!(node.kind, Kind::Fork | Kind::Join))
    };
    let across = |index: usize| {
        scene.nodes.get(index).map(|node| {
            let rect = node.rect;
            match container.direction {
                Direction::Down => rect.x + rect.w / 2.0,
                Direction::Right => rect.y + rect.h / 2.0,
            }
        })
    };
    for edge in &container.edges {
        for (at, other) in [(edge.from, edge.to), (edge.to, edge.from)] {
            if at == other || !bar(at) || bar(other) {
                continue;
            }
            let (Some(middle), Some(state)) = (across(at), across(other)) else {
                continue;
            };
            let length = 2.0 * ((state - middle).abs() + PAST);
            let entry = needed
                .entry(container.nodes[at].key.clone())
                .or_insert(BAR_LONG);
            *entry = entry.max(length);
        }
    }
    for (node, placed) in container.nodes.iter().zip(&scene.nodes) {
        if !node.children.nodes.is_empty() {
            gather(&node.children, &placed.children, needed);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::state::{draw_mapped, parse};

    /// The bars of `source` as drawn, each as `(x, width)`.
    fn bars(source: &str) -> Vec<(f32, f32)> {
        let (_, lines) = crate::source::body(source).unwrap();
        let (svg, _) = draw_mapped(&parse(&lines).unwrap()).unwrap();
        svg.split("<rect ")
            .skip(1)
            .filter(|rect| rect.contains(" height=\"8\""))
            .map(|rect| {
                let number = |name: &str| -> f32 {
                    rect.split(&format!("{name}=\""))
                        .nth(1)
                        .unwrap()
                        .split('"')
                        .next()
                        .unwrap()
                        .parse()
                        .unwrap()
                };
                (number("x"), number("width"))
            })
            .collect()
    }

    #[test]
    fn a_bar_spans_the_states_it_splits_and_joins() {
        let source = "@startuml\nstate f <<fork>>\nstate j <<join>>\n[*] --> f\nf --> Saving\nf --> Uploading\nSaving --> j\nUploading --> j\nj --> [*]\n@enduml\n";
        let drawn = bars(source);
        assert_eq!(drawn.len(), 2, "{drawn:?}");
        for (_, width) in &drawn {
            assert!(*width > super::BAR_LONG, "{drawn:?}");
        }
        // A bar with one state to each side keeps its length.
        let one = "@startuml\nstate f <<fork>>\n[*] --> f\nf --> A\n@enduml\n";
        assert!(
            bars(one)
                .iter()
                .all(|(_, width)| (*width - super::BAR_LONG).abs() < 0.01)
        );
    }

    #[test]
    fn each_line_meets_a_join_at_a_place_of_its_own() {
        let source = "@startuml\nstate j <<join>>\n[*] --> Saving\n[*] --> Uploading\nSaving --> j\nUploading --> j\nj --> [*]\n@enduml\n";
        let (_, lines) = crate::source::body(source).unwrap();
        let (svg, _) = draw_mapped(&parse(&lines).unwrap()).unwrap();
        // The tips of the two heads into the join stand apart.
        let tips: Vec<&str> = svg
            .split("<polygon points=\"")
            .skip(1)
            .filter_map(|rest| rest.split(' ').nth(1))
            .collect();
        let mut distinct = tips.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), tips.len(), "{tips:?}");
    }
}
