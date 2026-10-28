//! The structure of a diagram: participants and groups
//! of a sequence, lanes and branches of an activity, states, classes and
//! packages, timing signals, mind map branches; and the same for Mermaid.

use std::collections::HashSet;

use omagram_uml::element_lines;
use regex::Regex;
use serde::Serialize;

use crate::kind::{Kind, classify};
use crate::lang::Language;
use crate::text::{brace_balance, re};

/// At most this many entries; a huge diagram stays usable.
pub const MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Zero-based line.
    pub line: usize,
    /// Nesting level, 0 at the top.
    pub depth: usize,
    /// What the entry is: `participant`, `alt`, `lane`, `state`, `class`, …;
    /// empty for mind map branches.
    pub tag: String,
    pub label: String,
}

pub fn outline(code: &str, lang: Language) -> Vec<Entry> {
    outline_with_folds(code, lang).0
}

/// The outline, and the blocks the editor can fold: header line and last
/// line of each block (`alt`, `if`, `subgraph`, `state X {`, …) and of
/// each of its branches (`else`, `case`, …), each `section`, and each
/// mind map branch with children. A block and each branch end on the
/// block's closing line (`end`, `endif`, `}`), which a collapsed fold
/// shows after its header, like `alt ok ⋯ end`. A block that never closes
/// runs to the last line before a trailing `@end…`.
pub fn outline_with_folds(code: &str, lang: Language) -> (Vec<Entry>, Vec<(usize, usize)>) {
    let lines: Vec<&str> = code.split('\n').collect();
    match lang {
        Language::Mermaid => mermaid(&lines),
        Language::PlantUml => plantuml(code, &lines, classify(code, lang).kind),
    }
}

struct Builder {
    out: Vec<Entry>,
    /// The header and branch lines of each open block, innermost last.
    blocks: Vec<Vec<usize>>,
    /// The first line of the open section.
    section: Option<usize>,
    folds: Vec<(usize, usize)>,
    seen: HashSet<String>,
}

impl Builder {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            blocks: Vec::new(),
            section: None,
            folds: Vec::new(),
            seen: HashSet::new(),
        }
    }

    /// The nesting level inside the open blocks.
    fn depth(&self) -> isize {
        self.blocks.len() as isize
    }

    /// Adds an entry at the current depth.
    fn add(&mut self, line: usize, tag: &str, label: String) {
        self.add_at(line, tag, label, self.depth());
    }

    fn add_at(&mut self, line: usize, tag: &str, label: String, depth: isize) {
        if self.out.len() < MAX_ENTRIES {
            self.out.push(Entry {
                line,
                depth: depth.max(0) as usize,
                tag: tag.to_owned(),
                label,
            });
        }
    }

    /// Opens a block whose header is on `row`.
    fn open(&mut self, row: usize) {
        self.blocks.push(vec![row]);
    }

    /// Closes the innermost block at its closing line `row`. The block and
    /// each branch fold up to it, if that hides a line.
    fn close(&mut self, row: usize) {
        for start in self.blocks.pop().unwrap_or_default() {
            if row > start + 1 {
                self.fold(start, row);
            }
        }
    }

    /// Adds a branch of the innermost block (`else`, `case`, …) on `row`.
    fn branch(&mut self, row: usize, tag: &str, label: String) {
        self.add_at(row, tag, label, self.depth() - 1);
        if let Some(starts) = self.blocks.last_mut() {
            starts.push(row);
        }
    }

    /// Adds a section on `row`. The section before it ends above it.
    fn section(&mut self, row: usize, label: String) {
        self.add_at(row, "section", label, 0);
        if let Some(start) = self.section.replace(row) {
            self.fold(start, row.saturating_sub(1));
        }
    }

    fn fold(&mut self, header: usize, last: usize) {
        if last > header && self.folds.len() < MAX_ENTRIES {
            self.folds.push((header, last));
        }
    }

    /// The entries and the folds. Blocks and the section still open run
    /// to the last line before a trailing `@end…`. With `nested`, each
    /// entry folds the deeper entries after it, as in a mind map.
    fn finish(mut self, lines: &[&str], nested: bool) -> (Vec<Entry>, Vec<(usize, usize)>) {
        let end = lines
            .iter()
            .rposition(|line| {
                let t = line.trim();
                !t.is_empty()
                    && !t.to_ascii_lowercase().starts_with("@end")
                    && !t.starts_with("```")
            })
            .unwrap_or(0);
        for start in self.blocks.drain(..).flatten().collect::<Vec<_>>() {
            self.fold(start, end);
        }
        if let Some(start) = self.section.take() {
            self.fold(start, end);
        }
        if nested {
            for (i, entry) in self.out.iter().enumerate() {
                let last = self.out[i + 1..]
                    .iter()
                    .find(|next| next.depth <= entry.depth)
                    .map_or(end, |next| next.line.saturating_sub(1));
                if last > entry.line && self.folds.len() < MAX_ENTRIES {
                    self.folds.push((entry.line, last));
                }
            }
        }
        self.folds.sort_unstable();
        (self.out, self.folds)
    }

    /// Remembers `key`; false if it was seen before.
    fn first(&mut self, key: String) -> bool {
        self.seen.insert(key)
    }
}

