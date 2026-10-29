//! Notes, after the container has been mirrored.

use mermaid_rs_renderer::StateNotePosition;
use omaspec_uml::graph::Rect;
use omaspec_uml::{Block, Family, Font};

use super::label;
use super::{Member, NOTE_GAP, NOTE_PAD, Palette, PlacedEdge, PlacedNote, Scene, Spot, Tree, slot};

fn label_box(edge: &PlacedEdge) -> (f32, f32, f32, f32) {
    (
        edge.label_at.0,
        edge.label_at.1,
        edge.label.width,
        edge.label.height,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

struct Draft {
    target: usize,
    side: Side,
    w: f32,
    h: f32,
    text: Block,
}

/// The room the notes beside each member need, left and right, gap
/// included. The layout keeps it free when the states run down.
pub(super) fn note_room(
    tree: &Tree,
    spot: Spot,
    members: &[Member],
    palette: &Palette,
    family: Family,
) -> Option<Vec<(f32, f32)>> {
    let mut room = vec![(0.0f32, 0.0f32); members.len()];
    for note in &tree.graph.state_notes {
        if tree.home(&note.target) != spot {
            continue;
        }
        let text = label::measure(
            &note.label,
            Font::new(palette.size, palette.text),
            family,
            None,
        );
        let side = room.get_mut(slot(members, &note.target, tree.graph)?)?;
        let need = NOTE_GAP + note_size(&text).0;
        match note.position {
            StateNotePosition::LeftOf => side.0 = side.0.max(need),
            StateNotePosition::RightOf => side.1 = side.1.max(need),
        }
    }
    Some(room)
}

/// A note's sheet around its text.
fn note_size(text: &Block) -> (f32, f32) {
    (
        (text.width + NOTE_PAD * 2.0 + 4.0).max(18.0),
        (text.height + NOTE_PAD * 2.0).max(18.0),
    )
}

pub(super) fn place_notes(
    tree: &Tree,
    spot: Spot,
    members: &[Member],
    scene: &Scene,
    palette: &Palette,
    family: Family,
) -> Option<Vec<PlacedNote>> {
    let mut sized = Vec::new();
    for note in &tree.graph.state_notes {
        if tree.home(&note.target) != spot {
            continue;
        }
        let text = label::measure(
            &note.label,
            Font::new(palette.size, palette.text),
            family,
            None,
        );
        sized.push(Draft {
            target: slot(members, &note.target, tree.graph)?,
            side: match note.position {
                StateNotePosition::RightOf => Side::Right,
                StateNotePosition::LeftOf => Side::Left,
            },
            w: note_size(&text).0,
            h: note_size(&text).1,
            text,
        });
    }
    let rects: Vec<Rect> = scene.nodes.iter().map(|node| node.rect).collect();
    let mut groups: Vec<(usize, Side, Vec<usize>)> = Vec::new();
    for (index, note) in sized.iter().enumerate() {
        if let Some(group) = groups
            .iter_mut()
            .find(|group| group.0 == note.target && group.1 == note.side)
        {
            group.2.push(index);
        } else {
            groups.push((note.target, note.side, vec![index]));
        }
    }
    let mut placed = Vec::with_capacity(sized.len());
    for (target, side, indices) in groups {
        let rect = note_anchor(rects.get(target).copied()?, &scene.edges, target);
        let mut cursor = rect.y;
        for index in indices {
            let note = &mut sized[index];
            let (x, y) = match side {
                Side::Right => (rect.x + rect.w + NOTE_GAP, cursor),
                Side::Left => (rect.x - NOTE_GAP - note.w, cursor),
            };
            cursor += note.h + 6.0;
            placed.push(PlacedNote {
                x,
                y,
                w: note.w,
                h: note.h,
                text: std::mem::replace(&mut note.text, Block::empty()),
            });
            let last = placed.len() - 1;
            clear_note(&mut placed[last], side, &scene.edges, &rects);
        }
    }
    Some(placed)
}

fn note_anchor(rect: Rect, edges: &[PlacedEdge], target: usize) -> Rect {
    let mut x0 = rect.x;
    let mut y0 = rect.y;
    let mut x1 = rect.x + rect.w;
    let mut y1 = rect.y + rect.h;
    for edge in edges {
        if edge.from != target || edge.to != target {
            continue;
        }
        for &(x, y) in &edge.points {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        if edge.label.width > 0.0 || edge.label.height > 0.0 {
            x0 = x0.min(edge.label_at.0);
            y0 = y0.min(edge.label_at.1);
            x1 = x1.max(edge.label_at.0 + edge.label.width);
            y1 = y1.max(edge.label_at.1 + edge.label.height);
        }
    }
    Rect {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0.0),
        h: (y1 - y0).max(0.0),
    }
}

fn clear_note(note: &mut PlacedNote, side: Side, edges: &[PlacedEdge], rects: &[Rect]) {
    for _ in 0..6 {
        let text = note_text_box(note);
        let Some(label) = edges.iter().find_map(|edge| {
            if edge.label.is_empty() {
                return None;
            }
            let label = label_box(edge);
            boxes_overlap(text, label).then_some(label)
        }) else {
            break;
        };
        let (dx, dy) = note_dodge(side, note, text, label, rects);
        if dx == 0.0 && dy == 0.0 {
            break;
        }
        note.x += dx;
        note.y += dy;
    }
}

fn note_text_box(note: &PlacedNote) -> (f32, f32, f32, f32) {
    (
        note.x + NOTE_PAD,
        note.y + NOTE_PAD,
        note.text.width,
        note.text.height,
    )
}

pub(super) fn boxes_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    let (x, y, w, h) = a;
    let (ox, oy, ow, oh) = b;
    x < ox + ow && ox < x + w && y < oy + oh && oy < y + h
}

fn note_dodge(
    side: Side,
    note: &PlacedNote,
    text: (f32, f32, f32, f32),
    label: (f32, f32, f32, f32),
    rects: &[Rect],
) -> (f32, f32) {
    const GAP: f32 = 4.0;
    let (x, y, w, h) = text;
    let (lx, ly, lw, lh) = label;
    let up = (0.0, -(y + h - ly + GAP));
    let down = (0.0, ly + lh - y + GAP);
    let left = (-(x + w - lx + GAP), 0.0);
    let right = (lx + lw - x + GAP, 0.0);
    let steps = match side {
        Side::Right => [up, down, right],
        Side::Left => [up, down, left],
    };
    let mut best = steps[0];
    let mut best_area = f32::MAX;
    let mut best_len = f32::MAX;
    for (dx, dy) in steps {
        let moved = (note.x + dx, note.y + dy, note.w, note.h);
        let area = rects
            .iter()
            .map(|rect| overlap_area(moved, (rect.x, rect.y, rect.w, rect.h)))
            .sum::<f32>();
        let len = dx.abs() + dy.abs();
        if area < best_area - 0.5 || ((area - best_area).abs() <= 0.5 && len < best_len) {
            best = (dx, dy);
            best_area = area;
            best_len = len;
        }
    }
    best
}

fn overlap_area(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let (x, y, w, h) = a;
    let (ox, oy, ow, oh) = b;
    let width = (x + w).min(ox + ow) - x.max(ox);
    let height = (y + h).min(oy + oh) - y.max(oy);
    if width > 0.0 && height > 0.0 {
        width * height
    } else {
        0.0
    }
}
