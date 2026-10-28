//! PlantUML state diagrams, as the native engine parses them.

use std::collections::HashMap;

use omagram_uml::Diagram;
use omagram_uml::state::{Container, Kind as NodeKind, Transition as Arrow};

use crate::{Kind, Machine, State, Transition, Trigger};

/// The machine of a PlantUML state diagram; `None` if the engine declines
/// the diagram or it is no state diagram.
pub fn read(code: &str) -> Option<Machine> {
    let Ok(Diagram::State(diagram)) = omagram_uml::parse(code) else {
        return None;
    };
    let mut reader = Reader::default();
    reader.container(&diagram.root, &[], None, 0);
    reader.links(&diagram.root, &[]);
    Some(reader.machine)
}

#[derive(Default)]
struct Reader {
    machine: Machine,
    /// Each node's state, by its path from the diagram down.
    states: HashMap<Vec<usize>, usize>,
}

impl Reader {
    /// Adds the states of `container`, at `path`, held by `parent`.
    fn container(
        &mut self,
        container: &Container,
        path: &[usize],
        parent: Option<usize>,
        region: usize,
    ) {
        let mut regions = 0;
        for (index, node) in container.nodes.iter().enumerate() {
            let here = [path, &[index]].concat();
            let kind = match node.kind {
                // A concurrent region of `parent`: no state of its own.
                NodeKind::Region => {
                    self.container(&node.children, &here, parent, regions);
                    regions += 1;
                    continue;
                }
                NodeKind::State => Kind::State,
                NodeKind::Start => Kind::Start,
                NodeKind::End => Kind::End,
                NodeKind::Choice => Kind::Choice,
                NodeKind::Fork => Kind::Fork,
                NodeKind::Join => Kind::Join,
                NodeKind::History => Kind::History,
                NodeKind::DeepHistory => Kind::DeepHistory,
            };
            let id = self.machine.states.len();
            self.machine.states.push(State {
                name: node.key.clone(),
                kind,
                parent,
                region,
                line: node.line,
            });
            self.states.insert(here.clone(), id);
            self.container(&node.children, &here, Some(id), 0);
        }
    }

    /// Adds the transitions of `container` at `path` and of every
    /// container inside it.
    fn links(&mut self, container: &Container, path: &[usize]) {
        for edge in &container.edges {
            let from = [path, &[edge.from]].concat();
            let to = [path, &[edge.to]].concat();
            self.link(&from, &to, edge);
        }
        for crossing in &container.crossings {
            let from = [path, &crossing.from].concat();
            let to = [path, &crossing.to].concat();
            self.link(&from, &to, &crossing.arrow);
        }
        for (index, node) in container.nodes.iter().enumerate() {
            self.links(&node.children, &[path, &[index]].concat());
        }
    }

    fn link(&mut self, from: &[usize], to: &[usize], arrow: &Arrow) {
        let (Some(&from), Some(&to)) = (self.states.get(from), self.states.get(to)) else {
            return;
        };
        self.machine.transitions.push(Transition {
            from,
            to,
            trigger: arrow
                .label
                .as_ref()
                .map_or_else(Trigger::default, |l| Trigger::parse(&l.value)),
            line: arrow.line,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nesting_regions_and_crossings() {
        let m = read(
            "@startuml\n[*] --> Idle\nstate Busy {\n  [*] --> Work\n  --\n  [*] --> Watch\n}\nIdle --> Busy : job [ready] / start\nWork --> Idle : abort\n@enduml\n",
        )
        .unwrap();
        let name = |i: usize| m.states[i].name.as_str();
        let busy = m.states.iter().position(|s| s.name == "Busy").unwrap();
        let work = m.states.iter().position(|s| s.name == "Work").unwrap();
        let watch = m.states.iter().position(|s| s.name == "Watch").unwrap();
        assert_eq!(m.states[work].parent, Some(busy));
        assert_eq!((m.states[work].region, m.states[watch].region), (0, 1));
        assert!(m.is_composite(busy));

        let arrows: Vec<(&str, &str, usize)> = m
            .transitions
            .iter()
            .map(|t| (name(t.from), name(t.to), t.line))
            .collect();
        assert!(arrows.contains(&("Idle", "Busy", 7)));
        // From inside `Busy` out to `Idle`: a crossing.
        assert!(arrows.contains(&("Work", "Idle", 8)));
        let job = m.transitions.iter().find(|t| t.line == 7).unwrap();
        assert_eq!(job.trigger.events, ["job"]);
        assert_eq!(job.trigger.guard.as_deref(), Some("ready"));
        assert_eq!(job.trigger.action.as_deref(), Some("start"));
        // Each region has its own start.
        let starts = m
            .states
            .iter()
            .filter(|s| s.kind == Kind::Start && s.parent == Some(busy))
            .count();
        assert_eq!(starts, 2);
    }

    #[test]
    fn other_diagrams_are_not_read() {
        assert!(read("@startuml\nA -> B\n@enduml\n").is_none());
        assert!(read("@startuml\n!include x.puml\n[*] --> A\n@enduml\n").is_none());
    }
}
