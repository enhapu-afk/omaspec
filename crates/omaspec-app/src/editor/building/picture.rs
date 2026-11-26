//! What each source line sets down in the picture: its parts, each at
//! the line that writes it, and its lines; or, for a picture without parts,
//! how far down each line wrote.

use std::collections::HashMap;

use anyhow::Result;
use omaspec_render::export::{self, usvg};
use omaspec_uml::map::{Head, Map};

use super::LINE_HEIGHT;

/// The diagram as drawn, and what each source line sets down in it.
pub(super) struct Picture {
    /// The picture as drawn, for a page: its parts by their classes.
    pub(super) shown: String,
    /// The picture with each group of a part numbered ([`omaspec_uml::numbered`]),
    /// which the frames are made of.
    pub(super) svg: String,
    pub(super) size: (f32, f32),
    /// The color of its paper, which an empty head is filled with.
    pub(super) paper: String,
    /// Each part, in the order its groups first come in the picture.
    pub(super) parts: Vec<Part>,
    /// Each line of the map, at the line it draws itself at.
    pub(super) wires: Vec<WireAt>,
    /// How far down each source line has drawn, by line: what a picture
    /// without parts is uncovered to.
    pub(super) reach: Vec<(usize, f32)>,
    /// Its drawer tied nothing to its lines: it comes whole with the line
    /// that closes the diagram, and nothing of it shows before.
    pub(super) untied: bool,
    /// It builds part by part ([`omaspec_uml::glides_by_parts`]).
    pub(super) glides: bool,
    /// The transform the lines' group draws in, to draw the lines anew in
    /// its place.
    pub(super) lines_group: Option<usvg::Transform>,
    /// How the picture's own units (its `viewBox`) land in its size: a
    /// `scale` in the source draws it larger or smaller than its units.
    pub(super) root: usvg::Transform,
}

pub(super) struct Part {
    pub(super) name: String,
    pub(super) line: usize,
    pub(super) middle: (f32, f32),
    /// The ids of its groups, one a layer, each with the transform it is
    /// drawn in.
    pub(super) groups: Vec<(String, usvg::Transform)>,
    /// The words of this line of the map: they come after it is drawn.
    pub(super) of_wire: Option<usize>,
}

pub(super) struct WireAt {
    pub(super) line: usize,
    pub(super) points: Vec<(f32, f32)>,
    pub(super) heads: Vec<Head>,
    pub(super) color: String,
    pub(super) width: f32,
    pub(super) dash: Option<(f32, f32)>,
}

