//! A line whose end is at a member (`Foo::field1 --> Bar::field3`), as
//! PlantUML draws one: it meets its box at the side, at that member's
//! row. Laid out as a line between the boxes, it is routed anew once they
//! stand, from the side that faces the other end, with a stem straight
//! out of the side that holds its head and its multiplicity. Of two boxes
//! one over the other it meets both at the right, as PlantUML draws it; a
//! second line at members of both takes the left, so the two do not
//! cross. The stems on one side fan out, the one whose line passes the
//! others longest; between two boxes side by side each takes half the
//! way. The box an end is at is closed to its line, and so is its stem,
//! so the line leaves it outward.

use std::collections::HashMap;

use super::{Placed, Scene, drawn_in, member_end, placements};
use crate::graph::Rect;
use crate::graph::across::{self, Drawn};
use crate::graph::by_hand::{Arrow, Lead};
use crate::hand::Key;
use crate::structure::{Group, Link, Member, Side};

/// How far a line runs straight out of the side before it turns: past a
/// head and a multiplicity.
const STEM: f32 = 24.0;
/// How much further a stem runs for each stem its line passes.
const FAN: f32 = 10.0;

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

/// Where a line meets its box at a member: the side, at the member's
/// row, and its stem out of it.
#[derive(Debug, Clone, Copy)]
struct Port {
    /// The side's x, and whether it is the right one.
    side: f32,
    right: bool,
    y: f32,
    stem: f32,
    /// How far the line heads up (below zero) or down from the row.
    heading: f32,
}

impl Port {
    /// The point the line stands on at the stem's end, as a little box
    /// the router ends at.
    fn stand(&self) -> Rect {
        Rect {
            x: self.tip() - 1.0,
            y: self.y - 1.0,
            w: 2.0,
            h: 2.0,
        }
    }

    /// The stem's end.
    fn tip(&self) -> f32 {
        if self.right {
            self.side + self.stem
        } else {
            self.side - self.stem
        }
    }

    /// The stem, which no line runs through.
    fn held(&self) -> Rect {
        Rect {
            x: self
                .side
                .min(self.tip() + if self.right { -1.0 } else { 1.0 }),
            y: self.y - 1.0,
            w: (self.stem - 1.0).max(0.0),
            h: 2.0,
        }
    }
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
    let ports = ports(scene, &found);
    let top = placements(&scene.nodes);
    let mut drawn = Drawn::default();
    drawn_in(scene, &mut drawn);
    for port in ports.iter().flatten().flatten() {
        drawn.notes.push(port.held());
    }
    let mut links = Vec::with_capacity(found.len());
    for (line, [from, to]) in found.iter().zip(&ports) {
        // Two stems that meet halfway between boxes side by side: the
        // line runs straight on, or steps across there.
        let met = matches!((from, to), (Some(a), Some(b)) if (a.tip() - b.tip()).abs() < 1.0);
        if met || (placed(scene, &line.from).is_none() || placed(scene, &line.to).is_none()) {
            links.push(None);
            continue;
        }
        let label = group_at(scene, &line.host)
            .and_then(|host| host.edges.get(line.slot))
            .map_or((0.0, 0.0), |edge| (edge.label.width, edge.label.height));
        links.push(Some(across::Link {
            from: line.from.clone(),
            to: line.to.clone(),
            label,
            shape: None,
            stand: [from.map(|p| p.stand()), to.map(|p| p.stand())],
            sealed: [from.is_some(), to.is_some()],
        }));
    }
    let routable: Vec<across::Link> = links.iter().flatten().cloned().collect();
    let mut routed = across::link(&top, &routable, &drawn).into_iter();
    for ((line, link), (points, [from, to])) in
        found.iter().zip(&links).zip(before.into_iter().zip(ports))
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
        let border = |port: Option<Port>| port.map(|p| (p.side, p.y));
        edge.look.stems = [from.is_some(), to.is_some()];
        match (laid, from, to) {
            (Some(laid), ..) => {
                // The stems run level with their rows: the route meets the
                // little box it ends at a pixel off.
                let mut points = laid.points;
                if let (Some(port), Some(first)) = (from, points.first_mut()) {
                    first.1 = port.y;
                }
                if let (Some(port), Some(last)) = (to, points.last_mut()) {
                    last.1 = port.y;
                }
                edge.points = border(from)
                    .into_iter()
                    .chain(points)
                    .chain(border(to))
                    .collect();
                edge.label_at = laid.label_at;
            }
            (None, Some(a), Some(b)) if (a.tip() - b.tip()).abs() < 1.0 => {
                let mut points = vec![(a.side, a.y)];
                if (a.y - b.y).abs() >= 0.5 {
                    points.extend([(a.tip(), a.y), (b.tip(), b.y)]);
                }
                points.push((b.side, b.y));
                edge.points = points;
            }
            // No way found: the line runs where it lay.
            _ => {
                edge.points = points;
                edge.look.stems = [false; 2];
            }
        }
    }
}

