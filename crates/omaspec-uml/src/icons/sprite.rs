//! PlantUML's sprites: small gray pictures a diagram defines
//! (`sprite $bolt [16x16/16] { … }`) and puts into its text (`<$bolt>`,
//! `<$bolt*2>`, `<$bolt,color=red>`). They are drawn in the text's color,
//! each pixel as dark as its level says (`0` is the page), so they read in
//! any theme as the words do.
//!
//! The definitions are taken out after the preprocessor has run, and each
//! use is written into the text with its pixels
//! (`<$:16x16:00F…>`), so a label carries its sprite wherever it is
//! measured and drawn. The forms read are those of `-encodesprite`: 16
//! levels a hex digit for each pixel, 8 levels two rows to a character and
//! 4 levels three, and each of them compressed (`z`, raw deflate of a byte
//! a pixel, in PlantUML's 64 letters). SVG and color sprites decline.

use std::collections::HashMap;

use crate::Declined;
use crate::preprocess::Owned;
use crate::source::Line;

/// PlantUML's letters for six bits, in order.
const LETTERS: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-_";
/// The most pixels a sprite may have.
const MAX_PIXELS: usize = 256 * 256;

/// A sprite's pixels: how dark each is, 0 (the page) to 15 (the ink).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub width: usize,
    pub height: usize,
    pub levels: Vec<u8>,
}

impl Sprite {
    /// How a use carries it in the text: `16x16:` and a hex digit a pixel.
    fn inline(&self) -> String {
        let hex: String = self
            .levels
            .iter()
            .map(|&v| char::from_digit(u32::from(v.min(15)), 16).unwrap_or('0'))
            .collect();
        format!("{}x{}:{hex}", self.width, self.height)
    }

    /// The sprite a use carries (`16x16:00F…`), as [`Self::inline`] wrote it.
    pub fn read_inline(text: &str) -> Option<Sprite> {
        let (size, hex) = text.split_once(':')?;
        let (w, h) = size.split_once('x')?;
        let (width, height): (usize, usize) = (w.parse().ok()?, h.parse().ok()?);
        let levels: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<_>>()?;
        (width * height == levels.len() && width > 0 && height > 0).then_some(Sprite {
            width,
            height,
            levels,
        })
    }
}

/// The sprites a diagram defines, by name.
pub(crate) type Sprites = HashMap<String, Sprite>;

/// `lines` without their sprite definitions, each use of one written with
/// its pixels, and the sprites; `None` when they define none and use none.
pub(crate) fn take(lines: &[Line]) -> Result<Option<(Vec<Owned>, Sprites)>, Declined> {
    let defines = |line: &Line| head(line.text).is_some();
    if !lines
        .iter()
        .any(|line| defines(line) || line.text.contains('$'))
    {
        return Ok(None);
    }
    let mut sprites: HashMap<String, Sprite> = HashMap::new();
    let mut out: Vec<Owned> = Vec::with_capacity(lines.len());
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let Some((name, rest)) = head(line.text) else {
            out.push(Owned::from(*line));
            i += 1;
            continue;
        };
        let at = |reason: String| Declined::at(line.no, reason);
        if rest.starts_with('<') {
            return Err(at(format!("the SVG sprite `${name}` is not supported yet")));
        }
        let Some((form, data)) = rest
            .strip_prefix('[')
            .and_then(|r| r.split_once(']'))
            .map(|(form, data)| (form.trim(), data.trim()))
        else {
            return Err(at(format!(
                "sprite `${name}` needs its size and levels, as `[16x16/16]`"
            )));
        };
        let form = Form::read(form).ok_or_else(|| {
            at(format!(
                "`[{form}]`: a sprite is `[WxH/16]`, `/8` or `/4`, `z` when compressed; color sprites are not supported yet"
            ))
        })?;
        let sprite = if let Some(first) = data.strip_prefix('{') {
            // Rows up to the closing brace, on the lines below or here.
            let mut rows: Vec<String> = Vec::new();
            // Whether the row text closes the braces; its pixels kept.
            let push = |text: &str, rows: &mut Vec<String>| {
                let text = text.trim();
                let (row, closed) = match text.strip_suffix('}') {
                    Some(before) => (before.trim(), true),
                    None => (text, false),
                };
                if !row.is_empty() {
                    rows.push(row.to_owned());
                }
                closed
            };
            let mut closed = push(first, &mut rows);
            while !closed {
                i += 1;
                let Some(next) = lines.get(i) else {
                    return Err(at(format!("sprite `${name}` has no closing `}}`")));
                };
                closed = push(next.text, &mut rows);
            }
            form.rows(&rows)
        } else {
            form.packed(data)
        }
        .map_err(|why| at(format!("sprite `${name}`: {why}")))?;
        sprites.insert(name.to_owned(), sprite);
        i += 1;
    }
    for line in &mut out {
        if line.text.contains('$') {
            line.text = uses(&line.text, &sprites).map_err(|why| Declined::at(line.no, why))?;
        }
    }
    Ok(Some((out, sprites)))
}

