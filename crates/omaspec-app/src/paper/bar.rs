//! The bar under a playing diagram: where the machine is and what can
//! happen next, how far the scenario is, or the trace's line; the way
//! back, and out.
//!
//! It keeps its height and its width for the whole play, so the diagram
//! above it never moves: one row (a trace has a second line for what its
//! line says), as wide as the pane allows. A machine's options take a
//! second or third row where one would leave only their numbers, wherever
//! in the play that is: the bar has those rows from the start. Else what
//! does not fit is cut short with `…`.

use iced::widget::{button, column, container, mouse_area, row, text};
use iced::{Element, Font, Length, Theme, Vector};
use omaspec_core::lint::Severity;
use omaspec_theme::Palette;

use super::Message;
use super::play::{Ask, Offer};
use crate::ui;

/// The widest the bar gets.
const WIDEST: f32 = 900.0;
/// Around the bar in the pane, across.
const MARGIN: f32 = 18.0;
/// Inside the bar, across: its padding and its edge.
const PADDING: f32 = 13.0;
/// Between the bar's items.
const GAP: f32 = 8.0;
/// The row's height: a chip's, a line of text and its padding.
const ROW: f32 = 26.0;
/// The height of a trace's second line.
const NOTE: f32 = 18.0;
/// How wide a character is, as a share of its text's size. The bar's font
/// is the monospace one, 0.6 of its size wide (JetBrains Mono, DejaVu Sans
/// Mono, Fira Mono; Iosevka less); a little more, so a label is cut before
/// the row would run out.
const CHAR: f32 = 0.605;
/// The fewest characters a label or a state is cut to.
const SHORTEST: usize = 6;
/// The most rows of options the bar takes; past them the chips show their
/// numbers alone.
const MOST_ROWS: usize = 3;
/// Between two rows of options.
const ROW_GAP: f32 = 4.0;

