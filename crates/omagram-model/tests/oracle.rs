//! The Java PlantUML as the oracle (docs/adr/0004): what Omagram reads
//! from each PlantUML state and sequence diagram in the repository, and
//! from those diagrams changed at random, against what PlantUML reads from
//! it. PlantUML must read it as the same type of diagram. A state machine
//! is held against PlantUML's SCXML export: its states, where they sit,
//! their kinds and the transitions with their labels (notes left out). A
//! scenario is held against its XMI export: the participants in order, the
//! messages in order with their ends, labels and the part of the group they
//! are in, and the groups. An example Omagram reads must be one PlantUML
//! reads without an error; for a changed diagram that is only told, since
//! Omagram reads some lines PlantUML refuses.
//!
//! Skipped without a `plantuml` (`$OMAGRAM_PLANTUML`, else `PATH`);
//! `OMAGRAM_REQUIRE_PLANTUML=1`, as in CI, makes that a failure.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs, thread};

use omagram_core::Language;
use omagram_model::{Kind, Machine, Trigger};
use omagram_uml::sequence::{self, End, GroupKind, Item};
use roxmltree::{Document, Node};

const DELIMITER: &str = "__omagram_oracle__";

#[test]
fn plantuml_reads_the_examples_as_omagram_does() {
    let Some(plantuml) = plantuml() else { return };
    let held = hold(&plantuml, diagrams());
    assert!(held.lenient.is_empty(), "\n{}", held.lenient.join("\n\n"));
    eprintln!(
        "compared {} machines and {} scenarios with {}",
        held.machines,
        held.scenarios,
        plantuml.display()
    );
    assert!(held.machines >= 5, "only {} machines", held.machines);
    assert!(held.scenarios >= 5, "only {} scenarios", held.scenarios);
    assert!(held.wrong.is_empty(), "\n{}", held.wrong.join("\n\n"));
}

