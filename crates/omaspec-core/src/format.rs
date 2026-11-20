//! Re-indents a diagram: two spaces per open block, nothing else changes
//! except trailing spaces and runs of blank lines. Line breaks stay where
//! they are.
//!
//! PlantUML blocks whose layout is their content (JSON, YAML, math, mind
//! maps, WBS, Gantt, …) are kept verbatim. In Mermaid, indentation is
//! syntax for mind maps, kanban, sankey, packet and pie, so those are only
//! tidied.

use crate::lang::{Language, mermaid_graph, mermaid_head};
use crate::text::{
    is_note_block_open, is_note_closer, normalize_newlines, re, significant_lines, strip_bom,
    strip_strings,
};

const INDENT: &str = "  ";

pub fn format(code: &str, lang: Language) -> String {
    match lang {
        Language::PlantUml => plantuml(code),
        Language::Mermaid => mermaid(code),
    }
}

/// Braces in a line: opened, closed, and closed before the first opening one.
struct Braces {
    open: usize,
    close: usize,
    leading_close: usize,
}

fn braces(s: &str) -> Braces {
    let mut b = Braces {
        open: 0,
        close: 0,
        leading_close: 0,
    };
    for c in s.chars() {
        match c {
            '{' => b.open += 1,
            '}' => {
                b.close += 1;
                if b.open == 0 {
                    b.leading_close += 1;
                }
            }
            _ => {}
        }
    }
    b
}

/// Collects output lines; at most one blank line in a row, none at the end.
struct Output {
    lines: Vec<String>,
    blank_run: usize,
    level: usize,
}

impl Output {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            blank_run: 0,
            level: 0,
        }
    }

    fn blank(&mut self) {
        self.blank_run += 1;
        if self.blank_run <= 1 {
            self.lines.push(String::new());
        }
    }

    fn push(&mut self, line: String) {
        self.blank_run = 0;
        self.lines.push(line);
    }

    /// Indents `trimmed` by the block structure. `stripped` is the line with
    /// its strings emptied.
    fn indented(&mut self, trimmed: &str, stripped: &str, mid: bool, closer: bool, opener: bool) {
        let b = braces(stripped);
        let keyword_dedent = usize::from(closer || mid);
        let this_level = self
            .level
            .saturating_sub(keyword_dedent.max(b.leading_close));
        self.push(format!("{}{trimmed}", INDENT.repeat(this_level)));
        let keyword_inc = usize::from(opener || mid);
        let post = keyword_inc.max(b.open) as isize - (b.close - b.leading_close) as isize;
        self.level = (this_level as isize + post).max(0) as usize;
    }

    fn finish(mut self, ended_with_newline: bool) -> String {
        while self.lines.last().is_some_and(String::is_empty) {
            self.lines.pop();
        }
        if ended_with_newline {
            self.lines.push(String::new());
        }
        self.lines.join("\n")
    }
}

/// Where a line of a story map stands: its activities and releases at
/// the left, a step under its activity, a story under its step.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Story {
    Top,
    Activity,
    Step,
}

impl Story {
    /// How deep `line` stands, and where the lines after it do.
    fn level(&mut self, line: &str) -> usize {
        let under = match self {
            Story::Top => 0,
            Story::Activity => 1,
            Story::Step => 2,
        };
        if re!(r"(?i)^activity\b").is_match(line) {
            *self = Story::Activity;
            0
        } else if re!(r"(?i)^release\b").is_match(line) {
            // A release between two stories leaves the step they stand
            // under, as the reader does.
            0
        } else if re!(r"(?i)^step\b").is_match(line) {
            *self = Story::Step;
            1
        } else if line.starts_with('*') || line.starts_with('\'') {
            under
        } else {
            0
        }
    }
}

