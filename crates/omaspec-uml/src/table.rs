//! A table drawn in a diagram's look: the columns named in a head row,
//! the rows in a head column, each cell a few lines. The state-event
//! matrix of a state machine (`omaspec matrix -o`, the pane's table view)
//! is drawn with it in the look of the machine's states
//! ([`crate::state::draw_table`]): the heads filled as a state is, the
//! words in its ink, the grid in its border.
//!
//! Each line of a cell may stand for a line of the source, the
//! transition it says; the map holds where ([`crate::map::Cell`]), so the
//! pane can land on it.

use crate::color::{Color, Paint};
use crate::graph::Rect;
use crate::map;
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{Block, Family, Font, Style, styled};

/// Room around the words of a cell.
const PAD_X: f32 = 10.0;
const PAD_Y: f32 = 6.0;
/// How far a row's head steps in for each state around it.
const INDENT: f32 = 14.0;
/// The contrast muted words keep with their ground (WCAG AA).
const LEGIBLE: f32 = 4.5;
const MUTED: Color = Color::hex(0x888888);

/// A table to draw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    /// What the head column holds, over it.
    pub corner: String,
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    pub head: String,
    /// How deep it sits among the rows: its head steps in so far.
    pub depth: usize,
    /// One for each column.
    pub cells: Vec<Cell>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cell {
    pub tone: Tone,
    /// Its lines, each with the line of the source it stands for.
    pub lines: Vec<(String, Option<usize>)>,
    /// A last line in gray that says where the lines come from.
    pub from: Option<String>,
}

/// How a cell reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Plain,
    /// Its lines are another row's, in italics.
    Borrowed,
    /// Its lines contradict each other: framed in `warn`.
    Warned,
}

/// The colors and the font a table is drawn in.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub font: Font,
    /// The heads' fill; the cells lie on the page.
    pub head: Paint,
    pub page: Paint,
    pub grid: Color,
    pub round: f32,
    /// The frame of a warned cell.
    pub warn: Color,
}

