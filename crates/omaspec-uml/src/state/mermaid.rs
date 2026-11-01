//! Mermaid state diagrams, read so that every state, transition and note
//! keeps its line. The state drawer draws this reading
//! ([`super::from_mermaid`]) and `omaspec-model` checks and plays it, so
//! what is checked is what is drawn. A line this reader does not know, or one Mermaid may read
//! otherwise, leaves the diagram unread: mmdr draws it, and it is not
//! checked.
//!
//! Names hold for the whole diagram: a state lives where it is first named,
//! and a transition at the top may join states in different containers.
//! Named again inside another state, Mermaid moves it there; such a diagram
//! is not read.

use std::collections::HashMap;

/// States nested deeper than this are not read, as PlantUML's are not.
const MAX_DEPTH: usize = 100;

/// A Mermaid state diagram as written.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Chart {
    /// In the order they are first named. The diagram and every state
    /// have a start and an end of their own in each concurrent region.
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    pub notes: Vec<Note>,
    /// The last `direction` written. The drawer lays the whole diagram out
    /// one way, as mmdr does.
    pub direction: Option<Direction>,
    /// `classDef name style`: each name and its style, in order.
    pub class_defs: Vec<(String, String)>,
    /// Each class given to a state (`class Id name`, `Id:::name`) and the
    /// state's id, in order.
    pub classes: Vec<(String, String)>,
    /// `style Id style`: each state's id and its style, in order.
    pub styles: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// The name transitions use; empty for a start or an end.
    pub id: String,
    /// `state "Label" as Id`.
    pub label: Option<String>,
    /// `Id : description`, every one.
    pub descriptions: Vec<String>,
    pub kind: Kind,
    /// `state Id { … }`.
    pub composite: bool,
    /// The state it sits in; `None` at the top.
    pub parent: Option<usize>,
    /// Which concurrent region of the parent holds it, counted by `--`.
    pub region: usize,
    /// Zero-based line where it is first named.
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    State,
    /// `[*]` as a source: where its container starts.
    Start,
    /// `[*]` as a target: where its container ends.
    End,
    Choice,
    Fork,
    Join,
}

/// `From --> To : label`.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub from: usize,
    pub to: usize,
    /// Empty when there is none.
    pub label: String,
    /// Zero-based line.
    pub line: usize,
}

/// `note right of Id : text`, or over several lines up to `end note`.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub target: usize,
    pub side: Side,
    /// Its lines, joined by line breaks.
    pub text: String,
    /// Zero-based line of its head.
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// `direction TB`, `BT`, `LR` or `RL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Down,
    Up,
    Right,
    Left,
}

/// The chart of a Mermaid state diagram; `None` if it is none or holds a
/// line this reader does not know.
pub fn read(source: &str) -> Option<Chart> {
    let mut reader = Reader::default();
    let mut head = false;
    let mut front = false;
    let mut skip: Option<fn(&str) -> bool> = None;
    let mut note: Option<Note> = None;
    for (no, raw) in source.split('\n').enumerate() {
        let l = raw.trim();
        if let Some(open) = &mut note {
            if is_end_note(l) {
                let done = note.take()?;
                if done.text.is_empty() {
                    return None;
                }
                reader.chart.notes.push(done);
            } else if !l.is_empty() {
                if !open.text.is_empty() {
                    open.text.push('\n');
                }
                open.text.push_str(l);
            }
            continue;
        }
        if let Some(end) = skip {
            if end(l) {
                skip = None;
            }
            continue;
        }
        if l.starts_with("%%{") {
            if !l.contains("}%%") {
                skip = Some(|l| l.contains("}%%"));
            }
            continue;
        }
        if l.is_empty() || l.starts_with("%%") || l.starts_with("```") {
            continue;
        }
        if l == "---" {
            front = !front;
            continue;
        }
        if front {
            continue;
        }
        if !head {
            if !l.starts_with("stateDiagram") {
                return None;
            }
            head = true;
            continue;
        }
        if l.starts_with("accTitle") || l.starts_with("accDescr") {
            if l.ends_with('{') {
                skip = Some(|l| l == "}");
            }
            continue;
        }
        let l = l.strip_suffix(';').unwrap_or(l).trim_end();
        // Mermaid ends a statement at `;`.
        if l.contains(';') {
            return None;
        }
        note = reader.line(l, no)?;
    }
    (head && reader.open.is_empty() && note.is_none()).then_some(reader.chart)
}

