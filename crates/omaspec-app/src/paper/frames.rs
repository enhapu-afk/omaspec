//! How smooth a motion of the diagram is, logged to standard error with
//! `OMASPEC_FRAMES=1`: a zoom into a link or out of it (a dive), a stepped
//! zoom or a fit (a glide), a play, and a new picture fading in.
//!
//! Each layer of the pane is drawn through [`Timed`]. A frame's *gap* is
//! the time from the redraw before it to its own, which is what the eye
//! sees: 16.7 ms on a 60 Hz screen while every frame is ready in time,
//! 33 ms when one missed its refresh. A frame's *work* is the time from
//! the start of its redraw to the end of the last layer drawn: the update,
//! the view and the shapes, not the GPU's part, which shows only in the
//! next gap. Each frame that came late has a line of its own, with what
//! happened since the frame before it, or in the [`BEFORE`] before it
//! ([`note`], [`took`]): new pixels
//! that came, an update or a view that took long. Each motion ends with a
//! line: how long it took, its gaps and its work.

use std::collections::VecDeque;
use std::fmt::{self, Write as _};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use iced::mouse::{self, Cursor};
use iced::time::Instant;
use iced::widget::canvas::{self, Action, Event, Geometry};
use iced::window::{self, RedrawRequest};
use iced::{Rectangle, Renderer, Theme};

/// A frame of a 60 Hz screen.
const BUDGET: Duration = Duration::from_micros(16_667);
/// A gap longer than this missed a 60 Hz refresh; one up to it is the
/// jitter of a screen's clock.
const LATE: Duration = Duration::from_millis(20);
/// A motion no frame came for in this long has ended, though it asked for
/// another one (its pane went, as at the end of a dive).
const GONE: Duration = Duration::from_secs(1);
/// A late frame names what happened this long before it.
pub const BEFORE: Duration = Duration::from_millis(50);
/// An update or a view is noted when it takes longer than this.
const NOTED: Duration = Duration::from_millis(1);

/// What moves on the diagram's pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Dive,
    Glide,
    Play,
    Fade,
}

impl Motion {
    fn name(self) -> &'static str {
        match self {
            Motion::Dive => "dive",
            Motion::Glide => "glide",
            Motion::Play => "play",
            Motion::Fade => "fade",
        }
    }
}

/// Whether frames are logged: `OMASPEC_FRAMES` set to anything but `0`.
pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var_os("OMASPEC_FRAMES").is_some_and(|v| !v.is_empty() && v != "0")
    })
}

/// A layer of the diagram's pane, timed while [`on`]. The layer that
/// drives a motion (asks for the frames it is drawn in) names it.
pub struct Timed<P> {
    pub program: P,
    pub drives: Option<Motion>,
}

impl<P> Timed<P> {
    pub fn new(program: P) -> Self {
        Self {
            program,
            drives: None,
        }
    }

    /// The layer asks for the frames of `motion`, while there is one.
    pub fn driving(self, motion: Option<Motion>) -> Self {
        Self {
            drives: motion,
            ..self
        }
    }
}

impl<M: Clone, P: canvas::Program<M>> canvas::Program<M> for Timed<P> {
    type State = P::State;