/// The name a `sprite` line defines and what follows it.
fn head(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("sprite")?;
    if !rest.starts_with([' ', '\t']) {
        return None;
    }
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('$').unwrap_or(rest);
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.')))
        .unwrap_or(rest.len());
    let (name, after) = rest.split_at(end);
    (!name.is_empty()).then_some((name, after.trim_start()))
}

/// `text` with each `<$name…>` and `<#color$name…>` carrying its sprite;
/// a stereotype (`<<$name>>`) stays as written.
pub(crate) fn uses(text: &str, sprites: &Sprites) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        // A color before the name: `#red`, `#ff0000`.
        let color = after.strip_prefix('#').map_or(0, |c| {
            1 + c
                .find(|ch: char| !ch.is_ascii_alphanumeric())
                .unwrap_or(c.len())
        });
        // A stereotype's name stays as written.
        if let Some(inner) = after.strip_prefix('<') {
            out.push_str("<<");
            rest = inner;
            continue;
        }
        let Some(named) = after[color..].strip_prefix('$') else {
            out.push('<');
            rest = after;
            continue;
        };
        let end = named
            .find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/')))
            .unwrap_or(named.len());
        let name = &named[..end];
        if name.is_empty() {
            out.push('<');
            rest = after;
            continue;
        }
        let Some(sprite) = sprites.get(name) else {
            // PlantUML's own: ArchiMate's icons stay as written.
            if name.starts_with("archimate/") {
                out.push('<');
                out.push_str(&after[..color]);
                out.push('$');
                out.push_str(name);
                rest = &named[end..];
                continue;
            }
            return Err(format!("`<${name}>`: no sprite of that name is defined"));
        };
        out.push('<');
        out.push_str(&after[..color]);
        out.push_str("$:");
        out.push_str(&sprite.inline());
        rest = &named[end..];
    }
    out.push_str(rest);
    Ok(out)
}

/// How a sprite's pixels are written.
#[derive(Debug, Clone, Copy)]
struct Form {
    width: usize,
    height: usize,
    /// 16, 8 or 4 levels of gray.
    levels: u8,
    compressed: bool,
}

impl Form {
    /// `16x16/16`, `32x32/8z`.
    fn read(text: &str) -> Option<Form> {
        let (size, levels) = text.split_once('/')?;
        let (w, h) = size.split_once('x')?;
        let (width, height): (usize, usize) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
        let (levels, compressed) = match levels.trim().strip_suffix('z') {
            Some(levels) => (levels, true),
            None => (levels.trim(), false),
        };
        let levels: u8 = levels.parse().ok().filter(|l| matches!(l, 4 | 8 | 16))?;
        (width > 0 && height > 0 && width * height <= MAX_PIXELS).then_some(Form {
            width,
            height,
            levels,
            compressed,
        })
    }

    /// The pixels written as rows between braces.
    fn rows(self, rows: &[String]) -> Result<Sprite, String> {
        if self.compressed {
            return self.packed(&rows.concat());
        }
        let mut levels = vec![0u8; self.width * self.height];
        // Rows of pixels that one line of text holds, and the bits of each.
        let (stacked, bits) = match self.levels {
            16 => (1, 4),
            8 => (2, 3),
            _ => (3, 2),
        };
        for (n, row) in rows.iter().enumerate() {
            for (x, c) in row.chars().enumerate().take(self.width) {
                let value = if self.levels == 16 {
                    c.to_digit(16)
                        .ok_or_else(|| format!("`{c}` is no hex digit"))?
                } else {
                    letter(c).ok_or_else(|| format!("`{c}` is no letter of a sprite"))?
                };
                for k in 0..stacked {
                    let y = n * stacked + k;
                    if y >= self.height {
                        break;
                    }
                    let shift = bits * (stacked - 1 - k);
                    let v = (value >> shift) & ((1 << bits) - 1);
                    levels[y * self.width + x] = self.to_16(v as u8);
                }
            }
        }
        Ok(Sprite {
            width: self.width,
            height: self.height,
            levels,
        })
    }

    /// The pixels compressed: PlantUML's letters, six bits each, into
    /// bytes, inflated to a byte a pixel. A stream cut short leaves the
    /// rest of the pixels as the page.
    fn packed(self, data: &str) -> Result<Sprite, String> {
        if !self.compressed {
            return Err("its rows go between `{` and `}`".into());
        }
        let mut bits: u32 = 0;
        let mut held = 0;
        let mut bytes = Vec::with_capacity(data.len() * 3 / 4 + 1);
        for c in data.chars().filter(|c| !c.is_whitespace()) {
            let v = letter(c).ok_or_else(|| format!("`{c}` is no letter of a sprite"))?;
            bits = (bits << 6) | v;
            held += 6;
            if held >= 8 {
                held -= 8;
                bytes.push((bits >> held) as u8);
                bits &= (1 << held) - 1;
            }
        }
        let size = self.width * self.height;
        let raw = match miniz_oxide::inflate::decompress_to_vec_with_limit(&bytes, size) {
            Ok(raw) => raw,
            Err(err) if !err.output.is_empty() => err.output,
            Err(_) => return Err("its data cannot be read".into()),
        };
        let mut levels: Vec<u8> = raw.iter().map(|&v| self.to_16(v)).collect();
        levels.resize(size, 0);
        Ok(Sprite {
            width: self.width,
            height: self.height,
            levels,
        })
    }

