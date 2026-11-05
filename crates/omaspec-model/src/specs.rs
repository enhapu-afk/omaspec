//! The state machines of a project, each found by the names it goes by.

use std::collections::HashMap;
use std::ops::Deref;

use crate::Spec;
use crate::across::key;

/// The state machines of a project, as a list, and each one found by its
/// names (letters and digits only, case aside) the way a participant of a
/// scenario or an object of a trace names it: one lookup, not a comparison
/// with every name of every machine.
#[derive(Debug, Clone, Default)]
pub struct Specs {
    list: Vec<Spec>,
    /// A name's key → the machines that go by it, by their place in `list`.
    named: HashMap<String, Vec<usize>>,
}

impl Specs {
    pub fn new(list: Vec<Spec>) -> Specs {
        let mut named: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, spec) in list.iter().enumerate() {
            let mut keys: Vec<String> = spec
                .names
                .iter()
                .map(|n| key(n))
                .filter(|k| !k.is_empty())
                .collect();
            keys.sort_unstable();
            keys.dedup();
            for k in keys {
                named.entry(k).or_default().push(index);
            }
        }
        Specs { list, named }
    }

    /// The machines that go by any of `names`, each once, in the project's
    /// order.
    pub fn named(&self, names: &[&str]) -> Vec<&Spec> {
        self.indices(names)
            .into_iter()
            .map(|index| &self.list[index])
            .collect()
    }

    /// [`Self::named`], by their places in the list.
    pub(crate) fn indices(&self, names: &[&str]) -> Vec<usize> {
        let mut found: Vec<usize> = names
            .iter()
            .filter_map(|name| self.named.get(&key(name)))
            .flatten()
            .copied()
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }
}

impl Deref for Specs {
    type Target = [Spec];

    fn deref(&self) -> &[Spec] {
        &self.list
    }
}

/// The same machines; the index follows from them.
impl PartialEq for Specs {
    fn eq(&self, other: &Specs) -> bool {
        self.list == other.list
    }
}

impl Eq for Specs {}

impl From<Vec<Spec>> for Specs {
    fn from(list: Vec<Spec>) -> Specs {
        Specs::new(list)
    }
}

impl FromIterator<Spec> for Specs {
    fn from_iter<I: IntoIterator<Item = Spec>>(iter: I) -> Specs {
        Specs::new(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use omaspec_core::Language;

    use super::*;

    fn machine(file: &str, title: &str) -> Spec {
        let code = format!("@startuml\ntitle {title}\n[*] --> Idle\n@enduml\n");
        Spec::read(&code, Language::PlantUml, file).unwrap()
    }

    #[test]
    fn a_machine_is_found_by_any_of_its_names_case_and_signs_aside() {
        let specs = Specs::new(vec![
            machine("door.puml", "Door control"),
            machine("lamp.puml", "Lamp"),
            machine("other/door.puml", "Back door"),
        ]);
        let files = |names: &[&str]| -> Vec<String> {
            specs.named(names).iter().map(|s| s.file.clone()).collect()
        };
        assert_eq!(files(&["door_control"]), ["door.puml"]);
        assert_eq!(files(&["LAMP", "lamp"]), ["lamp.puml"]);
        // Two machines go by their file's name.
        assert_eq!(files(&["Door"]), ["door.puml", "other/door.puml"]);
        assert_eq!(files(&["Door control", "Back door"]).len(), 2);
        assert!(files(&["", "—"]).is_empty());
        assert_eq!(specs.len(), 3);
        assert_eq!(specs, Specs::new(specs.to_vec()));
    }
}
