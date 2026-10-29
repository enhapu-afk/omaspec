//! Lays out a mind map the way PlantUML does and draws it.
//!
//! Every idea with what hangs below it is a tee: its box, then the
//! column of its children. The children of an idea are packed one below
//! the other, each as high as both parts of its tee allow ("Tetris"),
//! then centered on the idea. A link is an S-curve from the end of an
//! idea's box to the start of each child's.
//!
//! The layout works along two axes: the elongation, the way the map
//! grows (right, or down when vertical), and the thickness across it.

use super::skin::{Skin, TreeKind};
use super::{Annotations, Idea, Label, MARGIN, Map, MindMap, Node};
use crate::Declined;
use crate::svg::{Canvas, Layer, n};
use crate::text::Family;

pub fn draw(d: &MindMap) -> Result<String, Declined> {
    let skin = Skin::from_params(&d.frame.skinparams, TreeKind::MindMap)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    let family = canvas.family();
    let annotations = Annotations::new(
        &d.frame,
        [
            skin.title_font,
            skin.header_font,
            skin.footer_font,
            skin.caption_font,
        ],
        family,
    )?;
    let geometry = Geometry {
        skin: &skin,
        vertical: d.vertical,
    };
    let mut top = annotations.top();
    for map in &d.maps {
        top += geometry.draw_map(&mut canvas, map, family, top)?;
    }
    annotations.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, skin.background))
}

struct Geometry<'a> {
    skin: &'a Skin,
    vertical: bool,
}

/// An idea with the ideas below it, measured and packed.
struct Finger {
    /// The idea's box, none for the root on its second side, where the
    /// first side draws it.
    label: Option<Label>,
    /// Elongation and thickness of the box with its margins.
    elongation: f32,
    thickness: f32,
    children: Vec<Finger>,
    /// Where each child's center line lies, from this idea's.
    offsets: Vec<f32>,
    /// The packed children's thickness and elongation.
    nail_thickness: f32,
    nail_elongation: f32,
}

impl Finger {
    fn full_thickness(&self) -> f32 {
        self.thickness.max(self.nail_thickness)
    }

    fn full_elongation(&self) -> f32 {
        self.elongation + self.nail_elongation
    }
}