impl Picture {
    pub(super) fn of(svg: &str, map: &Map, text: &str) -> Result<Picture> {
        let shown = svg;
        // Each group of a part with an id of its own, which a frame finds
        // it by: usvg keeps no classes (docs/adr/0037).
        let svg = &omaspec_uml::numbered(svg);
        let tree = export::parse(svg).map_err(|err| anyhow::anyhow!("{err}"))?;
        let size = (tree.size().width(), tree.size().height());
        let mut found: Vec<(String, String, usvg::Transform, usvg::Rect)> = Vec::new();
        let mut lines_group = None;
        visit(tree.root(), &mut |group| {
            let id = group.id();
            if id == omaspec_uml::LINES {
                lines_group = Some(group.abs_transform());
            }
            if let Some(name) = omaspec_uml::part_of_id(id) {
                let bounds = group.abs_layer_bounding_box().to_rect();
                found.push((name, id.to_owned(), group.abs_transform(), bounds));
            }
        });
        // One part a name, its groups together and its bounds their union.
        let mut parts: Vec<Part> = Vec::new();
        let mut bounds: Vec<usvg::Rect> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for (name, id, transform, rect) in found {
            match index.get(&name) {
                Some(&at) => {
                    parts[at].groups.push((id, transform));
                    bounds[at] = union(bounds[at], rect);
                }
                None => {
                    index.insert(name.clone(), parts.len());
                    parts.push(Part {
                        name,
                        line: 0,
                        middle: (0.0, 0.0),
                        groups: vec![(id, transform)],
                        of_wire: None,
                    });
                    bounds.push(rect);
                }
            }
        }
        let words_of: HashMap<&str, usize> = map
            .wires
            .iter()
            .enumerate()
            .flat_map(|(at, wire)| wire.words.iter().flatten().map(move |w| (w.as_str(), at)))
            .collect();
        let source: Vec<&str> = text.split('\n').collect();
        let mut known: Vec<Option<usize>> = Vec::with_capacity(parts.len());
        for (part, rect) in parts.iter_mut().zip(&bounds) {
            part.middle = (
                rect.x() + rect.width() / 2.0,
                rect.y() + rect.height() / 2.0,
            );
            part.of_wire = words_of.get(part.name.as_str()).copied();
            known.push(line_of_part(&part.name, *rect, map, &source));
        }
        // A part no line writes (an activity's end, a bar, a merge) comes
        // with the nearest box one does.
        for (at, line) in known.iter().enumerate() {
            parts[at].line = match line {
                Some(line) => *line,
                None => nearest_line(&parts, &known, at).unwrap_or(0),
            };
        }
        let wires: Vec<WireAt> = map
            .wires
            .iter()
            .enumerate()
            .map(|(at, wire)| {
                let words = parts
                    .iter()
                    .filter(|part| part.of_wire == Some(at))
                    .map(|part| part.line)
                    .min();
                let ends = [wire.points.first(), wire.points.last()]
                    .into_iter()
                    .flatten()
                    .filter_map(|&end| line_at(&parts, end))
                    .max();
                WireAt {
                    line: wire.line.or(words).or(ends).unwrap_or(0),
                    points: wire.points.clone(),
                    heads: wire.heads.clone(),
                    color: wire.stroke.color.to_string(),
                    width: wire.stroke.width,
                    dash: wire.stroke.dash,
                }
            })
            .collect();
        // A line's words come with it.
        for part in &mut parts {
            if let Some(wire) = part.of_wire.and_then(|at| wires.get(at)) {
                part.line = wire.line;
            }
        }
        // How far down each line wrote: its words and its messages; not its
        // shapes, a sequence's box reaching down the whole picture.
        let mut reach: HashMap<usize, f32> = HashMap::new();
        for (line, bottom) in map
            .tied
            .iter()
            .map(|t| (t.line, t.rect.y + t.rect.h))
            .chain(
                map.messages
                    .iter()
                    .filter(|_| map.run == omaspec_uml::map::Run::Down)
                    .map(|step| (step.line, step.bottom)),
            )
        {
            let at = reach.entry(line).or_insert(bottom);
            *at = at.max(bottom);
        }
        let mut reach: Vec<(usize, f32)> = reach.into_iter().collect();
        reach.sort_by_key(|&(line, _)| line);
        // A drawer that ties no words to their lines says nothing of
        // what each line drew: the picture comes whole as the line that
        // closes the diagram is written, rather than all of it but its
        // foot from the first line on, a row cut in half to the end
        // (REQ-142).
        let boxes = parts
            .iter()
            .any(|part| !omaspec_uml::FRAME_PARTS.contains(&part.name.as_str()));
        let untied = reach.is_empty() && !boxes;
        if untied {
            reach.push((closing_line(text), size.1));
        }
        Ok(Picture {
            svg: svg.to_owned(),
            shown: shown.to_owned(),
            size,
            paper: paper_of(svg),
            parts,
            wires,
            reach,
            untied,
            glides: omaspec_uml::glides_by_parts(shown),
            lines_group,
            root: root_of(svg, size),
        })
    }

    /// A point of the picture's size in its own units.
    pub(super) fn own(&self, (x, y): (f32, f32)) -> (f32, f32) {
        let mut point = usvg::tiny_skia_path::Point::from_xy(x, y);
        if let Some(back) = self.root.invert() {
            back.map_point(&mut point);
        }
        (point.x, point.y)
    }

