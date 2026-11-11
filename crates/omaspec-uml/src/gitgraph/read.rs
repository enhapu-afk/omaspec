//! Reads a Mermaid `gitGraph`: its statements run one after another on a
//! small repository that knows its branches, their heads and its commits.

use std::collections::HashMap;

use super::{Branch, Commit, Diagram, Direction, From, Kind, Look};
use crate::common::{Text, text_at};
use crate::source::starts_with_ci;

pub(super) struct Reason {
    pub line: usize,
    pub what: &'static str,
}

pub(super) enum Stop {
    /// Not a git graph.
    Foreign,
    At(Reason),
}

fn stop(line: usize, what: &'static str) -> Reason {
    Reason { line, what }
}

pub(super) fn read(source: &str) -> Result<Diagram, Stop> {
    let Scanned {
        title,
        front,
        lines,
    } = scan(source);
    let Some(((line, header), rest)) = lines.split_first() else {
        return Err(Stop::Foreign);
    };
    let direction = match header_of(header) {
        None => return Err(Stop::Foreign),
        Some(Err(())) => return Err(Stop::At(stop(*line, "a git graph direction"))),
        Some(Ok(direction)) => direction,
    };
    if let Some(reason) = front {
        return Err(Stop::At(reason));
    }
    let mut repo = Repo::new(*line);
    for (line, text) in rest {
        repo.run(*line, text).map_err(Stop::At)?;
    }
    Ok(repo.finish(title, direction))
}

struct Scanned {
    title: Option<Text>,
    front: Option<Reason>,
    lines: Vec<(usize, String)>,
}

/// The lines that say something, without comments; a front matter at the
/// top gives the title. A setting of the front matter that nothing here
/// carries out is a reason to stop.
fn scan(source: &str) -> Scanned {
    let mut title = None;
    let mut front = None;
    let mut lines = Vec::new();
    let mut iter = source.lines().enumerate().peekable();
    if iter.peek().is_some_and(|(_, raw)| raw.trim() == "---") {
        iter.next();
        let mut kept = crate::source::Front::default();
        for (no, raw) in iter.by_ref() {
            let text = raw.trim();
            if text == "---" {
                break;
            }
            if text.is_empty() || text.starts_with('#') || kept.drawn(raw) {
                continue;
            }
            match text.split_once(':') {
                Some((key, value)) if key.trim().eq_ignore_ascii_case("title") => {
                    let value = unquote(value.trim());
                    if !value.is_empty() {
                        title = Some(text_at(value, no));
                    }
                }
                _ => {
                    front.get_or_insert(stop(no, "a front matter setting"));
                }
            }
        }
    }
    for (no, raw) in iter {
        let text = raw.trim();
        let text = match text.find("%%") {
            Some(at) => text[..at].trim(),
            None => text,
        };
        if !text.is_empty() {
            lines.push((no, text.to_owned()));
        }
    }
    Scanned {
        title,
        front,
        lines,
    }
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(text)
}

