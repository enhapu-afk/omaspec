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
use super::tile::{Edge, Tile, pt};
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
    /// down is reached by a line down and across; one above, by a way up
    /// outside everything, on the side the goto stands on.
    fn gotos(&self, mut tile: Tile) -> Result<Tile, Declined> {
        let (left, right) = (tile.bounds.0, tile.bounds.2);
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
            let points = if from.y <= to.y + 0.5 {
                if (from.x - to.x).abs() < 0.5 {
                    vec![from, to]
                } else {
                    vec![from, pt(from.x, to.y, from.lane), to]
                }
            } else {
                let below = from.y + GAP / 2.0;
                let side = if from.x <= to.x {
                    left - RAIL
                } else {
                    right + RAIL
                };
                vec![
                    from,
                    pt(from.x, below, from.lane),
                    pt(side, below, from.lane),
                    pt(side, to.y, to.lane),
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
    /// arrow into it ends.
    pub(super) fn spacer(&self, room: f32, lane: usize) -> Tile {
        let height = (room - GAP).max(0.0);
        let (top, bottom) = (pt(0.0, 0.0, lane), pt(0.0, height, lane));
        let edges = match height > 0.0 {
            true => vec![Edge::new(vec![top, bottom], false, self.skin.arrow)],
            false => Vec::new(),
        };
        let mut tile = Tile::new(Vec::new(), edges, top, Some(bottom));
        tile.through = true;
        tile
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
        // Labels last in a flow, and a flow that only goes to one, draw.
        crate::render("@startuml\nstart\n:a;\nlabel end\n@enduml\n").unwrap();
        crate::render("@startuml\nstart\nlabel top\n:a;\n:b;\ngoto top\n@enduml\n").unwrap();
    }
}
