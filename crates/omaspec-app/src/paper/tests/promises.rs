//! What a glide promises (docs/adr/0029), held against real folds of the
//! gallery's diagrams: its first frame is the picture before and its last
//! the new one, nothing of a box lands outside the box's place of the
//! moment, and no pixel of a box is drawn twice.

use iced::widget::canvas::{Frame, Geometry, Program};
use iced::{Color, Element, Length, Renderer, Theme, mouse};

use super::super::canvas::{self as draw, Grid};
use super::super::raster::Raster;
use super::*;

/// The pane the glides are drawn in.
const PANE: Size = Size::new(800.0, 600.0);

/// Folds of the gallery's diagrams, each a glide: a state machine, a
/// package of classes, a flowchart's subgraph, a JSON card.
fn folds() -> Vec<(&'static str, Paper)> {
    let gallery = |name: &str| {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/gallery")
                .join(name),
        )
        .unwrap()
    };
    [
        ("coffee-machine.puml", "Brewing", Language::PlantUml),
        ("library.puml", "Lending", Language::PlantUml),
        ("pipeline.mmd", "Build", Language::Mermaid),
        ("recipe.puml", "batter", Language::PlantUml),
    ]
    .into_iter()
    .map(|(file, container, language)| {
        let open = gallery(file);
        let folded = match language {
            Language::PlantUml => {
                let at = open.rfind("@end").unwrap();
                let line = omaspec_uml::fold::line(container, false);
                format!("{}{line}{}", &open[..at], &open[at..])
            }
            Language::Mermaid => format!("{open}{}", omaspec_uml::fold::line(container, true)),
        };
        let mut paper = Paper {
            folding: true,
            ..Paper::default()
        };
        paper.send(Message::Viewport(PANE));
        show_source(&mut paper, &open, language);
        paper.send(Message::Faded);
        paper.set_fold(container, true);
        show_source(&mut paper, &folded, language);
        assert!(paper.fading_lines(), "{file}: folding {container} glides");
        (file, paper)
    })
    .collect()
}

/// A frame of the glide on `paper` at `t`, as the window draws it.
fn frame_at<'a>(paper: &'a Paper, t: f32) -> (Grid, Rectangle, morph::Paint<'a>) {
    let fade = paper.arrival.fade().expect("a glide");
    let (old, new) = fade.bare.as_ref().expect("its sheets");
    let local = Rectangle::with_size(PANE);
    (
        Grid::of(local, 1.0),
        local,
        morph::Paint {
            t,
            morph: fade.morph.as_ref().expect("its boxes"),
            old_view: fade.old_view,
            old_size: fade.old_size,
            old,
            new_view: fade.new_view,
            new_size: fade.new_size,
            new,
            background: Color::BLACK,
        },
    )
}

/// The glide's frame at `t`, drawn by tiny-skia: its pictures, and its
/// lines in a layer over them, as the window stacks them.
fn glide_pixels(paper: &Paper, t: f32) -> Vec<u8> {
    let fade = paper.arrival.fade().unwrap();
    let lines = fade.lines.as_ref().expect("its lines");
    let pictures = |frame: &mut Frame, _: Rectangle| {
        let (grid, local, paint) = frame_at(paper, t);
        morph::paint(frame, grid, local, &paint);
    };
    let over = |frame: &mut Frame, _: Rectangle| {
        let (grid, local, paint) = frame_at(paper, t);
        let sheets = morph::sheets(grid, local, &paint);
        wires::paint(frame, local, lines, &sheets);
    };
    pixels(&[&pictures, &over])
}

/// `raster` shown as the pane shows a picture that stands, at `view` of
/// a diagram of `size`.
fn plain_pixels(raster: &Raster, view: View, size: Size) -> Vec<u8> {
    let sheet = |frame: &mut Frame, local: Rectangle| {
        let grid = Grid::of(local, 1.0);
        let paper = grid.snap(view.diagram_rect(size, local));
        draw::shadow(frame, paper, Color::BLACK, 1.0, 1.0);
        frame.fill(&draw::rounded(paper, draw::RADIUS), raster.paper);
        grid.draw(
            frame,
            paper,
            draw::RADIUS,
            [(view.on_screen(raster.region, local), raster, 1.0)],
        );
    };
    pixels(&[&sheet])
}

type Layer<'a> = &'a dyn Fn(&mut Frame, Rectangle);

/// `layers` drawn one over another by tiny-skia on black, as RGBA rows.
fn pixels(layers: &[Layer<'_>]) -> Vec<u8> {
    use iced::advanced::renderer::{Headless, Style};
    struct Painted<'a>(Layer<'a>);
    impl Program<()> for Painted<'_> {
        type State = ();
        fn draw(
            &self,
            _: &(),
            renderer: &Renderer,
            _: &Theme,
            bounds: Rectangle,
            _: mouse::Cursor,
        ) -> Vec<Geometry> {
            let mut frame = Frame::new(renderer, bounds.size());
            (self.0)(&mut frame, Rectangle::with_size(bounds.size()));
            vec![frame.into_geometry()]
        }
    }
    let mut renderer = iced::futures::executor::block_on(Renderer::new(
        iced::Font::DEFAULT,
        iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("tiny-skia draws without a GPU");
    let stack = iced::widget::Stack::with_children(layers.iter().map(|layer| {
        iced::widget::Canvas::new(Painted(*layer))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }));
    let element: Element<'_, (), Theme, Renderer> = stack.into();
    let mut ui =
        iced_runtime::UserInterface::build(element, PANE, Default::default(), &mut renderer);
    let style = Style {
        text_color: Color::WHITE,
    };
    ui.draw(
        &mut renderer,
        &Theme::Dark,
        &style,
        mouse::Cursor::Unavailable,
    );
    renderer.screenshot(
        iced::Size::new(PANE.width as u32, PANE.height as u32),
        1.0,
        Color::BLACK,
    )
}

/// How many pixels of `a` and `b` differ by more than `by` in a channel,
/// as a share of all of them.
fn differing(a: &[u8], b: &[u8], by: u8) -> f32 {
    let (a, b) = (a.as_chunks::<4>().0, b.as_chunks::<4>().0);
    let far = a
        .iter()
        .zip(b)
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > by))
        .count();
    far as f32 / a.len() as f32
}