/// The examples changed in a few places, from a fixed seed so a finding
/// repeats; `OMAGRAM_ORACLE_ROUNDS` sets how many (default 300).
#[test]
fn plantuml_reads_changed_diagrams_as_omagram_does() {
    let Some(plantuml) = plantuml() else { return };
    let rounds = env::var("OMAGRAM_ORACLE_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let starts: Vec<(String, String, &[&str])> = diagrams()
        .into_iter()
        .filter_map(|(name, block)| {
            if Machine::read(&block, Language::PlantUml).is_some() {
                Some((name, block, STATE_TOKENS))
            } else if let Ok(omagram_uml::Diagram::Sequence(_)) = omagram_uml::parse(&block) {
                Some((name, block, SEQUENCE_TOKENS))
            } else {
                None
            }
        })
        .collect();
    let mut rng = Rng(0x0c1e_5eed_0000_0001);
    let cases = (0..rounds)
        .map(|round| {
            let (name, block, tokens) = &starts[rng.below(starts.len())];
            let times = rng.below(4) + 1;
            (
                format!("{name}, changed (round {round})"),
                mutated(&mut rng, block, tokens, times),
            )
        })
        .collect();
    let held = hold(&plantuml, cases);
    eprintln!(
        "compared {} changed machines and {} changed scenarios; \
         {} PlantUML finds an error in:\n\n{}",
        held.machines,
        held.scenarios,
        held.lenient.len(),
        held.lenient.join("\n\n")
    );
    assert!(held.wrong.is_empty(), "\n{}", held.wrong.join("\n\n"));
}

/// The configured `plantuml`, or the one in `PATH`; `none` is none. Without
/// one a test is skipped, unless `OMAGRAM_REQUIRE_PLANTUML` is set.
fn plantuml() -> Option<PathBuf> {
    let found = match env::var_os("OMAGRAM_PLANTUML") {
        Some(path) if path == "none" => None,
        Some(path) => Some(PathBuf::from(path)),
        None => env::var_os("PATH").and_then(|path| {
            env::split_paths(&path)
                .map(|dir| dir.join("plantuml"))
                .find(|p| p.is_file())
        }),
    };
    if found.is_none() {
        assert!(
            env::var_os("OMAGRAM_REQUIRE_PLANTUML").is_none(),
            "OMAGRAM_REQUIRE_PLANTUML is set, but there is no plantuml"
        );
        eprintln!("skipped: no plantuml");
    }
    found
}

/// How many machines and scenarios were held against PlantUML, what
/// PlantUML read otherwise, and what it found an error in.
struct Held {
    machines: usize,
    scenarios: usize,
    wrong: Vec<String>,
    lenient: Vec<String>,
}

/// Each named block Omagram reads as a state machine or a scenario,
/// against PlantUML's reading of it.
fn hold(plantuml: &Path, cases: Vec<(String, String)>) -> Held {
    let mut machines = Vec::new();
    let mut scenarios = Vec::new();
    for (name, block) in cases {
        if let Some(machine) = Machine::read(&block, Language::PlantUml) {
            machines.push((name, block, from_machine(&machine)));
        } else if let Ok(omagram_uml::Diagram::Sequence(d)) = omagram_uml::parse(&block)
            // Without a participant Omagram does not draw it.
            && !d.participants.is_empty()
        {
            scenarios.push((name, block, from_sequence(&d)));
        }
    }
    let mut wrong = Vec::new();
    let mut lenient = Vec::new();
    for (cases, format, what, kind) in [
        (&machines, "-tscxml", "machine", "STATE"),
        (&scenarios, "-txmi", "scenario", "SEQUENCE"),
    ] {
        let blocks: Vec<String> = cases.iter().map(|(_, b, _)| b.clone()).collect();
        // An export of a diagram with an error stops PlantUML: those go first.
        let drawn = export(plantuml, "-tsvg", &blocks);
        let mut readable = Vec::new();
        for ((name, block, ours), answer) in cases.iter().zip(drawn) {
            match answer.as_deref().map(|a| a.strip_prefix("ERROR\n")) {
                Some(None) => {
                    let svg = answer.as_deref().unwrap_or_default();
                    let drawn = svg
                        .split_once("data-diagram-type=\"")
                        .and_then(|(_, rest)| rest.split_once('"'))
                        .map_or("", |(kind, _)| kind);
                    if drawn.is_empty() {
                        // A crash report: PlantUML could not draw it.
                        eprintln!("{name}: PlantUML stopped on it");
                    } else if what == "machine" && block.contains("<-") {
                        // SCXML gives a transition written backwards the wrong way round.
                        eprintln!("{name}: has a backward arrow");
                    } else if drawn == kind {
                        readable.push((name, block, ours));
                    } else {
                        wrong.push(format!(
                            "{name}: Omagram reads a {what}, PlantUML a {drawn} diagram\n{block}"
                        ));
                    }
                }
                Some(Some(error)) => lenient.push(format!(
                    "{name}: Omagram reads it, PlantUML finds an error in line {} of it\n{block}",
                    error.lines().take(2).collect::<Vec<_>>().join(": ")
                )),
                None => eprintln!("{name}: PlantUML stopped on it"),
            }
        }
        let blocks: Vec<String> = readable.iter().map(|(_, b, _)| (*b).clone()).collect();
        for ((name, block, ours), answer) in readable.iter().zip(export(plantuml, format, &blocks))
        {
            let Some(answer) = answer else {
                eprintln!("{name}: PlantUML {format} stopped on it");
                continue;
            };
            let theirs = match Document::parse(&answer) {
                Ok(doc) if what == "machine" => from_scxml(&doc),
                Ok(doc) => from_xmi(&doc),
                Err(e) => {
                    wrong.push(format!(
                        "{name}: PlantUML's {format} does not parse: {e}\n{answer}"
                    ));
                    continue;
                }
            };
            compare(&mut wrong, name, block, what, ours, &theirs);
        }
    }
    Held {
        machines: machines.len(),
        scenarios: scenarios.len(),
        wrong,
        lenient,
    }
}

/// The first `@start…` block of every PlantUML file in the repository's
/// tests and examples, by file name.
fn diagrams() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut folders: Vec<PathBuf> = [
        "crates/omagram-uml/tests",
        "crates/omagram-core/tests/fixtures",
        "examples",
    ]
    .iter()
    .map(|f| root.join(f))
    .collect();
    let mut files = Vec::new();
    while let Some(folder) = folders.pop() {
        for path in fs::read_dir(&folder).unwrap().map(|e| e.unwrap().path()) {
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|e| e == "puml") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let code = fs::read_to_string(&path).ok()?;
            let name = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            Some((name, first_block(&code)?))
        })
        .collect()
}

/// From the first `@start…` line to the `@end…` after it.
fn first_block(code: &str) -> Option<String> {
    let lines: Vec<&str> = code.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim_start().starts_with("@start"))?;
    let end = start
        + lines[start..]
            .iter()
            .position(|l| l.trim_start().starts_with("@end"))?;
    Some(lines[start..=end].join("\n") + "\n")
}

