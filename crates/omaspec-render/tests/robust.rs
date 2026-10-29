//! Robustness of what reads foreign input: the SVG sanitizer against
//! mutated and booby-trapped SVGs (its output must stay free of scripts,
//! handlers and outside links), the SVG parser behind it, and mmdr with
//! broken diagrams (no hang, no crash reaching the caller). Diagrams nested
//! thousands deep, in every engine, must not overflow a small stack. From a
//! fixed seed; `OMASPEC_ROBUST_ROUNDS` and `OMASPEC_ROBUST_SEED` as in
//! omaspec-core's robustness test.

use std::path::Path;
use std::time::{Duration, Instant};

use omaspec_core::Language;
use omaspec_render::sanitize::sanitize;
use omaspec_render::{Config, Renderer, export};

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

fn seed(base: u64) -> Rng {
    let extra: u64 = std::env::var("OMASPEC_ROBUST_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    Rng(base ^ extra.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
}

fn rounds(default: usize) -> usize {
    std::env::var("OMASPEC_ROBUST_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn fixtures(extension: &str) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut texts: Vec<String> = [
        root.join("crates/omaspec-core/tests/fixtures"),
        root.join("examples"),
    ]
    .iter()
    .flat_map(|dir| std::fs::read_dir(dir).unwrap().flatten())
    .map(|e| e.path())
    .filter(|p| p.extension().is_some_and(|e| e == extension))
    .filter_map(|p| std::fs::read_to_string(p).ok())
    .collect();
    texts.sort();
    texts
}

fn mermaid_fixtures() -> Vec<String> {
    fixtures("mmd")
}

/// `code` with a few tokens put in and pieces cut out, on char boundaries.
fn mutate(rng: &mut Rng, mut code: String, tokens: &[&str]) -> String {
    for _ in 0..=rng.below(4) {
        let at = (0..=code.len())
            .filter(|i| code.is_char_boundary(*i))
            .nth(rng.below(code.len() + 1))
            .unwrap_or(0);
        if rng.below(3) == 0 {
            let end = (at + rng.below(20)).min(code.len());
            if code.is_char_boundary(end) {
                code.replace_range(at..end, "");
            }
        } else {
            code.insert_str(at, rng.pick(tokens));
        }
    }
    code
}

/// What an attacker would put into an SVG.
const TRAPS: &[&str] = &[
    "<script>alert(1)</script>",
    "<script xlink:href=\"evil.js\"/>",
    "<foreignObject><iframe src=\"https://evil\"/></foreignObject>",
    "<a href=\"javascript:alert(1)\"><text>x</text></a>",
    "<a xlink:href=\"file:///etc/passwd\"><text>x</text></a>",
    "<image href=\"file:///etc/passwd\" width=\"1\" height=\"1\"/>",
    "<image href=\"https://evil/x.png\" width=\"1\" height=\"1\"/>",
    "<use href=\"https://evil/sprite.svg#a\"/>",
    "<g onload=\"alert(1)\"><rect onclick=\"x()\" width=\"1\" height=\"1\"/></g>",
    "<style>@import url(https://evil/x.css); rect{fill:url(https://evil/p)}</style>",
    "<style>@\\69mport url(https://evil/x.css);</style>",
    "<style>@im/**/port url(https://evil/x.css);</style>",
    "<rect style=\"fill:url(https://evil/p)\" width=\"1\" height=\"1\"/>",
    "<rect style=\"fill:\\75rl(https://evil/p)\" width=\"1\" height=\"1\"/>",
    "<rect style=\"fill:image-set(url(https://evil/p))\" width=\"1\" height=\"1\"/>",
    "<animate attributeName=\"href\" to=\"javascript:alert(1)\"/>",
    "<set attributeName=\"onload\" to=\"alert(1)\"/>",
    "<!--<script>-->",
    "<?xml-stylesheet href=\"https://evil/x.css\"?>",
    "<!DOCTYPE svg [<!ENTITY x SYSTEM \"file:///etc/passwd\">]>",
    "&x;",
    "<iframe src=\"https://evil\"/>",
];

/// Fails if `css` (already lowercased) still carries an import or a
/// reference that leaves the document, including forms written with a
/// CSS escape.
fn assert_css_inert(css: &str, context: &dyn Fn() -> String) {
    assert!(!css.contains("@import"), "import kept {}", context());
    assert!(!css.contains("@\\"), "escaped at-rule kept {}", context());
    assert!(!css.contains("\\75rl"), "escaped url kept {}", context());
    assert!(
        !(css.contains("image-set(") && css.contains("://")),
        "image-set kept {}",
        context()
    );
    for name in ["url(", "src("] {
        for part in css.split(name).skip(1) {
            let target = part.trim_start_matches(['\'', '"', ' ', '\t']);
            assert!(target.starts_with('#'), "{name}{target} kept {}", context());
        }
    }
}

/// Fails if `svg` (a sanitizer's output) holds anything active or
/// anything reaching outside the document.
fn assert_inert(svg: &str, context: &dyn Fn() -> String) {
    let doc = roxmltree::Document::parse_with_options(
        svg,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            ..Default::default()
        },
    )
    .unwrap_or_else(|e| panic!("sanitized SVG does not parse: {e} {}", context()));
    for node in doc.descendants() {
        if node.is_comment() || node.is_pi() {
            panic!("comment or instruction kept {}", context());
        }
        if !node.is_element() {
            continue;
        }
        let name = node.tag_name().name().to_ascii_lowercase();
        assert!(
            !matches!(
                name.as_str(),
                "script" | "foreignobject" | "iframe" | "animate" | "set" | "animatetransform"
            ),
            "element {name} kept {}",
            context()
        );
        for attr in node.attributes() {
            let key = attr.name().to_ascii_lowercase();
            let value = attr.value().trim().to_ascii_lowercase();
            assert!(!key.starts_with("on"), "handler {key} kept {}", context());
            if key == "href" {
                let fine = value.starts_with('#')
                    || (name == "a" && (value.starts_with("http") || value.starts_with("mailto:")))
                    || (name == "image" && value.starts_with("data:image/"));
                assert!(fine, "link {value} on {name} kept {}", context());
            }
            if key == "style" {
                assert_css_inert(&value, context);
            }
        }
        if name == "style" {
            let css = node.text().unwrap_or_default().to_ascii_lowercase();
            assert_css_inert(&css, context);
        }
    }
}

