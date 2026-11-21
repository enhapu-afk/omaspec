//! PlantUML's *Language Reference Guide* as a yardstick: every example in
//! it drawn by Omaspec's own engines, held to `tests/guide.tsv`.
//!
//! The guide is PlantUML's text and stays out of the repository; the test
//! reads a copy of the PDF (pdf.plantuml.net) through `pdftotext -layout`:
//!
//! ```text
//! OMASPEC_GUIDE=PlantUML_Language_Reference_Guide_en.pdf \
//!   cargo test --release -p omaspec-render --test guide -- --nocapture
//! ```
//!
//! An example that drew before and declines now fails the test; the
//! counts per chapter are printed. `UPDATE_GUIDE=1` writes what is drawn
//! now into the baseline. `OMASPEC_GUIDE_CHAPTER=N` lists what chapter N
//! declines, with its page and reason; `OMASPEC_GUIDE_DIR=DIR` writes each
//! example there (`0042-p024.puml`), to look at and to draw. Which examples count (`valid`) was decided once,
//! with PlantUML 1.2026.8 drawing each of them: what it refuses does not
//! count, and neither do the network diagrams it now wants opened with
//! `@startnwdiag` (the guide opens them with `@startuml`) count against it.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

use omaspec_core::Language;
use omaspec_render::{Config, Renderer};
use regex::Regex;

/// One example: its number in the guide, the printed page it starts on,
/// its chapter and its code.
struct Example {
    n: usize,
    page: usize,
    chapter: String,
    code: String,
}

/// The examples of the guide's text, from each `@start…` to its `@end…`,
/// the running header and footer of a page left out where one breaks an
/// example.
fn examples(text: &str) -> Vec<Example> {
    let footer =
        Regex::new(r"^\s*PlantUML Language Reference Guide \(1\.\d+\.\d+\)\s+\d+\s*/\s*\d+\s*$")
            .unwrap();
    // "1.2   Declaring participant        1 SEQUENCE DIAGRAM", or a
    // chapter's first page with the chapter alone; a long section title
    // wraps over two or three lines.
    let header = Regex::new(r"^\s*(\d+(?:\.\d+)*\s{2,}\S|\s{20,}\d+\s+[A-Z])").unwrap();
    let chapter_of = Regex::new(r"(\d+)\s+([A-Z][A-Z0-9 ()/&,.'-]+)$").unwrap();
    let start = Regex::new(r"^@start[a-z]+").unwrap();
    let end = Regex::new(r"^@end[a-z]+").unwrap();
    let mut out = Vec::new();
    let mut chapter = String::new();
    let mut block: Option<(usize, String, Vec<&str>)> = None;
    for (index, page) in text.split('\u{c}').enumerate() {
        let lines: Vec<&str> = page.split('\n').collect();
        let mut skip = 0;
        if lines.first().is_some_and(|l| header.is_match(l)) {
            while skip < lines.len() && !lines[skip].trim().is_empty() {
                skip += 1;
            }
            let joined = lines[..skip]
                .iter()
                .map(|l| l.trim())
                .collect::<Vec<_>>()
                .join(" ");
            if let Some(c) = chapter_of.captures(&joined) {
                chapter = format!("{} {}", &c[1], c[2].trim());
            }
        }
        for line in &lines[skip..] {
            if footer.is_match(line) {
                continue;
            }
            let trimmed = line.trim();
            match &mut block {
                None if start.is_match(trimmed) => {
                    // The guide's printed page: the PDF's first is its cover.
                    block = Some((index, chapter.clone(), vec![line.trim_end()]));
                }
                None => {}
                Some((_, _, body)) => {
                    body.push(line.trim_end());
                    if end.is_match(trimmed) {
                        let (page, chapter, body) = block.take().unwrap();
                        let indent = body
                            .iter()
                            .filter(|l| !l.trim().is_empty())
                            .map(|l| l.len() - l.trim_start().len())
                            .min()
                            .unwrap_or(0);
                        let code: Vec<&str> =
                            body.iter().map(|l| l.get(indent..).unwrap_or("")).collect();
                        out.push(Example {
                            n: out.len() + 1,
                            page,
                            chapter,
                            code: code.join("\n") + "\n",
                        });
                    }
                }
            }
        }
    }
    out
}

/// One row of the baseline.
struct Known {
    chapter: String,
    valid: bool,
    drawn: bool,
}

fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/guide.tsv")
}

fn baseline() -> BTreeMap<usize, Known> {
    let text = std::fs::read_to_string(baseline_path()).unwrap();
    text.lines()
        .skip(1)
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let n = f[0].parse().unwrap();
            let known = Known {
                chapter: f[2].to_owned(),
                valid: f[3] == "1",
                drawn: f[4] == "1",
            };
            (n, known)
        })
        .collect()
}

