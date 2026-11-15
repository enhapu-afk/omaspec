//! A machine as a play shows it: what each option says, what each state is
//! called, and every place the machine can come to from its start, each
//! with its options and where they lead. The editor's play bar and an
//! exported presentation (docs/adr/0031) read the same.

use std::collections::HashMap;

use crate::{Kind, Machine, Run, Taken, Trigger};

/// The most runs [`Graph::of`] visits: a machine that can be in more ways is
/// shown by the first of them, the options past them leading nowhere.
pub const VISITED: usize = 256;

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
}