#[test]
fn the_sanitizer_lets_nothing_active_through() {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let svgs: Vec<String> = mermaid_fixtures()
        .iter()
        .filter_map(|code| renderer.render(code, Language::Mermaid).ok())
        .map(|out| out.svg)
        .collect();
    assert!(svgs.len() >= 5);
    let mut rng = seed(0xdead_beef_cafe_f00d);
    for round in 0..rounds(400) {
        let mut svg = rng.pick(&svgs).clone();
        for _ in 0..=rng.below(5) {
            // At a tag boundary most of the time, anywhere sometimes.
            let spots: Vec<usize> = svg.match_indices('<').map(|(i, _)| i).collect();
            let at = if rng.below(4) == 0 {
                (0..=svg.len())
                    .filter(|i| svg.is_char_boundary(*i))
                    .nth(rng.below(svg.len() + 1))
                    .unwrap_or(0)
            } else {
                *rng.pick(&spots)
            };
            match rng.below(3) {
                0 => svg.insert_str(at, rng.pick(TRAPS)),
                1 => {
                    let end = (at + rng.below(40)).min(svg.len());
                    if svg.is_char_boundary(end) {
                        svg.replace_range(at..end, "");
                    }
                }
                _ => {
                    let attr = rng.pick(&[
                        " onmouseover=\"x()\"",
                        " href=\"javascript:x()\"",
                        " xlink:href=\"file:///etc\"",
                        " style=\"background:url(https://evil)\"",
                    ]);
                    if let Some(tag) = svg[at..].find(['>', ' ']).map(|i| at + i)
                        && svg[at..].starts_with('<')
                    {
                        svg.insert_str(tag, attr);
                    }
                }
            }
        }
        let context = || format!("round {round}");
        if let Ok(clean) = sanitize(&svg) {
            assert_inert(&clean, &context);
            // What the sanitizer lets through, the exporter can read.
            let _ = export::parse(&clean);
        }
    }
}

#[test]
fn mermaid_survives_broken_diagrams() {
    let fixtures = mermaid_fixtures();
    let tokens = [
        "-->",
        "---",
        "-.->",
        "==>",
        "|x|",
        "[",
        "]",
        "(",
        ")",
        "{",
        "}",
        "subgraph",
        "end",
        "participant",
        "Note right of A:",
        "loop",
        "alt",
        "else",
        "[*]",
        "state",
        "class",
        "%%{init: {'theme': 'dark'}}%%",
        "%%{init:",
        "\"",
        ":",
        ";",
        "\n",
        "\t",
        "é",
        "😀",
        "A",
        "B",
        "A --> A",
        "A --> B --> A",
    ];
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let mut rng = seed(0x0bad_cafe_0bad_cafe);
    for round in 0..rounds(150) {
        let start = rng.pick(&fixtures).clone();
        let code = mutate(&mut rng, start, &tokens);
        let started = Instant::now();
        // Errors are fine; a panic would surface here, mmdr's own are caught.
        if let Ok(out) = renderer.render(&code, Language::Mermaid) {
            assert!(sanitize(&out.svg).is_ok(), "round {round}");
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "round {round} took {:?}: {code:?}",
            started.elapsed()
        );
    }
}

