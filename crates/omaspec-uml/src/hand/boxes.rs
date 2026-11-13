//! What the block of lines by hand says about boxes: a box held beside
//! another ([0021](../../../../docs/adr/0021-boxes-held-in-place.md)), the
//! lines kept apart ([0022](../../../../docs/adr/0022-lines-bundled.md)),
//! sorted by key into the containers of a diagram ([`Members`]), and what
//! the writer reads about an entry that does not hold.

use std::collections::HashMap;

use super::{Apart, Hand, Hold, read};
use crate::graph::hold::{Held, Unheld};

/// A container of a diagram whose members have keys and may be containers
/// themselves: a state diagram's, a structure diagram's. The entries are
/// sorted into it by those keys ([`hold`], [`part`]).
pub(crate) trait Members {
    /// How many members it has.
    fn count(&self) -> usize;
    /// The key of member `index`; empty for one no entry can name (a
    /// start, an end, a history).
    fn key(&self, index: usize) -> &str;
    /// What member `index` holds.
    fn inner(&self, index: usize) -> &Self;
    fn inner_mut(&mut self, index: usize) -> &mut Self;
    /// What the block keeps for it.
    fn kept(&mut self) -> Kept<'_>;
}

/// What the block keeps for one container.
pub(crate) struct Kept<'a> {
    /// Its boxes held beside others.
    pub held: &'a mut Vec<Held>,
    /// When false, no line here bundles.
    pub bundled: &'a mut bool,
    /// Members whose lines stay out of a bundle.
    pub apart: &'a mut Vec<usize>,
}

/// Every key among the members of `root` and of what they hold, the first
/// of each: the path of its container (the indices of its members from
/// the top) and its index there.
fn places<T: Members>(root: &T) -> HashMap<String, (Vec<usize>, usize)> {
    fn walk<T: Members>(
        container: &T,
        path: &mut Vec<usize>,
        out: &mut HashMap<String, (Vec<usize>, usize)>,
    ) {
        for index in 0..container.count() {
            let key = container.key(index);
            if !key.is_empty() {
                out.entry(key.to_owned()).or_insert((path.clone(), index));
            }
            path.push(index);
            walk(container.inner(index), path, out);
            path.pop();
        }
    }
    let mut out = HashMap::new();
    walk(root, &mut Vec::new(), &mut out);
    out
}

/// Gives each container of `root` the boxes of `held` it holds both of, by
/// their keys; returns the entries that name no box, or two in different
/// containers.
pub(crate) fn hold<T: Members>(root: &mut T, held: &[(usize, Hold)]) -> Vec<(usize, String)> {
    if held.is_empty() {
        return Vec::new();
    }
    let (sorted, misses) = crate::graph::hold::sort(held, &places(root));
    for (path, holds) in sorted {
        let mut container = &mut *root;
        for &index in &path {
            container = container.inner_mut(index);
        }
        // After what the source itself holds (`-[hidden]right-`).
        container.kept().held.extend(holds);
    }
    misses
}

/// Keeps the lines of `entries` apart in the containers of `root`;
/// returns the entries that name no box.
pub(crate) fn part<T: Members>(root: &mut T, entries: &[(usize, Apart)]) -> Vec<(usize, String)> {
    fn apply<T: Members>(container: &mut T, path: &mut Vec<usize>, parted: &Parted) {
        let kept = container.kept();
        *kept.bundled = parted.bundled;
        if let Some(members) = parted.members.get(path.as_slice()) {
            kept.apart.clone_from(members);
        }
        for index in 0..container.count() {
            path.push(index);
            apply(container.inner_mut(index), path, parted);
            path.pop();
        }
    }
    if entries.is_empty() {
        return Vec::new();
    }
    let parted = parted(entries, &places(root));
    apply(root, &mut Vec::new(), &parted);
    parted.misses
}