impl Geometry<'_> {
    /// Before and after an idea's box on the way to its children: the
    /// margin, then 30 more (5 when vertical) for the links.
    fn gaps(&self) -> (f32, f32) {
        let m = self.skin.margin;
        (m, if self.vertical { m + 5.0 } else { m + 30.0 })
    }

    fn finger(
        &self,
        node: Option<&Node>,
        children: &[Idea],
        right: bool,
        family: Family,
    ) -> Result<Finger, Declined> {
        let (label, elongation, thickness) = match node {
            Some(node) => {
                let label = Label::new(node, self.skin, family)?;
                let (e, t) = self.phalanx(&label, right);
                (Some(label), e, t)
            }
            None => (None, 0.0, 0.0),
        };
        let children = children
            .iter()
            .map(|idea| self.finger(Some(&idea.node), &idea.children, right, family))
            .collect::<Result<Vec<_>, _>>()?;
        let (x1, x2) = self.gaps();
        let mut tetris = Tetris::default();
        for child in &children {
            tetris.add(if child.children.is_empty() {
                Tee::leaf(child.thickness, child.elongation)
            } else {
                Tee {
                    thickness1: child.thickness,
                    elongation1: child.elongation + x1,
                    thickness2: child.nail_thickness,
                    elongation2: x2 + child.nail_elongation,
                }
            });
        }
        tetris.balance();
        Ok(Finger {
            label,
            elongation,
            thickness,
            nail_thickness: tetris.height(),
            nail_elongation: tetris.width(),
            offsets: tetris.placed.iter().map(|p| p.y).collect(),
            children,
        })
    }

    /// The box of `label` with its margins, as elongation and thickness.
    fn phalanx(&self, label: &Label, right: bool) -> (f32, f32) {
        let skin = self.skin;
        let (before, after) = if right { (3.0, 0.0) } else { (0.0, 3.0) };
        let (w, h) = label.size(skin, before, after);
        let margin = if label.shape.spreads() {
            2.0 * skin.margin
        } else {
            0.0
        };
        if self.vertical {
            (h, w + margin)
        } else {
            (w, h + margin)
        }
    }

    /// Draws one map below `top`; the height it takes.
    fn draw_map(
        &self,
        canvas: &mut Canvas,
        map: &Map,
        family: Family,
        top: f32,
    ) -> Result<f32, Declined> {
        let (x1, x2) = self.gaps();
        // The root's box belongs to the right side; the left side draws
        // it only when it is alone.
        let left_alone = map.right.is_empty() && !map.left.is_empty();
        let right = (!left_alone)
            .then(|| self.finger(Some(&map.root), &map.right, true, family))
            .transpose()?;
        let left = (!map.left.is_empty())
            .then(|| {
                let root = left_alone.then_some(&map.root);
                self.finger(root, &map.left, false, family)
            })
            .transpose()?;
        let half = [&right, &left]
            .into_iter()
            .flatten()
            .map(|f| f.full_thickness() / 2.0)
            .fold(0.0, f32::max);
        let before = left.as_ref().map_or(0.0, |f| f.full_elongation() + x1 + x2);
        let after = right
            .as_ref()
            .map_or(0.0, |f| f.full_elongation() + x1 + x2);
        // The root: on the center line, `before` into the map.
        let origin = if self.vertical {
            (half, top + before)
        } else {
            (before, top + half)
        };
        for (finger, dir) in [(&right, 1.0), (&left, -1.0)] {
            if let Some(finger) = finger {
                self.draw_finger(canvas, finger, origin, dir);
            }
        }
        Ok(if self.vertical {
            before + after
        } else {
            2.0 * half
        })
    }

    /// A point `e` along the map and `t` across it from `origin`.
    fn at(&self, (ox, oy): (f32, f32), e: f32, t: f32) -> (f32, f32) {
        if self.vertical {
            (ox + t, oy + e)
        } else {
            (ox + e, oy + t)
        }
    }

    /// Draws `finger` with its box starting at `start` on its center line,
    /// growing in `dir` (1 or -1).
    fn draw_finger(&self, canvas: &mut Canvas, finger: &Finger, start: (f32, f32), dir: f32) {
        let skin = self.skin;
        if let Some(label) = &finger.label {
            // The box without its margins, which lie across the map.
            let margin = if label.shape.spreads() {
                skin.margin
            } else {
                0.0
            };
            let e0 = if dir > 0.0 { 0.0 } else { -finger.elongation };
            let corner = self.at(start, e0, margin - finger.thickness / 2.0);
            let before = if dir > 0.0 { 3.0 } else { 0.0 };
            label.draw(canvas, skin, corner, before);
        }
        let (x1, x2) = self.gaps();
        let p1 = self.at(start, dir * finger.elongation, 0.0);
        for (child, &offset) in finger.children.iter().zip(&finger.offsets) {
            let p2 = self.at(start, dir * (finger.elongation + x1 + x2), offset);
            self.draw_finger(canvas, child, p2, dir);
            self.link(canvas, p1, p2, dir);
        }
    }

    /// PlantUML's link: a short straight piece out of each box and a
    /// cubic curve between them.
    fn link(&self, canvas: &mut Canvas, p1: (f32, f32), p2: (f32, f32), dir: f32) {
        let (near, far) = if self.vertical {
            (3.0 * dir, 10.0 * dir)
        } else {
            (10.0 * dir, 25.0 * dir)
        };
        let along = |p: (f32, f32), d: f32| {
            if self.vertical {
                (p.0, p.1 + d)
            } else {
                (p.0 + d, p.1)
            }
        };
        let points = [
            p1,
            along(p1, near),
            along(p1, far),
            along(p2, -far),
            along(p2, -near),
            p2,
        ];
        let pt = |p: (f32, f32)| format!("{},{}", n(p.0), n(p.1));
        let d = format!(
            "M{} L{} C{} {} {} L{}",
            pt(points[0]),
            pt(points[1]),
            pt(points[2]),
            pt(points[3]),
            pt(points[4]),
            pt(points[5])
        );
        let bounds = points.iter().fold(
            (
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ),
            |(l, t, r, b), &(x, y)| (l.min(x), t.min(y), r.max(x), b.max(y)),
        );
        canvas.path(Layer::Lifelines, &d, bounds, None, Some(self.skin.link));
    }
}

/// A subtree as PlantUML packs it: the box (thickness 1 over elongation
/// 1), then its children (thickness 2 over elongation 2), both centered
/// on the same line.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tee {
    thickness1: f32,
    elongation1: f32,
    thickness2: f32,
    elongation2: f32,
}

impl Tee {
    fn leaf(thickness: f32, elongation: f32) -> Tee {
        Tee {
            thickness1: thickness,
            elongation1: elongation,
            thickness2: 0.0,
            elongation2: 0.0,
        }
    }

