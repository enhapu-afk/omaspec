//! What the pane shows: the sheet on its canvas with the marks over it,
//! what a missing picture says, and the bar under a play.

use iced::widget::{Canvas, Stack, button, column, container, mouse_area, row, text};
use iced::{Element, Font, Length, Theme, Vector};
use omaspec_core::lint::Severity;
use omaspec_theme::Palette;

use super::frames::{Motion, Timed};
use super::passage::PassageLayer;
use super::play::{Ask, Offer};
use super::{Message, Paper, Status, canvas, focus, fold, hand, held, play};
use crate::ui;

impl Paper {
    /// The diagram, and the play over it. Without a picture it says what
    /// is missing, unless `quiet` (the editor shows an empty state of its
    /// own then).
    pub fn view(&self, palette: &Palette, font: Font, quiet: bool) -> Element<'_, Message> {
        let p = palette;
        let (stale, placeholder) = match &self.status {
            Status::InSync => (false, ""),
            Status::Waiting | Status::Rendering => (false, "rendering…"),
            Status::Failed(_) | Status::Unreadable(_) => {
                (true, if quiet { "" } else { "no diagram yet" })
            }
        };
        // What the diagram's own layer asks frames for; a play asks in its
        // layer over the diagram.
        let motion = match (&self.glide, &self.passage, &self.fade) {
            (Some(_), Some(_), _) => Some(Motion::Dive),
            (Some(_), None, _) => Some(Motion::Glide),
            (None, _, Some(_)) => Some(Motion::Fade),
            (None, _, None) => None,
        };
        // While the new raster is on its way, the marks of the picture still
        // on screen stay on it, and move with it. Nothing there is a click:
        // the boxes a click would find are the new diagram's.
        let held = self.held_picture();
        let (links, spots, mark_view, mark_glide) = match held {
            Some(previous) => (
                previous.links.as_slice(),
                previous.spots.clone(),
                previous.view_at(self.view),
                self.glide
                    .map(|glide| glide.mapped(|view| previous.view_at(view))),
            ),
            None if self.overview.shown.is_some() => {
                (self.followable(), self.fold_spots(), self.view, self.glide)
            }
            None => (self.followable(), Vec::new(), self.view, self.glide),
        };
        let (click_links, click_folds) = if held.is_some() {
            (&[][..], Vec::new())
        } else {
            (links, spots.clone())
        };
        let canvas: Element<'_, Message> = Canvas::new(
            Timed::new(canvas::DiagramCanvas {
                diagram: self.diagram.as_ref().map(|d| d.size),
                overview: self.overview.shown.as_ref(),
                detail: self.detail.shown.as_ref(),
                fade: self.fade.as_ref(),
                previous: held,
                view: self.view,
                glide: self.glide,
                background: ui::color(p.background),
                placeholder,
                text: ui::color(p.dark_foreground),
                font,
                scale: self.scale_factor,
                by_hand: self.drawable(),
                hand_cursor: self
                    .hand
                    .as_ref()
                    .map(hand::Hand::cursor)
                    .or(self.grab.as_ref().map(held::Grab::cursor)),
                links: click_links,
                folds: click_folds,
                focusing: self.folding && !self.document && self.play.is_none(),
            })
            .driving(motion),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
        // The lines of a glide, over the pictures and the boxes on their way.
        let lines = self
            .fade
            .as_ref()
            .filter(|fade| fade.lines.is_some())
            .map(|fade| {
                Canvas::new(Timed::new(canvas::GlideLines {
                    fade,
                    glide: self.glide,
                    background: ui::color(p.background),
                    scale: self.scale_factor,
                }))
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
            });
        let marks = (!links.is_empty() && self.overview.shown.is_some()).then(|| {
            Canvas::new(Timed::new(canvas::LinkMarks {
                links,
                view: mark_view,
                glide: mark_glide,
                color: ui::color(p.accent),
                scale: self.scale_factor,
            }))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        });
        let folds = (!spots.is_empty()).then(|| {
            Canvas::new(Timed::new(fold::FoldMarks {
                spots,
                view: mark_view,
                glide: mark_glide,
                color: ui::color(p.accent),
                paper: ui::color(p.background),
                scale: self.scale_factor,
            }))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        });
        // The focus: a veil and what comes through, then its light.
        let focus = match (self.lit(), &self.diagram, self.overview.shown.as_ref()) {
            (Some(lit), Some(diagram), Some(overview)) => {
                let layer = |over: bool| -> Element<'_, Message> {
                    Canvas::new(Timed::new(focus::FocusLayer {
                        lit: lit.clone(),
                        diagram: diagram.size,
                        overview,
                        detail: self.detail.shown.as_ref(),
                        view: self.view,
                        glide: self.glide,
                        accent: ui::color(p.accent),
                        veil: ui::color(p.background),
                        over,
                        scale: self.scale_factor,
                    }))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
                };
                [Some(layer(false)), Some(layer(true))]
            }
            _ => [None, None],
        };
        let hand = match (&self.hand, &self.diagram) {
            (Some(hand), Some(diagram)) if self.overview.shown.is_some() => Some(
                Canvas::new(Timed::new(hand::HandLayer {
                    hand,
                    map: &diagram.drawn.map,
                    view: self.view,
                    glide: self.glide,
                    accent: ui::color(p.accent),
                    paper: ui::color(p.background),
                }))
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            ),
            _ => None,
        };
        let grab = match (&self.grab, &self.diagram) {
            (Some(grab), Some(diagram)) if self.overview.shown.is_some() => Some(
                Canvas::new(Timed::new(held::HeldLayer {
                    grab,
                    map: &diagram.drawn.map,
                    view: self.view,
                    glide: self.glide,
                    accent: ui::color(p.accent),
                }))
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            ),
            _ => None,
        };
        // Its own layer: in the picture's layer the raster would cover it.
        let light = self.breath.map(|breath| {
            Canvas::new(Timed::new(canvas::BreathLight {
                view: self.view,
                glide: self.glide,
                breath,
                color: ui::color(p.accent),
                paper: ui::color(p.background),
                scale: self.scale_factor,
            }))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        });
        // Over the diagram, its marks and its lights: the veil covers them.
        let passage = self.passage.as_ref().map(|passage| {
            Canvas::new(Timed::new(PassageLayer {
                passage,
                view: self.view,
                glide: self.glide,
                veil: ui::color(p.background),
                scale: self.scale_factor,
            }))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        });
        // The red edge lies over everything else, a play's veil too.
        let edge = match (&self.diagram, &self.overview.shown) {
            (Some(diagram), Some(_)) if stale => Some(
                Canvas::new(Timed::new(canvas::PaperEdge {
                    diagram: diagram.size,
                    view: self.view,
                    glide: self.glide,
                    color: ui::color(p.red),
                    scale: self.scale_factor,
                }))
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            ),
            _ => None,
        };
        let (Some(play), Some(diagram), Some(overview)) =
            (&self.play, &self.diagram, self.overview.shown.as_ref())
        else {
            let [veil, glow] = focus;
            return container(Stack::with_children(
                [
                    Some(canvas),
                    lines,
                    veil,
                    glow,
                    marks,
                    folds,
                    light,
                    hand,
                    grab,
                    passage,
                    edge,
                ]
                .into_iter()
                .flatten(),
            ))
            .clip(true)
            .into();
        };
        let layer = |over: bool| -> Element<'_, Message> {
            Canvas::new(
                Timed::new(canvas::PlayCanvas {
                    diagram: diagram.size,
                    overview,
                    detail: self.detail.shown.as_ref(),
                    view: self.view,
                    glide: self.glide,
                    play,
                    look: play::Look {
                        accent: ui::color(p.accent),
                        veil: ui::color(p.background),
                        light: ui::color(p.bright_foreground),
                    },
                    over,
                    at: self.clock,
                    leaving: self.leaving,
                    scale: self.scale_factor,
                })
                .driving(over.then_some(Motion::Play)),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };
        // Clipped to its own room: unclipped, a picture zoomed past the
        // pane's foot, and a play's veil over it, lay over the bar below.
        let diagram: Element<'_, Message> = container(Stack::with_children(
            [
                Some(canvas),
                lines,
                light,
                Some(layer(false)),
                Some(layer(true)),
                edge,
            ]
            .into_iter()
            .flatten(),
        ))
        .clip(true)
        .into();
        // The strip stays while the play fades, so the diagram does not
        // jump under it. Steps are ignored once it is leaving.
        column![diagram, play_bar(&play.offer(), p, font)].into()
    }
}

