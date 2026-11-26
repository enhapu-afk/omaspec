//! The parts of a state machine's picture by name, from the source
//! (docs/adr/0037): a state by its key (`state-Idle`), a mark by its
//! kind and the state it is in (`start-Brewing`, `end` at the top), a
//! region by its state and its number (`region-Brewing-2`), a
//! transition by its ends and its event (`transition-Idle-Busy-start`).
//! Nothing is named by its place in the layout; a name given twice takes
//! `-2`. The drawer, its map and its notes name the parts by one table.

use std::collections::HashMap;

use super::super::{Container, Kind, Node, Transition};
use crate::svg::{Given, element, push_written};

#[derive(Debug, Default)]
pub(in crate::state) struct Names {
    nodes: HashMap<Vec<usize>, String>,
    edges: HashMap<(Vec<usize>, usize), String>,
}

impl Names {
    /// Every part of the machine `root` holds, by its place in its tree:
    /// the nodes depth first, then each container's transitions in their
    /// slots (its edges, then its crossings), as the scene holds them.
    pub(in crate::state) fn of(root: &Container) -> Names {
        let mut names = Names::default();
        let mut given = Given::default();
        names.nodes_of(root, &mut Vec::new(), None, &mut given);
        names.edges_of(root, &mut Vec::new(), &mut given);
        names
    }

    /// The part of the node at `path`; empty where it has none.
    pub(in crate::state) fn node(&self, path: &[usize]) -> String {
        self.nodes.get(path).cloned().unwrap_or_default()
    }

    /// The part of the transition at `slot` of the container at `path`,
    /// which its words are drawn in.
    pub(in crate::state) fn edge(&self, path: &[usize], slot: usize) -> String {
        self.edges
            .get(&(path.to_vec(), slot))
            .cloned()
            .unwrap_or_default()
    }

    /// Names the nodes of `container`, at `path`, which the state named
    /// `owner` (as an id writes it) holds.
    fn nodes_of(
        &mut self,
        container: &Container,
        path: &mut Vec<usize>,
        owner: Option<&str>,
        given: &mut Given,
    ) {
        let mut regions = 0;
        for (index, node) in container.nodes.iter().enumerate() {
            path.push(index);
            let inside = if node.kind == Kind::Region {
                regions += 1;
                let of = format!("{}.{regions}", owner.unwrap_or_default());
                let name = element("region", "");
                let name = match owner {
                    Some(owner) => format!("{name}-{owner}-{regions}"),
                    None => format!("{name}-{regions}"),
                };
                self.nodes.insert(path.clone(), given.unique(name));
                Some(of)
            } else {
                let name = match node.key.is_empty() {
                    true => match owner {
                        Some(owner) => format!("{}-{owner}", kind_word(node.kind)),
                        None => kind_word(node.kind).to_owned(),
                    },
                    false => element(kind_word(node.kind), &node.key),
                };
                self.nodes.insert(path.clone(), given.unique(name));
                (!node.key.is_empty()).then(|| written(&node.key))
            };
            let owner = inside.as_deref().or(owner);
            self.nodes_of(&node.children, path, owner, given);
            path.pop();
        }
    }

    /// Names the transitions of `container`, at `path`, and of every one
    /// inside it.
    fn edges_of(&mut self, container: &Container, path: &mut Vec<usize>, given: &mut Given) {
        let end = |names: &Names, at: &[usize], within: &[usize]| {
            let whole = [at, within].concat();
            let node = names.node(&whole);
            match find(container, within) {
                Some(found) if !found.key.is_empty() => written(&found.key),
                _ => node,
            }
        };
        let edges = container
            .edges
            .iter()
            .map(|edge| (vec![edge.from], vec![edge.to], edge))
            .chain(
                container
                    .crossings
                    .iter()
                    .map(|c| (c.from.clone(), c.to.clone(), &c.arrow)),
            );
        for (slot, (from, to, arrow)) in edges.enumerate().collect::<Vec<_>>() {
            let mut name = format!(
                "transition-{}-{}",
                end(self, path, &from),
                end(self, path, &to)
            );
            let event = event_of(arrow);
            if !event.is_empty() {
                name.push('-');
                push_written(&mut name, event);
            }
            self.edges.insert((path.clone(), slot), given.unique(name));
        }
        for (index, node) in container.nodes.iter().enumerate() {
            path.push(index);
            self.edges_of(&node.children, path, given);
            path.pop();
        }
    }
}

