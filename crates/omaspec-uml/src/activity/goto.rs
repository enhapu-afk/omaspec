//! `label` and `goto`: a goto's flow stops where it stands, and a line of
//! its own leads from there to the point its label marks, halfway along
//! the line into the step after the label.
//!
//! As PlantUML aligns them, a label in another branch above its goto is
//! moved down to it, so the line between comes straight across: the
//! diagram is laid out again with room before that label. Where the room
//! moves the goto down with it (a goto back up its own flow), the label
//! keeps its place and the line goes up outside everything.

use std::collections::HashMap;

use super::Flow;
use super::layout::{GAP, Layout, RAIL};
use super::tile::{Edge, Pt, Shape, Tile, pt};
use crate::Declined;

impl Layout<'_> {
    /// `flow`, the whole diagram's, laid out with its labels moved down to
    /// their gotos where that brings them level, and the gotos' lines.
    pub fn whole(&self, flow: &Flow) -> Result<Tile, Declined> {
        let mut tile = self.flow(flow, 0)?;
        let wanted = above(&tile);
        if !wanted.is_empty() {
            let mut spacers = wanted;
            // At most once more without the rooms that moved their gotos
            // down too.
            for _ in 0..2 {
                let again = Layout {
                    skin: self.skin,
                    family: self.family,
                    spacers: spacers.clone(),
                }
                .flow(flow, 0)?;
                let still = above(&again);
                let kept: HashMap<String, f32> = spacers
                    .iter()
                    .filter(|(name, _)| !still.contains_key(*name))
                    .map(|(name, room)| (name.clone(), *room))
                    .collect();
                if kept.len() == spacers.len() {
                    tile = again;
                    break;
                }
                spacers = kept;
                if spacers.is_empty() {
                    break;
                }
            }
        }
        self.gotos(tile)
    }

    /// The lines of the `goto`s in `tile`, the whole diagram: each from
    /// where its flow stops to where its label stands. A label further
    /// down is reached by a line down and across where nothing stands in
    /// its way; one above, or one the shapes between hide, by a rail
    /// outside everything in the goto's swimlane: on the goto's own side
    /// of the label, clear of what stands on the other; toward the
    /// label's lane where that is another.
    fn gotos(&self, mut tile: Tile) -> Result<Tile, Declined> {
        let Some(jumps) = tile.jumps.take() else {
            return Ok(tile);
        };
        for (name, from, line) in jumps.gotos {
            let Some(&(_, to)) = jumps.marks.iter().find(|(n, _)| *n == name) else {
                return Err(Declined::at(
                    line,
                    format!("`goto {name}` leads to no `label {name}`"),
                ));
            };
            let straight = match (from.x - to.x).abs() < 0.5 && from.lane == to.lane {
                true => vec![from, to],
                false => vec![from, pt(from.x, to.y, from.lane), to],
            };
            let points = if from.y <= to.y + 0.5 && clear(&tile, &straight) {
                straight
            } else {
                let below = from.y + GAP / 2.0;
                let leftward = match from.lane == to.lane {
                    true => from.x <= to.x,
                    false => to.lane < from.lane,
                };
                let (left, right) = reach(&tile, from.lane);
                let side = if leftward { left - RAIL } else { right + RAIL };
                vec![
                    from,
                    pt(from.x, below, from.lane),
                    pt(side, below, from.lane),
                    pt(side, to.y, from.lane),
                    to,
                ]
            };
            tile.edges.push(Edge::new(points, true, self.skin.arrow));
        }
        tile.measure();
        Ok(tile)
    }

    /// Room before the step after a label, `room` more than the arrow
    /// between two steps: a line straight down, with no head where the
    /// arrow into it ends; with no line and no way out where the flow
    /// before it stopped (`goes_on` false).
    pub(super) fn spacer(&self, room: f32, lane: usize, goes_on: bool) -> Tile {
        let height = (room - GAP).max(0.0);
        let (top, bottom) = (pt(0.0, 0.0, lane), pt(0.0, height, lane));
        let mut stroke = self.skin.arrow;
        if !goes_on {
            // No width: the painter passes over it, and it holds the room.
            stroke.width = 0.0;
        }
        let edges = match height > 0.0 {
            true => vec![Edge::new(vec![top, bottom], false, stroke)],
            false => Vec::new(),
        };
        let mut tile = Tile::new(Vec::new(), edges, top, goes_on.then_some(bottom));
        tile.through = true;
        tile
    }
}

/// Whether the line through `points` passes no shape of `tile` that
/// stands in its swimlane: a box, a diamond, a circle, a bar or a note.
/// A line from one lane to another is not looked at.
fn clear(tile: &Tile, points: &[Pt]) -> bool {
    points.windows(2).all(|pair| {
        let (a, b) = (pair[0], pair[1]);
        if a.lane != b.lane {
            return true;
        }
        let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
        let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
        tile.shapes.iter().all(|shape| {
            if matches!(
                shape,
                Shape::Frame { .. } | Shape::Label { .. } | Shape::Test { .. }
            ) || shape.lane().is_some_and(|lane| lane != a.lane)
            {
                return true;
            }
            let (l, t, r, bo) = shape.bounds();
            // Touching an edge is no crossing: a line leaves a box there.
            let e = 0.5;
            !(x0 < r - e && x1 > l + e && y0 < bo - e && y1 > t + e)
        })
    })
}

