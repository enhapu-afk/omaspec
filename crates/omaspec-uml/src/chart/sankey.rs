//! A sankey (`sankey-beta`): what flows from where to where, a line of
//! `source,target,value` each. The nodes stand in columns by how far
//! they are from where the flows start, each as tall as what passes
//! through it; a flow is a band as wide as its value, in the color of
//! where it comes from, let through to what lies under it. A node's name
//! stands right of it, in the last column left of it; the columns stand
//! as far apart as their names need, so no name runs over a node or
//! another name.

use super::{Look, body, number, said, title, words};
use crate::Declined;
use crate::svg::{Align, Canvas, Layer, n};
use crate::text::Font;

const WIDTH: f32 = 640.0;
const HEIGHT: f32 = 380.0;
/// A node's width, and the room between two nodes of a column.
const NODE: f32 = 12.0;
const GAP: f32 = 14.0;
/// From a node to its name's backing, the backing's margin round the
/// words, and the least room between a name and the next node.
const LABEL_OFF: f32 = 3.0;
const LABEL_PAD: f32 = 3.0;
const CLEAR: f32 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<String>,
    pub nodes: Vec<String>,
    /// From, to, how much.
    pub flows: Vec<(usize, usize, f64)>,
}

/// The sankey of `source`; `None` when it is none this drawer reads, or
/// its flows run in a circle.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    let (title, lines) = body(source, "sankey-beta").or_else(|| body(source, "sankey"))?;
    let mut nodes: Vec<String> = Vec::new();
    let mut flows = Vec::new();
    let index = |name: String, nodes: &mut Vec<String>| match nodes.iter().position(|n| *n == name)
    {
        Some(at) => at,
        None => {
            nodes.push(name);
            nodes.len() - 1
        }
    };
    for (_, line) in lines {
        let fields = csv(line)?;
        let [from, to, value] = <[String; 3]>::try_from(fields).ok()?;
        let value = number(&value).filter(|v| *v >= 0.0)?;
        let from = index(from, &mut nodes);
        let to = index(to, &mut nodes);
        if from == to {
            return None;
        }
        flows.push((from, to, value));
    }
    let d = Diagram {
        title,
        nodes,
        flows,
    };
    (!d.flows.is_empty() && columns(&d).is_some()).then_some(d)
}

/// Each node's column: how many flows lead to it at most from a node
/// nothing flows into. `None` for flows in a circle.
fn columns(d: &Diagram) -> Option<Vec<usize>> {
    let mut column = vec![0usize; d.nodes.len()];
    for _ in 0..=d.nodes.len() {
        let mut moved = false;
        for &(from, to, _) in &d.flows {
            if column[to] < column[from] + 1 {
                column[to] = column[from] + 1;
                moved = true;
            }
        }
        if !moved {
            return Some(column);
        }
    }
    None
}

