//! The time the window's motions are drawn at: the wall clock, except
//! while `omaspec play` records a picture. Then it is a clock of its own,
//! which moves on by one frame for each frame drawn, so the picture gets
//! every frame of every motion however long one takes to draw (without a
//! GPU, longer than the frame lasts), and no frame waits for the wall
//! clock to catch up.
//!
//! The clock is the thread's: a recording draws on the thread that
//! records, and tests on other threads keep the wall clock.

use std::cell::Cell;
use std::time::{Duration, Instant};

thread_local! {
    static RECORDING: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Now, on the clock the motions are drawn at.
pub fn now() -> Instant {
    RECORDING.with(Cell::get).unwrap_or_else(Instant::now)
}

/// The clock of a recording, from the wall clock's now, until it is
/// dropped. It moves only by [`advance`].
pub struct Recording(());

impl Recording {
    pub fn start() -> Recording {
        RECORDING.with(|clock| clock.set(Some(Instant::now())));
        Recording(())
    }
}

/// Moves a recording's clock on by `by`, and says whether there is one;
/// the wall clock moves on its own.
pub fn advance(by: Duration) -> bool {
    RECORDING.with(|clock| {
        let at = clock.get();
        clock.set(at.map(|at| at + by));
        at.is_some()
    })
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORDING.with(|clock| clock.set(None));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_s_clock_moves_by_frames_only() {
        let wall = now();
        let recording = Recording::start();
        let start = now();
        assert!(start >= wall);
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(now(), start, "the wall clock moved, the recording's not");
        assert!(advance(Duration::from_millis(40)));
        assert_eq!(now(), start + Duration::from_millis(40));
        drop(recording);
        assert!(!advance(Duration::from_millis(40)), "no recording to move");
        assert!(now() >= start, "back on the wall clock");
    }

    #[test]
    fn another_thread_keeps_the_wall_clock() {
        let recording = Recording::start();
        let elsewhere = std::thread::spawn(|| advance(Duration::ZERO))
            .join()
            .unwrap();
        assert!(!elsewhere, "the other thread has no recording");
        drop(recording);
    }
}
