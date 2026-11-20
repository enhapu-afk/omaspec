//! Presenting a diagram (Shift+F5): the diagram full screen in a window
//! of its own, played as the pane plays it, with the keys of a
//! presentation page (docs/adr/0031) and of a presenter's remote; the
//! editor's window shows the speaker's notes: the comments written just
//! above the line the play is at, the time since the start, and the
//! keys. On two screens the diagram goes where the room looks, the notes
//! stay with the speaker. Escape ends it as it began.

use std::time::Duration;

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::time::Instant;
use iced::widget::{Space, column, container, row, text};
use iced::{Element, Length, Task, window};

use super::{App, Command, Message};
use crate::paper;
use crate::ui;

/// How often the speaker's clock goes on.
pub(super) const TICK: Duration = Duration::from_secs(1);

/// A presentation under way.
#[derive(Debug, Clone)]
pub(super) struct Presenting {
    /// The tab presented.
    pub key: u64,
    pub since: Instant,
    /// The zero-based line of the step the play is at; none before the
    /// first.
    pub line: Option<usize>,
    /// The diagram had its own window before, and keeps it after.
    pub had_window: bool,
    /// Whether that window is full screen now.
    pub full: bool,
}

/// The keys while presenting, in both windows: those of a presentation
/// page and of a presenter's remote, which sends Page Down and Page Up.
pub(super) fn key(key: &Key, mods: Modifiers) -> Option<Command> {
    let plain = !mods.control() && !mods.alt() && !mods.logo();
    let ask = |ask: paper::Ask| Some(Command::PlayAsk(ask));
    match key.as_ref() {
        Key::Named(Named::Escape) => Some(Command::Present),
        _ if !plain => None,
        Key::Character(c) if matches!(c, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9") => {
            ask(paper::Ask::Take(c.parse::<usize>().unwrap_or(1) - 1))
        }
        Key::Named(
            Named::Space | Named::ArrowRight | Named::ArrowDown | Named::PageDown | Named::Enter,
        ) => ask(paper::Ask::Next),
        Key::Named(Named::ArrowLeft | Named::ArrowUp | Named::PageUp | Named::Backspace) => {
            ask(paper::Ask::Back)
        }
        Key::Named(Named::Home) => ask(paper::Ask::Restart),
        Key::Character(c) if c.eq_ignore_ascii_case("f") => Some(Command::PresentFullScreen),
        Key::Named(Named::F11) => Some(Command::PresentFullScreen),
        _ => None,
    }
}

/// The speaker's notes for the step at the zero-based `line` of `text`:
/// the comments written right above it, one after the other, up to a
/// blank line or one that is no comment. Before the first step, those
/// that open the diagram, after its `@start…`. Omaspec's own lines (a
/// fold, a mark, lines by hand) are no notes.
pub(super) fn notes(text: &str, line: Option<usize>) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    match line {
        Some(line) => {
            let mut found: Vec<String> = (0..line.min(lines.len()))
                .rev()
                .map(|at| comment(lines[at]))
                .take_while(Option::is_some)
                .flatten()
                .filter(|note| !note.is_empty())
                .collect();
            found.reverse();
            found
        }
        None => {
            let start = lines
                .iter()
                .position(|l| {
                    let l = l.trim_start();
                    l.starts_with("@start") || l.starts_with("```")
                })
                .map_or(0, |at| at + 1);
            lines[start.min(lines.len())..]
                .iter()
                .skip_while(|l| l.trim().is_empty())
                .map(|l| comment(l))
                .take_while(Option::is_some)
                .flatten()
                .filter(|note| !note.is_empty())
                .collect()
        }
    }
}

/// The words of a comment line, PlantUML's `'` or Mermaid's `%%`; none
/// for a line that is no comment, or one Omaspec writes for itself.
fn comment(line: &str) -> Option<String> {
    let line = line.trim();
    let words = line
        .strip_prefix('\'')
        .filter(|rest| !rest.starts_with('/'))
        .or_else(|| {
            line.strip_prefix("%%")
                .filter(|rest| !rest.starts_with('{'))
        })?
        .trim();
    if words.contains("(Omaspec)") {
        return Some(String::new());
    }
    Some(words.to_owned())
}