/// Each block through `plantuml -pipe` in `format`: its answer, or `None`
/// where PlantUML stopped on it (it starts again for the rest).
fn export(plantuml: &Path, format: &str, blocks: &[String]) -> Vec<Option<String>> {
    let mut answers = Vec::new();
    while answers.len() < blocks.len() {
        let rest = &blocks[answers.len()..];
        let got = pipe(plantuml, format, rest);
        let stopped = got.len() < rest.len();
        answers.extend(got.into_iter().map(Some));
        if stopped {
            answers.push(None);
        }
    }
    answers
}

/// The answers of one `plantuml -pipe` to `blocks`, up to where it stopped.
fn pipe(plantuml: &Path, format: &str, blocks: &[String]) -> Vec<String> {
    let mut child = Command::new(plantuml)
        .args([
            "-pipe",
            format,
            "-charset",
            "UTF-8",
            "-nometadata",
            "-pipeNoStderr",
            "-pipedelimitor",
            DELIMITER,
        ])
        .env("PLANTUML_SECURITY_PROFILE", "SANDBOX")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("cannot start {}: {e}", plantuml.display()));
    let mut stdin = child.stdin.take().unwrap();
    let input = blocks.concat();
    // It may stop before it has read everything.
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let mut out = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    writer.join().unwrap();
    child.wait().unwrap();
    // An SVG ends without a line break, so the delimiter may follow it on
    // its line; what comes after the last delimiter is no whole answer.
    let mut answers: Vec<String> = out
        .split(DELIMITER)
        .map(|a| a.trim_start_matches('\n').to_owned())
        .collect();
    answers.pop();
    answers.truncate(blocks.len());
    answers
}

/// xorshift64*: small, fast, the same everywhere.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const STATE_TOKENS: &[&str] = &[
    "[*]",
    "-->",
    "->",
    "-left->",
    "state ",
    "{",
    "}",
    "--",
    "||",
    ":",
    "[",
    "]",
    "/",
    ",",
    "<<choice>>",
    "<<fork>>",
    "<<join>>",
    "[H]",
    "[H*]",
    "\"",
    "as ",
    "note ",
    "end note",
    "\n",
    " ",
    "A",
    "B",
    "\\n",
    ";",
];

const SEQUENCE_TOKENS: &[&str] = &[
    "->",
    "-->",
    "->>",
    "<-",
    "-x",
    "->o",
    "[->",
    "->]",
    "?->",
    ":",
    "\n",
    " ",
    "A",
    "B",
    "participant ",
    "actor ",
    "as ",
    "\"",
    "alt ",
    "else ",
    "end",
    "opt ",
    "loop ",
    "par",
    "break",
    "critical",
    "group ",
    "[x]",
    "activate ",
    "deactivate ",
    "create ",
    "destroy ",
    "return ",
    "++",
    "--",
    "**",
    "...",
    "|||",
    "== d ==",
    "note over A : n",
    "ref over A : r",
    "autonumber",
    "\\n",
];

/// `start` changed in `times` places: a token put in, a piece cut out or
/// a line doubled. The first and last lines stay.
fn mutated(rng: &mut Rng, start: &str, tokens: &[&str], times: usize) -> String {
    let lines: Vec<&str> = start.lines().collect();
    let (head, body, foot) = (lines[0], &lines[1..lines.len() - 1], lines[lines.len() - 1]);
    let mut code = body.join("\n");
    for _ in 0..times {
        let mut at = rng.below(code.len() + 1);
        while !code.is_char_boundary(at) {
            at -= 1;
        }
        match rng.below(3) {
            0 => code.insert_str(at, tokens[rng.below(tokens.len())]),
            1 => {
                let mut end = (at + rng.below(12)).min(code.len());
                while !code.is_char_boundary(end) {
                    end -= 1;
                }
                code.replace_range(at..end, "");
            }
            _ => {
                let line = code
                    .lines()
                    .nth(rng.below(code.lines().count()))
                    .unwrap_or("");
                let line = format!("{line}\n");
                code.insert_str(at, &line);
            }
        }
    }
    // No `@end…` inside, so PlantUML reads one block, as Omagram does.
    let code = code.replace("@end", "end");
    format!("{head}\n{code}\n{foot}\n")
}

/// What both sides read, in words, so a difference reads as one.
#[derive(Debug, Default)]
struct Reading {
    /// In order: participants. Sorted: states.
    things: Vec<String>,
    /// In order: messages. Sorted: transitions.
    steps: Vec<String>,
    /// Groups, in order.
    groups: Vec<String>,
}