#[derive(Default)]
struct Reader {
    chart: Chart,
    by_name: HashMap<String, usize>,
    /// Open composite states, innermost last, each with its current region.
    open: Vec<(usize, usize)>,
    /// The start and the end of each container: its state, its region.
    markers: HashMap<(Option<usize>, usize, bool), usize>,
}

impl Reader {
    /// Reads one statement; a note it opens over several lines comes back
    /// for the lines that follow.
    fn line(&mut self, l: &str, no: usize) -> Option<Option<Note>> {
        let first = l.split_whitespace().next().unwrap_or_default();
        match first {
            "direction" => {
                let words: Vec<&str> = l.split_whitespace().collect();
                let [_, way] = words.as_slice() else {
                    return None;
                };
                self.chart.direction = Some(direction(way)?);
            }
            "classDef" => {
                if let Some((name, style)) = name_and_rest(l) {
                    self.chart
                        .class_defs
                        .push((name.to_owned(), style.to_owned()));
                }
            }
            "class" => {
                let words: Vec<&str> = l.split_whitespace().collect();
                if let [_, ids @ .., names] = words.as_slice()
                    && !ids.is_empty()
                {
                    for id in ids.join(" ").split(',').map(str::trim) {
                        for name in names.split(',').map(str::trim) {
                            if !id.is_empty() && !name.is_empty() {
                                self.chart.classes.push((id.to_owned(), name.to_owned()));
                            }
                        }
                    }
                }
            }
            "style" => {
                if let Some((ids, style)) = name_and_rest(l) {
                    for id in ids.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                        self.chart.styles.push((id.to_owned(), style.to_owned()));
                    }
                }
            }
            "state" => self.declare(l["state".len()..].trim(), no)?,
            _ if first.eq_ignore_ascii_case("note") => return self.note(l, no),
            _ => self.statement(l, no)?,
        }
        Some(None)
    }

    fn statement(&mut self, l: &str, no: usize) -> Option<()> {
        if l == "}" {
            let (state, _) = self.open.pop()?;
            // The drawer nests a state by what it holds, so an empty one
            // inside another would be drawn at the top.
            let empty = !self.chart.states.iter().any(|s| s.parent == Some(state));
            return (!empty || self.chart.states[state].parent.is_none()).then_some(());
        }
        if l == "--" {
            self.open.last_mut()?.1 += 1;
            return Some(());
        }
        if let Some((left, right)) = l.split_once("-->") {
            let (source, source_class) = single(left)?;
            let (target, target_class, rest) = id(right.trim_start())?;
            let rest = rest.trim();
            let label = match rest.strip_prefix(':') {
                Some(label) => label.trim(),
                None if rest.is_empty() => "",
                None => return None,
            };
            let from = self.end(source, source_class, true, no)?;
            let to = self.end(target, target_class, false, no)?;
            self.chart.transitions.push(Transition {
                from,
                to,
                label: label.to_owned(),
                line: no,
            });
            return Some(());
        }
        // `Id`, `Id:::class`, `Id : description`.
        let (name, class, rest) = id(l)?;
        let rest = rest.trim_start();
        let description = match rest.strip_prefix(':') {
            Some(text) if !text.starts_with(':') => strip_quotes(text.trim()),
            None if rest.is_empty() => "",
            _ => return None,
        };
        if name == "[*]" {
            return None;
        }
        let state = self.named(name, class, no)?;
        if !description.is_empty() {
            self.chart.states[state]
                .descriptions
                .push(description.to_owned());
        }
        Some(())
    }

    /// `state Id`, `state "Label" as Id`, `state Id <<choice>>`, each with
    /// a `{` after it to open the state.
    fn declare(&mut self, rest: &str, no: usize) -> Option<()> {
        // Mermaid's reading of a class on a declared state is not certain.
        if rest.contains(":::") {
            return None;
        }
        let (rest, open) = match rest.strip_suffix('{') {
            Some(rest) => (rest.trim_end(), true),
            None => (rest, false),
        };
        let (label, name, rest) = match rest.strip_prefix('"') {
            Some(quoted) => {
                let (label, after) = quoted.split_once('"')?;
                let name = after
                    .trim_start()
                    .strip_prefix("as")
                    .filter(|name| name.starts_with(char::is_whitespace))?;
                let (name, _, rest) = id(name.trim_start())?;
                (Some(label), name, rest)
            }
            None => {
                let (name, _, rest) = id(rest)?;
                (None, name, rest)
            }
        };
        let kind = match rest.trim() {
            "" => None,
            "<<choice>>" => Some(Kind::Choice),
            "<<fork>>" => Some(Kind::Fork),
            "<<join>>" => Some(Kind::Join),
            _ => return None,
        };
        if name == "[*]" {
            return None;
        }
        let here = self.here();
        let state = self.state(name, no)?;
        let s = &mut self.chart.states[state];
        if let Some(label) = label {
            s.label = Some(label.to_owned());
        }
        if let Some(kind) = kind {
            if s.composite {
                return None;
            }
            s.kind = kind;
        }
        if open {
            // Opened twice, as a pseudo state, or away from where it lives.
            if s.composite || s.kind != Kind::State || (s.parent, s.region) != here {
                return None;
            }
            if self.open.len() >= MAX_DEPTH {
                return None;
            }
            s.composite = true;
            self.open.push((state, 0));
        }
        Some(())
    }

    /// `note right of Id : text`, or the head of a note over several
    /// lines (`note left of Id`, also with a `:` and nothing after it).
    fn note(&mut self, l: &str, no: usize) -> Option<Option<Note>> {
        let rest = l.get(4..)?;
        let rest = rest
            .starts_with(char::is_whitespace)
            .then(|| rest.trim_start())?;
        let (side, rest) = match strip_word(rest, "right") {
            Some(rest) => (Side::Right, rest),
            None => (Side::Left, strip_word(rest, "left")?),
        };
        let rest = strip_word(rest, "of")?;
        let (target, text) = rest.split_once(':').unwrap_or((rest, ""));
        let (name, class) = single(target)?;
        if name == "[*]" || class.is_some() {
            return None;
        }
        let note = Note {
            target: self.state(name, no)?,
            side,
            text: text.trim().to_owned(),
            line: no,
        };
        if note.text.is_empty() {
            return Some(Some(note));
        }
        self.chart.notes.push(note);
        Some(None)
    }

    /// The state an end of a transition names; `[*]` is the open
    /// container's start (`source`) or end.
    fn end(&mut self, name: &str, class: Option<&str>, source: bool, no: usize) -> Option<usize> {
        if name != "[*]" {
            return self.named(name, class, no);
        }
        if class.is_some() {
            return None;
        }
        let (parent, region) = self.here();
        let key = (parent, region, source);
        if let Some(&state) = self.markers.get(&key) {
            return Some(state);
        }
        let kind = if source { Kind::Start } else { Kind::End };
        let state = self.push(String::new(), kind, no);
        self.markers.insert(key, state);
        Some(state)
    }

    /// The state called `name`, given `class` if there is one.
    fn named(&mut self, name: &str, class: Option<&str>, no: usize) -> Option<usize> {
        let state = self.state(name, no)?;
        if let Some(class) = class {
            self.chart.classes.push((name.to_owned(), class.to_owned()));
        }
        Some(state)
    }

    /// The state called `name`, made in the open container at its first
    /// mention. Named again, it must be at the top or in its own container.
    fn state(&mut self, name: &str, no: usize) -> Option<usize> {
        let here = self.here();
        if let Some(&state) = self.by_name.get(name) {
            let s = &self.chart.states[state];
            return (here.0.is_none() || (s.parent, s.region) == here).then_some(state);
        }
        // The drawer names starts, ends and regions so.
        if name.starts_with("__") {
            return None;
        }
        let state = self.push(name.to_owned(), Kind::State, no);
        self.by_name.insert(name.to_owned(), state);
        Some(state)
    }

    fn push(&mut self, id: String, kind: Kind, line: usize) -> usize {
        let (parent, region) = self.here();
        self.chart.states.push(State {
            id,
            label: None,
            descriptions: Vec::new(),
            kind,
            composite: false,
            parent,
            region,
            line,
        });
        self.chart.states.len() - 1
    }

    fn here(&self) -> (Option<usize>, usize) {
        self.open
            .last()
            .map_or((None, 0), |&(state, region)| (Some(state), region))
    }
}

