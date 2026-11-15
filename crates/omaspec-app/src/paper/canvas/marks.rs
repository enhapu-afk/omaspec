//! What lies over the diagram in layers of its own: the marks of its
//! links, the light on the caret's name, the paper's edge.

use iced::mouse::{self, Cursor};
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Path, Stroke};
use iced::window;
use iced::{Color, Point, Rectangle, Renderer, Size, Theme};

use omaspec_render::Link;

use super::*;

/// The caret's name, lit in a layer of its own. In one layer iced draws
/// pictures over shapes, so this light would sit under the diagram if it
/// were drawn with the picture.
/// The links of the diagram, in a layer of their own: a small mark in
/// the corner of each state that opens a diagram of its own, and a light
/// around the link under the pointer.
pub struct LinkMarks<'a> {
    pub links: &'a [Link],
    pub view: View,
    pub glide: Option<Glide>,
    pub color: Color,
    pub scale: f32,
}

#[derive(Default)]
pub struct LinkState {
    /// The link the pointer was last over.
    over: Option<usize>,
}

impl LinkMarks<'_> {
    fn under(&self, cursor: Cursor, bounds: Rectangle) -> Option<usize> {
        let at = cursor.position_in(bounds)?;
        let p = self.view.to_diagram(at, bounds.size());
        let found = link_under(self.links, p)?;
        self.links.iter().position(|l| std::ptr::eq(l, found))
    }
}

