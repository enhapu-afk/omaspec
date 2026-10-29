//! Mermaid state diagrams, as Omaspec reads them to draw them.

use omaspec_uml::state::mermaid::{self, Kind as ChartKind};

use crate::{Kind, Machine, State, Transition, Trigger};

/// The machine of a Mermaid state diagram; `None` if the reader declines
/// the diagram or it is no state diagram.
pub fn read(code: &str) -> Option<Machine> {
    let chart = mermaid::read(code)?;
    let states = chart
        .states
        .into_iter()
        .map(|s| State {
            name: s.id,
            kind: match s.kind {
                ChartKind::State => Kind::State,
                ChartKind::Start => Kind::Start,
                ChartKind::End => Kind::End,
                ChartKind::Choice => Kind::Choice,
                ChartKind::Fork => Kind::Fork,
                ChartKind::Join => Kind::Join,
            },
            parent: s.parent,
            region: s.region,
            line: s.line,
        })
        .collect();
    let transitions = chart
        .transitions
        .iter()
        .map(|t| Transition {
            from: t.from,
            to: t.to,
            trigger: Trigger::parse(&t.label),
            line: t.line,
        })
        .collect();
    Some(Machine {
        states,
        transitions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_chart_the_drawer_draws() {
        let m = read(
            "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy: job [ready] / start\n  state Busy {\n    [*] --> Work\n    --\n    [*] --> Watch\n  }\n  state pick <<choice>>\n  Work --> pick\n",
        )
        .unwrap();
        let find = |n: &str| m.states.iter().position(|s| s.name == n).unwrap();
        let (idle, busy, work, watch) = (find("Idle"), find("Busy"), find("Work"), find("Watch"));
        assert_eq!(m.states[work].parent, Some(busy));
        assert_eq!((m.states[work].region, m.states[watch].region), (0, 1));
        assert_eq!(m.states[find("pick")].kind, Kind::Choice);
        let job = &m.transitions[1];
        assert_eq!((job.from, job.to, job.line), (idle, busy, 2));
        assert_eq!(job.trigger.guard.as_deref(), Some("ready"));
        assert!(read("stateDiagram\nA -> B").is_none());
    }
}