/// How many of a frame's pixels may differ from the picture it promises
/// by more than a quarter of a channel: the lines are drawn anew as
/// strokes, not taken from the pictures, and their edges differ by a
/// little. A box half a pixel off is more than this.
const SAME: f32 = 0.005;

/// The first frame is the picture before, and the last the new one: no
/// box, line or word pops in or out at either end, and none lies half a
/// pixel off, blurred, while the glide runs.
#[test]
fn a_glide_begins_on_the_old_picture_and_ends_on_the_new_one() {
    for (file, paper) in folds() {
        let fade = paper.arrival.fade().unwrap();
        let before = plain_pixels(&fade.old[0], fade.old_view, fade.old_size);
        let after = plain_pixels(
            paper.overview.shown.as_ref().unwrap(),
            fade.new_view,
            fade.new_size,
        );
        let first = differing(&glide_pixels(&paper, 0.0), &before, 64);
        let last = differing(&glide_pixels(&paper, 1.0), &after, 64);
        assert!(
            first < SAME,
            "{file}: the first frame is not the picture before ({first})"
        );
        assert!(
            last < SAME,
            "{file}: the last frame is not the new picture ({last})"
        );
    }
}

/// The pieces each box is drawn in, at every tenth of the way: none
/// reaches past the box's place of the moment, no two overlap, and
/// together they fill it, unless the box is a card whose rows stay where
/// they are and wipes what it gains.
#[test]
fn a_box_is_drawn_inside_its_place_and_each_pixel_once() {
    let hair = 0.01;
    let area = |r: Rectangle| r.width * r.height;
    for (file, paper) in folds() {
        for tenth in 0..=10 {
            let t = tenth as f32 / 10.0;
            let (grid, local, paint) = frame_at(&paper, t);
            for picture in morph::box_pictures_of(grid, local, &paint) {
                let dest = picture.dest;
                let drawn: Vec<Rectangle> = picture
                    .fit
                    .pieces
                    .iter()
                    .filter_map(|piece| piece.dest.intersection(&piece.limit))
                    .filter(|r| area(*r) > hair)
                    .collect();
                for piece in &drawn {
                    let inside = piece.x >= dest.x - hair
                        && piece.y >= dest.y - hair
                        && piece.x + piece.width <= dest.x + dest.width + hair
                        && piece.y + piece.height <= dest.y + dest.height + hair;
                    assert!(inside, "{file} at {t}: {piece:?} lands outside {dest:?}");
                }
                for (i, a) in drawn.iter().enumerate() {
                    for b in &drawn[i + 1..] {
                        let twice = a.intersection(b).map_or(0.0, area);
                        assert!(
                            twice <= hair * a.width.max(a.height),
                            "{file} at {t}: {a:?} and {b:?} draw {twice} twice"
                        );
                    }
                }
                // A card drawn at its size from its top left wipes what
                // it gains; every other box is filled.
                let first = picture.fit.pieces[0].dest;
                let stretched = picture.fit.pieces.len() > 1
                    || ((first.x - dest.x).abs() < hair
                        && (first.y - dest.y).abs() < hair
                        && (first.width - dest.width).abs() < hair
                        && (first.height - dest.height).abs() < hair);
                if stretched {
                    let fills = drawn.iter().map(|r| area(*r)).sum::<f32>();
                    assert!(
                        (fills - area(dest)).abs() <= 1.0 + area(dest) * 0.002,
                        "{file} at {t}: {fills} of {} of {dest:?} drawn",
                        area(dest)
                    );
                }
            }
        }
    }
}

/// A mark laid over a box of the glide ([`morph::box_at`]) is where the
/// picture before has the box at the start and where the new one has it
/// at the end, and moves with the box between: the glow of a box that
/// went fades on the box as it goes.
#[test]
fn a_mark_lies_on_its_box_at_both_ends_of_the_glide() {
    for (file, paper) in folds() {
        let fade = paper.arrival.fade().unwrap();
        let morph = fade.morph.as_ref().unwrap();
        for (index, step) in morph.moves.iter().enumerate() {
            let ends = [
                (
                    0.0,
                    step.role != morph::Role::Arrive,
                    fade.old_view,
                    step.from,
                ),
                (1.0, step.role != morph::Role::Leave, fade.new_view, step.to),
            ];
            for (t, there, view, rect) in ends {
                if !there {
                    continue;
                }
                let (grid, local, paint) = frame_at(&paper, t);
                let mark = morph::box_at(grid, local, &paint, index).unwrap();
                let place = view.on_screen(rect, local);
                for (a, b) in [
                    (mark.x, place.x),
                    (mark.y, place.y),
                    (mark.width, place.width),
                    (mark.height, place.height),
                ] {
                    assert!(
                        (a - b).abs() < 1.5,
                        "{file}: box {index} at {t}: {mark:?}, its place {place:?}"
                    );
                }
            }
        }
    }
}