#[test]
fn native_plantuml_survives_broken_diagrams() {
    let mut fixtures = fixtures("puml");
    for fixture in [
        include_str!("../../omaspec-uml/tests/sequence/every-construct.puml"),
        include_str!("../../omaspec-uml/tests/activity/every-step.puml"),
        include_str!("../../omaspec-uml/tests/activity/partitions.puml"),
        include_str!("../../omaspec-uml/tests/activity/backward.puml"),
        include_str!("../../omaspec-uml/tests/mindmap/mindmap.puml"),
        include_str!("../../omaspec-uml/tests/mindmap/mindmap-vertical.puml"),
        include_str!("../../omaspec-uml/tests/wbs/wbs.puml"),
        include_str!("../../omaspec-uml/tests/sequence/styled.puml"),
        include_str!("../../omaspec-uml/tests/activity/styled-steps.puml"),
        include_str!("../../omaspec-uml/tests/state/styled-states.puml"),
    ] {
        fixtures.push(fixture.to_owned());
    }
    let tokens = [
        "->",
        "-->",
        "->>",
        "<-",
        "<->",
        "-\\",
        "//--",
        "->x",
        "o->",
        "-[#red]>",
        "-[",
        "[->",
        "->]",
        "?->",
        "->?",
        "++",
        "--",
        "**",
        "!!",
        ":",
        "\"",
        "#",
        "<<",
        ">>",
        "participant ",
        "actor A as \"B\"",
        "note left of A",
        "note over A, B",
        "end note",
        "hnote over",
        "ref over A",
        "end ref",
        "alt",
        "else",
        "end",
        "loop",
        "group g [s]",
        "box \"B\"",
        "end box",
        "== x ==",
        "...",
        "... x ...",
        "|||",
        "||4||",
        "autonumber 3 \"[00]\"",
        "autonumber stop",
        "autonumber resume",
        "activate A",
        "deactivate",
        "return",
        "create",
        "destroy A",
        "title",
        "end title",
        "skinparam x {",
        "}",
        "hide footbox",
        "/'",
        "'/",
        "'",
        "\n",
        "\t",
        "é",
        "😀",
        "\u{feff}",
        "A",
        "B",
        "* ",
        "** ",
        "***",
        "**_ ",
        "**[#red]",
        "**:",
        ";",
        "+ ",
        "-- ",
        "***<",
        "***> ",
        "(c)",
        "  ",
        "left side",
        "right side",
        "top to bottom direction",
        "start",
        "stop",
        ":x;",
        ";",
        "if (a) then (yes)",
        "(no) elseif (b) then",
        "else (no)",
        "endif",
        "while (w) is (y)",
        "endwhile (n)",
        "repeat",
        "repeat while (r) is (y) not (n)",
        "fork",
        "fork again",
        "end fork",
        "end merge",
        "split",
        "split again",
        "end split",
        "|Lane|",
        "|#pink|Other|",
        "-> label;",
        "-[#red,dashed]->",
        "note right: n",
        "note left",
        "detach",
        "#pink:",
        "partition P {",
        "partition #aqua \"Q\" {",
        "group G",
        "end group",
        "backward :b;",
        "(in) backward :b; (out)",
        "@startmindmap",
        "@startwbs",
        "<style>",
        "</style>",
        "<style>arrow { LineColor red }</style>",
        "root {",
        "participant, actor {",
        "--x: #f00;",
        "var(--x)",
        "LineColor: blue;",
        "FontSize 0",
        "RoundCorner",
        ".s {",
        "depth(1)",
        " * ",
        "/*",
        "*/",
        "//",
        "@media",
    ];
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let mut rng = seed(0xfeed_f00d_feed_f00d);
    // Mostly diagrams the engine draws, so the mutations reach the layout.
    let drawable: Vec<String> = fixtures
        .iter()
        .filter(|f| omaspec_uml::render(f).is_ok())
        .cloned()
        .collect();
    assert!(drawable.len() >= 2);
    let rounds = rounds(1000);
    let mut drawn = 0;
    for round in 0..rounds {
        let pool = if rng.below(4) == 0 {
            &fixtures
        } else {
            &drawable
        };
        let start = rng.pick(pool).clone();
        let code = mutate(&mut rng, start, &tokens);
        let lines = code.split('\n').count();
        if let Err(declined) = omaspec_uml::render(&code) {
            assert!(
                declined.line.is_none_or(|l| l < lines),
                "round {round}: line {:?} of {lines}: {code:?}",
                declined.line
            );
        }
        if let Ok(out) = renderer.render(&code, Language::PlantUml) {
            // Unchanged by the sanitizer: the engine writes only what it keeps.
            assert_eq!(
                sanitize(&out.svg).ok().as_ref(),
                Some(&out.svg),
                "round {round}"
            );
            drawn += 1;
        }
    }
    assert!(drawn * 10 > rounds, "only {drawn} of {rounds} drawn");
}