/// Where the play is, in a few words: a scenario's message of all, a
/// machine's states and its options with the digit that takes each, a
/// trace's line.
fn where_at(offer: &paper::Offer) -> String {
    match offer {
        paper::Offer::Scenario { at, of } => format!("message {at} of {of}"),
        paper::Offer::Machine {
            options, at, ended, ..
        } => {
            if *ended || options.is_empty() {
                return format!("in {at}, at the end");
            }
            let options: Vec<String> = options
                .iter()
                .enumerate()
                .map(|(i, (label, to))| format!("{} {label} → {to}", i + 1))
                .collect();
            format!("in {at} · {}", options.join(" · "))
        }
        paper::Offer::Trace { played, of, .. } => format!("line {played} of {of}"),
    }
}

/// `elapsed` as a clock shows it: `4:07`, `1:02:30`.
fn clock(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

impl App {
    /// Presents the active diagram, or ends the presentation: the diagram
    /// in its own window full screen, played, the notes here.
    pub(super) fn toggle_present(&mut self) -> Task<Message> {
        if let Some(presenting) = self.presenting.take() {
            return self.end_present(presenting);
        }
        let key = self.doc().key;
        let had_window = self.detached.is_some();
        let window = match self.detached {
            Some(id) => window::set_mode(id, window::Mode::Fullscreen),
            None => self.detach_with(true),
        };
        let play = if self.doc().paper.playing() {
            Task::none()
        } else {
            self.toggle_play()
        };
        self.presenting = Some(Presenting {
            key,
            since: crate::clock::now(),
            line: None,
            had_window,
            full: true,
        });
        self.after(crate::worker::Alarm::Clock, TICK);
        Task::batch([window, play])
    }

    /// Ends `presenting`: the play stops, and the diagram goes back
    /// where it was, its window as before or beside the source again.
    fn end_present(&mut self, presenting: Presenting) -> Task<Message> {
        let key = presenting.key;
        let stop = match self.find(key) {
            Some(doc) if doc.paper.playing() => {
                self.close_beside(key);
                self.find(key)
                    .map(|doc| doc.paper.update(paper::Message::StopPlay))
                    .unwrap_or_else(Task::none)
                    .map(move |m| Message::Paper(key, m))
            }
            _ => Task::none(),
        };
        let window = match (presenting.had_window, self.detached) {
            (true, Some(id)) => window::set_mode(id, window::Mode::Windowed),
            _ => self.attach(),
        };
        Task::batch([stop, window])
    }

    /// The diagram's window full screen, or a window again.
    pub(super) fn toggle_present_full(&mut self) -> Task<Message> {
        let (Some(presenting), Some(id)) = (&mut self.presenting, self.detached) else {
            return Task::none();
        };
        presenting.full = !presenting.full;
        window::set_mode(
            id,
            if presenting.full {
                window::Mode::Fullscreen
            } else {
                window::Mode::Windowed
            },
        )
    }

    /// The step the play of tab `key` is at, from the source line it
    /// lights, for the notes.
    pub(super) fn presented_line(&mut self, key: u64, line: Option<usize>) {
        if let Some(presenting) = &mut self.presenting
            && presenting.key == key
        {
            presenting.line = line;
        }
    }

    /// The speaker's view, in the editor's window while presenting: the
    /// time since the start, where the play is, the notes of its step in
    /// large letters, the line they stand above, and the keys.
    pub(super) fn presenter_view(&self, presenting: &Presenting) -> Element<'_, Message> {
        let p = &self.palette;
        let Some(doc) = self.docs.iter().find(|d| d.key == presenting.key) else {
            return Space::new().into();
        };
        let source = doc.text();
        let notes = notes(&source, presenting.line);
        let (state, color) = match doc.paper.offer() {
            Some(offer) => (where_at(&offer), p.accent),
            None => doc.paper.state(p),
        };
        // A scenario's next message, and its notes, to see what comes.
        let next = match doc.paper.offer() {
            Some(paper::Offer::Scenario { at, of }) if at < of => {
                doc.paper.message_lines().get(at).copied()
            }
            _ => None,
        };
        let coming = next
            .map(|line| self::notes(&source, Some(line)))
            .unwrap_or_default();
        let elapsed = crate::clock::now().saturating_duration_since(presenting.since);
        let words = |s: String, size: f32, color| {
            text(s)
                .font(self.font)
                .size(size)
                .color(ui::color(color))
                .wrapping(text::Wrapping::WordOrGlyph)
        };
        let head = row![
            words(doc.name(), 18.0, p.foreground),
            Space::new().width(Length::Fill),
            words(clock(elapsed), 40.0, p.accent),
        ]
        .align_y(iced::Alignment::Center);
        let said: Element<'_, Message> = if notes.is_empty() {
            words(
                "no notes for this step: write them as comments right above its line".into(),
                18.0,
                p.muted,
            )
            .into()
        } else {
            column(
                notes
                    .into_iter()
                    .map(|note| words(note, 30.0, p.foreground).into()),
            )
            .spacing(10)
            .into()
        };
        let at = presenting
            .line
            .and_then(|line| source.lines().nth(line).map(|text| (line, text.trim())))
            .map_or_else(
                || "before the first step".to_owned(),
                |(line, text)| format!("line {} · {text}", line + 1),
            );
        let keys = "→ Space PageDown on · ← PageUp back · Home again · 1–9 an option · F full screen · Esc ends";
        let body = column![
            head,
            words(state, 16.0, color),
            container(said).padding([24, 0]),
            words(at, 15.0, p.dark_foreground),
            Space::new().height(Length::Fill),
            words(
                match (next, coming.is_empty()) {
                    (None, _) => String::new(),
                    (Some(_), true) => "next: a step without notes".to_owned(),
                    (Some(_), false) => format!("next: {}", coming.join(" ")),
                },
                18.0,
                p.dark_foreground,
            ),
            words(keys.into(), 14.0, p.muted),
        ]
        .spacing(14)
        .padding(40);
        container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style {
                background: Some(ui::color(self.palette.background).into()),
                ..container::Style::default()
            })
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notes_are_the_comments_right_above_the_step() {
        let text = "@startuml\n' Welcome: this is how an order goes.\n' Ask who has ordered before.\n\nAlice -> Shop : order\n' The shop checks the stock first.\n%% and keeps it a while\nShop -> Stock : reserve\n' fold (Omaspec) Box\nShop -> Alice : ok\n@enduml\n";
        assert_eq!(
            notes(text, None),
            [
                "Welcome: this is how an order goes.",
                "Ask who has ordered before."
            ]
        );
        // A blank line ends them: the welcome is not the first step's.
        assert!(notes(text, Some(4)).is_empty());
        assert_eq!(
            notes(text, Some(7)),
            ["The shop checks the stock first.", "and keeps it a while"]
        );
        // Omaspec's own lines are none.
        assert!(notes(text, Some(9)).is_empty());
        // A Mermaid diagram's opening notes come after its head, a
        // directive is none.
        assert_eq!(
            notes("sequenceDiagram\n%%{init: {}}%%\nA->>B: hi\n", Some(2)),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_presenter_steps_with_a_remote_and_escape_ends() {
        let none = Modifiers::empty();
        let named = |n| Key::Named(n);
        for on in [
            Named::PageDown,
            Named::ArrowRight,
            Named::ArrowDown,
            Named::Space,
        ] {
            assert_eq!(
                key(&named(on), none),
                Some(Command::PlayAsk(paper::Ask::Next))
            );
        }
        for back in [Named::PageUp, Named::ArrowLeft, Named::ArrowUp] {
            assert_eq!(
                key(&named(back), none),
                Some(Command::PlayAsk(paper::Ask::Back))
            );
        }
        assert_eq!(key(&named(Named::Escape), none), Some(Command::Present));
        assert_eq!(
            key(&Key::Character("f".into()), none),
            Some(Command::PresentFullScreen)
        );
        assert_eq!(
            key(&Key::Character("2".into()), none),
            Some(Command::PlayAsk(paper::Ask::Take(1)))
        );
        assert_eq!(clock(Duration::from_secs(247)), "4:07");
        assert_eq!(clock(Duration::from_secs(3750)), "1:02:30");
    }
}
