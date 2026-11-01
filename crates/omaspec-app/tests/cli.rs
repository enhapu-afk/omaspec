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

    let out = omaspec(&["lint", "--json", broken.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json[0]["line"], 1);
    assert_eq!(json[0]["severity"], "error");
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
    assert_eq!(
        json["native"]["declined"]["reason"],
        "Git graphs are not drawn by Omaspec; mmdr draws them"
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
    fs::write(
        &file,
        "@startuml\nskinparam handwritten true\nA -> B\n@enduml\n",
    )
    .unwrap();
    let out = omaspec(&["inspect", file.to_str().unwrap()]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["native"]["declined"]["line"], 2);
    assert!(
        json["native"]["declined"]["reason"]
            .as_str()
            .unwrap()
            .contains("handwritten")
    );
}

fn omaspec_with(env: (&str, &str), args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_omaspec"))
        .env(env.0, env.1)
        .args(args)
        .output()
        .expect("omaspec runs")
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

    let file = dir.join("salt.puml");
    fs::write(&file, "@startsalt\n{+\n}\n@endsalt\n").unwrap();
    let path = file.to_str().unwrap();

    let out = omaspec_with(("OMASPEC_PLANTUML", "none"), &["render", path]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Salt UIs are not drawn"), "{stderr}");
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
    assert!(message.contains("Salt UIs are not drawn"), "{message}");
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
        fs::read_to_string(dir.join("salt.svg"))
            .unwrap()
            .contains("diagram 1")
    );
}

/// The examples are clean: no lint findings, and the Mermaid ones and
/// the PlantUML sequence, state, mind map and WBS diagrams draw without Java.
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
    // A salt diagram: the engine draws sequence, state, structure, timing and gantt itself.
    fs::write(
        &diagram,
        format!(
            "@startsalt\n' try {} {port} {}\n{{+\n}}\n@endsalt\n",
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
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
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
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 8);

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
            "created {0}/system.puml\ncreated {0}/omaspec-project.toml\n",
            shop.display()
        )
    );
    let toml = fs::read_to_string(shop.join("omaspec-project.toml")).unwrap();
    assert!(
        toml.contains("name = \"shop\"\nmain = \"system.puml\"\n"),
        "{toml}"
    );

    // The first diagram is clean in both languages and drawn by Omaspec.
    let out = omaspec(&[
        "new",
        "--lang",
        "mermaid",
        "--json",
        dir.join("lager").to_str().unwrap(),
    ]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        (json["name"].as_str(), json["main"].as_str()),
        (Some("lager"), Some("system.mmd"))
    );
    assert_eq!(json["git"], serde_json::Value::Null);
    for main in [shop.join("system.puml"), dir.join("lager/system.mmd")] {
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
            && stdout(&out).contains("(system.puml)"),
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
