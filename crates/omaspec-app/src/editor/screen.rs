//! The editor drawn headless, at any scale: the window as a whole, or
//! the diagram's pane alone, redrawn as a window redraws, its pixels read
//! back. The snapshot tests take their pictures here, and `omaspec play
//! FILE -o play.gif` its frames.

use std::time::{Duration, Instant};

use iced::Size;

use super::{App, Message};

/// Which of the editor's views is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shows {
    /// The window: tabs, source, diagram, status bar.
    Window,
    /// The diagram's pane alone, as its own window shows it.
    Diagram,
}

/// The editor drawn headless at any scale; iced_test's simulator draws
/// at 2 only.
pub struct Screen {
    pub renderer: iced::Renderer,
    pub scale: f32,
    pub size: Size,
    pub shows: Shows,
    warm: bool,
    /// The widgets' state between the redraws of [`Self::run`], as a
    /// window keeps it.
    cache: Option<iced_runtime::user_interface::Cache>,
    /// How long the view and its layout took, and the pixels, in each
    /// redraw of [`Self::run`].
    spent: Vec<(Duration, Duration)>,
}

/// A screenshot, as RGBA rows.
pub struct Shot {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Screen {
    pub const SIZE: Size = Size::new(1200.0, 640.0);

    /// A screen for `app`, or why there is none: a GPU the renderer can
    /// draw on without a window.
    pub fn new(app: &App, scale: f32) -> Result<Screen, String> {
        let backend = std::env::var("ICED_TEST_BACKEND").ok();
        Self::drawn_by(app, scale, backend.as_deref())
    }

    /// A screen drawn by the renderer `backend` names (`wgpu`, `tiny-skia`:
    /// the one without a GPU), or the first that can.
    pub fn drawn_by(app: &App, scale: f32, backend: Option<&str>) -> Result<Screen, String> {
        use iced::advanced::graphics::text::font_system;
        use iced::advanced::renderer::Headless;
        for font in scrive_iced::required_fonts() {
            font_system()
                .write()
                .map_err(|_| "the fonts are locked".to_owned())?
                .load_font(std::borrow::Cow::Borrowed(*font));
        }
        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            app.font,
            iced::Pixels(16.0),
            backend,
        ))
        .ok_or_else(|| "no GPU to draw on without a window".to_owned())?;
        Ok(Screen {
            renderer,
            scale,
            size: Self::SIZE,
            shows: Shows::Window,
            warm: false,
            cache: None,
            spent: Vec::new(),
        })
    }

    pub fn shoot(&mut self, app: &App) -> Shot {
        // The first frame starts uploading the pictures in the background.
        if !self.warm {
            let _ = self.frame(app);
            std::thread::sleep(Duration::from_millis(300));
            self.warm = true;
        }
        self.frame(app)
    }

    pub fn frame(&mut self, app: &App) -> Shot {
        self.cache = None;
        self.redraw(app).0
    }

    /// A redraw, as a window's: the layers update and draw, and the GPU
    /// makes the pixels; with what the layers published.
    pub fn redraw(&mut self, app: &App) -> (Shot, Vec<Message>) {
        use iced::advanced::renderer::{Headless, Style};
        use iced::theme::Base;
        let start = Instant::now();
        let view = match self.shows {
            Shows::Window => app.main_view(),
            Shows::Diagram => app.diagram_view(),
        };
        let mut ui = iced_runtime::UserInterface::build(
            view,
            self.size,
            self.cache.take().unwrap_or_default(),
            &mut self.renderer,
        );
        let built = start.elapsed();
        let redraw = iced::Event::Window(iced::window::Event::RedrawRequested(
            iced::time::Instant::now(),
        ));
        let cursor = iced::mouse::Cursor::Unavailable;
        let mut published = Vec::new();
        let _ = ui.update(
            &[redraw],
            cursor,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut published,
        );
        let base = app.theme.base();
        let style = Style {
            text_color: base.text_color,
        };
        ui.draw(&mut self.renderer, &app.theme, &style, cursor);
        let (width, height) = (
            (self.size.width * self.scale).round() as u32,
            (self.size.height * self.scale).round() as u32,
        );
        // The interface lives on until the screenshot: the renderer holds
        // its text only weakly.
        let drawn = Instant::now();
        let rgba = self.renderer.screenshot(
            iced::Size::new(width, height),
            self.scale,
            base.background_color,
        );
        self.spent.push((built, drawn.elapsed()));
        self.cache = Some(ui.into_cache());
        let shot = Shot {
            width: width as usize,
            height: height as usize,
            rgba,
        };
        (shot, published)
    }

    /// Redraws at 60 Hz, as a window does, until `done` or `most` frames;
    /// what the layers publish goes to the app. `paper::frames` logs it.
    #[cfg(test)]
    pub fn run(&mut self, app: &mut App, most: usize, done: impl Fn(&App) -> bool) {
        self.run_frames(app, most, Duration::from_micros(16_667), done, |_| {});
    }

    /// Redraws every `tick` until `done` or `most` frames, as [`Self::run`]
    /// does, and hands each frame's pixels to `each`.
    pub fn run_frames(
        &mut self,
        app: &mut App,
        most: usize,
        tick: Duration,
        done: impl Fn(&App) -> bool,
        mut each: impl FnMut(&Shot),
    ) {
        self.spent.clear();
        for _ in 0..most {
            if done(app) {
                break;
            }
            let start = Instant::now();
            let (shot, published) = self.redraw(app);
            each(&shot);
            for message in published {
                let _ = app.update(message);
            }
            if let Some(wait) = (start + tick).checked_duration_since(Instant::now()) {
                std::thread::sleep(wait);
            }
        }
        if !crate::paper::frames::on() && !cfg!(test) {
            return;
        }
        let median = |mut d: Vec<Duration>| {
            d.sort_unstable();
            d.get(d.len() / 2).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
        };
        eprintln!(
            "frames: headless, {} redraws: the view and its layout {:.1} ms, the pixels made and read back {:.1} ms (medians)",
            self.spent.len(),
            median(self.spent.iter().map(|s| s.0).collect()),
            median(self.spent.iter().map(|s| s.1).collect()),
        );
    }
}

