//! Traffic as a page: many recorded traces flowing through a state machine
//! at once, in vectors and sharp at any size (`omaspec play order.puml
//! --traffic week.jsonl -o traffic.html`). The page holds the machine's
//! picture and when everything happens: the legs each dot runs and when,
//! when dots pass each transition, when a dot turns red. The script only
//! shows the moment it is at.

use omaspec_uml::Color;
use serde_json::{Value, json};

use super::{ExportError, Source};

const SCRIPT: &str = include_str!("traffic.js");

/// What a page of traffic plays. Times are in seconds from its start;
/// points in the units of the picture's `viewBox`.
#[derive(Debug, Clone)]
pub struct Traffic {
    pub svg: String,
    /// By transition: its route, and when dots set off along it and how
    /// long they take; one never taken blinks at the end.
    pub wires: Vec<Wire>,
    pub dots: Vec<Dot>,
    /// When the last dot has arrived; how long the ways never taken blink
    /// after, and how long the end stays.
    pub end: f32,
    pub blink: f32,
    pub hold: f32,
    /// What the caption says in the end, for a reader without the script.
    pub caption: String,
    pub objects: usize,
    pub machine: String,
    pub colors: Colors,
}

#[derive(Debug, Clone)]
pub struct Wire {
    pub points: Vec<(f32, f32)>,
    pub passes: Vec<(f32, f32)>,
}

#[derive(Debug, Clone)]
pub struct Dot {
    pub legs: Vec<Leg>,
    /// When it took a line its machine cannot take.
    pub wrong: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct Leg {
    pub start: f32,
    pub took: f32,
    pub points: Vec<(f32, f32)>,
}

#[derive(Debug, Clone, Copy)]
pub struct Colors {
    /// The picture's own paper, which the veil and the page take.
    pub paper: Color,
    pub ink: Color,
    pub muted: Color,
    pub accent: Color,
    pub light: Color,
    pub wrong: Color,
}

/// The page that plays `traffic`, called `title`, with the machine's
/// `source` in its picture. The picture is sanitized; only the source is
/// written after.
pub fn page(traffic: &Traffic, source: Option<Source>, title: &str) -> Result<String, ExportError> {
    let svg = crate::fonts::embed_with(&traffic.svg, &[]);
    let svg = crate::sanitize::sanitize(&svg).map_err(|e| ExportError::Svg(e.to_string()))?;
    let svg = match source {
        Some(s) => omaspec_core::embed::svg_with_source(&svg, s.code, s.language, s.file),
        None => svg,
    };
    let c = traffic.colors;
    let mut out = String::with_capacity(svg.len() + SCRIPT.len() + 16384);
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
        "<style>\nhtml, body {{ margin: 0; height: 100%; background: {}; color: {}; }}\n\
         #traffic {{ height: 100%; box-sizing: border-box; display: flex; flex-direction: column; padding: 14px; cursor: pointer; font: 14px/20px system-ui, sans-serif; }}\n\
         #picture {{ flex: 1; min-height: 0; display: flex; align-items: center; justify-content: center; }}\n\
         #picture > svg {{ width: 100%; height: 100%; }}\n\
         #caption {{ display: flex; justify-content: space-between; padding: 8px 12px 0; }}\n\
         #caption .wrong {{ color: {}; }}\n</style>\n",
        c.paper, c.ink, c.wrong
    ));
    out.push_str("</head>\n<body>\n<main id=\"traffic\">\n<div id=\"picture\">");
    out.push_str(&svg);
    out.push_str(&format!(
        "</div>\n<div id=\"caption\"><span class=\"said\">{}</span><span class=\"wrong\"></span></div>\n</main>\n",
        escape(&traffic.caption)
    ));
    out.push_str(&format!(
        "<script type=\"application/json\" id=\"play\">{}</script>\n",
        data(traffic).to_string().replace('<', "\\u003c")
    ));
    out.push_str(&format!("<script>\n{SCRIPT}</script>\n</body>\n</html>\n"));
    Ok(out)
}

fn data(t: &Traffic) -> Value {
    let points = |points: &[(f32, f32)]| {
        points
            .iter()
            .map(|&(x, y)| json!([round(x), round(y)]))
            .collect::<Vec<_>>()
    };
    let c = t.colors;
    json!({
        "kind": "traffic",
        "end": round(t.end),
        "blink": round(t.blink),
        "hold": round(t.hold),
        "objects": t.objects,
        "machine": t.machine,
        "colors": {
            "paper": c.paper.to_string(),
            "muted": c.muted.to_string(),
            "accent": c.accent.to_string(),
            "light": c.light.to_string(),
            "wrong": c.wrong.to_string(),
        },
        "wires": t.wires.iter().map(|w| json!({
            "points": points(&w.points),
            "passes": w.passes.iter().map(|&(s, took)| json!([round(s), round(took)])).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "dots": t.dots.iter().map(|d| json!({
            "legs": d.legs.iter().map(|l| json!({
                "start": round(l.start),
                "took": round(l.took),
                "points": points(&l.points),
            })).collect::<Vec<_>>(),
            "wrong": d.wrong.map(round),
        })).collect::<Vec<_>>(),
    })
}

fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}

/// `text` for HTML: `&`, `<`, `>` and `"` written as entities.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_holds_its_picture_and_where_every_dot_runs() {
        let traffic = Traffic {
            svg: r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><rect width="40" height="20" fill="#fff"/><script>alert(1)</script></svg>"##.to_owned(),
            wires: vec![Wire {
                points: vec![(0.0, 0.0), (10.0, 0.0)],
                passes: vec![(0.5, 0.4)],
            }],
            dots: vec![Dot {
                legs: vec![Leg {
                    start: 0.5,
                    took: 0.4,
                    points: vec![(0.0, 0.0), (10.0, 0.0)],
                }],
                wrong: Some(1.0),
            }],
            end: 1.0,
            blink: 2.0,
            hold: 1.0,
            caption: "2 objects of <Order>".into(),
            objects: 2,
            machine: "</script>".into(),
            colors: Colors {
                paper: Color::WHITE,
                ink: Color::BLACK,
                muted: Color::BLACK,
                accent: Color::BLACK,
                light: Color::WHITE,
                wrong: Color::BLACK,
            },
        };
        let page = page(&traffic, None, "week").unwrap();
        assert!(!page.contains("alert(1)"));
        assert!(page.contains("2 objects of &lt;Order&gt;"));
        assert!(page.contains(r#""kind":"traffic""#));
        assert!(!page.contains("\"</script>\""));
        assert!(page.contains(r#""wrong":1.0"#));
    }
}
