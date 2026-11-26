//! What a pane needs of an activity diagram to glide its boxes and let
//! them through a veil in their own shapes (docs/adr/0029): a name for
//! each shape, the part it is drawn in, and each line from the shape it
//! leaves to the one it enters.
//!
//! The layout knows no names: an action is its words, and a line runs from
//! point to point, often to a point where lines meet and on from there. So
//! a shape is named by its kind and its words, the second of the same name
//! counted, and a line is followed through such points to the next shape:
//! the line the map tells is the whole way from one shape to another.

use std::collections::{HashMap, HashSet};

use super::tile::Shape;
use crate::graph::Rect;
use crate::hand::{End, Key};
use crate::map::{Head, HeadFill, Place, Wire};
use crate::svg::Stroke;
use crate::text::Block;

/// How far from a shape's box a line may end and still end on it.
const REACH: f32 = 1.5;
/// How many ways through points where lines meet are followed for each
/// line drawn, before the rest is told run by run.
const WAYS: usize = 8;

/// How far from a line its words may stand and still be its words.
const BESIDE: f32 = 14.0;

/// How far `rect` is from the line through `points`.
fn away(rect: Rect, points: &[(f32, f32)]) -> f32 {
    let to = |(x, y): (f32, f32)| {
        let dx = (rect.x - x).max(0.0).max(x - rect.x - rect.w);
        let dy = (rect.y - y).max(0.0).max(y - rect.y - rect.h);
        dx.hypot(dy)
    };
    points
        .windows(2)
        .flat_map(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let steps = ((b.0 - a.0).hypot(b.1 - a.1) / 3.0).ceil().max(1.0) as usize;
            (0..=steps).map(move |i| {
                let t = i as f32 / steps as f32;
                (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
            })
        })
        .map(to)
        .fold(f32::INFINITY, f32::min)
}

#[derive(Default)]
pub(super) struct Found {
    /// How many shapes of each kind and words there were.
    seen: HashMap<String, usize>,
    /// The names given: no two shapes share one.
    used: HashSet<String>,
    pub places: Vec<Place>,
    runs: Vec<Run>,
    /// The words beside lines: each one's part and where it is drawn.
    labels: Vec<(String, Rect)>,
}

/// One line as drawn, between two points.
struct Run {
    /// As drawn, its corners rounded.
    points: Vec<(f32, f32)>,
    /// Its straight runs as laid, a point at each corner.
    path: Vec<(f32, f32)>,
    head: Option<Vec<(f32, f32)>>,
    stroke: Stroke,
}

/// Where a run ends: on a shape, or at a point.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Node {
    Shape(usize),
    Point(i32, i32),
}

