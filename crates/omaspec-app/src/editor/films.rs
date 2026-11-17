//! Frames as a picture that moves in full color: an animated PNG or an
//! animated WebP, for a glide or a fade that a GIF's 256 colors break
//! into bands, at up to 60 frames a second where a GIF has 50.
//!
//! Both are written without loss, and as the GIF is ([`super::gifs`]): of
//! each frame only the box around what differs from the one before, the
//! pixels that stayed left clear in it, laid over what is shown. A WebP is
//! the smaller file; an animated PNG is shown as a still picture by a
//! viewer that does not know it moves. A fold can also be a page that
//! plays it in vectors ([`Film::Page`]).

use std::path::Path;

use anyhow::{Context, Result};

use super::gifs::{Frames, changed};

/// What a picture that moves is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Film {
    Gif,
    Apng,
    Webp,
    /// A page that plays a fold in vectors ([`super::moving::fold_page`]).
    Page,
}

impl Film {
    /// By the file's ending: `.gif`, `.png` or `.apng`, `.webp`.
    pub fn of(path: &Path) -> Option<Film> {
        let ending = path.extension()?.to_str()?.to_ascii_lowercase();
        match ending.as_str() {
            "gif" => Some(Film::Gif),
            "png" | "apng" => Some(Film::Apng),
            "webp" => Some(Film::Webp),
            "html" | "htm" => Some(Film::Page),
            _ => None,
        }
    }

    /// The frames a second it has when none are asked for: a GIF's file
    /// grows with each, a picture in full color is asked for to be smooth.
    pub fn usual_fps(self) -> u32 {
        match self {
            Film::Gif => 20,
            Film::Apng | Film::Webp | Film::Page => 60,
        }
    }

    /// The most frames a second it holds: a GIF counts in hundredths of a
    /// second, and viewers slow one down that asks for less than two.
    pub fn most_fps(self) -> u32 {
        match self {
            Film::Gif => 50,
            Film::Apng | Film::Webp | Film::Page => 60,
        }
    }
}

/// One frame's part to write: where it lies, its pixels (RGBA), whether
/// they are laid over what is shown (a clear one keeps it) or replace it,
/// and for how many ticks it stays.
struct Part {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    rgba: Vec<u8>,
    over: bool,
    ticks: u16,
}

/// Hands each frame's part to `write`, in order. With `even`, a part
/// starts at an even place, as a WebP has it.
fn parts(frames: &Frames, even: bool, mut write: impl FnMut(Part) -> Result<()>) -> Result<()> {
    let (w, h) = frames.size().context("no frame")?;
    let mut shown: Option<Vec<u8>> = None;
    frames.each(|rgba, ticks| {
        let Some(before) = &shown else {
            write(Part {
                left: 0,
                top: 0,
                width: w,
                height: h,
                rgba: rgba.to_vec(),
                over: false,
                ticks,
            })?;
            shown = Some(rgba.to_vec());
            return Ok(());
        };
        // Frames are kept only where they differ from the one before.
        let (mut left, mut top, right, bottom) =
            changed(before, rgba, w, h, 0).context("a frame like the one before it")?;
        if even {
            (left, top) = (left & !1, top & !1);
        }
        // A pixel that is not opaque cannot be laid over what is shown.
        let over = rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255);
        let mut part = Vec::with_capacity((right - left) * (bottom - top) * 4);
        for y in top..bottom {
            let row = (y * w + left) * 4..(y * w + right) * 4;
            let (now, was) = (&rgba[row.clone()], &before[row]);
            for (pixel, old) in now.as_chunks::<4>().0.iter().zip(was.as_chunks::<4>().0) {
                if over && pixel == old {
                    part.extend_from_slice(&[0; 4]);
                } else {
                    part.extend_from_slice(pixel);
                }
            }
        }
        write(Part {
            left,
            top,
            width: right - left,
            height: bottom - top,
            rgba: part,
            over,
            ticks,
        })?;
        shown = Some(rgba.to_vec());
        Ok(())
    })
}

