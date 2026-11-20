//! The bar under a playing diagram: where the machine is and what can
//! happen next, how far the scenario is, or the trace's line; the way
//! back, and out.
//!
//! It keeps its height and its width for the whole play, so the diagram
//! above it never moves: one row (a trace has a second line for what its
//! line says), as wide as the pane allows up to [`WIDEST`]. A machine's
//! options take a second or third row where one would leave only their
//! numbers, wherever in the play that is: the bar has those rows from the
//! start. Where not even three rows keep their labels, the bar is as wide
//! as the pane; where that does not either, the numbers take the rows
//! they need, and one where even three would clip them. Else what does
//! not fit is cut short with `…`.

use std::sync::Arc;

use iced::widget::{Space, button, column, container, mouse_area, row, text};
use iced::{Element, Font, Length, Theme, Vector};
use omaspec_core::lint::Severity;
use omaspec_theme::Palette;

use super::Message;
use super::play::{Ask, Offer, Offered};
use crate::ui;

mod fit;
use fit::*;

/// Inside the bar, above and below what it says.
const PAD_Y: f32 = 8.0;
/// Around the bar in the pane, above and below.
const AROUND_Y: f32 = 10.0;

/// How high the bar for `offer` stands at the foot of a pane `pane` pixels
/// wide, with the room around it: what a camera that follows the play
/// keeps what it looks at above.
pub fn height(offer: &Offer, pane: f32) -> f32 {
    let said = match offer {
        Offer::Machine {
            options,
            at,
            steps,
            ended,
            can_go_back,
            every,
            ..
        } => {
            let machine = machine_fit(
                every,
                pane,
                (!*ended).then_some(at.as_str()),
                (*can_go_back, *steps > 0 || *ended),
                options,
            );
            rows_height(machine.rows + usize::from(machine.layout.stacked))
        }
        Offer::Scenario { .. } => ROW,
        // Beside a scenario, what went wrong may take two lines.
        Offer::Trace { beside: true, .. } => ROW + 6.0 + 2.0 * NOTE,
        Offer::Trace { .. } => ROW + 6.0 + NOTE,
    };
    said + 2.0 * (PAD_Y + AROUND_Y)
}

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
    let mut inside = inside_of(pane);
    // What the bar says on the left; what is offered, in the room between,
    // clipped there should it not fit after all; the tools on the right.
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    // In rows of their own, should they need more than one.
    let mut offered: Vec<Vec<Element<'a, Message>>> = Vec::new();
    let mut rows = 1;
    // The head in a row of its own, the tools beside it, the options below.
    let mut stacked = false;
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
            let machine = machine_fit(
                every,
                pane,
                (!*ended).then_some(at.as_str()),
                (back.is_some(), again.is_some()),
                options,
            );
            (inside, stacked, rows) = (machine.layout.inside, machine.layout.stacked, machine.rows);
            let (room, signs) = (machine.room, machine.signs);
            items.push(words(machine.head, accent).into());
            let fit = arranged(options, room, signs, rows);
            for (label, sign, ask) in back.into_iter().chain(again) {
                tools.push(tool(
                    if fit.signs || stacked { sign } else { label },
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
            beside,
        } => {
            let third = (inside / 3.0 / (13.0 * CHAR)) as usize;
            let (head, color) = if at.is_empty() {
                (format!("○ {object}: not followed here"), muted)
            } else {
                (format!("● {object}: {at}"), accent)
            };
            // Beside a scenario only the event it took stands in the row
            // with the head: the head has the room that leaves.
            let head = if *beside {
                let row = (inside / (13.0 * CHAR)) as usize;
                let event =
                    said.chars().count() + time.as_ref().map_or(0, |t| t.chars().count() + 2);
                cut(&head, row.saturating_sub(event + 3).max(row / 2))
            } else {
                cut(&head, third)
            };
            let place = match line {
                Some(line) => format!("{name}:{line} · {played} / {of}"),
                None => format!("{name} · {played} / {of}"),
            };
            let place = cut(&place, third);
            // Beside a scenario, which steps it: where it is, nothing to press.
            let used = if *beside {
                width_of(head.chars().count(), 13.0) + 2.0 * GAP
            } else {
                width_of(head.chars().count() + place.chars().count(), 13.0)
                    + tool_width("‹ back")
                    + 28.0
                    + width_of(6, 13.0)
                    + tool_width("⟲ again")
                    + tool_width("✕")
                    + 6.0 * GAP
            };
            items.push(words(head, color).into());
            if !*beside {
                items.push(words(place, muted).into());
            }
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
            if !*beside {
                tools.push(tool("‹ back", (*played > 0).then_some(Ask::Back), p, font));
                tools.push(chip_next((*played < *of).then_some(Ask::Next), p, font));
                tools.push(tool(
                    "⟲ again",
                    (*played > 0).then_some(Ask::Restart),
                    p,
                    font,
                ));
            }
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
    if !matches!(offer, Offer::Trace { beside: true, .. }) {
        tools.push(close.into());
    }
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
    let offered = container(column(offered).spacing(ROW_GAP))
        .width(Length::Fill)
        .clip(true);
    let line: Element<'a, Message> = if stacked {
        // The head and the tools above, the options across the whole bar.
        items.push(Space::new().width(Length::Fill).into());
        items.extend(tools);
        let top = container(row(items).spacing(GAP).align_y(iced::Alignment::Center))
            .height(ROW)
            .align_y(iced::Alignment::Center)
            .clip(true);
        let below = container(offered)
            .height(rows_height(rows))
            .align_y(iced::Alignment::Center)
            .clip(true);
        column![top, below].spacing(ROW_GAP).into()
    } else {
        items.push(offered.into());
        items.extend(tools);
        container(row(items).spacing(GAP).align_y(iced::Alignment::Center))
            .height(rows_height(rows))
            .align_y(iced::Alignment::Center)
            .clip(true)
            .into()
    };
    let content: Element<'a, Message> = match note {
        Some((severity, message)) => {
            let color = match severity {
                Some(Severity::Error) => ui::color(p.red),
                Some(_) | None => muted,
            };
            let room = (inside / (13.0 * CHAR)) as usize;
            // Beside a scenario the bar has nothing to press, and room for
            // the whole of what went wrong.
            let lines = if matches!(offer, Offer::Trace { beside: true, .. }) {
                2
            } else {
                1
            };
            let note = container(words(wrapped(&message, room, lines), color))
                .height(NOTE * lines as f32)
                .clip(true);
            column![line, note].spacing(6).into()
        }
        None => line,
    };
    let bar = container(content)
        .padding([PAD_Y, 12.0])
        .width(Length::Fill)
        .max_width(inside + 2.0 * PADDING)
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
        .padding([AROUND_Y, MARGIN])
        .into()
}

/// How much higher than a row the bar of a play of `source` is in a pane
/// `pane` pixels wide: the rows its options will need, as the play finds
/// them, so a recording leaves the room.
pub fn extra_height(source: &str, language: omaspec_core::Language, pane: f32) -> f32 {
    let Some(every) = super::play::every_offer_of(source, language) else {
        return 0.0;
    };
    extra_height_of(layout_of(&Arc::new(every), pane))
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
