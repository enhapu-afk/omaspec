//! The `omaspec` subcommands, run as a user would.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use omaspec_core::{Language, embed};

fn omaspec(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args(args)
        .output()
        .expect("omaspec runs")
}

/// A fresh directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("omaspec-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn render_fits_a_picture_too_large_with_fit() {
    let dir = scratch("fit");
    let long = dir.join("long.puml");
    let mut source = String::from("@startuml\n");
    for i in 1..=60 {
        source.push_str(&format!(
            "Alice -> Bob : message number {i} with some words\n"
        ));
    }
    source.push_str("@enduml\n");
    fs::write(&long, source).unwrap();
    let png = dir.join("long.png");
    let args = |fit: bool| {
        let mut args = vec![
            "render",
            "--json",
            long.to_str().unwrap(),
            "-o",
            png.to_str().unwrap(),
            "--scale",
            "16",
        ];
        if fit {
            args.push("--fit");
        }
        args.into_iter().map(str::to_owned).collect::<Vec<_>>()
    };
    let refused = omaspec(&args(false).iter().map(String::as_str).collect::<Vec<_>>());
    assert!(!refused.status.success());
    let said = String::from_utf8_lossy(&refused.stderr);
    assert!(said.contains("--fit"), "{said}");
    // A debug build takes over a minute for a hundred million pixels; the
    // release build a second (`cargo test --release`).
    if cfg!(debug_assertions) {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    let fitted = omaspec(&args(true).iter().map(String::as_str).collect::<Vec<_>>());
    assert!(
        fitted.status.success(),
        "{}",
        String::from_utf8_lossy(&fitted.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&fitted.stdout).unwrap();
    let scale = report[0]["fitted_scale"].as_f64().unwrap();
    assert!(scale > 1.0 && scale < 16.0, "{scale}");
    // The PNG's header: its width and height, within the limit.
    let bytes = fs::read(&png).unwrap();
    let side = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    assert!(u64::from(side(16)) * u64::from(side(20)) <= 100_000_000);
    let _ = fs::remove_dir_all(&dir);
}

/// REQ-070: `--min-font` keeps a picture's words at least so many pixels
/// high. One that fits stays one file; one that does not is cut into
/// tiles, `NAME-tiles/1.png` and on, each within one file's size, the
/// source in each, and one drawn as one file again takes them away;
/// `--fit` alone is as it was.
#[test]
fn render_keeps_words_readable_with_min_font() {
    let dir = scratch("min-font");
    let long = dir.join("long.puml");
    let mut source = String::from("@startuml\n");
    for i in 1..=60 {
        source.push_str(&format!(
            "Alice -> Bob : message number {i} with some words\n"
        ));
    }
    source.push_str("@enduml\n");
    fs::write(&long, &source).unwrap();
    let png = dir.join("long.png");
    let run = |least: &str, more: &[&str]| {
        let mut args = vec![
            "render",
            "--json",
            long.to_str().unwrap(),
            "-o",
            png.to_str().unwrap(),
            "--min-font",
            least,
        ];
        args.extend_from_slice(more);
        omaspec(&args)
    };
    // It fits at scale 2: one file, its words larger than asked.
    let one = run("10", &[]);
    assert!(
        one.status.success(),
        "{}",
        String::from_utf8_lossy(&one.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&one.stdout).unwrap();
    assert_eq!(report[0]["scale"].as_f64(), Some(2.0));
    assert!(report[0]["smallest_text_px"].as_f64().unwrap() >= 10.0);
    assert!(report[0]["tiles"].is_null() && png.exists());
    // A scale below what the words need is raised to it.
    let raised = run("10", &["--scale", "0.2"]);
    let report: serde_json::Value = serde_json::from_slice(&raised.stdout).unwrap();
    let px = report[0]["smallest_text_px"].as_f64().unwrap();
    assert!((10.0..10.5).contains(&px), "{px}");
    // Only a PNG or a JPEG has pixels to size.
    let svg = omaspec(&[
        "render",
        long.to_str().unwrap(),
        "-o",
        dir.join("long.svg").to_str().unwrap(),
        "--min-font",
        "10",
    ]);
    assert!(!svg.status.success());
    assert!(String::from_utf8_lossy(&svg.stderr).contains("PNG or JPEG"));
    // Nor has text; and --fit is what --min-font does already.
    let text = run("10", &["-f", "text"]);
    assert!(!text.status.success());
    let both = run("10", &["--fit"]);
    assert!(!both.status.success());
    assert!(String::from_utf8_lossy(&both.stderr).contains("cannot be used with"));
    // A debug build takes minutes for a few hundred million pixels.
    if cfg!(debug_assertions) {
        let _ = fs::remove_dir_all(&dir);
        return;
    }
    let cut = run("190", &[]);
    assert!(
        cut.status.success(),
        "{}",
        String::from_utf8_lossy(&cut.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&cut.stdout).unwrap();
    let tiles = report[0]["tiles"].as_array().unwrap();
    assert!(tiles.len() > 1, "{report}");
    assert!(report[0]["smallest_text_px"].as_f64().unwrap() >= 189.9);
    let rows = report[0]["rows"].as_u64().unwrap() as usize;
    let columns = report[0]["columns"].as_u64().unwrap() as usize;
    assert!(tiles.len() <= rows * columns);
    let mut height = 0;
    for (n, tile) in tiles.iter().enumerate() {
        let path = PathBuf::from(tile.as_str().unwrap());
        assert_eq!(path, dir.join("long-tiles").join(format!("{}.png", n + 1)));
        let bytes = fs::read(&path).unwrap();
        let side = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
        assert!(u64::from(side(16)) * u64::from(side(20)) <= 100_000_000);
        assert!(side(16) <= 16_384 && side(20) <= 16_384);
        if columns == 1 {
            height += side(20);
        }
        let stored = embed::source_from_png(&bytes).expect("each tile holds the source");
        assert_eq!(stored.code, source);
    }
    if columns == 1 {
        assert!(height > 16_384, "{height}");
    }
    // Drawn again as one file, the tiles are taken away, their folder too.
    let one = run("10", &[]);
    assert!(one.status.success());
    assert!(!dir.join("long-tiles").exists());
    // Tiles are files, not something to paste.
    let copied = omaspec(&[
        "render",
        long.to_str().unwrap(),
        "--min-font",
        "190",
        "--copy",
        "-f",
        "png",
    ]);
    assert!(!copied.status.success());
    assert!(String::from_utf8_lossy(&copied.stderr).contains("tiles"));
    let _ = fs::remove_dir_all(&dir);
}

/// REQ-065: a refusal that points at another line counts the way there,
/// which holds where a look lays its skin before the diagram; it said
/// "line 75" of a file of ten.
#[test]
fn a_refusal_points_at_its_other_line_in_any_look() {
    let dir = scratch("other-line");
    let file = dir.join("flight.puml");
    fs::write(
        &file,
        "@startuml\nstate \"Engine cut off\" as Cutoff\n[*] --> Flight\nstate Flight {\n  [*] --> Ascent\n  ||\n  [*] --> Idle\n  Idle --> Cutoff\n}\n@enduml\n",
    )
    .unwrap();
    let png = dir.join("flight.png");
    for look in ["light", "plain"] {
        let out = omaspec(&[
            "render",
            "--json",
            "--look",
            look,
            file.to_str().unwrap(),
            "-o",
            png.to_str().unwrap(),
        ]);
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(report[0]["line"].as_u64(), Some(8), "{report}");
        let message = report[0]["message"].as_str().unwrap();
        assert!(message.contains("named first 6 lines above"), "{message}");
    }
    let _ = fs::remove_dir_all(&dir);
}

/// REQ-068: `--text-as-paths` writes an SVG whose words are outlines; a
/// PNG has pixels already and refuses it.
#[test]
fn render_writes_the_words_as_outlines() {
    let dir = scratch("outlines");
    let file = dir.join("door.puml");
    fs::write(
        &file,
        "@startuml\n[*] --> Closed\nClosed --> Open : open\n@enduml\n",
    )
    .unwrap();
    let svg = dir.join("door.svg");
    let out = omaspec(&[
        "render",
        file.to_str().unwrap(),
        "-o",
        svg.to_str().unwrap(),
        "--text-as-paths",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let written = fs::read_to_string(&svg).unwrap();
    assert!(!written.contains("<text") && written.contains("<path"));
    let png = omaspec(&[
        "render",
        file.to_str().unwrap(),
        "-o",
        dir.join("door.png").to_str().unwrap(),
        "--text-as-paths",
    ]);
    assert!(!png.status.success());
    // The check draws the one beside the diagram as it was written: with
    // outlines, asked for or not.
    let beside = |more: &[&str]| {
        let mut args = vec!["render", file.to_str().unwrap()];
        args.extend(more);
        omaspec(&args)
    };
    for more in [&["--check"][..], &["--check", "--text-as-paths"]] {
        let held = beside(more);
        assert!(
            held.status.success(),
            "{more:?}: {}",
            String::from_utf8_lossy(&held.stderr)
        );
    }
    // Both color schemes keep their colors as outlines.
    let auto = beside(&["--look", "auto", "--text-as-paths"]);
    assert!(auto.status.success());
    let written = fs::read_to_string(&svg).unwrap();
    assert!(!written.contains("<text"));
    assert!(written.contains("prefers-color-scheme: dark"), "{written}");
    assert!(beside(&["--check", "--look", "auto"]).status.success());
    // Words where outlines are asked for differ.
    assert!(beside(&[]).status.success());
    assert!(!beside(&["--check", "--text-as-paths"]).status.success());
    let _ = fs::remove_dir_all(&dir);
}

/// Every finding has a code a program can hold on to, and the guide
/// lists each one: a new kind of finding is written down there too.
#[test]
fn every_finding_code_is_in_the_guide() {
    fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let guide = fs::read_to_string(root.join("docs/guide/reference.md")).unwrap();
    let mut files = Vec::new();
    for krate in ["omaspec-core", "omaspec-model", "omaspec-app"] {
        sources(&root.join("crates").join(krate).join("src"), &mut files);
    }
    let mut codes = std::collections::BTreeSet::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for call in ["Finding::new(", "finding("] {
            for (at, _) in text.match_indices(call) {
                let rest = text[at + call.len()..].trim_start();
                let Some(code) = rest.strip_prefix('"').and_then(|r| r.split('"').next()) else {
                    continue;
                };
                let kebab =
                    !code.is_empty() && code.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
                // The two a fix carries inside the lint are never reported.
                if kebab && !matches!(code, "fix" | "nothing-to-fix") {
                    codes.insert(code.to_owned());
                }
            }
        }
    }
    for more in ["transition-twice", "ambiguous-event"] {
        codes.insert(more.to_owned());
    }
    assert!(codes.len() >= 30, "{codes:?}");
    for code in &codes {
        assert!(
            guide.contains(&format!("`{code}`")),
            "{code} is not in the guide"
        );
    }
    // The report carries it.
    let dir = scratch("codes");
    let file = dir.join("a.puml");
    fs::write(&file, "@startuml\n[*] --> A\nA --> B\n@enduml\n").unwrap();
    let out = omaspec(&["lint", "--json", file.to_str().unwrap()]);
    assert!(
        stdout(&out).contains(r#""code": "dead-end""#),
        "{}",
        stdout(&out)
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn render_names_the_characters_no_font_draws() {
    let dir = scratch("undrawable");
    let flow = dir.join("flow.mmd");
    // U+0378 is not assigned, so no font draws it.
    fs::write(
        &flow,
        "flowchart LR\n  A[Unassigned \u{378} here] --> B[Plain]\n",
    )
    .unwrap();
    let png = dir.join("flow.png");
    let out = omaspec(&[
        "render",
        flow.to_str().unwrap(),
        "-o",
        png.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("warning: no installed font draws \u{378}"),
        "{said}"
    );
    let out = omaspec(&[
        "render",
        "--json",
        flow.to_str().unwrap(),
        "-o",
        png.to_str().unwrap(),
    ]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report[0]["undrawable"][0], "\u{378}");
    // An SVG's words are set where it is shown: nothing is said.
    let svg = dir.join("flow.svg");
    let out = omaspec(&[
        "render",
        flow.to_str().unwrap(),
        "-o",
        svg.to_str().unwrap(),
    ]);
    assert!(!String::from_utf8_lossy(&out.stderr).contains("warning"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn lint_fails_on_errors_and_with_strict_on_warnings() {
    let dir = scratch("lint");
    let broken = dir.join("broken.puml");
    fs::write(&broken, "@startuml\nA -> B\n").unwrap();
    let warned = dir.join("warned.mmd");
    fs::write(&warned, "sequenceDiagram\nloop forever\nA->>B: x\n").unwrap();

    let out = omaspec(&["lint", broken.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("broken.puml:1: error: @startuml without matching @enduml"));

    let out = omaspec(&["lint", warned.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("warned.mmd:2: warning: Unclosed loop"));
    let out = omaspec(&["lint", "--strict", warned.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    // A diagram only the Java PlantUML draws is a hint, and with --strict
    // a failure: it is drawn nowhere Java is not.
    let java = dir.join("java.puml");
    fs::write(&java, "@startsalt\n{S\n  Line one\n}\n@endsalt\n").unwrap();
    let out = omaspec(&["lint", java.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert!(stdout(&out).contains("only the Java PlantUML draws"));
    let out = omaspec(&["lint", "--strict", java.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    // So is one that takes what Omaspec lacks of PlantUML's standard
    // library (REQ-115), not one of C4, which it draws.
    let aws = dir.join("aws.puml");
    fs::write(
        &aws,
        "@startuml\n!include <awslib/AWSCommon>\nA -> B\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&["lint", "--strict", aws.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out)
            .contains("aws.puml:2: info: `<awslib/AWSCommon>` is in PlantUML's standard library"),
        "{}",
        stdout(&out)
    );
    let c4 = dir.join("c4.puml");
    fs::write(
        &c4,
        "@startuml\n!include <C4/C4_Container>\nPerson(u, \"User\")\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&["lint", "--strict", c4.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));

    let out = omaspec(&["lint", "--json", broken.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["line"], 1);
    assert_eq!(json[0]["severity"], "error");
}

/// `trace --follow` says each finding the moment its line is complete,
/// as JSON Lines with `--json`: a file not there yet is waited for, a
/// line half written waits for its end (docs/adr/0036).
#[test]
fn trace_follows_a_trace_while_it_is_written() {
    use std::io::{BufRead, BufReader, Write};
    let dir = scratch("trace-follow");
    fs::write(
        dir.join("door.puml"),
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
    )
    .unwrap();
    let live = dir.join("live.jsonl");
    let mut child = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args(["trace", "--follow", "--json", live.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let (sent, said) = std::sync::mpsc::channel();
    let out = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            if sent.send(line).is_err() {
                break;
            }
        }
    });
    let next = || said.recv_timeout(std::time::Duration::from_secs(10));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&live)
        .unwrap();
    // A line that holds, then one that deviates, its end written later.
    file.write_all(b"{\"object\":\"Door\",\"event\":\"open\"}\n{\"object\":\"Door\",")
        .unwrap();
    file.flush().unwrap();
    assert!(
        said.recv_timeout(std::time::Duration::from_millis(600))
            .is_err(),
        "nothing yet"
    );
    file.write_all(b"\"event\":\"open\"}\n").unwrap();
    file.flush().unwrap();
    let json: serde_json::Value = serde_json::from_str(&next().unwrap()).unwrap();
    assert_eq!(json["line"], 2);
    assert_eq!(json["code"], "trace-deviation");
    assert_eq!(json["task"]["expected"]["events"][0]["event"], "close");
    let _ = child.kill();
    let _ = child.wait();
    // Standard input is followed until it ends.
    let mut child = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args(["trace", "--follow", "--project", dir.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"object\":\"Door\",\"event\":\"close\"}\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stdout(&out).contains("<stdin>:1: error: Door does not take close here"),
        "{}",
        stdout(&out)
    );
    // What is no file, a pipe, is read as it streams, until it ends.
    #[cfg(unix)]
    {
        let mut child = Command::new(env!("CARGO_BIN_EXE_omaspec"))
            .args([
                "trace",
                "--follow",
                "/dev/stdin",
                "--project",
                dir.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"{\"object\":\"Door\",\"event\":\"close\"}\n")
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
        assert!(
            stdout(&out).contains("/dev/stdin:1: error: Door does not take close here"),
            "{}",
            stdout(&out)
        );
    }
}

#[test]
fn trace_holds_a_recording_against_the_machines_of_its_folder() {
    let dir = scratch("trace");
    fs::write(
        dir.join("door.puml"),
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n",
    )
    .unwrap();
    let good = dir.join("good.jsonl");
    fs::write(
        &good,
        "{\"object\":\"Door\",\"event\":\"lock\"}\n{\"object\":\"Door\",\"event\":\"unlock\",\"state\":\"Closed\"}\n{\"object\":\"Car\",\"event\":\"park\"}\n",
    )
    .unwrap();
    let bad = dir.join("bad.jsonl");
    fs::write(
        &bad,
        "{\"object\":\"Door\",\"event\":\"lock\"}\n{\"time\":\"10:30\",\"object\":\"Door\",\"event\":\"open\"}\n",
    )
    .unwrap();

    let out = omaspec(&["trace", good.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stdout(&out).contains("good.jsonl:3: info: No state machine of the project is named Car")
    );

    let out = omaspec(&["trace", bad.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stdout(&out).contains("bad.jsonl:2: error: At 10:30, Door does not take open here"),
        "{}",
        stdout(&out)
    );

    // Elsewhere, the folder that holds the machines is named.
    let other = scratch("trace-elsewhere");
    let moved = other.join("bad.jsonl");
    fs::copy(&bad, &moved).unwrap();
    let out = omaspec(&["trace", "--json", moved.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["severity"], "info", "no machine there");
    let out = omaspec(&[
        "trace",
        "--json",
        "--project",
        dir.to_str().unwrap(),
        moved.to_str().unwrap(),
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["line"], 2);
    assert_eq!(json[0]["severity"], "error");
    // A deviation is a task: what happened, and what the machine expected
    // there with the lines of the diagram (docs/trace.md).
    let task = &json[0]["task"];
    assert_eq!(task["object"], "Door");
    assert_eq!(task["happened"]["event"], "open");
    assert_eq!(task["happened"]["time"], "10:30");
    assert_eq!(
        task["expected"]["states"],
        serde_json::json!([{"name": "Locked", "line": 5}])
    );
    assert_eq!(
        task["expected"]["events"],
        serde_json::json!([{"event": "unlock", "to": "Closed", "line": 6}])
    );
    // The format's schema, for whoever writes traces.
    let out = omaspec(&["trace", "--schema"]);
    assert_eq!(out.status.code(), Some(0));
    let schema: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(schema["$id"], "urn:omaspec:trace:1");
    assert_eq!(schema["required"], serde_json::json!(["object"]));
    // An event or a state that is null counts as absent, as the check
    // reads it: one of them must be there and not null.
    assert_eq!(
        schema["anyOf"][0]["properties"]["event"]["type"],
        serde_json::json!(["string", "number"])
    );

    // As a timing diagram: written as PlantUML, or drawn.
    let out = omaspec(&["trace", "--as", "timing", bad.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let text = stdout(&out);
    assert!(
        text.starts_with("@startuml\ntitle bad\n") && text.contains("concise \"Door\" as R0"),
        "{text}"
    );
    assert!(
        text.contains("highlight 2 to 2.5 #FFC9C9 : line 2: Door open?"),
        "{text}"
    );
    let picture = dir.join("bad.svg");
    let out = omaspec(&[
        "trace",
        "--as",
        "timing",
        bad.to_str().unwrap(),
        "-o",
        picture.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let svg = fs::read_to_string(&picture).unwrap();
    assert!(
        svg.contains("Locked") && svg.contains("Door open?"),
        "{svg}"
    );
    let out = omaspec(&["trace", "--as", "timing", good.to_str().unwrap(), "-o", "-"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(stdout(&out).starts_with("<svg"), "{}", stdout(&out));
    let out = omaspec(&[
        "trace",
        "--as",
        "timing",
        bad.to_str().unwrap(),
        "-o",
        "x.gif",
    ]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}

#[test]
fn refs_lists_the_uses_of_a_name_across_the_project() {
    let dir = scratch("refs");
    fs::write(dir.join("omaspec-project.toml"), "name = \"house\"\n").unwrap();
    // A plan's boxes are its own words: no use of the diagram they name.
    fs::write(
        dir.join("plan.puml"),
        "@startwbs\n* House\n** Door control\n@endwbs\n",
    )
    .unwrap();
    fs::write(
        dir.join("door.puml"),
        "@startuml\ntitle Door control\n[*] --> Closed\nClosed --> Open : open\n@enduml\n",
    )
    .unwrap();
    fs::write(
        dir.join("visit.puml"),
        "@startuml\nparticipant Guest\nparticipant DoorControl\nGuest -> DoorControl : open\n@enduml\n",
    )
    .unwrap();
    let project = dir.to_str().unwrap();
    let out = omaspec(&["refs", "door_control", "--project", project]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stdout(&out),
        "door.puml:2: diagram: title Door control\n\
         visit.puml:3: participant: participant DoorControl\n"
    );
    let out = omaspec(&["refs", "open", "--json", "--project", project]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[2]["kind"], "message");
    assert_eq!(json[2]["machine"], "door.puml");
    let out = omaspec(&["refs", "Window", "--project", project]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("uses Window"));
}

#[test]
fn rename_renames_a_name_at_every_use_or_says_why_not() {
    let dir = scratch("rename");
    fs::write(dir.join("omaspec-project.toml"), "name = \"house\"\n").unwrap();
    let door = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
    fs::write(dir.join("door.puml"), door).unwrap();
    fs::write(
        dir.join("visit.puml"),
        "@startuml\nparticipant Guest\nparticipant Door\nGuest -> Door : open\n@enduml\n",
    )
    .unwrap();
    fs::write(
        dir.join("today.jsonl"),
        "{\"object\":\"Door\",\"event\":\"open\"}\n",
    )
    .unwrap();
    let project = dir.to_str().unwrap();

    // Open is a state and an event of the door: which, is asked.
    let out = omaspec(&["rename", "Open", "Ajar", "--project", project]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--state or --event"));

    // A dry run writes nothing.
    let out = omaspec(&[
        "rename",
        "open",
        "swing",
        "--event",
        "--dry-run",
        "--project",
        project,
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stdout(&out).starts_with("would rename the event open of door.puml to swing in 2 files:")
    );
    assert!(stdout(&out).contains("today.jsonl:1: left alone, a recorded trace"));
    assert_eq!(fs::read_to_string(dir.join("door.puml")).unwrap(), door);

    let out = omaspec(&[
        "rename",
        "open",
        "swing",
        "--event",
        "--json",
        "--project",
        project,
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["renamed"], true);
    assert_eq!(
        json["files"],
        serde_json::json!(["door.puml", "visit.puml"])
    );
    assert!(
        fs::read_to_string(dir.join("door.puml"))
            .unwrap()
            .contains("Closed --> Open : swing\n")
    );
    assert!(
        fs::read_to_string(dir.join("visit.puml"))
            .unwrap()
            .contains("Guest -> Door : swing\n")
    );
    // The trace is a record.
    assert!(
        fs::read_to_string(dir.join("today.jsonl"))
            .unwrap()
            .contains("\"open\"")
    );
}

#[test]
fn lint_names_a_link_to_a_diagram_that_is_not_there() {
    let dir = scratch("gone-links");
    fs::write(dir.join("omaspec-project.toml"), "name = \"shop\"\n").unwrap();
    fs::write(
        dir.join("checkout.puml"),
        "@startuml\nstart\n:Pay;\nstop\n@enduml\n",
    )
    .unwrap();
    let orders = dir.join("orders.puml");
    fs::write(
        &orders,
        "@startuml\nstate Pay [[checkout.svg]]\nstate Refund [[refund.svg]]\n[*] --> Pay\nPay --> Refund\nRefund --> [*]\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&["lint", orders.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "a warning");
    assert!(
        stdout(&out).ends_with(
            "orders.puml:3: warning: The link to refund.svg leads to no diagram of the project: there is none at refund.\n"
        ),
        "{}",
        stdout(&out)
    );
    assert_eq!(stdout(&out).lines().count(), 1);
    fs::write(dir.join("refund.puml"), "@startuml\nstart\nstop\n@enduml\n").unwrap();
    let out = omaspec(&["lint", orders.to_str().unwrap()]);
    assert_eq!(stdout(&out), "");
}

/// What the editor shows under the picture about the block of lines by
/// hand, lint says too, so `check` holds it in CI: a box held beside one
/// that is not there, lines kept apart at a box that is not there.
#[test]
fn lint_names_an_entry_by_hand_for_a_box_that_is_not_there() {
    let dir = scratch("by-hand");
    let machine = dir.join("door.puml");
    fs::write(
        &machine,
        "@startuml\n[*] --> Idle\nIdle --> Open\nOpen --> [*]\n\n' lines by hand (Omaspec)\n' Open after Ghost\n' apart at Nobody\n' apart at Open\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&["lint", machine.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "warnings");
    assert_eq!(
        stdout(&out),
        "door.puml:7: warning: No box is named Ghost; Open after Ghost is left out\n\
         door.puml:8: warning: No box is named Nobody; apart at Nobody is left out\n"
            .replace("door.puml", machine.to_str().unwrap())
    );
    let out = omaspec(&["lint", "--strict", machine.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let chart = dir.join("flow.mmd");
    fs::write(
        &chart,
        "flowchart TD\n  A --> C\n  B --> C\n\n%% lines by hand (Omaspec)\n%% apart at Nobody\n",
    )
    .unwrap();
    let out = omaspec(&["lint", chart.to_str().unwrap()]);
    assert!(
        stdout(&out)
            .ends_with(":6: warning: No box is named Nobody; apart at Nobody is left out\n"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn a_markdown_document_is_formatted_and_outlined_by_its_diagrams_and_converted_never() {
    let dir = scratch("document-commands");
    let doc = dir.join("README.md");
    let text = "# Door\n\n  - a list item\n      indented more\n\n```plantuml\nstate A {\n[*] --> B\n}\n```\n\n```mermaid\nsequenceDiagram\nparticipant Guest\nGuest->>Door: knock\n```\n";
    fs::write(&doc, text).unwrap();
    let path = doc.to_str().unwrap();

    // Only the diagrams are formatted; the list keeps its indentation.
    let out = omaspec(&["fmt", "--write", path]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&doc).unwrap(),
        text.replace("\n[*] --> B\n", "\n  [*] --> B\n")
    );
    let out = omaspec(&["fmt", "--check", path]);
    assert_eq!(out.status.code(), Some(0), "formatted already");

    // The outline of each diagram, on the document's lines.
    let out = omaspec(&["outline", path]);
    assert_eq!(
        stdout(&out),
        "    7  state A\n    8    state B\n   14  participant Guest\n"
    );

    // A command for one diagram file says why not.
    for command in [&["convert", path][..], &["diff", path, path][..]] {
        let out = omaspec(command);
        assert_ne!(out.status.code(), Some(0));
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains("README.md is a Markdown document with 2 diagrams"),
            "{command:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn fmt_check_and_write() {
    let dir = scratch("fmt");
    let file = dir.join("flow.puml");
    fs::write(&file, "@startuml\nalt x\nA -> B\nend\n@enduml\n").unwrap();

    let out = omaspec(&["fmt", "--check", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));

    let out = omaspec(&["fmt", "--write", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "@startuml\nalt x\n  A -> B\nend\n@enduml\n"
    );
    // No temporary file left behind.
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);

    let out = omaspec(&["fmt", "--check", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn fmt_portable_writes_a_flowchart_the_way_mermaid_js_reads_it() {
    let dir = scratch("fmt-portable");
    let file = dir.join("README.md");
    let text = "# Flow\n\na | b :::c\n\n```mermaid\nflowchart TD\nsubgraph 🔴 Breaks\nA --> B[Low | High] :::hot\nend\n```\n";
    fs::write(&file, text).unwrap();

    let out = omaspec(&["lint", file.to_str().unwrap()]);
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "a hint is no error: {said}");
    assert!(said.contains(":7: info: Omaspec draws this"), "{said}");
    assert_eq!(said.matches("mermaid.js (GitHub)").count(), 3, "{said}");

    // Formatting alone leaves the words as they are.
    let out = omaspec(&["fmt", file.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("B[Low | High] :::hot"));

    let out = omaspec(&["fmt", "--portable", "--write", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "# Flow\n\na | b :::c\n\n```mermaid\nflowchart TD\nsubgraph \"🔴 Breaks\"\n  A --> B[\"Low | High\"]:::hot\nend\n```\n"
    );
    let out = omaspec(&["lint", file.to_str().unwrap()]);
    assert!(!String::from_utf8_lossy(&out.stdout).contains("mermaid.js"));
}

#[test]
fn lint_fix_makes_the_fixes_findings_carry_and_fmt_pretty_writes_evenly() {
    let dir = scratch("lint-fix");
    let file = dir.join("door.puml");
    fs::write(
        &file,
        "@startuml\n[*]-->Closed\nClosed-->Open : open\n\n' lines by hand (Omaspec)\n' Open under Closed\n",
    )
    .unwrap();
    let path = file.to_str().unwrap();

    let out = omaspec(&["lint", "--json", path]);
    let said = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "an error: {said}");
    assert!(
        said.contains("\"title\": \"Add @enduml at the end\""),
        "{said}"
    );
    assert!(said.contains("\"title\": \"Add Open --> [*]\""), "{said}");
    // The fix's line is counted from 1, as the finding's is.
    assert!(said.contains("\"line\": 4,\n      \"remove\": 0"), "{said}");

    let out = omaspec(&["lint", "--fix", path]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("fixed 3 in"));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "@startuml\n[*]-->Closed\nClosed-->Open : open\nOpen --> [*]\n\n' lines by hand (Omaspec)\n@enduml\n"
    );
    let out = omaspec(&["lint", "--fix"]);
    assert_ne!(out.status.code(), Some(0), "standard input has no file");

    let out = omaspec(&["fmt", "--pretty", "--write", path]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "@startuml\n[*]    --> Closed\nClosed --> Open : open\nOpen   --> [*]\n\n' lines by hand (Omaspec)\n@enduml\n"
    );
}

#[test]
fn extract_reads_png_and_svg_and_never_overwrites_silently() {
    let dir = scratch("extract");
    let code = "@startuml\nBob -> Alice : hi\n@enduml\n";
    let pixel: &[u8] = &[
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6,
        0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ];
    let png = dir.join("diagram.png");
    fs::write(
        &png,
        embed::png_with_source(pixel, code, Language::PlantUml, None).unwrap(),
    )
    .unwrap();
    let svg = dir.join("diagram.svg");
    let markup = embed::svg_with_source(
        r#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#,
        "pie\n\"a\": 1\n",
        Language::Mermaid,
        None,
    );
    fs::write(&svg, markup).unwrap();

    assert_eq!(stdout(&omaspec(&["extract", png.to_str().unwrap()])), code);
    assert_eq!(
        stdout(&omaspec(&["extract", svg.to_str().unwrap()])),
        "pie\n\"a\": 1\n"
    );

    let target = dir.join("out.puml");
    fs::write(&target, "keep me").unwrap();
    let out = omaspec(&[
        "extract",
        png.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&target).unwrap(), "keep me");
    let out = omaspec(&[
        "extract",
        png.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
        "--force",
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&target).unwrap(), code);

    let plain = dir.join("plain.svg");
    fs::write(&plain, r#"<svg xmlns="http://www.w3.org/2000/svg"/>"#).unwrap();
    assert_eq!(
        omaspec(&["extract", plain.to_str().unwrap()]).status.code(),
        Some(1)
    );
}

#[test]
fn inspect_reports_kind_and_outline() {
    let dir = scratch("inspect");
    let file = dir.join("states.mmd");
    fs::write(&file, "stateDiagram-v2\n[*] --> Idle\nIdle --> Busy\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["language"], "mermaid");
    assert_eq!(json["kind"]["kind"], "state");
    assert_eq!(json["outline"][0]["label"], "Idle");
    assert_eq!(json["outline"][0]["line"], 2);
    assert_eq!(json["native"], "understood");

    let file = dir.join("pie.mmd");
    fs::write(&file, "pie\n  \"a\": 1\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");

    let file = dir.join("journey.mmd");
    fs::write(
        &file,
        "journey\n  title First boot\n  section Install\n    Partition disk: 3: User\n",
    )
    .unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");

    let file = dir.join("timeline.mmd");
    fs::write(&file, "timeline\n  title History\n  2020 : Launch\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");

    let file = dir.join("er.mmd");
    fs::write(&file, "erDiagram\n  USER ||--o{ ORDER : places\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");

    let file = dir.join("c4.mmd");
    fs::write(
        &file,
        "C4Context\nPerson(admin, \"Admin\")\nSystem(sys, \"System\")\nRel(admin, sys, \"Uses\")\n",
    )
    .unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");

    let file = dir.join("architecture.mmd");
    fs::write(
        &file,
        "architecture-beta\n  service web(server)[Web]\n  service db(database)[DB]\n  web:R --> L:db\n",
    )
    .unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");

    let file = dir.join("block.mmd");
    fs::write(&file, "block-beta\n  a[\"Web\"] b[\"DB\"]\n  a --> b\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");

    // Mermaid that Omaspec leaves to mmdr names the construct, and a
    // diagram nobody can draw names the line of the render error.
    let file = dir.join("git.mmd");
    fs::write(&file, "gitGraph\n  commit\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");

    let file = dir.join("board.mmd");
    fs::write(&file, "kanban\n  todo[Todo]\n    a[Write]\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");

    let file = dir.join("sideways.mmd");
    fs::write(&file, "xychart-beta horizontal\n  bar [1, 2]\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["native"]["declined"]["reason"],
        "Omaspec does not draw this XY chart; mmdr does"
    );
    assert!(json["native"]["declined"].get("line").is_none());

    let file = dir.join("rect.mmd");
    fs::write(
        &file,
        "sequenceDiagram\nAlice->>Bob: hi\nrect rgb(0, 0, 0)\nend\n",
    )
    .unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");

    let file = dir.join("brace.mmd");
    fs::write(&file, "sequenceDiagram\nA->>B: hi @{\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"]["declined"]["line"], 2);
    assert!(
        json["native"]["declined"]["reason"]
            .as_str()
            .unwrap()
            .contains("@{")
    );

    let file = dir.join("open.mmd");
    fs::write(&file, "flowchart TD\n  subgraph S\n  A --> B\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"]["declined"]["line"], 2);

    // PlantUML: whether Omaspec's own engine draws it, or why not.
    let file = dir.join("seq.puml");
    fs::write(&file, "@startuml\nA -> B\n@enduml\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"], "understood");
    fs::write(&file, "@startuml\nskin rose\nA -> B\n@enduml\n").unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"]["declined"]["line"], 2);
    assert!(
        json["native"]["declined"]["reason"]
            .as_str()
            .unwrap()
            .contains("skin")
    );
}

fn omaspec_with(env: (&str, &str), args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .env(env.0, env.1)
        .args(args)
        .output()
        .expect("omaspec runs")
}

/// `render --fold` (REQ-104): the containers named are drawn folded to
/// their box, as the editor folds them; the source the image keeps is the
/// file's. A name the diagram does not hold is said with those it has,
/// and nothing is written; of several diagrams, each name is held by one.
#[test]
fn render_folds_the_containers_named_and_keeps_the_file_s_source() {
    let dir = scratch("fold");
    let coffee = dir.join("coffee.puml");
    let code = "@startuml\n[*] --> Idle\nIdle --> Brewing : start\nstate Brewing {\n  [*] --> Heat\n  Heat --> Pour : hot\n}\nBrewing --> Idle : done\n@enduml\n";
    fs::write(&coffee, code).unwrap();
    let net = dir.join("net.mmd");
    fs::write(
        &net,
        "flowchart LR\n  A --> B\n  subgraph Core [Core network]\n    B[Router] --> C[Switch]\n  end\n  C --> D\n",
    )
    .unwrap();
    let (coffee, net) = (coffee.to_str().unwrap(), net.to_str().unwrap());
    let svg = dir.join("coffee.svg");
    let out = omaspec(&[
        "render",
        coffee,
        "--fold",
        "Brewing",
        "-o",
        svg.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("folded Brewing"));
    let drawn = fs::read_to_string(&svg).unwrap();
    assert!(
        drawn.contains("2 states · 2 transitions"),
        "the box says what it holds"
    );
    assert!(!drawn.contains(">Heat<"), "what it holds is not drawn");
    assert_eq!(embed::source_from_svg(&drawn).unwrap().code, code);

    // A name it does not hold: those it has, and no picture.
    let none = dir.join("none.svg");
    let out = omaspec(&[
        "render",
        coffee,
        "--fold",
        "Pantry",
        "-o",
        none.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("Pantry is no container to fold; the diagram has Brewing"),
        "{out:?}"
    );
    assert!(!none.exists());

    // Of two diagrams, each folds what it holds, and says so.
    let out = omaspec(&["render", coffee, net, "--fold", "Core,Brewing", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report[0]["folded"], serde_json::json!(["Brewing"]));
    assert_eq!(report[1]["folded"], serde_json::json!(["Core"]));
    let out = omaspec(&["render", coffee, net, "--fold", "Pantry"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("Pantry is no container to fold in any of the diagrams")
    );
    // The image keeps the folds: `render --check` draws it so again.
    let beside = omaspec(&["render", coffee, "--fold", "Brewing"]);
    assert_eq!(beside.status.code(), Some(0), "{beside:?}");
    let check = omaspec(&["render", "--check", coffee]);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    // Names trimmed, given twice or in quotes with a comma.
    let sales = dir.join("sales.puml");
    fs::write(
        &sales,
        "@startuml\npackage \"Sales, EU\" {\n  class Order\n}\nclass Shop\nShop --> Order\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&[
        "render",
        sales.to_str().unwrap(),
        "--fold",
        r#" "Sales, EU" "#,
        "-o",
        dir.join("sales.svg").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("folded Sales, EU"),
        "{out:?}"
    );
    let out = omaspec(&[
        "render",
        coffee,
        "--fold",
        " Brewing , Brewing",
        "-o",
        svg.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("folded Brewing)"),
        "{out:?}"
    );
    // Of several, a name none holds writes nothing, lists what they have,
    // and is in the report.
    let before = fs::read(&svg).unwrap();
    let out = omaspec(&["render", coffee, net, "--fold", "Pantry", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let said = report.as_array().unwrap().last().unwrap()["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(said.contains("they have Brewing, Core"), "{said}");
    assert_eq!(fs::read(&svg).unwrap(), before, "nothing written");
    // Beside its diagram, a folded picture keeps its folds with the
    // source; without one it could not be held again.
    let out = omaspec(&["render", coffee, "--fold", "Brewing", "--no-source"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--no-source"));
    // Text and a presentation are the whole behavior.
    for output in ["coffee.txt", "coffee.html"] {
        let out = omaspec(&[
            "render",
            coffee,
            "--fold",
            "Brewing",
            "-o",
            dir.join(output).to_str().unwrap(),
        ]);
        assert_eq!(out.status.code(), Some(2), "{output}");
    }

    // `export --fold` folds in every diagram that holds the name; one no
    // diagram holds stops it before anything is written.
    let project = dir.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("omaspec-project.toml"), "name = \"Cafe\"\n").unwrap();
    fs::write(project.join("coffee.puml"), code).unwrap();
    let folder = project.join("out");
    let out = omaspec(&[
        "export",
        project.to_str().unwrap(),
        "-o",
        folder.to_str().unwrap(),
        "--fold",
        "Brewing",
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let drawn = fs::read_to_string(folder.join("coffee.svg")).unwrap();
    assert!(drawn.contains("2 states · 2 transitions") && !drawn.contains(">Heat<"));
    let never = project.join("never");
    let out = omaspec(&[
        "export",
        project.to_str().unwrap(),
        "-o",
        never.to_str().unwrap(),
        "--fold",
        "Pantry",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!never.exists());
    // The map is drawn whole.
    let toml = project.join("omaspec-project.toml");
    let out = omaspec(&[
        "render",
        toml.to_str().unwrap(),
        "--fold",
        "Brewing",
        "-o",
        dir.join("map.svg").to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("a project's map is drawn whole"));
    // A diagram that does not draw is said, not a name it may hold.
    fs::write(
        project.join("broken.puml"),
        "@startuml\nstate Brewing {\n  [*] -->\n}\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&[
        "export",
        project.to_str().unwrap(),
        "-o",
        never.to_str().unwrap(),
        "--fold",
        "Pantry",
    ]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(
        said.contains("broken.puml") && !said.contains("Pantry"),
        "{said}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn render_takes_a_look_and_keeps_the_file_s_source() {
    let dir = scratch("look");
    let file = dir.join("door.puml");
    // A line the engine does not take: its number is the file's.
    let code = "@startuml\n[*] --> Closed\nClosed --> Open : open\n@enduml\n";
    fs::write(&file, code).unwrap();
    let path = file.to_str().unwrap();
    let out = |name: &str| dir.join(name).to_str().unwrap().to_owned();

    let dark = out("dark.svg");
    assert_eq!(
        omaspec(&["render", path, "--look", "dark", "-o", &dark])
            .status
            .code(),
        Some(0)
    );
    let svg = fs::read_to_string(&dark).unwrap();
    assert!(
        svg.to_ascii_lowercase().contains("#16181d"),
        "the dark paper"
    );
    assert_eq!(embed::source_from_svg(&svg).unwrap().code, code);

    let auto = out("auto.svg");
    assert_eq!(
        omaspec(&["render", path, "--look", "auto", "-o", &auto])
            .status
            .code(),
        Some(0)
    );
    let svg = fs::read_to_string(&auto).unwrap();
    assert!(svg.contains("@media (prefers-color-scheme: dark)"));
    assert_eq!(embed::source_from_svg(&svg).unwrap().code, code);

    // auto is an SVG's: a PNG is light or dark.
    let png = omaspec(&["render", path, "--look", "auto", "-o", &out("auto.png")]);
    assert_eq!(png.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&png.stderr).contains("take light or dark"));

    // An error's line is the file's, not the look's.
    let broken = dir.join("broken.puml");
    fs::write(&broken, "@startuml\n[*] --> Closed\nClosed -> \n@enduml\n").unwrap();
    let native = ("OMASPEC_PLANTUML", "none");
    let broken = broken.to_str().unwrap();
    let dark = omaspec_with(native, &["render", broken, "--look", "dark", "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&dark.stdout).unwrap();
    assert_eq!(report[0]["line"], 3, "{report}");
}

#[test]
fn an_exported_map_and_a_linked_state_link_to_the_diagrams_they_open() {
    let dir = scratch("export-links");
    fs::create_dir_all(dir.join("orders")).unwrap();
    let write = |file: &str, text: &str| fs::write(dir.join(file), text).unwrap();
    write("omaspec-project.toml", "name = \"shop\"\n");
    write(
        "orders/orders.puml",
        "@startuml\ntitle Orders\n[*] --> New\nNew --> Payment : pay\n@enduml\n",
    );
    write(
        "payment.puml",
        "@startuml\ntitle Payment\n[*] --> Waiting\n@enduml\n",
    );
    let path = |file: &str| dir.join(file).to_str().unwrap().to_owned();
    let out = omaspec(&[
        "render",
        &path("omaspec-project.toml"),
        &path("orders/orders.puml"),
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let map = fs::read_to_string(dir.join("omaspec-project.svg")).unwrap();
    assert!(
        map.contains(r#"<a href="orders/orders.svg"><title>Orders</title>"#),
        "{map}"
    );
    let orders = fs::read_to_string(dir.join("orders/orders.svg")).unwrap();
    assert!(
        orders.contains(r#"<a href="../payment.svg"><title>Payment</title>"#),
        "{orders}"
    );
    // A picture has no links to follow.
    let png = path("orders/orders.png");
    assert_eq!(
        omaspec(&["render", &path("orders/orders.puml"), "-o", &png])
            .status
            .code(),
        Some(0)
    );
}

#[test]
fn a_play_is_written_as_a_gif_that_moves() {
    let dir = scratch("play-gif");
    let door = dir.join("door.puml");
    fs::write(
        &door,
        "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Closed : knock twice [at night]\n@enduml\n",
    )
    .unwrap();
    let gif = dir.join("door.gif");
    let out = omaspec(&[
        "play",
        door.to_str().unwrap(),
        "-o",
        gif.to_str().unwrap(),
        "--events",
        "open,close,knock twice",
        "--theme",
        "built-in",
        "--width",
        "400",
        "--fps",
        "10",
    ]);
    let err = String::from_utf8_lossy(&out.stderr);
    if err.contains("no GPU") {
        eprintln!("skipped: {err}");
        return;
    }
    assert_eq!(out.status.code(), Some(0), "{err}");
    assert!(
        err.contains("open, close, knock twice [at night]") && err.contains("frames"),
        "{err}"
    );
    let bytes = fs::read(&gif).unwrap();
    assert!(bytes.starts_with(b"GIF89a"), "a GIF");
    // An event the machine does not offer is named, with what it does.
    let out = omaspec(&[
        "play",
        door.to_str().unwrap(),
        "-o",
        gif.to_str().unwrap(),
        "--events",
        "fly",
    ]);
    assert_ne!(out.status.code(), Some(0));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("fly is not offered") && err.contains("open"),
        "{err}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `omaspec coverage --river` draws the picture as a trace river: the
/// ways as wide as taken, where a trace left the machine said in red.
#[test]
fn coverage_draws_a_trace_river() {
    let dir = scratch("river");
    let door = dir.join("door.puml");
    fs::write(
        &door,
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\n@enduml\n",
    )
    .unwrap();
    let trace = dir.join("night.jsonl");
    fs::write(
        &trace,
        "{\"object\":\"Door\",\"event\":\"open\"}\n{\"object\":\"Door\",\"event\":\"lock\"}\n",
    )
    .unwrap();
    let svg = dir.join("river.svg");
    let out = omaspec(&[
        "coverage",
        door.to_str().unwrap(),
        "--traces",
        trace.to_str().unwrap(),
        "--river",
        "-o",
        svg.to_str().unwrap(),
    ]);
    let picture = fs::read_to_string(&svg).unwrap_or_default();
    assert!(
        picture.contains("trace left the machine here"),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // Without a picture there is no river to draw.
    let out = omaspec(&[
        "coverage",
        door.to_str().unwrap(),
        "--traces",
        trace.to_str().unwrap(),
        "--river",
    ]);
    assert!(!out.status.success());
    let _ = fs::remove_dir_all(&dir);
}

/// `omaspec orbit` draws a state machine's life cycle on a ring, the SVG
/// on standard output or a picture; a scenario has none.
#[test]
fn orbit_draws_a_life_cycle_on_a_ring() {
    let dir = scratch("orbit");
    let door = dir.join("door.puml");
    fs::write(
        &door,
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&["orbit", door.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let svg = String::from_utf8_lossy(&out.stdout);
    assert!(svg.starts_with("<svg") && svg.contains("Closed"), "{svg}");
    let png = dir.join("door.png");
    let out = omaspec(&["orbit", door.to_str().unwrap(), "-o", png.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(fs::read(&png).unwrap().starts_with(b"\x89PNG"));
    let scenario = dir.join("keys.puml");
    fs::write(&scenario, "@startuml\nA -> B : hi\n@enduml\n").unwrap();
    let out = omaspec(&["orbit", scenario.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no state machine"));
    let _ = fs::remove_dir_all(&dir);
}

/// A project's file is linted as what it is: nothing to say of a good
/// one, its line where it is wrong; no word of a diagram.
#[test]
fn lint_reads_a_project_s_file_as_one() {
    let dir = scratch("lint-project-file");
    let file = dir.join("omaspec-project.toml");
    fs::write(&file, "# A project.\nname = \"cafe\"\n").unwrap();
    let out = omaspec(&["lint", "--strict", file.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        out.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    fs::write(&file, "name = \"cafe\"\n[look]\npack = \"nowhere\"\n").unwrap();
    let out = omaspec(&["lint", file.to_str().unwrap()]);
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "{said}");
    assert!(said.contains("omaspec-project.toml:3: error:"), "{said}");
    let _ = fs::remove_dir_all(&dir);
}

/// `--themes` alone takes every bundled theme and leaves the file to the
/// file; names follow an equals sign.
#[test]
fn themes_alone_leave_the_file_its_place() {
    let dir = scratch("themes-file");
    let door = dir.join("door.puml");
    fs::write(&door, "@startuml\n[*] --> Closed\n@enduml\n").unwrap();
    let out = dir.join("a.html");
    let before = omaspec(&[
        "play",
        "--themes",
        door.to_str().unwrap(),
        "-o",
        out.to_str().unwrap(),
    ]);
    let said = String::from_utf8_lossy(&before.stderr);
    assert!(!said.contains("required arguments"), "{said}");
    let named = omaspec(&[
        "play",
        door.to_str().unwrap(),
        "--themes=built-in,built-in",
        "-o",
        out.to_str().unwrap(),
    ]);
    let said = String::from_utf8_lossy(&named.stderr);
    assert_eq!(named.status.code(), Some(0), "{said}");
    assert!(said.contains("2 themes"), "{said}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_fold_and_a_zoom_into_a_map_are_written_as_gifs() {
    let dir = scratch("fold-dive-gif");
    fs::write(dir.join("omaspec-project.toml"), "name = \"shop\"\n").unwrap();
    let orders = dir.join("orders.puml");
    fs::write(
        &orders,
        "@startuml\ntitle Orders\n[*] --> New\nstate Paying {\n  [*] --> Card\n  Card --> Paid : ok\n}\nNew --> Paying : pay\n@enduml\n",
    )
    .unwrap();
    let gif = dir.join("moves.gif");
    let play = |args: &[&str]| {
        let mut all = vec!["play"];
        all.extend_from_slice(args);
        all.extend_from_slice(&["-o", gif.to_str().unwrap(), "--theme", "built-in"]);
        all.extend_from_slice(&["--width", "400", "--fps", "10"]);
        let out = omaspec(&all);
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let (code, err) = play(&[orders.to_str().unwrap(), "--fold", "Paying"]);
    if err.contains("no GPU") {
        eprintln!("skipped: {err}");
        return;
    }
    assert_eq!(code, Some(0), "{err}");
    assert!(err.contains("Paying folded and unfolded"), "{err}");
    assert!(fs::read(&gif).unwrap().starts_with(b"GIF89a"));
    // What is no container is named, with those the diagram has.
    let (code, err) = play(&[orders.to_str().unwrap(), "--fold", "New"]);
    assert_ne!(code, Some(0));
    assert!(
        err.contains("New is no container") && err.contains("Paying"),
        "{err}"
    );

    let project = dir.join("omaspec-project.toml");
    let (code, err) = play(&[project.to_str().unwrap(), "--dive", "Orders"]);
    assert_eq!(code, Some(0), "{err}");
    assert!(err.contains("into Orders and back"), "{err}");
    assert!(fs::read(&gif).unwrap().starts_with(b"GIF89a"));
    let (code, err) = play(&[project.to_str().unwrap(), "--dive", "Nowhere"]);
    assert_ne!(code, Some(0));
    assert!(
        err.contains("Nowhere is no tile") && err.contains("Orders"),
        "{err}"
    );
    // A flight: into the tile, on along a state that links to a diagram
    // of its name, and back out the same way.
    let paying = dir.join("paying.puml");
    fs::write(
        &paying,
        "@startuml\ntitle Paying\n[*] --> Card\nCard --> [*] : ok\n@enduml\n",
    )
    .unwrap();
    let (code, err) = play(&[project.to_str().unwrap(), "--dive", "Orders/Paying"]);
    assert_eq!(code, Some(0), "{err}");
    assert!(err.contains("into Orders/Paying and back"), "{err}");
    let (code, err) = play(&[project.to_str().unwrap(), "--dive", "Orders/Nowhere"]);
    assert_ne!(code, Some(0));
    assert!(
        err.contains("Nowhere is no link of Orders") && err.contains("Paying"),
        "{err}"
    );
    // The state inside the one named does not take the click meant for
    // it, and a name with a `/` of its own is one name.
    let card = dir.join("card.puml");
    fs::write(&card, "@startuml\ntitle Card\n[*] --> Swipe\n@enduml\n").unwrap();
    let slash = dir.join("slash.puml");
    fs::write(&slash, "@startuml\ntitle Log/Out\n[*] --> Gone\n@enduml\n").unwrap();
    let (code, err) = play(&[
        project.to_str().unwrap(),
        "--dive",
        "Orders/Paying/Card,Log/Out",
    ]);
    assert_eq!(code, Some(0), "{err}");
    let (code, err) = play(&[project.to_str().unwrap(), "--dive", "Orders/Paying/Card/X"]);
    assert_ne!(code, Some(0));
    assert!(err.contains("X is no link of Card; it has none"), "{err}");
    fs::remove_file(&card).unwrap();
    fs::remove_file(&slash).unwrap();
    // A diagram that leads back to one before it: the way back is the way
    // in, so the next flight starts on the map again.
    fs::write(
        &paying,
        "@startuml\ntitle Paying\n[*] --> Card\nCard --> Orders : again\n@enduml\n",
    )
    .unwrap();
    let (code, err) = play(&[
        project.to_str().unwrap(),
        "--dive",
        "Orders/Paying/Orders,Orders",
    ]);
    assert_eq!(code, Some(0), "{err}");
    fs::remove_file(&paying).unwrap();
    // A diagram on the way that includes another file of its folder.
    fs::write(dir.join("card.iuml"), "Card --> [*] : ok\n").unwrap();
    fs::write(
        &paying,
        "@startuml\ntitle Paying\n[*] --> Card\n!include card.iuml\n@enduml\n",
    )
    .unwrap();
    let (code, err) = play(&[project.to_str().unwrap(), "--dive", "Orders/Paying"]);
    assert_eq!(code, Some(0), "{err}");
    fs::remove_file(&paying).unwrap();
    fs::remove_file(dir.join("card.iuml")).unwrap();
    // A folder's tile: its map, and on to a diagram there. A project
    // shows its folders as tiles once it is too large for one map.
    fs::create_dir_all(dir.join("more")).unwrap();
    fs::write(
        dir.join("more/refund.puml"),
        "@startuml\ntitle Refund\n[*] --> Asked\n@enduml\n",
    )
    .unwrap();
    for n in 0..36 {
        fs::write(
            dir.join(format!("more/d{n}.puml")),
            format!("@startuml\ntitle D{n}\n[*] --> S{n}\n@enduml\n"),
        )
        .unwrap();
    }
    let (code, err) = play(&[project.to_str().unwrap(), "--dive", "more/Refund,Orders"]);
    assert_eq!(code, Some(0), "{err}");
    fs::remove_dir_all(dir.join("more")).unwrap();
    // A dive wants the project's file, and the project's file a dive.
    let (code, err) = play(&[orders.to_str().unwrap(), "--dive", "Orders"]);
    assert_ne!(code, Some(0));
    assert!(err.contains("omaspec-project.toml"), "{err}");
    let (code, err) = play(&[project.to_str().unwrap()]);
    assert_ne!(code, Some(0));
    assert!(err.contains("--dive TILE"), "{err}");
    // The project is as it was: the command writes its GIF and nothing else.
    let mut left: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["moves.gif", "omaspec-project.toml", "orders.puml"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_project_s_file_is_drawn_as_the_map_made_from_its_diagrams() {
    let dir = scratch("made-map");
    fs::create_dir_all(dir.join("scenarios")).unwrap();
    let write = |file: &str, text: &str| fs::write(dir.join(file), text).unwrap();
    write("omaspec-project.toml", "name = \"shop\"\n");
    write(
        "orders.puml",
        "@startuml\ntitle Orders\n[*] --> New\nNew --> Paid : pay\n@enduml\n",
    );
    // A plan beside it: a diagram like any other, not the map.
    write("plan.puml", "@startwbs\n* Plan\n** Orders\n@endwbs\n");
    write(
        "scenarios/buy.puml",
        "@startuml\ntitle Buy\nparticipant User\nparticipant Orders\nUser -> Orders : pay\n@enduml\n",
    );
    let path = |file: &str| dir.join(file).to_str().unwrap().to_owned();
    let out = omaspec(&["render", &path("omaspec-project.toml")]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let map = fs::read_to_string(dir.join("omaspec-project.svg")).unwrap();
    // Every diagram of the folder, each a link to its own picture, also
    // the one a folder down; nobody listed them.
    for (file, name) in [
        ("orders.svg", "Orders"),
        ("plan.svg", "plan"),
        ("scenarios/buy.svg", "Buy"),
    ] {
        assert!(
            map.contains(&format!(r#"<a href="{file}"><title>{name}</title>"#)),
            "{file}: {map}"
        );
    }
    assert!(map.contains(">shop<") && map.contains("System map · 3 diagrams"));
    assert!(map.contains(">plays<") && !map.contains("not on the map"));
    // The map is made, so it carries no source to open it from.
    assert!(!map.contains("urn:omaspec:source"), "{map}");
    // As a picture, and in a look.
    let png = path("map.png");
    let out = omaspec(&[
        "render",
        &path("omaspec-project.toml"),
        "--look",
        "dark",
        "-o",
        &png,
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(fs::metadata(&png).unwrap().len() > 1000);
}

#[test]
fn a_creole_link_is_kept_and_a_script_is_not() {
    let dir = scratch("creole-links");
    let src = dir.join("seq.puml");
    fs::write(
        &src,
        "@startuml\nAlice -> Bob : see [[https://example.org{The site} the docs]]\n\
         note right : [[javascript:alert(1) bad]] and [[guide.html local]]\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&["render", src.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let svg = fs::read_to_string(dir.join("seq.svg")).unwrap();
    assert!(
        svg.contains(r#"<a href="https://example.org"><title>The site</title><tspan"#),
        "{svg}"
    );
    assert!(svg.contains(r#"<a href="guide.html">"#), "{svg}");
    assert!(!svg.contains(r#"href="javascript"#), "{svg}");
}

#[test]
fn a_mermaid_click_is_a_link_and_a_callback_is_not() {
    let dir = scratch("mermaid-clicks");
    let write = |file: &str, text: &str| fs::write(dir.join(file), text).unwrap();
    write(
        "flow.mmd",
        "flowchart LR\n  A[Start] --> B[Docs]\n  B --> C[Script]\n  \
         click A \"https://example.org/start\" \"Begin here\"\n  \
         click B href \"docs/guide.html\" _blank\n  click C callback\n",
    );
    write(
        "classes.mmd",
        "classDiagram\n  Animal <|-- Duck\n  link Duck \"https://example.org/duck\"\n  \
         click Animal call alert()\n",
    );
    write(
        "plan.mmd",
        "gantt\n  dateFormat YYYY-MM-DD\n  section A\n  One :a1, 2026-01-01, 3d\n  \
         Two :a2, after a1, 2d\n  click a1,a2 href \"https://example.org/plan\"\n",
    );
    let files: Vec<String> = ["flow.mmd", "classes.mmd", "plan.mmd"]
        .iter()
        .map(|f| dir.join(f).to_str().unwrap().to_owned())
        .collect();
    let args: Vec<&str> = std::iter::once("render")
        .chain(files.iter().map(String::as_str))
        .collect();
    let out = omaspec(&args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let read = |file: &str| fs::read_to_string(dir.join(file)).unwrap();
    let flow = read("flow.svg");
    assert!(
        flow.contains(r#"<a href="https://example.org/start"><title>Begin here</title>"#),
        "{flow}"
    );
    assert!(flow.contains(r#"<a href="docs/guide.html">"#), "{flow}");
    assert!(!flow.contains(r#"href="callback""#), "{flow}");
    let classes = read("classes.svg");
    assert!(
        classes.contains(r#"<a href="https://example.org/duck">"#),
        "{classes}"
    );
    assert!(!classes.contains(r#"href="alert"#), "{classes}");
    // Each of the two tasks: its bar and its name.
    assert_eq!(
        read("plan.svg")
            .matches(r#"<a href="https://example.org/plan">"#)
            .count(),
        4
    );
}

#[test]
fn a_project_exports_as_svgs_linked_to_one_another_and_a_zip() {
    let dir = scratch("export-project");
    fs::create_dir_all(dir.join("orders")).unwrap();
    let write = |file: &str, text: &str| fs::write(dir.join(file), text).unwrap();
    write("omaspec-project.toml", "name = \"shop\"\n");
    write(
        "orders/orders.puml",
        "@startuml\ntitle Orders\n[*] --> New\nNew --> Payment : pay\n@enduml\n",
    );
    write(
        "payment.mmd",
        "stateDiagram-v2\n  title: Payment\n  [*] --> Waiting\n",
    );
    let root = dir.to_str().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args(["export", root, "--look", "auto"])
        .env("OMASPEC_PLANTUML", "none")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let export = dir.join("export");
    let read = |file: &str| fs::read_to_string(export.join(file)).unwrap();
    // The project's map, made from its diagrams, is the page to start at.
    let svg = read("index.svg");
    for tile in [
        r#"<a href="orders/orders.svg"><title>Orders</title>"#,
        r#"<a href="payment.svg"><title>payment</title>"#,
    ] {
        assert!(svg.contains(tile), "{tile}: {svg}");
    }
    assert!(svg.contains("prefers-color-scheme: dark"));
    assert!(!export.join("omaspec-project.svg").exists());
    assert!(
        read("orders/orders.svg").contains(r#"<a href="../payment.svg"><title>Payment</title>"#)
    );
    // Each picture opens as its diagram again.
    let stored = omaspec_core::embed::source_from_svg(&read("payment.svg")).unwrap();
    assert!(stored.code.starts_with("stateDiagram-v2"));
    // The ZIP unpacks into a folder named after the project.
    let zip = fs::read(export.join("shop.zip")).unwrap();
    assert!(zip.starts_with(b"PK"));
    for entry in [
        "shop/index.svg",
        "shop/orders/orders.svg",
        "shop/payment.svg",
    ] {
        assert!(
            zip.windows(entry.len()).any(|w| w == entry.as_bytes()),
            "{entry}"
        );
    }
    // Nothing beside the folder.
    let mut top: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    top.sort();
    assert_eq!(
        top,
        ["export", "omaspec-project.toml", "orders", "payment.mmd"]
    );

    // A diagram that does not draw is reported; the rest is written.
    write(
        "broken.puml",
        "@startuml\n\"Long name\" --> X\n[*] --> X\n@enduml\n",
    );
    let out = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args([
            "export",
            root,
            "-o",
            dir.join("out").to_str().unwrap(),
            "--json",
        ])
        .env("OMASPEC_PLANTUML", "none")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["failed"][0]["file"], "broken.puml", "{report}");
    assert_eq!(report["failed"][0]["line"], 2, "{report}");
    assert_eq!(report["written"].as_array().unwrap().len(), 2, "{report}");
    // The map shows the diagram that does not draw as a tile that says so.
    assert_eq!(report["index"], true);
    assert!(dir.join("out/index.svg").is_file());
    assert!(dir.join("out/orders/orders.svg").is_file());

    // Two diagrams that would be one SVG stop it before anything is written.
    write("orders/orders.mmd", "flowchart LR\n  A --> B\n");
    let out = omaspec(&["export", root, "-o", dir.join("twice").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("would both be orders/orders.svg"), "{err}");
    assert!(!dir.join("twice").exists());
}

#[test]
fn a_markdown_document_is_linted_inspected_and_drawn_block_by_block() {
    let dir = scratch("markdown");
    let doc = dir.join("README.md");
    fs::write(
        &doc,
        "# Door\n\n```plantuml\n@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\n@enduml\n```\n\n\
         ```mermaid\nsequenceDiagram\n  User->>Door: kick\n```\n\n```rust\nfn main() {}\n```\n\n\
         ```puml\nAlice -> Bob : hi\n```\n",
    )
    .unwrap();
    let path = doc.to_str().unwrap();
    // The scenario against the machine beside it, on the document's line.
    let out = omaspec(&["lint", path]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains(&format!(
            "{path}:13: warning: Door has no transition for kick"
        )),
        "{text}"
    );
    let out = omaspec(&["inspect", path]);
    let inspected: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let lines: Vec<&serde_json::Value> = inspected["diagrams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| &d["line"])
        .collect();
    assert_eq!(lines, [3, 11, 20], "{inspected}");
    assert_eq!(inspected["diagrams"][0]["outline"][0]["line"], 5);
    // One image per diagram, numbered, each with its own source.
    let out = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args(["render", path])
        .env("OMASPEC_PLANTUML", "none")
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for (n, start) in [
        (1, "@startuml\ntitle Door"),
        (2, "sequenceDiagram"),
        (3, "@startuml\nAlice"),
    ] {
        let svg = fs::read_to_string(dir.join(format!("README-{n}.svg"))).unwrap();
        let stored = embed::source_from_svg(&svg).unwrap();
        assert!(stored.code.starts_with(start), "{n}: {}", stored.code);
    }
    assert!(!dir.join("README-4.svg").exists());
    let out = omaspec(&["render", path, "-o", dir.join("one.svg").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    // A document without a diagram is said to be one.
    let empty = dir.join("notes.md");
    fs::write(&empty, "# Notes\n").unwrap();
    let out = omaspec(&["render", empty.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no PlantUML or Mermaid diagram"));
}

#[test]
fn a_project_s_look_draws_its_diagrams_unless_another_is_asked_for() {
    let dir = scratch("pinned-look");
    fs::write(
        dir.join("omaspec-project.toml"),
        "name = \"pinned\"\n\n[look]\npack = \"dark\"\naccent = \"#d73a49\"\nfont = \"Inter\"\n",
    )
    .unwrap();
    let door = dir.join("door.puml");
    let text = "@startuml\n[*] --> Closed\nClosed --> Open : open\n@enduml\n";
    fs::write(&door, text).unwrap();
    let render = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_omaspec"))
            .arg("render")
            .args(args)
            .env("OMASPEC_PLANTUML", "none")
            .output()
            .unwrap();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let door_path = door.to_str().unwrap();
    assert_eq!(render(&[door_path]).0, Some(0));
    let svg = fs::read_to_string(dir.join("door.svg"))
        .unwrap()
        .to_ascii_lowercase();
    assert!(svg.contains("#16181d") && svg.contains("#d73a49"), "{svg}");
    assert!(svg.contains(r#"font-family="inter, arial, "#), "{svg}");
    // The file and the source stored in the picture stay as written.
    assert_eq!(fs::read_to_string(&door).unwrap(), text);
    assert_eq!(
        embed::source_from_svg(&svg)
            .unwrap()
            .code
            .to_ascii_lowercase(),
        text.to_ascii_lowercase()
    );
    // Another look, or the file's own colors, when asked for.
    let own = dir.join("own.svg");
    assert_eq!(
        render(&[door_path, "--look", "file", "-o", own.to_str().unwrap()]).0,
        Some(0)
    );
    assert!(
        !fs::read_to_string(&own)
            .unwrap()
            .to_ascii_lowercase()
            .contains("#16181d")
    );
    // A look that is not the same on every machine is refused, by its line.
    fs::write(
        dir.join("omaspec-project.toml"),
        "[look]\npack = \"omarchy\"\n",
    )
    .unwrap();
    let (code, err) = render(&[door_path]);
    assert_eq!(code, Some(2));
    assert!(err.contains("omaspec-project.toml:2:"), "{err}");
}

#[test]
fn render_check_holds_a_project_against_its_committed_svgs() {
    let dir = scratch("render-check");
    fs::create_dir_all(dir.join("states")).unwrap();
    fs::write(
        dir.join("omaspec-project.toml"),
        "name = \"ci\"\n[look]\npack = \"light\"\n",
    )
    .unwrap();
    let door = dir.join("states/door.puml");
    fs::write(
        &door,
        "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\n@enduml\n",
    )
    .unwrap();
    fs::write(dir.join("flow.mmd"), "flowchart LR\n  A --> B\n").unwrap();
    let root = dir.to_str().unwrap();
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_omaspec"))
            .arg("render")
            .args(args)
            .env("OMASPEC_PLANTUML", "none")
            .output()
            .unwrap();
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    // A folder: every diagram in it, each beside its own.
    assert_eq!(run(&[root]).0, Some(0));
    assert!(dir.join("states/door.svg").is_file() && dir.join("flow.svg").is_file());
    // And the project's map, beside the project's file.
    assert!(dir.join("omaspec-project.svg").is_file());
    // As text, a project is its diagrams: its file is no diagram to read.
    let (_, out, err) = run(&["--format", "text", root]);
    assert!(err.contains("door.txt"), "{err}");
    assert!(
        !out.contains("omaspec-project") && !err.contains("omaspec-project"),
        "{out}\n{err}"
    );
    // A project below a folder that is none: its map is drawn with its
    // diagrams.
    let around = scratch("render-around");
    fs::create_dir_all(around.join("nested")).unwrap();
    fs::write(
        around.join("nested/omaspec-project.toml"),
        "name = \"nested\"\n",
    )
    .unwrap();
    fs::write(
        around.join("nested/lamp.puml"),
        "@startuml\n[*] --> Off\n@enduml\n",
    )
    .unwrap();
    assert_eq!(run(&[around.to_str().unwrap()]).0, Some(0));
    assert!(around.join("nested/omaspec-project.svg").is_file());
    let _ = fs::remove_dir_all(&around);
    let (code, out, err) = run(&["--check", root]);
    assert_eq!((code, out.as_str()), (Some(0), ""), "{err}");
    assert!(err.contains("3 SVGs drawn as they are"), "{err}");
    // A diagram changed without its SVG.
    fs::write(&door, "@startuml\ntitle Door\n[*] --> Closed\n@enduml\n").unwrap();
    let (code, out, _) = run(&["--check", root]);
    assert_eq!(code, Some(1));
    assert!(
        out.contains("door.svg: the diagram changed since this SVG was drawn"),
        "{out}"
    );
    // The same diagrams in another look: drawn differently.
    assert_eq!(run(&[root]).0, Some(0));
    fs::write(
        dir.join("omaspec-project.toml"),
        "[look]\npack = \"dark\"\n",
    )
    .unwrap();
    let (code, out, _) = run(&["--check", root]);
    assert_eq!(code, Some(1));
    assert!(
        out.contains("flow.svg: drawn differently:") && out.contains("in other colors"),
        "{out}"
    );
    // Nothing was written by the check.
    assert!(
        !fs::read_to_string(dir.join("flow.svg"))
            .unwrap()
            .to_ascii_lowercase()
            .contains("#16181d")
    );
    // A missing SVG, and what --check refuses.
    fs::remove_file(dir.join("flow.svg")).unwrap();
    let (code, out, _) = run(&["--check", dir.join("flow.mmd").to_str().unwrap()]);
    assert_eq!(code, Some(1));
    assert!(out.contains("flow.svg: not there"), "{out}");
    assert_eq!(run(&["--check", "-f", "png", root]).0, Some(2));
}

#[test]
fn render_writes_svg_png_and_pdf_next_to_the_diagram() {
    let dir = scratch("render");
    let file = dir.join("states.mmd");
    let code = "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : job\n  Busy --> Idle\n";
    fs::write(&file, code).unwrap();
    let path = file.to_str().unwrap();

    assert_eq!(omaspec(&["render", path]).status.code(), Some(0));
    let svg = fs::read_to_string(dir.join("states.svg")).unwrap();
    assert!(svg.starts_with("<svg"));
    assert_eq!(embed::source_from_svg(&svg).unwrap().code, code);

    let png = dir.join("out.png");
    assert_eq!(
        omaspec(&["render", path, "-o", png.to_str().unwrap()])
            .status
            .code(),
        Some(0)
    );
    let back = embed::source_from_png(&fs::read(&png).unwrap()).unwrap();
    assert_eq!(back.code, code);
    assert_eq!(back.file.as_deref(), Some("states.mmd"));

    assert_eq!(
        omaspec(&["render", path, "-f", "pdf"]).status.code(),
        Some(0)
    );
    assert!(
        fs::read(dir.join("states.pdf"))
            .unwrap()
            .starts_with(b"%PDF-")
    );

    assert_eq!(
        omaspec(&["render", path, "-f", "jpeg"]).status.code(),
        Some(0)
    );
    assert!(
        fs::read(dir.join("states.jpg"))
            .unwrap()
            .starts_with(&[0xFF, 0xD8])
    );

    assert_eq!(
        omaspec(&["render", path, "-f", "markdown"]).status.code(),
        Some(0)
    );
    let md = fs::read_to_string(dir.join("states.md")).unwrap();
    assert!(md.contains("```mermaid\n"));
    assert!(md.contains(code));

    assert_eq!(
        omaspec(&["render", path, "-f", "zip"]).status.code(),
        Some(0)
    );
    assert!(fs::read(dir.join("states.zip")).unwrap().starts_with(b"PK"));

    let out = omaspec(&["render", path, "-o", "-", "--no-source"]);
    assert!(!stdout(&out).contains("omaspec:source"));
}

#[test]
fn render_reads_includes_inside_the_diagrams_folder_only() {
    let dir = scratch("include");
    let project = dir.join("project");
    fs::create_dir_all(project.join("lib")).unwrap();
    fs::write(
        project.join("lib/parts.puml"),
        "@startuml\n!procedure $ask($who)\n  $who -> Api : ask\n!endprocedure\n@enduml\n",
    )
    .unwrap();
    fs::write(
        project.join("main.puml"),
        "@startuml\n!include lib/parts.puml\n$ask(\"User\")\n@enduml\n",
    )
    .unwrap();
    fs::write(dir.join("secret.puml"), "Alice -> Bob : secret\n").unwrap();
    fs::write(
        project.join("escape.puml"),
        "@startuml\n!include ../secret.puml\n@enduml\n",
    )
    .unwrap();

    let main = project.join("main.puml");
    let out = omaspec_with(
        ("OMASPEC_PLANTUML", "none"),
        &["render", main.to_str().unwrap(), "-o", "-"],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(stdout(&out).contains(">ask<"));

    let escape = project.join("escape.puml");
    let out = omaspec_with(
        ("OMASPEC_PLANTUML", "none"),
        &["render", "--json", escape.to_str().unwrap()],
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report[0]["line"], 2);
    let message = report[0]["message"].as_str().unwrap();
    assert!(message.contains("outside the project folder"), "{message}");
    assert!(!project.join("escape.svg").exists());
}

#[test]
fn render_reports_diagram_errors_and_protects_the_source() {
    let dir = scratch("render-errors");
    let file = dir.join("broken.mmd");
    fs::write(&file, "flowchart TD\n  subgraph S\n  A --> B\n").unwrap();
    let out = omaspec(&["render", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("broken.mmd:2: error:"));

    let out = omaspec(&["render", "--json", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["ok"], false);
    assert_eq!(json[0]["line"], 2);
    assert_eq!(json[0]["engine"], "mmdr");
    assert!(json[0]["message"].as_str().unwrap().contains("mmdr"));

    let good = dir.join("good.mmd");
    fs::write(&good, "flowchart LR\n  A --> B\n").unwrap();
    let out = omaspec(&["render", "--json", good.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["ok"], true);
    assert_eq!(json[0]["engine"], "Omaspec Mermaid");
    assert_eq!(json[0]["output"], dir.join("good.svg").to_str().unwrap());
    assert!(dir.join("good.svg").is_file());

    let out = omaspec(&["render", "--json", file.to_str().unwrap(), "-o", "-"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--json"));

    let ok = dir.join("ok.mmd");
    fs::write(&ok, "pie\n  \"a\": 1\n").unwrap();
    let out = omaspec(&[
        "render",
        ok.to_str().unwrap(),
        "-o",
        ok.to_str().unwrap(),
        "-f",
        "svg",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&ok).unwrap(), "pie\n  \"a\": 1\n");
}

#[test]
fn render_plantuml_falls_back_to_java_or_says_why_not() {
    let dir = scratch("render-plantuml");
    // Sequence diagrams need no Java.
    let sequence = dir.join("seq.puml");
    fs::write(&sequence, "@startuml\nA -> B : hi\n@enduml\n").unwrap();
    let out = omaspec_with(
        ("OMASPEC_PLANTUML", "none"),
        &["render", sequence.to_str().unwrap()],
    );
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("(Omaspec PlantUML,"));
    assert!(
        fs::read_to_string(dir.join("seq.svg"))
            .unwrap()
            .contains(">hi</tspan>")
    );

    let file = dir.join("boxes.puml");
    fs::write(&file, "@startdot\ndigraph { a -> b }\n@enddot\n").unwrap();
    let path = file.to_str().unwrap();

    let out = omaspec_with(("OMASPEC_PLANTUML", "none"), &["render", path]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("`@startdot` (Graphviz) is not drawn"),
        "{stderr}"
    );
    assert!(
        stderr.contains("install the `plantuml` package"),
        "{stderr}"
    );

    let out = omaspec_with(("OMASPEC_PLANTUML", "none"), &["render", "--json", path]);
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["ok"], false);
    assert!(json[0].get("line").is_none());
    assert!(json[0].get("engine").is_none());
    let message = json[0]["message"].as_str().unwrap();
    assert!(
        message.contains("`@startdot` (Graphviz) is not drawn"),
        "{message}"
    );
    assert!(
        message.contains("install the `plantuml` package"),
        "{message}"
    );

    let fake = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../omaspec-render/tests/fake-plantuml"
    );
    let out = omaspec_with(("OMASPEC_PLANTUML", fake), &["render", path]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("(Java PlantUML,"));
    assert!(
        fs::read_to_string(dir.join("boxes.svg"))
            .unwrap()
            .contains("diagram 1")
    );
}

/// The examples are clean: no lint findings (but what a scenario or a
/// trace wrong on purpose says), and the Mermaid ones and the PlantUML
/// sequence, state, mind map and WBS diagrams draw without Java.
#[test]
fn examples_are_clean() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    // With the Help Omaspec pages one folder down.
    let mut examples = Vec::new();
    let mut folders = vec![dir];
    while let Some(folder) = folders.pop() {
        for path in std::fs::read_dir(&folder)
            .unwrap()
            .flatten()
            .map(|e| e.path())
        {
            if path.is_dir() {
                folders.push(path);
            } else {
                examples.push(path);
            }
        }
    }
    examples.sort();
    assert!(examples.len() >= 8);
    for example in &examples {
        // A recorded trace holds against the machines beside it.
        let command = if example.extension().is_some_and(|e| e == "jsonl") {
            "trace"
        } else {
            "lint"
        };
        // A scenario or a trace wrong on purpose shows what a deviation
        // looks like: the scenario warns, the trace's line is an error,
        // and nothing else is said.
        let on_purpose =
            std::fs::read_to_string(example).is_ok_and(|text| text.contains("Wrong on purpose"));
        if on_purpose {
            let out = omaspec(&[command, example.to_str().unwrap()]);
            let said = String::from_utf8_lossy(&out.stdout);
            let lines: Vec<&str> = said.lines().collect();
            let wanted = if command == "trace" {
                ": error: "
            } else {
                ": warning: "
            };
            assert!(
                !lines.is_empty() && lines.iter().all(|line| line.contains(wanted)),
                "{}: {said}",
                example.display()
            );
            if command == "trace" {
                continue;
            }
            // A warning alone fails no lint.
            assert!(out.status.success(), "{}: {said}", example.display());
        } else {
            let out = omaspec(&[command, "--strict", example.to_str().unwrap()]);
            assert!(
                out.status.success(),
                "{}: {}",
                example.display(),
                String::from_utf8_lossy(&out.stdout)
            );
            if command == "trace" {
                assert!(
                    out.stdout.is_empty(),
                    "{}",
                    String::from_utf8_lossy(&out.stdout)
                );
                continue;
            }
        }
        let text = fs::read_to_string(example).unwrap();
        let native = [" -> ", "@startmindmap", "@startwbs", "[*]"]
            .iter()
            .any(|t| text.contains(t));
        if example.extension().is_some_and(|e| e == "mmd") || native {
            let out = omaspec_with(
                ("OMASPEC_PLANTUML", "none"),
                &["render", example.to_str().unwrap(), "-o", "-"],
            );
            assert!(
                out.status.success(),
                "{}: {}",
                example.display(),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(out.stdout.starts_with(b"<svg"));
        }
    }
}

/// The Java PlantUML runs confined: it cannot write outside its scratch
/// folder, open a TCP connection, read a file outside the system's folders
/// and its own, or read the environment of the process that started it;
/// the scratch folder is gone afterwards. It never sees Omaspec's
/// environment and may hold few files open. With OMASPEC_SANDBOX=off the same stand-in gets
/// everywhere but into the environment.
#[test]
fn the_java_plantuml_runs_confined() {
    let landlock = fs::read_to_string("/sys/kernel/security/lsm")
        .is_ok_and(|lsm| lsm.split(',').any(|m| m.trim() == "landlock"));
    if !landlock {
        eprintln!("skipped: this kernel has no Landlock");
        return;
    }
    let dir = scratch("sandbox");
    let diagram = dir.join("a.puml");
    let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake-plantuml-escape");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let escaped = dir.join("escaped");
    let secret = dir.join("secret");
    fs::write(&secret, "key\n").unwrap();
    // A Graphviz diagram: the engine draws the others itself.
    fs::write(
        &diagram,
        format!(
            "@startdot\n' try {} {port} {}\ndigraph {{ a -> b }}\n@enddot\n",
            escaped.display(),
            secret.display()
        ),
    )
    .unwrap();
    let run = |sandbox: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_omaspec"))
            .env("OMASPEC_PLANTUML", &fake)
            .env("OMASPEC_SANDBOX", sandbox)
            .env("OMASPEC_TEST_SECRET", "token")
            .args(["render", diagram.to_str().unwrap(), "-o", "-"])
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{said}");
        // Where Landlock holds all of it, since Linux 6.12, no fence is
        // said to lack anything; the sandbox's report is not passed on.
        let fence = said.contains("warning: the Java PlantUML ran with");
        let whole = fs::read_to_string("/proc/sys/kernel/osrelease").is_ok_and(|release| {
            let mut parts = release
                .split(['.', '-'])
                .map(|p| p.parse::<u32>().unwrap_or(0));
            (parts.next().unwrap_or(0), parts.next().unwrap_or(0)) >= (6, 12)
        });
        if sandbox == "on" && whole {
            assert!(!fence, "{said}");
        }
        assert!(!said.contains("omaspec-sandbox"), "{said}");
        let svg = String::from_utf8(out.stdout).unwrap();
        let text = &svg[svg.find("<text>").unwrap() + 6..svg.find("</text>").unwrap()];
        text.to_owned()
    };

    let confined = run("on");
    assert!(
        confined.starts_with(
            "write:refused tcp:refused tmp:written read:refused env:clean proc:refused files:"
        ),
        "{confined}"
    );
    assert!(!escaped.exists());
    // Few enough that Java closes them quickly in each Graphviz it starts.
    let files = confined.split("files:").nth(1).unwrap();
    let files: u64 = files.split(' ').next().unwrap().parse().unwrap();
    assert!(files <= 4096, "{confined}");
    let scratch_dir = confined.split("dir:").nth(1).unwrap();
    assert!(scratch_dir.contains("omaspec-plantuml-"));
    assert!(!Path::new(scratch_dir).exists(), "the scratch folder stays");

    let free = run("off");
    assert!(
        free.starts_with("write:written tcp:connected tmp:written read:read env:clean proc:read"),
        "{free}"
    );
    assert!(escaped.exists());
    let _ = fs::remove_dir_all(&dir);
}

/// `omaspec mcp` as an agent's client runs it: a session over standard
/// input and output, one JSON-RPC message per line, and nothing else on
/// standard output.
#[test]
fn mcp_serves_an_agent_over_standard_io() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;

    let dir = scratch("mcp");
    fs::write(
        dir.join("door.mmd"),
        "stateDiagram-v2\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n",
    )
    .unwrap();
    fs::write(
        dir.join("visit.puml"),
        "@startuml\nparticipant Door\nUser -> Door : open\nUser -> Door : open\n@enduml\n",
    )
    .unwrap();
    let mut server = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .arg("mcp")
        .current_dir(&dir)
        .env("OMASPEC_PLANTUML", "none")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = server.stdin.take().unwrap();
    let mut output = BufReader::new(server.stdout.take().unwrap());
    let mut ask = |message: serde_json::Value| -> serde_json::Value {
        writeln!(input, "{message}").unwrap();
        if message.get("id").is_none() {
            return serde_json::Value::Null;
        }
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line:?}"))
    };
    let hello = ask(serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}},
    }));
    assert_eq!(hello["result"]["protocolVersion"], "2025-06-18");
    ask(serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    let tools = ask(serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 11);

    // A relative path, from the folder the server runs in: the second
    // `open` finds the door open already.
    let lint = ask(serde_json::json!({
        "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "lint", "arguments": {"path": "visit.puml"}},
    }));
    let findings = &lint["result"]["structuredContent"]["findings"];
    assert_eq!(findings[0]["file"], "visit.puml", "{lint}");
    assert_eq!(findings[0]["line"], 4, "{lint}");
    assert_eq!(findings[0]["severity"], "warning", "{lint}");
    assert!(
        findings[0]["message"]
            .as_str()
            .unwrap()
            .contains("door.mmd")
    );

    let render = ask(serde_json::json!({
        "jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": {"name": "render", "arguments": {"path": "door.mmd", "output": "door.svg"}},
    }));
    assert_eq!(
        render["result"]["structuredContent"]["output"], "door.svg",
        "{render}"
    );
    let svg = fs::read_to_string(dir.join("door.svg")).unwrap();
    assert!(svg.contains("<svg") && svg.contains("Closed"));
    assert_eq!(
        embed::source_from_svg(&svg).unwrap().code,
        fs::read_to_string(dir.join("door.mmd")).unwrap()
    );

    drop(input);
    assert!(server.wait().unwrap().success());
    let mut rest = String::new();
    std::io::Read::read_to_string(&mut output, &mut rest).unwrap();
    assert!(rest.is_empty(), "more on standard output: {rest:?}");
    let _ = fs::remove_dir_all(&dir);
}

/// `diff` tells what a change did to the behavior, between files, across
/// the languages and against the last commit; `convert` writes the other
/// language and refuses what it cannot say.
#[test]
fn diff_and_convert_go_by_the_behavior() {
    let dir = scratch("diff");
    let old = dir.join("door.puml");
    fs::write(
        &old,
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
    )
    .unwrap();
    let new = dir.join("door.mmd");
    fs::write(
        &new,
        "stateDiagram-v2\n[*] --> Closed\nClosed --> Open : open\nOpen --> Jammed : close\n",
    )
    .unwrap();
    let (old_s, new_s) = (old.to_str().unwrap(), new.to_str().unwrap());

    let out = omaspec(&["diff", old_s, new_s]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(
        text.contains("door.mmd:4: changed: On close, Open now goes to Jammed instead of Closed."),
        "{text}"
    );
    assert!(
        text.contains("found warning: Nothing leaves Jammed"),
        "{text}"
    );
    let out = omaspec(&["diff", old_s, old_s]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
    let out = omaspec(&["diff", "--json", old_s, new_s]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["change"], "added");
    assert_eq!(json[0]["message"], "New state Jammed.");
    assert_eq!(json[1]["change"], "changed");
    assert_eq!(json[1]["old"]["line"], 4);
    let sequence = dir.join("visit.puml");
    fs::write(&sequence, "@startuml\nA -> B : x\n@enduml\n").unwrap();
    let out = omaspec(&["diff", old_s, sequence.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("different kinds"));

    // One file: against its last commit.
    let git = |args: &[&str]| {
        Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(args)
            .output()
            .is_ok_and(|o| o.status.success())
    };
    if git(&["init", "-q"])
        && git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "add",
            "door.puml",
        ])
        && git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "door",
        ])
    {
        let out = omaspec(&["diff", old_s]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        fs::write(
            &old,
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\n@enduml\n",
        )
        .unwrap();
        let out = omaspec(&["diff", old_s]);
        assert_eq!(out.status.code(), Some(1));
        assert!(
            stdout(&out).contains("door.puml:5: added: New transition: Closed to Locked on lock."),
            "{}",
            stdout(&out)
        );
        let out = omaspec(&["diff", "--rev", "--output=x", old_s]);
        assert_eq!(out.status.code(), Some(2));
    }

    let mermaid = dir.join("converted.mmd");
    let out = omaspec(&["convert", old_s, "-o", mermaid.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let written = fs::read_to_string(&mermaid).unwrap();
    assert!(written.starts_with("stateDiagram-v2\n"), "{written}");
    let out = omaspec(&["diff", old_s, mermaid.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    let out = omaspec(&["convert", new_s]);
    assert!(stdout(&out).starts_with("@startuml\n"), "{}", stdout(&out));
    let history = dir.join("history.puml");
    fs::write(
        &history,
        "@startuml\n[*] --> On\nstate On {\n[*] --> A\n}\nOn --> Off : pause\nOff --> On[H] : resume\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&["convert", history.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("history"));
    assert!(out.stdout.is_empty());
    let out = omaspec(&["convert", old_s, "-o", old_s]);
    assert_eq!(out.status.code(), Some(2));
    let _ = fs::remove_dir_all(&dir);
}

/// A reader that stops early (`| head`) ends the command quietly: no
/// message, and success. A broken pipe to anything else stays an error.
#[test]
fn a_reader_that_stops_early_is_no_error() {
    let elevator =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/gallery/elevator.puml");
    let mut child = Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .args(["render", elevator.to_str().unwrap(), "-f", "png", "-o", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("omaspec runs");
    // The PNG is more than a pipe holds, and nothing reads it.
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{:?}", out.status);
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn new_makes_a_project_whose_diagrams_check_against_all_of_it() {
    let dir = scratch("new");
    let shop = dir.join("shop");
    let out = omaspec(&["new", shop.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        stdout(&out),
        format!(
            "created {0}/shop.puml, the first state machine\ncreated {0}/omaspec-project.toml; beside it stands the project's map, made from its diagrams\n",
            shop.display()
        )
    );
    let toml = fs::read_to_string(shop.join("omaspec-project.toml")).unwrap();
    assert!(
        toml.ends_with("\nname = \"shop\"\n") && !toml.contains("main"),
        "{toml}"
    );
    // No map is written: it is made from the project's diagrams.
    assert!(!shop.join("system.puml").exists());

    // The first diagram is clean in both languages and drawn by Omaspec.
    let out = omaspec(&[
        "new",
        "--lang",
        "mermaid",
        "--json",
        dir.join("lager").to_str().unwrap(),
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["name"], "lager");
    assert_eq!(json["main"], serde_json::Value::Null);
    assert_eq!(json["git"], serde_json::Value::Null);
    assert_eq!(json["machine"], "lager.mmd");
    for main in [shop.join("shop.puml"), dir.join("lager/lager.mmd")] {
        let main = main.to_str().unwrap();
        assert_eq!(
            omaspec(&["lint", "--strict", main]).status.code(),
            Some(0),
            "{main}"
        );
        assert_eq!(
            omaspec(&["fmt", "--check", main]).status.code(),
            Some(0),
            "{main}"
        );
        let out = omaspec(&["inspect", main]);
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json["native"], "understood", "{main}");
    }

    // The map shows its project's diagrams: the first machine, and a new
    // diagram as soon as its file is there.
    let svg = dir.join("map.svg");
    let render = |svg: &Path| {
        let out = omaspec(&[
            "render",
            shop.join("omaspec-project.toml").to_str().unwrap(),
            "-o",
            svg.to_str().unwrap(),
        ]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        fs::read_to_string(svg).unwrap()
    };
    assert_eq!(render(&svg).matches("<svg").count(), 2);
    fs::write(
        shop.join("orders.puml"),
        "@startuml\ntitle Orders\n[*] --> Open\nOpen --> Shop : buy\n@enduml\n",
    )
    .unwrap();
    let drawn = render(&svg);
    assert_eq!(drawn.matches("<svg").count(), 3, "{drawn}");
    assert!(drawn.contains(">opens<"), "{drawn}");

    // A scenario deep in the project is held against the machine at its
    // root, as a scenario beside it would be.
    fs::create_dir_all(shop.join("scenarios/night")).unwrap();
    let scenario = shop.join("scenarios/night/late.puml");
    fs::write(
        &scenario,
        "@startuml\nparticipant Shop\nUser -> Shop : start\nUser -> Shop : start\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&["lint", "--strict", scenario.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("late.puml:4: warning: Shop does not take start here")
            && stdout(&out).contains("(shop.puml)"),
        "{}",
        stdout(&out)
    );
    // Without the project file the scenario's own folder is all there is.
    let toml = shop.join("omaspec-project.toml");
    let kept = fs::read(&toml).unwrap();
    fs::remove_file(&toml).unwrap();
    let out = omaspec(&["lint", "--strict", scenario.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    fs::write(&toml, kept).unwrap();

    // Twice is refused.
    let out = omaspec(&["new", shop.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("is a project already"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn coverage_counts_what_traces_take_and_draws_it_over_the_machine() {
    let dir = scratch("coverage");
    let door = "@startuml\ntitle Door\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n";
    // The same machine in Mermaid, under a front matter: its lines move.
    let gate = "---\ntitle: Gate\n---\nstateDiagram-v2\n  [*] --> Closed\n  Closed --> Open : open\n  Open --> Closed : close\n  Closed --> Locked : lock\n  Locked --> Closed : unlock\n";
    fs::write(dir.join("door.puml"), door).unwrap();
    fs::write(dir.join("gate.mmd"), gate).unwrap();
    fs::write(
        dir.join("night.jsonl"),
        "{\"object\":\"Door\",\"event\":\"open\"}\n{\"object\":\"Door\",\"event\":\"close\"}\n{\"object\":\"Gate\",\"event\":\"open\"}\n",
    )
    .unwrap();
    let door_path = dir.join("door.puml");
    let night = dir.join("night.jsonl");
    let out = omaspec(&[
        "coverage",
        door_path.to_str().unwrap(),
        "--traces",
        night.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(
        text.contains("3 of 5 transitions taken (60 %), 2 of 3 states, by 1 object"),
        "{text}"
    );
    assert!(
        text.contains("door.puml:6: never taken: Closed --> Locked : lock"),
        "{text}"
    );
    assert!(
        text.contains("door.puml:7: never taken: Locked --> Closed : unlock"),
        "{text}"
    );

    // Below what --min asks for: it fails, and says so.
    let out = omaspec(&[
        "coverage",
        door_path.to_str().unwrap(),
        "--traces",
        night.to_str().unwrap(),
        "--min",
        "80",
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("below the 80 % asked for"));

    // The gate is its own object: its start and its open, no close.
    for (file, svg, taken) in [
        ("door.puml", "door-cov.svg", 3),
        ("gate.mmd", "gate-cov.svg", 2),
    ] {
        let path = dir.join(file);
        let image = dir.join(svg);
        let out = omaspec(&[
            "coverage",
            path.to_str().unwrap(),
            "--traces",
            night.to_str().unwrap(),
            "--json",
            "-o",
            image.to_str().unwrap(),
        ]);
        assert_eq!(out.status.code(), Some(0), "{file}: {out:?}");
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json["covered_transitions"], taken, "{file}");
        assert_eq!(json["total_transitions"], 5, "{file}");
        assert_eq!(json["covered_states"], 2, "{file}");
        let lock = json["transitions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["trigger"] == "lock")
            .unwrap();
        assert_eq!(lock["hits"], 0, "{file}");
        assert_eq!(json["image"], image.to_str().unwrap());
        let drawn = fs::read_to_string(&image).unwrap();
        assert!(
            drawn.contains("open ×1") && drawn.contains("#2F9E44"),
            "{file}: {drawn}"
        );
        assert!(drawn.contains("#E8590C"), "{file}: never taken drawn apart");
        assert!(
            drawn.contains(&format!("{taken} of 5 transitions taken")),
            "{file}"
        );
    }
    // The source stays as it was.
    assert_eq!(fs::read_to_string(&door_path).unwrap(), door);

    // Not a state machine: said, and no picture.
    let scenario = dir.join("talk.puml");
    fs::write(&scenario, "@startuml\nA -> B : hi\n@enduml\n").unwrap();
    let out = omaspec(&[
        "coverage",
        scenario.to_str().unwrap(),
        "--traces",
        night.to_str().unwrap(),
    ]);
    assert_ne!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no state machine"));
}

/// `diff --markdown`: the changes as a comment for a pull request, each
/// changed machine drawn as Mermaid below them; for files and folders
/// alike, and nothing but a line where nothing changed (docs/ci/).
#[test]
fn diff_writes_a_pull_request_s_comment() {
    let dir = scratch("diff-markdown");
    let (old, new) = (dir.join("old"), dir.join("new"));
    fs::create_dir_all(&old).unwrap();
    fs::create_dir_all(&new).unwrap();
    fs::write(
        old.join("door.puml"),
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
    )
    .unwrap();
    fs::write(
        new.join("door.puml"),
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\n@enduml\n",
    )
    .unwrap();
    let files = [
        old.join("door.puml").to_str().unwrap().to_owned(),
        new.join("door.puml").to_str().unwrap().to_owned(),
    ];
    let out = omaspec(&["diff", &files[0], &files[1], "--markdown"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    let text = stdout(&out);
    assert!(text.starts_with("### Behavior changes from "), "{text}");
    assert!(text.contains("```mermaid\nstateDiagram-v2"), "{text}");
    assert!(text.contains("class Locked omaspecCame"), "{text}");
    let out = omaspec(&[
        "diff",
        old.to_str().unwrap(),
        new.to_str().unwrap(),
        "--markdown",
    ]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    let text = stdout(&out);
    assert!(text.contains("#### `door.puml`"), "{text}");
    assert!(text.contains("class Locked omaspecCame"), "{text}");
    // Nothing changed: said so, and the exit says it too.
    let out = omaspec(&["diff", &files[0], &files[0], "--markdown"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("No diagram changed in its behavior."));
    // Markdown or JSON, not both.
    let out = omaspec(&["diff", &files[0], &files[1], "--markdown", "--json"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn diff_draws_a_state_machine_s_changes_over_its_new_version() {
    let dir = scratch("diff-picture");
    let old = dir.join("door-old.puml");
    let new = dir.join("door.puml");
    fs::write(&old, "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\nClosed --> Locked : lock\nLocked --> Closed : unlock\n@enduml\n").unwrap();
    let text = "@startuml\n[*] --> Closed\nClosed --> Open : open [allowed]\nOpen --> Closed : close\nClosed --> Locked : lock\nOpen --> Jammed : force\nJammed --> Closed : repair\n@enduml\n";
    fs::write(&new, text).unwrap();
    let image = dir.join("door-diff.svg");
    let out = omaspec(&[
        "diff",
        old.to_str().unwrap(),
        new.to_str().unwrap(),
        "-o",
        image.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(1), "the behavior changed");
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["image"], image.to_str().unwrap());
    assert!(json["changes"].as_array().unwrap().len() >= 4, "{json}");
    let svg = fs::read_to_string(&image).unwrap();
    for part in [
        "#2F9E44",
        "#C77700",
        "#E8590C",
        "line-through",
        "3 came · 1 changed · 1 went",
    ] {
        assert!(svg.contains(part), "{part}: {svg}");
    }
    assert_eq!(fs::read_to_string(&new).unwrap(), text, "the source stays");
    // A sequence diagram's changes are listed, not drawn.
    let a = dir.join("a.puml");
    let b = dir.join("b.puml");
    fs::write(&a, "@startuml\nA -> B : hi\n@enduml\n").unwrap();
    fs::write(&b, "@startuml\nA -> B : hello\n@enduml\n").unwrap();
    let out = omaspec(&[
        "diff",
        a.to_str().unwrap(),
        b.to_str().unwrap(),
        "-o",
        image.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("only what changed in a state machine"));
    assert!(
        stdout(&out).contains("b.puml:2"),
        "the changes are still listed: {}",
        stdout(&out)
    );
}

#[test]
fn matrix_tabulates_a_state_machine() {
    let dir = scratch("matrix");
    let file = dir.join("door.mmd");
    fs::write(&file, "stateDiagram-v2\n  [*] --> Closed\n  Closed --> Open : open\n  Open --> Closed : close\n  Closed --> Locked : lock\n").unwrap();
    let out = omaspec(&["matrix", file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let text = stdout(&out);
    assert!(
        text.starts_with("| state | open | close | lock |\n"),
        "{text}"
    );
    assert!(text.contains("| Closed | → Open |  | → Locked |"), "{text}");
    let out = omaspec(&["matrix", file.to_str().unwrap(), "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["states"],
        serde_json::json!(["Closed", "Open", "Locked"])
    );
    let cell = &json["cells"][0];
    assert_eq!(
        (cell["state"].as_str(), cell["event"].as_str()),
        (Some("Closed"), Some("open"))
    );
    assert_eq!(cell["kind"], "transition");
    assert_eq!(cell["transitions"][0]["line"], 3);
    let md = dir.join("door.md");
    let out = omaspec(&["matrix", file.to_str().unwrap(), "-o", md.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        fs::read_to_string(&md)
            .unwrap()
            .contains("| Open |  | → Closed |  |")
    );
    // A picture, in the look of the machine.
    let picture = dir.join("door.svg");
    let out = omaspec(&[
        "matrix",
        file.to_str().unwrap(),
        "-o",
        picture.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let svg = fs::read_to_string(&picture).unwrap();
    assert!(svg.contains(">Locked<") && svg.contains("→ Open"), "{svg}");
    let out = omaspec(&[
        "matrix",
        file.to_str().unwrap(),
        "-o",
        dir.join("door.gif").to_str().unwrap(),
    ]);
    assert_ne!(out.status.code(), Some(0));
}

/// `--version` tells the release, and past it the commit it was built
/// from when that is not the release's own: a build of main is told apart
/// from the release it follows.
#[test]
fn the_version_tells_the_commit_it_was_built_from() {
    let out = omaspec(&["--version"]);
    assert!(out.status.success());
    let said = String::from_utf8_lossy(&out.stdout);
    let told = said
        .trim()
        .strip_prefix("omaspec ")
        .expect("its name first");
    let release = env!("CARGO_PKG_VERSION");
    let rest = told.strip_prefix(release).expect("the release first");
    let commit = rest.strip_prefix('+').unwrap_or(rest);
    assert!(
        rest.is_empty() || (commit.len() >= 7 && commit.chars().all(|c| c.is_ascii_hexdigit())),
        "{told}"
    );
}

/// REQ-062: the help of `play` names what makes a GIF smaller, and asks
/// for values it can take.
#[test]
fn play_names_how_a_gif_gets_smaller() {
    let help = omaspec(&["play", "--help"]);
    let said = String::from_utf8_lossy(&help.stdout);
    assert!(
        said.contains("--colors") && said.contains("--lossy"),
        "{said}"
    );
    let wrong = omaspec(&["play", "x.puml", "-o", "x.gif", "--colors", "1"]);
    assert!(!wrong.status.success());
    let wrong = omaspec(&["play", "x.puml", "-o", "x.gif", "--lossy", "200"]);
    assert!(!wrong.status.success());
}

/// A presentation is one page that plays the diagram: a state machine by
/// its places, a scenario by its messages; nothing fetched (docs/adr/0031).
#[test]
fn render_writes_a_presentation_that_plays() {
    let dir = scratch("present");
    let machine = dir.join("door.puml");
    fs::write(
        &machine,
        "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n",
    )
    .unwrap();
    let page = dir.join("door.html");
    let out = omaspec(&[
        "render",
        machine.to_str().unwrap(),
        "-o",
        page.to_str().unwrap(),
        "--look",
        "dark",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = fs::read_to_string(&page).unwrap();
    assert!(text.starts_with("<!DOCTYPE html>"));
    assert!(text.contains("default-src 'none'"));
    assert!(
        text.contains("\"kind\":\"machine\""),
        "the play is the machine's"
    );
    assert!(text.contains("--paper: #16181D") || text.contains("--paper: #16181d"));
    let scenario = dir.join("talk.mmd");
    fs::write(
        &scenario,
        "sequenceDiagram\n  Alice->>Bob: hi\n  Bob-->>Alice: hello\n",
    )
    .unwrap();
    let page = dir.join("talk.html");
    let out = omaspec(&[
        "render",
        scenario.to_str().unwrap(),
        "-o",
        page.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    assert!(
        fs::read_to_string(&page)
            .unwrap()
            .contains("\"kind\":\"scenario\"")
    );
    let _ = fs::remove_dir_all(&dir);
}
