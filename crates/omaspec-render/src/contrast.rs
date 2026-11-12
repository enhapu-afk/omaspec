//! How well a picture reads, from its SVG as drawn: each text against what
//! lies under it, and each line against the paper. The paper is white,
//! as exports and the pane lay a picture on it, unless the picture fills
//! it with a color of its own.
//!
//! What lies under a text is the last shape filled before it whose box
//! holds the middle of the text, its fill blended over what lies under it
//! by its opacity; the paper where there is none. Gradients and patterns
//! count as the color of their first stop, or not at all. The lines of a
//! chart's grid ([`omaspec_uml::GRID`]) only guide the eye and are left
//! out.

use crate::export::usvg::{self, Node, Paint};
use crate::export::{self, ExportError};

/// A color as `(r, g, b)`.
pub type Rgb = (u8, u8, u8);

/// What a [`Pair`] is about.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// A text, with its words.
    Text(String),
    /// A line or a border: a stroke.
    Line,
}

/// A color drawn on another.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub kind: Kind,
    /// Its color, blended by its opacity over `on`.
    pub color: Rgb,
    /// What it is drawn on: what lies under a text, the paper under a line.
    pub on: Rgb,
    /// Where it is, in the picture's units: `(x, y, width, height)`.
    pub at: (f32, f32, f32, f32),
}

impl Pair {
    /// The WCAG contrast ratio of the pair, 1 to 21.
    pub fn ratio(&self) -> f32 {
        ratio(self.color, self.on)
    }
}

