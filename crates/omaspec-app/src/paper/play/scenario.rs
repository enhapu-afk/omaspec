//! A scenario played: the veil lifted message by message while a light
//! runs along each arrow.

use std::time::Duration;

use iced::time::Instant;
use iced::widget::canvas;
use iced::widget::canvas::{Frame, Path};
use iced::{Color, Point, Rectangle, Size};
use omaspec_uml::map::{Ground, Step};

use super::super::canvas::fill_paper_band_across;
use super::super::light::{
    LINGER, Light, REVEAL, SCENARIO, appearance, draw_run, draw_run_over, ease_out, fraction,
    smooth, soft_spot,
};

use super::draw::*;
use super::*;

pub struct ScenarioPlay {
    pub(super) steps: Vec<Step>,
    /// The diagram's boxes with a fill of their own: the veil dims what
    /// lies on them toward their color ([`Ground`]).
    pub(super) grounds: Vec<Ground>,
    /// How many messages are shown.
    pub(super) at: usize,
    /// When the veil last moved, and from where.
    pub(super) moved: Instant,
    pub(super) from_y: f32,
    pub(super) begun: Instant,
    pub(super) jump: Option<usize>,
    /// The step just taken runs the light. Going back shows the line as it rests.
    pub(super) onward: bool,
    /// The message the highlight just left, so it can fade instead of vanishing.
    pub(super) leave: Option<Leaving>,
}

/// A message the highlight is leaving: the light as it stood, and the band
/// on screen at that moment. The fade starts from that light, so a line
/// that has already dimmed does not jump back to full brightness.
pub(super) struct Leaving {
    pub(super) index: usize,
    pub(super) light: Light,
    pub(super) spot: Spot4,
}

/// Where the highlight lies, in diagram units: top, bottom, left, right.
type Spot4 = (f32, f32, f32, f32);

/// How far the highlight reaches past a message's ends, in diagram units:
/// across, above the label, and below the arrow, so the arrow lies in its
/// full light and not in the fade, and the message above stays out.
pub(super) const SPOT_PAD: (f32, f32, f32) = (44.0, 2.0, 14.0);

/// The highlight of `step`: its row, from a little before the leftmost of
/// its points to a little past the rightmost.
pub(super) fn spot_of(step: &Step) -> Spot4 {
    let (left, right) = step
        .points
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), &(x, _)| {
            (l.min(x), r.max(x))
        });
    let (across, above, below) = SPOT_PAD;
    (
        step.top - above,
        step.bottom + below,
        left - across,
        right + across,
    )
}