fn compare(
    wrong: &mut Vec<String>,
    name: &str,
    block: &str,
    what: &str,
    ours: &Reading,
    theirs: &Reading,
) {
    let mut lines = Vec::new();
    for (part, a, b) in [
        ("states or participants", &ours.things, &theirs.things),
        ("transitions or messages", &ours.steps, &theirs.steps),
        ("groups", &ours.groups, &theirs.groups),
    ] {
        if a != b {
            lines.push(format!(
                "  {part}\n    Omagram: {a:?}\n    PlantUML:  {b:?}"
            ));
        }
    }
    if !lines.is_empty() {
        wrong.push(format!(
            "{name}: PlantUML reads another {what}\n{}\n{block}",
            lines.join("\n")
        ));
    }
}

/// `events [guard] / action`, as both sides mean it.
fn trigger(t: &Trigger) -> String {
    let mut text = t.events.join(", ");
    if let Some(guard) = &t.guard {
        text.push_str(&format!(" [{guard}]"));
    }
    if let Some(action) = &t.action {
        text.push_str(&format!(" / {action}"));
    }
    text.trim().to_owned()
}

/// A state as `Outer/Inner#1/Name`: the states around it, with the region
/// it sits in when that is not the first; a start, an end and a history
/// by their mark, a choice, fork or join with its stereotype.
fn state_key(path: &str, kind: Kind, name: &str) -> String {
    let named = |stereotype: &str, mark: &str| {
        if name.is_empty() {
            mark.to_owned()
        } else {
            format!("{name} <<{stereotype}>>")
        }
    };
    let token = match kind {
        Kind::State => name.to_owned(),
        Kind::Start => named("start", "(start)"),
        Kind::End => named("end", "(end)"),
        Kind::Choice => named("choice", "<<choice>>"),
        Kind::Fork => named("fork", "<<fork>>"),
        Kind::Join => named("join", "<<join>>"),
        Kind::History => "[H]".into(),
        Kind::DeepHistory => "[H*]".into(),
    };
    format!("{path}{token}")
}

fn from_machine(m: &Machine) -> Reading {
    let prefix = |s: usize| {
        let mut prefix = String::new();
        let mut at = s;
        while let Some(p) = m.states[at].parent {
            let region = match m.states[at].region {
                0 => String::new(),
                r => format!("#{r}"),
            };
            prefix = format!("{}{region}/{prefix}", m.states[p].name);
            at = p;
        }
        prefix
    };
    let keys: Vec<String> = (0..m.states.len())
        .map(|s| state_key(&prefix(s), m.states[s].kind, &m.states[s].name))
        .collect();
    let mut things = keys.clone();
    things.sort();
    things.dedup();
    let mut steps: Vec<String> = m
        .transitions
        .iter()
        .map(|t| {
            format!(
                "{} -> {} : {}",
                keys[t.from],
                keys[t.to],
                trigger(&t.trigger)
            )
        })
        .collect();
    steps.sort();
    Reading {
        things,
        steps,
        ..Reading::default()
    }
}

/// PlantUML's SCXML: `start…`, `end…`, `historical…` and `deephistory…`
/// followed by the id of the state around are its pseudo states, a
/// `CONC…` state holds a region after the first, a stereotype gives the
/// kind.
fn from_scxml(doc: &Document) -> Reading {
    /// The states inside `node`, at `path`; `parent_id` names their pseudo states.
    fn walk<'a>(
        node: Node<'a, 'a>,
        path: &str,
        parent_id: &str,
        keys: &mut HashMap<String, String>,
        notes: &mut HashSet<String>,
        transitions: &mut Vec<(String, Node<'a, 'a>)>,
    ) {
        let mut regions = 0;
        for child in node.children().filter(|c| c.has_tag_name("state")) {
            let id = child.attribute("id").unwrap_or_default();
            // A note is no part of the machine.
            if child.attribute("stereotype") == Some("note") {
                notes.insert(id.to_owned());
                continue;
            }
            if is_region(id) {
                regions += 1;
                let path = format!("{}#{regions}/", path.trim_end_matches('/'));
                walk(child, &path, id, keys, notes, transitions);
                continue;
            }
            let (kind, name) = pseudo(id, parent_id, child.attribute("stereotype"));
            keys.insert(id.to_owned(), state_key(path, kind, &name));
            for t in child.children().filter(|c| c.has_tag_name("transition")) {
                transitions.push((id.to_owned(), t));
            }
            walk(child, &format!("{path}{id}/"), id, keys, notes, transitions);
        }
    }
    let mut keys: HashMap<String, String> = HashMap::new();
    let mut notes = HashSet::new();
    let mut transitions = Vec::new();
    walk(
        doc.root_element(),
        "",
        "",
        &mut keys,
        &mut notes,
        &mut transitions,
    );

    let mut things: Vec<String> = keys.values().cloned().collect();
    things.sort();
    let mut steps: Vec<String> = transitions
        .iter()
        .filter(|(_, t)| !notes.contains(t.attribute("target").unwrap_or_default()))
        .map(|(from, t)| {
            let to = t.attribute("target").unwrap_or_default();
            let event = t.attribute("event").unwrap_or_default();
            let label = Trigger::parse(&event.replace('«', "<<").replace('»', ">>"));
            format!(
                "{} -> {} : {}",
                keys.get(from).map_or(from.as_str(), String::as_str),
                keys.get(to).map_or(to, String::as_str),
                trigger(&label)
            )
        })
        .collect();
    steps.sort();
    Reading {
        things,
        steps,
        ..Reading::default()
    }
}