pub(super) fn words(text: &Block) -> String {
    text.lines
        .iter()
        .map(|line| {
            line.runs
                .iter()
                .map(|run| run.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Found {
    /// The part `shape`, drawn at `rect`, is drawn in. A shape a line may
    /// end on is a place of the map; a partition and a label are parts
    /// only.
    pub fn shape(&mut self, shape: &Shape, rect: Rect) -> String {
        let (kind, said, place) = match shape {
            Shape::Box { text, .. } => ("action", words(text), true),
            Shape::Diamond { text, .. } => ("test", words(text), true),
            Shape::Start { .. } => ("start", String::new(), true),
            Shape::Stop { .. } => ("stop", String::new(), true),
            Shape::Connector { text, .. } => ("connector", words(text), true),
            Shape::End { .. } => ("end", String::new(), true),
            Shape::Bar { .. } => ("bar", String::new(), true),
            Shape::Note { text, .. } => ("note", words(text), false),
            Shape::Frame { title, kind, .. } => {
                let kind = match kind {
                    super::FrameKind::Partition => "partition",
                    super::FrameKind::Group => "group",
                    super::FrameKind::Package => "package",
                    super::FrameKind::Rectangle => "rectangle",
                    super::FrameKind::Card => "card",
                };
                (kind, words(title), false)
            }
            Shape::Label { text, .. } => ("label", words(text), false),
            Shape::Test { text, .. } => ("test", words(text), false),
        };
        // Its kind and its words, as an id may hold them
        // (`action-Pay_20_the_20_bill`, docs/adr/0037).
        let base = crate::svg::element(kind, &said);
        // The second of a name is counted; a count that is another
        // shape's own words (`A-2`) is passed over.
        let mut nth = self.seen.get(&base).copied().unwrap_or(0);
        let name = loop {
            nth += 1;
            let name = if nth > 1 {
                format!("{base}-{nth}")
            } else {
                base.clone()
            };
            if self.used.insert(name.clone()) {
                break name;
            }
        };
        self.seen.insert(base, nth);
        if matches!(shape, Shape::Label { .. }) {
            self.labels.push((name.clone(), rect));
        }
        if place {
            self.places.push(Place {
                name: name.clone(),
                rect,
                within: Vec::new(),
                across: false,
                entry: None,
                frame: false,
                fixed: true,
                part: Some(name.clone()),
            });
        }
        name
    }

    /// A line: its corners as laid, its points as drawn, and its head.
    pub fn run(
        &mut self,
        path: Vec<(f32, f32)>,
        points: Vec<(f32, f32)>,
        head: Option<Vec<(f32, f32)>>,
        stroke: Stroke,
    ) {
        self.runs.push(Run {
            points,
            path,
            head,
            stroke,
        });
    }

    fn node(&self, (x, y): (f32, f32)) -> Node {
        self.places
            .iter()
            .enumerate()
            .filter(|(_, place)| {
                let r = place.rect;
                x >= r.x - REACH
                    && x <= r.x + r.w + REACH
                    && y >= r.y - REACH
                    && y <= r.y + r.h + REACH
            })
            .min_by(|(_, a), (_, b)| (a.rect.w * a.rect.h).total_cmp(&(b.rect.w * b.rect.h)))
            .map_or(
                Node::Point((x * 2.0).round() as i32, (y * 2.0).round() as i32),
                |(at, _)| Node::Shape(at),
            )
    }

    /// The run each label stands beside: the nearest, within [`BESIDE`].
    fn labelled(&self) -> Vec<Option<usize>> {
        self.labels
            .iter()
            .map(|(_, rect)| {
                self.runs
                    .iter()
                    .enumerate()
                    .map(|(at, run)| (at, away(*rect, &run.points)))
                    .filter(|(_, d)| *d <= BESIDE)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(at, _)| at)
            })
            .collect()
    }

    /// The lines from shape to shape: each run that leaves a shape,
    /// followed through the points where runs meet. A run no such way
    /// takes is a line of its own.
    pub fn wires(&self) -> Vec<Wire> {
        let ends: Vec<(Node, Node)> = self
            .runs
            .iter()
            .map(|run| {
                (
                    self.node(run.points[0]),
                    self.node(run.points[run.points.len() - 1]),
                )
            })
            .collect();
        let mut leaving: HashMap<Node, Vec<usize>> = HashMap::new();
        for (at, (from, _)) in ends.iter().enumerate() {
            leaving.entry(*from).or_default().push(at);
        }
        let mut ways: Vec<Vec<usize>> = Vec::new();
        let mut taken = vec![false; self.runs.len()];
        let budget = self.runs.len() * WAYS;
        // Each way a list of runs, the first leaving a shape.
        let mut open: Vec<Vec<usize>> = ends
            .iter()
            .enumerate()
            .filter(|(_, (from, _))| matches!(from, Node::Shape(_)))
            .map(|(at, _)| vec![at])
            .rev()
            .collect();
        while let Some(way) = open.pop() {
            let last = way[way.len() - 1];
            let next: Vec<usize> = match ends[last].1 {
                Node::Shape(_) => Vec::new(),
                point => leaving
                    .get(&point)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|run| !way.contains(run))
                    .collect(),
            };
            if next.is_empty() || ways.len() + open.len() >= budget {
                for &run in &way {
                    taken[run] = true;
                }
                ways.push(way);
                continue;
            }
            for run in next.into_iter().rev() {
                let mut longer = way.clone();
                longer.push(run);
                open.push(longer);
            }
        }
        ways.extend(
            (0..self.runs.len())
                .filter(|&run| !taken[run])
                .map(|run| vec![run]),
        );

        // A line's words are lit with it; they travel with the first line
        // that takes their run.
        let labelled = self.labelled();
        let mut given = vec![false; self.labels.len()];
        let mut nth: HashMap<(String, String), usize> = HashMap::new();
        ways.iter()
            .map(|way| {
                let mut labels = Vec::new();
                let mut words: [Option<String>; 3] = Default::default();
                for (label, run) in labelled.iter().enumerate() {
                    if run.is_some_and(|run| way.contains(&run)) {
                        labels.push(self.labels[label].1);
                        if !given[label] && words[crate::map::WORDS].is_none() {
                            words[crate::map::WORDS] = Some(self.labels[label].0.clone());
                            given[label] = true;
                        }
                    }
                }
                let (first, last) = (way[0], way[way.len() - 1]);
                let joined = |of: fn(&Run) -> &[(f32, f32)]| {
                    let mut points: Vec<(f32, f32)> = Vec::new();
                    for &run in way {
                        for &p in of(&self.runs[run]) {
                            if points.last() != Some(&p) {
                                points.push(p);
                            }
                        }
                    }
                    points
                };
                let points = joined(|run| &run.points);
                let path = joined(|run| &run.path);
                let end = |node: Node, at: (f32, f32)| match node {
                    Node::Shape(place) => {
                        (self.places[place].name.clone(), self.places[place].rect)
                    }
                    Node::Point(..) => (
                        String::new(),
                        Rect {
                            x: at.0,
                            y: at.1,
                            w: 0.0,
                            h: 0.0,
                        },
                    ),
                };
                let (from, from_rect) = end(ends[first].0, points[0]);
                let (to, to_rect) = end(ends[last].1, points[points.len() - 1]);
                let count = nth.entry((from.clone(), to.clone())).or_insert(0);
                let key = Key {
                    from,
                    to,
                    nth: *count,
                };
                *count += 1;
                Wire {
                    key,
                    line: None,
                    path,
                    points,
                    from: End {
                        rect: from_rect,
                        flat: false,
                    },
                    to: End {
                        rect: to_rect,
                        flat: false,
                    },
                    entry: None,
                    labels,
                    stroke: self.runs[first].stroke,
                    heads: self.runs[last]
                        .head
                        .iter()
                        .map(|points| Head {
                            points: points.clone(),
                            closed: true,
                            fill: HeadFill::Ink,
                        })
                        .collect(),
                    fixed: true,
                    words,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    /// An action whose words are another one's name with its count keeps
    /// a name of its own.
    #[test]
    fn no_two_shapes_share_a_name() {
        let source = "@startuml\nstart\n:A;\n:A #2;\n:A;\n:A;\nstop\n@enduml";
        let (_, map) = mapped(source);
        let names: Vec<&str> = map.places.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "start",
                "action-A",
                "action-A_20__23_2",
                "action-A-2",
                "action-A-3",
                "stop"
            ]
        );
    }

    const PARTS: &str = "\n' parts (Omaspec)\n";

    fn mapped(source: &str) -> (String, crate::map::Map) {
        crate::render_mapped(&format!("{source}{PARTS}"), None).unwrap()
    }

    /// Asked for its parts, an activity diagram draws each shape in a part
    /// the map names, and tells each line from the shape it leaves to the
    /// one it enters; a picture for a file has no parts and is the same.
    #[test]
    fn shapes_are_parts_and_lines_run_from_shape_to_shape() {
        let source = "@startuml\nstart\n:Build;\nif (green?) then (yes)\n  :Publish;\nelse (no)\n  :Fix;\n  stop\nendif\n:Build;\nstop\n@enduml";
        let (svg, map) = mapped(source);
        let names: Vec<&str> = map.places.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "start",
                "action-Build",
                "test-green_3f_",
                "action-Publish",
                "action-Fix",
                "stop",
                "action-Build-2",
                "stop-2"
            ]
        );
        let drawn: HashSet<String> = crate::part_groups(&svg)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        for place in &map.places {
            assert!(place.fixed && !place.frame);
            assert!(
                drawn.contains(place.part.as_deref().unwrap()),
                "{}",
                place.name
            );
        }
        assert!(drawn.contains("label-yes") && drawn.contains("label-no"));
        let lines: Vec<(&str, &str)> = map
            .wires
            .iter()
            .map(|w| (w.key.from.as_str(), w.key.to.as_str()))
            .collect();
        for line in [
            ("start", "action-Build"),
            ("test-green_3f_", "action-Publish"),
            ("test-green_3f_", "action-Fix"),
            ("action-Fix", "stop"),
            ("action-Publish", "action-Build-2"),
            ("action-Build-2", "stop-2"),
        ] {
            assert!(lines.contains(&line), "{line:?} not in {lines:?}");
        }
        for wire in &map.wires {
            assert!(wire.fixed && wire.heads.len() <= 1 && wire.points.len() >= 2);
            assert_eq!(wire.points.first(), wire.path.first());
            assert_eq!(wire.points.last(), wire.path.last());
        }
        // A line that turns is told as drawn, round, and laid, a point at
        // each corner.
        let turning = map.wires.iter().find(|w| w.path.len() > 2).unwrap();
        assert!(turning.points.len() > turning.path.len());
        // The words beside a line are its words.
        let words = |from: &str, to: &str| {
            let wire = map
                .wires
                .iter()
                .find(|w| w.key.from == from && w.key.to == to)
                .unwrap();
            (wire.words[crate::map::WORDS].clone(), wire.labels.len())
        };
        assert_eq!(
            words("test-green_3f_", "action-Publish"),
            (Some("label-yes".to_owned()), 1)
        );
        assert_eq!(
            words("test-green_3f_", "action-Fix"),
            (Some("label-no".to_owned()), 1)
        );
        assert_eq!(words("start", "action-Build"), (None, 0));

        let plain = crate::render(source).unwrap();
        assert!(!crate::has_parts(&plain));
        assert_eq!(crate::unmarked(&svg), plain);
    }

    /// Every line of every activity diagram of the tests is told once or
    /// on a way through a point where lines meet, with a key of its own.
    #[test]
    fn every_line_has_a_key_of_its_own() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/activity");
        for file in std::fs::read_dir(dir).unwrap() {
            let path = file.unwrap().path();
            let source = std::fs::read_to_string(&path).unwrap();
            let Ok((svg, map)) = crate::render_mapped(&format!("{source}{PARTS}"), None) else {
                continue;
            };
            let mut keys = HashSet::new();
            for wire in &map.wires {
                assert!(
                    keys.insert(wire.key.clone()),
                    "{path:?}: {:?} twice",
                    wire.key
                );
            }
            let mut names = HashSet::new();
            for place in &map.places {
                assert!(names.insert(&place.name), "{path:?}: {} twice", place.name);
            }
            assert!(!map.places.is_empty() && crate::has_parts(&svg), "{path:?}");
        }
    }
}
