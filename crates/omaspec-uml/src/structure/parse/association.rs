//! Association classes (`(A, B) .. C`), as PlantUML reads and lays them:
//! the line between the pair runs through a point of no size, and the
//! class's line starts there.

use super::super::{Dash, Element, Group, Head, Link, Shape};
use super::{ACROSS, BESIDE, Lookup, Parser, Pending, RawName};
use crate::Declined;
use crate::common::text_at;

/// A line to an association class, laid once the lines between boxes
/// are: the pair's line runs through the point the class's starts at.
pub(super) struct Association {
    /// The pair's two names, as written.
    pub(super) pair: (String, String),
    /// The class.
    pub(super) class: Vec<usize>,
    /// Whether the pair was written first (`(A, B) .. C`).
    pub(super) pair_first: bool,
    pub(super) item: Pending,
}

impl RawName {
    /// The two names of `(A, B)`, which is an association class's pair
    /// where both name elements of the diagram, as PlantUML reads it:
    /// two plain names, a comma between them. Else it is a use case.
    pub(super) fn pair(&self) -> Option<(&str, &str)> {
        if self.shape != Some(Shape::UseCase) {
            return None;
        }
        let (a, b) = self.key.split_once(',')?;
        let plain = |name: &str| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.'))
        };
        let (a, b) = (a.trim(), b.trim());
        (plain(a) && plain(b)).then_some((a, b))
    }
}

