//! Robustness: thousands of broken and mutated diagrams through every
//! text function, from a fixed seed so a failure repeats. Nothing may
//! panic, line numbers stay inside the text, formatting settles after one
//! pass. (Deeper, coverage-guided fuzzing: `fuzz/`, with cargo-fuzz.)
//!
//! `OMASPEC_ROBUST_ROUNDS` sets how many inputs to try (default 1000),
//! `OMASPEC_ROBUST_SEED` another start (a failure names its round).

use std::path::Path;

use omaspec_core::skin::{self, Colors, Pack};
use omaspec_core::{
    Kind, Language, classify, detect, diff, embed, format, lang, lint, locate, outline,
};

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

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Pieces that mean something to the parsers, and some that break things.
const TOKENS: &[&str] = &[
    "@startuml",
    "@enduml",
    "@startmindmap",
    "@endmindmap",
    "alt",
    "else",
    "end",
    "loop",
    "opt",
    "note left",
    "end note",
    "note over A, B",
    "partition",
    "if (x) then (yes)",
    "endif",
    "while (x)",
    "endwhile",
    "fork",
    "end fork",
    "state X {",
    "}",
    "{",
    "[*] -->",
    "-->",
    "->",
    "->>",
    "-->>",
    "|Lane|",
    ":act;",
    "' comment",
    "/'",
    "'/",
    "!include x",
    "skinparam x y",
    "sequenceDiagram",
    "flowchart LR",
    "graph TD",
    "stateDiagram-v2",
    "classDiagram",
    "gantt",
    "pie",
    "mindmap",
    "subgraph S",
    "participant A as B",
    "%%{init: {\"theme\": \"dark\"}}%%",
    "%%{init:",
    "}%%",
    "%%",
    "---",
    "title: x",
    "```mermaid",
    "```",
    "A[label]",
    "B{x}",
    "C((y))",
    "|edge|",
    "\"",
    "'",
    "\\n",
    "<br>",
    "&lt;",
    "==",
    "== Section ==",
    "class A",
    "<<stereo>>",
    "\t",
    "  ",
    "\r",
    "\0",
    "é",
    "\u{301}",
    "\u{202e}",
    "\u{feff}",
    "😀",
    "\u{fffd}",
    "ẞ",
];

fn fixtures() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut texts: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .collect();
    texts.sort();
    texts
}

/// A broken diagram: a fixture with lines dropped, doubled, swapped or
/// spiked with tokens, cut anywhere; or plain noise.
fn input(rng: &mut Rng, fixtures: &[String]) -> String {
    if rng.below(8) == 0 {
        let len = rng.below(400);
        let bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        return String::from_utf8_lossy(&bytes).into_owned();
    }
    let mut lines: Vec<String> = rng.pick(fixtures).split('\n').map(str::to_owned).collect();
    for _ in 0..=rng.below(6) {
        let at = rng.below(lines.len() + 1);
        match rng.below(6) {
            0 if !lines.is_empty() => {
                lines.remove(at.min(lines.len() - 1));
            }
            1 if !lines.is_empty() => {
                let line = lines[at.min(lines.len() - 1)].clone();
                lines.insert(at.min(lines.len()), line);
            }
            2 if lines.len() > 1 => {
                let (one, other) = (at.min(lines.len() - 1), rng.below(lines.len()));
                lines.swap(one, other);
            }
            3 => lines.insert(at.min(lines.len()), (*rng.pick(TOKENS)).to_owned()),
            _ if !lines.is_empty() => {
                let i = at.min(lines.len() - 1);
                let line = &mut lines[i];
                let cut = line
                    .char_indices()
                    .map(|(i, _)| i)
                    .nth(rng.below(line.chars().count() + 1))
                    .unwrap_or(line.len());
                line.insert_str(cut, rng.pick(TOKENS));
            }
            _ => lines.push((*rng.pick(TOKENS)).to_owned()),
        }
    }
    let text = lines.join(if rng.below(10) == 0 { "\r\n" } else { "\n" });
    // Sometimes cut off mid-way, on a char boundary.
    if rng.below(5) == 0 {
        let cut = text
            .char_indices()
            .map(|(i, _)| i)
            .nth(rng.below(text.chars().count() + 1))
            .unwrap_or(text.len());
        return text[..cut].to_owned();
    }
    text
}

