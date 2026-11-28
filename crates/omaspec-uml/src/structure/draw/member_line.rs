//! A line whose end is at a member (`Foo::field1 --> Bar::field3`), as
//! PlantUML draws one: it meets its box at the side, at that member's
//! row. Laid out as a line between the boxes, it is routed anew once they
//! stand, from the side that faces the other end (of two boxes one over
//! the other, left for the first member, right for the second, and so
//! on), with a short stem straight out of the side.

use super::{Placed, Scene, drawn_in, member_end, placements};
use crate::graph::Rect;
use crate::graph::across::{self, Drawn};
use crate::structure::{Group, Link, Side};

/// How far a line runs straight out of the side before it turns.
const STEM: f32 = 10.0;

/// A line with an end at a member: the group it is in, its place among
/// that group's lines, the paths to its ends and the members.
struct Found<'a> {
    host: Vec<usize>,
    slot: usize,
    from: Vec<usize>,
    to: Vec<usize>,
    arrow: &'a Link,
}

/// Routes the lines of `root` with an end at a member, laid out in
/// `scene`, to those members' rows.
pub(super) fn link(root: &Group, scene: &mut Scene) {
    let mut found = Vec::new();
    gather(root, &mut Vec::new(), &mut found);
    if found.is_empty() {
        return;
    }
    // Their lines are routed anew: the others keep clear of them then,
    // not of where they lay.
    let mut before = Vec::with_capacity(found.len());
    for line in &found {
        let points = scene
            .scene_at(&line.host)
            .and_then(|host| host.edges.get_mut(line.slot))
            .map(|edge| std::mem::take(&mut edge.points))
            .unwrap_or_default();
        before.push(points);
    }
    let top = placements(&scene.nodes);
    let mut drawn = Drawn::default();
    drawn_in(scene, &mut drawn);
    let mut links = Vec::with_capacity(found.len());
    let mut borders = Vec::with_capacity(found.len());
    for line in &found {
        let (Some(from), Some(to)) = (placed(scene, &line.from), placed(scene, &line.to)) else {
            links.push(None);
            borders.push([None, None]);
            continue;
        };
        let label = group_at(scene, &line.host)
            .and_then(|host| host.edges.get(line.slot))
            .map_or((0.0, 0.0), |edge| (edge.label.width, edge.label.height));
        let [a, b] = line.arrow.members;
        let from_port = a.and_then(|member| port(from, member, to));
        let to_port = b.and_then(|member| port(to, member, from));
        links.push(Some(across::Link {
            from: line.from.clone(),
            to: line.to.clone(),
            label,
            shape: None,
            stand: [from_port.map(|p| p.0), to_port.map(|p| p.0)],
            sealed: [false; 2],
        }));
        borders.push([from_port.map(|p| p.1), to_port.map(|p| p.1)]);
    }
    let routable: Vec<across::Link> = links.iter().flatten().cloned().collect();
    let mut routed = across::link(&top, &routable, &drawn).into_iter();
    for ((line, link), (points, [from, to])) in found
        .iter()
        .zip(&links)
        .zip(before.into_iter().zip(borders))
    {
        let laid = match link {
            Some(_) => routed.next().flatten(),
            None => None,
        };
        let Some(edge) = scene
            .scene_at(&line.host)
            .and_then(|host| host.edges.get_mut(line.slot))
        else {
            continue;
        };
        match laid {
            Some(laid) => {
                edge.points = from.into_iter().chain(laid.points).chain(to).collect();
                edge.label_at = laid.label_at;
            }
            // No way found: the line runs where it lay.
            None => edge.points = points,
        }
    }
}

/// Where a line meets `node` at its `member`, toward `other`: the point
/// it stands on a stem's length out of the side, as a little box the
/// router ends at, and the point on the side.
fn port(node: &Placed, member: usize, other: &Placed) -> Option<(Rect, (f32, f32))> {
    let (_, y) = member_end(node, member, Side::Left)?;
    let rect = node.rect;
    let (own, theirs) = (rect.x + rect.w / 2.0, other.rect.x + other.rect.w / 2.0);
    let right = if (theirs - own).abs() > rect.w / 2.0 {
        theirs > own
    } else {
        member % 2 == 1
    };
    let (side, out) = if right {
        (rect.x + rect.w, STEM)
    } else {
        (rect.x, -STEM)
    };
    let stand = Rect {
        x: side + out - 1.0,
        y: y - 1.0,
        w: 2.0,
        h: 2.0,
    };
    Some((stand, (side, y)))
}

/// The group at `path` in `scene`.
fn group_at<'a>(scene: &'a Scene, path: &[usize]) -> Option<&'a Scene> {
    let mut here = scene;
    for &index in path {
        here = &here.nodes.get(index)?.children;
    }
    Some(here)
}

