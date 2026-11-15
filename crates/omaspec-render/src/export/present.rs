//! A presentation: one HTML page that plays the diagram as the pane does
//! (docs/adr/0031). The picture as an export draws it, the play as data
//! worked out here, and a small script that only walks it, all in the
//! page, which fetches nothing.

use omaspec_core::Language;
use omaspec_model::play::Graph;
use omaspec_model::{Kind, Machine};
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

/// The page's script and its style, beside this file.
const SCRIPT: &str = include_str!("present.js");
const STYLE: &str = include_str!("present.css");

/// The page for `svg` (as an export writes it) with `play` over it and
/// `source` in it, called `title`.
pub fn page(svg: &str, play: Option<Play>, source: Option<Source>, title: &str) -> String {
    let data = match play {
        Some(play) => data(play),
        None => json!({ "kind": "picture" }),
    };
    let paper = play.map_or_else(|| "#ffffff".to_owned(), |p| p.paper.to_string());
    let ink = play.map_or_else(|| "#000000".to_owned(), |p| ink_on(p.paper).to_string());
    let accent = play.map_or_else(|| "#3584e4".to_owned(), |p| p.accent.to_string());
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
    if let Some(source) = source {
        out.push_str(&format!(
            "<script type=\"text/plain\" id=\"source\" data-language=\"{}\"{}>{}</script>\n",
            source.language.extension(),
            source
                .file
                .map(|file| format!(" data-file=\"{}\"", escape(file)))
                .unwrap_or_default(),
            in_script(source.code)
        ));
    }
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

/// A state machine: every state's box, and every place its start reaches
/// ([`Graph`]), with the states it is in and its options; `None` where the
/// picture does not show every state and transition, as the pane refuses
/// to play it.
fn machine_data(m: &Machine, map: &Map) -> Option<Value> {
    let placed = m.place(map);
    if placed.states.iter().any(Option::is_none) || placed.routes.iter().any(Option::is_none) {
        return None;
    }
    let graph = Graph::of(m)?;
    let states: Vec<Value> = placed
        .states
        .iter()
        .map(|spot| {
            let spot = spot.as_ref().expect("every state placed");
            let r = spot.rect;
            json!({
                "rect": [round(r.x), round(r.y), round(r.w), round(r.h)],
                "radius": round(spot.radius),
            })
        })
        .collect();
    let route = |t: usize| -> Value {
        let points = placed.routes[t].as_ref().expect("every route placed");
        Value::Array(points.iter().copied().map(point).collect())
    };
    let places: Vec<Value> = graph
        .runs
        .iter()
        .zip(&graph.options)
        .map(|(run, ways)| {
            let lit: Vec<usize> = run
                .active()
                .iter()
                .copied()
                .filter(|&s| m.states[s].kind == Kind::State)
                .collect();
            let mut names: Vec<String> = lit.iter().map(|&s| m.shown_name(s)).collect();
            names.dedup();
            let options: Vec<Value> = ways
                .iter()
                .map(|way| {
                    let (label, to) = m.option(way.transition);
                    json!({
                        "label": label,
                        "to": to,
                        "next": way.next,
                        "routes": way.taken.iter().map(|t| route(t.transition)).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({
                "at": names.join(", "),
                "lit": lit,
                "ended": run.ended(m),
                "options": options,
            })
        })
        .collect();
    Some(json!({
        "kind": "machine",
        "states": states,
        "places": places,
        "begun": graph.begun.iter().map(|t| route(t.transition)).collect::<Vec<_>>(),
    }))
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

/// `text` inside a `<script>` element, which ends at the first `</`: that
/// is written `<\/`, which JSON and the source read the same.
fn in_script(text: &str) -> String {
    text.replace("</", "<\\/").replace("<!--", "<\\!--")
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
        serde_json::from_str(&body[open..close].replace("<\\/", "</")).unwrap()
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
        // The source is in it, its `</script>` unable to end the element.
        assert_eq!(page.matches("</script>").count(), 3);
        assert!(page.contains("hello <\\/script>"));
        let data = play_of(&page);
        assert_eq!(data["kind"], "scenario");
        assert_eq!(data["steps"].as_array().unwrap().len(), 2);
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
        assert!(
            places[0]["options"][0]["routes"][0]
                .as_array()
                .unwrap()
                .len()
                >= 2
        );
        assert_eq!(data["begun"].as_array().unwrap().len(), 1);
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
  {{at: "Closed", lit: [1], options: [{{label: "open", to: "Open", next: 1, routes: [[[0, 0], [1, 1]]]}}, {{label: "far", to: "X", next: null, routes: []}}]}},
  {{at: "Open", lit: [2], options: [{{label: "close", to: "Closed", next: 0, routes: []}}]}},
];
const m = P.machine(places);
assert.strictEqual(m.take(1), null); assert.strictEqual(m.take(5), null); assert.strictEqual(m.place, 0);
assert.deepStrictEqual(m.take(0), [[[0, 0], [1, 1]]]); assert.strictEqual(m.here().at, "Open");
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
