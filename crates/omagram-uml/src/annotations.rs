//! The texts and frame around a diagram, placed the same way for every
//! diagram type: header and title above it, then a legend at the top, a
//! main frame around the diagram, a legend at the bottom, caption and
//! footer below. Inside out, as PlantUML adds them: frame, legend, title,
//! caption, header and footer.

use crate::Declined;
use crate::color::{self, Color, Paint};
use crate::common::{Frame, Scale, Skinparam, Text, normalize_key};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

/// Space between the diagram and a legend.
const GAP: f32 = 10.0;
/// Space between a legend's border and its text.
const LEGEND_PAD: (f32, f32) = (8.0, 5.0);
/// Space between the main frame and the diagram.
const FRAME_PAD: f32 = 10.0;

/// The frame's texts, measured: what goes above and below a diagram. The
/// same places as in a sequence diagram.
pub struct Annotations {
    title: Block,
    header: Block,
    footer: Block,
    caption: Block,
    legend: Option<Placed>,
    frame: Option<Block>,
    look: Look,
    scale: Option<Scale>,
}

struct Placed {
    text: Block,
    top: bool,
    align: Align,
}

impl Annotations {
    /// `fonts` are those of the title, the header, the footer and the
    /// caption.
    pub fn new(frame: &Frame, fonts: [Font; 4], family: Family) -> Result<Self, Declined> {
        let [title, header, footer, caption] = fonts;
        let look = Look::read(&frame.skinparams)?;
        let measure = |text: Option<&Text>, font: Font| {
            text.map_or(Ok(Block::empty()), |t| {
                text::block(&t.value, font, family).map_err(|e| Declined::at(t.line, e))
            })
        };
        let legend = match &frame.legend {
            Some(legend) => Some(Placed {
                text: measure(Some(&legend.text), look.font)?,
                top: legend.top,
                align: legend.align,
            }),
            None => None,
        };
        Ok(Annotations {
            title: measure(frame.title.as_ref(), title)?,
            header: measure(frame.header.as_ref(), header)?,
            footer: measure(frame.footer.as_ref(), footer)?,
            caption: measure(frame.caption.as_ref(), caption)?,
            legend,
            frame: match &frame.mainframe {
                Some(label) => Some(measure(Some(label), look.frame_font)?),
                None => None,
            },
            look,
            scale: frame.scale,
        })
    }

    /// The widest text above or below the diagram, outside the main frame.
    pub fn width(&self) -> f32 {
        [&self.title, &self.header, &self.footer, &self.caption]
            .iter()
            .map(|b| b.width)
            .fold(0.0, f32::max)
    }

    /// Whether a main frame goes around the diagram.
    pub fn framed(&self) -> bool {
        self.frame.is_some()
    }

    /// Where the diagram starts: below the header, the title, a legend at
    /// the top and the main frame's tab.
    pub fn top(&self) -> f32 {
        let places = self.places();
        match &self.frame {
            Some(label) => places.frame + tab_size(label).1 + FRAME_PAD - 2.0,
            None => places.frame,
        }
    }

    /// The tops of the title, a legend at the top and the main frame.
    fn places(&self) -> Places {
        let mut top = 0.0;
        if !self.header.is_empty() {
            top += self.header.height + 5.0;
        }
        let title = top;
        if !self.title.is_empty() {
            top += self.title.height + 10.0;
        }
        let legend = top;
        if let Some(placed) = self.legend.as_ref().filter(|l| l.top) {
            top += legend_size(&placed.text).1 + GAP;
        }
        Places {
            title,
            legend,
            frame: top,
        }
    }