#[cfg(test)]
impl Shot {
    pub fn red(&self, (x, y): (usize, usize)) -> u8 {
        self.rgba[(y * self.width + x) * 4]
    }

    pub fn green(&self, (x, y): (usize, usize)) -> u8 {
        self.rgba[(y * self.width + x) * 4 + 1]
    }

    /// Whether the pixel is Tokyo Night's accent, as the crisp line round a
    /// lit state is.
    pub fn accent(&self, (x, y): (usize, usize)) -> bool {
        self.red((x, y)) <= 0x90 && self.rgba[(y * self.width + x) * 4 + 2] >= 0xE8
    }

    /// The first pixel from `from`, in steps of `step`, redder than `above`.
    pub fn first(&self, from: (usize, usize), step: (usize, usize), above: u8) -> (usize, usize) {
        let mut at = from;
        while self.red(at) <= above {
            at = (at.0 + step.0, at.1 + step.1);
            assert!(
                at.0 < self.width && at.1 < self.height,
                "nothing from {from:?}"
            );
        }
        at
    }

    /// The red of the pixels across an edge found by [`Self::first`]: ten
    /// before it, three after.
    pub fn across(&self, edge: (usize, usize), step: (usize, usize)) -> Vec<u8> {
        (0..13)
            .map(|i| {
                self.red((
                    edge.0 + i * step.0 - 10 * step.0,
                    edge.1 + i * step.1 - 10 * step.1,
                ))
            })
            .collect()
    }

    pub fn save(&self, path: &std::path::Path) {
        use omaspec_render::export::tiny_skia::{IntSize, Pixmap};
        let size = IntSize::from_wh(self.width as u32, self.height as u32).unwrap();
        let png = Pixmap::from_vec(self.rgba.clone(), size)
            .unwrap()
            .encode_png()
            .unwrap();
        std::fs::write(path, png).unwrap();
    }
}