    /// A level of this form on the scale of 16.
    fn to_16(self, v: u8) -> u8 {
        let top = self.levels - 1;
        ((u32::from(v.min(top)) * 15 + u32::from(top) / 2) / u32::from(top)) as u8
    }
}

fn letter(c: char) -> Option<u32> {
    LETTERS
        .iter()
        .position(|&l| l as char == c)
        .map(|p| p as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dot over a bar, as `plantuml -encodesprite` writes it.
    const DOT16: &str = "sprite $dot [16x16/16] {\n0000000000000000\n0000000000000000\n00000039F9300000\n00002FFFFFFF2000\n0002FFFFFFFFF200\n000FFFFFFFFFFF00\n003FFFFFFFFFFF30\n009FFFFFFFFFFF90\n00FFFFFFFFFFFFF0\n009FFFFFFFFFFF90\n003FFFFFFFFFFF30\n000FFFFFFFFFFF00\n0002FFFFFFFFF200\n0044444444444400\n0044444444444400\n0000000000000000\n}";

    fn lines(text: &str) -> Vec<Owned> {
        let raw: Vec<Line> = text
            .lines()
            .enumerate()
            .map(|(no, text)| Line {
                no,
                text: text.trim(),
                indent: 0,
                blank_before: 0,
            })
            .collect();
        take(&raw).unwrap().unwrap().0
    }

    fn sprite_of(text: &str) -> Sprite {
        let out = lines(&format!("{text}\nA : <$dot>"));
        let used = &out.last().unwrap().text;
        let inline = used
            .strip_prefix("A : <$:")
            .unwrap()
            .strip_suffix('>')
            .unwrap();
        Sprite::read_inline(inline).unwrap()
    }

    #[test]
    fn every_form_of_a_sprite_reads_the_same_picture() {
        let dot = sprite_of(DOT16);
        assert_eq!((dot.width, dot.height), (16, 16));
        assert_eq!(dot.levels[2 * 16 + 8], 15);
        assert_eq!(dot.levels[0], 0);
        assert_eq!(dot.levels[13 * 16 + 3], 4);
        // Compressed, the stream cut short as PlantUML writes it.
        let z = sprite_of(
            "sprite $dot [16x16/16z] Oo0AC7FoSpB3UKpy4C04vw88y2C0M2yI7sG69nAV4w8S0T3bCVIZQi1cFmiImE1Z",
        );
        assert_eq!(z, dot);
        // Eight levels, two rows to a letter; four, three.
        let eight = sprite_of(
            "sprite $dot [16x16/8z] Oo0ACBAmip0YUEmGm0ZdeWYm8m126g601OdF0b6E0EZo6FfHDM2pdma9OF0n",
        );
        assert_eq!(eight.levels[2 * 16 + 8], 15);
        assert_eq!(eight.levels[2 * 16 + 7], 9, "4 of 7");
        let rows = sprite_of("sprite $dot [16x16/8] {\n0000000000000000\n000017Fd_dF71000\n}");
        assert_eq!(rows.levels[2 * 16 + 7], 9);
        assert_eq!(rows.levels[3 * 16 + 5], 15);
        let four = sprite_of("sprite $dot [16x16/4] {\n0000000232000000\n0003F_______F300\n}");
        assert_eq!(four.levels[2 * 16 + 8], 15);
        assert_eq!(four.levels[5 * 16 + 3], 15);
        assert_eq!(four.levels[3 * 16 + 3], 0);
    }

    #[test]
    fn a_definition_is_taken_out_and_its_uses_carry_it() {
        let out = lines(&format!(
            "{DOT16}\nrectangle \"<$dot*2> Power\" as p <<$dot>>\nA -> B : <#red$dot>"
        ));
        assert!(out.iter().all(|line| !line.text.starts_with("sprite")));
        let card = &out[out.len() - 2].text;
        assert!(card.starts_with("rectangle \"<$:16x16:0000"), "{card}");
        assert!(card.contains("*2> Power\""), "{card}");
        assert!(card.ends_with("<<$dot>>"), "a stereotype stays: {card}");
        assert!(out.last().unwrap().text.contains("<#red$:16x16:"));
        // Keeps the lines of what follows.
        assert_eq!(out.last().unwrap().no, 19);
        let raw = [Line {
            no: 0,
            text: "A : <$nothing>",
            indent: 0,
            blank_before: 0,
        }];
        let err = take(&raw).unwrap_err();
        assert!(err.reason.contains("no sprite"), "{}", err.reason);
        let svg = [Line {
            no: 0,
            text: "sprite $s <svg viewBox=\"0 0 1 1\"/>",
            indent: 0,
            blank_before: 0,
        }];
        assert!(take(&svg).unwrap_err().reason.contains("SVG sprite"));
    }
}
