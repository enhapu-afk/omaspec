//! Reading a ditaa drawing: the characters into a grid, the boxes found in
//! it, then the lines and heads among what is left, then the text.

use super::{Box, Diagram, Point, Shape};
use crate::Declined;
use crate::color::Color;
use crate::common::Frame;
use crate::source::Line;

/// The most cells a drawing may have.
const MAX_CELLS: usize = 200_000;

fn horizontal(c: char) -> bool {
    matches!(c, '-' | '=')
}

fn vertical(c: char) -> bool {
    matches!(c, '|' | ':')
}

fn corner(c: char) -> bool {
    matches!(c, '+' | '/' | '\\')
}

fn line_char(c: char) -> bool {
    horizontal(c) || vertical(c) || corner(c)
}

pub fn parse(lines: &[Line], source: &str) -> Result<Diagram, Declined> {
    let mut frame = Frame::default();
    let mut rows: Vec<Vec<char>> = Vec::new();
    let mut first: Option<usize> = None;
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        // Only before the drawing: a title or a caption.
        if rows.is_empty()
            && let Some(next) = frame.line(lines, at)?
        {
            at = next;
            continue;
        }
        let first_no = *first.get_or_insert(line.no);
        while rows.len() < line.no - first_no {
            rows.push(Vec::new());
        }
        let mut row: Vec<char> = " ".repeat(line.indent).chars().collect();
        row.extend(line.text.chars().map(|c| if c == '\t' { ' ' } else { c }));
        rows.push(row);
        at += 1;
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    if rows.len() * width > MAX_CELLS {
        return Err(Declined::at(0, "this drawing is too large"));
    }
    for row in &mut rows {
        row.resize(width, ' ');
    }
    let shadows = !source
        .lines()
        .find(|l| {
            l.trim_start()
                .to_ascii_lowercase()
                .starts_with("@startditaa")
        })
        .is_some_and(|head| head.contains("--no-shadows") || head.contains("-S"));
    let mut d = Diagram {
        boxes: Vec::new(),
        lines: Vec::new(),
        heads: Vec::new(),
        texts: Vec::new(),
        shadows,
        frame,
    };
    if rows.is_empty() {
        return Err(Declined {
            line: None,
            reason: "a ditaa diagram needs a drawing".into(),
            refused: true,
        });
    }
    let g = Grid { rows };
    let mut border = vec![vec![false; width]; g.rows.len()];
    find_boxes(&g, &mut d, &mut border);
    // What is written inside a box and is a code of it: its color and shape.
    let mut used = vec![vec![false; width]; g.rows.len()];
    codes(&g, &mut d, &mut used);
    connect(&g, &mut d, &border, &mut used);
    text(&g, &mut d, &border, &used);
    Ok(d)
}

struct Grid {
    rows: Vec<Vec<char>>,
}

impl Grid {
    fn at(&self, r: isize, c: isize) -> char {
        if r < 0 || c < 0 {
            return ' ';
        }
        self.rows
            .get(r as usize)
            .and_then(|row| row.get(c as usize))
            .copied()
            .unwrap_or(' ')
    }
}

/// Every rectangle whose sides are drawn, the smallest from each corner.
fn find_boxes(g: &Grid, d: &mut Diagram, border: &mut [Vec<bool>]) {
    let (h, w) = (g.rows.len(), g.rows.first().map_or(0, Vec::len));
    for r in 0..h {
        for c in 0..w {
            let ch = g.rows[r][c];
            if !matches!(ch, '+' | '/') {
                continue;
            }
            if !(horizontal(g.at(r as isize, c as isize + 1))
                || g.at(r as isize, c as isize + 1) == '+')
                || !(vertical(g.at(r as isize + 1, c as isize))
                    || g.at(r as isize + 1, c as isize) == '+')
            {
                continue;
            }
            if let Some(found) = smallest(g, r, c) {
                for side in [found.top, found.bottom] {
                    border[side][found.left..=found.right].fill(true);
                }
                for row in &mut border[found.top..=found.bottom] {
                    row[found.left] = true;
                    row[found.right] = true;
                }
                d.boxes.push(found);
            }
        }
    }
}

/// The smallest box with its top left corner at `(r, c)`.
fn smallest(g: &Grid, r: usize, c: usize) -> Option<Box> {
    let w = g.rows[r].len();
    let h = g.rows.len();
    let mut best: Option<(usize, Box)> = None;
    let mut c2 = c + 1;
    while c2 < w {
        let ch = g.rows[r][c2];
        if corner(ch) && matches!(ch, '+' | '\\') {
            // Down the right side to a corner, then back along the bottom.
            let mut r2 = r + 1;
            while r2 < h {
                let side = g.rows[r2][c2];
                if corner(side) && matches!(side, '+' | '/') && closes(g, r, c, r2, c2) {
                    let area = (r2 - r) * (c2 - c);
                    if best.as_ref().is_none_or(|(a, _)| area < *a) {
                        let round = g.rows[r][c] == '/' || g.rows[r][c2] == '\\';
                        best = Some((
                            area,
                            Box {
                                top: r,
                                left: c,
                                bottom: r2,
                                right: c2,
                                round,
                                dashed: dashed(g, r, c, r2, c2),
                                color: None,
                                shape: Shape::Rect,
                            },
                        ));
                    }
                    break;
                }
                if !(vertical(side) || side == '+') {
                    break;
                }
                r2 += 1;
            }
        }
        if !(horizontal(ch) || ch == '+') {
            break;
        }
        c2 += 1;
    }
    best.map(|(_, b)| b)
}

