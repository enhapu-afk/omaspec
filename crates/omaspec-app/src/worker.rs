//! Background work for Omaspec's windows, as iced subscriptions:
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
use notify::event::{AccessKind, AccessMode, CreateKind, ModifyKind, RemoveKind};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use omaspec_core::Language;
use omaspec_render::export::{self, usvg};
use omaspec_render::{Atlas, Engine, Generation, Link, Map, Place, RenderError, RenderService};

/// A diagram the render thread drew and parsed for display.
#[derive(Clone)]
pub struct Drawn {
    pub svg: Arc<str>,
    pub tree: Arc<usvg::Tree>,
    pub engine: Engine,
    pub elapsed: Duration,
    /// Where its states, transitions and messages are, for a run.
    pub map: Arc<Map>,
    /// Its tiles and states that open other diagrams of the project.
    pub links: Arc<Vec<Link>>,
    /// Whether `tree` is a large map's cards without their pictures
    /// (`Rendered::cards`); `svg` has them, for the pane to parse when it
    /// shows them.
    pub cards: bool,
    /// The picture without its lines, for a glide that draws them itself
    /// (`paper::wires`); a graph diagram has one.
    pub bare: Option<Arc<usvg::Tree>>,
}

/// A map's SVG up to this size is parsed whole at once, as quick to parse
/// as the pane can wait for (about 40 ms); a larger one is shown by its
/// cards first.
const WHOLE: usize = 128 << 10;

impl Drawn {
    /// `rendered` as the pane shows it, parsed here: a large map by its
    /// cards, its pictures when the pane asks for them.
    pub fn of(rendered: omaspec_render::Rendered) -> Result<Drawn, RenderError> {
        Self::parsed(rendered, WHOLE)
    }

    /// [`Self::of`], with a map's SVG up to `whole` bytes parsed whole.
    pub(crate) fn parsed(
        rendered: omaspec_render::Rendered,
        whole: usize,
    ) -> Result<Drawn, RenderError> {
        let cards = rendered
            .cards
            .as_deref()
            .filter(|_| rendered.svg.len() > whole);
        // The picture and the one without its lines are read side by
        // side: each takes as long as the other, a quarter of a second
        // for a tree of some thousand boxes.
        let wanted = cards.is_none() && !rendered.map.wires.is_empty();
        let (tree, bare) = std::thread::scope(|scope| {
            let bare = scope.spawn(|| {
                wanted
                    .then(|| omaspec_uml::without_lines(&rendered.svg))
                    .flatten()
                    .and_then(|svg| export::parse(&svg).ok())
                    .map(Arc::new)
            });
            let tree = export::parse(cards.unwrap_or(&rendered.svg));
            (tree, bare.join().ok().flatten())
        });
        let tree = tree.map_err(|e| RenderError::Engine {
            engine: rendered.engine,
            message: e.to_string(),
        })?;
        let cards = cards.is_some();
        Ok(Drawn {
            svg: rendered.svg.into(),
            tree: Arc::new(tree),
            engine: rendered.engine,
            elapsed: rendered.elapsed,
            map: Arc::new(rendered.map),
            links: Arc::new(rendered.links),
            cards,
            bare,
        })
    }
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
    /// The zoom into a link has arrived.
    Follow,
    /// The diagram a link leads to is still not drawn: the zoom into the
    /// link starts anyway.
    Dive,
    /// The window has its new size: the gallery's pictures are drawn for it.
    Thumbs,
    /// Files of a project changed and have settled: its index is brought
    /// up to date.
    Project,
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
    /// A diagram, a Markdown document, a project file or a folder in a
    /// project's folders came, changed or went ([`watch_projects`]).
    ProjectChanged,
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
    /// A handle whose requests go nowhere, so that a test sees what a
    /// window asks for.
    #[cfg(test)]
    pub fn detached() -> Worker {
        let (input, _) = mpsc::unbounded();
        let (timer, _) = std_mpsc::channel();
        Worker { input, timer }
    }