/// Where each line of `found` meets its boxes at their members.
fn ports(scene: &Scene, found: &[Found]) -> Vec<[Option<Port>; 2]> {
    let mut out = Vec::with_capacity(found.len());
    // How many lines at members of both of two boxes one over the other
    // took a side, by the two.
    let mut turns: HashMap<[Vec<usize>; 2], usize> = HashMap::new();
    for line in found {
        let (Some(from), Some(to)) = (placed(scene, &line.from), placed(scene, &line.to)) else {
            out.push([None, None]);
            continue;
        };
        let (a, b) = (from.rect, to.rect);
        let stacked = a.x < b.x + b.w && b.x < a.x + a.w;
        let [m, n] = line.members;
        let turn = if stacked && m.is_some() && n.is_some() {
            let mut pair = [line.from.clone(), line.to.clone()];
            pair.sort();
            let taken = turns.entry(pair).or_default();
            *taken += 1;
            *taken % 2 == 1
        } else {
            true
        };
        let middle = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let port = |node: &Placed, member: Option<usize>, other: Rect| {
            let (_, y) = member_end(node, member?, Side::Left)?;
            let right = if stacked {
                turn
            } else {
                middle(other).0 > middle(node.rect).0
            };
            let rect = node.rect;
            let side = if right { rect.x + rect.w } else { rect.x };
            // Beside the other box, half the way to it where it stands
            // close: its own stem may come the other half.
            let room = if right {
                other.x - side
            } else {
                side - (other.x + other.w)
            };
            let beside = rect.y < other.y + other.h && other.y < rect.y + rect.h;
            let stem = if !stacked && beside && room > 0.0 && room < 2.0 * STEM {
                room / 2.0
            } else {
                STEM
            };
            Some(Port {
                side,
                right,
                y,
                stem,
                heading: middle(other).1 - y,
            })
        };
        out.push([port(from, m, b), port(to, n, a)]);
    }
    fan(found, &mut out);
    out
}

/// Fans out the full stems on each side of a box: one whose line heads
/// past the rows of the others on that side runs further, by [`FAN`] for
/// each, so the lines do not run down one another's ends.
fn fan(found: &[Found], ports: &mut [[Option<Port>; 2]]) {
    let all: Vec<(&[usize], Port)> = found
        .iter()
        .zip(ports.iter())
        .flat_map(|(line, ends)| {
            [
                ends[0].map(|p| (line.from.as_slice(), p)),
                ends[1].map(|p| (line.to.as_slice(), p)),
            ]
        })
        .flatten()
        .collect();
    for (line, ends) in found.iter().zip(ports.iter_mut()) {
        let [first, second] = ends;
        for (path, port) in [(line.from.as_slice(), first), (line.to.as_slice(), second)] {
            let Some(port) = port.as_mut().filter(|p| p.stem == STEM) else {
                continue;
            };
            let passed = all
                .iter()
                .filter(|(at, other)| {
                    *at == path
                        && other.right == port.right
                        && other.stem == STEM
                        && (other.y - port.y) * port.heading > 0.5
                })
                .count();
            port.stem = STEM + FAN * passed as f32;
        }
    }
}