/// The node at `within` of `container`, a path down its tree.
fn find<'a>(container: &'a Container, within: &[usize]) -> Option<&'a Node> {
    let (&first, rest) = within.split_first()?;
    let node = container.nodes.get(first)?;
    match rest.is_empty() {
        true => Some(node),
        false => find(&node.children, rest),
    }
}

/// `name` as an id holds it ([`element`]).
fn written(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    push_written(&mut out, name);
    out
}

/// What a transition says it happens on: its label's first line before
/// its guard and its action (`start [tank empty] / beep` is `start`).
fn event_of(arrow: &Transition) -> &str {
    let Some(label) = &arrow.label else {
        return "";
    };
    let first = label.value.split("\\n").next().unwrap_or_default();
    let first = first.split('\n').next().unwrap_or_default();
    first.split(['[', '/']).next().unwrap_or_default().trim()
}

/// The word a kind of node is named by.
fn kind_word(kind: Kind) -> &'static str {
    match kind {
        Kind::State => "state",
        Kind::Choice => "choice",
        Kind::Fork => "fork",
        Kind::Join => "join",
        Kind::History => "history",
        Kind::DeepHistory => "deep-history",
        Kind::Start => "start",
        Kind::End => "end",
        Kind::Region => "region",
        Kind::Note => "note",
        Kind::Data => "data",
        Kind::Entry => "entry",
        Kind::Exit => "exit",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Names {
        let crate::Diagram::State(d) = crate::parse(source).unwrap() else {
            panic!("a state diagram");
        };
        Names::of(&d.root)
    }

    #[test]
    fn parts_are_named_from_the_source() {
        let n = names(
            "@startuml\n[*] --> Idle\nIdle --> Busy : start [full] / beep\nIdle --> Busy : start\nstate Busy {\n  [*] --> Grind\n  Grind --> [*]\n}\nstate \"Order parts\" as Order\nBusy --> Order\n@enduml\n",
        );
        let all: Vec<String> = {
            let mut v: Vec<String> = n.nodes.values().cloned().collect();
            v.sort();
            v
        };
        assert_eq!(
            all,
            [
                "end-Busy",
                "start",
                "start-Busy",
                "state-Busy",
                "state-Grind",
                "state-Idle",
                "state-Order"
            ]
        );
        let mut edges: Vec<String> = n.edges.values().cloned().collect();
        edges.sort();
        assert_eq!(
            edges,
            [
                "transition-Busy-Order",
                "transition-Grind-end-Busy",
                "transition-Idle-Busy-start",
                "transition-Idle-Busy-start-2",
                "transition-start-Busy-Grind",
                "transition-start-Idle"
            ]
        );
    }

    #[test]
    fn regions_and_what_they_hold_are_named_by_their_state() {
        let n = names(
            "@startuml\nstate On {\n  [*] --> A\n  --\n  [*] --> B\n}\n[*] --> On\n@enduml\n",
        );
        let mut all: Vec<String> = n.nodes.values().cloned().collect();
        all.sort();
        assert!(all.contains(&"region-On-1".to_owned()), "{all:?}");
        assert!(all.contains(&"region-On-2".to_owned()), "{all:?}");
        assert!(all.contains(&"start-On.1".to_owned()), "{all:?}");
        assert!(all.contains(&"start-On.2".to_owned()), "{all:?}");
    }
}
