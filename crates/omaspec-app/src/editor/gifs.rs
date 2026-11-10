//! Frames as a GIF that stays small: one table of colors for the whole
//! picture, and of each frame only the part that differs from the one
//! before it, the pixels that stayed left clear in it.
//!
//! A table per frame, as `gif::Frame::from_rgba` makes one, costs its
//! bytes each time and lets a color flicker from frame to frame; a whole
//! frame each time is most of a file in which only a light moves.

use std::borrow::Cow;
use std::collections::HashMap;

use anyhow::{Context, Result};
use color_quant::NeuQuant;

/// The table's last place: a pixel that stays as it was.
const CLEAR: u8 = 255;
/// How many pixels the table of colors is learned from, about.
const LEARNED: usize = 1 << 20;

/// The GIF of `frames` (RGBA, each with how long it stays, in hundredths
/// of a second), looping.
pub fn encode(width: u16, height: u16, frames: &[(Vec<u8>, u16)]) -> Result<Vec<u8>> {
    let (w, h) = (usize::from(width), usize::from(height));
    anyhow::ensure!(
        frames.iter().all(|(rgba, _)| rgba.len() == w * h * 4),
        "a frame of another size"
    );
    let mut flat = table(frames, w * h);
    flat.resize(256 * 3, 0);
    let colors = flat[..usize::from(CLEAR) * 3].to_vec();
    let mut near: HashMap<[u8; 3], u8> = HashMap::new();
    let mut index = |pixel: &[u8]| -> u8 {
        *near
            .entry([pixel[0], pixel[1], pixel[2]])
            .or_insert_with(|| nearest(&colors, pixel))
    };

    let mut bytes = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut bytes, width, height, &flat)?;
        encoder.set_repeat(gif::Repeat::Infinite)?;
        let mut before: Option<&[u8]> = None;
        for (rgba, stay) in frames {
            // The part that changed: all of the first frame, and at least
            // a pixel of one that only stays.
            let (left, top, right, bottom) = match before {
                None => (0, 0, w, h),
                Some(before) => changed(before, rgba, w, h).unwrap_or((0, 0, 1, 1)),
            };
            let mut buffer = Vec::with_capacity((right - left) * (bottom - top));
            for y in top..bottom {
                for x in left..right {
                    let at = (y * w + x) * 4;
                    let pixel = &rgba[at..at + 4];
                    let stays = before.is_some_and(|b| &b[at..at + 4] == pixel);
                    buffer.push(if stays { CLEAR } else { index(pixel) });
                }
            }
            let frame = gif::Frame {
                delay: *stay,
                dispose: gif::DisposalMethod::Keep,
                transparent: before.is_some().then_some(CLEAR),
                left: u16::try_from(left).context("too wide for a GIF")?,
                top: u16::try_from(top).context("too high for a GIF")?,
                width: u16::try_from(right - left).context("too wide for a GIF")?,
                height: u16::try_from(bottom - top).context("too high for a GIF")?,
                buffer: Cow::Owned(buffer),
                ..gif::Frame::default()
            };
            encoder.write_frame(&frame)?;
            before = Some(rgba);
        }
    }
    Ok(bytes)
}

/// At most 255 colors for all the frames, as RGB one after another,
/// from pixels spread over them: the colors themselves where they are
/// that few, else learned. A color counts by the size of its number, not
/// by the number, so the few pixels of a light keep their color beside
/// the many of the background.
fn table(frames: &[(Vec<u8>, u16)], pixels: usize) -> Vec<u8> {
    let step = (frames.len() * pixels / LEARNED).max(1);
    let mut seen: HashMap<[u8; 3], u32> = HashMap::new();
    let mut at = 0;
    for (rgba, _) in frames {
        while at < pixels {
            *seen
                .entry([rgba[at * 4], rgba[at * 4 + 1], rgba[at * 4 + 2]])
                .or_default() += 1;
            at += step;
        }
        at -= pixels;
    }
    // In one order every time, so the same frames give the same file.
    let mut seen: Vec<([u8; 3], u32)> = seen.into_iter().collect();
    seen.sort_unstable();
    if seen.len() <= usize::from(CLEAR) {
        return seen.iter().flat_map(|(color, _)| *color).collect();
    }
    let mut sample = Vec::new();
    for (color, count) in &seen {
        for _ in 0..=count.ilog2() {
            sample.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    NeuQuant::new(1, usize::from(CLEAR), &sample).color_map_rgb()
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

/// The box around the pixels in which `now` differs from `before`: left,
/// top, right and bottom, the last two past it.
fn changed(before: &[u8], now: &[u8], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let row = |y: usize| y * w * 4..(y + 1) * w * 4;
    let top = (0..h).find(|&y| before[row(y)] != now[row(y)])?;
    let bottom = (top..h).rfind(|&y| before[row(y)] != now[row(y)])? + 1;
    let (mut left, mut right) = (w, 0);
    for y in top..bottom {
        let (a, b) = (&before[row(y)], &now[row(y)]);
        let differs = |x: &usize| a[x * 4..x * 4 + 4] != b[x * 4..x * 4 + 4];
        if let Some(first) = (0..left).find(differs) {
            left = first;
        }
        if let Some(last) = (right..w).rfind(differs) {
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
        assert_eq!(changed(&first, &second, w, h), Some((10, 12, 18, 21)));
        assert_eq!(changed(&first, &first, w, h), None);

        let frames = vec![(first, 5), (second.clone(), 50), (second.clone(), 5)];
        let bytes = encode(40, 30, &frames).unwrap();
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
        assert_eq!(sizes, [(40, 30, 5), (8, 9, 50), (1, 1, 5)]);
        // Few colors come back as they were.
        for (shown, meant) in screen.chunks(3).zip(second.chunks(4)) {
            for c in 0..3 {
                assert_eq!(shown[c], meant[c], "{shown:?} for {meant:?}");
            }
        }
    }
}
