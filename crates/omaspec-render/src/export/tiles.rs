//! A picture too large for one file, cut into tiles that keep its words
//! readable (REQ-070). `render --min-font PX` draws a PNG or JPEG no
//! smaller than where its smallest words are `PX` pixels high: where the
//! whole picture fits in one file at that size, it is one file, shrunk as
//! `--fit` shrinks it, but no further; where it does not, it is cut into
//! tiles, each within the size of one file, and each cut runs between two
//! lines of words, never through one, where the picture leaves room.

use resvg::{tiny_skia, usvg};

use super::{ExportError, Format, MAX_PIXELS, MAX_SIDE};

/// The longest side of a tile, in pixels: what an image viewer or a
/// graphics card shows whole.
pub const TILE_SIDE: u32 = 16_384;
/// The largest scale a picture is drawn at for its smallest words, as
/// `--scale` allows.
const MAX_SCALE: f32 = 16.0;
/// How far a cut may move from where an even cut would fall, to run between
/// two lines of words, as a share of a tile's side.
const MOVE: f32 = 0.1;

/// How a picture is drawn for words of a least size: the scale, and the
/// pixels of the whole picture each file holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Pixels per unit of the SVG.
    pub scale: f32,
    /// The pixels of the whole picture each file holds, row by row, left
    /// to right; one, the whole, where it fits in one file.
    pub parts: Vec<tiny_skia::IntRect>,
    /// The rows of tiles, and the most a row has.
    pub rows: usize,
    pub columns: usize,
    /// The size of the smallest words at `scale`, in pixels; `None` for a
    /// picture without words.
    pub smallest: Option<f32>,
}

impl Plan {
    /// Whether the picture is cut into tiles.
    pub fn tiled(&self) -> bool {
        self.parts.len() > 1
    }
}

/// How to draw `tree` as `format`, at `scale` where it fits in one file,
/// shrunk where it does not, but never below where its smallest words are
/// `least` pixels high; cut into tiles where it then does not fit.
pub fn plan(tree: &usvg::Tree, format: Format, scale: f32, least: f32) -> Plan {
    let smallest = smallest_words(tree);
    let floor = smallest.map_or(0.0, |size| (least / size).min(MAX_SCALE));
    let side = match format {
        Format::Jpeg => f32::from(u16::MAX),
        _ => MAX_SIDE,
    };
    let fits = fitting(tree, side);
    let scale = scale.min(fits).max(floor);
    let size = tree.size();
    let (width, height) = (
        (size.width() * scale).ceil().max(1.0) as u32,
        (size.height() * scale).ceil().max(1.0) as u32,
    );
    let smallest = smallest.map(|size| size * scale);
    if scale <= fits {
        let whole =
            tiny_skia::IntRect::from_xywh(0, 0, width, height).expect("a picture that fits");
        return Plan {
            scale,
            parts: vec![whole],
            rows: 1,
            columns: 1,
            smallest,
        };
    }
    let words = word_boxes(tree, scale);
    let (parts, rows, columns) = grid(width, height, &words);
    Plan {
        scale,
        parts,
        rows,
        columns,
        smallest,
    }
}

/// The largest scale, to two places, at which all of `tree` fits in
/// [`MAX_PIXELS`] and `side` pixels a side; as `--fit` draws it.
pub(super) fn fitting(tree: &usvg::Tree, side: f32) -> f32 {
    let size = tree.size();
    let by_area = (MAX_PIXELS as f32 / (size.width() * size.height())).sqrt();
    let by_side = side / size.width().max(size.height());
    (by_area.min(by_side) * 100.0).floor() / 100.0
}

/// The size of the smallest words drawn in `tree`, in its units, as they
/// are drawn: a group's scale shrinks or grows them.
pub fn smallest_words(tree: &usvg::Tree) -> Option<f32> {
    let mut smallest: Option<f32> = None;
    each_text(tree.root(), &mut |text| {
        let t = text.abs_transform();
        let k = (t.sx * t.sy - t.kx * t.ky).abs().sqrt();
        for chunk in text.chunks() {
            for span in chunk.spans() {
                let words = chunk.text().get(span.start()..span.end()).unwrap_or("");
                let seen = span.fill().is_some() || span.stroke().is_some();
                if !seen || words.trim().is_empty() {
                    continue;
                }
                let size = span.font_size().get() * k;
                if size > 0.0 {
                    smallest = Some(smallest.map_or(size, |s| s.min(size)));
                }
            }
        }
    });
    smallest
}

