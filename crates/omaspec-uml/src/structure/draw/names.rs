//! The parts of a structure diagram's picture by name, from the source
//! (docs/adr/0037): an element by its kind and its key (`class-Order`,
//! `package-Shop`), a line's words by what it joins and what it says
//! (`link-Order-Customer-places`), its multiplicities after it
//! (`link-Order-Customer.from`). Nothing is named by its place in the
//! layout; a name given twice takes `-2`. The drawer, its map and its
//! notes name the parts by one table.

use std::collections::HashMap;

use super::super::{Element, Group, Link, Shape};
use crate::svg::{Given, element, push_written};

#[derive(Debug, Default)]
pub(in crate::structure) struct Names {
    nodes: HashMap<Vec<usize>, String>,
    edges: HashMap<(Vec<usize>, usize), String>,
}

impl Names {
    /// Every part of the diagram `root` holds, by its place in its tree:
    /// the elements depth first, then each group's lines in their slots
    /// (its edges, then its crossings), as the scene holds them.
    pub(in crate::structure) fn of(root: &Group) -> Names {
        let mut names = Names::default();
        let mut given = Given::default();
        names.nodes_of(root, &mut Vec::new(), &mut given);
        names.edges_of(root, &mut Vec::new(), &mut given);
        names
    }

    /// The part of the element at `path`; empty where it has none.
    pub(in crate::structure) fn node(&self, path: &[usize]) -> String {
        self.nodes.get(path).cloned().unwrap_or_default()
    }

    /// The part of the words of the line at `slot` of the group at `path`.
    pub(in crate::structure) fn edge(&self, path: &[usize], slot: usize) -> String {
        self.edges
            .get(&(path.to_vec(), slot))
            .cloned()
            .unwrap_or_default()
    }

    /// The part of a line's multiplicity at its start, or at its end.
    pub(in crate::structure) fn card(&self, path: &[usize], slot: usize, at_start: bool) -> String {
        let side = if at_start { "from" } else { "to" };
        format!("{}.{side}", self.edge(path, slot))
    }

    fn nodes_of(&mut self, group: &Group, path: &mut Vec<usize>, given: &mut Given) {
        for (index, node) in group.nodes.iter().enumerate() {
            path.push(index);
            let name = element(kind_word(node.shape), &node.key);
            self.nodes.insert(path.clone(), given.unique(name));
            self.nodes_of(&node.children, path, given);
            path.pop();
        }
    }

    fn edges_of(&mut self, group: &Group, path: &mut Vec<usize>, given: &mut Given) {
        let lines = group
            .edges
            .iter()
            .map(|edge| (vec![edge.from], vec![edge.to], edge))
            .chain(
                group
                    .crossings
                    .iter()
                    .map(|c| (c.from.clone(), c.to.clone(), &c.arrow)),
            );
        for (slot, (from, to, link)) in lines.enumerate() {
            let mut name = String::from("link");
            for end in [&from, &to] {
                name.push('-');
                push_written(&mut name, find(group, end).map_or("", |e| e.key.as_str()));
            }
            let words = said(link);
            if !words.is_empty() {
                name.push('-');
                push_written(&mut name, words);
            }
            self.edges.insert((path.clone(), slot), given.unique(name));
        }
        for (index, node) in group.nodes.iter().enumerate() {
            path.push(index);
            self.edges_of(&node.children, path, given);
            path.pop();
        }
    }
}

/// The element at `within` of `group`, a path down its tree.
fn find<'a>(group: &'a Group, within: &[usize]) -> Option<&'a Element> {
    let (&first, rest) = within.split_first()?;
    let node = group.nodes.get(first)?;
    match rest.is_empty() {
        true => Some(node),
        false => find(&node.children, rest),
    }
}

/// The first line of what a line says.
fn said(link: &Link) -> &str {
    let Some(label) = &link.label else {
        return "";
    };
    let first = label.value.split("\\n").next().unwrap_or_default();
    first.split('\n').next().unwrap_or_default().trim()
}

/// The word a kind of element is named by.
fn kind_word(shape: Shape) -> &'static str {
    match shape {
        Shape::Class => "class",
        Shape::Abstract => "abstract",
        Shape::Interface | Shape::Lollipop => "interface",
        Shape::Enum => "enum",
        Shape::Entity => "entity",
        Shape::Annotation => "annotation",
        Shape::Component => "component",
        Shape::UseCase => "usecase",
        Shape::Actor => "actor",
        Shape::Person => "person",
        Shape::Queue => "queue",
        Shape::Node => "node",
        Shape::Artifact => "artifact",
        Shape::Cloud => "cloud",
        Shape::Database => "database",
        Shape::Folder => "folder",
        Shape::Frame => "frame",
        Shape::Rectangle => "rectangle",
        Shape::Object => "object",
        Shape::Package => "package",
        Shape::File => "file",
        Shape::Card => "card",
        Shape::Storage => "storage",
        Shape::Stack => "stack",
        Shape::Hexagon => "hexagon",
        Shape::Agent => "agent",
        Shape::Note => "note",
        Shape::Point => "point",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_are_named_from_the_source() {
        let crate::Diagram::Structure(d) = crate::parse(
            "@startuml\npackage Shop {\n  class Order\n}\nclass Customer\nOrder \"1\" --> \"*\" Customer : places\nOrder --> Customer : places\ninterface Shop.Pay\n@enduml\n",
        )
        .unwrap() else {
            panic!("a class diagram");
        };
        let n = Names::of(&d.root);
        let mut all: Vec<String> = n.nodes.values().cloned().collect();
        all.sort();
        assert!(all.contains(&"package-Shop".to_owned()), "{all:?}");
        assert!(all.contains(&"class-Order".to_owned()), "{all:?}");
        assert!(all.contains(&"class-Customer".to_owned()), "{all:?}");
        let mut edges: Vec<String> = n.edges.values().cloned().collect();
        edges.sort();
        assert_eq!(
            edges,
            ["link-Order-Customer-places", "link-Order-Customer-places-2"]
        );
        let slot = (0..2).find(|&s| n.edge(&[], s) == "link-Order-Customer-places");
        assert!(slot.is_some(), "{:?}", n.edges);
        assert_eq!(
            n.card(&[], slot.unwrap(), true),
            "link-Order-Customer-places.from"
        );
    }
}
