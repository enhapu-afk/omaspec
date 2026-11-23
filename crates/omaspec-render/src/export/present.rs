//! A presentation: one HTML page that plays the diagram as the pane does
//! (docs/adr/0031). The picture as an export draws it, the play as data
//! worked out here, and a small script that only walks it, all in the
//! page, which fetches nothing.

use omaspec_core::Language;
use omaspec_core::embed;
use omaspec_core::skin::{Colors, LIGHT};
use omaspec_model::play::{self, Graph};
use omaspec_model::{Machine, Taken};
use omaspec_uml::Color;
use omaspec_uml::map::Map;
use serde_json::{Value, json};

use super::Source;

/// What the page plays and in which colors: the map of the picture and
/// the source it was drawn from (a state machine's play is worked out from
/// it), the look's paper (the veil) and accent (the light).
#[derive(Debug, Clone, Copy)]
pub struct Play<'a> {
    pub map: &'a Map,
    pub code: Option<(&'a str, Language)>,
    pub paper: Color,
    pub accent: Color,
}

/// The paper and the accent of `colors`: a presentation's veil and light;
/// Omaspec's light palette's for one that does not read.
pub fn stage(colors: &Colors) -> (Color, Color) {
    let color = |hex: &str| omaspec_uml::parse_color(hex).ok().flatten();
    let paper = color(colors.background).or_else(|| color(LIGHT.background));
    let accent = color(colors.accent).or_else(|| color(LIGHT.accent));
    (
        paper.unwrap_or(Color::WHITE),
        accent.unwrap_or(Color::BLACK),
    )
}

/// The page's script and its style, beside this file.
const SCRIPT: &str = include_str!("present.js");
const STYLE: &str = include_str!("present.css");

/// The page for `svg` (as an export writes it) with `play` over it and
/// `source` in it, called `title`. Without a play, in Omaspec's light
/// colors.
pub fn page(svg: &str, play: Option<Play>, source: Option<Source>, title: &str) -> String {
    let data = match play {
        Some(play) => data(play),
        None => json!({ "kind": "picture" }),
    };
    let (paper, accent) = play.map_or_else(|| stage(&LIGHT), |p| (p.paper, p.accent));
    // The source as an SVG export keeps it, which `omaspec extract` reads.
    let svg = match source {
        Some(s) => embed::svg_with_source(svg, s.code, s.language, s.file),
        None => svg.to_owned(),
    };
    written(&svg, &data, (paper, accent), title)
}

/// The page around the pictures `svg` (one, or several one after
/// another) that plays `data`, on `paper` with its light in `accent`.
pub(super) fn written(
    svg: &str,
    data: &Value,
    (paper, accent): (Color, Color),
    title: &str,
) -> String {
    let ink = ink_on(paper);
    let mut out = String::with_capacity(svg.len() + SCRIPT.len() + 4096);
    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    // Nothing from anywhere: what the page shows is in it.
    out.push_str(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:\">\n",
    );
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<meta name=\"generator\" content=\"Omaspec\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape(title)));
    out.push_str(&format!(
        "<style>\n:root {{ --paper: {paper}; --ink: {ink}; --accent: {accent}; }}\n{STYLE}</style>\n"
    ));
    out.push_str("</head>\n<body>\n<main id=\"diagram\">\n");
    out.push_str(svg);
    out.push_str("\n</main>\n<nav id=\"bar\" aria-live=\"polite\"></nav>\n");
    out.push_str(&format!(
        "<script type=\"application/json\" id=\"play\">{}</script>\n",
        in_script(&data.to_string())
    ));
    out.push_str(&format!("<script>\n{SCRIPT}</script>\n</body>\n</html>\n"));
    out
}

/// The play of the diagram: a state machine's places, or a scenario's
/// steps, or the picture alone.
fn data(play: Play) -> Value {
    if let Some((code, language)) = play.code
        && let Some(machine) = Machine::read(code, language)
        && let Some(data) = machine_data(&machine, play.map)
    {
        return data;
    }
    // A presentation lifts its veil down the page: a git graph whose time
    // runs down plays there, one whose time runs across or up shows as its
    // picture.
    if !play.map.messages.is_empty() && play.map.run == omaspec_uml::map::Run::Down {
        return scenario_data(play.map);
    }
    json!({ "kind": "picture" })
}

fn point(at: (f32, f32)) -> Value {
    json!([round(at.0), round(at.1)])
}

/// `value` to a hundredth, without the float's noise in the page.
fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

