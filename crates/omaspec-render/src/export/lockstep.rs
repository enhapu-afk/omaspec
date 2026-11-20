//! A lockstep as a page: a scenario played beside the state machines of
//! its participants, in vectors and sharp at any size (`omaspec play
//! SCENARIO --lockstep -o lockstep.html`). The page holds each picture and
//! when everything happens: when each message's light sets off and
//! arrives, when each state's light goes on and off, when a light runs
//! along a transition, when a machine flashes where it cannot follow, and
//! what each machine's bar says. The script only shows the moment it is
//! at: a veil over what is still to come, the lights, the bars.

use omaspec_uml::Color;
use serde_json::{Value, json};

use super::{ExportError, Source};

const SCRIPT: &str = include_str!("lockstep.js");
const STYLE: &str = include_str!("lockstep.css");

/// What a page of a lockstep plays. Times are in seconds from its start;
/// points and boxes in the units of their picture's `viewBox`.
#[derive(Debug, Clone)]
pub struct Lockstep {
    pub scenario: Scenario,
    pub machines: Vec<Machine>,
    /// When the play ends, and how long its end stays before it starts
    /// again.
    pub end: f32,
    pub hold: f32,
    pub colors: Colors,
}

#[derive(Debug, Clone)]
pub struct Scenario {
    pub svg: String,
    /// Its paper, which the veil over what is still to come takes.
    pub paper: Color,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone)]
pub struct Message {
    /// From the sender to the tip of its head.
    pub points: Vec<(f32, f32)>,
    /// The band of the diagram it takes up.
    pub top: f32,
    pub bottom: f32,
    /// When its light sets off and when it reaches the receiver.
    pub start: f32,
    pub arrive: f32,
    /// Its receiver's machine cannot take it.
    pub wrong: bool,
}

#[derive(Debug, Clone)]
pub struct Machine {
    /// The participant it is the machine of.
    pub name: String,
    pub svg: String,
    pub paper: Color,
    /// How much of its column it takes: its height as drawn, by the
    /// widest machine's width.
    pub share: f32,
    /// By state: its box, and how round its corners are; none where the
    /// picture does not say.
    pub states: Vec<Option<Glow>>,
    /// By state: when its light goes on (`true`) and off.
    pub lights: Vec<Vec<(f32, bool)>>,
    pub travels: Vec<Travel>,
    /// When it flashes, where it cannot follow.
    pub flashes: Vec<f32>,
    /// What its bar says from each moment on: before its first step, and
    /// after each.
    pub bars: Vec<Bar>,
}

#[derive(Debug, Clone, Copy)]
pub struct Glow {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub radius: f32,
}

/// A light along a transition.
#[derive(Debug, Clone)]
pub struct Travel {
    pub points: Vec<(f32, f32)>,
    pub start: f32,
    pub took: f32,
}

#[derive(Debug, Clone)]
pub struct Bar {
    pub from: f32,
    /// Where it is, by name; empty while that is open.
    pub at: String,
    /// The event it just took.
    pub said: String,
    /// What went wrong, or that nothing did.
    pub note: String,
    pub wrong: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Colors {
    /// Around the panes, and the panes'.
    pub ground: Color,
    pub pane: Color,
    pub ink: Color,
    pub muted: Color,
    pub accent: Color,
    pub light: Color,
    pub wrong: Color,
}

/// The page that plays `lockstep`, called `title`, with the scenario's
/// `source` in its picture. The pictures are sanitized; only the source is
/// written after. The scenario carries the faces the words of every
/// picture are set in, and each picture's ids are its own, as they stand
/// in one page.
pub fn page(
    lockstep: &Lockstep,
    source: Option<Source>,
    title: &str,
) -> Result<String, ExportError> {
    let clean = |svg: &str, prefix: &str| -> Result<String, ExportError> {
        let svg = crate::atlas::own_ids(svg, prefix);
        crate::sanitize::sanitize(&svg).map_err(|e| ExportError::Svg(e.to_string()))
    };
    let others: Vec<&str> = lockstep.machines.iter().map(|m| m.svg.as_str()).collect();
    let scenario = clean(
        &crate::fonts::embed_with(&lockstep.scenario.svg, &others),
        "s-",
    )?;
    let scenario = match source {
        Some(s) => omaspec_core::embed::svg_with_source(&scenario, s.code, s.language, s.file),
        None => scenario,
    };
    let c = lockstep.colors;
    let mut out = String::with_capacity(scenario.len() + SCRIPT.len() + 16384);
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
        "<style>\n:root {{ --ground: {}; --pane: {}; --ink: {}; --muted: {}; --accent: {}; --wrong: {}; }}\n{STYLE}</style>\n",
        c.ground, c.pane, c.ink, c.muted, c.accent, c.wrong
    ));
    out.push_str("</head>\n<body>\n<main id=\"lockstep\">\n");
    out.push_str("<section class=\"pane scenario\">\n<div class=\"picture\">");
    out.push_str(&scenario);
    out.push_str("</div>\n<div class=\"bar\"><span class=\"head\"></span></div>\n</section>\n");
    out.push_str("<div class=\"machines\">\n");
    for (index, machine) in lockstep.machines.iter().enumerate() {
        out.push_str(&format!(
            "<section class=\"pane machine\" data-machine=\"{index}\" style=\"flex-grow:{:.3}\">\n<div class=\"picture\">",
            machine.share.max(0.05)
        ));
        out.push_str(&clean(&machine.svg, &format!("m{index}-"))?);
        out.push_str(
            "</div>\n<div class=\"bar\"><span class=\"head\"></span> <span class=\"said\"></span><div class=\"note\"></div></div>\n</section>\n",
        );
    }
    out.push_str("</div>\n</main>\n");
    out.push_str(&format!(
        "<script type=\"application/json\" id=\"play\">{}</script>\n",
        data(lockstep).to_string().replace('<', "\\u003c")
    ));
    out.push_str(&format!("<script>\n{SCRIPT}</script>\n</body>\n</html>\n"));
    Ok(out)
}

