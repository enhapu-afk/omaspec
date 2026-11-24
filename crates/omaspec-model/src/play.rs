//! A machine as a play shows it: what each option says, what each state is
//! called, and every place the machine can come to from its start, each
//! with its options and where they lead. The editor's play bar and an
//! exported presentation (docs/adr/0031) read the same.

use std::collections::HashMap;

use crate::{Kind, Machine, Run, Taken, Trigger};

/// The most runs [`Graph::of`] visits: a machine that can be in more ways is
/// shown by the first of them, the options past them leading nowhere.
pub const VISITED: usize = 256;

/// Between the arrival of a beat's longest light and the next beat, in
/// milliseconds.
pub const STEP_GAP: u64 = 70;

/// How long a light takes along `points` (diagram units), in
/// milliseconds: longer ways a little longer, never so long that the
/// writer waits.
pub fn travel_millis(points: &[(f32, f32)]) -> u64 {
    let length: f32 = points
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum();
    (280.0 + length * 1.1).clamp(380.0, 980.0) as u64
}

/// A step of a play in time ([`timed`]): the step of what was taken at
/// `index`, when its light sets off after the first one's, and how long
/// it travels, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timed {
    pub index: usize,
    pub at: u64,
    pub took: u64,
}

/// The steps of `taken` in time, as a play shows them, the pane and a
/// presentation alike: the steps of one beat set off at once (the arms of a
/// fork, the starts of a state's regions), the next beat when the longest
/// of them has arrived and [`STEP_GAP`] after; `took` says how long each
/// travels. And when all is through, the last gap with it.
pub fn timed(taken: &[Taken], took: impl Fn(&Taken) -> u64) -> (Vec<Timed>, u64) {
    let mut beats: Vec<u32> = Vec::new();
    for step in taken {
        if !beats.contains(&step.beat) {
            beats.push(step.beat);
        }
    }
    let mut out = Vec::with_capacity(taken.len());
    let mut began = 0;
    for beat in beats {
        let mut longest = 0;
        for (index, step) in taken.iter().enumerate().filter(|(_, s)| s.beat == beat) {
            let travels = took(step);
            longest = longest.max(travels);
            out.push(Timed {
                index,
                at: began,
                took: travels,
            });
        }
        began += longest + STEP_GAP;
    }
    (out, began)
}

impl Machine {
    /// How a play offers the transition `t`: its label, and the state it
    /// leads to.
    pub fn option(&self, t: usize) -> (String, String) {
        let tr = &self.transitions[t];
        (label(&tr.trigger), self.shown_name(tr.to))
    }

    /// A state as its box shows it (its long name, if it has one), or what
    /// a pseudo-state is.
    pub fn shown_name(&self, state: usize) -> String {
        let s = &self.states[state];
        let what = match s.kind {
            Kind::End => "end",
            Kind::History => "history",
            Kind::DeepHistory => "deep history",
            _ => return s.shown().to_owned(),
        };
        match s.parent {
            Some(p) => format!("{what} of {}", self.states[p].shown()),
            None => what.to_owned(),
        }
    }

    /// Where a machine in `states` is, as a play says it: their names, each
    /// once in a row.
    pub fn shown_at(&self, states: &[usize]) -> String {
        let mut at: Vec<String> = states
            .iter()
            .map(|&s| self.shown_name(s))
            .filter(|name| !name.is_empty())
            .collect();
        at.dedup();
        at.join(", ")
    }
}

/// An option as its line says it: `event [guard] / action`, or `go on` for
/// a line without event or guard.
pub fn label(trigger: &Trigger) -> String {
    let mut out = trigger.events.join(", ");
    if let Some(guard) = &trigger.guard {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&format!("[{guard}]"));
    }
    if out.is_empty() {
        out.push_str("go on");
    }
    if let Some(action) = &trigger.action {
        out.push_str(&format!(" / {action}"));
    }
    out
}

/// Every place a machine comes to from its start, as far as [`VISITED`]
/// runs: the first is the start.
#[derive(Debug, Clone)]
pub struct Graph {
    /// Each place, its run.
    pub runs: Vec<Run>,
    /// What the start took by itself before the first place.
    pub begun: Vec<Taken>,
    /// For each place, its options in the order the machine offers them:
    /// the transition, the place it leads to (`None` past the runs
    /// visited), and what it takes, the steps that follow by themselves
    /// with it.
    pub options: Vec<Vec<Way>>,
}