/// The box at `path` in `scene`.
fn placed<'a>(scene: &'a Scene, path: &[usize]) -> Option<&'a Placed> {
    let (&first, rest) = path.split_first()?;
    let mut node = scene.nodes.get(first)?;
    for &index in rest {
        node = node.children.nodes.get(index)?;
    }
    Some(node)
}

/// Every line of `group` and of the groups inside with an end at a
/// member, but a loop.
fn gather<'a>(group: &'a Group, path: &mut Vec<usize>, out: &mut Vec<Found<'a>>) {
    let at = |path: &[usize], end: &[usize]| [path, end].concat();
    for (slot, edge) in group.edges.iter().enumerate() {
        if edge.members.iter().any(Option::is_some) && edge.from != edge.to {
            out.push(Found {
                host: path.clone(),
                slot,
                from: at(path, &[edge.from]),
                to: at(path, &[edge.to]),
                arrow: edge,
            });
        }
    }
    for (index, crossing) in group.crossings.iter().enumerate() {
        if crossing.arrow.members.iter().any(Option::is_some) {
            out.push(Found {
                host: path.clone(),
                slot: group.edges.len() + index,
                from: at(path, &crossing.from),
                to: at(path, &crossing.to),
                arrow: &crossing.arrow,
            });
        }
    }
    for (index, node) in group.nodes.iter().enumerate() {
        path.push(index);
        gather(&node.children, path, out);
        path.pop();
    }
}

#[cfg(test)]
mod tests {
    fn diagram(body: &str) -> crate::structure::Diagram {
        let source = format!("@startuml\n{body}\n@enduml\n");
        let (_, lines) = crate::source::body(&source).unwrap();
        crate::structure::parse(&lines).unwrap_or_else(|err| panic!("{err:?}"))
    }

    const CLASSES: &str = "class Foo {\n  + field1\n  + field2\n}\nclass Bar {\n  + field3\n}\n";

    /// A line's ends at members are those members, by their places, the
    /// line written either way; one `hide` took, or that names none,
    /// meets its box as any line does.
    #[test]
    fn the_ends_are_at_their_members() {
        let d = diagram(&format!("{CLASSES}Foo::field2 --> Bar::field3"));
        assert_eq!(d.root.edges[0].members, [Some(1), Some(0)]);
        let d = diagram(&format!("{CLASSES}Bar::field3 <-- Foo::field2"));
        let edge = &d.root.edges[0];
        let at = |i: usize| d.root.nodes[i].key.as_str();
        let members: Vec<(&str, Option<usize>)> = vec![
            (at(edge.from), edge.members[0]),
            (at(edge.to), edge.members[1]),
        ];
        assert!(
            members.contains(&("Foo", Some(1))) && members.contains(&("Bar", Some(0))),
            "{members:?}"
        );
        let d = diagram(&format!("{CLASSES}Foo::field1 --> Bar\nhide Foo fields"));
        assert_eq!(d.root.edges[0].members, [None, None]);
        let d = diagram(&format!("{CLASSES}Foo::nothing --> Bar"));
        assert_eq!(d.root.edges[0].members, [None, None]);
        assert_eq!(d.root.nodes.len(), 2);
    }

    /// The line meets its box at the side, at the member's row.
    #[test]
    fn the_line_meets_the_row_at_the_side() {
        let svg = crate::render(&format!(
            "@startuml\nleft to right direction\n{CLASSES}Foo::field2 --> Bar::field3\n@enduml\n"
        ))
        .unwrap();
        let number = |text: &str, attr: &str| -> f32 {
            let start = text.find(&format!(" {attr}=\"")).unwrap() + attr.len() + 3;
            text[start..].split('"').next().unwrap().parse().unwrap()
        };
        let foo = &svg[svg.find("id=\"class-Foo\"").unwrap()..];
        let (x, y, w, h) = (
            number(foo, "x"),
            number(foo, "y"),
            number(foo, "width"),
            number(foo, "height"),
        );
        let line = &svg[svg.find("id=\"link-Foo-Bar\"").unwrap()..];
        let d = &line[line.find(" d=\"M").unwrap() + 5..];
        let start: Vec<f32> = d
            .split([' ', 'L', 'C'])
            .next()
            .unwrap()
            .split(',')
            .map(|n| n.parse().unwrap())
            .collect();
        assert!((start[0] - (x + w)).abs() < 1.0, "{svg}");
        // The second member's row, below the middle of the box.
        assert!(start[1] > y + h / 2.0 && start[1] < y + h, "{svg}");
    }
}
