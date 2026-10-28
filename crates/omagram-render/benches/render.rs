//! How long the steps from source to image take.
//! `cargo bench -p omagram-render`

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use mermaid_rs_renderer::{RenderOptions, compute_layout, parse_mermaid_strict, render_svg};
use omagram_core::Language;
use omagram_render::export::{self, Format, Options};
use omagram_render::sanitize::sanitize;
use omagram_render::{Config, Renderer};

const SEQUENCE: &str = "sequenceDiagram
    participant U as User
    participant B as Bar
    participant H as Hyprland
    U->>B: click workspace 2
    B->>H: dispatch workspace 2
    H-->>B: workspace changed
    loop every second
        B->>B: redraw clock
    end
    alt battery low
        B->>U: notify
    else charging
        B->>U: show icon
    end
";

const CLASS: &str = "classDiagram
    class Theme {
        +String name
        +apply()
    }
    class Bar
    class Menu
    Theme <|-- TokyoNight
    Theme <|-- MatteBlack
    Bar --> Theme
    Menu --> Theme
";

const STATE: &str = "stateDiagram-v2
    [*] --> Locked
    Locked --> Unlocked : password
    Unlocked --> Locked : timeout
    Unlocked --> Sleeping : lid closed
    Sleeping --> Unlocked : lid opened
    Unlocked --> [*]
";

/// Nested states, concurrent regions, a note, and transitions between
/// containers, which are routed after the layout.
const NESTED_STATE: &str = "stateDiagram-v2
    [*] --> Standby
    Standby --> Booting : power on
    state Booting {
        [*] --> SelfTest
        SelfTest --> LoadConfig : passed
        SelfTest --> Fault : failed
        LoadConfig --> [*]
    }
    Booting --> Operational
    state Operational {
        [*] --> Idle
        Idle --> Measuring : start
        Measuring --> Idle : stop
        state Measuring {
            [*] --> Sampling
            Sampling --> Filtering : sample ready
            Filtering --> Sampling : next
            --
            [*] --> Logging
            Logging --> Flushing : buffer full
            Flushing --> Logging : flushed
        }
        Idle --> Calibrating : calibrate
        Calibrating --> Idle : done
        note right of Calibrating : takes about 30 s
    }
    Operational --> Standby : power off
    Filtering --> Fault : sensor lost
    Calibrating --> Fault : out of range
    Fault --> Standby : reset
    Fault --> [*]
";

const FLOWCHART: &str = include_str!("../../omagram-core/tests/fixtures/flowchart.mmd");

const JOURNEY: &str = include_str!("../../omagram-core/tests/fixtures/journey.mmd");

const PIE: &str = include_str!("../../omagram-core/tests/fixtures/pie.mmd");

const GANTT: &str = include_str!("../../omagram-core/tests/fixtures/gantt.mmd");

const MINDMAP: &str = include_str!("../../omagram-core/tests/fixtures/mindmap.mmd");

const ER: &str = include_str!("../../omagram-core/tests/fixtures/er.mmd");