fn is_region(id: &str) -> bool {
    id.strip_prefix("CONC")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The kind and name of an SCXML state inside the one with `parent_id`.
fn pseudo(id: &str, parent_id: &str, stereotype: Option<&str>) -> (Kind, String) {
    for (mark, kind) in [
        ("start", Kind::Start),
        ("end", Kind::End),
        ("historical", Kind::History),
        ("deephistory", Kind::DeepHistory),
    ] {
        if id.strip_prefix(mark) == Some(parent_id) {
            return (kind, String::new());
        }
    }
    let kind = match stereotype {
        Some("start") => Kind::Start,
        Some("end") => Kind::End,
        Some("choice") => Kind::Choice,
        Some("fork") => Kind::Fork,
        Some("join") => Kind::Join,
        Some("history") => Kind::History,
        Some("deepHistory") => Kind::DeepHistory,
        _ => Kind::State,
    };
    (kind, id.to_owned())
}

/// Text as both sides mean it: PlantUML's `\n` and line breaks as spaces,
/// runs of spaces as one, `«` and `»` as `<<` and `>>`, no spaces inside.
fn words(text: &str) -> String {
    text.replace("\\n", " ")
        .replace('«', "<<")
        .replace('»', ">>")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("<< ", "<<")
        .replace(" >>", ">>")
}

fn from_sequence(d: &sequence::Diagram) -> Reading {
    // PlantUML's XMI names a lifeline with its spot's letter and its
    // stereotype in front.
    let things: Vec<String> = d
        .participants
        .iter()
        .map(|p| {
            let spot = p.spot.as_ref().map(|s| s.letter.to_string());
            let stereotype = p.stereotype.as_ref().map(|s| format!("<<{s}>>"));
            let name = [spot, stereotype, Some(p.label.clone())];
            words(&name.into_iter().flatten().collect::<Vec<_>>().join(" "))
        })
        .collect();
    let end = |e: &End, edge: &str| match e {
        End::Participant(i) => things[*i].clone(),
        End::Edge(_) | End::Short(_) => edge.to_owned(),
    };
    let mut steps = Vec::new();
    let mut groups = Vec::new();
    // The groups around, outermost first: operator and part.
    let mut around: Vec<(String, usize)> = Vec::new();
    let mut guards: Vec<Vec<String>> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let text = |t: &Option<sequence::Text>| t.as_ref().map_or(String::new(), |t| words(&t.value));
    for item in &d.items {
        match item {
            Item::Message(m) => {
                let path: Vec<String> = around.iter().map(|(o, p)| format!("{o}#{p}")).collect();
                steps.push(format!(
                    "{} -> {} : {} @ {}",
                    end(&m.from, "["),
                    end(&m.to, "]"),
                    text(&m.text),
                    path.join("/")
                ));
            }
            Item::GroupStart {
                kind,
                label,
                secondary,
                ..
            } => {
                let (operator, guard) = match kind {
                    GroupKind::Group => (text(label), text(secondary)),
                    _ => (kind.keyword().to_owned(), text(label)),
                };
                open.push(groups.len());
                groups.push(format!("{}{operator}", "  ".repeat(around.len())));
                guards.push(vec![guard]);
                around.push((operator, 0));
            }
            Item::GroupElse { label, .. } => {
                if let (Some(top), Some(g)) = (around.last_mut(), guards.last_mut()) {
                    top.1 += 1;
                    g.push(text(label));
                }
            }
            Item::GroupEnd { .. } => {
                if let (Some(at), Some(g)) = (open.pop(), guards.pop()) {
                    groups[at].push_str(&format!(" [{}]", g.join(" | ")));
                }
                around.pop();
            }
            _ => {}
        }
    }
    Reading {
        things,
        steps,
        groups,
    }
}

/// PlantUML's XMI: lifelines in order; a message's ends are occurrences
/// covering a lifeline, and the first one of them, in order, says where
/// the message is and in which operand of which fragments.
fn from_xmi(doc: &Document) -> Reading {
    let interaction = doc
        .descendants()
        .find(|n| xmi(*n, "type") == "uml:Interaction")
        .expect("an interaction");

    let lifelines: BTreeMap<&str, String> = interaction
        .children()
        .filter(|n| n.has_tag_name("lifeline"))
        .map(|n| (xmi(n, "id"), words(n.attribute("name").unwrap_or_default())))
        .collect();
    let things: Vec<String> = interaction
        .children()
        .filter(|n| n.has_tag_name("lifeline"))
        .map(|n| words(n.attribute("name").unwrap_or_default()))
        .collect();

    // Occurrence id → the lifeline it covers.
    let mut covers: HashMap<&str, String> = HashMap::new();
    // Message id → where its first occurrence is.
    let mut first: Vec<(&str, String)> = Vec::new();
    let mut groups = Vec::new();
    for node in interaction.descendants() {
        match xmi(node, "type") {
            "uml:MessageOccurrenceSpecification" => {
                let covered = node.attribute("covered").unwrap_or_default();
                covers.insert(
                    xmi(node, "id"),
                    lifelines.get(covered).cloned().unwrap_or_default(),
                );
                let message = node.attribute("message").unwrap_or_default();
                if !first.iter().any(|(m, _)| *m == message) {
                    first.push((message, operands(node)));
                }
            }
            "uml:CombinedFragment" => {
                let depth = node
                    .ancestors()
                    .skip(1)
                    .filter(|a| xmi(*a, "type") == "uml:CombinedFragment")
                    .count();
                let guards: Vec<String> = node
                    .children()
                    .filter(|c| c.has_tag_name("operand"))
                    .map(|o| {
                        o.descendants()
                            .find(|s| s.has_tag_name("specification"))
                            .and_then(|s| s.attribute("value"))
                            .map_or(String::new(), words)
                    })
                    .collect();
                groups.push(format!(
                    "{}{} [{}]",
                    "  ".repeat(depth),
                    words(node.attribute("interactionOperator").unwrap_or_default()),
                    guards.join(" | ")
                ));
            }
            _ => {}
        }
    }
    let messages: HashMap<&str, Node> = interaction
        .children()
        .filter(|n| n.has_tag_name("message"))
        .map(|n| (xmi(n, "id"), n))
        .collect();
    let steps = first
        .iter()
        .filter_map(|(id, path)| {
            let m = messages.get(id)?;
            let end = |attr: &str, edge: &str| {
                m.attribute(attr)
                    .and_then(|o| covers.get(o).cloned())
                    .unwrap_or_else(|| edge.to_owned())
            };
            Some(format!(
                "{} -> {} : {} @ {path}",
                end("sendEvent", "["),
                end("receiveEvent", "]"),
                words(m.attribute("name").unwrap_or_default())
            ))
        })
        .collect();
    Reading {
        things,
        steps,
        groups,
    }
}

/// An `xmi:` attribute of `node`, or "".
fn xmi<'a>(node: Node<'a, '_>, name: &str) -> &'a str {
    node.attribute(("http://schema.omg.org/spec/XMI/2.1", name))
        .unwrap_or_default()
}

/// The operands around an XMI node, outermost first, as `operator#part`.
fn operands(node: Node) -> String {
    let mut path: Vec<String> = node
        .ancestors()
        .filter(|a| a.has_tag_name("operand"))
        .filter_map(|operand| {
            let fragment = operand.parent()?;
            let part = fragment
                .children()
                .filter(|c| c.has_tag_name("operand"))
                .position(|c| c == operand)?;
            Some(format!(
                "{}#{part}",
                words(
                    fragment
                        .attribute("interactionOperator")
                        .unwrap_or_default()
                )
            ))
        })
        .collect();
    path.reverse();
    path.join("/")
}
