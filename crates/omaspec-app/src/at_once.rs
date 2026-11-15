//! The window's tasks, run here and now. In the window, iced's runtime
//! runs the tasks an update gives in the background and hands their
//! messages in as they come: a picture rasterized on a thread of its own,
//! put on the GPU, then shown. `omaspec play` and the tests run the same
//! tasks to their end at once, so a recording takes its pictures the way
//! the window takes them, only without waiting for the wall clock.

use std::collections::VecDeque;

use iced::Task;
use iced::futures::StreamExt;
use iced::futures::executor::block_on;
use iced_runtime::Action;

/// Runs `task` to its end, and each task `update` gives for a message it
/// gives, here and now. A picture one of them puts on the GPU goes onto
/// `renderer`'s, as the window's runtime puts it on the window's; what
/// only a window does (its size, the clipboard, a widget's focus) is
/// dropped, and whatever waits for an answer to it gets none.
pub fn run<T>(renderer: &iced::Renderer, task: Task<T>, mut update: impl FnMut(T) -> Task<T>) {
    use iced::advanced::image::Renderer as _;
    let mut queue = VecDeque::new();
    queue.extend(iced_runtime::task::into_stream(task));
    while let Some(mut stream) = queue.pop_front() {
        while let Some(action) = block_on(stream.next()) {
            match action {
                Action::Output(message) => {
                    queue.extend(iced_runtime::task::into_stream(update(message)));
                }
                Action::Image(iced_runtime::image::Action::Allocate(handle, answer)) => {
                    let _ = answer.send(renderer.load_image(&handle));
                }
                _ => {}
            }
        }
    }
}

/// A renderer that draws without a GPU, for [`run`] where nothing is
/// drawn on a screen: the tests of a pane. One a thread, made once.
#[cfg(test)]
pub fn with_renderer<R>(f: impl FnOnce(&iced::Renderer) -> R) -> R {
    use iced::advanced::renderer::Headless;
    thread_local! {
        static RENDERER: iced::Renderer = block_on(iced::Renderer::new(
            iced::Font::default(),
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("tiny-skia draws anywhere");
    }
    RENDERER.with(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A task's messages go to the update, and the tasks it gives run too,
    /// in the order they come; a picture put on the GPU is there.
    #[test]
    fn a_task_runs_to_its_end_with_what_it_gives() {
        let picture = iced::widget::image::Handle::from_rgba(2, 2, vec![255; 16]);
        let mut seen = Vec::new();
        with_renderer(|renderer| {
            let first = Task::batch([
                Task::done(1),
                iced_runtime::image::allocate(picture.clone())
                    .map(|allocated| if allocated.is_ok() { 10 } else { 0 }),
            ]);
            run(renderer, first, |n| {
                seen.push(n);
                match n {
                    1 => Task::done(2),
                    _ => Task::none(),
                }
            });
        });
        seen.sort_unstable();
        assert_eq!(seen, [1, 2, 10]);
    }
}