    fn half(&self) -> f32 {
        self.thickness1.max(self.thickness2) / 2.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Placed {
    tee: Tee,
    /// Its center line.
    y: f32,
}

/// PlantUML's packing of the children of an idea: each one goes below
/// the ones before, as high as the frontier they left allows, part by
/// part.
#[derive(Default)]
struct Tetris {
    frontier: Frontier,
    placed: Vec<Placed>,
}

impl Tetris {
    fn add(&mut self, tee: Tee) {
        let y = if self.placed.is_empty() {
            0.0
        } else {
            let e1 = tee.elongation1;
            let c1 = self.frontier.contact(0.0, e1);
            let c2 = self.frontier.contact(e1, e1 + tee.elongation2);
            (c1 + tee.thickness1 / 2.0).max(c2 + tee.thickness2 / 2.0)
        };
        self.placed.push(Placed { tee, y });
        let e1 = tee.elongation1;
        self.frontier.add(0.0, e1, y + tee.thickness1 / 2.0);
        if tee.elongation2 > 0.0 {
            self.frontier
                .add(e1, e1 + tee.elongation2, y + tee.thickness2 / 2.0);
        }
    }

    /// Centers the children on the idea's line.
    fn balance(&mut self) {
        if self.placed.is_empty() {
            return;
        }
        let mean = (self.min() + self.max()) / 2.0;
        for p in &mut self.placed {
            p.y -= mean;
        }
    }

    fn min(&self) -> f32 {
        self.placed
            .iter()
            .map(|p| p.y - p.tee.half())
            .fold(f32::INFINITY, f32::min)
    }

    fn max(&self) -> f32 {
        self.placed
            .iter()
            .map(|p| p.y + p.tee.half())
            .fold(f32::NEG_INFINITY, f32::max)
    }

    fn height(&self) -> f32 {
        if self.placed.is_empty() {
            0.0
        } else {
            self.max() - self.min()
        }
    }

    fn width(&self) -> f32 {
        self.placed
            .iter()
            .map(|p| p.tee.elongation1 + p.tee.elongation2)
            .fold(0.0, f32::max)
    }
}

/// The lowest edge so far over every stretch of the elongation: stripes
/// from left to right, covering everything.
struct Frontier {
    /// Start, end and the edge's place.
    stripes: Vec<(f32, f32, f32)>,
}

impl Default for Frontier {
    fn default() -> Self {
        Frontier {
            stripes: vec![(f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY)],
        }
    }
}

impl Frontier {
    /// The stripes `x1..x2` touches, as indexes.
    fn touched(&self, x1: f32, x2: f32) -> std::ops::Range<usize> {
        let first = self
            .stripes
            .iter()
            .position(|s| x1 < s.1)
            .unwrap_or(self.stripes.len() - 1);
        let last = self.stripes[first..]
            .iter()
            .position(|s| x2 <= s.1)
            .map_or(self.stripes.len() - 1, |i| first + i);
        first..last + 1
    }

    /// The lowest edge over `x1..x2`.
    fn contact(&self, x1: f32, x2: f32) -> f32 {
        self.stripes[self.touched(x1, x2)]
            .iter()
            .map(|s| s.2)
            .fold(f32::NEG_INFINITY, f32::max)
    }

    /// Lowers the edge over `x1..x2` to `value` where it is higher.
    fn add(&mut self, x1: f32, x2: f32, value: f32) {
        if x2 <= x1 {
            return;
        }
        let touched = self.touched(x1, x2);
        if touched.len() > 1 {
            let starts: Vec<f32> = self.stripes[touched].iter().skip(1).map(|s| s.0).collect();
            let mut x = x1;
            for start in starts {
                self.add(x, start, value);
                x = start;
            }
            self.add(x, x2, value);
            return;
        }
        let i = touched.start;
        let (start, end, old) = self.stripes[i];
        if value <= old {
            return;
        }
        let mut parts = Vec::with_capacity(3);
        if start != x1 {
            parts.push((start, x1, old));
        }
        parts.push((x1, x2, value));
        if x2 != end {
            parts.push((x2, end, old));
        }
        self.stripes.splice(i..=i, parts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tetris_tucks_a_short_child_under_a_long_one() {
        let mut t = Tetris::default();
        // A box 20 thick and 50 long with a column 100 thick behind it.
        t.add(Tee {
            thickness1: 20.0,
            elongation1: 50.0,
            thickness2: 100.0,
            elongation2: 100.0,
        });
        // A leaf shorter than the first box: it fits right below it, in
        // the shadow of the column.
        t.add(Tee::leaf(20.0, 40.0));
        // A long leaf has to go below the column.
        t.add(Tee::leaf(20.0, 200.0));
        let ys: Vec<f32> = t.placed.iter().map(|p| p.y).collect();
        assert_eq!(ys, [0.0, 20.0, 60.0]);
        t.balance();
        assert_eq!(t.height(), 120.0);
        assert_eq!(t.min(), -60.0);
        assert_eq!(t.width(), 200.0);
    }

    #[test]
    fn the_frontier_keeps_the_lowest_edge() {
        let mut f = Frontier::default();
        f.add(0.0, 10.0, 5.0);
        f.add(5.0, 20.0, 3.0);
        f.add(5.0, 20.0, 8.0);
        assert_eq!(f.contact(0.0, 4.0), 5.0);
        assert_eq!(f.contact(0.0, 30.0), 8.0);
        assert_eq!(f.contact(20.0, 30.0), f32::NEG_INFINITY);
        assert_eq!(f.contact(12.0, 12.0), 8.0);
    }
}
