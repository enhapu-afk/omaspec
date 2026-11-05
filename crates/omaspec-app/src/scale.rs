//! How a project of many diagrams fares (ROADMAP, *Step 0*): projects
//! generated in a scratch folder, scanned, read, linted and mapped as the
//! editor, `lint` and `render` do it. `OMASPEC_SCALE=1 cargo test
//! --release -p omaspec-app scale -- --nocapture` times projects of about
//! 100, 500 and 2,000 diagrams; without it a small one keeps the generator
//! honest.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use omaspec_core::lint::Severity;
use omaspec_core::{Language, detect};
use omaspec_render::{Config, Place, Rendered, Renderer};

use crate::editor::folder::scan;
use crate::specs::{self, Maps};

/// State machines in each area of a generated project.
const MACHINES: usize = 40;
/// Scenarios in each area; with the machines and the area's map, an area
/// holds 50 diagrams.
const SCENARIOS: usize = 9;

/// A generated project.
struct Project {
    root: PathBuf,
    /// Every diagram, the maps included.
    files: Vec<PathBuf>,
    /// The main map, which also shows every diagram on no map's box.
    main: PathBuf,
    /// The map of the first area, whose boxes name its diagrams.
    area: PathBuf,
}

/// A project of `areas` areas under `root`: each with machines (every
/// third in Mermaid), scenarios between them in PlantUML, and a map; the
/// main map names the areas.
fn generate(root: &Path, areas: usize) -> Project {
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root).unwrap();
    let write = |relative: &str, text: &str| {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    };
    write(
        omaspec_core::project::FILE,
        "name = \"scale\"\nmain = \"system.puml\"\n",
    );
    let mut files = Vec::new();
    let mut main = String::from("@startwbs\n* System\n");
    for area in 1..=areas {
        let dir = format!("area-{area:02}");
        let mut map = format!("@startwbs\n* Area {area:02}\n");
        let first = (area - 1) * MACHINES + 1;
        for unit in first..first + MACHINES {
            let mermaid = unit % 3 == 0;
            let ext = if mermaid { "mmd" } else { "puml" };
            files.push(write(
                &format!("{dir}/unit-{unit:04}.{ext}"),
                &machine(unit, mermaid),
            ));
            map.push_str(&format!("** Unit {unit:04}\n"));
        }
        for run in 0..SCENARIOS {
            let number = (area - 1) * SCENARIOS + run + 1;
            let unit = first + run * 4;
            files.push(write(
                &format!("{dir}/run-{number:04}.puml"),
                &scenario(number, unit, unit + 1),
            ));
            map.push_str(&format!("** Run {number:04}\n"));
        }
        map.push_str("@endwbs\n");
        files.push(write(&format!("{dir}/{dir}.puml"), &map));
        main.push_str(&format!("** Area {area:02}\n"));
    }
    main.push_str("@endwbs\n");
    let main = write("system.puml", &main);
    files.push(main.clone());
    Project {
        root: root.to_path_buf(),
        files,
        main,
        area: root.join("area-01/area-01.puml"),
    }
}

/// The state machine of unit `n`.
fn machine(n: usize, mermaid: bool) -> String {
    let body = "[*] --> Idle\nIdle --> Warming : power\nWarming --> Ready : warm\n\
                Ready --> Busy : start\nBusy --> Ready : done\nBusy --> Fault : fail\n\
                Fault --> Idle : reset\nReady --> Idle : sleep\n";
    if mermaid {
        format!("---\ntitle: Unit {n:04}\n---\nstateDiagram-v2\n{body}")
    } else {
        format!("@startuml\ntitle Unit {n:04}\n{body}@enduml\n")
    }
}

/// A scenario in which an operator starts unit `a`, and `a` unit `b`;
/// every message is one the machines take in that order.
fn scenario(n: usize, a: usize, b: usize) -> String {
    format!(
        "@startuml\ntitle Run {n:04}\nparticipant Operator\nparticipant Unit{a:04}\n\
         participant Unit{b:04}\nOperator -> Unit{a:04} : power\nOperator -> Unit{a:04} : warm\n\
         Unit{a:04} -> Unit{b:04} : power\nOperator -> Unit{a:04} : start\n\
         Unit{a:04} --> Operator : busy\nOperator -> Unit{a:04} : done\n@enduml\n"
    )
}

/// What one project measured.
struct Measured {
    diagrams: usize,
    scan: Duration,
    read: Duration,
    machines: usize,
    bytes: usize,
    lint: Duration,
    errors: usize,
    main: Drawn,
    area: Drawn,
}

/// A map drawn twice: with no picture at hand, then again.
struct Drawn {
    tiles: usize,
    cold: Duration,
    warm: Duration,
    svg: usize,
    parse: Duration,
    /// Its pixels for a pane of 1600 × 1000, fitted, as the overview.
    raster: Duration,
}

