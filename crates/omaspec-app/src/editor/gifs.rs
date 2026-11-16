//! Frames as a GIF that stays small: one table of colors for the whole
//! picture, and of each frame only the part that differs from the one
//! before it, the pixels that stayed left clear in it.
//!
//! A table per frame, as `gif::Frame::from_rgba` makes one, costs its
//! bytes each time and lets a color flicker from frame to frame; a whole
//! frame each time is most of a file in which only a light moves.
//!
//! [`Quality`] makes it smaller still, at a cost the writer chooses: fewer
//! colors, each pixel's code shorter, and a pixel drawn again only when
//! its color goes into another step.
//!
//! The frames wait in [`Frames`] as they are drawn, each deflated, their
//! colors counted for the table as each comes: a play of some hundred
//! frames 1,400 pixels wide held them as they were, gigabytes (REQ-074).

use std::borrow::Cow;
use std::collections::HashMap;

use anyhow::{Context, Result};
use color_quant::NeuQuant;

/// How many pixels of the first frame the table of colors learns from, at
/// most: every one of a frame up to a megapixel.
const LEARNED: usize = 1 << 20;
/// How many of the pixels that changed it learns from in each later frame,
/// at most: a light, its glow, a fade, where the colors the first frame
/// lacks come.
const CHANGED: usize = 1 << 16;
/// How hard a frame is deflated while it waits: the fastest, which still
/// packs a diagram's even colors some fifty times.
const PACKED: u8 = 1;

/// How a GIF trades its size for its look. The default keeps 256 places
/// in its table and draws every change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quality {
    /// The places of its table, 2 to 256, one of them for a pixel that
    /// stays as it was: so 32 places are 31 colors and codes of 5 bits.
    /// Fewer make each pixel's code shorter and the file smaller; a
    /// light's glow and the veil's shades come out in steps.
    pub colors: u16,
    /// How far each of red, green and blue may stray from what a pixel
    /// shows: it is drawn again when its color goes into another step of
    /// this many and one. 0 draws every change; a fade then moves in a few
    /// even steps, and the file is much smaller.
    pub lossy: u8,
}

impl Default for Quality {
    fn default() -> Self {
        Quality {
            colors: 256,
            lossy: 0,
        }
    }
}

/// The frames of a picture that moves, as they are drawn: each one unlike
/// the one before it deflated, with how long it stays; and the colors
/// seen, from pixels spread over them, for its table. One frame at a time
/// is held as it is.
#[derive(Default)]
pub struct Frames {
    width: usize,
    height: usize,
    /// Each frame unlike the one before it, deflated, and how long it
    /// stays, in hundredths of a second.
    kept: Vec<(Vec<u8>, u16)>,
    /// The last frame as it is, to know one like it.
    last: Vec<u8>,
    /// Each color counted, how often: of the first frame, and of what
    /// changed in each one after it.
    seen: HashMap<[u8; 3], u32>,
    /// A frame of another size than the first came.
    mixed: bool,
}

impl Frames {
    /// Adds a frame `width` by `height` pixels (RGBA) that stays `stay`
    /// hundredths of a second; one like the frame before only makes that
    /// one stay longer.
    pub fn push(&mut self, width: usize, height: usize, rgba: &[u8], stay: u16) {
        if self.kept.is_empty() {
            (self.width, self.height) = (width, height);
        }
        if (width, height) != (self.width, self.height) || rgba.len() != width * height * 4 {
            self.mixed = true;
            return;
        }
        if let Some((_, last)) = self.kept.last_mut()
            && rgba == self.last
        {
            *last = last.saturating_add(stay);
            return;
        }
        self.count(rgba);
        self.kept
            .push((miniz_oxide::deflate::compress_to_vec(rgba, PACKED), stay));
        self.last.clear();
        self.last.extend_from_slice(rgba);
    }