/// The bar for `offer`, in a pane `pane` pixels wide.
pub fn play_bar<'a>(offer: &Offer, p: &Palette, font: Font, pane: f32) -> Element<'a, Message> {
    let accent = ui::color(p.accent);
    let muted = ui::color(p.dark_foreground);
    let words = move |s: String, color: iced::Color| {
        text(s)
            .font(font)
            .size(13)
            .color(color)
            .wrapping(text::Wrapping::None)
    };
    let inside = (pane - 2.0 * MARGIN).min(WIDEST) - 2.0 * PADDING;
    // What the bar says on the left; what is offered, in the room between,
    // clipped there should it not fit after all; the tools on the right.
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    // In rows of their own, should they need more than one.
    let mut offered: Vec<Vec<Element<'a, Message>>> = Vec::new();
    let mut rows = 1;
    let mut tools: Vec<Element<'a, Message>> = Vec::new();
    // A trace's second line: what its line says is wrong, or that it is not.
    let mut note: Option<(Option<Severity>, String)> = None;
    match offer {
        Offer::Machine {
            options,
            hover,
            at,
            steps,
            ended,
            can_go_back,
            every,
        } => {
            let back = (*can_go_back).then_some(("↶ back", "↶", Ask::Back));
            let again = (*steps > 0 || *ended).then_some(("⟲ again", "⟲", Ask::Restart));
            let head = if *ended {
                "● ended".to_owned()
            } else {
                head_of(at, inside)
            };
            let (room, signs) = room_beside(&head, back.is_some(), again.is_some(), inside);
            rows = rows_of(every, inside).max(rows_needed(options, room, signs));
            items.push(words(head, accent).into());
            let fit = arranged(options, room, signs, rows);
            for (label, sign, ask) in back.into_iter().chain(again) {
                tools.push(tool(
                    if fit.signs { sign } else { label },
                    Some(ask),
                    p,
                    font,
                ));
            }
            let per_row = fit.per_row;
            let mut chips = fit.options.into_iter().enumerate().peekable();
            while chips.peek().is_some() {
                let line: Vec<Element<'a, Message>> = chips
                    .by_ref()
                    .take(per_row)
                    .map(|(i, (label, to))| {
                        let chip = chip(i, &label, to.as_deref(), *hover == Some(i), p, font);
                        mouse_area(chip)
                            .on_enter(Message::Play(Ask::Hover(Some(i))))
                            .on_exit(Message::Play(Ask::Hover(None)))
                            .into()
                    })
                    .collect();
                offered.push(line);
            }
            if options.is_empty() && !*ended {
                offered.push(vec![words("nothing can happen here".into(), muted).into()]);
            }
        }
        Offer::Scenario { at, of } => {
            items.push(words(format!("● {at} / {of}"), accent).into());
            tools.push(tool("‹ back", (*at > 0).then_some(Ask::Back), p, font));
            tools.push(chip_next((*at < *of).then_some(Ask::Next), p, font));
            tools.push(tool("⟲ again", (*at > 0).then_some(Ask::Restart), p, font));
        }
        Offer::Trace {
            name,
            object,
            played,
            of,
            line,
            time,
            said,
            at,
            finding,
        } => {
            let third = (inside / 3.0 / (13.0 * CHAR)) as usize;
            let (head, color) = if at.is_empty() {
                (format!("○ {object}: not followed here"), muted)
            } else {
                (format!("● {object}: {at}"), accent)
            };
            let head = cut(&head, third);
            let place = match line {
                Some(line) => format!("{name}:{line} · {played} / {of}"),
                None => format!("{name} · {played} / {of}"),
            };
            let place = cut(&place, third);
            let used = width_of(head.chars().count() + place.chars().count(), 13.0)
                + tool_width("‹ back")
                + 28.0
                + width_of(6, 13.0)
                + tool_width("⟲ again")
                + tool_width("✕")
                + 6.0 * GAP;
            items.push(words(head, color).into());
            items.push(words(place, muted).into());
            if !said.is_empty() {
                let said = match time {
                    Some(time) => format!("{time}  {said}"),
                    None => said.clone(),
                };
                let room = ((inside - used) / (13.0 * CHAR)).max(0.0) as usize;
                offered.push(vec![
                    words(cut(&said, room), ui::color(p.bright_foreground)).into(),
                ]);
            }
            tools.push(tool("‹ back", (*played > 0).then_some(Ask::Back), p, font));
            tools.push(chip_next((*played < *of).then_some(Ask::Next), p, font));
            tools.push(tool(
                "⟲ again",
                (*played > 0).then_some(Ask::Restart),
                p,
                font,
            ));
            note = Some(match finding {
                Some((severity, message)) => (Some(*severity), message.clone()),
                None if *played > 0 => (None, "as the machine allows".to_owned()),
                None => (None, String::new()),
            });
        }
    }
    let accent_rgb = p.accent;
    let close = button(words("✕".into(), muted))
        .on_press(Message::StopPlay)
        .padding([3, 8])
        .style(move |_: &Theme, status| quiet(status, accent_rgb));
    tools.push(close.into());
    let fill = ui::color(p.dark_background);
    let wrong = note
        .as_ref()
        .is_some_and(|(severity, _)| *severity == Some(Severity::Error));
    let edge = if wrong {
        iced::Color {
            a: 0.8,
            ..ui::color(p.red)
        }
    } else {
        iced::Color { a: 0.45, ..accent }
    };
    // The tools at the right end, whatever is offered before them.
    let offered = offered.into_iter().map(|line| {
        row(line)
            .spacing(GAP)
            .align_y(iced::Alignment::Center)
            .into()
    });
    items.push(
        container(column(offered).spacing(ROW_GAP))
            .width(Length::Fill)
            .clip(true)
            .into(),
    );
    items.extend(tools);
    let line = container(row(items).spacing(GAP).align_y(iced::Alignment::Center))
        .height(rows_height(rows))
        .align_y(iced::Alignment::Center)
        .clip(true);
    let content: Element<'a, Message> = match note {
        Some((severity, message)) => {
            let color = match severity {
                Some(Severity::Error) => ui::color(p.red),
                Some(_) | None => muted,
            };
            let room = (inside / (13.0 * CHAR)) as usize;
            let note = container(words(cut(&message, room), color))
                .height(NOTE)
                .clip(true);
            column![line, note].spacing(6).into()
        }
        None => line.into(),
    };
    let bar = container(content)
        .padding([8, 12])
        .width(Length::Fill)
        .max_width(WIDEST)
        .style(move |_: &Theme| {
            container::Style::default()
                .background(iced::Color { a: 0.94, ..fill })
                .border(iced::Border {
                    color: edge,
                    width: 1.0,
                    radius: 14.0.into(),
                })
                .shadow(iced::Shadow {
                    color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.35),
                    offset: Vector::new(0.0, 4.0),
                    blur_radius: 18.0,
                })
        });
    container(bar)
        .width(Length::Fill)
        .align_x(iced::Alignment::Center)
        .padding([10.0, MARGIN])
        .into()
}

