//! How a new picture comes on screen: one small state machine, drawn in
//! `arrival.puml` beside this file (docs/adr/0030). The pane moves it only
//! through [`Paper::arrive`]; in the tests each move is written to a trace
//! and held against that diagram by Omaspec's own trace check, so the code
//! and the diagram cannot drift apart.

use super::{Fade, Paper, Previous};

/// Where the arrival of a new picture is.
pub(super) enum Arrival {
    /// The picture on screen stands.
    Still,
    /// A new diagram is drawn. The picture on screen stays as it was, with
    /// what a glide reads of it, until the new one's overview is
    /// rasterized; none on the way from an older drawing, which starts
    /// there ([`Paper::glide_from`]).
    Coming(Option<Previous>),
    /// The overview is in; the sheets of parts of both pictures are on
    /// their way (docs/adr/0029).
    Sheets(Previous),
    /// One picture fades into the other.
    Fading(Fade),
    /// The boxes glide; a newer diagram drawn meanwhile waits for the end.
    Gliding(Fade, Next),
}

/// A newer diagram, drawn while a glide runs.
pub(super) enum Next {
    /// None is.
    Alone,
    /// One is drawn; its overview is on its way.
    Following(Option<Previous>),
    /// Its overview is in; it comes once the glide has ended.
    Waiting(Option<Previous>),
}

/// What moves an arrival on, as `arrival.puml` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Event {
    /// A new diagram is drawn.
    Drawn,
    /// Its overview is rasterized.
    Overview,
    /// Its overview could not be.
    NoOverview,
    /// A sheet of parts came.
    Sheet,
    /// A sheet of parts could not be drawn.
    NoSheet,
    /// The fade or the glide on screen has ended.
    Faded,
    /// The pane is hidden.
    Released,
}

impl Event {
    /// Its name in `arrival.puml`.
    pub fn name(self) -> &'static str {
        match self {
            Event::Drawn => "drawn",
            Event::Overview => "overview",
            Event::NoOverview => "no overview",
            Event::Sheet => "sheet",
            Event::NoSheet => "no sheet",
            Event::Faded => "faded",
            Event::Released => "released",
        }
    }
}

impl Arrival {
    /// Its state in `arrival.puml`, the innermost.
    pub fn state(&self) -> &'static str {
        match self {
            Arrival::Still => "Still",
            Arrival::Coming(_) => "Coming",
            Arrival::Sheets(_) => "Sheets",
            Arrival::Fading(_) => "Fading",
            Arrival::Gliding(_, Next::Alone) => "Alone",
            Arrival::Gliding(_, Next::Following(_)) => "Following",
            Arrival::Gliding(_, Next::Waiting(_)) => "Waiting",
        }
    }

    /// The fade or the glide on screen.
    pub fn fade(&self) -> Option<&Fade> {
        match self {
            Arrival::Fading(fade) | Arrival::Gliding(fade, _) => Some(fade),
            _ => None,
        }
    }

    pub fn fade_mut(&mut self) -> Option<&mut Fade> {
        match self {
            Arrival::Fading(fade) | Arrival::Gliding(fade, _) => Some(fade),
            _ => None,
        }
    }

    /// Whether a new diagram is drawn whose fade has not started.
    pub fn coming(&self) -> bool {
        matches!(
            self,
            Arrival::Coming(_)
                | Arrival::Sheets(_)
                | Arrival::Gliding(_, Next::Following(_) | Next::Waiting(_))
        )
    }

    /// The pictures on screen when the diagram that is coming was drawn.
    pub fn previous(&self) -> Option<&Previous> {
        match self {
            Arrival::Coming(previous)
            | Arrival::Gliding(_, Next::Following(previous) | Next::Waiting(previous)) => {
                previous.as_ref()
            }
            Arrival::Sheets(previous) => Some(previous),
            _ => None,
        }
    }

    pub fn previous_mut(&mut self) -> Option<&mut Previous> {
        match self {
            Arrival::Coming(previous)
            | Arrival::Gliding(_, Next::Following(previous) | Next::Waiting(previous)) => {
                previous.as_mut()
            }
            Arrival::Sheets(previous) => Some(previous),
            _ => None,
        }
    }

    /// The picture still on screen while the new one is on its way, and
    /// nothing moves: what the pane draws in its place.
    pub fn held(&self) -> Option<&Previous> {
        match self {
            Arrival::Coming(Some(previous)) | Arrival::Sheets(previous) => Some(previous),
            _ => None,
        }
    }

    /// Forgets the pictures the coming diagram would fade or glide from:
    /// it comes from nothing.
    pub fn forget_previous(&mut self) {
        if let Arrival::Coming(previous)
        | Arrival::Gliding(_, Next::Following(previous) | Next::Waiting(previous)) = self
        {
            *previous = None;
        }
    }
}