/// The animated PNG of `frames`, looping, a tick the `fps`th of a second;
/// and how many frames it has.
pub fn apng(frames: &Frames, fps: u32) -> Result<(Vec<u8>, usize)> {
    let (w, h) = frames.size().context("no frame")?;
    let count = frames.len();
    let per_second = u16::try_from(fps).context("too many frames a second")?;
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(
        &mut bytes,
        u32::try_from(w).context("too wide")?,
        u32::try_from(h).context("too high")?,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    if count > 1 {
        encoder.set_animated(u32::try_from(count).context("too many frames")?, 0)?;
    }
    let mut writer = encoder.write_header()?;
    parts(frames, false, |part| {
        if count > 1 {
            // Each is held against the other as it is set: from the
            // corner, where any size fits.
            writer.set_frame_position(0, 0)?;
            writer.set_frame_dimension(part.width as u32, part.height as u32)?;
            writer.set_frame_position(part.left as u32, part.top as u32)?;
            writer.set_frame_delay(part.ticks, per_second)?;
            writer.set_blend_op(if part.over {
                png::BlendOp::Over
            } else {
                png::BlendOp::Source
            })?;
            writer.set_dispose_op(png::DisposeOp::None)?;
        }
        writer.write_image_data(&part.rgba)?;
        Ok(())
    })?;
    writer.finish()?;
    Ok((bytes, count))
}

/// The animated WebP of `frames`, looping, a tick the `fps`th of a second;
/// and how many frames it has. Each frame is a lossless picture of its
/// own (`image-webp`), in the container this writes around them.
pub fn webp(frames: &Frames, fps: u32) -> Result<(Vec<u8>, usize)> {
    /// A WebP's side has 14 bits without loss.
    const MOST: usize = 16_384;
    let (w, h) = frames.size().context("no frame")?;
    anyhow::ensure!(w <= MOST && h <= MOST, "too large for a WebP");
    let three = |value: usize| -> [u8; 3] {
        let bytes = (value as u32).to_le_bytes();
        [bytes[0], bytes[1], bytes[2]]
    };
    let mut body = Vec::new();
    // Animated, and with pixels that are clear.
    let mut head = vec![0x02 | 0x10, 0, 0, 0];
    head.extend_from_slice(&three(w - 1));
    head.extend_from_slice(&three(h - 1));
    chunk(&mut body, b"VP8X", &head);
    // No ground of its own, and no end to its loop.
    chunk(&mut body, b"ANIM", &[0; 6]);
    // Whole milliseconds: each frame ends where its ticks end, so the
    // thirds of a 60th do not add up.
    let mut ticks = 0u64;
    let mut count = 0;
    parts(frames, true, |part| {
        let from = ticks * 1000 / u64::from(fps);
        ticks += u64::from(part.ticks);
        let stays = (ticks * 1000 / u64::from(fps) - from).min((1 << 24) - 1);
        let mut picture = Vec::new();
        image_webp::WebPEncoder::new(&mut picture)
            .encode(
                &part.rgba,
                part.width as u32,
                part.height as u32,
                image_webp::ColorType::Rgba8,
            )
            .map_err(|e| anyhow::anyhow!("a frame as WebP: {e}"))?;
        // Past the file's own head (`RIFF`, its length, `WEBP`): the
        // picture's chunk.
        let picture = picture.get(12..).context("a frame as WebP")?;
        let mut frame = Vec::with_capacity(16 + picture.len());
        frame.extend_from_slice(&three(part.left / 2));
        frame.extend_from_slice(&three(part.top / 2));
        frame.extend_from_slice(&three(part.width - 1));
        frame.extend_from_slice(&three(part.height - 1));
        frame.extend_from_slice(&three(stays as usize));
        // Kept when the next comes; laid over what is shown, or in its
        // place.
        frame.push(if part.over { 0 } else { 0x02 });
        frame.extend_from_slice(picture);
        chunk(&mut body, b"ANMF", &frame);
        count += 1;
        Ok(())
    })?;
    let mut bytes = Vec::with_capacity(body.len() + 12);
    bytes.extend_from_slice(b"RIFF");
    let length = u32::try_from(body.len() + 4).context("too large for a WebP")?;
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(&body);
    Ok((bytes, count))
}

/// A chunk of a RIFF file: its name, its length, its bytes, and one more
/// where they are odd.
fn chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(name);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three frames 40 by 30: a ground, two lights on it, and one of them
    /// gone again; the second stays ten times as long.
    fn frames() -> (Frames, Vec<Vec<u8>>) {
        let (w, h) = (40, 30);
        let ground: Vec<u8> = [26, 27, 38, 255].repeat(w * h);
        let mut lit = ground.clone();
        for (x, y) in [(11, 13), (17, 20)] {
            let at = (y * w + x) * 4;
            lit[at..at + 4].copy_from_slice(&[255, 158, 100, 255]);
        }
        let mut one = lit.clone();
        let at = (13 * w + 11) * 4;
        one[at..at + 4].copy_from_slice(&[26, 27, 38, 255]);
        let mut kept = Frames::default();
        kept.push(w, h, &ground, 1);
        kept.push(w, h, &lit, 10);
        kept.push(w, h, &one, 1);
        // One like the frame before only makes that one stay longer.
        kept.push(w, h, &one, 1);
        (kept, vec![ground, lit, one])
    }

    #[test]
    fn a_film_is_told_by_its_ending() {
        assert_eq!(Film::of(Path::new("fold.gif")), Some(Film::Gif));
        assert_eq!(Film::of(Path::new("fold.PNG")), Some(Film::Apng));
        assert_eq!(Film::of(Path::new("fold.apng")), Some(Film::Apng));
        assert_eq!(Film::of(Path::new("a/fold.webp")), Some(Film::Webp));
        assert_eq!(Film::of(Path::new("fold.html")), Some(Film::Page));
        assert_eq!(Film::of(Path::new("fold.mp4")), None);
        assert_eq!(Film::of(Path::new("fold")), None);
    }

    #[test]
    fn an_animated_webp_comes_back_as_it_was_drawn() {
        let (kept, drawn) = frames();
        let (bytes, count) = webp(&kept, 60).unwrap();
        assert_eq!(count, 3);
        let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
        assert!(decoder.is_animated());
        assert_eq!(decoder.dimensions(), (40, 30));
        assert_eq!(decoder.num_frames(), 3);
        assert_eq!(decoder.loop_count(), image_webp::LoopCount::Forever);
        let mut stays = Vec::new();
        for meant in &drawn {
            let mut shown = vec![0; 40 * 30 * 4];
            stays.push(decoder.read_frame(&mut shown).unwrap());
            let differs: Vec<_> = shown
                .chunks(4)
                .zip(meant.chunks(4))
                .enumerate()
                // This reader lays an opaque pixel over the one under it
                // by a division that loses one; libwebp copies it.
                .filter(|(_, (a, b))| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 1))
                .map(|(i, (a, b))| (i % 40, i / 40, a.to_vec(), b.to_vec()))
                .take(4)
                .collect();
            assert!(differs.is_empty(), "frame {}: {differs:?}", stays.len());
        }
        // A 60th is 16 and two thirds of a millisecond: 16, then up to
        // the 183 the first eleven take, then up to the 216 of all 13.
        assert_eq!(stays, [16, 167, 33]);
        // Only what changed is in the later frames: a few bytes each.
        assert!(bytes.len() < 700, "{}", bytes.len());
    }

    #[test]
    fn an_animated_png_comes_back_as_it_was_drawn() {
        let (kept, drawn) = frames();
        let (bytes, count) = apng(&kept, 60).unwrap();
        assert_eq!(count, 3);
        let mut reader = png::Decoder::new(std::io::Cursor::new(&bytes))
            .read_info()
            .unwrap();
        let animation = reader.info().animation_control.unwrap();
        assert_eq!((animation.num_frames, animation.num_plays), (3, 0));
        let mut screen = vec![0u8; 40 * 30 * 4];
        let mut stays = Vec::new();
        for meant in &drawn {
            let mut part = vec![0; reader.output_buffer_size().unwrap()];
            let read = reader.next_frame(&mut part).unwrap();
            let frame = reader.info().frame_control.unwrap();
            stays.push((frame.delay_num, frame.delay_den));
            let (fw, left, top) = (
                frame.width as usize,
                frame.x_offset as usize,
                frame.y_offset as usize,
            );
            for (i, pixel) in part[..read.buffer_size()].chunks(4).enumerate() {
                if frame.blend_op == png::BlendOp::Over && pixel[3] == 0 {
                    continue;
                }
                let at = ((top + i / fw) * 40 + left + i % fw) * 4;
                screen[at..at + 4].copy_from_slice(pixel);
            }
            assert!(&screen == meant, "a frame differs");
        }
        assert_eq!(stays, [(1, 60), (10, 60), (2, 60)]);
        // The second frame is the box around the two lights.
        assert!(bytes.len() < 500, "{}", bytes.len());
    }

    /// One frame is a picture that stands: a plain PNG, a WebP of one frame.
    #[test]
    fn one_frame_is_a_picture() {
        let mut kept = Frames::default();
        kept.push(4, 4, &[9, 9, 9, 255].repeat(16), 1);
        let (bytes, count) = apng(&kept, 30).unwrap();
        assert_eq!(count, 1);
        let reader = png::Decoder::new(std::io::Cursor::new(&bytes))
            .read_info()
            .unwrap();
        assert!(reader.info().animation_control.is_none());
        assert_eq!(webp(&kept, 30).unwrap().1, 1);
    }
}