pub fn draw(d: &Diagram, look: &Look) -> Result<String, Declined> {
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let column = columns(d).unwrap_or_else(|| vec![0; d.nodes.len()]);
    let last = column.iter().copied().max().unwrap_or(0).max(1);
    let through: Vec<f64> = (0..d.nodes.len())
        .map(|i| {
            let into: f64 = d.flows.iter().filter(|f| f.1 == i).map(|f| f.2).sum();
            let out: f64 = d.flows.iter().filter(|f| f.0 == i).map(|f| f.2).sum();
            into.max(out)
        })
        .collect();
    // One scale for all columns: the fullest fits.
    let mut scale = f64::MAX;
    for c in 0..=last {
        let members: Vec<usize> = (0..d.nodes.len()).filter(|&i| column[i] == c).collect();
        let sum: f64 = members.iter().map(|&i| through[i]).sum();
        if sum > 0.0 {
            let room = HEIGHT as f64 - GAP as f64 * (members.len().saturating_sub(1)) as f64;
            scale = scale.min(room.max(10.0) / sum);
        }
    }
    if scale == f64::MAX {
        scale = 1.0;
    }
    // Where each node stands down the page, from the top of the columns,
    // and its name.
    let font = Font::new(look.size - 1.0, look.ink);
    let mut down = vec![(0.0f32, 0.0f32); d.nodes.len()];
    for c in 0..=last {
        let mut y = 0.0;
        for i in (0..d.nodes.len()).filter(|&i| column[i] == c) {
            let h = (through[i] * scale) as f32;
            down[i] = (y, h);
            y += h + GAP;
        }
    }
    let labels: Vec<_> = d
        .nodes
        .iter()
        .enumerate()
        .map(|(i, name)| words(&format!("{name} {}", said(through[i])), font, family))
        .collect();
    let label_y = |i: usize| down[i].0 + (down[i].1 - labels[i].height) / 2.0;
    // Across: each gap between two columns as wide as the names in it.
    let room = |i: usize| labels[i].width + 2.0 * LABEL_PAD + LABEL_OFF;
    let even = (WIDTH - NODE) / last as f32 - NODE;
    let mut x_of = vec![0.0f32; last + 1];
    for c in 0..last {
        let right: Vec<usize> = (0..d.nodes.len()).filter(|&i| column[i] == c).collect();
        let left: Vec<usize> = if c + 1 == last {
            (0..d.nodes.len()).filter(|&i| column[i] == last).collect()
        } else {
            Vec::new()
        };
        let mut need = right
            .iter()
            .chain(&left)
            .map(|&i| room(i))
            .fold(0.0, f32::max);
        // A name on each side of the gap, at one height, share it.
        for &r in &right {
            for &l in &left {
                let (ra, la) = (label_y(r), label_y(l));
                if ra < la + labels[l].height && la < ra + labels[r].height {
                    need = need.max(room(r) + room(l));
                }
            }
        }
        x_of[c + 1] = x_of[c] + NODE + even.max(need + CLEAR);
    }
    let width = x_of[last] + NODE;
    let top = title(&mut canvas, d.title.as_deref(), look, width / 2.0);
    let place: Vec<(f32, f32, f32)> = (0..d.nodes.len())
        .map(|i| (x_of[column[i]], top + down[i].0, down[i].1))
        .collect();
    // The bands, stacked in the order written at both ends.
    let mut out_at: Vec<f32> = place.iter().map(|p| p.1).collect();
    let mut in_at: Vec<f32> = place.iter().map(|p| p.1).collect();
    for &(from, to, value) in &d.flows {
        let w = (value * scale) as f32;
        let (x0, x1) = (place[from].0 + NODE, place[to].0);
        let (a, b) = (out_at[from], in_at[to]);
        out_at[from] += w;
        in_at[to] += w;
        let mid = (x0 + x1) / 2.0;
        let path = format!(
            "M{} {}C{} {} {} {} {} {}L{} {}C{} {} {} {} {} {}Z",
            n(x0),
            n(a),
            n(mid),
            n(a),
            n(mid),
            n(b),
            n(x1),
            n(b),
            n(x1),
            n(b + w),
            n(mid),
            n(b + w),
            n(mid),
            n(a + w),
            n(x0),
            n(a + w)
        );
        canvas.glaze(
            Layer::Background,
            &path,
            (x0, a.min(b), x1, (a + w).max(b + w)),
            look.color(from),
            0.4,
        );
    }
    // The nodes over them, and what they are called.
    for (i, label) in labels.iter().enumerate() {
        let (x, y, h) = place[i];
        let color = look.color(i);
        canvas.rect(
            Layer::Frames,
            (x, y, NODE, h.max(1.0)),
            0.0,
            Some(color),
            // Drawn by hand, a node has its line.
            look.sketch.then(|| crate::svg::Stroke::new(color, 1.0)),
        );
        let ly = y + (h - label.height) / 2.0;
        let (lx, align, from) = if column[i] == last {
            (
                x - LABEL_OFF - LABEL_PAD,
                Align::Right,
                x - LABEL_OFF - 2.0 * LABEL_PAD - label.width,
            )
        } else {
            (
                x + NODE + LABEL_OFF + LABEL_PAD,
                Align::Left,
                x + NODE + LABEL_OFF,
            )
        };
        // On the page's color, so it reads over the bands under it.
        canvas.rect(
            Layer::Bars,
            (from, ly, label.width + 2.0 * LABEL_PAD, label.height),
            3.0,
            Some(look.ground()),
            None,
        );
        canvas.text(Layer::Front, label, (lx, ly), align);
    }
    Ok(canvas.finish(10.0, look.page))
}