impl canvas::Program<Message> for LinkMarks<'_> {
    type State = LinkState;

    fn update(
        &self,
        state: &mut LinkState,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<Message>> {
        match event {
            Event::Window(window::Event::RedrawRequested(now)) => self
                .glide
                .filter(|g| !g.done(*now))
                .map(|_| Action::request_redraw()),
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                let over = self.under(cursor, bounds);
                (over != state.over).then(|| {
                    state.over = over;
                    Action::request_redraw()
                })
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &LinkState,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let now = crate::clock::now();
        let view = self.glide.map_or(self.view, |glide| glide.at(now));
        let grid = Grid::of(bounds, self.scale);
        let local = Rectangle::with_size(bounds.size());
        for (index, link) in self.links.iter().enumerate() {
            let on_screen = grid.snap(view.on_screen(link_rect(link), local));
            if on_screen.width <= 1.0 || on_screen.height <= 1.0 {
                continue;
            }
            if state.over == Some(index) {
                let radius = RADIUS.min(on_screen.width.min(on_screen.height) * 0.5);
                let path = rounded(on_screen, radius);
                frame.fill(
                    &path,
                    Color {
                        a: 0.08,
                        ..self.color
                    },
                );
                frame.stroke(
                    &path,
                    Stroke::default().with_color(self.color).with_width(2.0),
                );
            }
            if !link.tile {
                // Two small squares, one behind the other, on the box's
                // top right corner, clear of its words: a diagram inside.
                let size = (on_screen.height * 0.22).clamp(5.0, 9.0);
                let at = Point::new(
                    on_screen.x + on_screen.width - size * 0.5,
                    on_screen.y - size * 0.5,
                );
                let back =
                    Path::rectangle(Point::new(at.x - 2.5, at.y + 2.5), Size::new(size, size));
                let front = Path::rectangle(at, Size::new(size, size));
                let stroke = Stroke::default().with_color(self.color).with_width(1.2);
                frame.stroke(&back, stroke);
                frame.fill(&front, self.color);
            }
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &LinkState,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}

pub struct BreathLight {
    pub view: View,
    pub glide: Option<Glide>,
    pub breath: super::super::breath::Breath,
    pub color: Color,
    /// The paper behind the diagram, so the rim stays visible when the
    /// accent is close to a box.
    pub paper: Color,
    pub scale: f32,
}

impl canvas::Program<Message> for BreathLight {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &Event,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> Option<Action<Message>> {
        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        (!self.breath.settled(*now)).then(Action::request_redraw)
    }

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let now = crate::clock::now();
        let shown = self.breath.at(now);
        if shown.alpha < 0.01 {
            return vec![frame.into_geometry()];
        }
        let grid = Grid::of(bounds, self.scale);
        let view = self.glide.map_or(self.view, |glide| glide.at(now));
        let on_screen = view.on_screen(shown.rect, Rectangle::with_size(bounds.size()));
        let opened = open_from_center(on_screen, shown.spread);
        let tiny = opened.width < 6.0 && opened.height < 6.0;
        if tiny {
            draw_point(
                &mut frame,
                opened.center(),
                shown.alpha,
                self.color,
                self.paper,
            );
        } else {
            let opened = grid.snap(opened);
            match shown.mark {
                super::super::breath::Mark::Words => {
                    let plate =
                        super::super::wash::plate(self.color, self.paper, shown.ink, shown.ground);
                    draw_words(&mut frame, opened, shown.alpha, plate);
                }
                super::super::breath::Mark::Shape => {
                    draw_shape(&mut frame, opened, shown.alpha, self.color, self.paper);
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

/// The accent pulled toward white, so a muted theme color still reads as light.
fn lit(color: Color) -> Color {
    Color {
        r: 1.0 - (1.0 - color.r) * 0.4,
        g: 1.0 - (1.0 - color.g) * 0.4,
        b: 1.0 - (1.0 - color.b) * 0.4,
        a: 1.0,
    }
}

fn with_a(color: Color, alpha: f32) -> Color {
    Color { a: alpha, ..color }
}

/// `rect` scaled about its center. `spread` 0 is the point it travels as.
fn open_from_center(rect: Rectangle, spread: f32) -> Rectangle {
    let spread = spread.clamp(0.0, 1.0);
    let center = rect.center();
    let size = Size::new(rect.width * spread, rect.height * spread);
    Rectangle::new(
        Point::new(center.x - size.width * 0.5, center.y - size.height * 0.5),
        size,
    )
}

fn outset(rect: Rectangle, by: f32) -> Rectangle {
    Rectangle::new(
        Point::new(rect.x - by, rect.y - by),
        Size::new(rect.width + by * 2.0, rect.height + by * 2.0),
    )
}

/// The travelling point: a bright core with a halo, rimmed in the paper
/// so it shows on a box of the accent's own color.
fn draw_point(frame: &mut Frame, at: Point, alpha: f32, accent: Color, paper: Color) {
    let glow = lit(accent);
    for (radius, a) in [(16.0, 0.10), (10.0, 0.20), (6.0, 0.38)] {
        frame.fill(&Path::circle(at, radius), with_a(glow, a * alpha));
    }
    frame.stroke(
        &Path::circle(at, 5.0),
        Stroke::default()
            .with_color(with_a(paper, 0.95 * alpha))
            .with_width(2.5),
    );
    frame.fill(&Path::circle(at, 4.2), with_a(glow, alpha));
}

/// A box: a luminous wash, a paper-colored gap, and a bright rim. The gap
/// keeps the rim readable when the accent is close to the box.
fn draw_shape(frame: &mut Frame, rect: Rectangle, alpha: f32, accent: Color, paper: Color) {
    let glow = lit(accent);
    let radius = RADIUS.min(rect.width.min(rect.height) * 0.5);
    for (grow, a) in [(14.0, 0.10), (8.0, 0.16), (3.0, 0.24)] {
        let grown = outset(rect, grow);
        frame.fill(&rounded(grown, radius + grow), with_a(glow, a * alpha));
    }
    let path = rounded(rect, radius);
    frame.fill(&path, with_a(glow, 0.28 * alpha));
    frame.stroke(
        &path,
        Stroke::default()
            .with_color(with_a(paper, 0.95 * alpha))
            .with_width(4.0),
    );
    frame.stroke(
        &path,
        Stroke::default()
            .with_color(with_a(glow, alpha))
            .with_width(2.0),
    );
}

/// Words on a line: a quiet wash a little wider than the letters. The
/// letters stay readable through it.
fn draw_words(frame: &mut Frame, letters: Rectangle, alpha: f32, plate: super::super::wash::Plate) {
    let rect = super::super::wash::bounds(letters);
    frame.fill(
        &Path::rectangle(rect.position(), rect.size()),
        with_a(plate.color, plate.fill * alpha),
    );
}

/// While the text does not draw, a red edge round the last good picture:
/// in a layer over the pictures, as iced draws pictures over shapes within
/// one.
pub struct PaperEdge {
    pub diagram: Size,
    pub view: View,
    pub glide: Option<Glide>,
    pub color: Color,
    pub scale: f32,
}

impl canvas::Program<Message> for PaperEdge {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let grid = Grid::of(bounds, self.scale);
        let view = self.glide.map_or(self.view, |g| g.at(crate::clock::now()));
        let paper = grid.snap(view.diagram_rect(self.diagram, Rectangle::with_size(bounds.size())));
        frame.stroke(
            &rounded(paper, RADIUS),
            Stroke::default().with_color(self.color).with_width(1.5),
        );
        vec![frame.into_geometry()]
    }
}