/// How far left and right what stands in swimlane `lane` reaches.
fn reach(tile: &Tile, lane: usize) -> (f32, f32) {
    let mut out = (f32::INFINITY, f32::NEG_INFINITY);
    let mut grow = |lo: f32, hi: f32| out = (out.0.min(lo), out.1.max(hi));
    for shape in &tile.shapes {
        match shape {
            Shape::Bar { spans, .. } => {
                for &(lo, hi, at) in spans {
                    if at == lane {
                        grow(lo, hi);
                    }
                }
            }
            _ if shape.lane() == Some(lane) => {
                let (l, _, r, _) = shape.bounds();
                grow(l, r);
            }
            _ => {}
        }
    }
    for p in tile.edges.iter().flat_map(|e| &e.points) {
        if p.lane == lane {
            grow(p.x, p.x);
        }
    }
    if out.0 > out.1 {
        (tile.bounds.0, tile.bounds.2)
    } else {
        out
    }
}

/// How far each label stands above a goto that leads to it, the farthest
/// for a label more than one leads to.
fn above(tile: &Tile) -> HashMap<String, f32> {
    let mut out: HashMap<String, f32> = HashMap::new();
    let Some(jumps) = &tile.jumps else {
        return out;
    };
    for (name, from, _) in &jumps.gotos {
        let Some((_, to)) = jumps.marks.iter().find(|(n, _)| n == name) else {
            continue;
        };
        let rise = from.y - to.y;
        if rise > 0.5 {
            let room = out.entry(name.clone()).or_default();
            *room = room.max(rise);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::skin::Skin;
    use super::*;
    use crate::text::Family;

    /// The whole diagram `source` laid out, its gotos' lines last.
    fn laid(source: &str) -> Tile {
        let Ok(crate::Diagram::Activity(d)) = crate::parse(source) else {
            panic!("{source}");
        };
        let skin = Skin::from_params(&d.frame.skinparams).unwrap();
        Layout {
            skin: &skin,
            family: Family::Inter,
            spacers: HashMap::new(),
        }
        .whole(&d.flow)
        .unwrap()
    }

    /// PlantUML's own example: the label in the first branch moves down
    /// to the goto in the second, which comes straight across to it.
    #[test]
    fn a_label_above_its_goto_in_another_branch_comes_down_to_it() {
        let tile = laid(
            "@startuml\nstart\nif (a?) then (yes)\nlabel sp_lab0\nlabel lab\n:shared;\nelse (no)\nif (b?) then (yes)\nlabel sp_lab2\ngoto lab\nelse\n:nonShared;\nendif\nendif\n:merge;\n@enduml\n",
        );
        let goto = tile.edges.last().unwrap();
        let [from, turn, to] = goto.points[..] else {
            panic!("{:?}", goto.points);
        };
        assert!(goto.head);
        assert!(from.x > to.x, "{:?}", goto.points);
        assert_eq!((from.x, turn.y), (turn.x, to.y), "{:?}", goto.points);
        assert!(from.y <= to.y, "{:?}", goto.points);
        // The line into the goto's branch goes on as the goto's, with no
        // head where it ends.
        let into = tile
            .edges
            .iter()
            .find(|e| e.points.last() == Some(&from))
            .unwrap();
        assert!(!into.head);
    }

    /// A goto back up its own flow keeps the label where it is and goes
    /// up outside everything, on its own side.
    #[test]
    fn a_goto_back_up_goes_around() {
        let tile = laid(
            "@startuml\nstart\n:x;\nlabel again\n:y;\nif (t) then\ngoto again\nelse\n:z;\nendif\nstop\n@enduml\n",
        );
        let goto = tile.edges.last().unwrap();
        assert_eq!(goto.points.len(), 5, "{:?}", goto.points);
        let side = goto.points[2].x;
        let left = tile
            .shapes
            .iter()
            .map(|s| s.bounds().0)
            .fold(f32::INFINITY, f32::min);
        assert!(side < left, "{side} {left}");
        assert!(goto.points[4].y < goto.points[0].y);
    }

    /// The words of a shape, if it has any.
    fn words(shape: &Shape) -> String {
        let block = match shape {
            Shape::Box { text, .. } | Shape::Label { text, .. } => text,
            _ => return String::new(),
        };
        block
            .lines
            .iter()
            .flat_map(|l| &l.runs)
            .map(|r| r.text.as_str())
            .collect()
    }

    fn shape<'a>(tile: &'a Tile, text: &str) -> &'a Shape {
        tile.shapes
            .iter()
            .find(|s| words(s) == text)
            .unwrap_or_else(|| panic!("{text}"))
    }

    /// Found in review (2026-11-24): a goto down went through the steps
    /// between, the room before a label after a flow that stopped drew a
    /// line from nothing, and a label after `stop` stood at the top.
    #[test]
    fn gotos_go_around_what_stands_between_and_stopped_flows_stay_stopped() {
        let tile = laid(
            "@startuml\nstart\nif (a) then\n:p;\ngoto out\nlabel lab\n:shared;\nelse\n:q;\n:r;\n:s;\n:t;\ngoto lab\nendif\n:merge;\nlabel out\n:end;\n@enduml\n",
        );
        let gotos = &tile.edges[tile.edges.len() - 2..];
        for goto in gotos {
            assert!(clear(&tile, &goto.points), "{:?}", goto.points);
        }
        // Nothing flows into `shared` but the goto, which ends above it.
        let Shape::Box { rect, .. } = shape(&tile, "shared") else {
            unreachable!()
        };
        let top = pt(rect.0 + rect.2 / 2.0, rect.1, 0);
        let into = tile.edges.iter().filter(|e| e.stroke.width > 0.0).any(|e| {
            e.points
                .last()
                .is_some_and(|p| (p.y - top.y).abs() < 0.5 && (p.x - top.x).abs() < 0.5)
        });
        assert!(!into, "{:?}", tile.edges);
        // A label after `stop` stands below it.
        let tile = laid(
            "@startuml\nstart\n:a;\nif (x) then\n:b;\ngoto skip\nendif\n:c;\nstop\nlabel skip\n@enduml\n",
        );
        let goto = tile.edges.last().unwrap();
        let lowest = tile.shapes.iter().map(|s| s.bounds().3).fold(0.0, f32::max);
        assert!(goto.points.last().unwrap().y > lowest, "{:?}", goto.points);
    }

    /// Found in review: a goto up into another lane jogged through the
    /// middle; its rail keeps to its own lane.
    #[test]
    fn a_goto_into_another_lane_keeps_its_rail_in_its_own() {
        let tile = laid(
            "@startuml\n|A|\nstart\n:x;\nlabel top\n:y;\n|B|\n:z;\nif (again?) then (yes)\ngoto top\nendif\n:w;\n|A|\nstop\n@enduml\n",
        );
        let goto = tile.edges.last().unwrap();
        assert_eq!(goto.points.len(), 5, "{:?}", goto.points);
        assert!(
            goto.points[..4].iter().all(|p| p.lane == 1),
            "{:?}",
            goto.points
        );
        assert_eq!(goto.points[4].lane, 0);
        // Leftward, toward the label's lane.
        assert!(goto.points[2].x < goto.points[1].x);
    }

    /// Found in review: a label first in a flow hid the words of its first
    /// arrow from the room above it.
    #[test]
    fn a_label_first_in_a_flow_leaves_its_arrow_room() {
        let tile = laid(
            "@startuml\nstart\nwhile (more?) is (yes)\nlabel top\n-> a long arrow label here;\n:a;\nendwhile\nstop\n@enduml\n",
        );
        let (yes, said) = (shape(&tile, "yes"), shape(&tile, "a long arrow label here"));
        let (Shape::Label { at: y, text: t, .. }, Shape::Label { at: s, .. }) = (yes, said) else {
            unreachable!()
        };
        assert!(s.1 >= y.1 + t.height - 0.5, "{y:?} {s:?}");
    }

    #[test]
    fn a_goto_needs_its_label() {
        let err = crate::render("@startuml\nstart\n:a;\ngoto nowhere\n@enduml\n").unwrap_err();
        assert_eq!(err.line, Some(3));
        assert!(
            err.reason
                .contains("`goto nowhere` leads to no `label nowhere`"),
            "{}",
            err.reason
        );
        let err = crate::render("@startuml\nstart\nlabel\n:a;\n@enduml\n").unwrap_err();
        assert!(err.reason.contains("a name of one word"), "{}", err.reason);
        let err = crate::render("@startuml\nstart\nlabel x\n:a;\nlabel x\n@enduml\n").unwrap_err();
        assert_eq!(err.line, Some(4));
        assert!(
            err.reason.contains("written twice, here and on line 3"),
            "{}",
            err.reason
        );
        let err =
            crate::render("@startuml\nstart\nlabel y\n:a;\nstop\ngoto y\n@enduml\n").unwrap_err();
        assert_eq!(err.line, Some(5));
        assert!(err.reason.contains("has stopped already"), "{}", err.reason);
        // Labels last in a flow, and a flow that only goes to one, draw.
        crate::render("@startuml\nstart\n:a;\nlabel end\n@enduml\n").unwrap();
        crate::render("@startuml\nstart\nlabel top\n:a;\n:b;\ngoto top\n@enduml\n").unwrap();
    }
}