    /// Draws them around what `canvas` holds.
    pub fn draw(&self, canvas: &mut Canvas) {
        let places = self.places();
        let (mut left, mut right) = canvas.width_span();
        if let Some(label) = &self.frame {
            let (tab_w, tab_h) = tab_size(label);
            let top = places.frame;
            let frame_left = left - FRAME_PAD;
            let frame_right = (right + FRAME_PAD).max(frame_left + tab_w + 10.0);
            let bottom = canvas.bottom().max(top + tab_h) + FRAME_PAD;
            let stroke = Stroke::new(self.look.line, self.look.line_thickness);
            canvas.rect(
                Layer::Frames,
                (frame_left, top, frame_right - frame_left, bottom - top),
                0.0,
                None,
                Some(stroke),
            );
            canvas.polygon(
                Layer::Frames,
                &tab(frame_left, top, tab_w, tab_h),
                None,
                Some(stroke),
            );
            canvas.text(
                Layer::Front,
                label,
                (frame_left + 6.0, top + 2.0),
                Align::Left,
            );
            (left, right) = (frame_left, frame_right);
        }
        if let Some(placed) = &self.legend {
            let (w, h) = legend_size(&placed.text);
            right = right.max(left + w);
            let x = match placed.align {
                Align::Left => left,
                Align::Center => (left + right - w) / 2.0,
                Align::Right => right - w,
            };
            let y = if placed.top {
                places.legend
            } else {
                canvas.bottom() + GAP
            };
            let look = &self.look;
            canvas.rect(
                Layer::Front,
                (x, y, w, h),
                look.round,
                look.fill,
                Some(Stroke::new(look.border, look.thickness)),
            );
            canvas.text(
                Layer::Front,
                &placed.text,
                (x + LEGEND_PAD.0, y + LEGEND_PAD.1),
                Align::Left,
            );
        }

        let left = left.min(0.0);
        let right = right.max(left + self.width());
        let middle = (left + right) / 2.0;
        canvas.include(left, places.title);
        canvas.include(right, places.title);
        canvas.text(
            Layer::Front,
            &self.title,
            (middle, places.title),
            Align::Center,
        );
        canvas.text(Layer::Front, &self.header, (right, 0.0), Align::Right);
        let mut y = canvas.bottom() + 8.0;
        if !self.caption.is_empty() {
            canvas.text(Layer::Front, &self.caption, (middle, y), Align::Center);
            y += self.caption.height + 5.0;
        }
        canvas.text(Layer::Front, &self.footer, (middle, y), Align::Center);
        if let Some(scale) = self.scale {
            canvas.scale(scale);
        }
    }
}

struct Places {
    title: f32,
    legend: f32,
    frame: f32,
}

fn legend_size(text: &Block) -> (f32, f32) {
    (
        text.width + 2.0 * LEGEND_PAD.0,
        text.height + 2.0 * LEGEND_PAD.1,
    )
}

fn tab_size(label: &Block) -> (f32, f32) {
    (label.width + 18.0, label.height + 4.0)
}

/// A frame's tab: its label's box with the lower right corner cut off.
fn tab(x: f32, y: f32, w: f32, h: f32) -> [(f32, f32); 5] {
    [
        (x, y),
        (x + w, y),
        (x + w, y + h - 6.0),
        (x + w - 6.0, y + h),
        (x, y + h),
    ]
}

/// How the legend and the main frame look.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Look {
    fill: Paint,
    border: Color,
    thickness: f32,
    round: f32,
    font: Font,
    /// The main frame's line.
    line: Color,
    /// How thick the main frame's line is.
    line_thickness: f32,
    /// The main frame's label.
    frame_font: Font,
}

