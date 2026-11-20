//! A diagram on a pane without a render thread or a window: what the
//! tests and `omaspec play FILE -o play.gif` need to show one and draw
//! its pixels here and now.

use iced::Size;
use omaspec_core::Language;
use omaspec_render::RenderError;

use super::Paper;
use super::view::Glide;
use crate::worker::{Answer, Drawn, Ticket};

/// A render thread's answer for the request `id` of the tab `doc`.
pub fn answer(doc: u64, id: u64, result: Result<Drawn, RenderError>) -> Answer {
    Answer {
        ticket: Ticket { doc, id },
        source: "".into(),
        language: Language::Mermaid,
        result,
    }
}

impl Paper {
    /// Pretends one more request went out; its number.
    pub fn pretend_next(&mut self) -> u64 {
        self.requested += 1;
        self.requested
    }

    /// The source lines of the scenario's messages on screen, in the
    /// order its play takes them; none for another diagram.
    pub fn message_lines(&self) -> Vec<usize> {
        self.diagram.as_ref().map_or_else(Vec::new, |d| {
            d.drawn.map.messages.iter().map(|m| m.line).collect()
        })
    }

    /// What the play offers now, if one runs.
    pub fn offer(&self) -> Option<super::Offer> {
        self.play.as_ref().map(super::Play::offer)
    }

    /// Whether the play's light is still on its way at `now`, or the view
    /// glides with it.
    pub fn play_moving(&self, now: std::time::Instant) -> bool {
        self.play.as_ref().is_some_and(|play| play.animating(now))
            || self.glide.is_some_and(|glide| !glide.done(now))
    }

    /// The containers the picture offers to fold, and those folded in it.
    pub fn containers(&self) -> Vec<String> {
        self.containers_to_fold()
            .into_iter()
            .map(|spot| spot.name)
            .collect()
    }

    /// The containers folded, as the next drawing is asked for.
    pub fn folded(&self) -> &[String] {
        &self.folds
    }

    /// The glide under way, a dive too.
    pub fn glide(&self) -> Option<Glide> {
        self.glide
    }

    /// What glows after the glide from another version, box by box
    /// ([`super::glow`]).
    #[cfg(test)]
    pub fn glowing(&self) -> Vec<super::glow::Change> {
        self.glow
            .iter()
            .flat_map(|glow| glow.boxes.iter().map(|b| b.change))
            .collect()
    }

    /// Whether the last picture drawn stands on screen: nothing on its
    /// way, fading or gliding.
    pub fn arrived(&self) -> bool {
        matches!(self.arrival, super::arrival::Arrival::Still) && self.glide.is_none()
    }

    /// Puts the view at `zoom` and `center` at once, as a recording holds
    /// it: the picture coming in, gliding or fading, goes there too, and
    /// the one still on screen before it stays where it is. Setting
    /// [`Self::view`] alone after a picture came would end its glide where
    /// the view was, and jump.
    pub fn hold_view(&mut self, zoom: f32, center: iced::Point) {
        let view = super::View {
            zoom,
            center,
            fit: false,
        };
        self.view = view;
        if let Some(fade) = self.arrival.fade_mut() {
            fade.new_view = view;
        }
        if let Some(previous) = self.arrival.previous_mut() {
            previous.then = view;
        }
    }

    /// How many pictures of the diagram before fade out, if a fade runs.
    pub fn fading(&self) -> Option<usize> {
        self.arrival.fade().map(|fade| fade.old.len())
    }

    /// The pane's size.
    pub fn viewport(&self) -> Size {
        self.viewport
    }
}

/// The glide from the picture `old` is of to the one `new` is of, as the
/// pane works it out when a container folds ([`super::morph`],
/// [`super::wires`]), for a page that plays it in vectors
/// ([`omaspec_render::export::folding`]). The sizes are the pictures' own.
/// Empty where no box moved, came or went. With `glow`, each box says
/// how it glows, as a history's glide does ([`super::glow`]).
pub fn fold_glide(
    old: &omaspec_uml::map::Map,
    new: &omaspec_uml::map::Map,
    old_size: (f32, f32),
    new_size: (f32, f32),
    glow: bool,
) -> omaspec_render::export::folding::Glide {
    use omaspec_render::export::folding;

    use super::morph::{self, Role};

    let size = |(w, h): (f32, f32)| Size::new(w, h);
    let Some(mut morph) = morph::plan_with(
        &morph::sides_of(old),
        &morph::sides_of(new),
        size(old_size),
        size(new_size),
        true,
    ) else {
        return folding::Glide::default();
    };
    morph::gather(&mut morph, old, new);
    let lines = super::wires::plan(old, new, &morph.moves);
    let rect = |r: iced::Rectangle| [r.x, r.y, r.width, r.height];
    let role = |role: Role| match role {
        Role::Stay => folding::Role::Stay,
        Role::Arrive => folding::Role::Arrive,
        Role::Leave => folding::Role::Leave,
    };
    let color = |c: iced::Color| {
        let [r, g, b, _] = c.into_rgba8();
        omaspec_uml::Color(r, g, b)
    };
    let route = |shape: super::wires::Shape| folding::Route {
        points: shape.points,
        heads: shape.heads,
    };
    folding::Glide {
        boxes: morph
            .moves
            .into_iter()
            .map(|step| folding::Moved {
                glow: glow.then(|| super::glow::change(&step)).flatten().map(
                    |change| match change {
                        super::glow::Change::Came => folding::Change::Came,
                        super::glow::Change::Changed => folding::Change::Changed,
                        super::glow::Change::Went => folding::Change::Went,
                    },
                ),
                parts: step.parts,
                from: rect(step.from),
                to: rect(step.to),
                role: role(step.role),
                into: step.into.map(|(was, is)| (rect(was), rect(is))),
            })
            .collect(),
        lines: lines
            .lines
            .into_iter()
            .map(|line| folding::Line {
                from: line.from.map(route),
                to: line.to.map(route),
                colors: [color(line.colors.0), color(line.colors.1)],
                width: line.width,
                dash: line.dash,
                fold: line.fold.map(role),
                words: line.words,
            })
            .collect(),
    }
}
