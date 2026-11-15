//! The window's tasks, run without its runtime. In the window, iced's
//! runtime runs the tasks an update gives in the background and hands
//! their messages in as they come: a picture rasterized on a thread of its
//! own, put on the GPU, then shown. `omaspec play` and the tests run the
//! same tasks: to their end at once ([`run`]), so a recording takes its
//! pictures the way the window takes them only without waiting for the
//! wall clock, or, while frames are drawn on the wall clock, in the
//! background as the window does ([`Background`]).

use std::collections::VecDeque;

use iced::Task;
use iced::futures::StreamExt;
use iced::futures::executor::block_on;
use iced_runtime::Action;

type Stream<T> = iced_runtime::futures::BoxStream<Action<T>>;

/// Runs `task` to its end, and each task `update` gives for a message it
/// gives, here and now. A picture one of them allocates goes onto
/// `renderer`, as the window's runtime puts it on the window's GPU; what
/// only a window does (its size, the clipboard, a widget's focus) is
/// dropped, and whatever waits for an answer to it gets none.
pub fn run<T>(renderer: &iced::Renderer, task: Task<T>, mut update: impl FnMut(T) -> Task<T>) {
    let mut queue = VecDeque::new();
    queue.extend(iced_runtime::task::into_stream(task));
    while let Some(mut stream) = queue.pop_front() {
        while let Some(action) = block_on(stream.next()) {
            queue.extend(take(action, renderer, &mut update));
        }
    }
}

/// Tasks on their way, as the window's runtime keeps them: each look takes
/// what they have given by then, and none is waited for.
pub struct Background<T> {
    streams: Vec<Stream<T>>,
}

impl<T> Default for Background<T> {
    fn default() -> Self {
        Self {
            streams: Vec::new(),
        }
    }
}

impl<T> Background<T> {
    pub fn add(&mut self, task: Task<T>) {
        self.streams.extend(iced_runtime::task::into_stream(task));
    }

    /// Takes what the tasks on their way have given by now, as [`run`]
    /// takes it, the tasks `update` gives joining them; what is still to
    /// come stays on its way. A stream that says it is not ready but wakes
    /// itself (iced's batches do, to be polled again) is polled again, as
    /// an executor would.
    pub fn poll(&mut self, renderer: &iced::Renderer, mut update: impl FnMut(T) -> Task<T>) {
        let woken = std::sync::Arc::new(Woken::default());
        let waker = std::task::Waker::from(std::sync::Arc::clone(&woken));
        let mut context = std::task::Context::from_waker(&waker);
        let (mut index, mut again) = (0, 0);
        while index < self.streams.len() {
            woken.0.store(false, std::sync::atomic::Ordering::SeqCst);
            match self.streams[index].poll_next_unpin(&mut context) {
                std::task::Poll::Ready(Some(action)) => {
                    let more = take(action, renderer, &mut update);
                    self.streams.extend(more);
                }
                std::task::Poll::Ready(None) => {
                    drop(self.streams.swap_remove(index));
                }
                std::task::Poll::Pending
                    if again < AGAIN && woken.0.load(std::sync::atomic::Ordering::SeqCst) =>
                {
                    again += 1;
                }
                std::task::Poll::Pending => {
                    (index, again) = (index + 1, 0);
                }
            }
        }
    }
}

/// How often in a row a stream that wakes itself is polled again in one
/// look: a batch whose tasks all wait wakes itself too, and is left for
/// the next look.
const AGAIN: usize = 4;

/// Whether a stream woke the one polling it.
#[derive(Default)]
struct Woken(std::sync::atomic::AtomicBool);

impl std::task::Wake for Woken {
    fn wake(self: std::sync::Arc<Self>) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// What the runtime does with one `action` of a task: a message to
/// `update`, whose task comes back to be run; a picture allocated onto
/// `renderer`.
fn take<T>(
    action: Action<T>,
    renderer: &iced::Renderer,
    update: &mut impl FnMut(T) -> Task<T>,
) -> Option<Stream<T>> {
    use iced::advanced::image::Renderer as _;
    match action {
        Action::Output(message) => iced_runtime::task::into_stream(update(message)),
        Action::Image(iced_runtime::image::Action::Allocate(handle, answer)) => {
            let _ = answer.send(renderer.load_image(&handle));
            None
        }
        _ => None,
    }
}

/// [`run`] on a renderer that draws without a GPU, for the tests of a pane
/// or an editor where no screen draws: it allocates the pictures as a
/// window's does, and holds them for the test's thread.
#[cfg(test)]
pub fn here<T>(task: Task<T>, update: impl FnMut(T) -> Task<T>) {
    use iced::advanced::renderer::Headless;
    thread_local! {
        static RENDERER: iced::Renderer = block_on(iced::Renderer::new(
            iced::Font::default(),
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("tiny-skia draws anywhere");
    }
    RENDERER.with(|renderer| run(renderer, task, update));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A task's messages go to the update, and the tasks it gives run too,
    /// in the order they come; a picture it allocates is allocated.
    #[test]
    fn a_task_runs_to_its_end_with_what_it_gives() {
        let picture = iced::widget::image::Handle::from_rgba(2, 2, vec![255; 16]);
        let mut seen = Vec::new();
        let first = Task::batch([
            Task::done(1),
            iced_runtime::image::allocate(picture.clone())
                .map(|allocated| if allocated.is_ok() { 10 } else { 0 }),
        ]);
        here(first, |n| {
            seen.push(n);
            match n {
                1 => Task::done(2),
                _ => Task::none(),
            }
        });
        seen.sort_unstable();
        assert_eq!(seen, [1, 2, 10]);
    }

    /// In the background, what is ready is taken at each look, and what
    /// is still on its way is not waited for.
    #[test]
    fn the_background_takes_what_is_ready_and_waits_for_nothing() {
        let (tx, rx) = iced::futures::channel::oneshot::channel::<u32>();
        let mut background = Background::default();
        background.add(Task::batch([
            Task::done(1),
            Task::perform(async move { rx.await.unwrap_or(0) }, |n| n),
        ]));
        let mut seen = Vec::new();
        let renderer = block_on(<iced::Renderer as iced::advanced::renderer::Headless>::new(
            iced::Font::default(),
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let look = |background: &mut Background<u32>, seen: &mut Vec<u32>| {
            background.poll(&renderer, |n| {
                seen.push(n);
                if n == 1 { Task::done(2) } else { Task::none() }
            });
        };
        look(&mut background, &mut seen);
        assert_eq!(seen, [1, 2], "the ready ones, and what they give");
        tx.send(7).unwrap();
        look(&mut background, &mut seen);
        assert_eq!(seen, [1, 2, 7]);
        assert!(background.streams.is_empty());
    }
}