/// One option of a place in a [`Graph`].
#[derive(Debug, Clone)]
pub struct Way {
    pub transition: usize,
    pub next: Option<usize>,
    pub taken: Vec<Taken>,
}

impl Graph {
    /// The places `m` comes to from its start; `None` without a start.
    pub fn of(m: &Machine) -> Option<Graph> {
        let (start, begun) = Run::start(m)?;
        let mut found = HashMap::from([(start.clone(), 0)]);
        let mut runs = vec![start];
        let mut options = Vec::new();
        let mut at = 0;
        while at < runs.len() {
            let mut ways = Vec::new();
            for transition in runs[at].options(m) {
                let mut after = runs[at].clone();
                let taken = after.take(m, transition);
                let next = match found.get(&after) {
                    Some(&place) => Some(place),
                    None if runs.len() < VISITED => {
                        found.insert(after.clone(), runs.len());
                        runs.push(after);
                        Some(runs.len() - 1)
                    }
                    None => None,
                };
                ways.push(Way {
                    transition,
                    next,
                    taken,
                });
            }
            options.push(ways);
            at += 1;
        }
        Some(Graph {
            runs,
            begun,
            options,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omaspec_core::Language;

    #[test]
    fn every_place_of_a_door_is_found_with_where_its_options_lead() {
        let source = "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close [nobody inside] / beep\nClosed --> [*] : remove\n@enduml\n";
        let m = Machine::read(source, Language::PlantUml).unwrap();
        let graph = Graph::of(&m).unwrap();
        let names =
            |run: &Run| -> Vec<String> { run.active().iter().map(|&s| m.shown_name(s)).collect() };
        assert_eq!(names(&graph.runs[0]), ["Closed"]);
        assert_eq!(graph.begun.len(), 1);
        let labels: Vec<(String, String)> = graph.options[0]
            .iter()
            .map(|way| m.option(way.transition))
            .collect();
        assert_eq!(
            labels,
            [
                ("open".to_owned(), "Open".to_owned()),
                ("remove".to_owned(), "end".to_owned())
            ]
        );
        let open = graph.options[0][0].next.unwrap();
        assert_eq!(names(&graph.runs[open]), ["Open"]);
        let back = &graph.options[open][0];
        assert_eq!(m.option(back.transition).0, "close [nobody inside] / beep");
        assert_eq!(back.next, Some(0));
        assert_eq!(back.taken.len(), 1);
    }

    /// A state is a halt, so regions side by side whose states go on
    /// without an event make a place of each meeting of theirs: three
    /// regions of three such states stay well within what a graph visits
    /// (and a trace follows, `MAX_WAYS`).
    #[test]
    fn regions_of_halts_stay_within_what_is_visited() {
        let source = "@startuml\n[*] --> P\nstate P {\n  [*] --> A1\n  A1 --> A2\n  A2 --> A3\n  A3 --> [*]\n  --\n  [*] --> B1\n  B1 --> B2\n  B2 --> B3\n  B3 --> [*]\n  --\n  [*] --> C1\n  C1 --> C2\n  C2 --> C3\n  C3 --> [*]\n}\nP --> Done\n@enduml\n";
        let m = Machine::read(source, Language::PlantUml).unwrap();
        let graph = Graph::of(&m).unwrap();
        // Four places in each region; all three at their end is Done.
        assert_eq!(graph.runs.len(), 4 * 4 * 4);
        assert!(graph.runs.len() < VISITED);
        assert!(graph.options.iter().flatten().all(|way| way.next.is_some()));
    }

    /// The arms of a fork set off at once; what follows them waits for the
    /// longest and the gap.
    #[test]
    fn one_beat_sets_off_at_once_the_next_after_the_longest() {
        let taken = |transition, beat| Taken {
            transition,
            left: Vec::new(),
            entered: Vec::new(),
            beat,
        };
        let steps = [taken(0, 0), taken(1, 1), taken(2, 1), taken(3, 2)];
        let (timed, total) = timed(&steps, |step| 100 * (step.transition as u64 + 1));
        let at: Vec<(usize, u64, u64)> = timed.iter().map(|t| (t.index, t.at, t.took)).collect();
        assert_eq!(
            at,
            [(0, 0, 100), (1, 170, 200), (2, 170, 300), (3, 540, 400)]
        );
        assert_eq!(total, 1010);
        assert_eq!(travel_millis(&[(0.0, 0.0), (0.0, 10.0)]), 380);
        assert_eq!(travel_millis(&[(0.0, 0.0), (0.0, 2000.0)]), 980);
    }
}