    /// Asks for `source` to be drawn; `place` is where the diagram's file
    /// is, so its `!include`s can be read. With `linked`, the diagram links
    /// to the project's diagrams in the atlas, from its own path there.
    pub fn render(
        &self,
        ticket: Ticket,
        source: Arc<str>,
        language: Language,
        place: Option<Place>,
        linked: Option<Linked>,
    ) {
        self.render_folded(ticket, source, language, place, linked, Asked::default());
    }

    /// [`Self::render`] with the containers named in `folds` folded to
    /// their box (docs/adr/0024), and a JSON or YAML diagram drawn nested
    /// when `nested` says so. The answer carries `source` as it is.
    pub fn render_folded(
        &self,
        ticket: Ticket,
        source: Arc<str>,
        language: Language,
        place: Option<Place>,
        linked: Option<Linked>,
        asked: Asked,
    ) {
        let _ = self.input.unbounded_send(Input::Render {
            ticket,
            source,
            language,
            place,
            linked,
            document: false,
            folds: asked.folds,
            views: asked.views,
            covered: asked.covered,
            map: None,
        });
    }

    /// Asks for the map of `folder` of the project in `atlas`, made from
    /// its diagrams, in the look of `skin` (docs/adr/0026).
    pub fn render_map(&self, ticket: Ticket, skin: Arc<str>, atlas: Arc<Atlas>, folder: String) {
        let _ = self.input.unbounded_send(Input::Render {
            ticket,
            source: skin,
            language: Language::PlantUml,
            place: None,
            linked: Some((atlas, None)),
            document: false,
            folds: Vec::new(),
            views: Views::default(),
            covered: None,
            map: Some(folder),
        });
    }

    /// Renders every diagram of the Markdown document `source`, one under
    /// another (`Renderer::render_document`).
    pub fn render_document(
        &self,
        ticket: Ticket,
        source: Arc<str>,
        place: Option<Place>,
        views: Views,
    ) {
        let _ = self.input.unbounded_send(Input::Render {
            ticket,
            source,
            language: Language::PlantUml,
            place,
            linked: None,
            document: true,
            folds: Vec::new(),
            views,
            covered: None,
            map: None,
        });
    }

    /// Sends [`Event::Due`] for `alarm` after `delay`. Setting it again
    /// before then starts the wait over.
    pub fn after(&self, alarm: Alarm, delay: Duration) {
        let _ = self.timer.send((alarm, Instant::now() + delay));
    }
}

/// The project a diagram links into, and the diagram's path there.
pub type Linked = (Arc<Atlas>, Option<String>);

#[derive(Debug)]
enum Input {
    Render {
        ticket: Ticket,
        source: Arc<str>,
        language: Language,
        place: Option<Place>,
        linked: Option<Linked>,
        document: bool,
        /// Containers folded to their box.
        folds: Vec<String>,
        /// How the pane asks the diagram drawn.
        views: Views,
        /// Traces whose coverage is drawn over a state machine.
        covered: Option<Arc<Covered>>,
        /// The folder of the project in `linked` whose map is asked for.
        map: Option<String>,
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
            let result = outcome.result.and_then(Drawn::of);
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
                    linked,
                    document,
                    folds,
                    views,
                    covered,
                    map,
                } => {
                    let generation = if let (Some(folder), Some((atlas, _))) = (map, &linked) {
                        service.submit_map(
                            ticket.doc,
                            source.to_string(),
                            Arc::clone(atlas),
                            folder,
                        )
                    } else if document {
                        service.submit_document(ticket.doc, source.to_string(), place, views.nested)
                    } else {
                        // The coverage is worked out on the render thread.
                        let after = covered.map(|covered| {
                            let source = Arc::clone(&source);
                            Box::new(move || covered.lines(&source, language))
                                as omaspec_render::After
                        });
                        service.submit_after(
                            ticket.doc,
                            folded(&source, language, &folds, views),
                            language,
                            place,
                            linked,
                            after,
                        )
                    };
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

/// What the pane asks of a drawing besides the text: the containers it
/// folds, its views, the traces whose coverage it shows.
#[derive(Debug, Clone, Default)]
pub struct Asked {
    pub folds: Vec<String>,
    pub views: Views,
    pub covered: Option<Arc<Covered>>,
}

/// Recorded traces whose coverage is drawn over a state machine: what
/// they take bold in green with how often, what they never take dashed
/// in red (`omaspec_model::coverage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Covered {
    /// The file name of the machine, which names it for the traces'
    /// objects when it has no title.
    pub name: String,
    pub traces: Vec<String>,
}

impl Covered {
    /// The lines that mark the coverage over `source`, the text drawn,
    /// after it (`omaspec_uml::state::Marks::lines`); none when it is no
    /// state machine.
    pub fn lines(&self, source: &str, language: Language) -> String {
        let Some(spec) = omaspec_model::Spec::read(source, language, &self.name) else {
            return String::new();
        };
        let traces: Vec<&str> = self.traces.iter().map(String::as_str).collect();
        let cov = omaspec_model::coverage(&spec, &traces);
        let mut lines = cov.marks(&spec).lines(language == Language::Mermaid);
        if !source.ends_with('\n') && !lines.is_empty() {
            lines.insert(0, '\n');
        }
        lines
    }
}

/// How the pane asks a diagram drawn, besides the file (View menu): the
/// lines it adds after the diagram, which every other reader takes for
/// comments.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Views {
    /// A JSON or a YAML diagram nested, not as a tree.
    pub nested: bool,
    /// A state machine as its table of states and events.
    pub table: bool,
}

