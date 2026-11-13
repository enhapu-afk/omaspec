//! A packet's fields (`packet-beta`): rows of 32 bits, each field a box
//! over its bits with its name in it and its first and last bit over it,
//! a field longer than its row going on in the next.

use super::{Look, body, said, title, unquote, words};
use crate::Declined;
use crate::svg::{Align, Canvas, Layer, Stroke};

/// Bits in a row.
const ROW: u32 = 32;
/// A bit's width, a row's height, the room for the bit numbers over it.
const BIT: f32 = 22.0;
const HEIGHT: f32 = 34.0;
const NUMBERS: f32 = 13.0;
/// Between two rows.
const GAP: f32 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<String>,
    /// From its first bit to its last, both in it.
    pub fields: Vec<(u32, u32, String)>,
}

/// The fields of `source`; `None` when it is no packet this drawer reads.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    let (title, lines) = body(source, "packet-beta").or_else(|| body(source, "packet"))?;
    let mut fields = Vec::new();
    let mut next = 0u32;
    for (_, line) in lines {
        let (bits, label) = line.split_once(':')?;
        let bits = bits.trim();
        let (from, to) = if let Some(count) = bits.strip_prefix('+') {
            let count: u32 = count.trim().parse().ok().filter(|c| *c > 0)?;
            (next, next + count - 1)
        } else if let Some((a, b)) = bits.split_once('-') {
            (a.trim().parse().ok()?, b.trim().parse().ok()?)
        } else {
            let bit: u32 = bits.parse().ok()?;
            (bit, bit)
        };
        if to < from || to > 1 << 16 {
            return None;
        }
        fields.push((from, to, unquote(label).to_owned()));
        next = to + 1;
    }
    (!fields.is_empty()).then_some(Diagram { title, fields })
}

pub fn draw(d: &Diagram, look: &Look) -> Result<String, Declined> {
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let width = ROW as f32 * BIT;
    let top = title(&mut canvas, d.title.as_deref(), look, width / 2.0);
    let stroke = Stroke::new(look.border, 1.0);
    let ink = look.ink_on(look.fill);
    let small = crate::text::Font::new((look.size - 4.0).max(8.0), look.ink);
    let label_font = crate::text::Font::new(look.size - 1.0, ink);
    for (from, to, label) in &d.fields {
        // The field, cut where its rows end.
        let mut bit = *from;
        while bit <= *to {
            let row = bit / ROW;
            let last = (*to).min(row * ROW + ROW - 1);
            let y = top + row as f32 * (HEIGHT + NUMBERS + GAP) + NUMBERS;
            let x = (bit % ROW) as f32 * BIT;
            let w = (last - bit + 1) as f32 * BIT;
            canvas.rect(
                Layer::Frames,
                (x, y, w, HEIGHT),
                0.0,
                Some(look.fill),
                Some(stroke),
            );
            // A name wider than its field is set smaller until it fits.
            let mut block = words(label, label_font, family);
            if block.width + 4.0 > w {
                let size = (label_font.size * (w - 4.0) / block.width).max(6.0);
                block = words(label, crate::text::Font { size, ..label_font }, family);
            }
            if block.width + 4.0 <= w || bit == *from {
                canvas.text(
                    Layer::Front,
                    &block,
                    (x + w / 2.0, y + (HEIGHT - block.height) / 2.0),
                    Align::Center,
                );
            }
            // The bit numbers over its ends.
            let first = words(&said(bit as f64), small, family);
            canvas.text(Layer::Front, &first, (x + 2.0, y - NUMBERS), Align::Left);
            if last != bit {
                let end = words(&said(last as f64), small, family);
                canvas.text(Layer::Front, &end, (x + w - 2.0, y - NUMBERS), Align::Right);
            }
            bit = last + 1;
        }
    }
    Ok(canvas.finish(10.0, look.page))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_read_by_their_bits_and_cut_at_the_row() {
        let d = from_mermaid(
            "packet-beta\ntitle UDP\n0-15: \"Source Port\"\n16-31: \"Destination Port\"\n+16: \"Length\"\n48: \"Flag\"\n49-95: \"Data\"\n",
        )
        .unwrap();
        assert_eq!(d.title.as_deref(), Some("UDP"));
        assert_eq!(d.fields[2], (32, 47, "Length".into()));
        assert_eq!(d.fields[3], (48, 48, "Flag".into()));
        let svg = draw(&d, &Look::default()).unwrap();
        assert!(
            svg.contains(">Source Port<") && svg.contains(">95<"),
            "{svg}"
        );
        // A field over two rows is drawn in both.
        assert_eq!(svg.matches(">Data<").count(), 2, "{svg}");
        assert!(from_mermaid("packet-beta\n0-15 Source\n").is_none());
        assert!(from_mermaid("pie\n\"a\": 1\n").is_none());
    }
}
