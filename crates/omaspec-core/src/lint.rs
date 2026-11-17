//! Findings while typing: syntax problems that stop a diagram from drawing
//! right, and hints for likely typos.
//!
//! Syntax: missing `@end…`, unclosed blocks and notes, an unknown Mermaid
//! header. Hints: a message to a name close to a declared participant (both
//! engines would silently draw a new one), duplicate and unused participants,
//! swimlanes that differ only in case or spaces, a state name seen once next
//! to a frequent one, an entry of the lines by hand that does not read or
//! comes twice, two colors the writer set that hardly differ
//! ([`crate::faint`]), a line of a Mermaid flowchart that mermaid.js would
//! not read ([`portable`]), an Open Iconic name the set lacks
//! ([`icons`]), a PlantUML line Omaspec's own engine does not draw yet, so
//! only the Java PlantUML draws the diagram, and one it refuses as PlantUML
//! does ([`engine`]).

mod fix;
mod icons;
mod namesakes;
mod portable;
mod written;

pub use portable::portable;

use std::collections::{HashMap, HashSet};

use omaspec_uml::{Mention, element_lines};
use regex::Regex;
use serde::Serialize;

use crate::kind::{Kind, classify};
use crate::lang::{Language, mermaid_graph, mermaid_head, puml_start};
use crate::text::{is_note_block_open, is_note_closer, is_note_one_line, normalize_newlines, re};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Worth a look.
    Info,
    /// Probably a mistake.
    Warning,
    /// The diagram will not draw as written.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// What kind of finding it is, for a program that reads the report:
    /// a short name that stays while the message's words may change
    /// (`dead-end`, `unreachable-state`; the guide lists them).
    pub code: &'static str,
    /// Zero-based line.
    pub line: usize,
    pub severity: Severity,
    pub message: String,
    /// How to put it right, where that is sure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
    /// That only the Java PlantUML draws the diagram: a hint, which
    /// `--strict` holds as a failure ([`only_java`]).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub java_only: bool,
}

impl Finding {
    pub fn new(
        code: &'static str,
        line: usize,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            line,
            severity,
            message: message.into(),
            fix: None,
            java_only: false,
        }
    }

    /// The hint that only the Java PlantUML draws the diagram, for
    /// `reason` on `line`.
    fn java(line: usize, reason: &str) -> Self {
        Self {
            java_only: true,
            ..Self::new(
                "java-only",
                line,
                Severity::Info,
                format!("{reason}: {JAVA_ONLY}"),
            )
        }
    }

    /// The finding with the way to put it right.
    #[must_use]
    pub fn with(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }
}

pub use fix::{Fix, fix_all};

/// All findings for `code`, in line order.
pub fn lint(code: &str, lang: Language) -> Vec<Finding> {
    let code = normalize_newlines(code);
    let lines: Vec<&str> = code.split('\n').collect();
    let mut findings = match lang {
        Language::Mermaid => mermaid_syntax(&lines),
        Language::PlantUml => plantuml_syntax(&lines),
    };
    lines_by_hand(&code, &mut findings);
    if lang == Language::PlantUml {
        icons::findings(&lines, &mut findings);
    }
    let faint = match lang {
        Language::PlantUml => crate::faint::plantuml(&lines),
        Language::Mermaid => crate::faint::mermaid(&lines),
    };
    findings.extend(
        faint
            .into_iter()
            .map(|f| Finding::new("faint-color", f.line, Severity::Warning, f.message)),
    );
    let kind = classify(&code, lang).kind;
    match lang {
        Language::Mermaid => {
            findings.extend(crate::settings::unheeded(&code));
            let rows = mermaid_rows(&lines);
            if portable::is_flowchart(&code) {
                portable::findings(&lines, &rows, &mut findings);
            }
            // The engine's reader says where each name is written; the
            // lines are read by pattern where it declines.
            match kind {
                Kind::Sequence => {
                    let names = omaspec_uml::sequence::from_mermaid(&code)
                        .map_or_else(|| written::mermaid_sequence(&rows), |d| d.names);
                    sequence(&names, &mut findings);
                }
                Kind::State => {
                    let names = omaspec_uml::state::mermaid::read(&code).map_or_else(
                        || {
                            written::states(
                                &rows,
                                re!(r"^([\w.]+|\[\*\])\s*-->\s*([\w.]+|\[\*\])"),
                                re!(r#"^state\s+(?:"[^"]*"\s+as\s+)?([\w.]+)"#),
                            )
                        },
                        |chart| chart.names,
                    );
                    state_names(&names, &mut findings);
                }
                _ => {}
            }
        }
        // The preprocessor can write, hide and repeat lines, so the rules
        // about names hold back once it runs, and the engine's word with
        // them ([`drawing`] has it, where the preprocessor has run).
        Language::PlantUml if lines.iter().any(|l| l.trim_start().starts_with('!')) => {}
        Language::PlantUml => {
            let read = omaspec_uml::parse(&code);
            plantuml_names(&code, kind, read.as_ref().ok(), &mut findings);
            // A line the rules already name needs no second word.
            if let Some(said) = read.err().and_then(said)
                && !findings
                    .iter()
                    .any(|f| f.line == said.line && f.severity >= Severity::Warning)
            {
                findings.push(said);
            }
        }
    }
    findings.sort_by_key(|f| f.line);
    findings
}

/// The rules about the names of a PlantUML diagram of `kind`. Where each
/// name is written says the engine's reader (`read`, what it made of the
/// text); the lines are read by pattern where it declined.
fn plantuml_names(
    code: &str,
    kind: Kind,
    read: Option<&omaspec_uml::Diagram>,
    findings: &mut Vec<Finding>,
) {
    let rows = || -> Vec<Row> {
        element_lines(code)
            .unwrap_or_default()
            .into_iter()
            .map(|line| Row {
                row: line.no,
                text: line.text,
            })
            .collect()
    };
    match (kind, read) {
        (Kind::Sequence, Some(omaspec_uml::Diagram::Sequence(d))) => sequence(&d.names, findings),
        (Kind::Sequence, _) => sequence(&written::plantuml_sequence(&rows()), findings),
        (Kind::Activity, _) => swimlanes(&rows(), findings),
        (Kind::State, read) => {
            let by_pattern;
            let names = match read {
                Some(omaspec_uml::Diagram::State(d)) => &d.names,
                _ => {
                    by_pattern = written::states(
                        &rows(),
                        re!(
                            r#"^("[^"]+"|[\w.]+|\[\*\])\s*-[-\w\[\],#]*>\s*("[^"]+"|[\w.]+|\[\*\])"#
                        ),
                        re!(r#"(?i)^state\s+(?:"[^"]*"\s+as\s+)?([\w.]+)"#),
                    );
                    &by_pattern
                }
            };
            state_names(names, findings);
            namesakes::namesakes(names, findings);
        }
        _ => {}
    }
}

/// What Omaspec's own engine says when it draws `code`, beyond what its
/// reading says: a skinparam a drawer does not draw, a diagram the
/// preprocessor writes. Too slow for every key typed, it is for `lint`
/// and `check` on the command line. A diagram that includes others is
/// left alone: where they are is not known here.
pub fn drawing(code: &str) -> Option<Finding> {
    let includes = code.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("!include") || line.starts_with("!import")
    });
    if includes {
        return None;
    }
    said(omaspec_uml::render(code).err()?)
}