/// `source` with a line for each container `folds` names after it, which
/// the drawers fold (`omaspec_uml::fold`), and one for each of `views` that
/// the diagram has; the file never holds them.
pub fn folded(source: &str, language: Language, folds: &[String], views: Views) -> String {
    let mut text = source.to_owned();
    // Only a JSON or a YAML diagram has a nested drawing, only a state
    // machine a table.
    let nested = views.nested && holds_tables(source, language);
    let table = views.table && holds_machine(source, language);
    if folds.is_empty() && !nested && !table {
        return text;
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    for name in folds {
        text.push_str(&omaspec_uml::fold::line(
            name,
            language == Language::Mermaid,
        ));
    }
    if nested {
        text.push_str(&omaspec_uml::fold::nested_line(
            language == Language::Mermaid,
        ));
    }
    if table {
        text.push_str(&omaspec_uml::fold::table_line(
            language == Language::Mermaid,
        ));
    }
    text
}

/// Whether `source` is a state machine, the kind drawn as a table too.
pub fn holds_machine(source: &str, language: Language) -> bool {
    omaspec_core::kind::classify(source, language).kind == omaspec_core::Kind::State
}

/// Whether `source` is a JSON or a YAML diagram, the kinds drawn as a tree
/// or nested.
pub fn holds_tables(source: &str, language: Language) -> bool {
    language == Language::PlantUml
        && matches!(
            omaspec_core::kind::plantuml_block(source).as_str(),
            "json" | "yaml"
        )
}

/// One thread for all alarms; it ends with the service.
fn timer(input: UnboundedSender<Input>) -> std_mpsc::Sender<(Alarm, Instant)> {
    let (set, alarms) = std_mpsc::channel::<(Alarm, Instant)>();
    std::thread::Builder::new()
        .name("omaspec-timer".into())
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
        .name("omaspec-worker".into())
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

/// Reports [`Event::ProjectChanged`] for a change in any of `dirs`, the
/// folders of the projects open ([`crate::index::Index::dirs`]): each
/// watched on its own, not their whole trees, which may hold build output
/// and dependencies. Changes that come together are reported once.
pub fn watch_projects(dirs: Arc<Vec<PathBuf>>) -> Subscription<Event> {
    Subscription::run_with(dirs, |dirs: &Arc<Vec<PathBuf>>| {
        watch_projects_stream(Arc::clone(dirs))
    })
}

fn watch_projects_stream(dirs: Arc<Vec<PathBuf>>) -> impl Stream<Item = Event> + use<> {
    iced::stream::channel(16, async move |mut output| {
        let (report, mut events) = mpsc::unbounded::<Event>();
        let failed = report.clone();
        let watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
                Ok(e) if touches_project(&e) => {
                    let _ = report.unbounded_send(Event::ProjectChanged);
                }
                Ok(_) => {}
                Err(err) => {
                    let _ = report.unbounded_send(Event::WatchFailed(err.to_string()));
                }
            });
        // Dropping the watcher stops it, so it lives as long as the loop.
        let _watcher = match watcher {
            Ok(mut watcher) => {
                for dir in dirs.iter() {
                    // A folder gone since the scan: the next one sees it.
                    let _ = watcher.watch(dir, RecursiveMode::NonRecursive);
                }
                Some(watcher)
            }
            Err(err) => {
                let _ = failed.unbounded_send(Event::WatchFailed(err.to_string()));
                None
            }
        };
        let mut held = None;
        loop {
            let event = match held.take() {
                Some(event) => event,
                None => match events.next().await {
                    Some(event) => event,
                    None => break,
                },
            };
            // A checkout changes many files at once: one report for them.
            if matches!(event, Event::ProjectChanged) {
                while let Ok(next) = events.try_recv() {
                    if !matches!(next, Event::ProjectChanged) {
                        held = Some(next);
                        break;
                    }
                }
            }
            if output.send(event).await.is_err() {
                break;
            }
        }
    })
}

