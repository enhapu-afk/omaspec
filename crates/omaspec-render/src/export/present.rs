//! A presentation: one HTML page that plays the diagram as the pane does
//! (docs/adr/0031). The picture as an export draws it, the play as data
//! worked out here, and a small script that only walks it, all in the
//! page, which fetches nothing.

use omaspec_core::Language;
use omaspec_core::embed;
use omaspec_core::skin::{Colors, LIGHT};
use omaspec_model::play::Graph;
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

/// The paper and the accent of `colors`: a presentation's veil and light.
pub fn stage(colors: &Colors) -> (Color, Color) {
    let color = |hex: &str, or: Color| omaspec_uml::parse_color(hex).ok().flatten().unwrap_or(or);
    (
        color(colors.background, Color::WHITE),
        color(colors.accent, Color::BLACK),
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
    let ink = ink_on(paper);
    // The source as an SVG export keeps it, which `omaspec extract` reads.
    let svg = match source {
        Some(s) => embed::svg_with_source(svg, s.code, s.language, s.file),
        None => svg.to_owned(),
    };
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
    out.push_str(&svg);
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
    if !play.map.messages.is_empty() {
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

/// A scenario: each message where it is, and the boxes under them.
fn scenario_data(map: &Map) -> Value {
    let steps: Vec<Value> = map
        .messages
        .iter()
        .map(|step| {
            json!({
                "points": step.points.iter().copied().map(point).collect::<Vec<_>>(),
                "top": round(step.top),
                "bottom": round(step.bottom),
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
/// its options, each with what it takes in beats (the steps of one beat
/// travel at once, as in the pane); `None` where the picture does not show
/// every state and transition, as the pane refuses to play it.
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
                    json!({
                        "label": label,
                        "to": to,
                        "next": way.next,
                        "taken": beats(&way.taken),
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
        "begun": beats(&graph.begun),
    }))
}

/// The transitions of `taken` by their beats, in the order they come.
fn beats(taken: &[Taken]) -> Vec<Vec<usize>> {
    let mut out: Vec<(u32, Vec<usize>)> = Vec::new();
    for step in taken {
        match out.iter_mut().find(|(beat, _)| *beat == step.beat) {
            Some((_, transitions)) => transitions.push(step.transition),
            None => out.push((step.beat, vec![step.transition])),
        }
    }
    out.into_iter()
        .map(|(_, transitions)| transitions)
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
        let next = places[0]["options"][0]["next"].as_u64().unwrap() as usize;
        assert_eq!(places[next]["at"], "Open");
        assert_eq!(places[next]["options"][0]["next"], 0);
        // What an option takes, by beats of transitions, each transition's
        // route written once.
        let taken = places[0]["options"][0]["taken"][0][0].as_u64().unwrap() as usize;
        assert!(data["routes"][taken].as_array().unwrap().len() >= 2);
        assert_eq!(data["routes"].as_array().unwrap().len(), 3);
        assert_eq!(data["begun"], json!([[0]]));
    }

    /// Steps of one beat light at once: a fork's arms are one beat.
    #[test]
    fn the_arms_of_a_fork_are_one_beat() {
        let taken = |transition, beat| Taken {
            transition,
            left: Vec::new(),
            entered: Vec::new(),
            beat,
        };
        assert_eq!(
            beats(&[taken(0, 0), taken(1, 1), taken(2, 1), taken(3, 2)]),
            vec![vec![0], vec![1, 2], vec![3]]
        );
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