/// A line of comma-separated values, a value in quotes keeping its commas
/// (`""` a quote in it).
fn csv(line: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut field).trim().to_owned()),
            c => field.push(c),
        }
    }
    if quoted {
        return None;
    }
    out.push(field.trim().to_owned());
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flows_stand_in_columns_by_their_distance() {
        let d = from_mermaid(
            "sankey-beta\nSolar,Grid,60\nWind,Grid,40\nGrid,Homes,55\nGrid,\"Industry, heavy\",45\n",
        )
        .unwrap();
        assert_eq!(
            d.nodes,
            ["Solar", "Grid", "Wind", "Homes", "Industry, heavy"]
        );
        assert_eq!(columns(&d).unwrap(), [0, 1, 0, 2, 2]);
        let svg = draw(&d, &Look::default()).unwrap();
        assert!(
            svg.contains(">Grid 100<") && svg.contains("fill-opacity"),
            "{svg}"
        );
        assert!(
            from_mermaid("sankey-beta\na,b,1\nb,a,1\n").is_none(),
            "a circle"
        );
        assert!(from_mermaid("sankey-beta\na,b\n").is_none());
    }

    /// The rects of `svg`: its nodes, `NODE` wide, and the backings of
    /// its names, with round corners.
    fn rects(svg: &str) -> (Vec<[f32; 4]>, Vec<[f32; 4]>) {
        let (mut nodes, mut names) = (Vec::new(), Vec::new());
        for tag in svg.split("<rect ").skip(1) {
            let tag = &tag[..tag.find('>').unwrap()];
            let value = |name: &str| -> f32 {
                tag.split_whitespace()
                    .find_map(|pair| pair.strip_prefix(name)?.strip_prefix("=\""))
                    .and_then(|rest| rest.trim_end_matches(['"', '/']).parse().ok())
                    .unwrap()
            };
            let rect = [value("x"), value("y"), value("width"), value("height")];
            if tag.contains("rx=") {
                names.push(rect);
            } else if rect[2] == NODE {
                nodes.push(rect);
            }
        }
        (nodes, names)
    }

    fn cross(a: [f32; 4], b: [f32; 4]) -> bool {
        a[0] < b[0] + b[2] - 0.5
            && b[0] < a[0] + a[2] - 0.5
            && a[1] < b[1] + b[3] - 0.5
            && b[1] < a[1] + a[3] - 0.5
    }

    #[test]
    fn a_name_runs_over_no_node_and_no_other_name() {
        let d = from_mermaid(
            "sankey-beta\nCoal and lignite mining,Power stations east,500\nNatural gas imports,Power stations east,300\nPower stations east,Transmission grid north,700\nPower stations east,Conversion losses,100\nTransmission grid north,Regional distribution,650\nTransmission grid north,Line losses,50\nRegional distribution,Households and small business,400\nRegional distribution,Industrial customers,250\n",
        )
        .unwrap();
        let svg = draw(&d, &Look::default()).unwrap();
        let (nodes, names) = rects(&svg);
        assert_eq!(nodes.len(), d.nodes.len());
        assert_eq!(names.len(), d.nodes.len());
        for (i, name) in names.iter().enumerate() {
            for node in &nodes {
                assert!(!cross(*name, *node), "name {name:?} over node {node:?}");
            }
            for other in &names[i + 1..] {
                assert!(!cross(*name, *other), "{name:?} over {other:?}");
            }
        }
        // Short names keep the page's width.
        let short = from_mermaid("sankey-beta\nA,B,5\nB,C,3\nB,D,2\n").unwrap();
        let svg = draw(&short, &Look::default()).unwrap();
        let (nodes, _) = rects(&svg);
        let right = nodes.iter().map(|r| r[0] + r[2]).fold(0.0, f32::max);
        assert!((right - WIDTH).abs() < 0.5, "{right}");
    }
}
