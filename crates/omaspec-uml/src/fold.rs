//! Containers folded to their box (docs/adr/0024): a composite state, a
//! package or a subgraph drawn as a box of its own, without what it
//! holds, a line into it or out of it ending on its border.
//!
//! A folded box says what it holds, *4 states · 6 transitions*, so a
//! folded picture loses less ([`counted`]).
//!
//! The pane folds; the file never says so. The pane hands the names to the
//! engine as lines it adds after the diagram, `' fold (Omaspec) Name` in
//! PlantUML and `%% fold (Omaspec) Name` in Mermaid, which every other
//! reader takes for a comment. The checks and the play read the file and
//! never see them; the drawers read them here.

use std::collections::HashSet;

use crate::common::Frame;

/// The words a fold line starts with, after its comment mark.
const MARK: &str = "fold (Omaspec)";

/// The names `source` folds, in the order written, each once.
pub fn read(source: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in source.lines() {
        let text = line.trim();
        let rest = text
            .strip_prefix('\'')
            .or_else(|| text.strip_prefix("%%"))
            .map(str::trim_start)
            .and_then(|rest| rest.strip_prefix(MARK));
        let Some(name) = rest.map(str::trim) else {
            continue;
        };
        if !name.is_empty() && !out.iter().any(|n| n == name) {
            out.push(name.to_owned());
        }
    }
    out
}

/// The line that folds `name`, in Mermaid's comments or PlantUML's.
pub fn line(name: &str, mermaid: bool) -> String {
    let mark = if mermaid { "%%" } else { "'" };
    format!("{mark} {MARK} {}\n", name.trim())
}