/// How high `rows` rows of options are, with the gaps between them.
fn rows_height(rows: usize) -> f32 {
    rows as f32 * ROW + rows.saturating_sub(1) as f32 * ROW_GAP
}

/// What the bar says first of a machine at `at`: its states, cut to a
/// third of the room `inside`.
fn head_of(at: &str, inside: f32) -> String {
    format!("● {}", cut(at, (inside / 3.0 / (13.0 * CHAR)) as usize))
}

/// The room left for a machine's options beside `head`, the tools shown
/// and the close, in a bar `inside` pixels wide inside; and how much more
/// the tools give when they show only their signs.
fn room_beside(head: &str, back: bool, again: bool, inside: f32) -> (f32, f32) {
    // The close, and the gap before it.
    let mut used = tool_width("✕") + GAP;
    let mut signs = 0.0;
    for (shown, label, sign) in [(back, "↶ back", "↶"), (again, "⟲ again", "⟲")] {
        if shown {
            used += tool_width(label) + GAP;
            signs += tool_width(label) - tool_width(sign);
        }
    }
    used += width_of(head.chars().count(), 13.0) + GAP;
    (inside - used, signs)
}

/// The rows a play needs for its options wherever the machine can be
/// (`every`), with both tools beside them as after a first step: the most
/// any place needs to keep its labels.
fn rows_of(every: &[super::play::Offered], inside: f32) -> usize {
    every
        .iter()
        .map(|(at, options)| {
            let (room, signs) = room_beside(&head_of(at, inside), true, true, inside);
            rows_needed(options, room, signs)
        })
        .max()
        .unwrap_or(1)
}

/// The fewest rows, up to [`MOST_ROWS`], in which `options` keep their
/// labels, if cut.
fn rows_needed(options: &[(String, String)], room: f32, signs: f32) -> usize {
    (1..=MOST_ROWS)
        .find(|&rows| fitted(options, room, signs, rows).shown > Shown::Numbers)
        .unwrap_or(MOST_ROWS)
}

/// The options in at most `rows` rows: in as many as show the most of
/// them, the fewest of those.
fn arranged(options: &[(String, String)], room: f32, signs: f32, rows: usize) -> Fit {
    let mut best = fitted(options, room, signs, 1);
    for rows in 2..=rows {
        let fit = fitted(options, room, signs, rows);
        if fit.shown > best.shown {
            best = fit;
        }
    }
    best
}

/// How wide `chars` characters of text `size` pixels high are.
fn width_of(chars: usize, size: f32) -> f32 {
    chars as f32 * size * CHAR
}

/// How wide a tool of the bar is with `label`.
fn tool_width(label: &str) -> f32 {
    16.0 + width_of(label.chars().count(), 13.0)
}

/// How wide the chip of option `number` is, with a label and a target of
/// these lengths (one of none is not shown).
fn chip_width(number: usize, label: usize, to: usize) -> f32 {
    let badge = 12.0 + width_of(number.to_string().len(), 11.0);
    let shown = |chars: usize, size: f32, extra: usize| {
        if chars == 0 {
            0.0
        } else {
            7.0 + width_of(chars + extra, size)
        }
    };
    20.0 + badge + shown(label, 13.0, 0) + shown(to, 12.0, 2)
}

/// How wide a row of chips is, each given by the lengths of its label and
/// its target, with the gaps between them.
fn chips_width(chips: impl Iterator<Item = (usize, usize)>) -> f32 {
    let all: f32 = chips
        .enumerate()
        .map(|(i, (label, to))| GAP + chip_width(i + 1, label, to))
        .sum();
    (all - GAP).max(0.0)
}

/// `text` in at most `most` characters, cut with `…`.
fn cut(text: &str, most: usize) -> String {
    let most = most.max(SHORTEST);
    if text.chars().count() <= most {
        return text.to_owned();
    }
    let kept: String = text.chars().take(most - 1).collect();
    format!("{}…", kept.trim_end())
}

/// The options as their chips show them, each a label and perhaps where it
/// leads; whether the tools beside them show only their signs; how many
/// stand in a row; and how much of them is shown.
#[derive(Debug, PartialEq)]
struct Fit {
    options: Vec<(String, Option<String>)>,
    signs: bool,
    per_row: usize,
    shown: Shown,
}

/// How much of its options a [`Fit`] shows, least first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Shown {
    /// Their numbers alone.
    Numbers,
    /// Their labels, some cut, without where they lead.
    Labels,
    /// Their labels whole; where they lead cut or left out.
    Targets,
    Whole,
}