fn direction(way: &str) -> Option<Direction> {
    Some(match way.to_ascii_uppercase().as_str() {
        "TB" | "TD" => Direction::Down,
        "BT" => Direction::Up,
        "LR" => Direction::Right,
        "RL" => Direction::Left,
        _ => return None,
    })
}

/// The word after the keyword that starts `l`, and the rest; `None` when
/// either is missing.
fn name_and_rest(l: &str) -> Option<(&str, &str)> {
    let mut parts = l.splitn(3, char::is_whitespace).skip(1);
    let name = parts.next()?.trim();
    let rest = parts.next()?.trim();
    (!name.is_empty() && !rest.is_empty()).then_some((name, rest))
}

/// `text` after `word` (in any case) and the space after it.
fn strip_word<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text
        .get(..word.len())
        .filter(|head| head.eq_ignore_ascii_case(word))
        .map(|_| &text[word.len()..])?;
    rest.starts_with(char::is_whitespace)
        .then(|| rest.trim_start())
}

fn is_end_note(l: &str) -> bool {
    let mut words = l.split_whitespace();
    matches!(
        (words.next(), words.next(), words.next()),
        (Some(end), Some(note), None)
            if end.eq_ignore_ascii_case("end") && note.eq_ignore_ascii_case("note")
    )
}