fn each_text(group: &usvg::Group, each: &mut impl FnMut(&usvg::Text)) {
    for node in group.children() {
        match node {
            usvg::Node::Group(group) => each_text(group, each),
            usvg::Node::Text(text) => each(text),
            _ => {}
        }
    }
}

/// Where words stand in `tree` drawn at `scale`, in pixels: left, top,
/// right and bottom.
fn word_boxes(tree: &usvg::Tree, scale: f32) -> Vec<[f32; 4]> {
    let mut boxes = Vec::new();
    each_text(tree.root(), &mut |text| {
        let at = text.abs_bounding_box();
        boxes.push([
            at.left() * scale,
            at.top() * scale,
            at.right() * scale,
            at.bottom() * scale,
        ]);
    });
    boxes
}

/// A picture of `width` × `height` pixels cut into tiles, each no more
/// than [`TILE_SIDE`] a side and [`MAX_PIXELS`] in all: first across, into
/// rows, then each row down, into columns; each cut between two lines of
/// `words` where it can be. The tiles, row by row, the rows and the most
/// columns a row has.
fn grid(width: u32, height: u32, words: &[[f32; 4]]) -> (Vec<tiny_skia::IntRect>, usize, usize) {
    let (w, h) = (width as f32, height as f32);
    let side = TILE_SIDE as f32;
    let columns = (w / (side * (1.0 - MOVE))).ceil().max(1.0) as usize;
    // A tile is as wide as the row's even share, or the share and the most
    // a cut may move.
    let widest = if columns == 1 {
        w
    } else {
        (w / columns as f32 * (1.0 + MOVE)).ceil().min(side)
    };
    let tallest = side.min(MAX_PIXELS as f32 / widest).floor();
    let rows = (h / (tallest * (1.0 - MOVE))).ceil().max(1.0) as usize;
    let across: Vec<(f32, f32)> = words.iter().map(|b| (b[1], b[3])).collect();
    let row_cuts = cuts(h, rows, tallest, &across);
    let mut parts = Vec::new();
    for pair in row_cuts.windows(2) {
        let (top, bottom) = (pair[0], pair[1]);
        let down: Vec<(f32, f32)> = words
            .iter()
            .filter(|b| b[3] > top as f32 && b[1] < bottom as f32)
            .map(|b| (b[0], b[2]))
            .collect();
        let column_cuts = cuts(w, columns, widest, &down);
        for col in column_cuts.windows(2) {
            let part = tiny_skia::IntRect::from_xywh(
                col[0] as i32,
                top as i32,
                col[1] - col[0],
                bottom - top,
            )
            .expect("a tile of a picture");
            parts.push(part);
        }
    }
    (parts, rows, columns)
}