/// Gives each of `arrows`, the lines as the block of lines by hand names
/// them, the rows its ends meet their boxes at, as `scene` lays them out
/// ([`Arrow::lead`]): a line shaped by hand meets them too.
pub(super) fn lead(root: &Group, scene: &Scene, arrows: &mut [(Arrow, Key)]) {
    for (arrow, _) in arrows {
        let Some(group) = group_of(root, &arrow.path) else {
            continue;
        };
        let local = |end: &[usize]| end[arrow.path.len()..].to_vec();
        let link = match arrow.slot.checked_sub(group.edges.len()) {
            None => group.edges.get(arrow.slot),
            Some(index) => group.crossings.get(index).map(|c| &c.arrow),
        };
        let Some(link) = link else {
            continue;
        };
        let members = kept(group, &local(&arrow.from), &local(&arrow.to), link);
        let row = |end: &[usize], member: Option<usize>| {
            let (_, y) = member_end(placed(scene, end)?, member?, Side::Left)?;
            Some(Lead { y, stem: STEM })
        };
        arrow.lead = [row(&arrow.from, members[0]), row(&arrow.to, members[1])];
    }
}

/// The group at `path` in `root`.
fn group_of<'a>(root: &'a Group, path: &[usize]) -> Option<&'a Group> {
    let mut here = root;
    for &index in path {
        here = &here.nodes.get(index)?.children;
    }
    Some(here)
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

    /// Between two boxes side by side, close, a line runs straight from
    /// row to row; of two lines between boxes one over the other, one
    /// takes the right and one the left; stems on one side fan out.
    #[test]
    fn lines_keep_clear_of_one_another() {
        let svg = crate::render(&format!(
            "@startuml\nleft to right direction\n{CLASSES}Foo::field1 --> Bar::field3\n@enduml\n"
        ))
        .unwrap();
        let points = path(&svg, "link-Foo-Bar");
        assert!(
            points.windows(2).all(|w| w[1].0 >= w[0].0 - 0.01),
            "{points:?}"
        );
        let classes = "class Foo {\n  + a\n  + b\n}\nclass Bar {\n  + x\n  + y\n}\n";
        let svg = crate::render(&format!(
            "@startuml\n{classes}Foo::a --> Bar::x\nFoo::b --> Bar::y\n@enduml\n"
        ))
        .unwrap();
        let starts: Vec<f32> = ["link-Foo-Bar", "link-Foo-Bar-2"]
            .iter()
            .map(|id| path(&svg, id)[0].0)
            .collect();
        assert!((starts[0] - starts[1]).abs() > 20.0, "{starts:?}");
        // The upper row's stem runs further than the lower one's, which its
        // line passes.
        let svg = crate::render(&format!(
            "@startuml\n{classes}Foo::a --> Bar::x\nFoo::b --> Bar\n@enduml\n"
        ))
        .unwrap();
        let mut lines: Vec<Vec<(f32, f32)>> = ["link-Foo-Bar", "link-Foo-Bar-2"]
            .iter()
            .map(|id| path(&svg, id))
            .collect();
        lines.sort_by(|a, b| a[0].1.total_cmp(&b[0].1));
        let far = |line: &[(f32, f32)]| line.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        assert!(far(&lines[0]) > far(&lines[1]) + 5.0, "{lines:?}");
    }

    /// A line shaped by hand leaves its row toward the corner it goes to,
    /// not through its box.
    #[test]
    fn a_line_by_hand_leaves_toward_its_corner() {
        let source = format!(
            "@startuml\n{CLASSES}Foo::field2 --> Bar::field3\n' lines by hand (Omaspec)\n' Foo -> Bar at -0.5 0.5\n@enduml\n"
        );
        let svg = crate::render(&source).unwrap();
        let points = path(&svg, "link-Foo-Bar");
        let foo = &svg[svg.find("id=\"class-Foo\"").unwrap()..];
        let x: f32 = foo[foo.find(" x=\"").unwrap() + 4..]
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            (points[0].0 - x).abs() < 1.0 && points[1].0 < points[0].0,
            "{points:?}"
        );
    }
}