/// The lines kept apart in `code`: each entry that reads, the first that
/// keeps every line apart and the first for each box, with the zero-based
/// line of its entry.
pub fn apart(code: &str) -> Vec<(usize, Apart)> {
    let mut out: Vec<(usize, Apart)> = Vec::new();
    let mut whole = false;
    for entry in read(code).map(|b| b.entries).unwrap_or_default() {
        if let Ok(Hand::Apart(apart)) = entry.read {
            let skip = match &apart.at {
                None => whole,
                Some(name) => out.iter().any(|(_, kept)| kept.at.as_deref() == Some(name)),
            };
            whole |= apart.at.is_none();
            if !skip {
                out.push((entry.line, apart));
            }
        }
    }
    out
}

/// Where `entries` keep lines apart, among the boxes of `places` (a name
/// to the path of its container and its index there). `bundled` is false
/// when an entry keeps every line apart. A name that is no box is a miss.
pub fn parted(entries: &[(usize, Apart)], places: &HashMap<String, (Vec<usize>, usize)>) -> Parted {
    let mut parted = Parted {
        bundled: true,
        members: HashMap::new(),
        misses: Vec::new(),
    };
    for (line, apart) in entries {
        match &apart.at {
            None => parted.bundled = false,
            Some(name) => match places.get(name) {
                Some((path, index)) => {
                    parted.members.entry(path.clone()).or_default().push(*index);
                }
                None => parted.misses.push((
                    *line,
                    format!("No box is named {name}; {apart} is left out"),
                )),
            },
        }
    }
    parted
}

/// What [`parted`] finds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parted {
    /// When false, no line of the diagram bundles.
    pub bundled: bool,
    /// Member indices, by the path of their container.
    pub members: HashMap<Vec<usize>, Vec<usize>>,
    /// Entries that name no box, by line, and what to say.
    pub misses: Vec<(usize, String)>,
}

/// The boxes held in `code`: each entry that reads, the first for a box,
/// with the zero-based line of its entry.
pub fn held(code: &str) -> Vec<(usize, Hold)> {
    let mut out: Vec<(usize, Hold)> = Vec::new();
    for entry in read(code).map(|b| b.entries).unwrap_or_default() {
        if let Ok(Hand::Hold(hold)) = entry.read
            && !out.iter().any(|(_, h)| h.name == hold.name)
        {
            out.push((entry.line, hold));
        }
    }
    out
}

/// What the writer reads about a held box that is not kept where its entry
/// says, and why; it stands where the layout puts it.
pub fn unheld(hold: &Hold, why: Unheld) -> String {
    let Hold { name, beside, .. } = hold;
    let why = match why {
        Unheld::Missing => "one of the two is no box of the diagram".to_owned(),
        Unheld::Itself => "a box cannot stand beside itself".to_owned(),
        Unheld::Apart => format!("{name} and {beside} lie in different containers"),
        Unheld::Joined => {
            format!("a line joins boxes this puts in one rank as {beside}, which cannot hold both")
        }
        Unheld::Round => format!("{name} and {beside} are held in one rank already"),
        Unheld::Frame => {
            "a container laid out with what is around it is not held as a whole".to_owned()
        }
        Unheld::Against => format!("the lines between {name} and {beside} rank them the other way"),
    };
    format!("{hold}: {why}; {name} stands where the layout puts it")
}

/// What the writer reads about the held boxes of a diagram that are not
/// kept: the entries that name no box or two apart (`unplaced`), and
/// those the layout could not keep (`unheld`), by line.
pub fn said(
    held: &[(usize, Hold)],
    unplaced: &[(usize, String)],
    unheld: &[(usize, Unheld)],
) -> Vec<(usize, String)> {
    let mut out = unplaced.to_vec();
    for &(line, why) in unheld {
        if let Some((_, hold)) = held.iter().find(|(at, _)| *at == line) {
            out.push((line, self::unheld(hold, why)));
        }
    }
    out.sort();
    out
}

/// What the writer reads about a held box, or its neighbor, that is not in
/// the diagram.
pub fn no_box(hold: &Hold, missing: &str) -> String {
    format!(
        "No box is named {}; {hold} is left out",
        super::name(missing)
    )
}