    /// Counts the colors of `rgba` for the table: those of the first frame,
    /// else those of the pixels that changed since the frame before, spread
    /// over them where there are many.
    fn count(&mut self, rgba: &[u8]) {
        let pixels = rgba.as_chunks::<4>().0;
        let before = self.last.as_chunks::<4>().0;
        let count = |pixel: &[u8; 4]| {
            *self.seen.entry([pixel[0], pixel[1], pixel[2]]).or_default() += 1;
        };
        if self.kept.is_empty() {
            let step = (pixels.len() / LEARNED).max(1);
            pixels.iter().step_by(step).for_each(count);
            return;
        }
        let changed = || pixels.iter().zip(before).filter(|(now, was)| now != was);
        let step = (changed().count() / CHANGED).max(1);
        changed().map(|(now, _)| now).step_by(step).for_each(count);
    }

    /// The frames' size in pixels; `None` before the first.
    pub fn size(&self) -> Option<(usize, usize)> {
        (!self.kept.is_empty()).then_some((self.width, self.height))
    }

    /// The GIF of the frames, looping, in `quality`; and how many frames
    /// it has.
    pub fn encode(&self, quality: Quality) -> Result<(Vec<u8>, usize)> {
        anyhow::ensure!(!self.mixed, "a frame of another size");
        let (w, h) = (self.width, self.height);
        let width = u16::try_from(w).context("too wide for a GIF")?;
        let height = u16::try_from(h).context("too high for a GIF")?;
        // The last place: a pixel that stays as it was. The table is as
        // long as the places asked for, rounded up to whole bits of a code.
        let places = quality.colors.clamp(2, 256);
        let clear = u8::try_from(places - 1).unwrap_or(u8::MAX);
        let mut flat = table(&self.seen, usize::from(clear));
        flat.resize(usize::from(places).next_power_of_two() * 3, 0);
        let colors = flat[..usize::from(clear) * 3].to_vec();
        let mut near: HashMap<[u8; 3], u8> = HashMap::new();
        let mut index = |pixel: &[u8]| -> u8 {
            *near
                .entry([pixel[0], pixel[1], pixel[2]])
                .or_insert_with(|| nearest(&colors, pixel))
        };

        let mut bytes = Vec::new();
        let mut count = 0;
        let mut encoder = gif::Encoder::new(&mut bytes, width, height, &flat)?;
        encoder.set_repeat(gif::Repeat::Infinite)?;
        // What the picture shows so far, as the frames meant it: a pixel is
        // drawn again once it strays from that, so small steps do not add
        // up.
        let mut shown: Option<Vec<u8>> = None;
        // The frame before, held back until it is known how long it stays.
        let mut held: Option<gif::Frame<'static>> = None;
        for (packed, stay) in &self.kept {
            let rgba = miniz_oxide::inflate::decompress_to_vec_with_limit(packed, w * h * 4)
                .map_err(|e| anyhow::anyhow!("a frame does not unpack: {e}"))?;
            // The part that changed: all of the first frame. A frame with
            // nothing to draw only makes the one before it stay longer.
            let part = match &shown {
                None => Some((0, 0, w, h)),
                Some(shown) => changed(shown, &rgba, w, h, quality.lossy),
            };
            let Some((left, top, right, bottom)) = part else {
                if let Some(last) = held.as_mut() {
                    last.delay = last.delay.saturating_add(*stay);
                }
                continue;
            };
            let first = shown.is_none();
            let screen = shown.get_or_insert_with(|| rgba.clone());
            let mut buffer = Vec::with_capacity((right - left) * (bottom - top));
            for y in top..bottom {
                for x in left..right {
                    let at = (y * w + x) * 4;
                    let pixel = &rgba[at..at + 4];
                    if !first && !differs(&screen[at..at + 4], pixel, quality.lossy) {
                        buffer.push(clear);
                    } else {
                        screen[at..at + 4].copy_from_slice(pixel);
                        buffer.push(index(pixel));
                    }
                }
            }
            if let Some(before) = held.take() {
                encoder.write_frame(&before)?;
                count += 1;
            }
            held = Some(gif::Frame {
                delay: *stay,
                dispose: gif::DisposalMethod::Keep,
                transparent: (!first).then_some(clear),
                left: u16::try_from(left).context("too wide for a GIF")?,
                top: u16::try_from(top).context("too high for a GIF")?,
                width: u16::try_from(right - left).context("too wide for a GIF")?,
                height: u16::try_from(bottom - top).context("too high for a GIF")?,
                buffer: Cow::Owned(buffer),
                ..gif::Frame::default()
            });
        }
        if let Some(last) = held {
            encoder.write_frame(&last)?;
            count += 1;
        }
        drop(encoder);
        Ok((bytes, count))
    }
}

