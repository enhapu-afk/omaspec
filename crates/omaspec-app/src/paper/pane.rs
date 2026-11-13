//! What the pane shows: the sheet on its canvas with the marks over it,
//! what a missing picture says, and the bar under a play.

use iced::widget::{Canvas, Stack, column, container};
use iced::{Element, Font, Length};
use omaspec_theme::Palette;

use super::frames::{Motion, Timed};
use super::passage::PassageLayer;
use super::{Message, Paper, Status, bar, canvas, focus, fold, hand, held, play};
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
        column![
            diagram,
            bar::play_bar(&play.offer(), p, font, self.viewport.width)
        ]
        .into()
    }
}