/// A scenario: each step where it is, a message's or a note's of its own
/// (REQ-106), with the boxes lit with it, and the boxes under them.
fn scenario_data(map: &Map) -> Value {
    let steps: Vec<Value> = map
        .steps()
        .iter()
        .map(|step| {
            json!({
                "points": step.points.iter().copied().map(point).collect::<Vec<_>>(),
                "also": step.also.iter().map(|&[x, y, w, h]| [round(x), round(y), round(w), round(h)]).collect::<Vec<_>>(),
                "top": round(step.top),
                "bottom": round(step.bottom),
                "took": play::travel_millis(&step.points),
            })
        })
        .collect();
    let grounds: Vec<Value> = map
        .grounds
        .iter()
        .map(|ground| {
            let r = ground.rect;
            json!({
                "rect": [round(r.x), round(r.y), round(r.w), round(r.h)],
                "fill": ground.fill.to_string(),
            })
        })
        .collect();
    json!({ "kind": "scenario", "steps": steps, "grounds": grounds })
}

/// A state machine: every state's box and every transition's route, and
/// every place its start reaches ([`Graph`]), with the states it is in and
/// its options, each with the steps it takes in time as the pane takes
/// them ([`play::timed`]); `None` where the picture does not show every
/// state and transition, as the pane refuses to play it.
fn machine_data(m: &Machine, map: &Map) -> Option<Value> {
    let placed = m.place(map);
    let states = placed
        .states
        .iter()
        .map(|spot| {
            let spot = spot.as_ref()?;
            let r = spot.rect;
            Some(json!({
                "rect": [round(r.x), round(r.y), round(r.w), round(r.h)],
                "radius": round(spot.radius),
            }))
        })
        .collect::<Option<Vec<Value>>>()?;
    let routes = placed
        .routes
        .iter()
        .map(|points| {
            Some(Value::Array(
                points.as_ref()?.iter().copied().map(point).collect(),
            ))
        })
        .collect::<Option<Vec<Value>>>()?;
    let graph = Graph::of(m)?;
    let places: Vec<Value> = graph
        .runs
        .iter()
        .zip(&graph.options)
        .map(|(run, ways)| {
            let options: Vec<Value> = ways
                .iter()
                .map(|way| {
                    let (label, to) = m.option(way.transition);
                    // Where its words are: read through the veil while
                    // it is offered, as in the pane.
                    let words: Vec<Value> = placed.words[way.transition]
                        .iter()
                        .map(|r| json!([round(r.x), round(r.y), round(r.w), round(r.h)]))
                        .collect();
                    json!({
                        "label": label,
                        "to": to,
                        "next": way.next,
                        "taken": steps(&way.taken, &placed.routes),
                        "words": words,
                    })
                })
                .collect();
            json!({
                "at": m.shown_at(run.active()),
                "lit": run.active(),
                "ended": run.ended(m),
                "options": options,
            })
        })
        .collect();
    Some(json!({
        "kind": "machine",
        "states": states,
        "routes": routes,
        "places": places,
        "begun": steps(&graph.begun, &placed.routes),
    }))
}

/// The steps of `taken` in time, as the pane takes them: each its
/// transition, when its light sets off and how long it travels, in
/// milliseconds, and the states it leaves (dark as it sets off) and enters
/// (lit as it arrives).
fn steps(taken: &[Taken], routes: &[Option<Vec<(f32, f32)>>]) -> Vec<Value> {
    let (timed, _) = play::timed(taken, |step| {
        routes[step.transition]
            .as_deref()
            .map_or(0, play::travel_millis)
    });
    timed
        .into_iter()
        .map(|play::Timed { index, at, took }| {
            let step = &taken[index];
            json!({
                "t": step.transition,
                "at": at,
                "took": took,
                "left": step.left,
                "entered": step.entered,
            })
        })
        .collect()
}