/// Whether `event` may change a project's index: a diagram, a Markdown
/// document or a project file written, made or removed, or a folder made,
/// removed or renamed. Reading the files, as the index does, is none.
fn touches_project(event: &notify::Event) -> bool {
    let named = || {
        event.paths.iter().any(|path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|name| {
                    Language::from_file_name(name).is_some()
                        || omaspec_core::markdown::is_markdown(name)
                        || name == omaspec_core::project::FILE
                })
        })
    };
    match event.kind {
        EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder) => true,
        EventKind::Modify(ModifyKind::Name(_)) => {
            named() || event.paths.iter().any(|p| p.extension().is_none())
        }
        EventKind::Remove(_) => named(),
        kind => is_change(&kind) && named(),
    }
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

/// Whether an event means the content changed. Reading a file (Omaspec
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
        let dir = std::env::temp_dir().join(format!("omaspec-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn reports_changes_in_a_project_s_folders() {
        let root = temp_dir("watch-project");
        let rooms = root.join("rooms");
        std::fs::create_dir_all(&rooms).unwrap();
        let door = rooms.join("door.puml");
        std::fs::write(&door, "@startuml\n@enduml\n").unwrap();
        let events = Events::start(watch_projects_stream(Arc::new(vec![
            root.clone(),
            rooms.clone(),
        ])));
        // The folders are watched once the stream runs.
        std::thread::sleep(Duration::from_millis(200));

        // Reading a diagram, as the index does, or writing a file that is
        // none, is no change.
        let _ = std::fs::read_to_string(&door).unwrap();
        std::fs::write(root.join("notes.txt"), "x").unwrap();
        assert!(events.settle(Duration::from_millis(300)).is_empty());

        // Diagrams written in a folder below.
        for n in 0..20 {
            std::fs::write(rooms.join(format!("unit-{n}.puml")), "@startuml\n@enduml\n").unwrap();
        }
        let came = events.settle(Duration::from_millis(300));
        assert!(!came.is_empty());
        assert!(
            came.iter().all(|e| matches!(e, Event::ProjectChanged)),
            "{came:?}"
        );

        // A diagram removed; a folder made.
        std::fs::remove_file(&door).unwrap();
        assert!(matches!(events.next(), Event::ProjectChanged));
        let _ = events.settle(Duration::from_millis(300));
        std::fs::create_dir(root.join("hall")).unwrap();
        assert!(matches!(events.next(), Event::ProjectChanged));
        let _ = std::fs::remove_dir_all(&root);
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
            None,
        );
        worker.render(
            ticket(2),
            "flowchart LR\n  A --> C\n".into(),
            Language::Mermaid,
            None,
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
        let theme = omaspec_theme::Theme::load_from(&current);
        assert_eq!(theme.name.as_deref(), Some("two"));
        assert_eq!(theme.palette.accent.to_string(), "#222222");
        assert!(events.settle(Duration::from_millis(300)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
