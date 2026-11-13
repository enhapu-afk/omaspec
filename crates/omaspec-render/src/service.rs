//! Rendering on a background thread, latest request first.
//!
//! While someone types, requests arrive faster than diagrams render. The
//! service works on the newest request of each slot (one diagram on
//! screen) only: an older request of that slot that queued up while it was
//! busy is skipped, and every result carries the [`Generation`] of its
//! request, so the caller can ignore results that are already stale.
//! Requests of other slots wait their turn.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;

use omaspec_core::Language;

use crate::{Atlas, Config, Engine, Place, RenderError, Rendered, Renderer};

/// Counts requests; a higher generation is newer.
pub type Generation = u64;

/// Lines laid after a request's text on the render thread, where they cost
/// nothing while typing: an overtaken request never makes them.
pub type After = Box<dyn FnOnce() -> String + Send>;

struct Job {
    generation: Generation,
    slot: u64,
    source: String,
    language: Language,
    place: Option<Place>,
    /// The project the diagram links into, and its own path there.
    linked: Option<(Arc<Atlas>, Option<String>)>,
    /// A Markdown document: all its diagrams ([`Renderer::render_document`]).
    document: bool,
    /// Its JSON and YAML diagrams drawn nested.
    nested: bool,
    /// The map of this folder of the project in `linked`, made from its
    /// diagrams ([`Renderer::render_map`]); `source` is the look it is
    /// drawn in.
    map: Option<String>,
    /// Lines to lay after `source` before it is drawn.
    after: Option<After>,
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
                    let mut job = waiting.remove(0);
                    let result = render_guarded(&mut renderer, &config, &mut job);
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
        self.submit_linked(slot, source, language, place, None)
    }

    /// The same for a diagram of a project that links to the diagrams in
    /// `linked`'s atlas, from its own path there
    /// ([`Renderer::render_linked`]).
    pub fn submit_linked(
        &mut self,
        slot: u64,
        source: String,
        language: Language,
        place: Option<Place>,
        linked: Option<(Arc<Atlas>, Option<String>)>,
    ) -> Generation {
        self.submit_after(slot, source, language, place, linked, None)
    }

    /// The same, with lines laid after `source` on the render thread
    /// ([`After`]).
    pub fn submit_after(
        &mut self,
        slot: u64,
        source: String,
        language: Language,
        place: Option<Place>,
        linked: Option<(Arc<Atlas>, Option<String>)>,
        after: Option<After>,
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
            linked,
            document: false,
            nested: false,
            map: None,
            after,
        });
        generation
    }

    /// Queues the map of `folder`, a folder of the project in `atlas` by
    /// its path there, in the look of `skin` ([`Renderer::render_map`]).
    pub fn submit_map(
        &mut self,
        slot: u64,
        skin: String,
        atlas: Arc<Atlas>,
        folder: String,
    ) -> Generation {
        let generation = self.next;
        self.next += 1;
        let _ = self.jobs.send(Job {
            generation,
            slot,
            source: skin,
            language: Language::PlantUml,
            place: None,
            linked: Some((atlas, None)),
            document: false,
            nested: false,
            map: Some(folder),
            after: None,
        });
        generation
    }

    /// Queues every diagram of the Markdown document `source` at `place`,
    /// drawn one under another.
    pub fn submit_document(
        &mut self,
        slot: u64,
        source: String,
        place: Option<Place>,
        nested: bool,
    ) -> Generation {
        let generation = self.next;
        self.next += 1;
        let _ = self.jobs.send(Job {
            generation,
            slot,
            source,
            language: Language::PlantUml,
            place,
            linked: None,
            document: true,
            nested,
            map: None,
            after: None,
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
    job: &mut Job,
) -> Result<Rendered, RenderError> {
    let after = job.after.take();
    let result = catch_unwind(AssertUnwindSafe(|| {
        if let Some(after) = after {
            job.source.push_str(&after());
        }
        render_job(renderer, job)
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

/// The job drawn by its kind.
fn render_job(renderer: &mut Renderer, job: &Job) -> Result<Rendered, RenderError> {
    match (&job.linked, &job.map) {
        _ if job.document => {
            renderer.render_document_with(&job.source, job.place.as_ref(), job.nested)
        }
        (Some((atlas, _)), Some(folder)) => renderer.render_map(atlas, folder, &job.source),
        (Some((atlas, own)), None) => renderer.render_linked(
            &job.source,
            job.language,
            job.place.as_ref(),
            own.as_deref(),
            atlas,
        ),
        (None, _) => renderer.render_at(&job.source, job.language, job.place.as_ref()),
    }
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

    #[test]
    fn lines_after_the_text_are_laid_on_the_render_thread() {
        let (tx, rx) = mpsc::channel();
        let mut service = RenderService::spawn(Config::default(), move |o| {
            let _ = tx.send(o);
        });
        let main = thread::current().id();
        let after: After = Box::new(move || {
            assert_ne!(thread::current().id(), main, "not on the caller's thread");
            "  \"later\": 2\n".into()
        });
        service.submit_after(
            0,
            "pie\n  \"a\": 1\n".into(),
            Language::Mermaid,
            None,
            None,
            Some(after),
        );
        let outcome = rx
            .recv_timeout(Duration::from_secs(30))
            .expect("an outcome");
        let svg = outcome.result.expect("drawn").svg;
        assert!(svg.contains("later"), "{svg}");
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