/// The options in `rows` rows of `room` pixels, whole where they fit. Else
/// what is clicked stays whole longest: the tools show only their signs
/// (which gives `signs` pixels more), the targets are cut, longest first,
/// then left out, and only then are the labels cut, longest first; where
/// even the shortest do not fit, the chips show their numbers alone (the
/// keys 1 to 9 take them). What still does not fit is clipped at the bar's
/// end.
fn fitted(options: &[(String, String)], room: f32, signs: f32, rows: usize) -> Fit {
    let per_row = options.len().div_ceil(rows.max(1)).max(1);
    let count = |s: &str| s.chars().count();
    let width = |label_most: usize, to_most: Option<usize>| -> f32 {
        options
            .chunks(per_row)
            .map(|line| {
                chips_width(line.iter().map(|(label, to)| {
                    let to = to_most.map_or(0, |most| count(to).min(most));
                    (count(label).min(label_most), to)
                }))
            })
            .fold(0.0, f32::max)
    };
    let shown = |label_most: usize, to_most: Option<usize>, signs: bool, shown: Shown| Fit {
        options: options
            .iter()
            .map(|(label, to)| (cut(label, label_most), to_most.map(|most| cut(to, most))))
            .collect(),
        signs,
        per_row,
        shown,
    };
    let longest = |of: fn(&(String, String)) -> &String| {
        options
            .iter()
            .map(|o| count(of(o)))
            .max()
            .unwrap_or(0)
            .max(SHORTEST)
    };
    let (labels, targets) = (longest(|o| &o.0), longest(|o| &o.1));
    if width(labels, Some(targets)) <= room {
        return shown(labels, Some(targets), false, Shown::Whole);
    }
    let room = room + signs;
    for most in (SHORTEST..=targets).rev() {
        if width(labels, Some(most)) <= room {
            let whole = if most == targets {
                Shown::Whole
            } else {
                Shown::Targets
            };
            return shown(labels, Some(most), true, whole);
        }
    }
    for most in (SHORTEST..=labels).rev() {
        if width(most, None) <= room {
            let whole = if most == labels {
                Shown::Targets
            } else {
                Shown::Labels
            };
            return shown(most, None, true, whole);
        }
    }
    Fit {
        options: options.iter().map(|_| (String::new(), None)).collect(),
        signs: true,
        per_row,
        shown: Shown::Numbers,
    }
}

/// An option of a playing machine: its number, its event, where it leads.
fn chip<'a>(
    index: usize,
    event: &str,
    to: Option<&str>,
    hot: bool,
    p: &Palette,
    font: Font,
) -> Element<'a, Message> {
    let accent = ui::color(p.accent);
    let on_accent = ui::color(p.background);
    let number = container(
        text((index + 1).to_string())
            .font(font)
            .size(11)
            .color(on_accent),
    )
    .padding([0, 6])
    .style(move |_: &Theme| {
        container::Style::default()
            .background(accent)
            .border(iced::Border {
                radius: 999.0.into(),
                ..iced::Border::default()
            })
    });
    let mut content = row![number].spacing(7).align_y(iced::Alignment::Center);
    if !event.is_empty() {
        content = content.push(
            text(event.to_owned())
                .font(font)
                .size(13)
                .color(ui::color(p.bright_foreground))
                .wrapping(text::Wrapping::None),
        );
    }
    if let Some(to) = to {
        content = content.push(
            text(format!("→ {to}"))
                .font(font)
                .size(12)
                .color(ui::color(p.dark_foreground))
                .wrapping(text::Wrapping::None),
        );
    }
    let (lighter, accent_rgb) = (p.lighter_background, p.accent);
    button(content)
        .on_press(Message::Play(Ask::Take(index)))
        .padding([4, 10])
        .style(move |_: &Theme, status| {
            let hovered =
                hot || matches!(status, button::Status::Hovered | button::Status::Pressed);
            let base = ui::color(lighter);
            let glow = ui::color(accent_rgb);
            button::Style {
                background: Some(
                    if hovered {
                        iced::Color { a: 0.22, ..glow }
                    } else {
                        base
                    }
                    .into(),
                ),
                border: iced::Border {
                    color: if hovered {
                        glow
                    } else {
                        iced::Color { a: 0.0, ..glow }
                    },
                    width: 1.0,
                    radius: 999.0.into(),
                },
                ..button::Style::default()
            }
        })
        .into()
}