/// The bar under a playing diagram: where the machine is and what can
/// happen next, or how far the scenario is; the way back, and out.
fn play_bar<'a>(offer: &Offer, p: &Palette, font: Font) -> Element<'a, Message> {
    let accent = ui::color(p.accent);
    let muted = ui::color(p.dark_foreground);
    let words = move |s: String, color: iced::Color| text(s).font(font).size(13).color(color);
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    // A line of its own under the bar's row: what a trace says is wrong.
    let mut note: Option<(Severity, String)> = None;
    match offer {
        Offer::Machine {
            options,
            hover,
            at,
            steps,
            ended,
            can_go_back,
        } => {
            let shown = if *ended {
                "ended".to_owned()
            } else {
                at.clone()
            };
            items.push(words(format!("● {shown}"), accent).into());
            for (i, (event, to)) in options.iter().enumerate() {
                let chip = chip(i, event, to, *hover == Some(i), p, font);
                items.push(
                    mouse_area(chip)
                        .on_enter(Message::Play(Ask::Hover(Some(i))))
                        .on_exit(Message::Play(Ask::Hover(None)))
                        .into(),
                );
            }
            if options.is_empty() && !*ended {
                items.push(words("nothing can happen here".into(), muted).into());
            }
            if *can_go_back {
                items.push(tool("↶ back", Some(Ask::Back), p, font));
            }
            if *steps > 0 || *ended {
                items.push(tool("⟲ again", Some(Ask::Restart), p, font));
            }
        }
        Offer::Scenario { at, of } => {
            items.push(words(format!("● {at} / {of}"), accent).into());
            items.push(tool("‹ back", (*at > 0).then_some(Ask::Back), p, font));
            items.push(chip_next((*at < *of).then_some(Ask::Next), p, font));
            items.push(tool("⟲ again", (*at > 0).then_some(Ask::Restart), p, font));
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
            let (head, color) = if at.is_empty() {
                (format!("○ {object}: not followed here"), muted)
            } else {
                (format!("● {object}: {at}"), accent)
            };
            items.push(words(head, color).into());
            let place = match line {
                Some(line) => format!("{name}:{line} · {played} / {of}"),
                None => format!("{name} · {played} / {of}"),
            };
            items.push(words(place, muted).into());
            if !said.is_empty() {
                let said = match time {
                    Some(time) => format!("{time}  {said}"),
                    None => said.clone(),
                };
                items.push(words(said, ui::color(p.bright_foreground)).into());
            }
            items.push(tool("‹ back", (*played > 0).then_some(Ask::Back), p, font));
            items.push(chip_next((*played < *of).then_some(Ask::Next), p, font));
            items.push(tool(
                "⟲ again",
                (*played > 0).then_some(Ask::Restart),
                p,
                font,
            ));
            note.clone_from(finding);
        }
    }
    let accent_rgb = p.accent;
    let close = button(words("✕".into(), muted))
        .on_press(Message::StopPlay)
        .padding([3, 8])
        .style(move |_: &Theme, status| quiet(status, accent_rgb));
    items.push(close.into());
    let fill = ui::color(p.dark_background);
    let wrong = note
        .as_ref()
        .is_some_and(|(severity, _)| *severity == Severity::Error);
    let edge = if wrong {
        iced::Color {
            a: 0.8,
            ..ui::color(p.red)
        }
    } else {
        iced::Color { a: 0.45, ..accent }
    };
    let items = row(items)
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .wrap();
    let content: Element<'a, Message> = match note {
        Some((severity, message)) => {
            let color = if severity == Severity::Error {
                ui::color(p.red)
            } else {
                muted
            };
            column![items, words(message, color)].spacing(6).into()
        }
        None => items.into(),
    };
    let bar = container(content)
        .padding([8, 12])
        .max_width(900)
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
        .padding([10, 18])
        .into()
}

/// An option of a playing machine: its number, its event, where it leads.
fn chip<'a>(
    index: usize,
    event: &str,
    to: &str,
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
    let content = row![
        number,
        text(event.to_owned())
            .font(font)
            .size(13)
            .color(ui::color(p.bright_foreground)),
        text(format!("→ {to}"))
            .font(font)
            .size(12)
            .color(ui::color(p.dark_foreground)),
    ]
    .spacing(7)
    .align_y(iced::Alignment::Center);
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
    button(text(label.to_owned()).font(font).size(13).color(color))
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