/// A refusal of the engine as a finding. A mistake PlantUML refuses too
/// is an error. Anything else is drawn by the Java PlantUML where it is
/// installed, and not at all where it is not: worth knowing before the
/// diagram goes where Java is not.
fn said(declined: omaspec_uml::Declined) -> Option<Finding> {
    let line = declined.line.unwrap_or(0);
    Some(if declined.refused {
        Finding::new("refused", line, Severity::Error, declined.reason)
    } else {
        Finding::java(line, &declined.reason)
    })
}

/// Whether `finding` says that only the Java PlantUML draws its diagram:
/// a hint, which `--strict` holds as a failure, since such a diagram is
/// not drawn where Java is not.
pub fn only_java(finding: &Finding) -> bool {
    finding.java_only
}

/// How [`engine`]'s finding ends when only Java draws the diagram.
const JAVA_ONLY: &str = "only the Java PlantUML draws this diagram";

/// Entries of the block of lines by hand that do not read, and a second
/// entry for the same line or the same box: the drawing leaves both out.
fn lines_by_hand(code: &str, findings: &mut Vec<Finding>) {
    use omaspec_uml::hand::Hand;
    let Some(block) = omaspec_uml::hand::read(code) else {
        return;
    };
    let mut seen = HashSet::new();
    let mut held = HashSet::new();
    let mut labelled = HashSet::new();
    let mut whole = false;
    let mut boxes = HashSet::new();
    for entry in &block.entries {
        let said = match &entry.read {
            Err(why) => Some(format!("{}. This entry is left out", capitalized(why))),
            Ok(Hand::Line(line)) if !seen.insert(line.key.clone()) => Some(format!(
                "{} has its shape above already; this one is left out",
                line.key
            )),
            Ok(Hand::Hold(hold)) if !held.insert(hold.name.clone()) => Some(format!(
                "{} is held in place above already; this entry is left out",
                hold.name
            )),
            Ok(Hand::Label(label)) if !labelled.insert(label.key.clone()) => Some(format!(
                "the label of {} has its side above already; this entry is left out",
                label.key
            )),
            Ok(Hand::Apart(apart)) => match &apart.at {
                None if whole => {
                    Some("the lines stay apart above already; this entry is left out".to_owned())
                }
                None => {
                    whole = true;
                    None
                }
                Some(name) if !boxes.insert(name.clone()) => Some(format!(
                    "{name} stays apart above already; this entry is left out"
                )),
                Some(_) => None,
            },
            Ok(_) => None,
        };
        if let Some(said) = said {
            findings.push(
                Finding::new("stale-entry", entry.line, Severity::Warning, said)
                    .with(Fix::remove("Take the entry out", entry.line)),
            );
        }
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// A line without string contents and without a `'` comment, trimmed.
fn strip_noise(line: &str) -> String {
    let s = crate::text::strip_strings(line);
    if s.trim_start().starts_with('\'') {
        String::new()
    } else {
        s.trim().to_owned()
    }
}

fn first_word(t: &str) -> String {
    t.split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

/// Removes the innermost open block that `matches`.
fn close(stack: &mut Vec<(usize, String)>, matches: impl Fn(&str) -> bool) {
    if let Some(i) = stack.iter().rposition(|(_, kind)| matches(kind)) {
        stack.remove(i);
    }
}

fn mermaid_syntax(lines: &[&str]) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut i = 0;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    if lines.get(i).is_some_and(|l| l.trim() == "---") {
        i += 1;
        while i < lines.len() && lines[i].trim() != "---" {
            i += 1;
        }
        if i < lines.len() {
            i += 1;
        }
    }
    // Skip comments and the init directive, which may span lines.
    while i < lines.len() {
        let t = lines[i].trim();
        if t.starts_with("%%{") {
            while i < lines.len() && !lines[i].contains("}%%") {
                i += 1;
            }
            i += 1;
            continue;
        }
        if t.is_empty() || t.starts_with("%%") {
            i += 1;
            continue;
        }
        break;
    }
    if let Some(head) = lines.get(i).map(|l| l.trim())
        && !puml_start().is_match(head)
        && !mermaid_head().is_match(head)
        && !mermaid_graph().is_match(head)
    {
        findings.push(Finding::new(
            "unknown-head",
            i,
            Severity::Warning,
            "Unknown diagram head — expected flowchart / sequenceDiagram / …",
        ));
    }

    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut in_init = false;
    for (row, raw) in lines.iter().enumerate() {
        if raw.trim().starts_with("%%{") {
            in_init = true;
        }
        if in_init {
            if raw.contains("}%%") {
                in_init = false;
            }
            continue;
        }
        let t = strip_noise(raw);
        if t.is_empty() || t.starts_with("%%") {
            continue;
        }
        if re!(r"(?i)^(subgraph|loop|alt|opt|par|critical|break|rect|box)\b").is_match(&t) {
            stack.push((row, first_word(&t)));
        } else if re!(r"(?i)^note\b").is_match(&t) && !t.contains(':') {
            stack.push((row, "note".into()));
        } else if re!(r"(?i)^end(?:note)?\b").is_match(&t) || re!(r"^\}\s*$").is_match(&t) {
            stack.pop();
        }
    }
    for (row, kind) in stack {
        findings.push(Finding::new(
            "unclosed-block",
            row,
            Severity::Warning,
            format!("Unclosed {kind}"),
        ));
    }
    findings
}

fn plantuml_syntax(lines: &[&str]) -> Vec<Finding> {
    let mut findings = Vec::new();

    let mut starts: Vec<(usize, String)> = Vec::new();
    for (row, line) in lines.iter().enumerate() {
        let t = line.trim();
        if let Some(c) = re!(r"(?i)^@start([A-Za-z]+)\b").captures(t) {
            starts.push((row, c[1].to_lowercase()));
        }
        if let Some(c) = re!(r"(?i)^@end([A-Za-z]+)\b").captures(t) {
            let kind = c[1].to_lowercase();
            close(&mut starts, |k| k == kind);
        }
    }
    // After the last line; the empty end a final line break leaves is none.
    let end = match lines.last() {
        Some(last) if last.is_empty() && lines.len() > 1 => lines.len() - 1,
        _ => lines.len(),
    };
    for (row, kind) in starts {
        findings.push(
            Finding::new(
                "missing-end",
                row,
                Severity::Error,
                format!("@start{kind} without matching @end{kind}"),
            )
            .with(Fix::insert(
                format!("Add @end{kind} at the end"),
                end,
                format!("@end{kind}"),
            )),
        );
    }

    let is_group = |k: &str| matches!(k, "alt" | "opt" | "loop" | "par" | "critical" | "group");
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut in_block_comment = false;
    // Selectors in a <style> block look like keywords (`note {`).
    let mut in_style = false;
    for (row, raw) in lines.iter().enumerate() {
        if in_block_comment {
            if raw.contains("'/") {
                in_block_comment = false;
            }
            continue;
        }
        if in_style || re!(r"(?i)^\s*<style>").is_match(raw) {
            in_style = !re!(r"(?i)</style>").is_match(raw);
            continue;
        }
        if raw.trim_start().starts_with("/'") && !raw.contains("'/") {
            in_block_comment = true;
            continue;
        }
        let t = strip_noise(raw);
        // Inside a note only its end counts: its text may say anything.
        if stack.last().is_some_and(|(_, kind)| kind == "note") {
            if is_note_closer(&t) {
                close(&mut stack, |k| k == "note");
            }
            continue;
        }
        if t.is_empty()
            || puml_start().is_match(&t)
            || re!(r"(?i)^@end").is_match(&t)
            || re!(r"(?i)^(else(?:if)?|elseif)\b").is_match(&t)
            || is_note_one_line(&t)
        {
            continue;
        }
        // `Critical --> Landed` or `Loop : entry / x` names a state or a
        // participant: an arrow or a description follows, not a label.
        if let Some(c) = re!(r"(?i)^(alt|opt|loop|par|critical|group|box)\b").captures(&t)
            && !re!(r"^\s*[-<.:]").is_match(&t[c[0].len()..])
        {
            // `alt#Gold` is an `alt`; `group Name {` closes with `}`.
            let mut kind = c[1].to_lowercase();
            if t.ends_with('{') {
                kind.push_str(" {");
            }
            stack.push((row, kind));
        } else if is_note_block_open(&t) {
            stack.push((row, "note".into()));
        } else if is_note_closer(&t) {
            close(&mut stack, |k| k == "note");
        } else if re!(r"(?i)^end\s*box\b").is_match(&t) {
            close(&mut stack, |k| k == "box");
        } else if re!(
            r"(?i)^end\s*(?:if|while|switch|legend|title|header|footer|caption|ref|fork|split)\b"
        )
        .is_match(&t)
        {
            // Activity and text blocks close without a matching opener here.
        } else if let Some(c) = re!(r"(?i)^end\s+(alt|opt|loop|par|critical|group)\b").captures(&t)
        {
            let kind = c[1].to_lowercase();
            close(&mut stack, |k| k == kind);
        } else if re!(r"(?i)^end\s*$").is_match(&t) {
            close(&mut stack, is_group);
        } else if let Some(c) = re!(r"(?i)^state\s+(.*?)\s*\{$").captures(&t) {
            // PlantUML ends a state without its `}` with the diagram.
            stack.push((row, format!("state {} {{", declared_state(&c[1]))));
        } else if re!(r"(?i)^skinparam\b.*\{$").is_match(&t) {
            stack.push((row, "skinparam {".into()));
        } else if t == "}" {
            close(&mut stack, |k| k.ends_with(" {"));
        }
    }
    for (row, kind) in stack {
        findings.push(Finding::new(
            "unclosed-block",
            row,
            Severity::Warning,
            format!("Unclosed {kind}"),
        ));
    }
    findings
}

/// The name a `state … {` line declares: its alias, else its name.
fn declared_state(decl: &str) -> &str {
    let decl = decl.trim();
    if let Some((_, alias)) = decl.split_once(" as ") {
        return alias.split_whitespace().next().unwrap_or(alias);
    }
    match decl.strip_prefix('"') {
        Some(quoted) => quoted.split('"').next().unwrap_or(quoted),
        None => decl.split_whitespace().next().unwrap_or(decl),
    }
}

/// A meaningful line and its zero-based number.
struct Row<'a> {
    row: usize,
    text: &'a str,
}

/// Mermaid lines after the header, without comments, fences and front matter.
fn mermaid_rows<'a>(lines: &[&'a str]) -> Vec<Row<'a>> {
    let mut rows = Vec::new();
    let mut head = false;
    let mut front = false;
    let mut in_init = false;
    for (row, raw) in lines.iter().enumerate() {
        let l = raw.trim();
        if in_init {
            in_init = !l.contains("}%%");
            continue;
        }
        if l.starts_with("%%{") {
            in_init = !l.contains("}%%");
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
            head = true;
            continue;
        }
        rows.push(Row { row, text: l });
    }
    rows
}

