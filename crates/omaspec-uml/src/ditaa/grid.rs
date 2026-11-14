//! Reading a ditaa drawing: the characters into a grid, the boxes found in
//! it, then the lines and heads among what is left, then the text.

use super::{Box, Diagram, Point, Shape, Words};
use crate::Declined;
use crate::color::Color;
use crate::common::Frame;
use crate::source::Line;

/// The most cells a drawing may have.
const MAX_CELLS: usize = 200_000;
/// The most steps finding the boxes may take: a drawing of many corners
/// that close late is declined rather than read for long.
const MAX_STEPS: usize = 20_000_000;

fn horizontal(c: char) -> bool {
    matches!(c, '-' | '=')
}

/// A `-` and a `=` between two letters or digits, `Re-run`, stand for
/// these while the drawing is read: they are words, not lines. Read as a
/// line, the hyphen turned `Re-run` into `Re run`, another word.
const WORD_DASH: char = '\u{E000}';
const WORD_EQUALS: char = '\u{E001}';

/// `row` with each dash between two letters or digits kept for the words.
fn keep_word_dashes(row: &mut [char]) {
    for c in 1..row.len().saturating_sub(1) {
        let word = |ch: char| ch.is_alphanumeric();
        if word(row[c - 1]) && word(row[c + 1]) {
            row[c] = match row[c] {
                '-' => WORD_DASH,
                '=' => WORD_EQUALS,
                other => other,
            };
        }
    }
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
        keep_word_dashes(&mut row);
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
    let g = Grid::new(rows);
    let mut border = vec![vec![false; width]; g.rows.len()];
    let mut steps = 0;
    find_boxes(&g, &mut d, &mut border, &mut steps)?;
    let inner = innermost(&g, &d, &mut steps)?;
    // What is written inside a box and is a code of it: its color and shape.
    let mut used = vec![vec![false; width]; g.rows.len()];
    codes(&g, &mut d, &inner, &mut used);
    connect(&g, &mut d, &border, &mut used);
    text(&g, &mut d, &border, &used, &inner);
    Ok(d)
}

/// A step more of reading the boxes, or the refusal once there are too
/// many.
fn step(steps: &mut usize, more: usize) -> Result<(), Declined> {
    *steps += more;
    if *steps > MAX_STEPS {
        return Err(Declined::at(0, "this drawing has too many corners to read"));
    }
    Ok(())
}

struct Grid {
    rows: Vec<Vec<char>>,
    /// From each cell, how many cells on to the right can be a box's top
    /// or bottom (`-`, `=`, `+`), itself counted.
    across: Vec<Vec<usize>>,
    /// The same downwards, for a box's sides (`|`, `:`, `+`).
    down: Vec<Vec<usize>>,
    /// For each column, the rows where a box's left side may end (`+`,
    /// `\`), from the top.
    ends: Vec<Vec<usize>>,
}

