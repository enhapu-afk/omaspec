//! What changed, glowing a moment after the glide from another version
//! of the diagram: B of a comparison, the commit before in a history
//! (`play --history`). A box that came glows in green once it is there,
//! one that changed (its size or its color) in amber, and one that went
//! takes on red as it fades out: the colors `omaspec diff -o` marks a
//! machine in. Only a glide that is asked for glows; an edit glides as it
//! always does.

use std::time::Duration;

use iced::mouse::Cursor;
use iced::time::Instant;
use iced::widget::canvas::{self, Action, Event, Frame, Geometry, Stroke};
use iced::window;
use iced::{Color, Rectangle, Renderer, Theme};

use super::canvas::{Grid, rounded};
use super::morph::{Morph, Move, Role};
use super::view::{Glide, View};
use super::{Fade, Message};

/// How long a box that came or changed glows once the glide has ended.
pub const GLOW: Duration = Duration::from_millis(1_100);
/// The share of the glide over which a box that goes fades out red.
const OUT: f32 = 0.45;
/// How far a box may change in either direction, in the diagram's units,
/// and still be the same size: a box that only moved does not glow.
const SAME: f32 = 1.5;
/// How far a box's fill may differ in a channel, 0 to 1, and still be
/// the same color: it is read from the pixels of each picture.
const HUE: f32 = 0.06;

/// What happened to a box from the version before to this one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Came,
    Changed,
    Went,
}

impl Change {
    /// Its color, as `omaspec diff -o` draws it.
    fn color(self) -> Color {
        let hex = match self {
            Change::Came => omaspec_uml::state::TAKEN,
            Change::Changed => omaspec_uml::state::CHANGED,
            Change::Went => omaspec_uml::state::MISSED,
        };
        omaspec_theme::Rgb::parse_hex(hex).map_or(Color::BLACK, crate::ui::color)
    }
}

/// A box that glows: where it is in its picture's units, the version
/// before for one that went, this one for the others; and which box of
/// the glide it is, so one that went glows where the glide draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Glowing {
    pub rect: Rectangle,
    pub change: Change,
    pub index: usize,
}

/// The boxes of one glide that glow, and when.
#[derive(Clone, Debug)]
pub struct Glow {
    pub boxes: Vec<Glowing>,
    /// When the glide began, how long it takes, and how long what came
    /// or changed glows after it.
    start: Instant,
    glide: Duration,
    length: Duration,
}

impl Glow {
    /// What glows of `morph`, a glide that began at `start` and takes
    /// `glide`, for `length` after it ([`GLOW`] at most); none when no box
    /// came, changed or went.
    pub fn of(morph: &Morph, start: Instant, glide: Duration, length: Duration) -> Option<Glow> {
        let boxes: Vec<Glowing> = morph
            .moves
            .iter()
            .enumerate()
            .filter_map(|(index, m)| glowing(index, m))
            .collect();
        (!boxes.is_empty()).then_some(Glow {
            boxes,
            start,
            glide,
            length: length.min(GLOW),
        })
    }

    /// How strongly `change` glows at `now`, 0 to 1: one that went from
    /// the glide's start as it fades, the others once the glide ended,
    /// rising quickly and fading slowly.
    pub fn strength(&self, change: Change, now: Instant) -> f32 {
        let since = now.saturating_duration_since(self.start).as_secs_f32();
        let glide = self.glide.as_secs_f32().max(0.001);
        match change {
            Change::Went => {
                let t = since / (glide * OUT);
                if now < self.start || t >= 1.0 {
                    0.0
                } else {
                    1.0 - t * t
                }
            }
            Change::Came | Change::Changed => {
                let t = (since - glide) / self.length.as_secs_f32().max(0.001);
                if !(0.0..1.0).contains(&t) {
                    0.0
                } else if t < 0.12 {
                    t / 0.12
                } else {
                    let rest = (1.0 - t) / 0.88;
                    rest * rest * (3.0 - 2.0 * rest)
                }
            }
        }
    }

    /// Whether nothing glows any more at `now`.
    pub fn done(&self, now: Instant) -> bool {
        now >= self.start + self.glide + self.length
    }
}

/// What glows of `m`: one that came or went, or one that stays and
/// changed its size or its color.
fn glowing(index: usize, m: &Move) -> Option<Glowing> {
    let change = change(m)?;
    let rect = if change == Change::Went { m.from } else { m.to };
    Some(Glowing {
        rect,
        change,
        index,
    })
}

/// How `m` changed from the version before, if it did more than move.
pub fn change(m: &Move) -> Option<Change> {
    Some(match m.role {
        Role::Arrive => Change::Came,
        Role::Leave => Change::Went,
        Role::Stay => {
            let resized = (m.from.width - m.to.width).abs() > SAME
                || (m.from.height - m.to.height).abs() > SAME;
            let recolored = match m.looks.fill {
                [Some(a), Some(b)] => a.iter().zip(&b).any(|(a, b)| (a - b).abs() > HUE),
                [a, b] => a.is_some() != b.is_some(),
            };
            if !resized && !recolored {
                return None;
            }
            Change::Changed
        }
    })
}

/// The glow over the diagram, in a layer of its own: a ring of its color
/// around each box and a wash over it, a wider halo outside.
pub struct GlowMarks<'a> {
    pub glow: &'a Glow,
    /// The glide a box that went fades out on, easing with its sheet.
    pub fade: Option<&'a Fade>,
    /// Where this version stands, gliding too while the view glides.
    pub view: View,
    pub glide: Option<Glide>,
    pub scale: f32,
}