/// The rules about a sequence diagram's participants, from where each
/// name is written.
fn sequence(names: &[Mention], findings: &mut Vec<Finding>) {
    let decls: Vec<&Mention> = names.iter().filter(|m| m.declares).collect();
    // Name → index of the declaration that owns it.
    let mut declared: HashMap<&str, usize> = HashMap::new();
    for (i, d) in decls.iter().enumerate() {
        match declared.get(d.name.as_str()) {
            Some(&first) => findings.push(Finding::new(
                "declared-twice",
                d.line,
                Severity::Warning,
                format!(
                    "{} is declared twice (first {}).",
                    d.name,
                    crate::lines_away(d.line, decls[first].line)
                ),
            )),
            None => {
                declared.insert(&d.name, i);
            }
        }
        if !d.shown.is_empty() && d.shown != d.name {
            declared.entry(&d.shown).or_insert(i);
        }
    }
    let keys: Vec<&str> = decls.iter().map(|d| d.name.as_str()).collect();
    let near = Near::new(&keys);
    let mut used: HashSet<&str> = HashSet::new();
    let mut warned: HashSet<&str> = HashSet::new();
    for m in names.iter().filter(|m| !m.declares) {
        let name = m.name.as_str();
        used.insert(name);
        if declared.contains_key(name) || decls.is_empty() || warned.contains(name) {
            continue;
        }
        let Some(near) = near.of(name) else {
            continue;
        };
        warned.insert(name);
        findings.push(Finding::new(
            "undeclared-participant",
            m.line,
            Severity::Warning,
            format!(
                "{name} is not declared. Did you mean {near}? PlantUML and Mermaid would draw a new participant."
            ),
        ));
    }
    for (i, d) in decls.iter().enumerate() {
        if declared.get(d.name.as_str()) != Some(&i) {
            continue;
        }
        if !used.contains(d.name.as_str())
            && !(!d.shown.is_empty() && used.contains(d.shown.as_str()))
        {
            findings.push(Finding::new(
                "unused-participant",
                d.line,
                Severity::Info,
                format!(
                    "{} is declared but never sends or receives a message.",
                    d.name
                ),
            ));
        }
    }
}