/// A line without its indentation and trailing space. A byte order mark
/// counts as space here: one behind spaces would otherwise move to the start
/// of the text and go only on a second pass.
fn trim_line(line: &str) -> &str {
    line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

fn plantuml(code: &str) -> String {
    let code = normalize_newlines(code);
    let lines: Vec<&str> = code.split('\n').collect();
    let mut out = Output::new();
    // The `@start…` kind whose block is copied verbatim until its `@end…`.
    let mut verbatim: Option<String> = None;
    // Inside a story map: where its lines stand.
    let mut story: Option<Story> = None;

    for line in &lines {
        let line = line.trim_end();
        let trimmed = trim_line(line);

        if let Some(kind) = &verbatim {
            let end = format!("@end{kind}");
            let ends = trimmed
                .get(..end.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(&end))
                && !trimmed[end.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_');
            if ends {
                verbatim = None;
            }
            out.push(line.to_owned());
            continue;
        }
        if let Some(at) = &mut story {
            if trimmed.is_empty() {
                out.blank();
            } else if re!(r"(?i)^@endstorymap\b").is_match(trimmed) {
                story = None;
                out.push(format!("{}{trimmed}", INDENT.repeat(out.level)));
            } else {
                let level = at.level(trimmed);
                out.push(format!("{}{trimmed}", INDENT.repeat(out.level + level)));
            }
            continue;
        }
        if let Some(c) = re!(r"^@start([A-Za-z]+)\b").captures(trimmed) {
            let kind = c[1].to_lowercase();
            if kind == "storymap" {
                out.push(format!("{}{trimmed}", INDENT.repeat(out.level)));
                story = Some(Story::Top);
                continue;
            }
            if re!(r"^(json|yaml|math|latex|ditaa|ebnf|salt|mindmap|wbs|dot|gantt|regex|nwdiag)$")
                .is_match(&kind)
            {
                out.push(format!("{}{trimmed}", INDENT.repeat(out.level)));
                verbatim = Some(kind);
                continue;
            }
        }
        if trimmed.is_empty() {
            out.blank();
            continue;
        }
        if re!(r"(?i)^@(?:start|end)uml\b").is_match(trimmed) {
            out.level = 0;
            out.push(trimmed.to_owned());
            continue;
        }

        let stripped = strip_strings(trimmed);
        let mid = plantuml_mid(&stripped);
        let closer = !mid && plantuml_closer(&stripped);
        let opener = braces(&stripped).open == 0 && plantuml_opener(trimmed);
        out.indented(trimmed, &stripped, mid, closer, opener);
    }
    out.finish(lines.last() == Some(&""))
}

/// A line that closes one branch and opens the next: `else`, `case`, …
fn plantuml_mid(t: &str) -> bool {
    re!(r"(?i)^(else(?:if)?|elseif|fork\s+again|split\s+again|also|case|!else(?:if)?)\b")
        .is_match(t)
}

fn plantuml_closer(t: &str) -> bool {
    re!(r"(?i)^repeat\s+while\b").is_match(t)
        || re!(r"(?i)^!end(?:if|while|for|foreach|function|procedure|definelong)?\b").is_match(t)
        || is_note_closer(t)
        || re!(r"(?i)^end(?:if|while|switch|note|legend|box|title|header|footer|caption|ref|fork|split)\b").is_match(t)
        || re!(r"(?i)^end\s+(?:if|while|switch|note|legend|box|title|header|footer|caption|ref|fork|split|group|alt|opt|loop|par|critical|break)\b").is_match(t)
        || re!(r"(?i)^end\s*$").is_match(t)
}

fn plantuml_opener(t: &str) -> bool {
    if plantuml_mid(t) || plantuml_closer(t) {
        return false;
    }
    if re!(r"(?i)^(?:repeat\s+while|fork\s+again|split\s+again)\b").is_match(t) {
        return false;
    }
    re!(r"(?i)^(alt|opt|loop|par|break|critical|group|box|partition)\b").is_match(t)
        || re!(r"(?i)^(if|while|repeat|fork|split|switch)\b").is_match(t)
        || re!(r"(?i)^!(?:ifn?def|if|while|foreach)\b").is_match(t)
        // A one-line `!function … !return …` has no body.
        || (re!(r"(?i)^!(?:(?:unquoted|final)\s+!?)*(?:procedure|function|definelong)\b").is_match(t)
            && !t.contains("!return"))
        || is_note_block_open(t)
        // `ref over A, B` without text opens a block up to `end ref`.
        || (re!(r"(?i)^ref\s+over\b").is_match(t) && !re!(r":\s*\S").is_match(t))
        || re!(r"(?i)^legend\b").is_match(t)
        || re!(r"(?i)^(title|header|footer|caption)\s*$").is_match(t)
}

fn mermaid(code: &str) -> String {
    let raw = normalize_newlines(strip_bom(code));
    let lines: Vec<&str> = raw.split('\n').collect();
    let ended_with_newline = lines.last() == Some(&"");

    let kind = significant_lines(&raw, 40)
        .into_iter()
        .filter(|l| !l.starts_with("%%{"))
        .find_map(|l| {
            mermaid_head()
                .find(l)
                .or_else(|| mermaid_graph().find(l))
                .map(|m| m.as_str().to_lowercase())
        })
        .unwrap_or_default();

    let mut out = Output::new();
    // Indentation is part of the language for these types: only tidy.
    if re!(r"^(mindmap|kanban|sankey-beta|packet-beta|pie)\b").is_match(&kind) {
        for line in &lines {
            let line = line.trim_end();
            if line.trim().is_empty() {
                out.blank();
            } else {
                out.push(line.to_owned());
            }
        }
        return out.finish(ended_with_newline);
    }

    // YAML front matter keeps its indentation: it is nesting there.
    let front_matter = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .filter(|&i| lines[i].trim() == "---");
    let mut in_front = false;
    let mut in_init = false;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = trim_line(line);
        if Some(i) == front_matter {
            in_front = true;
            out.push("---".into());
            continue;
        }
        if in_front {
            if trimmed == "---" {
                in_front = false;
                out.push("---".into());
            } else if trimmed.is_empty() {
                out.blank();
            } else {
                out.push(line.trim_end().to_owned());
            }
            continue;
        }
        if trimmed.is_empty() {
            out.blank();
            continue;
        }
        if trimmed.starts_with("%%{") {
            in_init = true;
        }
        if in_init {
            out.push(trimmed.to_owned());
            if trimmed.contains("}%%") {
                in_init = false;
            }
            continue;
        }
        if trimmed == "---" || re!(r"(?i)^title\s*:").is_match(trimmed) {
            out.push(trimmed.to_owned());
            continue;
        }
        let stripped = strip_strings(trimmed);
        let mid = re!(r"(?i)^(else|and|option|section)\b").is_match(&stripped);
        let closer = !mid && mermaid_closer(&stripped);
        let opener = braces(&stripped).open == 0 && mermaid_opener(trimmed);
        out.indented(trimmed, &stripped, mid, closer, opener);
    }
    out.finish(ended_with_newline)
}

fn mermaid_closer(t: &str) -> bool {
    re!(r"(?i)^end(?:note)?\b").is_match(t) || re!(r"^\}\s*$").is_match(t)
}

fn mermaid_opener(t: &str) -> bool {
    if re!(r"(?i)^(else|and|option|section)\b").is_match(t) || mermaid_closer(t) {
        return false;
    }
    re!(r"(?i)^(subgraph|loop|alt|opt|par|critical|break|rect|box)\b").is_match(t)
        || (re!(r"(?i)^note\b").is_match(t) && !t.contains(':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_by_hand_stay_at_the_end() {
        let code = "@startuml\nstate A {\nA1 --> A2\n}\nA --> B\n\n' lines by hand (Omaspec)\n' A -> B at 0.5 -60\n@enduml\n";
        let want = "@startuml\nstate A {\n  A1 --> A2\n}\nA --> B\n\n' lines by hand (Omaspec)\n' A -> B at 0.5 -60\n@enduml\n";
        let formatted = format(code, Language::PlantUml);
        assert_eq!(formatted, want);
        assert_eq!(omaspec_uml::hand::lines(&formatted).len(), 1);
        let flow = "flowchart LR\nA --> B\n\n%% lines by hand (Omaspec)\n%% A -> B at 0.5 -60\n";
        let formatted = format(flow, Language::Mermaid);
        assert_eq!(omaspec_uml::hand::lines(&formatted).len(), 1, "{formatted}");
    }

    #[test]
    fn indents_plantuml_blocks() {
        let code = "@startuml\nalt ok\nA -> B\nelse\n    loop\nB -> A\nend\nend\nnote left\ntext\nend note\n@enduml\n";
        let want = "@startuml\nalt ok\n  A -> B\nelse\n  loop\n    B -> A\n  end\nend\nnote left\n  text\nend note\n@enduml\n";
        assert_eq!(format(code, Language::PlantUml), want);
    }

    #[test]
    fn braces_and_strings() {
        let code = "@startuml\nclass A {\n+x : int\n}\npackage P {\nclass \"{odd}\"\n}\n@enduml";
        let want =
            "@startuml\nclass A {\n  +x : int\n}\npackage P {\n  class \"{odd}\"\n}\n@enduml";
        assert_eq!(format(code, Language::PlantUml), want);
    }

    #[test]
    fn a_story_map_stands_by_its_activities_steps_and_stories() {
        let code = "@startstorymap\ntitle Shop\nrelease MVP\nactivity Find\n* Browse\n      step Search\n* By title : MVP\n' later\n\n\nactivity Buy\n   step Pay\n@endstorymap\n";
        let want = "@startstorymap\ntitle Shop\nrelease MVP\nactivity Find\n  * Browse\n  step Search\n    * By title : MVP\n    ' later\n\nactivity Buy\n  step Pay\n@endstorymap\n";
        assert_eq!(format(code, Language::PlantUml), want);
        assert_eq!(format(want, Language::PlantUml), want);
        // A release written amid the stories: the next one is still its
        // step's.
        let code = "@startstorymap\nactivity Find\nstep Search\nrelease MVP\n* By title : MVP\n@endstorymap";
        let want = "@startstorymap\nactivity Find\n  step Search\nrelease MVP\n    * By title : MVP\n@endstorymap";
        assert_eq!(format(code, Language::PlantUml), want);
    }

    #[test]
    fn blank_lines_and_verbatim_blocks() {
        let code = "@startjson\n  {\"a\":   1}\n@endjson\n\n\n\n@startuml\nA->B   \n@enduml\n\n";
        let want = "@startjson\n  {\"a\":   1}\n@endjson\n\n@startuml\nA->B\n@enduml\n";
        assert_eq!(format(code, Language::PlantUml), want);
    }

    #[test]
    fn indents_mermaid_blocks() {
        let code = "sequenceDiagram\nloop every minute\nA->>B: ping\nalt ok\nB-->>A: pong\nelse\nB-->>A: error\nend\nend";
        let want = "sequenceDiagram\nloop every minute\n  A->>B: ping\n  alt ok\n    B-->>A: pong\n  else\n    B-->>A: error\n  end\nend";
        assert_eq!(format(code, Language::Mermaid), want);
    }

    #[test]
    fn keeps_mermaid_mindmap_indentation() {
        let code = "mindmap\n  root((x))\n      a   \n\n\n    b";
        assert_eq!(
            format(code, Language::Mermaid),
            "mindmap\n  root((x))\n      a\n\n    b"
        );
    }

    #[test]
    fn keeps_front_matter_nesting() {
        let code = "---\nconfig:\n  theme: dark\n---\nflowchart TD\nsubgraph S\nA-->B\nend";
        let want = "---\nconfig:\n  theme: dark\n---\nflowchart TD\nsubgraph S\n  A-->B\nend";
        assert_eq!(format(code, Language::Mermaid), want);
    }

    #[test]
    fn indents_preprocessor_blocks() {
        let code = "@startuml\n!procedure $p($a)\n!if $a\n$a -> B\n!endif\n!endprocedure\n!function $f() !return 1\nA -> B\n@enduml";
        let want = "@startuml\n!procedure $p($a)\n  !if $a\n    $a -> B\n  !endif\n!endprocedure\n!function $f() !return 1\nA -> B\n@enduml";
        assert_eq!(format(code, Language::PlantUml), want);
    }

    #[test]
    fn is_idempotent() {
        let code = "@startuml\nif (x) then\n:a;\nelseif (y)\n:b;\nendif\n@enduml";
        let once = format(code, Language::PlantUml);
        assert_eq!(format(&once, Language::PlantUml), once);
    }
}
