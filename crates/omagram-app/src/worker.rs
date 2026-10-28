//! Background work for Omagram's windows, as iced subscriptions:
//! [`service`] renders on the render thread, follows the Omarchy theme and
//! keeps a timer for things that wait for a pause; [`watch`] reports changes
//! to files.
//!
//! A window gets a [`Worker`] handle from the service first and sends render
//! requests and alarms through it. The two run apart, so opening another
//! file never restarts the render thread (and with it a Java PlantUML).

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self as std_mpsc, RecvTimeoutError};
use std::time::{Duration, Instant};

use iced::Subscription;
use iced::futures::channel::mpsc::{self, UnboundedSender};
use iced::futures::{SinkExt, Stream, StreamExt};
use notify::event::{AccessKind, AccessMode, ModifyKind};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use omagram_core::Language;
use omagram_render::export::{self, usvg};
use omagram_render::{Engine, Generation, Map, Place, RenderError, RenderService};

/// A diagram the render thread drew and parsed for display.
#[derive(Clone)]
pub struct Drawn {
    pub svg: Arc<str>,
    pub tree: Arc<usvg::Tree>,
    pub engine: Engine,
    pub elapsed: Duration,
    /// Where its states, transitions and messages are, for a run.
    pub map: Arc<Map>,
}

impl fmt::Debug for Drawn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Drawn")
            .field("engine", &self.engine)
            .field("elapsed", &self.elapsed)
            .finish_non_exhaustive()
    }
}

/// Names a render request: the document it is for and its number there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ticket {
    pub doc: u64,
    pub id: u64,
}

/// The answer to one render request.
#[derive(Debug, Clone)]
pub struct Answer {
    pub ticket: Ticket,
    pub source: Arc<str>,
    pub language: Language,
    pub result: Result<Drawn, RenderError>,
}

/// Something that waits for a pause: a burst of file events, typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alarm {
    File,
    Theme,
    Lint(u64),
    Render(u64),
    /// A line lit by a jump goes dark again.
    Flash(u64),
    /// Time to write the session.
    Session,
    Font,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// The service is up; requests go through this handle.
    Connected(Worker),
    Rendered(Box<Answer>),
    ThemeChanged,
    /// The fontconfig changed: `monospace` may be another font now.
    FontChanged,
    FileChanged(PathBuf),
    WatchFailed(String),
    Due(Alarm),
}

/// A window's handle for render requests and alarms.
#[derive(Debug, Clone)]
pub struct Worker {
    input: UnboundedSender<Input>,
    timer: std_mpsc::Sender<(Alarm, Instant)>,
}

impl Worker {
    /// Asks for `source` to be drawn; `place` is where the diagram's file
    /// is, so its `!include`s can be read.
    pub fn render(
        &self,
        ticket: Ticket,
        source: Arc<str>,
        language: Language,
        place: Option<Place>,
    ) {
        let _ = self.input.unbounded_send(Input::Render {
            ticket,
            source,
            language,
            place,
        });
    }

    /// Sends [`Event::Due`] for `alarm` after `delay`. Setting it again
    /// before then starts the wait over.
    pub fn after(&self, alarm: Alarm, delay: Duration) {
        let _ = self.timer.send((alarm, Instant::now() + delay));
    }
}

#[derive(Debug)]
enum Input {
    Render {
        ticket: Ticket,
        source: Arc<str>,
        language: Language,
        place: Option<Place>,
    },
    Done(Generation, Result<Drawn, RenderError>),
    Forward(Event),
}

/// What the service watches besides rendering: Omarchy's `current/`
/// state directory (theme switches) and the fontconfig directory (font
/// changes).
#[derive(Debug, Clone, Default, Hash)]
pub struct Watched {
    pub theme_dir: Option<PathBuf>,
    pub font_dir: Option<PathBuf>,
}

/// Rendering, the timer, theme switches and font changes.
pub fn service(watched: Watched) -> Subscription<Event> {
    Subscription::run_with(watched, service_stream)
}