fn measure(project: &Project) -> Measured {
    let start = Instant::now();
    let found = scan(&project.root);
    let scan = start.elapsed();
    assert_eq!(
        found.len(),
        project.files.len(),
        "the scan finds every diagram"
    );

    // What the editor does when the folder opens, a file opens or the
    // window gets the focus.
    let start = Instant::now();
    let (machines, atlas) = specs::read_project(&project.root);
    let read = start.elapsed();
    let bytes = atlas.diagrams.iter().map(|d| d.source.len()).sum();

    // `omaspec lint` over every file, against the project's machines.
    let start = Instant::now();
    let mut errors = 0;
    for file in &project.files {
        let text = std::fs::read_to_string(file).unwrap();
        let language = detect(&text, Language::from_file_name(&file.to_string_lossy()));
        let findings = omaspec_model::findings_with(&text, language, &machines);
        for finding in findings.iter().filter(|f| f.severity == Severity::Error) {
            eprintln!(
                "{}:{}: {}",
                file.display(),
                finding.line + 1,
                finding.message
            );
            errors += 1;
        }
    }
    let lint = start.elapsed();

    Measured {
        diagrams: project.files.len(),
        scan,
        read,
        machines: machines.len(),
        bytes,
        lint,
        errors,
        main: draw_map(&project.main),
        area: draw_map(&project.area),
    }
}

/// Draws the map at `path` as `omaspec render` does, twice with one
/// renderer, and parses and rasterizes what the diagram pane shows first.
fn draw_map(path: &Path) -> Drawn {
    let text = std::fs::read_to_string(path).unwrap();
    let place = Place::of(path, None);
    let mut maps = Maps::default();
    // The project is read here, so the drawing is timed on its own.
    assert!(
        maps.of(&text, Language::PlantUml, Some(path), place.as_ref())
            .is_some(),
        "{} is a map",
        path.display()
    );
    let mut renderer = Renderer::new(&Config {
        java_fallback: false,
        ..Config::default()
    });
    let mut render = || -> (Rendered, Duration) {
        let start = Instant::now();
        let drawn = maps
            .render(
                &mut renderer,
                &text,
                Language::PlantUml,
                Some(path),
                place.as_ref(),
            )
            .unwrap();
        (drawn, start.elapsed())
    };
    let (drawn, cold) = render();
    let (_, warm) = render();
    let (tiles, svg) = (drawn.links.len(), drawn.svg.len());
    // What the pane parses first: a large map's cards, its pictures once
    // they can be read.
    let start = Instant::now();
    let shown = crate::worker::Drawn::of(drawn).unwrap();
    let parse = start.elapsed();
    let size = shown.tree.size();
    let fit = (1600.0 / size.width()).min(1000.0 / size.height());
    let start = Instant::now();
    omaspec_render::export::rasterize(&shown.tree, fit).unwrap();
    Drawn {
        tiles,
        cold,
        warm,
        svg,
        parse,
        raster: start.elapsed(),
    }
}

fn scratch(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("omaspec-scale-{name}-{}", std::process::id()))
}

fn ms(d: Duration) -> String {
    format!("{:.0} ms", d.as_secs_f64() * 1000.0)
}

#[test]
fn a_generated_project_reads_lints_and_maps_cleanly() {
    let project = generate(&scratch("small"), 1);
    let measured = measure(&project);
    assert_eq!(measured.diagrams, 51);
    assert_eq!(measured.machines, MACHINES);
    assert_eq!(measured.errors, 0, "the generated diagrams hold together");
    // The area's map has a tile for each of its diagrams; the main map
    // shows the area, whose map shows them.
    assert_eq!(measured.area.tiles, MACHINES + SCENARIOS);
    assert_eq!(measured.main.tiles, 1);
    let _ = std::fs::remove_dir_all(&project.root);
}

#[test]
fn scale_timings() {
    if std::env::var_os("OMASPEC_SCALE").is_none() {
        return;
    }
    println!(
        "\n{:>9} {:>7} {:>8} {:>8} {:>8} | {:>17} {:>17} {:>8} {:>8} {:>8} | {:>17} {:>8} {:>8} {:>8}",
        "diagrams",
        "scan",
        "read",
        "text",
        "lint",
        "main map tiles",
        "cold / warm",
        "svg",
        "parse",
        "raster",
        "area map cold",
        "warm",
        "parse",
        "raster"
    );
    for areas in [2, 10, 40] {
        let project = generate(&scratch(&areas.to_string()), areas);
        let m = measure(&project);
        assert_eq!(m.errors, 0);
        println!(
            "{:>9} {:>7} {:>8} {:>6}kB {:>8} | {:>17} {:>8} / {:>6} {:>6}kB {:>8} {:>8} | {:>17} {:>8} {:>8} {:>8}",
            m.diagrams,
            ms(m.scan),
            ms(m.read),
            m.bytes / 1024,
            ms(m.lint),
            m.main.tiles,
            ms(m.main.cold),
            ms(m.main.warm),
            m.main.svg / 1024,
            ms(m.main.parse),
            ms(m.main.raster),
            ms(m.area.cold),
            ms(m.area.warm),
            ms(m.area.parse),
            ms(m.area.raster),
        );
        let _ = std::fs::remove_dir_all(&project.root);
    }
}
