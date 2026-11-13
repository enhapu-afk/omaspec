//! A quadrant chart (`quadrantChart`): a square in four, each quarter
//! with its name, the axes' two ends said under and beside it, and each
//! point where its two values, from 0 to 1, put it, with its name.

use super::{Look, body, number, title, words};
use crate::Declined;
use crate::color::{self, Color};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::Font;

/// The square's side.
const SIDE: f32 = 400.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<String>,
    /// Low and high end of each axis.
    pub x: (String, String),
    pub y: (String, String),
    /// Quadrants 1 to 4: top right, top left, bottom left, bottom right.
    pub quadrants: [String; 4],
    pub points: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Point {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub color: Option<Color>,
    pub radius: Option<f32>,
}

/// The chart of `source`; `None` when it is none this drawer reads.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    let (title, lines) = body(source, "quadrantChart")?;
    let mut d = Diagram {
        title,
        x: Default::default(),
        y: Default::default(),
        quadrants: Default::default(),
        points: Vec::new(),
    };
    for (_, line) in lines {
        if let Some(rest) = line.strip_prefix("x-axis") {
            d.x = ends(rest);
        } else if let Some(rest) = line.strip_prefix("y-axis") {
            d.y = ends(rest);
        } else if let Some(rest) = line.strip_prefix("quadrant-") {
            let (n, name) = rest.split_once(char::is_whitespace)?;
            let n: usize = n.parse().ok().filter(|n| (1..=4).contains(n))?;
            d.quadrants[n - 1] = name.trim().to_owned();
        } else if line.starts_with("classDef") {
            return None;
        } else {
            d.points.push(point(line)?);
        }
    }
    Some(d)
}

/// `Low Reach --> High Reach` as its two ends.
fn ends(text: &str) -> (String, String) {
    match text.split_once("-->") {
        Some((low, high)) => (low.trim().to_owned(), high.trim().to_owned()),
        None => (text.trim().to_owned(), String::new()),
    }
}

/// `Campaign A: [0.3, 0.6] radius: 12, color: #ff3300`.
fn point(line: &str) -> Option<Point> {
    let (name, rest) = line.split_once(':')?;
    let open = rest.find('[')?;
    let close = rest.find(']')?;
    let (x, y) = rest[open + 1..close].split_once(',')?;
    let (x, y) = (number(x)?, number(y)?);
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }
    let mut p = Point {
        name: name.trim().to_owned(),
        x,
        y,
        color: None,
        radius: None,
    };
    let style = rest[close + 1..].trim();
    for part in style.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        let (key, value) = part.split_once(':')?;
        match key.trim() {
            "color" => p.color = color::parse(value.trim()).ok().flatten(),
            "radius" => {
                p.radius = value
                    .trim()
                    .parse()
                    .ok()
                    .filter(|r| (1.0..=40.0).contains(r))
            }
            _ => {}
        }
    }
    Some(p)
}

pub fn draw(d: &Diagram, look: &Look) -> Result<String, Declined> {
    let mut canvas = Canvas::new(&look.font_name, false);
    canvas.sketch(look.sketch);
    let family = canvas.family();
    let font = Font::new(look.size - 1.0, look.ink);
    let reach = [&d.y.0, &d.y.1]
        .iter()
        .map(|t| words(t, font, family).width)
        .fold(0.0, f32::max);
    let left = reach + 16.0;
    let top = title(&mut canvas, d.title.as_deref(), look, left + SIDE / 2.0);
    let half = SIDE / 2.0;
    // Quadrants 2, 1 above, 3, 4 below, each in a light color of its own.
    let places = [(half, 0.0), (0.0, 0.0), (0.0, half), (half, half)];
    for (q, (dx, dy)) in places.iter().enumerate() {
        let fill = look.color(q).mix(look.ground(), 0.82);
        // Apart by a gap of the page; drawn by hand, with a line.
        canvas.rect(
            Layer::Background,
            (left + dx + 1.0, top + dy + 1.0, half - 2.0, half - 2.0),
            0.0,
            Some(fill),
            look.sketch
                .then(|| Stroke::new(fill.mix(look.ink, 0.3), 1.0)),
        );
        let name = words(
            &d.quadrants[q],
            Font::new(look.size, look.ink_on(fill)).bold(),
            family,
        );
        canvas.text(
            Layer::Front,
            &name,
            (left + dx + half / 2.0, top + dy + 8.0),
            Align::Center,
        );
    }
    // The axes' ends: low and high under the square and beside it.
    let under = top + SIDE + 6.0;
    for (text, x) in [(&d.x.0, left + half / 2.0), (&d.x.1, left + half * 1.5)] {
        let block = words(text, font, family);
        canvas.text(Layer::Front, &block, (x, under), Align::Center);
    }
    for (text, y) in [(&d.y.1, top + half / 2.0), (&d.y.0, top + half * 1.5)] {
        let block = words(text, font, family);
        canvas.text(
            Layer::Front,
            &block,
            (left - 10.0, y - block.height / 2.0),
            Align::Right,
        );
    }
    // The points, each with its name under it.
    for p in &d.points {
        let x = left + p.x as f32 * SIDE;
        let y = top + SIDE - p.y as f32 * SIDE;
        let r = p.radius.unwrap_or(5.0);
        let color = p.color.unwrap_or_else(|| look.ink.mix(look.color(0), 0.4));
        canvas.ellipse(
            Layer::Front,
            (x, y),
            (r, r),
            Some(color),
            look.sketch.then(|| Stroke::new(color, 1.0)),
        );
        let quadrant = match (p.x >= 0.5, p.y >= 0.5) {
            (true, true) => 0,
            (false, true) => 1,
            (false, false) => 2,
            (true, false) => 3,
        };
        let ground = look.color(quadrant).mix(look.ground(), 0.82);
        let label = words(
            &p.name,
            Font::new(look.size - 2.0, look.ink_on(ground)),
            family,
        );
        canvas.text(Layer::Front, &label, (x, y + r + 2.0), Align::Center);
    }
    Ok(canvas.finish(10.0, look.page))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadrants_axes_and_points_are_read() {
        let d = from_mermaid(
            "quadrantChart\n    title Reach\n    x-axis Low Reach --> High Reach\n    y-axis Low Engagement --> High Engagement\n    quadrant-1 Expand\n    quadrant-3 Re-evaluate\n    Campaign A: [0.3, 0.6]\n    Point B: [0.9, 0.1] radius: 10, color: #ff3300\n",
        )
        .unwrap();
        assert_eq!(d.x, ("Low Reach".into(), "High Reach".into()));
        assert_eq!(d.quadrants[0], "Expand");
        assert_eq!(d.points[1].radius, Some(10.0));
        assert_eq!(d.points[1].color, Some(Color::hex(0xFF3300)));
        let svg = draw(&d, &Look::default()).unwrap();
        assert!(
            svg.contains(">Campaign A<") && svg.contains(">Re-evaluate<"),
            "{svg}"
        );
        assert!(from_mermaid("quadrantChart\nA: [1.5, 0.2]\n").is_none());
    }
}