/// `gitGraph`, `gitGraph:`, `gitGraph LR:`, `TB` or `BT`: the direction,
/// `Err` for one this drawer does not know, `None` for another diagram.
fn header_of(text: &str) -> Option<Result<Direction, ()>> {
    const WORD: &str = "gitGraph";
    if !starts_with_ci(text, WORD) {
        return None;
    }
    let rest = &text[WORD.len()..];
    if rest.starts_with(|c: char| c.is_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    let rest = rest.trim();
    let rest = rest.strip_suffix(':').unwrap_or(rest).trim();
    Some(match rest {
        "" | "LR" => Ok(Direction::Across),
        "TB" => Ok(Direction::Down),
        "BT" => Ok(Direction::Up),
        _ => Err(()),
    })
}

/// A branch as it was made.
struct Made {
    name: String,
    line: usize,
    order: Option<f32>,
    /// Its newest commit; a branch starts at the head it was made from.
    head: Option<usize>,
}

/// The repository the statements run on.
struct Repo {
    branches: Vec<Made>,
    current: usize,
    commits: Vec<Commit>,
    ids: HashMap<String, usize>,
    in_descr: bool,
}

impl Repo {
    fn new(line: usize) -> Repo {
        Repo {
            branches: vec![Made {
                name: "main".to_owned(),
                line,
                order: None,
                head: None,
            }],
            current: 0,
            commits: Vec::new(),
            ids: HashMap::new(),
            in_descr: false,
        }
    }

    fn run(&mut self, line: usize, text: &str) -> Result<(), Reason> {
        if self.in_descr {
            self.in_descr = !text.contains('}');
            return Ok(());
        }
        if starts_with_ci(text, "accDescr") && text.contains('{') && !text.contains('}') {
            self.in_descr = true;
            return Ok(());
        }
        if starts_with_ci(text, "accTitle") || starts_with_ci(text, "accDescr") {
            return Ok(());
        }
        let (word, rest) = match text.split_once(char::is_whitespace) {
            Some((word, rest)) => (word, rest.trim()),
            None => (text, ""),
        };
        match word {
            "commit" => {
                let said = Said::read(rest, line, "a commit option", &["id", "tag", "type"])?;
                self.commit(line, &said, None)
            }
            "branch" => {
                let (name, rest) = name(rest).ok_or(stop(line, "a branch without a name"))?;
                let said = Said::read(rest, line, "a branch option", &["order"])?;
                let order = match said.get("order") {
                    Some(order) => Some(order.parse().map_err(|_| stop(line, "a branch option"))?),
                    None => None,
                };
                if self.find(name).is_some() {
                    return Err(stop(line, "a branch named twice"));
                }
                self.branches.push(Made {
                    name: name.to_owned(),
                    line,
                    order,
                    head: self.branches[self.current].head,
                });
                self.current = self.branches.len() - 1;
                Ok(())
            }
            "checkout" | "switch" => {
                let (name, rest) = name(rest).ok_or(stop(line, "a checkout without a branch"))?;
                if !rest.is_empty() {
                    return Err(stop(line, "a git graph statement"));
                }
                self.current = self
                    .find(name)
                    .ok_or(stop(line, "a branch that is not there"))?;
                Ok(())
            }
            "merge" => {
                let (name, rest) = name(rest).ok_or(stop(line, "a merge without a branch"))?;
                let said = Said::read(rest, line, "a merge option", &["id", "tag", "type"])?;
                let other = self
                    .find(name)
                    .ok_or(stop(line, "a branch that is not there"))?;
                if other == self.current {
                    return Err(stop(line, "a branch merged into itself"));
                }
                let (here, there) = (self.branches[self.current].head, self.branches[other].head);
                let (Some(_), Some(there)) = (here, there) else {
                    return Err(stop(line, "a merge that brings nothing"));
                };
                if here == Some(there) {
                    return Err(stop(line, "a merge that brings nothing"));
                }
                self.commit(line, &said, Some(From::Merged(there)))
            }
            "cherry-pick" => {
                let said =
                    Said::read(rest, line, "a cherry-pick option", &["id", "tag", "parent"])?;
                let id = said
                    .get("id")
                    .filter(|id| !id.is_empty())
                    .ok_or(stop(line, "a cherry-pick without an id"))?;
                let source = *self
                    .ids
                    .get(id)
                    .ok_or(stop(line, "a cherry-pick of a commit that is not there"))?;
                if self.commits[source].branch == self.current {
                    return Err(stop(line, "a cherry-pick from its own branch"));
                }
                // The copy has no id of its own to show: it is the one it
                // copies, which its tag says.
                let tag = said
                    .get("tag")
                    .map_or_else(|| format!("cherry-pick: {id}"), str::to_owned);
                let copy = Said {
                    pairs: vec![("tag".to_owned(), tag)],
                };
                self.commit(line, &copy, Some(From::Picked(source)))
            }
            _ => Err(stop(line, "a git graph statement")),
        }
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.branches.iter().position(|branch| branch.name == name)
    }

    fn commit(&mut self, line: usize, said: &Said, from: Option<From>) -> Result<(), Reason> {
        let kind = match said.get("type") {
            None | Some("NORMAL") => Kind::Normal,
            Some("HIGHLIGHT") => Kind::Highlight,
            Some("REVERSE") => Kind::Reverse,
            Some(_) => return Err(stop(line, "a commit type")),
        };
        let id = said.get("id").filter(|id| !id.is_empty());
        let at = self.commits.len();
        if let Some(id) = id
            && self.ids.insert(id.to_owned(), at).is_some()
        {
            return Err(stop(line, "a commit id used twice"));
        }
        let branch = &mut self.branches[self.current];
        self.commits.push(Commit {
            id: id.map(str::to_owned),
            tag: said
                .get("tag")
                .filter(|tag| !tag.is_empty())
                .map(str::to_owned),
            kind,
            branch: self.current,
            parent: branch.head,
            from,
            line,
        });
        branch.head = Some(at);
        Ok(())
    }

    /// The graph, its lanes in their order: `main` first, then the
    /// branches in the order they were made, then those that asked for a
    /// place by their `order`.
    fn finish(self, title: Option<Text>, direction: Direction) -> Diagram {
        let mut lanes: Vec<usize> = (0..self.branches.len()).collect();
        lanes.sort_by(|&a, &b| {
            let key = |at: usize| {
                let branch = &self.branches[at];
                (at != 0, branch.order.is_some(), branch.order.unwrap_or(0.0))
            };
            let (ka, kb) = (key(a), key(b));
            (ka.0, ka.1)
                .cmp(&(kb.0, kb.1))
                .then(ka.2.total_cmp(&kb.2))
                .then(a.cmp(&b))
        });
        let mut lane_of = vec![0; lanes.len()];
        for (lane, &made) in lanes.iter().enumerate() {
            lane_of[made] = lane;
        }
        let mut commits = self.commits;
        for commit in &mut commits {
            commit.branch = lane_of[commit.branch];
        }
        let branches = lanes
            .iter()
            .map(|&made| Branch {
                name: self.branches[made].name.clone(),
                line: self.branches[made].line,
            })
            .collect();
        Diagram {
            title,
            direction,
            branches,
            commits,
            look: Look::default(),
        }
    }
}

/// The name a statement starts with, in quotes or one word, and what
/// follows it.
fn name(text: &str) -> Option<(&str, &str)> {
    if let Some(rest) = text.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some((&rest[..end], rest[end + 1..].trim())).filter(|(name, _)| !name.is_empty());
    }
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (end > 0).then(|| (&text[..end], text[end..].trim()))
}

/// What a statement says after its word: `id: "x" tag: "y" type: REVERSE`.
struct Said {
    pairs: Vec<(String, String)>,
}

impl Said {
    /// Reads `key: value` pairs, a value in quotes or one word. A key that
    /// is not among `known`, or text that is no pair, stops with `what`.
    fn read(text: &str, line: usize, what: &'static str, known: &[&str]) -> Result<Said, Reason> {
        let mut pairs = Vec::new();
        let mut rest = text.trim();
        while !rest.is_empty() {
            let (key, after) = rest.split_once(':').ok_or(stop(line, what))?;
            let key = key.trim();
            if !known.contains(&key) {
                return Err(stop(line, what));
            }
            let after = after.trim_start();
            let (value, more) = match after.strip_prefix('"') {
                Some(quoted) => {
                    let end = quoted.find('"').ok_or(stop(line, what))?;
                    (&quoted[..end], &quoted[end + 1..])
                }
                None => {
                    let end = after.find(char::is_whitespace).unwrap_or(after.len());
                    (&after[..end], &after[end..])
                }
            };
            pairs.push((key.to_owned(), value.to_owned()));
            rest = more.trim();
        }
        Ok(Said { pairs })
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value.as_str())
    }
}