/// The GIF of `frames` (RGBA, each with how long it stays, in hundredths
/// of a second), looping, in `quality`; and how many frames it has.
#[cfg(test)]
pub fn encode(
    width: u16,
    height: u16,
    frames: &[(Vec<u8>, u16)],
    quality: Quality,
) -> Result<(Vec<u8>, usize)> {
    let mut kept = Frames::default();
    for (rgba, stay) in frames {
        kept.push(usize::from(width), usize::from(height), rgba, *stay);
    }
    kept.encode(quality)
}

/// Whether `now` is in another step than `was` in a channel, the steps
/// `lossy + 1` wide: then it strays by up to `lossy` at most. A pixel is
/// judged by the step its color is in, not by the color it was drawn in,
/// so pixels of one color change together, whenever each was drawn last,
/// and a fade does not break into patches.
fn differs(was: &[u8], now: &[u8], lossy: u8) -> bool {
    let step = u16::from(lossy) + 1;
    was.iter()
        .zip(now)
        .any(|(a, b)| u16::from(*a) / step != u16::from(*b) / step)
}

/// At most `most` colors for all the frames, as RGB one after another,
/// from the colors `seen` in pixels spread over them: the colors
/// themselves where they are that few, else learned. A color counts by the
/// size of its number, not by the number, so the few pixels of a light
/// keep their color beside the many of the background.
fn table(seen: &HashMap<[u8; 3], u32>, most: usize) -> Vec<u8> {
    // In one order every time, so the same frames give the same file.
    let mut seen: Vec<([u8; 3], u32)> = seen.iter().map(|(c, n)| (*c, *n)).collect();
    seen.sort_unstable();
    if seen.len() <= most {
        return seen.iter().flat_map(|(color, _)| *color).collect();
    }
    let mut sample = Vec::new();
    for (color, count) in &seen {
        for _ in 0..=count.ilog2() {
            sample.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    NeuQuant::new(1, most, &sample).color_map_rgb()
}

/// The place of the color of `colors` nearest to `pixel`.
fn nearest(colors: &[u8], pixel: &[u8]) -> u8 {
    let far = |color: &[u8; 3]| -> u32 {
        (0..3)
            .map(|c| u32::from(color[c].abs_diff(pixel[c])).pow(2))
            .sum()
    };
    colors
        .as_chunks::<3>()
        .0
        .iter()
        .enumerate()
        .min_by_key(|(_, color)| far(color))
        .map_or(0, |(place, _)| place as u8)
}

/// The box around the pixels in which `now` strays from `before` by more
/// than `lossy`: left, top, right and bottom, the last two past it.
fn changed(
    before: &[u8],
    now: &[u8],
    w: usize,
    h: usize,
    lossy: u8,
) -> Option<(usize, usize, usize, usize)> {
    let row = |y: usize| y * w * 4..(y + 1) * w * 4;
    let row_differs = |y: usize| {
        let (a, b) = (&before[row(y)], &now[row(y)]);
        if lossy == 0 {
            a != b
        } else {
            differs(a, b, lossy)
        }
    };
    let top = (0..h).find(|&y| row_differs(y))?;
    let bottom = (top..h).rfind(|&y| row_differs(y))? + 1;
    let (mut left, mut right) = (w, 0);
    for y in top..bottom {
        let (a, b) = (&before[row(y)], &now[row(y)]);
        let strays = |x: &usize| differs(&a[x * 4..x * 4 + 4], &b[x * 4..x * 4 + 4], lossy);
        if let Some(first) = (0..left).find(strays) {
            left = first;
        }
        if let Some(last) = (right..w).rfind(strays) {
            right = last + 1;
        }
    }
    Some((left, top, right, bottom))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: usize, h: usize, color: [u8; 4]) -> Vec<u8> {
        color.repeat(w * h)
    }

    #[test]
    fn only_what_changed_is_written_and_the_picture_comes_back() {
        let (w, h) = (40, 30);
        let first = picture(w, h, [26, 27, 38, 255]);
        let mut second = first.clone();
        for (x, y) in [(10, 12), (17, 20)] {
            let at = (y * w + x) * 4;
            second[at..at + 4].copy_from_slice(&[255, 158, 100, 255]);
        }
        assert_eq!(changed(&first, &second, w, h, 0), Some((10, 12, 18, 21)));
        assert_eq!(changed(&first, &first, w, h, 0), None);

        let frames = vec![(first, 5), (second.clone(), 50), (second.clone(), 5)];
        let (bytes, count) = encode(40, 30, &frames, Quality::default()).unwrap();
        assert_eq!(count, 2);
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::Indexed);
        let mut decoder = options.read_info(bytes.as_slice()).unwrap();
        let palette = decoder.global_palette().unwrap().to_vec();
        let mut screen = vec![0u8; w * h * 3];
        let mut sizes = Vec::new();
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            sizes.push((frame.width, frame.height, frame.delay));
            for (i, &index) in frame.buffer.iter().enumerate() {
                if Some(index) == frame.transparent {
                    continue;
                }
                let x = usize::from(frame.left) + i % usize::from(frame.width);
                let y = usize::from(frame.top) + i / usize::from(frame.width);
                let color = &palette[usize::from(index) * 3..usize::from(index) * 3 + 3];
                screen[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(color);
            }
        }
        // A frame like the one before it makes that one stay longer.
        assert_eq!(sizes, [(40, 30, 5), (8, 9, 55)]);
        // Few colors come back as they were.
        for (shown, meant) in screen.chunks(3).zip(second.chunks(4)) {
            for c in 0..3 {
                assert_eq!(shown[c], meant[c], "{shown:?} for {meant:?}");
            }
        }
    }

    /// Fewer colors make shorter codes, and a pixel that stays in its step
    /// is not drawn again: a slow fade is a few frames, the file smaller,
    /// and nothing drifts further than the steps allow.
    #[test]
    fn colors_and_steps_make_it_smaller() {
        let (w, h) = (64, 48);
        let mut frames = Vec::new();
        for step in 0..40u8 {
            let mut rgba = Vec::with_capacity(w * h * 4);
            for y in 0..h {
                for x in 0..w {
                    let shade = ((x * 3 + y * 2) as u8).wrapping_add(step);
                    rgba.extend_from_slice(&[shade, shade / 2, 255 - shade, 255]);
                }
            }
            frames.push((rgba, 5));
        }
        let size = |quality: Quality| encode(64, 48, &frames, quality).unwrap().0.len();
        let full = size(Quality::default());
        let few = size(Quality {
            colors: 32,
            lossy: 0,
        });
        let steps = size(Quality {
            colors: 256,
            lossy: 12,
        });
        assert!(few < full, "{few} {full}");
        // 32 places are 31 colors and the clear one, in codes of 5 bits.
        let (bytes, _) = encode(
            64,
            48,
            &frames,
            Quality {
                colors: 32,
                lossy: 0,
            },
        )
        .unwrap();
        let decoder = gif::DecodeOptions::new()
            .read_info(bytes.as_slice())
            .unwrap();
        assert_eq!(decoder.global_palette().unwrap().len(), 32 * 3);
        assert!(steps < full * 2 / 3, "{steps} {full}");
        // What the last frame shows strays from it by no more than the steps.
        let (bytes, _) = encode(
            64,
            48,
            &frames,
            Quality {
                colors: 255,
                lossy: 12,
            },
        )
        .unwrap();
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(bytes.as_slice()).unwrap();
        let mut screen = vec![0u8; w * h * 4];
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            let fw = usize::from(frame.width);
            for (i, pixel) in frame.buffer.chunks(4).enumerate() {
                if pixel[3] == 0 {
                    continue;
                }
                let x = usize::from(frame.left) + i % fw;
                let y = usize::from(frame.top) + i / fw;
                screen[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(pixel);
            }
        }
        let last = &frames.last().unwrap().0;
        let worst = screen
            .iter()
            .zip(last)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        // The steps, and what the table's colors are off by.
        assert!(worst <= 12 + 16, "{worst}");
    }
}