    fn update(
        &self,
        state: &mut P::State,
        event: &Event,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Option<Action<M>> {
        let action = self.program.update(state, event, bounds, cursor);
        if on()
            && let Event::Window(window::Event::RedrawRequested(now)) = event
        {
            let more = action
                .as_ref()
                .is_some_and(|a| matches!(a.clone().into_inner().1, RedrawRequest::NextFrame));
            let mut log = log();
            let mut lines = log.redraw(*now);
            if let Some(motion) = self.drives {
                lines.extend(log.frame(motion, *now, more));
            }
            drop(log);
            for line in lines {
                eprintln!("{line}");
            }
        }
        action
    }

    fn draw(
        &self,
        state: &P::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> Vec<Geometry> {
        let drawn = self.program.draw(state, renderer, theme, bounds, cursor);
        if on() {
            log().drew(Instant::now());
        }
        drawn
    }

    fn mouse_interaction(
        &self,
        state: &P::State,
        bounds: Rectangle,
        cursor: Cursor,
    ) -> mouse::Interaction {
        self.program.mouse_interaction(state, bounds, cursor)
    }
}

/// Something happened that may hold up the next frames: a late frame
/// names it.
pub fn note(what: impl FnOnce() -> String) {
    if on() {
        log().note(Instant::now(), what());
    }
}

/// `what` took `took`; noted when that is long.
pub fn took(what: impl FnOnce() -> String, took: Duration) {
    if on() && took > NOTED {
        log().note(Instant::now(), format!("{} took {}", what(), ms(took)));
    }
}

/// Every motion under way has ended: its pane went, before the frame that
/// ends it came (a zoom into a link ends as its tab shows).
pub fn end() {
    if on() {
        let lines: Vec<String> = log().runs.drain(..).map(|run| run.summary()).collect();
        for line in lines {
            eprintln!("{line}");
        }
    }
}

/// `value` as `Debug` writes it, cut short: a message can hold much.
pub fn short(value: &impl fmt::Debug) -> String {
    struct Short(String);
    impl fmt::Write for Short {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let room = 60usize.saturating_sub(self.0.len());
            let cut = s.char_indices().nth(room).map_or(s.len(), |(i, _)| i);
            self.0.push_str(&s[..cut]);
            if cut < s.len() {
                Err(fmt::Error)
            } else {
                Ok(())
            }
        }
    }
    let mut out = Short(String::new());
    if write!(out, "{value:?}").is_err() {
        out.0.push('…');
    }
    out.0
}

fn log() -> std::sync::MutexGuard<'static, Log> {
    static LOG: Mutex<Log> = Mutex::new(Log {
        redraw: None,
        runs: Vec::new(),
        notes: VecDeque::new(),
    });
    LOG.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The frames seen so far, and the motions under way.
#[derive(Default)]
struct Log {
    /// The redraw under way, and when its last layer was drawn.
    redraw: Option<(Instant, Option<Instant>)>,
    runs: Vec<Run>,
    /// What happened lately, for a late frame to name.
    notes: VecDeque<(Instant, String)>,
}

/// A motion under way.
struct Run {
    motion: Motion,
    /// The redraw of each of its frames, and its work once known.
    frames: Vec<(Instant, Option<Duration>)>,
}

impl Log {
    /// A redraw at `now` begins: the one before is done, and motions no
    /// frame came for in a while have ended.
    fn redraw(&mut self, now: Instant) -> Vec<String> {
        match self.redraw {
            Some((at, _)) if at == now => return Vec::new(),
            Some((at, Some(drawn))) => {
                let work = drawn.saturating_duration_since(at);
                for run in &mut self.runs {
                    if let Some(last) = run.frames.last_mut().filter(|f| f.0 == at) {
                        last.1 = Some(work);
                    }
                }
            }
            _ => {}
        }
        self.redraw = Some((now, None));
        let (gone, runs) = std::mem::take(&mut self.runs)
            .into_iter()
            .partition(|run: &Run| {
                run.frames
                    .last()
                    .is_some_and(|f| now.saturating_duration_since(f.0) > GONE)
            });
        self.runs = runs;
        gone.iter().map(Run::summary).collect()
    }

    fn note(&mut self, at: Instant, what: String) {
        while self
            .notes
            .front()
            .is_some_and(|(then, _)| at.saturating_duration_since(*then) > GONE)
        {
            self.notes.pop_front();
        }
        if self.notes.len() == 64 {
            self.notes.pop_front();
        }
        self.notes.push_back((at, what));
    }

