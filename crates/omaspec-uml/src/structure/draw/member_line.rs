//! A line whose end is at a member (`Foo::field1 --> Bar::field3`), as
//! PlantUML draws one: it meets its box at the side, at that member's
//! row. Laid out as a line between the boxes, it is routed anew once they
//! stand, from the side that faces the other end, with a stem straight
//! out of the side that holds its head and its multiplicity; of two
//! boxes one over the other at the right, both ends. The box an end is
//! at is closed to its line, and so is its stem, so the line leaves it
//! outward.

use super::{Placed, Scene, drawn_in, member_end, placements};
use crate::graph::Rect;
use crate::graph::across::{self, Drawn};
use crate::structure::{Group, Link, Member, Side};

/// How far a line runs straight out of the side before it turns: past a
/// head and a multiplicity.
const STEM: f32 = 24.0;

/// A line with an end at a member: the group it is in, its place among
/// that group's lines, the paths to its ends and the members.
struct Found {
    host: Vec<usize>,
    slot: usize,
    from: Vec<usize>,
    to: Vec<usize>,
    /// The members, each one its box has: none on a box a fold closed.
    members: [Option<usize>; 2],
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
        let [a, b] = line.members;
        let from_port = a.and_then(|member| port(from, member, to));
        let to_port = b.and_then(|member| port(to, member, from));
        for port in [from_port, to_port].into_iter().flatten() {
            drawn.notes.push(port.2);
        }
        links.push(Some(across::Link {
            from: line.from.clone(),
            to: line.to.clone(),
            label,
            shape: None,
            stand: [from_port.map(|p| p.0), to_port.map(|p| p.0)],
            sealed: [from_port.is_some(), to_port.is_some()],
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
                // The stems run level with their rows: the route meets the
                // little box it ends at a pixel off.
                let mut points = laid.points;
                if let (Some(border), Some(first)) = (from, points.first_mut()) {
                    first.1 = border.1;
                }
                if let (Some(border), Some(last)) = (to, points.last_mut()) {
                    last.1 = border.1;
                }
                edge.points = from.into_iter().chain(points).chain(to).collect();
                edge.label_at = laid.label_at;
            }
            // No way found: the line runs where it lay.
            None => edge.points = points,
        }
    }
}