const C4: &str = "C4Container
title Banking
Person(customer, \"Customer\", \"A customer of the bank\")
System_Boundary(bank, \"Internet Banking\") {
  Container(web, \"Web\", \"React\", \"Serves the UI\")
  ContainerDb(db, \"Database\", \"SQL\", \"Stores accounts\")
  ContainerQueue(q, \"Bus\", \"Kafka\", \"Events\")
  Rel(web, db, \"Reads\", \"JDBC\")
  Rel(web, q, \"Publishes\")
}
Rel(customer, web, \"Uses\")
";

const ARCHITECTURE: &str = "architecture-beta
    group api(cloud)[API]
    service db(database)[Database] in api
    service disk1(disk)[Storage] in api
    service disk2(disk)[Storage] in api
    service server(server)[Server] in api
    db:L -- R:server
    disk1:T -- B:server
    disk2:T -- B:db
";

const BLOCK: &str = "block-beta
columns 3
  db[(\"DB\")] api[\"API\"] web(\"Web\")
  block:side
    columns 1
    reader[\"Reader\"]
    writer[\"Writer\"]
  end
  queue([\"Queue\"])
  disk[(\"Disk\")]
  go<[\"flow\"]>(right)
  db --> api
  api --> web
  reader --> writer
  web --> queue
";

const TIMELINE: &str = "timeline
    title History of Social Media Platform
    section Early
    2002 : LinkedIn
    2004 : Facebook : Google
    section Later
    2006 : Twitter
         : Vine
";

/// Subgraphs three deep, most lines between them.
const PLATFORM: &str = include_str!("../../../examples/platform.mmd");

fn mermaid(c: &mut Criterion) {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    for (name, source) in [
        ("sequence", SEQUENCE),
        ("class", CLASS),
        ("state", STATE),
        ("state nested", NESTED_STATE),
        ("flowchart with subgraph", FLOWCHART),
        ("flowchart nested subgraphs", PLATFORM),
        ("journey", JOURNEY),
        ("timeline", TIMELINE),
        ("pie", PIE),
        ("gantt", GANTT),
        ("mindmap", MINDMAP),
        ("er", ER),
        ("c4", C4),
        ("architecture", ARCHITECTURE),
        ("block", BLOCK),
    ] {
        c.bench_function(&format!("mermaid {name}"), |b| {
            b.iter(|| {
                renderer
                    .render(black_box(source), Language::Mermaid)
                    .unwrap()
            })
        });
    }
    // The same source, through mmdr's layout instead of the native drawer.
    // Both sides sanitize, which is what the editor waits for.
    c.bench_function("mermaid journey mmdr", |b| {
        b.iter(|| {
            let parsed = parse_mermaid_strict(black_box(JOURNEY)).unwrap();
            let options = RenderOptions::default();
            let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
            let svg = render_svg(&layout, &options.theme, &options.layout);
            sanitize(&svg).unwrap()
        });
    });
    c.bench_function("mermaid pie mmdr", |b| {
        b.iter(|| {
            let parsed = parse_mermaid_strict(black_box(PIE)).unwrap();
            let options = RenderOptions::default();
            let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
            let svg = render_svg(&layout, &options.theme, &options.layout);
            sanitize(&svg).unwrap()
        });
    });
    c.bench_function("mermaid mindmap mmdr", |b| {
        b.iter(|| {
            let parsed = parse_mermaid_strict(black_box(MINDMAP)).unwrap();
            let options = RenderOptions::default();
            let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
            let svg = render_svg(&layout, &options.theme, &options.layout);
            sanitize(&svg).unwrap()
        });
    });
    c.bench_function("mermaid class mmdr", |b| {
        b.iter(|| {
            let parsed = parse_mermaid_strict(black_box(CLASS)).unwrap();
            let options = RenderOptions::default();
            let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
            let svg = render_svg(&layout, &options.theme, &options.layout);
            sanitize(&svg).unwrap()
        });
    });
    c.bench_function("mermaid gantt mmdr", |b| {
        b.iter(|| {
            let parsed = parse_mermaid_strict(black_box(GANTT)).unwrap();
            let options = RenderOptions::default();
            let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
            let svg = render_svg(&layout, &options.theme, &options.layout);
            sanitize(&svg).unwrap()
        });
    });
    c.bench_function("mermaid er mmdr", |b| {
        b.iter(|| {
            let parsed = parse_mermaid_strict(black_box(ER)).unwrap();
            let options = RenderOptions::default();
            let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
            let svg = render_svg(&layout, &options.theme, &options.layout);
            sanitize(&svg).unwrap()
        });
    });
    c.bench_function("mermaid c4 mmdr", |b| {
        b.iter(|| {
            let parsed = parse_mermaid_strict(black_box(C4)).unwrap();
            let options = RenderOptions::default();
            let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
            let svg = render_svg(&layout, &options.theme, &options.layout);
            sanitize(&svg).unwrap()
        });
    });
    c.bench_function("mermaid architecture mmdr", |b| {
        b.iter(|| {
            let parsed = parse_mermaid_strict(black_box(ARCHITECTURE)).unwrap();
            let options = RenderOptions::default();
            let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
            let svg = render_svg(&layout, &options.theme, &options.layout);
            sanitize(&svg).unwrap()
        });
    });
    c.bench_function("mermaid block mmdr", |b| {
        b.iter(|| {
            let parsed = parse_mermaid_strict(black_box(BLOCK)).unwrap();
            let options = RenderOptions::default();
            let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
            let svg = render_svg(&layout, &options.theme, &options.layout);
            sanitize(&svg).unwrap()
        });
    });
    c.bench_function("mermaid timeline mmdr", |b| {
        b.iter(|| {
            let parsed = parse_mermaid_strict(black_box(TIMELINE)).unwrap();
            let options = RenderOptions::default();
            let layout = compute_layout(&parsed.graph, &options.theme, &options.layout);
            let svg = render_svg(&layout, &options.theme, &options.layout);
            sanitize(&svg).unwrap()
        });
    });
}

const JSON: &str = r#"@startjson
title People
#highlight "lastName"
#highlight "address" / "city"
{
  "firstName": "John",
  "lastName": "Smith",
  "isAlive": true,
  "age": 27,
  "address": {
    "streetAddress": "21 2nd Street",
    "city": "New York",
    "state": "NY",
    "postalCode": "10021-3100"
  },
  "phoneNumbers": [
    {"type": "home", "number": "212 555-1234"},
    {"type": "office", "number": "646 555-4567"}
  ],
  "children": [],
  "spouse": null
}
@endjson
"#;

const FORMULA: &str = r#"@startlatex
f(t) = \int_{0}^{t} h(\tau)\,d\tau + \sum_{i=0}^{n} \frac{i^{2}}{\sqrt{x+1}}
@endlatex
"#;

/// Omagram's own PlantUML engine: the login example, a sequence diagram
/// with every construct it draws, a mind map, a WBS diagram, a JSON
/// diagram and a formula, sanitizing included.
fn plantuml(c: &mut Criterion) {
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    for (name, source) in [
        (
            "sequence login",
            include_str!("../../../examples/login.puml"),
        ),
        (
            "sequence every construct",
            include_str!("../../omagram-uml/tests/sequence/every-construct.puml"),
        ),
        (
            "state nested",
            include_str!("../../omagram-uml/tests/state/nested.puml"),
        ),
        (
            "mind map",
            include_str!("../../omagram-uml/tests/mindmap/mindmap.puml"),
        ),
        ("wbs", include_str!("../../omagram-uml/tests/wbs/wbs.puml")),
        ("json", JSON),
        ("formula", FORMULA),
    ] {
        c.bench_function(&format!("plantuml {name}"), |b| {
            b.iter(|| {
                renderer
                    .render(black_box(source), Language::PlantUml)
                    .unwrap()
            })
        });
    }
}

fn export(c: &mut Criterion) {
    let svg = Renderer::new(&Config::default())
        .render(SEQUENCE, Language::Mermaid)
        .unwrap()
        .svg;
    // Loads the system fonts once, outside the measurement.
    export::parse(&svg).unwrap();
    c.bench_function("sanitize sequence svg", |b| {
        b.iter(|| sanitize(black_box(&svg)).unwrap())
    });
    c.bench_function("export png 2x", |b| {
        b.iter(|| export::export(black_box(&svg), Format::Png, &Options::default()).unwrap())
    });
    c.bench_function("export pdf", |b| {
        b.iter(|| export::export(black_box(&svg), Format::Pdf, &Options::default()).unwrap())
    });
}

criterion_group!(benches, mermaid, plantuml, export);
criterion_main!(benches);