    /// Whether it builds part by part, else from the top down: it has
    /// boxes of its own, not only its frame's parts (a titled timing
    /// chart), and they glide (not a sequence's, drawn as a whole).
    pub(super) fn by_parts(&self) -> bool {
        self.glides
    }

    /// Down to where the picture shows before any line: above the first
    /// thing a line wrote, the heads of a sequence.
    pub(super) fn first_reach(&self) -> f32 {
        if self.untied {
            return 0.0;
        }
        let top = self
            .reach
            .iter()
            .map(|&(_, bottom)| bottom)
            .fold(self.size.1, f32::min);
        (top - LINE_HEIGHT).max(0.0)
    }

    /// Whether `line` sets down something: a box, a line or, from the top
    /// down, words.
    pub(super) fn draws(&self, line: usize) -> (bool, bool) {
        if self.by_parts() {
            let boxes = self
                .parts
                .iter()
                .any(|part| part.line == line && part.of_wire.is_none());
            let wires = self.wires.iter().any(|wire| wire.line == line);
            (boxes, wires)
        } else {
            (self.reach.iter().any(|&(at, _)| at == line), false)
        }
    }
}

/// Calls `each` with every group of `group`'s tree, it included.
pub(super) fn visit(group: &usvg::Group, each: &mut dyn FnMut(&usvg::Group)) {
    each(group);
    for node in group.children() {
        if let usvg::Node::Group(inner) = node {
            visit(inner, each);
        }
    }
}

pub(super) fn union(a: usvg::Rect, b: usvg::Rect) -> usvg::Rect {
    let (left, top) = (a.left().min(b.left()), a.top().min(b.top()));
    let (right, bottom) = (a.right().max(b.right()), a.bottom().max(b.bottom()));
    usvg::Rect::from_ltrb(left, top, right, bottom).unwrap_or(a)
}

/// The fill of the picture's paper, the rectangle before its first
/// group; white without one, as the engine fills an empty head then.
pub(in crate::editor) fn paper_of(svg: &str) -> String {
    let rect = svg.find("<rect ");
    let group = svg.find("<g");
    rect.filter(|&rect| group.is_none_or(|group| rect < group))
        .and_then(|rect| {
            let tag = &svg[rect..rect + svg[rect..].find('>')?];
            tag.split(" fill=\"").nth(1)?.split('"').next()
        })
        .filter(|fill| fill.starts_with('#'))
        .unwrap_or("#FFFFFF")
        .to_owned()
}

/// How the units of `svg`'s `viewBox` land in its `size`, as SVG fits
/// them by default: as large as they fit, in the middle.
pub(in crate::editor) fn root_of(svg: &str, (width, height): (f32, f32)) -> usvg::Transform {
    let view = svg.find("<svg").and_then(|start| {
        let tag = &svg[start..start + svg[start..].find('>')?];
        let view = tag.split(" viewBox=\"").nth(1)?.split('"').next()?;
        let numbers: Vec<f32> = view
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|n| !n.is_empty())
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        match numbers[..] {
            [x, y, w, h] if w > 0.0 && h > 0.0 => Some((x, y, w, h)),
            _ => None,
        }
    });
    let Some((x, y, w, h)) = view else {
        return usvg::Transform::identity();
    };
    let scale = (width / w).min(height / h);
    usvg::Transform::from_row(
        scale,
        0.0,
        0.0,
        scale,
        (width - w * scale) / 2.0 - x * scale,
        (height - h * scale) / 2.0 - y * scale,
    )
}