/// A swimlane, `|Lane|` or `|#color|Lane|`, with its name.
fn puml_swimlane() -> &'static Regex {
    re!(r"^\|(?:#[^|]*\|)?([^|]+)\|")
}

fn swimlanes(rows: &[Row], findings: &mut Vec<Finding>) {
    // Normalized name → (spelling, row) in order of appearance.
    let mut lanes: Vec<(String, Vec<(&str, usize)>)> = Vec::new();
    for r in rows {
        let Some(c) = puml_swimlane().captures(r.text) else {
            continue;
        };
        let raw = c.get(1).map_or("", |m| m.as_str());
        let norm = re!(r"\s+").replace_all(raw.trim(), " ").to_lowercase();
        match lanes.iter_mut().find(|(n, _)| *n == norm) {
            Some((_, list)) => list.push((raw, r.row)),
            None => lanes.push((norm, vec![(raw, r.row)])),
        }
    }
    for (_, list) in &lanes {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for &(raw, _) in list {
            match counts.iter_mut().find(|(s, _)| *s == raw) {
                Some((_, n)) => *n += 1,
                None => counts.push((raw, 1)),
            }
        }
        if counts.len() < 2 {
            continue;
        }
        counts.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        let main = counts[0].0;
        for &(raw, row) in list {
            if raw != main {
                findings.push(Finding::new(
                    "lane-spelling",
                    row,
                    Severity::Warning,
                    format!(
                        "Lane \"{raw}\" differs from \"{main}\" only in case or spaces. PlantUML draws two lanes."
                    ),
                ));
            }
        }
    }
}

