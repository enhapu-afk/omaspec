//! `omaspec preview FILE`: a window that shows a diagram and follows the
//! file.
//!
//! Made for editing in Neovim (or any editor) with the preview tiled next to
//! it: every save renders again, an error shows in the status bar while the
//! last good picture stays, and the window follows Omarchy theme switches.
//!
//! Keys: `+`/`-` zoom, `0` 100 %, `f` fit, `h j k l` or arrows pan,
//! `c` copy PNG, `C` copy SVG, `r` render again, `q` quit.

use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{column, container, row, text};
use iced::{Element, Font, Length, Subscription, Task, Theme, window};
use omaspec_core::{Language, detect};
use omaspec_render::Place;
use omaspec_render::export::Format;
use omaspec_theme::{Palette, Rgb, Theme as Omarchy};

use crate::paper::{self, Ask, Paper, Status};
use crate::ui;
use crate::worker::{self, Alarm, Worker};

/// Editors write a file in several steps; read it once they are done.
const FILE_SETTLE: Duration = Duration::from_millis(50);
/// A theme switch replaces several files.
const THEME_SETTLE: Duration = Duration::from_millis(200);
const KEY_PAN: f32 = 80.0;
const KEY_ZOOM: f32 = 1.25;

pub fn run(path: PathBuf, forced: Option<Language>) -> iced::Result {
    iced::application(
        move || Preview::new(path.clone(), forced),
        Preview::update,
        Preview::view,
    )
    .title(Preview::title)
    .theme(Preview::theme)
    .subscription(Preview::subscription)
    .default_font(ui::mono_font())
    .window(window::Settings {
        platform_specific: window::settings::PlatformSpecific {
            application_id: "omaspec-preview".into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .run()
}

#[derive(Debug, Clone)]
pub enum Message {
    Worker(worker::Event),
    Paper(paper::Message),
    ScaleFactor(f32),
    Key(keyboard::Event),
    Copied(Result<&'static str, String>),
}

struct Preview {
    path: PathBuf,
    name: String,
    forced: Option<Language>,
    worker: Option<Worker>,

    theme_name: String,
    palette: Palette,
    theme: Theme,
    font: Font,

    paper: Paper,
    notice: Option<String>,
}

impl Preview {
    fn new(path: PathBuf, forced: Option<Language>) -> (Self, Task<Message>) {
        let omarchy = Omarchy::load_current();
        let palette = omarchy.palette.legible();
        let theme_name = omarchy.name.unwrap_or_else(|| "built-in".into());
        let preview = Self {
            name: path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            path,
            forced,
            worker: None,
            theme: ui::iced_theme(&theme_name, &palette),
            theme_name,
            palette,
            font: ui::mono_font(),
            paper: Paper::default(),
            notice: None,
        };
        let scale = window::latest()
            .and_then(window::scale_factor)
            .map(Message::ScaleFactor);
        (preview, scale)
    }

    fn title(&self) -> String {
        format!("Omaspec · {}", self.name)
    }

    fn theme(&self) -> Theme {
        self.theme.clone()
    }

    fn subscription(&self) -> Subscription<Message> {
        let theme_dir = omaspec_theme::current_dir().filter(|d| d.is_dir());
        Subscription::batch([
            worker::service(worker::Watched {
                theme_dir,
                font_dir: ui::fontconfig_dir(),
            })
            .map(Message::Worker),
            worker::watch(vec![self.path.clone()]).map(Message::Worker),
            keyboard::listen().map(Message::Key),
            window::events().filter_map(|(_, event)| match event {
                window::Event::Rescaled(factor) => Some(Message::ScaleFactor(factor)),
                _ => None,
            }),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Worker(event) => self.worker_event(event),
            Message::Paper(paper::Message::Pick(at)) => {
                // No editor here to jump in; say where it is.
                self.notice = Some(match self.paper.label_at(at) {
                    Some((label, source)) => {
                        match omaspec_core::locate::line_of(&source, &label.text, label.nth) {
                            Some(line) => format!("line {}: {}", line + 1, label.text),
                            None => format!("“{}” is not in the source", label.text),
                        }
                    }
                    None => "no label there".into(),
                });
                Task::none()
            }
            Message::Paper(message) => self.paper.update(message).map(Message::Paper),
            Message::ScaleFactor(factor) => self.paper.set_scale_factor(factor).map(Message::Paper),
            Message::Key(event) => self.key(event),
            Message::Copied(result) => {
                self.notice = Some(match result {
                    Ok(what) => {
                        ui::notify(
                            "Diagram copied",
                            &format!("as {what}, with its source inside"),
                        );
                        format!("copied {what}")
                    }
                    Err(err) => err,
                });
                Task::none()
            }
        }
    }

    fn worker_event(&mut self, event: worker::Event) -> Task<Message> {
        match event {
            worker::Event::Connected(worker) => {
                self.worker = Some(worker);
                self.request_render();
            }
            worker::Event::FileChanged(_) => self.after(Alarm::File, FILE_SETTLE),
            worker::Event::ThemeChanged => self.after(Alarm::Theme, THEME_SETTLE),
            worker::Event::FontChanged => self.after(Alarm::Font, THEME_SETTLE),
            worker::Event::Due(Alarm::Font) => self.font = ui::current_mono_font(),
            worker::Event::Due(Alarm::File) => self.request_render(),
            worker::Event::Due(Alarm::Theme) => {
                let omarchy = Omarchy::load_current();
                let palette = omarchy.palette.legible();
                self.theme_name = omarchy.name.unwrap_or_else(|| "built-in".into());
                self.theme = ui::iced_theme(&self.theme_name, &palette);
                self.palette = palette;
            }
            worker::Event::Due(_) => {}
            worker::Event::WatchFailed(err) => {
                self.notice = Some(format!("not following the file: {err}"));
            }
            worker::Event::Rendered(answer) => {
                self.notice = None;
                return self.paper.rendered(*answer).map(Message::Paper);
            }
        }
        Task::none()
    }

    fn after(&self, alarm: Alarm, delay: Duration) {
        if let Some(worker) = &self.worker {
            worker.after(alarm, delay);
        }
    }

    /// Reads the file and asks the worker to render it.
    fn request_render(&mut self) {
        let Some(worker) = &self.worker else { return };
        match std::fs::read_to_string(&self.path) {
            Ok(source) => {
                let hint = Language::from_file_name(&self.name);
                let language = self.forced.unwrap_or_else(|| detect(&source, hint));
                let place = Place::of(&self.path, None);
                self.paper
                    .request(worker, 0, source.into(), language, place);
            }
            // Mid-save the file can be briefly missing; the next event retries.
            Err(err) => self.paper.status = Status::Unreadable(err.to_string()),
        }
    }

    fn key(&mut self, event: keyboard::Event) -> Task<Message> {
        let keyboard::Event::KeyPressed { key, modifiers, .. } = event else {
            return Task::none();
        };
        if modifiers.control() || modifiers.alt() || modifiers.logo() {
            return Task::none();
        }
        let paper = &mut self.paper;
        let playing = paper.playing();
        let step = |ask| paper::Message::Play(ask);
        let task = match key.as_ref() {
            Key::Named(Named::F5) | Key::Character("p") => {
                if playing {
                    paper.update(paper::Message::StopPlay)
                } else {
                    self.notice = Some(match paper.start_play() {
                        Ok(()) => {
                            "playing: 1–9 a step, Space next, Backspace back, Esc stops".into()
                        }
                        Err(why) => why,
                    });
                    Task::none()
                }
            }
            Key::Character(c) if playing && c.len() == 1 && ("1"..="9").contains(&c) => {
                let n = c.parse::<usize>().unwrap_or(1);
                paper.update(step(Ask::Take(n - 1)))
            }
            Key::Named(Named::Space | Named::Enter) if playing => paper.update(step(Ask::Next)),
            Key::Named(Named::Backspace) if playing => paper.update(step(Ask::Back)),
            Key::Named(Named::Home) if playing => paper.update(step(Ask::Restart)),
            Key::Named(Named::Escape) if playing => paper.update(paper::Message::StopPlay),
            Key::Character("+" | "=") => paper.zoom(KEY_ZOOM),
            Key::Character("-") => paper.zoom(1.0 / KEY_ZOOM),
            Key::Character("0") => paper.actual_size(),
            Key::Character("f") => paper.fit(),
            Key::Character("h") | Key::Named(Named::ArrowLeft) => paper.pan(KEY_PAN, 0.0),
            Key::Character("l") | Key::Named(Named::ArrowRight) => paper.pan(-KEY_PAN, 0.0),
            Key::Character("k") | Key::Named(Named::ArrowUp) => paper.pan(0.0, KEY_PAN),
            Key::Character("j") | Key::Named(Named::ArrowDown) => paper.pan(0.0, -KEY_PAN),
            Key::Character("c") => {
                return self
                    .paper
                    .copy(Format::Png, &self.name)
                    .map(Message::Copied);
            }
            Key::Character("C") => {
                return self
                    .paper
                    .copy(Format::Svg, &self.name)
                    .map(Message::Copied);
            }
            Key::Character("r") => {
                self.request_render();
                return Task::none();
            }
            Key::Character("q") => return iced::exit(),
            _ => return Task::none(),
        };
        task.map(Message::Paper)
    }

    fn view(&self) -> Element<'_, Message> {
        column![
            self.paper
                .view(&self.palette, self.font, false)
                .map(Message::Paper),
            self.status_bar()
        ]
        .into()
    }

    /// One line: file and language, the state (or an error, cut to fit),
    /// the zoom.
    fn status_bar(&self) -> Element<'_, Message> {
        let p = &self.palette;
        let label = |s: String, c: Rgb| {
            text(s)
                .font(self.font)
                .size(13)
                .color(ui::color(c))
                .wrapping(text::Wrapping::None)
        };
        let (state, color) = self.paper.state(p);
        let middle = match (&self.notice, &self.paper.status) {
            (Some(notice), _) => row![
                label(state, color),
                label(format!("  {notice}"), p.light_foreground)
            ],
            (None, Status::InSync) => row![
                label(state, color),
                label(
                    "   +/- zoom · f fit · c copy · q quit".into(),
                    p.dark_foreground
                )
            ],
            (None, _) => row![label(state, color)],
        };
        let language = self
            .paper
            .diagram
            .as_ref()
            .map_or("", |d| d.language.name());
        let bar = row![
            label("󰈸".into(), p.accent),
            label(self.name.clone(), p.foreground),
            label(language.into(), p.dark_foreground),
            container(middle).width(Length::Fill).clip(true),
            label(
                format!("{:.0}%", self.paper.zoom_percent()),
                p.dark_foreground
            ),
        ]
        .spacing(10);
        ui::bar(bar.into(), p)
    }
}

/// Checks that `path` names a file the preview can follow.
pub fn check(path: &Path) -> anyhow::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    anyhow::ensure!(path.is_file(), "{}: no such file", path.display());
    Ok(path)
}

#[cfg(test)]
mod tests {
    use iced::Size;
    use iced::keyboard::key::{NativeCode, Physical};
    use iced::keyboard::{Location, Modifiers};
    use omaspec_render::{Engine, RenderError};

    use super::*;
    use crate::paper::testing::{answer, drawn, tall};
    use crate::worker::Drawn;

    const FLOW: &str = "flowchart LR\n  A[Edit] --> B{Saved?}\n  B -->|yes| C[Render]\n";

    fn rendered(id: u64, result: Result<Drawn, RenderError>) -> Message {
        Message::Worker(worker::Event::Rendered(Box::new(answer(0, id, result))))
    }

    /// A preview with `drawn` on screen, on a screen with scale factor 2
    /// like the headless snapshot.
    fn showing(drawn: Drawn) -> Preview {
        let (mut p, _) = Preview::new(PathBuf::from("/tmp/flow.mmd"), None);
        let _ = p.update(Message::ScaleFactor(2.0));
        let _ = p.update(Message::Paper(paper::Message::Viewport(Size::new(
            900.0, 520.0,
        ))));
        p.paper.pretend_requested(1);
        let _ = p.update(rendered(1, Ok(drawn)));
        p.paper.finish_rasters();
        p
    }

    fn showing_flow() -> Preview {
        showing(drawn(FLOW, Language::Mermaid))
    }

    fn press(c: &str) -> Message {
        let key = Key::Character(c.into());
        Message::Key(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Unidentified(NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::empty(),
            text: Some(c.into()),
            repeat: false,
        })
    }

    #[test]
    fn shows_the_diagram_fitted_and_in_sync() {
        let p = showing_flow();
        assert!(matches!(p.paper.status, Status::InSync));
        assert!(p.paper.view.fit && p.paper.view.zoom <= 1.0);
        assert!(p.paper.overview_shown() && p.paper.detail().0.is_none());
        assert_eq!(
            p.paper.diagram.as_ref().unwrap().drawn.engine,
            Engine::NativeMermaid
        );
    }

    #[test]
    fn large_diagrams_stay_sharp_when_zoomed_in() {
        let mut p = showing(tall());
        assert!(p.paper.overview_shown() && p.paper.detail().0.is_none());

        let _ = p.update(press("0"));
        let detail = p.paper.detail().0.expect("a detail at 100 %");
        assert_eq!(detail.scale, 2.0);
        let (region, seen) = (detail.region(), p.paper.view.visible(p.paper.viewport()));
        assert!(region.x <= seen.x && region.y <= seen.y);
        assert!(region.x + region.width >= seen.x + seen.width);
        assert!(region.y + region.height >= seen.y + seen.height);
        p.paper.finish_rasters();
        assert!(p.paper.detail().1);

        // A short pan needs nothing new, fitting needs no detail.
        let _ = p.update(press("j"));
        assert_eq!(p.paper.detail().0, Some(detail));
        let _ = p.update(press("f"));
        assert_eq!(p.paper.detail(), (None, false));
    }

    #[test]
    fn late_rasters_are_dropped() {
        let mut p = showing(tall());
        let _ = p.update(press("0"));
        let early = p.paper.early_detail();
        let _ = p.update(press("+"));
        let _ = p.update(Message::Paper(paper::Message::Detail(1, early)));
        assert!(!p.paper.detail().1);
    }

    #[test]
    fn keeps_the_last_diagram_when_the_next_fails() {
        let mut p = showing_flow();
        p.paper.pretend_requested(2);
        let err = RenderError::Diagram {
            engine: Engine::Mermaid,
            line: Some(2),
            message: "unexpected token".into(),
        };
        let _ = p.update(rendered(2, Err(err)));
        assert!(matches!(p.paper.status, Status::Failed(_)));
        assert!(p.paper.diagram.is_some() && p.paper.overview_shown());
    }

    #[test]
    fn ignores_answers_older_than_the_shown_one() {
        let mut p = showing_flow();
        p.paper.pretend_requested(3);
        let _ = p.update(rendered(
            3,
            Ok(drawn("pie\n  \"a\": 1\n", Language::Mermaid)),
        ));
        let pie_size = p.paper.diagram.as_ref().unwrap().size;
        let _ = p.update(rendered(2, Ok(drawn(FLOW, Language::Mermaid))));
        assert_eq!(p.paper.diagram.as_ref().unwrap().size, pie_size);
    }

    #[test]
    fn p_plays_a_state_machine() {
        let source = "stateDiagram-v2\n[*] --> Idle\nIdle --> Busy : go\nBusy --> Idle : done\n";
        let (mut p, _) = Preview::new(PathBuf::from("/tmp/machine.mmd"), None);
        let _ = p.update(Message::Paper(paper::Message::Viewport(Size::new(
            900.0, 520.0,
        ))));
        p.paper.pretend_requested(1);
        let mut reply = answer(0, 1, Ok(drawn(source, Language::Mermaid)));
        reply.source = source.into();
        let _ = p.update(Message::Worker(worker::Event::Rendered(Box::new(reply))));
        p.paper.finish_rasters();
        let _ = p.update(press("p"));
        assert!(p.paper.playing(), "{:?}", p.notice);
        let _ = p.update(press("1"));
        let later = std::time::Instant::now() + std::time::Duration::from_secs(10);
        assert_eq!(p.paper.take_jump(later), Some(2));
        let _ = p.update(press("p"));
        assert!(!p.paper.playing());
    }

    #[test]
    fn keys_zoom_pan_and_fit() {
        let mut p = showing_flow();
        let fitted = p.paper.view;
        let _ = p.update(press("+"));
        assert!(p.paper.view.zoom > fitted.zoom && !p.paper.view.fit);
        let _ = p.update(press("l"));
        assert!(p.paper.view.center.x > fitted.center.x);
        let _ = p.update(press("0"));
        assert_eq!(p.paper.view.zoom, 1.0);
        let _ = p.update(press("f"));
        assert_eq!(p.paper.view, fitted);
    }

    /// Writes headless screenshots when OMASPEC_SNAPSHOT_DIR is set:
    /// `preview` fitted and `preview-zoomed`, each with the renderer's name
    /// added (`preview-wgpu.png`).
    #[test]
    fn snapshot() {
        let Some(dir) = std::env::var_os("OMASPEC_SNAPSHOT_DIR") else {
            return;
        };
        let dir = PathBuf::from(dir);
        let mut p = showing_flow();
        shoot(&p, &dir, "preview");
        for _ in 0..8 {
            let _ = p.update(press("+"));
        }
        let _ = p.update(press("j"));
        p.paper.finish_rasters();
        shoot(&p, &dir, "preview-zoomed");
    }

    fn shoot(p: &Preview, dir: &Path, name: &str) {
        let settings = iced::Settings {
            default_font: p.font,
            ..Default::default()
        };
        let mut ui =
            iced_test::simulator::Simulator::with_size(settings, Size::new(900.0, 545.0), p.view());
        ui::snapshot(&mut ui, &p.theme, dir, name);
    }
}
