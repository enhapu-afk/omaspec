//! How a chart looks: its page and its words as the skinparams set them
//! (a pack's colors too), and what `<style>` gives `chartDiagram`, its
//! parts and a series by its class. The axes and the grid follow the
//! words' color, a step and some steps toward the page, so a chart reads
//! on a dark page as on a light one.

use std::collections::HashMap;

use crate::Declined;
use crate::color::{self, Color, Paint};
use crate::common::{Skinparam, normalize_key};
use crate::svg::Stroke;
use crate::text::Font;

/// The shape of a scatter's points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    Circle,
    Square,
    Triangle,
}

/// What a style class (`.highlights { … }`) gives the series under it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Class {
    pub fill: Option<Color>,
    pub line: Option<Color>,
    pub thickness: Option<f32>,
    pub shape: Option<Shape>,
    pub size: Option<f32>,
    pub marker: Option<Color>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Look {
    pub font_name: String,
    pub sketch: bool,
    pub page: Paint,
    pub ink: Color,
    /// The chart's words: its axes' titles and its annotations.
    pub font: Font,
    pub title: Font,
    /// The ticks' words, the values a series writes, the legend's.
    pub tick_font: Font,
    pub label_font: Font,
    pub legend_font: Font,
    pub note_font: Font,
    pub axis: Stroke,
    pub grid: Stroke,
    pub note_line: Color,
    /// What a part's style gives every series of its kind.
    pub bar: Class,
    pub bar_border: Option<Stroke>,
    /// How much of its category's room the bars take.
    pub bar_width: f32,
    pub line: Class,
    pub area: Class,
    pub scatter: Class,
    pub legend_fill: Paint,
    pub legend_border: Option<Color>,
    pub legend_padding: f32,
    pub legend_margin: f32,
    pub classes: HashMap<String, Class>,
    /// The colors of the series that name none, in turn.
    pub palette: Vec<Color>,
}

impl Look {
    pub fn read(params: &[Skinparam]) -> Result<Look, Declined> {
        let mut page: Paint = None;
        let mut ink = Color::BLACK;
        let mut size = 14.0;
        let mut font_name = crate::FACE.to_owned();
        let mut sketch = false;
        let mut own: HashMap<String, &Skinparam> = HashMap::new();
        let mut classes: HashMap<String, Class> = HashMap::new();
        for p in params {
            let key = normalize_key(&p.key);
            if let Some((only, key)) = key.split_once('|') {
                let class = only
                    .strip_prefix("class=")
                    .filter(|class| !class.contains('&'))
                    .ok_or_else(|| p.unsupported())?;
                let entry = classes.entry(class.to_owned()).or_default();
                class_param(entry, key.strip_prefix("chartseries").unwrap_or(key), p)?;
                continue;
            }
            match key.as_str() {
                "backgroundcolor" | "chartbackgroundcolor" => {
                    page = color::parse(&p.value).map_err(|why| p.fail(why))?;
                }
                "defaultfontcolor" | "chartfontcolor" => ink = p.solid()?,
                "defaultfontsize" | "chartfontsize" => size = p.number(4.0, 100.0)?,
                "defaultfontname" | "diagramfontname" => {
                    font_name = p.value.trim().trim_matches('"').to_owned();
                }
                "handwritten" => sketch = p.value.trim().eq_ignore_ascii_case("true"),
                _ => {}
            }
            own.insert(key, p);
        }
        let color = |key: &str| own.get(key).map(|p| p.solid()).transpose();
        let number =
            |key: &str, low: f32, high: f32| own.get(key).map(|p| p.number(low, high)).transpose();
        let ground = page.unwrap_or(Color::WHITE);
        let font = Font::new(size, ink);
        let sized = |key: &str, size: f32, base: Color| -> Result<Font, Declined> {
            let mut font = Font::new(
                number(&format!("{key}fontsize"), 4.0, 100.0)?.unwrap_or(size),
                base,
            );
            if let Some(c) = color(&format!("{key}fontcolor"))? {
                font.color = c;
            }
            Ok(font)
        };
        // A step toward the page, as far as a line still reads on it.
        let axis_color = color("chartaxisbordercolor")?
            .unwrap_or_else(|| ink.mix(ground, 0.45).legible_on(ground, ink, 3.0));
        let axis = Stroke::new(
            axis_color,
            number("chartaxisborderthickness", 0.0, 20.0)?.unwrap_or(1.0),
        );
        let grid = Stroke::new(
            color("chartgridbordercolor")?.unwrap_or(ink.mix(ground, 0.85)),
            number("chartgridborderthickness", 0.0, 20.0)?.unwrap_or(1.0),
        );
        let part = |prefix: &str| -> Result<Class, Declined> {
            let mut class = Class::default();
            for (key, p) in &own {
                if let Some(rest) = key.strip_prefix(prefix) {
                    class_param(&mut class, rest, p)?;
                }
            }
            Ok(class)
        };
        let bar = part("chartbar")?;
        let bar_border = bar
            .line
            .map(|c| Stroke::new(c, bar.thickness.unwrap_or(1.0)));
        Ok(Look {
            sketch,
            page,
            ink,
            font,
            title: Font::new(size + 2.0, ink).bold(),
            tick_font: sized("chartaxis", (size - 2.0).max(6.0), ink)?,
            label_font: Font::new((size - 3.0).max(6.0), ink).bold(),
            legend_font: sized("chartlegend", (size - 1.0).max(6.0), ink)?,
            note_font: sized("chartannotation", (size - 1.0).max(6.0), ink)?,
            axis,
            grid,
            note_line: color("chartannotationbordercolor")?.unwrap_or(ink),
            bar_border,
            bar_width: number("chartbarwidth", 0.05, 1.0)?.unwrap_or(0.8),
            bar,
            line: part("chartline")?,
            area: part("chartarea")?,
            scatter: part("chartscatter")?,
            legend_fill: match own.get("chartlegendbackgroundcolor") {
                Some(p) => color::parse(&p.value).map_err(|why| p.fail(why))?,
                None => None,
            },
            legend_border: color("chartlegendbordercolor")?,
            legend_padding: number("chartlegendpadding", 0.0, 200.0)?.unwrap_or(0.0),
            legend_margin: number("chartlegendmargin", 0.0, 200.0)?.unwrap_or(0.0),
            classes,
            palette: crate::chart::PALETTE.into_iter().map(Color::hex).collect(),
            font_name,
        })
    }