/// Whether Omaspec's own engines draw each example.
fn drawn(examples: &[Example]) -> Vec<Result<(), String>> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = examples.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = examples
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    let mut renderer = Renderer::new(&Config {
                        java_fallback: false,
                        ..Config::default()
                    });
                    part.iter()
                        .map(|example| {
                            let drawn =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    renderer.render(&example.code, Language::PlantUml)
                                }));
                            match drawn {
                                Ok(Ok(_)) => Ok(()),
                                Ok(Err(err)) => Err(err.to_string()),
                                Err(_) => Err("panicked".to_owned()),
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

#[test]
fn the_guide_is_drawn_as_far_as_it_was() {
    let Some(pdf) = std::env::var_os("OMASPEC_GUIDE") else {
        eprintln!("skipped: OMASPEC_GUIDE names no copy of PlantUML's guide");
        return;
    };
    let text = Command::new("pdftotext")
        .args(["-layout".as_ref(), pdf.as_os_str(), "-".as_ref()])
        .output()
        .expect("pdftotext (poppler)");
    assert!(text.status.success(), "pdftotext failed");
    let examples = examples(&String::from_utf8_lossy(&text.stdout));
    let known = baseline();
    assert_eq!(
        examples.len(),
        known.len(),
        "the guide holds another number of examples than the baseline: another edition?"
    );
    let results = drawn(&examples);
    if let Some(dir) = std::env::var_os("OMASPEC_GUIDE_DIR") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for example in &examples {
            let name = format!("{:04}-p{:03}.puml", example.n, example.page);
            std::fs::write(dir.join(name), &example.code).unwrap();
        }
    }
    let shown = std::env::var("OMASPEC_GUIDE_CHAPTER").ok();

    // Per chapter: valid examples, drawn before, drawn now.
    let mut chapters: BTreeMap<(usize, String), [usize; 3]> = BTreeMap::new();
    let mut lost = Vec::new();
    let mut gained = 0;
    for (example, result) in examples.iter().zip(&results) {
        let was = &known[&example.n];
        if !was.valid {
            continue;
        }
        let number: usize = was.chapter.split(' ').next().unwrap().parse().unwrap();
        let counts = chapters.entry((number, was.chapter.clone())).or_default();
        counts[0] += 1;
        counts[1] += usize::from(was.drawn);
        counts[2] += usize::from(result.is_ok());
        if let (Some(shown), Err(err)) = (&shown, result)
            && *shown == number.to_string()
        {
            eprintln!("{:04} p{:<4} {err}", example.n, example.page);
        }
        match (was.drawn, result) {
            (true, Err(err)) => lost.push(format!(
                "example {} (page {}, {}): {err}",
                example.n, example.page, example.chapter
            )),
            (false, Ok(())) => gained += 1,
            _ => {}
        }
    }
    let mut report = String::new();
    let [mut valid, mut before, mut now] = [0; 3];
    for ((_, name), [v, b, n]) in &chapters {
        let _ = writeln!(report, "{n:4} of {v:<4} (was {b:4})  {name}");
        valid += v;
        before += b;
        now += n;
    }
    let _ = writeln!(
        report,
        "{now:4} of {valid:<4} (was {before:4})  drawn natively, {:.1} %; {gained} more than before",
        100.0 * now as f64 / valid as f64
    );
    eprintln!("{report}");

    if std::env::var_os("UPDATE_GUIDE").is_some() {
        let mut tsv = String::from("n\tpage\tchapter\tvalid\tdrawn\n");
        for (example, result) in examples.iter().zip(&results) {
            let was = &known[&example.n];
            let _ = writeln!(
                tsv,
                "{}\t{}\t{}\t{}\t{}",
                example.n,
                example.page,
                was.chapter,
                u8::from(was.valid),
                u8::from(result.is_ok())
            );
        }
        std::fs::write(baseline_path(), tsv).unwrap();
    }
    assert!(
        lost.is_empty(),
        "{} examples that drew natively decline now:\n{}",
        lost.len(),
        lost.join("\n")
    );
}

/// What the cutting keeps: an example broken by a page, its header and
/// footer left out; the chapter from the running header.
#[test]
fn an_example_across_a_page_is_cut_whole() {
    let text = concat!(
        "1.2   Declaring participant                       1 SEQUENCE DIAGRAM\n",
        "\n",
        "Some words.\n",
        "  @startuml\n",
        "  Alice -> Bob\n",
        "\n",
        "       PlantUML Language Reference Guide (1.2025.0)          2 / 606\n",
        "\u{c}1.3   A section whose title is long enough to          1 SEQUENCE\n",
        "      wrap                                                  DIAGRAM\n",
        "\n",
        "    Bob -> Alice\n",
        "  @enduml\n",
    );
    let found = examples(text);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].chapter, "1 SEQUENCE DIAGRAM");
    assert_eq!(found[0].page, 0);
    let lines: Vec<&str> = found[0].code.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines,
        ["@startuml", "Alice -> Bob", "  Bob -> Alice", "@enduml"]
    );
}