/// The line of the part `name`, standing in `bounds`: a state's first
/// line, the first line whose words are drawn in it, else the first that
/// names the box drawn there.
pub(super) fn line_of_part(
    name: &str,
    bounds: usvg::Rect,
    map: &Map,
    source: &[&str],
) -> Option<usize> {
    if let Some(spot) = map.states.iter().find(|spot| spot.part == name) {
        return Some(spot.line);
    }
    // A part one line writes alone, as a git graph's commit, whose lines
    // reach past the words of others.
    if let Some((_, line)) = map.written.iter().find(|(part, _)| part == name) {
        return Some(*line);
    }
    let inside = |x: f32, y: f32| {
        x >= bounds.left() - 0.5
            && x <= bounds.right() + 0.5
            && y >= bounds.top() - 0.5
            && y <= bounds.bottom() + 0.5
    };
    let written = map
        .tied
        .iter()
        .chain(&map.shapes)
        .filter(|tied| {
            inside(
                tied.rect.x + tied.rect.w / 2.0,
                tied.rect.y + tied.rect.h / 2.0,
            )
        })
        .map(|tied| tied.line)
        .min();
    // The box drawn there, by its part, else the one whose middle and size
    // come nearest; the first line that names it may come before its
    // words (`class B {` before its members).
    let area = bounds.width() * bounds.height();
    let place = map
        .places
        .iter()
        .find(|place| place.part.as_deref() == Some(name))
        .or_else(|| {
            map.places
                .iter()
                .filter(|place| {
                    inside(
                        place.rect.x + place.rect.w / 2.0,
                        place.rect.y + place.rect.h / 2.0,
                    )
                })
                .min_by(|a, b| {
                    let off = |p: &&omaspec_uml::map::Place| (p.rect.w * p.rect.h - area).abs();
                    off(a).total_cmp(&off(b))
                })
        });
    let named = place.and_then(|place| named_at(&place.name, source));
    match (written, named) {
        (Some(written), Some(named)) => Some(written.min(named)),
        (written, named) => written.or(named),
    }
}

/// The line of the box nearest part `at` whose line is `known`, its
/// words' parts left out.
pub(super) fn nearest_line(parts: &[Part], known: &[Option<usize>], at: usize) -> Option<usize> {
    let here = parts[at].middle;
    parts
        .iter()
        .zip(known)
        .enumerate()
        .filter(|&(other, (part, line))| other != at && line.is_some() && part.of_wire.is_none())
        .min_by(|(_, (a, _)), (_, (b, _))| {
            let d = |p: &Part| (p.middle.0 - here.0).hypot(p.middle.1 - here.1);
            d(a).total_cmp(&d(b))
        })
        .and_then(|(_, (_, line))| *line)
}

/// The first line of `source` that names `name` as a word of its own.
pub(super) fn named_at(name: &str, source: &[&str]) -> Option<usize> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    // Words about the diagram, not a box of it: a title, a caption, a
    // header or a footer, a legend, a note, a comment.
    let about = |line: &str| {
        let line = line.trim_start().to_ascii_lowercase();
        [
            "title", "caption", "header", "footer", "legend", "note", "'", "%%", "@start",
        ]
        .iter()
        .any(|start| line.starts_with(start))
    };
    if name.is_empty() {
        return None;
    }
    source.iter().position(|line| {
        !about(line)
            && line.match_indices(name).any(|(at, _)| {
                let before = line[..at].chars().next_back().is_none_or(|c| !word(c));
                let after = line[at + name.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !word(c));
                before && after
            })
    })
}

/// The line of the box a line's end at `at` meets: the nearest one.
pub(super) fn line_at(parts: &[Part], at: (f32, f32)) -> Option<usize> {
    parts
        .iter()
        .filter(|part| part.of_wire.is_none())
        .min_by(|a, b| {
            let d = |p: &Part| (p.middle.0 - at.0).hypot(p.middle.1 - at.1);
            d(a).total_cmp(&d(b))
        })
        .map(|part| part.line)
}

/// The zero-based line that closes the diagram of `text`: its first
/// `@end…`, else its last line that is neither blank nor a comment.
fn closing_line(text: &str) -> usize {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let end = lines.iter().position(|line| {
        line.get(..4)
            .is_some_and(|head| head.eq_ignore_ascii_case("@end"))
    });
    end.or_else(|| {
        lines.iter().rposition(|line| {
            !line.is_empty() && !line.starts_with('\'') && !line.starts_with("%%")
        })
    })
    .unwrap_or(0)
}