/// "1 state", "4 states".
pub(crate) fn counted(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// A container of a graph diagram whose members may hold more: a state
/// diagram's, a structure diagram's.
pub(crate) trait Folds: Sized {
    fn count(&self) -> usize;
    /// The key of member `index`.
    fn key(&self, index: usize) -> &str;
    /// Whether member `index` holds others: a box to fold.
    fn composite(&self, index: usize) -> bool;
    fn inner(&self, index: usize) -> &Self;
    fn inner_mut(&mut self, index: usize) -> &mut Self;
    /// Makes member `index` a box of its own, holding nothing.
    fn close(&mut self, index: usize);
    /// The paths of each line between two of its members' boxes, from
    /// here down to each end.
    fn crossings(&mut self) -> Vec<(&mut Vec<usize>, &mut Vec<usize>)>;
    /// Turns each line between two of its members whose ends now lie on
    /// those members themselves into a line between them, and keeps one of
    /// lines that are now alike: the same ends, words and look.
    fn settle(&mut self);
}

/// Folds the members of `root` named in `names`, at any depth, and keeps
/// out of `frame` the lines by hand and the holds that name what they
/// held. A name that is no member holding others folds nothing.
pub(crate) fn fold<C: Folds>(root: &mut C, frame: &mut Frame, names: &[String]) {
    if names.is_empty() {
        return;
    }
    let names: HashSet<&str> = names.iter().map(String::as_str).collect();
    let mut hidden = HashSet::new();
    walk(root, &names, &mut hidden);
    if hidden.is_empty() {
        return;
    }
    frame
        .by_hand
        .retain(|(_, line)| !hidden.contains(&line.key.from) && !hidden.contains(&line.key.to));
    frame
        .held
        .retain(|(_, hold)| !hidden.contains(&hold.name) && !hidden.contains(&hold.beside));
}

fn folded<C: Folds>(container: &C, index: usize, names: &HashSet<&str>) -> bool {
    container.composite(index) && names.contains(container.key(index))
}

/// Folds what `container` holds, and cuts its lines short where they go
/// into a box folded on the way to their end.
fn walk<C: Folds>(container: &mut C, names: &HashSet<&str>, hidden: &mut HashSet<String>) {
    // How much of each line's paths stays, read from the tree as it is
    // before anything in it is folded.
    let copies: Vec<(Vec<usize>, Vec<usize>)> = container
        .crossings()
        .into_iter()
        .map(|(from, to)| (from.clone(), to.clone()))
        .collect();
    let lengths: Vec<(usize, usize)> = copies
        .iter()
        .map(|(from, to)| (kept(container, from, names), kept(container, to, names)))
        .collect();
    let mut cut = false;
    for ((from, to), (keep_from, keep_to)) in container.crossings().into_iter().zip(lengths) {
        cut |= keep_from < from.len() || keep_to < to.len();
        from.truncate(keep_from);
        to.truncate(keep_to);
    }
    if cut {
        container.settle();
    }
    for index in 0..container.count() {
        if folded(container, index, names) {
            gather(container.inner(index), hidden);
            container.close(index);
        } else {
            walk(container.inner_mut(index), names, hidden);
        }
    }
}

/// How much of `path`, from `container` down, stays: up to the first box
/// on the way that is folded.
fn kept<C: Folds>(container: &C, path: &[usize], names: &HashSet<&str>) -> usize {
    let mut here = container;
    for (depth, &index) in path.iter().enumerate() {
        if index >= here.count() {
            break;
        }
        if depth + 1 < path.len() && folded(here, index, names) {
            return depth + 1;
        }
        here = here.inner(index);
    }
    path.len()
}

/// Every key held in `container`, at any depth.
fn gather<C: Folds>(container: &C, out: &mut HashSet<String>) {
    for index in 0..container.count() {
        let key = container.key(index);
        if !key.is_empty() {
            out.insert(key.to_owned());
        }
        gather(container.inner(index), out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_lines_are_read_in_either_language_and_written_back() {
        let source = "@startuml\nstate A {\n}\n@enduml\n' fold (Omaspec) A\n%% fold (Omaspec)  Long name \n' fold (Omaspec) A\n' folded A\n";
        assert_eq!(read(source), ["A", "Long name"]);
        assert_eq!(line("A", false), "' fold (Omaspec) A\n");
        assert_eq!(read(&line("B c", true)), ["B c"]);
    }

    #[test]
    fn a_folded_box_says_what_it_holds() {
        let source = "@startuml\nstate Running {\n  state Warm\n  state Hot {\n    state Core\n  }\n  [*] --> Warm\n  Warm --> Hot : heat\n  Hot --> Warm : cool\n}\n[*] --> Running\n@enduml\n' fold (Omaspec) Running\n";
        let svg = crate::render(source).unwrap();
        assert!(svg.contains("3 states · 3 transitions"), "{svg}");
        assert!(!svg.contains(">Warm<"), "folded away");
        // One of each, in a package of a class diagram.
        let source = "@startuml\npackage Shop {\n  class Order\n  class Line\n  Order --> Line\n}\nclass Customer\nCustomer --> Order\n@enduml\n' fold (Omaspec) Shop\n";
        let svg = crate::render(source).unwrap();
        assert!(svg.contains("2 elements · 1 line"), "{svg}");
    }

    #[test]
    fn a_branch_of_a_tree_folds_to_its_box() {
        let wbs = "@startwbs\n* Launch\n** Build\n*** Tests\n*** Docs\n**** API\n** Budget\n*** Tests\n@endwbs\n";
        let (_, map) = crate::render_mapped(wbs, None).unwrap();
        let named: Vec<(&str, bool)> = map
            .places
            .iter()
            .map(|p| (p.name.as_str(), p.frame))
            .collect();
        for expected in [
            ("Launch", true),
            ("Build", true),
            ("Docs", true),
            ("Tests", false),
            ("Tests (2)", false),
        ] {
            assert!(named.contains(&expected), "{expected:?} in {named:?}");
        }
        assert!(map.places.iter().all(|p| p.fixed));
        let (svg, map) =
            crate::render_mapped(&format!("{wbs}' fold (Omaspec) Build\n"), None).unwrap();
        assert!(svg.contains("3 nodes") && !svg.contains(">Docs<"), "{svg}");
        let build = map.places.iter().find(|p| p.name == "Build").unwrap();
        assert!(!build.frame, "folded, it holds nothing");
        // The other box of the same words keeps its name.
        assert!(map.places.iter().any(|p| p.name == "Tests (2)"));
        assert!(!map.places.iter().any(|p| p.name == "Tests"));
        // A mind map, on both sides of its root.
        let mind = "@startmindmap\n* Root\n** Right\n*** A\n-- Left\n--- B\n@endmindmap\n' fold (Omaspec) Root\n";
        let (svg, map) = crate::render_mapped(mind, None).unwrap();
        assert!(svg.contains("4 nodes") && !svg.contains(">Left<"), "{svg}");
        assert_eq!(map.places.len(), 1);
    }
}