/// The scenario's step forward, the bar's main button.
fn chip_next<'a>(ask: Option<Ask>, p: &Palette, font: Font) -> Element<'a, Message> {
    let accent = p.accent;
    let on_accent = ui::color(p.background);
    let muted = ui::color(p.dark_foreground);
    let enabled = ask.is_some();
    button(
        text("next ›")
            .font(font)
            .size(13)
            .color(if enabled { on_accent } else { muted }),
    )
    .on_press_maybe(ask.map(Message::Play))
    .padding([4, 14])
    .style(move |_: &Theme, status| {
        let glow = ui::color(accent);
        let strong = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: enabled.then(|| {
                if strong {
                    glow.into()
                } else {
                    iced::Color { a: 0.85, ..glow }.into()
                }
            }),
            border: iced::Border {
                color: if enabled { glow } else { muted },
                width: 1.0,
                radius: 999.0.into(),
            },
            ..button::Style::default()
        }
    })
    .into()
}

/// A small text button of the bar; greyed out without `ask`.
fn tool<'a>(label: &str, ask: Option<Ask>, p: &Palette, font: Font) -> Element<'a, Message> {
    let color = if ask.is_some() {
        ui::color(p.foreground)
    } else {
        iced::Color {
            a: 0.4,
            ..ui::color(p.dark_foreground)
        }
    };
    let accent = p.accent;
    button(
        text(label.to_owned())
            .font(font)
            .size(13)
            .color(color)
            .wrapping(text::Wrapping::None),
    )
    .on_press_maybe(ask.map(Message::Play))
    .padding([3, 8])
    .style(move |_: &Theme, status| quiet(status, accent))
    .into()
}