/// Black or white, whichever reads better on `paper`.
fn ink_on(paper: Color) -> Color {
    if Color::WHITE.contrast(paper) > Color::BLACK.contrast(paper) {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

/// `text` for HTML: `&`, `<`, `>` and `"` written as entities.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// JSON inside a `<script>` element, which a `</` or a `<!--` would end or
/// upset: every `<` is written `<`, which JSON reads the same (it
/// stands in strings alone).
fn in_script(json: &str) -> String {
    json.replace('<', "\\u003c")
}

#[cfg(test)]
mod tests {
    use super::*;
    use omaspec_core::Language;

    fn rendered(source: &str, language: Language) -> (String, Map) {
        let mut renderer = crate::Renderer::new(&crate::Config {
            java_fallback: false,
            ..crate::Config::default()
        });
        let drawn = renderer.render(source, language).unwrap();
        (drawn.svg, drawn.map)
    }

    fn play_of(page: &str) -> Value {
        let start = page
            .find("<script type=\"application/json\" id=\"play\">")
            .unwrap();
        let body = &page[start..];
        let open = body.find('>').unwrap() + 1;
        let close = body.find("</script>").unwrap();
        serde_json::from_str(&body[open..close]).unwrap()
    }

    fn colors() -> (Color, Color) {
        (Color::hex(0x1a1b26), Color::hex(0x7aa2f7))
    }

    /// A page fetches nothing: no address in it but the SVG's namespaces,
    /// and a policy that allows no request.
    #[test]
    fn a_page_holds_everything_and_fetches_nothing() {
        let source = "@startuml\nAlice -> Bob : hello </script>\nBob --> Alice : hi\n@enduml\n";
        let (svg, map) = rendered(source, Language::PlantUml);
        let (paper, accent) = colors();
        let page = page(
            &svg,
            Some(Play {
                map: &map,
                code: Some((source, Language::PlantUml)),
                paper,
                accent,
            }),
            Some(Source {
                code: source,
                language: Language::PlantUml,
                file: Some("talk.puml"),
                folds: &[],
            }),
            "A <talk>",
        );
        assert!(page.contains("default-src 'none'"));
        assert!(page.contains("<title>A &lt;talk&gt;</title>"));
        for found in page.match_indices("http") {
            let rest = &page[found.0..];
            assert!(
                rest.starts_with("http://www.w3.org/") || rest.starts_with("http-equiv"),
                "{}",
                &rest[..rest.len().min(60)]
            );
        }
        assert!(!page.contains("src=\"http"));
        // The source is in it as an SVG export keeps it, unchanged and
        // unable to end an element; `omaspec extract` reads it.
        assert_eq!(page.matches("</script>").count(), 2);
        let kept = embed::source_from_svg(&page).unwrap();
        assert_eq!(kept.code, source);
        assert_eq!(kept.file.as_deref(), Some("talk.puml"));
        let data = play_of(&page);
        assert_eq!(data["kind"], "scenario");
        assert_eq!(data["steps"].as_array().unwrap().len(), 2);
    }

    /// A note of its own is a step of the page's play too, its box lit
    /// with it and no arrow to run along (REQ-106).
    #[test]
    fn a_note_is_a_step_of_the_page() {
        let source = "@startuml\nA -> B : one\nnote over A, B: between\nB -> A : two\n@enduml\n";
        let (_, map) = rendered(source, Language::PlantUml);
        let data = scenario_data(&map);
        let steps = data["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 3);
        assert!(steps[1]["points"].as_array().unwrap().is_empty());
        assert_eq!(steps[1]["also"].as_array().unwrap().len(), 1);
    }

    /// The play's data stays JSON whatever its names say: a `<!--` or a
    /// `</script>` in a guard neither ends the element nor breaks it.
    #[test]
    fn the_play_s_data_reads_whatever_its_names_say() {
        let source =
            "@startuml\n[*] --> Closed\nClosed --> Open : open [a <!-- b </script>]\n@enduml\n";
        let (svg, map) = rendered(source, Language::PlantUml);
        let (paper, accent) = colors();
        let play = Play {
            map: &map,
            code: Some((source, Language::PlantUml)),
            paper,
            accent,
        };
        let page = page(&svg, Some(play), None, "door");
        assert!(!page.contains("<!-- b"));
        let data = play_of(&page);
        assert_eq!(
            data["places"][0]["options"][0]["label"],
            "open [a <!-- b </script>]"
        );
    }

    /// Where the machine waits at a choice, the page names it and lights
    /// it, as the play bar does.
    #[test]
    fn a_choice_the_machine_waits_at_is_named_and_lit() {
        let source = "@startuml\nstate c <<choice>>\n[*] --> A\nA --> c : go\nc --> B : [yes]\nc --> A : [no]\n@enduml\n";
        let (svg, map) = rendered(source, Language::PlantUml);
        let (paper, accent) = colors();
        let play = Play {
            map: &map,
            code: Some((source, Language::PlantUml)),
            paper,
            accent,
        };
        let data = play_of(&page(&svg, Some(play), None, "choice"));
        assert_eq!(data["kind"], "machine");
        let places = data["places"].as_array().unwrap();
        let next = places[0]["options"][0]["next"].as_u64().unwrap() as usize;
        assert_eq!(places[next]["at"], "c");
        assert_eq!(places[next]["lit"].as_array().unwrap().len(), 1);
    }

    /// A state machine's page holds every place its start reaches, with
    /// the states lit there and the options as the play bar says them.
    #[test]
    fn a_machine_s_page_holds_its_places_and_options() {
        let source =
            "@startuml\n[*] --> Closed\nClosed --> Open : open\nOpen --> Closed : close\n@enduml\n";
        let (svg, map) = rendered(source, Language::PlantUml);
        let (paper, accent) = colors();
        let page = page(
            &svg,
            Some(Play {
                map: &map,
                code: Some((source, Language::PlantUml)),
                paper,
                accent,
            }),
            Some(Source {
                code: source,
                language: Language::PlantUml,
                file: None,
                folds: &[],
            }),
            "door",
        );
        let data = play_of(&page);
        assert_eq!(data["kind"], "machine");
        let places = data["places"].as_array().unwrap();
        assert_eq!(places.len(), 2);
        assert_eq!(places[0]["at"], "Closed");
        assert_eq!(places[0]["options"][0]["label"], "open");
        assert_eq!(places[0]["options"][0]["to"], "Open");
        // Where the option's word is, to read it through the veil.
        let words = places[0]["options"][0]["words"].as_array().unwrap();
        assert_eq!(words.len(), 1, "{words:?}");
        assert!(words[0][2].as_f64().unwrap() > 10.0, "{words:?}");
        let next = places[0]["options"][0]["next"].as_u64().unwrap() as usize;
        assert_eq!(places[next]["at"], "Open");
        assert_eq!(places[next]["options"][0]["next"], 0);
        // What an option takes, step by step in time, each transition's
        // route written once.
        let step = &places[0]["options"][0]["taken"][0];
        let taken = step["t"].as_u64().unwrap() as usize;
        assert!(data["routes"][taken].as_array().unwrap().len() >= 2);
        assert_eq!(data["routes"].as_array().unwrap().len(), 3);
        assert_eq!(step["at"], 0);
        assert!(step["took"].as_u64().unwrap() >= 380);
        assert_eq!(step["entered"].as_array().unwrap().len(), 1);
        assert_eq!(data["begun"][0]["t"], 0);
    }

    /// The page's script steps as the pane does, run where Node is: a
    /// scenario from before its first message to after its last and back,
    /// a machine by its options and back the way it came.
    #[test]
    fn the_script_steps_as_the_pane_does() {
        let script =
            std::env::temp_dir().join(format!("omaspec-present-{}.js", std::process::id()));
        std::fs::write(&script, SCRIPT).unwrap();
        let check = format!(
            r#"
const P = require({script:?});
const assert = require("assert");
const s = P.scenario([{{top: 10, bottom: 20, points: [[0, 15], [5, 15]]}}, {{top: 30, bottom: 40, points: [[0, 35], [5, 35]]}}]);
assert.strictEqual(s.edge(), 10); assert.strictEqual(s.lit(), null);
assert.ok(s.next()); assert.strictEqual(s.edge(), 20); assert.strictEqual(s.lit().top, 10);
assert.ok(s.next()); assert.ok(!s.next()); assert.strictEqual(s.edge(), 40);
assert.ok(s.back()); assert.strictEqual(s.at, 1); s.home(); assert.ok(!s.back()); s.go(9); assert.strictEqual(s.at, 2);
const places = [
  {{at: "Closed", lit: [1], options: [{{label: "open", to: "Open", next: 1, taken: [[1], [2, 3]]}}, {{label: "far", to: "X", next: null, taken: []}}]}},
  {{at: "Open", lit: [2], options: [{{label: "close", to: "Closed", next: 0, taken: []}}]}},
];
const m = P.machine(places);
assert.strictEqual(m.take(1), null); assert.strictEqual(m.take(5), null); assert.strictEqual(m.place, 0);
assert.deepStrictEqual(m.take(0), [[1], [2, 3]]); assert.strictEqual(m.here().at, "Open");
m.take(0); assert.strictEqual(m.place, 0); assert.ok(m.back()); assert.strictEqual(m.place, 1);
assert.ok(m.back()); assert.ok(!m.back()); m.go(1); m.home(); assert.strictEqual(m.place, 0);
// A glide: the first frame is the picture before, the last the new one.
const glide = {{
  boxes: [
    {{parts: ["a", "b"], from: [10, 10, 40, 20], to: [60, 30, 20, 20], role: "stay", into: null}},
    {{parts: ["c", "c"], from: [0, 0, 10, 10], to: [0, 0, 10, 10], role: "leave", into: null}},
    {{parts: ["d", "d"], from: [5, 5, 10, 10], to: [5, 5, 10, 10], role: "arrive", into: null}},
  ],
  lines: [
    {{from: {{points: [[0, 0], [10, 0]], heads: []}}, to: {{points: [[0, 10], [20, 10]], heads: []}}, colors: ["red", "blue"], width: 1, dash: null, fold: null, words: [[null, null, null], [null, null, null]]}},
    {{from: {{points: [[0, 0], [10, 0]], heads: []}}, to: null, colors: ["red", "red"], width: 1, dash: null, fold: null, words: [[null, null, null], [null, null, null]]}},
  ],
}};
const sizes = [[100, 50], [200, 50]];
const view = [400, 200];
const first = P.frame(glide, sizes, view, 0);
const last = P.frame(glide, sizes, view, 1);
const mid = P.frame(glide, sizes, view, 0.5);
// 100 by 50 fits 400 by 200 four times as large; 200 by 50 twice, in the middle.
assert.deepStrictEqual(first.paper, [0, 0, 400, 200]);
assert.deepStrictEqual(last.paper, [0, 50, 400, 100]);
assert.deepStrictEqual(first.boxes[0], {{rect: [40, 40, 160, 80], old: 1, now: 0, scale: 4}});
assert.deepStrictEqual(last.boxes[0], {{rect: [120, 110, 40, 40], old: 1, now: 1, scale: 2}});
assert.deepStrictEqual(first.grounds, [1, 0]); assert.deepStrictEqual(last.grounds, [0, 1]);
assert.deepStrictEqual(mid.grounds, [0, 0]);
assert.strictEqual(first.boxes[1].old, 1); assert.strictEqual(last.boxes[1].old, 0);
assert.strictEqual(first.boxes[2].now, 0); assert.strictEqual(last.boxes[2].now, 1);
assert.deepStrictEqual(first.lines[0].points, [[0, 0], [40, 0]]);
assert.deepStrictEqual(last.lines[0].points, [[0, 70], [40, 70]]);
assert.strictEqual(first.lines[0].color, "red"); assert.strictEqual(last.lines[0].color, "blue");
assert.strictEqual(first.lines[1].alpha, 1); assert.strictEqual(mid.lines[1].alpha, 0);
// Words a line keeps travel, the new over the old; others go with their picture.
const kept = P.words(glide.lines[0], [0, 0, 10, 5], [10, 0, 10, 5], sizes, view, 0.5);
assert.deepStrictEqual(kept.old.rect, kept.now.rect); assert.strictEqual(kept.old.alpha, 1); assert.strictEqual(kept.now.alpha, 0.5);
const lost = P.words(glide.lines[1], [0, 0, 10, 5], null, sizes, view, 0);
assert.strictEqual(lost.now, null); assert.strictEqual(lost.old.alpha, 1);
console.log("stepped");
"#,
            script = script.display().to_string()
        );
        let ran = std::process::Command::new("node")
            .arg("-e")
            .arg(&check)
            .output();
        let _ = std::fs::remove_file(&script);
        // Told only where Node is installed.
        let Ok(ran) = ran else { return };
        assert!(
            ran.status.success() && String::from_utf8_lossy(&ran.stdout).contains("stepped"),
            "{}",
            String::from_utf8_lossy(&ran.stderr)
        );
    }

    /// A look whose colors do not read plays in Omaspec's light ones, its
    /// light blue rather than black.
    #[test]
    fn a_stage_that_does_not_read_is_omaspec_s() {
        let colors = Colors {
            background: "paper",
            accent: "#12",
            ..LIGHT
        };
        assert_eq!(stage(&colors), stage(&LIGHT));
        assert_eq!(stage(&LIGHT).1, Color::hex(0x2F6FDB));
    }

    /// Without a play a page shows the picture alone.
    #[test]
    fn a_picture_without_a_play_is_a_page_too() {
        let source = "@startuml\nclass Order\n@enduml\n";
        let (svg, map) = rendered(source, Language::PlantUml);
        let (paper, accent) = colors();
        let page = page(
            &svg,
            Some(Play {
                map: &map,
                code: Some((source, Language::PlantUml)),
                paper,
                accent,
            }),
            None,
            "order",
        );
        assert_eq!(play_of(&page)["kind"], "picture");
    }
}