/// Draws `table` on `canvas` with its top at `top`, and says where each
/// line of a cell that stands for a line of the source is.
pub fn draw(
    canvas: &mut Canvas,
    table: &Table,
    look: &Look,
    family: Family,
    top: f32,
) -> Vec<map::Cell> {
    let ink = look.font.color;
    let page = look.page.unwrap_or(Color::WHITE);
    let bold = look.font.bold();
    let muted = Font {
        color: MUTED.legible_on(page, ink, LEGIBLE),
        ..look.font
    };
    let borrowed = Font {
        color: muted.color,
        ..look.font.italic()
    };
    let small = Font {
        size: (look.font.size - 2.0).max(8.0),
        ..muted
    };
    let words = |text: &str, font: Font| one(text, font, family);

    let corner = words(&table.corner, muted);
    let columns: Vec<Block> = table.columns.iter().map(|c| words(c, bold)).collect();
    let heads: Vec<Block> = table.rows.iter().map(|r| words(&r.head, bold)).collect();
    // Each cell's blocks: its lines, then where they come from.
    let cells: Vec<Vec<(Vec<Block>, Option<Block>)>> = table
        .rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| {
                    let font = match cell.tone {
                        Tone::Borrowed => borrowed,
                        _ => look.font,
                    };
                    let lines = cell.lines.iter().map(|(t, _)| words(t, font)).collect();
                    (lines, cell.from.as_deref().map(|f| words(f, small)))
                })
                .collect()
        })
        .collect();

    // The widths of the columns, the head column first.
    let mut widths = vec![corner.width];
    widths.extend(columns.iter().map(|b| b.width));
    for (r, row) in table.rows.iter().enumerate() {
        widths[0] = widths[0].max(heads[r].width + row.depth as f32 * INDENT);
        for (c, (lines, from)) in cells[r].iter().enumerate() {
            let wide = lines
                .iter()
                .chain(from)
                .map(|b| b.width)
                .fold(0.0f32, f32::max);
            if let Some(width) = widths.get_mut(c + 1) {
                *width = width.max(wide);
            }
        }
    }
    let widths: Vec<f32> = widths.iter().map(|w| w + 2.0 * PAD_X).collect();
    let head_h = columns
        .iter()
        .chain([&corner])
        .map(|b| b.height)
        .fold(0.0f32, f32::max)
        + 2.0 * PAD_Y;
    let heights: Vec<f32> = (0..table.rows.len())
        .map(|r| {
            let tall = cells[r]
                .iter()
                .map(|(lines, from)| lines.iter().chain(from).map(|b| b.height).sum::<f32>())
                .fold(heads[r].height, f32::max);
            tall + 2.0 * PAD_Y
        })
        .collect();
    let width: f32 = widths.iter().sum();
    let height = head_h + heights.iter().sum::<f32>();
    let lefts: Vec<f32> = widths
        .iter()
        .scan(0.0, |x, w| {
            let left = *x;
            *x += w;
            Some(left)
        })
        .collect();
    let stroke = Stroke::new(look.grid, 1.0);

    // The heads filled, the cells on the page: the whole table in the
    // heads' fill with its round corners, the cells' part over it, round
    // only where it meets the table's corner.
    canvas.plain(|canvas| {
        canvas.rect(
            Layer::Background,
            (0.0, top, width, height),
            look.round,
            Some(look.head.unwrap_or(page)),
            None,
        );
        if look.head.is_some() {
            let (left, y0, bottom) = (widths[0], top + head_h, top + height);
            let r = look.round.min(width - left).min(height - head_h).max(0.0);
            let d = format!(
                "M{} {}H{}V{}Q{} {} {} {}H{}Z",
                n(left),
                n(y0),
                n(width),
                n(bottom - r),
                n(width),
                n(bottom),
                n(width - r),
                n(bottom),
                n(left)
            );
            canvas.path(
                Layer::Background,
                &d,
                (left, y0, width, bottom),
                Some(page),
                None,
            );
        }
        // The grid, each line once.
        let mut y = top + head_h;
        for h in &heights[..heights.len().saturating_sub(1)] {
            y += h;
            canvas.line(Layer::Frames, (0.0, y), (width, y), stroke);
        }
        for left in lefts.iter().skip(2) {
            canvas.line(Layer::Frames, (*left, top), (*left, top + height), stroke);
        }
    });
    let strong = Stroke::new(look.grid, 1.5);
    canvas.line(
        Layer::Frames,
        (0.0, top + head_h),
        (width, top + head_h),
        strong,
    );
    if widths.len() > 1 {
        canvas.line(
            Layer::Frames,
            (widths[0], top),
            (widths[0], top + height),
            strong,
        );
    }
    canvas.rect(
        Layer::Frames,
        (0.0, top, width, height),
        look.round,
        None,
        Some(strong),
    );

    // The words.
    let mut out = Vec::new();
    canvas.text(
        Layer::Front,
        &corner,
        (PAD_X, top + (head_h - corner.height) / 2.0),
        Align::Left,
    );
    for (c, block) in columns.iter().enumerate() {
        let x = lefts[c + 1] + widths[c + 1] / 2.0;
        canvas.text(
            Layer::Front,
            block,
            (x, top + (head_h - block.height) / 2.0),
            Align::Center,
        );
    }
    let mut y = top + head_h;
    for (r, row) in table.rows.iter().enumerate() {
        let h = heights[r];
        let head = &heads[r];
        canvas.text(
            Layer::Front,
            head,
            (
                PAD_X + row.depth as f32 * INDENT,
                y + (h - head.height) / 2.0,
            ),
            Align::Left,
        );
        for (c, cell) in row.cells.iter().enumerate() {
            let Some((lines, from)) = cells[r].get(c) else {
                continue;
            };
            let (left, w) = (lefts[c + 1], widths[c + 1]);
            if cell.tone == Tone::Warned {
                canvas.rect(
                    Layer::Frames,
                    (left + 2.0, y + 2.0, w - 4.0, h - 4.0),
                    3.0,
                    None,
                    Some(Stroke::new(look.warn, 2.0)),
                );
            }
            let mut ly = y + PAD_Y;
            for (block, (_, line)) in lines.iter().zip(&cell.lines) {
                canvas.text(Layer::Front, block, (left + PAD_X, ly), Align::Left);
                if let Some(line) = line {
                    out.push(map::Cell {
                        rect: Rect {
                            x: left,
                            y: ly,
                            w,
                            h: block.height,
                        },
                        line: *line,
                    });
                }
                ly += block.height;
            }
            if let Some(from) = from {
                canvas.text(Layer::Front, from, (left + PAD_X, ly), Align::Left);
            }
        }
        y += h;
    }
    out
}

/// `text` on one line, as written: no markup.
fn one(text: &str, font: Font, family: Family) -> Block {
    if text.is_empty() {
        return styled(Vec::new(), font, family);
    }
    styled(
        vec![vec![(text.to_owned(), Style::from(font))]],
        font,
        family,
    )
}