impl Parser {
    /// Lays the line of an association class (`(A, B) .. C`), as PlantUML
    /// does: the line between A and B, or a plain one where none was
    /// written, runs through a point of no size ([`Shape::Point`]), and
    /// the class's line starts there. A second class of the pair meets
    /// the same point. Where the pair's line runs along the flow, the
    /// class stands beside the point, in its rank (after it, or before it
    /// when the class is written first); where it runs within a rank,
    /// the class is ranked after the point, or before it. Every second
    /// class of a pair stands on the other side.
    pub(super) fn associate(&mut self, association: Association) -> Result<(), Declined> {
        let Association {
            pair: (a, b),
            class,
            pair_first,
            mut item,
        } = association;
        let line = item.line;
        // The pair as a line's ends are found, from where it is written.
        let home = item.home.clone();
        let found = |name: &str| self.find_from(name, Lookup::Line, &home, None);
        let (Some(a), Some(b)) = (found(&a), found(&b)) else {
            return Err(Declined::at(
                line,
                "an association class needs two elements",
            ));
        };
        if a == b {
            return Err(Declined::at(
                line,
                "an association class of an element with itself is not supported yet",
            ));
        }
        let home = a[..a.len() - 1].to_vec();
        if b.len() != a.len() || !b.starts_with(&home) {
            return Err(Declined::at(
                line,
                "an association class between elements of different packages is not supported yet",
            ));
        }
        let (a, b) = (a[home.len()], b[home.len()]);
        let group = self.container_mut(&home);
        let names =
            |x: usize, y: usize| format!("({}, {})", group.nodes[x].key, group.nodes[y].key);
        let keys = [names(a, b), names(b, a)];
        let found = group
            .nodes
            .iter()
            .position(|node| node.shape == Shape::Point && keys.contains(&node.key));
        // How many classes meet the point already: every second one
        // stands on the other side.
        let (point, along, classes) = match found {
            Some(point) => {
                let half = |edge: &&Link| {
                    (edge.from == point && [a, b].contains(&edge.to))
                        || (edge.to == point && [a, b].contains(&edge.from))
                };
                let mut along = group
                    .edges
                    .iter()
                    .find(half)
                    .is_none_or(|edge| edge.level.is_none());
                let meets = |edge: &&Link| edge.from == point || edge.to == point;
                let classes = group.edges.iter().filter(meets).count()
                    - group.edges.iter().filter(half).count();
                // A second class: PlantUML runs the pair down the page, a
                // pair side by side turned, the first class left of its
                // point and the second right of it.
                let turned = !along && classes > 0;
                if classes == 1 {
                    for edge in &mut group.edges {
                        let ends = (edge.from, edge.to);
                        let halved = (ends.0 == point && [a, b].contains(&ends.1))
                            || (ends.1 == point && [a, b].contains(&ends.0));
                        if halved {
                            edge.level = None;
                        } else if (ends.0 == point || ends.1 == point)
                            && (turned || edge.level.is_some())
                        {
                            edge.level = Some(ends.0 != point);
                        }
                    }
                    along = true;
                }
                (point, along, classes)
            }
            None => {
                let at = group.edges.iter().position(|edge| {
                    (edge.from, edge.to) == (a, b) || (edge.from, edge.to) == (b, a)
                });
                let edge = match at {
                    Some(at) => group.edges.remove(at),
                    None => Link {
                        from: a,
                        to: b,
                        label: None,
                        from_card: None,
                        to_card: None,
                        from_head: Head::None,
                        to_head: Head::None,
                        color: None,
                        dash: Dash::Solid,
                        bold: false,
                        hidden: false,
                        level: None,
                        line,
                        label_before: None,
                        note: None,
                        members: [None, None],
                    },
                };
                let point = group.nodes.len();
                let [key, _] = keys;
                group.nodes.push(Element {
                    key,
                    label: text_at("", line),
                    stereotypes: Vec::new(),
                    members: Vec::new(),
                    shape: Shape::Point,
                    fill: None,
                    line_color: None,
                    thickness: None,
                    text_color: None,
                    border_dash: None,
                    explicit: false,
                    composite: false,
                    children: Group::default(),
                    link: None,
                    hidden: false,
                    spot: None,
                    line,
                });
                // The line's label and its first end on the first half,
                // its second end on the second.
                let first = Link {
                    to: point,
                    to_head: Head::None,
                    to_card: None,
                    ..edge.clone()
                };
                let second = Link {
                    from: point,
                    from_head: Head::None,
                    from_card: None,
                    label: None,
                    ..edge.clone()
                };
                let at = at.unwrap_or(group.edges.len());
                group.edges.splice(at..at, [first, second]);
                (point, edge.level.is_none(), 0)
            }
        };
        let point = [home.as_slice(), &[point]].concat();
        // Where the class stands is the pair's to say, one dot or dash
        // (`(A, B) . C`) too: the first beside the point in the order
        // written; the second right of it, the first then left; and so on,
        // side by side.
        if item.toward.is_none() || item.toward == Some(BESIDE) {
            let beside = class.len() == point.len() && class.starts_with(&home);
            let right = if pair_first { ACROSS } else { "left" };
            let left = if pair_first { "left" } else { ACROSS };
            item.toward = match (along, beside) {
                (true, true) if classes == 0 => Some(ACROSS),
                (true, true) if classes % 2 == 1 => Some(right),
                (true, true) => Some(left),
                _ => None,
            };
        }
        if pair_first {
            self.place_link(point, class, &item)
        } else {
            self.place_link(class, point, &item)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::{Diagram, Group};
    use super::*;

    fn diagram(body: &str) -> Diagram {
        let source = format!("@startuml\n{body}\n@enduml\n");
        let lines = crate::source::body(&source).unwrap().1;
        super::super::parse(&lines).unwrap_or_else(|err| panic!("{err:?}"))
    }

    fn declined(body: &str) -> String {
        let source = format!("@startuml\n{body}\n@enduml\n");
        let lines = crate::source::body(&source).unwrap().1;
        super::super::parse(&lines).unwrap_err().reason
    }

    fn index(group: &Group, key: &str) -> usize {
        group
            .nodes
            .iter()
            .position(|node| node.key == key)
            .unwrap_or_else(|| panic!("no {key}"))
    }

    fn edge(group: &Group, from: &str, to: &str) -> Link {
        let (from, to) = (index(group, from), index(group, to));
        group
            .edges
            .iter()
            .find(|edge| (edge.from, edge.to) == (from, to))
            .cloned()
            .unwrap_or_else(|| panic!("no line {from} to {to}"))
    }

    const PAIR: &str = "(Student, Course)";

    #[test]
    fn the_pairs_line_runs_through_a_point_the_class_starts_at() {
        // The guide's example: the multiplicities stay at their ends.
        let d = diagram(
            "class Student {\n  Name\n}\nStudent \"0..*\" -- \"1..*\" Course\n(Student, Course) .. Enrollment",
        );
        let root = &d.root;
        assert_eq!(root.nodes[index(root, PAIR)].shape, Shape::Point);
        assert_eq!(root.edges.len(), 3);
        let first = edge(root, "Student", PAIR);
        assert_eq!(first.from_card.unwrap().value, "0..*");
        assert!(first.to_card.is_none());
        let second = edge(root, PAIR, "Course");
        assert_eq!(second.to_card.unwrap().value, "1..*");
        assert!(second.from_card.is_none());
        // The pair's line runs along the flow: the class stands beside
        // the point, after it.
        let class = edge(root, PAIR, "Enrollment");
        assert_eq!(class.dash, Dash::Dashed);
        assert_eq!(class.level, Some(true));
        assert_eq!(class.line, 5);
        // One dot alone is a dotted line too, as the guide's second
        // example writes it, and the class beside the point all the same.
        let d = diagram("Student \"0..*\" -- \"1..*\" Course\n(Student, Course) . Enrollment");
        let class = edge(&d.root, PAIR, "Enrollment");
        assert_eq!((class.dash, class.level), (Dash::Dashed, Some(true)));
        // One dash lays the pair side by side, as PlantUML does: the class
        // stands under the point.
        let d = diagram("Student \"0..*\" - \"1..*\" Course\n(Student, Course) .. Enrollment");
        assert_eq!(edge(&d.root, "Student", PAIR).level, Some(true));
        assert_eq!(edge(&d.root, PAIR, "Enrollment").level, None);
    }

    #[test]
    fn the_lines_heads_and_label_keep_their_ends() {
        let d = diagram("Student \"*\" --> \"1\" Course : takes\n(Student, Course) .. Enrollment");
        let first = edge(&d.root, "Student", PAIR);
        let second = edge(&d.root, PAIR, "Course");
        assert_eq!(first.label.unwrap().value, "takes");
        assert!(second.label.is_none());
        assert_eq!((first.to_head, second.to_head), (Head::None, Head::Arrow));
    }

    #[test]
    fn the_class_written_first_stands_before_the_point() {
        let d = diagram("Enrollment .. (Student, Course)\nStudent -- Course");
        assert_eq!(edge(&d.root, "Enrollment", PAIR).level, Some(true));
    }

    #[test]
    fn a_second_class_of_a_pair_meets_its_point_on_the_other_side() {
        let d = diagram(
            "Student -- Course\n(Student, Course) .. Enrollment\n(Course, Student) .. Grade",
        );
        let points = d.root.nodes.iter().filter(|n| n.shape == Shape::Point);
        assert_eq!(points.count(), 1);
        // The first left of the point, the second right of it, as PlantUML
        // lays them, the classes written first or last.
        assert_eq!(edge(&d.root, PAIR, "Enrollment").level, Some(false));
        assert_eq!(edge(&d.root, PAIR, "Grade").level, Some(true));
        let d = diagram(
            "Student -right- Course\nEnrollment .. (Student, Course)\nGrade .. (Student, Course)",
        );
        assert_eq!(edge(&d.root, "Enrollment", PAIR).level, Some(true));
        assert_eq!(edge(&d.root, "Grade", PAIR).level, Some(false));
    }

    #[test]
    fn a_pair_within_a_rank_has_its_class_ranked_under_the_point() {
        let d = diagram(
            "Student -right- Course\n(Student, Course) .. Enrollment\n(Student, Course) .. Grade",
        );
        // A second class turns the pair down the page, as PlantUML lays
        // it: the first class left of the point, the second right of it.
        assert_eq!(edge(&d.root, "Student", PAIR).level, None);
        assert_eq!(edge(&d.root, PAIR, "Course").level, None);
        assert_eq!(edge(&d.root, PAIR, "Enrollment").level, Some(false));
        assert_eq!(edge(&d.root, PAIR, "Grade").level, Some(true));
        // So does one dash.
        let d = diagram("A - B\n(A, B) . C\n(A, B) . D");
        assert_eq!(edge(&d.root, "A", "(A, B)").level, None);
        assert_eq!(edge(&d.root, "(A, B)", "C").level, Some(false));
        assert_eq!(edge(&d.root, "(A, B)", "D").level, Some(true));
    }

    #[test]
    fn a_pair_without_a_line_has_one_drawn() {
        let d = diagram("class Student\nclass Course\n(Student, Course) .. Enrollment");
        let first = edge(&d.root, "Student", PAIR);
        assert_eq!((first.dash, first.to_head), (Dash::Solid, Head::None));
        edge(&d.root, PAIR, "Course");
    }

    #[test]
    fn names_that_are_no_boxes_are_a_use_case() {
        let d = diagram("(Eat, Drink) --> User");
        let case = &d.root.nodes[index(&d.root, "Eat, Drink")];
        assert_eq!(case.shape, Shape::UseCase);
        assert!(d.root.nodes.iter().all(|n| n.shape != Shape::Point));
        // A name with a space is no pair's.
        let d = diagram("class Eat\n(Eat, and drink) --> User");
        assert_eq!(
            d.root.nodes[index(&d.root, "Eat, and drink")].shape,
            Shape::UseCase
        );
    }

    #[test]
    fn what_cannot_be_drawn_is_declined_on_its_line() {
        for (body, says) in [
            (
                "A -- B\nC -- D\n(A, B) .. (C, D)",
                "a line between two association classes",
            ),
            ("A -- B\n(A, A) .. C", "of an element with itself"),
            (
                "package P {\n  class A\n}\nA -- B\n(A, B) .. C",
                "between elements of different packages",
            ),
        ] {
            let reason = declined(body);
            assert!(reason.contains(says), "{body}: {reason}");
        }
    }

    #[test]
    fn the_point_goes_with_either_box_of_its_pair() {
        let d = diagram("Student -- Course\n(Student, Course) .. Enrollment\nremove Student");
        assert!(d.root.nodes.iter().all(|n| n.shape != Shape::Point));
        assert!(d.root.edges.is_empty());
        // Without its class the pair's line still runs through it.
        let d = diagram("Student -- Course\n(Student, Course) .. Enrollment\nremove Enrollment");
        assert_eq!(d.root.edges.len(), 2);
        // Removed by a pattern and restored by name, the three come back
        // with their lines; so do they shown again.
        for (out, back) in [("remove", "restore"), ("hide", "show")] {
            let d = diagram(&format!(
                "A -- B\n(A, B) .. C\n{out} *\n{back} A\n{back} B\n{back} C"
            ));
            assert_eq!(d.root.edges.len(), 3, "{out}");
            assert!(d.root.edges.iter().all(|edge| !edge.hidden), "{out}");
        }
        // A pair hidden hides its line and its class's.
        let d = diagram("Student -- Course\n(Student, Course) .. Enrollment\nhide Student");
        assert!(d.root.nodes[index(&d.root, PAIR)].hidden);
        assert!(d.root.edges.iter().all(|edge| edge.hidden));
    }

    #[test]
    fn a_fold_counts_the_pairs_line_once_and_no_point() {
        let mut d = diagram("package P {\n  class A\n  class B\n  A -- B\n  (A, B) .. C\n}");
        d.fold(&["P".to_owned()]);
        let folded = &d.root.nodes[index(&d.root, "P")];
        let said = folded.stereotypes.last().unwrap();
        assert_eq!(said.value, "3 elements · 2 lines");
    }

    /// A class outside the package of its pair leaves the pair's line
    /// straight: its point follows the pair alone, as in PlantUML.
    #[test]
    fn a_class_outside_its_pair_s_package_leaves_the_pair_straight() {
        let svg = crate::render(
            "@startuml\npackage P {\n  class Student\n  class Course\n  Student -- Course\n}\n\
             (Student, Course) .. Enrollment\n@enduml\n",
        )
        .unwrap();
        let half = |id: &str| -> Vec<f32> {
            let at = svg
                .find(&format!("id=\"{id}\""))
                .unwrap_or_else(|| panic!("{svg}"));
            let points = &svg[at..];
            let points = &points[points.find("points=\"").unwrap() + 8..];
            let points = &points[..points.find('"').unwrap()];
            points
                .split_whitespace()
                .map(|pair| pair.split(',').next().unwrap().parse().unwrap())
                .collect()
        };
        let xs: Vec<f32> = [
            half("link-Student-_28_Student_2c__20_Course_29_"),
            half("link-_28_Student_2c__20_Course_29_-Course"),
        ]
        .concat();
        assert!(xs.iter().all(|x| (x - xs[0]).abs() < 0.5), "{xs:?}: {svg}");
    }

    #[test]
    fn the_point_is_drawn_as_nothing() {
        let svg = crate::render(
            "@startuml\nStudent -- Course\n(Student, Course) .. Enrollment\n@enduml\n",
        )
        .unwrap();
        // The three classes' circles, none for the point.
        assert_eq!(svg.matches("<ellipse").count(), 3, "{svg}");
        assert!(!svg.contains("Student, Course"), "{svg}");
        assert_eq!(svg.matches("stroke-dasharray").count(), 1, "{svg}");
    }
}