/// The WCAG contrast ratio of two colors, 1 to 21.
pub fn ratio(a: Rgb, b: Rgb) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn luminance((r, g, b): Rgb) -> f32 {
    let channel = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// Every text and line of `svg`, with what it is drawn on.
pub fn pairs(svg: &str) -> Result<Vec<Pair>, ExportError> {
    let tree = export::parse(svg)?;
    let mut walk = Walk {
        filled: Vec::new(),
        pairs: Vec::new(),
        on_shapes: Vec::new(),
        paper: WHITE,
        size: (tree.size().width(), tree.size().height()),
    };
    walk.group(tree.root(), 1.0, (false, false));
    // A line lies on the paper, whatever filled it first; an icon keeps
    // what lies under it.
    let paper = walk.paper;
    for (i, pair) in walk.pairs.iter_mut().enumerate() {
        if pair.kind == Kind::Line && !walk.on_shapes.contains(&i) {
            pair.on = paper;
        }
    }
    Ok(walk.pairs)
}

const WHITE: Rgb = (255, 255, 255);

/// A filled shape, in the order drawn.
struct Filled {
    rect: usvg::Rect,
    color: Rgb,
    opacity: f32,
}

struct Walk {
    filled: Vec<Filled>,
    pairs: Vec<Pair>,
    /// The lines among `pairs` that are icons in the words.
    on_shapes: Vec<usize>,
    /// The paper: white, or the fill of a shape that covers the picture.
    paper: Rgb,
    size: (f32, f32),
}

impl Walk {
    /// The nodes of `group`; in a `grid`, lines need not stand out.
    /// `marks`: the group is (inside) a grid, an icon.
    fn group(&mut self, group: &usvg::Group, opacity: f32, (grid, icon): (bool, bool)) {
        let opacity = opacity * group.opacity().get();
        let grid = grid || group.id() == omaspec_uml::GRID;
        let icon = icon || group.id() == omaspec_uml::ICON;
        for node in group.children() {
            match node {
                Node::Group(inner) => self.group(inner, opacity, (grid, icon)),
                Node::Path(path) => self.path(path, opacity, (grid, icon)),
                Node::Text(text) => self.text(text, opacity),
                Node::Image(_) => {}
            }
        }
    }

    fn path(&mut self, path: &usvg::Path, opacity: f32, (grid, icon): (bool, bool)) {
        if !path.is_visible() {
            return;
        }
        let rect = path.abs_bounding_box();
        if let Some(fill) = path.fill()
            && let Some(color) = color(fill.paint())
        {
            let opacity = opacity * fill.opacity().get();
            if opacity > 0.0 {
                // A shape over the whole picture is its paper.
                if self.filled.is_empty()
                    && opacity >= 1.0
                    && rect.left() <= 0.5
                    && rect.top() <= 0.5
                    && rect.right() >= self.size.0 - 0.5
                    && rect.bottom() >= self.size.1 - 0.5
                {
                    self.paper = color;
                }
                self.filled.push(Filled {
                    rect,
                    color,
                    opacity,
                });
            }
        }
        if let Some(stroke) = path.stroke().filter(|_| !grid)
            && let Some(color) = color(stroke.paint())
        {
            let opacity = opacity * stroke.opacity().get();
            if opacity > 0.0 && stroke.width().get() > 0.0 {
                // A line runs over the page; an icon in the words is seen
                // as they are, against what lies under it.
                let on = if icon {
                    self.on_shapes.push(self.pairs.len());
                    self.under((
                        rect.x() + rect.width() / 2.0,
                        rect.y() + rect.height() / 2.0,
                    ))
                } else {
                    self.paper
                };
                self.pairs.push(Pair {
                    kind: Kind::Line,
                    color: blend(color, on, opacity),
                    on,
                    at: (rect.x(), rect.y(), rect.width(), rect.height()),
                });
            }
        }
    }

    fn text(&mut self, text: &usvg::Text, opacity: f32) {
        let rect = text.abs_bounding_box();
        let middle = (
            rect.x() + rect.width() / 2.0,
            rect.y() + rect.height() / 2.0,
        );
        let on = self.under(middle);
        for chunk in text.chunks() {
            for span in chunk.spans() {
                if !span.is_visible() {
                    continue;
                }
                let Some(fill) = span.fill() else {
                    continue;
                };
                let Some(color) = color(fill.paint()) else {
                    continue;
                };
                let words = chunk
                    .text()
                    .get(span.start()..span.end())
                    .unwrap_or_default()
                    .trim()
                    .to_owned();
                if words.is_empty() {
                    continue;
                }
                let opacity = opacity * fill.opacity().get();
                self.pairs.push(Pair {
                    kind: Kind::Text(words),
                    color: blend(color, on, opacity),
                    on,
                    at: (rect.x(), rect.y(), rect.width(), rect.height()),
                });
            }
        }
    }

    /// The color at `point` of what was filled so far.
    fn under(&self, (x, y): (f32, f32)) -> Rgb {
        let layers = self.filled.iter().rev().filter(|f| {
            f.rect.left() <= x && x <= f.rect.right() && f.rect.top() <= y && y <= f.rect.bottom()
        });
        // From the top down to the first opaque one, then blended upwards.
        let mut base = self.paper;
        let mut over = Vec::new();
        for layer in layers {
            if layer.opacity >= 1.0 {
                base = layer.color;
                break;
            }
            over.push(layer);
        }
        over.iter()
            .rev()
            .fold(base, |below, f| blend(f.color, below, f.opacity))
    }
}

fn color(paint: &Paint) -> Option<Rgb> {
    match paint {
        Paint::Color(c) => Some((c.red, c.green, c.blue)),
        Paint::LinearGradient(g) => g.stops().first().map(stop),
        Paint::RadialGradient(g) => g.stops().first().map(stop),
        Paint::Pattern(_) => None,
    }
}

fn stop(stop: &usvg::Stop) -> Rgb {
    let c = stop.color();
    (c.red, c.green, c.blue)
}

/// `color` at `opacity` over `on`.
fn blend(color: Rgb, on: Rgb, opacity: f32) -> Rgb {
    let a = opacity.clamp(0.0, 1.0);
    let mix = |c: u8, o: u8| (f32::from(c) * a + f32::from(o) * (1.0 - a)).round() as u8;
    (mix(color.0, on.0), mix(color.1, on.1), mix(color.2, on.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_reads_against_the_box_it_lies_in_and_a_line_against_the_paper() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100">
            <rect width="200" height="100" fill="#101010"/>
            <rect x="10" y="10" width="80" height="40" fill="#303030" stroke="#202020"/>
            <text x="20" y="35" font-family="sans-serif" font-size="12" fill="#e0e0e0">Idle</text>
            <text x="120" y="80" font-family="sans-serif" font-size="12" fill="#e0e0e0" fill-opacity="0.5">note</text>
            </svg>"##;
        let pairs = pairs(svg).unwrap();
        let idle = pairs
            .iter()
            .find(|p| p.kind == Kind::Text("Idle".into()))
            .unwrap();
        assert_eq!(idle.on, (0x30, 0x30, 0x30));
        let note = pairs
            .iter()
            .find(|p| p.kind == Kind::Text("note".into()))
            .unwrap();
        assert_eq!(note.on, (0x10, 0x10, 0x10));
        assert_eq!(note.color, (0x78, 0x78, 0x78));
        let border = pairs.iter().find(|p| p.kind == Kind::Line).unwrap();
        assert_eq!(
            (border.color, border.on),
            ((0x20, 0x20, 0x20), (0x10, 0x10, 0x10))
        );
        assert!(border.ratio() < 1.2);
        assert!((ratio((0, 0, 0), WHITE) - 21.0).abs() < 0.01);
    }

    #[test]
    fn a_grid_need_not_stand_out() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40">
            <g id="grid"><path d="M 10 0 L 10 40" stroke="#eeeeee"/></g>
            <path d="M 0 20 L 100 20" stroke="#000000"/>
            </svg>"##;
        let pairs = pairs(svg).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].color, (0, 0, 0));
    }

    #[test]
    fn a_picture_without_a_ground_lies_on_white_paper() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="40">
            <path d="M 0 20 L 100 20" stroke="#cccccc"/>
            </svg>"##;
        let pairs = pairs(svg).unwrap();
        assert_eq!(pairs[0].on, WHITE);
        assert!(pairs[0].ratio() < 3.0);
    }
}