/// Diagrams that nest `depth` deep, one construct each: blocks over lines,
/// brackets in one line, and indentation (whose file grows with the
/// square, so it nests a tenth as deep).
fn deep_diagrams(depth: usize) -> Vec<(&'static str, Language, String)> {
    let d = depth;
    let nest = |open: &str, inner: &str, close: &str| {
        format!("{}{inner}{}", open.repeat(d), close.repeat(d))
    };
    let uml = |body: String| format!("@startuml\n{body}\n@enduml\n");
    let indented = |mark: &str| -> String {
        (1..=d / 10)
            .map(|i| format!("{} n{i}\n", mark.repeat(i)))
            .collect()
    };
    let plantuml = [
        (
            "sequence groups",
            uml(nest("group g\n", "A -> B\n", "end\n")),
        ),
        (
            "sequence alts",
            uml(nest("alt a\nA -> B\nelse b\n", "A -> B\n", "end\n")),
        ),
        (
            "activations",
            uml(format!(
                "A -> B\n{}",
                nest("activate B\n", "", "deactivate B\n")
            )),
        ),
        (
            "ifs",
            uml(format!(
                "start\n{}stop",
                nest("if (c) then (y)\n", ":a;\n", "endif\n")
            )),
        ),
        (
            "whiles",
            uml(format!(
                "start\n{}stop",
                nest("while (c)\n", ":a;\n", "endwhile\n")
            )),
        ),
        (
            "repeats",
            uml(format!(
                "start\n{}stop",
                nest("repeat\n", ":a;\n", "repeat while (c)\n")
            )),
        ),
        (
            "forks",
            uml(format!(
                "start\n{}stop",
                nest("fork\n", ":a;\n", "end fork\n")
            )),
        ),
        (
            "partitions",
            uml(format!(
                "start\n{}stop",
                nest("partition p {\n", ":a;\n", "}\n")
            )),
        ),
        ("states", uml(nest("state S {\n", "[*] --> X\n", "}\n"))),
        ("packages", uml(nest("package p {\n", "class A\n", "}\n"))),
        ("nodes", uml(nest("node n {\n", "[c]\n", "}\n"))),
        ("rectangles", uml(nest("rectangle r {\n", "(u)\n", "}\n"))),
        (
            "generics",
            uml(format!("class A<{}>", nest("List<", "T", ">"))),
        ),
        (
            "mind map",
            format!("@startmindmap\n{}@endmindmap\n", indented("*")),
        ),
        ("wbs", format!("@startwbs\n{}@endwbs\n", indented("*"))),
        (
            "json arrays",
            format!("@startjson\n{}\n@endjson\n", nest("[", "", "]")),
        ),
        (
            "json objects",
            format!("@startjson\n{}\n@endjson\n", nest("{\"a\":", "1", "}")),
        ),
        (
            "tex groups",
            format!("@startlatex\n{}\n@endlatex\n", nest("{", "x", "}")),
        ),
        (
            "asciimath brackets",
            format!("@startmath\n{}\n@endmath\n", nest("(", "x", ")")),
        ),
        ("!if", uml(nest("!if 1\n", "A -> B\n", "!endif\n"))),
        (
            "brackets",
            uml(format!("!$x = {}\nA -> B : $x", nest("(", "1", ")"))),
        ),
        (
            "signs",
            uml(format!("!$x = {}1\nA -> B : $x", "-".repeat(d))),
        ),
        (
            "nots",
            uml(format!("!$x = {}1\nA -> B : $x", "!".repeat(d))),
        ),
        (
            "chain",
            uml(format!("!$x = 1{}\nA -> B : $x", " + 1".repeat(d))),
        ),
        (
            "calls",
            uml(format!(
                "!function $f($n)\n!return $n\n!endfunction\nA -> B : {}",
                nest("$f(", "1", ")")
            )),
        ),
        (
            "creole bold",
            uml(format!("A -> B : {}", nest("<b>", "x", "</b>"))),
        ),
        (
            "creole colors",
            uml(format!("A -> B : {}", nest("<color:red>", "x", "</color>"))),
        ),
        (
            "creole links",
            uml(format!("A -> B : {}", nest("[[", "x", "]]"))),
        ),
        (
            "styles",
            uml(format!(
                "<style>\n{}</style>\nA -> B",
                nest("x {\n", "", "}\n")
            )),
        ),
        (
            "skinparams",
            uml(format!("{}A -> B", nest("skinparam x {\n", "", "}\n"))),
        ),
        (
            "flow blocks",
            format!(
                "@startflow\n{}@endflow\n",
                nest("block B {\n", "in a\n", "}\n")
            ),
        ),
    ];
    let mermaid = [
        (
            "subgraphs",
            format!("flowchart TD\n{}", nest("subgraph s\n", "A-->B\n", "end\n")),
        ),
        (
            "flowchart shapes",
            format!("flowchart LR\nA{}", nest("[", "x", "]")),
        ),
        (
            "flowchart rounds",
            format!("flowchart LR\nA{}", nest("(", "x", ")")),
        ),
        (
            "mermaid states",
            format!(
                "stateDiagram-v2\n{}",
                nest("state S {\n", "[*] --> X\n", "}\n")
            ),
        ),
        (
            "loops",
            format!(
                "sequenceDiagram\n{}",
                nest("loop l\n", "A->>B: m\n", "end\n")
            ),
        ),
        (
            "rects",
            format!(
                "sequenceDiagram\n{}",
                nest("rect rgb(1,2,3)\n", "A->>B: m\n", "end\n")
            ),
        ),
        (
            "mermaid activations",
            format!("sequenceDiagram\n{}", "A->>+B: m\n".repeat(d)),
        ),
        (
            "blocks",
            format!("block-beta\n{}", nest("block:b\n", "a\n", "end\n")),
        ),
        (
            "boundaries",
            format!(
                "C4Context\n{}",
                nest("Boundary(b, \"x\") {\n", "Person(p, \"P\")\n", "}\n")
            ),
        ),
        (
            "groups",
            format!(
                "architecture-beta\ngroup g0(cloud)[G]\n{}service s(server)[S] in g{}\n",
                (1..d)
                    .map(|i| format!("group g{i}(cloud)[G] in g{}\n", i - 1))
                    .collect::<String>(),
                d - 1
            ),
        ),
        ("mermaid mind map", format!("mindmap\n{}", indented(" "))),
        ("kanban", format!("kanban\n{}", indented(" "))),
        (
            "init",
            format!(
                "%%{{init: {}}}%%\nflowchart LR\nA-->B\n",
                nest("{\"a\":", "1", "}")
            ),
        ),
    ];
    plantuml
        .into_iter()
        .map(|(name, code)| (name, Language::PlantUml, code))
        .chain(
            mermaid
                .into_iter()
                .map(|(name, code)| (name, Language::Mermaid, code)),
        )
        .collect()
}