/// A state named once whose name is near one named often: a typo, mostly.
fn state_names(names: &[Mention], findings: &mut Vec<Finding>) {
    // State name → rows it appears on, in order of first appearance.
    let mut seen: Vec<(&str, Vec<usize>)> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    for m in names {
        let name = m.name.as_str();
        if name.is_empty() || name == "[*]" {
            continue;
        }
        match index.get(name) {
            Some(&i) => seen[i].1.push(m.line),
            None => {
                index.insert(name, seen.len());
                seen.push((name, vec![m.line]));
            }
        }
    }
    let frequent: Vec<&str> = seen
        .iter()
        .filter(|(_, rows)| rows.len() > 1)
        .map(|(n, _)| *n)
        .collect();
    let near = Near::new(&frequent);
    for (name, rows) in &seen {
        if rows.len() != 1 || name.chars().count() < 4 {
            continue;
        }
        if let Some(near) = near.of(name) {
            findings.push(Finding::new(
                "lone-state",
                rows[0],
                Severity::Info,
                format!("State {name} appears only here. Did you mean {near}?"),
            ));
        }
    }
}

/// Names a typo may be close to, prepared once, so that a diagram with
/// thousands of names stays quick: only names of a near length are
/// compared, and only those whose letters are near enough get the full
/// edit distance.
struct Near<'a> {
    names: Vec<Candidate<'a>>,
    /// Length in characters → indexes into `names`, in order.
    by_length: HashMap<usize, Vec<usize>>,
}

struct Candidate<'a> {
    name: &'a str,
    /// Lower case.
    low: Vec<char>,
    /// `low`, sorted.
    letters: Vec<char>,
}

impl<'a> Near<'a> {
    fn new(candidates: &[&'a str]) -> Self {
        let mut by_length: HashMap<usize, Vec<usize>> = HashMap::new();
        let names: Vec<Candidate> = candidates
            .iter()
            .enumerate()
            .map(|(i, &name)| {
                let low: Vec<char> = name.to_lowercase().chars().collect();
                by_length.entry(low.len()).or_default().push(i);
                let mut letters = low.clone();
                letters.sort_unstable();
                Candidate { name, low, letters }
            })
            .collect();
        Self { names, by_length }
    }

    /// The first candidate that differs from `name` only in case or by a
    /// small edit distance (1, or 2 for names longer than five characters).
    fn of(&self, name: &str) -> Option<&'a str> {
        let low: Vec<char> = name.to_lowercase().chars().collect();
        if low.len() < 3 {
            return None;
        }
        let max = if low.len() > 5 { 2 } else { 1 };
        let mut letters = low.clone();
        letters.sort_unstable();
        (low.len().saturating_sub(max)..=low.len() + max)
            .filter_map(|length| self.by_length.get(&length))
            .flatten()
            .copied()
            .filter(|&i| {
                let c = &self.names[i];
                c.name != name
                    && (c.low == low
                        || (c.low.len() >= 3
                            && unshared(&letters, &c.letters) <= max
                            && distance(&low, &c.low, max) <= max))
            })
            .min()
            .map(|i| self.names[i].name)
    }
}

/// How many letters of one sorted list the other lacks, the larger count
/// of the two. An edit changes at most one letter on each side, so the
/// edit distance is at least this.
fn unshared(a: &[char], b: &[char]) -> usize {
    let (mut i, mut j, mut shared) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                shared += 1;
                i += 1;
                j += 1;
            }
        }
    }
    (a.len() - shared).max(b.len() - shared)
}

