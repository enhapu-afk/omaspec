//! Findings while typing: syntax problems that stop a diagram from drawing
//! right, and hints for likely typos.
//!
//! Syntax: missing `@end…`, unclosed blocks and notes, an unknown Mermaid
//! header. Hints: a message to a name close to a declared participant (both
//! engines would silently draw a new one), duplicate and unused participants,
//! swimlanes that differ only in case or spaces, a state name seen once next
//! to a frequent one.

use std::collections::{HashMap, HashSet};

use omaspec_uml::element_lines;
use regex::Regex;
use serde::Serialize;

use crate::kind::{Kind, classify};
use crate::lang::{Language, mermaid_graph, mermaid_head, puml_start};
use crate::text::{
    is_note_block_open, is_note_closer, is_note_one_line, normalize_newlines, re, unquote,
};

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
    /// Zero-based line.
    pub line: usize,
    pub severity: Severity,
    pub message: String,
}

impl Finding {
    fn new(line: usize, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            line,
            severity,
            message: message.into(),
        }
    }
}

/// All findings for `code`, in line order.
pub fn lint(code: &str, lang: Language) -> Vec<Finding> {
    let code = normalize_newlines(code);
    let lines: Vec<&str> = code.split('\n').collect();
    let mut findings = match lang {
        Language::Mermaid => mermaid_syntax(&lines),
        Language::PlantUml => plantuml_syntax(&lines),
    };
    let kind = classify(&code, lang).kind;
    match lang {
        Language::Mermaid => {
            let rows = mermaid_rows(&lines);
            match kind {
                Kind::Sequence => sequence(&mermaid_parts(&rows), &mut findings),
                Kind::State => state_names(
                    &rows,
                    re!(r"^([\w.]+|\[\*\])\s*-->\s*([\w.]+|\[\*\])"),
                    re!(r#"^state\s+(?:"[^"]*"\s+as\s+)?([\w.]+)"#),
                    &mut findings,
                ),
                _ => {}
            }
        }
        // The preprocessor can write, hide and repeat lines, so the rules
        // about names hold back once it runs.
        Language::PlantUml if lines.iter().any(|l| l.trim_start().starts_with('!')) => {}
        Language::PlantUml => {
            let rows: Vec<Row> = element_lines(&code)
                .unwrap_or_default()
                .into_iter()
                .map(|line| Row {
                    row: line.no,
                    text: line.text,
                })
                .collect();
            match kind {
                Kind::Sequence => sequence(&plantuml_parts(&rows), &mut findings),
                Kind::Activity => swimlanes(&rows, &mut findings),
                Kind::State => state_names(
                    &rows,
                    re!(r#"^("[^"]+"|[\w.]+|\[\*\])\s*-[-\w\[\],#]*>\s*("[^"]+"|[\w.]+|\[\*\])"#),
                    re!(r#"(?i)^state\s+(?:"[^"]*"\s+as\s+)?([\w.]+)"#),
                    &mut findings,
                ),
                _ => {}
            }
        }
    }
    findings.sort_by_key(|f| f.line);
    findings
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
    for (row, kind) in starts {
        findings.push(Finding::new(
            row,
            Severity::Error,
            format!("@start{kind} without matching @end{kind}"),
        ));
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
        if let Some(c) = re!(r"(?i)^(alt|opt|loop|par|critical|group|box)\b").captures(&t) {
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
        } else if t == "}" {
            close(&mut stack, |k| k.ends_with(" {"));
        }
    }
    for (row, kind) in stack {
        findings.push(Finding::new(
            row,
            Severity::Warning,
            format!("Unclosed {kind}"),
        ));
    }
    findings
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

struct Decl {
    /// The name messages use: the alias if there is one.
    key: String,
    /// The other name, if any.
    name: String,
    row: usize,
}

struct Message {
    from: String,
    to: String,
    row: usize,
}

#[derive(Default)]
struct SequenceParts {
    decls: Vec<Decl>,
    messages: Vec<Message>,
    uses: Vec<String>,
}

fn plantuml_parts(rows: &[Row]) -> SequenceParts {
    let mut parts = SequenceParts::default();
    for r in rows {
        if let Some(c) = re!(r"(?i)^(?:create\s+)?(?:actor|participant|boundary|control|entity|database|collections|queue)\s+(.+)$")
            .captures(r.text)
        {
            let Some(d) = re!(r#"(?i)^(?:"([^"]+)"|([^\s"<#]+))(?:\s+as\s+(?:"([^"]+)"|([^\s"<#]+)))?"#)
                .captures(&c[1])
            else {
                continue;
            };
            let g = |i: usize| d.get(i).map_or("", |m| m.as_str()).to_owned();
            let (key, name) = if d.get(1).is_some() {
                let alias = if d.get(3).is_some() { g(3) } else { g(4) };
                (if alias.is_empty() { g(1) } else { alias }, g(1))
            } else if d.get(3).is_some() {
                (g(2), g(3))
            } else if d.get(4).is_some() {
                (g(4), g(2))
            } else {
                (g(2), String::new())
            };
            parts.decls.push(Decl { key, name, row: r.row });
        } else if let Some(c) = re!(r#"(?i)^(?:activate|deactivate|destroy)\s+("[^"]+"|\S+)"#).captures(r.text) {
            parts.uses.push(unquote(&c[1]).to_owned());
        } else if let Some(c) = re!(r"(?i)^[hr]?note\s+(?:over|left of|right of)\s+([^:]+)")
            .captures(r.text)
            .or_else(|| re!(r"(?i)^ref\s+over\s+([^:]+)").captures(r.text))
        {
            parts.uses.extend(c[1].split(',').map(|k| unquote(k).to_owned()));
        } else if let Some(c) = re!(r#"^("[^"]+"|[\p{L}\p{N}_.@]+)\s*([ox]?<{0,2}-+(?:\[[^\]]*\])?-*(?:>{1,2}|\\{1,2}|/{1,2})?[ox]?)\s*("[^"]+"|[\p{L}\p{N}_.@]+)\s*(?:\+\+|--|\*\*|!!)?\s*(?::\s*(.*))?$"#)
            .captures(r.text)
        {
            let left = unquote(&c[1]).to_owned();
            let right = unquote(&c[3]).to_owned();
            let back = c[2].contains('<') && !c[2].contains('>');
            let (from, to) = if back { (right, left) } else { (left, right) };
            parts.messages.push(Message { from, to, row: r.row });
        }
    }
    parts
}

fn mermaid_parts(rows: &[Row]) -> SequenceParts {
    let mut parts = SequenceParts::default();
    for r in rows {
        if let Some(c) = re!(r"(?i)^(?:create\s+)?(?:participant|actor)\s+(.+?)(?:\s+as\s+(.+))?$")
            .captures(r.text)
        {
            let key = re!(r"@\{.*\}\s*$").replace(&c[1], "").trim().to_owned();
            let name = c.get(2).map_or("", |m| m.as_str().trim()).to_owned();
            parts.decls.push(Decl {
                key,
                name,
                row: r.row,
            });
        } else if let Some(c) =
            re!(r"(?i)^(?:activate|deactivate|destroy)\s+(.+)$").captures(r.text)
        {
            parts.uses.push(c[1].trim().to_owned());
        } else if let Some(c) =
            re!(r"(?i)^note\s+(?:over|left of|right of)\s+([^:]+):").captures(r.text)
        {
            parts
                .uses
                .extend(c[1].split(',').map(|k| k.trim().to_owned()));
        } else if let Some(c) = re!(
            r"^([^:;,<>+\s][^:;,<>+]*?)\s*(<<)?--?(?:>>|>|x|\))\s*[+-]?\s*([^:;,<>+]+?)\s*:\s*(.*)$"
        )
        .captures(r.text)
        {
            let (a, b) = (c[1].trim().to_owned(), c[3].trim().to_owned());
            let (from, to) = if c.get(2).is_some() { (b, a) } else { (a, b) };
            parts.messages.push(Message {
                from,
                to,
                row: r.row,
            });
        }
    }
    parts
}

fn sequence(parts: &SequenceParts, findings: &mut Vec<Finding>) {
    // Name → index of the declaration that owns it.
    let mut declared: HashMap<&str, usize> = HashMap::new();
    for (i, d) in parts.decls.iter().enumerate() {
        match declared.get(d.key.as_str()) {
            Some(&first) => findings.push(Finding::new(
                d.row,
                Severity::Warning,
                format!(
                    "{} is declared twice (first on line {}).",
                    d.key,
                    parts.decls[first].row + 1
                ),
            )),
            None => {
                declared.insert(&d.key, i);
            }
        }
        if !d.name.is_empty() && d.name != d.key {
            declared.entry(&d.name).or_insert(i);
        }
    }
    let keys: Vec<&str> = parts.decls.iter().map(|d| d.key.as_str()).collect();
    let near = Near::new(&keys);
    let mut used: HashSet<&str> = parts.uses.iter().map(String::as_str).collect();
    let mut warned: HashSet<&str> = HashSet::new();
    for m in &parts.messages {
        for end in [m.from.as_str(), m.to.as_str()] {
            used.insert(end);
            if declared.contains_key(end) || parts.decls.is_empty() || warned.contains(end) {
                continue;
            }
            let Some(near) = near.of(end) else {
                continue;
            };
            warned.insert(end);
            findings.push(Finding::new(
                m.row,
                Severity::Warning,
                format!(
                    "{end} is not declared. Did you mean {near}? PlantUML and Mermaid would draw a new participant."
                ),
            ));
        }
    }
    for (i, d) in parts.decls.iter().enumerate() {
        if declared.get(d.key.as_str()) != Some(&i) {
            continue;
        }
        if !used.contains(d.key.as_str()) && !(!d.name.is_empty() && used.contains(d.name.as_str()))
        {
            findings.push(Finding::new(
                d.row,
                Severity::Info,
                format!(
                    "{} is declared but never sends or receives a message.",
                    d.key
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

fn state_names(rows: &[Row], transition: &Regex, declaration: &Regex, findings: &mut Vec<Finding>) {
    // State name → rows it appears on, in order of first appearance.
    let mut seen: Vec<(String, Vec<usize>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut note = |name: &str, row: usize| {
        let name = unquote(name);
        if name.is_empty() || name == "[*]" {
            return;
        }
        match index.get(name) {
            Some(&i) => seen[i].1.push(row),
            None => {
                index.insert(name.to_owned(), seen.len());
                seen.push((name.to_owned(), vec![row]));
            }
        }
    };
    for r in rows {
        if let Some(c) = declaration.captures(r.text) {
            note(&c[1], r.row);
        } else if let Some(c) = transition.captures(r.text) {
            note(&c[1], r.row);
            note(&c[2], r.row);
        }
    }
    let frequent: Vec<&str> = seen
        .iter()
        .filter(|(_, rows)| rows.len() > 1)
        .map(|(n, _)| n.as_str())
        .collect();
    let near = Near::new(&frequent);
    for (name, rows) in &seen {
        if rows.len() != 1 || name.chars().count() < 4 {
            continue;
        }
        if let Some(near) = near.of(name) {
            findings.push(Finding::new(
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

    /// From PlantUML's own examples: blocks that are closed after all.
    #[test]
    fn closed_blocks_are_not_reported() {
        for code in [
            "@startuml\nstate A\nnote right of A\n note\n on A\nend note\n@enduml",
            "@startuml\nstart\ngroup Init {\n  :read;\n}\nstop\n@enduml",
            "@startuml\nA -> B : hi\nalt#Gold #LightBlue ok\n  B -> A : yes\nelse #Pink no\n  B -> A : no\nend\n@enduml",
            "@startgantt\n[a] requires 2 days\nnote bottom\n  note for a\nend note\n@endgantt",
        ] {
            let found = messages(code, Language::PlantUml);
            assert!(found.is_empty(), "{code:?}: {found:?}");
        }
        let found = messages(
            "@startuml\nstart\ngroup Init {\n  :read;\nstop\n@enduml",
            Language::PlantUml,
        );
        assert_eq!(found, [(2, Severity::Warning, "Unclosed group {".into())]);
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
            "Client is declared twice (first on line 3).".into()
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