impl Paper {
    /// Moves the arrival on by `event`, to `to`. A late frame names the
    /// move ([`super::frames`]).
    pub(super) fn arrive(&mut self, event: Event, to: Arrival) {
        super::frames::note(|| format!("{} → {}", event.name(), to.state()));
        #[cfg(test)]
        self.trace.note(event, &to);
        self.arrival = to;
    }

    /// Takes the arrival out, to move it on; it is [`Arrival::Still`] until
    /// [`Self::arrive`] puts the next one in, or the same one is put back.
    pub(super) fn take_arrival(&mut self) -> Arrival {
        std::mem::replace(&mut self.arrival, Arrival::Still)
    }
}

/// The moves of a pane's arrival as a trace, each held against
/// `arrival.puml` as it is made. With `OMASPEC_ARRIVAL_TRACES` set to a
/// file, each pane's trace is added to it when the pane goes, for
/// `omaspec coverage` to tell which transitions the tests take.
#[cfg(test)]
pub(super) struct Trace {
    /// The pane, among those of the tests.
    object: String,
    lines: String,
}

#[cfg(test)]
impl Default for Trace {
    fn default() -> Self {
        static PANES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let pane = PANES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Trace {
            object: format!("pane {pane}"),
            lines: String::new(),
        }
    }
}

#[cfg(test)]
impl Trace {
    fn note(&mut self, event: Event, to: &Arrival) {
        use std::fmt::Write;
        let _ = writeln!(
            self.lines,
            r#"{{"object":"{}","machine":"Arrival","event":"{}","state":"{}"}}"#,
            self.object,
            event.name(),
            to.state()
        );
        let findings = omaspec_model::check_trace(&self.lines, &tests::SPECS);
        assert!(
            findings.is_empty(),
            "the pane's arrival left arrival.puml:\n{}{findings:#?}",
            self.lines
        );
    }
}

#[cfg(test)]
impl Trace {
    /// The trace so far, one JSON object a line.
    pub fn lines(&self) -> &str {
        &self.lines
    }
}