impl ScenarioPlay {
    /// The message played last, from end to end and top to bottom, in
    /// diagram units: where a following camera looks. None before the
    /// first.
    pub(super) fn focus(&self) -> Option<iced::Rectangle> {
        let step = &self.steps[self.at.checked_sub(1)?];
        let (left, right) = step
            .points
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, r), p| (l.min(p.0), r.max(p.0)));
        (left <= right).then(|| {
            iced::Rectangle::new(
                iced::Point::new(left, step.top),
                iced::Size::new((right - left).max(1.0), (step.bottom - step.top).max(1.0)),
            )
        })
    }

    pub(super) fn ask(&mut self, ask: Ask, now: Instant) {
        let to = match ask {
            Ask::Next | Ask::Take(_) => (self.at + 1).min(self.steps.len()),
            Ask::Back => self.at.saturating_sub(1),
            Ask::Restart => 0,
            Ask::Hover(_) => return,
        };
        if to == self.at {
            return;
        }
        // Sample the light before the clocks move, so the fade starts from
        // the brightness on screen.
        self.leave = self.leaving(now);
        self.from_y = self.edge(now);
        self.onward = matches!(ask, Ask::Next | Ask::Take(_));
        self.moved = now;
        self.at = to;
        self.jump = to.checked_sub(1).map(|i| self.steps[i].line);
    }

    /// The highlight as it stands at `now`, kept so the next step can fade it.
    pub(super) fn leaving(&self, now: Instant) -> Option<Leaving> {
        let index = self.at.checked_sub(1)?;
        let k = ease_out(fraction(now.saturating_duration_since(self.moved), REVEAL));
        let spot = self
            .band_span(k)
            .unwrap_or_else(|| spot_of(&self.steps[index]));
        Some(Leaving {
            index,
            light: self.light_now(now)?,
            spot,
        })
    }

    /// The light of the message on screen at `now`.
    pub(super) fn light_now(&self, now: Instant) -> Option<Light> {
        let index = self.at.checked_sub(1)?;
        let since = now.saturating_duration_since(self.moved);
        let took = travel_time(&self.steps[index].points);
        let presence = ease_out(fraction(since, REVEAL));
        Some(appearance(since, took, self.onward, presence, SCENARIO))
    }

    /// Where the highlight sits at reveal progress `k`, gliding from the
    /// message left to the one shown.
    pub(super) fn band_span(&self, k: f32) -> Option<Spot4> {
        let to = self.at.checked_sub(1).map(|i| spot_of(&self.steps[i]));
        let from = self.leave.as_ref().map(|left| left.spot);
        match (from, to) {
            (Some(a), Some(b)) => Some((
                lerp(a.0, b.0, k),
                lerp(a.1, b.1, k),
                lerp(a.2, b.2, k),
                lerp(a.3, b.3, k),
            )),
            (None, Some(spot)) | (Some(spot), None) => Some(spot),
            (None, None) => None,
        }
    }

    pub(super) fn band_alpha(&self, k: f32) -> f32 {
        if self.at == 0 {
            1.0 - k
        } else if self.leave.is_none() {
            k
        } else {
            1.0
        }
    }

    /// Where the veil's upper edge is at `now`, in diagram units.
    pub(super) fn edge(&self, now: Instant) -> f32 {
        let target = self.target();
        let k = ease_out(fraction(now.saturating_duration_since(self.moved), REVEAL));
        self.from_y + (target - self.from_y) * k
    }

    pub(super) fn target(&self) -> f32 {
        match self.at {
            0 => self.steps[0].top,
            n => self.steps[n - 1].bottom,
        }
    }

    pub(super) fn settled(&self) -> Instant {
        self.moved + REVEAL.max(self.travel()) + LINGER + VEIL_FADE
    }

    pub(super) fn travel(&self) -> Duration {
        self.at
            .checked_sub(1)
            .map_or(Duration::ZERO, |i| travel_time(&self.steps[i].points))
    }

    pub(super) fn offer(&self) -> Offer {
        Offer::Scenario {
            at: self.at,
            of: self.steps.len(),
        }
    }

    pub(super) fn draw_under(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
        let veil = ramp(now, self.begun, VEIL_FADE);
        let edge = view.point((0.0, self.edge(now))).y;
        let paper = view.paper;
        // A long smooth edge, then the veil over what is still to come.
        // A short shelf read as a stripe travelling down the diagram.
        pub(super) const SOFT: f32 = 56.0;
        let top = edge.max(paper.y);
        let soft_end = (top + SOFT).min(paper.y + paper.height);
        let full = |color: Color| with_alpha(color, VEIL * veil);
        // The edge in steps of one color each, on whole pixels: as the
        // veil under it is drawn. A gradient showed a line at each of its
        // stops in some windows, a band where the reader looks.
        const STEPS: usize = 28;
        for step in 0..STEPS {
            let from = top + SOFT * step as f32 / STEPS as f32;
            let to = (top + SOFT * (step + 1) as f32 / STEPS as f32).min(soft_end);
            if to <= from {
                break;
            }
            let share = smooth((step as f32 + 0.5) / STEPS as f32);
            self.veil_band(frame, look, view, (from, to), |color| {
                with_alpha(full(color), share)
            });
        }
        let below = top + SOFT;
        if below < paper.y + paper.height {
            self.veil_band(frame, look, view, (below, paper.y + paper.height), full);
        }
    }

    /// The veil from `top` to `bottom`, in `fill` of the color it dims
    /// toward: the window's background over the paper, a box's own fill
    /// over the box ([`Ground`]), so a box under the veil keeps one shade
    /// and what lies on it is dimmed all the same. Side by side, the
    /// pieces meet on whole pixels.
    fn veil_band<F: Into<canvas::Fill>>(
        &self,
        frame: &mut Frame,
        look: &Look,
        view: &Viewer,
        (top, bottom): (f32, f32),
        fill: impl Fn(Color) -> F,
    ) {
        let paper = view.paper;
        let grounds: Vec<(Rectangle, Color)> = self
            .grounds
            .iter()
            .map(|ground| {
                let (rect, fill) = ((view.place)(ground.rect), ground.fill);
                (rect, Color::from_rgb8(fill.0, fill.1, fill.2))
            })
            .filter(|(rect, _)| {
                rect.y < bottom
                    && rect.y + rect.height > top
                    && rect.x < paper.x + paper.width
                    && rect.x + rect.width > paper.x
            })
            .collect();
        let mut cuts: Vec<f32> = grounds
            .iter()
            .flat_map(|(rect, _)| [rect.x, rect.x + rect.width])
            .filter(|x| *x > paper.x && *x < paper.x + paper.width)
            .collect();
        cuts.sort_by(f32::total_cmp);
        cuts.dedup();
        // Columns between the cuts; the outer ones end at the paper's side.
        let mut edges: Vec<Option<f32>> = vec![None];
        edges.extend(cuts.iter().copied().map(Some));
        edges.push(None);
        for pair in edges.windows(2) {
            let (left, right) = (pair[0], pair[1]);
            let middle = (left.unwrap_or(paper.x) + right.unwrap_or(paper.x + paper.width)) / 2.0;
            let on = grounds
                .iter()
                .find(|(rect, _)| rect.x <= middle && middle <= rect.x + rect.width);
            let mut piece = |from: f32, to: f32, color: Color| {
                if to > from {
                    fill_paper_band_across(
                        frame,
                        paper,
                        view.scale,
                        (from, to),
                        (left, right),
                        fill(color),
                    );
                }
            };
            match on {
                Some((rect, color)) => {
                    let (from, to) = (rect.y.max(top), (rect.y + rect.height).min(bottom));
                    piece(top, from, look.veil);
                    piece(from, to, *color);
                    piece(to, bottom, look.veil);
                }
                None => piece(top, bottom, look.veil),
            }
        }
    }

    pub(super) fn draw_over(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
        let k = ease_out(fraction(now.saturating_duration_since(self.moved), REVEAL));
        self.draw_band(frame, look, view, k);
        if k < 1.0
            && let Some(left) = &self.leave
        {
            self.draw_left(frame, look, view, left, 1.0 - k);
        }
        if let Some(index) = self.at.checked_sub(1) {
            self.draw_step(frame, look, view, index, now);
        }
    }

    /// The light around the message, gliding from the one before. It
    /// fades out on every side, so it has no hard line of its own, also
    /// where the paper's shadow begins.
    pub(super) fn draw_band(&self, frame: &mut Frame, look: &Look, view: &Viewer, k: f32) {
        let Some((top, bottom, left, right)) = self.band_span(k) else {
            return;
        };
        let alpha = self.band_alpha(k);
        if alpha <= 0.01 {
            return;
        }
        let (a, b) = (view.point((left, top)), view.point((right, bottom)));
        soft_spot(
            frame,
            view.paper,
            view.scale,
            Rectangle::new(a, Size::new(b.x - a.x, b.y - a.y)),
            with_alpha(look.accent, 0.09 * alpha),
        );
    }

    /// The message being left, held where its light was and fading out.
    pub(super) fn draw_left(
        &self,
        frame: &mut Frame,
        look: &Look,
        view: &Viewer,
        left: &Leaving,
        presence: f32,
    ) {
        let Some(step) = self.steps.get(left.index) else {
            return;
        };
        let screen: Vec<Point> = step.points.iter().map(|&p| view.point(p)).collect();
        // The line fades from the brightness it had, head included.
        draw_run(
            frame,
            &screen,
            look.accent,
            look.light,
            Light {
                head: left.light.head,
                rest: left.light.rest * presence,
                extra: left.light.extra * presence,
            },
        );
    }

    /// The message just shown. Going forward runs the light along it;
    /// going back shows the line as it rests.
    pub(super) fn draw_step(
        &self,
        frame: &mut Frame,
        look: &Look,
        view: &Viewer,
        index: usize,
        now: Instant,
    ) {
        let step = &self.steps[index];
        let screen: Vec<Point> = step.points.iter().map(|&p| view.point(p)).collect();
        let since = now.saturating_duration_since(self.moved);
        let took = travel_time(&step.points);
        let presence = ease_out(fraction(since, REVEAL));
        let light = appearance(since, took, self.onward, presence, SCENARIO);
        // The message is drawn under the light: what the head has not
        // reached yet waits in shade, so the run reads as it lights it.
        draw_run_over(
            frame,
            &screen,
            look.accent,
            look.light,
            light,
            Some(with_alpha(look.veil, AHEAD)),
        );
        if self.onward && since >= took && since < took + RIPPLE {
            let ripple = ease_out(fraction(since - took, RIPPLE));
            if ripple < 1.0
                && let Some(&tip) = screen.last()
            {
                frame.stroke(
                    &Path::circle(tip, 4.0 + 16.0 * ripple),
                    soft(look.accent, 2.0, 0.6 * (1.0 - ripple) * presence),
                );
            }
        }
    }
}