fn data(l: &Lockstep) -> Value {
    let point = |&(x, y): &(f32, f32)| json!([round(x), round(y)]);
    let points = |points: &[(f32, f32)]| points.iter().map(point).collect::<Vec<_>>();
    json!({
        "kind": "lockstep",
        "end": round(l.end),
        "hold": round(l.hold),
        "accent": l.colors.accent.to_string(),
        "light": l.colors.light.to_string(),
        "wrong": l.colors.wrong.to_string(),
        "scenario": {
            "paper": l.scenario.paper.to_string(),
            "messages": l.scenario.messages.iter().map(|m| json!({
                "points": points(&m.points),
                "top": round(m.top),
                "bottom": round(m.bottom),
                "start": round(m.start),
                "arrive": round(m.arrive),
                "wrong": m.wrong,
            })).collect::<Vec<_>>(),
        },
        "machines": l.machines.iter().map(|m| json!({
            "name": m.name,
            "paper": m.paper.to_string(),
            "states": m.states.iter().map(|g| g.map(|g| json!([
                round(g.x), round(g.y), round(g.w), round(g.h), round(g.radius)
            ]))).collect::<Vec<_>>(),
            "lights": m.lights.iter().map(|events| events.iter()
                .map(|&(at, on)| json!([round(at), on]))
                .collect::<Vec<_>>()).collect::<Vec<_>>(),
            "travels": m.travels.iter().map(|t| json!({
                "points": points(&t.points),
                "start": round(t.start),
                "took": round(t.took),
            })).collect::<Vec<_>>(),
            "flashes": m.flashes.iter().map(|&f| round(f)).collect::<Vec<_>>(),
            "bars": m.bars.iter().map(|b| json!({
                "from": round(b.from),
                "at": b.at,
                "said": b.said,
                "note": b.note,
                "wrong": b.wrong,
            })).collect::<Vec<_>>(),
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

    fn svg() -> String {
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20"><rect width="40" height="20" fill="#fff"/><g id="lines"/><script>alert(1)</script></svg>"##.to_owned()
    }

    #[test]
    fn a_page_holds_its_pictures_and_when_everything_happens() {
        let lockstep = Lockstep {
            scenario: Scenario {
                svg: svg(),
                paper: Color::WHITE,
                messages: vec![Message {
                    points: vec![(0.0, 5.0), (30.0, 5.0)],
                    top: 0.0,
                    bottom: 10.0,
                    start: 0.5,
                    arrive: 1.0,
                    wrong: true,
                }],
            },
            machines: vec![Machine {
                name: "Door <1>".into(),
                svg: svg(),
                paper: Color::WHITE,
                share: 0.5,
                states: vec![Some(Glow {
                    x: 1.0,
                    y: 1.0,
                    w: 10.0,
                    h: 5.0,
                    radius: 3.0,
                })],
                lights: vec![vec![(0.0, true)]],
                travels: Vec::new(),
                flashes: vec![1.0],
                bars: vec![Bar {
                    from: 0.0,
                    at: "Closed".into(),
                    said: "</script>".into(),
                    note: String::new(),
                    wrong: false,
                }],
            }],
            end: 2.0,
            hold: 1.0,
            colors: Colors {
                ground: Color::BLACK,
                pane: Color::BLACK,
                ink: Color::WHITE,
                muted: Color::WHITE,
                accent: Color::WHITE,
                light: Color::WHITE,
                wrong: Color::WHITE,
            },
        };
        let page = page(&lockstep, None, "keys <1>").unwrap();
        assert!(page.contains("<title>keys &lt;1&gt;</title>"));
        // Each picture sanitized; the data where the script reads it, and
        // nothing in it that ends the script early.
        assert!(!page.contains("alert(1)"), "{page}");
        assert!(page.contains(r#"data-machine="0""#));
        // One page, so each picture's ids are its own.
        assert!(page.contains(r#"id="s-lines""#) && page.contains(r#"id="m0-lines""#));
        assert!(page.contains(r#""kind":"lockstep""#));
        assert!(page.contains(r#""wrong":true"#));
        assert!(!page.contains("\"</script>\""));
        assert!(page.contains("default-src 'none'"));
    }
}
