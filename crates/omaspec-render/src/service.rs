//! Rendering on a background thread, latest request first.
//!
//! While someone types, requests arrive faster than diagrams render. The
//! service works on the newest request of each slot (one diagram on
//! screen) only: an older request of that slot that queued up while it was
//! busy is skipped, and every result carries the [`Generation`] of its
//! request, so the caller can ignore results that are already stale.
//! Requests of other slots wait their turn.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Sender};
use std::thread;

use omaspec_core::Language;

use crate::{Config, Engine, Place, RenderError, Rendered, Renderer};

/// Counts requests; a higher generation is newer.
pub type Generation = u64;

struct Job {
    generation: Generation,
    slot: u64,
    source: String,
    language: Language,
    place: Option<Place>,
}

/// The result of one request.
#[derive(Debug)]
pub struct Outcome {
    pub generation: Generation,
    pub result: Result<Rendered, RenderError>,
}

pub struct RenderService {
    jobs: Sender<Job>,
    next: Generation,
}

impl RenderService {
    /// Starts the render thread. `deliver` runs on that thread for every
    /// finished request. The thread ends when the service is dropped.
    pub fn spawn(config: Config, mut deliver: impl FnMut(Outcome) + Send + 'static) -> Self {
        let (jobs, queue) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("omaspec-render".into())
            // Room for deep layouts; only the pages used are taken.
            .stack_size(64 << 20)
            .spawn(move || {
                let mut renderer = Renderer::new(&config);
                // Oldest first; at most one per slot.
                let mut waiting: Vec<Job> = Vec::new();
                loop {
                    if waiting.is_empty() {
                        let Ok(job) = queue.recv() else {
                            break;
                        };
                        waiting.push(job);
                    }
                    // Only the newest request of a slot matters.
                    while let Ok(newer) = queue.try_recv() {
                        waiting.retain(|job| job.slot != newer.slot);
                        waiting.push(newer);
                    }
                    let job = waiting.remove(0);
                    let result = render_guarded(&mut renderer, &config, &job);
                    deliver(Outcome {
                        generation: job.generation,
                        result,
                    });
                }
            })
            .expect("the render thread starts");
        Self { jobs, next: 1 }
    }

    /// Queues a diagram for `slot` and returns the generation its outcome
    /// will carry.
    pub fn submit(&mut self, slot: u64, source: String, language: Language) -> Generation {
        self.submit_at(slot, source, language, None)
    }

    /// The same for a diagram at `place`, whose `!include`s are read there.
    pub fn submit_at(
        &mut self,
        slot: u64,
        source: String,
        language: Language,
        place: Option<Place>,
    ) -> Generation {
        let generation = self.next;
        self.next += 1;
        // The thread only stops when `self` is dropped, so sending succeeds.
        let _ = self.jobs.send(Job {
            generation,
            slot,
            source,
            language,
            place,
        });
        generation
    }
}

/// A panic in an engine ends its request, not the thread: without the
/// thread, the preview would stand still for the rest of the session. The
/// renderer starts afresh, in case the panic left it half way.
fn render_guarded(
    renderer: &mut Renderer,
    config: &Config,
    job: &Job,
) -> Result<Rendered, RenderError> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        renderer.render_at(&job.source, job.language, job.place.as_ref())
    }));
    result.unwrap_or_else(|_| {
        *renderer = Renderer::new(config);
        Err(RenderError::Engine {
            engine: match job.language {
                Language::Mermaid => Engine::Mermaid,
                Language::PlantUml => Engine::NativePlantUml,
            },
            message: "the engine failed on this diagram".into(),
        })
    })
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    #[test]
    fn delivers_results_and_skips_overtaken_requests() {
        let (tx, rx) = mpsc::channel();
        let mut service = RenderService::spawn(Config::default(), move |o| {
            let _ = tx.send(o);
        });
        // A slow first diagram keeps the thread busy while the others queue.
        service.submit(0, slow(), Language::Mermaid);
        for i in 0..20 {
            service.submit(
                0,
                format!("sequenceDiagram\n  A->>B: {i}\n"),
                Language::Mermaid,
            );
        }
        let last = service.submit(0, "pie\n  \"a\": 1\n".into(), Language::Mermaid);

        let mut seen = Vec::new();
        loop {
            let outcome = rx
                .recv_timeout(Duration::from_secs(30))
                .expect("an outcome");
            assert!(outcome.result.is_ok(), "{:?}", outcome.result);
            seen.push(outcome.generation);
            if outcome.generation == last {
                break;
            }
        }
        assert!(seen.windows(2).all(|w| w[0] < w[1]), "{seen:?}");
        assert!(
            seen.len() < 22,
            "overtaken requests were rendered: {seen:?}"
        );
    }

    #[test]
    fn every_slot_gets_its_newest_request_drawn() {
        let (tx, rx) = mpsc::channel();
        let mut service = RenderService::spawn(Config::default(), move |o| {
            let _ = tx.send(o);
        });
        service.submit(0, slow(), Language::Mermaid);
        let tabs: Vec<Generation> = (1..=3)
            .map(|slot| {
                service.submit(slot, "pie\n  \"a\": 1\n".into(), Language::Mermaid);
                service.submit(slot, "pie\n  \"b\": 1\n".into(), Language::Mermaid)
            })
            .collect();

        let mut seen = Vec::new();
        while !tabs.iter().all(|generation| seen.contains(generation)) {
            let outcome = rx
                .recv_timeout(Duration::from_secs(30))
                .expect("every slot gets an outcome");
            seen.push(outcome.generation);
        }
        assert!(seen.windows(2).all(|w| w[0] < w[1]), "{seen:?}");
    }

    /// A diagram that takes a while, so later requests queue behind it.
    fn slow() -> String {
        let mut slow = String::from("flowchart TD\n");
        for i in 0..60 {
            slow.push_str(&format!("  n{i} --> n{}\n", (i * 7 + 3) % 60));
        }
        slow
    }
}