/// Where a line meets `node` at its `member`, toward `other`: the point
/// it stands on a stem's length out of the side, as a little box the
/// router ends at, the point on the side, and the stem, which no line
/// runs through.
fn port(node: &Placed, member: usize, other: &Placed) -> Option<(Rect, (f32, f32), Rect)> {
    let (_, y) = member_end(node, member, Side::Left)?;
    let (rect, there) = (node.rect, other.rect);
    let (own, theirs) = (rect.x + rect.w / 2.0, there.x + there.w / 2.0);
    // The side that faces the other box, where one stands beside it; of
    // two one over the other the right, as PlantUML draws them.
    let right = (theirs - own).abs() <= rect.w.min(there.w) / 2.0 || theirs > own;
    let side = if right { rect.x + rect.w } else { rect.x };
    // Where the other box stands close ahead, half the way to it: its
    // own stem may come the other half.
    let room = if right {
        there.x - side
    } else {
        side - (there.x + there.w)
    };
    let stem = if room > 0.0 && room < 2.0 * STEM {
        room / 2.0
    } else {
        STEM
    };
    let out = if right { stem } else { -stem };
    let stand = Rect {
        x: side + out - 1.0,
        y: y - 1.0,
        w: 2.0,
        h: 2.0,
    };
    let held = Rect {
        x: side.min(side + out - out.signum()),
        y: y - 1.0,
        w: (stem - 1.0).max(0.0),
        h: 2.0,
    };
    Some((stand, (side, y), held))
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

/// Which ends of `link`, from `from` to `to` in `group`, meet their box
/// at a member's row: where the line is no loop and the box has that
/// member.
pub(super) fn leads(group: &Group, from: &[usize], to: &[usize], link: &Link) -> [bool; 2] {
    let members = kept(group, from, to, link);
    [members[0].is_some(), members[1].is_some()]
}

/// The members `link`'s ends are at, each one its box has (none on a box
/// a fold closed), and none for a loop.
fn kept(group: &Group, from: &[usize], to: &[usize], link: &Link) -> [Option<usize>; 2] {
    let has = |end: &[usize], member: Option<usize>| {
        let member = member?;
        let mut here = group;
        let mut node = None;
        for &index in end {
            let found = here.nodes.get(index)?;
            here = &found.children;
            node = Some(found);
        }
        matches!(node?.members.get(member), Some(Member::Text { .. })).then_some(member)
    };
    if from == to {
        return [None, None];
    }
    [has(from, link.members[0]), has(to, link.members[1])]
}

/// Every line of `group` and of the groups inside with an end at a
/// member of its box, but a loop.
fn gather(group: &Group, path: &mut Vec<usize>, out: &mut Vec<Found>) {
    let at = |path: &[usize], end: &[usize]| [path, end].concat();
    let mut push = |slot: usize, from: Vec<usize>, to: Vec<usize>, link: &Link| {
        let members = kept(group, &from, &to, link);
        if members.iter().any(Option::is_some) {
            out.push(Found {
                host: path.clone(),
                slot,
                from: at(path, &from),
                to: at(path, &to),
                members,
            });
        }
    };
    for (slot, edge) in group.edges.iter().enumerate() {
        push(slot, vec![edge.from], vec![edge.to], edge);
    }
    for (index, crossing) in group.crossings.iter().enumerate() {
        push(
            group.edges.len() + index,
            crossing.from.clone(),
            crossing.to.clone(),
            &crossing.arrow,
        );
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

    /// The structure `source` reads to, its procedures expanded.
    fn expanded(source: &str) -> crate::structure::Diagram {
        match crate::parse(source).unwrap_or_else(|err| panic!("{err:?}")) {
            crate::Diagram::Structure(d) => d,
            _ => panic!("no structure"),
        }
    }

    /// Each end, as the key of its box and the member there.
    fn ends(d: &crate::structure::Diagram) -> Vec<[(String, Option<usize>); 2]> {
        let key = |path: &[usize]| {
            let mut here = &d.root;
            let mut key = String::new();
            for &index in path {
                key.clone_from(&here.nodes[index].key);
                here = &here.nodes[index].children;
            }
            key
        };
        let mut out = Vec::new();
        for edge in &d.root.edges {
            out.push([
                (key(&[edge.from]), edge.members[0]),
                (key(&[edge.to]), edge.members[1]),
            ]);
        }
        for crossing in &d.root.crossings {
            out.push([
                (key(&crossing.from), crossing.arrow.members[0]),
                (key(&crossing.to), crossing.arrow.members[1]),
            ]);
        }
        for end in &mut out {
            end.sort();
        }
        out.sort();
        out
    }

    /// Two lines written on one line of a procedure keep their own
    /// members; so does a line by a whole name, turned up.
    #[test]
    fn each_line_keeps_its_own_members() {
        let classes = "class Foo {\n  + a\n  + b\n}\nclass Bar {\n  + x\n  + y\n}\n";
        let d = expanded(&format!(
            "@startuml\n{classes}!procedure $links()\nBar::y <-- Foo::a\nFoo::b --> Bar::x\nBar --> Foo\n!endprocedure\n$links()\n@enduml\n"
        ));
        let at = |a: (&str, Option<usize>), b: (&str, Option<usize>)| {
            let mut end = [(a.0.to_owned(), a.1), (b.0.to_owned(), b.1)];
            end.sort();
            end
        };
        let mut want = vec![
            at(("Foo", Some(0)), ("Bar", Some(1))),
            at(("Foo", Some(1)), ("Bar", Some(0))),
            at(("Bar", None), ("Foo", None)),
        ];
        want.sort();
        assert_eq!(ends(&d), want);
        let d = expanded(
            "@startuml\npackage p {\n  class Foo {\n    + x\n    + b\n  }\n}\nclass Bar {\n  + x\n  + b\n}\np.Foo::b -up-> Bar::x\n@enduml\n",
        );
        assert_eq!(ends(&d), vec![at(("Bar", Some(0)), ("Foo", Some(1)))]);
    }

    /// `::` names no member where a box has the whole name, beside an
    /// association's pair, and parts a name at its last `::`.
    #[test]
    fn a_whole_name_with_colons_is_no_member() {
        let keys = |source: &str| {
            let d = expanded(&format!("@startuml\n{source}\n@enduml\n"));
            let mut keys: Vec<String> = d.root.nodes.iter().map(|n| n.key.clone()).collect();
            keys.sort();
            keys
        };
        assert_eq!(
            keys("class std::vector\nA --> std::vector"),
            ["A", "std::vector"]
        );
        assert!(
            keys("class Foo\nclass Bar\nclass C {\n  c\n}\n(Foo, Bar) .. C::c")
                .contains(&"C::c".to_owned())
        );
        assert_eq!(
            keys("class Foo\nclass Bar\nFoo::a::b --> Bar"),
            ["Bar", "Foo", "Foo::a"]
        );
    }

    /// The points of the line from `from` to `to` in `svg`.
    fn path(svg: &str, id: &str) -> Vec<(f32, f32)> {
        let line = &svg[svg.find(&format!("id=\"{id}\"")).unwrap()..];
        let d = &line[line.find(" d=\"M").unwrap() + 5..];
        let d = &d[..d.find('"').unwrap()];
        d.split(['L', 'C', ' '])
            .filter(|p| !p.is_empty())
            .map(|p| {
                let (x, y) = p.split_once(',').unwrap();
                (x.parse().unwrap(), y.parse().unwrap())
            })
            .collect()
    }

    /// Of two boxes one over the other, both ends meet their rows at the
    /// right, level: a head there stands straight.
    #[test]
    fn stacked_boxes_are_met_at_the_right_and_level() {
        let svg = crate::render(&format!(
            "@startuml\n{CLASSES}Foo::field1 <|-- Bar::field3\n@enduml\n"
        ))
        .unwrap();
        let points = path(&svg, "link-Foo-Bar");
        let (first, last) = (points[0], points[points.len() - 1]);
        let (second, before) = (points[2], points[points.len() - 2]);
        assert!(
            (first.1 - second.1).abs() < 0.01 && (last.1 - before.1).abs() < 0.01,
            "{points:?}"
        );
        // Right of the boxes' middle, which stand one over the other.
        let foo = &svg[svg.find("id=\"class-Foo\"").unwrap()..];
        let number = |attr: &str| -> f32 {
            let start = foo.find(&format!(" {attr}=\"")).unwrap() + attr.len() + 3;
            foo[start..].split('"').next().unwrap().parse().unwrap()
        };
        let middle = number("x") + number("width") / 2.0;
        assert!(first.0 > middle && last.0 > middle, "{points:?}");
    }

    /// A folded package keeps no member: a line into it meets its border
    /// as any line does.
    #[test]
    fn a_folded_box_has_no_rows_to_meet() {
        let mut d = expanded(
            "@startuml\npackage p {\n  class Foo {\n    + a\n  }\n  class Mid\n}\nclass Bar {\n  + y\n}\nFoo::a --> Bar::y\n@enduml\n",
        );
        d.fold(&["p".to_owned()]);
        let root = &d.root;
        let edge = &root.edges[0];
        let p = root.nodes.iter().position(|n| n.key == "p").unwrap();
        assert_eq!(edge.from, p);
        assert_eq!(
            super::kept(root, &[edge.from], &[edge.to], edge),
            [None, Some(0)]
        );
    }

    /// A line shaped by hand keeps meeting its members' rows.
    #[test]
    fn a_line_by_hand_keeps_its_rows() {
        let source = format!(
            "@startuml\n{CLASSES}Foo::field2 --> Bar::field3\n' lines by hand (Omaspec)\n' Foo -> Bar at 1.4 0.5\n@enduml\n"
        );
        let plain = crate::render(&source.replace("' Foo -> Bar at 1.4 0.5\n", "")).unwrap();
        let shaped = crate::render(&source).unwrap();
        let (a, b) = (path(&plain, "link-Foo-Bar"), path(&shaped, "link-Foo-Bar"));
        assert_eq!(a[0], b[0]);
        assert_eq!(a[a.len() - 1], b[b.len() - 1]);
        assert_ne!(a, b);
    }
}