impl Grid {
    fn new(rows: Vec<Vec<char>>) -> Grid {
        let (h, w) = (rows.len(), rows.first().map_or(0, Vec::len));
        let mut across = vec![vec![0; w + 1]; h];
        let mut down = vec![vec![0; w]; h + 1];
        let mut ends = vec![Vec::new(); w];
        for (r, row) in rows.iter().enumerate() {
            for (c, ch) in row.iter().enumerate() {
                if matches!(ch, '+' | '\\') {
                    ends[c].push(r);
                }
            }
        }
        for r in (0..h).rev() {
            for c in (0..w).rev() {
                let ch = rows[r][c];
                if horizontal(ch) || ch == '+' {
                    across[r][c] = across[r][c + 1] + 1;
                }
                if vertical(ch) || ch == '+' {
                    down[r][c] = down[r + 1][c] + 1;
                }
            }
        }
        Grid {
            rows,
            across,
            down,
            ends,
        }
    }

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
fn find_boxes(
    g: &Grid,
    d: &mut Diagram,
    border: &mut [Vec<bool>],
    steps: &mut usize,
) -> Result<(), Declined> {
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
            if let Some(found) = smallest(g, r, c, steps)? {
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
    Ok(())
}

/// The smallest box with its top left corner at `(r, c)`: along the top
/// to a corner, down its right side and the left one to corners in one
/// row, and the bottom drawn between them. A box no smaller than the best
/// found so far is not looked for.
fn smallest(g: &Grid, r: usize, c: usize, steps: &mut usize) -> Result<Option<Box>, Declined> {
    let w = g.rows[r].len();
    // The rows where the left side may end: below the top, as far as it
    // runs.
    let column = &g.ends[c];
    let below = column.partition_point(|&row| row <= r);
    let reach = r + 1 + g.down[r + 1][c];
    let till = column.partition_point(|&row| row <= reach);
    let ends = &column[below..till.max(below)];
    let mut best: Option<(usize, Box)> = None;
    let mut c2 = c + 1;
    while c2 < w && best.as_ref().is_none_or(|(a, _)| c2 - c < *a) {
        step(steps, 1)?;
        let ch = g.rows[r][c2];
        if matches!(ch, '+' | '\\') {
            let right_reach = r + 1 + g.down[r + 1][c2];
            for &r2 in ends {
                step(steps, 1)?;
                let area = (r2 - r) * (c2 - c);
                if r2 > right_reach || best.as_ref().is_some_and(|(a, _)| area >= *a) {
                    break;
                }
                let bottom = g.across[r2][c + 1] >= c2 - c - 1;
                if matches!(g.rows[r2][c2], '+' | '/') && bottom {
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
                    break;
                }
            }
        }
        if !(horizontal(ch) || ch == '+') {
            break;
        }
        c2 += 1;
    }
    Ok(best.map(|(_, b)| b))
}

/// For each cell, the innermost box it lies inside, by its place in the
/// diagram's boxes: the larger painted first, the smaller over them.
fn innermost(
    g: &Grid,
    d: &Diagram,
    steps: &mut usize,
) -> Result<Vec<Vec<Option<usize>>>, Declined> {
    let width = g.rows.first().map_or(0, Vec::len);
    let mut inner = vec![vec![None; width]; g.rows.len()];
    let mut order: Vec<usize> = (0..d.boxes.len()).collect();
    order.sort_by_key(|&i| {
        let b = &d.boxes[i];
        std::cmp::Reverse((b.bottom - b.top) * (b.right - b.left))
    });
    for i in order {
        let b = &d.boxes[i];
        step(steps, (b.bottom - b.top) * (b.right - b.left))?;
        for row in &mut inner[b.top + 1..b.bottom] {
            row[b.left + 1..b.right].fill(Some(i));
        }
    }
    Ok(inner)
}

fn dashed(g: &Grid, r: usize, c: usize, r2: usize, c2: usize) -> bool {
    (c..=c2).any(|cc| g.rows[r][cc] == '=' || g.rows[r2][cc] == '=')
        || (r..=r2).any(|rr| g.rows[rr][c] == ':' || g.rows[rr][c2] == ':')
}

/// The color and shape codes inside the boxes.
fn codes(g: &Grid, d: &mut Diagram, inner: &[Vec<Option<usize>>], used: &mut [Vec<bool>]) {
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
            let Some(b) = inner[r][start].map(|i| &mut d.boxes[i]) else {
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
fn text(
    g: &Grid,
    d: &mut Diagram,
    border: &[Vec<bool>],
    used: &[Vec<bool>],
    inner: &[Vec<Option<usize>>],
) {
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
            let words: String = row[start..end]
                .iter()
                .map(|&ch| match ch {
                    WORD_DASH => '-',
                    WORD_EQUALS => '=',
                    other => other,
                })
                .collect();
            if !words.trim().is_empty() {
                d.texts.push(Words {
                    at: (r, start),
                    text: words.trim_end().to_owned(),
                    inside: inner[r][start],
                });
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
    fn a_dash_between_letters_is_a_word_s() {
        let d = read(
            "+-------------+\n| Re-run job  +---->\n+-------------+\n  a=b  x--y  pre-commit\n",
        );
        let words: Vec<&str> = d.texts.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(words, ["Re-run job", "a=b", "x", "y", "pre-commit"]);
        // `x--y` is a line between two words; the box's sides and the
        // arrow stay lines too.
        assert!(
            d.lines
                .iter()
                .any(|&((r, _), (r2, _), _)| r == 3 && r2 == 3)
        );
        assert_eq!(d.boxes.len(), 1);
        assert_eq!(d.heads.len(), 1);
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
        let words: Vec<&str> = d.texts.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(words, ["Client", "Store"], "codes are not words");
        assert!(
            d.lines
                .iter()
                .any(|(a, b, _)| *a == (1, 10) && *b == (1, 11))
        );
    }

    #[test]
    fn a_field_of_corners_and_a_comb_are_read_at_once() {
        // Every cell a corner: each box is the smallest at its corner.
        let n = 120;
        let field = vec!["+".repeat(n); n].join("\n");
        let start = std::time::Instant::now();
        assert_eq!(read(&field).boxes.len(), (n - 1) * (n - 1));
        // A top with teeth and no bottom: no box, and no long search.
        let comb = format!("{}\n{}", "+".repeat(n), vec!["|".repeat(n); n].join("\n"));
        assert!(read(&comb).boxes.is_empty());
        assert!(start.elapsed().as_secs() < 10, "{:?}", start.elapsed());
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