/// A state's id at the start of `text`, the class after it
/// (`Id:::class`), and what follows.
fn id(text: &str) -> Option<(&str, Option<&str>, &str)> {
    let end = text
        .find(|c: char| c.is_whitespace() || c == ':' || c == '{' || c == '<')
        .unwrap_or(text.len());
    let (name, mut rest) = text.split_at(end);
    let mut class = None;
    if let Some(after) = rest.strip_prefix(":::") {
        let end = after.find(char::is_whitespace).unwrap_or(after.len());
        let (name, after) = after.split_at(end);
        if name.is_empty() || name.contains(':') {
            return None;
        }
        class = Some(name);
        rest = after;
    }
    // A part of an arrow or a bracket is no name: Mermaid reads `A->`,
    // `--` and `A[x]` otherwise.
    let plain = !name.is_empty()
        && !name.contains(['"', '\'', '[', ']', '}', '>', ','])
        && !["--", "->", "<-", "=>"]
            .iter()
            .any(|part| name.contains(part));
    (plain || name == "[*]").then_some((name, class, rest))
}

/// `text` as one id and nothing more, and its class.
fn single(text: &str) -> Option<(&str, Option<&str>)> {
    let (name, class, rest) = id(text.trim())?;
    rest.trim().is_empty().then_some((name, class))
}