/// Display text: no `{`, no `\n`, no stereotypes, no quotes, at most 60 chars.
fn label(raw: &str) -> String {
    let t = re!(r"\s*\{\s*$").replace(raw, "");
    let t = t.replace("\\n", " ");
    let t = re!(r"<<[^>]*>>").replace_all(&t, "");
    let t = re!(r"\s+").replace_all(&t, " ");
    let t = t.trim();
    let t = t.strip_prefix('"').unwrap_or(t);
    let t = t.strip_suffix('"').unwrap_or(t).trim();
    if t.chars().count() > 60 {
        let mut short: String = t.chars().take(59).collect();
        short.push('…');
        short
    } else {
        t.to_owned()
    }
}

/// The display name of a declaration: `"Long name" as L` → `Long name`,
/// `[Comp]`, `(Use case)`, `:Actor:`, or the first word.
fn decl_name(rest: &str) -> String {
    let decl = re!(r#"(?i)^(?:"([^"]+)"|\[([^\]]+)\]|\(([^)]+)\)|:([^:]+):|([^\s"<#{]+))(?:\s+as\s+(?:"([^"]+)"|([^\s"<#{]+)))?"#)
        .captures(rest);
    match decl {
        Some(c) => {
            let name = [1, 2, 3, 4, 6, 5]
                .into_iter()
                .find_map(|i| c.get(i))
                .map_or("", |m| m.as_str());
            label(name)
        }
        None => label(rest),
    }
}

fn plantuml(code: &str, lines: &[&str], kind: Kind) -> (Vec<Entry>, Vec<(usize, usize)>) {
    let mut b = Builder::new();
    let mut until: Option<&Regex> = None;
    let mut body = 0isize;
    let mind = lines
        .iter()
        .find(|l| re!(r"(?i)^\s*@start").is_match(l))
        .is_some_and(|l| re!(r"(?i)^@start(mindmap|wbs)\b").is_match(l));
    let flow = lines
        .iter()
        .any(|l| re!(r"(?i)^\s*@startflow\b").is_match(l));
    // The lines the engine reads, without the comments, notes, legends and
    // styles it skips; all lines of a diagram it cannot read.
    let rows: Vec<(usize, &str)> = match element_lines(code) {
        Some(found) => found.into_iter().map(|l| (l.no, l.text)).collect(),
        None => lines
            .iter()
            .map(|l| l.trim())
            .enumerate()
            .filter(|(_, l)| !l.is_empty() && !l.starts_with('\''))
            .collect(),
    };

    for (row, l) in rows {
        if let Some(end) = until {
            if end.is_match(l) {
                until = None;
            }
            continue;
        }
        if body > 0 {
            body += brace_balance(l);
            continue;
        }
        // A procedure's lines are a template, not the diagram.
        if re!(r"(?i)^!(?:(?:unquoted|final)\s+!?)*(?:procedure|function|definelong)\b").is_match(l)
            && !l.contains("!return")
        {
            until = Some(re!(r"(?i)^!end(?:procedure|function|definelong)\b"));
            continue;
        }
        // A body's first line and its `}` stay, so the `}` does not close
        // a package.
        if re!(r"(?i)^skinparam\b.*\{\s*$").is_match(l) {
            body = brace_balance(l);
            continue;
        }
        if let Some(c) = re!(r"(?i)^title\s+(.+)$").captures(l) {
            b.add_at(row, "title", label(&c[1]), 0);
            continue;
        }
        if mind {
            if let Some(c) = re!(r"^([*+-]+)(?:\[[^\]]*\])?[_<>]*\s+(.+)$").captures(l) {
                b.add_at(row, "", label(&c[2]), c[1].len() as isize - 1);
            }
            continue;
        }
        if flow {
            flow_line(&mut b, row, l);
            continue;
        }
        match kind {
            Kind::Sequence => sequence_line(&mut b, row, l),
            Kind::Activity => activity_line(&mut b, row, l),
            Kind::State => state_line(&mut b, row, l),
            Kind::Timing => {
                if let Some(c) = re!(r"(?i)^(robust|concise|clock|binary|analog)\s+(.+)$").captures(l) {
                    b.add_at(row, &c[1].to_lowercase(), decl_name(&c[2]), 0);
                } else if let Some(c) = re!(r"(?i)^@(\S+)\s*(?:as\s+(\S+))?").captures(l)
                    && !re!(r"(?i)^(?:start|end)").is_match(&c[1])
                {
                    let text = match c.get(2) {
                        Some(alias) => format!("{} ({})", &c[1], alias.as_str()),
                        None => c[1].to_owned(),
                    };
                    b.add_at(row, "@", label(&text), 0);
                }
            }
            Kind::Structure => {
                if let Some(c) = re!(r"(?i)^(abstract\s+class|abstract|class|interface|enum|annotation|struct|record|component|node|package|namespace|rectangle|frame|cloud|folder|database|usecase|actor|object|artifact|card|storage|queue|stack|file|agent|hexagon|map|json)\s+(.+)$")
                    .captures(l)
                {
                    let word = re!(r"\s+").replace_all(&c[1].to_lowercase(), " ").into_owned();
                    b.add(row, &word, decl_name(&c[2]));
                    if re!(r"\{\s*$").is_match(l) {
                        if re!(r"(?i)^(abstract\s+class|abstract|class|interface|enum|annotation|struct|record|object|map|json)$")
                            .is_match(&word)
                        {
                            body = brace_balance(l);
                        } else {
                            b.open(row);
                        }
                    }
                } else if l.starts_with('}') {
                    b.close(row);
                }
            }
            Kind::Other => {}
        }
    }
    b.finish(lines, mind)
}

/// A signal flow lists its blocks. Ports and wires stay out of the outline.
/// A `{` on the block's own line folds the body; one on the next line does not.
fn flow_line(b: &mut Builder, row: usize, l: &str) {
    if let Some(c) = re!(r#"(?i)^block\s+([A-Za-z_][A-Za-z0-9_]*)(?:\s+"([^"]*)")?"#).captures(l) {
        let shown = c.get(2).map(|m| m.as_str()).unwrap_or(&c[1]);
        b.add(row, "block", label(shown));
        let after = c.get(2).or(c.get(1)).map(|m| m.end()).unwrap_or(0);
        if l[after..].contains('{') {
            b.open(row);
        }
    } else if l == "}" {
        b.close(row);
    }
}

fn sequence_line(b: &mut Builder, row: usize, l: &str) {
    if let Some(c) = re!(r"^==+\s*(.*?)\s*==+$").captures(l) {
        b.add_at(row, "==", label(&c[1]), 0);
    } else if let Some(c) =
        re!(r"(?i)^(participant|actor|boundary|control|entity|database|collections|queue)\s+(.+)$")
            .captures(l)
    {
        b.add(row, &c[1].to_lowercase(), decl_name(&c[2]));
    } else if let Some(c) = re!(r"(?i)^box\b\s*(.*)$").captures(l) {
        let text = re!(r"#\w+\s*$").replace(&c[1], "");
        b.add(row, "box", label(&text));
        b.open(row);
    } else if re!(r"(?i)^end\s*box\b").is_match(l) {
        b.close(row);
    } else if let Some(c) =
        re!(r"(?i)^(alt|opt|loop|par|break|critical|group)\b\s*(.*)$").captures(l)
    {
        b.add(row, &c[1].to_lowercase(), label(&c[2]));
        b.open(row);
    } else if let Some(c) = re!(r"(?i)^else\b\s*(.*)$").captures(l) {
        b.branch(row, "else", label(&c[1]));
    } else if re!(r"(?i)^end\b").is_match(l) {
        b.close(row);
    } else if let Some(c) = re!(r"(?i)^ref\s+over\s+[^:]+:\s*(.+)$").captures(l) {
        b.add(row, "ref", label(&c[1]));
    } else if re!(r"(?i)^newpage\b").is_match(l) {
        let rest = l.get(7..).unwrap_or("");
        b.add_at(row, "newpage", label(rest), 0);
    }
}

fn activity_line(b: &mut Builder, row: usize, l: &str) {
    if let Some(c) = re!(r"^\|(?:#[^|]*\|)?([^|]+)\|").captures(l) {
        if b.first(format!("lane:{}", c[1].trim().to_lowercase())) {
            b.add_at(row, "lane", label(&c[1]), 0);
        }
    } else if let Some(c) = re!(r"(?i)^partition\s+(.+?)\s*\{?\s*$").captures(l) {
        b.add(row, "partition", label(&c[1]));
        b.open(row);
    } else if let Some(c) = re!(r"(?i)^group\s+(.+)$").captures(l) {
        b.add(row, "group", label(&c[1]));
        b.open(row);
    } else if re!(r"(?i)^end\s*group\b|^\}$").is_match(l) {
        b.close(row);
    } else if let Some(c) = re!(r"(?i)^if\s*\((.*?)\)")
        .captures(l)
        // Legacy activity syntax writes if "cond" then … else … endif.
        .or_else(|| re!(r"(?i)^if\s+(.*?)\s*then\b").captures(l))
    {
        b.add(row, "if", label(&c[1]));
        b.open(row);
    } else if let Some(c) = re!(r"(?i)^elseif\s*\((.*?)\)").captures(l) {
        b.branch(row, "elseif", label(&c[1]));
    } else if let Some(c) = re!(r"(?i)^else\b\s*(?:\((.*?)\))?").captures(l) {
        let text = c.get(1).map_or("", |m| m.as_str());
        b.branch(row, "else", label(text));
    } else if re!(r"(?i)^end\s*if\b").is_match(l) {
        b.close(row);
    } else if let Some(c) = re!(r"(?i)^while\s*\((.*?)\)").captures(l) {
        b.add(row, "while", label(&c[1]));
        b.open(row);
    } else if re!(r"(?i)^end\s*while\b").is_match(l) {
        b.close(row);
    } else if re!(r"(?i)^repeat\b").is_match(l) && !re!(r"(?i)^repeat\s*while\b").is_match(l) {
        b.add(row, "repeat", String::new());
        b.open(row);
    } else if re!(r"(?i)^repeat\s*while\b").is_match(l) {
        b.close(row);
    } else if let Some(c) = re!(r"(?i)^(fork|split)\s+again\b").captures(l) {
        b.branch(
            row,
            &format!("{} again", c[1].to_lowercase()),
            String::new(),
        );
    } else if let Some(c) = re!(r"(?i)^(fork|split)\b").captures(l) {
        b.add(row, &c[1].to_lowercase(), String::new());
        b.open(row);
    } else if re!(r"(?i)^end\s*(fork|merge|split)\b").is_match(l) {
        b.close(row);
    } else if let Some(c) = re!(r"(?i)^switch\s*\((.*?)\)").captures(l) {
        b.add(row, "switch", label(&c[1]));
        b.open(row);
    } else if let Some(c) = re!(r"(?i)^case\s*\((.*?)\)").captures(l) {
        b.branch(row, "case", label(&c[1]));
    } else if re!(r"(?i)^end\s*switch\b").is_match(l) {
        b.close(row);
    }
}

fn state_line(b: &mut Builder, row: usize, l: &str) {
    if let Some(c) = re!(r"(?i)^state\s+(.+)$").captures(l) {
        let decl = re!(r"\s*:.*$").replace(&c[1], "");
        let name = decl_name(&decl);
        b.first(name.to_lowercase());
        // Transitions name the state by its id, so "Long name" as LS must not list LS again.
        if let Some(ids) =
            re!(r#"(?i)^(?:"[^"]+"|([^\s"<#{]+))(?:\s+as\s+(?:"[^"]+"|([^\s"<#{]+)))?"#)
                .captures(&decl)
        {
            for id in [ids.get(1), ids.get(2)].into_iter().flatten() {
                b.first(id.as_str().to_lowercase());
            }
        }
        b.add(row, "state", name);
        if re!(r"\{\s*$").is_match(l) {
            b.open(row);
        }
    } else if l.starts_with('}') {
        b.close(row);
    } else if let Some(c) =
        re!(r#"^("[^"]+"|[\w.]+|\[\*\])\s*-[-\w\[\],#]*>\s*("[^"]+"|[\w.]+|\[\*\])"#).captures(l)
    {
        for end in [&c[1], &c[2]] {
            let name = label(end);
            if end != "[*]" && b.first(name.to_lowercase()) {
                b.add(row, "state", name);
            }
        }
    }
}

fn mermaid(lines: &[&str]) -> (Vec<Entry>, Vec<(usize, usize)>) {
    let mut b = Builder::new();
    let mut head = String::new();
    let mut front = false;
    let mut first_indent: Option<usize> = None;
    let mut class_body = false;

    for (row, raw) in lines.iter().enumerate() {
        let l = raw.trim();
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
        if head.is_empty() {
            head = re!(r"^[\w-]+")
                .find(l)
                .map_or("", |m| m.as_str())
                .to_owned();
            continue;
        }
        match head.as_str() {
            "sequenceDiagram" | "zenuml" => {
                if let Some(c) = re!(r"(?i)^(?:create\s+)?(participant|actor)\s+(.+)$").captures(l)
                {
                    let rest = re!(r"@\{.*\}\s*$").replace(&c[2], "");
                    let name = match re!(r"^(.+?)(?:\s+as\s+(.+))?$").captures(&rest) {
                        Some(d) => d.get(2).or(d.get(1)).map_or("", |m| m.as_str()).to_owned(),
                        None => c[2].to_owned(),
                    };
                    b.add(row, &c[1].to_lowercase(), label(&name));
                } else if let Some(c) =
                    re!(r"(?i)^(loop|alt|opt|par|critical|break|rect|box)\b\s*(.*)$").captures(l)
                {
                    let tag = c[1].to_lowercase();
                    let text = if tag == "rect" {
                        String::new()
                    } else {
                        label(&c[2])
                    };
                    b.add(row, &tag, text);
                    b.open(row);
                } else if let Some(c) = re!(r"(?i)^(else|and|option)\b\s*(.*)$").captures(l) {
                    b.branch(row, &c[1].to_lowercase(), label(&c[2]));
                } else if l.eq_ignore_ascii_case("end") {
                    b.close(row);
                }
            }
            h if h.starts_with("stateDiagram") => {
                if let Some(c) = re!(r"^state\s+(.+?)\s*\{\s*$").captures(l) {
                    let name = decl_name(&c[1]);
                    b.first(name.to_lowercase());
                    b.add(row, "state", name);
                    b.open(row);
                } else if l == "}" {
                    b.close(row);
                } else if let Some(c) = re!(r"^state\s+(.+)$").captures(l) {
                    let decl = re!(r"\s*:.*$").replace(&c[1], "");
                    let decl = re!(r"<<[^>]*>>").replace(&decl, "");
                    let name = decl_name(&decl);
                    b.first(name.to_lowercase());
                    b.add(row, "state", name);
                } else if let Some(c) = re!(r"^([\w.]+|\[\*\])\s*-->\s*([\w.]+|\[\*\])").captures(l)
                {
                    for end in [&c[1], &c[2]] {
                        if end != "[*]" && b.first(end.to_lowercase()) {
                            b.add(row, "state", end.to_owned());
                        }
                    }
                }
            }
            "flowchart" | "graph" => {
                if let Some(c) = re!(r"(?i)^subgraph\s+(.+)$").captures(l) {
                    let text = match re!(r#"^[\w-]+\s*\[\s*"?(.+?)"?\s*\]$"#).captures(&c[1]) {
                        Some(sub) => sub[1].to_owned(),
                        None => c[1].to_owned(),
                    };
                    b.add(row, "subgraph", label(&text));
                    b.open(row);
                } else if l.eq_ignore_ascii_case("end") {
                    b.close(row);
                } else if !re!(r"^(style|classDef|class|click|linkStyle|direction)\b").is_match(l) {
                    // Labelled nodes at their first appearance; a rhombus is a decision.
                    // An id starts and ends with a word character, so an arrow
                    // like `-->` is never read as id `--` with the shape `>`.
                    let nodes = re!(
                        r#"(\w(?:[\w-]*\w)?)\s*(\[\[|\[\(|\(\[|\(\(|\[/|\[\\|\{\{|\[|\(|\{|>)\s*"?([^\])}"]+?)"?\s*(?:\]\]|\)\]|\]\)|\)\)|/\]|\\\]|\}\}|\]|\)|\})"#
                    );
                    for c in nodes.captures_iter(l) {
                        if b.first(format!("node:{}", &c[1])) {
                            let tag = if &c[2] == "{" { "decision" } else { "node" };
                            b.add(row, tag, label(&c[3]));
                        }
                    }
                }
            }
            "classDiagram" => {
                if class_body {
                    // A class's members end at its own `}`, not its namespace's.
                    class_body = !l.starts_with('}');
                } else if let Some(c) = re!(r"^namespace\s+(.+?)\s*\{?$").captures(l) {
                    b.add(row, "namespace", label(&c[1]));
                    b.open(row);
                } else if let Some(c) = re!(r"^class\s+([\w~<>,]+)").captures(l) {
                    b.add(row, "class", label(&c[1]));
                    class_body = l.ends_with('{');
                } else if l == "}" {
                    b.close(row);
                }
            }
            "erDiagram" => {
                if let Some(c) = re!(r"^([\w-]+)\s*\{$").captures(l) {
                    b.add(row, "entity", label(&c[1]));
                }
            }
            "gantt" | "timeline" | "journey" => {
                if let Some(c) = re!(r"(?i)^section\s+(.+)$").captures(l) {
                    b.section(row, label(&c[1]));
                } else if let Some(c) = re!(r"(?i)^title\s+(.+)$").captures(l) {
                    b.add_at(row, "title", label(&c[1]), 0);
                }
            }
            "mindmap" => {
                if l.starts_with("::icon(") {
                    continue;
                }
                let indent = raw.len() - raw.trim_start().len();
                let first = *first_indent.get_or_insert(indent);
                // id(text), id[text], ((text)) … or plain text.
                let plain = re!(r":::[\w-]+\s*$").replace(l, "");
                let text = match re!(r"^[\w-]*\s*(?:\(\(|\(\[|\[\[|\{\{|\)\)|\(|\[|\{|\))(.+?)(?:\)\)|\]\)|\]\]|\}\}|\(\(|\)|\]|\})$").captures(&plain) {
                    Some(c) => c[1].to_owned(),
                    None => plain.into_owned(),
                };
                let depth = ((indent as f64 - first as f64) / 2.0).round() as isize;
                b.add_at(row, "", label(&text), depth);
            }
            _ => {}
        }
    }
    b.finish(lines, head == "mindmap")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(code: &str, lang: Language) -> Vec<(usize, usize, String, String)> {
        outline(code, lang)
            .into_iter()
            .map(|e| (e.line, e.depth, e.tag, e.label))
            .collect()
    }

    fn e(line: usize, depth: usize, tag: &str, label: &str) -> (usize, usize, String, String) {
        (line, depth, tag.into(), label.into())
    }

    #[test]
    fn plantuml_sequence() {
        let code = "@startuml\ntitle Login\nactor User\nparticipant \"Web App\" as W\n== Start ==\nalt ok\n  User -> W : go\nelse fail\n  loop 3 times\n  end\nend\n@enduml";
        assert_eq!(
            tags(code, Language::PlantUml),
            [
                e(1, 0, "title", "Login"),
                e(2, 0, "actor", "User"),
                e(3, 0, "participant", "Web App"),
                e(4, 0, "==", "Start"),
                e(5, 0, "alt", "ok"),
                e(7, 0, "else", "fail"),
                e(8, 1, "loop", "3 times"),
            ]
        );
        // `alt` and its `else` end on their `end`. The empty `loop` and the
        // declarations fold nothing.
        assert_eq!(folds(code, Language::PlantUml), [(5, 10), (7, 10)]);
    }

    fn folds(code: &str, lang: Language) -> Vec<(usize, usize)> {
        outline_with_folds(code, lang).1
    }

    #[test]
    fn procedure_bodies_are_not_the_diagram() {
        let code = "@startuml\n!procedure $part($t)\n  == $t ==\n!endprocedure\n!function $f() !return 1\n== Real ==\n$part(\"x\")\n@enduml";
        assert_eq!(tags(code, Language::PlantUml), [e(5, 0, "==", "Real")]);
    }

    #[test]
    fn declarations_fold_nothing() {
        let code = "@startuml\nparticipant A\nparticipant B\nA -> B : hi\nB -> A : ok\n@enduml";
        assert!(folds(code, Language::PlantUml).is_empty());
    }

    #[test]
    fn blocks_end_at_their_own_closing_line() {
        let code = "@startuml\n|Dev|\nstart\nif (ok?) then (yes)\n  :ship;\nelse (no)\n  :fix;\nendif\n|dev|\nstop\n@enduml";
        assert_eq!(folds(code, Language::PlantUml), [(3, 7), (5, 7)]);
        let unclosed = "@startuml\nalt ok\n  A -> B\n  B -> A\n\n@enduml\n";
        assert_eq!(folds(unclosed, Language::PlantUml), [(1, 3)]);
        let mermaid = "flowchart TD\nsubgraph S\nA --> B\nend\nB --> C";
        assert_eq!(folds(mermaid, Language::Mermaid), [(1, 3)]);
    }

    #[test]
    fn mermaid_class_body_does_not_close_its_namespace() {
        let code = "classDiagram\nnamespace N {\nclass A {\n+x\n}\nclass B\n}";
        assert_eq!(
            tags(code, Language::Mermaid),
            [
                e(1, 0, "namespace", "N"),
                e(2, 1, "class", "A"),
                e(5, 1, "class", "B")
            ]
        );
        assert_eq!(folds(code, Language::Mermaid), [(1, 6)]);
    }

    #[test]
    fn sections_and_mind_map_branches_fold() {
        let gantt = "gantt\nsection A\nt1 :a, 1d\nsection B\nt2 :b, 1d";
        assert_eq!(folds(gantt, Language::Mermaid), [(1, 2), (3, 4)]);
        let mind = "@startmindmap\n* Root\n** A\n*** A1\n** B\n@endmindmap";
        assert_eq!(folds(mind, Language::PlantUml), [(1, 4), (2, 3)]);
    }

    #[test]
    fn plantuml_activity() {
        let code = "@startuml\n|Dev|\nstart\nif (tests pass?) then (yes)\n  :ship;\nelse (no)\n  :fix;\nendif\n|dev|\nstop\n@enduml";
        assert_eq!(
            tags(code, Language::PlantUml),
            [
                e(1, 0, "lane", "Dev"),
                e(3, 0, "if", "tests pass?"),
                e(5, 0, "else", "no")
            ]
        );
    }

    #[test]
    fn plantuml_state_lists_each_state_once() {
        let code = "@startuml\nstate \"Waiting for input\" as W\n[*] --> W\nW --> Done\nDone --> [*]\n@enduml";
        assert_eq!(
            tags(code, Language::PlantUml),
            [
                e(1, 0, "state", "Waiting for input"),
                e(3, 0, "state", "Done")
            ]
        );
    }

    #[test]
    fn plantuml_mindmap() {
        let code = "@startmindmap\n* Root\n** A\n*** A1\n** B\n@endmindmap";
        assert_eq!(
            tags(code, Language::PlantUml),
            [
                e(1, 0, "", "Root"),
                e(2, 1, "", "A"),
                e(3, 2, "", "A1"),
                e(4, 1, "", "B")
            ]
        );
    }

    #[test]
    fn mermaid_flowchart_nodes() {
        let code = "flowchart TD\nsubgraph S[\"Build\"]\nA[Start] --> B{OK?}\nB --> A\nend";
        assert_eq!(
            tags(code, Language::Mermaid),
            [
                e(1, 0, "subgraph", "Build"),
                e(2, 1, "node", "Start"),
                e(2, 1, "decision", "OK?")
            ]
        );
    }

    #[test]
    fn a_signal_flow_lists_its_blocks() {
        let code = "@startflow\ntitle Charge\nblock Limit \"Current\" {\n  in voltage\n}\n@endflow";
        assert_eq!(
            tags(code, Language::PlantUml),
            [e(1, 0, "title", "Charge"), e(2, 0, "block", "Current")]
        );
        assert_eq!(folds(code, Language::PlantUml), [(2, 4)]);
    }

    #[test]
    fn a_subsystem_nests_its_blocks() {
        let code =
            "@startflow\nblock Charger {\nblock Limit \"Current\" {\nin voltage\n}\n}\n@endflow";
        assert_eq!(
            tags(code, Language::PlantUml),
            [e(1, 0, "block", "Charger"), e(2, 1, "block", "Current")]
        );
        assert_eq!(folds(code, Language::PlantUml), [(1, 5), (2, 4)]);
    }

    #[test]
    fn long_labels_are_cut() {
        let long = "x".repeat(80);
        assert_eq!(label(&long).chars().count(), 60);
        assert_eq!(label("\"a\\nb\" <<x>> {"), "a b");
    }
}