/// The seed for one test, moved by `OMASPEC_ROBUST_SEED`.
fn seed(base: u64) -> Rng {
    let extra: u64 = std::env::var("OMASPEC_ROBUST_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    Rng(base ^ extra.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
}

fn rounds() -> usize {
    std::env::var("OMASPEC_ROBUST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000)
}

const COLORS: Colors = Colors {
    background: "#121212",
    surface: "#1e1e1e",
    text: "#bebebe",
    accent: "#e68e0d",
    muted: "#555555",
    note: "#2a2a2a",
};

#[test]
fn text_functions_survive_broken_diagrams() {
    let fixtures = fixtures();
    let mut rng = seed(0x5eed_5eed_5eed_5eed);
    for round in 0..rounds() {
        let code = input(&mut rng, &fixtures);
        let lines = code.split('\n').count();
        let context = || format!("round {round}: {code:?}");
        let _ = lang::unwrap_paste(&code);
        let _ = lang::looks_like_diagram(&code);
        let _ = lang::strip_mermaid_fence(&code);
        let detected = detect(&code, None);
        for language in [Language::PlantUml, Language::Mermaid, detected] {
            let _ = classify(&code, language);
            for finding in lint::lint(&code, language) {
                assert!(
                    finding.line < lines,
                    "lint line {} {}",
                    finding.line,
                    context()
                );
            }
            for entry in outline::outline(&code, language) {
                assert!(
                    entry.line < lines,
                    "outline line {} {}",
                    entry.line,
                    context()
                );
            }
            let once = format::format(&code, language);
            assert_eq!(
                format::format(&once, language),
                once,
                "format settles {}",
                context()
            );
            for pack in [Some(*rng.pick(&Pack::ALL)), None] {
                if let Some((range, text)) = skin::edit(&code, language, pack, &COLORS) {
                    assert!(
                        range.end <= code.len() && range.start <= range.end,
                        "{}",
                        context()
                    );
                    assert!(code.is_char_boundary(range.start) && code.is_char_boundary(range.end));
                    let _ = skin::current(&format!(
                        "{}{text}{}",
                        &code[..range.start],
                        &code[range.end..]
                    ));
                }
            }
        }
        let label: String = code
            .split_whitespace()
            .nth(rng.below(8))
            .unwrap_or("x")
            .to_owned();
        if let Some(line) = locate::line_of(&code, &label, rng.below(3)) {
            assert!(line < lines, "locate line {line} {}", context());
        }
        let other = input(&mut rng, &fixtures);
        let _ = diff::line_diff(&code, &other);
        for change in diff::changes(&code, &other) {
            let other_lines = other.split('\n').count();
            match change {
                diff::Change::Lines(range) => assert!(range.end <= other_lines, "{}", context()),
                diff::Change::Removed(at) => assert!(at <= other_lines, "{}", context()),
            }
        }
    }
}

/// Every PlantUML diagram in the repository: the fixtures here, the
/// engine's and the examples.
fn plantuml_diagrams() -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "puml") {
                out.extend(std::fs::read_to_string(&path).ok());
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut texts = Vec::new();
    for dir in ["tests/fixtures", "../omaspec-uml/tests", "../../examples"] {
        walk(&root.join(dir), &mut texts);
    }
    texts.sort();
    texts
}

#[test]
fn classify_names_the_type_the_engine_draws() {
    let diagrams = plantuml_diagrams();
    assert!(diagrams.len() > 40, "{}", diagrams.len());
    let mut rng = seed(0x0c1a_551f_0c1a_551f);
    let mut wrong = Vec::new();
    for round in 0..rounds() * 4 {
        let code = if round < diagrams.len() {
            diagrams[round].clone()
        } else {
            input(&mut rng, &diagrams)
        };
        let Ok(diagram) = omaspec_uml::parse(&code) else {
            continue;
        };
        // An empty sequence diagram is what PlantUML draws for nothing; a
        // line such as `autonumber` may already name it a sequence.
        let empty = matches!(&diagram, omaspec_uml::Diagram::Sequence(d)
            if d.participants.is_empty() && d.items.is_empty());
        let drawn = match diagram {
            omaspec_uml::Diagram::Sequence(_) => Kind::Sequence,
            omaspec_uml::Diagram::Activity(_) => Kind::Activity,
            omaspec_uml::Diagram::State(_) => Kind::State,
            omaspec_uml::Diagram::Structure(_) => Kind::Structure,
            omaspec_uml::Diagram::Timing(_) => Kind::Timing,
            _ => Kind::Other,
        };
        let named = classify(&code, Language::PlantUml).kind;
        if named != drawn && !(empty && named == Kind::Other) {
            wrong.push(format!(
                "round {round}: drawn {drawn:?}, named {named:?}: {code:?}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of them:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn image_readers_survive_broken_images() {
    let mut rng = seed(0x1234_5678_9abc_def0);
    let code = "@startuml\nA -> B : hi\n@enduml\n";
    // A real PNG and SVG with a source inside, then broken in many ways.
    let png = embed::png_with_source(TINY_PNG, code, Language::PlantUml, Some("a.puml")).unwrap();
    let svg = embed::svg_with_source(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"/>",
        code,
        Language::PlantUml,
        Some("a.puml"),
    );
    assert_eq!(embed::source_from_png(&png).unwrap().code, code);
    assert_eq!(embed::source_from_svg(&svg).unwrap().code, code);
    for _ in 0..rounds() {
        let mut bytes = if rng.below(2) == 0 {
            png.clone()
        } else {
            svg.clone().into_bytes()
        };
        for _ in 0..=rng.below(8) {
            if bytes.is_empty() {
                break;
            }
            let at = rng.below(bytes.len());
            match rng.below(4) {
                0 => bytes[at] = rng.next() as u8,
                1 => {
                    bytes.remove(at);
                }
                2 => bytes.truncate(at),
                _ => bytes.insert(at, rng.next() as u8),
            }
        }
        let _ = embed::source_from_png(&bytes);
        let _ = embed::source_from_svg(&String::from_utf8_lossy(&bytes));
    }
}

/// A 1×1 PNG.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];