/// Edit distance counting a swap of two neighbouring characters as one edit
/// (optimal string alignment), but only up to `max`; larger distances return
/// `max + 1`. `Auht` is one edit from `Auth`.
fn distance(a: &[char], b: &[char], max: usize) -> usize {
    if a.len().abs_diff(b.len()) > max {
        return max + 1;
    }
    let mut before: Vec<usize> = Vec::new();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut row = vec![i];
        let mut best = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (prev[j] + 1).min(row[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(before[j - 2] + 1);
            }
            row.push(v);
            best = best.min(v);
        }
        // A row's minimum never drops by more than one per row, and a swap
        // reaches back only one row, so the distance can only grow from here.
        if best > max {
            return max + 1;
        }
        before = std::mem::replace(&mut prev, row);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(code: &str, lang: Language) -> Vec<(usize, Severity, String)> {
        lint(code, lang)
            .into_iter()
            .map(|f| (f.line, f.severity, f.message))
            .collect()
    }

    fn codes(code: &str, lang: Language) -> Vec<(usize, &'static str)> {
        lint(code, lang)
            .into_iter()
            .map(|f| (f.line, f.code))
            .collect()
    }

    /// The rules about names read what the engine read, so they know
    /// every line that names something, not the two a pattern knew.
    #[test]
    fn the_names_are_the_ones_the_engine_read() {
        // A transition written backwards and a description name the state
        // too: `Runing` is the one written once.
        let state = "@startuml\n[*] --> Running\nStopped <-- Running : stop\nStopped : at rest\nRuning --> Stopped\n@enduml\n";
        assert_eq!(codes(state, Language::PlantUml), [(4, "lone-state")]);
        // A note and an activation use a participant as a message does.
        let sequence = "@startuml\nparticipant Alice\nparticipant Bob\nparticipant Carol\nAlice -> Bob : hi\nnote over Carol : waits\nactivate Alise\n@enduml\n";
        assert_eq!(
            codes(sequence, Language::PlantUml),
            [(6, "undeclared-participant")]
        );
        let mermaid = "sequenceDiagram\n  participant Alice\n  participant Bob\n  participant Alice\n  Alice->>Bobb: hi\n";
        assert_eq!(
            codes(mermaid, Language::Mermaid),
            [
                (2, "unused-participant"),
                (3, "declared-twice"),
                (4, "undeclared-participant")
            ]
        );
        let chart = "stateDiagram-v2\n  [*] --> Running\n  Running --> Stopped\n  Stopped: at rest\n  note right of Stopped: done\n  Runing --> Stopped\n";
        assert_eq!(codes(chart, Language::Mermaid), [(5, "lone-state")]);
        // A note names its state in PlantUML as well.
        let noted = "@startuml\n[*] --> Running\nRunning --> Runing\nnote right of Runing : on purpose\nRunning --> [*]\n@enduml\n";
        assert_eq!(codes(noted, Language::PlantUml), []);
        // `create` declares the participant it creates.
        let created = "@startuml\nparticipant Alice\nparticipant Bob\nAlice -> Bob : hi\ncreate Bobb\nAlice -> Bobb : new\n@enduml\n";
        assert_eq!(codes(created, Language::PlantUml), []);
    }

    /// A diagram the engine declines keeps its rules about names: its
    /// lines are read by pattern.
    #[test]
    fn a_declined_diagram_has_its_names_read_by_pattern() {
        let state = "@startuml\n[*] --> Running\nRunning --> Stopped\nStopped --> Running\nRuning --> Stopped\nnewpage\n@enduml\n";
        assert!(omaspec_uml::parse(state).is_err());
        let found = codes(state, Language::PlantUml);
        assert!(found.contains(&(4, "lone-state")), "{found:?}");
        let sequence = "@startuml\nparticipant Alice\nparticipant Bob\nAlice -> Bobb : hi\nautoactivate on\n@enduml\n";
        assert!(omaspec_uml::parse(sequence).is_err());
        let found = codes(sequence, Language::PlantUml);
        assert!(found.contains(&(3, "undeclared-participant")), "{found:?}");
        assert!(found.contains(&(2, "unused-participant")), "{found:?}");
    }

    #[test]
    fn lines_by_hand_that_do_not_read_or_come_twice() {
        let code = "@startuml\nA --> B\n\n' lines by hand (Omaspec)\n' A -> B at 0.5 -60\n' A -> B at 0.2 1\n' B -> A at 0.5\n' C after A\n' C before B\n' C after\n' apart\n' apart\n' apart at Idle\n' apart at Idle\n@enduml\n";
        assert_eq!(
            messages(code, Language::PlantUml),
            [
                (
                    5,
                    Severity::Warning,
                    "A -> B has its shape above already; this one is left out".into()
                ),
                (
                    6,
                    Severity::Warning,
                    "A corner is two numbers, across and down. This entry is left out".into()
                ),
                (
                    8,
                    Severity::Warning,
                    "C is held in place above already; this entry is left out".into()
                ),
                (
                    9,
                    Severity::Warning,
                    "A box held in place reads `Box after Neighbor`, `before`, `below`, \
                     `above`, `right of`, `left of` or `level with`. This entry is left out"
                        .into()
                ),
                (
                    11,
                    Severity::Warning,
                    "the lines stay apart above already; this entry is left out".into()
                ),
                (
                    13,
                    Severity::Warning,
                    "Idle stays apart above already; this entry is left out".into()
                ),
            ]
        );
        let flow =
            "flowchart LR\n  A --> B\n  %% lines by hand (Omaspec)\n  %% A -> B at 0.5 -60\n";
        assert!(messages(flow, Language::Mermaid).is_empty());
    }

    /// The hint on a Salt window, which only Java draws.
    fn found_java() -> Finding {
        lint(
            "@startsalt\n{S\n  Line one\n}\n@endsalt\n",
            Language::PlantUml,
        )
        .remove(0)
    }

    /// The findings of `code` but the note that only Java draws it.
    fn but_java(code: &str) -> Vec<(usize, Severity, String)> {
        messages(code, Language::PlantUml)
            .into_iter()
            .filter(|(_, _, message)| !message.ends_with(JAVA_ONLY))
            .collect()
    }

    #[test]
    fn a_line_only_java_draws_is_named() {
        let found = messages(
            "@startsalt\n{S\n  Line one\n}\n@endsalt\n",
            Language::PlantUml,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].0, found[0].1), (1, Severity::Info));
        assert!(found[0].2.ends_with(JAVA_ONLY), "{found:?}");
        assert!(only_java(&found_java()));
        // Said in other words, a hint is no such finding.
        assert!(!only_java(&Finding::new(
            "refused",
            1,
            Severity::Info,
            found[0].2.clone()
        )));
        // Drawing says what reading does not: a skinparam no drawer draws,
        // a diagram the preprocessor writes. One that includes others is
        // left alone.
        let skin = "@startuml\nskinparam componentStyle fancy\n[Web] --> [Api]\n@enduml\n";
        assert!(lint(skin, Language::PlantUml).iter().all(|f| !only_java(f)));
        assert!(drawing(skin).is_some_and(|f| only_java(&f) && f.line == 1));
        let written = "@startsalt\n!$a = 1\n{S\n  Line one\n}\n@endsalt\n";
        assert!(drawing(written).is_some_and(|f| only_java(&f)));
        assert!(drawing("@startuml\n!include other.puml\nA -> B\n@enduml\n").is_none());
        assert!(drawing("@startuml\nA -> B\n@enduml\n").is_none());
        // What Omaspec draws itself says nothing.
        assert!(but_java("@startuml\nA -> B : hi\n@enduml\n").is_empty());
        // What PlantUML refuses too is an error: a rule PlantUML reads as
        // the picture's header.
        let found = messages(
            "@startebnf\nheader = \"x\", body ;\nbody = \"y\" ;\n@endebnf\n",
            Language::PlantUml,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].0, found[0].1), (1, Severity::Error));
        assert!(found[0].2.contains("named `header`"), "{found:?}");
        assert!(messages("@startuml\nA -> B : hi\n@enduml\n", Language::PlantUml).is_empty());
    }

    /// From PlantUML's own examples: blocks that are closed after all.
    #[test]
    fn closed_blocks_are_not_reported() {
        for code in [
            "@startuml\nstate A\nnote right of A\n note\n on A\nend note\n@enduml",
            "@startuml\nstart\ngroup Init {\n  :read;\n}\nstop\n@enduml",
            "@startuml\nA -> B : hi\nalt#Gold #LightBlue ok\n  B -> A : yes\nelse #Pink no\n  B -> A : no\nend\n@enduml",
            "@startgantt\n[a] requires 2 days\nnote bottom\n  note for a\nend note\n@endgantt",
        ] {
            // Group colors only the Java PlantUML draws, which says so.
            let found = but_java(code);
            assert!(found.is_empty(), "{code:?}: {found:?}");
        }
        let found = messages(
            "@startuml\nstart\ngroup Init {\n  :read;\nstop\n@enduml",
            Language::PlantUml,
        );
        assert_eq!(found, [(2, Severity::Warning, "Unclosed group {".into())]);
        // A `}` closes the innermost state, so the outer one is left
        // open; a skin parameter's braces are no state's.
        let found = messages(
            "@startuml\nstate \"Long name\" as L {\n  state P #pink {\n    [*] --> X\n  skinparam state {\n    BackgroundColor red\n  }\n}\n@enduml",
            Language::PlantUml,
        );
        assert_eq!(found, [(1, Severity::Warning, "Unclosed state L {".into())]);
        assert!(
            messages(
                "@startuml\nstate P{\n[*] --> X\n}\n@enduml",
                Language::PlantUml
            )
            .is_empty()
        );
    }

    #[test]
    fn missing_end_and_unclosed_blocks() {
        let found = messages("@startuml\nalt x\nnote left\nA -> B\n", Language::PlantUml);
        assert_eq!(
            found[0],
            (
                0,
                Severity::Error,
                "@startuml without matching @enduml".into()
            )
        );
        assert!(found.contains(&(1, Severity::Warning, "Unclosed alt".into())));
        assert!(found.contains(&(2, Severity::Warning, "Unclosed note".into())));
    }

    #[test]
    fn a_name_like_a_group_opens_none() {
        // A state or participant may be called Critical or Loop; an arrow
        // or a description after it makes it no group.
        for code in [
            "@startuml\n[*] --> Critical\nCritical --> [*] : landed\nCritical : entry / land()\n@enduml",
            "@startuml\nLoop -> Bob : hi\nBob <- Group : back\nPar ..> Alt\n@enduml",
            // A signal-flow block and its ports (issue #1).
            "@startflow\nin h\nout height\nblock Alt \"Altimeter\" {\n  in h\n  out height\n}\nh -> Alt.h\nAlt.height -> height\nLoop.out -> height\n@endflow",
        ] {
            // PlantUML does read `Par ..> Alt` as a group, which only Java
            // draws, unclosed: lint says no more than that.
            let found = but_java(code);
            assert!(found.is_empty(), "{code:?}: {found:?}");
        }
    }

    #[test]
    fn closed_blocks_are_quiet() {
        let code = "@startuml\nalt x\n  A -> B\nelse y\n  loop\n  B -> A\n  end\nend\nnote over A\n x\nend note\n@enduml";
        assert!(lint(code, Language::PlantUml).is_empty());
    }

    #[test]
    fn names_hold_back_with_the_preprocessor() {
        // The messages come from a procedure; `C` is used after all.
        let code = "@startuml\n!procedure $call($a, $b)\n$a -> $b\n!endprocedure\nparticipant C\nparticipant S\n$call(\"C\", \"S\")\n@enduml";
        assert!(lint(code, Language::PlantUml).is_empty());
    }

    #[test]
    fn typo_duplicate_and_unused_participants() {
        let code = "@startuml\nparticipant Server\nparticipant Client\nparticipant Client\nparticipant Cache\nClient -> Sever : get\n@enduml";
        let found = messages(code, Language::PlantUml);
        assert!(found.contains(&(
            3,
            Severity::Warning,
            "Client is declared twice (first on the line above).".into()
        )));
        assert!(found.iter().any(|f| {
            f.0 == 5
                && f.2
                    .starts_with("Sever is not declared. Did you mean Server?")
        }));
        assert!(found.contains(&(
            4,
            Severity::Info,
            "Cache is declared but never sends or receives a message.".into()
        )));
    }

    #[test]
    fn aliases_count_as_names() {
        let code = "@startuml\nparticipant \"Web Server\" as WS\nWS -> WS : ping\n@enduml";
        assert!(lint(code, Language::PlantUml).is_empty());
    }

    #[test]
    fn swimlane_spelling() {
        let code =
            "@startuml\n|Build|\nstart\n:a;\n|Test|\n:b;\n|build|\n:c;\n|Build|\nstop\n@enduml";
        let found = messages(code, Language::PlantUml);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, 6);
        assert!(
            found[0]
                .2
                .starts_with("Lane \"build\" differs from \"Build\"")
        );
    }

    #[test]
    fn state_typo() {
        let code = "@startuml\n[*] --> Running\nRunning --> Paused\nPaused --> Running\nRunning --> Stoped\nPaused --> Stopped\nStopped --> [*]\n@enduml";
        let found = messages(code, Language::PlantUml);
        assert_eq!(
            found,
            [(
                4,
                Severity::Info,
                "State Stoped appears only here. Did you mean Stopped?".into()
            )]
        );
    }

    #[test]
    fn mermaid_head_and_blocks() {
        let found = messages("sequencDiagram\nA->>B: x", Language::Mermaid);
        assert_eq!(found[0].0, 0);
        let found = messages(
            "%%{init: {\n  \"theme\": \"dark\"\n}}%%\nflowchart TD\nsubgraph S\nA-->B\n",
            Language::Mermaid,
        );
        assert_eq!(found, [(4, Severity::Warning, "Unclosed subgraph".into())]);
    }

    #[test]
    fn mermaid_sequence_typo() {
        let code = "sequenceDiagram\nparticipant Alice\nparticipant Bob\nAlice->>Bobb: hi\nBob-->>Alice: ok";
        let found = messages(code, Language::Mermaid);
        assert_eq!(found.len(), 1);
        assert!(
            found[0]
                .2
                .starts_with("Bobb is not declared. Did you mean Bob?")
        );
    }

    #[test]
    fn near_finds_what_a_full_scan_finds() {
        // The old search: every candidate, the full edit distance.
        fn scan<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
            let low: Vec<char> = name.to_lowercase().chars().collect();
            if low.len() < 3 {
                return None;
            }
            let max = if low.len() > 5 { 2 } else { 1 };
            candidates.iter().copied().find(|&c| {
                let cl: Vec<char> = c.to_lowercase().chars().collect();
                c != name && (cl == low || (cl.len() >= 3 && distance(&low, &cl, max) <= max))
            })
        }
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        let mut word = || {
            let mut w = String::new();
            for _ in 0..3 + seed % 6 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                w.push(['a', 'b', 'c', 'A', 'é', '1'][(seed % 6) as usize]);
            }
            w
        };
        let candidates: Vec<String> = (0..300).map(|_| word()).collect();
        let candidates: Vec<&str> = candidates.iter().map(String::as_str).collect();
        let near = Near::new(&candidates);
        for _ in 0..3000 {
            let name = word();
            assert_eq!(near.of(&name), scan(&name, &candidates), "{name}");
        }
    }

    #[test]
    fn edit_distance() {
        let d = |a: &str, b: &str| {
            distance(
                &a.chars().collect::<Vec<_>>(),
                &b.chars().collect::<Vec<_>>(),
                2,
            )
        };
        assert_eq!(d("kitten", "sitten"), 1);
        assert_eq!(d("abc", "abcdef"), 3);
        assert_eq!(d("auht", "auth"), 1);
        assert_eq!(d("ab", "ba"), 1);
    }
}