fn service_stream(watched: &Watched) -> impl Stream<Item = Event> + use<> {
    let (theme_dir, font_dir) = (watched.theme_dir.clone(), watched.font_dir.clone());
    iced::stream::channel(64, async move |mut output| {
        let (input, mut inputs) = mpsc::unbounded::<Input>();
        let worker = Worker {
            input: input.clone(),
            timer: timer(input.clone()),
        };
        if output.send(Event::Connected(worker)).await.is_err() {
            return;
        }

        let done = input.clone();
        let mut service = RenderService::spawn(crate::commands::render_config(), move |outcome| {
            // Parse here, on the render thread, so the window never waits for it.
            let result = outcome.result.and_then(|rendered| {
                let tree = export::parse(&rendered.svg).map_err(|e| RenderError::Engine {
                    engine: rendered.engine,
                    message: e.to_string(),
                })?;
                Ok(Drawn {
                    svg: rendered.svg.into(),
                    tree: Arc::new(tree),
                    engine: rendered.engine,
                    elapsed: rendered.elapsed,
                    map: Arc::new(rendered.map),
                })
            });
            let _ = done.unbounded_send(Input::Done(outcome.generation, result));
        });

        // Dropping a watcher stops it, so it lives as long as the loop.
        let _theme_watch = theme_dir
            .as_ref()
            .map(|dir| watch_dir_changes(dir, true, Event::ThemeChanged, input.clone()));
        let _font_watch = font_dir
            .as_ref()
            .filter(|dir| dir.is_dir())
            .map(|dir| watch_dir_changes(dir, false, Event::FontChanged, input.clone()));

        // Render generation → (ticket, source, language).
        let mut requests: HashMap<Generation, (Ticket, Arc<str>, Language)> = HashMap::new();
        while let Some(item) = inputs.next().await {
            let event = match item {
                Input::Render {
                    ticket,
                    source,
                    language,
                    place,
                } => {
                    let generation =
                        service.submit_at(ticket.doc, source.to_string(), language, place);
                    requests.insert(generation, (ticket, source, language));
                    continue;
                }
                Input::Done(generation, result) => {
                    let Some((ticket, source, language)) = requests.remove(&generation) else {
                        continue;
                    };
                    // Requests of the same diagram overtaken by this one were
                    // skipped; forget them.
                    requests.retain(|&g, (other, ..)| g > generation || other.doc != ticket.doc);
                    Event::Rendered(Box::new(Answer {
                        ticket,
                        source,
                        language,
                        result,
                    }))
                }
                Input::Forward(event) => event,
            };
            if output.send(event).await.is_err() {
                break;
            }
        }
    })
}

/// One thread for all alarms; it ends with the service.
fn timer(input: UnboundedSender<Input>) -> std_mpsc::Sender<(Alarm, Instant)> {
    let (set, alarms) = std_mpsc::channel::<(Alarm, Instant)>();
    std::thread::Builder::new()
        .name("omagram-timer".into())
        .spawn(move || {
            let mut pending: Vec<(Alarm, Instant)> = Vec::new();
            loop {
                let received = match pending.iter().map(|&(_, at)| at).min() {
                    Some(at) => alarms.recv_timeout(at.saturating_duration_since(Instant::now())),
                    None => alarms.recv().map_err(|_| RecvTimeoutError::Disconnected),
                };
                match received {
                    Ok((alarm, at)) => {
                        pending.retain(|&(a, _)| a != alarm);
                        pending.push((alarm, at));
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                let now = Instant::now();
                pending.retain(|&(alarm, at)| {
                    let due = at <= now;
                    if due {
                        let _ = input.unbounded_send(Input::Forward(Event::Due(alarm)));
                    }
                    !due
                });
            }
        })
        .expect("the timer thread starts");
    set
}

/// Runs `f` on its own thread; `None` if it panicked.
pub fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> impl Future<Output = Option<T>> {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::Builder::new()
        .name("omagram-worker".into())
        // It reads and draws whole project folders, as the render thread
        // does; only the pages used are taken.
        .stack_size(64 << 20)
        .spawn(move || {
            let _ = tx.send(f());
        })
        .expect("a worker thread starts");
    async move { rx.await.ok() }
}

/// Changes to `files` (absolute paths). Editors often save by writing a new
/// file and renaming it over the old one, so their directories are watched,
/// not the files.
pub fn watch(files: Vec<PathBuf>) -> Subscription<Event> {
    Subscription::run_with(files, |files: &Vec<PathBuf>| watch_stream(files))
}

fn watch_stream(files: &[PathBuf]) -> impl Stream<Item = Event> + use<> {
    let files = files.to_vec();
    iced::stream::channel(16, async move |mut output| {
        let (report, mut events) = mpsc::unbounded::<Event>();
        let mut dirs: Vec<&Path> = files.iter().filter_map(|f| f.parent()).collect();
        dirs.sort();
        dirs.dedup();
        // Dropping a watcher stops it, so they live as long as the loop.
        let _watchers: Vec<RecommendedWatcher> = dirs
            .into_iter()
            .filter_map(|dir| watch_dir(dir, &files, report.clone()))
            .collect();
        while let Some(event) = events.next().await {
            if output.send(event).await.is_err() {
                break;
            }
        }
    })
}

fn watch_dir(
    dir: &Path,
    files: &[PathBuf],
    report: UnboundedSender<Event>,
) -> Option<RecommendedWatcher> {
    let wanted: Vec<PathBuf> = files
        .iter()
        .filter(|f| f.parent() == Some(dir))
        .cloned()
        .collect();
    let failed = report.clone();
    let watcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(e) if is_change(&e.kind) => {
                for path in wanted.iter().filter(|w| e.paths.contains(w)) {
                    let _ = report.unbounded_send(Event::FileChanged(path.clone()));
                }
            }
            Ok(_) => {}
            Err(err) => {
                let _ = report.unbounded_send(Event::WatchFailed(err.to_string()));
            }
        });
    match watcher.and_then(|mut w| w.watch(dir, RecursiveMode::NonRecursive).map(|()| w)) {
        Ok(watcher) => Some(watcher),
        Err(err) => {
            let _ = failed.unbounded_send(Event::WatchFailed(err.to_string()));
            None
        }
    }
}