/// Whether the bottom side and the left side of a box are drawn.
fn closes(g: &Grid, r: usize, c: usize, r2: usize, c2: usize) -> bool {
    let corner_ok = matches!(g.rows[r2][c], '+' | '\\');
    let bottom = (c + 1..c2).all(|cc| horizontal(g.rows[r2][cc]) || g.rows[r2][cc] == '+');
    let left = (r + 1..r2).all(|rr| vertical(g.rows[rr][c]) || g.rows[rr][c] == '+');
    corner_ok && bottom && left
}

fn dashed(g: &Grid, r: usize, c: usize, r2: usize, c2: usize) -> bool {
    (c..=c2).any(|cc| g.rows[r][cc] == '=' || g.rows[r2][cc] == '=')
        || (r..=r2).any(|rr| g.rows[rr][c] == ':' || g.rows[rr][c2] == ':')
}

/// The color and shape codes inside the boxes.
fn codes(g: &Grid, d: &mut Diagram, used: &mut [Vec<bool>]) {
    for (r, row) in g.rows.iter().enumerate() {
        let text: String = row.iter().collect();
        for (start, token) in tokens(&text) {
            // A box's side may touch the code: `{d}|`.
            let token = token.trim_end_matches(line_char);
            let code = if let Some(color) = color_code(token) {
                Some((Some(color), None))
            } else {
                shape_code(token).map(|shape| (None, Some(shape)))
            };
            let Some((color, shape)) = code else { continue };
            // The innermost box around it.
            let Some(b) = d
                .boxes
                .iter_mut()
                .filter(|b| b.top < r && r < b.bottom && b.left < start && start < b.right)
                .min_by_key(|b| (b.bottom - b.top) * (b.right - b.left))
            else {
                continue;
            };
            if let Some(color) = color {
                b.color = Some(color);
            }
            if let Some(shape) = shape {
                b.shape = shape;
            }
            for cell in used[r].iter_mut().skip(start).take(token.chars().count()) {
                *cell = true;
            }
        }
    }
}

/// The words of `text` apart by spaces, each with its column.
fn tokens(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].1 == ' ' {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].1 != ' ' {
            i += 1;
        }
        let from = chars[start].0;
        let to = chars.get(i).map_or(text.len(), |c| c.0);
        out.push((start, &text[from..to]));
    }
    out
}

fn color_code(token: &str) -> Option<Color> {
    let code = token.strip_prefix('c')?;
    if code.len() != 3 {
        return None;
    }
    let named = match code {
        "RED" => Some(0xE53935),
        "BLU" => Some(0x5555BB),
        "GRE" => Some(0x7ED17E),
        "PNK" => Some(0xFF8FA3),
        "YEL" => Some(0xF6E05E),
        "BLK" => Some(0x333333),
        "WHT" => Some(0xFFFFFF),
        _ => None,
    };
    if let Some(hex) = named {
        return Some(Color::hex(hex));
    }
    let digits: Vec<u32> = code
        .chars()
        .map(|c| c.to_digit(16))
        .collect::<Option<_>>()?;
    Some(Color::hex(
        digits[0] * 0x110000 + digits[1] * 0x1100 + digits[2] * 0x11,
    ))
}

fn shape_code(token: &str) -> Option<Shape> {
    Some(match token {
        "{d}" => Shape::Document,
        "{s}" => Shape::Storage,
        "{io}" => Shape::Io,
        "{c}" => Shape::Choice,
        "{o}" => Shape::Ellipse,
        "{tr}" | "{mo}" => Shape::Trapezoid,
        _ => return None,
    })
}