fn strip_quotes(text: &str) -> &str {
    ['"', '\'']
        .iter()
        .find_map(|&q| text.strip_prefix(q).and_then(|inner| inner.strip_suffix(q)))
        .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(chart: &Chart, id: &str) -> usize {
        chart.states.iter().position(|s| s.id == id).unwrap()
    }

    #[test]
    fn reads_states_composites_and_regions() {
        let chart = read(
            "---\ntitle: Player\n---\nstateDiagram-v2\n  %% a comment\n  direction LR\n  [*] --> Idle\n  Idle --> Busy: job [ready] / start\n  state \"Working hard\" as Busy {\n    [*] --> Work\n    --\n    [*] --> Watch\n  }\n  state pick <<choice>>\n  Work --> pick\n  note right of Idle\n    waits here\n    and there\n  end note\n  Busy --> [*]\n  classDef hot fill:#f00\n  class Idle hot\n",
        )
        .unwrap();
        let (idle, busy, work, watch, pick) = (
            find(&chart, "Idle"),
            find(&chart, "Busy"),
            find(&chart, "Work"),
            find(&chart, "Watch"),
            find(&chart, "pick"),
        );
        assert_eq!(chart.direction, Some(Direction::Right));
        assert_eq!(chart.states[idle].line, 6);
        assert_eq!(chart.states[busy].line, 7);
        assert!(chart.states[busy].composite);
        assert_eq!(chart.states[busy].label.as_deref(), Some("Working hard"));
        assert_eq!(chart.states[work].parent, Some(busy));
        assert_eq!(
            (chart.states[work].region, chart.states[watch].region),
            (0, 1)
        );
        assert_eq!(chart.states[pick].kind, Kind::Choice);
        // The top start, one start per region, the top end.
        let markers: Vec<(Kind, Option<usize>, usize)> = chart
            .states
            .iter()
            .filter(|s| s.id.is_empty())
            .map(|s| (s.kind, s.parent, s.region))
            .collect();
        assert_eq!(
            markers,
            [
                (Kind::Start, None, 0),
                (Kind::Start, Some(busy), 0),
                (Kind::Start, Some(busy), 1),
                (Kind::End, None, 0),
            ]
        );
        let job = &chart.transitions[1];
        assert_eq!((job.from, job.to, job.line), (idle, busy, 7));
        assert_eq!(job.label, "job [ready] / start");
        let note = &chart.notes[0];
        assert_eq!((note.target, note.side, note.line), (idle, Side::Right, 15));
        assert_eq!(note.text, "waits here\nand there");
        assert_eq!(chart.class_defs, [("hot".into(), "fill:#f00".into())]);
        assert_eq!(chart.classes, [("Idle".into(), "hot".into())]);
    }

    #[test]
    fn reads_what_mermaid_writes_beyond_transitions() {
        let chart = read(
            "stateDiagram\nOff\nOn:::hot\nOn : glowing\nOn : \"warm\"\nOn --> Off : x<-->y\nOff : on -> off\nnote left of Off : cold\nstyle On,Off fill:#0f0\n",
        )
        .unwrap();
        let (off, on) = (find(&chart, "Off"), find(&chart, "On"));
        assert_eq!((chart.states[off].line, chart.states[on].line), (1, 2));
        assert_eq!(chart.states[on].descriptions, ["glowing", "warm"]);
        assert_eq!(chart.states[off].descriptions, ["on -> off"]);
        assert_eq!(chart.transitions[0].label, "x<-->y");
        assert_eq!(chart.classes, [("On".into(), "hot".into())]);
        assert_eq!(chart.notes[0].side, Side::Left);
        assert_eq!(
            chart.styles,
            [
                ("On".into(), "fill:#0f0".into()),
                ("Off".into(), "fill:#0f0".into())
            ]
        );
    }

    #[test]
    fn a_state_named_again_elsewhere_is_where_it_was_first() {
        let chart = read(
            "stateDiagram-v2\nstate On {\n[*] --> Idle\nIdle --> Busy\n}\n[*] --> Off\nBusy --> Off : fault\nOff --> Idle : wake\n",
        )
        .unwrap();
        let on = find(&chart, "On");
        assert_eq!(chart.states[find(&chart, "Busy")].parent, Some(on));
        assert_eq!(chart.states[find(&chart, "Off")].parent, None);
    }

    #[test]
    fn what_it_does_not_know_is_not_read() {
        for code in [
            "flowchart LR\nA --> B",
            "stateDiagram\nA --> B : x\nwhatever this is",
            "stateDiagram\nstate A {\nA1 --> A2",
            "stateDiagram\nA -> B",
            "stateDiagram\nA <--> B",
            "stateDiagram\nA ..> B",
            "stateDiagram\nA --> B --> C",
            "stateDiagram\nA --> B; B --> C",
            "stateDiagram\nstate S {\ndirection LR A --> B\n}",
            "stateDiagram\ndirection XY",
            "stateDiagram\nA --> --",
            "stateDiagram\nstate On:::hot {\n[*] --> A\n}",
            "stateDiagram\nA --> B\nB-> : x",
            "stateDiagram\nstate \"Long name\" asB",
            "stateDiagram\nstate \"Long\" as B==>C",
            "stateDiagram\n[*]",
            "stateDiagram\nA --> B\nstate",
            "stateDiagram\n[*]:::hot --> A",
            "stateDiagram\nA[x] --> B",
            "stateDiagram\n\"A\" --> B",
            "stateDiagram\n__start_0__ --> B",
            "stateDiagram\nnote over A : x",
            "stateDiagram\nnote right of A\nnever ended",
            "stateDiagram\nnote right of A\nend note",
            "stateDiagram\n}",
            "stateDiagram\n--",
            // Named again inside another state: Mermaid moves it there.
            "stateDiagram\n[*] --> A\nstate S {\nA --> B\n}",
            "stateDiagram\nstate S {\n[*] --> A\n}\nstate T {\nA --> B\n}",
            "stateDiagram\nstate S {\n[*] --> A\n--\nA --> B\n}",
            "stateDiagram\nstate S {\nS --> A\n}",
            // Opened twice, or as a pseudo state.
            "stateDiagram\nstate S {\n[*] --> A\n}\nstate S {\n[*] --> B\n}",
            "stateDiagram\nstate c <<choice>>\nstate c {\n[*] --> A\n}",
            // An empty state inside another.
            "stateDiagram\nstate S {\nstate T {\n}\n}",
        ] {
            assert!(read(code).is_none(), "{code}");
        }
    }
}