impl Look {
    /// PlantUML's look, changed by the `Legend…` skinparams and the default
    /// font. PlantUML sets the main frame's look only with `<style>`, which
    /// arrives here as `Mainframe…` skinparams. Its gray legend does not
    /// suit a dark background; there the legend and the frame take their
    /// colors from the background and the text, unless they have their
    /// own, so a dark theme stays readable. The rest of the skinparams are
    /// the diagram type's to check.
    fn read(params: &[Skinparam]) -> Result<Look, Declined> {
        let mut look = Look {
            fill: Some(Color::hex(0xDDDDDD)),
            border: Color::BLACK,
            thickness: 1.0,
            round: 7.5,
            font: Font::new(14.0, Color::BLACK),
            line: Color::BLACK,
            line_thickness: 1.0,
            frame_font: Font::new(14.0, Color::BLACK),
        };
        let mut background = None;
        let mut default_size = None;
        let mut default_color = None;
        let (mut own_fill, mut own_border, mut own_size, mut own_color) =
            (false, false, false, false);
        let (mut own_line, mut own_frame_size, mut own_frame_color) = (false, false, false);
        for p in params {
            let key = normalize_key(&p.key);
            let value = p.value.trim();
            // `bold`, `italic` or both, or `plain`.
            let font_style = || {
                let v = value.to_ascii_lowercase();
                let words: Vec<&str> = v.split_whitespace().collect();
                if words.is_empty()
                    || words
                        .iter()
                        .any(|w| !matches!(*w, "plain" | "bold" | "italic"))
                {
                    return Err(p.unsupported());
                }
                Ok((words.contains(&"bold"), words.contains(&"italic")))
            };
            match key.as_str() {
                "backgroundcolor" => background = color::parse(value).ok().flatten(),
                "defaultfontsize" => default_size = value.parse::<f32>().ok(),
                "defaultfontcolor" => default_color = color::parse(value).ok().flatten(),
                "roundcorner" => {
                    if let Ok(v) = value.parse::<f32>() {
                        look.round = v.clamp(0.0, 200.0) / 2.0;
                    }
                }
                "legendbackgroundcolor" => {
                    look.fill = color::parse(value).map_err(|e| p.fail(e))?;
                    own_fill = true;
                }
                "legendbordercolor" => {
                    look.border = p.solid()?;
                    own_border = true;
                }
                "legendborderthickness" => look.thickness = p.number(0.0, 20.0)?,
                "legendfontsize" => {
                    look.font.size = p.number(4.0, 100.0)?;
                    own_size = true;
                }
                "legendfontcolor" => {
                    look.font.color = p.solid()?;
                    own_color = true;
                }
                "legendfontstyle" => (look.font.bold, look.font.italic) = font_style()?,
                "legendroundcorner" | "legendborderroundcorner" => {
                    look.round = p.number(0.0, 200.0)? / 2.0;
                }
                "mainframebordercolor" => {
                    look.line = p.solid()?;
                    own_line = true;
                }
                "mainframeborderthickness" => look.line_thickness = p.number(0.0, 20.0)?,
                "mainframefontsize" => {
                    look.frame_font.size = p.number(4.0, 100.0)?;
                    own_frame_size = true;
                }
                "mainframefontcolor" => {
                    look.frame_font.color = p.solid()?;
                    own_frame_color = true;
                }
                "mainframefontstyle" => {
                    (look.frame_font.bold, look.frame_font.italic) = font_style()?;
                }
                other if other.starts_with("legend") || other.starts_with("mainframe") => {
                    return Err(p.unsupported());
                }
                _ => {}
            }
        }
        if let Some(size) = default_size.filter(|s| (4.0..=100.0).contains(s)) {
            if !own_frame_size {
                look.frame_font.size = size;
            }
            if !own_size {
                look.font.size = size;
            }
        }
        let dark = background.filter(|c: &Color| c.is_dark());
        let ink = default_color.or(dark.map(|_| Color::WHITE));
        if let Some(ink) = ink {
            if !own_frame_color {
                look.frame_font.color = ink;
            }
            if !own_color {
                look.font.color = ink;
            }
        }
        if let Some(background) = dark {
            let ink = ink.unwrap_or(Color::WHITE);
            if !own_line {
                look.line = ink;
            }
            if !own_fill {
                look.fill = Some(background.mix(ink, 0.12));
            }
            if !own_border {
                look.border = background.mix(ink, 0.6);
            }
        }
        Ok(look)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(lines: &[(&str, &str)]) -> Vec<Skinparam> {
        lines
            .iter()
            .map(|(key, value)| Skinparam {
                key: (*key).into(),
                value: (*value).into(),
                line: 3,
                declared: None,
            })
            .collect()
    }

    #[test]
    fn the_legend_follows_its_skinparams_and_dark_backgrounds() {
        let plain = Look::read(&[]).unwrap();
        assert_eq!(plain.fill, Some(Color::hex(0xDDDDDD)));
        assert_eq!(plain.font.color, Color::BLACK);

        let own = Look::read(&params(&[
            ("LegendBackgroundColor", "#FFFFCC"),
            ("LegendFontColor", "red"),
            ("LegendFontSize", "11"),
            ("LegendFontStyle", "bold"),
            ("defaultFontSize", "16"),
            ("roundCorner", "0"),
        ]))
        .unwrap();
        assert_eq!(own.fill, Some(Color::hex(0xFFFFCC)));
        assert_eq!(own.font.color, Color::hex(0xFF0000));
        assert_eq!(own.font.size, 11.0);
        assert!(own.font.bold);
        assert_eq!(own.frame_font.size, 16.0);
        assert_eq!(own.round, 0.0);

        let dark = Look::read(&params(&[
            ("backgroundColor", "#121212"),
            ("defaultFontColor", "#EEEEEE"),
        ]))
        .unwrap();
        assert_eq!(dark.font.color, Color::hex(0xEEEEEE));
        assert_eq!(dark.line, Color::hex(0xEEEEEE));
        assert!(dark.fill.is_some_and(Color::is_dark));

        for (key, value, part) in [
            ("LegendFontName", "Arial", "not supported"),
            ("LegendBorderColor", "transparent", "transparent"),
            ("LegendFontSize", "900", "number from"),
            ("LegendFontStyle", "wavy", "not supported"),
        ] {
            let err = Look::read(&params(&[(key, value)])).unwrap_err();
            assert_eq!(err.line, Some(3));
            assert!(err.reason.contains(part), "{key}: {}", err.reason);
        }
    }
}