    /// The color of the `index`th series that names none.
    pub fn color(&self, index: usize) -> Color {
        self.palette[index % self.palette.len()]
    }

    /// The page, white where it is left open.
    pub fn ground(&self) -> Color {
        self.page.unwrap_or(Color::WHITE)
    }
}

/// One of a part's or a class's settings: `backgroundcolor`,
/// `bordercolor`, `borderthickness` and the markers'.
fn class_param(class: &mut Class, key: &str, p: &Skinparam) -> Result<(), Declined> {
    match key {
        "backgroundcolor" => class.fill = Some(p.solid()?),
        "bordercolor" => class.line = Some(p.solid()?),
        "borderthickness" => class.thickness = Some(p.number(0.0, 20.0)?),
        "markercolor" => class.marker = Some(p.solid()?),
        "markersize" => class.size = Some(p.number(1.0, 100.0)?),
        "markershape" => {
            class.shape = Some(match p.value.trim().to_ascii_lowercase().as_str() {
                "circle" => Shape::Circle,
                "square" => Shape::Square,
                "triangle" => Shape::Triangle,
                _ => return Err(p.unsupported()),
            });
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(key: &str, value: &str) -> Skinparam {
        Skinparam {
            key: key.into(),
            value: value.into(),
            line: 0,
            declared: None,
        }
    }

    #[test]
    fn a_chart_takes_the_page_and_its_parts_and_classes() {
        let look = Look::read(&[
            param("backgroundColor", "#1a1b26"),
            param("defaultFontColor", "#c0caf5"),
            param("ChartBarBackgroundColor", "#3498db"),
            param("ChartBarWidth", "0.5"),
            param("ChartScatterMarkerShape", "square"),
            param("class=highlights|ChartSeriesMarkerColor", "#e74c3c"),
            param("class=highlights|ChartSeriesMarkerShape", "triangle"),
        ])
        .unwrap();
        assert_eq!(look.page, Some(Color::hex(0x1a1b26)));
        assert_eq!(look.ink, Color::hex(0xc0caf5));
        assert_eq!(
            (look.bar.fill, look.bar_width),
            (Some(Color::hex(0x3498db)), 0.5)
        );
        assert_eq!(look.scatter.shape, Some(Shape::Square));
        let class = &look.classes["highlights"];
        assert_eq!(
            (class.marker, class.shape),
            (Some(Color::hex(0xe74c3c)), Some(Shape::Triangle))
        );
        // The axes follow the words toward the page: light on a dark one.
        assert!(look.axis.color.contrast(Color::hex(0x1a1b26)) > 3.0);
        assert!(Look::read(&[param("ChartScatterMarkerShape", "star")]).is_err());
    }
}