/// A button that shows only when the pointer is on it.
fn quiet(status: button::Status, accent: omaspec_theme::Rgb) -> button::Style {
    let glow = ui::color(accent);
    button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then(|| iced::Color { a: 0.16, ..glow }.into()),
        border: iced::Border {
            radius: 999.0.into(),
            ..iced::Border::default()
        },
        ..button::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(of: &[(&str, &str)]) -> Vec<(String, String)> {
        of.iter()
            .map(|(label, to)| ((*label).to_owned(), (*to).to_owned()))
            .collect()
    }

    fn width(fitted: &[(String, Option<String>)]) -> f32 {
        chips_width(fitted.iter().map(|(label, to)| {
            (
                label.chars().count(),
                to.as_ref().map_or(0, |t| t.chars().count()),
            )
        }))
    }

    #[test]
    fn options_that_fit_are_shown_whole() {
        let of = options(&[("send failed", "Retry"), ("still down", "Still down")]);
        let fit = fitted(&of, 800.0, 90.0, 1);
        assert_eq!(
            fit,
            Fit {
                options: vec![
                    ("send failed".to_owned(), Some("Retry".to_owned())),
                    ("still down".to_owned(), Some("Still down".to_owned())),
                ],
                signs: false,
                per_row: 2,
                shown: Shown::Whole,
            }
        );
    }

    /// Where the options do not fit, the tools show their signs and the
    /// targets are cut before a label is.
    #[test]
    fn the_targets_give_way_before_the_labels() {
        let of = options(&[
            ("servers unexpectedly unavailable", "Degraded"),
            ("send failed", "Retry"),
        ]);
        let whole = width(&fitted(&of, 2000.0, 0.0, 1).options);
        // A little short: the longer target is cut.
        let fit = fitted(&of, whole - 10.0, 0.0, 1);
        assert!(fit.signs);
        assert_eq!(
            fit.options,
            [
                (
                    "servers unexpectedly unavailable".to_owned(),
                    Some("Degra…".to_owned())
                ),
                ("send failed".to_owned(), Some("Retry".to_owned())),
            ]
        );
        // Shorter: the targets are left out, the labels still whole.
        let fit = fitted(&of, whole - 30.0, 0.0, 1);
        assert_eq!(
            fit.options,
            [
                ("servers unexpectedly unavailable".to_owned(), None),
                ("send failed".to_owned(), None),
            ]
        );
        // Shorter still: the longest label is cut, the short one whole.
        let fit = fitted(&of, whole - 200.0, 0.0, 1);
        assert!(fit.options[0].0.ends_with('…'), "{fit:?}");
        assert_eq!(fit.options[1], ("send failed".to_owned(), None));
        assert!(width(&fit.options) <= whole - 200.0, "{fit:?}");
    }

    /// Grok's three options in a pane 800 pixels wide, beside the state and
    /// the tools: on one line, the short label whole, the long ones cut.
    #[test]
    fn long_options_are_cut_to_the_room_longest_first() {
        let of = options(&[
            ("restarts / reroute", "Fix attempt"),
            ("early signs of stabilisation", "Stabilizing"),
            ("send failed", "Retry"),
        ]);
        let room = 800.0 - 2.0 * MARGIN - 2.0 * PADDING - 330.0;
        let fit = fitted(&of, room, 90.0, 1);
        assert!(fit.signs);
        assert!(width(&fit.options) <= room + 90.0, "{fit:?}");
        assert!(fit.options.iter().all(|(_, to)| to.is_none()), "{fit:?}");
        assert!(fit.options[1].0.ends_with('…'), "{fit:?}");
        assert_eq!(fit.options[2].0, "send failed");
        // As long as the room allows: a little less, and it is cut more.
        let less = fitted(&of, width(&fit.options) - 90.0 - 1.0, 90.0, 1);
        assert_ne!(less, fit);
    }

    #[test]
    fn options_too_many_for_the_room_show_their_numbers() {
        let of = options(&[
            ("restarts and reroute", "Fix attempt"),
            ("early signs of stabilisation", "Stabilizing"),
            ("intermittent issues", "Degraded"),
            ("send failed", "Retry"),
        ]);
        // Room for the shortest labels: cut to them.
        let shortest = chips_width((0..4).map(|_| (SHORTEST, 0)));
        let fit = fitted(&of, shortest, 0.0, 1);
        assert!(
            fit.options
                .iter()
                .all(|(label, to)| to.is_none() && (5..=SHORTEST).contains(&label.chars().count())),
            "{fit:?}"
        );
        // Less: the numbers alone, every option still there.
        let fit = fitted(&of, shortest - 1.0, 0.0, 1);
        assert_eq!(fit.options, vec![(String::new(), None); 4]);
        assert!(chips_width((0..4).map(|_| (0, 0))) < shortest / 2.0);
    }

    /// REQ-045, `never.mmd` 800 pixels wide: in `Unstable`, after a step,
    /// six options beside back and again left their numbers alone. They
    /// take a second row, whole, and the bar has it in every state.
    #[test]
    fn options_that_would_show_their_numbers_take_a_second_row() {
        let unstable = options(&[
            ("try 1", "Attempt 1"),
            ("try 2", "Attempt 2"),
            ("try 3", "Attempt 3"),
            ("try 4", "Attempt 4"),
            ("try 5", "Attempt 5"),
            ("Rust 1.100", "Stable"),
        ]);
        let inside = 800.0 - 2.0 * MARGIN - 2.0 * PADDING;
        let (room, signs) = room_beside(&head_of("Unstable", inside), true, true, inside);
        assert_eq!(fitted(&unstable, room, signs, 1).shown, Shown::Numbers);
        assert_eq!(rows_needed(&unstable, room, signs), 2);
        let fit = arranged(&unstable, room, signs, 2);
        assert_eq!(fit.per_row, 3);
        assert!(fit.shown >= Shown::Targets, "{fit:?}");
        assert_eq!(fit.options[5].0, "Rust 1.100");
        // At the start, without the tools, they fit in one; the bar keeps
        // the second row all the same.
        let (first, signs_first) = room_beside(&head_of("Unstable", inside), false, false, inside);
        assert!(fitted(&unstable, first, signs_first, 1).shown > Shown::Numbers);
        let reverted = options(&[("reverted", "Unstable")]);
        let every = vec![
            ("Unstable".to_owned(), unstable),
            ("Attempt 1".to_owned(), reverted.clone()),
        ];
        assert_eq!(rows_of(&every, inside), 2);
        // A state with one option in a bar of two rows shows it in one.
        assert_eq!(arranged(&reverted, room, signs, 2).per_row, 1);
        assert_eq!(rows_of(&every[1..], inside), 1);
        assert_eq!(rows_height(2), 2.0 * ROW + ROW_GAP);
    }

    #[test]
    fn a_cut_keeps_its_length_and_ends_in_an_ellipsis() {
        assert_eq!(cut("early signs of stabilisation", 12), "early signs…");
        assert_eq!(cut("send failed", 12), "send failed");
        // Never shorter than the shortest.
        assert_eq!(cut("restarts and reroute", 2), "resta…");
        assert_eq!(cut("Degraded, Send", 40), "Degraded, Send");
    }
}
