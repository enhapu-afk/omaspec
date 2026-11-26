//! A scenario played: the veil lifted message by message while a light
//! runs along each arrow. A git graph plays so too, commit by commit, the
//! veil drawn back the way its time runs ([`Run`]).

use std::time::Duration;

use iced::time::Instant;
use iced::widget::canvas;
use iced::widget::canvas::{Frame, Path};
use iced::{Color, Point, Rectangle, Size};
use omaspec_uml::map::{Ground, Run, Step};

use super::super::canvas::fill_paper_band_across;
use super::super::light::{
    LINGER, Light, REVEAL, SCENARIO, appearance, draw_run, draw_run_over, ease_out, fraction,
    smooth, soft_spot,
};

use super::draw::*;
use super::*;

/// The veil's long smooth edge, before the veil over what is still to
/// come: a short shelf read as a stripe travelling down the diagram.
const SOFT: f32 = 56.0;
/// The edge in steps of one color each, on whole pixels: as the veil
/// under it is drawn. A gradient showed a line at each of its stops in
/// some windows, a band where the reader looks.
const STEPS: usize = 28;

/// Where the veil's edge goes with `at` of `steps` shown, the way they
/// `run`: over the first before any, past the last shown after.
pub(super) fn target(steps: &[Step], at: usize, run: Run) -> f32 {
    match (at, run) {
        (0, Run::Up) => steps[0].bottom,
        (0, _) => steps[0].top,
        (n, Run::Up) => steps[n - 1].top,
        (n, _) => steps[n - 1].bottom,
    }
}

pub struct ScenarioPlay {
    pub(super) steps: Vec<Step>,
    /// Which way the steps follow one another, and the veil goes back.
    pub(super) run: Run,
    /// Where a veil going back to the right or up begins, from the top, in
    /// diagram units: what stands over the steps stays clear of it.
    pub(super) clear: Option<f32>,
    /// Its steps are a git graph's commits.
    pub(super) commits: bool,
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
    /// The source lines of the messages a machine cannot take: their
    /// light runs in the color of what deviates.
    pub(super) wrong: Vec<usize>,
    /// The message the highlight just left, so it can fade instead of vanishing.
    pub(super) leave: Option<Leaving>,
    /// How much of the veil stood at the last move, the play's beginning
    /// aside ([`Self::standing`]): it goes on from there.
    pub(super) veil_from: f32,
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

/// How far `step` reaches across the way the steps run: its points and
/// the boxes lit with it (a note beside a message, or the note that is
/// the step), from the least to the most.
fn across_of(step: &Step, run: Run) -> (f32, f32) {
    let right = run == Run::Right;
    step.points
        .iter()
        .map(|p| if right { p.1 } else { p.0 })
        .chain(step.also.iter().flat_map(
            |&[x, y, w, h]| {
                if right { [y, y + h] } else { [x, x + w] }
            },
        ))
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), v| {
            (l.min(v), r.max(v))
        })
}

/// The highlight of `step`: its row, from a little before the leftmost of
/// its points and of the boxes lit with it to a little past the rightmost;
/// run to the right, its column, from a little above the highest to a
/// little below the lowest.
pub(super) fn spot_of(step: &Step, run: Run) -> Spot4 {
    let (across, above, below) = SPOT_PAD;
    let (low, high) = across_of(step, run);
    if run == Run::Right {
        (
            low - across,
            high + across,
            step.top - above,
            step.bottom + below,
        )
    } else {
        (
            step.top - above,
            step.bottom + below,
            low - across,
            high + across,
        )
    }
}

impl ScenarioPlay {
    /// The color the light of step `index` runs in.
    pub(super) fn color(&self, index: usize, look: &Look) -> Color {
        match self.steps.get(index) {
            Some(step) if self.wrong.contains(&step.line) => look.wrong,
            _ => look.accent,
        }
    }

    /// When the light of the message played last reaches its head.
    pub(super) fn arrives(&self) -> Option<Instant> {
        let step = self.steps.get(self.at.checked_sub(1)?)?;
        Some(self.moved + travel_time(&step.points))
    }