impl GlowMarks<'_> {
    /// Where box `index` of the glide is drawn at `now`: a box that went
    /// eases with the sheet it fades on, not where it stood before.
    fn on_glide(
        &self,
        index: usize,
        grid: Grid,
        local: Rectangle,
        now: Instant,
    ) -> Option<Rectangle> {
        let fade = self.fade?;
        let morph = fade.morph.as_ref()?;
        let (old, new) = fade.bare.as_ref()?;
        let paint = super::morph::Paint {
            t: fade.shown.interpolate(0.0, 1.0, now).clamp(0.0, 1.0),
            morph,
            old_view: fade.old_view,
            old_size: fade.old_size,
            old,
            new_view: self.glide.map_or(fade.new_view, |glide| glide.at(now)),
            new_size: fade.new_size,
            new,
            background: Color::TRANSPARENT,
        };
        super::morph::box_at(grid, local, &paint, index)
    }
}

impl canvas::Program<Message> for GlowMarks<'_> {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &Event,
        _bounds: Rectangle,
        _cursor: Cursor,
    ) -> Option<Action<Message>> {
        match event {
            Event::Window(window::Event::RedrawRequested(now)) if !self.glow.done(*now) => {
                Some(Action::request_redraw())
            }
            _ => None,
        }
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
        let view = self.glide.map_or(self.view, |glide| glide.at(now));
        let grid = Grid::of(bounds, self.scale);
        let local = Rectangle::with_size(bounds.size());
        for glowing in &self.glow.boxes {
            let strength = self.glow.strength(glowing.change, now);
            if strength <= 0.0 {
                continue;
            }
            let at = match glowing.change {
                Change::Went => self.on_glide(glowing.index, grid, local, now),
                Change::Came | Change::Changed => Some(view.on_screen(glowing.rect, local)),
            };
            let Some(at) = at else {
                continue;
            };
            let rect = grid.snap(at);
            if rect.width <= 1.0 || rect.height <= 1.0 {
                continue;
            }
            let color = glowing.change.color();
            let tinted = |a: f32| Color {
                a: a * strength,
                ..color
            };
            let radius = 6.0_f32.min(rect.width.min(rect.height) * 0.5);
            let grow = 4.0;
            let halo = Rectangle::new(
                iced::Point::new(rect.x - grow, rect.y - grow),
                iced::Size::new(rect.width + 2.0 * grow, rect.height + 2.0 * grow),
            );
            frame.stroke(
                &rounded(halo, radius + grow),
                Stroke::default().with_color(tinted(0.28)).with_width(6.0),
            );
            let ring = rounded(rect, radius);
            frame.fill(&ring, tinted(0.14));
            frame.stroke(
                &ring,
                Stroke::default().with_color(tinted(0.95)).with_width(2.5),
            );
        }
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use iced::{Point, Size};

    use super::super::morph::Looks;
    use super::*;

    fn moved(role: Role, from: (f32, f32), to: (f32, f32), fill: [Option<[f32; 3]>; 2]) -> Move {
        let rect = |(w, h): (f32, f32)| Rectangle::new(Point::new(10.0, 10.0), Size::new(w, h));
        Move {
            name: "A".into(),
            from: rect(from),
            to: rect(to),
            parts: [String::new(), String::new()],
            anchored: false,
            looks: Looks {
                fill,
                ..Looks::default()
            },
            role,
            into: None,
        }
    }

    #[test]
    fn what_came_changed_and_went_glows_and_what_only_moved_does_not() {
        let red = Some([1.0, 0.0, 0.0]);
        let blue = Some([0.0, 0.0, 1.0]);
        let morph = Morph {
            moves: vec![
                moved(Role::Arrive, (40.0, 20.0), (40.0, 20.0), [None, None]),
                moved(Role::Leave, (40.0, 20.0), (40.0, 20.0), [None, None]),
                moved(Role::Stay, (40.0, 20.0), (60.0, 20.0), [None, None]),
                moved(Role::Stay, (40.0, 20.0), (40.0, 20.0), [red, blue]),
                // Moved alone, and a little larger in its new place.
                moved(Role::Stay, (40.0, 20.0), (41.0, 20.0), [red, red]),
            ],
            orphans: [Vec::new(), Vec::new()],
            kept: Vec::new(),
        };
        let start = Instant::now();
        let glide = Duration::from_millis(500);
        let glow = Glow::of(&morph, start, glide, GLOW).unwrap();
        assert_eq!(
            glow.boxes.iter().map(|g| g.change).collect::<Vec<_>>(),
            [Change::Came, Change::Went, Change::Changed, Change::Changed]
        );
        // What went is red as it fades, at once; the rest waits for the
        // glide to end, glows, and is gone after GLOW.
        assert!(glow.strength(Change::Went, start) > 0.9);
        assert_eq!(glow.strength(Change::Went, start + glide), 0.0);
        assert_eq!(glow.strength(Change::Came, start + glide / 2), 0.0);
        let lit = start + glide + GLOW / 5;
        assert!(glow.strength(Change::Came, lit) > 0.8);
        assert!(!glow.done(lit));
        assert_eq!(glow.strength(Change::Changed, start + glide + GLOW), 0.0);
        assert!(glow.done(start + glide + GLOW));
        // A commit that stays a short while glows as long, and no longer
        // than GLOW however long it stays.
        let quick = Glow::of(&morph, start, glide, GLOW / 4).unwrap();
        assert!(quick.strength(Change::Came, start + glide + GLOW / 40) > 0.0);
        assert!(quick.done(start + glide + GLOW / 4));
        let long = Glow::of(&morph, start, glide, GLOW * 3).unwrap();
        assert!(long.done(start + glide + GLOW));
        // A glide where boxes only moved glows not at all.
        let quiet = Morph {
            moves: vec![moved(Role::Stay, (40.0, 20.0), (40.0, 20.0), [red, red])],
            ..morph
        };
        assert!(Glow::of(&quiet, start, glide, GLOW).is_none());
    }
}