/// The lines: each line character not on a box's side joined to the
/// characters beside it it runs into, and a head where an arrow meets it.
fn connect(g: &Grid, d: &mut Diagram, border: &[Vec<bool>], used: &mut [Vec<bool>]) {
    let (h, w) = (g.rows.len(), g.rows.first().map_or(0, Vec::len));
    let runs_h = |r: usize, c: usize| {
        let ch = g.rows[r][c];
        horizontal(ch) || corner(ch) || matches!(ch, '<' | '>')
    };
    let runs_v = |r: usize, c: usize| {
        let ch = g.rows[r][c];
        vertical(ch) || corner(ch) || matches!(ch, '^' | 'v' | 'V')
    };
    for r in 0..h {
        for c in 0..w {
            let ch = g.rows[r][c];
            if border[r][c] || !line_char(ch) {
                continue;
            }
            used[r][c] = true;
            let dashed = matches!(ch, '=' | ':');
            // Right and down, so each joint is drawn once.
            if (horizontal(ch) || corner(ch)) && c + 1 < w && runs_h(r, c + 1) {
                d.lines
                    .push(((r, c), (r, c + 1), dashed || g.rows[r][c + 1] == '='));
            }
            if (vertical(ch) || corner(ch)) && r + 1 < h && runs_v(r + 1, c) {
                d.lines
                    .push(((r, c), (r + 1, c), dashed || g.rows[r + 1][c] == ':'));
            }
            // A line that ends on a box's side reaches it.
            if horizontal(ch) && c > 0 && border[r][c - 1] {
                d.lines.push(((r, c - 1), (r, c), dashed));
            }
            if vertical(ch) && r > 0 && border[r - 1][c] {
                d.lines.push(((r - 1, c), (r, c), dashed));
            }
        }
    }
    // Heads, where an arrow character meets a line.
    for (r, row) in g.rows.iter().enumerate() {
        for (c, &ch) in row.iter().enumerate() {
            let (ri, ci) = (r as isize, c as isize);
            let head = match ch {
                '>' if line_char(g.at(ri, ci - 1)) => Some((Point::Right, (r, c - 1))),
                '<' if line_char(g.at(ri, ci + 1)) => Some((Point::Left, (r, c + 1))),
                '^' if line_char(g.at(ri + 1, ci)) => Some((Point::Up, (r + 1, c))),
                'v' | 'V' if line_char(g.at(ri - 1, ci)) => Some((Point::Down, (r - 1, c))),
                _ => None,
            };
            if let Some((point, from)) = head {
                used[r][c] = true;
                d.lines.push((from, (r, c), false));
                d.heads.push(((r, c), point));
            }
        }
    }
}

/// The words left: runs of characters apart by two spaces or more.
fn text(g: &Grid, d: &mut Diagram, border: &[Vec<bool>], used: &[Vec<bool>]) {
    for (r, row) in g.rows.iter().enumerate() {
        let mut c = 0;
        while c < row.len() {
            if row[c] == ' ' || border[r][c] || used[r][c] {
                c += 1;
                continue;
            }
            let start = c;
            let mut end = c;
            while end < row.len() && !border[r][end] && !used[r][end] {
                if row[end] == ' ' && row.get(end + 1).is_none_or(|n| *n == ' ') {
                    break;
                }
                end += 1;
            }
            let words: String = row[start..end].iter().collect();
            if !words.trim().is_empty() {
                d.texts.push(((r, start), words.trim_end().to_owned()));
            }
            c = end.max(start + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Diagram {
        let lines: Vec<Line> = text
            .lines()
            .enumerate()
            .map(|(no, t)| Line {
                no,
                text: t.trim(),
                indent: t.len() - t.trim_start().len(),
                blank_before: 0,
            })
            .collect();
        parse(&lines, "@startditaa\n").unwrap()
    }

    #[test]
    fn boxes_lines_heads_codes_and_words_are_found() {
        let d = read(
            "/--------\\     +-------+\n| cBLU   |---->| cGRE  |\n| Client |     | Store |\n\\--------/     |  {s}  |\n               +-------+",
        );
        assert_eq!(d.boxes.len(), 2, "{:?}", d.boxes);
        assert!(d.boxes[0].round && !d.boxes[1].round);
        assert_eq!(d.boxes[0].color, Some(Color::hex(0x5555BB)));
        assert_eq!(d.boxes[1].shape, Shape::Storage);
        let doc = read("+---------+\n| Text {d}|\n+---------+");
        assert_eq!(doc.boxes[0].shape, Shape::Document);
        assert_eq!(doc.texts.len(), 1, "{:?}", doc.texts);
        assert_eq!(d.heads, [((1, 14), Point::Right)]);
        let words: Vec<&str> = d.texts.iter().map(|(_, t)| t.as_str()).collect();
        assert_eq!(words, ["Client", "Store"], "codes are not words");
        assert!(
            d.lines
                .iter()
                .any(|(a, b, _)| *a == (1, 10) && *b == (1, 11))
        );
    }

    #[test]
    fn a_dashed_side_and_a_line_that_bends() {
        let d = read("+==+\n|  |\n+--+\n |\n +--->X");
        assert!(d.boxes[0].dashed);
        assert!(
            d.lines.iter().any(|(a, b, _)| *a == (2, 1) && *b == (3, 1)),
            "{:?}",
            d.lines
        );
        assert_eq!(d.heads.len(), 1);
    }
}