    /// What happened since `since`, or in the [`BEFORE`] up to `now` if
    /// that is longer, each with how long before `now`.
    fn before(&self, since: Instant, now: Instant) -> String {
        let since = since.min(now.checked_sub(BEFORE).unwrap_or(since));
        let seen: Vec<String> = self
            .notes
            .iter()
            .filter(|(at, _)| since <= *at && *at <= now)
            .map(|(at, what)| format!("{} before: {what}", ms(now.saturating_duration_since(*at))))
            .collect();
        if seen.is_empty() {
            String::new()
        } else {
            format!("; {}", seen.join("; "))
        }
    }

    /// The last layer drawn so far of this redraw is done at `at`.
    fn drew(&mut self, at: Instant) {
        if let Some((_, drawn)) = &mut self.redraw {
            *drawn = Some(at);
        }
    }

    /// A frame of `motion` at `now`; `more` when it asks for another.
    fn frame(&mut self, motion: Motion, now: Instant, more: bool) -> Vec<String> {
        let mut lines = Vec::new();
        let index = match self.runs.iter().position(|r| r.motion == motion) {
            Some(index) => index,
            // A redraw of a layer that asks for no frame: nothing moves.
            None if !more => return lines,
            None => {
                self.runs.push(Run {
                    motion,
                    frames: Vec::new(),
                });
                self.runs.len() - 1
            }
        };
        let since = self.runs[index].frames.last().map_or(now, |f| f.0);
        let named = self.before(since, now);
        let run = &mut self.runs[index];
        if run.frames.last().is_some_and(|f| f.0 == now) {
            // A second layer driving the same motion.
        } else {
            if let Some(&(before, work)) = run.frames.last() {
                let gap = now.saturating_duration_since(before);
                if gap > LATE {
                    let line = format!(
                        "frames: {} frame {} came {} after the one before{}",
                        motion.name(),
                        run.frames.len() + 1,
                        ms(gap),
                        work.map_or(String::new(), |w| format!(", which took {} to draw", ms(w))),
                    );
                    lines.push(line + &named);
                }
            }
            run.frames.push((now, None));
        }
        if !more {
            lines.push(self.runs.remove(index).summary());
        }
        lines
    }
}

impl Run {
    /// One line on the whole motion.
    fn summary(&self) -> String {
        let name = self.motion.name();
        let (Some(first), Some(last)) = (self.frames.first(), self.frames.last()) else {
            return format!("frames: {name}, no frame");
        };
        let mut gaps: Vec<Duration> = self
            .frames
            .windows(2)
            .map(|w| w[1].0.saturating_duration_since(w[0].0))
            .collect();
        let mut works: Vec<Duration> = self.frames.iter().filter_map(|f| f.1).collect();
        let late = gaps.iter().filter(|g| **g > LATE).count();
        let over = works.iter().filter(|w| **w > BUDGET).count();
        let mut line = format!(
            "frames: {name} of {}, {} frame{}",
            ms(last.0.saturating_duration_since(first.0)),
            self.frames.len(),
            if self.frames.len() == 1 { "" } else { "s" },
        );
        if let Some((median, most)) = spread(&mut gaps) {
            line += &format!(
                ", {} apart (median), {} at most, {late} late",
                ms(median),
                ms(most)
            );
        }
        if let Some((median, most)) = spread(&mut works) {
            line += &format!(
                "; drawn in {} (median), {} at most, {over} over {}",
                ms(median),
                ms(most),
                ms(BUDGET)
            );
        }
        line
    }
}

/// The median and the largest of `values`.
fn spread(values: &mut [Duration]) -> Option<(Duration, Duration)> {
    values.sort_unstable();
    Some((*values.get(values.len() / 2)?, *values.last()?))
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn a_motion_ends_with_its_gaps_and_its_work() {
        let start = Instant::now();
        let mut log = Log::default();
        let mut lines = Vec::new();
        for (i, t) in [0, 17, 33, 50].into_iter().enumerate() {
            lines.extend(log.redraw(at(start, t)));
            // Two layers drawn, the last one 2 ms into the frame; the
            // second one drawn is the one that drives the motion.
            lines.extend(log.frame(Motion::Glide, at(start, t), i < 3));
            log.drew(at(start, t + 1));
            log.drew(at(start, t + 2));
        }
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = &lines[0];
        assert!(
            line.starts_with("frames: glide of 50.0 ms, 4 frames"),
            "{line}"
        );
        assert!(
            line.contains("17.0 ms apart (median), 17.0 ms at most, 0 late"),
            "{line}"
        );
        // The last frame's work is still under way when the motion ends.
        assert!(
            line.contains("drawn in 2.0 ms (median), 2.0 ms at most, 0 over"),
            "{line}"
        );
    }

    #[test]
    fn a_late_frame_is_named_with_the_work_before_it() {
        let start = Instant::now();
        let mut log = Log::default();
        let mut lines = Vec::new();
        lines.extend(log.redraw(start));
        lines.extend(log.frame(Motion::Dive, start, true));
        log.drew(at(start, 25));
        lines.extend(log.redraw(at(start, 33)));
        lines.extend(log.frame(Motion::Dive, at(start, 33), false));
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(
            lines[0],
            "frames: dive frame 2 came 33.0 ms after the one before, which took 25.0 ms to draw"
        );
        assert!(
            lines[1].contains("1 late; drawn in 25.0 ms"),
            "{}",
            lines[1]
        );
        assert!(lines[1].ends_with("1 over 16.7 ms"), "{}", lines[1]);
    }

    #[test]
    fn a_late_frame_names_what_came_since_the_frame_before() {
        let start = Instant::now();
        let mut log = Log::default();
        log.note(at(start, 2), "long ago".into());
        let _ = log.redraw(at(start, 10));
        let _ = log.frame(Motion::Glide, at(start, 10), true);
        log.note(at(start, 20), "overview 1904×1200 px shown".into());
        log.note(at(start, 31), "update of ZoomStep took 2.0 ms".into());
        let _ = log.redraw(at(start, 74));
        let lines = log.frame(Motion::Glide, at(start, 74), true);
        assert_eq!(
            lines,
            ["frames: glide frame 2 came 64.0 ms after the one before; \
              54.0 ms before: overview 1904×1200 px shown; \
              43.0 ms before: update of ZoomStep took 2.0 ms"]
        );
    }

    #[test]
    fn a_long_message_is_cut_short() {
        let long = vec![7u8; 1000];
        let cut = short(&long);
        assert!(cut.chars().count() <= 61 && cut.ends_with('…'), "{cut}");
        assert_eq!(short(&"Fit"), "\"Fit\"");
    }

    #[test]
    fn a_redraw_that_asks_for_no_frame_is_no_motion() {
        let start = Instant::now();
        let mut log = Log::default();
        assert!(log.redraw(start).is_empty());
        assert!(log.frame(Motion::Play, start, false).is_empty());
        assert!(log.runs.is_empty());
    }

    #[test]
    fn layers_of_one_redraw_count_once() {
        let start = Instant::now();
        let mut log = Log::default();
        for t in [0, 16] {
            for _ in 0..3 {
                assert!(log.redraw(at(start, t)).is_empty());
                assert!(log.frame(Motion::Play, at(start, t), true).is_empty());
            }
        }
        assert_eq!(log.runs[0].frames.len(), 2);
    }

    #[test]
    fn a_motion_whose_pane_went_ends_at_the_next_redraw_after_a_while() {
        let start = Instant::now();
        let mut log = Log::default();
        assert!(log.redraw(start).is_empty());
        assert!(log.frame(Motion::Dive, start, true).is_empty());
        assert!(log.redraw(at(start, 500)).is_empty());
        let lines = log.redraw(at(start, 1500));
        assert_eq!(lines, ["frames: dive of 0.0 ms, 1 frame"]);
        assert!(log.runs.is_empty());
    }
}