    /// The message played last, from end to end and top to bottom, in
    /// diagram units: where a following camera looks. None before the
    /// first.
    pub(super) fn focus(&self) -> Option<iced::Rectangle> {
        let step = &self.steps[self.at.checked_sub(1)?];
        let span = |of: fn(&(f32, f32)) -> f32| {
            step.points
                .iter()
                .map(of)
                .fold((f32::MAX, f32::MIN), |(l, r), v| (l.min(v), r.max(v)))
        };
        // Along the way the steps run, as far as the light comes from:
        // a merge's from what it took in, far back; across, its points and
        // the boxes lit with it.
        let along = |(low, high): (f32, f32)| (step.top.min(low), step.bottom.max(high));
        let across = across_of(step, self.run);
        let ((left, right), (top, bottom)) = if self.run == Run::Right {
            (along(span(|p| p.0)), across)
        } else {
            (across, along(span(|p| p.1)))
        };
        (left <= right && top <= bottom).then(|| {
            iced::Rectangle::new(
                iced::Point::new(left, top),
                iced::Size::new((right - left).max(1.0), (bottom - top).max(1.0)),
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
        self.veil_from = self.left_standing(now);
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
            .unwrap_or_else(|| spot_of(&self.steps[index], self.run));
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
        let to = self
            .at
            .checked_sub(1)
            .map(|i| spot_of(&self.steps[i], self.run));
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

    /// Where the veil's edge is at `now`, in diagram units, along the way
    /// the steps run: its upper edge, its left one to the right, its lower
    /// one up.
    pub(super) fn edge(&self, now: Instant) -> f32 {
        let target = self.target();
        let k = ease_out(fraction(now.saturating_duration_since(self.moved), REVEAL));
        self.from_y + (target - self.from_y) * k
    }

    pub(super) fn target(&self) -> f32 {
        target(&self.steps, self.at, self.run)
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
            commits: self.commits,
        }
    }

    /// How much of the veil stands at `now`. It comes up as the play
    /// begins; once the last step is shown and its light has come, it
    /// goes, as nothing is to come under it: the lifelines' feet stood
    /// veiled to the end, their heads bright over them (REQ-139). A step
    /// back from there brings it back.
    pub(super) fn standing(&self, now: Instant) -> f32 {
        ramp(now, self.begun, VEIL_FADE) * self.left_standing(now)
    }

    /// [`Self::standing`] without the veil's coming up at the beginning:
    /// from what stood at the last move toward all of it, or, at the end,
    /// as it stood until the last light has come, then away.
    fn left_standing(&self, now: Instant) -> f32 {
        if self.at >= self.steps.len() {
            let lit = ramp(now, self.moved + REVEAL.max(self.travel()), VEIL_FADE);
            self.veil_from * (1.0 - lit)
        } else {
            lerp(self.veil_from, 1.0, ramp(now, self.moved, VEIL_FADE))
        }
    }

    pub(super) fn draw_under(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant) {
        let veil = self.standing(now);
        if self.run != Run::Down {
            self.draw_back(frame, look, view, now, veil);
            return;
        }
        let edge = view.point((0.0, self.edge(now))).y;
        let paper = view.paper;
        let top = edge.max(paper.y);
        let soft_end = (top + SOFT).min(paper.y + paper.height);
        let full = |color: Color| with_alpha(color, VEIL * veil);
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

    /// The veil over what is still to come where the steps run to the right
    /// or up, its soft edge where the play has come to: a git graph's time.
    /// A git graph has no box of a fill of its own under it.
    fn draw_back(&self, frame: &mut Frame, look: &Look, view: &Viewer, now: Instant, veil: f32) {
        let paper = view.paper;
        let full = with_alpha(look.veil, VEIL * veil);
        let (right, bottom) = (paper.x + paper.width, paper.y + paper.height);
        // What stands over the steps, the title, stays clear.
        let top = self
            .clear
            .map_or(paper.y, |at| view.point((0.0, at)).y.max(paper.y));
        let band = |frame: &mut Frame, (from, to): (f32, f32), alpha: f32| {
            if to <= from {
                return;
            }
            let fill = with_alpha(full, alpha);
            if self.run == Run::Right {
                fill_paper_band_across(
                    frame,
                    paper,
                    view.scale,
                    (top, bottom),
                    (Some(from), (to < right).then_some(to)),
                    fill,
                );
            } else {
                fill_paper_band_across(frame, paper, view.scale, (from, to), (None, None), fill);
            }
        };
        // From the edge on into what is to come: to the right, or up.
        let (edge, end, sign) = if self.run == Run::Right {
            (view.point((self.edge(now), 0.0)).x.max(paper.x), right, 1.0)
        } else {
            (view.point((0.0, self.edge(now))).y.min(bottom), top, -1.0)
        };
        let at = |share: f32| edge + sign * SOFT * share;
        for step in 0..STEPS {
            let (a, b) = (
                at(step as f32 / STEPS as f32),
                at((step + 1) as f32 / STEPS as f32),
            );
            let (from, to) = if self.run == Run::Right {
                (a.min(b).max(paper.x), a.max(b).min(right))
            } else {
                (a.min(b).max(top), a.max(b).min(bottom))
            };
            band(
                frame,
                (from, to),
                smooth((step as f32 + 0.5) / STEPS as f32),
            );
        }
        let beyond = at(1.0);
        if self.run == Run::Right {
            band(frame, (beyond, end), 1.0);
        } else {
            band(frame, (end, beyond), 1.0);
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
        let color = self.color(self.at.saturating_sub(1), look);
        soft_spot(
            frame,
            view.paper,
            view.scale,
            Rectangle::new(a, Size::new(b.x - a.x, b.y - a.y)),
            with_alpha(color, 0.09 * alpha),
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
        // A note has no arrow for a light: its band alone lights it.
        let Some(step) = self.steps.get(left.index).filter(|s| s.points.len() > 1) else {
            return;
        };
        let screen: Vec<Point> = step.points.iter().map(|&p| view.point(p)).collect();
        // The line fades from the brightness it had, head included.
        draw_run(
            frame,
            &screen,
            self.color(left.index, look),
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
        // A note has no arrow for a light: its band alone lights it.
        if step.points.len() < 2 {
            return;
        }
        let screen: Vec<Point> = step.points.iter().map(|&p| view.point(p)).collect();
        let since = now.saturating_duration_since(self.moved);
        let took = travel_time(&step.points);
        let presence = ease_out(fraction(since, REVEAL));
        let light = appearance(since, took, self.onward, presence, SCENARIO);
        // The message is drawn under the light: what the head has not
        // reached yet waits in shade, so the run reads as it lights it.
        let color = self.color(index, look);
        draw_run_over(
            frame,
            &screen,
            color,
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
                    soft(color, 2.0, 0.6 * (1.0 - ripple) * presence),
                );
            }
        }
    }
}