/// Whether an event means the content changed. Reading a file (Omagram
/// reads the diagram and the theme) raises open and close events too; those
/// must not trigger another read, or a window would loop.
fn is_change(kind: &EventKind) -> bool {
    match kind {
        EventKind::Create(_) => true,
        EventKind::Modify(ModifyKind::Metadata(_)) => false,
        EventKind::Modify(_) => true,
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        _ => false,
    }
}

/// Sends `event` for every change in `dir`: Omarchy replaces what is in
/// `current/` on a theme switch, `omarchy font set` rewrites the
/// fontconfig.
fn watch_dir_changes(
    dir: &Path,
    recursive: bool,
    event: Event,
    input: UnboundedSender<Input>,
) -> Option<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |found: notify::Result<notify::Event>| {
        if found.is_ok_and(|e| is_change(&e.kind)) {
            let _ = input.unbounded_send(Input::Forward(event.clone()));
        }
    })
    .ok()?;
    let mode = if recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    watcher.watch(dir, mode).ok()?;
    Some(watcher)
}

#[cfg(test)]
mod tests {
    use notify::event::{CreateKind, DataChange, MetadataKind, RenameMode};

    use super::*;

    #[test]
    fn only_writes_count_as_changes() {
        assert!(is_change(&EventKind::Create(CreateKind::File)));
        assert!(is_change(&EventKind::Modify(ModifyKind::Data(
            DataChange::Content
        ))));
        assert!(is_change(&EventKind::Modify(ModifyKind::Name(
            RenameMode::To
        ))));
        assert!(is_change(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
        assert!(!is_change(&EventKind::Access(AccessKind::Open(
            AccessMode::Any
        ))));
        assert!(!is_change(&EventKind::Access(AccessKind::Close(
            AccessMode::Read
        ))));
        assert!(!is_change(&EventKind::Modify(ModifyKind::Metadata(
            MetadataKind::AccessTime
        ))));
    }

    /// A subscription's stream, run on its own thread as iced would.
    struct Events(std::sync::mpsc::Receiver<Event>);

    impl Events {
        fn start<S: Stream<Item = Event> + Send + 'static>(stream: S) -> Self {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                iced::futures::executor::block_on(async {
                    let mut events = std::pin::pin!(stream);
                    while let Some(event) = events.next().await {
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                });
            });
            Self(rx)
        }

        fn next(&self) -> Event {
            self.0
                .recv_timeout(Duration::from_secs(10))
                .expect("no event within 10 s")
        }

        /// Waits until nothing came for `quiet`; returns what came before.
        fn settle(&self, quiet: Duration) -> Vec<Event> {
            std::iter::from_fn(|| self.0.recv_timeout(quiet).ok()).collect()
        }

        fn worker(&self) -> Worker {
            match self.next() {
                Event::Connected(worker) => worker,
                other => panic!("the service starts with its handle, not {other:?}"),
            }
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("omagram-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn renders_the_newest_request() {
        let events = Events::start(service_stream(&Watched::default()));
        let worker = events.worker();
        let ticket = |id| Ticket { doc: 7, id };
        worker.render(
            ticket(1),
            "flowchart LR\n  A --> B\n".into(),
            Language::Mermaid,
            None,
        );
        worker.render(
            ticket(2),
            "flowchart LR\n  A --> C\n".into(),
            Language::Mermaid,
            None,
        );
        // The first request may be skipped for the second; the second always comes.
        let answer = loop {
            if let Event::Rendered(answer) = events.next()
                && answer.ticket == ticket(2)
            {
                break answer;
            }
        };
        assert!(answer.result.is_ok());
    }

    #[test]
    fn alarms_ring_once_after_the_last_setting() {
        let events = Events::start(service_stream(&Watched::default()));
        let worker = events.worker();
        let start = Instant::now();
        worker.after(Alarm::Render(1), Duration::from_millis(80));
        worker.after(Alarm::Lint(1), Duration::from_millis(20));
        std::thread::sleep(Duration::from_millis(40));
        worker.after(Alarm::Render(1), Duration::from_millis(80));
        assert!(matches!(events.next(), Event::Due(Alarm::Lint(1))));
        assert!(matches!(events.next(), Event::Due(Alarm::Render(1))));
        assert!(start.elapsed() >= Duration::from_millis(120));
        assert!(events.settle(Duration::from_millis(200)).is_empty());
    }

    #[test]
    fn reports_saves_of_watched_files_only() {
        let dir = temp_dir("watch-file");
        let file = dir.join("flow.mmd");
        std::fs::write(&file, "flowchart LR\n  A --> B\n").unwrap();
        let events = Events::start(watch_stream(std::slice::from_ref(&file)));
        // The watcher is set up once the stream runs.
        std::thread::sleep(Duration::from_millis(100));

        // Saving the way Neovim does: write a new file, rename it over the old.
        let swap = dir.join("flow.mmd~");
        std::fs::write(&swap, "pie\n  \"a\": 1\n").unwrap();
        std::fs::write(dir.join("other.mmd"), "pie\n").unwrap();
        std::fs::rename(&swap, &file).unwrap();
        assert!(matches!(events.next(), Event::FileChanged(path) if path == file));
        let rest = events.settle(Duration::from_millis(300));
        assert!(
            rest.iter()
                .all(|e| matches!(e, Event::FileChanged(path) if *path == file))
        );

        // Reading the file, as a window does next, must not count as a change.
        std::fs::read_to_string(&file).unwrap();
        assert!(events.settle(Duration::from_millis(300)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reports_font_changes() {
        let fonts = temp_dir("watch-fonts");
        let events = Events::start(service_stream(&Watched {
            theme_dir: None,
            font_dir: Some(fonts.clone()),
        }));
        let _ = events.worker();
        // What omarchy-font-set does.
        std::fs::write(fonts.join("fonts.conf"), "<fontconfig/>\n").unwrap();
        assert!(matches!(events.next(), Event::FontChanged));
        let _ = std::fs::remove_dir_all(&fonts);
    }

    #[test]
    fn reports_theme_switches() {
        let colors = |accent: &str| {
            format!("background = \"#101010\"\nforeground = \"#c0c0c0\"\naccent = \"{accent}\"\n")
        };
        let dir = temp_dir("watch-theme");
        let current = dir.join("current");
        std::fs::create_dir_all(current.join("theme")).unwrap();
        std::fs::write(current.join("theme/colors.toml"), colors("#111111")).unwrap();
        std::fs::write(current.join("theme.name"), "one\n").unwrap();
        let events = Events::start(service_stream(&Watched {
            theme_dir: Some(current.clone()),
            font_dir: None,
        }));
        let _ = events.worker();

        // What omarchy-theme-set does.
        let next = current.join("next-theme");
        std::fs::create_dir_all(&next).unwrap();
        std::fs::write(next.join("colors.toml"), colors("#222222")).unwrap();
        std::fs::remove_dir_all(current.join("theme")).unwrap();
        std::fs::rename(&next, current.join("theme")).unwrap();
        std::fs::write(current.join("theme.name"), "two\n").unwrap();
        assert!(matches!(events.next(), Event::ThemeChanged));
        let _ = events.settle(Duration::from_millis(300));

        // Loading the new theme must not count as another switch.
        let theme = omagram_theme::Theme::load_from(&current);
        assert_eq!(theme.name.as_deref(), Some("two"));
        assert_eq!(theme.palette.accent.to_string(), "#222222");
        assert!(events.settle(Duration::from_millis(300)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