/// Where `length` pixels are cut into `count` pieces of no more than
/// `longest` each: 0, the cuts and `length`. Each cut falls where an even
/// cut would, or as near it as the next gap between `taken` spans lies,
/// within [`MOVE`] of a piece.
fn cuts(length: f32, count: usize, longest: f32, taken: &[(f32, f32)]) -> Vec<u32> {
    let mut spans: Vec<(f32, f32)> = taken
        .iter()
        .map(|&(a, b)| (a.min(b) - 1.0, a.max(b) + 1.0))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for (a, b) in spans {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    let even = length / count as f32;
    let mut at = vec![0u32];
    let mut prev = 0.0;
    for i in 1..count {
        let left = (count - i + 1) as f32;
        let ideal = prev + (length - prev) / left;
        let lo = (ideal - MOVE * even).max(length - (left - 1.0) * longest);
        let hi = (ideal + MOVE * even).min(prev + longest);
        let cut = free_near(&merged, ideal, lo, hi).round();
        at.push(cut as u32);
        prev = cut;
    }
    at.push(length as u32);
    at
}

/// The point nearest `ideal`, within `lo` and `hi`, that no span of
/// `merged` (sorted, apart) covers; `ideal` where there is none.
fn free_near(merged: &[(f32, f32)], ideal: f32, lo: f32, hi: f32) -> f32 {
    let Some(&(start, end)) = merged.iter().find(|&&(a, b)| a < ideal && ideal < b) else {
        return ideal;
    };
    [start.floor(), end.ceil()]
        .into_iter()
        .filter(|&p| (lo..=hi).contains(&p))
        .min_by(|a, b| (a - ideal).abs().total_cmp(&(b - ideal).abs()))
        .unwrap_or(ideal)
}

/// The pixels `part` of `tree` drawn at `scale`, as a PNG or JPEG file.
pub(super) fn tile(
    tree: &usvg::Tree,
    scale: f32,
    part: tiny_skia::IntRect,
    format: Format,
) -> Result<Vec<u8>, ExportError> {
    let pixmap = super::rasterize_part(tree, scale, part)?;
    match format {
        Format::Jpeg => super::encode_jpeg(&pixmap, || ExportError::TooLarge {
            width: part.width(),
            height: part.height(),
            fits: None,
        }),
        _ => pixmap
            .encode_png()
            .map_err(|e| ExportError::Svg(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cut that would run through a line of words moves to the gap above
    /// or below it, whichever is nearer, as far as a tile may grow.
    #[test]
    fn a_cut_runs_between_two_lines_of_words() {
        assert_eq!(cuts(1000.0, 2, 1000.0, &[]), [0, 500, 1000]);
        // A line from 490 to 520: the cut moves up, to its top.
        assert_eq!(cuts(1000.0, 2, 1000.0, &[(490.0, 520.0)]), [0, 489, 1000]);
        // Two lines from 480 to 504, one after the other: down is nearer.
        assert_eq!(
            cuts(1000.0, 2, 1000.0, &[(480.0, 495.0), (495.0, 504.0)]),
            [0, 505, 1000]
        );
        // Words from 300 to 700: no gap within reach, the cut stays.
        assert_eq!(cuts(1000.0, 2, 1000.0, &[(300.0, 700.0)]), [0, 500, 1000]);
        // A tile may not grow past its longest: under a line from 470 to
        // 545, the gap at 546 is nearer, but the first tile would be 546
        // long, past 540; the gap above it is taken.
        assert_eq!(cuts(1000.0, 2, 540.0, &[(470.0, 545.0)]), [0, 469, 1000]);
    }

    /// Every tile stays within the size of one file and they cover the
    /// picture without a gap or an overlap, row by row.
    #[test]
    fn tiles_cover_the_picture_each_within_one_file() {
        let words: Vec<[f32; 4]> = (0..400)
            .map(|i| {
                let y = i as f32 * 120.0;
                [10.0, y, 7000.0, y + 14.0]
            })
            .collect();
        let (parts, rows, columns) = grid(7459, 47700, &words);
        assert_eq!((rows, columns), (4, 1));
        let mut covered = 0u64;
        let mut bottom = 0;
        for part in &parts {
            assert!(
                part.width() <= TILE_SIDE && part.height() <= TILE_SIDE,
                "{part:?}"
            );
            assert!(
                u64::from(part.width()) * u64::from(part.height()) <= MAX_PIXELS,
                "{part:?}"
            );
            assert_eq!(part.y() as u32, bottom);
            bottom = part.bottom() as u32;
            covered += u64::from(part.width()) * u64::from(part.height());
            // Each cut between lines: none of them crosses a tile's top.
            let top = part.y() as f32;
            assert!(
                words.iter().all(|w| !(w[1] < top && top < w[3])),
                "{part:?}"
            );
        }
        assert_eq!(bottom, 47700);
        assert_eq!(covered, 7459 * 47700);
        // Wider than a tile: columns, each row cut alike.
        let (parts, rows, columns) = grid(40_000, 9000, &[]);
        assert_eq!(columns, 3);
        assert_eq!(parts.len(), rows * columns);
        assert!(parts.iter().all(|p| p.width() <= TILE_SIDE));
    }
}