#[cfg(test)]
impl Drop for Trace {
    fn drop(&mut self) {
        use std::io::Write;
        let Some(path) = std::env::var_os("OMASPEC_ARRIVAL_TRACES") else {
            return;
        };
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = file.write_all(self.lines.as_bytes());
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::LazyLock;

    use omaspec_core::Language;
    use omaspec_model::{Spec, Specs};

    use super::*;

    pub const DIAGRAM: &str = include_str!("arrival.puml");

    pub static SPECS: LazyLock<Specs> = LazyLock::new(|| {
        Specs::new(vec![
            Spec::read(DIAGRAM, Language::PlantUml, "arrival.puml").expect("a state machine"),
        ])
    });

    /// The diagram is clean, and its states and events are the code's.
    #[test]
    fn the_diagram_is_the_code_s_machine() {
        let findings = omaspec_model::findings(DIAGRAM, Language::PlantUml);
        assert!(findings.is_empty(), "{findings:#?}");
        let machine = &SPECS.named(&["Arrival"])[0].machine;
        let mut states: Vec<&str> = machine
            .states
            .iter()
            .filter(|state| state.kind == omaspec_model::Kind::State)
            .map(|state| state.name.as_str())
            .collect();
        states.sort_unstable();
        let mut ours = vec![
            "Still",
            "Coming",
            "Sheets",
            "Fading",
            "Gliding",
            "Alone",
            "Following",
            "Waiting",
        ];
        ours.sort_unstable();
        assert_eq!(states, ours);
        let mut events: Vec<String> = machine
            .transitions
            .iter()
            .flat_map(|t| t.trigger.events.iter().cloned())
            .collect();
        events.sort_unstable();
        events.dedup();
        let mut named: Vec<String> = [
            Event::Drawn,
            Event::Overview,
            Event::NoOverview,
            Event::Sheet,
            Event::NoSheet,
            Event::Faded,
            Event::Released,
        ]
        .map(|event| event.name().to_owned())
        .to_vec();
        named.sort_unstable();
        assert_eq!(events, named);
    }

    mod tour {
        use iced::Size;

        use super::super::super::testing::{answer, drawn};
        use super::super::super::{Gesture, Message, Paper, raster};
        use super::*;

        const FIRST: &str = "@startuml\n[*] --> A\nA --> B\n@enduml\n";
        /// The same boxes elsewhere: they glide.
        const MOVED: &str = "@startuml\n[*] --> A\nA --> B\nB --> C\nC --> D\n@enduml\n";
        const MORE: &str =
            "@startuml\n[*] --> A\nA --> B\nB --> C\nC --> D\nD --> E\nE --> F\n@enduml\n";

        fn pane() -> Paper {
            let mut paper = Paper {
                by_hand: true,
                ..Paper::default()
            };
            let _ = paper.update(Message::Viewport(Size::new(400.0, 300.0)));
            paper
        }

        fn show(paper: &mut Paper, source: &str) {
            let mut reply = answer(
                1,
                paper.pretend_next(),
                Ok(drawn(source, Language::PlantUml)),
            );
            reply.source = source.into();
            reply.language = Language::PlantUml;
            let _ = paper.rendered(reply);
        }

        /// The overview alone, its sheets not drawn.
        fn overview(paper: &mut Paper) {
            let tree = std::sync::Arc::clone(paper.shown_tree().unwrap());
            let raster = raster::render(&tree, paper.overview.asked.unwrap());
            let _ = paper.update(Message::Overview(paper.overview.seq(), raster));
        }

        fn no_overview(paper: &mut Paper) {
            let _ = paper.update(Message::Overview(paper.overview.seq(), None));
        }

        /// The old picture's sheet, or the new one's.
        fn sheet(paper: &mut Paper, old: bool) {
            let (sheet, plan) = if old {
                let previous = paper.arrival.previous().unwrap();
                (
                    previous.sheet.clone().unwrap(),
                    paper.bare_old.asked.unwrap(),
                )
            } else {
                let drawn = &paper.diagram.as_ref().unwrap().drawn;
                (
                    super::super::super::sheet::Sheet::of(drawn).unwrap(),
                    paper.bare_new.asked.unwrap(),
                )
            };
            let message = if old {
                Message::BareOld(paper.bare_old.seq(), sheet.draw(plan))
            } else {
                Message::BareNew(paper.bare_new.seq(), sheet.draw(plan))
            };
            let _ = paper.update(message);
        }

        fn faded(paper: &mut Paper) {
            let _ = paper.update(Message::Faded);
        }

        /// Alt held over a box: it is lit, and nothing glides.
        fn hold(paper: &mut Paper) {
            let at = paper.screen_of_box("A");
            let _ = paper.update(Message::Hand(Gesture::Hover(Some(at))));
        }

        fn at(paper: &Paper, state: &str) {
            assert_eq!(paper.arrival.state(), state, "{}", paper.trace.lines());
        }

        /// A pane in `FIRST`, still, then `MOVED` drawn and its overview
        /// in: it waits for the sheets.
        fn sheets() -> Paper {
            let mut paper = pane();
            show(&mut paper, FIRST);
            overview(&mut paper);
            faded(&mut paper);
            show(&mut paper, MOVED);
            overview(&mut paper);
            at(&paper, "Sheets");
            paper
        }

        /// A pane gliding from `FIRST` to `MOVED`.
        fn gliding() -> Paper {
            let mut paper = sheets();
            sheet(&mut paper, true);
            sheet(&mut paper, false);
            at(&paper, "Alone");
            paper
        }

        /// Every transition of `arrival.puml` is one the pane takes: the
        /// way each takes is walked here, and Omaspec's coverage of the
        /// walks is the whole diagram.
        #[test]
        fn the_pane_takes_every_transition_of_its_diagram() {
            let mut walks: Vec<String> = Vec::new();
            let mut walk = |paper: Paper| walks.push(paper.trace.lines().to_owned());

            // The first picture fades in; a new one fails, another comes
            // twice before its overview, and fades too.
            let mut paper = pane();
            show(&mut paper, FIRST);
            at(&paper, "Coming");
            overview(&mut paper);
            at(&paper, "Fading");
            faded(&mut paper);
            at(&paper, "Still");
            show(&mut paper, FIRST);
            no_overview(&mut paper);
            at(&paper, "Still");
            show(&mut paper, FIRST);
            show(&mut paper, FIRST);
            overview(&mut paper);
            at(&paper, "Fading");
            // A short fade gives way to the next diagram at once.
            show(&mut paper, FIRST);
            at(&paper, "Coming");
            no_overview(&mut paper);
            walk(paper);

            // Waiting for the sheets: an overview again, a newer diagram,
            // a failed overview.
            let mut paper = sheets();
            overview(&mut paper);
            at(&paper, "Sheets");
            show(&mut paper, MOVED);
            at(&paper, "Coming");
            overview(&mut paper);
            at(&paper, "Sheets");
            no_overview(&mut paper);
            at(&paper, "Still");
            walk(paper);

            // One sheet, then the other: the glide, to its end.
            let mut paper = sheets();
            sheet(&mut paper, true);
            at(&paper, "Sheets");
            sheet(&mut paper, false);
            at(&paper, "Alone");
            faded(&mut paper);
            at(&paper, "Still");
            walk(paper);

            // A sheet that could not be drawn: the pictures fade; the pane
            // hidden while they do.
            let mut paper = sheets();
            let _ = paper.update(Message::BareOld(paper.bare_old.seq(), None));
            at(&paper, "Fading");
            paper.release();
            at(&paper, "Still");
            walk(paper);

            // The pane hidden while a new diagram's overview is drawn.
            let mut paper = pane();
            show(&mut paper, FIRST);
            paper.release();
            at(&paper, "Still");
            walk(paper);

            // The pane hidden while the sheets come.
            let mut paper = sheets();
            paper.release();
            at(&paper, "Still");
            walk(paper);

            // A box held while the sheets come: they fade after all, when
            // the overview comes again or when the sheets do.
            let mut paper = sheets();
            hold(&mut paper);
            overview(&mut paper);
            at(&paper, "Fading");
            walk(paper);
            let mut paper = sheets();
            hold(&mut paper);
            sheet(&mut paper, true);
            sheet(&mut paper, false);
            at(&paper, "Fading");
            walk(paper);

            // Newer diagrams while a glide runs: they wait for its end.
            let mut paper = gliding();
            show(&mut paper, MORE);
            at(&paper, "Following");
            show(&mut paper, MORE);
            at(&paper, "Following");
            overview(&mut paper);
            at(&paper, "Waiting");
            show(&mut paper, MORE);
            at(&paper, "Following");
            no_overview(&mut paper);
            at(&paper, "Alone");
            show(&mut paper, MORE);
            overview(&mut paper);
            at(&paper, "Waiting");
            no_overview(&mut paper);
            at(&paper, "Alone");
            show(&mut paper, MORE);
            faded(&mut paper);
            at(&paper, "Coming");
            walk(paper);

            // One whose overview is in glides on when the glide ends; one
            // that moved nothing fades.
            let mut paper = gliding();
            show(&mut paper, MORE);
            overview(&mut paper);
            faded(&mut paper);
            at(&paper, "Sheets");
            walk(paper);
            let mut paper = gliding();
            show(&mut paper, MOVED);
            overview(&mut paper);
            faded(&mut paper);
            at(&paper, "Fading");
            walk(paper);

            // The pane hidden while a glide runs.
            let mut paper = gliding();
            paper.release();
            at(&paper, "Still");
            walk(paper);

            let spec = &SPECS.named(&["Arrival"])[0];
            let traces: Vec<&str> = walks.iter().map(String::as_str).collect();
            let coverage = omaspec_model::coverage(spec, &traces);
            let never: Vec<String> = coverage
                .transitions
                .iter()
                .zip(&spec.machine.transitions)
                .filter(|(taken, _)| **taken == 0)
                .map(|(_, t)| {
                    let name = |state: usize| spec.machine.states[state].name.clone();
                    format!(
                        "{} --> {} : {:?}",
                        name(t.from),
                        name(t.to),
                        t.trigger.events
                    )
                })
                .collect();
            assert!(never.is_empty(), "never taken: {never:#?}");
            assert_eq!(coverage.deviations, 0);
        }
    }
}