/// A recursion without a limit overflows the stack, which ends the program
/// and cannot be caught. Each diagram runs on a thread as small as Rust's
/// default (the editor's are larger), named after it, so an overflow says
/// which one: rendered, checked, outlined, and read as a specification.
#[test]
fn deep_diagrams_end_without_overflowing_the_stack() {
    let cases = deep_diagrams(5_000);
    std::thread::scope(|scope| {
        let runs: Vec<_> = cases
            .iter()
            .map(|(name, language, code)| {
                let run = std::thread::Builder::new()
                    .name(format!("deep {name}"))
                    .stack_size(2 << 20)
                    .spawn_scoped(scope, move || {
                        let start = Instant::now();
                        let mut renderer = Renderer::new(&Config {
                            java_fallback: false,
                            ..Config::default()
                        });
                        let _ = renderer.render(code, *language);
                        let _ = omaspec_core::lint::lint(code, *language);
                        let _ = omaspec_core::outline::outline(code, *language);
                        let _ = omaspec_model::Spec::read(code, *language, "deep");
                        start.elapsed()
                    })
                    .expect("a thread starts");
                (name, run)
            })
            .collect();
        for (name, run) in runs {
            let took = run.join().unwrap_or_else(|_| panic!("{name} panicked"));
            eprintln!("{name}: {took:?}");
        }
    });
}
